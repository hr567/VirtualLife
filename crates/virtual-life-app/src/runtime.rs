//! Ordered Bevy systems are the only owner of gameplay, UI and platform effects.
use crate::scene::Rect;
use crate::{
    PlatformEvent, PlatformEvents, PlatformRequest, PlatformRequests, pixels::*, presentation,
    scene::*,
};
use bevy::prelude::*;
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use virtual_life_core::{GameCommand, GameResponse, GameSession};

#[derive(Resource, Default)]
pub struct Actions(pub VecDeque<Action>);
#[derive(Resource, Default)]
pub struct CurrentScene(pub SceneFrame);

fn clean_restored_input(ui: &mut PresentationState) {
    ui.screen = Screen::Game;
    ui.hover = None;
    ui.pressed = None;
    if let Some(profile) = ui.profile.as_object_mut() {
        for key in [
            "_testKey",
            "_testControl",
            "_imeActive",
            "_uiClickControl",
            "_uiClickSerial",
            "_uiSoundClickSerial",
            "_uiSoundHover",
            "_uiSoundPressed",
        ] {
            profile.remove(key);
        }
    }
}
fn encode_save(session: &GameSession, ui: &PresentationState) -> String {
    let mut ui = ui.clone();
    clean_restored_input(&mut ui);
    let mut envelope: Value =
        serde_json::from_str(&session.save()).expect("Domain save serializes");
    envelope["presentation"] = serde_json::to_value(ui).expect("Bevy presentation serializes");
    envelope.to_string()
}
fn decode_save(payload: &str) -> Result<(String, Option<PresentationState>), String> {
    let mut envelope: Value =
        serde_json::from_str(payload).map_err(|error| format!("存档内容无效：{error}"))?;
    let fields = envelope.as_object_mut().ok_or("存档内容必须是对象。")?;
    let presentation = fields
        .remove("presentation")
        .map(|value| -> Result<PresentationState, String> {
            let mut ui: PresentationState = serde_json::from_value(value)
                .map_err(|error| format!("存档中的场景状态无效：{error}"))?;
            if !ui.profile.is_object()
                || !ui.allocations.is_object()
                || ui.runtime.motion.as_ref().is_some_and(|motion| {
                    motion.frames.len() > 2048 || motion.cursor > motion.frames.len()
                })
            {
                return Err("存档中的场景状态不完整。".into());
            }
            clean_restored_input(&mut ui);
            Ok(ui)
        })
        .transpose()?;
    Ok((envelope.to_string(), presentation))
}
#[derive(Resource)]
pub struct GameHost {
    pub session: Option<GameSession>,
    pub catalogs: Value,
    pub snapshot: Value,
    pub ui: PresentationState,
    pub hidden: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub notice: String,
    pub seed: u64,
    pub generation: u32,
    pub scene_key: String,
    pub return_screen: Screen,
    pub frame_ms: u32,
    pub manual_frame: Option<u32>,
    pub test_clock: bool,
    discard_next_frame: bool,
    last_real_ms: Option<u128>,
    next_request: u32,
    restore_request: Option<u32>,
    init_sent: bool,
    video_request: Option<(String, u64)>,
    video_resource: Option<String>,
    last_prefetch: Vec<String>,
}
impl Default for GameHost {
    fn default() -> Self {
        Self {
            session: None,
            catalogs: Value::Null,
            snapshot: Value::Null,
            ui: Default::default(),
            hidden: false,
            loading: true,
            error: None,
            notice: "正在载入原版资源…".into(),
            seed: 1,
            generation: 1,
            scene_key: String::new(),
            return_screen: Screen::Menu,
            frame_ms: 0,
            manual_frame: None,
            test_clock: false,
            discard_next_frame: false,
            last_real_ms: None,
            next_request: 1,
            restore_request: None,
            init_sent: false,
            video_request: None,
            video_resource: None,
            last_prefetch: vec![],
        }
    }
}
impl GameHost {
    pub fn request(&mut self, requests: &mut PlatformRequests, op: &str, payload: Value) -> u32 {
        let id = self.next_request;
        self.next_request = self.next_request.wrapping_add(1).max(1);
        if matches!(op, "load" | "import") {
            self.restore_request = Some(id);
        }
        requests.0.push_back(PlatformRequest {
            id,
            operation: op.into(),
            payload,
        });
        id
    }
    pub fn paused(&self) -> bool {
        self.hidden
            || self.loading
            || self.error.is_some()
            || self.ui.screen != Screen::Game
            || self.test_clock && self.frame_ms == 0
    }
    fn response(&mut self, response: GameResponse) {
        let next = serde_json::to_value(&response.snapshot).expect("Typed snapshot serializes");
        let completed = response.ok && next["phase"] == "completed";
        if !self.catalogs.is_null() {
            let content = SceneContent {
                manifest: &self.catalogs["manifest"],
                maps: &self.catalogs["maps"],
                rules: &self.catalogs["rules"],
            };
            let events: Vec<Value> = response
                .events
                .iter()
                .map(|e| serde_json::to_value(e).unwrap())
                .collect();
            presentation::accept_response(&content, &self.snapshot, &next, &events, &mut self.ui);
        }
        self.snapshot = next;
        if let Some(error) = response.error {
            self.ui.profile["_nativeError"] = json!(error.message);
            self.notice = error.message;
        } else {
            self.ui.profile["_nativeError"] = Value::Null;
        }
        if completed {
            // 0041c040's No branch returns through 004158a0 to the main
            // menu. Keep the terminal domain state, never render an idle map.
            self.ui.runtime.motion = None;
            self.set_screen(Screen::Menu);
        }
    }
    pub fn send(&mut self, mut value: Value) {
        value["seq"] = json!(0);
        match serde_json::from_value::<GameCommand>(value) {
            Ok(command) => {
                if let Some(session) = self.session.as_mut() {
                    let response = session.send(command);
                    self.response(response);
                }
            }
            Err(error) => self.error = Some(format!("游戏输入无效：{error}")),
        }
    }
    pub(crate) fn save(&mut self, requests: &mut PlatformRequests) {
        if let Some(session) = &self.session {
            if self.snapshot["player"].is_null() {
                self.notice = "尚未开始新的人生。".into();
                return;
            }
            let payload = json!({"payload":encode_save(session,&self.ui),"contentId":session.content_identity()});
            self.request(requests, "save", payload);
        }
    }
    fn set_screen(&mut self, screen: Screen) {
        if self.ui.screen != screen {
            self.ui.screen = screen;
            self.ui.scene_ms = 0;
            self.ui.hover = None;
            self.ui.pressed = None;
            self.generation = self.generation.wrapping_add(1);
            self.video_request = None;
            self.restore_request = None;
        }
    }
}

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameHost>()
            .init_resource::<Actions>()
            .init_resource::<CurrentScene>()
            .init_resource::<ImageBank>()
            .init_resource::<PixelCanvas>()
            .add_systems(
                Update,
                (
                    receive_platform,
                    apply_actions,
                    advance_game,
                    prepare_scene,
                    compose_pixels,
                    publish_scene,
                )
                    .chain(),
            );
    }
}

