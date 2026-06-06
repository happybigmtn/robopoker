//! Blueprint integrity gate (slice 4).
//!
//! Computes per-position opening and 3-bet frequencies from a trained
//! blueprint and asserts basic poker-sanity invariants:
//!
//! 1. Early position (UTG / SB / seat 0) opens TIGHTER than late
//!    position (BTN / BB / seat 1).
//! 2. Preflop 3-bet frequency falls within [5%, 15%].
//!
//! A seat-collapsed blueprint (position ignored, identical strategy for
//! both seats) fails invariant 1.  A degenerate blueprint that never
//! 3-bets or 3-bets too aggressively fails invariant 2.
//!
//! The gate is invoked inside every `Trainer::sync` implementation
//! (`FastSession`, `Fast2Session`, `Fast3Session`) so a bad run aborts
//! before the corrupted profile is persisted to the database.  A
//! standalone `--integrity` CLI mode lets CI verify an on-disk
//! blueprint without running a full training loop.

use rbp_cards::Street;
use rbp_gameplay::Edge;
use rbp_nlhe::NlheProfile;

/// Report produced by a successful integrity check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntegrityReport {
    /// Per-position open frequency (aggregate weight ratio).
    /// Index 0 = early (SB/UTG), index 1 = late (BB/BTN).
    pub open_freq: [f32; 2],
    /// Per-position 3-bet frequency (aggregate weight ratio).
    pub threebet_freq: [f32; 2],
}

/// Failure modes for the integrity gate.
#[derive(Debug, Clone, PartialEq)]
pub enum IntegrityError {
    /// Early position does not open tighter than late position.
    SeatCollapse { early: f32, late: f32 },
    /// Aggregate 3-bet frequency is outside the [5%, 15%] band.
    ThreeBetRange { value: f32 },
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SeatCollapse { early, late } => {
                write!(
                    f,
                    "seat collapse detected: early position opens {:.1}% not tighter than late {:.1}%",
                    early * 100.0,
                    late * 100.0
                )
            }
            Self::ThreeBetRange { value } => {
                write!(
                    f,
                    "3-bet frequency {:.1}% outside allowed range [5%, 15%]",
                    value * 100.0
                )
            }
        }
    }
}

impl std::error::Error for IntegrityError {}

/// Minimum 3-bet frequency (inclusive).
const THREE_BET_MIN: f32 = 0.05;
/// Maximum 3-bet frequency (inclusive).
const THREE_BET_MAX: f32 = 0.15;

