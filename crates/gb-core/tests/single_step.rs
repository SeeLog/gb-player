//! Runs the SingleStepTests SM83 suite (https://github.com/SingleStepTests/sm83) when
//! `SM83_TESTS_DIR` points to a directory holding its v1 JSON files.

use std::collections::HashMap;

use gb_core::{Bus, Cpu};
use serde_json::Value;

struct FlatRam(HashMap<u16, u8>);

impl Bus for FlatRam {
    fn read(&mut self, addr: u16) -> u8 {
        *self.0.get(&addr).unwrap_or(&0)
    }
    fn write(&mut self, addr: u16, value: u8) {
        self.0.insert(addr, value);
    }
}

fn u(v: &Value, k: &str) -> u16 {
    v[k].as_u64().unwrap() as u16
}

#[test]
fn sm83_single_step_tests() {
    let Ok(dir) = std::env::var("SM83_TESTS_DIR") else {
        eprintln!("SM83_TESTS_DIR not set; skipping");
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    let mut failures = Vec::new();
    for path in files {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        // HALT and STOP depend on interrupt hardware that the suite does not model the same way.
        if name == "76" || name == "10" {
            continue;
        }
        let tests: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for t in &tests {
            let i = &t["initial"];
            let mut cpu = Cpu::default();
            cpu.a = u(i, "a") as u8;
            cpu.f = u(i, "f") as u8;
            cpu.b = u(i, "b") as u8;
            cpu.c = u(i, "c") as u8;
            cpu.d = u(i, "d") as u8;
            cpu.e = u(i, "e") as u8;
            cpu.h = u(i, "h") as u8;
            cpu.l = u(i, "l") as u8;
            cpu.sp = u(i, "sp");
            cpu.pc = u(i, "pc");
            cpu.ime = u(i, "ime") != 0;
            let mut ram = FlatRam(HashMap::new());
            for e in i["ram"].as_array().unwrap() {
                ram.0
                    .insert(e[0].as_u64().unwrap() as u16, e[1].as_u64().unwrap() as u8);
            }
            let cycles = cpu.step(&mut ram);
            let f = &t["final"];
            let got = [cpu.a, cpu.f, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l];
            let want = ["a", "f", "b", "c", "d", "e", "h", "l"].map(|k| u(f, k) as u8);
            let mut ok = got == want && cpu.sp == u(f, "sp") && cpu.pc == u(f, "pc");
            for e in f["ram"].as_array().unwrap() {
                ok &= ram.read(e[0].as_u64().unwrap() as u16) == e[1].as_u64().unwrap() as u8;
            }
            let want_cycles = t["cycles"].as_array().unwrap().len() as u32 * 4;
            if !ok || cycles != want_cycles {
                failures.push(format!(
                    "{}: regs {:02X?} want {:02X?} sp {:04X}/{:04X} pc {:04X}/{:04X} cycles {}/{}",
                    t["name"],
                    got,
                    want,
                    cpu.sp,
                    u(f, "sp"),
                    cpu.pc,
                    u(f, "pc"),
                    cycles,
                    want_cycles
                ));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} opcode(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
