//! Observation and data adapters around the unchanged production reward cycle.
use std::{collections::{HashMap, VecDeque}, io::Write, path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, AtomicU64, Ordering}, mpsc, Arc, Mutex, OnceLock}, time::Instant};
use serde_json::{json, Value};
use tauri::{Emitter, Listener, Manager, State};
use tracing::{field::{Field, Visit}, Event};
use tracing_subscriber::{layer::{Context, SubscriberExt}, registry::LookupSpan, Layer};
use crate::{app_state::AppState, wfcd::{RelicReward, WfcdItem, RecipeComponent}};

#[derive(serde::Serialize)]
pub(crate) struct LiveCapture {
    id: String,
    timestamp: String,
    image_path: PathBuf,
    log: Option<String>,
    outcome: &'static str,
    reason: &'static str,
}

static ROOT: OnceLock<PathBuf> = OnceLock::new();
static RECORDER: OnceLock<Recorder> = OnceLock::new();
static CURRENT_CAPTURE_DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);
static OVERLAY_READY: AtomicBool = AtomicBool::new(false);
static CYCLE: AtomicU64 = AtomicU64::new(0);

#[derive(serde::Deserialize)]
struct CachedInventoryItem {
    #[serde(default)]
    amount: i64,
    #[serde(default)]
    category: String,
    #[serde(default)]
    is_flavour: bool,
    #[serde(default)]
    mod_ranks: Option<Value>,
}

#[derive(serde::Deserialize)]
struct InventoryStateCache {
    #[serde(default)]
    items: HashMap<String, CachedInventoryItem>,
}

struct Recorder {
    started: Instant,
    sender: mpsc::Sender<Value>,
    events: Arc<Mutex<VecDeque<Value>>>,
    write_error: Arc<Mutex<Option<String>>>,
}

