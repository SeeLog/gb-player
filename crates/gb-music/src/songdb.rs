//! The song database: which songs a game has and what they are called in each language.
//!
//! Songs are grouped into series that share a sound engine and a song set (`songdb/<series>/`
//! in the repository, embedded at build time):
//!
//! * `songs.toml` is language independent. For every game of the series it lists the header
//!   titles it is recognized by and maps the in-ROM song IDs to stable song keys. Songs can
//!   also be hidden or given fixed loop points there.
//! * `<lang>.toml` maps song keys (and game IDs) to display names in one language.
//!
//! Users can add or replace names without rebuilding by putting `<lang>.toml` files in
//! [`user_dir`]`/<series>/`. A title is looked up in this order: user file for the chosen
//! language, built-in file for the chosen language, built-in English, the song key.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::driver::Driver;
use crate::length::{SongLength, FRAME_RATE};

pub const GEN1: &str = "pokemon-gen1";
pub const GEN2: &str = "pokemon-gen2";

/// Language used when a name is missing in the chosen one.
pub const FALLBACK_LANGUAGE: &str = "en";

struct Builtin {
    series: &'static str,
    songs: &'static str,
    locales: &'static [(&'static str, &'static str)],
}

const BUILTIN: &[Builtin] = &[
    Builtin {
        series: GEN1,
        songs: include_str!("../../../songdb/pokemon-gen1/songs.toml"),
        locales: &[
            ("en", include_str!("../../../songdb/pokemon-gen1/en.toml")),
            ("ja", include_str!("../../../songdb/pokemon-gen1/ja.toml")),
        ],
    },
    Builtin {
        series: GEN2,
        songs: include_str!("../../../songdb/pokemon-gen2/songs.toml"),
        locales: &[
            ("en", include_str!("../../../songdb/pokemon-gen2/en.toml")),
            ("ja", include_str!("../../../songdb/pokemon-gen2/ja.toml")),
        ],
    },
];

/// Languages with built-in names, as (code, name in that language).
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("ja", "日本語")];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Series {
    #[serde(rename = "game")]
    pub games: Vec<Game>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Game {
    pub id: String,
    /// Cartridge header titles of the releases that use this mapping.
    #[serde(default)]
    pub titles: Vec<String>,
    pub songs: Vec<SongDef>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongDef {
    /// Audio bank ordinal starting at 1 (Gen 1 only).
    pub bank: Option<u8>,
    pub id: u16,
    pub key: String,
    /// Leave the song out of the list (sound effects that look like music, test songs...).
    #[serde(default)]
    pub hidden: bool,
    /// Fixed loop points in seconds, used instead of the automatic analysis.
    pub intro: Option<f64>,
    #[serde(rename = "loop")]
    pub loop_secs: Option<f64>,
    /// Fixed length in seconds for a song that does not loop.
    pub end: Option<f64>,
}

impl SongDef {
    pub fn length(&self) -> Option<SongLength> {
        let frames = |s: f64| (s * FRAME_RATE).round() as u32;
        match (self.loop_secs, self.end) {
            (Some(period), _) => Some(SongLength::Loops {
                intro: frames(self.intro.unwrap_or(0.0)),
                period: frames(period),
            }),
            (None, Some(at)) => Some(SongLength::Ends { at: frames(at) }),
            (None, None) => None,
        }
    }
}

impl Series {
    pub fn game(&self, id: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.id == id)
    }

    /// The game whose header titles include `title`.
    pub fn game_for_title(&self, title: &str) -> Option<&Game> {
        self.games
            .iter()
            .find(|g| g.titles.iter().any(|t| t == title))
    }
}

/// The display names of one `<lang>.toml` file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NameTable {
    #[serde(default)]
    games: HashMap<String, String>,
    #[serde(default)]
    songs: HashMap<String, String>,
}

struct Loaded {
    series: Series,
    locales: HashMap<&'static str, NameTable>,
}

fn builtin() -> &'static HashMap<&'static str, Loaded> {
    static DB: OnceLock<HashMap<&'static str, Loaded>> = OnceLock::new();
    DB.get_or_init(|| {
        BUILTIN
            .iter()
            .map(|b| {
                let series = toml::from_str(b.songs)
                    .unwrap_or_else(|e| panic!("songdb/{}/songs.toml: {e}", b.series));
                let locales = b
                    .locales
                    .iter()
                    .map(|&(lang, text)| {
                        let names = toml::from_str(text)
                            .unwrap_or_else(|e| panic!("songdb/{}/{lang}.toml: {e}", b.series));
                        (lang, names)
                    })
                    .collect();
                (b.series, Loaded { series, locales })
            })
            .collect()
    })
}

