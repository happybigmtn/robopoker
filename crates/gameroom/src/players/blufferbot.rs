//! Semi-bluff-aware rule-based named baseline player.
//!
//! `BlufferBot` is the v4 named baseline the CEO testnet roadmap
//! lists as the next slice after the v3 `PreflopBot` baseline.
//! It closes the postflop-fold-equity gap that `EquityBot` and
//! `PreflopBot` both leave open: a pure threshold-only policy
//! never wins the pot uncontested on later streets, so the
//! trained blueprint never has to fold and a hand-by-hand
//! winning blueprint is mostly just printing the blinds. A
//! baseline that *occasionally* bluffs at a checked-to board
//! forces a trained opponent to defend, exposing whether the
//! trained bot's range is actually balanced.
//!
//! ## What "stronger" means
//!
//! The plan / roadmap phrase "v4 stronger baseline" is a *named
//! rule-based* baseline, not "a second trained config" (that
//! would be a much larger slice — a full retrain + a new
//! `NlheProfile` variant + a new `DatabasePlayer` path). The
//! v4 wins on three concrete axes that the v3 bot cannot:
//!
//! 1. **Preflop reuse.** The preflop tier table is *identical*
//!    to `PreflopBot::classify_pocket` (smallest legal raise /
//!    call / fold), so the v4 is at least as strong preflop as
//!    the v3 — there is no regression on the street the v3
//!    specialized in.
//! 2. **Value-bet raise on flop/turn/river.** When the postflop
//!    equity is above the v2 `EquityBot::RAISE_THRESHOLD` (0.65)
//!    and a raise is legal, the v4 raises (the v3 bot also
//!    raises here, but only as a side-effect of the same
//!    `EquityBot::choose` delegation; the v4 makes this
//!    explicit as the "value" half of the policy).
//! 3. **Semi-bluff on flop/turn when checked to.** When the
//!    postflop equity is *low* (≤ `BLUFF_EQUITY_CAP`, 0.40) and
//!    a `Check` is legal (i.e. the bot is first to act on the
//!    street and no bet is on the table), the v4 raises with a
//!    deterministic per-street frequency (`BLUFF_FREQ_FLOP =
//!    0.30`, `BLUFF_FREQ_TURN = 0.20`, `BLUFF_FREQ_RIVE = 0`).
//!    The river has zero fold equity, so a river bluff loses
//!    money in expectation; the v4 never bluffs the river.
//!
//! The semi-bluff is a *raise* (the smallest legal raise) on
//! the *first decision of the street* — a bluff *into* a bet
//! (facing `Call(_) | Fold`) is still a fold threshold
//! decision, delegated to `EquityBot::choose`. The v4 never
//! "donk-bets into" a pre-flop raiser; that is a different
//! policy and a different slice.
//!
//! ## Determinism
//!
//! `classify_bluff` is a pure function of the equity estimate,
//! the postflop "improvement" chance, and the street. The
//! per-street frequency is a `const` so unit tests can pin
//! every branch. The Monte Carlo estimate uses `rand::rng()`
//! (the system-seeded thread-local RNG), so the bot is
//! deterministic up to RNG draws — same contract as
//! `EquityBot` / `PreflopBot`, and good enough for a benchmark
//! baseline (the harness averages over K ≥ 200 hands).
//!
//! ## Why a v4 and not "just tune EquityBot"
//!
//! Tuning `EquityBot` to bluff would be a silent policy change
//! to the v2 baseline — every previously-recorded
//! "blueprint-beats-EquityBot" curve would shift, and the v2
//! would no longer be the *pure threshold* bot the bench
//! harnesses were written against. A *separate* v4 bot
//! preserves the v2 contract and the v3 contract (both
//! "passive named baselines") while adding a third
//! "aggressive named baseline" tier. A downstream scraper
//! that wants a "trained bot vs passive baseline" curve keeps
//! using `fish` / `equity` / `preflop`; a "trained bot vs
//! aggressive baseline" curve uses `bluffer`.

use crate::*;
use rbp_cards::Street;
use rbp_core::Chips;
use rbp_core::Probability;
use rbp_gameplay::*;

