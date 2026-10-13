//! Child processes started through the host's `rb_proc` calls.

use super::rbproc;

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
}

impl Child {
    pub(crate) const fn new(pid: u32) -> Self {
        Self { pid }
    }

    /// The child's process ID (the host's).
    pub fn id(&self) -> Option<u32> {
        Some(self.pid)
    }

    /// Waits for the child to end (blocks this program; the child runs on its own).
    pub async fn wait_with_output(self) -> std::io::Result<Output> {
        let code = rbproc::wait(self.pid)?;
        Ok(Output {
            status: ExitStatus(code),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }
}
