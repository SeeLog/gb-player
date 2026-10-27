//! Playlists of songs from any number of ROMs, kept as json files in `gb-player/playlists`
//! next to the user song names. An entry remembers its ROM by path and by a hash of the file,
//! so a ROM that was moved can be pointed at again and checked to be the same one.

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

fn dir() -> Option<PathBuf> {
    Some(gb_music::songdb::user_dir()?.parent()?.join("playlists"))
}

fn file(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// The saved playlists in name order, and the problems found reading them.
pub fn load(t: &Text) -> (Vec<Playlist>, Vec<String>) {
    let mut lists = Vec::new();
    let mut warnings = Vec::new();
    let Some(entries) = dir().and_then(|d| std::fs::read_dir(d).ok()) else {
        return (lists, warnings);
    };
    for path in entries.flatten().map(|e| e.path()) {
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
    lists.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
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
}
