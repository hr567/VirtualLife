//! The visual projection is independent from input, audio and domain clocks.
//! Bevy's Local cache avoids allocating a JSON render signature on stable frames.
use crate::{pixels::*, runtime::CurrentScene, scene::*};
use bevy::prelude::*;

#[derive(Clone, PartialEq)]
struct VideoPixels {
    resource: String,
    bounds: crate::scene::Rect,
    scanlines: bool,
}
impl VideoPixels {
    fn matches(&self, video: &SceneVideo) -> bool {
        self.resource == video.resource_id
            && self.bounds == video.bounds
            && self.scanlines == video.scanlines
    }
}
struct VisualScene {
    sprites: Vec<SceneSprite>,
    texts: Vec<SceneText>,
    video: Option<VideoPixels>,
    summary: Option<SceneSummary>,
    offset: (i32, i32),
    palette: Option<String>,
    fills: Vec<SceneFill>,
    color_scale: Option<([u8; 2], [f32; 3])>,
    palette_sections: Vec<(String, u8, u8)>,
    palette_rgba: Option<Vec<u8>>,
    palette_overrides: Vec<PaletteOverride>,
}
impl VisualScene {
    fn new(scene: &SceneFrame) -> Self {
        Self {
            sprites: scene.sprites.clone(),
            texts: scene.texts.clone(),
            video: scene.video.as_ref().map(|video| VideoPixels {
                resource: video.resource_id.clone(),
                bounds: video.bounds,
                scanlines: video.scanlines,
            }),
            summary: scene.summary.clone(),
            offset: scene.offset,
            palette: scene.palette.clone(),
            fills: scene.fills.clone(),
            color_scale: scene.color_scale,
            palette_sections: scene.palette_sections.clone(),
            palette_rgba: scene.palette_rgba.clone(),
            palette_overrides: scene.palette_overrides.clone(),
        }
    }
    fn matches(&self, scene: &SceneFrame) -> bool {
        self.sprites == scene.sprites
            && self.texts == scene.texts
            && match (&self.video, &scene.video) {
                (Some(before), Some(now)) => before.matches(now),
                (None, None) => true,
                _ => false,
            }
            && self.summary == scene.summary
            && self.offset == scene.offset
            && self.palette == scene.palette
            && self.fills == scene.fills
            && self.color_scale == scene.color_scale
            && self.palette_sections == scene.palette_sections
            && self.palette_rgba == scene.palette_rgba
            && self.palette_overrides == scene.palette_overrides
    }
}
#[derive(Default)]
pub struct CanvasRenderCache {
    asset_revision: u64,
    visual: Option<VisualScene>,
}
impl CanvasRenderCache {
    fn changed(&mut self, scene: &SceneFrame, asset_revision: u64) -> bool {
        if self.asset_revision == asset_revision
            && self
                .visual
                .as_ref()
                .is_some_and(|visual| visual.matches(scene))
        {
            return false;
        }
        self.asset_revision = asset_revision;
        self.visual = Some(VisualScene::new(scene));
        true
    }
}
pub fn compose_pixels(
    current: Res<CurrentScene>,
    mut pixels: ResMut<PixelCanvas>,
    mut images: ResMut<ImageBank>,
    mut cache: Local<CanvasRenderCache>,
) {
    if cache.changed(&current.0, images.revision) {
        // Dereference ResMut only on a real redraw. Bevy then keeps the Image
        // unchanged, so the existing render-asset extraction skips its upload.
        pixels.compose(&current.0, &mut images);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_projection_ignores_nonpixel_work_and_invalidates_every_visual_input() {
        let mut scene = SceneFrame::default();
        let mut cache = CanvasRenderCache::default();
        assert!(cache.changed(&scene, 0));
        scene.description = "new diagnostic".into();
        scene.sounds.push(SceneSound::default());
        scene.tick = Some(serde_json::json!({"type":"endingTick","elapsedMs":1}));
        assert!(!cache.changed(&scene, 0));
        macro_rules! change {
            ($edit:expr) => {{
                $edit;
                assert!(cache.changed(&scene, 0));
                assert!(!cache.changed(&scene, 0));
            }};
        }
        change!(scene.sprites.push(SceneSprite::default()));
        change!(scene.sprites[0].index_map.push([2, 7]));
        change!(scene.texts.push(SceneText::default()));
        change!(scene.texts[0].color[0] = 42);
        change!(scene.offset.0 = 1);
        change!(scene.palette = Some("palette".into()));
        change!(scene.palette_sections.push(("other".into(), 1, 8)));
        change!(scene.palette_rgba = Some(vec![128; 1024]));
        change!(scene.palette_overrides.push(PaletteOverride::default()));
        change!(scene.fills.push(SceneFill::default()));
        change!(scene.color_scale = Some(([0, 255], [0.5; 3])));
        change!(scene.summary = Some(SceneSummary::default()));
        assert!(cache.changed(&scene, 1));
    }
    #[test]
    fn video_uses_decoded_asset_arrival_not_the_clock_between_frames() {
        let mut scene = SceneFrame {
            video: Some(SceneVideo {
                resource_id: "film".into(),
                fps: 24,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut cache = CanvasRenderCache::default();
        assert!(cache.changed(&scene, 1));
        scene.video.as_mut().unwrap().time_ms = 16;
        assert!(!cache.changed(&scene, 1));
        assert!(cache.changed(&scene, 2));
        scene.video.as_mut().unwrap().scanlines = true;
        assert!(cache.changed(&scene, 2));
        scene.video.as_mut().unwrap().bounds.x = 1;
        assert!(cache.changed(&scene, 2));
        scene.video.as_mut().unwrap().resource_id = "outro".into();
        assert!(cache.changed(&scene, 2));
    }
    #[test]
    fn a_static_frame_does_not_mark_the_bevy_canvas_as_changed() {
        #[derive(Resource, Default)]
        struct Changes(Vec<bool>);
        fn observe(canvas: Res<PixelCanvas>, mut changes: ResMut<Changes>) {
            changes.0.push(canvas.is_changed());
        }
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CurrentScene>()
            .init_resource::<PixelCanvas>()
            .init_resource::<ImageBank>()
            .init_resource::<Changes>()
            .add_systems(Update, (compose_pixels, observe).chain());
        app.update();
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Changes>().0, [true, false, false]);
        assert_eq!(app.world().resource::<PixelCanvas>().revision, 1);
    }
}
