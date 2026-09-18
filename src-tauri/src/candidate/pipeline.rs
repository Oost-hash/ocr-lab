use std::{
    path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Mutex, OnceLock},
    time::Instant,
};

use serde::Serialize;

static ENABLED: AtomicBool = AtomicBool::new(false);
static TRIGGER_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static LAST_RESULT: OnceLock<Mutex<Option<PipelineResult>>> = OnceLock::new();
static COMPARISON: OnceLock<Mutex<Option<Comparison>>> = OnceLock::new();

#[derive(Clone, Serialize)]
pub(crate) struct PhaseResult {
    status: &'static str,
    duration_ms: Option<u64>,
}

#[derive(Clone, Serialize)]
pub(crate) struct CaptureResult {
    status: &'static str,
    duration_ms: Option<u64>,
    era: Option<String>,
    raw_ocr: Option<String>,
    screenshot: Option<PathBuf>,
}

#[derive(Clone, Serialize)]
pub(crate) struct RenderingResult {
    status: &'static str,
    duration_ms: Option<u64>,
    payload_count: Option<u64>,
    rendered_count: Option<u64>,
}

#[derive(Clone, Serialize)]
pub(crate) struct PipelinePhases {
    trigger: PhaseResult,
    capture: CaptureResult,
    rendering: RenderingResult,
}

#[derive(Clone, Serialize)]
pub(crate) struct PipelineResult {
    schema_version: u8,
    pipeline: &'static str,
    trigger_id: String,
    status: &'static str,
    total_ms: u64,
    phases: PipelinePhases,
    error: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct CandidateSnapshot {
    enabled: bool,
    result: Option<PipelineResult>,
}

struct Comparison {
    directory: PathBuf,
    started: Instant,
    capture_finished: Option<Instant>,
    baseline: PipelineResult,
}

#[tauri::command]
pub(crate) fn set_candidate_enabled(enabled: bool) -> CandidateSnapshot {
    ENABLED.store(enabled, Ordering::SeqCst);
    let mut result = LAST_RESULT.get_or_init(|| Mutex::new(None)).lock()
        .unwrap_or_else(|error| error.into_inner());
    *result = enabled.then(|| candidate_result("pending", false));
    drop(result);
    if !enabled {
        *COMPARISON.get_or_init(|| Mutex::new(None)).lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
    }
    snapshot()
}

#[tauri::command]
pub(crate) fn get_candidate_snapshot() -> CandidateSnapshot {
    snapshot()
}

#[tauri::command]
pub(crate) fn start_candidate_pipeline() -> Result<(), String> {
    if !ENABLED.load(Ordering::SeqCst) {
        return Err("Candidate pipeline is not selected".into());
    }
    Err("Candidate pipeline is not yet implemented".into())
}

pub(crate) fn observe_relic_picker_trigger(run_root: &Path) -> Result<(), String> {
    if !ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }
    let trigger_id = format!("picker-{:03}", TRIGGER_SEQUENCE.fetch_add(1, Ordering::SeqCst));
    let directory = run_root.join("results").join(&trigger_id);
    let candidate = candidate_result(&trigger_id, true);
    let baseline = baseline_result(&trigger_id);
    write_result(&directory, "candidate.json", &candidate)?;
    write_result(&directory, "baseline.json", &baseline)?;
    *LAST_RESULT.get_or_init(|| Mutex::new(None)).lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(candidate);
    *COMPARISON.get_or_init(|| Mutex::new(None)).lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(Comparison {
            directory,
            started: Instant::now(),
            capture_finished: None,
            baseline,
        });
    Ok(())
}

pub(crate) fn observe_ocr_result(era: Option<String>) -> Result<(), String> {
    update_baseline(|comparison| {
        let finished = Instant::now();
        comparison.capture_finished = Some(finished);
        comparison.baseline.phases.capture.duration_ms = Some(comparison.started.elapsed().as_millis() as u64);
        comparison.baseline.phases.capture.era = era;
        if comparison.baseline.phases.capture.era.is_some() {
            comparison.baseline.phases.capture.status = "complete";
            comparison.baseline.phases.rendering.status = "running";
        } else {
            comparison.baseline.status = "failed";
            comparison.baseline.total_ms = comparison.started.elapsed().as_millis() as u64;
            comparison.baseline.phases.capture.status = "failed";
            comparison.baseline.error = Some("Relic picker era not detected".into());
        }
    })
}