/// Set process-local production diagnostic paths before worker threads start.
pub(crate) fn initialize_environment() -> Result<PathBuf, String> {
    let root = crate::runs_dir()?.join(format!("live-{}-{}", chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ"), std::process::id()));
    let temp = root.join("temp");
    std::fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
    std::env::set_var("TEMP", &temp);
    std::env::set_var("TMP", &temp);
    ROOT.set(root.clone()).map_err(|_| "Lab environment already initialized")?;
    Ok(root)
}

fn start_recorder() -> Result<(), String> {
    let path = ROOT.get().ok_or("Lab environment not initialized")?.join("timeline.jsonl");
    let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (sender, receiver) = mpsc::channel::<Value>();
    let events = Arc::new(Mutex::new(VecDeque::new()));
    let write_error = Arc::new(Mutex::new(None));
    RECORDER.set(Recorder { started: Instant::now(), sender, events: events.clone(), write_error: write_error.clone() })
        .map_err(|_| "Recorder already started")?;
    std::thread::spawn(move || {
        let mut file = std::io::BufWriter::new(file);
        for value in receiver {
            let written = serde_json::to_writer(&mut file, &value)
                .map_err(std::io::Error::other)
                .and_then(|_| file.write_all(b"\n"))
                .and_then(|_| file.flush());
            if let Err(error) = written {
                *write_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.to_string());
            }
            let mut history = events.lock().unwrap_or_else(|e| e.into_inner());
            if history.len() == 500 { history.pop_front(); }
            history.push_back(value);
        }
    });
    Ok(())
}

pub(crate) fn record(name: &str, detail: Value) {
    if let Some(recorder) = RECORDER.get() {
        let _ = recorder.sender.send(json!({
            "event": name, "elapsed_us": recorder.started.elapsed().as_micros(),
            "utc": chrono::Utc::now().to_rfc3339(), "cycle": CYCLE.load(Ordering::SeqCst),
            "detail": detail,
        }));
    }
}

pub(crate) fn set_capture_session(path: Option<PathBuf>) {
    *CURRENT_CAPTURE_DIR.get_or_init(|| Mutex::new(None)).lock()
        .unwrap_or_else(|error| error.into_inner()) = path;
}

pub(crate) fn preserve_issue_frame(app: &tauri::AppHandle, kind: &str, attempt: u32) {
    let frame = app.state::<AppState>().last_ocr_frame.lock()
        .ok().and_then(|frame| frame.clone());
    let directory = CURRENT_CAPTURE_DIR.get()
        .and_then(|directory| directory.lock().ok().and_then(|path| path.clone()));
    if let (Some((pixels, width, height)), Some(directory)) = (frame, directory) {
        let path = directory.join(format!("issue-{kind}-attempt-{attempt}.bmp"));
        match crate::diagnostics::write_bmp(&path, &pixels, width, height) {
            Ok(()) => record("ocr_issue_capture_saved", json!({
                "attempt": attempt, "kind": kind, "path": path,
            })),
            Err(error) => record("ocr_issue_capture_failed", json!({
                "attempt": attempt, "kind": kind, "error": error.to_string(),
            })),
        }
    }
}

fn capture_relic_picker_failure() -> Result<(PathBuf, String), String> {
    let (pixels, width, height) = crate::ocr::capture_warframe_pixels()?;
    let raw_text = crate::ocr::ocr_pixels_rect(&pixels, width, height, 0.0, 0.5, 0.0, 0.25)
        .unwrap_or_else(|error| format!("[OCR error: {error}]"));
    let crop_width = width / 2;
    let crop_height = height / 4;
    let mut cropped = Vec::with_capacity((crop_width * crop_height * 4) as usize);
    for row in 0..crop_height as usize {
        let start = row * width as usize * 4;
        let end = start + crop_width as usize * 4;
        cropped.extend_from_slice(&pixels[start..end]);
    }

    let root = ROOT.get().ok_or("Lab environment not initialized")?;
    let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S%.3f").to_string();
    let directory = root.join("production-captures").join(format!("relic-picker-{timestamp}"));
    std::fs::create_dir_all(&directory).map_err(|error| format!("Create {}: {error}", directory.display()))?;
    let image_path = directory.join("issue-era-ocr-attempt-1.bmp");
    crate::diagnostics::write_bmp(&image_path, &cropped, crop_width, crop_height)
        .map_err(|error| format!("Write {}: {error}", image_path.display()))?;
    let log = format!(
        "RELIC PICKER ERA OCR FAILURE\nCapture region: left 50%, top 25% ({crop_width}x{crop_height})\nExpected: LITH, MESO, NEO, AXI, or ALL\nRaw OCR:\n{}\n",
        if raw_text.trim().is_empty() { "(no text)" } else { raw_text.trim() },
    );
    std::fs::write(directory.join("ocr_session_log.txt"), log)
        .map_err(|error| format!("Write relic picker diagnosis: {error}"))?;
    Ok((image_path, raw_text))
}

#[tauri::command]
pub(crate) fn load_live_captures() -> Result<Vec<LiveCapture>, String> {
    let runs = crate::runs_dir()?;
    if !runs.exists() {
        return Ok(Vec::new());
    }

    let mut captures = Vec::new();
    for run in std::fs::read_dir(&runs).map_err(|error| format!("Read {}: {error}", runs.display()))? {
        let run = run.map_err(|error| format!("Read run entry: {error}"))?;
        let run_name = run.file_name().to_string_lossy().to_string();
        if !run_name.starts_with("live-") || !run.path().is_dir() {
            continue;
        }
        let capture_root = run.path().join("production-captures");
        if !capture_root.is_dir() {
            continue;
        }
        for session in std::fs::read_dir(&capture_root)
            .map_err(|error| format!("Read {}: {error}", capture_root.display()))?
        {
            let session = session.map_err(|error| format!("Read capture entry: {error}"))?;
            let session_name = session.file_name().to_string_lossy().to_string();
            let log = std::fs::read_to_string(session.path().join("ocr_session_log.txt")).ok();
            let (session_outcome, session_reason) = match log.as_deref() {
                Some(text) if text.contains("[STEP 3] OVERLAY OPENED") => ("success", "Overlay opened"),
                Some(text) if text.contains("OCR TIMEOUT") => ("failed", "OCR timed out"),
                Some(text) if text.contains("OCR STOPPED") => ("failed", "OCR stopped before confirmation"),
                Some(_) => ("failed", "No confirmed overlay"),
                None => ("pending", "Session is still being recorded"),
            };
            let images = std::fs::read_dir(session.path())
                .map_err(|error| format!("Read {}: {error}", session.path().display()))?;
            for image in images {
                let image = image.map_err(|error| format!("Read capture image: {error}"))?;
                let name = image.file_name().to_string_lossy().to_string();
                if !name.ends_with(".bmp") {
                    continue;
                }
                let issue_reason = if name.starts_with("issue-no-match-") {
                    Some("No catalog match")
                } else if name.starts_with("issue-ocr-empty-") {
                    Some("OCR returned no text")
                } else if name.starts_with("issue-dark-frame-") {
                    Some("Dark capture")
                } else if name.starts_with("issue-era-ocr-") {
                    Some("Relic picker era not detected")
                } else {
                    None
                };
                captures.push(LiveCapture {
                    id: format!("{run_name}/{session_name}/{name}"),
                    timestamp: session_name.clone(),
                    image_path: image.path(),
                    log: log.clone(),
                    outcome: if issue_reason.is_some() { "issue" } else { session_outcome },
                    reason: issue_reason.unwrap_or(session_reason),
                });
            }
        }
    }
    captures.sort_by(|left, right| right.id.cmp(&left.id));
    Ok(captures)
}

pub(crate) fn begin_cycle() {
    CYCLE.fetch_add(1, Ordering::SeqCst);
    record("ee_trigger_accepted", json!({}));
}

pub(crate) fn observe_diagnostic(path: &Path, text: &str) {
    if text.contains("[OV] dataReady=true") { OVERLAY_READY.store(true, Ordering::SeqCst); }
    let name = if text.contains("[STEP 4] DISMISS") { "dismiss_received" }
        else if text.contains("[STEP 4] AUTO-DISMISS") { "auto_dismiss" }
        else if text.contains("[STEP 3] OVERLAY OPENED") { "rewards_confirmed" }
        else { "production_diagnostic" };
    record(name, json!({"path": path, "text": text}));
}

struct OcrTrace;

#[derive(Default)]
struct EventFields {
    message: Option<String>,
}

impl Visit for EventFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
}

