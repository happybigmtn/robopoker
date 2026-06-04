# Implementation Plan

This plan is promoted from the `auto steward --report-only` pass that ran on
2026-06-03 against `main`. Source of truth for the promoted rows is
`/tmp/robopoker-steward-9283/{DRIFT,HAZARDS,HINGES,PROMOTIONS,RETIRE,STEWARDSHIP-REPORT}.md`.

`auto parallel` consumes this file. Each item has an owner set, a scope
boundary, acceptance criteria, verification commands, dependencies, and a
completion signal so a worker can claim and finish it without re-discovering
context.

## Active items (worker-ready)

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

- [ ] `STW-011` Rule-based named baselines (`CallStation`, `Maniac`, `Tight`) + bench baseline selector.
  Owner files: `crates/gameroom/src/players/{callstation,maniac,tight}.rs` (new),
  `crates/gameroom/src/players/mod.rs`,
  `crates/autotrain/src/bench.rs`,
  `crates/autotrain/src/mode.rs`,
  `IMPLEMENTATION_PLAN.md`,
  `genesis/plans/000-ceo-testnet-roadmap.md`.
  Scope boundary: add three deterministic, named, rule-based baseline
  `Player` implementations to `rbp-gameroom::players`, plumb a
  `RBP_BENCH_BASELINE` env-var selector into `trainer --bench` (default
  `fish`), and stamp the chosen baseline name into the `BenchReport`
  JSON. Do NOT introduce a new `Player` kind, do NOT change the
  `DatabasePlayer` / `Room` / `Schema` surface, do NOT touch the
  trained-blueprint hydration path, do NOT make any baseline inspect
  the blueprint.
  Acceptance criteria:
  (a) `crates/gameroom/src/players/callstation.rs` — a `CallStation`
      unit struct that, on every decision, picks `Check` if legal,
      else the call amount (i.e. never folds and never raises, the
      textbook "calling station"). Its `decide()` must never
      return a fold or a raise/shove; if the only legal move is a
      fold (e.g. facing an all-in with insufficient chips) it
      still calls because a call is legal when the stake is
      coverable, and the runtime is guaranteed to never face
      a sole-fold decision on a stack that can cover the bet.
  (b) `crates/gameroom/src/players/maniac.rs` — a `Maniac` unit
      struct that, on every decision, picks the all-in shove if
      legal, else the max raise. On the very first decision of a
      hand (no prior raise), `Raise` and `Shove` are both legal
      and the maniac picks `Shove`. On a subsequent bet where
      calling is the only legal move (a shove is already on
      the table), the maniac calls — this preserves the
      invariant that the chosen action is always in
      `recall.head().legal()`.
  (c) `crates/gameroom/src/players/tight.rs` — a `Tight` struct
      parameterized by a preflop hand-rank threshold (default
      `TIGHT_THRESHOLD = 7`, the 7th-best starting hand: pocket
      pairs TT+, AK, AQ, AJ, KQs). Preflop, the `Tight` player
      calls/raises only if its two hole cards' high-rank index
      is at or above the threshold (so it plays a tight range
      and folds most hands). On the flop/turn/river, `Tight`
      falls back to the `CallStation` policy (always check or
      call) so the postflop decision is deterministic and the
      hand deterministically sees a showdown with at most
      preflop raises. The threshold is `pub const` so a worker
      can vary it (and the bench math is independent of it).
  (d) `crates/gameroom/src/players/mod.rs` re-exports
      `CallStation`, `Maniac`, `Tight` alongside the existing
      `Fish`, `Human`, `DatabasePlayer`, `RealTimePlayer`,
      `ZeroTempPlayer`. All three are unit structs (or
      zero-sized for `Tight`) with no allocation cost.
  (e) `crates/autotrain/src/bench.rs` — a new `bench_baseline()`
      env-var reader returning an enum-shaped value
      (`Fish | CallStation | Maniac | Tight`, default `Fish`,
      unknown value also defaults to `Fish` and emits a
      `log::warn!`); a new `BenchReport.baseline: String` field
      stamped with the selected name; the `run_hands` signature
      parameterised on the chosen baseline so the seat-1 player
      is `CallStation` / `Maniac` / `Tight` instead of `Fish`
      when the env var selects one. The `to_json` output
      includes `"baseline": "<name>"` so a downstream scraper
      can read it with `jq '.baseline'`.
  (f) `crates/autotrain/src/bench.rs::tests` — six new lib tests
      pinning the deterministic policy of each baseline:
      `callstation_picks_check_when_legal_else_call`,
      `callstation_never_picks_fold_or_raise`,
      `maniac_picks_shove_when_legal`,
      `maniac_falls_back_to_call_when_shove_not_legal`,
      `tight_plays_premium_preflop_and_folds_garbage`,
      `tight_postflop_plays_like_callstation`. These tests
      construct a known `Game` (no I/O, no DB) and check that
      `CallStation::decide`, `Maniac::decide`, and `Tight::decide`
      return the expected `Action` variants. They pin the policy
      at the API boundary so a future refactor that lets one of
      these strategies fold or raise accidentally is caught at
      unit-test time rather than discovered in a 200-hand
      bench run.
  (g) The default `RBP_BENCH_BASELINE` is `fish`; an
      integration test in
      `crates/autotrain/tests/bench.rs` (gated on `database` +
      `DATABASE_URL` like the existing bench test) drives
      `trainer --bench` with `RBP_BENCH_BASELINE=callstation`
      and asserts the printed JSON contains
      `"baseline":"callstation"`. This is the only new test in
      that file.
  Verification commands:
  `cargo test -p rbp-gameroom --lib`,
  `cargo test -p rbp-autotrain --lib`,
  `cargo test -p rbp-autotrain --features database --tests --lib`,
  `cargo test --workspace -- --test-threads=4`,
  `cargo check --workspace`,
  `cargo fmt --check`.
  Required tests: the six lib tests in (f) + the one integration
  test in (g). They are the only new tests this slice adds (no
  padding of unrelated suites).
  Dependencies: `STW-010` (head-to-head bench harness) for the
  `bench::run` / `bench::run_hands` / `BenchReport` surface this
  slice extends.
  Estimated scope: M.
  Completion signal: `trainer --bench` with
  `RBP_BENCH_BASELINE=callstation` exits 0 on a freshly-`--reset`
  DB and prints `"baseline":"callstation"` in the JSON; the six
  lib tests pin each baseline's policy; the existing
  `cargo test --workspace` and `cargo fmt --check` stay green.

- [x] `STW-010` Head-to-head bench harness for trained `DatabasePlayer` vs `Fish`.
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
