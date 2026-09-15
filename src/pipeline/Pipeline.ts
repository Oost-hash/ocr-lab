import { invoke } from "@tauri-apps/api/core";
import type { PipelineResult, PipelineTiming } from "./types.js";

export async function runPipeline(
  path: string,
  preprocess: boolean,
  filterUsernames: boolean,
): Promise<PipelineResult> {
  const result = await invoke<PipelineResult>("recognize_image", {
    path,
    crop: { left: 0, top: 0, right: 1, bottom: 1 },
    preprocess,
    filterUsernames,
  });
  return result;
}

export async function runScreenshotPipeline(
  label: string,
  filterUsernames: boolean,
): Promise<PipelineResult> {
  const result = await invoke<PipelineResult>("run_screenshot_pipeline", {
    label,
    filterUsernames,
  });
  return result;
}

export function extractTiming(result: PipelineResult): PipelineTiming {
  const ocrStage = result.ocr.stages.find(s => s.name === "ocr_recognize");
  const matchStage = result.ocr.stages.find(s => s.name === "match");
  return {
    ocrMs: ocrStage?.wallMs ?? 0,
    matchMs: matchStage?.wallMs ?? 0,
    totalMs: result.totalMs,
  };
}
