use std::{collections::HashSet, fs, path::PathBuf};
use syn::spanned::Spanned;

// Select whole, unchanged Rust items from the snapshot. This avoids compiling
// unrelated inventory/trade services. Bodies are copied as original bytes, not
// rewritten or maintained as another implementation. Missing items fail builds.
fn select(file: &str, output: &str, names: &[&str]) {
    let path = PathBuf::from("src/prod-code").join(file);
    println!("cargo:rerun-if-changed={}", path.display());
    let source = fs::read_to_string(&path).expect("read production snapshot");
    let parsed = syn::parse_file(&source).expect("parse production snapshot");
    let mut missing: HashSet<&str> = names.iter().copied().collect();
    let mut selected = String::new();
    for item in parsed.items {
        let name = match &item {
            syn::Item::Fn(item) => item.sig.ident.to_string(),
            syn::Item::Struct(item) => item.ident.to_string(),
            syn::Item::Type(item) => item.ident.to_string(),
            syn::Item::Impl(item) => match item.self_ty.as_ref() {
                syn::Type::Path(path) => format!("impl {}", path.path.segments.last().unwrap().ident),
                _ => continue,
            },
            _ => continue,
        };
        if missing.remove(name.as_str()) {
            selected.push_str(&source[item.span().byte_range()]);
            selected.push('\n');
        }
    }
    assert!(missing.is_empty(), "{file}: production items missing: {missing:?}");
    fs::write(PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join(output), selected)
        .expect("write selected production items");
}

fn main() {
    select("monitor.rs", "monitor.rs", &[
        "scan_heap_for_trigger", "start_memory_trigger", "start_legacy_reward_worker", "CraftingJob",
    ]);
    select("log_watcher.rs", "log_watcher.rs", &[
        "ParsedTrade", "clean_trade_item", "extract_trade_items", "parse_trade_dialog", "start_log_watcher",
        "RewardOcrResult", "VoidProjectionState", "impl VoidProjectionState",
        "parse_logged_in_name", "seed_ee_log_names", "collect_ee_log_names", "collect_session_relics",
        "collect_void_projection_state", "apply_reward_inventory_update", "dismiss_relic_rewards",
        "auto_dismiss_relic_rewards", "build_relic_reward_catalog", "build_fallback_reward_catalog",
        "prepare_reward_session", "prepare_reward_trigger", "wait_for_squad_hint", "capture_reward_items",
        "schedule_reward_diagnostic_capture", "schedule_reward_safety_cleanup", "publish_relic_rewards",
        "finalize_reward_ocr_timeout", "log_reward_ocr_stopped", "log_reward_dark_frame",
        "log_reward_ocr_empty", "log_reward_capture_failed", "log_reward_no_match",
        "log_reward_best_result", "log_reward_confirm_no_improvement", "parse_and_emit_wfm_whisper",
    ]);
    select("diagnostics.rs", "diagnostics.rs", &["append_to_diag", "write_bmp", "log_relic_fe", "get_warframe_window_rect", "set_overlay_topmost"]);
    select("relic_pick.rs", "relic_pick.rs", &[
        "RelicPickReward", "RelicPickRelic", "relic_drop_rate", "relic_pick_show", "relic_pick_hide",
        "build_relic_pick_payload", "show_overlay_window", "move_overlay_offscreen", "get_pending_relic_rewards",
    ]);
    select("worldstate.rs", "worldstate.rs", &["store_to_unique"]);
    select("inventory_state.rs", "inventory_state.rs", &["is_unique_path"]);
    select("wfcd.rs", "wfcd.rs", &["WfcdItem", "RecipeComponent", "default_one", "RelicReward"]);
    select("lib.rs", "utilities.rs", &["sanitize_chat_item_name"]);
    select("catalogue.rs", "catalogue.rs", &[
        "get_items_by_paths", "prime_set_prefix", "get_recipe", "get_current_crafting",
        "CatalogItem", "get_all_items_inner", "fix_category", "camel_to_words",
    ]);
    select("app_state.rs", "corrections.rs", &["CorrectionEntry", "load_corrections"]);
    tauri_build::build()
}
