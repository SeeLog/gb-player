//! End-to-end tests on tiny hand-assembled ROMs that contain the same entry-point code shapes
//! as the real games, with a toy sound engine behind them.

use gb_music::{analyze, Engine, PlayState, Player, SongLength};

struct Rom(Vec<u8>);

impl Rom {
    fn new(cart_type: u8, banks: usize) -> Rom {
        let mut rom = vec![0u8; banks * 0x4000];
        rom[0x134..0x13B].copy_from_slice(b"SYNTHGB");
        rom[0x147] = cart_type;
        Rom(rom)
    }

    fn put(&mut self, bank: usize, addr: u16, bytes: &[u8]) {
        let off = if addr < 0x4000 {
            addr as usize
        } else {
            bank * 0x4000 + addr as usize - 0x4000
        };
        self.0[off..off + bytes.len()].copy_from_slice(bytes);
    }
}

fn lo(a: u16) -> u8 {
    a as u8
}
fn hi(a: u16) -> u8 {
    (a >> 8) as u8
}

/// Toy per-frame update: does nothing until a song is selected ([`flag`] != 0), then
/// counts frames in `counter` and triggers a note on channel 1 every 32 frames.
fn toy_update(flag: u16, counter: u16) -> Vec<u8> {
    vec![
        0xFA,
        lo(flag),
        hi(flag), // ld a,[flag]
        0xA7,     // and a
        0xC8,     // ret z
        0xFA,
        lo(counter),
        hi(counter), // ld a,[counter]
        0x3C,        // inc a
        0xEA,
        lo(counter),
        hi(counter), // ld [counter],a
        0xE6,
        0x1F, // and $1F
        0xC0, // ret nz
        0x3E,
        0xF0,
        0xE0,
        0x12, // NR12 = $F0
        0x3E,
        0x00,
        0xE0,
        0x13, // NR13 = 0
        0x3E,
        0x87,
        0xE0,
        0x14, // NR14 = $87 (trigger)
        0xC9,
    ]
}

fn wrapper(bank: u8, target: u16) -> Vec<u8> {
    vec![
        0xE5,
        0xD5,
        0xC5,
        0xF5,
        0xF0,
        0x9F,
        0xF5,
        0x3E,
        bank,
        0xE0,
        0x9F,
        0xEA,
        0x00,
        0x20,
        0xCD,
        lo(target),
        hi(target),
        0xF1,
        0xE0,
        0x9F,
        0xEA,
        0x00,
        0x20,
        0xF1,
        0xC1,
        0xD1,
        0xE1,
        0xC9,
    ]
}

fn gen2_rom() -> Vec<u8> {
    let mut rom = Rom::new(0x10, 4);
    let (init, play, update, table) = (0x4000u16, 0x4100u16, 0x4200u16, 0x5000u16);
    rom.put(0, 0x0200, &wrapper(2, init));
    rom.put(0, 0x0240, &wrapper(2, update));
    let mut pm = vec![
        0xE5, 0xD5, 0xC5, 0xF5, 0xF0, 0x9F, 0xF5, 0x3E, 0x02, 0xE0, 0x9F, 0xEA, 0x00, 0x20,
    ];
    pm.extend([
        0x7B,
        0xA7,
        0x28,
        0x05,
        0xCD,
        lo(play),
        hi(play),
        0x18,
        0x03,
        0xCD,
        lo(init),
        hi(init),
    ]);
    pm.extend([
        0xF1, 0xE0, 0x9F, 0xEA, 0x00, 0x20, 0xF1, 0xC1, 0xD1, 0xE1, 0xC9,
    ]);
    rom.put(0, 0x0280, &pm);
    // _InitSound: power the APU, set volume/panning, clear the playing flag.
    rom.put(
        2,
        init,
        &[
            0x3E, 0x80, 0xE0, 0x26, 0x3E, 0x77, 0xE0, 0x24, 0x3E, 0xFF, 0xE0, 0x25, 0xAF, 0xEA,
            0x02, 0xC0, 0xC9,
        ],
    );
    // _PlayMusic: call MusicOff / ld hl,wMusicID / ld [hl],e / inc hl / ld [hl],d / ld hl,Music
    rom.put(
        2,
        play,
        &[
            0xCD,
            lo(init),
            hi(init),
            0x21,
            0x00,
            0xC0,
            0x73,
            0x23,
            0x72,
            0x21,
            lo(table),
            hi(table),
        ],
    );
    rom.put(2, play + 12, &[0x3E, 0x01, 0xEA, 0x02, 0xC0, 0xC9]);
    rom.put(2, update, &toy_update(0xC002, 0xC003));
    // Music table: three valid entries, then garbage.
    rom.put(
        2,
        table,
        &[
            0x02, 0x00, 0x60, 0x02, 0x10, 0x60, 0x02, 0x20, 0x60, 0xFF, 0xFF, 0xFF,
        ],
    );
    rom.put(2, 0x6000, &[0xC0]);
    rom.put(2, 0x6010, &[0x80]);
    rom.put(2, 0x6020, &[0x40]);
    rom.0
}

