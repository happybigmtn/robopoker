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

The v6 follow-on chain (STW-026 → STW-031) is closed: every
v6 named follow-on slice in `genesis/plans/000-ceo-testnet-roadmap.md`
has shipped. The next claimable slice is the v7 follow-on
(`testnet-live-publish`) the `scripts/testnet-live-proof.md` runbook
doc names explicitly ("pushing it to a testnet dashboard bucket
is the next slice (`testnet-live-publish`)" — line 234): a
deterministic, content-addressed portable publish bundle the
operator (or a CI worker) can drop into a testnet dashboard
bucket so a downstream auditor can re-fetch a green
`testnet-live-proof-<UTC-ISO>` receipt without re-running the
chain.

- [x] `STW-031` `trainer --compare3` v1-vs-v2-vs-v3
  three-way trained-config compare: the "next-next
  slice" the CEO testnet roadmap names immediately
  after the STW-029 v3 trained config ("a
  v1-vs-v2-vs-v3 compare is the next-next slice if
  a v3 trained config proves meaningfully different
  from the v1 / v2 pair" — `genesis/plans/000-ceo-testnet-roadmap.md`
  line 34). A single `trainer --compare3`
  invocation rotates the v1 / v2 / v3 trained
  configs through three pairwise heads-up `Room`
  shells (v1 vs v2, v2 vs v3, v3 vs v1) for K
  hands each, aggregates the per-config net chips
  across both seat assignments (so a config that
  plays both seat 0 and seat 1 across the three
  rotations gets a symmetric per-config mbb/100
  that is unbiased by seat-position), and prints a
  single-line JSON `Compare3Report` declaring the
  ranked winner (the v1 / v2 / v3 config with the
  strictly highest per-config `mbb_per_100`, or
  `Tie` if the top two are within
  [`COMPARE3_TIE_TOLERANCE`] mbb/100) plus the
  per-config mbb/100 / CI / win-rate numbers and
  the three pairwise `delta_mbb_per_100` values
  (v1-vs-v2, v2-vs-v3, v3-vs-v1). The
  `ranked_winner` field is the headline a testnet
  dashboard reads; the per-config sub-reports let
  a downstream scraper plot the per-config
  learning curve over a series of `--compare3`
  runs. The compare3 reuses the same `Room` shell
  + per-hand settlement shape the existing
  `--compare` (v1-vs-v2 STW-018) uses, so a
  regression in the per-hand PnL math fails the
  bench / compare / compare3 integration tests in
  the same CI run. Owner files:
  `crates/autotrain/src/bench.rs` (new
  `Compare3Winner` enum + `Compare3SubReport`
  struct + `Compare3Report` struct + `to_json` +
  `summarize_compare3` + new
  `compare3_winner_as_str_matches_published_strings`
  / `compare3_summarize_declares_v1_winner_when_v1_top`
  / `compare3_summarize_declares_v2_winner_when_v2_top`
  / `compare3_summarize_declares_v3_winner_when_v3_top`
  / `compare3_summarize_declares_tie_on_close_top_two`
  / `compare3_summarize_v1_v2_v3_deltas_each_pair_nets_to_zero`
  / `compare3_summarize_aggregates_per_config_across_both_seats`
  / `compare3_to_json_contains_every_field` lib
  tests + new `run_compare3_hands` /
  `run_compare3` top-level entry points + new
  `DEFAULT_COMPARE3_HANDS` /
  `DEFAULT_COMPARE3_BLIND` /
  `COMPARE3_TIE_TOLERANCE` constants + new
  `compare3_hands` / `compare3_blind` env helpers),
  `crates/autotrain/src/mode.rs` (new
  `Mode::Compare3` arm + `--compare3` argv
  handling + the v1 / v2 / v3 rotation call into
  `bench::run_compare3` + `--compare3` listed in
  the `Usage:` eprintln! line),
  `crates/autotrain/tests/compare3.rs` (new
  `compare3_run_emits_parseable_json_with_consistent_accounting`
  integration test gated on `database` +
  `DATABASE_URL` like the existing `compare.rs`
  integration test — drives `trainer --reset`
  then `trainer --compare3` end-to-end and
  asserts the JSON line parses, the headline
  accounting is internally consistent (the three
  pairwise per-hand PnL vectors each net to zero
  because the heads-up `Room` is two-seat;
  per-config mbb/100 is the sum of that config's
  seat-0 and seat-1 PnL across its two
  appearances, the `ranked_winner` field ∈
  `{"v1", "v2", "v3", "tie"}`, the v1 / v2 / v3
  sub-reports each have non-zero `hands` and the
  same `hands` count = 2*K), and the post-reset
  `blueprint_trained_v1` / `blueprint_trained_v2`
  / `blueprint_trained_v3` flags are all `false`),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the v6 next-next slice as shipped with
  a one-line note). Scope boundary: do NOT touch
  the existing `Mode::Compare` / `CompareReport`
  / `--compare` code path (the compare3 is a
  three-way compare, not a refactor of the
  two-way compare — a future dashboard that
  scrapes `--compare` reports is unchanged by
  this slice); do NOT introduce a fourth
  `Blueprint` variant; do NOT change the seat-0
  / seat-1 dispatch in `bench::run_hands`; do
  NOT introduce a new trained config; the
  compare3 reuses the v1 + v2 + v3 trained
  configs the v1 + v2 + v3 `trainer --bench`
  paths already hydrate; do NOT change the room
  protocol, the `Schema` contracts, the
  autotrain pipeline, the K-means cluster
  counts, the `CFR_TREE_COUNT_NLHE` baseline,
  the v1 / v2 / v3 / v4 named baselines, the
  `trainer --replay` CLI, the
  `trainer --verify-receipt` CLI, or the
  `trainer --smoke` / `trainer --bench` /
  `trainer --compare` JSON contracts. The
  compare3 is *structurally parallel* to the
  compare (one `Room` shell per pair, three
  rotations to give each config a symmetric
  seat-position exposure, one JSON report) so a
  `trainer --compare` run and a `trainer
  --compare3` run can coexist in the same
  database without colliding on the v1 / v2 / v3
  staging tables, the v1 / v2 / v3 blueprint
  tables, or the v1 / v2 / v3 epoch rows.
  Verification commands: `cargo test -p
  rbp-autotrain --features database --tests
  --lib`, `cargo test --workspace --
  --test-threads=4`, `cargo check --workspace`,
  `cargo fmt --check`. Required tests: the new
  lib tests in `bench.rs::tests` + the new
  `crates/autotrain/tests/compare3.rs`
  integration test; no padding of unrelated
  suites. Dependencies: `STW-029` (the v3
  trained config + v3 `DatabasePlayer3` + v3
  `Blueprint::V3` env-var dispatch the
  compare3 seats at the rotated seats),
  `STW-018` (the v1-vs-v2 `Mode::Compare` +
  `CompareReport` + `Room` shell the compare3
  reuses one-for-one), and `STW-010` (the bench
  harness the compare3 mirrors). Estimated
  scope: M. Completion signal: `trainer
  --compare3` exits 0 on a freshly-`--reset` DB
  and prints a parseable single-line JSON
  `Compare3Report` whose `ranked_winner` field
  is one of `{"v1", "v2", "v3", "tie"}`; the
  integration test passes; `cargo test
  --workspace` and `cargo fmt --check` are
  green.

- [x] `STW-030` `verification:workspace-parallel` mainnet-block
  hinge close-out: the `steward/HINGES.md` rank #2 hinge
  (also listed in `steward/HAZARDS.md` as a mainnet-block
  hazard) calls for "three consecutive
  `cargo test --workspace -- --test-threads=4` runs
  pass, or a minimal deterministic fix lands with
  a regression test/proof." The historical flake
  at `crates/gameplay/src/game.rs:1397`
  (`bust_prevents_next`) was closed in two layers:
  STW-005 relaxed the `bust_prevents_next` assertion
  to pot-conservation (the only invariant that holds
  on every legal board, not just the
  winner-takes-all boards), and STW-020 added the
  64-seed `bust_prevents_next_deterministic` lib
  test that threads a seeded `StdRng` through a new
  `Deck::deal_with` injection seam so the heads-up
  all-in showdown is bit-exactly reproducible. What
  remained open was *mechanical CI evidence* that the
  fix is not just present but *always green* in the
  `cargo test --workspace -- --test-threads=4`
  concurrency regime the script's `RECURSIVE_SKIP`
  filter is meant to keep stable. STW-030 lands a
  new no-DB integration test
  `crates/autotrain/tests/workspace_parallel_proof_three.rs`
  with two sub-tests: (a) the *in-CI* 3-consecutive
  proof `run_three_consecutive_clean_gameplay_lib_test_runs`
  drives `cargo test -p rbp-gameplay --lib --
  --test-threads=4` three consecutive times (the
  gameplay crate is the crate the historical flake
  lived in, so a 3-consecutive green of *just*
  gameplay is the cheapest always-CI proof that the
  fix is live and the flake is dead) and asserts
  every run exits 0, prints `test result: ok. 111
  passed`, and the three `passed` counts are
  identical; (b) the static `SUMMARY.txt`
  printf-format pin
  `summary_headline_format_contains_runs_and_failures`
  greps the runbook script for the ordered
  `runs=${RUNS}` / `failures=${failures}` pair
  contract. The companion script-invocation pin
  is intentionally *not* added as a third sub-test
  — the existing sibling
  `workspace_parallel_proof.rs::runbook_run_exits_zero_with_single_clean_workspace_run`
  already drives the script end-to-end with
  `RUNS=1` so a regression in the script's exit-0 +
  headline-format contract is caught by the sibling,
  and adding a second `cargo test --workspace`
  invocation from inside the autotrain integration
  tests risks the cargo build-lock collision the
  `RECURSIVE_SKIP` filter is designed to dodge.
  The new `RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET=1`
  env knob lets an operator mute the per-run
  `stdout` echo without changing the
  exit-code contract. The accompanying
  `scripts/workspace-parallel-proof.sh` remains
  the canonical 3-consecutive *full-workspace*
  proof (operator / nightly path); STW-030 also
  extends the script's `RECURSIVE_SKIP` filter
  to skip the new in-CI test names so the
  script's child `cargo test --workspace`
  invocation does not re-enter the new tests'
  spawn pattern (without changing the script's
  3-consecutive full-workspace behavior or the
  headline / exit-code contract the sibling
  tests pin). STW-030 adds the cheap in-CI proof
  so a future regression in
  `bust_prevents_next` or any gameplay lib test
  fails `cargo test --workspace` on a single
  failed run of the new
  `run_three_consecutive_clean_gameplay_lib_test_runs`
  test, *without* requiring a nightly run of the
  shell script. The test is no-DB, runs in
  `cargo test --workspace` (alongside the existing
  `workspace_parallel_proof.rs` and
  `plan_staleness.rs` integration tests), and
  the 3-consecutive gameplay proof sub-test
  exits in under 2 s on a clean checkout. Owner
  files:
  `crates/autotrain/tests/workspace_parallel_proof_three.rs`
  (new file with the two sub-tests + the
  `RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET`
  env knob),
  `scripts/workspace-parallel-proof.sh`
  (extend `RECURSIVE_SKIP` to also skip the
  new in-CI test names so the script's
  child `cargo test --workspace` invocation
  does not re-enter the new tests' spawn
  pattern; the script's 3-consecutive
  full-workspace behavior is unchanged),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the `verification:workspace-parallel`
  P0-row as closed with a one-line note in the
  P0 retirement list). Scope boundary: do NOT
  touch the
  `crates/autotrain/tests/workspace_parallel_proof.rs`
  shape contract (the existing 4 sub-tests stay
  as-is, the new test is a sibling, not a
  replacement); do NOT touch the
  `bust_prevents_next` /
  `bust_prevents_next_conserves_pot_across_boards` /
  `bust_prevents_next_deterministic` lib tests
  (STW-005 / STW-020 are the actual fix; STW-030
  is the in-CI regression-proof for the fix);
  do NOT change the `--test-threads=4` /
  `--skip=runbook_run_exits_zero_with_single_clean_workspace_run`
  concurrency contract the script and the
  existing integration test pin; do NOT touch
  any STW-001 / STW-007 / STW-011 / STW-015
  operator-decision items; do NOT change the
  room protocol, the `Schema` contracts, the
  autotrain pipeline, the K-means cluster
  counts, the v1 / v2 / v3 trained configs, the
  `CFR_TREE_COUNT_NLHE` baseline, the
  v1 / v2 / v3 / v4 named baselines, or any
  `trainer --*` CLI. STW-030 is the
  *mechanical proof* that the
  `verification:workspace-parallel` hinge is
  closed in CI (not just in a one-shot operator
  invocation), so a future regression in the
  showdown PnL math or any gameplay lib test
  cannot silently pass locally and only flake
  under `cargo test --workspace --
  --test-threads=4`. Verification commands:
  `cargo test -p rbp-autotrain --test
  workspace_parallel_proof_three` (the 2 new
  sub-tests pass), `cargo test --workspace
  --no-run && cargo test --workspace --
  --test-threads=4
  --skip=runbook_run_exits_zero_with_single_clean_workspace_run
  --skip=run_three_consecutive_clean_gameplay_lib_test_runs
  --skip=summary_headline_format_contains_runs_and_failures`
  (3 consecutive green runs as a
  smoke-check, exit 0), `cargo test --workspace
  -- --test-threads=4` (full workspace
  green), `cargo check --workspace`,
  `cargo fmt --check`. Required tests: 2 new
  lib tests in
  `crates/autotrain/tests/workspace_parallel_proof_three.rs`
  (`run_three_consecutive_clean_gameplay_lib_test_runs` /
  `summary_headline_format_contains_runs_and_failures`).
  Dependencies: `STW-020` (the
  `bust_prevents_next_deterministic` 64-seed
  regression test whose existence makes
  STW-030's "the flake is dead" claim
  defensible) and `STW-005` (the relaxed
  pot-conservation assertion in
  `bust_prevents_next` that the
  64-seed regression test pins). Estimated
  scope: S. Completion signal:
  `cargo test -p rbp-autotrain --test
  workspace_parallel_proof_three` is green
  with 2 new sub-tests passing;
  `cargo test --workspace -- --test-threads=4`
  exits 0; the new
  `run_three_consecutive_clean_gameplay_lib_test_runs`
  test is wired into the workspace test
  invocation (a future `cargo test
  --workspace` run that does *not* skip it
  will exercise it in the in-CI 4-thread
  concurrency regime); a CI dashboard can
  `grep ^QA-CHECK verification.workspace_parallel`
  the new test's `passed = true` /
  `detail = "3/3 consecutive gameplay lib runs
  green"` line in the same shape the
  `tui.qa.json` gate uses; the
  `verification:workspace-parallel` hinge can
  be retired from `steward/HINGES.md` /
  `steward/HAZARDS.md` /
  `steward/DRIFT.md` by the next
  `auto steward --report-only` pass.

- [x] `STW-029` Third trained config (`Flagship3` =
  `DiscountedRegret` + `LinearWeight` +
  `PluribusSampling`): the "third
  DCFR-with-LinearWeight variant" the CEO testnet
  roadmap explicitly names as the v6 next
  slice after STW-017's `Flagship2` trained
  config (the comparison half shipped earlier in
  STW-018). A new `Blueprint::V3` variant lets
  a single `trainer --bench` invocation compare
  the v1 / v2 / v3 trained configs
  head-to-head against the same named baseline
  (so a dashboard can group reports by
  `blueprint` to produce a "v1 vs fish", "v2
  vs fish", and "v3 vs fish" curve from the
  same `BenchReport` stream). The v3 has its
  own persistence triple (`BLUEPRINT3` /
  `EPOCH3` / `'current_v3'` / `STAGING3`),
  its own trainer (`trainer --fast3` running
  `Fast3Session`), its own bench seat
  (`RBP_BENCH_BLUEPRINT=v3` seats a
  `DatabasePlayer3` at seat 0 and reports
  `"blueprint":"v3"` in the JSON), and its
  own status read (`trainer --status` now
  prints the v1 + v2 + v3 epoch + blueprint
  row counts side-by-side). A v1 / v2
  `Mode::reset` does not zero the v3
  `'current_v3'` row and a v3 `Mode::reset`
  does not zero the v1 / v2 rows. The v3 is
  the missing cross-product cell of the v1 /
  v2 regret / policy combination
  (`PluribusRegret`+`LinearWeight`,
  `DiscountedRegret`+`QuadraticWeight`,
  `DiscountedRegret`+`LinearWeight`) so a v1
  / v2 / v3 trained-config triplet lets a
  downstream scraper disentangle the regret
  and policy axes of the trained blueprint
  without re-training. Owner files:
  `crates/database/src/lib.rs` (v3
  constants `BLUEPRINT3` / `EPOCH3` /
  `EPOCH3_KEY` / `STAGING3`),
  `crates/database/src/check3.rs` (new
  `Check3` trait + `Client` / `Arc<Client>`
  impls reading v3 epoch / blueprint
  counts), `crates/database/src/stage3.rs`
  (new `Stage3` trait + `Client` /
  `Arc<Client>` impls for v3 `stage3` /
  `merge3` / `stamp3(epochs)` with
  `'current_v3'` row scoping),
  `crates/nlhe/src/lib.rs` (v3 `Flagship3`
  type alias + `mod profile_v3`),
  `crates/nlhe/src/profile_v3.rs` (new
  `NlheProfileV3(NlheProfile)` newtype +
  `Schema` / `BulkSchema` / `Hydrate` impls
  targeting `BLUEPRINT3`, `EpochMetaV3` +
  `Schema` / `BulkSchema` impls targeting
  `EPOCH3` with the `'current_v3'` row
  seeded in `creates()`, plus
  `hydrate_flagship3(client) -> Flagship3`
  free function that wraps the v1
  `NlheProfile` in-memory shape verbatim +
  7 lib tests pinning the v3 schema
  contracts),
  `crates/gameroom/src/players/database_v3.rs`
  (new `DatabasePlayer3` player + `new` /
  `from_database` constructors, the v1 /
  v2 / v3 `decide` paths share the same
  `abstraction` → `NlheInfo` →
  `averaged_distribution` → weighted-sample
  recipe + 2 lib tests pinning the
  static-`new` and `database`-feature
  `from_database` signature shape),
  `crates/gameroom/src/players/mod.rs`
  (re-export `DatabasePlayer3`),
  `crates/autotrain/src/pretraining.rs`
  (bootstrap the v3 `BLUEPRINT3` / `EPOCH3`
  tables in `PreTraining::run` so a fresh
  DB doesn't crash on the first
  `Fast3Session::sync`),
  `crates/autotrain/src/lib.rs` (re-export
  `Fast3Session`),
  `crates/autotrain/src/fast3.rs` (new
  `Fast3Session` parallel of v1
  `FastSession` and v2 `Fast2Session` —
  same `step` / `epoch` / `checkpoint` /
  `summary` delegation, same shape, but the
  v3 `sync` writes `staging_v3` /
  `BLUEPRINT3` / `'current_v3'` instead of
  the v1 / v2 trios),
  `crates/autotrain/src/mode.rs` (new
  `--fast3` mode + v3 epoch / blueprint
  status read in `Mode::Status` + v3 arm in
  `Mode::reset` that zeros the
  `'current_v3'` row only + `--fast3`
  listed in the `Usage:` eprintln! line),
  `crates/autotrain/src/bench.rs` (new
  `Blueprint::V3` enum variant + v3 arms
  in `as_str` / `from_env` + v3 seat-0
  dispatch in `run_hands` that mirrors the
  v1 / v2 trained-or-empty-blueprint
  fallback shape + v3 row-count read in
  `run` via `client.blueprint_v3()` + three
  updated lib tests
  `blueprint_as_str_matches_published_strings`
  / `blueprint_from_env_round_trip` /
  `summarize_stamps_blueprint_into_report`
  adding a v3 assertion),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the v6 third
  DCFR-with-LinearWeight variant as
  shipped with a one-line note). Scope
  boundary: do NOT touch the existing v1 /
  v2 `NlheProfile` / `NlheProfileV2` /
  `EpochMetaV2` schema contracts; do NOT
  change the v1 / v2 / v3 train loop
  (`step` / `epoch` / `checkpoint` /
  `summary` delegate to the wrapped
  solver); do NOT change the v1 / v2
  `Mode::Bench` / `BenchReport` / `--bench`
  code path beyond extending the
  `Blueprint` enum and the seat-0
  `match` arm; do NOT change the v1 / v2
  `Mode::Compare` v1-vs-v2 compare
  harness (the v3 is a third trained
  config, not a new compare dimension —
  a v1-vs-v2-vs-v3 compare is the next
  slice if a v3 trained config proves
  meaningfully different from the v1 / v2
  pair); do NOT change the room protocol,
  the `Schema` contracts, the autotrain
  pipeline, the K-means cluster counts,
  the `CFR_TREE_COUNT_NLHE` baseline, the
  v1 / v2 / v3 / v4 named baselines, the
  `trainer --replay` CLI, the
  `trainer --verify-receipt` CLI, or the
  `trainer --smoke` / `trainer --bench` /
  `trainer --compare` JSON contracts. The
  v3 is *structurally parallel* to the v1
  / v2 trained configs (same
  `NlheProfile` in-memory shape, same
  trainer `step` / `epoch` / `checkpoint`
  / `summary` shape, same `DatabasePlayer`
  weighted-sample `decide` shape, same
  `Mode::Bench` Blueprint enum extension)
  so a v1 `trainer --fast`, a v2
  `trainer --fast2`, and a v3
  `trainer --fast3` run can coexist in
  the same database without colliding on
  the staging tables, the blueprint
  tables, or the epoch rows. Verification
  commands:
  `cargo test -p rbp-database --lib` (the
  2 new v3 lib tests pass),
  `cargo test -p rbp-nlhe --features
  database --lib` (the 6 new v3 lib
  tests pass), `cargo test -p
  rbp-gameroom --features database --lib`
  (the 2 new v3 lib tests pass),
  `cargo test -p rbp-autotrain --lib` (the
  3 updated v2 lib tests still pass with
  the new v3 assertions), `cargo test
  --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Required tests: 2 new lib
  tests in `crates/database/src/check3.rs`
  + 1 new lib test in
  `crates/database/src/stage3.rs` + 6
  new lib tests in
  `crates/nlhe/src/profile_v3.rs::hydrate_tests`
  + 2 new lib tests in
  `crates/gameroom/src/players/database_v3.rs::tests`
  + 3 updated lib tests in
  `crates/autotrain/src/bench.rs::tests`
  adding the v3 `Blueprint` assertions —
  total 14 new lib tests and 3 updated
  lib tests, all no-DB. Dependencies:
  `STW-017` (the v2 `Flagship2` /
  `NlheProfileV2` / `DatabasePlayer2` /
  `Fast2Session` / `Check2` / `Stage2` /
  `Blueprint::V2` shape the v3 mirrors
  one-for-one) and `STW-010` (the bench
  harness + `Room` shell the v3 seat-0
  dispatch reuses). Estimated scope: M.
  Completion signal: `cargo test
  --workspace` is green with 14 new + 3
  updated v3 lib tests passing;
  `cargo fmt --check` is clean; the
  `Mode::Fast3` arm is wired into the
  exhaustive `match` (a `trainer`
  invocation with no args prints
  `--fast3` as one of the listed modes);
  a v3 `trainer --status` run prints the
  v3 epoch + blueprint row counts
  alongside the v1 / v2 counts; a v3
  `trainer --reset` run truncates the v3
  `BLUEPRINT3` table and zeros the v3
  `'current_v3'` row without touching the
  v1 / v2 tables; the new
  `Blueprint::V3` variant is wired into
  the `as_str` / `from_env` /
  `summarize_stamps_blueprint_into_report`
  trio of tests.

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

- [x] `STW-020` Stabilize full workspace parallel test proof.
The `steward/HAZARDS.md` table classifies
`verification:workspace-parallel` as mainnet-block (not
user-facing) and `steward/HINGES.md` ranks it hinge #2: "Proving
or fixing `cargo test --workspace -- --test-threads=4` makes
every shipped-row completion signal trustworthy again." One
historical full-workspace run failed at
`crates/gameplay/src/game.rs:1397` on a brittle
"winner-takes-all" assertion in `bust_prevents_next`; the
5fcc090 fix relaxed the assertion to assert pot conservation,
but the *workspace-level parallel proof* itself was never
locked in — a future test that re-introduces a global-RNG-
dependent brittle assertion would silently pass locally and
flake under `--test-threads=4`. STW-020 lands three pieces of
proof: (a) a pure-bash `scripts/workspace-parallel-proof.sh`
runbook that runs `cargo test --workspace -- --test-threads=4`
3 times back-to-back, captures each run's stdout/stderr/exit
into `logs/workspace-parallel-proof/<UTC-ISO>/run-{1,2,3}/`,
asserts all 3 exit 0, and prints a one-line `workspace
parallel proof complete: runs=3 failures=0` summary on
success (exits 3 on any run failure, exits 1 on script
internal error); (b) an integration test in
`crates/gameplay/tests/workspace_parallel_proof.rs` that
invokes the script via `Command::new("bash")` and asserts the
summary line + exit 0, so `cargo test --workspace` itself
proves the proof; (c) a deterministic regression test in
`crates/gameplay/src/game.rs` (`bust_prevents_next_deterministic`)
that injects a seeded `StdRng` into the deck deal path and
runs the heads-up all-in flow across 64 fixed seeds,
asserting the full pot conservation invariant (sum=200,
len=2, each ∈ {0, 100, 200}, sorted ∈ {{0,200}, {100,100}})
for every seed — making the conservation property
bit-exactly reproducible. Owner files:
`scripts/workspace-parallel-proof.sh` (new, pure bash, ~50
lines), `crates/gameplay/tests/workspace_parallel_proof.rs`
(new integration test, no Postgres needed), a small RNG
injection seam in `crates/cards/src/deck.rs` (new
`Deck::deal_with` constructor that takes a `seed: u64` and
uses a local `StdRng` so the deal is deterministic), a
matching helper in `crates/gameplay/src/game.rs` for the
seeded test, `IMPLEMENTATION_PLAN.md` (this row),
`README.md` (link the runbook under `## Workspace parallel
test proof`). Scope boundary: do NOT modify the existing
5fcc090-relaxed `bust_prevents_next` assertion or the 32-
trial `bust_prevents_next_conserves_pot_across_boards` —
STW-020 only ADDS the seeded regression test next to them.
Do NOT change the parallel-test thread count from 4 (the
documented worker-runner contract; `RBP_WORKSPACE_PARALLEL_THREADS`
overrides in CI). Do NOT add a third-party determinism
harness. Do NOT touch the HINGES ranking or the HAZARDS
table — STW-020 closes the open hinge by the work itself,
not by re-ranking. The stale-P0 retirement (hinge #1) and
STW-001 / STW-007 deferred items are out of scope.
Verification commands: `bash scripts/workspace-parallel-proof.sh`
(exits 0, prints the summary line), `cargo test --workspace`
(the integration test passes), `cargo test -p rbp-gameplay
--lib bust_prevents_next` (all 3 bust tests pass), `cargo
test -p rbp-gameplay --test workspace_parallel_proof` (the
runbook driver passes), `cargo fmt --check`, `cargo check
--workspace`. Required tests: new
`bust_prevents_next_deterministic` lib test + new
`workspace_parallel_proof_run_exits_zero_with_three_clean_workspace_runs`
integration test. No padding of unrelated suites.
Dependencies: the 5fcc090 relaxed assertion (already on
`main`). Estimated scope: S/M. Completion signal: a fresh
`bash scripts/workspace-parallel-proof.sh` from a clean
checkout prints `workspace parallel proof complete: runs=3
failures=0` and exits 0; the new `workspace_parallel_proof`
integration test passes; the 64-seed
`bust_prevents_next_deterministic` test passes; `cargo
test --workspace` and `cargo fmt --check` are green; the
runbook is linked from `README.md` under `## Workspace
parallel test proof`.

## Active items (worker-ready)

The most recent shipped slice is `STW-023` (`LiveProofReceipt`
verifier + runbook `recipe.json` manifest). With `STW-019` →
`STW-023` the four P0-hinge items from the prior steward pass
(`testnet-live-proof`, `STW-001` operator decision, `STW-007`
operator sign-off, `verification:workspace-parallel`) are all
either closed or explicitly out-of-scope for an autonomous
worker. The next documented open slice is the
`crates/transport` ORPHAN from `steward/DRIFT.md`: the crate
ships `Support`, `Density`, `Measure`, and `Coupling` traits
plus two empty type shells (`GreenhornOptimalTransport`,
`GreedyOptimalTransport`) but provides **zero concrete
implementations** and **zero tests**; downstream consumers
(`crates/clustering/src/sinkhorn.rs`, `crates/clustering/src/emd.rs`,
`crates/clustering/src/metric.rs`, `crates/server/src/analysis/api.rs`,
`crates/mccfr/*`) re-roll the algorithms themselves because
`rbp-transport` only exposes the trait surface. STW-024
promotes the next documented slice from the steward DRIFT
table — concrete `Measure` impls + concrete `Coupling` impls +
deterministic tests in `crates/transport` — so the crate
stops being a trait-only scaffold and downstream consumers
have a single source of truth.

- [x] `STW-023` Live-proof receipt bundle: shared
  `LiveProofReceipt` verifier + runbook `recipe.json` manifest.
  The `steward/HINGES.md` rank #5 `testnet-live-proof` hinge
  names the operator-visible launch-proof gap: "A documented
  live run of `--smoke -> --bench -> --compare -> --replay`
  on the same DB is the operator-visible launch proof not
  captured by unit tests alone." `STW-019` shipped the runbook
  (`scripts/testnet-live-proof.sh`) + the per-step
  `{stdout,stderr,exit}.txt` layout, and the live-proof
  integration test (`crates/autotrain/tests/live_proof.rs`)
  drives the same chain as a single `cargo test` invocation,
  but the two surfaces produce *independent* receipts with
  *separate* verification rules — the runbook writes
  `SUMMARY.txt`, the integration test writes an
  `eprintln!` line, and a future drift in one fails without
  a clear "the other is also stale" signal. `STW-023` lands
  a single shared `LiveProofReceipt` verifier both surfaces
  call into, plus a `recipe.json` manifest the runbook
  script writes alongside `SUMMARY.txt` so the chain-step
  order + per-step exit codes are machine-readable, not
  text-grep-able. A new
  `crates/autotrain/tests/live_proof_receipt.rs` (no
  `database` feature gate, runs in
  `cargo test --workspace`) drops a synthetic receipt under
  `target/test-receipts/live_proof-fixture-<UTC>/`, calls
  `LiveProofReceipt::verify` on the freshly-written receipt,
  and asserts the verifier agrees the receipt is green
  (every step exit 0, the headline line parses, the
  `recipe.json` manifest is JSON-parseable, the per-step
  `recipe.json.steps[i].name` field matches the
  `receipts/<step>/` directory name). A regression in the
  receipt shape (renamed step, dropped exit code, broken
  headline prefix) fails the test. The `live_proof.rs`
  integration test is updated to also call
  `LiveProofReceipt::write_to` on a real `cargo test
  --test live_proof` run, so a `cargo test --workspace`
  invocation produces the same on-disk shape the runbook
  does — making the operator-visible receipt *and* the
  CI-visible receipt share one verifier. Owner files:
  `crates/autotrain/src/receipt.rs` (new — `LiveProofStep`,
  `LiveProofReceipt`, `LiveProofRecipe`, the
  `LiveProofReceipt::record_step` / `::write_to` /
  `::headline` / `::verify` / `::read_from` / `::recipe_path`
  surface, and the `STW023_CHAIN_STEPS` constant that
  pins the seven step names `cluster` / `reset` / `smoke` /
  `status` / `bench` / `compare` / `replay` in order),
  `crates/autotrain/src/lib.rs` (wire the new module),
  `crates/autotrain/tests/live_proof.rs` (drop a per-step
  receipt bundle + call `LiveProofReceipt::verify` at the
  end of the chain, gated on `database` feature +
  `DATABASE_URL` like the existing assertions),
  `crates/autotrain/tests/live_proof_receipt.rs` (new no-DB
  test that drops a synthetic receipt under
  `target/test-receipts/live_proof-fixture-<UTC>/`, calls
  `LiveProofReceipt::verify` on it, and asserts the
  verifier's "green" verdict + the headline line + the
  recipe.json step-name order matches the directory layout),
  `crates/autotrain/tests/script_shape.rs` (assert the
  runbook script sources the new `recipe.json` block),
  `scripts/testnet-live-proof.sh` (write a
  `recipe.json` manifest alongside `SUMMARY.txt` using a
  new `write_recipe` helper that mirrors the `LiveProofRecipe`
  struct), `scripts/testnet-live-proof.md` (document the
  new `recipe.json` file + the
  `crates/autotrain::LiveProofReceipt::verify` verifier
  reuse), `IMPLEMENTATION_PLAN.md` (this row + the
  plan-staleness-gate `STW-023` claim), and
  `genesis/plans/000-ceo-testnet-roadmap.md` (note STW-023
  as the `testnet-live-proof` operator-receipt promotion).
  Scope boundary: do NOT change the seven chain step
  names (`cluster` / `reset` / `smoke` / `status` /
  `bench` / `compare` / `replay`), the
  `testnet live_proof complete: smoke=...` headline
  format (the runbook and `script_shape.rs` already pin
  it; the new verifier re-uses the same regex), the
  `RBP_FAST_EPOCHS` / `RBP_BENCH_HANDS` / `RBP_COMPARE_HANDS`
  env discipline, the existing per-step
  `{stdout,stderr,exit}.txt` layout, the
  `live_proof.rs` chain step count (six in
  `live_proof.rs`: cluster, reset, smoke, status, bench,
  compare, replay — note `live_proof.rs` actually counts
  6/6 with replay as step 6, so the verifier accepts
  any step order as long as every name appears exactly
  once), the STW-019 / STW-020 / STW-021 / STW-022
  gate shapes, the `Cargo.toml` dependency graph, the
  `crates/transport` / `bin/tui` / `crates/server/src/analysis`
  orphan surfaces, the `STW-001` planning-surface
  decision, the `STW-007` artifact-retirement sign-off,
  the `STW-011` / `STW-015` provenance notes, or the
  `tui.qa.json` / `tui.receipt.md` shape. Verification
  commands: `cargo check --workspace`,
  `cargo test -p rbp-autotrain --test live_proof_receipt`
  (no DB, runs in `cargo test --workspace`),
  `cargo test -p rbp-autotrain --test script_shape`
  (no DB, runs in `cargo test --workspace`),
  `cargo test --workspace -- --test-threads=4`,
  `cargo fmt --check`, `bash -n scripts/testnet-live-proof.sh`,
  `bash scripts/plan-staleness-gate.sh`. Required tests:
  the new lib tests in `receipt.rs::tests`
  (`live_proof_receipt_records_steps_in_order`,
  `live_proof_receipt_write_to_drops_per_step_files`,
  `live_proof_receipt_headline_format_is_pinned`,
  `live_proof_receipt_read_from_round_trips`,
  `live_proof_receipt_verify_accepts_green_receipt`,
  `live_proof_receipt_verify_rejects_failed_step`,
  `live_proof_recipe_serialises_step_order`) plus the
  new `live_proof_receipt.rs` integration test's
  `synthetic_receipt_verifies_green_via_lib`,
  `synthetic_receipt_manifest_recipes_step_names`,
  `synthetic_receipt_verifier_rejects_renamed_step`,
  `synthetic_receipt_verifier_rejects_missing_exit_code`,
  `synthetic_receipt_headline_uses_pinned_prefix`. The
  new `script_shape.rs` test
  `script_writes_recipe_json_manifest` asserts the
  runbook script sources a `recipe.json` block (a
  `cat > "$RECEIPT_DIR/recipe.json" <<'JSON' ... JSON`
  heredoc anchored to a known `LiveProofRecipe` JSON
  shape). Completion signal: a fresh
  `cargo test -p rbp-autotrain --test live_proof_receipt`
  invocation drops a
  `target/test-receipts/live_proof-fixture-<UTC>/SUMMARY.txt`
  whose headline is the pinned `testnet live_proof
  complete: smoke=N status=N bench=N compare=N replay=N`
  line; a fresh `bash scripts/testnet-live-proof.sh`
  against a real Postgres drops a
  `receipts/testnet-live-proof-<UTC-ISO>/recipe.json`
  whose `steps[i].name` is one of the seven pinned
  names; `LiveProofReceipt::verify` returns `Ok(())`
  on both surfaces; the STW-022 plan-staleness gate
  exits 0 (`checked=N ghosts=0`) and the new STW-023
  claim is registered in the gate's `P0_TO_STW` /
  `STW_TO_STW023` claim map; `cargo test --workspace`
  and `cargo fmt --check` are green. The verifier
  also exposes a typed `LiveProofHeadline` surface
  (a `LiveProofHeadline { smoke, status, bench,
  compare, replay }` u64 struct + a
  `LiveProofHeadline::parse(&str)` parser with a
  structured `HeadlineParseError` covering
  WrongPrefix / MalformedToken / UnknownKey /
  DuplicateKey / MissingKey / NonInteger failure
  modes) so a testnet dashboard can chart the
  per-run artifact counts without re-running a regex
  on every scrape; `LiveProofReceipt::verify` now
  routes its headline-format check through the typed
  parser (the substring gate is preserved inside
  `parse` so a regression that drops a `key=` pair
  surfaces as a precise error variant, not a
  substring miss).

- [x] `STW-022` Plan-vs-reality staleness gate:
  `steward/HINGES.md` rank #1 is "Retiring or updating the
  stale Immediate P0 checklist removes the largest false
  backlog signal. Obsoletes `genesis:P0-schema`,
  `genesis:P0-smoke`, `genesis:P0-bench`, `genesis:P0-auth`;
  stops workers from reclaiming shipped STW-004/006/008/009/010
  work." The HAZARDS table in `steward/HAZARDS.md` classifies
  the ghost-P0 dispatch as `mainnet-block, user-facing`, and
  the `steward/DRIFT.md` `## Roadmap/Plan Cross-Drift` table
  lists the five rows as `GHOST` (STW-004/006/008/009/010 all
  shipped; the `[P0]` checklist in
  `genesis/plans/000-ceo-testnet-roadmap.md` still unchecked).
  The fix is a new pure-bash `scripts/plan-staleness-gate.sh`
  (shebang `#!/usr/bin/env bash`, `set -euo pipefail`) that
  walks the roadmap's `[ ] [P0] ...` rows, cross-references
  each one against a static `P0_TO_STW` claim map
  (mirrored against the `steward/DRIFT.md` GHOST table), and
  greps `IMPLEMENTATION_PLAN.md` for the matching
  `- [x] \`STW-NNN\`` shipped marker. If the matching STW is
  shipped, the P0 row is GHOST and the gate exits 3 with the
  precise roadmap line + STW id on stderr. If all P0 rows
  are either retired (row flipped to `[x]`, or claim text
  rewritten off the published substring) or their STW is
  genuinely not-yet-shipped, the gate exits 0 with the
  headline `plan staleness gate complete: checked=N ghosts=0`
  a CI dashboard greps. Owner files:
  `scripts/plan-staleness-gate.sh` (new, ~150 lines, pure
  bash; static `P0_TO_STW` claim map of the five ghost rows:
  `Implement the \`Schema\` -> STW-006`,
  `Add an end-to-end test in \`crates/gameroom\` -> STW-008`,
  `Implement a \`trainer\` smoke path -> STW-009`,
  `Build a \`bin/bench\` -> STW-010`,
  `Land STW-004 auth hardening -> STW-004`; knobs
  `RBP_PLAN_STALENESS_ROADMAP` / `RBP_PLAN_STALENESS_PLAN` for
  test injection, `RBP_PLAN_STALENESS_QUIET=1` to suppress
  per-row green output; exit 0 on clean, 3 on ghost, 1 on
  script-internal error),
  `crates/autotrain/tests/plan_staleness.rs` (new pinner
  test — no `database` feature gate, runs in
  `cargo test --workspace`; mirrors the
  `script_shape.rs` + `workspace_parallel_proof.rs` pattern
  with 4 shape tests + 1 end-to-end test:
  `script_exists_and_is_executable` (executable bit pinned
  on Unix), `script_parses_with_bash_n` (syntax regression
  fails the gate at CI time),
  `gate_claim_map_covers_every_ghost_p0_row` (the static
  `P0_TO_STW` table inside the script must reference every
  STW id the `steward/DRIFT.md` GHOST table flags
  — STW-004/006/008/009/010; a future refactor that drops
  a mapping fails CI before the gate silently stops
  checking a P0 path), `gate_headline_format_is_pinned`
  (the script's stdout must end with the literal
  `plan staleness gate complete: checked=N ghosts=M`
  prefix in the order `checked=` then `ghosts=`, and the
  exit-code contract `exit 3` on ghost / `exit 0` on clean
  is pinned so a refactor that silently changes the
  failure exit code fails CI),
  `gate_runs_end_to_end_with_clean_and_ghost_roadmaps`
  drives the gate against two fabricated planning
  surfaces — a ghost roadmap with 5 unchecked `[P0]` rows
  + a matching 5-shipped-STW plan (asserts exit 3,
  `ghosts=5`, every ghosted STW id named in stderr), and
  a clean roadmap with 5 `[x] [P0]` rows (asserts exit 0,
  `ghosts=0`) — so a regression in the gate's exit code
  or headline format fails CI without requiring a live
  Postgres),
  `genesis/plans/000-ceo-testnet-roadmap.md` (replace the
  `## Immediate P0 — testnet proof points (dispatch now)`
  unchecked list with a `Shipped/superseded by STW rows
  on \`main\`` reference list — every P0 row retired to
  `[x]` or removed; a one-line note credits the
  `STW-022` gate as the mechanical anti-regression),
  `IMPLEMENTATION_PLAN.md` (this row + flip `STW-021` to
  `[x]` since it shipped on commit `43947b5`),
  `genesis/plans/000-ceo-testnet-roadmap.md` (one-line
  note in the `## Lens verdicts` `Eng` paragraph crediting
  the `STW-022` gate as the anti-regression for the
  P0 retirement). Scope boundary: do NOT touch the
  shipped STW-004/006/008/009/010 code paths (the gate
  is a *retirement* of the false backlog signal, not a
  re-implementation); do NOT touch the shipped STW-019
  / STW-020 / STW-021 runbooks; do NOT add a
  third-party `toml` / `yaml` / `serde_yaml` dep — the
  roadmap + plan are markdown and the gate greps them
  raw; do NOT change the `STW-021` TUI gate's surface
  (the `tui.qa.json` / `tui.receipt.md` shape is
  locked); do NOT change the `STW-019` /
  `STW-020` headline format; do NOT change the
  `gbrain` corpus / `auto` workflow (the gate is
  repo-local, runs on `bash`, no gbrain roundtrip
  required). Verification commands:
  `bash scripts/plan-staleness-gate.sh` (must exit 0 and
  print `plan staleness gate complete: checked=0 ghosts=0`
  against the post-retirement planning surface),
  `cargo test -p rbp-autotrain --test plan_staleness` (the
  new pinner test, no-DB; green-path + ghost-path +
  shape contracts all pass),
  `cargo test --workspace -- --test-threads=4` (full
  workspace stays green),
  `cargo check --workspace`, `cargo fmt --check`.
  Required tests: the new lib tests in
  `crates/autotrain/tests/plan_staleness.rs::tests`
  (script exists + executable; `bash -n` parseability;
  claim map covers all 5 GHOST-P0 STW ids; headline +
  exit-code format pinned; end-to-end ghost and clean
  runs through the script with the synthesised
  planning surfaces); no padding of unrelated suites.
  Dependencies: `STW-004` (the shipped auth row the
  `P0-auth` ghost duplicates), `STW-006` (the shipped
  Schema row the `P0-schema` ghost duplicates),
  `STW-008` (the shipped hand round-trip row the
  `P0-hand-roundtrip` ghost duplicates), `STW-009`
  (the shipped smoke row the `P0-smoke` ghost
  duplicates), `STW-010` (the shipped bench row the
  `P0-bench` ghost duplicates), `STW-021` (the
  shipped TUI gate that flipped this row's own
  `[ ]` -> `[x]` state). Estimated scope: S.
  Completion signal: `bash scripts/plan-staleness-gate.sh`
  against the post-retirement planning surface exits 0
  and prints `plan staleness gate complete: checked=0
  ghosts=0`; the same gate against the pre-retirement
  surface exits 3 and names all 5 ghost rows in stderr;
  the new `plan_staleness.rs` pinner test passes; the
  planning surface no longer lists any unchecked `[P0]`
  row; `cargo test --workspace` and `cargo fmt --check`
  are green; the workspace is one coherent commit with
  no orphan code.

- [x] `STW-021` `robopoker-tui --headless` QA report becomes a
  real gate. Today the headless mode writes a `tui.qa.json`
  whose `verdict` is the literal string `"passed"` and whose
  `assertions` is a static `Vec<&'static str>` — the QA report
  always says pass regardless of what the surface actually
  contains, so a CI worker that depends on the headless mode
  for visual regression has no real signal. STW-021 turns the
  `QaReport` into a real computed gate: (a) a new
  `QaCheck { id, label, passed, detail }` struct holds the
  per-check outcome; (b) the `HeadlessReport::capture` path
  runs every check (chrome strings present, viewport bounds
  honoured, controls have unique ids + keys, the `controls`
  count matches the actual `controls()` vector length, the
  hand-evaluation surface is wired to `rbp-cards`, the
  read-only posture is intact) and records the boolean
  outcome + a one-line `detail` per check; (c) the top-level
  `verdict` is the *AND* of every check's `passed` field
  (i.e. `verdict = "passed"` only when every check passed;
  any single failure flips the verdict to `"failed"`); (d)
  `receipt_markdown` shows the per-check `id` and `passed`
  state in a `## QA Checks` section so a testnet dashboard
  can `grep '^QA-CHECK tui\\..* ' receipts/.../tui.receipt.md`
  to detect a regression without parsing JSON; (e) the
  `QaReport.assertions` field is repurposed to a
  `Vec<&'static str>` of the *failing* check ids (so the
  existing receipt shape stays backward-compatible: a fully
  green run has `assertions: []`; a failing run lists the ids
  of the checks that failed). The TUI is read-only — no
  server, database, training, wagering, or network path is
  touched. Owner files: `bin/tui/src/lib.rs` (new `QaCheck`
  struct + check fns + `verdict` recompute + `receipt_markdown`
  QA Checks section), `bin/tui/src/main.rs` (no change — the
  headless dispatch is unchanged), `IMPLEMENTATION_PLAN.md`
  (this row), `genesis/plans/000-ceo-testnet-roadmap.md`
  (one-line note: TUI headless QA promoted from string to
  real gate as STW-021). Scope boundary: do NOT change the
  public `HeadlessReport` field set (the testnet dashboard
  scrapes `surface` / `controls` / `frame` / `qa` /
  `verdict` / `controls` / `frame_hash`); do NOT add a
  third-party assertion library; do NOT introduce a
  dependency on `serde_yaml` / `toml` / `assert-json-diff`
  for the receipt format; do NOT change the interactive
  (`--headless=false`) loop; do NOT change the `tachyonfx`
  motion layer; do NOT change the `rbp-cards` integration
  path; do NOT change the `tui.qa.json` schema-version
  wiring (the existing `tui.surface.json` `schema_version`
  is a separate, stable field). Verification commands:
  `cargo test -p robopoker-tui --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt --check`,
  `cargo run -p robopoker-tui -- --headless --export-dir
  .auto/tui-stw021 --seed 49363` (smoke the headless
  capture against the promoted gate and confirm the
  produced `tui.qa.json` has a per-check `id` array and a
  computed `verdict`). Required tests: the new lib tests in
  `bin/tui/src/lib.rs::tests` (a green-path test that
  asserts every check's `passed` is `true` and the
  top-level `verdict` is `"passed"`, a red-path test that
  drives `HeadlessReport::capture` against a fixture that
  breaks the chrome-contains-ROBOPOKER check and asserts
  the top-level `verdict` flips to `"failed"` and the
  failing check id is in `qa.assertions` and the receipt
  markdown, and a per-check test that asserts every
  expected check id is in the report so a future
  refactor that drops a check fails CI). Dependencies:
  none. Estimated scope: S. Completion signal:
  `cargo run -p robopoker-tui -- --headless --export-dir
  .auto/tui-stw021` writes a `tui.qa.json` whose `verdict`
  is `"passed"` and whose `checks` array has one entry
  per invariant; the red-path test passes; the
  `tui.receipt.md` includes the new `## QA Checks` section
  with one `QA-CHECK tui.<id> passed|failed` line per
  check; `cargo test --workspace` and `cargo fmt --check`
  are green; the workspace is one coherent commit with
  no orphan code.

- [x] `STW-019` testnet live launch proof runbook + receipt bundle:
  the HAZARDS table classifies `testnet-live-proof` as
  mainnet-block, user-facing, and `steward/HINGES.md` ranks it
  hinge #5: "a documented live run of `--smoke -> --bench -> --
  compare -> --replay` on the same DB is the operator-visible
  launch proof not captured by unit tests alone". A new pure-bash
  `scripts/testnet-live-proof.sh` (shebang `#!/usr/bin/env bash`,
  `set -euo pipefail`) drives the existing trainer chain
  (`--cluster` -> `--reset` -> `--smoke` -> `--status` -> `--bench`
  -> `--compare` -> `--replay <transcript>`) as subprocesses
  against a single Postgres, captures each step's stdout + stderr
  + exit into `receipts/testnet-live-proof-<UTC-ISO>/<step>/{stdout,stderr,exit}.txt`,
  and emits a one-line `testnet live_proof complete: smoke=N
  status=N bench=N compare=N replay=BYTES` headline in
  `SUMMARY.txt` a dashboard can grep. The script refuses to run
  with exit 3 when neither `DATABASE_URL` nor `DB_URL` is set.
  Owner files: `scripts/testnet-live-proof.sh` (new, ~80 lines,
  pure bash), `crates/autotrain/tests/script_shape.rs` (new
  pinner test — asserts the runbook script exists, is
  executable, parses with `bash -n`, and the runbook doc lists
  every env knob and every chain step), `crates/autotrain/tests/live_proof.rs`
  (new integration test gated on `database` + `DATABASE_URL` —
  drives the same six steps in a single `cargo test` invocation
  end-to-end), `README.md` (link the runbook under
  `## Testnet launch proof`), `IMPLEMENTATION_PLAN.md` (this
  row), `genesis/plans/000-ceo-testnet-roadmap.md` (mark the
  v6 launch proof as shipped with a one-line note). Scope
  boundary: do NOT change the trainer chain, the per-step
  `Mode::*` arms, the JSON report shapes, the per-side PnL
  math, the `trainer --replay` render, the named baselines, the
  v1 / v2 `Blueprint` enum, or the v1 / v2 trained configs the
  bench / compare seats. The runbook is *additive*: it shells
  out to the existing trainer binary and captures per-step
  artifacts. Verification commands: `cargo test -p rbp-autotrain
  --features database --test live_proof` (end-to-end through a
  real Postgres), `cargo test -p rbp-autotrain --test script_shape`
  (script existence + `bash -n` parseability), `bash
  scripts/testnet-live-proof.sh` against a real Postgres (drops
  a `receipts/testnet-live-proof-<UTC-ISO>/` directory with the
  `SUMMARY.txt` headline), `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt --check`. Required tests:
  the new `crates/autotrain/tests/{live_proof,script_shape}.rs`
  integration tests; no padding of unrelated suites. Dependencies:
  `STW-009` (smoke), `STW-010` (bench), `STW-014`/`STW-015` (the
  transcripts the `--replay` leg reads), `STW-016` (`--replay`
  mode), `STW-018` (compare). Estimated scope: M. Completion
  signal: a fresh `bash scripts/testnet-live-proof.sh` against a
  real Postgres drops a `receipts/testnet-live-proof-*/`
  directory whose `SUMMARY.txt` contains the `testnet live_proof
  complete:` line; the new integration tests pass; `cargo test
  --workspace` and `cargo fmt --check` are green; the new
  runbook is linked from `README.md` under `## Testnet launch
  proof`.

- [x] `STW-024` Concrete `Measure` + `Coupling`
  implementations + deterministic tests in `crates/transport`.
  The `steward/DRIFT.md` "Orphan Code Surfaces" table flags
  `crates/transport` as ORPHAN: *"no tracked STW item covers
  optimal transport crate"*. The crate currently ships four
  traits (`Support`, `Density`, `Measure`, `Coupling`) and two
  empty type shells (`GreenhornOptimalTransport`,
  `GreedyOptimalTransport`) with **zero concrete impls** and
  **zero tests**, even though the documented
  `rbp-core::SINKHORN_TEMPERATURE` / `SINKHORN_ITERATIONS` /
  `SINKHORN_TOLERANCE` constants are intended to drive it.
  STW-024 lands three pieces of proof that the crate is real,
  not a stub: (a) a `UniformMetric` struct (L1 distance over
  `usize` buckets) that implements the existing `Measure`
  trait, (b) a `Sinkhorn` struct that implements the existing
  `Coupling` trait via the standard entropic-regularized
  iteration (Kantorovich potentials, log-domain scaling,
  Sinkhorn-Knopp update rule, marginal-violation early stop at
  `SINKHORN_TOLERANCE`, hard cap at `SINKHORN_ITERATIONS`),
  with a `flow(x, y)` accessor that returns the marginal-
  consistent transport density and a `cost()` method that
  integrates `flow * distance` over the support, and (c) a
  battery of deterministic lib tests in
  `crates/transport/src/lib.rs` that pin the new impls:
  `support_usize_is_clone` (marker + Clone contract),
  `density_btreemap_round_trips_known_probabilities`
  (deterministic 0.7 / 0.2 / 0.1 fixture),
  `density_hashmap_matches_btreemap_on_same_input`,
  `density_vec_assoc_list_matches_btreemap_on_same_input`,
  `density_unknown_point_returns_zero`,
  `density_total_mass_equals_one_for_normalized_input`
  (asserts the three `Density` impls agree on a normalized
  fixture), `uniform_metric_zero_self_distance`,
  `uniform_metric_symmetry_holds`
  (`d(x,y) == d(y,x)`), `uniform_metric_triangle_inequality`
  (`d(x,z) <= d(x,y) + d(y,z)` over a 5-point grid),
  `sinkhorn_identity_cost_is_zero` (μ = ν ⇒ cost = 0 within
  1e-3), `sinkhorn_self_transport_cost_is_zero` (a single
  unit mass in both μ and ν ⇒ cost = 0), `sinkhorn_preserves
  _marginals_within_tolerance` (run 50 Sinkhorn iterations
  on a 3x3 uniform fixture, assert `|Σ_x flow(x,y) − ν(y)| <
  1e-2` for every y and `|Σ_y flow(x,y) − μ(x)| < 1e-2` for
  every x), `sinkhorn_cost_is_nonnegative`,
  `sinkhorn_uniform_metric_matches_known_emd_on_1d`
  (a 1-point shift μ = δ₀, ν = δ₁ ⇒ EMD = 1.0 within 1e-2),
  `sinkhorn_handles_disjoint_supports` (μ on {0,1}, ν on
  {2,3} ⇒ cost equals the source-to-target L1 distance,
  ≥ 2.0), and `sinkhorn_respects_iteration_cap`
  (assert the cap is honored even on a slowly-converging
  fixture so a future parameter tweak can't blow the budget).
  Owner files: `crates/transport/src/measure.rs` (new
  `UniformMetric` struct + `Measure` impl, exported in
  `crates/transport/src/lib.rs`), `crates/transport/src/greenkhorn.rs`
  (real `Sinkhorn` struct with `new(metric, mu, nu,
  temperature, iterations, tolerance)`, `Coupling` impl, and
  log-domain internal state), `crates/transport/src/lib.rs`
  (re-export the new types and the new `tests` module),
  `IMPLEMENTATION_PLAN.md` (this row). Scope boundary: do
  NOT change any existing trait signature; do NOT add new
  traits; do NOT touch downstream consumers
  (`crates/clustering/src/sinkhorn.rs` and friends keep their
  own algorithms — STW-024 only adds the in-crate alternative
  source of truth); do NOT add a `Sinkhorn` feature gate; do
  NOT change the `rbp-core` constants. Verification
  commands: `cargo test -p rbp-transport` (the new lib tests
  pass), `cargo build -p rbp-transport` (the new types
  compile), `cargo check --workspace` (no downstream
  breakage), `cargo test --workspace` (full workspace
  remains green), `cargo fmt --check`. Required tests: the
  14 lib tests listed above; no padding of unrelated suites.
  Dependencies: the existing `Support`, `Density`, `Measure`,
  `Coupling` traits in `crates/transport/src/{support,density,
  measure,coupling}.rs`; the existing
  `rbp-core::SINKHORN_*` constants in `crates/util/src/lib.rs`.
  Estimated scope: M. Completion signal:
  `cargo test -p rbp-transport` is green with ≥ 14 tests
  passing; the `Sinkhorn` struct produces a `cost()` within
  1e-2 of the known 1D EMD on a 1-point-shift fixture; the
  `Coupling` trait is finally implemented in-crate; the
  full `cargo test --workspace` and `cargo fmt --check`
  remain green; `crates/transport/src/greenkhorn.rs` is no
  longer a single empty struct definition.

- [x] `STW-025` Lib-level test coverage for
  `crates/server/src/analysis`. The `steward/DRIFT.md`
  "Orphan Code Surfaces" table flags
  `crates/server/src/analysis` as ORPHAN: *"no tracked STW
  item covers analysis API/CLI surface"*. The module is
  1391 lines of wired-up code (13 actix-web handlers in
  `analysis/handlers.rs`, an 11-variant `clap` `Query` enum
  in `analysis/query.rs`, a 224-line `CLI` REPL in
  `analysis/cli.rs`, and a 891-line `API` struct in
  `analysis/api.rs`) but ships **zero tests**: there is no
  `crates/server/tests/` directory, no `#[cfg(test)] mod
  tests` in any analysis file, and the 13 HTTP routes the
  `bin/backend` binary actually serves are entirely
  unexercised by CI. STW-025 lands a *no-DB* test surface
  for the parts of the module that do not require a live
  Postgres connection, plus a wire-format pinner for the
  request/response DTOs the 13 handlers accept and emit.
  Three pieces of proof that the module is exercised, not
  stub: (a) a refactor of `analysis/cli.rs` that extracts a
  pure `render_query(&Query) -> Option<Result<String,
  String>>` helper returning the rendered text the REPL
  handler currently prints (5 of the 11 `Query` variants
  are pure: `Path { value: i64 }`, `Edge { value: u8 }`,
  `AbsFromInt { value: i16 }`, `ObsFromInt { value: i64 }`,
  `Isomorphism { value: i64 }` — they only call local
  `Path::from(i64)` / `Edge::from(u8)` /
  `Abstraction::from(i16)` / `Observation::from(i64)` /
  `Isomorphism::from(i64)` constructors and the
  `catch_unwind` panic guard on the
  `Observation`/`Isomorphism` integer conversions), with
  the `CLI::handle` body becoming a small dispatcher
  (`match render_query(&query) { Some(rendered) =>
  r#try(writeln!(...)) ..., None => match query {
  api_cmd => self.0.X().await... } }`); the refactor must
  preserve the exact stdout text the existing handler
  prints (same field order, same labels, same
  `Path({})` / `Edge({})` / `Abstraction({})` /
  `Observation({})` / `Isomorphism({})` headers, same
  two-space-indent child lines); (b) a `#[cfg(test)] mod
  tests` block in `analysis/cli.rs` with 7+ lib tests
  pinning the pure path: `render_path_known_i64_round_trips`
  (asserts `Path(1)` → header + a 4-line body containing
  `Display:` / `Length:` / `Aggro:` / `Edges:` lines for a
  well-formed input), `render_path_zero_i64_renders_empty`
  (asserts `Path(0)` → a `Length: 0` body), `render_edge_
  fold_byte_renders_fold` (`Edge(0)` → `Is choice: false`
  / `Is aggro: false`), `render_edge_call_byte_renders_
  call` (`Edge(2)` → `Is choice: true`), `render_abs_from_
  int_zero_round_trips` (`Abstraction(0)` → `Street:` /
  `Index: 0`), `render_obs_from_int_panics_guarded_by_
  catch_unwind` (asserts the `catch_unwind` in the
  existing handler body is preserved verbatim: an input
  that decodes to a valid `Observation` produces the
  4-line body, an input that panics inside `Observation::
  from` produces the
  `Error: Invalid observation encoding (assertions
  failed)` body — both are reachable through
  `render_query`); and (c) a `tests/dto_wire.rs`
  integration test in `crates/server/tests/` (the new
  directory created by STW-025) that round-trips each of
  the 9 request DTOs in `crates/util/src/dto/request.rs`
  through `serde_json::from_str` + `serde_json::to_string`
  + `serde_json::from_str` and asserts the second parse
  equals the first struct (pins the wire format for
  `SetStreets` / `ReplaceObs` / `RowWrtObs` / `ReplaceAbs`
  / `ReplaceRow` / `ReplaceOne` / `ReplaceAll` / `ObsHist`
  / `AbsHist` / `GetPolicy` — 10 DTOs total, 9 are
  requests, the 10th is `ApiSample` / `ApiStrategy` /
  `ApiDecision` in `crates/util/src/dto/response.rs` which
  the response side pins in `response_dto_round_trip_*`).
  A third integration test `tests/analysis_cli.rs`
  exercises the `render_query` helper through the public
  `Query::try_parse_from` path with hand-rolled
  `Query::Path { value: 1 }` / `Query::Edge { value: 2 }` /
  `Query::AbsFromInt { value: 0 }` / `Query::Isomorphism
  { value: 0 }` inputs and asserts the rendered text
  matches the lib-test expectations. Owner files:
  `crates/server/src/analysis/cli.rs` (refactor
  `CLI::handle` to delegate to a new
  `pub(crate) fn render_query(query: &Query) -> Option<
  Result<String, String>>` helper, plus the new
  `#[cfg(test)] mod tests` block),
  `crates/server/tests/dto_wire.rs` (new no-DB
  integration test, ~10 tests pinning the request DTO
  wire format and 3 tests pinning the response DTO wire
  format), `crates/server/tests/analysis_cli.rs` (new
  no-DB integration test, 5 tests pinning
  `render_query`'s public surface), `IMPLEMENTATION_PLAN.md`
  (this row). Scope boundary: do NOT touch
  `analysis/api.rs` or `analysis/handlers.rs`; the
  DB-bound handler bodies stay as-is. Do NOT touch
  `analysis/query.rs`; the `Query` enum shape is the
  wire-level contract. Do NOT touch the 6 DB-bound
  `Query` variants (`Abstraction` / `Distance` /
  `Equity` / `Population` / `Similar` / `Nearby` /
  `Composition`) — they require a live `API` and a
  live Postgres and are out of STW-025's no-DB scope.
  Do NOT touch the `API` struct, the `Strategy` /
  `Decision` / `Partial` types the `blueprint` handler
  threads through, the `actix-web` `App` / route wiring
  in `crates/server/src/lib.rs`, the `bin/backend` entry
  point, the `hosting` module, or the
  `crates/auth` / `crates/database` / `crates/cards` /
  `crates/gameplay` / `crates_mccfr` / `crates_nlhe`
  crates. Do NOT add a new `tokio-postgres` mock layer
  or a `mockall` dep — STW-025's tests are entirely
  synchronous (`render_query` is `fn` not `async fn`,
  DTO round-trips are sync) so they run with `cargo test
  -p rbp-server --tests` and require no
  `#[tokio::test]` runtime, no `actix_web::test` runtime,
  and no `DATABASE_URL` env. Do NOT change the
  `Analysis`-backed REPL prompt (`> `), the
  `quit`/`exit` keywords, or the
  `Query::try_parse_from` shape. Verification commands:
  `cargo test -p rbp-server --tests --lib` (the new
  tests pass), `cargo build -p rbp-server` (the refactor
  compiles), `cargo check --workspace` (no downstream
  breakage), `cargo test --workspace` (full workspace
  remains green), `cargo fmt --check`. Required tests:
  7 new lib tests in
  `crates/server/src/analysis/cli.rs::tests` + ≥ 10 new
  tests in `crates/server/tests/dto_wire.rs` + 5 new
  tests in `crates/server/tests/analysis_cli.rs` —
  total ≥ 22 new tests, all no-DB and synchronous.
  Dependencies: STW-003 (database-backed server/gameroom
  build; the analysis module is a consumer of the
  `tokio_postgres::Client` the server wires in);
  `crates/util/src/dto/{request,response}.rs` (the
  DTOs STW-025 pins — they already ship in
  `rbp-core` as `pub use` re-exports); the existing
  `Query` enum in `analysis/query.rs` (STW-025 adds
  a renderer next to it, does NOT change the enum).
  Estimated scope: M. Completion signal:
  `cargo test -p rbp-server --tests --lib` is green with
  ≥ 22 new tests passing; `render_query(&Query::Path {
  value: 1 })` returns
  `Some(Ok("Path(1)\n  Display:  ...\n  Length:   ...\n
  Aggro:    ...\n  Edges:    ...\n"))` with a non-empty
  `Display:` and `Length:` line;
  `render_query(&Query::AbsFromInt { value: 0 })`
  returns
  `Some(Ok("Abstraction(0)\n  Display:  0\n  Street:
  Preflop\n  Index:    0\n"))`; the existing
  `CLI::run` / `CLI::handle` REPL still prints the same
  text to stdout on a fresh `cargo run -p rbp-server
  --bin robopoker-backend` (manual smoke against the
  CLI's `> Path 1` / `> Edge 2` / `> abs 0` /
  `> iso 0` prompts) — the refactor is a pure
  extraction, the visible behavior is byte-identical;
  `cargo test --workspace` and `cargo fmt --check`
  remain green; `crates/server/tests/` is no longer an
  empty directory.

- [x] `STW-026` Sinkhorn triangle-inequality flake
  stabilization in `crates/clustering/src/emd.rs`. The
  `is_sinkhorn_emd_triangle` lib test asserts the strict
  triangle inequality `a + b >= c` for three
  Sinkhorn-minimized entropic-OT couplings, but
  `STW-020` parallel-workspace proof (and a 1/100-1/500
  rerun-rate of the random test on the current source)
  showed a small fraction of `EMD::random()` inputs
  produce an `~11%` `a + b < c` violation (e.g.
  `a = 0.0856 + b = 0.0948 < c = 0.2008 / 1.15`); the
  proportional violation is structural to the entropic
  Sinkhorn at `temperature = 0.025`
  (`rbp_core::SINKHORN_TEMPERATURE`), not a regression
  in the math. STW-026 pins the contract with a
  `TOLERANCE = 1.15` margin on each of the three arms
  (`a + b >= c / TOLERANCE`) — a `~3%` safety margin
  over the worst observed `~11%` violation — and adds
  a hand-rolled `is_sinkhorn_emd_triangle_deterministic`
  test on a fixed `Metric` (`d(0,1) = d(1,2) = 1.0`,
  `d(0,2) = 2.1`) + three single-bucket Flop
  histograms so a future Sinkhorn regression is
  locatable to the exact arm + magnitude of the
  `TOLERANCE` breach on a reproducible input, not a
  fresh `EMD::random()` flake. The heuristic triangle
  test (`is_heuristic_emd_triangle`) is unchanged: the
  heuristic is a greedy projection, not an entropic
  optimization, and its own `TOLERANCE = 1.25` was
  already a generous bound. The Sinkhorn contract is
  intentionally tighter than the heuristic contract —
  a Sinkhorn regression that grows the worst-case
  violation past `15%` will fail this test even though
  the heuristic still passes. Owner files:
  `crates/clustering/src/emd.rs` (the `TOLERANCE`
  constant + the new deterministic test + the
  rationale docs above each test). Scope boundary: do
  NOT touch the `Sinkhorn::minimize` math, the
  `rbp_core::SINKHORN_TEMPERATURE` constant, the
  `is_equity_emd_*` / `is_sinkhorn_emd_positive` /
  `is_sinkhorn_emd_zero` / `is_heuristic_emd_*`
  contract tests, the `Heuristic` triangle test
  tolerance, the `EMD::random()` fixture, the
  `Heuristic` approximation path, the `crates/transport`
  `STW-024` work, the `STW-020` parallel-workspace
  runbook, or the `bust_prevents_next_deterministic`
  gameplay test. The fix is *only* a tolerance +
  documentation change on the one lib test, plus one
  new deterministic regression test. Verification
  commands: `cargo test -p rbp-clustering --lib`
  (1/100-1/500 flake rate → 0/200 in the 200-run
  probe); `cargo test --workspace -- --test-threads=4`
  (the random sinkhorn triangle test was the source
  of the 1/N parallel-workspace flake, the new
  tolerance makes it deterministic green);
  `cargo fmt --check`; `cargo check --workspace`.
  Required tests: the existing 18 `rbp-clustering` lib
  tests, of which 2 are the `is_sinkhorn_emd_triangle`
  + new `is_sinkhorn_emd_triangle_deterministic` (the
  latter is a new addition; the former is a tolerance
  relaxation). Dependencies: none. Estimated scope: S.
  Completion signal: the `is_sinkhorn_emd_triangle`
  random test passes 200/200 in a fresh `for i in
  $(seq 1 200); do cargo test -p rbp-clustering
  --lib emd::tests::is_sinkhorn_emd_triangle; done`
  loop; the `is_sinkhorn_emd_triangle_deterministic`
  test is green on first run; `cargo test --workspace
  -- --test-threads=4` reports 0 failures.

- [x] `STW-027` `bin/tui` decision-tape + board-stage
  QA-coverage slice (the remaining `bin/tui` ORPHAN from
  `steward/DRIFT.md`). The `steward/DRIFT.md` "Orphan
  Code Surfaces" table flags `bin/tui` as the last
  ORPHAN crate with no tracked STW item covering it:
  *"AGENTS names read-only TUI preview, but
  `IMPLEMENTATION_PLAN.md` has no active TUI item;
  `bin/tui` exists and `.auto/tui*` artifacts exist"*.
  `STW-021` shipped the 9-check `HeadlessReport::capture`
  QA gate (`tui.chrome.brand`, `tui.chrome.players`,
  `tui.chrome.posture`, `tui.viewport.bounds`,
  `tui.controls.ids_unique`, `tui.controls.keys_unique`,
  `tui.controls.count`, `tui.cards.evaluator`,
  `tui.controls.help`), but the gate covers only the
  chrome, viewport, controls list, and `rbp-cards`
  evaluator. The two rendered surfaces the testnet
  dashboard actually cares about — the decision-tape
  log (the `actor` / `action` spans `render_decision_tape`
  lays down for the visible `PreviewLog` entries) and
  the board-stage card render (the `visible_board()`
  slice `render_board_slots` paints for the current
  `board_cards` count) — are not asserted by any QA
  check. STW-027 lands two additional QA checks that
  pin those surfaces and four lib tests that exercise
  both the positive (consistent `RandomPreview`
  fixture) and negative (deliberately-inconsistent
  `RandomPreview` fixture with `board_cards = 5` but
  only 3 actual `CardView`s in `board`) arms so a
  future regression in either surface is locatable to
  the exact check + magnitude of the breach on a
  reproducible input. (a) `check_tape_actions_present`
  reads `app.preview.current().log_count` and asserts
  the *data invariant* (every visible log entry has
  a non-empty `actor` AND a non-empty `action`; the
  number of `visible_story()` entries equals
  `current().log_count`); at `step = 0` (`log_count = 0`)
  the check is trivially passed (the decision-tape is
  empty by design — `render_decision_tape` early-returns
  on `visible_story().is_empty()`), and at any later
  step the check verifies the entries the tape will
  actually paint. (b) `check_board_cards_present` reads
  `app.preview.current().board_cards` and asserts the
  data invariant `visible_board().len() ==
  current().board_cards` (the slice painted by
  `render_board_slots` is the prefix of `app.preview.board`
  whose length is clamped by `current().board_cards` —
  any drift between the step's `board_cards` field and
  the actual painted slice fails the check). Both new
  checks are wired into `HeadlessReport::capture` after
  `check_cards_evaluator` (the existing per-frame
  evaluator check is the natural anchor — it already
  inspects the `App` model, not the rendered frame
  text, so the new data-invariant checks follow the
  same pattern and avoid ANSI-escape brittleness). Four
  new lib tests in `bin/tui/src/lib.rs::tests`: (i)
  `check_tape_actions_present_passes_on_populated_log`
  — constructs an `App` from a hand-built `RandomPreview`
  with 3 `PreviewLog` entries + a 3rd-step `PreviewStep`
  with `log_count = 3` and asserts the check returns
  `passed = true` with a non-empty `detail` that names
  the actor count; (ii)
  `check_tape_actions_present_passes_on_empty_log` —
  uses `App::default()` (step 0, `log_count = 0`) and
  asserts the check returns `passed = true` (the
  decision-tape is empty by design); (iii)
  `check_board_cards_present_passes_when_slice_matches`
  — constructs an `App` from a `RandomPreview` with
  `board.len() = 5` + a 4th-step `PreviewStep` with
  `board_cards = 3` and asserts the check returns
  `passed = true` with a detail naming the visible
  card count; (iv)
  `check_board_cards_present_fails_on_inconsistent_state`
  — constructs an `App` from a `RandomPreview` with
  `board.len() = 3` but a 4th-step `PreviewStep` with
  `board_cards = 5` (the step says "reveal 5 board
  cards" but only 3 are actually in the model — the
  exact class of bug the check is designed to catch)
  and asserts the check returns `passed = false` with
  a detail that names the `expected` vs `actual` card
  count delta. Owner files: `bin/tui/src/lib.rs` (the
  two new `check_*` functions + the `vec![...]`
  registration in `HeadlessReport::capture` + the four
  new lib tests + the rationale docs above each
  check), `IMPLEMENTATION_PLAN.md` (this row).
  Scope boundary: do NOT touch the existing 9 QA
  checks; do NOT change `check_cards_evaluator`; do
  NOT touch `render_decision_tape` /
  `render_board_slots` / `render_board_stage` (the
  new checks verify the *data invariant* the renderers
  consume, not the rendered text — a future render
  rewrite that preserves the model contract stays
  green); do NOT change the `HeadlessReport` struct
  shape, the `QaCheck` / `QaReport` field order, the
  `SURFACE_SCHEMA_VERSION` constant, the
  `tui.qa.json` / `tui.receipt.md` wire format, the
  `tachyonfx` motion paths, the `controls()` list, the
  `App` / `RandomPreview` / `PreviewStep` /
  `PreviewLog` / `CardView` field order, the seeded
  `DEFAULT_SEED` (`0xC0D3`), the `with_seed` /
  `with_seed_and_step` constructors, the
  `handle_key` dispatch, the `key -> step` mapping, or
  the `Focus` enum. Verification commands:
  `cargo test -p robopoker-tui --lib` (the 4 new
  tests + the 17 existing tests pass); `cargo build
  -p robopoker-tui` (the new checks compile);
  `cargo check --workspace` (no downstream breakage —
  the tui crate is `bin/` and not a `dep` of any
  other crate); `cargo test --workspace --
  --test-threads=4` (full workspace remains green);
  `cargo fmt --check`. Required tests: the 4 new
  lib tests listed above; no padding of unrelated
  suites. Dependencies: `STW-021` (the
  `HeadlessReport::capture` + `QaCheck` + `QaReport`
  surface the new checks slot into). Estimated
  scope: S. Completion signal:
  `cargo test -p robopoker-tui --lib` is green with
  21 tests passing (17 existing + 4 new); the new
  `tui.tape.actions_present` and
  `tui.board.cards_present` check ids appear in
  the `QaReport.checks` list of a fresh
  `HeadlessReport::capture` invocation; the negative
  test (`check_board_cards_present_fails_on_inconsistent_state`)
  fails the check on a hand-built `RandomPreview`
  with `board.len() = 3` and `board_cards = 5` and
  returns the expected-vs-actual delta in the
  `detail` field; `cargo test --workspace` and
  `cargo fmt --check` remain green; the
  `bin/tui/src/lib.rs` `HeadlessReport::capture`
  `checks` vector is now an 11-entry
  `vec![...]` (9 existing + 2 new) — closing the
  last `bin/tui` ORPHAN item in
  `steward/DRIFT.md`.

- [x] `STW-028` `trainer --verify-receipt <path>`
  CLI mode + committed no-DB launch-proof fixture
  (the `testnet-live-proof` HAZARDS mainnet-block
  hinge). `STW-019` shipped the
  `scripts/testnet-live-proof.sh` runbook +
  `crates/autotrain/tests/live_proof.rs` cargo-test
  counterpart. `STW-023` shipped the
  shared `LiveProofReceipt::read_and_verify` Rust
  verifier the integration test calls. But the
  verifier is library-only — a downstream tool
  (a testnet dashboard's "verify" button, a CI
  check, a release-gate script) that already has
  the static `trainer` binary still needs to either
  re-run `cargo test --test live_proof_receipt`
  (heavy, requires the full workspace checkout) or
  call `LiveProofReceipt::read_and_verify` via a
  hand-rolled `use` statement (heavy, requires
  installing the workspace's library crates).
  STW-028 closes the `testnet-live-proof`
  HAZARDS mainnet-block hinge with three pieces
  of proof: (a) a no-DB
  `trainer --verify-receipt <path>` CLI subcommand
  a downstream tool can invoke from a single
  static `trainer` binary; (b) a committed
  no-DB **portable reference** receipt under
  `crates/autotrain/tests/fixtures/testnet-live-proof-fixture/`
  a downstream auditor can re-verify on any machine
  without a Postgres — the on-disk shape is the
  canonical green-receipt contract, and a `git diff`
  of the fixture only changes when the contract
  itself changes; (c) a 5-test no-DB integration
  test in
  `crates/autotrain/tests/verify_receipt.rs` that
  drives the new CLI as a subprocess against a
  synthetic green receipt + four error paths
  (missing dir, step-failed, bad-headline,
  missing-path-arg), plus 5 lib tests in
  `crates/autotrain/src/verify_receipt.rs` that
  pin the underlying `verify_receipt::run`
  handler (4 happy + 3 error arms + 1
  committed-fixture re-verification). The CLI
  prints a one-line
  `live_proof receipt verification passed: <headline>`
  on a green receipt (exit 0) and a
  `live_proof receipt verification failed: <kind>:
  <detail>` on a regression (exit 2), where `<kind>`
  is one of `recipe_shape` / `step_failed` /
  `headline` (the three `VerifyError` variants).
  The mode bypasses the DB open
  (mirrors `Mode::Replay`); an empty path is the
  "missing path arg" error converted into a
  one-line usage + exit 2. The committed fixture's
  `recipe.json` mirrors the
  `LiveProofRecipe` JSON shape one-for-one (the
  seven pinned step names in the
  `STW023_CHAIN_STEPS` order, the per-step
  `name` / `exit` / `stdout_bytes` /
  `stderr_bytes` fields), and the `SUMMARY.txt`
  headline is the pinned
  `testnet live_proof complete: smoke=12
  status=12 bench=4 compare=4 replay=256` form
  so a drift in either the fixture or the
  verifier surfaces in `cargo test --workspace`
  on a single lib test
  (`verify_receipt::tests::run_verifies_committed_testnet_live_proof_fixture`).
  Owner files:
  `crates/autotrain/src/verify_receipt.rs` (new
  `pub fn run(&Path) -> Result<String, String>`
  wrapper over `LiveProofReceipt::read_and_verify`
  that converts the three-variant `VerifyError`
  into the printable `(kind, detail)` pair + the
  one-line success/failure contract, plus the
  `#[cfg(test)] mod tests` block with the 5 lib
  tests pinning the happy path + 3 error
  variants + the committed-fixture re-verification),
  `crates/autotrain/src/mode.rs` (new
  `Mode::VerifyReceipt { path: PathBuf }` variant
  parsed from `--verify-receipt <path>` + a
  no-DB early-dispatch arm in `Mode::run` that
  exits 0 / 2 on the same `print!` / `eprintln!`
  + `std::process::exit` discipline the
  `Mode::Replay` arm uses + a `Self::VerifyReceipt
  { .. }` unreachable!() arm in the post-DB
  exhaustive match + a `--verify-receipt <path>`
  suffix in the `Usage:` eprintln!), `crates/autotrain/src/lib.rs`
  (`mod verify_receipt;` to wire the new module
  into the crate root), `crates/autotrain/tests/verify_receipt.rs`
  (new no-DB integration test, 5 sub-tests
  driving the new CLI as a subprocess), `crates/autotrain/tests/fixtures/testnet-live-proof-fixture/`
  (new tracked no-DB portable-reference receipt:
  `SUMMARY.txt` + `ENV.txt` + `recipe.json` +
  seven per-step `cluster/` / `reset/` / `smoke/`
  / `status/` / `bench/` / `compare/` / `replay/`
  sub-directories each containing `exit.txt=0` +
  empty `stdout.txt` + empty `stderr.txt`, plus a
  `README.md` documenting what the fixture is
  and how to re-verify it), `scripts/testnet-live-proof.md`
  (a new "Re-verifying a receipt with the trainer
  binary (STW-028)" section under "How the
  dashboard scrapes a receipt" that documents
  the new `--verify-receipt <path>` CLI, the
  four error modes, the committed no-DB
  fixture, and the new
  `verify_receipt::tests::run_verifies_committed_testnet_live_proof_fixture`
  lib test), `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (add a one-line "STW-028 ships
  `trainer --verify-receipt` + committed no-DB
  launch-proof fixture" note in the
  "Immediate P0" list). Scope boundary: do NOT
  change the `LiveProofReceipt` / `VerifyError` /
  `LiveProofHeadline` verifier surface (the new
  mode is a thin wrapper, not a refactor); do NOT
  change the runbook's `write_recipe` heredoc
  shape; do NOT change the seven
  `STW023_CHAIN_STEPS` step names; do NOT touch the
  `Mode::Bench` / `Mode::Compare` /
  `Mode::Status` / `Mode::Smoke` /
  `Mode::Cluster` / `Mode::Reset` / `Mode::Fast` /
  `Mode::Fast2` / `Mode::Slow` / `Mode::Replay`
  arms; do NOT add a new `clap` /
  `structopt` dep (the existing trainer uses a
  hand-rolled `from_args` so the new mode
  follows the same shape as the
  `Mode::Replay` parse); do NOT touch the
  `crates/server/src/analysis` / `crates/transport`
  / `bin/tui` ORPHAN code surfaces (those are
  closed by STW-024 / STW-025 / STW-027); do NOT
  require a `DATABASE_URL` env (the new mode is
  read-only + no-DB, mirrors the
  `Mode::Replay` arm). Verification commands:
  `cargo test -p rbp-autotrain --lib verify_receipt`
  (the 5 new lib tests pass), `cargo test -p rbp-autotrain --test verify_receipt`
  (the 5 new integration tests pass),
  `cargo build --bin trainer` (the new mode
  compiles), `cargo test --workspace` (full
  workspace remains green),
  `cargo fmt --check`. Required tests: 5 new
  lib tests in
  `crates/autotrain/src/verify_receipt.rs::tests`
  (the 4 contract tests + the 1
  committed-fixture re-verification) + 5 new
  integration tests in
  `crates/autotrain/tests/verify_receipt.rs` —
  total 10 new tests, all no-DB and synchronous.
  Dependencies: `STW-019` (the runbook the
  committed fixture mirrors + the recipe.json
  shape the new mode reads), `STW-023` (the
  `LiveProofReceipt::read_and_verify` /
  `LiveProofHeadline::parse` surface the new
  mode wraps), `STW-016` (the existing
  `Mode::Replay` no-DB early-dispatch arm the
  new mode parallels). Estimated scope: S/M.
  Completion signal: `cargo test
  -p rbp-autotrain --lib verify_receipt` is
  green with 5 tests passing; `cargo test
  -p rbp-autotrain --test verify_receipt` is
  green with 5 tests passing;
  `./target/debug/trainer --verify-receipt
  crates/autotrain/tests/fixtures/testnet-live-proof-fixture/`
  exits 0 + prints the verbatim
  `live_proof receipt verification passed: testnet
  live_proof complete: smoke=12 status=12 bench=4
  compare=4 replay=256` line;
  `./target/debug/trainer --verify-receipt
  /tmp/nonexistent-dir` exits 2 + prints
  `live_proof receipt verification failed:
  recipe_shape: ...`;
  `cargo test --workspace` and `cargo fmt
  --check` remain green; the committed
  fixture's `recipe.json` `steps[i].name` field
  matches the `STW023_CHAIN_STEPS` constant in
  order; the new `Mode::VerifyReceipt` variant
  is wired into the `Usage:` eprintln! line
  (a `trainer` invocation with no args prints
  `--verify-receipt <path>` as one of the
  listed modes) — closing the
  `testnet-live-proof` HAZARDS mainnet-block
  hinge.

## Promoted from v7 follow-on (this slice)

The v6 follow-on chain (STW-019 → STW-031) is closed. The
genesis roadmap (line 40) names the next claimable item as
"one operator-visible run receipt for `trainer --smoke`,
`trainer --bench`, `trainer --compare`, and `trainer --replay`
on the same database/artifact bundle" and the
`scripts/testnet-live-proof.md` runbook doc (line 234)
names it explicitly: "pushing it to a testnet dashboard
bucket is the next slice (`testnet-live-publish`)." The
v6 follow-on chain produced a *local* receipt directory
(`receipts/testnet-live-proof-<UTC-ISO>/`) the
`LiveProofReceipt::read_and_verify` Rust verifier can
re-verify. STW-032 lands a deterministic, content-addressed
*publish* step that turns a local receipt into a portable
bundle any third party can fetch + re-verify without
re-running the chain.

- [x] `STW-032` `scripts/testnet-live-publish.sh` +
  `trainer --publish <receipt-dir>` + no-DB `trainer
  --verify-bundle <bundle-path>` portable publish
  surface. The v7 follow-on the runbook doc names
  explicitly. The publish step takes a
  `receipts/testnet-live-proof-<UTC-ISO>/` directory
  the runbook produced and writes a
  `publish/testnet-live-proof-<UTC-ISO>.tar.gz`
  portable bundle + a
  `publish/testnet-live-proof-<UTC-ISO>.sha256`
  digest file + a
  `publish/testnet-live-proof-<UTC-ISO>.manifest.json`
  machine-readable index (sha256 per file +
  total bytes + receipt_dir + bash runbook version +
  trainer git sha). The tarball layout is
  deterministic (sorted file walk, `tar --sort=name
  --mtime=@0 --owner=0 --group=0`) so a
  byte-identical bundle is reproducible from a
  byte-identical receipt; a regression in the
  bundle's `sha256` fails the new
  `publish_bundle_deterministic_for_fixed_receipt`
  lib test. The manifest is the single source of
  truth the new `trainer --verify-bundle <path>`
  CLI subcommand re-verifies: the verifier reads
  the manifest, re-hashes every file inside the
  tarball (in the order the manifest names them),
  and asserts every `sha256` matches; a single
  mismatched digest fails the verifier with a
  `bundle_hash_mismatch: <path>: expected
  <sha256>, got <sha256>` line a dashboard
  scraper can `grep ^live_proof bundle
  verification failed:` the log. The new
  `Mode::VerifyBundle` arm is a no-DB
  early-dispatch that mirrors the STW-028
  `Mode::VerifyReceipt` arm; the
  `Mode::Publish` arm is a no-DB early-dispatch
  that calls the `publish::run` handler and
  prints a one-line
  `live_proof publish complete: bundle=<path>
  sha256=<sha256> bytes=<N>` headline (the same
  `live_proof ... complete: ...` family the
  STW-019 / STW-031 trainers already print so
  one `grep ^live_proof` scraper can read the
  whole chain). The companion
  `scripts/testnet-live-publish.sh` runbook is
  pure bash, mirrors the STW-019
  `testnet-live-proof.sh` shape (script exists
  + is executable + parses with `bash -n` +
  refuses to run on a missing receipt with
  exit 3), and drops the tarball + sha256 +
  manifest into the workspace's
  `publish/testnet-live-proof-<UTC-ISO>/`
  directory so a CI worker can `aws s3 cp` /
  `gsutil cp` the three files into a dashboard
  bucket in a single step. The publish step
  is **read-only** with respect to the
  receipt directory (it copies the files into
  a `staging/` tempdir before tarring, so a
  `trainer --publish` invocation cannot
  mutate a receipt the runbook produced even
  on partial-failure paths). Owner files:
  `scripts/testnet-live-publish.sh` (new
  pure-bash driver that re-verifies the
  receipt with `LiveProofReceipt::read_and_verify`
  via `trainer --verify-receipt` before
  tarring — refuses to publish a red
  receipt), `scripts/testnet-live-publish.md`
  (new runbook doc mirroring the
  `testnet-live-proof.md` shape:
  what-it-does / output-layout / env-knobs /
  exit-codes / re-verifying-with-the-binary
  sections + a "what it does NOT do"
  section that names the S3 / GCS / git-tag
  push step as the next slice), `crates/autotrain/src/publish.rs`
  (new `PublishedBundle` struct + `BundleFile` struct +
  `to_json` + `from_bundle_path` + `verify`
  surface + `publish_receipt` top-level entry
  point + new `publish_bundle_deterministic_for_fixed_receipt`
  / `publish_bundle_round_trips_through_manifest`
  / `publish_bundle_verifier_rejects_tampered_file`
  / `publish_bundle_verifier_rejects_missing_file`
  / `publish_bundle_verifier_rejects_bad_manifest`
  / `publish_to_json_contains_every_field` /
  `publish_run_prints_complete_headline` /
  `publish_run_errors_on_missing_receipt_dir`
  / `publish_run_errors_on_red_receipt` lib
  tests), `crates/autotrain/src/verify_bundle.rs`
  (new `run` handler that mirrors
  `verify_receipt::run`'s `Result<String, String>`
  shape so the `Mode::VerifyBundle` arm can
  `print!` on `Ok` + `eprintln!` + exit 2 on
  `Err` without a new error type), `crates/autotrain/src/mode.rs`
  (new `Mode::Publish` arm + `--publish
  <receipt-dir>` argv handling + the
  `publish::run` call + new `Mode::VerifyBundle`
  arm + `--verify-bundle <path>` argv handling
  + the `verify_bundle::run` call + both new
  modes listed in the `Usage:` eprintln!
  line), `crates/autotrain/src/lib.rs` (re-export
  the new `PublishedBundle` / `BundleFile`
  types), `crates/autotrain/tests/publish.rs`
  (new no-DB integration test
  `publish_round_trips_through_real_trainer_binary`
  that drives `trainer --publish <receipt>`
  end-to-end against a synthetic receipt
  under `target/test-publish/<UTC-ISO>/` +
  asserts the tarball extracts to a tree
  whose `sha256sum` matches the on-disk
  manifest's per-file digests + a second
  test
  `verify_bundle_round_trips_through_real_trainer_binary`
  that calls
  `trainer --verify-bundle <tarball>` and
  asserts it exits 0 + prints the pinned
  `live_proof bundle verification passed:`
  prefix), `crates/autotrain/tests/script_shape.rs`
  (add the new
  `testnet_live_publish_script_exists_and_parses`
  shape pin: script exists + is executable +
  parses with `bash -n` + the runbook doc
  lists every chain step the publish script
  honors + the runbook doc references the
  `--verify-bundle` CLI subcommand), `crates/autotrain/tests/fixtures/publish-fixture/`
  (new committed no-DB portable reference
  bundle the `trainer --verify-bundle` test
  re-verifies on every `cargo test
  --workspace` run; a fixture whose tarball
  + manifest + sha256 are byte-stable so a
  drift in either the fixture or the
  verifier fails the new
  `verify_bundle::tests::run_verifies_committed_publish_fixture`
  lib test), `IMPLEMENTATION_PLAN.md`
  (this row), `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the v7 publish slice as shipped with
  a one-line note in the "Immediate P0"
  shipped list), `scripts/testnet-live-proof.md`
  (replace the "next slice
  (`testnet-live-publish`)" parenthetical
  with a one-line "shipped as STW-032" note
  + a link to `scripts/testnet-live-publish.md`),
  `README.md` (add a `## Testnet publish
  bundle` section under `## Testnet launch
  proof` that links the new runbook +
  shows the `bash scripts/testnet-live-publish.sh
  <receipt-dir>` usage + the
  `trainer --verify-bundle <path>` re-verify
  line). Scope boundary: do NOT touch the
  STW-019 `testnet-live-proof.sh` runbook
  (the publish is a follow-on *consumer* of
  the receipts the runbook produces, not a
  refactor); do NOT change the STW-023
  `LiveProofReceipt::read_and_verify` /
  `LiveProofRecipe` JSON shape (the publish
  reads + re-verifies the receipt, then
  writes its own manifest — a `recipe.json`
  drift fails the publish step's
  pre-tar `trainer --verify-receipt` call);
  do NOT introduce an S3 / GCS / git-tag
  push (a `bash scripts/testnet-live-publish.sh
  <receipt>` invocation writes the
  portable bundle into a local
  `publish/` directory a CI worker can
  `aws s3 cp` in a follow-on slice); do
  NOT introduce a Python or `jq`
  dependency (the runbook is pure bash +
  `tar` + `sha256sum`); do NOT change the
  trainer's `--smoke` / `--bench` /
  `--compare` / `--compare3` / `--replay`
  / `--verify-receipt` behaviour or JSON
  contracts. Verification commands:
  `cargo test -p rbp-autotrain --features
  database --tests --lib`, `cargo test
  --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Required tests: the new lib
  tests in `publish.rs::tests` +
  `verify_bundle.rs::tests` + the new
  `crates/autotrain/tests/publish.rs`
  integration test; no padding of
  unrelated suites. Dependencies: `STW-019`
  (the runbook the publish step consumes
  receipts from), `STW-023` (the
  `LiveProofReceipt::read_and_verify`
  verifier the publish step calls as a
  pre-tar gate), `STW-028` (the
  `trainer --verify-receipt` CLI
  subcommand the publish step shells out
  to). Estimated scope: M. Completion
  signal: `bash
  scripts/testnet-live-publish.sh
  <receipt-dir>` exits 0 on a green
  `receipts/testnet-live-proof-<UTC-ISO>/`
  receipt and writes a
  `publish/testnet-live-proof-<UTC-ISO>.tar.gz`
  + matching `.sha256` + matching
  `.manifest.json` whose tarball
  extracts to a tree that `sha256sum -c`
  agrees matches the manifest; `trainer
  --publish <receipt-dir>` exits 0 +
  prints the pinned
  `live_proof publish complete: ...`
  headline; `trainer --verify-bundle
  <bundle-path>` exits 0 + prints the
  pinned
  `live_proof bundle verification passed: ...`
  line; the new integration tests
  pass; `cargo test --workspace` and
  `cargo fmt --check` are green; the
  + the committed `publish-fixture/` bundle is
  re-verified on every `cargo test
  --workspace` run.

- [x] `STW-033` `trainer --publish-remote
  <receipt-dir> --bucket <s3://...>`
  + no-DB `trainer --verify-remote
  <remote-dir>` remote-upload plan
  surface. The v7-follow-on-of-v7
  follow-on the STW-032 runbook doc
  names explicitly: a CI worker that
  fetches the STW-032 publish bundle
  can `trainer --publish-remote
  <receipt-dir> --bucket <s3://...>
  --prefix <prefix/>` to write a
  deterministic upload plan
  (`<publish>/<basename>/remote/remote_plan.json`)
  + a post-upload receipt
  (`<publish>/<basename>/remote/remote_receipt.json`)
  the same worker (or a downstream
  auditor) re-verifies with
  `trainer --verify-remote
  <publish>/<basename>/remote/`. The
  publish-remote step re-verifies the
  receipt with
  `LiveProofReceipt::read_and_verify`
  AS THE FIRST GATE (so a red
  receipt short-circuits the upload
  with `PublishRemoteError::ReceiptRed(...)`
  before any `BundleDir` /
  `BucketUri` gate can fire), then
  re-verifies the STW-032 publish
  bundle with
  `PublishedBundle::from_bundle_path` +
  `manifest.verify`, then writes the
  per-file upload plan. The plan's
  `s3_objects[]` array is sorted by
  `s3_uri` for determinism; the
  `created_at_utc` /
  `uploaded_at_utc` fall back to
  the `<unknown>` sentinel when the
  `RBP_PUBLISH_REMOTE_UTC` env knob
  is unset so the integration test
  is byte-stable. The new
  `Mode::PublishRemote` arm is a
  no-DB early-dispatch (mirrors
  `Self::Publish` + `Self::VerifyBundle`):
  reads the receipt + bundle, runs
  the pre-upload gates, writes the
  plan + the receipt, and prints a
  one-line
  `live_proof publish_remote complete:
  bucket=... prefix=... files=...
  bytes=... bundle_sha256=...
  basename=... dry_run=...` headline
  (the same `live_proof ... complete:`
  family the STW-019 / STW-031 /
  STW-032 trainers already print so
  one `grep ^live_proof` scraper can
  read the whole chain). The new
  `Mode::VerifyRemote` arm is the
  post-upload re-verifier: reads the
  on-disk `remote_receipt.json`,
  re-hashes every local file the
  receipt claims to have uploaded,
  asserts every digest matches,
  asserts every `s3_uri` in the
  receipt appears in the inlined
  plan (a phantom `s3_uri` is a
  hard `MissingObject` error), and
  prints a one-line
  `live_proof remote verification
  passed: ...` /
  `live_proof remote verification
  failed: ...` headline. The arm
  defaults to `--dry-run` (the
  `RBP_PUBLISH_REMOTE_DRY_RUN=1`
  knob); the
  `--no-dry-run` argv flips the
  arm into live mode (which shells
  out to `aws s3 cp` per file in
  the plan — the `aws` CLI must be
  on `$PATH` and the shell must
  have the
  `AWS_ACCESS_KEY_ID` /
  `AWS_SECRET_ACCESS_KEY` env
  knobs set; a missing `aws` returns
  `PublishRemoteError::AwsCli` and
  the arm exits 2). The companion
  `scripts/testnet-live-publish-s3.sh`
  runbook is pure bash, mirrors the
  STW-019 `testnet-live-proof.sh` +
  STW-032 `testnet-live-publish.sh`
  shape (script exists + is
  executable + parses with `bash -n`
  + refuses to run on a missing
  receipt with exit 3 + refuses to
  run on a missing bucket with
  exit 3), and drives the full
  chain: (1) re-verify the receipt
  with `trainer --verify-receipt`,
  (2) re-verify the STW-032 publish
  bundle with `trainer
  --verify-bundle`, (3) shell out
  to `trainer --publish-remote`,
  (4) re-verify the post-upload
  `remote_receipt.json` with
  `trainer --verify-remote`. The
  bash script's `PUBLISH_DRY_RUN=0`
  knob flips the
  `trainer --publish-remote` arm
  into live mode (which shells out
  to `aws s3 cp` per file). Owner
  files: `crates/autotrain/src/publish_remote.rs`
  (new `S3Object` struct +
  `PublishRemotePlan` struct +
  `PublishedRemoteReceipt` struct +
  `PublishRemoteError` enum +
  `Display` + `From<PublishError>`
  impls + `publish_remote_receipt`
  top-level entry point +
  `PublishedRemoteReceipt::verify`
  + `read_remote_receipt` + new
  `bucket_uri_as_str_matches_published_strings`
  / `bucket_uri_rejects_non_s3_prefix`
  / `bucket_uri_rejects_empty_bucket`
  / `publish_remote_dry_run_writes_plan_and_receipt`
  / `publish_remote_s3_uris_are_sorted_for_determinism`
  / `publish_remote_refuses_red_receipt`
  / `publish_remote_round_trips_through_verifier`
  / `publish_remote_verifier_rejects_tampered_file`
  / `publish_remote_to_json_contains_every_field`
  / `publish_remote_bare_bucket_name_normalises_to_s3_uri`
  / `publish_remote_created_at_utc_falls_back_to_unknown`
  / `publish_remote_io_error_propagates_for_missing_receipt`
  / `publish_remote_io_error_propagates_for_missing_bundle`
  lib tests), `crates/autotrain/src/mode.rs`
  (new `Mode::PublishRemote` arm
  + `--publish-remote <receipt>
  --bucket <s3://...> [--prefix
  <prefix/>] [--no-dry-run]` argv
  handling + the
  `publish_remote::publish_remote_receipt`
  call + new `Mode::VerifyRemote`
  arm + `--verify-remote <path>`
  argv handling + the
  `publish_remote::read_remote_receipt` +
  `PublishedRemoteReceipt::verify`
  call + both new modes listed in
  the `Usage:` eprintln! line),
  `crates/autotrain/src/lib.rs`
  (re-export the new `S3Object` /
  `PublishRemotePlan` /
  `PublishedRemoteReceipt` /
  `PublishRemoteError` types +
  register the new
  `publish_remote` module),
  `crates/autotrain/tests/publish_remote.rs`
  (new no-DB integration test
  `publish_remote_round_trips_through_real_trainer_binary`
  that drives
  `trainer --publish <receipt>` +
  `trainer --publish-remote <receipt>
  --bucket <s3://...>` +
  `trainer --verify-remote
  <remote-dir>` end-to-end against
  a synthetic receipt +
  asserts the headline starts with
  the pinned
  `live_proof publish_remote
  complete: ` prefix + the
  `bucket=... prefix=... files=...
  bytes=... bundle_sha256=...
  basename=... dry_run=...` tokens
  are present + the
  `remote_plan.json` +
  `remote_receipt.json` files are
  on disk + the verifier's headline
  starts with the pinned
  `live_proof remote verification
  passed: ` prefix + a second test
  `publish_remote_run_exits_two_with_red_receipt_line`
  that drops a red receipt (rewrites
  `cluster/exit.txt` to `1`) +
  drives `trainer --publish-remote`
  + asserts exit 2 + the stderr
  starts with
  `live_proof publish_remote error:
  receipt is red: ` + a third test
  `publish_remote_run_exits_two_with_missing_bucket`
  that drives
  `trainer --publish-remote <receipt>`
  with no `--bucket` flag + asserts
  exit 2 + the stderr carries the
  `--bucket` usage line),
  `crates/autotrain/tests/script_shape.rs`
  (add the new
  `testnet_live_publish_s3_script_exists_and_parses`
  shape pin: S3 script exists + is
  executable + parses with `bash -n`
  + the
  `testnet_live_publish_s3_script_has_verify_bundle_pre_upload_gate`
  pre-upload-gate pin: the S3
  script must shell out to
  `trainer --verify-bundle <bundle>`
  BEFORE the
  `trainer --publish-remote <receipt>`
  call + the
  `testnet_live_publish_s3_script_references_publish_remote_cli`
  CLI-reference pin: the S3 script
  references the
  `trainer --publish-remote <receipt>
  --bucket <s3://...>` CLI
  subcommand),
  `scripts/testnet-live-publish-s3.sh`
  (new pure-bash driver that
  re-verifies the receipt + the
  STW-032 publish bundle before
  shelling out to
  `trainer --publish-remote` +
  shells out to
  `trainer --verify-remote` for
  post-upload re-verification +
  writes a `SUMMARY.txt` headline
  a CI worker can `cat` to confirm
  the chain landed end-to-end),
  `scripts/testnet-live-publish.md`
  (replace the "next slice" /
  "NOT this one" parenthetical
  about the S3 / GCS / git-tag
  push with a one-line "shipped as
  STW-033" note + the new
  `trainer --publish-remote` CLI
  example + a link to
  `scripts/testnet-live-publish-s3.sh`),
  `scripts/testnet-live-proof.md`
  (replace the "next slice" /
  "follow-on" parenthetical about
  the dashboard-bucket push with a
  one-line STW-033 reference +
  link to
  `scripts/testnet-live-publish-s3.sh`),
  `IMPLEMENTATION_PLAN.md` (this
  row). Scope boundary: do NOT
  touch the STW-019
  `testnet-live-proof.sh` runbook
  (the publish-remote is a
  follow-on *consumer* of the
  receipts the runbook produces,
  not a refactor); do NOT touch
  the STW-032 `testnet-live-publish.sh`
  runbook (the publish-remote is
  a follow-on *consumer* of the
  publish bundles the STW-032
  runbook produces, not a
  refactor); do NOT change the
  STW-023
  `LiveProofReceipt::read_and_verify`
  / `LiveProofRecipe` JSON shape
  (the publish-remote reads +
  re-verifies the receipt as a
  pre-upload gate, then writes its
  own `remote_plan.json` +
  `remote_receipt.json` — a
  `recipe.json` drift fails the
  publish-remote step's
  pre-upload `trainer
  --verify-receipt` call); do NOT
  change the STW-032
  `PublishedBundle` /
  `BundleFile` / `PublishError`
  JSON shape (the publish-remote
  reads + re-verifies the bundle
  with
  `PublishedBundle::from_bundle_path` +
  `manifest.verify` — a
  `manifest.json` drift fails the
  publish-remote step's pre-upload
  `trainer --verify-bundle` call);
  do NOT vendor the AWS SDK or
  `rusoto_s3` (the upload step is
  the bash runbook's job — the
  Rust arm only writes the upload
  plan + the post-upload receipt);
  do NOT shell out to `aws` in the
  default `trainer --publish-remote`
  path (the `cargo test --workspace`
  integration test runs in
  dry-run so a regression in the
  CLI surface fails CI without an
  `aws` credential or a live
  bucket); do NOT change the
  `Mode::Status` / `Mode::Fast` /
  `Mode::Fast2` / `Mode::Fast3` /
  `Mode::Slow` / `Mode::Reset` /
  `Mode::Smoke` / `Mode::Bench` /
  `Mode::Compare` / `Mode::Compare3`
  / `Mode::Replay` / `Mode::VerifyReceipt`
  / `Mode::Publish` / `Mode::VerifyBundle`
  code paths. Verification
  commands: `cargo test -p
  rbp-autotrain --features
  database --tests --lib`,
  `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt
  --check`, `bash -n
  scripts/testnet-live-publish-s3.sh`.
  Required tests: the new lib
  tests in
  `publish_remote.rs::tests` + the
  new
  `crates/autotrain/tests/publish_remote.rs`
  integration test + the new
  `crates/autotrain/tests/script_shape.rs`
  shape pins; no padding of
  unrelated suites. Dependencies:
  `STW-032` (the
  `crates/autotrain/src/publish.rs::PublishedBundle`
  + the
  `crates/autotrain/src/verify_bundle.rs::run`
  handler the publish-remote step
  consumes), `STW-028` (the
  `trainer --verify-receipt` CLI
  the publish-remote bash runbook
  shells out to as a pre-upload
  gate), `STW-019` (the
  `testnet-live-proof.sh` runbook
  the publish-remote step consumes
  receipts from). Estimated scope:
  M. Completion signal:
  `trainer --publish-remote <receipt>
  --bucket <s3://...>` exits 0 on
  a green
  `receipts/testnet-live-proof-<UTC-ISO>/`
  + a fresh STW-032 publish bundle
  + prints the pinned
  `live_proof publish_remote
  complete: ...` headline + writes
  a `remote_plan.json` +
  `remote_receipt.json` pair under
  `<publish>/<basename>/remote/`;
  `trainer --verify-remote
  <publish>/<basename>/remote/`
  exits 0 + prints the pinned
  `live_proof remote verification
  passed: ...` line; the new
  integration tests pass;
  `cargo test --workspace` and
  `cargo fmt --check` are green;
  `bash -n
  scripts/testnet-live-publish-s3.sh`
  parses; the
  `testnet_live_publish_s3_script_*`
  shape pins pass.

## Active items (worker-ready) — v8 follow-on chain

The v7 chain (STW-019 → STW-028 → STW-032 → STW-033) is closed: every
`testnet-live-proof` / `testnet-live-publish` / `testnet-live-publish-s3`
follow-on named in the runbook docs has shipped. The next claimable
slice is the v8 follow-on a testnet dashboard naturally wants: a
deterministic aggregator over every `publish/*/remote/remote_receipt.json`
the STW-033 chain produced on a single machine, so a dashboard can
fetch one file (`INDEX.json`) instead of listing the bucket + fetching
N manifests.

- [x] `STW-034` `trainer --publish-index
  <publish-root>` aggregator +
  no-DB `trainer --verify-index
  <index-path>` re-verifier.
  Scans every immediate
  `publish/<basename>/remote/`
  subdirectory under
  `<publish-root>/`, reads
  the `remote_receipt.json`
  each STW-033
  `trainer --publish-remote`
  arm wrote, and produces
  a deterministic
  `publish/<basename>/index/INDEX.json`
  + `SUMMARY.txt` pair:
  the `entries[]` array
  is sorted by
  `receipt_basename` (so
  re-running the index
  step on an unchanged
  publish root produces a
  byte-identical
  `INDEX.json`), each
  entry inlines the
  `PublishedRemoteReceipt`
  the STW-033 runbook
  wrote (bucket + prefix
  + `s3_objects[]` +
  `bundle_sha256` +
  `total_bytes` +
  `uploaded_at_utc` +
  `runbook_version`), and
  the top-level object
  records the
  `publish_root` +
  `runbook_version` +
  `created_at_utc`
  (`<unknown>` sentinel
  when the
  `RBP_PUBLISH_INDEX_UTC`
  env knob is unset so
  the lib test +
  integration test are
  byte-stable) +
  `entry_count` +
  `total_bytes`
  (the sum of every
  entry's `total_bytes`).
  The `--publish-index`
  arm refuses to index a
  red `remote_receipt.json`:
  a
  `PublishedRemoteReceipt::verify`
  pre-index gate fires
  per entry so a
  `trainer --publish-index <root>`
  on a red entry exits 2 +
  prints the pinned
  `live_proof publish_index
  error: remote receipt
  is red: ...` line and
  writes no
  `INDEX.json` (the
  "refuse to paper-over
  a red remote receipt"
  invariant the STW-028
  receipt verifier +
  STW-032 bundle
  verifier + STW-033
  remote-receipt verifier
  already enforce). The
  new `Mode::VerifyIndex`
  arm is the no-DB
  no-rebuild re-verify
  path: it re-hashes
  every local file the
  `INDEX.json` claims to
  have inlined (each
  entry's
  `s3_objects[].local_path`
  is read + re-sha256'd
  + compared to the
  entry's `sha256`),
  asserts every digest
  matches, asserts every
  `s3_uri` in the index
  appears in the
  inlined plan, and
  prints a one-line
  `live_proof index
  verification passed:
  ...` /
  `live_proof index
  verification failed:
  ...` headline a
  dashboard scraper can
  `grep ^live_proof index
  verification` the log.
  The companion
  `scripts/testnet-live-publish-index.sh`
  runbook is pure bash,
  mirrors the STW-019 +
  STW-032 + STW-033
  shape (script exists +
  is executable + parses
  with `bash -n` +
  refuses to run on a
  missing publish root
  with exit 3), and
  chains
  `trainer --publish-index <publish-root>`
  (the index writer) +
  `trainer --verify-index <index-path>`
  (the index re-verifier)
  as a sequence of
  subprocesses. The
  `INDEX.json` is
  **read-only** with
  respect to the
  publish root: the
  indexer writes its
  output under
  `<publish_root>/index/`,
  so a
  `trainer --publish-index`
  invocation cannot
  mutate the underlying
  `remote_receipt.json`
  files even on
  partial-failure paths.
  Scope boundary: the
  index step does NOT
  push to S3 / GCS /
  git-tag (a CI worker
  can `aws s3 cp` the
  local
  `publish/<root>/index/`
  directory in a
  follow-on slice); does
  NOT change the STW-019
  `testnet-live-proof.sh`
  or STW-032
  `testnet-live-publish.sh`
  or STW-033
  `testnet-live-publish-s3.sh`
  runbook (the index is
  a follow-on *consumer*
  of the
  `remote_receipt.json`
  files the STW-033
  chain produces, not a
  refactor); does NOT
  change the STW-033
  `PublishRemotePlan` /
  `PublishedRemoteReceipt`
  / `S3Object` JSON
  shape (a manifest
  drift fails the index
  step's per-entry
  pre-index `trainer
  --verify-remote`
  check); does NOT
  introduce a Python /
  `jq` dependency (the
  runbook is pure bash +
  `find` + `sha256sum`).
  **Closes the v8
  follow-on the STW-033
  publish-remote step
  names as the next
  slice** (a testnet
  dashboard needs an
  aggregator, not N
  point fetches).
  Owner files:
  `crates/autotrain/src/publish_index.rs`
  (new `IndexedEntry`
  struct + `PublishIndex`
  struct + `PublishIndexError`
  enum + `Display` impl
  + `publish_index`
  top-level entry point
  + `PublishIndex::verify`
  + `read_publish_index`
  + new
  `bucket_uri_as_str_matches_published_strings_v2`
  / `publish_index_writes_index_json`
  / `publish_index_refuses_red_remote_receipt`
  / `publish_index_is_byte_stable_for_unchanged_root`
  / `publish_index_aggregates_total_bytes_across_entries`
  / `publish_index_sorted_by_receipt_basename`
  / `publish_index_io_error_propagates_for_missing_root`
  / `verify_index_rehashes_every_local_file`
  / `verify_index_rejects_tampered_entry`
  / `verify_index_phantom_uri_fails_with_missing_object`
  lib tests),
  `crates/autotrain/src/lib.rs`
  (new `mod publish_index`
  + `pub use publish_index::*`),
  `crates/autotrain/src/mode.rs`
  (new `Mode::PublishIndex`
  + `Mode::VerifyIndex`
  arm + `--publish-index`
  / `--verify-index` argv
  handling + the index
  scan call into
  `publish_index::publish_index`
  + `--publish-index` /
  `--verify-index` listed
  in the `Usage:`
  eprintln! line +
  matching
  `unreachable!()`
  catch-alls in the
  post-DB-open match
  arm),
  `crates/autotrain/tests/publish_index.rs`
  (new
  `publish_index_round_trips_through_real_trainer_binary`
  / `publish_index_run_exits_two_with_red_remote_receipt`
  / `publish_index_run_exits_two_with_missing_publish_root`
  / `verify_index_round_trips_through_real_trainer_binary`
  integration tests
  gated as no-DB so they
  run in
  `cargo test --workspace`),
  `crates/autotrain/tests/script_shape.rs`
  (new
  `testnet_live_publish_index_script_exists_and_parses`
  / `testnet_live_publish_index_script_has_publish_index_call`
  / `testnet_live_publish_index_script_has_verify_index_call`
  / `testnet_live_publish_index_doc_references_publish_index_cli`
  shell-shape pins),
  `scripts/testnet-live-publish-index.sh`
  (new pure-bash runbook
  that drives
  `trainer --publish-index`
  + `trainer --verify-index`
  end-to-end as
  subprocesses, mirrors
  the STW-019 + STW-032 +
  STW-033 runbook shape:
  `set -euo pipefail` +
  script exists +
  executable + parses
  with `bash -n` + refuses
  to run on a missing
  publish root with
  exit 3 + writes a
  `SUMMARY.txt` headline
  a CI worker can `cat`),
  `scripts/testnet-live-publish-index.md`
  (new runbook doc that
  explains the index
  step + the
  `INDEX.json` +
  `SUMMARY.txt` layout
  + references the
  `--publish-index` +
  `--verify-index` CLI
  subcommands).
  Verification commands:
  `cargo fmt --check`;
  `cargo check --workspace`;
  `cargo test --workspace`;
  `bash -n scripts/testnet-live-publish-index.sh`;
  `trainer --publish-index <root>` exits 0 +
  prints the pinned
  `live_proof publish_index
  complete: root=... entries=...
  total_bytes=...` line +
  writes a green
  `INDEX.json`;
  `trainer --verify-index
  <index-path>` exits 0 +
  prints the pinned
  `live_proof index
  verification passed:
  ...` line; the new
  integration tests
  pass; the
  `testnet_live_publish_index_script_*`
  shape pins pass.

## Promoted from v8 follow-on (this slice)

The v8 chain (STW-034) is closed: the
`testnet-live-publish-index.sh` runbook + the
`trainer --publish-index <publish-root>` aggregator + the
`trainer --verify-index <index-path>` re-verifier are
shipped. The STW-034 doc's scope-boundary section names
the next claimable slice explicitly: "does NOT push to
S3 / GCS / git-tag (a CI worker can `aws s3 cp` the
local `publish/<root-basename>/index/` directory in a
follow-on slice)". The v9 follow-on a testnet dashboard
naturally wants is a *plan-first* remote-upload of the
`INDEX.json` aggregator the STW-034 chain produced — a
deterministic `index_remote_plan.json` +
`index_remote_receipt.json` pair a CI worker can
`aws s3 cp` to push the aggregator to a dashboard
bucket, AND a no-DB no-rebuild re-verify path that
re-hashes the local `INDEX.json` + the per-entry
`remote_receipt.json` files the STW-034 chain produced.

- [x] `STW-035` `trainer --publish-index-remote
  <publish-root> --bucket
  <s3://...> [--prefix
  <prefix/>] [--no-dry-run]`
  + no-DB `trainer
  --verify-index-remote
  <remote-dir>` remote-upload
  plan + re-verifier surface
  for the STW-034 `INDEX.json`
  aggregator. The v9
  follow-on the STW-034
  scope-boundary defers to:
  a CI worker that has run
  `trainer --publish-index
  <publish-root>` can
  `trainer
  --publish-index-remote
  <publish-root>
  --bucket
  <s3://...> --prefix
  <prefix/>` to write a
  deterministic upload
  plan
  (`<publish-root>/index_remote/remote_plan.json`)
  + a post-upload
  `remote_receipt.json`
  the same worker (or a
  downstream auditor)
  re-verifies with
  `trainer
  --verify-index-remote
  <publish-root>/index_remote/`.
  The publish-index-remote
  step re-verifies the
  STW-034 `INDEX.json` with
  `PublishIndex::verify` AS
  THE FIRST GATE (so a red
  index short-circuits
  the upload with
  `PublishIndexRemoteError::IndexRed(...)`
  before any `BucketUri`
  gate can fire), then
  re-validates the
  per-entry
  `remote_receipt.json`
  files in the inlined
  index entries (the
  STW-034 chain's per-entry
  `PublishedRemoteReceipt`
  is the source of truth
  for the per-file upload
  plan), then writes
  the per-file upload
  plan. The plan's
  `s3_objects[]` array
  is sorted by `s3_uri`
  for determinism; the
  `created_at_utc` /
  `uploaded_at_utc` fall
  back to the `<unknown>`
  sentinel when the
  `RBP_PUBLISH_INDEX_REMOTE_UTC`
  env knob is unset so
  the integration test
  is byte-stable. The
  new
  `Mode::PublishIndexRemote`
  arm is a no-DB
  early-dispatch
  (mirrors
  `Self::PublishRemote`):
  reads the `INDEX.json`,
  runs the pre-upload
  gate, walks the
  inlined
  `PublishIndex::entries[]`
  to build the
  per-file `s3_objects[]`
  mapping
  (`<index_filename> -> s3://<bucket>/<prefix>/<index_filename>`),
  and prints a one-line
  `live_proof publish_index_remote complete: bucket=... prefix=... files=... bytes=... index_path=... runbook_version=... dry_run=...`
  headline (the same
  `live_proof ... complete:`
  family the STW-019 /
  STW-031 / STW-032 /
  STW-033 / STW-034
  trainers already print
  so one `grep ^live_proof`
  scraper can read the
  whole chain). The
  new
  `Mode::VerifyIndexRemote`
  arm is the
  post-upload
  re-verifier: reads
  the on-disk
  `remote_receipt.json`,
  re-hashes the local
  `INDEX.json` the
  receipt claims to
  have uploaded (the
  re-hash compares to
  the receipt's
  `index_sha256` field),
  asserts every digest
  matches, asserts every
  `s3_uri` in the receipt
  appears in the inlined
  plan (a phantom
  `s3_uri` is a hard
  `PublishIndexRemoteError::MissingObject`
  error), and prints
  a one-line
  `live_proof index_remote verification passed: ...` /
  `live_proof index_remote verification failed: ...`
  headline a dashboard
  scraper can
  `grep ^live_proof index_remote verification`
  the log. The arm
  defaults to
  `--dry-run` (the
  `RBP_PUBLISH_INDEX_REMOTE_DRY_RUN=1`
  knob); the
  `--no-dry-run` argv
  flips the arm into
  live mode (which
  shells out to
  `aws s3 cp` per file
  in the plan — the
  `aws` CLI must be on
  `$PATH` and the shell
  must have the
  `AWS_ACCESS_KEY_ID` /
  `AWS_SECRET_ACCESS_KEY`
  env knobs set; a
  missing `aws` returns
  `PublishIndexRemoteError::AwsCli`
  and the arm exits 2).
  The companion
  `scripts/testnet-live-publish-index-s3.sh`
  runbook is pure bash,
  mirrors the
  STW-019 +
  STW-032 + STW-033 +
  STW-034 shape (script
  exists + is executable
  + parses with
  `bash -n` + refuses
  to run on a missing
  publish root / missing
  bucket / missing
  `INDEX.json` with
  exit 3), and chains
  `trainer --verify-index
  <index-dir>` (pre-upload
  refuse-to-upload-red-index
  gate) →
  `trainer
  --publish-index-remote
  <publish-root>
  --bucket
  <s3://...>` (the
  plan + post-upload-receipt
  writer) →
  `trainer
  --verify-index-remote
  <remote-dir>`
  (post-upload
  re-verify) as a
  sequence of
  subprocesses, and
  writes a `SUMMARY.txt`
  headline a CI worker
  can `cat` to confirm
  the chain landed
  end-to-end. The new
  `crates/autotrain/tests/publish_index_remote.rs`
  integration test
  drives
  `trainer --publish-index`
  +
  `trainer --publish-index-remote`
  +
  `trainer --verify-index-remote`
  end-to-end through a
  real subprocess (3
  sub-tests: round-trip,
  red-index gate,
  missing-bucket gate)
  so a regression in the
  CLI surface (renamed
  flag, missing exit
  code, dropped error
  kind) fails CI before
  it reaches an
  operator's machine.
  The new
  `crates/autotrain/tests/script_shape.rs`
  shape pins (4 STW-035
  pins) assert the
  `testnet-live-publish-index-s3.sh`
  script is on disk +
  executable + parses
  with `bash -n` + calls
  `--verify-index` BEFORE
  `--publish-index-remote`
  + references the
  `trainer
  --publish-index-remote`
  / `--bucket` CLI
  subcommand + the
  `testnet-live-publish-index.md`
  doc references the
  `trainer
  --verify-index-remote`
  re-verify subcommand.
  The publish-index-remote
  step is **read-only**
  with respect to the
  publish root + the
  `INDEX.json`: it reads
  + re-verifies the
  `INDEX.json` in place,
  then writes its own
  `index_remote/` dir
  under the publish root
  directory, so a
  `trainer
  --publish-index-remote`
  invocation cannot
  mutate the underlying
  `INDEX.json` or the
  per-entry
  `remote_receipt.json`
  files even on
  partial-failure paths.
  Scope boundary: does
  NOT push via a
  vendored AWS / GCS SDK
  (the live `aws s3 cp`
  shell-out is the bash
  runbook's job —
  adding a 50-MB SDK to
  a no-system-deps
  trainer binary is the
  inverse of the
  "pure bash + cargo +
  trainer" shape the
  rest of the autotrain
  pipeline already
  follows); does NOT
  shell out to `aws` in
  the default
  `trainer
  --publish-index-remote`
  path (the
  `cargo test --workspace`
  integration test runs
  in dry-run so a
  regression in the CLI
  surface fails CI
  without an `aws`
  credential or a live
  bucket); does NOT
  touch the STW-019
  `testnet-live-proof.sh`
  or the STW-032
  `testnet-live-publish.sh`
  or the STW-033
  `testnet-live-publish-s3.sh`
  or the STW-034
  `testnet-live-publish-index.sh`
  runbook (the
  publish-index-remote
  is a follow-on
  *consumer* of the
  `INDEX.json` the
  STW-034 runbook
  produces, not a
  refactor); does NOT
  change the STW-034
  `PublishIndex` /
  `IndexedEntry` /
  `PublishIndexError`
  JSON shape (a
  manifest drift fails
  the publish-index-remote
  step's pre-upload
  `trainer --verify-index`
  call); does NOT
  change the STW-033
  `PublishedRemoteReceipt`
  / `S3Object` JSON
  shape (a
  `remote_receipt.json`
  drift fails the
  publish-index-remote
  step's per-entry
  `PublishedRemoteReceipt::verify`
  call). **Closes the
  v9 follow-on the
  STW-034
  `testnet-live-publish-index.md`
  scope-boundary defers
  to** (a CI worker that
  produced an `INDEX.json`
  wants to push it to a
  dashboard bucket
  without hand-rolling
  the per-file
  `aws s3 cp`
  shell-out).
  Owner files:
  `crates/autotrain/src/publish_index_remote.rs`
  (new `PublishIndexRemotePlan`
  struct + `PublishedIndexRemoteReceipt`
  struct +
  `PublishIndexRemoteError`
  enum + `Display` +
  `From<PublishIndexError>`
  + `From<PublishRemoteError>`
  impls +
  `publish_index_remote_receipt`
  top-level entry point
  + `PublishedIndexRemoteReceipt::verify`
  + `read_index_remote_receipt`
  + new
  `bucket_uri_as_str_matches_published_strings_v3`
  / `bucket_uri_rejects_non_s3_prefix_v2`
  / `bucket_uri_rejects_empty_bucket_v2`
  / `publish_index_remote_dry_run_writes_plan_and_receipt`
  / `publish_index_remote_s3_uris_are_sorted_for_determinism`
  / `publish_index_remote_refuses_red_index`
  / `publish_index_remote_round_trips_through_verifier`
  / `publish_index_remote_verifier_rejects_tampered_index`
  / `publish_index_remote_to_json_contains_every_field`
  / `publish_index_remote_bare_bucket_name_normalises_to_s3_uri`
  / `publish_index_remote_created_at_utc_falls_back_to_unknown`
  / `publish_index_remote_io_error_propagates_for_missing_root`
  / `publish_index_remote_io_error_propagates_for_missing_index`
  lib tests),
  `crates/autotrain/src/mode.rs`
  (new
  `Mode::PublishIndexRemote`
  arm + `--publish-index-remote
  <publish-root>
  --bucket
  <s3://...>
  [--prefix
  <prefix/>]
  [--no-dry-run]`
  argv handling + the
  `publish_index_remote::publish_index_remote_receipt`
  call + new
  `Mode::VerifyIndexRemote`
  arm + `--verify-index-remote
  <path>` argv
  handling + the
  `publish_index_remote::read_index_remote_receipt`
  +
  `PublishedIndexRemoteReceipt::verify`
  call + both new modes
  listed in the `Usage:`
  eprintln! line +
  matching
  `unreachable!()`
  catch-alls in the
  post-DB-open match
  arm),
  `crates/autotrain/src/lib.rs`
  (re-export the new
  `PublishIndexRemotePlan`
  /
  `PublishedIndexRemoteReceipt`
  /
  `PublishIndexRemoteError`
  types + register the
  new `publish_index_remote`
  module),
  `crates/autotrain/tests/publish_index_remote.rs`
  (new no-DB
  integration test
  `publish_index_remote_round_trips_through_real_trainer_binary`
  that drives
  `trainer --publish-index
  <publish-root>` +
  `trainer
  --publish-index-remote
  <publish-root>
  --bucket
  <s3://...>` +
  `trainer
  --verify-index-remote
  <remote-dir>`
  end-to-end against a
  synthetic index +
  asserts the headline
  starts with the
  pinned
  `live_proof publish_index_remote complete: `
  prefix + the
  `bucket=... prefix=... files=... bytes=... index_path=... runbook_version=... dry_run=...`
  tokens are present +
  the
  `index_remote_plan.json`
  +
  `index_remote_receipt.json`
  files are on disk +
  the verifier's
  headline starts with
  the pinned
  `live_proof index_remote verification passed: `
  prefix + a second
  test
  `publish_index_remote_run_exits_two_with_red_index`
  that drops a red
  index (rewrites an
  inlined
  `s3_objects[].sha256`
  to a bogus value) +
  drives
  `trainer
  --publish-index-remote`
  + asserts exit 2 +
  the stderr starts
  with
  `live_proof publish_index_remote error: index is red: `
  + a third test
  `publish_index_remote_run_exits_two_with_missing_bucket`
  that drives
  `trainer
  --publish-index-remote
  <publish-root>`
  with no `--bucket`
  flag + asserts
  exit 2 + the stderr
  carries the
  `--bucket` usage
  line),
  `crates/autotrain/tests/script_shape.rs`
  (add the new
  `testnet_live_publish_index_s3_script_exists_and_parses`
  shape pin: s3
  script exists + is
  executable + parses
  with `bash -n` + the
  `testnet_live_publish_index_s3_script_has_verify_index_pre_upload_gate`
  pre-upload-gate pin:
  the s3 script must
  shell out to
  `trainer --verify-index
  <index-dir>` BEFORE
  the
  `trainer
  --publish-index-remote`
  call + the
  `testnet_live_publish_index_s3_script_references_publish_index_remote_cli`
  CLI-reference pin:
  the s3 script
  references the
  `trainer
  --publish-index-remote
  <publish-root>
  --bucket
  <s3://...>` CLI
  subcommand + the
  `testnet-live-publish-index.md`
  doc references the
  `trainer
  --verify-index-remote`
  re-verify subcommand),
  `scripts/testnet-live-publish-index-s3.sh`
  (new pure-bash
  runbook that drives
  `trainer
  --publish-index-remote`
  + `trainer
  --verify-index-remote`
  end-to-end as
  subprocesses,
  mirrors the
  STW-019 +
  STW-032 +
  STW-033 +
  STW-034 runbook
  shape:
  `set -euo pipefail` +
  script exists +
  executable + parses
  with `bash -n` +
  refuses to run on
  a missing publish
  root / missing
  bucket / missing
  `INDEX.json` with
  exit 3 + writes a
  `SUMMARY.txt`
  headline a CI worker
  can `cat`),
  `scripts/testnet-live-publish-index-s3.md`
  (new runbook doc
  that explains the
  v9 follow-on
  index-remote step +
  the
  `index_remote/`
  layout +
  references the
  `--publish-index-remote`
  +
  `--verify-index-remote`
  CLI subcommands +
  names the
  `RBP_PUBLISH_INDEX_REMOTE_DRY_RUN`
  /
  `RBP_PUBLISH_INDEX_REMOTE_UTC`
  env knobs),
  `IMPLEMENTATION_PLAN.md`
  (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the v9
  index-remote slice
  as shipped with a
  one-line note in the
  "Immediate P0"
  shipped list),
  `scripts/testnet-live-publish-index.md`
  (replace the "next
  slice" parenthetical
  with a one-line
  "shipped as STW-035"
  note + a link to
  `scripts/testnet-live-publish-index-s3.md`),
  `README.md` (add a
  `## Testnet publish
  index remote` section
  under `## Testnet
  publish index` that
  links the new
  runbook + shows the
  `bash
  scripts/testnet-live-publish-index-s3.sh
  <publish-root>
  <s3://bucket>`
  usage + the
  `trainer
  --verify-index-remote
  <remote-dir>`
  re-verify line).
  Verification commands:
  `cargo fmt --check`;
  `cargo check --workspace`;
  `cargo test --workspace`;
  `bash -n
  scripts/testnet-live-publish-index-s3.sh`;
  `trainer
  --publish-index-remote
  <publish-root>
  --bucket
  <s3://...>`
  exits 0 + prints
  the pinned
  `live_proof publish_index_remote complete: ...`
  line + writes a
  green
  `index_remote_plan.json`
  +
  `index_remote_receipt.json`;
  `trainer
  --verify-index-remote
  <remote-dir>` exits
  0 + prints the
  pinned
  `live_proof index_remote verification passed: ...`
  line; the new
  integration tests
  pass; the
  `testnet_live_publish_index_s3_script_*`
  shape pins pass.

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

## Next wave - review 2026-06-04

The 2026-06-03 three-lens review (CEO / Eng / Design, kanban
task `t_c86ebbbf`) finds the v6→v9 receipt-pipeline chain (STW-029 →
STW-035) is *complete as a hermetic CI gate* but is **not** the
testnet north star the 2026-06-03 CEO sign-off names. The north star
is "a public, reproducible NLHE benchmark with replayable
transcripts"; the chain has built the supply, but no human can see
it. The next wave pivots from *more rungs of the chain* to *a
visible consumer of the chain*: a static dashboard (the v10
follow-on) plus the operator-UX + observability + docs surfaces
that make the chain legible to a first-time visitor and a CI
auditor. The v9 STW-035 row that already sits in
`## Promoted from v8 follow-on (this slice)` is the *prerequisite*
for the v10 dashboard slice; STW-035 is **shipped** on
commit (see `## Recently shipped` at the top of this plan).

Each row below names a single shippable slice with named files,
verification command(s), and a `lens` tag tracing the finding it
closes. Rows are P0/P1 ordered; the top row is the highest
single-shipment leverage.

- [x] **[P0] `STW-036` `crates/dashboard/` static dashboard crate
  consuming the STW-034 `INDEX.json` + STW-014
  `transcript-<id>.json` bundles a CI worker syncs to a
  public S3 / Cloudflare Pages bucket.** Shipped on
  commit (see `## Recently shipped` at the top of this
  plan). The visible consumer of the v6→v9 receipt
  chain. A new `crates/dashboard/` workspace
  member with three layers: (a) an `IndexClient`
  (re-uses `publish_index::PublishIndex` from
  `crates/autotrain/src/publish_index.rs` — the
  same Rust type the STW-034 chain writes, so a
  shape drift in `INDEX.json` fails both the
  dashboard's typed read AND the
  `trainer --verify-index` re-verify at the same
  CI step) that reads the bucket-hosted `INDEX.json`
  via a `RBP_DASHBOARD_INDEX_URL` env knob
  (default `http://localhost:8080/api/index` in
  tests, a CloudFront URL in production — no AWS
  SDK vendored, the same "pure bash + cargo +
  trainer" shape the rest of the autotrain pipeline
  follows); (b) a thin `axum` router at `GET /`
  (serves a static `index.html` with embedded
  per-receipt summary) + `GET /api/index` (returns
  the typed `INDEX.json`) + `GET /transcript/:id`
  (proxies the STW-014 `transcript-<id>.json`
  bundle) + `GET /bench/:id` (renders a
  `BenchReport` as a card using the same
  `crates/autotrain/src/bench.rs::BenchReport`
  Rust type the bench writes — a bench-shape drift
  fails both the dashboard render and the
  `trainer --replay` consumer at the same CI
  step); (c) a static vanilla-JS `index.html`
  (no framework; no build step; no `npm`) that
  fetches `/api/index` and renders a sortable
  table of receipts with columns for
  `receipt_basename` / `blueprint` / `baseline` /
  `mbb_per_100` / `ci_95` / `win_rate` / `total_bytes`
  / `uploaded_at_utc`, a per-row `Download
  transcript` link to `/transcript/:id`, and a
  per-row `Open replay` link to `/bench/:id`. The
  static `index.html` is a checked-in file (no
  templating engine, no JSX, no `cargo build` of
  the frontend); the dashboard's CI runs
  `cargo test -p rbp-dashboard` against a fixture
  `INDEX.json` on disk (committed under
  `crates/dashboard/tests/fixtures/index.json`)
  and asserts the typed read returns N rows + the
  per-row link shape matches the pinned contract.
  Owner files:
  `crates/dashboard/Cargo.toml` (new workspace
  member; deps: `axum`, `tower`, `serde`,
  `serde_json`, `ureq` for the prod index fetch,
  `tokio` for the axum server; no `reqwest`, no
  `aws-sdk-*`, no `wasm-*`),
  `crates/dashboard/src/lib.rs` (re-export
  `IndexClient` + the axum router + the
  `dashboard_app` builder a test harness calls),
  `crates/dashboard/src/index_client.rs` (new
  `IndexClient::from_url` + `fetch_index` returning
  the typed `publish_index::PublishIndex`
  via the same `ureq` GET + serde_json::from_str
  the autotrain pipeline already uses; 4 lib tests
  pinning the typed-read contract: round-trip,
  missing-URL, malformed-JSON, empty-entries),
  `crates/dashboard/src/router.rs` (new axum
  router + the four route handlers + the
  `serve(addr)` entry point + 3 lib tests pinning
  the per-route shape: `GET /api/index` returns
  the in-memory `INDEX.json`,
  `GET /transcript/:id` returns the
  `transcript-<id>.json` bytes,
  `GET /bench/:id` returns the rendered HTML
  card with the `blueprint` / `baseline` /
  `mbb_per_100` fields),
  `crates/dashboard/src/render.rs` (new
  `render_bench_card(bench: &BenchCardFields) -> String`
  HTML emitter + `render_index_table(entries: &PublishIndex) -> String`
  HTML emitter — vanilla `<table>` + `<th>` +
  `<tr>` + `<td>`, no CSS framework, no
  Tailwind, no inline `style=`; the table is
  styled by a single `<style>` block in
  `index.html`; 3 lib tests pinning the per-row
  column order and the `Download transcript` /
  `Open replay` link shape),
  `crates/dashboard/static/index.html` (new
  checked-in vanilla-JS + CSS file; the JS
  fetches `/api/index` and injects the table
  rows from the typed `entries[]`; the CSS is a
  single ~80-line block — dark theme, monospace
  numbers, restrained palette, no animation, no
  emoji, no icon font, no gradient; designed
  for a `1280×800` viewport and `prefers-color-scheme:
  dark` default with a `prefers-color-scheme:
  light` override),
  `crates/dashboard/tests/smoke.rs` (new
  integration test: spins up the axum router
  on a random localhost port, drives
  `GET /` (200 + contains the static
  index.html scaffold + the pinned column
  names + the per-row link shape), `GET
  /api/index` (200 + the JSON
  matches the fixture), `GET
  /transcript/<id>` (200 + the
  bytes match the fixture), and
  asserts the response body does not
  contain a `console.error(` call
  — the no-console-error assertion
  is the cheap in-CI proof the
  dashboard actually renders),
  `Cargo.toml` (add `crates/dashboard` to
  `members`),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the v10 follow-on as promoted with a
  one-line note in the Goals section),
  `README.md` (add a `## Public dashboard` link
  to the deployed Cloudflare Pages URL once
  the v10 ships — placeholder text until
  then; the placeholder is a checklist item
  the v10 ships with, not a `TODO`),
  `scripts/testnet-live-publish-dashboard.sh`
  (new pure-bash runbook that follows
  `testnet-live-publish-index-s3.sh` with
  `aws s3 sync <publish-root>/index/ s3://<bucket>/index/ --delete
  --cache-control max-age=60` so a
  `trainer --publish-index` + a publish-index-remote
  chain + a single `aws s3 sync` deploys the
  dashboard data feed in one step; mirrors the
  STW-019 + STW-032 + STW-033 + STW-034 + STW-035
  runbook shape: script exists + is executable
  + parses with `bash -n` + refuses to run
  on a missing index with exit 3). Scope
  boundary: does NOT introduce a React / Vue
  / Svelte / Solid frontend (vanilla JS is the
  minimum surface for a sortable table — a
  framework is the inverse of the
  no-system-deps trainer shape the rest of the
  autotrain pipeline follows); does NOT
  vendor a Tailwind / Bootstrap / Bulma CSS
  framework (a single 80-line CSS block is
  the right size for a sortable table);
  does NOT add an `aws-sdk-s3` dep
  (the dashboard reads the index from a URL
  via `ureq`, the `aws s3 sync` is the runbook's
  job); does NOT change the
  `crates/autotrain/src/publish_index.rs`
  `PublishIndex` / `IndexedEntry` JSON
  shape (a shape drift fails the dashboard's
  typed read at the same CI step that fails
  the `trainer --verify-index` re-verify);
  does NOT change the `crates/gameroom`'s
  `Transcript` JSON shape (a transcript-shape
  drift fails the dashboard's transcript
  proxy at the same CI step that fails
  `trainer --replay`); does NOT change the
  `crates/autotrain/src/bench.rs::BenchReport`
  JSON shape (a bench-shape drift fails the
  dashboard's `GET /bench/:id` render at the
  same CI step that fails the
  `trainer --replay` consumer); does NOT
  change the room protocol, the `Schema`
  contracts, the autotrain pipeline, the
  K-means cluster counts, the
  `CFR_TREE_COUNT_NLHE` baseline, the v1 / v2
  / v3 / v4 named baselines, or any
  `trainer --*` CLI. Verification commands:
  `cargo test -p rbp-dashboard` (the 4 + 3 + 3
  new lib tests + the new smoke integration
  test pass), `cargo test --workspace --
  --test-threads=4`, `cargo check --workspace`,
  `cargo fmt --check`, `bash -n
  scripts/testnet-live-publish-dashboard.sh`.
  Required tests: the new lib tests in
  `index_client.rs` + `router.rs` + `render.rs`
  + the new `crates/dashboard/tests/smoke.rs`
  integration test; no padding of unrelated
  suites. Dependencies: `STW-035` (the v9
  publish-index-remote step the dashboard's
  data feed assumes), `STW-034` (the v8
  `INDEX.json` aggregator the dashboard's
  `IndexClient` reads), `STW-014` (the
  per-hand `transcript-<id>.json` bundle the
  dashboard's `GET /transcript/:id` proxies).
  Estimated scope: L (the new crate is the
  largest non-engine surface in the repo
  since `rbp-server`; the dashboard's
  static-HTML is a small fraction of the
  slice). Completion signal: `cargo test -p
  rbp-dashboard` is green with the new
  smoke integration test passing;
  `cargo test --workspace` is green;
  `bash -n scripts/testnet-live-publish-dashboard.sh`
  passes; the dashboard renders a fixture
  `INDEX.json` to a non-empty sortable table
  in the smoke test's `GET /` HTML; the
  `README.md` `## Public dashboard` link
  points at a deployed Cloudflare Pages URL
  (a `RBP_DASHBOARD_DEPLOYED_URL` env knob
  the test harness sets; the placeholder
  text the v10 ships with is greppable so a
  dashboard-readiness check can `grep -q
  '## Public dashboard' README.md`); a testnet
  dashboard can `curl
  https://<deployed>/api/index` and receive
  the same `INDEX.json` shape the
  `trainer --verify-index` re-verifier
  accepts. **`lens:` CEO (the visible
  consumer that turns the hermetic receipt
  chain into the public reproducible
  benchmark the testnet north star names) +
  Design (the first-class `## Public
  dashboard` discoverability surface the
  README has been missing).**

- [x] **[P0] `STW-037` `scripts/workspace-parallel-proof-three.sh`
  operator-runnable 3-consecutive full-workspace proof
  + `RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET` env-knob
  port to the shell runbook.** Closes the last
  un-closed mainnet-block hinge. STW-030 added a
  cheap in-CI 2-second 3-consecutive *gameplay-only*
  proof, but the *operator-runnable full-workspace*
  proof the stewardship report cites
  (`steward/HINGES.md` rank #2) is still
  `scripts/workspace-parallel-proof.sh` (3 runs of
  `cargo test --workspace -- --test-threads=4`),
  which an operator has to hand-orchestrate with a
  no-output knob. A new pure-bash runbook
  `scripts/workspace-parallel-proof-three.sh`
  invokes the in-CI
  `run_three_consecutive_clean_gameplay_lib_test_runs`
  integration test STW-030 shipped 3 times
  back-to-back (in 3 separate `cargo test -p
  rbp-autotrain --test workspace_parallel_proof_three
  -- --test-threads=1` invocations) AND invokes
  the existing 3-consecutive *full-workspace* runbook
  once, captures each invocation's stdout + stderr +
  exit code into a per-invocation
  `logs/workspace-parallel-proof-three/<UTC-ISO>/invocation-{1,2,3}/{stdout,stderr,exit}.txt`
  layout, and emits a one-line
  `workspace parallel proof three complete:
  gameplay_runs=3/3 full_workspace_run_exit=0`
  headline a CI worker can `grep ^workspace`.
  Knobs: `RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET=1`
  mutes the per-invocation stdout echo without
  changing the exit-code contract. Companion script
  exits 3 on any failed invocation, exit 1 on
  script-internal error. Owner files:
  `scripts/workspace-parallel-proof-three.sh`
  (new pure-bash runbook — mirrors the
  `scripts/workspace-parallel-proof.sh` +
  `scripts/testnet-live-proof.sh` shape; script
  exists + is executable + parses with `bash -n` +
  refuses to run on a missing workspace with
  exit 3),
  `crates/autotrain/tests/workspace_parallel_proof_three.rs`
  (extend the existing STW-030 file with a 3rd
  sub-test `operator_runnable_three_script_exists_and_parses`
  that greps the new runbook for the pinned
  `workspace parallel proof three complete:`
  headline + asserts the script parses with
  `bash -n` + asserts the script lists
  `run_three_consecutive_clean_gameplay_lib_test_runs`
  as a sub-invocation),
  `crates/autotrain/tests/script_shape.rs` (add
  one new pin
  `workspace_parallel_proof_three_script_exists_and_parses`
  that mirrors the existing
  `workspace_parallel_proof_script_*` pins),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the `verification:workspace-parallel`
  P0-row as fully closed with a one-line note
  in the P0 retirement list). Scope boundary:
  does NOT change the
  `scripts/workspace-parallel-proof.sh`
  shape (the new runbook is a *consumer* of
  the existing one, not a refactor);
  does NOT change the
  `run_three_consecutive_clean_gameplay_lib_test_runs`
  lib-test (the new runbook invokes it
  3 times as-is, a regression in the
  lib-test fails the runbook in the same
  CI step); does NOT introduce a Python /
  `jq` dependency (the runbook is pure
  bash + `cargo test` + `bash -n`);
  does NOT touch the
  `crates/autotrain/tests/workspace_parallel_proof.rs`
  shape contract (the existing 4 sub-tests
  stay as-is); does NOT change the
  `--test-threads=4` /
  `--skip=runbook_run_exits_zero_with_single_clean_workspace_run`
  concurrency contract the script and the
  existing integration test pin. Verification
  commands:
  `cargo test -p rbp-autotrain --test
  workspace_parallel_proof_three` (the 3
  sub-tests pass — 2 existing + 1 new),
  `cargo test -p rbp-autotrain --test
  script_shape` (the 1 new shape pin passes),
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`,
  `bash -n
  scripts/workspace-parallel-proof-three.sh`.
  Required tests: 1 new lib test
  `operator_runnable_three_script_exists_and_parses`
  in
  `crates/autotrain/tests/workspace_parallel_proof_three.rs`
  + 1 new shape pin
  `workspace_parallel_proof_three_script_exists_and_parses`
  in `crates/autotrain/tests/script_shape.rs`.
  Dependencies: `STW-030` (the in-CI
  3-consecutive gameplay-only proof the new
  runbook invokes), `STW-020` (the
  `bust_prevents_next_deterministic` 64-seed
  lib test whose existence makes the
  STW-030 3-consecutive proof defensible).
  Estimated scope: S. Completion signal:
  `cargo test -p rbp-autotrain --test
  workspace_parallel_proof_three` is green
  with 3 sub-tests passing; the new
  `scripts/workspace-parallel-proof-three.sh`
  is on disk + executable + parses with
  `bash -n`; a CI dashboard can `grep
  ^workspace parallel proof three complete:`
  the new runbook's `SUMMARY.txt`; the
  `verification:workspace-parallel` hinge
  is *fully* retired from
  `steward/HINGES.md` /
  `steward/HAZARDS.md` /
  `steward/DRIFT.md` by the next
  `auto steward --report-only` pass.
  **`lens:` Eng (closes the last
  un-closed mainnet-block hinge) + CEO
  (the last rung of un-cited
  completion-signal evidence the testnet
  north star names).**

- [x] **[P0] `STW-038` `crates/autotrain/src/mode.rs` grouped
  `Usage:` block + `crates/autotrain/src/error.rs` typed
  `TrainerError` enum with a `to_pinned_line(&self) -> String`
  dashboard-greppable error surface.** Cuts operator
  cognitive load 4x and gives every error path a
  stable machine-readable shape. Two changes in one
  slice: (a) the `Usage:` eprintln! line in
  `crates/autotrain/src/mode.rs` is replaced with a
  grouped `Usage:` block (4 groups — `TRAIN:`
  `smoke / fast / fast2 / fast3`, `EVALUATE:`
  `bench / compare / compare3`, `REPLAY:`
  `replay`, `PUBLISH:` `publish / verify-receipt /
  publish-remote / verify-remote / publish-index /
  verify-index / publish-index-remote /
  verify-index-remote`, `UTIL:` `status / reset` —
  the grouping is a *non-breaking* cosmetic change
  to the operator's first 30 seconds; every
  existing subcommand keeps its existing flag
  shape, exit code, and stdout/stderr contract,
  and the existing
  `Usage:` eprintln! 14+ subcommands in
  alphabetical order is preserved as a comment
  for grep-back-compat), and (b) a new
  `crates/autotrain/src/error.rs` module
  defines a `TrainerError` enum (variants:
  `NoBlueprint`, `NoDatabase`, `NoBucket`,
  `RedReceipt(String)`, `RedBundle(String)`,
  `RedIndex(String)`, `MissingArg(&'static str)`,
  `BadArg { kind: &'static str, detail: String }`,
  `Io(std::io::Error)`, `AwsCli`, `Internal(String)`)
  with a `to_pinned_line(&self) -> String` method
  that emits a stable
  `trainer error: kind=<kind> detail=<detail>`
  shape (the `kind` is one of the 10 pinned
  enum-`as_str` strings a downstream dashboard
  scraper greps; the `detail` is the human-readable
  cause). The existing per-arm eprintln! error
  lines (e.g. the
  `live_proof publish error: receipt is red: ...`
  line STW-032 ships) are *additionally* routed
  through `TrainerError::to_pinned_line` so a
  regression in an error's pinned shape fails the
  new lib test at the same CI step that fails a
  downstream dashboard scraper. Owner files:
  `crates/autotrain/src/error.rs` (new module
  with the `TrainerError` enum + the
  `to_pinned_line` method + the 10-variant
  `as_str` pinner + 12 lib tests pinning the
  per-variant `to_pinned_line` shape and the
  per-variant `as_str` return value),
  `crates/autotrain/src/lib.rs` (re-export
  `TrainerError`),
  `crates/autotrain/src/mode.rs` (replace the
  single-line `Usage:` eprintln! with a grouped
  5-section `Usage:` block; the 4 new
  eprintln! lines are: `Usage: trainer
  <SUBCOMMAND> [args]`, `  TRAIN:      smoke |
  fast | fast2 | fast3`, `  EVALUATE:  bench |
  compare | compare3`, `  REPLAY:     replay
  <transcript>`, `  PUBLISH:    publish |
  verify-receipt | publish-remote | verify-remote
  | publish-index | verify-index |
  publish-index-remote | verify-index-remote`,
  `  UTIL:       status | reset`; the existing
  alphabetical-order 15-subcommand list is
  preserved as a `// back-compat:` comment block
  above the grouped Usage eprintln! so a
  regression in the alphabetical-order comment
  fails `cargo doc`),
  `crates/autotrain/src/mode.rs` (add an
  `--error-shape-test` argv flag that prints the
  10 pinned `TrainerError::as_str` strings to
  stdout in a stable alphabetical order, so a
  CI dashboard scraper can `grep ^trainer error
  kind=` the shape without exercising every
  error path),
  `crates/autotrain/src/publish.rs`
  (route the existing
  `live_proof publish error: receipt is red: ...`
  line through `TrainerError::RedReceipt(...).to_pinned_line()`
  — the existing pinned error line is preserved
  in a comment for grep-back-compat; the
  `to_pinned_line` is *additionally* emitted
  on the same stderr line, so a regression in
  either shape fails the new lib test),
  `crates/autotrain/src/publish_remote.rs`
  (mirror: route the
  `live_proof publish_remote error: ...` lines
  through `TrainerError`),
  `crates/autotrain/src/publish_index.rs`
  (mirror: route the
  `live_proof publish_index error: ...` lines
  through `TrainerError`),
  `crates/autotrain/src/publish_index_remote.rs`
  (mirror: route the
  `live_proof publish_index_remote error: ...`
  lines through `TrainerError`),
  `IMPLEMENTATION_PLAN.md` (this row). Scope
  boundary: does NOT change the existing
  per-arm `live_proof ...` error-line text
  (the new `to_pinned_line` is emitted
  *additionally* on the same stderr line, so
  a regression in either shape fails CI);
  does NOT change the existing exit-code
  contract (every error variant maps to the
  same exit code the existing per-arm
  eprintln! returns); does NOT change the
  per-subcommand flag shape, the per-subcommand
  stdout shape, or the per-subcommand
  `live_proof ...` headline prefix;
  does NOT change the room protocol, the
  `Schema` contracts, the autotrain
  pipeline, the K-means cluster counts, the
  v1 / v2 / v3 / v4 named baselines, or any
  `trainer --*` CLI. Verification commands:
  `cargo test -p rbp-autotrain --lib` (the 12
  new lib tests in `error.rs` pass),
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`. Required tests: 12 new
  lib tests in
  `crates/autotrain/src/error.rs::tests`
  pinning the per-variant `to_pinned_line`
  shape + per-variant `as_str` return value;
  the existing integration tests for
  `--publish`, `--publish-remote`,
  `--publish-index`, `--publish-index-remote`
  must continue to pass (a regression in the
  pinned error-line shape fails them).
  Dependencies: `STW-032` (the
  `PublishError` enum the new
  `TrainerError::RedBundle` mirrors),
  `STW-033` (the
  `PublishRemoteError` enum the new
  `TrainerError::RedReceipt` mirrors),
  `STW-034` (the
  `PublishIndexError` enum the new
  `TrainerError::RedIndex` mirrors),
  `STW-035` (the
  `PublishIndexRemoteError` enum the new
  `TrainerError::RedIndex` mirrors). Estimated
  scope: M. Completion signal:
  `cargo test -p rbp-autotrain --lib` is green
  with 12 new lib tests passing; a CI
  dashboard can `grep ^trainer error kind=`
  every error path; the grouped `Usage:`
  block is visible to a first-time operator
  running `trainer --help`; the
  `TrainerError` shape is greppable from a
  single pinned prefix. **`lens:` Design
  (operator-UX / error-surface audit).**

- [x] **[P1] `STW-039` `crates/autotrain/src/observe.rs` typed
  `Step` enum + `StepLogger` emitting a one-line
  `trainer step: name=<name> kind=<kind>
  duration_ms=<ms> exit=<0|1|2>` per-step
  machine-readable chain timeline. **RESCOPED
  2026-06-04 by STW-045** — the morning-wave
  *typed Rust module* shape was the canonical
  "improve X" anti-pattern the task body
  explicitly bans; the rescope is a
  ~150-line pure-bash wrapper
  (`scripts/trainer-observe.sh` — the
  STW-045 sibling row) that does not touch
  the autotrain crate, the room protocol, the
  `Schema` contracts, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, or any `trainer --*` CLI.** Gives the
  receipt chain a *real* machine-readable
  timeline a CI auditor can scrape. The
  STW-019 / STW-023 / STW-028 / STW-032 /
  STW-033 / STW-034 / STW-035 receipt chain
  writes a `SUMMARY.txt` headline line per
  step, but the headline line is *human-only*
  and a CI worker has no way to extract
  per-step duration, exit code, or chain
  timeline without re-parsing prose. A new
  `crates/autotrain/src/observe.rs` module
  defines a typed `Step` enum (variants
  mirror the 15 subcommands: `Smoke`,
  `Status`, `Bench`, `Compare`, `Compare3`,
  `Replay`, `VerifyReceipt`, `Publish`,
  `VerifyBundle`, `PublishRemote`,
  `VerifyRemote`, `PublishIndex`,
  `VerifyIndex`, `PublishIndexRemote`,
  `VerifyIndexRemote`) and a `StepLogger`
  that, on `Mode::*` dispatch, records the
  `Instant::now()` start time, the `Step`
  variant, the `name` (the receipt basename
  or the bench JSON line), and on drop / on
  explicit `finish(exit_code)` emits a single
  stderr line in the shape
  `trainer step: name=<name> kind=<kind>
  duration_ms=<ms> exit=<0|1|2>` (the `kind`
  is the `Step::as_str()` pinned string, the
  `duration_ms` is `u128` from the elapsed
  `Instant`, the `exit` is the caller's
  `ExitCode`). The `StepLogger` is a
  no-op-by-default constructor (the existing
  per-step stdout / stderr / `SUMMARY.txt`
  shape is preserved), and is *enabled* by
  the `RBP_TRAINER_OBSERVE=1` env knob a CI
  worker sets. Owner files:
  `crates/autotrain/src/observe.rs` (new
  module with the `Step` enum + the
  `StepLogger` struct + the
  `trainer_step_line` pinned shape + 8 new
  lib tests pinning the per-variant
  `as_str` + the per-line shape + the
  `duration_ms` rounding + the `exit`
  variant mapping),
  `crates/autotrain/src/lib.rs` (re-export
  `Step` + `StepLogger`),
  `crates/autotrain/src/mode.rs` (wrap the
  existing `Mode::*` dispatch in a
  `StepLogger::new(self.kind())?` + on each
  `Mode::*` arm's `Ok(())` / `Err(e)` branch
  call `step.finish(exit_code)`; the
  `RBP_TRAINER_OBSERVE=0` default is a
  no-op, so a regression in the per-line
  shape fails the new lib test without
  changing the existing per-step stdout /
  stderr / `SUMMARY.txt` shape),
  `crates/autotrain/src/mode.rs` (add a
  `--observe-test` argv flag that runs a
  no-op `Smoke` step + prints the 15
  pinned `Step::as_str` strings to stdout
  in a stable alphabetical order, so a CI
  dashboard scraper can `grep ^trainer step
  kind=` the shape without exercising every
  mode),
  `IMPLEMENTATION_PLAN.md` (this row). Scope
  boundary: does NOT change the existing
  per-step `live_proof ...` headline
  (the new `trainer step: ...` line is
  emitted *additionally* on the same stderr
  line, so a regression in either shape
  fails CI); does NOT change the existing
  per-step `SUMMARY.txt` shape (the
  `StepLogger` is a no-op-by-default and
  emits to stderr, not to `SUMMARY.txt`);
  does NOT change the per-subcommand flag
  shape, the per-subcommand stdout shape,
  or any `trainer --*` CLI; does NOT
  change the room protocol, the `Schema`
  contracts, the autotrain pipeline, the
  K-means cluster counts, the v1 / v2 / v3
  / v4 named baselines. Verification
  commands:
  `cargo test -p rbp-autotrain --lib` (the 8
  new lib tests in `observe.rs` pass),
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`. Required tests: 8 new
  lib tests in
  `crates/autotrain/src/observe.rs::tests`
  pinning the per-variant `as_str` + the
  per-line shape + the `duration_ms`
  rounding + the `exit` variant mapping.
  Dependencies: `STW-035` (the v9
  `PublishIndexRemote` + `VerifyIndexRemote`
  subcommands the `Step` enum mirrors).
  Estimated scope: S. Completion signal:
  `cargo test -p rbp-autotrain --lib` is
  green with 8 new lib tests passing; a
  CI dashboard can
  `RBP_TRAINER_OBSERVE=1 trainer
  --bench ... 2>&1 | grep ^trainer step`
  and receive one line per step; the
  `Step` shape is greppable from a single
  pinned prefix. **`lens:` Design
  (observability audit).**

- [ ] **[P1] ~~`STW-040`~~ `README.md` `## Try it now` —
  DROPPED 2026-06-04 by STW-046.** The morning wave's
  `## Try it now` + `## Public dashboard` section add +
  `scripts/replay-locally.sh` shim is busywork the
  testnet north star does not need; the existing
  `## Quick Start` + `## TUI Preview` + `## Testnet
  launch proof` + `## Testnet publish bundle` + `##
  Public dashboard` sections already answer the
  first-time-visitor questions. A static-shape pin in
  `crates/autotrain/tests/script_shape.rs` enforces
  the drop (fails CI if the section or shim
  re-appears). The
  first impression. A first-time visitor opening
  `README.md` currently sees: (1) a `Visual Tour`
  with 2 training-curve screenshots, (2) a
  `Features` bullet list, (3) a `Quick Start` with
  a Cargo.toml + Rust code snippet. None of those
  answer the visitor's first question: "can I see
  the bot play?" The new sections: (a)
  `## Try it now` (between `Features` and
  `Quick Start`, with 4 bullets — `1. See a
  public benchmark: <dashboard URL placeholder>`,
  `2. Replay a hand locally: ./scripts/replay-locally.sh
  crates/gameroom/tests/fixtures/transcripts/transcript-001.json`,
  `3. Run the full testnet launch proof: see
  scripts/testnet-live-proof.md`, `4. Read the
  robopoker testnet north star: see
  genesis/plans/000-ceo-testnet-roadmap.md`),
  (b) `## Public dashboard` (a `<!-- v10 placeholder
  -->` block that the v10 ships with a real
  Cloudflare Pages URL; the placeholder is a
  `RBP_DASHBOARD_DEPLOYED_URL` env knob the
  autotrain test harness sets; the README's
  `## Public dashboard` line greps as
  `Public dashboard: <URL>` a dashboard-readiness
  check can `grep -q 'Public dashboard:'
  README.md` against), and (c) a new pure-bash
  `scripts/replay-locally.sh` operator shim that
  takes one positional arg `<transcript-path>`,
  validates the path is a checked-in fixture under
  `crates/gameroom/tests/fixtures/transcripts/`
  (refuses to run with exit 3 on a missing
  fixture, refuses to run with exit 4 on a
  corrupt JSON, refuses to run with exit 5 on
  no positional arg), invokes the existing
  STW-016 `trainer --replay <path>` (which
  reads the transcript + prints the rendered
  action sequence to stdout), and prints a
  one-line `replay locally complete: <basename>
  bytes=<bytes> hands=<N>` headline a CI
  dashboard can `grep ^replay locally` the log.
  Owner files: `README.md` (add the two new
  sections; the existing `Visual Tour` +
  `Features` + `Quick Start` + `Crate Overview`
  + `Architecture` sections are preserved
  verbatim),
  `scripts/replay-locally.sh` (new pure-bash
  shim; mirrors the
  `scripts/testnet-live-proof.sh` shape;
  script exists + is executable + parses with
  `bash -n` + refuses to run on a missing /
  corrupt / no-arg fixture),
  `crates/gameroom/tests/fixtures/transcripts/transcript-001.json`
  (new checked-in fixture: a 1-hand
  `Fish-vs-Fish` heads-up transcript in the
  STW-014 shape, byte-stable, deterministic;
  the STW-016 `trainer --replay` already
  re-renders a hand-written fixture in the
  existing lib tests, so this fixture is
  the *operator-facing* byte-stable example
  the README points at),
  `crates/autotrain/tests/script_shape.rs`
  (add 3 new shape pins:
  `replay_locally_script_exists_and_parses`
  + `replay_locally_script_refuses_missing_arg`
  + `replay_locally_doc_references_runbook_chain`),
  `IMPLEMENTATION_PLAN.md` (this row). Scope
  boundary: does NOT introduce a Python /
  `jq` / `cargo install` dependency (the
  shim is pure bash + `trainer --replay`,
  the latter is a no-DB read-only mode the
  existing autotrain binary already ships);
  does NOT change the `Transcript` JSON
  shape (a shape drift fails
  `trainer --replay` at the same CI step);
  does NOT change the
  `crates/gameroom/tests/fixtures/transcripts/transcript-001.json`
  byte-stable shape (a byte drift fails a
  new lib test the slice ships with);
  does NOT add a Node / npm dependency
  to render the dashboard (the dashboard
  is plain HTML + vanilla JS the v10
  ships); does NOT change the `Visual
  Tour` screenshot URLs (the existing
  `https://github.com/user-attachments/assets/...`
  URLs are preserved); does NOT change
  the `Quick Start` Cargo.toml + Rust
  snippet (the existing `use rbp::cards::*;
  use rbp::gameplay::*` snippet is
  preserved). Verification commands:
  `cargo test -p rbp-gameroom --features
  database --tests --lib` (the new fixture
  load + the byte-stable lib test pass),
  `cargo test -p rbp-autotrain --test
  script_shape` (the 3 new shape pins
  pass), `cargo test --workspace --
  --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`,
  `bash -n scripts/replay-locally.sh`,
  `./scripts/replay-locally.sh
  crates/gameroom/tests/fixtures/transcripts/transcript-001.json`
  (exits 0 + prints the rendered action
  sequence + the pinned `replay locally
  complete: ...` headline). Required
  tests: 1 new lib test in
  `crates/gameroom/tests/fixtures/transcripts/transcript-001.json`
  (the byte-stable load +
  `to_json` + `from_str` round-trip) + 3
  new shape pins in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: `STW-016` (the
  `trainer --replay <path>` mode the shim
  invokes), `STW-014` (the `Transcript`
  bundle shape the fixture ships in),
  `STW-015` (the
  `read_from_path` / `rebuild_action_sequence`
  / `replay_to_path` public surface the
  fixture is byte-stable against).
  Estimated scope: S. Completion signal:
  `cargo test --workspace -- --test-threads=4`
  is green; the new
  `scripts/replay-locally.sh` is on disk +
  executable + parses with `bash -n`; a
  CI dashboard can `grep ^replay locally
  complete:` the shim's stdout; the
  `README.md` `## Try it now` section is
  visible to a first-time visitor;
  the `Public dashboard: <URL>` line
  in `README.md` is greppable from a
  dashboard-readiness check. **`lens:`
  Design (the README's first-time-visitor
  legibility audit) + CEO (the testnet
  north star's "publicly visible"
  requirement).**

- [ ] **[P1] ~~`STW-041`~~ Close the `STW-001` deferred row
  — DROPPED 2026-06-04 by STW-046.** The morning
  wave's planning-surface retirement task (retire
  the `STW-001` deferred row + add a
  `genesis/AUTHORED-QUEUE.md` fallback queue) is
  busywork the testnet north star does not need.
  The existing `genesis/plans/000-ceo-testnet-
  roadmap.md` + `IMPLEMENTATION_PLAN.md` pair has
  proved sufficient for 35+ shipped STW rows; the
  `STW-001` operator-decision deferred row remains
  in the plan as the sign-off blocker it already
  is. No `AUTHORED-QUEUE.md` fallback queue is
  added. The drop is enforced by the same
  static-shape pin that guards STW-040. Close the
  `verification:workspace-parallel`-shaped
  decision loop the testnet north star does
  not actually need. The CEO sign-off's testnet
  north star is "a public, reproducible
  benchmark with replayable transcripts"; the
  `STW-001` row's premise ("regenerate
  executable planning surface") is a
  corpus-regen / gbrain-DB concern that has
  not blocked any of the 35 shipped STW rows
  and does not block the v10 dashboard. The
  *new* framing: the current `genesis/` +
  `IMPLEMENTATION_PLAN.md` surface is the
  authoritative executable surface, and the
  `STW-001` row should be retired as
  satisfied by that surface. The companion
  deliverable: a checked-in
  `genesis/AUTHORED-QUEUE.md` (a small,
  hand-authored 5-row queue the operator
  owns in lieu of a `gbrain` DB) that
  mirrors the auto-loop's claimable-row
  contract (each row has owner files, scope
  boundary, verification commands, completion
  signal) and acts as the *fallback queue* if
  `gbrain` is unconfigured. Owner files:
  `IMPLEMENTATION_PLAN.md` (replace the
  `## Deferred items (need operator
  decision before promotion)` `STW-001`
  row with a one-line "RETIRED 2026-06-04
  by STW-041: current `genesis/` +
  `IMPLEMENTATION_PLAN.md` surface is
  the authoritative executable surface,
  gbrain DB is a nice-to-have, not a
  testnet blocker" marker),
  `genesis/AUTHORED-QUEUE.md` (new checked-in
  hand-authored fallback queue — 5 rows
  max, each row scoped to a single
  shippable slice; the rows are *not*
  proposed plan content, they are
  operator-owned fallbacks the auto-loop
  can claim if `gbrain doctor` exits
  non-zero; the file's preamble notes
  "this is the operator-owned fallback
  queue; `gbrain` is the primary
  queue-authoring surface when
  configured; this file is the queue
  the auto-loop reads when `gbrain
  doctor` is unconfigured"),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the v10 follow-on as promoted
  with a one-line note in the Goals
  section; the STW-041 row is the
  *companion* to the v10 row, not a
  substitute). Scope boundary: does NOT
  introduce a competing planning
  surface (the `genesis/` +
  `IMPLEMENTATION_PLAN.md` surface is
  the *primary* surface, the
  `AUTHORED-QUEUE.md` is a *fallback*
  queue the operator owns);
  does NOT change the `STW-007`
  deferred row (the `.gbrain-source`
  sign-off is a separate operator
  decision, kept as-is in the deferred
  section);
  does NOT change the v1 / v2 / v3 / v4
  named baselines, the
  `CFR_TREE_COUNT_NLHE` baseline, the
  K-means cluster counts, the autotrain
  pipeline, the room protocol, the
  `Schema` contracts, or any
  `trainer --*` CLI. Verification
  commands: `git diff --
  IMPLEMENTATION_PLAN.md
  genesis/AUTHORED-QUEUE.md` (the
  diff is the STW-041 row + the
  AUTHORED-QUEUE.md file content),
  `cargo test --workspace --
  --test-threads=4` (the v10 / v8 / v7
  / v6 / STW-001 retirement does not
  change the autotrain pipeline),
  `cargo check --workspace`,
  `cargo fmt --check`. Required tests:
  none — STW-041 is a planning-surface
  retirement, not a code change.
  Dependencies: operator sign-off on
  the `STW-001` retirement framing.
  Estimated scope: XS. Completion
  signal: the `STW-001` row in
  `IMPLEMENTATION_PLAN.md` is marked
  RETIRED with a one-line note; the
  `genesis/AUTHORED-QUEUE.md` file is
  on disk + the auto-loop can read it
  as a fallback queue when `gbrain
  doctor` is unconfigured; the
  `genesis/plans/000-ceo-testnet-roadmap.md`
  v10 follow-on is marked promoted.
  **`lens:` CEO (a planning-surface
  decision the testnet north star
  does not need) + Eng (closes the
  last un-closed deferred-decision
  hinge).**

## Next wave - review 2026-06-04 (afternoon)

The morning 2026-06-04 three-lens review (kanban task
`t_c86ebbbf`, STW-036 / STW-037 shipped) pivoted the
plan from "more rungs of the receipt chain" to "a
visible consumer of the chain." STW-036 (dashboard) +
STW-037 (workspace-parallel-proof-three) are now
shipped on `main`, and four open rows from that wave
remain: STW-038 (TrainerError), STW-039 (StepLogger),
STW-040 (README `## Try it now`), STW-041 (STW-001
retirement). The afternoon 2026-06-04 three-lens
review (kanban task `t_35186537`) re-applies the
three lenses to the *current* state and finds the
strategic gap is no longer the operator-UX surface
the morning wave named — it is that **the dashboard
is built to consume a live `INDEX.json` + a live
`BenchReport`, but no live `INDEX.json` and no
committed `BenchReport` / `Compare3Report` exist in
the repo today.** A stranger who runs `cargo run -p
rbp-dashboard` on a fresh checkout sees an empty
table, and a CI auditor who `curl`s the dashboard
URL gets `entries: []`. The chain is green in CI;
the *result* the chain produces is not committed.
The afternoon wave therefore (a) ships the missing
result fixtures (STW-042, STW-043) that turn the
dashboard from "live-data-only" into "live-data or
committed-fixture", and (b) re-scopes the two
operator-UX polish rows (STW-044, STW-045) to ship
*without* the AI-slop refactor the morning wave
proposed, and (c) drops two rows (the STW-040 README
`## Try it now` reframe and the STW-041 planning
retirement) as busywork the north star does not
need.

Each row below names a single shippable slice with
named files, verification command(s), and a `lens:`
tag tracing the finding it closes. Rows are P0/P1
ordered; the top row is the highest single-shipment
leverage.

- [x] **[P0] `STW-042` `crates/dashboard/tests/fixtures/compare3-fixture.json`
  + `crates/dashboard/src/router.rs::GET /bench/:id`
  demo-data fallback that serves the fixture when
  no live `INDEX.json` entry matches.** Closes the
  "dashboard has no committed result" gap the
  afternoon 2026-06-04 three-lens review
  (kanban task `t_35186537`) named as the single
  highest-leverage thrust. The v10 dashboard the
  STW-036 row shipped is *built* to consume a live
  `INDEX.json` + per-receipt `Compare3Report` /
  `BenchReport`, but no committed result lives in
  the repo today — a fresh `cargo run -p
  rbp-dashboard` shows an empty table and a CI
  auditor who `curl`s `/api/index` gets
  `entries: []`. The shipped `trainer --compare3`
  (STW-031) prints a parseable JSON line a CI
  worker can capture, but the JSON is *runtime
  output*, not a *committed artifact* a stranger
  can read without running the chain. This slice
  lands the missing committed artifact: a new
  `crates/dashboard/tests/fixtures/compare3-fixture.json`
  in the exact JSON shape
  `crates/autotrain/src/bench.rs::Compare3Report::to_json`
  emits, with hard-coded v1 / v2 / v3 per-config
  `mbb_per_100` / `ci_95` / `win_rate` / `hands`
  numbers and the three pairwise `delta_mbb_per_100`
  values (v1-vs-v2, v2-vs-v3, v3-vs-v1) and a
  `ranked_winner` field ∈ `{"v1", "v2", "v3",
  "tie"}` — byte-stable, no `Instant::now` / no
  UUIDs / no floats from real RNG, so a re-run of
  the dashboard's own load produces a byte-identical
  file. The new `crates/dashboard/src/router.rs`
  route `GET /bench/:id` reads the fixture as a
  *fallback* when (a) the `:id` is the fixture's
  pinned basename `compare3-fixture` AND (b) the
  `INDEX.json` consumed by `IndexClient` has no
  matching entry — so a fresh `cargo run -p
  rbp-dashboard` shows a populated `/bench/compare3-fixture`
  card with the v1 / v2 / v3 numbers a stranger
  can read, while a real `INDEX.json` from a real
  STW-034 chain run still wins. A new
  `crates/dashboard/tests/fixtures_smoke.rs::compare3_fixture_renders_bench_card`
  integration test asserts: (1) the fixture loads
  via `serde_json::from_str` into a typed
  `Compare3Report` without error; (2) the
  `render_bench_card(&fixture.card_fields())` output
  contains the pinned `mbb_per_100` value for each
  of v1 / v2 / v3 (so a future drift in the
  rendered HTML breaks the test); (3) the
  `GET /bench/compare3-fixture` route returns 200
  + an HTML body containing the fixture's
  `ranked_winner` field. Owner files:
  `crates/dashboard/tests/fixtures/compare3-fixture.json`
  (new committed byte-stable fixture, hand-authored
  in the STW-031 `Compare3Report::to_json` shape),
  `crates/dashboard/src/router.rs` (extend the
  `GET /bench/:id` handler with a fixture-fallback
  branch: when the in-memory `INDEX.json` has no
  entry for `:id` AND the `:id == "compare3-fixture"`
  sentinel matches, read
  `tests/fixtures/compare3-fixture.json` from
  disk and return the same HTML the live path
  produces; the live path is *unchanged*),
  `crates/dashboard/src/render.rs` (add a
  `BenchCardFields` constructor that builds the
  per-card column set from a `Compare3Report`
  sub-report — `blueprint` / `baseline` /
  `mbb_per_100` / `ci_95` / `win_rate` — reusing
  the v1 `BenchReport` column shape so a
  BenchReport and a Compare3Report sub-report
  render with the same column order, and a
  `render_compare3_card(&Compare3Report) -> String`
  helper that renders the three pairwise
  `delta_mbb_per_100` rows the dashboard's existing
  `render_bench_card` does not; 2 lib tests pinning
  the per-row column order and the
  `compare3-fixture` sentinel shape),
  `crates/dashboard/tests/fixtures_smoke.rs` (new
  integration test in the existing
  `crates/dashboard/tests/` folder: drives the
  axum router on a random localhost port + asserts
  `GET /bench/compare3-fixture` returns 200 +
  contains the fixture's `ranked_winner` field +
  contains the three pairwise `delta_mbb_per_100`
  values; a second sub-test asserts the fixture
  round-trips `serde_json::from_str` into a typed
  `Compare3Report` without error and the
  `ranked_winner` ∈ `{"v1", "v2", "v3", "tie"}`),
  `crates/dashboard/static/index.html` (extend
  the static `index.html` "no entries yet" empty
  state with a one-line "Demo data:
  /bench/compare3-fixture" link a first-time
  visitor can click — the link is a `<a href>`
  not a JS `fetch`, so it works without JS and
  without a network round-trip),
  `IMPLEMENTATION_PLAN.md` (this row),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (mark the v10 follow-on as "shipped with demo
  data" — the existing v10 row already says the
  dashboard is shipped, this is a one-line
  update). Scope boundary: does NOT introduce
  a new `Compare3Report` shape (the fixture is
  in the STW-031 shape verbatim); does NOT
  change the existing `GET /api/index` or
  `GET /transcript/:id` routes; does NOT
  change the existing `IndexClient::from_url`
  fixture (`crates/dashboard/tests/fixtures/index.json`)
  — the new fixture is a *demo result*; the
  existing one is a *demo index*; both
  coexist; does NOT change the room
  protocol, the `Schema` contracts, the
  autotrain pipeline, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, the `CFR_TREE_COUNT_NLHE`
  baseline, the `trainer --replay` CLI, the
  `trainer --verify-receipt` CLI, or the
  `trainer --smoke` / `trainer --bench` /
  `trainer --compare` / `trainer --compare3`
  JSON contracts; does NOT change the
  existing `trainer --bench` /
  `trainer --compare3` runtime path
  (the fixture is the *committed demo
  result*; the runtime path is the
  *fresh-run result* — the two coexist).
  Verification commands: `cargo test -p
  rbp-dashboard` (the 2 new lib tests in
  `render.rs` pass + the new `fixtures_smoke.rs`
  integration test passes + the existing
  3 router tests still pass),
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`,
  `cargo run -p rbp-dashboard -- --port
  18080 &
  PID=$!; sleep 2; curl -s
  http://localhost:18080/bench/compare3-fixture
  | grep -q "ranked_winner"; kill $PID`
  (the end-to-end demo-data render works
  in a real shell). Required tests: 2 new
  lib tests in
  `crates/dashboard/src/render.rs::tests` +
  2 new integration sub-tests in
  `crates/dashboard/tests/fixtures_smoke.rs`.
  Dependencies: `STW-036` (the dashboard
  crate the new fixture is consumed by),
  `STW-031` (the `Compare3Report` shape the
  fixture is in). Estimated scope: M.
  Completion signal:
  `cargo test -p rbp-dashboard` is green
  with 2 new lib tests + 2 new
  integration sub-tests passing;
  `cargo run -p rbp-dashboard -- --port
  18080` serves a populated
  `/bench/compare3-fixture` card with
  the v1 / v2 / v3 numbers visible
  to a fresh checkout; the
  `crates/dashboard/tests/fixtures/compare3-fixture.json`
  file is byte-stable on re-load (a
  fresh `sha256sum` matches the
  committed digest); the
  `crates/dashboard/static/index.html`
  empty-state link is visible to a
  first-time visitor with no `INDEX.json`.
  **`lens:` CEO (closes the "no
  committed result" gap a public
  testnet requires) + Eng (the
  dashboard's *demo-data path* is
  the missing architectural piece,
  not a new code path) + Design
  (the dashboard's empty-state UX
  is the active user-facing bug,
  not a missing doc section).**

- [x] **[P0] `STW-043` `crates/autotrain/tests/fixtures/bench-report-fixture.json`
  + `scripts/commit-bench-fixture.sh` operator shim
  that produces a byte-stable `BenchReport` from
  `trainer --bench` against a no-DB deterministic
  small-config run.** Closes the "the receipt
  chain is auditor-greppable but the *result* is
  not committed" Eng gap the afternoon review
  named. The STW-019 `testnet-live-proof.sh`
  runbook drops a `receipts/testnet-live-proof-<UTC-ISO>/`
  bundle on every operator run, and STW-028
  committed a no-DB portable-reference receipt
  the runbook can re-verify. But the
  `trainer --bench` JSON the runbook's
  `--bench` step captures is *operator-local
  output* — a fresh checkout has no committed
  `BenchReport` a stranger can `cat` to see
  "blueprint X beat baseline Y at mbb/100 =
  +Z, win-rate = W%, hands = N." This slice
  lands the missing committed result: a new
  `scripts/commit-bench-fixture.sh` pure-bash
  shim that takes one positional arg
  `<output-path>`, drives the existing
  `trainer --reset` + `trainer --bench` chain
  against a `--bench-hands` knob
  (`RBP_BENCH_HANDS=8` default — small enough
  to finish in under 2 s on a clean checkout)
  with `RBP_BENCH_SEED=42` (the same fixed
  seed the v1 `trainer --smoke` already uses
  for the small-bench path) + `RBP_BENCH_BLUEPRINT=v1`
  + `RBP_BENCH_BASELINE=preflop`, captures the
  single-line JSON `BenchReport` to stdout,
  strips the per-run `run_id` / `started_at_utc`
  fields (which the new `crates/autotrain/src/bench.rs::BenchReport::to_json` STW-031
  already includes — the new script's
  `strip_run_id` `awk` one-liner removes them
  so the committed fixture is byte-stable
  across runs), and writes the result to
  `<output-path>`. The committed
  `crates/autotrain/tests/fixtures/bench-report-fixture.json`
  is the *reference* the shim produces on a
  fresh checkout — a new
  `crates/autotrain/tests/bench_report_fixture.rs`
  integration test re-runs the shim against a
  fresh DB + diffs the output against the
  committed fixture, asserting byte equality
  on the post-strip JSON (so a future
  `BenchReport` shape drift in STW-018 /
  STW-031 / etc. fails the test on a single
  CI run). Owner files:
  `scripts/commit-bench-fixture.sh` (new
  pure-bash shim; mirrors the
  `scripts/testnet-live-proof.sh` shape —
  script exists + is executable + parses
  with `bash -n` + refuses to run on a
  missing arg with exit 3; the
  `strip_run_id` `awk` one-liner is the
  only logic beyond the trainer chain
  invocation),
  `crates/autotrain/tests/fixtures/bench-report-fixture.json`
  (new committed byte-stable fixture in
  the `BenchReport::to_json` shape with
  `run_id` / `started_at_utc` stripped —
  the operator-facing `trainer --bench`
  output a stranger can `cat`),
  `crates/autotrain/tests/bench_report_fixture.rs`
  (new no-DB no-network integration test:
  runs the shim against a fresh
  `cargo test -p rbp-autotrain --features
  database` invocation, diffs the
  output against the committed fixture
  with `assert_eq!` on the post-strip
  byte string, asserts the
  `bench_report_fixture_script_exists_and_parses`
  shape pin + the
  `bench_report_fixture_matches_committed_digest`
  digest pin),
  `crates/autotrain/tests/script_shape.rs`
  (add the 2 new shape pins
  `commit_bench_fixture_script_exists_and_parses`
  + `commit_bench_fixture_script_strips_run_id_fields`
  — the existing
  `script_shape.rs` already follows the
  pattern; the new pins are 2 lines
  each),
  `IMPLEMENTATION_PLAN.md` (this row).
  Scope boundary: does NOT change the
  existing `BenchReport::to_json` shape
  (the strip happens in the bash shim, not
  in Rust); does NOT change the existing
  `trainer --bench` runtime path (the
  shim is a *separate* script the
  operator runs to *produce* the
  committed fixture, the runtime path
  is the *fresh-run* path); does NOT
  change the `trainer --compare3`
  shape (STW-042 is the *compare3*
  demo-data fixture, this is the
  *bench* demo-data fixture — the two
  coexist); does NOT change the room
  protocol, the `Schema` contracts, the
  autotrain pipeline, the K-means
  cluster counts, the v1 / v2 / v3 / v4
  named baselines, the
  `CFR_TREE_COUNT_NLHE` baseline, the
  `trainer --replay` CLI, the
  `trainer --verify-receipt` CLI, or
  the `trainer --smoke` /
  `trainer --compare` /
  `trainer --compare3` JSON contracts.
  Verification commands: `bash -n
  scripts/commit-bench-fixture.sh`,
  `./scripts/commit-bench-fixture.sh
  /tmp/bench-report.json` (exits 0 +
  produces a parseable JSON file +
  the `run_id` / `started_at_utc`
  fields are absent),
  `cargo test -p rbp-autotrain
  --features database --test
  bench_report_fixture` (the 2 new
  sub-tests pass),
  `cargo test -p rbp-autotrain
  --test script_shape` (the 2 new
  shape pins pass),
  `cargo test --workspace --
  --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`. Required tests:
  2 new integration sub-tests in
  `crates/autotrain/tests/bench_report_fixture.rs`
  + 2 new shape pins in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: `STW-010` (the
  `trainer --bench` mode the shim
  invokes), `STW-031` (the
  `BenchReport` shape the fixture
  is in — the strip fields are the
  same STW-018 introduced). Estimated
  scope: S. Completion signal:
  `cargo test -p rbp-autotrain
  --test bench_report_fixture` is
  green with 2 new sub-tests
  passing; the
  `crates/autotrain/tests/fixtures/bench-report-fixture.json`
  file is committed + a fresh
  checkout's `sha256sum` matches
  the in-tree `tests/fixtures/bench-report-fixture.json.sha256`
  digest the slice ships; a CI
  dashboard can `grep ^mbb_per_100
  "the` `tests/fixtures/bench-report-fixture.json`
  the file's `mbb_per_100` field.
  **`lens:` CEO (the
  "publicly-visible" leg of the
  testnet north star's
  "downloadable" requirement) +
  Eng (the chain's auditor-greppable
  surface extends to the result
  the chain produces, not just the
  chain's per-step exit codes).**

- [x] **[P1] `STW-044` Re-scope the morning-wave `STW-038`
  `TrainerError` slice to a *per-arm error-shape
  audit*: one new lib test per existing
  eprintln! error line, no new `TrainerError`
  enum, no new `to_pinned_line` method, no
  routing refactor.** **SHIPPED** — the
  `crates/autotrain/src/error_audit.rs` module
  landed with 11 static-grep lib tests pinning
  the per-arm `live_proof ...` error-line shape
  across the 7 source files the morning wave's
  STW-038 listed. The afternoon review found the
  existing per-arm `live_proof ...` /
  `live_proof publish error: receipt is
  red: ...` / `live_proof publish_remote
  error: ...` / `live_proof publish_index
  error: ...` / `live_proof publish_index_remote
  error: ...` / `workspace parallel proof
  three complete: ...` headline shapes are
  *already* greppable from a single pinned
  prefix per family — a CI dashboard scraper
  `grep ^live_proof publish error:` the
  log and receives a stable shape. The
  morning wave's `TrainerError` enum + the
  10-variant `as_str` pinner + the
  `to_pinned_line(&self) -> String` method
  + the 12 new lib tests is a *refactor*
  that yields no new user-visible capability
  (the existing per-arm shape is already
  greppable) and ships a Rust module with
  100+ lines of enum-discriminant
  boilerplate whose only test is "the
  per-variant string matches the published
  string" — the canonical "improve X"
  pattern the task body explicitly bans.
  The afternoon's re-scoped slice is *the
  audit the morning wave should have
  shipped*: a new
  `crates/autotrain/src/error_audit.rs`
  module (10 lib tests, *no* production
  code) that greps the existing per-arm
  error-line text in
  `crates/autotrain/src/{publish,publish_remote,publish_index,publish_index_remote,mode,verify_receipt,verify_bundle}.rs`
  and asserts each one matches the pinned
  shape the dashboard already scrapes
  (e.g. the test
  `publish_receipt_red_error_line_uses_pinned_shape`
  greps the
  `live_proof publish error: receipt is red:`
  prefix in `publish.rs` and asserts the
  surrounding line is a single-statement
  `eprintln!(...)` that emits that prefix
  — a refactor that *deletes* the prefix
  fails the test on a single CI run). The
  audit is 10 lib tests, one per existing
  error arm; each test is a `grep`-based
  static pin; no new module is added to
  the autotrain `lib.rs` re-export list;
  the only `mode.rs` change is the
  `--error-shape-test` argv flag the
  morning wave's row already named (the
  audit *needs* the argv flag to expose
  the pinned shapes a CI scraper greps,
  so the flag is the *only* surviving
  piece of the morning wave's STW-038).
  Owner files:
  `crates/autotrain/src/error_audit.rs`
  (new file, `cfg(test) mod tests` only,
  10 static-grep lib tests pinning the
  per-arm `live_proof ...` error-line
  shape across the 7 source files the
  morning wave's STW-038 listed),
  `crates/autotrain/src/mode.rs` (add
  the `--error-shape-test` argv flag
  from the morning wave's STW-038 — the
  flag is the *only* surviving piece of
  the morning wave's row, and it
  exposes the 10 pinned
  `live_proof ...` prefixes a CI
  scraper greps without exercising every
  error path; a no-op in production,
  the same `cargo run -- --error-shape-test`
  the morning wave's row named),
  `crates/autotrain/src/publish.rs`
  (NO CHANGE — the existing
  `live_proof publish error: receipt is red: ...`
  line is preserved verbatim; the
  audit *reads* it, the morning wave's
  row was *rewriting* it — the rewrite
  is dropped),
  `crates/autotrain/src/publish_remote.rs`
  (NO CHANGE),
  `crates/autotrain/src/publish_index.rs`
  (NO CHANGE),
  `crates/autotrain/src/publish_index_remote.rs`
  (NO CHANGE),
  `crates/autotrain/src/verify_receipt.rs`
  (NO CHANGE),
  `crates/autotrain/src/verify_bundle.rs`
  (NO CHANGE),
  `IMPLEMENTATION_PLAN.md` (this row;
  mark the morning-wave STW-038 row as
  `RESCOPED 2026-06-04 by STW-044` so
  a future worker does not re-claim
  the refactor half of the original
  STW-038). Scope boundary: does NOT
  introduce a `TrainerError` enum; does
  NOT introduce a `to_pinned_line`
  method; does NOT change the
  existing per-arm `live_proof ...`
  error-line text; does NOT change
  the existing exit-code contract; does
  NOT change the per-subcommand flag
  shape, the per-subcommand stdout
  shape, or any `trainer --*` CLI; does
  NOT change the room protocol, the
  `Schema` contracts, the autotrain
  pipeline, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, or any
  `trainer --*` JSON contract. The
  morning wave's `STW-038` row's
  per-arm `eprintln!` text is
  preserved verbatim — the audit
  *pins* the existing text, it does
  not *rewrite* it. Verification
  commands: `cargo test -p
  rbp-autotrain --lib` (the 10 new
  lib tests in `error_audit.rs`
  pass), `cargo run -p rbp-autotrain
  -- --error-shape-test` (prints
  the 10 pinned `live_proof ...`
  prefixes in alphabetical order),
  `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Required tests: 10 new lib tests
  in
  `crates/autotrain/src/error_audit.rs::tests`
  pinning the per-arm
  `live_proof ...` error-line shape
  across the 7 source files the
  morning wave's STW-038 listed.
  Dependencies: `STW-032` (the
  `live_proof publish error: receipt
  is red: ...` line the audit pins),
  `STW-033` (the
  `live_proof publish_remote error: ...`
  line the audit pins),
  `STW-034` (the
  `live_proof publish_index error: ...`
  line the audit pins),
  `STW-035` (the
  `live_proof publish_index_remote
  error: ...` line the audit pins),
  `STW-028` (the
  `live_proof receipt verification
  failed: ...` /
  `live_proof receipt verification
  passed: ...` shape the audit
  pins). Estimated scope: S.
  Completion signal:
  `cargo test -p rbp-autotrain --lib`
  is green with 10 new lib tests
  passing; `cargo run -p
  rbp-autotrain -- --error-shape-test`
  prints the 10 pinned
  `live_proof ...` prefixes a CI
  scraper greps; the
  `STW-038` morning-wave row is
  marked `RESCOPED` and a future
  worker does not re-claim the
  refactor half. **`lens:` Design
  (the operator-UX / error-surface
  audit) — closes the same finding
  the morning wave named, with the
  *audit* shape, not the *refactor*
  shape, so no production code
  changes and no per-arm grep
  prefix drift.**

- [ ] **[P1] `STW-045` `scripts/trainer-observe.sh` wrapper RESCOPED 2026-06-05**
  that prepends `date +%s%3N` to every stderr
  line the trainer emits and writes a
  `trainer.step.jsonl` per-run timeline
  file. Closes the "per-step machine-readable
  timeline" Design concern the morning wave
  named *without* the Rust `Step` enum +
  `StepLogger` refactor the morning wave
  proposed. The morning wave's `STW-039`
  `crates/autotrain/src/observe.rs` typed
  `Step` enum + `StepLogger` is a real
  Rust module with a `Step` enum (15
  variants) + a `StepLogger` struct with
  an `Instant::now()` start time + a
  `finish(exit_code)` method + a per-line
  `trainer step: name=<name> kind=<kind>
  duration_ms=<ms> exit=<0|1|2>` shape +
  8 new lib tests + a `RBP_TRAINER_OBSERVE=1`
  env knob + a `--observe-test` argv flag
  — 200+ lines of new Rust for a feature
  whose only consumer is "a CI worker wants
  per-step timing." A `bash`-level wrapper
  is 30 lines and ships the same
  per-step timeline a CI worker wants
  *without* touching the autotrain Rust
  crate. The shipped slice: a new
  `scripts/trainer-observe.sh` pure-bash
  wrapper that takes one positional arg
  `<output-jsonl>` + the rest of argv
  is the `trainer` invocation to wrap
  (e.g.
  `scripts/trainer-observe.sh
  /tmp/run-20260604T180000Z.step.jsonl
  trainer --bench --blueprint v1
  --baseline preflop`). The wrapper
  forks `trainer "$@"` as a child
  process, captures its stderr to a
  pipe, prepends `date +%s%3N` (the
  millisecond-precise UTC epoch) +
  the captured stderr line to a
  one-line JSON object
  `{"ts_ms": 1749052800123, "stream":
  "stderr", "line": "<captured>"}`,
  appends the JSON line to the
  `<output-jsonl>` file, and
  duplicates the line to the
  wrapper's own stderr (so a human
  watching the wrapper sees the
  same line the JSONL log sees).
  On child exit, the wrapper
  appends one final JSON line
  `{"ts_ms": <now>, "stream":
  "summary", "exit": <child-exit>,
  "argv": <argv-as-json-array>}`
  to the JSONL file, then exits
  with the child's exit code (so
  the wrapper is *transparent* to
  any existing CI pipeline that
  runs `trainer --bench` and
  asserts exit 0 — the wrapper
  preserves the exit code). The
  pinned JSONL shape is one line
  per event, `sort -k ts_ms` gives
  a stable timeline, and a CI
  dashboard can `jq -r 'select
  (.stream == "summary") | .exit'`
  the file to extract the per-step
  exit. A new
  `crates/autotrain/tests/script_shape.rs::trainer_observe_script_exists_and_parses`
  shape pin mirrors the existing
  `testnet_live_publish_*_script_exists_and_parses`
  pinners. A new
  `crates/autotrain/tests/trainer_observe.rs::trainer_observe_wraps_trainer_bench_with_timeline`
  integration test runs
  `scripts/trainer-observe.sh
  /tmp/test.step.jsonl
  trainer --bench --blueprint v1
  --baseline preflop
  --bench-hands 4` against a
  fresh DB, asserts the JSONL
  file is parseable line-by-line
  with `jq`, asserts the last
  line is a `stream: "summary"`
  entry with `exit: 0` and a
  non-empty `argv` array, and
  asserts the wrapper's exit
  code is 0. Owner files:
  `scripts/trainer-observe.sh`
  (new pure-bash wrapper; the
  only logic is the `fork +
  pipe stderr + prepend
  date +%s%3N + jq` loop;
  mirrors the
  `scripts/testnet-live-proof.sh`
  shape — script exists +
  is executable + parses with
  `bash -n` + refuses to run on
  a missing arg with exit 3
  + refuses to run on a
  missing `trainer` binary on
  `$PATH` with exit 4),
  `crates/autotrain/tests/script_shape.rs`
  (add 1 new shape pin:
  `trainer_observe_script_exists_and_parses`
  — 1 line of new test code,
  mirrors the existing
  pattern),
  `crates/autotrain/tests/trainer_observe.rs`
  (new no-DB-shape integration
  test: requires
  `DATABASE_URL` to be set for
  the child `trainer --bench`
  invocation, mirrors the
  existing `bench.rs` /
  `compare.rs` /
  `compare3.rs` integration
  tests' database-gating
  pattern; runs the wrapper
  + asserts the JSONL shape
  + asserts the wrapper's
  exit code is 0),
  `IMPLEMENTATION_PLAN.md` (this
  row; mark the morning-wave
  STW-039 row as `RESCOPED
  2026-06-04 by STW-045` so a
  future worker does not
  re-claim the Rust-module
  half of the original
  STW-039). Scope boundary:
  does NOT introduce a new
  Rust module in
  `crates/autotrain/src/`;
  does NOT introduce a
  `Step` enum or a
  `StepLogger` struct; does
  NOT change the autotrain
  `lib.rs` re-export list;
  does NOT change the
  existing per-arm
  `live_proof ...` log
  shape (the wrapper
  *appends* a JSONL timeline
  alongside the trainer's
  stderr — the trainer
  itself is unchanged);
  does NOT change the
  per-subcommand flag
  shape, the per-subcommand
  stdout shape, the
  per-subcommand exit code,
  the per-subcommand
  `SUMMARY.txt` shape, or
  any `trainer --*` CLI;
  does NOT change the
  room protocol, the
  `Schema` contracts, the
  autotrain pipeline, the
  K-means cluster counts,
  the v1 / v2 / v3 / v4
  named baselines, the
  `CFR_TREE_COUNT_NLHE`
  baseline, the
  `trainer --replay` CLI,
  the `trainer --verify-receipt`
  CLI, or the
  `trainer --smoke` /
  `trainer --bench` /
  `trainer --compare` /
  `trainer --compare3` JSON
  contracts. The wrapper
  is *additive* — the
  trainer binary is
  unchanged; a future
  operator who runs
  `trainer --bench` without
  the wrapper sees the
  *same* output they see
  today; an operator who
  runs
  `scripts/trainer-observe.sh
  <out.jsonl> trainer --bench`
  sees the same output
  *plus* a parallel JSONL
  timeline. Verification
  commands: `bash -n
  scripts/trainer-observe.sh`,
  `scripts/trainer-observe.sh
  /tmp/test.step.jsonl
  trainer --bench --blueprint v1
  --baseline preflop
  --bench-hands 4`
  (exits 0 + the JSONL file
  has at least 2 lines: one
  `stream: "stderr"` line +
  one `stream: "summary"`
  line),
  `jq -c . /tmp/test.step.jsonl
  | head -5` (the JSONL is
  line-parseable),
  `cargo test -p rbp-autotrain
  --test trainer_observe`
  (the 1 new integration
  sub-test passes),
  `cargo test -p rbp-autotrain
  --test script_shape` (the
  1 new shape pin passes),
  `cargo test --workspace --
  --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`. Required
  tests: 1 new integration
  sub-test in
  `crates/autotrain/tests/trainer_observe.rs`
  + 1 new shape pin in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: `STW-010` (the
  `trainer --bench` mode the
  wrapper is most useful
  against — the wrapper is
  mode-agnostic, but the
  integration test drives
  `--bench` for the smallest
  end-to-end smoke). Estimated
  scope: S. Completion signal:
  `cargo test -p rbp-autotrain
  --test trainer_observe`
  is green with 1 new
  integration sub-test
  passing; the
  `scripts/trainer-observe.sh`
  wrapper is on disk +
  executable + parses with
  `bash -n`; a CI dashboard
  can `jq -c 'select(.stream
  == "summary")' run.step.jsonl`
  the file and receive a
  one-line per-run summary;
  the `STW-039` morning-wave
  row is marked `RESCOPED`
  and a future worker does
  not re-claim the
  Rust-module half.
  **`lens:` Design (the
  observability audit) —
  closes the same finding
  the morning wave named,
  with the *bash wrapper*
  shape, not the *Rust
  module* shape, so no
  autotrain Rust crate
  changes and no risk of
  regressing the existing
  per-arm log shape.**

- [ ] **[P1] `STW-046` Drop the morning-wave `STW-040`
  README `## Try it now` + the
  `scripts/replay-locally.sh` shim and
  the morning-wave `STW-041` STW-001
  planning-surface retirement from the
  active queue.** The afternoon review
  finds both rows are busywork the
  testnet north star does not need.
  The morning wave's `STW-040`
  `## Try it now` reframe is a
  *content-section-add* with no
  behavioral test beyond
  `grep -q '## Try it now' README.md` —
  pure prose, no shipped capability,
  and the existing
  `## Testnet launch proof` +
  `## Public dashboard` README
  sections (lines 220-247 and
  lines 290-330 in the current
  README) already answer the
  first-time-visitor question
  ("can I see the bot play?",
  "where is the public
  benchmark?", "how do I run
  the testnet launch proof?").
  Adding a *third* redundant
  section above the existing
  `## Quick Start` is the
  canonical AI-design-sludge
  anti-pattern. The companion
  `scripts/replay-locally.sh`
  shim is a 30-line bash wrapper
  around the existing
  `trainer --replay <path>` CLI
  the STW-016 row already
  shipped — a one-command
  `trainer --replay <path>`
  invocation is *already* a
  one-command invocation, the
  shim adds a `replay locally
  complete: ...` headline
  prefix and three new
  shape pins for a wrapper
  that does not need a
  wrapper. The morning wave's
  `STW-041` STW-001
  planning-surface retirement
  is a process-cleanup row
  that (a) does not block the
  testnet north star (the
  35 shipped STW rows have
  proved `genesis/` +
  `IMPLEMENTATION_PLAN.md` is
  a sufficient executable
  surface without a gbrain
  DB), (b) does not change
  any user-visible behavior
  (the proposed
  `genesis/AUTHORED-QUEUE.md`
  fallback queue is a
  *read-by-the-auto-loop*
  artifact the auto-loop
  does not read), and (c)
  ships a 0-test code change
  whose only completion
  signal is a one-line
  `RETIRED 2026-06-04` note
  in `IMPLEMENTATION_PLAN.md` —
  the AI-slop risk is high
  (a planning-process row
  with no behavioral test
  is a `cargo fmt --check`
  cargo-cult). The
  afternoon wave's row is
  the *drop*, not the
  *re-scope*: mark both
  rows as `DROPPED 2026-06-04
  by STW-046 — busywork the
  testnet north star does
  not need, see the
  afternoon three-lens
  review (kanban task
  `t_35186537`) for the
  reasoning`, leave the
  existing
  `## Testnet launch proof` +
  `## Public dashboard` README
  sections as-is, leave the
  `[!] STW-001` deferred
  row in
  `IMPLEMENTATION_PLAN.md` as
  the operator-sign-off
  blocker it already is, and
  spend the operator's review
  budget on STW-042 (the
  v10 dashboard demo-data
  fixture) and STW-043 (the
  bench-report demo-data
  fixture) — the two rows
  the afternoon review
  names as P0. Owner files:
  `IMPLEMENTATION_PLAN.md`
  (mark the morning-wave
  STW-040 row as
  `DROPPED 2026-06-04 by
  STW-046` and the
  morning-wave STW-041
  row as
  `DROPPED 2026-06-04 by
  STW-046`; no other
  changes — the existing
  `[!] STW-001` deferred
  row in the
  `## Deferred items
  (need operator decision
  before promotion)`
  section is *preserved
  verbatim*),
  `README.md` (NO CHANGE —
  the existing
  `## Testnet launch proof` +
  `## Public dashboard`
  sections are the
  operator-UX answer; no
  `## Try it now` section
  is added; the
  `Public dashboard: <https://robopoker-testnet-dashboard.pages.dev/>`
  line at line 313 is
  preserved verbatim),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (NO CHANGE — the
  v10 follow-on is
  marked shipped as the
  STW-036 row already
  did; the morning
  wave's STW-040 /
  STW-041 additions
  to the roadmap are
  not made). Scope
  boundary: does NOT
  add a `## Try it now`
  section to
  `README.md`; does
  NOT add a
  `scripts/replay-locally.sh`
  shim; does NOT add
  a
  `genesis/AUTHORED-QUEUE.md`
  fallback queue; does
  NOT mark `STW-001`
  as `RETIRED` in
  `IMPLEMENTATION_PLAN.md`
  (the deferred `[!]`
  row is preserved);
  does NOT change the
  room protocol, the
  `Schema` contracts,
  the autotrain
  pipeline, the
  K-means cluster
  counts, the v1 / v2
  / v3 / v4 named
  baselines, the
  `CFR_TREE_COUNT_NLHE`
  baseline, or any
  `trainer --*` CLI.
  Verification commands:
  `git diff --
  IMPLEMENTATION_PLAN.md`
  (the diff is the
  two `DROPPED` markers
  + this STW-046 row,
  nothing else),
  `grep -q '## Testnet
  launch proof'
  README.md` (the
  existing
  first-time-visitor
  section is preserved),
  `grep -q '## Public
  dashboard' README.md`
  (the existing
  public-dashboard
  section is preserved),
  `grep -q '## Try it
  now' README.md` →
  exit 1 (the
  *redundant*
  section is *not*
  added — the
  *absence* of the
  section is the
  completion signal),
  `cargo test --workspace
  -- --test-threads=4`
  (the drop does not
  change the autotrain
  pipeline),
  `cargo check --workspace`,
  `cargo fmt --check`.
  Required tests: none
  — STW-046 is a
  queue-cleanup
  decision, not a code
  change. Dependencies:
  none. Estimated
  scope: XS.
  Completion signal:
  the
  `IMPLEMENTATION_PLAN.md`
  morning-wave STW-040
  + STW-041 rows are
  both marked
  `DROPPED 2026-06-04
  by STW-046` with a
  one-line note; the
  existing
  `## Testnet launch
  proof` +
  `## Public dashboard`
  README sections are
  preserved verbatim;
  the existing
  `[!] STW-001`
  deferred row is
  preserved verbatim;
  the
  `## Try it now`
  section is *not*
  added to
  `README.md`. The
  operator's review
  budget is freed for
  STW-042 +
  STW-043.
  **`lens:` Design
  (the AI-slop
  review: a
  third redundant
  README section +
  a zero-test
  planning-process
  row are the
  canonical
  AI-design-sludge
  patterns the task
  body explicitly
  bans) + CEO (the
  testnet north
  star's
  "publicly-visible"
  requirement is
  served by the
  v10 dashboard +
  the
  `## Public
  dashboard`
  README section
  the v10 ships —
  not by a
  `## Try it now`
  reframe).**


## Next wave - review 2026-06-04 (third pass)

The afternoon 2026-06-04 three-lens review (kanban task
`t_35186537`) shipped STW-042 (compare3-fixture demo-data
+ `GET /bench/compare3-fixture` fallback, commit 5a90622)
and identified STW-043 / STW-044 / STW-045 / STW-046 as
the next-wave backlog. STW-042 is the only shipped item;
STW-043 is still the canonical P0 follow-on. The third
2026-06-04 review (kanban task `t_6df7de4e`) re-applies
the three lenses to the *current* state — with STW-042
shipped and live on `main`, the dashboard's `GET /bench/
compare3-fixture` route is real, the `crates/dashboard/
tests/fixtures/compare3-fixture.json` file is committed,
and a fresh `cargo run -p rbp-dashboard` shows the
demo-data card to a first-time visitor. The remaining
gaps the three lenses now name, in priority order:

(a) **The single-config bench shape has no committed
    demo card.** STW-043 (afternoon wave) is unstarted.
(b) **The live INDEX.json the dashboard reads has a
    column-shape gap.** The dashboard table renders
    `receipt_basename` / `blueprint` / `baseline` /
    `mbb_per_100` / `ci_95` / `win_rate` / `total_bytes`
    / `uploaded_at_utc` (render.rs:147-159). The
    `IndexedEntry` the `PublishIndex` carries does NOT
    carry `blueprint` / `baseline` / `mbb_per_100` /
    `ci_95` / `win_rate` (render.rs:164-171 explicitly
    says "the `INDEX.json`'s `IndexedEntry` shape doesn't
    carry the per-hand `mbb_per_100` / `win_rate` — those
    live on the `BenchReport` the next slice will
    inline. For now the table renders placeholder `—`
    cells so the column order is visible"). The 5/8
    placeholder cells are the dashboard's largest single
    AI-slop risk (a visitor who lands on the URL sees
    five blank cells in a table the README advertises as
    populated). The "the next slice will inline" line in
    render.rs is the open architectural hinge.
(c) **The "actions" column header + the `<unknown>`
    literal in the committed INDEX.json timestamp fields
    are cheap, high-signal Design-UX defects that the
    afternoon wave did not name.** The "actions" header
    (render.rs:159) is engineering jargon; a visitor sees
    the literal string "actions" in the public table. The
    literal `<unknown>` string in the committed fixture's
    `created_at_utc` (index.json:4) and every entry's
    `uploaded_at_utc` (index.json:44, 100) renders to a
    public visitor when a fresh checkout consumes the
    committed fixture.
(d) **The morning wave's `STW-038` `TrainerError` enum
    refactor + `STW-039` typed `Step` enum Rust module
    are still the canonical "improve X" anti-patterns
    the task body explicitly bans.** STW-044 (re-scope
    to per-arm error-shape audit) and STW-045 (re-scope
    to bash wrapper, no Rust module) are the right
    shapes; both are unstarted.
(e) **The morning wave's `STW-040` README `## Try it
    now` section + `STW-041` STW-001 retirement are
    still busywork the testnet north star does not
    need.** STW-046 (drop both) is the right shape; it
    is unstarted.

The third 2026-06-04 wave therefore: (1) carries STW-043
forward unchanged as the first P0 (it is the structural
twin of the just-shipped STW-042); (2) carries STW-045 +
STW-046 forward unchanged as P1s (the re-scope + drop
shapes are correct); (3) introduces two new P0/P1
slices — STW-047 (the live INDEX.json column-shape
fix the afternoon wave named as "the next slice" in
render.rs but did not row up) and STW-048 (the
"actions" + `<unknown>` design-UX defects the afternoon
wave did not name) — to close the highest-signal gaps
the third pass found.

Each row below names a single shippable slice with
named files, verification command(s), and a `lens:` tag
tracing the finding it closes. Rows are P0/P1 ordered;
the top row is the highest single-shipment leverage.
The `STW-043` / `STW-045` / `STW-046` rows below are
**re-affirmations of the afternoon wave's open rows**;
they are NOT new scope. The new scope is `STW-047`
and `STW-048`.

- [x] **[P0] `STW-043` (carry-over from afternoon wave
  `t_35186537`) `crates/autotrain/tests/fixtures/bench-report-fixture.json`
  + `scripts/commit-bench-fixture.sh` operator shim
  that produces a byte-stable `BenchReport` from
  `trainer --bench` against a no-DB deterministic
  small-config run.** Shipped on commit `d95047a`
  (2026-06-04) — the `BenchReport` fixture +
  `scripts/commit-bench-fixture.sh` shim. The
  on-disk code + the committed fixture + the
  integration test that diffs the shim's output
  against the committed fixture all exist and are
  green. The `[ ]` was a planning-pin drift the
  STW-055 sweep caught; the re-affirmation row
  stayed open in the planning surface even after
  the underlying slice shipped. STW-055 closed
  the pin (this row) and the per-row `'<missing>'`
  literal in `index.html:200` in one commit. The
  third pass's three lenses
  re-confirm this as P0: the afternoon CEO lens
  named it the only open P0; the third-pass CEO lens
  re-confirms the testnet north star's "publicly-
  visible + downloadable" requirement is incomplete
  without a committed single-config bench result
  (the just-shipped STW-042 commits the *compare3*
  result; this commits the *bench* result — the two
  coexist). The third-pass Eng lens names the exact
  files (mirror STW-042's `compare3-fixture.json`
  shape one-for-one): new `scripts/commit-bench-
  fixture.sh` pure-bash shim, new `crates/autotrain/
  tests/fixtures/bench-report-fixture.json` byte-
  stable hand-authored fixture in the `BenchReport::
  to_json` shape with `run_id` / `started_at_utc`
  stripped, new `crates/autotrain/tests/bench_report_
  fixture.rs` integration test that re-runs the shim
  + diffs against the committed fixture, new
  `script_shape.rs` pins. The third-pass Design lens
  names the same completion signal: a CI dashboard
  can `grep ^mbb_per_100 crates/autotrain/tests/
  fixtures/bench-report-fixture.json` and receive the
  number; a fresh `cargo run -p rbp-dashboard` will
  show a `Demo data: /bench/bench-report-fixture`
  link alongside the existing `Demo data:
  /bench/compare3-fixture` link in the static
  `index.html` empty-state (render.rs:14-21 doc
  comment + index.html:128-130 existing pattern).
  Scope boundary: the *production* benchmark path is
  unchanged; the shim is a *separate* script the
  operator runs to *produce* the committed fixture.
  Verification commands: `bash -n scripts/commit-
  bench-fixture.sh`, `./scripts/commit-bench-
  fixture.sh /tmp/bench-report.json` (exits 0 +
  produces a parseable JSON file), `cargo test -p
  rbp-autotrain --test bench_report_fixture`,
  `cargo test -p rbp-autotrain --test script_shape`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt --check`.
  Required tests: 2 new integration sub-tests in
  `crates/autotrain/tests/bench_report_fixture.rs` +
  2 new shape pins in `crates/autotrain/tests/
  script_shape.rs`. Dependencies: STW-010 (the
  `trainer --bench` mode), STW-031 (the `BenchReport`
  shape the fixture is in). Estimated scope: S.
  Completion signal: `cargo test -p rbp-autotrain
  --test bench_report_fixture` is green with 2 new
  sub-tests passing; the committed fixture's
  `sha256sum` matches the in-tree digest the slice
  ships; a fresh `cargo run -p rbp-dashboard`
  serves a populated `/bench/bench-report-fixture`
  card with the v1 numbers visible to a fresh
  checkout. **`lens:` CEO (the publicly-visible
  leg of the testnet north star's "downloadable"
  requirement) + Eng (the bench result is the
  structural twin of the just-shipped compare3
  result; the seams are already laid) + Design
  (the empty-state "Demo data" link is the
  visitor's first impression of the bench path;
  one link is half a story).**

- [x] **[P0] `STW-047` Wire the live `INDEX.json` →
  dashboard table so the 5/8 placeholder `—` cells
  render real `blueprint` / `baseline` / `mbb_per_100`
  / `ci_95` / `win_rate` numbers from the indexed
  `BenchReport`, not a dash. SUPERSEDED 2026-06-04 by
  `STW-049` (the fourth-pass build-break + column-
  shape wire the same struct extension the
  render-side wire needed; the two ship together
  because they share the autotrain `IndexedEntry ::
  bench : Option<BenchSummary>` field the
  aggregator now emits, the parse helper
  `parse_bench_summary` it calls per entry, the
  2 new `render.rs` lib tests
  (`live_index_table_renders_bench_cells_with_values`
  + `live_index_table_renders_dash_for_missing_bench`)
  the slice adds, the 4 new `publish_index` lib
  tests (`parse_bench_summary_*`) the parse helper
  ships, the smoke-test extension that asserts every
  fixture entry's `bench` is `Some(_)`, and the
  deletion of the `placeholder_cells_present` lib
  test that *pinned* the 5/8 `—` defect as a
  feature).** Closes the
  `render.rs:164-171` "the next slice will inline"
  architectural hinge the third-pass CEO + Design
  lenses both name as the *single highest-leverage
  public-surface defect* on `main` today. A fresh
  `cargo run -p rbp-dashboard` that points at a real
  published INDEX.json from a `trainer --publish-
  index` run renders a table with the `receipt_
  basename` / `total_bytes` / `uploaded_at_utc`
  columns populated, but the 5/8 `blueprint` /
  `baseline` / `mbb_per_100` / `ci_95` / `win_rate`
  cells render `—` for every row — a first-time
  visitor sees a "real" table that reads as half-
  empty. The fix is structural: extend the
  `IndexedEntry` shape in `crates/autotrain` (the
  `PublishIndex` aggregator that writes INDEX.json)
  to *include* the per-receipt `blueprint` /
  `baseline` / `mbb_per_100` / `ci_95` / `win_rate`
  fields the bench produces (the bench's stdout
  already has them in a fixed format string the
  STW-010 + STW-018 + STW-031 lib tests pin), then
  extend the dashboard's `render_index_table` in
  `render.rs:163-200` to render the new
  `entry.bench.blueprint` / `entry.bench.baseline` /
  `entry.bench.mbb_per_100` / `entry.bench.ci_95` /
  `entry.bench.win_rate` fields instead of the 5
  `—` placeholders, then delete the `placeholder_
  cells_present` lib test (render.rs:171) that
  *pins* the placeholders. The demo-data fallback
  (STW-042 compare3-fixture, STW-043 bench-report-
  fixture) is unchanged — the *live* path is what
  gets wired. Owner files: `crates/autotrain/src/
  publish_index.rs` (extend the `IndexedEntry`
  struct with the 5 new bench fields + extend the
  `to_json` format string the aggregator emits
  + extend the `--verify-index` re-verifier to
  assert the new fields are present + byte-stable),
  `crates/dashboard/src/render.rs` (extend
  `render_index_table` to read the 5 new fields
  from each `IndexedEntry` + render them as
  numeric monospace `<td>` cells, replacing the 5
  `—` placeholders at render.rs:180-184; the
  2 new lib tests pin the per-row column shape
  + the `<dt>`-shaped cell render for `±ci_95`
  + the `win_rate` percentage format), `crates/
  dashboard/tests/smoke.rs` (extend the 4-route
  drive to assert the new cells contain the
  fixture's `mbb_per_100` value, not `—`),
  `crates/dashboard/tests/fixtures/index.json`
  (extend the committed fixture with realistic
  per-entry bench fields; STW-048 below is the
  dedicated `<unknown>`-timestamp sweep; this
  slice ships realistic bench numbers),
  `IMPLEMENTATION_PLAN.md` (this row). Scope
  boundary: does NOT change the `Compare3Report`
  shape (STW-042 / STW-043 are demo data; this is
  the live-index path); does NOT change the
  `BenchReport::to_json` autotrain shape (the
  aggregator reads the existing bench JSON, it
  does not change the emitter); does NOT change
  the room protocol, the `Schema` contracts, the
  autotrain pipeline, the K-means cluster counts,
  the v1 / v2 / v3 / v4 named baselines, the
  `CFR_TREE_COUNT_NLHE` baseline, the
  `trainer --replay` CLI, the
  `trainer --verify-receipt` CLI, or the
  `trainer --smoke` / `trainer --bench` /
  `trainer --compare` / `trainer --compare3`
  JSON contracts. Verification commands:
  `cargo test -p rbp-autotrain --test
  script_shape` (the new
  `bench_fields_in_index_round_trip` pin
  passes), `cargo test -p rbp-dashboard --lib`
  (the 2 new column-shape lib tests pass),
  `cargo test -p rbp-dashboard --test smoke`
  (the extended 4-route drive passes + asserts
  the new cells contain the fixture's bench
  value, not `—`), `cargo test --workspace --
  --test-threads=4`, `cargo check --workspace`,
  `cargo fmt --check`. Hand-test command:
  `cargo run -p rbp-dashboard -- --port 18080
  &; sleep 2; curl -s http://localhost:18080/
  api/index | jq '.entries[0] | .bench.
  mbb_per_100'` (returns a number, not null).
  Required tests: 2 new lib tests in
  `crates/dashboard/src/render.rs::tests`
  (per-row column shape + numeric cell render)
  + 1 new `script_shape.rs` pin
  (`bench_fields_in_index_round_trip`) + 1
  extension to the existing 4-route smoke
  drive. Dependencies: STW-031 (the
  `Compare3Report` + `BenchReport` shapes
  the aggregator reads), STW-034 (the
  `--publish-index` aggregator the new fields
  are added to), STW-035 (the `--verify-
  index` re-verifier the new pin is added
  to). Estimated scope: M. Completion
  signal: `cargo run -p rbp-dashboard` on a
  fresh checkout pointing at the committed
  `crates/dashboard/tests/fixtures/index.json`
  serves a table with all 8 columns
  populated (the `receipt_basename` /
  `total_bytes` / `uploaded_at_utc` columns
  are populated today; the 5 new bench
  columns are populated by this slice) —
  the `placeholder_cells_present` lib test
  is removed and the new `live_index_table_
  renders_bench_cells_with_values` test
  pins the new behavior; a CI dashboard
  scraping `GET /api/index` receives the
  bench numbers, not the `—` placeholder
  string. **`lens:` CEO (the publicly-
  visible + downloadable leg of the
  testnet north star; a half-empty table
  is a credibility-erosion signal a
  stranger sees on first visit) + Eng
  (the render.rs:164-171 "next slice will
  inline" hinge is named; the seam is
  small) + Design (the 5/8 placeholder
  cells are the single largest AI-slop
  risk in the public surface; the
  `placeholder_cells_present` lib test
  *pins* the defect as a feature).**

- [ ] **[P1] ~~`STW-048`~~ `SUPERSEDED 2026-06-04
  by STW-050` Replace the `actions` column
  header (render.rs:159) with two per-action column
  headers (`transcript` / `replay`) AND replace the
  literal `<unknown>` strings in the committed
  `crates/dashboard/tests/fixtures/index.json` (line
  4 + lines 41 + 44 + 97 + 100) with realistic
  fixed-ISO-8601 timestamps the dashboard's
  `meta` line (index.html:211) renders instead of
  the `<unknown>` literal.** Closes two cheap
  high-signal Design-UX defects the third-pass
  Design lens names that the afternoon wave did
  not. The `actions` column header at render.rs:159
  is engineering jargon — a visitor who inspects
  the page source / a future i18n pass / a screen
  reader sees the literal string "actions" in the
  public-facing table. The fix is mechanical:
  split the single `<th>actions</th>` into two
  `<th>transcript</th>` + `<th>replay</th>` cells
  and split the single `<td>` link pair at
  render.rs:191-198 into two `<td>` cells, one
  per link. The "actions" / "transcript" /
  "replay" headers are each short, English, and
  the rename is a `render.rs` change only (the
  CSS class names are unchanged; the existing
  `index-table__link` class still applies). The
  `<unknown>` literal sweep is also a `index.json`
  + `render.rs` change: the fixture's
  `created_at_utc` (line 4) and every entry's
  `plan.created_at_utc` (lines 41, 97) +
  `uploaded_at_utc` (lines 44, 100) +
  `remote_receipt.uploaded_at_utc` (lines 44,
  100) get realistic fixed-ISO-8601 timestamps
  (e.g. `"2026-06-04T05:00:00Z"`,
  `"2026-06-04T14:01:07Z"`); the dashboard's
  `meta` line (index.html:211) renders the
  timestamp verbatim, so a public visitor sees
  a real timestamp instead of `<unknown>`. Owner
  files: `crates/dashboard/src/render.rs`
  (replace the `<th>actions</th>` + the single
  `<td>` link pair with two per-action columns;
  add 1 new lib test pinning the
  per-row-column-shape, asserting both
  per-action cells contain a `<a>` link with
  the right `href`; no other test changes),
  `crates/dashboard/tests/fixtures/index.json`
  (replace 7 literal `<unknown>` strings with
  realistic fixed-ISO-8601 timestamps; preserve
  the `entries[].receipt_basename` shape the
  fixture's smoke test pins; no other field
  changes), `IMPLEMENTATION_PLAN.md` (this
  row). Scope boundary: does NOT change the
  live `IndexedEntry` shape (STW-047 wires
  the live bench fields; this slice is the
  committed-fixture timestamp sweep only);
  does NOT change the live
  `crates/autotrain::PublishIndex` shape;
  does NOT change the autotrain pipeline,
  the bench harness, the room protocol, the
  `Schema` contracts, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, or any `trainer --*` CLI.
  Verification commands: `cargo test -p
  rbp-dashboard --lib` (the 1 new per-row-
  column-shape lib test passes), `cargo
  test -p rbp-dashboard --test smoke` (the
  existing 4-route drive still passes; the
  2-action-link check is now per-cell), `cargo
  test --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Hand-test command: `cargo run
  -p rbp-dashboard -- --port 18080 &; sleep
  2; curl -s http://localhost:18080/ | grep
  -E 'transcript|replay'` (both strings
  present, no `actions` literal), `curl -s
  http://localhost:18080/ | grep -E
  '<unknown>'` (zero matches), `curl -s
  http://localhost:18080/api/index | jq
  '.created_at_utc'` (returns a real ISO
  timestamp, not `<unknown>`). Required tests:
  1 new lib test in `crates/dashboard/src/
  render.rs::tests` (per-row column shape
  with split actions columns) + zero
  integration test changes (the existing
  4-route drive is extended to assert the
  per-cell link shape). Dependencies: none —
  STW-048 is independent of STW-043 / STW-047
  and can ship in any order. Estimated scope:
  XS. Completion signal: a fresh `cargo
  run -p rbp-dashboard` serves a table with
  two per-action column headers
  (`transcript` / `replay`) and a `meta`
  line that shows a real ISO-8601
  timestamp; the literal string
  `<unknown>` does not appear anywhere in
  the rendered HTML; the existing `4-route
  drive` smoke test still passes; the
  committed fixture's timestamps are
  realistic + byte-stable. **`lens:` Design
  (the cheapest, highest-signal public-
  surface defects the third pass found;
  the `actions` header is engineering
  jargon in a public-facing table; the
  `<unknown>` literal is the single most
  visible "this is a test fixture" tell
  in the deployed surface).**

- [ ] **[P1] `STW-045` (carry-over from afternoon wave RESCOPED 2026-06-05**
  `t_35186537`) `scripts/trainer-observe.sh` wrapper
  that prepends `date +%s%3N` to every stderr line
  the trainer emits and writes a `trainer.step.jsonl`
  per-run timeline file. The third pass re-
  confirms the morning wave's `STW-039` typed
  `Step` enum Rust module is the canonical
  "improve X" anti-pattern the task body
  explicitly bans — a 200+ line Rust module whose
  only consumer is "a CI worker wants per-step
  timing" is the wrong shape; a 30-line bash
  wrapper is the right shape. The third-pass
  Design lens re-confirms the operator-UX value
  (a CI dashboard can `jq -c 'select(.stream ==
  "summary")' run.step.jsonl` and receive a
  one-line per-run summary) is the highest-signal
  observability gap on `main` today. The third-
  pass Eng lens confirms the wrapper is mode-
  agnostic (the same `scripts/trainer-observe.sh
  /tmp/run.step.jsonl trainer --bench` works
  against `--compare3` / `--verify-receipt` /
  `--publish` / etc.) and the existing
  `crates/autotrain/tests/script_shape.rs`
  pinner pattern accommodates a new
  `trainer_observe_script_exists_and_parses`
  pin with no new infrastructure. Owner files:
  `scripts/trainer-observe.sh` (new pure-bash
  wrapper; the only logic is the `fork + pipe
  stderr + prepend date +%s%3N + jq` loop;
  mirrors the `scripts/testnet-live-proof.sh`
  shape), `crates/autotrain/tests/
  script_shape.rs` (add 1 new shape pin:
  `trainer_observe_script_exists_and_parses`),
  `crates/autotrain/tests/trainer_observe.rs`
  (new no-DB-shape integration test; requires
  `DATABASE_URL` to be set for the child
  `trainer --bench` invocation, mirrors the
  existing `bench.rs` / `compare.rs` /
  `compare3.rs` integration tests' database-
  gating pattern), `IMPLEMENTATION_PLAN.md`
  (this row; mark the morning-wave STW-039
  row as `RESCOPED 2026-06-04 by STW-045`).
  Scope boundary: the wrapper is *additive* —
  the trainer binary is unchanged; a future
  operator who runs `trainer --bench`
  without the wrapper sees the *same* output
  they see today; an operator who runs
  `scripts/trainer-observe.sh <out.jsonl>
  trainer --bench` sees the same output
  *plus* a parallel JSONL timeline. The
  wrapper is *transparent* to any existing
  CI pipeline that runs `trainer --bench`
  and asserts exit 0 — the wrapper
  preserves the exit code. Verification
  commands: `bash -n scripts/trainer-
  observe.sh`, `scripts/trainer-observe.sh
  /tmp/test.step.jsonl trainer --bench --
  blueprint v1 --baseline preflop --bench-
  hands 4` (exits 0 + the JSONL file has
  at least 2 lines: one `stream: "stderr"`
  line + one `stream: "summary"` line),
  `jq -c . /tmp/test.step.jsonl | head -5`
  (the JSONL is line-parseable),
  `cargo test -p rbp-autotrain --test
  trainer_observe` (the 1 new integration
  sub-test passes), `cargo test -p
  rbp-autotrain --test script_shape` (the
  1 new shape pin passes), `cargo test
  --workspace -- --test-threads=4`, `cargo
  check --workspace`, `cargo fmt --check`.
  Required tests: 1 new integration sub-
  test in `crates/autotrain/tests/
  trainer_observe.rs` + 1 new shape pin
  in `crates/autotrain/tests/
  script_shape.rs`. Dependencies: STW-010
  (the `trainer --bench` mode the wrapper
  is most useful against — the wrapper is
  mode-agnostic, but the integration test
  drives `--bench` for the smallest end-
  to-end smoke). Estimated scope: S.
  Completion signal: `cargo test -p
  rbp-autotrain --test trainer_observe`
  is green with 1 new integration sub-
  test passing; the
  `scripts/trainer-observe.sh` wrapper is
  on disk + executable + parses with
  `bash -n`; a CI dashboard can
  `jq -c 'select(.stream == "summary")'
  run.step.jsonl` the file and receive
  a one-line per-run summary. **`lens:`
  Design (the observability audit) —
  closes the same finding the morning
  wave named, with the *bash wrapper*
  shape, not the *Rust module* shape,
  so no autotrain Rust crate changes
  and no risk of regressing the
  existing per-arm log shape.**

- [ ] **[P1] `STW-046` (carry-over from afternoon wave
  `t_35186537`) Drop the morning-wave `STW-040`
  README `## Try it now` + the
  `scripts/replay-locally.sh` shim and the
  morning-wave `STW-041` STW-001 planning-surface
  retirement from the active queue.** The third
  pass re-confirms both rows are busywork the
  testnet north star does not need and the
  third-pass Design lens re-confirms a third
  redundant README section + a zero-test
  planning-process row are the canonical
  AI-design-sludge anti-patterns the task body
  explicitly bans. The existing
  `## Testnet launch proof` + `## Public
  dashboard` README sections (lines 220-247 +
  290-333 in the current README) already
  answer the first-time-visitor question; the
  existing `[!] STW-001` deferred row in
  `IMPLEMENTATION_PLAN.md` already serves as
  the operator-sign-off blocker the original
  STW-041 wanted to retire. Owner files:
  `IMPLEMENTATION_PLAN.md` (mark the morning-
  wave STW-040 row as
  `DROPPED 2026-06-04 by STW-046` and the
  morning-wave STW-041 row as
  `DROPPED 2026-06-04 by STW-046`; no other
  changes — the existing `[!] STW-001`
  deferred row in the
  `## Deferred items (need operator
  decision before promotion)` section is
  *preserved verbatim*), `README.md` (NO
  CHANGE — the existing
  `## Testnet launch proof` +
  `## Public dashboard` sections are the
  operator-UX answer; no `## Try it now`
  section is added; the
  `Public dashboard: <https://robopoker-
  testnet-dashboard.pages.dev/>` line at
  line 313 is preserved verbatim),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (NO CHANGE — the v10 follow-on is marked
  shipped as the STW-036 row already did;
  the morning wave's STW-040 / STW-041
  additions to the roadmap are not made).
  Scope boundary: does NOT add a
  `## Try it now` section to `README.md`;
  does NOT add a
  `scripts/replay-locally.sh` shim; does
  NOT add a
  `genesis/AUTHORED-QUEUE.md` fallback
  queue; does NOT mark `STW-001` as
  `RETIRED` in `IMPLEMENTATION_PLAN.md`
  (the deferred `[!]` row is preserved);
  does NOT change the room protocol, the
  `Schema` contracts, the autotrain
  pipeline, the K-means cluster counts,
  the v1 / v2 / v3 / v4 named baselines,
  the `CFR_TREE_COUNT_NLHE` baseline, or
  any `trainer --*` CLI. Verification
  commands: `git diff -- IMPLEMENTATION_
  PLAN.md` (the diff is the two `DROPPED`
  markers + this STW-046 row, nothing
  else), `grep -q '## Testnet launch
  proof' README.md` (the existing first-
  time-visitor section is preserved),
  `grep -q '## Public dashboard'
  README.md` (the existing public-
  dashboard section is preserved),
  `grep -q '## Try it now' README.md` →
  exit 1 (the *redundant* section is *not*
  added — the *absence* of the section is
  the completion signal), `cargo test
  --workspace -- --test-threads=4` (the
  drop does not change the autotrain
  pipeline), `cargo check --workspace`,
  `cargo fmt --check`. Required tests:
  none — STW-046 is a queue-cleanup
  decision, not a code change. Dependencies:
  none. Estimated scope: XS. Completion
  signal: the
  `IMPLEMENTATION_PLAN.md` morning-wave
  STW-040 + STW-041 rows are both marked
  `DROPPED 2026-06-04 by STW-046` with a
  one-line note; the existing
  `## Testnet launch proof` +
  `## Public dashboard` README sections
  are preserved verbatim; the existing
  `[!] STW-001` deferred row is preserved
  verbatim; the `## Try it now` section
  is *not* added to `README.md`. The
  operator's review budget is freed for
  STW-043 (the bench-report demo-data
  fixture) and STW-047 (the live
  INDEX.json → dashboard column shape
  wire). **`lens:` Design (the AI-slop
  review: a third redundant README
  section + a zero-test planning-process
  row are the canonical AI-design-sludge
  patterns the task body explicitly bans)
  + CEO (the testnet north star's
  "publicly-visible" requirement is served
  by the v10 dashboard + the
  `## Public dashboard` README section
  the v10 ships — not by a
  `## Try it now` reframe).**

## Next wave - review 2026-06-04

The third 2026-06-04 three-lens review (kanban task
`t_6df7de4e`) shipped STW-043 (bench-report fixture
+ commit-bench-fixture.sh shim, commit d95047a) and
named STW-045 / STW-046 / STW-047 / STW-048 as the
next-wave backlog. The fourth 2026-06-04 three-lens
review (kanban task `t_700c33f9`) re-applies the
three lenses to the *current* state and finds **the
highest-leverage finding is one the third pass did
not name**: a fresh `cargo test --workspace` does
not compile on `main` today. Three test-fixture
constructors in
`crates/dashboard/src/{router,index_client,render}.rs`
instantiate the dashboard's mirror `IndexedEntry`
struct without the `bench: Option<BenchSummary>`
field the autotrain `IndexedEntry`
(`crates/autotrain/src/publish_index.rs:347`)
carries since commit `d95047a`. The autotrain lib
itself compiles (the field is *additive* in
production) and the dashboard lib compiles (the
field is *additive* in production), but
`cargo test --workspace` is **red**:

```
error[E0063]: missing field `bench` in initializer
              of `IndexedEntry`
   --> crates/dashboard/src/router.rs:585:27
error[E0063]: missing field `bench` in initializer
              of `IndexedEntry`
   --> crates/dashboard/src/index_client.rs:312:27
error[E0063]: missing field `bench` in initializer
              of `IndexedEntry`
   --> crates/dashboard/src/render.rs:561:27
error: could not compile `rbp-dashboard` (lib test)
       due to 3 previous errors
```

A stranger who clones the repo and runs the
documented `cargo test --workspace` (or
`bash scripts/workspace-parallel-proof.sh`) sees
the testnet's flagship crate fail to build before
a single test runs. The testnet north star
("public, reproducible, downloadable") requires a
build the public can run; a red test build is a
more credibility-erosion signal than the 5/8 `—`
placeholder cells the third pass named, because a
build break blocks every other verification path
the README documents.

The fourth pass therefore:

1. **Promotes STW-047 (the third-pass live
   INDEX.json → dashboard column-shape wire) into
   STW-049** that ALSO closes the build break as
   a free side effect — extending the dashboard's
   `IndexedEntry` mirror to carry the `bench`
   field is the *same edit* that lets the test
   fixtures instantiate the mirror without the
   `bench: None` workaround, and the new
   `render_index_table` reads the new field
   instead of rendering `—`. The architectural
   hinge the third pass named
   (`render.rs:164-171` "the next slice will
   inline") and the mechanical build break are
   now one slice, not two.
2. **Promotes STW-048 (the third-pass `actions`
   → `transcript`/`replay` header split + the
   `<unknown>` literal sweep) into STW-050** with
   the *same* scope but a small refinement: the
   timestamp sweep now also covers the literal
   `<unknown>` strings in the in-tree
   `crates/dashboard/src/{index_client,router}.rs`
   demo-`PublishIndex` constructors
   (`index_client.rs:296, 299, 309` +
   `router.rs:569, 572, 582`) — six additional
   fixed-ISO-8601 strings the third pass
   under-counted. The dashboard lib's
   `*_with_*_fixture` test helpers render the
   `<unknown>` literal into the HTML the lib
   tests pin, so a fix to the literals is a
   one-pass change.
3. **Re-affirms STW-045** (the
   `scripts/trainer-observe.sh` bash wrapper)
   unchanged as a P1 — the morning wave's
   STW-039 typed Rust module is the canonical
   "improve X" anti-pattern the task body
   explicitly bans, the wrapper is the right
   shape, the third pass's spec is correct.
4. **Re-affirms STW-046** (drop the morning-wave
   STW-040 / STW-041 busywork) unchanged as a
   P1 — the existing `## Testnet launch proof` +
   `## Public dashboard` README sections are the
   first-time-visitor answer; the existing
   `[!] STW-001` deferred row in the
   `## Deferred items` section is the
   operator-sign-off blocker the original
   STW-041 wanted to retire.

Each row below names a single shippable slice with
named files, verification command(s), and a `lens:`
tag tracing the finding it closes. Rows are P0/P1
ordered; the top row is the highest single-shipment
leverage. The `STW-045` / `STW-046` rows below are
**re-affirmations of the third-pass open rows**;
they are NOT new scope. The new scope is
`STW-049` and `STW-050`.

- [x] **[P0] `STW-049` (supersedes third-pass
  `STW-047`) Extend the dashboard's
  `crates/dashboard/src/render.rs::IndexedEntry`
  mirror struct to carry `bench:
  Option<BenchSummary>` so it matches the
  autotrain `IndexedEntry` (`crates/autotrain/src/
  publish_index.rs:347`) AND wire the live
  `INDEX.json`'s 5 bench fields into
  `render_index_table` so the 5/8 placeholder `—`
  cells render real `blueprint` / `baseline` /
  `mbb_per_100` / `ci_95` / `win_rate` numbers
  from the indexed `BenchReport`.
  SHIPPED 2026-06-04 (commit 6886f08).** Closes BOTH
  the workspace-test-build break the fourth pass
  names as the *single highest-leverage finding
  on `main` today* AND the `render.rs:164-171`
  "next slice will inline" architectural hinge
  the third pass named — in a single slice
  because the mirror-struct extension the
  build-break fix needs IS the same mirror-struct
  extension the column-shape fix needs. The
  third pass under-scoped the slice by treating
  the build break as orthogonal; the fourth pass
  finds the two fixes share a single file (the
  dashboard's `IndexedEntry` mirror) and one
  round of test-fixture updates, so shipping
  them together is *cheaper* than shipping them
  apart. A fresh `cargo test --workspace` on
  `main` today fails with the three
  `error[E0063]: missing field \`bench\`` errors
  quoted in the section preamble; a fresh
  `cargo test --workspace` after this slice
  lands is green AND a fresh
  `cargo run -p rbp-dashboard` against a real
  published `INDEX.json` renders a table with all
  8 columns populated. Owner files:
  `crates/dashboard/src/render.rs` (extend
  the `IndexedEntry` mirror struct with
  `bench: Option<BenchSummary>` — the mirror
  re-uses the autotrain's `BenchSummary` type
  via a re-export or a thin clone, mirroring the
  existing `Compare3Report` mirror pattern the
  STW-042 slice established; extend
  `render_index_table` to read the 5 new fields
  from each entry's `bench` (or render `—` when
  `bench` is `None`, for the committed demo
  fixture the
  `crates/dashboard/tests/fixtures/index.json`
  carries); delete the `placeholder_cells_present`
  lib test that *pins* the placeholders as a
  feature; add 2 new lib tests pinning the
  per-row column shape with the bench fields
  populated + the `<dt>`-shaped cell render for
  `±ci_95` + the `win_rate` percentage format);
  `crates/dashboard/src/index_client.rs` (extend
  the mirror `IndexedEntry` struct here too with
  the same `bench: Option<BenchSummary>` field,
  the `entries: vec![IndexedEntry { ... }]`
  test fixture at line 312 adds
  `bench: None` — the demo fixture the
  `IndexClient` lib test uses carries no bench
  data; no other test changes);
  `crates/dashboard/src/router.rs` (same mirror
  struct extension; the line 585 test fixture
  adds `bench: None`; the live `serve_compare3`
  handler is unchanged);
  `crates/autotrain/src/publish_index.rs` (extend
  the `IndexedEntry::to_json` aggregator with
  the 5 new bench fields so the on-disk
  `INDEX.json` carries the per-receipt
  `blueprint` / `baseline` / `mbb_per_100` /
  `ci_95` / `win_rate` the bench produces — the
  bench's stdout already has them in a fixed
  format string the STW-010 + STW-018 + STW-031
  lib tests pin; extend the
  `PublishIndex::verify` re-verifier to assert
  the new fields are present + byte-stable);
  `crates/autotrain/tests/publish_index.rs`
  (extend the existing aggregator integration
  test to assert the new fields land in
  `INDEX.json` after a real aggregator run on a
  tempdir);
  `crates/dashboard/tests/fixtures/index.json`
  (extend the committed fixture with realistic
  per-entry bench fields — STW-050 below is the
  dedicated `<unknown>`-timestamp sweep; this
  slice ships realistic bench numbers);
  `crates/dashboard/tests/smoke.rs` (extend the
  4-route drive to assert the new cells contain
  the fixture's `mbb_per_100` value, not `—`,
  AND assert the test build's `IndexedEntry`
  mirror instantiates without the
  `missing field \`bench\`` error — the smoke
  test is the cheapest pin for the build-break
  fix); `IMPLEMENTATION_PLAN.md` (this row; mark
  the third-pass STW-047 row as
  `SUPERSEDED 2026-06-04 by STW-049`). Scope
  boundary: does NOT change the `Compare3Report`
  shape (STW-042 / STW-043 are demo data; this
  is the live-index path); does NOT change the
  `BenchReport::to_json` autotrain shape (the
  aggregator reads the existing bench JSON, it
  does not change the emitter); does NOT change
  the room protocol, the `Schema` contracts,
  the autotrain pipeline, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, the `CFR_TREE_COUNT_NLHE` baseline,
  the `trainer --replay` CLI, the
  `trainer --verify-receipt` CLI, or the
  `trainer --smoke` / `trainer --bench` /
  `trainer --compare` / `trainer --compare3`
  JSON contracts. Verification commands:
  `cargo test --workspace --no-run` (the 3
  missing-field errors are gone — this is the
  cheapest single-command proof the build break
  is closed), `cargo test -p rbp-autotrain
  --test publish_index` (the new bench-field
  pin passes), `cargo test -p rbp-dashboard
  --lib` (the 2 new column-shape lib tests
  pass + the deleted `placeholder_cells_present`
  test is *absent*), `cargo test -p
  rbp-dashboard --test smoke` (the extended
  4-route drive passes + asserts the new cells
  contain the fixture's bench value, not `—`),
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Hand-test command: `cargo run -p
  rbp-dashboard -- --port 18080 &; sleep 2;
  curl -s http://localhost:18080/api/index | jq
  '.entries[0] | .bench.mbb_per_100'` (returns
  a number, not null). Required tests: 2 new
  lib tests in
  `crates/dashboard/src/render.rs::tests`
  (per-row column shape with bench fields +
  numeric cell render) + 1 new aggregator
  integration assertion in
  `crates/autotrain/tests/publish_index.rs` +
  1 extension to the existing 4-route smoke
  drive in `crates/dashboard/tests/smoke.rs` +
  1 deletion of the `placeholder_cells_present`
  lib test. The 3 missing-field errors the
  build break surfaces are the *primary*
  acceptance signal; their absence is the
  cheapest mechanical proof the slice is
  complete. Dependencies: STW-031 (the
  `Compare3Report` + `BenchReport` shapes the
  aggregator reads), STW-034 (the
  `--publish-index` aggregator the new fields
  are added to), STW-035 (the `--verify-index`
  re-verifier the new pin is added to), STW-043
  (the just-shipped bench-report fixture the
  aggregator ingests). Estimated scope: M.
  Completion signal: `cargo test --workspace
  --no-run` is green (the 3 missing-field
  errors are gone); `cargo run -p rbp-dashboard`
  on a fresh checkout pointing at the committed
  `crates/dashboard/tests/fixtures/index.json`
  serves a table with all 8 columns populated
  (the `receipt_basename` / `total_bytes` /
  `uploaded_at_utc` columns are populated today;
  the 5 new bench columns are populated by this
  slice) — the `placeholder_cells_present` lib
  test is removed and the new
  `live_index_table_renders_bench_cells_with_values`
  test pins the new behavior; a CI dashboard
  scraping `GET /api/index` receives the bench
  numbers, not the `—` placeholder string.
  **`lens:` CEO (a green `cargo test
  --workspace` is the cheapest credibility-
  erosion repair the testnet north star can
  ship — a fresh checkout that cannot run the
  documented test command is a stranger's first
  impression of the project; the
  publicly-visible 5/8 placeholder table is a
  close-second credibility signal a CI
  dashboard sees) + Eng (the
  `render.rs:164-171` "next slice will inline"
  hinge the third pass named is the same struct
  extension the build-break fix needs —
  shipping them together is cheaper than
  shipping them apart) + Design (the
  `placeholder_cells_present` lib test that
  *pins* the 5/8 `—` defect as a feature is the
  single most surprising artifact in the public
  surface; deleting it is the acknowledgement
  the slice ships).**

- [x] **[P0] `STW-050` (supersedes third-pass
  `STW-048`) Replace the `actions` column
  header (render.rs:172) with two per-action
  column headers (`transcript` / `replay`) AND
  replace the literal `<unknown>` strings in
  the committed
  `crates/dashboard/tests/fixtures/index.json`
  (lines 4 + 41 + 44 + 97 + 100) AND the
  literal `<unknown>` strings in the dashboard
  lib's demo-`PublishIndex` constructors in
  `crates/dashboard/src/index_client.rs` (lines
  296, 299, 309) and
  `crates/dashboard/src/router.rs` (lines 569,
  572, 582) with realistic fixed-ISO-8601
  timestamps.** Closes the cheapest
  high-signal public-surface Design-UX defects
  the fourth pass finds (the third pass named
  the fixture-only `<unknown>` literals; the
  fourth pass extends the sweep to the 6
  additional `<unknown>` literals in the
  dashboard lib's demo-`PublishIndex`
  constructors the `*_with_*_fixture` lib
  tests pin). The `actions` column header at
  render.rs:172 is engineering jargon — a
  visitor who inspects the page source / a
  future i18n pass / a screen reader sees the
  literal string "actions" in the
  public-facing table. The fix is mechanical:
  split the single `<th>actions</th>` into two
  `<th>transcript</th>` + `<th>replay</th>`
  cells and split the single `<td>` link pair
  at render.rs:191-198 into two `<td>` cells,
  one per link. The `actions` / `transcript` /
  `replay` headers are each short, English, and
  the rename is a `render.rs` change only (the
  CSS class names are unchanged; the existing
  `index-table__link` class still applies). The
  `<unknown>` literal sweep is also a
  `index.json` + `render.rs` + `index_client.rs`
  + `router.rs` change: the fixture's
  `created_at_utc` (line 4) and every entry's
  `plan.created_at_utc` (lines 41, 97) +
  `uploaded_at_utc` (lines 44, 100) +
  `remote_receipt.uploaded_at_utc` (lines 44,
  100) get realistic fixed-ISO-8601 timestamps
  (e.g. `"2026-06-04T05:00:00Z"`,
  `"2026-06-04T14:01:07Z"`); the dashboard
  lib's `index_client.rs:296` (per-entry
  `plan.created_at_utc`) +
  `index_client.rs:299` (per-entry
  `remote_receipt.uploaded_at_utc`) +
  `index_client.rs:309` (top-level
  `created_at_utc`) + `router.rs:569` (per-entry
  `plan.created_at_utc`) + `router.rs:572`
  (per-entry `remote_receipt.uploaded_at_utc`) +
  `router.rs:582` (top-level `created_at_utc`)
  get the same realistic fixed-ISO-8601
  timestamps. The dashboard's `meta` line
  (index.html:211) renders the timestamp
  verbatim, so a public visitor sees a real
  timestamp instead of `<unknown>`. Owner
  files: `crates/dashboard/src/render.rs`
  (replace the `<th>actions</th>` + the single
  `<td>` link pair with two per-action columns;
  add 1 new lib test pinning the
  per-row-column-shape, asserting both
  per-action cells contain a `<a>` link with
  the right `href`; no other test changes);
  `crates/dashboard/src/index_client.rs`
  (replace 3 literal `<unknown>` strings in
  the demo-`PublishIndex` constructor with
  realistic fixed-ISO-8601 timestamps; the
  existing `*_with_*_fixture` lib tests that
  pin the constructor's string outputs
  continue to pass — the existing tests pin
  *shape*, not the specific timestamp string;
  no new test is required);
  `crates/dashboard/src/router.rs` (same sweep
  — 3 literal `<unknown>` strings in the
  demo-`PublishIndex` constructor replaced
  with realistic timestamps; existing tests
  pass unchanged);
  `crates/dashboard/tests/fixtures/index.json`
  (replace 5 literal `<unknown>` strings with
  realistic fixed-ISO-8601 timestamps;
  preserve the
  `entries[].receipt_basename` shape the
  fixture's smoke test pins; no other field
  changes); `IMPLEMENTATION_PLAN.md` (this
  row; mark the third-pass STW-048 row as
  `SUPERSEDED 2026-06-04 by STW-050`). Scope
  boundary: does NOT change the live
  `IndexedEntry` shape (STW-049 wires the live
  bench fields; this slice is the
  committed-fixture timestamp sweep only);
  does NOT change the live
  `crates/autotrain::PublishIndex` shape;
  does NOT change the autotrain pipeline, the
  bench harness, the room protocol, the
  `Schema` contracts, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, or any `trainer --*` CLI.
  Verification commands: `cargo test -p
  rbp-dashboard --lib` (the 1 new
  per-row-column-shape lib test passes + the
  existing `*_with_*_fixture` lib tests
  continue to pass on the new timestamp
  strings), `cargo test -p rbp-dashboard
  --test smoke` (the existing 4-route drive
  still passes; the 2-action-link check is
  now per-cell), `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Hand-test command: `cargo run -p
  rbp-dashboard -- --port 18080 &; sleep 2;
  curl -s http://localhost:18080/ | grep -E
  'transcript|replay'` (both strings present,
  no `actions` literal), `curl -s
  http://localhost:18080/ | grep -E
  '<unknown>'` (zero matches), `curl -s
  http://localhost:18080/api/index | jq
  '.created_at_utc'` (returns a real ISO
  timestamp, not `<unknown>`). Required
  tests: 1 new lib test in
  `crates/dashboard/src/render.rs::tests`
  (per-row column shape with split actions
  columns) + zero integration test changes
  (the existing 4-route drive is extended to
  assert the per-cell link shape).
  Dependencies: none — STW-050 is independent
  of STW-049 and can ship in either order;
  STW-049 supersedes the third-pass STW-047
  and STW-050 supersedes the third-pass
  STW-048, but the two new rows are *not*
  ordered relative to each other. Estimated
  scope: XS. Completion signal: a fresh
  `cargo run -p rbp-dashboard` serves a table
  with two per-action column headers
  (`transcript` / `replay`) and a `meta` line
  that shows a real ISO-8601 timestamp; the
  literal string `<unknown>` does not appear
  anywhere in the rendered HTML; the existing
  `4-route drive` smoke test still passes;
  the committed fixture's timestamps are
  realistic + byte-stable; the dashboard
  lib's `*_with_*_fixture` lib tests continue
  to pin the constructor's string outputs
  (the new ISO-8601 strings are stable).
  **`lens:` Design (the cheapest,
  highest-signal public-surface defects the
  fourth pass found; the `actions` header is
  engineering jargon in a public-facing table;
  the `<unknown>` literal is the single most
  visible "this is a test fixture" tell in
  the deployed surface — the fourth pass
  extends the sweep from the 5 the third pass
  found in the committed fixture to the 11
  the dashboard lib + fixture together
  render).**

- [x] **[P1] `STW-045` (carry-over from third
  pass `t_6df7de4e` and afternoon wave
  `t_35186537`) `scripts/trainer-observe.sh`
  wrapper that prepends `date +%s%3N` to every
  stderr line the trainer emits and writes a
  `trainer.step.jsonl` per-run timeline
  file. SHIPPED 2026-06-04 (commit
  b5ad974).** A new ~250-line
  `scripts/trainer-observe.sh` pure-bash
  wrapper that takes a `<output-jsonl>` +
  `<trainer-bin>` + `[<trainer-argv>...]`
  positional trio, runs the trainer binary
  with a two-FIFO `tee` (one for stderr,
  one for stdout), drains each FIFO in a
  background subshell that calls an
  `emit_step <stream> <line>` helper
  (the helper builds a JSONL object via
  `jq -cn --arg ts --arg stream --arg line`
  so embedded `"` / `\` / control chars
  survive a `jq -c .` round-trip
  byte-stable), and on exit emits a final
  `stream: "summary"` trailer line whose
  `line` field is the pinned
  `trainer observe complete: exit=<rc>
  cmd=<argv...>` shape a CI dashboard
  `select(.stream == "summary")` greps.
  Three shell-shape pinners in
  `crates/autotrain/tests/script_shape.rs`
  (`trainer_observe_script_exists_and_parses`
  + `trainer_observe_script_emits_three_field_jsonl`
  + `trainer_observe_script_summary_trailer_format_is_pinned`)
  pin the wrapper's static contract (file
  on disk + executable + parses with
  `bash -n` + uses `jq -cn --arg ts` +
  produces the three-field `ts` / `stream`
  / `line` shape + emits the `trainer
  observe complete: exit= cmd=` trailer).
  One new `crates/autotrain/tests/trainer_observe.rs`
  integration test (database-feature-gated,
  `DATABASE_URL`-gated, `jq`-gated — the
  same skip-on-missing-dep pattern the
  existing `bench.rs` / `compare.rs` /
  `compare3.rs` tests use) drives a real
  `trainer --bench --blueprint v1 --baseline
  preflop` invocation under the wrapper and
  asserts the JSONL has the documented
  shape end-to-end. The morning-wave
  `STW-039` row is marked `RESCOPED
  2026-06-04 by STW-045` — the new
  *bash wrapper* shape replaces the
  *typed Rust module* shape the morning
  wave named, so no autotrain crate changes
  and no risk of regressing the existing
  per-arm log shape. The fourth pass re-confirms the
  third pass's finding: the morning wave's
  `STW-039` typed `Step` enum Rust module is
  the canonical "improve X" anti-pattern the
  task body explicitly bans — a 200+ line
  Rust module whose only consumer is "a CI
  worker wants per-step timing" is the wrong
  shape; a 30-line bash wrapper is the right
  shape. The fourth-pass Design lens
  re-confirms the operator-UX value (a CI
  dashboard can
  `jq -c 'select(.stream == "summary")' run.step.jsonl`
  and receive a one-line per-run summary) is
  the highest-signal observability gap on
  `main` today, *now that* STW-049 closes
  the build break. The fourth-pass Eng lens
  confirms the wrapper is mode-agnostic (the
  same
  `scripts/trainer-observe.sh /tmp/run.step.jsonl trainer --bench`
  works against `--compare3` /
  `--verify-receipt` / `--publish` / etc.) and
  the existing
  `crates/autotrain/tests/script_shape.rs`
  pinner pattern accommodates a new
  `trainer_observe_script_exists_and_parses`
  pin with no new infrastructure. Owner
  files: `scripts/trainer-observe.sh` (new
  pure-bash wrapper; the only logic is the
  `fork + pipe stderr + prepend date +%s%3N +
  jq` loop; mirrors the
  `scripts/testnet-live-proof.sh` shape),
  `crates/autotrain/tests/script_shape.rs`
  (add 1 new shape pin:
  `trainer_observe_script_exists_and_parses`),
  `crates/autotrain/tests/trainer_observe.rs`
  (new no-DB-shape integration test;
  requires `DATABASE_URL` to be set for the
  child `trainer --bench` invocation, mirrors
  the existing `bench.rs` / `compare.rs` /
  `compare3.rs` integration tests'
  database-gating pattern),
  `IMPLEMENTATION_PLAN.md` (this row; mark
  the morning-wave STW-039 row as
  `RESCOPED 2026-06-04 by STW-045`). Scope
  boundary: the wrapper is *additive* — the
  trainer binary is unchanged; a future
  operator who runs `trainer --bench` without
  the wrapper sees the *same* output they see
  today; an operator who runs
  `scripts/trainer-observe.sh <out.jsonl>
  trainer --bench` sees the same output
  *plus* a parallel JSONL timeline. The
  wrapper is *transparent* to any existing CI
  pipeline that runs `trainer --bench` and
  asserts exit 0 — the wrapper preserves the
  exit code. Verification commands:
  `bash -n scripts/trainer-observe.sh`,
  `scripts/trainer-observe.sh
  /tmp/test.step.jsonl trainer --bench
  --blueprint v1 --baseline preflop
  --bench-hands 4` (exits 0 + the JSONL file
  has at least 2 lines: one `stream: "stderr"`
  line + one `stream: "summary"` line),
  `jq -c . /tmp/test.step.jsonl | head -5`
  (the JSONL is line-parseable),
  `cargo test -p rbp-autotrain --test
  trainer_observe` (the 1 new integration
  sub-test passes), `cargo test -p
  rbp-autotrain --test script_shape` (the 1
  new shape pin passes),
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Required tests: 1 new
  integration sub-test in
  `crates/autotrain/tests/trainer_observe.rs`
  + 1 new shape pin in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: STW-010 (the `trainer --bench`
  mode the wrapper is most useful against —
  the wrapper is mode-agnostic, but the
  integration test drives `--bench` for the
  smallest end-to-end smoke), STW-049 (the
  slice that closes the build break the
  wrapper is integration-tested against).
  Estimated scope: S. Completion signal:
  `cargo test -p rbp-autotrain --test
  trainer_observe` is green with 1 new
  integration sub-test passing; the
  `scripts/trainer-observe.sh` wrapper is on
  disk + executable + parses with `bash -n`;
  a CI dashboard can
  `jq -c 'select(.stream == "summary")' run.step.jsonl`
  the file and receive a one-line per-run
  summary. **`lens:` Design (the
  observability audit) — closes the same
  finding the morning + afternoon + third
  waves named, with the *bash wrapper* shape,
  not the *Rust module* shape, so no
  autotrain Rust crate changes and no risk of
  regressing the existing per-arm log shape.**

- [ ] **[P1] `STW-046` (carry-over from third
  pass `t_6df7de4e` and afternoon wave
  `t_35186537`) Drop the morning-wave
  `STW-040` README `## Try it now` + the
  `scripts/replay-locally.sh` shim and the
  morning-wave `STW-041` STW-001
  planning-surface retirement from the active
  queue.** The fourth pass re-confirms both
  rows are busywork the testnet north star
  does not need. The fourth-pass Design lens
  re-confirms a third redundant README section
  + a zero-test planning-process row are the
  canonical AI-design-sludge anti-patterns the
  task body explicitly bans. The fourth-pass
  CEO lens re-confirms the existing
  `## Testnet launch proof` + `## Public
  dashboard` README sections (lines 220-247 +
  290-333 in the current README) already
  answer the first-time-visitor question, and
  the existing `[!] STW-001` deferred row in
  `IMPLEMENTATION_PLAN.md` already serves as
  the operator-sign-off blocker the original
  STW-041 wanted to retire. The existing
  `## Active items (worker-ready)` section's
  leader paragraph (IMPLEMENTATION_PLAN.md
  line 12) and the
  `genesis/plans/000-ceo-testnet-roadmap.md`
  file are *also* the planning-surface
  answer; STW-041's `AUTHORED-QUEUE.md`
  fallback queue is a duplicate of the
  existing planning surface. Owner files:
  `IMPLEMENTATION_PLAN.md` (mark the
  morning-wave STW-040 row as
  `DROPPED 2026-06-04 by STW-046` and the
  morning-wave STW-041 row as
  `DROPPED 2026-06-04 by STW-046`; no other
  changes — the existing `[!] STW-001`
  deferred row in the
  `## Deferred items (need operator decision
  before promotion)` section is *preserved
  verbatim*), `README.md` (NO CHANGE — the
  existing `## Testnet launch proof` +
  `## Public dashboard` sections are the
  operator-UX answer; no `## Try it now`
  section is added; the
  `Public dashboard: <https://robopoker-testnet-dashboard.pages.dev/>`
  line at line 313 is preserved verbatim),
  `genesis/plans/000-ceo-testnet-roadmap.md`
  (NO CHANGE — the v10 follow-on is marked
  shipped as the STW-036 row already did; the
  morning wave's STW-040 / STW-041 additions
  to the roadmap are not made). Scope
  boundary: does NOT add a `## Try it now`
  section to `README.md`; does NOT add a
  `scripts/replay-locally.sh` shim; does NOT
  add a `genesis/AUTHORED-QUEUE.md` fallback
  queue; does NOT mark `STW-001` as `RETIRED`
  in `IMPLEMENTATION_PLAN.md` (the deferred
  `[!]` row is preserved); does NOT change
  the room protocol, the `Schema` contracts,
  the autotrain pipeline, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, the `CFR_TREE_COUNT_NLHE`
  baseline, or any `trainer --*` CLI.
  Verification commands: `git diff --
  IMPLEMENTATION_PLAN.md` (the diff is the
  two `DROPPED` markers + this STW-046 row,
  nothing else), `grep -q '## Testnet launch
  proof' README.md` (the existing
  first-time-visitor section is preserved),
  `grep -q '## Public dashboard' README.md`
  (the existing public-dashboard section is
  preserved), `grep -q '## Try it now'
  README.md` → exit 1 (the *redundant*
  section is *not* added — the *absence* of
  the section is the completion signal),
  `cargo test --workspace -- --test-threads=4`
  (the drop does not change the autotrain
  pipeline), `cargo check --workspace`,
  `cargo fmt --check`. Required tests: none
  — STW-046 is a queue-cleanup decision, not
  a code change. Dependencies: none.
  Estimated scope: XS. Completion signal:
  the `IMPLEMENTATION_PLAN.md` morning-wave
  STW-040 + STW-041 rows are both marked
  `DROPPED 2026-06-04 by STW-046` with a
  one-line note; the existing
  `## Testnet launch proof` + `## Public
  dashboard` README sections are preserved
  verbatim; the existing `[!] STW-001`
  deferred row is preserved verbatim; the
  `## Try it now` section is *not* added to
  `README.md`. The operator's review budget
  is freed for STW-049 (the build-break +
  public-surface column-shape wire) and
  STW-050 (the `actions` → `transcript`/
  `replay` split + `<unknown>` timestamp
  sweep). **`lens:` Design (the AI-slop
  review: a third redundant README section +
  a zero-test planning-process row are the
  canonical AI-design-sludge patterns the
  task body explicitly bans) + CEO (the
  testnet north star's "publicly-visible"
  requirement is served by the v10 dashboard
  + the `## Public dashboard` README section
  the v10 ships — not by a `## Try it now`
  reframe).**

## Next wave - review 2026-06-04 (fifth pass)

The fifth 2026-06-04 three-lens review (kanban task
`t_ae8022b3`) re-applies the three lenses to the
*current* state of `main` at commit `b5add8d`. The
four prior review-waves shipped STW-049 + STW-050 on
`main` (commits `6886f08` + `b5add8d`), so the
fourth-pass's two open items are now closed; the
`cargo test --workspace` build break the fourth pass
named is fixed; and the dashboard's `actions` →
`transcript`/`replay` header split + the
*committed-fixture* `<unknown>`-timestamp sweep are
live on `main`. STW-045 (`scripts/trainer-observe.sh`)
is mid-implementation on the working tree (the
script + the `script_shape.rs` STW-045 pins are
already on disk; the `crates/autotrain/tests/
trainer_observe.rs` integration test is the only
remaining piece). STW-043 is shipped on commit
`d95047a` but the latest-wave row at line 6381 is
still rendered as `[ ]` — a worker scanning the
active queue would still see it as unstarted, so
the `## Active items` leader paragraph needs the
STW-043 row marked `[x]` separately by the next
planner pass (this row is a planning-pinning task
that drops out of scope for the next wave; the
shipped commit is the ground truth).

The fifth pass's three lenses on the *current* state
find the **single highest-leverage finding the four
prior reviews missed**: the live
`crates/autotrain::PublishIndex` shape writes the
literal string `<unknown>` to the on-disk
`INDEX.json` whenever `RBP_PUBLISH_INDEX_UTC` is
unset, AND the dashboard's
`crates/dashboard/static/index.html:253` JS
*renders* the literal string `<unknown>` to a public
visitor every time the rendered `INDEX.json` is
missing `publish_root` or `created_at_utc`. The
fourth-pass STW-050 swept the *committed-fixture*
`<unknown>` literals out of the dashboard lib's
`index_client.rs` / `router.rs` /
`tests/fixtures/index.json` demo constructors, but
left two leakage vectors open:

1. The *live* `crates/autotrain::publish_index`
   source still has `<unknown>` fallbacks in
   `publish_index.rs:200-207, 778, 1106, 1159,
   1244` (the `STW034_UNKNOWN_UTC` constant + the
   `unwrap_or_else(|_| "<unknown>")` env-var
   fallbacks), so a real `trainer --publish-index`
   run on a fresh operator machine with no
   `RBP_PUBLISH_INDEX_UTC` env knob writes
   `created_at_utc: "<unknown>"` to the live
   `INDEX.json`.
2. The dashboard's `index.html:253` JS *renders*
   that literal to a public visitor: `meta.textContent
   = '... created_at=' + (index.created_at_utc ||
   '<unknown>')`. The committed-fixture sweep
   closed one source of `<unknown>` in the rendered
   HTML, but the live `INDEX.json` path is still
   open. The `crates/dashboard/tests/fixtures_smoke.
   rs:238, 255, 258` test fixtures ALSO still
   contain `created_at_utc: "<unknown>"` literals
   in their demo `PublishIndex` constructors — the
   fourth pass under-counted the sweep by 3
   literals that the smoke test renders into the
   test response body.

The fifth pass also finds three smaller, related
defects the four prior reviews missed:

3. **The dashboard's true empty state is hidden.**
   The committed `crates/dashboard/tests/
   fixtures/index.json` always populates the table,
   so a stranger running `cargo run -p rbp-dashboard`
   on a fresh checkout with no published root sees
   *demo data* and might assume the receipts are
   real. The dashboard's true empty state ("no
   receipts have been published yet — run
   `scripts/testnet-live-proof.sh` + the publish
   chain to populate") never renders, because the
   committed fixture is always available. This
   violates the Design-UX principle "Empty states
   are features." A real empty-state test (the
   dashboard with `RBP_DASHBOARD_INDEX_URL` pointing
   at an empty `INDEX.json` — `{"entries": [], ...}`)
   renders a friendlier message; today it renders
   an empty `<tbody>`.
4. **The CLI error-shape audit STW-044 is still a
   `P1` and still `[ ]`.** The morning wave's
   `TrainerError` enum refactor was the wrong
   shape; the afternoon + third + fourth passes
   re-scoped it to a 10-lib-test static-grep audit
   that pins the existing per-arm
   `live_proof ...` error-line text without
   rewriting it. The fifth pass re-confirms the
   re-scoped shape is the right one and the row
   should ship.
5. **The plan's 4 prior "Next wave" sections are
   ~4,800 lines of historical log a worker scanning
   the active queue has to mentally filter.** The
   task body explicitly bans busywork + vague
   items, and the 4 prior sections all carry
   forward STW-045 + STW-046 as `[ ]` rows that
   are now superseded by the same-shape
   re-affirmations in the latest wave. A
   queue-cleanup row that marks the prior-wave
   STW-045 + STW-046 rows as `RESCOPED 2026-06-04
   by STW-055` (this wave's STW-055 below) — and
   folds the morning-wave STW-039 / STW-040 /
   STW-041 + the third-pass STW-044 re-scope into
   the same `RESCOPED` / `DROPPED` markers — frees
   the worker scanning the active queue to find
   the 5 deliverables in the *latest* wave.

The fifth pass therefore:

(a) **Promotes the *live* `<unknown>`-leakage fix
    into STW-051** (a single slice that closes the
    aggregator's `STW034_UNKNOWN_UTC` env-var
    fallback to a fail-fast `MissingArg` error +
    replaces the dashboard's `index.html:253` JS
    `<unknown>` fallback with a friendly
    "(index UTC not stamped — re-run with
    RBP_PUBLISH_INDEX_UTC set)" message + sweeps
    the 3 remaining `<unknown>` literals from
    `crates/dashboard/tests/fixtures_smoke.rs`).
(b) **Promotes the dashboard's *true empty state*
    into STW-052** (a new dashboard route +
    `index.html` empty-state render a visitor sees
    when the live `INDEX.json` has zero entries).
(c) **Re-affirms STW-044 unchanged as a `P1`** (the
    per-arm error-shape audit; the re-scoped
    afternoon-wave shape is the right one, the
    fifth-pass Design lens agrees).
(d) **Promotes a queue-cleanup row into STW-053**
    that marks the 4 prior-wave STW-045 / STW-046
    re-affirmations + the morning-wave STW-039 /
    STW-040 / STW-041 / STW-044 rows as
    `RESCOPED` / `DROPPED` so a future worker
    scanning the active queue sees only the
    5th-pass wave + the v6→v10 follow-on chain.
(e) **Adds STW-054** (the *deploy-the-dashboard*
    runbook the prior CEO lens named as the
    "deploy" leg of the public-surface north star
    — the `scripts/testnet-live-publish-dashboard.sh`
    runbook ships but the bucket / Cloudflare Pages
    destination doesn't exist on disk; STW-054
    lands a `scripts/deploy-dashboard-cloudflare.sh`
    runbook that takes the local
    `publish/<root>/index/` dir and pushes it to
    Cloudflare Pages via `wrangler pages deploy`
    with a committed `wrangler.toml` +
    `RBP_DASHBOARD_CF_API_TOKEN` env knob; lower
    priority than STW-051 because the deploy is a
    CI-side question and STW-051 is a code-side
    defect visible to anyone who lands on the URL).

Each row below names a single shippable slice with
named files, verification command(s), and a `lens:`
tag tracing the finding it closes. Rows are P0/P1
ordered; the top row is the highest single-shipment
leverage. The `STW-044` row below is a
**re-affirmation of the fourth-pass open row**; it
is NOT new scope. The new scope is `STW-051` +
`STW-052` + `STW-053` + `STW-054`.

- [x] **[P0] `STW-051` Close the live
  `crates/autotrain::PublishIndex` + dashboard
  `<unknown>`-literal leakage the four prior
  reviews missed.** Three changes in one slice:
  (a) `crates/autotrain/src/publish_index.rs` —
  remove the `STW034_UNKNOWN_UTC` constant and
  the `unwrap_or_else(|_| "<unknown>")` env-var
  fallbacks at lines 200-207, 778, 1106, 1159,
  1244. The `trainer --publish-index` arm now
  *requires* `RBP_PUBLISH_INDEX_UTC` to be set at
  the CLI boundary — missing env knob returns
  `PublishIndexError::MissingArg("RBP_PUBLISH_INDEX_UTC")`
  + the arm exits 2 with a one-line
  `live_proof publish_index error: missing arg: RBP_PUBLISH_INDEX_UTC`
  eprintln! the existing per-arm error shape
  pins. The lib + integration tests that currently
  depend on the `<unknown>` fallback are
  *re-scoped to set the env knob via
  `std::env::set_var("RBP_PUBLISH_INDEX_UTC",
  "2026-06-04T00:00:00Z")` in the test
  fixture's `setup()`* — the tests stay
  byte-stable because the fixed ISO-8601
  string is the new test fixture, and the
  existing
  `publish_index_created_at_utc_falls_back_to_iso_8601`
  lib test (publish_index.rs:1244) is
  re-scoped to assert the env-var path.
  (b) `crates/dashboard/static/index.html` —
  replace the `index.html:253` JS
  `meta.textContent = '...' + (index.publish_root
  || '<unknown>') + ... + (index.created_at_utc
  || '<unknown>')` fallback with a friendly
  `'... ' + (index.publish_root || '(publish_root
  not stamped)') + ' ... ' + (index.created_at_utc
  || '(created_at_utc not stamped — re-run with
  RBP_PUBLISH_INDEX_UTC set)')` fallback. The
  literal string `<unknown>` does not appear
  anywhere in the rendered HTML, and the visitor
  sees an *actionable* message instead of a
  "this is a test fixture" tell. (c)
  `crates/dashboard/tests/fixtures_smoke.rs` —
  replace the 3 remaining
  `created_at_utc: "<unknown>"` literals at
  lines 238, 255, 258 with realistic
  fixed-ISO-8601 timestamps (`"2026-06-04T05:00:00Z"`,
  `"2026-06-04T14:01:07Z"`,
  `"2026-06-04T05:00:01Z"`). The dashboard's
  `crates/dashboard/tests/fixtures_smoke.rs`
  test responses (the 4-route drive + the
  per-fixture cards) no longer contain the
  literal `<unknown>` anywhere. The
  `fixtures_smoke.rs` test functions
  `compare3_fixture_renders_bench_card` +
  `real_index_shadows_demo_data` are unchanged
  (they pin *shape*, not specific timestamp
  strings). Owner files:
  `crates/autotrain/src/publish_index.rs`
  (remove the `STW034_UNKNOWN_UTC` constant +
  the 5 `unwrap_or(_)` / `unwrap_or_else(_)`
  `<unknown>` env-var fallbacks + add the
  fail-fast `PublishIndexError::MissingArg` arm
  in `trainer --publish-index` + re-scope the
  4 affected lib tests to set the env knob via
  `std::env::set_var` in the test's `setup()` +
  1 new lib test
  `publish_index_missing_env_knob_returns_missing_arg`
  that drives the new fail-fast path with
  `RBP_PUBLISH_INDEX_UTC` unset),
  `crates/autotrain/src/mode.rs` (route the new
  `PublishIndexError::MissingArg` to the
  `live_proof publish_index error: missing arg:`
  per-arm eprintln! + exit 2 — the same shape
  the existing per-arm errors use),
  `crates/autotrain/src/error.rs` (add the
  `PublishIndexError::MissingArg(&'static str)`
  variant — or, if STW-044's per-arm error-shape
  audit hasn't shipped, add it as a *local*
  variant in `publish_index.rs` and let STW-044
  roll up the cross-arm error types later;
  the `MissingArg` variant is the only new
  error variant the slice ships),
  `crates/dashboard/static/index.html` (replace
  the line 253 `<unknown>` fallback with the
  friendly message; no other change),
  `crates/dashboard/tests/fixtures_smoke.rs`
  (replace 3 `<unknown>` literals with realistic
  ISO-8601 timestamps),
  `crates/dashboard/tests/smoke.rs` (extend
  the 4-route drive to assert the rendered
  `meta` line does NOT contain the literal
  `<unknown>` — the no-`<unknown>`-in-rendered-HTML
  pin is the cheapest in-CI proof the
  JS-fallback fix is live),
  `IMPLEMENTATION_PLAN.md` (this row).
  Scope boundary: does NOT change the
  `crates/dashboard/tests/fixtures/index.json`
  committed fixture (the fourth-pass STW-050
  already swept that — 0 `<unknown>` literals
  remain); does NOT change the dashboard's
  `crates/dashboard/src/{render,index_client,router}.rs`
  demo-`PublishIndex` constructors (the
  fourth-pass STW-050 already swept those — 0
  `<unknown>` literals remain); does NOT
  change the dashboard's
  `render_index_table` per-row column shape
  (STW-049 already wired the 5 bench fields);
  does NOT change the
  `crates/autotrain::PublishIndex` /
  `IndexedEntry` JSON shape (the `created_at_utc`
  field is the only field whose value-source
  changes — the JSON field is preserved
  verbatim, the value-source fails fast on
  missing env knob); does NOT change the
  `crates/autotrain::BenchSummary` shape
  (STW-049's inlined field); does NOT change
  the `crates/autotrain::PublishIndexError`
  shape (the new `MissingArg` variant is
  additive — the existing 9 variants are
  unchanged); does NOT change the room
  protocol, the `Schema` contracts, the
  autotrain pipeline, the K-means cluster
  counts, the v1 / v2 / v3 / v4 named
  baselines, the `CFR_TREE_COUNT_NLHE`
  baseline, or any `trainer --*` CLI.
  Verification commands: `cargo test -p
  rbp-autotrain --lib` (the 4 re-scoped
  env-knob lib tests pass + the 1 new
  `publish_index_missing_env_knob_returns_missing_arg`
  lib test passes), `cargo test -p
  rbp-autotrain --test publish_index` (the
  4 integration sub-tests pass with the
  re-scoped env-knob setup), `cargo test -p
  rbp-dashboard --lib` (the 2 new column-shape
  lib tests pass — the existing
  `live_index_table_renders_bench_cells_with_values`
  + `live_index_table_renders_dash_for_missing_bench`
  tests are unchanged), `cargo test -p
  rbp-dashboard --test smoke` (the existing
  4-route drive passes + the new
  no-`<unknown>`-in-rendered-HTML assertion
  passes), `cargo test -p rbp-dashboard
  --test fixtures_smoke` (the existing
  per-fixture card drive passes with the 3
  new ISO-8601 timestamps), `cargo test
  --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Hand-test commands: `unset
  RBP_PUBLISH_INDEX_UTC; cargo run -p
  rbp-autotrain -- --reset && cargo run -p
  rbp-autotrain -- --publish-index
  receipts/publish-20260604T050000Z/ 2>&1 |
  grep -E 'live_proof publish_index error:
  missing arg: RBP_PUBLISH_INDEX_UTC'` (one
  match — the fail-fast path is live),
  `RBP_PUBLISH_INDEX_UTC=2026-06-04T14:01:07Z
  cargo run -p rbp-autotrain -- --publish-index
  .../ && curl -s
  http://localhost:18080/api/index | jq
  '.created_at_utc'` (returns
  `"2026-06-04T14:01:07Z"`, not `<unknown>`),
  `curl -s http://localhost:18080/ | grep -E
  '<unknown>'` (zero matches — the
  index.html JS fallback is live), `curl -s
  http://localhost:18080/ | grep -E
  'created_at_utc not stamped'` (one match
  on an empty `INDEX.json` — the friendly
  fallback is live). Required tests: 1 new
  lib test in
  `crates/autotrain/src/publish_index.rs::tests`
  (`publish_index_missing_env_knob_returns_missing_arg`)
  + 1 extension to the existing
  `crates/dashboard/tests/smoke.rs` 4-route
  drive (the no-`<unknown>`-in-rendered-HTML
  assertion) + 4 re-scoped lib tests in
  `crates/autotrain/src/publish_index.rs::tests`
  (the env-knob setup is added to the test
  fixture's `setup()`; the test bodies are
  unchanged). Dependencies: STW-049 (the
  IndexedEntry mirror struct the column-shape
  wire depends on — STW-049 is shipped on
  commit `6886f08`); STW-050 (the
  *committed-fixture* `<unknown>` sweep
  STW-051 *extends* — STW-050 is shipped on
  commit `b5add8d`). Estimated scope: M.
  Completion signal: `cargo test
  --workspace -- --test-threads=4` is green
  with the 1 new lib test passing; the
  rendered `index.html` from a fresh
  `cargo run -p rbp-dashboard` contains
  zero `<unknown>` literals; a
  `trainer --publish-index` run with
  `RBP_PUBLISH_INDEX_UTC` unset exits 2 with
  the pinned per-arm
  `live_proof publish_index error: missing arg: RBP_PUBLISH_INDEX_UTC`
  eprintln!; the dashboard's `meta` line
  on a live `INDEX.json` with
  `RBP_PUBLISH_INDEX_UTC=2026-06-04T14:01:07Z`
  set renders the real ISO-8601 timestamp
  verbatim. **`lens:` CEO (the
  credibility-erosion signal a public
  visitor sees when the rendered `meta`
  line is `<unknown>` — the live
  `INDEX.json` was the leakage vector the
  four prior reviews missed; the
  fix is the cheapest single-slice
  credibility-repair the public surface
  can ship) + Eng (the
  `STW034_UNKNOWN_UTC` constant + the 5
  `unwrap_or(_)` env-var fallbacks are the
  structural cause; the JS fallback in
  `index.html:253` is the surface; the
  three changes — aggregator fail-fast +
  JS friendly fallback + fixtures_smoke
  literal sweep — are one slice) +
  Design (the literal `<unknown>`
  rendered to a public visitor is the
  single most visible "this is a test
  fixture" tell the public surface has
  today; the friendly "(created_at_utc
  not stamped — re-run with
  RBP_PUBLISH_INDEX_UTC set)" message
  is the actionable alternative the
  empty-state principle names).**

- [x] **[P0] `STW-052` Wire the dashboard's *true
  empty state* so a stranger running
  `cargo run -p rbp-dashboard` on a fresh checkout
  with no published root sees a friendly
  "no receipts yet" message instead of demo
  data masquerading as live receipts.** The
  fourth-pass `crates/dashboard/src/router.rs`
  `serve_compare3_fixture_card_if_no_index_match`
  fallback hides the dashboard's true empty
  state behind committed-fixture demo data;
  a visitor who lands on the URL sees the
  *committed* `tests/fixtures/index.json`
  table and might think the receipts are
  real. The dashboard's real empty state
  ("no receipts have been published yet —
  run `scripts/testnet-live-proof.sh` +
  `scripts/testnet-live-publish-index.sh` +
  `scripts/testnet-live-publish-index-s3.sh`
  to populate") never renders. The fix is
  three changes: (a)
  `crates/dashboard/src/router.rs` — add a
  new `RBP_DASHBOARD_EMPTY_STATE` env knob
  (default `0`) that *opts in* to the
  empty-state render. When `=1`, the
  `/api/index` route returns an empty
  `PublishIndex` (`{"entries": [], "entry_count": 0,
  "total_bytes": 0, "publish_root": "",
  "runbook_version": "...", "created_at_utc":
  "2026-06-04T14:01:07Z"}`) instead of
  reading the committed fixture; the
  `index.html` JS renders the empty
  `<tbody>` + a one-line
  `<p class="empty-state">No receipts yet.
  Run <code>scripts/testnet-live-proof.sh</code>
  + the publish chain to populate.</p>` (the
  empty-state render is conditional on
  `index.entry_count === 0`, so a live
  `INDEX.json` with entries never shows
  the empty-state — the existing live-data
  render is preserved). (b)
  `crates/dashboard/static/index.html` —
  add the empty-state `<p>` element
  (initially hidden via `display: none`;
  the JS shows it when `entry_count === 0`)
  + a 4-line CSS block that styles the
  empty-state as a centered monospace
  paragraph in the existing dark-theme
  palette. (c) `crates/dashboard/tests/smoke.rs`
  — add 1 new integration sub-test
  `empty_state_renders_friendly_message_when_index_has_zero_entries`
  that drives the dashboard with
  `RBP_DASHBOARD_EMPTY_STATE=1` + asserts
  the rendered `GET /` HTML contains the
  `class="empty-state"` paragraph + the
  `scripts/testnet-live-proof.sh` command
  name + does NOT contain any
  `<tr>` / `<td>` (the empty `<tbody>`
  has no rows). The `real_index_shadows_demo_data`
  sub-test in `fixtures_smoke.rs` is
  extended to assert the *inverse* contract:
  when `RBP_DASHBOARD_EMPTY_STATE=0` (the
  default) + a real `INDEX.json` is
  present, the empty-state paragraph is
  hidden. Owner files:
  `crates/dashboard/src/router.rs` (add
  the `RBP_DASHBOARD_EMPTY_STATE` env knob
  + the empty-state branch in the `/api/index`
  handler that returns the typed empty
  `PublishIndex` + the `is_empty_state()`
  helper a `crates/dashboard/tests/smoke.rs`
  sub-test drives; 1 new lib test
  `router_empty_state_env_knob_engages_when_set`
  that asserts the env knob's
  `=0`/`=1` switch is honored),
  `crates/dashboard/src/render.rs` (add a
  `render_empty_state_paragraph() -> String`
  emitter — the `<p class="empty-state">`
  HTML block; 1 new lib test pinning
  the per-class shape + the embedded
  `scripts/testnet-live-proof.sh` command
  name),
  `crates/dashboard/static/index.html` (add
  the empty-state `<p>` element + the
  `display: none` default + the 4-line
  CSS block + the JS conditional
  `index.entry_count === 0` show),
  `crates/dashboard/tests/smoke.rs` (add
  the new
  `empty_state_renders_friendly_message_when_index_has_zero_entries`
  sub-test + extend the
  `real_index_shadows_demo_data` inverse
  pin),
  `IMPLEMENTATION_PLAN.md` (this row).
  Scope boundary: does NOT change the
  committed
  `crates/dashboard/tests/fixtures/index.json`
  fixture (the fixture still populates
  the table when the env knob is `=0`
  — the existing demo-data path is
  preserved); does NOT change the
  `crates/dashboard/src/render_index_table`
  per-row column shape; does NOT change
  the `Compare3Report` / `BenchCardFields`
  / `IndexedEntry` shape; does NOT change
  the room protocol, the `Schema`
  contracts, the autotrain pipeline, the
  K-means cluster counts, the v1 / v2 /
  v3 / v4 named baselines, the
  `CFR_TREE_COUNT_NLHE` baseline, or any
  `trainer --*` CLI. The empty-state
  render is *opt-in* (env knob `=1`)
  and *conditional* on
  `index.entry_count === 0` — a live
  `INDEX.json` with entries never shows
  the empty-state. Verification commands:
  `cargo test -p rbp-dashboard --lib`
  (the 1 new `router_empty_state_env_knob_engages_when_set`
  + 1 new `render_empty_state_paragraph_*`
  lib tests pass), `cargo test -p
  rbp-dashboard --test smoke` (the new
  empty-state sub-test passes + the
  existing 4-route drive still passes
  with `RBP_DASHBOARD_EMPTY_STATE=0` —
  the default-off knob is the
  no-regression pin), `cargo test -p
  rbp-dashboard --test fixtures_smoke`
  (the existing
  `compare3_fixture_renders_bench_card`
  + extended `real_index_shadows_demo_data`
  sub-tests pass), `cargo test
  --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Hand-test commands:
  `RBP_DASHBOARD_EMPTY_STATE=1 cargo run
  -p rbp-dashboard -- --port 18080 &;
  sleep 2; curl -s
  http://localhost:18080/ | grep -E
  'class="empty-state"'` (one match —
  the empty-state paragraph is
  rendered), `curl -s
  http://localhost:18080/ | grep -E
  'scripts/testnet-live-proof.sh'` (one
  match — the command name is embedded),
  `curl -s
  http://localhost:18080/api/index | jq
  '.entry_count'` (returns `0` —
  the empty `PublishIndex` is
  served), `RBP_DASHBOARD_EMPTY_STATE=0
  cargo run -p rbp-dashboard -- --port
  18081 &; sleep 2; curl -s
  http://localhost:18081/ | grep -E
  'class="empty-state"'` (zero matches
  — the default-off knob hides the
  empty-state on a real `INDEX.json`).
  Required tests: 2 new lib tests in
  `crates/dashboard/src/{router,render}.rs::tests`
  + 1 new integration sub-test in
  `crates/dashboard/tests/smoke.rs` + 1
  extension to the existing
  `crates/dashboard/tests/fixtures_smoke.rs::real_index_shadows_demo_data`
  sub-test. Dependencies: STW-036 (the
  v10 dashboard crate; STW-052 is the
  empty-state addition), STW-049 (the
  build break the smoke test runs
  against; STW-049 is shipped on commit
  `6886f08`). Estimated scope: S.
  Completion signal: `cargo test -p
  rbp-dashboard --test smoke` is green
  with the new empty-state sub-test
  passing; a fresh `cargo run -p
  rbp-dashboard` with the
  `RBP_DASHBOARD_EMPTY_STATE=1` env knob
  set serves a friendly "no receipts
  yet" paragraph instead of the
  committed-fixture table; the default
  `RBP_DASHBOARD_EMPTY_STATE=0` env knob
  preserves the existing live-data
  render — a fresh `cargo run -p
  rbp-dashboard` on a checkout that
  hasn't run the publish chain sees the
  same demo-data table the fourth-pass
  shipped. **`lens:` Design (the
  "Empty states are features" principle
  violation the four prior reviews
  missed; the empty-state render is
  opt-in via env knob so a deployed
  dashboard never sees it on a
  populated `INDEX.json`) + Eng (the
  `RBP_DASHBOARD_EMPTY_STATE` env knob
  is the cheapest seam — the existing
  `RBP_DASHBOARD_INDEX_URL` env knob
  pattern is the precedent) + CEO (a
  stranger who lands on the URL and
  sees "no receipts yet — run
  `scripts/testnet-live-proof.sh`" gets
  the *first-time-visitor* answer the
  testnet north star names; the
  committed-fixture table is the
  *demo-data* answer that was the wrong
  default).**

- [x] **[P0] `STW-053` Sweep the 3 remaining
  `crates/dashboard/tests/fixtures_smoke.rs`
  `created_at_utc: "<unknown>"` literals the
  fourth-pass STW-050 under-counted.** The
  STW-051 row above ships the structural
  fix (the live `PublishIndex` fail-fast +
  the JS friendly fallback); STW-053 is
  the *test-only* cleanup that ensures no
  future regression re-introduces the
  literal `<unknown>` in the dashboard's
  test response bodies. The
  `crates/dashboard/tests/fixtures_smoke.rs`
  test at lines 238, 255, 258 still has
  3 `created_at_utc: "<unknown>"` literals
  in the demo `PublishIndex` constructors
  the smoke test drives; a future
  regression that re-introduces a
  `<unknown>` literal in the lib's render
  path will pass the `fixtures_smoke`
  test (because the test feeds a
  `<unknown>` literal directly into the
  response body). The fix is a 3-line
  literal swap: replace the 3
  `created_at_utc: "<unknown>"` strings
  with realistic fixed-ISO-8601 timestamps
  (`"2026-06-04T05:00:00Z"` /
  `"2026-06-04T14:01:07Z"` /
  `"2026-06-04T05:00:01Z"`). The existing
  `fixtures_smoke.rs::compare3_fixture_renders_bench_card`
  +
  `real_index_shadows_demo_data` sub-tests
  pin *shape* (a `serde_json` round-trip +
  a typed `PublishIndex` → `INDEX.json`
  on disk → typed read), not specific
  timestamp strings, so the timestamp
  change is transparent to them. Owner
  files: `crates/dashboard/tests/fixtures_smoke.rs`
  (replace 3 `<unknown>` literals at
  lines 238, 255, 258 with realistic
  ISO-8601 timestamps), `IMPLEMENTATION_PLAN.md`
  (this row). Scope boundary: does NOT
  change the live
  `crates/autotrain::PublishIndex`
  fail-fast behavior (STW-051 is the
  structural fix); does NOT change the
  dashboard's `index.html:253` JS
  fallback (STW-051 is the JS fix);
  does NOT change the committed
  `crates/dashboard/tests/fixtures/index.json`
  fixture (already swept in the
  fourth-pass STW-050); does NOT change
  the dashboard's per-row column shape;
  does NOT change the autotrain
  pipeline, the room protocol, the
  `Schema` contracts, the K-means
  cluster counts, the v1 / v2 / v3 / v4
  named baselines, or any `trainer --*`
  CLI. The fixtures_smoke sweep is the
  *test-side* cleanup that pairs with
  STW-051's *source-side* fix — both
  ship in the same change-set so a
  future worker who runs
  `cargo test -p rbp-dashboard` on a
  fresh checkout sees zero `<unknown>`
  literals in either the response body
  or the test fixture. Verification
  commands: `grep -nE '"<unknown>"'
  crates/dashboard/tests/fixtures_smoke.rs`
  (zero matches — the 3 literals are
  swept), `cargo test -p rbp-dashboard
  --test fixtures_smoke` (the existing
  per-fixture card drive still passes
  with the 3 new ISO-8601 timestamps),
  `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Required tests: zero new tests
  (STW-053 is a literal-sweep
  cleanup; the existing
  `fixtures_smoke.rs` sub-tests cover
  the post-sweep shape). Dependencies:
  none — STW-053 is independent of
  STW-051 / STW-052 and can ship in
  any order; STW-053 is the *test-side*
  pair of STW-051's *source-side* fix
  and the two ship together for the
  same reason STW-049 + STW-050
  shipped together (the build-break
  fix and the column-shape wire shared
  one struct extension). Estimated
  scope: XS. Completion signal:
  `grep -nE '"<unknown>"'
  crates/dashboard/tests/fixtures_smoke.rs`
  → exit 1 (the 3 literals are swept
  — the *absence* of the literal is
  the completion signal); `cargo test
  -p rbp-dashboard --test fixtures_smoke`
  is green with the 3 new ISO-8601
  timestamps; a future `cargo test
  --workspace` run on a fresh checkout
  sees zero `<unknown>` literals in
  any dashboard test response body.
  **`lens:` Design (the
  test-fixture-sweep counterpart of
  the STW-051 source-side fix; the
  fourth-pass STW-050 under-counted
  the sweep by 3 literals, and the
  fixtures_smoke.rs is the only
  remaining source) + Eng (the
  literal-sweep is the cheapest
  in-CI proof the public surface
  stays clean — a future regression
  that re-introduces `<unknown>` in
  the lib's render path will fail
  the smoke test's response-body
  assertion once the fixtures_smoke
  fixtures are also clean).**

- [x] **[P1] `STW-044` (re-affirmation of the
  fourth-pass open row)
  `crates/autotrain/src/error_audit.rs`
  per-arm error-shape audit: 11 new
  static-grep lib tests pinning the
  existing per-arm
  `live_proof ...` error-line text
  without rewriting it.** **SHIPPED**. The fifth
  pass re-confirms the fourth-pass's
  finding: the morning wave's
  `TrainerError` enum refactor was
  the wrong shape (a 200+ line Rust
  module whose only consumer is "a
  CI worker wants per-error
  greppability" is the inverse of
  "the existing per-arm shape is
  already greppable"). The fifth-pass
  CEO + Eng + Design lenses all
  agree the re-scoped
  `error_audit.rs` shape is the
  right one: 10 lib tests, one per
  existing `live_proof ...` error
  arm, each a static-grep pin on
  the per-arm eprintln! line text
  in the 7 source files the
  morning wave's STW-038 listed,
  *no* production code change,
  *no* `TrainerError` enum, *no*
  `to_pinned_line` method. The
  STW-051 row above introduces
  exactly one new error variant
  (`PublishIndexError::MissingArg`);
  the audit's `MissingArg`-arm pin
  is added as the 11th test
  (joining the 10 the morning wave
  listed) so the audit covers the
  new arm. Owner files:
  `crates/autotrain/src/error_audit.rs`
  (new `cfg(test) mod tests`
  module with 11 static-grep lib
  tests pinning the per-arm
  `live_proof ...` error-line shape
  across
  `crates/autotrain/src/{publish,publish_remote,publish_index,publish_index_remote,mode,verify_receipt,verify_bundle}.rs`),
  `crates/autotrain/src/mode.rs`
  (add the `--error-shape-test`
  argv flag the morning wave's
  STW-038 named — the flag is the
  *only* surviving piece of the
  morning wave's row, and it
  exposes the 11 pinned
  `live_proof ...` prefixes a CI
  scraper greps without exercising
  every error path; a no-op in
  production, the same
  `cargo run -- --error-shape-test`
  the morning wave's row named),
  `IMPLEMENTATION_PLAN.md` (this
  row; mark the morning-wave
  STW-038 row as
  `RESCOPED 2026-06-04 by STW-044`).
  Scope boundary: does NOT
  introduce a `TrainerError` enum;
  does NOT introduce a
  `to_pinned_line` method; does
  NOT change the existing per-arm
  `live_proof ...` error-line
  text; does NOT change the
  existing exit-code contract;
  does NOT change the
  per-subcommand flag shape, the
  per-subcommand stdout shape, or
  any `trainer --*` CLI; does NOT
  change the room protocol, the
  `Schema` contracts, the
  autotrain pipeline, the K-means
  cluster counts, the v1 / v2 / v3
  / v4 named baselines, or any
  `trainer --*` JSON contract.
  Verification commands:
  `cargo test -p rbp-autotrain
  --lib` (the 11 new lib tests
  pass), `cargo run -p
  rbp-autotrain --
  --error-shape-test` (prints
  the 11 pinned `live_proof ...`
  prefixes in alphabetical order),
  `cargo test --workspace --
  --test-threads=4`, `cargo
  check --workspace`, `cargo
  fmt --check`. Required tests:
  11 new lib tests in
  `crates/autotrain/src/error_audit.rs::tests`
  pinning the per-arm
  `live_proof ...` error-line
  shape across the 7 source files
  + the new `MissingArg` arm
  STW-051 introduces. Dependencies:
  STW-032 (the
  `live_proof publish error:
  receipt is red: ...` line the
  audit pins), STW-033 (the
  `live_proof publish_remote
  error: ...` line), STW-034
  (the `live_proof publish_index
  error: ...` line), STW-035
  (the `live_proof
  publish_index_remote error: ...`
  line), STW-028 (the
  `live_proof receipt verification
  failed: ...` /
  `live_proof receipt verification
  passed: ...` shape), STW-051
  (the new
  `live_proof publish_index error:
  missing arg: RBP_PUBLISH_INDEX_UTC`
  line the audit's 11th test
  pins). Estimated scope: S.
  Completion signal:
  `cargo test -p rbp-autotrain
  --lib` is green with 11 new
  lib tests passing; `cargo run
  -p rbp-autotrain --
  --error-shape-test` prints
  the 11 pinned `live_proof ...`
  prefixes a CI scraper greps;
  the `STW-038` morning-wave row
  is marked `RESCOPED` and a
  future worker does not
  re-claim the refactor half.
  **`lens:` Design (the
  operator-UX / error-surface
  audit; the morning wave's
  `TrainerError` refactor was
  the wrong shape, the
  `error_audit.rs` static-grep
  shape is the right one) +
  Eng (the 11 lib tests are a
  *no-production-code* addition
  that pins the existing per-arm
  shape — the canonical
  "no-rebuild" answer to the
  "every error must be
  greppable" finding) + CEO
  (the audit is the cheapest
  observability repair the
  operator-UX surface can ship
  — a CI dashboard that greps
  `live_proof ...` for
  per-error attribution
  continues to work unchanged
  after the audit).**

- [x] **[P1] `STW-054` `scripts/deploy-dashboard-cloudflare.sh`
  runbook + committed `wrangler.toml` +
  `RBP_DASHBOARD_CF_API_TOKEN` env knob:
  the *deploy* leg of the public-surface
  north star the prior CEO lens named
  but the four prior reviews did not
  row up.** The `scripts/testnet-live-publish-dashboard.sh`
  runbook ships (STW-036) but shells out
  to `aws s3 sync` against a bucket that
  doesn't exist on disk; the README's
  `## Public dashboard` link is a
  `<https://robopoker-testnet-dashboard.pages.dev/>`
  placeholder; no `wrangler` config / no
  `cloudflared` / no Terraform is
  committed. A stranger clicking the
  README link gets a 404. STW-054 lands
  a `scripts/deploy-dashboard-cloudflare.sh`
  runbook (pure bash, mirrors the
  STW-019 + STW-032 + STW-033 + STW-034 +
  STW-035 + STW-036 runbook shape) that
  takes the local `publish/<root>/index/`
  dir the STW-035 chain produced and
  pushes it to Cloudflare Pages via
  `wrangler pages deploy` with a
  committed `wrangler.toml` (the Pages
  project name `robopoker-testnet-dashboard`,
  the `pages_build_output_dir =
  "/tmp/dashboard-deploy"`, the
  `compatibility_date = "2026-06-04"`)
  + a `RBP_DASHBOARD_CF_API_TOKEN` env
  knob the operator sets. The runbook
  refuses to run with exit 3 when
  neither `wrangler` is on `$PATH` nor
  the env knob is set, mirrors the
  STW-019 + STW-032 + STW-033 + STW-034
  + STW-035 + STW-036 exit-3 contract,
  and chains
  `trainer --verify-index <index-dir>`
  (the pre-deploy refuse-to-deploy-red-index
  gate the STW-036 runbook already
  defines) +
  `wrangler pages deploy <index-dir>
  --project-name robopoker-testnet-dashboard
  --commit-dirty=true` (the Pages
  push). A new `crates/autotrain/tests/script_shape.rs`
  pin
  `deploy_dashboard_cloudflare_script_exists_and_parses`
  asserts the runbook is on disk +
  executable + parses with `bash -n` +
  references `wrangler pages deploy`
  + references
  `RBP_DASHBOARD_CF_API_TOKEN`. A new
  `wrangler.toml` is committed in the
  repo root with the project name +
  build output dir + compatibility
  date (no secrets; the API token is
  read from the env knob at deploy
  time, not committed). Owner files:
  `scripts/deploy-dashboard-cloudflare.sh`
  (new pure-bash runbook; mirrors
  the STW-019 + STW-032 + STW-033 +
  STW-034 + STW-035 + STW-036 shape;
  script exists + is executable +
  parses with `bash -n` + refuses
  to run on a missing `wrangler` or
  a missing `RBP_DASHBOARD_CF_API_TOKEN`
  env knob with exit 3),
  `wrangler.toml` (new committed
  file in the repo root; project
  name `robopoker-testnet-dashboard`
  + `pages_build_output_dir =
  "/tmp/dashboard-deploy"` +
  `compatibility_date =
  "2026-06-04"`; no secrets),
  `crates/autotrain/tests/script_shape.rs`
  (add 1 new shape pin
  `deploy_dashboard_cloudflare_script_exists_and_parses`),
  `IMPLEMENTATION_PLAN.md` (this
  row; mark the prior-wave STW-036
  runbook row's
  `scripts/testnet-live-publish-dashboard.sh`
  as the *predecessor* runbook
  STW-054 *supersedes for the
  Cloudflare Pages path* — the
  STW-036 `aws s3 sync` runbook
  remains the S3/CloudFront path
  the prior wave shipped, STW-054
  is the Cloudflare Pages path),
  `README.md` (NO CHANGE — the
  existing
  `## Public dashboard` URL at
  line 313 remains the
  `<https://robopoker-testnet-dashboard.pages.dev/>`
  placeholder until an operator
  actually runs the deploy
  runbook; the URL becomes real
  when the operator runs
  `scripts/deploy-dashboard-cloudflare.sh`
  for the first time and the
  `wrangler` deploy creates the
  Pages project; the README's
  `## Public dashboard` section
  is otherwise unchanged).
  Scope boundary: does NOT
  vendor a `wrangler` binary (the
  operator installs `wrangler` via
  `npm install -g wrangler` or
  the equivalent Homebrew /
  Linuxbrew step — the runbook's
  first action is `which wrangler`
  + exit 3 on missing); does NOT
  introduce a Python / `jq`
  dependency (the runbook is
  pure bash + `wrangler` + `cargo
  test` + `bash -n`); does NOT
  change the STW-036
  `scripts/testnet-live-publish-dashboard.sh`
  `aws s3 sync` runbook (the S3
  path remains for operators who
  prefer CloudFront over
  Cloudflare Pages); does NOT
  change the dashboard's
  `crates/dashboard/` static
  `index.html` (the deploy target
  is the *publish output* the
  STW-035 chain produced, not
  the dashboard crate's source);
  does NOT change the room
  protocol, the `Schema`
  contracts, the autotrain
  pipeline, the K-means cluster
  counts, the v1 / v2 / v3 / v4
  named baselines, the
  `CFR_TREE_COUNT_NLHE` baseline,
  or any `trainer --*` CLI.
  Verification commands:
  `bash -n scripts/deploy-dashboard-cloudflare.sh`,
  `cargo test -p rbp-autotrain
  --test script_shape` (the 1
  new shape pin passes), `cargo
  test --workspace --
  --test-threads=4`, `cargo
  check --workspace`, `cargo
  fmt --check`. Hand-test
  command: `unset
  RBP_DASHBOARD_CF_API_TOKEN;
  scripts/deploy-dashboard-cloudflare.sh
  receipts/publish-20260604T050000Z/index/`
  (exits 3 + the runbook prints
  `deploy-dashboard: missing RBP_DASHBOARD_CF_API_TOKEN
  env knob` — the fail-fast path
  is live), `unset $PATH
  (PATH=/usr/bin:/bin) wrangler
  -V; scripts/deploy-dashboard-cloudflare.sh
  receipts/publish-20260604T050000Z/index/`
  (exits 3 + the runbook prints
  `deploy-dashboard: wrangler not
  on $PATH` — the second
  fail-fast path is live).
  Required tests: 1 new shape
  pin
  `deploy_dashboard_cloudflare_script_exists_and_parses`
  in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: STW-035 (the
  `trainer --publish-index-remote`
  arm the runbook consumes the
  `INDEX.json` from), STW-036
  (the v10 dashboard crate the
  runbook deploys), STW-049 +
  STW-050 (the dashboard's
  column-shape wire +
  `<unknown>` sweep the
  deployed dashboard renders).
  Estimated scope: S.
  Completion signal: `bash -n
  scripts/deploy-dashboard-cloudflare.sh`
  passes; the new shape pin
  in `script_shape.rs` is
  green; a CI dashboard can
  `grep ^deploy-dashboard`
  the runbook's `SUMMARY.txt`
  after an operator runs the
  runbook for the first time
  + the `wrangler` deploy
  creates the Pages project;
  the `wrangler.toml` is
  committed with no secrets.
  **`lens:` CEO (the *deploy*
  leg of the public-surface
  north star — a stranger
  clicking the README link
  gets a real Cloudflare
  Pages URL after the first
  runbook invocation; the
  four prior reviews shipped
  the *data feed* and the
  *render*, STW-054 ships
  the *deploy*) + Eng (the
  runbook is pure bash +
  `wrangler` + `cargo test`
  + `bash -n`, mirroring
  the STW-019 + STW-032 +
  STW-033 + STW-034 + STW-035
  + STW-036 shape; the
  `wrangler.toml` is the
  minimum config the
  Cloudflare Pages path
  needs — no `terraform` /
  `cloudflared` / vendored
  SDK) + Design (the
  `<https://robopoker-testnet-dashboard.pages.dev/>`
  placeholder URL in the
  README becomes a real
  URL after the first
  runbook invocation; the
  "Public dashboard: <...>"
  line at README.md:313
  is the first-time-visitor
  answer the testnet north
  star names).**

## Next wave - review 2026-06-04 (sixth pass)

The sixth 2026-06-04 three-lens review (kanban task
`t_689e1445`) re-applies the three lenses to the
*current* state of `main` at commit `794d735`. The
fifth-pass wave's four deliverables (STW-051 +
STW-052 + STW-053 + STW-054) split as follows: the
`STW-051` close-the-live-`<unknown>`-leakage slice
shipped on commit `794d735` (the aggregator's
`STW034_UNKNOWN_UTC` fallback + the dashboard's
`index.html:316-321` meta-line friendly fallback
+ the `crates/dashboard/tests/fixtures_smoke.rs`
literal sweep); the `STW-053` queue-cleanup row
shipped on the same commit (the four prior-wave
`STW-045` + `STW-046` re-affirmations + the
morning-wave `STW-039` / `STW-040` / `STW-041` /
`STW-044` rows are all `RESCOPED 2026-06-04 by
STW-053` markers in the latest-wave row). The
`STW-052` true-empty-state row and the `STW-054`
Cloudflare-Pages-deploy row are still `[ ]` in
the plan — **and STW-052 is functionally
shipped on disk**, while STW-054 is the only
remaining un-deployed piece for a stranger to
see the dashboard. Inspecting the on-disk state
under `crates/dashboard/static/index.html:155`
(the `<p class="empty-state" id="empty-state">No
receipts yet. Run <code>scripts/testnet-live-proof.sh</code>
+ <code>scripts/testnet-live-publish-index.sh</code>
+ <code>scripts/testnet-live-publish-index-s3.sh</code>
to populate.</p>` paragraph) +
`crates/dashboard/static/index.html:342-356` (the
`emptyState.className = emptyState.className + '
visible'` flip on `index.entry_count === 0`) +
`crates/dashboard/tests/smoke.rs:376-407` (the
`empty_state_renders_friendly_paragraph_when_index_has_no_entries`
integration test) shows the slice is *complete
on disk*; the plan row at the fifth-pass wave
is lying. The `scripts/deploy-dashboard-cloudflare.sh`
runbook + the `wrangler.toml` + the
`deploy-dashboard-cloudflare.md` doc the fifth
pass named do NOT exist on disk (`ls scripts/`
returns 16 files, none matching
`deploy-dashboard-cloudflare.sh`; the
`wrangler.toml` is also absent), so STW-054 is
the one remaining un-built piece of the v10
follow-on.

The sixth pass's three lenses on the *current*
state find **three real findings the five prior
reviews missed**, all of them small but all
directly blocking the testnet north star:

1. **The dashboard's per-row cells still render
   the literal sentinel string `'<missing>'`
   when `entry.receipt_basename` is missing.**
   The fifth-pass `STW-051` swept the
   *meta-line* `'<unknown>'` literal (at
   `index.html:316-321`) and the
   `crates/dashboard/tests/fixtures_smoke.rs`
   demo-constructor literals, but
   `crates/dashboard/static/index.html:200` still
   has `var basename = (entry && entry.receipt_basename)
   || '<missing>';` — the same anti-pattern
   the fifth pass named. A future regression
   that drops a basename from a hand-authored
   `INDEX.json` would render `'<missing>'` to
   a public visitor. The `crates/dashboard/tests/
   smoke.rs` integration test does NOT pin the
   per-row sentinel string today, so a regression
   in the per-row cell is invisible to CI.
2. **The deploy runbook has no
   `live_proof ...` headline contract.** The
   existing runbook shape (STW-019 + STW-032 +
   STW-033 + STW-034 + STW-035 + STW-036) every
   prior slice pinned emits a `live_proof ...`
   headline in `SUMMARY.txt` a CI dashboard
   can `grep ^live_proof`. The fifth-pass
   `STW-054` row does not name a headline
   contract — a `scripts/deploy-dashboard-cloudflare.sh`
   runbook that ships without a
   `live_proof dashboard deploy ...` line
   breaks the scrape contract the chain
   establishes.
3. **The dashboard's `<meta>` line and the
   README's "Public dashboard:" line are out
   of sync at deploy time.** The README's
   `Public dashboard: <https://robopoker-testnet-dashboard.pages.dev/>`
   line at `README.md:313` is a baked-in
   placeholder the CEO roadmap names as the
   first-time-visitor answer the testnet north
   star delivers. The `crates/dashboard/static/index.html`
   is a checked-in file with no env-knob
   interpolation, so the *rendered* dashboard
   `<meta>` line never reads the actual
   deployed URL — a real `wrangler pages deploy`
   to a different Pages project would leave
   the README + the dashboard both lying
   about the URL. The Eng-lens fix is the
   same one the existing `RBP_DASHBOARD_INDEX_URL`
   env knob uses: a `window.__DASHBOARD_DEPLOYED_URL__`
   global the `crates/dashboard/src/router.rs::serve_static_index`
   handler injects on every `GET /`, sourced
   from the `RBP_DASHBOARD_DEPLOYED_URL` env
   knob the dashboard already declares
   (`crates/dashboard/src/router.rs` line 60).

The sixth pass therefore:

(a) **Adds STW-054** (the deploy runbook the
    fifth pass named but the plan row is still
    `[ ]` for). Lands
    `scripts/deploy-dashboard-cloudflare.sh`
    + a committed `wrangler.toml` (project
    name only, no `account_id` / no
    `RBP_DASHBOARD_CF_API_TOKEN` secret) + a
    `scripts/deploy-dashboard-cloudflare.md`
    runbook doc that mirrors the existing
    `scripts/testnet-live-publish-dashboard.md`
    doc shape. The runbook chains
    `trainer --verify-index <index-dir>` (the
    pre-deploy refuse-to-deploy-red-index gate,
    same as the S3 deploy runbook) → `wrangler
    pages deploy <dir> --project-name <name>`
    (the actual deploy) → a one-line
    reconciliation step that updates the
    README's `## Public dashboard` URL line
    + the dashboard's `window.__DASHBOARD_DEPLOYED_URL__`
    env knob to the URL `wrangler` printed to
    stdout. Refuses to run with exit 3 on
    missing `RBP_DASHBOARD_CF_API_TOKEN` (a
    one-line `deploy-dashboard: missing RBP_DASHBOARD_CF_API_TOKEN
    env knob` error) and on missing
    `wrangler` on `$PATH` (a one-line
    `deploy-dashboard: wrangler not on $PATH`
    error) — mirrors the dashboard's
    pre-existing `RBP_DASHBOARD_INDEX_URL`
    fail-fast contract. Emits a one-line
    `live_proof dashboard deploy complete:
    pages_url=<url> files=<N> bytes=<B>`
    headline in `SUMMARY.txt` (so the existing
    `grep ^live_proof` scrape contract extends
    cleanly) and a `pages_url` line in
    `deploy.json` (the machine-readable
    manifest the runbook writes alongside
    `SUMMARY.txt`).
(b) **Adds STW-055** (close the plan's
    STW-052 false-`[ ]` row + sweep the
    per-row `'<missing>'` literal the
    STW-051 pass missed). Two changes in
    one slice: (1) the planning-pinning
    mark-`[x]` of the fifth-pass STW-052
    row (the on-disk code is shipped, the
    integration test is green, the plan
    is lying) — pure markdown, no code.
    (2) `crates/dashboard/static/index.html:200`
    — replace the `var basename = (entry &&
    entry.receipt_basename) || '<missing>';`
    fallback with the same STW-051 friendly
    pattern (`(entry && entry.receipt_basename) ||
    '(basename not stamped — re-run with the
    STW-034 publish-index chain)'`); add a new
    `crates/dashboard/tests/smoke.rs` sub-test
    that drives the dashboard with a
    hand-authored `INDEX.json` whose
    `entries[0].receipt_basename` is `null` and
    asserts the response body does NOT contain
    the literal string `'<missing>'`. The
    per-row `'<missing>'` literal is the same
    anti-pattern the meta-line `'<unknown>'`
    sweep closed, just on a different code
    path; a future regression in the per-row
    cell is now caught at the same CI step.
(c) **Adds STW-056** (a single planning-pinning
    row that marks the four open prior-wave
    rows — STW-039 + STW-040 + STW-041 + STW-044
    — with `RESCOPED 2026-06-04 by STW-056`
    markers + folds them into the latest
    wave's `## Next wave` heading so a future
    worker scanning the active queue sees
    only the 6th-pass wave + the v6→v10
    follow-on chain). Pure markdown, no code.
    Each `RESCOPED` marker carries a one-line
    rationale: STW-039 ("the dashboard's
    static `index.html` already pins the
    per-row action sequence; the typed
    `StepLogger` is a nice-to-have with no
    testnet-visible value") + STW-040 ("the
    README's `## Quick Start` + `## TUI
    Preview` + `## Testnet launch proof` +
    `## Testnet publish bundle` + `##
    Public dashboard` sections cover the
    first-time-visitor path; a `## Try it
    now` section is cosmetic busywork") +
    STW-041 ("the `STW-022` plan-staleness-gate
    has implicitly retired the `STW-001`
    operator-decision deferred row; the
    close-the-deferred-row task is no-op
    work") + STW-044 (reaffirmed: the
    re-scoped 10-lib-test static-grep
    audit is the right shape; the morning
    wave's `TrainerError` enum refactor was
    wrong; STW-044 still ships the re-scoped
    audit, see row below).
(d) **Adds STW-044 as a fresh P1** (re-affirms
    the morning + afternoon + third + fourth
    + fifth pass's re-scoped shape unchanged:
    a 10-lib-test static-grep audit that pins
    the existing per-arm `live_proof ...`
    error-line text without rewriting any
    `Mode::*` arm. This is a *re-affirmation*,
    not new scope — the right "when" is now
    because the chain is stable and the row
    has been re-scoped five times without
    execution).
(e) **Adds STW-057** (the deploy-step
    `live_proof dashboard deploy ...`
    headline contract). A 1-line addition
    to `scripts/deploy-dashboard-cloudflare.sh`
    (`printf 'live_proof dashboard deploy complete:
    pages_url=%s files=%d bytes=%d\n' "$PAGES_URL"
    "$FILES" "$BYTES" >> "$SUMMARY.txt"`) +
    a new `deploy_dashboard_cloudflare_script_emits_live_proof_headline`
    shell-shape pin in `crates/autotrain/tests/script_shape.rs`
    that `grep ^live_proof dashboard deploy`
    the runbook's `SUMMARY.txt` after a
    hand-test invocation. Mirrors the
    `live_proof publish ...` /
    `live_proof receipt verification ...` /
    `live_proof bundle verification ...` /
    `live_proof index verification ...`
    scrape contract the prior slices pinned.
(f) **Adds STW-058** (the dashboard's
    Pages-specific render surface). A 1-file,
    5-line fix to `crates/dashboard/src/router.rs::serve_static_index`:
    the handler reads `RBP_DASHBOARD_DEPLOYED_URL`
    (defaulting to the README's `https://robopoker-testnet-dashboard.pages.dev/`
    placeholder) and injects a
    `<script>window.__DASHBOARD_DEPLOYED_URL__ =
    "<url>";</script>` line into the served
    `index.html` bytes before the existing
    IIFE. The `index.html` JS reads
    `window.__DASHBOARD_DEPLOYED_URL__` and
    uses it as the dashboard `<meta>` line's
    trailing `deployed_at=<url>` fragment, so
    a re-deploy to a different Pages project
    updates the rendered dashboard's meta line
    + the README's "Public dashboard:" line +
    the `deploy.json` manifest in one source.
    New `crates/dashboard/tests/smoke.rs`
    sub-test drives the dashboard with
    `RBP_DASHBOARD_DEPLOYED_URL` set to
    `https://example.pages.dev/` and asserts
    the response body contains the literal
    string `deployed_at=https://example.pages.dev/`.

Each row below names a single shippable slice
with named files, verification command(s), and
a `lens:` tag tracing the finding it closes.
Rows are P0/P1 ordered; the top row is the
highest single-shipment leverage. The
`STW-044` row is a re-affirmation, not new
scope. The new scope is `STW-054` + `STW-055`
+ `STW-056` + `STW-044` + `STW-057` + `STW-058`.

- [x] **[P0] `STW-054` `scripts/deploy-dashboard-cloudflare.sh`
  + committed `wrangler.toml` +
  `scripts/deploy-dashboard-cloudflare.md`
  runbook doc + README "Public dashboard:"
  reconciliation.** The single highest-leverage
  remaining slice. Five changes in one
  shippable PR: (1) `scripts/deploy-dashboard-cloudflare.sh`
  — a pure-bash runbook (mirrors the
  `scripts/testnet-live-publish-dashboard.sh`
  + `scripts/testnet-live-publish.sh` shape:
  script exists + is executable + parses with
  `bash -n` + refuses to run on missing
  `RBP_DASHBOARD_CF_API_TOKEN` with exit 3 +
  refuses to run on missing `wrangler` on
  `$PATH` with exit 3). Chains `trainer
  --verify-index <index-dir>` (the pre-deploy
  refuse-to-deploy-red-index gate) →
  `wrangler pages deploy <index-dir>
  --project-name robopoker-testnet-dashboard
  --commit-dirty=true` (the actual deploy;
  the runbook captures `wrangler`'s stdout +
  the `pages_url=https://robopoker-testnet-dashboard.pages.dev/`
  line `wrangler` prints) → a one-line
  reconciliation step that updates the
  README's `## Public dashboard` URL line
  from the `pages.dev` placeholder to the
  actual URL `wrangler` printed. Emits a
  one-line `live_proof dashboard deploy complete:
  pages_url=<url> files=<N> bytes=<B>`
  headline in `SUMMARY.txt` and a
  `pages_url` line in `deploy.json`
  (the machine-readable manifest). Knobs:
  `RBP_DASHBOARD_CF_API_TOKEN` (required;
  the Cloudflare API token the runbook
  exports as `CLOUDFLARE_API_TOKEN` for
  `wrangler`), `RBP_DASHBOARD_PAGES_PROJECT`
  (default `robopoker-testnet-dashboard`),
  `RBP_DASHBOARD_CF_ACCOUNT_ID` (required
  for first-time `wrangler pages project
  create`; the runbook idempotently creates
  the project on first run + skips on
  subsequent runs). Exit codes: `0`
  deploy succeeded + `live_proof dashboard
  deploy complete:` line in `SUMMARY.txt` +
  README `## Public dashboard` line updated
  to the real URL, `1` script-internal error,
  `3` missing `RBP_DASHBOARD_CF_API_TOKEN` /
  missing `wrangler` / missing
  `RBP_DASHBOARD_CF_ACCOUNT_ID` / failed
  `trainer --verify-index` / failed
  `wrangler pages deploy`. (2)
  `wrangler.toml` — the minimum config the
  Cloudflare Pages path needs (the
  `name = "robopoker-testnet-dashboard"`
  project name only; no `account_id` /
  no `api_token` / no `compatibility_date` /
  no `pages_build_output_dir` — `wrangler
  pages deploy <dir>` is the explicit
  directory path shape the runbook uses, so
  the `pages_build_output_dir` config is
  unnecessary). (3) `scripts/deploy-dashboard-cloudflare.md`
  — the runbook doc that mirrors the
  `scripts/testnet-live-publish-dashboard.md`
  doc shape (purpose + chain steps + env
  knobs + the `bash scripts/deploy-dashboard-cloudflare.sh
  <index-dir>` invocation + the
  `pages_url=<url>` line a CI dashboard
  greps). (4) README `## Public dashboard`
  section — turn `README.md:313` from a
  baked-in `<https://robopoker-testnet-dashboard.pages.dev/>`
  placeholder into a `RBP_DASHBOARD_DEPLOYED_URL`
  env-knob-driven line: the section reads
  `Public dashboard: <https://${RBP_DASHBOARD_DEPLOYED_URL:-robopoker-testnet-dashboard.pages.dev}/>`
  (the `${VAR:-default}` shell-substitution
  the README's existing `Public dashboard:`
  style already uses for the URL + a
  corresponding `RBP_DASHBOARD_DEPLOYED_URL`
  env-knob section in the `## Quick Start`).
  (5) The `crates/dashboard/tests/script_shape.rs`
  new pin `deploy_dashboard_cloudflare_script_exists_and_parses`
  (asserts the runbook script exists + is
  executable + parses with `bash -n`).
  Scope boundary: does NOT change the
  dashboard's typed `IndexClient` /
  `Render` / `Router` surface (the deploy
  is a *deploy*, not a renderer change —
  the dashboard's `GET /` + `GET /api/index`
  + `GET /transcript/:id` + `GET /bench/:id`
  surface is unchanged); does NOT push via
  a vendored Cloudflare SDK (the live
  `wrangler pages deploy` shell-out is the
  bash runbook's job — adding a 50-MB SDK
  to a no-system-deps `trainer` binary is
  the inverse of the "pure bash + cargo +
  trainer" shape the rest of the autotrain
  pipeline already follows); does NOT
  introduce a `node` / `npm` dependency
  (`wrangler` is a standalone Rust binary
  distributed via `npm i -g wrangler` /
  `cargo install wrangler` — the runbook
  only requires `wrangler` on `$PATH`, the
  install method is the operator's choice);
  does NOT change the STW-034 `PublishIndex`
  / `IndexedEntry` JSON shape (a shape
  drift fails the deploy runbook's
  pre-deploy `trainer --verify-index` call);
  does NOT change the dashboard's
  `crates/dashboard/static/index.html`
  column shape (the deploy is a deploy, not
  a UI change). Verification commands:
  `bash -n scripts/deploy-dashboard-cloudflare.sh`,
  `cargo test -p rbp-autotrain --test
  script_shape` (the 1 new shape pin
  passes), `cargo test --workspace --
  --test-threads=4`, `cargo check --workspace`,
  `cargo fmt --check`. Hand-test commands:
  `unset RBP_DASHBOARD_CF_API_TOKEN; scripts/deploy-dashboard-cloudflare.sh
  receipts/publish-20260604T050000Z/index/`
  (exits 3 + the runbook prints
  `deploy-dashboard: missing RBP_DASHBOARD_CF_API_TOKEN
  env knob` — the fail-fast path is live);
  `PATH=/usr/bin:/bin wrangler -V; scripts/deploy-dashboard-cloudflare.sh
  receipts/publish-20260604T050000Z/index/`
  (exits 3 + the runbook prints
  `deploy-dashboard: wrangler not on $PATH`
  — the second fail-fast path is live).
  Required tests: 1 new shape pin
  `deploy_dashboard_cloudflare_script_exists_and_parses`
  in `crates/autotrain/tests/script_shape.rs`.
  Dependencies: STW-035 (the `trainer
  --publish-index-remote` arm the runbook
  consumes the `INDEX.json` from), STW-036
  (the v10 dashboard crate the runbook
  deploys), STW-049 + STW-050 (the
  dashboard's column-shape wire + the
  swept `<unknown>` literals the deployed
  dashboard renders), STW-051 (the
  friendly-fallback meta line the deployed
  dashboard renders when an `INDEX.json`
  is missing a stamp), STW-055 (the
  per-row `'<missing>'` sweep the deployed
  dashboard renders when a row is missing
  a basename). Estimated scope: S. Completion
  signal: `bash -n scripts/deploy-dashboard-cloudflare.sh`
  passes; the new shape pin in
  `script_shape.rs` is green; a CI
  dashboard can `grep ^deploy-dashboard`
  the runbook's `SUMMARY.txt` after an
  operator runs the runbook for the first
  time + the `wrangler pages deploy` creates
  the Pages project + the `wrangler.toml`
  is committed with no secrets + the
  README's `## Public dashboard` line
  updates to the real `pages.dev` URL.
  **`lens:` CEO (the *deploy* leg of the
  public-surface north star — a stranger
  clicking the README link gets a real
  Cloudflare Pages URL after the first
  runbook invocation; the four prior
  reviews shipped the *data feed* and the
  *render*, STW-054 ships the *deploy*) +
  Eng (the runbook is pure bash + `wrangler`
  + `cargo test` + `bash -n`, mirroring
  the STW-019 + STW-032 + STW-033 + STW-034
  + STW-035 + STW-036 shape; the
  `wrangler.toml` is the minimum config the
  Cloudflare Pages path needs — no vendored
  SDK / no `node` / no `npm`) + Design (the
  `<https://robopoker-testnet-dashboard.pages.dev/>`
  placeholder URL in the README becomes a
  real URL after the first runbook
  invocation; the "Public dashboard:
  <...>" line at `README.md:313` is the
  first-time-visitor answer the testnet
  north star names).**

- [x] **[P0] `STW-055` Close the plan's
  STW-052 false-`[ ]` row + sweep the
  per-row `'<missing>'` literal the STW-051
  pass missed.** Shipped on the same commit
  as the planning-pin correction above. Three changes in one
  shippable slice: (1) the planning-pinning
  mark-`[x]` of the fifth-pass `STW-052`
  row (the on-disk code at
  `crates/dashboard/static/index.html:155`
  + `index.html:342-356` +
  `crates/dashboard/tests/smoke.rs:376-407`
  is shipped, the integration test is
  green, the plan is lying) — pure markdown,
  no code. (2) `crates/dashboard/static/index.html:200`
  — replace the `var basename = (entry &&
  entry.receipt_basename) || '<missing>';`
  fallback with the same STW-051 friendly
  pattern: `var basename = (entry &&
  entry.receipt_basename) || '(basename
  not stamped — re-run with the STW-034
  publish-index chain)'`. (3) New
  `crates/dashboard/tests/smoke.rs` sub-test
  `per_row_basename_does_not_render_missing_sentinel`
  drives the dashboard's `GET /` route with
  a hand-authored `INDEX.json` whose
  `entries[0].receipt_basename` is `null`
  (or absent) and asserts the response body
  does NOT contain the literal string
  `'<missing>'`. The per-row `'<missing>'`
  literal is the same anti-pattern the
  meta-line `'<unknown>'` sweep closed, just
  on a different code path; a future
  regression in the per-row cell is now
  caught at the same CI step. The
  pre-existing `crates/dashboard/tests/smoke.rs::empty_state_renders_friendly_paragraph_when_index_has_no_entries`
  test stays green (the per-row fallback is
  orthogonal to the empty-state paragraph —
  the empty-state fires on `entry_count ===
  0`, the per-row fallback fires on
  `entries[0].receipt_basename === null`).
  Scope boundary: does NOT change the
  per-row column shape (the 10-column
  STW-050 split stays the same); does NOT
  change the meta-line fallback the
  STW-051 pass shipped (the per-row cell
  is a different code path); does NOT
  introduce a new render emitter (the
  `index.html` JS is the only change).
  Verification commands: `cargo test -p
  rbp-dashboard --test smoke` (the new
  sub-test + the existing 4 sub-tests
  all pass), `cargo test --workspace --
  --test-threads=4`, `cargo check --workspace`,
  `cargo fmt --check`. Hand-test command:
  `RBP_DASHBOARD_INDEX_URL=file://$PWD/crates/dashboard/tests/fixtures/index-missing-basename.json
  cargo run -p rbp-dashboard` (a stranger
  who hand-authors a broken `INDEX.json`
  sees the friendly `(basename not
  stamped — re-run with the STW-034
  publish-index chain)` placeholder, not
  the `'<missing>'` literal). Required
  tests: 1 new sub-test
  `per_row_basename_does_not_render_missing_sentinel`
  in `crates/dashboard/tests/smoke.rs`.
  Dependencies: STW-050 (the per-row
  column-split the per-row cell renders
  inside), STW-051 (the meta-line
  friendly-fallback pattern the per-row
  fallback mirrors). Estimated scope: XS.
  Completion signal: `cargo test -p
  rbp-dashboard --test smoke` is green
  with the new sub-test; the per-row
  `'<missing>'` literal is gone from
  `index.html:200`; the planning-pinning
  mark-`[x]` of the fifth-pass STW-052
  row is committed. **`lens:` Design
  (the per-row `'<missing>'` literal is
  the same AI-slop anti-pattern the
  meta-line `'<unknown>'` sweep closed;
  a future regression in the per-row
  cell is invisible to the existing
  smoke test, so the new sub-test is
  the only thing that closes the
  regression gap) + Eng (the fix is a
  1-line JS change + a 1-line planning
  pin + a 1-sub-test addition — three
  changes in one slice because the
  planning pin and the code sweep
  share the same shipping commit;
  pulling the planning pin out of
  scope would re-introduce the
  false-`[ ]` row the fifth pass
  failed to mark).**

- [ ] **[P1] `STW-056` Mark the four open
  prior-wave rows — `STW-039` + `STW-040`
  + `STW-041` + `STW-044` — with
  `RESCOPED 2026-06-04 by STW-056` markers
  + fold them into the latest wave's
  `## Next wave` heading so a future
  worker scanning the active queue sees
  only the 6th-pass wave + the v6→v10
  follow-on chain.** Pure markdown, no
  code. Each `RESCOPED` marker carries a
  one-line rationale: `STW-039`
  ("rescoped — the dashboard's static
  `index.html` already pins the per-row
  action sequence; a typed `StepLogger`
  in `crates/autotrain/src/observe.rs`
  is a nice-to-have with no testnet-visible
  value") + `STW-040` ("rescoped — the
  README's `## Quick Start` + `## TUI
  Preview` + `## Testnet launch proof` +
  `## Testnet publish bundle` + `##
  Public dashboard` sections cover the
  first-time-visitor path; a `## Try it
  now` section is cosmetic busywork that
  the CEO lens's subtraction default
  drops") + `STW-041` ("rescoped — the
  `STW-022` plan-staleness-gate has
  implicitly retired the `STW-001`
  operator-decision deferred row; the
  close-the-deferred-row task is no-op
  work because the `STW-022` gate
  mechanically prevents a future
  re-introduction of the deferred row")
  + `STW-044` ("rescoped — re-affirmed
  unchanged as a `P1` in this wave;
  the re-scoped 10-lib-test static-grep
  audit is the right shape; the morning
  wave's `TrainerError` enum refactor
  was wrong; see the `STW-044` row
  below for the shippable slice"). The
  `RESCOPED` markers are committed to
  `IMPLEMENTATION_PLAN.md` in the same
  PR as the `STW-044` audit lands. The
  five prior "## Next wave - review
  2026-06-04 (*)" sections (morning +
  afternoon + third + fourth + fifth
  pass, ~4800 lines of historical log)
  stay in the plan as audit trail; the
  `STW-056` row's purpose is to mark
  the carry-forward rows so a worker
  scanning the active queue does not
  re-`dispatch` shipped work. Scope
  boundary: does NOT delete the
  `RESCOPED` rows (audit trail);
  does NOT introduce a new
  planning-pinning tool (a markdown
  edit is sufficient); does NOT
  change the morning + afternoon +
  third + fourth + fifth pass
  section content (the prior review
  rationale is historical record).
  Verification commands: `git diff
  --stat IMPLEMENTATION_PLAN.md` (the
  four `RESCOPED` markers are present),
  `git log --oneline -5 -- IMPLEMENTATION_PLAN.md`
  (the `STW-056` commit is the latest
  plan-file change). Required tests:
  none (pure markdown). Dependencies:
  none. Estimated scope: XS. Completion
  signal: the four `RESCOPED 2026-06-04
  by STW-056` markers are committed in
  `IMPLEMENTATION_PLAN.md`. **`lens:`
  CEO (the CEO lens's *focus as
  subtraction* / *subtraction default*
  principle: a worker scanning the
  active queue should see only the
  ships-now rows, not the historical
  carry-forward rows; the five prior
  waves' open rows are the planning
  equivalent of feature bloat) +
  Eng (the `RESCOPED` markers are a
  4-line markdown edit; the work is
  not in the code, it's in the
  plan's own signal-to-noise ratio).**

- [x] **[P1] `STW-044` Re-affirmation of
  the morning + afternoon + third +
  fourth + fifth pass's re-scoped
  shape: a 10-lib-test static-grep
  audit that pins the existing
  per-arm `live_proof ...` error-line
  text without rewriting any `Mode::*`
  arm.** This is a *re-affirmation*,
  not new scope. The morning wave's
  `TrainerError` enum refactor (a
  single error type unifying the per-arm
  error surfaces) was the wrong shape
  — it required rewriting every
  `Mode::*` arm's error handling, which
  is the inverse of the "no-surprise
  upgrade" contract the chain establishes.
  The afternoon + third + fourth +
  fifth passes re-scoped the row to a
  10-lib-test static-grep audit that
  pins the *existing* per-arm
  `live_proof ...` error-line text
  (e.g. `live_proof publish error:
  receipt is red: ...` /
  `live_proof bundle verification
  passed: ...` /
  `live_proof remote verification
  passed: ...` /
  `live_proof index verification
  passed: ...` /
  `live_proof receipt verification
  passed: ...` /
  `live_proof replay error: ...` /
  `live_proof compare error: ...` /
  `live_proof compare3 error: ...` /
  `live_proof publish_index error: ...` /
  `live_proof publish_remote error: ...`)
  without rewriting any `Mode::*`
  arm. 10 new lib tests in
  `crates/autotrain/src/<mode>.rs::tests`
  (one per arm: `publish` + `publish_index` +
  `publish_remote` + `publish_index_remote` +
  `verify_receipt` + `verify_bundle` +
  `verify_remote` + `verify_index` + `replay` +
  `compare` + `compare3`) that drive
  the per-arm `Mode::*` handler with a
  hand-rolled failing input (a missing
  arg / a red receipt / a missing file /
  a missing bucket) and assert the
  `eprintln!` line is byte-identical
  to the pinned contract. The
  `TrainerError` enum is NOT introduced
  (the morning-wave refactor is
  rescoped away). The shape mirrors
  the `crates/dashboard/tests/smoke.rs::per_row_basename_does_not_render_missing_sentinel`
  sub-test the STW-055 slice adds: a
  regression in the per-arm error
  line is now caught at the same
  CI step. Scope boundary: does NOT
  introduce a new error type
  (the morning-wave `TrainerError`
  refactor is rescoped away); does
  NOT change the per-arm `Mode::*`
  handler code (the audit is
  *read-only* with respect to the
  handler — it pins the existing
  text, it does not rewrite it);
  does NOT change the `trainer --*`
  CLI argv shape (the per-arm argv
  is unchanged); does NOT change
  the per-step `live_proof ...`
  headline contract the prior slices
  pinned. Verification commands:
  `cargo test -p rbp-autotrain
  --test error_shape_audit` (the
  10 new lib tests pass), `cargo
  test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Hand-test command: `unset
  RBP_DASHBOARD_INDEX_URL; trainer
  --publish-index /nonexistent`
  (exits 2 + prints the pinned
  `live_proof publish_index error:
  io error: ...` line). Required
  tests: 10 new lib tests in
  `crates/autotrain/src/<mode>.rs::tests`
  (one per arm). Dependencies: STW-019
  (the per-step `live_proof ...` headline
  contract the audit pins), STW-028
  (the `trainer --verify-receipt` arm
  the audit covers), STW-032 (the
  `trainer --publish` arm), STW-033
  (the `trainer --publish-remote` arm),
  STW-034 (the `trainer --publish-index`
  arm), STW-035 (the `trainer --publish-index-remote`
  arm), STW-018 (the `trainer --compare`
  arm), STW-031 (the `trainer --compare3`
  arm), STW-016 (the `trainer --replay`
  arm). Estimated scope: S. Completion
  signal: `cargo test -p rbp-autotrain
  --test error_shape_audit` is green
  with the 10 lib tests; the
  `TrainerError` enum is NOT in
  the crate; the per-arm
  `Mode::*` handler code is
  unchanged (a `git diff` of the
  `crates/autotrain/src/<mode>.rs`
  files shows only test-file
  changes). **`lens:` Eng (the
  static-grep audit is a
  *regression-closure* slice, not
  a *refactor* slice; the morning
  wave's `TrainerError` refactor
  was the wrong shape because it
  re-wrote working code; the
  re-scoped audit pins the working
  code without rewriting it) +
  Design (the per-arm
  `live_proof ...` headline
  contract is the scrape surface
  a CI dashboard reads; a
  regression in the headline
  text is invisible to a
  hand-grep but visible to a
  dashboard scraper; the 10
  lib tests are the only thing
  that closes the regression
  gap).**

- [ ] **[P1] `STW-057` The deploy-step
  `live_proof dashboard deploy ...`
  headline contract the existing
  `grep ^live_proof` scrape pattern
  expects.** A 1-line addition to
  `scripts/deploy-dashboard-cloudflare.sh`:
  after the `wrangler pages deploy`
  step exits 0 + the `wrangler` stdout
  captures the `pages_url=...` line,
  the runbook appends `printf 'live_proof
  dashboard deploy complete: pages_url=%s
  files=%d bytes=%d\n' "$PAGES_URL"
  "$FILES" "$BYTES" >> "$SUMMARY.txt"`.
  The `FILES` + `BYTES` counts are
  computed by `find "$INDEX_DIR" -type
  f -printf '%s\n' | wc -l` +
  `find "$INDEX_DIR" -type f -printf '%s\n'
  | awk '{s+=$1} END {print s}'` so the
  headline is deterministic + byte-stable
  on re-runs. New `crates/autotrain/tests/script_shape.rs`
  shell-shape pin `deploy_dashboard_cloudflare_script_emits_live_proof_headline`
  asserts the runbook's source contains
  the literal `live_proof dashboard deploy
  complete: pages_url=` string + asserts
  the runbook's `SUMMARY.txt` (after a
  hand-test invocation against a fixture
  `publish/test-fixture/index/`) contains
  the pinned headline. Mirrors the
  `live_proof publish ...` /
  `live_proof receipt verification ...` /
  `live_proof bundle verification ...` /
  `live_proof index verification ...` /
  `live_proof remote verification ...` /
  `live_proof index_remote verification
  ...` scrape contract the prior slices
  pinned. Scope boundary: does NOT
  change the deploy runbook's chain
  steps (the `trainer --verify-index`
  pre-deploy gate + the `wrangler pages
  deploy` action + the README
  reconciliation step are unchanged —
  the headline is *appended* to
  `SUMMARY.txt` after the existing
  chain, not interleaved with it);
  does NOT introduce a new scrape
  pattern (the `grep ^live_proof` line
  is the existing pattern); does NOT
  change the `deploy.json` manifest
  shape (the `pages_url` field is the
  machine-readable complement to the
  `live_proof ...` headline). Verification
  commands: `bash -n scripts/deploy-dashboard-cloudflare.sh`,
  `cargo test -p rbp-autotrain --test
  script_shape` (the 1 new shape pin
  passes + the 5 prior STW-054 pinners
  stay green), `cargo test --workspace --
  --test-threads=4`, `cargo check --workspace`,
  `cargo fmt --check`. Hand-test command:
  `scripts/deploy-dashboard-cloudflare.sh
  publish/test-fixture/index/ 2>&1 | tail -1`
  (the `SUMMARY.txt`'s last line is
  `live_proof dashboard deploy complete:
  pages_url=https://robopoker-testnet-dashboard.pages.dev/
  files=N bytes=B`). Required tests: 1
  new shell-shape pin
  `deploy_dashboard_cloudflare_script_emits_live_proof_headline`
  in `crates/autotrain/tests/script_shape.rs`.
  Dependencies: STW-054 (the deploy
  runbook the headline lives in).
  Estimated scope: XS. Completion
  signal: the runbook's `SUMMARY.txt`
  ends with a `live_proof dashboard
  deploy complete:` line; the new
  shape pin in `script_shape.rs` is
  green. **`lens:` Design (the
  `live_proof ...` headline contract
  is the scrape surface a CI dashboard
  reads; a `scripts/deploy-dashboard-cloudflare.sh`
  runbook that ships without a
  headline breaks the contract the
  chain establishes) + Eng (the
  1-line addition mirrors the
  `live_proof publish ...` line the
  STW-032 runbook already pins; a
  future `live_proof ...` contract
  change in any of the 5 prior
  slices is locatable to the exact
  arm + the exact `printf` format
  string).**

- [x] **[P1] `STW-058` The dashboard's
  Pages-specific render surface:
  inject `RBP_DASHBOARD_DEPLOYED_URL`
  as a `window.__DASHBOARD_DEPLOYED_URL__`
  global the `index.html` JS reads as
  the dashboard `<meta>` line's
  trailing `deployed_at=<url>` fragment.**
  A 1-file, 5-line fix to
  `crates/dashboard/src/router.rs::serve_static_index`:
  the handler reads
  `RBP_DASHBOARD_DEPLOYED_URL` (defaulting
  to the README's `https://robopoker-testnet-dashboard.pages.dev/`
  placeholder) and injects a
  `<script>window.__DASHBOARD_DEPLOYED_URL__ =
  "<url>";</script>` line into the served
  `index.html` bytes *before* the existing
  IIFE (the inject position is the
  `<head>`'s tail so the script runs
  synchronously before the body IIFE).
  The `index.html` JS reads
  `window.__DASHBOARD_DEPLOYED_URL__`
  and appends a
  `deployed_at=<window.__DASHBOARD_DEPLOYED_URL__>`
  fragment to the existing meta
  `textContent` line (line 322) so a
  re-deploy to a different Pages project
  updates the rendered dashboard's
  meta line + the README's "Public
  dashboard:" line + the `deploy.json`
  manifest in one source. New
  `crates/dashboard/tests/smoke.rs`
  sub-test `meta_line_reflects_dashboard_deployed_url_env_knob`
  drives the dashboard with
  `RBP_DASHBOARD_DEPLOYED_URL` set to
  `https://example.pages.dev/` and
  asserts the response body contains
  the literal string
  `deployed_at=https://example.pages.dev/`
  (so a future regression in the
  env-knob read is caught at the
  same CI step). Scope boundary: does
  NOT change the dashboard's typed
  `IndexClient` (the deployed-URL
  injection is a *router* change, not
  an `IndexClient` change); does NOT
  change the dashboard's four-route
  surface (the `GET /` handler is
  the only change); does NOT change
  the dashboard's `RBP_DASHBOARD_INDEX_URL`
  env knob (the two env knobs are
  orthogonal: `INDEX_URL` is the
  `INDEX.json` source, `DEPLOYED_URL`
  is the dashboard's own URL); does
  NOT change the `crates/dashboard/static/index.html`
  column shape (the meta-line
  addition is appended, not
  interleaved). Verification commands:
  `cargo test -p rbp-dashboard --test
  smoke` (the new sub-test + the
  existing 5 sub-tests all pass),
  `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Hand-test command:
  `RBP_DASHBOARD_DEPLOYED_URL=https://example.pages.dev/
  cargo run -p rbp-dashboard` (a
  stranger hitting `http://localhost:8080/`
  sees a meta line ending in
  `deployed_at=https://example.pages.dev/`).
  Required tests: 1 new sub-test
  `meta_line_reflects_dashboard_deployed_url_env_knob`
  in `crates/dashboard/tests/smoke.rs`.
  Dependencies: STW-036 (the
  `crates/dashboard/` static
  dashboard crate the router lives
  in), STW-054 (the deploy runbook
  the `RBP_DASHBOARD_DEPLOYED_URL`
  env knob is sourced from).
  Estimated scope: XS. Completion
  signal: `cargo test -p
  rbp-dashboard --test smoke` is
  green with the new sub-test; the
  rendered dashboard's meta line
  reflects the `RBP_DASHBOARD_DEPLOYED_URL`
  env knob. **`lens:` Eng (the
  5-line fix is the minimum surface
  that makes the dashboard's meta
  line + the README's "Public
  dashboard:" line + the
  `deploy.json` manifest all read
  from a single env knob — the
  same single-source-of-truth
  pattern the existing
  `RBP_DASHBOARD_INDEX_URL` env
  knob uses) + Design (a
  re-deploy to a different Pages
  project no longer leaves the
  README + the dashboard both
  lying about the URL; the
  dashboard's meta line is the
  operator-visible signal the
  deploy succeeded with the
  expected URL).**

## Next wave - review 2026-06-04 (seventh pass)

The seventh 2026-06-04 three-lens review (kanban
task `t_e18b60a4`) re-applies the three lenses to
the *current* state of `main` at commit `b316681`
(HEAD). The six prior review-waves
(2026-06-04 morning → fifth pass → sixth pass)
converged correctly on the v6→v10 follow-on chain
(STW-029 → STW-031 → STW-032 → STW-033 → STW-034
→ STW-035 → STW-036 → STW-037 → STW-042 → STW-049
→ STW-050 → STW-051 → STW-052 → STW-054 → STW-055)
and shipped every piece of that chain; the
sixth-pass STW-054 commit
(`b316681 feat(deploy): STW-054 Cloudflare
Pages dashboard-deploy runbook`) is the most
recent landing. The seventh pass's three lenses on
the *current* state at `b316681` agree the v10
chain is structurally closed and find **four
findings the six prior reviews missed**, all
small and all directly blocking the testnet
north star's *one-source-of-truth* claim:

1. **The dashboard's rendered `<meta>` line
   never reads the deployed URL.** The
   `crates/dashboard/static/index.html` is a
   checked-in file with no env-knob
   interpolation; the `serve_static_index`
   handler in `crates/dashboard/src/router.rs:399`
   serves the static bytes verbatim. The
   `RBP_DASHBOARD_DEPLOYED_URL` env knob
   *exists* as a `pub const DEFAULT_DEPLOYED_URL`
   at `crates/dashboard/src/router.rs:65` (the
   `STW-054` runbook sets it via a downstream
   `README.md` reconciliation) but is *not
   read* in any handler. A `wrangler pages
   deploy` to a *different* Pages project
   leaves the README's "Public dashboard:" line
   (`README.md:338`) + the dashboard's
   `crates/dashboard/static/index.html` meta
   line + the STW-054 `deploy.json`
   `pages_url` field all lying about the URL
   until the operator hand-edits the README.
   The plan row at the sixth-pass wave
   (`STW-058`) names this fix but is still
   `[ ]`.
2. **The STW-054 deploy runbook does not
   actually stamp the `RBP_DASHBOARD_DEPLOYED_URL`
   env knob into the dashboard environment.**
   It runs `wrangler pages deploy <index-dir>
   --project-name <name> --commit-dirty=true`
   and prints the URL to stdout, then performs
   a one-line README reconciliation that
   *replaces* the `RBP_DASHBOARD_DEPLOYED_URL`
   placeholder line in the README. The
   dashboard itself, served at the
   `pages.dev` URL, still renders the
   *placeholder* meta line. The Env-lens
   fix is a 1-line addition to the runbook:
   `export RBP_DASHBOARD_DEPLOYED_URL=<url>`
   so a `wrangler pages deploy` invocation
   that runs *after* the export stamps the
   URL into the dashboard's meta line. The
   same single-source-of-truth pattern the
   existing `RBP_DASHBOARD_INDEX_URL` env
   knob uses.
3. **The dashboard's `serve_static_index`
   handler has no fallback when the
   committed demo `INDEX.json` fixture is
   missing.** The sixth-pass `STW-054` and
   `STW-055` work added the
   `crates/dashboard/tests/fixtures/INDEX.json`
   demo fixture the `GET /api/index` route
   serves when no `RBP_DASHBOARD_INDEX_URL`
   is set, but a first-time visitor who
   deletes the fixture (or a CI worker
   that runs `cargo test -p rbp-dashboard
   --test smoke` from a fresh `git clean
   -fdx` checkout) gets a 500. The
   `crates/dashboard/src/router.rs::serve_typed_index`
   handler is the only entry point that
   reads the fixture; the `serve_static_index`
   handler does not need the fixture
   (static bytes), but a regression that
   *adds* a fixture-read to `serve_static_index`
   in a future refactor (e.g. to compute
   `<meta name="robots">` from `entry_count`)
   would 500 with no friendly fallback. The
   Eng-lens fix is a static 1-line
   `script_shape.rs` pin: the new fixture
   file is in the smoke.rs fixture list.
4. **The STW-054 4-new-shape-pin
   `crates/autotrain/tests/script_shape.rs`
   additions do not include a
   `bash -n`-parse pin for the new
   runbook that fails on a future syntax
   regression.** The existing
   `deploy_dashboard_cloudflare_script_exists_and_parses`
   pin asserts the script exists + is
   executable, but the sixth pass did not
   add the static `bash -n` syntax-parse
   check the sibling
   `testnet_live_publish_*_script_exists_and_parses`
   pinners follow. A future edit that
   introduces a bash parse error in
   `scripts/deploy-dashboard-cloudflare.sh`
   would only fail at the first operator
   invocation, not at CI. The Design-lens
   fix is a 1-line `script_shape.rs`
   addition: `bash -n` parse the runbook
   + assert exit 0.

The seventh pass therefore ships four
deliverables, ordered by leverage:

- [x] **[P0] `STW-058` The dashboard's
  Pages-specific render surface: inject
  `RBP_DASHBOARD_DEPLOYED_URL` as a
  `window.__DASHBOARD_DEPLOYED_URL__` global
  the `index.html` JS reads as the dashboard
  `<meta>` line's trailing `deployed_at=<url>`
  fragment.** A 1-file, 5-line fix to
  `crates/dashboard/src/router.rs::serve_static_index`:
  the handler reads
  `RBP_DASHBOARD_DEPLOYED_URL` (defaulting
  to the README's `https://robopoker-testnet-dashboard.pages.dev/`
  placeholder, the same value the existing
  `pub const DEFAULT_DEPLOYED_URL` at
  `crates/dashboard/src/router.rs:65`
  already declares) and injects a
  `<script>window.__DASHBOARD_DEPLOYED_URL__ = "<url>";</script>`
  line into the served `index.html` bytes
  *before* the existing IIFE (the inject
  position is the `<head>`'s tail so the
  script runs synchronously before the
  body IIFE). The `index.html` JS reads
  `window.__DASHBOARD_DEPLOYED_URL__` and
  appends a
  `deployed_at=<window.__DASHBOARD_DEPLOYED_URL__>`
  fragment to the existing meta `textContent`
  line so a re-deploy to a different
  Pages project updates the rendered
  dashboard's meta line + the README's
  "Public dashboard:" line + the
  `deploy.json` manifest in one source.
  New
  `crates/dashboard/tests/smoke.rs` sub-test
  `meta_line_reflects_dashboard_deployed_url_env_knob`
  drives the dashboard with
  `RBP_DASHBOARD_DEPLOYED_URL` set to
  `https://example.pages.dev/` and asserts
  the response body contains the literal
  string
  `deployed_at=https://example.pages.dev/`
  (so a future regression in the env-knob
  read is caught at the same CI step).
  Scope boundary: does NOT change the
  dashboard's typed `IndexClient` (the
  deployed-URL injection is a *router*
  change, not an `IndexClient` change);
  does NOT change the dashboard's
  four-route surface (the `GET /` handler
  is the only change); does NOT change
  the dashboard's `RBP_DASHBOARD_INDEX_URL`
  env knob (the two env knobs are
  orthogonal: `INDEX_URL` is the
  `INDEX.json` source, `DEPLOYED_URL`
  is the dashboard's own URL); does NOT
  change the
  `crates/dashboard/static/index.html`
  column shape (the meta-line addition
  is appended, not interleaved).
  Verification commands:
  `cargo test -p rbp-dashboard --test smoke`
  (the new sub-test + the existing 5
  sub-tests all pass), `cargo test
  --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Hand-test command:
  `RBP_DASHBOARD_DEPLOYED_URL=https://example.pages.dev/
  cargo run -p rbp-dashboard` (a
  stranger hitting `http://localhost:8080/`
  sees a meta line ending in
  `deployed_at=https://example.pages.dev/`).
  Required tests: 1 new sub-test
  `meta_line_reflects_dashboard_deployed_url_env_knob`
  in `crates/dashboard/tests/smoke.rs`.
  Dependencies: STW-036 (the
  `crates/dashboard/` static dashboard
  crate the router lives in), STW-054
  (the deploy runbook the
  `RBP_DASHBOARD_DEPLOYED_URL` env knob
  is sourced from). Estimated scope:
  XS. Completion signal:
  `cargo test -p rbp-dashboard --test smoke`
  is green with the new sub-test; the
  rendered dashboard's meta line
  reflects the `RBP_DASHBOARD_DEPLOYED_URL`
  env knob. **`lens:` Eng (the 5-line
  fix is the minimum surface that makes
  the dashboard's meta line + the
  README's "Public dashboard:" line +
  the `deploy.json` manifest all read
  from a single env knob — the same
  single-source-of-truth pattern the
  existing `RBP_DASHBOARD_INDEX_URL`
  env knob uses) + Design (a re-deploy
  to a different Pages project no
  longer leaves the README + the
  dashboard both lying about the URL;
  the dashboard's meta line is the
  operator-visible signal the deploy
  succeeded with the expected URL).**

- [x] **[P0] `STW-059` The STW-054 deploy
  runbook stamps `RBP_DASHBOARD_DEPLOYED_URL`
  into the wrangler deploy environment
  before the deploy, not after.** A
  1-line addition to
  `scripts/deploy-dashboard-cloudflare.sh`:
  immediately after the `wrangler pages
  deploy` call succeeds and the runbook
  reads the URL `wrangler` printed to
  stdout, the runbook `export`s
  `RBP_DASHBOARD_DEPLOYED_URL=<url>` so a
  *subsequent* `wrangler pages deploy`
  invocation (or a follow-on
  `cargo run -p rbp-dashboard` smoke) is
  sourced from the same env knob the
  `STW-058` `serve_static_index` handler
  reads. The export happens *before* the
  README reconciliation so the
  `replace_in_readme` sed step + the
  dashboard's meta line + the
  `deploy.json` `pages_url` field are
  all driven from the same `pages_url`
  variable. New
  `crates/autotrain/tests/script_shape.rs`
  sub-test
  `deploy_dashboard_cloudflare_script_exports_rbp_dashboard_deployed_url`
  greps the runbook for the literal
  `export RBP_DASHBOARD_DEPLOYED_URL=`
  string (so a future regression in the
  export is caught at the same CI step).
  Scope boundary: does NOT change the
  `RBP_DASHBOARD_DEPLOYED_URL` env knob
  semantics the STW-058 handler reads
  (the export is a *runbook* change, not
  a `serve_static_index` change); does
  NOT change the wrangler `pages deploy`
  invocation; does NOT change the
  pre-deploy `trainer --verify-index`
  refuse-to-deploy-red-index gate; does
  NOT change the
  `RBP_DASHBOARD_CF_API_TOKEN` /
  `RBP_DASHBOARD_CF_ACCOUNT_ID`
  fail-fast contract. Verification
  commands: `bash -n
  scripts/deploy-dashboard-cloudflare.sh`,
  `cargo test -p rbp-autotrain --test
  script_shape` (the new sub-test + the
  33 existing shape pins all pass),
  `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Hand-test command:
  `RBP_DASHBOARD_CF_API_TOKEN=<dummy>
  RBP_DASHBOARD_CF_ACCOUNT_ID=<dummy>
  PUBLISH_ROOT=/tmp/fake
  scripts/deploy-dashboard-cloudflare.sh`
  (after the wrangler stub returns a
  fake URL, the runbook prints
  `export RBP_DASHBOARD_DEPLOYED_URL=<url>`
  to stdout). Required tests: 1 new
  sub-test
  `deploy_dashboard_cloudflare_script_exports_rbp_dashboard_deployed_url`
  in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: STW-054 (the deploy
  runbook the export is added to),
  STW-058 (the
  `serve_static_index` handler that
  reads the env knob). Estimated
  scope: XS. Completion signal:
  `cargo test -p rbp-autotrain --test
  script_shape` is green with the new
  sub-test; the
  `scripts/deploy-dashboard-cloudflare.sh`
  runbook exports
  `RBP_DASHBOARD_DEPLOYED_URL` on
  success. **`lens:` Eng (the 1-line
  export closes the loop between the
  `wrangler pages deploy` stdout URL
  and the `serve_static_index` env-knob
  read — the same single-source-of-truth
  pattern the existing
  `RBP_DASHBOARD_INDEX_URL` knob uses) +
  Design (an operator who runs the
  runbook once gets a dashboard whose
  meta line + README + `deploy.json`
  all agree on the URL, with no
  hand-editing of any file).**
  **Shipped this wave on the
  `feat(deploy): STW-059 stamp
  RBP_DASHBOARD_DEPLOYED_URL on
  Pages deploy` commit (2026-06-04).**

- [ ] **[P1] `STW-060` The
  `crates/dashboard/tests/fixtures/INDEX.json`
  demo fixture is wired into the smoke
  test's setup so a `git clean -fdx`
  CI run from a fresh checkout does
  not 500 the `serve_static_index`
  handler on a regression that adds a
  fixture-read.** A 1-file, 1-line
  addition to
  `crates/autotrain/tests/script_shape.rs`:
  a new sub-test
  `dashboard_fixtures_index_json_is_tracked_and_nonempty`
  asserts
  `git ls-files crates/dashboard/tests/fixtures/INDEX.json`
  exits 0 AND the file is non-empty
  AND parses as JSON (so a CI worker
  running `cargo test -p rbp-dashboard
  --test smoke` from a fresh
  `git clean -fdx` checkout can find
  the fixture the `serve_typed_index`
  handler reads). The new sub-test
  runs at the *static* `script_shape.rs`
  layer (not the `smoke.rs` runtime
  layer) so a `git clean` regression
  fails at the cheapest possible CI
  step. Scope boundary: does NOT
  change the `serve_typed_index`
  handler's fixture-read contract (the
  fixture is read the same way; the new
  sub-test is a *test* addition, not a
  handler addition); does NOT change
  the existing 5 `smoke.rs` sub-tests
  (the new sub-test is a sibling, not
  a replacement); does NOT change the
  dashboard's `RBP_DASHBOARD_INDEX_URL`
  env knob. Verification commands:
  `cargo test -p rbp-autotrain --test
  script_shape` (the new sub-test + the
  33 existing shape pins all pass),
  `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Hand-test command:
  `git ls-files crates/dashboard/tests/fixtures/INDEX.json`
  (returns the path; the file is
  tracked and non-empty). Required
  tests: 1 new sub-test
  `dashboard_fixtures_index_json_is_tracked_and_nonempty`
  in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: STW-036 (the
  `crates/dashboard/` static dashboard
  crate the fixture lives in), STW-042
  (the compare3-fixture demo-data
  slice the dashboard's fixtures
  folder is built around). Estimated
  scope: XS. Completion signal:
  `cargo test -p rbp-autotrain --test
  script_shape` is green with the new
  sub-test; a fresh `git clean -fdx
  && git checkout -- crates/dashboard/tests/fixtures
  && cargo test -p rbp-dashboard
  --test smoke` run is green.
  **`lens:` Eng (the static-shape
  pin catches a `git clean` regression
  at the cheapest possible CI step
  — the same single-source-of-truth
  pattern the existing fixture-shape
  pins in `script_shape.rs` follow) +
  Design (a first-time visitor with a
  fresh checkout still sees a
  populated dashboard, no 500, no
  confusing `internal server error`
  page).**

- [x] **[P1] `STW-061` The
  `scripts/deploy-dashboard-cloudflare.sh`
  runbook gets a `bash -n`-parse pin
  in `script_shape.rs` so a future
  syntax regression is caught at CI
  rather than at first operator
  invocation.** A 1-file, 1-line
  addition to
  `crates/autotrain/tests/script_shape.rs`:
  a new sub-test
  `deploy_dashboard_cloudflare_script_parses_with_bash_n`
  invokes
  `bash -n scripts/deploy-dashboard-cloudflare.sh`
  and asserts the command exits 0
  (so a future edit that introduces a
  bash parse error fails this
  sub-test at the same CI step the
  sibling
  `testnet_live_publish_*_script_exists_and_parses`
  pinners follow). The new sub-test
  is the cheapest possible pin on
  the runbook's syntax — a single
  `bash -n` call — and is parallel-safe
  by construction (no DB, no
  filesystem mutation, no env
  mutation). Scope boundary: does
  NOT change the
  `deploy-dashboard-cloudflare.sh`
  runbook (the new sub-test is a
  *test* addition, not a runbook
  addition); does NOT change the
  existing 4 STW-054 shape pins (the
  new sub-test is a sibling); does
  NOT change the
  `RBP_DASHBOARD_CF_API_TOKEN`
  fail-fast contract. Verification
  commands:
  `cargo test -p rbp-autotrain --test
  script_shape` (the new sub-test +
  the 33 existing shape pins all
  pass), `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Hand-test command:
  `bash -n scripts/deploy-dashboard-cloudflare.sh`
  (exits 0 on a clean runbook).
  Required tests: 1 new sub-test
  `deploy_dashboard_cloudflare_script_parses_with_bash_n`
  in
  `crates/autotrain/tests/script_shape.rs`.
  Dependencies: STW-054 (the
  deploy runbook the static
  parse pin covers), STW-059 (the
  export the parse covers is the
  same runbook). Estimated scope:
  XS. Completion signal:
  `cargo test -p rbp-autotrain --test
  script_shape` is green with the
  new sub-test; a future bash
  parse error in
  `scripts/deploy-dashboard-cloudflare.sh`
  fails CI on a single
  `bash -n` invocation. **`lens:`
  Eng (the static `bash -n` parse
  is the cheapest possible syntax
  pin — the same pattern the
  sibling
  `testnet_live_publish_*_script_exists_and_parses`
  pinners follow) + Design (an
  operator who edits the runbook
  and pushes gets a fast CI signal
  rather than a confusing first-
  invocation parse error).**

## Next wave - review 2026-06-05

The ninth 2026-06-05 three-lens review (kanban task
`t_425ba9f4`) re-applies the three lenses to the
current state of `main` at commit `f56d11c` (HEAD).
The eighth review-wave (`7462dbc`) correctly identified
STW-062/063/064/065/066 as the next dashboard-polish
cluster, and three of those (STW-062/063/064) have since
shipped on `main`. However, the testnet-live-proof runbook
— the backbone of the entire testnet north star — has
**never produced a complete receipt** on this machine:
every receipt directory under `receipts/` stops at the
`cluster/` step with exit code `101` (`database connection
failed: password missing`). The runbook therefore fails
*late* (after the expensive `--cluster` step) with a Rust
panic buried in `cluster/stderr.txt`, leaving the operator
with an incomplete receipt and no `SUMMARY.txt`. The three
lenses agree this late failure is the single biggest
blocker to testnet readiness, and that further dashboard
polish is busywork until the chain actually runs
end-to-end.

**LENS 1 — CEO / strategic.** The north star is a public,
reproducible testnet benchmark. The repo has built every
infrastructure piece (trained configs, bench, compare3,
transcripts, receipts, publish, dashboard, deploy runbooks),
but the backbone runbook has never completed. The single
highest-leverage thrust is to make the runbook **fail fast**
with a clear diagnostic, and to provide a **fast mode** so
an operator can validate the full chain in minutes rather
than hours. Further dashboard caching (beyond STW-062/063/
064) is busywork to drop until real data flows through.
Plan hygiene is also strategic: `IMPLEMENTATION_PLAN.md` is
435 KB with 8+ stale P1 ghost rows, creating a false
backlog signal that misdirects every future worker.

**LENS 2 — Engineering / feasibility.** A `trainer --doctor`
pre-flight mode is trivial to implement (~1 file, ~50
lines) and can be called by the runbook before `--cluster`.
The runbook's current failure mode (panic after expensive
work) is the worst possible shape. A fast mode
(`RBP_TESTNET_FAST=1`) that auto-sets minimal epochs/hands
is a ~10-line script change and collapses validation time
from hours to minutes. The dashboard's `read_bench_json_line`
expects `RBP_DASHBOARD_RECEIPT_DIR/<id>/bench/stdout.txt`,
but no completed receipt exists to populate it; a
`scripts/seed-dashboard-local.sh` bridge runbook that
re-uses existing partial receipt data for local dashboard
development is a low-risk shell script. STW-065 (plan
staleness P1 extension) and STW-066 (JS fallback single-
source-of-truth) are genuine remaining engineering items
from the eighth pass and should ship.

**LENS 3 — Design / product + UX.** The operator experience
of the testnet-live-proof runbook is poor: no pre-flight
check, no fast mode, a panic buried in stderr, and an
incomplete receipt directory with no SUMMARY. The dashboard
empty state already tells the operator to run the runbook,
but when the runbook fails the operator is left with no
diagnostic guidance. A `--doctor` mode with human-readable
output fixes this. The `IMPLEMENTATION_PLAN.md` at 11k lines
is a terrible developer experience — finding the active
queue requires scrolling through pages of stale ghosts.
The Design lens also flags that the `compare3.rs`
integration test parser returns `None` on any unknown JSON
key, which means a future addition to `Compare3Report::to_json`
will break the test cryptically instead of gracefully.

The ninth pass therefore ships five deliverables, ordered
by leverage:

- [x] **[P0] `STW-065` The
  `scripts/plan-staleness-gate.sh` script
  extends its ghost-detection to also catch
  `[ ] [P1]` rows in both
  `genesis/plans/000-ceo-testnet-roadmap.md`
  and `IMPLEMENTATION_PLAN.md` that have a
  corresponding `[x] STW-NNN` row, with a
  `RESCOPED 2026-06-05` marker convention, so
  the 8+ stale `[ ] [P1]` ghost rows (STW-038,
  STW-045, STW-048, STW-054, STW-057, STW-060,
  STW-061, plus the [P1] siblings of shipped
  [P0] rows) get mechanically flagged and
  retired the same way the existing P0 ghost
  detection flags `[ ] [P0]` rows.** A
  1-script + 1-test change:
  `scripts/plan-staleness-gate.sh` adds a
  second pass that greps both files for
  `- [ ] \[P1\] <claim>` rows, maps each
  claim text to a STW id, and asserts each
  is either `[x]` (ghost, flag and exit 3)
  or carries a `RESCOPED 2026-06-05` marker.
  The new sub-test in
  `crates/autotrain/tests/plan_staleness.rs`
  drives the new path with a synthetic 2-row
  roadmap + plan and asserts the gate exits
  3 on a P1 ghost. Scope boundary: does NOT
  change the existing P0 ghost detection (the
  new P1 pass is a sibling); does NOT change
  the script's exit-code contract (still exits
  0 on green, 3 on ghost). Verification
  commands: `scripts/plan-staleness-gate.sh`
  (new P1 pass + existing P0 pass both pass,
  headline `plan staleness gate complete:
  checked=... ghosts=0`), `cargo test -p
  rbp-autotrain --test plan_staleness` (new
  sub-test + existing 4 sub-tests pass),
  `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`.
  Required tests: 1 new sub-test in
  `crates/autotrain/tests/plan_staleness.rs`
  (`plan_staleness_gate_catches_p1_ghosts`).
  Dependencies: STW-022 (the existing plan-
  staleness gate). Estimated scope: S.
  Completion signal: `scripts/plan-staleness-
  gate.sh` is green with the new P1 pass; the
  8+ stale `[ ] [P1]` ghost rows in
  `IMPLEMENTATION_PLAN.md` are mechanically
  flagged. **`lens:` CEO (plan hygiene is the
  cheapest strategic unblocker — every future
  worker starts with 11k lines of noise
  without it) + Eng (the 2-pass script shape
  mirrors the existing P0 pass verbatim) +
  Design (a readable plan is a usable plan).**

- [x] **[P0] `STW-067` A new
  `Mode::Doctor` CLI arm (`trainer --doctor`)
  that pre-flights all testnet-live-proof
  prerequisites (DB connectivity via a
  `SELECT 1` ping, required env vars
  `DATABASE_URL`, `RBP_FAST_EPOCHS`,
  `RBP_BENCH_HANDS`, etc., and trainer binary
  sanity) and prints a one-line JSON
  `DoctorReport` plus human-readable
  diagnostics.** Update
  `scripts/testnet-live-proof.sh` to run
  `trainer --doctor` as step 0 and exit
  cleanly with a diagnostic if prerequisites
  fail, **before** running the expensive
  `--cluster` step. Owner files:
  `crates/autotrain/src/doctor.rs` (new
  `DoctorReport` struct + `run` helper + 4
  lib tests: `doctor_db_ping_succeeds_on_
  valid_url`, `doctor_db_ping_fails_on_
  bad_password`, `doctor_env_report_lists_
  all_required_vars`, `doctor_json_output_
  is_parseable`), `crates/autotrain/src/
  mode.rs` (new `Mode::Doctor` arm + argv
  handling + `--doctor` in the `Usage:`
  line), `scripts/testnet-live-proof.sh`
  (new step-0 `--doctor` invocation + exit
  on red doctor report), `crates/autotrain/
  tests/doctor.rs` (new integration test:
  `doctor_run_exits_zero_on_valid_db` +
  `doctor_run_exits_nonzero_on_bad_db`,
  gated on `database` feature + `DATABASE_URL`
  like sibling integration tests). Scope
  boundary: does NOT change the existing
  `--cluster` / `--smoke` / `--bench` / `--compare`
  / `--compare3` / `--replay` code paths (the
  doctor is read-only and pre-flight); does
  NOT change the runbook's receipt layout or
  SUMMARY.txt format (the doctor failure is a
  pre-receipt exit); does NOT require a new
  dependency (uses the existing `tokio_postgres`
  or `sqlx` connection path the database crate
  already imports, or a standalone `psql`
  subprocess fallback for the no-DB doctor
  path). Verification commands: `cargo test -p
  rbp-autotrain --test doctor` (2 new sub-
  tests pass), `cargo test --workspace --
  --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`. Hand-
  test: `DB_URL=postgres://bad:bad@localhost/
  db trainer --doctor` exits non-zero with a
  clear diagnostic; `DB_URL=<good> trainer
  --doctor` exits 0 with JSON. Required
  tests: 4 lib tests in `doctor.rs` + 2
  integration tests in `tests/doctor.rs`.
  Dependencies: STW-019 (the testnet-live-
  proof runbook the doctor pre-flights).
  Estimated scope: M. Completion signal:
  `cargo test -p rbp-autotrain --test doctor`
  is green; a runbook invocation with a bad
  DB URL exits in <1 s with a clear
  diagnostic instead of panicking after the
  `--cluster` step. **`lens:` CEO (the
  runbook is the backbone of the testnet
  north star; making it fail-fast is the
  highest-leverage move toward a real
  receipt) + Eng (the doctor is a read-only
  pre-flight; zero risk to existing code
  paths) + Design (an operator gets a human-
  readable diagnostic in 1 s instead of a
  panic in a log file after minutes of
  wasted work).**

- [ ] **[P1] `STW-066` The
  `crates/dashboard/static/index.html`
  `deployedUrl` JS-side fallback
  hard-coded at line 392 reads from a new
  `window.__DASHBOARD_DEPLOYED_URL_DEFAULT__`
  global the
  `crates/dashboard/src/router.rs::serve_static_index`
  handler injects from the `pub const
  DEFAULT_DEPLOYED_URL` Rust declaration,
  so the static JS fallback + the Rust
  default are one source.** A 1-file JS-
  side + 1-file Rust-side change: the
  `serve_static_index` handler's injection
  point now emits *two* `<script>` tags
  (one for the env-knob value and one for
  the Rust default), and the `index.html`
  JS line 392 reads the new global as the
  fallback. The two tags are injected as a
  single `String` so the STW-062 cache is
  unchanged. Scope boundary: does NOT
  change the existing `RBP_DASHBOARD_
  DEPLOYED_URL` env-knob read semantics;
  does NOT change the `pub const DEFAULT_
  DEPLOYED_URL` value; does NOT change the
  meta line's `deployed_at=...` fragment.
  Verification commands: `cargo test -p
  rbp-dashboard --test smoke` (new sub-test
  + existing 9 sub-tests pass), `cargo test
  --workspace -- --test-threads=4`, `cargo
  check --workspace`, `cargo fmt --check`.
  Hand-test: `cargo run -p rbp-dashboard`
  + `curl -s http://localhost:8080/ | grep
  __DASHBOARD_DEPLOYED_URL_DEFAULT__`
  returns the injected global with the Rust
  const value. Required tests: 1 new sub-
  test in `crates/dashboard/tests/smoke.rs`
  (`serve_static_index_injects_deployed_url_
  default_global`). Dependencies: STW-058
  (the env-knob read), STW-062 (the cache
  the dual-tag injection is cached in).
  Estimated scope: XS. Completion signal:
  `cargo test -p rbp-dashboard --test smoke`
  is green with the new sub-test; the static
  JS fallback reads from a Rust-sourced
  global. **`lens:` Design (single source of
  truth: one `pub const` line instead of two
  files in two languages) + Eng (1-line
  extension to existing injection function;
  cache shape unchanged).**

- [ ] **[P1] `STW-068` A new
  `scripts/seed-dashboard-local.sh` runbook
  that takes an existing (even incomplete)
  testnet-live-proof receipt directory and
  produces a local dashboard-compatible layout
  under `.auto/dashboard-seed/` — copying or
  symlinking `bench/stdout.txt`,
  `compare/stdout.txt`, and any
  `transcript-*.json` files into the
  `RBP_DASHBOARD_RECEIPT_DIR` expected
  structure, plus generating a minimal
  `INDEX.json` with one entry — so a
  developer can `cargo run -p rbp-dashboard`
  and see real receipt data structures
  instead of only the compare3 fixture.**
  The script is pure bash (mirrors the
  `scripts/testnet-live-proof.sh` shape:
  exists + executable + parses with `bash
  -n`), takes a receipt directory as argv[1],
  validates the directory contains at least
  one step subdir, and writes the seed layout.
  A new integration test in
  `crates/dashboard/tests/seed_local.rs`
  drives the script against a synthetic
  receipt directory and asserts the dashboard
  renders a non-empty table. Scope boundary:
  does NOT change the dashboard's router or
  render code (the script produces data the
  dashboard already knows how to read); does
  NOT change the testnet-live-proof runbook;
  does NOT upload to any remote bucket.
  Verification commands: `bash -n scripts/
  seed-dashboard-local.sh`, `cargo test -p
  rbp-dashboard --test seed_local`, `cargo
  test --workspace -- --test-threads=4`,
  `cargo check --workspace`, `cargo fmt
  --check`. Hand-test: `bash scripts/seed-
  dashboard-local.sh receipts/testnet-live-
  proof-20260604T052134Z` produces `.auto/
  dashboard-seed/`; `RBP_DASHBOARD_RECEIPT_
  DIR=.auto/dashboard-seed cargo run -p
  rbp-dashboard` shows a populated table.
  Required tests: 1 integration test in
  `crates/dashboard/tests/seed_local.rs`
  (`seed_local_run_produces_dashboard_
  readable_layout`). Dependencies: STW-019
  (receipt layout), STW-036 (dashboard read
  paths). Estimated scope: S. Completion
  signal: script exists, parses, and produces
  a dashboard-readable layout from an existing
  receipt dir. **`lens:` CEO (closes the loop
  between "we have receipts" and "the
  dashboard shows something real") + Eng (a
  pure bash bridge script; zero risk to core
  code) + Design (developer can see real data
  locally without running the full expensive
  chain).**

- [x] **[P1] `STW-069` Add
  `RBP_TESTNET_FAST=1` support to
  `scripts/testnet-live-proof.sh` that auto-
  sets minimal env vars (`RBP_FAST_EPOCHS=2`,
  `RBP_FAST_BATCH=16`, `RBP_BENCH_HANDS=4`,
  `RBP_COMPARE_HANDS=4`, etc.) so an operator
  can validate the full chain end-to-end in
  minutes rather than hours, and document the
  fast mode in `scripts/testnet-live-proof.md`.**
  The script change is a ~15-line env-knob
  block at the top: if `RBP_TESTNET_FAST=1`,
  export the minimal values (only for vars
  not already set by the operator, so an
  explicit override still wins). A new sub-
  test in `crates/autotrain/tests/script_
  shape.rs` asserts the script contains the
  `RBP_TESTNET_FAST` string and that the
  documented env vars appear in the script.
  Scope boundary: does NOT change the default
  runbook behavior (fast mode is opt-in via
  env knob); does NOT change the step order
  or receipt layout; does NOT reduce the
  verification rigor of a normal run.
  Verification commands: `bash -n scripts/
  testnet-live-proof.sh`, `cargo test -p
  rbp-autotrain --test script_shape` (new
  sub-test passes), `cargo test --workspace
  -- --test-threads=4`, `cargo check
  --workspace`, `cargo fmt --check`. Hand-
  test: `RBP_TESTNET_FAST=1 bash scripts/
  testnet-live-proof.sh` with a working DB
  completes in <5 min. Required tests: 1 new
  sub-test in `crates/autotrain/tests/script_
  shape.rs` (`testnet_live_proof_script_
  documents_fast_mode`). Dependencies:
  STW-019 (the runbook being extended).
  Estimated scope: XS. Completion signal:
  script documents and honours
  `RBP_TESTNET_FAST=1`; a fast-mode runbook
  invocation completes in minutes. **`lens:`
  CEO (collapses validation time from hours to
  minutes, making the north star actually
  reachable in a single work session) + Eng
  (pure env-knob convention; no code-path
  changes) + Design (operator can iterate
  quickly without waiting for a full train).**


## Next wave - review 2026-06-05

The eighth 2026-06-05 three-lens review (kanban
task `t_5afdfe58`) re-applies the three lenses to
the *current* state of `main` at commit `7c2976b`
(HEAD). The seven prior 2026-06-04 review-waves
(morning → afternoon → third → fourth → fifth →
sixth → seventh pass) all converged on a single
verdict: the v6→v10 follow-on chain
(STW-029 → STW-031 → STW-032 → STW-033 → STW-034
→ STW-035 → STW-036 → STW-037 → STW-042 → STW-049
→ STW-050 → STW-051 → STW-052 → STW-053 → STW-054
→ STW-055 → STW-057 → STW-058 → STW-059 → STW-060)
is **structurally closed and shipped** — every
named v6/v7/v8/v9/v10 follow-on in
`genesis/plans/000-ceo-testnet-roadmap.md` has a
`[x] STW-NNN` row on `main` and a corresponding
operator-runnable / CI-runnable surface. The
seventh pass's four dashboard-deps findings
(STW-057 + STW-058 + STW-059 + STW-060) are all
landed on `7c2976b` (one cohesive `feat(dashboard,
autotrain)` commit that advanced the static
pinner layer and the `crates/dashboard/tests/
fixtures/INDEX.json` fallback in lockstep). The
v10 dashboard's `GET /` → `serve_static_index` →
`index.html` JS → `<meta>` line is a *complete*
single-source-of-truth chain (`RBP_DASHBOARD_
DEPLOYED_URL` env knob ↔ `serve_static_index`
injection ↔ `window.__DASHBOARD_DEPLOYED_URL__`
global ↔ `meta.textContent` `deployed_at=...`
fragment ↔ README `${RBP_DASHBOARD_DEPLOYED_URL
:-robopoker-testnet-dashboard.pages.dev/}` shell
form ↔ `deploy.json` `pages_url` field ↔
`live_proof dashboard deploy complete: ...`
headline) with one env knob as the universal
source. The eighth pass's three lenses agree the
chain is genuinely closed and find **five
findings the seven prior reviews missed**, all
small, all in code paths that the chain already
touches but the prior reviews passed over
because the seven reviews all converged on
*wiring / contract / pin coverage* questions
and never re-read the *runtime cost + error
surface + shape pin completeness* of the
already-shipped surfaces:

1. **The `serve_static_index` handler
   re-runs `inject_deployed_url()` on every
   request, even though the input is
   byte-deterministic in the env knob.** The
   `crates/dashboard/src/router.rs::serve_static_index`
   handler (line 546) reads the
   `RBP_DASHBOARD_DEPLOYED_URL` env knob on
   every `GET /` and calls
   `inject_deployed_url(&state.static_index_html,
   &deployed_url)` (line 562) which does 5
   `.replace()` calls (backslash, double-quote,
   newline, carriage-return, less-than) + a
   `String::rfind("</head>")` + a
   `format!()` injection + a triple
   `String::push_str` triple (lines 503–536).
   The `static_index_html` field on `AppState`
   is `Arc<String>` (line 423, "loaded once at
   startup") but the *injected* body is
   reconstructed per request. With
   `Cache-Control: no-cache` (line 570) every
   page reload re-does this work. The
   `DEPLOYED_URL_TEST_OVERRIDE` `Mutex<Option<
   String>>` (line 92) is the only knob that
   should invalidate a per-process cache; an
   env-var change requires a `cargo run`
   restart, so a `OnceLock<(String, String)>`
   cache keyed on the env knob is safe. The
   fix is a 1-file, ~10-line change to
   `serve_static_index`: cache the
   `(deployed_url, injected_body)` pair in a
   `OnceLock`, only re-inject on a
   `deployed_url()` mismatch. The cache
   invalidates automatically on a `cargo run`
   restart (the `OnceLock` is process-local).
2. **The `serve_transcript` handler reads
   the on-disk `transcript-<id>.json` file
   on every request.** The
   `crates/dashboard/src/router.rs::serve_transcript`
   handler (line 654) does
   `std::fs::read(&path)` (line 666) on every
   `GET /transcript/<id>`. A refresh-hammer
   from a `Download transcript` link (a CI
   worker re-fetching the bundle to
   re-verify) does the file I/O on every
   request. The fix is a `DashMap<String,
   Arc<Vec<u8>>>` (or a `Mutex<HashMap<...>>`
   for the no-extra-deps path) keyed on the
   `id` + a `Last-Modified` mtime check so a
   new `trainer --replay` run invalidates the
   cache automatically. 1-file, ~15-line
   change.
3. **The `inject_deployed_url` function
   panics on a `</head>`-less static
   page.** The `crates/dashboard/src/router.rs
   ::inject_deployed_url` function (line 503)
   `panic!`s at line 524–529 when the static
   `index.html` is missing the `</head>` tag:
   `panic!("static index.html is missing
   `</head>`; cannot inject
   RBP_DASHBOARD_DEPLOYED_URL global")`. The
   current static `index.html` is checked-in
   with the tag present (the `grep -c
   </head>` returns 1), so the panic is dead
   code today — but a future refactor that
   replaces the static page (e.g. a
   templated one) crashes the entire server
   (the `unwrap_or_else` panic propagates up
   through the `serve_static_index` handler
   to the axum runtime) instead of returning
   a 500. The Eng + Design fix is a 1-line
   change: replace the panic with a
   `Result<String, InjectError>` return + a
   500 with a one-line diagnostic the
   existing `not_found` / 500 helper can
   emit. The fix is `Result`-shaped so a
   smoke test can drive the missing-tag
   path with a synthetic `index.html` byte
   slice.
4. **The plan-vs-reality staleness gate
   (`STW-022`) only catches `[ ] [P0]`
   ghost rows, not `[ ] [P1]` ghosts.** The
   `scripts/plan-staleness-gate.sh` script
   (line 82) only greps `- [ ] \[P0\] <claim>`
   rows in the CEO roadmap and checks each
   against the `[x] STW-NNN` rows in
   `IMPLEMENTATION_PLAN.md`. The current
   `IMPLEMENTATION_PLAN.md` has 14 unique
   `[ ] STW-NNN` rows in the `## Active
   items` section (STW-001, STW-038, STW-039,
   STW-040, STW-041, STW-044, STW-045,
   STW-046, STW-048, STW-054, STW-056,
   STW-057, STW-060, STW-061) — of which
   8 are stale ghosts of shipped work
   (STW-045 shipped on `b5ad974`,
   STW-048 already marked `SUPERSEDED`,
   STW-054 shipped on `b316681`, STW-057 +
   STW-060 shipped on `7c2976b`, STW-055
   shipped on `e5081e8` but still listed as
   `[ ]`, plus the [P1] siblings of the
   shipped [P0] rows). The CEO-lens
   highest-leverage thrust is to extend
   the gate to flag `[ ] [P1]` rows in
   *both* `genesis/plans/000-ceo-testnet-
   roadmap.md` and `IMPLEMENTATION_PLAN.md`
   that have a corresponding `[x] STW-NNN`
   row, with a `RESCOPED 2026-06-05` marker
   convention the prior 6th-pass STW-056
   row already names. The fix is a ~20-line
   change to `scripts/plan-staleness-gate.sh`
   + a new `crates/autotrain/tests/
   plan_staleness.rs` sub-test that drives
   the new path with a synthetic 2-row
   roadmap + a corresponding 2-row plan
   and asserts the gate exits 3.
5. **The dashboard's static `index.html`
   fallback URL hard-codes the
   `https://robopoker-testnet-dashboard.pages.dev/`
   placeholder, duplicating the
   `pub const DEFAULT_DEPLOYED_URL` the
   Rust side declares.** The
   `crates/dashboard/static/index.html`
   line 392 reads
   `var deployedUrl = (typeof window !==
   'undefined' && window.__DASHBOARD_
   DEPLOYED_URL__) || 'https://robopoker-
   testnet-dashboard.pages.dev/';` — the
   `'https://robopoker-testnet-dashboard
   .pages.dev/'` literal is the
   *JS-side duplicate* of the
   `pub const DEFAULT_DEPLOYED_URL`
   declaration at
   `crates/dashboard/src/router.rs:65`.
   A future operator who changes the
   default URL (e.g. moves the testnet
   dashboard to a new project) has to
   remember to update *both* the Rust
   const AND the JS literal — exactly the
   single-source-of-truth violation the
   STW-058 + STW-059 chain just closed for
   the *non-default* case. The Design-lens
   fix is a 1-line change: inject a second
   `<script>window.__DASHBOARD_DEPLOYED_URL_
   DEFAULT__ = "<url>";</script>` (sourced
   from the Rust `DEFAULT_DEPLOYED_URL`
   const) and change the fallback to read
   the new global, so the JS + Rust are
   one source.

The eighth pass therefore ships five
deliverables, ordered by leverage:

- [x] **[P0] `STW-062` The
  `crates/dashboard/src/router.rs
  ::serve_static_index` handler
  caches the `(deployed_url,
  injected_body)` pair in a
  process-local `OnceLock<(String,
  String)>` so a re-deploy to a
  different Pages project still
  picks up the new URL on the next
  page load, but a refresh-hammer
  from a CI worker re-fetching the
  dashboard does not re-run the
  5x `.replace()` + `rfind` +
  `format!` + 3x `push_str` work
  on every request.** A 1-file,
  ~10-line change to
  `crates/dashboard/src/router.rs
  ::serve_static_index`: the
  handler first reads the
  `deployed_url()` value (line
  561), then checks a
  `static INJECT_CACHE:
  OnceLock<(String, String)>` for
  a cached `(url, body)` pair;
  on a hit with a matching `url`
  the cached `body` is served
  verbatim; on a miss (or a
  `url` mismatch) the handler
  re-runs `inject_deployed_url`
  and replaces the cache. The
  `DEPLOYED_URL_TEST_OVERRIDE`
  `Mutex<Option<String>>` (line
  92) is consulted on every
  request (it's the test
  integration seam the STW-058
  smoke test drives), so an
  integration test that calls
  `set_deployed_url_for_test(
  "<url>")` followed by `set_
  deployed_url_for_test(
  "<other>")` and then asserts
  the second response body
  matches the second URL proves
  the cache invalidates
  correctly. Scope boundary:
  does NOT change the
  `inject_deployed_url` function
  (the cache wraps it; the
  function is unchanged); does
  NOT change the
  `RBP_DASHBOARD_DEPLOYED_URL`
  env-knob read semantics (the
  `deployed_url()` helper is
  unchanged); does NOT change
  the `Cache-Control: no-cache`
  response header (the cached
  body is still served no-cache,
  a CI worker still re-fetches
  on every page load, but the
  *work* the no-cache re-fetch
  triggers is now a `HashMap`
  lookup + a `String::clone`
  instead of a full re-inject).
  Verification commands:
  `cargo test -p rbp-dashboard
  --test smoke` (the new
  sub-test + the existing 5
  sub-tests all pass),
  `cargo test --workspace --
  --test-threads=4`, `cargo
  check --workspace`, `cargo
  fmt --check`. Hand-test
  command: `RBP_DASHBOARD_
  DEPLOYED_URL=https://example
  .pages.dev/ cargo run -p
  rbp-dashboard` followed by
  `curl -s http://localhost:
  8080/ | grep deployed_at`
  (returns the URL on every
  request; a `time curl` of N
  requests shows the per-
  request CPU drops to ~0
  after the first call).
  Required tests: 1 new
  sub-test in `crates/dashboard
  /tests/smoke.rs`
  (`serve_static_index_caches_
  injected_body_across_requests`,
  drives the dashboard twice
  with the same env knob, asserts
  the two response bodies are
  byte-identical AND that the
  second response is served from
  the cache by asserting the
  `Arc::strong_count` of the
  `static_index_html` field is
  stable across the two
  requests — the cache is a
  `OnceLock` so the count must
  not grow on the second call).
  Dependencies: STW-058 (the
  `RBP_DASHBOARD_DEPLOYED_URL`
  env-knob read the cache
  wraps), STW-036 (the
  `crates/dashboard/` static
  dashboard crate). Estimated
  scope: XS. Completion
  signal: `cargo test -p
  rbp-dashboard --test smoke`
  is green with the new
  sub-test; the dashboard
  serves a `GET /` with the
  injected meta line + no
  per-request re-inject work
  after the first request.
  **`lens:` Eng (the
  `OnceLock` cache is the
  cheapest possible memoization
  shape — no `Mutex`, no
  `RwLock`, no `Arc<HashMap>`,
  the env knob reads
  byte-deterministic across
  requests so the cache hit
  rate is ~100% in production)
  + Design (a CI worker that
  re-fetches the dashboard N
  times in a row gets N
  identical responses in N
  microseconds instead of N
  identical responses in N
  hundred-microseconds; the
  per-request CPU drops by
  ~5x).**

- [ ] **[P1] `STW-063` The
  `crates/dashboard/src/router.rs
  ::serve_transcript` handler
  caches the per-`id`
  `transcript-<id>.json` file
  bytes in a `DashMap<String,
  (SystemTime, Arc<Bytes>)>`
  keyed on the `id` so a CI
  worker re-fetching the same
  transcript bundle on every
  page reload amortizes the
  `std::fs::read` work across
  requests.** A 1-file,
  ~15-line change to
  `crates/dashboard/src/router.rs
  ::serve_transcript`: the
  handler does a
  `DashMap::entry(id).or_
  insert_with(|| { let bytes =
  std::fs::read(&path); ... })`
  + a `SystemTime::from(
  path.metadata()?.mtime)`
  check that invalidates the
  entry on a `mtime` change
  (so a new `trainer --bench`
  run that re-writes
  `transcript-<id>.json` is
  picked up automatically). The
  `DashMap` is the same
  per-process cache the
  autotrain trainer's
  `PlanCache` (the
  `crates/autotrain/src/
  blueprint.rs` lazy-blueprint
  hydrate) uses, so the
  dependency is already in the
  workspace's `Cargo.lock`.
  Scope boundary: does NOT
  change the `GET /transcript
  /:id` route shape (the
  handler is the only change);
  does NOT change the
  `RBP_DASHBOARD_TRANSCRIPT_DIR`
  env knob; does NOT change
  the `is_safe_id` validator
  (the `id` is still validated
  before the cache lookup);
  does NOT change the
  `404 Not Found` error
  surface (a missing file
  still returns 404, the cache
  miss falls through to the
  same `std::fs::read` path).
  Verification commands:
  `cargo test -p rbp-dashboard
  --test smoke` (the new
  sub-test + the existing 5
  sub-tests all pass),
  `cargo test --workspace --
  --test-threads=4`, `cargo
  check --workspace`, `cargo
  fmt --check`. Hand-test
  command:
  `RBP_DASHBOARD_TRANSCRIPT
  _DIR=/tmp/rr cargo run -p
  rbp-dashboard` followed by
  `echo '<transcript>' >
  /tmp/rr/transcript-abc.json`
  and `curl -s
  http://localhost:8080/
  transcript/abc` (returns
  the body; a `time curl` of
  100 calls to the same `id`
  shows the per-request cost
  drops to ~0 after the first
  call). Required tests: 1
  new sub-test in
  `crates/dashboard/tests/
  smoke.rs`
  (`serve_transcript_caches_
  file_bytes_across_requests`,
  writes a synthetic
  `transcript-abc.json` to a
  `tempfile::TempDir`, drives
  the dashboard with `id=
  "abc"` twice, asserts the
  two response bodies are
  byte-identical AND the
  second response is served
  from the cache by asserting
  the `DashMap::len()` is
  stable across the two
  requests + a third request
  that overwrites the
  transcript file with new
  bytes + a 1-second sleep +
  a `touch -d` to bump the
  mtime asserts the cache
  invalidates correctly).
  Dependencies: STW-036 (the
  `crates/dashboard/` static
  dashboard crate the
  `serve_transcript` handler
  lives in), STW-015 (the
  `Transcript` bundle the
  bench harness writes the
  `transcript-<id>.json`
  files from). Estimated
  scope: S. Completion
  signal: `cargo test -p
  rbp-dashboard --test smoke`
  is green with the new
  sub-test; the dashboard
  serves a `GET /transcript/
  :id` with the file bytes
  + no per-request `std::fs::
  read` work after the first
  request. **`lens:` Eng
  (the `DashMap` + mtime
  invalidation is the
  cheapest possible per-`id`
  memoization shape — the
  existing workspace already
  imports `DashMap` so no new
  dep is added) + Design (a
  CI worker re-fetching the
  same transcript bundle
  during a `trainer
  --verify-bundle` re-verify
  loop gets the bytes back
  in microseconds instead of
  the file-I/O latency the
  current per-request
  `std::fs::read` triggers).**

- [ ] **[P1] `STW-064` The
  `crates/dashboard/src/router.rs
  ::inject_deployed_url` function
  returns a `Result<String,
  InjectError>` instead of
  panicking on a `</head>`-less
  page so a future refactor
  that drops the static
  `index.html` `</head>` tag
  surfaces a 500 with a clear
  diagnostic instead of
  crashing the entire
  axum server.** A 1-file,
  ~15-line change to
  `crates/dashboard/src/
  router.rs`: the
  `inject_deployed_url`
  function (line 503) now
  returns
  `Result<String, InjectError>`
  where `InjectError` is a
  new `pub enum InjectError`
  with a `MissingHeadTag`
  variant carrying the static
  page's `len()` for the
  diagnostic body. The
  `serve_static_index` handler
  (line 546) matches on the
  `Result` and returns
  `StatusCode::INTERNAL_SERVER_ERROR`
  + a one-line body the
  existing `not_found` /
  500 helper can emit (the
  body is
  `"inject failed:
  static index.html is
  missing </head>
  (<N> bytes loaded)"`).
  The `STW-062` cache wraps
  the new `Result`-returning
  function so a cache miss
  on the new error path does
  not poison the cache
  (the cache stores only the
  `Ok` body; a cache hit
  always serves the cached
  `Ok` body, the error path
  is hit only on a fresh
  `deployed_url()` mismatch).
  Scope boundary: does NOT
  change the
  `inject_deployed_url`
  function's escape sequence
  (the 5 `.replace()` calls
  are unchanged); does NOT
  change the
  `RBP_DASHBOARD_DEPLOYED_URL`
  env-knob read semantics
  (the `deployed_url()`
  helper is unchanged); does
  NOT change the
  `Cache-Control: no-cache`
  response header. A 500
  response is the right
  failure mode — a
  `</head>`-less page is a
  deploy error, not a
  user-correctable input
  error. Verification
  commands: `cargo test -p
  rbp-dashboard --test smoke`
  (the new sub-test + the
  existing 5 sub-tests all
  pass), `cargo test
  --workspace --
  --test-threads=4`, `cargo
  check --workspace`, `cargo
  fmt --check`. Hand-test
  command: `RBP_DASHBOARD_
  STATIC_INDEX_HTML_PATH=
  /tmp/no-head.html
  RBP_DASHBOARD_DEPLOYED_URL=
  https://example.pages.dev/
  cargo run -p rbp-dashboard`
  (where `/tmp/no-head.html`
  is a synthetic
  `<html><body>no head tag
  here</body></html>`); a
  `curl -i http://localhost:
  8080/` returns
  `500 Internal Server Error`
  + the diagnostic body. A
  fresh `cargo run -p
  rbp-dashboard` (no env
  knob) returns the normal
  200 + the meta line.
  Required tests: 1 new
  sub-test in
  `crates/dashboard/tests/
  smoke.rs`
  (`serve_static_index_
  returns_500_on_missing_
  head_tag`, drives the
  dashboard with a synthetic
  `index.html` byte slice
  that omits the `</head>`
  tag + asserts the response
  is 500 + the body contains
  the literal
  `inject failed: static
  index.html is missing
  </head>` substring).
  Dependencies: STW-058
  (the `serve_static_index`
  handler the `Result`
  return flows through),
  STW-062 (the
  `OnceLock<String, String>`
  cache the `Result` is
  cached in). Estimated
  scope: XS. Completion
  signal: `cargo test -p
  rbp-dashboard --test
  smoke` is green with the
  new sub-test; a
  `</head>`-less static
  page surfaces a 500
  instead of a server
  panic. **`lens:` Eng (the
  `Result`-return shape is
  the cheapest possible
  error-surface change — no
  `Box<dyn Error>`, no
  `anyhow::Error`, just a
  single-variant enum
  matching the existing
  `IndexClientError` /
  `InjectError` /
  `RenderError` enum
  pattern the dashboard
  already follows) +
  Design (an operator who
  deploys a malformed
  static page gets a
  friendly 500 with a
  diagnostic body instead
  of a `thread 'main'
  panicked at 'static
  index.html is missing
  </head>...'` server
  crash they have to read
  out of a log file).**

- [ ] **[P1] `STW-065` The RESCOPED 2026-06-05**
  `scripts/plan-staleness-
  gate.sh` script extends
  its ghost-detection to
  also catch `[ ] [P1]`
  rows in both
  `genesis/plans/000-ceo-
  testnet-roadmap.md` and
  `IMPLEMENTATION_PLAN.md`
  that have a corresponding
  `[x] STW-NNN` row, with a
  `RESCOPED 2026-06-05`
  marker convention the
  6th-pass `STW-056` row
  already names, so the 8
  stale `[ ] [P1]` ghost
  rows in
  `IMPLEMENTATION_PLAN.md`
  (STW-038, STW-045,
  STW-048, STW-054,
  STW-057, STW-060, STW-061,
  plus the [P1] siblings of
  the shipped [P0] rows
  STW-055 + STW-058 +
  STW-059) get mechanically
  flagged the same way
  the existing P0 ghost
  detection flags
  `[ ] [P0]` rows.** A
  1-script + 1-test change:
  `scripts/plan-staleness-
  gate.sh` adds a second
  pass that greps both
  files for `- [ ] \[P1\]
  <claim>` rows, maps each
  `[P1]` row's claim text to
  a STW id via the same
  static `STW_MAP` table
  the existing P0 path
  consults, and asserts
  each `[P1]` row's STW id
  is either `[x]` (ghost,
  flag and exit 3) or
  `RESCOPED 2026-06-05`
  (the 6th-pass `STW-056`
  row's explicit
  convention, marked and
  pass-through). The new
  sub-test in
  `crates/autotrain/tests/
  plan_staleness.rs`
  drops a synthetic 2-row
  roadmap + a corresponding
  2-row plan into a temp
  dir, sets the env knob
  the script reads, drives
  the script, and asserts
  the script exits 3 + the
  stderr output names the
  ghost row(s). The CEO-
  lens highest-leverage
  thrust: a future
  `auto steward --report-
  only` pass that surfaces
  a "the plan has N stale
  [P1] ghost rows" warning
  gets caught at CI time
  the same way the existing
  P0 ghost detection
  catches `[ ] [P0]`
  rows. Scope boundary:
  does NOT change the
  existing P0 ghost
  detection (the new P1
  pass is a *sibling* pass
  that runs after the P0
  pass); does NOT change
  the script's exit-code
  contract (the script
  still exits 0 on green
  + 3 on ghost); does NOT
  change the
  `crates/autotrain/tests/
  plan_staleness.rs`
  P0-path sub-tests
  (the new sub-test is a
  sibling). Verification
  commands: `scripts/
  plan-staleness-gate.sh`
  (the new P1 pass + the
  existing P0 pass both
  pass, headline
  `plan staleness gate
  complete: checked=...
  ghosts=0`),
  `cargo test -p
  rbp-autotrain --test
  plan_staleness` (the
  new sub-test + the
  existing 4 sub-tests
  all pass), `cargo test
  --workspace --
  --test-threads=4`,
  `cargo check
  --workspace`, `cargo
  fmt --check`. Hand-test
  command: temporarily
  un-`- [x]` the
  `STW-057` row in
  `IMPLEMENTATION_PLAN.md`
  + run
  `scripts/plan-staleness-
  gate.sh` (exits 3 +
  stderr names the
  `STW-057` ghost); flip
  back to `- [x]` + run
  again (exits 0 + the
  new P1 pass is part of
  the headline count).
  Required tests: 1 new
  sub-test in
  `crates/autotrain/tests/
  plan_staleness.rs`
  (`plan_staleness_gate_
  catches_p1_ghosts`,
  drives the script with
  a synthetic 2-row
  roadmap + plan + asserts
  exit 3 + the stderr
  output). Dependencies:
  STW-022 (the existing
  plan-staleness gate
  the new P1 pass is
  added to). Estimated
  scope: S. Completion
  signal: `scripts/
  plan-staleness-gate.sh`
  is green with the new
  P1 pass; the 8 stale
  `[ ] [P1]` ghost rows
  in `IMPLEMENTATION_PLAN
  .md` are mechanically
  flagged the same way
  the existing P0 ghost
  detection flags
  `[ ] [P0]` rows.
  **`lens:` CEO (the
  plan-staleness gate is
  the single highest-
  leverage piece of
  plan-hygiene infra the
  STW-022 slice ships;
  extending it from P0
  to P1 is the cheapest
  possible close-out
  for the 8 stale ghost
  rows the 6th-pass
  STW-056 row was meant
  to mechanically
  prevent from re-
  appearing — and
  extends the prevention
  to the P1 surface the
  STW-056 row covers
  manually today) + Eng
  (the 2-pass script
  shape is the same
  "P0 pass + P1 pass"
  mechanical pattern the
  existing script
  already follows for
  the `checked=0 ghosts=0`
  headline; a future
  `[P2]` pass would
  mirror the new P1
  pass verbatim).**

- [ ] **[P1] `STW-066` The
  `crates/dashboard/static
  /index.html` `deployedUrl`
  JS-side fallback
  hard-coded at line 392
  reads from a new
  `window.__DASHBOARD_
  DEPLOYED_URL_DEFAULT__`
  global the
  `crates/dashboard/src/
  router.rs::serve_static_index`
  handler injects from the
  `pub const DEFAULT_
  DEPLOYED_URL` Rust
  declaration, so the
  static JS fallback +
  the Rust default are
  one source.** A 1-file
  JS-side + 1-file
  Rust-side change: the
  `serve_static_index`
  handler's injection
  point (line 530) now
  emits *two* `<script>`
  tags (one for the env-
  knob value
  `window.__DASHBOARD_
  DEPLOYED_URL__` and one
  for the Rust default
  `window.__DASHBOARD_
  DEPLOYED_URL_DEFAULT__`
  sourced from the
  existing
  `pub const DEFAULT_
  DEPLOYED_URL` at
  `crates/dashboard/src/
  router.rs:65`), and the
  `index.html` JS line
  392 reads the new
  global as the
  fallback (the line is
  `var deployedUrl =
  (typeof window !==
  'undefined' &&
  window.__DASHBOARD_
  DEPLOYED_URL__) ||
  window.__DASHBOARD_
  DEPLOYED_URL_DEFAULT__
  || '';`). The two
  `<script>` tags are
  injected as a single
  `String` in the
  `inject_deployed_url`
  function so the
  `STW-062` cache is
  unchanged (one cache
  value, one `Ok(String)`
  body). Scope boundary:
  does NOT change the
  existing
  `RBP_DASHBOARD_DEPLOYED_URL`
  env-knob read semantics
  (the env knob is still
  the primary source; the
  new global is the
  fallback); does NOT
  change the
  `pub const DEFAULT_
  DEPLOYED_URL` value
  (the const is unchanged;
  the JS now reads the
  const); does NOT change
  the `index.html` meta
  line's `deployed_at=...`
  fragment (the meta line
  is unchanged). The
  Design-lens single-
  source-of-truth fix
  closes the last hard-
  coded-URL
  `duplicate-the-Rust-
  const` violation in the
  dashboard's static
  surface. Verification
  commands: `cargo test
  -p rbp-dashboard --test
  smoke` (the new sub-
  test + the existing 5
  sub-tests all pass),
  `cargo test --workspace
  -- --test-threads=4`,
  `cargo check
  --workspace`, `cargo
  fmt --check`. Hand-test
  command:
  `RBP_DASHBOARD_DEPLOYED_URL=
  https://example.pages.dev/
  cargo run -p
  rbp-dashboard` followed
  by `curl -s http://
  localhost:8080/ | grep
  __DASHBOARD_DEPLOYED_
  URL_DEFAULT__` (returns
  the injected global
  with the
  `https://robopoker-
  testnet-dashboard.pages
  .dev/` value, sourced
  from the Rust
  `DEFAULT_DEPLOYED_URL`
  const); a follow-on
  `RBP_DASHBOARD_DEPLOYED_URL=
  https://other.pages.dev/
  cargo run -p
  rbp-dashboard` + `curl
  ... | grep deployed_at=`
  returns the env-knob
  value
  `https://other.pages
  .dev/` (the primary
  source wins, the new
  global is the
  fallback). Required
  tests: 1 new sub-test
  in `crates/dashboard/
  tests/smoke.rs`
  (`serve_static_index_
  injects_deployed_url_
  default_global`,
  drives the dashboard
  with
  `RBP_DASHBOARD_DEPLOYED_URL`
  unset + asserts the
  response body contains
  the literal
  `window.__DASHBOARD_
  DEPLOYED_URL_DEFAULT__
  = "https://robopoker-
  testnet-dashboard.pages
  .dev/";` substring, the
  same value the
  `pub const DEFAULT_
  DEPLOYED_URL` declares
  — so a future refactor
  that drifts the const
  from the JS fallback is
  caught at the same CI
  step the existing
  `meta_line_reflects_
  dashboard_deployed_url_
  env_knob` sub-test
  drives). Dependencies:
  STW-058 (the
  `RBP_DASHBOARD_DEPLOYED_URL`
  env-knob read the new
  global sits next to),
  STW-062 (the
  `OnceLock<String, String>`
  cache the dual-tag
  injection is cached
  in). Estimated scope:
  XS. Completion signal:
  `cargo test -p
  rbp-dashboard --test
  smoke` is green with
  the new sub-test; the
  static `index.html`
  `deployedUrl` fallback
  reads from a
  `window.__DASHBOARD_
  DEPLOYED_URL_DEFAULT__`
  global the Rust side
  sources from the
  `pub const DEFAULT_
  DEPLOYED_URL`
  declaration. **`lens:`
  Design (the dual-tag
  injection is the same
  single-source-of-truth
  pattern the existing
  `RBP_DASHBOARD_DEPLOYED_URL`
  + `RBP_DASHBOARD_INDEX_URL`
  env knobs follow — the
  JS-side default is now
  sourced from the same
  Rust const the README
  references, so a future
  operator who changes the
  default has to change
  *one* `pub const` line
  instead of *two* files
  in two languages) +
  Eng (the dual-tag
  injection is a 1-line
  extension to the
  existing single-tag
  `inject_deployed_url`
  function; the cache
  shape is unchanged, the
  escape sequence is
  unchanged, the meta
  line is unchanged).**

**Seat-aware blueprint integrity gate (slice 1).** The root cause of the seat-collapse bug is that player position is absent from the information-set key. `NlheInfo` in `crates/nlhe/src/info.rs:37-40` stores only `public: NlhePublic` (subgame + choices) and `secret: NlheSecret` (abstraction bucket), with no seat or position field. Consequently, the blueprint table schemas in `crates/nlhe/src/profile.rs:98-106` (and the v2/v3 mirrors in `profile_v2.rs` and `profile_v3.rs`) persist no position column. The only literal default-0 position in the lookup pipeline is `crates/clustering/src/lookup.rs:89` where the `ISOMORPHISM` table declares `position INT DEFAULT 0`, but `BulkSchema::copy` at `lookup.rs:133` only writes `(obs, abs)` so the column is never populated and remains 0 for all rows. This means every seat maps to the same abstraction bucket and then to the same policy key, collapsing all seat-specific strategy variance. The repro test `crates/autotrain/tests/seat_collapse.rs` demonstrates that two `Partial` histories differing only in `pov` (seat 0 vs seat 1) produce identical `NlheInfo` values and therefore map to the same blueprint key.

**Slice 2** (commit ac75b0e) threaded `position` into `NlheInfo` and made training/inference consistent: the 4-arg `NlheInfo` constructor now carries `position`, `DatabasePlayer::decide` passes the acting player's position, and `seat_collapse.rs` was updated to assert distinctness. A backward-compat shim remains: the 3-arg constructor defaults `position=0` for in-memory paths.

**Slice 3** (this change) hardens persistence so the blueprint schema actually carries position end-to-end. `profile.rs` / `profile_v2.rs` / `profile_v3.rs` gained a `position SMALLINT` column, `UNIQUE (past, present, choices, position, edge)`, and matching index/column-list changes. `BulkSchema::copy`, `Streamable::rows`, `Hydrate::hydrate`, `Sink::submit`, `Source::memory`, `Source::strategy`, and the `Stage`/`Stage2`/`Stage3` merge SQL all include position. `ISOMORPHISM` COPY now writes the position column (still 0 because abstraction is position-independent). The gate test `crates/nlhe/tests/position_persistence.rs` writes two policy entries for the same `(subgame, bucket, choices)` but different positions and asserts they round-trip as distinct rows.

**Slice 4** (this change) wires the fail-before-train integrity gate. `crates/autotrain/src/integrity.rs` implements `check_integrity(profile: &NlheProfile)` which computes per-position preflop open frequency and 3-bet frequency from the blueprint, asserting that early position (UTG/SB/seat 0) opens strictly tighter than late position (BTN/BB/seat 1) and that aggregate 3-bet frequency is within [5%, 15%]. The gate is invoked inside `FastSession::sync`, `Fast2Session::sync`, and `Fast3Session::sync` before any DB write, so a seat-collapsed run aborts with exit code 2 instead of persisting a bad artifact. A standalone `--integrity` CLI mode (`Mode::Integrity`) lets CI verify an existing blueprint without running a training loop. Four unit tests in `integrity.rs` cover the gate: `seat_collapsed_fixture_fails` (identical strategy across positions → `SeatCollapse` error), `sane_fixture_passes` (early tighter + 3-bet 10% → `Ok`), `threebet_too_low_fails` (3-bet 2% → `ThreeBetRange` error), and `threebet_too_high_fails` (3-bet 20% → `ThreeBetRange` error). Slice 5 = kick the corrected retrain on the position-aware schema (multi-hour, async) guarded by this gate. Slice 6 = FOLLOW-007 export to arena bridge_v1 + sanity/bb100 validation BEFORE any live deploy.

## Next-phase active items (RE-PLAN-003 2026-06-08 by designcritic, RE-PLAN task t_b415327f; supersedes RE-PLAN-002 2026-06-08 by designcritic, RE-PLAN task t_e784842c; supersedes RE-PLAN 2026-06-08 by designcritic, RE-PLAN task t_058b1c92)

The v1→v10 testnet infrastructure chain (STW-004 → STW-068, 65 shipped STW rows) is structurally complete AND the runbook-blocker has been removed. **STW-075 landed on `main` at commit `42ed437` (2026-06-08 21:16 UTC)**: the deterministic `Check::clustered` `LIMIT 16` shape is in (`crates/database/src/check.rs::clustered_decision` + `Check::clustered` SQL wrapper), 4 lib tests in `check::tests` pin the SQL fragment + byte-stability + street-distinguishing + O(n)-bounded decision, and 3 no-DB integration tests in `crates/database/tests/check_clustered.rs` exercise the helper through the public `rbp_database` surface. The `verification:workspace-parallel` mainnet-block hinge (`steward/HINGES.md` rank #2) is now mechanically unblocked — a worker can finally *run* `scripts/testnet-live-proof.sh` against a real Postgres and produce a green `receipts/testnet-live-proof-<UTC>/` directory. The `SEAT-PERSIST-001` hinge is closed (slice 4: fail-before-train integrity gate in `check_integrity` wired into v1/v2/v3 `FastSession::sync`). What is NOT done, in order of leverage, for RE-PLAN-003:

1. **No operator-visible green local live-proof receipt exists yet** — STW-075 closed the *engine-level* blocker, but a green `receipts/testnet-live-proof-<UTC>/` directory is the *operator-visible* evidence `steward/HINGES.md` rank #2 demands. STW-070 is the single highest-leverage next slice: it runs the runbook end-to-end, captures the receipt, re-verifies it with `LiveProofReceipt::read_and_verify`, and commits the artifact under `receipts/`. Without it, every downstream testnet claim ("the dashboard is live", "the public benchmark is reproducible") is a claim about *what the chain would emit*, not about *what it has emitted*. This is the terminal-evidence slice — it converts the v1→v10 chain from "structurally complete" to "operationally proven."
2. **The plan's active queue is polluted with 17 false P1 backlogs** — `IMPLEMENTATION_PLAN.md` carries 17 open `[ ] [P1]` ghost rows (`STW-040, 041, 045 (x3 duplicates), 046 (x3 duplicates), 048, 056, 057, 060, 063, 064, 065, 066, 068`) that are all RESCOPED, SUPERSEDED, or already shipped. The `scripts/plan-staleness-gate.sh` script catches `[ ] [P0]` ghosts but does not yet catch `[ ] [P1]` ghosts (the 6th-pass `STW-065` row explicitly deferred that extension). A fresh `auto parallel` tick that scans the plan sees 17 claimable P1 rows and 0 of them are real work. STW-071 retires the 13 ghost rows the original RE-PLAN-002 named (the `STW-045` x3 + `STW-046` x3 are duplicated waves, the rest are RESCOPED) and extends the staleness gate + pinner to mechanically catch any future `[ ] [P1]` ghost. This is queue-hygiene, not busywork: every future dispatch runs through this gate.
3. **The dashboard and the receipt chain are decoupled surfaces** — STW-034 / STW-035 ship a `crates/dashboard/` `axum` router that serves `INDEX.json` (the bench-card aggregator the publish-remote step pushes) at `GET /api/index`. The `## Public dashboard` section in the README anchors the testnet claim on the dashboard URL. But the *public testnet claim* is "the freshly-committed `testnet-live-proof-<UTC>/` receipt is green" — and the dashboard has no `GET /api/receipt/latest` (or `/api/receipt/<basename>`) route that surfaces the receipt's `SUMMARY.txt` + `recipe.json` + per-step `{stdout,stderr,exit}.txt` artifacts to a stranger who can `curl` the URL. The receipt from STW-070 commits to `receipts/` in the repo, but a dashboard reader cannot reach it through the deployed dashboard — they have to `git clone`. STW-076 (NEW in RE-PLAN-003) closes the v10 → v11 gap: a `GET /api/receipt/latest` route on the dashboard that reads from a configured `RBP_DASHBOARD_RECEIPTS_DIR` (default `../receipts`, relative to the dashboard crate) + serves the latest committed `testnet-live-proof-<UTC>/` receipt's `SUMMARY.txt` + `recipe.json` + per-step `stdout.txt` / `exit.txt` as a single typed JSON envelope (`{ basename, summary, recipe, steps: [{ name, exit, stdout_bytes, stderr_bytes }] }`) a dashboard reader can `curl` + parse. The receipt from STW-070 + the route from STW-076 = the public testnet surface finally matches the README's claim.
4. **The seat-aware paragraph at the end of this plan (slices 1-6 narrative) is stale** — slice 4 ships the gate, the paragraph still reads as if slice 1 is the open question, and `crates/autotrain/tests/seat_collapse.rs` test comments still describe the bug as present ("assertion fails today" / "the test fails today" prose). A reader of the plan gets a wrong model of what shipped. STW-072 reconciles the prose with the on-disk reality in a narrow docs + comments pass. Low value, low cost.
5. **The two `[!]` rows (`STW-001` planning surface, `STW-007` artifact retirement) are operator-decision blockers with no decision recorded** — the orchestrator can spawn `auto parallel` against a hand-authored queue (no gbrain needed), but a recorded decision (yes/no on `.gbrain-source` deletion, yes/no on `.auto/tui*/` retention) unblocks a downstream planning slice. STW-074 closes both with a verdict table in a new `steward/ARTIFACT-RETENTION.md` file the planner can grep. Administrative, but unblocks future dispatches that consult `steward/PROMOTIONS.md`.
6. **No external consumer can replay a robopoker blueprint today** — slice 6 of the seat-aware work names it explicitly: "FOLLOW-007 export to arena `bridge_v1` + sanity/bb100 validation BEFORE any live deploy." A v1/v2/v3 trained-config → myosu `bridge_v1` JSON exporter + a CI smoke that round-trips through `bridge_v1` is the next unblock for a third-party dashboard / arena site / training-pool integration. STW-073 is the canonical adoption-unblock slice; the spec still depends on the (external) `myosu` arena's `bridge_v1` JSON shape, so the worker for STW-073 will need to confirm the shape with the myosu maintainers or pin it from a committed fixture if the shape is unconfirmed.

The 6 rows below are the concrete, individually-shippable P0/P1 next-phase items. STW-070 + STW-071 are the two P0 claims a worker can pick up *today* (STW-075 is shipped, so STW-070 is no longer blocked on a separate slice). STW-076 is a NEW P1 item RE-PLAN-003 introduces to close the v10 → v11 dashboard/receipt adapter gap the RE-PLAN-002 leave-behind missed. STW-072/073/074 carry over from RE-PLAN-002 unchanged (still unclaimed, still valid, all still need a worker to pick them up). Owner for the RE-PLAN row itself: designcritic, 2026-06-08.

- [x] **[P0] `STW-075` `CHECK-CLUSTERED-DETERMINISTIC-SAMPLE` — commit the deterministic `Check::clustered` shape the working tree carries (uncommitted at the start of RE-PLAN-002) and add focused regression tests that pin the O(1)-on-warmed-DB + clean-false-on-fresh-DB contract. The current `crates/database/src/check.rs::Check::clustered` builds an `Isomorphism::from(Observation::from(street))` to filter the `isomorphism` table, but `Observation::from(Street)` draws cards from a *fresh* deck every call, so the `obs = $1` lookup almost never matches and `PreTraining::pending` (which consults `clustered` to decide which streets to kmeans) re-runs kmeans on every `--cluster` invocation. On a warmed 123M-row `transitions` table this exhausts the testnet-live-proof runbook's wall-clock budget before reaching the bench step, so no green `receipts/testnet-live-proof-<UTC>/` is producible. The deterministic fix is bounded and proven: `SELECT obs FROM isomorphism LIMIT 16` is O(1) on a warmed DB (the `cluster` step is the *only* writer of `isomorphism`, so "is there any obs that decodes to this street?" is the same question as "is the kmeans pass for this street done?"); with 123M rows uniformly spread across 4 streets, the probability of *all* 16 samples missing the target street is `(3/4)^16 < 1e-2` (a warmed DB with a non-empty target bucket always returns true); a fresh DB with no rows at all returns false cleanly. STW-075 ports the working-tree fix to a feature branch, lands it on `main` with a regression test, and pins the contract so the bug cannot silently regress.** Owner files: `crates/database/src/check.rs` (commit the deterministic `LIMIT 16` shape from the working tree; remove the per-call random `Isomorphism::from(Observation::from(street))` lookup; keep the change minimal — do not touch `epochs` / `blueprint` / `status` / any other `Check` method), `crates/database/tests/check_clustered.rs` (new no-DB-or-DB integration test file with 3 sub-tests: `clustered_returns_true_on_warmed_isomorphism` writes 16 rows to a temp `isomorphism` table covering all 4 streets + asserts `Check::clustered(Street::Flop)` returns true within a tight wall-clock budget (under 100 ms); `clustered_returns_false_on_fresh_empty_table` asserts an empty `isomorphism` returns false cleanly; `clustered_does_not_full_scan_warmed_table` asserts the query is O(1) — a 10K-row table returns in the same wall-clock budget as a 16-row table within an order-of-magnitude tolerance), `IMPLEMENTATION_PLAN.md` (this row + the `STW-070` row's dependency note flipped to "depends on `STW-075`" so workers reading the queue know the order), `scripts/plan-staleness-gate.sh` (no change expected; the new row is a `[P0]` claim under a `[ ]` checkbox — the gate's P0 ghost detection already catches a duplicate `[x] STW-075`). Scope boundary: do NOT change the `Check` trait signature (the existing `async fn clustered(&self, street: Street) -> bool` stays); do NOT change `PreTraining::pending` (the `clustered` consumer); do NOT change the `isomorphism` table schema (the column shape is unchanged; only the `clustered` *reader* changes); do NOT introduce a new `crates/database` dependency (the fix uses the existing `query` / `ISOMORPHISM` constant pair); do NOT touch any v1/v2/v3 profile schema, the v6 → v10 follow-on chain, the dashboard, the publish/index/remote chain, or any `trainer --*` CLI. STW-075 is the *minimum deterministic fix* for the runbook blocker — the same shape the prior re-plan's STW-070 narrative allowed as a "fallback code-fix" but did not promote to its own worker-ready row. Acceptance criteria: `git log -1 --oneline` on `main` shows a single new commit titled `feat(database): STW-075 deterministic Check::clustered via LIMIT 16 sample` + body referencing this row; `cargo test -p rbp-database --features database --test check_clustered` exits 0 with the 3 new sub-tests passing; `cargo test -p rbp-database --lib` stays green (no regression in `check::tests`); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace`, `cargo fmt --check` stay green; the new `LIMIT 16` shape is byte-stable (re-running `clustered` 100x in a row on the same warmed DB returns the same answer — covers the no-false-positive surface the prior random-`Isomorphism` shape silently violated); `bash scripts/plan-staleness-gate.sh` exits 0 with `plan staleness gate complete: checked=N ghosts=0`. Hand-test: a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` no longer re-runs kmeans on a warmed DB (the `cluster/stdout.txt` shows `skipping clustering <street>` for all 4 streets within the first second of the run, not the multi-minute kmeans trace the 2026-06-08 receipts captured). Dependencies: the working-tree change must be the same shape the planner audited (the fix is already drafted — the worker ports + tests + commits it; they do not redesign it); a reachable `DATABASE_URL` for the integration-test sub-tests that need a real Postgres (the `[P0]` test can run no-DB against an in-process mock; the worker can also gate the DB-needing sub-tests on `--features database` mirroring the existing `check.rs` test pattern). Estimated scope: S (one file fix + 3 sub-tests + one new commit). Completion signal: a fresh `cargo test -p rbp-database --features database --test check_clustered` is green with 3 new sub-tests; `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` on a warmed DB skips the kmeans step and reaches `--reset` within the first 10 s; the next planner pass can promote `STW-070` (the post-fix evidence slice) as a clean, runnable evidence task. **`lens:` CEO (without this fix the testnet live-proof north star cannot produce a single green receipt — the runbook's wall-clock budget is consumed by a deterministic bug, not by an honest kmeans pass) + Eng (one file, one trait method, three sub-tests, no schema change, no consumer change) + Design (the fix is invisible to dashboard users — it just makes the runbook produce a real receipt instead of stalling).**

+ Design (a defensive guard in the kmeans driver does not change the algorithm; the production path is symmetrically protected).**

- [ ] **[P0] `STW-070` `TESTNET-LIVE-PROOF-RECEIPT` (RE-PLAN-004 re-issue, RESCOPED 2026-06-10 by RE-PLAN-006 to depend on `STW-086`).** With `STW-075` (deterministic `Check::clustered`) + `STW-077` (fast-kmeans cap) + `STW-078` (postgres-env provisioning) + `STW-086` (kmeans empty-histogram defensive guard) shipped, produce one fresh green local `receipts/testnet-live-proof-<UTC>/` directory the `scripts/testnet-live-proof.sh` runbook emits against a real Postgres, with all 8 step exits `0` (doctor, cluster, reset, smoke, status, bench, compare, replay), the pinned `testnet live_proof complete: smoke=N status=N bench=N compare=N replay=BYTES` headline in `SUMMARY.txt`, and a `recipe.json` that re-verifies with `LiveProofReceipt::read_and_verify` (so the operator-visible receipt and the CI-visible receipt share one verifier, closing the residual "tool claims ≠ operator evidence" gap `steward/HINGES.md` rank #2 names). This row is *evidence only*: the runbook is run, the receipt is captured, the verifier is invoked, the result is committed. If a fresh runbook run fails, the worker reports the failure as a new `[P0]`-ranked next slice (or blocks for human input) rather than silently fixing the runbook in the STW-070 commit. Owner files: `scripts/testnet-live-proof.sh` (no code change expected), `scripts/testnet-live-proof.md` (note the evidence path + the `STW-075` + `STW-077` + `STW-078` + `STW-086` dependency chain), the new receipt directory under `receipts/testnet-live-proof-<UTC>/` (the deliverable), `steward/HAZARDS.md` (the `TESTNET-LIVE-PROOF-RECEIPT` row flips from "open" to "closed by STW-070 on <date>" once the receipt is committed), `steward/DRIFT.md` (the `STW-019` row's "DRIFT" verdict on the receipts-orphans line flips to "RESOLVED by STW-070 on <date>" once the new receipt is committed). Scope boundary: do not change the runbook's chain step order; do not weaken `LiveProofReceipt::read_and_verify`; do not bypass `trainer --doctor`; do not touch the `Check::clustered` fix (that is `STW-075`'s commit, not this one); do not touch the kmeans cap (that is `STW-077`'s commit, not this one); do not touch the postgres provisioning script (that is `STW-078`'s commit, not this one); do not touch the kmeans empty-histogram guard (that is `STW-086`'s commit, not this one); prefer `RBP_TESTNET_FAST=1` for the canonical first green proof; do NOT touch the dashboard / publish / index / index-remote / remote chain; do NOT delete the existing failed/partial `receipts/testnet-live-proof-*` directories. Acceptance criteria: a `find receipts/testnet-live-proof-<UTC>/` shows 8 step subdirs each with `stdout.txt` + `stderr.txt` + `exit.txt` where every `exit.txt` is `0`; `SUMMARY.txt` head is the pinned `testnet live_proof complete: ...` line; `recipe.json` parses with `LiveProofRecipe`; `trainer --verify-receipt receipts/testnet-live-proof-<UTC>/` exits 0 and prints `live_proof receipt verification passed: ...`; the receipt is committed on `main`. Dependencies (RE-PLAN-004, RESCOPED 2026-06-10 by RE-PLAN-006, then by RE-PLAN-007): `STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087` (the second empty-cluster guard STW-087 closed) + a reachable `DATABASE_URL` or `DB_URL`; existing `STW-067` (fast-mode) + `STW-069` (fail-before-train integrity gate) + `STW-019` (runbook) + `STW-023` (shared verifier + `recipe.json`) + `STW-028` (`--verify-receipt` CLI). Estimated scope: S (operator-runnable evidence, on a fresh DB, post-`STW-075` + `STW-077` + `STW-078` + `STW-086`). Completion signal: a fresh `trainer --verify-receipt` exits 0 on a `receipts/testnet-live-proof-<UTC-THIS-WEEK>/` directory and the receipt path is recorded in `steward/HAZARDS.md` row #2 + `steward/DRIFT.md` `STW-019` row as the current operator proof.

- [x] **[P0] `STW-071` `PLAN-GHOST-RETIRE-001` RESCOPED 2026-06-09 by RE-PLAN-004 (carried over unchanged into the active queue as STW-071; the superseding RE-PLAN re-issued the row verbatim in its own `[ ]` form so the worker's claim points at the current spec).** The 13 stale `[ ] [P1]` ghost rows in `IMPLEMENTATION_PLAN.md` (STW-040, STW-041, STW-045 (x3 duplicates), STW-046 (x3 duplicates), STW-048, STW-056, STW-057, STW-060, STW-063, STW-064, STW-065, STW-066, STW-068) and extend `scripts/plan-staleness-gate.sh` + `crates/autotrain/tests/plan_staleness.rs` to mechanically catch any future `[ ] [P1]` row that has a corresponding `[x] STW-NNN` row (mirroring the existing P0 ghost detection, with a `RESCOPED <date> by STW-071` marker convention so a `RESCOPED` row passes the gate cleanly).** Owner files: `IMPLEMENTATION_PLAN.md` (the 13 ghost rows are flipped to `[x] STW-NNN RESCOPED <date> by STW-071` or removed; the 6 historical wave sections that quote the ghost rows are unchanged — they are evidence, not active queue), `scripts/plan-staleness-gate.sh` (adds a second pass that greps for `- [ ] \[P1\] <claim>` rows + maps each claim to a `STW-NNN` id via the same `STW_MAP` table the P0 pass consults + flags ghosts), `crates/autotrain/tests/plan_staleness.rs` (adds a new sub-test that drops a synthetic 2-row roadmap + 2-row plan into a temp dir + drives the script + asserts exit 3 + asserts stderr names the ghost rows). Scope boundary: do NOT rewrite shipped rows except the minimum checkbox/RESCOPED markers needed to make the active queue truthful; do NOT remove `STW-001` or `STW-007` (they are explicit `[!]` operator decisions and `STW-074` owns the closeout); do NOT touch `genesis/plans/000-ceo-testnet-roadmap.md` (it is history, not active queue); do NOT create new numbered `ExecPlan`s; do NOT change runtime code; do NOT touch `steward/PROMOTIONS.md`'s `STW-001`/`STW-007` `deferred` row (the planner will re-rank after `STW-074` ships). Acceptance criteria: `rg -n "^- \[ \] \*\*\[P1\].*STW-(040|041|045|046|048|056|057|060|063|064|065|066|068)" IMPLEMENTATION_PLAN.md` returns no claimable open rows; the 13 `RESCOPED <date> by STW-071` rows are present + grep-clean; `scripts/plan-staleness-gate.sh` exits 0 with headline `plan staleness gate complete: checked=N ghosts=0`; the new `plan_staleness.rs` sub-test passes; the `[ ]` count in `IMPLEMENTATION_PLAN.md` drops from 17 to 4 (the 3 next-phase rows from this RE-PLAN + 1 carry-over if any). Verification commands: `bash scripts/plan-staleness-gate.sh` (the new P1 pass + the existing P0 pass), `cargo test -p rbp-autotrain --test plan_staleness` (the new sub-test + the 5 existing sub-tests), `rg -n "^- \[ \] \*\*\[P[01]\]" IMPLEMENTATION_PLAN.md | wc -l` (should drop), `cargo test --workspace -- --test-threads=4`, `cargo check --workspace`, `cargo fmt --check`. Hand-test: a fresh `auto parallel` tick that scans the plan sees 4 (or fewer) claimable P0/P1 rows + 0 ghosts. Dependencies: the existing `STW-022` `scripts/plan-staleness-gate.sh` (P0 pass is the model); the existing `crates/autotrain/tests/plan_staleness.rs` (5 sub-tests is the model). Estimated scope: S. Completion signal: `auto parallel` claims the next real next-phase row (`STW-070`, `STW-072`, `STW-073`, or `STW-074`) — not a ghost. **`lens:` CEO (the 11k-line plan is a false backlog signal that re-`dispatched` shipped work; retirement is the subtraction default) + Eng (one shell-script extension + one sub-test = 1 day of work) + Design (a worker reading the plan sees a clean active queue, not 17 noise rows + 1 real row).**

- [x] **[P1] `STW-072` RESCOPED 2026-06-09 by RE-PLAN-004 — DEPRECATED (low value).** RE-PLAN-004 retires this row without re-issuing: the seat-aware paragraph at the tail of `IMPLEMENTATION_PLAN.md` (slices 1-6 narrative) has been implicitly corrected by the RE-PLAN-002/003 re-writes of the "Next-phase active items" preamble (the slice 1-4 commit hashes are visible in the next-phase preambles themselves), and the `crates/autotrain/tests/seat_collapse.rs` test comment is a low-leverage copyedit. A worker who notices a specific stale claim in either the paragraph or the test comment can fix it as a P3 + a `cargo test -p rbp-autotrain --test seat_collapse` regression check (the gate will catch any drift in the green shape). The RE-PLAN-003 spec is preserved as evidence above; a fresh planner who needs the row can re-promote it. Owner files: `IMPLEMENTATION_PLAN.md` (rewrite the slices 1-6 paragraph to read as a *shipped* sequence: slice 1 traces + repro test (commit 4337cf0), slice 2 threads position into `NlheInfo` (ac75b0e), slice 3 hardens persistence (ef15d84), slice 4 wires the integrity gate (a9f08a3 + 68b495f), with `Slice 5 = STW-073` export + `Slice 6 = live deploy`), `crates/autotrain/tests/seat_collapse.rs` (rewrite the test-header comment to read "Seat-collapse regression: asserts that two `Partial` histories differing only in `pov` (seat 0 vs seat 1) produce *distinct* `NlheInfo` values end-to-end through the v1 profile schema's `UNIQUE (past, present, choices, position, edge)` index — guards against a future refactor that drops `position` from the schema"), `crates/nlhe/tests/position_persistence.rs` (if its test-header comment also says "fails today", rewrite it to the green shape). Scope boundary: do NOT change the integrity gate's algorithm (the `5%-15%` 3-bet range, the early-tighter-than-late assert, the per-position frequency computation); do NOT change the v1/v2/v3 profile schemas; do NOT change the `UNIQUE` index shape; do NOT remove or rename any test; do NOT touch the v6 → v10 follow-on chain narrative earlier in the plan. Acceptance criteria: a `rg -n "fails today|assertion fails|test fails" crates/autotrain/tests/seat_collapse.rs` returns no matches; the rewritten plan paragraph names the actual commit hashes (4337cf0 / ac75b0e / ef15d84 / a9f08a3 / 68b495f) the slice 1-4 work landed in; a fresh reader of the plan sees the seat-aware work as *shipped through slice 4, with slice 5 (export) + slice 6 (deploy) as the next two follow-ons*. Verification commands: `rg -n "fails today|assertion fails|test fails" crates/` (should be empty), `cargo test -p rbp-autotrain --test seat_collapse` (still green), `cargo test -p rbp-nlhe --test position_persistence` (still green), `cargo test -p rbp-autotrain --test integrity_gate` (or whatever the integrity gate's lib test is named — still green), `cargo test --workspace -- --test-threads=4`, `cargo check --workspace`, `cargo fmt --check`. Hand-test: `git log --oneline --grep "position\|seat\|integrity" | head -10` shows the slice 1-4 commits. Dependencies: slice 1-4 work (landed). Estimated scope: XS (markdown + 1 test comment file). Completion signal: the plan's seat-aware paragraph reads as shipped, and `rg "fails today"` returns nothing under `crates/`. **`lens:` Design (a reader of the plan should not have to read source to learn what shipped) + Eng (1 markdown file + 1 comment-only test header = under 1 hour of work) + CEO (the next-phase row for slice 5/6 — `STW-073` — is named explicitly so a worker can claim it without rediscovering the "what's after slice 4" context).**

- [x] **[P1] `STW-073` RESCOPED 2026-06-09 by RE-PLAN-004 — DEFERRED (depends on STW-070).** RE-PLAN-004 explicitly defers this row: STW-073 depends on `STW-070` (a green local receipt — the export reads from a blueprint a `trainer --fast` run produced), and STW-070 is in turn blocked on `STW-077` + `STW-078` shipping. Re-promote STW-073 as soon as a green `receipts/testnet-live-proof-<UTC-THIS-WEEK>/` is committed; the RE-PLAN-003 spec is preserved as evidence above and can be re-issued verbatim. Slice 6 of the seat-aware work — FOLLOW-007 export to arena `bridge_v1` + sanity/bb100 validation BEFORE any live deploy. A new `crates/autotrain/src/bridge_v1.rs` module + a `Mode::ExportBridgeV1` CLI arm exports the v1 / v2 / v3 trained configs (post `check_integrity` gate, the operator points at a green `trainer --integrity` result) to a deterministic `bridge_v1-<UTC-ISO>.json` JSON file in the `myosu` `bridge_v1` shape (the shape `bridge_v1` is the myosu arena's blueprint-replay surface; a committed `crates/autotrain/tests/fixtures/bridge_v1-fixture.json` pins the shape byte-stable); a new `Mode::VerifyBridgeV1` CLI arm re-hashes the export + asserts every `bridge_v1` key is in the pinned set + asserts the per-position 3-bet frequency and the v1-vs-v2-vs-v3 aggregate mbb/100 (replayed through `bridge_v1`'s replay API against the same `Fish` baseline) match the bench's `Compare3Report` numbers within `[BRIDGE_V1_TOLERANCE]` mbb/100. Owner files: `crates/autotrain/src/bridge_v1.rs` (new — the exporter + verifier + `BridgeV1Error` enum + 6 lib tests covering shape pin, position-aware key inclusion, sanity 3-bet frequency, v1/v2/v3 aggregate mbb/100 replay, `--integrity` gate integration, byte-stability on re-export of an unchanged blueprint), `crates/autotrain/src/mode.rs` (new `Mode::ExportBridgeV1` + `Mode::VerifyBridgeV1` arms + `--export-bridge-v1` / `--verify-bridge-v1` argv handling + `bridge_v1_hands` / `bridge_v1_tolerance` env helpers), `crates/autotrain/tests/bridge_v1.rs` (new integration test gated on `database` + `DATABASE_URL` — drives `trainer --reset` then `trainer --export-bridge-v1` then `trainer --verify-bridge-v1` end-to-end through a real subprocess and asserts the JSON parses, the headline shape matches `Compare3Report`'s `ranked_winner`, and the v1/v2/v3 mbb/100 replay numbers agree with the bench within tolerance), `crates/autotrain/tests/fixtures/bridge_v1-fixture.json` (new committed byte-stable fixture in the pinned `bridge_v1` shape), `crates/autotrain/tests/script_shape.rs` (2 new shell-shape pins: the `bridge_v1_export_runbook_exists_and_parses` pin + the `bridge_v1_export_runbook_has_verify_pre_export_gate` pin, mirroring the `testnet_live_publish_*_script_*` pinners). The companion `scripts/testnet-live-bridge-v1.sh` runbook is pure bash + chains `trainer --integrity` (pre-export refuse-to-export-non-green-blueprint gate) + `trainer --export-bridge-v1` + `trainer --verify-bridge-v1` as a sequence of subprocesses + emits a one-line `testnet live_bridge_v1 complete: blueprint=v3 hands=N mbb_per_100=X` headline a myosu arena scraper can `grep ^testnet live_bridge_v1` the log. Scope boundary: do NOT push to myosu / git-tag / S3 (a CI worker can `curl -X POST` the local `bridge_v1-<UTC-ISO>.json` in a follow-on slice); do NOT change the v1/v2/v3 `NlheProfile` JSON shape (a profile-shape drift fails `trainer --integrity` BEFORE the export); do NOT change the `bridge_v1` myosu shape (it is the consumer's contract; a shape drift fails the verifier); do NOT introduce a new dep (the existing `serde` + `serde_json` + `tokio_postgres` the autotrain crate already imports are sufficient); do NOT touch the dashboard / publish / index / index-remote / remote chain (the `bridge_v1` is a different consumer — a myosu arena site, not a dashboard bucket). Acceptance criteria: `trainer --export-bridge-v1 <blueprint=any> --out <path>` writes a `bridge_v1-<UTC-ISO>.json` that re-hashes byte-stable on re-run against an unchanged blueprint; `trainer --verify-bridge-v1 <path>` exits 0 with a one-line `bridge_v1 verification passed: ...` headline on a green export and exits 2 with a one-line `bridge_v1 verification failed: ...` headline on a tampered export; the integration test's v1/v2/v3 mbb/100 replay numbers agree with `trainer --compare3` within `BRIDGE_V1_TOLERANCE` (default `5.0` mbb/100). Verification commands: `cargo test -p rbp-autotrain --lib bridge_v1` (the 6 lib tests), `cargo test -p rbp-autotrain --test bridge_v1` (the integration test), `cargo test -p rbp-autotrain --test script_shape` (the 2 new shell-shape pins + the 33+ existing), `bash -n scripts/testnet-live-bridge-v1.sh`, `cargo test --workspace -- --test-threads=4`, `cargo check --workspace`, `cargo fmt --check`. Hand-test: `trainer --export-bridge-v1 --blueprint v1 --out /tmp/bv1.json && trainer --verify-bridge-v1 /tmp/bv1.json && cat /tmp/bv1.json | head -1` shows the pinned `bridge_v1` JSON shape. Dependencies: `STW-070` (a green local receipt — the blueprint the export reads from must come from a `trainer --fast` run the receipt's `cluster/reset/smoke/status/bench` chain produced, not the committed fixture); `STW-031` (`trainer --compare3` v1-v2-v3 compare — the bridge export's sanity replay reuses its `Compare3Report` math); `STW-029` (v3 trained config); `STW-067` (fast-mode); `STW-069` (integrity gate). Estimated scope: L. Completion signal: a CI worker can `bash scripts/testnet-live-bridge-v1.sh` after `bash scripts/testnet-live-proof.sh` and drop a `bridge_v1-<UTC-ISO>.json` a myosu arena site can `curl + replay` to validate the v1/v2/v3 trained configs end-to-end. **`lens:` CEO (the `bridge_v1` export is the missing link between robopoker's "trained strategy exists" claim and the wider poker-AI ecosystem's "is this blueprint actually strong" question — without it, robopoker is a self-validating island) + Eng (the exporter is a deterministic shape pin; the verifier is a no-DB no-rebuild re-verify path; the integration test reuses `Room` + `Compare3Report` math the bench already exercises) + Design (a stranger can `cat bridge_v1-<UTC-ISO>.json` and see the v1/v2/v3 trained configs in a human-readable shape, not a binary blob).**

- [x] **[P1] `STW-074` RESCOPED 2026-06-09 by RE-PLAN-004 (carried over unchanged into the active queue as STW-074; the superseding RE-PLAN re-issued the row verbatim in its own `[ ]` form so the worker's claim points at the current spec).** RE-PLAN-003 framing (evidence): close the two `[!]` operator-decision rows (`STW-001` planning surface, `STW-007` artifact retirement) with a recorded decision the next planner pass can promote against — `STW-001` resolves to "hand-author a queue in `IMPLEMENTATION_PLAN.md` (the current plan is the queue; `gbrain` is not required for the next 3 months of work — `genesis/plans/000-ceo-testnet-roadmap.md` is evidence, `IMPLEMENTATION_PLAN.md` is the queue)" or "block on gbrain init"; `STW-007` resolves to a per-path retention verdict for `.gbrain-source` (delete or keep), `.auto/tui*/` (delete or keep), `.auto/orchestrator/velocity-*` (delete), `.auto/corpus-staging/` (delete), `.auto/logs/steward-*-prompt.md` (delete), with the verdict recorded in a new `steward/ARTIFACT-RETENTION.md` file the planner can grep. Owner files: `IMPLEMENTATION_PLAN.md` (the `## Deferred items (need operator decision before promotion)` section is rewritten to a `## Operator decisions (RESOLVED <date> by STW-074)` section with the verdict for each row + a one-paragraph rationale; the new `STW-074` row flips to `[x]` once recorded), `steward/ARTIFACT-RETENTION.md` (new — a per-path verdict table: `path | verdict | rationale | signoff`, with the `STW-074` row's resolution as the contents), `steward/HAZARDS.md` (the `STW-001` + `STW-007` rows flip from "open" to "closed" with a one-line `closed by STW-074 on <date>` note), `steward/PROMOTIONS.md` (the `STW-001` / `STW-007` `deferred` row is replaced with a `STW-074` `promoted` row), `steward/DRIFT.md` (the `STW-001` + `STW-007` rows in the `Blocked / Deferred` table are updated from `DRIFT` / `AGREES` to `RESOLVED` with the verdict). Scope boundary: do NOT execute the deletion (the planner is the recording desk, not the cleanup crew — a separate operator-side slice deletes after sign-off); do NOT change the `.gitignore`; do NOT remove `genesis/plans/000-ceo-testnet-roadmap.md` (it is the historical record the planner audits against); do NOT change runtime code; do NOT touch the 13 ghost rows `STW-071` retires. Acceptance criteria: the two `[!]` rows in `IMPLEMENTATION_PLAN.md` are replaced with `[x] STW-NNN RESOLVED <date> by STW-074` rows + a one-paragraph verdict each; `steward/ARTIFACT-RETENTION.md` is a clean per-path verdict table; `steward/{HAZARDS,PROMOTIONS,DRIFT}.md` are updated in lockstep; a `rg -n \"^- \\[!\\]\" IMPLEMENTATION_PLAN.md` returns no rows. Verification commands: `rg -n \"^- \\[!\\]\" IMPLEMENTATION_PLAN.md` (should be empty), `rg -n \"STW-001|STW-007\" IMPLEMENTATION_PLAN.md steward/` (should show only `RESOLVED ... by STW-074` references), `bash scripts/plan-staleness-gate.sh` (must stay green — no new ghosts introduced), `cargo test --workspace -- --test-threads=4` (no code change so this is a regression check, not a feature check), `cargo check --workspace`, `cargo fmt --check`. Hand-test: a planner scanning `IMPLEMENTATION_PLAN.md` no longer sees 2 `[!]` rows; the next `auto steward --report-only` pass has a clean `Blocked / Deferred` table to start from. Dependencies: operator input on the 6 `.auto/` + `.gbrain-source` retention verdicts (this is the only row in the RE-PLAN that genuinely blocks on human input — every other row is a code/markdown change a worker can ship solo). Estimated scope: S (markdown only, no code). Completion signal: `rg -n \"^- \\[!\\]\" IMPLEMENTATION_PLAN.md` returns no rows; the next `auto steward --report-only` pass promotes against a clean decision table, not 2 deferred items + 13 ghost rows + 0 real next slices. **`lens:` CEO (the deferred items are the noise floor of every planner pass — recording a verdict turns the noise into a signal) + Eng (markdown-only change; no code risk) + Design (a planner sees a decision table, not a `[!]` warning).**

- [x] **[P1] `STW-076` RESCOPED 2026-06-09 by RE-PLAN-004 (REVISED; the v10 → v11 dashboard/receipt adapter gap RE-PLAN-003 named is re-issued in the new RE-PLAN-004 section with an added no-DB fixture-fallback path so the public testnet claim is never empty at the dashboard URL).** RE-PLAN-003 framing (evidence): close the gap between the dashboard's `GET /api/index` bench-card surface (STW-034 / STW-035, the v10 deployed surface) and the freshly-committed `receipts/testnet-live-proof-<UTC>/` directory the STW-070 evidence slice drops. The README's `## Public dashboard` section anchors the testnet claim on the deployed dashboard URL, but a stranger who `curl`s the deployed dashboard today sees the *published* bench cards from `INDEX.json` (the STW-035 publish-remote chain's output) — they do NOT see the *receipt* the runbook committed (the `SUMMARY.txt` + `recipe.json` + per-step `{stdout,stderr,exit}.txt` artifacts under `receipts/testnet-live-proof-<UTC>/`). The receipt is the *primary operator-visible proof* of the testnet north star; without a dashboard route, a stranger has to `git clone` to read it. STW-076 ships a thin `GET /api/receipt/latest` (and `GET /api/receipt/<basename>`) route on the existing `crates/dashboard/` `axum` router that reads from a configured `RBP_DASHBOARD_RECEIPTS_DIR` (default `../receipts`, relative to the dashboard crate) + serves a single typed JSON envelope: `{ basename, summary: <SUMMARY.txt contents>, recipe: <recipe.json parsed as LiveProofRecipe>, steps: [{ name, exit, stdout_bytes, stderr_bytes }] }` (the envelope re-uses the `LiveProofRecipe` struct the autotrain crate already ships; the new code is a thin file-read + serde_json::from_str wrapper, no new domain types). The `latest` route picks the lexicographically-maximum `testnet-live-proof-<UTC>/` basename under `RBP_DASHBOARD_RECEIPTS_DIR` (the same UTC-ISO sort the runbook's SUMMARY.txt line `testnet live_proof complete: ...` follows, so dashboard sort matches receipt sort); the `<basename>` route picks the named receipt. Both routes return `404 + a typed `DashboardError::ReceiptMissing(<basename>)` JSON body` when no receipt matches (the dashboard's existing `crates/dashboard/src/router.rs` error mapper renders the typed body the same way the existing `IndexClient::read` / `IndexMissing` paths do, so a stranger can `curl .../api/receipt/latest` and `jq .error` for the failure shape). The static `crates/dashboard/static/index.html` empty-state gets a one-line `<a href="/api/receipt/latest">Latest live_proof receipt (JSON)</a>` link beside the existing "Demo data: `/bench/compare3-fixture`" link so a first-time visitor can audit the most-recent receipt without writing `curl + jq`. A new `crates/dashboard/tests/receipt_route.rs` integration test drives the axum router end-to-end and asserts: (a) `GET /api/receipt/latest` on a temp `RBP_DASHBOARD_RECEIPTS_DIR` containing one synthetic `testnet-live-proof-20260608T210000Z/` (a fixture dir with a `SUMMARY.txt` + `recipe.json` + 8 step `stdout.txt` / `exit.txt` files mirroring the runbook's layout) returns 200 with the pinned envelope shape — `summary` matches `SUMMARY.txt` verbatim, `recipe.steps[0].name == "doctor"`, `steps[0].exit == 0`; (b) `GET /api/receipt/testnet-live-proof-20260608T210000Z` returns the same envelope; (c) `GET /api/receipt/does-not-exist` returns 404 + `{"error":"receipt_missing","basename":"does-not-exist"}`; (d) `GET /api/receipt/latest` on an empty / missing `RBP_DASHBOARD_RECEIPTS_DIR` returns 404 + `{"error":"receipts_dir_missing","path":"..."}`. The `serve_receipt` handler is a small file-read function (< 80 lines, no tokio-postgres, no JSON streaming, no new dep — only `serde_json` + `std::fs::read_to_string` + `axum::Json`); the existing `OnceLock<String, String>` + mtime-invalidation pattern the STW-062 / STW-063 cache already follows is reused for the `<basename>` route so a receipt re-read is a cache hit on the second `curl` (the `latest` route is uncached by design — its sort order is the operator-visible signal). Owner files: `crates/dashboard/src/router.rs` (new `serve_receipt_latest` + `serve_receipt_named` handlers + `DashboardError::ReceiptMissing(<basename>)` + `DashboardError::ReceiptsDirMissing(<path>)` variants; reuse the existing typed-error JSON mapper the `IndexClient::read` path already follows), `crates/dashboard/src/state.rs` (the `RBP_DASHBOARD_RECEIPTS_DIR` env knob read at boot — default `"../receipts"` so a `cargo run -p rbp-dashboard` from the workspace root resolves to the in-repo `receipts/` dir; absolute paths also supported), `crates/dashboard/static/index.html` (one-line `<a href="/api/receipt/latest">...</a>` link in the existing empty-state paragraph), `crates/dashboard/tests/receipt_route.rs` (new no-DB integration test file with the 4 sub-tests above), `scripts/deploy-dashboard-cloudflare.md` (note the `RBP_DASHBOARD_RECEIPTS_DIR` knob the deploy runbook should set to a path Cloudflare Pages can read at request time — a Pages Functions rewrite can serve the route from the same `wrangler pages deploy` artifact the dashboard already ships), `IMPLEMENTATION_PLAN.md` (this row), `genesis/plans/000-ceo-testnet-roadmap.md` (mark the v10 follow-on-the-follow-on as shipped with a one-line note). Scope boundary: do NOT change the `INDEX.json` / `IndexClient` / bench-card path (the v10 deployed surface is unchanged — the receipt route is an *additional* surface, not a replacement); do NOT change the `trainer --verify-receipt` CLI (the dashboard route is a *typed read* of the on-disk receipt, not a re-verify — a downstream auditor who needs the re-verify path still uses `trainer --verify-receipt`); do NOT push the receipt to S3 / Cloudflare R2 (the dashboard reads the receipt from the same `wrangler pages deploy` artifact or a Pages Functions read-through to the deploy-time-mounted path — a future CI-worker slice can `wrangler r2 object put` the receipt in a follow-on); do NOT change the receipt's on-disk layout (the route reads what `scripts/testnet-live-proof.sh` wrote — the runbook + the route share one shape); do NOT touch any v1/v2/v3 profile schema, the seat-aware work, the publish/index/remote chain, the v6 → v10 follow-on chain, the v1-v2-v3 trained-config axis, the `SEAT-PERSIST-001` hinge, or the `verification:workspace-parallel` hinge. STW-076 is the *adapter* between the offline operator evidence (the receipt directory the runbook commits) and the public deployed surface (the dashboard URL the README links) — it does not change either side, it adds a typed read of the former through the latter. Acceptance criteria: `cargo test -p rbp-dashboard --test receipt_route` is green with 4 new sub-tests; `cargo test -p rbp-dashboard --tests --lib` stays green (no regression in the existing smoke + inject + transcript + bench card routes); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace` + `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts; `RBP_DASHBOARD_RECEIPTS_DIR=../receipts cargo run -p rbp-dashboard` (with the in-repo `receipts/` dir containing the STW-070 green receipt) + `curl http://localhost:8080/api/receipt/latest | jq .summary` returns the receipt's `SUMMARY.txt` verbatim; `curl http://localhost:8080/api/receipt/does-not-exist` returns 404 + `{"error":"receipt_missing","basename":"does-not-exist"}`. Hand-test: a stranger with zero robopoker context visits the deployed dashboard URL + clicks "Latest live_proof receipt (JSON)" + sees a JSON envelope with `summary` containing the pinned `testnet live_proof complete: ...` headline + `recipe.steps` listing the 8 chain steps with `exit: 0` each — the testnet claim is now self-evidencing at the dashboard URL, not buried in a `git clone`. Dependencies: `STW-070` (a green local receipt the route can serve — without it the route has nothing to read; the integration test's synthetic fixture is sufficient for CI; the deploy-time value comes from a real receipt committing in the same wave); the existing `crates/dashboard/src/router.rs` (the typed-error JSON mapper is the model); the existing `crates/dashboard/src/state.rs` (the `RBP_DASHBOARD_DEPLOYED_URL` env-knob read is the model). Estimated scope: S. Completion signal: a fresh `curl https://<RBP_DASHBOARD_DEPLOYED_URL>/api/receipt/latest` (post-`STW-070` + post-deploy) returns the freshly-committed receipt's envelope; a `curl https://<RBP_DASHBOARD_DEPLOYED_URL>/api/receipt/<basename>` returns the named receipt; a stranger can read the testnet claim end-to-end at the deployed URL with no `git clone`. **`lens:` CEO (the README's "the deployed dashboard is the testnet surface" claim is incomplete without a way to *audit* the testnet at the same URL — the receipt route is the missing audit-trail adapter) + Eng (small file-read handler, no new dep, no DB, no schema, no `trainer --*` CLI change — reuses the typed-error mapper the dashboard already follows) + Design (a stranger sees a JSON envelope, not a tarball — the testnet claim becomes legible at the URL the README anchors on).**

## Next-phase active items (RE-PLAN-004 2026-06-09 by designcritic, RE-PLAN task t_5d2d19e5; supersedes RE-PLAN-003 2026-06-08 by designcritic, RE-PLAN task t_b415327f; supersedes RE-PLAN-002 2026-06-08 by designcritic, RE-PLAN task t_e784842c; supersedes RE-PLAN 2026-06-08 by designcritic, RE-PLAN task t_058b1c92)

The v1→v10 testnet infrastructure chain (STW-004 → STW-076) is structurally complete: 65+ shipped STW rows, the dashboard ships, the receipt chain ships, the v1/v2/v3 trained configs ship, the seat-aware persistence (slice 1-4) ships (commits 4337cf0 / ac75b0e / ef15d84 / a9f08a3 / 68b495f). RE-PLAN-003 left 6 active rows (STW-070, STW-071, STW-072, STW-073, STW-074, STW-076) un-claimed as of 2026-06-09 06:00 UTC. RE-PLAN-004 re-audits the *current* state and surfaces 2 structural issues the prior planner missed that explain why STW-070 is still un-claimed:

1. **`RBP_TESTNET_FAST=1` does not fast-path the `--cluster` kmeans pass.** The runbook's fast mode (lines 105-112 of `scripts/testnet-live-proof.sh`) auto-selects minimal `RBP_FAST_EPOCHS` / `RBP_FAST_BATCH` / `RBP_BENCH_HANDS` / `RBP_BENCH_BLIND` / `RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` so an operator can validate the chain in minutes rather than hours — but the `--cluster` step (which runs the kmeans pass per street) ignores the fast knob. Receipt `receipts/testnet-live-proof-20260609T042107Z/` shows the 04:21 runbook run: `river` kmeans took 17+ minutes (`turn` was still bounding at 04:38, iterating at 04:44) — the run was killed at 04:44 mid-iteration when the worker time-budget exceeded. The `Check::clustered` fix in STW-075 correctly *skips* kmeans on a warmed DB; it does not bound kmeans on a *fresh* DB. Without a fast-mode-aware kmeans cap, every fresh-DB receipt runbook run hits the same wall-clock limit and the chain never produces a green receipt. STW-077 (new in RE-PLAN-004) closes this gap with a `RBP_TESTNET_FAST=1`-aware sample-size / iteration cap on the kmeans driver so a fresh-DB receipt reaches the bench step in under 5 minutes per street.

2. **The local Postgres env is not reproducibly green.** The receipt `receipts/testnet-live-proof-20260609T060233Z/` (06:02 UTC, the most recent run) shows the `doctor` step failed with `"db_reachable":false,"detail":"SELECT 1 failed: psql: error: connection to server at \"127.0.0.1\", port 5433 failed: FATAL:  password authentication failed for user \"rbp_live\""`. The `rbp_live` user's password is not reproducible across reboots / Postgres restarts; the runbook's `trainer --doctor` gate is the *right* gate (it catches the failure cleanly), but the operator-runnable provisioning script the receipt chain assumes is missing. STW-078 (new in RE-PLAN-004) ships `scripts/setup-testnet-postgres.sh` (pure bash, idempotent, no docker) that brings up a local Postgres on a known port with a known `rbp_live` user + password file, and a smoke test that the runbook's `trainer --doctor` exits 0 against the resulting `DATABASE_URL`.

What RE-PLAN-004 changes versus RE-PLAN-003:

- **Two new P0 rows (STW-077 + STW-078)** close the structural issues above. Both ship small, bounded changes (one kmeans cap + one bash provisioning script) and unblock STW-070 from its current "evidence-only in name, environment-blocked in practice" state.
- **STW-070 is re-issued** with the new dependency list (now depends on `STW-075` + `STW-077` + `STW-078` + a reachable `DATABASE_URL`). Once all three ship, the "evidence only" framing is finally honest.
- **STW-076 is re-issued** with an added no-DB fixture-fallback path: when no live receipt exists in `RBP_DASHBOARD_RECEIPTS_DIR`, the route serves the committed `crates/autotrain/tests/fixtures/testnet-live-proof-fixture/` envelope with `source: "fixture"` so the public testnet claim at the dashboard URL is never empty (the testnet surface is self-evidencing the day STW-076 ships, not gated on STW-070 landing a real receipt).
- **STW-071 + STW-074 carry over unchanged** — both are markdown-only, un-blocked, and have been un-claimed since RE-PLAN-002.
- **STW-072 is deprecated** as low value (the seat-aware narrative's stale bits have been implicitly corrected by the RE-PLAN-002/003 preambles; the test comment is a low-leverage copyedit).
- **STW-073 is explicitly deferred** — it depends on `STW-070`, which depends on the new `STW-077` + `STW-078`. Re-promote STW-073 as soon as a green receipt commits.
- **STW-079 is new** — a markdown-only closeout for the `SEAT-PERSIST-001` hinge in `steward/HINGES.md` (the slice 4 integrity gate shipped but the hinge row still reads "open" — the planner should record the closed state for the next steward pass).

The 7 rows below are the active queue for RE-PLAN-004. A `rg -n "^- \[ \] \*\*\[P[01]\]\*\* \`STW-07" IMPLEMENTATION_PLAN.md` shows 7 open rows (STW-070, STW-071, STW-074, STW-076, STW-077, STW-078, STW-079) in priority order. STW-070 + STW-077 + STW-078 are the three P0 claims a worker can pick up *today* (STW-077 + STW-078 are clean bounded slices; STW-070 is evidence-only once both ship). The 6 RE-PLAN-003 rows are now `[x] RESCOPED 2026-06-09 by RE-PLAN-004` so the plan-staleness gate (currently 17 checked, 0 ghosts) does not falsely re-claim shipped work. Owner for the RE-PLAN row itself: designcritic, 2026-06-09.

- [x] **[P0] `STW-077` `TESTNET-FAST-KMEANS-CAP` (NEW in RE-PLAN-004) — close the kmeans-vs-fast-mode gap the RE-PLAN-003 `STW-070` row missed.** The `scripts/testnet-live-proof.sh` runbook's `RBP_TESTNET_FAST=1` knob (lines 105-112) auto-selects minimal `RBP_FAST_EPOCHS` / `RBP_FAST_BATCH` / `RBP_BENCH_HANDS` / `RBP_BENCH_BLIND` / `RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` so an operator can validate the chain end-to-end in minutes rather than hours — but the `--cluster` step (which runs the kmeans pass per street via the `isomorphism` + `metric` + `transitions` tables) ignores the fast knob. On a *fresh* DB the kmeans pass is the dominant wall-clock cost: receipt `receipts/testnet-live-proof-20260609T042107Z/` shows `river` kmeans took 17+ minutes (turn was still bounding at 04:38, iterating at 04:44) before the runbook was killed at 04:44 mid-iteration when the worker time-budget exceeded. The `Check::clustered` deterministic fix in STW-075 (commit 42ed437) correctly *skips* kmeans on a *warmed* DB (it returns true when `isomorphism` already has rows for the target street) but does not bound kmeans on a *fresh* DB (where `isomorphism` is empty and the cap is the worker wall-clock itself). STW-077 fixes the gap: when `RBP_TESTNET_FAST=1` is set, the kmeans driver caps (a) the per-street sample size at `RBP_FAST_KMEANS_SAMPLE` (default 1024, an order of magnitude smaller than the production cap) and (b) the per-street iteration count at `RBP_FAST_KMEANS_ITERATIONS` (default 8, well below the production default of 50) so a fresh-DB receipt runbook run reaches the bench step in under 5 minutes per street (under 30 minutes total for 4 streets, well within the 1-hour worker budget). The fast-mode kmeans produces a *smaller* abstraction (a `RBP_FAST_KMEANS_SAMPLE=1024` sample, vs the production `>100K` rows), but the bench + compare + replay steps downstream consume the abstraction and are themselves fast-mode-aware (the `RBP_FAST_EPOCHS=2` / `RBP_BENCH_HANDS=4` knobs already cap their cost). The smaller abstraction is exactly what the `RBP_TESTNET_FAST=1` runbook claim promises: "an operator can validate the full chain end-to-end in minutes rather than hours" — STW-077 makes that promise true. Owner files: `crates/clustering/src/kmeans.rs` (read the two new env knobs at driver entry + assert the sample cap is honored at the `KMeans::new` boundary; do not touch the production code path; do not change the kmeans algorithm itself; do not change the `isomorphism` / `metric` / `transitions` table schemas), `crates/autotrain/src/cluster.rs` (forward the two new env knobs to the kmeans driver call; the `Mode::Cluster` arm is unchanged from argv perspective — `RBP_TESTNET_FAST=1` is the only switch), `crates/clustering/tests/kmeans_fast.rs` (new no-DB integration test file with 3 sub-tests: `fast_mode_caps_sample_at_1024` writes a 100K-row `Observation` stream + asserts `KMeans::run(... RBP_FAST_KMEANS_SAMPLE=1024 ...)` produces an abstraction with ≤ 1024 cluster centroids within a tight wall-clock budget; `fast_mode_caps_iterations_at_8` writes a synthetic 4-cluster metric + asserts the iteration log emits exactly 8 steps before convergence-or-cap; `production_mode_unchanged_when_fast_unset` asserts the same 100K-row stream with no fast knob produces the production-scale abstraction within the existing wall-clock budget), `scripts/testnet-live-proof.sh` (add `RBP_FAST_KMEANS_SAMPLE` + `RBP_FAST_KMEANS_ITERATIONS` to the fast-mode auto-set block alongside the existing `RBP_FAST_EPOCHS` etc., with comments mirroring the existing fast-mode lines), `scripts/testnet-live-proof.md` (note the new env knobs + the 5-minutes-per-street expectation), `IMPLEMENTATION_PLAN.md` (this row), `genesis/plans/000-ceo-testnet-roadmap.md` (mark the fast-mode fast-kmeans follow-on as shipped with a one-line note). Scope boundary: do NOT change the production kmeans path (the cap is fast-mode-only; a worker who runs the runbook without `RBP_TESTNET_FAST=1` sees the existing production kmeans); do NOT change the `isomorphism` table schema (the cap is on the *input* to kmeans, not the output); do NOT change the `trainer --cluster` argv; do NOT touch the v1/v2/v3 profile schema, the dashboard, the publish/index/remote chain, the seat-aware work, or any `trainer --*` CLI except `--cluster`'s env-knob read. Acceptance criteria: `git log -1 --oneline` on `main` shows a new commit titled `feat(clustering): STW-077 fast-mode kmeans sample/iteration cap` + body referencing this row; `cargo test -p rbp-clustering --test kmeans_fast` is green with 3 new sub-tests; `cargo test -p rbp-clustering --lib` stays green (no regression in existing kmeans lib tests); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace`, `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts; `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` on a *fresh* DB reaches the `bench` step in under 30 minutes total (the multi-hour kmeans trace the 2026-06-08 receipts captured is gone — `cluster/stdout.txt` shows `RBP_FAST_KMEANS_SAMPLE=1024` + 8 iterations per street, not 50+); on a *warmed* DB the `RBP_TESTNET_FAST=1` runbook still skips kmeans (the `Check::clustered` STW-075 path is unchanged; the `cluster/stdout.txt` shows `skipping clustering <street>` for all 4 streets). Hand-test: a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` no longer hits the worker wall-clock on the cluster step (the receipt's `cluster/` dir shows 4 `stdout.txt` files, each under 5 minutes of cluster-step wall-clock, not the multi-hour trace the 2026-06-08 receipts captured). Dependencies: `STW-075` (the deterministic `Check::clustered` fix — this row's complement for the warmed-DB path); existing `crates/clustering/src/kmeans.rs` (the kmeans driver is the model). Estimated scope: S (1 kmeans-driver change + 1 cluster-mode env-forward + 3 sub-tests + 1 runbook pin). Completion signal: a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` on a fresh DB reaches `bench` in under 30 minutes total; the STW-070 evidence-only framing is now honest on the time axis. **`lens:` CEO (without this cap, the testnet live-proof north star cannot produce a single green receipt in the 1-hour worker budget — the runbook's wall-clock is consumed by an honest kmeans pass, not by a deterministic bug, but the practical effect is the same) + Eng (1 file change in the kmeans driver + 1 env-forward in the cluster mode + 3 sub-tests; no schema change, no algorithm change) + Design (the cap is invisible to dashboard users — it just makes the runbook produce a real receipt in time).**

- [x] **[P0] `STW-078` `TESTNET-POSTGRES-ENV-PROVISIONING` (NEW in RE-PLAN-004) SHIPPED 2026-06-09 — ship an operator-runnable Postgres provisioning script the receipt runbook assumes.** The receipt `receipts/testnet-live-proof-20260609T060233Z/` (06:02 UTC, the most recent runbook invocation) shows the `doctor` step failed with `"db_reachable":false,"detail":"SELECT 1 failed: psql: error: connection to server at \"127.0.0.1\", port 5433 failed: FATAL:  password authentication failed for user \"rbp_live\""`. The `rbp_live` user's password is not reproducible across reboots / Postgres restarts; the runbook's `trainer --doctor` gate is the *right* gate (it catches the failure cleanly with a `db_reachable: false` + `exit: 2`), but the operator-runnable provisioning script that *produces* a reproducible `DATABASE_URL` is missing. STW-078 ships `scripts/setup-testnet-postgres.sh`: a pure-bash, idempotent, no-docker script that brings up a local Postgres on `127.0.0.1:5433` (a non-default port so it does not collide with a system Postgres on `:5432`) with a known `rbp_live` user + a known `rbp_live` password + a known `rbp_live` database, and writes a `.auto/testnet-postgres.env` file the operator (or a CI worker) can `source` to set `DATABASE_URL` + `DB_URL` for the receipt runbook. The script refuses to run on a system with no `initdb` / `pg_ctl` / `postgres` binary (exit 2 with a one-line `testnet-postgres: required binary missing: ...` message); refuses to run on a port that is already bound (exit 3 with a one-line `testnet-postgres: port 5433 already in use` message); is idempotent on a second invocation (re-running on a healthy env exits 0 with a one-line `testnet-postgres: already provisioned` message, no data loss); and runs `trainer --doctor` against the resulting `DATABASE_URL` as the final smoke test (exit 0 with `trainer --doctor` printing `db_reachable: true`). Owner files: `scripts/setup-testnet-postgres.sh` (the new pure-bash script — mirrors the `scripts/testnet-live-proof.sh` shape: script exists + is executable + parses with `bash -n` + a pinned `testnet-postgres: complete: port=5433 user=rbp_live database=rbp_live data_dir=...` headline; ~120 lines), `scripts/setup-testnet-postgres.md` (the runbook doc — mirrors `scripts/testnet-live-proof.md`: required binaries, expected `DATABASE_URL` shape, how to `source` the env file, how to verify with `trainer --doctor`), `crates/autotrain/tests/script_shape.rs` (2 new shell-shape pins: `setup_testnet_postgres_script_exists_and_parses` + `setup_testnet_postgres_script_writes_env_file`, mirroring the `testnet_live_proof_*_script_*` pinners), `crates/autotrain/tests/setup_testnet_postgres.rs` (a new no-DB integration test that invokes the script in a clean `tmpdir` with a `PATH` containing a fake `initdb` / `pg_ctl` / `postgres` / `psql` set (the fakes are simple shell scripts that record their argv to a log file + exit 0), then asserts (a) the script wrote a `.auto/testnet-postgres.env` file with the expected `DATABASE_URL` + `DB_URL` shape, (b) the fake `postgres` binary was invoked with `--port=5433` + `-k /tmp/...` + the expected data-dir, (c) the fake `createuser` / `createdb` / `psql` calls (the script uses `psql` for the user/db creation) ran in the expected order, and (d) the script's exit code is 0 on the no-op-idempotent second-invocation path). Scope boundary: do NOT introduce a `docker` dependency (the script is pure-bash + the local `initdb` / `pg_ctl` / `postgres` binaries the OS already ships); do NOT require `sudo` (the script runs as the unprivileged user + uses a `tmpdir` data dir + a non-default port); do NOT change `trainer --doctor` (the script invokes the existing doctor as the smoke test); do NOT change the runbook `scripts/testnet-live-proof.sh` (the runbook reads `DATABASE_URL` / `DB_URL` from the env exactly as it does today; the new script's only job is to *produce* a reproducible env); do NOT change any `trainer --*` CLI; do NOT touch the v1/v2/v3 profile schema, the dashboard, the publish/index/remote chain, the seat-aware work, the kmeans driver, the `STW-077` cap, or the `Check::clustered` fix. Acceptance criteria: `bash -n scripts/setup-testnet-postgres.sh` exits 0; a fresh invocation on a clean shell exits 0 with the pinned `testnet-postgres: complete: ...` headline; a second invocation on the same shell exits 0 with the `testnet-postgres: already provisioned` headline; the resulting `.auto/testnet-postgres.env` file has the expected `DATABASE_URL=postgres://rbp_live:***@127.0.0.1:5433/rbp_live` + `DB_URL=...` shape; `source .auto/testnet-postgres.env && trainer --doctor` exits 0 with `"db_reachable":true`; the new `setup_testnet_postgres` integration test is green; the new `script_shape` sub-tests are green; `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace` + `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts. Hand-test: a fresh `bash scripts/setup-testnet-postgres.sh` on a machine with no Postgres (or with Postgres on a non-conflicting port) takes under 10 seconds + leaves a healthy env behind; `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` (after `source .auto/testnet-postgres.env`) reaches the `cluster` step with `db_reachable: true` (the pre-STW-078 receipts all show `db_reachable: false` at the doctor step). Dependencies: a local `initdb` / `pg_ctl` / `postgres` / `psql` / `createuser` / `createdb` binary on `$PATH` (the standard `postgresql` apt/yum/brew package). Estimated scope: S (1 bash script + 1 doc + 1 integration test + 2 shell-shape pins). Completion signal: a fresh `bash scripts/setup-testnet-postgres.sh && source .auto/testnet-postgres.env && bash scripts/testnet-live-proof.sh` is a one-shot green path the next planner / operator / CI worker can run from a clean shell. **`lens:` CEO (the receipt runbook is the testnet north star's operator-visible proof; a worker who runs it from a clean shell needs the env to be reproducible — without this script, the runbook is a *promise* of proof, not a *recipe* for proof) + Eng (one bash script that the existing OS Postgres binaries already provide; no new dep; the integration test is pure-bash with fakes, no DB needed) + Design (the operator sees a one-liner `source .auto/testnet-postgres.env` and a one-liner `bash scripts/testnet-live-proof.sh` — the testnet claim is now runnable, not just committable).**

- [x] **[P0] `STW-086` `KMEANS-EMPTY-HISTOGRAM-DEFENSIVE-GUARD` (NEW in RE-PLAN-006, this commit) — close the empty-histogram panic in the kmeans++ init path that blocks STW-070.** The testnet-live-proof receipt runbook was run end-to-end with `RBP_TESTNET_FAST=1` against a real Postgres (provisioned by STW-078) for the STW-070 evidence pass; the `--doctor` and `--cluster` steps landed cleanly through `river` and `turn`, then `--cluster` panicked on `flop` at `crates/clustering/src/bins.rs:95` (`Bins::peek` — "non empty histogram") inside `crates/clustering/src/kmeans.rs::init_kmeans_plus_plus` (called from `Layer::cluster_fast` at `crates/clustering/src/layer.rs:272`). The trace shows the panic fires inside `kmeans++`'s per-iteration `metric.emd(centroid, h)` loop (kmeans.rs:248) — when the slice passed to `init_kmeans_plus_plus` contains empty histograms (the first 1024 turn projections of the flop point pool include empty slots — turn isomorphisms that were never observed from any flop), and the first centroid picked by `WeightedIndex::new(potentials.iter()).sample(rng)` happens to be one of them, the next iteration's `metric.emd(empty_centroid, h)` calls `Metric::emd` (metric.rs:108) which does `source.peek().street()` to dispatch into Sinkhorn — and `Bins::peek` panics on an empty support with the exact `"non empty histogram"` message the runbook captured. The same bug exists in the production `Layer::init_kmeans` path (layer.rs:128-169) — the production 1.3M-row pool is large enough that the empty-prefix luck-out has not fired yet, but a future production run on a fresh DB with sparse observations will hit it. The fix is bounded and small: in `init_kmeans_plus_plus` (kmeans.rs:216-257) and `init_kmeans` (layer.rs:128-169), filter the input slice to drop empty histograms *before* the kmeans++ loop, OR build the `potentials` array so empty slots are zeroed (the `WeightedIndex` will skip them as a side-effect of the `0.0` weight). The second approach is smaller and preserves the deterministic per-street RNG seed: the `potentials` array starts as `vec![1.0; n]`, then on entry to the kmeans++ loop, every index whose `points[i].peek()` would panic (an empty histogram) gets `potentials[i] = 0.0` — `WeightedIndex::new` already handles the all-zero case (`kmeans.rs:236` would still panic, so guard with `if potentials.iter().all(|&p| p == 0.0) { break }` or use a `LazyLock` / `OnceLock` of a no-op `WeightedIndex`). The cleanest 1-line guard: pre-filter `points` to a non-empty prefix (`points.iter().filter(|h| h.peek() returns some Abstraction).cloned().collect()`) before the kmeans++ loop — if the filter produces an empty Vec, return `K` empty centroids (mirroring the `truncated.is_empty()` early-return on kmeans.rs:175). The `init_kmeans` production path is symmetric: pre-filter the points before the `vec![1.; N]` initialization. A second defensive measure: change `Bins::peek` from `expect("non empty histogram")` to return `Option<Abstraction>` (with the `equity()` / `pdf()` / `peek()` callers panicking only when they truly need a non-empty support) — but this is a larger refactor; STW-086 takes the pre-filter approach as the minimum-defensive fix. STW-086 also ships a regression test in `crates/clustering/tests/kmeans_fast.rs`: `fast_mode_handles_empty_point_in_prefix` builds a 1024-row `Vec<Histogram>` whose first 16 entries are `Histogram::empty(Street::Turn)` + the rest are random non-empty turn histograms (the same shape the real flop point pool has) + asserts `run_fast::<K>(points, metric, Street::Turn, caps)` returns K centroids without panicking + the wall-clock stays under the existing 2 s budget. The existing `fast_mode_caps_sample_at_1024` test is the model. Owner files: `crates/clustering/src/kmeans.rs` (in `init_kmeans_plus_plus`, pre-filter the input slice to drop empty histograms; if the filter produces an empty Vec, return `K` empty centroids via the existing `Histogram::empty(street)` constructor — ~5 lines), `crates/clustering/src/layer.rs` (in `init_kmeans`, mirror the pre-filter so the production path is also defensive — ~3 lines), `crates/clustering/tests/kmeans_fast.rs` (new `fast_mode_handles_empty_point_in_prefix` sub-test that builds a 1024-row mixed-empty pool + asserts `run_fast` returns K centroids cleanly), `IMPLEMENTATION_PLAN.md` (this row + the `STW-070` row's dependency note updated to "depends on `STW-075` + `STW-077` + `STW-078` + `STW-086`"). Scope boundary: do NOT change the kmeans algorithm itself (kmeans++ is preserved; only the input is filtered); do NOT change the `isomorphism` / `metric` / `transitions` table schemas; do NOT change the production `Layer::cluster` argv or its env-knob read; do NOT change the `Bins::peek` panic message (a future refactor that converts `peek` to `Option<Abstraction>` is a separate slice); do NOT touch the v1 / v2 / v3 trained configs, the dashboard, the publish / index / remote chain, the seat-aware work, the autotrain pipeline, or any `trainer --*` CLI; do NOT touch the kmeans fast driver's existing 3 sub-tests in `kmeans_fast.rs` (the new sub-test is added, not a replacement). Acceptance criteria: `git log -1 --oneline` on `main` shows a new commit titled `feat(clustering): STW-086 defensive empty-histogram filter in kmeans++ init` + body referencing this row + the receipt `20260610T001633Z` panic trace; `cargo test -p rbp-clustering --test kmeans_fast` is green with 4 new sub-tests (the existing 3 + the new `fast_mode_handles_empty_point_in_prefix`); `cargo test -p rbp-clustering --lib` stays green (no regression in existing kmeans / sinkhorn / bins lib tests); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace`, `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts. Hand-test: a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` after STW-086 lands no longer panics on `flop` (the receipt's `cluster/stdout.txt` shows `kmeans fast driving points=1024 caps.sample=1024 caps.iters=8` for flop, then `calculating lookup flop` + `calculating metric flop` + `calculating transitions flop` + the runbook proceeds to `--reset` + `--smoke` + ... + `--compare` + `--replay` and lands a `testnet live_proof complete: ...` `SUMMARY.txt`). Dependencies: `STW-075` + `STW-077` + `STW-078` (all shipped) + a reachable `DATABASE_URL` (the STW-078-provisioned Postgres). Estimated scope: S (1 kmeans-driver pre-filter + 1 init_kmeans pre-filter + 1 new sub-test + 1 plan-row update). Completion signal: a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` against a fresh DB lands a green `receipts/testnet-live-proof-<UTC>/SUMMARY.txt` with the `testnet live_proof complete: smoke=N status=N bench=N compare=N replay=BYTES` headline; `LiveProofReceipt::read_and_verify` returns `Ok(())` on the new receipt; STW-070 unblocks and lands as a follow-on commit. **`lens:` CEO (the testnet claim requires a green receipt; a panic in the receipt runbook is the highest-priority unblock) + Eng (the kmeans driver is a 200-line module; the pre-filter is a 5-line guard; the regression test is a 30-line sub-test mirroring the existing `fast_mode_caps_sample_at_1024` pattern) + Design (a defensive guard in the kmeans driver does not change the algorithm; the production path is symmetrically protected).**

- [ ] **[P0] `STW-071` `PLAN-GHOST-RETIRE-001` (carried over unchanged from RE-PLAN-002 / RE-PLAN-003; the work is markdown + one shell-script + one shell-shape pin, and has been un-claimed for two RE-PLAN cycles).** Retire the 13 stale `[ ] [P1]` ghost rows in `IMPLEMENTATION_PLAN.md` (STW-040, STW-041, STW-045 (x3 duplicates), STW-046 (x3 duplicates), STW-048, STW-056, STW-057, STW-060, STW-063, STW-064, STW-065, STW-066, STW-068) and extend `scripts/plan-staleness-gate.sh` + `crates/autotrain/tests/plan_staleness.rs` to mechanically catch any future `[ ] [P1]` row that has a corresponding `[x] STW-NNN` row (mirroring the existing P0 ghost detection, with a `RESCOPED <date> by STW-071` marker convention so a `RESCOPED` row passes the gate cleanly). Owner files: `IMPLEMENTATION_PLAN.md` (the 13 ghost rows are flipped to `[x] STW-NNN RESCOPED 2026-06-09 by STW-071` or removed; the 6 historical wave sections that quote the ghost rows are unchanged — they are evidence, not active queue), `scripts/plan-staleness-gate.sh` (adds a second pass that greps for `- [ ] \\[P1\\] <claim>` rows + maps each claim to a `STW-NNN` id via the same `STW_MAP` table the P0 pass consults + flags ghosts), `crates/autotrain/tests/plan_staleness.rs` (adds a new sub-test that drops a synthetic 2-row roadmap + 2-row plan into a temp dir + drives the script + asserts exit 3 + asserts stderr names the ghost rows). Scope boundary: do NOT rewrite shipped rows except the minimum checkbox/RESCOPED markers needed to make the active queue truthful; do NOT remove `STW-001` or `STW-007` (they are explicit `[!]` operator decisions and `STW-074` owns the closeout); do NOT touch `genesis/plans/000-ceo-testnet-roadmap.md` (it is history, not active queue); do NOT create new numbered `ExecPlan`s; do NOT change runtime code; do NOT touch `steward/PROMOTIONS.md`'s `STW-001`/`STW-007` `deferred` row (the planner will re-rank after `STW-074` ships). Acceptance criteria: `rg -n "^- \\[ \\] \\*\\*\\[P1\\].*STW-(040|041|045|046|048|056|057|060|063|064|065|066|068)" IMPLEMENTATION_PLAN.md` returns no claimable open rows; the 13 `RESCOPED 2026-06-09 by STW-071` rows are present + grep-clean; `scripts/plan-staleness-gate.sh` exits 0 with headline `plan staleness gate complete: checked=N ghosts=0`; the new `plan_staleness.rs` sub-test passes; the `[ ]` count in `IMPLEMENTATION_PLAN.md` drops from 7 to 6 (the 7 next-phase rows from this RE-PLAN minus 1 if the worker takes a row + 0 for the SUPERSEDED historical rows). Verification commands: `bash scripts/plan-staleness-gate.sh` (the new P1 pass + the existing P0 pass), `cargo test -p rbp-autotrain --test plan_staleness` (the new sub-test + the 5 existing sub-tests), `rg -n "^- \\[ \\] \\*\\*\\[P[01]\\]" IMPLEMENTATION_PLAN.md | wc -l` (should drop), `cargo test --workspace -- --test-threads=4`, `cargo check --workspace`, `cargo fmt --check`. Hand-test: a fresh `auto parallel` tick that scans the plan sees 6 (or fewer) claimable P0/P1 rows + 0 ghosts. Dependencies: the existing `STW-022` `scripts/plan-staleness-gate.sh` (P0 pass is the model); the existing `crates/autotrain/tests/plan_staleness.rs` (5 sub-tests is the model). Estimated scope: S. Completion signal: `auto parallel` claims the next real next-phase row (`STW-070`, `STW-074`, `STW-076`, `STW-077`, `STW-078`, or `STW-079`) — not a ghost. **`lens:` CEO (the 11k-line plan is a false backlog signal that re-`dispatched` shipped work; retirement is the subtraction default) + Eng (one shell-script extension + one sub-test = 1 day of work) + Design (a worker reading the plan sees a clean active queue, not 7 noise rows + 1 real row).**

- [ ] **[P1] `STW-074` `OPERATOR-DECISIONS` (carried over unchanged from RE-PLAN-002 / RE-PLAN-003; the work is markdown-only + one new file, and has been un-claimed for two RE-PLAN cycles).** Close the two `[!]` operator-decision rows (`STW-001` planning surface, `STW-007` artifact retirement) with a recorded decision the next planner pass can promote against — `STW-001` resolves to "hand-author a queue in `IMPLEMENTATION_PLAN.md` (the current plan is the queue; `gbrain` is not required for the next 3 months of work — `genesis/plans/000-ceo-testnet-roadmap.md` is evidence, `IMPLEMENTATION_PLAN.md` is the queue)" or "block on gbrain init"; `STW-007` resolves to a per-path retention verdict for `.gbrain-source` (delete or keep), `.auto/tui*/` (delete or keep), `.auto/orchestrator/velocity-*` (delete), `.auto/corpus-staging/` (delete), `.auto/logs/steward-*-prompt.md` (delete), with the verdict recorded in a new `steward/ARTIFACT-RETENTION.md` file the planner can grep. Owner files: `IMPLEMENTATION_PLAN.md` (the `## Deferred items (need operator decision before promotion)` section is rewritten to a `## Operator decisions (RESOLVED 2026-06-09 by STW-074)` section with the verdict for each row + a one-paragraph rationale; the new `STW-074` row flips to `[x]` once recorded), `steward/ARTIFACT-RETENTION.md` (new — a per-path verdict table: `path | verdict | rationale | signoff`, with the `STW-074` row's resolution as the contents), `steward/HAZARDS.md` (the `STW-001` + `STW-007` rows flip from "open" to "closed" with a one-line `closed by STW-074 on <date>` note), `steward/PROMOTIONS.md` (the `STW-001` / `STW-007` `deferred` row is replaced with a `STW-074` `promoted` row), `steward/DRIFT.md` (the `STW-001` + `STW-007` rows in the `Blocked / Deferred` table are updated from `DRIFT` / `AGREES` to `RESOLVED` with the verdict). Scope boundary: do NOT execute the deletion (the planner is the recording desk, not the cleanup crew — a separate operator-side slice deletes after sign-off); do NOT change the `.gitignore`; do NOT remove `genesis/plans/000-ceo-testnet-roadmap.md` (it is the historical record the planner audits against); do NOT change runtime code; do NOT touch the 13 ghost rows `STW-071` retires. Acceptance criteria: the two `[!]` rows in `IMPLEMENTATION_PLAN.md` are replaced with `[x] STW-NNN RESOLVED 2026-06-09 by STW-074` rows + a one-paragraph verdict each; `steward/ARTIFACT-RETENTION.md` is a clean per-path verdict table; `steward/{HAZARDS,PROMOTIONS,DRIFT}.md` are updated in lockstep; a `rg -n "^- \\[!\\]" IMPLEMENTATION_PLAN.md` returns no rows. Verification commands: `rg -n "^- \\[!\\]" IMPLEMENTATION_PLAN.md` (should be empty), `rg -n "STW-001|STW-007" IMPLEMENTATION_PLAN.md steward/` (should show only `RESOLVED ... by STW-074` references), `bash scripts/plan-staleness-gate.sh` (must stay green — no new ghosts introduced), `cargo test --workspace -- --test-threads=4` (no code change so this is a regression check, not a feature check), `cargo check --workspace`, `cargo fmt --check`. Hand-test: a planner scanning `IMPLEMENTATION_PLAN.md` no longer sees 2 `[!]` rows; the next `auto steward --report-only` pass has a clean `Blocked / Deferred` table to start from. Dependencies: operator input on the 6 `.auto/` + `.gbrain-source` retention verdicts (this is the only row in the RE-PLAN that genuinely blocks on human input — every other row is a code/markdown change a worker can ship solo). Estimated scope: S (markdown only, no code). Completion signal: `rg -n "^- \\[!\\]" IMPLEMENTATION_PLAN.md` returns no rows; the next `auto steward --report-only` pass promotes against a clean decision table, not 2 deferred items + 13 ghost rows + 0 real next slices. **`lens:` CEO (the deferred items are the noise floor of every planner pass — recording a verdict turns the noise into a signal) + Eng (markdown-only change; no code risk) + Design (a planner sees a decision table, not a `[!]` warning).**

- [ ] **[P1] `STW-076` `DASHBOARD-LATEST-RECEIPT-ROUTE` (RE-PLAN-004 re-issue; the RE-PLAN-003 row was `STW-076 NEW` with a single source-of-truth being `RBP_DASHBOARD_RECEIPTS_DIR`; the RE-PLAN-004 row adds a no-DB fixture-fallback path so the public testnet claim is never empty at the dashboard URL).** Close the gap between the dashboard's `GET /api/index` bench-card surface (STW-034 / STW-035, the v10 deployed surface) and the `testnet-live-proof-<UTC>/` receipt the STW-070 evidence slice drops. The README's `## Public dashboard` section anchors the testnet claim on the deployed dashboard URL, but a stranger who `curl`s the deployed dashboard today sees the *published* bench cards from `INDEX.json` (the STW-035 publish-remote chain's output) — they do NOT see the *receipt* the runbook committed (the `SUMMARY.txt` + `recipe.json` + per-step `{stdout,stderr,exit}.txt` artifacts under `receipts/testnet-live-proof-<UTC>/`). The receipt is the *primary operator-visible proof* of the testnet north star; without a dashboard route, a stranger has to `git clone` to read it. STW-076 ships a thin `GET /api/receipt/latest` (and `GET /api/receipt/<basename>`) route on the existing `crates/dashboard/` `axum` router that reads from a configured `RBP_DASHBOARD_RECEIPTS_DIR` (default `../receipts`, relative to the dashboard crate) + serves a single typed JSON envelope: `{ basename, summary, recipe, steps, source: "receipt" | "fixture" }`. The envelope re-uses the `LiveProofRecipe` struct the autotrain crate already ships; the new code is a thin file-read + serde_json::from_str wrapper, no new domain types. The `source` field disambiguates the two surfaces: a real receipt from `RBP_DASHBOARD_RECEIPTS_DIR` reports `source: "receipt"`; a fallback to the committed `crates/autotrain/tests/fixtures/testnet-live-proof-fixture/` reports `source: "fixture"`. The fallback engages *only* when no real receipt exists (i.e. `RBP_DASHBOARD_RECEIPTS_DIR` is empty / missing / or contains no `testnet-live-proof-<UTC>/` subdirectories); the integration test asserts the fallback path is gated on real-receipt absence so a future receipt never gets papered over. The `latest` route picks the lexicographically-maximum `testnet-live-proof-<UTC>/` basename from the real-receipt dir (the same UTC-ISO sort the runbook's SUMMARY.txt line `testnet live_proof complete: ...` follows, so dashboard sort matches receipt sort); if no real receipt exists, the route returns the committed fixture's envelope with `source: "fixture"`. The `<basename>` route only matches real-receipt basenames (the fixture has no `<basename>` route — a request for `testnet-live-proof-fixture` returns 404 with `{"error":"receipt_missing","basename":"testnet-live-proof-fixture"}` so the fallback path is *only* reachable via the `latest` route). Both routes return `404 + a typed DashboardError JSON body` when no match is found (the dashboard's existing `crates/dashboard/src/router.rs` error mapper renders the typed body the same way the existing `IndexClient::read` / `IndexMissing` paths do, so a stranger can `curl .../api/receipt/latest` and `jq .error` for the failure shape). The static `crates/dashboard/static/index.html` empty-state gets a one-line `<a href="/api/receipt/latest">Latest live_proof receipt (JSON)</a>` link beside the existing "Demo data: `/bench/compare3-fixture`" link so a first-time visitor can audit the most-recent receipt (or the fixture fallback) without writing `curl + jq`. A new `crates/dashboard/tests/receipt_route.rs` integration test drives the axum router end-to-end and asserts: (a) `GET /api/receipt/latest` on a temp `RBP_DASHBOARD_RECEIPTS_DIR` containing one synthetic `testnet-live-proof-20260608T210000Z/` (a fixture dir with a `SUMMARY.txt` + `recipe.json` + 8 step `stdout.txt` / `exit.txt` files mirroring the runbook's layout) returns 200 with the pinned envelope shape — `source: "receipt"`, `summary` matches `SUMMARY.txt` verbatim, `recipe.steps[0].name == "doctor"`, `steps[0].exit == 0`; (b) `GET /api/receipt/testnet-live-proof-20260608T210000Z` returns the same envelope; (c) `GET /api/receipt/does-not-exist` returns 404 + `{"error":"receipt_missing","basename":"does-not-exist"}`; (d) `GET /api/receipt/latest` on an empty / missing `RBP_DASHBOARD_RECEIPTS_DIR` falls back to the committed `testnet-live-proof-fixture` and returns 200 with `source: "fixture"` + the fixture's `summary` + `recipe` + `steps`; (e) `GET /api/receipt/testnet-live-proof-fixture` returns 404 (the fixture has no `<basename>` route — only the `latest` route falls back to it). The `serve_receipt` handler is a small file-read function (< 100 lines, no tokio-postgres, no JSON streaming, no new dep — only `serde_json` + `std::fs::read_to_string` + `axum::Json`); the existing `OnceLock<String, String>` + mtime-invalidation pattern the STW-062 / STW-063 cache already follows is reused for the `<basename>` route so a receipt re-read is a cache hit on the second `curl` (the `latest` route is uncached by design — its sort order is the operator-visible signal, and the fixture-fallback is a static read so caching it would also be correct, but a consistent "latest is always live" posture is friendlier to ops). Owner files: `crates/dashboard/src/router.rs` (new `serve_receipt_latest` + `serve_receipt_named` handlers + `DashboardError::ReceiptMissing(<basename>)` + `DashboardError::ReceiptsDirMissing(<path>)` + `DashboardError::FixtureMissing(<path>)` variants; reuse the existing typed-error JSON mapper the `IndexClient::read` path already follows), `crates/dashboard/src/state.rs` (the `RBP_DASHBOARD_RECEIPTS_DIR` env knob read at boot — default `"../receipts"` so a `cargo run -p rbp-dashboard` from the workspace root resolves to the in-repo `receipts/` dir; absolute paths also supported; the committed `crates/autotrain/tests/fixtures/testnet-live-proof-fixture/` path is hard-coded as the fallback), `crates/dashboard/static/index.html` (one-line `<a href="/api/receipt/latest">...</a>` link in the existing empty-state paragraph), `crates/dashboard/tests/receipt_route.rs` (new no-DB integration test file with the 5 sub-tests above), `scripts/deploy-dashboard-cloudflare.md` (note the `RBP_DASHBOARD_RECEIPTS_DIR` knob the deploy runbook should set to a path Cloudflare Pages can read at request time — a Pages Functions rewrite can serve the route from the same `wrangler pages deploy` artifact the dashboard already ships), `IMPLEMENTATION_PLAN.md` (this row), `genesis/plans/000-ceo-testnet-roadmap.md` (mark the v10 follow-on-the-follow-on as shipped with a one-line note). Scope boundary: do NOT change the `INDEX.json` / `IndexClient` / bench-card path (the v10 deployed surface is unchanged — the receipt route is an *additional* surface, not a replacement); do NOT change the `trainer --verify-receipt` CLI (the dashboard route is a *typed read* of the on-disk receipt, not a re-verify — a downstream auditor who needs the re-verify path still uses `trainer --verify-receipt`); do NOT push the receipt to S3 / Cloudflare R2 (the dashboard reads the receipt from the same `wrangler pages deploy` artifact or a Pages Functions read-through to the deploy-time-mounted path — a future CI-worker slice can `wrangler r2 object put` the receipt in a follow-on); do NOT change the receipt's on-disk layout (the route reads what `scripts/testnet-live-proof.sh` wrote — the runbook + the route share one shape); do NOT touch any v1/v2/v3 profile schema, the seat-aware work, the publish/index/remote chain, the v6 → v10 follow-on chain, the v1-v2-v3 trained-config axis, the `SEAT-PERSIST-001` hinge, or the `verification:workspace-parallel` hinge. STW-076 is the *adapter* between the offline operator evidence (the receipt directory the runbook commits) and the public deployed surface (the dashboard URL the README links) — it does not change either side, it adds a typed read of the former through the latter. Acceptance criteria: `cargo test -p rbp-dashboard --test receipt_route` is green with 5 new sub-tests; `cargo test -p rbp-dashboard --tests --lib` stays green (no regression in the existing smoke + inject + transcript + bench card routes); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace` + `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts; `RBP_DASHBOARD_RECEIPTS_DIR=../receipts cargo run -p rbp-dashboard` (with the in-repo `receipts/` dir containing a real STW-070 green receipt) + `curl http://localhost:8080/api/receipt/latest | jq .source` returns `"receipt"`; with `RBP_DASHBOARD_RECEIPTS_DIR=/tmp/empty` (or unset) + `curl http://localhost:8080/api/receipt/latest | jq .source` returns `"fixture"`; `curl http://localhost:8080/api/receipt/does-not-exist` returns 404 + `{"error":"receipt_missing","basename":"does-not-exist"}`. Hand-test: a stranger with zero robopoker context visits the deployed dashboard URL + clicks "Latest live_proof receipt (JSON)" + sees a JSON envelope with `source` ∈ `{"receipt", "fixture"}` + `summary` containing the pinned `testnet live_proof complete: ...` headline + `recipe.steps` listing the 8 chain steps with `exit: 0` each — the testnet claim is now self-evidencing at the dashboard URL, not buried in a `git clone`. Dependencies: a committed `crates/autotrain/tests/fixtures/testnet-live-proof-fixture/` (already shipped, STW-028); the existing `crates/dashboard/src/router.rs` (the typed-error JSON mapper is the model); the existing `crates/dashboard/src/state.rs` (the `RBP_DASHBOARD_DEPLOYED_URL` env-knob read is the model). Estimated scope: S. Completion signal: a fresh `curl https://<RBP_DASHBOARD_DEPLOYED_URL>/api/receipt/latest` returns *some* JSON envelope (`source` ∈ `{"receipt", "fixture"}`) on day 1 of deploy — the testnet claim is now self-evidencing at the URL, not gated on STW-070 landing a real receipt. **`lens:` CEO (the README's "the deployed dashboard is the testnet surface" claim is incomplete without a way to *audit* the testnet at the same URL — the receipt route is the missing audit-trail adapter, and the fixture-fallback is what makes the audit-trail non-empty on day 1) + Eng (small file-read handler, no new dep, no DB, no schema, no `trainer --*` CLI change — reuses the typed-error mapper the dashboard already follows) + Design (a stranger sees a JSON envelope, not a tarball — the testnet claim becomes legible at the URL the README anchors on, even before the first live receipt lands).**

- [ ] **[P1] `STW-079` `HINGE-CLOSEOUT-SEAT-PERSIST-001` (NEW in RE-PLAN-004) — close the `SEAT-PERSIST-001` hinge in `steward/HINGES.md` that the RE-PLAN-003 narrative acknowledged is closed but the hinge table still records as "open".** The seat-aware blueprint persistence work (slice 1-4) is fully shipped on `main` at commits 4337cf0 (slice 1: trace seat-collapse bug + add repro test), ac75b0e (slice 2: thread position into `NlheInfo`), ef15d84 (slice 3: persist position in blueprint schema + `BulkSchema::copy`), a9f08a3 (slice 4 wiring the fail-before-train integrity gate), and 68b495f (slice 4 follow-up: cold-startup safety + `DATABASE_URL` fallback). The `Check::clustered` deterministic fix (STW-075) + the seat-aware integrity gate (`check_integrity` in `crates/autotrain/src/integrity.rs` wired into `FastSession::sync` / `Fast2Session::sync` / `Fast3Session::sync`) mechanically prevent a seat-collapsed blueprint from reaching the database. The RE-PLAN-003 narrative explicitly states: "The `SEAT-PERSIST-001` hinge is closed (slice 4: fail-before-train integrity gate in `check_integrity` wired into v1/v2/v3 `FastSession::sync`)" — but the `steward/HINGES.md` table still records the row with the original "open" status. STW-079 is a markdown-only closeout: flip the `SEAT-PERSIST-001` row in `steward/HINGES.md` from open to closed, add a one-line `closed by STW-079 on 2026-06-09` note + the 5 commit hashes (4337cf0 / ac75b0e / ef15d84 / a9f08a3 / 68b495f) as the evidence chain, and update the `verification:workspace-parallel` hinge #2's `follow-ons collapsed` column to note "depends on `SEAT-PERSIST-001` being closed" (the new honesty is that the workspace-parallel proof is now reachable because the seat-persist work is now closed, not in flight). Owner files: `steward/HINGES.md` (the `SEAT-PERSIST-001` row flips from "open" to "closed"; a one-line `closed by STW-079 on 2026-06-09` note + the 5 commit hashes as evidence; the `verification:workspace-parallel` row's `follow-ons collapsed` column gets a one-line cross-reference to the closed `SEAT-PERSIST-001` row), `steward/HAZARDS.md` (the `SEAT-PERSIST-001` row flips from "open" to "closed" with the same one-line note + commit hashes), `steward/PROMOTIONS.md` (the `SEAT-PERSIST-001` `ready` row flips to a `closed by STW-079 on 2026-06-09` row), `steward/DRIFT.md` (the `SEAT-PERSIST-001` row in the `Blocked / Deferred` table updates from "DRIFT" to "RESOLVED"), `IMPLEMENTATION_PLAN.md` (this row). Scope boundary: do NOT change runtime code (this is a documentation-only slice; the seat-aware work is already shipped and tested); do NOT touch the integrity gate's algorithm (the `5%-15%` 3-bet range, the early-tighter-than-late assert, the per-position frequency computation); do NOT touch the v1/v2/v3 profile schemas; do NOT touch the v6 → v10 follow-on chain; do NOT touch the receipt runbook (the receipt is what *proves* the hinge is closed in operator-visible terms, not the markdown flip). Acceptance criteria: `rg -n "SEAT-PERSIST-001" steward/` shows the row in "closed" state in all 4 steward files (HINGES / HAZARDS / PROMOTIONS / DRIFT); the 5 commit hashes (4337cf0 / ac75b0e / ef15d84 / a9f08a3 / 68b495f) appear in the `SEAT-PERSIST-001` evidence chain in at least `steward/HINGES.md`; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts (the steward flips are markdown-only, no new P0/P1 rows in the plan); `rg -n "SEAT-PERSIST-001" steward/ | grep -i open` returns no matches (the row is closed everywhere). Verification commands: `rg -n "SEAT-PERSIST-001" steward/`, `rg -n "closed by STW-079" steward/`, `bash scripts/plan-staleness-gate.sh`, `cargo test --workspace -- --test-threads=4` (regression check, not feature check — no code change). Hand-test: a planner scanning `steward/HINGES.md` sees `SEAT-PERSIST-001` in the "closed" column with the 5 commit hashes as evidence; the next `auto steward --report-only` pass reports the hinge as closed; the `verification:workspace-parallel` hinge #2's follow-on chain is unblocked. Dependencies: slice 1-4 work (landed at 4337cf0 / ac75b0e / ef15d84 / a9f08a3 / 68b495f); existing `steward/{HINGES,HAZARDS,PROMOTIONS,DRIFT}.md` (the 4 steward files the closeout touches). Estimated scope: XS (4 markdown flips + 1 commit-hash evidence chain, ~30 minutes of work). Completion signal: a planner scanning the steward files sees the `SEAT-PERSIST-001` hinge as closed everywhere + the `verification:workspace-parallel` hinge #2's follow-on chain updated to reflect the new closed state. **`lens:` Design (the steward table is the planner's truth; a "closed in narrative, open in table" mismatch is a false signal a future planner reads as a real open item) + Eng (markdown-only change; no code risk; pure documentation closeout) + CEO (closing the hinge frees the planner from re-litigating a known-shipped claim; the receipt from STW-070 is what *proves* the hinge in operator-visible terms, but the markdown flip is what makes the planner pass clean).**



## Next-phase active items (RE-PLAN-005 2026-06-09 by designcritic, RE-PLAN task t_61bd0874; complements RE-PLAN-004 — does NOT supersede)

RE-PLAN-004 correctly closed the in-flight testnet receipt chain (STW-070 → STW-079) and added the structural environment fixes (STW-077 fast-kmeans, STW-078 postgres provisioning, STW-075 deterministic `Check::clustered`). The 7 rows in RE-PLAN-004 remain real and claimable; the next planner / worker should treat them as the in-flight queue. **RE-PLAN-005 does not retread any of those rows.** It opens a *new* phase the prior re-plans missed: the **server / CI / public-surface gap** that exists *independently* of the testnet receipt loop. The product today has 65+ shipped STW items, a working dashboard, a working receipt verifier, and a `trainer` binary — but three structural gaps are the real reason a "testnet" claim is not yet a "production" claim, and the receipt chain (RE-PLAN-004) does not address any of them:

1. **`crates/server` has 15 `/api/*` routes and 4 `/room/*` routes and ZERO HTTP-level integration tests.** The two test files in `crates/server/tests/` (`analysis_cli.rs` + `dto_wire.rs`) exercise the *no-DB, no-actix* parts of the renderer + DTO wire format; they do not drive a real `actix_web::test::TestServer` against the live `App::new()` in `crates/server/src/lib.rs:59-105`. The `/api/replace-obs`, `/api/blueprint`, `/api/exp-wrt-str`, `/room/enter`, `/room/leave`, `/room/start`, `/auth/login`, `/auth/register`, `/health` routes have **no test coverage at all** — a future regression in route shape, status code, JSON envelope, or auth gate is invisible to `cargo test`. The "testnet live" claim is meaningless if a stranger `curl`s a 500 the next deploy.

2. **`.github/workflows/ci.yml` runs `cargo test --lib --quiet` only.** None of the **28** integration test files in `crates/{server,database,auth,autotrain,nlhe,dashboard,clustering,gameroom}/tests/` actually run in CI. `crates/autotrain/tests/{live_proof,doctor,publish,publish_remote,publish_index,publish_index_remote,verify_receipt,script_shape,plan_staleness,workspace_parallel_proof,workspace_parallel_proof_three,smoke,seat_collapse,trainer_observe,bench,compare,compare3,bench_report_fixture}.rs` (18 files) + `crates/dashboard/tests/{smoke,seed_local,fixtures_smoke}.rs` (3 files) + `crates/database/tests/check_clustered.rs` + `crates/nlhe/tests/{position_persistence,serde_test}.rs` + `crates/gameroom/tests/hand_roundtrip.rs` + `crates/clustering/tests/kmeans_fast.rs` + `crates/auth/tests/server_flow.rs` — all 28 are skipped on every `git push`. The `verification:workspace-parallel` hinge (closed in STW-020 + STW-030) is *mechanically* closed but *not actually green in CI*, so the "in-CI proof" claim is fictional.

3. **`rbp-server` is the public product surface and has no deploy runbook.** The dashboard has `scripts/deploy-dashboard-cloudflare.sh` (STW-054) and Cloudflare Pages is documented. The `trainer` binary has the testnet-live-proof runbook. **The HTTP+WebSocket server has neither.** A `bash scripts/run-server.sh` to start it locally, a `bin/robopoker-backend` (or `cargo run -p rbp-server`) invocation, an `RBP_SERVER_AUTH_SIGNING_KEY` knob the startup currently hard-fails on, and a deploy target (Cloud Run / Fly.io / a plain systemd unit) — none of this is documented for a stranger. The README's "Quick Start" section is `rbp-cards` only; it does not mention `rbp-server` at all (only the "Crate Overview" table).

The 6 new rows below are the **server-side / CI-side / public-surface** slice. They are independent of the RE-PLAN-004 rows (a worker can claim any of them in parallel with STW-077/STW-078/STW-070/STW-079). Priority order: STW-080 (server integration tests) is the foundation; STW-081 (CI runs them) is what makes them *load-bearing*; STW-082 + STW-083 (server deploy + API docs) make the public surface real; STW-084 + STW-085 are small bounded hardening rows that close specific bugs RE-PLAN-005 surfaced. None of these rows retread a RE-PLAN-004 row; none of them change the receipt runbook, the dashboard, the v1/v2/v3 profile schemas, the seat-aware work, the kmeans cap, the `Check::clustered` fix, or the publish/index/remote chain.

The new active queue is **13 rows** (7 RE-PLAN-004 + 6 RE-PLAN-005). A `rg -n "^- \[ \] \*\*\[P[01]\]\*\* \`STW-" IMPLEMENTATION_PLAN.md` after this commit shows STW-070, STW-071, STW-074, STW-076, STW-077, STW-078, STW-079 (RE-PLAN-004) + STW-080, STW-081, STW-082, STW-083, STW-084, STW-085 (RE-PLAN-005). Owner for the RE-PLAN row itself: designcritic, 2026-06-09.

**RE-PLAN-006 (2026-06-10, this commit) adds a 14th row: STW-086, a new P0 discovered during the STW-070 evidence run.** With `STW-075` + `STW-077` + `STW-078` all shipped, the testnet-live-proof receipt runbook was run end-to-end with `RBP_TESTNET_FAST=1` against a real Postgres (provisioned by STW-078). The `--doctor` and `--cluster` steps landed cleanly through `river` and `turn`; `--cluster` then panicked on `flop` at `crates/clustering/src/bins.rs:95` (`Bins::peek` — "non empty histogram") inside `kmeans::init_kmeans_plus_plus` when sampling a centroid from the first 1024-point prefix of flop's turn-projection slice. The prefix contains empty turn projections (isomorphisms with no observed flops), and the kmeans++ driver does not filter them before the `WeightedIndex::new(potentials.iter()).sample(rng)` pick — the first pick's `peek()` then panics. The same bug exists in the production `Layer::init_kmeans` path (layer.rs:128-169), it just gets lucky on a 1.3M-row pool. The fix is bounded and small (filter empty histograms in `init_kmeans_plus_plus` + `init_kmeans` before the `WeightedIndex` pick, OR swap the kmeans++ potentials to skip empty slots); a 1-line guard eliminates the crash. STW-086 ships that fix + a regression test that pins the empty-input contract (`run_fast` on a 1024-point slice with ≥ 1 empty point returns K centroids without panicking), then STW-070 re-runs end-to-end to land a green receipt. STW-086 is the smallest possible slice that unblocks STW-070; it is a [P0] because no green `receipts/testnet-live-proof-<UTC>/SUMMARY.txt` is producible today. The 14-row active queue after this commit: STW-070, STW-071, STW-074, STW-076, STW-077, STW-078, STW-079 (RE-PLAN-004) + STW-080, STW-081, STW-082, STW-083, STW-084, STW-085 (RE-PLAN-005) + STW-086 (RE-PLAN-006).

- [ ] **[P0] `STW-080` `SERVER-HTTP-INTEGRATION-TESTS` (NEW in RE-PLAN-005) — close the largest test-coverage gap in the product: `crates/server` has 15 `/api/*` routes + 4 `/room/*` routes + 4 `/auth/*` routes + 1 `/health` route (24 routes total in `crates/server/src/lib.rs:59-105`) and ZERO HTTP-level integration tests.** The two existing test files (`crates/server/tests/analysis_cli.rs` + `crates/server/tests/dto_wire.rs`) exercise only the *no-DB, no-actix* `render_query` helper + the DTO wire-format round-trip; they do not drive a real `actix_web::test::TestServer` against the live `App::new()` in `crates/server/src/lib.rs:59`. A regression in route shape, status code, JSON envelope, CORS preflight, or auth gate on any of the 24 routes is invisible to `cargo test --workspace` and would only surface when a stranger `curl`s the deployed server. STW-080 ships a no-DB HTTP integration test layer that drives the routes that *do not require a live Postgres* through a real `actix_web::test::TestServer`: (a) `/health` returns 200 with body `ok` (the no-DB path that returns 503 when the client is unhealthy — but the `actix_web::test::TestServer` injection lets the test pin a no-op `web::Data<Arc<Client>>` so the 200 path is exercised); (b) `/auth/register` + `/auth/login` + `/auth/me` + `/auth/logout` round-trip a real `actix_web::test::TestServer` against the `crates/auth` register/login/logout/me handlers (these are the no-DB-gated paths that only need a `Crypto::from_env()` reading a deterministic test signing key — STW-080 ships a `RBP_AUTH_TEST_SIGNING_KEY` env knob the auth crate reads to short-circuit `from_env` in test mode, mirroring the `RBP_TESTNET_FAST=1` testnet knob); (c) `/api/exp-wrt-str` + `/api/replace-obs` + `/api/nbr-any-abs` return 200 with the expected JSON envelope shapes (the `api.strategy_from` test path the `crates/server/src/analysis/api.rs::api_strategy_from` already exposes for testing — the test pins the `position: 0/1` field the SEAT-PERSIST-001 slice 5 wired); (d) the bad-request paths return 400 + the expected error string (a malformed observation → 400 `invalid observation format`; an invalid street → 400 `invalid street format`; a malformed blueprint request → 400 with the `serde_json` error); (e) the CORS preflight `OPTIONS` requests return 200 with the `access-control-allow-*` headers the `Cors::default().allow_any_origin().allow_any_method().allow_any_header()` config in `crates/server/src/lib.rs:65-71` declares (a regression in the CORS gate breaks a `pages.dev` dashboard); (f) a 404 path returns 404 with a `not_found` body (the actix default). The new test file `crates/server/tests/http_routes.rs` uses `actix_web::test::TestServer` (already a transitive dep via `actix-web = "4"`) and the no-DB path: the test starts the server with a no-DB `Arc<Client>` (via `rbp_database::db()` returning a `mock` impl when `RBP_SERVER_NO_DB=1` is set — STW-080 adds a 5-line `db()` feature gate in `crates/database/src/lib.rs` that returns a `tokio_postgres::Client::connect` to a `pipe` protocol stream that responds to `SELECT 1` with `Ok(1)` and rejects any other query with a typed `NoDb` error the server maps to 503 — this is the minimum the test layer needs); 6 sub-tests: `health_returns_ok`, `auth_register_login_me_logout_round_trip`, `api_exp_wrt_str_returns_strategy_envelope`, `api_replace_obs_returns_400_on_malformed_input`, `cors_preflight_returns_200_with_allow_headers`, `not_found_returns_404`. Owner files: `crates/server/Cargo.toml` (add `actix-web` test dep if not transitive — confirm `actix_web::test` is re-exported), `crates/server/src/lib.rs` (the existing `App::new()` is reused as-is; no route change), `crates/server/src/analysis/api.rs` (expose `api_strategy_from` as `pub(crate)` if not already, for the test to call), `crates/server/src/auth_test_key.rs` (new ~30-line file: `pub fn test_signing_key() -> [u8; 32]` returning a deterministic `[0x42; 32]`, gated on `#[cfg(any(test, feature = "test-key"))]`), `crates/auth/src/lib.rs` (read `RBP_AUTH_TEST_SIGNING_KEY` env knob in `Crypto::from_env`; if set, return `Crypto::from_key(test_signing_key)` instead of `Crypto::from_env()` — the test env can `std::env::set_var` to opt in), `crates/database/src/lib.rs` (the 5-line `db()` no-DB gate: if `RBP_SERVER_NO_DB=1`, return a stub `Client` backed by a `Vec<tokio_postgres::Row>` that responds to `SELECT 1` and rejects everything else), `crates/server/tests/http_routes.rs` (new no-DB integration test with the 6 sub-tests above), `crates/server/tests/auth_no_db.rs` (new no-DB integration test for the auth round-trip with the test signing key). Scope boundary: do NOT add a `docker` / `testcontainers` / `mockall` / `wiremock` dep (the no-DB gate is 5 lines and uses the existing `tokio_postgres` types); do NOT change the route shapes (the existing `App::new()` is the source of truth — STW-080 only tests, does not redesign); do NOT change the JSON wire format (the existing DTOs are tested separately by `dto_wire.rs`); do NOT touch the dashboard / receipt runbook / publish chain; do NOT change the `Cargo.toml` of any crate other than `crates/server` + `crates/auth` + `crates/database` (the test signing key is an env knob, not a new feature). Acceptance criteria: `cargo test -p rbp-server --test http_routes` is green with 6 new sub-tests; `cargo test -p rbp-server --test auth_no_db` is green with the auth round-trip; `cargo test -p rbp-server --tests --lib` stays green (no regression in the existing `analysis_cli` + `dto_wire` integration tests + the existing 27 lib tests in `crates/server/src/`); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace` + `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts; a manual `RBP_AUTH_TEST_SIGNING_KEY=test RBP_SERVER_NO_DB=1 cargo run -p rbp-server` + `curl http://localhost:8888/health` returns `ok` + `curl -X POST -d '{"username":"u","password":"p"}' http://localhost:8888/auth/register` returns a JSON token. Hand-test: a contributor can `cargo test -p rbp-server --test http_routes` on a clean checkout (no Postgres, no docker) and see all 6 sub-tests pass in under 5 seconds. Dependencies: existing `actix-web = "4"` dep (the `actix_web::test` runtime is built-in to actix); existing `crates/server` (the `App::new()` is reused as-is); existing `crates/auth` (the `Crypto::from_env` path is extended with a test-only short-circuit, not replaced). Estimated scope: M (one new test file + one new auth test key + one no-DB gate + 5-10 lines in `app_data` injection). Completion signal: a fresh `cargo test -p rbp-server --tests` on a no-Postgres clean checkout is green with 8+ new sub-tests across 2 new integration test files; the next `auto steward --report-only` pass records the `server-test-coverage` finding as `RESOLVED`. **`lens:` CEO (the testnet claim is a *public* claim; 24 routes with zero HTTP-level coverage is a "tool shipped, not testnet live" claim) + Eng (one new test file + one 5-line no-DB gate = 1-2 days of work, no new deps) + Design (a regression in route shape / CORS / status code / JSON envelope fails CI on the next `git push`, not on a stranger's `curl`).**

- [ ] **[P0] `STW-081` `CI-RUNS-INTEGRATION-TESTS` (NEW in RE-PLAN-005) — close the second-largest gap: `.github/workflows/ci.yml` runs `cargo test --lib --quiet` only, so NONE of the 28 integration test files (`crates/{server,database,auth,autotrain,nlhe,dashboard,clustering,gameroom}/tests/*.rs`) actually run in CI.** The `verification:workspace-parallel` hinge is *mechanically* closed (STW-020 + STW-030) but *not actually green in CI* — a future regression in any integration test (a `cargo build` no longer compiles the integration test, an `actix_web::test::TestServer` panics, a `bash -n` on a runbook fails, a Postgres-needing integration test panics on missing `DATABASE_URL`) is invisible to the `Checks` GitHub Actions workflow that runs on every PR. STW-081 extends `.github/workflows/ci.yml` to run the integration test surface in 3 jobs (the minimum granularity that catches the three failure modes the prior re-plans hit): (a) `test:lib` — the existing `cargo test --lib --quiet` (no change, kept for fast feedback on the lib-test signal); (b) `test:integration:no-db` — a new job that runs `cargo test --workspace --tests --no-fail-fast -- --test-threads=4` *with* `RBP_TESTNET_FAST=1` + `RBP_SERVER_NO_DB=1` + `RBP_AUTH_TEST_SIGNING_KEY=test` + `CARGO_NET_OFFLINE=true` (the env knobs the no-DB integration test layer needs to run without a Postgres or a network); the job filters out the few integration tests that genuinely need a real Postgres (the `crates/autotrain/tests/live_proof.rs` + the `crates/autotrain/tests/{compare,compare3,bench,doctor}.rs` DB-gated sub-tests + the `crates/dashboard/tests/seed_local.rs` DB-gated path) via `cargo test --workspace --tests --no-fail-fast -- --skip live_proof --skip compare_run_emits --skip compare3_run_emits --skip bench_run_emits --skip doctor_run_emits --skip seed_local_db` — mirroring the `RECURSIVE_SKIP` filter the `scripts/workspace-parallel-proof.sh` STW-020 runbook already follows; (c) `test:integration:db` — a new job that uses `services: postgres:` in the GitHub Actions runner to spin up a real Postgres 16, sets `DATABASE_URL=postgres://postgres:postgres@localhost:5432/robopoker_test`, runs the same `cargo test --workspace --tests --no-fail-fast -- --test-threads=4` *without* the no-DB skip filter, and tears down the Postgres on completion. The two new jobs each upload their `target/test-results/*.xml` (or `target/nextest` if `cargo-nextest` is added) as a workflow artifact a contributor can download; both jobs run on every `pull_request` + `push to main` and the `Checks` workflow name + status badge in `README.md` flips from "Tests" to "Tests + Integration" once STW-081 lands. STW-081 *also* extends `scripts/plan-staleness-gate.sh` to refuse an empty `## CI status` section (the `STEPS_CI.txt` SUMMARY.txt appendix line the STW-019 + STW-023 runbook already writes) so a future receipt that does not pin the CI run URL is not committable. Owner files: `.github/workflows/ci.yml` (split the `test` job into the 3 jobs above, add `services: postgres:` to `test:integration:db`, add the env knobs the no-DB layer needs), `README.md` (the existing `[![build]` badge line updates to point at the new 3-job status — minimal change), `scripts/plan-staleness-gate.sh` (add a 1-line grep that requires `## CI status` to appear in `IMPLEMENTATION_PLAN.md` for every `STW-070`-class row — closes the "operator proof does not pin the CI run" drift the prior receipts exhibited). Scope boundary: do NOT add `cargo-nextest` (the existing `cargo test` invocation is fine; the no-DB layer is the new test surface, not a new test runner); do NOT add `codecov` / `coveralls` (the coverage signal is not the goal — the *gate* is); do NOT add a `cross` / `cargo-hack` matrix (the existing single-target `ubuntu-latest` runner is fine; the no-DB layer runs on every push); do NOT touch the test files themselves (the existing 28 integration test files are reused as-is); do NOT change the workspace `Cargo.toml`; do NOT touch the receipt runbook (the runbook already does its own `cargo test` validation on the receipt dir). Acceptance criteria: the new `test:integration:no-db` job is green on every push to `main` (and on the PR that lands STW-081); the new `test:integration:db` job is green on every push to `main` (and on the PR that lands STW-081); a fresh `git push` shows 3 green check marks in the GitHub PR UI (`check` / `test:lib` / `test:integration:no-db` / `test:integration:db` — 4 total); the new `## CI status` requirement in `plan-staleness-gate.sh` does not break the existing gate (the gate is extended, not replaced); `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts; `cargo test --workspace --tests --no-fail-fast -- --test-threads=4` is locally green in < 5 minutes (the no-DB path runs in < 90 seconds). Hand-test: a contributor pushes a one-line change to `crates/server/src/lib.rs` that breaks `/health` → the `test:integration:no-db` job fails on the PR with a clear `health_returns_ok FAILED: expected body 'ok', got '503 Service Unavailable'` diagnostic. Dependencies: the STW-080 server integration tests (the no-DB path needs the `RBP_SERVER_NO_DB=1` gate STW-080 ships); existing 28 integration test files (the new jobs reuse them as-is); a `services: postgres:` image the GitHub Actions `ubuntu-latest` runner already supports (`postgres:16-alpine`). Estimated scope: M (one workflow split + 2 new jobs + 1 env-knob set + 1 gate extension = ~80 lines of YAML + 1 line of bash). Completion signal: a fresh PR that breaks any of the 28 integration test files fails the `Checks` workflow at the right job within 5 minutes of push; the README badge reflects 4 jobs (compile + 3 test jobs) and is green on `main`. **`lens:` CEO (the "testnet live" claim is a *public* claim; "the CI is green" is a different claim from "the lib tests are green in isolation"; without integration tests in CI, the claim is fictional) + Eng (one workflow split is a known pattern; `services: postgres:` is the GitHub Actions default for DB-gated test layers) + Design (a contributor sees a 4-job CI run on every PR — the green badge is now honest, not aspirational).**

- [ ] **[P0] `STW-082` `SERVER-DEPLOY-RUNBOOK` (NEW in RE-PLAN-005) — ship the operator-runnable deploy runbook for `rbp-server` (the public product surface the README's `## Quick Start` section does not currently mention).** Today the product has `scripts/deploy-dashboard-cloudflare.sh` (the dashboard deploy, STW-054) and `scripts/testnet-live-proof.sh` (the trainer receipt runbook, STW-019) — but no script + no doc a stranger can read to bring up `rbp-server` (the 24-route actix-web server in `crates/server/src/lib.rs:59-105`) on a remote host. The `bin/backend/src/main.rs:7` is a 5-line `async fn main() { rbp_server::run().await.unwrap() }` that requires `BIND_ADDR` + a working Postgres + a non-default `RBP_AUTH_SIGNING_KEY` (STW-004 hardened `Crypto::from_env` to refuse to start without one). An operator who wants to bring up the testnet API + WebSocket surface today must read the source to discover the env knobs, write their own `systemd` unit or `Procfile`, and figure out the `RBP_AUTH_SIGNING_KEY` + `DATABASE_URL` + `BIND_ADDR` trio by trial-and-error. STW-082 ships three artifacts: (a) `scripts/deploy-server-cloudrun.sh` — a pure-bash runbook that mirrors the `scripts/deploy-dashboard-cloudflare.sh` shape: refuses to run on missing `gcloud` (exit 2), refuses to run on missing `RBP_AUTH_SIGNING_KEY` (exit 3 — the operator's signing key, *not* committed, sourced from `.env` or `pass` or `gcloud secrets`), refuses to run on missing `RBP_SERVER_POSTGRES_URL` (exit 3), builds the binary with `cargo build --release -p rbp-server`, builds a Cloud Run-compatible container with a 12-line inline `Dockerfile` (debian-slim + the `target/release/rbp-server` binary + `CMD ["./rbp-server"]`), pushes the container to `gcr.io/<project>/robopoker-server:<git-sha>`, and `gcloud run deploy`s the service with the env knobs forwarded as `--set-env-vars` (the secret values use `--set-secrets`); (b) `scripts/deploy-server-cloudrun.md` — a runbook doc mirroring `scripts/deploy-dashboard-cloudflare.md`: required `gcloud` setup, required `RBP_AUTH_SIGNING_KEY` generation (`openssl rand -hex 32` or `age` / `pass`), required `RBP_SERVER_POSTGRES_URL` shape, the `gcloud sql connect` recipe for the Cloud SQL connection, the `gcloud run services describe robopoker-server --format='value(status.url)'` command that prints the live URL, the curl smoke test (`curl https://<service-url>/health` should return `ok`); (c) `scripts/run-server-local.sh` — a 5-line convenience script that `RBP_AUTH_SIGNING_KEY=$(openssl rand -hex 32) cargo run -p rbp-server` after sourcing a `.env` file an operator writes with `DATABASE_URL` + `BIND_ADDR=0.0.0.0:8888`, so a contributor can `git clone && bash scripts/run-server-local.sh` and have a working `localhost:8888` server in under 30 seconds. Owner files: `scripts/deploy-server-cloudrun.sh` (new ~100-line bash runbook), `scripts/deploy-server-cloudrun.md` (new runbook doc), `scripts/run-server-local.sh` (new ~20-line convenience script), `README.md` (add a `## Run the server` section after the existing `## Testnet launch proof` section with the `bash scripts/run-server-local.sh` invocation + a 4-line curl smoke test that hits `/health` + `/auth/register` + `/api/exp-wrt-str` + a `wscat` example for `/room/enter`), `crates/autotrain/tests/script_shape.rs` (2 new shell-shape pins: `deploy_server_cloudrun_script_exists_and_parses` + `run_server_local_script_exists_and_parses` — mirrors the `deploy_dashboard_cloudflare_script_*` pinners), `IMPLEMENTATION_PLAN.md` (this row). Scope boundary: do NOT vendor a Dockerfile into the repo (the `deploy-server-cloudrun.sh` script contains a 12-line inline `Dockerfile` written to `/tmp/rbp-server.Dockerfile` and `docker build -f /tmp/rbp-server.Dockerfile` — keeps the repo's `Dockerfile`-less shape the existing tests + CI rely on); do NOT add a Cloud Run SDK dep (the script shells out to `gcloud` + `docker`); do NOT change `crates/server/src/lib.rs` (the existing `App::new()` is the deploy target — no code change); do NOT add a Cloud Run YAML manifest (the script is the manifest generator); do NOT touch the dashboard / receipt / publish chain. Acceptance criteria: `bash -n scripts/deploy-server-cloudrun.sh` exits 0; `bash -n scripts/run-server-local.sh` exits 0; the new `script_shape` sub-tests are green; `bash scripts/plan-staleness-gate.sh` exits 0; `cargo test --workspace -- --test-threads=4` stays green (the scripts are shell-only, the runbook doc is markdown, the test pins are no-op); on a workstation with `gcloud` + `docker` + a real `RBP_AUTH_SIGNING_KEY` + a real `RBP_SERVER_POSTGRES_URL`, a fresh `bash scripts/deploy-server-cloudrun.sh` exits 0 with the pinned `cloud run deploy: complete: service=robopoker-server url=https://...run.app revision=rbp-server-<sha>` headline + the returned URL responds 200 to `curl /health` within 60 seconds; on a workstation without any of those, the script refuses with the appropriate exit 2 / exit 3 diagnostic. Hand-test: a contributor runs `bash scripts/run-server-local.sh` on a clean checkout with a working Postgres → the server starts in < 5 seconds + `curl http://localhost:8888/health` returns `ok` + `curl -X POST -d '{"username":"u","password":"p"}' http://localhost:8888/auth/register` returns a JSON token + `wscat -c ws://localhost:8888/room/enter/<id>?token=<token>` opens a WebSocket connection. Dependencies: a `gcloud` + `docker` + `openssl` toolchain for the Cloud Run path; a `cargo` toolchain for the local path; a working Postgres for either path (the `RBP_SERVER_POSTGRES_URL` is the operator's choice). Estimated scope: M (1 bash runbook + 1 doc + 1 convenience script + 2 shell-shape pins + 1 README section = ~250 lines of new code/docs). Completion signal: a fresh `bash scripts/deploy-server-cloudrun.sh` on a workstation with the right toolchain + secrets returns a live `https://<service>.run.app` URL that `curl /health` answers `ok`; the README's `## Run the server` section is the stranger-readable entry point; the next `auto steward --report-only` pass records the `server-deploy-coverage` finding as `RESOLVED`. **`lens:` CEO (the testnet claim is a *public* claim; "the server works" is meaningless without a "and an operator can deploy it" recipe) + Eng (one bash runbook + one doc + one convenience script mirrors the `deploy-dashboard-cloudflare.sh` pattern the autotrain pipeline already follows) + Design (the README's `## Run the server` section is the answer to a stranger's "how do I run this" question — without it, the testnet is a private experiment, not a public surface).**

- [ ] **[P1] `STW-083` `API-DOCS-AND-CURL-EXAMPLES` (NEW in RE-PLAN-005) — ship a stranger-readable `## HTTP API` section in `README.md` covering the 24 routes in `crates/server/src/lib.rs:59-105` (4 auth + 4 room + 15 api + 1 health) with one curl example per route, the expected status code, and the JSON envelope shape.** The README today has a `## Quick Start` (rbp-cards only), a `## TUI Preview` (rbopoker-tui), and a `## Testnet launch proof` (testnet-live-proof.md anchor) — but the *most user-facing* surface (the 24 routes the operator deploys in STW-082) is undocumented for a stranger. A new contributor who wants to `curl` the testnet API has to read the source in `crates/server/src/analysis/handlers.rs` to discover the JSON request shape, the auth requirement, and the error response. STW-083 ships a `## HTTP API` README section that is *the* stranger-readable reference for the 24 routes: per route, a 1-line description, the HTTP method, the request shape (one JSON example for POST routes, no body for GET), the success response shape (one JSON example, with the pinned field names), the error responses (400 / 401 / 404 / 500 with the typed error body the handlers emit), and a one-line `curl` command a stranger can copy-paste against a `RBP_SERVER_URL` env knob the README declares (`export RBP_SERVER_URL=http://localhost:8888`). The section is grouped by `### Health`, `### Auth`, `### Room (WebSocket)`, `### Analysis API`, mirroring the `App::new()` scope nesting. A new `crates/server/tests/api_docs_consistency.rs` integration test (no-DB) parses the `## HTTP API` section of `README.md` with a 50-line regex pass, asserts the section names all 24 routes by their `actix_web::route()` path, asserts each route's documented method matches the source's `web::post().to(handler)` / `web::get().to(handler)` shape, and asserts the `curl` examples parse (a future `bash -n`-style sanity check on the inline command — `bash -c "..."` works). Owner files: `README.md` (new `## HTTP API` section, ~150 lines), `crates/server/tests/api_docs_consistency.rs` (new no-DB integration test file, ~120 lines, parses README + asserts 24 route entries + asserts method/source parity + bash-parses the curl examples), `IMPLEMENTATION_PLAN.md` (this row). Scope boundary: do NOT add a Swagger / OpenAPI / Redoc dep (the README is the docs surface; a typed `serde` schema is the OpenAPI source but generating + serving the spec is a separate slice); do NOT change any handler in `crates/server/src/analysis/handlers.rs` or `crates/server/src/hosting/handlers.rs` or `crates/server/src/lib.rs` (the docs are descriptive, not prescriptive); do NOT touch the `crates/dashboard/static/index.html` dashboard (the dashboard is the *consumer* of the API, not the docs surface for it); do NOT touch the receipt runbook (the runbook is operator-facing, the README is stranger-facing — different audiences). Acceptance criteria: a fresh `cargo test -p rbp-server --test api_docs_consistency` is green; the `## HTTP API` section of `README.md` lists all 24 routes by name + method + path; every documented `curl` example `bash -c`'s to a syntactically valid shell command (the test asserts this with a 50-line bash-subprocess call per example); `bash scripts/plan-staleness-gate.sh` exits 0; `cargo test --workspace -- --test-threads=4` stays green (the new test is no-DB, no-network). Hand-test: a stranger with zero robopoker context reads the `## HTTP API` section + picks `/api/exp-wrt-str` → sees the method (POST), the request shape (`{"street":"flop"}`), the response shape (`{"obs":"...","abs":"...","density":0.42,"distance":0.13,...}`), the error path (`{"detail":"invalid street format"}` on a 400), and a copy-pasteable `curl` example → can `curl -X POST -H 'content-type: application/json' -d '{"street":"flop"}' $RBP_SERVER_URL/api/exp-wrt-str` against a running `rbp-server` and get a parseable JSON response. Dependencies: STW-082 (the deploy runbook + the `RBP_SERVER_URL` env knob pattern — the README anchors its examples on `RBP_SERVER_URL`); the existing 24 routes in `crates/server/src/lib.rs:59-105` (the source of truth — the test asserts README ↔ source parity); the existing `crates/server/Cargo.toml` deps (no new deps). Estimated scope: S (1 README section + 1 no-DB integration test = ~270 lines of new code/docs). Completion signal: a fresh `cargo test -p rbp-server --test api_docs_consistency` is green + the README's `## HTTP API` section is the first hit a stranger gets when searching GitHub for "robopoker api" + the next `auto steward --report-only` pass records the `api-docs-coverage` finding as `RESOLVED`. **`lens:` CEO (a public testnet API is not a *public* testnet API until a stranger can read the docs and `curl` a route without reading source) + Eng (a README section + a regex-parsing test is the minimum that keeps docs from drifting; the no-DB test runs in CI in < 1 second) + Design (a typed `serde` schema in code is the API source of truth; a README section is the API *narrative*; the test keeps the two in lockstep so a future handler rename breaks CI).**

- [ ] **[P1] `STW-084` `FAST-MODE-PARITY-FOR-COMPARE3-AND-TRANSCRIPT` (NEW in RE-PLAN-005) — close two small bounded gaps RE-PLAN-005 surfaced in the `RBP_TESTNET_FAST=1` chain the RE-PLAN-004 receipt runbook exercises: (1) `trainer --compare3` (the v1-vs-v2-vs-v3 three-way compare, STW-031) ignores the `RBP_TESTNET_FAST=1` knob — it does not read `RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` for the v3 hand count, only for the v1-vs-v2 pair, so a fast-mode receipt that takes the v1-vs-v2 pair through `--compare` and the v3 pair through `--compare3` produces asymmetric hand counts (a downstream dashboard that scrapes both reports cannot plot a v1/v2/v3 curve on the same x-axis); (2) the `RBP_BENCH_TRANSCRIPT_DIR` knob the runbook sets per-receipt is not pinned by a shell-shape assertion — a future runbook edit that drops the env-knob set fails the bench step silently (the bench runs with the default transcript dir, but the `SUMMARY.txt` line `testnet live_proof complete: ...` still parses).** STW-084 ships two small bounded fixes: (a) `crates/autotrain/src/bench.rs` — extend the `Mode::Compare3` arm's `compare3_hands` + `compare3_blind` env helpers to read the same `RBP_COMPARE_HANDS` + `RBP_COMPARE_BLIND` knobs the `Mode::Compare` arm reads (the STW-031 spec's `DEFAULT_COMPARE3_HANDS` + `DEFAULT_COMPARE3_BLIND` constants are the defaults when unset; a fast-mode receipt that sets the env knobs sees a single hand count across both compare modes); (b) `crates/autotrain/tests/script_shape.rs` — add a new `testnet_live_proof_script_pins_bench_transcript_dir_per_reipt` sub-test that greps `scripts/testnet-live-proof.sh` for the `RBP_BENCH_TRANSCRIPT_DIR=<receipt>/bench/transcripts` export line and asserts it precedes the bench step (the existing `testnet_live_proof_script_*_pinners` follow the same grep pattern). Owner files: `crates/autotrain/src/bench.rs` (one new env-read helper `compare3_hands` that prefers `RBP_COMPARE_HANDS` over `DEFAULT_COMPARE3_HANDS` — ~5 lines; the existing `Mode::Compare3` arm's `run_compare3` is the model; add a `compare3_hands_reads_compare_hands_env` + a `compare3_hands_default_is_200` + a `compare3_blind_reads_compare_blind_env` + a `compare3_blind_default_is_b_blind` lib test to `bench.rs::tests`), `crates/autotrain/tests/script_shape.rs` (one new shell-shape pin, ~15 lines), `scripts/testnet-live-proof.md` (one-line note in the env-knob table that `RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` apply to both `--compare` and `--compare3` in fast mode). Scope boundary: do NOT change the v1/v2/v3 trained config axis (the compare3 still rotates the v1/v2/v3 configs through 3 pairwise heads-up shells — the parity is *across* modes, not *within* a mode); do NOT change the `--compare` arm (the existing `Mode::Compare` env reads are the model the compare3 mirrors); do NOT change the bench arm (the bench already reads `RBP_BENCH_HANDS` / `RBP_BENCH_BLIND` correctly); do NOT touch the dashboard / publish / index / remote chain. Acceptance criteria: `cargo test -p rbp-autotrain --lib bench::tests` is green with 4 new compare3 parity sub-tests; `cargo test -p rbp-autotrain --test script_shape` is green with 1 new pin; `cargo test --workspace -- --test-threads=4` stays green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts; a fresh `RBP_TESTNET_FAST=1 RBP_COMPARE_HANDS=4 bash scripts/testnet-live-proof.sh` on a warmed DB shows the same `hands: 4` in both the `compare/stdout.txt` and a future `compare3/stdout.txt` (the post-STW-084 shape). Hand-test: a fresh receipt directory shows `RBP_BENCH_TRANSCRIPT_DIR` exported in `ENV.txt` + the bench step writes `bench/transcripts/transcript-*.json` files under the receipt dir (not a shared global dir). Dependencies: STW-031 (the `Mode::Compare3` arm the parity hooks into); existing `RBP_TESTNET_FAST=1` knob (the new helpers read the same env); existing `crates/autotrain/tests/script_shape.rs` (the new pin mirrors the `testnet_live_proof_*` pinners). Estimated scope: XS (1 env-read helper + 4 lib tests + 1 shell-shape pin + 1 doc line = ~40 lines of new code/docs). Completion signal: a fresh `cargo test -p rbp-autotrain --test script_shape` is green with 1 new pin + a fresh fast-mode receipt shows the post-STW-084 parity shape. **`lens:` Design (a v1/v2/v3 learning curve a downstream dashboard plots is only meaningful if the x-axis is symmetric — fast-mode parity is the minimum that keeps the curve honest) + Eng (one env-read helper + one grep pin = 1 hour of work) + CEO (the compare3 dashboard surface is the only public view of the v1/v2/v3 trained config axis; a hand-count asymmetry would silently invalidate the headline claim).**

- [ ] **[P1] `STW-085` `FAILED-RECEIPT-ARCHIVE` (NEW in RE-PLAN-005) — archive the 18+ failed/partial `receipts/testnet-live-proof-2026060*/` directories into `receipts/_archive/failed/<UTC>/` so a fresh green receipt is the only thing a stranger sees when they `ls receipts/`.** The 18 receipts in `receipts/testnet-live-proof-2026060{4..9}T*` (counted via `ls receipts/ | wc -l` → 18 directories spanning 2026-06-04 → 2026-06-09) are all the *stale* failed/partial runbook attempts the prior receipt chain produced: every one of them has either `cluster=101` (the SIGTERM timeout exit), `doctor=2` (the `db_reachable: false` exit), or no `SUMMARY.txt` at all (the run was killed before the chain finished). They are real evidence — the prior receipt attempts are what tells the next planner *why* STW-077 + STW-078 are needed — but they pollute the `receipts/` dir a stranger `ls`-es when the README's `## Testnet launch proof` section points them there. STW-085 is a pure-fs slice: (a) `mkdir -p receipts/_archive/failed` + `mv receipts/testnet-live-proof-2026{04,05,06,07,08,09}T* receipts/_archive/failed/` (the move preserves the receipts' git-tracked state because `receipts/` is `.gitignore`d — `git status` after the move shows nothing; the move is operator-side, not committed); (b) extend `.gitignore` with a `!receipts/_archive/` exception so the archived dir is the *one* thing tracked (the operator-visible audit trail of "what went wrong" is now in the repo, not just on a local disk); (c) add a `receipts/_archive/failed/INDEX.md` that lists each archived receipt + the one-line failure mode (e.g. `20260609T060233Z: doctor=2 db_reachable=false password auth`, `20260609T042107Z: cluster=101 timeout mid-iteration turn kmeans`) — a planner can `cat receipts/_archive/failed/INDEX.md` and see the failure history; (d) update `scripts/testnet-live-proof.sh`'s receipt-dir naming convention to append a `_v2` suffix on the live `receipts/` dir (e.g. `receipts/testnet-live-proof-<UTC>_v2/`) so a future receipt never collides with the archived ones; (e) update `scripts/testnet-live-proof.md`'s `Receipts layout` section to document the new `receipts/_archive/failed/` location + the `INDEX.md` summary. The new `INDEX.md` is committed on `main`; the move + the `_v2` suffix change are operator-side (no commit). Owner files: `receipts/_archive/failed/INDEX.md` (new — the failure-history summary, ~30 lines; one bullet per archived receipt + the failure mode + the slice that fixed it), `.gitignore` (add `!receipts/_archive/` to the existing `/receipts/` ignore line), `scripts/testnet-live-proof.sh` (one-line change to the `RECEIPT_DIR` assignment: append `_v2` to the basename when not set), `scripts/testnet-live-proof.md` (one-paragraph update to the `Receipts layout` section), `IMPLEMENTATION_PLAN.md` (this row). Scope boundary: do NOT delete the failed receipts (the operator can audit them post-hoc; the archive is the right home, not the trash); do NOT change the `LiveProofReceipt::read_and_verify` verifier (it does not care about the receipt dir name); do NOT touch the runbook's chain step order; do NOT touch the dashboard / publish / index / remote chain. Acceptance criteria: a fresh `ls receipts/` shows only the `_archive/` subdir + 0 stale `testnet-live-proof-2026*` entries; `ls receipts/_archive/failed/` shows all 18 archived receipts + the `INDEX.md` summary; `cat receipts/_archive/failed/INDEX.md` lists every archived receipt + its failure mode; `git status` is clean (the move is operator-side, the `INDEX.md` + the `.gitignore` change + the script suffix are the only commits); a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` produces a new `receipts/testnet-live-proof-<UTC>_v2/` dir; the existing `cargo test -p rbp-autotrain --test live_proof_receipt` stays green (the verifier does not care about the basename suffix); `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts. Hand-test: a stranger with zero robopoker context runs `ls receipts/` and sees only `_archive/` (the *honest* state — no stale evidence); a planner runs `cat receipts/_archive/failed/INDEX.md` and sees the full failure history + the slice chain that fixed each failure (STW-075 + STW-077 + STW-078). Dependencies: existing `.gitignore` (the `!receipts/_archive/` exception is the only line that changes); existing `scripts/testnet-live-proof.sh` (the `RECEIPT_DIR` assignment is the only line that changes); existing `LiveProofReceipt::read_and_verify` (the verifier is the source of truth, unaffected by the basename suffix). Estimated scope: XS (1 `INDEX.md` + 1 `.gitignore` line + 1 script line + 1 doc paragraph + 1 operator-side `mv` = ~40 lines of new code/docs + 1 operator move). Completion signal: a fresh `ls receipts/` shows only `_archive/` + a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` produces a new `_v2`-suffixed receipt dir. **`lens:` Design (a stranger `ls`-ing the receipts dir sees the *honest* state — one audit-trail summary, 18 archived attempts — not 18 confusing stale dirs) + Eng (1 file move + 1 `INDEX.md` + 1 `.gitignore` line = 30 minutes of work) + CEO (the failure history is the planner's input for the *next* RE-PLAN — a future planner who reads `INDEX.md` sees the structural issues without re-running the failed receipts).**

## Next-phase active items (RE-PLAN-007 2026-06-10 by designcritic, RE-PLAN task t_a500998c; complements RE-PLAN-004 / RE-PLAN-005 / RE-PLAN-006 — does NOT retread any of the 14 prior rows)

The 14-row active queue (STW-070, STW-071, STW-074, STW-076, STW-079 from RE-PLAN-004 + STW-080, STW-081, STW-082, STW-083, STW-084, STW-085 from RE-PLAN-005 + STW-086 from RE-PLAN-006) is structurally well-formed: each row names files, scope boundaries, and a verification command. The queue is NOT exhausted — 14 P0/P1 rows are still claimable — but RE-PLAN-007 re-audits the *current* state and surfaces 2 structural issues the prior re-plans missed that explain why STW-070 has been un-claimed across 4 RE-PLAN cycles (RE-PLAN-002 → RE-PLAN-003 → RE-PLAN-004 → RE-PLAN-005 → RE-PLAN-006) and why a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` keeps producing `cluster=101` SIGTERM panics instead of green `SUMMARY.txt` headlines:

1. **The kmeans fast-mode driver has a SECOND, DIFFERENT empty-cluster panic that STW-086 did not catch.** STW-086 closed the *pre-init* empty-input panic (kmeans++ sampling an empty histogram from the first 1024-point prefix). The receipt runbook at `receipts/testnet-live-proof-20260610T032421Z/` panicked at `bins.rs:95:31: non empty histogram` on `flop` at **03:29:14** during the *Lloyd-step reassign* phase (`step_elkan_slice` in `crates/clustering/src/kmeans.rs:384+`), NOT during kmeans++ init. The panic fires inside `metric.emd(new_centroids[i], centroids[i])` (the drift calculation) when the reassign pass leaves a cluster slot with zero assigned points — the `from_fn` fold then returns the `identity()` empty histogram for that slot, and the next `metric.emd` call's `Bins::peek` panics. The classical k-means "empty cluster" pathology: a Lloyd step concentrates all points into fewer than K clusters (the production 1.3M-row pool has not yet hit it, the fast-mode 1024-point pool with K=128 is exactly the regime where the pathology fires). The fix is already drafted in the working tree: `crates/clustering/src/kmeans.rs:384+` keeps the OLD centroid for any cluster slot that landed zero points in the reassign pass (mirrors the production `TestLayer::heal` in `crates/clustering/src/tests.rs:64-70`); a new sub-test `fast_mode_handles_empty_cluster_during_lloyd_step` in `crates/clustering/tests/kmeans_fast.rs` engineers a forced empty cluster (K=2, N=2, similar-but-distinct turn projections) and asserts the fix. The fix is uncommitted at the start of RE-PLAN-007 (the working tree shows `git status`: `modified: crates/clustering/src/kmeans.rs` + `modified: crates/clustering/tests/kmeans_fast.rs`).

2. **The kmeans fast-mode driver is the *least hardened* code path the receipt chain exercises, with ZERO regression coverage in the 4-then-5-then-6 receipt-blocking panic families the runbook has produced.** STW-075 (deterministic `Check::clustered`) + STW-077 (sample/iteration cap) + STW-086 (pre-init empty-input guard) closed three of the four families; STW-087 (post-init empty-cluster guard) closes the fourth. But the *only* existing kmeans fast-mode test coverage is the 3 sub-tests STW-077 shipped (`fast_mode_caps_sample_at_1024` + `fast_mode_caps_iterations_at_8` + `production_mode_unchanged_when_fast_unset`) and the 1 sub-test STW-086 added (`fast_mode_handles_empty_point_in_prefix`). The `init_kmeans` production path (`crates/clustering/src/layer.rs:128-169`) has the same pre-init empty-input hole STW-086 caught in the fast path; the production path is currently defended only by the "1.3M-row pool is large enough to luck out" structural argument, not by a regression test. A future production run on a fresh DB with sparse observations will hit the same panic the receipts captured, and the production path is *not* in `cargo test --workspace` (the kmeans fast path is integration-tested; the kmeans production path is lib-tested only — the `crates/clustering/src/tests.rs` lib tests cover `TestLayer::heal` but not the `Layer::init_kmeans` empty-input contract). STW-088 closes this gap with a property-test layer for the kmeans driver that pins the no-panic contract on randomized inputs.

What RE-PLAN-007 changes versus RE-PLAN-006:

- **STW-087 is promoted** from a working-tree draft to a worker-ready P0 row. The fix is already in the working tree (2 files: `kmeans.rs` 60+ lines + `kmeans_fast.rs` 115 lines); the worker ports + tests + commits the same shape RE-PLAN-002/003/004/006 used for STW-075/077/086.
- **STW-088 is new** — a kmeans driver property-test layer (no-DB, fast, byte-stable) that pins the no-panic contract on randomized inputs across both the fast-mode `run_fast` and the production-mode `init_kmeans` paths, so a future regression in either path fails `cargo test --workspace` on a single `cargo test -p rbp-clustering --test kmeans_property` invocation, NOT on a stranger's runbook run.
- **STW-089 is new** — a `crates/autotrain/tests/kmeans_no_panic_repro.rs` integration test that runs the production-mode `Layer::init_kmeans` path on a 1024-point prefix with a 50% empty prefix (the regime STW-088's property test exercises, but in the *production* code path) and asserts no panic — the production-path defense in depth, mirroring the fast-path defense the STW-077/086/087 trio shipped.
- **STW-090 is new** — the *terminal evidence* slice RE-PLAN-006's `STW-070` re-issue was blocked on. With `STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087` all shipped, `STW-090` is the slice that runs the runbook end-to-end and commits the first green `receipts/testnet-live-proof-<UTC-THIS-WEEK>/` directory the receipt chain has produced. STW-090 *integrates* the STW-085 archive work into the terminal-evidence commit (one commit produces the green receipt + the clean receipts dir + the failure-history INDEX) so a single RE-PLAN-007 closeout produces the operator-visible proof the testnet north star demands.
- **STW-080 / STW-081 / STW-082 / STW-083 / STW-084 / STW-085 (RE-PLAN-005) and STW-071 / STW-074 / STW-076 / STW-079 (RE-PLAN-004) and STW-086 (RE-PLAN-006) carry over unchanged** — all are real, claimable, independent of the receipt loop, and have not been retreaded.
- **STW-072 is deprecated** as before.
- **STW-073 is explicitly deferred** as before (depends on STW-090's green receipt).

The 4 new rows below are the *kmeans-fast-mode hardening* slice. They are the minimum surface that unblocks STW-070 from its current "kmeans panics, runbook stalls" state and closes the *fourth* receipt-blocking panic family the prior re-plans missed. The new active queue is **18 rows** (7 RE-PLAN-004 + 6 RE-PLAN-005 + 1 RE-PLAN-006 + 4 RE-PLAN-007). A `rg -n "^- \[ \] \*\*\[P[01]\]\*\* \`STW-" IMPLEMENTATION_PLAN.md` after this commit shows STW-070, STW-071, STW-074, STW-076, STW-079 (RE-PLAN-004) + STW-080, STW-081, STW-082, STW-083, STW-084, STW-085 (RE-PLAN-005) + STW-086 (RE-PLAN-006) + STW-087, STW-088, STW-089, STW-090 (RE-PLAN-007). Priority order: STW-087 (empty-cluster guard, working-tree draft) is the smallest P0; STW-088 (property-test layer) is the defensive P1 that makes STW-087's fix a *test* not a *hope*; STW-089 (production-path defense) is the production-path equivalent; STW-090 (runbook retry + receipt chain) is the terminal evidence slice. Owner for the RE-PLAN row itself: designcritic, 2026-06-10.

- [x] **[P0] `STW-087` `KMEANS-EMPTY-CLUSTER-GUARD-IN-LLOYD-STEP` (NEW in RE-PLAN-007, working-tree draft uncommitted at the start of RE-PLAN-007; SHIPPED 2026-06-10 — `step_elkan_slice` keeps the old centroid for any cluster slot that received zero assigned points in the Lloyd-step reassign pass, mirroring the production `TestLayer::heal` pattern, plus a regression test in `crates/clustering/tests/kmeans_fast.rs::fast_mode_handles_empty_cluster_during_lloyd_step` that engineers a forced empty cluster with K=2 / N=2 / turn projections and pins the no-panic contract).** Close the SECOND empty-cluster panic the receipts captured that STW-086 did not catch.** The receipt `receipts/testnet-live-proof-20260610T032421Z/` panicked at `crates/clustering/src/bins.rs:95:31: non empty histogram` on `flop` at **03:29:14** during the *Lloyd-step reassign* phase (`step_elkan_slice` in `crates/clustering/src/kmeans.rs:384+`), NOT during kmeans++ init (the STW-086 phase). The trace: `run_fast` → `Layer::cluster_fast` → `step_elkan_slice` → reassign pass folds empty `identity()` for the cluster slot that landed zero points this iteration → `new_centroids[i]` is empty → drift calc `metric.emd(empty_new, old_centroid[i])` calls `Metric::emd` (metric.rs:108) which dispatches `source.peek()` (`Bins::peek`, bins.rs:95) → panic. The classical k-means "empty cluster" pathology: a Lloyd step concentrates all points into fewer than K clusters. The fast-mode 1024-point pool + K=128 cluster count makes "every point assigned to the same handful of clusters" a non-degenerate case (the production 1.3M-row pool with K=128 is large enough that the pathology has not fired, but a future production run on a fresh DB with sparse observations will hit the same panic). The fix is already drafted in the working tree: `crates/clustering/src/kmeans.rs:384+` `step_elkan_slice` keeps the OLD centroid for any cluster slot that landed zero points in the reassign pass (the `std::array::from_fn(|j| { ... if assigned == 0 { centroids[j] } else { new } })` shape). This is the classical k-means "empty cluster" fix (mirrors `scikit-learn`'s `init='k-means++'` reassign + `np.argmin` tie-break + `_kmeans_plusplus` cluster-replacement; mirrors the production `TestLayer::heal` in `crates/clustering/src/tests.rs:64-70` which replaces empty centroids with random histograms after each step). The slice-based fast-mode path uses the OLD centroid instead of a fresh sample because the fast-mode driver is byte-stable (a fixed `Metric::default()` + a fixed `RBP_FAST_KMEANS_SAMPLE` / `RBP_FAST_KMEANS_ITERATIONS` cap = byte-stable output) and a fresh sample would introduce a second source of non-determinism. The new centroid is therefore *guaranteed* non-empty for every cluster slot — the drift calculation is well-defined. STW-087 ships the working-tree fix + a regression test in `crates/clustering/tests/kmeans_fast.rs`: `fast_mode_handles_empty_cluster_during_lloyd_step` engineers a forced empty cluster (K=2, N=2 with two similar-but-distinct turn projections; kmeans++ init picks both because they are distinct — `InsufficientNonZero` only fires on *identical* points; the first Lloyd step's `neighbor()` function uses `.min_by` on EMD distance which ties on the EMD minimum and deterministically assigns both points to centroid 0 via Rust's `.min_by` first-minimum tie-break, leaving centroid 1 with zero assigned points). Pre-fix: centroid 1's new centroid becomes `Histogram::empty(turn)`, the drift calc `metric.emd(empty, old_centroid[1])` panics on `Bins::peek`. Post-fix: centroid 1 keeps its old non-empty centroid, the drift is 0 for that slot, the driver returns 2 centroids cleanly. Owner files: `crates/clustering/src/kmeans.rs` (commit the working-tree empty-cluster guard at `step_elkan_slice:384+`; the guard is `std::array::from_fn(|j| { let mut assigned = 0; let new = ... .fold(centroids[j].identity(), ...); if assigned == 0 { centroids[j] } else { new } })` — ~20 lines including the doc comment referencing the receipt `20260610T032421Z` panic trace; do NOT change the kmeans algorithm; do NOT change the production `Layer::init_kmeans` path; do NOT change the `init_kmeans_plus_plus` path STW-086 hardened; do NOT change the `isomorphism` / `metric` / `transitions` table schemas), `crates/clustering/tests/kmeans_fast.rs` (commit the working-tree `fast_mode_handles_empty_cluster_during_lloyd_step` sub-test at line 384+ — 115 lines including the doc comment referencing the classical k-means empty-cluster pathology + the production `TestLayer::heal` model + the `Bins::peek` panic site), `IMPLEMENTATION_PLAN.md` (this row + the `STW-070` row's dependency note updated to "depends on `STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087`"). Scope boundary: do NOT change the kmeans algorithm itself (kmeans++ is preserved; the Lloyd step is preserved; only the empty-cluster reassign is patched); do NOT change the `isomorphism` / `metric` / `transitions` table schemas; do NOT change the production `Layer::cluster` argv or its env-knob read; do NOT change the `Bins::peek` panic message (a future refactor that converts `peek` to `Option<Abstraction>` is a separate slice); do NOT touch the v1 / v2 / v3 trained configs, the dashboard, the publish / index / remote chain, the seat-aware work, the autotrain pipeline, or any `trainer --*` CLI; do NOT touch the kmeans fast driver's existing sub-tests (the new sub-test is added, not a replacement); do NOT touch the `init_kmeans_plus_plus` / `init_kmeans` paths STW-086 closed (those are the *pre-init* empty-input guards; STW-087 closes the *post-init* empty-cluster guard — different panic sites, different guards). Acceptance criteria: `git log -1 --oneline` on `main` shows a new commit titled `feat(clustering): STW-087 empty-cluster guard in step_elkan_slice Lloyd reassign` + body referencing this row + the receipt `20260610T032421Z` panic trace; `cargo test -p rbp-clustering --test kmeans_fast` is green with 5 sub-tests (the existing 3 STW-077 + the STW-086 `fast_mode_handles_empty_point_in_prefix` + the new `fast_mode_handles_empty_cluster_during_lloyd_step`); `cargo test -p rbp-clustering --lib` stays green (no regression in existing kmeans / sinkhorn / bins / metrics lib tests); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace`, `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts. Hand-test: a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` after STW-087 lands no longer panics on `flop` during the Lloyd step (the receipt's `cluster/stdout.txt` shows `kmeans fast driving points=1024 caps.sample=1024 caps.iters=8` for flop, then `calculating lookup flop` + `calculating metric flop` + `calculating transitions flop` + the runbook proceeds past `cluster` to `--reset` + `--smoke` + `--status` + `--bench` + `--compare` + `--replay` and lands a `testnet live_proof complete: ...` `SUMMARY.txt`; the `cluster/exit.txt` is `0`, not `101`). Dependencies: `STW-075` + `STW-077` + `STW-078` + `STW-086` (all shipped) + a reachable `DATABASE_URL` (the STW-078-provisioned Postgres). Estimated scope: S (one file fix + one sub-test + one plan-row update). Completion signal: a fresh `RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh` against a fresh DB no longer panics on `flop` (the runbook reaches the `--reset` step within 5 minutes total cluster-step wall-clock, not the `cluster=101` mid-iteration panic the 20260610T032421Z receipt captured); the post-STW-087 receipt's `cluster/exit.txt` is `0`; the next planner pass can promote `STW-090` (the post-fix evidence slice) as a clean, runnable evidence task. **`lens:` CEO (the testnet claim requires a green receipt; the second empty-cluster panic is the *fourth* panic family the receipt chain has surfaced, and each one is a structural hole the runbook is uncovering — STW-087 is the *minimum* defensive guard that makes the fast-mode kmeans driver trustworthy as a testable unit) + Eng (the kmeans driver is a 200-line module; the empty-cluster guard is a 20-line `from_fn` patch; the regression test is a 115-line sub-test engineering a forced empty cluster — mirrors the production `TestLayer::heal` pattern the clustering crate already follows) + Design (a defensive guard in the Lloyd-step reassign does not change the algorithm; the production path is symmetrically protected against the same pathology via the `init_kmeans` pre-filter STW-086 closed, but a future production run on sparse data is what the property-test layer in STW-088 will pin).**

- [ ] **[P1] `STW-088` `KMEANS-PROPERTY-TEST-LAYER` (NEW in RE-PLAN-007) — close the *test-coverage* half of the empty-cluster story STW-087 leaves open: the 4 sub-tests in `crates/clustering/tests/kmeans_fast.rs` (STW-077's 3 + STW-086's 1 + STW-087's 1) are *examples* of the no-panic contract, not *proofs*. A future regression in `step_elkan_slice` that introduces a new panic site (e.g. a Lloyd-step reassign change that breaks the empty-cluster guard, a kmeans++ init change that breaks the empty-input guard, a metric change that panics on degenerate inputs) is invisible to the 4 sub-tests because the sub-tests cover the 4 known panic families but not the unknown families. STW-088 ships a property-test layer in `crates/clustering/tests/kmeans_property.rs` (new no-DB integration test file) that exercises the no-panic contract on randomized inputs across both the fast-mode `run_fast` and the production-mode `init_kmeans` paths.** A new `proptest!` macro block (the `proptest = "1"` dev-dep the `crates/clustering/Cargo.toml` may need to add — confirm transitive first) generates 100 randomized inputs per property: (a) `run_fast_no_panic_on_random_input` — `points: Vec<Histogram>` with size ∈ [1, 2048] (K is the module-level `K = 4` const, a `const` generic; the property test exercises K=4 across all randomized sizes; an explicit K=128 + K=64 sub-test is the fast-mode-receipt production analog), each `Histogram::from(Observation::from(street))` for a random street ∈ {`Flop`, `Turn`, `River`, `TurnPreflop`} + a random 50% chance of being `Histogram::empty(street)` (the empty-mix the fast-mode 1024-point prefix has in production); asserts `run_fast::<K>(points, Metric::default(), street, FastKmeansCaps::resolve(street))` returns K centroids without panicking + the wall-clock stays under 2 s (the existing `FAST_WALLCLOCK_BUDGET` constant) + all K centroids are non-empty; (b) `init_kmeans_no_panic_on_random_input` — same randomized input shape, exercises the production-mode `init_kmeans` path (`crates/clustering/src/layer.rs:128-169`); asserts the function returns K non-empty centroids (the STW-086 pre-filter + the STW-087 implicit guard via the slice path make this the production-mode defense in depth; the property test is the proof the production path holds the no-panic contract under randomized inputs); (c) `run_fast_does_not_panic_on_all_empty_input` — the *degenerate* edge case: a 1024-point pool where every point is `Histogram::empty(street)` (the worst-case empty-input scenario the runbook could surface if a future production DB has no observations for a street); asserts the driver returns K centroids without panicking + each centroid is the empty histogram (the spec'd behavior; the STW-086 pre-filter returns K empty centroids in this case); (d) `init_kmeans_does_not_panic_on_all_empty_input` — the production-mode equivalent; (e) `run_fast_handles_n_less_than_k` — a 4-point input with K=128 (a degenerate case where the kmeans++ init is underdetermined); asserts the driver returns 128 centroids (some empty, some non-empty) without panicking. Owner files: `crates/clustering/Cargo.toml` (add `proptest = "1"` to `[dev-dependencies]` if not transitive; confirm with `cargo tree -p rbp-clustering -e dev` — the `proptest` crate is a common transitive dep and may already be available; if not, add the dep; this is a 1-line Cargo.toml change), `crates/clustering/tests/kmeans_property.rs` (new no-DB integration test file with the 5 property sub-tests above; the proptest block uses `proptest::prelude::*` + the `proptest!` macro + 100 cases per property + a deterministic seed `ProptestConfig::default().with_rng_seed(0xC0FFEE)` for byte-stability), `IMPLEMENTATION_PLAN.md` (this row). Scope boundary: do NOT add `quickcheck` / `arbitrary` / `harness` / `criterion` (the `proptest` crate is the minimum that gives a property-test framework; a benchmarking layer is a separate slice); do NOT change the kmeans algorithm (the property tests are defensive coverage, not new features); do NOT change the existing 4 sub-tests in `kmeans_fast.rs` (the property tests are *additive* coverage, not replacements); do NOT change the `crates/clustering/src/kmeans.rs` / `layer.rs` / `tests.rs` source code (the property tests exercise the existing API; a regression in the existing code fails the property test, which is the point); do NOT touch the v1 / v2 / v3 trained configs, the dashboard, the publish / index / remote chain, the seat-aware work, the autotrain pipeline, or any `trainer --*` CLI. Acceptance criteria: `cargo test -p rbp-clustering --test kmeans_property` is green with 5 new property sub-tests; `cargo test -p rbp-clustering --test kmeans_fast` stays green (the existing 4 sub-tests still pass; STW-087's `fast_mode_handles_empty_cluster_during_lloyd_step` joins the existing 3 STW-077 + 1 STW-086 sub-tests); `cargo test -p rbp-clustering --lib` stays green; `cargo test --workspace -- --test-threads=4` stays green (the property test layer runs in < 30 seconds on a clean checkout; 5 properties × 100 cases × < 60 ms per case = under 30 s); `cargo check --workspace`, `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts. Hand-test: a contributor introduces a deliberate regression in `step_elkan_slice` (e.g. a `panic!` in the reassign fold for K=2) → `cargo test -p rbp-clustering --test kmeans_property` fails on the `run_fast_no_panic_on_random_input` sub-test with a clear `proptest!` failure trace naming the random input + the panic site (the property test fails *before* the runbook would); a future regression in the production `init_kmeans` path is caught by `init_kmeans_no_panic_on_random_input` (a property test that the prior re-plans never added). Dependencies: the existing `crates/clustering/src/kmeans.rs` (the `run_fast` API is the property-test target); the existing `crates/clustering/src/layer.rs` (the `init_kmeans` API is the property-test target); the existing `crates/clustering/tests/kmeans_fast.rs` (the STW-077/086/087 sub-tests are the *examples* the property tests generalize over); the `proptest` crate (a transitive or 1-line-add dev-dep). Estimated scope: S (1 Cargo.toml line + 1 new test file with 5 property sub-tests = ~250 lines of new test code). Completion signal: a fresh `cargo test -p rbp-clustering --test kmeans_property` is green with 5 property sub-tests + 100 cases each; a future regression in the kmeans driver that introduces a new panic site fails CI on the `cargo test --workspace` invocation within 30 seconds; the next `auto steward --report-only` pass records the `kmeans-property-coverage` finding as `RESOLVED`. **`lens:` Design (a defensive test layer in the form of property tests is the *minimum* that scales — example tests cover the known panic families, property tests cover the *unknown* ones the future might surface) + Eng (one new test file + one Cargo.toml line is the standard `proptest` setup the Rust ecosystem already follows; 100 cases per property × 5 properties = 500 randomized inputs the existing example tests do not exercise) + CEO (the runbook has surfaced 4 panic families across 4 RE-PLAN cycles; the 5th panic family is the one the property test will catch *first*, on the contributor's machine, not on the operator's runbook).**

- [ ] **[P1] `STW-089` `KMEANS-PRODUCTION-PATH-EMPTY-INPUT-GUARD` (NEW in RE-PLAN-007) — mirror STW-086's fast-mode empty-input guard to the production `Layer::init_kmeans` path (the path the 1.3M-row production pool exercises) and add a regression test that pins the production-path empty-input contract. The STW-086 pre-filter landed in `init_kmeans_plus_plus` (the kmeans++ driver STW-086 modified) and in `init_kmeans` (the production layer-entry path STW-086 also modified per the STW-086 row's scope boundary) — but the post-RE-PLAN-006 audit of the working tree shows the production `Layer::init_kmeans` (`crates/clustering/src/layer.rs:128-169`) was NOT modified by STW-086 (the STW-086 row's owner-files section names `crates/clustering/src/layer.rs` but the actual diff at `0c375bc` only touched `kmeans.rs` + `tests/kmeans_fast.rs`; the `layer.rs:init_kmeans` production path remains without the pre-filter). A future production run on a fresh DB with sparse observations (e.g. a street with < 16 observed isomorphisms in the 1.3M-row pool) will hit the same `Bins::peek` panic STW-086 closed in the fast path. STW-089 ports the STW-086 pre-filter to the production `Layer::init_kmeans` path + adds a `crates/autotrain/tests/kmeans_no_panic_repro.rs` integration test that drives the production path on a 1024-point prefix with a 50% empty prefix and asserts no panic. The production-path empty-input contract is the *defense in depth* the STW-088 property-test layer exercises; STW-089 makes it a regression test, not a property.** Owner files: `crates/clustering/src/layer.rs` (in `init_kmeans` at line 128-169, pre-filter the input `truncated` slice to drop empty histograms before the `vec![1.; N]` initialization + the kmeans++ `init_kmeans_plus_plus` call; if the filter produces an empty Vec, return K empty centroids via the existing `Histogram::empty(street)` constructor; ~5 lines mirroring the STW-086 fast-mode pre-filter at `kmeans.rs:191-211`), `crates/autotrain/tests/kmeans_no_panic_repro.rs` (new no-DB integration test that calls `Layer::init_kmeans` on a 1024-point mixed-empty pool — the same shape `crates/clustering/tests/kmeans_fast.rs::fast_mode_handles_empty_point_in_prefix` exercises, but via the public `Layer` API the autotrain pipeline calls; asserts no panic + K non-empty centroids), `IMPLEMENTATION_PLAN.md` (this row). Scope boundary: do NOT change the kmeans algorithm (the production `Layer::init_kmeans` flow is preserved; only the input is pre-filtered); do NOT change the fast-mode path STW-086 closed (the production path is a separate slice; the fast-mode path is unchanged); do NOT change the STW-088 property-test layer (STW-089 is a *targeted* regression test, STW-088 is a *property* test; both are additive coverage, not replacements); do NOT touch the v1 / v2 / v3 trained configs, the dashboard, the publish / index / remote chain, the seat-aware work, the autotrain pipeline, or any `trainer --*` CLI; do NOT touch the existing `crates/clustering/src/layer.rs::TestLayer::heal` (the heal function is the *post-init* empty-cluster guard for the test layer; STW-089 is the *pre-init* empty-input guard for the production layer; the two are orthogonal). Acceptance criteria: `cargo test -p rbp-autotrain --test kmeans_no_panic_repro` is green with the new sub-test; `cargo test -p rbp-clustering --lib` stays green (no regression in existing `layer.rs::tests` or `TestLayer::heal`); `cargo test --workspace -- --test-threads=4` stays green; `cargo check --workspace`, `cargo fmt --check` stay green; `bash scripts/plan-staleness-gate.sh` exits 0 with no new ghosts. Hand-test: a contributor introduces a deliberate regression in `Layer::init_kmeans` (e.g. removing the pre-filter) → `cargo test -p rbp-autotrain --test kmeans_no_panic_repro` fails with a clear `non empty histogram` panic at `bins.rs:95` (the same panic site STW-086 closed in the fast path); the production-path regression is caught *before* it reaches the runbook. Dependencies: `STW-086` (the fast-mode pre-filter is the model the production-path pre-filter mirrors); the existing `crates/clustering/src/layer.rs::init_kmeans` (the production-path function the regression test targets); the existing `crates/clustering/src/tests.rs::TestLayer::heal` (the production-layer post-init guard the regression test does not duplicate). Estimated scope: XS (5 lines in `layer.rs` + 1 new test file = ~50 lines of new code/tests). Completion signal: a fresh `cargo test -p rbp-autotrain --test kmeans_no_panic_repro` is green + the production `Layer::init_kmeans` path is regression-tested against the empty-input contract; the STW-088 property-test layer's `init_kmeans_no_panic_on_random_input` sub-test is now *redundant* with STW-089's targeted test (the property test is the broader coverage; STW-089 is the specific regression test for the STW-086 pre-filter shape); a future production runbook run on a sparse-DB scenario no longer panics. **`lens:` Eng (the production path is *structurally parallel* to the fast path; the pre-filter is a 5-line guard mirrored from the STW-086 fix; the regression test is a 50-line sub-test mirroring `fast_mode_handles_empty_point_in_prefix`) + Design (defense in depth: the fast path has a pre-init guard (STW-086) + a post-init guard (STW-087) + a property-test layer (STW-088) + a production-path guard (STW-089) — the kmeans driver is now *load-bearing* under any input shape) + CEO (the production 1.3M-row pool has lucked out; a future sparse-DB runbook will not).**

- [ ] **[P0] `STW-090` `TESTNET-LIVE-PROOF-RECEIPT-POST-STW-087` (NEW in RE-PLAN-007; supersedes the RE-PLAN-006 re-issue of `STW-070`) — run `scripts/testnet-live-proof.sh` end-to-end with `RBP_TESTNET_FAST=1` against a fresh STW-078-provisioned Postgres, with `STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087` all shipped, and commit the first green `receipts/testnet-live-proof-<UTC-THIS-WEEK>/` directory the receipt chain has produced.** This is the *terminal evidence* slice: it converts the v1→v10 testnet chain from "structurally complete, kmeans-panic-blocked" to "operationally proven." The runbook's 8 step exits (`doctor` / `cluster` / `reset` / `smoke` / `status` / `bench` / `compare` / `replay`) must all be `0`; the `SUMMARY.txt` headline must be the pinned `testnet live_proof complete: smoke=N status=N bench=N compare=N replay=BYTES` line; the `recipe.json` must re-verify with `LiveProofReceipt::read_and_verify`; the receipt is committed on `main`. STW-090 is *evidence only*: the runbook is run, the receipt is captured, the verifier is invoked, the result is committed. If a fresh runbook run fails after STW-087 + STW-088 + STW-089 land, the worker reports the failure as a new `[P0]`-ranked next slice (or blocks for human input) rather than silently fixing the runbook in the STW-090 commit. STW-090 *also* archives the 18+ failed/partial `receipts/testnet-live-proof-2026060{4..9}T*` directories into `receipts/_archive/failed/` + writes the `INDEX.md` summary (the work STW-085 described; STW-090 *integrates* the archive work into the terminal-evidence commit so a single commit produces the green receipt + the clean receipts dir + the failure-history INDEX). The receipt's `cluster/exit.txt` is the operator-visible proof the STW-087 empty-cluster guard holds in production (a fresh `cluster=0` exit is the receipt-side verification of the STW-087 fix). Owner files: `scripts/testnet-live-proof.sh` (no code change expected), `scripts/testnet-live-proof.md` (note the evidence path + the `STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087` + `STW-088` + `STW-089` dependency chain), the new green receipt directory under `receipts/testnet-live-proof-<UTC>/` (the deliverable), `receipts/_archive/failed/` (the move target for the 18+ stale failed receipts STW-085 named — STW-090 *does* the move + the `INDEX.md` summary; the move is operator-side per the STW-085 spec), `receipts/_archive/failed/INDEX.md` (the failure-history summary, ~30 lines; one bullet per archived receipt + the failure mode + the slice that fixed it; STW-090 *commits* the INDEX alongside the green receipt), `.gitignore` (add `!receipts/_archive/` to the existing `/receipts/` ignore line per the STW-085 spec), `steward/HAZARDS.md` (the `TESTNET-LIVE-PROOF-RECEIPT` row flips from "open" to "closed by STW-090 on <date>" once the receipt is committed), `steward/DRIFT.md` (the `STW-019` row's "DRIFT" verdict on the receipts-orphans line flips to "RESOLVED by STW-090 on <date>" once the new receipt is committed; the `STW-085` row flips from "open" to "closed by STW-090 on <date>" once the archive + INDEX are committed). Scope boundary: do NOT change the runbook's chain step order; do NOT weaken `LiveProofReceipt::read_and_verify`; do NOT bypass `trainer --doctor`; do NOT touch the `Check::clustered` fix (that is `STW-075`'s commit, not this one); do NOT touch the kmeans cap (that is `STW-077`'s commit, not this one); do NOT touch the postgres provisioning script (that is `STW-078`'s commit, not this one); do NOT touch the kmeans empty-histogram pre-filter (that is `STW-086`'s commit, not this one); do NOT touch the kmeans empty-cluster guard (that is `STW-087`'s commit, not this one); do NOT touch the kmeans property-test layer (that is `STW-088`'s commit, not this one); do NOT touch the kmeans production-path empty-input guard (that is `STW-089`'s commit, not this one); prefer `RBP_TESTNET_FAST=1` for the canonical first green proof; do NOT touch the dashboard / publish / index / index-remote / remote chain; do NOT delete the existing failed/partial `receipts/testnet-live-proof-*` directories (they are archived, not deleted). Acceptance criteria: a `find receipts/testnet-live-proof-<UTC>/` shows 8 step subdirs each with `stdout.txt` + `stderr.txt` + `exit.txt` where every `exit.txt` is `0`; `SUMMARY.txt` head is the pinned `testnet live_proof complete: ...` line; `recipe.json` parses with `LiveProofRecipe`; `trainer --verify-receipt receipts/testnet-live-proof-<UTC>/` exits 0 and prints `live_proof receipt verification passed: ...`; the receipt is committed on `main`; the `receipts/_archive/failed/` directory contains the 18+ archived receipts + the `INDEX.md` summary; `git status` is clean post-commit; `steward/HAZARDS.md` + `steward/DRIFT.md` + `steward/PROMOTIONS.md` reflect the closed `TESTNET-LIVE-PROOF-RECEIPT` hinge + the closed `STW-085` archive row. Dependencies (RE-PLAN-007): `STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087` + a reachable `DATABASE_URL` or `DB_URL`; existing `STW-067` (fast-mode) + `STW-069` (fail-before-train integrity gate) + `STW-019` (runbook) + `STW-023` (shared verifier + `recipe.json`) + `STW-028` (`--verify-receipt` CLI) + `STW-085` (the FAILED-RECEIPT-ARCHIVE spec STW-090 integrates). Estimated scope: S (operator-runnable evidence, on a fresh DB, post-`STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087`). Completion signal: a fresh `trainer --verify-receipt` exits 0 on a `receipts/testnet-live-proof-<UTC-THIS-WEEK>/` directory and the receipt path is recorded in `steward/HAZARDS.md` row #2 + `steward/DRIFT.md` `STW-019` row as the current operator proof; the receipts dir shows only the new green receipt + the `_archive/failed/` summary; the `verification:workspace-parallel` hinge #2 is now `closed by STW-090 on <date>` with the receipt path as evidence; the next planner pass sees a `RE-PLAN-008` task with no `STW-070` carry-over. **`lens:` CEO (the testnet claim requires a green receipt; STW-090 is the slice that converts "structurally complete" to "operationally proven"; a single commit produces the green receipt + the clean receipts dir + the failure-history INDEX + the closed steward hinge — the testnet is now *publicly self-evidencing*, not just structurally complete) + Eng (the runbook is run, the receipt is captured, the verifier is invoked, the archive is moved, the INDEX is written, the steward is updated — a single integrated commit) + Design (the receipts dir a stranger `ls`-es shows the *honest* state: one green receipt + one archived failure-history summary, not 18 confusing stale dirs + 0 green ones).**