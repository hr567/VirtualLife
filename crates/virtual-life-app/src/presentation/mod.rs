//! Original scene composition. All positions are logical original pixels.
use crate::scene::*;
use serde_json::{Value, json};
mod battle;
mod clocks;
mod competition;
mod ending;
mod map;
mod minigames;
mod native_allocation;
mod native_fortune;
mod native_housing;
mod native_panels;
mod native_school;
mod panels;
mod preload;
mod shell;
#[cfg(test)]
mod tests;
pub use clocks::accept_response;
pub fn update_presentation(
    c: &SceneContent<'_>,
    snapshot: &Value,
    ui: &mut PresentationState,
    elapsed_ms: u32,
) {
    native_panels::update(c, snapshot, ui, elapsed_ms);
    native_housing::update(c, snapshot, ui, elapsed_ms);
    native_fortune::update(c, snapshot, ui, elapsed_ms);
    native_school::update(c, snapshot, ui, elapsed_ms);
    native_allocation::update(c, snapshot, ui, elapsed_ms);
    clocks::update_presentation(c, snapshot, ui, elapsed_ms);
}

pub fn compose(content: &SceneContent<'_>, snapshot: &Value, ui: &PresentationState) -> SceneFrame {
    let mut frame = SceneFrame::default();
    if ui.screen == Screen::Game {
        if let Some(motion) = &ui.runtime.motion {
            if let Some(pose) = motion.frames.get(motion.cursor) {
                let mut state = motion.snapshot.clone();
                if let Some((x, y)) = pose.actor_position {
                    state["_actorPosition"] = json!([x, y]);
                }
                state["_actorResource"] = pose
                    .actor_resource
                    .as_ref()
                    .map_or(Value::Null, |v| json!(v));
                state["_actorFrame"] = json!(pose.actor_frame);
                state["mapFacing"] = json!(pose.facing);
                map::compose(&mut frame, content, &state, ui);
                if let Some((resource, number)) = &pose.die {
                    let index = sprite(&mut frame, "dice-result", resource, 270, 48, 150);
                    frame.sprites[index].frame = *number;
                    frame.sprites[index].transparent = Transparency::FirstPixel;
                }
                for control in &mut frame.controls {
                    control.enabled = false;
                }
                frame.key = "map-motion".into();
                return frame;
            }
        }
    }
    frame.key = format!(
        "{:?}:{}:{}:{}",
        ui.screen,
        s(snapshot, "/pending/kind"),
        s(snapshot, "/pending/game"),
        s(snapshot, "/pending/state/phase")
    );
    match ui.screen {
        Screen::Menu => shell::menu(&mut frame, ui),
        Screen::Profile => shell::profile(&mut frame, content, ui),
        Screen::Options => shell::options(&mut frame, content, ui),
        Screen::Saves => shell::saves(&mut frame, ui),
        Screen::MapOverview => map::overview(&mut frame, content, snapshot, ui),
        Screen::Game => {
            if native_school::compose(&mut frame, content, snapshot, ui)
                || native_fortune::compose(&mut frame, content, snapshot, ui)
                || native_housing::compose(&mut frame, content, snapshot, ui)
                || native_panels::compose(&mut frame, content, snapshot, ui)
            {
            } else {
                match s(snapshot, "/pending/kind") {
                    "minigame" => minigames::compose(&mut frame, content, snapshot, ui),
                    "competition" => competition::compose(&mut frame, content, snapshot, ui),
                    "battle" => battle::compose(&mut frame, content, snapshot, ui),
                    "ending" => ending::compose(&mut frame, content, snapshot, ui),
                    "inventory" | "status" | "relationship" => {
                        panels::compose(&mut frame, content, snapshot, ui)
                    }
                    "dialogue" | "choice" => map::event(&mut frame, content, snapshot, ui),
                    _ => map::compose(&mut frame, content, snapshot, ui),
                }
            }
        }
    }
    for sp in &mut frame.sprites {
        if let Some(playback) = &sp.playback {
            if let Some(clock) = ui.runtime.animations.get(&sp.id) {
                if clock.resource == sp.resource_id {
                    sp.frame = clock.frame;
                }
            } else {
                sp.frame = 0;
            }
            if playback.once {
                let frames = n(&content.media(&sp.resource_id), "/frameCount").max(1) as u32;
                sp.frame = sp.frame.min(frames - 1);
            }
        }
    }
    frame.sounds.extend(ui.runtime.one_shots.iter().cloned());
    frame
        .requests
        .push(SceneRequest::Prefetch(ui.runtime.preload_resources.clone()));
    frame.description = s(snapshot, "/message").to_owned();
    frame
}
pub(super) fn s<'a>(v: &'a Value, path: &str) -> &'a str {
    v.pointer(path).and_then(Value::as_str).unwrap_or("")
}
pub(super) fn n(v: &Value, path: &str) -> i32 {
    v.pointer(path).and_then(Value::as_f64).unwrap_or(0.0) as i32
}
pub(super) fn b(v: &Value, path: &str) -> bool {
    v.pointer(path).and_then(Value::as_bool).unwrap_or(false)
}
pub(super) fn a<'a>(v: &'a Value, path: &str) -> &'a [Value] {
    v.pointer(path)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
