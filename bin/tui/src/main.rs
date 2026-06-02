use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use robopoker_tui::{App, Cli, HeadlessReport, handle_key, write_headless_artifacts};
use std::io;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.headless {
        let app = App::with_seed_and_step(cli.seed, cli.step);
        let report = HeadlessReport::capture(&app, cli.width, cli.height);
        if let Some(dir) = cli.export_dir.as_deref() {
            write_headless_artifacts(dir, &app, &report)?;
        }
        println!("{}", serde_json::to_string_pretty(&report.qa)?);
        return Ok(());
    }

    run_interactive(cli.seed, cli.step)
}

fn run_interactive(seed: u64, step: usize) -> Result<()> {
    let mut guard = TerminalGuard::activate()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::with_seed_and_step(seed, step);
    let mut last_frame = Instant::now();

    let result = loop {
        let elapsed = last_frame.elapsed();
        last_frame = Instant::now();
        terminal.draw(|frame| {
            let area = frame.area();
            robopoker_tui::render(&app, area, frame.buffer_mut());
            app.process_motion(elapsed, frame.buffer_mut(), area);
        })?;

        let poll_for = if app.motion_is_running() {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(50)
        };
        if event::poll(poll_for)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Char('c')
                        && key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        break Ok(());
                    }
                    if handle_key(&mut app, key.code) {
                        break Ok(());
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    };

    guard.restore(&mut terminal)?;
    result
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn activate() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self { active: true })
    }

    fn restore(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        self.active = false;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
        }
    }
}
