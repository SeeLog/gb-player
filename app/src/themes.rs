//! Colour themes: the built-in palettes of `ui/src/app.css` and custom themes kept as toml
//! files in `gb-player/themes` next to the user song names.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Text;

/// The built-in palettes as (id, name); the first is the default.
pub const BUILTIN: [(&str, &str); 4] = [
    ("dmg", "DMG"),
    ("pocket", "Pocket"),
    ("color", "Color"),
    ("light", "Light"),
];

/// The colour tokens a theme may set, the CSS custom properties without the leading `--`.
pub const TOKENS: [&str; 18] = [
    "bg",
    "panel",
    "panel-2",
    "line",
    "text",
    "muted",
    "faint",
    "accent",
    "accent-2",
    "accent-ink",
    "lcd-hi",
    "lcd",
    "lcd-lo",
    "lcd-ink",
    "lcd-grid",
    "bezel",
    "overlay",
    "danger",
];

/// Ids of custom themes start with this, so they never clash with the built-in ones.
const CUSTOM_PREFIX: &str = "custom:";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    /// The built-in palette the colours are laid over.
    pub base: String,
    pub colors: BTreeMap<String, String>,
    pub custom: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ThemeFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<String>,
    colors: BTreeMap<String, String>,
}

pub fn dir() -> Option<PathBuf> {
    Some(gb_music::songdb::user_dir()?.parent()?.join("themes"))
}

fn is_builtin(id: &str) -> bool {
    BUILTIN.iter().any(|&(b, _)| b == id)
}

/// Whether `value` is a plain colour (`#rgb`, `#rrggbb`, ... or an `rgb()`/`hsl()` call with
/// numbers), so a theme file cannot put arbitrary CSS into the page.
pub fn is_color(value: &str) -> bool {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let Some((func, rest)) = v.split_once('(') else {
        return false;
    };
    let Some(args) = rest.strip_suffix(')') else {
        return false;
    };
    matches!(
        func.to_ascii_lowercase().as_str(),
        "rgb" | "rgba" | "hsl" | "hsla"
    ) && args.chars().any(|c| c.is_ascii_digit())
        && args
            .chars()
            .all(|c| c.is_ascii_digit() || " .,%/-".contains(c) || "deg".contains(c))
}

/// Keep the known tokens with valid colours; report the rest.
fn clean_colors(
    colors: BTreeMap<String, String>,
    file: &str,
    t: &Text,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (token, value) in colors {
        if !TOKENS.contains(&token.as_str()) {
            warnings.push(t.fmt("theme_bad_token", &[("file", &file), ("token", &token)]));
        } else if !is_color(&value) {
            warnings.push(t.fmt(
                "theme_bad_color",
                &[("file", &file), ("token", &token), ("value", &value)],
            ));
        } else {
            out.insert(token, value.trim().to_string());
        }
    }
    out
}

fn parse(path: &Path, text: &str, t: &Text, warnings: &mut Vec<String>) -> Option<Theme> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parsed: ThemeFile = match toml::from_str(text) {
        Ok(f) => f,
        Err(e) => {
            warnings.push(t.fmt("theme_bad_file", &[("file", &file), ("error", &e)]));
            return None;
        }
    };
    let base = match parsed.base {
        Some(b) if is_builtin(&b) => b,
        Some(b) => {
            warnings.push(t.fmt("theme_bad_base", &[("file", &file), ("base", &b)]));
            BUILTIN[0].0.to_string()
        }
        None => BUILTIN[0].0.to_string(),
    };
    Some(Theme {
        id: format!("{CUSTOM_PREFIX}{stem}"),
        name: parsed.name.filter(|n| !n.trim().is_empty()).unwrap_or(stem),
        base,
        colors: clean_colors(parsed.colors, &file, t, warnings),
        custom: true,
    })
}

