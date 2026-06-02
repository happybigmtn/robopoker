# AGENTS.md — robopoker development instructions

This repository is a Rust workspace for game-theoretically optimal poker strategies (No-Limit Texas Hold'em, short deck, MCCFR, optimal transport, hand evaluation).

## Build & Test
- `cargo check --workspace`
- `cargo test --workspace` (may be long; use -- --test-threads=4 for parallelism)
- `cargo build --release` for binaries in `bin/`

## TUI / Frontend
- The `bin/tui` crate provides a read-only TUI preview (ratatui based).
- Artifacts from development live in `.auto/tui*/`
- Prefer incremental changes; run `cargo run -p rbp-tui` or equivalent after changes.

## Auto dev tooling
- `auto doctor` for readiness
- `auto corpus`, `auto gen`, `auto parallel` for planning/execution
- Always run `auto doctor` before model-backed work
- Planning root: genesis/ (create with `auto corpus` if missing)

## Git & Commits
- Work on feature branches for changes
- Commit only verified changes with green tests
- Push only from feature/work branches
- Never push broken main

## Scope for agents
- Focus on one bounded slice per run
- Do not discard uncommitted work without explicit reason
- Prefer repo `auto` commands
- Update this file when conventions change

## Crates
See README.md for crate overview (rbp-cards, rbp-mccfr, rbp-nlhe, bin/tui, etc).
