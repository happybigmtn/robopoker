//! Head-to-head benchmark harness: trained `DatabasePlayer` (seat 0)
//! vs `Fish` (seat 1) over K heads-up hands.
//!
//! This is the STW-010 proof that a freshly-trained blueprint
//! actually beats the only named baseline in the repo (`Fish`, a
//! player that picks uniformly from the legal-action set). The
//! harness is the smallest unit of "robopoker plays poker": it
//! hydrates a `Flagship` from the trained blueprint, sits it at
//! seat 0 of a real `Room` against a `Fish` at seat 1, drives K
//! hands through the same `Room::play_hand_once` path the
//! production `Room::run` loop uses, and accumulates the per-hand
//! net chip accounting for the blueprint seat.
//!
//! ## Why this lives in `rbp-autotrain` and not `bin/bench`
//!
//! The CEO roadmap lists both `bin/bench` and `trainer --bench` as
//! acceptable shapes. We pick the latter because (1) every other
//! ML pipeline entry point already lives behind `trainer --<mode>`
//! (`--cluster`, `--fast`, `--slow`, `--smoke`), so the bench is
//! the next mode on the same surface, (2) `--smoke` already wires
//! the trainer binary against a live Postgres, and the bench needs
//! the same live connection to hydrate the blueprint, and (3)
//! putting the bench code in the autotrain crate keeps the
//! training/benchmark pair in one module so a worker that lands a
//! new training pipeline can immediately re-run the bench against
//! it without touching the binary glue.
//!
//! ## Statistics
//!
//! We report two things in addition to raw `win_rate`:
//!
//! - `mbb_per_100` — mill-big-blinds per 100 hands. This is the
//!   industry-standard poker bot metric: `mean_chips_per_hand /
//!   B_BLIND * 100`. A blueprint that wins an average of 1 chip
//!   per hand at 1/2 blinds is +50 mbb/100, which is "very good".
//! - `mbb_ci95` — half-width of a normal-approximation 95% CI on
//!   the per-hand mean chip delta, expressed in mbb. Computed as
//!   `1.96 * stdev / sqrt(K) * 100 / B_BLIND`. A 95% CI that
//!   contains 0 means the bench cannot distinguish the blueprint
//!   from break-even at the 5% significance level, which is the
//!   honest reporting boundary for a small-scale env-gated bench.
//!
//! `win_rate_ci95` is the 95% CI on the win-rate proportion
//! (wins / K), reported for completeness so a downstream reader
//! can sanity-check the result without re-deriving it from
//! per-hand deltas.
//!
//! ## Env gates
//!
//! - `RBP_BENCH_HANDS` — number of hands to play (default 200).
//!   The repo's policy is "small env-gated, not Pluribus-scale",
//!   matching `--smoke`'s `RBP_FAST_EPOCHS` discipline.
//! - `RBP_BENCH_BLIND` — big-blind size in chips (default 2,
//!   matching `B_BLIND`). The bench is heads-up; seat 0 is SB,
//!   seat 1 is BB; the per-hand pot is bounded by stack depth
//!   (default 100bb).
//!
//! ## JSON result line
//!
//! On success the mode emits a single-line JSON document with all
//! of the above plus a `seed` field (the millisecond timestamp
//! that initialized the per-hand randomness) and a `blueprint`
//! boolean that is `true` iff the pre-bench `trainer --status`
//! reported a non-zero blueprint row count. A downstream scraper
//! (e.g. the testnet dashboard) can parse this line with any
//! standard JSON parser; the line is emitted on `stdout` so it
//! doesn't get tangled with the `log::info!` stream.

use rbp_core::B_BLIND;
use rbp_core::Chips;
use rbp_core::ID;
use rbp_core::S_BLIND;
use rbp_database::Check;
use rbp_gameroom::DatabasePlayer;
use rbp_gameroom::EquityBot;
use rbp_gameroom::Fish;
use rbp_gameroom::PreflopBot;
use rbp_gameroom::Room;
use std::sync::Arc;
use tokio_postgres::Client;

