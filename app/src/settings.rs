//! GUI settings kept between runs, in `gb-player/settings.toml` next to the user song names.

use std::path::PathBuf;

use gb_music::{Profile, Song, DEFAULT_FADE_SECS};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// `None` follows the OS language.
    pub language: Option<String>,
    pub volume: f32,
    /// The volume of each channel (pulse 1, pulse 2, wave, noise), from 0 to 1.
    pub channel_volumes: [f32; 4],
    /// Loops before fading out; `None` loops forever.
    pub loop_count: Option<u32>,
    /// How long the fade-out after the last loop takes, in seconds.
    pub fade_secs: f32,
    /// The tempo games' songs play at, in percent of their own.
    pub tempo: u32,
    /// Where the sound comes out: the line out, or a console's speaker.
    pub profile: Profile,
    /// Whether the speaker plays the console's own noise too.
    pub speaker_noise: bool,
    /// Whether a finished song is followed by the next one.
    pub auto_next: bool,
    pub shuffle: Shuffle,
    /// A built-in palette such as `dmg`, or `custom:<file stem>` for a custom theme.
    pub theme: String,
    /// Whether the files open when the app closed are opened again on startup.
    pub reopen: bool,
    /// How many songs the history keeps.
    pub history_len: usize,
    /// The ROM and GBS files that were open, in the order of the list, opened again on startup.
    pub open_roms: Vec<PathBuf>,
    /// The one of them whose songs were shown.
    pub shown_rom: Option<PathBuf>,
    /// The one of them that was playing.
    pub last_rom: Option<PathBuf>,
    /// The song that was selected in it.
    pub last_song: Option<SavedSong>,
    /// The playlist entry that song was played from.
    pub last_entry: Option<SavedEntry>,
    /// The id of the playlist shown instead of the ROM's songs.
    pub open_playlist: Option<String>,
    pub mini: Mini,
    /// Whether the main window's screen shows the notes instead of the oscilloscope.
    pub notes: bool,
    /// How many times a second the playback state is sent to the UI: the waveform, the meters
    /// and the progress move this often. 0 follows the monitor's refresh rate.
    pub fps: u32,
    /// The folder songs were last exported to.
    pub export_dir: Option<PathBuf>,
}

/// The mini player: the window shrunk to a strip with the song and the transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mini {
    /// Whether the window is the mini player.
    pub on: bool,
    /// Whether the mini player stays above other windows.
    pub on_top: bool,
    /// Whether the mini player's screen shows the notes instead of the oscilloscope.
    pub notes: bool,
    /// Where the mini player was (its size is fixed).
    pub bounds: Option<Bounds>,
    /// Where the full window was before it became the mini player, to go back to. On macOS also
    /// where it was when the app closed.
    pub full: Option<Bounds>,
}

impl Default for Mini {
    fn default() -> Mini {
        Mini {
            on: false,
            on_top: true,
            notes: false,
            bounds: None,
            full: None,
        }
    }
}

/// The place and size of a window, in physical pixels; in points on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

/// What Next (and a finished song) plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shuffle {
    /// The next song of the list, or of the playlist being played.
    #[default]
    Off,
    /// A random song of the ROM, or of the playlist being played.
    List,
    /// A random song of any open ROM, or of the playlist being played.
    All,
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
            channel_volumes: [1.0; 4],
            loop_count: None,
            fade_secs: DEFAULT_FADE_SECS,
            tempo: 100,
            profile: Profile::Line,
            speaker_noise: true,
            auto_next: true,
            shuffle: Shuffle::Off,
            theme: "dmg".into(),
            reopen: true,
            history_len: 100,
            open_roms: Vec::new(),
            shown_rom: None,
            last_rom: None,
            last_song: None,
            last_entry: None,
            open_playlist: None,
            export_dir: None,
            mini: Mini::default(),
            notes: false,
            fps: 0,
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
            channel_volumes: [1.0, 0.5, 0.25, 0.0],
            loop_count: Some(2),
            fade_secs: 3.0,
            tempo: 150,
            profile: Profile::GbcSpeaker,
            speaker_noise: false,
            auto_next: false,
            shuffle: Shuffle::All,
            theme: "custom:night".into(),
            reopen: false,
            history_len: 250,
            open_roms: vec![
                PathBuf::from("C:/roms/game.gb"),
                PathBuf::from("C:/roms/other.gbs"),
            ],
            shown_rom: Some(PathBuf::from("C:/roms/other.gbs")),
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
            export_dir: Some(PathBuf::from("C:/music")),
            notes: true,
            fps: 120,
            mini: Mini {
                on: true,
                on_top: false,
                notes: true,
                bounds: Some(Bounds {
                    x: -1200,
                    y: 40,
                    width: 288,
                    height: 468,
                    maximized: false,
                }),
                full: Some(Bounds {
                    x: 10,
                    y: 20,
                    width: 1040,
                    height: 760,
                    maximized: true,
                }),
            },
        };
        let back: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let partial: Settings = toml::from_str("volume = 0.25").unwrap();
        assert_eq!(partial.volume, 0.25);
        assert_eq!(partial.channel_volumes, [1.0; 4]);
        assert_eq!(partial.loop_count, None);
        assert_eq!(partial.fade_secs, DEFAULT_FADE_SECS);
        assert!(partial.reopen);
        assert_eq!(partial.history_len, 100);
        assert_eq!(partial.fps, 0);
        assert_eq!(partial.tempo, 100);
        assert_eq!(partial.profile, Profile::Line);
        assert!(partial.speaker_noise);
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
        assert!(old.open_roms.is_empty());
        assert_eq!(old.shuffle, Shuffle::Off);
        assert_eq!(old.last_song, None);
        assert_eq!(old.open_playlist, None);
        assert!(!old.mini.on);
        assert!(old.mini.on_top);
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
