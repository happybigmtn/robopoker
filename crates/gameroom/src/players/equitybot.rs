//! Rule-based named baseline player.
//!
//! `EquityBot` is the v2 named baseline the CEO testnet roadmap
//! points to as the "next slice" after the random `Fish` baseline:
//! a deterministic, position-agnostic rule policy that
//! (1) estimates its current hand strength via Monte Carlo
//!     `Observation::simulate`, then
//! (2) picks a legal action by a small, well-defined threshold
//!     table on that estimate.
//!
//! The bot is *named* (not random) so a benchmark harness can
//! report "trained blueprint beats a *named* rule-based baseline"
//! instead of just "beats random". It is also deliberately
//! *weak*: it never bluffs, never raises for value, and never
//! considers opponent modelling. A trained blueprint should
//! dominate it; if it doesn't, the blueprint isn't trained.
//!
//! ## Decision table
//!
//! For a given `Partial`, the bot computes `eq = simulate(256)`
//! from `recall.seen()` (probability of winning against a
//! uniform-random opponent on the current street, with future
//! streets sampled). It then classifies the legal-action set:
//!
//! - **If only `Check` is legal** → take it. (Free card; showdown
//!   equity will resolve the hand.)
//! - **If only `Call` and/or `Fold` are legal** (facing a bet):
//!   - `eq >= CALL_THRESHOLD` (0.50) → call.
//!   - else → fold.
//! - **If a `Raise` or `Shove` is legal** (i.e. we are first to
//!   act on this street, or a raise is on the table):
//!   - `eq >= RAISE_THRESHOLD` (0.65) → shove (all-in, the
//!     strongest action we have; we don't know how to size).
//!   - `eq >= CALL_THRESHOLD` (0.50) and `Call` is legal → call.
//!   - else if `Check` is legal → check (we decline to put
//!     money in with a marginal hand).
//!   - else → fold.
//!
//! The thresholds are deliberately conservative (a 50% call
//! threshold is the break-even point against a uniform opponent
//! given no rake; the 65% raise threshold leaves headroom for
//! the variance of the Monte Carlo estimate). The bot does not
//! attempt to read the board texture, count outs, or distinguish
//! semi-bluff from value — it is a *baseline*, not a heuristic
//! solver.
//!
//! ## Determinism
//!
//! The Monte Carlo estimate uses `rand::rng()` (a thread-local
//! RNG with a system-seeded handle). The threshold table is a
//! pure function of the estimate and the legal-action set, so
//! the bot is deterministic up to RNG draws. This is good
//! enough for a benchmark baseline: the harness aggregates over
//! many hands, so per-hand RNG noise averages out.

use crate::*;
use rbp_core::Probability;
use rbp_gameplay::*;

/// Number of Monte Carlo trials used to estimate hand equity
/// from `Observation::simulate`. Picked at 256 because the
/// `EquityBot` is a *baseline* — a noisier estimate just makes
/// the bot weaker in a deterministic direction, and the bench
/// already averages over K≥200 hands.
const MC_TRIALS: usize = 256;

/// Equity threshold (win-probability vs uniform random) at or
/// above which the bot is willing to call a bet. 0.50 is the
/// break-even point for a no-rake call against a uniform
/// opponent; the bot rounds up to 0.50 because the legal-action
/// table is discrete and the threshold is meant to be the
/// smallest sane value, not a calibrated edge.
pub const CALL_THRESHOLD: Probability = 0.50;

/// Equity threshold at or above which the bot is willing to
/// raise / shove. 0.65 is intentionally well above the break-
/// even point because a raise is a *worse* mistake than a call:
/// the bot commits more chips on a marginal hand, so the
/// threshold needs more confidence to justify it.
pub const RAISE_THRESHOLD: Probability = 0.65;

/// Rule-based named baseline player. See [module docs](self).
pub struct EquityBot;

impl Default for EquityBot {
    fn default() -> Self {
        Self
    }
}

impl EquityBot {
    /// Pick the highest-priority legal action that matches the
    /// bot's rule table for the given equity estimate.
    ///
    /// The function is pure (it does no RNG itself; the equity
    /// estimate is supplied by the caller) so unit tests can pin
    /// the threshold table exactly without standing up a `Room`.
    pub fn choose(legal: &[Action], equity: Probability) -> Action {
        debug_assert!(
            !legal.is_empty(),
            "EquityBot::choose called with empty legal actions"
        );
        let may_raise = legal
            .iter()
            .any(|a| matches!(a, Action::Raise(_) | Action::Shove(_)));
        let may_call = legal.iter().any(|a| matches!(a, Action::Call(_)));
        let may_check = legal.iter().any(|a| matches!(a, Action::Check));
        let may_fold = legal.iter().any(|a| matches!(a, Action::Fold));
        // Order: shove/raise (highest confidence) > call > check > fold.
        if may_raise && equity >= RAISE_THRESHOLD {
            // Prefer `Shove` over `Raise` if both are legal;
            // the bot has no sizing model, so a shove is the
            // "commit everything" action that a deterministic
            // threshold can justify. The legal-action set
            // already includes shove iff may_shove; if only a
            // min-raise is legal we take it.
            return legal
                .iter()
                .find(|a| matches!(a, Action::Shove(_)))
                .cloned()
                .or_else(|| {
                    legal
                        .iter()
                        .find(|a| matches!(a, Action::Raise(_)))
                        .cloned()
                })
                .expect("may_raise implies at least one of Shove/Raise is in `legal`");
        }
        if may_call && equity >= CALL_THRESHOLD {
            return legal
                .iter()
                .find(|a| matches!(a, Action::Call(_)))
                .cloned()
                .expect("may_call implies Call is in `legal`");
        }
        if may_check {
            return Action::Check;
        }
        if may_fold {
            return Action::Fold;
        }
        // If none of the four patterns matched, the legal set
        // is non-empty but contains only actions we don't
        // classify (e.g. `Blind` posting at the very start of
        // a hand, before the bot has any decision to make).
        // The first such action is correct by construction:
        // the legal set at a Blind-post node is a singleton.
        legal
            .first()
            .cloned()
            .expect("non-empty legal actions at the EquityBot::choose API boundary")
    }
}

