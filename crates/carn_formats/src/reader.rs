//! A little-endian cursor over a byte slice.

use std::fmt;

#[derive(Debug, Clone)]
pub struct FormatError(pub String);

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormatError {}

impl From<std::io::Error> for FormatError {
    fn from(e: std::io::Error) -> Self {
        FormatError(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, FormatError>;

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(FormatError(format!(
                "unexpected end of file: wanted {n} bytes at offset {}, {} left",
                self.pos,
                self.remaining()
            )));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// A count read from the file, refused if it is negative or absurd, so a
    /// damaged file ends in an error rather than a huge allocation.
    pub fn count(&mut self, what: &str, max: usize) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > max {
            return Err(FormatError(format!("bad {what} count {n}")));
        }
        Ok(n as usize)
    }

    pub fn u16_vec(&mut self, n: usize) -> Result<Vec<u16>> {
        let b = self.bytes(n * 2)?;
        Ok(b.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect())
    }

    pub fn i16_vec(&mut self, n: usize) -> Result<Vec<i16>> {
        let b = self.bytes(n * 2)?;
        Ok(b.chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect())
    }

    pub fn u8_vec(&mut self, n: usize) -> Result<Vec<u8>> {
        Ok(self.bytes(n)?.to_vec())
    }

    /// A fixed-size, NUL-padded name.
    pub fn name(&mut self, n: usize) -> Result<String> {
        let b = self.bytes(n)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(n);
        Ok(b[..end].iter().map(|&c| c as char).collect())
    }
}
