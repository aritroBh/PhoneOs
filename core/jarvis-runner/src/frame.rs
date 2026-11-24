//! Length-prefixed framing: `length (u32 BE) || body`, `1 <= length <= limit`.
//!
//! The length is checked as soon as its four bytes arrive, before any body byte is
//! buffered, and the body buffer grows only with bytes actually received. Frames come out
//! one at a time, so a channel can tighten or loosen the limit between two frames that
//! arrived in the same read (the handshake limit is tiny; the data limit is not).

use crate::error::RunnerError;

/// Reassembles frames from a byte stream.
#[derive(Debug)]
pub struct Deframer {
    header: [u8; 4],
    header_len: usize,
    /// Length of the frame being read, once its header is complete.
    expected: Option<usize>,
    body: Vec<u8>,
    limit: usize,
}

impl Deframer {
    /// A deframer accepting bodies of at most `limit` bytes.
    pub fn new(limit: usize) -> Self {
        Self {
            header: [0; 4],
            header_len: 0,
            expected: None,
            body: Vec::new(),
            limit,
        }
    }

    /// Changes the limit for frames whose header has not been read yet.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// Whether a frame is partly read.
    pub fn is_mid_frame(&self) -> bool {
        self.header_len > 0 || self.expected.is_some()
    }

    /// Consumes bytes from `input` until one frame is complete (returned) or `input` is
    /// empty (`None`). A length of 0 or over the limit is an error.
    pub fn next_frame(&mut self, input: &mut &[u8]) -> Result<Option<Vec<u8>>, RunnerError> {
        loop {
            match self.expected {
                None => {
                    let Some((&b, rest)) = input.split_first() else {
                        return Ok(None);
                    };
                    *input = rest;
                    if let Some(slot) = self.header.get_mut(self.header_len) {
                        *slot = b;
                    }
                    self.header_len += 1;
                    if self.header_len == 4 {
                        let len = u32::from_be_bytes(self.header);
                        let len = usize::try_from(len).map_err(|_| RunnerError::Frame("length"))?;
                        if len == 0 {
                            return Err(RunnerError::Frame("empty frame"));
                        }
                        if len > self.limit {
                            return Err(RunnerError::Frame("frame over the limit"));
                        }
                        self.header_len = 0;
                        self.expected = Some(len);
                    }
                }
                Some(len) => {
                    let want = len.saturating_sub(self.body.len());
                    let take = want.min(input.len());
                    let (now, rest) = input.split_at(take);
                    self.body.extend_from_slice(now);
                    *input = rest;
                    if self.body.len() < len {
                        return Ok(None);
                    }
                    self.expected = None;
                    return Ok(Some(std::mem::take(&mut self.body)));
                }
            }
        }
    }
}

/// Prefixes `body` with its length.
pub fn frame(body: &[u8]) -> Result<Vec<u8>, RunnerError> {
    let len = u32::try_from(body.len()).map_err(|_| RunnerError::Frame("length"))?;
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_come_out_one_at_a_time_and_split_anywhere() {
        let mut bytes = frame(b"abc").unwrap();
        bytes.extend(frame(b"de").unwrap());
        for split in 0..=bytes.len() {
            let mut d = Deframer::new(16);
            let mut out = Vec::new();
            for chunk in [&bytes[..split], &bytes[split..]] {
                let mut input = chunk;
                while let Some(f) = d.next_frame(&mut input).unwrap() {
                    out.push(f);
                }
                assert!(input.is_empty());
            }
            assert_eq!(out, vec![b"abc".to_vec(), b"de".to_vec()]);
        }
    }

    #[test]
    fn an_oversized_length_fails_before_any_body_byte() {
        let mut d = Deframer::new(8);
        let mut input: &[u8] = &[0, 0, 0, 9];
        assert_eq!(
            d.next_frame(&mut input),
            Err(RunnerError::Frame("frame over the limit"))
        );
        let mut d = Deframer::new(8);
        let mut input: &[u8] = &[0, 0, 0, 0];
        assert_eq!(
            d.next_frame(&mut input),
            Err(RunnerError::Frame("empty frame"))
        );
        // A huge claimed length allocates nothing: the body grows only with real bytes.
        let mut d = Deframer::new(usize::MAX);
        let mut input: &[u8] = &[0xff, 0xff, 0xff, 0xff, 1];
        assert_eq!(d.next_frame(&mut input), Ok(None));
        assert!(d.body.capacity() < 64);
    }
}
