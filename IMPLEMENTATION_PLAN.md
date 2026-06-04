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
