# gb-player

Game Boy / Game Boy Color のROM(.gb / .gbc)を読み込み、ゲーム自身のサウンドエンジンを
エミュレータ上で動かしてBGMをリアルタイム再生するRust製GUIプレイヤーです。

- SM83 CPU・MBC1/2/3/5・APU(4チャンネル)を実装した最小限のエミュレーションコア
- ゲームの「曲を再生する関数」と「毎フレーム呼ばれる更新関数」をROM内から自動検出して呼び出す(GBS方式)
- ループ点を自動解析し、無限ループ再生/N回ループ後フェードアウトして次の曲へ、を選択可能
- チャンネルごとのミュートとレベルメーター、出力波形のオシロスコープ表示
- 曲名とUIは日本語・英語に対応(OSの言語に合わせて自動選択、切り替え可)

## 対応ソフト

| ソフト | エンジン |
| --- | --- |
| ポケットモンスター 赤・緑・青 / Pokémon Red・Blue | 第1世代 |
| ポケットモンスター ピカチュウ / Pokémon Yellow | 第1世代(Yellow) |
| ポケットモンスター 金・銀 / Pokémon Gold・Silver | 第2世代 |
| ポケットモンスター クリスタル / Pokémon Crystal | 第2世代 |

関数のアドレスは埋め込まず、機械語のパターンで探しているため、言語やバージョン違いでも
同じサウンドエンジンなら動く設計です。ROMイメージはリポジトリに含めません。各自で吸い出したものを使ってください。

## 使い方

