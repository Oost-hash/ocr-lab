use std::sync::Mutex;
use crate::ocr;
use crate::pipeline;
use crate::types::{Crop, OcrResult, PipelineResult};

/// Store the current image path for the second window to request.
pub struct CurrentImage(pub Mutex<Option<String>>);

/// Existing command: recognize image (kept for backwards compat).
#[tauri::command]
pub fn recognize_image(path: String, crop: Crop, preprocess: bool, filter_usernames: bool) -> Result<PipelineResult, String> {
    pipeline::run_pipeline(&path, crop, preprocess, filter_usernames)
}

/// Get the current image path (for second window).
#[tauri::command]
pub fn get_current_image_path(state: tauri::State<'_, CurrentImage>) -> Result<Option<String>, String> {
    Ok(state.0.lock().unwrap().clone())
}

/// Set the current image path (called from main window).
#[tauri::command]
pub fn set_current_image_path(state: tauri::State<'_, CurrentImage>, path: String) -> Result<(), String> {
    *state.0.lock().unwrap() = Some(path);
    Ok(())
}

/// Read raw image bytes for frontend preview.
#[tauri::command]
pub fn read_image_bytes(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|e| format!("Read image: {e}"))
}

/// Get image dimensions (width, height).
#[tauri::command]
pub fn get_image_dimensions(path: String) -> Result<(u32, u32), String> {
    ocr::get_image_dimensions(&path)
}

/// Get image SHA-256 hash.
#[tauri::command]
pub fn get_image_sha256(path: String) -> Result<String, String> {
    ocr::get_sha256(&path)
}

/// Emit an event to the overlay window.
#[tauri::command]
pub async fn emit_to_overlay(
    app: tauri::AppHandle,
    event: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    use tauri::Emitter;
    app.emit(&event, payload).map_err(|e| format!("Emit event: {e}"))
}

/// Position the overlay window relative to the main window.
#[tauri::command]
pub fn position_overlay(
    app: tauri::AppHandle,
    main_x: i32,
    main_y: i32,
    main_w: u32,
    main_h: u32,
) -> Result<(), String> {
    use tauri::Manager;
    let Some(win) = app.get_webview_window("pipeline-overlay") else {
        return Err("Overlay window not found".into());
    };
    let y_offset = (main_h as f32 * 0.54) as i32;
    let overlay_h = (main_h as f32 * 0.28) as u32;
    let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: main_x,
        y: main_y + y_offset,
    }));
    let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: main_w,
        height: overlay_h,
    }));
    let _ = win.show();
    Ok(())
}

/// Remove window shadow (for borderless fullscreen).
#[tauri::command]
pub fn set_window_shadow(app: tauri::AppHandle, label: String, shadow: bool) -> Result<(), String> {
    use tauri::Manager;
    let Some(win) = app.get_webview_window(&label) else {
        return Err(format!("Window '{}' not found", label));
    };
    win.set_shadow(shadow).map_err(|e| format!("set_shadow: {e}"))
}

/// Capture a window's content as raw BGRA pixels via PrintWindow.
/// Returns (pixels, width, height).
#[tauri::command]
pub fn capture_window_screenshot(app: tauri::AppHandle, label: String) -> Result<(Vec<u8>, u32, u32), String> {
    use tauri::Manager;
    #[cfg(target_os = "windows")] {
        let Some(win) = app.get_webview_window(&label) else {
            return Err(format!("Window '{}' not found", label));
        };
        let hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
        crate::screenshot::capture_window_by_hwnd(hwnd.0 as isize)
            .map_err(|e| format!("capture: {e}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Screenshot only supported on Windows".into())
    }
}

/// Capture a window by label, run OCR pipeline (screenshot → preprocess → BMP → OCR).
#[tauri::command]
pub fn recognize_screenshot(app: tauri::AppHandle, label: String) -> Result<OcrResult, String> {
    use tauri::Manager;
    #[cfg(target_os = "windows")] {
        let Some(win) = app.get_webview_window(&label) else {
            return Err(format!("Window '{}' not found", label));
        };
        let hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
        ocr::recognize_from_screenshot(hwnd.0 as isize)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Screenshot only supported on Windows".into())
    }
}

/// Full pipeline via screenshot: capture window → OCR → filter → match.
#[tauri::command]
pub fn run_screenshot_pipeline(
    app: tauri::AppHandle,
    label: String,
    filter_usernames: bool,
) -> Result<PipelineResult, String> {
    use tauri::Manager;
    #[cfg(target_os = "windows")] {
        let Some(win) = app.get_webview_window(&label) else {
            return Err(format!("Window '{}' not found", label));
        };
        let hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
        pipeline::run_screenshot_pipeline(hwnd.0 as isize, &label, filter_usernames)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Screenshot only supported on Windows".into())
    }
}

/// Capture window screenshot and return raw, preprocessed, and cropped PNG.
#[tauri::command]
pub fn capture_screenshot_images(
    app: tauri::AppHandle,
    label: String,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), String> {
    use tauri::Manager;
    #[cfg(target_os = "windows")] {
        let Some(win) = app.get_webview_window(&label) else {
            return Err(format!("Window '{}' not found", label));
        };
        let hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
        let (pixels, w, h) = crate::screenshot::capture_window_by_hwnd(hwnd.0 as isize)?;

        // Raw PNG
        let raw_png = encode_png(&pixels, w, h)?;

        // Crop to item list region (top 22%)
        let crop = crate::types::Crop {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 0.22,
        };
        let (cropped, cw, ch) = crate::ocr::crop_bgra(&pixels, w, h, crop)?;
        let crop_png = encode_png(&cropped, cw, ch)?;

        // Preprocessed (greyscale + contrast)
        let mut preprocessed = cropped.clone();
        crate::ocr::preprocess_bgra(&mut preprocessed);
        let prep_png = encode_png(&preprocessed, cw, ch)?;

        Ok((raw_png, prep_png, crop_png))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Screenshot only supported on Windows".into())
    }
}

/// Encode BGRA pixels to PNG bytes.
fn encode_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, RgbImage};
    let mut rgb_img: RgbImage = ImageBuffer::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let bgra = &pixels[i..i + 4];
            rgb_img.put_pixel(x, y, image::Rgb([bgra[2], bgra[1], bgra[0]]));
        }
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    rgb_img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("Encode PNG: {e}"))?;
    Ok(buf.into_inner())
}
