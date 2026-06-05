//! Repro test for seat-collapse bug (slice 1).
//!
//! Two infosets that differ ONLY in player position (seat 0 vs seat 1)
//! currently map to the same policy key because `NlheInfo` does not
//! include seat/position. This test asserts they *should* be different;
//! the assertion fails today, reproducing the bug.

use rbp_cards::Street;
use rbp_gameplay::{Action, Partial, Turn};
use rbp_nlhe::NlheInfo;

#[test]
fn seat_position_collapses_to_same_policy_key() {
    // Build an identical action history for both seats (single preflop call).
    let base = Partial::initial(Turn::Choice(0)).push(Action::Call(1));

    let seat0 = base.clone();
    let seat1 = base.with_pov(Turn::Choice(1));

    // Use the same abstraction bucket for both.
    let abs = rbp_gameplay::Abstraction::from(Street::Pref);

    let info0 = NlheInfo::from((&seat0, abs));
    let info1 = NlheInfo::from((&seat1, abs));

    // Fails today: NlheInfo ignores pov/position, so info0 == info1.
    assert_ne!(
        info0, info1,
        "infosets from seat 0 and seat 1 must not collapse to the same policy key"
    );
}
