//! `rbp-dashboard` — STW-036 testnet dashboard crate.
//!
//! The visible consumer of the STW-034 `INDEX.json` aggregator
//! + the STW-014 `transcript-<id>.json` bundles. A pure-`axum`
//! typed-read + render + serve surface that turns the
//! hermetic-receipt chain into a public reproducible
//! benchmark a testnet dashboard can `curl` + render.
//!
//! ## Architecture (3 layers, mirrors the spec's
//! `(a) / (b) / (c)` shape)
//!
//! 1. [`index_client`] — a typed [`IndexClient`] that reads the
//!    `INDEX.json` aggregator either from a local file path
//!    (the test path) or from a URL the
//!    `RBP_DASHBOARD_INDEX_URL` env knob points at (the prod
//!    path). The client re-uses
//!    `rbp_autotrain::PublishIndex` (the same Rust type the
//!    `trainer --publish-index` arm writes) so a shape drift
//!    in `INDEX.json` fails both the dashboard's typed read
//!    AND the `trainer --verify-index` re-verify at the same
//!    CI step.
//! 2. [`router`] — a thin `axum` router that exposes four
//!    routes on a single [`serve`] entry point:
//!      - `GET /` — serves the static `index.html` (the
//!        vanilla-JS sortable-table frontend the spec ships).
//!      - `GET /api/index` — returns the typed `INDEX.json`
//!        the dashboard's JS fetches.
//!      - `GET /transcript/:id` — proxies the
//!        STW-014 `transcript-<id>.json` bundle a per-row
//!        `Download transcript` link points at.
//!      - `GET /bench/:id` — renders a `BenchReport`-shaped
//!        HTML card (a typed render the
//!        `crates/autotrain/src/bench.rs::BenchReport` struct
//!        feeds, when a future slice wires the bench
//!        JSON into the index).
//! 3. [`render`] — two pure HTML emitters
//!    ([`render_bench_card`], [`render_index_table`]) that
//!    produce the card / table the router serves. Vanilla
//!    `<table>` / `<th>` / `<tr>` / `<td>`; no CSS framework,
//!    no Tailwind, no inline `style=`; the styling lives in a
//!    single `<style>` block in the checked-in `index.html`.
//!
//! ## Scope boundary
//!
//! - No engine state, no DB connection, no training pipeline
//!   dependency. The crate depends on `rbp-autotrain` only
//!   for the typed `PublishIndex` / `IndexedEntry` /
//!   `PublishedRemoteReceipt` types — the bench / trainer
//!   / replay pipelines are NOT re-invoked from here.
//! - No `reqwest` / no `aws-sdk-*` / no `wasm-*`. The
//!   prod-index fetch is `ureq` (a tiny blocking client);
//!   the bucket deploy is the bash runbook's job.
//! - No vendored CSS framework. The static `index.html`
//!   ships a single ~80-line `<style>` block.
//! - No Node / `npm` build step. The `index.html` is a
//!   checked-in file a `cargo build` of the frontend
//!   never touches.

#![warn(unreachable_pub)]

pub mod index_client;
pub mod render;
pub mod router;

pub use index_client::{IndexClient, IndexClientError};
pub use render::{
    BenchCardFields, COMPARE3_FIXTURE_ID, Compare3Report, Compare3SubReport, Compare3Winner,
    EMPTY_STATE_CLASS, compare3_fixture_path, render_bench_card, render_compare3_card,
    render_empty_state_paragraph, render_index_table,
};
pub use router::{AppState, dashboard_app, serve};

/// `pub` (not `#[cfg(test)]`): re-export the
/// `set_empty_state_for_test` /
/// `clear_empty_state_for_test` helpers the
/// `crates/dashboard/tests/smoke.rs`
/// integration tests use to drive the
/// `RBP_DASHBOARD_EMPTY_STATE` env knob
/// without racing on the process-global env
/// var (a `set_var` / `remove_var` pair would
/// leak the value across parallel test
/// boundaries in a
/// `cargo test --test-threads=4` run).
///
/// The functions are *named* `_for_test` so a
/// downstream dashboard binary consumer of
/// `rbp_dashboard` is not tempted to call
/// them — the public API is the env-knob +
/// the `is_empty_state` helper, not the
/// internal override. The integration tests
/// access them via the `rbp_dashboard::`
/// path. The `_for_test` suffix is the same
/// convention the `set_var` /
/// `remove_var` / `Mutex<Option<bool>>` lib
/// tests use elsewhere in the workspace.
pub use router::{
    DeployedUrlTestGuard, EmptyStateTestGuard, clear_deployed_url_for_test,
    clear_empty_state_for_test, clear_inject_cache_for_test, engaged_deployed_url_for_test,
    engaged_empty_state_for_test, inject_cache_snapshot_for_test, set_deployed_url_for_test,
    set_empty_state_for_test,
};
