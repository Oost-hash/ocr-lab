use std::{io::Cursor, time::Instant};

use image::ImageReader;
use sha2::Digest;

use crate::types::{Crop, OcrLine, OcrResult, Stage};

// ─── Public API ──────────────────────────────────────────────────────────────

/// Load an image file, optionally crop/preprocess, run Windows OCR.
pub fn recognize_from_file(
    path: &str,
    crop: Crop,
    preprocess: bool,
) -> Result<OcrResult, String> {
    let mut stages = Vec::new();

    // Read file
    let read_start = Instant::now();
    let encoded = std::fs::read(path).map_err(|e| format!("Read image: {e}"))?;
    stages.push(Stage { name: "input_read".into(), wall_ms: elapsed_ms(read_start) });

    // SHA-256
    let hash_start = Instant::now();
    let sha256 = format!("{:x}", sha2::Sha256::digest(&encoded));
    let _ = sha256; // used by caller
    stages.push(Stage { name: "input_hash".into(), wall_ms: elapsed_ms(hash_start) });

    // Decode image
    let decode_start = Instant::now();
    let image = ImageReader::new(Cursor::new(encoded))
        .with_guessed_format()
        .map_err(|e| format!("Identify image format: {e}"))?
        .decode()
        .map_err(|e| format!("Decode image: {e}"))?
        .to_rgba8();
    let width = image.width();
    let height = image.height();
    stages.push(Stage { name: "image_decode".into(), wall_ms: elapsed_ms(decode_start) });

    // RGBA → BGRA
    let normalize_start = Instant::now();
    let mut bgra = image.into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    stages.push(Stage { name: "frame_normalize".into(), wall_ms: elapsed_ms(normalize_start) });

    // Crop
    let crop_start = Instant::now();
    let (mut frame, fw, fh) = crop_bgra(&bgra, width, height, crop)?;
    stages.push(Stage { name: "crop".into(), wall_ms: elapsed_ms(crop_start) });

    // Preprocess
    if preprocess {
        let preprocess_start = Instant::now();
        preprocess_bgra(&mut frame);
        stages.push(Stage { name: "preprocess".into(), wall_ms: elapsed_ms(preprocess_start) });
    }

    // BMP encode
    let bmp_start = Instant::now();
    let bmp = encode_bmp(&frame, fw, fh)?;
    stages.push(Stage { name: "bmp_encode".into(), wall_ms: elapsed_ms(bmp_start) });

    // Windows OCR
    let (text, lines, com_ms, bitmap_ms, engine_ms, recognize_ms, parse_ms) =
        recognize_windows_ocr(&bmp, fw, fh)?;
    stages.push(Stage { name: "com_initialize".into(), wall_ms: com_ms });
    stages.push(Stage { name: "winrt_bitmap_decode".into(), wall_ms: bitmap_ms });
    stages.push(Stage { name: "ocr_engine_create".into(), wall_ms: engine_ms });
    stages.push(Stage { name: "ocr_recognize".into(), wall_ms: recognize_ms });
    stages.push(Stage { name: "ocr_result_parse".into(), wall_ms: parse_ms });

    Ok(OcrResult { text, lines, stages })
}

pub fn get_sha256(path: &str) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| format!("Read image: {e}"))?;
    Ok(format!("{:x}", sha2::Sha256::digest(&data)))
}

pub fn get_image_dimensions(path: &str) -> Result<(u32, u32), String> {
    let data = std::fs::read(path).map_err(|e| format!("Read image: {e}"))?;
    let image = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("Identify image format: {e}"))?
        .decode()
        .map_err(|e| format!("Decode image: {e}"))?;
    Ok((image.width(), image.height()))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

pub fn crop_bgra(
    pixels: &[u8],
    width: u32,
    height: u32,
    crop: Crop,
) -> Result<(Vec<u8>, u32, u32), String> {
    if !(0.0..=1.0).contains(&crop.left)
        || !(0.0..=1.0).contains(&crop.top)
        || !(0.0..=1.0).contains(&crop.right)
        || !(0.0..=1.0).contains(&crop.bottom)
        || crop.left >= crop.right
        || crop.top >= crop.bottom
    {
        return Err("Crop must be ordered normalized values between 0 and 1".into());
    }
    let x0 = (width as f32 * crop.left) as u32;
    let y0 = (height as f32 * crop.top) as u32;
    let x1 = (width as f32 * crop.right).ceil() as u32;
    let y1 = (height as f32 * crop.bottom).ceil() as u32;
    let cw = x1.saturating_sub(x0);
    let ch = y1.saturating_sub(y0);
    if cw < 4 || ch < 4 {
        return Err("Crop must be at least 4 by 4 pixels".into());
    }
    let mut out = vec![0u8; (cw * ch * 4) as usize];
    let src_stride = width as usize * 4;
    let dst_stride = cw as usize * 4;
    for row in 0..ch as usize {
        let src = (y0 as usize + row) * src_stride + x0 as usize * 4;
        let dst = row * dst_stride;
        out[dst..dst + dst_stride].copy_from_slice(&pixels[src..src + dst_stride]);
    }
    Ok((out, cw, ch))
}