impl<S> Layer<S> for OcrTrace where S: tracing::Subscriber + for<'a> LookupSpan<'a> {
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &tracing::Id, ctx: Context<'_, S>) {
        if !attrs.metadata().target().contains("::ocr::") { return; }
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Instant::now());
            record("ocr_span_start", json!({"span": attrs.metadata().name(), "id": id.into_u64()}));
        }
    }
    fn on_close(&self, id: tracing::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            if let Some(started) = span.extensions().get::<Instant>() {
                record("ocr_span_end", json!({"span": span.metadata().name(), "id": id.into_u64(), "duration_us": started.elapsed().as_micros()}));
            }
        }
    }
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if !event.metadata().target().contains("log_watcher") { return; }
        let mut fields = EventFields::default();
        event.record(&mut fields);
        let Some(message) = fields.message else { return; };
        let name = if message.contains("PopulateInventoryGrid detected") { "relic_picker_detected" }
            else if message.contains("relic-pick: OCR result") { "relic_picker_ocr_result" }
            else if message.contains("relic-pick: emitting relic-pick-open") { "relic_picker_payload_ready" }
            else if message.contains("relic-pick: dismiss fired") { "relic_picker_dismiss_detected" }
            else if message.contains("relic-pick: trigger suppressed") { "relic_picker_trigger_suppressed" }
            else { return; };
        record(name, json!({"message": message, "target": event.metadata().target()}));
        if name == "relic_picker_ocr_result" && message.ends_with("None") {
            std::thread::spawn(|| {
                let started = Instant::now();
                match capture_relic_picker_failure() {
                    Ok((path, raw_text)) => record("relic_picker_failure_probe", json!({
                        "duration_us": started.elapsed().as_micros(),
                        "raw_text": raw_text, "screenshot": path,
                    })),
                    Err(error) => record("relic_picker_failure_probe", json!({
                        "duration_us": started.elapsed().as_micros(), "error": error,
                    })),
                }
            });
        }
    }
}

