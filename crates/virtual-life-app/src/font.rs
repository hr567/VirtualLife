//! Shared 1bpp Chinese bitmap strikes. Layout uses one width class per character,
//! independent of the selected strike, size or synthetic weight.
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub const UI_FONT_RESOURCE: &str = "derived/font/wqy-bitmaps.bin";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    format_version: u32,
    resource: String,
    pixel_format: String,
    bytes: usize,
    glyph_count: usize,
    characters: String,
    half_width_characters: String,
    narrow_characters: String,
    strikes: Vec<Strike>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Strike {
    height: u32,
    offset: usize,
    row_bytes: usize,
    bytes_per_glyph: usize,
}

#[derive(Clone, Copy, Debug)]
enum Width {
    Full,
    Half,
    Narrow,
}
impl Width {
    fn target(self, size: u32) -> u32 {
        match self {
            Self::Full => size,
            Self::Half => size / 2,
            Self::Narrow => (size / 2).saturating_sub(1),
        }
        .max(1)
    }
    fn source(self, size: u32) -> u32 {
        match self {
            Self::Full => size,
            Self::Half => size.div_ceil(2),
            Self::Narrow => size.div_ceil(2).saturating_sub(1),
        }
        .max(1)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "Catalog")]
pub struct GlyphAtlas {
    characters: HashMap<char, (usize, Width)>,
    strikes: Vec<Strike>,
    expected_bytes: usize,
    pixels: Vec<u8>,
}
impl TryFrom<Catalog> for GlyphAtlas {
    type Error = String;
    fn try_from(catalog: Catalog) -> Result<Self, Self::Error> {
        if catalog.format_version != 2
            || catalog.resource != UI_FONT_RESOURCE
            || catalog.pixel_format != "mono1"
            || catalog.glyph_count == 0
            || catalog.strikes.is_empty()
        {
            return Err("Unsupported bitmap font catalog".into());
        }
        let half: HashSet<_> = catalog.half_width_characters.chars().collect();
        let narrow: HashSet<_> = catalog.narrow_characters.chars().collect();
        if !half.is_disjoint(&narrow) {
            return Err("Overlapping bitmap font width classes".into());
        }
        let mut characters = HashMap::with_capacity(catalog.glyph_count);
        for (index, ch) in catalog.characters.chars().enumerate() {
            let width = if half.contains(&ch) {
                Width::Half
            } else if narrow.contains(&ch) {
                Width::Narrow
            } else {
                Width::Full
            };
            if characters.insert(ch, (index, width)).is_some() {
                return Err("Duplicate bitmap font character".into());
            }
        }
        if characters.len() != catalog.glyph_count
            || half
                .iter()
                .chain(&narrow)
                .any(|ch| !characters.contains_key(ch))
            || !characters.contains_key(&'?')
            || !characters.contains_key(&'□')
        {
            return Err("Invalid bitmap font character coverage".into());
        }
        let mut end = 0usize;
        let mut heights = HashSet::new();
        for strike in &catalog.strikes {
            if !(1..=64).contains(&strike.height)
                || !heights.insert(strike.height)
                || strike.offset != end
                || strike.row_bytes != strike.height.div_ceil(8) as usize
                || strike.bytes_per_glyph != strike.row_bytes * strike.height as usize
            {
                return Err("Invalid bitmap font strike layout".into());
            }
            end = strike
                .bytes_per_glyph
                .checked_mul(characters.len())
                .and_then(|bytes| end.checked_add(bytes))
                .ok_or("Bitmap font strike overflow")?;
        }
        if end != catalog.bytes {
            return Err("Bitmap font size does not match its strikes".into());
        }
        Ok(Self {
            characters,
            strikes: catalog.strikes,
            expected_bytes: end,
            pixels: Vec::new(),
        })
    }
}
impl GlyphAtlas {
    pub fn set_pixels(&mut self, pixels: Vec<u8>) -> Result<(), String> {
        if pixels.len() != self.expected_bytes {
            return Err("Bitmap font data has the wrong length".into());
        }
        self.pixels = pixels;
        Ok(())
    }
    pub fn glyph(&self, ch: char, size: u32, bold: bool) -> Option<Glyph<'_>> {
        if size == 0 || size > 128 {
            return None;
        }
        let &(index, width) = self
            .characters
            .get(&ch)
            .or_else(|| self.characters.get(&'□'))
            .or_else(|| self.characters.get(&'?'))?;
        let strike = self
            .strikes
            .iter()
            .min_by_key(|strike| (strike.height.abs_diff(size), strike.height))?;
        Some(Glyph {
            atlas: self,
            strike,
            offset: strike.offset + index * strike.bytes_per_glyph,
            source_width: width.source(strike.height),
            base_width: width.target(size),
            height: size,
            bold,
        })
    }
}