pub fn preprocess_bgra(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let gray = ((px[2] as u32 * 299 + px[1] as u32 * 587 + px[0] as u32 * 114) / 1_000) as i32;
        let v = ((gray - 20) * 255 / 215).clamp(0, 255) as u8;
        px[0] = v;
        px[1] = v;
        px[2] = v;
    }
}

fn encode_bmp(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    if pixels.len() != (width as usize * height as usize * 4) {
        return Err("Invalid BGRA frame length".into());
    }
    let row_bytes = width * 3;
    let padding = (4 - row_bytes % 4) % 4;
    let image_size = (row_bytes + padding) * height;
    let mut bmp = Vec::with_capacity((54 + image_size) as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(54 + image_size).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&image_size.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    for pixel in pixels.chunks_exact(4) {
        bmp.extend_from_slice(&pixel[..3]);
        if (bmp.len() - 54) as u32 % (row_bytes + padding) == row_bytes {
            bmp.extend(std::iter::repeat_n(0, padding as usize));
        }
    }
    Ok(bmp)
}

#[cfg(target_os = "windows")]
fn recognize_windows_ocr(
    bmp: &[u8],
    width: u32,
    height: u32,
) -> Result<(String, Vec<OcrLine>, f64, f64, f64, f64, f64), String> {
    use windows::{
        Foundation::Collections::IVectorView,
        Globalization::Language,
        Graphics::Imaging::BitmapDecoder,
        Media::Ocr::{OcrEngine, OcrLine as WinOcrLine},
        Storage::Streams::{DataWriter, InMemoryRandomAccessStream},
    };

    let com_start = Instant::now();
    unsafe {
        windows_sys::Win32::System::Com::CoInitializeEx(
            std::ptr::null(),
            windows_sys::Win32::System::Com::COINIT_MULTITHREADED.try_into().unwrap_or(0),
        );
    }
    let com_ms = elapsed_ms(com_start);

    let bitmap_start = Instant::now();
    let stream = InMemoryRandomAccessStream::new()
        .map_err(|e| format!("Create OCR stream: {e}"))?;
    let writer = DataWriter::CreateDataWriter(&stream)
        .map_err(|e| format!("Create OCR writer: {e}"))?;
    writer.WriteBytes(bmp).map_err(|e| format!("Write OCR bytes: {e}"))?;
    writer.StoreAsync().map_err(|e| format!("Store OCR bytes: {e}"))?.get()
        .map_err(|e| format!("Store OCR bytes: {e}"))?;
    writer.FlushAsync().map_err(|e| format!("Flush OCR bytes: {e}"))?.get()
        .map_err(|e| format!("Flush OCR bytes: {e}"))?;
    writer.DetachStream().map_err(|e| format!("Detach OCR stream: {e}"))?;
    stream.Seek(0).map_err(|e| format!("Seek OCR stream: {e}"))?;
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| format!("Decode OCR bitmap: {e}"))?.get()
        .map_err(|e| format!("Decode OCR bitmap: {e}"))?;
    let bitmap = decoder.GetSoftwareBitmapAsync()
        .map_err(|e| format!("Read OCR bitmap: {e}"))?.get()
        .map_err(|e| format!("Read OCR bitmap: {e}"))?;
    let bitmap_ms = elapsed_ms(bitmap_start);

    let engine_start = Instant::now();
    let language = Language::CreateLanguage(&windows::core::HSTRING::from("en-US"))
        .map_err(|e| format!("Create OCR language: {e}"))?;
    let engine = OcrEngine::TryCreateFromLanguage(&language)
        .map_err(|_| "Windows OCR language pack en-US unavailable. Install in Windows Settings > Time & language.".to_string())?;
    let engine_ms = elapsed_ms(engine_start);

    let recognize_start = Instant::now();
    let recognized = engine.RecognizeAsync(&bitmap)
        .map_err(|e| format!("Start OCR recognition: {e}"))?.get()
        .map_err(|e| format!("Run OCR recognition: {e}"))?;
    let recognize_ms = elapsed_ms(recognize_start);

    let parse_start = Instant::now();
    let lines: IVectorView<WinOcrLine> = recognized.Lines()
        .map_err(|e| format!("Read OCR lines: {e}"))?;
    let mut text = String::new();
    let mut output = Vec::new();
    for index in 0..lines.Size().map_err(|e| format!("Count OCR lines: {e}"))? {
        let line = lines.GetAt(index).map_err(|e| format!("Read OCR line: {e}"))?;
        let line_text = line.Text().map_err(|e| format!("Read OCR text: {e}"))?.to_string();
        let words = line.Words().map_err(|e| format!("Read OCR words: {e}"))?;
        let count = words.Size().map_err(|e| format!("Count OCR words: {e}"))?;
        let mut x_sum = 0.0;
        let mut y_sum = 0.0;
        for wi in 0..count {
            let rect = words.GetAt(wi).map_err(|e| format!("Read OCR word: {e}"))?
                .BoundingRect().map_err(|e| format!("Read OCR bounds: {e}"))?;
            x_sum += (rect.X + rect.Width / 2.0) / width as f32;
            y_sum += (rect.Y + rect.Height / 2.0) / height as f32;
        }
        text.push_str(&line_text);
        text.push('\n');
        output.push(OcrLine {
            text: line_text,
            x_center: if count == 0 { 0.5 } else { x_sum / count as f32 },
            y_center: if count == 0 { 0.5 } else { y_sum / count as f32 },
        });
    }
    let parse_ms = elapsed_ms(parse_start);
    Ok((text, output, com_ms, bitmap_ms, engine_ms, recognize_ms, parse_ms))
}

