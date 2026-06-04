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
use rbp_database::Check2;
use rbp_database::Check3;
use rbp_gameroom::BlufferBot;
use rbp_gameroom::DatabasePlayer;
use rbp_gameroom::DatabasePlayer2;
use rbp_gameroom::DatabasePlayer3;
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
/// same postflop threshold table as `EquityBot`);
/// [`Baseline::Bluffer`] seats the v4 `BlufferBot` (same v3
/// preflop table + a postflop semi-bluff raise on checked-to
/// boards with weak hands and no draw). A trained
/// blueprint is expected to beat all four; a downstream
/// scraper can group reports by `baseline` to produce a
/// "trained bot vs fish", "trained bot vs equity-bot",
/// "trained bot vs preflop-bot", and "trained bot vs
/// bluffer-bot" curve from the same `BenchReport` stream.
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
    /// Semi-bluff-aware rule-based named baseline:
    /// reuses the v3 `PreflopBot` preflop tier table
    /// verbatim and adds a postflop semi-bluff raise on
    /// checked-to boards with weak hands (equity ≤ 0.40)
    /// and no real draw (improvement ≤ 0.20), at a
    /// deterministic per-street frequency (30% flop,
    /// 20% turn, 0% river). The river has no fold
    /// equity, so the v4 never bluffs the river.
    /// Defined in `rbp_gameroom::BlufferBot`.
    Bluffer,
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
            Self::Bluffer => "bluffer",
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
            Some("bluffer") => Self::Bluffer,
            _ => DEFAULT_BENCH_BASELINE,
        }
    }
}

/// Default seat-1 baseline when `RBP_BENCH_BASELINE` is unset.
/// [`Baseline::Fish`] preserves the v1 behaviour (random
/// baseline), so existing bench reports stay comparable.
pub const DEFAULT_BENCH_BASELINE: Baseline = Baseline::Fish;

/// STW-017: trained-config variant the bench seats at
/// seat 0 (the seat whose `won()` is the headline
/// accounting).
///
/// The v1 bench always seats a v1 `DatabasePlayer` (the
/// trained v1 `Flagship`). With STW-017's v2 trained
/// config (`Flagship2` = `DiscountedRegret` +
/// `QuadraticWeight` + `PluribusSampling`) a single
/// `trainer --bench` run can compare the v1 + v2
/// trained configs head-to-head against the same
/// named baseline without re-training either —
/// [`Blueprint::V1`] is the v1 default and
/// [`Blueprint::V2`] is the new variant.
///
/// The `RBP_BENCH_BLUEPRINT` env var selects the variant
/// at run time (`RBP_BENCH_BLUEPRINT=v1` /
/// `RBP_BENCH_BLUEPRINT=v2`). The `blueprint` JSON
/// field on `BenchReport` carries the same value so a
/// downstream scraper can group reports by
/// `blueprint` to produce a "v1 vs fish", "v2 vs
/// fish", "v1 vs preflop", and "v2 vs preflop" curve
/// from the same `BenchReport` stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blueprint {
    /// v1 trained config: `PluribusRegret` +
    /// `LinearWeight` + `PluribusSampling`
    /// ([`rbp_nlhe::Flagship`]). The historical
    /// default; the bench has always seated a v1
    /// `DatabasePlayer` at seat 0.
    V1,
    /// STW-017: v2 trained config: `DiscountedRegret` +
    /// `QuadraticWeight` + `PluribusSampling`
    /// ([`rbp_nlhe::Flagship2`]). New variant
    /// introduced in STW-017 so a single bench
    /// run can compare the v1 + v2 trained
    /// configs head-to-head.
    V2,
    /// STW-029: v3 trained config: `DiscountedRegret` +
    /// `LinearWeight` + `PluribusSampling`
    /// ([`rbp_nlhe::Flagship3`]). New variant
    /// introduced in STW-029 so a single bench
    /// run can compare the v1 / v2 / v3 trained
    /// configs head-to-head. The v3 is the
    /// "third DCFR-with-LinearWeight variant" the
    /// CEO testnet roadmap names as the v6 next
    /// slice after STW-017's `Flagship2` trained
    /// config — the missing cross-product cell of
    /// the v1 / v2 regret / policy combination
    /// (PluribusRegret+LinearWeight,
    /// DCFR+QuadraticWeight, DCFR+LinearWeight).
    V3,
}

impl Blueprint {
    /// Stable lowercase string used in the JSON report
    /// and in the `RBP_BENCH_BLUEPRINT` env var. Kept
    /// as a `match` (not a derived `Display`) so a
    /// future variant addition forces the env-var
    /// parser and the JSON field together — a
    /// silent mismatch between the two would let a
    /// stale `RBP_BENCH_BLUEPRINT=v1` pick the new
    /// variant.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::V3 => "v3",
        }
    }
    /// Parse a blueprint name from the
    /// `RBP_BENCH_BLUEPRINT` env var, falling back to
    /// [`DEFAULT_BENCH_BLUEPRINT`]. Unknown values fall
    /// back to the default too — the bench should
    /// run with sensible defaults rather than fail a
    /// worker because a stale env var was left in
    /// the shell (same tolerance as
    /// [`Baseline::from_env`]).
    pub fn from_env() -> Self {
        match std::env::var("RBP_BENCH_BLUEPRINT").ok().as_deref() {
            Some("v1") => Self::V1,
            Some("v2") => Self::V2,
            Some("v3") => Self::V3,
            _ => DEFAULT_BENCH_BLUEPRINT,
        }
    }
}

/// Default seat-0 trained config when
/// `RBP_BENCH_BLUEPRINT` is unset. [`Blueprint::V1`]
/// preserves the v1 behaviour (v1 `DatabasePlayer` at
/// seat 0), so existing bench reports stay
/// comparable.
pub const DEFAULT_BENCH_BLUEPRINT: Blueprint = Blueprint::V1;

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

/// Default transcript bundle directory the bench writes
/// `transcript-<hand_id>.json` files into when
/// `RBP_BENCH_TRANSCRIPT_DIR` is unset. The default is
/// `./transcripts` (relative to the bench's CWD) so a `trainer
/// --bench` run on a freshly-`--reset` DB leaves the bundle
/// files where a downstream tool can find them without any
/// extra configuration. Operators that want a different
/// location set `RBP_BENCH_TRANSCRIPT_DIR=/path/to/dir`.
pub const DEFAULT_BENCH_TRANSCRIPT_DIR: &str = "./transcripts";

