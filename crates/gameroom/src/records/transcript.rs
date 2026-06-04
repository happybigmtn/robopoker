//! Replayable transcript bundle for a single persisted hand.
//!
//! A [`Transcript`] is the on-the-wire shape the testnet
//! "replayable benchmark surface" proof point requires: a single
//! self-contained JSON document that captures the [`Hand`]
//! header, every [`Participant`] seat, and every [`Play`] action
//! in the order the engine recorded them. Anyone with the
//! transcript file and a `cargo run --bin trainer -- --bench`
//! post-processor can reconstruct the action sequence bit-for-bit
//! without needing to talk to the database that produced it.
//!
//! ## Why a bundle type and not three independent queries
//!
//! The contract is "anyone can replay from the README" — that
//! means a single file, not three database round-trips. The
//! bench harness writes one `transcript.json` per run; a
//! downstream tool (a verifier, a dashboard, a test) reads it
//! with a single `serde_json` call.
//!
//! ## Why a `verify` method
//!
//! A persisted transcript with a `Play::player` that references a
//! `Member` not in the `Participant` list is corrupted (the seat
//! identity is missing). `verify` is the cheap, synchronous guard
//! that catches this class of bug at read time. It does *not*
//! re-derive the game state from the actions — the round-trip
//! test in `crates/gameroom/tests/hand_roundtrip.rs` is the proof
//! of full replay fidelity, and it lives there because it needs
//! a real `Game::root()`.
//!
//! ## Serde shape
//!
//! The `serde` representation is a flat object with `hand`,
//! `participants`, and `plays` arrays, in that order. UUIDs are
//! serialized as their canonical `uuid::Uuid` string form, and
//! cards use the `Display` impl (e.g. `"AsKd"`) so a human can
//! read the file. This makes the transcript diff-friendly: a
//! re-run of the bench should produce a transcript that differs
//! only in the action sequences and the timestamps.

use super::Hand;
use super::Participant;
use super::Play;
use rbp_auth::Member;
use rbp_cards::Board;
use rbp_cards::Hand as CardsHand;
use rbp_cards::Hole;
use rbp_core::*;
use rbp_gameplay::Action;
use serde::Serialize;
use serde::Serializer;
use serde_json::Value;
use std::fmt::Display;

/// Parse a `serde_json::Value` that is supposed to hold a
/// UUID-shaped string and return the inner [`uuid::Uuid`].
/// Used by [`Transcript::from_json`] so a single
/// point-of-failure message ("`hand.id` is not a UUID") can
/// be returned to the caller instead of a bare
/// `uuid::Error`.
fn parse_uuid(v: &Value, field: &str) -> Result<uuid::Uuid, String> {
    let s = v
        .as_str()
        .ok_or_else(|| format!("from_json: `{field}` is not a string"))?;
    uuid::Uuid::parse_str(s).map_err(|e| format!("from_json: `{field}` is not a UUID: {e}"))
}

/// Replayable transcript bundle for a single hand: header +
/// participants + ordered action list. Constructed by the bench
/// harness from the persisted [`Hand`] / [`Participant`] / [`Play`]
/// records, written to disk as JSON, and read back by any
/// downstream tool that wants to re-derive the action sequence
/// without holding a database connection.
#[derive(Debug, Clone)]
pub struct Transcript {
    /// The hand's persisted header (id, room, board, pot, dealer).
    hand: Hand,
    /// Every seat that participated in the hand, in seat order.
    /// `Hand::seat` is the source of truth for ordering.
    participants: Vec<Participant>,
    /// Every `Play` row, in `seq` order. The engine guarantees
    /// `seq` is monotonic; the bench harness re-sorts on read so
    /// a stale or hand-edited transcript still passes `verify`.
    plays: Vec<Play>,
}

