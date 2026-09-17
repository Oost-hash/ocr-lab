#![allow(dead_code, unused_imports)]

// ─── Imports ──────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::Emitter;

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(serde::Deserialize)]
struct OcrLabConfig {
    paths: OcrLabPaths,
    #[serde(default)]
    warframe: WarframeConfig,
}

#[derive(serde::Deserialize)]
struct OcrLabPaths {
    runs: PathBuf,
}

#[derive(serde::Deserialize)]
struct WarframeConfig {
    ee_log: Option<PathBuf>,
    #[serde(default = "default_trigger_delay_ms")]
    trigger_delay_ms: u64,
}

impl Default for WarframeConfig {
    fn default() -> Self {
        Self {
            ee_log: None,
            trigger_delay_ms: default_trigger_delay_ms(),
        }
    }
}

fn default_trigger_delay_ms() -> u64 {
    500
}

fn runs_dir() -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "OCR Lab root directory not found".to_string())?;
    let config_path = root.join("ocr-lab.toml");
    let config_text = std::fs::read_to_string(&config_path)
        .map_err(|error| format!("Read {}: {error}", config_path.display()))?;
    let config: OcrLabConfig = toml::from_str(&config_text)
        .map_err(|error| format!("Parse {}: {error}", config_path.display()))?;

    Ok(if config.paths.runs.is_absolute() {
        config.paths.runs
    } else {
        root.join(config.paths.runs)
    })
}

fn warframe_config() -> Result<(PathBuf, u64), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "OCR Lab root directory not found".to_string())?;
    let config_path = root.join("ocr-lab.toml");
    let config_text = std::fs::read_to_string(&config_path)
        .map_err(|error| format!("Read {}: {error}", config_path.display()))?;
    let config: OcrLabConfig = toml::from_str(&config_text)
        .map_err(|error| format!("Parse {}: {error}", config_path.display()))?;
    let default_log = dirs::data_local_dir()
        .ok_or_else(|| "Local application-data directory not found".to_string())?
        .join("Warframe")
        .join("EE.log");
    let log_path = config.warframe.ee_log.unwrap_or(default_log);

    Ok((
        if log_path.is_absolute() { log_path } else { root.join(log_path) },
        config.warframe.trigger_delay_ms,
    ))
}

// ─── Modules ──────────────────────────────────────────────────────────────────

#[path = "prod-code/ocr/mod.rs"]
mod ocr;
#[path = "prod-code/ocr_fallback.rs"]
mod ocr_fallback;
#[path = "lab/ocr.rs"]
mod lab_ocr;
#[path = "lab/live.rs"]
mod live;
#[path = "lab/app_state.rs"]
mod app_state;
#[path = "lab/production_modules.rs"]
mod production_modules;
use production_modules::{catalogue, diagnostics, inventory_state, log_watcher, memory_scanner, monitor, relic_pick, wfcd, worldstate};
#[path = "prod-code/platform/mod.rs"]
mod platform;
#[path = "prod-code/reward_watcher.rs"]
mod reward_watcher;
include!(concat!(env!("OUT_DIR"), "/utilities.rs"));

pub(crate) fn append_to_file(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    live::observe_diagnostic(path, text);
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(text.as_bytes())
}

// ─── Structs ──────────────────────────────────────────────────────────────────

pub struct OcrParams<'a> {
    pub pixels: &'a [u8],
    pub pix_w: u32,
    pub pix_h: u32,
    pub game_h: u32,
    pub catalog: &'a [(String, String)],
    pub capture_info: &'a str,
    pub hint_squad_size: Option<usize>,
    pub player_names: &'a [String],
}

// ─── RelicReward type ─────────────────────────────────────────────────────────

pub use wfcd::RelicReward;

#[derive(serde::Serialize)]
struct PipelineStage {
    name: &'static str,
    elapsed_ms: u128,
}

fn record_stage(stages: &mut Vec<PipelineStage>, name: &'static str, started: std::time::Instant) {
    stages.push(PipelineStage {
        name,
        elapsed_ms: started.elapsed().as_millis(),
    });
}

// ─── Catalog loading ──────────────────────────────────────────────────────────

pub fn load_relic_rewards_cache() -> HashMap<String, Vec<RelicReward>> {
    let cache_path = dirs::data_local_dir()
        .map(|d| d.join("frameforge").join("relic_rewards_cache.json"));
    if let Some(path) = cache_path {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(map) = serde_json::from_str(&contents) {
                return map;
            }
        }
    }
    HashMap::new()
}

