//! ADR-116 — RuView Home Assistant + Matter integration service.
//!
//! This crate is the edge-deployable wrapper around ADR-115's
//! `wifi-densepose-sensing-server::mqtt` publisher. It adds the
//! integration surfaces ADR-115's `--mqtt` flag can't easily reach:
//!
//! 1. **mDNS service advertisement** — `_ruview-ha._tcp` so HA discovers
//!    the cog automatically (no manual broker host/port config).
//! 2. **Optional embedded MQTT broker** — for nodes running without an
//!    external mosquitto. Defaults to off; the cog can either embed
//!    rumqttd or connect to a user-provided broker.
//! 3. **RuVector-backed semantic-primitive thresholds** — replaces
//!    static `semantic-thresholds.yaml` with a SONA-adapted RuVector
//!    inference. Per-home thresholds learned from the node's own
//!    long-term observation stream.
//! 4. **Ed25519 witness chain** — every state transition signed so
//!    regulated deployments (healthcare, education, shared housing)
//!    have a tamper-evident audit log.
//! 5. **Multi-node federation** — peer discovery via mDNS + cross-node
//!    event deduplication keyed on ADR-110's ≤100 µs mesh-aligned
//!    timestamps. One fall in a shared room emits one alert, not N.
//! 6. **OTA firmware coordination** — the cog manages C6 firmware
//!    rollouts for ESP32-C6 nodes in the local mesh.
//!
//! The cog binary entrypoint is in `bin/main.rs`. Library modules
//! below are intentionally small and testable per the /loop-worker
//! discipline rules (see `docs/ADR-110-BRANCH-STATE.md`).

pub mod manifest;
pub mod mdns;
pub mod runtime;
pub mod witness;
pub mod witness_signing;

/// Stable integration identifier used in the manifest.
pub const COG_ID: &str = "ha-matter";

/// mDNS service type advertised when the cog starts.
pub const MDNS_SERVICE_TYPE: &str = "_ruview-ha._tcp";

/// Default port for the cog's local HTTP control surface (`/health`,
/// `/api/v1/cog/status`). Distinct from the MQTT broker port.
pub const DEFAULT_CONTROL_PORT: u16 = 9180;

/// Default port for the embedded MQTT broker, when enabled.
pub const DEFAULT_EMBEDDED_BROKER_PORT: u16 = 1883;