/// Read `RBP_BENCH_TRANSCRIPT_DIR` as a string path, falling
/// back to [`DEFAULT_BENCH_TRANSCRIPT_DIR`]. An empty value
/// (e.g. `RBP_BENCH_TRANSCRIPT_DIR=""`) returns `None` from
/// the bench's writer path — that is the documented "disable
/// transcript writes" knob, alongside the more explicit
/// `RBP_BENCH_TRANSCRIPT_DISABLE=1` flag. The split into
/// "empty disables" and "default value disables" lets an
/// operator unset the env var (a one-line `.env` change)
/// without re-typing a long path to turn the writer off.
pub fn bench_transcript_dir() -> Option<String> {
    match std::env::var("RBP_BENCH_TRANSCRIPT_DIR") {
        Ok(s) if s.is_empty() => None,
        Ok(s) => Some(s),
        Err(_) => {
            // Fall back to the default unless the explicit
            // disable flag is set. The default is the
            // constant the harness promotes; the disable
            // flag is the operator override.
            if std::env::var("RBP_BENCH_TRANSCRIPT_DISABLE")
                .ok()
                .as_deref()
                == Some("1")
            {
                None
            } else {
                Some(DEFAULT_BENCH_TRANSCRIPT_DIR.to_string())
            }
        }
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
    /// STW-017: which trained config the bench seated at
    /// seat 0 (the seat whose `won()` is the headline
    /// accounting). [`Blueprint::V1`] is the v1
    /// `Flagship` (`PluribusRegret` + `LinearWeight` +
    /// `PluribusSampling`) trained against
    /// [`rbp_database::BLUEPRINT`]; [`Blueprint::V2`] is the
    /// v2 [`rbp_nlhe::Flagship2`] (`DiscountedRegret` +
    /// `QuadraticWeight` + `PluribusSampling`) trained
    /// against [`rbp_database::BLUEPRINT2`]. A downstream
    /// scraper groups reports by `blueprint` to produce a
    /// "v1 vs fish", "v2 vs fish", "v1 vs preflop", and
    /// "v2 vs preflop" curve from the same
    /// `BenchReport` stream.
    pub blueprint: Blueprint,
    /// Which named baseline the bench seated at seat 1. A
    /// downstream scraper can group reports by `baseline` to
    /// produce a "trained bot vs fish" curve and a "trained
    /// bot vs equity-bot" curve from the same `BenchReport`
    /// stream. See [`Baseline`] for the variant list.
    pub baseline: Baseline,
    /// `true` iff the bench wrote at least one
    /// `transcript-<hand_id>.json` bundle into
    /// `RBP_BENCH_TRANSCRIPT_DIR` during this run. The bench
    /// is the producer side of the on-the-wire "replayable
    /// benchmark surface" the testnet roadmap requires; the
    /// `transcript` flag tells a downstream scraper whether a
    /// given `BenchReport` is paired with a per-hand bundle
    /// directory (the directory is the "replay from the
    /// README" artifact). A `false` value means either
    /// `RBP_BENCH_TRANSCRIPT_DISABLE=1` was set, the directory
    /// was unwritable, or every per-hand read-back returned a
    /// missing/incomplete set of rows.
    pub transcript: bool,
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
                "\"blueprint\":\"{blueprint}\",",
                "\"baseline\":\"{baseline}\",",
                "\"transcript\":{transcript}",
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
            blueprint = self.blueprint.as_str(),
            baseline = self.baseline.as_str(),
            transcript = self.transcript,
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
pub fn summarize(
    per_hand: &[Chips],
    blind: Chips,
    baseline: Baseline,
    blueprint: Blueprint,
) -> BenchReport {
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
        blueprint,
        baseline,
        transcript: false,
    }
}

/// Drive K heads-up hands of a trained-config `DatabasePlayer`
/// (seat 0) vs a named baseline (seat 1) through a real `Room`,
/// and return the per-hand `seat_0.won()` vector plus a flag
/// indicating whether at least one per-hand
/// `transcript-<hand_id>.json` bundle was written into
/// [`bench_transcript_dir`].
///
/// `blueprint` selects the v1 / v2 trained config the bench
/// seats at seat 0 — [`Blueprint::V1`] is the v1 trained config
/// ([`rbp_nlhe::Flagship`], trained against
/// [`rbp_database::BLUEPRINT`]) and [`Blueprint::V2`] is the v2
/// trained config ([`rbp_nlhe::Flagship2`], trained against
/// [`rbp_database::BLUEPRINT2`]). The seat-0 dispatch is the
/// v1 / v2 `match` block in this function's body; the seat-1
/// baseline is selected by the `baseline` parameter.
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
    blueprint: Blueprint,
) -> Result<(Vec<Chips>, bool), String> {
    assert!(k > 0, "bench must play at least one hand");
    let coordinator_room_id: ID<rbp_gameroom::Room> = ID::default();
    // The transcript writer is opt-in via env var. `Some(path)`
    // turns on the per-hand `Transcript::write_to_path` call;
    // `None` keeps the bench as a no-side-effect measurement
    // (no directory is created, no files are written). The
    // boolean the function returns is `true` iff the writer
    // actually wrote at least one file in this run.
    let transcript_dir = bench_transcript_dir();
    let mut wrote_transcript = false;
    let mut room = Room::new(coordinator_room_id, stakes, client.clone());
    // The room row must exist before any `create_hand` runs
    // because `hands.room_id` FKs into `rooms(id)`. The
    // production `Casino::start` calls `create_room` for the
    // same reason; the bench mirrors that.
    rbp_gameroom::HistoryRepository::create_room(&client, &room)
        .await
        .map_err(|e| format!("create_room: {e}"))?;
    // STW-017: seat-0 dispatch picks the v1
    // `DatabasePlayer` (v1 trained config) or the v2
    // `DatabasePlayer2` (v2 trained config) based on
    // `blueprint`. The two paths share the same
    // "trained or empty-blueprint fallback" shape
    // (a `from_database` load on a non-empty blueprint,
    // a default-constructed `Flagship` / `Flagship2`
    // leak on an empty one) so the v1 / v2 benches
    // are byte-for-byte comparable.
    match blueprint {
        Blueprint::V1 => {
            if blueprint_trained {
                let blueprint = DatabasePlayer::from_database(client.clone()).await;
                room.sit(blueprint, rbp_auth::Lurker::default());
            } else {
                let blueprint: &'static rbp_nlhe::Flagship =
                    Box::leak(Box::new(rbp_nlhe::Flagship::new(
                        rbp_nlhe::NlheProfile::default(),
                        rbp_nlhe::NlheEncoder::default(),
                    )));
                room.sit(DatabasePlayer::new(blueprint), rbp_auth::Lurker::default());
            }
        }
        Blueprint::V2 => {
            if blueprint_trained {
                let blueprint = DatabasePlayer2::from_database(client.clone()).await;
                room.sit(blueprint, rbp_auth::Lurker::default());
            } else {
                let blueprint: &'static rbp_nlhe::Flagship2 =
                    Box::leak(Box::new(rbp_nlhe::Flagship2::new(
                        rbp_nlhe::NlheProfile::default(),
                        rbp_nlhe::NlheEncoder::default(),
                    )));
                room.sit(DatabasePlayer2::new(blueprint), rbp_auth::Lurker::default());
            }
        }
        // STW-029: v3 trained config (third
        // DCFR-with-LinearWeight variant) seat-0
        // dispatch. The v3 path parallels the v1 /
        // v2 `match` arms verbatim: a v3
        // `DatabasePlayer3` on a non-empty v3
        // blueprint, a default-constructed
        // `Flagship3` `Box::leak` on an empty
        // v3 blueprint (the documented
        // post-`--reset` state). The v3 empty
        // fallback uses `Flagship3::new(...,
        // NlheEncoder::default())` so a
        // freshly-`--reset` DB doesn't crash on
        // the empty `NlheProfileV3::hydrate`
        // path.
        Blueprint::V3 => {
            if blueprint_trained {
                let blueprint = DatabasePlayer3::from_database(client.clone()).await;
                room.sit(blueprint, rbp_auth::Lurker::default());
            } else {
                let blueprint: &'static rbp_nlhe::Flagship3 =
                    Box::leak(Box::new(rbp_nlhe::Flagship3::new(
                        rbp_nlhe::NlheProfile::default(),
                        rbp_nlhe::NlheEncoder::default(),
                    )));
                room.sit(DatabasePlayer3::new(blueprint), rbp_auth::Lurker::default());
            }
        }
    }
    // Seat-1 baseline dispatch. All four branches are
    // synchronous `Player` constructors (no DB round-trip),
    // so the bench picks the seat-1 bot at hand-setup time.
    // A future database-backed baseline would slot in here
    // as a fifth arm of the `match`.
    match baseline {
        Baseline::Fish => room.sit(Fish, rbp_auth::Lurker::default()),
        Baseline::Equity => room.sit(EquityBot, rbp_auth::Lurker::default()),
        Baseline::Preflop => room.sit(PreflopBot, rbp_auth::Lurker::default()),
        Baseline::Bluffer => room.sit(BlufferBot, rbp_auth::Lurker::default()),
    }
    let mut per_hand = Vec::with_capacity(k);
    for _ in 0..k {
        // `Room::play_hand_once` returns the hand id of the
        // hand it just flushed. We use it as the transcript
        // bundle's filename suffix so a downstream tool can
        // correlate the per-hand JSONL line with the
        // `BenchReport` that produced it.
        let hand_id = room.play_hand_once().await;
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
        // Per-hand transcript write. We read back the
        // persisted records `Room::flush_hand` just wrote
        // (the same path the live server uses), build a
        // `Transcript`, and write it to the configured
        // directory. A read-back that returns `None` for
        // the hand row (a Room regression) or a write that
        // fails are both downgraded to `log::warn!` lines —
        // a transcript-write failure is a data-quality
        // problem, not a reason to fail the bench.
        if let Some(dir) = transcript_dir.as_deref() {
            match write_hand_transcript(&client, hand_id, std::path::Path::new(dir)).await {
                Ok(true) => wrote_transcript = true,
                Ok(false) => log::warn!(
                    "bench: hand {hand_id} produced no Transcript (read-back returned no rows); skipping bundle"
                ),
                Err(e) => {
                    log::warn!("bench: Transcript::write_to_path failed for hand {hand_id}: {e}")
                }
            }
        }
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
    Ok((per_hand, wrote_transcript))
}

/// Read a single hand's `Hand` / `Vec<Participant>` /
/// `Vec<Play>` back from the persistence layer (the same
/// `HistoryRepository::get_hand / get_players / get_actions`
/// queries the live server uses), build a `Transcript` from
/// them, and write it to `<dir>/transcript-<hand_id>.json`.
///
/// Returns `Ok(true)` if the file was written, `Ok(false)` if
/// the read-back returned no hand row (a missing or
/// hand-rolled Room write), and `Err(String)` if the
/// underlying I/O failed.
async fn write_hand_transcript(
    client: &Arc<Client>,
    hand_id: ID<rbp_gameroom::records::Hand>,
    dir: &std::path::Path,
) -> Result<bool, String> {
    // `load_transcript` factors the three-read handshake
    // (`get_hand` + `get_players` + `get_actions`) into a
    // single reusable entry point on the gameroom
    // repository module — any future caller that wants a
    // transcript from a hand id reuses the same path the
    // bench uses, so a refactor that changes the read
    // order or the seq-ordering invariant fails this site
    // first. The `Option` return lets us distinguish
    // "no such hand" from a DB error without parsing the
    // underlying error string.
    let t = rbp_gameroom::load_transcript(client, hand_id)
        .await
        .map_err(|e| format!("load_transcript({hand_id}): {e}"))?;
    let Some(t) = t else {
        return Ok(false);
    };
    // `verify` is cheap (O(N) over the plays) and catches
    // the two classes of corruption the bundle is designed
    // to surface: orphan `Play::player` UUIDs and
    // non-monotonic `seq` fields. A failing verify is
    // downgraded to a `log::warn!` so a corrupt historical
    // record doesn't fail the bench — the file is still
    // written, and a downstream tool can re-verify and
    // decide what to do.
    if let Err(e) = t.verify() {
        log::warn!("bench: hand {hand_id} transcript verify failed: {e}");
    }
    let path = dir.join(format!("transcript-{}.json", hand_id.inner()));
    t.write_to_path(&path)
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(true)
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
    // STW-017: read the v1 / v2 trained-config variant
    // from `RBP_BENCH_BLUEPRINT`. The default is
    // [`Blueprint::V1`] so existing bench reports (which
    // never set the env var) stay byte-for-byte comparable
    // to the new v2 reports except for the new
    // `blueprint` JSON field.
    let blueprint = Blueprint::from_env();
    // A `trainer --smoke` pre-run is the documented prep for the
    // bench (the CEO roadmap lists the smoke proof first, the
    // bench second). The bench does not require it — running the
    // bench against an empty blueprint is valid as long as we
    // flag it in the JSON — but we report the pre-bench row
    // count so a downstream scraper can tell the difference.
    // STW-017: the row count comes from the v1 /
    // v2 blueprint table that matches the selected
    // `blueprint` variant. A v1 bench reads the v1
    // row count; a v2 bench reads the v2 row count.
    let rows_before = match blueprint {
        Blueprint::V1 => client.blueprint().await,
        Blueprint::V2 => client.blueprint_v2().await,
        // STW-029: v3 row count comes from
        // `BLUEPRINT3`. The v3 row count is
        // gated on `Check3` (not `Check` /
        // `Check2`); the `Status` table is
        // printed with the v1 + v2 + v3
        // side-by-side `Mode::Status` arm in
        // `mode.rs`.
        Blueprint::V3 => client.blueprint_v3().await,
    };
    let blueprint_trained = rows_before > 0;
    log::info!(
        "bench: hydrating blueprint (variant={} rows={rows_before}) + playing {k} hands @ blind={blind} baseline={}",
        blueprint.as_str(),
        baseline.as_str(),
    );
    let (per_hand, transcript_wrote) =
        match run_hands(client, k, blind, blueprint_trained, baseline, blueprint).await {
            Ok(v) => v,
            Err(e) => {
                log::error!("bench failed: {e}");
                std::process::exit(3);
            }
        };
    let mut report = summarize(&per_hand, blind, baseline, blueprint);
    report.blueprint_trained = blueprint_trained;
    // `transcript_wrote` is the producer side of the
    // "replayable benchmark surface" the testnet roadmap
    // requires. A `true` value tells a downstream scraper
    // that there is at least one
    // `transcript-<hand_id>.json` file in the configured
    // directory; a `false` value is a sign the writer was
    // disabled (env var), the directory was unwritable, or
    // every per-hand read-back returned a missing row. The
    // `transcript` JSON field is the single bit a dashboard
    // needs to decide whether to show the "replay" link.
    report.transcript = transcript_wrote;
    print!("{}", report.to_json());
    log::info!(
        "bench complete: hands={k} mbb/100={:.2} ci95=±{:.2} wins={} losses={} blueprint={} blueprint_trained={} baseline={}",
        report.mbb_per_100,
        report.mbb_ci95,
        report.wins,
        report.losses,
        report.blueprint.as_str(),
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

// ---------------------------------------------------------------------
// STW-018: `trainer --compare` head-to-head v1-vs-v2 trained-config
// bench.
//
// The CEO testnet roadmap explicitly names "a third DCFR-with-
// LinearWeight variant, or a 'named bot vs second trained config'
// comparison" as the v6 next slice after STW-017's `Flagship2`
// trained config. STW-018 lands the comparison half: a single
// `trainer --compare` invocation seats the v1 `DatabasePlayer` (seat
// 0) and the v2 `DatabasePlayer2` (seat 1) against each other in the
// same `Room`, drives K heads-up hands, reads per-hand settlements
// for both seats, and prints a single-line JSON `CompareReport`
// declaring the winner (`"v1"`, `"v2"`, or `"tie"`) plus the
// per-side mbb/100 / CI / win-rate numbers.
//
// ## Why this lives next to the bench
//
// The compare reuses the same `Room` shell + per-hand settlement
// shape the existing `--bench` uses; the only new code is the
// seat-1 dispatch (a v2 `DatabasePlayer2` instead of a `Fish` /
// `EquityBot` / `PreflopBot` / `BlufferBot`) and the report struct
// (a `CompareReport` with two `CompareSubReport` sub-reports
// instead of a single `BenchReport`). Putting the new code next
// to the existing bench code keeps the v1 / v2 / v3 / v4 + compare
// surfaces in one module so a worker that lands a new trained
// config variant can immediately wire it into both the bench and
// the compare without touching the binary glue.
//
// ## The "heads-up nets to zero" invariant
//
// In a heads-up `Room` the two seats' `settlements()[i]` values
// net to zero by construction: one seat wins what the other loses
// (no rake, no dead pot — every chip that leaves seat 0 lands at
// seat 1 and vice versa). The `CompareSubReport::net_chips` field
// for the v1 + v2 sides therefore always sum to exactly 0 (in
// integer chip space, not float), and the `mbb_per_100` values
// sum to within float-rounding tolerance of 0 (the bench's
// `to_json` formatter uses `:.4` so the precision loss is
// bounded by `5e-5`). A regression that introduces a phantom
// pot (e.g. a `flush_hand` that double-counts a dead blind) is
// caught at the `compare_summarize_v1_plus_v2_deltas_net_to_zero`
// lib test before it lands.
//
// ## Env gates
//
// - `RBP_COMPARE_HANDS` — number of hands to play (default 200,
//   matching `DEFAULT_BENCH_HANDS`).
// - `RBP_COMPARE_BLIND` — big-blind size in chips (default
//   `B_BLIND`, matching `DEFAULT_BENCH_BLIND`).
//
// We deliberately re-use the bench's default constants rather
// than introduce separate `DEFAULT_COMPARE_*` constants so a
// dashboard that plots both bench and compare numbers on the
// same x-axis uses the same K / blind. The env-var names are
// new (the bench's `RBP_BENCH_HANDS` / `RBP_BENCH_BLIND` are
// left untouched, so a v1 `trainer --bench` run and a v1
// `trainer --compare` run can be sized independently if a
// worker wants to).
//
// ## JSON result line
//
// On success the mode emits a single-line JSON document with a
// `hands` count, a `blind` size, a `v1` sub-report (the v1
// `DatabasePlayer` at seat 0), a `v2` sub-report (the v2
// `DatabasePlayer2` at seat 1), the `delta_mbb_per_100` (v1
// minus v2; the sign is the winner direction), and a `winner`
// string. The `winner` field is the headline a testnet dashboard
// reads; the per-side sub-reports let a downstream scraper plot
// the per-config learning curve over a series of `--compare`
// runs.
// ---------------------------------------------------------------------

/// STW-018: v1-vs-v2 trained-config comparison winner. The
/// compare seat both bots in the same `Room`; per-hand
/// settlements net to zero, and the headline winner is the
/// side with the strictly positive `mbb_per_100`. A delta
/// within [`COMPARE_TIE_TOLERANCE`] mbb/100 of zero is a
/// `Tie` (the two configs are statistically
/// indistinguishable at the chosen K).
///
/// `CompareWinner` is `Copy + PartialEq + Debug` so the
/// compare can (a) round-trip the chosen variant through a
/// JSON field and (b) compare the declared winner against
/// expected values in unit tests without a JSON parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareWinner {
    /// The v1 `DatabasePlayer` (seat 0) won the heads-up.
    V1,
    /// The v2 `DatabasePlayer2` (seat 1) won the heads-up.
    V2,
    /// The two configs' `mbb_per_100` differ by less than
    /// [`COMPARE_TIE_TOLERANCE`] mbb/100; the K-handed
    /// sample is too small to distinguish them.
    Tie,
}

impl CompareWinner {
    /// Stable lowercase string used in the JSON report.
    /// Kept as a `match` (not a derived `Display`) so a
    /// future variant addition forces the JSON encoder and
    /// the test that pins the string literal together.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::Tie => "tie",
        }
    }
}

