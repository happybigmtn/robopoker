//! `trainer --replay <path>` — read a `transcript-*.json` file
//! the bench harness produced and re-derive the `(Position,
//! Action)` sequence + a renderable text summary, without a
//! database connection or a sister-crate invocation.
//!
//! This is the STW-016 "anyone with a transcript file can
//! re-derive the hand" surface the CEO roadmap's "Public
//! reproducible benchmark surface" proof point requires.
//! The bench writes one `transcript-<hand_id>.json` per
//! hand into `$RBP_BENCH_TRANSCRIPT_DIR` (default
//! `./transcripts`); this module is the consumer side.
//!
//! The whole module is a thin wrapper over the STW-015
//! public `rbp_gameroom::records::Transcript::replay_to_path`
//! surface. If the public surface changes, this module is a
//! one-line update.
//!
//! ## Why a new module and not a function on `bench`
//!
//! The new `--replay` mode does not need a `tokio_postgres`
//! connection — the whole point of the slice is "no DB
//! needed". Putting the handler in its own module keeps
//! the autotrain crate's `mod bench; ... pub mod replay;`
//! surface honest: a future reader of `mod.rs` sees
//! "bench is the producer side, replay is the consumer
//! side" without having to dig into the `bench.rs` file.
//!
//! ## Why the handler is sync, not async
//!
//! The whole pipeline is `read_file` + `serde_json` + an
//! in-memory `Vec` rebuild. There is no I/O latency worth
//! awaiting, and the upstream `Mode::run` is `async` only
//! because every other variant opens a DB. A sync `run`
//! here means a single `.await` call in the `Mode::Replay`
//! arm of the dispatch match (or no `.await` at all if
//! the dispatch is `tokio::task::spawn_blocking`-ed by
//! the caller).
use std::path::Path;

/// Replay a `transcript-<hand_id>.json` file to a `String`.
/// Thin wrapper over
/// [`rbp_gameroom::records::Transcript::replay_to_path`]
/// so the autotrain `Mode::Replay` arm has a single
/// `Result<String, String>` shape to print-and-exit on.
///
/// On success the returned `String` is the rendered
/// transcript:
///
/// ```text
/// transcript: <hand_id>
/// seat 0: <user-or-<bot>> <hole>
/// seat 1: <user-or-<bot>> <hole>
/// actions:
/// 0 P<pos> <Action>
/// 1 P<pos> <Action>
/// ...
/// ```
///
/// The caller (`Mode::run`) prints it to stdout and
/// exits 0. On error, the returned `String` is a
/// one-line diagnostic that starts with `read_from_path:`
/// (I/O failure), `from_json:` (malformed JSON),
/// or `play seq=` / `play at seq=` (transcript failed
/// `verify`); the caller prints it to stderr and
/// exits 2.
pub fn run(path: &Path) -> Result<String, String> {
    rbp_gameroom::records::Transcript::replay_to_path(path)
}

#[cfg(test)]
mod tests {
    //! Pure-in-memory lib tests for the STW-016 `replay`
    //! consumer. These tests do NOT require a live
    //! Postgres (the whole point of the slice is "no DB
    //! needed"); they build a `Transcript` in-memory,
    //! `write_to_path` it to a temp file, and round-trip
    //! it through `replay::run` to assert the on-the-wire
    //! render format a downstream tool sees. The
    //! `replay_run_errors_on_*` tests pin the error
    //! surface the `Mode::Replay` arm uses for the exit
    //! code.
    //!
    //! Fixture style mirrors the existing
    //! `crates/gameroom/src/records/transcript.rs::tests`
    //! helpers: a `sample_hand` with an empty preflop
    //! board, two `Participant` rows (seat 0 with a
    //! real `Member`, seat 1 bot-bound with `user =
    //! None`), and a short `Play` sequence (Call / Check
    //! / Check — the same deterministic preflop limp the
    //! STW-008 round-trip uses).
    use super::*;
    use rbp_auth::Member;
    use rbp_cards::Board;
    use rbp_cards::Card;
    use rbp_cards::Hand as CardsHand;
    use rbp_cards::Hole;
    use rbp_core::*;
    use rbp_gameplay::Action;
    use rbp_gameroom::records::Hand;
    use rbp_gameroom::records::Participant;
    use rbp_gameroom::records::Play;
    use rbp_gameroom::records::Transcript;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering;

    /// A unique counter so parallel test runs do not
    /// collide on the same temp dir. `tempfile::tempdir`
    /// would be cleaner, but the autotrain dev-deps do
    /// not include `tempfile`; a process-unique suffix
    /// is the documented alternative for `cargo test
    /// --test-threads=4` parallelism.
    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// The marker `Room` (the same type the `Hand::room`
    /// getter uses — distinct from the full
    /// `rbp_gameroom::Room` engine type). The
    /// `transcript.rs::tests` fixture uses
    /// `let room: ID<Room> = ID::from(uuid::Uuid::nil())`
    /// because it has `uuid` as a direct dep; the
    /// autotrain crate does not, so the marker is
    /// resolved through `rbp_gameroom::records::Room`
    /// (which the crate already re-exports through
    /// `rbp_gameroom`'s `pub mod records;`).
    fn sample_hand() -> Hand {
        let room: ID<rbp_gameroom::records::Room> = ID::default();
        let board: Board = Board::from(CardsHand::empty());
        Hand::new(ID::default(), room, board, 6, 0)
    }

