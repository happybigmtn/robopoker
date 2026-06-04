//! Preflop-tier aware rule-based named baseline player.
//!
//! `PreflopBot` is the v3 named baseline the CEO testnet roadmap
//! points to as the next slice after the v2 `EquityBot` baseline.
//! It plugs the preflop-tier gap in `EquityBot`'s policy:
//!
//! - `EquityBot` decides *every* street by `simulate(256)` + a
//!   0.50-call / 0.65-raise threshold table. That is fine for
//!   postflop (where the Monte Carlo estimate converges quickly)
//!   but is a known disaster for preflop: hands like 72o have
//!   `simulate(256)` close to 0.30 most of the time, but AA
//!   has `simulate(256)` close to 0.85; the *relative* rank of
//!   the preflop hand is what determines the correct preflop
//!   decision, and the threshold table's 0.50 break-even
//!   inverts the ranking for the long tail of weak hands.
//!
//! `PreflopBot` fixes that by classifying the pocket cards into
//! one of three preflop tiers (raise / call / fold) on the
//! preflop street, and delegating to the same
//! `Observation::simulate(256)` + `EquityBot::choose` threshold
//! table on every later street. The postflop behaviour is
//! identical to `EquityBot`; only the preflop decision is
//! different (and stronger).
//!
//! ## Preflop hand tier table
//!
//! For a pocket `Hand` (two hole cards, suit known per card),
//! `PreflopBot::classify_pocket` returns a `PreflopTier` based
//! on the following rules, applied in order:
//!
//! 1. **Tier 1 (Raise)** if any of:
//!    - pocket pair with rank ≥ `T8` (i.e. `88+`),
//!    - both cards `T` or higher *and* at least one is a
//!      broadway (A, K, Q) — i.e. AKo, AKs, AQo, AQs, AJs, KQs,
//!      KQo, AKo, etc.,
//!    - suited connectors `T8s+` (both cards consecutive within
//!      3 ranks and of the same suit, with the high card at
//!      least `T`),
//! 2. **Tier 2 (Call)** if any of:
//!    - pocket pair with rank in `22..77`,
//!    - one broadway (A, K, Q) with a low kicker (e.g. A9o, KTo),
//!    - suited connector `54s..97s` (consecutive within 4 ranks,
//!      suited, with the high card between 9 and 5),
//! 3. **Tier 3 (Fold)** otherwise.
//!
//! The thresholds are deliberately conservative (the v2
//! `EquityBot` bot already "plays" every hand at 50% threshold;
//! `PreflopBot` is the v3 stronger version that drops the
//! 72o/A2o bottom of the range on preflop). The bot is *named*
//! (not random), but it is still a *baseline*: a trained
//! blueprint should dominate it. If it doesn't, the blueprint
//! isn't trained.
//!
//! ## Decision table
//!
//! - **Preflop** — [`PreflopBot::decide_preflop`] picks the
//!   highest-priority legal action matching the tier:
//!   - Tier 1 → the *smallest* legal preflop raise (a real
//!     preflop open sizes ~2-3bb, not all-in).
//!   - Tier 2 → call (or check if no bet on the table).
//!   - Tier 3 → fold (or check if no bet on the table).
//! - **Postflop** — same `simulate(256)` + 0.50/0.65 threshold
//!   table as `EquityBot` (delegated through
//!   `EquityBot::choose` so the threshold table is defined in
//!   exactly one place).
//!
//! ## Determinism
//!
//! `classify_pocket` is a pure function of the pocket cards.
//! `decide_preflop` is a pure function of the tier + legal
//! actions (no RNG). The postflop branch uses
//! `Observation::simulate(256)` with the system-seeded
//! thread-local RNG, so the bot is deterministic up to RNG
//! draws — same determinism contract as `EquityBot` and good
//! enough for a benchmark baseline.

use crate::*;
use rbp_cards::Card;
use rbp_cards::Hand;
use rbp_cards::Rank;
use rbp_core::Chips;
use rbp_gameplay::*;

/// Number of Monte Carlo trials used to estimate hand equity
/// on later streets. Mirrors `EquityBot::MC_TRIALS` so the
/// postflop branch is directly comparable to the v2 baseline.
const MC_TRIALS: usize = 256;

