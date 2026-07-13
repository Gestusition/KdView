//! Edge Module Registry — surfaces an open, compiled-in module catalog through
//! the sensing-server's HTTP API. The default path is fully offline; operators
//! may opt into a remote mirror with `--edge-registry-url`.
//!
//! Both built-in and remote sources use the same in-process TTL cache. Remote
//! mirrors retain stale-while-error semantics: if the mirror is unreachable
//! but a cached copy exists, return it with `stale: true` rather than 503.

use std::io::Read;
use std::sync::RwLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Stable source identifier for the open registry compiled into the binary.
pub const BUILTIN_REGISTRY_URL: &str = "builtin://wifi-densepose/open-edge-registry.json";

/// Default registry source. The historical constant name is retained for API
/// compatibility, but the value now identifies the built-in offline asset.
pub const DEFAULT_UPSTREAM_URL: &str = BUILTIN_REGISTRY_URL;

const BUILTIN_REGISTRY_BYTES: &[u8] = include_bytes!("../assets/open-edge-registry.json");

/// Default cache TTL for parsed registry responses and remote mirrors.
pub const DEFAULT_TTL_SECS: u64 = 3600;

/// Wire request timeout for an explicitly configured remote mirror.
pub const DEFAULT_FETCH_TIMEOUT_SECS: u64 = 10;

/// Response shape served by `GET /api/v1/edge/registry`. Documented in
/// ADR-102 §"Response shape".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryResponse {
    pub fetched_at: u64,
    pub ttl_seconds: u64,
    pub stale: bool,
    pub upstream_url: String,
    pub upstream_sha256: String,
    pub registry: Value,
}

/// Internal cache entry.
#[derive(Debug, Clone)]
struct CachedEntry {
    payload: Value,
    fetched_at_instant: Instant,
    fetched_at_unix: u64,
    upstream_sha256: String,
}

/// On-demand registry fetcher + cache. Cheap to construct; one instance is
/// shared across all incoming HTTP requests via `Arc<EdgeRegistry>`.
pub struct EdgeRegistry {
    cached: RwLock<Option<CachedEntry>>,
    ttl: Duration,
    upstream_url: String,
    fetcher: Box<dyn Fetcher>,
}

/// Pluggable fetcher abstraction — concrete impl is `UreqFetcher`; tests
/// can swap in `MockFetcher` to drive the cache logic without network.
pub trait Fetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetcherError>;
}

#[derive(Debug, thiserror::Error)]
pub enum FetcherError {
    #[error("network error: {0}")]
    Network(String),
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("response too large: {0} bytes")]
    TooLarge(usize),
}

/// Cap on remote response size to avoid pathological mirrors consuming
/// unbounded memory. 8 MiB leaves ample room for the technical catalog.
pub const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

/// Fetcher for the open registry included at compile time.
struct BuiltinFetcher;

impl Fetcher for BuiltinFetcher {
    fn fetch(&self, _url: &str) -> Result<Vec<u8>, FetcherError> {
        Ok(BUILTIN_REGISTRY_BYTES.to_vec())
    }
}

/// Live `ureq`-backed fetcher for operator-configured mirrors.
pub struct UreqFetcher {
    timeout: Duration,
}

impl UreqFetcher {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for UreqFetcher {
    fn default() -> Self {
        Self::new(Duration::from_secs(DEFAULT_FETCH_TIMEOUT_SECS))
    }
}

impl Fetcher for UreqFetcher {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetcherError> {
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let resp = agent.get(url).call().map_err(|e| match e {
            ureq::Error::Status(status, r) => FetcherError::Http {
                status,
                body: r.into_string().unwrap_or_default(),
            },
            ureq::Error::Transport(t) => FetcherError::Network(t.to_string()),
        })?;
        let mut reader = resp.into_reader().take((MAX_PAYLOAD_BYTES + 1) as u64);
        let mut buf = Vec::with_capacity(64 * 1024);
        reader
            .read_to_end(&mut buf)
            .map_err(|e| FetcherError::Network(e.to_string()))?;
        if buf.len() > MAX_PAYLOAD_BYTES {
            return Err(FetcherError::TooLarge(buf.len()));
        }
        Ok(buf)
    }
}

impl EdgeRegistry {
    pub fn new(upstream_url: impl Into<String>, ttl: Duration) -> Self {
        let upstream_url = upstream_url.into();
        let fetcher: Box<dyn Fetcher> = if upstream_url == BUILTIN_REGISTRY_URL {
            Box::new(BuiltinFetcher)
        } else {
            Box::new(UreqFetcher::default())
        };
        Self::with_fetcher(upstream_url, ttl, fetcher)
    }

