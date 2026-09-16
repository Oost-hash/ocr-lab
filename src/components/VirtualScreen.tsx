import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TAURI_EVENTS } from "../constants/tauri";

interface VirtualGameData {
  image_path: string;
}

export default function VirtualScreen() {
  const [data, setData] = useState<VirtualGameData | null>(() => {
    const imagePath = localStorage.getItem("ocr-lab.virtual-game-source");
    return imagePath ? { image_path: imagePath } : null;
  });

  useEffect(() => {
    const unlistenUpdate = listen<VirtualGameData>(
      TAURI_EVENTS.UPDATE_VIRTUAL_GAME,
      (event) => {
        setData(event.payload);
      }
    );

    return () => {
      unlistenUpdate.then((fn) => fn());
    };
  }, []);

  return (
    <div className="virtual-screen">
      {data ? (
        <>
          <div className="image-container">
            <img src={convertFileSrc(data.image_path)} alt="Screenshot" />
          </div>
        </>
      ) : (
        <div className="empty-state">
          <span>Select images in OCR Lab to start the virtual game.</span>
        </div>
      )}
    </div>
  );
}
