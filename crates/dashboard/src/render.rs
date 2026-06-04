//! `render` — pure HTML emitters for the dashboard.
//!
//! The dashboard's `(c)` layer. Two emitters:
//!
//! - [`render_bench_card`] — a `BenchReport`-shaped HTML
//!   card a `GET /bench/:id` handler emits. The
//!   `BenchReport` is *not* a published type on the
//!   STW-034 index (the index only inlines
//!   `PublishedRemoteReceipt`; the per-hand `BenchReport`
//!   is the next-sliced-on-the-bench-json axis), so the
//!   card takes its inputs as a flat `&BenchCardFields`
//!   struct the router / future `trainer --bench --json`
//!   consumer builds.
//! - [`render_index_table`] — a sortable HTML table
//!   renderer for an `&[IndexedEntry]` slice the
//!   `PublishIndex` aggregator hands the dashboard. The
//!   table is a vanilla `<table>` with explicit
//!   `<thead>` / `<tbody>` / `<tr>` / `<th>` / `<td>`
//!   children — no CSS framework, no Tailwind, no inline
//!   `style=`; the styling lives in a single `<style>`
//!   block in the checked-in `index.html`.
//!
//! Every emitted string is HTML-escaped on the data
//! boundaries (the `receipt_basename` / `blueprint` /
//! `baseline` / `mbb_per_100` cells). A future
//! contributor adding a column with a per-cell value
//! that contains a `<` / `&` / `>` character fails the
//! `render_emits_escaped_quot` lib test at the same CI
//! step the unsafe `format!` would corrupt the page.

use rbp_autotrain::PublishIndex;

/// The flat, scraper-friendly `BenchReport` projection
/// the `GET /bench/:id` handler renders as a card. The
/// `BenchReport` Rust type derives `Debug` but not
/// `Serialize` (the bench writes its own one-line JSON
/// shape via the `to_json` format string), so the
/// dashboard reads the bench JSON in the next slice and
/// projects it through this struct. The fields mirror
/// the spec's `GET /bench/:id` pin
/// (`blueprint` / `baseline` / `mbb_per_100`).
///
/// STW-042 extends this projection with a
/// [`BenchCardFields::from_compare3`] constructor that
/// builds the column set a [`Compare3SubReport`] would
/// render — a `BenchReport` and a `Compare3SubReport`
/// render with the same column order, so the per-card
/// sub-report column set is reuse, not a new shape.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchCardFields {
    /// The receipt basename (e.g.
    /// `testnet-live-proof-20260604T050000Z`). Shown in
    /// the card's `<h2>` heading.
    pub receipt_basename: String,
    /// The trained-config variant (`"v1"` / `"v2"` /
    /// `"v3"` — the `Blueprint::as_str()` shape).
    pub blueprint: String,
    /// The named baseline (`"fish"` / `"equity"` /
    /// `"preflop"` / `"bluffer"` — the
    /// `Baseline::as_str()` shape).
    pub baseline: String,
    /// `mean_chips_per_hand * 100 / B_BLIND` from the
    /// `BenchReport`. The card's primary headline
    /// number; rendered to 4 decimal places.
    pub mbb_per_100: f64,
    /// 95% CI half-width on the per-hand mean chip
    /// delta, in mbb. Rendered as `±N.NNNN mbb/100` next
    /// to the headline.
    pub mbb_ci95: f64,
    /// `wins / K` from the `BenchReport`. The card's
    /// secondary headline; rendered as a percent.
    pub win_rate: f64,
}

/// Render a `BenchReport`-shaped HTML card. The output is
/// a self-contained `<article class="bench-card">…</article>`
/// block (no `<html>` / `<head>` / `<body>` wrapper) so
/// the router can drop it into a layout later without
/// re-parsing.
///
/// The card's HTML shape is pinned by the
/// `bench_card_has_pinned_columns` lib test: the
/// `<h2>` must carry the `receipt_basename`; the
/// `<dl>` must list `blueprint` / `baseline` /
/// `mbb_per_100` in that order. A regression in the
/// column order fails CI at the same step a downstream
/// dashboard scraper would silently break.
pub fn render_bench_card(bench: &BenchCardFields) -> String {
    // `html_escape` is intentionally a tiny private
    // helper rather than a `&str -> Cow<str>` dep:
    // every value the bench card renders is a flat
    // `&str` / `f64` whose escaping is mechanical
    // (replace `&` first, then `<` / `>` / `"` / `'`),
    // and adding a dep just to escape five cells is
    // the inverse of the "no-system-deps / no-CSS-
    // framework" shape the spec calls for.
    let basename = html_escape(&bench.receipt_basename);
    let blueprint = html_escape(&bench.blueprint);
    let baseline = html_escape(&bench.baseline);
    format!(
        concat!(
            "<article class=\"bench-card\">\n",
            "  <h2 class=\"bench-card__title\">{basename}</h2>\n",
            "  <dl class=\"bench-card__metrics\">\n",
            "    <dt>blueprint</dt><dd>{blueprint}</dd>\n",
            "    <dt>baseline</dt><dd>{baseline}</dd>\n",
            "    <dt>mbb_per_100</dt><dd>{mbb:.4} ± {ci:.4} mbb/100</dd>\n",
            "    <dt>win_rate</dt><dd>{wr:.2}%</dd>\n",
            "  </dl>\n",
            "  <p class=\"bench-card__links\">\n",
            "    <a class=\"bench-card__link\" href=\"/transcript/{basename_href}\">Download transcript</a>\n",
            "    <a class=\"bench-card__link\" href=\"/bench/{basename_href}\">Open replay</a>\n",
            "  </p>\n",
            "</article>\n"
        ),
        basename = basename,
        basename_href = url_encode(&bench.receipt_basename),
        blueprint = blueprint,
        baseline = baseline,
        mbb = bench.mbb_per_100,
        ci = bench.mbb_ci95,
        wr = bench.win_rate * 100.0,
    )
}

