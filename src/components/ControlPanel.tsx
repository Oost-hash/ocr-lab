import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { TAURI_EVENTS } from "../constants/tauri";
import ImagePicker from "./ImagePicker";
import ResultView from "./ResultView";
import ScreenshotHistory from "./ScreenshotHistory";

interface ParsedResult {
  is_complete: boolean;
  skip: boolean;
  items: string[];
  positions: number[];
  debug: string;
}

export default function ControlPanel() {
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
  const playbackToken = useRef(0);

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
      <div className="toolbar">
        <ImagePicker onSelect={selectImages} disabled={loading || playing} />

        <div className="carousel-controls" aria-label="Image carousel">
          <button
            className="secondary"
            onClick={() => moveFrame(-1)}
            disabled={loading || playing || imagePaths.length < 2}
          >
            Previous
          </button>
          <span className="carousel-position">
            {imagePaths.length > 0 ? `${imageIndex + 1} / ${imagePaths.length}` : "No images"}
          </span>
          <button
            className="secondary"
            onClick={() => moveFrame(1)}
            disabled={loading || playing || imagePaths.length < 2}
          >
            Next
          </button>
        </div>

        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={virtualGameVisible}
            disabled={!imagePath || playing}
            onChange={(event) => { void setVirtualGameVisibility(event.target.checked); }}
          />
          <span>Show virtual game</span>
        </label>

        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={preprocess}
            onChange={(e) => setPreprocess(e.target.checked)}
          />
          <span>Preprocess</span>
        </label>

        <button onClick={() => handleRun()} disabled={!imagePath || loading || playing}>
          {loading ? "Running..." : "Run file"}
        </button>

        <button className="secondary" onClick={() => handleRun("window")} disabled={!imagePath || loading || playing}>
          Run virtual game
        </button>

        <label className="interval-control">
          <span>Gap</span>
          <input
            type="number"
            min="0"
            step="100"
            value={intervalMs}
            disabled={playing}
            onChange={(event) => setIntervalMs(Math.max(0, Number(event.target.value) || 0))}
          />
          <span>ms</span>
        </label>

        <button
          className={playing ? "danger" : "secondary"}
          onClick={playing ? stopPlayback : startPlayback}
          disabled={!playing && imagePaths.length === 0}
        >
          {playing ? "Stop sequence" : "Play sequence"}
        </button>
      </div>

      <div className="content">
        {error && (
          <div className="error" onClick={() => setError(null)}>
            <span>{error}</span>
            <span className="dismiss">&times;</span>
          </div>
        )}

        {result && <ResultView result={result} />}

        <ScreenshotHistory />
      </div>

      {status && (
        <div className="status-bar">
          {loading && <span className="spinner" />}
          <span>{status}</span>
        </div>
      )}
    </div>
  );
}
