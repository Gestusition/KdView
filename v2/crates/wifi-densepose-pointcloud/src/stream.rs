//! HTTP server — live camera + ESP32 CSI + fusion → real-time point cloud.

use crate::brain_bridge;
use crate::camera;
use crate::csi_pipeline;
use crate::depth;
use crate::fusion;
use crate::pointcloud;
use axum::{
    extract::State,
    http::{HeaderValue, Method},
    response::Html,
    routing::get,
    Json, Router,
};
use std::sync::{Arc, Mutex};
use tower_http::cors::{AllowOrigin, CorsLayer};

const KDVIEW_PAGES_ORIGIN: &str = "https://gestusition.github.io";

fn configured_cors_origins() -> Vec<String> {
    std::env::var("KDVIEW_CORS_ORIGINS")
        .or_else(|_| std::env::var("RUVIEW_CORS_ORIGINS"))
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(str::to_owned)
        .collect()
}

fn is_loopback_origin(origin: &str) -> bool {
    origin == "http://localhost"
        || origin
            .strip_prefix("http://localhost:")
            .is_some_and(|port| !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit()))
        || origin == "http://127.0.0.1"
        || origin
            .strip_prefix("http://127.0.0.1:")
            .is_some_and(|port| !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit()))
        || origin == "http://[::1]"
        || origin
            .strip_prefix("http://[::1]:")
            .is_some_and(|port| !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit()))
}

fn is_cors_origin_allowed(origin: &str, configured: &[String]) -> bool {
    origin == KDVIEW_PAGES_ORIGIN
        || is_loopback_origin(origin)
        || origin == "null"
        || configured.iter().any(|allowed| allowed == origin)
}

struct AppState {
    latest_cloud: Mutex<pointcloud::PointCloud>,
    latest_splats: Mutex<Vec<pointcloud::GaussianSplat>>,
    latest_pipeline: Mutex<Option<csi_pipeline::PipelineOutput>>,
    frame_count: Mutex<u64>,
    use_camera: bool,
}

