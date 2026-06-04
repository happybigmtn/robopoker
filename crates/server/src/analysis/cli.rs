//! Interactive CLI for poker analysis.
//!
//! Provides commands for type conversions and database queries.
use super::*;
use clap::Parser;
use rbp_cards::*;
use rbp_gameplay::*;
use std::io::Write;

pub struct CLI(API);

impl From<API> for CLI {
    fn from(api: API) -> Self {
        Self(api)
    }
}

impl CLI {
    pub async fn run() -> () {
        log::info!("entering analysis");
        let cli = Self(API::from(rbp_database::db().await));
        loop {
            print!("> ");
            let ref mut input = String::new();
            std::io::stdout().flush().unwrap();
            std::io::stdin().read_line(input).unwrap();
            match input.trim() {
                "quit" => break,
                "exit" => break,
                _ => match cli.handle(input).await {
                    Err(e) => eprintln!("{}", e),
                    Ok(_) => continue,
                },
            }
        }
    }
    async fn handle(&self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let query = Query::try_parse_from(std::iter::once("> ").chain(input.split_whitespace()))?;
        // STW-025: pure (no-API) commands delegate to a renderer that
        // returns the exact stdout text the REPL would print. DB-bound
        // commands keep their inline async bodies because the renderer
        // does not (and cannot) own a database connection.
        if let Some(rendered) = render_query(&query) {
            match rendered {
                Ok(text) => print!("{}", text),
                Err(e) => return Err(e.into()),
            }
            return Ok(());
        }
        match query {
            Query::Abstraction { target } => {
                if let Ok(obs) = Observation::try_from(target.as_str()) {
                    return Ok(println!("{}", self.0.obs_to_abs(obs).await?));
                }
                Err("invalid abstraction target".into())
            }
            Query::Distance { target1, target2 } => {
                if let (Ok(o1), Ok(o2)) = (
                    Observation::try_from(target1.as_str()),
                    Observation::try_from(target2.as_str()),
                ) {
                    return Ok(println!("{:.4}", self.0.obs_distance(o1, o2).await?));
                }
                if let (Ok(a1), Ok(a2)) = (
                    Abstraction::try_from(target1.as_str()),
                    Abstraction::try_from(target2.as_str()),
                ) {
                    return Ok(println!("{:.4}", self.0.abs_distance(a1, a2).await?));
                }
                Err("invalid distance targets".into())
            }
            Query::Equity { target } => {
                if let Ok(obs) = Observation::try_from(target.as_str()) {
                    return Ok(println!("{:.4}", self.0.obs_equity(obs).await?));
                }
                if let Ok(abs) = Abstraction::try_from(target.as_str()) {
                    return Ok(println!("{:.4}", self.0.abs_equity(abs).await?));
                }
                Err("invalid equity target".into())
            }
            Query::Population { target } => {
                if let Ok(obs) = Observation::try_from(target.as_str()) {
                    return Ok(println!("{}", self.0.obs_population(obs).await?));
                }
                if let Ok(abs) = Abstraction::try_from(target.as_str()) {
                    return Ok(println!("{}", self.0.abs_population(abs).await?));
                }
                Err("invalid population target".into())
            }
            Query::Similar { target } => {
                if let Ok(obs) = Observation::try_from(target.as_str()) {
                    let members = self
                        .0
                        .obs_similar(obs)
                        .await?
                        .iter()
                        .map(|obs| (obs, Strength::from(Hand::from(*obs))))
                        .map(|(o, s)| format!(" - {:<18} {}", o, s))
                        .collect::<Vec<String>>()
                        .join("\n");
                    return Ok(println!("{}", members));
                }
                if let Ok(abs) = Abstraction::try_from(target.as_str()) {
                    let members = self
                        .0
                        .abs_similar(abs)
                        .await?
                        .iter()
                        .map(|obs| (obs, Strength::from(Hand::from(*obs))))
                        .map(|(o, s)| format!(" - {:<18} {}", o, s))
                        .collect::<Vec<String>>()
                        .join("\n");
                    return Ok(println!("{}", members));
                }
                Err("invalid similarity target".into())
            }
            Query::Nearby { target } => {
                if let Ok(obs) = Observation::try_from(target.as_str()) {
                    let neighborhood = self
                        .0
                        .obs_nearby(obs)
                        .await?
                        .iter()
                        .enumerate()
                        .map(|(i, (abs, dist))| format!("{:>2}. {} ({:.4})", i + 1, abs, dist))
                        .collect::<Vec<String>>()
                        .join("\n");
                    return Ok(println!("{}", neighborhood));
                }
                if let Ok(abs) = Abstraction::try_from(target.as_str()) {
                    let neighborhood = self
                        .0
                        .abs_nearby(abs)
                        .await?
                        .iter()
                        .enumerate()
                        .map(|(i, (abs, dist))| format!("{:>2}. {} ({:.4})", i + 1, abs, dist))
                        .collect::<Vec<String>>()
                        .join("\n");
                    return Ok(println!("{}", neighborhood));
                }
                Err("invalid neighborhood target".into())
            }
            Query::Composition { target } => {
                if let Ok(obs) = Observation::try_from(target.as_str()) {
                    let distribution = self
                        .0
                        .obs_histogram(obs)
                        .await?
                        .distribution()
                        .iter()
                        .enumerate()
                        .map(|(i, (abs, dist))| format!("{:>2}. {} ({:.4})", i + 1, abs, dist))
                        .collect::<Vec<String>>()
                        .join("\n");
                    return Ok(println!("{}", distribution));
                }
                if let Ok(abs) = Abstraction::try_from(target.as_str()) {
                    let distribution = self
                        .0
                        .abs_histogram(abs)
                        .await?
                        .distribution()
                        .iter()
                        .enumerate()
                        .map(|(i, (abs, dist))| format!("{:>2}. {} ({:.4})", i + 1, abs, dist))
                        .collect::<Vec<String>>()
                        .join("\n");
                    return Ok(println!("{}", distribution));
                }
                Err("invalid histogram target".into())
            }
            // The five pure commands below are unreachable here because
            // `render_query` returned `Some(...)` for them above. They are
            // listed in this `match` only because the `query` binding
            // is moved out of `if let`; the arms below are unreachable.
            Query::Path { .. }
            | Query::Edge { .. }
            | Query::AbsFromInt { .. }
            | Query::ObsFromInt { .. }
            | Query::Isomorphism { .. } => {
                unreachable!("pure commands are dispatched via render_query")
            }
        }
    }
}