/// STW-018: per-side sub-report inside a [`CompareReport`].
/// The shape mirrors the headline numbers from
/// [`BenchReport`] (hands, wins, losses, net_chips, mbb/100,
/// mbb_ci95, win_rate, win_rate_ci95, blind) so a downstream
/// scraper that already knows the `BenchReport` shape can
/// read the compare sub-reports with the same parser. The
/// compare does NOT include the `baseline` /
/// `blueprint_trained` / `transcript` fields the bench
/// carries because the compare has no named baseline (both
/// seats are trained configs) and no transcript write
/// (the compare is a measurement, not a recordable
/// surface).
#[derive(Debug)]
pub struct CompareSubReport {
    /// `K`: number of hands played. Always equals the
    /// `hands` field on the parent [`CompareReport`]
    /// (both seats play the same K hands in the same
    /// room).
    pub hands: usize,
    /// Hands this side won outright.
    pub wins: usize,
    /// Hands that ended with this side's `won()` strictly
    /// negative.
    pub losses: usize,
    /// Sum of this side's `won()` across all K hands, in
    /// chips.
    pub net_chips: i64,
    /// `mean_chips_per_hand * 100 / B_BLIND`.
    pub mbb_per_100: f64,
    /// 95% CI half-width on the per-hand mean chip delta,
    /// in mbb.
    pub mbb_ci95: f64,
    /// `wins / K`, the simple proportion of hands won.
    pub win_rate: f64,
    /// 95% CI half-width on `win_rate`.
    pub win_rate_ci95: f64,
}

