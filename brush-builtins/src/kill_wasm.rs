use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Signal a job or process. On WASI there are no signals: `HUP`, `INT`, `QUIT`, `KILL` and `TERM`
/// all end a background job, which then reports `128` plus that signal's number, and `-0` only
/// asks whether the job is still running.
#[derive(Parser)]
pub(crate) struct KillCommand {
    /// Name of the signal to send.
    #[arg(short = 's', value_name = "SIG_NAME")]
    signal_name: Option<String>,

    /// Number of the signal to send.
    #[arg(short = 'n', value_name = "SIG_NUM")]
    signal_number: Option<usize>,

    /// List known signal names.
    #[arg(short = 'l', short_alias = 'L')]
    list_signals: bool,

    // Interpretation of these depends on whether -l is present.
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The signals that end a job, by number.
const SIGNALS: [(&str, u8); 5] = [
    ("HUP", 1),
    ("INT", 2),
    ("QUIT", 3),
    ("KILL", 9),
    ("TERM", 15),
];

/// The number of a signal given by name (with or without `SIG`) or by number.
fn signal_number(spec: &str) -> Option<u8> {
    if let Ok(n) = spec.parse::<u8>() {
        return (n == 0 || SIGNALS.iter().any(|(_, s)| *s == n)).then_some(n);
    }
    let name = spec.strip_prefix("SIG").unwrap_or(spec);
    SIGNALS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

impl builtins::Command for KillCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.list_signals {
            for (name, _) in SIGNALS {
                writeln!(context.stdout(), "{name}")?;
            }
            return Ok(ExecutionResult::success());
        }

        // Default signal is TERM.
        let mut signal = 15;
        let mut targets = Vec::new();
        let given = self
            .signal_name
            .clone()
            .or_else(|| self.signal_number.map(|n| n.to_string()));
        if let Some(spec) = given {
            let Some(n) = signal_number(&spec) else {
                writeln!(
                    context.stderr(),
                    "{}: {spec}: invalid signal specification (only HUP, INT, QUIT, KILL and TERM exist here)",
                    context.command_name
                )?;
                return Ok(ExecutionExitCode::InvalidUsage.into());
            };
            signal = n;
        }
        for arg in &self.args {
            if let Some(spec) = arg.strip_prefix('-') {
                let Some(n) = signal_number(spec) else {
                    writeln!(
                        context.stderr(),
                        "{}: {spec}: invalid signal specification (only HUP, INT, QUIT, KILL and TERM exist here)",
                        context.command_name
                    )?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                };
                signal = n;
            } else {
                targets.push(arg.as_str());
            }
        }
        if targets.is_empty() {
            writeln!(context.stderr(), "{}: invalid usage", context.command_name)?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        let mut result = ExecutionResult::success();
        for target in targets {
            let ended = if target.starts_with('%') {
                context
                    .shell
                    .jobs_mut()
                    .resolve_job_spec(target)
                    .map(|job| (job.id, job))
            } else {
                let pid = target.parse::<i32>().ok();
                context
                    .shell
                    .jobs_mut()
                    .jobs
                    .iter_mut()
                    .find(|j| pid.is_some() && j.representative_pid() == pid)
                    .map(|job| (job.id, job))
            };
            let outcome = match ended {
                None => Err("no such job or process"),
                Some((_, job)) if signal == 0 => job
                    .poll_done()
                    .ok()
                    .filter(|done| done.is_none())
                    .map(|_| ())
                    .ok_or("no such process"),
                Some((_, job)) => job.terminate(signal).map_err(|_| "no such process"),
            };
            if let Err(why) = outcome {
                writeln!(
                    context.stderr(),
                    "{}: {target}: {why}",
                    context.command_name
                )?;
                result = ExecutionResult::general_error();
            }
        }
        Ok(result)
    }
}
