// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--image" {
        // CLI mode: --image <path> [--preprocess]
        if args.len() < 3 {
            eprintln!("Usage: ocr-lab --image <path> [--preprocess]");
            std::process::exit(1);
        }

        let path = &args[2];
        let preprocess = args.iter().any(|a| a == "--preprocess");

        match ocr_lab_lib::run_pipeline(path, preprocess) {
            Ok(result) => println!("{}", result),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Tauri GUI mode
        ocr_lab_lib::run();
    }
}