/// Run the integrity gate against an in-memory blueprint.
///
/// Returns [`IntegrityError::SeatCollapse`] when the early-position open
/// frequency is not strictly less than the late-position open frequency.
/// Returns [`IntegrityError::ThreeBetRange`] when the aggregate 3-bet
/// frequency (across both positions) is outside `[5%, 15%]`.
pub fn check_integrity(profile: &NlheProfile) -> Result<IntegrityReport, IntegrityError> {
    let mut open_total = [0.0f32; 2];
    let mut open_aggro = [0.0f32; 2];
    let mut threebet_total = [0.0f32; 2];
    let mut threebet_aggro = [0.0f32; 2];

    for (info, edges) in &profile.encounters {
        if info.street() != Street::Pref {
            continue;
        }
        let pos = info.position();
        if pos > 1 {
            continue; // heads-up only
        }

        let total_weight: f32 = edges.values().map(|e| e.weight).sum();
        if total_weight <= 0.0 {
            continue;
        }

        let aggro_weight: f32 = edges
            .iter()
            .filter(|(e, _)| Edge::from(**e).is_aggro())
            .map(|(_, enc)| enc.weight)
            .sum();

        let aggression = info.subgame().aggression();
        if aggression == 0 {
            open_total[pos] += total_weight;
            open_aggro[pos] += aggro_weight;
        } else if aggression == 1 {
            threebet_total[pos] += total_weight;
            threebet_aggro[pos] += aggro_weight;
        }
    }

    let open_freq = [
        if open_total[0] > 0.0 {
            open_aggro[0] / open_total[0]
        } else {
            0.0
        },
        if open_total[1] > 0.0 {
            open_aggro[1] / open_total[1]
        } else {
            0.0
        },
    ];

    let threebet_freq = [
        if threebet_total[0] > 0.0 {
            threebet_aggro[0] / threebet_total[0]
        } else {
            0.0
        },
        if threebet_total[1] > 0.0 {
            threebet_aggro[1] / threebet_total[1]
        } else {
            0.0
        },
    ];

    let early = open_freq[0];
    let late = open_freq[1];

    if early >= late {
        return Err(IntegrityError::SeatCollapse { early, late });
    }

    let threebet_overall = if threebet_total[0] + threebet_total[1] > 0.0 {
        (threebet_aggro[0] + threebet_aggro[1]) / (threebet_total[0] + threebet_total[1])
    } else {
        0.0
    };

    if threebet_overall < THREE_BET_MIN || threebet_overall > THREE_BET_MAX {
        return Err(IntegrityError::ThreeBetRange {
            value: threebet_overall,
        });
    }

    Ok(IntegrityReport {
        open_freq,
        threebet_freq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbp_cards::Street;
    use rbp_gameplay::{Abstraction, Edge, Path};
    use rbp_mccfr::Encounter;
    use rbp_nlhe::{NlheEdge, NlheInfo, NlheProfile, NlheSecret};
    use std::collections::BTreeMap;

    /// Build a single info-action entry with the given weights.
    fn info_with_weights(
        subgame: Path,
        position: usize,
        weights: &[(Edge, f32)],
    ) -> (NlheInfo, BTreeMap<NlheEdge, Encounter>) {
        let bucket = NlheSecret::from(Abstraction::from(Street::Pref));
        let choices: Path = weights.iter().map(|(e, _)| *e).collect();
        let info = NlheInfo::from((subgame, bucket.into(), choices, position));
        let edges: BTreeMap<NlheEdge, Encounter> = weights
            .iter()
            .map(|(e, w)| (NlheEdge::from(*e), Encounter::new(*w, 0.0, 0.0, 1)))
            .collect();
        (info, edges)
    }

    #[test]
    fn seat_collapsed_fixture_fails() {
        let mut profile = NlheProfile::default();

        // Both positions have identical opening strategy (seat collapse).
        let open_weights = &[
            (Edge::Fold, 0.5f32),
            (Edge::Call, 0.3f32),
            (Edge::Open(2), 0.2f32),
        ];
        let (info0, edges0) = info_with_weights(Path::default(), 0, open_weights);
        let (info1, edges1) = info_with_weights(Path::default(), 1, open_weights);
        profile.encounters.insert(info0, edges0);
        profile.encounters.insert(info1, edges1);

        // Both positions have identical 3-bet strategy.
        let threebet_weights = &[
            (Edge::Fold, 0.8f32),
            (Edge::Call, 0.15f32),
            (Edge::Raise(rbp_gameplay::Odds::new(1, 1)), 0.05f32),
        ];
        let subgame_3bet: Path = vec![Edge::Open(2)].into_iter().collect();
        let (info0_3b, edges0_3b) = info_with_weights(subgame_3bet, 0, threebet_weights);
        let (info1_3b, edges1_3b) = info_with_weights(subgame_3bet, 1, threebet_weights);
        profile.encounters.insert(info0_3b, edges0_3b);
        profile.encounters.insert(info1_3b, edges1_3b);

        let result = check_integrity(&profile);
        assert!(
            matches!(result, Err(IntegrityError::SeatCollapse { .. })),
            "seat-collapsed fixture must fail with SeatCollapse, got: {:?}",
            result
        );
    }

    #[test]
    fn sane_fixture_passes() {
        let mut profile = NlheProfile::default();

        // Early position opens tighter (10% vs 40%).
        let early_open = &[
            (Edge::Fold, 0.6f32),
            (Edge::Call, 0.3f32),
            (Edge::Open(2), 0.1f32),
        ];
        let late_open = &[
            (Edge::Fold, 0.3f32),
            (Edge::Call, 0.3f32),
            (Edge::Open(2), 0.4f32),
        ];
        let (info0, edges0) = info_with_weights(Path::default(), 0, early_open);
        let (info1, edges1) = info_with_weights(Path::default(), 1, late_open);
        profile.encounters.insert(info0, edges0);
        profile.encounters.insert(info1, edges1);

        // 3-bet at exactly 10% aggregate (5% each position).
        let threebet_weights = &[
            (Edge::Fold, 0.75f32),
            (Edge::Call, 0.20f32),
            (Edge::Raise(rbp_gameplay::Odds::new(1, 1)), 0.05f32),
        ];
        let subgame_3bet: Path = vec![Edge::Open(2)].into_iter().collect();
        let (info0_3b, edges0_3b) = info_with_weights(subgame_3bet, 0, threebet_weights);
        let (info1_3b, edges1_3b) = info_with_weights(subgame_3bet, 1, threebet_weights);
        profile.encounters.insert(info0_3b, edges0_3b);
        profile.encounters.insert(info1_3b, edges1_3b);

        let result = check_integrity(&profile);
        assert!(
            result.is_ok(),
            "sane fixture must pass integrity gate, got: {:?}",
            result
        );
        let report = result.unwrap();
        assert!(
            (report.open_freq[0] - 0.10).abs() < 0.001,
            "early open freq should be ~10%, got {}",
            report.open_freq[0]
        );
        assert!(
            (report.open_freq[1] - 0.40).abs() < 0.001,
            "late open freq should be ~40%, got {}",
            report.open_freq[1]
        );
        assert!(
            (report.threebet_freq[0] - 0.05).abs() < 0.001,
            "early 3-bet freq should be ~5%, got {}",
            report.threebet_freq[0]
        );
        assert!(
            (report.threebet_freq[1] - 0.05).abs() < 0.001,
            "late 3-bet freq should be ~5%, got {}",
            report.threebet_freq[1]
        );
    }

    #[test]
    fn threebet_too_low_fails() {
        let mut profile = NlheProfile::default();

        // Early opens tighter.
        let early_open = &[
            (Edge::Fold, 0.6f32),
            (Edge::Call, 0.3f32),
            (Edge::Open(2), 0.1f32),
        ];
        let late_open = &[
            (Edge::Fold, 0.3f32),
            (Edge::Call, 0.3f32),
            (Edge::Open(2), 0.4f32),
        ];
        let (info0, edges0) = info_with_weights(Path::default(), 0, early_open);
        let (info1, edges1) = info_with_weights(Path::default(), 1, late_open);
        profile.encounters.insert(info0, edges0);
        profile.encounters.insert(info1, edges1);

        // 3-bet at 2% aggregate — too low.
        let threebet_weights = &[
            (Edge::Fold, 0.90f32),
            (Edge::Call, 0.08f32),
            (Edge::Raise(rbp_gameplay::Odds::new(1, 1)), 0.02f32),
        ];
        let subgame_3bet: Path = vec![Edge::Open(2)].into_iter().collect();
        let (info0_3b, edges0_3b) = info_with_weights(subgame_3bet, 0, threebet_weights);
        let (info1_3b, edges1_3b) = info_with_weights(subgame_3bet, 1, threebet_weights);
        profile.encounters.insert(info0_3b, edges0_3b);
        profile.encounters.insert(info1_3b, edges1_3b);

        let result = check_integrity(&profile);
        assert!(
            matches!(result, Err(IntegrityError::ThreeBetRange { .. })),
            "low 3-bet fixture must fail with ThreeBetRange, got: {:?}",
            result
        );
    }

    #[test]
    fn threebet_too_high_fails() {
        let mut profile = NlheProfile::default();

        // Early opens tighter.
        let early_open = &[
            (Edge::Fold, 0.6f32),
            (Edge::Call, 0.3f32),
            (Edge::Open(2), 0.1f32),
        ];
        let late_open = &[
            (Edge::Fold, 0.3f32),
            (Edge::Call, 0.3f32),
            (Edge::Open(2), 0.4f32),
        ];
        let (info0, edges0) = info_with_weights(Path::default(), 0, early_open);
        let (info1, edges1) = info_with_weights(Path::default(), 1, late_open);
        profile.encounters.insert(info0, edges0);
        profile.encounters.insert(info1, edges1);

        // 3-bet at 20% aggregate — too high.
        let threebet_weights = &[
            (Edge::Fold, 0.60f32),
            (Edge::Call, 0.20f32),
            (Edge::Raise(rbp_gameplay::Odds::new(1, 1)), 0.20f32),
        ];
        let subgame_3bet: Path = vec![Edge::Open(2)].into_iter().collect();
        let (info0_3b, edges0_3b) = info_with_weights(subgame_3bet, 0, threebet_weights);
        let (info1_3b, edges1_3b) = info_with_weights(subgame_3bet, 1, threebet_weights);
        profile.encounters.insert(info0_3b, edges0_3b);
        profile.encounters.insert(info1_3b, edges1_3b);

        let result = check_integrity(&profile);
        assert!(
            matches!(result, Err(IntegrityError::ThreeBetRange { .. })),
            "high 3-bet fixture must fail with ThreeBetRange, got: {:?}",
            result
        );
    }
}