    fn sample_participants(hand_id: ID<Hand>) -> Vec<Participant> {
        let m: ID<Member> = ID::default();
        // A real 2-card hole (As Kd) so `Hole::from` is
        // happy (the constructor debug-asserts the
        // underlying hand is exactly two cards).
        let ace_spades: Card = Card::try_from("As").unwrap();
        let king_diamonds: Card = Card::try_from("Kd").unwrap();
        let two_card = CardsHand::add(
            CardsHand::from(u64::from(ace_spades)),
            CardsHand::from(u64::from(king_diamonds)),
        );
        let hole = Hole::from(two_card);
        vec![
            Participant::new(hand_id, Some(m), 0, hole, 100),
            Participant::new(hand_id, None, 1, hole, 100),
        ]
    }

    fn sample_plays(hand_id: ID<Hand>, member: Option<ID<Member>>) -> Vec<Play> {
        vec![
            Play::new(hand_id, 0, member, Action::Call(0)),
            Play::new(hand_id, 1, member, Action::Check),
            Play::new(hand_id, 2, member, Action::Check),
        ]
    }

    /// Happy path: a fixture transcript written to a temp
    /// file is re-rendered by `replay::run` with the
    /// expected on-the-wire shape (`transcript: <id>`, a
    /// `seat N: ...` line per participant, an
    /// `actions:` section, and a per-action
    /// `seq P<pos> <Action>` line). The test does NOT
    /// depend on `database` — the `Transcript` is built
    /// in memory and `write_to_path` is pure I/O. A
    /// regression that breaks the render format (e.g. a
    /// `replay_to_path` rename) fails this test before
    /// the `Mode::Replay` arm ships.
    #[test]
    fn replay_run_renders_fixture_transcript() {
        let hand = sample_hand();
        let hand_id = hand.id();
        let participants = sample_participants(hand_id);
        let member = participants[0].user();
        let plays = sample_plays(hand_id, member);
        let t = Transcript::new(hand.clone(), participants, plays);
        // Write the fixture to a temp file.
        let dir = std::env::temp_dir().join(format!(
            "rbp-autotrain-replay-test-{}",
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join(format!("transcript-{}.json", hand.id().inner()));
        t.write_to_path(&path).expect("write_to_path must succeed");
        // Replay.
        let rendered = run(&path).expect("replay::run must succeed against a fixture file");
        // Format checks: the header, the seat lines, the
        // actions section header, and the per-action
        // lines all appear in the rendered text.
        assert!(
            rendered.starts_with(&format!("transcript: {}\n", hand.id().inner())),
            "rendered output must start with `transcript: <hand_id>`; got: {rendered}"
        );
        assert!(
            rendered.contains("seat 0: "),
            "rendered output must include a seat-0 line; got: {rendered}"
        );
        assert!(
            rendered.contains("seat 1: "),
            "rendered output must include a seat-1 line; got: {rendered}"
        );
        assert!(
            rendered.contains("actions:\n"),
            "rendered output must include the `actions:` section header; got: {rendered}"
        );
        // One line per play. The sample is 3 plays, so
        // there must be exactly 3 `P<pos>` lines.
        let action_line_count = rendered
            .lines()
            .filter(|l| l.contains("P0 ") || l.contains("P1 "))
            .count();
        assert_eq!(
            action_line_count, 3,
            "rendered output must have exactly 3 per-action lines; got {action_line_count}: {rendered}"
        );
        // Clean up the temp dir so a re-run does not see
        // stale files. The dir is process-unique so a
        // cleanup failure does not break other tests.
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Missing-file error: a non-existent path returns
    /// `Err` whose message starts with `read_from_path:`
    /// (the same prefix the STW-015 public surface
    /// documents). A regression that swaps the prefix
    /// (or swallows the error and returns `Ok`) fails
    /// the `Mode::Replay` arm's exit-code mapping.
    #[test]
    fn replay_run_errors_on_missing_file() {
        let dir = std::env::temp_dir().join(format!(
            "rbp-autotrain-replay-test-missing-{}",
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        // Do NOT create the dir — the path is guaranteed
        // to not exist.
        let path = dir.join("transcript-does-not-exist.json");
        let result = run(&path);
        match result {
            Err(e) => assert!(
                e.starts_with("read_from_path:"),
                "missing-file error must start with `read_from_path:`; got: {e}"
            ),
            Ok(s) => panic!("replay::run must error on a missing file; got Ok({s:?})"),
        }
    }

    /// Corrupt-file error: a file that exists but is not
    /// valid JSON returns `Err` whose message contains
    /// `not valid JSON` (the prefix the STW-015
    /// `from_json` surface uses for a JSON parse
    /// failure). A regression that accepts a non-JSON
    /// file (or reports a different error) breaks the
    /// CI surface the new mode is meant to plug into.
    #[test]
    fn replay_run_errors_on_corrupt_file() {
        let dir = std::env::temp_dir().join(format!(
            "rbp-autotrain-replay-test-corrupt-{}",
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("transcript-corrupt.json");
        std::fs::write(&path, b"not-json").expect("write corrupt fixture");
        let result = run(&path);
        match result {
            Err(e) => assert!(
                e.contains("not valid JSON"),
                "corrupt-file error must contain `not valid JSON`; got: {e}"
            ),
            Ok(s) => panic!("replay::run must error on a non-JSON file; got Ok({s:?})"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
