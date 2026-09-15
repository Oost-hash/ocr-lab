import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { invoke } from "@tauri-apps/api/core";
import { availableMonitors } from "@tauri-apps/api/window";

let imageWin: WebviewWindow | null = null;

export async function openImageWindow(
  imagePath: string,
  _imgWidth: number,
  _imgHeight: number,
): Promise<void> {
  // Close existing if any
  if (imageWin) {
    try { await imageWin.close(); } catch { /* ignore */ }
    imageWin = null;
  }

  // Store path in Rust state
  await invoke("set_current_image_path", { path: imagePath });

  // Fullscreen borderless: fill entire primary monitor
  let screenW = 1920;
  let screenH = 1080;
  let screenX = 0;
  let screenY = 0;
  try {
    const monitors = await availableMonitors();
    if (monitors.length > 0) {
      const primary = monitors[0];
      screenW = primary.size.width;
      screenH = primary.size.height;
      screenX = primary.position.x;
      screenY = primary.position.y;
    }
  } catch { /* use defaults */ }

  imageWin = new WebviewWindow("image-preview", {
    url: "index.html#image-preview",
    title: "Image Preview",
    width: screenW,
    height: screenH,
    x: screenX,
    y: screenY,
    decorations: false,
    transparent: true,
    maximized: true,
    resizable: false,
    focus: false,
  });

  // Remove shadow for true borderless
  await invoke("set_window_shadow", { label: "image-preview", shadow: false });
}

export async function closeImageWindow(): Promise<void> {
  if (imageWin) {
    try { await imageWin.close(); } catch { /* ignore */ }
    imageWin = null;
  }
}
