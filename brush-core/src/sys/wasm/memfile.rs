//! In-process streams: shared files (no `dup` on WASI, so a clone is another handle to the same
//! file), the memory buffer a command substitution writes into, a memory source for here-documents,
//! and the null device.

use std::fs::File;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::openfiles::{OpenFile, Stream};

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A file shared by every clone (they share the position, as duplicated descriptors do).
#[derive(Clone)]
pub(crate) struct SharedFile(Arc<Mutex<File>>);

impl From<File> for SharedFile {
    fn from(file: File) -> Self {
        Self(Arc::new(Mutex::new(file)))
    }
}

impl Read for SharedFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        locked(&self.0).read(buf)
    }
}

impl Write for SharedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        locked(&self.0).write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        locked(&self.0).flush()
    }
}

impl Stream for SharedFile {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(self.clone())
    }
}

/// Where a command substitution's output goes: a buffer in memory, read once the command is done.
#[derive(Clone, Default)]
pub(crate) struct MemSink(Arc<Mutex<Vec<u8>>>);

impl MemSink {
    /// Takes what has been written so far.
    pub(crate) fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *locked(&self.0))
    }
}

impl Read for MemSink {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl Write for MemSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        locked(&self.0).extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Stream for MemSink {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(self.clone())
    }
}

/// A fixed text that reads once, then end of file (a here-document or here-string).
#[derive(Clone)]
pub(crate) struct MemSource(Arc<Mutex<(Vec<u8>, usize)>>);

impl Read for MemSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut guard = locked(&self.0);
        let (bytes, at) = &mut *guard;
        let n = buf.len().min(bytes.len() - *at);
        buf[..n].copy_from_slice(&bytes[*at..*at + n]);
        *at += n;
        Ok(n)
    }
}

impl Write for MemSource {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Stream for MemSource {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(self.clone())
    }
}

/// `/dev/null`: reads end at once and writes are dropped.
#[derive(Clone, Copy)]
pub(crate) struct NullFile;

impl Read for NullFile {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl Write for NullFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Stream for NullFile {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(*self)
    }
}

/// A memory sink and the open file that writes into it.
pub(crate) fn memory_sink() -> (OpenFile, MemSink) {
    let sink = MemSink::default();
    (OpenFile::Stream(Box::new(sink.clone())), sink)
}

/// An open file that reads the given text.
pub(crate) fn memory_source(text: &str) -> OpenFile {
    OpenFile::Stream(Box::new(MemSource(Arc::new(Mutex::new((
        text.as_bytes().to_vec(),
        0,
    ))))))
}

/// The null device as an open file.
pub(crate) fn null_file() -> OpenFile {
    OpenFile::Stream(Box::new(NullFile))
}

/// An opened file as an open file.
pub(crate) fn shared_file(file: File) -> OpenFile {
    OpenFile::Stream(Box::new(SharedFile::from(file)))
}
