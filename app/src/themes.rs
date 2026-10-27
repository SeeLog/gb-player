//! Colour themes: the built-in palettes of `ui/src/app.css` and custom themes kept as toml
//! files in `gb-player/themes` next to the user song names.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
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
pub const TOKENS: [&str; 24] = [
    "backdrop",
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
    "shell",
    "shell-ink",
    "shell-frame",
    "key",
    "key-ink",
];

/// The consoles the mini player can be drawn as: the first model and the colour one.
pub const CONSOLES: [&str; 2] = ["dmg", "gbc"];

/// The console a theme without one is drawn as: the colour model for the color palette.
fn default_console(base: &str) -> String {
    if base == "color" { "gbc" } else { "dmg" }.to_string()
}

/// Ids of custom themes start with this, so they never clash with the built-in ones.
const CUSTOM_PREFIX: &str = "custom:";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    /// The built-in palette the colours are laid over.
    pub base: String,
    pub colors: BTreeMap<String, String>,
    pub background: Option<Background>,
    /// The console the mini player is drawn as, one of [`CONSOLES`].
    pub console: String,
    pub custom: bool,
}

/// An image drawn behind the whole window, under the `bg` colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Background {
    /// A path inside the themes folder, such as `images/sky.png`.
    pub image: String,
    /// `cover`, `contain` or `tile`.
    pub fit: String,
    /// Blur radius in pixels, 0 to 40.
    pub blur: f32,
    /// How much to darken the image, 0 to 1.
    pub dim: f32,
}

impl Default for Background {
    fn default() -> Background {
        Background {
            image: String::new(),
            fit: "cover".into(),
            blur: 0.0,
            dim: 0.0,
        }
    }
}

/// Image types a background may have, by extension.
const IMAGE_TYPES: [(&str, &str); 5] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];
/// The largest background image that is loaded.
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ThemeFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<String>,
    colors: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<Background>,
    #[serde(skip_serializing_if = "Option::is_none")]
    console: Option<String>,
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

fn image_type(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    IMAGE_TYPES
        .iter()
        .find(|&&(e, _)| e == ext)
        .map(|&(_, mime)| mime)
}

/// Whether `name` is a relative path that stays inside the themes folder and names an image.
pub fn is_image_name(name: &str) -> bool {
    use std::path::Component;
    !name.is_empty()
        && !name.contains(':')
        && !name.starts_with(['/', '\\'])
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && name.split(['/', '\\']).all(|part| part != "..")
        && image_type(name).is_some()
}

/// Check a background and clamp its numbers; `None` (with a warning) when it cannot be used.
fn clean_background(
    bg: Background,
    file: &str,
    t: &Text,
    warnings: &mut Vec<String>,
) -> Option<Background> {
    if !is_image_name(&bg.image) {
        warnings.push(t.fmt("theme_bad_image", &[("file", &file), ("image", &bg.image)]));
        return None;
    }
    let fit = if matches!(bg.fit.as_str(), "cover" | "contain" | "tile") {
        bg.fit
    } else {
        "cover".into()
    };
    Some(Background {
        image: bg.image.replace('\\', "/"),
        fit,
        blur: if bg.blur.is_finite() {
            bg.blur.clamp(0.0, 40.0)
        } else {
            0.0
        },
        dim: if bg.dim.is_finite() {
            bg.dim.clamp(0.0, 1.0)
        } else {
            0.0
        },
    })
}