#[async_trait::async_trait]
impl Player for EquityBot {
    async fn decide(&mut self, recall: &Partial) -> Action {
        let equity = recall.seen().simulate(MC_TRIALS);
        let legal = recall.head().legal();
        Self::choose(&legal, equity)
    }
    async fn notify(&mut self, _: &Event) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivially-strong equity estimate (1.0 = guaranteed win)
    /// with a raise-shove-call-check legal set must produce a
    /// raise-class action. This pins the "value-bet" half of
    /// the threshold table.
    #[test]
    fn high_equity_with_raise_legal_picks_shove_or_raise() {
        let legal = vec![
            Action::Shove(100),
            Action::Raise(4),
            Action::Call(2),
            Action::Check,
            Action::Fold,
        ];
        let chosen = EquityBot::choose(&legal, 0.95);
        assert!(
            matches!(chosen, Action::Shove(_) | Action::Raise(_)),
            "high equity + raise legal must pick a raise-class action; got {chosen:?}"
        );
    }

    /// A break-even equity estimate (0.55, above the call
    /// threshold but below the raise threshold) with a
    /// call-check-fold legal set must produce a call. This
    /// pins the "showdown-priced call" half of the table.
    #[test]
    fn call_threshold_equity_picks_call() {
        let legal = vec![Action::Call(2), Action::Check, Action::Fold];
        let chosen = EquityBot::choose(&legal, 0.55);
        assert_eq!(
            chosen,
            Action::Call(2),
            "0.55 equity (>= CALL_THRESHOLD) must call; got {chosen:?}"
        );
    }

    /// A sub-threshold equity (0.40) facing a bet must fold;
    /// it cannot check (a bet is on the table), so the only
    /// non-fold legal action is a call. The threshold table
    /// correctly folds rather than calls into a likely-loser
    /// showdown.
    #[test]
    fn sub_call_threshold_folds_to_bet() {
        let legal = vec![Action::Call(2), Action::Fold];
        let chosen = EquityBot::choose(&legal, 0.40);
        assert_eq!(
            chosen,
            Action::Fold,
            "0.40 equity (< CALL_THRESHOLD) facing a bet must fold; got {chosen:?}"
        );
    }

    /// A sub-threshold equity with a check legal option must
    /// take the free card. This is the "I have nothing but
    /// checking is free" path that makes the bot at least
    /// not terrible on later streets when it missed.
    #[test]
    fn sub_call_threshold_with_check_picks_check() {
        let legal = vec![Action::Check, Action::Raise(4)];
        let chosen = EquityBot::choose(&legal, 0.40);
        assert_eq!(
            chosen,
            Action::Check,
            "0.40 equity + check legal + no bet must take the free card; got {chosen:?}"
        );
    }

    /// At or above the raise threshold (0.65), the bot
    /// prefers `Shove` over `Raise` when both are legal —
    /// because the bot has no sizing model and an all-in is
    /// the maximal-value commitment. The 0.65 boundary is
    /// the published constant, so a refactor that drops the
    /// "shove beats raise" tie-break will fail this test
    /// before it lands.
    #[test]
    fn shove_preferred_over_raise_when_both_legal() {
        let legal = vec![Action::Shove(100), Action::Raise(4)];
        let chosen = EquityBot::choose(&legal, 0.80);
        assert!(
            matches!(chosen, Action::Shove(_)),
            "EquityBot::choose must prefer Shove over Raise when both are legal; got {chosen:?}"
        );
    }

    /// The published thresholds are the *whole* point of the
    /// bot: a future tuning pass that wants to tighten the
    /// table will move these numbers, and the test forces
    /// that decision to be conscious (it has to update the
    /// test alongside the constant). Locking them in here
    /// also pins the v2-baseline contract for downstream
    /// bench reports: a "trained bot beats EquityBot" result
    /// is computed against these specific numbers, not a
    /// silently-revised table.
    #[test]
    fn thresholds_match_published_constants() {
        assert!(
            (CALL_THRESHOLD - 0.50).abs() < 1e-9,
            "CALL_THRESHOLD must be 0.50 (break-even for a no-rake call); got {CALL_THRESHOLD}"
        );
        assert!(
            RAISE_THRESHOLD > CALL_THRESHOLD,
            "RAISE_THRESHOLD must be strictly greater than CALL_THRESHOLD; got raise={RAISE_THRESHOLD} call={CALL_THRESHOLD}"
        );
        assert!(
            (RAISE_THRESHOLD - 0.65).abs() < 1e-9,
            "RAISE_THRESHOLD must be 0.65 (raises are a worse mistake than calls, so they need more confidence); got {RAISE_THRESHOLD}"
        );
    }
}
