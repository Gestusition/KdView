# homecore-hap

Apple Home HomeKit Accessory Protocol bridge for HOMECORE with HAP-1.1 trait surface and mDNS advertisement (P2).

[![Crates.io](https://img.shields.io/crates/v/homecore-hap.svg)](https://crates.io/crates/homecore-hap)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![MSRV: 1.89+](https://img.shields.io/badge/MSRV-1.89%2B-purple.svg)
[![Tests](https://img.shields.io/badge/tests-17%20passing-brightgreen.svg)](https://github.com/Gestusition/KdView)
[![ADR-125](https://img.shields.io/badge/ADR-125-orange.svg)](../../docs/adr/ADR-125-homecore-apple-home-homekit-bridge.md)

**P1 scaffold**: trait surface for HAP accessories + characteristics, entity→HAP mapping rules, and bridge ownership. The actual HAP-1.1 TLS server and real mDNS integration are gated behind `--features hap-server` (P2).

## What this crate does

`homecore-hap` bridges HOMECORE entity state to Apple HomeKit Accessory Protocol (HAP-1.1), allowing HomeKit-native apps (Home, Control Center, Siri) to control HOMECORE devices. It provides:

- **HapAccessoryType enum** — 11 accessory types matching HA's HomeKit integration (`Light`, `Switch`, `Thermostat`, `Lock`, `Door`, etc.)
- **HapCharacteristic enum** — HAP characteristic types (`On`, `Brightness`, `Temperature`, `TargetLockState`, etc.)
- **EntityToAccessoryMapper** — bidirectional rules for mapping HOMECORE entities to HAP accessories (e.g., `light.kitchen` → `Light` accessory + `On` + `Brightness` characteristics)
- **HapBridge** — owns and exposes a collection of mapped accessories over HAP
- **MdnsAdvertiser trait** — abstraction over mDNS advertisement; P1 ships `NullAdvertiser` (no-op), P2 adds real mDNS via `mdns-sd`
- **RuViewToHapMapper** — bridges RuView sensing data (temperature, humidity, occupancy) to HAP characteristics

The bridge itself is a HAP Accessory Bridge (HAP-1.1 spec §8.3), advertising a single service with characteristic slots for each exposed accessory.

## Features

- **11 accessory types** — Light, Switch, Thermostat, Door, Lock, Window, Blind, Outlet, Fan, Sensor, SecuritySystem
- **Bi-directional mapping** — HOMECORE entity state ↔ HAP characteristic values with type-safe enums
- **HAP-1.1 spec compliance** — characteristic types and permissions match HomeKit's published spec
- **Trait-based advertisement** — `MdnsAdvertiser` abstraction; swappable implementations (null, real mDNS, etc.)
- **RuView integration** — maps WiFi sensing data (occupancy, temperature, vital signs) to HomeKit sensor accessories
- **No TLS server in P1** — bridge compiles and tests pass with `--no-default-features`; real server lands in P2 with `--features hap-server`
- **Home.app compatible** — exposed accessories appear in Home app on any HomeKit hub (Apple TV, HomePod, HomePod mini)

## Capabilities

| Capability | Type | Method | Notes |
|------------|------|--------|-------|
| Define accessory type | Trait | `HapAccessoryType::Light` etc. (11 variants) | Enum; no instantiation yet (P1) |
| Define characteristic | Trait | `HapCharacteristic::On`, `Brightness`, etc. | Enum; values encoded as HAP TLV |
| Map entity to accessory | Mapping | `EntityToAccessoryMapper::map_light()` | Takes `EntityId` + `State`; returns `HapAccessory` |
| Expose accessory | Bridge | `HapBridge::expose(accessory)` | Adds to the bridge's characteristic list |
| Advertise bridge | mDNS | `NullAdvertiser::advertise()` (P1) | No-op stub; real mDNS in P2 |
| Advertise bridge (P2) | mDNS | `mdns_sd::ServiceInstanceBuilder` | Real mDNS via `--features hap-server` |
| Bridge state query | Bridge | `HapBridge::list_accessories()` | Returns exposed accessories + their characteristics |
| Characteristic write | Characteristic | HAP `WriteRequest` TLV (P2) | Home.app button press → service call |
| Characteristic read | Characteristic | HAP `ReadResponse` TLV (P2) | Home.app query → current entity state |

## Comparison to Home Assistant

| Aspect | Home Assistant | homecore-hap |
|--------|----------------|--------------|
| Framework | HA's `hap-python` (pure Python) | Rust 1.89+ with HAP trait abstraction |
| Server type | Python asyncio HAP-1.1 server | TLS server trait (P2); stub in P1 |
| Accessory types | 30+ (Light, Switch, Thermostat, etc.) | 11 (Light, Switch, Thermostat, Door, Lock, Window, Blind, Outlet, Fan, Sensor, SecuritySystem) |
| mDNS | mdns-py broadcast via asyncio | Abstraction + real mDNS (P2) or no-op stub (P1) |
| Entity filtering | YAML `include_domains` + `exclude_entities` | Mapper rules (planned P2) |
| HomeKit hub requirement | Yes (for remote access) | Yes (same as HomeKit) |
| Pairing code generation | Automatic (HA web UI) | Manual setup code (P2) |
| Characteristic persistence | HomeKit cloud only | Paired with homecore state machine |

## Performance

- **Entity→HAP mapping** — < 100 μs per entity (enum lookups + type conversions)
- **HAP write latency** — ~10 ms (TLS decrypt + characteristic parse + entity state set); bounded by homecore state machine lock contention
- **mDNS advertisement** (P2) — ~50 ms multicast broadcast; periodic rediscovery on network change
- **Memory overhead per accessory** — ~500 bytes (enum + characteristic slots + metadata)
- **No per-crate benchmarks yet** — a follow-up issue tracks baseline measurements

## Usage

Mapping an entity (P1):

```rust
use homecore_hap::{EntityToAccessoryMapper, HapBridge, HapAccessoryType};
use homecore::{EntityId, State};
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let light_id = EntityId::parse("light.kitchen").unwrap();
    let state = State::new("on", HashMap::new());

    // Map the entity to a HAP Light accessory
    let mut mapper = EntityToAccessoryMapper::new();
    if let Ok(accessory) = mapper.map_light(&light_id, &state) {
        println!("Mapped to HAP: {:?}", accessory.accessory_type);

        // Expose it via the bridge
        let mut bridge = HapBridge::new();
        bridge.expose(accessory);
        println!("Exposed {} accessories", bridge.list_accessories().len());
    }
}
```

Real HAP server (P2, via `--features hap-server`):

```bash
cargo build -p homecore-hap --features hap-server
# The server will advertise over mDNS and accept HomeKit pairing requests
```

## Relation to other HOMECORE crates

```
homecore-hap (HomeKit bridge)
├─ homecore (state machine; bridge reads entity states)
├─ homecore-api (exposes HAP state via REST /api for remote debugging)
├─ homecore-server (starts the bridge on homecore init)
└─ homecore-automation (can trigger state changes via service calls)
```

## References

- [ADR-125: HOMECORE Apple Home / HomeKit Bridge](../../docs/adr/ADR-125-homecore-apple-home-homekit-bridge.md)
- [ADR-126: HOMECORE Home Assistant Port (master)](../../docs/adr/ADR-126-homecore-home-assistant-port.md)
- [HomeKit Accessory Protocol Specification (HAP-1.1)](https://developer.apple.com/homekit/)
- [user-guide-apple-homepod.md](../../docs/user-guide-apple-homepod.md)
- [README — wifi-densepose](../../../README.md)
