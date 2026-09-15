use std::{fs, io::Cursor, path::Path, time::Instant};

use image::ImageReader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Crop {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[derive(Serialize)]
struct Stage {
    name: &'static str,
    wall_ms: f64,
}

#[derive(Serialize)]
struct OcrLine {
    text: String,
    x_center: f32,
    y_center: f32,
}

#[derive(Serialize)]
struct OcrRun {
    input: InputInfo,
    config: RunConfig,
    backend: &'static str,
    language: &'static str,
    stages: Vec<Stage>,
    total_ms: f64,
    text: String,
    lines: Vec<OcrLine>,
}

#[derive(Serialize)]
struct InputInfo {
    filename: String,
    sha256: String,
    width_px: u32,
    height_px: u32,
}

#[derive(Serialize)]
struct RunConfig {
    crop_norm: [f32; 4],
    preprocess: bool,
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn crop_bgra(
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
    let crop_width = x1.saturating_sub(x0);
    let crop_height = y1.saturating_sub(y0);
    if crop_width < 4 || crop_height < 4 {
        return Err("Crop must be at least 4 by 4 pixels".into());
    }

    let mut out = vec![0; (crop_width * crop_height * 4) as usize];
    let source_stride = width as usize * 4;
    let target_stride = crop_width as usize * 4;
    for row in 0..crop_height as usize {
        let source_start = (y0 as usize + row) * source_stride + x0 as usize * 4;
        let target_start = row * target_stride;
        out[target_start..target_start + target_stride]
            .copy_from_slice(&pixels[source_start..source_start + target_stride]);
    }
    Ok((out, crop_width, crop_height))
}

fn preprocess_bgra(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let gray = ((pixel[2] as u32 * 299 + pixel[1] as u32 * 587 + pixel[0] as u32 * 114) / 1_000)
            as i32;
        let value = ((gray - 20) * 255 / 215).clamp(0, 255) as u8;
        pixel[0] = value;
        pixel[1] = value;
        pixel[2] = value;
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
            windows_sys::Win32::System::Com::COINIT_MULTITHREADED
                .try_into()
                .unwrap_or(0),
        );
    }
    let com_ms = elapsed_ms(com_start);

    let bitmap_start = Instant::now();
    let stream =
        InMemoryRandomAccessStream::new().map_err(|error| format!("Create OCR stream: {error}"))?;
    let writer = DataWriter::CreateDataWriter(&stream)
        .map_err(|error| format!("Create OCR writer: {error}"))?;
    writer
        .WriteBytes(bmp)
        .map_err(|error| format!("Write OCR bytes: {error}"))?;
    writer
        .StoreAsync()
        .map_err(|error| format!("Store OCR bytes: {error}"))?
        .get()
        .map_err(|error| format!("Store OCR bytes: {error}"))?;
    writer
        .FlushAsync()
        .map_err(|error| format!("Flush OCR bytes: {error}"))?
        .get()
        .map_err(|error| format!("Flush OCR bytes: {error}"))?;
    writer
        .DetachStream()
        .map_err(|error| format!("Detach OCR stream: {error}"))?;
    stream
        .Seek(0)
        .map_err(|error| format!("Seek OCR stream: {error}"))?;
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|error| format!("Decode OCR bitmap: {error}"))?
        .get()
        .map_err(|error| format!("Decode OCR bitmap: {error}"))?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(|error| format!("Read OCR bitmap: {error}"))?
        .get()
        .map_err(|error| format!("Read OCR bitmap: {error}"))?;
    let bitmap_ms = elapsed_ms(bitmap_start);

    let engine_start = Instant::now();
    let language = Language::CreateLanguage(&windows::core::HSTRING::from("en-US"))
        .map_err(|error| format!("Create OCR language: {error}"))?;
    let engine = OcrEngine::TryCreateFromLanguage(&language)
        .map_err(|_| "Windows OCR language pack en-US is unavailable. Install it in Windows Settings > Time & language > Language & region.".to_string())?;
    let engine_ms = elapsed_ms(engine_start);

    let recognize_start = Instant::now();
    let recognized = engine
        .RecognizeAsync(&bitmap)
        .map_err(|error| format!("Start OCR recognition: {error}"))?
        .get()
        .map_err(|error| format!("Run OCR recognition: {error}"))?;
    let recognize_ms = elapsed_ms(recognize_start);
    let parse_start = Instant::now();
    let lines: IVectorView<WinOcrLine> = recognized
        .Lines()
        .map_err(|error| format!("Read OCR lines: {error}"))?;
    let mut text = String::new();
    let mut output = Vec::new();
    for index in 0..lines
        .Size()
        .map_err(|error| format!("Count OCR lines: {error}"))?
    {
        let line = lines
            .GetAt(index)
            .map_err(|error| format!("Read OCR line: {error}"))?;
        let line_text = line
            .Text()
            .map_err(|error| format!("Read OCR text: {error}"))?
            .to_string();
        let words = line
            .Words()
            .map_err(|error| format!("Read OCR words: {error}"))?;
        let count = words
            .Size()
            .map_err(|error| format!("Count OCR words: {error}"))?;
        let mut x_sum = 0.0;
        let mut y_sum = 0.0;
        for word_index in 0..count {
            let rect = words
                .GetAt(word_index)
                .map_err(|error| format!("Read OCR word: {error}"))?
                .BoundingRect()
                .map_err(|error| format!("Read OCR bounds: {error}"))?;
            x_sum += (rect.X + rect.Width / 2.0) / width as f32;
            y_sum += (rect.Y + rect.Height / 2.0) / height as f32;
        }
        text.push_str(&line_text);
        text.push('\n');
        output.push(OcrLine {
            text: line_text,
            x_center: if count == 0 {
                0.5
            } else {
                x_sum / count as f32
            },
            y_center: if count == 0 {
                0.5
            } else {
                y_sum / count as f32
            },
        });
    }
    let parse_ms = elapsed_ms(parse_start);
    Ok((
        text,
        output,
        com_ms,
        bitmap_ms,
        engine_ms,
        recognize_ms,
        parse_ms,
    ))
}