pub fn receive_platform(
    mut events: ResMut<PlatformEvents>,
    mut host: ResMut<GameHost>,
    mut bank: ResMut<ImageBank>,
    mut requests: ResMut<PlatformRequests>,
) {
    if !host.init_sent {
        host.init_sent = true;
        host.request(
            &mut requests,
            "init",
            json!({"testApi":cfg!(feature="test-api")}),
        );
    }
    while let Some(PlatformEvent {
        id,
        operation,
        value,
        error,
        bytes,
    }) = events.0.pop_front()
    {
        if matches!(operation.as_str(), "load" | "import") {
            // A file chooser or storage read can finish after the player has
            // left Saves or requested another slot. Only the current request
            // may replace the Bevy-owned session.
            if host.restore_request != Some(id) {
                continue;
            }
            host.restore_request = None;
        }
        if let Some(error) = error {
            if ["asset", "videoFrame"].contains(&operation.as_str())
                && value["generation"]
                    .as_u64()
                    .is_some_and(|generation| generation != u64::from(host.generation))
            {
                if operation == "asset" {
                    if let Some(resource) = value["resourceId"].as_str() {
                        // The obsolete download must stop owning this id, or
                        // revisiting its scene would wait forever on a request
                        // whose error was deliberately discarded.
                        bank.requested.remove(resource);
                    }
                }
                continue;
            }
            if [
                "save",
                "load",
                "import",
                "export",
                "settingsSet",
                "audioUnlock",
                "audioUnlocked",
            ]
            .contains(&operation.as_str())
            {
                host.notice = error;
                continue;
            }
            if operation == "asset" {
                if let Some(resource) = value["resourceId"].as_str() {
                    bank.requested.remove(resource);
                    bank.failures.insert(resource.into(), error.clone());
                }
            }
            host.error = Some(error);
            continue;
        }
        match operation.as_str() {
            "init" => {
                let mut content = value["maps"].clone();
                content["rules"] = value["rules"].clone();
                content["events"] = value["events"].clone();
                content["questionnaires"] = value["questionnaires"].clone();
                match GameSession::from_content(content) {
                    Ok(session) => {
                        host.snapshot = serde_json::to_value(session.snapshot()).unwrap();
                        host.session = Some(session);
                        host.seed = value["seed"].as_u64().unwrap_or(1);
                        host.ui.volume =
                            value["settings"]["volume"].as_u64().unwrap_or(50).min(100) as u8;
                        host.error = None;
                        host.notice.clear();
                        host.catalogs = value;
                    }
                    Err(error) => host.error = Some(error),
                }
                if !host.catalogs["glyphs"].is_null() {
                    match serde_json::from_value::<GlyphAtlas>(host.catalogs["glyphs"].clone()) {
                        Ok(atlas) => bank.glyphs = Some(atlas),
                        Err(error) => host.error = Some(format!("点阵字体目录无效：{error}")),
                    }
                }
            }
            "asset" => {
                if let Some(id) = value["resourceId"].as_str() {
                    if let Some(wave) = crate::audio::WaveInfo::parse(&bytes) {
                        host.catalogs["manifest"]["_audioDurations"][id] = json!(
                            wave.duration().as_millis() as u64
                                + u64::from(wave.duration().subsec_nanos() % 1_000_000 != 0)
                        );
                    }
                    if id == crate::font::UI_FONT_RESOURCE {
                        if let Some(glyphs) = bank.glyphs.as_mut() {
                            if let Err(error) = glyphs.set_pixels(bytes) {
                                host.error = Some(error);
                                continue;
                            }
                        }
                        bank.insert(id.into(), Vec::new());
                    } else {
                        bank.insert(id.into(), bytes);
                    }
                }
            }
            "load" | "import" => {
                if value.is_null() {
                    host.notice = "尚无本版本存档，请开始新游戏。".into();
                    continue;
                }
                if let Some(payload) = value["payload"].as_str() {
                    let (domain, presentation) = match decode_save(payload) {
                        Ok(save) => save,
                        Err(error) => {
                            host.notice = error;
                            continue;
                        }
                    };
                    if let Some(session) = host.session.as_mut() {
                        let response = session.restore(&domain);
                        let success = response.ok;
                        host.response(response);
                        if success {
                            let volume = host.ui.volume;
                            let mut presentation = presentation.unwrap_or_default();
                            presentation.volume = volume;
                            host.ui = presentation;
                            let completed = host.snapshot["phase"] == "completed";
                            host.set_screen(if completed {
                                Screen::Menu
                            } else {
                                Screen::Game
                            });
                            host.notice = if completed {
                                "这段人生已经结束，请开始新游戏。".into()
                            } else {
                                "已恢复进度。".into()
                            };
                            if operation == "import" {
                                host.save(&mut requests);
                            }
                        }
                    }
                }
            }
            "save" => {
                if host.ui.screen == Screen::Saves {
                    host.notice = "进度已保存。".into();
                }
            }
            "export" => host.notice = "存档已导出。".into(),
            "visibility" => {
                let hidden = value["hidden"].as_bool().unwrap_or(false);
                if host.hidden != hidden {
                    host.discard_next_frame = true;
                }
                host.hidden = hidden;
                host.frame_ms = 0;
                host.ui.pressed = None;
                host.ui.hover = None;
                if host.hidden {
                    if matches!(
                        host.snapshot["pending"]["game"].as_str(),
                        Some("fist" | "firebee")
                    ) {
                        host.send(json!({"type":"minigameAction","action":"input","left":false,"right":false,"fire":false}));
                    }
                    if host.snapshot["pending"]["game"] == "fire" {
                        let rest = host.snapshot["pending"]["state"]["bodyRest"]
                            .as_i64()
                            .unwrap_or(0);
                        if rest == 1 || rest == 2 {
                            host.send(json!({"type":"minigameAction","action":if rest==1{"releaseLeft"}else{"releaseRight"}}));
                        }
                    }
                    host.save(&mut requests);
                }
            }
            "ime" => {
                if value["generation"]
                    .as_u64()
                    .is_some_and(|generation| generation != u64::from(host.generation))
                {
                    continue;
                }
                if !value["composing"].as_bool().unwrap_or(false) {
                    if let Some(text) = value["text"].as_str() {
                        host.ui.profile["name"] = json!(text.chars().take(64).collect::<String>());
                    }
                }
                if value["active"] == false {
                    host.ui.profile["_imeActive"] = json!(false);
                }
            }
            "videoFrame" => {
                let resource = value["resourceId"].as_str().unwrap_or("").to_owned();
                let frame = value["frame"].as_u64().unwrap_or(0);
                if value["generation"].as_u64() == Some(host.generation as u64) {
                    let width = value["width"].as_u64().unwrap_or(0) as u32;
                    let height = value["height"].as_u64().unwrap_or(0) as u32;
                    if bytes.len() == width as usize * height as usize * 4 {
                        bank.video = Some((
                            resource,
                            frame,
                            std::sync::Arc::new(PixelImage {
                                width,
                                height,
                                rgba: bytes,
                                indices: None,
                            }),
                        ));
                        bank.revision = bank.revision.wrapping_add(1);
                    }
                    host.video_request = None;
                }
            }
            #[cfg(feature = "test-api")]
            "testInput" => {
                if let Some(ms) = value["frameMs"].as_u64() {
                    host.test_clock = true;
                    host.manual_frame = Some(ms.min(u32::MAX as u64) as u32);
                }
                if let Some(control) = value["controlId"].as_str() {
                    host.ui.profile["_testControl"] = json!(control);
                }
                if value["key"].is_string() {
                    host.ui.profile["_testKey"] = value;
                }
            }
            _ => {}
        }
    }
}

