//! Property tests with hostile input: random ROM bytes, random song database entries and
//! random GBS files must never make the player panic or run without end. Each case runs
//! the emulated machine for a bounded number of frames; the cycle budgets keep every frame
//! short however the code behaves.

use gb_music::{analyze, songdb, Driver, Player};
use proptest::prelude::*;

/// Code shapes the detectors look for, with `??` as wildcards. Planting them into random
/// bytes (also right at the end of a bank) drives the detectors past their first checks.
const PATTERNS: &[&str] = &[
    "47 EA ?? ?? AF EA ?? ?? 79 EA ?? ?? EA ?? ?? 78",
    "FE ?? 20 05 CD ?? ?? 18 ?? FE ?? 20",
    "3E ?? CD ?? ?? CD ?? ?? 3E ?? CD ?? ?? CD ?? ??",
    "6F 5F 26 00 54 29 19 11 ?? ?? 19",
    "E5 D5 C5 F5 F0 ?? F5 3E ?? E0 ?? EA 00 20 7B A7 28 05 CD ?? ?? 18 03 CD ?? ??",
    "E5 D5 C5 F5 F0 ?? F5 3E ?? E0 ?? EA 00 20 CD ?? ?? F1 E0 ?? EA 00 20 F1 C1 D1 E1 C9",
    "CD ?? ?? 21 ?? ?? 73 23 72 21 ?? ??",
];

fn plant(rom: &mut [u8], pattern: &str, pos: usize, fill: u8) {
    for (i, t) in pattern.split_whitespace().enumerate() {
        if let Some(b) = rom.get_mut(pos + i) {
            *b = u8::from_str_radix(t, 16).unwrap_or(fill.wrapping_add(i as u8));
        }
    }
}

/// Random bytes with a cartridge header the emulator accepts, and detector patterns planted
/// at random places.
fn rom() -> impl Strategy<Value = Vec<u8>> {
    (
        2usize..=4,
        prop::collection::vec(any::<u8>(), 0x200),
        prop::sample::select(vec![0x00u8, 0x01, 0x03, 0x05, 0x13, 0x19]),
        prop::collection::vec((0..PATTERNS.len(), 0usize..0x10000, any::<u8>()), 0..12),
    )
        .prop_map(|(banks, seed, cart_type, planted)| {
            let mut rom: Vec<u8> = seed.iter().copied().cycle().take(banks * 0x4000).collect();
            for (i, b) in rom.iter_mut().enumerate() {
                *b ^= (i / 0x200) as u8;
            }
            rom[0x134..0x13B].copy_from_slice(b"HOSTILE");
            rom[0x143] = 0;
            rom[0x147] = cart_type;
            for (p, pos, fill) in planted {
                let pos = pos % rom.len();
                plant(&mut rom, PATTERNS[p], pos, fill);
                // Also right against the end of the bank it landed in.
                let end = (pos / 0x4000 + 1) * 0x4000;
                plant(
                    &mut rom,
                    PATTERNS[p],
                    end.saturating_sub(1 + fill as usize % 8),
                    fill,
                );
            }
            rom
        })
}

fn value() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<u16>().prop_map(|v| v.to_string()),
        (0u16..0x100).prop_map(|v| v.to_string()),
        Just("\"song\"".to_string()),
        Just("\"song_bank\"".to_string()),
        any::<u16>().prop_map(|a| format!("{{ ram = {a} }}")),
        Just("\"bogus\"".to_string()),
    ]
}

fn addr() -> impl Strategy<Value = u16> {
    prop_oneof![any::<u16>(), 0u16..0x8000, 0xFE90u16..0xFF10]
}

fn step() -> impl Strategy<Value = String> {
    let reg = prop::sample::select(vec!["a", "b", "c", "d", "e", "h", "l", "bc", "de", "hl"]);
    prop_oneof![
        (addr(), value()).prop_map(|(a, v)| format!("{{ write = {a}, value = {v} }}")),
        value().prop_map(|v| format!("{{ bank = {v} }}")),
        (addr(), prop::collection::vec((reg, value()), 0..3)).prop_map(|(a, regs)| {
            let regs: String = regs.iter().map(|(r, v)| format!(", {r} = {v}")).collect();
            format!("{{ call = {a}{regs} }}")
        }),
        (
            addr(),
            prop::collection::vec((any::<u8>(), addr()), 0..4),
            addr()
        )
            .prop_map(|(ram, cases, default)| {
                let cases: Vec<String> = cases.iter().map(|(v, a)| format!("[{v}, {a}]")).collect();
                format!(
                    "{{ call_by = {ram}, cases = [{}], default = {default} }}",
                    cases.join(", ")
                )
            }),
        (0u16..3).prop_map(|n| format!("{{ frames = {n} }}")),
        (addr(), addr()).prop_map(|(a, b)| format!("{{ write = {a}, call = {b} }}")),
    ]
}

