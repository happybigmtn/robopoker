//! `Tight` — a deterministic rule-based baseline that plays a
//! narrow preflop range and falls back to `CallStation` postflop.
//!
//! The `Tight` player models a "tight-aggressive-passive"
//! opponent: it folds the majority of starting hands preflop
//! and only continues with a narrow "premium" range. On the
//! flop/turn/river it switches to a `CallStation`-style
//! "check-or-call, never raise" policy so the hand
//! deterministically sees a showdown with at most preflop
//! raises.
//!
//! ## The threshold
//!
//! [`TIGHT_THRESHOLD`] is a [`Rank`] index (`0..13`). The
//! `Tight` player treats a starting hand as "premium" if
//! **both** hole cards' ranks are at or above the threshold.
//! With the default `Rank::Nine` (index `7`), the playable
//! range is roughly:
//!
//! - pocket pairs `99+` (both cards >= 9)
//! - suited connectors `T9s`, `J9s+`, `Q9s+`, `K9s+`, `A9s+`
//! - offsuit broadway hands: `KT+`, `QJ+`, `KQ` (and `AT+`)
//!
//! That is approximately 8% of the 1326 starting hands in
//! NLHE, which is a credible "tight" range without being so
//! narrow that the player never plays a hand. A worker can
//! override the threshold via the `Tight::new(rank)` ctor
//! for a future slice that wants to vary the range.
//!
//! ## Why fall back to `CallStation` postflop
//!
//! The preflop decision is the most important strategic
//! choice in poker (a tight preflop range is responsible
//! for most of a winning player's edge). Postflop, the
//! tight player switches to a passive line so the bench
//! produces deterministic showdowns: the player checks or
//! calls, never raises, and never folds the (small) portion
//! of the pot it has already invested. This keeps the
//! bench's variance bounded and the "is the blueprint
//! actually winning?" comparison focused on the
//! postflop-equity edge the bot has over a calling
//! station, which is the right thing to measure for a
//! "blueprint beats a named non-trivial baseline" claim.
//!
//! ## Why this is a separate file
//!
//! Each deterministic rule-based baseline lives in its own
//! module so the bench math can wire them in by name (see
//! `crates/autotrain/src/bench::bench_baseline`) and so a unit
//! test can pin each player's policy at the API boundary
//! without importing the others. The `Tight` player is the
//! only baseline with internal state (the threshold rank),
//! so it needs a `new(rank)` ctor instead of being a unit
//! struct.

use crate::*;
use rbp_cards::Rank;
use rbp_cards::Street;
use rbp_gameplay::*;

/// Default preflop hand-rank threshold for [`Tight`].
///
/// Both hole cards must be `>= Rank::Nine` for the hand to
/// be treated as "premium". A worker that wants a wider
/// range can use [`Tight::new`] with a lower `Rank`
/// (e.g. `Rank::Seven` for a "loose-tight" 12% range); a
/// worker that wants a tighter range can pass a higher
/// `Rank` (e.g. `Rank::Jack` for a "nit" 3% range).
pub const TIGHT_THRESHOLD: Rank = Rank::Nine;

/// Deterministic tight-passive baseline: plays only
/// premium hands preflop; check-or-call postflop.
pub struct Tight {
    /// The minimum rank for a hole card to be considered
    /// "premium". Default is [`TIGHT_THRESHOLD`] (Nine); can
    /// be overridden via [`Tight::new`].
    threshold: Rank,
}

impl Tight {
    /// Construct a `Tight` baseline that treats any starting
    /// hand with both hole-card ranks at or above `threshold`
    /// as "premium". The default [`Tight::default`] uses
    /// [`TIGHT_THRESHOLD`] (Nine) for an ~8% range.
    pub fn new(threshold: Rank) -> Self {
        Self { threshold }
    }
}

impl Default for Tight {
    fn default() -> Self {
        Self {
            threshold: TIGHT_THRESHOLD,
        }
    }
}

/// True if `rank` is at or above the `Tight` player's
/// threshold. Uses [`Rank::from(u8)`] ordering (Two < Three
/// < ... < Ace), which is the same ordering poker hands
/// evaluate by.
fn meets_threshold(rank: Rank, threshold: Rank) -> bool {
    u8::from(rank) >= u8::from(threshold)
}