pub fn apply_actions(
    mut host: ResMut<GameHost>,
    mut actions: ResMut<Actions>,
    mut requests: ResMut<PlatformRequests>,
    mut bank: ResMut<ImageBank>,
) {
    while let Some(action) = actions.0.pop_front() {
        match action {
            Action::Game(value) => {
                if !host.loading && !host.hidden && host.error.is_none() {
                    host.ui.profile["_diceChoices"] = json!(false);
                    host.send(value);
                    host.save(&mut requests);
                }
            }
            Action::Ui(action) => {
                match action {
                    UiAction::MainMenu => {
                        host.save(&mut requests);
                        host.restore_request = None;
                        host.ui.runtime.motion = None;
                        host.set_screen(Screen::Menu);
                        host.ui.profile["_imeActive"] = json!(false);
                        host.request(&mut requests, "ime", json!({"active":false}));
                    }
                    UiAction::NewProfile => {
                        host.notice.clear();
                        let volume = host.ui.volume;
                        host.ui = PresentationState::default();
                        host.ui.volume = volume;
                        host.return_screen = Screen::Menu;
                        host.set_screen(Screen::Profile);
                        let defaults =
                            host.catalogs["manifest"]["_nativeProfile"]["defaults"].clone();
                        if defaults.is_object() {
                            host.ui.profile = defaults;
                        } else {
                            host.ui.profile["name"] = json!("丁马克");
                        }
                    }
                    UiAction::StartGame => {
                        let profile = json!({"name":host.ui.profile["name"],"sex":host.ui.profile["sex"],"birthMonth":host.ui.profile["birthMonth"],"birthDay":host.ui.profile["birthDay"],"bloodType":host.ui.profile["bloodType"]});
                        let seed = host.seed;
                        host.send(json!({"type":"newGame","profile":profile,"seed":seed}));
                        if host.error.is_none()
                            && host.ui.profile["_nativeError"].is_null()
                            && !host.snapshot["player"].is_null()
                        {
                            host.set_screen(Screen::Game);
                            host.save(&mut requests);
                        }
                        host.ui.profile["_imeActive"] = json!(false);
                        host.request(&mut requests, "ime", json!({"active":false}));
                    }
                    UiAction::OpenSaves => {
                        host.return_screen = host.ui.screen;
                        host.set_screen(Screen::Saves);
                    }
                    UiAction::Save => host.save(&mut requests),
                    UiAction::Load => {
                        let identity = host
                            .session
                            .as_ref()
                            .map(|s| s.content_identity().to_owned());
                        host.request(
                            &mut requests,
                            "load",
                            json!({"slot":"current","contentId":identity}),
                        );
                    }
                    UiAction::LoadPrevious => {
                        let identity = host
                            .session
                            .as_ref()
                            .map(|s| s.content_identity().to_owned());
                        host.request(
                            &mut requests,
                            "load",
                            json!({"slot":"previous","contentId":identity}),
                        );
                    }
                    UiAction::Import => {
                        let identity = host
                            .session
                            .as_ref()
                            .map(|s| s.content_identity().to_owned());
                        host.request(&mut requests, "import", json!({"contentId":identity}));
                    }
                    UiAction::Export => {
                        let identity = host
                            .session
                            .as_ref()
                            .map(|s| s.content_identity().to_owned());
                        host.request(
                            &mut requests,
                            "export",
                            json!({"slot":"current","contentId":identity}),
                        );
                    }
                    UiAction::Options => {
                        host.return_screen = host.ui.screen;
                        host.ui.profile["_optionsPage"] = json!("menu");
                        host.set_screen(Screen::Options);
                    }
                    UiAction::MapOverview => {
                        host.return_screen = host.ui.screen;
                        host.set_screen(Screen::MapOverview);
                    }
                    UiAction::Back => {
                        if host.error.take().is_some() {
                            host.generation = host.generation.wrapping_add(1);
                            bank.failures.clear();
                            bank.requested.clear();
                            host.video_request = None;
                            if host.catalogs.is_null() {
                                host.init_sent = false;
                            }
                        } else {
                            let screen = host.return_screen;
                            host.set_screen(screen);
                        }
                    }
                    UiAction::SetField { field, value } => {
                        if field == "_nativePanel" {
                            host.ui.profile["_nativeError"] = Value::Null;
                        }
                        host.ui.profile[&field] = value;
                    }
                    UiAction::SetVolume(level) => {
                        host.ui.volume = level.clamp(0, 100) as u8;
                        let volume = host.ui.volume;
                        host.request(
                            &mut requests,
                            "settingsSet",
                            json!({"key":"volume","value":volume}),
                        );
                    }
                    UiAction::AdjustAllocation { field, delta } => {
                        let value = host.ui.allocations[&field].as_i64().unwrap_or(0);
                        let total = host
                            .ui
                            .allocations
                            .as_object()
                            .map(|v| v.values().filter_map(Value::as_i64).sum::<i64>())
                            .unwrap_or(0);
                        let available = host.snapshot["pending"]["points"].as_i64().unwrap_or(0);
                        if delta < 0 && value > 0 || delta > 0 && total < available {
                            host.ui.allocations[&field] = json!((value + delta as i64).max(0));
                        }
                    }
                    UiAction::ConfirmAllocation => {
                        let allocations = host.ui.allocations.clone();
                        host.send(json!({"type":"allocateStats","allocations":allocations}));
                        host.save(&mut requests);
                    }
                    UiAction::AutoAllocate => {
                        host.ui.allocations = host.snapshot["pending"]["defaults"].clone();
                    }
                    UiAction::EditName => {
                        host.ui.profile["_imeActive"] = json!(true);
                        let name = host.ui.profile["name"].clone();
                        let generation = host.generation;
                        host.request(&mut requests,"ime",json!({"active":true,"text":name,"maxLength":64,"generation":generation}));
                    }
                }
            }
        }
    }
}

