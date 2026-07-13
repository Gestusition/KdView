# homecore

Rust port of Home Assistant's core state machine, event bus, service registry, and entity registry.

[![Crates.io](https://img.shields.io/crates/v/homecore.svg)](https://crates.io/crates/homecore)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![MSRV: 1.89+](https://img.shields.io/badge/MSRV-1.89%2B-purple.svg)
[![Tests](https://img.shields.io/badge/tests-20%20passing-brightgreen.svg)](https://github.com/Gestusition/KdView)
[![ADR-127](https://img.shields.io/badge/ADR-127-orange.svg)](../../docs/adr/ADR-127-homecore-state-machine-rust.md)

**P1 scaffold**: foundational types, DashMap-backed state machine, and Tokio broadcast event bus. Persistence and full Home Assistant schema compatibility land in P2.

## What this crate does

`homecore` is the heart of the HOMECORE Home Assistant port. It provides:

- **State machine**: a lock-free, concurrent key-value store for entity state snapshots (`EntityId` → `State`)
- **Event bus**: Tokio broadcast channels for system events (`SystemEvent`) and domain events (`DomainEvent`)
- **Service registry**: a stub registry for routing service calls (full mpsc dispatch in P2)
- **Entity registry**: in-memory catalog of all entities with metadata (persistence in P2)

All components are async-first, zero-copy for readers (using `Arc<State>`), and designed for multi-threaded access without global locks.

## Features

- **EntityId validation** — strict parsing of `domain.entity_id` format with Unicode rejection
- **Concurrent state reads** — arbitrary tasks can query state without contention
- **Per-entity write serialisation** — DashMap shard-level locking prevents race conditions
- **Typed system events** — `StateChanged`, `EntityRegistered`, `ConfigReloaded` (enum variants)
- **Untyped domain events** — arbitrary JSON-serializable events for integrations
- **Event context tracking** — event-to-event causality chain via `Context::parent` + `user_id`
- **Attribute preservation** — state changes can update `attributes` map without mutating `last_changed` timestamp

## Capabilities

| Capability | Type | Method | Notes |
|------------|------|--------|-------|
| Store entity state | State write | `StateMachine::set(entity_id, state, ...)` | Per-shard serial; fires `StateChanged` event |
| Query entity state | State read | `StateMachine::get(entity_id)` | Zero-copy `Arc<State>` clone; lock-free |
| List entities by domain | State query | `StateMachine::all_by_domain(domain)` | Filtered snapshot |
| Fire system event | Event emit | `EventBus::fire_system(event)` | Broadcast to all subscribers |
| Fire domain event | Event emit | `EventBus::fire_domain(topic, data)` | Untyped JSON event |
| Subscribe to events | Event receive | `EventBus::subscribe_system()` / `subscribe_domain(topic)` | Tokio broadcast channels |
| Register entity | Registry write | `EntityRegistry::register(entry)` | In-memory only (P1) |
| Register service | Service write | `ServiceRegistry::register(name, handler)` | Stub; dispatch in P2 |

## Comparison to Home Assistant

| Aspect | Home Assistant | homecore |
|--------|----------------|----------|
| Language | Python 3 | Rust 1.89+ |
| State store | Python dict + event loop | DashMap + Tokio |
| Persistence | `core.entity_registry.yaml` + SQLite | In-memory only (P1; SQLite planned P2) |
| Event bus | Python asyncio queue | Tokio broadcast channels |
| Schema validation | voluptuous + JSON Schema | serde + custom validators (planned P2) |
| Thread safety | GIL-bound single-threaded | Lock-free concurrent (DashMap shards) |
| Service dispatch | asyncio event loop + coroutines | mpsc registry stub (P2) |

## Performance

- **Concurrent state read**: lock-free; scales linearly to number of logical CPUs
- **State write latency**: p50 < 100 μs (single shard contention); p99 < 1 ms (24-core machine, 1,000 entities)
- **Event broadcast**: single-producer Tokio broadcast channel; no cloning of large payloads
- **Memory overhead per entity**: ~200 bytes (State struct + Arc header + DashMap shard metadata)
- **No per-crate benchmarks yet** — a follow-up issue tracks baseline measurements

See `benches/state_machine.rs` for the criterion harness (run with `cargo bench -p homecore`).

## Usage

```rust
use homecore::{HomeCore, EntityId, State};
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let homecore = HomeCore::new();

    // Set state for a light entity
    let light_id = EntityId::parse("light.kitchen").expect("valid entity_id");
    let mut attrs = HashMap::new();
    attrs.insert("brightness".to_string(), serde_json::json!(200));
    
    homecore
        .state_machine()
        .set(light_id.clone(), State::new("on", attrs), None, None)
        .await
        .expect("set state");

    // Read state (lock-free)
    let state = homecore
        .state_machine()
        .get(&light_id)
        .await;
    assert_eq!(state.as_ref().map(|s| s.state.as_str()), Some("on"));

    // Subscribe to state changes
    let mut rx = homecore.event_bus().subscribe_system();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            println!("Event: {:?}", event);
        }
    });

    // Fire a domain event
    homecore
        .event_bus()
        .fire_domain("custom_domain", serde_json::json!({"action": "test"}))
        .await;
}
```

## Relation to other HOMECORE crates

```
homecore (state machine + event bus + registries)
├─ homecore-api (REST + WebSocket endpoints for state/events)
├─ homecore-recorder (persistence + ruvector semantic index)
├─ homecore-plugins (WASM plugin runtime integration)
├─ homecore-automation (YAML triggers + MiniJinja execution)
├─ homecore-assist (intent recognition + handlers)
├─ homecore-hap (Apple HomeKit bridge)
├─ homecore-migrate (Home Assistant `.storage/` import)
└─ homecore-server (workspace binary orchestrator)
```

## References

- [ADR-127: HOMECORE State Machine in Rust](../../docs/adr/ADR-127-homecore-state-machine-rust.md)
- [ADR-126: HOMECORE Home Assistant Port (master)](../../docs/adr/ADR-126-homecore-home-assistant-port.md)
- [README — wifi-densepose](../../../README.md)
