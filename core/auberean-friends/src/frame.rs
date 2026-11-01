//! The stream's bytes: the preamble, then frames, each a 4-byte little-endian length and that many
//! bytes of UTF-8 holding one element.

use core::fmt;

/// The client's first four bytes.
pub const PREAMBLE: [u8; 4] = *b"DNF1";

/// The first four bytes of a WebSocket upgrade on the same port.
pub const WEBSOCKET_OPENING: [u8; 4] = *b"GET ";

/// The longest frame, in bytes of text.
pub const MAX_FRAME: usize = 65_536;

/// What is wrong with a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// A length of zero.
    Empty,
    /// A length past [`MAX_FRAME`].
    TooLong(usize),
    /// Text that is not UTF-8.
    NotUtf8,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a frame of no bytes"),
            Self::TooLong(n) => write!(f, "a frame of {n} bytes, past {MAX_FRAME}"),
            Self::NotUtf8 => f.write_str("a frame that is not UTF-8"),
        }
    }
}

/// Append one frame holding `text` to `out`.
///
/// # Errors
/// [`FrameError`] when `text` is empty or longer than [`MAX_FRAME`] bytes; `out` is unchanged.
pub fn write_frame(text: &str, out: &mut Vec<u8>) -> Result<(), FrameError> {
    let len = text.len();
    if len == 0 {
        return Err(FrameError::Empty);
    }
    let Ok(word) = u32::try_from(len) else {
        return Err(FrameError::TooLong(len));
    };
    if len > MAX_FRAME {
        return Err(FrameError::TooLong(len));
    }
    out.extend_from_slice(&word.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    Ok(())
}

/// Cuts frames out of the bytes a connection delivers, however they arrive.
#[derive(Debug, Default)]
pub struct FrameReader {
    buf: Vec<u8>,
    failed: Option<FrameError>,
}

impl FrameReader {
    /// A reader that has read nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take more bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.failed.is_none() {
            self.buf.extend_from_slice(bytes);
        }
    }

    /// The next whole frame's text, `Ok(None)` while one is still arriving.
    ///
    /// # Errors
    /// [`FrameError`] for a frame the stream must be closed over; the reader then stays failed.
    pub fn next_frame(&mut self) -> Result<Option<String>, FrameError> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        let Some(head) = self.buf.first_chunk::<4>() else {
            return Ok(None);
        };
        let len = usize::try_from(u32::from_le_bytes(*head)).unwrap_or(usize::MAX);
        let bad = if len == 0 {
            Some(FrameError::Empty)
        } else if len > MAX_FRAME {
            Some(FrameError::TooLong(len))
        } else {
            None
        };
        if let Some(e) = bad {
            return Err(self.fail(e));
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let body: Vec<u8> = self.buf.drain(..4 + len).skip(4).collect();
        match String::from_utf8(body) {
            Ok(text) => Ok(Some(text)),
            Err(_) => Err(self.fail(FrameError::NotUtf8)),
        }
    }

    fn fail(&mut self, e: FrameError) -> FrameError {
        self.failed = Some(e);
        self.buf = Vec::new();
        e
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the frame reader against bytes cut at every place)
    use super::*;

    #[test]
    fn frames_cut_anywhere_read_whole() {
        let mut bytes = Vec::new();
        write_frame("<Ping Seq=\"1\"/>", &mut bytes).unwrap();
        write_frame("<Pong Seq=\"2\"/>", &mut bytes).unwrap();
        for cut in 0..bytes.len() {
            let mut r = FrameReader::new();
            let mut got = Vec::new();
            for part in [&bytes[..cut], &bytes[cut..]] {
                r.push(part);
                while let Some(f) = r.next_frame().unwrap() {
                    got.push(f);
                }
            }
            assert_eq!(
                got,
                ["<Ping Seq=\"1\"/>", "<Pong Seq=\"2\"/>"],
                "cut at {cut}"
            );
        }
    }

    #[test]
    fn a_failed_reader_stays_failed() {
        let mut r = FrameReader::new();
        r.push(&[0, 0, 0, 0]);
        assert_eq!(r.next_frame(), Err(FrameError::Empty));
        r.push(b"\x01\0\0\0x");
        assert_eq!(r.next_frame(), Err(FrameError::Empty));
    }
}