pub fn build_relic_reward_catalog(
    relic_rewards: &HashMap<String, Vec<RelicReward>>,
) -> Vec<(String, String)> {
    let mut catalog: Vec<(String, String)> = relic_rewards
        .values()
        .flat_map(|rewards| rewards.iter().map(|r| (r.unique_name.clone(), r.name.clone())))
        .filter(|(_, name)| !name.is_empty())
        .collect();
    catalog.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    catalog.dedup_by(|a, b| !a.0.is_empty() && a.0 == b.0);
    catalog
}

// ─── Pipeline ─────────────────────────────────────────────────────────────────

fn run_pipeline_from_bgra(
    mut pixels: Vec<u8>,
    width: u32,
    height: u32,
    source_kind: &str,
    capture_info: String,
    input_is_rgba: bool,
    preprocess: bool,
    started: std::time::Instant,
    mut stages: Vec<PipelineStage>,
) -> Result<serde_json::Value, String> {
    if input_is_rgba {
        let frame_normalize_started = std::time::Instant::now();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        record_stage(&mut stages, "frame_normalize", frame_normalize_started);
    }

    let (frame, fw, fh) = if preprocess {
        let preprocess_started = std::time::Instant::now();
        let (processed, w, h) = ocr::preprocess_for_ocr(&pixels, width, height);
        record_stage(&mut stages, "preprocess", preprocess_started);
        (processed, w, h)
    } else {
        (pixels, width, height)
    };

    let catalog_load_started = std::time::Instant::now();
    let catalog = {
        let rewards = load_relic_rewards_cache();
        build_relic_reward_catalog(&rewards)
    };
    record_stage(&mut stages, "catalog_load", catalog_load_started);

    let extraction = lab_ocr::extract_reward_items_timed(OcrParams {
        pixels: &frame,
        pix_w: fw,
        pix_h: fh,
        game_h: fh,
        catalog: &catalog,
        capture_info: &capture_info,
        hint_squad_size: None,
        player_names: &[],
    });
    stages.push(PipelineStage { name: "bmp_encode", elapsed_ms: extraction.bmp_encode_ms });
    stages.push(PipelineStage { name: "ocr", elapsed_ms: extraction.ocr_ms });
    stages.push(PipelineStage { name: "match", elapsed_ms: extraction.match_ms });

    let result = serde_json::json!({
        "run_id": format!(
            "{}-{}-{}",
            source_kind,
            chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
            RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ),
        "source_kind": source_kind,
        "execution_kind": "lab_instrumented_production_ocr",
        "source_dimensions": { "width": width, "height": height },
        "preprocess": preprocess,
        "is_complete": extraction.is_complete,
        "skip": extraction.skip,
        "items": extraction.items,
        "positions": extraction.positions,
        "debug": extraction.debug,
        "stages": stages,
        "total_ms": started.elapsed().as_millis(),
    });

    Ok(result)
}