/// Start the HTTP/viewer server bound to `bind` (e.g.
/// `"127.0.0.1:9880"` — the safe default — or `"0.0.0.0:9880"` to expose
/// the viewer to the LAN).
///
/// **Security**: the viewer streams live camera/CSI/vitals data. Bind to
/// `127.0.0.1` unless you intentionally want remote viewers.
pub async fn serve(bind: &str, _brain: Option<&str>) -> anyhow::Result<()> {
    let has_camera = camera::camera_available();

    // Start CSI pipeline — listens for UDP CSI data from ESP32 nodes.
    // Kept on 0.0.0.0 because ESP32 nodes are remote devices on the LAN.
    let csi_pipeline_state = csi_pipeline::start_pipeline("0.0.0.0:3333");
    eprintln!("  CSI pipeline: UDP port 3333 (ADR-018 binary frames)");

    let initial_cloud = if has_camera {
        capture_camera_cloud()
    } else {
        demo_cloud()
    };
    let initial_splats = pointcloud::to_gaussian_splats(&initial_cloud);

    let state = Arc::new(AppState {
        latest_cloud: Mutex::new(initial_cloud),
        latest_splats: Mutex::new(initial_splats),
        latest_pipeline: Mutex::new(None),
        frame_count: Mutex::new(0),
        use_camera: has_camera,
    });

    // Background: capture + fuse every 500ms (motion-adaptive)
    let bg = state.clone();
    let bg_csi = csi_pipeline_state.clone();
    let bg_cam = has_camera;
    tokio::spawn(async move {
        let mut skip_depth = false;
        loop {
            // Motion-adaptive: check CSI motion score
            let pipeline_out = Some(csi_pipeline::get_pipeline_output(&bg_csi));
            if let Some(ref out) = pipeline_out {
                // Only run expensive depth when motion detected or every 5th frame
                let frame_num = *bg.frame_count.lock().unwrap();
                skip_depth = !out.motion_detected && frame_num % 5 != 0;
            }
            let pipeline_clone = pipeline_out.clone();
            *bg.latest_pipeline.lock().unwrap() = pipeline_out;
            let pipeline_out = pipeline_clone;

            let interval = if skip_depth { 1000 } else { 500 }; // slower when no motion
            tokio::time::sleep(std::time::Duration::from_millis(interval)).await;

            let (cloud, luminance) = if bg_cam && !skip_depth {
                tokio::task::spawn_blocking(capture_camera_cloud_with_luminance)
                    .await
                    .unwrap_or_else(|_| (demo_cloud(), None))
            } else {
                // Reuse previous cloud when no motion
                (bg.latest_cloud.lock().unwrap().clone(), None)
            };
            // Feed luminance into the CSI pipeline so is_dark toggles for the
            // viewer. The lock is held briefly here — the UDP thread never
            // touches it (messages go through the mpsc channel).
            if let Some(lum) = luminance {
                if let Ok(mut st) = bg_csi.lock() {
                    st.set_light_level(lum);
                }
            }
            let splats = pointcloud::to_gaussian_splats(&cloud);
            *bg.latest_cloud.lock().unwrap() = cloud;
            *bg.latest_splats.lock().unwrap() = splats;
            let frame_num = {
                let mut fc = bg.frame_count.lock().unwrap();
                *fc += 1;
                *fc
            };

            // Brain sync — sparse, every 120 frames (~60 seconds)
            if frame_num % 120 == 0 {
                if let Some(ref out) = pipeline_out {
                    brain_bridge::sync_to_brain(out, frame_num).await;
                }
            }
        }
    });

    if has_camera {
        eprintln!("  Camera: LIVE (/dev/video0)");
    } else {
        eprintln!("  Camera: DEMO");
    }

    // CORS — allow the hosted GitHub Pages viewer to fetch /api/splats from a
    // locally-running instance of this server. Modern browsers treat
    // 127.0.0.1/localhost as a "potentially trustworthy" origin so the HTTPS
    // page can reach a plain-HTTP loopback backend without mixed-content
    // blocking. Origins permitted:
    //   - https://gestusition.github.io (the published KdView Pages demo)
    //   - http://localhost:* / http://127.0.0.1:* (developer running the
    //     viewer.html bundled with this binary)
    //   - exact origins listed in KDVIEW_CORS_ORIGINS (comma-separated), with
    //     RUVIEW_CORS_ORIGINS retained as a compatibility alias
    // Anything else is denied, so this is not a "wildcard" CORS.
    let configured_origins = configured_cors_origins();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin: &HeaderValue, _req| {
            let s = match origin.to_str() {
                Ok(v) => v,
                Err(_) => return false,
            };
            is_cors_origin_allowed(s, &configured_origins)
        }))
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/", get(index))
        .route("/api/cloud", get(api_cloud))
        .route("/api/splats", get(api_splats))
        .route("/api/status", get(api_status))
        .route("/health", get(api_health))
        .layer(cors)
        .with_state(state);

    println!("╔══════════════════════════════════════════════╗");
    println!("║  RuView Dense Point Cloud — ALL SENSORS      ║");
    println!("╚══════════════════════════════════════════════╝");
    println!("  Viewer: http://{bind}/");
    if bind.starts_with("0.0.0.0") || bind.starts_with("::") {
        eprintln!(
            "  WARNING: bound to {bind} — camera/CSI/vitals are exposed \
             to the network. Use --bind 127.0.0.1:9880 to restrict to loopback."
        );
    }

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod cors_tests {
    use super::is_cors_origin_allowed;

    #[test]
    fn allows_fork_pages_and_loopback_development_origins() {
        let configured = Vec::new();
        assert!(is_cors_origin_allowed(
            "https://gestusition.github.io",
            &configured
        ));
        assert!(is_cors_origin_allowed("http://localhost:5173", &configured));
        assert!(is_cors_origin_allowed("http://127.0.0.1:9880", &configured));
        assert!(is_cors_origin_allowed("http://[::1]:9880", &configured));
        assert!(is_cors_origin_allowed("null", &configured));
    }

    #[test]
    fn allows_only_exact_configured_origins() {
        let configured = vec!["https://viewer.example".to_owned()];
        assert!(is_cors_origin_allowed(
            "https://viewer.example",
            &configured
        ));
        assert!(!is_cors_origin_allowed(
            "https://viewer.example.evil",
            &configured
        ));
    }

    #[test]
    fn rejects_prefix_spoofing_and_retired_vendor_origin_by_default() {
        let configured = Vec::new();
        assert!(!is_cors_origin_allowed(
            "http://localhost.evil:5173",
            &configured
        ));
        assert!(!is_cors_origin_allowed(
            "https://ruvnet.github.io",
            &configured
        ));
    }
}

