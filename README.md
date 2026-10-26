# gb-player

Game Boy / Game Boy Color のROM(.gb / .gbc)を読み込み、ゲーム自身のサウンドエンジンを
エミュレータ上で動かしてBGMをリアルタイム再生するRust製GUIプレイヤーです。

- SM83 CPU・MBC1/2/3/5・APU(4チャンネル)を実装した最小限のエミュレーションコア
- ゲームの「曲を再生する関数」と「毎フレーム呼ばれる更新関数」をROM内から自動検出して呼び出す(GBS方式)
- ループ点を自動解析し、無限ループ再生/N回ループ後フェードアウトして次の曲へ、を選択可能
- チャンネルごとのミュートとレベルメーター
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

```sh
cargo run --release                      # GUIを起動(ROMは「ROMを開く…」かドラッグ&ドロップ)
cargo run --release -- pokemon.gb        # ROMを指定して起動
cargo run --release -- --info pokemon.gb # 検出結果と全曲のループ長を表示
cargo run --release -- --wav pokemon.gb 1 out.wav 2   # 1曲目を2ループ+フェードでWAV出力
cargo run --release -- --lang en --info pokemon.gb     # 言語を指定(既定はOSの言語)
```

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

### Linux

ALSA の開発パッケージ(Debian/Ubuntuなら `libasound2-dev`)が必要です。

### macOS

追加のパッケージは不要です。

## 構成

```
crates/gb-core   エミュレーションコア(CPU / メモリマップ / MBC / APU)
crates/gb-music  サウンドドライバ検出、曲リスト、再生とループ解析
app              GUI(egui/eframe)と音声出力(cpal)、CLI
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
