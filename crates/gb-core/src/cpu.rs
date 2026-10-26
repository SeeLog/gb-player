//! Sharp SM83 CPU core.
//!
//! Instructions execute atomically; [`Cpu::step`] returns the number of T-cycles taken so the
//! caller can advance the rest of the system.

pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);
}

const FLAG_Z: u8 = 0x80;
const FLAG_N: u8 = 0x40;
const FLAG_H: u8 = 0x20;
const FLAG_C: u8 = 0x10;

#[derive(Debug, Clone, Default)]
pub struct Cpu {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    pub ime: bool,
    ime_pending: bool,
    pub halted: bool,
    /// Set when an undefined opcode was executed; the CPU locks up like real hardware.
    pub locked: bool,
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu {
            a: 0x01,
            f: 0xB0,
            b: 0,
            c: 0x13,
            d: 0,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            sp: 0xFFFE,
            pc: 0x100,
            ..Default::default()
        }
    }

    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }
    pub fn set_bc(&mut self, v: u16) {
        [self.b, self.c] = v.to_be_bytes();
    }
    pub fn set_de(&mut self, v: u16) {
        [self.d, self.e] = v.to_be_bytes();
    }
    pub fn set_hl(&mut self, v: u16) {
        [self.h, self.l] = v.to_be_bytes();
    }
    pub fn set_af(&mut self, v: u16) {
        [self.a, self.f] = v.to_be_bytes();
        self.f &= 0xF0;
    }

    fn flag(&self, f: u8) -> bool {
        self.f & f != 0
    }

    fn set_flags(&mut self, z: bool, n: bool, h: bool, c: bool) {
        self.f = (z as u8) << 7 | (n as u8) << 6 | (h as u8) << 5 | (c as u8) << 4;
    }

    fn fetch<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    fn fetch16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch(bus);
        let hi = self.fetch(bus);
        u16::from_le_bytes([lo, hi])
    }

    pub fn push<B: Bus>(&mut self, bus: &mut B, v: u16) {
        let [hi, lo] = v.to_be_bytes();
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, hi);
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, lo);
    }

    fn pop<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = bus.read(self.sp);
        self.sp = self.sp.wrapping_add(1);
        let hi = bus.read(self.sp);
        self.sp = self.sp.wrapping_add(1);
        u16::from_le_bytes([lo, hi])
    }

    /// Read 8-bit register by its encoding index (B C D E H L (HL) A).
    fn reg<B: Bus>(&mut self, bus: &mut B, i: u8) -> u8 {
        match i {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => bus.read(self.hl()),
            _ => self.a,
        }
    }

    fn set_reg<B: Bus>(&mut self, bus: &mut B, i: u8, v: u8) {
        match i {
            0 => self.b = v,
            1 => self.c = v,
            2 => self.d = v,
            3 => self.e = v,
            4 => self.h = v,
            5 => self.l = v,
            6 => bus.write(self.hl(), v),
            _ => self.a = v,
        }
    }

    /// 16-bit register pair by index (BC DE HL SP).
    fn rp(&self, i: u8) -> u16 {
        match i {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.sp,
        }
    }

    fn set_rp(&mut self, i: u8, v: u16) {
        match i {
            0 => self.set_bc(v),
            1 => self.set_de(v),
            2 => self.set_hl(v),
            _ => self.sp = v,
        }
    }

    fn cond(&self, i: u8) -> bool {
        match i {
            0 => !self.flag(FLAG_Z),
            1 => self.flag(FLAG_Z),
            2 => !self.flag(FLAG_C),
            _ => self.flag(FLAG_C),
        }
    }

    fn alu(&mut self, op: u8, v: u8) {
        let a = self.a;
        match op {
            0 | 1 => {
                // ADD / ADC
                let carry = if op == 1 && self.flag(FLAG_C) { 1 } else { 0 };
                let r = a as u16 + v as u16 + carry as u16;
                let h = (a & 0xF) + (v & 0xF) + carry > 0xF;
                self.a = r as u8;
                self.set_flags(self.a == 0, false, h, r > 0xFF);
            }
            2 | 3 | 7 => {
                // SUB / SBC / CP
                let carry = if op == 3 && self.flag(FLAG_C) { 1 } else { 0 };
                let r = a as i16 - v as i16 - carry as i16;
                let h = ((a & 0xF) as i16) - ((v & 0xF) as i16) - (carry as i16) < 0;
                self.set_flags(r as u8 == 0, true, h, r < 0);
                if op != 7 {
                    self.a = r as u8;
                }
            }
            4 => {
                self.a &= v;
                self.set_flags(self.a == 0, false, true, false);
            }
            5 => {
                self.a ^= v;
                self.set_flags(self.a == 0, false, false, false);
            }
            _ => {
                self.a |= v;
                self.set_flags(self.a == 0, false, false, false);
            }
        }
    }

    fn add_sp_e8<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let e = self.fetch(bus) as i8 as i16 as u16;
        let sp = self.sp;
        let h = (sp & 0xF) + (e & 0xF) > 0xF;
        let c = (sp & 0xFF) + (e & 0xFF) > 0xFF;
        self.set_flags(false, false, h, c);
        sp.wrapping_add(e)
    }

    /// Request servicing of an interrupt with the given vector if IME is set.
    /// Returns true if it was taken.
    pub fn interrupt<B: Bus>(&mut self, bus: &mut B, vector: u16) -> bool {
        self.halted = false;
        if !self.ime {
            return false;
        }
        self.ime = false;
        let pc = self.pc;
        self.push(bus, pc);
        self.pc = vector;
        true
    }

    /// Execute one instruction and return the number of T-cycles it took.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        if self.locked || self.halted {
            return 4;
        }
        if self.ime_pending {
            self.ime_pending = false;
            self.ime = true;
        }
        let op = self.fetch(bus);
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let m = match x {
            0 => self.exec_x0(bus, op, y, z),
            1 => {
                if op == 0x76 {
                    self.halted = true;
                    1
                } else {
                    let v = self.reg(bus, z);
                    self.set_reg(bus, y, v);
                    if y == 6 || z == 6 {
                        2
                    } else {
                        1
                    }
                }
            }
            2 => {
                let v = self.reg(bus, z);
                self.alu(y, v);
                if z == 6 {
                    2
                } else {
                    1
                }
            }
            _ => self.exec_x3(bus, op, y, z),
        };
        m * 4
    }

    fn exec_x0<B: Bus>(&mut self, bus: &mut B, op: u8, y: u8, z: u8) -> u32 {
        match z {
            0 => match y {
                0 => 1, // NOP
                1 => {
                    let a = self.fetch16(bus);
                    let [hi, lo] = self.sp.to_be_bytes();
                    bus.write(a, lo);
                    bus.write(a.wrapping_add(1), hi);
                    5
                }
                2 => {
                    // STOP: also used for the CGB speed switch, which we treat as instant.
                    self.fetch(bus);
                    1
                }
                3 => {
                    let e = self.fetch(bus) as i8;
                    self.pc = self.pc.wrapping_add(e as u16);
                    3
                }
                _ => {
                    let e = self.fetch(bus) as i8;
                    if self.cond(y - 4) {
                        self.pc = self.pc.wrapping_add(e as u16);
                        3
                    } else {
                        2
                    }
                }
            },
            1 => {
                let p = y >> 1;
                if y & 1 == 0 {
                    let v = self.fetch16(bus);
                    self.set_rp(p, v);
                    3
                } else {
                    let hl = self.hl();
                    let v = self.rp(p);
                    let r = hl as u32 + v as u32;
                    let h = (hl & 0xFFF) + (v & 0xFFF) > 0xFFF;
                    self.f = (self.f & FLAG_Z) | (h as u8) << 5 | ((r > 0xFFFF) as u8) << 4;
                    self.set_hl(r as u16);
                    2
                }
            }
            2 => {
                let addr = match y >> 1 {
                    0 => self.bc(),
                    1 => self.de(),
                    _ => self.hl(),
                };
                if y & 1 == 0 {
                    bus.write(addr, self.a);
                } else {
                    self.a = bus.read(addr);
                }
                match y >> 1 {
                    2 => self.set_hl(addr.wrapping_add(1)),
                    3 => self.set_hl(addr.wrapping_sub(1)),
                    _ => {}
                }
                2
            }
            3 => {
                let p = y >> 1;
                let v = self.rp(p);
                self.set_rp(
                    p,
                    if y & 1 == 0 {
                        v.wrapping_add(1)
                    } else {
                        v.wrapping_sub(1)
                    },
                );
                2
            }
            4 | 5 => {
                let v = self.reg(bus, y);
                let (r, h, n) = if z == 4 {
                    (v.wrapping_add(1), v & 0xF == 0xF, false)
                } else {
                    (v.wrapping_sub(1), v & 0xF == 0, true)
                };
                self.set_reg(bus, y, r);
                self.f =
                    (self.f & FLAG_C) | ((r == 0) as u8) << 7 | (n as u8) << 6 | (h as u8) << 5;
                if y == 6 {
                    3
                } else {
                    1
                }
            }
            6 => {
                let v = self.fetch(bus);
                self.set_reg(bus, y, v);
                if y == 6 {
                    3
                } else {
                    2
                }
            }
            _ => {
                match y {
                    0 => {
                        // RLCA
                        let c = self.a >> 7;
                        self.a = self.a.rotate_left(1);
                        self.set_flags(false, false, false, c != 0);
                    }
                    1 => {
                        // RRCA
                        let c = self.a & 1;
                        self.a = self.a.rotate_right(1);
                        self.set_flags(false, false, false, c != 0);
                    }
                    2 => {
                        // RLA
                        let c = self.a >> 7;
                        self.a = (self.a << 1) | self.flag(FLAG_C) as u8;
                        self.set_flags(false, false, false, c != 0);
                    }
                    3 => {
                        // RRA
                        let c = self.a & 1;
                        self.a = (self.a >> 1) | (self.flag(FLAG_C) as u8) << 7;
                        self.set_flags(false, false, false, c != 0);
                    }
                    4 => self.daa(),
                    5 => {
                        self.a = !self.a;
                        self.f |= FLAG_N | FLAG_H;
                    }
                    6 => self.f = (self.f & FLAG_Z) | FLAG_C,
                    _ => self.f = (self.f & FLAG_Z) | ((!self.flag(FLAG_C)) as u8) << 4,
                }
                let _ = op;
                1
            }
        }
    }

    fn daa(&mut self) {
        let mut a = self.a;
        let mut carry = self.flag(FLAG_C);
        if !self.flag(FLAG_N) {
            if carry || a > 0x99 {
                a = a.wrapping_add(0x60);
                carry = true;
            }
            if self.flag(FLAG_H) || a & 0x0F > 0x09 {
                a = a.wrapping_add(0x06);
            }
        } else {
            if carry {
                a = a.wrapping_sub(0x60);
            }
            if self.flag(FLAG_H) {
                a = a.wrapping_sub(0x06);
            }
        }
        self.a = a;
        self.f = ((a == 0) as u8) << 7 | (self.f & FLAG_N) | (carry as u8) << 4;
    }

    fn exec_x3<B: Bus>(&mut self, bus: &mut B, op: u8, y: u8, z: u8) -> u32 {
        match op {
            0xC0 | 0xC8 | 0xD0 | 0xD8 => {
                if self.cond(y) {
                    self.pc = self.pop(bus);
                    5
                } else {
                    2
                }
            }
            0xE0 => {
                let n = self.fetch(bus);
                bus.write(0xFF00 | n as u16, self.a);
                3
            }
            0xE8 => {
                self.sp = self.add_sp_e8(bus);
                4
            }
            0xF0 => {
                let n = self.fetch(bus);
                self.a = bus.read(0xFF00 | n as u16);
                3
            }
            0xF8 => {
                let v = self.add_sp_e8(bus);
                self.set_hl(v);
                3
            }
            0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                let v = self.pop(bus);
                match y >> 1 {
                    0 => self.set_bc(v),
                    1 => self.set_de(v),
                    2 => self.set_hl(v),
                    _ => self.set_af(v),
                }
                3
            }
            0xC9 => {
                self.pc = self.pop(bus);
                4
            }
            0xD9 => {
                self.pc = self.pop(bus);
                self.ime = true;
                4
            }
            0xE9 => {
                self.pc = self.hl();
                1
            }
            0xF9 => {
                self.sp = self.hl();
                2
            }
            0xC2 | 0xCA | 0xD2 | 0xDA => {
                let a = self.fetch16(bus);
                if self.cond(y) {
                    self.pc = a;
                    4
                } else {
                    3
                }
            }
            0xE2 => {
                bus.write(0xFF00 | self.c as u16, self.a);
                2
            }
            0xEA => {
                let a = self.fetch16(bus);
                bus.write(a, self.a);
                4
            }
            0xF2 => {
                self.a = bus.read(0xFF00 | self.c as u16);
                2
            }
            0xFA => {
                let a = self.fetch16(bus);
                self.a = bus.read(a);
                4
            }
            0xC3 => {
                self.pc = self.fetch16(bus);
                4
            }
            0xCB => self.exec_cb(bus),
            0xF3 => {
                self.ime = false;
                self.ime_pending = false;
                1
            }
            0xFB => {
                self.ime_pending = true;
                1
            }
            0xC4 | 0xCC | 0xD4 | 0xDC => {
                let a = self.fetch16(bus);
                if self.cond(y) {
                    let pc = self.pc;
                    self.push(bus, pc);
                    self.pc = a;
                    6
                } else {
                    3
                }
            }
            0xC5 | 0xD5 | 0xE5 | 0xF5 => {
                let v = match y >> 1 {
                    0 => self.bc(),
                    1 => self.de(),
                    2 => self.hl(),
                    _ => self.af(),
                };
                self.push(bus, v);
                4
            }
            0xCD => {
                let a = self.fetch16(bus);
                let pc = self.pc;
                self.push(bus, pc);
                self.pc = a;
                6
            }
            _ if z == 6 => {
                let v = self.fetch(bus);
                self.alu(y, v);
                2
            }
            _ if z == 7 => {
                let pc = self.pc;
                self.push(bus, pc);
                self.pc = (y as u16) * 8;
                4
            }
            _ => {
                // Undefined opcode: the real CPU hangs.
                self.locked = true;
                1
            }
        }
    }

    fn exec_cb<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let op = self.fetch(bus);
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let v = self.reg(bus, z);
        match x {
            0 => {
                let carry_in = self.flag(FLAG_C) as u8;
                let (r, c) = match y {
                    0 => (v.rotate_left(1), v >> 7),
                    1 => (v.rotate_right(1), v & 1),
                    2 => ((v << 1) | carry_in, v >> 7),
                    3 => ((v >> 1) | (carry_in << 7), v & 1),
                    4 => (v << 1, v >> 7),
                    5 => ((v >> 1) | (v & 0x80), v & 1),
                    6 => (v.rotate_left(4), 0),
                    _ => (v >> 1, v & 1),
                };
                self.set_reg(bus, z, r);
                self.set_flags(r == 0, false, false, c != 0);
            }
            1 => {
                self.f = (self.f & FLAG_C) | FLAG_H | (((v >> y) & 1 == 0) as u8) << 7;
                return if z == 6 { 3 } else { 2 };
            }
            2 => self.set_reg(bus, z, v & !(1 << y)),
            _ => self.set_reg(bus, z, v | (1 << y)),
        }
        if z == 6 {
            4
        } else {
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ram(Vec<u8>);

    impl Bus for Ram {
        fn read(&mut self, addr: u16) -> u8 {
            self.0[addr as usize]
        }
        fn write(&mut self, addr: u16, value: u8) {
            self.0[addr as usize] = value;
        }
    }

    fn run(program: &[u8], steps: usize) -> (Cpu, Ram) {
        let mut ram = Ram(vec![0; 0x10000]);
        ram.0[0x100..0x100 + program.len()].copy_from_slice(program);
        let mut cpu = Cpu::new();
        for _ in 0..steps {
            cpu.step(&mut ram);
        }
        (cpu, ram)
    }

    #[test]
    fn loads_and_arithmetic() {
        // ld a,$3A; ld b,$C6; add a,b
        let (cpu, _) = run(&[0x3E, 0x3A, 0x06, 0xC6, 0x80], 3);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.f, FLAG_Z | FLAG_H | FLAG_C);
    }

    #[test]
    fn sub_and_compare() {
        // ld a,$3E; sub $0F; cp $2F
        let (cpu, _) = run(&[0x3E, 0x3E, 0xD6, 0x0F, 0xFE, 0x2F], 3);
        assert_eq!(cpu.a, 0x2F);
        assert_eq!(cpu.f, FLAG_Z | FLAG_N);
    }

    #[test]
    fn call_and_ret() {
        // call $0110; ld b,1; ... at $110: ld c,7; ret
        let mut prog = vec![0u8; 0x20];
        prog[0..3].copy_from_slice(&[0xCD, 0x10, 0x01]);
        prog[3..5].copy_from_slice(&[0x06, 0x01]);
        prog[0x10..0x13].copy_from_slice(&[0x0E, 0x07, 0xC9]);
        let (cpu, _) = run(&prog, 4);
        assert_eq!((cpu.b, cpu.c), (1, 7));
        assert_eq!(cpu.sp, 0xFFFE);
    }

    #[test]
    fn hl_increment_store_and_daa() {
        // ld hl,$C000; ld a,$15; ld [hl+],a; add a,$27; daa
        let (cpu, ram) = run(&[0x21, 0x00, 0xC0, 0x3E, 0x15, 0x22, 0xC6, 0x27, 0x27], 5);
        assert_eq!(ram.0[0xC000], 0x15);
        assert_eq!(cpu.hl(), 0xC001);
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn cb_ops() {
        // ld a,$85; swap a; bit 7,a; res 0,a; srl a
        let (cpu, _) = run(
            &[0x3E, 0x85, 0xCB, 0x37, 0xCB, 0x7F, 0xCB, 0x87, 0xCB, 0x3F],
            5,
        );
        assert_eq!(cpu.a, 0x2C);
    }

    #[test]
    fn push_pop_af_masks_flags() {
        // ld bc,$12FF; push bc; pop af
        let (cpu, _) = run(&[0x01, 0xFF, 0x12, 0xC5, 0xF1], 3);
        assert_eq!(cpu.af(), 0x12F0);
    }

    #[test]
    fn add_sp_flags() {
        // ld sp,$00FF; add sp,1
        let (cpu, _) = run(&[0x31, 0xFF, 0x00, 0xE8, 0x01], 2);
        assert_eq!(cpu.sp, 0x0100);
        assert_eq!(cpu.f, FLAG_H | FLAG_C);
    }

    #[test]
    fn conditional_jr() {
        // xor a; jr z,+2; ld b,1; ld c,2
        let (cpu, _) = run(&[0xAF, 0x28, 0x02, 0x06, 0x01, 0x0E, 0x02], 3);
        assert_eq!((cpu.b, cpu.c), (0, 2));
    }
}
