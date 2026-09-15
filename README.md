# OCR Lab

Standalone Windows tool for reproducing and measuring OCR from saved images.

## Current Scope

- Opens local PNG, JPEG, and BMP files.
- Converts images to a validated BGRA8 frame.
- Runs full-image Windows Media OCR with optional grayscale and contrast preprocessing.
- Shows recognized text and normalized line positions.
- Reports file read, file hashing, decode, normalization, crop, preprocessing,
  BMP encoding, COM initialization, WinRT bitmap decode, OCR engine creation,
  OCR recognition, serialization, result parsing, and total time.
- Copies a JSON result record to the clipboard.

FrameForge is not a dependency. No FrameForge production code is changed by
this project.

## Requirements

- Windows 10 or 11.
- Rust and pnpm.
- English (United States) Windows OCR language pack. The app reports an
  actionable error when it is unavailable.

## Run

```powershell
pnpm install
pnpm tauri dev
```

## Build

```powershell
pnpm tauri build --no-bundle
```

The resulting executable is at `src-tauri/target/release/ocr-lab.exe`.

## Deferred

- Crop interaction and source-image preview.
- Cold/warm batch benchmark UI and JSONL export.
- Reward matching.
- Video-frame extraction through a version-pinned FFmpeg sidecar.