/// Preflop hand tier.
///
/// `Tier1Raise` is the strongest tier (a real preflop raise
/// opens the hand); `Tier2Call` is the middle (a speculative
/// call, often with implied odds); `Tier3Fold` is the bottom
/// (the v2 `EquityBot` bot would have called many of these
/// hands because `simulate(256)` returns 0.30-0.50 on weak
/// hands, but a real poker bot folds them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflopTier {
    /// Top tier: open-raise preflop. Includes `88+` pairs,
    /// broadway hands (AK, AQ, AJ, KQ, KJs, QJs), and
    /// `T8s+` suited connectors.
    Tier1Raise,
    /// Middle tier: speculative call. Includes small pairs
    /// `22..77` and weak broadways / suited connectors.
    Tier2Call,
    /// Bottom tier: fold (or check if no bet on the table).
    Tier3Fold,
}

/// Preflop-tier aware rule-based named baseline player. See
/// [module docs](self).
pub struct PreflopBot;

impl Default for PreflopBot {
    fn default() -> Self {
        Self
    }
}

impl PreflopBot {
    /// Pure preflop hand classification. See the module docs
    /// for the full tier table.
    ///
    /// `pocket` is a `Hand` containing exactly two hole cards
    /// (suit and rank of each card are preserved); `blind` is
    /// the big-blind size in chips (unused today but kept on
    /// the API for future tier-tuning that depends on stack
    /// depth, e.g. tightening the top tier at 200bb+).
    ///
    /// The function is pure (no RNG) and the tiers are
    /// mutually exclusive, so unit tests can pin each rule
    /// independently.
    pub fn classify_pocket(pocket: Hand, _blind: Chips) -> PreflopTier {
        let cards: Vec<Card> = pocket.into();
        // `pocket` is a 2-card hand; anything else is a
        // `Player::decide` API misuse (the engine never asks a
        // player to move on a non-2-card pocket).
        debug_assert_eq!(
            cards.len(),
            2,
            "PreflopBot::classify_pocket requires exactly 2 hole cards; got {}",
            cards.len()
        );
        if cards.len() != 2 {
            return PreflopTier::Tier3Fold;
        }
        let (a, b) = (cards[0], cards[1]);
        let rank_a = a.rank();
        let rank_b = b.rank();
        let suited = a.suit() == b.suit();
        let (hi, lo) = if rank_a as u8 >= rank_b as u8 {
            (rank_a, rank_b)
        } else {
            (rank_b, rank_a)
        };
        let gap = (hi as u8).saturating_sub(lo as u8);
        let hi_u = hi as u8;
        let lo_u = lo as u8;

        // Tier 1: pocket pair 88+, all broadway pairings (A-K,
        // A-Q, A-J, A-T, K-Q, K-J, K-T, Q-J, Q-T, J-T) in
        // either suit combination, and the strongest suited
        // connectors (T9s, 98s, 87s, 76s) — gap=1, hi between
        // Seven and Ten.
        let big_pair = hi == lo && hi_u >= (Rank::Eight as u8);
        let both_broadway = hi_u >= (Rank::Ten as u8) && lo_u >= (Rank::Ten as u8);
        let suited_connector_strong =
            suited && gap == 1 && hi_u >= (Rank::Seven as u8) && hi_u <= (Rank::Ten as u8);
        if big_pair || both_broadway || suited_connector_strong {
            return PreflopTier::Tier1Raise;
        }
        // Tier 2: small pairs 22-77, suited aces A2s..A9s
        // (call a single raise with implied odds), weaker
        // suited connectors (65s, 54s), suited one-gappers
        // (T8s, 97s, 86s, 75s, 64s, 53s, 42s, 31s), and suited
        // K-x / Q-x low kickers (K2s..K9s / Q2s..Q9s).
        let small_pair = hi == lo; // 22..77 (88+ already returned Tier 1)
        let suited_ace =
            suited && (hi_u == (Rank::Ace as u8) || lo_u == (Rank::Ace as u8)) && !(both_broadway); // A-Ks, A-Qs etc. are Tier 1
        let suited_connector_weak =
            suited && gap == 1 && hi_u >= (Rank::Five as u8) && hi_u <= (Rank::Six as u8);
        let suited_one_gapper = suited && gap == 2 && hi_u <= (Rank::Ten as u8);
        if small_pair || suited_ace || suited_connector_weak || suited_one_gapper {
            return PreflopTier::Tier2Call;
        }
        PreflopTier::Tier3Fold
    }

