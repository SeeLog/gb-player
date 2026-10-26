//! GUI settings kept between runs, in `gb-player/settings.toml` next to the user song names.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// `None` follows the OS language.
    pub language: Option<String>,
    pub volume: f32,
    /// Loops before fading out; `None` loops forever.
    pub loop_count: Option<u32>,
    /// Whether a finished song is followed by the next one.
    pub auto_next: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            language: None,
            volume: 0.8,
            loop_count: None,
            auto_next: true,
        }
    }
}

fn path() -> Option<PathBuf> {
    Some(
        gb_music::songdb::user_dir()?
            .parent()?
            .join("settings.toml"),
    )
}

impl Settings {
    /// The saved settings, or the defaults when there are none or they cannot be read.
    pub fn load() -> Settings {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        let Ok(text) = toml::to_string(self) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("{}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_missing_fields() {
        let s = Settings {
            language: Some("ja".into()),
            volume: 0.5,
            loop_count: Some(2),
            auto_next: false,
        };
        let back: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let partial: Settings = toml::from_str("volume = 0.25").unwrap();
        assert_eq!(partial.volume, 0.25);
        assert_eq!(partial.loop_count, None);
        assert!(partial.auto_next);
    }
}
