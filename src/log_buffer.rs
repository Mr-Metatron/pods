use std::collections::VecDeque;

use bytes::Bytes;

pub(crate) const MAX_LOG_LINES: usize = 10_000;
pub(crate) const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_LOG_CHUNK_BYTES: usize = 64 * 1024;

/// Keeps the most recent history without retaining an arbitrarily large daemon response.
#[derive(Debug, Default)]
pub(crate) struct LogBuffer {
    chunks: VecDeque<Bytes>,
    bytes: usize,
}

impl LogBuffer {
    /// Returns whether any output was omitted. Full logs can still be streamed to a file.
    pub(crate) fn push_back(&mut self, chunk: Bytes) -> bool {
        let truncated = chunk.len() > MAX_LOG_CHUNK_BYTES;
        let chunk = if truncated {
            // A Bytes slice would keep the entire original allocation alive.
            Bytes::copy_from_slice(&chunk[..MAX_LOG_CHUNK_BYTES])
        } else {
            chunk
        };
        let mut omitted = truncated;
        while self.chunks.len() >= MAX_LOG_LINES || self.bytes + chunk.len() > MAX_LOG_BYTES {
            if let Some(oldest) = self.chunks.pop_front() {
                self.bytes -= oldest.len();
                omitted = true;
            } else {
                break;
            }
        }
        self.bytes += chunk.len();
        self.chunks.push_back(chunk);
        omitted
    }

    pub(crate) fn pop_back(&mut self) -> Option<Bytes> {
        let chunk = self.chunks.pop_back()?;
        self.bytes -= chunk.len();
        Some(chunk)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_history_retains_a_bounded_recent_window() {
        let mut buffer = LogBuffer::default();
        for index in 0..100_000 {
            buffer.push_back(Bytes::from(format!("{index:06} {}", "x".repeat(1024))));
            assert!(buffer.bytes <= MAX_LOG_BYTES);
            assert!(buffer.chunks.len() <= MAX_LOG_LINES);
        }
        assert!(buffer.pop_back().unwrap().starts_with(b"099999 "));
        while buffer.pop_back().is_some() {}
        assert_eq!(buffer.bytes, 0);
    }

    #[test]
    fn one_oversized_frame_cannot_bypass_the_byte_limit() {
        let mut buffer = LogBuffer::default();
        assert!(buffer.push_back(Bytes::from(vec![b'x'; MAX_LOG_BYTES * 2])));
        assert_eq!(buffer.pop_back().unwrap().len(), MAX_LOG_CHUNK_BYTES);
    }

    #[test]
    fn many_tiny_frames_are_also_bounded() {
        let mut buffer = LogBuffer::default();
        for _ in 0..MAX_LOG_LINES * 2 {
            buffer.push_back(Bytes::from_static(b"x"));
        }
        assert_eq!(buffer.chunks.len(), MAX_LOG_LINES);
    }
}
