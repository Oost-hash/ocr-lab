//! Lab-owned state satisfying the original reward pipeline's dependencies.
//! All caches are read from production; reward inventory writes stay in memory.
use std::{collections::HashMap, path::PathBuf, sync::{Arc, Mutex, atomic::AtomicBool}};
use crate::{monitor::CraftingJob, wfcd::{RelicReward, WfcdItem, RecipeComponent}};
const BUNDLED_CORRECTIONS: &str = include_str!("../prod-code/resources/corrections.json");
include!(concat!(env!("OUT_DIR"), "/corrections.rs"));

#[derive(Default)]
pub(crate) struct AppState {
    pub monitor_active: Arc<AtomicBool>,
    pub mem_trigger_enabled: Arc<AtomicBool>,
    pub local_player_name: Mutex<Option<String>>,
    pub current_quantities: Mutex<HashMap<String, i64>>,
    pub current_crafting: Mutex<Vec<CraftingJob>>,
    pub relic_rewards: Mutex<HashMap<String, Vec<RelicReward>>>,
    pub wfcd_items: Mutex<Vec<WfcdItem>>,
    pub blueprint_to_result: Mutex<HashMap<String, (String, Option<u32>)>>,
    pub recipes: Mutex<HashMap<String, Vec<RecipeComponent>>>,
    pub pending_relic_rewards: Mutex<Option<serde_json::Value>>,
    pub last_ocr_frame: Arc<Mutex<Option<(Vec<u8>, u32, u32)>>>,
    pub changes_log_path: PathBuf,
    pub corrections: HashMap<String, CorrectionEntry>,
    pub prices: Mutex<HashMap<String, u32>>,
    pub relics_run_prices: Mutex<HashMap<String, u32>>,
    pub relic_pick_overlay_enabled: Arc<AtomicBool>,
}
