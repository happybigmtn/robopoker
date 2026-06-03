//! Discounted CFR (DCFR) regret update strategy.
//!
//! Implements the asymmetric discounting scheme of Brown & Sandholm (2019),
//! "Solving Imperfect-Information Games via Discounted Regret Minimization".
//!
//! Positive regrets are decayed with exponent α and negative regrets with
//! exponent β < α, so negative history is forgotten faster than positive
//! history. This is what distinguishes DCFR from vanilla CFR and from CFR+.

use super::*;

/// Discounted CFR (DCFR) regret update strategy.
///
/// Asymmetric discounting per Brown & Sandholm (2019):
/// - Positive regrets are scaled by `T^α / (T^α + 1)` (α = 1.5)
/// - Negative regrets are scaled by `T^β / (T^β + 1)` (β = 0.5)
///
/// Both are then summed with the immediate gain, floored at [`REGRET_MIN`].
/// Because β < α, negative regret decays faster than positive, which is the
/// key property that gives DCFR its convergence speed-up over vanilla CFR.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiscountedRegret;

impl DiscountedRegret {
    /// Exponent for the positive-regret discount factor `δᵃ(T) = T^α / (T^α + 1)`.
    pub const ALPHA: f32 = 1.5;
    /// Exponent for the negative-regret discount factor `δᵇ(T) = T^β / (T^β + 1)`.
    /// Smaller than ALPHA so negative history is forgotten faster.
    pub const BETA: f32 = 0.5;

    /// Discount factor `T^k / (T^k + 1)` for a given exponent `k` and epoch `T`.
    /// `T` is taken as `f32`; for very large epochs it saturates to 1.0,
    /// which is the correct asymptotic behaviour.
    #[inline]
    fn discount(t: f32, k: f32) -> f32 {
        let x = t.powf(k);
        x / (x + 1.0)
    }
}