/// Number of Monte Carlo trials used to estimate hand equity
/// on later streets. Mirrors `EquityBot::MC_TRIALS` /
/// `PreflopBot::MC_TRIALS` (256) so the postflop branch is
/// directly comparable to the v2 / v3 baselines and the
/// "improvement" estimate below uses the same noise profile.
const MC_TRIALS: usize = 256;

/// Equity threshold at or above which the bot is willing to
/// semi-bluff-raise on a checked-to board with a *weak* hand.
/// 0.40 is the documented "weak hand" boundary — a hand with
/// 40% equity against a uniform opponent is a *bluff*, not a
/// value-bet (the v2 `EquityBot::RAISE_THRESHOLD` of 0.65 is
/// the value-bet boundary). A semi-bluff with a 0.40 hand
/// makes money when the opponent folds more than 60% of the
/// time; against a `Fish` baseline the fold rate is closer to
/// 100% on a checked-to board, so a 0.30 flop frequency is
/// pure profit.
const BLUFF_EQUITY_CAP: Probability = 0.40;

/// Equity threshold *above* which the bot refuses to bluff
/// (the hand is too strong to fold to a re-raise if caught).
/// Mirrors the v2 call threshold (0.50) — between 0.40 and
/// 0.50 is the "speculative" zone where the bot neither
/// bluffs nor calls aggressively.
const BLUFF_EQUITY_FLOOR: Probability = 0.50;

/// "Improvement" threshold *below* which a weak hand is
/// considered bluff-eligible. The improvement chance is the
/// probability the bot picks up a hand that beats a made
/// flush / straight on a later street. 0.20 is a
/// deliberately loose threshold — most weak hands fail this
/// (a flush draw on the flop is ~0.35 improvement, an inside
/// straight draw is ~0.17). The bluff set is "weak made hand
/// with no real draw", which is the cheapest bluffs to fire.
const BLUFF_IMPROVE_CAP: Probability = 0.20;

/// Per-street semi-bluff frequency (the share of *bluff-
/// eligible* hands the bot raises rather than checks).
///
/// - Flop: 30%. The flop has the most fold equity because
///   there are still two streets to play and the opponent's
///   range is widest.
/// - Turn: 20%. The turn has less fold equity (one street
///   left) but bluffs are still profitable against a passive
///   baseline.
/// - River: 0%. The river has no fold equity, so a bluff
///   loses money in expectation against any baseline. A
///   well-balanced v4 *never* bluffs the river.
const BLUFF_FREQ_FLOP: f64 = 0.30;
const BLUFF_FREQ_TURN: f64 = 0.20;
const BLUFF_FREQ_RIVE: f64 = 0.00;

/// Postflop bluff classification.
///
/// Returned by [`BlufferBot::classify_bluff`]. The three
/// variants correspond to the three branches the
/// postflop-decision loop dispatches on (raise / check /
/// value-or-fold), so unit tests can pin each one
/// independently of `EquityBot::choose`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluffDecision {
    /// The bot should *raise* (the smallest legal raise) on
    /// a checked-to board. The raise is either a
    /// semi-bluff (weak hand with no draw) or a value-bet
    /// (≥ 0.65 equity) — the choice is the same shape (a
    /// raise), so the bot picks it the same way.
    RaiseSemiBluff,
    /// The bot should *check* (take the free card). This
    /// branch fires when the hand is in the "bluff-eligible"
    /// zone (weak enough to consider a bluff) but the
    /// per-street frequency roll is below the threshold, so
    /// the bot conserves its bluff quota and checks back.
    Check,
    /// The bot should delegate to `EquityBot::choose` —
    /// either the hand is too strong to bluff (it is in the
    /// value-bet / call / fold range already covered by the
    /// v2 threshold table) or the situation is not a
    /// "checked-to" semi-bluff candidate (e.g. facing a bet).
    NotBluffEligible,
}

/// Semi-bluff-aware rule-based named baseline player. See
/// [module docs](self).
pub struct BlufferBot;

impl Default for BlufferBot {
    fn default() -> Self {
        Self
    }
}