pub(crate) fn observe_payload(payload_count: u64) -> Result<(), String> {
    update_baseline(|comparison| {
        comparison.baseline.phases.rendering.payload_count = Some(payload_count);
    })
}

pub(crate) fn observe_paint(rendered_count: u64) -> Result<(), String> {
    update_baseline(|comparison| {
        comparison.baseline.status = "success";
        comparison.baseline.total_ms = comparison.started.elapsed().as_millis() as u64;
        comparison.baseline.phases.rendering.status = "complete";
        comparison.baseline.phases.rendering.duration_ms = comparison.capture_finished
            .map(|finished| finished.elapsed().as_millis() as u64);
        comparison.baseline.phases.rendering.rendered_count = Some(rendered_count);
        comparison.baseline.error = None;
    })
}

pub(crate) fn observe_failure_evidence(raw_ocr: String, screenshot: PathBuf) -> Result<(), String> {
    update_baseline(|comparison| {
        comparison.baseline.phases.capture.raw_ocr = Some(raw_ocr);
        comparison.baseline.phases.capture.screenshot = Some(screenshot);
    })
}

fn update_baseline(update: impl FnOnce(&mut Comparison)) -> Result<(), String> {
    let mut comparison = COMPARISON.get_or_init(|| Mutex::new(None)).lock()
        .unwrap_or_else(|error| error.into_inner());
    let Some(comparison) = comparison.as_mut() else { return Ok(()); };
    update(comparison);
    write_result(&comparison.directory, "baseline.json", &comparison.baseline)
}

fn candidate_result(trigger_id: &str, triggered: bool) -> PipelineResult {
    PipelineResult {
        schema_version: 1,
        pipeline: "candidate",
        trigger_id: trigger_id.to_string(),
        status: "not_implemented",
        total_ms: 0,
        phases: PipelinePhases {
            trigger: PhaseResult {
                status: if triggered { "complete" } else { "not_reached" },
                duration_ms: triggered.then_some(0),
            },
            capture: CaptureResult {
                status: "not_implemented", duration_ms: None, era: None,
                raw_ocr: None, screenshot: None,
            },
            rendering: RenderingResult {
                status: "not_reached", duration_ms: None,
                payload_count: None, rendered_count: None,
            },
        },
        error: Some("Candidate pipeline is not yet implemented".into()),
    }
}

fn baseline_result(trigger_id: &str) -> PipelineResult {
    PipelineResult {
        schema_version: 1,
        pipeline: "baseline",
        trigger_id: trigger_id.to_string(),
        status: "running",
        total_ms: 0,
        phases: PipelinePhases {
            trigger: PhaseResult { status: "complete", duration_ms: Some(0) },
            capture: CaptureResult {
                status: "running", duration_ms: None, era: None,
                raw_ocr: None, screenshot: None,
            },
            rendering: RenderingResult {
                status: "not_reached", duration_ms: None,
                payload_count: None, rendered_count: None,
            },
        },
        error: None,
    }
}

fn write_result(directory: &Path, filename: &str, result: &PipelineResult) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("Create {}: {error}", directory.display()))?;
    let path = directory.join(filename);
    let file = std::fs::File::create(&path)
        .map_err(|error| format!("Create {}: {error}", path.display()))?;
    serde_json::to_writer_pretty(file, result)
        .map_err(|error| format!("Write {}: {error}", path.display()))
}

fn snapshot() -> CandidateSnapshot {
    CandidateSnapshot {
        enabled: ENABLED.load(Ordering::SeqCst),
        result: LAST_RESULT.get_or_init(|| Mutex::new(None)).lock()
            .unwrap_or_else(|error| error.into_inner()).clone(),
    }
}
