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
    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    pub fn u64(&mut self, v: u64) {
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
    /// Zigzag varint.
    pub fn svarint(&mut self, v: i64) {
        self.varint(((v << 1) ^ (v >> 63)) as u64);
    }

    /// Position quantized to 1/32 u (zigzag varints: 3 bytes per axis within +-4096 u).
    pub fn qpos(&mut self, v: glam::Vec3) {
        for a in 0..3 {
            self.svarint((v[a] * 32.0).round() as i64);
        }
    }

    /// Angles in degrees wrapped to [-180, 180) at 1/64 degree (i16 per axis).
    pub fn qang(&mut self, v: glam::Vec3) {
        for a in 0..3 {
            let d = (v[a] + 180.0).rem_euclid(360.0) - 180.0;
            self.i16((d * 64.0).round().clamp(-32768.0, 32767.0) as i16);
        }
    }

    /// Velocity at `1/scale` units (i16 per axis, clamped).
    pub fn qvel(&mut self, v: glam::Vec3, scale: f32) {
        for a in 0..3 {
            self.i16((v[a] * scale).round().clamp(-32768.0, 32767.0) as i16);
        }
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
    pub fn i32(&mut self) -> Result<i32, Truncated> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64, Truncated> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
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
    pub fn svarint(&mut self) -> Result<i64, Truncated> {
        let u = self.varint()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    pub fn qpos(&mut self) -> Result<glam::Vec3, Truncated> {
        let mut v = glam::Vec3::ZERO;
        for a in 0..3 {
            v[a] = self.svarint()? as f32 / 32.0;
        }
        Ok(v)
    }

    pub fn qang(&mut self) -> Result<glam::Vec3, Truncated> {
        let mut v = glam::Vec3::ZERO;
        for a in 0..3 {
            v[a] = self.i16()? as f32 / 64.0;
        }
        Ok(v)
    }

    pub fn qvel(&mut self, scale: f32) -> Result<glam::Vec3, Truncated> {
        let mut v = glam::Vec3::ZERO;
        for a in 0..3 {
            v[a] = self.i16()? as f32 / scale;
        }
        Ok(v)
    }

    pub fn vec3(&mut self) -> Result<glam::Vec3, Truncated> {
        Ok(glam::Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    pub fn bytes(&mut self) -> Result<&'a [u8], Truncated> {
        let n = self.varint()? as usize;
        self.take(n)
    }
}
