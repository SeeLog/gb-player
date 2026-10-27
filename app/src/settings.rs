//! GUI settings kept between runs, in `gb-player/settings.toml` next to the user song names.

use std::path::PathBuf;

use gb_music::Song;
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
    /// A built-in palette such as `dmg`, or `custom:<file stem>` for a custom theme.
    pub theme: String,
    /// The ROM or GBS file that was open, opened again on startup.
    pub last_rom: Option<PathBuf>,
    /// The song that was selected in it.
    pub last_song: Option<SavedSong>,
    /// The playlist entry that song was played from.
    pub last_entry: Option<SavedEntry>,
    /// The id of the playlist shown instead of the ROM's songs.
    pub open_playlist: Option<String>,
}

/// A song of the last ROM: by its song database key when it has one, so that a change to the
/// database does not pick another song, and by its position otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSong {
    pub key: Option<String>,
    pub index: usize,
}

impl SavedSong {
    pub fn new(song: &Song) -> SavedSong {
        SavedSong {
            key: song.key.clone(),
            index: song.index,
        }
    }

    /// The song's index in `songs`, if it is still there.
    pub fn find(&self, songs: &[Song]) -> Option<usize> {
        match &self.key {
            Some(key) => songs.iter().find(|s| s.key.as_ref() == Some(key)),
            None => songs.iter().find(|s| s.index == self.index),
        }
        .map(|s| s.index)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedEntry {
    pub playlist: String,
    pub position: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            language: None,
            volume: 0.8,
            loop_count: None,
            auto_next: true,
            theme: "dmg".into(),
            last_rom: None,
            last_song: None,
            last_entry: None,
            open_playlist: None,
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
            theme: "custom:night".into(),
            last_rom: Some(PathBuf::from("C:/roms/game.gb")),
            last_song: Some(SavedSong {
                key: Some("route_1".into()),
                index: 3,
            }),
            last_entry: Some(SavedEntry {
                playlist: "a1".into(),
                position: 2,
            }),
            open_playlist: Some("a1".into()),
        };
        let back: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let partial: Settings = toml::from_str("volume = 0.25").unwrap();
        assert_eq!(partial.volume, 0.25);
        assert_eq!(partial.loop_count, None);
        assert!(partial.auto_next);
        assert_eq!(partial.theme, "dmg");
        let none = Settings::default();
        assert_eq!(
            toml::from_str::<Settings>(&toml::to_string(&none).unwrap()).unwrap(),
            none
        );
    }

    #[test]
    fn settings_from_before_the_session_was_saved_still_load() {
        let old: Settings = toml::from_str(
            "language = \"ja\"
volume = 0.5
loop_count = 2
auto_next = false
theme = \"gbp\"
",
        )
        .unwrap();
        assert_eq!(old.language.as_deref(), Some("ja"));
        assert_eq!(old.theme, "gbp");
        assert_eq!(old.last_rom, None);
        assert_eq!(old.last_song, None);
        assert_eq!(old.open_playlist, None);
    }

    fn song(index: usize, key: Option<&str>) -> Song {
        Song {
            index,
            key: key.map(String::from),
            id: index as u16,
            bank: 0,
            length: None,
        }
    }

    #[test]
    fn a_saved_song_is_found_by_key_before_index() {
        let songs = [
            song(0, Some("title")),
            song(1, Some("route_1")),
            song(2, None),
        ];
        let saved = SavedSong::new(&songs[1]);
        assert_eq!(saved.find(&songs), Some(1));
        // The database moved the song: the key still finds it.
        let moved = [song(0, Some("route_1")), song(1, Some("title"))];
        assert_eq!(saved.find(&moved), Some(0));
        // The key is gone: no other song is picked in its place.
        assert_eq!(saved.find(&[song(0, Some("title")), song(1, None)]), None);
        // A song without a key is found by its index.
        assert_eq!(SavedSong::new(&songs[2]).find(&songs), Some(2));
        assert_eq!(SavedSong::new(&songs[2]).find(&songs[..2]), None);
    }
}