impl RegretSchedule for DiscountedRegret {
    fn gain(accumulated: Utility, immediate: Utility, epoch: usize) -> Utility {
        let t = epoch as f32;
        let updated = accumulated + immediate;
        let discount = if updated >= 0.0 {
            Self::discount(t, Self::ALPHA)
        } else {
            Self::discount(t, Self::BETA)
        };
        (updated * discount).max(REGRET_MIN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The discount factor must approach 1.0 as T → ∞ so old regrets aren't
    /// crushed to zero and recent updates dominate. α saturates faster than
    /// β (1.5 vs 0.5 exponent), so the ALPHA bound kicks in at a smaller T.
    #[test]
    fn discount_saturates_to_one() {
        assert!(DiscountedRegret::discount(1.0e6, DiscountedRegret::ALPHA) > 0.999_999);
        assert!(DiscountedRegret::discount(1.0e18, DiscountedRegret::BETA) > 0.999_999);
    }

    /// At T = 0 the discount factor is 0/(0+1) = 0, so the first epoch's
    /// accumulated regret is the immediate gain itself.
    #[test]
    fn discount_zero_at_epoch_zero() {
        assert_eq!(DiscountedRegret::discount(0.0, DiscountedRegret::ALPHA), 0.0);
        assert_eq!(DiscountedRegret::discount(0.0, DiscountedRegret::BETA), 0.0);
        assert_eq!(DiscountedRegret::gain(0.0, 2.0, 0), 0.0);
    }

    /// For a given epoch ≥ 2, the negative-regret discount must be strictly
    /// smaller than the positive-regret discount (the defining property of
    /// DCFR — β < α). At T=1 the two are equal (both = 0.5) because the
    /// exponents both evaluate to 1; we skip that edge case.
    #[test]
    fn negative_discount_strictly_smaller() {
        for epoch in [2usize, 10, 100, 1_000, 10_000] {
            let t = epoch as f32;
            let d_pos = DiscountedRegret::discount(t, DiscountedRegret::ALPHA);
            let d_neg = DiscountedRegret::discount(t, DiscountedRegret::BETA);
            assert!(
                d_neg < d_pos,
                "epoch {epoch}: δβ={d_neg} should be < δα={d_pos}"
            );
        }
    }

    /// A purely positive regret: the gain is the discounted (accumulated + immediate).
    /// At T=1, δα = 1^1.5 / (1^1.5 + 1) = 1/2, so the gain is half the sum.
    #[test]
    fn positive_regret_uses_alpha() {
        let g = DiscountedRegret::gain(2.0, 4.0, 1);
        let expected = (2.0 + 4.0) * 0.5;
        assert!((g - expected).abs() < 1e-6, "got {g}, expected {expected}");
    }

    /// A purely negative regret: the gain uses β instead of α. At T=1,
    /// δβ = 1^0.5 / (1^0.5 + 1) = 1/2, but we want to verify the *path* is β,
    /// not just the value. At T=3 the two paths diverge: δα = 3^1.5 / (3^1.5+1)
    /// ≈ 0.838, δβ = 3^0.5 / (3^0.5+1) ≈ 0.634.
    #[test]
    fn negative_regret_uses_beta() {
        let t = 3.0_f32;
        let d_alpha = t.powf(1.5) / (t.powf(1.5) + 1.0);
        let d_beta = t.powf(0.5) / (t.powf(0.5) + 1.0);
        let sum = -2.0 + -4.0;
        let g = DiscountedRegret::gain(-2.0, -4.0, 3);
        let expected = sum * d_beta;
        assert!((g - expected).abs() < 1e-5, "got {g}, expected {expected}");
        // sanity: positive branch must use the larger α discount at the same T
        let g_pos = DiscountedRegret::gain(2.0, 4.0, 3);
        let expected_pos = (2.0 + 4.0) * d_alpha;
        assert!((g_pos - expected_pos).abs() < 1e-5, "got {g_pos}, expected {expected_pos}");
        assert!(g.abs() < g_pos.abs(), "β-discount must shrink negative regret more than α shrinks positive");
    }

    /// The floor REGRET_MIN is a *lower* bound: a discounted regret may lift
    /// the stored value above the floor (less negative), but a hypothetical
    /// super-discounted update that would push it below REGRET_MIN must be
    /// clamped. Discount ∈ (0, 1) always *reduces magnitude*, so for normal
    /// inputs the floor is never binding; the test instead verifies the floor
    /// is exactly the value returned when the discount multiplier would be
    /// negative (which is impossible for the positive discount factors we use,
    /// so we verify the contract by checking the output respects the bound
    /// across a range of extremes).
    #[test]
    fn floor_is_a_lower_bound() {
        for (acc, imm, epoch) in [
            (REGRET_MIN, 0.0, 1usize),
            (REGRET_MIN, 0.0, 100),
            (REGRET_MIN, 0.0, 1_000_000),
            (0.0, REGRET_MIN, 100),
            (-1.0e7, 0.0, 100),
        ] {
            let g = DiscountedRegret::gain(acc, imm, epoch);
            assert!(
                g >= REGRET_MIN,
                "gain({acc}, {imm}, {epoch}) = {g} < REGRET_MIN = {REGRET_MIN}"
            );
        }
    }

    /// The original "PERIOD = 1" bug: the old `gain` returned
    /// `accumulated + immediate` for `epoch % 1 != 0`, but `epoch % 1` is
    /// always 0, so that branch was dead code. The new implementation must
    /// always apply a non-trivial discount (except at T=0, which is the
    /// trivial zero-discount fixed point).
    #[test]
    fn discount_is_applied_at_every_epoch() {
        for epoch in 1usize..=16 {
            let g = DiscountedRegret::gain(1.0, 0.0, epoch);
            // The accumulated value must shrink for the first epochs and only
            // recover to ≈1.0 in the asymptotic limit.
            assert!(
                g < 1.0,
                "epoch {epoch}: gain {g} should be < 1.0 (discount must be applied)"
            );
        }
    }
}
