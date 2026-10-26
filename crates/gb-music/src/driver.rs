//! Sound drivers: how to start a song and advance it by one frame for each supported engine.
//!
//! Instead of embedding per-version addresses, each driver finds the game's own audio entry
//! points by searching for the machine code that surrounds them. The code is taken from the
//! pret disassemblies (pokered, pokeyellow, pokegold, pokecrystal); the same engine code is
//! shared by the Japanese and international releases, only at different addresses.

use gb_core::{CallError, Machine};

use crate::names;
use crate::pattern::{word, Pattern};

/// Upper bound for one call into the sound engine (about 10 frames of CPU time).
const CALL_BUDGET: u32 = 700_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    /// Position in [`crate::Player::songs`].
    pub index: usize,
    pub title: String,
    /// The value the game passes to its play routine.
    pub id: u16,
    /// ROM bank whose engine plays the song (Gen 1 only; 0 otherwise).
    pub bank: u8,
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
                    let arrays = p + 32;
                    (m.read(arrays + c16) != 0).then(|| ChannelKey {
                        address: m.read16(p + 2 * c16),
                        return_address: m.read16(p + 16 + 2 * c16),
                        loop_count: m.read(arrays + 8 * 19 + c16),
                        // Only the "inside sound_call" bit; the others change mid-note.
                        flags: m.read(arrays + 8 + c16) & 0x02,
                    })
                }
                ChannelProbe::Gen2 { channel1 } => {
                    let base = channel1 + 0x32 * c16;
                    let flags = m.read(base + 3);
                    (flags & 1 != 0).then(|| ChannelKey {
                        address: m.read16(base + 6),
                        return_address: m.read16(base + 8),
                        loop_count: m.read(base + 0x16),
                        flags: flags & 0x06,
                    })
                }
            };
        }
        keys
    }

    /// Check right after starting `song` that the probe points at real channel state.
    pub fn validate(&self, m: &mut Machine, song: &Song) -> bool {
        match *self {
            ChannelProbe::Gen1 {
                command_pointers: p,
            } => {
                let ids: Vec<u8> = (0..4).map(|c| m.read(p + 32 + c)).collect();
                ids.contains(&(song.id as u8)) && ids.iter().all(|&i| i == 0 || i == song.id as u8)
            }
            ChannelProbe::Gen2 { channel1 } => (0..4).any(|c| {
                let base = channel1 + 0x32 * c;
                m.read(base + 3) & 1 != 0 && m.read16(base) == song.id
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Driver {
    pub engine: Engine,
    pub probe: Option<ChannelProbe>,
    pub game: &'static str,
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
    pub fn detect(rom: &[u8]) -> Option<Driver> {
        detect_gen2(rom).or_else(|| detect_gen1(rom))
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
        if (0x4000..0x8000).contains(&base) {
            return Some(&bank[base as usize - 0x4000..]);
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

fn detect_gen1(rom: &[u8]) -> Option<Driver> {
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
    let yellow = matches!(engine, Engine::Gen1Yellow { .. });
    let table: &[(u8, u8, &str)] = if yellow {
        names::YELLOW
    } else {
        names::RED_BLUE
    };

    let mut songs = Vec::new();
    for (ordinal, (bank, groups)) in audio_banks.iter().enumerate() {
        let ordinal = ordinal as u8 + 1;
        let music: Vec<u8> = groups.iter().filter(|g| g.2 < 4).map(|g| g.0).collect();
        let known: Vec<&(u8, u8, &str)> = table.iter().filter(|t| t.0 == ordinal).collect();
        for (n, &id) in music.iter().enumerate() {
            // Match titles by sound ID; if a release shifted IDs but kept the song order,
            // fall back to the position within the bank.
            let title = known
                .iter()
                .find(|k| k.1 == id)
                .or_else(|| (known.len() == music.len()).then(|| &known[n]))
                .map(|k| k.2.to_string())
                .unwrap_or_else(|| format!("Song ${id:02X}"));
            songs.push(Song {
                index: songs.len(),
                title,
                id: id as u16,
                bank: *bank,
            });
        }
    }
    let home_bank = audio_banks[0].0;
    // In the Gen 1 audio RAM layout wNewSoundID directly precedes wAudioROMBank, and the
    // channel arrays end 233 bytes earlier. Only trust that layout if the first part holds.
    let new_sound_id = word(home, pm + 2);
    let probe = (new_sound_id + 1 == audio_rom_bank).then(|| ChannelProbe::Gen1 {
        command_pointers: audio_rom_bank - 233,
    });
    let game = if yellow {
        "Pokémon Yellow"
    } else {
        "Pokémon Red / Green / Blue"
    };
    Some(Driver {
        engine,
        probe,
        game,
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
        let skip = home[pos + 12] as usize;
        let mut p = pos + 13;
        // Engine 2 may first call Music_DoLowHealthAlarm.
        let calls_after = if skip == 8 { 2 } else { 1 };
        let mut last_call = 0;
        let mut ok = true;
        for _ in 0..calls_after {
            if home.get(p) != Some(&0xCD) {
                ok = false;
                break;
            }
            last_call = word(home, p + 1);
            p += 3;
        }
        if !ok || home.get(p..p + 3) != Some(&[0x18, 0x03, 0xCD][..]) {
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

fn detect_gen2(rom: &[u8]) -> Option<Driver> {
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

    let crystal = ids.len() > names::GOLD_SILVER.len();
    let (titles, game) = if crystal {
        (names::CRYSTAL, "Pokémon Crystal")
    } else {
        (names::GOLD_SILVER, "Pokémon Gold / Silver")
    };
    let songs = ids
        .iter()
        .skip(1) // ID 0 is "no music"
        .enumerate()
        .map(|(index, &id)| Song {
            index,
            title: titles
                .get(id as usize)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Song ${id:02X}")),
            id,
            bank: 0,
        })
        .collect();
    Some(Driver {
        engine: Engine::Gen2 {
            play_music: pm as u16,
            update_sound: update_sound as u16,
        },
        probe,
        game,
        songs,
        home_bank: bank,
    })
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
