use std::{env, fs, path::{Path, PathBuf}, time::Instant};
use virtual_life_core::{bmp, flic};

fn visit(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() { visit(&path, files)?; }
        else { files.push(path); }
    }
    Ok(())
}

fn main() {
    let root = env::args().nth(1).expect("usage: validate_assets <disc Data directory>");
    let start = Instant::now();
    let mut files = Vec::new(); visit(Path::new(&root), &mut files).unwrap(); files.sort();
    let (mut images, mut animations, mut frames, mut failures) = (0usize, 0usize, 0usize, Vec::new());
    let known: serde_json::Value = serde_json::from_str(include_str!("../../../tests/fixtures/unused-malformed-media.json")).unwrap();
    let mut rejected_unused_originals = Vec::new();
    for path in files {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if extension != "flc" && extension != "bmp" && !(extension.len() == 3 && extension.starts_with('m') && extension[1..].bytes().all(|b| b.is_ascii_digit())) { continue; }
        let bytes = fs::read(&path).unwrap();
        let result = if extension == "flc" {
            animations += 1;
            flic::Decoder::new(&bytes).and_then(|mut decoder| {
                for frame in 0..decoder.frame_count {
                    decoder.next_frame().map_err(|e| format!("frame {frame}: {e}"))?;
                    frames += 1;
                }
                Ok(())
            })
        } else { images += 1; bmp::decode(&bytes).map(|_| ()) };
        if let Err(error) = result {
            let digest = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let matching = known["cases"].as_array().unwrap().iter().find(|case| case["sha256"].as_str() == Some(digest));
            let fnv = bytes.iter().fold(2166136261u32, |sum, byte| (sum ^ u32::from(*byte)).wrapping_mul(16777619));
            if matching.is_some_and(|case| case["bytes"].as_u64() == Some(bytes.len() as u64)
                && case["fnv32"].as_u64() == Some(u64::from(fnv)) && case["error"].as_str() == Some(&error)) {
                rejected_unused_originals.push(serde_json::json!({"path":path.to_string_lossy(),"error":error}));
            } else { failures.push(serde_json::json!({"path":path.to_string_lossy(),"error":error})); }
        }
    }
    println!("{}", serde_json::json!({"images":images,"animations":animations,"frames":frames,"seconds":start.elapsed().as_secs_f64(),"failures":failures,"rejectedUnusedOriginals":rejected_unused_originals}));
    if !failures.is_empty() { std::process::exit(1); }
}
