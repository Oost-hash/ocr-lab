import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import Overlay from "../../src-tauri/src/prod-code/frontend/relic-overlay/Overlay";
import { applyScale } from "../../src-tauri/src/prod-code/frontend/lib/uiScale";

// Observe the original DOM without changing the production component.
// A requestAnimationFrame timestamp is a paint opportunity, not proof of
// physical display scanout. Window move events are recorded separately.
export default function ProductionOverlay() {
  const container = useRef<HTMLDivElement>(null);
  useEffect(() => {
    applyScale(true);
    const root = document.getElementById("root");
    if (root) {
      root.style.transform = "scale(var(--ff-scale, 1))";
      root.style.transformOrigin = "top left";
      root.style.width = "calc(100% / var(--ff-scale, 1))";
      root.style.height = "calc(100% / var(--ff-scale, 1))";
    }
    let previous = "";
    let revision = 0;
    let frame = 0;
    let secondFrame = 0;
    const report = () => {
      const names = Array.from(container.current?.querySelectorAll(".ov-name") ?? []).map((node) => node.textContent);
      const key = JSON.stringify(names);
      if (key === previous) return;
      previous = key;
      const current = ++revision;
      const detail = { names, browser_epoch_ms: performance.timeOrigin + performance.now() };
      void invoke("record_lab_frontend", { name: names.length ? "cards_dom_committed" : "cards_dom_cleared", detail });
      cancelAnimationFrame(frame);
      cancelAnimationFrame(secondFrame);
      frame = requestAnimationFrame(() => {
        secondFrame = requestAnimationFrame(() => {
          if (revision !== current) return;
          void invoke("record_lab_frontend", {
            name: names.length ? "cards_paint_opportunity" : "clear_paint_opportunity",
            detail: { names, browser_epoch_ms: performance.timeOrigin + performance.now() },
          });
        });
      });
    };
    const observer = new MutationObserver(report);
    if (container.current) observer.observe(container.current, { subtree: true, childList: true, characterData: true });
    report();
    return () => { observer.disconnect(); cancelAnimationFrame(frame); cancelAnimationFrame(secondFrame); };
  }, []);
  return <div ref={container} style={{ height: "100%" }}><Overlay /></div>;
}
