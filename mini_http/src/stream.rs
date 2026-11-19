use std::io::{ErrorKind, Read};

use crate::error::{HttpError, HttpResult};

/// How much is asked of the socket in one read.
const READ_SIZE: usize = 4096;

/// A byte stream with a holding area in front of it.
///
/// TCP delivers bytes, not messages. One read may bring half a request, or
/// a request and the start of the next one. This type collects whatever
/// arrives and hands out exactly the piece asked for, leaving the rest for
/// the next caller.
pub struct Buffered<R> {
    inner: R,
    buffer: Vec<u8>,
}

impl<R: Read> Buffered<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
        }
    }

    /// Bytes that have arrived but have not been taken yet.
    pub fn buffered(&self) -> &[u8] {
        &self.buffer
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Pull more bytes from the stream. Returns how many arrived, with 0
    /// meaning the other side has finished sending.
    fn fill(&mut self) -> HttpResult<usize> {
        let mut chunk = [0; READ_SIZE];
        let count = loop {
            match self.inner.read(&mut chunk) {
                Ok(count) => break count,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        };
        self.buffer.extend_from_slice(&chunk[..count]);
        Ok(count)
    }

    /// Take everything up to and including the first `delimiter`.
    ///
    /// Fails with `LimitExceeded` if `limit` bytes go by without one, so a
    /// peer cannot make us buffer without bound. Fails with `Closed` if the
    /// stream ends cleanly with nothing pending, and `UnexpectedEof` if it
    /// ends part-way through.
    pub fn read_until(&mut self, delimiter: &[u8], limit: usize) -> HttpResult<Vec<u8>> {
        let mut searched: usize = 0;
        loop {
            // Only the new bytes need searching, plus enough of the old to
            // catch a delimiter that straddles the two.
            let from = searched.saturating_sub(delimiter.len().saturating_sub(1));
            let found = self.buffer[from..]
                .windows(delimiter.len())
                .position(|w| w == delimiter);
            if let Some(offset) = found {
                let end = from + offset + delimiter.len();
                if end > limit {
                    return Err(HttpError::LimitExceeded);
                }
                return Ok(self.buffer.drain(..end).collect());
            }
            if self.buffer.len() >= limit {
                return Err(HttpError::LimitExceeded);
            }
            searched = self.buffer.len();
            if self.fill()? == 0 {
                let nothing_pending = self.buffer.is_empty();
                return Err(if nothing_pending {
                    HttpError::Closed
                } else {
                    HttpError::UnexpectedEof
                });
            }
        }
    }

    /// Take exactly `len` bytes.
    pub fn read_exact(&mut self, len: usize) -> HttpResult<Vec<u8>> {
        while self.buffer.len() < len {
            if self.fill()? == 0 {
                return Err(HttpError::UnexpectedEof);
            }
        }
        Ok(self.buffer.drain(..len).collect())
    }

    /// Take everything until the stream ends, up to `limit` bytes.
    pub fn read_to_end(&mut self, limit: usize) -> HttpResult<Vec<u8>> {
        loop {
            if self.buffer.len() > limit {
                return Err(HttpError::LimitExceeded);
            }
            if self.fill()? == 0 {
                return Ok(std::mem::take(&mut self.buffer));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Delivers its data a few bytes per read, as a slow network would.
    struct Trickle {
        data: Vec<u8>,
        step: usize,
    }

    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let count = self.step.min(self.data.len()).min(buf.len());
            buf[..count].copy_from_slice(&self.data[..count]);
            self.data.drain(..count);
            Ok(count)
        }
    }

    fn trickle(data: &str, step: usize) -> Buffered<Trickle> {
        Buffered::new(Trickle {
            data: data.as_bytes().to_vec(),
            step,
        })
    }

    #[test]
    fn a_message_split_across_many_reads_comes_out_whole() {
        // Three bytes at a time, so the delimiter itself arrives in pieces.
        let mut stream = trickle("first line\r\n\r\nrest", 3);
        assert_eq!(
            stream.read_until(b"\r\n\r\n", 100),
            Ok(b"first line\r\n\r\n".to_vec())
        );
        assert_eq!(stream.read_exact(4), Ok(b"rest".to_vec()));
    }

    #[test]
    fn two_messages_in_one_read_are_handed_out_one_at_a_time() {
        let mut stream = trickle("one\r\ntwo\r\nthree", 4096);
        assert_eq!(stream.read_until(b"\r\n", 100), Ok(b"one\r\n".to_vec()));
        // The second message was already received and is waiting.
        assert_eq!(stream.buffered(), b"two\r\nthree");
        assert_eq!(stream.read_until(b"\r\n", 100), Ok(b"two\r\n".to_vec()));
        assert_eq!(stream.read_to_end(100), Ok(b"three".to_vec()));
    }

    #[test]
    fn the_end_of_the_stream_is_clean_or_unexpected() {
        let mut empty = trickle("", 10);
        assert_eq!(empty.read_until(b"\r\n", 100), Err(HttpError::Closed));

        let mut cut_off = trickle("half a li", 10);
        assert_eq!(
            cut_off.read_until(b"\r\n", 100),
            Err(HttpError::UnexpectedEof)
        );
        let mut short = trickle("abc", 10);
        assert_eq!(short.read_exact(5), Err(HttpError::UnexpectedEof));
    }

    #[test]
    fn limits_stop_unbounded_buffering() {
        let endless = "x".repeat(10_000);
        let mut stream = trickle(&endless, 512);
        assert_eq!(
            stream.read_until(b"\r\n", 2000),
            Err(HttpError::LimitExceeded)
        );
        // Reading stopped soon after the limit, not at the end of the data.
        assert!(stream.buffered().len() < 3000);

        let mut stream = trickle(&endless, 512);
        assert_eq!(stream.read_to_end(2000), Err(HttpError::LimitExceeded));
        // A delimiter that only appears beyond the limit does not count.
        let mut late = trickle("0123456789\r\n", 4096);
        assert_eq!(late.read_until(b"\r\n", 8), Err(HttpError::LimitExceeded));
    }
}
