//! Module glue for unchanged items selected by build.rs from prod-code.
//! Lab wrappers only observe calls and delegate to the original implementation.
pub(crate) mod wfcd { include!(concat!(env!("OUT_DIR"), "/wfcd.rs")); }
pub(crate) mod worldstate { include!(concat!(env!("OUT_DIR"), "/worldstate.rs")); }
pub(crate) mod inventory_state { include!(concat!(env!("OUT_DIR"), "/inventory_state.rs")); }
pub(crate) mod memory_scanner {
    pub(crate) fn find_warframe_pid_pub() -> Option<u32> {
        <crate::platform::Platform as crate::platform::ProcessAccess>::find_warframe_pid()
    }
}
pub(crate) mod monitor {
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};
    use crate::app_state::AppState;
    include!(concat!(env!("OUT_DIR"), "/monitor.rs"));
}
pub(crate) mod diagnostics {
    use crate::append_to_file;
    include!(concat!(env!("OUT_DIR"), "/diagnostics.rs"));
}
pub(crate) mod relic_pick {
    use std::collections::HashMap;
    use tauri::{Manager, State};
    use tracing::{debug, info};
    use crate::app_state::AppState;
    include!(concat!(env!("OUT_DIR"), "/relic_pick.rs"));
}
pub(crate) mod catalogue {
    use std::collections::HashMap;
    use tauri::State;
    use crate::{app_state::AppState, monitor::CraftingJob, wfcd::{self, RecipeComponent}};
    include!(concat!(env!("OUT_DIR"), "/catalogue.rs"));
}
pub(crate) mod log_watcher {
    mod original {
        use std::collections::HashMap;
        use std::sync::atomic::Ordering;
        use tauri::{Emitter, Manager};
        use tracing::{info, warn};
        use crate::{app_state::AppState, append_to_file, ocr, sanitize_chat_item_name, OcrParams};
        use crate::diagnostics::{append_to_diag, write_bmp};
        use crate::relic_pick::{build_relic_pick_payload, relic_pick_hide, relic_pick_show};
        include!(concat!(env!("OUT_DIR"), "/log_watcher.rs"));
    }
    pub(crate) use original::*;
    use std::sync::{Arc, Mutex};

    pub(crate) fn collect_void_projection_state(
        text: &str, state: &mut VoidProjectionState, size: &Arc<Mutex<Option<usize>>>,
        session_log_path: &std::path::Path,
    ) {
        let lines: Vec<&str> = text.lines().filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("voidprojection") || lower.contains("relic reward screen")
                || lower.contains("matchingservice::endsession") || lower.contains("gets reward /lotus/")
                || lower.contains("still waiting on response from") || lower.contains("has reward info for all players now")
        }).collect();
        if !lines.is_empty() {
            crate::live::record("ee_log_batch_observed", serde_json::json!({"batch_bytes": text.len(), "relevant_lines": lines}));
        }
        original::collect_void_projection_state(text, state, size, session_log_path);
    }

    pub(crate) fn prepare_reward_trigger(
        app: &tauri::AppHandle, names: &Arc<Mutex<Vec<String>>>,
        size: &Arc<Mutex<Option<usize>>>, relics: &[String],
    ) {
        crate::live::begin_cycle();
        original::prepare_reward_trigger(app, names, size, relics);
    }

    pub(crate) async fn wait_for_squad_hint(size: &Arc<Mutex<Option<usize>>>) {
        crate::live::record("squad_wait_start", serde_json::json!({}));
        original::wait_for_squad_hint(size).await;
        crate::live::record("squad_wait_end", serde_json::json!({"hint": size.lock().ok().and_then(|v| *v)}));
    }

    pub(crate) fn prepare_reward_session(
        session_log_path: &std::path::Path, names: &Arc<Mutex<Vec<String>>>,
        timestamp: &str, trigger_line: &str, prefilter_log: &str, catalog_len: usize,
        auto_capture_dir: &std::path::Path, diag_dir: &Arc<Mutex<Option<std::path::PathBuf>>>,
        last_found_path: &std::path::Path,
    ) {
        crate::live::record("ee_trigger_context", serde_json::json!({
            "trigger_line": trigger_line, "production_timestamp": timestamp,
            "prefilter": prefilter_log, "catalog_size": catalog_len,
        }));
        original::prepare_reward_session(session_log_path, names, timestamp, trigger_line,
            prefilter_log, catalog_len, auto_capture_dir, diag_dir, last_found_path);
    }

    pub(crate) async fn capture_reward_items(
        app: &tauri::AppHandle, catalog: Arc<Vec<(String, String)>>,
        size: Arc<Mutex<Option<usize>>>, names: Arc<Mutex<Vec<String>>>,
    ) -> Option<RewardOcrResult> {
        let started = std::time::Instant::now();
        crate::live::record("capture_ocr_start", serde_json::json!({"catalog_size": catalog.len()}));
        let result = original::capture_reward_items(app, catalog, size, names).await;
        crate::live::record("capture_ocr_end", serde_json::json!({
            "duration_us": started.elapsed().as_micros(), "result": result,
        }));
        result
    }
}