    pub fn with_fetcher(
        upstream_url: impl Into<String>,
        ttl: Duration,
        fetcher: Box<dyn Fetcher>,
    ) -> Self {
        Self {
            cached: RwLock::new(None),
            ttl,
            upstream_url: upstream_url.into(),
            fetcher,
        }
    }

    /// Return a `RegistryResponse`. Uses the cache if fresh; otherwise
    /// re-fetches from upstream. On upstream failure with a non-empty
    /// cache, returns the stale copy.
    pub fn get(&self, force_refresh: bool) -> Result<RegistryResponse, FetcherError> {
        if !force_refresh {
            if let Some(entry) = self.fresh_cache_snapshot() {
                return Ok(self.response_from(&entry, false));
            }
        }

        // Either no cache, expired, or forced refresh — load the configured source.
        match self.fetch_and_cache() {
            Ok(entry) => Ok(self.response_from(&entry, false)),
            Err(e) => {
                // A remote source failed — serve stale if available.
                if let Some(entry) = self.any_cache_snapshot() {
                    Ok(self.response_from(&entry, true))
                } else {
                    Err(e)
                }
            }
        }
    }

    fn fresh_cache_snapshot(&self) -> Option<CachedEntry> {
        let guard = self.cached.read().ok()?;
        let entry = guard.as_ref()?;
        if entry.fetched_at_instant.elapsed() < self.ttl {
            Some(entry.clone())
        } else {
            None
        }
    }

    fn any_cache_snapshot(&self) -> Option<CachedEntry> {
        let guard = self.cached.read().ok()?;
        guard.clone()
    }

    fn fetch_and_cache(&self) -> Result<CachedEntry, FetcherError> {
        let bytes = self.fetcher.fetch(&self.upstream_url)?;
        let payload: Value = serde_json::from_slice(&bytes)
            .map_err(|e| FetcherError::Network(format!("invalid registry JSON: {e}")))?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let upstream_sha256 = hex_encode(&hasher.finalize());
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let entry = CachedEntry {
            payload,
            fetched_at_instant: Instant::now(),
            fetched_at_unix: now_unix,
            upstream_sha256,
        };
        if let Ok(mut guard) = self.cached.write() {
            *guard = Some(entry.clone());
        }
        Ok(entry)
    }

    fn response_from(&self, entry: &CachedEntry, stale: bool) -> RegistryResponse {
        RegistryResponse {
            fetched_at: entry.fetched_at_unix,
            ttl_seconds: self.ttl.as_secs(),
            stale,
            upstream_url: self.upstream_url.clone(),
            upstream_sha256: entry.upstream_sha256.clone(),
            registry: entry.payload.clone(),
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Mock fetcher backed by a queue of canned responses. Lets us drive
    /// the cache logic deterministically.
    struct MockFetcher {
        responses: std::sync::Mutex<Vec<Result<Vec<u8>, FetcherError>>>,
        call_count: AtomicUsize,
    }

    impl MockFetcher {
        fn new(responses: Vec<Result<Vec<u8>, FetcherError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: std::sync::Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            })
        }
    }

    impl Fetcher for Arc<MockFetcher> {
        fn fetch(&self, _url: &str) -> Result<Vec<u8>, FetcherError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut q = self.responses.lock().unwrap();
            if q.is_empty() {
                return Err(FetcherError::Network("mock: queue empty".into()));
            }
            q.remove(0)
        }
    }

    fn sample_payload() -> Vec<u8> {
        br#"{"version":"2.1.0","updated":"2026-05-13","cogs":[]}"#.to_vec()
    }

    #[test]
    fn builtin_default_is_available_offline_with_open_metadata() {
        let reg = EdgeRegistry::new(DEFAULT_UPSTREAM_URL, Duration::from_secs(3600));
        let resp = reg.get(false).expect("built-in registry");

        assert!(!resp.stale);
        assert_eq!(resp.upstream_url, BUILTIN_REGISTRY_URL);
        assert_eq!(resp.registry["version"], "1.0.0");
        let cogs = resp.registry["cogs"].as_array().expect("cogs array");
        assert_eq!(resp.registry["module_count"].as_u64(), Some(64));
        assert_eq!(cogs.len(), 64);
        assert_eq!(cogs[0]["id"], "gesture");
        assert_eq!(resp.upstream_sha256.len(), 64);

        let wire = serde_json::to_value(&resp).expect("wire response");
        for field in [
            "fetched_at",
            "ttl_seconds",
            "stale",
            "upstream_url",
            "upstream_sha256",
            "registry",
        ] {
            assert!(wire.get(field).is_some(), "missing response field {field}");
        }

        for module in cogs {
            let object = module.as_object().expect("module object");
            for omitted in ["author", "license", "rating", "downloads"] {
                assert!(!object.contains_key(omitted), "unexpected field {omitted}");
            }
        }
    }

