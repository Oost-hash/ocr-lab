import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { TAURI_EVENTS } from "../constants/tauri";

interface OverlayData {
  items: string[];
  positions: number[];
}

export default function ToolOverlay() {
  const [data, setData] = useState<OverlayData | null>(null);

  useEffect(() => {
    document.documentElement.style.setProperty("background", "transparent", "important");
    document.body.style.setProperty("background", "transparent", "important");
    document.getElementById("root")?.style.setProperty("background", "transparent", "important");

    const unlistenUpdate = listen<OverlayData>(TAURI_EVENTS.UPDATE_OVERLAY, (event) => {
      setData(event.payload);
    });
    const unlistenClear = listen(TAURI_EVENTS.CLEAR_OVERLAY, () => {
      setData(null);
    });

    return () => {
      unlistenUpdate.then((fn) => fn());
      unlistenClear.then((fn) => fn());
    };
  }, []);

  if (!data || data.items.length === 0) return null;

  return (
    <div className="tool-overlay">
      {data.items.map((item, index) => {
        const name = item.split("/").pop() ?? item;
        const x = data.positions[index] ?? 0.5;
        return (
          <div key={`${item}-${index}`} className="overlay-card" style={{ left: `${x * 100}%` }}>
            <span className="card-name">{name}</span>
          </div>
        );
      })}
    </div>
  );
}
