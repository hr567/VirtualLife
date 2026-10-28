//! Streaming decoder for the disc's FLI/FLC variants. Delta chunks retain prior
//! pixels. Compatibility was checked against N2Game.exe functions 0x00458030,
//! 0x00457d00, 0x00457dc0 and 0x00457ea0, including partial BRUN rows, repeated
//! LC blocks and safely bounded cross-row runs present in the original assets.
use crate::binary::{Reader, pixel_count};

pub struct Decoder {
    data: Vec<u8>,
    frames: Vec<(usize, usize)>,
    pub width: usize,
    pub height: usize,
    pub frame_count: usize,
    pub frame_delay_ms: u32,
    current: usize,
    pixels: Vec<u8>,
    palette: [[u8; 4]; 256],
}

impl Decoder {
    pub fn new(data: &[u8]) -> Result<Self, String> {
        if data.len() < 128 { return Err("Truncated FLIC header".into()); }
        let mut r = Reader::new(data);
        let file_size = r.u32()? as usize;
        let kind = r.u16()?;
        if kind != 0xaf11 && kind != 0xaf12 { return Err(format!("Unsupported FLIC type 0x{kind:04x}")); }
        let frame_count = r.u16()? as usize;
        let width = r.u16()? as usize; let height = r.u16()? as usize;
        let depth = r.u16()?;
        if depth != 0 && depth != 8 { return Err(format!("Unsupported FLIC depth {depth}")); }
        r.skip(2)?;
        let speed = if kind == 0xaf11 { (u32::from(r.u16()?) * 1000).div_ceil(70) } else { r.u32()? };
        if frame_count == 0 { return Err("FLIC has no frames".into()); }
        if file_size > data.len() || file_size < 128 { return Err("Invalid FLIC file size".into()); }
        let count = pixel_count(width, height)?;
        let mut frames = Vec::with_capacity(frame_count);
        let mut offset = 128usize;
        while offset + 6 <= file_size && frames.len() < frame_count {
            let mut chunk = Reader::new(&data[offset..file_size]);
            let size = chunk.u32()? as usize;
            let chunk_type = chunk.u16()?;
            if size < 6 || size > file_size - offset { return Err(format!("Invalid FLIC frame size at {offset}")); }
            if chunk_type == 0xf1fa {
                if size < 16 { return Err("Truncated FLIC frame header".into()); }
                frames.push((offset, size));
            } else if chunk_type != 0xf100 { return Err(format!("Unknown FLIC top-level chunk 0x{chunk_type:04x}")); }
            offset += size;
        }
        if frames.len() != frame_count { return Err(format!("FLIC declares {frame_count} frames but contains {}", frames.len())); }
        Ok(Self { data: data.to_vec(), frames, width, height, frame_count, frame_delay_ms: speed, current: 0, pixels: vec![0; count], palette: [[0,0,0,255]; 256] })
    }

    pub fn reset(&mut self) { self.current = 0; self.pixels.fill(0); self.palette.fill([0,0,0,255]); }
    pub fn indices(&self)->Vec<u8>{self.pixels.clone()}

    pub fn next_frame(&mut self) -> Result<Vec<u8>, String> {
        if self.current == self.frame_count { self.reset(); }
        let (offset, size) = self.frames[self.current];
        let frame = &self.data[offset..offset + size];
        let mut r = Reader::new(frame);
        r.skip(6)?;
        let chunks = r.u16()? as usize;
        r.skip(8)?;
        for chunk_index in 0..chunks {
            let start = r.pos;
            let size = r.u32()? as usize;
            let kind = r.u16()?;
            if size < 6 || size > frame.len() - start { return Err("Invalid FLIC chunk size".into()); }
            // Three verified disc files (FENG_ANI, DOOR, JAPAN) under-report
            // the last COPY chunk by exactly two bytes. The outer frame size
            // includes both bytes. Accept only that precisely bounded variant.
            let missing_copy_header_bytes = kind == 16 && chunk_index + 1 == chunks
                && size - 6 + 2 == self.width * self.height
                && frame.len() - start == size + 2;
            let chunk = r.take(size - 6 + if missing_copy_header_bytes {2} else {0})?;
            apply_chunk(kind, chunk, self.width, self.height, &mut self.pixels, &mut self.palette)?;
        }
        self.current += 1;
        let mut rgba = Vec::with_capacity(self.pixels.len() * 4);
        for index in &self.pixels { rgba.extend_from_slice(&self.palette[*index as usize]); }
        Ok(rgba)
    }
}

