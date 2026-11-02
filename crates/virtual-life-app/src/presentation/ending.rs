//! Evaluation scroll, timed movies, credits and original reincarnation controls.
use super::*;

fn color(value: &str) -> [u8; 4] {
    let hex = value.trim_start_matches('#');
    if hex.len() == 6 {
        if let Ok(value) = u32::from_str_radix(hex, 16) {
            return [(value >> 16) as u8, (value >> 8) as u8, value as u8, 255];
        }
    }
    [255, 255, 255, 255]
}
fn clip(value: &Value) -> Rect {
    Rect::new(
        n(value, "/0"),
        n(value, "/1"),
        (n(value, "/2") - n(value, "/0")).max(0) as u32,
        (n(value, "/3") - n(value, "/1")).max(0) as u32,
    )
}
fn timed_sound(f: &mut SceneFrame, id: &str, resource: &str, time_ms: u64, looped: bool) {
    sound(f, id, resource, looped, 0);
    if let Some(last) = f.sounds.last_mut() {
        if last.id == id {
            last.position_ms = Some(time_ms);
        }
    }
}
fn key_release(f: &mut SceneFrame, id: &str, label: &str, keys: &[&str], down: &str, up: &str) {
    control(
        f,
        id,
        label,
        Rect::new(-1, -1, 0, 0),
        game("endingAction", down, None),
        keys,
    );
    f.controls.last_mut().unwrap().on_release = Some(game("endingAction", up, None));
}

pub(super) fn compose(
    f: &mut SceneFrame,
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &PresentationState,
) {
    let st = &snapshot["pending"]["state"];
    let rules = &c.rules["epilogue"];
    match s(st, "/phase") {
        "summary" => {
            summary(f, &st["summary"], &c.rules["endingSummary"]);
            control(
                f,
                "ending-summary-continue",
                "继续结局",
                Rect::new(0, 0, 640, 480),
                game("endingAction", "continue", None),
                &[],
            );
            key_release(
                f,
                "ending-summary-speed",
                "加速人生评价",
                &["KeyS"],
                "sDown",
                "sUp",
            );
            control(
                f,
                "ending-summary-skip",
                "跳过人生评价",
                Rect::new(-1, -1, 0, 0),
                game("endingAction", "escape", None),
                &["Escape:up"],
            );
            timed_sound(
                f,
                "ending-summary-music",
                s(&c.rules["endingSummary"], "/music"),
                n(&st["summary"], "/clockMs").max(0) as u64,
                true,
            );
        }
        "movie" => movie(f, st, rules),
        "credits" => credits(f, &st["credits"], rules),
        "endImage" => {
            let image_meta = &rules["credits"]["end"];
            if let Some(index) = image(f, "ending-end-image", image_meta, 0) {
                f.sprites[index].x = (640 - n(image_meta, "/width")) / 2;
                f.sprites[index].y = (480 - n(image_meta, "/height")) / 2;
            }
            control(
                f,
                "ending-end-continue",
                "继续",
                Rect::new(0, 0, 640, 480),
                game("endingAction", "continue", None),
                &["Enter", "Space", "Escape"],
            );
        }
        "reincarnation" => reincarnation(f, &rules["choice"], ui),
        _ => {}
    }
    if !["endImage", "reincarnation", "finished"].contains(&s(st, "/phase")) {
        f.tick = Some(json!({"type":"endingTick"}));
    }
}

fn summary(f: &mut SceneFrame, st: &Value, rules: &Value) {
    let index = sprite(
        f,
        "ending-summary-background",
        s(st, "/background"),
        0,
        0,
        0,
    );
    f.sprites[index].transparent = Transparency::Opaque;
    f.palette_rgba = Some(
        a(st, "/palette")
            .iter()
            .flat_map(|row| {
                [
                    n(row, "/0") as u8,
                    n(row, "/1") as u8,
                    n(row, "/2") as u8,
                    255,
                ]
            })
            .collect(),
    );
    let bounds = clip(&rules["clip"]);
    if b(st, "/showActor") {
        f.summary = Some(SceneSummary {
            actor_resource: s(st, "/actor/resource").into(),
            actor_frame: n(st, "/actor/frame").max(0) as u32,
            portrait_resource: s(st, "/portrait").into(),
            actor_y: n(st, "/actorY"),
            clip: bounds,
            original_palette: a(st, "/originalPalette")
                .iter()
                .map(|row| [n(row, "/0") as u8, n(row, "/1") as u8, n(row, "/2") as u8])
                .collect(),
        });
    }
    if b(st, "/showText") {
        for (index, paragraph) in a(st, "/paragraphs").iter().enumerate() {
            let y = n(paragraph, "/y") + n(st, "/scrollY");
            let height = n(paragraph, "/height").max(0) as u32;
            if y >= bounds.y + bounds.height as i32 || y + height as i32 <= bounds.y {
                continue;
            }
            // Original line wrapping can split a GBK glyph; decode the stored
            // wrapped bytes, not the convenient unwrapped text annotation.
            let bytes: Vec<u8> = a(paragraph, "/bytes")
                .iter()
                .map(|value| n(value, "") as u8)
                .collect();
            let (decoded, _, _) = encoding_rs::GBK.decode(&bytes);
            text(
                f,
                &format!("ending-summary-paragraph-{index}"),
                decoded.into_owned(),
                Rect::new(n(paragraph, "/x"), y, bounds.width, height),
                [255, 255, 255, 255],
            );
            let text = f.texts.last_mut().unwrap();
            text.font = TextFont::Gdi {
                size: n(rules, "/fontSize").max(1) as u32,
                bold: false,
            };
            text.line_height = n(rules, "/lineHeight").max(1) as u32;
            text.spacing = n(rules, "/characterSpacing");
            text.clip = Some(bounds);
            text.palette_index = Some(255);
            text.edge_palette = true;
            text.z = 1;
        }
    }
}

