//! Ordered browser input, with one physical held-state packet per visible frame.
#![cfg_attr(not(feature = "web-client"), allow(dead_code))]
use crate::scene::{Action, SceneControl, SceneFrame};
#[cfg(feature = "web-client")]
use crate::scene::{Screen, UiAction};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Source {
    Key(String),
    Mouse,
    Touch(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeldField {
    Left,
    Right,
    Fire,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Held {
    left: bool,
    right: bool,
    fire: bool,
}
impl Held {
    fn packet(self) -> Value {
        json!({"type":"minigameAction","action":"input","left":self.left,"right":self.right,"fire":self.fire})
    }
}
#[derive(Clone)]
struct Capture {
    control: SceneControl,
    held: Option<HeldField>,
}
#[derive(Clone)]
struct Drag {
    source: Source,
    point: (i32, i32),
}
#[derive(bevy::prelude::Resource)]
pub struct ActiveInputs {
    captures: HashMap<Source, Capture>,
    hovered: Option<SceneControl>,
    drag: Option<Drag>,
    scope: String,
    last_sent: Held,
    fire_pulse: bool,
    focused: bool,
    last_mouse_point: Option<(i32, i32)>,
    pointer_from_key: bool,
}
impl Default for ActiveInputs {
    fn default() -> Self {
        Self {
            captures: HashMap::new(),
            hovered: None,
            drag: None,
            scope: String::new(),
            last_sent: Held::default(),
            fire_pulse: false,
            focused: true,
            last_mouse_point: None,
            pointer_from_key: false,
        }
    }
}
impl ActiveInputs {
    fn begin(&mut self, source: Source, control: SceneControl, game: &str) -> bool {
        if self.captures.contains_key(&source) {
            return false;
        }
        let held = held_field(&control, game);
        if held == Some(HeldField::Fire) {
            self.fire_pulse = true;
        }
        self.captures.insert(source, Capture { control, held });
        true
    }
    fn held(&self, game: &str) -> Held {
        let mut held = Held::default();
        for capture in self.captures.values() {
            match capture.held {
                Some(HeldField::Left) => held.left = true,
                Some(HeldField::Right) => held.right = true,
                Some(HeldField::Fire) => {}
                _ => {}
            }
        }
        held.fire = self.fire_pulse;
        let _ = game;
        held
    }
    fn packet(&mut self, game: &str) -> Option<Value> {
        let held = self.held(game);
        self.fire_pulse = false;
        if held == self.last_sent {
            return None;
        }
        self.last_sent = held;
        Some(held.packet())
    }
    fn clear(&mut self) {
        self.captures.clear();
        self.hovered = None;
        self.drag = None;
        self.last_sent = Held::default();
        self.fire_pulse = false;
        self.pointer_from_key = false;
    }
}
fn held_field(control: &SceneControl, game: &str) -> Option<HeldField> {
    if !["fist", "firebee"].contains(&game) {
        return None;
    }
    let Action::Game(value) = &control.action else {
        return None;
    };
    if value["type"] != "minigameAction" || value["action"] != "input" {
        return None;
    }
    match control.id.strip_prefix(&format!("{game}-")) {
        Some("left") => Some(HeldField::Left),
        Some("right") => Some(HeldField::Right),
        Some("fire") => Some(HeldField::Fire),
        _ => None,
    }
}
/// Convert a letterboxed window position to the exact original logical pixel grid.
fn logical(width: f32, height: f32, position: (f32, f32)) -> (i32, i32) {
    let scale = (width / 640.0).min(height / 480.0);
    if !scale.is_finite() || scale <= 0.0 {
        return (-1, -1);
    }
    let x = (position.0 - (width - 640.0 * scale) / 2.0) / scale;
    let y = (position.1 - (height - 480.0 * scale) / 2.0) / scale;
    (x.floor() as i32, y.floor() as i32)
}
fn hit(scene: &SceneFrame, point: (i32, i32)) -> Option<SceneControl> {
    if !(0..640).contains(&point.0) || !(0..480).contains(&point.1) {
        return None;
    }
    scene
        .controls
        .iter()
        .enumerate()
        .filter(|(_, control)| {
            control.enabled
                && point.0 >= control.bounds.x
                && point.1 >= control.bounds.y
                && point.0 < control.bounds.x + control.bounds.width as i32
                && point.1 < control.bounds.y + control.bounds.height as i32
        })
        .max_by_key(|(index, control)| (control.z, *index))
        .map(|(_, control)| control.clone())
}
fn key_names(physical: &str) -> Vec<String> {
    let canonical = if physical == " " {
        "Space".to_owned()
    } else if physical.len() == 1 && physical.as_bytes()[0].is_ascii_alphabetic() {
        format!("Key{}", physical.to_ascii_uppercase())
    } else if physical.len() == 1 && physical.as_bytes()[0].is_ascii_digit() {
        format!("Digit{physical}")
    } else {
        physical.to_owned()
    };
    let mut names = vec![canonical.clone()];
    if let Some(letter) = canonical.strip_prefix("Key") {
        names.push(letter.to_owned());
        names.push(letter.to_lowercase());
    }
    if let Some(digit) = canonical.strip_prefix("Digit") {
        names.push(digit.to_owned());
    }
    names
}
fn key_control(scene: &SceneFrame, names: &[String], up: bool) -> Option<SceneControl> {
    scene
        .controls
        .iter()
        .rev()
        .find(|control| {
            control.enabled
                && control.keys.iter().any(|key| {
                    if up {
                        key.strip_suffix(":up")
                            .is_some_and(|key| names.iter().any(|name| name == key))
                    } else {
                        !key.ends_with(":up") && names.contains(key)
                    }
                })
        })
        .cloned()
}
fn hammer_action(control: &SceneControl, point: (i32, i32), mouse: bool) -> Action {
    let Action::Game(value) = &control.action else {
        return control.action.clone();
    };
    if value["type"] == "minigameAction"
        && (value["action"] == "strike" || mouse && value["action"] == "strikeIndex")
    {
        Action::Game(
            json!({"type":"minigameAction","action":"strike","x":point.0.clamp(102,467),"y":point.1.clamp(59,339)}),
        )
    } else {
        control.action.clone()
    }
}
fn pan_delta(key: &str) -> Option<(i32, i32)> {
    match key {
        "ArrowLeft" => Some((-66, 0)),
        "ArrowRight" => Some((66, 0)),
        "ArrowUp" => Some((0, -38)),
        "ArrowDown" => Some((0, 38)),
        _ => None,
    }
}
fn clamp_camera(camera: (i32, i32), delta: (i32, i32), size: (i32, i32)) -> (i32, i32) {
    (
        (camera.0 + delta.0).clamp(0, (size.0 - 640).max(0)),
        (camera.1 + delta.1).clamp(0, (size.1 - 480).max(0)),
    )
}

#[cfg(feature = "web-client")]
mod web {
    use super::*;
    use crate::{
        PlatformRequests,
        runtime::{Actions, CurrentScene, GameHost},
    };
    use bevy::{
        input::{
            ButtonState, InputSystems,
            keyboard::{KeyboardFocusLost, KeyboardInput},
            mouse::MouseWheel,
            touch::{TouchInput, TouchPhase},
        },
        prelude::*,
        window::{CursorOptions, PrimaryWindow, WindowFocused},
    };

    #[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
    pub struct GameInputSystems;
    pub struct InputPlugin;
    impl Plugin for InputPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<ActiveInputs>()
                .add_systems(
                    PreUpdate,
                    (
                        lifecycle,
                        keyboard_input,
                        pointer_input,
                        wheel_input,
                        touch_input,
                        test_input,
                        flush_held,
                    )
                        .chain()
                        .after(InputSystems)
                        .in_set(GameInputSystems),
                )
                .add_systems(
                    Update,
                    post_actions_lifecycle
                        .after(crate::runtime::apply_actions)
                        .before(crate::runtime::advance_game),
                )
                .add_systems(PostUpdate, synchronize_cursor);
        }
    }
    fn s<'a>(v: &'a Value, path: &str) -> &'a str {
        v.pointer(path).and_then(Value::as_str).unwrap_or("")
    }
    fn n(v: &Value, path: &str) -> i32 {
        v.pointer(path).and_then(Value::as_i64).unwrap_or(0) as i32
    }
    fn game(host: &GameHost) -> &str {
        s(&host.snapshot, "/pending/game")
    }
    fn scope(host: &GameHost) -> String {
        format!(
            "{:?}:{}:{}:{}:error={}:cue={:?}",
            host.ui.screen,
            s(&host.snapshot, "/pending/kind"),
            game(host),
            s(&host.snapshot, "/pending/state/phase"),
            host.error.is_some(),
            host.ui
                .runtime
                .sequence
                .as_ref()
                .and_then(|q| q.cues.front())
                .map(|cue| cue.serial)
        )
    }
    fn editing(host: &GameHost) -> bool {
        host.ui.profile["_imeActive"].as_bool().unwrap_or(false)
    }
    fn blocked(host: &GameHost, _inputs: &ActiveInputs) -> bool {
        // IME owns keyboard text, while canvas buttons remain clickable. A load
        // error replaces the busy overlay with controls that must accept retry.
        host.hidden || host.error.is_none() && (host.loading || host.ui.runtime.motion.is_some())
    }

    fn held_game(host: &GameHost) -> bool {
        host.ui.screen == Screen::Game
            && !crate::presentation::sequence_active(&host.ui)
            && ["fist", "firebee"].contains(&game(host))
            && s(&host.snapshot, "/pending/state/phase") == "playing"
    }
    fn release_domain(host: &mut GameHost) {
        if game(host) == "fire" && s(&host.snapshot, "/pending/state/phase") == "playing" {
            let rest = n(&host.snapshot, "/pending/state/bodyRest");
            if rest == 1 || rest == 2 {
                host.send(json!({"type":"minigameAction","action":if rest==1{"releaseLeft"}else{"releaseRight"}}));
            }
        }
        if ["fist", "firebee"].contains(&game(host)) {
            let state = &host.snapshot["pending"]["state"];
            if ["left", "right", "fire"]
                .iter()
                .any(|field| state[*field].as_bool().unwrap_or(false))
            {
                host.send(Held::default().packet());
            }
        }
    }
    fn release_capture_edges(host: &mut GameHost, inputs: &ActiveInputs) {
        let kind = s(&host.snapshot, "/pending/kind").to_owned();
        let current_game = game(host).to_owned();
        for capture in inputs.captures.values() {
            if let Some(Action::Game(value)) = &capture.control.on_release {
                let same = kind == "ending" && value["type"] == "endingAction"
                    || current_game == "fire" && value["type"] == "minigameAction";
                if same {
                    host.send(value.clone());
                }
            }
        }
    }
    fn reconcile(host: &mut GameHost, inputs: &mut ActiveInputs) {
        let next = scope(host);
        if inputs.scope != next || blocked(host, inputs) {
            release_capture_edges(host, inputs);
            release_domain(host);
            inputs.clear();
            host.ui.pressed = None;
            host.ui.hover = None;
        }
        inputs.scope = next;
    }
    fn lifecycle(
        windows: Query<&Window, With<PrimaryWindow>>,
        mut focus: MessageReader<WindowFocused>,
        mut lost: MessageReader<KeyboardFocusLost>,
        mut host: ResMut<GameHost>,
        mut inputs: ResMut<ActiveInputs>,
    ) {
        let was_focused = inputs.focused;
        for event in focus.read() {
            inputs.focused = event.focused;
        }
        let lost_event = lost.read().count() > 0;
        if let Ok(window) = windows.single() {
            inputs.focused = window.focused;
        }
        if lost_event || was_focused && !inputs.focused {
            release_capture_edges(&mut host, &inputs);
            release_domain(&mut host);
            inputs.clear();
        }
        reconcile(&mut host, &mut inputs);
    }
    fn post_actions_lifecycle(mut host: ResMut<GameHost>, mut inputs: ResMut<ActiveInputs>) {
        reconcile(&mut host, &mut inputs);
    }
    fn map_active(host: &GameHost) -> bool {
        host.ui.screen == Screen::Game
            && host.snapshot["pending"].is_null()
            && host.ui.runtime.motion.is_none()
            && !host.hidden
            && !host.loading
            && host.error.is_none()
    }
    fn pan(host: &mut GameHost, delta: (i32, i32)) {
        let map = host.catalogs["maps"]["maps"]
            .as_array()
            .and_then(|maps| maps.iter().find(|map| map["id"] == host.snapshot["mapId"]));
        let Some(map) = map else { return };
        let camera = if host.ui.profile["_cameraManual"].as_bool().unwrap_or(false)
            || host.ui.camera != (0, 0)
        {
            host.ui.camera
        } else {
            let tile = &map["chips"][host.snapshot["tileId"].as_u64().unwrap_or(0) as usize];
            let col = n(tile, "/column") + 1;
            let row = n(tile, "/row") + 1;
            (
                n(map, "/chipOrigin/x") + (n(map, "/chipSize/width") * col).div_euclid(2)
                    - col.div_euclid(2)
                    - 320,
                n(map, "/chipOrigin/y") + (n(map, "/chipSize/height") * row).div_euclid(2) - 245,
            )
        };
        host.ui.camera = clamp_camera(
            camera,
            delta,
            (n(map, "/dimensions/width"), n(map, "/dimensions/height")),
        );
        host.ui.profile["_cameraManual"] = json!(true);
    }
    fn emit(actions: &mut Actions, action: Action) {
        actions.0.push_back(action);
    }
    fn mark_ui_click(host: &mut GameHost, control: &SceneControl) {
        if host.ui.screen == Screen::Profile && control.id.starts_with("profile-") {
            let serial = host.ui.profile["_uiClickSerial"].as_u64().unwrap_or(0);
            host.ui.profile["_uiClickSerial"] = json!(serial.wrapping_add(1));
            host.ui.profile["_uiClickControl"] = json!(control.id);
        }
    }
    fn key_event(
        key: &str,
        down: bool,
        repeat: bool,
        scene: &SceneFrame,
        host: &mut GameHost,
        inputs: &mut ActiveInputs,
        actions: &mut Actions,
        requests: &mut PlatformRequests,
    ) {
        let names = key_names(key);
        let source = Source::Key(names[0].clone());
        if !down {
            if !host.hidden
                && names.iter().any(|key| key == "Escape")
                && host
                    .ui
                    .runtime
                    .sequence
                    .as_ref()
                    .and_then(|q| q.cues.front())
                    .is_some_and(|cue| cue.kind == "prologue")
            {
                emit(actions, Action::Ui(UiAction::AdvancePresentation));
                return;
            }
            if let Some(capture) = inputs.captures.remove(&source) {
                if capture.held.is_none() {
                    if let Some(action) = capture.control.on_release {
                        emit(actions, action);
                    }
                }
            }
            if !host.hidden && !editing(host) {
                if let Some(control) = key_control(scene, &names, true) {
                    emit(actions, control.action);
                }
            }
            return;
        }
        if host.hidden || editing(host) || !inputs.focused {
            return;
        }
        if !repeat {
            if names.iter().any(|key| key == "Escape")
                && host
                    .ui
                    .runtime
                    .sequence
                    .as_ref()
                    .and_then(|q| q.cues.front())
                    .is_some_and(|cue| cue.kind == "opening")
            {
                // The original startup uses GetAsyncKeyState. Allow its skip
                // while the large movie's first browser frame is downloading.
                emit(actions, Action::Ui(UiAction::AdvancePresentation));
                return;
            }
            if names.iter().any(|key| key == "F6") {
                if host.ui.runtime.motion.is_none() {
                    release_capture_edges(host, inputs);
                    release_domain(host);
                    inputs.clear();
                    emit(
                        actions,
                        Action::Ui(if host.ui.screen == Screen::Saves {
                            UiAction::Back
                        } else {
                            UiAction::OpenSaves
                        }),
                    );
                }
                return;
            }
            if names.iter().any(|key| key == "F10") {
                release_capture_edges(host, inputs);
                release_domain(host);
                inputs.clear();
                emit(actions, Action::Ui(UiAction::MainMenu));
                return;
            }
            if names.iter().any(|key| key == "F11") {
                host.request(requests, "fullscreen", Value::Null);
                return;
            }
        }
        if blocked(host, inputs) {
            return;
        }
        if map_active(host) {
            if let Some(delta) = names.iter().find_map(|name| pan_delta(name)) {
                pan(host, delta);
                return;
            }
        }
        if repeat {
            return;
        }
        if let Some(control) = key_control(scene, &names, false) {
            if game(host) == "hammer" {
                if let Action::Game(value) = &control.action {
                    if value["action"] == "strikeIndex" {
                        let index = value["index"].as_u64().unwrap_or(0) as usize;
                        let target = &host.snapshot["pending"]["state"]["targets"][index];
                        host.ui.pointer = (n(target, "/x") + 38, n(target, "/y") - 28);
                        inputs.pointer_from_key = true;
                    }
                }
            }
            if inputs.begin(source, control.clone(), game(host)) {
                mark_ui_click(host, &control);
                if held_field(&control, game(host)).is_none() {
                    emit(actions, control.on_press.unwrap_or(control.action));
                }
            }
        }
    }
    fn keyboard_input(
        mut events: MessageReader<KeyboardInput>,
        scene: Res<CurrentScene>,
        mut host: ResMut<GameHost>,
        mut inputs: ResMut<ActiveInputs>,
        mut actions: ResMut<Actions>,
        mut requests: ResMut<PlatformRequests>,
    ) {
        for event in events.read() {
            key_event(
                &format!("{:?}", event.key_code),
                event.state == ButtonState::Pressed,
                event.repeat,
                &scene.0,
                &mut host,
                &mut inputs,
                &mut actions,
                &mut requests,
            );
        }
    }
    fn test_input(
        scene: Res<CurrentScene>,
        mut host: ResMut<GameHost>,
        mut inputs: ResMut<ActiveInputs>,
        mut actions: ResMut<Actions>,
        mut requests: ResMut<PlatformRequests>,
    ) {
        let value = host.ui.profile["_testKey"].take();
        let values = if let Value::Array(values) = value {
            values
        } else {
            vec![value]
        };
        for value in values {
            if let Some(key) = value["key"].as_str() {
                key_event(
                    key,
                    value["down"].as_bool().unwrap_or(true),
                    value["repeat"].as_bool().unwrap_or(false),
                    &scene.0,
                    &mut host,
                    &mut inputs,
                    &mut actions,
                    &mut requests,
                );
            }
        }
    }
    fn hover(
        host: &mut GameHost,
        inputs: &mut ActiveInputs,
        target: Option<&SceneControl>,
        actions: &mut Actions,
    ) {
        if inputs.hovered.as_ref().map(|control| &control.id) != target.map(|control| &control.id) {
            if let Some(previous) = inputs.hovered.take() {
                if let Some(action) = previous.on_leave {
                    emit(actions, action);
                }
            }
            if let Some(target) = target {
                if let Some(action) = &target.on_hover {
                    emit(actions, action.clone());
                }
                inputs.hovered = Some(target.clone());
            }
        }
        host.ui.hover = target.map(|control| control.id.clone());
    }
    fn press(
        source: Source,
        point: (i32, i32),
        mouse: bool,
        scene: &SceneFrame,
        host: &mut GameHost,
        inputs: &mut ActiveInputs,
        actions: &mut Actions,
    ) {
        let mut target = hit(scene, point);
        if !mouse
            && host
                .ui
                .runtime
                .sequence
                .as_ref()
                .and_then(|q| q.cues.front())
                .is_some_and(|cue| ["opening", "prologue"].contains(&cue.kind.as_str()))
        {
            // Touch has no Escape key; tapping the movie provides the same
            // skip action without changing the original desktop hitboxes.
            target = scene
                .controls
                .iter()
                .find(|c| c.id == "native-presentation-continue")
                .cloned();
            if let Some(control) = target.as_mut() {
                control.on_press = Some(control.action.clone());
            }
        }
        if game(host) == "hammer" && s(&host.snapshot, "/pending/state/phase") == "playing" {
            if let Some(control) = target.as_mut() {
                let action = hammer_action(control, point, mouse);
                control.action = action.clone();
                control.on_press = Some(action);
            }
            host.ui.pointer = (point.0.clamp(102, 467), point.1.clamp(59, 339));
            if !mouse {
                if let Some(control) = &target {
                    if let Action::Game(value) = &control.action {
                        if value["action"] == "strikeIndex" {
                            let index = value["index"].as_u64().unwrap_or(0) as usize;
                            let target = &host.snapshot["pending"]["state"]["targets"][index];
                            host.ui.pointer = (n(target, "/x") + 38, n(target, "/y") - 28);
                        }
                    }
                }
            }
        }
        if let Some(control) = target {
            host.ui.pressed = Some(control.id.clone());
            if inputs.begin(source, control.clone(), game(host)) {
                mark_ui_click(host, &control);
                if held_field(&control, game(host)).is_none() {
                    if let Some(action) = control.on_press {
                        emit(actions, action);
                    }
                }
            }
        } else if map_active(host) && inputs.drag.is_none() {
            inputs.drag = Some(Drag { source, point });
        }
    }
    fn release(
        source: &Source,
        point: (i32, i32),
        cancelled: bool,
        scene: &SceneFrame,
        host: &mut GameHost,
        inputs: &mut ActiveInputs,
        actions: &mut Actions,
    ) {
        if inputs
            .drag
            .as_ref()
            .is_some_and(|drag| &drag.source == source)
        {
            inputs.drag = None;
        }
        if let Some(capture) = inputs.captures.remove(source) {
            if capture.held.is_none() {
                if let Some(action) = capture.control.on_release {
                    emit(actions, action);
                }
                if !cancelled
                    && capture.control.on_press.is_none()
                    && hit(scene, point).is_some_and(|target| target.id == capture.control.id)
                {
                    emit(actions, capture.control.action);
                }
            }
        }
        host.ui.pressed = inputs
            .captures
            .values()
            .next()
            .map(|capture| capture.control.id.clone());
    }
    fn move_drag(
        source: &Source,
        point: (i32, i32),
        host: &mut GameHost,
        inputs: &mut ActiveInputs,
    ) {
        if let Some(drag) = inputs.drag.as_mut() {
            if &drag.source == source && map_active(host) {
                let delta = (drag.point.0 - point.0, drag.point.1 - point.1);
                drag.point = point;
                pan(host, delta);
            }
        }
    }
    fn move_volume(
        source: &Source,
        point: (i32, i32),
        scene: &SceneFrame,
        host: &GameHost,
        inputs: &ActiveInputs,
        actions: &mut Actions,
    ) {
        let dragging = inputs.captures.get(source).is_some_and(|capture| {
            matches!(capture.control.action, Action::Ui(UiAction::SetVolume(_)))
        });
        if dragging {
            if let Some(control) = hit(scene, point) {
                if let Action::Ui(UiAction::SetVolume(value)) = control.action {
                    if value != i32::from(host.ui.volume) {
                        emit(actions, Action::Ui(UiAction::SetVolume(value)));
                    }
                }
            }
        }
    }
    fn pointer_input(
        windows: Query<&Window, With<PrimaryWindow>>,
        buttons: Res<ButtonInput<MouseButton>>,
        scene: Res<CurrentScene>,
        mut host: ResMut<GameHost>,
        mut inputs: ResMut<ActiveInputs>,
        mut actions: ResMut<Actions>,
    ) {
        let Ok(window) = windows.single() else { return };
        if blocked(&host, &inputs) {
            return;
        }
        if inputs
            .captures
            .keys()
            .any(|source| matches!(source, Source::Touch(_)))
            || inputs
                .drag
                .as_ref()
                .is_some_and(|drag| matches!(drag.source, Source::Touch(_)))
        {
            return;
        }
        let point = window
            .cursor_position()
            .map(|p| logical(window.width(), window.height(), (p.x, p.y)))
            .unwrap_or((-1, -1));
        // Polling an unchanged mouse must not move a keyboard-controlled hammer cursor back.
        let moved = inputs.last_mouse_point != Some(point);
        inputs.last_mouse_point = Some(point);
        if !inputs.pointer_from_key
            && (moved
                || buttons.just_pressed(MouseButton::Left)
                || buttons.just_released(MouseButton::Left))
        {
            host.ui.pointer = point;
        }
        inputs.pointer_from_key = false;
        let target = hit(&scene.0, point);
        hover(&mut host, &mut inputs, target.as_ref(), &mut actions);
        if point != (-1, -1) {
            move_drag(&Source::Mouse, point, &mut host, &mut inputs);
            if moved && buttons.pressed(MouseButton::Left) {
                move_volume(
                    &Source::Mouse,
                    point,
                    &scene.0,
                    &host,
                    &inputs,
                    &mut actions,
                );
            }
        }
        if buttons.just_pressed(MouseButton::Left) {
            press(
                Source::Mouse,
                point,
                true,
                &scene.0,
                &mut host,
                &mut inputs,
                &mut actions,
            );
        }
        if buttons.just_released(MouseButton::Left) {
            release(
                &Source::Mouse,
                point,
                false,
                &scene.0,
                &mut host,
                &mut inputs,
                &mut actions,
            );
        }
        if buttons.just_pressed(MouseButton::Right) {
            if let Some(control) = key_control(&scene.0, &["MouseRight".into()], false) {
                emit(&mut actions, control.action);
            }
        }
    }
    fn wheel_input(
        mut events: MessageReader<MouseWheel>,
        scene: Res<CurrentScene>,
        host: Res<GameHost>,
        inputs: Res<ActiveInputs>,
        mut actions: ResMut<Actions>,
    ) {
        for event in events.read() {
            if blocked(&host, &inputs) || editing(&host) || !inputs.focused {
                continue;
            }
            let key = if event.y > 0.0 {
                "WheelUp"
            } else if event.y < 0.0 {
                "WheelDown"
            } else {
                continue;
            };
            if let Some(control) = key_control(&scene.0, &[key.to_owned()], false) {
                emit(&mut actions, control.action);
            }
        }
    }
    fn touch_input(
        mut events: MessageReader<TouchInput>,
        windows: Query<&Window, With<PrimaryWindow>>,
        scene: Res<CurrentScene>,
        mut host: ResMut<GameHost>,
        mut inputs: ResMut<ActiveInputs>,
        mut actions: ResMut<Actions>,
    ) {
        let Ok(window) = windows.single() else { return };
        for event in events.read() {
            let point = logical(
                window.width(),
                window.height(),
                (event.position.x, event.position.y),
            );
            let source = Source::Touch(event.id);
            if blocked(&host, &inputs) {
                continue;
            }
            host.ui.pointer = point;
            match event.phase {
                TouchPhase::Started => {
                    let target = hit(&scene.0, point);
                    hover(&mut host, &mut inputs, target.as_ref(), &mut actions);
                    press(
                        source,
                        point,
                        false,
                        &scene.0,
                        &mut host,
                        &mut inputs,
                        &mut actions,
                    );
                }
                TouchPhase::Moved => {
                    move_drag(&source, point, &mut host, &mut inputs);
                    move_volume(&source, point, &scene.0, &host, &inputs, &mut actions);
                }
                TouchPhase::Ended => release(
                    &source,
                    point,
                    false,
                    &scene.0,
                    &mut host,
                    &mut inputs,
                    &mut actions,
                ),
                TouchPhase::Canceled => release(
                    &source,
                    point,
                    true,
                    &scene.0,
                    &mut host,
                    &mut inputs,
                    &mut actions,
                ),
            }
        }
    }
    fn flush_held(
        host: Res<GameHost>,
        mut inputs: ResMut<ActiveInputs>,
        mut actions: ResMut<Actions>,
    ) {
        if !held_game(&host) || blocked(&host, &inputs) || host.error.is_some() {
            return;
        }
        let game = game(&host).to_owned();
        if let Some(packet) = inputs.packet(&game) {
            emit(&mut actions, Action::Game(packet));
        }
    }
    fn synchronize_cursor(
        host: Res<GameHost>,
        scene: Res<CurrentScene>,
        mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    ) {
        let hidden =
            host.ui.screen == Screen::Game
                && !host.hidden
                && !host.loading
                && host.error.is_none()
                && scene.0.requests.iter().any(|request| {
                    matches!(request, crate::scene::SceneRequest::CursorHidden(true))
                });
        for mut cursor in &mut windows {
            cursor.visible = !hidden;
        }
    }
}
#[cfg(feature = "web-client")]
pub use web::{GameInputSystems, InputPlugin};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Rect;
    fn control(id: &str) -> SceneControl {
        SceneControl {
            id: id.into(),
            label: id.into(),
            bounds: Rect::new(0, 0, 640, 480),
            action: Action::Game(
                json!({"type":"minigameAction","action":"input","left":false,"right":false,"fire":false}),
            ),
            enabled: true,
            z: 0,
            keys: vec![],
            on_press: None,
            on_release: None,
            on_hover: None,
            on_leave: None,
        }
    }
    #[test]
    fn simultaneous_keys_and_multiple_touches_merge_without_overwriting_or_early_release() {
        let mut inputs = ActiveInputs::default();
        let left = Source::Key("ArrowLeft".into());
        let right = Source::Touch(2);
        let fire = Source::Key("Space".into());
        inputs.begin(left.clone(), control("firebee-left"), "firebee");
        inputs.begin(right.clone(), control("firebee-right"), "firebee");
        inputs.begin(fire.clone(), control("firebee-fire"), "firebee");
        assert_eq!(
            inputs.packet("firebee").unwrap(),
            Held {
                left: true,
                right: true,
                fire: true
            }
            .packet()
        );
        assert_eq!(
            inputs.packet("firebee").unwrap(),
            Held {
                left: true,
                right: true,
                fire: false
            }
            .packet()
        );
        inputs.begin(Source::Touch(3), control("firebee-left"), "firebee");
        inputs.captures.remove(&left);
        assert!(inputs.packet("firebee").is_none());
        inputs.captures.remove(&right);
        assert_eq!(
            inputs.packet("firebee").unwrap(),
            Held {
                left: true,
                right: false,
                fire: false
            }
            .packet()
        );
        inputs.captures.remove(&fire);
        assert!(inputs.packet("firebee").is_none());
        inputs.clear();
        assert!(inputs.captures.is_empty());
        assert_eq!(inputs.held("firebee"), Held::default());
    }
    #[test]
    fn fist_fire_is_one_press_pulse_while_held_direction_changes_remain_independent() {
        let mut inputs = ActiveInputs::default();
        inputs.begin(Source::Key("Space".into()), control("fist-fire"), "fist");
        assert!(inputs.packet("fist").unwrap()["fire"].as_bool().unwrap());
        inputs.begin(
            Source::Key("ArrowRight".into()),
            control("fist-right"),
            "fist",
        );
        let next = inputs.packet("fist").unwrap();
        assert_eq!(next["right"], true);
        assert_eq!(next["fire"], false);
        assert!(!inputs.begin(Source::Key("Space".into()), control("fist-fire"), "fist"));
        assert!(inputs.packet("fist").is_none());
    }
    #[test]
    fn hammer_mouse_uses_current_point_and_clamps_without_reusing_composed_coordinates() {
        let mut hit = control("hammer-hole-2");
        hit.action = Action::Game(
            json!({"type":"minigameAction","action":"strikeIndex","index":2,"x":0,"y":0}),
        );
        assert_eq!(
            hammer_action(&hit, (700, 20), true),
            Action::Game(json!({"type":"minigameAction","action":"strike","x":467,"y":59}))
        );
        assert_eq!(hammer_action(&hit, (700, 20), false), hit.action);
    }
    #[test]
    fn letterboxing_and_map_steps_use_original_logical_pixels_and_bounds() {
        assert_eq!(logical(1280.0, 960.0, (204.0, 118.0)), (102, 59));
        assert_eq!(logical(640.0, 800.0, (100.0, 150.0)), (100, -10));
        assert_eq!(logical(0.0, 0.0, (10.0, 10.0)), (-1, -1));
        assert_eq!(pan_delta("ArrowRight"), Some((66, 0)));
        assert_eq!(pan_delta("ArrowUp"), Some((0, -38)));
        assert_eq!(clamp_camera((20, 20), (-66, -38), (1280, 960)), (0, 0));
        assert_eq!(clamp_camera((630, 470), (66, 38), (1280, 960)), (640, 480));
    }
    #[test]
    fn release_key_bindings_and_topmost_hit_remain_distinct() {
        let mut down = control("down");
        down.keys = vec!["S".into()];
        let mut up = control("up");
        up.keys = vec!["S:up".into()];
        up.z = 5;
        let scene = SceneFrame {
            controls: vec![down, up],
            ..Default::default()
        };
        let names = key_names("KeyS");
        assert_eq!(key_control(&scene, &names, false).unwrap().id, "down");
        assert_eq!(key_control(&scene, &names, true).unwrap().id, "up");
        assert_eq!(hit(&scene, (10, 10)).unwrap().id, "up");
        assert!(hit(&scene, (-1, 10)).is_none());
    }
}