/// The built-in themes followed by the custom ones, and warnings about bad theme files.
pub fn load(t: &Text) -> (Vec<Theme>, Vec<String>) {
    let mut themes: Vec<Theme> = BUILTIN
        .iter()
        .map(|&(id, name)| Theme {
            id: id.to_string(),
            name: name.to_string(),
            base: id.to_string(),
            colors: BTreeMap::new(),
            custom: false,
        })
        .collect();
    let mut warnings = Vec::new();
    let mut paths: Vec<PathBuf> = dir()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        })
        .collect();
    paths.sort();
    for path in paths {
        match std::fs::read_to_string(&path) {
            Ok(text) => themes.extend(parse(&path, &text, t, &mut warnings)),
            Err(e) => warnings.push(t.fmt(
                "theme_bad_file",
                &[("file", &path.display()), ("error", &e)],
            )),
        }
    }
    (themes, warnings)
}

/// A file stem made from a theme name, safe on every file system.
pub fn file_stem(name: &str) -> String {
    let mut stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    stem = stem.trim().trim_end_matches('.').to_string();
    if stem.is_empty() {
        stem = "theme".into();
    }
    // Windows reserves these names whatever the extension.
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit());
    if reserved {
        stem.insert(0, '_');
    }
    stem
}

/// Write a custom theme, replacing one with the same file name; returns its id.
pub fn save(
    name: &str,
    base: &str,
    colors: BTreeMap<String, String>,
    t: &Text,
) -> Result<String, String> {
    let dir = dir().ok_or_else(|| t.get("theme_no_dir").to_string())?;
    let stem = file_stem(name);
    let mut warnings = Vec::new();
    let file = ThemeFile {
        name: Some(name.trim().to_string()).filter(|n| !n.is_empty()),
        base: Some(if is_builtin(base) { base } else { BUILTIN[0].0 }.to_string()),
        colors: clean_colors(colors, &stem, t, &mut warnings),
    };
    if !warnings.is_empty() {
        return Err(warnings.join("\n"));
    }
    let text = toml::to_string(&file).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{stem}.toml"));
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, text))
        .map_err(|e| t.fmt("err_write", &[("path", &path.display()), ("error", &e)]))?;
    Ok(format!("{CUSTOM_PREFIX}{stem}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        for ok in [
            "#abc",
            "#A0B1C2",
            "#0f380fcc",
            "rgb(1, 2, 3)",
            "rgba(15, 56, 15, 0.18)",
            "hsl(120deg 50% 40% / 0.5)",
        ] {
            assert!(is_color(ok), "{ok}");
        }
        for bad in [
            "",
            "red",
            "#12",
            "#ggg",
            "rgb()",
            "url(x)",
            "rgb(1,2,3); background: url(x)",
            "rgb(1,2,3)) x",
            "var(--bg)",
            "expression(alert(1))",
        ] {
            assert!(!is_color(bad), "{bad}");
        }
    }

    #[test]
    fn parse_checks_tokens_and_base() {
        let t = Text::new("en");
        let mut warnings = Vec::new();
        let theme = parse(
            Path::new("night.toml"),
            "base = \"nope\"\n[colors]\nbg = \"#000\"\nfoo = \"#fff\"\ntext = \"red\"\n",
            &t,
            &mut warnings,
        )
        .unwrap();
        assert_eq!(theme.id, "custom:night");
        assert_eq!(theme.name, "night");
        assert_eq!(theme.base, "dmg");
        assert_eq!(theme.colors.len(), 1);
        assert_eq!(theme.colors["bg"], "#000");
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn file_stems() {
        assert_eq!(file_stem("My theme"), "My theme");
        assert_eq!(file_stem("夜/空:1"), "夜_空_1");
        assert_eq!(file_stem("../x."), "___x_");
        assert_eq!(file_stem("  "), "theme");
        assert_eq!(file_stem("con"), "_con");
        assert_eq!(file_stem("COM1"), "_COM1");
    }
}
