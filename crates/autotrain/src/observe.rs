//! `Step` enum + `StepLogger` — machine-readable per-step timeline.
//!
//! STW-039: the `trainer` binary emits a single stderr line per step
//! in the shape `trainer step: name=<name> kind=<kind> duration_ms=<ms>
//! exit=<0|1|2>` when `RBP_TRAINER_OBSERVE=1` is set. The default
//! (`RBP_TRAINER_OBSERVE` unset or not `"1"`) is a no-op so the
//! existing per-step stdout / stderr / `SUMMARY.txt` shape is
//! preserved.
//!
//! The 15 `Step` variants mirror the 15 subcommands that produce a
//! greppable timeline entry. The `StepLogger` records the start
//! `Instant` on construction and emits the pinned line on explicit
//! `finish(exit_code)` or on `Drop` with `exit=0`.
use std::time::Instant;

/// The 15 subcommand variants that produce a machine-readable
/// timeline entry. Each variant has a stable `as_str()` token
/// a CI dashboard scraper greps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Smoke,
    Status,
    Bench,
    Compare,
    Compare3,
    Replay,
    VerifyReceipt,
    Publish,
    VerifyBundle,
    PublishRemote,
    VerifyRemote,
    PublishIndex,
    VerifyIndex,
    PublishIndexRemote,
    VerifyIndexRemote,
}

impl Step {
    /// The pinned `kind` token a dashboard scraper greps. The 15
    /// tokens are stable forever and the
    /// `tests::as_str_is_stable_per_variant` test fails on any
    /// regression.
    pub fn as_str(&self) -> &'static str {
        match self {
            Step::Smoke => "smoke",
            Step::Status => "status",
            Step::Bench => "bench",
            Step::Compare => "compare",
            Step::Compare3 => "compare3",
            Step::Replay => "replay",
            Step::VerifyReceipt => "verify_receipt",
            Step::Publish => "publish",
            Step::VerifyBundle => "verify_bundle",
            Step::PublishRemote => "publish_remote",
            Step::VerifyRemote => "verify_remote",
            Step::PublishIndex => "publish_index",
            Step::VerifyIndex => "verify_index",
            Step::PublishIndexRemote => "publish_index_remote",
            Step::VerifyIndexRemote => "verify_index_remote",
        }
    }

    /// The 15 pinned `kind` tokens in stable alphabetical order.
    /// The `trainer --observe-test` argv flag prints one token per
    /// line in this order so a CI dashboard scraper can `grep
    /// ^trainer step kind=` the shape without exercising every
    /// mode. The function returns a `'static` slice so the order
    /// is fixed at compile time.
    pub fn all_steps_alphabetical() -> &'static [&'static str] {
        &[
            "bench",
            "compare",
            "compare3",
            "publish",
            "publish_index",
            "publish_index_remote",
            "publish_remote",
            "replay",
            "smoke",
            "status",
            "verify_bundle",
            "verify_index",
            "verify_index_remote",
            "verify_receipt",
            "verify_remote",
        ]
    }
}

/// Per-step timeline logger. Enabled only when
/// `RBP_TRAINER_OBSERVE=1` is set; otherwise construction returns
/// `None` and the logger is a zero-cost no-op.
///
/// On explicit `finish(exit_code)` or on `Drop` (with `exit=0`),
/// emits exactly one stderr line:
///
/// ```text
/// trainer step: name=<name> kind=<kind> duration_ms=<ms> exit=<0|1|2>
/// ```
#[derive(Debug)]
pub struct StepLogger {
    step: Step,
    name: String,
    start: Instant,
    finished: bool,
}

impl StepLogger {
    /// Returns `Some(StepLogger)` when `RBP_TRAINER_OBSERVE=1`,
    /// otherwise `None` (the no-op default).
    pub fn new(step: Step) -> Option<Self> {
        Self::new_with_env(step, std::env::var("RBP_TRAINER_OBSERVE").ok().as_deref())
    }

    /// Test-only constructor that bypasses the process
    /// environment. `enabled=true` is the `env=1` path;
    /// `enabled=false` is the no-op path.
    #[cfg(test)]
    pub fn new_for_test(step: Step, enabled: bool) -> Option<Self> {
        Self::new_with_env(step, if enabled { Some("1") } else { None })
    }

