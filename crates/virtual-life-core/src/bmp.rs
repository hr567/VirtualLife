//! BMP decoding for the disc's indexed map tiles and UI images.
//! The image is returned top-down RGBA; transparency is not guessed from palette entries.
use crate::binary::{Reader, pixel_count};

#[derive(Debug)]
pub struct Image { pub width: u32, pub height: u32, pub rgba: Vec<u8>, pub indices: Option<Vec<u8>> }

pub fn decode(data: &[u8]) -> Result<Image, String> {
    // ENDFIGHT/XMP02.BMP, XMP03.BMP and XNP01.BMP are PCX files despite their suffix.
    if data.first() == Some(&0x0a) { return crate::pcx::decode(data); }
    let mut r = Reader::new(data);
    if r.take(2)? != b"BM" { return Err("Not a BMP image".into()); }
    r.skip(8)?;
    let pixel_offset = r.u32()? as usize;
    let dib_size = r.u32()? as usize;
    if dib_size < 40 { return Err(format!("Unsupported BMP header size {dib_size}")); }
    let width = r.i32()?;
    let signed_height = r.i32()?;
    if width <= 0 || signed_height == i32::MIN { return Err("Invalid BMP dimensions".into()); }
    let height = signed_height.unsigned_abs() as usize;
    let width = width as usize;
    let count = pixel_count(width, height)?;
    if r.u16()? != 1 { return Err("BMP must contain one image plane".into()); }
    let depth = r.u16()?;
    let compression = r.u32()?;
    r.skip(12)?;
    let colors_used = r.u32()? as usize;
    r.skip(4)?;
    let header_end = 14usize.checked_add(dib_size).ok_or("BMP header overflow")?;
    if pixel_offset < header_end || pixel_offset > data.len() { return Err("Invalid BMP pixel offset".into()); }
    if !matches!(depth, 1 | 4 | 8 | 16 | 24 | 32) { return Err(format!("Unsupported BMP depth {depth}")); }
    if compression != 0 && !(compression == 1 && depth == 8) && !(compression == 2 && depth == 4) && !(compression == 3 && (depth == 16 || depth == 32)) {
        return Err(format!("Unsupported BMP compression {compression} for {depth} bits"));
    }
    let mut palette = Vec::new();
    if depth <= 8 {
        let color_count = if colors_used == 0 { 1usize << depth } else { colors_used };
        if color_count > 1usize << depth { return Err("Invalid BMP palette size".into()); }
        let palette_end = header_end.checked_add(color_count * 4).ok_or("Palette overflow")?;
        if palette_end > pixel_offset { return Err("BMP palette overlaps pixels".into()); }
        let mut p = Reader::new(data.get(header_end..palette_end).ok_or("Truncated BMP palette")?);
        for _ in 0..color_count {
            let b = p.u8()?; let g = p.u8()?; let red = p.u8()?; p.skip(1)?;
            palette.push([red, g, b, 255]);
        }
        // LoadImageA(LR_LOADFROMFILE | LR_CREATEDIBSECTION), used at 0x45cf40,
        // zero-fills undeclared palette entries. The two PM0P backgrounds have
        // 255 entries but contain index 255. Original GDI capture is retained.
        palette.resize(1usize << depth, [0, 0, 0, 255]);
    }
    let masks = if compression == 3 {
        let mut m = Reader::new(data.get(54..66).ok_or("Truncated BMP bit masks")?);
        [m.u32()?, m.u32()?, m.u32()?]
    } else if depth == 16 { [0x7c00, 0x03e0, 0x001f] } else { [0xff0000, 0xff00, 0xff] };
    let mut rgba = vec![0u8; count * 4];
    let mut output_indices = (depth <= 8).then(|| vec![0u8; count]);
    if compression == 1 || compression == 2 {
        if signed_height < 0 { return Err("Top-down compressed BMP is not supported".into()); }
        let indices = decode_rle(&data[pixel_offset..], width, height, depth)?;
        for y in 0..height {
            for x in 0..width {
                let color = palette.get(indices[y * width + x] as usize).ok_or("BMP palette index out of range")?;
                rgba[((height - 1 - y) * width + x) * 4..][..4].copy_from_slice(color);
                output_indices.as_mut().unwrap()[(height - 1 - y) * width + x] = indices[y * width + x];
            }
        }
    } else {
        let stride = (width * depth as usize).div_ceil(32) * 4;
        let pixels = data.get(pixel_offset..pixel_offset.checked_add(stride * height).ok_or("BMP pixel extent overflow")?).ok_or("Truncated BMP pixels")?;
        for y in 0..height {
            let source_y = if signed_height > 0 { height - 1 - y } else { y };
            let row = &pixels[source_y * stride..][..stride];
            for x in 0..width {
                let color = match depth {
                    1 | 4 | 8 => {
                        let bit = x * depth as usize;
                        let index = (row[bit / 8] >> (8 - depth as usize - bit % 8)) & ((1u16 << depth) - 1) as u8;
                        output_indices.as_mut().unwrap()[y * width + x] = index;
                        *palette.get(index as usize).ok_or("BMP palette index out of range")?
                    }
                    16 | 32 => {
                        let p = x * (depth as usize / 8);
                        let word = if depth == 16 { u16::from_le_bytes(row[p..p + 2].try_into().unwrap()) as u32 } else { u32::from_le_bytes(row[p..p + 4].try_into().unwrap()) };
                        [component(word, masks[0])?, component(word, masks[1])?, component(word, masks[2])?, 255]
                    }
                    24 => [row[x * 3 + 2], row[x * 3 + 1], row[x * 3], 255],
                    _ => unreachable!(),
                };
                rgba[(y * width + x) * 4..][..4].copy_from_slice(&color);
            }
        }
    }
    Ok(Image { width: width as u32, height: height as u32, rgba, indices: output_indices })
}

