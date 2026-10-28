//! End-to-end tests on tiny hand-assembled ROMs that contain the same entry-point code shapes
//! as the real games, with a toy sound engine behind them.

use gb_music::{analyze, songdb, Driver, Engine, PlayState, Player, SongLength};

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
        player.driver().unwrap().engine,
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
    let len = analyze(&rom, player.driver().unwrap(), &song, 2000).unwrap();
    assert!(
        matches!(len, SongLength::Loops { period: 256, .. }),
        "{len:?}"
    );
}

#[test]
fn gen1_detect_play_and_loop() {
    let rom = gen1_rom();
    let mut player = Player::new(rom.clone(), 48_000).expect("driver detected");
    match player.driver().unwrap().engine {
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
    let len = analyze(&rom, player.driver().unwrap(), &song, 2000).unwrap();
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

#[test]
fn huge_lengths_do_not_overflow() {
    let mut player = Player::new(gen2_rom(), 48_000).unwrap();
    player.play(0);
    player.loop_count = Some(u32::MAX);
    player.set_length(
        0,
        SongLength::Loops {
            intro: u32::MAX,
            period: u32::MAX,
        },
    );
    peak(&mut player, 0.2);
    player.play(0);
    player.set_length(0, SongLength::Ends { at: u32::MAX });
    peak(&mut player, 0.2);
    assert_eq!(player.state(), PlayState::Playing);
}

#[test]
fn short_or_garbage_input_is_not_detected() {
    for len in [0, 1, 0x100, 0x3FFF, 0x4000, 0x7FFF] {
        assert!(Driver::detect(&vec![0xCD; len]).is_none(), "{len}");
    }
}

/// A ROM with a toy engine that no detector knows: song start at $4000 and update at $4100
/// of bank 1.
fn configured_rom() -> Vec<u8> {
    let mut rom = Rom::new(0x01, 4);
    // ld [$C000],a / ld a,1 / ld [$C002],a / ret
    rom.put(
        1,
        0x4000,
        &[0xEA, 0x00, 0xC0, 0x3E, 0x01, 0xEA, 0x02, 0xC0, 0xC9],
    );
    rom.put(1, 0x4100, &toy_update(0xC002, 0xC003));
    rom.0
}

const CONFIGURED: &str = r#"
[[game]]
id = "toy"
titles = ["SYNTHGB"]
songs = [{ id = 3, key = "three" }, { id = 9, key = "nine", hidden = true }]
song_range = { first = 1, last = 4 }

[game.playback]
setup = [
    { write = 0xFF26, value = 0x80 },
    { write = 0xFF24, value = 0x77 },
    { write = 0xFF25, value = 0xFF },
]
start = [{ bank = 1, call = 0x4000, a = "song" }]
update = [{ bank = 1, call = 0x4100 }]
"#;

#[test]
fn a_song_database_entry_drives_an_unknown_engine() {
    let rom = configured_rom();
    assert!(Driver::detect(&rom).is_none());
    let series: songdb::Series = toml::from_str(CONFIGURED).unwrap();
    let game = &series.games[0];
    game.validate().unwrap();
    let driver = Driver::from_game("toy-series", game).unwrap();
    assert_eq!(
        driver.songs.iter().map(|s| s.id).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(driver.songs[2].key.as_deref(), Some("three"));
    assert_eq!(driver.engine, Engine::Configured);

    let mut player = Player::with_driver(rom.clone(), 48_000, driver.clone()).unwrap();
    player.play(2);
    assert!(peak(&mut player, 1.0) > 0.05);
    assert_eq!(player.state(), PlayState::Playing);
    let song = driver.songs[2].clone();
    let len = analyze(&rom, &driver, &song, 2000).unwrap();
    assert!(
        matches!(len, SongLength::Loops { period: 256, .. }),
        "{len:?}"
    );
}

#[test]
fn hostile_playbacks_stay_inside_their_budget() {
    // Calls into an endless loop, through every kind of step.
    let mut rom = Rom::new(0x01, 4);
    rom.put(1, 0x4000, &[0x18, 0xFE]); // jr -2
    let series: songdb::Series = toml::from_str(
        r#"
        [[game]]
        id = "loop"
        titles = ["SYNTHGB"]
        song_range = { first = 0, last = 0 }
        [game.playback]
        setup = [{ frames = 300 }]
        start = [{ bank = 1, call = 0x4000 }]
        update = [{ bank = 1, call = 0x4000 }, { bank = 1, call = 0x4000 }]
        "#,
    )
    .unwrap();
    let driver = Driver::from_game("loop", &series.games[0]).unwrap();
    let mut player = Player::with_driver(rom.0.clone(), 48_000, driver.clone()).unwrap();
    player.play(0);
    assert!(matches!(player.state(), PlayState::Failed(_)));
    let song = driver.songs[0].clone();
    assert!(analyze(&rom.0, &driver, &song, 100).is_err());
}

/// A GBS file of the toy engine: init at $0400 (song number in A), play at $0410.
fn toy_gbs(tma: u8, tac: u8) -> Vec<u8> {
    let mut gbs = vec![0u8; 0x70];
    gbs[0..4].copy_from_slice(b"GBS\x01");
    gbs[4] = 2;
    gbs[5] = 1;
    for (pos, v) in [(6, 0x0400u16), (8, 0x0400), (10, 0x0410), (12, 0xDFFF)] {
        gbs[pos..pos + 2].copy_from_slice(&v.to_le_bytes());
    }
    gbs[0x0E] = tma;
    gbs[0x0F] = tac;
    gbs[0x10..0x18].copy_from_slice(b"Toy Tune");
    let mut code = vec![0xEA, 0x00, 0xC0, 0x3E, 0x01, 0xEA, 0x02, 0xC0, 0xC9];
    code.resize(0x10, 0);
    code.extend(toy_update(0xC002, 0xC003));
    gbs.extend(code);
    gbs
}

#[test]
fn gbs_files_play_and_loop() {
    for (tma, tac, period) in [(0, 0, 256), (0xBB, 0x04, 256)] {
        let gbs = toy_gbs(tma, tac);
        let mut player = Player::new(gbs, 48_000).expect("GBS loaded");
        assert_eq!(player.songs().len(), 2);
        assert_eq!(player.driver().unwrap().game_id, "Toy Tune");
        player.play(1);
        assert!(peak(&mut player, 1.0) > 0.05);
        assert_eq!(player.state(), PlayState::Playing);
        let song = player.songs()[1].clone();
        let len = analyze(
            &player.rom().unwrap(),
            player.driver().unwrap(),
            &song,
            4000,
        )
        .unwrap();
        // 4096 Hz / (256 - $BB) is about 59.4 Hz, so with the timer the toy engine's
        // 256-update loop takes a little longer than 256 frames.
        match len {
            SongLength::Loops { period: p, .. } => {
                assert!((period..period + 4).contains(&p), "{p}")
            }
            other => panic!("TAC {tac:02X}: {other:?}"),
        }
    }
}

#[test]
fn broken_gbs_files_are_refused() {
    let mut gbs = toy_gbs(0, 0);
    gbs[6..8].copy_from_slice(&0x0100u16.to_le_bytes());
    assert!(Player::new(gbs, 48_000).is_err());
    assert!(Player::new(b"GBS".to_vec(), 48_000).is_err());
}
