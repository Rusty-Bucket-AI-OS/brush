//! Running an external command on WASI: ask the host (`rb_proc.spawn`) to start the program with
//! the shell's environment and folder and the standard streams the redirections chose.
//!
//! The host gives a child only stream descriptors, so a standard stream that is a file (or any
//! other in-process stream) is connected through a pipe that this program pumps: input is written
//! before the child's output is read, then output (standard output, then standard error) is read to
//! its end. Pipes between commands of a pipeline are real host pipes, so those stages run together.

use std::io::{Read as _, Write as _};

use crate::commands::{CommandArg, ExecutionContext};
use crate::openfiles::{OpenFile, OpenFiles};
use crate::results::ExecutionSpawnResult;
use crate::{ErrorKind, error, extensions, processes};

use super::pipes::{PipeReader, PipeWriter, pipe};
use super::process::Child;
use super::rbproc::{self, NO_FD};

/// What the shell keeps of a standard stream it pumps.
enum Pump {
    /// The child reads what this file gives; the pipe end is ours to write.
    Input(OpenFile, PipeWriter),
    /// The child writes the other end; ours is read into this file.
    Output(OpenFile, PipeReader),
}

/// Which descriptor of ours the child gets as a standard stream, and what must be pumped.
fn connect(
    file: Option<OpenFile>,
    slot: u32,
    pumps: &mut Vec<Pump>,
    child_ends: &mut Vec<std::sync::Arc<std::fs::File>>,
) -> Result<u32, std::io::Error> {
    Ok(match file {
        None => slot,
        Some(OpenFile::Stdin(_)) => 0,
        Some(OpenFile::Stdout(_)) => 1,
        Some(OpenFile::Stderr(_)) => 2,
        Some(OpenFile::PipeReader(r)) => r.fd(),
        Some(OpenFile::PipeWriter(w)) => w.fd(),
        Some(other) => {
            let (reader, writer) = pipe()?;
            if slot == 0 {
                let fd = reader.fd();
                child_ends.push(reader.0);
                pumps.push(Pump::Input(other, writer));
                fd
            } else {
                let fd = writer.fd();
                child_ends.push(writer.0);
                pumps.push(Pump::Output(other, reader));
                fd
            }
        }
    })
}

/// Runs the pumps to their ends (input first, then output).
fn run_pumps(pumps: Vec<Pump>) {
    let (inputs, outputs): (Vec<_>, Vec<_>) = pumps
        .into_iter()
        .partition(|p| matches!(p, Pump::Input(..)));
    for pump in inputs.into_iter().chain(outputs) {
        match pump {
            Pump::Input(mut source, mut sink) => {
                // (A child that stops reading closes the pipe; that is not an error here.)
                let _ = std::io::copy(&mut source, &mut sink);
            }
            Pump::Output(mut sink, mut source) => {
                let mut buffer = [0u8; 8192];
                while let Ok(n) = source.read(&mut buffer) {
                    if n == 0 || sink.write_all(&buffer[..n]).is_err() {
                        break;
                    }
                }
                let _ = sink.flush();
            }
        }
    }
}

fn absolute(path: &str, cwd: &std::path::Path) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        cwd.join(path).to_string_lossy().into_owned()
    }
}

fn exported_env<SE: extensions::ShellExtensions>(
    context: &ExecutionContext<'_, SE>,
    program: &str,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = context
        .shell
        .env()
        .iter_exported()
        .filter(|(_, v)| v.value().is_set())
        .map(|(k, v)| (k.clone(), v.value().to_cow_str(context.shell).into_owned()))
        .filter(|(k, _)| !k.is_empty() && !k.contains('='))
        .collect();
    env.push(("_".to_owned(), program.to_owned()));
    env
}

/// Starts the command and returns a handle to wait on.
pub(crate) fn execute_external_command<SE: extensions::ShellExtensions>(
    context: ExecutionContext<'_, SE>,
    executable_path: &str,
    _process_group_id: Option<i32>,
    _argv0_override: Option<&str>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    let cwd = context.shell.working_dir().to_path_buf();
    // A program with a slash in its name is a file; a bare word is a command the host keeps.
    let program = if executable_path.contains('/') {
        absolute(executable_path, &cwd)
    } else {
        executable_path.to_owned()
    };
    let mut argv = vec![program.clone()];
    argv.extend(args.iter().map(ToString::to_string));
    let env = exported_env(&context, &program);

    let (mut pumps, mut child_ends) = (Vec::new(), Vec::new());
    let mut stdio = [NO_FD; 3];
    for (slot, fd) in [
        OpenFiles::STDIN_FD,
        OpenFiles::STDOUT_FD,
        OpenFiles::STDERR_FD,
    ]
    .into_iter()
    .enumerate()
    {
        stdio[slot] = connect(context.try_fd(fd), slot as u32, &mut pumps, &mut child_ends)
            .map_err(|e| ErrorKind::FailedToExecuteCommand(context.command_name.clone(), e))?;
    }

    // What this program has printed so far must come before what the child prints.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    let cwd_text = cwd.to_string_lossy().into_owned();
    let start = |argv: &[String]| {
        rbproc::spawn(&rbproc::Request {
            argv,
            env: &env,
            cwd: &cwd_text,
            stdio,
        })
    };
    let mut started = start(&argv);
    // A command word that is not a file is retried by its own name (the host's kept commands).
    if matches!(&started, Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        && program != context.command_name
        && !context.command_name.contains('/')
    {
        argv[0].clone_from(&context.command_name);
        started = start(&argv);
    }
    // The child has its ends now; ours must go, or the other side never sees the end of the data.
    drop(child_ends);

    let pid = match started {
        Ok(pid) => pid,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ErrorKind::CommandNotFound(context.command_name).into());
        }
        Err(e) => return Err(ErrorKind::FailedToExecuteCommand(context.command_name, e).into()),
    };
    run_pumps(pumps);

    #[expect(clippy::cast_possible_wrap)]
    let pid_i32 = pid as i32;
    Ok(ExecutionSpawnResult::StartedProcess(
        processes::ChildProcess::new(Child::new(pid), Some(pid_i32), None),
    ))
}
