//! The host's `rb_proc` calls: pipes and child processes for a program that has neither `fork`
//! nor `exec`. The host (a runtime that provides the module `rb_proc`) owns the descriptors; the
//! calls below put them in this program's descriptor table.

use std::fs::File;
use std::os::fd::{AsRawFd as _, FromRawFd as _};

#[link(wasm_import_module = "rb_proc")]
unsafe extern "C" {
    #[link_name = "pipe_new"]
    fn host_pipe_new(read_fd: *mut u32, write_fd: *mut u32) -> u32;
    #[link_name = "spawn"]
    fn host_spawn(request: *const u8, len: u32, pid: *mut u32) -> u32;
    #[link_name = "wait"]
    fn host_wait(pid: u32, flags: u32, out: *mut u32) -> u32;
    #[link_name = "kill"]
    fn host_kill(pid: u32) -> u32;
}

/// The "no descriptor" value of a child's standard stream slot: the child gets a closed stream.
pub(crate) const NO_FD: u32 = u32::MAX;
/// Request flag: a write to a pipe nobody reads ends the child (like SIGPIPE) instead of failing.
const FLAG_SIGPIPE_TERMINATES: u32 = 1;
/// The WASI errno for "no such file or directory".
pub(crate) const ENOENT: u32 = 44;
/// The WASI errno for "try again": a child that is still running, asked without waiting.
const EAGAIN: u32 = 6;

/// The most arguments or variables in one request, and the longest single string (the host's
/// limits).
const MAX_ITEMS: usize = 1024;
const MAX_STRING: usize = 32 * 1024;

fn errno_error(errno: u32) -> std::io::Error {
    match errno {
        ENOENT => std::io::Error::from(std::io::ErrorKind::NotFound),
        _ => std::io::Error::other(format!("rb_proc call failed (errno {errno})")),
    }
}

/// Makes a pipe; both ends are owned files that close the descriptor when dropped.
pub(crate) fn pipe_new() -> std::io::Result<(File, File)> {
    let (mut read_fd, mut write_fd) = (0u32, 0u32);
    // SAFETY: both pointers are to live local `u32`s, which is all the host writes through them.
    let errno = unsafe { host_pipe_new(&raw mut read_fd, &raw mut write_fd) };
    if errno != 0 {
        return Err(errno_error(errno));
    }
    // SAFETY: the host just created these two descriptors for this program and nothing else owns
    // them.
    Ok(unsafe {
        (
            File::from_raw_fd(read_fd as i32),
            File::from_raw_fd(write_fd as i32),
        )
    })
}

/// What to start.
pub(crate) struct Request<'a> {
    /// The program (a command the host keeps, or an absolute path) first, then its arguments.
    pub argv: &'a [String],
    /// The environment, as `(name, value)`.
    pub env: &'a [(String, String)],
    /// The working folder as the child sees it (absolute).
    pub cwd: &'a str,
    /// The descriptors of this program the child reads and writes as 0, 1 and 2 (or [`NO_FD`]).
    pub stdio: [u32; 3],
}

fn put_text(out: &mut Vec<u8>, text: &str) -> std::io::Result<()> {
    if text.len() > MAX_STRING || text.contains('\0') {
        return Err(std::io::Error::other("an argument or variable is too long"));
    }
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    Ok(())
}

/// Encodes a request (version 1 of the host's wire format: little-endian words and
/// length-prefixed UTF-8).
fn encode(request: &Request<'_>) -> std::io::Result<Vec<u8>> {
    if request.argv.len() > MAX_ITEMS || request.env.len() > MAX_ITEMS {
        return Err(std::io::Error::other("too many arguments or variables"));
    }
    let mut out = Vec::new();
    for word in [
        1u32,
        // The flag: a write to a pipe nobody reads ends the child, as SIGPIPE does (`yes | head`).
        FLAG_SIGPIPE_TERMINATES,
        request.stdio[0],
        request.stdio[1],
        request.stdio[2],
        request.argv.len() as u32,
    ] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    for arg in request.argv {
        put_text(&mut out, arg)?;
    }
    out.extend_from_slice(&(request.env.len() as u32).to_le_bytes());
    for (name, value) in request.env {
        put_text(&mut out, &format!("{name}={value}"))?;
    }
    put_text(&mut out, request.cwd)?;
    Ok(out)
}

/// Starts a child; returns its pid.
pub(crate) fn spawn(request: &Request<'_>) -> std::io::Result<u32> {
    let bytes = encode(request)?;
    let mut pid = 0u32;
    // SAFETY: `bytes` is a live buffer of the length given, and `pid` a live `u32` the host writes.
    let errno = unsafe { host_spawn(bytes.as_ptr(), bytes.len() as u32, &raw mut pid) };
    if errno != 0 {
        return Err(errno_error(errno));
    }
    Ok(pid)
}

/// Waits for the child and returns its exit code (the low eight bits; 128 plus the signal).
pub(crate) fn wait(pid: u32) -> std::io::Result<i32> {
    let mut out = [0u32; 2];
    // SAFETY: `out` is a live array of the two `u32`s the host writes (the status, then the pid).
    let errno = unsafe { host_wait(pid, 0, out.as_mut_ptr()) };
    if errno != 0 {
        return Err(errno_error(errno));
    }
    Ok(out[0] as i32)
}

/// Asks about the child without waiting: its exit code if it has ended, `None` while it runs.
pub(crate) fn try_wait(pid: u32) -> std::io::Result<Option<i32>> {
    let mut out = [0u32; 2];
    // SAFETY: as for `wait`; flag 1 is the host's `WNOHANG`.
    let errno = unsafe { host_wait(pid, 1, out.as_mut_ptr()) };
    match errno {
        0 => Ok(Some(out[0] as i32)),
        EAGAIN => Ok(None),
        _ => Err(errno_error(errno)),
    }
}

/// Ends a child.
pub(crate) fn kill(pid: u32) -> std::io::Result<()> {
    // SAFETY: the call takes a number and touches no memory of this program.
    match unsafe { host_kill(pid) } {
        0 => Ok(()),
        errno => Err(errno_error(errno)),
    }
}

/// The descriptor number of an owned stream file.
pub(crate) fn raw_fd(file: &File) -> u32 {
    file.as_raw_fd() as u32
}