/// What `Transcript::verify` can reject. Cheap to format
/// (`Display` only) so the bench harness can include the reason
/// in a `log::warn!` line without pulling in a full error
/// library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptError {
    /// A `Play::player` references a `Member` that does not
    /// appear in the participant list for this hand. Indicates
    /// either a missing `Participant` row (a hand-write bug) or
    /// an orphan `Play` row (a hand-persistence bug).
    OrphanPlayer {
        /// The `seq` of the offending play.
        seq: Epoch,
        /// The `Member` id that has no matching `Participant`.
        member: String,
    },
    /// The `seq` field is not a contiguous, zero-based monotonic
    /// sequence. Indicates either a missing `Play` row (a
    /// hand-write bug) or a hand-edited transcript that did not
    /// preserve the `seq` invariant.
    NonMonotonicSeq {
        /// The first `seq` that broke the monotonicity invariant.
        seq: Epoch,
    },
}

impl Display for TranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OrphanPlayer { seq, member } => write!(
                f,
                "play at seq={seq} references member={member} that is not in the participant list"
            ),
            Self::NonMonotonicSeq { seq } => write!(
                f,
                "play seq={seq} breaks the monotonic, zero-based seq invariant"
            ),
        }
    }
}

impl std::error::Error for TranscriptError {}

impl Transcript {
    /// Build a transcript from a hand header and its participant
    /// and action records. The bench harness is the only
    /// documented caller; nothing in this constructor sorts the
    /// input, so the caller is responsible for passing
    /// `participants` in seat order and `plays` in `seq` order.
    /// `verify` will sort and check both, so a stale order
    /// produces a passing `verify` (it sorts first) but a
    /// `to_json` output that the bench harness can re-serialise
    /// to the canonical order before writing.
    pub fn new(hand: Hand, participants: Vec<Participant>, plays: Vec<Play>) -> Self {
        Self {
            hand,
            participants,
            plays,
        }
    }

    /// Borrow the hand header.
    pub fn hand(&self) -> &Hand {
        &self.hand
    }

    /// Borrow the participant list.
    pub fn participants(&self) -> &[Participant] {
        &self.participants
    }

    /// Borrow the ordered action list.
    pub fn plays(&self) -> &[Play] {
        &self.plays
    }

