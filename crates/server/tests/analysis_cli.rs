//! Integration tests for the analysis CLI `render_query` helper.
//!
//! These tests exercise the renderer's *public* surface (the
//! `Query::try_parse_from` path that the `bin/robopoker-backend`
//! binary actually feeds in) and assert the rendered text matches
//! the lib-test expectations. They live as a `tests/` integration
//! test (not a `#[cfg(test)] mod tests`) so they compile and run
//! against the public API of `rbp_server` and would catch a
//! regression that hid the renderer behind a non-`pub` boundary.
//!
//! The tests are entirely synchronous and require no `tokio`
//! runtime, no `actix_web::test` runtime, and no `DATABASE_URL`.
//! They are the no-DB layer of the STW-025 analysis test surface.

use clap::Parser;
use rbp_server::analysis::Query;
use rbp_server::analysis::render_query;

/// Parse a REPL-style input (no leading `> `) into a `Query`.
/// Mirrors the production parser: a single leading `> ` followed
/// by the user's argv. The clap `Parser` derive converts enum
/// variant names to kebab-case command names (`Path` -> `path`,
/// `AbsFromInt` -> `abs-from-int`); the short aliases declared in
/// `analysis/query.rs` (`pth`, `edg`, `abi`, `obi`, `iso`) are
/// the convenient user-facing names, so the test surface uses the
/// aliases.
fn parse(input: &str) -> Query {
    let argv: Vec<&str> = std::iter::once("> ")
        .chain(input.split_whitespace())
        .collect();
    Query::try_parse_from(argv.iter().copied()).expect("pure-command input must parse")
}

/// Run `render_query` and unwrap the `Some(Result<_, _>)` shape.
/// `render_query` returns `None` for DB-bound variants and
/// `Some(Ok(_))` for pure variants; tests below only feed pure
/// variants, so the `None` arm is unreachable here.
fn render(input: &str) -> String {
    match render_query(&parse(input)) {
        Some(Ok(s)) => s,
        Some(Err(e)) => panic!("pure command must not error: {e}"),
        None => panic!("pure command must not be a DB-bound variant"),
    }
}

#[test]
fn analysis_cli_path_value_one_renders_full_block() {
    // A one-step path: a single 4-bit edge in the low slot, which
    // `Path::from` decodes as one edge (Draw, value 1). The
    // rendered text is a 5-line block: header + Display/Length/
    // Aggro/Edges. We assert the structural shape so the test
    // is robust against minor `Path::Display` impl changes.
    let text = render("pth 1");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("Path(1)"));
    let display = lines.next().expect("Display line");
    assert!(display.starts_with("  Display:"), "got: {display}");
    let length = lines.next().expect("Length line");
    assert!(length.starts_with("  Length:   "), "got: {length}");
    assert!(length.contains("1"), "length should be 1, got: {length}");
    let aggro = lines.next().expect("Aggro line");
    assert!(aggro.starts_with("  Aggro:"), "got: {aggro}");
    let edges = lines.next().expect("Edges line");
    assert!(edges.starts_with("  Edges:"), "got: {edges}");
    assert!(text.ends_with('\n'), "rendered text must end in newline");
}

#[test]
fn analysis_cli_edge_value_two_renders_fold_choice() {
    // Fold has byte value 2 in the canonical `From<u8> for Edge`
    // encoding (1=Draw, 2=Fold, 3=Check, 4=Call, 5=Shove). The
    // `Is choice: true` assertion is the stable contract: any
    // non-chance edge is a player decision.
    let text = render("edg 2");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Edge(2)");
    let choice_line = lines
        .iter()
        .find(|l| l.contains("Is choice:"))
        .expect("Is choice line");
    assert!(
        choice_line.contains("true"),
        "Fold (value=2) must be a choice edge: {choice_line}"
    );
}

#[test]
fn analysis_cli_abs_from_int_zero_renders_preflop() {
    // Abstraction(0) has `street() == Preflop` because the high
    // bits of the i16 are zero. The rendered block ends in
    // newline with an `Index:    0` line.
    let text = render("abi 0");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Abstraction(0)");
    assert!(lines[1].starts_with("  Display:"));
    assert!(lines[2].starts_with("  Street:"));
    assert!(lines[3].starts_with("  Index:    "));
    assert!(
        lines[3].contains("0"),
        "Index 0 expected, got: {}",
        lines[3]
    );
    assert!(text.ends_with('\n'));
}

#[test]
fn analysis_cli_iso_from_int_zero_panics_guarded() {
    // value=0 trips the `pocket.size() == 2` debug_assert inside
    // the `Isomorphism::from` path. The renderer must catch the
    // panic and produce the "Invalid isomorphism encoding" error
    // block — same panic-guard contract the production CLI
    // handler preserves.
    let q = Query::Isomorphism { value: 0 };
    let text = match render_query(&q) {
        Some(Ok(s)) => s,
        Some(Err(e)) => panic!("pure command must not error: {e}"),
        None => panic!("pure command must not be a DB-bound variant"),
    };
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Isomorphism(0)");
    assert!(lines[1].starts_with("  Error:"));
    assert!(
        lines[1].contains("Invalid isomorphism encoding"),
        "Error line must name the encoding failure: {}",
        lines[1]
    );
    assert!(lines[2].starts_with("  Note:"));
}

#[test]
fn analysis_cli_db_bound_variant_returns_none() {
    // The seven DB-bound `Query` variants must return `None` from
    // `render_query` so the caller dispatches them through the
    // live `API` connection. A regression that "accidentally"
    // renders one inline (and silently skips the database call)
    // would be caught here. We exercise one representative
    // variant (`Abstraction`) and pin the contract for the
    // remaining six by spot-checking the variant list length.
    let db: Vec<Query> = vec![
        Query::Abstraction { target: "x".into() },
        Query::Distance {
            target1: "x".into(),
            target2: "y".into(),
        },
        Query::Equity { target: "x".into() },
        Query::Population { target: "x".into() },
        Query::Similar { target: "x".into() },
        Query::Nearby { target: "x".into() },
        Query::Composition { target: "x".into() },
    ];
    assert_eq!(db.len(), 7, "all 7 DB-bound variants must be covered");
    for variant in &db {
        assert!(
            render_query(variant).is_none(),
            "DB-bound variant must not have a pure renderer"
        );
    }
}
