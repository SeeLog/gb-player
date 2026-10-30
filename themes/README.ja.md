[English](README.md) | 日本語

# テーマ

テーマは設定で選びます。「テーマを編集…」を押すとメインウィンドウの横にエディタが開き、色の変更がその場で反映されます。「名前を付けて保存…」で[設定フォルダ](../README.ja.md#使い方)の `themes` に保存します。ファイルを手で書いても構いません。

| 内蔵テーマ | 配色 | ミニプレイヤー |
| --- | --- | --- |
| DMG | 初代の4階調の緑 | 初代型、グレー |
| Pocket | グレー | 初代型、シルバー |
| Color | 紺と黄 | カラー型、黄色 |
| Prism | 明るいグレーにパールのような光沢 | カラー型、クリア |
| Nebula | 深い紫に夕焼けのような光沢 | カラー型、クリアパープル |
| Daylight | 明るい背景に初代の緑 | 初代型、ホワイト |

このフォルダの [Mew](Mew.toml) と [Shiny Mew](ShinyMew.toml) は自作テーマの例です。`themes` にコピーすると使えます。

## ファイルの形式

```toml
name = "夜のマサラタウン"   # 省略するとファイル名
base = "color"              # dmg / pocket / color / prism / nebula / daylight
console = "gbc"             # ミニプレイヤーの本体。dmg か gbc。省略すると base に合わせる

[colors]                    # 変えたい色だけ書く
bg = "#0b0d1e"
accent = "#ff8fb1"
lcd-grid = "rgba(30, 42, 63, 0.15)"
panel = "#15183399"         # 半透明にすると背景画像が透ける

[background]                # 省略可
image = "images/sky.png"    # themes フォルダ内の png / jpg / webp / gif、20 MBまで
fit = "cover"               # cover / contain / tile
blur = 6                    # 0〜40 px
dim = 0.2                   # 0〜1

[sheen]                     # 省略すると base のまま。colors = [] で光沢なし
angle = 115                 # 角度(度)
colors = ["#bdf3e4", "#d2c8fb", "#f8cbe4"]   # 8色まで
```

色は `#rgb`、`#rrggbb`、`#rrggbbaa` か、数値だけの `rgb()`、`rgba()`、`hsl()`、`hsla()` で書きます。知らない名前や色でない値は無視され、画面上部に警告が出ます。

## 色の名前

| 名前 | 場所 |
| --- | --- |
| `backdrop` | いちばん奥。背景画像があると画像に替わる |
| `bg` | ウィンドウの背景。`backdrop` の上に重なる |
| `panel`、`panel-2` | パネル。`bg` の上に重なる |
| `line` | 線 |
| `overlay` | シークバーのイントロ部分 |
| `danger` | エラー、警告、取り消せない操作のボタン |
| `text`、`muted`、`faint` | 文字 |
| `accent` | 差し色の文字。再生ボタンなど差し色で塗る部分には光沢もかかる |
| `accent-2` | 縁、メーター、リストのハイライト |
| `accent-ink` | 差し色の上の文字 |
| `meter-start`、`meter-end` | レベルメーターの左端と右端。省略すると `accent-2` と `accent` |
| `lcd-hi`、`lcd`、`lcd-lo`、`lcd-ink`、`lcd-grid` | 液晶 |
| `lcd-shadow`、`bezel` | 液晶の内側の縁の影、液晶の外枠 |
| `knob` | スライダーとスイッチのつまみ |
| `shell`、`shell-ink`、`shell-frame` | ミニプレイヤーの本体、本体の文字、画面の枠 |
| `key`、`key-ink` | A・Bボタン、ボタンの文字 |
| `rubber`、`board` | キーの下のラバー、中の基板 |

内蔵テーマの `bg` と `panel`、`panel-2` は最初から半透明で、`backdrop` に重ねると不透明に見える色にしてあります。画像を設定するだけで透けて見えます。`bg` を不透明に近くすると画像はほとんど見えなくなり、エディタに注意が出ます。エディタで選んだ画像は `themes\images\` にコピーされます。

`shell` を半透明にすると、中の基板とラバーと、縁から後ろの画面が透けて見えます。
