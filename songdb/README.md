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

## 内蔵の検出器が無いゲーム(`[game.playback]`)

ポケモン以外のゲームは、ゲームのサウンドドライバの「呼び方」を書けば鳴らせます。
曲データの中身を書く必要はありません(ゲーム自身のドライバが読みます)。
次のファイルを置くと、ヘッダタイトル(と `checksums`)が一致するROMを開いたときに使われます。

- Windows: `%APPDATA%\gb-player\songdb\<シリーズ>\songs.toml`(Linux / macOS は上と同じ場所)
- `<シリーズ>` は英小文字・数字・`-` だけの名前です。同じフォルダの `<言語コード>.toml` が曲名になります。

```toml
[[game]]
id = "some-game"
titles = ["SOME GAME"]
songs = [{ id = 0x01, key = "title" }, { id = 0x02, key = "field", intro = 1.5, loop = 30.0 }]
song_range = { first = 0x01, last = 0x30 }    # 任意: songs に無い番号も「Song $XX」として出す

[game.playback]
rate = "vblank"                               # 既定。タイマー割り込みのゲームは { timer = { tac = 0x04, tma = 0xC0 } }
setup = [                                     # 任意: 曲ごとのリセット直後に1回
  { write = 0xFF26, value = 0x80 },
  { bank = 1, call = 0x4000 },                # ドライバの初期化
  { frames = 30 },                            # update を30回空回し
]
start = [ { bank = 1, call = 0x4003, a = "song" } ]     # 曲を始める
update = [ { bank = 1, call = 0x4006 } ]                # rate の間隔で(既定は毎フレーム 59.7Hz)

[game.probe]                                  # 任意: 正確なループ検出用。無ければRAM全体で判定
active  = { at = 0xC104, step = 0x32, mask = 0x01 }     # チャンネル c の値は at + c * step
address = { at = 0xC107, step = 0x32, size = 2 }        # 次に読むコマンドのアドレス
return  = { at = 0xC109, step = 0x32, size = 2 }        # サブルーチンの戻り先
loop    = { at = 0xC119, step = 0x32 }                  # ループカウンタ
flags   = { at = 0xC104, step = 0x32, mask = 0x06 }
timer   = { at = 0xC116, step = 0x32 }                  # 今の音符の残りフレーム
check   = { song_id = { at = 0xC101, step = 0x32, size = 2 }, all = false }
```

`rate` の timer は TAC/TMA レジスタの値で、更新の間隔はタイマー割り込みと同じ(4096Hz / (256 - tma) など)になります。
更新が1フレームに16回を超える設定は受け付けません。

手順(ステップ)は次のどれか1つです。

- `{ write = アドレス, value = 値 }`: 1バイト書く(RAM、I/O、バンク切替レジスタ)
- `{ bank = 値 }`: $4000〜$7FFF のROMバンクを切り替える。`call` / `call_by` と組み合わせると呼ぶ直前に切り替える
- `{ call = アドレス, a = 値, de = 値, ... }`: レジスタ(a b c d e h l bc de hl)を設定して呼ぶ
- `{ call_by = アドレス, cases = [[値, 呼び先], ...], default = 呼び先 }`: RAMの値で呼び先を選ぶ
- `{ frames = 回数 }`: update をその回数だけ実行する(setup / start のみ、合わせて300回まで)

値は数値、`"song"`(曲番号)、`"song_bank"`(曲の `bank`)、`{ ram = アドレス }`(そのときのRAMの値)のどれかです。

このファイルにできるのは、エミュレートしたマシンの中でバイトを書き、レジスタを設定し、アドレスを呼ぶことだけです。
ファイルは1MiBまで、曲は512曲まで、手順は各64個まで、1回の呼び出しは約10フレーム分のCPU時間までに制限していて、
外れたファイルやゲームは警告を出して読み飛ばします。
