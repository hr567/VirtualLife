//! The Bevy window, input, image and audio lifecycle for the browser client.
use crate::{
    PlatformRequests,
    audio::synchronization::{PlaybackState, WaveCache, out_of_sync},
    pixels::*,
    runtime::*,
    scene::*,
};
use bevy::{
    asset::RenderAssetUsages,
    audio::{PlaybackSettings, Volume},
    camera::ScalingMode,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        renderer::RenderDevice,
    },
    window::WindowResolution,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Resource)]
struct DisplayImage(Handle<Image>);
#[derive(Component)]
struct OriginalCanvas;
struct AudioTrack {
    entity: Entity,
    resource: String,
    serial: u64,
    origin: Duration,
    duration: Duration,
    applied: Option<PlaybackState>,
    wanted: bool,
}
#[derive(Resource, Default)]
struct PlayingAudio(HashMap<String, AudioTrack>);
#[derive(Resource, Default)]
struct AudioWaves(WaveCache<Handle<AudioSource>>);
#[derive(Resource, Default, Clone)]
struct DeviceFailure(Arc<Mutex<Option<String>>>);

pub fn configure(app: &mut App, canvas_2d: bool) {
    let mut plugins = DefaultPlugins
        .build()
        .set(ImagePlugin::default_nearest())
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "虚拟人生".into(),
                canvas: Some("#game-canvas".into()),
                resolution: WindowResolution::new(640, 480),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        });
    if canvas_2d {
        // Keep the existing window, physical input, audio and schedules.
        plugins = plugins
            .disable::<bevy::render::RenderPlugin>()
            .disable::<bevy::core_pipeline::CorePipelinePlugin>()
            .disable::<bevy::post_process::PostProcessPlugin>()
            .disable::<bevy::gizmos_render::GizmoRenderPlugin>()
            .disable::<bevy::sprite_render::SpriteRenderPlugin>()
            .disable::<bevy::ui_render::UiRenderPlugin>();
    }
    app.add_plugins(plugins);
    app.add_plugins(crate::input::InputPlugin)
        .init_resource::<PlayingAudio>()
        .init_resource::<AudioWaves>()
        .init_resource::<DeviceFailure>()
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(PostUpdate, synchronize_audio);
    if !canvas_2d {
        app.add_systems(Startup, setup_canvas)
            .add_systems(
                PreUpdate,
                device_failure.after(crate::input::GameInputSystems),
            )
            .add_systems(PostUpdate, upload_canvas.before(synchronize_audio));
    }
}