pub fn run_pipeline(path: &str, preprocess: bool) -> Result<String, String> {
    let started = std::time::Instant::now();
    let mut stages = Vec::new();

    let image_decode_started = std::time::Instant::now();
    let img = image::open(path).map_err(|e| format!("Failed to open image: {}", e))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();
    record_stage(&mut stages, "image_decode", image_decode_started);

    run_pipeline_from_bgra(
        pixels,
        width,
        height,
        "file_decode",
        format!("file: {path}"),
        true,
        preprocess,
        started,
        stages,
    ).map(|result| result.to_string())
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn recognize_from_file(
    path: String,
    _crop: bool,
    preprocess: bool,
) -> Result<String, String> {
    run_pipeline(&path, preprocess)
}

#[tauri::command]
async fn recognize_virtual_game(preprocess: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        const VIRTUAL_GAME_TITLE: &str = "FrameForge Pipeline Lab - Virtual Game";
        const MAX_ATTEMPTS: u32 = 4;
        let session_started = std::time::Instant::now();
        let session_id = format!(
            "window-{}-{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
            RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        );
        let mut attempts = Vec::new();
        let mut best_result: Option<serde_json::Value> = None;
        let mut best_item_count = 0usize;

        for attempt in 1..=MAX_ATTEMPTS {
            let attempt_started = std::time::Instant::now();
            let capture_started = std::time::Instant::now();
            let capture = lab_ocr::capture_window_reward_area(VIRTUAL_GAME_TITLE);
            let result = match capture {
                Ok((pixels, width, capture_height, _full_height, capture_info)) => {
                    let mut stages = Vec::new();
                    record_stage(&mut stages, "window_capture", capture_started);
                    run_pipeline_from_bgra(
                        pixels,
                        width,
                        capture_height,
                        "window_capture",
                        capture_info,
                        false,
                        preprocess,
                        attempt_started,
                        stages,
                    )?
                }
                Err(error) => {
                    let retry_delay_ms = (attempt < MAX_ATTEMPTS).then_some(500);
                    attempts.push(serde_json::json!({
                        "attempt": attempt,
                        "elapsed_ms": attempt_started.elapsed().as_millis(),
                        "outcome": "capture_failed",
                        "error": error,
                        "retry_delay_ms": retry_delay_ms,
                    }));
                    if let Some(delay_ms) = retry_delay_ms {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                        continue;
                    }
                    break;
                }
            };

            let item_count = result["items"].as_array().map_or(0, Vec::len);
            let is_complete = result["is_complete"].as_bool().unwrap_or(false);
            let skipped = result["skip"].as_bool().unwrap_or(false);
            let retry_delay_ms = if is_complete || skipped || attempt == MAX_ATTEMPTS {
                None
            } else if result["debug"].as_str().is_some_and(|debug| debug.contains("dark-frame")) {
                Some(100)
            } else if result["debug"].as_str().is_some_and(|debug| debug.contains("ocr-empty")) {
                Some(300)
            } else if item_count == 0 {
                Some(700)
            } else {
                Some(400)
            };

            attempts.push(serde_json::json!({
                "attempt": attempt,
                "elapsed_ms": attempt_started.elapsed().as_millis(),
                "outcome": if is_complete { "complete" } else if skipped { "skipped" } else { "partial" },
                "item_count": item_count,
                "retry_delay_ms": retry_delay_ms,
            }));

            if item_count > best_item_count || best_result.is_none() {
                best_item_count = item_count;
                best_result = Some(result.clone());
            }
            if is_complete || skipped {
                break;
            }
            if let Some(delay_ms) = retry_delay_ms {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }

        let mut result = best_result.unwrap_or_else(|| serde_json::json!({
            "source_kind": "window_capture",
            "is_complete": false,
            "skip": false,
            "items": [],
            "positions": [],
            "debug": "Virtual Game capture failed on every attempt",
            "stages": [],
        }));
        result["run_id"] = serde_json::Value::String(session_id);
        result["attempts"] = serde_json::Value::Array(attempts);
        result["attempt_count"] = serde_json::json!(result["attempts"].as_array().map_or(0, Vec::len));
        result["total_ms"] = serde_json::json!(session_started.elapsed().as_millis());
        result["terminal_outcome"] = serde_json::Value::String(
            if result["is_complete"].as_bool().unwrap_or(false) {
                "complete"
            } else if result["skip"].as_bool().unwrap_or(false) {
                "skipped"
            } else if result["items"].as_array().is_some_and(|items| !items.is_empty()) {
                "partial"
            } else {
                "failed"
            }.to_string(),
        );
        Ok(result.to_string())
    })
    .await
    .map_err(|error| format!("Virtual Game capture task failed: {error}"))?
}

pub(crate) fn recognize_warframe_attempt(
    catalog: &[(String, String)],
    hint_squad_size: Option<usize>,
    player_names: &[String],
) -> Result<serde_json::Value, String> {
    let started = std::time::Instant::now();
    let capture_started = std::time::Instant::now();
    let (pixels, width, capture_height, game_height, capture_info) = ocr::capture_warframe_reward_area()
        .ok_or_else(|| "Warframe window not found or capture failed".to_string())?;
    let mut stages = Vec::new();
    record_stage(&mut stages, "window_capture", capture_started);
    let extraction = lab_ocr::extract_reward_items_timed(OcrParams {
        pixels: &pixels,
        pix_w: width,
        pix_h: capture_height,
        game_h: game_height,
        catalog,
        capture_info: &capture_info,
        hint_squad_size,
        player_names,
    });
    stages.push(PipelineStage { name: "bmp_encode", elapsed_ms: extraction.bmp_encode_ms });
    stages.push(PipelineStage { name: "ocr", elapsed_ms: extraction.ocr_ms });
    stages.push(PipelineStage { name: "match", elapsed_ms: extraction.match_ms });
    Ok(serde_json::json!({
        "source_kind": "warframe_capture",
        "execution_kind": "lab_instrumented_production_ocr",
        "source_dimensions": { "width": width, "height": capture_height, "game_height": game_height },
        "preprocess": false,
        "is_complete": extraction.is_complete,
        "skip": extraction.skip,
        "items": extraction.items,
        "positions": extraction.positions,
        "debug": extraction.debug,
        "stages": stages,
        "total_ms": started.elapsed().as_millis(),
    }))
}

