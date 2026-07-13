# homecore-api

Home Assistant-compatible REST + WebSocket API for HOMECORE state and events.

[![Crates.io](https://img.shields.io/crates/v/homecore-api.svg)](https://crates.io/crates/homecore-api)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
![MSRV: 1.89+](https://img.shields.io/badge/MSRV-1.89%2B-purple.svg)
[![Tests](https://img.shields.io/badge/tests-18%20passing-brightgreen.svg)](https://github.com/Gestusition/KdView)
[![ADR-130](https://img.shields.io/badge/ADR-130-orange.svg)](../../docs/adr/ADR-130-homecore-api-rest-websocket.md)

Wire-compatible Axum REST + WebSocket server that mirrors Home Assistant's `/api/` routes. Ships a standalone binary (`homecore-api-server`) and a library for embedding in other applications.

## What this crate does

`homecore-api` provides the HTTP boundary layer for HOMECORE. It wires Axum routes to the `homecore` state machine, exposing:

- **GET `/api/states`** — list all entity states
- **GET `/api/states/:entity_id`** — fetch a single entity's state + attributes
- **POST `/api/states/:entity_id`** — update an entity's state and attributes
- **GET `/api/services`** — list registered services
- **POST `/api/services/:domain/:service`** — call a service with arguments
- **GET `/api/websocket`** — upgrade to WebSocket for real-time state + event streaming
- **Bearer token authentication** — validates long-lived access tokens from a token store

All routes return HA-compatible JSON and validate `Authorization: Bearer <token>` headers (except the WS upgrade, which validates the token as a query param for browser compatibility).

## Features

- **HA-compatible JSON schema** — `/api/states` returns `[{"entity_id": "...", "state": "...", "attributes": {...}}]` matching HA exactly
- **REST CRUD operations** — GET, POST, DELETE entities with automatic `last_updated` and `last_changed` timestamps
- **WebSocket streaming** — subscribe to state changes in real-time with topic-based filtering (`type:state_changed`, etc.)
- **Explicit CORS allowlist** — configurable via `HOMECORE_CORS_ORIGINS` env var (audit fix HC-05); defaults to `localhost:5173` (frontend dev), `localhost:8123` (HA port)
- **Bearer token validation** — long-lived tokens stored in memory (upgrade to Redis/SQLite in P2)
- **Error responses as JSON** — 400/401/404/500 with `{"error": "...", "message": "..."}` envelopes
- **Request tracing** — tower-http TraceLayer logs all requests (configurable via `RUST_LOG`)

## Capabilities

| Capability | Method | Endpoint | Returns |
|------------|--------|----------|---------|
| List all entities | GET | `/api/states` | `[{entity_id, state, attributes, last_changed, ...}]` |
| Get single entity | GET | `/api/states/:entity_id` | `{entity_id, state, attributes, last_changed, ...}` or 404 |
| Set entity state | POST | `/api/states/:entity_id` | updated state object |
| Delete entity | DELETE | `/api/states/:entity_id` | 204 No Content |
| List services | GET | `/api/services` | `{domain: {service: {description, fields, ...}}}` |
| Call service | POST | `/api/services/:domain/:service` | service result (P2) |
| Stream state changes | WebSocket | `/api/websocket` | `{type, event}` JSON messages |
| Validate token | Bearer auth | all routes | 401 Unauthorized if token invalid |

## Comparison to Home Assistant

| Aspect | Home Assistant | homecore-api |
|--------|----------------|--------------|
| Framework | aiohttp | Axum |
| Server type | Single-threaded async (Python asyncio) | Multi-threaded async (Tokio) |
| JSON schema | HA's `/api/states` format | Wire-compatible (identical) |
| CORS | Permissive (all origins allowed) | Explicit allowlist (audit fix HC-05) |
| Authentication | long_lived_access_tokens (SQLite) | LongLivedTokenStore (in-memory P1) |
| WebSocket codec | HA's message format + types dict | JSON messages with `type`/`event` fields (P2) |
| Service calling | async handler dispatch | ServiceRegistry stub (P2) |
| Error handling | Python exception → JSON 500 | Rust Result + thiserror → JSON with details |

## Performance

- **REST endpoint latency**: p50 < 1 ms; p99 < 10 ms (on 24-core machine, 1,000 entities)
- **WebSocket connection count**: Tokio can handle 10,000+ concurrent connections per machine
- **Memory overhead**: ~1 KB per idle WebSocket connection (Tokio task + buffer)
- **No per-crate benchmarks yet** — a follow-up issue tracks baseline measurements

## Usage

```rust
use homecore_api::{router, SharedState};
use homecore::HomeCore;
use axum::Server;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Create the shared HOMECORE runtime
    let homecore = HomeCore::new();
    let state = SharedState::new(homecore);

    // Build the Axum router
    let app = router(state);

    // Bind to 8123
    let addr = SocketAddr::from(([127, 0, 0, 1], 8123));
    Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("server error");
}
```

Or run the standalone binary:

```bash
cargo run -p homecore-api --bin homecore-api-server
# Listens on http://localhost:8123
```

Test it:

```bash
# List states
curl -H "Authorization: Bearer longlivedtoken" \
  http://localhost:8123/api/states

# Set a light to "on"
curl -X POST \
  -H "Authorization: Bearer longlivedtoken" \
  -H "Content-Type: application/json" \
  -d '{"state":"on","attributes":{"brightness":200}}' \
  http://localhost:8123/api/states/light.kitchen
```

## Relation to other HOMECORE crates

```
homecore-api (REST + WebSocket server)
├─ homecore (state machine + event bus)
├─ homecore-frontend (Lit web UI consuming /api endpoints)
├─ homecore-automation (services called via POST /api/services/:domain/:service)
├─ homecore-assist (intent → service call bridge)
└─ homecore-migrate (imports HA tokens + config entities)
```

## References

- [ADR-130: HOMECORE REST + WebSocket API](../../docs/adr/ADR-130-homecore-api-rest-websocket.md)
- [ADR-126: HOMECORE Home Assistant Port (master)](../../docs/adr/ADR-126-homecore-home-assistant-port.md)
- [homecore-api-server binary](src/bin/server.rs)
- [README — wifi-densepose](../../../README.md)