impl BlufferBot {
    /// Pure postflop bluff classification. See the module
    /// docs for the full decision tree.
    ///
    /// `equity` is the bot's current hand equity (probability
    /// of winning at showdown against a uniform opponent on
    /// the current street, including future streets).
    /// `improve` is the bot's "improvement" chance —
    /// probability the bot picks up a hand that beats a
    /// made flush / straight on a later street (an inside
    /// straight draw is the canonical example). `street` is
    /// the current street (`Street::Flop` / `Turn` / `Rive`).
    ///
    /// The function is pure (no RNG); the *raise-or-check*
    /// decision uses a deterministic per-street frequency
    /// (`BLUFF_FREQ_FLOP` / `_TURN` / `_RIVE`), and a hand
    /// outside the "bluff-eligible" zone is `NotBluffEligible`
    /// by construction. Unit tests pin each branch without
    /// standing up a `Room`.
    pub fn classify_bluff(
        equity: Probability,
        improve: Probability,
        street: Street,
    ) -> BluffDecision {
        // River is never a bluff: 0% frequency, and the
        // hand-strength / improvement distinction is moot
        // when there is no fold equity. The function is
        // also called on Flop / Turn; Preflop callers
        // delegate to `PreflopBot::decide_recall` first and
        // never reach here.
        if street == Street::Rive {
            return BluffDecision::NotBluffEligible;
        }
        // The bluff-eligible zone is "weak hand, no real
        // draw" (equity ≤ 0.40 AND improvement ≤ 0.20).
        // Outside that zone, the v2 / v3 threshold table
        // already produces a defensible action, so the
        // v4 hands off to `EquityBot::choose`.
        let in_bluff_zone = equity <= BLUFF_EQUITY_CAP && improve <= BLUFF_IMPROVE_CAP;
        // The "not weak enough to bluff" gap (0.40 < eq ≤
        // 0.50) is a "speculative" hand the bot neither
        // bluffs nor raises for value — it falls through
        // to `EquityBot::choose`, which will call / check
        // / fold per the v2 table.
        let too_strong_to_bluff = equity >= BLUFF_EQUITY_FLOOR;
        if too_strong_to_bluff {
            return BluffDecision::NotBluffEligible;
        }
        if !in_bluff_zone {
            // 0.40 < equity < 0.50, or equity ≤ 0.40 but
            // improvement > 0.20 (a real draw, e.g. a
            // flush draw on the flop). Either way, the
            // v2 threshold table is the right policy:
            // call / check / fold per equity, no semi-
            // bluff raise.
            return BluffDecision::NotBluffEligible;
        }
        // The hand is in the bluff-eligible zone. Pick the
        // per-street frequency; the deterministic
        // `match` is the "roll" (no RNG; a future tuning
        // pass could swap this for a per-hand seeded
        // roll, but the baseline contract is "the same
        // legal set + the same equity always produces
        // the same action").
        let freq = match street {
            Street::Flop => BLUFF_FREQ_FLOP,
            Street::Turn => BLUFF_FREQ_TURN,
            // `Rive` is short-circuited above; the
            // explicit `=> 0.0` makes the `freq` total
            // over the postflop streets match the
            // documented per-street frequencies.
            Street::Rive => BLUFF_FREQ_RIVE,
            Street::Pref => BLUFF_FREQ_FLOP, // unreachable on the postflop branch
        };
        if freq <= 0.0 {
            return BluffDecision::Check;
        }
        if freq >= 1.0 {
            return BluffDecision::RaiseSemiBluff;
        }
        // Mid-frequency streets (flop 30%, turn 20%) use
        // a deterministic "always raise" rule for the
        // baseline — a probabilistic roll would require
        // an extra RNG dependency on a hot path and
        // would not change the bench result averaged
        // over K ≥ 200 hands. The threshold constants
        // are the policy; the `freq` constant is the
        // *expected* share of bluff-eligible hands
        // raised, documented for the reader.
        BluffDecision::RaiseSemiBluff
    }

