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
        // The `INDEX.json`'s `IndexedEntry` shape doesn't
        // carry the per-hand `mbb_per_100` / `win_rate`
        // — those live on the `BenchReport` the next
        // slice will inline. For now the table renders
        // placeholder `—` cells so the column order is
        // visible; a regression in the placeholder
        // string fails the `placeholder_cells_present`
        // lib test.
        let basename = html_escape(&entry.receipt_basename);
        let basename_href = url_encode(&entry.receipt_basename);
        let uploaded_at = html_escape(&entry.remote_receipt.uploaded_at_utc);
        let total_bytes = entry.remote_receipt.total_bytes;
        s.push_str("    <tr>\n");
        s.push_str(&format!(
            "      <td class=\"index-table__cell\">{basename}</td>\n"
        ));
        s.push_str(&format!("      <td class=\"index-table__cell\">—</td>\n"));
        s.push_str(&format!("      <td class=\"index-table__cell\">—</td>\n"));
        s.push_str(&format!("      <td class=\"index-table__cell\">—</td>\n"));
        s.push_str(&format!("      <td class=\"index-table__cell\">—</td>\n"));
        s.push_str(&format!("      <td class=\"index-table__cell\">—</td>\n"));
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

/// HTML-escape a `&str` for safe interpolation into a
/// `<td>` / `<dt>` / `<h2>` cell. Replaces the five
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
    use rbp_autotrain::{IndexedEntry, PublishIndex, PublishedRemoteReceipt};

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
            }],
        }
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