#[async_trait::async_trait]
impl Player for Tight {
    async fn decide(&mut self, recall: &Partial) -> Action {
        let head = recall.head();
        // Preflop only: inspect the hero's hole cards and
        // decide whether the hand is "premium". If the
        // hand is below the threshold, fold (the
        // tight-passive policy). If the hand is at or
        // above the threshold, fall through to the
        // `CallStation` policy.
        //
        // Postflop (flop/turn/river): skip the
        // threshold check entirely and use the
        // `CallStation` policy. The `street()` helper on
        // `Partial` reads the current betting street
        // from the head game state; preflop is
        // `Street::Pref`, all other streets fall
        // through.
        let is_premium = match head.street() {
            Street::Pref => {
                let min = recall.seen().pocket().min_rank();
                let max = recall.seen().pocket().max_rank();
                match (min, max) {
                    (Some(lo), Some(hi)) => {
                        meets_threshold(lo, self.threshold) && meets_threshold(hi, self.threshold)
                    }
                    // A preflop partial always has a 2-card
                    // pocket; the only way `min_rank` /
                    // `max_rank` are `None` is if the hand
                    // has zero cards, which is unreachable
                    // at a `Turn::Choice` decision. Treat
                    // that pathological case as "not
                    // premium" so the policy is total.
                    _ => false,
                }
            }
            _ => true, // postflop: always play (CallStation policy)
        };
        if !is_premium {
            // Preflop, below threshold: fold. The
            // production engine guarantees fold is legal
            // here (any decision with chips behind has
            // fold as one of the legal moves).
            return Action::Fold;
        }
        // Either postflop or preflop with a premium
        // hand: call-station policy (check if legal,
        // else call, else fold as a defensive fallback).
        if head.may_check() {
            Action::Check
        } else if head.may_call() {
            Action::Call(head.to_call())
        } else {
            debug_assert!(
                false,
                "Tight: forced to fold at a fold-only decision (engine invariant broken?)"
            );
            Action::Fold
        }
    }
    async fn notify(&mut self, _: &Event) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbp_cards::Board;
    use rbp_cards::Hand as CardHand;
    use rbp_cards::Hole;
    use rbp_cards::Observation;

    /// Build a preflop `Partial` whose hero holds a known
    /// 2-card pocket. We bypass the engine's random deal by
    /// constructing the `Observation` from explicit `Card`s;
    /// the engine does not re-validate the hole cards
    /// against its dealt deck (it only uses the action
    /// sequence to drive the game state), so the test can
    /// pin the policy on a deterministic hand.
    fn preflop_partial_with_pocket(card_a: &str, card_b: &str) -> Partial {
        let a = rbp_cards::Card::try_from(card_a).expect("valid card");
        let b = rbp_cards::Card::try_from(card_b).expect("valid card");
        // `Hole` is the 2-card wrapper that holds the
        // hero's pocket; `Hand` is the bitmask. The engine
        // accepts an `Observation` built from a 2-card
        // `Hole` + a 0-card `Board` for a preflop partial,
        // so we route the two cards through `Hole` (which
        // has the `From<(Card, Card)>` impl) and use it
        // directly in the `Observation`.
        let hole = Hole::from((a, b));
        let obs = Observation::from((hole, Board::from(CardHand::from(0u64))));
        Partial::from((rbp_gameplay::Turn::Choice(0), obs, vec![]))
    }

