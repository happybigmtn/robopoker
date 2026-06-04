# Implementation Plan

This plan is promoted from the `auto steward --report-only` pass that ran on
2026-06-03 against `main`. Source of truth for the promoted rows is
`/tmp/robopoker-steward-9283/{DRIFT,HAZARDS,HINGES,PROMOTIONS,RETIRE,STEWARDSHIP-REPORT}.md`.

`auto parallel` consumes this file. Each item has an owner set, a scope
boundary, acceptance criteria, verification commands, dependencies, and a
completion signal so a worker can claim and finish it without re-discovering
context.

## Active items (worker-ready)

The most recent slice in the active queue is below. It is shipped
(committed on `main`); the next open slice will be promoted from
`genesis/plans/` or the next `auto steward --report-only` pass before
a worker claims a new card. If neither surfaces a P0 / SEC /
mainnet-blocking item, the next claimable slice is whatever the
planner's `PROMOTIONS.md` ranks highest.

- [x] `STW-018` `trainer --compare` head-to-head v1-vs-v2
  trained-config bench: the CEO testnet roadmap explicitly
  names "a third DCFR-with-LinearWeight variant, or a 'named
  bot vs second trained config' comparison" as the v6 next
  slice after STW-017's `Flagship2` trained config. STW-018
  lands the comparison half. A new `Mode::Compare` + new
  `bench::run_compare` + new `CompareReport` struct + new
  `RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` env knobs + a
  new `compare complete: ...` log line + a new integration
  test in `crates/autotrain/tests/compare.rs` lets a single
  `trainer --compare` invocation seat the v1
  `DatabasePlayer` (seat 0) and the v2 `DatabasePlayer2`
  (seat 1) against each other in the same `Room` over K
  heads-up hands, read per-hand settlements for both
  seats, and print a single-line JSON `CompareReport`
  declaring the winner ("v1", "v2", or "tie") with the
  mbb/100 / CI / win-rate numbers for each side plus the
  v1-vs-v2 `delta_mbb_per_100` (the sign of the delta is
  the winner). The `winner` field is the headline a
  testnet dashboard reads; the v1 / v2 per-side
  `mbb_per_100` / `mbb_ci95` fields let a downstream
  scraper plot the per-config learning curve over a
  series of `--compare` runs (e.g. once per training
  epoch). The compare reuses the same v1 + v2 `Room` shell
  paths the existing `--bench` uses, so a regression in
  the per-hand PnL math fails both the bench and compare
  integration tests in the same CI run. Owner files:
  `crates/autotrain/src/bench.rs` (new `CompareWinner`
  enum + `CompareSubReport` struct + `CompareReport`
  struct + `to_json` + `summarize_compare` + new
  `compare_winner_as_str_matches_published_strings` /
  `compare_summarize_declares_v1_winner_when_v1_positive` /
  `compare_summarize_declares_v2_winner_when_v2_positive` /
  `compare_summarize_declares_tie_on_zero_delta` /
  `compare_summarize_v1_plus_v2_deltas_net_to_zero` /
  `compare_to_json_contains_every_field` lib tests),
  `crates/autotrain/src/mode.rs` (new `Mode::Compare` arm
  + `--compare` argv handling + the v1 / v2 comparison
  call into `bench::run_compare`),
  `crates/autotrain/tests/compare.rs` (new
  `compare_run_emits_parseable_json_with_consistent_accounting`
  integration test gated on `database` + `DATABASE_URL`
  like the existing `bench.rs` integration test — drives
  `trainer --reset` then `trainer --compare` end-to-end
  and asserts the JSON line parses, the headline
  accounting is internally consistent
  (`v1.mbb_per_100 + v2.mbb_per_100 ≈ 0` within
  `1e-3` because the heads-up room nets to zero by
  construction: v1's chips come from v2's chip losses
  and vice versa, so the per-hand deltas sum to zero,
  `winner` ∈ `{"v1", "v2", "tie"}`, the `v1` and `v2`
  sub-reports each have non-zero `hands` and the same
  `hands` count), and the post-reset `blueprint_trained`
  flag is `false` for both sides),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md` (mark the v6
  candidate as shipped with a one-line note). Scope
  boundary: do NOT touch the existing `Mode::Bench` /
  `BenchReport` / `--bench` code path. Do NOT introduce
  a third `Blueprint` variant. Do NOT change the seat-0
  / seat-1 dispatch in `bench::run_hands`. Do NOT
  introduce a new trained config; the compare reuses the
  v1 + v2 trained configs the v1 + v2 `trainer --bench`
  paths already hydrate. Do NOT change the room
  protocol, the `Schema` contracts, the autotrain
  pipeline, the K-means cluster counts, the
  `CFR_TREE_COUNT_NLHE` baseline, the
  v1 / v2 / v3 / v4 named baselines, or the
  `trainer --replay` CLI. The compare is *structurally
  parallel* to the bench (one v1 + one v2 player, one
  `Room` shell, one JSON report) so a `trainer --bench`
  run and a `trainer --compare` run can coexist in the
  same database without colliding on the v1 / v2
  staging tables, the v1 / v2 blueprint tables, or the
  v1 / v2 epoch rows. Verification commands:
  `cargo test -p rbp-autotrain --features database --tests --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`.
  Required tests: the new lib tests in `bench.rs::tests`
  + the new `crates/autotrain/tests/compare.rs`
  integration test; no padding of unrelated suites.
  Dependencies: `STW-010` (the bench harness + `Room`
  shell the compare reuses) and `STW-017` (the v2
  trained config + v2 `DatabasePlayer2` + v2
  `Blueprint::V2` env-var dispatch the compare seats at
  the mirrored seat). Estimated scope: M. Completion
  signal: `trainer --compare` exits 0 on a freshly-`--reset`
  DB and prints a parseable single-line JSON
  `CompareReport` whose `winner` field is one of
  `{"v1", "v2", "tie"}`; the integration test passes;
  `cargo test --workspace` and `cargo fmt --check` are
  green.

- [x] `STW-017` Second trained config (`Flagship2`):
  `DiscountedRegret` + `QuadraticWeight` + `PluribusSampling`
  vs the v1 `Flagship` (`PluribusRegret` + `LinearWeight` +
  `PluribusSampling`). A new `Blueprint::V1` / `Blueprint::V2`
  enum lets a single `trainer --bench` invocation compare
  the two trained configs head-to-head against the same
  named baseline. The v2 has its own persistence pair
  (`BLUEPRINT2` / `EPOCH2` / `'current_v2'` / `STAGING2`),
  its own trainer (`trainer --fast2` running
  `Fast2Session`), its own bench seat
  (`RBP_BENCH_BLUEPRINT=v2` seats a `DatabasePlayer2` at
  seat 0 and reports `"blueprint":"v2"` in the JSON), and
  its own status read (`trainer --status` prints both v1
  and v2 epoch + blueprint row counts). A v1 `Mode::reset`
  does not zero the v2 `'current_v2'` row and vice versa.
  Owner files: `crates/database/src/lib.rs` (v2
  constants `BLUEPRINT2` / `EPOCH2` / `EPOCH2_KEY` /
  `STAGING2`),
  `crates/database/src/check2.rs` (new `Check2` trait +
  `Client` / `Arc<Client>` impls reading v2 epoch /
  blueprint counts), `crates/database/src/stage2.rs` (new
  `Stage2` trait + `Client` / `Arc<Client>` impls for
  v2 `stage2` / `merge2` / `stamp2(epochs)` with
  `'current_v2'` row scoping),
  `crates/nlhe/src/lib.rs` (v2 `Flagship2` type alias
  + `mod profile_v2`),
  `crates/nlhe/src/profile_v2.rs` (new
  `NlheProfileV2(NlheProfile)` newtype + `Schema` /
  `BulkSchema` / `Hydrate` impls targeting
  `BLUEPRINT2`, `EpochMetaV2` + `Schema` / `BulkSchema`
  impls targeting `EPOCH2` with the `'current_v2'`
  row seeded in `creates()`, plus
  `hydrate_flagship2(client) -> Flagship2` free function
  that wraps the v1 `NlheProfile` in-memory shape
  verbatim),
  `crates/gameroom/src/players/database_v2.rs` (new
  `DatabasePlayer2` player + `from_database` /
  `new` constructors, the v1 / v2 `decide` paths share
  the same `abstraction` → `NlheInfo` → `averaged_distribution`
  → weighted-sample recipe),
  `crates/gameroom/src/players/mod.rs`
  (re-export `DatabasePlayer2`),
  `crates/autotrain/src/pretraining.rs`
  (bootstrap the v2 `BLUEPRINT2` / `EPOCH2` tables in
  `PreTraining::run` so a fresh DB doesn't crash on
  the first `Fast2Session::sync`),
  `crates/autotrain/src/lib.rs` (re-export
  `Fast2Session`),
  `crates/autotrain/src/fast2.rs` (new `Fast2Session`
  parallel of v1 `FastSession` — same `step` / `epoch` /
  `checkpoint` / `summary` delegation, same shape, but
  the v2 `sync` writes `staging_v2` / `BLUEPRINT2` /
  `'current_v2'` instead of the v1 trio),
  `crates/autotrain/src/mode.rs` (new `--fast2` mode +
  v2 epoch / blueprint status read in `Mode::Status` +
  v2 arm in `Mode::reset` that zeros the `'current_v2'`
  row only),
  `crates/autotrain/src/bench.rs` (new `Blueprint::V1` /
  `Blueprint::V2` enum + `as_str` / `from_env` /
  `DEFAULT_BENCH_BLUEPRINT` + `Check2` import for the v2
  row-count read + the new `BenchReport.blueprint:
  Blueprint` field + the new `"blueprint":"{v1|v2}"`
  JSON field + a v1 / v2 seat-0 dispatch in `run_hands`
  that mirrors the existing
  `blueprint_trained` / empty-blueprint fallback shape +
  four new lib tests
  `blueprint_as_str_matches_published_strings` /
  `blueprint_from_env_round_trip` /
  `default_bench_blueprint_is_v1` /
  `summarize_stamps_blueprint_into_report` + a
  `"blueprint":"v1"` assertion in the
  `to_json_contains_every_field` contract test +
  the new `"blueprint"` field in `bench complete: ...`
  log line),
  `crates/autotrain/tests/bench.rs` (extended
  `parse_bench_line` to read the new `blueprint` field
  with a v1 default fallback for pre-STW-017 binary
  output + a v1-default assertion in the existing
  `bench_run_emits_parseable_json_with_consistent_accounting`
  test + a new
  `bench_run_v2_blueprint_round_trips_through_json`
  integration test that drives
  `trainer --bench` with `RBP_BENCH_BLUEPRINT=v2` and
  asserts the JSON `blueprint` field is `"v2"`),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md` (mark
  the v5 "second trained config" line as shipped).
  Scope boundary: do NOT touch the v1
  `NlheProfile` / `EpochMeta` schema contracts; the v2
  path is a new newtype + new table pair. Do NOT change
  the v1 `FastSession` / `Mode::Fast` / `Mode::Reset` v1
  arm / `BenchReport.baseline` / `Baseline` /
  `FastSession::sync` shape. Do NOT touch the autotrain
  pipeline, the K-means cluster counts, the
  `CFR_TREE_COUNT_NLHE` baseline, or the v1 / v2 / v3 /
  v4 named baselines (`Fish` / `EquityBot` /
  `PreflopBot` / `BlufferBot`). Do NOT add a new
  `Player` trait, do NOT introduce a new solver, do NOT
  change the room protocol. The v2 path is
  *structurally parallel* to the v1 path — separate
  tables, separate trainer, separate bench seat — so a
  v1 `trainer --fast` run and a v2 `trainer --fast2` run
  can coexist in the same database without colliding
  on the staging table or the epoch row.
  Acceptance criteria:
  (a) `crates/database/src/lib.rs` adds
      `BLUEPRINT2 = "blueprint_v2"`,
      `EPOCH2 = "epoch_v2"`,
      `EPOCH2_KEY = "current_v2"`, and
      `STAGING2 = "staging_v2"` constants.
  (b) `crates/database/src/check2.rs` is a new
      module with a `Check2` trait exposing
      `epochs_v2() -> usize` /
      `blueprint_v2() -> usize` /
      `status_v2()` (the latter prints a
      comma-formatted v2 epoch + blueprint table the
      same way `Check::status` does for v1), plus
      `Client` + `Arc<Client>` impls, plus a
      `epochs_v2_sql_targets_current_v2_row` lib test
      pinning `EPOCH2_KEY == "current_v2"`.
  (c) `crates/database/src/stage2.rs` is a new
      module with a `Stage2` trait exposing
      `stage2()` (recreates the v2
      `UNLOGGED staging_v2` table from `blueprint_v2`)
      / `merge2()` (upserts staging_v2 → blueprint_v2
      + drops staging_v2) / `stamp2(n)` (adds n to
      the `'current_v2'` row of `epoch_v2`), plus
      `Client` + `Arc<Client>` impls, plus two
      lib tests
      (`stage2_trait_is_object_safe_via_arc`,
      `stage2_stamp_targets_current_v2_row`).
  (d) `crates/nlhe/src/profile_v2.rs` is a new
      module with a
      `pub struct NlheProfileV2(pub NlheProfile)`
      newtype + `Schema` impl targeting
      `BLUEPRINT2` (creates / indices / truncates /
      freeze) + `BulkSchema` impl targeting
      `BLUEPRINT2` (columns + copy) + `Hydrate` impl
      reading `BLUEPRINT2` + `EPOCH2` rows + a
      `pub struct EpochMetaV2` with `Schema` impl
      targeting `EPOCH2` (creates seeds the
      `'current_v2'` row to 0 in an
      `ON CONFLICT DO NOTHING` block; truncates
      zeros the `'current_v2'` value; freeze pins
      `fillfactor = 100` but keeps autovacuum
      enabled for the UPDATE-heavy epoch table) +
      `BulkSchema` impl on `EpochMetaV2` + a
      `hydrate_flagship2(client) -> Flagship2`
      free function (the v2 hydration is a
      type-specific function, not a blanket
      `impl Hydrate for NlheSolver`, because the
      blanket impl would read the v1 `BLUEPRINT` /
      `EPOCH` tables). Nine lib tests pin the
      schema contracts
      (`nlhe_profile_v2_name_matches_const_table_name` /
      `nlhe_profile_v2_creates_targets_v2_table` /
      `nlhe_profile_v2_truncates_targets_v2_table` /
      `nlhe_profile_v2_copy_targets_v2_table_with_matching_arity` /
      `nlhe_profile_v2_freeze_disables_autovacuum` /
      `epoch_meta_v2_name_matches_const_table_name` /
      `epoch_meta_v2_creates_seeds_current_v2_row` /
      `epoch_meta_v2_truncates_zeros_current_v2_value` /
      `epoch_meta_v2_freeze_keeps_autovacuum_enabled`).
  (e) `crates/nlhe/src/lib.rs` adds a
      `mod profile_v2` (gated on
      `#[cfg(feature = "database")]`) + a
      `pub type Flagship2 = NlheSolver<DiscountedRegret, QuadraticWeight, PluribusSampling>`
      alias.
  (f) `crates/gameroom/src/players/database_v2.rs`
      is a new module with a
      `pub struct DatabasePlayer2(&'static Flagship2)`
      + `new(blueprint: &'static Flagship2) -> Self`
      constructor (the bench's empty-blueprint
      fallback path) + a
      `from_database(client: Arc<Client>) -> Self`
      constructor that hydrates from `BLUEPRINT2` /
      `EPOCH2` via `hydrate_flagship2` + a
      `Player::decide` impl that mirrors the v1
      path (`abstraction` → `NlheInfo` →
      `averaged_distribution` → weighted-sample
      with uniform `game.legal()` fallback). Two
      lib tests pin the public API
      (`new_wraps_blueprint`,
      `from_database_signature_is_stable`).
  (g) `crates/autotrain/src/pretraining.rs`
      extends `PreTraining::run` to call
      `Self::ensure::<NlheProfileV2>(client)` and
      `Self::ensure::<EpochMetaV2>(client)` after
      the v1 bootstrap, so a fresh DB has the v2
      tables before the first
      `Fast2Session::sync` and the first
      `Mode::Reset` v2 arm.
  (h) `crates/autotrain/src/fast2.rs` is a new
      module with
      `pub struct Fast2Session { client, solver: Flagship2 }`
      + a `Trainer` impl that mirrors the v1
      `FastSession` (step / epoch / checkpoint /
      summary delegate to `self.solver`; sync
      consumes the session, drops
      `client.stage2().await` → builds a
      `BinaryCopyInWriter` against
      `NlheProfileV2::copy()` and
      `NlheProfileV2::columns()` → writes the
      v2 in-memory rows → `client.merge2().await`
      → `client.stamp2(epochs).await`).
  (i) `crates/autotrain/src/mode.rs` adds a
      `Mode::Fast2` variant (parsed from
      `--fast2`) + extends `from_args` /
      `Usage: trainer --status | --cluster |
      --fast | --fast2 | --slow | --reset |
      --smoke | --bench | --replay <path>` +
      `Mode::run` adds the `Self::Fast2 =>
      Fast2Session::new(client).await.train().await`
      arm + `Mode::Status` now calls both
      `client.status().await` and
      `client.status_v2().await` so a
      `--status` run reports both v1 and v2
      epoch + blueprint row counts +
      `Mode::reset` zeroes the v2
      `'current_v2'` row in addition to the v1
      `'current'` row.
  (j) `crates/autotrain/src/bench.rs` adds a
      `Blueprint` enum
      (`V1` / `V2`) + `Blueprint::as_str` /
      `Blueprint::from_env` /
      `DEFAULT_BENCH_BLUEPRINT = Blueprint::V1` +
      a `blueprint: Blueprint` field on
      `BenchReport` + a
      `"blueprint":"{blueprint}"` JSON field in
      `to_json` (between
      `blueprint_trained` and `baseline`) + a
      `blueprint` parameter on `summarize` and
      `run_hands` + a v1 / v2 seat-0 dispatch in
      `run_hands` (v1 seats a `DatabasePlayer` /
      v2 seats a `DatabasePlayer2`, both with
      the same trained / empty-blueprint fallback
      shape) + a v1 / v2 row-count read in `run`
      (v1 uses `client.blueprint()` / v2 uses
      `client.blueprint_v2()`) + a `blueprint`
      field in the `bench complete: ...` log
      line + four new lib tests
      (`blueprint_as_str_matches_published_strings` /
      `blueprint_from_env_round_trip` /
      `default_bench_blueprint_is_v1` /
      `summarize_stamps_blueprint_into_report`) +
      a `"blueprint":"v1"` needle in
      `to_json_contains_every_field`. The
      `bench` `from_env` falls back to
      `DEFAULT_BENCH_BLUEPRINT` on a missing
      or unknown `RBP_BENCH_BLUEPRINT` so a
      stale env var doesn't crash the bench.
  (k) `crates/autotrain/tests/bench.rs`
      extends `parse_bench_line` to read the
      new `blueprint` field (with a
      `"v1".to_string()` fallback for
      pre-STW-017 binary output) + a
      `parsed.blueprint == "v1"` assertion in
      the existing
      `bench_run_emits_parseable_json_with_consistent_accounting`
      test + a new
      `bench_run_v2_blueprint_round_trips_through_json`
      integration test that runs
      `trainer --bench` with
      `RBP_BENCH_BLUEPRINT=v2` and asserts
      the JSON `blueprint` field is `"v2"`,
      the `hands` / `blind` fields round-trip
      from the env vars, and the default
      `baseline` is still `"fish"`. Both
      integration tests are
      `#[cfg(feature = "database")]`-gated
      AND short-circuit on a missing
      `DATABASE_URL` (same pattern as the
      existing STW-010 / STW-012 / STW-013
      bench tests).
  Required tests: 9 new lib tests in
  `crates/database/src/{check2,stage2}.rs` (2
  total), 9 new lib tests in
  `crates/nlhe/src/profile_v2.rs::hydrate_tests`
  + 2 new lib tests in
  `crates/gameroom/src/players/database_v2.rs::tests`
  + 4 new lib tests in
  `crates/autotrain/src/bench.rs::tests` (the
  four `blueprint_*` / `default_bench_blueprint_*`
  / `summarize_stamps_blueprint_*` tests) + 1
  new integration test in
  `crates/autotrain/tests/bench.rs`
  (`bench_run_v2_blueprint_round_trips_through_json`).
  Total new tests: 25 (19 unit + 1 integration
  + 5 contract-extending). The contract-extending
  tests are the `to_json_contains_every_field`
  needle assertion, the v1 default assertion in
  the existing bench integration test, and the
  three `bench::tests::baseline_*` tests
  threading the new `Blueprint` arg through
  their `summarize(&per_hand, blind, Baseline,
  Blueprint)` calls (no new test, but each
  test now also pins the v1 `blueprint` JSON
  field — see (j) and (k)).
  Dependencies: STW-006 (Schema/BulkSchema
  split) for the v1 `Schema::creates()` paths
  the v2 tables parallel; STW-008 (hand
  persistence round-trip) for the
  `HistoryRepository` plumbing that the
  `DatabasePlayer2` constructor (gated on
  `feature = "database"`) uses; STW-010 (bench
  harness) for the seat-0 dispatch the v2
  `Blueprint` enum hooks into.
  Estimated scope: L.
  Completion signal: `cargo check --workspace
  --all-features`, `cargo test --workspace`,
  `cargo fmt --check` all pass; the 25 new
  tests land green; the `trainer --status`
  call prints both v1 and v2 epoch + blueprint
  row counts on the same v1 + v2 row pair; a
  `trainer --bench` call with
  `RBP_BENCH_BLUEPRINT=v2` prints a JSON
  `BenchReport` with `blueprint:"v2"`; a
  `trainer --bench` call with
  `RBP_BENCH_BLUEPRINT` unset prints a JSON
  `BenchReport` with `blueprint:"v1"`; a
  `trainer --reset` call zeros the v1
  `'current'` row only (the v2 `'current_v2'`
  row is untouched).