GUIは [Tauri](https://tauri.app/) 製で、画面はWeb技術(React + TypeScript + Tailwind CSS、`ui/`)で書いています。
GUIのビルドには Rust に加えて [bun](https://bun.sh/) が必要です。

```sh
bun install                              # 初回のみ
bun run tauri dev                        # 開発用に起動(画面の変更はその場で反映)
bun run tauri dev -- -- pokemon.gb       # ROMを指定して開発用に起動
bun run tauri build --no-bundle          # target/release/gb-player(.exe) をビルド
bun run tauri build                      # Windowsならインストーラ(NSIS)も作る
```

ビルドしたexeの使い方:

```sh
gb-player                                # GUIを起動(ROMは「ROMを開く…」かドラッグ&ドロップ)
gb-player pokemon.gb                     # ROMを指定して起動
gb-player --info pokemon.gb              # 検出結果と全曲のループ長を表示
gb-player --wav pokemon.gb 1 out.wav 2   # 1曲目を2ループ+フェードでWAV出力
gb-player --lang en pokemon.gb           # 言語を指定して起動(既定は前回選んだ言語かOSの言語。GUIの右上でも切り替え可)
```

`--info` / `--wav` は bun なしの `cargo run --release -- --info pokemon.gb` でも動きます。

GUIのキー操作: Space 再生/一時停止、← → 前/次の曲、Ctrl+O ROMを開く、Ctrl+F 検索。
言語・音量・ループ・テーマの設定は `%APPDATA%\gb-player\settings.toml`(Linux/macOSは `~/.config/gb-player/settings.toml`)に保存されます。
ウィンドウの位置と大きさも次回の起動に引き継がれます。

## テーマ

画面右上のパレットのメニューで配色を切り替えられます。内蔵は DMG(初代の4階調の緑)/ Pocket(グレー)/
Color(紺と黄)/ Light(明るい背景)の4つです。

隣の鉛筆ボタンでテーマエディタが開き、色を変えるとその場で画面に反映されます。「名前を付けて保存…」で
`%APPDATA%\gb-player\themes\<名前>.toml`(Linux/macOSは `~/.config/gb-player/themes/`)に保存され、メニューに追加されます。
このフォルダのファイルは手で書いても構いません(起動時と保存時に読み込みます)。

```toml
name = "夜のマサラタウン"   # 省略するとファイル名
base = "color"              # 元にする内蔵テーマ: dmg / pocket / color / light
[colors]                    # 変えたい色だけ書く(書かなかった色は base のまま)
bg = "#0b0d1e"
accent = "#ff8fb1"
lcd-grid = "rgba(30, 42, 63, 0.15)"
panel = "#15183399"         # 末尾2桁(または rgba() の最後)が不透明度。半透明にすると背景画像が透ける

[background]                # 背景画像(省略可)
image = "images/sky.png"    # themes フォルダ内のファイル(png / jpg / webp / gif、20 MBまで)
fit = "cover"               # cover(全体を覆う)/ contain(全体を表示)/ tile(並べる)
blur = 6                    # ぼかし(0〜40 px)
dim = 0.2                   # 暗さ(0〜1)
```

重なり方は奥から `backdrop`(背景画像があるときは画像)→ `bg` → パネル類です。内蔵テーマの `bg` と
`panel` `panel-2` は最初から半透明で、`backdrop` の上に重なると不透明な色と同じ見た目になるように
作ってあるので、画像を設定するだけで透けて見えます。`bg` を不透明に近くすると画像はほとんど見えなくなります
(エディタに注意が出ます)。エディタの「画像を選ぶ…」で選んだ画像は `themes\images\` にコピーされます。

色の名前は `backdrop` `bg` `panel` `panel-2` `line` `text` `muted` `faint` `accent` `accent-2` `accent-ink`
`lcd-hi` `lcd` `lcd-lo` `lcd-ink` `lcd-grid` `bezel` `overlay` `danger` です。値は `#rgb` / `#rrggbb` /
`#rrggbbaa` か、数値だけの `rgb()` `rgba()` `hsl()` `hsla()` に限ります。知らない名前や色でない値は無視され、
画面上部に警告が出ます。

## 曲名

曲名はROMには入っていないので、`songdb/` の曲データベースから引いています。日本語の曲名は場所や場面を表す
説明的な名前で、公式サウンドトラックの曲名ではありません。英語名は pret の逆アセンブルのラベルが元です。

曲名の修正、他の言語の追加、新しいソフトの曲リストはPR歓迎です。書き方は [songdb/README.md](songdb/README.md) を参照してください。
手元だけで名前を変えたいときは、同じ形式のファイルを `%APPDATA%\gb-player\songdb\<シリーズ>\<言語>.toml` に置くと内蔵の名前より優先されます。

ROMは `rom/` に置くのがおすすめです(`.gitignore` で除外済み、`*.gb` / `*.gbc` もどこに置いてもコミットされません)。

## ビルド環境

### Windows

MSVC版のRustツールチェーンを使ってください。GNU版(`x86_64-pc-windows-gnu`)だとMinGWの
`dlltool.exe` が必要になり、無い環境では `error calling dlltool 'dlltool.exe': program not found` で失敗します。

```powershell
# C++ビルドツール(リンカ)を入れる
winget install -e --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
# MSVC版ツールチェーンを既定にする
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
```

`rustup show` の "Default host" が `x86_64-pc-windows-msvc` になっていればOKです。

bun は `winget install Oven-sh.Bun`(または `powershell -c "irm bun.sh/install.ps1 | iex"`)で入れられます。
入れた後はPATHを反映させるためにターミナルを開き直してください。WebView2 は Windows 10/11 に標準で入っています。

### Linux

ALSA と WebKitGTK の開発パッケージが必要です(Debian/Ubuntuなら
`libasound2-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev`)。

### macOS

追加のパッケージは不要です。

## 構成

```
crates/gb-core   エミュレーションコア(CPU / メモリマップ / MBC / APU)
crates/gb-music  サウンドドライバ検出、曲リスト、再生とループ解析
app              GUIのRust側(Tauri)と音声出力(cpal)、CLI
ui               GUIの画面(React + TypeScript + Tailwind CSS、bun/Viteでビルド)
songdb           曲データベース(ゲームの照合、曲ID → 曲キー、言語ごとの曲名)
tools            曲データベースの生成スクリプト
```

設計の詳細は [docs/DESIGN.md](docs/DESIGN.md) を参照してください。

## テスト

```sh
cargo test --workspace
```

CPUは [SingleStepTests/sm83](https://github.com/SingleStepTests/sm83) のテストベクタでも検証できます
(`v1/*.json` を置いたディレクトリを `SM83_TESTS_DIR` に指定)。
