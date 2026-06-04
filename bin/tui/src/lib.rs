use anyhow::Result;
use clap::Parser;
use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap};
use rbp_cards::{Card, Hand, Strength};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tachyonfx::{EffectManager, Interpolation, Motion, fx};

const SURFACE_SCHEMA_VERSION: u8 = 1;
const DEFAULT_SEED: u64 = 0xC0D3;

#[derive(Debug, Parser)]
#[command(about = "Read-only robopoker Ratatui preview")]
pub struct Cli {
    /// Render once, print QA JSON, and optionally write artifacts.
    #[arg(long)]
    pub headless: bool,

    /// Headless viewport width.
    #[arg(long, default_value_t = 96)]
    pub width: u16,

    /// Headless viewport height.
    #[arg(long, default_value_t = 28)]
    pub height: u16,

    /// Seed for the local random-policy opponent preview.
    #[arg(long, default_value_t = DEFAULT_SEED)]
    pub seed: u64,

    /// Initial beat to render, 1-based.
    #[arg(long, default_value_t = 1)]
    pub step: usize,

    /// Directory for headless artifacts.
    #[arg(long)]
    pub export_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Focus {
    Table,
    Rail,
}

impl Focus {
    const fn next(self) -> Self {
        match self {
            Self::Table => Self::Rail,
            Self::Rail => Self::Table,
        }
    }
}

pub struct App {
    pub focus: Focus,
    pub show_help: bool,
    pub preview: RandomPreview,
    motion: MotionDirector,
}

impl Default for App {
    fn default() -> Self {
        Self::with_seed(DEFAULT_SEED)
    }
}

impl App {
    #[must_use]
    pub fn with_seed(seed: u64) -> Self {
        Self::with_seed_and_step(seed, 1)
    }

    #[must_use]
    pub fn with_seed_and_step(seed: u64, step: usize) -> Self {
        let mut preview = RandomPreview::from_seed(seed);
        preview.set_step_1_based(step);
        Self {
            focus: Focus::Table,
            show_help: false,
            preview,
            motion: MotionDirector::default(),
        }
    }

    pub fn process_motion(&mut self, elapsed: Duration, buf: &mut Buffer, area: Rect) {
        self.motion.process_frame(elapsed, buf, area);
    }

    #[must_use]
    pub fn motion_is_running(&self) -> bool {
        self.motion.is_running()
    }

