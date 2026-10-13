//! Filesystem utilities for WASM.

pub use crate::sys::stubs::fs::*;

/// The folders commands are looked for in when `PATH` is not set.
pub(crate) fn get_default_executable_search_paths() -> Vec<std::path::PathBuf> {
    vec!["/usr/bin".into(), "/bin".into()]
}

/// Whether `path` names a command the host keeps (the core utilities are programs of the host, not
/// files): a name in `/usr/bin` or `/bin` that is in the list of the variable `RB_COMMANDS`, which
/// the host starts the shell with (space-separated).
fn is_host_command(path: &std::path::Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return false;
    };
    (dir == std::path::Path::new("/usr/bin") || dir == std::path::Path::new("/bin"))
        && std::env::var("RB_COMMANDS").is_ok_and(|list| list.split_whitespace().any(|c| c == name))
}

impl crate::sys::fs::PathExt for std::path::Path {
    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        true
    }

    fn executable(&self) -> bool {
        self.is_file() || is_host_command(self)
    }

    fn exists_and_is_block_device(&self) -> bool {
        false
    }

    fn exists_and_is_char_device(&self) -> bool {
        false
    }

    fn exists_and_is_fifo(&self) -> bool {
        false
    }

    fn exists_and_is_socket(&self) -> bool {
        false
    }

    fn exists_and_is_setgid(&self) -> bool {
        false
    }

    fn exists_and_is_setuid(&self) -> bool {
        false
    }

    fn exists_and_is_sticky_bit(&self) -> bool {
        false
    }

    fn get_device_and_inode(&self) -> Result<(u64, u64), crate::error::Error> {
        Ok((0, 0))
    }
}

/// Splits a PATH-like value into individual paths.
///
/// On WASM, `std::env::split_paths` is not available, so this
/// implementation splits by the `:` separator.
pub fn split_paths<T: AsRef<std::ffi::OsStr> + ?Sized>(
    s: &T,
) -> impl Iterator<Item = std::path::PathBuf> {
    s.as_ref()
        .to_str()
        .unwrap_or_default()
        .split(':')
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>()
        .into_iter()
}
