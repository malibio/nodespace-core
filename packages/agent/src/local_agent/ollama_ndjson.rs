//! Bounded NDJSON line buffering for Ollama HTTP streams.
//!
//! Ollama streams responses as newline-delimited JSON. A stream chunk may split
//! a line at an arbitrary byte boundary — including mid-way through a multi-byte
//! UTF-8 sequence — so bytes must be accumulated raw and decoded only once a
//! complete line is in hand. The buffer is capped so a malfunctioning or hostile
//! local daemon emitting an unterminated line cannot grow memory without bound.

/// Maximum bytes buffered between newlines before the stream is rejected.
///
/// A single Ollama NDJSON line is a small JSON object (chat delta or pull
/// progress); 1 MiB is orders of magnitude past any legitimate line.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Reason a [`NdjsonLineBuffer`] rejected input.
#[derive(Debug, PartialEq, Eq)]
pub enum NdjsonError {
    /// The inter-newline buffer exceeded [`MAX_LINE_BYTES`].
    LineTooLong { limit: usize },
    /// A complete line was not valid UTF-8.
    InvalidUtf8,
}

impl std::fmt::Display for NdjsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NdjsonError::LineTooLong { limit } => write!(
                f,
                "Ollama stream line exceeded {limit} bytes without a newline"
            ),
            NdjsonError::InvalidUtf8 => {
                write!(f, "Ollama stream line was not valid UTF-8")
            }
        }
    }
}

/// Accumulates raw stream bytes and yields complete, UTF-8-decoded lines.
///
/// Bytes are buffered until a `\n` is seen; each complete line is decoded with a
/// strict UTF-8 check (safe because the line is complete, unlike a per-chunk
/// lossy decode that can straddle a multi-byte boundary). The buffer is bounded
/// by [`MAX_LINE_BYTES`]: once that many bytes accumulate without a newline,
/// [`push`](Self::push) returns [`NdjsonError::LineTooLong`].
#[derive(Default)]
pub struct NdjsonLineBuffer {
    buffer: Vec<u8>,
}

impl NdjsonLineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a stream chunk and drain every newly-completed line.
    ///
    /// Empty and whitespace-only lines are skipped. Returns the trimmed lines in
    /// order, or an error if the buffer overflowed or a completed line was not
    /// valid UTF-8.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, NdjsonError> {
        self.buffer.extend_from_slice(chunk);

        let mut lines = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buffer.drain(..=pos).collect();
            // Exclude the trailing newline before decoding.
            let line = std::str::from_utf8(&line_bytes[..pos])
                .map_err(|_| NdjsonError::InvalidUtf8)?
                .trim();
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }

        // A newline resets the count, so only an unterminated tail can overflow.
        if self.buffer.len() > MAX_LINE_BYTES {
            return Err(NdjsonError::LineTooLong {
                limit: MAX_LINE_BYTES,
            });
        }

        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_multiple_lines_in_one_chunk() {
        let mut buf = NdjsonLineBuffer::new();
        let lines = buf.push(b"{\"a\":1}\n{\"b\":2}\n").unwrap();
        assert_eq!(lines, vec!["{\"a\":1}", "{\"b\":2}"]);
    }

    #[test]
    fn buffers_partial_line_across_chunks() {
        let mut buf = NdjsonLineBuffer::new();
        assert!(buf.push(b"{\"a\":").unwrap().is_empty());
        let lines = buf.push(b"1}\n").unwrap();
        assert_eq!(lines, vec!["{\"a\":1}"]);
    }

    #[test]
    fn decodes_multibyte_utf8_split_across_chunks() {
        // "é" is 0xC3 0xA9 in UTF-8; split it across two chunks.
        let mut buf = NdjsonLineBuffer::new();
        assert!(buf.push(&[b'"', 0xC3]).unwrap().is_empty());
        let lines = buf.push(&[0xA9, b'"', b'\n']).unwrap();
        assert_eq!(lines, vec!["\"é\""]);
    }

    #[test]
    fn skips_empty_and_whitespace_lines() {
        let mut buf = NdjsonLineBuffer::new();
        let lines = buf.push(b"\n   \n{\"a\":1}\n").unwrap();
        assert_eq!(lines, vec!["{\"a\":1}"]);
    }

    #[test]
    fn errors_when_unterminated_line_exceeds_cap() {
        let mut buf = NdjsonLineBuffer::new();
        let huge = vec![b'x'; MAX_LINE_BYTES + 1];
        assert_eq!(
            buf.push(&huge),
            Err(NdjsonError::LineTooLong {
                limit: MAX_LINE_BYTES
            })
        );
    }

    #[test]
    fn newline_resets_overflow_count() {
        // A large but newline-terminated stream must not trip the cap: the
        // buffer never holds more than one incomplete line's worth at a time.
        let mut buf = NdjsonLineBuffer::new();
        let line = format!("{}\n", "x".repeat(MAX_LINE_BYTES - 1));
        for _ in 0..4 {
            buf.push(line.as_bytes()).unwrap();
        }
        // Buffer is empty after each terminated line; another chunk still works.
        assert_eq!(buf.push(b"ok\n").unwrap(), vec!["ok"]);
    }
}
