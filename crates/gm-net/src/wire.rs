//! Byte-level encoding: little-endian scalars, LEB128 varints, length-prefixed strings.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Truncated;

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.u16(v as u16);
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }
    pub fn varint(&mut self, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(b);
                return;
            }
            self.buf.push(b | 0x80);
        }
    }
    pub fn str(&mut self, s: &str) {
        self.varint(s.len() as u64);
        self.buf.extend_from_slice(s.as_bytes());
    }
    pub fn vec3(&mut self, v: glam::Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.varint(b.len() as u64);
        self.buf.extend_from_slice(b);
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], Truncated> {
        if self.pos + n > self.buf.len() {
            return Err(Truncated);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, Truncated> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Truncated> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn i16(&mut self) -> Result<i16, Truncated> {
        Ok(self.u16()? as i16)
    }
    pub fn u32(&mut self) -> Result<u32, Truncated> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32, Truncated> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn bool(&mut self) -> Result<bool, Truncated> {
        Ok(self.u8()? != 0)
    }
    pub fn varint(&mut self) -> Result<u64, Truncated> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(Truncated)
    }
    pub fn str(&mut self) -> Result<String, Truncated> {
        let n = self.varint()? as usize;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| Truncated)
    }
    pub fn vec3(&mut self) -> Result<glam::Vec3, Truncated> {
        Ok(glam::Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    pub fn bytes(&mut self) -> Result<&'a [u8], Truncated> {
        let n = self.varint()? as usize;
        self.take(n)
    }
}