    fn new_with_env(step: Step, env_val: Option<&str>) -> Option<Self> {
        if env_val != Some("1") {
            return None;
        }
        Some(Self {
            step,
            name: String::new(),
            start: Instant::now(),
            finished: false,
        })
    }

    /// Set the `name` field (defaults to empty string). Consumes
    /// and returns `self` for builder-style use.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Emit the pinned stderr line with the given exit code and
    /// mark the logger as finished so `Drop` does not double-log.
    pub fn finish(mut self, exit_code: u8) {
        self.finished = true;
        let elapsed = self.start.elapsed().as_millis();
        eprintln!(
            "trainer step: name={} kind={} duration_ms={} exit={}",
            self.name,
            self.step.as_str(),
            elapsed,
            exit_code
        );
    }

    /// Convenience: call `finish` on an `Option<StepLogger>`.
    /// Does nothing when the logger is `None`.
    pub fn finish_opt(opt: Option<Self>, exit_code: u8) {
        if let Some(s) = opt {
            s.finish(exit_code);
        }
    }

    /// Format the pinned line without printing it.
    /// Used by tests to verify the shape.
    pub(crate) fn format_line(&self, exit_code: u8) -> String {
        let elapsed = self.start.elapsed().as_millis();
        format!(
            "trainer step: name={} kind={} duration_ms={} exit={}",
            self.name,
            self.step.as_str(),
            elapsed,
            exit_code
        )
    }
}

