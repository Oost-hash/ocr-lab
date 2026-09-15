use serde::{Deserialize, Serialize};

// ─── Image domein ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    pub filename: String,
    pub sha256: String,
    pub width_px: u32,
    pub height_px: u32,
    pub path: String,
}

// ─── OCR domein ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub name: String,
    pub wall_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    pub x_center: f32,
    pub y_center: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResult {
    pub text: String,
    pub lines: Vec<OcrLine>,
    pub stages: Vec<Stage>,
}

// ─── Match domein ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub unique_name: String,
    pub name: String,
    pub rarity: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedItem {
    pub name: String,
    pub score: f32,
    pub x_center: f32,
    pub y_center: f32,
}

// ─── Pipeline domein ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Crop {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEntry {
    pub step: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineResult {
    pub image: ImageInput,
    pub ocr: OcrResult,
    pub matches: Vec<MatchedItem>,
    pub total_ms: f64,
    pub trace: Vec<TraceEntry>,
}

// ─── Overlay domein ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayPayload {
    pub matches: Vec<MatchedItem>,
    pub image_width: u32,
    pub image_height: u32,
}
