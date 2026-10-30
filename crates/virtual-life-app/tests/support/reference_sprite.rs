// Test-only scalar oracle captured before the 2026-10-30 render optimization.
fn scalar_reference_sprite(
        canvas: &mut PixelCanvas,
        sp: &SceneSprite,
        image: &PixelImage,
        palette: Option<&[[u8; 4]; 256]>,
        scale: Option<([u8; 2], [f32; 3])>,
        offset: (i32, i32),
    ) {
        let count = sp.frame_count.unwrap_or(1).max(1);
        let mut source = sp
            .source
            .unwrap_or(Rect::new(0, 0, image.width, image.height / count));
        if sp.frame_count.is_some() {
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
        for dy in 0..height {
            let yy = y + dy as i32;
            if yy < 0
                || yy >= HEIGHT as i32
                || yy < clip.y + offset.1
                || yy >= clip.y + offset.1 + clip.height as i32
            {
                continue;
            }
            let sy = source.y
                + if sp.repeat {
                    dy % source.height
                } else {
                    (dy as u64 * source.height as u64 / height as u64) as u32
                } as i32;
            if sy < 0 || sy >= image.height as i32 {
                continue;
            }
            for dx in 0..width {
                let xx = x + dx as i32;
                if xx < 0
                    || xx >= WIDTH as i32
                    || xx < clip.x + offset.0
                    || xx >= clip.x + offset.0 + clip.width as i32
                {
                    continue;
                }
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
                let mut color: [u8; 4] = image.rgba[i * 4..i * 4 + 4].try_into().unwrap();
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
                if let (Some(p), Some(index)) = (palette, index) {
                    if p[index as usize][3] != 0 {
                        color = p[index as usize];
                    }
                }
                if let (Some((range, factor)), Some(index)) = (scale, index) {
                    if index >= range[0] && index <= range[1] {
                        for c in 0..3 {
                            color[c] = (color[c] as f32 * factor[c] + 0.0001)
                                .floor()
                                .clamp(0.0, 255.0) as u8;
                        }
                    }
                }
                canvas.pixel(xx, yy, color);
            }
        }
    }

