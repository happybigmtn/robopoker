//! Database-loaded player that samples directly from trained blueprint.
use crate::*;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;
use rbp_database::Hydrate;
use rbp_gameplay::*;
use rbp_mccfr::*;
use rbp_nlhe::*;
use rbp_transport::Density;

/// Compute player using only blueprint lookup.
///
/// Fast decision-making by directly sampling from the trained blueprint
/// strategy without any real-time refinement.
pub struct DatabasePlayer(&'static Flagship);

impl DatabasePlayer {
    /// Creates a new database player from static blueprint reference.
    pub fn new(blueprint: &'static Flagship) -> Self {
        Self(blueprint)
    }
    /// Creates a database player by loading from database and leaking.
    pub async fn from_database(client: std::sync::Arc<tokio_postgres::Client>) -> Self {
        Self(Box::leak(Box::new(Flagship::hydrate(client).await)))
    }
    /// Samples an action from policy using weighted random selection.
    fn sample(game: &Game, policy: Policy<NlheEdge>) -> Action {
        let edges = policy.support().collect::<Vec<_>>();
        let weights = edges.iter().map(|e| policy.density(e)).collect::<Vec<_>>();
        WeightedIndex::new(&weights)
            .ok()
            .map(|dist| edges[dist.sample(&mut rand::rng())])
            .map(|edge| game.actionize(Edge::from(edge)))
            .unwrap_or_else(|| game.legal().choose(&mut rand::rng()).copied().unwrap())
    }
}

#[async_trait::async_trait]
impl Player for DatabasePlayer {
    async fn notify(&mut self, _: &Event) {}
    async fn decide(&mut self, recall: &Partial) -> Action {
        let game = recall.head();
        let observation = recall.seen();
        let abstraction = self.0.encoder().abstraction(&observation);
        let info = NlheInfo::from((recall, abstraction));
        let policy = self.0.profile().averaged_distribution(&info);
        Self::sample(&game, policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms the static-blueprint constructor path compiles and yields
    /// a `DatabasePlayer` wrapping the same `Flagship` reference. This is
    /// the always-available construction (no database required) that
    /// `from_database` funnels into after `Flagship::hydrate` succeeds.
    #[test]
    fn new_wraps_blueprint() {
        let blueprint: &'static Flagship = Box::leak(Box::new(Flagship::new(
            NlheProfile::default(),
            NlheEncoder::default(),
        )));
        let player = DatabasePlayer::new(blueprint);
        assert!(std::ptr::eq(player.0, blueprint));
    }

    /// Compile-time check that the `database`-feature `from_database`
    /// constructor exists with the expected async signature. This pins
    /// the `rbp-nlhe/database` feature chain: if the `Hydrate` impl for
    /// `Flagship` is ever removed, this test will stop compiling under
    /// `--features database` and flag the regression.
    #[cfg(feature = "database")]
    #[test]
    fn from_database_signature_is_stable() {
        fn _assert_takes_arc_client(
            _f: for<'a> fn(
                std::sync::Arc<tokio_postgres::Client>,
                &'a (),
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = DatabasePlayer> + Send + 'a>,
            >,
        ) {
        }
        let blueprint: &'static Flagship = Box::leak(Box::new(Flagship::new(
            NlheProfile::default(),
            NlheEncoder::default(),
        )));
        let _ = blueprint;
        _assert_takes_arc_client(|c, _| Box::pin(DatabasePlayer::from_database(c)));
    }
}
