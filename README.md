# gb-player

Game Boy / Game Boy Color のROM(.gb / .gbc)を読み込み、ゲーム自身のサウンドエンジンを
エミュレータ上で動かしてBGMをリアルタイム再生するRust製GUIプレイヤーです。

- SM83 CPU・MBC1/2/3/5・APU(4チャンネル)を実装した最小限のエミュレーションコア
- ゲームの「曲を再生する関数」と「毎フレーム呼ばれる更新関数」をROM内から自動検出して呼び出す(GBS方式)
- ループ点を自動解析し、無限ループ再生/N回ループ後フェードアウトして次の曲へ、を選択可能
- チャンネルごとのミュートとレベルメーター、出力波形のオシロスコープ表示
- 出力は「ライン」(音源の出力そのまま)と「GBCスピーカー」から選べる。GBCスピーカーは実機のゲームボーイカラーで測った
  スピーカーの特性と、液晶などから漏れる本体のノイズ(オフにもできる)を再現したモノラルの音
  (測り方と結果は [docs/GBC_SPEAKER.md](docs/GBC_SPEAKER.md))
- 一般の音声ファイル(WAV・FLAC・MP3・Ogg Vorbis・M4A(AAC/ALAC)・AIFF)も、フォルダ単位で開いて
  ROMと同じように再生できる。GBCスピーカーの音もかけられる(ループ点は持たないので、無限ループでは曲全体を繰り返す)
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

このほかのゲームは、サウンドドライバの呼び方を `songs.toml` に書けば鳴らせます(書き方は [songdb/README.md](songdb/README.md))。
また、手元にあるGBSファイル(`.gbs`)も開けます。GBSファイルもゲームのデータなので、ROMと同じく各自で用意したものを使ってください。

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
gb-player                                # GUIを起動(ROMや音声ファイルは「開く…」かドラッグ&ドロップ)
gb-player pokemon.gb                     # ROMを指定して起動(前回開いていたROMに加えて開く)
gb-player --info pokemon.gb              # 検出結果と全曲のループ長を表示
gb-player --wav pokemon.gb 1 out.wav 2   # 1曲目を2ループ+フェードでWAV出力
gb-player --wav pokemon.gb 1 out.wav 2 gbc-speaker   # 同じく、GBCのスピーカーの音で
gb-player --wav song.mp3 1 out.wav 1 gbc-speaker     # 音声ファイルをGBCのスピーカーの音に
gb-player --lang en pokemon.gb           # 言語を指定して起動(既定は前回選んだ言語かOSの言語。GUIの右上でも切り替え可)
```

`--info` / `--wav` は bun なしの `cargo run --release -- --info pokemon.gb` でも動きます。

GUIのキー操作: Space 再生/一時停止、← → 前/次の曲、Ctrl+O ROMを開く、Ctrl+F 検索、Ctrl+M ミニプレイヤーと切り替え。
シークバーはクリックかドラッグで再生位置を動かせ、フォーカス中は ← → で5秒ずつ移動します。
言語・音量・ループ・出力・テーマの設定は `%APPDATA%\gb-player\settings.toml`(Linux/macOSは `~/.config/gb-player/settings.toml`)に保存されます。
ウィンドウの位置と大きさも次回の起動に引き継がれます。

ROMはいくつでも同時に開けます(ファイル選択で複数選ぶか、まとめてドロップ)。開いたROMは左の「開いているROM」に並び、
クリックで曲リストを切り替えても再生中の曲はそのまま鳴り続けます。× か右クリックの「ROMを閉じる」で閉じられます。
開いていたROMは次回の起動でもすべて開き直します。

右側の「シャッフル」で次の曲の選び方を変えられます。「一覧の中」は再生中のROMの曲(プレイリストの再生中はそのプレイリストの曲)を、
「全ROM」は開いているROMすべての曲(プレイリストの再生中はそのプレイリストの曲)を混ぜます。どちらも全曲を一巡するまで
同じ曲は出ず、「前の曲」でシャッフルで聴いた順に戻れます。

複数のROMの曲を混ぜたプレイリストを作れます。曲を右クリックして「プレイリストに追加」から
追加先(または新しいプレイリスト)を選ぶと左の一覧に入り、プレイリスト内はドラッグで並べ替え、Deleteで外せます。
プレイリストは `%APPDATA%\gb-player\playlists\*.json`(Linux/macOSは `~/.config/gb-player/playlists/`)に
ROMのパスとファイルのハッシュで保存されます。ROMを移動して見つからなくなった曲はグレーで表示され、
クリックするとファイルを選び直せます(ハッシュが違うROMを選ぶと確認が出ます。勝手にファイルを探し回ることはしません)。

## ミニプレイヤー

右上のボタン(最小化の左)か Ctrl+M、タイトルバーの右クリックの「ミニプレイヤー」で、ウィンドウが携帯ゲーム機の形の
小さなプレイヤーになります。画面に曲名・波形・再生位置が出て、ボタンで操作できます。

- 十字キーの ← → で前/次の曲、↑ ↓ で音量
- A で再生/一時停止、B で停止
- SELECT でシャッフルの切り替え、START(か Ctrl+M)で元の大きさに戻る
- 上のピンで常に手前に表示するかを切り替え

ミニプレイヤーの位置と、元のウィンドウの位置・大きさはそれぞれ覚えていて、ミニプレイヤーのまま閉じると次もミニプレイヤーで
起動します。本体の形(初代型かカラー型か)と色はテーマで決まり、本体の色を半透明にすると中の部品と、縁から後ろの画面が透けて見えます。

## テーマ

画面右上のパレットのメニューで配色を切り替えられます。内蔵は次の6つです。

| テーマ | 配色 | ミニプレイヤー |
|---|---|---|
| DMG | 初代の4階調の緑 | 初代型・グレー |
| Pocket | グレー | 初代型・シルバー |
| Color | 紺と黄 | カラー型・黄色 |
| Prism | 明るいグレーにパールのような虹色の光沢 | カラー型・クリア |
| Nebula | 深い紫に夕焼けのような虹色の光沢 | カラー型・クリアパープル |
| Daylight | 明るい背景に初代の緑 | 初代型・ホワイト |

以前の名前の `clear` と `light` は、それぞれ Prism と Daylight として読み込みます。

隣の鉛筆ボタンでテーマエディタが開き、色を変えるとその場で画面に反映されます。「名前を付けて保存…」で
`%APPDATA%\gb-player\themes\<名前>.toml`(Linux/macOSは `~/.config/gb-player/themes/`)に保存され、メニューに追加されます。
このフォルダのファイルは手で書いても構いません(起動時と保存時に読み込みます)。

```toml
name = "夜のマサラタウン"   # 省略するとファイル名
base = "color"              # 元にする内蔵テーマ: dmg / pocket / color / prism / nebula / daylight
console = "gbc"             # ミニプレイヤーの本体: dmg(初代型)/ gbc(カラー型)。省略すると base に合わせる
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

