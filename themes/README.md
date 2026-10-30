English | [日本語](README.ja.md)

# Themes

Pick a theme in Settings. **Edit theme…** opens an editor beside the main window and shows each change live. **Save as…** writes the theme to the `themes` folder in the [settings folder](../README.md#usage), and you can also write those files by hand.

| Built-in | Colours | Mini player |
| --- | --- | --- |
| DMG | The original four shades of green | Original, grey |
| Pocket | Grey | Original, silver |
| Color | Navy and yellow | Color, yellow |
| Prism | Light grey with a pearly sheen | Color, clear |
| Nebula | Deep purple with a sunset sheen | Color, clear purple |
| Daylight | Light background with the original green | Original, white |

This folder has two examples to copy over: [Mew](Mew.toml) and [Shiny Mew](ShinyMew.toml).

## File format

```toml
name = "Pallet Town at night"   # defaults to the file name
base = "color"                  # dmg, pocket, color, prism, nebula or daylight
console = "gbc"                 # mini player body, dmg or gbc; defaults to the base's

[colors]                        # only the colours you change
bg = "#0b0d1e"
accent = "#ff8fb1"
lcd-grid = "rgba(30, 42, 63, 0.15)"
panel = "#15183399"             # alpha below 1 lets the background image through

[background]                    # optional
image = "images/sky.png"        # inside the themes folder; png, jpg, webp or gif, up to 20 MB
fit = "cover"                   # cover, contain or tile
blur = 6                        # 0 to 40 px
dim = 0.2                       # 0 to 1

[sheen]                         # defaults to the base's; colors = [] turns it off
angle = 115                     # degrees
colors = ["#bdf3e4", "#d2c8fb", "#f8cbe4"]   # up to 8
```

A colour is `#rgb`, `#rrggbb`, `#rrggbbaa`, or `rgb()`, `rgba()`, `hsl()`, `hsla()` with plain numbers. Unknown names and invalid values are ignored, with a warning at the top of the window.

## Colours

| Name | Used for |
| --- | --- |
| `backdrop` | Furthest layer; the background image replaces it |
| `bg` | Window background, above the backdrop |
| `panel`, `panel-2` | Panels, above `bg` |
| `line` | Borders |
| `overlay` | Intro section of the seek bar |
| `danger` | Errors, warnings and destructive buttons |
| `text`, `muted`, `faint` | Text |
| `accent` | Accent text; the sheen also paints accent-coloured parts such as the play button |
| `accent-2` | Borders, meters and list highlights |
| `accent-ink` | Text on the accent colour |
| `meter-start`, `meter-end` | Left and right ends of the level meters; default to `accent-2` and `accent` |
| `lcd-hi`, `lcd`, `lcd-lo`, `lcd-ink`, `lcd-grid` | LCD |
| `lcd-shadow`, `bezel` | Shadow inside the LCD's edge, frame around it |
| `knob` | Slider and switch knobs |
| `shell`, `shell-ink`, `shell-frame` | Mini player body, its text, the screen's frame |
| `key`, `key-ink` | A and B buttons, their text |
| `rubber`, `board` | Rubber under the keys, circuit board inside |

The built-in `bg`, `panel` and `panel-2` are already translucent, mixed to look solid over `backdrop`, so setting an image is enough to make it show. An almost opaque `bg` hides the image, and the editor warns you. Images chosen in the editor are copied to `themes\images\`.

A translucent `shell` shows the board, the rubber, and the screen behind the mini player's edges.
