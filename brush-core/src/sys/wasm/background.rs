//! A background job (`cmd &`) on WASI: there is one thread and no `fork`, so the job is a child
//! shell process started through the host (`rb_proc.spawn`), as a subshell would be in bash. It is
//! given the command's text, the functions and variables the command mentions, the exported
//! environment, the working folder, and the shell's standard output and error. Nothing the job
//! sets comes back, which is also true of a subshell in bash. The job is tracked like any other
//! child, so `jobs`, `wait`, `$!` and `kill` work on it.

use std::fmt::Write as _;
use std::io::Write as _;

use crate::interp::ExecutionParameters;
use crate::openfiles::OpenFiles;
use crate::{ErrorKind, Shell, error, extensions, processes};

use super::background_vars::declarations;
use super::env::PARENT_PID_VAR;
use super::process::Child;
use super::rbproc::{self, NO_FD};
use super::spawn::{Pump, connect};

/// The most background jobs that run at once; a further one waits for a slot.
pub const MAX_RUNNING_JOBS: usize = 8;

/// Blocks until a child of this shell ends (the host wakes the shell then).
pub(crate) fn wait_for_any_child() -> std::io::Result<()> {
    rbproc::wait_any()
}

/// The longest command text (with what it needs) one job is given: the host limits one argument.
const MAX_TEXT: usize = 30_000;

/// The program that runs a job: this shell, started under the name this process was started by
/// (a name the host keeps).
fn shell_program() -> String {
    std::env::args()
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "brush".to_owned())
}

/// The text a job starts with: the functions and variables `text` mentions (and the functions
/// those mention), then `text`.
fn program_text<SE: extensions::ShellExtensions>(
    shell: &Shell<SE>,
    text: &str,
) -> Result<String, error::Error> {
    let mut included: Vec<(String, String)> = Vec::new();
    let mut scope = text.to_owned();
    loop {
        let before = included.len();
        for (name, registration) in shell.funcs().iter() {
            if !included.iter().any(|(n, _)| n == name) && scope.contains(name.as_str()) {
                let definition = registration.definition().to_string();
                scope.push('\n');
                scope.push_str(&definition);
                included.push((name.clone(), definition));
            }
        }
        if included.len() == before {
            break;
        }
    }
    let mut out = String::new();
    // A new shell starts in the root folder (WASI has no working folder to inherit).
    let cwd = shell.working_dir().to_string_lossy().into_owned();
    if cwd != "/" {
        let quoted = crate::escape::force_quote(&cwd, crate::escape::QuoteMode::SingleQuote);
        let _ = writeln!(out, "cd -- {quoted} || exit 1");
    }
    for (_, definition) in &included {
        out.push_str(definition);
        out.push('\n');
    }
    out.push_str(&declarations(shell, &scope)?);
    out.push_str(text);
    Ok(out)
}

/// Starts `text` as a background job and returns the child to track. `shell` is the shell the job
/// is started from; `params` carries its standard streams.
pub(crate) fn spawn_background<SE: extensions::ShellExtensions>(
    shell: &Shell<SE>,
    params: &ExecutionParameters,
    text: &str,
) -> Result<processes::ChildProcess, error::Error> {
    let program = program_text(shell, text)?;
    if program.len() > MAX_TEXT {
        return Err(error::ErrorKind::Unimplemented(
            "a background job whose command, functions, variables and read-only variables are over 30,000 bytes (it was not started)",
        )
        .into());
    }
    let failed = |e: std::io::Error| ErrorKind::FailedToExecuteCommand(text.to_owned(), e);

    let program_name = shell_program();
    let mut argv: Vec<String> = [program_name.as_str(), "--norc", "--noprofile", "-c"]
        .iter()
        .map(ToString::to_string)
        .collect();
    argv.push(program);
    argv.push(
        shell
            .current_shell_name()
            .map_or_else(|| program_name.clone(), |n| n.into_owned()),
    );
    argv.extend(shell.current_shell_args().iter().cloned());

    let cwd = shell.working_dir().to_string_lossy().into_owned();
    let env = {
        let mut env: Vec<(String, String)> = shell
            .env()
            .iter_exported()
            .filter(|(k, v)| v.value().is_set() && !k.is_empty() && !k.contains('='))
            .map(|(k, v)| (k.clone(), v.value().to_cow_str(shell).into_owned()))
            .collect();
        env.push(("_".to_owned(), program_name.clone()));
        // `$$` in the job is the script's, as in a bash subshell.
        env.retain(|(k, _)| k != PARENT_PID_VAR);
        env.push((
            PARENT_PID_VAR.to_owned(),
            crate::sys::current_pid().to_string(),
        ));
        env
    };

    // Standard input is empty (as for any background job); output and error go where the shell's
    // do.
    let (stdin_reader, stdin_writer) = rbproc::pipe_new().map_err(failed)?;
    drop(stdin_writer);
    let (mut pumps, mut child_ends): (Vec<Pump>, Vec<std::sync::Arc<std::fs::File>>) =
        (Vec::new(), Vec::new());
    let mut stdio = [rbproc::raw_fd(&stdin_reader), NO_FD, NO_FD];
    for (slot, fd) in [(1usize, OpenFiles::STDOUT_FD), (2, OpenFiles::STDERR_FD)] {
        stdio[slot] = connect(
            params.try_fd(shell, fd),
            slot as u32,
            &mut pumps,
            &mut child_ends,
        )
        .map_err(failed)?;
    }

    // What this shell has printed so far comes before what the job prints.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    let started = rbproc::spawn(&rbproc::Request {
        argv: &argv,
        env: &env,
        cwd: &cwd,
        stdio,
    });
    // The job has its ends now; ours must go, or the other side never sees the end of the data.
    drop(child_ends);
    drop(stdin_reader);
    let pid = started.map_err(failed)?;

    #[expect(clippy::cast_possible_wrap)]
    let pid_i32 = pid as i32;
    Ok(processes::ChildProcess::new(
        Child::with_pumps(pid, pumps),
        Some(pid_i32),
        None,
    ))
}
