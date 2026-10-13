//! Reading a pipe to its end. There is one thread, so the read just blocks until the writer is
//! done.

use std::io::{self, Read};

use super::pipes::PipeReader;

pub(crate) struct AsyncPipeReader {
    inner: Option<PipeReader>,
}

impl AsyncPipeReader {
    pub(crate) fn new(reader: PipeReader) -> io::Result<Self> {
        Ok(Self {
            inner: Some(reader),
        })
    }

    pub(crate) async fn read_to_string(&mut self) -> io::Result<String> {
        let Some(mut reader) = self.inner.take() else {
            return Ok(String::new());
        };
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
