//! Process management

#[cfg(not(target_family = "wasm"))]
use futures::FutureExt;

use crate::{error, sys};

/// A waitable future that will yield the results of a child process's execution.
pub(crate) type WaitableChildProcess = std::pin::Pin<
    Box<dyn futures::Future<Output = Result<sys::process::Output, std::io::Error>> + Send + Sync>,
>;

/// Tracks a child process being awaited.
pub struct ChildProcess {
    /// A waitable future that will yield the results of a child process's execution.
    exec_future: WaitableChildProcess,
    /// If available, the process ID of the child.
    pid: Option<sys::process::ProcessId>,
    /// If available, the process group ID of the child.
    pgid: Option<sys::process::ProcessId>,
    /// On WASI: the child to ask the host about, when asking is enough to learn how it ended.
    #[cfg(target_family = "wasm")]
    pollable_pid: Option<u32>,
}

impl ChildProcess {
    /// Wraps a child process and its future.
    pub fn new(
        child: sys::process::Child,
        pid: Option<sys::process::ProcessId>,
        pgid: Option<sys::process::ProcessId>,
    ) -> Self {
        #[cfg(target_family = "wasm")]
        let pollable_pid = child.id().filter(|_| child.is_pollable());
        Self {
            exec_future: Box::pin(child.wait_with_output()),
            pid,
            pgid,
            #[cfg(target_family = "wasm")]
            pollable_pid,
        }
    }

    /// Returns the process's ID.
    pub const fn pid(&self) -> Option<sys::process::ProcessId> {
        self.pid
    }

    /// On WASI: whether the host can be asked how the process ended without waiting for it.
    #[cfg(target_family = "wasm")]
    pub const fn is_pollable(&self) -> bool {
        self.pollable_pid.is_some()
    }

    /// Returns the process's group ID.
    pub const fn pgid(&self) -> Option<sys::process::ProcessId> {
        self.pgid
    }

    /// Waits for the process to exit.
    pub async fn wait(&mut self) -> Result<ProcessWaitResult, error::Error> {
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut sigtstp = sys::signal::tstp_signal_listener()?;
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut sigchld = sys::signal::chld_signal_listener()?;

        #[allow(clippy::ignored_unit_patterns)]
        loop {
            tokio::select! {
                output = &mut self.exec_future => {
                    break Ok(ProcessWaitResult::Completed(output?))
                },
                _ = sigtstp.recv() => {
                    break Ok(ProcessWaitResult::Stopped)
                },
                _ = sigchld.recv() => {
                    if sys::signal::poll_for_stopped_children()? {
                        break Ok(ProcessWaitResult::Stopped);
                    }
                },
                _ = sys::signal::await_ctrl_c() => {
                    // SIGINT got thrown. Handle it and continue looping. The child should
                    // have received it as well, and either handled it or ended up getting
                    // terminated (in which case we'll see the child exit).
                },
            }
        }
    }

    // On WASI, waiting for a child blocks this program, so a poll asks the host without waiting
    // (a child whose streams still wait to be carried is only done once it is waited for).
    #[cfg(target_family = "wasm")]
    pub(crate) fn poll(&mut self) -> Option<Result<sys::process::Output, error::Error>> {
        sys::process::poll_child(self.pollable_pid?).map(|r| r.map_err(Into::into))
    }

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn poll(&mut self) -> Option<Result<sys::process::Output, error::Error>> {
        let checkable_future = &mut self.exec_future;
        checkable_future
            .now_or_never()
            .map(|result| result.map_err(Into::into))
    }
}

/// Represents the result of waiting for an executing process.
pub enum ProcessWaitResult {
    /// The process completed.
    Completed(sys::process::Output),
    /// The process stopped and has not yet completed.
    Stopped,
}
