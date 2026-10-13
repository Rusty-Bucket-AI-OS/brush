//! Pipes on the host's `rb_proc.pipe_new`. An end is a shared handle (cloning shares the
//! descriptor, since WASI has no `dup`); the descriptor closes when the last copy is dropped, which
//! is what lets the other end see end of file.

use std::fs::File;
use std::io::{Read, Write};
use std::sync::Arc;

use super::rbproc;

/// The read end of a pipe.
#[derive(Clone)]
#[allow(unnameable_types)]
pub struct PipeReader(pub(crate) Arc<File>);

/// The write end of a pipe.
#[derive(Clone)]
#[allow(unnameable_types)]
pub struct PipeWriter(pub(crate) Arc<File>);

impl PipeReader {
    /// Makes another handle to the same end.
    pub fn try_clone(&self) -> std::io::Result<Self> {
        Ok(self.clone())
    }

    /// The descriptor number, as the host knows it.
    pub(crate) fn fd(&self) -> u32 {
        rbproc::raw_fd(&self.0)
    }
}

impl PipeWriter {
    /// Makes another handle to the same end.
    pub fn try_clone(&self) -> std::io::Result<Self> {
        Ok(self.clone())
    }

    /// The descriptor number, as the host knows it.
    pub(crate) fn fd(&self) -> u32 {
        rbproc::raw_fd(&self.0)
    }
}

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        (&*self.0).read(buf)
    }
}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Makes a pipe.
pub(crate) fn pipe() -> std::io::Result<(PipeReader, PipeWriter)> {
    let (reader, writer) = rbproc::pipe_new()?;
    Ok((PipeReader(Arc::new(reader)), PipeWriter(Arc::new(writer))))
}