pub fn advance_game(time: Res<Time<bevy::time::Real>>, mut host: ResMut<GameHost>) {
    // GetTickCount quantizes a global time endpoint. Quantizing every delta
    // separately loses a fraction on every frame (60 * 16.666ms != 960ms).
    // Record endpoints even while paused so resuming cannot catch up.
    let now_ms = time.elapsed().as_millis();
    let previous_ms = host
        .last_real_ms
        .replace(now_ms)
        .unwrap_or_else(|| time.elapsed().saturating_sub(time.delta()).as_millis());
    let elapsed = if host.test_clock {
        host.manual_frame.take().unwrap_or(0)
    } else {
        now_ms.saturating_sub(previous_ms).min(u32::MAX as u128) as u32
    };
    let discard = std::mem::take(&mut host.discard_next_frame);
    let frozen = discard || host.hidden || host.loading || host.error.is_some();
    host.frame_ms = if frozen { 0 } else { elapsed };
    // A real visible frame still performs the original frame-based work when
    // the millisecond endpoint has not changed. Test clock 0 explicitly freezes.
    if frozen || host.test_clock && host.frame_ms == 0 {
        return;
    }
    host.ui.time_ms += u64::from(host.frame_ms);
    host.ui.scene_ms += u64::from(host.frame_ms);
    if host.ui.screen == Screen::Game && host.ui.runtime.motion.is_none() {
        let content = SceneContent {
            manifest: &host.catalogs["manifest"],
            maps: &host.catalogs["maps"],
            rules: &host.catalogs["rules"],
        };
        if let Some(mut tick) = presentation::compose(&content, &host.snapshot, &host.ui).tick {
            tick["elapsedMs"] = json!(host.frame_ms);
            host.send(tick);
        }
    }
    if !host.catalogs.is_null() {
        let host = &mut *host;
        let elapsed = host.frame_ms;
        let content = SceneContent {
            manifest: &host.catalogs["manifest"],
            maps: &host.catalogs["maps"],
            rules: &host.catalogs["rules"],
        };
        presentation::update_presentation(&content, &host.snapshot, &mut host.ui, elapsed);
    }
}