/// The built-in song set of `series`.
pub fn series(series: &str) -> &'static Series {
    &builtin()
        .get(series)
        .unwrap_or_else(|| panic!("unknown song series {series}"))
        .series
}

/// Where users can put their own `<series>/<lang>.toml` name files.
pub fn user_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("gb-player").join("songdb"))
}

/// Reduce a locale tag such as `ja-JP` or `ja_JP.UTF-8` to its language code.
pub fn language_code(tag: &str) -> String {
    tag.split(['-', '_', '.'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Name lookup for one series in one language.
pub struct Localizer {
    layers: Vec<NameTable>,
    builtin: Vec<&'static NameTable>,
}

impl Localizer {
    /// Build the lookup chain for `language`. Problems with the user's files are returned as
    /// warnings; the built-in names are still used.
    pub fn new(series: &str, language: &str) -> (Localizer, Vec<String>) {
        let mut warnings = Vec::new();
        let mut layers = Vec::new();
        if let Some(path) = user_dir().map(|d| d.join(series).join(format!("{language}.toml"))) {
            match std::fs::read_to_string(&path) {
                Ok(text) => match toml::from_str(&text) {
                    Ok(names) => layers.push(names),
                    Err(e) => warnings.push(format!("{}: {e}", path.display())),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => warnings.push(format!("{}: {e}", path.display())),
            }
        }
        let locales = builtin().get(series).map(|l| &l.locales);
        let builtin = [language, FALLBACK_LANGUAGE]
            .iter()
            .filter_map(|lang| locales.and_then(|l| l.get(*lang)))
            .collect();
        (Localizer { layers, builtin }, warnings)
    }

    fn lookup(&self, pick: impl Fn(&NameTable) -> Option<&String>) -> Option<String> {
        self.layers
            .iter()
            .chain(self.builtin.iter().copied())
            .find_map(|n| pick(n).cloned())
    }

    pub fn song(&self, key: &str) -> String {
        self.lookup(|n| n.songs.get(key))
            .unwrap_or_else(|| key.to_string())
    }

    pub fn game(&self, id: &str) -> String {
        self.lookup(|n| n.games.get(id))
            .unwrap_or_else(|| id.to_string())
    }
}

/// The game and song names of a detected ROM in one language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names {
    pub game: String,
    /// Song titles, in the order of [`Driver::songs`].
    pub songs: Vec<String>,
}

impl Names {
    /// Look up the names of `driver`'s game and songs in `language`. Problems found in the
    /// user's name files are returned as warnings; those files are then skipped.
    pub fn new(driver: &Driver, language: &str) -> (Names, Vec<String>) {
        let (names, warnings) = Localizer::new(driver.series, language);
        let songs = driver
            .songs
            .iter()
            .map(|song| match &song.key {
                Some(key) => names.song(key),
                None => format!("Song ${:02X}", song.id),
            })
            .collect();
        let game = names.game(&driver.game_id);
        (Names { game, songs }, warnings)
    }

    /// The title of the song at `index` in [`Driver::songs`].
    pub fn song(&self, index: usize) -> &str {
        self.songs.get(index).map_or("", String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_files_parse_and_every_key_has_an_english_name() {
        for b in BUILTIN {
            let loaded = &builtin()[b.series];
            let en = &loaded.locales["en"];
            for game in &loaded.series.games {
                assert!(en.games.contains_key(&game.id), "{}: {}", b.series, game.id);
                for song in &game.songs {
                    assert!(
                        en.songs.contains_key(&song.key),
                        "{}: {}",
                        b.series,
                        song.key
                    );
                }
            }
            // Translations must not invent keys that no game uses.
            for (lang, names) in &loaded.locales {
                for key in names.songs.keys() {
                    let used = loaded
                        .series
                        .games
                        .iter()
                        .any(|g| g.songs.iter().any(|s| &s.key == key));
                    assert!(used, "{}/{lang}.toml: unknown key {key}", b.series);
                }
            }
        }
    }

    #[test]
    fn fallback_to_english_then_key() {
        let (ja, _) = Localizer::new(GEN1, "ja");
        assert_eq!(ja.song("pallet_town"), "マサラタウン");
        let (fr, _) = Localizer::new(GEN1, "fr");
        assert_eq!(fr.song("pallet_town"), "Pallet Town");
        assert_eq!(fr.song("no_such_song"), "no_such_song");
    }

    #[test]
    fn locale_tags() {
        assert_eq!(language_code("ja-JP"), "ja");
        assert_eq!(language_code("en_US.UTF-8"), "en");
    }
}
