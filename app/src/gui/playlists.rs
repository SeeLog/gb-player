//! Playlists of songs from any number of ROMs, kept as json files in `gb-player/playlists`
//! next to the user song names. An entry remembers its ROM by path and by a hash of the file,
//! so a ROM that was moved can be pointed at again and checked to be the same one.
//!
//! Two of them are kept by the app itself, under fixed ids: the favourites and the history of
//! songs played. They come first, cannot be renamed or deleted, and are there even before
//! their files are.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::views::LengthView;
use crate::i18n::Text;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    /// [`hash`] of the ROM file.
    pub hash: String,
    /// The song number in the ROM.
    pub song: usize,
    /// Names when the song was added, shown while its ROM is not loaded.
    pub game: String,
    pub title: String,
    #[serde(default)]
    pub length: Option<LengthView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    /// The file stem; not stored in the file.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

/// An identifier of a ROM file's contents: FNV-1a over the bytes, with the method named so a
/// later one can be told apart.
pub fn hash(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}")
}

/// The id of the favourites.
pub const FAVORITES: &str = "favorites";
/// The id of the songs played lately, the latest first.
pub const HISTORY: &str = "history";
/// The choices of how many songs the history keeps (see `Settings::history_len`).
pub const HISTORY_LENS: [usize; 4] = [50, 100, 200, 500];

/// Whether `id` is one of the lists the app keeps itself.
pub fn is_auto(id: &str) -> bool {
    id == FAVORITES || id == HISTORY
}

/// The favourites, then the history, then the user's playlists by name.
pub fn sort(lists: &mut [Playlist]) {
    let rank = |l: &Playlist| match l.id.as_str() {
        FAVORITES => 0,
        HISTORY => 1,
        _ => 2,
    };
    lists.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Whether two entries are the same song: of the same ROM and number, or for a song of a
/// folder (whose numbers change as files come and go), of the same file.
pub fn same_song(a: &Entry, b: &Entry) -> bool {
    a.hash == b.hash
        && if a.hash.starts_with("folder:") {
            a.path == b.path
        } else {
            a.song == b.song
        }
}

/// Put `entry` at the top of the history `entries`, dropping its earlier play and the songs
/// beyond the `len` latest.
pub fn record(entries: &mut Vec<Entry>, entry: Entry, len: usize) {
    entries.retain(|e| !same_song(e, &entry));
    entries.insert(0, entry);
    entries.truncate(len);
}

pub fn dir() -> Option<PathBuf> {
    Some(gb_music::songdb::user_dir()?.parent()?.join("playlists"))
}

fn file(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// The saved playlists in [`sort`] order, the favourites and the history among them, and the
/// problems found reading them.
pub fn load(t: &Text) -> (Vec<Playlist>, Vec<String>) {
    load_from(dir().as_deref(), t)
}

fn load_from(dir: Option<&Path>, t: &Text) -> (Vec<Playlist>, Vec<String>) {
    let mut lists = Vec::new();
    let mut warnings = Vec::new();
    for path in dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
    {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let read = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Playlist>(&s).map_err(|e| e.to_string()));
        match read {
            Ok(mut list) => {
                list.id = id.to_string();
                lists.push(list);
            }
            Err(e) => warnings.push(t.fmt(
                "playlist_bad_file",
                &[("file", &path.display()), ("error", &e)],
            )),
        }
    }
    for id in [FAVORITES, HISTORY] {
        if lists.iter().all(|l| l.id != id) {
            lists.push(Playlist {
                id: id.to_string(),
                name: id.to_string(),
                entries: Vec::new(),
            });
        }
    }
    sort(&mut lists);
    (lists, warnings)
}

/// An id no playlist in `lists` has.
pub fn new_id(lists: &[Playlist]) -> String {
    let start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    (start..)
        .map(|n| format!("p{n}"))
        .find(|id| lists.iter().all(|l| &l.id != id))
        .expect("an unused id")
}

pub fn save(list: &Playlist, t: &Text) -> Result<(), String> {
    let dir = dir().ok_or_else(|| t.get("playlist_no_dir").to_string())?;
    let path = file(&dir, &list.id);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(list)?;
        std::fs::write(&path, json)
    };
    write().map_err(|e| t.fmt("err_write", &[("path", &path.display()), ("error", &e)]))
}

pub fn delete(id: &str) {
    if let Some(dir) = dir() {
        let _ = std::fs::remove_file(file(&dir, id));
    }
}