pub(crate) fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(OcrTrace))?;
    for name in ["relic-trigger", "relic-rewards", "relic-pick-open", "relic-pick-close", "ff-status"] {
        app.listen(name, move |event| {
            let payload = serde_json::from_str::<Value>(event.payload()).unwrap_or(Value::Null);
            record(name, payload);
        });
    }
    if let Some(window) = app.get_webview_window("relic-overlay") {
        window.set_ignore_cursor_events(true)?;
        window.on_window_event(|event| {
            if let tauri::WindowEvent::Moved(position) = event {
                record("overlay_window_moved", json!({"x": position.x, "y": position.y, "offscreen": position.y <= -3000}));
            }
        });
    }
    if let Some(window) = app.get_webview_window("relic-pick-overlay") {
        window.set_ignore_cursor_events(false)?;
        window.on_window_event(|event| {
            if let tauri::WindowEvent::Moved(position) = event {
                record("relic_picker_window_moved", json!({"x": position.x, "y": position.y, "offscreen": position.y <= -3000}));
            }
        });
    }
    if let Some(window) = app.get_webview_window("main") {
        let app = app.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                record("control_panel_closed", json!({"action": "exit_app"}));
                app.exit(0);
            }
        });
    }
    Ok(())
}

fn read_cache<T: serde::de::DeserializeOwned>(directory: &Path, name: &str) -> Result<T, String> {
    let path = directory.join(name);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_inventory_quantities(directory: &Path) -> Result<HashMap<String, i64>, String> {
    let cache: InventoryStateCache = read_cache(directory, "inventory_state_cache.json")?;
    Ok(cache.items.into_iter().filter_map(|(path, item)| {
        let included = item.is_flavour
            || (item.mod_ranks.is_none()
                && (!crate::inventory_state::is_unique_path(&path)
                    || matches!(item.category.as_str(), "Blueprints" | "Parts"))
                && item.amount > 0);
        included.then_some((path, if item.is_flavour { 1 } else { item.amount }))
    }).collect())
}

#[tauri::command]
pub(crate) fn start_live_production(app: tauri::AppHandle, memory_trigger: bool) -> Result<Value, String> {
    if STARTED.load(Ordering::SeqCst) { return Err("This run has already started. Restart the lab for a new run.".into()); }
    let directory = dirs::data_local_dir().ok_or("LocalAppData not found")?.join("frameforge");
    let ee_log = dirs::data_local_dir().unwrap().join("Warframe/EE.log");
    std::fs::File::open(&ee_log).map_err(|e| format!("{}: {e}", ee_log.display()))?;
    let rewards: HashMap<String, Vec<RelicReward>> = read_cache(&directory, "relic_rewards_cache.json")?;
    let items: Vec<WfcdItem> = read_cache(&directory, "items_cache.json")?;
    if rewards.values().all(Vec::is_empty) || items.is_empty() { return Err("Production relic rewards or item cache is empty".into()); }
    let mut cache_notes = Vec::new();
    let recipes: HashMap<String, Vec<RecipeComponent>> = read_cache(&directory, "recipes_cache.json").unwrap_or_else(|error| {
        cache_notes.push(error); HashMap::new()
    });
    let quantities = read_inventory_quantities(&directory).unwrap_or_else(|error| {
        cache_notes.push(error); HashMap::new()
    });
    let price_cache: Value = read_cache(&directory, "relics_run_prices.json").unwrap_or_else(|error| {
        cache_notes.push(error); Value::Null
    });
    let prices: HashMap<String, u32> = serde_json::from_value(price_cache["by_name"].clone()).unwrap_or_default();
    let root = ROOT.get().ok_or("Lab environment not initialized")?;
    let captures = root.join("production-captures");
    std::fs::create_dir_all(&captures).map_err(|e| e.to_string())?;
    STARTED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| "A run is already starting")?;
    let input_snapshot = json!({"items": items, "relic_rewards": rewards, "recipes": recipes,
        "quantities": quantities, "price_cache": price_cache, "cache_notes": cache_notes});
    let input_path = root.join("inputs.json");
    let save_inputs = std::fs::File::create(&input_path)
        .and_then(|file| serde_json::to_writer(file, &input_snapshot).map_err(std::io::Error::other));
    if let Err(error) = save_inputs { STARTED.store(false, Ordering::SeqCst); return Err(format!("{}: {error}", input_path.display())); }
    if let Err(error) = start_recorder() { STARTED.store(false, Ordering::SeqCst); return Err(error); }
    let state = app.state::<AppState>();
    *state.relic_rewards.lock().unwrap() = rewards.clone();
    *state.wfcd_items.lock().unwrap() = items;
    *state.recipes.lock().unwrap() = recipes;
    *state.current_quantities.lock().unwrap() = quantities;
    *state.prices.lock().unwrap() = prices;
    *state.relics_run_prices.lock().unwrap() = state.prices.lock().unwrap().clone();
    state.relic_pick_overlay_enabled.store(true, Ordering::SeqCst);
    state.mem_trigger_enabled.store(memory_trigger, Ordering::SeqCst);
    state.monitor_active.store(true, Ordering::SeqCst);
    record("run_started", json!({
        "source": "production", "ee_log": ee_log, "cache_directory": directory,
        "cache_notes": cache_notes, "memory_trigger": memory_trigger,
        "overlay_ready_at_start": OVERLAY_READY.load(Ordering::SeqCst),
        "price_cache_date": price_cache["date"],
        "data_adapter": "production inventory_state_cache quantity projection; empty ExportRecipes blueprint map; no inventory worker, live crafting or network prices",
        "snapshot": serde_json::from_str::<Value>(include_str!("../../../prod-code-manifest.json")).ok(),
    }));
    crate::reward_watcher::spawn_reward_watcher_thread(crate::reward_watcher::RewardWatcherDeps {
        app: app.clone(), flag: state.monitor_active.clone(), relic_rewards: rewards, auto_capture_dir: captures,
    });
    crate::log_watcher::start_log_watcher(app.clone())?;
    Ok(json!({"path": root, "cache_notes": cache_notes}))
}