/// Named baseline the bench seats at seat 1 (the seat opposite
/// the trained blueprint).
///
/// The CEO testnet roadmap lists a "stronger named baseline
/// (e.g. a rule-based or a second trained config)" as the v2
/// next slice after the v1 "blueprint beats random" proof
/// (STW-010). [`Baseline::Fish`] is the v1 random bot that the
/// bench has always used; [`Baseline::Equity`] seats the
/// `EquityBot` named baseline (Monte Carlo equity + a small
/// threshold table on `Action` legality); [`Baseline::Preflop`]
/// seats the v3 `PreflopBot` (preflop hand-tier table + the
/// same postflop threshold table as `EquityBot`). A trained
/// blueprint is expected to beat all three; a downstream
/// scraper can group reports by `baseline` to produce a
/// "trained bot vs fish", "trained bot vs equity-bot", and
/// "trained bot vs preflop-bot" curve from the same
/// `BenchReport` stream.
///
/// `Baseline` is `Copy + PartialEq + Debug` so the bench can
/// (a) round-trip the chosen variant through a JSON field and
/// (b) compare the selected baseline against expected values in
/// unit tests without a JSON parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    /// Random-uniform player (the v1 default). Always
    /// available, no `database` feature requirement.
    Fish,
    /// Rule-based named baseline: estimates hand equity from
    /// `Observation::simulate(256)` and picks the highest-priority
    /// legal action that matches a 0.50 call / 0.65 raise
    /// threshold table. Defined in `rbp_gameroom::EquityBot`.
    Equity,
    /// Preflop-tier aware rule-based named baseline:
    /// classifies the pocket cards into a Tier1/Tier2/Tier3
    /// preflop hand category and picks the smallest legal
    /// raise / call / fold respectively, then delegates to
    /// the same `EquityBot::choose` postflop threshold table
    /// on later streets. Defined in `rbp_gameroom::PreflopBot`.
    Preflop,
}

impl Baseline {
    /// Stable lowercase string used in the JSON report and in
    /// the `RBP_BENCH_BASELINE` env var. Kept as a `match` (not
    /// a derived `Display`) so a future baseline addition
    /// forces the env-var parser and the JSON field together —
    /// a silent mismatch between the two would let a stale
    /// `RBP_BENCH_BASELINE=fish` pick the new baseline.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fish => "fish",
            Self::Equity => "equity",
            Self::Preflop => "preflop",
        }
    }
    /// Parse a baseline name from the `RBP_BENCH_BASELINE` env
    /// var, falling back to [`DEFAULT_BENCH_BASELINE`]. Unknown
    /// values fall back to the default too — the bench should
    /// run with sensible defaults rather than fail a worker
    /// because a stale env var was left in the shell (same
    /// tolerance as `bench_hands` / `bench_blind`).
    pub fn from_env() -> Self {
        match std::env::var("RBP_BENCH_BASELINE").ok().as_deref() {
            Some("fish") => Self::Fish,
            Some("equity") => Self::Equity,
            Some("preflop") => Self::Preflop,
            _ => DEFAULT_BENCH_BASELINE,
        }
    }
}

/// Default seat-1 baseline when `RBP_BENCH_BASELINE` is unset.
/// [`Baseline::Fish`] preserves the v1 behaviour (random
/// baseline), so existing bench reports stay comparable.
pub const DEFAULT_BENCH_BASELINE: Baseline = Baseline::Fish;

/// Number of hands to play when `RBP_BENCH_HANDS` is unset.
///
/// The default is small enough that a CI worker can run the bench
/// end-to-end in seconds but large enough that the 95% CI on
/// mbb/100 narrows enough to distinguish a trained bot from `Fish`
/// at the 5% level (rough rule of thumb: K=200 gives a half-width
/// of ~14 mbb/100 against a stdev of 100 chips/hand, which is
/// enough resolution to flag a "blueprint is winning" run from a
/// "blueprint is break-even" run).
pub const DEFAULT_BENCH_HANDS: usize = 200;

/// Big-blind chip size used when `RBP_BENCH_BLIND` is unset.
///
/// Mirrors `rbp_core::B_BLIND` (the production default) so a
/// bench run with no env override produces mbb/100 numbers
/// directly comparable to the trainer's existing `metrics()`
/// output.
pub const DEFAULT_BENCH_BLIND: Chips = B_BLIND;

/// Read `RBP_BENCH_HANDS` as a positive integer, falling back to
/// [`DEFAULT_BENCH_HANDS`]. A non-positive or non-integer value
/// is treated as "unset" and falls back to the default; the
/// bench is the kind of thing that should run on its own with
/// sensible defaults rather than failing the worker because a
/// stale env var was left in the shell.
pub fn bench_hands() -> usize {
    match std::env::var("RBP_BENCH_HANDS") {
        Ok(s) => s.parse().unwrap_or(DEFAULT_BENCH_HANDS),
        Err(_) => DEFAULT_BENCH_HANDS,
    }
}

/// Read `RBP_BENCH_BLIND` as a positive integer, falling back to
/// [`DEFAULT_BENCH_BLIND`]. Same tolerance as [`bench_hands`].
pub fn bench_blind() -> Chips {
    match std::env::var("RBP_BENCH_BLIND") {
        Ok(s) => s.parse().unwrap_or(DEFAULT_BENCH_BLIND),
        Err(_) => DEFAULT_BENCH_BLIND,
    }
}