pub fn prepare_scene(
    mut host: ResMut<GameHost>,
    mut current: ResMut<CurrentScene>,
    mut bank: ResMut<ImageBank>,
    mut requests: ResMut<PlatformRequests>,
    mut actions: ResMut<Actions>,
) {
    if host.catalogs.is_null() {
        return;
    }
    let content = SceneContent {
        manifest: &host.catalogs["manifest"],
        maps: &host.catalogs["maps"],
        rules: &host.catalogs["rules"],
    };
    let mut frame = presentation::compose(&content, &host.snapshot, &host.ui);
    let mut resources: HashSet<String> = frame
        .required_resources
        .iter()
        .map(|id| content.resolve(id))
        .collect();
    for sp in &frame.sprites {
        let meta = content.media(&sp.resource_id);
        let width = sp
            .width
            .unwrap_or(meta["width"].as_u64().unwrap_or(640) as u32);
        let height = sp
            .height
            .unwrap_or(meta["height"].as_u64().unwrap_or(480) as u32);
        if sp.x < 640 && sp.y < 480 && sp.x + width as i32 > 0 && sp.y + height as i32 > 0 {
            resources.insert(content.resolve(&sp.resource_id));
        }
        if let Some(p) = &sp.palette {
            resources.insert(content.resolve(p));
        }
    }
    for s in &frame.sounds {
        resources.insert(content.resolve(&s.resource_id));
    }
    for (p, _, _) in &frame.palette_sections {
        resources.insert(content.resolve(p));
    }
    for over in &frame.palette_overrides {
        resources.insert(content.resolve(&over.resource_id));
    }
    if let Some(p) = &frame.palette {
        resources.insert(content.resolve(p));
    }
    if let Some(summary) = &frame.summary {
        resources.insert(content.resolve(&summary.actor_resource));
        resources.insert(content.resolve(&summary.portrait_resource));
    }
    if !frame.texts.is_empty() {
        resources.insert("install/font/ascfont.16".into());
        resources.insert("install/font/stdfont.16".into());
        if bank.glyphs.is_some() {
            resources.insert(crate::font::UI_FONT_RESOURCE.into());
        }
    }
    if host.scene_key != frame.key {
        host.scene_key = frame.key.clone();
        host.ui.scene_ms = 0;
        host.generation = host.generation.wrapping_add(1);
        host.video_request = None;
    }
    let next_video = frame.video.as_ref().map(|video| video.resource_id.clone());
    if host.video_resource != next_video {
        // Decoder objects belong to the current scene/clip. In particular,
        // subtitles and the defeat/victory outro share a domain phase but
        // must not share a decoder generation or retain its old DOM video.
        if host.video_resource.is_some() {
            host.generation = host.generation.wrapping_add(1);
            host.request(&mut requests, "videoClose", Value::Null);
        }
        host.video_resource = next_video;
        host.video_request = None;
        bank.video = None;
        bank.revision = bank.revision.wrapping_add(1);
    }
    let mut missing = false;
    for id in &resources {
        if !bank.has(id) {
            missing = true;
            if !bank.requested.contains(id) && !bank.failures.contains_key(id) {
                bank.requested.insert(id.clone());
                let generation = host.generation;
                host.request(
                    &mut requests,
                    "asset",
                    json!({"resourceId":id,"generation":generation}),
                );
            }
        }
    }
    if let Some((_, error)) = bank.failures.iter().next() {
        host.error = Some(error.clone());
    }
    if let Some(video) = &frame.video {
        let desired = video.time_ms * u64::from(video.fps.max(1)) / 1000;
        let ready = bank
            .video
            .as_ref()
            .is_some_and(|(id, frame, _)| *id == video.resource_id && *frame == desired);
        if !ready {
            missing = true;
            let wanted = (video.resource_id.clone(), desired);
            if host.video_request.as_ref() != Some(&wanted) {
                let generation = host.generation;
                host.request(&mut requests,"videoFrame",json!({"resourceId":video.resource_id,"frame":desired,"fps":video.fps,"generation":generation}));
                host.video_request = Some(wanted);
            }
        }
    }
    host.ui.profile["_cursorHidden"] = json!(false);
    for request in &frame.requests {
        match request {
            SceneRequest::AfterDelay {
                elapsed_ms,
                delay_ms,
                action,
            } => {
                if !host.hidden && !missing && host.error.is_none() && *elapsed_ms >= *delay_ms {
                    actions.0.push_back(action.clone());
                }
            }
            SceneRequest::Prefetch(ids) => {
                if *ids != host.last_prefetch {
                    host.last_prefetch = ids.clone();
                    host.request(&mut requests, "prefetch", json!({"resources":ids}));
                }
            }
            SceneRequest::AfterAnimation {
                resource_id,
                elapsed_ms,
                action,
            } => {
                resources.insert(resource_id.clone());
                if let Some(count) = bank.frame_count(resource_id) {
                    if !host.hidden
                        && !missing
                        && host.error.is_none()
                        && *elapsed_ms > u64::from(count) * u64::from(bank.delay(resource_id))
                    {
                        actions.0.push_back(action.clone());
                    }
                }
            }
            SceneRequest::CursorHidden(hidden) => {
                host.ui.profile["_cursorHidden"] = json!(hidden);
            }
        }
    }
    bank.retain_scene(&resources);
    host.loading = missing;
    if missing {
        frame.description = "正在读取原版资源…".into();
    }
    if !host.notice.is_empty()
        && matches!(
            host.ui.screen,
            Screen::Profile | Screen::Saves | Screen::Options
        )
    {
        frame.texts.push(SceneText {
            id: "notice".into(),
            text: host.notice.clone(),
            bounds: Rect::new(72, 408, 480, 24),
            color: [255, 230, 180, 255],
            z: 9500,
            line_height: 20,
            ..Default::default()
        });
    }
    if let Some(error) = &host.error {
        frame.fills.push(SceneFill {
            id: "error-panel".into(),
            bounds: Rect::new(80, 145, 480, 190),
            color: [0, 0, 0, 240],
            z: 10_000,
            ..Default::default()
        });
        frame.texts.push(SceneText {
            id: "error-text".into(),
            text: error.clone(),
            bounds: Rect::new(96, 162, 448, 110),
            color: [255; 4],
            z: 10_001,
            line_height: 22,
            ..Default::default()
        });
        frame.texts.push(SceneText {
            id: "retry-label".into(),
            text: "重试 / 返回".into(),
            bounds: Rect::new(232, 290, 180, 24),
            color: [255; 4],
            z: 10_001,
            line_height: 22,
            ..Default::default()
        });
        frame.controls.push(SceneControl {
            id: "retry".into(),
            label: "重试 / 返回".into(),
            bounds: Rect::new(220, 282, 200, 38),
            action: Action::Ui(UiAction::Back),
            enabled: true,
            z: 10_002,
            keys: vec!["Escape".into(), "Enter".into()],
            on_press: None,
            on_release: None,
            on_hover: None,
            on_leave: None,
        });
    }
    #[cfg(feature = "test-api")]
    if let Some(id) = host.ui.profile["_testControl"].as_str() {
        if let Some(control) = frame.controls.iter().find(|c| c.id == id && c.enabled) {
            actions.0.push_back(control.action.clone());
        }
        host.ui.profile["_testControl"] = Value::Null;
    }
    current.0 = frame;
}