#[tauri::command]
pub(crate) fn stop_live_production(app: tauri::AppHandle) -> Result<(), String> {
    app.state::<AppState>().monitor_active.store(false, Ordering::SeqCst);
    record("run_stopped", json!({}));
    crate::relic_pick::move_overlay_offscreen(app)
}

#[tauri::command]
pub(crate) fn test_live_relic_picker(app: tauri::AppHandle, era: String) -> Result<Value, String> {
    if !app.state::<AppState>().monitor_active.load(Ordering::SeqCst) {
        return Err("Live run is not active".into());
    }
    record("relic_picker_test_trigger", json!({"era": era}));
    let payload = crate::relic_pick::build_relic_pick_payload(&era, &app);
    crate::relic_pick::relic_pick_show(&app);
    app.emit("relic-pick-open", &payload).map_err(|error| error.to_string())?;
    Ok(payload)
}

#[tauri::command]
pub(crate) fn get_live_snapshot(state: State<AppState>) -> Value {
    let events: Vec<Value> = RECORDER.get().map(|r| r.events.lock().unwrap().iter().cloned().collect()).unwrap_or_default();
    let write_error = RECORDER.get().and_then(|r| r.write_error.lock().unwrap().clone());
    json!({"started": STARTED.load(Ordering::SeqCst), "active": state.monitor_active.load(Ordering::SeqCst),
        "overlay_ready": OVERLAY_READY.load(Ordering::SeqCst), "path": ROOT.get(), "events": events, "write_error": write_error})
}

#[tauri::command]
pub(crate) fn record_lab_frontend(name: String, detail: Value) {
    record(&format!("frontend:{name}"), detail);
}

#[tauri::command]
pub(crate) fn get_current_quantities(state: State<AppState>) -> HashMap<String, i64> {
    if !STARTED.load(Ordering::SeqCst) {
        return dirs::data_local_dir().and_then(|directory|
            read_inventory_quantities(&directory.join("frameforge")).ok()).unwrap_or_default();
    }
    state.current_quantities.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub(crate) fn get_item_price(item_name: String, state: State<AppState>) -> Option<u32> {
    let price = state.prices.lock().unwrap_or_else(|e| e.into_inner()).get(&item_name.to_lowercase()).copied();
    record("price_adapter", json!({"item": item_name, "price": price, "source": "local cache only"}));
    price
}
