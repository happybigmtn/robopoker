//! STW-029: v3 trained-config database-loaded player.
//!
//! Parallels [`super::DatabasePlayer`] (v1) and
//! [`super::DatabasePlayer2`] (v2) but hydrates from
//! the v3 tables
//! ([`rbp_database::BLUEPRINT3`] /
//! [`rbp_database::EPOCH3`]) and wraps a v3
//! [`rbp_nlhe::Flagship3`] solver
//! (`DiscountedRegret` + `LinearWeight` +
//! `PluribusSampling`) instead of the v1
//! [`rbp_nlhe::Flagship`] or the v2
//! [`rbp_nlhe::Flagship2`]. The bench seats a v3
//! `DatabasePlayer3` at seat 0 when
//! `RBP_BENCH_BLUEPRINT=v3` is set, so a single
//! `trainer --bench` invocation can compare the v1 /
//! v2 / v3 trained configs head-to-head against a
//! named baseline without re-training any of them.
//!
//! The v3 is the missing cross-product cell of the
//! v1 / v2 regret / policy combination:
//! - v1 `Flagship`  = `PluribusRegret` + `LinearWeight` +
//!   `PluribusSampling` (Pluribus defaults).
//! - v2 `Flagship2` = `DiscountedRegret` +
//!   `QuadraticWeight` + `PluribusSampling` (DCFR + t^2).
//! - v3 `Flagship3` = `DiscountedRegret` +
//!   `LinearWeight` + `PluribusSampling` (DCFR + t^1.5 —
//!   the "DCFR-with-LinearWeight variant" the CEO roadmap
//!   names as the v6 next slice).
//!
//! The `decide` path is the v1 / v2 path verbatim
//! (`abstraction` → `NlheInfo::from` →
//! `averaged_distribution` → weighted-sample): the v3
//! solver's `averaged_distribution(&info)` reads the
//! same in-memory `NlheProfile` shape the v1 / v2
//! solvers read, so the bench's per-hand accounting is
//! byte-for-byte comparable across the v1 / v2 / v3
//! seat-0 players.

use crate::*;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;
use rbp_gameplay::*;
use rbp_mccfr::*;
use rbp_nlhe::Flagship3;
use rbp_nlhe::NlheEdge;
use rbp_nlhe::NlheInfo;
use rbp_transport::Density;

/// STW-029: v3 trained-config compute player that
/// samples directly from the v3 blueprint strategy
/// without any real-time refinement.
///
/// Parallels [`DatabasePlayer`] (v1) and
/// [`DatabasePlayer2`] (v2) for the v3 trained
/// config. The wrapped solver is
/// [`rbp_nlhe::Flagship3`] = `DiscountedRegret` +
/// `LinearWeight` + `PluribusSampling`; the encoder
/// is the v1 [`rbp_nlhe::NlheEncoder`] (the
/// abstraction clustering is the v1 recipe; only the
/// regret / policy combination differs in the v3
/// solver). The `from_database` constructor hydrates
/// from [`rbp_database::BLUEPRINT3`] +
/// [`rbp_database::EPOCH3`] via
/// [`rbp_nlhe::hydrate_flagship3`].
pub struct DatabasePlayer3(&'static Flagship3);

impl DatabasePlayer3 {
    /// Create a v3 database player from a static v3
    /// `Flagship3` reference. Always available
    /// (no database required); the bench's
    /// empty-blueprint fallback path uses a
    /// default-constructed `Flagship3` wrapped in a
    /// leaked `Box` the same way the v1
    /// `DatabasePlayer::new(leaked_default_flagship)`
    /// and the v2 `DatabasePlayer2::new(...)` do.
    pub fn new(blueprint: &'static Flagship3) -> Self {
        Self(blueprint)
    }
    /// Create a v3 database player by loading from
    /// the v3 database tables and leaking the
    /// resulting `Flagship3` solver. The hydration
    /// path is the v1 / v2 path verbatim (run
    /// pretraining, hydrate the profile from the v3
    /// `'current_v3'` epoch row, hydrate the encoder
    /// from the v1 clustering tables) — only the
    /// table names differ.
    pub async fn from_database(client: std::sync::Arc<tokio_postgres::Client>) -> Self {
        Self(Box::leak(Box::new(
            rbp_nlhe::hydrate_flagship3(client).await,
        )))
    }
    /// Sample an action from a policy using the same
    /// weighted-index recipe as the v1
    /// [`DatabasePlayer::sample`] and the v2
    /// [`DatabasePlayer2::sample`]. The v3 path is
    /// byte-for-byte identical: a uniform fallback to
    /// `game.legal()` covers a zero-weight edge list
    /// (a degenerate-but-valid empty-blueprint
    /// state).
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
impl Player for DatabasePlayer3 {
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

    /// Confirms the static-blueprint constructor
    /// path compiles and yields a `DatabasePlayer3`
    /// wrapping the same `Flagship3` reference. The
    /// bench's empty-blueprint fallback uses this
    /// path against a default-constructed
    /// `Flagship3`; if the wrapper ever drops the
    /// reference (e.g. takes ownership), this test
    /// fails to compile under the v3 feature chain.
    #[test]
    fn new_wraps_blueprint() {
        let blueprint: &'static Flagship3 = Box::leak(Box::new(Flagship3::new(
            rbp_nlhe::NlheProfile::default(),
            rbp_nlhe::NlheEncoder::default(),
        )));
        let player = DatabasePlayer3::new(blueprint);
        assert!(std::ptr::eq(player.0, blueprint));
    }

    /// Compile-time check that the `database`-feature
    /// `from_database` constructor exists with the
    /// expected async signature, parallel to the v1
    /// `DatabasePlayer::from_database_signature_is_stable`
    /// and the v2
    /// `DatabasePlayer2::from_database_signature_is_stable`.
    /// Pins the `rbp-nlhe/database` feature chain
    /// for the v3 hydration path: if
    /// `rbp_nlhe::hydrate_flagship3` is ever
    /// removed, this test will stop compiling under
    /// `--features database` and flag the regression.
    #[cfg(feature = "database")]
    #[test]
    fn from_database_signature_is_stable() {
        fn _assert_takes_arc_client(
            _f: for<'a> fn(
                std::sync::Arc<tokio_postgres::Client>,
                &'a (),
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = DatabasePlayer3> + Send + 'a>,
            >,
        ) {
        }
        let blueprint: &'static Flagship3 = Box::leak(Box::new(Flagship3::new(
            rbp_nlhe::NlheProfile::default(),
            rbp_nlhe::NlheEncoder::default(),
        )));
        let _ = blueprint;
        _assert_takes_arc_client(|c, _| Box::pin(DatabasePlayer3::from_database(c)));
    }
}