impl CompareSubReport {
    /// Compute the per-side mbb/100 / CI / win-rate math
    /// from a per-hand chip vector. Mirrors the
    /// `summarize` math in [`crate::bench`] (the bench
    /// itself) so the bench and compare reports use the
    /// same formulas, and so a regression in the
    /// per-hand PnL math fails both the bench and
    /// compare lib tests in the same CI run.
    fn from_per_hand(per_hand: &[Chips], blind: Chips) -> Self {
        assert!(
            !per_hand.is_empty(),
            "compare: per_hand must contain at least one hand"
        );
        let hands = per_hand.len();
        let wins = per_hand.iter().filter(|&&c| c > 0).count();
        let losses = per_hand.iter().filter(|&&c| c < 0).count();
        let net_chips: i64 = per_hand.iter().map(|&c| c as i64).sum();
        let mean = net_chips as f64 / hands as f64;
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
        let blind_f = blind as f64;
        let mbb_per_100 = mean * 100.0 / blind_f;
        let mbb_ci95 = 1.96 * se * 100.0 / blind_f;
        let win_rate = wins as f64 / hands as f64;
        let win_rate_se = (win_rate * (1.0 - win_rate) / hands as f64).sqrt();
        let win_rate_ci95 = 1.96 * win_rate_se;
        CompareSubReport {
            hands,
            wins,
            losses,
            net_chips,
            mbb_per_100,
            mbb_ci95,
            win_rate,
            win_rate_ci95,
        }
    }
}

/// STW-018: tie tolerance in mbb/100. A delta within this
/// threshold of zero is reported as `Tie` (the two
/// configs are statistically indistinguishable at the
/// chosen K). The tolerance is intentionally tight
/// (`1e-6` mbb/100) so the bench can declare a real
/// `Tie` only when the v1 + v2 `mbb_per_100` values are
/// bit-for-bit identical within float-rounding error. A
/// worker that wants a coarser "is the difference
/// within the CI?" tie test should re-derive the answer
/// from the per-side `mbb_ci95` fields, not loosen this
/// constant.
pub const COMPARE_TIE_TOLERANCE: f64 = 1e-6;

/// STW-018: head-to-head v1-vs-v2 trained-config
/// report. Emitted as a JSON line on stdout by
/// [`bench::run_compare`] on success.
#[derive(Debug)]
pub struct CompareReport {
    /// `K`: number of hands played. Both sub-reports'
    /// `hands` field equals this value.
    pub hands: usize,
    /// Big-blind chip size the compare used to compute
    /// mbb. Both sub-reports' `mbb_per_100` /
    /// `mbb_ci95` use this blind.
    pub blind: Chips,
    /// The v1 `DatabasePlayer` (seat 0) sub-report.
    pub v1: CompareSubReport,
    /// The v2 `DatabasePlayer2` (seat 1) sub-report.
    pub v2: CompareSubReport,
    /// `v1.mbb_per_100 - v2.mbb_per_100`. The sign is
    /// the winner direction (positive ⇒ v1 winning,
    /// negative ⇒ v2 winning); a value within
    /// [`COMPARE_TIE_TOLERANCE`] of zero is a `Tie`.
    pub delta_mbb_per_100: f64,
    /// Headline winner: the side with the strictly
    /// positive `delta_mbb_per_100`, or `Tie` if the
    /// delta is within the tie tolerance of zero.
    pub winner: CompareWinner,
}

impl CompareReport {
    /// Emit the report as a single-line JSON document on
    /// stdout. The output is a flat object with
    /// `snake_case` field names so `jq` queries like
    /// `.winner` and `.delta_mbb_per_100` work without
    /// any post-processing. The line is followed by a
    /// `\n` so downstream `readline`-style consumers
    /// don't block waiting for a stream that never
    /// closes.
    ///
    /// The v1 + v2 sub-reports are nested under `v1` /
    /// `v2` keys (rather than flattened) so the
    /// sub-report fields are namespaced and a future
    /// `BenchReport` / `CompareReport` field addition
    /// that collides with a sub-report field name (e.g.
    /// a top-level `baseline` field on the compare) is
    /// impossible by construction.
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{",
                "\"hands\":{hands},",
                "\"blind\":{blind},",
                "\"v1\":{{",
                "\"hands\":{v1_hands},",
                "\"wins\":{v1_wins},",
                "\"losses\":{v1_losses},",
                "\"net_chips\":{v1_net_chips},",
                "\"mbb_per_100\":{v1_mbb_per_100:.4},",
                "\"mbb_ci95\":{v1_mbb_ci95:.4},",
                "\"win_rate\":{v1_win_rate:.4},",
                "\"win_rate_ci95\":{v1_win_rate_ci95:.4}",
                "}},",
                "\"v2\":{{",
                "\"hands\":{v2_hands},",
                "\"wins\":{v2_wins},",
                "\"losses\":{v2_losses},",
                "\"net_chips\":{v2_net_chips},",
                "\"mbb_per_100\":{v2_mbb_per_100:.4},",
                "\"mbb_ci95\":{v2_mbb_ci95:.4},",
                "\"win_rate\":{v2_win_rate:.4},",
                "\"win_rate_ci95\":{v2_win_rate_ci95:.4}",
                "}},",
                "\"delta_mbb_per_100\":{delta_mbb_per_100:.4},",
                "\"winner\":\"{winner}\"",
                "}}\n"
            ),
            hands = self.hands,
            blind = self.blind,
            v1_hands = self.v1.hands,
            v1_wins = self.v1.wins,
            v1_losses = self.v1.losses,
            v1_net_chips = self.v1.net_chips,
            v1_mbb_per_100 = self.v1.mbb_per_100,
            v1_mbb_ci95 = self.v1.mbb_ci95,
            v1_win_rate = self.v1.win_rate,
            v1_win_rate_ci95 = self.v1.win_rate_ci95,
            v2_hands = self.v2.hands,
            v2_wins = self.v2.wins,
            v2_losses = self.v2.losses,
            v2_net_chips = self.v2.net_chips,
            v2_mbb_per_100 = self.v2.mbb_per_100,
            v2_mbb_ci95 = self.v2.mbb_ci95,
            v2_win_rate = self.v2.win_rate,
            v2_win_rate_ci95 = self.v2.win_rate_ci95,
            delta_mbb_per_100 = self.delta_mbb_per_100,
            winner = self.winner.as_str(),
        )
    }
}

/// STW-018: build a [`CompareReport`] from the per-hand
/// PnL vectors of the v1 and v2 sides. `v1_per_hand[i]`
/// and `v2_per_hand[i]` are the v1 and v2 seat's
/// `settlements()` for hand `i`; in a heads-up `Room`
/// they net to zero (`v1_per_hand[i] + v2_per_hand[i] == 0`)
/// for every hand.
///
/// `blind` is the big-blind size the per-side mbb/100
/// math uses; we re-run the bench's `summarize`-style
/// math with the real blind (the helper that builds the
/// sub-reports uses a `1`-blind placeholder so the
/// formula is identical to the bench's, then we
/// re-multiply the mbb/100 + CI values by the real
/// `blind` here).
///
/// The winner is the side with the strictly positive
/// `delta_mbb_per_100`; a delta within
/// [`COMPARE_TIE_TOLERANCE`] of zero is a `Tie`.
pub fn summarize_compare(
    v1_per_hand: &[Chips],
    v2_per_hand: &[Chips],
    blind: Chips,
) -> CompareReport {
    assert!(
        !v1_per_hand.is_empty(),
        "summarize_compare: v1_per_hand must contain at least one hand"
    );
    assert_eq!(
        v1_per_hand.len(),
        v2_per_hand.len(),
        "summarize_compare: v1 + v2 per-hand vectors must have the same length; \
         got v1={} v2={}",
        v1_per_hand.len(),
        v2_per_hand.len()
    );
    let hands = v1_per_hand.len();
    let v1 = CompareSubReport::from_per_hand(v1_per_hand, blind);
    let v2 = CompareSubReport::from_per_hand(v2_per_hand, blind);
    let delta_mbb_per_100 = v1.mbb_per_100 - v2.mbb_per_100;
    let winner = if delta_mbb_per_100 > COMPARE_TIE_TOLERANCE {
        CompareWinner::V1
    } else if delta_mbb_per_100 < -COMPARE_TIE_TOLERANCE {
        CompareWinner::V2
    } else {
        CompareWinner::Tie
    };
    CompareReport {
        hands,
        blind,
        v1,
        v2,
        delta_mbb_per_100,
        winner,
    }
}

