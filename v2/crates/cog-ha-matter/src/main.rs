//! `cog-ha-matter` — RuView Home Assistant + Matter integration (ADR-116).
//!
//! Binary entrypoint. The actual wiring lives in [`cog_ha_matter`] —
//! this main.rs is intentionally tiny so the cog runtime can call
//! into the library from tests and from an edge supervisor's
//! integration tests without re-launching the binary.

use std::process::ExitCode;

use clap::Parser;
use cog_ha_matter::runtime;
use tokio::sync::broadcast;
use tracing::{info, warn};
use wifi_densepose_sensing_server::mqtt::state::VitalsSnapshot;

#[derive(Parser, Debug)]
#[command(
    name = "cog-ha-matter",
    version,
    about = "RuView Home Assistant + Matter integration service",
    long_about = "Wraps the ADR-115 HA-DISCO + HA-MIND publisher as an \
                  edge-deployable service with mDNS, embedded broker, \
                  RuVector-backed thresholds, and Ed25519 witness. See \
                  docs/adr/ADR-116-cog-ha-matter-seed.md for the design."
)]
struct Args {
    /// Where to find the local sensing-server (the cog speaks to it
    /// to pull `VitalsSnapshot` for republication over MQTT/Matter).
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    sensing_url: String,

    /// MQTT broker host. When omitted the cog can spin up an embedded
    /// rumqttd on `DEFAULT_EMBEDDED_BROKER_PORT` (v1: external only).
    #[arg(long, default_value = "127.0.0.1")]
    mqtt_host: String,

    /// MQTT broker port.
    #[arg(long, default_value_t = cog_ha_matter::DEFAULT_EMBEDDED_BROKER_PORT)]
    mqtt_port: u16,

    /// Strip biometrics at the wire — only semantic primitives published.
    /// Matches ADR-115 `--privacy-mode`. The right default for any
    /// deployment with non-tenant occupants.
    #[arg(long)]
    privacy_mode: bool,

    /// Print the manifest the service would self-report at runtime
    /// and exit. Useful for the build-time signer.
    #[arg(long)]
    print_manifest: bool,

    /// mDNS hostname for the local advertisement. Must end with
    /// `.local.` per RFC 6762. Default lets HA's discovery find a
    /// dev cog on localhost without LAN config.
    #[arg(long, default_value = "cog-ha-matter.local.")]
    mdns_hostname: String,

    /// LAN-routable IPv4 the cog binds the control plane on. The
    /// mDNS responder advertises this; HA reaches back to it for
    /// MQTT + Matter Bridge.
    #[arg(long, default_value = "127.0.0.1")]
    mdns_ipv4: String,

    /// Skip the mDNS responder. Useful in containerised CI where
    /// multicast bind is filtered, or when running multiple cog
    /// instances on the same loopback.
    #[arg(long)]
    no_mdns: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cog_ha_matter=info,info".into()),
        )
        .init();

    let args = Args::parse();

    info!(
        sensing_url = %args.sensing_url,
        mqtt = format!("{}:{}", args.mqtt_host, args.mqtt_port),
        privacy = args.privacy_mode,
        "cog-ha-matter starting (ADR-116 P2 scaffold)"
    );

    if args.print_manifest {
        // Emit the manifest with build-time-template placeholders. The
        // Makefile substitutes {{VERSION}} / {{ARCH}} before signing.
        let m = cog_ha_matter::manifest::CogManifest {
            id: cog_ha_matter::COG_ID.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            binary_url: concat!(
                "https://github.com/Gestusition/KdView/releases/download/cog-ha-matter-v",
                env!("CARGO_PKG_VERSION"),
                "/cog-ha-matter-{{ARCH}}"
            )
            .into(),
            binary_bytes: 0,
            binary_sha256: String::new(),
            binary_signature: String::new(),
            installed_at: 0,
            status: "installed".into(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&m).expect("manifest serialization is infallible")
        );
        return ExitCode::SUCCESS;
    }

    // P3: boot the ADR-115 publisher. The broadcast tx is held by
    // main so the channel doesn't close before the sensing-server
    // bridge (next iter) wires its VitalsSnapshot producer in.
    let identity = runtime::CogIdentity::default_for_build();
    let inputs = runtime::build_publisher_inputs(
        &args.mqtt_host,
        args.mqtt_port,
        args.privacy_mode,
        identity,
    );
    let (state_tx, state_rx) =
        broadcast::channel::<VitalsSnapshot>(runtime::DEFAULT_STATE_CHANNEL_CAPACITY);
    let publisher_handle = runtime::spawn_publisher(inputs, state_rx);
    info!(
        capacity = runtime::DEFAULT_STATE_CHANNEL_CAPACITY,
        "publisher spawned — awaiting VitalsSnapshot bridge (P3.5)"
    );

    // P3.5 (next iter): subscribe to the sensing-server's
    // `/v1/snapshot` WebSocket and republish into `state_tx`. Until
    // that lands the cog connects to MQTT, advertises discovery,
    // and just doesn't have any state to publish — exactly what an
    // HA install with no nodes online looks like.
    let _ = &state_tx;

    // P4: mDNS responder. HA's auto-discovery picks the cog up on
    // `_ruview-ha._tcp` so users don't need to type broker host/port.
    let _mdns_handle = if args.no_mdns {
        None
    } else {
        let identity = runtime::CogIdentity::default_for_build();
        let service = cog_ha_matter::mdns::build_mdns_service(
            &identity,
            cog_ha_matter::DEFAULT_CONTROL_PORT,
            args.mqtt_port,
            args.privacy_mode,
        );
        match runtime::start_mdns_responder(&service, &args.mdns_hostname, &args.mdns_ipv4) {
            Ok(h) => {
                info!(
                    fullname = h.fullname(),
                    hostname = %args.mdns_hostname,
                    ipv4 = %args.mdns_ipv4,
                    "mDNS responder registered — HA auto-discovery should find the cog now"
                );
                Some(h)
            }
            Err(e) => {
                warn!(error = ?e, "mDNS responder failed to start — discovery disabled, falling back to manual HA config");
                None
            }
        }
    };

    // Wait on Ctrl-C so the cog runs as a long-lived daemon under
    // the edge process supervisor.
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("ctrl-c received — shutting down");
        }
        joined = publisher_handle => {
            warn!(?joined, "publisher task exited unexpectedly");
        }
    }

    // _mdns_handle drops here, sending the mDNS goodbye packet so
    // HA's discovery integration sees the service leave cleanly.
    ExitCode::SUCCESS
}