#[cfg(not(target_os = "windows"))]
fn recognize_windows_ocr(
    _: &[u8], _: u32, _: u32,
) -> Result<(String, Vec<OcrLine>, f64, f64, f64, f64, f64), String> {
    Err("Windows Media OCR is only available on Windows".into())
}

// ─── Screenshot capture pipeline ─────────────────────────────────────────────

/// Capture a window by HWND, crop, preprocess, OCR — no temp files.
/// Same approach as FrameForge's capture_warframe_pixels + ocr_pixels_rect.
pub fn recognize_from_screenshot(hwnd: isize) -> Result<OcrResult, String> {
    let mut stages = Vec::new();

    // Capture window via PrintWindow
    let capture_start = Instant::now();
    let (pixels, w, h) = crate::screenshot::capture_window_by_hwnd(hwnd)?;
    stages.push(Stage { name: "screenshot_capture".into(), wall_ms: elapsed_ms(capture_start) });

    // Crop to item list region (top 22% of screen)
    let crop_start = Instant::now();
    let crop = crate::types::Crop {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 0.22,
    };
    let (cropped, cw, ch) = crop_bgra(&pixels, w, h, crop)?;
    stages.push(Stage { name: "crop".into(), wall_ms: elapsed_ms(crop_start) });

    // Preprocess (grayscale + contrast)
    let preprocess_start = Instant::now();
    let mut frame = cropped;
    preprocess_bgra(&mut frame);
    stages.push(Stage { name: "preprocess".into(), wall_ms: elapsed_ms(preprocess_start) });

    // Encode to BMP in memory
    let bmp_start = Instant::now();
    let bmp = encode_bmp(&frame, cw, ch)?;
    stages.push(Stage { name: "bmp_encode".into(), wall_ms: elapsed_ms(bmp_start) });

    // Windows OCR
    let (text, lines, com_ms, bitmap_ms, engine_ms, recognize_ms, parse_ms) =
        recognize_windows_ocr(&bmp, cw, ch)?;
    stages.push(Stage { name: "com_initialize".into(), wall_ms: com_ms });
    stages.push(Stage { name: "winrt_bitmap_decode".into(), wall_ms: bitmap_ms });
    stages.push(Stage { name: "ocr_engine_create".into(), wall_ms: engine_ms });
    stages.push(Stage { name: "ocr_recognize".into(), wall_ms: recognize_ms });
    stages.push(Stage { name: "ocr_result_parse".into(), wall_ms: parse_ms });

    Ok(OcrResult { text, lines, stages })
}
