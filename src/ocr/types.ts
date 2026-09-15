export interface Stage {
  name: string;
  wallMs: number;
}

export interface OcrLine {
  text: string;
  xCenter: number;
  yCenter: number;
}

export interface OcrResult {
  text: string;
  lines: OcrLine[];
  stages: Stage[];
}