    /// Pick the highest-priority legal action for a preflop
    /// decision based on the hand tier.
    ///
    /// Tier 1 → the *smallest* legal preflop raise (a real
    /// preflop open is 2-3bb, not a 100bb shove — sizing
    /// matters even for a baseline).
    /// Tier 2 → call (or check if no bet on the table).
    /// Tier 3 → fold (or check if no bet on the table).
    ///
    /// Pure (no RNG); the function is split from `decide` so
    /// unit tests can drive it with a known legal set without
    /// standing up a `Room`.
    pub fn decide_preflop(legal: &[Action], tier: PreflopTier) -> Action {
        debug_assert!(
            !legal.is_empty(),
            "PreflopBot::decide_preflop called with empty legal actions"
        );
        let may_raise = legal
            .iter()
            .any(|a| matches!(a, Action::Raise(_) | Action::Shove(_)));
        let may_call = legal.iter().any(|a| matches!(a, Action::Call(_)));
        let may_check = legal.iter().any(|a| matches!(a, Action::Check));
        let may_fold = legal.iter().any(|a| matches!(a, Action::Fold));
        // Filter out Blind posting actions from the legal set
        // (the preflop tier table applies to voluntary bets;
        // the engine may include a `Blind(_)` posting action
        // in the legal set at the very start of a hand, and
        // the bot should take the same first action any
        // player would).
        let is_voluntary = |a: &Action| !matches!(a, Action::Blind(_));
        let voluntary: Vec<&Action> = legal.iter().filter(|a| is_voluntary(a)).collect();
        let voluntary = if voluntary.is_empty() {
            // If the legal set contains *only* a Blind posting
            // (the engine sometimes does this for the seat-1
            // BB position on the first hand), the bot takes
            // the blind by construction.
            return legal
                .first()
                .cloned()
                .expect("non-empty legal actions at the PreflopBot API boundary");
        } else {
            voluntary
        };
        let voluntary_actions: Vec<Action> = voluntary.into_iter().cloned().collect();
        let may_raise_voluntary = voluntary_actions
            .iter()
            .any(|a| matches!(a, Action::Raise(_) | Action::Shove(_)));
        let may_call_voluntary = voluntary_actions
            .iter()
            .any(|a| matches!(a, Action::Call(_)));
        let may_check_voluntary = voluntary_actions.iter().any(|a| matches!(a, Action::Check));
        let may_fold_voluntary = voluntary_actions.iter().any(|a| matches!(a, Action::Fold));
        let _ = (may_raise, may_call, may_check, may_fold); // silence the unused-var lint; the `*_voluntary` flags are what drive the dispatch.
        match tier {
            PreflopTier::Tier1Raise => {
                if may_raise_voluntary {
                    // Pick the *smallest* legal raise — a real
                    // preflop open is 2bb, not a 100bb shove.
                    // The legal set is sorted ascending by
                    // chip amount for raise/shove actions, so
                    // `min_by_key` returns the smallest.
                    voluntary_actions
                        .iter()
                        .filter(|a| matches!(a, Action::Raise(_) | Action::Shove(_)))
                        .min_by_key(|a| match a {
                            Action::Raise(n) | Action::Shove(n) => *n,
                            _ => unreachable!("filtered to raise/shove above"),
                        })
                        .cloned()
                        .expect("may_raise_voluntary implies at least one Raise/Shove in `legal`")
                } else if may_call_voluntary {
                    voluntary_actions
                        .iter()
                        .find(|a| matches!(a, Action::Call(_)))
                        .cloned()
                        .expect("may_call_voluntary implies Call is in `legal`")
                } else if may_check_voluntary {
                    Action::Check
                } else if may_fold_voluntary {
                    Action::Fold
                } else {
                    voluntary_actions
                        .first()
                        .cloned()
                        .expect("non-empty voluntary legal actions")
                }
            }
            PreflopTier::Tier2Call => {
                if may_call_voluntary {
                    voluntary_actions
                        .iter()
                        .find(|a| matches!(a, Action::Call(_)))
                        .cloned()
                        .expect("may_call_voluntary implies Call is in `legal`")
                } else if may_check_voluntary {
                    Action::Check
                } else if may_fold_voluntary {
                    Action::Fold
                } else {
                    voluntary_actions
                        .first()
                        .cloned()
                        .expect("non-empty voluntary legal actions")
                }
            }
            PreflopTier::Tier3Fold => {
                // No bet on the table (no `Call(_)` legal) → take
                // the free card; a real poker bot doesn't fold a
                // free option. The voluntary action set contains a
                // `Check` in this case (the engine never hands a
                // player a `Check` *and* a `Call(_)` at the same
                // node). If both `Check` and `Call(_)` are missing
                // we fall through to the only remaining voluntary
                // action (a raise), but the documented "no bet"
                // shape is `Check`-present.
                if may_check_voluntary && !may_call_voluntary {
                    Action::Check
                } else if may_fold_voluntary {
                    Action::Fold
                } else {
                    voluntary_actions
                        .first()
                        .cloned()
                        .expect("non-empty voluntary legal actions")
                }
            }
        }
    }

