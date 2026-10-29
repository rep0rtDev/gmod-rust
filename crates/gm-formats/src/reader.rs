use crate::{FormatError, Result};

/// Little-endian cursor over a byte slice. Every read is bounds checked and
/// reports the offset on failure so harness diffs point at the exact byte.
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(FormatError::Eof {
                offset: pos,
                needed: 0,
            });
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(FormatError::Eof {
            offset: self.pos,
            needed: n,
        })?;
        if end > self.data.len() {
            return Err(FormatError::Eof {
                offset: self.pos,
                needed: n,
            });
        }
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let b = self.bytes(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(b);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// NUL-terminated string. The terminator is consumed.
    pub fn cstr(&mut self) -> Result<&'a str> {
        let start = self.pos;
        let rest = &self.data[self.pos.min(self.data.len())..];
        let len = rest.iter().position(|&b| b == 0).ok_or(FormatError::Eof {
            offset: start,
            needed: 1,
        })?;
        let s =
            std::str::from_utf8(&rest[..len]).map_err(|_| FormatError::Utf8 { offset: start })?;
        self.pos = start + len + 1;
        Ok(s)
    }
}
