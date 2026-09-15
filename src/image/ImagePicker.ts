import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import type { ImageInput, ImageState } from "./types.js";

const state: ImageState = {
  path: null,
  previewUrl: null,
  input: null,
};

let previewObjectUrl: string | null = null;

export function getState(): Readonly<ImageState> {
  return state;
}

export async function pickImage(): Promise<ImageState> {
  const path = await open({
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "bmp"] }],
  });
  if (!path || Array.isArray(path)) return state;

  state.path = path;

  // Read bytes for preview
  try {
    const bytes = await invoke<number[]>("read_image_bytes", { path });
    const blob = new Blob([new Uint8Array(bytes)], { type: "image/png" });
    if (previewObjectUrl) URL.revokeObjectURL(previewObjectUrl);
    previewObjectUrl = URL.createObjectURL(blob);
    state.previewUrl = previewObjectUrl;
  } catch {
    state.previewUrl = null;
  }

  // Get image metadata
  try {
    const dims = await invoke<[number, number]>("get_image_dimensions", { path });
    const sha256 = await invoke<string>("get_image_sha256", { path });
    const filename = path.split(/[\\/]/).pop() ?? path;
    state.input = {
      filename,
      sha256,
      widthPx: dims[0],
      heightPx: dims[1],
      path,
    };
  } catch {
    state.input = null;
  }

  return state;
}

export function reset(): void {
  if (previewObjectUrl) URL.revokeObjectURL(previewObjectUrl);
  previewObjectUrl = null;
  state.path = null;
  state.previewUrl = null;
  state.input = null;
}
