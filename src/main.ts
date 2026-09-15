import { pickImage, getState as getImageState } from "./image/ImagePicker.js";
import { renderImagePreview } from "./image/ImagePreview.js";
import { runPipeline, runScreenshotPipeline, extractTiming } from "./pipeline/Pipeline.js";
import { openImageWindow } from "./overlay/OverlayWindow.js";
import "./style.css";

// ─── URL routing ─────────────────────────────────────────────────────────────

const hash = window.location.hash;

if (hash === "#image-preview") {
  initImagePreview();
} else {
  initMain();
}

// ─── Main window ─────────────────────────────────────────────────────────────

function initMain() {
  // Keep main window on top of image preview
  import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
    getCurrentWindow().setAlwaysOnTop(true);
  });

  const app = document.querySelector<HTMLDivElement>("#app")!;
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
      <label class="toggle"><input id="filter-usernames" type="checkbox" checked /> Filter usernames</label>
      <button id="run-ocr" disabled>Run Pipeline</button>
    </section>
    <section class="image-preview" hidden>
      <div class="screenshot-triple">
        <div class="screenshot-box">
          <h3>Raw Screenshot</h3>
          <img id="preview-raw" alt="Raw screenshot from PrintWindow" />
        </div>
        <div class="screenshot-box">
          <h3>Cropped (Item Region)</h3>
          <img id="preview-crop" alt="Cropped to item list region" />
        </div>
        <div class="screenshot-box">
          <h3>Preprocessed (Greyscale)</h3>
          <img id="preview-prep" alt="Greyscale + contrast preprocessed" />
        </div>
      </div>
      <div id="preview-dims" class="preview-dims"></div>
    </section>
    <p id="status" role="status">Choose a PNG, JPEG, or BMP image to begin.</p>
    <section class="timing-bar" hidden>
      <div class="timing-row">
        <span class="timing-label">OCR:</span> <span id="t-ocr">—</span>
        <span class="timing-label">Match:</span> <span id="t-match">—</span>
        <span class="timing-label">Total:</span> <span id="t-total">—</span>
      </div>
    </section>
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
      <article class="wide" hidden>
        <h2>Filtered lines</h2>
        <table><thead><tr><th>Text</th><th>Reason</th></tr></thead><tbody id="filtered"></tbody></table>
      </article>
      <article class="wide">
        <h2>Remaining lines</h2>
        <table><thead><tr><th>Text</th><th>X</th><th>Y</th></tr></thead><tbody id="remaining"></tbody></table>
      </article>
      <article class="wide">
        <h2>Matched items</h2>
        <table><thead><tr><th>#</th><th>Item</th><th>Score</th><th>X</th><th>Y</th></tr></thead><tbody id="matches"></tbody></table>
      </article>
      <article class="wide">
        <h2>Export</h2>
        <button id="copy-json">Copy full result JSON</button>
        <button id="copy-compact">Copy compact result</button>
      </article>
    </section>
  `;

  const openButton = document.querySelector<HTMLButtonElement>("#open-image")!;
  const runButton = document.querySelector<HTMLButtonElement>("#run-ocr")!;
  const preprocess = document.querySelector<HTMLInputElement>("#preprocess")!;
  const filterUsernames = document.querySelector<HTMLInputElement>("#filter-usernames")!;
  const fileName = document.querySelector<HTMLSpanElement>("#file-name")!;
  const status = document.querySelector<HTMLParagraphElement>("#status")!;
  const results = document.querySelector<HTMLElement>(".results")!;
  const timingBar = document.querySelector<HTMLElement>(".timing-bar")!;
  const imagePreview = document.querySelector<HTMLElement>(".image-preview")!;
  const inputData = document.querySelector<HTMLDListElement>("#input-data")!;
  const timings = document.querySelector<HTMLTableSectionElement>("#timings")!;
  const ocrText = document.querySelector<HTMLPreElement>("#ocr-text")!;
  const lines = document.querySelector<HTMLTableSectionElement>("#lines")!;
  const matches = document.querySelector<HTMLTableSectionElement>("#matches")!;
  const filtered = document.querySelector<HTMLTableSectionElement>("#filtered")!;
  const remaining = document.querySelector<HTMLTableSectionElement>("#remaining")!;
  const filteredArticle = filtered.closest("article")!;
  const copyJson = document.querySelector<HTMLButtonElement>("#copy-json")!;
  const copyCompact = document.querySelector<HTMLButtonElement>("#copy-compact")!;
  const tOcr = document.querySelector<HTMLSpanElement>("#t-ocr")!;
  const tMatch = document.querySelector<HTMLSpanElement>("#t-match")!;
  const tTotal = document.querySelector<HTMLSpanElement>("#t-total")!;

  let lastResult: Awaited<ReturnType<typeof runScreenshotPipeline>> | null = null;

  openButton.addEventListener("click", async () => {
    const imageState = await pickImage();
    if (!imageState.path) return;
    fileName.textContent = imageState.path.split(/[\\/]/).pop() ?? imageState.path;
    renderImagePreview(imagePreview, imageState);
    status.textContent = "Ready to run pipeline.";
    runButton.disabled = false;

    // Open second window with the image
    if (imageState.input) {
      await openImageWindow(imageState.path, imageState.input.widthPx, imageState.input.heightPx);
    }

    // Ensure main window is focused on top
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setFocus();
  });

  runButton.addEventListener("click", async () => {
    const imageState = getImageState();
    if (!imageState.path) return;
    runButton.disabled = true;
    results.hidden = true;
    timingBar.hidden = true;
    status.textContent = "Capturing screenshot...";
    const pipelineStart = performance.now();
    try {
      // Capture and display all three screenshots
      const { invoke } = await import("@tauri-apps/api/core");
      const [rawPng, prepPng, cropPng] = await invoke<[number[], number[], number[]]>("capture_screenshot_images", {
        label: "image-preview",
      });

      // Display raw screenshot
      const rawBlob = new Blob([new Uint8Array(rawPng)], { type: "image/png" });
      const rawUrl = URL.createObjectURL(rawBlob);
      const rawImg = document.querySelector<HTMLImageElement>("#preview-raw");
      if (rawImg) rawImg.src = rawUrl;

      // Display cropped screenshot
      const cropBlob = new Blob([new Uint8Array(cropPng)], { type: "image/png" });
      const cropUrl = URL.createObjectURL(cropBlob);
      const cropImg = document.querySelector<HTMLImageElement>("#preview-crop");
      if (cropImg) cropImg.src = cropUrl;

      // Display preprocessed screenshot
      const prepBlob = new Blob([new Uint8Array(prepPng)], { type: "image/png" });
      const prepUrl = URL.createObjectURL(prepBlob);
      const prepImg = document.querySelector<HTMLImageElement>("#preview-prep");
      if (prepImg) prepImg.src = prepUrl;

      // Show preview section
      imagePreview.hidden = false;
      status.textContent = "Running OCR pipeline...";

      // Run the full pipeline
      const result = await runScreenshotPipeline("image-preview", filterUsernames.checked);

      // Timing bar
      const timing = extractTiming(result);
      const pipelineMs = performance.now() - pipelineStart;
      tOcr.textContent = timing.ocrMs ? `${timing.ocrMs.toFixed(1)} ms` : "—";
      tMatch.textContent = timing.matchMs ? `${timing.matchMs.toFixed(1)} ms` : "—";
      tTotal.textContent = `${pipelineMs.toFixed(1)} ms`;
      timingBar.hidden = false;

      renderRun(result);
      results.hidden = false;
      lastResult = result;
      const highCount = result.matches.filter((m) => m.score >= 0.75).length;
      status.textContent = `Screenshot pipeline complete in ${pipelineMs.toFixed(1)} ms. ${highCount} items matched (>= 0.75).`;
    } catch (error) {
      status.textContent = `Pipeline failed: ${String(error)}`;
    } finally {
      runButton.disabled = false;
    }
  });

  copyJson.addEventListener("click", async () => {
    if (!lastResult) {
      status.textContent = "No result to copy. Run the pipeline first.";
      return;
    }
    await navigator.clipboard.writeText(JSON.stringify(lastResult, null, 2));
    status.textContent = "Full pipeline result JSON copied to the clipboard.";
  });

  copyCompact.addEventListener("click", async () => {
    if (!lastResult) {
      status.textContent = "No result to copy. Run the pipeline first.";
      return;
    }
    const compact = {
      image: lastResult.image.filename,
      ocrText: lastResult.ocr.text,
      ocrLines: lastResult.ocr.lines,
      matches: lastResult.matches
        .filter((m) => m.score >= 0.75)
        .map((m) => ({ name: m.name, score: m.score, x: m.xCenter, y: m.yCenter })),
      stages: lastResult.ocr.stages,
    };
    await navigator.clipboard.writeText(JSON.stringify(compact, null, 2));
    status.textContent = "Compact result (score >= 0.75) copied to the clipboard.";
  });

  function renderRun(result: Awaited<ReturnType<typeof runScreenshotPipeline>>) {
    inputData.innerHTML = [
      ["File", result.image.filename],
      ["SHA-256", result.image.sha256],
      ["Dimensions", `${result.image.widthPx} x ${result.image.heightPx}`],
    ].map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(value)}</dd>`).join("");
    timings.innerHTML = [
      ...result.ocr.stages,
      { name: "total", wallMs: result.totalMs },
    ].map((stage) => `<tr><td>${escapeHtml(stage.name)}</td><td>${stage.wallMs.toFixed(2)} ms</td></tr>`).join("");
    ocrText.textContent = result.ocr.text || "No text recognized.";
    lines.innerHTML = result.ocr.lines.map((line) => `
      <tr><td>${escapeHtml(line.text)}</td><td>${line.xCenter.toFixed(3)}</td><td>${line.yCenter.toFixed(3)}</td></tr>
    `).join("");
    const highMatches = result.matches.filter((m) => m.score >= 0.75);
    matches.innerHTML = highMatches.map((m, i) => `
      <tr><td>${i + 1}</td><td>${escapeHtml(m.name)}</td><td>${m.score.toFixed(3)}</td><td>${m.xCenter.toFixed(3)}</td><td>${m.yCenter.toFixed(3)}</td></tr>
    `).join("") || '<tr><td colspan="5">No matches above 0.75 threshold</td></tr>';

    // Render filtered + remaining from trace
    const removed = result.trace.filter((t) => t.step === "filter_remove");
    const remainLines = result.trace.filter((t) => t.step === "remaining_line");
    if (removed.length > 0) {
      filteredArticle.hidden = false;
      filtered.innerHTML = removed.map((t) => {
        const [text, reason] = t.detail.split(" (");
        return `<tr><td>${escapeHtml(text.replace(/"/g, ""))}</td><td>${escapeHtml(reason?.replace(")", "") ?? "")}</td></tr>`;
      }).join("");
    } else {
      filteredArticle.hidden = true;
      filtered.innerHTML = "";
    }
    remaining.innerHTML = remainLines.map((t) => {
      const m = t.detail.match(/^\[([0-9.]+), ([0-9.]+)\] (.+)$/);
      if (!m) return `<tr><td>${escapeHtml(t.detail)}</td><td></td><td></td></tr>`;
      return `<tr><td>${escapeHtml(m[3])}</td><td>${m[1]}</td><td>${m[2]}</td></tr>`;
    }).join("");
  }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function escapeHtml(value: string): string {
  const el = document.createElement("span");
  el.textContent = value;
  return el.innerHTML;
}