/// Render the text a pure (no-database) `Query` variant would print to
/// stdout. Returns `None` for DB-bound commands (the caller must
/// dispatch those through `API`).
///
/// The returned `String` is byte-identical to the sequence of
/// `println!` calls the pre-STW-025 handler emitted for the same
/// command (each `println!` adds a trailing `\n`).
///
/// Exposed as `pub` (not just `pub(crate)`) because the
/// `crates/server/tests/analysis_cli.rs` integration test pins
/// this helper's contract through the crate's public API. The
/// surface area is intentionally narrow: it takes `&Query` and
/// returns an `Option<Result<String, String>>` — no `&self`, no
/// `&API`, no database handle. Callers with a live `API` should
/// continue to dispatch DB-bound variants through the existing
/// `API` methods.
pub fn render_query(query: &Query) -> Option<Result<String, String>> {
    Some(match query {
        Query::Path { value } => {
            let path = Path::from(*value);
            Ok([
                format!("Path({})", value),
                format!("  Display:  {}", path),
                format!("  Length:   {}", path.length()),
                format!("  Aggro:    {}", path.aggression()),
                format!("  Edges:    {:?}", Vec::<Edge>::from(path)),
            ]
            .join("\n")
                + "\n")
        }
        Query::Edge { value } => {
            let edge = Edge::from(*value);
            Ok([
                format!("Edge({})", value),
                format!("  Display:  {}", edge),
                format!("  Is choice: {}", edge.is_choice()),
                format!("  Is aggro:  {}", edge.is_aggro()),
            ]
            .join("\n")
                + "\n")
        }
        Query::AbsFromInt { value } => {
            let abs = Abstraction::from(*value);
            Ok([
                format!("Abstraction({})", value),
                format!("  Display:  {}", abs),
                format!("  Street:   {}", abs.street()),
                format!("  Index:    {}", abs.index()),
            ]
            .join("\n")
                + "\n")
        }
        Query::ObsFromInt { value } => {
            let header = format!("Observation({})\n", value);
            match std::panic::catch_unwind(|| Observation::from(*value)) {
                Ok(obs) => Ok(header
                    + &[
                        format!("  Display:  {}", obs),
                        format!("  Street:   {}", obs.street()),
                        format!("  i64:      {}", i64::from(obs)),
                    ]
                    .join("\n")
                    .as_str()
                    + "\n"),
                Err(_) => Ok(header
                    + "  Error: Invalid observation encoding (assertions failed)\n  Note: Observations require valid poker hand representations\n"),
            }
        }
        Query::Isomorphism { value } => {
            let header = format!("Isomorphism({})\n", value);
            match std::panic::catch_unwind(|| {
                let iso = Isomorphism::from(*value);
                let obs = Observation::from(iso);
                (iso, obs)
            }) {
                Ok((iso, obs)) => Ok(header
                    + &[
                        format!("  Observation: {}", obs),
                        format!("  Street:      {}", obs.street()),
                        format!("  i64:         {}", i64::from(iso)),
                    ]
                    .join("\n")
                    .as_str()
                    + "\n"),
                Err(_) => Ok(header
                    + "  Error: Invalid isomorphism encoding (assertions failed)\n  Note: Isomorphisms require valid poker hand representations\n"),
            }
        }
        // DB-bound commands return `None` so the caller dispatches them
        // through the live `API` connection.
        Query::Abstraction { .. }
        | Query::Distance { .. }
        | Query::Equity { .. }
        | Query::Population { .. }
        | Query::Similar { .. }
        | Query::Nearby { .. }
        | Query::Composition { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smallest well-formed `Observation` (and `Isomorphism`) integer.
    /// The `Observation::from(i64)` impl decodes the input as a stream
    /// of one-byte card slots: byte i = `(bits >> (i*8)) as u8 - 1`,
    /// terminating when the shifted byte is zero. `0x0102` decodes to
    /// two distinct cards (slot 0 -> card 1, slot 1 -> card 0), so
    /// the pocket hand `Hand::add` invariant `(lhs & rhs) == 0` holds
    /// and the `pocket.size() == 2` debug_assert in
    /// `Observation::from((Hand, Hand))` is satisfied. Values <= 0
    /// decode to an empty pocket and trip the size assertion, which is
    /// exactly the panic path the catch_unwind guard exists to handle.
    const WELL_FORMED_OBS: i64 = 0x0102;

    /// Parse a REPL-style input string (no leading `> `) into a
    /// `Query`. Mirrors the production parser: a single leading
    /// `> ` followed by the user's argv. The clap `Parser` derive
    /// converts enum variant names to kebab-case command names
    /// (`Path` -> `path`, `AbsFromInt` -> `abs-from-int`), but the
    /// short aliases declared in `analysis/query.rs` (`pth`, `edg`,
    /// `abi`, `obi`, `iso`) are the convenient user-facing names,
    /// so the test surface uses the aliases.
    fn parse(input: &str) -> Query {
        Query::try_parse_from(std::iter::once("> ").chain(input.split_whitespace()))
            .expect("pure-command input must parse")
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
    fn render_path_known_i64_round_trips() {
        // value=1 (a single 4-bit edge = 1, the Draw edge) yields a
        // one-step path. We assert the structural shape: header +
        // a Display line, a Length line of 1, an Aggro line of 0,
        // and an Edges debug line that contains at least one edge.
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
    fn render_path_zero_i64_renders_empty() {
        // value=0 produces an empty path (zero edges). The Display
        // line and Length: 0 are the two assertions; we deliberately
        // do not assert the exact Display string (it is a `Path::from`
        // impl detail, not the wire contract).
        let text = render("pth 0");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Path(0)");
        assert!(lines[1].starts_with("  Display:"), "got: {}", lines[1]);
        assert!(
            lines[2].contains("0"),
            "Length: 0 expected, got: {}",
            lines[2]
        );
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn render_edge_draw_byte_renders_non_choice() {
        // The Draw edge has byte value 1 (see `From<u8> for Edge` in
        // `crates/gameplay/src/edge.rs`: 1=Draw, 2=Fold, 3=Check,
        // 4=Call, 5=Shove, 6..=9=Open, 10..=15=Raise). `is_choice()`
        // is the negation of `is_chance()`, so Draw is NOT a choice
        // and NOT aggressive. This pins the chance-node contract.
        let text = render("edg 1");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Edge(1)");
        assert!(lines[1].starts_with("  Display:"));
        assert!(lines[2].contains("Is choice:"));
        assert!(
            lines[2].contains("false"),
            "Draw is not a choice: {lines:?}"
        );
        assert!(lines[3].contains("Is aggro:"));
        assert!(lines[3].contains("false"), "Draw is not aggro: {lines:?}");
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn render_edge_fold_byte_renders_choice() {
        // The Fold edge has byte value 2. `is_choice()` is true for
        // any non-chance edge; `is_aggro()` is false for Fold. This
        // pins the contract that any player-decision edge is a
        // "choice" and Fold is not aggressive.
        let text = render("edg 2");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Edge(2)");
        let choice_line = lines
            .iter()
            .find(|l| l.contains("Is choice:"))
            .expect("Is choice line");
        assert!(
            choice_line.contains("true"),
            "Fold must be a choice edge: {choice_line}"
        );
        let aggro_line = lines
            .iter()
            .find(|l| l.contains("Is aggro:"))
            .expect("Is aggro line");
        assert!(
            aggro_line.contains("false"),
            "Fold is not aggressive: {aggro_line}"
        );
    }

    #[test]
    fn render_abs_from_int_zero_round_trips() {
        // value=0 is the canonical "first abstraction in the index
        // space". We assert the contract: a 4-line block ending in
        // newline with the `Index:    0` line. The exact street is
        // a `Abstraction::street()` impl detail and is not pinned.
        let text = render("abi 0");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Abstraction(0)");
        assert!(lines[1].starts_with("  Display:"), "got: {}", lines[1]);
        assert!(lines[2].starts_with("  Street:"), "got: {}", lines[2]);
        assert!(lines[3].starts_with("  Index:    "), "got: {}", lines[3]);
        assert!(
            lines[3].contains("0"),
            "Index 0 expected, got: {}",
            lines[3]
        );
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn render_obs_from_int_well_formed_renders_full_body() {
        // Well-formed Observation input: 0x010101 decodes to a
        // 2-card pocket via the byte-slot stream in
        // `Observation::from(i64)`, so the construction passes
        // the `pocket.size() == 2` debug_assert and the renderer
        // returns the full Display/Street/i64 body.
        let text = render(&format!("obi {}", WELL_FORMED_OBS));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], format!("Observation({})", WELL_FORMED_OBS));
        assert!(lines[1].starts_with("  Display:"));
        assert!(lines[2].starts_with("  Street:"));
        assert!(lines[3].starts_with("  i64:"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn render_obs_from_int_zero_panics_guarded_by_catch_unwind() {
        // value=0 decodes to an empty pocket, which trips
        // `debug_assert!(pocket.size() == 2)` inside
        // `Observation::from((Hand, Hand))`. The `catch_unwind` in
        // the renderer catches the panic and produces the
        // "Invalid observation encoding" error body. This is the
        // exact panic-guard contract the production handler
        // preserves.
        let q = Query::ObsFromInt { value: 0 };
        let text = match render_query(&q) {
            Some(Ok(s)) => s,
            Some(Err(e)) => panic!("pure command must not error: {e}"),
            None => panic!("pure command must not be a DB-bound variant"),
        };
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Observation(0)");
        assert!(lines[1].starts_with("  Error:"));
        assert!(
            lines[1].contains("Invalid observation encoding"),
            "Error line must name the encoding failure: {}",
            lines[1]
        );
        assert!(lines[2].starts_with("  Note:"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn render_iso_from_int_well_formed_renders_full_body() {
        // Well-formed Isomorphism input: 0x010101 decodes to a
        // valid 2-card observation, so the renderer returns the
        // full Observation/Street/i64 body.
        let text = render(&format!("iso {}", WELL_FORMED_OBS));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], format!("Isomorphism({})", WELL_FORMED_OBS));
        assert!(lines[1].starts_with("  Observation:"));
        assert!(lines[2].starts_with("  Street:      "));
        assert!(lines[3].starts_with("  i64:         "));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn render_iso_from_int_zero_panics_guarded_by_catch_unwind() {
        // value=0 decodes to an empty observation, which trips
        // the same `pocket.size() == 2` debug_assert. The
        // `catch_unwind` guard catches the panic and produces
        // the "Invalid isomorphism encoding" error body.
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
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn render_db_bound_query_returns_none() {
        // Pin the contract that DB-bound variants do NOT have a
        // pure renderer — the caller dispatches them through `API`.
        // A regression that "accidentally" renders one inline (and
        // silently skips the database call) would be caught here.
        // We don't print the variant value in the failure message
        // because `Query` does not implement `Debug` and adding a
        // `Debug` derive is out of scope for STW-025 (touching
        // `analysis/query.rs` would change the wire contract for
        // any future `Debug`-formatting caller).
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
}