pub use crate::render::compose_pixels;

/// The last diagnostic projection belongs to this Bevy system, not game state.
#[derive(Default)]
pub struct PublishedSceneCache(Option<SceneDiagnostics>);

#[derive(serde::Serialize)]
struct DiagnosticControl {
    id: String,
    label: String,
    bounds: Rect,
    enabled: bool,
}

#[derive(serde::Serialize)]
struct SceneDiagnostics {
    screen: Screen,
    description: String,
    controls: Vec<DiagnosticControl>,
    loading: bool,
    error: Option<String>,
    notice: String,
    #[cfg(feature = "test-api")]
    snapshot: Value,
}

impl SceneDiagnostics {
    fn matches(&self, host: &GameHost, frame: &SceneFrame) -> bool {
        self.screen == host.ui.screen
            && self.description == frame.description
            && self.loading == host.loading
            && self.error == host.error
            && self.notice == host.notice
            && self.controls.len() == frame.controls.len()
            && self.controls.iter().zip(&frame.controls).all(|(old, new)| {
                old.id == new.id
                    && old.label == new.label
                    && old.bounds == new.bounds
                    && old.enabled == new.enabled
            })
            && {
                #[cfg(feature = "test-api")]
                {
                    same_json_representation(&self.snapshot, &host.snapshot)
                }
                #[cfg(not(feature = "test-api"))]
                {
                    true
                }
            }
    }

