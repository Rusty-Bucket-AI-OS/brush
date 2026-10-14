//! Platform abstraction facilities

#![allow(unused)]

#[cfg(unix)]
pub(crate) mod unix;
#[cfg(unix)]
pub(crate) use unix as platform;

#[cfg(windows)]
pub(crate) mod windows;
#[cfg(windows)]
pub(crate) use windows as platform;

#[cfg(target_family = "wasm")]
pub(crate) mod wasm;
#[cfg(target_family = "wasm")]
pub(crate) use wasm as platform;
#[cfg(target_family = "wasm")]
pub(crate) use wasm::{background, memfile, spawn};

#[cfg(not(unix))]
pub(crate) mod stubs;

#[cfg(any(unix, windows))]
pub(crate) mod hostname;
#[cfg(any(unix, windows))]
pub mod tokio_process;

pub mod fs;

#[cfg(not(target_family = "wasm"))]
pub(crate) mod pipes {
    //! Pipes (the standard library's, where it has them).
    pub use std::io::{PipeReader, PipeWriter, pipe};
}
#[cfg(target_family = "wasm")]
pub(crate) use wasm::pipes;

pub use platform::async_pipe;
pub use platform::commands;
pub(crate) use platform::env;
pub use platform::fd;
pub use platform::input;
pub(crate) use platform::network;
pub use platform::poll;
pub use platform::process;
pub use platform::resource;
pub use platform::signal;
pub use platform::terminal;
pub(crate) use platform::users;

pub use platform::PlatformError;

/// The shell's own process ID. WASI has none: a number from the clock stands in for it, so
/// that names built from `$$` differ between runs. A background job is a new shell that reports
/// the number of the shell that started it (`$$` is the script's in a bash subshell too).
pub(crate) fn current_pid() -> u32 {
    #[cfg(target_family = "wasm")]
    {
        static PID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *PID.get_or_init(|| {
            if let Some(inherited) = std::env::var(platform::env::PARENT_PID_VAR)
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
            {
                return inherited;
            }
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.subsec_nanos());
            2 + nanos % 30_000
        })
    }
    #[cfg(not(target_family = "wasm"))]
    std::process::id()
}