fn gen1_rom() -> Vec<u8> {
    let mut rom = Rom::new(0x13, 4);
    let (new_id, fade, audio_bank, saved_bank, sound) =
        (0xC001u16, 0xC002u16, 0xC003u16, 0xC004u16, 0xC010u16);
    // PlayMusic, falling through into a toy PlaySound that just records the ID.
    rom.put(
        0,
        0x0300,
        &[
            0x47,
            0xEA,
            lo(new_id),
            hi(new_id),
            0xAF,
            0xEA,
            lo(fade),
            hi(fade),
            0x79,
            0xEA,
            lo(audio_bank),
            hi(audio_bank),
            0xEA,
            lo(saved_bank),
            hi(saved_bank),
            0x78,
            0xEA,
            lo(sound),
            hi(sound),
            0xC9,
        ],
    );
    // VBlank dispatch with the low-health-alarm call before engine 2.
    let u = 0x4200u16;
    rom.put(
        0,
        0x0400,
        &[
            0xFA,
            lo(audio_bank),
            hi(audio_bank),
            0xE0,
            0xB8,
            0xEA,
            0x00,
            0x20,
            0xFE,
            0x02,
            0x20,
            0x05,
            0xCD,
            lo(u),
            hi(u),
            0x18,
            0x0B,
            0xFE,
            0x03,
            0x20,
            0x08,
            0xCD,
            0x00,
            0x41,
            0xCD,
            lo(u),
            hi(u),
            0x18,
            0x03,
            0xCD,
            lo(u),
            hi(u),
            0xC9,
        ],
    );
    for bank in [2usize, 3] {
        let mut table = vec![0xFF, 0xFF, 0xFF];
        for _ in 0..19 {
            table.extend([0x07, 0x00, 0x50]);
        }
        table.extend([0x80, 0x00, 0x51, 0x01, 0x00, 0x52, 0x02, 0x00, 0x53]);
        rom.put(bank, 0x4000, &table);
        let mut code = vec![
            0x3E, 0x80, 0xE0, 0x26, 0x3E, 0x77, 0xE0, 0x24, 0x3E, 0xFF, 0xE0, 0x25,
        ];
        code.extend(toy_update(sound, 0xC011));
        rom.put(bank, u, &code);
    }
    rom.0
}

fn peak(player: &mut Player, secs: f32) -> f32 {
    let mut buf = vec![0.0f32; (48_000.0 * secs) as usize * 2];
    player.render(&mut buf);
    buf.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

#[test]
fn gen2_detect_play_and_loop() {
    let rom = gen2_rom();
    let mut player = Player::new(rom.clone(), 48_000).expect("driver detected");
    assert!(matches!(
        player.driver().engine,
        Engine::Gen2 {
            play_music: 0x0280,
            update_sound: 0x0240
        }
    ));
    assert_eq!(player.songs().len(), 2);
    assert_eq!(player.songs()[0].id, 1);

    player.play(0);
    assert_eq!(player.state(), PlayState::Playing);
    assert!(peak(&mut player, 1.0) > 0.05);

    let song = player.songs()[0].clone();
    let len = analyze(&rom, player.driver(), &song, 2000).unwrap();
    assert!(
        matches!(len, SongLength::Loops { period: 256, .. }),
        "{len:?}"
    );
}

#[test]
fn gen1_detect_play_and_loop() {
    let rom = gen1_rom();
    let mut player = Player::new(rom.clone(), 48_000).expect("driver detected");
    match player.driver().engine {
        Engine::Gen1 {
            play_music,
            audio_rom_bank,
            loaded_rom_bank,
            banks,
            updates,
        } => {
            assert_eq!(play_music, 0x0300);
            assert_eq!(audio_rom_bank, 0xC003);
            assert_eq!(loaded_rom_bank, 0xFFB8);
            assert_eq!(banks, [2, 3]);
            assert_eq!(updates, [0x4200; 3]);
        }
        e => panic!("unexpected engine {e:?}"),
    }
    let songs: Vec<_> = player.songs().iter().map(|s| (s.bank, s.id)).collect();
    assert_eq!(songs, vec![(2, 0x14), (3, 0x14)]);

    player.play(1);
    assert_eq!(player.state(), PlayState::Playing);
    assert!(peak(&mut player, 1.0) > 0.05);

    let song = player.songs()[1].clone();
    let len = analyze(&rom, player.driver(), &song, 2000).unwrap();
    assert!(
        matches!(len, SongLength::Loops { period: 256, .. }),
        "{len:?}"
    );
}

#[test]
fn loop_count_fades_and_finishes() {
    let rom = gen2_rom();
    let mut player = Player::new(rom, 48_000).unwrap();
    player.play(0);
    player.loop_count = Some(1);
    player.fade_secs = 1.0;
    player.set_length(
        0,
        SongLength::Loops {
            intro: 32,
            period: 64,
        },
    );
    // 96 frames of music plus a one-second fade is about 2.6 s.
    peak(&mut player, 3.0);
    assert_eq!(player.state(), PlayState::Finished);
}

#[test]
fn rejects_unknown_rom() {
    let rom = Rom::new(0x00, 2).0;
    assert!(Player::new(rom, 48_000).is_err());
}