#[cfg(not(target_os = "windows"))]
fn recognize_windows_ocr(
    _: &[u8],
    _: u32,
    _: u32,
) -> Result<(String, Vec<OcrLine>, f64, f64, f64, f64, f64), String> {
    Err("Windows Media OCR is only available on Windows".into())
}

#[tauri::command]
fn recognize_image(path: String, crop: Crop, preprocess: bool) -> Result<OcrRun, String> {
    let total_start = Instant::now();
    let mut stages = Vec::new();

    let read_start = Instant::now();
    let encoded = fs::read(&path).map_err(|error| format!("Read image: {error}"))?;
    stages.push(Stage {
        name: "input_read",
        wall_ms: elapsed_ms(read_start),
    });
    let hash_start = Instant::now();
    let sha256 = format!("{:x}", Sha256::digest(&encoded));
    stages.push(Stage {
        name: "input_hash",
        wall_ms: elapsed_ms(hash_start),
    });

    let decode_start = Instant::now();
    let image = ImageReader::new(Cursor::new(encoded))
        .with_guessed_format()
        .map_err(|error| format!("Identify image format: {error}"))?
        .decode()
        .map_err(|error| format!("Decode image: {error}"))?
        .to_rgba8();
    let width = image.width();
    let height = image.height();
    stages.push(Stage {
        name: "image_decode",
        wall_ms: elapsed_ms(decode_start),
    });

    let normalize_start = Instant::now();
    let mut bgra = image.into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    stages.push(Stage {
        name: "frame_normalize",
        wall_ms: elapsed_ms(normalize_start),
    });

    let crop_start = Instant::now();
    let (mut frame, frame_width, frame_height) = crop_bgra(&bgra, width, height, crop)?;
    stages.push(Stage {
        name: "crop",
        wall_ms: elapsed_ms(crop_start),
    });

    if preprocess {
        let preprocess_start = Instant::now();
        preprocess_bgra(&mut frame);
        stages.push(Stage {
            name: "preprocess",
            wall_ms: elapsed_ms(preprocess_start),
        });
    }

    let bmp_start = Instant::now();
    let bmp = encode_bmp(&frame, frame_width, frame_height)?;
    stages.push(Stage {
        name: "bmp_encode",
        wall_ms: elapsed_ms(bmp_start),
    });

    let (text, lines, com_ms, bitmap_ms, engine_ms, recognize_ms, parse_ms) =
        recognize_windows_ocr(&bmp, frame_width, frame_height)?;
    stages.push(Stage {
        name: "com_initialize",
        wall_ms: com_ms,
    });
    stages.push(Stage {
        name: "winrt_bitmap_decode",
        wall_ms: bitmap_ms,
    });
    stages.push(Stage {
        name: "ocr_engine_create",
        wall_ms: engine_ms,
    });
    stages.push(Stage {
        name: "ocr_recognize",
        wall_ms: recognize_ms,
    });
    stages.push(Stage {
        name: "ocr_result_parse",
        wall_ms: parse_ms,
    });

    let serialize_start = Instant::now();
    let filename = Path::new(&path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("image")
        .to_owned();
    let input = InputInfo {
        filename,
        sha256,
        width_px: width,
        height_px: height,
    };
    let config = RunConfig {
        crop_norm: [crop.left, crop.top, crop.right, crop.bottom],
        preprocess,
    };
    let _ = serde_json::to_vec(&(&input, &config, &text, &lines))
        .map_err(|error| format!("Serialize result: {error}"))?;
    stages.push(Stage {
        name: "result_serialize",
        wall_ms: elapsed_ms(serialize_start),
    });

    Ok(OcrRun {
        input,
        config,
        backend: "windows_media_ocr",
        language: "en-US",
        stages,
        total_ms: elapsed_ms(total_start),
        text,
        lines,
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![recognize_image])
        .run(tauri::generate_context!())
        .expect("error while running OCR Lab");
}