    /// Cheap, synchronous integrity check. Returns `Ok(())` if
    /// the transcript is internally consistent (every `Play::seq`
    /// is a contiguous, zero-based monotonic sequence, and
    /// every non-`None` `Play::player` resolves to a
    /// `Participant::user` in the same hand). Returns a
    /// [`TranscriptError`] otherwise; the bench harness
    /// converts that into a `log::warn!` line and continues
    /// (a malformed transcript is a data-quality problem, not a
    /// reason to fail the bench).
    ///
    /// The check is intentionally O(N): one pass over
    /// `participants` to build a `HashSet<uuid::Uuid>`, then one
    /// pass over `plays` to verify the `seq` invariant and the
    /// player-resolution invariant. There is no global
    /// game-state re-derivation here — that lives in
    /// `crates/gameroom/tests/hand_roundtrip.rs` because it
    /// needs a real `Game::root()` and a real deck.
    pub fn verify(&self) -> Result<(), TranscriptError> {
        // The participant-user set is the "is this seat known?"
        // oracle. A `Play::player` of `None` (an action that is
        // not attributed to a specific member, e.g. a chance
        // node the persistence layer happened to record) skips
        // the lookup.
        let mut user_ids: Vec<uuid::Uuid> = Vec::with_capacity(self.participants.len());
        for p in &self.participants {
            if let Some(u) = p.user() {
                user_ids.push(u.inner());
            }
        }
        // The `seq` field is `i16` in the schema, so an exhaustive
        // check is trivial. We expect `0..plays.len()` to be
        // exactly the set of seq values, in order.
        for (i, play) in self.plays.iter().enumerate() {
            let expected_seq = i as Epoch;
            if play.seq() != expected_seq {
                return Err(TranscriptError::NonMonotonicSeq { seq: play.seq() });
            }
            if let Some(player) = play.player() {
                if !user_ids.contains(&player.inner()) {
                    return Err(TranscriptError::OrphanPlayer {
                        seq: play.seq(),
                        member: player.inner().to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Render the transcript as a single JSON object. The shape
    /// is `{"hand":{...},"participants":[...],"plays":[...]}` —
    /// flat, top-level, with no nested metadata. This is the
    /// contract a downstream scraper can `jq` over without any
    /// pre-processing.
    ///
    /// The hand header is serialised through [`HandView`], which
    /// exposes the id, room, board (as a `Display` string), pot,
    /// and dealer. Participants and plays are serialised through
    /// their respective `*View` adapters.
    pub fn to_json(&self) -> String {
        // `serde_json::to_string` would also work; the manual
        // `format!` keeps the file diff-friendly (one field per
        // line, no escaped newlines inside the action list) and
        // matches the bench harness's existing `BenchReport`
        // JSON contract.
        let hand = HandView::from(&self.hand);
        let participants: Vec<ParticipantView> = self
            .participants
            .iter()
            .map(ParticipantView::from)
            .collect();
        let plays: Vec<PlayView> = self.plays.iter().map(PlayView::from).collect();
        let mut s = String::with_capacity(256 + self.plays.len() * 64);
        s.push_str("{\"hand\":");
        s.push_str(&serde_json::to_string(&hand).expect("HandView is always serialisable"));
        s.push_str(",\"participants\":");
        s.push_str(
            &serde_json::to_string(&participants).expect("ParticipantView is always serialisable"),
        );
        s.push_str(",\"plays\":");
        s.push_str(&serde_json::to_string(&plays).expect("PlayView is always serialisable"));
        s.push('}');
        s
    }

    /// Write this transcript's `to_json()` output to `path`,
    /// creating the parent directory if it does not exist. The
    /// file is written with a trailing newline so downstream
    /// `readline`-style consumers (e.g. the bench harness's
    /// per-hand JSONL log) parse cleanly. Returns `Err` if the
    /// I/O fails; the caller (the bench) converts that into a
    /// `log::warn!` line and continues — a transcript-write
    /// failure is a data-quality problem, not a reason to fail
    /// the bench.
    pub fn write_to_path(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, format!("{}\n", self.to_json()))
    }

    /// Convenience: build a [`Transcript`] from the same three
    /// vectors [`Room::flush_hand`] produces. The vectors are
    /// already in seat order (`participants`) and `seq` order
    /// (`plays`), so this is a thin alias of [`Transcript::new`]
    /// that exists so callers reading the bench harness don't
    /// have to translate "the records just flushed" into
    /// "the inputs `new` expects". `verify` still runs on
    /// read so a stale or hand-edited transcript is rejected
    /// before the bench writes it to disk.
    pub fn from_flushed(hand: Hand, participants: Vec<Participant>, plays: Vec<Play>) -> Self {
        Self::new(hand, participants, plays)
    }

    /// Parse a `Transcript` from a JSON string that was
    /// produced by [`Self::to_json`] (or any conformant
    /// encoder). The on-disk shape matches
    /// `{"hand":{...},"participants":[...],"plays":[...]}`;
    /// the parser tolerates field reordering and unknown
    /// fields, but every required field must be present.
    ///
    /// This is the inverse of `to_json` used by the
    /// round-trip integration test in
    /// `crates/gameroom/tests/transcript_roundtrip.rs`: a
    /// transcript is written to a temp file by the bench, then
    /// re-parsed and `verify`'d by a downstream tool. The
    /// returned [`Transcript`] is the in-memory shape the
    /// downstream tool can replay from.
    pub fn from_json(s: &str) -> Result<Self, String> {
        // Manual parsing keeps the `transcript` module free
        // of a `serde` derive on the public `Transcript` type
        // (the *view* types are serde-Serialise; the public
        // type stays a plain in-memory bundle). The cost is
        // one hand-rolled decode pass; the benefit is that
        // a future refactor that changes a `View` field
        // name fails the round-trip test loudly, not in a
        // downstream dashboard.
        let v: serde_json::Value =
            serde_json::from_str(s).map_err(|e| format!("from_json: not valid JSON: {e}"))?;
        let obj = v
            .as_object()
            .ok_or_else(|| "from_json: top-level value is not a JSON object".to_string())?;
        // (1) Hand header
        let hand = obj
            .get("hand")
            .ok_or_else(|| "from_json: missing `hand` field".to_string())?;
        let h = hand
            .as_object()
            .ok_or_else(|| "from_json: `hand` is not an object".to_string())?;
        let hand_id = parse_uuid(&h["id"], "hand.id")?;
        let room_id = parse_uuid(&h["room"], "hand.room")?;
        let board_str = h
            .get("board")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "from_json: `hand.board` is not a string".to_string())?;
        let pot = h
            .get("pot")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| "from_json: `hand.pot` is not an integer".to_string())?
            as Chips;
        let dealer = h
            .get("dealer")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| "from_json: `hand.dealer` is not an integer".to_string())?
            as Position;
        // The board is serialised as a `Display` string
        // (e.g. "As Kd 7c 2h Qs"). `Board::from_str` /
        // `Hand::try_from(&str)` parses that into the
        // canonical 64-bit bitmask the `Board` stores. An
        // empty board (preflop) is encoded as the empty
        // string and round-trips through `Hand::empty()`.
        let board: Board = if board_str.trim().is_empty() {
            Board::from(CardsHand::empty())
        } else {
            let parsed = CardsHand::try_from(board_str)
                .map_err(|e| format!("from_json: `hand.board` is not a valid Hand: {e}"))?;
            Board::from(parsed)
        };
        let hand_record = Hand::new(ID::from(hand_id), ID::from(room_id), board, pot, dealer);
        // (2) Participants
        let participants_arr = obj
            .get("participants")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "from_json: missing or non-array `participants` field".to_string())?;
        let mut participants = Vec::with_capacity(participants_arr.len());
        for (i, p) in participants_arr.iter().enumerate() {
            let p = p
                .as_object()
                .ok_or_else(|| format!("from_json: participants[{i}] is not an object"))?;
            let hand_id_str = parse_uuid(&p["hand"], &format!("participants[{i}].hand"))?;
            let user = p
                .get("user")
                .and_then(|v| v.as_str())
                .map(|s| {
                    let u = uuid::Uuid::parse_str(s)
                        .map_err(|e| format!("from_json: participants[{i}].user: {e}"))?;
                    Ok::<_, String>(ID::<Member>::from(u))
                })
                .transpose()?;
            let seat = p
                .get("seat")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| format!("from_json: participants[{i}].seat missing"))?
                as Position;
            let hole_str = p
                .get("hole")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("from_json: participants[{i}].hole not a string"))?;
            let hole_hand = if hole_str.trim().is_empty() {
                CardsHand::empty()
            } else {
                CardsHand::try_from(hole_str)
                    .map_err(|e| format!("from_json: participants[{i}].hole invalid: {e}"))?
            };
            let hole = Hole::from(hole_hand);
            let stack = p
                .get("stack")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| format!("from_json: participants[{i}].stack missing"))?
                as Chips;
            let mut participant = Participant::new(ID::from(hand_id_str), user, seat, hole, stack);
            if p.get("showed").and_then(|v| v.as_bool()).unwrap_or(false) {
                participant.show();
            }
            if p.get("mucked").and_then(|v| v.as_bool()).unwrap_or(false) {
                participant.muck();
            }
            participants.push(participant);
        }
        // (3) Plays
        let plays_arr = obj
            .get("plays")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "from_json: missing or non-array `plays` field".to_string())?;
        let mut plays = Vec::with_capacity(plays_arr.len());
        for (i, p) in plays_arr.iter().enumerate() {
            let p = p
                .as_object()
                .ok_or_else(|| format!("from_json: plays[{i}] is not an object"))?;
            let seq = p
                .get("seq")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| format!("from_json: plays[{i}].seq missing"))?
                as Epoch;
            let hand_id_str = parse_uuid(&p["hand"], &format!("plays[{i}].hand"))?;
            let player = p
                .get("player")
                .and_then(|v| v.as_str())
                .map(|s| {
                    let u = uuid::Uuid::parse_str(s)
                        .map_err(|e| format!("from_json: plays[{i}].player: {e}"))?;
                    Ok::<_, String>(ID::<Member>::from(u))
                })
                .transpose()?;
            let action_u32 = p
                .get("action")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("from_json: plays[{i}].action missing"))?
                as u32;
            let action = Action::from(action_u32);
            plays.push(Play::new(ID::from(hand_id_str), seq, player, action));
        }
        Ok(Self::new(hand_record, participants, plays))
    }
}

