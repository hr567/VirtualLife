//! Repeatable CPU benchmark over the same public saves used by browser QA.
//! Run `pnpm test:fixtures` first. Asset loading is outside the timed sections.
use bevy::prelude::*;
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::HashSet,
    hint::black_box,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
use virtual_life_app::{
    pixels::{GlyphAtlas, ImageBank, PixelCanvas},
    presentation,
    runtime::{CurrentScene, compose_pixels},
    scene::{PresentationState, SceneContent, SceneFrame, Screen},
};
use virtual_life_core::GameSession;

struct CountAllocations;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[global_allocator]
static ALLOCATOR: CountAllocations = CountAllocations;
unsafe impl GlobalAlloc for CountAllocations {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}
fn bank(root: &Path, manifest: &Value, scene: &SceneFrame) -> ImageBank {
    let mut bank = ImageBank::default();
    let mut used: HashSet<String> = scene.required_resources.iter().cloned().collect();
    used.extend(
        scene
            .sprites
            .iter()
            .map(|sprite| sprite.resource_id.clone()),
    );
    used.extend(
        scene
            .sprites
            .iter()
            .filter_map(|sprite| sprite.palette.clone()),
    );
    used.extend(scene.texts.iter().filter_map(|text| text.palette.clone()));
    used.extend(scene.fills.iter().filter_map(|fill| fill.palette.clone()));
    used.extend(scene.palette_sections.iter().map(|(id, _, _)| id.clone()));
    used.extend(
        scene
            .palette_overrides
            .iter()
            .map(|over| over.resource_id.clone()),
    );
    used.extend(scene.palette.iter().cloned());
    if let Some(summary) = &scene.summary {
        used.insert(summary.actor_resource.clone());
        used.insert(summary.portrait_resource.clone());
    }
    if !scene.texts.is_empty() {
        used.insert("install/font/ascfont.16".into());
        used.insert("install/font/stdfont.16".into());
        used.insert(virtual_life_app::font::UI_FONT_RESOURCE.into());
        let mut atlas: GlyphAtlas =
            serde_json::from_value(read(root.join("assets/catalog/glyphs.json"))).unwrap();
        let hash = manifest["resources"][virtual_life_app::font::UI_FONT_RESOURCE]["sha256"]
            .as_str()
            .unwrap();
        let path = manifest["_objects"][hash]["path"].as_str().unwrap();
        atlas
            .set_pixels(std::fs::read(root.join(path)).unwrap())
            .unwrap();
        bank.glyphs = Some(atlas);
    }
    for id in used {
        let Some(hash) = manifest["resources"][&id]["sha256"].as_str() else {
            bank.failures
                .insert(id, "Resource absent from manifest".into());
            continue;
        };
        let path = manifest["_objects"][hash]["path"].as_str().unwrap();
        bank.insert(id, std::fs::read(root.join(path)).unwrap());
    }
    bank
}
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let iterations = args
        .first()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(40)
        .max(1);
    let mut manifest = read(root.join("assets/manifest.json"));
    manifest["_objects"] = manifest["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|object| {
            (
                object["sha256"].as_str().unwrap().to_owned(),
                object.clone(),
            )
        })
        .collect::<serde_json::Map<String, Value>>()
        .into();
    for (field, file) in [
        ("_nativeUi", "native-ui"),
        ("_nativeProfile", "native-profile"),
        ("_nativeSchool", "native-school"),
        ("_nativeHousing", "native-housing"),
        ("_nativeFortune", "native-fortune"),
        ("_nativeAllocation", "native-allocation"),
    ] {
        manifest[field] = read(root.join(format!("assets/catalog/{file}.json")));
    }
    let maps = read(root.join("assets/catalog/maps.json"));
    let rules = read(root.join("assets/catalog/rules.json"));
    let mut domain = maps.clone();
    domain["rules"] = rules.clone();
    domain["events"] = read(root.join("assets/catalog/events.json"));
    domain["questionnaires"] = read(root.join("assets/catalog/questionnaires.json"));
    let content = SceneContent {
        manifest: &manifest,
        maps: &maps,
        rules: &rules,
    };
    let mut results = Vec::new();
    let record_all = args.get(2).is_some_and(|mode| mode == "--record-all");
    let mut names: Vec<String> = [
        "menu",
        "map-park",
        "activity-native-career-select",
        "activity-native-school-exam-question",
        "activity-minigame-gas-playing",
        "activity-ending-summary",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if record_all {
        names = vec![
            "menu".into(),
            "profile".into(),
            "options".into(),
            "volume".into(),
            "saves".into(),
        ];
        // The manifest excludes obsolete captures left over by previous runs.
        let index = read(root.join("output/bevy-scenarios/manifest.json"));
        let mut saves: Vec<String> = index["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap().to_owned())
            .collect();
        saves.sort();
        names.extend(saves);
        names.push("exit".into());
        for tab in ["special", "equipment"] {
            names.push(format!("inventory@{tab}"));
        }
        for page in ["skills", "careerHistory", "house", "investment"] {
            names.push(format!("status@{page}"));
        }
        for map in [
            "acient", "business", "desert", "lost", "park", "play", "social", "study",
        ] {
            names.push(format!("map-{map}@overview"));
        }
        for entry in index["scenarios"].as_array().unwrap() {
            let id = entry["id"].as_str().unwrap();
            if id.starts_with("activity-native-") {
                names.push(format!("{id}@settled"));
            }
        }
        for category in 0..4 {
            names.push(format!(
                "activity-native-item-shop-browse@category-{category}"
            ));
        }
        names.push("activity-native-stat-shop-browse@confirmation".into());
    }
    for name in names {
        let (base, variant) = name.split_once('@').unwrap_or((&name, ""));
        let mut ui = PresentationState::default();
        let snapshot = if ["menu", "profile", "options", "volume", "saves", "exit"].contains(&base)
        {
            ui.screen = match name.as_str() {
                "profile" => Screen::Profile,
                "options" | "volume" | "exit" => Screen::Options,
                "saves" => Screen::Saves,
                _ => Screen::Menu,
            };
            if name == "volume" || name == "exit" {
                ui.profile["_optionsPage"] = json!(name);
            }
            if name == "profile" {
                ui.profile = content.native_profile()["defaults"].clone();
            }
            Value::Null
        } else {
            let save = read(root.join(format!("output/bevy-scenarios/{base}.json")));
            let mut envelope: Value =
                serde_json::from_str(save["payload"].as_str().unwrap()).unwrap();
            if let Some(presentation) = envelope.as_object_mut().unwrap().remove("presentation") {
                ui = serde_json::from_value(presentation).unwrap();
            }
            ui.screen = Screen::Game;
            let mut session = GameSession::from_content(domain.clone()).unwrap();
            let restored = session.restore(&envelope.to_string());
            if !restored.ok && record_all {
                eprintln!("Skipped invalid save {name}: {:?}", restored.error);
                results.push(json!({"scene":name,"error":restored.error}));
                continue;
            }
            assert!(restored.ok, "Invalid public save {name}: {restored:?}");
            if !variant.is_empty() && (base == "status" || base == "inventory") {
                let before = serde_json::to_value(session.snapshot()).unwrap();
                let mut command = json!({"seq":before["lastSeq"].as_u64().unwrap()+1,"type":format!("{base}Action"),"action":if base=="status"{"page"}else{"tab"}});
                command[if base == "status" { "page" } else { "tab" }] = json!(variant);
                let response = session.send(serde_json::from_value(command).unwrap());
                assert!(response.ok, "{name}: {:?}", response.error);
                // Property pages start with the original rain/mosaic transition.
                // Inspect their completed screen as well as the fixture's entry frame.
                if base == "status" {
                    for _ in 0..600 {
                        let state = serde_json::to_value(session.snapshot()).unwrap();
                        if state["pending"]["state"]["transition"].is_null() {
                            break;
                        }
                        let command = json!({"seq":state["lastSeq"].as_u64().unwrap()+1,"type":"statusTick","elapsedMs":16});
                        let response = session.send(serde_json::from_value(command).unwrap());
                        assert!(response.ok, "{name}: {:?}", response.error);
                    }
                }
            }
            serde_json::to_value(session.snapshot()).unwrap()
        };
        if variant == "overview" {
            ui.screen = Screen::MapOverview;
        }
        if base.starts_with("activity-native-") && !variant.is_empty() {
            for _ in 0..1600 {
                presentation::update_presentation(&content, &snapshot, &mut ui, 16);
            }
            let id = if let Some(category) = variant.strip_prefix("category-") {
                Some(format!("native-category-{category}"))
            } else if variant == "confirmation" {
                Some("native-stat-0".into())
            } else {
                None
            };
            if let Some(id) = id {
                let scene = presentation::compose(&content, &snapshot, &ui);
                let control = scene
                    .controls
                    .iter()
                    .find(|control| control.id == id && control.enabled)
                    .unwrap_or_else(|| panic!("{name}: missing {id}"));
                if let virtual_life_app::scene::Action::Ui(
                    virtual_life_app::scene::UiAction::SetField { field, value },
                ) = &control.action
                {
                    ui.profile[field] = value.clone();
                } else {
                    panic!("{name}: expected a panel selection");
                }
            }
        }
        let scene = presentation::compose(&content, &snapshot, &ui);
        let mut bank = bank(&root, &manifest, &scene);
        let mut pixels = PixelCanvas::default();
        pixels.compose(&scene, &mut bank);
        for sprite in &scene.sprites {
            if bank.image(&sprite.resource_id, sprite.frame).is_none() {
                bank.failures
                    .entry(sprite.resource_id.clone())
                    .or_insert_with(|| format!("Undecodable sprite {}", sprite.id));
            }
        }
        assert!(
            record_all || bank.failures.is_empty(),
            "{name}: {:?}",
            bank.failures
        );
        if record_all {
            let directory = Path::new(args.get(3).expect("--record-all needs an output directory"));
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(
                directory.join(format!("{name}.scene.json")),
                serde_json::to_vec_pretty(&scene).unwrap(),
            )
            .unwrap();
            println!("Rendered {name}");
        }
        let expected = pixels.rgba.clone();
        let resource_errors = bank.failures.clone();
        let mut samples = Vec::with_capacity(iterations as usize);
        let allocations = ALLOCATIONS.load(Ordering::Relaxed);
        let start = Instant::now();
        for _ in 0..iterations {
            let frame_start = Instant::now();
            pixels.compose(black_box(&scene), &mut bank);
            samples.push(frame_start.elapsed().as_secs_f64() * 1_000_000.0);
        }
        let redraw_us = start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(iterations);
        let redraw_allocations = ALLOCATIONS.load(Ordering::Relaxed) - allocations;
        samples.sort_by(f64::total_cmp);
        let redraw_p50 = samples[samples.len() / 2];
        let redraw_p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        assert_eq!(pixels.rgba, expected);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(CurrentScene(scene))
            .insert_resource(bank)
            .insert_resource(pixels)
            .add_systems(Update, compose_pixels);
        app.update();
        let before = app.world().resource::<PixelCanvas>().revision;
        let idle_iterations = iterations * 25;
        let allocations = ALLOCATIONS.load(Ordering::Relaxed);
        let start = Instant::now();
        for _ in 0..idle_iterations {
            app.update();
        }
        let idle_us = start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(idle_iterations);
        let idle_allocations = ALLOCATIONS.load(Ordering::Relaxed) - allocations;
        let pixels = app.world().resource::<PixelCanvas>();
        assert_eq!(pixels.revision, before, "Stable frames must not redraw");
        assert_eq!(pixels.rgba, expected);
        if let (Some(mode), Some(directory)) = (args.get(2), args.get(3)) {
            let directory = Path::new(directory);
            let path = directory.join(format!("{name}.rgba"));
            match mode.as_str() {
                "--record" | "--record-all" => {
                    std::fs::create_dir_all(directory).unwrap();
                    std::fs::write(path, &pixels.rgba).unwrap();
                }
                "--verify" => assert_eq!(
                    pixels.rgba,
                    std::fs::read(path).expect("Missing pre-optimization pixel reference"),
                    "Pixel mismatch for {name}"
                ),
                _ => panic!("Use --record or --verify for pixel references"),
            }
        }
        results.push(json!({"scene":name,"errors":resource_errors,"redrawUs":redraw_us,"redrawP50Us":redraw_p50,"redrawP95Us":redraw_p95,"redrawAllocationsPerFrame":redraw_allocations as f64/f64::from(iterations),"idleUpdateUs":idle_us,"idleAllocationsPerFrame":idle_allocations as f64/f64::from(idle_iterations),"fingerprint":fingerprint(&pixels.rgba),"idleRedraws":pixels.revision-before}));
    }
    let result = json!({"format":"virtual-life-render-benchmark-v1","iterations":iterations,"idleIterations":iterations*25,"results":results});
    let output = serde_json::to_string_pretty(&result).unwrap();
    if let Some(path) = args.get(1) {
        std::fs::write(path, &output).unwrap();
    }
    if record_all {
        let failed = results.iter().any(|result| {
            !result["error"].is_null()
                || result["errors"]
                    .as_object()
                    .is_some_and(|errors| !errors.is_empty())
        });
        println!(
            "Composed {} audited scenes; resource/restore failures: {failed}",
            results.len()
        );
        if failed {
            std::process::exit(1);
        }
    } else {
        println!("{output}");
    }
}
