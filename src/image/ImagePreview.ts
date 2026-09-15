import type { ImageState } from "./types.js";

export function renderImagePreview(
  container: HTMLElement,
  state: ImageState,
): void {
  if (!state.previewUrl || !state.path) {
    container.hidden = true;
    return;
  }

  // Show the container with placeholder images (actual screenshots will be captured on Run)
  const dims = container.querySelector<HTMLDivElement>("#preview-dims");
  if (dims) {
    dims.textContent = state.path.split(/[\\/]/).pop() ?? state.path;
  }
  container.hidden = false;
}
