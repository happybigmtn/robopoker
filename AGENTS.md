## Build & Run

Rust project using Cargo (Edition 2024, Rust 1.90+):

```bash
cargo build --release
cargo run --bin hosting --release   # WebSocket game server
cargo run --bin convert --release   # Interactive CLI
```

## Validation

Run these after implementing to get immediate feedback:

- Tests: `cargo test`
- Typecheck: `cargo check`
- Lint: `cargo clippy -- -D warnings`
- Format: `cargo fmt --check`
- Benchmarks: `cargo bench --features benchmark`

## Feature Flags

- `database` (default): PostgreSQL integration
- `server`: Server-side dependencies (Actix, Tokio, Rayon)
- `shortdeck`: 36-card short deck variant

## Operational Notes

- PostgreSQL required for training/analysis features
- WebSocket server runs on port 8888
- Use `tokio` for async runtime
- Use commonware library (MCP: https://mcp.commonware.xyz) for distributed RNG and consensus

### Existing Codebase Structure

- `src/cards/` - Card types, deck, hand evaluation (fast bitwise)
- `src/gameplay/` - Game engine, betting, showdown, payouts
- `src/gameroom/` - Async game coordination, actors, events
- `src/hosting/` - WebSocket server, HTTP endpoints
- `src/players/` - Player trait (Human, Compute, Network)
- `src/mccfr/` - CFR solver for AI
- `src/clustering/` - Hand abstraction
- `src/database/` - PostgreSQL persistence
