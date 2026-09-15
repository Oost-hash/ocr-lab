use std::time::Instant;

use crate::catalog;
use crate::match_engine;
use crate::ocr;
use crate::types::{Crop, ImageInput, PipelineResult, TraceEntry};

/// Run the full pipeline: load image → OCR → filter → match → return result.
pub fn run_pipeline(path: &str, crop: Crop, preprocess: bool, filter_usernames: bool) -> Result<PipelineResult, String> {
    let total_start = Instant::now();
    let mut trace: Vec<TraceEntry> = Vec::new();

    // ── Step 1: Image info ────────────────────────────────────────────────────
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();
    let sha256 = ocr::get_sha256(path)?;
    let (width, height) = ocr::get_image_dimensions(path)?;
    trace.push(TraceEntry {
        step: "image_load".into(),
        detail: format!("{filename} ({width}x{height})"),
    });

    let image = ImageInput {
        filename,
        sha256,
        width_px: width,
        height_px: height,
        path: path.to_string(),
    };

    // ── Step 2: OCR ───────────────────────────────────────────────────────────
    let ocr_output = ocr::recognize_from_file(path, crop, preprocess)?;
    trace.push(TraceEntry {
        step: "ocr_raw".into(),
        detail: format!("{} lines", ocr_output.lines.len()),
    });
    for line in &ocr_output.lines {
        trace.push(TraceEntry {
            step: "ocr_line".into(),
            detail: format!("[{:.3}, {:.3}] {}", line.x_center, line.y_center, line.text),
        });
    }

    // ── Step 3: Filter usernames ──────────────────────────────────────────────
    let (filtered_text, filtered_lines, _filtered_out) = if filter_usernames {
        let mut kept = Vec::new();
        let mut removed = Vec::new();
        for line in &ocr_output.lines {
            if is_username_like(&line.text) {
                let reason = username_filter_reason(&line.text);
                removed.push(format!("\"{}\" ({})", line.text, reason));
            } else {
                kept.push(line.clone());
            }
        }
        let text: String = kept.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        trace.push(TraceEntry {
            step: "filter".into(),
            detail: format!("removed {} lines", removed.len()),
        });
        for r in &removed {
            trace.push(TraceEntry {
                step: "filter_remove".into(),
                detail: r.clone(),
            });
        }
        (text, kept, removed)
    } else {
        trace.push(TraceEntry {
            step: "filter".into(),
            detail: "disabled".into(),
        });
        (ocr_output.text.clone(), ocr_output.lines.clone(), Vec::new())
    };

    trace.push(TraceEntry {
        step: "filter_remaining".into(),
        detail: format!("{} lines", filtered_lines.len()),
    });
    for line in &filtered_lines {
        trace.push(TraceEntry {
            step: "remaining_line".into(),
            detail: format!("[{:.3}, {:.3}] {}", line.x_center, line.y_center, line.text),
        });
    }

    // ── Step 4: Match ─────────────────────────────────────────────────────────
    let match_start = Instant::now();
    let pairs = catalog::catalog_pairs();
    let mut matches = match_engine::match_items(
        &filtered_text,
        &filtered_lines.iter().map(|l| (l.text.clone(), l.x_center, l.y_center)).collect::<Vec<_>>(),
        &pairs,
        0.75,
    );
    let match_ms = match_start.elapsed().as_secs_f64() * 1_000.0;

    trace.push(TraceEntry {
        step: "match".into(),
        detail: format!("{} matches above 0.75 ({} ms)", matches.len(), match_ms as u32),
    });
    for m in &matches {
        trace.push(TraceEntry {
            step: "match_hit".into(),
            detail: format!("{} ({:.3}) at [{:.3}, {:.3}]", m.name, m.score, m.x_center, m.y_center),
        });
    }

    let total_ms = total_start.elapsed().as_secs_f64() * 1_000.0;

    // Add match stage to OCR stages for timing display
    let mut ocr_with_match = ocr_output;
    ocr_with_match.stages.push(crate::types::Stage {
        name: "match".into(),
        wall_ms: match_ms,
    });

    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(PipelineResult {
        image,
        ocr: ocr_with_match,
        matches,
        total_ms,
        trace,
    })
}

/// Detect Warframe username patterns to exclude from matching.
fn is_username_like(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() { return false; }
    if t.contains("::") { return true; }
    if !t.contains(' ') && t.chars().any(|c| c.is_ascii_digit()) { return true; }
    if t.len() < 3 || t.len() > 50 { return true; }
    false
}