pub(super) fn movie(f: &mut SceneFrame, st: &Value, rules: &Value) {
    let movie = &st["movie"];
    let video = &rules["video"];
    f.video = Some(SceneVideo {
        resource_id: s(
            rules,
            if s(movie, "/phase") == "subtitles" {
                "/movie"
            } else if !b(st, "/victory") {
                "/lossOutro"
            } else {
                "/outro"
            },
        )
        .into(),
        time_ms: n(movie, "/displayedVideoTimeMs").max(0) as u64,
        fps: 24,
        bounds: Rect::new(
            n(video, "/x"),
            n(video, "/y"),
            n(video, "/width").max(0) as u32,
            n(video, "/height").max(0) as u32,
        ),
        scanlines: b(video, "/scanlines"),
    });
    if s(movie, "/phase") == "subtitles" {
        if let Some(index) = movie["displayedSubtitle"].as_u64() {
            let caption = &rules["subtitles"][usize::from(b(st, "/victory"))][index as usize];
            let segment = &rules["segments"][n(caption, "/segment").max(0) as usize];
            text(
                f,
                "ending-subtitle",
                s(caption, "/text"),
                clip(&rules["subtitleRect"]),
                color(s(segment, "/color")),
            );
            let text = f.texts.last_mut().unwrap();
            text.font = TextFont::Gdi {
                size: n(rules, "/fontSize").max(1) as u32,
                bold: true,
            };
            text.spacing = n(rules, "/characterSpacing");
            text.line_height = n(rules, "/fontSize").max(1) as u32;
            text.clip = Some(text.bounds);
        }
    }
    timed_sound(
        f,
        "ending-movie-music",
        s(rules, "/music"),
        n(movie, "/clockMs").max(0) as u64,
        true,
    );
    if b(movie, "/narrationStarted") {
        timed_sound(
            f,
            "ending-narration",
            s(&rules["narration"][usize::from(b(st, "/victory"))], ""),
            n(movie, "/clockMs")
                .saturating_sub(n(movie, "/narrationStartedMs"))
                .max(0) as u64,
            false,
        );
    }
    control(
        f,
        "ending-movie-skip",
        "跳过结局影片",
        Rect::new(-1, -1, 0, 0),
        game("endingAction", "escape", None),
        &["Escape:up"],
    );
}

fn credits(f: &mut SceneFrame, st: &Value, rules: &Value) {
    let credits = &rules["credits"];
    let bounds = clip(&credits["rect"]);
    let brightness = n(st, "/brightness").clamp(0, 224) as u8;
    let rgba = [brightness, brightness, brightness, 255];
    for (index, paragraph) in a(credits, "/paragraphs").iter().enumerate() {
        let y = bounds.y + n(paragraph, "/y") + n(st, "/offset");
        let height = n(paragraph, "/height").max(0) as u32;
        if y >= bounds.y + bounds.height as i32 || y + height as i32 <= bounds.y {
            continue;
        }
        text(
            f,
            &format!("ending-credit-{index}"),
            s(paragraph, "/text"),
            Rect::new(bounds.x, y, bounds.width, height),
            rgba,
        );
        let text = f.texts.last_mut().unwrap();
        text.font = TextFont::Gdi {
            size: n(credits, "/fontSize").max(1) as u32,
            bold: true,
        };
        text.spacing = n(credits, "/characterSpacing");
        text.line_height = height / s(paragraph, "/text").lines().count().max(1) as u32;
        text.centered = true;
        text.clip = Some(bounds);
    }
    key_release(
        f,
        "ending-credits-speed",
        "加速制作人员名单",
        &["KeyS"],
        "sDown",
        "sUp",
    );
    control(
        f,
        "ending-credits-skip",
        "跳过制作人员名单",
        Rect::new(-1, -1, 0, 0),
        game("endingAction", "escape", None),
        &["Escape:up"],
    );
}

