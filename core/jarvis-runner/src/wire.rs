//! Bounds-checked primitive reads and writes for the message codec.
//!
//! Every read checks the remaining length before it touches or allocates anything, so a
//! length field can never make the decoder allocate more than the bytes it was given.

use crate::error::MessageError;

/// Appends primitives to a buffer.
#[derive(Debug, Default)]
pub(crate) struct Writer {
    pub(crate) buf: Vec<u8>,
}

impl Writer {
    pub(crate) fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub(crate) fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    pub(crate) fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub(crate) fn f64(&mut self, v: f64) -> Result<(), MessageError> {
        if !v.is_finite() {
            return Err(MessageError::NonFinite);
        }
        self.buf.extend_from_slice(&v.to_bits().to_be_bytes());
        Ok(())
    }

    /// A length as u32; refuses anything that does not fit.
    pub(crate) fn length(&mut self, n: usize, what: &'static str) -> Result<(), MessageError> {
        let n = u32::try_from(n).map_err(|_| MessageError::Limit(what))?;
        self.u32(n);
        Ok(())
    }

    pub(crate) fn bytes(&mut self, v: &[u8], what: &'static str) -> Result<(), MessageError> {
        self.length(v.len(), what)?;
        self.buf.extend_from_slice(v);
        Ok(())
    }

    pub(crate) fn str(&mut self, v: &str) -> Result<(), MessageError> {
        self.bytes(v.as_bytes(), "string")
    }

    pub(crate) fn opt_str(&mut self, v: Option<&str>) -> Result<(), MessageError> {
        match v {
            None => {
                self.u8(0);
                Ok(())
            }
            Some(s) => {
                self.u8(1);
                self.str(s)
            }
        }
    }
}

/// Reads primitives from a slice.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    pub(crate) fn finish(&self) -> Result<(), MessageError> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(MessageError::Trailing)
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], MessageError> {
        if n > self.rest.len() {
            return Err(MessageError::Truncated);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], MessageError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, MessageError> {
        Ok(self.array::<1>()?[0])
    }

    pub(crate) fn bool(&mut self) -> Result<bool, MessageError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(MessageError::Unknown("boolean")),
        }
    }

    pub(crate) fn u32(&mut self) -> Result<u32, MessageError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, MessageError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub(crate) fn f64(&mut self) -> Result<f64, MessageError> {
        let v = f64::from_bits(self.u64()?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(MessageError::NonFinite)
        }
    }

    /// A u32 length, checked against the bytes left (each counted item takes at least
    /// `min_item` bytes), so no claimed length allocates more than the input holds.
    pub(crate) fn count(&mut self, min_item: usize) -> Result<usize, MessageError> {
        let n = usize::try_from(self.u32()?).map_err(|_| MessageError::Truncated)?;
        if n.saturating_mul(min_item.max(1)) > self.rest.len() {
            return Err(MessageError::Truncated);
        }
        Ok(n)
    }

    pub(crate) fn bytes(&mut self) -> Result<&'a [u8], MessageError> {
        let n = self.count(1)?;
        self.take(n)
    }

    pub(crate) fn string(&mut self) -> Result<String, MessageError> {
        let raw = self.bytes()?;
        std::str::from_utf8(raw)
            .map(str::to_owned)
            .map_err(|_| MessageError::Utf8)
    }

    pub(crate) fn opt_string(&mut self) -> Result<Option<String>, MessageError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(MessageError::Unknown("option tag")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claimed_length_beyond_the_input_is_truncated_without_allocating() {
        let mut r = Reader::new(&[0xff, 0xff, 0xff, 0xff, 1, 2]);
        assert_eq!(r.bytes(), Err(MessageError::Truncated));
        let mut r = Reader::new(&[0, 0, 0, 3, 1, 2]);
        assert_eq!(r.count(1), Err(MessageError::Truncated));
    }

    #[test]
    fn non_finite_numbers_are_refused_both_ways() {
        let mut w = Writer::default();
        assert_eq!(w.f64(f64::NAN), Err(MessageError::NonFinite));
        let bits = f64::INFINITY.to_bits().to_be_bytes();
        assert_eq!(Reader::new(&bits).f64(), Err(MessageError::NonFinite));
    }
}
