//! Sound drivers: how to start a song and advance it by one frame for each supported engine.
//!
//! Instead of embedding per-version addresses, each driver finds the game's own audio entry
//! points by searching for the machine code that surrounds them. The code is taken from the
//! pret disassemblies (pokered, pokeyellow, pokegold, pokecrystal); the same engine code is
//! shared by the Japanese and international releases, only at different addresses.

use gb_core::{CallError, Header, Machine};

use crate::length::SongLength;
use crate::pattern::{word, Pattern};
use crate::playback::{Playback, Rate, Reg, Step, Target, Value};
use crate::probe::{ChannelProbe, Field, SongCheck};
use crate::songdb::{self, Game, SongDef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    /// Position in [`crate::Player::songs`].
    pub index: usize,
    /// Stable song key from the song database; `None` for songs it does not know.
    pub key: Option<String>,
    /// The value the game passes to its play routine.
    pub id: u16,
    /// ROM bank whose engine plays the song (Gen 1 only; 0 otherwise).
    pub bank: u8,
    /// Loop points fixed in the song database, used instead of analyzing the song.
    pub length: Option<SongLength>,
}

impl Song {
    fn new(index: usize, id: u16, bank: u8, def: Option<&SongDef>) -> Song {
        Song {
            index,
            key: def.map(|d| d.key.clone()),
            id,
            bank,
            length: def.and_then(SongDef::length),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Pokémon Red / Green / Blue: one engine copy per audio bank, picked by the bank.
    Gen1 {
        play_music: u16,
        audio_rom_bank: u16,
        loaded_rom_bank: u16,
        banks: [u8; 2],
        updates: [u16; 3],
    },
    /// Pokémon Yellow: engine 1 plays music from every audio bank.
    Gen1Yellow {
        play_music: u16,
        loaded_rom_bank: u16,
        bank: u8,
        update: u16,
    },
    /// Pokémon Gold / Silver / Crystal.
    Gen2 { play_music: u16, update_sound: u16 },
    /// No built-in detector: driven by the playback of the game's song database entry.
    Configured,
}

#[derive(Debug, Clone)]
pub struct Driver {
    pub engine: Engine,
    pub probe: Option<ChannelProbe>,
    /// Song database series and game the ROM was matched to.
    pub series: String,
    pub game_id: String,
    pub songs: Vec<Song>,
    /// How to start songs and advance the engine.
    pub playback: Playback,
    /// Problems found in the user's song database files, which were skipped.
    pub warnings: Vec<String>,
}

fn bank_slice(rom: &[u8], bank: u8) -> &[u8] {
    let start = bank as usize * 0x4000;
    &rom[start.min(rom.len())..(start + 0x4000).min(rom.len())]
}

fn rom_at(rom: &[u8], bank: u8, addr: u16) -> Option<&[u8]> {
    if addr < 0x4000 {
        return rom.get(addr as usize..);
    }
    if addr >= 0x8000 {
        return None;
    }
    rom.get(bank as usize * 0x4000 + (addr as usize - 0x4000)..)
}

impl Driver {
    /// Find the sound engine of `rom` and the song database entries of its songs.
    pub fn detect(rom: &[u8]) -> Option<Driver> {
        if rom.len() < 0x8000 {
            return None;
        }
        let (title, checksum) = Header::parse(rom)
            .map(|h| (h.title, h.global_checksum))
            .unwrap_or_default();
        // A song database entry with its own playback comes first: it was written for this
        // very release.
        let (user, warnings) = songdb::user_series();
        let mut all: Vec<(&str, &songdb::Series)> = songdb::builtin_series().collect();
        all.extend(user.iter().map(|(name, series)| (name.as_str(), series)));
        let configured = all.into_iter().find_map(|(name, series)| {
            let game = series
                .games
                .iter()
                .find(|g| g.playback.is_some() && g.matches(&title, checksum))?;
            Some((name, game))
        });
        let mut driver = match configured {
            Some((series, game)) => Driver::from_game(series, game)?,
            None => {
                detect_gen2(rom, &title, checksum).or_else(|| detect_gen1(rom, &title, checksum))?
            }
        };
        driver.warnings = warnings;
        Some(driver)
    }

    /// The driver a song database entry with a playback describes; `None` if it has none or
    /// breaks the limits of [`Game::validate`].
    pub fn from_game(series: &str, game: &Game) -> Option<Driver> {
        game.validate().ok()?;
        let playback = game.playback.as_ref()?.build().ok()?;
        let probe = game.probe.as_ref().map(|p| p.build()).transpose().ok()?;
        let songs = game
            .song_ids()
            .into_iter()
            .enumerate()
            .map(|(index, id)| {
                let def = game.song(id);
                Song::new(index, id, def.and_then(|d| d.bank).unwrap_or(0), def)
            })
            .collect();
        Some(Driver {
            engine: Engine::Configured,
            probe,
            series: series.to_string(),
            game_id: game.id.clone(),
            songs,
            playback,
            warnings: Vec::new(),
        })
    }

    /// Reset the sound engine and start `song`.
    pub fn play(&self, m: &mut Machine, song: &Song) -> Result<(), CallError> {
        self.playback.start(m, song)
    }

    /// Run the per-frame update the game normally calls from its VBlank handler.
    /// Returns the T-cycles it took.
    pub fn update(&self, m: &mut Machine) -> Result<u32, CallError> {
        self.playback.update(m)
    }
}

impl Engine {
    /// The steps that drive this engine. `home_bank` is the first bank of the audio engine,
    /// which the Gen 1 engines are reset from.
    fn playback(&self, home_bank: u8) -> Playback {
        let write = |addr: u16, v: u16| Step::Write {
            addr,
            value: Value::Const(v),
        };
        let call = |addr: u16, regs: &[(Reg, Value)]| Step::Call {
            target: Target::Fixed(addr),
            regs: regs.to_vec(),
        };
        let gen1_start = |play_music: u16, loaded_rom_bank: u16| {
            vec![
                write(0xFF26, 0x80),
                write(0xFF24, 0x77),
                write(0xFF25, 0xFF),
                write(loaded_rom_bank, home_bank as u16),
                Step::Bank(Value::Const(home_bank as u16)),
                // Sound ID $FF stops everything, like the game's StopAllSounds.
                call(
                    play_music,
                    &[
                        (Reg::A, Value::Const(0xFF)),
                        (Reg::C, Value::Const(home_bank as u16)),
                    ],
                ),
                call(
                    play_music,
                    &[(Reg::A, Value::Song), (Reg::C, Value::SongBank)],
                ),
            ]
        };
        let (start, update) = match *self {
            Engine::Gen1 {
                play_music,
                audio_rom_bank,
                loaded_rom_bank,
                banks,
                updates,
            } => (
                gen1_start(play_music, loaded_rom_bank),
                // The engine copy to run is the one in the bank the music plays from.
                vec![
                    Step::Write {
                        addr: loaded_rom_bank,
                        value: Value::Ram(audio_rom_bank),
                    },
                    Step::Bank(Value::Ram(audio_rom_bank)),
                    Step::Call {
                        target: Target::ByRam {
                            ram: audio_rom_bank,
                            cases: vec![(banks[0], updates[0]), (banks[1], updates[1])],
                            default: updates[2],
                        },
                        regs: Vec::new(),
                    },
                ],
            ),
            Engine::Gen1Yellow {
                play_music,
                loaded_rom_bank,
                bank,
                update,
            } => (
                gen1_start(play_music, loaded_rom_bank),
                vec![
                    write(loaded_rom_bank, bank as u16),
                    Step::Bank(Value::Const(bank as u16)),
                    call(update, &[]),
                ],
            ),
            Engine::Gen2 {
                play_music,
                update_sound,
            } => (
                vec![
                    write(0xFF26, 0x80),
                    Step::Bank(Value::Const(1)),
                    call(play_music, &[(Reg::De, Value::Const(0))]),
                    call(play_music, &[(Reg::De, Value::Song)]),
                ],
                vec![call(update_sound, &[])],
            ),
            // Described by data instead; see Driver::from_game.
            Engine::Configured => (Vec::new(), Vec::new()),
        };
        Playback {
            rate: Rate::VBlank,
            setup: Vec::new(),
            start,
            update,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Generation 1

/// home/audio.asm `PlayMusic`:
/// ld b,a / ld [wNewSoundID],a / xor a / ld [wAudioFadeOutControl],a / ld a,c /
/// ld [wAudioROMBank],a / ld [wAudioSavedROMBank],a / ld a,b
const GEN1_PLAY_MUSIC: &str = "47 EA ?? ?? AF EA ?? ?? 79 EA ?? ?? EA ?? ?? 78";

/// home/vblank.asm (Red/Blue): cp BANK(Audio1_UpdateMusic) / jr nz,.audio2 /
/// call Audio1_UpdateMusic / jr .after / cp BANK(Audio2_UpdateMusic) / jr nz,.audio3
const GEN1_VBLANK_DISPATCH: &str = "FE ?? 20 05 CD ?? ?? 18 ?? FE ?? 20";

/// home/vblank.asm (Yellow): ld a,BANK(Music_DoLowHealthAlarm) / call BankswitchCommon /
/// call Music_DoLowHealthAlarm / ld a,BANK(Audio1_UpdateMusic) / call BankswitchCommon /
/// call Audio1_UpdateMusic
const YELLOW_VBLANK: &str = "3E ?? CD ?? ?? CD ?? ?? 3E ?? CD ?? ?? CD ?? ??";

/// (sound ID, channel count, first channel)
type HeaderGroup = (u8, usize, u8);

/// audio/engine_N.asm `AudioN_PlaySound`: ld l,a / ld e,a / ld h,0 / ld d,h / add hl,hl /
/// add hl,de / ld de,SFX_Headers_N / add hl,de — the sound ID times three plus the table base.
const GEN1_HEADER_LOOKUP: &str = "6F 5F 26 00 54 29 19 11 ?? ?? 19";

/// Locate the channel header table of an audio bank. The international releases put it at
/// the start of the bank behind `FF FF FF` padding; the Japanese ones put engine code first,
/// so the table address is taken from the engine's own lookup code.
fn gen1_header_table(bank: &[u8]) -> Option<&[u8]> {
    let lookup = Pattern::new(GEN1_HEADER_LOOKUP);
    for pos in lookup.find_all(bank) {
        let base = word(bank, pos + 8);
        if let Some(table) = (0x4000..0x8000)
            .contains(&base)
            .then(|| bank.get(base as usize - 0x4000..))
            .flatten()
        {
            return Some(table);
        }
    }
    (bank.get(0..3) == Some(&[0xFF, 0xFF, 0xFF][..])).then_some(bank)
}

/// Parse a Gen 1 header table: 3-byte channel headers indexed by sound ID, where ID 0 is
/// unused. Returns (sound ID, channel count, first channel) for every header group.
fn gen1_header_groups(table: &[u8]) -> Vec<HeaderGroup> {
    let mut groups = Vec::new();
    let mut id = 1usize;
    while id <= 0xFF {
        let Some(&first) = table.get(id * 3) else {
            break;
        };
        let count = (first >> 6) as usize + 1;
        if id + count > 0x100 {
            break;
        }
        let valid = (0..count).all(|k| {
            let Some(e) = table.get((id + k) * 3..(id + k) * 3 + 3) else {
                return false;
            };
            let ptr = u16::from_le_bytes([e[1], e[2]]);
            let ch = e[0] & 0x0F;
            let hi_ok = if k == 0 {
                e[0] & 0x30 == 0
            } else {
                e[0] & 0xF0 == 0
            };
            hi_ok && ch < 8 && (0x4000..0x8000).contains(&ptr)
        });
        if !valid {
            break;
        }
        groups.push((id as u8, count, first & 0x0F));
        id += count;
    }
    groups
}

fn detect_gen1(rom: &[u8], title: &str, checksum: u16) -> Option<Driver> {
    let home = &rom[..0x4000];
    let pm_pat = Pattern::new(GEN1_PLAY_MUSIC);
    let pm = pm_pat.find_all(home).next()?;
    let audio_rom_bank = word(home, pm + 10);

    // Audio banks are the ones whose header table parses into a sizeable list.
    let bank_count = (rom.len() / 0x4000).min(0x100);
    let mut audio_banks: Vec<(u8, Vec<HeaderGroup>)> = (1..bank_count)
        .map(|b| {
            let table = gen1_header_table(bank_slice(rom, b as u8));
            (b as u8, table.map(gen1_header_groups).unwrap_or_default())
        })
        .filter(|(_, g)| g.len() >= 20 && g.iter().any(|&(_, _, ch)| ch < 4))
        .collect();
    audio_banks.sort_by_key(|(b, _)| *b);
    if audio_banks.is_empty() {
        return None;
    }

    let engine = if let Some(e) = find_rb_dispatch(home, audio_rom_bank) {
        e(pm as u16)
    } else {
        find_yellow_dispatch(home, pm as u16)?
    };
    // The music IDs of each audio bank, with the bank's ordinal as the song database uses it.
    let music: Vec<(u8, u8, Vec<u8>)> = audio_banks
        .iter()
        .enumerate()
        .map(|(i, (bank, groups))| {
            let ids = groups.iter().filter(|g| g.2 < 4).map(|g| g.0).collect();
            (*bank, i as u8 + 1, ids)
        })
        .collect();
    let game = songdb::series(songdb::GEN1).identify(title, checksum, |g| {
        music
            .iter()
            .map(|(_, ordinal, ids)| {
                (0..ids.len())
                    .filter(|&n| gen1_song_def(g, *ordinal, ids, n).is_some())
                    .count()
            })
            .sum()
    });

    let mut songs = Vec::new();
    for (bank, ordinal, ids) in &music {
        for (n, &id) in ids.iter().enumerate() {
            let def = game.and_then(|g| gen1_song_def(g, *ordinal, ids, n));
            if def.is_some_and(|d| d.hidden) {
                continue;
            }
            songs.push(Song::new(songs.len(), id as u16, *bank, def));
        }
    }
    let home_bank = audio_banks[0].0;
    // In the Gen 1 audio RAM layout wNewSoundID directly precedes wAudioROMBank, and the
    // channel arrays end 233 bytes earlier. Only trust that layout if the first part holds.
    let new_sound_id = word(home, pm + 2);
    let probe = audio_rom_bank
        .checked_sub(233)
        .filter(|_| new_sound_id.wrapping_add(1) == audio_rom_bank)
        .map(gen1_probe);
    Some(Driver {
        playback: engine.playback(home_bank),
        engine,
        probe,
        series: songdb::GEN1.to_string(),
        warnings: Vec::new(),
        game_id: game_id(game),
        songs,
    })
}

/// The song database entry of the `n`th music ID in `ids`, the music of the audio bank with
/// this ordinal. Songs are matched by sound ID; if a release shifted the IDs but kept the song
/// order, by their position within the bank.
fn gen1_song_def<'a>(game: &'a Game, ordinal: u8, ids: &[u8], n: usize) -> Option<&'a SongDef> {
    let known: Vec<&SongDef> = game
        .songs
        .iter()
        .filter(|s| s.bank == Some(ordinal))
        .collect();
    known
        .iter()
        .find(|k| k.id == ids[n] as u16)
        .or_else(|| (known.len() == ids.len()).then(|| &known[n]))
        .copied()
}

/// The channel state of the Gen 1 engine: parallel arrays starting at
/// `wChannelCommandPointers`. The command pointers and return addresses are words; after them
/// come byte arrays of eight entries each, among them wChannelSoundIDs (0), wChannelFlags1
/// (1), wChannelNoteDelayCounters (18) and wChannelLoopCounters (19).
fn gen1_probe(command_pointers: u16) -> ChannelProbe {
    let p = command_pointers;
    let array = |n: u16| Field::byte(p.wrapping_add(32 + 8 * n), 1);
    ChannelProbe {
        active: array(0),
        address: Field::word(p, 2),
        return_address: Field::word(p.wrapping_add(16), 2),
        loop_count: array(19),
        // Only the "inside sound_call" bit; the others change mid-note.
        flags: array(1).masked(0x02),
        note_timer: array(18),
        check: Some(SongCheck {
            song_id: array(0),
            all: true,
        }),
    }
}

type Gen1Builder = Box<dyn Fn(u16) -> Engine>;

fn find_rb_dispatch(home: &[u8], audio_rom_bank: u16) -> Option<Gen1Builder> {
    let pat = Pattern::new(GEN1_VBLANK_DISPATCH);
    for pos in pat.find_all(home) {
        let b1 = home[pos + 1];
        let u1 = word(home, pos + 5);
        let b2 = home[pos + 10];
        let Some(&skip) = home.get(pos + 12) else {
            continue;
        };
        let skip = skip as usize;
        let mut p = pos + 13;
        // Engine 2 may first call Music_DoLowHealthAlarm.
        let calls_after = if skip == 8 { 2 } else { 1 };
        let mut last_call = 0;
        let mut ok = true;
        for _ in 0..calls_after {
            if home.get(p) != Some(&0xCD) || home.len() < p + 3 {
                ok = false;
                break;
            }
            last_call = word(home, p + 1);
            p += 3;
        }
        if !ok || home.get(p..p + 3) != Some(&[0x18, 0x03, 0xCD][..]) || home.len() < p + 5 {
            continue;
        }
        let u2 = last_call;
        let u3 = word(home, p + 3);
        // Look back for `ld a,[wAudioROMBank]` and `ldh [hLoadedROMBank],a`.
        let back = &home[pos.saturating_sub(12)..pos];
        let ld = back
            .windows(3)
            .any(|w| w[0] == 0xFA && u16::from_le_bytes([w[1], w[2]]) == audio_rom_bank);
        let Some(hl) = back.windows(2).rposition(|w| w[0] == 0xE0) else {
            continue;
        };
        if !ld {
            continue;
        }
        let loaded = 0xFF00 | back[hl + 1] as u16;
        return Some(Box::new(move |pm| Engine::Gen1 {
            play_music: pm,
            audio_rom_bank,
            loaded_rom_bank: loaded,
            banks: [b1, b2],
            updates: [u1, u2, u3],
        }));
    }
    None
}

fn find_yellow_dispatch(home: &[u8], pm: u16) -> Option<Engine> {
    let pat = Pattern::new(YELLOW_VBLANK);
    for pos in pat.find_all(home) {
        let bs = word(home, pos + 3);
        if word(home, pos + 11) != bs {
            continue;
        }
        // BankswitchCommon: ldh [hLoadedROMBank],a / ld [rROMB],a
        let body = home.get(bs as usize..bs as usize + 5)?;
        if body[0] != 0xE0 || body[2] != 0xEA {
            continue;
        }
        return Some(Engine::Gen1Yellow {
            play_music: pm,
            loaded_rom_bank: 0xFF00 | body[1] as u16,
            bank: home[pos + 9],
            update: word(home, pos + 14),
        });
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Generation 2

/// home/audio.asm `PlayMusic`: push hl/de/bc/af / ldh a,[hROMBank] / push af /
/// ld a,BANK(_PlayMusic) / ldh [hROMBank],a / ld [MBC3RomBank],a / ld a,e / and a /
/// jr z,.nomusic / call _PlayMusic / jr .end / call _InitSound
const GEN2_PLAY_MUSIC: &str =
    "E5 D5 C5 F5 F0 ?? F5 3E ?? E0 ?? EA 00 20 7B A7 28 05 CD ?? ?? 18 03 CD ?? ??";

/// home/audio.asm `InitSound` / `UpdateSound`, which share this shape.
const GEN2_WRAPPER: &str =
    "E5 D5 C5 F5 F0 ?? F5 3E ?? E0 ?? EA 00 20 CD ?? ?? F1 E0 ?? EA 00 20 F1 C1 D1 E1 C9";

/// audio/engine.asm `_PlayMusic`: call MusicOff / ld hl,wMusicID / ld [hl],e / inc hl /
/// ld [hl],d / ld hl,Music
const GEN2_PLAY_MUSIC_BODY: &str = "CD ?? ?? 21 ?? ?? 73 23 72 21 ?? ??";

fn detect_gen2(rom: &[u8], title: &str, checksum: u16) -> Option<Driver> {
    let home = &rom[..0x4000];
    let pm = Pattern::new(GEN2_PLAY_MUSIC).find_all(home).next()?;
    let bank = home[pm + 8];
    let play_impl = word(home, pm + 19);
    let init_impl = word(home, pm + 24);

    let update_sound = Pattern::new(GEN2_WRAPPER)
        .find_all(home)
        .find(|&p| home[p + 8] == bank && word(home, p + 15) != init_impl)?;

    let body = rom_at(rom, bank, play_impl)?;
    let body_pat = Pattern::new(GEN2_PLAY_MUSIC_BODY);
    if !body_pat.matches_at(body, 0) {
        return None;
    }
    let table = word(body, 10);
    // wMusicID follows the eight channel structs and twelve bytes of other audio state.
    let music_id = word(body, 4);
    let probe = music_id.checked_sub(8 * 0x32 + 12).map(gen2_probe);
    let entries = rom_at(rom, bank, table)?;

    let mut ids = Vec::new();
    for i in 0..0x100usize {
        let Some(e) = entries.get(i * 3..i * 3 + 3) else {
            break;
        };
        let addr = u16::from_le_bytes([e[1], e[2]]);
        let Some(header) = rom_at(rom, e[0], addr) else {
            break;
        };
        let ok = (0x4000..0x8000).contains(&addr)
            && (e[0] as usize) < rom.len() / 0x4000
            && header.first().is_some_and(|&h| h & 0x3C == 0);
        if !ok {
            break;
        }
        ids.push(i as u16);
    }
    if ids.len() < 2 {
        return None;
    }

    // ID 0 is "no music".
    let ids = &ids[1..];
    let game = songdb::series(songdb::GEN2).identify(title, checksum, |g| {
        ids.iter()
            .filter(|&&id| gen2_song_def(g, id).is_some())
            .count()
    });
    let mut songs = Vec::new();
    for &id in ids {
        let def = game.and_then(|g| gen2_song_def(g, id));
        if def.is_some_and(|d| d.hidden) {
            continue;
        }
        songs.push(Song::new(songs.len(), id, 0, def));
    }
    let engine = Engine::Gen2 {
        play_music: pm as u16,
        update_sound: update_sound as u16,
    };
    Some(Driver {
        playback: engine.playback(bank),
        engine,
        probe,
        series: songdb::GEN2.to_string(),
        warnings: Vec::new(),
        game_id: game_id(game),
        songs,
    })
}

fn gen2_song_def(game: &Game, id: u16) -> Option<&SongDef> {
    game.songs.iter().find(|s| s.id == id)
}

/// The channel state of the Gen 2 engine: one $32-byte `channel_struct` per channel starting
/// at `wChannel1`, with the music ID at +$00, the flags at +$03, MusicAddress at +$06,
/// LastMusicAddress at +$08, NoteDuration at +$15 and LoopCount at +$18.
fn gen2_probe(channel1: u16) -> ChannelProbe {
    let field = |offset: u16| Field::byte(channel1.wrapping_add(offset), 0x32);
    ChannelProbe {
        active: field(3).masked(0x01),
        address: Field::word(channel1.wrapping_add(6), 0x32),
        return_address: Field::word(channel1.wrapping_add(8), 0x32),
        loop_count: field(0x18),
        flags: field(3).masked(0x06),
        note_timer: field(0x15),
        check: Some(SongCheck {
            song_id: Field::word(channel1, 0x32),
            all: false,
        }),
    }
}

fn game_id(game: Option<&Game>) -> String {
    game.map_or_else(|| "unknown".to_string(), |g| g.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_groups_parse() {
        let mut bank = vec![0u8; 0x4000];
        bank[0..3].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
        let mut id = 1;
        // 19 one-channel SFX on channel 8, then a 3-channel song.
        for _ in 0..19 {
            bank[id * 3..id * 3 + 3].copy_from_slice(&[0x07, 0x00, 0x50]);
            id += 1;
        }
        bank[id * 3..id * 3 + 3].copy_from_slice(&[0x80, 0x00, 0x51]);
        bank[id * 3 + 3..id * 3 + 6].copy_from_slice(&[0x01, 0x00, 0x52]);
        bank[id * 3 + 6..id * 3 + 9].copy_from_slice(&[0x02, 0x00, 0x53]);
        let groups = gen1_header_groups(&bank);
        assert_eq!(groups.len(), 20);
        assert_eq!(groups[19], (20, 3, 0));
    }

    #[test]
    fn header_table_found_through_lookup_code() {
        // Japanese layout: engine code first, table elsewhere in the bank.
        let mut bank = vec![0u8; 0x4000];
        bank[0x100..0x10B].copy_from_slice(&[
            0x6F, 0x5F, 0x26, 0x00, 0x54, 0x29, 0x19, 0x11, 0x00, 0x60, 0x19,
        ]);
        let table = 0x2000;
        let mut id = 1;
        for _ in 0..19 {
            bank[table + id * 3..table + id * 3 + 3].copy_from_slice(&[0x07, 0x00, 0x50]);
            id += 1;
        }
        bank[table + id * 3..table + id * 3 + 6]
            .copy_from_slice(&[0x40, 0x00, 0x51, 0x01, 0x00, 0x52]);
        let groups = gen1_header_groups(gen1_header_table(&bank).unwrap());
        assert_eq!(groups.len(), 20);
        assert_eq!(groups[19], (20, 2, 0));
    }
}