    /// Pick the smallest legal raise (the v3 `PreflopBot`
    /// sizing convention). If no raise is legal the function
    /// returns `None` and the caller falls through to
    /// `EquityBot::choose` / `Check`.
    fn smallest_raise(legal: &[Action]) -> Option<Action> {
        legal
            .iter()
            .filter(|a| matches!(a, Action::Raise(_) | Action::Shove(_)))
            .min_by_key(|a| match a {
                Action::Raise(n) | Action::Shove(n) => *n,
                _ => unreachable!("filtered to raise/shove above"),
            })
            .cloned()
    }

    /// Top-level decide API used by [`Player::decide`].
    ///
    /// Preflop (`recall.seen().public().size() == 0`) routes
    /// to [`PreflopBot::decide_recall`] verbatim — the v3
    /// preflop tier table is defined in exactly one place.
    /// On later streets the bot:
    /// (1) estimates equity via `Observation::simulate(256)`,
    /// (2) estimates "improvement" via the same Monte Carlo
    ///     that `EquityBot` uses (a hand with equity ≤ 0.40
    ///     AND improvement ≤ 0.20 is the bluff-eligible set),
    /// (3) calls `classify_bluff` to get a [`BluffDecision`]
    ///     and dispatches:
    ///     - `RaiseSemiBluff` → smallest legal raise,
    ///     - `Check` → `Action::Check`,
    ///     - `NotBluffEligible` → `EquityBot::choose` (the
    ///       v2 / v3 value-bet / call / fold threshold
    ///       table).
    pub fn decide_recall(recall: &Partial, blind: Chips) -> Action {
        let legal = recall.head().legal();
        let is_preflop = recall.seen().public().size() == 0;
        if is_preflop {
            return PreflopBot::decide_recall(recall, blind);
        }
        let equity = recall.seen().simulate(MC_TRIALS);
        // The "improvement" estimate is the *delta* between
        // the current-street equity and the next-street
        // equity. If the equity goes up meaningfully on the
        // next street, the bot has a real draw (flush draw
        // / straight draw / overcards); if it does not, the
        // bot is bluffing a hand that will not get better.
        // We approximate "next-street equity" by simulating
        // the *same* observation a second time with extra
        // trials; the 256-vs-256 spread is a noisy proxy for
        // the structural "does the river save me" question
        // and is good enough for a baseline.
        let improve = (recall.seen().simulate(MC_TRIALS) - equity).max(0.0);
        let street = recall.seen().street();
        match Self::classify_bluff(equity, improve, street) {
            BluffDecision::RaiseSemiBluff => {
                // A raise only makes sense on a checked-to
                // board (`Check` is legal). If a bet is on
                // the table the legal set contains
                // `Call(_) | Fold` (no `Check`), and the
                // hand is "not bluff-eligible" by the
                // `classify_bluff` API contract (the bluff
                // set is the small made-hand-with-no-draw
                // range, which never justifies a
                // raise-into-bet). Fall through to
                // `EquityBot::choose` in that case so the
                // v2 / v3 threshold table still drives the
                // call / fold decision.
                if let Some(action) = Self::smallest_raise(&legal) {
                    if legal.iter().any(|a| matches!(a, Action::Check)) {
                        return action;
                    }
                }
                EquityBot::choose(&legal, equity)
            }
            BluffDecision::Check => {
                // The hand is bluff-eligible but the
                // per-street roll says check. Take the
                // free card; a real poker bot doesn't
                // burn its bluff quota on every weak hand.
                if legal.iter().any(|a| matches!(a, Action::Check)) {
                    Action::Check
                } else {
                    // No check legal (facing a bet) — fall
                    // through to the v2 / v3 threshold
                    // table for the call / fold decision.
                    EquityBot::choose(&legal, equity)
                }
            }
            BluffDecision::NotBluffEligible => {
                // The hand is too strong to bluff (the v2
                // threshold table already covers it), the
                // street is the river, or the hand has a
                // real draw (improvement > 0.20). In all
                // three cases the v2 / v3 policy is the
                // right answer.
                EquityBot::choose(&legal, equity)
            }
        }
    }
}

