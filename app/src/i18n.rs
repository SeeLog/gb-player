//! UI text in the chosen language, from `app/locales/<lang>.toml`, with English as the
//! fallback. Song and game names come from the song database instead (`gb_music::songdb`).

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

use gb_music::songdb::{language_code, FALLBACK_LANGUAGE};

const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.toml")),
    ("ja", include_str!("../locales/ja.toml")),
];

type Table = HashMap<String, String>;

fn tables() -> &'static HashMap<&'static str, Table> {
    static TABLES: OnceLock<HashMap<&'static str, Table>> = OnceLock::new();
    TABLES.get_or_init(|| {
        LOCALES
            .iter()
            .map(|&(lang, text)| {
                let table =
                    toml::from_str(text).unwrap_or_else(|e| panic!("app/locales/{lang}.toml: {e}"));
                (lang, table)
            })
            .collect()
    })
}

/// The language of the OS user interface, as a language code.
pub fn system_language() -> String {
    sys_locale::get_locale()
        .map(|tag| language_code(&tag))
        .filter(|code| !code.is_empty())
        .unwrap_or_else(|| FALLBACK_LANGUAGE.to_string())
}

pub struct Text {
    language: String,
    tables: Vec<&'static Table>,
}

impl Text {
    pub fn new(language: &str) -> Text {
        let tables = [language, FALLBACK_LANGUAGE]
            .iter()
            .filter_map(|lang| tables().get(*lang))
            .collect();
        Text {
            language: language.to_string(),
            tables,
        }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// The text for `key`; the key itself if no language has it.
    pub fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.tables
            .iter()
            .find_map(|t| t.get(key))
            .map_or(key, String::as_str)
    }

    /// Every key with its text, for the web UI, which fills in the placeholders itself.
    pub fn table(&self) -> HashMap<&str, &str> {
        let mut all = HashMap::new();
        for t in self.tables.iter().rev() {
            all.extend(t.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        }
        all
    }

    /// The text for `key` with each `{name}` replaced by its value.
    pub fn fmt(&self, key: &str, args: &[(&str, &dyn Display)]) -> String {
        let mut s = self.get(key).to_string();
        for (name, value) in args {
            s = s.replace(&format!("{{{name}}}"), &value.to_string());
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_has_the_english_keys() {
        let en = &tables()["en"];
        for (lang, table) in tables() {
            for key in table.keys() {
                assert!(en.contains_key(key), "{lang}.toml: {key} is not in en.toml");
            }
        }
    }

    #[test]
    fn placeholders_and_fallback() {
        let ja = Text::new("ja");
        assert_eq!(ja.fmt("song_count", &[("n", &3)]), "3 曲");
        let fr = Text::new("fr");
        assert_eq!(fr.fmt("song_count", &[("n", &3)]), "3 songs");
        assert_eq!(fr.get("no_such_key"), "no_such_key");
        assert_eq!(ja.table()["search"], "検索");
        assert_eq!(fr.table()["search"], "Search");
    }
}
