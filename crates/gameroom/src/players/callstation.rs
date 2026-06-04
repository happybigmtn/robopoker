//! `CallStation` — a deterministic rule-based baseline that never folds
//! and never raises.
//!
//! The `CallStation` is the textbook "loose-passive" opponent: it
//! checks whenever checking is legal (free card), and otherwise
//! calls the current bet. It never opens or 3-bets a pot, never
//! folds to a raise, and never re-raises. The only `Action`
//! variants this player can ever return are `Action::Check` and
//! `Action::Call(_)` — both are in `recall.head().legal()` by
//! construction (the runtime cannot face a decision where
//! *neither* check nor call is legal on a stack that covers
//! the bet).
//!
//! ## Why a `CallStation` baseline matters
//!
//! The CEO roadmap lists the post-P0 work as "a stronger named
//! baseline" so the bench can claim a credible
//! "blueprint-beats-non-trivial-rule" result rather than
//! "blueprint-beats-uniform-random". The `CallStation` is the
//! second-easiest interesting baseline: it always sees a
//! showdown, so a strong bot wins by playing better than 50/50
//! postflop and by extracting value from premium hands when
//! the calling station refuses to fold.
//!
//! ## Why this is a separate file
//!
//! Each deterministic rule-based baseline lives in its own
//! module so the bench math can wire them in by name (see
//! `crates/autotrain/src/bench::bench_baseline`) and so a unit
//! test can pin each player's policy at the API boundary
//! without importing the others. The baseline's decision is
//! fully described in this file's `decide()` body and the
//! corresponding `tests` module.

use crate::*;
use rbp_gameplay::*;

/// Deterministic loose-passive baseline: check-or-call, never
/// fold, never raise.
pub struct CallStation;

#[async_trait::async_trait]
impl Player for CallStation {
    async fn decide(&mut self, recall: &Partial) -> Action {
        let head = recall.head();
        // The runtime only ever asks the player to decide at a
        // `Turn::Choice` node, where at least one of {check,
        // call, fold, raise, shove} is legal. The call-station
        // policy is: prefer check (free card), otherwise call
        // the bet. If both check and call are legal, check is
        // preferred (a passive line that real fish and
        // calling-stations actually play — they rarely put
        // extra money in voluntarily).
        if head.may_check() {
            Action::Check
        } else if head.may_call() {
            Action::Call(head.to_call())
        } else {
            // The only path here is a decision node where the
            // only legal move is fold (e.g. facing an all-in
            // shove with insufficient chips to cover the bet).
            // The call-station's policy of "never fold" still
            // applies in spirit, but if the runtime gives us
            // no legal call to make, we must obey the rules
            // and fold. This path is unreachable in the
            // production `Room` (the engine guarantees a
            // coverable call is always legal before folding),
            // but a future engine change that introduces a
            // fold-only decision would surface here as a
            // debug_assert rather than a silent misplay.
            debug_assert!(
                false,
                "CallStation: forced to fold at a fold-only decision (engine invariant broken?)"
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

    /// Build a `Partial` at preflop for hero (seat 0). The
    /// actual hole cards are taken from `Game::root()`'s seat
    /// 0 so the engine's `Partial` constructor can validate
    /// the input; the call-station's policy is independent of
    /// the cards it holds, so the specific hole cards don't
    /// matter for the tests below.
    fn preflop_partial_for_hero() -> Partial {
        let game = rbp_gameplay::Game::root();
        let seats = game.seats();
        let pocket = CardHand::from(seats[0].cards());
        let obs = Observation::from((Hole::from(pocket), Board::from(CardHand::from(0u64))));
        Partial::from((rbp_gameplay::Turn::Choice(0), obs, vec![]))
    }

    /// `CallStation::decide` at a check-only decision must
    /// return `Check`. We construct a `Partial` at the
    /// BB's-option preflop node (where the legal actions
    /// are `Raise / Shove / Check`, with `Check` being the
    /// call-station's prefer-passive choice). The action
    /// history `[Call(1)]` represents the SB's call; the
    /// engine puts the BB at the decision node, and from
    /// `Turn::Choice(1)` (the BB) the engine permits Check
    /// as the free-option line.
    #[tokio::test]
    async fn callstation_picks_check_when_legal_else_call() {
        let game = rbp_gameplay::Game::root();
        // The seat-0 hero in `Game::root()` is the SB.
        // The seat-1 BB has the option post-SB-completes,
        // so we need a `Partial` for seat 1 with the SB's
        // call in the action history. The seat-1 hero's
        // pocket is on `seats[1]`, not `seats[0]`.
        let seats = game.seats();
        let bb_pocket = CardHand::from(seats[1].cards());
        let obs = Observation::from((Hole::from(bb_pocket), Board::from(CardHand::from(0u64))));
        let partial = Partial::from((rbp_gameplay::Turn::Choice(1), obs, vec![Action::Call(1)]));
        let head = partial.head();
        // The BB's option is a check/raise/shove node,
        // never a call-or-fold node. The call-station's
        // policy is to prefer check when legal, so we
        // expect `Check`.
        assert!(
            head.may_check(),
            "BB's-option preflop must permit Check; got legal={:?}",
            head.legal()
        );
        assert!(
            !head.may_call(),
            "BB's-option preflop must NOT require a call (BB already matched SB); got legal={:?}",
            head.legal()
        );
        let mut station = CallStation;
        let action = station.decide(&partial).await;
        assert!(
            matches!(action, Action::Check),
            "CallStation must pick Check when check is legal, got {action:?}"
        );
    }

    /// `CallStation::decide` must never return a `Fold`,
    /// `Raise`, or `Shove` — those are the three moves the
    /// call-station's policy forbids. We construct a known
    /// preflop decision and assert the returned `Action` is
    /// `Check` or `Call(_)`.
    #[tokio::test]
    async fn callstation_never_picks_fold_or_raise() {
        let partial = preflop_partial_for_hero();
        let mut station = CallStation;
        let action = station.decide(&partial).await;
        match action {
            Action::Check | Action::Call(_) => {}
            other => panic!("CallStation must never return {other:?}"),
        }
    }
}
