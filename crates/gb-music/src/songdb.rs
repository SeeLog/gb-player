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
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::config::{PlaybackDef, ProbeDef, MAX_SONGS};
use crate::driver::Driver;
use crate::length::{SongLength, FRAME_RATE};

pub const GEN1: &str = "pokemon-gen1";
pub const GEN2: &str = "pokemon-gen2";

/// Language used when a name is missing in the chosen one.
pub const FALLBACK_LANGUAGE: &str = "en";

/// Longest fixed song length accepted from the database, in seconds.
pub const MAX_SONG_SECS: f64 = 3600.0;

/// Largest user file that is read; anything bigger is reported and skipped.
pub const MAX_USER_FILE: u64 = 1 << 20;

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
    /// Global checksums ($014E-$014F) of the releases, to tell apart releases that share a
    /// header title; when empty, the title alone decides.
    #[serde(default)]
    pub checksums: Vec<u16>,
    #[serde(default)]
    pub songs: Vec<SongDef>,
    /// Also list the IDs from `first` to `last` that `songs` leaves out, as "Song $XX".
    #[serde(default)]
    pub song_range: Option<SongRange>,
    /// How to drive the sound engine, for games without a built-in detector.
    #[serde(default)]
    pub playback: Option<PlaybackDef>,
    /// Where the engine keeps each channel's position, for exact loop points.
    #[serde(default)]
    pub probe: Option<ProbeDef>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongRange {
    pub first: u16,
    pub last: u16,
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

/// Longest game ID, song key or header title accepted from the database.
const MAX_NAME: usize = 64;

impl Game {
    /// Whether the game lists this header title and, if it lists any, this checksum.
    pub fn matches(&self, title: &str, checksum: u16) -> bool {
        self.titles.iter().any(|t| t == title)
            && (self.checksums.is_empty() || self.checksums.contains(&checksum))
    }

    /// Check the limits that keep a user's database file from making the player work
    /// without end; [`Game::playback`] and [`Game::probe`] are only valid after this.
    pub fn validate(&self) -> Result<(), String> {
        let name_ok = |s: &str| !s.is_empty() && s.len() <= MAX_NAME;
        if !name_ok(&self.id) {
            return Err(format!("game id must have 1 to {MAX_NAME} characters"));
        }
        if self.titles.len() > MAX_NAME || self.titles.iter().any(|t| t.len() > 16) {
            return Err("titles: at most 64 titles of up to 16 characters".into());
        }
        if self.checksums.len() > MAX_NAME {
            return Err("checksums: at most 64 checksums".into());
        }
        // The count of `songs` first: song_ids takes time quadratic in it.
        if self.songs.len() > MAX_SONGS || self.song_ids().len() > MAX_SONGS {
            return Err(format!("more than {MAX_SONGS} songs"));
        }
        for song in &self.songs {
            if !name_ok(&song.key) {
                return Err(format!("song key must have 1 to {MAX_NAME} characters"));
            }
            let secs = [song.intro, song.loop_secs, song.end];
            if secs
                .iter()
                .flatten()
                .any(|s| !(0.0..=MAX_SONG_SECS).contains(s))
            {
                return Err(format!(
                    "{}: lengths must be between 0 and {MAX_SONG_SECS} seconds",
                    song.key
                ));
            }
        }
        if let Some(r) = self.song_range {
            if r.first > r.last || (r.last - r.first) as usize >= MAX_SONGS {
                return Err(format!(
                    "song_range must go up and have at most {MAX_SONGS} songs"
                ));
            }
        }
        match &self.playback {
            Some(p) => {
                p.build()?;
                if let Some(probe) = &self.probe {
                    probe.build()?;
                }
                if self.song_ids().is_empty() {
                    return Err("a game with playback needs songs or a song_range".into());
                }
            }
            None if self.probe.is_some() || self.song_range.is_some() => {
                return Err("probe and song_range need playback".into());
            }
            None => {}
        }
        Ok(())
    }

    /// The song IDs of a game with its own playback: those of `song_range` (or of `songs`
    /// without one) in order, then the listed songs outside the range; hidden ones left out.
    pub fn song_ids(&self) -> Vec<u16> {
        let range = self.song_range.map(|r| r.first..=r.last);
        let mut ids: Vec<u16> = match &range {
            Some(r) => r.clone().take(MAX_SONGS + 1).collect(),
            None => Vec::new(),
        };
        for song in &self.songs {
            if !range.as_ref().is_some_and(|r| r.contains(&song.id)) && !ids.contains(&song.id) {
                ids.push(song.id);
            }
        }
        ids.retain(|&id| !self.song(id).is_some_and(|s| s.hidden));
        ids
    }

    pub fn song(&self, id: u16) -> Option<&SongDef> {
        self.songs.iter().find(|s| s.id == id)
    }
}

impl SongDef {
    pub fn length(&self) -> Option<SongLength> {
        // Clamped so that a mistyped value cannot overflow frame arithmetic later on.
        let frames = |s: f64| (s.clamp(0.0, MAX_SONG_SECS) * FRAME_RATE).round() as u32;
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

    /// The game of a ROM: the one that lists its header title (and checksum, if the game
    /// lists any), otherwise the one for which `score` (the number of the ROM's songs that
    /// the game's song list names) is highest; the first game on a tie.
    pub fn identify(
        &self,
        title: &str,
        checksum: u16,
        score: impl Fn(&Game) -> usize,
    ) -> Option<&Game> {
        let listed = self.games.iter().find(|g| g.matches(title, checksum));
        listed.or_else(|| {
            let mut best: Option<(&Game, usize)> = None;
            for g in &self.games {
                let s = score(g);
                if best.is_none_or(|(_, b)| s > b) {
                    best = Some((g, s));
                }
            }
            best.map(|(g, _)| g)
        })
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

/// The built-in series, as (name, songs).
pub fn builtin_series() -> impl Iterator<Item = (&'static str, &'static Series)> {
    BUILTIN.iter().map(|b| (b.series, series(b.series)))
}

/// Most series folders of the user folder that are read.
const MAX_USER_SERIES: usize = 64;
/// Most games one user `songs.toml` may describe.
const MAX_USER_GAMES: usize = 256;

/// The series in the user folder, `<user_dir>/<series>/songs.toml`, as (name, songs). They
/// describe games without a built-in detector; problems are returned as warnings and the
/// broken file or game is skipped.
pub fn user_series() -> (Vec<(String, Series)>, Vec<String>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let Some(entries) = user_dir().and_then(|d| std::fs::read_dir(d).ok()) else {
        return (found, warnings);
    };
    let mut names: Vec<_> = entries
        .take(4 * MAX_USER_SERIES)
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| is_plain_name(n) && !builtin().contains_key(n.as_str()))
        .collect();
    names.sort();
    names.truncate(MAX_USER_SERIES);
    for name in names {
        let Some(path) = user_dir().map(|d| d.join(&name).join("songs.toml")) else {
            continue;
        };
        let text = match read_user_file(&path) {
            Ok(Some(text)) => text,
            Ok(None) => continue,
            Err(e) => {
                warnings.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let mut series: Series = match toml::from_str(&text) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        if series.games.len() > MAX_USER_GAMES {
            warnings.push(format!(
                "{}: more than {MAX_USER_GAMES} games",
                path.display()
            ));
            continue;
        }
        series.games.retain(|g| match g.validate() {
            Ok(()) => true,
            Err(e) => {
                warnings.push(format!("{}: {}: {e}", path.display(), g.id));
                false
            }
        });
        found.push((name, series));
    }
    (found, warnings)
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

/// Whether `name` is safe to use as one component of a path: lowercase letters, digits and
/// hyphens only, like the series names and language codes.
pub fn is_plain_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Read a user file of at most [`MAX_USER_FILE`] bytes; `None` if it does not exist.
pub fn read_user_file(path: &Path) -> std::io::Result<Option<String>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut text = String::new();
    file.take(MAX_USER_FILE + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_USER_FILE {
        return Err(std::io::Error::other(format!(
            "larger than {} KiB",
            MAX_USER_FILE / 1024
        )));
    }
    Ok(Some(text))
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
        // Both names become part of a path, so anything but a plain code is not looked up.
        let path = (is_plain_name(series) && is_plain_name(language))
            .then(user_dir)
            .flatten()
            .map(|d| d.join(series).join(format!("{language}.toml")));
        if let Some(path) = path {
            match read_user_file(&path) {
                Ok(Some(text)) => match toml::from_str(&text) {
                    Ok(names) => layers.push(names),
                    Err(e) => warnings.push(format!("{}: {e}", path.display())),
                },
                Ok(None) => {}
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
    /// user's song database and name files are returned as warnings; those files are then
    /// skipped.
    pub fn new(driver: &Driver, language: &str) -> (Names, Vec<String>) {
        let (names, name_warnings) = Localizer::new(&driver.series, language);
        let mut warnings = driver.warnings.clone();
        warnings.extend(name_warnings);
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
                // Hidden songs are never listed, so they need no name.
                for song in game.songs.iter().filter(|s| !s.hidden) {
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
        assert_eq!(ja.song("pallet_town"), "マサラタウンのテーマ");
        let (fr, _) = Localizer::new(GEN1, "fr");
        assert_eq!(fr.song("pallet_town"), "Pallet Town Theme");
        assert_eq!(fr.song("no_such_song"), "no_such_song");
    }

    #[test]
    fn locale_tags() {
        assert_eq!(language_code("ja-JP"), "ja");
        assert_eq!(language_code("en_US.UTF-8"), "en");
    }

    #[test]
    fn only_plain_names_become_paths() {
        assert!(is_plain_name("ja"));
        assert!(is_plain_name(GEN1));
        for bad in [
            "",
            "../ja",
            "ja/..",
            "..",
            "a\\b",
            "JA",
            "ja.toml",
            &"a".repeat(33),
        ] {
            assert!(!is_plain_name(bad), "{bad}");
        }
        // A language code that would escape the user folder still gets the built-in names.
        let (names, warnings) = Localizer::new(GEN1, "../../../ja");
        assert!(warnings.is_empty());
        assert_eq!(names.song("pallet_town"), "Pallet Town Theme");
    }

    #[test]
    fn oversized_user_files_are_refused() {
        let dir = std::env::temp_dir().join(format!("gb-music-songdb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let small = dir.join("small.toml");
        let big = dir.join("big.toml");
        std::fs::write(&small, "[songs]\n").unwrap();
        std::fs::write(&big, vec![b'#'; MAX_USER_FILE as usize + 1]).unwrap();
        assert_eq!(
            read_user_file(&small).unwrap().as_deref(),
            Some("[songs]\n")
        );
        assert!(read_user_file(&big).is_err());
        assert!(read_user_file(&dir.join("missing.toml")).unwrap().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn builtin_games_are_valid() {
        for (name, series) in builtin_series() {
            for game in &series.games {
                game.validate()
                    .unwrap_or_else(|e| panic!("{name}: {}: {e}", game.id));
            }
        }
    }

    #[test]
    fn games_beyond_the_limits_are_refused() {
        let game = |extra: &str| {
            let text =
                format!("[[game]]\nid = \"g\"\nsongs = [{{ id = 1, key = \"one\" }}]\n{extra}");
            toml::from_str::<Series>(&text).unwrap().games.remove(0)
        };
        let playback =
            "[game.playback]\nstart = [{ call = 0x4000 }]\nupdate = [{ call = 0x4100 }]\n";
        assert!(game("").validate().is_ok());
        assert!(game(playback).validate().is_ok());
        assert!(game("song_range = { first = 1, last = 2 }")
            .validate()
            .is_err());
        let probe = "[game.probe]\nactive = { at = 1, step = 1 }\naddress = { at = 1, step = 1 }\n\
                     return = { at = 1, step = 1 }\nloop = { at = 1, step = 1 }\n\
                     flags = { at = 1, step = 1 }\ntimer = { at = 1, step = 1 }\n";
        assert!(game(probe).validate().is_err());
        assert!(game(&format!("{playback}{probe}")).validate().is_ok());
        let range = |last: u16| {
            game(&format!(
                "song_range = {{ first = 0, last = {last} }}\n{playback}"
            ))
            .validate()
        };
        assert!(range(MAX_SONGS as u16 - 1).is_ok());
        assert!(range(MAX_SONGS as u16).is_err());
        let mut long = game(playback);
        long.songs[0].loop_secs = Some(MAX_SONG_SECS + 1.0);
        assert!(long.validate().is_err());
        long.songs[0].loop_secs = Some(f64::NAN);
        assert!(long.validate().is_err());
    }

    #[test]
    fn games_are_identified_by_title_checksum_then_songs() {
        let series: Series = toml::from_str(
            r#"
            [[game]]
            id = "a"
            titles = ["GAME"]
            checksums = [0x1234]
            songs = [{ id = 1, key = "one" }]
            [[game]]
            id = "b"
            titles = ["GAME"]
            songs = [{ id = 1, key = "one" }, { id = 2, key = "two" }]
            "#,
        )
        .unwrap();
        let named = |g: &Game, ids: &[u16]| {
            ids.iter()
                .filter(|&&id| g.songs.iter().any(|s| s.id == id))
                .count()
        };
        let id = |title, checksum, ids: &[u16]| {
            series
                .identify(title, checksum, |g| named(g, ids))
                .map(|g| g.id.clone())
        };
        assert_eq!(id("GAME", 0x1234, &[]).as_deref(), Some("a"));
        assert_eq!(id("GAME", 0x9999, &[]).as_deref(), Some("b"));
        // Unknown title: the game that names most songs, the first one on a tie.
        assert_eq!(id("OTHER", 0, &[1, 2]).as_deref(), Some("b"));
        assert_eq!(id("OTHER", 0, &[1]).as_deref(), Some("a"));
        assert_eq!(id("OTHER", 0, &[]).as_deref(), Some("a"));
    }

    #[test]
    fn fixed_lengths_are_clamped() {
        let def = SongDef {
            bank: None,
            id: 1,
            key: "x".into(),
            hidden: false,
            intro: Some(f64::INFINITY),
            loop_secs: Some(1e30),
            end: None,
        };
        let max = (MAX_SONG_SECS * FRAME_RATE).round() as u32;
        assert_eq!(
            def.length(),
            Some(SongLength::Loops {
                intro: max,
                period: max
            })
        );
    }
}