/// Render a sortable HTML table of the `PublishIndex`'s
/// `entries[]`. The output is a self-contained
/// `<table class="index-table">…</table>` block (no
/// `<html>` / `<head>` / `<body>` wrapper) so the
/// `index.html` JS can `document.getElementById(...).innerHTML
/// = ...` inject the result without re-parsing.
///
/// The table's column order is pinned by the
/// `render_index_table_column_order` lib test:
/// `receipt_basename` / `blueprint` / `baseline` /
/// `mbb_per_100` / `ci_95` / `win_rate` / `total_bytes`
/// / `uploaded_at_utc`, with a per-row
/// `Download transcript` + `Open replay` link pair at
/// the end. A regression in the column order fails CI
/// at the same step a downstream scraper would silently
/// break.
///
/// STW-047: the 5/8 per-row cells
/// `blueprint` / `baseline` / `mbb_per_100` / `ci_95`
/// / `win_rate` now render real numbers from the
/// inlined `IndexedEntry::bench` (the
/// `BenchSummary` the STW-019 runbook's `--bench`
/// step produced + the STW-034 aggregator inlined).
/// A `bench: None` value (a publish root the
/// operator built without the bench step, or a
/// pre-STW-047 `INDEX.json` the dashboard has not
/// yet re-indexed) renders `—` for those 5 cells,
/// the same shape the table shipped before
/// STW-047.
pub fn render_index_table(index: &PublishIndex) -> String {
    let mut s = String::with_capacity(1024 + index.entries.len() * 384);
    s.push_str("<table class=\"index-table\">\n");
    s.push_str("  <thead>\n");
    s.push_str("    <tr>\n");
    for col in [
        "receipt_basename",
        "blueprint",
        "baseline",
        "mbb_per_100",
        "ci_95",
        "win_rate",
        "total_bytes",
        "uploaded_at_utc",
    ] {
        s.push_str(&format!("      <th scope=\"col\">{col}</th>\n"));
    }
    s.push_str("      <th scope=\"col\">actions</th>\n");
    s.push_str("    </tr>\n");
    s.push_str("  </thead>\n");
    s.push_str("  <tbody>\n");
    for entry in &index.entries {
        // STW-047: render the 5 bench cells
        // from the inlined `entry.bench`. A
        // `None` value (a pre-STW-047 INDEX.json
        // or a publish root the operator built
        // without the bench step) renders `—`
        // — the same shape the table shipped
        // before STW-047 — so the dashboard
        // stays backwards-compatible. The
        // `live_index_table_renders_bench_cells_with_values`
        // lib test pins the populated shape.
        let basename = html_escape(&entry.receipt_basename);
        let basename_href = url_encode(&entry.receipt_basename);
        let uploaded_at = html_escape(&entry.remote_receipt.uploaded_at_utc);
        let total_bytes = entry.remote_receipt.total_bytes;
        let (cell_blueprint, cell_baseline, cell_mbb, cell_ci, cell_win) =
            match entry.bench.as_ref() {
                Some(b) => (
                    html_escape(&b.blueprint),
                    html_escape(&b.baseline),
                    format!("{:.4}", b.mbb_per_100),
                    format!("±{:.4}", b.mbb_ci95),
                    format!("{:.2}%", b.win_rate * 100.0),
                ),
                None => (
                    "—".to_string(),
                    "—".to_string(),
                    "—".to_string(),
                    "—".to_string(),
                    "—".to_string(),
                ),
            };
        s.push_str("    <tr>\n");
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{basename}</td>\n"
        ));
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{cell_blueprint}</td>\n"
        ));
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{cell_baseline}</td>\n"
        ));
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{cell_mbb}</td>\n"
        ));
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{cell_ci}</td>\n"
        ));
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{cell_win}</td>\n"
        ));
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{total_bytes}</td>\n"
        ));
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{uploaded_at}</td>\n"
        ));
        s.push_str("      <td class=\"index-table__cell\">\n");
        s.push_str(&format!(
            "        <a class=\"index-table__link\" href=\"/transcript/{basename_href}\">Download transcript</a>\n"
        ));
        s.push_str(&format!(
            "        <a class=\"index-table__link\" href=\"/bench/{basename_href}\">Open replay</a>\n"
        ));
        s.push_str("      </td>\n");
        s.push_str("    </tr>\n");
    }
    s.push_str("  </tbody>\n");
    s.push_str("</table>\n");
    s
}