fn component(value: u32, mask: u32) -> Result<u8, String> {
    if mask == 0 { return Err("Empty BMP color mask".into()); }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    if max & max.wrapping_add(1) != 0 { return Err("Non-contiguous BMP color mask".into()); }
    Ok((u64::from((value & mask) >> shift) * 255 / u64::from(max)) as u8)
}

fn decode_rle(data: &[u8], width: usize, height: usize, depth: u16) -> Result<Vec<u8>, String> {
    let mut r = Reader::new(data);
    let mut pixels = vec![0; width * height];
    let (mut x, mut y) = (0usize, 0usize);
    loop {
        let n = r.u8()? as usize;
        let value = r.u8()?;
        if n > 0 {
            if y >= height || x + n > width { return Err("BMP RLE run exceeds image".into()); }
            for i in 0..n { pixels[y * width + x + i] = if depth == 8 { value } else if i % 2 == 0 { value >> 4 } else { value & 15 }; }
            x += n;
        } else {
            match value {
                0 => { x = 0; y += 1; if y > height { return Err("BMP RLE line exceeds image".into()); } }
                1 => return Ok(pixels),
                2 => { x += r.u8()? as usize; y += r.u8()? as usize; if x > width || y >= height { return Err("BMP RLE delta exceeds image".into()); } }
                n => {
                    let n = n as usize;
                    if y >= height || x + n > width { return Err("BMP RLE literal exceeds image".into()); }
                    let bytes = if depth == 8 { n } else { n.div_ceil(2) };
                    let literal = r.take(bytes)?;
                    for i in 0..n { pixels[y * width + x + i] = if depth == 8 { literal[i] } else if i % 2 == 0 { literal[i / 2] >> 4 } else { literal[i / 2] & 15 }; }
                    if bytes % 2 == 1 { r.skip(1)?; }
                    x += n;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_short_palettes_match_native_gdi() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!("../../../tests/fixtures/original-bmp.json")).unwrap();
        for case in fixtures["cases"].as_array().unwrap() {
            let hash = case["sha256"].as_str().unwrap();
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../assets/game/objects/{}/{hash}.bmp", &hash[..2]));
            let image = decode(&std::fs::read(path).unwrap()).unwrap();
            let actual = image.rgba.iter().fold(2166136261u32, |sum, byte| (sum ^ u32::from(*byte)).wrapping_mul(16777619));
            assert_eq!(actual, case["rgbaFnv32"].as_u64().unwrap() as u32, "{}", case["resource"]);
        }
    }
    #[test]
    fn indexed_rows_use_bottom_up_order_and_padding() {
        let mut bmp = vec![0; 70]; bmp[0..2].copy_from_slice(b"BM");
        bmp[10..14].copy_from_slice(&62u32.to_le_bytes()); bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&2i32.to_le_bytes()); bmp[22..26].copy_from_slice(&2i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes()); bmp[28..30].copy_from_slice(&8u16.to_le_bytes());
        bmp[46..50].copy_from_slice(&2u32.to_le_bytes()); bmp[58..62].copy_from_slice(&[3, 2, 1, 0]);
        bmp[62..70].copy_from_slice(&[0, 1, 0, 0, 1, 0, 0, 0]);
        let decoded = decode(&bmp).unwrap();
        assert_eq!(decoded.rgba, [1,2,3,255, 0,0,0,255, 0,0,0,255, 1,2,3,255]);
        assert_eq!(decoded.indices.as_deref(), Some([1,0,0,1].as_slice()));
        assert!(decode(&bmp[..69]).is_err());
    }
    #[test]
    fn rle_absolute_padding_and_bounds() {
        assert_eq!(decode_rle(&[0,3,1,2,3,0,0,1], 3, 1, 8).unwrap(), [1,2,3]);
        assert!(decode_rle(&[4,1,0,1], 3, 1, 8).is_err());
    }
}
