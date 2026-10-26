//! GB Player: plays the music of Game Boy games from their ROM.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analyzer;
mod audio;
mod cli;
mod console;
mod gui;
mod i18n;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args
        .first()
        .is_some_and(|a| a.to_string_lossy().starts_with("--"))
    {
        console::attach_console();
        std::process::exit(cli::run(&args));
    }
    let initial_rom = args.first().map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GB Player")
            .with_inner_size([720.0, 560.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "GB Player",
        options,
        Box::new(|_cc| Ok(Box::new(gui::GbPlayerApp::new(initial_rom)))),
    )
}
