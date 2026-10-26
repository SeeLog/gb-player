//! Sound drivers: how to start a song and advance it by one frame for each supported engine.
//!
//! Instead of embedding per-version addresses, each driver finds the game's own audio entry
//! points by searching for the machine code that surrounds them. The code is taken from the
//! pret disassemblies (pokered, pokeyellow, pokegold, pokecrystal); the same engine code is
//! shared by the Japanese and international releases, only at different addresses.

use gb_core::{CallError, Header, Machine};

use crate::length::SongLength;
use crate::pattern::{word, Pattern};
use crate::songdb::{self, Game, SongDef};

/// Upper bound for one call into the sound engine (about 10 frames of CPU time).
const CALL_BUDGET: u32 = 700_000;

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
}

/// Where the sound engine keeps its per-channel playback state in WRAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelProbe {
    /// Gen 1: parallel arrays starting at `wChannelCommandPointers`.
    Gen1 { command_pointers: u16 },
    /// Gen 2: one `channel_struct` per channel starting at `wChannel1`.
    Gen2 { channel1: u16 },
}

/// The position of one music channel in its score: everything that decides which commands it
/// reads next. Tempo remainders and similar timing state are deliberately left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelKey {
    pub address: u16,
    pub return_address: u16,
    pub loop_count: u8,
    pub flags: u8,
}

impl ChannelProbe {
    /// Read the score position of the four music channels; `None` for an inactive channel.
    pub fn read(&self, m: &mut Machine) -> [Option<ChannelKey>; 4] {
        let mut keys = [None; 4];
        for (c, key) in keys.iter_mut().enumerate() {
            let c16 = c as u16;
            *key = match *self {
                ChannelProbe::Gen1 {
                    command_pointers: p,
                } => {
                    // wChannelSoundIDs, wChannelFlags1 and wChannelLoopCounters follow the
                    // command pointers and return addresses as arrays of eight bytes.
                    let at = |offset: u16| p.wrapping_add(offset);
                    (m.read(at(32 + c16)) != 0).then(|| ChannelKey {
                        address: m.read16(at(2 * c16)),
                        return_address: m.read16(at(16 + 2 * c16)),
                        loop_count: m.read(at(32 + 8 * 19 + c16)),
                        // Only the "inside sound_call" bit; the others change mid-note.
                        flags: m.read(at(32 + 8 + c16)) & 0x02,
                    })
                }
                ChannelProbe::Gen2 { channel1 } => {
                    // channel_struct: MusicAddress +$06, LastMusicAddress +$08, LoopCount +$18.
                    let at = |offset: u16| channel1.wrapping_add(0x32 * c16 + offset);
                    let flags = m.read(at(3));
                    (flags & 1 != 0).then(|| ChannelKey {
                        address: m.read16(at(6)),
                        return_address: m.read16(at(8)),
                        loop_count: m.read(at(0x18)),
                        flags: flags & 0x06,
                    })
                }
            };
        }
        keys
    }

    /// Frames left on each channel's current note. A rise means a new note was read, which
    /// catches a channel whose loop is a single note and whose position therefore never
    /// changes between frames.
    pub fn note_timers(&self, m: &mut Machine) -> [u8; 4] {
        let mut timers = [0; 4];
        for (c, t) in timers.iter_mut().enumerate() {
            let c16 = c as u16;
            *t = match *self {
                // wChannelNoteDelayCounters
                ChannelProbe::Gen1 {
                    command_pointers: p,
                } => m.read(p.wrapping_add(32 + 8 * 18 + c16)),
                // channel_struct NoteDuration
                ChannelProbe::Gen2 { channel1 } => m.read(channel1.wrapping_add(0x32 * c16 + 0x15)),
            };
        }
        timers
    }

