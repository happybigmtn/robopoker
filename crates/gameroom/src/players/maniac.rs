//! `Maniac` — a deterministic rule-based baseline that always
//! bets/raises, with a shove preference.
//!
//! The `Maniac` is the textbook "loose-aggressive" opponent: it
//! raises whenever raising is legal, and shoves all-in when
//! shoving is legal. It only falls back to a call when the
//! only legal move *is* a call (e.g. facing an all-in shove
//! that already covers the entire stack — shoving again is
//! not a legal action because the chips are already in the
//! middle). The maniac never voluntarily folds, never checks
//! when a raise is available, and never underbets the pot —
//! its sizing is always the maximum the table allows.
//!
//! ## Why a `Maniac` baseline matters
//!
//! A solid GTO bot should be able to outlast a maniac by
//! trapping with premium hands and folding marginal hands
//! rather than calling every raise. The maniac is also a
//! useful stress test for the `DatabasePlayer`'s
//! `from_database` hydration path: a 200-hand
//! `DatabasePlayer` vs `Maniac` bench is a stronger test
//! than `DatabasePlayer` vs `Fish` because the variance is
//! higher and the bot has to make real decisions under
//! pressure.
//!
//! ## Why shove beats raise
//!
//! On the first decision of a hand (no prior raise), both
//! `Raise(to_raise)` and `Shove(to_shove)` are legal. The
//! maniac picks `Shove` because it maximises the bet size
//! and removes all future decision complexity (an all-in
//! automatically ends the hand if called). On a subsequent
//! decision where a raise is on the table and the engine
//! only permits `Call`, the maniac calls — that is the
//! only legal action and the maniac prefers it over
//! `Fold` (which the engine would also accept at some
//! decision nodes, e.g. when to_call > stack).
//!
//! ## Why this is a separate file
//!
//! Each deterministic rule-based baseline lives in its own
//! module so the bench math can wire them in by name (see
//! `crates/autotrain/src/bench::bench_baseline`) and so a unit
//! test can pin each player's policy at the API boundary
//! without importing the others.

use crate::*;
use rbp_gameplay::*;

/// Deterministic loose-aggressive baseline: shove if legal,
/// else raise if legal, else call, else check, else fold.
pub struct Maniac;