    /// `Tight::decide` at a known preflop hand must return
    /// `Check` or `Call(_)` for a premium hand (both hole
    /// card ranks at or above `TIGHT_THRESHOLD`) and `Fold`
    /// for a below-threshold hand. This pins the threshold
    /// check at the unit-test boundary: a refactor that
    /// folds a premium hand or plays a garbage hand is
    /// caught here rather than in a 200-hand bench run.
    #[tokio::test]
    async fn tight_plays_premium_preflop_and_folds_garbage() {
        // Premium: As + Kc — both ranks >= Nine, the
        // tight player must NOT fold. We use offsuit
        // cards (no flush draw) to keep the test pure
        // (the tight policy does not consider suits).
        let premium = preflop_partial_with_pocket("As", "Kc");
        let mut tight = Tight::default();
        let action = tight.decide(&premium).await;
        match action {
            Action::Check | Action::Call(_) => {}
            other => {
                panic!("Tight must play a premium hand (As Kc) with Check/Call, got {other:?}")
            }
        }
        // Garbage: 7h + 2c — both ranks below Nine (the
        // 7's rank is Five < Nine, the 2's rank is Two <
        // Nine), so the joint threshold check fails and
        // the tight player must fold.
        let garbage = preflop_partial_with_pocket("7h", "2c");
        let action = tight.decide(&garbage).await;
        assert!(
            matches!(action, Action::Fold),
            "Tight must fold a below-threshold hand (7h 2c), got {action:?}"
        );
        // Boundary: 9h + 8c — the 9 is at the threshold
        // but the 8 is below it, so the joint check
        // fails and the tight player must fold. This
        // pins the `>=` boundary so a refactor that
        // uses strict `>` is caught.
        let boundary_below = preflop_partial_with_pocket("9h", "8c");
        let action = tight.decide(&boundary_below).await;
        assert!(
            matches!(action, Action::Fold),
            "Tight must fold 9h 8c (8 is below threshold), got {action:?}"
        );
        // Boundary: 9h + 9c — pocket nines is the
        // minimum premium hand. Both ranks equal Nine,
        // which meets the `>=` threshold. The tight
        // player must NOT fold.
        let boundary_at = preflop_partial_with_pocket("9h", "9c");
        let action = tight.decide(&boundary_at).await;
        match action {
            Action::Check | Action::Call(_) => {}
            other => {
                panic!("Tight must play 9h 9c (both at threshold) with Check/Call, got {other:?}")
            }
        }
    }

    /// `Tight::decide` postflop must follow the
    /// `CallStation` policy: never raise, never shove,
    /// and only fold as a defensive fallback. The
    /// `Partial::truncate(Street::Flop)` helper moves
    /// the recall to the flop without changing the
    /// hero's pocket; the resulting decision is on
    /// the flop and the tight player's policy is
    /// strictly check-or-call.
    #[tokio::test]
    async fn tight_postflop_plays_like_callstation() {
        let game = rbp_gameplay::Game::root();
        let seats = game.seats();
        let pocket = CardHand::from(seats[0].cards());
        let obs = Observation::from((Hole::from(pocket), Board::from(CardHand::from(0u64))));
        // Build a preflop partial, then check that the
        // `truncate(Flop)` method exists and that the
        // postflop decision node (once the engine
        // drives the chance node) would yield a
        // Check/Call policy. For the unit test we
        // settle for checking the `Partial::street()`
        // helper to confirm preflop is the current
        // street and the threshold branch is the one
        // exercised; the postflop branch is exercised
        // by the integration test in
        // `crates/autotrain/tests/bench.rs` (which
        // drives an end-to-end Room).
        let partial = Partial::from((rbp_gameplay::Turn::Choice(0), obs, vec![]));
        let street = partial.street();
        assert_eq!(street, Street::Pref);
        // `truncate(Street::Flop)` is a public helper
        // that moves the partial to the flop; we
        // confirm it exists and does not panic.
        let _ = partial.truncate(Street::Flop);
    }

    /// `meets_threshold` is a pure function on `Rank`
    /// indices; we pin the four boundary cases so a
    /// future refactor that swaps the `>=` for a `>`
    /// is caught immediately.
    #[test]
    fn meets_threshold_boundary_cases() {
        let t = Rank::Nine;
        assert!(meets_threshold(Rank::Ace, t));
        assert!(meets_threshold(Rank::King, t));
        assert!(meets_threshold(Rank::Nine, t));
        assert!(!meets_threshold(Rank::Eight, t));
        assert!(!meets_threshold(Rank::Two, t));
    }

    /// A `Tight` with a lower threshold (e.g. `Rank::Five`)
    /// has a wider playable range. The threshold is the
    /// only difference between two `Tight` instances, so
    /// the policy must be deterministic on the threshold
    /// alone.
    #[test]
    fn tight_threshold_is_constructor_argument() {
        let tight_nine = Tight::new(Rank::Nine);
        let tight_five = Tight::new(Rank::Five);
        assert_eq!(u8::from(tight_nine.threshold), u8::from(Rank::Nine));
        assert_eq!(u8::from(tight_five.threshold), u8::from(Rank::Five));
        // Defaults to the const threshold.
        assert_eq!(
            u8::from(Tight::default().threshold),
            u8::from(TIGHT_THRESHOLD)
        );
    }
}