pub(super) fn game(kind: &str, action: &str, index: Option<usize>) -> Action {
    let mut v = json!({"type":kind,"action":action});
    if let Some(index) = index {
        v["index"] = json!(index)
    }
    Action::Game(v)
}
pub(super) fn sprite(
    f: &mut SceneFrame,
    id: &str,
    resource: impl Into<String>,
    x: i32,
    y: i32,
    z: i32,
) -> usize {
    let index = f.sprites.len();
    f.sprites.push(SceneSprite {
        id: id.to_owned(),
        resource_id: resource.into(),
        x,
        y,
        z,
        ..Default::default()
    });
    index
}
pub(super) fn image(f: &mut SceneFrame, id: &str, v: &Value, z: i32) -> Option<usize> {
    let resource = s(v, "/resource");
    if resource.is_empty() {
        return None;
    }
    let index = sprite(f, id, resource, n(v, "/x"), n(v, "/y"), z);
    let sp = &mut f.sprites[index];
    sp.width = v["width"].as_u64().map(|n| n as u32);
    sp.height = v["height"].as_u64().map(|n| n as u32);
    sp.transparent = match v.get("transparent") {
        Some(Value::Null) => Transparency::Opaque,
        Some(Value::String(value)) if value == "firstPixel" => Transparency::FirstPixel,
        Some(Value::Number(value)) => Transparency::Index(value.as_u64().unwrap_or(1) as u8),
        Some(Value::Bool(false)) => Transparency::Opaque,
        _ => Transparency::Index(1),
    };
    Some(index)
}
pub(super) fn animation(f: &mut SceneFrame, id: &str, v: &Value, z: i32) -> Option<usize> {
    let index = image(f, id, v, z)?;
    let sp = &mut f.sprites[index];
    sp.frame = n(v, "/frame").max(0) as u32;
    Some(index)
}
pub(super) fn control(
    f: &mut SceneFrame,
    id: &str,
    label: &str,
    rect: Rect,
    action: Action,
    keys: &[&str],
) {
    f.controls.push(SceneControl {
        id: id.to_owned(),
        label: label.to_owned(),
        bounds: rect,
        action,
        enabled: true,
        z: 1000,
        keys: keys.iter().map(|s| (*s).to_owned()).collect(),
        on_press: None,
        on_release: None,
        on_hover: None,
        on_leave: None,
    })
}
pub(super) fn text(
    f: &mut SceneFrame,
    id: &str,
    value: impl Into<String>,
    rect: Rect,
    color: [u8; 4],
) {
    f.texts.push(SceneText {
        id: id.to_owned(),
        text: value.into(),
        bounds: rect,
        color,
        z: 900,
        line_height: 16,
        font: TextFont::Gdi {
            size: 16,
            bold: false,
        },
        ..Default::default()
    })
}
pub(super) fn sound(
    f: &mut SceneFrame,
    id: &str,
    resource: impl Into<String>,
    looped: bool,
    serial: u64,
) {
    let resource = resource.into();
    if !resource.is_empty() {
        f.sounds.push(SceneSound {
            id: id.to_owned(),
            resource_id: resource,
            looped,
            restart_serial: serial,
            ..Default::default()
        })
    }
}
pub(super) fn button(
    f: &mut SceneFrame,
    id: &str,
    v: &Value,
    action: Action,
    label: &str,
    z: i32,
    ui: &PresentationState,
) {
    if let Some(index) = image(f, id, v, z) {
        let count = 3;
        let sp = &mut f.sprites[index];
        sp.frame_count = Some(count);
        let h = sp.height.unwrap_or(75) / count;
        sp.height = Some(h);
        sp.frame = if ui.pressed.as_deref() == Some(id) {
            2
        } else if ui.hover.as_deref() == Some(id) {
            1
        } else {
            0
        };
        let rect = Rect::new(sp.x, sp.y, sp.width.unwrap_or(100), h);
        control(f, id, label, rect, action, &[]);
    }
}
