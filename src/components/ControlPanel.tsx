import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { TAURI_EVENTS } from "../constants/tauri";
import ImagePicker from "./ImagePicker";
import ResultView from "./ResultView";
import ScreenshotHistory from "./ScreenshotHistory";
import LivePipeline from "./LivePipeline";
import { useOverlays } from "../../src-tauri/src/prod-code/frontend/hooks/useOverlays";

interface ParsedResult {
  is_complete: boolean;
  skip: boolean;
  items: string[];
  positions: number[];
  debug: string;
}

export default function ControlPanel() {
  const [, setProductionQuantities] = useState<Record<string, number>>({});
  useOverlays(setProductionQuantities);
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [imagePaths, setImagePaths] = useState<string[]>([]);
  const [imageIndex, setImageIndex] = useState(0);
  const [preprocess, setPreprocess] = useState(true);
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [intervalMs, setIntervalMs] = useState(1000);
  const [virtualGameVisible, setVirtualGameVisible] = useState(false);
  const [activeTab, setActiveTab] = useState<"static" | "live">("static");
  const [scanStage, setScanStage] = useState<"scanning" | "done" | null>(null);
  const playbackToken = useRef(0);
  const scanDoneTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let unlistenRun: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;
    let unlistenScan: (() => void) | undefined;
    let unlistenTrigger: (() => void) | undefined;

    void listen<string>(TAURI_EVENTS.WARFRAME_RUN, async (event) => {
      setLoading(false);
      setError(null);
      setResult(event.payload);
      setStatus("Warframe reward captured");
      try {
        const parsed: ParsedResult = JSON.parse(event.payload);
        await showToolOverlay();
        await emit(TAURI_EVENTS.UPDATE_OVERLAY, {
          image_path: "Warframe",
          items: parsed.items,
          positions: parsed.positions,
        });
      } catch {}
    }).then((unlisten) => { unlistenRun = unlisten; });

    void listen<string>(TAURI_EVENTS.WARFRAME_STATUS, (event) => {
      setStatus(event.payload);
    }).then((unlisten) => { unlistenStatus = unlisten; });

    void listen<{ source: string; detail: string }>(TAURI_EVENTS.PRODUCTION_SCAN, (event) => {
      setStatus(`[lab observer: ${event.payload.source}] ${event.payload.detail}`);
    }).then((unlisten) => { unlistenScan = unlisten; });

    void listen<{ source: string; detail: string }>(TAURI_EVENTS.PRODUCTION_TRIGGER, (event) => {
      setStatus(`[lab trigger: ${event.payload.source}] ${event.payload.detail}`);
    }).then((unlisten) => { unlistenTrigger = unlisten; });

    return () => {
      unlistenRun?.();
      unlistenStatus?.();
      unlistenScan?.();
      unlistenTrigger?.();
    };
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter" && !loading && imagePath) {
        handleRun();
      } else if (e.key === "Escape") {
        handleClear();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [imagePath, loading]);

  const handleRun = async (
    sourceKind: "file" | "window" = "file",
    selectedPath = imagePath,
  ) => {
    if (!selectedPath) return;

    if (sourceKind === "window") {
      setVirtualGameVisible(true);
      await showVirtualGame();
      await wait(200);
    }

    setLoading(true);
    setError(null);
    setResult(null);
    setScanStage("scanning");
    if (scanDoneTimer.current) clearTimeout(scanDoneTimer.current);
    setStatus(sourceKind === "window" ? "Capturing virtual game..." : "Running...");

    const startTime = performance.now();

    try {
      const output = sourceKind === "window"
        ? await invoke<string>("recognize_virtual_game", { preprocess })
        : await invoke<string>("recognize_from_file", {
            path: selectedPath,
            crop: false,
            preprocess,
          });
      setResult(output);

      const elapsed = ((performance.now() - startTime) / 1000).toFixed(2);
      setStatus(`Done in ${elapsed}s`);
      setScanStage("done");
      scanDoneTimer.current = setTimeout(() => setScanStage(null), 4000);

      try {
        const parsed: ParsedResult = JSON.parse(output);
        await showToolOverlay();
        await emit(TAURI_EVENTS.UPDATE_OVERLAY, {
          image_path: selectedPath,
          items: parsed.items,
          positions: parsed.positions,
        });
      } catch {}

      try {
        const filename = selectedPath.split(/[\\/]/).pop() ?? selectedPath;
        const encoder = new TextEncoder();
        const data = encoder.encode(selectedPath);
        const hashBuffer = await crypto.subtle.digest("SHA-256", data);
        const hashArray = Array.from(new Uint8Array(hashBuffer));
        const sha256 = hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
        await invoke("save_screenshot_record", { filename, sha256, resultJson: output, preprocess });
      } catch {}
    } catch (e) {
      setError(String(e));
      setStatus(null);
      setScanStage(null);
    } finally {
      setLoading(false);
    }
  };

  const presentFrame = async (path: string) => {
    localStorage.setItem("ocr-lab.virtual-game-source", path);
    await emit(TAURI_EVENTS.UPDATE_VIRTUAL_GAME, { image_path: path });
    await emit(TAURI_EVENTS.CLEAR_OVERLAY, {});
  };

  const selectImages = (paths: string[]) => {
    const first = paths[0] ?? null;
    setImagePaths(paths);
    setImageIndex(0);
    setImagePath(first);
    setResult(null);
    setError(null);
    if (first) void presentFrame(first);
  };

  const moveFrame = (offset: number) => {
    if (imagePaths.length === 0) return;
    const nextIndex = (imageIndex + offset + imagePaths.length) % imagePaths.length;
    const nextPath = imagePaths[nextIndex];
    setImageIndex(nextIndex);
    setImagePath(nextPath);
    setResult(null);
    setError(null);
    void presentFrame(nextPath);
  };

  const showVirtualGame = async () => {
    await invoke("show_virtual_game_window");
    if (imagePath) await presentFrame(imagePath);
  };

  const setVirtualGameVisibility = async (visible: boolean) => {
    setVirtualGameVisible(visible);
    try {
      if (visible) {
        await showVirtualGame();
      } else {
        await invoke("hide_virtual_game_window");
      }
    } catch (error) {
      setVirtualGameVisible(false);
      setError(String(error));
    }
  };

  const showToolOverlay = async () => {
    const overlay = await WebviewWindow.getByLabel("tool-overlay");
    if (!overlay) return;
    await overlay.show();
  };

  const wait = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms));
  const selectedName = imagePath?.split(/[\\/]/).pop();

  const startPlayback = async () => {
    if (imagePaths.length === 0 || playing) return;

    const token = playbackToken.current + 1;
    playbackToken.current = token;
    setPlaying(true);
    await showVirtualGame();

    for (let index = imageIndex; index < imagePaths.length; index += 1) {
      if (playbackToken.current !== token) break;

      const path = imagePaths[index];
      setImageIndex(index);
      setImagePath(path);
      setResult(null);
      setError(null);
      setStatus(`Frame ${index + 1} / ${imagePaths.length} presented`);
      await presentFrame(path);
      // Give the source webview a frame to paint before PrintWindow captures it.
      await wait(100);
      if (playbackToken.current !== token) break;

      await handleRun("window", path);
      if (index < imagePaths.length - 1 && playbackToken.current === token) {
        await wait(intervalMs);
      }
    }

    if (playbackToken.current === token) {
      setStatus("Sequence complete");
    }
    setPlaying(false);
  };

  const stopPlayback = () => {
    playbackToken.current += 1;
    setStatus("Stopping after the current capture...");
  };

  const handleClear = async () => {
    setResult(null);
    setError(null);
    setStatus(null);
    await emit(TAURI_EVENTS.CLEAR_OVERLAY, {});
  };

  return (
    <div className="control-panel">
      <div className="tabs" role="tablist" aria-label="OCR Lab mode">
        <button
          className={activeTab === "static" ? "tab active" : "tab"}
          role="tab"
          aria-selected={activeTab === "static"}
          onClick={() => setActiveTab("static")}
        >
          Static tests
        </button>
        <button
          className={activeTab === "live" ? "tab active" : "tab"}
          role="tab"
          aria-selected={activeTab === "live"}
          onClick={() => setActiveTab("live")}
        >
          Live scanner
        </button>
      </div>

      {activeTab === "static" ? <div className="static-scanner">
      <header className="static-header">
        <div>
          <span className="live-eyebrow">Controlled replay</span>
          <h2>Static OCR bench</h2>
          <p>Compare direct file recognition with the captured Virtual Game path.</p>
        </div>
        <div className={`source-state ${imagePath ? "has-source" : ""}`}>
          <span>{imagePaths.length || 0}</span>
          {imagePaths.length === 1 ? "source" : "sources"}
        </div>
      </header>

      <div className="static-toolbar">
        <section className="static-control-group source-controls">
          <span className="control-kicker">Source</span>
          <div className="control-row">
            <ImagePicker onSelect={selectImages} disabled={loading || playing} />
            <div className="carousel-controls" aria-label="Image carousel">
              <button className="secondary compact" onClick={() => moveFrame(-1)} disabled={loading || playing || imagePaths.length < 2}>Previous</button>
              <span className="carousel-position">{imagePaths.length > 0 ? `${imageIndex + 1} / ${imagePaths.length}` : "No images"}</span>
              <button className="secondary compact" onClick={() => moveFrame(1)} disabled={loading || playing || imagePaths.length < 2}>Next</button>
            </div>
          </div>
          <span className="selected-source" title={imagePath ?? undefined}>{selectedName ?? "Select one or more reward screenshots"}</span>
        </section>

        <section className="static-control-group processing-controls">
          <span className="control-kicker">Processing</span>
          <div className="control-row option-row">
            <label className="checkbox-label">
              <input type="checkbox" checked={preprocess} onChange={(e) => setPreprocess(e.target.checked)} />
              <span>Preprocess</span>
            </label>
            <label className="checkbox-label">
              <input type="checkbox" checked={virtualGameVisible} disabled={!imagePath || playing}
                onChange={(event) => { void setVirtualGameVisibility(event.target.checked); }} />
              <span>Show game</span>
            </label>
          </div>
          <div className="control-row run-actions">
            <button onClick={() => handleRun()} disabled={!imagePath || loading || playing}>{loading ? "Running..." : "Run file"}</button>
            <button className="secondary" onClick={() => handleRun("window")} disabled={!imagePath || loading || playing}>Run virtual</button>
          </div>
        </section>

        <section className="static-control-group sequence-controls">
          <span className="control-kicker">Sequence</span>
          <div className="control-row">
            <label className="interval-control">
              <span>Gap</span>
              <input type="number" min="0" step="100" value={intervalMs} disabled={playing}
                onChange={(event) => setIntervalMs(Math.max(0, Number(event.target.value) || 0))} />
              <span>ms</span>
            </label>
            <button className={playing ? "danger" : "secondary"} onClick={playing ? stopPlayback : startPlayback}
              disabled={!playing && imagePaths.length === 0}>{playing ? "Stop" : "Play all"}</button>
          </div>
          <span className="sequence-hint">Replays every selected frame through Virtual Game.</span>
        </section>

        {scanStage && <span className={`scan-badge scan-${scanStage}`}>{scanStage === "scanning" ? "Scanning..." : "Done"}</span>}
      </div>

      <div className="content static-content">
        {error && (
          <div className="error" onClick={() => setError(null)}>
            <span>{error}</span>
            <span className="dismiss">&times;</span>
          </div>
        )}

        {result && <ResultView result={result} />}

        {!result && !error && (
          <div className={`static-empty ${imagePath ? "is-ready" : ""}`}>
            <span className="empty-index">{imagePath ? String(imageIndex + 1).padStart(2, "0") : "--"}</span>
            <div>
              <span className="section-kicker">{imagePath ? "Ready to inspect" : "No source loaded"}</span>
              <h3>{selectedName ?? "Build a repeatable OCR test"}</h3>
              <p>{imagePath ? "Run the file directly, or route it through Virtual Game to include window capture." : "Select screenshots to compare preprocessing, capture routes and OCR timings."}</p>
            </div>
          </div>
        )}

        <ScreenshotHistory />
      </div>
      </div> : <LivePipeline />}

      {status && (
        <div className="status-bar">
          {loading && <span className="spinner" />}
          <span>{status}</span>
        </div>
      )}
    </div>
  );
}
