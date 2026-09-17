import { lazy, Suspense, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import ControlPanel from "./components/ControlPanel";
import ToolOverlay from "./components/ToolOverlay";
import VirtualScreen from "./components/VirtualScreen";
import "./control-panel.css";
import "./virtual-screen.css";

const params = new URLSearchParams(window.location.search);
const IS_VIRTUAL_SCREEN = params.has("virtual-screen");
const IS_TOOL_OVERLAY = params.has("tool-overlay");
const IS_PRODUCTION_OVERLAY = params.has("overlay");
const IS_RELIC_PICK_OVERLAY = params.has("relic-pick-overlay");
const ProductionOverlay = lazy(() => import("./components/ProductionOverlay"));
const RelicPickOverlay = lazy(() => import("../src-tauri/src/prod-code/frontend/relic-overlay/RelicPickOverlay"));

function RelicPickLabObserver() {
  useEffect(() => {
    let generation = 0;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<{ era: string; relics: unknown[] }>("relic-pick-open", (event) => {
      const current = ++generation;
      let frames = 0;
      const waitForDom = () => {
        if (current !== generation) return;
        const root = document.querySelector(".rpo-root");
        if (!root && frames++ < 120) {
          requestAnimationFrame(waitForDom);
          return;
        }
        requestAnimationFrame(() => {
          if (current !== generation) return;
          void invoke("record_lab_frontend", { name: "relic_picker_paint_opportunity", detail: {
            browser_epoch_ms: performance.timeOrigin + performance.now(),
            era: event.payload.era,
            payload_relics: event.payload.relics.length,
            rendered_cards: document.querySelectorAll(".rpo-card").length,
            dom_ready: Boolean(root),
          } });
        });
      };
      requestAnimationFrame(waitForDom);
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => { disposed = true; generation += 1; unlisten?.(); };
  }, []);
  return null;
}

export default function App() {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    setReady(true);
  }, []);

  if (!ready) return null;

  if (IS_PRODUCTION_OVERLAY) {
    return <Suspense fallback={null}><ProductionOverlay /></Suspense>;
  }

  if (IS_RELIC_PICK_OVERLAY) {
    return <Suspense fallback={null}><RelicPickOverlay /><RelicPickLabObserver /></Suspense>;
  }

  if (IS_VIRTUAL_SCREEN) {
    return <VirtualScreen />;
  }

  if (IS_TOOL_OVERLAY) {
    return <ToolOverlay />;
  }

  return <ControlPanel />;
}