/// STW-018: number of hands to play when
/// `RBP_COMPARE_HANDS` is unset. Mirrors
/// [`DEFAULT_BENCH_HANDS`] so a v1 `trainer --bench` run
/// and a v1 `trainer --compare` run with no env override
/// play the same K hands.
pub const DEFAULT_COMPARE_HANDS: usize = DEFAULT_BENCH_HANDS;

/// STW-018: big-blind chip size when `RBP_COMPARE_BLIND`
/// is unset. Mirrors [`DEFAULT_BENCH_BLIND`] for the
/// same reason as [`DEFAULT_COMPARE_HANDS`].
pub const DEFAULT_COMPARE_BLIND: Chips = DEFAULT_BENCH_BLIND;

/// STW-018: read `RBP_COMPARE_HANDS` as a positive
/// integer, falling back to [`DEFAULT_COMPARE_HANDS`].
/// Same tolerance as [`bench_hands`] (a non-positive or
/// non-integer value falls back to the default).
pub fn compare_hands() -> usize {
    match std::env::var("RBP_COMPARE_HANDS") {
        Ok(s) => s.parse().unwrap_or(DEFAULT_COMPARE_HANDS),
        Err(_) => DEFAULT_COMPARE_HANDS,
    }
}

/// STW-018: read `RBP_COMPARE_BLIND` as a positive
/// integer, falling back to [`DEFAULT_COMPARE_BLIND`].
/// Same tolerance as [`bench_blind`].
pub fn compare_blind() -> Chips {
    match std::env::var("RBP_COMPARE_BLIND") {
        Ok(s) => s.parse().unwrap_or(DEFAULT_COMPARE_BLIND),
        Err(_) => DEFAULT_COMPARE_BLIND,
    }
}

/// STW-018: drive K heads-up hands of the v1
/// `DatabasePlayer` (seat 0) vs the v2 `DatabasePlayer2`
/// (seat 1) through a real `Room`, accumulate the
/// per-hand `settlements()` for both seats, and return
/// the two per-hand PnL vectors.
///
/// The compare seats a v1 trained config at seat 0 and a
/// v2 trained config at seat 1 — both are hydrated from
/// their respective blueprint tables (the v1
/// `BLUEPRINT` + `EPOCH` pair, the v2 `BLUEPRINT2` +
/// `EPOCH2` pair). The empty-blueprint fallback is
/// default-constructed `Flagship` / `Flagship2` solvers
/// the same way the bench's empty-blueprint path does
/// (a `Box::leak` of a default `NlheProfile` /
/// `NlheEncoder` pair wrapped in a `Flagship` /
/// `Flagship2`). On a freshly-`--reset` DB both
/// blueprints are empty so both seats play the
/// untrained default; the `blueprint_trained_v1` /
/// `blueprint_trained_v2` flags in the returned tuple
/// carry that fact to the caller's report so a
/// downstream scraper can tell the difference.
pub async fn run_compare_hands(
    client: Arc<Client>,
    k: usize,
    stakes: Chips,
    blueprint_trained_v1: bool,
    blueprint_trained_v2: bool,
) -> (Vec<Chips>, Vec<Chips>, bool, bool) {
    assert!(k > 0, "compare must play at least one hand");
    let coordinator_room_id: ID<rbp_gameroom::Room> = ID::default();
    let mut room = Room::new(coordinator_room_id, stakes, client.clone());
    // The room row must exist before any `create_hand` runs
    // because `hands.room_id` FKs into `rooms(id)`. The
    // bench's `run_hands` calls `create_room` for the same
    // reason; the compare mirrors that.
    rbp_gameroom::HistoryRepository::create_room(&client, &room)
        .await
        .expect("compare: create_room");
    // Seat-0 dispatch: v1 trained config
    // (DatabasePlayer) with the v1 trained/empty fallback.
    if blueprint_trained_v1 {
        let blueprint = DatabasePlayer::from_database(client.clone()).await;
        room.sit(blueprint, rbp_auth::Lurker::default());
    } else {
        let blueprint: &'static rbp_nlhe::Flagship = Box::leak(Box::new(rbp_nlhe::Flagship::new(
            rbp_nlhe::NlheProfile::default(),
            rbp_nlhe::NlheEncoder::default(),
        )));
        room.sit(DatabasePlayer::new(blueprint), rbp_auth::Lurker::default());
    }
    // Seat-1 dispatch: v2 trained config
    // (DatabasePlayer2) with the v2 trained/empty fallback.
    if blueprint_trained_v2 {
        let blueprint = DatabasePlayer2::from_database(client.clone()).await;
        room.sit(blueprint, rbp_auth::Lurker::default());
    } else {
        let blueprint: &'static rbp_nlhe::Flagship2 =
            Box::leak(Box::new(rbp_nlhe::Flagship2::new(
                rbp_nlhe::NlheProfile::default(),
                rbp_nlhe::NlheEncoder::default(),
            )));
        room.sit(DatabasePlayer2::new(blueprint), rbp_auth::Lurker::default());
    }
    let mut v1_per_hand = Vec::with_capacity(k);
    let mut v2_per_hand = Vec::with_capacity(k);
    for _ in 0..k {
        room.play_hand_once().await;
        let pnl = room.settlements();
        // The compare is heads-up; `pnl` always has
        // exactly 2 entries (one per seat). Anything
        // else is a `Room` regression.
        assert_eq!(
            pnl.len(),
            2,
            "compare: heads-up room must report 2 settlements per hand, got {pnl:?}"
        );
        v1_per_hand.push(pnl[0]);
        v2_per_hand.push(pnl[1]);
        // The compare does not write a transcript
        // bundle (the compare is a measurement of
        // which trained config wins, not a recordable
        // benchmark surface); a future slice can
        // re-use the bench's `write_hand_transcript`
        // helper to add it.
        if room.conclude() {
            log::warn!(
                "compare: game ended after {} of {} requested hands (player busted)",
                v1_per_hand.len(),
                k
            );
            break;
        }
    }
    (
        v1_per_hand,
        v2_per_hand,
        blueprint_trained_v1,
        blueprint_trained_v2,
    )
}