    /// Check right after starting `song` that the probe points at real channel state.
    pub fn validate(&self, m: &mut Machine, song: &Song) -> bool {
        match *self {
            ChannelProbe::Gen1 {
                command_pointers: p,
            } => {
                let ids: Vec<u8> = (0..4).map(|c| m.read(p.wrapping_add(32 + c))).collect();
                ids.contains(&(song.id as u8)) && ids.iter().all(|&i| i == 0 || i == song.id as u8)
            }
            ChannelProbe::Gen2 { channel1 } => (0..4).any(|c| {
                let base = channel1.wrapping_add(0x32 * c);
                m.read(base.wrapping_add(3)) & 1 != 0 && m.read16(base) == song.id
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Driver {
    pub engine: Engine,
    pub probe: Option<ChannelProbe>,
    /// Song database series and game the ROM was matched to.
    pub series: &'static str,
    pub game_id: String,
    pub songs: Vec<Song>,
    /// First bank of the audio engine, used to reset the machine.
    home_bank: u8,
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
        let title = Header::parse(rom).map(|h| h.title).unwrap_or_default();
        detect_gen2(rom, &title).or_else(|| detect_gen1(rom, &title))
    }

    /// Reset the sound engine and start `song`.
    pub fn play(&self, m: &mut Machine, song: &Song) -> Result<(), CallError> {
        match self.engine {
            Engine::Gen1 {
                play_music,
                loaded_rom_bank,
                ..
            }
            | Engine::Gen1Yellow {
                play_music,
                loaded_rom_bank,
                ..
            } => {
                m.write(0xFF26, 0x80);
                m.write(0xFF24, 0x77);
                m.write(0xFF25, 0xFF);
                m.write(loaded_rom_bank, self.home_bank);
                m.switch_rom_bank(self.home_bank);
                // Sound ID $FF stops everything, like the game's StopAllSounds.
                m.cpu.a = 0xFF;
                m.cpu.c = self.home_bank;
                m.call(play_music, CALL_BUDGET)?;
                m.cpu.a = song.id as u8;
                m.cpu.c = song.bank;
                m.call(play_music, CALL_BUDGET)?;
            }
            Engine::Gen2 { play_music, .. } => {
                m.write(0xFF26, 0x80);
                m.switch_rom_bank(1);
                m.cpu.set_de(0);
                m.call(play_music, CALL_BUDGET)?;
                m.cpu.set_de(song.id);
                m.call(play_music, CALL_BUDGET)?;
            }
        }
        Ok(())
    }

    /// Run the per-frame update the game normally calls from its VBlank handler.
    /// Returns the T-cycles it took.
    pub fn update(&self, m: &mut Machine) -> Result<u32, CallError> {
        match self.engine {
            Engine::Gen1 {
                audio_rom_bank,
                loaded_rom_bank,
                banks,
                updates,
                ..
            } => {
                let bank = m.read(audio_rom_bank);
                m.write(loaded_rom_bank, bank);
                m.switch_rom_bank(bank);
                let routine = if bank == banks[0] {
                    updates[0]
                } else if bank == banks[1] {
                    updates[1]
                } else {
                    updates[2]
                };
                m.call(routine, CALL_BUDGET)
            }
            Engine::Gen1Yellow {
                loaded_rom_bank,
                bank,
                update,
                ..
            } => {
                m.write(loaded_rom_bank, bank);
                m.switch_rom_bank(bank);
                m.call(update, CALL_BUDGET)
            }
            Engine::Gen2 { update_sound, .. } => m.call(update_sound, CALL_BUDGET),
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

fn detect_gen1(rom: &[u8], title: &str) -> Option<Driver> {
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
    // Pick the song mapping by header title, or by engine for unknown titles.
    let series = songdb::series(songdb::GEN1);
    let game = series.game_for_title(title).or_else(|| {
        let yellow = matches!(engine, Engine::Gen1Yellow { .. });
        series.game(if yellow { "yellow" } else { "red-green-blue" })
    });

    let mut songs = Vec::new();
    for (ordinal, (bank, groups)) in audio_banks.iter().enumerate() {
        let ordinal = ordinal as u8 + 1;
        let music: Vec<u8> = groups.iter().filter(|g| g.2 < 4).map(|g| g.0).collect();
        let known: Vec<&SongDef> = game
            .iter()
            .flat_map(|g| &g.songs)
            .filter(|s| s.bank == Some(ordinal))
            .collect();
        for (n, &id) in music.iter().enumerate() {
            // Match songs by sound ID; if a release shifted IDs but kept the song order,
            // fall back to the position within the bank.
            let def = known
                .iter()
                .find(|k| k.id == id as u16)
                .or_else(|| (known.len() == music.len()).then(|| &known[n]))
                .copied();
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
        .map(|command_pointers| ChannelProbe::Gen1 { command_pointers });
    Some(Driver {
        engine,
        probe,
        series: songdb::GEN1,
        game_id: game_id(game),
        songs,
        home_bank,
    })
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

fn detect_gen2(rom: &[u8], title: &str) -> Option<Driver> {
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
    let probe = music_id
        .checked_sub(8 * 0x32 + 12)
        .map(|channel1| ChannelProbe::Gen2 { channel1 });
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

    // Pick the song mapping by header title; for unknown titles Crystal is told apart from
    // Gold / Silver by its longer song table.
    let series = songdb::series(songdb::GEN2);
    let game = series.game_for_title(title).or_else(|| {
        let gold_silver = series.game("gold-silver")?;
        let crystal = ids.len() - 1 > gold_silver.songs.len();
        series.game(if crystal { "crystal" } else { "gold-silver" })
    });
    let mut songs = Vec::new();
    // ID 0 is "no music".
    for &id in ids.iter().skip(1) {
        let def = game.and_then(|g| g.songs.iter().find(|s| s.id == id));
        if def.is_some_and(|d| d.hidden) {
            continue;
        }
        songs.push(Song::new(songs.len(), id, 0, def));
    }
    Some(Driver {
        engine: Engine::Gen2 {
            play_music: pm as u16,
            update_sound: update_sound as u16,
        },
        probe,
        series: songdb::GEN2,
        game_id: game_id(game),
        songs,
        home_bank: bank,
    })
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
