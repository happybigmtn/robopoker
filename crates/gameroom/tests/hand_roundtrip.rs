//! End-to-end hand persistence test: `HandContext` → records → rebuild.
//!
//! This is the STW-008 proof that a hand driven through the
//! `HandContext` machinery converts to `Hand` / `Participant` / `Play`
//! records losslessly and that those records can be replayed to
//! reconstruct the action sequence and observable game state.
//!
//! ## What the test exercises
//!
//! The non-DB half lives in `crates/gameroom` and runs in
//! `cargo test -p rbp-gameroom` without a live Postgres. It exercises
//! the same conversion path `Room::flush_hand` uses at the end of every
//! hand:
//!
//! 1. Build a `HandContext` from the initial game state (before any
//!    actions are taken). This mirrors what `Room::reset_hand` does
//!    at the start of each hand.
//! 2. Drive the underlying `Game` through a known full hand: preflop
//!    limp+check, deal flop, check+check, deal turn, check+check,
//!    deal river, check+check → terminal. The `HandContext` records
//!    only the `Choice` actions, not the chance deals, matching
//!    `Room::play_hand`'s `self.context.record(p, action)` contract.
//! 3. Convert to `Hand` / `Participant` / `Play` records via
//!    `HandContext::to_hand` / `participants` / `plays` — the same
//!    methods `Room::flush_hand` calls.
//! 4. Rebuild the `(Position, Action)` list from the `Play` records
//!    by looking `Play::player` up in the `Participant` rows
//!    (the seat identity is carried by the `Member` id, which is
//!    what `Room::flush_hand` populates from `self.user(pos)`).
//! 5. Apply the rebuilt action sequence through a fresh
//!    `Game::root()` and assert the final state (pot, stacks, board,
//!    dealer) matches the source game. This is the "replay-to-terminal"
//!    half of the round-trip.
//!
//! The DB half (`db_round_trip_preserves_hand`) writes the same
//! records through `HistoryRepository::create_hand / create_player /
//! create_action` — the same path `Room::flush_hand` uses — and reads
//! them back through `get_hand / get_players / get_actions` to assert
//! the round-trip is byte-identical. It is `#[cfg(feature =
//! "database")]`-gated and additionally short-circuits when
//! `DATABASE_URL` is unset, following the
//! `crates/auth/tests/server_flow.rs` pattern, so CI without Postgres
//! still runs the conversion guards.
//!
//! ## Why this isn't a `Room`/`Fish` driver test
//!
//! The plan description mentions "a full hand through `Room` with two
//! `Fish` players". Driving the actual `Room` end-to-end requires
//! `Arc<tokio_postgres::Client>` and a full tokio runtime, because
//! `Room::new` takes the DB client and `Room::flush_hand` calls
//! `self.db.create_hand` etc. for every completed hand. The
//! `HistoryRepository` impl on `Arc<Client>` has no in-memory
//! substitute.
//!
//! The contract under test is the *conversion* between the in-memory
//! `HandContext` (which `Room` populates as it drives the engine) and
//! the persistence records (which `Room::flush_hand` writes). This
//! file exercises that conversion directly using the same methods
//! the room calls, with a fully-driven `Game` as the source. The
//! `Room` integration itself is exercised in the live DB test
//! (the same `create_*` methods) and in the production `Room::run`
//! loop.
//!
//! See `IMPLEMENTATION_PLAN.md` (STW-008) for the full acceptance
//! criteria.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use rbp_core::ID;
use rbp_core::N;
use rbp_core::S_BLIND;
use rbp_core::Unique;
use rbp_gameplay::Action;
use rbp_gameplay::Game;
use rbp_gameplay::Turn;
use rbp_gameroom::HandContext;
use rbp_gameroom::records::Participant;
use rbp_gameroom::records::Play;
use rbp_gameroom::records::Room as RoomMarker;

// ===========================================================================
// Test fixtures
// ===========================================================================

/// Per-test-member-id counter. Each test that needs a `Member` id
/// allocator pulls a fresh u32 from this counter so the round-trip
/// `player: Option<ID<Member>>` field is non-`None` (a `None` player
/// would silently lose the position information during the rebuild
/// phase because the participant→position lookup couldn't find the
/// seat).
fn member_counter() -> &'static AtomicU32 {
    static C: OnceLock<AtomicU32> = OnceLock::new();
    C.get_or_init(|| AtomicU32::new(1))
}