fn reincarnation(f: &mut SceneFrame, rules: &Value, ui: &PresentationState) {
    // 0041c040 constructs end_mouse before entering its input loop. Keep it
    // resident even before the first mouse move places the cursor on screen;
    // loading it during that press would clear the captured choice input.
    if let Some(resource) = rules["cursor"]["resource"].as_str() {
        f.required_resources.push(resource.into());
    }
    image(f, "ending-choice-background", &rules["background"], 0);
    for (index, meta) in a(rules, "/buttons").iter().enumerate() {
        let mut meta = meta.clone();
        meta["transparent"] = json!(1);
        let id = format!("ending-choice-{index}");
        button(
            f,
            &id,
            &meta,
            game("endingAction", "choose", Some(index)),
            if index == 0 {
                "再次轮回"
            } else {
                "结束人生"
            },
            1,
            ui,
        );
        f.controls
            .last_mut()
            .unwrap()
            .keys
            .push(if index == 0 { "KeyY" } else { "KeyN" }.into());
    }
    {
        if let Some(index) = image(f, "ending-cursor", &rules["cursor"], 10) {
            let sprite = &mut f.sprites[index];
            sprite.x = ui.pointer.0;
            sprite.y = ui.pointer.1;
            sprite.transparent = Transparency::Index(1);
            sprite.frame_count = Some(2);
            sprite.height = sprite.height.map(|height| height / 2);
            sprite.frame = 0;
            sprite.playback = Some(Playback {
                interval_ms: 250,
                once: false,
                max_advances: 1,
                immediate: true,
            });
            if !(0..640).contains(&ui.pointer.0) || !(0..480).contains(&ui.pointer.1) {
                sprite.x = -1000;
                sprite.y = -1000;
            }
            f.requests.push(SceneRequest::CursorHidden(true));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reincarnation_loads_its_original_cursor_before_the_first_pointer_input() {
        let rules: Value =
            serde_json::from_str(include_str!("../../../../assets/catalog/rules.json")).unwrap();
        let mut frame = SceneFrame::default();
        reincarnation(
            &mut frame,
            &rules["epilogue"]["choice"],
            &PresentationState::default(),
        );
        assert!(
            frame
                .required_resources
                .contains(&"install/bmp/end_mouse.bmp".into())
        );
        let cursor = frame
            .sprites
            .iter()
            .find(|sprite| sprite.id == "ending-cursor")
            .unwrap();
        assert_eq!((cursor.x, cursor.y), (-1000, -1000));
        assert_eq!(
            frame
                .controls
                .iter()
                .find(|control| control.id == "ending-choice-1")
                .unwrap()
                .action,
            game("endingAction", "choose", Some(1))
        );
    }
    #[test]
    fn movie_frame_uses_saved_display_time_and_native_scanlines() {
        let mut frame = SceneFrame::default();
        let rules = json!({"movie":"movie.webm","outro":"outro.webm","lossOutro":"loss.webm","music":"music.wav","video":{"x":0,"y":60,"width":640,"height":360,"scanlines":true}});
        let state = json!({"phase":"movie","victory":true,"movie":{"phase":"outro","clockMs":4000,"displayedVideoTimeMs":1234}});
        movie(&mut frame, &state, &rules);
        let video = frame.video.unwrap();
        assert_eq!(video.resource_id, "outro.webm");
        assert_eq!(video.time_ms, 1234);
        assert_eq!(video.bounds, Rect::new(0, 60, 640, 360));
        assert!(video.scanlines);
        let mut loss = state.clone();
        loss["victory"] = json!(false);
        let mut frame = SceneFrame::default();
        movie(&mut frame, &loss, &rules);
        assert_eq!(frame.video.unwrap().resource_id, "loss.webm");
    }
    #[test]
    fn summary_uses_wrapped_gbk_and_dynamic_palette() {
        let mut frame = SceneFrame::default();
        summary(
            &mut frame,
            &json!({"background":"summary.bmp","showText":true,"scrollY":2,"palette":[[12,13,14]],"paragraphs":[{"text":"ignored annotation","bytes":[65,10,66],"x":110,"y":20,"height":40}]}),
            &json!({"clip":[100,10,500,450],"fontSize":16,"lineHeight":20}),
        );
        assert_eq!(frame.palette_rgba, Some(vec![12, 13, 14, 255]));
        assert_eq!(frame.texts[0].text, "A\nB");
        assert_eq!(frame.texts[0].bounds.y, 22);
        assert!(frame.texts[0].edge_palette);
    }
}
