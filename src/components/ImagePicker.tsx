import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface Props {
  onSelect: (paths: string[]) => void;
  disabled?: boolean;
}

export default function ImagePicker({ onSelect, disabled }: Props) {
  const [fileCount, setFileCount] = useState(0);

  const handleOpen = async () => {
    try {
      const path = await open({
        multiple: true,
        filters: [
          {
            name: "Images",
            extensions: ["png", "jpg", "jpeg", "bmp", "webp"],
          },
        ],
      });

      if (path && path.length > 0) {
        setFileCount(path.length);
        onSelect(path);
      }
    } catch (e) {
      console.error("Failed to open file:", e);
    }
  };

  return (
    <div className="image-picker-bar">
      <button onClick={handleOpen} disabled={disabled} className="secondary">
        {fileCount > 0 ? `${fileCount} image${fileCount === 1 ? "" : "s"} selected` : "Select images"}
      </button>
    </div>
  );
}