    fn capture(host: &GameHost, frame: &SceneFrame) -> Self {
        Self {
            screen: host.ui.screen,
            description: frame.description.clone(),
            controls: frame
                .controls
                .iter()
                .map(|control| DiagnosticControl {
                    id: control.id.clone(),
                    label: control.label.clone(),
                    bounds: control.bounds,
                    enabled: control.enabled,
                })
                .collect(),
            loading: host.loading,
            error: host.error.clone(),
            notice: host.notice.clone(),
            #[cfg(feature = "test-api")]
            snapshot: host.snapshot.clone(),
        }
    }
}

pub fn publish_scene(
    host: Res<GameHost>,
    current: Res<CurrentScene>,
    mut requests: ResMut<PlatformRequests>,
    mut last: Local<PublishedSceneCache>,
) {
    // The upstream systems also mutate animation/input fields that diagnostics
    // do not expose. Check this projection before allocating any JSON, even
    // when those writes mark the entire resources changed on a stable screen.
    if last.0.is_some()
        && ((!host.is_changed() && !current.is_changed())
            || last
                .0
                .as_ref()
                .is_some_and(|previous| previous.matches(&host, &current.0)))
    {
        return;
    }
    let state = SceneDiagnostics::capture(&host, &current.0);
    let payload = serde_json::to_value(&state).expect("Scene diagnostics serialize");
    last.0 = Some(state);
    requests.0.push_back(PlatformRequest {
        id: 0,
        operation: "scene".into(),
        payload,
    });
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<GameHost>()
            .init_resource::<CurrentScene>()
            .init_resource::<PlatformRequests>()
            .add_systems(Update, publish_scene);
        app
    }

    fn control(id: &str) -> SceneControl {
        SceneControl {
            id: id.into(),
            label: format!("控件 {id}"),
            bounds: Rect::new(-1, 20, 100, 40),
            action: Action::Ui(UiAction::Back),
            enabled: true,
            z: 4,
            keys: vec!["Enter".into()],
            on_press: None,
            on_release: None,
            on_hover: None,
            on_leave: None,
        }
    }

    // Keep the old wire projection as the oracle, including its serialized
    // change criterion. The production cache must publish on the same frames.
    fn previous_payload(host: &GameHost, current: &SceneFrame) -> Value {
        let controls: Vec<Value> = current
            .controls
            .iter()
            .map(|c| json!({"id":c.id,"label":c.label,"bounds":c.bounds,"enabled":c.enabled}))
            .collect();
        let state = json!({"screen":format!("{:?}",host.ui.screen),"description":current.description,"controls":controls,"loading":host.loading,"error":host.error,"notice":host.notice});
        #[cfg(feature = "test-api")]
        let state = {
            let mut state = state;
            state["snapshot"] = host.snapshot.clone();
            state
        };
        state
    }

    fn check_frame(app: &mut App, previous: &mut Option<String>) {
        let expected = previous_payload(
            app.world().resource::<GameHost>(),
            &app.world().resource::<CurrentScene>().0,
        )
        .to_string();
        let changed = previous.as_ref() != Some(&expected);
        app.update();
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<PlatformRequests>()
            .0
            .drain(..)
            .collect();
        assert_eq!(requests.len(), usize::from(changed));
        if changed {
            assert_eq!(requests[0].id, 0);
            assert_eq!(requests[0].operation, "scene");
            assert_eq!(requests[0].payload.to_string(), expected);
        }
        *previous = Some(expected);
    }

    #[test]
    fn diagnostic_cache_preserves_the_previous_payload_and_publication_frames() {
        let mut app = app();
        let mut previous = None;
        app.world_mut().resource_mut::<CurrentScene>().0.controls =
            vec![control("first"), control("second")];
        check_frame(&mut app, &mut previous);
        check_frame(&mut app, &mut previous);
        for screen in [
            Screen::Profile,
            Screen::Game,
            Screen::Options,
            Screen::Saves,
            Screen::MapOverview,
            Screen::Menu,
        ] {
            app.world_mut().resource_mut::<GameHost>().ui.screen = screen;
            check_frame(&mut app, &mut previous);
        }
        app.world_mut().resource_mut::<GameHost>().loading = false;
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().error = Some("资源错误".into());
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().error = None;
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().notice = "保存成功。".into();
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<CurrentScene>().0.description = "场景说明".into();
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<CurrentScene>().0.controls[0].id = "changed".into();
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<CurrentScene>().0.controls[0].label = "新名称".into();
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<CurrentScene>().0.controls[0].bounds =
            Rect::new(23, -40, 90, 31);
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<CurrentScene>().0.controls[0].enabled = false;
        check_frame(&mut app, &mut previous);
        app.world_mut()
            .resource_mut::<CurrentScene>()
            .0
            .controls
            .swap(0, 1);
        check_frame(&mut app, &mut previous);
        app.world_mut()
            .resource_mut::<CurrentScene>()
            .0
            .controls
            .pop();
        check_frame(&mut app, &mut previous);
        app.world_mut()
            .resource_mut::<CurrentScene>()
            .0
            .controls
            .push(control("third"));
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().snapshot =
            json!({"lastSeq":9,"pending":{"state":{"clockMs":1}}});
        check_frame(&mut app, &mut previous);
    }

    #[test]
    fn stable_diagnostics_ignore_animation_and_input_resource_writes() {
        let mut app = app();
        let mut previous = None;
        app.world_mut().resource_mut::<CurrentScene>().0.controls = vec![control("first")];
        check_frame(&mut app, &mut previous);
        let requests_changed_at = app
            .world()
            .get_resource_ref::<PlatformRequests>()
            .unwrap()
            .last_changed();
        for frame in 0..120 {
            {
                let mut host = app.world_mut().resource_mut::<GameHost>();
                host.ui.time_ms = frame * 16;
                host.ui.pointer = (frame as i32, 5);
                host.ui.hover = Some("first".into());
                host.ui.profile["_nativePanel"] = json!({"elapsed":frame * 16});
            }
            {
                let mut current = app.world_mut().resource_mut::<CurrentScene>();
                current.0.controls[0].action = Action::Ui(UiAction::SetVolume(frame as i32));
                current.0.controls[0].keys = vec!["Space".into()];
                current.0.controls[0].z = frame as i32;
                current.0.sprites = vec![SceneSprite {
                    frame: frame as u32,
                    ..Default::default()
                }];
            }
            app.update();
            assert!(app.world().resource::<PlatformRequests>().0.is_empty());
            assert_eq!(
                app.world()
                    .get_resource_ref::<PlatformRequests>()
                    .unwrap()
                    .last_changed(),
                requests_changed_at
            );
        }
        check_frame(&mut app, &mut previous);
    }

    #[cfg(feature = "test-api")]
    #[test]
    fn test_diagnostics_publish_every_snapshot_change_without_relying_on_sequence_numbers() {
        let mut app = app();
        let mut previous = None;
        app.world_mut().resource_mut::<GameHost>().snapshot = json!({
            "lastSeq":7,
            "pending":{"state":{"clockMs":99,"values":[0.0,null,{"active":false}]}},
            "nativeView":{"kind":"fortune","text":"完整快照"}
        });
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().snapshot["pending"]["state"]["clockMs"] =
            json!(100);
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().snapshot["pending"]["state"]["values"][0] =
            json!(-0.0);
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().snapshot["pending"]["state"]["values"][0] =
            json!(0.0);
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().snapshot["pending"]["state"]["values"][0] =
            json!(0);
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().snapshot["pending"]["state"]["values"][2]["active"] =
            json!(true);
        check_frame(&mut app, &mut previous);
        app.world_mut().resource_mut::<GameHost>().snapshot["nativeView"] = Value::Null;
        check_frame(&mut app, &mut previous);
        check_frame(&mut app, &mut previous);
    }
}