    #[test]
    fn remote_first_call_fetches_then_uses_fresh_cache() {
        let fetcher = MockFetcher::new(vec![Ok(sample_payload())]);
        let reg = EdgeRegistry::with_fetcher(
            "http://test.invalid/registry.json",
            Duration::from_secs(3600),
            Box::new(fetcher.clone()),
        );
        let resp = reg.get(false).expect("get");
        assert!(!resp.stale);
        assert_eq!(resp.registry["version"], "2.1.0");
        assert_eq!(resp.upstream_url, "http://test.invalid/registry.json");
        assert_eq!(fetcher.call_count.load(Ordering::SeqCst), 1);
        // Second call within TTL — no new fetch.
        let _ = reg.get(false).expect("get");
        assert_eq!(fetcher.call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn remote_ttl_expiry_triggers_refetch() {
        let fetcher = MockFetcher::new(vec![Ok(sample_payload()), Ok(sample_payload())]);
        let reg = EdgeRegistry::with_fetcher(
            "http://test.invalid/registry.json",
            Duration::from_millis(10), // very short TTL
            Box::new(fetcher.clone()),
        );
        let _ = reg.get(false).expect("first");
        std::thread::sleep(Duration::from_millis(30));
        let _ = reg.get(false).expect("second after expiry");
        assert_eq!(fetcher.call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn remote_force_refresh_bypasses_fresh_cache() {
        let fetcher = MockFetcher::new(vec![Ok(sample_payload()), Ok(sample_payload())]);
        let reg = EdgeRegistry::with_fetcher(
            "http://test.invalid/registry.json",
            Duration::from_secs(3600),
            Box::new(fetcher.clone()),
        );
        let _ = reg.get(false).expect("first");
        let _ = reg.get(true).expect("refresh");
        assert_eq!(fetcher.call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn remote_failure_serves_stale_cached_success() {
        // First call succeeds and populates the cache. Second call hits upstream
        // failure but we still have a cached copy — should serve it with stale=true.
        let fetcher = MockFetcher::new(vec![
            Ok(sample_payload()),
            Err(FetcherError::Network("simulated".into())),
        ]);
        let reg = EdgeRegistry::with_fetcher(
            "http://test.invalid/registry.json",
            Duration::from_millis(1), // expire quickly so call 2 retries upstream
            Box::new(fetcher.clone()),
        );
        let first = reg.get(false).expect("first");
        assert!(!first.stale);
        std::thread::sleep(Duration::from_millis(5));
        let second = reg.get(false).expect("stale-serve");
        assert!(second.stale, "expected stale=true when upstream failed");
        assert_eq!(second.registry["version"], "2.1.0");
    }

    #[test]
    fn remote_failure_without_cache_returns_error() {
        let fetcher = MockFetcher::new(vec![Err(FetcherError::Network("down".into()))]);
        let reg = EdgeRegistry::with_fetcher(
            "http://test.invalid/registry.json",
            Duration::from_secs(3600),
            Box::new(fetcher),
        );
        let err = reg.get(false).expect_err("should be err");
        match err {
            FetcherError::Network(_) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn remote_invalid_json_is_treated_as_error() {
        let fetcher = MockFetcher::new(vec![Ok(b"not json".to_vec())]);
        let reg = EdgeRegistry::with_fetcher(
            "http://test.invalid/registry.json",
            Duration::from_secs(3600),
            Box::new(fetcher),
        );
        let err = reg.get(false).expect_err("invalid json");
        match err {
            FetcherError::Network(msg) => assert!(msg.contains("invalid registry JSON")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn remote_sha256_is_deterministic() {
        let fetcher = MockFetcher::new(vec![Ok(sample_payload())]);
        let reg = EdgeRegistry::with_fetcher(
            "http://test.invalid/registry.json",
            Duration::from_secs(3600),
            Box::new(fetcher),
        );
        let resp = reg.get(false).expect("get");
        // SHA-256 of br#"{"version":"2.1.0","updated":"2026-05-13","cogs":[]}"#
        let mut hasher = Sha256::new();
        hasher.update(sample_payload());
        let expected = hex_encode(&hasher.finalize());
        assert_eq!(resp.upstream_sha256, expected);
        assert_eq!(resp.upstream_sha256.len(), 64);
    }
}