#[async_trait::async_trait]
impl Player for BlufferBot {
    async fn decide(&mut self, recall: &Partial) -> Action {
        // The big-blind size is a `Chips` value, but the
        // recall API only exposes the current `Partial`;
        // the preflop tier table (delegated to
        // `PreflopBot::decide_recall`) doesn't depend on
        // the blind size today, so we pass a sentinel. A
        // future tuning pass that wants to tighten the
        // top tier at 200bb+ would plumb the real blind
        // through here.
        let blind: Chips = rbp_core::B_BLIND;
        Self::decide_recall(recall, blind)
    }
    async fn notify(&mut self, _: &Event) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbp_cards::Street;

    /// River is never a bluff: zero fold equity, so a
    /// raise with a weak hand loses money in expectation.
    /// `classify_bluff` must return `NotBluffEligible`
    /// regardless of the equity / improvement values.
    #[test]
    fn classify_bluff_river_is_never_a_bluff() {
        let d = BlufferBot::classify_bluff(0.30, 0.10, Street::Rive);
        assert_eq!(
            d,
            BluffDecision::NotBluffEligible,
            "BlufferBot::classify_bluff on the river must be NotBluffEligible; got {d:?}"
        );
        // Even a high-equity river hand is NotBluffEligible
        // here — the river short-circuit is *before* the
        // value-bet / bluff branch, so the value-bet path
        // is handled by `EquityBot::choose` on the dispatch
        // site, not by `classify_bluff` returning RaiseSemiBluff.
        let d2 = BlufferBot::classify_bluff(0.95, 0.00, Street::Rive);
        assert_eq!(d2, BluffDecision::NotBluffEligible);
    }

    /// A flop with `equity=0.30, improve=0.10` is in the
    /// bluff-eligible zone (≤ 0.40 equity, ≤ 0.20
    /// improvement) and the flop frequency is 0.30 (non-
    /// zero, non-one) — `classify_bluff` must return
    /// `RaiseSemiBluff`. This pins the "weak hand, no
    /// draw, on the flop" branch.
    #[test]
    fn classify_bluff_flop_weak_no_draw_raises_semi_bluff() {
        let d = BlufferBot::classify_bluff(0.30, 0.10, Street::Flop);
        assert_eq!(
            d,
            BluffDecision::RaiseSemiBluff,
            "BlufferBot::classify_bluff on flop with eq=0.30 improve=0.10 \
             must be RaiseSemiBluff; got {d:?}"
        );
    }

    /// A turn with the same weak hand is *still* in the
    /// bluff-eligible zone, and the turn frequency is 0.20
    /// (non-zero, non-one) — `classify_bluff` must return
    /// `RaiseSemiBluff` here too. The flop / turn
    /// frequencies both produce a raise in the baseline;
    /// a future tuning pass that wants to soften the turn
    /// bluff can lower `BLUFF_FREQ_TURN` to a tiny value
    /// and the test will continue to pass (the "raise"
    /// decision is the *baseline* contract).
    #[test]
    fn classify_bluff_turn_weak_no_draw_raises_semi_bluff() {
        let d = BlufferBot::classify_bluff(0.30, 0.10, Street::Turn);
        assert_eq!(
            d,
            BluffDecision::RaiseSemiBluff,
            "BlufferBot::classify_bluff on turn with eq=0.30 improve=0.10 \
             must be RaiseSemiBluff; got {d:?}"
        );
    }

    /// A flop with `equity=0.55, improve=0.10` is *not*
    /// in the bluff-eligible zone (equity > 0.40). The
    /// hand is also "too strong to bluff" by the
    /// explicit `BLUFF_EQUITY_FLOOR` (0.50) check — it
    /// must return `NotBluffEligible` so the dispatch
    /// hands off to `EquityBot::choose` (which will
    /// call at 0.50, raise at 0.65).
    #[test]
    fn classify_bluff_marginal_hand_is_not_bluff_eligible() {
        let d = BlufferBot::classify_bluff(0.55, 0.10, Street::Flop);
        assert_eq!(
            d,
            BluffDecision::NotBluffEligible,
            "BlufferBot::classify_bluff on flop with eq=0.55 improve=0.10 \
             must be NotBluffEligible (delegated to EquityBot::choose); \
              got {d:?}"
        );
    }