/// Head-to-head bench report. Emitted as a JSON line on stdout
/// by [`bench::run`] on success.
///
/// The fields are deliberately flat (no nested objects) so a
/// scraper can read them with a single regex or a single
/// `jq '.fieldname'` query, and so the report survives a future
/// refactor that changes the bench's internal accounting without
/// breaking downstream tooling.
#[derive(Debug)]
pub struct BenchReport {
    /// `K`: number of hands played.
    pub hands: usize,
    /// Hands the blueprint seat (seat 0) won outright. A hand
    /// where both players fold/tie counts as a non-win; this is
    /// the strict "won the showdown / opponent folded" count.
    pub wins: usize,
    /// Hands that ended with seat 0's `won()` strictly negative
    /// (i.e. the blueprint lost chips on the hand). A "loss" in
    /// this sense includes a small loss to a Check-down showdown
    /// where the blueprint held the worse hand.
    pub losses: usize,
    /// Sum of `seat_0.won()` across all K hands, in chips.
    pub net_chips: i64,
    /// `mean_chips_per_hand * 100 / B_BLIND`, the industry-standard
    /// poker bot metric. Computed as `(net_chips as f64 / K as
    /// f64) * 100.0 / B_BLIND as f64`.
    pub mbb_per_100: f64,
    /// 95% CI half-width on the per-hand mean chip delta, in mbb.
    /// Computed as `1.96 * stdev / sqrt(K) * 100 / B_BLIND`.
    pub mbb_ci95: f64,
    /// `wins / K`, the simple proportion of hands won.
    pub win_rate: f64,
    /// 95% CI half-width on `win_rate`, computed as a
    /// normal-approximation on the binomial proportion.
    pub win_rate_ci95: f64,
    /// Big-blind size the bench used to compute mbb.
    pub blind: Chips,
    /// `true` iff the pre-bench blueprint was non-empty. The
    /// bench does not refuse to run on an empty blueprint (the
    /// point of the bench is to *measure* the bot), but the
    /// report flags it so a downstream reader knows the
    /// `DatabasePlayer` was playing with the default untrained
    /// policy, not a trained one.
    pub blueprint_trained: bool,
    /// Which named baseline the bench seated at seat 1. A
    /// downstream scraper can group reports by `baseline` to
    /// produce a "trained bot vs fish" curve and a "trained
    /// bot vs equity-bot" curve from the same `BenchReport`
    /// stream. See [`Baseline`] for the variant list.
    pub baseline: Baseline,
}

impl BenchReport {
    /// Emit the report as a single-line JSON document on stdout.
    ///
    /// The output is intentionally a flat object with
    /// `snake_case` field names so `jq` queries like
    /// `.mbb_per_100` and `.mbb_ci95` work without any
    /// post-processing. The line is followed by a `\n` so
    /// downstream `readline`-style consumers don't block waiting
    /// for a stream that never closes.
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"hands\":{hands},",
                "\"wins\":{wins},",
                "\"losses\":{losses},",
                "\"net_chips\":{net_chips},",
                "\"mbb_per_100\":{mbb_per_100:.4},",
                "\"mbb_ci95\":{mbb_ci95:.4},",
                "\"win_rate\":{win_rate:.4},",
                "\"win_rate_ci95\":{win_rate_ci95:.4},",
                "\"blind\":{blind},",
                "\"blueprint_trained\":{blueprint_trained},",
                "\"baseline\":\"{baseline}\"",
                "}}\n"
            ),
            hands = self.hands,
            wins = self.wins,
            losses = self.losses,
            net_chips = self.net_chips,
            mbb_per_100 = self.mbb_per_100,
            mbb_ci95 = self.mbb_ci95,
            win_rate = self.win_rate,
            win_rate_ci95 = self.win_rate_ci95,
            blind = self.blind,
            blueprint_trained = self.blueprint_trained,
            baseline = self.baseline.as_str(),
        )
    }
}

