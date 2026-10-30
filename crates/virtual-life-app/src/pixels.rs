//! Byte-exact original image composition, presented by Bevy as one 640×480 Image.
//! Keeping palette arithmetic in byte space avoids GPU color-space/rounding drift.
pub use crate::font::GlyphAtlas;
use crate::scene::*;
use bevy::prelude::Resource;
use encoding_rs::GBK;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use virtual_life_core::{bmp, flic};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 480;

#[derive(Clone, Debug)]
pub struct PixelImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub indices: Option<Vec<u8>>,
}
impl From<bmp::Image> for PixelImage {
    fn from(image: bmp::Image) -> Self {
        Self {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
            indices: image.indices,
        }
    }
}
enum Decoded {
    Bitmap(Arc<PixelImage>),
    Animation {
        decoder: flic::Decoder,
        frame: u32,
        image: Arc<PixelImage>,
    },
}
#[derive(Resource, Default)]
pub struct ImageBank {
    pub revision: u64,
    raw: HashMap<String, Arc<[u8]>>,
    decoded: HashMap<String, Decoded>,
    palettes: HashMap<String, [[u8; 4]; 256]>,
    pub failures: HashMap<String, String>,
    pub requested: HashSet<String>,
    pub video: Option<(String, u64, Arc<PixelImage>)>,
    pub glyphs: Option<GlyphAtlas>,
    remaps: HashMap<Vec<[u8; 3]>, Arc<Vec<[u8; 256]>>>,
}
impl ImageBank {
    pub fn insert(&mut self, id: String, bytes: Vec<u8>) {
        self.revision = self.revision.wrapping_add(1);
        self.requested.remove(&id);
        self.failures.remove(&id);
        self.raw.insert(id, bytes.into());
    }
    pub fn bytes(&self, id: &str) -> Option<Arc<[u8]>> {
        self.raw.get(id).cloned()
    }
    pub fn has(&self, id: &str) -> bool {
        self.raw.contains_key(id)
    }
    pub fn frame_count(&self, id: &str) -> Option<u32> {
        let bytes = self.raw.get(id)?;
        if bytes.len() > 8 && matches!(u16::from_le_bytes([bytes[4], bytes[5]]), 0xaf11 | 0xaf12) {
            Some(u16::from_le_bytes([bytes[6], bytes[7]]) as u32)
        } else {
            Some(1)
        }
    }
    pub fn delay(&self, id: &str) -> u32 {
        self.raw
            .get(id)
            .and_then(|bytes| {
                if bytes.len() < 20 {
                    return None;
                }
                let kind = u16::from_le_bytes([bytes[4], bytes[5]]);
                match kind {
                    0xaf11 => Some(
                        (u32::from(u16::from_le_bytes([bytes[16], bytes[17]])) * 1000).div_ceil(70),
                    ),
                    0xaf12 => Some(u32::from_le_bytes(bytes[16..20].try_into().unwrap())),
                    _ => None,
                }
            })
            .unwrap_or(100)
            .max(1)
    }
    pub fn image(&mut self, id: &str, target: u32) -> Option<Arc<PixelImage>> {
        if !self.decoded.contains_key(id) {
            let bytes = self.raw.get(id)?;
            let decoded = if bytes.len() > 6
                && matches!(u16::from_le_bytes([bytes[4], bytes[5]]), 0xaf11 | 0xaf12)
            {
                flic::Decoder::new(bytes).and_then(|mut decoder| {
                    let rgba = decoder.next_frame()?;
                    let image = Arc::new(PixelImage {
                        width: decoder.width as u32,
                        height: decoder.height as u32,
                        rgba,
                        indices: Some(decoder.indices()),
                    });
                    Ok(Decoded::Animation {
                        decoder,
                        frame: 0,
                        image,
                    })
                })
            } else {
                bmp::decode(bytes).map(|image| Decoded::Bitmap(Arc::new(image.into())))
            };
            match decoded {
                Ok(decoded) => {
                    self.decoded.insert(id.into(), decoded);
                }
                Err(error) => {
                    self.failures.insert(id.into(), error);
                    return None;
                }
            }
        }
        match self.decoded.get_mut(id)? {
            Decoded::Bitmap(image) => Some(image.clone()),
            Decoded::Animation {
                decoder,
                frame,
                image,
            } => {
                let target = target % decoder.frame_count as u32;
                if target < *frame {
                    decoder.reset();
                    *frame = u32::MAX;
                }
                while *frame != target {
                    let rgba = match decoder.next_frame() {
                        Ok(rgba) => rgba,
                        Err(error) => {
                            self.failures.insert(id.into(), error);
                            return None;
                        }
                    };
                    *frame = if *frame == u32::MAX { 0 } else { *frame + 1 };
                    *image = Arc::new(PixelImage {
                        width: decoder.width as u32,
                        height: decoder.height as u32,
                        rgba,
                        indices: Some(decoder.indices()),
                    });
                }
                Some(image.clone())
            }
        }
    }
    pub fn palette(&mut self, id: &str) -> Option<[[u8; 4]; 256]> {
        if let Some(palette) = self.palettes.get(id) {
            return Some(*palette);
        }
        let image = self.image(id, 0)?;
        let mut palette = [[0, 0, 0, 255]; 256];
        if let Some(indices) = &image.indices {
            for (i, index) in indices.iter().enumerate() {
                palette[*index as usize].copy_from_slice(&image.rgba[i * 4..i * 4 + 4]);
            }
        }
        let raw = self.raw.get(id)?;
        if raw.starts_with(b"BM") && raw.len() >= 54 {
            let offset = 14 + u32::from_le_bytes(raw[14..18].try_into().ok()?) as usize;
            let depth = u16::from_le_bytes(raw[28..30].try_into().ok()?);
            if depth <= 8 {
                let count = u32::from_le_bytes(raw[46..50].try_into().ok()?);
                let count = if count == 0 {
                    1usize << depth
                } else {
                    count as usize
                };
                for (i, color) in palette.iter_mut().take(count.min(256)).enumerate() {
                    let p = offset + i * 4;
                    if p + 4 <= raw.len() {
                        *color = [raw[p + 2], raw[p + 1], raw[p], 255]
                    }
                }
            }
        }
        if raw.first() == Some(&10) && raw.len() >= 769 && raw[raw.len() - 769] == 12 {
            for (index, color) in raw[raw.len() - 768..].chunks_exact(3).enumerate() {
                palette[index] = [color[0], color[1], color[2], 255];
            }
        }
        // Decoder::palette includes unused entries and all original palette packets.
        if let Some(Decoded::Animation { decoder, .. }) = self.decoded.get(id) {
            palette = decoder.palette();
        }
        self.palettes.insert(id.into(), palette);
        Some(palette)
    }
    pub fn retain_scene(&mut self, used: &HashSet<String>) {
        self.decoded.retain(|id, _| used.contains(id));
        self.palettes.retain(|id, _| used.contains(id));
        self.raw
            .retain(|id, _| used.contains(id) || id.contains("/font/"));
        self.failures.retain(|id, _| used.contains(id));
    }
}

