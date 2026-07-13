//! ADR-115 §2 — MQTT auto-discovery publisher (HA-DISCO).
//!
//! This module implements the dual-protocol Home Assistant integration's
//! primary path: MQTT + HA auto-discovery. It owns the full lifecycle:
//!
//! 1. Connect to a user-supplied broker with optional TLS / mTLS.
//! 2. Publish HA discovery `config` topics (retained) on connect and at
//!    a refresh interval, so HA auto-creates one device + N entities per
//!    RuView node.
//! 3. Translate `sensing-server` broadcast messages (`edge_vitals`,
//!    `pose_data`, `sensing_update`) into per-entity state messages with
//!    rate limits.
//! 4. Maintain a `availability` topic per entity with LWT for offline
//!    detection.
//!
//! The module is gated behind the `mqtt` Cargo feature so the default
//! `sensing-server` binary stays small for users who don't need HA
//! integration. CLI flags parse unconditionally; the publisher is a
//! no-op without the feature.
//!
//! ## Layout
//!
//! - [`discovery`] — HA discovery payload generators per entity type
//! - [`state`]     — per-entity state-message encoders + rate limiter
//! - [`publisher`] — connection lifecycle + topic publication
//! - [`privacy`]   — biometric stripping per `--privacy-mode`
//! - [`config`]    — `MqttConfig` struct fed by [`crate::cli::Args`]
//!
//! ## Cross-protocol coupling
//!
//! The semantic inference layer (ADR-115 §3.12, future `crate::semantic`)
//! emits primitive state changes onto a `tokio::broadcast` channel that
//! this module also subscribes to. Same channel is consumed by the Matter
//! Bridge (ADR-115 §3.11, future `crate::matter`), so adding a new
//! semantic primitive automatically flows to all surfaces.

pub mod config;
pub mod discovery;
pub mod privacy;
pub mod security;
// State encoders + rate limiter compile without rumqttc, so they're
// available for testing under `--no-default-features`. Only the
// publisher itself (which holds the `rumqttc::AsyncClient`) needs the
// `mqtt` feature.
pub mod state;

#[cfg(feature = "mqtt")]
pub mod publisher;

pub use config::MqttConfig;
pub use discovery::{
    AvailabilityPayload, DeviceMeta, DiscoveryComponent, DiscoveryConfig, OriginMeta,
};

/// Stable origin string written into every HA discovery payload's `origin`
/// block so HA users can see which RuView version emitted the entities.
pub const ORIGIN_NAME: &str = "wifi-densepose-sensing-server";

/// Stable manufacturer string written into every HA discovery payload's
/// `device` block.
pub const MANUFACTURER: &str = "KdView";

/// Stable `support_url` written into every HA discovery payload's `origin`
/// block. Points operators at the canonical fork's issue tracker.
pub const SUPPORT_URL: &str = "https://github.com/Gestusition/KdView/issues";

/// Stable HA discovery topic prefix default. Maintainer-accepted in
/// ADR-115 §9.2 — ship Home Assistant's own default rather than a
/// RuView-namespaced one, so the integration is plug-and-play with a
/// stock Mosquitto add-on. Operators with custom HA setups can override
/// via `--mqtt-prefix`.
pub const DEFAULT_DISCOVERY_PREFIX: &str = "homeassistant";