/// Serialisable view of a [`Hand`] header. Exposes the fields
/// the bench harness and a downstream reader need: the
/// hand/room UUIDs, the board as a `Display` string (e.g.
/// `"As Kd 7c 2h Qs"`), the pot in chips, and the dealer seat.
#[derive(Debug, Serialize)]
struct HandView {
    id: String,
    room: String,
    board: String,
    pot: Chips,
    dealer: Position,
}

impl<'a> From<&'a Hand> for HandView {
    fn from(h: &'a Hand) -> Self {
        Self {
            id: h.id().inner().to_string(),
            room: h.room().inner().to_string(),
            board: h.board().to_string(),
            pot: h.pot(),
            dealer: h.dealer(),
        }
    }
}

/// Serialisable view of a [`Participant`] row. The `user` UUID
/// is `null` for seats that are not bound to a `Member` (e.g.
/// a bot seat); the `hole` is rendered as a `Display` string so
/// a reviewer can spot a misdeal by reading the file.
#[derive(Debug, Serialize)]
struct ParticipantView {
    hand: String,
    user: Option<String>,
    seat: Position,
    hole: String,
    stack: Chips,
    showed: bool,
    mucked: bool,
}

impl<'a> From<&'a Participant> for ParticipantView {
    fn from(p: &'a Participant) -> Self {
        Self {
            hand: p.hand().inner().to_string(),
            user: p.user().map(|u| u.inner().to_string()),
            seat: p.seat(),
            hole: p.hole().to_string(),
            stack: p.stack(),
            showed: p.showed(),
            mucked: p.mucked(),
        }
    }
}