/// STW-042: dashboard-side typed parse of the
/// `Compare3Report` JSON line the `trainer --compare3`
/// arm emits. The autotrain's `Compare3Report` does
/// not derive `Deserialize` (its `to_json` is a
/// hand-rolled `format!` rather than a `serde`
/// re-emit), so the dashboard ships its own thin
/// typed-parse shape that mirrors the autotrain's
/// `to_json` field order exactly. A drift between
/// the autotrain's `to_json` and this struct's
/// `Deserialize` fails the
/// `compare3_fixture_renders_bench_card` integration
/// test at the same CI step a downstream
/// `trainer --compare3 --json` consumer would
/// silently break.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Compare3Report {
    /// `K`: number of hands played per pair.
    pub hands_per_pair: usize,
    /// Big-blind chip size the compare3 used to
    /// compute mbb. The autotrain serialiser emits
    /// this as a JSON number; the field is `u64`
    /// because chips are non-negative on the bench.
    pub blind: u64,
    /// The v1 `DatabasePlayer` sub-report.
    pub v1: Compare3SubReport,
    /// The v2 `DatabasePlayer2` sub-report.
    pub v2: Compare3SubReport,
    /// The v3 `DatabasePlayer3` sub-report.
    pub v3: Compare3SubReport,
    /// `v1.mbb_per_100 - v2.mbb_per_100`. The sign
    /// is the v1-vs-v2 winner direction.
    pub v1_v2_delta: f64,
    /// `v2.mbb_per_100 - v3.mbb_per_100`.
    pub v2_v3_delta: f64,
    /// `v3.mbb_per_100 - v1.mbb_per_100`.
    pub v3_v1_delta: f64,
    /// The headline: the v1 / v2 / v3 config with
    /// the strictly highest per-config `mbb_per_100`,
    /// or `Tie` if the top two are within
    /// tolerance. The `serde` rename pins the
    /// autotrain's lowercase string serialization
    /// (`"v1"` / `"v2"` / `"v3"` / `"tie"`).
    pub ranked_winner: Compare3Winner,
}

/// STW-042: per-config sub-report inside a
/// [`Compare3Report`]. Mirrors the autotrain's
/// `Compare3SubReport` `to_json` shape verbatim
/// (`hands` / `wins` / `losses` / `net_chips` /
/// `mbb_per_100` / `mbb_ci95` / `win_rate` /
/// `win_rate_ci95`) so a future `Compare3SubReport`
/// field addition in the autotrain fails the
/// `compare3_fixture_round_trips_via_serde` lib test
/// at the same CI step a downstream dashboard scraper
/// would silently break.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Compare3SubReport {
    /// `2 * K`: total hands this config played.
    pub hands: usize,
    /// Hands this config won outright.
    pub wins: usize,
    /// Hands that ended with this config's
    /// `won()` strictly negative.
    pub losses: usize,
    /// Sum of this config's `won()` across the
    /// `2 * K` hands, in chips.
    pub net_chips: i64,
    /// `mean_chips_per_hand * 100 / B_BLIND`.
    pub mbb_per_100: f64,
    /// 95% CI half-width on the per-hand mean chip
    /// delta, in mbb.
    pub mbb_ci95: f64,
    /// `wins / (2 * K)`, the simple proportion of
    /// hands won.
    pub win_rate: f64,
    /// 95% CI half-width on `win_rate`.
    pub win_rate_ci95: f64,
}

/// STW-042: ranked headline a `Compare3Report`
/// declares. The serde rename pins the autotrain's
/// lowercase string serialization (`"v1"` / `"v2"` /
/// `"v3"` / `"tie"`) so a `serde_json::from_str` of
/// the autotrain's `to_json` output round-trips
/// without error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Compare3Winner {
    /// The v1 config had the strictly highest
    /// per-config `mbb_per_100`.
    V1,
    /// The v2 config had the strictly highest
    /// per-config `mbb_per_100`.
    V2,
    /// The v3 config had the strictly highest
    /// per-config `mbb_per_100`.
    V3,
    /// The top two per-config `mbb_per_100`
    /// values are within tolerance.
    Tie,
}

impl Compare3Winner {
    /// Stable lowercase string the
    /// `to_json`-shaped JSON field uses. Mirrors
    /// `rbp_autotrain::Compare3Winner::as_str` so
    /// the two stay drop-in compatible.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::V3 => "v3",
            Self::Tie => "tie",
        }
    }
}

