# 曲データベース

ROMには曲名が入っていないので、どのゲームにどの曲があり、各言語で何と呼ぶかをここで管理しています。
ファイルはビルド時にプレイヤーへ埋め込まれます。

```
songdb/
  pokemon-gen1/      赤・緑・青・ピカチュウ(第1世代サウンドエンジン)
    songs.toml       ゲームの照合と、ROM内の曲ID → 曲キー(言語に依存しない)
    en.toml          曲キー → 英語名
    ja.toml          曲キー → 日本語名
  pokemon-gen2/      金・銀・クリスタル(第2世代サウンドエンジン)
    ...
```

ディレクトリはソフト1本ではなく「サウンドエンジンと曲セットを共有するシリーズ」単位です。
版違い(赤/緑/青など)や言語違いは、同じシリーズの `songs.toml` の中で `[[game]]` として区別します。

## songs.toml

```toml
[[game]]
id = "red-green-blue"                                # 言語ファイルの [games] で名前を付けるID
titles = ["POKEMON RED", "POKEMON GREEN", "POKEMON BLUE"]  # カートリッジヘッダのタイトル($0134〜)
checksums = [0x91E6]                                # 任意: グローバルチェックサム($014E〜)。同じタイトルの版を分けたいときだけ
songs = [
    { bank = 1, id = 0xBA, key = "pallet_town" },   # 第1世代: オーディオバンクの順番(1〜)とサウンドID
    { bank = 1, id = 0xBD, key = "pokecenter", intro = 1.82, loop = 28.93 },
    { bank = 1, id = 0xB8, key = "some_jingle", hidden = true },
]
```

- 第2世代は `bank` を書かず `{ id = 0x1C, key = "pallet_town" }` のように曲番号だけです。
- `intro` / `loop`(秒)や `end`(ループしない曲の長さ、秒)を書くと、自動解析の代わりにその値を使います。
- `hidden = true` の曲は一覧に出ません(効果音を曲と誤認する場合など)。
- タイトル(とチェックサム)がどのゲームにも一致しないROMは、見つかった曲のうち `songs` で名前が付く曲がいちばん多いゲームとして扱います。
- 曲キーはシリーズ内で共通の、言語に依存しない名前です。pret のラベルから作っています(`Music_PalletTown` → `pallet_town`)。

`songs.toml` と `en.toml` は `tools/gen_song_names.py` で pret の逆アセンブルから生成できます。
生成し直すと手で足した `intro` / `hidden` などは消えるので、その場合は差分を見て戻してください。

## 言語ファイル (`<言語コード>.toml`)

```toml
[games]
"red-green-blue" = "ポケットモンスター 赤・緑・青"

[songs]
pallet_town = "マサラタウン"
```

- 書かれていない曲は英語名、それも無ければ曲キーで表示されます。一部の曲だけの翻訳でも大丈夫です。
- `songs.toml` に無いキーを書くとテストが失敗します(タイプミス防止)。
- `ja.toml` の曲名は場所や場面を表す説明的な名前です。公式サウンドトラックの曲名ではありません。

新しい言語を追加したときは、`crates/gb-music/src/songdb.rs` の `BUILTIN` と `LANGUAGES` にも1行ずつ足してください。

## 手元だけで変えたいとき

同じ形式の言語ファイルを次の場所に置くと、内蔵の名前より優先されます(再ビルド不要)。

- Windows: `%APPDATA%\gb-player\songdb\<シリーズ>\<言語コード>.toml`
- Linux / macOS: `$XDG_CONFIG_HOME/gb-player/songdb/...`(未設定なら `~/.config/gb-player/songdb/...`)