fn apply_chunk(kind: u16, data: &[u8], width: usize, height: usize, pixels: &mut [u8], palette: &mut [[u8;4];256]) -> Result<(), String> {
    let mut r = Reader::new(data);
    match kind {
        4 | 11 => {
            let packets = r.u16()?;
            let mut index = 0usize;
            for _ in 0..packets {
                index += r.u8()? as usize;
                let count = r.u8()? as usize;
                let count = if count == 0 { 256 } else { count };
                if index + count > 256 { return Err("FLIC palette packet exceeds 256 colors".into()); }
                for color in &mut palette[index..index + count] {
                    for component in &mut color[..3] {
                        let value = r.u8()?;
                        // Native 0x00457cb0 and 0x00458610 retain six-bit palette precision.
                        *component = if kind == 11 { value << 2 } else { value & 0xfc };
                    }
                    color[3] = 255;
                }
                index += count;
            }
        }
        7 => {
            let mut lines = r.u16()? as usize;
            let mut y = 0usize;
            while lines > 0 {
                let mut control = r.i16()?;
                while (control as u16)&0xc000 == 0xc000 {
                    y += -(control as i32) as usize;
                    if y >= height { return Err("FLIC delta skips beyond image".into()); }
                    control=r.i16()?;
                }
                if y >= height { return Err("FLIC delta line exceeds image".into()); }
                let last_byte=if control<0 {Some(control as u8)}else{None};
                let packets=if last_byte.is_some(){r.i16()?}else{control};
                let mut cursor=y*width;
                for _ in 0..packets {
                    cursor+=r.u8()? as usize;
                    let run=r.i8()? as i16;
                    let count=run.unsigned_abs() as usize*2;
                    let destination=pixels.get_mut(cursor..cursor+count).ok_or("FLIC delta run exceeds image buffer")?;
                    if run<0 {
                        let pair=r.take(2)?;
                        for pixel in destination.chunks_exact_mut(2){pixel.copy_from_slice(pair);}
                    }else{destination.copy_from_slice(r.take(count)?);}
                    cursor+=count;
                }
                if let Some(value)=last_byte {
                    // The native decoder places the odd byte at the packet cursor.
                    // On the shipped files this can differ from width - 1.
                    *pixels.get_mut(cursor).ok_or("FLIC final delta byte exceeds image")?=value;
                }
                y+=1;lines-=1;
            }
        }
        12 => {
            let mut y=0usize;
            // Native 0x00457dc0 accepts multiple LC blocks in one frame chunk.
            while r.pos+1<data.len() {
                y+=r.u16()? as usize;
                let lines=r.u16()? as usize;
                if y+lines>height{return Err("FLIC line-compressed range exceeds image".into());}
                for _ in 0..lines {
                    let packets=r.u8()?;
                    let mut cursor=y*width;
                    for _ in 0..packets {
                        cursor+=r.u8()? as usize;
                        let run=r.i8()? as i16;
                        let count=run.unsigned_abs() as usize;
                        let destination=pixels.get_mut(cursor..cursor+count).ok_or("FLIC line-compressed run exceeds image buffer")?;
                        if run<0 {destination.fill(r.u8()?);}else{destination.copy_from_slice(r.take(count)?);}
                        cursor+=count;
                        if r.pos+1>=data.len(){return Ok(());}
                    }
                    y+=1;
                }
            }
        }
        13 => pixels.fill(0),
        15 => {
            for y in 0..height {
                // Native 0x00457d00 uses the packet count, including incomplete rows.
                let packets=r.u8()?;
                let mut cursor=y*width;
                for _ in 0..packets {
                    let run = r.i8()? as i16;
                    let count = run.unsigned_abs() as usize;
                    let destination=pixels.get_mut(cursor..cursor+count).ok_or("FLIC byte run exceeds image buffer")?;
                    if run >= 0 {destination.fill(r.u8()?);}
                    else {destination.copy_from_slice(r.take(count)?);}
                    cursor+=count;
                }
            }
        }
        16 => {
            // Encoders on the disc use both tightly packed and DWORD-aligned rows.
            let stride = if data.len() == width * height { width } else { width.div_ceil(4) * 4 };
            if data.len() < stride * height { return Err("Truncated FLIC copy frame".into()); }
            for y in 0..height { pixels[y * width..][..width].copy_from_slice(&data[y * stride..][..width]); }
        }
        18 => {}, // Postage-stamp preview: not part of the full-resolution frame.
        _ => return Err(format!("Unsupported FLIC frame chunk {kind}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delta_preserves_untouched_pixels_and_handles_odd_last_pixel() {
        let mut pixels = vec![9; 15]; let mut palette = [[0;4];256];
        // One changed line; skip first row, set odd last byte, then repeat a pair.
        let data = [1,0, 255,255, 7,128, 1,0, 0,254, 1,2];
        apply_chunk(7, &data, 5, 3, &mut pixels, &mut palette).unwrap();
        assert_eq!(&pixels[5..10], &[1,2,1,2,7]);
        assert_eq!(&pixels[0..5], &[9;5]); assert_eq!(&pixels[10..15], &[9;5]);
    }
    #[test]
    fn compressed_frames_reject_out_of_bounds_runs() {
        let mut pixels = vec![0; 4]; let mut palette = [[0;4];256];
        assert!(apply_chunk(15, &[1,5,0], 4,1,&mut pixels,&mut palette).is_err());
        assert!(apply_chunk(12, &[0,0,1,0,1,3,2,1,2],4,1,&mut pixels,&mut palette).is_err());
    }
}
