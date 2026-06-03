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

- [ ] `STW-008` End-to-end hand persistence round-trip test.
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
| not mainnet-b  |                                                       | `STW-001` planning surface; `STW-007` artifact retirement |

## Promotion provenance

The rows above were promoted from
`/tmp/robopoker-steward-9283/PROMOTIONS.md` on 2026-06-03 as part of kanban
task `t_9283ea83`. The first promotion to land is `STW-003` (highest-priority
hinge, mainnet-blocking, user-facing).
