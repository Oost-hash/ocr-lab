export const TAURI_EVENTS = {
  UPDATE_VIRTUAL_GAME: "ocr-update-virtual-game",
  UPDATE_OVERLAY: "ocr-update-overlay",
  CLEAR_OVERLAY: "ocr-clear-overlay",
  SCREENSHOT_SELECTED: "ocr-screenshot-selected",
  WARFRAME_RUN: "ocr-warframe-run",
  WARFRAME_STATUS: "ocr-warframe-status",
  PRODUCTION_SCAN: "ocr-production-scan",
  PRODUCTION_TRIGGER: "ocr-production-trigger",
} as const;

export const TAURI_COMMANDS = {
  RECOGNIZE_FROM_FILE: "recognize_from_file",
  GET_CATALOG: "get_catalog",
  READ_IMAGE_BYTES: "read_image_bytes",
  GET_IMAGE_DIMENSIONS: "get_image_dimensions",
} as const;