/// The position `delta` steps from `from` that plays, wrapping around and passing over the
/// positions `skip` rejects; the first playable one when `from` is `None`.
pub fn step(
    from: Option<usize>,
    delta: i64,
    len: usize,
    skip: impl Fn(usize) -> bool,
) -> Option<usize> {
    let n = len as i64;
    if n == 0 {
        return None;
    }
    let dir = if delta < 0 { -1 } else { 1 };
    let mut at = match from {
        Some(f) => (f as i64 + delta).rem_euclid(n),
        None => 0,
    };
    for _ in 0..n {
        if !skip(at as usize) {
            return Some(at as usize);
        }
        at = (at + dir).rem_euclid(n);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_names_its_method() {
        assert_eq!(hash(b""), "fnv1a64:cbf29ce484222325");
        assert_eq!(hash(b"a"), "fnv1a64:af63dc4c8601ec8c");
        assert_ne!(hash(b"ab"), hash(b"ba"));
    }

    #[test]
    fn step_wraps_and_passes_over_skipped_entries() {
        let none = |_| false;
        assert_eq!(step(Some(4), 1, 5, none), Some(0));
        assert_eq!(step(Some(0), -1, 5, none), Some(4));
        assert_eq!(step(None, 1, 5, none), Some(0));
        assert_eq!(step(None, 1, 0, none), None);
        let missing = |i| i == 1 || i == 2;
        assert_eq!(step(Some(0), 1, 4, missing), Some(3));
        assert_eq!(step(Some(3), -1, 4, missing), Some(0));
        assert_eq!(step(None, 1, 3, |i| i == 0), Some(1));
        assert_eq!(step(Some(0), 1, 3, |_| true), None);
    }

    #[test]
    fn a_playlist_round_trips_without_its_id() {
        let list = Playlist {
            id: "p1".into(),
            name: "Mix".into(),
            entries: vec![Entry {
                path: "a.gb".into(),
                hash: hash(b"rom"),
                song: 3,
                game: "Game".into(),
                title: "Song".into(),
                length: Some(LengthView::Ends { at: 12.5 }),
            }],
        };
        let back: Playlist = serde_json::from_str(&serde_json::to_string(&list).unwrap()).unwrap();
        assert_eq!(back.id, "");
        assert_eq!(back.entries, list.entries);
        let bare: Playlist = serde_json::from_str(r#"{"name":"Empty"}"#).unwrap();
        assert!(bare.entries.is_empty());
    }

    fn entry(hash: &str, path: &str, song: usize) -> Entry {
        Entry {
            path: path.into(),
            hash: hash.into(),
            song,
            game: "Game".into(),
            title: format!("Song {song}"),
            length: None,
        }
    }

    #[test]
    fn a_song_is_known_by_its_rom_and_number_or_by_its_file() {
        let rom = entry("fnv1a64:1", "a.gb", 3);
        assert!(same_song(&rom, &entry("fnv1a64:1", "moved/a.gb", 3)));
        assert!(!same_song(&rom, &entry("fnv1a64:1", "a.gb", 4)));
        assert!(!same_song(&rom, &entry("fnv1a64:2", "a.gb", 3)));
        // Songs of a folder move along as files are added.
        let file = entry("folder:1", "d/x.ogg", 0);
        assert!(same_song(&file, &entry("folder:1", "d/x.ogg", 2)));
        assert!(!same_song(&file, &entry("folder:1", "d/y.ogg", 0)));
    }

    #[test]
    fn the_history_keeps_the_latest_play_of_each_song_up_to_its_length() {
        let mut h = Vec::new();
        for song in [1, 2, 3, 2] {
            record(&mut h, entry("fnv1a64:1", "a.gb", song), 100);
        }
        let songs: Vec<usize> = h.iter().map(|e| e.song).collect();
        assert_eq!(songs, [2, 3, 1]);
        for song in 0..60 {
            record(&mut h, entry("fnv1a64:2", "b.gb", song), 50);
        }
        assert_eq!(h.len(), 50);
        assert_eq!(h[0].song, 59);
        assert_eq!(h[49].song, 10);
    }

    #[test]
    fn the_favourites_and_the_history_load_first_even_without_files() {
        let t = Text::new("en");
        let dir = std::env::temp_dir().join(format!("gb-player-lists-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |id: &str, list: &Playlist| {
            std::fs::write(file(&dir, id), serde_json::to_string(list).unwrap()).unwrap();
        };
        let list = |name: &str, entries: Vec<Entry>| Playlist {
            id: String::new(),
            name: name.into(),
            entries,
        };
        write("p1", &list("Alpha", vec![]));
        write(HISTORY, &list(HISTORY, vec![entry("fnv1a64:1", "a.gb", 5)]));

        let (lists, warnings) = load_from(Some(&dir), &t);
        let ids: Vec<&str> = lists.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, [FAVORITES, HISTORY, "p1"]);
        assert!(warnings.is_empty());
        assert!(lists[0].entries.is_empty());
        assert_eq!(lists[1].entries[0].song, 5);

        let (lists, _) = load_from(None, &t);
        let ids: Vec<&str> = lists.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, [FAVORITES, HISTORY]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