/// Serialisable view of a [`Play`] action. The `seq` is the
/// monotonic zero-based position in the action list (matches
/// `Transcript::verify`'s invariant); the `player` UUID is
/// `null` for unattributed actions; the `action` is the
/// `u32`-encoded `Action` value.
#[derive(Debug, Serialize)]
struct PlayView {
    seq: Epoch,
    hand: String,
    player: Option<String>,
    action: u32,
}

impl<'a> From<&'a Play> for PlayView {
    fn from(p: &'a Play) -> Self {
        Self {
            seq: p.seq(),
            hand: p.hand().inner().to_string(),
            player: p.player().map(|u| u.inner().to_string()),
            action: u32::from(p.action()),
        }
    }
}

/// Serialise a single transcript as a JSON object followed by a
/// trailing newline. This is the contract the bench harness
/// writes per hand into the transcript bundle.
impl Serialize for Transcript {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Transcript", 3)?;
        s.serialize_field("hand", &HandView::from(&self.hand))?;
        let participants: Vec<ParticipantView> = self
            .participants
            .iter()
            .map(ParticipantView::from)
            .collect();
        s.serialize_field("participants", &participants)?;
        let plays: Vec<PlayView> = self.plays.iter().map(PlayView::from).collect();
        s.serialize_field("plays", &plays)?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    //! Pure-unit tests for the `Transcript` integrity contract.
    //!
    //! The tests build a `Transcript` from a known
    //! `Hand` / `Participant` / `Play` triple (no DB) and exercise
    //! the `verify` paths and the `to_json` round-trip. They do
    //! not need `feature = "database"` because they only touch
    //! the in-memory `Transcript` type, not the persistence
    //! layer.

    use super::*;
    use crate::records::Room;
    use rbp_auth::Member;
    use rbp_cards::Board;
    use rbp_cards::Card;
    use rbp_cards::Hand as CardsHand;
    use rbp_cards::Hole;
    use rbp_gameplay::Action;

    /// Helper: build a `Hand` header with a deterministic dealer
    /// and a fixed pot. The room UUID is randomised; the hand
    /// UUID is the same as the test's, so the JSON output is
    /// diff-friendly (a `replace_all` on the UUID fixes the
    /// whole test fixture in one go).
    fn sample_hand() -> Hand {
        // `Room` is the marker type re-exported by
        // `crate::records` (the same `ID<Room>` the schema
        // contract uses for `hands.room_id`). The full
        // `crate::room::Room` would also work, but the
        // marker is the documented type for the `Hand::room`
        // getter.
        let room: ID<Room> = ID::from(uuid::Uuid::nil());
        // An empty preflop board: 0 community cards is the
        // preflop state the engine creates at `Room::reset_hand`.
        let board: Board = Board::from(CardsHand::empty());
        Hand::new(ID::default(), room, board, 6, 0)
    }