#[tauri::command]
fn show_virtual_game_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let window = app.get_webview_window("virtual-screen")
        .ok_or_else(|| "Virtual Game window not found".to_string())?;
    window.set_decorations(false).map_err(|error| error.to_string())?;
    window.set_fullscreen(true).map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn hide_virtual_game_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let window = app.get_webview_window("virtual-screen")
        .ok_or_else(|| "Virtual Game window not found".to_string())?;
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn get_catalog() -> Vec<(String, String)> {
    let rewards = load_relic_rewards_cache();
    build_relic_reward_catalog(&rewards)
}

#[tauri::command]
fn save_screenshot_record(
    filename: String,
    sha256: String,
    result_json: String,
    preprocess: bool,
) -> Result<String, String> {
    let runs_dir = runs_dir()?;

    std::fs::create_dir_all(&runs_dir)
        .map_err(|e| format!("Create runs dir: {}", e))?;

    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let record_dir = runs_dir.join(&timestamp);

    std::fs::create_dir_all(&record_dir)
        .map_err(|e| format!("Create record dir: {}", e))?;

    // Save result JSON
    let result_path = record_dir.join("result.json");
    std::fs::write(&result_path, &result_json)
        .map_err(|e| format!("Write result.json: {}", e))?;

    // Save metadata
    let metadata = serde_json::json!({
        "id": timestamp,
        "filename": filename,
        "sha256": sha256,
        "preprocess": preprocess,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let metadata_path = record_dir.join("metadata.json");
    std::fs::write(&metadata_path, metadata.to_string())
        .map_err(|e| format!("Write metadata.json: {}", e))?;

    Ok(record_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn load_screenshot_history() -> Result<Vec<serde_json::Value>, String> {
    let runs_dir = runs_dir()?;

    if !runs_dir.exists() {
        return Ok(vec![]);
    }

    let mut records = Vec::new();

    for entry in std::fs::read_dir(&runs_dir)
        .map_err(|e| format!("Read runs dir: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Read entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            let metadata_path = path.join("metadata.json");
            if metadata_path.exists() {
                if let Ok(contents) = std::fs::read_to_string(&metadata_path) {
                    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&contents) {
                        records.push(metadata);
                    }
                }
            }
        }
    }

    // Sort by timestamp descending (newest first)
    records.sort_by(|a, b| {
        let ts_a = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let ts_b = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        ts_b.cmp(ts_a)
    });

    Ok(records)
}

#[tauri::command]
fn delete_screenshot_record(id: String) -> Result<(), String> {
    let runs_dir = runs_dir()?.join(&id);

    if runs_dir.exists() {
        std::fs::remove_dir_all(&runs_dir)
            .map_err(|e| format!("Delete record: {}", e))?;
    }

    Ok(())
}

// ─── Main ─────────────────────────────────────────────────────────────────────

pub fn run() {
    let live_root = live::initialize_environment().expect("initialize lab run directory");
    tauri::Builder::default()
        .manage(app_state::AppState {
            changes_log_path: live_root.join("inventory-changes.log"),
            corrections: app_state::load_corrections(&dirs::config_dir().unwrap_or_default().join("frameforge/corrections.json")),
            ..Default::default()
        })
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(directory) = dirs::data_local_dir() {
                ocr_fallback::set_data_dir(directory.join("frameforge"));
            }
            live::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(|invoke| {
            use tauri::Manager;
            let command = invoke.message.command();
            if command == "show_overlay_window"
                && !invoke.message.webview().state::<app_state::AppState>().monitor_active.load(Ordering::SeqCst)
            {
                invoke.resolver.reject("Live run is not active");
                return true;
            }
            if matches!(command, "show_overlay_window" | "move_overlay_offscreen" | "get_items_by_paths"
                | "get_current_quantities" | "get_current_crafting" | "get_recipe" | "get_pending_relic_rewards")
            {
                live::record("production_ipc_received", serde_json::json!({"command": command}));
            }
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
            recognize_from_file,
            recognize_virtual_game,
            show_virtual_game_window,
            hide_virtual_game_window,
            get_catalog,
            save_screenshot_record,
            load_screenshot_history,
            delete_screenshot_record,
            live::start_live_production,
            live::stop_live_production,
            live::get_live_snapshot,
            live::load_live_captures,
            live::record_lab_frontend,
            live::test_live_relic_picker,
            diagnostics::log_relic_fe,
            relic_pick::show_overlay_window,
            relic_pick::move_overlay_offscreen,
            relic_pick::get_pending_relic_rewards,
            diagnostics::get_warframe_window_rect,
            diagnostics::set_overlay_topmost,
            catalogue::get_items_by_paths,
            catalogue::get_recipe,
            catalogue::get_current_crafting,
            live::get_current_quantities,
            live::get_item_price,
            ];
            handler(invoke)
        })
        .run(tauri::generate_context!())
        .expect("error while running OCR Lab");
}
