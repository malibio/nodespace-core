//! Opt-in output capture for PTY agent sessions.
//!
//! [`SessionCapture`] maintains a circular byte buffer of PTY output chunks.
//! On session end, [`SessionCapture::transcript`] and [`SessionCapture::summary`]
//! assemble the content for the ai-chat node.

use std::collections::VecDeque;

use crate::pty::session::OutputChunk;

/// Maximum total bytes kept in the ring buffer (1 MiB). Older chunks are
/// evicted when the buffer is full.
pub const MAX_BUFFER_BYTES: usize = 1024 * 1024;

/// Summary length cap (first N bytes of the transcript).
pub const SUMMARY_MAX_BYTES: usize = 500;

/// Accumulates PTY output chunks in a bounded circular buffer.
///
/// The buffer evicts the *oldest* chunks when `max_bytes` is exceeded, so
/// the most recent output is always retained.
#[derive(Clone)]
pub struct SessionCapture {
    buffer: VecDeque<OutputChunk>,
    max_bytes: usize,
    current_bytes: usize,
}

impl SessionCapture {
    pub fn new() -> Self {
        Self::with_max_bytes(MAX_BUFFER_BYTES)
    }

    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            max_bytes,
            current_bytes: 0,
        }
    }

    /// Append a chunk, evicting the oldest chunks if the buffer would exceed
    /// `max_bytes`.
    pub fn push(&mut self, chunk: OutputChunk) {
        let chunk_len = chunk.data.len();

        // If a single chunk is larger than the entire buffer, just store it
        // alone (truncated to max_bytes).
        if chunk_len >= self.max_bytes {
            self.buffer.clear();
            self.current_bytes = 0;
            let truncated = OutputChunk {
                data: chunk.data[..self.max_bytes].to_vec(),
                timestamp: chunk.timestamp,
            };
            self.current_bytes = truncated.data.len();
            self.buffer.push_back(truncated);
            return;
        }

        // Evict oldest chunks until there is room.
        while self.current_bytes + chunk_len > self.max_bytes {
            if let Some(oldest) = self.buffer.pop_front() {
                self.current_bytes -= oldest.data.len();
            } else {
                break;
            }
        }

        self.current_bytes += chunk_len;
        self.buffer.push_back(chunk);
    }

    /// Concatenate all buffered chunks into a single UTF-8 string.
    /// Non-UTF-8 bytes are replaced with the Unicode replacement character.
    pub fn transcript(&self) -> String {
        let mut bytes = Vec::with_capacity(self.current_bytes);
        for chunk in &self.buffer {
            bytes.extend_from_slice(&chunk.data);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Return the first [`SUMMARY_MAX_BYTES`] bytes of the transcript as a
    /// UTF-8 string (truncated at a character boundary).
    pub fn summary(&self) -> String {
        let t = self.transcript();
        if t.len() <= SUMMARY_MAX_BYTES {
            return t;
        }
        // Truncate at a valid char boundary.
        let mut end = SUMMARY_MAX_BYTES;
        while !t.is_char_boundary(end) {
            end -= 1;
        }
        t[..end].to_string()
    }

    /// Total bytes currently in the buffer.
    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    /// Number of chunks currently buffered.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for SessionCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn chunk(data: &[u8]) -> OutputChunk {
        OutputChunk {
            data: data.to_vec(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn push_and_transcript_basic() {
        let mut cap = SessionCapture::new();
        cap.push(chunk(b"hello "));
        cap.push(chunk(b"world"));
        assert_eq!(cap.transcript(), "hello world");
        assert_eq!(cap.current_bytes(), 11);
        assert_eq!(cap.len(), 2);
    }

    #[test]
    fn evicts_oldest_on_overflow() {
        let mut cap = SessionCapture::with_max_bytes(10);
        cap.push(chunk(b"aaaaa")); // 5 bytes
        cap.push(chunk(b"bbbbb")); // 5 bytes — total 10, exactly full
        assert_eq!(cap.current_bytes(), 10);

        cap.push(chunk(b"ccccc")); // 5 bytes — should evict "aaaaa"
        assert_eq!(cap.current_bytes(), 10);
        assert_eq!(cap.transcript(), "bbbbbccccc");
    }

    #[test]
    fn oversized_chunk_replaces_entire_buffer() {
        let mut cap = SessionCapture::with_max_bytes(5);
        cap.push(chunk(b"xx"));
        cap.push(chunk(b"yyyyyy")); // 6 bytes > max_bytes=5, truncated to 5
        assert_eq!(cap.current_bytes(), 5);
        assert_eq!(cap.transcript(), "yyyyy");
    }

    #[test]
    fn summary_truncates_at_char_boundary() {
        let mut cap = SessionCapture::new();
        let long = "a".repeat(600);
        cap.push(chunk(long.as_bytes()));
        let s = cap.summary();
        assert_eq!(s.len(), SUMMARY_MAX_BYTES);
    }

    #[test]
    fn summary_short_returns_full_transcript() {
        let mut cap = SessionCapture::new();
        cap.push(chunk(b"short"));
        assert_eq!(cap.summary(), "short");
    }

    #[test]
    fn empty_capture_returns_empty_strings() {
        let cap = SessionCapture::new();
        assert_eq!(cap.transcript(), "");
        assert_eq!(cap.summary(), "");
        assert!(cap.is_empty());
    }

    #[test]
    fn multiple_evictions_maintain_correct_byte_count() {
        let mut cap = SessionCapture::with_max_bytes(15);
        for i in 0..10 {
            cap.push(chunk(format!("{:05}", i).as_bytes())); // 5 bytes each
        }
        // Buffer should hold exactly the last 3 chunks (15 bytes).
        assert_eq!(cap.current_bytes(), 15);
        assert_eq!(cap.len(), 3);
    }
}