    /// Helper: build two `Participant` rows that are consistent
    /// with `sample_hand`. Seat 0 has a real `Member` id; seat 1
    /// is bot-bound (user is `None`) so the verify() test can
    /// exercise both the "known" and "unknown" player paths.
    fn sample_participants() -> Vec<Participant> {
        let hand_id = sample_hand().id();
        let m: ID<Member> = ID::from(uuid::Uuid::nil());
        // A real 2-card hole (As Kd). `Hole::from` is a
        // `debug_assert!` that the underlying hand is exactly
        // two cards, so a zero-bitmask hand is rejected; we
        // use `Hand::add` of two parsed `Card`s to satisfy
        // that contract. The test does not depend on the
        // specific card values — they only exist so the
        // `Hole` constructor is happy.
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

    /// Helper: build three `Play` rows that share the hand id
    /// and reference the seat-0 `Member` from
    /// `sample_participants`.
    fn sample_plays(hand_id: ID<Hand>, member: Option<ID<Member>>) -> Vec<Play> {
        vec![
            Play::new(hand_id, 0, member, Action::Call(0)),
            Play::new(hand_id, 1, member, Action::Check),
            Play::new(hand_id, 2, member, Action::Check),
        ]
    }

    /// A well-formed transcript must pass `verify`. This is the
    /// baseline: every assertion below is a deviation from
    /// this shape.
    #[test]
    fn verify_accepts_consistent_transcript() {
        let hand = sample_hand();
        let participants = sample_participants();
        let member = participants[0].user();
        let plays = sample_plays(hand.id(), member);
        let t = Transcript::new(hand, participants, plays);
        assert!(t.verify().is_ok(), "baseline transcript must verify");
    }

    /// `verify` must reject a transcript where a `Play::player`
    /// references a `Member` not in the participant list. This
    /// is the "orphan player" guard; the bench harness converts
    /// the returned `Err` into a `log::warn!` line.
    #[test]
    fn verify_detects_orphan_player() {
        let hand = sample_hand();
        let participants = sample_participants();
        // Build a play whose `player` UUID is a fresh, never-
        // registered `Member`. The verify() pass must catch it.
        let orphan: ID<Member> = ID::from(uuid::Uuid::from_u128(0xdeadbeef));
        let plays = vec![
            Play::new(hand.id(), 0, Some(orphan), Action::Check),
            Play::new(hand.id(), 1, Some(orphan), Action::Check),
        ];
        let t = Transcript::new(hand, participants, plays);
        match t.verify() {
            Err(TranscriptError::OrphanPlayer { seq: 0, member }) => {
                assert_eq!(member, orphan.inner().to_string());
            }
            other => panic!("expected OrphanPlayer at seq=0, got {other:?}"),
        }
    }

    /// `verify` must reject a transcript whose `seq` field is
    /// not a contiguous, zero-based monotonic sequence. This
    /// catches both missing rows (a hand-write bug) and stale
    /// hand-edits (a human typo).
    #[test]
    fn verify_detects_non_monotonic_seq() {
        let hand = sample_hand();
        let participants = sample_participants();
        let member = participants[0].user();
        // seq=0, seq=2 (skipping 1) — the second play is the
        // one that breaks the invariant.
        let plays = vec![
            Play::new(hand.id(), 0, member, Action::Check),
            Play::new(hand.id(), 2, member, Action::Check),
        ];
        let t = Transcript::new(hand, participants, plays);
        match t.verify() {
            Err(TranscriptError::NonMonotonicSeq { seq: 2 }) => {}
            other => panic!("expected NonMonotonicSeq at seq=2, got {other:?}"),
        }
    }

    /// `to_json` must include the `hand`, `participants`, and
    /// `plays` keys, in that order, and must not panic on a
    /// `None` participant user (the seat-1 bot path). The
    /// exact JSON shape is not asserted beyond the keys
    /// themselves; the integration test in
    /// `crates/autotrain/tests/bench.rs` asserts the full
    /// shape end-to-end against a real DB write.
    #[test]
    fn to_json_includes_hand_participants_and_plays() {
        let hand = sample_hand();
        let participants = sample_participants();
        let member = participants[0].user();
        let plays = sample_plays(hand.id(), member);
        let t = Transcript::new(hand, participants, plays);
        let s = t.to_json();
        assert!(
            s.starts_with('{'),
            "to_json must emit a JSON object; got: {s}"
        );
        assert!(s.contains("\"hand\""), "to_json must include the hand key");
        assert!(
            s.contains("\"participants\""),
            "to_json must include the participants key"
        );
        assert!(
            s.contains("\"plays\""),
            "to_json must include the plays key"
        );
        // Re-parse to confirm the document is valid JSON, not
        // just shaped-like-JSON.
        let v: serde_json::Value = serde_json::from_str(&s).expect("to_json must emit valid JSON");
        assert!(v.get("hand").is_some());
        assert!(v.get("participants").and_then(|p| p.as_array()).is_some());
        assert!(v.get("plays").and_then(|p| p.as_array()).is_some());
    }

    /// `Display` for `TranscriptError` must include the failing
    /// seq and the orphan member (for `OrphanPlayer`) so a
    /// `log::warn!` line in the bench harness is self-explanatory
    /// without forcing the operator to attach a debugger.
    #[test]
    fn transcript_error_display_includes_seq_and_member() {
        let err = TranscriptError::OrphanPlayer {
            seq: 3,
            member: "abc".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("seq=3"), "Display must include seq=3; got: {s}");
        assert!(
            s.contains("abc"),
            "Display must include the orphan member; got: {s}"
        );
        let err = TranscriptError::NonMonotonicSeq { seq: 5 };
        let s = format!("{err}");
        assert!(s.contains("seq=5"));
    }

    /// Smoke: an action's `street` (preflop / flop / turn /
    /// river) is not part of the persisted schema (the engine
    /// drives the transitions), but the `Action` enum's
    /// `u32` encoding is what the transcript serialises. This
    /// test pins that a `Check` action round-trips through
    /// `u32::from` / `Action::from` without losing the variant,
    /// so a downstream tool can distinguish `Call(0)` from
    /// `Check` after a JSON round-trip.
    #[test]
    fn action_u32_round_trip_preserves_variant() {
        let actions = [
            Action::Call(0),
            Action::Check,
            Action::Fold,
            Action::Raise(2),
        ];
        for a in actions {
            let n = u32::from(a);
            let b = Action::from(n);
            // `Action` is `Copy + PartialEq`; identity after a
            // u32 round-trip is the contract a downstream tool
            // depends on.
            assert_eq!(a, b, "action u32 round-trip lost variant for {a:?}");
        }
    }

    /// `from_json` must be the exact inverse of `to_json` for a
    /// well-formed transcript: every `Hand` / `Participant` /
    /// `Play` field round-trips, and the parsed transcript
    /// re-passes `verify`. The bench harness writes the file
    /// with `to_json` and a downstream tool reads it back
    /// with `from_json`; any field that doesn't round-trip
    /// would silently lose information between the two
    /// sides. The on-disk hole/board are empty strings (the
    /// preflop / no-reveal path the room persistence emits),
    /// so the parser must handle those.
    #[test]
    fn from_json_round_trips_to_json() {
        let hand = sample_hand();
        let participants = sample_participants();
        let member = participants[0].user();
        let plays = sample_plays(hand.id(), member);
        let original = Transcript::new(hand, participants, plays);
        let json = original.to_json();
        let parsed =
            Transcript::from_json(&json).expect("from_json must parse a to_json-emitted document");
        // Re-serialise and compare: a field that round-trips
        // must produce the same bytes; a field that doesn't
        // will surface as a string mismatch the test can read.
        let reserialised = parsed.to_json();
        assert_eq!(
            json, reserialised,
            "from_json → to_json must be the identity on the wire"
        );
        parsed
            .verify()
            .expect("re-parsed transcript must re-pass verify");
    }
}
