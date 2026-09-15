import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./style.css";

type Stage = { name: string; wall_ms: number };
type OcrLine = { text: string; x_center: number; y_center: number };
type OcrRun = {
  input: { filename: string; sha256: string; width_px: number; height_px: number };
  config: { crop_norm: [number, number, number, number]; preprocess: boolean };
  backend: string;
  language: string;
  stages: Stage[];
  total_ms: number;
  text: string;
  lines: OcrLine[];
};

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Missing app root");

app.innerHTML = `
  <header>
    <p class="eyebrow">LOCAL IMAGE PIPELINE</p>
    <h1>OCR Lab</h1>
    <p class="intro">Measure Windows OCR from a saved image, not a live game screen.</p>
  </header>
  <section class="controls" aria-label="OCR controls">
    <button id="open-image">Open image</button>
    <span id="file-name">No image selected</span>
    <label class="toggle"><input id="preprocess" type="checkbox" checked /> Preprocess: grayscale + contrast</label>
    <button id="run-ocr" disabled>Run OCR</button>
  </section>
  <p id="status" role="status">Choose a PNG, JPEG, or BMP image to begin.</p>
  <section class="results" hidden>
    <article>
      <h2>Input</h2>
      <dl id="input-data"></dl>
    </article>
    <article>
      <h2>Timing</h2>
      <table><thead><tr><th>Stage</th><th>Wall time</th></tr></thead><tbody id="timings"></tbody></table>
    </article>
    <article class="wide">
      <h2>Recognized text</h2>
      <pre id="ocr-text"></pre>
    </article>
    <article class="wide">
      <h2>Recognized lines</h2>
      <table><thead><tr><th>Text</th><th>X</th><th>Y</th></tr></thead><tbody id="lines"></tbody></table>
    </article>
    <article class="wide">
      <h2>Export</h2>
      <button id="copy-json">Copy result JSON</button>
    </article>
  </section>
`;

const openButton = document.querySelector<HTMLButtonElement>("#open-image")!;
const runButton = document.querySelector<HTMLButtonElement>("#run-ocr")!;
const preprocess = document.querySelector<HTMLInputElement>("#preprocess")!;
const fileName = document.querySelector<HTMLSpanElement>("#file-name")!;
const status = document.querySelector<HTMLParagraphElement>("#status")!;
const results = document.querySelector<HTMLElement>(".results")!;
const inputData = document.querySelector<HTMLDListElement>("#input-data")!;
const timings = document.querySelector<HTMLTableSectionElement>("#timings")!;
const ocrText = document.querySelector<HTMLPreElement>("#ocr-text")!;
const lines = document.querySelector<HTMLTableSectionElement>("#lines")!;
const copyJson = document.querySelector<HTMLButtonElement>("#copy-json")!;

let selectedPath: string | null = null;
let lastRun: OcrRun | null = null;

openButton.addEventListener("click", async () => {
  const path = await open({
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "bmp"] }],
  });
  if (!path || Array.isArray(path)) return;
  selectedPath = path;
  fileName.textContent = path.split(/[\\/]/).pop() ?? path;
  status.textContent = "Ready to run Windows Media OCR.";
  runButton.disabled = false;
});

runButton.addEventListener("click", async () => {
  if (!selectedPath) return;
  runButton.disabled = true;
  results.hidden = true;
  status.textContent = "Running OCR...";
  try {
    lastRun = await invoke<OcrRun>("recognize_image", {
      path: selectedPath,
      crop: { left: 0, top: 0, right: 1, bottom: 1 },
      preprocess: preprocess.checked,
    });
    renderRun(lastRun);
    results.hidden = false;
    status.textContent = `OCR complete in ${lastRun.total_ms.toFixed(2)} ms.`;
  } catch (error) {
    status.textContent = `OCR failed: ${String(error)}`;
  } finally {
    runButton.disabled = false;
  }
});

copyJson.addEventListener("click", async () => {
  if (!lastRun) return;
  await navigator.clipboard.writeText(JSON.stringify(lastRun, null, 2));
  status.textContent = "Result JSON copied to the clipboard.";
});

function renderRun(run: OcrRun) {
  inputData.innerHTML = [
    ["File", run.input.filename],
    ["SHA-256", run.input.sha256],
    ["Dimensions", `${run.input.width_px} x ${run.input.height_px}`],
    ["Backend", run.backend],
    ["Language", run.language],
  ].map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value)}</dd>`).join("");
  timings.innerHTML = [
    ...run.stages,
    { name: "total", wall_ms: run.total_ms },
  ].map((stage) => `<tr><td>${escapeHtml(stage.name)}</td><td>${stage.wall_ms.toFixed(2)} ms</td></tr>`).join("");
  ocrText.textContent = run.text || "No text recognized.";
  lines.innerHTML = run.lines.map((line) => `
    <tr><td>${escapeHtml(line.text)}</td><td>${line.x_center.toFixed(3)}</td><td>${line.y_center.toFixed(3)}</td></tr>
  `).join("");
}

function escapeHtml(value: string) {
  const element = document.createElement("span");
  element.textContent = value;
  return element.innerHTML;
}