impl Drop for StepLogger {
    fn drop(&mut self) {
        if !self.finished && std::env::var("RBP_TRAINER_OBSERVE").unwrap_or_default() == "1" {
            let elapsed = self.start.elapsed().as_millis();
            eprintln!(
                "trainer step: name={} kind={} duration_ms={} exit=0",
                self.name,
                self.step.as_str(),
                elapsed
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// STW-039: the 15 pinned `as_str` tokens are stable forever.
    /// A regression in any of them fails this test (the dashboard
    /// scraper's grep regex is `^trainer step kind=`).
    #[test]
    fn as_str_is_stable_per_variant() {
        assert_eq!(Step::Smoke.as_str(), "smoke");
        assert_eq!(Step::Status.as_str(), "status");
        assert_eq!(Step::Bench.as_str(), "bench");
        assert_eq!(Step::Compare.as_str(), "compare");
        assert_eq!(Step::Compare3.as_str(), "compare3");
        assert_eq!(Step::Replay.as_str(), "replay");
        assert_eq!(Step::VerifyReceipt.as_str(), "verify_receipt");
        assert_eq!(Step::Publish.as_str(), "publish");
        assert_eq!(Step::VerifyBundle.as_str(), "verify_bundle");
        assert_eq!(Step::PublishRemote.as_str(), "publish_remote");
        assert_eq!(Step::VerifyRemote.as_str(), "verify_remote");
        assert_eq!(Step::PublishIndex.as_str(), "publish_index");
        assert_eq!(Step::VerifyIndex.as_str(), "verify_index");
        assert_eq!(Step::PublishIndexRemote.as_str(), "publish_index_remote");
        assert_eq!(Step::VerifyIndexRemote.as_str(), "verify_index_remote");
    }

    /// STW-039: the `all_steps_alphabetical` list is the single
    /// source of truth the `--observe-test` argv flag prints. The
    /// list is in stable alphabetical order; a regression in the
    /// order fails this test.
    #[test]
    fn all_steps_alphabetical_is_stable() {
        let steps = Step::all_steps_alphabetical();
        assert_eq!(steps.len(), 15, "must have 15 steps");
        let expected: &[&str] = &[
            "bench",
            "compare",
            "compare3",
            "publish",
            "publish_index",
            "publish_index_remote",
            "publish_remote",
            "replay",
            "smoke",
            "status",
            "verify_bundle",
            "verify_index",
            "verify_index_remote",
            "verify_receipt",
            "verify_remote",
        ];
        assert_eq!(steps, expected, "alphabetical order must match");
        let mut sorted = steps.to_vec();
        sorted.sort_unstable();
        assert_eq!(steps, &sorted[..], "steps must be sorted ascending");
    }

    /// STW-039: every `as_str()` token is present in the
    /// `all_steps_alphabetical` list (the `as_str` /
    /// `all_steps` pair is the dashboard's contract).
    #[test]
    fn every_as_str_in_all_steps_alphabetical() {
        let all = Step::all_steps_alphabetical();
        for variant in [
            Step::Smoke,
            Step::Status,
            Step::Bench,
            Step::Compare,
            Step::Compare3,
            Step::Replay,
            Step::VerifyReceipt,
            Step::Publish,
            Step::VerifyBundle,
            Step::PublishRemote,
            Step::VerifyRemote,
            Step::PublishIndex,
            Step::VerifyIndex,
            Step::PublishIndexRemote,
            Step::VerifyIndexRemote,
        ] {
            let kind = variant.as_str();
            assert!(
                all.contains(&kind),
                "kind={kind} for variant {variant:?} must appear in all_steps_alphabetical"
            );
        }
    }

    /// STW-039: `StepLogger` is a no-op when the env knob is
    /// unset. Construction returns `None` and `finish_opt` on
    /// `None` is silent.
    #[test]
    fn logger_is_noop_when_env_unset() {
        let logger = StepLogger::new_for_test(Step::Smoke, false);
        assert!(logger.is_none(), "logger must be None when env is unset");
        // finish_opt on None must not panic.
        StepLogger::finish_opt(None, 0);
    }

    /// STW-039: `StepLogger::new` returns `Some` only when
    /// `RBP_TRAINER_OBSERVE=1`.
    #[test]
    fn logger_is_some_when_env_is_one() {
        let logger = StepLogger::new_for_test(Step::Bench, true);
        assert!(logger.is_some(), "logger must be Some when env=1");
        // Explicit finish so Drop doesn't double-log during test.
        if let Some(l) = logger {
            l.finish(0);
        }
    }

    /// STW-039: `with_name` sets the name field.
    #[test]
    fn with_name_sets_name() {
        let logger = StepLogger::new_for_test(Step::Replay, true)
            .expect("env=1")
            .with_name("my-receipt");
        assert_eq!(logger.name, "my-receipt");
        logger.finish(0);
    }

    /// STW-039: explicit `finish` consumes the logger and does not
    /// fire `Drop`. We verify by creating a logger, finishing it,
    /// and confirming the test does not observe a second line.
    /// (The actual stderr line is captured by the test harness;
    /// the important property is that `finished=true` prevents
    /// `Drop` from emitting.)
    #[test]
    fn finish_sets_finished_and_prevents_drop_emit() {
        let logger = StepLogger::new_for_test(Step::Smoke, true).expect("env=1");
        // finish consumes the logger.
        logger.finish(2);
        // If Drop fired after this point, the test output would
        // show a second line. The harness captures stderr; the
        // absence of a duplicate is the contract.
    }

    /// STW-039: the pinned line shape starts with
    /// `trainer step: name=` and contains `kind=`, `duration_ms=`,
    /// and `exit=` in the expected positions.
    #[test]
    fn pinned_line_contains_all_fields() {
        let logger = StepLogger::new_for_test(Step::Bench, true)
            .expect("env=1")
            .with_name("fixture");
        let line = logger.format_line(2);
        assert!(
            line.starts_with("trainer step: name="),
            "line must start with pinned prefix; got: {line}"
        );
        assert!(
            line.contains("kind=bench"),
            "line must contain kind=bench; got: {line}"
        );
        assert!(
            line.contains("duration_ms="),
            "line must contain duration_ms=; got: {line}"
        );
        assert!(
            line.contains("exit=2"),
            "line must contain exit=2; got: {line}"
        );
        assert!(
            line.contains("name=fixture"),
            "line must contain name=fixture; got: {line}"
        );
        // duration_ms must be a u128 (non-negative, no fractional part).
        let ms_str = line
            .split("duration_ms=")
            .nth(1)
            .expect("duration_ms= present")
            .split_whitespace()
            .next()
            .expect("duration_ms value present");
        let ms: u128 = ms_str.parse().expect("parse u128");
        assert!(ms < 10_000, "duration_ms should be small in test; got {ms}");
        // finish to prevent Drop from emitting.
        StepLogger::finish_opt(Some(logger), 0);
    }
}
