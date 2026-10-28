pub struct Reader<'a> { bytes: &'a [u8], pub pos: usize }

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self { Self { bytes, pos: 0 } }
    pub fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(count).ok_or("Binary offset overflow")?;
        let result = self.bytes.get(self.pos..end).ok_or_else(|| format!("Truncated binary data at offset {} (need {count} bytes)", self.pos))?;
        self.pos = end;
        Ok(result)
    }
    pub fn skip(&mut self, count: usize) -> Result<(), String> { self.take(count).map(|_| ()) }
    pub fn u8(&mut self) -> Result<u8, String> { Ok(self.take(1)?[0]) }
    pub fn i8(&mut self) -> Result<i8, String> { Ok(self.u8()? as i8) }
    pub fn u16(&mut self) -> Result<u16, String> { Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap())) }
    pub fn i16(&mut self) -> Result<i16, String> { Ok(self.u16()? as i16) }
    pub fn u32(&mut self) -> Result<u32, String> { Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap())) }
    pub fn i32(&mut self) -> Result<i32, String> { Ok(self.u32()? as i32) }
}

pub fn pixel_count(width: usize, height: usize) -> Result<usize, String> {
    let count = width.checked_mul(height).ok_or("Image dimensions overflow")?;
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 || count > 16_777_216 {
        return Err(format!("Unsupported image dimensions {width}×{height}"));
    }
    Ok(count)
}
