English | [日本語](README.ja.md)

# Song database

ROMs don't contain song names, so this folder records which songs each game has and what they are called in each language. The files are embedded in the player when it is built.

```
songdb/
  pokemon-gen1/      Red, Green, Blue, Yellow (Gen 1 sound engine)
    songs.toml       matches the game, and maps song IDs in the ROM to song keys (language-neutral)
    en.toml          song key -> English title
    ja.toml          song key -> Japanese title
  pokemon-gen2/      Gold, Silver, Crystal (Gen 2 sound engine)
    ...
```

A folder is a series that shares a sound engine and a set of songs, not a single game. Revisions (Red, Green, Blue and so on) and regional versions are separate `[[game]]` entries in the same `songs.toml`.

## songs.toml

```toml
[[game]]
id = "red-green-blue"                                # the ID the language files name in [games]
titles = ["POKEMON RED", "POKEMON GREEN", "POKEMON BLUE"]  # cartridge header title ($0134 on)
checksums = [0x91E6]                                # optional: global checksum ($014E on), only to tell apart versions with the same title
songs = [
    { bank = 1, id = 0xBA, key = "pallet_town" },   # Gen 1: audio bank number (from 1) and sound ID
    { bank = 1, id = 0xBD, key = "pokecenter", intro = 1.82, loop = 28.93 },
    { bank = 1, id = 0xB8, key = "some_jingle", hidden = true },
]
```

- Gen 2 has no `bank`, only the song number: `{ id = 0x1C, key = "pallet_town" }`.
- `intro` and `loop` (seconds), or `end` (the length of a song that doesn't loop, in seconds), replace the automatic analysis.
- Songs with `hidden = true` are left out of the list, for example a sound effect that looks like a song.
- A ROM whose title and checksum match no game is treated as the game whose `songs` name the most of the songs found in it.
- Song keys are language-neutral names shared within a series, made from pret's labels (`Music_PalletTown` becomes `pallet_town`).

## Language files (`<language code>.toml`)

```toml
[games]
"red-green-blue" = "Pokémon Red / Green / Blue"

[songs]
pallet_town = "Pallet Town Theme"
```

- A song with no entry shows its English title, or its song key if that is missing too, so a partial translation is fine.
- A key that isn't in `songs.toml` fails the tests, which catches typos.
- Titles are the official ones: the Pokémon Game Sound Library for Gen 1, and for Gen 2 the HeartGold & SoulSilver soundtrack's title for the arrangement of the same song. Songs that aren't listed there, or whose match is uncertain, have a name describing the place or scene.

When you add a language, also add a line to `BUILTIN` and `LANGUAGES` in `crates/gb-music/src/songdb.rs`.

## Changing names on your own machine

A language file in the same format takes priority over the built-in names, with no rebuild, when placed at:

- Windows: `%APPDATA%\gb-player\songdb\<series>\<language code>.toml`
- Linux and macOS: `$XDG_CONFIG_HOME/gb-player/songdb/...` (`~/.config/gb-player/songdb/...` if unset)

## Games without a built-in detector (`[game.playback]`)

A game other than Pokémon can play once you describe how to call its sound driver. You don't describe the song data, because the game's own driver reads it. The following file is used when a ROM's header title (and `checksums`) match:

- Windows: `%APPDATA%\gb-player\songdb\<series>\songs.toml` (Linux and macOS: the same place as above)
- `<series>` may contain only lowercase letters, digits and `-`. The `<language code>.toml` files in the same folder give the song names.

```toml
[[game]]
id = "some-game"
titles = ["SOME GAME"]
songs = [{ id = 0x01, key = "title" }, { id = 0x02, key = "field", intro = 1.5, loop = 30.0 }]
song_range = { first = 0x01, last = 0x30 }    # optional: also list numbers not in songs, as "Song $XX"

[game.playback]
rate = "vblank"                               # the default; for games on the timer interrupt, { timer = { tac = 0x04, tma = 0xC0 } }
setup = [                                     # optional: once, after the reset for each song
  { write = 0xFF26, value = 0x80 },
  { bank = 1, call = 0x4000 },                # initialise the driver
  { frames = 30 },                            # run update 30 times
]
start = [ { bank = 1, call = 0x4003, a = "song" } ]     # start the song
update = [ { bank = 1, call = 0x4006 } ]                # at the rate (by default every frame, 59.7 Hz)

[game.probe]                                  # optional: for exact loop detection; without it, all of RAM is compared
active  = { at = 0xC104, step = 0x32, mask = 0x01 }     # the value for channel c is at + c * step
address = { at = 0xC107, step = 0x32, size = 2 }        # address of the next command to read
return  = { at = 0xC109, step = 0x32, size = 2 }        # where a subroutine returns to
loop    = { at = 0xC119, step = 0x32 }                  # loop counter
flags   = { at = 0xC104, step = 0x32, mask = 0x06 }
timer   = { at = 0xC116, step = 0x32 }                  # frames left in the current note
check   = { song_id = { at = 0xC101, step = 0x32, size = 2 }, all = false }
```

The `rate` timer takes TAC and TMA register values, and updates run as often as the timer interrupt would (4096 Hz / (256 - tma) and so on). Settings with more than 16 updates per frame are rejected.

Each step is one of these:

- `{ write = address, value = value }`: write one byte (RAM, I/O, bank switching registers)
- `{ bank = value }`: switch the ROM bank at $4000 to $7FFF; with `call` or `call_by`, it switches just before the call
- `{ call = address, a = value, de = value, ... }`: set registers (a b c d e h l bc de hl) and call
- `{ call_by = address, cases = [[value, target], ...], default = target }`: choose the target by a value in RAM
- `{ frames = count }`: run update that many times (`setup` and `start` only, up to 300 in total)

A value is a number, `"song"` (the song number), `"song_bank"` (the song's `bank`) or `{ ram = address }` (the value in RAM at that moment).

This file can only write bytes, set registers and call addresses inside the emulated machine. Files are limited to 1 MiB, songs to 512, steps to 64 each and one call to about 10 frames of CPU time. Files and games outside those limits are skipped with a warning.
