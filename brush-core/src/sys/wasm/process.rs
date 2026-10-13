//! Child processes started through the host's `rb_proc` calls.

use super::rbproc;
use super::spawn::{Pump, run_pumps};

pub(crate) type ProcessId = i32;

/// How a child ended: its exit code (128 plus the signal for a child that was stopped by one).
#[derive(Clone, Copy, Debug)]
#[allow(unnameable_types)]
pub struct ExitStatus(i32);

impl ExitStatus {
    /// The exit code.
    pub const fn code(&self) -> Option<i32> {
        Some(self.0)
    }

    /// Whether the child succeeded.
    pub const fn success(&self) -> bool {
        self.0 == 0
    }
}

/// A finished child: its status. (Its output went to the descriptors it was given.)
#[allow(unnameable_types)]
pub struct Output {
    /// How it ended.
    pub status: ExitStatus,
    /// Always empty: the child writes to its own descriptors.
    pub stdout: Vec<u8>,
    /// Always empty.
    pub stderr: Vec<u8>,
}

/// A running child.
pub struct Child {
    pid: u32,
    /// Streams the shell still has to carry for the child (a background job whose output goes to
    /// a file or to memory): they are carried when the job is waited for.
    pumps: Vec<Pump>,
}

impl Child {
    pub(crate) const fn new(pid: u32) -> Self {
        Self {
            pid,
            pumps: Vec::new(),
        }
    }

    /// A child whose streams are carried later, by [`Child::wait_with_output`].
    pub(super) const fn with_pumps(pid: u32, pumps: Vec<Pump>) -> Self {
        Self { pid, pumps }
    }

    /// Whether asking the host without waiting is enough to learn how the child ended.
    pub(crate) const fn is_pollable(&self) -> bool {
        self.pumps.is_empty()
    }

    /// The child's process ID (the host's).
    pub fn id(&self) -> Option<u32> {
        Some(self.pid)
    }

    /// Waits for the child to end (blocks this program; the child runs on its own).
    pub async fn wait_with_output(self) -> std::io::Result<Output> {
        // (The streams first: a child that fills a pipe nobody reads would never end.)
        run_pumps(self.pumps);
        let code = rbproc::wait(self.pid)?;
        Ok(finished(code))
    }
}

fn finished(code: i32) -> Output {
    Output {
        status: ExitStatus(code),
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}

/// Asks about a child without waiting: how it ended, or `None` while it runs.
pub(crate) fn poll_child(pid: u32) -> Option<std::io::Result<Output>> {
    match rbproc::try_wait(pid) {
        Ok(Some(code)) => Some(Ok(finished(code))),
        Ok(None) => None,
        Err(e) => Some(Err(e)),
    }
}

/// Ends a child (the host's `kill`: its status is then 137).
pub fn kill_child(pid: u32) -> std::io::Result<()> {
    rbproc::kill(pid)
}