fn capture_camera_cloud() -> pointcloud::PointCloud {
    capture_camera_cloud_with_luminance().0
}

/// Grab one camera frame, backproject it to a point cloud, and return the
/// mean luminance alongside (used to drive `set_light_level` for night mode).
fn capture_camera_cloud_with_luminance() -> (pointcloud::PointCloud, Option<f32>) {
    let config = camera::CameraConfig::default();
    match camera::capture_frame(&config) {
        Ok(frame) => {
            // Mean luminance across the RGB frame (BT.601 coefficients).
            let pixels = (frame.width as usize) * (frame.height as usize);
            let mut sum = 0.0f64;
            let mut n = 0usize;
            for chunk in frame.rgb.chunks_exact(3).take(pixels) {
                sum += 0.299 * chunk[0] as f64 + 0.587 * chunk[1] as f64 + 0.114 * chunk[2] as f64;
                n += 1;
            }
            let lum = if n > 0 {
                Some((sum / n as f64) as f32)
            } else {
                None
            };

            let cloud = match depth::estimate_depth(&frame.rgb, frame.width, frame.height) {
                Ok(dm) => {
                    let intr = depth::CameraIntrinsics::default();
                    depth::backproject_depth(&dm, &intr, Some(&frame.rgb), 2)
                }
                Err(_) => depth::demo_depth_cloud(),
            };
            (cloud, lum)
        }
        Err(_) => (depth::demo_depth_cloud(), None),
    }
}

fn demo_cloud() -> pointcloud::PointCloud {
    let occ = fusion::demo_occupancy();
    let wc = fusion::occupancy_to_pointcloud(&occ);
    let dc = depth::demo_depth_cloud();
    fusion::fuse_clouds(&[&wc, &dc], 0.05)
}

async fn api_cloud(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let cloud = state.latest_cloud.lock().unwrap();
    let (min, max) = cloud.bounds();
    let frames = *state.frame_count.lock().unwrap();
    let pipeline = state.latest_pipeline.lock().unwrap();
    Json(serde_json::json!({
        "points": cloud.points.len(),
        "bounds_min": min, "bounds_max": max,
        "live": state.use_camera,
        "frame": frames,
        "pipeline": &*pipeline,
        "cloud": cloud.points.iter().take(1000).collect::<Vec<_>>(),
    }))
}

async fn api_splats(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let splats = state.latest_splats.lock().unwrap();
    let frames = *state.frame_count.lock().unwrap();
    let pipeline = state.latest_pipeline.lock().unwrap();
    Json(serde_json::json!({
        "splats": &*splats,
        "count": splats.len(),
        "live": state.use_camera,
        "frame": frames,
        "pipeline": &*pipeline,
        "timestamp": chrono::Utc::now().timestamp_millis(),
    }))
}

async fn api_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let frames = *state.frame_count.lock().unwrap();
    let pipeline = state.latest_pipeline.lock().unwrap();
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "live": state.use_camera,
        "camera": if state.use_camera { "/dev/video0" } else { "demo" },
        "csi_pipeline": "active (UDP:3333)",
        "pipeline": &*pipeline,
        "frames_captured": frames,
    }))
}

async fn api_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

/// Viewer HTML/JS, compiled into the binary at build time. Keep the
/// markup in `viewer.html` to keep this file under the 500-LOC limit and
/// to make it trivially editable (no Rust rebuild when tweaking JS).
static VIEWER_HTML: &str = include_str!("viewer.html");

async fn index() -> Html<&'static str> {
    Html(VIEWER_HTML)
}