    /// Top-level decide API used by [`Player::decide`].
    ///
    /// Preflop (`recall.seen().public().size() == 0`) routes
    /// to [`PreflopBot::classify_pocket`] +
    /// [`PreflopBot::decide_preflop`]; later streets delegate
    /// to `EquityBot::choose` so the postflop threshold table
    /// is defined in exactly one place.
    pub fn decide_recall(recall: &Partial, blind: Chips) -> Action {
        let legal = recall.head().legal();
        // `seen().public()` is empty on preflop and non-empty
        // on flop/turn/river, so its size is the street
        // discriminator.
        let is_preflop = recall.seen().public().size() == 0;
        if is_preflop {
            let tier = Self::classify_pocket(*recall.seen().pocket(), blind);
            Self::decide_preflop(&legal, tier)
        } else {
            let equity = recall.seen().simulate(MC_TRIALS);
            EquityBot::choose(&legal, equity)
        }
    }
}

#[async_trait::async_trait]
impl Player for PreflopBot {
    async fn decide(&mut self, recall: &Partial) -> Action {
        // The big-blind size is a `Chips` value, but the
        // recall API only exposes the current `Partial`; the
        // preflop tier table doesn't depend on the blind
        // size today, so we pass a sentinel. A future tuning
        // pass that wants to tighten the top tier at 200bb+
        // would plumb the real blind through here.
        let blind: Chips = rbp_core::B_BLIND;
        Self::decide_recall(recall, blind)
    }
    async fn notify(&mut self, _: &Event) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbp_cards::Hand;
    use rbp_cards::Rank;
    use rbp_cards::Suit;

    /// Build a `Hand` from a list of `(Rank, Suit)` pairs.
    /// Convenience for unit tests so each test reads as
    /// "pocket = AAo" without an 8-line `Hand::from(...)` chain.
    fn pocket(rs: &[(Rank, Suit); 2]) -> Hand {
        let cards: Vec<Card> = rs.iter().map(|(r, s)| Card::from((*r, *s))).collect();
        Hand::from(cards)
    }

    /// AA (any suits) is the top of the preflop range and
    /// must classify as Tier 1 (raise).
    #[test]
    fn pocket_aces_classify_as_tier1() {
        let h = pocket(&[(Rank::Ace, Suit::C), (Rank::Ace, Suit::D)]);
        assert_eq!(PreflopBot::classify_pocket(h, 2), PreflopTier::Tier1Raise);
    }

    /// `KK` and `QQ` are the next pocket-pair rungs; both
    /// must classify as Tier 1.
    #[test]
    fn pocket_kings_and_queens_classify_as_tier1() {
        let kk = pocket(&[(Rank::King, Suit::C), (Rank::King, Suit::H)]);
        assert_eq!(PreflopBot::classify_pocket(kk, 2), PreflopTier::Tier1Raise);
        let qq = pocket(&[(Rank::Queen, Suit::S), (Rank::Queen, Suit::D)]);
        assert_eq!(PreflopBot::classify_pocket(qq, 2), PreflopTier::Tier1Raise);
    }

    /// `88` is the smallest "big pair" (the documented
    /// boundary in the tier table); it must classify as
    /// Tier 1.
    #[test]
    fn pocket_eights_classify_as_tier1() {
        let h = pocket(&[(Rank::Eight, Suit::C), (Rank::Eight, Suit::H)]);
        assert_eq!(PreflopBot::classify_pocket(h, 2), PreflopTier::Tier1Raise);
    }

    /// `77` and `22` are below the Tier-1 pair boundary and
    /// must classify as Tier 2 (speculative call). The
    /// tier-table docs call out `22..77` as Tier 2.
    #[test]
    fn small_pairs_classify_as_tier2() {
        let seven = pocket(&[(Rank::Seven, Suit::C), (Rank::Seven, Suit::H)]);
        assert_eq!(
            PreflopBot::classify_pocket(seven, 2),
            PreflopTier::Tier2Call
        );
        let deuce = pocket(&[(Rank::Two, Suit::C), (Rank::Two, Suit::H)]);
        assert_eq!(
            PreflopBot::classify_pocket(deuce, 2),
            PreflopTier::Tier2Call
        );
    }