/// A background image of the themes folder as a `data:` URL for the page.
pub fn image_url(name: &str, t: &Text) -> Result<String, String> {
    let bad = || t.fmt("theme_bad_image", &[("file", &name), ("image", &name)]);
    if !is_image_name(name) {
        return Err(bad());
    }
    let mime = image_type(name).ok_or_else(bad)?;
    let path = dir().ok_or_else(bad)?.join(name);
    let read =
        |e: &dyn std::fmt::Display| t.fmt("err_read", &[("path", &path.display()), ("error", &e)]);
    let size = std::fs::metadata(&path).map_err(|e| read(&e))?.len();
    if size > MAX_IMAGE_BYTES {
        return Err(t.fmt("theme_image_too_big", &[("path", &path.display())]));
    }
    let bytes = std::fs::read(&path).map_err(|e| read(&e))?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Copy a picked image into `images/` of the themes folder; returns the name to use in a theme.
pub fn import_image(src: &Path, t: &Text) -> Result<String, String> {
    let dir = dir().ok_or_else(|| t.get("theme_no_dir").to_string())?;
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| IMAGE_TYPES.iter().any(|&(x, _)| x == e))
        .ok_or_else(|| {
            t.fmt(
                "theme_bad_image",
                &[("file", &src.display()), ("image", &src.display())],
            )
        })?;
    let size = std::fs::metadata(src)
        .map_err(|e| t.fmt("err_read", &[("path", &src.display()), ("error", &e)]))?
        .len();
    if size > MAX_IMAGE_BYTES {
        return Err(t.fmt("theme_image_too_big", &[("path", &src.display())]));
    }
    let stem = file_stem(&src.file_stem().unwrap_or_default().to_string_lossy());
    let images = dir.join("images");
    let mut name = format!("{stem}.{ext}");
    let mut n = 2;
    while images.join(&name).exists() {
        name = format!("{stem}-{n}.{ext}");
        n += 1;
    }
    let dest = images.join(&name);
    std::fs::create_dir_all(&images)
        .and_then(|_| std::fs::copy(src, &dest))
        .map_err(|e| t.fmt("err_write", &[("path", &dest.display()), ("error", &e)]))?;
    Ok(format!("images/{name}"))
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
    let console = match parsed.console {
        Some(c) if CONSOLES.contains(&c.as_str()) => c,
        Some(c) => {
            warnings.push(t.fmt("theme_bad_console", &[("file", &file), ("console", &c)]));
            default_console(&base)
        }
        None => default_console(&base),
    };
    Some(Theme {
        id: format!("{CUSTOM_PREFIX}{stem}"),
        name: parsed.name.filter(|n| !n.trim().is_empty()).unwrap_or(stem),
        base,
        colors: clean_colors(parsed.colors, &file, t, warnings),
        background: parsed
            .background
            .and_then(|b| clean_background(b, &file, t, warnings)),
        console,
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
            background: None,
            console: default_console(id),
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
    background: Option<Background>,
    console: &str,
    t: &Text,
) -> Result<String, String> {
    let dir = dir().ok_or_else(|| t.get("theme_no_dir").to_string())?;
    let stem = file_stem(name);
    let mut warnings = Vec::new();
    let file = ThemeFile {
        name: Some(name.trim().to_string()).filter(|n| !n.is_empty()),
        base: Some(if is_builtin(base) { base } else { BUILTIN[0].0 }.to_string()),
        colors: clean_colors(colors, &stem, t, &mut warnings),
        background: background.and_then(|b| clean_background(b, &stem, t, &mut warnings)),
        console: CONSOLES.contains(&console).then(|| console.to_string()),
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
            "#abc8",
            "rgb(1, 2, 3)",
            "rgba(15, 56, 15, 0.18)",
            "rgb(15 56 15 / 40%)",
            "hsl(120deg 50% 40% / 0.5)",
            "hsla(120, 50%, 40%, 0.5)",
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
            "base = \"nope\"\n[colors]\nbackdrop = \"#123\"\nbg = \"#000\"\nfoo = \"#fff\"\ntext = \"red\"\n",
            &t,
            &mut warnings,
        )
        .unwrap();
        assert_eq!(theme.id, "custom:night");
        assert_eq!(theme.name, "night");
        assert_eq!(theme.base, "dmg");
        assert_eq!(theme.colors.len(), 2);
        assert_eq!(theme.colors["backdrop"], "#123");
        assert_eq!(theme.colors["bg"], "#000");
        assert_eq!(theme.console, "dmg");
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn console_comes_from_the_file_or_the_base() {
        let t = Text::new("en");
        let mut warnings = Vec::new();
        let path = Path::new("x.toml");
        let console = |text: &str, warnings: &mut Vec<String>| {
            parse(path, text, &t, warnings).unwrap().console
        };
        assert_eq!(console("base = \"color\"\n", &mut warnings), "gbc");
        assert_eq!(console("base = \"pocket\"\n", &mut warnings), "dmg");
        assert_eq!(
            console("base = \"pocket\"\nconsole = \"gbc\"\n", &mut warnings),
            "gbc"
        );
        assert_eq!(
            console("base = \"color\"\nconsole = \"dmg\"\n", &mut warnings),
            "dmg"
        );
        assert!(warnings.is_empty());
        assert_eq!(
            console("base = \"color\"\nconsole = \"nope\"\n", &mut warnings),
            "gbc"
        );
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn image_names() {
        for ok in ["sky.png", "images/sky.JPG", "a/b/c.webp", "images\\x.gif"] {
            assert!(is_image_name(ok), "{ok}");
        }
        for bad in [
            "",
            "sky.svg",
            "sky",
            "../sky.png",
            "images/../../sky.png",
            "images\\..\\sky.png",
            "/etc/sky.png",
            "\\\\server\\share\\sky.png",
            "C:\\sky.png",
            "C:sky.png",
            "./sky.png",
        ] {
            assert!(!is_image_name(bad), "{bad}");
        }
    }

    #[test]
    fn background_is_checked_and_clamped() {
        let t = Text::new("en");
        let mut warnings = Vec::new();
        let theme = parse(
            Path::new("sky.toml"),
            "[background]\nimage = \"images\\\\sky.png\"\nfit = \"stretch\"\nblur = 99\ndim = -1\n",
            &t,
            &mut warnings,
        )
        .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let bg = theme.background.unwrap();
        assert_eq!(bg.image, "images/sky.png");
        assert_eq!(bg.fit, "cover");
        assert_eq!(bg.blur, 40.0);
        assert_eq!(bg.dim, 0.0);

        let theme = parse(
            Path::new("bad.toml"),
            "[background]\nimage = \"../secret.png\"\n",
            &t,
            &mut warnings,
        )
        .unwrap();
        assert_eq!(theme.background, None);
        assert_eq!(warnings.len(), 1);
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