    #[cfg(test)]
    #[must_use]
    fn pending_motion_count(&self) -> usize {
        self.motion.pending_len()
    }
}

#[derive(Debug, Serialize)]
pub struct RandomPreview {
    pub seed: u64,
    pub hero: PlayerPreview,
    pub opponent: PlayerPreview,
    pub board: Vec<CardView>,
    pub final_pot_bb: u16,
    pub winner: &'static str,
    pub story: Vec<PreviewLog>,
    pub steps: Vec<PreviewStep>,
    pub step: usize,
}

impl RandomPreview {
    fn from_seed(seed: u64) -> Self {
        let mut rng = PreviewRng::new(seed);
        let mut deck = (0u8..52).map(Card::from).collect::<Vec<_>>();
        for i in (1..deck.len()).rev() {
            let j = rng.next_usize(i + 1);
            deck.swap(i, j);
        }

        let hero_cards = vec![deck.pop().unwrap(), deck.pop().unwrap()];
        let opponent_cards = vec![deck.pop().unwrap(), deck.pop().unwrap()];
        let board_cards = (0..5).map(|_| deck.pop().unwrap()).collect::<Vec<_>>();
        let hero_strength = strength_for(&hero_cards, &board_cards);
        let opponent_strength = strength_for(&opponent_cards, &board_cards);
        let winner = match hero_strength.cmp(&opponent_strength) {
            std::cmp::Ordering::Greater => "Hero",
            std::cmp::Ordering::Less => "Fish",
            std::cmp::Ordering::Equal => "Split",
        };

        let mut pot_bb = 0;
        let mut story = Vec::new();
        let mut steps = vec![PreviewStep::ready()];

        append_step(
            &mut story,
            &mut steps,
            StepSpec::new("deal", "Hero", "checks cards", "two private cards", pot_bb).show_hero(),
        );

        pot_bb += 3;
        append_step(
            &mut story,
            &mut steps,
            StepSpec::new("blinds", "Dealer", "posts blinds", "1bb / 2bb", pot_bb).show_hero(),
        );

        pot_bb += 3;
        append_step(
            &mut story,
            &mut steps,
            StepSpec::new("preflop", "Hero", "opens", "3bb", pot_bb).show_hero(),
        );

        match rng.pick(&["calls", "raises", "calls"]) {
            "raises" => {
                pot_bb += 6;
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("preflop", "Fish", "random raise", "9bb", pot_bb).show_hero(),
                );
                pot_bb += 6;
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("preflop", "Hero", "calls", "6bb", pot_bb).show_hero(),
                );
            }
            _ => {
                pot_bb += 3;
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("preflop", "Fish", "random call", "3bb", pot_bb).show_hero(),
                );
            }
        }

        append_step(
            &mut story,
            &mut steps,
            StepSpec::new(
                "flop",
                "Board",
                "reveals flop",
                street_cards(&board_cards[0..3]),
                pot_bb,
            )
            .show_hero()
            .board_cards(3),
        );
        match rng.pick(&["checks", "bets", "checks"]) {
            "bets" => {
                pot_bb += 5;
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("flop", "Fish", "random bet", "5bb", pot_bb)
                        .show_hero()
                        .board_cards(3),
                );
                pot_bb += 5;
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("flop", "Hero", "calls", "5bb", pot_bb)
                        .show_hero()
                        .board_cards(3),
                );
            }
            _ => {
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("flop", "Fish", "checks", "0bb", pot_bb)
                        .show_hero()
                        .board_cards(3),
                );
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("flop", "Hero", "checks back", "0bb", pot_bb)
                        .show_hero()
                        .board_cards(3),
                );
            }
        }

        append_step(
            &mut story,
            &mut steps,
            StepSpec::new(
                "turn",
                "Board",
                "reveals turn",
                street_cards(&board_cards[3..4]),
                pot_bb,
            )
            .show_hero()
            .board_cards(4),
        );
        match rng.pick(&["checks", "bets"]) {
            "bets" => {
                pot_bb += 9;
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("turn", "Fish", "random bet", "9bb", pot_bb)
                        .show_hero()
                        .board_cards(4),
                );
                pot_bb += 9;
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("turn", "Hero", "calls", "9bb", pot_bb)
                        .show_hero()
                        .board_cards(4),
                );
            }
            _ => {
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("turn", "Fish", "checks", "0bb", pot_bb)
                        .show_hero()
                        .board_cards(4),
                );
                append_step(
                    &mut story,
                    &mut steps,
                    StepSpec::new("turn", "Hero", "checks back", "0bb", pot_bb)
                        .show_hero()
                        .board_cards(4),
                );
            }
        }

        append_step(
            &mut story,
            &mut steps,
            StepSpec::new(
                "river",
                "Board",
                "reveals river",
                street_cards(&board_cards[4..5]),
                pot_bb,
            )
            .show_hero()
            .board_cards(5),
        );
        append_step(
            &mut story,
            &mut steps,
            StepSpec::new("showdown", "Showdown", "resolves", winner, pot_bb)
                .show_hero()
                .show_opponent()
                .show_strengths()
                .board_cards(5),
        );

        Self {
            seed,
            hero: PlayerPreview::new(
                "Hero",
                hero_cards,
                hero_strength,
                result_for("Hero", winner),
            ),
            opponent: PlayerPreview::new(
                "Fish",
                opponent_cards,
                opponent_strength,
                result_for("Fish", winner),
            ),
            board: board_cards.into_iter().map(CardView::from).collect(),
            final_pot_bb: pot_bb,
            winner,
            story,
            steps,
            step: 0,
        }
    }

    #[must_use]
    pub fn current(&self) -> &PreviewStep {
        &self.steps[self.step.min(self.steps.len().saturating_sub(1))]
    }

    #[must_use]
    pub fn visible_board(&self) -> &[CardView] {
        let count = self.current().board_cards.min(self.board.len());
        &self.board[..count]
    }

    #[must_use]
    pub fn visible_story(&self) -> &[PreviewLog] {
        let count = self.current().log_count.min(self.story.len());
        &self.story[..count]
    }

    #[must_use]
    pub fn advance(&mut self) -> bool {
        if self.step + 1 >= self.steps.len() {
            return false;
        }
        self.step += 1;
        true
    }

    #[must_use]
    pub fn retreat(&mut self) -> bool {
        if self.step == 0 {
            return false;
        }
        self.step -= 1;
        true
    }

    #[must_use]
    pub fn progress(&self) -> String {
        format!("{}/{}", self.step + 1, self.steps.len())
    }

    #[must_use]
    pub fn winner_label(&self) -> &'static str {
        if self.current().show_strengths {
            self.winner
        } else {
            "pending"
        }
    }

    pub fn set_step_1_based(&mut self, step: usize) {
        self.step = step
            .saturating_sub(1)
            .min(self.steps.len().saturating_sub(1));
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewStep {
    pub label: &'static str,
    pub actor: &'static str,
    pub action: &'static str,
    pub detail: String,
    pub pot_bb: u16,
    pub board_cards: usize,
    pub show_hero: bool,
    pub show_opponent: bool,
    pub show_strengths: bool,
    pub log_count: usize,
}

impl PreviewStep {
    fn ready() -> Self {
        Self {
            label: "ready",
            actor: "Dealer",
            action: "awaiting input",
            detail: "Space deals one beat at a time".to_owned(),
            pot_bb: 0,
            board_cards: 0,
            show_hero: false,
            show_opponent: false,
            show_strengths: false,
            log_count: 0,
        }
    }
}

struct StepSpec {
    label: &'static str,
    actor: &'static str,
    action: &'static str,
    detail: String,
    pot_bb: u16,
    board_cards: usize,
    show_hero: bool,
    show_opponent: bool,
    show_strengths: bool,
}

impl StepSpec {
    fn new(
        label: &'static str,
        actor: &'static str,
        action: &'static str,
        detail: impl Into<String>,
        pot_bb: u16,
    ) -> Self {
        Self {
            label,
            actor,
            action,
            detail: detail.into(),
            pot_bb,
            board_cards: 0,
            show_hero: false,
            show_opponent: false,
            show_strengths: false,
        }
    }

    fn board_cards(mut self, board_cards: usize) -> Self {
        self.board_cards = board_cards;
        self
    }

    fn show_hero(mut self) -> Self {
        self.show_hero = true;
        self
    }

    fn show_opponent(mut self) -> Self {
        self.show_opponent = true;
        self
    }

    fn show_strengths(mut self) -> Self {
        self.show_strengths = true;
        self
    }
}

fn append_step(story: &mut Vec<PreviewLog>, steps: &mut Vec<PreviewStep>, spec: StepSpec) {
    story.push(PreviewLog::new(
        spec.actor,
        spec.action,
        spec.detail.clone(),
    ));
    steps.push(PreviewStep {
        label: spec.label,
        actor: spec.actor,
        action: spec.action,
        detail: spec.detail,
        pot_bb: spec.pot_bb,
        board_cards: spec.board_cards,
        show_hero: spec.show_hero,
        show_opponent: spec.show_opponent,
        show_strengths: spec.show_strengths,
        log_count: story.len(),
    });
}

#[derive(Debug, Serialize)]
pub struct PlayerPreview {
    pub name: &'static str,
    pub cards: Vec<CardView>,
    pub strength: String,
    pub result: &'static str,
}

impl PlayerPreview {
    fn new(name: &'static str, cards: Vec<Card>, strength: Strength, result: &'static str) -> Self {
        Self {
            name,
            cards: cards.into_iter().map(CardView::from).collect(),
            strength: human_strength(&strength),
            result,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CardView {
    pub rank: String,
    pub suit: char,
    pub red: bool,
}

impl From<Card> for CardView {
    fn from(card: Card) -> Self {
        let suit = card.suit().ascii();
        Self {
            rank: card.rank().to_string(),
            suit,
            red: matches!(suit, '♥' | '♦'),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PreviewLog {
    pub actor: &'static str,
    pub action: &'static str,
    pub detail: String,
}

impl PreviewLog {
    fn new(actor: &'static str, action: &'static str, detail: impl Into<String>) -> Self {
        Self {
            actor,
            action,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Control {
    pub id: &'static str,
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct SurfaceMeta {
    pub app_id: &'static str,
    pub schema_version: u8,
    pub viewport: Viewport,
    pub theme: &'static str,
    pub source: &'static str,
    pub posture: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Viewport {
    pub width: u16,
    pub height: u16,
}

/// A single QA check result. The `id` is a stable dotted slug a
/// downstream testnet dashboard can grep on (e.g. `tui.chrome.brand`).
/// The `label` is a human-readable description. `passed` is the
/// boolean outcome. `detail` is a one-line context the operator
/// can read to understand what the check actually saw.
#[derive(Clone, Debug, Serialize)]
pub struct QaCheck {
    pub id: &'static str,
    pub label: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct QaReport {
    pub verdict: &'static str,
    pub assertions: Vec<&'static str>,
    pub frame_hash: u64,
    pub controls: usize,
    pub checks: Vec<QaCheck>,
}

#[derive(Debug, Serialize)]
pub struct HeadlessReport {
    pub surface: SurfaceMeta,
    pub controls: Vec<Control>,
    pub frame: String,
    pub qa: QaReport,
}

impl HeadlessReport {
    #[must_use]
    pub fn capture(app: &App, width: u16, height: u16) -> Self {
        let frame = render_lines(app, width, height).join("\n");
        let controls = controls();
        let surface = SurfaceMeta {
            app_id: "robopoker-tui",
            schema_version: SURFACE_SCHEMA_VERSION,
            viewport: Viewport { width, height },
            theme: "black-chrome-minimal",
            source: "seeded local random-policy preview + rbp-cards evaluator",
            posture: "read-only; no server, database, training, wagering, or network path",
        };

        // The QA gate is a real computed gate (STW-021): each check
        // actually runs against the rendered frame / controls / app
        // state, and the top-level `verdict` is the AND of every
        // check's `passed` field. The backward-compat `assertions`
        // field is repurposed to a `Vec<&'static str>` of the
        // *failing* check ids so the existing receipt shape stays
        // stable: a fully green run still has `assertions: []`.
        // STW-027: `tui.tape.actions_present` and
        // `tui.board.cards_present` extend the gate to the
        // decision-tape log + board-stage card render (the two
        // rendered surfaces STW-021 deliberately deferred).
        let checks = vec![
            check_chrome_branding(&frame),
            check_chrome_players(&frame),
            check_chrome_posture(&frame),
            check_viewport_bounds(app, width, height),
            check_controls_unique(&controls),
            check_controls_keys_unique(&controls),
            check_controls_count(&controls),
            check_cards_evaluator(app),
            check_tape_actions_present(app),
            check_board_cards_present(app),
            check_help_toggle(&controls),
        ];
        let verdict = compute_verdict(&checks);
        let assertions: Vec<&'static str> = checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| check.id)
            .collect();
        let qa = QaReport {
            verdict,
            assertions,
            frame_hash: hash_frame(&frame),
            controls: controls.len(),
            checks,
        };

        Self {
            surface,
            controls,
            frame,
            qa,
        }
    }
}

/// Compute the top-level QA `verdict`: `"passed"` when every check
/// passed, `"failed"` if any single check failed. A testnet
/// dashboard can grep `tui.qa.json` for the `verdict` field to gate
/// a release on this output.
#[must_use]
pub fn compute_verdict(checks: &[QaCheck]) -> &'static str {
    if checks.iter().all(|check| check.passed) {
        "passed"
    } else {
        "failed"
    }
}

fn check_chrome_branding(frame: &str) -> QaCheck {
    let id = "tui.chrome.brand";
    let label = "frame contains the ROBOPOKER brand chrome";
    let passed = frame.contains("ROBOPOKER");
    let detail = if passed {
        "ROBOPOKER header present".to_owned()
    } else {
        "ROBOPOKER header missing from the rendered frame".to_owned()
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_chrome_players(frame: &str) -> QaCheck {
    let id = "tui.chrome.players";
    let label = "frame names both the hero and the fish seats";
    let has_hero = frame.contains("YOU");
    let has_fish = frame.contains("FISH");
    let passed = has_hero && has_fish;
    let detail = match (has_hero, has_fish) {
        (true, true) => "both YOU and FISH seat labels present".to_owned(),
        (false, true) => "FISH label present, YOU label missing".to_owned(),
        (true, false) => "YOU label present, FISH label missing".to_owned(),
        (false, false) => "neither YOU nor FISH seat label present".to_owned(),
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_chrome_posture(frame: &str) -> QaCheck {
    let id = "tui.chrome.posture";
    let label = "frame surfaces the read-only offline posture";
    let has_offline = frame.contains("offline");
    let has_read_only = frame.contains("read-only");
    let passed = has_offline && has_read_only;
    let detail = match (has_offline, has_read_only) {
        (true, true) => "offline + read-only posture markers both present".to_owned(),
        (false, true) => "read-only marker present, offline marker missing".to_owned(),
        (true, false) => "offline marker present, read-only marker missing".to_owned(),
        (false, false) => "neither offline nor read-only posture marker present".to_owned(),
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_viewport_bounds(app: &App, width: u16, height: u16) -> QaCheck {
    let id = "tui.viewport.bounds";
    let label = "rendered lines fit inside the requested viewport";
    let lines = render_lines(app, width, height);
    let row_count = lines.len() as u16;
    let row_within = row_count <= height;
    let col_within = lines
        .iter()
        .all(|line| line.chars().count() <= width as usize);
    let passed = row_within && col_within;
    let detail = if passed {
        format!("{row_count} rows of ≤{width} cols fits inside {height}-row viewport")
    } else {
        let over_cols: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| {
                let count = line.chars().count();
                if count > width as usize {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        if !row_within {
            format!("row count {row_count} exceeds viewport height {height}")
        } else {
            format!(
                "{} line(s) exceed viewport width {width} (first overflowing line: {:?})",
                over_cols.len(),
                over_cols.first().copied()
            )
        }
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_controls_unique(controls: &[Control]) -> QaCheck {
    let id = "tui.controls.ids_unique";
    let label = "every control id is unique";
    let total = controls.len();
    let mut sorted = controls.to_vec();
    sorted.sort_by_key(|c| c.id);
    let unique = sorted.windows(2).all(|pair| pair[0].id != pair[1].id);
    let passed = total > 0 && unique;
    let detail = if passed {
        format!("{total} control id(s) are all unique")
    } else if total == 0 {
        "controls() returned an empty vector".to_owned()
    } else {
        let mut sorted = controls.to_vec();
        sorted.sort_by_key(|c| c.id);
        let dup = sorted
            .windows(2)
            .find(|pair| pair[0].id == pair[1].id)
            .map(|pair| pair[0].id)
            .unwrap_or("<unknown>");
        format!("duplicate control id detected: {dup}")
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_controls_keys_unique(controls: &[Control]) -> QaCheck {
    let id = "tui.controls.keys_unique";
    let label = "every control key is unique";
    let total = controls.len();
    let mut sorted = controls.to_vec();
    sorted.sort_by_key(|c| c.key);
    let unique = sorted.windows(2).all(|pair| pair[0].key != pair[1].key);
    let passed = total > 0 && unique;
    let detail = if passed {
        format!("{total} control key(s) are all unique")
    } else {
        let mut sorted = controls.to_vec();
        sorted.sort_by_key(|c| c.key);
        let dup = sorted
            .windows(2)
            .find(|pair| pair[0].key == pair[1].key)
            .map(|pair| pair[0].key)
            .unwrap_or("<unknown>");
        format!("duplicate control key detected: {dup}")
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_controls_count(controls: &[Control]) -> QaCheck {
    let id = "tui.controls.count";
    let label = "controls() returns a non-empty vector of enabled entries";
    let count = controls.len();
    let all_enabled = controls.iter().all(|c| c.enabled);
    let passed = count > 0 && all_enabled;
    let detail = if passed {
        format!("{count} control(s), all enabled")
    } else if count == 0 {
        "controls() returned an empty vector".to_owned()
    } else {
        let disabled = controls.iter().filter(|c| !c.enabled).count();
        format!("{disabled} of {count} control(s) are disabled")
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_cards_evaluator(app: &App) -> QaCheck {
    let id = "tui.cards.evaluator";
    let label = "the hero / fish hand evaluator returned a real rbp-cards strength";
    let winner = app.preview.winner;
    let has_known_winner = matches!(winner, "Hero" | "Fish" | "Split");
    let hero_strength = app.preview.hero.strength.as_str();
    let fish_strength = app.preview.opponent.strength.as_str();
    let strengths_populated = !hero_strength.is_empty() && !fish_strength.is_empty();
    let passed = has_known_winner && strengths_populated;
    let detail = if passed {
        format!("winner={winner} hero={hero_strength} fish={fish_strength}")
    } else if !has_known_winner {
        format!("winner {winner:?} is not one of {{Hero, Fish, Split}}")
    } else {
        format!(
            "evaluator returned an empty strength: hero={hero_strength:?} fish={fish_strength:?}"
        )
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

fn check_help_toggle(controls: &[Control]) -> QaCheck {
    let id = "tui.controls.help";
    let label = "a keyboard help toggle is exposed in the controls list";
    let passed = controls.iter().any(|c| c.id == "help.toggle");
    let detail = if passed {
        "help.toggle control is exposed".to_owned()
    } else {
        "no help.toggle control is in the controls list".to_owned()
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

/// Pin the decision-tape data invariant the `render_decision_tape`
/// renderer consumes: at any step past `step = 0` every visible log
/// entry must have a non-empty `actor` AND a non-empty `action`, and
/// `visible_story().len()` must equal `current().log_count`. At
/// `step = 0` the decision-tape is empty by design (`render_decision_tape`
/// early-returns on `visible_story().is_empty()`), so the check is
/// trivially passed on the initial render. STW-027.
fn check_tape_actions_present(app: &App) -> QaCheck {
    let id = "tui.tape.actions_present";
    let label = "decision-tape log entries have populated actor + action fields";
    let current = app.preview.current();
    let story = app.preview.visible_story();
    let expected = current.log_count;
    let actual = story.len();
    let all_populated = story
        .iter()
        .all(|entry| !entry.actor.is_empty() && !entry.action.is_empty());
    let count_matches = actual == expected;
    let passed = all_populated && count_matches;
    let detail = if passed {
        if expected == 0 {
            format!(
                "{actual} log entr{} visible (initial step, tape empty by design)",
                if actual == 1 { "y" } else { "ies" }
            )
        } else {
            format!(
                "{actual} log entr{} visible, all actor/action fields populated",
                if actual == 1 { "y" } else { "ies" }
            )
        }
    } else if !count_matches {
        format!(
            "visible_story().len() = {actual} but current().log_count = {expected} \
             (delta = {})",
            actual as isize - expected as isize
        )
    } else {
        let empty_actors = story.iter().filter(|entry| entry.actor.is_empty()).count();
        let empty_actions = story.iter().filter(|entry| entry.action.is_empty()).count();
        format!(
            "{empty_actors} entry/entries with empty actor, {empty_actions} entry/entries with empty action"
        )
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

/// Pin the board-stage data invariant `visible_board().len() ==
/// current().board_cards`. `render_board_slots` paints the prefix
/// of `app.preview.board` whose length is clamped by
/// `current().board_cards`; any drift between the step's
/// `board_cards` field and the actual painted slice (e.g. a
/// `board_cards = 5` step on a `board.len() = 3` model) fails the
/// check. STW-027.
fn check_board_cards_present(app: &App) -> QaCheck {
    let id = "tui.board.cards_present";
    let label = "visible_board() length matches the current step's board_cards field";
    let current = app.preview.current();
    let expected = current.board_cards;
    let actual = app.preview.visible_board().len();
    let passed = actual == expected;
    let detail = if passed {
        format!("{actual} card(s) visible on the board stage for the current step")
    } else {
        format!(
            "visible_board().len() = {actual} but current().board_cards = {expected} \
             (delta = {})",
            actual as isize - expected as isize
        )
    };
    QaCheck {
        id,
        label,
        passed,
        detail,
    }
}

#[must_use]
pub fn controls() -> Vec<Control> {
    vec![
        Control {
            id: "preview.next",
            key: "Space / Enter",
            label: "Next beat",
            description: "Advance the local hand preview by one visible decision beat.",
            enabled: true,
        },
        Control {
            id: "preview.back",
            key: "b / Backspace",
            label: "Back",
            description: "Step back one beat without changing the deterministic hand.",
            enabled: true,
        },
        Control {
            id: "preview.random",
            key: "r",
            label: "New hand",
            description: "Load another deterministic local random-policy hand.",
            enabled: true,
        },
        Control {
            id: "focus.next",
            key: "Tab",
            label: "Focus rail",
            description: "Move focus between the chrome table and the information rail.",
            enabled: true,
        },
        Control {
            id: "help.toggle",
            key: "?",
            label: "Help",
            description: "Show or hide this generated control guide.",
            enabled: true,
        },
        Control {
            id: "app.quit",
            key: "q / Esc / Ctrl-C",
            label: "Quit",
            description: "Leave the read-only preview.",
            enabled: true,
        },
    ]
}

#[must_use]
pub fn handle_key(app: &mut App, key: KeyCode) -> bool {
    match key {
        KeyCode::Char(' ') | KeyCode::Enter => {
            if app.preview.advance() {
                app.motion.queue(MotionCue::StepForward);
            }
            false
        }
        KeyCode::Backspace | KeyCode::Char('b') => {
            if app.preview.retreat() {
                app.motion.queue(MotionCue::StepBack);
            }
            false
        }
        KeyCode::Char('r') => {
            app.preview = RandomPreview::from_seed(app.preview.seed.wrapping_add(1));
            app.focus = Focus::Table;
            app.motion.queue(MotionCue::NewHand);
            false
        }
        KeyCode::Tab => {
            app.focus = app.focus.next();
            app.motion.queue(MotionCue::Focus(app.focus));
            false
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
            app.motion.queue(MotionCue::Help);
            false
        }
        KeyCode::Esc => {
            if app.show_help {
                app.show_help = false;
                false
            } else {
                true
            }
        }
        KeyCode::Char('q') => true,
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum MotionCue {
    StepForward,
    StepBack,
    NewHand,
    Focus(Focus),
    Help,
}

impl MotionCue {
    const fn name(self) -> &'static str {
        match self {
            Self::StepForward => "step-forward",
            Self::StepBack => "step-back",
            Self::NewHand => "new-hand",
            Self::Focus(_) => "focus",
            Self::Help => "help",
        }
    }
}

#[derive(Default)]
struct MotionDirector {
    effects: EffectManager<String>,
    pending: Vec<MotionCue>,
    serial: u64,
}

impl MotionDirector {
    fn queue(&mut self, cue: MotionCue) {
        self.pending.push(cue);
    }

    fn is_running(&self) -> bool {
        self.effects.is_running() || !self.pending.is_empty()
    }

    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.pending.len()
    }

    fn process_frame(&mut self, elapsed: Duration, buf: &mut Buffer, area: Rect) {
        let pending = self.pending.drain(..).collect::<Vec<_>>();
        for cue in pending {
            let target = motion_target(cue, area);
            if target.is_empty() {
                continue;
            }
            let key = format!("{}-{}", cue.name(), self.serial);
            self.serial = self.serial.wrapping_add(1);
            self.effects
                .add_unique_effect(key, motion_effect(cue, target));
        }
        self.effects.process_effects(elapsed.into(), buf, area);
    }
}

fn motion_effect(cue: MotionCue, area: Rect) -> tachyonfx::Effect {
    match cue {
        MotionCue::StepForward => fx::parallel(&[
            fx::sweep_in(
                Motion::LeftToRight,
                area.width.min(14),
                1,
                Theme::stage(),
                (220, Interpolation::SineOut),
            )
            .with_area(area),
            fx::fade_from_fg(Theme::accent(), (160, Interpolation::SineOut)).with_area(area),
        ]),
        MotionCue::StepBack => fx::parallel(&[
            fx::sweep_in(
                Motion::RightToLeft,
                area.width.min(10),
                1,
                Theme::stage(),
                (180, Interpolation::SineOut),
            )
            .with_area(area),
            fx::fade_from_fg(Theme::ion(), (120, Interpolation::SineOut)).with_area(area),
        ]),
        MotionCue::NewHand => fx::sequence(&[
            fx::coalesce((120, Interpolation::ExpoOut)).with_area(area),
            fx::sweep_in(
                Motion::LeftToRight,
                area.width.min(16),
                2,
                Theme::bg(),
                (220, Interpolation::ExpoOut),
            )
            .with_area(area),
        ]),
        MotionCue::Focus(_) => {
            fx::fade_from_fg(Theme::ion(), (140, Interpolation::SineOut)).with_area(area)
        }
        MotionCue::Help => {
            fx::fade_from_fg(Theme::accent(), (120, Interpolation::SineOut)).with_area(area)
        }
    }
}

fn motion_target(cue: MotionCue, area: Rect) -> Rect {
    match cue {
        MotionCue::StepForward | MotionCue::StepBack | MotionCue::NewHand => {
            table_motion_rect(area)
        }
        MotionCue::Focus(focus) => focused_motion_rect(area, focus),
        MotionCue::Help => centered_rect(area, 72, 14),
    }
}

pub fn write_headless_artifacts(dir: &Path, app: &App, report: &HeadlessReport) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join("tui.surface.json"),
        serde_json::to_string_pretty(&report.surface)?,
    )?;
    fs::write(
        dir.join("tui.controls.json"),
        serde_json::to_string_pretty(&report.controls)?,
    )?;
    fs::write(dir.join("tui.frame.txt"), &report.frame)?;
    fs::write(
        dir.join("tui.frame.ansi"),
        render_ansi(
            app,
            report.surface.viewport.width,
            report.surface.viewport.height,
        ),
    )?;
    fs::write(
        dir.join("tui.qa.json"),
        serde_json::to_string_pretty(&report.qa)?,
    )?;
    fs::write(dir.join("tui.receipt.md"), receipt_markdown(app, report))?;
    Ok(())
}

#[must_use]
pub fn render_lines(app: &App, width: u16, height: u16) -> Vec<String> {
    buffer_to_lines(&render_buffer(app, width, height))
}

#[must_use]
pub fn render_ansi(app: &App, width: u16, height: u16) -> String {
    buffer_to_ansi(&render_buffer(app, width, height))
}

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    fill(area, buf, Theme::bg());

    if area.width < 78 || area.height < 24 {
        render_compact(app, area, buf);
    } else {
        render_full(app, area, buf);
    }

    if app.show_help {
        render_help(area, buf);
    }
}

fn render_buffer(app: &App, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    render(app, area, &mut buffer);
    buffer
}

fn table_motion_rect(area: Rect) -> Rect {
    if area.width < 78 || area.height < 24 {
        let [_header, stage, _rail, _footer] = compact_layout(area);
        return stage;
    }

    let (_header, _body, _footer, stage, _rail) = full_layout(area);
    stage
}

fn focused_motion_rect(area: Rect, focus: Focus) -> Rect {
    if area.width < 78 || area.height < 24 {
        let [_header, stage, rail, _footer] = compact_layout(area);
        return match focus {
            Focus::Table => stage,
            Focus::Rail => rail,
        };
    }

    let (_header, _body, _footer, stage, rail) = full_layout(area);
    match focus {
        Focus::Table => stage,
        Focus::Rail => rail,
    }
}

fn full_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let [header, body, footer] = areas3(
        area,
        Direction::Vertical,
        [
            Constraint::Length(2),
            Constraint::Min(18),
            Constraint::Length(1),
        ],
    );
    let rail_width = if body.width >= 112 { 28 } else { 24 };
    let [stage, rail] = areas2(
        body.inner(Margin {
            horizontal: 1,
            vertical: 0,
        }),
        Direction::Horizontal,
        [Constraint::Min(54), Constraint::Length(rail_width)],
    );
    (header, body, footer, stage, rail)
}

fn compact_layout(area: Rect) -> [Rect; 4] {
    areas4(
        area,
        Direction::Vertical,
        [
            Constraint::Length(2),
            Constraint::Min(14),
            Constraint::Length(7),
            Constraint::Length(1),
        ],
    )
}

fn render_full(app: &App, area: Rect, buf: &mut Buffer) {
    let (header, _body, footer, stage, rail) = full_layout(area);
    render_header(app, header, buf);
    render_table(app, stage, buf);
    render_rail(app, rail, buf);
    render_footer(footer, buf);
}

fn render_compact(app: &App, area: Rect, buf: &mut Buffer) {
    let [header, stage, rail, footer] = compact_layout(area);
    render_header(app, header, buf);
    render_table(app, stage, buf);
    render_rail(app, rail, buf);
    render_footer(footer, buf);
}

fn render_header(app: &App, area: Rect, buf: &mut Buffer) {
    let step = app.preview.current();
    Paragraph::new(vec![
        Line::from(vec![
            Span::styled("ROBOPOKER", Theme::brand()),
            Span::styled("  ", Theme::dim()),
            Span::styled(format!("#{}", app.preview.seed), Theme::badge()),
            Span::styled("   ", Theme::dim()),
            Span::styled(
                format!(
                    "{:>2} / {:<2}",
                    app.preview.step + 1,
                    app.preview.steps.len()
                ),
                Theme::accent_header(),
            ),
            Span::styled("   ", Theme::dim()),
            Span::styled(step.label.to_ascii_uppercase(), Theme::dim()),
        ]),
        Line::from(vec![
            Span::styled("offline", Theme::good_badge()),
            Span::styled("  read-only  ", Theme::dim()),
            Span::styled(step.actor, Theme::accent_header()),
            Span::styled(format!(" {}", step.action), Theme::dim()),
        ]),
    ])
    .style(Style::default().bg(Theme::bg()))
    .render(area, buf);
}

fn render_table(app: &App, area: Rect, buf: &mut Buffer) {
    let active = app.focus == Focus::Table;
    let block = Block::default()
        .title(Span::styled(" table ", Theme::panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if active {
            Theme::accent()
        } else {
            Theme::border()
        }))
        .style(Style::default().bg(Theme::stage()));
    let inner = block.inner(area);
    block.render(area, buf);
    fill(inner, buf, Theme::stage());

    let [opponent, board, hero, tape] = areas4(
        inner.inner(Margin {
            horizontal: 1,
            vertical: 0,
        }),
        Direction::Vertical,
        [
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Min(4),
        ],
    );

    let step = app.preview.current();
    render_player_row(
        &app.preview.opponent,
        opponent,
        false,
        step.show_opponent,
        step.show_strengths,
        buf,
    );
    render_board_stage(app, board, buf);
    render_player_row(
        &app.preview.hero,
        hero,
        true,
        step.show_hero,
        step.show_strengths,
        buf,
    );
    render_decision_tape(app, tape, buf);
}

fn render_player_row(
    player: &PlayerPreview,
    area: Rect,
    hero: bool,
    cards_visible: bool,
    strengths_visible: bool,
    buf: &mut Buffer,
) {
    let [label, _cards] = areas2(
        area,
        Direction::Horizontal,
        [Constraint::Length(12), Constraint::Min(18)],
    );
    let title = if hero { "YOU" } else { "FISH" };
    let subtitle = if strengths_visible {
        if hero { player.result } else { "random" }
    } else if cards_visible {
        if hero { "live" } else { "shown" }
    } else if hero {
        "ready"
    } else {
        "random"
    };
    Paragraph::new(vec![
        Line::from(Span::styled(title, Theme::eyebrow())),
        Line::from(Span::styled(subtitle, Theme::dim_on_stage())),
    ])
    .style(Style::default().bg(Theme::stage()))
    .render(label, buf);

    let card_area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    let visible_cards = if cards_visible {
        &player.cards[..]
    } else {
        &[]
    };
    render_card_slots(visible_cards, 2, card_area, buf);
    let strength_y = area.y + area.height.saturating_sub(1);
    let strength = if strengths_visible {
        player.strength.as_str()
    } else if hero && cards_visible {
        "private"
    } else {
        ""
    };
    Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default().bg(Theme::stage())),
        Span::styled(strength, Theme::strength()),
    ]))
    .alignment(Alignment::Center)
    .render(Rect::new(area.x, strength_y, area.width, 1), buf);
}

fn render_board_stage(app: &App, area: Rect, buf: &mut Buffer) {
    let [pot, cards, result] = areas3(
        area,
        Direction::Vertical,
        [
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(1),
        ],
    );
    Paragraph::new(Line::from(vec![
        Span::styled(format!("{}bb", app.preview.current().pot_bb), Theme::pot()),
        Span::styled(" pot", Theme::dim_on_stage()),
    ]))
    .alignment(Alignment::Center)
    .style(Style::default().bg(Theme::stage()))
    .render(pot, buf);
    render_board_slots(app.preview.visible_board(), 5, cards, buf);
    let result_line = if app.preview.current().show_strengths {
        Line::from(vec![
            Span::styled("SHOWDOWN ", Theme::dim_on_stage()),
            Span::styled(app.preview.winner, Theme::pot()),
        ])
    } else {
        let step = app.preview.current();
        Line::from(vec![
            Span::styled(
                format!(
                    "{:>2}/{:<2}  ",
                    app.preview.step + 1,
                    app.preview.steps.len()
                ),
                Theme::dim_on_stage(),
            ),
            Span::styled(step.actor, Theme::pot()),
            Span::styled(format!(" {}", step.action), Theme::dim_on_stage()),
        ])
    };
    Paragraph::new(result_line)
        .alignment(Alignment::Center)
        .style(Style::default().bg(Theme::stage()))
        .render(result, buf);
}

fn render_decision_tape(app: &App, area: Rect, buf: &mut Buffer) {
    if app.preview.visible_story().is_empty() {
        return;
    }

    let lines = app
        .preview
        .visible_story()
        .iter()
        .rev()
        .take(area.height.saturating_sub(1) as usize)
        .rev()
        .map(|entry| {
            Line::from(vec![
                Span::styled(format!("{:<9}", entry.actor), Theme::dim_on_stage()),
                Span::styled(format!("{:<15}", entry.action), Theme::text_on_stage()),
                Span::styled(&entry.detail, Theme::accent_on_stage()),
            ])
        })
        .collect::<Vec<_>>();
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Theme::stage_line()))
                .title(Span::styled(" log ", Theme::dim_on_stage())),
        )
        .style(Style::default().bg(Theme::stage()))
        .render(area, buf);
}

fn render_rail(app: &App, area: Rect, buf: &mut Buffer) {
    let active = app.focus == Focus::Rail;
    let block = Block::default()
        .title(Span::styled(" status ", Theme::panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if active {
            Theme::accent()
        } else {
            Theme::border()
        }))
        .style(Style::default().bg(Theme::panel()));
    let inner = block.inner(area);
    block.render(area, buf);

    let [run, controls_area] = areas2(
        inner.inner(Margin {
            horizontal: 1,
            vertical: 0,
        }),
        Direction::Vertical,
        [Constraint::Length(8), Constraint::Min(6)],
    );

    render_run_state(app, run, buf);
    render_controls(controls_area, buf);
}

fn render_run_state(app: &App, area: Rect, buf: &mut Buffer) {
    let step = app.preview.current();
    let pot = format!("{}bb", step.pot_bb);
    let actor_line = format!("{} {}", step.actor, short_action(step.action));
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:>2}/{:<2}", app.preview.step + 1, app.preview.steps.len()),
                Theme::hero_number(),
            ),
            Span::styled("  ", Theme::dim()),
            Span::styled(step.label.to_ascii_uppercase(), Theme::accent_style()),
        ]),
        Line::from(vec![Span::styled(actor_line, Theme::text_style())]),
        Line::from(vec![
            Span::styled(pot, Theme::accent_style()),
            Span::styled("  pot", Theme::dim()),
        ]),
        Line::from(vec![
            Span::styled(app.preview.winner_label(), Theme::text_style()),
            Span::styled("  winner", Theme::dim()),
        ]),
        Line::from(Span::styled("offline only", Theme::good_style())),
    ];
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Theme::quiet_style()),
        )
        .style(Theme::text_style())
        .render(area, buf);
}

fn short_action(action: &str) -> &str {
    match action {
        "awaiting input" => "ready",
        "checks cards" => "cards",
        "posts blinds" => "blinds",
        "reveals flop" => "flop",
        "reveals turn" => "turn",
        "reveals river" => "river",
        "resolves" => "showdown",
        value => value,
    }
}

fn render_controls(area: Rect, buf: &mut Buffer) {
    let lines = vec![
        Line::from(Span::styled("keys", Theme::rail_title())),
        Line::from(vec![
            Span::styled("Space", Theme::key_style()),
            Span::styled(" next", Theme::text_style()),
        ]),
        Line::from(vec![
            Span::styled("B", Theme::key_style()),
            Span::styled(" back", Theme::text_style()),
            Span::styled("   R", Theme::key_style()),
            Span::styled(" new", Theme::text_style()),
        ]),
        Line::from(vec![
            Span::styled("Tab", Theme::key_style()),
            Span::styled(" focus", Theme::text_style()),
        ]),
        Line::from(vec![
            Span::styled("?", Theme::key_style()),
            Span::styled(" help", Theme::text_style()),
            Span::styled("   Q", Theme::key_style()),
            Span::styled(" quit", Theme::text_style()),
        ]),
    ];
    Paragraph::new(lines)
        .style(Theme::text_style())
        .render(area, buf);
}

fn render_footer(area: Rect, buf: &mut Buffer) {
    Paragraph::new(Line::from(vec![
        Span::styled("Space", Theme::key_style()),
        Span::styled(" next   ", Theme::dim()),
        Span::styled("B", Theme::key_style()),
        Span::styled(" back   ", Theme::dim()),
        Span::styled("R", Theme::key_style()),
        Span::styled(" new   ", Theme::dim()),
        Span::styled("Tab", Theme::key_style()),
        Span::styled(" focus   ", Theme::dim()),
        Span::styled("?", Theme::key_style()),
        Span::styled(" help   ", Theme::dim()),
        Span::styled("Q/Esc", Theme::key_style()),
        Span::styled(" quit", Theme::dim()),
    ]))
    .style(Style::default().bg(Theme::bg()))
    .render(area, buf);
}

fn render_help(area: Rect, buf: &mut Buffer) {
    let modal = centered_rect(area, 72, 14);
    Clear.render(modal, buf);
    let lines = controls()
        .into_iter()
        .map(|control| {
            Line::from(vec![
                Span::styled(format!("{:<16}", control.key), Theme::key_style()),
                Span::styled(format!("{:<14}", control.label), Theme::text_style()),
                Span::styled(control.description, Theme::dim()),
            ])
        })
        .collect::<Vec<_>>();
    Paragraph::new(lines)
        .block(
            Block::default()
                .title(Span::styled(" generated help ", Theme::panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Theme::accent()))
                .style(Style::default().bg(Theme::panel())),
        )
        .style(Style::default().bg(Theme::panel()))
        .wrap(Wrap { trim: true })
        .render(modal, buf);
}

fn render_card_slots(cards: &[CardView], slots: usize, area: Rect, buf: &mut Buffer) {
    if slots == 0 || area.width < 2 || area.height == 0 {
        return;
    }

    let card_width = if area.width >= slots as u16 * 7 + slots.saturating_sub(1) as u16 {
        7
    } else if area.width >= slots as u16 * 5 {
        5
    } else {
        3
    };
    let gap = if card_width >= 5 { 1 } else { 0 };
    let total = slots as u16 * card_width + slots.saturating_sub(1) as u16 * gap;
    let mut x = area.x + area.width.saturating_sub(total) / 2;
    let y = area.y + area.height.saturating_sub(5) / 2;

    for index in 0..slots {
        let rect = Rect::new(x, y, card_width.min(area.width), area.height.min(5));
        if let Some(card) = cards.get(index) {
            render_card(card, rect, buf);
        } else {
            render_card_back(rect, buf);
        }
        x = x.saturating_add(card_width + gap);
    }
}

fn render_board_slots(cards: &[CardView], slots: usize, area: Rect, buf: &mut Buffer) {
    render_card_slots(cards, slots, area, buf);
}

fn render_card(card: &CardView, area: Rect, buf: &mut Buffer) {
    if area.width <= 3 || area.height <= 2 {
        Paragraph::new(format!("{}{}", card.rank, card.suit))
            .style(card_style(card))
            .alignment(Alignment::Center)
            .render(area, buf);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::card_edge()).bg(Theme::card()))
        .style(Style::default().bg(Theme::card()));
    let inner = block.inner(area);
    block.render(area, buf);
    let width = inner.width as usize;
    Paragraph::new(vec![
        Line::from(Span::styled(
            format!("{:<width$}", card.rank.as_str()),
            card_style(card),
        )),
        Line::from(Span::styled(
            centered_text(&card.suit.to_string(), width),
            card_style(card),
        )),
        Line::from(Span::styled(
            format!("{:>width$}", card.rank.as_str()),
            card_style(card),
        )),
    ])
    .render(inner, buf);
}

fn render_card_back(area: Rect, buf: &mut Buffer) {
    if area.width <= 3 || area.height <= 2 {
        Paragraph::new(" ")
            .style(Style::default().bg(Theme::card_back()))
            .alignment(Alignment::Center)
            .render(area, buf);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(Theme::card_back_edge())
                .bg(Theme::card_back()),
        )
        .style(Style::default().bg(Theme::card_back()));
    block.render(area, buf);
}

fn centered_text(value: &str, width: usize) -> String {
    let value_width = value.chars().count();
    if value_width >= width {
        return value.chars().take(width).collect();
    }
    let left = (width - value_width) / 2;
    let right = width - value_width - left;
    format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
}

fn card_style(card: &CardView) -> Style {
    Style::default()
        .fg(if card.red {
            Theme::red_suit()
        } else {
            Theme::black_suit()
        })
        .bg(Theme::card())
        .add_modifier(Modifier::BOLD)
}

fn strength_for(hole: &[Card], board: &[Card]) -> Strength {
    let hand = Hand::from(
        hole.iter()
            .chain(board.iter())
            .copied()
            .collect::<Vec<Card>>(),
    );
    Strength::from(hand)
}

fn street_cards(cards: &[Card]) -> String {
    cards
        .iter()
        .map(|card| format!("{}{}", card.rank(), card.suit().ascii()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn result_for(player: &str, winner: &str) -> &'static str {
    if winner == "Split" {
        "split pot"
    } else if player == winner {
        "wins pot"
    } else {
        "loses"
    }
}

fn human_strength(strength: &Strength) -> String {
    let raw = strength
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut parts = raw.split_whitespace();
    let Some(kind) = parts.next() else {
        return raw;
    };
    let rest = parts.collect::<Vec<_>>();
    match (kind, rest.as_slice()) {
        ("StraightFlush", [high]) => format!("Straight flush · {high} high"),
        ("FourOfAKind", [rank, kicker]) => format!("Quads {rank} · {kicker} kicker"),
        ("FullHouse", [ranks]) => {
            let mut chars = ranks.chars();
            match (chars.next(), chars.next()) {
                (Some(trips), Some(pair)) => format!("Full house · {trips}s over {pair}s"),
                _ => raw,
            }
        }
        ("Flush", [high]) => format!("Flush · {high} high"),
        ("Straight", [high]) => format!("Straight · {high} high"),
        ("ThreeOfAKind", [rank, kickers @ ..]) => {
            format!("Trips {rank} · {}", kickers.join(" "))
        }
        ("TwoPair", [ranks]) => {
            let mut chars = ranks.chars();
            match (chars.next(), chars.next(), chars.next()) {
                (Some(hi), Some(lo), Some(kicker)) => {
                    format!("Two pair · {hi}s/{lo}s · {kicker} kicker")
                }
                _ => raw,
            }
        }
        ("OnePair", [rank, kickers @ ..]) => {
            format!("Pair of {rank}s · {}", kickers.join(" "))
        }
        ("HighCard", [high, kickers @ ..]) => {
            format!("{high} high · {}", kickers.join(" "))
        }
        _ => raw,
    }
}

fn receipt_markdown(app: &App, report: &HeadlessReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# robopoker TUI QA Receipt\n\n- verdict: `{}`\n- viewport: `{}x{}`\n- frame hash: `{}`\n- focus: `{:?}`\n- seed: `{}`\n- winner: `{}`\n- posture: `{}`\n- checks: `{} total, {} failed`\n",
        report.qa.verdict,
        report.surface.viewport.width,
        report.surface.viewport.height,
        report.qa.frame_hash,
        app.focus,
        app.preview.seed,
        app.preview.winner,
        report.surface.posture,
        report.qa.checks.len(),
        report.qa.assertions.len(),
    ));
    out.push_str("\n## QA Checks\n\n");
    out.push_str(
        "Each line below is `QA-CHECK <id> <passed|failed> <detail>` so a testnet\ndashboard can `grep '^QA-CHECK tui\\.' receipts/.../tui.receipt.md` to detect a\nregression without parsing JSON.\n\n",
    );
    for check in &report.qa.checks {
        let state = if check.passed { "passed" } else { "failed" };
        out.push_str(&format!(
            "- QA-CHECK {} {} — {}\n",
            check.id, state, check.detail
        ));
    }
    out.push_str("\nArtifacts in this directory are generated by `robopoker-tui --headless`.\n");
    out
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn fill(area: Rect, buf: &mut Buffer, color: Color) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            buf[(x, y)]
                .set_symbol(" ")
                .set_style(Style::default().bg(color));
        }
    }
}

fn areas2(area: Rect, direction: Direction, constraints: [Constraint; 2]) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area);
    [chunks[0], chunks[1]]
}

fn areas3(area: Rect, direction: Direction, constraints: [Constraint; 3]) -> [Rect; 3] {
    let chunks = Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area);
    [chunks[0], chunks[1], chunks[2]]
}

fn areas4(area: Rect, direction: Direction, constraints: [Constraint; 4]) -> [Rect; 4] {
    let chunks = Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area);
    [chunks[0], chunks[1], chunks[2], chunks[3]]
}

fn buffer_to_lines(buf: &Buffer) -> Vec<String> {
    let area = buf.area();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn buffer_to_ansi(buf: &Buffer) -> String {
    let area = buf.area();
    let mut out = String::new();
    for y in 0..area.height {
        let mut current_style = None;
        for x in 0..area.width {
            let cell = &buf[(x, y)];
            let style = cell.style();
            if current_style != Some(style) {
                out.push_str(&ansi_style(style));
                current_style = Some(style);
            }
            out.push_str(cell.symbol());
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

fn ansi_style(style: Style) -> String {
    let mut codes = Vec::new();
    if style.add_modifier.contains(Modifier::BOLD) {
        codes.push("1".to_owned());
    }
    if let Some(fg) = style.fg {
        codes.push(ansi_color(fg, true));
    }
    if let Some(bg) = style.bg {
        codes.push(ansi_color(bg, false));
    }
    if codes.is_empty() {
        "\x1b[0m".to_owned()
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

fn ansi_color(color: Color, foreground: bool) -> String {
    let prefix = if foreground { 38 } else { 48 };
    match color {
        Color::Rgb(r, g, b) => format!("{prefix};2;{r};{g};{b}"),
        Color::Black => (if foreground { 30 } else { 40 }).to_string(),
        Color::Red => (if foreground { 31 } else { 41 }).to_string(),
        Color::Green => (if foreground { 32 } else { 42 }).to_string(),
        Color::Yellow => (if foreground { 33 } else { 43 }).to_string(),
        Color::Blue => (if foreground { 34 } else { 44 }).to_string(),
        Color::Magenta => (if foreground { 35 } else { 45 }).to_string(),
        Color::Cyan => (if foreground { 36 } else { 46 }).to_string(),
        Color::Gray | Color::White => (if foreground { 37 } else { 47 }).to_string(),
        Color::DarkGray => (if foreground { 90 } else { 100 }).to_string(),
        Color::LightRed => (if foreground { 91 } else { 101 }).to_string(),
        Color::LightGreen => (if foreground { 92 } else { 102 }).to_string(),
        Color::LightYellow => (if foreground { 93 } else { 103 }).to_string(),
        Color::LightBlue => (if foreground { 94 } else { 104 }).to_string(),
        Color::LightMagenta => (if foreground { 95 } else { 105 }).to_string(),
        Color::LightCyan => (if foreground { 96 } else { 106 }).to_string(),
        Color::Indexed(index) => format!("{prefix};5;{index}"),
        Color::Reset => (if foreground { 39 } else { 49 }).to_string(),
    }
}

fn hash_frame(frame: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in frame.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

struct PreviewRng(u64);

impl PreviewRng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn next_usize(&mut self, max: usize) -> usize {
        (self.next() as usize) % max
    }

    fn pick<'a>(&mut self, values: &'a [&'a str]) -> &'a str {
        values[self.next_usize(values.len())]
    }
}

struct Theme;

impl Theme {
    const fn bg() -> Color {
        Color::Rgb(5, 7, 10)
    }
    const fn panel() -> Color {
        Color::Rgb(12, 15, 20)
    }
    const fn stage() -> Color {
        Color::Rgb(14, 18, 24)
    }
    const fn stage_line() -> Color {
        Color::Rgb(58, 68, 86)
    }
    const fn card() -> Color {
        Color::Rgb(29, 34, 43)
    }
    const fn card_back() -> Color {
        Color::Rgb(15, 20, 28)
    }
    const fn card_edge() -> Color {
        Color::Rgb(73, 84, 102)
    }
    const fn card_back_edge() -> Color {
        Color::Rgb(40, 49, 64)
    }
    const fn text() -> Color {
        Color::Rgb(232, 237, 243)
    }
    const fn muted() -> Color {
        Color::Rgb(130, 145, 164)
    }
    const fn border() -> Color {
        Color::Rgb(42, 52, 68)
    }
    const fn accent() -> Color {
        Color::Rgb(224, 174, 76)
    }
    const fn ion() -> Color {
        Color::Rgb(80, 220, 198)
    }
    const fn red_suit() -> Color {
        Color::Rgb(224, 94, 104)
    }
    const fn black_suit() -> Color {
        Color::Rgb(220, 226, 235)
    }

    fn brand() -> Style {
        Style::default()
            .fg(Self::text())
            .bg(Self::bg())
            .add_modifier(Modifier::BOLD)
    }
    fn badge() -> Style {
        Style::default()
            .fg(Self::bg())
            .bg(Self::accent())
            .add_modifier(Modifier::BOLD)
    }
    fn accent_header() -> Style {
        Style::default()
            .fg(Self::accent())
            .bg(Self::bg())
            .add_modifier(Modifier::BOLD)
    }
    fn good_badge() -> Style {
        Style::default()
            .fg(Self::ion())
            .bg(Self::bg())
            .add_modifier(Modifier::BOLD)
    }
    fn panel_title() -> Style {
        Style::default()
            .fg(Self::text())
            .bg(Self::panel())
            .add_modifier(Modifier::BOLD)
    }
    fn rail_title() -> Style {
        Style::default()
            .fg(Self::accent())
            .bg(Self::panel())
            .add_modifier(Modifier::BOLD)
    }
    fn hero_number() -> Style {
        Style::default()
            .fg(Self::accent())
            .bg(Self::panel())
            .add_modifier(Modifier::BOLD)
    }
    fn eyebrow() -> Style {
        Style::default()
            .fg(Self::text())
            .bg(Self::stage())
            .add_modifier(Modifier::BOLD)
    }
    fn strength() -> Style {
        Style::default().fg(Self::muted()).bg(Self::stage())
    }
    fn pot() -> Style {
        Style::default()
            .fg(Self::accent())
            .bg(Self::stage())
            .add_modifier(Modifier::BOLD)
    }
    fn text_on_stage() -> Style {
        Style::default().fg(Self::text()).bg(Self::stage())
    }
    fn accent_on_stage() -> Style {
        Style::default()
            .fg(Self::text())
            .bg(Self::stage())
            .add_modifier(Modifier::BOLD)
    }
    fn dim_on_stage() -> Style {
        Style::default().fg(Self::muted()).bg(Self::stage())
    }
    fn text_style() -> Style {
        Style::default().fg(Self::text()).bg(Self::panel())
    }
    fn good_style() -> Style {
        Style::default()
            .fg(Self::ion())
            .bg(Self::panel())
            .add_modifier(Modifier::BOLD)
    }
    fn accent_style() -> Style {
        Style::default()
            .fg(Self::accent())
            .bg(Self::panel())
            .add_modifier(Modifier::BOLD)
    }
    fn dim() -> Style {
        Style::default().fg(Self::muted())
    }
    fn quiet_style() -> Style {
        Style::default().fg(Self::border()).bg(Self::panel())
    }
    fn key_style() -> Style {
        Style::default()
            .fg(Self::accent())
            .add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn renders_table_first_random_opponent_preview() {
        let app = App::default();
        let lines = render_lines(&app, 96, 28);
        let frame = lines.join("\n");

        assert!(frame.contains("ROBOPOKER"));
        assert!(frame.contains("table"));
        assert!(frame.contains("FISH"));
        assert!(frame.contains("YOU"));
        assert!(frame.contains("1 / 14"));
        assert!(frame.contains("awaiting input"));
        assert!(!frame.contains("SHOWDOWN"));
        assert!(frame.contains("random"));
        assert!(frame.contains("offline"));
        assert!(lines.len() <= 28);
        assert!(lines.iter().all(|line| line.chars().count() <= 96));
    }

    #[test]
    fn space_advances_exactly_one_visible_beat() {
        let mut app = App::with_seed(42);
        assert_eq!(app.preview.step, 0);

        assert!(!handle_key(&mut app, KeyCode::Char(' ')));

        assert_eq!(app.preview.step, 1);
        assert_eq!(app.preview.current().actor, "Hero");
        assert!(app.preview.current().show_hero);
        assert!(!app.preview.current().show_opponent);
        assert_eq!(app.preview.current().board_cards, 0);

        let frame = render_lines(&app, 96, 28).join("\n");
        assert!(frame.contains("Hero"));
        assert!(frame.contains("two private cards"));
        assert!(!frame.contains("SHOWDOWN"));
    }

    #[test]
    fn back_steps_without_redealing() {
        let mut app = App::with_seed(42);
        let original_winner = app.preview.winner;

        assert!(!handle_key(&mut app, KeyCode::Enter));
        assert!(!handle_key(&mut app, KeyCode::Enter));
        assert_eq!(app.preview.step, 2);

        assert!(!handle_key(&mut app, KeyCode::Backspace));

        assert_eq!(app.preview.step, 1);
        assert_eq!(app.preview.seed, 42);
        assert_eq!(app.preview.winner, original_winner);
    }

    #[test]
    fn new_hand_key_advances_seed_and_changes_frame() {
        let mut app = App::with_seed(42);
        let before = render_lines(&app, 96, 28).join("\n");

        assert!(!handle_key(&mut app, KeyCode::Char('r')));

        let after = render_lines(&app, 96, 28).join("\n");
        assert_eq!(app.preview.seed, 43);
        assert_eq!(app.preview.step, 0);
        assert_eq!(app.focus, Focus::Table);
        assert_ne!(before, after);
        assert!(["Hero", "Fish", "Split"].contains(&app.preview.winner));
    }

    #[test]
    fn generated_help_is_keyboard_accessible() {
        let mut app = App::default();
        assert!(!app.show_help);
        assert!(!handle_key(&mut app, KeyCode::Char('?')));
        assert!(app.show_help);

        let frame = render_lines(&app, 96, 28).join("\n");
        assert!(frame.contains("generated help"));
        assert!(frame.contains("Next beat"));
        assert!(frame.contains("Back"));
        assert!(frame.contains("New hand"));
        assert!(frame.contains("Focus rail"));
        assert!(frame.contains("Quit"));
    }

    #[test]
    fn tab_cycles_focus_without_private_actions() {
        let mut app = App::default();
        assert_eq!(app.focus, Focus::Table);
        assert!(!handle_key(&mut app, KeyCode::Tab));
        assert_eq!(app.focus, Focus::Rail);
        assert!(!handle_key(&mut app, KeyCode::Tab));
        assert_eq!(app.focus, Focus::Table);
    }

    #[test]
    fn headless_artifacts_are_machine_readable() {
        let app = App::default();
        let report = HeadlessReport::capture(&app, 80, 24);
        let dir = std::env::temp_dir().join(format!(
            "robopoker-tui-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should move forward")
                .as_nanos()
        ));

        write_headless_artifacts(&dir, &app, &report).expect("artifacts should write");

        for name in [
            "tui.surface.json",
            "tui.controls.json",
            "tui.frame.txt",
            "tui.frame.ansi",
            "tui.qa.json",
            "tui.receipt.md",
        ] {
            assert!(dir.join(name).exists(), "{name} should exist");
        }

        let qa = fs::read_to_string(dir.join("tui.qa.json")).expect("qa json should read");
        assert!(qa.contains("\"verdict\": \"passed\""));
        assert_eq!(report.qa.controls, 6);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_can_render_a_specific_interaction_step() {
        let app = App::with_seed_and_step(49363, 8);
        assert_eq!(app.preview.step, 7);
        assert_eq!(app.preview.current().label, "flop");
        assert_eq!(app.preview.visible_board().len(), 3);

        let frame = render_lines(&app, 96, 28).join("\n");
        assert!(frame.contains("8/14"));
        assert!(frame.contains("random bet"));
    }

    #[test]
    fn cards_use_compact_chrome_geometry() {
        let area = Rect::new(0, 0, 9, 5);
        let mut back = Buffer::empty(area);
        render_card_slots(&[], 1, area, &mut back);

        assert_eq!(back[(4, 2)].symbol(), " ");
        assert_eq!(back[(4, 2)].style().bg, Some(Theme::card_back()));
        assert_eq!(back[(1, 0)].symbol(), "╭");
        assert_eq!(back[(7, 4)].symbol(), "╯");
        assert_eq!(back[(1, 0)].style().fg, Some(Theme::card_back_edge()));
        assert_eq!(back[(1, 0)].style().bg, Some(Theme::card_back()));

        let mut face = Buffer::empty(area);
        render_card_slots(
            &[CardView {
                rank: "10".to_owned(),
                suit: '♠',
                red: false,
            }],
            1,
            area,
            &mut face,
        );

        assert_eq!(face[(2, 1)].symbol(), "1");
        assert_eq!(face[(3, 1)].symbol(), "0");
        assert_eq!(face[(4, 2)].symbol(), "♠");
        assert_eq!(face[(5, 3)].symbol(), "1");
        assert_eq!(face[(6, 3)].symbol(), "0");
        assert_eq!(face[(4, 2)].style().bg, Some(Theme::card()));
        assert_eq!(face[(1, 0)].style().fg, Some(Theme::card_edge()));
    }

    #[test]
    fn tachyonfx_step_animation_changes_buffer_styles() {
        let mut app = App::default();
        assert!(!handle_key(&mut app, KeyCode::Char(' ')));
        assert_eq!(app.pending_motion_count(), 1);

        let area = Rect::new(0, 0, 96, 28);
        let mut animated = Buffer::empty(area);
        render(&app, area, &mut animated);
        let static_frame = animated.clone();

        app.process_motion(Duration::from_millis(110), &mut animated, area);

        assert_eq!(app.pending_motion_count(), 0);
        assert!(
            changed_cell_count(&static_frame, &animated, area) > 0,
            "tachyonfx should visibly modify the rendered buffer"
        );
    }

    #[test]
    fn ansi_frame_preserves_terminal_style_layer() {
        let app = App::default();
        let ansi = render_ansi(&app, 80, 24);
        let plain = strip_ansi_for_test(&ansi);

        assert!(ansi.contains("\x1b["));
        assert!(plain.contains("ROBOPOKER"));
        assert!(plain.contains("FISH"));
        assert!(plain.contains("YOU"));
    }

    // ---- STW-021: headless QA report is a real gate ----

    /// Green-path gate: every check passes for the default app and
    /// the top-level verdict is `"passed"`. The `assertions` field
    /// (the backward-compat surface the existing
    /// `headless_artifacts_are_machine_readable` test grep'd on) is
    /// empty for a fully green run.
    #[test]
    fn qa_gate_passes_for_default_app() {
        let app = App::default();
        let report = HeadlessReport::capture(&app, 96, 28);

        assert_eq!(report.qa.verdict, "passed");
        assert!(
            report.qa.assertions.is_empty(),
            "a green run should have no failing-check ids in qa.assertions, got {:?}",
            report.qa.assertions
        );
        assert!(
            !report.qa.checks.is_empty(),
            "the QA report should expose a per-check breakdown"
        );
        for check in &report.qa.checks {
            assert!(
                check.passed,
                "check {} unexpectedly failed: {}",
                check.id, check.detail
            );
        }
    }

    /// All expected check ids are present in the report. A future
    /// refactor that drops a check (or renames a check id without
    /// updating the dashboard's grep pattern) fails this test.
    #[test]
    fn qa_gate_includes_every_expected_check_id() {
        let app = App::default();
        let report = HeadlessReport::capture(&app, 96, 28);
        let expected = [
            "tui.chrome.brand",
            "tui.chrome.players",
            "tui.chrome.posture",
            "tui.viewport.bounds",
            "tui.controls.ids_unique",
            "tui.controls.keys_unique",
            "tui.controls.count",
            "tui.cards.evaluator",
            "tui.controls.help",
        ];
        let present: Vec<&'static str> = report.qa.checks.iter().map(|c| c.id).collect();
        for needle in expected {
            assert!(
                present.contains(&needle),
                "expected check id `{needle}` missing from report.qa.checks (got {present:?})"
            );
        }
    }

    /// `compute_verdict` is the pure AND of every check's `passed`
    /// field. A red check flips the verdict to `"failed"` regardless
    /// of the other checks' state.
    #[test]
    fn compute_verdict_flips_to_failed_on_any_check_failure() {
        let green = vec![QaCheck {
            id: "demo.ok",
            label: "always-ok",
            passed: true,
            detail: "demo".to_owned(),
        }];
        assert_eq!(compute_verdict(&green), "passed");

        let mixed = vec![
            QaCheck {
                id: "demo.ok",
                label: "always-ok",
                passed: true,
                detail: "demo".to_owned(),
            },
            QaCheck {
                id: "demo.bad",
                label: "always-failing",
                passed: false,
                detail: "demo".to_owned(),
            },
        ];
        assert_eq!(compute_verdict(&mixed), "failed");
    }

    /// The `tui.chrome.brand` check fires when the frame loses its
    /// `ROBOPOKER` header. The check is the smallest direct trigger
    /// we can hand-craft (it does not depend on the app state or
    /// controls); the `assertions` field then contains the failing
    /// check id, the top-level verdict flips to `"failed"`, and the
    /// receipt markdown shows a `QA-CHECK tui.chrome.brand failed`
    /// line.
    #[test]
    fn qa_gate_flips_to_failed_when_chrome_brand_missing() {
        let broken = check_chrome_branding(
            "this is some text without the brand header, and no FISH or YOU either",
        );
        assert_eq!(broken.id, "tui.chrome.brand");
        assert!(!broken.passed);
        assert!(broken.detail.contains("ROBOPOKER"));

        let one_failing = vec![broken];
        let report_for_one_failing = HeadlessReport {
            surface: SurfaceMeta {
                app_id: "robopoker-tui",
                schema_version: SURFACE_SCHEMA_VERSION,
                viewport: Viewport {
                    width: 80,
                    height: 24,
                },
                theme: "black-chrome-minimal",
                source: "test",
                posture: "read-only",
            },
            controls: vec![],
            frame: String::new(),
            qa: QaReport {
                verdict: compute_verdict(&one_failing),
                assertions: one_failing
                    .iter()
                    .filter(|c| !c.passed)
                    .map(|c| c.id)
                    .collect(),
                frame_hash: 0,
                controls: 0,
                checks: one_failing.clone(),
            },
        };
        assert_eq!(report_for_one_failing.qa.verdict, "failed");
        assert_eq!(
            report_for_one_failing.qa.assertions,
            vec!["tui.chrome.brand"],
            "the failing check id should land in qa.assertions"
        );

        // The receipt markdown exposes the per-check `id` and
        // `passed` state in a `## QA Checks` section so a testnet
        // dashboard can grep the receipt without parsing JSON. The
        // prefix `QA-CHECK tui.` is the contract the dashboard
        // scrapes.
        let rendered = receipt_markdown_for_test(&App::default(), &report_for_one_failing);
        assert!(rendered.contains("verdict: `failed`"));
        assert!(rendered.contains("- QA-CHECK tui.chrome.brand failed"));
        assert!(rendered.contains("## QA Checks"));
    }

    /// Receipt markdown lists every check with a `QA-CHECK <id>
    /// <passed|failed>` line a dashboard can grep on. This is the
    /// end-to-end contract for the headless receipt shape.
    #[test]
    fn receipt_markdown_lists_every_check_as_qa_check_line() {
        let app = App::default();
        let report = HeadlessReport::capture(&app, 96, 28);
        let rendered = receipt_markdown_for_test(&app, &report);

        assert!(rendered.contains("verdict: `passed`"));
        assert!(rendered.contains("## QA Checks"));
        for check in &report.qa.checks {
            let line = format!("- QA-CHECK {} passed", check.id);
            assert!(
                rendered.contains(&line),
                "expected `{line}` in tui.receipt.md:\n{rendered}"
            );
        }
    }

    /// `controls()` is the public surface a testnet dashboard
    /// scrapes. The unique-id + unique-key + count + help-toggle
    /// checks guard against a refactor that adds a control without
    /// wiring it through the QA gate. Each is a unit test on the
    /// check fn, not on the full capture, so a regression points
    /// straight at the check.
    #[test]
    fn per_check_guards_match_published_controls_surface() {
        let controls = controls();
        let ids_check = check_controls_unique(&controls);
        let keys_check = check_controls_keys_unique(&controls);
        let count_check = check_controls_count(&controls);
        let help_check = check_help_toggle(&controls);

        assert!(ids_check.passed, "ids check: {}", ids_check.detail);
        assert!(keys_check.passed, "keys check: {}", keys_check.detail);
        assert!(count_check.passed, "count check: {}", count_check.detail);
        assert!(help_check.passed, "help check: {}", help_check.detail);
        assert_eq!(ids_check.id, "tui.controls.ids_unique");
        assert_eq!(keys_check.id, "tui.controls.keys_unique");
        assert_eq!(count_check.id, "tui.controls.count");
        assert_eq!(help_check.id, "tui.controls.help");
    }

    /// Test-only wrapper over the private `receipt_markdown` so the
    /// lib tests can inspect the per-check breakdown a testnet
    /// dashboard would scrape. Kept here (not in the production
    /// module) so the public surface of `bin/tui` does not grow a
    /// `receipt_markdown` re-export the dashboard doesn't need.
    fn receipt_markdown_for_test(app: &App, report: &HeadlessReport) -> String {
        super::receipt_markdown(app, report)
    }

    fn strip_ansi_for_test(value: &str) -> String {
        let mut out = String::new();
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                let _ = chars.next();
                for code in chars.by_ref() {
                    if code.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn changed_cell_count(before: &Buffer, after: &Buffer, area: Rect) -> usize {
        let mut changed = 0;
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if before[(x, y)].symbol() != after[(x, y)].symbol()
                    || before[(x, y)].style() != after[(x, y)].style()
                {
                    changed += 1;
                }
            }
        }
        changed
    }

    // ---- STW-027: decision-tape + board-stage QA coverage ----

    /// Positive arm: when the current step's `log_count` matches the
    /// number of `visible_story()` entries and every entry has
    /// non-empty `actor` + `action` fields, the
    /// `tui.tape.actions_present` check passes with a detail that
    /// names the visible entry count. STW-027.
    #[test]
    fn check_tape_actions_present_passes_on_populated_log() {
        // Hand-build a deterministic fixture: a fresh app from a known
        // seed, then pin `step` to a known-good index and rewrite the
        // target step's `log_count` so it matches the slice the
        // renderer will paint. Using the real `RandomPreview::from_seed`
        // path keeps the populated story entries (actor/action
        // fields) consistent with what the renderer actually consumes.
        let mut app = App::with_seed(7);
        let step = 3_usize;
        app.preview.step = step;
        let visible_entries = app.preview.story.len().min(3);
        app.preview.steps[step].log_count = visible_entries;

        let check = check_tape_actions_present(&app);
        assert_eq!(check.id, "tui.tape.actions_present");
        assert!(
            check.passed,
            "populated-log check should pass: {}",
            check.detail
        );
        assert!(
            check.detail.contains(&visible_entries.to_string()),
            "detail should name the visible entry count, got `{}`",
            check.detail
        );
    }

    /// Trivially-passed arm: at `step = 0` the decision-tape is empty
    /// by design (`render_decision_tape` early-returns on
    /// `visible_story().is_empty()`), and the check is passed with a
    /// detail that names the empty-by-design state. STW-027.
    #[test]
    fn check_tape_actions_present_passes_on_empty_log() {
        let app = App::default();
        assert_eq!(app.preview.step, 0);
        assert_eq!(app.preview.current().log_count, 0);

        let check = check_tape_actions_present(&app);
        assert_eq!(check.id, "tui.tape.actions_present");
        assert!(
            check.passed,
            "empty-log check should pass (initial step is empty by design): {}",
            check.detail
        );
        assert!(
            check.detail.contains("0"),
            "detail should name the visible entry count (0), got `{}`",
            check.detail
        );
    }

    /// Positive arm: when the current step's `board_cards` matches
    /// the visible slice length (`visible_board().len()`), the
    /// `tui.board.cards_present` check passes with a detail that names
    /// the visible card count. STW-027.
    #[test]
    fn check_board_cards_present_passes_when_slice_matches() {
        let mut app = App::with_seed(7);
        // `RandomPreview::from_seed` always populates 5 board cards;
        // we just need to pick a step whose `board_cards` field is
        // already <= board.len() (the real steps from_seed builds
        // already satisfy this for the flop, turn, and river steps).
        let step = 8_usize; // 1-based 9 → step 8, typically the "flop" stage
        app.preview.step = step;
        let visible_cards = app.preview.steps[step]
            .board_cards
            .min(app.preview.board.len());
        // The from_seed stage is already consistent (visible_cards is
        // bounded by both), but pin it explicitly so the test reads
        // as a positive arm of the contract.
        app.preview.steps[step].board_cards = visible_cards;

        let check = check_board_cards_present(&app);
        assert_eq!(check.id, "tui.board.cards_present");
        assert!(
            check.passed,
            "matching-slice check should pass: {}",
            check.detail
        );
        assert!(
            check.detail.contains(&visible_cards.to_string()),
            "detail should name the visible card count, got `{}`",
            check.detail
        );
    }

    /// Negative arm: when the current step's `board_cards` field is
    /// greater than the actual `visible_board().len()` (the step
    /// says "reveal 5 board cards" but only 3 are actually in the
    /// model), the `tui.board.cards_present` check fails with a
    /// detail that names the `expected` vs `actual` card count
    /// delta — the exact class of bug the check is designed to
    /// catch. STW-027.
    #[test]
    fn check_board_cards_present_fails_on_inconsistent_state() {
        let mut app = App::with_seed(7);
        // Truncate the board to a known smaller length so the test
        // is reproducible regardless of the seed's exact card count.
        app.preview.board.truncate(3);
        let step = 8_usize;
        app.preview.step = step;
        // Force an inconsistent state: step says 5 cards visible,
        // model only has 3.
        app.preview.steps[step].board_cards = 5;

        let check = check_board_cards_present(&app);
        assert_eq!(check.id, "tui.board.cards_present");
        assert!(
            !check.passed,
            "inconsistent-state check should fail, got `{}`",
            check.detail
        );
        // The check's detail message uses the exact phrasing
        // `visible_board().len() = <actual> but current().board_cards
        // = <expected>` so an operator reading the receipt knows which
        // side of the contract drifted. The two counts (3 and 5) must
        // both appear so the breach is locatable from the receipt
        // alone.
        assert!(
            check.detail.contains("3") && check.detail.contains("5"),
            "detail should name both the actual (3) and expected (5) card counts, got `{}`",
            check.detail
        );
    }
}
