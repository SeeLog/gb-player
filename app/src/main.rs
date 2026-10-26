//! GB Player: plays the music of Game Boy games from their ROM.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analyzer;
mod audio;
mod cli;
mod console;
mod gui;
mod i18n;
mod settings;
mod themes;

use std::path::PathBuf;

fn main() {
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    // `--lang <code>` applies to both the GUI and the CLI commands.
    let mut language = None;
    if args.first().is_some_and(|a| a == "--lang") && args.len() >= 2 {
        language = Some(args[1].to_string_lossy().into_owned());
        args.drain(..2);
    }
    if args
        .first()
        .is_some_and(|a| a.to_string_lossy().starts_with("--"))
    {
        console::attach_console();
        let language = language.unwrap_or_else(i18n::system_language);
        std::process::exit(cli::run(&args, &language));
    }
    gui::run(args.first().map(PathBuf::from), language);
}