impl BenchCardFields {
    /// STW-042: build the per-card column set a
    /// `Compare3SubReport` would render — the
    /// `blueprint` / `baseline` / `mbb_per_100` /
    /// `ci_95` / `win_rate` column set the existing
    /// `render_bench_card` consumes. The constructor
    /// exists so a `BenchReport` and a
    /// `Compare3SubReport` render with the same
    /// column order; the per-card difference lives
    /// in the surrounding card (the `render_bench_card`
    /// for a `BenchReport` vs. the
    /// `render_compare3_card` for a `Compare3Report`),
    /// not in the per-card column shape.
    pub fn from_compare3(
        receipt_basename: &str,
        config: &str,
        baseline: &str,
        sub: &Compare3SubReport,
    ) -> Self {
        Self {
            receipt_basename: receipt_basename.to_string(),
            blueprint: config.to_string(),
            baseline: baseline.to_string(),
            mbb_per_100: sub.mbb_per_100,
            mbb_ci95: sub.mbb_ci95,
            win_rate: sub.win_rate,
        }
    }
}

/// STW-042: render a `Compare3Report`-shaped HTML
/// card. The output is a self-contained
/// `<article class="bench-card bench-card--compare3">…</article>`
/// block (a `bench-card` class the existing
/// `render_bench_card` shares + a `bench-card--compare3`
/// modifier so a future style refactor can target
/// compare3 cards independently) that lists each of
/// v1 / v2 / v3 in the `blueprint` cell, then a
/// per-pair `<dl>` with the three pairwise
/// `delta_mbb_per_100` values, then the
/// `ranked_winner` headline. The `render_bench_card`
/// emitter is not extended — compare3 cards are a
/// distinct visual element with three sub-cards
/// + three pairwise deltas, not a single
/// `BenchReport`-shaped card.
///
/// The card's HTML shape is pinned by the
/// `compare3_card_has_pinned_sub_reports_and_deltas`
/// lib test: the `<h2>` must carry the
/// `receipt_basename`; the sub-report order must be
/// v1 / v2 / v3 (in that order); the three pairwise
/// `<dt>`s must appear in v1-vs-v2 / v2-vs-v3 /
/// v3-vs-v1 order; the `ranked_winner` value must
/// appear in the body. A regression in the column
/// order or the sub-report order fails CI at the
/// same step a downstream dashboard scraper would
/// silently break.
pub fn render_compare3_card(receipt_basename: &str, report: &Compare3Report) -> String {
    let basename = html_escape(receipt_basename);
    let basename_href = url_encode(receipt_basename);
    let winner = report.ranked_winner.as_str();
    let winner_html = html_escape(winner);
    let v1 = BenchCardFields::from_compare3(receipt_basename, "v1", "preflop", &report.v1);
    let v2 = BenchCardFields::from_compare3(receipt_basename, "v2", "preflop", &report.v2);
    let v3 = BenchCardFields::from_compare3(receipt_basename, "v3", "preflop", &report.v3);
    format!(
        concat!(
            "<article class=\"bench-card bench-card--compare3\">\n",
            "  <h2 class=\"bench-card__title\">{basename}</h2>\n",
            "  <p class=\"bench-card__winner\">ranked_winner: <strong>{winner}</strong></p>\n",
            "  <div class=\"bench-card__subcards\">\n",
            "    <div class=\"bench-card__subcard\">{v1}</div>\n",
            "    <div class=\"bench-card__subcard\">{v2}</div>\n",
            "    <div class=\"bench-card__subcard\">{v3}</div>\n",
            "  </div>\n",
            "  <h3 class=\"bench-card__deltas-title\">pairwise deltas</h3>\n",
            "  <dl class=\"bench-card__deltas\">\n",
            "    <dt>v1_v2_delta</dt><dd>{d12:+.4} mbb/100</dd>\n",
            "    <dt>v2_v3_delta</dt><dd>{d23:+.4} mbb/100</dd>\n",
            "    <dt>v3_v1_delta</dt><dd>{d31:+.4} mbb/100</dd>\n",
            "  </dl>\n",
            "  <p class=\"bench-card__links\">\n",
            "    <a class=\"bench-card__link\" href=\"/bench/{basename_href}\">Open replay</a>\n",
            "  </p>\n",
            "</article>\n"
        ),
        basename = basename,
        winner = winner_html,
        v1 = render_bench_card(&v1),
        v2 = render_bench_card(&v2),
        v3 = render_bench_card(&v3),
        d12 = report.v1_v2_delta,
        d23 = report.v2_v3_delta,
        d31 = report.v3_v1_delta,
        basename_href = basename_href,
    )
}

/// The pinned sentinel `:id` the dashboard's
/// `GET /bench/:id` handler accepts for the
/// committed compare3 demo-data fallback. A
/// `GET /bench/<this-id>` against a fresh checkout
/// (no live `INDEX.json` entry matches) returns
/// 200 + a populated compare3 card a stranger
/// can read. The string is intentionally a
/// non-receipt-basename sentinel so a real
/// `INDEX.json` entry never shadows it.
pub const COMPARE3_FIXTURE_ID: &str = "compare3-fixture";