// ─── Image preview window (second window) ────────────────────────────────────

async function initImagePreview() {
  const app = document.querySelector<HTMLDivElement>("#app")!;
  app.innerHTML = `<div style="margin:0;background:#101719;display:flex;align-items:center;justify-content:center;height:100vh;"><p style="color:#8fa2a6;">Loading image...</p><pre id="debug" style="color:#666;font-size:11px;position:fixed;top:8px;left:8px;"></pre></div>`;
  const debug = document.querySelector<HTMLPreElement>("#debug")!;
  const log = (msg: string) => { debug.textContent += msg + "\n"; console.log(msg); };

  log("initImagePreview started");

  const { invoke } = await import("@tauri-apps/api/core");

  log("requesting image path from Rust...");
  const imagePath = await invoke<string | null>("get_current_image_path");
  log("got path: " + imagePath);

  if (!imagePath) {
    log("no image path found!");
    return;
  }

  try {
    log("reading image bytes...");
    const bytes = await invoke<number[]>("read_image_bytes", { path: imagePath });
    log("got " + bytes.length + " bytes");
    const blob = new Blob([new Uint8Array(bytes)], { type: "image/png" });
    const url = URL.createObjectURL(blob);
    log("created blob url, displaying...");
    log("creating image element...");
    app.innerHTML = `
      <style>
        html, body { margin: 0; padding: 0; background: url("${url}") center/contain no-repeat #101719; width: 100vw; height: 100vh; overflow: hidden; }
      </style>
    `;
    log("done!");
  } catch (err) {
    log("ERROR: " + String(err));
  }
}
