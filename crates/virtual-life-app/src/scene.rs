//! Renderer-independent original 640 × 480 scene description.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Compare the stored JSON representation without allocating a serialized string.
/// In particular, preserve the sign of floating-point zero in cached snapshots.
pub(crate) fn same_json_representation(old: &Value, new: &Value) -> bool {
    match (old, new) {
        (Value::Number(old), Value::Number(new)) if old.is_f64() && new.is_f64() => {
            old.as_f64().unwrap().to_bits() == new.as_f64().unwrap().to_bits()
        }
        (Value::Array(old), Value::Array(new)) => {
            old.len() == new.len()
                && old
                    .iter()
                    .zip(new)
                    .all(|(a, b)| same_json_representation(a, b))
        }
        (Value::Object(old), Value::Object(new)) => {
            old.len() == new.len()
                && old.iter().zip(new).all(|((a_key, a), (b_key, b))| {
                    a_key == b_key && same_json_representation(a, b)
                })
        }
        _ => old == new,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transparency {
    Opaque,
    Index(u8),
    FirstPixel,
}
impl Default for Transparency {
    fn default() -> Self {
        Self::Index(1)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneSprite {
    pub id: String,
    pub resource_id: String,
    pub x: i32,
    pub y: i32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame: u32,
    /// Vertical BMP strip; FLIC frames use the decoder's frame index directly.
    pub frame_count: Option<u32>,
    pub source: Option<Rect>,
    pub clip: Option<Rect>,
    pub transparent: Transparency,
    pub z: i32,
    pub palette: Option<String>,
    pub color_scale: Option<([u8; 2], [f32; 3])>,
    /// N1 background palette effects affect only entries 22..61.
    #[serde(default)]
    pub palette_effect: Option<PaletteEffect>,
    pub repeat: bool,
    /// Original GDI DIB-to-display-palette index mapping, if present.
    pub index_map: Vec<[u8; 2]>,
    /// Presentation-only clock, advanced at most once per visible Bevy frame.
    pub playback: Option<Playback>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaletteEffect {
    pub grayscale: bool,
    pub subtract: u8,
    pub ceiling: u8,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Playback {
    pub interval_ms: u32,
    pub once: bool,
    pub max_advances: u32,
    pub immediate: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneFill {
    pub id: String,
    pub bounds: Rect,
    pub color: [u8; 4],
    pub z: i32,
    pub palette: Option<String>,
    pub palette_index: Option<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PaletteOverride {
    pub resource_id: String,
    pub index: u8,
    pub source_index: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TextFont {
    Original,
    Ascii,
    Gdi { size: u32, bold: bool },
}
impl Default for TextFont {
    fn default() -> Self {
        Self::Original
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneText {
    pub id: String,
    pub text: String,
    pub bounds: Rect,
    pub color: [u8; 4],
    pub z: i32,
    pub font: TextFont,
    pub palette: Option<String>,
    pub palette_index: Option<u8>,
    pub centered: bool,
    pub line_height: u32,
    pub spacing: i32,
    pub clip: Option<Rect>,
    pub edge_palette: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UiAction {
    MainMenu,
    NewProfile,
    StartGame,
    OpenSaves,
    Save,
    Load,
    SelectSave(usize),
    Import(usize),
    Export(usize),
    ConfirmSave,
    CancelSave,
    Options,
    MapOverview,
    Back,
    SetField { field: String, value: Value },
    AdjustAllocation { field: String, delta: i32 },
    AutoAllocate,
    ConfirmAllocation,
    SetVolume(i32),
    EditName,
    AdvancePresentation,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Game(Value),
    Ui(UiAction),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneControl {
    pub id: String,
    pub label: String,
    pub bounds: Rect,
    pub action: Action,
    pub enabled: bool,
    pub z: i32,
    pub keys: Vec<String>,
    pub on_press: Option<Action>,
    pub on_release: Option<Action>,
    pub on_hover: Option<Action>,
    pub on_leave: Option<Action>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneSound {
    pub id: String,
    pub resource_id: String,
    pub looped: bool,
    pub position_ms: Option<u64>,
    pub restart_serial: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneVideo {
    pub resource_id: String,
    pub time_ms: u64,
    pub fps: u32,
    pub bounds: Rect,
    pub scanlines: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneSummary {
    pub actor_resource: String,
    pub actor_frame: u32,
    pub portrait_resource: String,
    pub actor_y: i32,
    pub clip: Rect,
    pub original_palette: Vec<[u8; 3]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SceneRequest {
    AfterDelay {
        elapsed_ms: u64,
        delay_ms: u64,
        action: Action,
    },
    Prefetch(Vec<String>),
    AfterAnimation {
        resource_id: String,
        elapsed_ms: u64,
        action: Action,
    },
    CursorHidden(bool),
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SceneFrame {
    pub key: String,
    pub sprites: Vec<SceneSprite>,
    pub texts: Vec<SceneText>,
    pub controls: Vec<SceneControl>,
    pub sounds: Vec<SceneSound>,
    pub stopped_sounds: Vec<String>,
    /// Assets loaded before this scene accepts input, even before playback.
    #[serde(default)]
    pub required_resources: Vec<String>,
    pub video: Option<SceneVideo>,
    pub requests: Vec<SceneRequest>,
    pub tick: Option<Value>,
    pub offset: (i32, i32),
    pub palette: Option<String>,
    pub fills: Vec<SceneFill>,
    pub color_scale: Option<([u8; 2], [f32; 3])>,
    pub description: String,
    pub palette_sections: Vec<(String, u8, u8)>,
    pub palette_rgba: Option<Vec<u8>>,
    pub summary: Option<SceneSummary>,
    pub palette_overrides: Vec<PaletteOverride>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Screen {
    #[default]
    Menu,
    Profile,
    Game,
    Options,
    Saves,
    MapOverview,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresentationState {
    #[serde(skip)]
    pub saves: SaveMenuState,
    pub screen: Screen,
    pub time_ms: u64,
    pub scene_ms: u64,
    pub hover: Option<String>,
    pub pressed: Option<String>,
    pub pointer: (i32, i32),
    pub camera: (i32, i32),
    pub profile: Value,
    pub allocations: Value,
    pub volume: u8,
    pub runtime: PresentationRuntime,
}
impl Default for PresentationState {
    fn default() -> Self {
        Self {
            saves: SaveMenuState::default(),
            screen: Screen::Menu,
            time_ms: 0,
            scene_ms: 0,
            hover: None,
            pressed: None,
            pointer: (-1, -1),
            camera: (0, 0),
            profile: json!({"name":"","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"}),
            allocations: json!({"intelligence":0,"morality":0,"charm":0,"stamina":0}),
            volume: 50,
            runtime: PresentationRuntime::default(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct SaveMenuState {
    pub parent_return: Screen,
    pub selected: usize,
    pub slots: Value,
    pub ready: bool,
    pub busy: bool,
    pub confirm: bool,
    pub can_save: bool,
}
impl Default for SaveMenuState {
    fn default() -> Self {
        Self {
            parent_return: Screen::Menu,
            selected: 1,
            slots: Value::Null,
            ready: false,
            busy: false,
            confirm: false,
            can_save: false,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PresentationRuntime {
    pub frame_number: u64,
    pub map: MapAnimationState,
    pub animations: std::collections::BTreeMap<String, AnimationClock>,
    pub previous_snapshot: Value,
    pub one_shots: Vec<SceneSound>,
    pub last_key: String,
    pub motion: Option<MotionSequence>,
    pub tracking_round: i32,
    pub tracking_transformed: bool,
    pub competition_answer: Option<i32>,
    pub options_elapsed: u64,
    pub options_frames: [u32; 2],
    pub options_first: bool,
    pub overview_elapsed: u64,
    pub overview_frame: u32,
    pub overview_color: u8,
    pub pattern: PatternAnimationState,
    pub gas_shake_scene: Option<Value>,
    pub relationship_question: String,
    pub preload_key: String,
    pub preload_resources: Vec<String>,
    #[serde(default)]
    pub sequence: Option<NativeSequence>,
    #[serde(default)]
    pub background: BackgroundAnimationState,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BackgroundAnimationState {
    pub key: String,
    pub frame: u32,
    pub started: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeSequence {
    pub snapshot: Value,
    pub cues: std::collections::VecDeque<NativeCue>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NativeCue {
    pub kind: String,
    pub scene: Value,
    pub data: Value,
    pub frame: u32,
    pub elapsed_ms: u64,
    pub started: bool,
    pub serial: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatternAnimationState {
    pub key: String,
    pub elapsed: u64,
    pub counter: u8,
    pub x: i32,
    pub y: i32,
    pub direction: i32,
}
impl Default for PatternAnimationState {
    fn default() -> Self {
        Self {
            key: String::new(),
            elapsed: 0,
            counter: 0,
            x: 0,
            y: 0,
            direction: 1,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MotionSequence {
    pub snapshot: Value,
    pub frames: Vec<MotionPose>,
    pub cursor: usize,
    pub started: bool,
    pub next_pending: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MotionPose {
    pub actor_resource: Option<String>,
    pub actor_position: Option<(i32, i32)>,
    pub actor_frame: u32,
    pub die: Option<(String, u32)>,
    pub facing: u8,
    pub walking: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnimationClock {
    pub resource: String,
    pub frame: u32,
    pub elapsed_ms: u64,
    pub initial: bool,
    pub finished: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapAnimationState {
    pub map_id: String,
    pub load_serial: i64,
    pub clock: u64,
    pub decorations: Vec<MapDecoration>,
    pub cycles: Vec<PaletteCycle>,
    pub spirit_frame: u32,
    pub spirit_elapsed: u64,
    pub cannon_frame: u32,
    pub cannon_elapsed: u64,
    pub cannon_animating: bool,
    pub cannon_wrapped: bool,
    pub cannon_visible: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapDecoration {
    pub id: String,
    pub resource: String,
    pub position: (i32, i32),
    pub dimensions: (i32, i32),
    pub foreground: bool,
    pub frame: u32,
    pub displayed_frame: u32,
    pub frames: u32,
    pub interval: u64,
    pub repeat: bool,
    pub delay: u64,
    pub last_tick: Option<u64>,
    pub initial: bool,
    pub playing: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PaletteCycle {
    pub first: u8,
    pub last: u8,
    pub kind: u8,
    pub interval: u64,
    pub last_tick: Option<u64>,
    pub position: u8,
    pub direction: i8,
    pub mapping: Vec<u8>,
}
pub struct SceneContent<'a> {
    pub manifest: &'a Value,
    pub maps: &'a Value,
    pub rules: &'a Value,
}
impl SceneContent<'_> {
    pub fn native_ui(&self) -> &Value {
        &self.manifest["_nativeUi"]
    }
    pub fn native_profile(&self) -> &Value {
        &self.manifest["_nativeProfile"]
    }
    pub fn native_housing(&self) -> &Value {
        &self.manifest["_nativeHousing"]
    }
    pub fn native_fortune(&self) -> &Value {
        &self.manifest["_nativeFortune"]
    }
    pub fn native_school(&self) -> &Value {
        &self.manifest["_nativeSchool"]
    }
    pub fn native_allocation(&self) -> &Value {
        &self.manifest["_nativeAllocation"]
    }
    pub fn resolve(&self, path: &str) -> String {
        let path = path.replace('\\', "/").to_lowercase();
        let path = path.trim_start_matches("./");
        for candidate in [
            path.to_owned(),
            format!("install/{path}"),
            format!("cd/data/{path}"),
            format!("cd/{path}"),
        ] {
            if self.manifest["resources"].get(&candidate).is_some() {
                return candidate;
            }
        }
        path.to_owned()
    }
    pub fn media(&self, resource: &str) -> Value {
        let resource = self.resolve(resource);
        let hash = self.manifest["resources"][&resource]["sha256"]
            .as_str()
            .unwrap_or("");
        let mut media = self.manifest["_objects"]
            .get(hash)
            .map(|object| object["media"].clone())
            .or_else(|| {
                self.manifest["objects"]
                    .as_array()
                    .and_then(|objects| objects.iter().find(|object| object["sha256"] == hash))
                    .map(|o| o["media"].clone())
            })
            .unwrap_or(Value::Null);
        if media.is_object() {
            if media["frameCount"].is_null() {
                media["frameCount"] = media["frames"].clone();
            }
            if media["frameDelayMs"].is_null() {
                let speed = media["speedRaw"].as_u64().unwrap_or(0);
                media["frameDelayMs"] = json!(if media["magic"] == "af11" {
                    (speed * 1000).div_ceil(70)
                } else {
                    speed
                });
            }
        }
        media
    }
}
