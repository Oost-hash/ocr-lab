import type { ImageInput } from "../image/types.js";
import type { OcrResult } from "../ocr/types.js";
import type { MatchedItem } from "../match/types.js";

export interface Crop {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface TraceEntry {
  step: string;
  detail: string;
}

export interface PipelineResult {
  image: ImageInput;
  ocr: OcrResult;
  matches: MatchedItem[];
  totalMs: number;
  trace: TraceEntry[];
}

export interface PipelineTiming {
  ocrMs: number;
  matchMs: number;
  totalMs: number;
}