#[derive(Clone, Copy)]
enum Layer {
    Sprite(usize),
    Text(usize),
    Fill(usize),
    Video,
    Summary,
}
#[derive(Resource)]
pub struct PixelCanvas {
    pub rgba: Vec<u8>,
    pub revision: u64,
    layers: Vec<(i32, usize, Layer)>,
}
impl Default for PixelCanvas {
    fn default() -> Self {
        Self {
            rgba: vec![0; WIDTH as usize * HEIGHT as usize * 4],
            revision: 0,
            layers: Vec::new(),
        }
    }
}

impl PixelCanvas {
    pub fn compose(&mut self, scene: &SceneFrame, bank: &mut ImageBank) {
        for pixel in self.rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 0, 0, 255]);
        }
        let mut palette = scene.palette.as_deref().and_then(|id| bank.palette(id));
        for (id, first, last) in &scene.palette_sections {
            if let Some(source) = bank.palette(id) {
                let p = palette.get_or_insert([[0; 4]; 256]);
                for i in *first..=*last {
                    p[i as usize] = source[i as usize];
                }
            }
        }
        if let Some(colors) = &scene.palette_rgba {
            if colors.len() == 1024 {
                let p = palette.get_or_insert([[0; 4]; 256]);
                for (i, color) in colors.chunks_exact(4).enumerate() {
                    p[i].copy_from_slice(color);
                }
            }
        }
        for over in &scene.palette_overrides {
            if let Some(source) = bank.palette(&over.resource_id) {
                palette.get_or_insert([[0; 4]; 256])[over.index as usize] =
                    source[over.source_index as usize];
            }
        }
        self.layers.clear();
        for (i, sp) in scene.sprites.iter().enumerate() {
            self.layers.push((sp.z, i, Layer::Sprite(i)))
        }
        for (i, text) in scene.texts.iter().enumerate() {
            self.layers
                .push((text.z, scene.sprites.len() + i, Layer::Text(i)))
        }
        for (i, fill) in scene.fills.iter().enumerate() {
            self.layers.push((
                fill.z,
                scene.sprites.len() + scene.texts.len() + i,
                Layer::Fill(i),
            ))
        }
        if scene.video.is_some() {
            self.layers.push((1, usize::MAX - 1, Layer::Video))
        }
        if scene.summary.is_some() {
            self.layers.push((2, usize::MAX, Layer::Summary))
        }
        // Insertion indices are unique across all layer kinds. The complete
        // (z, insertion) key therefore preserves the original stable order.
        self.layers.sort_unstable_by_key(|(z, i, _)| (*z, *i));
        for layer_index in 0..self.layers.len() {
            let layer = self.layers[layer_index].2;
            match layer {
                Layer::Sprite(index) => {
                    let sp = &scene.sprites[index];
                    let Some(image) = bank.image(&sp.resource_id, sp.frame) else {
                        continue;
                    };
                    let own_palette = sp.palette.as_deref().and_then(|id| bank.palette(id));
                    let chosen = own_palette.as_ref().or(palette.as_ref());
                    // FLIC decoding already selected a complete frame. Only
                    // indexed bitmap button strips need vertical frame slicing.
                    let frame_count = if matches!(
                        bank.decoded.get(&sp.resource_id),
                        Some(Decoded::Animation { .. })
                    ) {
                        None
                    } else {
                        sp.frame_count
                    };
                    self.blit_sprite(
                        sp,
                        &image,
                        chosen,
                        sp.color_scale.or(scene.color_scale),
                        scene.offset,
                        frame_count,
                    );
                }
                Layer::Fill(index) => {
                    let fill = &scene.fills[index];
                    let p = fill.palette.as_deref().and_then(|id| bank.palette(id));
                    let color = fill
                        .palette_index
                        .and_then(|i| p.as_ref().or(palette.as_ref()).map(|p| p[i as usize]))
                        .unwrap_or(fill.color);
                    self.fill(fill.bounds, color, scene.offset);
                }
                Layer::Text(index) => {
                    self.text(&scene.texts[index], bank, palette.as_ref(), scene.offset)
                }
                Layer::Video => {
                    let video = scene.video.as_ref().unwrap();
                    if let Some((id, _, image)) = &bank.video {
                        if *id == video.resource_id {
                            let sp = SceneSprite {
                                x: video.bounds.x,
                                y: video.bounds.y,
                                width: Some(video.bounds.width),
                                height: Some(video.bounds.height),
                                transparent: Transparency::Opaque,
                                ..Default::default()
                            };
                            self.sprite(&sp, image, None, None, scene.offset);
                            if video.scanlines {
                                for row in (1..video.bounds.height).step_by(2) {
                                    self.fill(
                                        Rect::new(
                                            video.bounds.x,
                                            video.bounds.y + row as i32,
                                            video.bounds.width,
                                            1,
                                        ),
                                        [0, 0, 0, 255],
                                        scene.offset,
                                    );
                                }
                            }
                        }
                    }
                }
                Layer::Summary => self.summary(
                    scene.summary.as_ref().unwrap(),
                    bank,
                    palette.as_ref(),
                    scene.offset,
                ),
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32 || color[3] == 0 {
            return;
        }
        let i = ((y as usize * WIDTH as usize) + x as usize) * 4;
        if color[3] == 255 {
            self.rgba[i..i + 4].copy_from_slice(&color);
            return;
        }
        let alpha = color[3] as u32;
        for channel in 0..3 {
            self.rgba[i + channel] = ((color[channel] as u32 * alpha
                + self.rgba[i + channel] as u32 * (255 - alpha)
                + 127)
                / 255) as u8;
        }
    }
    fn fill(&mut self, rect: Rect, color: [u8; 4], offset: (i32, i32)) {
        for y in
            (rect.y + offset.1).max(0)..(rect.y + offset.1 + rect.height as i32).min(HEIGHT as i32)
        {
            for x in (rect.x + offset.0).max(0)
                ..(rect.x + offset.0 + rect.width as i32).min(WIDTH as i32)
            {
                self.pixel(x, y, color)
            }
        }
    }
    fn sprite(
        &mut self,
        sp: &SceneSprite,
        image: &PixelImage,
        palette: Option<&[[u8; 4]; 256]>,
        scale: Option<([u8; 2], [f32; 3])>,
        offset: (i32, i32),
    ) {
        self.blit_sprite(sp, image, palette, scale, offset, sp.frame_count);
    }
    fn blit_sprite(
        &mut self,
        sp: &SceneSprite,
        image: &PixelImage,
        palette: Option<&[[u8; 4]; 256]>,
        scale: Option<([u8; 2], [f32; 3])>,
        offset: (i32, i32),
        frame_count: Option<u32>,
    ) {
        let count = frame_count.unwrap_or(1).max(1);
        let mut source = sp
            .source
            .unwrap_or(Rect::new(0, 0, image.width, image.height / count));
        if frame_count.is_some() {
            source.y += (sp.frame % count) as i32 * (image.height / count) as i32;
        }
        let width = sp.width.unwrap_or(source.width);
        let height = sp.height.unwrap_or(source.height);
        if width == 0 || height == 0 || source.width == 0 || source.height == 0 {
            return;
        }
        let x = sp.x + offset.0;
        let y = sp.y + offset.1;
        let clip = sp
            .clip
            .unwrap_or(Rect::new(-offset.0, -offset.1, WIDTH, HEIGHT));
        let remap = if sp.index_map.is_empty() {
            None
        } else {
            let mut table = std::array::from_fn::<_, 256, _>(|index| index as u8);
            for [source, target] in &sp.index_map {
                table[*source as usize] = *target;
            }
            Some(table)
        };
        let key = match sp.transparent {
            Transparency::Opaque => None,
            Transparency::Index(key) => Some(key),
            Transparency::FirstPixel => image
                .indices
                .as_ref()
                .and_then(|v| v.first().copied())
                .map(|index| {
                    remap
                        .as_ref()
                        .map(|table| table[index as usize])
                        .unwrap_or(index)
                }),
        };
        let x_start = 0_i64
            .max(-i64::from(x))
            .max(i64::from(clip.x) + i64::from(offset.0) - i64::from(x))
            .min(i64::from(width)) as u32;
        let x_end = i64::from(width)
            .min(i64::from(WIDTH) - i64::from(x))
            .min(i64::from(clip.x) + i64::from(offset.0) + i64::from(clip.width) - i64::from(x))
            .max(0) as u32;
        let y_start = 0_i64
            .max(-i64::from(y))
            .max(i64::from(clip.y) + i64::from(offset.1) - i64::from(y))
            .min(i64::from(height)) as u32;
        let y_end = i64::from(height)
            .min(i64::from(HEIGHT) - i64::from(y))
            .min(i64::from(clip.y) + i64::from(offset.1) + i64::from(clip.height) - i64::from(y))
            .max(0) as u32;
        if x_start >= x_end || y_start >= y_end {
            return;
        }
        let apply_scale = |mut color: [u8; 4], index: u8| {
            if let Some((range, factor)) = scale {
                if index >= range[0] && index <= range[1] {
                    for channel in 0..3 {
                        color[channel] = (color[channel] as f32 * factor[channel] + 0.0001)
                            .floor()
                            .clamp(0.0, 255.0) as u8;
                    }
                }
            }
            color
        };
        // Palette conversion and its exact f32 rounding depend on the index,
        // not the destination pixel. Preserve source-RGBA fallback for holes.
        let colors = palette.filter(|_| image.indices.is_some()).map(|palette| {
            std::array::from_fn::<_, 256, _>(|raw| {
                let index = remap.as_ref().map_or(raw as u8, |table| table[raw]);
                let color = palette[index as usize];
                (color[3] != 0).then(|| apply_scale(color, index))
            })
        });
        let row_copy = palette.is_none()
            && scale.is_none()
            && sp.index_map.is_empty()
            && sp.transparent == Transparency::Opaque
            && !sp.repeat
            && source.width == width
            && source.height == height;
        for dy in y_start..y_end {
            let yy = y + dy as i32;
            let sy = source.y
                + if sp.repeat {
                    dy % source.height
                } else {
                    (dy as u64 * source.height as u64 / height as u64) as u32
                } as i32;
            if sy < 0 || sy >= image.height as i32 {
                continue;
            }
            if row_copy {
                let first = i64::from(source.x) + i64::from(x_start);
                let last = i64::from(source.x) + i64::from(x_end);
                if first >= 0 && last <= i64::from(image.width) {
                    let from = (sy as usize * image.width as usize + first as usize) * 4;
                    let count = (x_end - x_start) as usize * 4;
                    let source_row = &image.rgba[from..from + count];
                    if source_row.chunks_exact(4).all(|pixel| pixel[3] == 255) {
                        let to = (yy as usize * WIDTH as usize + (x + x_start as i32) as usize) * 4;
                        self.rgba[to..to + count].copy_from_slice(source_row);
                        continue;
                    }
                }
            }
            for dx in x_start..x_end {
                let xx = x + dx as i32;
                let sx = source.x
                    + if sp.repeat {
                        dx % source.width
                    } else {
                        (dx as u64 * source.width as u64 / width as u64) as u32
                    } as i32;
                if sx < 0 || sx >= image.width as i32 {
                    continue;
                }
                let i = sy as usize * image.width as usize + sx as usize;
                let raw_index = image.indices.as_ref().map(|v| v[i]);
                let mut index = image.indices.as_ref().map(|v| v[i]);
                if let Some(index) = index.as_mut() {
                    if let Some(table) = &remap {
                        *index = table[*index as usize];
                    }
                }
                // SetColorKey receives a destination index after the original
                // GDI bitmap conversion, not an index from the source BMP.
                if index.is_some() && index == key {
                    continue;
                }
                let color = raw_index
                    .and_then(|raw| colors.as_ref().and_then(|table| table[raw as usize]))
                    .unwrap_or_else(|| {
                        let color = image.rgba[i * 4..i * 4 + 4].try_into().unwrap();
                        index.map_or(color, |index| apply_scale(color, index))
                    });
                self.pixel(xx, yy, color);
            }
        }
    }
    fn text(
        &mut self,
        text: &SceneText,
        bank: &mut ImageBank,
        default_palette: Option<&[[u8; 4]; 256]>,
        offset: (i32, i32),
    ) {
        let own_palette = text.palette.as_deref().and_then(|id| bank.palette(id));
        let color = text
            .palette_index
            .and_then(|i| {
                own_palette
                    .as_ref()
                    .or(default_palette)
                    .map(|p| p[i as usize])
            })
            .unwrap_or(text.color);
        let ascii = bank.bytes("install/font/ascfont.16");
        let chinese = bank.bytes("install/font/stdfont.16");
        let style = match text.font {
            TextFont::Gdi { size, bold } => bank.glyphs.as_ref().map(|atlas| (atlas, size, bold)),
            _ => None,
        };
        let width_of = |ch: char| -> i32 {
            if let Some((atlas, size, bold)) = style {
                atlas
                    .glyph(ch, size, bold)
                    .map(|g| g.advance())
                    .unwrap_or(0)
            } else if ch.is_ascii() {
                8
            } else {
                16
            }
        };
        let line_height = text.line_height.max(1) as i32;
        let mut y = text.bounds.y;
        for line in text.text.split('\n') {
            let line_width = line
                .chars()
                .map(|ch| width_of(ch) + text.spacing)
                .sum::<i32>()
                .saturating_sub(text.spacing);
            let mut x = text.bounds.x
                + if text.centered {
                    (text.bounds.width as i32 - line_width) / 2
                } else {
                    0
                };
            for ch in line.chars() {
                let advance = width_of(ch);
                if x + advance > text.bounds.x + text.bounds.width as i32 && !text.centered {
                    x = text.bounds.x;
                    y += line_height;
                }
                if let Some((atlas, size, bold)) = style {
                    if let Some(glyph) = atlas.glyph(ch, size, bold) {
                        for gy in 0..glyph.height {
                            for gx in 0..glyph.width() {
                                let alpha = glyph.alpha(gx, gy);
                                let mut c = color;
                                if text.edge_palette {
                                    let row = y + gy as i32 - text.clip.unwrap_or(text.bounds).y;
                                    let slot = if row < 70 {
                                        246 + (row.max(0) * 9 / 69)
                                    } else if row >= 354 {
                                        246 + ((424 - row).max(0) * 9 / 69)
                                    } else {
                                        255
                                    };
                                    if let Some(p) = default_palette {
                                        c = p[slot.clamp(0, 255) as usize];
                                    }
                                    c[3] = if alpha >= 128 { 255 } else { 0 };
                                } else {
                                    c[3] = (alpha as u32 * c[3] as u32 / 255) as u8;
                                }
                                self.text_pixel(text, x + gx as i32, y + gy as i32, c, offset);
                            }
                        }
                    }
                } else {
                    let (font, start, glyph_width, row_bytes) = if ch.is_ascii() {
                        (ascii.as_ref(), (ch as usize).clamp(32, 127) * 16, 8, 1)
                    } else {
                        let mut encoded = [0u8; 4];
                        let encoded = ch.encode_utf8(&mut encoded);
                        let (encoded, _, bad) = GBK.encode(encoded);
                        if bad || encoded.len() != 2 || encoded[0] < 0xa1 || encoded[1] < 0xa1 {
                            (None, 0, 16, 2)
                        } else {
                            (
                                chinese.as_ref(),
                                ((encoded[0] as usize - 0xa1) * 94 + encoded[1] as usize - 0xa1)
                                    * 32,
                                16,
                                2,
                            )
                        }
                    };
                    if let Some(font) = font {
                        for gy in 0..16 {
                            for gx in 0..glyph_width {
                                if font
                                    .get(start + gy * row_bytes + gx / 8)
                                    .is_some_and(|b| b & (128 >> (gx % 8)) != 0)
                                {
                                    self.text_pixel(
                                        text,
                                        x + gx as i32,
                                        y + gy as i32,
                                        color,
                                        offset,
                                    )
                                }
                            }
                        }
                    }
                }
                x += advance + text.spacing;
            }
            y += line_height;
        }
    }
    fn text_pixel(&mut self, text: &SceneText, x: i32, y: i32, color: [u8; 4], offset: (i32, i32)) {
        let clip = text.clip.unwrap_or(text.bounds);
        if x >= clip.x
            && y >= clip.y
            && x < clip.x + clip.width as i32
            && y < clip.y + clip.height as i32
        {
            self.pixel(x + offset.0, y + offset.1, color)
        }
    }
    fn summary(
        &mut self,
        summary: &SceneSummary,
        bank: &mut ImageBank,
        palette: Option<&[[u8; 4]; 256]>,
        offset: (i32, i32),
    ) {
        let Some(actor) = bank.image(&summary.actor_resource, summary.actor_frame) else {
            return;
        };
        let Some(portrait) = bank.image(&summary.portrait_resource, 0) else {
            return;
        };
        let (Some(actor_indices), Some(portrait_indices)) = (&actor.indices, &portrait.indices)
        else {
            return;
        };
        if summary.original_palette.len() != 256 {
            return;
        }
        let remaps = bank
            .remaps
            .entry(summary.original_palette.clone())
            .or_insert_with(|| {
                Arc::new(
                    (0..70)
                        .map(|row| {
                            let mut remap = [0u8; 256];
                            for (color, value) in
                                summary.original_palette.iter().enumerate().skip(1)
                            {
                                let target = value.map(|v| i32::from(v) * row / 70);
                                let mut distance = i32::MAX;
                                for (index, candidate) in
                                    summary.original_palette.iter().enumerate()
                                {
                                    let square = (0..3)
                                        .map(|c| (i32::from(candidate[c]) - target[c]).pow(2))
                                        .sum();
                                    if square < distance {
                                        distance = square;
                                        remap[color] = index as u8;
                                    }
                                }
                            }
                            remap
                        })
                        .collect(),
                )
            })
            .clone();
        let mut indices = actor_indices.clone();
        let mut rgba = vec![0; indices.len() * 4];
        for y in 0..actor.height {
            let row = summary.actor_y + y as i32 - summary.clip.y;
            let remap_row = if row < 70 {
                row
            } else if row >= 354 {
                424 - row
            } else {
                70
            };
            for x in 0..actor.width {
                let i = (y * actor.width + x) as usize;
                let mut color = actor_indices[i];
                if x < portrait.width && y < portrait.height {
                    let p = (y * portrait.width + x) as usize;
                    if portrait_indices[p] != 1 {
                        color = portrait_indices[p];
                    }
                }
                if color != 0 && (0..70).contains(&remap_row) {
                    color = remaps[remap_row as usize][color as usize];
                }
                indices[i] = color;
                rgba[i * 4 + 3] = 255;
            }
        }
        let composite = PixelImage {
            width: actor.width,
            height: actor.height,
            rgba,
            indices: Some(indices),
        };
        let sprite = SceneSprite {
            x: 110,
            y: summary.actor_y,
            clip: Some(summary.clip),
            transparent: Transparency::Opaque,
            ..Default::default()
        };
        self.sprite(&sprite, &composite, palette, None, offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("../tests/support/reference_sprite.rs");

    #[test]
    fn optimized_blits_match_the_scalar_renderer_for_clipping_palette_and_alpha() {
        let mut palette = [[0, 0, 0, 255]; 256];
        for (index, color) in palette.iter_mut().enumerate() {
            *color = [
                index as u8,
                (index as u8).wrapping_mul(17),
                255 - index as u8,
                if index % 11 == 0 {
                    0
                } else if index % 7 == 0 {
                    128
                } else {
                    255
                },
            ];
        }
        for case in 0..256_u32 {
            let image = PixelImage {
                width: 13,
                height: 12,
                rgba: (0..156_u32)
                    .flat_map(|pixel| {
                        [
                            pixel as u8,
                            (pixel * 19) as u8,
                            (pixel * 47) as u8,
                            if case & 128 == 0 {
                                255
                            } else {
                                [0, 65, 128, 254, 255][pixel as usize % 5]
                            },
                        ]
                    })
                    .collect(),
                indices: (case & 64 != 0)
                    .then(|| (0..156_u32).map(|pixel| (pixel * 37) as u8).collect()),
            };
            let sp = SceneSprite {
                x: [-8, 3, 635, -25][case as usize % 4],
                y: [-4, 4, 475, 19][case as usize / 4 % 4],
                width: Some([3, 13, 27, 13][case as usize / 16 % 4]),
                height: Some([2, 12, 23, 12][case as usize / 64 % 4]),
                transparent: match case % 4 {
                    0 => Transparency::Opaque,
                    1 => Transparency::FirstPixel,
                    2 => Transparency::Index(1),
                    _ => Transparency::Index(33),
                },
                source: (case % 5 == 0).then(|| Rect::new(-2, 1, 8, 9)),
                frame_count: (case % 5 == 1).then_some(3),
                frame: case % 7,
                clip: (case % 3 == 0).then(|| Rect::new(2, 1, 636, 477)),
                repeat: case & 32 != 0,
                index_map: if case & 8 != 0 {
                    vec![[0, 33], [37, 1], [74, 33], [74, 127]]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let offset = if case % 2 == 0 { (-3, 2) } else { (1, -1) };
            let colors = (case & 4 != 0).then_some(&palette);
            let scale = (case & 16 != 0).then_some(([12, 180], [0.25, 0.999, 1.2]));
            let mut actual = PixelCanvas::default();
            for pixel in actual.rgba.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[73, 24, 155, 37]);
            }
            let mut expected = PixelCanvas::default();
            expected.rgba.clone_from(&actual.rgba);
            actual.sprite(&sp, &image, colors, scale, offset);
            scalar_reference_sprite(&mut expected, &sp, &image, colors, scale, offset);
            if let Some(index) = actual
                .rgba
                .iter()
                .zip(&expected.rgba)
                .position(|(a, b)| a != b)
            {
                panic!(
                    "case {case}, pixel ({}, {}), channel {}: actual {}, expected {}",
                    index / 4 % WIDTH as usize,
                    index / 4 / WIDTH as usize,
                    index % 4,
                    actual.rgba[index],
                    expected.rgba[index]
                );
            }
        }
    }

    #[test]
    fn reusable_layer_order_keeps_cross_type_ties_and_drops_removed_layers() {
        let mut bank = ImageBank::default();
        bank.decoded.insert(
            "red".into(),
            Decoded::Bitmap(Arc::new(PixelImage {
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 255, 0, 0, 255],
                indices: None,
            })),
        );
        let mut scene = SceneFrame {
            sprites: vec![SceneSprite {
                resource_id: "red".into(),
                z: 4,
                transparent: Transparency::Opaque,
                ..Default::default()
            }],
            fills: vec![
                SceneFill {
                    bounds: Rect::new(0, 0, 1, 1),
                    color: [0, 255, 0, 255],
                    z: 4,
                    ..Default::default()
                },
                SceneFill {
                    bounds: Rect::new(1, 0, 1, 1),
                    color: [0, 0, 255, 255],
                    z: 4,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut canvas = PixelCanvas::default();
        canvas.compose(&scene, &mut bank);
        assert_eq!(&canvas.rgba[..8], &[0, 255, 0, 255, 0, 0, 255, 255]);
        scene.fills.pop();
        canvas.compose(&scene, &mut bank);
        assert_eq!(&canvas.rgba[..8], &[0, 255, 0, 255, 255, 0, 0, 255]);
        canvas.compose(&SceneFrame::default(), &mut bank);
        assert!(
            canvas
                .rgba
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 0, 255])
        );
    }
    #[test]
    fn bitmap_gdi_remapping_precedes_the_destination_colorkey() {
        let image = PixelImage {
            width: 3,
            height: 1,
            rgba: vec![0; 12],
            indices: Some(vec![1, 2, 3]),
        };
        let mut palette = [[0, 0, 0, 255]; 256];
        palette[5] = [100, 200, 50, 255];
        palette[1] = [70, 80, 90, 255];
        let mut sp = SceneSprite {
            transparent: Transparency::Index(1),
            index_map: vec![[1, 5], [2, 1], [3, 5]],
            ..Default::default()
        };
        let mut canvas = PixelCanvas::default();
        canvas.sprite(&sp, &image, Some(&palette), None, (0, 0));
        assert_eq!(
            &canvas.rgba[..12],
            &[100, 200, 50, 255, 0, 0, 0, 0, 100, 200, 50, 255]
        );
        sp.transparent = Transparency::FirstPixel;
        let mut canvas = PixelCanvas::default();
        canvas.sprite(&sp, &image, Some(&palette), None, (0, 0));
        assert_eq!(
            &canvas.rgba[..12],
            &[0, 0, 0, 0, 70, 80, 90, 255, 0, 0, 0, 0]
        );
    }
    #[test]
    fn pcx_palette_keeps_entries_not_used_by_the_background_pixels() {
        let mut bytes = vec![0u8; 128];
        bytes[0] = 10;
        bytes[2] = 1;
        bytes[3] = 8;
        bytes[65] = 1;
        bytes[66] = 2;
        bytes.extend_from_slice(&[1, 0, 12]);
        let mut colors = vec![0; 768];
        colors[3..6].copy_from_slice(&[12, 34, 56]);
        colors[9..12].copy_from_slice(&[91, 82, 73]);
        bytes.extend(colors);
        let mut bank = ImageBank::default();
        bank.insert("pcx".into(), bytes);
        assert_eq!(bank.palette("pcx").unwrap()[3], [91, 82, 73, 255]);
    }
    #[test]
    fn flic_animation_counts_do_not_crop_an_already_decoded_frame() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("assets/manifest.json")).unwrap(),
        )
        .unwrap();
        let id = "install/ani/oka6.flc";
        let hash = &manifest["resources"][id]["sha256"];
        let object = manifest["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["sha256"] == *hash)
            .unwrap();
        let mut bank = ImageBank::default();
        bank.insert(
            id.into(),
            std::fs::read(root.join(object["path"].as_str().unwrap())).unwrap(),
        );
        let decoded = bank.image(id, 2).unwrap();
        let scene = SceneFrame {
            sprites: vec![SceneSprite {
                resource_id: id.into(),
                width: Some(decoded.width),
                height: Some(decoded.height),
                frame: 2,
                frame_count: Some(5),
                transparent: Transparency::Opaque,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut canvas = PixelCanvas::default();
        canvas.compose(&scene, &mut bank);
        for y in 0..decoded.height as usize {
            assert_eq!(
                &canvas.rgba
                    [y * WIDTH as usize * 4..y * WIDTH as usize * 4 + decoded.width as usize * 4],
                &decoded.rgba[y * decoded.width as usize * 4..(y + 1) * decoded.width as usize * 4]
            );
        }
    }
    #[test]
    fn transparency_palette_and_integer_scale_are_exact() {
        let image = PixelImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
            indices: Some(vec![1, 2]),
        };
        let mut canvas = PixelCanvas::default();
        let mut palette = [[0, 0, 0, 255]; 256];
        palette[2] = [101, 51, 255, 255];
        let sp = SceneSprite {
            transparent: Transparency::Index(1),
            ..Default::default()
        };
        canvas.sprite(
            &sp,
            &image,
            Some(&palette),
            Some(([2, 2], [0.5, 0.5, 0.5])),
            (0, 0),
        );
        assert_eq!(&canvas.rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&canvas.rgba[4..8], &[50, 25, 127, 255]);
    }
    #[test]
    fn source_strip_clip_and_repeat_preserve_original_pixels() {
        let image = PixelImage {
            width: 2,
            height: 2,
            rgba: vec![10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255],
            indices: None,
        };
        let mut canvas = PixelCanvas::default();
        let sp = SceneSprite {
            width: Some(5),
            height: Some(1),
            frame_count: Some(2),
            frame: 1,
            repeat: true,
            clip: Some(Rect::new(1, 0, 3, 1)),
            ..Default::default()
        };
        canvas.sprite(&sp, &image, None, None, (0, 0));
        assert_eq!(
            (0..5).map(|x| canvas.rgba[x * 4]).collect::<Vec<_>>(),
            [0, 40, 30, 40, 0]
        );
    }
}