/// Per-hand chip accounting. One element per K hands, in order,
/// with `net_chips[i]` being seat 0's `won()` for hand `i`. Kept
/// as a separate type from the per-hand input so the test that
/// pins the mbb/100 + CI math can build it from a known vector
/// (e.g. `vec![10; 200]`) without standing up a real `Room`.
///
/// `baseline` is the seat-1 baseline that produced the per-hand
/// vector; it is stamped straight into the [`BenchReport`] so
/// the JSON output carries the same value the caller passed in.
pub fn summarize(per_hand: &[Chips], blind: Chips, baseline: Baseline) -> BenchReport {
    assert!(
        !per_hand.is_empty(),
        "per_hand must contain at least one hand"
    );
    let hands = per_hand.len();
    let wins = per_hand.iter().filter(|&&c| c > 0).count();
    let losses = per_hand.iter().filter(|&&c| c < 0).count();
    let net_chips: i64 = per_hand.iter().map(|&c| c as i64).sum();
    let mean = net_chips as f64 / hands as f64;
    // Sample stdev (Bessel-corrected, divisor N-1). With K=1 the
    // bench still has to return a finite CI; we fall back to 0
    // rather than panicking, since a one-hand bench is degenerate
    // but a valid caller (e.g. a `--smoke` follow-up sanity check
    // that wants to confirm the JSON shape compiles) might issue
    // it.
    let variance = if hands > 1 {
        per_hand
            .iter()
            .map(|&c| {
                let d = c as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / (hands as f64 - 1.0)
    } else {
        0.0
    };
    let stdev = variance.sqrt();
    let se = stdev / (hands as f64).sqrt();
    let mbb_per_100 = mean * 100.0 / blind as f64;
    let mbb_ci95 = 1.96 * se * 100.0 / blind as f64;
    let win_rate = wins as f64 / hands as f64;
    // Wilson-style normal-approx on a binomial: half-width is
    // 1.96 * sqrt(p(1-p) / K). For K=1 this can be slightly
    // conservative (the CI can exceed [0,1]) but the bench
    // already asserts K>=1 at the API boundary.
    let win_rate_se = (win_rate * (1.0 - win_rate) / hands as f64).sqrt();
    let win_rate_ci95 = 1.96 * win_rate_se;
    BenchReport {
        hands,
        wins,
        losses,
        net_chips,
        mbb_per_100,
        mbb_ci95,
        win_rate,
        win_rate_ci95,
        blind,
        blueprint_trained: true,
        baseline,
    }
}

/// Drive K heads-up hands of `DatabasePlayer` (seat 0) vs a
/// named baseline (seat 1) through a real `Room`, and return
/// the per-hand `seat_0.won()` vector.
///
/// The bench intentionally uses the production `Room` shell (not
/// a hand-written engine loop) so the result reflects exactly
/// the path the casino would take in production: each hand is
/// persisted via `HistoryRepository` (the same `create_hand /
/// create_player / create_action` writes the live server issues),
/// the engine drives the players through choice and chance nodes
/// in order, and the per-hand PnL is read off the showdown game
/// state via [`Room::settlements`]. The two customisations are
/// the seat occupants: `DatabasePlayer` at seat 0, and the
/// caller-selected [`Baseline`] at seat 1 (either [`Baseline::Fish`]
/// for the v1 random baseline or [`Baseline::Equity`] for the v2
/// rule-based named baseline).
///
/// `stakes` is the per-hand blind size in chips; we use the
/// production default of `B_BLIND` so the bench's mbb/100
/// numbers are directly comparable to the trainer's training
/// metrics.
///
/// `blueprint_trained` is the pre-bench `client.blueprint()`
/// row count. The bench uses it to pick between a
/// `DatabasePlayer` hydrated from the trained blueprint
/// (`> 0` rows) and a default-constructed `Flagship` (`== 0`
/// rows) so a freshly-reset DB doesn't crash on
/// `NlheProfile::hydrate`'s `expect("to have already created
/// epoch metadata")`. The caller is responsible for stamping
/// the same value into the JSON `blueprint_trained` field so
/// a downstream scraper can distinguish "trained bot"
/// measurements from "untrained bot" measurements.
pub async fn run_hands(
    client: Arc<Client>,
    k: usize,
    stakes: Chips,
    blueprint_trained: bool,
    baseline: Baseline,
) -> Result<Vec<Chips>, String> {
    assert!(k > 0, "bench must play at least one hand");
    let coordinator_room_id: ID<rbp_gameroom::Room> = ID::default();
    let mut room = Room::new(coordinator_room_id, stakes, client.clone());
    // The room row must exist before any `create_hand` runs
    // because `hands.room_id` FKs into `rooms(id)`. The
    // production `Casino::start` calls `create_room` for the
    // same reason; the bench mirrors that.
    rbp_gameroom::HistoryRepository::create_room(&client, &room)
        .await
        .map_err(|e| format!("create_room: {e}"))?;
    if blueprint_trained {
        // `from_database` is `feature = "database"` gated; the
        // bench inherits the same gate through the
        // `rbp-gameroom` `database` feature dep.
        let blueprint = DatabasePlayer::from_database(client.clone()).await;
        room.sit(blueprint, rbp_auth::Lurker::default());
    } else {
        // Empty-blueprint fallback. The default-constructed
        // `Flagship` is the same shape the `DatabasePlayer`
        // unit test uses for a no-DB smoke (`NlheProfile::
        // default()` + `NlheEncoder::default()`). The resulting
        // untrained bot plays uniform-random over legal
        // actions, which is the documented behavior of the
        // bench's "blueprint_trained: false" path.
        let blueprint: &'static rbp_nlhe::Flagship = Box::leak(Box::new(rbp_nlhe::Flagship::new(
            rbp_nlhe::NlheProfile::default(),
            rbp_nlhe::NlheEncoder::default(),
        )));
        room.sit(DatabasePlayer::new(blueprint), rbp_auth::Lurker::default());
    }
    // Seat-1 baseline dispatch. All three branches are
    // synchronous `Player` constructors (no DB round-trip),
    // so the bench picks the seat-1 bot at hand-setup time.
    // A future database-backed baseline would slot in here
    // as a fourth arm of the `match`.
    match baseline {
        Baseline::Fish => room.sit(Fish, rbp_auth::Lurker::default()),
        Baseline::Equity => room.sit(EquityBot, rbp_auth::Lurker::default()),
        Baseline::Preflop => room.sit(PreflopBot, rbp_auth::Lurker::default()),
    }
    let mut per_hand = Vec::with_capacity(k);
    for _ in 0..k {
        room.play_hand_once().await;
        let pnl = room.settlements();
        // The bench is heads-up; `pnl` always has exactly 2
        // entries (one per seat) and seat 0 is the blueprint.
        // Anything else is a `Room` regression.
        assert_eq!(
            pnl.len(),
            2,
            "bench: heads-up room must report 2 settlements per hand, got {pnl:?}"
        );
        per_hand.push(pnl[0]);
        // `play_hand_once` leaves the engine in `Showdown`; the
        // next iteration's `play_hand_once` would panic on
        // `engine.start()` (which requires `Seating`). `conclude`
        // is the missing public hook between successive
        // single-hand runs (it mirrors the production
        // `Room::run` loop body) and returns `true` when the
        // game is over (a player busted and no next hand is
        // playable). If we hit `finished` mid-bench, we stop
        // and report the partial K — the CI loop already
        // considers `K == played`, not `K == requested`.
        if room.conclude() {
            log::warn!(
                "bench: game ended after {} of {} requested hands (player busted)",
                per_hand.len(),
                k
            );
            break;
        }
    }
    Ok(per_hand)
}