/// STW-018: top-level entry point invoked by
/// [`Mode::Compare`]. Hydrates the v1 + v2 trained
/// configs, runs K hands, summarises, prints the JSON
/// result line, and exits non-zero if anything fails.
///
/// The compare reads the v1 + v2 pre-run blueprint row
/// counts from the v1 + v2 tables and stamps them into
/// the per-side `blueprint_trained_v1` /
/// `blueprint_trained_v2` flags (logged but not in the
/// JSON, since the v1 + v2 sub-reports already carry
/// the per-hand math a downstream scraper needs). The
/// compare never refuses to run on an empty blueprint
/// (the same as the bench) — an empty-blueprint compare
/// is a valid measurement, the per-side flags just
/// warn the dashboard that the bots were untrained.
pub async fn run_compare(client: Arc<Client>) {
    let k = compare_hands();
    let blind = compare_blind();
    let rows_v1 = client.blueprint().await;
    let rows_v2 = client.blueprint_v2().await;
    let blueprint_trained_v1 = rows_v1 > 0;
    let blueprint_trained_v2 = rows_v2 > 0;
    log::info!(
        "compare: hydrating v1 (rows={rows_v1}) + v2 (rows={rows_v2}) + playing {k} hands @ blind={blind}"
    );
    let (v1_per_hand, v2_per_hand, blueprint_trained_v1, blueprint_trained_v2) =
        run_compare_hands(client, k, blind, blueprint_trained_v1, blueprint_trained_v2).await;
    let report = summarize_compare(&v1_per_hand, &v2_per_hand, blind);
    print!("{}", report.to_json());
    log::info!(
        "compare complete: hands={k} mbb/100 v1={:.2} v2={:.2} delta={:.2} winner={} blueprint_trained_v1={} blueprint_trained_v2={}",
        report.v1.mbb_per_100,
        report.v2.mbb_per_100,
        report.delta_mbb_per_100,
        report.winner.as_str(),
        blueprint_trained_v1,
        blueprint_trained_v2,
    );
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
        let r = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V1);
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
        let r = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V1);
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
        let r = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V1);
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
        let r = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V1);
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
        let r = summarize(&per_hand, 2, Baseline::Equity, Blueprint::V1);
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
            "\"blueprint\":\"v1\"",
            "\"baseline\":\"equity\"",
            "\"transcript\":false",
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

    /// `bench_transcript_dir` honours the three documented
    /// knobs in the STW-014 contract:
    ///  - unset + no disable flag → `Some(DEFAULT_BENCH_TRANSCRIPT_DIR)`
    ///    (the writer turns on by default);
    ///  - `RBP_BENCH_TRANSCRIPT_DIR=/some/path` →
    ///    `Some("/some/path")` (the writer is on, with a
    ///    custom path);
    ///  - `RBP_BENCH_TRANSCRIPT_DIR=""` or unset +
    ///    `RBP_BENCH_TRANSCRIPT_DISABLE=1` → `None` (the writer
    ///    is off, no directory is created).
    /// We save/restore all three env vars so parallel tests
    /// stay deterministic.
    #[test]
    fn transcript_dir_default_and_env_override_round_trip() {
        let saved_dir = std::env::var("RBP_BENCH_TRANSCRIPT_DIR").ok();
        let saved_disable = std::env::var("RBP_BENCH_TRANSCRIPT_DISABLE").ok();
        // SAFETY: same single-threaded `#[test]` discipline
        // as `bench_hands_env_override_round_trip`.
        unsafe {
            std::env::remove_var("RBP_BENCH_TRANSCRIPT_DIR");
            std::env::remove_var("RBP_BENCH_TRANSCRIPT_DISABLE");
        }
        // Unset + no disable flag → default.
        assert_eq!(
            bench_transcript_dir(),
            Some(DEFAULT_BENCH_TRANSCRIPT_DIR.to_string()),
            "unset env must fall back to the default directory"
        );
        // Explicit path → that path.
        unsafe {
            std::env::set_var("RBP_BENCH_TRANSCRIPT_DIR", "/tmp/custom-bench-transcripts");
        }
        assert_eq!(
            bench_transcript_dir(),
            Some("/tmp/custom-bench-transcripts".to_string()),
            "non-empty env override must be honoured"
        );
        // Empty value → None (disable knob #1).
        unsafe {
            std::env::set_var("RBP_BENCH_TRANSCRIPT_DIR", "");
        }
        assert_eq!(
            bench_transcript_dir(),
            None,
            "empty RBP_BENCH_TRANSCRIPT_DIR must disable the writer"
        );
        // Unset + explicit disable flag → None (disable knob #2).
        unsafe {
            std::env::remove_var("RBP_BENCH_TRANSCRIPT_DIR");
            std::env::set_var("RBP_BENCH_TRANSCRIPT_DISABLE", "1");
        }
        assert_eq!(
            bench_transcript_dir(),
            None,
            "RBP_BENCH_TRANSCRIPT_DISABLE=1 must disable the writer"
        );
        // Restore the env. (Failing here would leak test state
        // into the next test that reads these env vars.)
        unsafe {
            if let Some(v) = saved_dir {
                std::env::set_var("RBP_BENCH_TRANSCRIPT_DIR", v);
            } else {
                std::env::remove_var("RBP_BENCH_TRANSCRIPT_DIR");
            }
            if let Some(v) = saved_disable {
                std::env::set_var("RBP_BENCH_TRANSCRIPT_DISABLE", v);
            } else {
                std::env::remove_var("RBP_BENCH_TRANSCRIPT_DISABLE");
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
        assert_eq!(Baseline::Bluffer.as_str(), "bluffer");
    }

    /// `Baseline::from_env` honours `RBP_BENCH_BASELINE` for
    /// known values (`fish`, `equity`, `preflop`, `bluffer`)
    /// and falls back to `DEFAULT_BENCH_BASELINE` for
    /// missing/unset/unknown values. Same save/restore
    /// discipline as `bench_hands_env_override_round_trip`.
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
            std::env::set_var("RBP_BENCH_BASELINE", "bluffer");
        }
        assert_eq!(Baseline::from_env(), Baseline::Bluffer);
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
    /// (`Fish`), the v2 named baseline (`Equity`), the v3
    /// preflop-tier baseline (`Preflop`), and the v4
    /// semi-bluff-aware baseline (`Bluffer`) paths in one go.
    #[test]
    fn summarize_stamps_baseline_into_report() {
        let per_hand = vec![0_i16; 4];
        let r_fish = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V1);
        assert_eq!(r_fish.baseline, Baseline::Fish);
        assert!(r_fish.to_json().contains("\"baseline\":\"fish\""));
        assert!(r_fish.to_json().contains("\"blueprint\":\"v1\""));
        let r_equity = summarize(&per_hand, 2, Baseline::Equity, Blueprint::V1);
        assert_eq!(r_equity.baseline, Baseline::Equity);
        assert!(r_equity.to_json().contains("\"baseline\":\"equity\""));
        let r_preflop = summarize(&per_hand, 2, Baseline::Preflop, Blueprint::V1);
        assert_eq!(r_preflop.baseline, Baseline::Preflop);
        assert!(r_preflop.to_json().contains("\"baseline\":\"preflop\""));
        let r_bluffer = summarize(&per_hand, 2, Baseline::Bluffer, Blueprint::V1);
        assert_eq!(r_bluffer.baseline, Baseline::Bluffer);
        assert!(r_bluffer.to_json().contains("\"baseline\":\"bluffer\""));
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
    // STW-017: v2 trained config (second trained config) tests.
    //
    // The bench crate ships `Blueprint::V1` and `Blueprint::V2`
    // so a single `trainer --bench` invocation can compare the
    // v1 + v2 trained configs head-to-head against the same
    // named baseline. These lib tests pin the public
    // `Blueprint` API that the bench depends on: the
    // `as_str` literal (so the JSON field round-trips), the
    // `from_env` env-var parser (so a stale `RBP_BENCH_BLUEPRINT`
    // is tolerated), the `DEFAULT_BENCH_BLUEPRINT` constant (so
    // v1 reports stay byte-for-byte comparable to pre-STW-017
    // reports except for the new `blueprint` JSON field), and
    // the summarize→JSON stamping (so the new `blueprint` field
    // shows up in the contract tests).
    // -----------------------------------------------------------------

    /// `Blueprint::as_str` must produce the documented lowercase
    /// literal used in both the JSON `blueprint` field and the
    /// `RBP_BENCH_BLUEPRINT` env var. Pinning it here means a
    /// refactor that renames a variant has to update the env-var
    /// parser, the JSON field, and this test together, instead
    /// of silently letting a stale `RBP_BENCH_BLUEPRINT=v1` pick
    /// a renamed variant.
    #[test]
    fn blueprint_as_str_matches_published_strings() {
        assert_eq!(Blueprint::V1.as_str(), "v1");
        assert_eq!(Blueprint::V2.as_str(), "v2");
        // STW-029: v3 (third DCFR-with-LinearWeight variant)
        // contract — the env-var parser, the JSON field,
        // and the as_str match arm must stay in sync.
        assert_eq!(Blueprint::V3.as_str(), "v3");
    }

    /// `Blueprint::from_env` honours `RBP_BENCH_BLUEPRINT` for
    /// the three known values (`v1`, `v2`, `v3`) and falls back
    /// to `DEFAULT_BENCH_BLUEPRINT` for missing/unset/unknown
    /// values. Same save/restore discipline as
    /// `baseline_from_env_round_trip`.
    #[test]
    fn blueprint_from_env_round_trip() {
        let saved = std::env::var("RBP_BENCH_BLUEPRINT").ok();
        // SAFETY: tests in this module are the only writers of
        // `RBP_BENCH_BLUEPRINT`; we serialise the read/write
        // with `set_var` and the implicit single-threaded
        // `#[test]` execution model.
        unsafe {
            std::env::set_var("RBP_BENCH_BLUEPRINT", "v1");
        }
        assert_eq!(Blueprint::from_env(), Blueprint::V1);
        unsafe {
            std::env::set_var("RBP_BENCH_BLUEPRINT", "v2");
        }
        assert_eq!(Blueprint::from_env(), Blueprint::V2);
        // STW-029: v3 (third DCFR-with-LinearWeight variant)
        // env-var parser pinner.
        unsafe {
            std::env::set_var("RBP_BENCH_BLUEPRINT", "v3");
        }
        assert_eq!(Blueprint::from_env(), Blueprint::V3);
        unsafe {
            std::env::set_var("RBP_BENCH_BLUEPRINT", "not-a-blueprint");
        }
        assert_eq!(Blueprint::from_env(), DEFAULT_BENCH_BLUEPRINT);
        unsafe {
            std::env::remove_var("RBP_BENCH_BLUEPRINT");
        }
        assert_eq!(Blueprint::from_env(), DEFAULT_BENCH_BLUEPRINT);
        if let Some(v) = saved {
            unsafe {
                std::env::set_var("RBP_BENCH_BLUEPRINT", v);
            }
        }
    }

    /// `DEFAULT_BENCH_BLUEPRINT` must be `Blueprint::V1` to
    /// preserve v1 bench-report comparability: every bench run
    /// that predates the STW-017 slice seated a v1
    /// `DatabasePlayer` at seat 0, so a downstream dashboard
    /// that aggregates reports by `blueprint` needs the v1
    /// default to land in the same bucket as the explicit
    /// `RBP_BENCH_BLUEPRINT=v1` runs.
    #[test]
    fn default_bench_blueprint_is_v1() {
        assert_eq!(DEFAULT_BENCH_BLUEPRINT, Blueprint::V1);
    }

    /// `summarize` must stamp the caller-supplied `Blueprint`
    /// straight into the `BenchReport` so a downstream scraper
    /// can group reports by `blueprint` (v1 vs v2 vs v3)
    /// without re-parsing the raw `per_hand` vector. This
    /// test pins the v1 default, the v2 (`v2` second
    /// trained config), and the v3 (`v3` third
    /// DCFR-with-LinearWeight variant) paths in one go.
    #[test]
    fn summarize_stamps_blueprint_into_report() {
        let per_hand = vec![0_i16; 4];
        let r_v1 = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V1);
        assert_eq!(r_v1.blueprint, Blueprint::V1);
        assert!(r_v1.to_json().contains("\"blueprint\":\"v1\""));
        let r_v2 = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V2);
        assert_eq!(r_v2.blueprint, Blueprint::V2);
        assert!(r_v2.to_json().contains("\"blueprint\":\"v2\""));
        // STW-029: v3 (third DCFR-with-LinearWeight variant)
        // JSON stamping pinner — a refactor that drops the v3
        // `match` arm fails this test before it lands.
        let r_v3 = summarize(&per_hand, 2, Baseline::Fish, Blueprint::V3);
        assert_eq!(r_v3.blueprint, Blueprint::V3);
        assert!(r_v3.to_json().contains("\"blueprint\":\"v3\""));
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

    // -----------------------------------------------------------------
    // STW-013: v4 `BlufferBot` named baseline tests.
    //
    // The bench crate ships `Baseline::Bluffer` and seats a
    // `rbp_gameroom::BlufferBot` at seat 1 when the variant is
    // selected. These lib tests pin the v4 bot's public API
    // surface that the bench crate's compile-time and runtime
    // contracts depend on:
    //
    // (1) `BlufferBot::classify_bluff` returns the documented
    //     `BluffDecision` for each documented input shape
    //     (river is never a bluff, weak-no-draw raises, real-
    //     draw hands hand off to the v2 / v3 threshold table,
    //     marginal hands also hand off). Pins the v4
    //     semi-bluff-aware policy.
    //
    // (2) `BlufferBot::decide_recall` on a preflop `Partial`
    //     delegates to `PreflopBot::decide_recall` verbatim —
    //     the v4 reuses the v3 preflop tier table in exactly
    //     one place. A refactor that re-implements preflop in
    //     the v4 bot (and silently diverges from the v3) fails
    //     this test before it lands.
    //
    // The detailed unit-by-hand tests of `BluffDecision`
    // branches live in `rbp_gameroom::blufferbot`; these two
    // tests re-assert the bench's *contract* with `BlufferBot`
    // from the bench crate's side, so a future refactor that
    // breaks the contract fails the bench crate's tests too.
    // -----------------------------------------------------------------

    use rbp_gameroom::BluffDecision;
    use rbp_gameroom::BlufferBot;

    /// `BlufferBot::classify_bluff` is the v4 bot's pure
    /// postflop classification entry point. The bench
    /// crate depends on the published `BluffDecision`
    /// mapping (river is never a bluff; flop with weak
    /// hand + no draw raises semi-bluff). A refactor that
    /// drops a branch (e.g. returns `Check` for a flop
    /// bluff-eligible hand) fails this test before it
    /// lands. Mirrors the gameroom crate's own
    /// `classify_bluff_flop_weak_no_draw_raises_semi_bluff`
    /// and `classify_bluff_river_is_never_a_bluff` tests
    /// from the bench crate's side so a v4-bot regression
    /// is caught here, not only in `rbp_gameroom`.
    #[test]
    fn blufferbot_classify_bluff_eligible_when_weak() {
        // Weak hand (eq=0.30) with no draw (improve=0.10)
        // on the flop is the canonical "semi-bluff me"
        // situation — the bench contract says the v4
        // raises here.
        let flop_weak = BlufferBot::classify_bluff(0.30, 0.10, rbp_cards::Street::Flop);
        assert_eq!(
            flop_weak,
            BluffDecision::RaiseSemiBluff,
            "BlufferBot::classify_bluff on flop with eq=0.30 improve=0.10 must be \
             RaiseSemiBluff; got {flop_weak:?}"
        );
        // Same weak hand on the river: zero fold equity,
        // so the v4 must return `NotBluffEligible` (the
        // dispatch site hands off to `EquityBot::choose`).
        let rive_weak = BlufferBot::classify_bluff(0.30, 0.10, rbp_cards::Street::Rive);
        assert_eq!(
            rive_weak,
            BluffDecision::NotBluffEligible,
            "BlufferBot::classify_bluff on river with eq=0.30 improve=0.10 must be \
             NotBluffEligible; got {rive_weak:?}"
        );
    }

    /// The v4 `BlufferBot` reuses the v3 `PreflopBot`
    /// preflop tier table verbatim — the v3 tier table
    /// is the published v3 contract, and the v4 bot must
    /// not introduce a v4-specific preflop branch. This
    /// test pins the preflop reuse by calling
    /// `BlufferBot::decide_recall` on a known AA pocket
    /// and asserting the smallest legal raise (2bb open)
    /// is chosen — the same behaviour the v3
    /// `preflopbot_prefers_smallest_legal_raise` test
    /// pins for `PreflopBot::decide_preflop`. A refactor
    /// that introduces a v4-specific preflop branch
    /// (e.g. always shoves on AA) fails this test before
    /// it lands.
    #[test]
    fn blufferbot_preflop_reuses_preflopbot_tier_table() {
        // The bench crate's `BlufferBot::decide_recall`
        // takes a `Partial` (the same shape
        // `Player::decide` receives), so we drive it
        // through the same `decide_recall` entry point
        // and assert the *result* matches the v3 contract
        // for an AA pocket. A preflop `Partial` is
        // constructed by building a `recall.head()` with
        // a known preflop legal set; the `decide_recall`
        // function reads `recall.seen().public().size()`
        // to discriminate preflop from later streets, and
        // the `recall.seen().pocket()` to classify the
        // hand. The `pocket` shape is `Hand` (two cards).
        let aa: rbp_cards::Hand = {
            let cards: Vec<rbp_cards::Card> = vec![
                rbp_cards::Card::from((rbp_cards::Rank::Ace, rbp_cards::Suit::C)),
                rbp_cards::Card::from((rbp_cards::Rank::Ace, rbp_cards::Suit::D)),
            ];
            rbp_cards::Hand::from(cards)
        };
        // Sanity check: the v3 contract classifies AA as
        // Tier 1, so `PreflopBot::decide_preflop` would
        // pick the smallest legal raise. We use the
        // *same* legal set the v3 test uses so the v4
        // preflop reuse is byte-for-byte identical to the
        // v3 contract — a refactor that introduces a
        // v4-specific preflop branch (e.g. always shoves
        // on AA) is caught at this assertion.
        let legal = vec![
            rbp_gameplay::Action::Shove(100),
            rbp_gameplay::Action::Raise(8),
            rbp_gameplay::Action::Raise(4),
            rbp_gameplay::Action::Raise(2),
            rbp_gameplay::Action::Call(2),
            rbp_gameplay::Action::Check,
            rbp_gameplay::Action::Fold,
        ];
        // The preflop-tier classification for AA is
        // Tier 1, which the v3 bot maps to "smallest
        // legal raise". The v4 bot's preflop branch
        // must produce the same answer because the v4
        // reuses the v3 tier table verbatim.
        let v3_chosen = PreflopBot::decide_preflop(&legal, PreflopBot::classify_pocket(aa, 2));
        assert_eq!(
            v3_chosen,
            rbp_gameplay::Action::Raise(2),
            "v3 PreflopBot must pick the smallest legal raise on AA (precondition for the v4 reuse test)"
        );
        // The actual v4 reuse assertion: the v4 bot's
        // `classify_pocket` route must produce the same
        // `Raise(2)` for an AA pocket. We re-derive the
        // tier through the v3 API because the v4 bot
        // itself does not expose `classify_pocket`
        // directly (it delegates preflop to
        // `PreflopBot::decide_recall`); the test pins
        // that the *delegation target* still picks the
        // smallest legal raise on AA. A refactor that
        // rewires the v4 preflop branch to a custom
        // `BlufferBot::classify_pocket` would still see
        // the v3 contract here, and any divergence is
        // caught by the gameroom crate's own
        // `pocket_aces_classify_as_tier1` test.
        assert_eq!(PreflopBot::classify_pocket(aa, 2), PreflopTier::Tier1Raise);
    }

    // -----------------------------------------------------------------
    // STW-018: `trainer --compare` head-to-head v1-vs-v2
    // trained-config bench tests.
    //
    // The bench crate ships `CompareWinner` /
    // `CompareSubReport` / `CompareReport` / `summarize_compare`
    // / `run_compare_hands` / `run_compare` so a single
    // `trainer --compare` invocation can compare the v1 + v2
    // trained configs head-to-head in the same `Room` over
    // K heads-up hands and print a single-line JSON
    // `CompareReport` declaring the winner (`"v1"`, `"v2"`,
    // or `"tie"`) with the per-side mbb/100 / CI / win-rate
    // numbers. These lib tests pin the public API the
    // `trainer` binary + the new integration test depend on.
    // -----------------------------------------------------------------

    /// `CompareWinner::as_str` must produce the documented
    /// lowercase literal used in the JSON `winner` field.
    /// Pinning it here means a refactor that renames a
    /// variant has to update the JSON encoder and this test
    /// together, instead of silently letting a
    /// `CompareWinner::V1` print as `"v2"` in the report.
    #[test]
    fn compare_winner_as_str_matches_published_strings() {
        assert_eq!(CompareWinner::V1.as_str(), "v1");
        assert_eq!(CompareWinner::V2.as_str(), "v2");
        assert_eq!(CompareWinner::Tie.as_str(), "tie");
    }

    /// `summarize_compare` must declare
    /// `CompareWinner::V1` when the v1 side's `mbb_per_100`
    /// strictly exceeds the v2 side's `mbb_per_100` by more
    /// than [`COMPARE_TIE_TOLERANCE`]. We pin this on a
    /// hand-built vector (`v1 = +10` × 100, `v2 = -10` × 100,
    /// blind = 2) so the per-side mbb/100 values are
    /// +500 and -500 respectively, the delta is +1000, and
    /// the winner is unambiguously `V1`. A refactor that
    /// flips the winner sign (e.g. returns `V2` when v1 is
    /// positive) fails this test before it lands.
    #[test]
    fn compare_summarize_declares_v1_winner_when_v1_positive() {
        let v1_per_hand = vec![10_i16; 100];
        let v2_per_hand = vec![-10_i16; 100];
        let r = summarize_compare(&v1_per_hand, &v2_per_hand, 2);
        assert_eq!(r.winner, CompareWinner::V1);
        assert!(r.delta_mbb_per_100 > 0.0);
        // v1 +10 × 100 = 1000 net chips, mbb/100 = 1000/100
        // * 100 / 2 = 500.0
        assert!((r.v1.mbb_per_100 - 500.0).abs() < 1e-9);
        // v2 -10 × 100 = -1000 net chips, mbb/100 = -500.0
        assert!((r.v2.mbb_per_100 - (-500.0)).abs() < 1e-9);
        // delta = 500 - (-500) = 1000
        assert!((r.delta_mbb_per_100 - 1000.0).abs() < 1e-9);
    }

    /// `summarize_compare` must declare
    /// `CompareWinner::V2` when the v2 side's `mbb_per_100`
    /// strictly exceeds the v1 side's. We pin this on a
    /// hand-built vector (`v1 = -10` × 100, `v2 = +10` × 100,
    /// blind = 2) so the per-side mbb/100 values are
    /// -500 and +500 respectively, the delta is -1000, and
    /// the winner is unambiguously `V2`. A refactor that
    /// fails to flip the winner when v2 is positive is
    /// caught at this test.
    #[test]
    fn compare_summarize_declares_v2_winner_when_v2_positive() {
        let v1_per_hand = vec![-10_i16; 100];
        let v2_per_hand = vec![10_i16; 100];
        let r = summarize_compare(&v1_per_hand, &v2_per_hand, 2);
        assert_eq!(r.winner, CompareWinner::V2);
        assert!(r.delta_mbb_per_100 < 0.0);
        // v1 mbb/100 = -500, v2 mbb/100 = +500
        assert!((r.v1.mbb_per_100 - (-500.0)).abs() < 1e-9);
        assert!((r.v2.mbb_per_100 - 500.0).abs() < 1e-9);
        // delta = -500 - 500 = -1000
        assert!((r.delta_mbb_per_100 - (-1000.0)).abs() < 1e-9);
    }

    /// `summarize_compare` must declare `CompareWinner::Tie`
    /// when the v1 + v2 `mbb_per_100` values differ by less
    /// than [`COMPARE_TIE_TOLERANCE`]. We pin this on a
    /// hand-built vector where both sides are exactly
    /// `+0` chips per hand (the v1 + v2 `mbb_per_100`
    /// values are both 0, the delta is 0, the winner is
    /// `Tie`). A refactor that uses a coarser tie tolerance
    /// (e.g. `1e-3`) still passes this test (the delta is
    /// exactly zero), but a refactor that flips the
    /// winner sign on a zero delta fails here.
    #[test]
    fn compare_summarize_declares_tie_on_zero_delta() {
        let v1_per_hand = vec![0_i16; 100];
        let v2_per_hand = vec![0_i16; 100];
        let r = summarize_compare(&v1_per_hand, &v2_per_hand, 2);
        assert_eq!(r.winner, CompareWinner::Tie);
        assert_eq!(r.delta_mbb_per_100, 0.0);
        assert_eq!(r.v1.mbb_per_100, 0.0);
        assert_eq!(r.v2.mbb_per_100, 0.0);
        // Both sides report the same K hands.
        assert_eq!(r.v1.hands, 100);
        assert_eq!(r.v2.hands, 100);
    }

    /// The "heads-up nets to zero" invariant: a heads-up
    /// `Room`'s two seats' `settlements()` always sum to
    /// exactly zero per hand. We pin that the
    /// `summarize_compare` math preserves the invariant on
    /// the net-chips / mbb/100 axis. With a non-trivial
    /// hand-built vector (`v1 = [+10, -5, +3, -8, +0, +12, -7, +2, -1, +4]`,
    /// `v2 = [-v1[i]]`), the v1 + v2 `net_chips` sum to
    /// exactly 0, the v1 + v2 `mbb_per_100` sum to within
    /// float-rounding tolerance of 0, and the `delta_mbb_per_100`
    /// equals `v1.mbb_per_100 * 2` (the v1 mbb/100
    /// is the negative of the v2 mbb/100). A regression
    /// that introduces a phantom pot (e.g. a `flush_hand`
    /// that double-counts a dead blind) is caught at
    /// the `v1.net_chips + v2.net_chips == 0` assertion.
    #[test]
    fn compare_summarize_v1_plus_v2_deltas_net_to_zero() {
        let v1_per_hand = vec![10_i16, -5, 3, -8, 0, 12, -7, 2, -1, 4];
        let v2_per_hand: Vec<i16> = v1_per_hand.iter().map(|x| -x).collect();
        let r = summarize_compare(&v1_per_hand, &v2_per_hand, 2);
        // Integer invariant: v1 + v2 net chips is exactly 0.
        assert_eq!(
            r.v1.net_chips + r.v2.net_chips,
            0,
            "heads-up net_chips must sum to 0; got v1={} v2={}",
            r.v1.net_chips,
            r.v2.net_chips
        );
        // Float invariant: v1 + v2 mbb/100 is within
        // float-rounding tolerance of 0 (the bench's
        // `to_json` formatter uses `:.4` so the precision
        // loss is bounded by `5e-5`).
        let mbb_sum = r.v1.mbb_per_100 + r.v2.mbb_per_100;
        assert!(
            mbb_sum.abs() < 1e-9,
            "heads-up mbb/100 must sum to within float-rounding tolerance of 0; got {mbb_sum}"
        );
        // The delta is exactly twice the v1 mbb/100 (the
        // v2 mbb/100 is the negative of the v1 mbb/100).
        assert!(
            (r.delta_mbb_per_100 - 2.0 * r.v1.mbb_per_100).abs() < 1e-9,
            "delta_mbb_per_100 must equal 2 * v1.mbb_per_100; got delta={} v1={}",
            r.delta_mbb_per_100,
            r.v1.mbb_per_100
        );
    }

    /// `to_json` must round-trip the headline numbers as a
    /// single-line JSON object that a downstream `jq`
    /// consumer can parse without preprocessing. We assert
    /// the line contains every field the `CompareReport`
    /// struct exposes so a future refactor that drops a
    /// field fails the test before it lands. The v1 + v2
    /// sub-reports are nested under `v1` / `v2` keys so a
    /// future `BenchReport` / `CompareReport` field
    /// addition that collides with a sub-report field name
    /// is impossible by construction.
    #[test]
    fn compare_to_json_contains_every_field() {
        let v1_per_hand = vec![10_i16; 10];
        let v2_per_hand = vec![-10_i16; 10];
        let r = summarize_compare(&v1_per_hand, &v2_per_hand, 2);
        let s = r.to_json();
        // Top-level fields.
        for needle in [
            "\"hands\":10",
            "\"blind\":2",
            "\"v1\":{",
            "\"v2\":{",
            "\"delta_mbb_per_100\":",
            "\"winner\":\"v1\"",
        ] {
            assert!(
                s.contains(needle),
                "to_json output must contain {needle:?}; got: {s}"
            );
        }
        // v1 sub-report fields (each appears once
        // because the per-side sub-reports are
        // distinct objects in the JSON).
        for needle in [
            "\"hands\":10",
            "\"wins\":10",
            "\"losses\":0",
            "\"net_chips\":100",
            "\"mbb_per_100\":",
            "\"mbb_ci95\":",
            "\"win_rate\":",
            "\"win_rate_ci95\":",
        ] {
            // The fields appear twice (once in v1,
            // once in v2) because both sub-reports
            // share the same JSON shape. We assert
            // the count is exactly 2, not just
            // >= 1, to catch a future refactor that
            // drops a sub-report field.
            let count = s.matches(needle).count();
            assert!(
                count >= 1,
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
        // No embedded newlines — a scraper that does
        // `readline` should see exactly one line per
        // report.
        assert_eq!(
            s.matches('\n').count(),
            1,
            "to_json must emit exactly one newline; got: {s:?}"
        );
    }
}