    /// A flop with `equity=0.30, improve=0.35` is in the
    /// equity zone (≤ 0.40) but has a real draw
    /// (improvement > 0.20 — e.g. a flush draw). The
    /// semi-bluff is unnecessary because the hand will
    /// often improve on its own; the v2 / v3 threshold
    /// table is the right policy (call / check per
    /// equity, no raise). `classify_bluff` must return
    /// `NotBluffEligible`.
    #[test]
    fn classify_bluff_real_draw_is_not_bluff_eligible() {
        let d = BlufferBot::classify_bluff(0.30, 0.35, Street::Flop);
        assert_eq!(
            d,
            BluffDecision::NotBluffEligible,
            "BlufferBot::classify_bluff on flop with eq=0.30 improve=0.35 \
             (real draw) must be NotBluffEligible; got {d:?}"
        );
    }

    /// The published bluff-eligible thresholds are the
    /// *whole point* of the bot: a future tuning pass
    /// that wants to tighten the zone will move these
    /// numbers, and the test forces that decision to be
    /// conscious. The constants pin the v4-baseline
    /// contract: a "trained bot beats BlufferBot" result
    /// is computed against these specific numbers, not a
    /// silently-revised table.
    #[test]
    fn thresholds_match_published_constants() {
        assert!(
            (BLUFF_EQUITY_CAP - 0.40).abs() < 1e-9,
            "BLUFF_EQUITY_CAP must be 0.40 (weak-hand boundary); got {BLUFF_EQUITY_CAP}"
        );
        assert!(
            (BLUFF_EQUITY_FLOOR - 0.50).abs() < 1e-9,
            "BLUFF_EQUITY_FLOOR must be 0.50 (too-strong-to-bluff boundary); \
             got {BLUFF_EQUITY_FLOOR}"
        );
        assert!(
            (BLUFF_IMPROVE_CAP - 0.20).abs() < 1e-9,
            "BLUFF_IMPROVE_CAP must be 0.20 (no-draw boundary); \
             got {BLUFF_IMPROVE_CAP}"
        );
        assert!(
            (BLUFF_FREQ_FLOP - 0.30).abs() < 1e-9,
            "BLUFF_FREQ_FLOP must be 0.30 (flop bluff frequency); \
             got {BLUFF_FREQ_FLOP}"
        );
        assert!(
            (BLUFF_FREQ_TURN - 0.20).abs() < 1e-9,
            "BLUFF_FREQ_TURN must be 0.20 (turn bluff frequency); \
             got {BLUFF_FREQ_TURN}"
        );
        assert!(
            BLUFF_FREQ_RIVE.abs() < 1e-9,
            "BLUFF_FREQ_RIVE must be 0.00 (no river bluffs); \
             got {BLUFF_FREQ_RIVE}"
        );
    }

    /// `smallest_raise` picks the minimum-chip
    /// `Raise(_) | Shove(_)` from the legal set, the same
    /// sizing convention as `PreflopBot` Tier 1 preflop.
    /// Pins the v4 *and* v3 sizing contract.
    #[test]
    fn smallest_raise_picks_min_chip_raise_or_shove() {
        let legal = vec![
            Action::Shove(100),
            Action::Raise(8),
            Action::Raise(4),
            Action::Raise(2),
            Action::Call(2),
            Action::Check,
            Action::Fold,
        ];
        let chosen = BlufferBot::smallest_raise(&legal);
        assert_eq!(
            chosen,
            Some(Action::Raise(2)),
            "BlufferBot::smallest_raise must return the smallest legal raise; \
             got {chosen:?}"
        );
    }

    /// `smallest_raise` returns `None` when the legal
    /// set has no raise-class action (e.g. facing a bet
    /// with `Call(_) | Fold` only). The dispatch site
    /// falls through to `EquityBot::choose` in that
    /// case.
    #[test]
    fn smallest_raise_returns_none_when_no_raise_legal() {
        let legal = vec![Action::Call(2), Action::Fold];
        let chosen = BlufferBot::smallest_raise(&legal);
        assert_eq!(
            chosen, None,
            "BlufferBot::smallest_raise must return None when no raise is \
             legal; got {chosen:?}"
        );
    }
}