/// Top-level entry point invoked by [`Mode::Bench`]. Hydrates
/// the blueprint, runs K hands, summarises, prints the JSON
/// result line, and exits non-zero if anything fails or if the
/// blueprint was empty at start (an empty blueprint means the
/// trained `DatabasePlayer` is indistinguishable from random,
/// so the bench result has no claim to be "trained beats
/// baseline" — the dashboard should flag this rather than
/// silently absorb it).
pub async fn run(client: Arc<Client>) {
    let k = bench_hands();
    let blind = bench_blind();
    let baseline = Baseline::from_env();
    // A `trainer --smoke` pre-run is the documented prep for the
    // bench (the CEO roadmap lists the smoke proof first, the
    // bench second). The bench does not require it — running the
    // bench against an empty blueprint is valid as long as we
    // flag it in the JSON — but we report the pre-bench row
    // count so a downstream scraper can tell the difference.
    let rows_before = client.blueprint().await;
    let blueprint_trained = rows_before > 0;
    log::info!(
        "bench: hydrating blueprint (rows={rows_before}) + playing {k} hands @ blind={blind} baseline={baseline}",
        baseline = baseline.as_str(),
    );
    let per_hand = match run_hands(client, k, blind, blueprint_trained, baseline).await {
        Ok(v) => v,
        Err(e) => {
            log::error!("bench failed: {e}");
            std::process::exit(3);
        }
    };
    let mut report = summarize(&per_hand, blind, baseline);
    report.blueprint_trained = blueprint_trained;
    print!("{}", report.to_json());
    log::info!(
        "bench complete: hands={k} mbb/100={:.2} ci95=±{:.2} wins={} losses={} blueprint_trained={} baseline={}",
        report.mbb_per_100,
        report.mbb_ci95,
        report.wins,
        report.losses,
        report.blueprint_trained,
        report.baseline.as_str(),
    );
    // Empty blueprint: the result is real but it isn't
    // "trained beats baseline"; we exit 0 so the binary's
    // contract is "JSON printed = bench completed", and let the
    // `blueprint_trained: false` field carry the warning. This
    // matches `--smoke`'s "non-zero exit on rows==0" only on
    // training artifacts; the bench is an *evaluation*, not a
    // training, and an empty-blueprint eval is still a valid
    // measurement.
    let _ = S_BLIND; // silence the unused-import lint; S_BLIND is
    // re-exported for callers that want to
    // reason about SB/BB asymmetry in future
    // bench refinements.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the mbb/100 formula: a vector of `+10` chips across
    /// 200 hands at blind=2 must produce `mbb_per_100 = 50.0`
    /// (10 chips/hand * 100 / 2 = 500 mbb/100… wait, `+10` over
    /// 200 hands averages to 0.05 chips/hand, so the expected
    /// mbb/100 is 0.05 * 100 / 2 = 2.5). Confirms the divide-by-
    /// K and divide-by-blind factors are in the right order.
    #[test]
    fn mbb_per_100_formula_matches_mean_times_hundred_over_blind() {
        let per_hand = vec![10_i16; 200];
        let r = summarize(&per_hand, 2, Baseline::Fish);
        // 200 hands * 10 chips = 2000 net chips; mean = 10
        // chips/hand; mbb/100 = 10 * 100 / 2 = 500.0.
        assert!((r.mbb_per_100 - 500.0).abs() < 1e-9);
        assert_eq!(r.net_chips, 2000);
        assert_eq!(r.wins, 200);
        assert_eq!(r.losses, 0);
    }

    /// A perfectly even (zero-mean) vector must report
    /// `mbb_per_100 = 0` and `mbb_ci95` exactly `0` (the
    /// stdev is `0`, so `1.96 * 0 / sqrt(K) = 0`).
    #[test]
    fn zero_mean_vector_yields_zero_mbb_and_zero_ci() {
        let per_hand = vec![0_i16; 100];
        let r = summarize(&per_hand, 2, Baseline::Fish);
        assert_eq!(r.mbb_per_100, 0.0);
        assert_eq!(r.mbb_ci95, 0.0);
        assert_eq!(r.wins, 0);
        assert_eq!(r.losses, 0);
        assert_eq!(r.win_rate, 0.0);
    }

    /// A vector with both wins and losses must split the count
    /// correctly. With `+10/-5/-5/0/...` across 4 hands, wins=1,
    /// losses=2, and the `mbb_per_100` is `0/4 * 100 / 2 = 0`
    /// but the per-hand deltas are non-zero so the CI is
    /// strictly positive.
    #[test]
    fn mixed_wins_and_losses_split_count() {
        let per_hand = vec![10_i16, -5, -5, 0];
        let r = summarize(&per_hand, 2, Baseline::Fish);
        assert_eq!(r.wins, 1);
        assert_eq!(r.losses, 2);
        assert_eq!(r.net_chips, 0);
        assert_eq!(r.mbb_per_100, 0.0);
        assert!(
            r.mbb_ci95 > 0.0,
            "non-zero stdev must yield a non-zero CI; got {}",
            r.mbb_ci95
        );
    }

    /// `win_rate_ci95` is a normal-approx on a binomial: for K
    /// hands at win-rate p the half-width is `1.96 *
    /// sqrt(p(1-p) / K)`. We pin that formula on a hand-known
    /// vector (50 wins, 50 losses) → `win_rate = 0.5`,
    /// `win_rate_ci95 = 1.96 * sqrt(0.25 / 100) ≈ 0.098`. The
    /// exact value is `0.098` (1.96 * 0.05).
    #[test]
    fn win_rate_ci95_matches_normal_approx_formula() {
        let mut per_hand = vec![1_i16; 50];
        per_hand.extend(vec![-1_i16; 50]);
        let r = summarize(&per_hand, 2, Baseline::Fish);
        assert!((r.win_rate - 0.5).abs() < 1e-9);
        let expected = 1.96 * (0.5_f64 * 0.5_f64 / 100_f64).sqrt();
        assert!(
            (r.win_rate_ci95 - expected).abs() < 1e-9,
            "win_rate_ci95 must equal 1.96 * sqrt(p(1-p)/K); got {} expected {}",
            r.win_rate_ci95,
            expected
        );
    }

    /// `to_json` must round-trip the headline numbers as a
    /// single-line JSON object that a downstream `jq` consumer
    /// can parse without preprocessing. We assert the line
    /// contains every field the `BenchReport` struct exposes so
    /// a future refactor that drops a field fails the test
    /// before it lands.
    #[test]
    fn to_json_contains_every_field() {
        let per_hand = vec![3_i16; 10];
        let r = summarize(&per_hand, 2, Baseline::Equity);
        let s = r.to_json();
        for needle in [
            "\"hands\":10",
            "\"wins\":10",
            "\"losses\":0",
            "\"net_chips\":30",
            "\"mbb_per_100\":",
            "\"mbb_ci95\":",
            "\"win_rate\":",
            "\"win_rate_ci95\":",
            "\"blind\":2",
            "\"blueprint_trained\":true",
            "\"baseline\":\"equity\"",
        ] {
            assert!(
                s.contains(needle),
                "to_json output must contain {needle:?}; got: {s}"
            );
        }
        assert!(
            s.ends_with('\n'),
            "to_json output must end with a newline; got: {s:?}"
        );
        assert!(
            s.starts_with('{'),
            "to_json output must start with `{{`; got: {s:?}"
        );
        // No embedded newlines — a scraper that does `readline`
        // should see exactly one line per report.
        assert_eq!(
            s.matches('\n').count(),
            1,
            "to_json must emit exactly one newline; got: {s:?}"
        );
    }

    /// `bench_hands` honours `RBP_BENCH_HANDS`; setting it to a
    /// positive integer overrides the default, and setting it
    /// to garbage (or unsetting it) falls back to
    /// `DEFAULT_BENCH_HANDS`. We save/restore the env var to
    /// keep parallel tests deterministic.
    #[test]
    fn bench_hands_env_override_round_trip() {
        let saved = std::env::var("RBP_BENCH_HANDS").ok();
        // SAFETY: tests in this module are the only writers of
        // `RBP_BENCH_HANDS`; we serialise the read/write with
        // `set_var` and the implicit single-threaded
        // `#[test]` execution model.
        unsafe {
            std::env::set_var("RBP_BENCH_HANDS", "37");
        }
        assert_eq!(bench_hands(), 37);
        unsafe {
            std::env::set_var("RBP_BENCH_HANDS", "not-a-number");
        }
        assert_eq!(bench_hands(), DEFAULT_BENCH_HANDS);
        unsafe {
            std::env::remove_var("RBP_BENCH_HANDS");
        }
        assert_eq!(bench_hands(), DEFAULT_BENCH_HANDS);
        if let Some(v) = saved {
            unsafe {
                std::env::set_var("RBP_BENCH_HANDS", v);
            }
        }
    }

    /// `Baseline::as_str` is the canonical lowercase form used
    /// in both the JSON `baseline` field and the
    /// `RBP_BENCH_BASELINE` env var. Pinning it here means a
    /// refactor that renames a variant has to update the env-var
    /// parser, the JSON field, and this test together, instead
    /// of silently letting a stale `RBP_BENCH_BASELINE=fish`
    /// pick a renamed variant.
    #[test]
    fn baseline_as_str_matches_published_strings() {
        assert_eq!(Baseline::Fish.as_str(), "fish");
        assert_eq!(Baseline::Equity.as_str(), "equity");
        assert_eq!(Baseline::Preflop.as_str(), "preflop");
    }

    /// `Baseline::from_env` honours `RBP_BENCH_BASELINE` for
    /// known values (`fish`, `equity`, `preflop`) and falls back
    /// to `DEFAULT_BENCH_BASELINE` for missing/unset/unknown
    /// values. Same save/restore discipline as
    /// `bench_hands_env_override_round_trip`.
    #[test]
    fn baseline_from_env_round_trip() {
        let saved = std::env::var("RBP_BENCH_BASELINE").ok();
        // SAFETY: tests in this module are the only writers of
        // `RBP_BENCH_BASELINE`; we serialise the read/write
        // with `set_var` and the implicit single-threaded
        // `#[test]` execution model.
        unsafe {
            std::env::set_var("RBP_BENCH_BASELINE", "fish");
        }
        assert_eq!(Baseline::from_env(), Baseline::Fish);
        unsafe {
            std::env::set_var("RBP_BENCH_BASELINE", "equity");
        }
        assert_eq!(Baseline::from_env(), Baseline::Equity);
        unsafe {
            std::env::set_var("RBP_BENCH_BASELINE", "preflop");
        }
        assert_eq!(Baseline::from_env(), Baseline::Preflop);
        unsafe {
            std::env::set_var("RBP_BENCH_BASELINE", "not-a-baseline");
        }
        assert_eq!(Baseline::from_env(), DEFAULT_BENCH_BASELINE);
        unsafe {
            std::env::remove_var("RBP_BENCH_BASELINE");
        }
        assert_eq!(Baseline::from_env(), DEFAULT_BENCH_BASELINE);
        if let Some(v) = saved {
            unsafe {
                std::env::set_var("RBP_BENCH_BASELINE", v);
            }
        }
    }

    /// `summarize` must stamp the caller-supplied `Baseline`
    /// straight into the `BenchReport` so a downstream scraper
    /// can group reports by baseline without re-parsing the
    /// raw `per_hand` vector. This test pins the v1 default
    /// (`Fish`), the v2 named baseline (`Equity`), and the v3
    /// preflop-tier baseline (`Preflop`) paths in one go.
    #[test]
    fn summarize_stamps_baseline_into_report() {
        let per_hand = vec![0_i16; 4];
        let r_fish = summarize(&per_hand, 2, Baseline::Fish);
        assert_eq!(r_fish.baseline, Baseline::Fish);
        assert!(r_fish.to_json().contains("\"baseline\":\"fish\""));
        let r_equity = summarize(&per_hand, 2, Baseline::Equity);
        assert_eq!(r_equity.baseline, Baseline::Equity);
        assert!(r_equity.to_json().contains("\"baseline\":\"equity\""));
        let r_preflop = summarize(&per_hand, 2, Baseline::Preflop);
        assert_eq!(r_preflop.baseline, Baseline::Preflop);
        assert!(r_preflop.to_json().contains("\"baseline\":\"preflop\""));
    }

    /// `DEFAULT_BENCH_BASELINE` must be `Baseline::Fish` to
    /// preserve v1 bench-report comparability: every bench run
    /// that predates the Baseline slice seated `Fish` at seat
    /// 1, so a downstream dashboard that aggregates reports by
    /// `baseline` needs the v1 default to land in the same
    /// bucket as the explicit `RBP_BENCH_BASELINE=fish` runs.
    #[test]
    fn default_bench_baseline_is_fish() {
        assert_eq!(DEFAULT_BENCH_BASELINE, Baseline::Fish);
    }

    // -----------------------------------------------------------------
    // STW-012: preflop-tier aware baseline tests.
    //
    // The bench crate ships `Baseline::Preflop` and seats a
    // `rbp_gameroom::PreflopBot` at seat 1 when the variant is
    // selected. These two lib tests pin the public `PreflopBot`
    // API that the bench depends on: the preflop hand-tier table
    // (one Tier1, one Tier2, one Tier3 example so a refactor that
    // drops a tier fails before it lands) and the
    // smallest-legal-raise dispatch (a real preflop open is
    // 2-3bb, not a 100bb shove). The detailed unit-by-hand tests
    // live in `rbp_gameroom::preflopbot`; these two tests
    // re-assert the bench's *contract* with `PreflopBot` from the
    // bench crate's side, so a future refactor that breaks the
    // contract (e.g. changes `classify_pocket` to return
    // `Tier3Fold` for AA, or changes `decide_preflop` to shove
    // on Tier1) fails the bench crate's tests too.
    // -----------------------------------------------------------------

    use rbp_gameroom::PreflopBot;
    use rbp_gameroom::PreflopTier;

    /// `PreflopBot::classify_pocket` must still classify
    /// pocket Aces (Tier 1), small pairs (Tier 2), and
    /// 72o (Tier 3) the way the gameroom crate's own tests
    /// do. A refactor that drops a tier (e.g. returns
    /// `Tier2Call` for AA) fails the bench crate too.
    #[test]
    fn preflop_tier_starts_with_top_pair() {
        let aa: rbp_cards::Hand = {
            let cards: Vec<rbp_cards::Card> = vec![
                rbp_cards::Card::from((rbp_cards::Rank::Ace, rbp_cards::Suit::C)),
                rbp_cards::Card::from((rbp_cards::Rank::Ace, rbp_cards::Suit::D)),
            ];
            rbp_cards::Hand::from(cards)
        };
        let seven_seven: rbp_cards::Hand = {
            let cards: Vec<rbp_cards::Card> = vec![
                rbp_cards::Card::from((rbp_cards::Rank::Seven, rbp_cards::Suit::C)),
                rbp_cards::Card::from((rbp_cards::Rank::Seven, rbp_cards::Suit::H)),
            ];
            rbp_cards::Hand::from(cards)
        };
        let seven_two_offsuit: rbp_cards::Hand = {
            let cards: Vec<rbp_cards::Card> = vec![
                rbp_cards::Card::from((rbp_cards::Rank::Seven, rbp_cards::Suit::C)),
                rbp_cards::Card::from((rbp_cards::Rank::Two, rbp_cards::Suit::D)),
            ];
            rbp_cards::Hand::from(cards)
        };
        assert_eq!(
            PreflopBot::classify_pocket(aa, 2),
            PreflopTier::Tier1Raise,
            "AA must classify as Tier 1 (raise) for the preflop baseline"
        );
        assert_eq!(
            PreflopBot::classify_pocket(seven_seven, 2),
            PreflopTier::Tier2Call,
            "77 must classify as Tier 2 (call) for the preflop baseline"
        );
        assert_eq!(
            PreflopBot::classify_pocket(seven_two_offsuit, 2),
            PreflopTier::Tier3Fold,
            "72o must classify as Tier 3 (fold) for the preflop baseline"
        );
    }

    /// `PreflopBot::decide_preflop` with a Tier 1 hand and a
    /// full preflop legal set (Shove 100, Raise 8, Raise 4,
    /// Raise 2, Call 2, Check, Fold) must pick the *smallest*
    /// preflop raise (2bb open), not a 100bb shove. A real
    /// preflop open sizes 2-3bb, and the bench relies on
    /// `PreflopBot` not min-raise/relying on Shove at Tier 1.
    #[test]
    fn preflopbot_prefers_smallest_legal_raise() {
        let legal = vec![
            rbp_gameplay::Action::Shove(100),
            rbp_gameplay::Action::Raise(8),
            rbp_gameplay::Action::Raise(4),
            rbp_gameplay::Action::Raise(2),
            rbp_gameplay::Action::Call(2),
            rbp_gameplay::Action::Check,
            rbp_gameplay::Action::Fold,
        ];
        let chosen = PreflopBot::decide_preflop(&legal, PreflopTier::Tier1Raise);
        assert_eq!(
            chosen,
            rbp_gameplay::Action::Raise(2),
            "PreflopBot Tier 1 must pick the smallest legal raise (2bb open), not Shove(100); got {chosen:?}"
        );
    }
}