pub struct Glyph<'a> {
    atlas: &'a GlyphAtlas,
    strike: &'a Strike,
    offset: usize,
    source_width: u32,
    base_width: u32,
    pub height: u32,
    bold: bool,
}
impl Glyph<'_> {
    pub fn width(&self) -> u32 {
        self.base_width + u32::from(self.bold)
    }
    pub fn advance(&self) -> i32 {
        self.width() as i32
    }
    pub fn alpha(&self, x: u32, y: u32) -> u8 {
        if y >= self.height || x >= self.width() {
            return 0;
        }
        let sample = |x: u32| {
            if x >= self.base_width {
                return false;
            }
            let sx = x * self.source_width / self.base_width;
            let sy = y * self.strike.height / self.height;
            let at = self.offset + sy as usize * self.strike.row_bytes + sx as usize / 8;
            self.atlas
                .pixels
                .get(at)
                .is_some_and(|byte| byte & (0x80 >> (sx % 8)) != 0)
        };
        if sample(x) || (self.bold && x > 0 && sample(x - 1)) {
            255
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn catalog() -> Value {
        json!({"formatVersion":2,"resource":UI_FONT_RESOURCE,"pixelFormat":"mono1",
            "bytes":12,"glyphCount":3,"characters":"?□中","halfWidthCharacters":"?",
            "narrowCharacters":"","strikes":[{"height":4,"offset":0,"rowBytes":1,"bytesPerGlyph":4}]})
    }
    #[test]
    fn bitmap_font_scales_and_emboldens_bits_without_changing_layout() {
        let mut font: GlyphAtlas = serde_json::from_value(catalog()).unwrap();
        font.set_pixels(vec![
            0x80, 0x40, 0, 0, 0xf0, 0x90, 0x90, 0xf0, 0x60, 0x90, 0xf0, 0x90,
        ])
        .unwrap();
        let glyph = font.glyph('中', 8, false).unwrap();
        assert_eq!((glyph.width(), glyph.height, glyph.advance()), (8, 8, 8));
        assert_eq!(
            (0..8).map(|x| glyph.alpha(x, 0)).collect::<Vec<_>>(),
            [0, 0, 255, 255, 255, 255, 0, 0]
        );
        let glyph = font.glyph('中', 8, true).unwrap();
        assert_eq!(glyph.advance(), 9);
        assert_eq!(
            (0..9).map(|x| glyph.alpha(x, 0)).collect::<Vec<_>>(),
            [0, 0, 255, 255, 255, 255, 255, 0, 0]
        );
        assert_eq!(font.glyph('?', 8, false).unwrap().advance(), 4);
        assert_eq!(font.glyph('🦀', 8, false).unwrap().advance(), 8);
    }
    #[test]
    fn malformed_font_catalogs_and_truncated_bitmaps_are_rejected() {
        for (pointer, value) in [
            ("/formatVersion", json!(1)),
            ("/glyphCount", json!(2)),
            ("/characters", json!("???")),
            ("/halfWidthCharacters", json!("x")),
            ("/narrowCharacters", json!("?")),
            ("/bytes", json!(11)),
            ("/strikes/0/offset", json!(1)),
            ("/strikes/0/height", json!(65)),
            ("/strikes/0/rowBytes", json!(2)),
        ] {
            let mut bad = catalog();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(
                serde_json::from_value::<GlyphAtlas>(bad).is_err(),
                "{pointer}"
            );
        }
        let mut font: GlyphAtlas = serde_json::from_value(catalog()).unwrap();
        assert!(font.set_pixels(vec![0; 11]).is_err());
        assert!(font.glyph('中', 0, false).is_none());
    }

    #[test]
    fn committed_bitmap_font_covers_chinese_ui_and_keeps_legacy_cell_advances() {
        let catalog: Value =
            serde_json::from_str(include_str!("../../../assets/catalog/glyphs.json")).unwrap();
        let mut font: GlyphAtlas = serde_json::from_value(catalog.clone()).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(root.join("assets/manifest.json")).unwrap())
                .unwrap();
        let hash = manifest["resources"][UI_FONT_RESOURCE]["sha256"]
            .as_str()
            .unwrap();
        let object = manifest["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|object| object["sha256"] == hash)
            .unwrap();
        font.set_pixels(std::fs::read(root.join(object["path"].as_str().unwrap())).unwrap())
            .unwrap();
        for size in [12, 14, 16, 20, 36] {
            for bold in [false, true] {
                for (ch, width) in [
                    ('中', size),
                    ('A', size / 2),
                    ('ǜ', size / 2),
                    ('�', size / 2 - 1),
                ] {
                    assert_eq!(
                        font.glyph(ch, size, bold).unwrap().advance(),
                        (width + u32::from(bold)) as i32
                    );
                }
                for ch in "虚拟人生开始游戏姓名保存继续□".chars() {
                    let glyph = font.glyph(ch, size, bold).unwrap();
                    assert!(
                        (0..glyph.height)
                            .any(|y| (0..glyph.width()).any(|x| glyph.alpha(x, y) != 0)),
                        "empty {ch} at {size}"
                    );
                }
                for code in 33..=126 {
                    let ch = char::from_u32(code).unwrap();
                    let glyph = font.glyph(ch, size, bold).unwrap();
                    assert!(
                        (0..glyph.height)
                            .any(|y| (0..glyph.width()).any(|x| glyph.alpha(x, y) != 0)),
                        "empty printable ASCII {ch} at {size}"
                    );
                }
            }
        }
    }
}
