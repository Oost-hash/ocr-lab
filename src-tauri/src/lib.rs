mod catalog;
mod commands;
mod match_engine;
mod ocr;
mod pipeline;
mod screenshot;
pub mod types;

use commands::CurrentImage;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(CurrentImage(Default::default()))
        .invoke_handler(tauri::generate_handler![
            commands::recognize_image,
            commands::get_current_image_path,
            commands::set_current_image_path,
            commands::read_image_bytes,
            commands::get_image_dimensions,
            commands::get_image_sha256,
            commands::emit_to_overlay,
            commands::position_overlay,
            commands::set_window_shadow,
            commands::capture_window_screenshot,
            commands::recognize_screenshot,
            commands::run_screenshot_pipeline,
            commands::capture_screenshot_images,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OCR Lab");
}