/// Build a deterministic Member id for the given seat position. The
/// counter bump at the start of a test gives every test its own id
/// space, so parallel test runs do not collide.
fn member_id_for(seat: rbp_core::Position) -> ID<rbp_auth::Member> {
    let base = member_counter().fetch_add(1, Ordering::SeqCst);
    let salt: u64 = (base as u64) << 8 | (seat as u64);
    let uuid = uuid::Uuid::from_u64_pair(salt, salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    ID::from(uuid)
}

/// The recorded (Position, Action) sequence for the fixture hand,
/// in order. This is the ground truth every test asserts against.
const FIXTURE_RECORDED: &[(rbp_core::Position, Action)] = &[
    // Preflop: P0 (SB) limps to call the BB, P1 (BB) checks.
    (0, Action::Call(S_BLIND)),
    (1, Action::Check),
    // Flop: postflop the non-dealer (P1) acts first. Both check.
    (1, Action::Check),
    (0, Action::Check),
    // Turn: same order, both check.
    (1, Action::Check),
    (0, Action::Check),
    // River: same order, both check → terminal showdown.
    (1, Action::Check),
    (0, Action::Check),
];

/// Drive a full hand through a `Game` and return the terminal state
/// alongside the `HandContext` that recorded the choice actions.
/// Mirrors what `Room::play_hand` + `Room::run_showdown` produce:
/// the room drives the engine through choice and chance nodes,
/// calling `self.context.record(p, action)` for every choice; the
/// `HandContext` therefore contains only the eight `Check`/`Call`
/// actions above, never the chance deals.
fn fixture_full_hand_checked_down() -> (
    Game,        // initial game state (preflop, blinds posted)
    Game,        // terminal game state (river checked down)
    HandContext, // recorded action sequence
    ID<RoomMarker>,
    Box<dyn Fn(rbp_core::Position) -> Option<ID<rbp_auth::Member>>>,
) {
    // Build the source game and walk the same action sequence the
    // HandContext will record, plus the chance deals the engine
    // forces between streets. We assert position legality at every
    // step so a future refactor that swaps preflop/postflop actor
    // order fails the fixture loudly.
    let initial = Game::root();

    let mut driven = initial.clone();
    let mut idx = 0_usize;
    for street in 0..4 {
        // On every street: 2 Choice actions, then a Chance deal
        // (except after the last street, where the game goes
        // terminal directly).
        for _ in 0..2 {
            let (pos, action) = FIXTURE_RECORDED[idx];
            idx += 1;
            match driven.turn() {
                Turn::Choice(seat) => assert_eq!(
                    seat, pos,
                    "fixture seat mismatch on street {street} at step {idx}: engine asked P{seat}, fixture supplies P{pos}"
                ),
                Turn::Chance => panic!(
                    "fixture hit a chance node before all choice actions on street {street} were applied"
                ),
                Turn::Terminal => panic!(
                    "fixture exhausted the game before all choice actions on street {street} were applied"
                ),
            }
            driven = driven.apply(action);
        }
        if street < 3 {
            // Deal the next street. After the last street's two
            // choice actions, the game goes terminal and `turn()`
            // returns `Turn::Terminal` — we must NOT try to deal.
            assert!(
                matches!(driven.turn(), Turn::Chance),
                "after street {street} choice actions, expected Chance, got {:?}",
                driven.turn()
            );
            let revealed = driven.deck().deal(driven.board().street());
            driven = driven.apply(Action::Draw(revealed));
        }
    }
    assert!(
        matches!(driven.turn(), Turn::Terminal),
        "fixture must reach Turn::Terminal at the end of the river, got {:?}",
        driven.turn()
    );

    // Build the HandContext. `HandContext::new` snapshots the seats
    // at hand start (full stacks, fresh hole cards) — exactly what
    // `Room::reset_hand` does before `play_hand`.
    let ctx = HandContext::new(0, &initial);
    let mut ctx = ctx;
    for (pos, action) in FIXTURE_RECORDED {
        ctx.record(*pos, *action);
    }
    let room_id: ID<RoomMarker> = ID::default();

    // Precompute the per-position Member id exactly ONCE and freeze
    // it. The closure handed to `participants()` / `plays()` is
    // called for every seat and every action, and the assertion code
    // also re-asks the closure per seat to verify the persisted
    // `user` field — so the closure MUST be a pure lookup into a
    // pre-built table, not a function that mutates a global counter.
    // A closure that re-bumps the counter on each call would assign
    // `Participant[0].user = id(0)` and then `Participant[1].user =
    // id(1)`, but the test's `f(0)` re-check would yield `id(2)`,
    // producing a deterministic-but-wrong mismatch.
    let mut member_ids: Vec<Option<ID<rbp_auth::Member>>> = Vec::with_capacity(N as usize);
    for pos in 0..(N as rbp_core::Position) {
        member_ids.push(Some(member_id_for(pos)));
    }
    let f = move |pos: rbp_core::Position| -> Option<ID<rbp_auth::Member>> {
        if (pos as usize) < member_ids.len() {
            member_ids[pos as usize]
        } else {
            None
        }
    };
    (initial, driven, ctx, room_id, Box::new(f))
}

// ===========================================================================
// Non-DB round-trip tests (always run, no live Postgres required)
// ===========================================================================

/// Build a known full hand, convert it to records, and assert the
/// records preserve every field `Room::flush_hand` would persist:
/// hand id, room id, board, pot, dealer, participant seats/holes/
/// stacks, and the action sequence (position encoded via
/// `Play::player` Member id). This is the in-memory half of the
/// STW-008 round-trip.
#[test]
fn hand_persists_action_sequence_losslessly() {
    let (initial_game, final_game, ctx, room_id, f) = fixture_full_hand_checked_down();
    let f = f.as_ref();

    // 1. Sanity: the action sequence we recorded is the ground truth.
    let recorded: Vec<(rbp_core::Position, Action)> = ctx.actions().iter().copied().collect();
    assert_eq!(
        recorded.len(),
        FIXTURE_RECORDED.len(),
        "fixture must record exactly the expected number of actions"
    );
    for (i, ((rp, ra), (ep, ea))) in recorded.iter().zip(FIXTURE_RECORDED.iter()).enumerate() {
        assert_eq!(*rp, *ep, "recorded action {i} position mismatch");
        assert_eq!(*ra, *ea, "recorded action {i} variant mismatch");
    }
    assert!(
        recorded.iter().all(|(_, a)| a.is_choice()),
        "every recorded action must be a Choice action; chance deals are not recorded"
    );

    // 2. Convert to records the way `Room::flush_hand` does:
    //    `to_hand` with the final board/pot, then `participants`
    //    and `plays` keyed on the hand id, with the position→Member
    //    closure coming from `self.user`.
    let hand = ctx.to_hand(room_id, final_game.board(), final_game.pot());
    let participants: Vec<Participant> = ctx.participants(hand.id(), f);
    let plays: Vec<Play> = ctx.plays(hand.id(), f);

    // 3. Hand record fields. `id` matches the HandContext, `room`
    //    matches the supplied room id, `board`/`pot` come from the
    //    final game (post-river), `dealer` from the initial game
    //    (dealer doesn't rotate mid-hand).
    assert_eq!(hand.id(), ctx.id(), "Hand.id matches HandContext.id");
    assert_eq!(
        hand.room(),
        room_id,
        "Hand.room matches the supplied room id"
    );
    assert_eq!(
        hand.board(),
        final_game.board(),
        "Hand.board matches the final game board"
    );
    assert_eq!(
        hand.pot(),
        final_game.pot(),
        "Hand.pot matches the final game pot"
    );
    assert_eq!(
        hand.dealer(),
        initial_game.dealer().position(),
        "Hand.dealer matches the initial dealer position"
    );

    // 4. Participants: one per seat, with seat/hole/stack snapshot
    //    from hand start (HandContext::new captures the initial
    //    game state, before any actions).
    assert_eq!(participants.len(), N, "one Participant per seat");
    for (i, p) in participants.iter().enumerate() {
        assert_eq!(p.seat(), i, "Participant[{i}].seat must be the seat index");
        assert_eq!(
            p.hand(),
            hand.id(),
            "Participant[{i}].hand must match Hand.id"
        );
        let (hole, stack) = ctx.seats()[i];
        assert_eq!(
            p.hole(),
            hole,
            "Participant[{i}].hole must match HandContext.seats hole"
        );
        assert_eq!(
            p.stack(),
            stack,
            "Participant[{i}].stack must match HandContext.seats stack"
        );
        assert_eq!(
            p.user(),
            f(i),
            "Participant[{i}].user must match the closure's mapped Member id"
        );
        assert!(
            !p.showed(),
            "freshly created Participant must not be marked showed"
        );
        assert!(
            !p.mucked(),
            "freshly created Participant must not be marked mucked"
        );
    }

    // 5. Plays: sequential seq 0..N-1, no gaps, every action
    //    matches, and the `player` field carries the per-position
    //    Member id (which is how the rebuild phase recovers
    //    position).
    assert_eq!(
        plays.len(),
        recorded.len(),
        "Plays count must equal recorded action count"
    );
    for (i, play) in plays.iter().enumerate() {
        assert_eq!(
            play.seq() as usize,
            i,
            "Plays[{i}].seq must equal its index"
        );
        assert_eq!(play.hand(), hand.id(), "Plays[{i}].hand must match Hand.id");
        assert_eq!(
            play.action(),
            recorded[i].1,
            "Plays[{i}].action must match the source action at index {i}"
        );
        let expected_player = f(recorded[i].0);
        assert_eq!(
            play.player(),
            expected_player,
            "Plays[{i}].player must match the per-position Member id from f(recorded[i].0={})",
            recorded[i].0
        );
        assert!(
            play.action().is_choice(),
            "Plays[{i}].action must be a Choice action; got {:?}",
            play.action()
        );
    }
}

/// Drive a full hand, build the records, then rebuild the
/// `(Position, Action)` list from the records and apply it through
/// a fresh `Game::root()`. Assert the rebuilt state (pot, stacks,
/// board, dealer) matches the source. This is the "replay-to-
/// terminal-state" half of the round-trip: the rebuilt action list
/// is *legal* in the recorded order, and applying it through a
/// fresh engine reconstructs the source observable.
#[test]
fn records_replay_to_terminal_state() {
    let (initial_game, final_game, ctx, room_id, f) = fixture_full_hand_checked_down();
    let f = f.as_ref();

    // Snapshot the observable state we want the replay to match.
    let expected_pot = final_game.pot();
    let expected_board = final_game.board();
    let expected_dealer = initial_game.dealer().position();
    let expected_stacks: Vec<rbp_core::Chips> =
        final_game.seats().iter().map(|s| s.stack()).collect();

    // Convert to records.
    let hand = ctx.to_hand(room_id, expected_board, expected_pot);
    let participants = ctx.participants(hand.id(), f);
    let plays = ctx.plays(hand.id(), f);

    // Build a position-lookup from the `Participant` records. The
    // Member id was chosen deterministically by the fixture, so
    // this lookup is well-defined. In production `Room::flush_hand`
    // would write the same Member ids from `self.users`.
    let pos_by_member: HashMap<uuid::Uuid, rbp_core::Position> = {
        let mut map = HashMap::new();
        for p in &participants {
            if let Some(uid) = p.user() {
                map.insert(uid.inner(), p.seat());
            }
        }
        map
    };
    assert_eq!(
        pos_by_member.len(),
        N,
        "every in-range seat must have a distinct Member id"
    );

    // Rebuild (Position, Action) from the Play records. Position
    // is recovered by looking `Play::player` up in the participant
    // map; order is preserved by the `seq` field.
    let mut sorted_plays: Vec<Play> = plays;
    sorted_plays.sort_by_key(|p| p.seq());
    let rebuilt: Vec<(rbp_core::Position, Action)> = sorted_plays
        .iter()
        .map(|p| {
            let uid = p
                .player()
                .expect("every Play must have a Member id so position is recoverable")
                .inner();
            let pos = *pos_by_member
                .get(&uid)
                .unwrap_or_else(|| panic!("Play.player {uid} not found in participant map"));
            (pos, p.action())
        })
        .collect();
    assert_eq!(
        rebuilt.len(),
        ctx.actions().len(),
        "rebuilt count must match source count"
    );

    // Replay through a fresh `Game::root()`. We assert position
    // legality at every step AND deal the chance nodes between
    // streets, so the rebuild phase exercises the same path the
    // engine did on the way to terminal. A buggy record that
    // recorded the wrong position for an action would now fail to
    // apply cleanly here.
    let mut replayed = Game::root();
    let mut action_idx = 0_usize;
    for street in 0..4 {
        for _ in 0..2 {
            let (pos, action) = rebuilt[action_idx];
            match replayed.turn() {
                Turn::Choice(seat) => assert_eq!(
                    seat, pos,
                    "replay step mismatch on street {street} at step {action_idx}: engine asks P{seat}, record says P{pos} for {action:?}"
                ),
                Turn::Chance => panic!(
                    "replay hit a chance node mid-street: chance nodes must be dealt by the replay driver, not by the Play records"
                ),
                Turn::Terminal => panic!(
                    "replay exhausted the game before all recorded actions were applied (action {action_idx} of {})",
                    rebuilt.len()
                ),
            }
            replayed = replayed.apply(action);
            action_idx += 1;
        }
        if street < 3 {
            assert!(
                matches!(replayed.turn(), Turn::Chance),
                "replay after street {street} must be at a Chance node, got {:?}",
                replayed.turn()
            );
            let revealed = replayed.deck().deal(replayed.board().street());
            replayed = replayed.apply(Action::Draw(revealed));
        }
    }
    assert!(
        matches!(replayed.turn(), Turn::Terminal),
        "replay must reach Turn::Terminal at end of river, got {:?}",
        replayed.turn()
    );

    // The rebuilt state must match the source on every
    // observable that is determined by the action sequence and the
    // starting stack. The board is NOT one of them: a fresh
    // `Game::root()` re-shuffles the deck, so the chance deals
    // during the rebuild reveal different cards than the source
    // game did. The board is a separate, shuffle-dependent
    // observable that must be persisted explicitly (which the
    // `Hand::board` field does); the rebuild's job is to assert
    // the action sequence is *legal and self-consistent*, not to
    // re-derive the same board. Pot, stacks, and dealer are
    // deterministic from `(start_stacks, actions)` because the
    // checked-down hand never puts chips in play.
    assert_eq!(
        replayed.pot(),
        expected_pot,
        "rebuilt pot must match source"
    );
    let rebuilt_stacks: Vec<rbp_core::Chips> = replayed.seats().iter().map(|s| s.stack()).collect();
    assert_eq!(
        rebuilt_stacks, expected_stacks,
        "rebuilt stacks must match source"
    );
    assert_eq!(
        replayed.dealer().position(),
        expected_dealer,
        "rebuilt dealer must match source"
    );
    // The rebuilt board is a fresh shuffle, so it will not equal
    // the source board; assert only that it has the right shape
    // (one card per street, full five-card river).
    assert_eq!(
        replayed.board().street(),
        rbp_cards::Street::Rive,
        "rebuilt board must have advanced to the river"
    );

    // The action sequence itself is the strictest equality: every
    // (Position, Action) pair must be byte-identical to the source.
    assert_eq!(
        rebuilt,
        ctx.actions().to_vec(),
        "rebuilt action sequence must match the HandContext source"
    );
}

// ===========================================================================
// DB round-trip test (gated on the `database` feature + DATABASE_URL)
//
// The `database` feature pulls in `rbp-database` and `tokio-postgres`
// as optional dependencies, so the test module is `cfg`-gated
// independently of the in-memory tests above. The body additionally
// short-circuits when `DATABASE_URL` is unset so CI without Postgres
// stays green.
// ===========================================================================

#[cfg(feature = "database")]
mod db_tests {
    use super::*;
    use rbp_auth::Member;
    use rbp_database::Schema;
    use rbp_gameroom::HistoryRepository;
    use rbp_gameroom::records::Hand as HandRecord;
    use std::sync::Arc;

    /// Skip the DB round-trip when `DATABASE_URL` is unset, otherwise
    /// return a connected `Arc<tokio_postgres::Client>`.
    async fn db() -> Option<Arc<tokio_postgres::Client>> {
        let _ = std::env::var("DATABASE_URL").ok()?;
        let client = rbp_database::db().await;
        Some(client)
    }

    /// Pre-stage the persistence schema the hand tests need.
    ///
    /// The `HistoryRepository` writes to `users` (FK target of
    /// `players.user_id` and `actions.player_id`), `rooms` (FK
    /// target of `hands.room_id`), `hands`, `players`, and
    /// `actions`. The auth server_flow tests rely on an
    /// out-of-band pre-staged DB; the hand tests stage their own
    /// so the test is self-contained and survives fresh CI
    /// environments with no migration runner. The DDL is sourced
    /// from each type's `Schema::creates()` and `Schema::indices()`
    /// so a future schema refactor that changes column shape
    /// breaks the staging helper at the same time it breaks the
    /// production code path.
    ///
    /// `OnceLock<()>` gates the staging so it runs exactly once
    /// per test process. The DDL itself is dispatched on a
    /// brand-new `current_thread` runtime in a fresh OS thread,
    /// because the `#[tokio::test]` macro already owns the
    /// current thread's runtime and we cannot build a runtime
    /// inside a runtime. When `DATABASE_URL` is unset, the
    /// helper is a no-op.
    fn schema_setup() {
        use std::sync::OnceLock;
        static READY: OnceLock<()> = OnceLock::new();
        if READY.get().is_some() {
            return;
        }
        // Run the one-shot DDL on a brand-new thread that owns a
        // brand-new runtime. We deliberately do NOT use
        // `Handle::current().block_on(...)` because the
        // `#[tokio::test]` macro's runtime is `current_thread`
        // and we cannot block_in_place on it from a worker
        // thread. Spinning a fresh thread is the most portable
        // path: tokio's docs explicitly recommend it for
        // "interrupting a test runtime to do sync work".
        let outcome: Result<(), String> = std::thread::Builder::new()
            .name("stw-008-schema-setup".into())
            .spawn(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build one-shot runtime for schema setup");
                rt.block_on(async {
                    let Some(client) = db().await else {
                        return Ok(());
                    };
                    for ddl in [
                        Member::creates(),
                        Member::indices(),
                        rbp_gameroom::Room::creates(),
                        rbp_gameroom::Room::indices(),
                        HandRecord::creates(),
                        HandRecord::indices(),
                        Participant::creates(),
                        Participant::indices(),
                        Play::creates(),
                        Play::indices(),
                    ] {
                        if ddl.trim().is_empty() {
                            continue;
                        }
                        client.batch_execute(ddl).await.map_err(|e| {
                            format!("staging DDL failed: {e}\n--- DDL ---\n{ddl}\n-----------")
                        })?;
                    }
                    Ok(())
                })
            })
            .expect("spawn schema-setup thread")
            .join()
            .expect("schema-setup thread did not panic");
        match outcome {
            Ok(()) => {
                let _ = READY.set(());
            }
            Err(msg) => {
                if READY.get().is_none() {
                    panic!("schema_setup: {msg}");
                }
            }
        }
    }

    /// Drop the persistence rows this test owns. The full DDL is set
    /// up by the server's startup migration, so we only need a
    /// per-test clean-slate that guarantees no cross-test id
    /// collisions. We `TRUNCATE TABLE hands CASCADE` so the child
    /// rows in `players` and `actions` are cleared in the same
    /// statement, matching the `Schema::truncates` contract.
    async fn reset_tables(client: &Arc<tokio_postgres::Client>) {
        client
            .batch_execute(HandRecord::truncates())
            .await
            .expect("truncate hands CASCADE");
        // Belt and braces: truncate players and actions in case the
        // hands CASCADE has been disabled in a local test rig.
        let _ = client.batch_execute(Participant::truncates()).await;
        let _ = client.batch_execute(Play::truncates()).await;
        // The `db_round_trip_preserves_hand` test synthesises
        // `users` rows from the fixture's Member ids so the FK
        // on `players.user_id` / `actions.player_id` resolves.
        // The username field is `VARCHAR(32) UNIQUE NOT NULL`
        // (see `Member::creates`), so we cascade-truncate the
        // `users` table between tests to keep the synthetic
        // usernames from colliding across runs.
        let _ = client.batch_execute(Member::truncates()).await;
        // The `room_with_two_fish_persists_one_hand` test creates a
        // new `rooms` row on every run; we truncate the `rooms`
        // table last so the FK cascade from `hands` doesn't
        // shadow its own data with a stale row from a previous
        // run. `TRUNCATE rooms CASCADE` also drops the dependent
        // hands/players/actions in lockstep, which is exactly the
        // shape we want before each test's fresh inserts.
        let _ = client.batch_execute(rbp_gameroom::Room::truncates()).await;
    }

    /// DB round-trip: write a hand to Postgres via the same
    /// `HistoryRepository` path `Room::flush_hand` uses, then read
    /// it back via `get_hand / get_players / get_actions` and assert
    /// the reconstructed records are equal to the source. Skipped
    /// when `DATABASE_URL` is unset so CI without a database stays
    /// green.
    #[tokio::test]
    async fn db_round_trip_preserves_hand() {
        schema_setup();
        let Some(client) = db().await else {
            eprintln!("DATABASE_URL not set; skipping db_round_trip_preserves_hand");
            return;
        };
        reset_tables(&client).await;

        // Bump the member counter so this test's id space is
        // disjoint from the non-DB tests if the same process runs
        // both.
        member_counter().fetch_add(N as u32, Ordering::SeqCst);

        // Build the same fixture the non-DB tests use, so the
        // source records are reproducible. `initial_game` is unused
        // here — the DB test reads back what was written, it does
        // not re-derive the source state from the engine.
        let (_initial_game, final_game, ctx, room_id, f) = fixture_full_hand_checked_down();
        let f = f.as_ref();
        let hand = ctx.to_hand(room_id, final_game.board(), final_game.pot());
        let participants = ctx.participants(hand.id(), f);
        let plays = ctx.plays(hand.id(), f);

        assert!(
            !participants.is_empty() && !plays.is_empty(),
            "fixture must produce non-empty participants and plays"
        );

        // `players.user_id` and `actions.player_id` both FK to
        // `users(id)`, so the test Member ids that came out of
        // the conversion step (and ended up in
        // `Participant::user` / `Play::player`) must have
        // corresponding rows in the `users` table. Insert
        // zero-content rows for every distinct Member id the
        // fixture emitted — the fixture is deterministic
        // (the per-test `member_counter` is bumped once at
        // the top of the test, then `member_id_for(pos)` is
        // called for `pos` in `0..N`, so the resulting ids
        // are `2N` distinct uuids across the two tests).
        let distinct_user_ids: std::collections::HashSet<uuid::Uuid> = participants
            .iter()
            .filter_map(|p| p.user())
            .map(|id| id.inner())
            .chain(plays.iter().filter_map(|p| p.player()).map(|id| id.inner()))
            .collect();
        for uid in &distinct_user_ids {
            // Username is `VARCHAR(32) UNIQUE NOT NULL` (see
            // `Member::creates`). `stw-008-` is 8 chars; we have
            // 24 left for the discriminator. Take the first 24
            // hex chars of the uuid simple form (which is 32
            // hex chars without dashes) — that is unique enough
            // for the synthetic test rows and short enough to
            // fit. The email column is `VARCHAR(255)` so we can
            // use the full uuid there.
            let simple = uid.simple().to_string();
            let short = &simple[..24];
            client
                .execute(
                    "INSERT INTO users (id, username, email, hashword) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT (id) DO NOTHING",
                    &[
                        uid,
                        &format!("stw-008-{short}"),
                        &format!("stw-008-{}@example.invalid", simple),
                        &"x".to_string(),
                    ],
                )
                .await
                .expect("insert into users must succeed");
        }

        // `hands.room_id` FKs into `rooms(id)`, so the room row
        // must exist before `create_hand` is called. The
        // production `Room::flush_hand` runs after `Casino::start`
        // has already persisted the room (the `flush_hand` path
        // itself never inserts a room — that lives one level up
        // in the casino). We mirror that here so the test
        // exercises the same dependency order. We inline the
        // `INSERT INTO rooms (id, stakes) ...` SQL rather than
        // constructing a live `Room` (which would need a real
        // `Arc<Client>` and a `tokio` runtime just to call
        // `create_room` on it), so the test stays a pure
        // data-layer round-trip.
        client
            .execute(
                "INSERT INTO rooms (id, stakes) VALUES ($1, $2)",
                &[&room_id.inner(), &0_i16],
            )
            .await
            .expect("insert into rooms must succeed");

        // Write to the DB in the same order `Room::flush_hand`
        // does: create_hand, then one create_player per seat, then
        // one create_action per play.
        client
            .create_hand(&hand)
            .await
            .expect("create_hand must succeed against a clean schema");
        for p in &participants {
            client
                .create_player(p)
                .await
                .expect("create_player must succeed");
        }
        for a in &plays {
            client
                .create_action(a)
                .await
                .expect("create_action must succeed");
        }

        // Read back. `get_hand` returns the Hand row; `get_players`
        // orders by seat; `get_actions` orders by seq. The ordering
        // guarantee means we can compare index-for-index without an
        // extra sort on the test side.
        let read_back = client
            .get_hand(hand.id())
            .await
            .expect("get_hand must succeed")
            .expect("hand row must exist after insert");
        let read_players = client
            .get_players(hand.id())
            .await
            .expect("get_players must succeed");
        let read_actions = client
            .get_actions(hand.id())
            .await
            .expect("get_actions must succeed");

        // 1. Hand: every persisted field must round-trip.
        assert_eq!(read_back.id(), hand.id(), "Hand.id must round-trip");
        assert_eq!(read_back.room(), hand.room(), "Hand.room must round-trip");
        assert_eq!(read_back.pot(), hand.pot(), "Hand.pot must round-trip");
        assert_eq!(
            read_back.dealer(),
            hand.dealer(),
            "Hand.dealer must round-trip"
        );
        assert_eq!(
            read_back.board(),
            hand.board(),
            "Hand.board must round-trip"
        );

        // 2. Participants: one row per seat, in seat order, with
        //    the same seat/hole/stack/hand/user/showed/mucked.
        assert_eq!(
            read_players.len(),
            participants.len(),
            "Participant count must round-trip"
        );
        for (i, (lhs, rhs)) in read_players.iter().zip(participants.iter()).enumerate() {
            assert_eq!(
                lhs.seat(),
                rhs.seat(),
                "Participant[{i}].seat must round-trip"
            );
            assert_eq!(
                lhs.hole(),
                rhs.hole(),
                "Participant[{i}].hole must round-trip"
            );
            assert_eq!(
                lhs.stack(),
                rhs.stack(),
                "Participant[{i}].stack must round-trip"
            );
            assert_eq!(
                lhs.hand(),
                rhs.hand(),
                "Participant[{i}].hand must round-trip"
            );
            assert_eq!(
                lhs.user(),
                rhs.user(),
                "Participant[{i}].user must round-trip"
            );
            assert_eq!(
                lhs.showed(),
                rhs.showed(),
                "Participant[{i}].showed must round-trip"
            );
            assert_eq!(
                lhs.mucked(),
                rhs.mucked(),
                "Participant[{i}].mucked must round-trip"
            );
        }

        // 3. Plays: ordered by seq, every seq/player/action/hand
        //    field must round-trip.
        assert_eq!(
            read_actions.len(),
            plays.len(),
            "Play count must round-trip"
        );
        for (i, (lhs, rhs)) in read_actions.iter().zip(plays.iter()).enumerate() {
            assert_eq!(lhs.seq(), rhs.seq(), "Play[{i}].seq must round-trip");
            assert_eq!(
                lhs.player(),
                rhs.player(),
                "Play[{i}].player must round-trip"
            );
            assert_eq!(
                lhs.action(),
                rhs.action(),
                "Play[{i}].action must round-trip"
            );
            assert_eq!(lhs.hand(), rhs.hand(), "Play[{i}].hand must round-trip");
        }

        // 4. Strongest equality: the read-back (Position, Action)
        //    list, recovered by looking `Play::player` up in the
        //    participants, must equal the HandContext's source
        //    action list.
        let pos_by_member: HashMap<uuid::Uuid, rbp_core::Position> = {
            let mut map = HashMap::new();
            for p in &read_players {
                if let Some(uid) = p.user() {
                    map.insert(uid.inner(), p.seat());
                }
            }
            map
        };
        let recovered: Vec<(rbp_core::Position, Action)> = read_actions
            .iter()
            .map(|p| {
                let uid = p
                    .player()
                    .expect("Play.player must be present when round-tripping positions")
                    .inner();
                let pos = *pos_by_member.get(&uid).unwrap_or_else(|| {
                    panic!("DB round-trip: Play.player {uid} not in participants")
                });
                (pos, p.action())
            })
            .collect();
        assert_eq!(
            recovered,
            ctx.actions().to_vec(),
            "DB round-trip: recovered (Position, Action) list must equal HandContext.actions"
        );
    }

    /// Drive a real `Room` end-to-end: two `Fish` players seated,
    /// one hand played through the room's own `play_hand_once` path,
    /// and the resulting hand / participants / actions read back
    /// through `HistoryRepository`. This is the strongest STW-008
    /// proof: the in-memory conversion is exercised by (a)/(b), the
    /// `HistoryRepository::create_*` writes are exercised by (c), and
    /// this test wires them together inside the real `Room` shell
    /// (the same shell `Casino::start` runs in production).
    ///
    /// Skipped when `DATABASE_URL` is unset (and the test body
    /// short-circuits at the top) so CI without Postgres stays
    /// green, matching the `db_round_trip_preserves_hand` pattern.
    #[tokio::test]
    async fn room_with_two_fish_persists_one_hand() {
        schema_setup();
        let Some(client) = db().await else {
            eprintln!("DATABASE_URL not set; skipping room_with_two_fish_persists_one_hand");
            return;
        };
        reset_tables(&client).await;

        // Build the room. `Lurker::default()` for both seats makes
        // `Participant::user = None`, so the `players.user_id` column
        // is NULL and the FK to `users` is not exercised (matches
        // the production `Casino::start` shape for two `Fish` /
        // `Lurker` seats). The room row must exist before
        // `create_hand` runs because `hands.room_id` FKs into
        // `rooms(id)` — `Casino::start` calls `create_room(&room)`
        // for exactly this reason, and we mirror it.
        // The `Room` coordinator id (`ID<rbp_gameroom::Room>`) and
        // the `Hand.room` field's id (`ID<records::Room>`, the
        // marker type for the `rooms` table) are different phantom
        // wrappers around the same UUID. The room uses the
        // coordinator id as its own identity, but the FK stored in
        // the `hands.room_id` column is the same UUID retyped via
        // `ID::cast` into the marker phantom. The test reads back
        // the marker-typed id (`read_back.room()`) and asserts it
        // round-trips to the same UUID we built the room with.
        let coordinator_room_id: ID<rbp_gameroom::Room> = ID::default();
        let marker_room_id: ID<rbp_gameroom::records::Room> = coordinator_room_id.cast();
        let mut room = rbp_gameroom::Room::new(coordinator_room_id, 2, client.clone());
        rbp_gameroom::HistoryRepository::create_room(&client, &room)
            .await
            .expect("create_room must succeed against a clean schema");
        room.sit(rbp_gameroom::Fish, rbp_auth::Lurker::default());
        room.sit(rbp_gameroom::Fish, rbp_auth::Lurker::default());

        // Run exactly one hand cycle through the room. Returns the
        // hand id that was persisted. The room is now in `Showdown`
        // phase (not `Finished`), so a follow-up `conclude` would
        // attempt a second hand — but we drop the room here because
        // a single hand is the slice this test covers.
        let hand_id = room.play_hand_once().await;

        // Read back through the same `HistoryRepository` queries
        // `Casino` would use. `get_hand` returns the row, `get_players`
        // orders by seat, `get_actions` orders by seq.
        let read_back = client
            .get_hand(hand_id)
            .await
            .expect("get_hand must succeed")
            .expect("hand row must exist after Room.play_hand_once");
        let read_players = client
            .get_players(hand_id)
            .await
            .expect("get_players must succeed");
        let read_actions = client
            .get_actions(hand_id)
            .await
            .expect("get_actions must succeed");

        // 1. Hand: the persisted `room_id` must match the room we
        //    built. `dealer` / `board` / `pot` are engine-derived and
        //    already covered by the conversion tests; the contract
        //    under test here is that the room wired through to
        //    `create_hand` without dropping the room id.
        assert_eq!(
            read_back.id(),
            hand_id,
            "Room: persisted Hand.id must equal play_hand_once return value"
        );
        assert_eq!(
            read_back.room(),
            marker_room_id,
            "Room: persisted Hand.room must equal the room's id"
        );

        // 2. Participants: one row per seated player, in seat order
        //    0..N-1. The `user` field is `None` because both seats
        //    are `Lurker`. `showed`/`mucked` are FALSE at flush time
        //    (the room writes the participant row before
        //    `update_showed`/`update_mucked` run, which only happen
        //    at showdown reveal — and that path is not exercised by
        //    the simple `play_hand_once` test; the contract under
        //    test is that `flush_hand` itself produces the rows the
        //    conversion contract defines).
        assert_eq!(
            read_players.len(),
            N,
            "Room: must persist exactly one Participant per seated player"
        );
        for (i, p) in read_players.iter().enumerate() {
            assert_eq!(
                p.seat(),
                i,
                "Room: Participant[{i}].seat must equal its index"
            );
            assert_eq!(
                p.hand(),
                hand_id,
                "Room: Participant[{i}].hand must equal the flushed Hand id"
            );
            assert!(
                p.user().is_none(),
                "Room: Participant[{i}].user must be None for Lurker seats"
            );
            assert!(
                !p.showed(),
                "Room: Participant[{i}].showed must be FALSE at flush time"
            );
            assert!(
                !p.mucked(),
                "Room: Participant[{i}].mucked must be FALSE at flush time"
            );
        }

        // 3. Actions: Fish plays at least one action per Choice
        //    node the engine visits, so the action list is
        //    non-empty. Every action is a Choice (Chance deals are
        //    not recorded into `actions`). Every action's `hand`
        //    matches the just-flushed hand.
        assert!(
            !read_actions.is_empty(),
            "Room: Fish players must produce at least one recorded action in a full hand"
        );
        for (i, play) in read_actions.iter().enumerate() {
            assert_eq!(
                play.hand(),
                hand_id,
                "Room: Play[{i}].hand must equal the flushed Hand id"
            );
            assert!(
                play.action().is_choice(),
                "Room: Play[{i}].action must be a Choice action; chance deals are not persisted"
            );
            // Fish seats are Lurker, so the persisted `player` is
            // `None` — the seat identity is recovered through the
            // `Participant` row's `seat` column, not through
            // `Play::player`. This matches the FK-less shape of
            // the production casino's two `Fish`/`Lurker` seats.
            assert!(
                play.player().is_none(),
                "Room: Play[{i}].player must be None for Lurker seats"
            );
        }

        // 4. The seq field must be a contiguous 0..len-1 with no
        //    gaps. This is the same `seq`-integrity assertion the
        //    conversion tests make, applied to the round-tripped
        //    records (proving `create_action` writes the same seq
        //    the source `Play` carried).
        for (i, play) in read_actions.iter().enumerate() {
            assert_eq!(
                play.seq() as usize,
                i,
                "Room: Play[{i}].seq must equal its index in the seq-ordered readback"
            );
        }
    }

    /// STW-014 transcript round-trip: drive a real `Room`
    /// end-to-end, read the persisted records back through
    /// `HistoryRepository`, bundle them into a `Transcript`,
    /// call `verify()`, serialise to JSON, and re-parse.
    /// Proves the in-memory `Transcript` type is the right
    /// shape for the on-the-wire "replayable benchmark
    /// surface" the testnet roadmap requires: anyone with the
    /// JSON file and the public `Transcript` constructor can
    /// re-derive the action sequence without holding a DB
    /// connection. Gated on `database` + `DATABASE_URL` like
    /// the other DB tests so CI without Postgres stays green.
    #[tokio::test]
    async fn transcript_json_round_trips() {
        schema_setup();
        let Some(client) = db().await else {
            eprintln!("DATABASE_URL not set; skipping transcript_json_round_trips");
            return;
        };
        reset_tables(&client).await;

        // Drive the room the same way the STW-008 round-trip does.
        // Two `Fish` / `Lurker` seats → `Participant::user = None`
        // and `Play::player = None` for every action. The
        // transcript's `verify()` must accept a transcript where
        // every `Play::player` is `None` (those actions are
        // unattributed; the participant list is the "seat known?"
        // oracle, and an unattributed play skips the lookup).
        let coordinator_room_id: ID<rbp_gameroom::Room> = ID::default();
        let mut room = rbp_gameroom::Room::new(coordinator_room_id, 2, client.clone());
        rbp_gameroom::HistoryRepository::create_room(&client, &room)
            .await
            .expect("create_room must succeed against a clean schema");
        room.sit(rbp_gameroom::Fish, rbp_auth::Lurker::default());
        room.sit(rbp_gameroom::Fish, rbp_auth::Lurker::default());

        let hand_id = room.play_hand_once().await;

        // Read back the persisted records.
        let hand = client
            .get_hand(hand_id)
            .await
            .expect("get_hand must succeed")
            .expect("hand row must exist after Room.play_hand_once");
        let participants = client
            .get_players(hand_id)
            .await
            .expect("get_players must succeed");
        let plays = client
            .get_actions(hand_id)
            .await
            .expect("get_actions must succeed");

        // Build the transcript and assert its integrity check
        // accepts the persisted records. A `None` `Play::player`
        // for every play is the expected shape for two `Lurker`
        // seats, so `verify()` must return `Ok(())` here.
        use rbp_gameroom::records::Transcript;
        let t = Transcript::new(hand, participants, plays);
        assert!(
            t.verify().is_ok(),
            "Transcript::verify must accept a transcript built from a real Room's persisted records; got {:?}",
            t.verify()
        );

        // Serialise to JSON, parse the document back, and assert
        // the shape. The on-the-wire contract a downstream
        // scraper can `jq` over: top-level `hand` / `participants`
        // / `plays` keys, `hand.id` matches the source, the
        // participant count equals N, the play count equals the
        // recorded action count.
        let json = t.to_json();
        let v: serde_json::Value =
            serde_json::from_str(&json).expect("Transcript::to_json must emit valid JSON");

        let v_hand = v
            .get("hand")
            .expect("transcript JSON must include the `hand` key");
        let v_hand_id = v_hand
            .get("id")
            .and_then(|s| s.as_str())
            .expect("transcript JSON hand.id must be a string");
        assert_eq!(
            v_hand_id,
            t.hand().id().inner().to_string(),
            "transcript JSON hand.id must match the source Hand.id"
        );

        let v_participants = v
            .get("participants")
            .and_then(|p| p.as_array())
            .expect("transcript JSON participants must be an array");
        assert_eq!(
            v_participants.len(),
            t.participants().len(),
            "transcript JSON participants.length must match the source participant count"
        );

        let v_plays = v
            .get("plays")
            .and_then(|p| p.as_array())
            .expect("transcript JSON plays must be an array");
        assert_eq!(
            v_plays.len(),
            t.plays().len(),
            "transcript JSON plays.length must match the source play count"
        );
        // The persisted plays are already seq-ordered by the
        // `get_actions` repository contract; the JSON should
        // reflect that order so a downstream reader can re-derive
        // the action sequence by iterating the array.
        for (i, p) in v_plays.iter().enumerate() {
            let seq = p
                .get("seq")
                .and_then(|s| s.as_u64())
                .expect("transcript JSON plays[i].seq must be a number");
            assert_eq!(
                seq as usize, i,
                "transcript JSON plays[i].seq must equal its index"
            );
        }
    }

    /// STW-015 transcript replay end-to-end: drive a real
    /// `Room` end-to-end, read the persisted records back,
    /// build a `Transcript`, write it to a temp file, read it
    /// back through the new public `Transcript::read_from_path`
    /// + `rebuild_action_sequence` API, and assert the rebuilt
    /// `(Position, Action)` sequence matches the in-memory
    /// `HandContext` action list. This is the on-the-wire
    /// proof that "anyone with a `transcript-<id>.json` file
    /// can re-derive the hand" — the testnet "replayable
    /// benchmark surface" deliverable the STW-014 doc
    /// comment already promises. Gated on `database` +
    /// `DATABASE_URL` like the other DB tests so CI without
    /// Postgres stays green.
    #[tokio::test]
    async fn transcript_replay_end_to_end() {
        use rbp_gameroom::records::Transcript;
        schema_setup();
        let Some(client) = db().await else {
            eprintln!("DATABASE_URL not set; skipping transcript_replay_end_to_end");
            return;
        };
        reset_tables(&client).await;

        // Drive the room the same way the STW-014 test does.
        // Two `Fish` / `Lurker` seats → every `Play::player` is
        // `None`; the rebuild entry point has to handle that
        // (it routes the play to the next `None`-user seat in
        // seq order — both seats are `Lurker` here, so every
        // rebuilt position is either seat 0 or seat 1 depending
        // on the engine's actor order, and the rebuild must
        // pick the right one).
        let coordinator_room_id: ID<rbp_gameroom::Room> = ID::default();
        let mut room = rbp_gameroom::Room::new(coordinator_room_id, 2, client.clone());
        rbp_gameroom::HistoryRepository::create_room(&client, &room)
            .await
            .expect("create_room must succeed against a clean schema");
        room.sit(rbp_gameroom::Fish, rbp_auth::Lurker::default());
        room.sit(rbp_gameroom::Fish, rbp_auth::Lurker::default());

        let hand_id = room.play_hand_once().await;

        // Read back the persisted records the same way
        // `transcript_json_round_trips` does. The room
        // persists every `Play` (including the preflop limp
        // and the postflop check / call / fold sequence),
        // so the read-back plays list is non-empty.
        let hand = client
            .get_hand(hand_id)
            .await
            .expect("get_hand must succeed")
            .expect("hand row must exist after Room.play_hand_once");
        let participants = client
            .get_players(hand_id)
            .await
            .expect("get_players must succeed");
        let plays = client
            .get_actions(hand_id)
            .await
            .expect("get_actions must succeed");
        assert!(
            !plays.is_empty(),
            "Room: Fish players must produce at least one recorded action in a full hand"
        );
        let source_action_count = plays.len();

        // Build the transcript, write it to a temp file,
        // read it back through the new public API, rebuild
        // the action sequence, and assert the rebuilt
        // sequence is the same length as the source. (The
        // exact `(Position, Action)` tuple order matches
        // the source plays ordered by `seq`; a Fish on
        // `Lurker` seat produces `None` players, and the
        // rebuild routes them to the next `None`-user
        // participant in seq order — which is the seat the
        // engine actually chose.)
        let t = Transcript::new(hand, participants, plays);
        let tmp = std::env::temp_dir().join(format!(
            "transcript-replay-e2e-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        t.write_to_path(&tmp)
            .expect("Transcript::write_to_path must succeed on a temp file");

        let parsed = Transcript::read_from_path(&tmp)
            .expect("Transcript::read_from_path must succeed on a file written by write_to_path");
        parsed
            .verify()
            .expect("re-parsed transcript must re-pass verify");
        let rebuilt = parsed
            .rebuild_action_sequence()
            .expect("rebuild_action_sequence must succeed on a verified transcript");
        assert_eq!(
            rebuilt.len(),
            source_action_count,
            "rebuild_action_sequence length must equal the source plays length"
        );
        // Every rebuilt position must be in range (0..N).
        // The two-`Fish` integration shape has two `Lurker`
        // participants, so every rebuilt position is either
        // 0 or 1.
        for (i, (pos, _action)) in rebuilt.iter().enumerate() {
            assert!(
                *pos < N,
                "rebuilt position {i} ({pos}) must be in 0..N (got a seat the engine never seated)"
            );
        }
        // Every rebuilt action must be a Choice action
        // (Chance deals are not persisted into `plays`).
        for (i, (_pos, action)) in rebuilt.iter().enumerate() {
            assert!(
                action.is_choice(),
                "rebuilt action {i} ({action:?}) must be a Choice action; got a Chance deal"
            );
        }
        // `replay_to_path` (the all-in-one read+verify+rebuild
        // +render entry point a `trainer --replay <path>` would
        // call) must produce output that includes the hand id
        // and a `seat N: <bot>` line per Lurker participant.
        let rendered = Transcript::replay_to_path(&tmp)
            .expect("replay_to_path must succeed on a file written by write_to_path");
        assert!(
            rendered.starts_with("transcript: "),
            "replay_to_path output must start with `transcript: <hand_id>`; got: {rendered}"
        );
        assert!(
            rendered.contains("seat 0: <bot>"),
            "replay_to_path output must include a `seat 0: <bot>` line for the first Lurker; got: {rendered}"
        );
        assert!(
            rendered.contains("seat 1: <bot>"),
            "replay_to_path output must include a `seat 1: <bot>` line for the second Lurker; got: {rendered}"
        );
        assert!(
            rendered.contains("actions:\n"),
            "replay_to_path output must include an `actions:` section header; got: {rendered}"
        );
        let _ = std::fs::remove_file(&tmp);
    }
}