fn steps(max: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(step(), 0..max).prop_map(|s| format!("[{}]", s.join(", ")))
}

/// A song database entry for the "HOSTILE" ROMs, mostly well-formed but with random
/// addresses and values.
fn game() -> impl Strategy<Value = String> {
    (
        prop_oneof![
            Just("\"vblank\"".to_string()),
            (any::<u8>(), any::<u8>())
                .prop_map(|(tac, tma)| format!("{{ timer = {{ tac = {tac}, tma = {tma} }} }}")),
        ],
        steps(4),
        steps(4),
        steps(3),
        (0u16..4, 0u16..8),
        prop::option::of((addr(), any::<u16>(), any::<u8>())),
    )
        .prop_map(|(rate, setup, start, update, (first, len), probe)| {
            let mut text = format!(
                "[[game]]\nid = \"hostile\"\ntitles = [\"HOSTILE\"]\n\
                 song_range = {{ first = {first}, last = {} }}\n\
                 [game.playback]\nrate = {rate}\nsetup = {setup}\nstart = {start}\nupdate = {update}\n",
                first + len
            );
            if let Some((at, step, mask)) = probe {
                let f = format!("{{ at = {at}, step = {step}, mask = {mask} }}");
                let w = format!("{{ at = {at}, step = {step}, size = 2 }}");
                text += &format!(
                    "[game.probe]\nactive = {f}\naddress = {w}\nreturn = {w}\nloop = {f}\n\
                     flags = {f}\ntimer = {f}\ncheck = {{ song_id = {w}, all = true }}\n"
                );
            }
            text
        })
}

/// Play the first songs a little and analyse them briefly.
fn exercise(rom: Vec<u8>, driver: Driver) {
    let Ok(mut player) = Player::with_driver(rom.clone(), 8_000, driver.clone()) else {
        return;
    };
    let mut buf = vec![0.0f32; 1600];
    for index in 0..driver.songs.len().min(2) {
        player.play(index);
        player.loop_count = Some(1);
        player.render(&mut buf);
        let _ = analyze(&rom, &driver, &driver.songs[index], 8);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_bytes_are_never_detected_with_a_panic(bytes in prop::collection::vec(any::<u8>(), 0..0x9000)) {
        let _ = Driver::detect(&bytes);
        let _ = Player::new(bytes, 8_000);
    }

    #[test]
    fn hostile_roms_play_safely(rom in rom()) {
        if let Some(driver) = Driver::detect(&rom) {
            exercise(rom, driver);
        }
    }

    #[test]
    fn hostile_song_database_entries_play_safely(rom in rom(), text in game()) {
        let Ok(series) = toml::from_str::<songdb::Series>(&text) else {
            return Ok(());
        };
        for game in &series.games {
            if let Some(driver) = Driver::from_game("hostile", game) {
                exercise(rom.clone(), driver);
            }
        }
    }

    #[test]
    fn garbage_song_databases_do_not_panic(text in "[\\[\\]a-z_=.{},\" 0-9x\n]{0,400}") {
        if let Ok(series) = toml::from_str::<songdb::Series>(&text) {
            for game in &series.games {
                let _ = game.validate();
                let _ = Driver::from_game("garbage", game);
            }
        }
    }

    #[test]
    fn hostile_gbs_files_play_safely(
        header in prop::collection::vec(any::<u8>(), 0x70),
        body in prop::collection::vec(any::<u8>(), 0..0x800),
        load in 0x0400u16..0x7800,
        init in addr(),
        play in addr(),
        tac in any::<u8>(),
        count in 1u8..4,
    ) {
        let mut gbs = header;
        gbs[0..4].copy_from_slice(b"GBS\x01");
        gbs[4] = count;
        gbs[6..8].copy_from_slice(&load.to_le_bytes());
        gbs[8..10].copy_from_slice(&init.to_le_bytes());
        gbs[10..12].copy_from_slice(&play.to_le_bytes());
        gbs[0x0F] = tac;
        gbs.extend(body);
        if let Ok((image, driver)) = gb_music::gbs::load(&gbs) {
            exercise(image, driver);
        }
        let _ = Player::new(gbs, 8_000);
    }
}