[sheen]                     # 光沢(省略すると base のまま。colors = [] で光沢なし)
angle = 115                 # グラデーションの角度(度)
colors = ["#bdf3e4", "#d2c8fb", "#f8cbe4"]   # 8色まで
```

光沢があると、差し色で塗る部分(再生ボタンや選んでいる項目など)にそのグラデーションがかかります。
差し色(`accent`)は文字だけに使い、縁やメーター、リストのハイライトは `accent-2` の色になります。

重なり方は奥から `backdrop`(背景画像があるときは画像)→ `bg` → パネル類です。内蔵テーマの `bg` と
`panel` `panel-2` は最初から半透明で、`backdrop` の上に重なると不透明な色と同じ見た目になるように
作ってあるので、画像を設定するだけで透けて見えます。`bg` を不透明に近くすると画像はほとんど見えなくなります
(エディタに注意が出ます)。エディタの「画像を選ぶ…」で選んだ画像は `themes\images\` にコピーされます。

色の名前は `backdrop` `bg` `panel` `panel-2` `line` `text` `muted` `faint` `accent` `accent-2` `accent-ink`
`lcd-hi` `lcd` `lcd-lo` `lcd-ink` `lcd-grid` `bezel` `overlay` `danger` `knob`(スライダーとスイッチのつまみ)、
ミニプレイヤーの `shell`(本体)`shell-ink`(本体の文字)`shell-frame`(画面の枠)`key`(A・Bボタン)`key-ink`
(ボタンの文字)です。値は `#rgb` / `#rrggbb` /
`#rrggbbaa` か、数値だけの `rgb()` `rgba()` `hsl()` `hsla()` に限ります。知らない名前や色でない値は無視され、
画面上部に警告が出ます。

## 曲名

曲名はROMには入っていないので、`songdb/` の曲データベースから引いています。日本語の曲名は場所や場面を表す
説明的な名前で、公式サウンドトラックの曲名ではありません。英語名は pret の逆アセンブルのラベルが元です。

曲名の修正、他の言語の追加、新しいソフトの曲リストはPR歓迎です。書き方は [songdb/README.md](songdb/README.md) を参照してください。
手元だけで名前を変えたいときは、同じ形式のファイルを `%APPDATA%\gb-player\songdb\<シリーズ>\<言語>.toml` に置くと内蔵の名前より優先されます。

ROMは `rom/` に置くのがおすすめです(`.gitignore` で除外済み、`*.gb` / `*.gbc` / `*.gbs` もどこに置いてもコミットされません)。

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
tools            曲データベースの生成スクリプト、スピーカーの測定スクリプト
```

設計の詳細は [docs/DESIGN.md](docs/DESIGN.md) を参照してください。

## テスト

```sh
cargo test --workspace
```

CPUは [SingleStepTests/sm83](https://github.com/SingleStepTests/sm83) のテストベクタでも検証できます
(`v1/*.json` を置いたディレクトリを `SM83_TESTS_DIR` に指定)。

## ライセンス

gb-player のソースコードは [MIT License](LICENSE) です。

使っているライブラリはそれぞれのライセンスに従います。ほとんどは MIT か Apache-2.0 で、
音声ファイルのデコードに使う [symphonia](https://github.com/pdeljanov/Symphonia) と、Tauri が使う一部のクレート
(cssparser など)は MPL-2.0 です。どれも変更せずに使っていて、ソースは crates.io から入手できます。
ビルドしたアプリを配布するときは、これらのライセンス表示を添えてください。

## 権利表記

gb-player は個人が作った非公式のツールで、任天堂・株式会社ポケモン・株式会社ゲームフリーク・
株式会社クリーチャーズとは関係ありません。「ポケットモンスター」「ポケモン」「Pokémon」「ゲームボーイ」
「Game Boy」は各権利者の商標または登録商標です。

ROMやGBSファイル、ゲームの音楽はリポジトリにも配布物にも含みません。手元の正規に入手したソフトから、
各自で吸い出したものを使ってください。曲名の一覧(`songdb/`)は、曲の識別と表示のための名前だけを収めています。