fn setup_canvas(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    device: Res<RenderDevice>,
    failure: Res<DeviceFailure>,
) {
    let image = Image::new(
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; WIDTH as usize * HEIGHT as usize * 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let handle = images.add(image);
    commands.spawn((
        Camera2d,
        Msaa::Off,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: WIDTH as f32,
                min_height: HEIGHT as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((
        Sprite {
            image: handle.clone(),
            custom_size: Some(Vec2::new(WIDTH as f32, HEIGHT as f32)),
            ..default()
        },
        OriginalCanvas,
    ));
    commands.insert_resource(DisplayImage(handle));
    let failed = failure.0.clone();
    device
        .wgpu_device()
        .set_device_lost_callback(move |reason, message| {
            if let Ok(mut failed) = failed.lock() {
                *failed = Some(format!(
                    "显示设备已中断（{reason:?}）：{message}。请刷新页面后读取最近成功保存的存档。"
                ));
            }
        });
}
fn upload_canvas(
    pixels: Res<PixelCanvas>,
    display: Option<Res<DisplayImage>>,
    mut images: ResMut<Assets<Image>>,
) {
    if !pixels.is_changed() {
        return;
    }
    let Some(display) = display else { return };
    if let Some(mut image) = images.get_mut(&display.0) {
        if let Some(data) = image.data.as_mut() {
            data.copy_from_slice(&pixels.rgba);
        }
    }
}
fn device_failure(
    failure: Res<DeviceFailure>,
    mut host: ResMut<GameHost>,
    mut requests: ResMut<PlatformRequests>,
) {
    let error = failure.0.lock().ok().and_then(|mut error| error.take());
    if let Some(error) = error {
        host.request(&mut requests, "fatal", serde_json::json!({"message":error}));
        host.error = Some(error);
        host.hidden = true;
    }
}

fn synchronize_audio(
    mut commands: Commands,
    host: Res<GameHost>,
    scene: Res<CurrentScene>,
    bank: Res<ImageBank>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut playing: ResMut<PlayingAudio>,
    mut waves: ResMut<AudioWaves>,
    mut sinks: Query<(&mut PlaybackSettings, Option<&mut AudioSink>)>,
) {
    // Mark and retain in linear time without allocating a wanted map, ended
    // vector or cloned IDs each frame. Last occurrence still wins for an ID.
    for track in playing.0.values_mut() {
        track.wanted = false;
    }
    for sound in &scene.0.sounds {
        if let Some(track) = playing.0.get_mut(&sound.id) {
            track.wanted =
                sound.resource_id == track.resource && sound.restart_serial == track.serial;
        }
    }
    playing.0.retain(|_, track| {
        let keep = track.wanted && !scene.0.stopped_sounds.contains(&track.resource);
        if !keep {
            commands.entity(track.entity).despawn();
        }
        keep
    });
    waves.0.retain(|id| {
        scene.0.sounds.iter().any(|sound| sound.resource_id == id)
            && !scene.0.stopped_sounds.iter().any(|stopped| stopped == id)
    });
    let volume = Volume::Linear(host.ui.volume as f32 / 100.0);
    let game_paused = host.paused();
    let ui_paused = host.hidden
        || host.loading
        || host.error.is_some()
        || !matches!(
            host.ui.screen,
            Screen::Menu | Screen::Profile | Screen::Game
        )
        || host.test_clock && host.frame_ms == 0;
    for sound in &scene.0.sounds {
        let paused = if sound.id.starts_with("ui-") {
            ui_paused
        } else {
            game_paused
        };
        if scene.0.stopped_sounds.contains(&sound.resource_id) {
            continue;
        }
        let Some(wave) = waves.0.resolve(&sound.resource_id, bank.revision, || {
            bank.bytes(&sound.resource_id)
        }) else {
            continue;
        };
        let mut restart = false;
        if let Some(track) = playing.0.get_mut(&sound.id) {
            if let Ok((mut settings, sink)) = sinks.get_mut(track.entity) {
                let state = PlaybackState {
                    volume: host.ui.volume,
                    paused,
                };
                if let Some(mut sink) = sink {
                    // The sink is inserted asynchronously by Bevy. Always apply
                    // its first state, then touch Rodio only when controls change.
                    let changes = state.changes_from(track.applied);
                    if changes.volume.is_some() {
                        sink.set_volume(volume);
                    }
                    if let Some(paused) = changes.paused {
                        if paused { sink.pause() } else { sink.play() }
                    }
                    track.applied = Some(state);
                    if !paused && sound.position_ms.is_some() {
                        let expected = wave
                            .info
                            .position(sound.position_ms.unwrap_or(0), sound.looped);
                        restart = expected.is_none()
                            || out_of_sync(
                                track.origin,
                                sink.position(),
                                track.duration,
                                expected,
                                sound.looped,
                            );
                    }
                } else {
                    // PlaybackSettings controls queued sources, not an already
                    // playing sink. Do not lose changes while a device unlocks.
                    if settings.volume.to_linear() != volume.to_linear() {
                        settings.volume = volume;
                    }
                    if settings.paused != paused {
                        settings.paused = paused;
                    }
                }
            }
        }
        if restart {
            if let Some(track) = playing.0.remove(&sound.id) {
                commands.entity(track.entity).despawn();
            }
        }
        if !playing.0.contains_key(&sound.id) {
            let Some(origin) = wave
                .info
                .position(sound.position_ms.unwrap_or(0), sound.looped)
            else {
                continue;
            };
            let handle = if origin.is_zero() {
                // Repeated UI cues can share the immutable, unpositioned asset.
                // Each player still owns its independent decoder/sink and tail.
                wave.source
                    .get_or_insert_with(|| {
                        sources.add(AudioSource {
                            bytes: wave.bytes.clone(),
                        })
                    })
                    .clone()
            } else {
                let bytes = crate::audio::positioned_wave(
                    wave.bytes.clone(),
                    wave.info,
                    origin,
                    sound.looped,
                );
                sources.add(AudioSource { bytes })
            };
            let settings = PlaybackSettings {
                mode: if sound.looped {
                    bevy::audio::PlaybackMode::Loop
                } else {
                    bevy::audio::PlaybackMode::Once
                },
                volume,
                paused,
                ..default()
            };
            let entity = commands.spawn((AudioPlayer::new(handle), settings)).id();
            playing.0.insert(
                sound.id.clone(),
                AudioTrack {
                    entity,
                    resource: sound.resource_id.clone(),
                    serial: sound.restart_serial,
                    origin,
                    duration: wave.duration,
                    applied: None,
                    wanted: true,
                },
            );
        }
    }
}
