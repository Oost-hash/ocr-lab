//! Lab instrumentation and virtual-window capture; not production entry points.
//! The measured stages call the unmodified production OCR primitives.
use crate::ocr::{self, MatchParams};

pub struct TimedRewardExtraction {
    pub is_complete: bool,
    pub skip: bool,
    pub items: Vec<String>,
    pub positions: Vec<f32>,
    pub debug: String,
    pub bmp_encode_ms: u128,
    pub ocr_ms: u128,
    pub match_ms: u128,
}

#[cfg(target_os = "windows")]
pub fn extract_reward_items_timed(params: crate::OcrParams<'_>) -> TimedRewardExtraction {
    let crate::OcrParams { pixels, pix_w, pix_h, game_h: _, catalog, capture_info, hint_squad_size, player_names } = params;
    let mut result = TimedRewardExtraction {
        is_complete: false, skip: false, items: vec![], positions: vec![],
        debug: String::new(), bmp_encode_ms: 0, ocr_ms: 0, match_ms: 0,
    };
    let started = std::time::Instant::now();
    let bmp = ocr::to_bmp(pixels, pix_w, pix_h);
    result.bmp_encode_ms = started.elapsed().as_millis();
    let started = std::time::Instant::now();
    let recognized = ocr::run_windows_ocr(bmp, pix_w, pix_h);
    result.ocr_ms = started.elapsed().as_millis();
    let (raw_full, ocr_lines) = match recognized {
        Ok(value) => value,
        Err(error) => {
            result.debug = format!("├─ Capture  : {}\n└─ OCR error: {}", capture_info, error);
            return result;
        }
    };
    if raw_full.len() < 4 {
        let _ = std::fs::write(
            std::env::temp_dir().join("frameforge_capture_debug.bmp"),
            ocr::to_bmp(pixels, pix_w, pix_h),
        );
        let avg = ocr::avg_brightness(pixels);
        let kind = if avg < 30 { "dark-frame" } else { "ocr-empty" };
        result.debug = format!(
            "├─ Capture  : {}\n└─ OCR      : returned no text ({}, avg={})\n   Saved: %TEMP%\\frameforge_capture_debug.bmp",
            capture_info, kind, avg,
        );
        return result;
    }
    let lower = raw_full.to_lowercase();
    const QUALITY: &[&str] = &["intact", "exceptional", "flawless", "radiant"];
    if lower.contains(" relic") && QUALITY.iter().any(|quality| lower.contains(quality)) {
        result.skip = true;
        result.debug = format!("├─ Capture  : {}\n└─ OCR      : relic selection screen detected (skipped)", capture_info);
        return result;
    }
    let started = std::time::Instant::now();
    (result.is_complete, result.skip, result.items, result.positions, result.debug) =
        ocr::match_reward_items(MatchParams {
            pixels, pix_w, pix_h, raw_full: &raw_full, ocr_lines: &ocr_lines,
            catalog, capture_info, hint_squad_size, player_names,
        });
    result.match_ms = started.elapsed().as_millis();
    result
}

#[cfg(not(target_os = "windows"))]
pub fn extract_reward_items_timed(_params: crate::OcrParams<'_>) -> TimedRewardExtraction {
    TimedRewardExtraction {
        is_complete: false, skip: false, items: vec![], positions: vec![],
        debug: "OCR not supported on this platform".into(),
        bmp_encode_ms: 0, ocr_ms: 0, match_ms: 0,
    }
}

/// Lab capture uses PrintWindow only: a desktop fallback could capture another window.
#[cfg(target_os = "windows")]
pub fn capture_window_reward_area(window_title: &str) -> Result<(Vec<u8>, u32, u32, u32, String), String> {
    use std::mem;
    use windows_sys::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{
            CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
            GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
            DIB_RGB_COLORS, RGBQUAD,
        },
        UI::WindowsAndMessaging::{FindWindowW, GetClientRect},
    };
    #[link(name = "user32")]
    extern "system" { fn PrintWindow(hwnd: isize, hdcblt: isize, nflags: u32) -> i32; }
    unsafe {
        let title: Vec<u16> = window_title.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd == 0 { return Err(format!("Source window not found or too small: {window_title}")); }
        let mut rect: RECT = mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let width = (rect.right - rect.left) as u32;
        let full_height = (rect.bottom - rect.top) as u32;
        if width < 100 || full_height < 100 { return Err(format!("Source window not found or too small: {window_title}")); }
        let capture_height = (full_height as f32 * 0.80) as u32;
        let hdc_win = GetDC(hwnd);
        let hdc_mem = CreateCompatibleDC(hdc_win);
        let hbm = CreateCompatibleBitmap(hdc_win, width as i32, full_height as i32);
        let old = SelectObject(hdc_mem, hbm);
        PrintWindow(hwnd, hdc_mem, 2);
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32, biHeight: -(capture_height as i32),
                biPlanes: 1, biBitCount: 32, biCompression: BI_RGB,
                biSizeImage: 0, biXPelsPerMeter: 0, biYPelsPerMeter: 0,
                biClrUsed: 0, biClrImportant: 0,
            },
            bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
        };
        let mut pixels = vec![0u8; (width * capture_height * 4) as usize];
        GetDIBits(hdc_mem, hbm, 0, capture_height, pixels.as_mut_ptr() as *mut _, &mut bmi, DIB_RGB_COLORS);
        SelectObject(hdc_mem, old);
        DeleteObject(hbm);
        DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_win);
        let brightness = ocr::avg_brightness(&pixels);
        let info = format!("PrintWindow source={window_title:?} {width}x{full_height}px (top 80%, cap {capture_height}px) avg_brightness={brightness}");
        Ok((pixels, width, capture_height, full_height, info))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn capture_window_reward_area(_window_title: &str) -> Result<(Vec<u8>, u32, u32, u32, String), String> {
    Err("Window capture not supported on this platform yet".into())
}