- [x] `STW-016` `trainer --replay <path>` CLI: wire the
  STW-015 public `Transcript` surface into the `trainer`
  binary so a downstream tool (a dashboard's "replay"
  button, a CI check, a README quickstart) can take a
  `transcript-<hand_id>.json` produced by the bench and
  re-derive the `(Position, Action)` sequence + a
  renderable text summary without a database connection
  or a `cargo run` against a sister crate. STW-015
  deliberately shipped the *public API* (`read_from_path`,
  `rebuild_action_sequence`, `replay_to_path`) and
  deferred the CLI wiring as "the next slice if a
  `trainer --replay` is needed"; this slice lands the
  CLI wiring. The new mode is read-only (no DB
  connection, no schema), accepts exactly one positional
  argument (the path to a `transcript-*.json` file the
  bench wrote), prints the rendered text to stdout, and
  exits non-zero on a missing/corrupt/unreadable file
  with a one-line diagnostic so a CI check or a
  dashboard can surface the failure without parsing
  arbitrary error text. Do NOT change the bench writer,
  do NOT change the `Transcript` shape, do NOT change
  any `Schema` contract, do NOT change the `trainer`
  flags, do NOT introduce a new binary. The new mode
  reuses `Transcript::replay_to_path` verbatim — the
  entire slice is a `Mode::Replay` variant + a
  one-arg-from-argv parser + a print-to-stdout + an
  exit-code mapping.
  Owner files: `crates/autotrain/src/mode.rs` (add
  `Mode::Replay`, extend `from_args` to parse
  `--replay <path>` *and* a non-flag positional
  fallback so the README quickstart can be
  `trainer --replay transcripts/transcript-abc.json`
  or `trainer transcripts/transcript-abc.json`),
  `crates/autotrain/src/replay.rs` (new — a thin
  `replay::run(&Path) -> Result<String, String>`
  wrapper over
  `rbp_gameroom::records::Transcript::replay_to_path`),
  `IMPLEMENTATION_PLAN.md`,
  `genesis/plans/000-ceo-testnet-roadmap.md` (mark the
  `trainer --replay <path>` line as shipped).
  Scope boundary: add a single `Mode::Replay` variant
  that owns `path: PathBuf`, parse it from
  `--replay <path>` or a lone positional arg, and
  dispatch to a `replay::run(&path)` helper that calls
  `rbp_gameroom::records::Transcript::replay_to_path`
  and prints the returned `String` to stdout. On
  error, print the returned `Err(String)` to stderr
  and exit 2 (matching the smoke mode's "data-quality
  problem is a non-zero exit" convention). The parser
  must reject `trainer --replay` with no path (prints
  a one-line usage to stderr, exits 2). Do NOT add a
  clap / structopt dep — the existing trainer uses a
  hand-rolled `from_args` so the new mode follows the
  same shape. Do NOT change the `Mode` variants for
  `--status` / `--cluster` / `--fast` / `--slow` /
  `--reset` / `--smoke` / `--bench`. Do NOT touch the
  bench writer, the transcript shape, the `Schema`
  contracts, the `HistoryRepository` API, or the room
  protocol. The new mode is a pure consumer of the
  STW-015 public surface; if the surface changes, the
  handler is a one-line update.
  Acceptance criteria:
  (a) `crates/autotrain/src/mode.rs` adds a
      `Mode::Replay { path: PathBuf }` variant.
      `from_args` parses `--replay <path>` AND a
      lone positional arg (so `trainer
      transcripts/transcript-abc.json` works as
      a shortcut). `trainer --replay` with no
      path prints `Usage: trainer --replay <path>`
      to stderr and returns `Self::Replay` with an
      empty path (the handler then fails fast).
      `trainer --status` / `--cluster` / `--fast` /
      `--slow` / `--reset` / `--smoke` / `--bench`
      parse paths are unchanged.
  (b) `crates/autotrain/src/mode.rs::run` adds a
      `Self::Replay { path }` arm that calls
      `replay::run(&path)` and propagates the exit
      code (0 on success, 2 on a missing/corrupt
      file or a missing path arg). The DB
      connection is *not* opened for this mode.
  (c) `crates/autotrain/src/replay.rs` is a new
      module with a public
      `replay::run(path: &Path) -> Result<String,
      String>` helper that wraps
      `Transcript::replay_to_path` and returns
      the rendered text. The handler in (b) prints
      the `Ok` string to stdout and the `Err`
      string to stderr.
  (d) `crates/autotrain/src/replay.rs::tests` adds
      three lib tests: `replay_run_renders_fixture_transcript`,
      `replay_run_errors_on_missing_file`,
      `replay_run_errors_on_corrupt_file`.
  (e) `bin/trainer/src/main.rs` is unchanged
      (the `Mode::run()` entry point is the
      existing dispatch surface; the new variant
      is reached through it).
  (f) `genesis/plans/000-ceo-testnet-roadmap.md`
      gets a one-line note marking the
      `trainer --replay <path>` (or equivalent
      downstream tool) line as shipped (STW-016).
  Required tests: the three new lib tests in
  (d); no padding of unrelated suites.
  Dependencies: `STW-015` for the
  `Transcript::replay_to_path` public surface
  this slice wraps. `STW-014` for the bench
  writer that produces the `transcript-*.json`
  files the new mode consumes.
  Estimated scope: S.
  Completion signal: `cargo test -p
  rbp-autotrain --tests --lib` is green
  with the three new lib tests, a hand-written
  `trainer --replay transcripts/transcript-abc.json`
  invocation against a fixture file prints
  the rendered text to stdout and exits 0,
  and `trainer --replay /no/such/file` exits
  2 with a one-line diagnostic on stderr.

- [x] `STW-003` Restore database-backed server/gameroom build.
  Owner files: `crates/gameroom/Cargo.toml`, `crates/gameroom/src/players/database.rs`,
  `crates/gameroom/src/players/realtime.rs`, `crates/gameroom/src/players/zerotemp.rs`,
  `crates/server/Cargo.toml`, `bin/backend/Cargo.toml`.
  Scope boundary: make `cargo check -p rbp-server`, `cargo check -p rbp-gameroom --features database`,
  and `cargo check -p backend` compile without weakening database-backed player behavior;
  do not redesign room protocol or training.
  Acceptance criteria: the database-backed players construct a `Flagship` through a real
  `Hydrate` impl that is selected by the same feature chain as the gameroom database feature;
  no `#[allow(dead_code)]` or feature-flag disablement used to silence the failure.
  Verification commands:
  `cargo check -p rbp-server`,
  `cargo check -p rbp-gameroom --features database`,
  `cargo check -p backend`,
  `cargo check --workspace`,
  `cargo test --workspace -- --test-threads=4`.
  Required tests: existing workspace tests; the new chain must not break `cargo check --workspace`.
  Dependencies: none.
  Estimated scope: S.
  Completion signal: feature-specific server and gameroom checks are green.

- [x] `STW-004` Harden auth secrets and session validation.
  Owner files: `crates/auth/src/crypto.rs`, `crates/auth/src/handlers.rs`,
  `crates/auth/src/middleware.rs`, `crates/auth/src/repository.rs`,
  `crates/auth/src/session.rs`, `crates/auth/Cargo.toml`.
  Scope boundary: validate `JWT_SECRET`, align stored session token hash with middleware
  checks, and add request-level tests; do not introduce OAuth, cookies, refresh tokens, or
  rate limiting in this slice.
  Acceptance criteria: missing or empty `JWT_SECRET` cannot silently create production
  tokens; login/register store a verifiable token/session binding; authenticated extractors
  reject revoked, expired, missing, malformed, and mismatched tokens.
  Verification commands:
  `cargo test -p rbp-auth --features server`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check -p rbp-server` (after `STW-003`).
  Required tests: positive login/me flow, missing-secret behavior, invalid token, revoked
  session, token/session mismatch.
  Dependencies: `STW-003` only for full server check, not for `rbp-auth` crate tests.
  Estimated scope: M.
  Completion signal: `rbp-auth --features server` has meaningful passing tests and no
  empty-secret fallback remains.

- [x] `STW-005` Restore workspace formatter gate.
  Owner files: every file reported by `cargo fmt --check` (auth, autotrain, cards,
  gameplay, gameroom, mccfr, nlhe, rbp, server).
  Scope boundary: run `cargo fmt` and commit only mechanical formatting changes; no
  behavior changes; do not reformat generated or vendored code.
  Acceptance criteria: `cargo fmt --check` exits 0 after the change.
  Verification commands:
  `cargo fmt --check`,
  `cargo test --workspace -- --test-threads=4`.
  Required tests: existing workspace tests only.
  Dependencies: none.
  Estimated scope: S.
  Completion signal: formatter gate is green and the diff is purely mechanical.

- [x] `STW-006` Classify and fix schema panic contracts.
  Owner files: `crates/autotrain/src/epoch.rs`, `crates/auth/src/member.rs`,
  `crates/auth/src/session.rs`, `crates/gameroom/src/room.rs`,
  `crates/gameroom/src/records/*.rs`, `crates/clustering/src/{future,lookup,metric}.rs`,
  `crates/database/src/{traits,schema}.rs`, `crates/nlhe/src/profile.rs`.
  Resolution: the historical `unimplemented!()` bodies on `Schema::copy` /
  `Schema::columns` for derived types (`Street`, `Abstraction`) were never
  meaningful — those tables are populated by `INSERT`, not binary `COPY`.
  The structural fix splits `Schema` (DDL-only) into `Schema` + `BulkSchema`
  (DDL + binary-COPY), and tightens the `Streamable` bound to `BulkSchema +
  Sized + Send` so a misuse that hands a derived type to `Streamable::stream`
  is now a compile-time error, not a runtime panic. Every implementor
  (`Room`, `Member`, `Session`, `Hand`, `Play`, `Participant`, `EpochMeta`,
  `Future`, `Lookup`, `Metric`, `NlheProfile`) now provides real `copy` /
  `columns` bodies on its `BulkSchema` impl, and the `Schema` DDL
  methods (`name`, `creates`, `indices`, `truncates`, `freeze`) are kept
  on every persisted table.
  Acceptance criteria: `cargo check --workspace`,
  `cargo check -p rbp-clustering --features database`,
  `cargo check -p rbp-gameroom --features database`,
  `cargo check -p rbp-auth --features server,database`,
  `cargo check -p rbp --features database`, `cargo test --workspace`,
  `cargo fmt --check` all pass; no `unimplemented!()` body remains on any
  `Schema` or `BulkSchema` method for a derived or persisted table.
  Verification commands: same as above.
  Required tests: existing workspace tests (now 384 passing).
  Dependencies: none.
  Estimated scope: M.
  Completion signal: the derived/Streamable misuse class is structurally
  impossible.

- [x] `STW-008` End-to-end hand persistence round-trip test.
  Owner files: `crates/gameroom/tests/hand_roundtrip.rs`,
  `IMPLEMENTATION_PLAN.md`, `genesis/plans/000-ceo-testnet-roadmap.md`.
  Scope boundary: prove the `HandContext` → `Hand` / `Participant` / `Play`
  conversion used by `Room::flush_hand` is lossless, that the
  `HistoryRepository` round-trip on a live Postgres preserves every
  persisted field, and that driving a real `Room` end-to-end with two
  `Fish` players writes the expected rows. Do not redesign the room
  protocol, do not introduce a new `Replay` type, do not change any
  `Schema` method bodies.
  Acceptance criteria: a new `crates/gameroom/tests/hand_roundtrip.rs`
  exists with four passing tests:
  (a) `hand_persists_action_sequence_losslessly` — `HandContext` →
      `Hand` / `Participant` / `Play` conversion preserves every field
      `Room::flush_hand` would persist.
  (b) `records_replay_to_terminal_state` — the rebuilt `(Position,
      Action)` list, when applied through a fresh `Game::root()`,
      reconstructs the source observable (pot, stacks, dealer) and
      the action sequence byte-for-byte.
  (c) `db_round_trip_preserves_hand` — the same records written
      through `HistoryRepository::create_hand / create_player /
      create_action` (the exact path `Room::flush_hand` uses) and
      read back through `get_hand / get_players / get_actions`
      round-trip identically. This test is `#[cfg(feature =
      "database")]`-gated AND short-circuits on a missing
      `DATABASE_URL` (following the `crates/auth/tests/server_flow.rs`
      pattern), so CI without Postgres stays green.
  (d) `room_with_two_fish_persists_one_hand` — drive a real `Room`
      end-to-end (two `Fish` players seated, `start` signal sent,
      wait for `done`), then read the persisted `Hand` / participants
      / actions back through `HistoryRepository` and assert the
      row count and the participant list match the room. Gated on
      `database` + `DATABASE_URL` like (c).
  The fixture in (a)/(b) drives a known hand
  (`Call(S_BLIND) / Check / Check x 6` — preflop limp, every street
  checked down) so the expected action sequence and the rebuilt
  observable are both deterministic and asserted inline.
  Verification commands:
  `cargo test -p rbp-gameroom --tests --lib`,
  `cargo test -p rbp-gameroom --features database --tests --lib`,
  `cargo test --workspace`,
  `cargo fmt --check`.
  Required tests: the four tests above; they are the only tests
  this slice adds (no padding of unrelated suites).
  Dependencies: `STW-006` (must have split `Schema` / `BulkSchema`
  so the `Schema` impls on `Hand` / `Participant` / `Play` are
  in their final shape); the live-DB tests assume the persistence
  tables are reachable and follow the same `DATABASE_URL` /
  `Schema::creates()` setup the auth server-flow tests use.
  Estimated scope: M.
  Completion signal: the four tests pass; the round-trip proof
  exercises both the in-memory conversion and the live Postgres
  path that `Room::flush_hand` actually runs in production.

- [x] `STW-009` Trainer smoke path: env-gated small config that
  clusters + trains + syncs a non-empty blueprint.
  Owner files: `bin/trainer/src/main.rs`,
  `crates/autotrain/src/{fast,mode,trainer}.rs`,
  `crates/nlhe/src/solver.rs`,
  `crates/autotrain/tests/smoke.rs` (new),
  `IMPLEMENTATION_PLAN.md`, `genesis/plans/000-ceo-testnet-roadmap.md`.
  Scope boundary: make `trainer --smoke` a one-shot pipeline that
  (1) honors env-gated knobs to keep the run short, (2) drives
  pretraining + N training epochs + sync, (3) prints
  `trainer --status`-style output, and (4) exits non-zero on an
  empty blueprint or any pre-existing clustering error. Do NOT
  redesign the autotrain pipeline, do NOT change the
  K-means cluster counts (the `Layer<K, N>` const-generic), do
  NOT touch the `CFR_TREE_COUNT_NLHE` baseline. Do NOT add a new
  Mode if the existing `--fast` path can be re-used.
  Acceptance criteria:
  (a) `crates/autotrain/src/trainer.rs` — the `Trainer::train()`
      default loop honors `RBP_FAST_EPOCHS` (positive integer
      env var) and stops after that many `step()` calls; a
      missing var keeps the existing `interrupted()` behavior.
  (b) `crates/nlhe/src/solver.rs` — `NlheSolver::batch_size()`
      honors `RBP_FAST_BATCH` (positive integer env var, default
      1000); a missing var keeps the production batch size.
  (c) `bin/trainer/src/main.rs` — a new `--smoke` mode runs
      `pretraining + train(epochs=RBP_FAST_EPOCHS) + sync +
      status` and exits non-zero (a) if the post-sync blueprint
      row count is 0, or (b) if pretraining was skipped
      (a clustering error message must precede the exit).
      A stdout line `smoke complete: epochs=N rows=M` is
      emitted on success.
  (d) `crates/autotrain/tests/smoke.rs` — a new integration
      test runs the `train --smoke` end-to-end against a live
      Postgres with `RBP_FAST_EPOCHS=2 RBP_FAST_BATCH=16`,
      asserts (i) the binary exits 0, (ii) the printed
      `rows=` value is `> 0`, and (iii) a follow-up
      `train --status` call reports `Epoch > 0` and
      `Blueprint > 0`. The test is `#[cfg(feature =
      "database")]`-gated AND short-circuits on a missing
      `DATABASE_URL` (same pattern as
      `crates/gameroom/tests/hand_roundtrip.rs` and
      `crates/auth/tests/server_flow.rs`).
  Verification commands:
  `cargo test -p rbp-autotrain --features database --tests --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`.
  Required tests: the integration test in (d); it is the only
  new test this slice adds.
  Dependencies: `STW-006` (Schema/BulkSchema split) for the
  pre-existing `Schema::creates()` and `Schema::freeze()` paths
  that the smoke pretraining + sync depend on.
  Estimated scope: M.
  Completion signal: `trainer --smoke` exits 0 with a non-empty
  blueprint row count under `RBP_FAST_EPOCHS=2`; the
  integration test passes; `cargo test --workspace` and
  `cargo fmt --check` are green.

- [x] `STW-010` Head-to-head bench harness for trained `DatabasePlayer` vs `Fish`.

- [x] `STW-012` v3 named baseline (`PreflopBot`): preflop-tier aware rule bot.

- [x] `STW-014` Replayable transcript bundle: land the `Transcript` data
  type, wire it into the bench harness, and prove the round-trip end-to-end
  against a real `Room` write.
  Owner files: `crates/gameroom/src/records/transcript.rs` (new),
  `crates/gameroom/src/records/mod.rs` (re-export),
  `crates/gameroom/tests/hand_roundtrip.rs` (extend with
  `transcript_json_round_trips`),
  `crates/autotrain/src/bench.rs` (write one `transcript-*.json` per
  bench hand into `RBP_BENCH_TRANSCRIPT_DIR`),
  `genesis/plans/000-ceo-testnet-roadmap.md` (mark the
  "Public reproducible benchmark surface" item as in-progress with
  STW-014),
  `IMPLEMENTATION_PLAN.md`.
  Scope boundary: the `Transcript` is a thin in-memory bundle of the
  already-persisted `Hand` / `Participant` / `Play` records plus a
  cheap `verify()` integrity check (orphan player + non-monotonic seq)
  and a `to_json()` serialiser. The bench harness writes one
  `transcript-<hand_id>.json` per hand under
  `RBP_BENCH_TRANSCRIPT_DIR` (default `./transcripts`); a downstream
  tool can replay the action sequence by reading one file. Do NOT
  redesign the `Schema` contracts, do NOT change the
  `HistoryRepository` API, do NOT add a new solver, do NOT change the
  room protocol, do NOT introduce a `Replay` v2 type.
  Acceptance criteria:
  (a) `crates/gameroom/src/records/transcript.rs` exists with the
      `Transcript` data type (a `Hand` + `Vec<Participant>` +
      `Vec<Play>` bundle), a `TranscriptError` enum with
      `OrphanPlayer { seq, member }` and `NonMonotonicSeq { seq }`
      variants, a `Transcript::new(...)` constructor, a
      `Transcript::verify() -> Result<(), TranscriptError>` integrity
      check, a `Transcript::to_json() -> String` serialiser, and
      `HandView` / `ParticipantView` / `PlayView` serialise
      adapters. The `Serialize` impl produces a flat
      `{"hand":{...},"participants":[...],"plays":[...]}` document.
  (b) `Transcript::verify` rejects an orphan player (a `Play::player`
      UUID not in the participant list) and a non-monotonic `seq`
      (`seq=0, seq=2` with the gap visible) and returns `Ok(())` for
      a consistent transcript.
  (c) `crates/gameroom/src/records/mod.rs` re-exports `Transcript` and
      `TranscriptError` so downstream callers can
      `use rbp_gameroom::records::Transcript`.
  (d) `crates/gameroom/src/records/transcript.rs::tests` includes
      the six unit tests: `verify_accepts_consistent_transcript`,
      `verify_detects_orphan_player`, `verify_detects_non_monotonic_seq`,
      `to_json_includes_hand_participants_and_plays`,
      `transcript_error_display_includes_seq_and_member`, and
      `action_u32_round_trip_preserves_variant`. They run under
      `cargo test -p rbp-gameroom` (no `database` feature required —
      they only touch the in-memory type).
  (e) `crates/gameroom/tests/hand_roundtrip.rs` adds a new
      `transcript_json_round_trips` integration test (gated on
      `database` + `DATABASE_URL` like the existing
      `room_with_two_fish_persists_one_hand`) that drives a real
      `Room` end-to-end, reads the persisted `Hand` /
      `Vec<Participant>` / `Vec<Play>` back through
      `HistoryRepository::get_hand / get_players / get_actions`,
      constructs a `Transcript::new(...)`, asserts
      `t.verify().is_ok()`, serialises to `t.to_json()`, parses the
      result back as `serde_json::Value`, and asserts (i) the JSON
      has `hand`, `participants`, `plays` top-level keys, (ii) the
      `hand.id` field matches the read-back hand id, (iii) the
      `participants` array length equals `N` (one per seat), and
      (iv) the `plays` array length equals the read-back action
      count.
  (f) `crates/autotrain/src/bench.rs` writes a `transcript-*.json`
      per hand under `RBP_BENCH_TRANSCRIPT_DIR` (default
      `./transcripts`). The bench creates the directory if it does
      not exist, and uses `HistoryRepository::get_hand / get_players
      / get_actions` to read back the records the `Room::flush_hand`
      just wrote. A `transcript` boolean field is added to
      `BenchReport` and stamped `true` iff the directory was
      non-empty after the run. The `RBP_BENCH_TRANSCRIPT_DIR=""` env
      value (or unset + `RBP_BENCH_TRANSCRIPT_DISABLE=1`) disables
      the writer for callers that do not want a directory side
      effect.
  (g) `crates/autotrain/src/bench.rs::tests` adds a
      `transcript_dir_default_and_env_override_round_trip` lib test
      that pins the env var contract (default `./transcripts`, env
      override honoured, empty value disables the writer) so a
      refactor that drops the env-var wiring fails before it lands.
  Verification commands:
  `cargo test -p rbp-gameroom --tests --lib`,
  `cargo test -p rbp-gameroom --features database --tests --lib`,
  `cargo test -p rbp-autotrain --features database --tests --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo check --workspace --all-features`,
  `cargo fmt --check`.
  Required tests: the six new lib tests in (d) + the new
  `transcript_json_round_trips` integration test in (e) + the
  `transcript_dir_default_and_env_override_round_trip` lib test
  in (g); no padding of unrelated suites.
  Dependencies: `STW-006` (Schema/BulkSchema split) for the
  `Schema::creates()` and `Schema::freeze()` paths the persistence
  reads depend on; `STW-008` (hand-persistence round-trip) for the
  `HistoryRepository::get_*` reads the bench writer and the
  integration test use; `STW-010` (bench harness) for the
  `trainer --bench` entry point the writer hooks into.
  Estimated scope: M.
  Completion signal: `cargo test --workspace` is green,
  `cargo fmt --check` is green, the `transcript_json_round_trips`
  integration test passes against a live Postgres (or is
  short-circuited by a missing `DATABASE_URL`), and a `trainer
  --bench` run on a freshly-`--reset` DB leaves at least one
  `transcript-<hand_id>.json` file under `./transcripts/` that
  re-parses as a JSON object with the expected `hand` /
  `participants` / `plays` keys.

- [x] `STW-013` v4 named baseline (`BlufferBot`): add a semi-bluff-aware
  rule bot that beats `PreflopBot` by raising on checked-to postflop
  boards with weak made hands / strong draws (the gap that `EquityBot`
  and `PreflopBot` both leave open: a passive threshold-only policy
  never wins the pot uncontested, so the trained bot never has to
  fold). Wire as `Baseline::Bluffer` so the bench harness can group
  reports by `baseline` and the v4 framing is honest (a stronger
  named baseline, not a "second trained config" — that would be a
  much larger slice).
  Owner files: `crates/gameroom/src/players/blufferbot.rs` (new),
  `crates/gameroom/src/players/mod.rs`,
  `crates/gameroom/src/players/preflopbot.rs` (re-use the
  `classify_pocket` tier table through a thin `super::` re-export
  path, *not* by duplicating the table),
  `crates/autotrain/src/bench.rs` (extend `Baseline` enum +
  `as_str` + `from_env` + seat-1 dispatch + add a `preflop_tier`
  re-use assertion),
  `crates/autotrain/tests/bench.rs` (extend the integration test
  for the v4 `bluffer` baseline),
  `IMPLEMENTATION_PLAN.md`,
  `genesis/plans/000-ceo-testnet-roadmap.md` (mark the "v4 stronger
  baseline" item as shipped).
  Scope boundary: add a v4 *named* rule-based baseline that
  (1) reuses the v3 `PreflopBot` preflop tier table verbatim
  (smallest legal raise / call / fold) and (2) on the flop, when
  the bot is *first to act* and `Check` is legal, raises with
  *either* an above-threshold made hand (≥ 0.65 equity, matching
  the v2 `EquityBot::choose` raise table) *or* a "bluff-eligible"
  weak hand (≤ 0.40 equity, ≤ 0.20 chance the bot improves to
  the nuts on a later street) at a fixed small raise size (the
  smallest legal raise), with the raise gated on a deterministic
  per-street frequency (e.g. 30% on the flop, 20% on the turn,
  0% on the river — the river has no fold equity, so a bluff
  loses money in expectation). The bot never bluffs *into* a
  bet (that is a `Call/Fold` decision, not a `Check/Raise`
  decision). Do NOT introduce a new solver, do NOT change
  `EquityBot` / `PreflopBot` / `Fish`, do NOT change the
  autotrain pipeline, do NOT add a new `Player` trait.
  Acceptance criteria:
  (a) `crates/gameroom/src/players/blufferbot.rs` exists with
      `BlufferBot` (a unit struct, `Default` impl, async
      `Player` impl) and a pure `BlufferBot::classify_bluff`
      helper that returns a `BluffDecision`
      (`RaiseSemiBluff | Check | NotBluffEligible`) based on
      the postflop equity, the street, and the
      "bluff-eligible" condition (equity ≤ 0.40 AND
      improvement ≤ 0.20).
  (b) The `Player::decide` impl:
      - on `Street::Pref` (no public board), delegates
        *verbatim* to `PreflopBot::decide_recall` so the
        v3 preflop tier table is defined in exactly one
        place;
      - on later streets, classifies the situation
        (`BluffDecision`) and acts:
        - `RaiseSemiBluff` → pick the smallest legal
          `Raise(_) | Shove(_)` (same sizing convention as
          `PreflopBot` Tier 1 preflop);
        - `Check` → take the free card;
        - `NotBluffEligible` → delegate to
          `EquityBot::choose` so the postflop value-bet
          threshold table stays the same as the v2 / v3
          baselines.
  (c) `crates/gameroom/src/players/mod.rs` exports
      `BlufferBot` and re-exports it from `rbp_gameroom`.
  (d) `crates/autotrain/src/bench.rs`:
      - adds `Baseline::Bluffer` to the `Baseline` enum,
      - extends `Baseline::as_str` with `"bluffer"`,
      - extends `Baseline::from_env` to parse `"bluffer"`,
      - wires `Baseline::Bluffer` into the `run_hands`
        match so seat 1 seats a `BlufferBot`,
      - stamps the variant into `BenchReport.baseline`
        (the existing `summarize` call already takes a
        `Baseline` argument),
      - extends the existing
        `baseline_as_str_round_trip` and
        `baseline_from_env_honours_env_var` lib tests
        with the new `Baseline::Bluffer` literals so a
        refactor that drops the variant from one of the
        three sites fails before it lands,
      - adds a `blufferbot_classify_bluff_eligible_when_weak`
        lib test that pins `BluffDecision::RaiseSemiBluff`
        for a flop with `eq=0.30, improve=0.10, street=Flop`
        and `BluffDecision::Check` for the same with
        `street=River` (river has 0% bluff frequency, so
        the decision is `Check` / `NotBluffEligible`),
      - adds a `blufferbot_preflop_reuses_preflopbot_tier_table`
        lib test that asserts a `BlufferBot::decide` call
        on a preflop `Partial` with the AA pocket from
        the existing `preflop_tier_starts_with_top_pair`
        test picks the smallest legal raise (the v3
        tier-table behaviour, not a v4-specific branch).
  (e) `crates/autotrain/tests/bench.rs` integration test
      (gated on `database` + `DATABASE_URL` like the
      existing STW-010 / STW-012 tests) extends the
      JSON parse to assert the `baseline` field is
      `"bluffer"` when run with
      `RBP_BENCH_BASELINE=bluffer`.
  (f) `genesis/plans/000-ceo-testnet-roadmap.md` gets a
      one-line note marking the "v4 stronger baseline
      (e.g. a second trained config)" item as shipped
      (STW-013) — the note explicitly says the v4 is
      a *named rule-based* baseline, not a second
      trained config (which the v4 framing as a
      "stronger named baseline" replaces), and points
      to a future "second trained config" as the next
      slice if a v5 is needed.
  Verification commands:
  `cargo test -p rbp-gameroom --tests --lib`,
  `cargo test -p rbp-gameroom --features database --tests --lib`,
  `cargo test -p rbp-autotrain --features database --tests --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`.
  Required tests: the new lib tests in (d) + the
  extended integration test in (e); no padding of
  unrelated suites.
  Dependencies: STW-011 (the v2 `EquityBot` postflop
  threshold table), STW-012 (the v3 `PreflopBot`
  preflop tier table that `BlufferBot` re-uses
  verbatim on the preflop street).
  Estimated scope: M.
  Completion signal: `trainer --bench` with
  `RBP_BENCH_BASELINE=bluffer` exits 0 on a
  freshly-`--reset` DB and prints a parseable
  single-line JSON `BenchReport` whose `baseline`
  field is `"bluffer"`; the new lib tests pass;
  `cargo test --workspace` and `cargo fmt --check`
  are green.
  Owner files: `crates/gameroom/src/players/preflopbot.rs` (new),
  `crates/gameroom/src/players/mod.rs`,
  `crates/autotrain/src/bench.rs` (extend `Baseline` enum + JSON),
  `crates/autotrain/tests/bench.rs` (extend integration test),
  `crates/gameroom/tests/hand_roundtrip.rs` (no change),
  `IMPLEMENTATION_PLAN.md`, `genesis/plans/000-ceo-testnet-roadmap.md`.
  Scope boundary: add a v3 *named* rule-based baseline that beats
  `EquityBot` (v2) on a hand-by-hand basis by using a real preflop
  hand-tier table (pairs 88+, broadways, suited connectors) on
  the preflop street, and falling through to the same Monte Carlo
  equity + threshold table as `EquityBot` on later streets. Wire
  it as a new `Baseline::Preflop` variant in the bench harness so a
  downstream scraper can group reports by `baseline`. Do NOT
  introduce a new solver, do NOT touch `EquityBot` or `Fish`, do
  NOT change the autotrain pipeline.
  Acceptance criteria:
  (a) `crates/gameroom/src/players/preflopbot.rs` exists with
      `PreflopBot` (a unit struct, `Default` impl, async `Player`
      impl) and a pure `PreflopBot::classify_pocket(pocket, blind)`
      helper that returns a `PreflopTier` enum
      (`Tier1Raise | Tier2Call | Tier3Fold`) based on the
      preflop hand-rank rules documented in the module.
  (b) The `Player::decide` impl:
      - on `Street::Pref` (no public board), calls
        `classify_pocket` and picks the highest-priority legal
        action matching the tier: Tier1 → prefer the *smallest*
        preflop raise (don't min-rely on Shove); Tier2 → call
        (or check if no bet); Tier3 → fold (or check if no bet);
      - on later streets, calls `Observation::simulate(256)` and
        delegates to the same `EquityBot::choose` threshold
        table.
  (c) `crates/gameroom/src/players/mod.rs` exports `PreflopBot`
      and re-exports it from `rbp_gameroom`.
  (d) `crates/autotrain/src/bench.rs`:
      - adds `Baseline::Preflop` to the `Baseline` enum,
      - extends `Baseline::as_str` with `"preflop"`,
      - extends `Baseline::from_env` to parse `"preflop"`,
      - wires `Baseline::Preflop` into the `run_hands` match so
        seat 1 seats a `PreflopBot`,
      - stamps the variant into `BenchReport.baseline` (the
        existing `summarize` call already takes a `Baseline`
        argument),
      - adds a `preflop_tier_starts_with_top_pair` lib test that
        pins `classify_pocket` for `Hand::of(Ace)+Hand::of(Ace)`,
        `Hand::of(7)+Hand::of(7)`, and
        `Hand::of(2)+Hand::of(7, off-suit)` so a refactor that
        drops a tier fails before it lands,
      - adds a `preflopbot_prefers_smallest_legal_raise` lib test
        that drives `PreflopBot::choose` with a known legal set
        and confirms the smallest preflop raise (not shove) is
        chosen for Tier1.
  (e) `crates/autotrain/tests/bench.rs` integration test
      (gated on `database` + `DATABASE_URL` like the existing
      STW-010 test) extends the JSON parse to assert the
      `baseline` field is `"preflop"` when run with
      `RBP_BENCH_BASELINE=preflop`.
  (f) `genesis/plans/000-ceo-testnet-roadmap.md` gets a one-line
      note marking the "stronger named baseline" item as shipped
      (STW-012) and pointing to the v3 (`PreflopBot`) as the
      next-iteration target if a v4 is needed.
  Verification commands:
  `cargo test -p rbp-gameroom --tests --lib`,
  `cargo test -p rbp-gameroom --features database --tests --lib`,
  `cargo test -p rbp-autotrain --features database --tests --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`.
  Required tests: the new `preflop_tier_starts_with_top_pair`
  and `preflopbot_prefers_smallest_legal_raise` lib tests in
  `bench.rs::tests` + the extended integration test in (e);
  no padding of unrelated suites.
  Dependencies: STW-011 (the v2 `EquityBot` baseline provides
  the postflop threshold table that `PreflopBot` re-uses for
  later streets).
  Estimated scope: S.
  Completion signal: `trainer --bench` with
  `RBP_BENCH_BASELINE=preflop` exits 0 on a freshly-`--reset`
  DB and prints a parseable single-line JSON `BenchReport`
  whose `baseline` field is `"preflop"`; the new lib tests
  pass; `cargo test --workspace` and `cargo fmt --check` are
  green.
  Owner files: `crates/autotrain/src/{bench,mode,lib}.rs`,
  `crates/autotrain/Cargo.toml`, `crates/gameroom/src/room.rs`,
  `crates/autotrain/tests/bench.rs`, `IMPLEMENTATION_PLAN.md`,
  `genesis/plans/000-ceo-testnet-roadmap.md`.
  Scope boundary: add a `trainer --bench` mode that drives K
  heads-up hands of `DatabasePlayer` (seat 0) vs `Fish` (seat 1)
  through the production `Room` shell, accumulates per-hand
  `settlements()[0]`, and prints a single-line JSON `BenchReport`
  on stdout with mbb/100, 95% CI, win-rate, and a `blueprint_trained`
  boolean. Do not redesign the room protocol, do not introduce a
  new `Player` type, do not change any `Schema` method body, do not
  pre-empt the next benchmark (a stronger baseline bot, a CI
  pipeline) — those are later slices.
  Acceptance criteria: a new `crates/autotrain/src/bench.rs` exists
  with `bench::run`, `bench::run_hands`, `bench::summarize`,
  `bench::BenchReport`, and `Mode::Bench` wired through
  `crates/autotrain/src/mode.rs`. `Room` exposes two new public
  methods required by the bench loop: `Room::settlements` (read
  per-seat `Settlement::won()` from the showdown snapshot) and
  `Room::conclude` (advance from `Showdown` to `Dealing` or
  `Finished`, mirroring the production `Room::run` loop body). The
  bench tolerates an empty-blueprint DB by falling back to a
  default-constructed `Flagship` (so a freshly-`--reset` DB
  doesn't crash on `NlheProfile::hydrate`'s
  `expect("to have already created epoch metadata")`) and the
  `blueprint_trained: false` JSON field flags the fallback for
  downstream consumers. A new `crates/autotrain/tests/bench.rs`
  integration test (gated on `database` + `DATABASE_URL` like
  `crates/autotrain/tests/smoke.rs`) drives
  `trainer --reset` then `trainer --bench` end-to-end and asserts
  the binary exits 0, the JSON line parses, the headline
  accounting is internally consistent
  (`mbb_per_100 == net_chips * 100 / (hands * blind)` within
  `1e-3`), and the post-reset `blueprint_trained` flag is
  `false`. Six lib tests pin the math:
  `mbb_per_100_formula_matches_mean_times_hundred_over_blind`,
  `zero_mean_vector_yields_zero_mbb_and_zero_ci`,
  `mixed_wins_and_losses_split_count`,
  `win_rate_ci95_matches_normal_approx_formula`,
  `to_json_contains_every_field`,
  `bench_hands_env_override_round_trip`.
  Verification commands:
  `cargo test -p rbp-autotrain --features database --tests --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`.
  Required tests: the 6 lib tests in `bench.rs::tests` + the
  `crates/autotrain/tests/bench.rs` integration test.
  Dependencies: `STW-006` (Schema/BulkSchema split) for the
  `Schema::creates()` paths the `Room` history writes need;
  `STW-008` (hand-persistence round-trip) for the live-DB
  `HistoryRepository` writes `Room::flush_hand` issues.
  Estimated scope: M.
  Completion signal: `trainer --bench` exits 0 on a freshly-`--reset`
  DB and prints a parseable single-line JSON `BenchReport`; the
  integration test passes; `cargo test --workspace` and
  `cargo fmt --check` are green.

## Promoted from steward HINGES (this slice)

The `steward/HINGES.md` and `steward/HAZARDS.md` reports name the
top open mainnet-blocking, user-facing hinge as `testnet-live-proof`:
*"A documented live run of `--smoke -> --bench -> --compare -> --replay` on
the same DB is the operator-visible launch proof not captured by unit
tests alone."* `cargo test --workspace` already passes per-stem
(`STW-009` smoke, `STW-010` bench, `STW-016` replay, `STW-018`
compare), and `crates/autotrain/tests/live_proof.rs` chains those
four sub-proofs inside one `cargo test --test live_proof` invocation,
but no **shell-level runbook** an operator (or a cron worker) can
run against a real Postgres to produce a single receipt directory
exists. STW-019 lands that runbook and a one-receipt-per-step
bundle layout a testnet dashboard can scrape.

- [x] `STW-019` Testnet live launch proof runbook + receipt bundle.
Owner files: `scripts/testnet-live-proof.sh` (new),
`scripts/testnet-live-proof.md` (new runbook + receipt layout),
`README.md` (link the runbook under a new `## Testnet launch proof`
section), and `crates/autotrain/tests/script_shape.rs` (new pure
shell-shape integration test gated off `database` so it runs in
`cargo test --workspace`).
Scope boundary: drive the existing four `trainer` modes
(`--cluster` -> `--reset` -> `--smoke` -> `--status` -> `--bench` ->
`--compare` -> `--replay <transcript>`) as subprocesses against a
single Postgres reachable via `DATABASE_URL`, capture each step's
stdout + stderr + exit code into a per-step
`receipts/<step>/{stdout,stderr,exit}.txt` layout, and emit a
one-line `testnet live_proof complete: ...` summary the dashboard
scrapes. Do not add new `trainer` modes; do not change the
integration test's chain; do not require Docker or a wrapper
runtime. Honour the `RBP_FAST_EPOCHS` / `RBP_FAST_BATCH` /
`RBP_BENCH_HANDS` / `RBP_BENCH_BLIND` / `RBP_BENCH_TRANSCRIPT_DIR` /
`RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` env discipline the four
existing integration tests use (small-budget defaults so the
runbook completes in seconds, not minutes).
Acceptance criteria: `bash scripts/testnet-live-proof.sh` against a
Postgres reachable via `DATABASE_URL` exits 0, drops a
`receipts/testnet-live-proof-<utc-iso>/` directory with one
sub-directory per chain step (cluster, reset, smoke, status,
bench, compare, replay) each containing `stdout.txt`,
`stderr.txt`, `exit.txt`, and a top-level `SUMMARY.txt` that
prints `testnet live_proof complete: smoke=N status=N bench=N
compare=N replay=BYTES` matching the
`crates/autotrain/tests/live_proof.rs` final assertion line.
`DATABASE_URL` is forwarded as `DB_URL` (the trainer's actual env
name) and the script refuses to run (exit 3) when neither is set.
The script is pure bash (no `python`/`jq` dependency at runtime;
the existing trainer JSON line is captured verbatim) and exits
non-zero on any chain step that returns non-zero so a worker can
treat the script as a single testnet launch gate.
Verification commands: `cargo build --bin trainer`; then
`DATABASE_URL=postgres://... bash scripts/testnet-live-proof.sh`;
inspect the resulting `receipts/testnet-live-proof-*/SUMMARY.txt`
for the headline line. The shell-shape integration test
(`cargo test -p rbp-autotrain --test script_shape`) runs without
Postgres and asserts (a) the runbook script exists and is
executable, (b) the runbook doc lists the same env knobs the
script honours, (c) `bash -n scripts/testnet-live-proof.sh`
parses, (d) the runbook doc references every step the live
proof integration test covers.
Required tests: existing
`crates/autotrain/tests/{smoke,bench,compare,live_proof}.rs` +
new `crates/autotrain/tests/script_shape.rs`.
Dependencies: `STW-009` (smoke), `STW-010` (bench),
`STW-014`/`STW-015` (transcripts the `--replay` leg reads),
`STW-016` (`--replay` mode), `STW-018` (compare).
Estimated scope: M.
Completion signal: a fresh `bash scripts/testnet-live-proof.sh`
against a real Postgres drops a `receipts/testnet-live-proof-*/`
directory whose `SUMMARY.txt` contains the `testnet live_proof
complete:` line; the new `script_shape` integration test passes;
`cargo test --workspace` and `cargo fmt --check` are green; the
new runbook is linked from `README.md` under `## Testnet launch
proof`.

## Deferred items (need operator decision before promotion)

- [!] `STW-001` Recreate executable planning surface.
  Owner files: `AGENTS.md`, future `genesis/`, future `IMPLEMENTATION_PLAN.md`.
  Blocker: `auto corpus` cannot run because `gbrain` is not configured in the
  operator environment (probe exits 1 with "No brain configured"). Operator must
  either init `gbrain` or hand-author a queue before this becomes claimable.

- [!] `STW-007` Retire stale generated/local artifacts.
  Owner files: `.auto/corpus-staging/`, `.auto/logs/steward-*-prompt.md`,
  `.auto/tui*/`, `.auto/orchestrator/velocity-*`, `.gbrain-source`.
  Blocker: `.gbrain-source` is tracked; operator must sign off before deletion
  even though the rest are ignored.

## Hazard summary (mirrors `steward/HAZARDS.md`)

|                | user-facing                                           | not user-facing                                  |
|----------------|-------------------------------------------------------|--------------------------------------------------|
| mainnet-block  | `STW-003` database-backed build; `STW-004` auth holes | `STW-005` formatter drift                            |
| not mainnet-b  | `STW-008` hand round-trip; ~~`STW-009` trainer smoke~~; `STW-010` bench harness | `STW-001` planning surface; `STW-007` artifact retirement |

## Promotion provenance

The rows above were promoted from
`/tmp/robopoker-steward-9283/PROMOTIONS.md` on 2026-06-03 as part of kanban
task `t_9283ea83`. The first promotion to land is `STW-003` (highest-priority
hinge, mainnet-blocking, user-facing).