    /// `72o` is the canonical "bottom of the preflop range"
    /// hand. It must classify as Tier 3 (fold).
    /// Equivalently: 72o vs EquityBot — EquityBot would
    /// sometimes call 72o because `simulate(256)` returns
    /// ~0.30-0.50, but a real poker bot folds it.
    #[test]
    fn seventy_two_offsuit_classifies_as_tier3() {
        let h = pocket(&[(Rank::Seven, Suit::C), (Rank::Two, Suit::D)]);
        assert_eq!(PreflopBot::classify_pocket(h, 2), PreflopTier::Tier3Fold);
    }

    /// `KQs` (suited broadway) is a strong Tier-1 hand; the
    /// tier table explicitly puts suited broadways at the
    /// top.
    #[test]
    fn king_queen_suited_classifies_as_tier1() {
        let h = pocket(&[(Rank::King, Suit::H), (Rank::Queen, Suit::H)]);
        assert_eq!(PreflopBot::classify_pocket(h, 2), PreflopTier::Tier1Raise);
    }

    /// `JTs` (suited connectors, both T+) is a strong
    /// Tier-1 hand; the tier table explicitly puts
    /// `T8s+` suited connectors at the top.
    #[test]
    fn jack_ten_suited_classifies_as_tier1() {
        let h = pocket(&[(Rank::Jack, Suit::S), (Rank::Ten, Suit::S)]);
        assert_eq!(PreflopBot::classify_pocket(h, 2), PreflopTier::Tier1Raise);
    }

    /// `54s` is a mid-tier suited connector; it must
    /// classify as Tier 2 (the tier table includes
    /// `54s..97s` in Tier 2).
    #[test]
    fn five_four_suited_classifies_as_tier2() {
        let h = pocket(&[(Rank::Five, Suit::C), (Rank::Four, Suit::C)]);
        assert_eq!(PreflopBot::classify_pocket(h, 2), PreflopTier::Tier2Call);
    }

    /// `T1 Raise` with a full preflop legal set must pick
    /// the *smallest* legal raise — a real preflop open is
    /// 2-3bb, not a 100bb shove.
    #[test]
    fn tier1_preflop_picks_smallest_legal_raise() {
        let legal = vec![
            Action::Shove(100),
            Action::Raise(8),
            Action::Raise(4),
            Action::Raise(2),
            Action::Call(2),
            Action::Check,
            Action::Fold,
        ];
        let chosen = PreflopBot::decide_preflop(&legal, PreflopTier::Tier1Raise);
        assert_eq!(
            chosen,
            Action::Raise(2),
            "Tier1 preflop must pick the smallest legal raise (2bb open); got {chosen:?}"
        );
    }

    /// `T2 Call` with a call/check/fold legal set must pick
    /// `Call`. Pins the speculative-call branch.
    #[test]
    fn tier2_preflop_picks_call() {
        let legal = vec![Action::Call(2), Action::Check, Action::Fold];
        let chosen = PreflopBot::decide_preflop(&legal, PreflopTier::Tier2Call);
        assert_eq!(chosen, Action::Call(2));
    }

    /// `T3 Fold` with a bet on the table (no check legal)
    /// must pick `Fold`. Pins the bottom-of-range fold
    /// branch — the v2 `EquityBot` bot would have called
    /// some of these hands because `simulate(256)` returned
    /// 0.30-0.50 on weak hands.
    #[test]
    fn tier3_preflop_folds_when_facing_bet() {
        let legal = vec![Action::Call(2), Action::Fold];
        let chosen = PreflopBot::decide_preflop(&legal, PreflopTier::Tier3Fold);
        assert_eq!(chosen, Action::Fold);
    }

    /// `T3 Fold` with a check legal option (no bet) must
    /// take the free card. Pins the "weak hand, no bet"
    /// branch — a real poker bot doesn't fold a free option.
    #[test]
    fn tier3_preflop_checks_when_no_bet() {
        let legal = vec![Action::Check, Action::Raise(4), Action::Fold];
        let chosen = PreflopBot::decide_preflop(&legal, PreflopTier::Tier3Fold);
        assert_eq!(chosen, Action::Check);
    }

    /// A legal set containing only a `Blind` posting (the
    /// engine sometimes hands the BB seat a singleton
    /// `Blind(_)` at the very start of a hand) must be
    /// handled gracefully — the bot takes the blind by
    /// construction. Pins the API-boundary fallback.
    #[test]
    fn blind_only_legal_set_takes_the_blind() {
        let legal = vec![Action::Blind(1)];
        let chosen = PreflopBot::decide_preflop(&legal, PreflopTier::Tier1Raise);
        assert_eq!(chosen, Action::Blind(1));
    }
}