/// Explain why a line was filtered.
fn username_filter_reason(text: &str) -> &'static str {
    let t = text.trim();
    if t.contains("::") { return "clan tag (::)" }
    if !t.contains(' ') && t.chars().any(|c| c.is_ascii_digit()) { return "digits in single word" }
    if t.len() < 3 { return "too short" }
    if t.len() > 50 { return "too long" }
    "unknown"
}

/// Run full pipeline via window screenshot: capture → crop → OCR → filter → match.
pub fn run_screenshot_pipeline(hwnd: isize, label: &str, filter_usernames: bool) -> Result<PipelineResult, String> {
    use std::time::Instant;
    let total_start = Instant::now();
    let mut trace: Vec<TraceEntry> = Vec::new();

    // Step 1: Screenshot capture
    let capture_start = Instant::now();
    let (_pixels, width, height) = crate::screenshot::capture_window_by_hwnd(hwnd)?;
    let capture_ms = capture_start.elapsed().as_secs_f64() * 1_000.0;
    trace.push(TraceEntry {
        step: "screenshot_capture".into(),
        detail: format!("{label} ({width}x{height}) {capture_ms:.1}ms"),
    });

    // Step 2: Crop to Warframe item list region (top ~22% of screen)
    // In Warframe, the item list is typically in the top portion of the screen
    let crop = crate::types::Crop {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 0.22,
    };
    trace.push(TraceEntry {
        step: "crop".into(),
        detail: format!("region [{:.0}%, {:.0}%] of {width}x{height}", crop.left * 100.0, crop.bottom * 100.0),
    });

    // Step 3: OCR from screenshot (with crop applied internally)
    let ocr_output = ocr::recognize_from_screenshot(hwnd)?;
    trace.push(TraceEntry {
        step: "ocr_raw".into(),
        detail: format!("{} lines", ocr_output.lines.len()),
    });
    for line in &ocr_output.lines {
        trace.push(TraceEntry {
            step: "ocr_line".into(),
            detail: format!("[{:.3}, {:.3}] {}", line.x_center, line.y_center, line.text),
        });
    }

    // Step 3: Filter usernames
    let (filtered_text, filtered_lines, _filtered_out) = if filter_usernames {
        let mut kept = Vec::new();
        let mut removed = Vec::new();
        for line in &ocr_output.lines {
            if is_username_like(&line.text) {
                let reason = username_filter_reason(&line.text);
                removed.push(format!("\"{}\" ({})", line.text, reason));
            } else {
                kept.push(line.clone());
            }
        }
        let text: String = kept.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        trace.push(TraceEntry {
            step: "filter".into(),
            detail: format!("removed {} lines", removed.len()),
        });
        for r in &removed {
            trace.push(TraceEntry {
                step: "filter_remove".into(),
                detail: r.clone(),
            });
        }
        (text, kept, removed)
    } else {
        trace.push(TraceEntry {
            step: "filter".into(),
            detail: "disabled".into(),
        });
        (ocr_output.text.clone(), ocr_output.lines.clone(), Vec::new())
    };

    trace.push(TraceEntry {
        step: "filter_remaining".into(),
        detail: format!("{} lines", filtered_lines.len()),
    });
    for line in &filtered_lines {
        trace.push(TraceEntry {
            step: "remaining_line".into(),
            detail: format!("[{:.3}, {:.3}] {}", line.x_center, line.y_center, line.text),
        });
    }

    // Step 4: Match
    let match_start = Instant::now();
    let pairs = catalog::catalog_pairs();
    let mut matches = match_engine::match_items(
        &filtered_text,
        &filtered_lines.iter().map(|l| (l.text.clone(), l.x_center, l.y_center)).collect::<Vec<_>>(),
        &pairs,
        0.75,
    );
    let match_ms = match_start.elapsed().as_secs_f64() * 1_000.0;

    trace.push(TraceEntry {
        step: "match".into(),
        detail: format!("{} matches above 0.75 ({} ms)", matches.len(), match_ms as u32),
    });
    for m in &matches {
        trace.push(TraceEntry {
            step: "match_hit".into(),
            detail: format!("{} ({:.3}) at [{:.3}, {:.3}]", m.name, m.score, m.x_center, m.y_center),
        });
    }

    let total_ms = total_start.elapsed().as_secs_f64() * 1_000.0;

    let mut ocr_with_match = ocr_output;
    ocr_with_match.stages.push(crate::types::Stage {
        name: "match".into(),
        wall_ms: match_ms,
    });

    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let image = crate::types::ImageInput {
        filename: label.to_string(),
        sha256: "screenshot".into(),
        width_px: width,
        height_px: height,
        path: format!("hwnd:{hwnd}"),
    };

    Ok(PipelineResult {
        image,
        ocr: ocr_with_match,
        matches,
        total_ms,
        trace,
    })
}