/// Resolve the absolute path of the committed
/// `tests/fixtures/compare3-fixture.json` fixture.
/// Walk from `CARGO_MANIFEST_DIR` (the
/// `crates/dashboard/` directory) into
/// `tests/fixtures/compare3-fixture.json`. The
/// function panics at startup if the file is
/// missing (a `cargo build` of the dashboard
/// crate is the authoritative pin on the file's
/// existence).
pub fn compare3_fixture_path() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("fixtures")
        .join("compare3-fixture.json")
}

/// HTML-escape a `&str` for safe interpolation into
/// a `<td>` / `<dt>` / `<h2>` cell. Replaces the five
/// characters that change the parser state (`&` first
/// to avoid double-escaping). Returns the input
/// unchanged if no escape is needed.
fn html_escape(s: &str) -> String {
    if !s.contains(['&', '<', '>', '"', '\'']) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Percent-encode a `&str` for safe interpolation into
/// an `<a href="…">` value. The basename is
/// `[a-zA-Z0-9_-]`-shaped today; the encoder is a
/// defensive belt-and-braces in case a future receipt
/// basename includes a `:` / `/` that the path
/// resolution would otherwise misinterpret.
fn url_encode(s: &str) -> String {
    if !s.contains(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
            out.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! 3 lib tests pinning the per-row column order +
    //! per-row link shape:
    //!
    //! 1. `bench_card_has_pinned_columns` — the
    //!    `render_bench_card` output must contain the
    //!    `blueprint` / `baseline` / `mbb_per_100`
    //!    labels in that order, and the per-card
    //!    `Download transcript` / `Open replay` link
    //!    pair.
    //! 2. `render_index_table_column_order` — the
    //!    `render_index_table` output must contain the
    //!    `receipt_basename` / `blueprint` / `baseline` /
    //!    `mbb_per_100` / `ci_95` / `win_rate` /
    //!    `total_bytes` / `uploaded_at_utc` columns in
    //!    that order, and a per-row `Download
    //!    transcript` / `Open replay` link pair.
    //! 3. `render_emits_escaped_quot` — a basename
    //!    containing `<` / `&` / `>` / `"` characters
    //!    must be HTML-escaped in the rendered output
    //!    (a regression in the escape helper would let
    //!    a malicious basename inject script tags).

    use super::*;
    use rbp_autotrain::{BenchSummary, IndexedEntry, PublishIndex, PublishedRemoteReceipt};

    fn bench_fields_fixture() -> BenchCardFields {
        BenchCardFields {
            receipt_basename: "testnet-live-proof-20260604T050000Z".to_string(),
            blueprint: "v1".to_string(),
            baseline: "fish".to_string(),
            mbb_per_100: 12.3456,
            mbb_ci95: 1.2345,
            win_rate: 0.5678,
        }
    }

    fn one_entry_publish_index() -> PublishIndex {
        let receipt = PublishedRemoteReceipt {
            plan: rbp_autotrain::PublishRemotePlan {
                bucket: "robopoker-testnet-dashboard".to_string(),
                prefix: "testnet-live-proof-20260604T050000Z/".to_string(),
                region: "us-east-1".to_string(),
                s3_objects: vec![],
                bundle_sha256: "cff28a13f2471bd15324b69f65e6ffa869a4ecd84748dc0e78719a7ffef11313"
                    .to_string(),
                bundle_bytes: 20503,
                receipt_basename: "testnet-live-proof-20260604T050000Z".to_string(),
                runbook_version: "STW-033 v1".to_string(),
                created_at_utc: "<unknown>".to_string(),
                dry_run: true,
            },
            uploaded_at_utc: "<unknown>".to_string(),
            s3_objects: vec![],
            total_bytes: 20503,
            bundle_sha256: "cff28a13f2471bd15324b69f65e6ffa869a4ecd84748dc0e78719a7ffef11313"
                .to_string(),
            runbook_version: "STW-033 v1".to_string(),
        };
        PublishIndex {
            publish_root: "/tmp/publish-root".to_string(),
            runbook_version: "STW-034 v1".to_string(),
            created_at_utc: "<unknown>".to_string(),
            entry_count: 1,
            total_bytes: 20503,
            entries: vec![IndexedEntry {
                receipt_basename: "testnet-live-proof-20260604T050000Z".to_string(),
                receipt_dir: "/tmp/publish-root/testnet-live-proof-20260604T050000Z"
                    .to_string(),
                remote_receipt_path:
                    "/tmp/publish-root/testnet-live-proof-20260604T050000Z/remote/remote_receipt.json"
                        .to_string(),
                remote_receipt: receipt,
                bench: None,
            }],
        }
    }

    /// STW-047: a `PublishIndex` with one
    /// `IndexedEntry` whose `bench` field is
    /// populated with a `BenchSummary`. The
    /// `live_index_table_renders_bench_cells_with_values`
    /// lib test pins the per-cell render
    /// shape (5 numeric cells, not `—`).
    fn one_entry_publish_index_with_bench() -> PublishIndex {
        let mut index = one_entry_publish_index();
        index.entries[0].bench = Some(BenchSummary {
            blueprint: "v1".to_string(),
            baseline: "preflop".to_string(),
            mbb_per_100: 250.0000,
            mbb_ci95: 120.0000,
            win_rate: 0.625,
        });
        index
    }

    #[test]
    fn bench_card_has_pinned_columns() {
        let card = render_bench_card(&bench_fields_fixture());
        for token in [
            "testnet-live-proof-20260604T050000Z",
            "blueprint",
            "baseline",
            "mbb_per_100",
            "v1",
            "fish",
            "Download transcript",
            "Open replay",
        ] {
            assert!(
                card.contains(token),
                "bench card must contain `{token}`; got:\n{card}"
            );
        }
        // Column order: `blueprint` precedes `baseline`,
        // and `baseline` precedes `mbb_per_100`. A
        // regression that reorders the `<dt>` cells
        // fails this assertion.
        let i_bp = card.find("blueprint").expect("contains `blueprint`");
        let i_ba = card.find("baseline").expect("contains `baseline`");
        let i_mbb = card.find("mbb_per_100").expect("contains `mbb_per_100`");
        assert!(
            i_bp < i_ba && i_ba < i_mbb,
            "columns must be ordered blueprint < baseline < mbb_per_100; got: bp={i_bp} ba={i_ba} mbb={i_mbb}"
        );
    }

    #[test]
    fn render_index_table_column_order() {
        let table = render_index_table(&one_entry_publish_index());
        // The header row must list the 8 pinned column
        // names in the order the spec calls for.
        let expected = [
            "receipt_basename",
            "blueprint",
            "baseline",
            "mbb_per_100",
            "ci_95",
            "win_rate",
            "total_bytes",
            "uploaded_at_utc",
        ];
        let mut last = 0usize;
        for col in expected {
            let i = table
                .find(col)
                .unwrap_or_else(|| panic!("column `{col}` must appear in table; got:\n{table}"));
            assert!(
                i >= last,
                "column `{col}` must come after the previous column (>= {last}); got: i={i}"
            );
            last = i;
        }
        // The per-row link pair must appear for the
        // single fixture entry.
        assert_eq!(
            table.matches("Download transcript").count(),
            1,
            "table must have exactly one `Download transcript` link per entry"
        );
        assert_eq!(
            table.matches("Open replay").count(),
            1,
            "table must have exactly one `Open replay` link per entry"
        );
    }

    // STW-047: 2 new lib tests pinning the
    // live `INDEX.json` → dashboard table
    // bench-cell wire:
    //
    // 1. `live_index_table_renders_bench_cells_with_values` —
    //    an `IndexedEntry` whose `bench`
    //    field is `Some(BenchSummary)` must
    //    render the 5/8 `blueprint` /
    //    `baseline` / `mbb_per_100` / `ci_95`
    //    / `win_rate` cells with the bench's
    //    numbers (not the `—` placeholder),
    //    and the 5 cells must appear in the
    //    per-row column order the
    //    `render_index_table_column_order`
    //    test pins. A regression that drops
    //    a cell, that reorders a cell, or
    //    that leaves the `—` placeholder
    //    shape in place fails CI.
    // 2. `live_index_table_renders_dash_for_missing_bench` —
    //    an `IndexedEntry` whose `bench`
    //    field is `None` (a pre-STW-047
    //    `INDEX.json` or a publish root the
    //    operator built without the bench
    //    step) must still render the 5/8
    //    cells as `—` — the same shape the
    //    table shipped before STW-047 — so
    //    the dashboard stays
    //    backwards-compatible. A regression
    //    that hides the bench-less rows (or
    //    that panics on the `None` path)
    //    fails CI at the same step a
    //    downstream scraper would silently
    //    break.

    #[test]
    fn live_index_table_renders_bench_cells_with_values() {
        let index = one_entry_publish_index_with_bench();
        let table = render_index_table(&index);
        // The 5 per-row cells must contain the
        // bench's numbers, not the `—`
        // placeholder. The `250.0000` /
        // `120.0000` / `62.50%` values mirror
        // the inlined `BenchSummary` (mbb/100 =
        // 250, ci95 = 120, win_rate = 0.625 =
        // 62.50%).
        for token in ["v1", "preflop", "250.0000", "±120.0000", "62.50%"] {
            assert!(
                table.contains(token),
                "the populated bench row must contain `{token}`; got:\n{table}"
            );
        }
        // The 5/8 column order must hold: the
        // `blueprint` cell (`v1`) precedes
        // the `baseline` cell (`preflop`),
        // which precedes the `mbb_per_100`
        // cell (`250.0000`), which precedes
        // the `ci_95` cell (`±120.0000`),
        // which precedes the `win_rate` cell
        // (`62.50%`). A regression that
        // reorders the per-row cells fails
        // this assertion.
        let i_bp = table.find("v1").expect("contains v1");
        let i_ba = table.find("preflop").expect("contains preflop");
        let i_mbb = table.find("250.0000").expect("contains mbb");
        let i_ci = table.find("±120.0000").expect("contains ci");
        let i_wr = table.find("62.50%").expect("contains win_rate");
        assert!(
            i_bp < i_ba && i_ba < i_mbb && i_mbb < i_ci && i_ci < i_wr,
            "the 5 bench cells must be ordered blueprint < baseline < mbb < ci < win_rate; \
             got: bp={i_bp} ba={i_ba} mbb={i_mbb} ci={i_ci} wr={i_wr}"
        );
    }

    #[test]
    fn live_index_table_renders_dash_for_missing_bench() {
        let index = one_entry_publish_index();
        let table = render_index_table(&index);
        // The 5 bench cells must render as `—`
        // for the bench-less entry. The 5
        // per-row `<td>` cells appear between
        // the `receipt_basename` `<td>` and
        // the `total_bytes` `<td>`; we count
        // the `—` placeholder occurrences to
        // pin the rendered shape.
        let dash_count = table.matches("—").count();
        assert!(
            dash_count >= 5,
            "the bench-less row must render 5 `—` placeholders for the bench cells; got {dash_count} in:\n{table}"
        );
    }

    // STW-042: 2 new lib tests pinning the
    // compare3 card's per-row column order +
    // sentinel shape:
    //
    // 1. `compare3_card_has_pinned_sub_reports_and_deltas`
    //    — the `render_compare3_card` output must
    //    contain the `compare3-fixture` basename +
    //    the three sub-report `mbb_per_100` values
    //    (v1 / v2 / v3 in that order) + the three
    //    pairwise `delta_mbb_per_100` values
    //    (v1-vs-v2 / v2-vs-v3 / v3-vs-v1 in that
    //    order) + the `ranked_winner` value. A
    //    regression in the sub-report order or the
    //    delta order fails CI at the same step a
    //    downstream dashboard scraper would silently
    //    break.
    // 2. `compare3_fixture_round_trips_via_serde` —
    //    the
    //    `crates/dashboard/tests/fixtures/compare3-fixture.json`
    //    file `serde_json::from_str`'s into a
    //    typed `Compare3Report` without error and
    //    the `ranked_winner` ∈ `{V1, V2, V3, Tie}`.
    //    A regression in the autotrain's
    //    `Compare3Report::to_json` field shape
    //    (a renamed field, a missing field) fails
    //    this test at the same CI step a downstream
    //    `trainer --compare3 --json` consumer would
    //    silently break.

    /// A byte-stable fixture matching the
    /// autotrain's `Compare3Report::to_json` shape
    /// (hand-authored, not produced by a
    /// `trainer --compare3` run, so a fresh
    /// `sha256sum` of the committed
    /// `tests/fixtures/compare3-fixture.json`
    /// matches the in-test fixture's digest).
    fn compare3_fixture_report() -> Compare3Report {
        Compare3Report {
            hands_per_pair: 16,
            blind: 2,
            v1: Compare3SubReport {
                hands: 32,
                wins: 18,
                losses: 14,
                net_chips: 120,
                mbb_per_100: 18.7500,
                mbb_ci95: 4.1234,
                win_rate: 0.5625,
                win_rate_ci95: 0.0867,
            },
            v2: Compare3SubReport {
                hands: 32,
                wins: 16,
                losses: 16,
                net_chips: 24,
                mbb_per_100: 3.7500,
                mbb_ci95: 3.9876,
                win_rate: 0.5000,
                win_rate_ci95: 0.0884,
            },
            v3: Compare3SubReport {
                hands: 32,
                wins: 22,
                losses: 10,
                net_chips: 240,
                mbb_per_100: 37.5000,
                mbb_ci95: 5.0123,
                win_rate: 0.6875,
                win_rate_ci95: 0.0815,
            },
            v1_v2_delta: 15.0000,
            v2_v3_delta: -33.7500,
            v3_v1_delta: 18.7500,
            ranked_winner: Compare3Winner::V3,
        }
    }

    #[test]
    fn compare3_card_has_pinned_sub_reports_and_deltas() {
        let report = compare3_fixture_report();
        let card = render_compare3_card(COMPARE3_FIXTURE_ID, &report);
        // The basename must appear as the card's
        // `<h2>`.
        assert!(
            card.contains(&format!(
                "<h2 class=\"bench-card__title\">{COMPARE3_FIXTURE_ID}</h2>"
            )),
            "compare3 card must carry the receipt basename in the <h2>; got:\n{card}"
        );
        // The three sub-report `mbb_per_100`
        // values must appear in v1 / v2 / v3 order
        // (the `<dt>mbb_per_100</dt>` first
        // occurrence is v1's, the second is v2's,
        // the third is v3's).
        let mbb_positions: Vec<usize> = card.match_indices("mbb_per_100").map(|(i, _)| i).collect();
        assert_eq!(
            mbb_positions.len(),
            3,
            "compare3 card must contain exactly 3 `mbb_per_100` cells (one per sub-report); got {mbb_positions:?} in:\n{card}"
        );
        // The three pinned sub-report headline
        // values must appear in v1 / v2 / v3
        // order.
        let i_v1 = card.find("18.7500").expect("contains v1 mbb/100");
        let i_v2 = card.find("3.7500").expect("contains v2 mbb/100");
        let i_v3 = card.find("37.5000").expect("contains v3 mbb/100");
        assert!(
            i_v1 < i_v2 && i_v2 < i_v3,
            "sub-report order must be v1 < v2 < v3; got: v1={i_v1} v2={i_v2} v3={i_v3}"
        );
        // The three pairwise `<dt>`s must appear
        // in v1-vs-v2 / v2-vs-v3 / v3-vs-v1
        // order.
        let i_d12 = card.find("v1_v2_delta").expect("contains v1_v2_delta");
        let i_d23 = card.find("v2_v3_delta").expect("contains v2_v3_delta");
        let i_d31 = card.find("v3_v1_delta").expect("contains v3_v1_delta");
        assert!(
            i_d12 < i_d23 && i_d23 < i_d31,
            "pairwise deltas must be ordered v1_v2 < v2_v3 < v3_v1; got: d12={i_d12} d23={i_d23} d31={i_d31}"
        );
        // The `ranked_winner` headline must
        // appear in the body.
        assert!(
            card.contains("ranked_winner") && card.contains("<strong>v3</strong>"),
            "compare3 card must show `ranked_winner: <strong>v3</strong>`; got:\n{card}"
        );
    }

    #[test]
    fn compare3_fixture_round_trips_via_serde() {
        // The committed
        // `tests/fixtures/compare3-fixture.json`
        // must `serde_json::from_str` into a
        // typed `Compare3Report` without error.
        // A regression in the autotrain's
        // `Compare3Report::to_json` field shape
        // (a renamed field, a missing field) fails
        // this test at the same CI step a
        // downstream `trainer --compare3 --json`
        // consumer would silently break.
        let path = compare3_fixture_path();
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let parsed: Compare3Report = serde_json::from_str(&body).unwrap_or_else(|e| {
            panic!("compare3 fixture must parse as Compare3Report: {e}; body:\n{body}")
        });
        // The `ranked_winner` must be one of the
        // four pinned variants.
        assert!(
            matches!(
                parsed.ranked_winner,
                Compare3Winner::V1 | Compare3Winner::V2 | Compare3Winner::V3 | Compare3Winner::Tie
            ),
            "ranked_winner must be in {{V1, V2, V3, Tie}}; got: {:?}",
            parsed.ranked_winner
        );
        // The fixture must have all three
        // sub-reports populated (not a degenerate
        // zero-hand fixture).
        assert!(
            parsed.v1.hands > 0 && parsed.v2.hands > 0 && parsed.v3.hands > 0,
            "compare3 fixture sub-reports must be populated; got: v1.hands={} v2.hands={} v3.hands={}",
            parsed.v1.hands,
            parsed.v2.hands,
            parsed.v3.hands
        );
        // The fixture's `mbb_per_100` per-sub-report
        // must equal the
        // `mbb_per_100` value the
        // `render_bench_card(&BenchCardFields::from_compare3(...))`
        // projection would carry (a
        // regression that drops a digit fails
        // this assertion).
        let v1_card = render_bench_card(&BenchCardFields::from_compare3(
            COMPARE3_FIXTURE_ID,
            "v1",
            "preflop",
            &parsed.v1,
        ));
        assert!(
            v1_card.contains(&format!("{:.4}", parsed.v1.mbb_per_100)),
            "v1 sub-card must contain v1's pinned mbb/100; got:\n{v1_card}"
        );
    }

    #[test]
    fn render_emits_escaped_quot_and_lt() {
        // A basename that contains characters which would
        // corrupt the page if interpolated raw: `<`,
        // `&`, and `"`. The `html_escape` helper must
        // turn them into `&lt;` / `&amp;` / `&quot;`.
        let fields = BenchCardFields {
            receipt_basename: "evil<name&with\"quote".to_string(),
            blueprint: "v2".to_string(),
            baseline: "equity".to_string(),
            mbb_per_100: 0.0,
            mbb_ci95: 0.0,
            win_rate: 0.0,
        };
        let card = render_bench_card(&fields);
        assert!(
            card.contains("&lt;name&amp;with&quot;quote"),
            "basename must be HTML-escaped in the card; got:\n{card}"
        );
        assert!(
            !card.contains("evil<name"),
            "raw `<` must not appear in the card body"
        );

        // Same check for the table: a basename
        // containing `<` / `&` must be escaped in the
        // per-row cell.
        let mut index = one_entry_publish_index();
        index.entries[0].receipt_basename = "evil<name&with\"quote".to_string();
        let table = render_index_table(&index);
        assert!(
            table.contains("&lt;name&amp;with&quot;quote"),
            "basename must be HTML-escaped in the table cell; got:\n{table}"
        );
    }
}
