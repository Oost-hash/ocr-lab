import { invoke } from "@tauri-apps/api/core";
import type { OcrResult } from "./types.js";
import type { Crop } from "../pipeline/types.js";

export async function runOcr(
  path: string,
  crop: Crop,
  preprocess: boolean,
): Promise<OcrResult> {
  const result = await invoke<{
    ocr: { text: string; lines: { text: string; x_center: number; y_center: number }[]; stages: { name: string; wall_ms: number }[] };
  }>("recognize_image", { path, crop, preprocess });

  return {
    text: result.ocr.text,
    lines: result.ocr.lines.map(l => ({
      text: l.text,
      xCenter: l.x_center,
      yCenter: l.y_center,
    })),
    stages: result.ocr.stages.map(s => ({
      name: s.name,
      wallMs: s.wall_ms,
    })),
  };
}
