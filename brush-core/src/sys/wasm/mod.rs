//! WebAssembly (WASI) platform support. A host that provides the module `rb_proc` (pipes and
//! child processes) is required; see `rbproc`.

pub mod async_pipe;
pub use crate::sys::stubs::commands;
pub(crate) mod env;
pub use crate::sys::stubs::fd;
pub(crate) mod fs;
pub use crate::sys::stubs::input;
pub(crate) use crate::sys::stubs::network;
pub(crate) mod memfile;
pub(crate) mod pipes;
pub use crate::sys::stubs::poll;
pub mod process;
pub(crate) mod rbproc;
pub use crate::sys::stubs::resource;
pub use crate::sys::stubs::signal;
pub(crate) mod spawn;
pub use crate::sys::stubs::terminal;
pub(crate) use crate::sys::stubs::users;

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {}