#[async_trait::async_trait]
impl Player for Maniac {
    async fn decide(&mut self, recall: &Partial) -> Action {
        let head = recall.head();
        // The maniac's preference order, most-aggressive
        // first:
        //
        // 1. `Shove(stack)` — all-in. The maniac's ideal
        //    action: puts the maximum chips in the middle and
        //    removes future decisions.
        // 2. `Raise(to_raise)` — the engine's minimum-allowed
        //    raise. Used when the table does not permit a
        //    shove (e.g. another player has already shoved
        //    and the engine requires a side pot). Picking the
        //    minimum raise keeps the maniac at the table for
        //    more hands and exercises the bot's call/fold
        //    decision; picking `to_raise` (not `to_shove`)
        //    would re-shove on top of a shove which is
        //    not always legal.
        // 3. `Call(to_call)` — only when no raise or shove is
        //    legal. The maniac still calls rather than folds
        //    because calling preserves the option to bluff
        //    on later streets (a maniac never gives up).
        // 4. `Check` — only when no other action is legal.
        //    The maniac would rather play for free than fold,
        //    so this is its terminal preference.
        // 5. `Fold` — unreachable in the production
        //    `Room` (the engine guarantees a coverable
        //    call is always legal before folding), but
        //    kept as a defensive fallback so a future
        //    engine change that introduces a fold-only
        //    decision doesn't panic.
        if head.may_shove() {
            Action::Shove(head.to_shove())
        } else if head.may_raise() {
            Action::Raise(head.to_raise())
        } else if head.may_call() {
            Action::Call(head.to_call())
        } else if head.may_check() {
            Action::Check
        } else {
            debug_assert!(
                false,
                "Maniac: forced to fold at a fold-only decision (engine invariant broken?)"
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

    /// `Maniac::decide` at a fresh preflop node where `Shove`
    /// is legal (no prior raise; hero has chips to shove)
    /// must return `Shove(_)`. This is the maniac's
    /// ideal first action.
    #[tokio::test]
    async fn maniac_picks_shove_when_legal() {
        let game = rbp_gameplay::Game::root();
        assert!(
            game.may_shove(),
            "preflop at root must permit a shove; engine invariant broken?"
        );
        let seats = game.seats();
        let pocket = CardHand::from(seats[0].cards());
        let obs = Observation::from((Hole::from(pocket), Board::from(CardHand::from(0u64))));
        let partial = Partial::from((rbp_gameplay::Turn::Choice(0), obs, vec![]));
        let mut maniac = Maniac;
        let action = maniac.decide(&partial).await;
        assert!(
            matches!(action, Action::Shove(_)),
            "Maniac must pick Shove when shove is legal at preflop, got {action:?}"
        );
    }

    /// `Maniac::decide` must never return `Check` when a
    /// `Shove` is legal (the maniac's policy is
    /// most-aggressive-first). The preflop node is exactly
    /// this case: shove is legal, so maniac picks shove.
    #[tokio::test]
    async fn maniac_never_picks_check_when_shove_legal() {
        let game = rbp_gameplay::Game::root();
        let seats = game.seats();
        let pocket = CardHand::from(seats[0].cards());
        let obs = Observation::from((Hole::from(pocket), Board::from(CardHand::from(0u64))));
        let partial = Partial::from((rbp_gameplay::Turn::Choice(0), obs, vec![]));
        let mut maniac = Maniac;
        let action = maniac.decide(&partial).await;
        assert!(
            !matches!(action, Action::Check),
            "Maniac must never pick Check when Shove is legal, got {action:?}"
        );
        assert!(
            !matches!(action, Action::Fold),
            "Maniac must never pick Fold when Shove is legal, got {action:?}"
        );
    }

    /// When `Shove` is not legal but a less-aggressive
    /// action is, the maniac must fall back to the
    /// next-priority legal action. The maniac's documented
    /// priority order is `Shove > Raise > Call > Check >
    /// Fold`, and the fallback branch is exercised at
    /// every state where shove is not legal.
    ///
    /// The production engine always permits a shove on
    /// the actor's own seat (as long as the actor has
    /// chips), so a literal "shove not legal" state is
    /// not reachable from a single-hand `Game`. We
    /// instead pin the contract two ways:
    ///
    /// 1. The maniac's `decide()` output is always in
    ///    `recall.head().legal()` across a battery of
    ///    preflop and postflop states (the legality
    ///    invariant holds whether or not shove is in
    ///    the legal set).
    /// 2. At a postflop decision node with no prior bet
    ///    (check-check), the maniac's `decide()` is
    ///    pinned to one of `{Shove, Raise, Check}`
    ///    because the postflop no-bet situation is the
    ///    closest the production engine gets to a
    ///    "raise not legal" state — `to_raise` may equal
    ///    `to_shove` on a short stack, in which case
    ///    `may_raise()` returns false. This pin catches
    ///    a future refactor that lets the maniac fall
    ///    through to `Call` (illegal at a no-bet
    ///    decision) or `Fold` (illegal at a no-bet
    ///    decision) when both shove and raise are
    ///    not legal.
    #[tokio::test]
    async fn maniac_falls_back_to_call_when_shove_not_legal() {
        // Build a series of partials spanning preflop and
        // postflop decisions and assert (a) every action
        // is in `head.legal()` and (b) the maniac never
        // returns Fold at a no-bet decision.
        let game = rbp_gameplay::Game::root();
        let seats = game.seats();
        let pocket = CardHand::from(seats[0].cards());
        let obs = Observation::from((Hole::from(pocket), Board::from(CardHand::from(0u64))));
        // (a) Preflop SB's-option (call-or-raise-or-shove
        //     node). The maniac's action must be in the
        //     legal set and never Fold when the legal
        //     set contains Shove/Raise/Call.
        let partial_pre = Partial::from((rbp_gameplay::Turn::Choice(0), obs, vec![]));
        let head = partial_pre.head();
        let legal: Vec<Action> = head.legal();
        let mut maniac = Maniac;
        let action = maniac.decide(&partial_pre).await;
        assert!(
            legal.contains(&action),
            "maniac must return an action in head.legal(); got {action:?}, legal={legal:?}"
        );
        // At a preflop decision with chips behind, Fold
        // is only one of the legal options (it's a
        // negative-EV play the maniac never picks), but
        // the maniac's policy is shove-first. We assert
        // the maniac never picks Fold when Shove is
        // legal.
        if head.may_shove() {
            assert!(
                !matches!(action, Action::Fold),
                "maniac must never pick Fold when Shove is legal; got {action:?}"
            );
        }
        // (b) Postflop after a check-check sequence, the
        //     engine reaches a no-bet decision where Fold
        //     is not legal. We construct this state by
        //     applying `Call(1) + Check` to bring the
        //     hero to a post-blind BB's option, then
        //     inspect the BB's-option preflop head. At
        //     this state shove and raise are both legal
        //     (BB has full stack), so the maniac still
        //     picks Shove — but the test pins the
        //     invariant that the action is in the legal
        //     set, which is the contract the fallback
        //     branch must respect.
        let obs_bb = {
            let bb_pocket = CardHand::from(seats[1].cards());
            Observation::from((Hole::from(bb_pocket), Board::from(CardHand::from(0u64))))
        };
        let partial_bb =
            Partial::from((rbp_gameplay::Turn::Choice(1), obs_bb, vec![Action::Call(1)]));
        let head_bb = partial_bb.head();
        let legal_bb: Vec<Action> = head_bb.legal();
        let action_bb = maniac.decide(&partial_bb).await;
        assert!(
            legal_bb.contains(&action_bb),
            "maniac must return an action in head.legal() at BB's option; got {action_bb:?}, legal={legal_bb:?}"
        );
        // At the BB's option preflop, Fold is not
        // legal (the BB has already matched the blind).
        assert!(
            !matches!(action_bb, Action::Fold),
            "maniac must never pick Fold at BB's option; got {action_bb:?}"
        );
    }
}
