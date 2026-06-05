//! NLHE public state: current-street history + available choices.
use super::*;
use rbp_gameplay::*;
use rbp_mccfr::*;

/// NLHE public state: subgame history, available choices, and position.
///
/// Stores the current-street action sequence, the available choices at this
/// decision point, and the player's relative position. All are encoded compactly.
///
/// # Design
///
/// Only what's needed for info set indexing:
/// - `subgame`: Current-street action history (resets on each Draw)
/// - `choices`: Available actions at this decision point
/// - `position`: Relative position (0 = BTN/SB, 1 = BB in heads-up)
///
/// Street information comes from [`NlheSecret`] which embeds street in its encoding.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NlhePublic {
    subgame: Path,
    choices: Path,
    position: u8,
}

impl NlhePublic {
    /// Creates a new public state from subgame history, available choices, and position.
    pub fn new(subgame: Path, choices: Path, position: u8) -> Self {
        Self {
            subgame,
            choices,
            position,
        }
    }
    /// Current-street historical edges as a Path.
    pub fn subgame(&self) -> Path {
        self.subgame
    }
    /// Relative position (0 = BTN/SB, 1 = BB in heads-up).
    pub fn position(&self) -> u8 {
        self.position
    }
    /// Aggression (trailing aggressive actions) for bet sizing grid selection.
    pub fn aggression(&self) -> usize {
        self.subgame.aggression()
    }
}

impl CfrPublic for NlhePublic {
    type E = NlheEdge;
    type T = NlheTurn;
    fn choices(&self) -> Vec<Self::E> {
        self.choices.into_iter().map(NlheEdge::from).collect()
    }
    fn history(&self) -> Vec<Self::E> {
        self.subgame.into_iter().map(NlheEdge::from).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggression_counts_from_path() {
        let subgame = [
            Edge::Check,
            Edge::Raise(Odds::new(1, 2)),
            Edge::Raise(Odds::new(1, 1)),
        ]
        .into_iter()
        .collect::<Path>();
        let choices = Path::default();
        let public = NlhePublic::new(subgame, choices, 0);
        assert_eq!(public.aggression(), 2);
    }
    #[test]
    fn history_returns_subgame_edges() {
        let subgame = [Edge::Check, Edge::Raise(Odds::new(1, 2))]
            .into_iter()
            .collect::<Path>();
        let choices = Path::default();
        let public = NlhePublic::new(subgame, choices, 0);
        let history = public.history();
        assert_eq!(history.len(), 2);
        assert_eq!(Edge::from(history[0]), Edge::Check);
        assert_eq!(Edge::from(history[1]), Edge::Raise(Odds::new(1, 2)));
    }
    #[test]
    fn choices_returns_stored_choices() {
        let subgame = Path::default();
        let choices = [Edge::Fold, Edge::Call, Edge::Shove]
            .into_iter()
            .collect::<Path>();
        let public = NlhePublic::new(subgame, choices, 1);
        let available = public.choices();
        assert_eq!(available.len(), 3);
    }
    #[test]
    fn path_returns_subgame() {
        let subgame = [Edge::Check, Edge::Check].into_iter().collect::<Path>();
        let choices = Path::default();
        let public = NlhePublic::new(subgame, choices, 0);
        assert_eq!(public.subgame(), subgame);
    }
    #[test]
    fn position_roundtrips() {
        let public = NlhePublic::new(Path::default(), Path::default(), 1);
        assert_eq!(public.position(), 1);
    }
}
