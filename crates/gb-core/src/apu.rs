//! The Game Boy audio processing unit (four sound channels).
//!
//! The APU is stepped once per M-cycle (4 T-cycles). Channel outputs are averaged over each
//! output sample period, which acts as a simple anti-aliasing filter, and then passed through
//! a high-pass filter that models the DC-blocking capacitor of the real hardware.

pub const CPU_CLOCK: u32 = 4_194_304;

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 1, 1, 1],
    [0, 1, 1, 1, 1, 1, 1, 0],
];

const NOISE_DIVISORS: [u32; 8] = [8, 16, 32, 48, 64, 80, 96, 112];

/// Bits that always read back as 1 for $FF10-$FF2F.
const READ_MASKS: [u8; 0x20] = [
    0x80, 0x3F, 0x00, 0xFF, 0xBF, // NR10-NR14
    0xFF, 0x3F, 0x00, 0xFF, 0xBF, // NR20-NR24
    0x7F, 0xFF, 0x9F, 0xFF, 0xBF, // NR30-NR34
    0xFF, 0xFF, 0x00, 0x00, 0xBF, // NR40-NR44
    0x00, 0x00, 0x70, // NR50-NR52
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

#[derive(Default, Clone)]
struct Envelope {
    initial: u8,
    increase: bool,
    period: u8,
    volume: u8,
    timer: u8,
}

impl Envelope {
    fn write(&mut self, v: u8) {
        self.initial = v >> 4;
        self.increase = v & 0x08 != 0;
        self.period = v & 0x07;
    }

    fn trigger(&mut self) {
        self.volume = self.initial;
        self.timer = if self.period == 0 { 8 } else { self.period };
    }

    fn clock(&mut self) {
        if self.period == 0 {
            return;
        }
        self.timer = self.timer.saturating_sub(1);
        if self.timer == 0 {
            self.timer = self.period;
            if self.increase && self.volume < 15 {
                self.volume += 1;
            } else if !self.increase && self.volume > 0 {
                self.volume -= 1;
            }
        }
    }
}

#[derive(Default, Clone)]
struct Square {
    enabled: bool,
    dac: bool,
    duty: u8,
    duty_pos: u8,
    length: u16,
    length_enable: bool,
    env: Envelope,
    freq: u16,
    timer: i32,
    // sweep (channel 1 only)
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_timer: u8,
    sweep_enabled: bool,
    sweep_shadow: u16,
}

impl Square {
    fn period(&self) -> i32 {
        (2048 - self.freq as i32) * 4
    }

    fn step(&mut self, cycles: i32) {
        self.timer -= cycles;
        while self.timer <= 0 {
            self.timer += self.period();
            self.duty_pos = (self.duty_pos + 1) & 7;
        }
    }

    fn output(&self) -> Option<u8> {
        if !self.dac {
            return None;
        }
        if !self.enabled {
            return Some(0);
        }
        Some(DUTY_TABLE[self.duty as usize][self.duty_pos as usize] * self.env.volume)
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn sweep_calc(&mut self) -> u16 {
        let delta = self.sweep_shadow >> self.sweep_shift;
        let f = if self.sweep_negate {
            self.sweep_shadow.wrapping_sub(delta)
        } else {
            self.sweep_shadow + delta
        };
        if f > 2047 {
            self.enabled = false;
        }
        f
    }

    fn clock_sweep(&mut self) {
        self.sweep_timer = self.sweep_timer.saturating_sub(1);
        if self.sweep_timer == 0 {
            self.sweep_timer = if self.sweep_period == 0 {
                8
            } else {
                self.sweep_period
            };
            if self.sweep_enabled && self.sweep_period != 0 {
                let f = self.sweep_calc();
                if f <= 2047 && self.sweep_shift != 0 {
                    self.sweep_shadow = f;
                    self.freq = f;
                    self.sweep_calc();
                }
            }
        }
    }

    fn trigger(&mut self, has_sweep: bool) {
        self.enabled = self.dac;
        if self.length == 0 {
            self.length = 64;
        }
        self.timer = self.period();
        self.env.trigger();
        if has_sweep {
            self.sweep_shadow = self.freq;
            self.sweep_timer = if self.sweep_period == 0 {
                8
            } else {
                self.sweep_period
            };
            self.sweep_enabled = self.sweep_period != 0 || self.sweep_shift != 0;
            if self.sweep_shift != 0 {
                self.sweep_calc();
            }
        }
    }
}

#[derive(Default, Clone)]
struct Wave {
    enabled: bool,
    dac: bool,
    length: u16,
    length_enable: bool,
    volume_code: u8,
    freq: u16,
    timer: i32,
    pos: u8,
    sample: u8,
    ram: [u8; 16],
}

impl Wave {
    fn period(&self) -> i32 {
        (2048 - self.freq as i32) * 2
    }

    fn step(&mut self, cycles: i32) {
        self.timer -= cycles;
        while self.timer <= 0 {
            self.timer += self.period();
            self.pos = (self.pos + 1) & 31;
            let byte = self.ram[(self.pos >> 1) as usize];
            self.sample = if self.pos & 1 == 0 {
                byte >> 4
            } else {
                byte & 0x0F
            };
        }
    }

    fn output(&self) -> Option<u8> {
        if !self.dac {
            return None;
        }
        if !self.enabled {
            return Some(0);
        }
        Some(match self.volume_code {
            0 => 0,
            1 => self.sample,
            2 => self.sample >> 1,
            _ => self.sample >> 2,
        })
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn trigger(&mut self) {
        self.enabled = self.dac;
        if self.length == 0 {
            self.length = 256;
        }
        // The first sample is played after an extra delay of a few cycles.
        self.timer = self.period() + 6;
        self.pos = 0;
    }
}

#[derive(Clone)]
struct Noise {
    enabled: bool,
    dac: bool,
    length: u16,
    length_enable: bool,
    env: Envelope,
    shift: u8,
    narrow: bool,
    divisor: u8,
    timer: i32,
    lfsr: u16,
}

impl Default for Noise {
    fn default() -> Self {
        Noise {
            enabled: false,
            dac: false,
            length: 0,
            length_enable: false,
            env: Envelope::default(),
            shift: 0,
            narrow: false,
            divisor: 0,
            timer: 8,
            lfsr: 0x7FFF,
        }
    }
}

impl Noise {
    fn period(&self) -> i32 {
        (NOISE_DIVISORS[self.divisor as usize] << self.shift) as i32
    }

    fn step(&mut self, cycles: i32) {
        self.timer -= cycles;
        while self.timer <= 0 {
            self.timer += self.period();
            // Shift amounts 14 and 15 stop the LFSR on real hardware.
            if self.shift >= 14 {
                continue;
            }
            let bit = (self.lfsr ^ (self.lfsr >> 1)) & 1;
            self.lfsr = (self.lfsr >> 1) | (bit << 14);
            if self.narrow {
                self.lfsr = (self.lfsr & !(1 << 6)) | (bit << 6);
            }
        }
    }

    fn output(&self) -> Option<u8> {
        if !self.dac {
            return None;
        }
        if !self.enabled {
            return Some(0);
        }
        Some(if self.lfsr & 1 == 0 {
            self.env.volume
        } else {
            0
        })
    }

    fn clock_length(&mut self) {
        if self.length_enable && self.length > 0 {
            self.length -= 1;
            if self.length == 0 {
                self.enabled = false;
            }
        }
    }

    fn trigger(&mut self) {
        self.enabled = self.dac;
        if self.length == 0 {
            self.length = 64;
        }
        self.timer = self.period();
        self.env.trigger();
        self.lfsr = 0x7FFF;
    }
}

#[derive(Clone)]
pub struct Apu {
    regs: [u8; 0x20],
    power: bool,
    ch1: Square,
    ch2: Square,
    ch3: Wave,
    ch4: Noise,
    frame_timer: u32,
    frame_step: u8,
    /// Channels muted by the user (not by the game).
    pub mute: [bool; 4],
    /// The volume of each channel set by the user, from 0 to 1, on top of the game's.
    pub gain: [f32; 4],

    sample_rate: u32,
    // Fixed point (16.16) cycle accounting for resampling.
    cycles_per_sample: u64,
    sample_clock: u64,
    acc_l: f32,
    acc_r: f32,
    acc_n: u32,
    hp_l: f32,
    hp_r: f32,
    hp_factor: f32,
    out: Vec<f32>,
    /// Peak absolute level per channel since the last call to [`Apu::take_levels`].
    levels: [f32; 4],
    /// When false, no samples are produced and channels are stepped in large chunks.
    output_enabled: bool,
}

impl Apu {
    pub fn new(sample_rate: u32) -> Apu {
        let mut apu = Apu {
            regs: [0; 0x20],
            power: false,
            ch1: Square::default(),
            ch2: Square::default(),
            ch3: Wave::default(),
            ch4: Noise::default(),
            frame_timer: 0,
            frame_step: 0,
            mute: [false; 4],
            gain: [1.0; 4],
            sample_rate: 0,
            cycles_per_sample: 0,
            sample_clock: 0,
            acc_l: 0.0,
            acc_r: 0.0,
            acc_n: 0,
            hp_l: 0.0,
            hp_r: 0.0,
            hp_factor: 0.0,
            out: Vec::with_capacity(4096),
            levels: [0.0; 4],
            output_enabled: true,
        };
        apu.set_sample_rate(sample_rate);
        apu
    }

    pub fn set_sample_rate(&mut self, rate: u32) {
        let rate = rate.max(1);
        self.sample_rate = rate;
        self.cycles_per_sample = ((CPU_CLOCK as u64) << 16) / rate as u64;
        self.hp_factor = 0.999958f32.powf(CPU_CLOCK as f32 / rate as f32);
    }

    /// Disable sample generation, e.g. to analyse a song faster than real time.
    pub fn set_output_enabled(&mut self, enabled: bool) {
        self.output_enabled = enabled;
    }

    /// The raw register file $FF10-$FF2F as last written.
    pub fn registers(&self) -> &[u8; 0x20] {
        &self.regs
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Interleaved stereo samples produced so far.
    pub fn samples(&self) -> &[f32] {
        &self.out
    }

    pub fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.out)
    }

    pub fn drain_samples_into(&mut self, dst: &mut Vec<f32>) {
        dst.append(&mut self.out);
    }

    pub fn take_levels(&mut self) -> [f32; 4] {
        std::mem::take(&mut self.levels)
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF10..=0xFF25 => {
                self.regs[(addr - 0xFF10) as usize] | READ_MASKS[(addr - 0xFF10) as usize]
            }
            0xFF26 => {
                (self.power as u8) << 7
                    | 0x70
                    | (self.ch1.enabled as u8)
                    | (self.ch2.enabled as u8) << 1
                    | (self.ch3.enabled as u8) << 2
                    | (self.ch4.enabled as u8) << 3
            }
            0xFF30..=0xFF3F => self.ch3.ram[(addr - 0xFF30) as usize],
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, v: u8) {
        if let 0xFF30..=0xFF3F = addr {
            self.ch3.ram[(addr - 0xFF30) as usize] = v;
            return;
        }
        if addr == 0xFF26 {
            let on = v & 0x80 != 0;
            if self.power && !on {
                for a in 0xFF10..=0xFF25 {
                    self.write_reg(a, 0);
                }
                self.ch1.enabled = false;
                self.ch2.enabled = false;
                self.ch3.enabled = false;
                self.ch4.enabled = false;
            } else if !self.power && on {
                self.frame_step = 0;
            }
            self.power = on;
            return;
        }
        if !self.power {
            // While powered off only the length counters can be written (DMG behaviour).
            match addr {
                0xFF11 => self.ch1.length = 64 - (v & 0x3F) as u16,
                0xFF16 => self.ch2.length = 64 - (v & 0x3F) as u16,
                0xFF1B => self.ch3.length = 256 - v as u16,
                0xFF20 => self.ch4.length = 64 - (v & 0x3F) as u16,
                _ => {}
            }
            return;
        }
        self.write_reg(addr, v);
    }

    fn write_reg(&mut self, addr: u16, v: u8) {
        if !(0xFF10..=0xFF25).contains(&addr) {
            return;
        }
        self.regs[(addr - 0xFF10) as usize] = v;
        match addr {
            0xFF10 => {
                self.ch1.sweep_period = (v >> 4) & 7;
                self.ch1.sweep_negate = v & 0x08 != 0;
                self.ch1.sweep_shift = v & 7;
            }
            0xFF11 => {
                self.ch1.duty = v >> 6;
                self.ch1.length = 64 - (v & 0x3F) as u16;
            }
            0xFF12 => {
                self.ch1.env.write(v);
                self.ch1.dac = v & 0xF8 != 0;
                if !self.ch1.dac {
                    self.ch1.enabled = false;
                }
            }
            0xFF13 => self.ch1.freq = (self.ch1.freq & 0x700) | v as u16,
            0xFF14 => {
                self.ch1.freq = (self.ch1.freq & 0xFF) | ((v as u16 & 7) << 8);
                self.ch1.length_enable = v & 0x40 != 0;
                if v & 0x80 != 0 {
                    self.ch1.trigger(true);
                }
            }
            0xFF16 => {
                self.ch2.duty = v >> 6;
                self.ch2.length = 64 - (v & 0x3F) as u16;
            }
            0xFF17 => {
                self.ch2.env.write(v);
                self.ch2.dac = v & 0xF8 != 0;
                if !self.ch2.dac {
                    self.ch2.enabled = false;
                }
            }
            0xFF18 => self.ch2.freq = (self.ch2.freq & 0x700) | v as u16,
            0xFF19 => {
                self.ch2.freq = (self.ch2.freq & 0xFF) | ((v as u16 & 7) << 8);
                self.ch2.length_enable = v & 0x40 != 0;
                if v & 0x80 != 0 {
                    self.ch2.trigger(false);
                }
            }
            0xFF1A => {
                self.ch3.dac = v & 0x80 != 0;
                if !self.ch3.dac {
                    self.ch3.enabled = false;
                }
            }
            0xFF1B => self.ch3.length = 256 - v as u16,
            0xFF1C => self.ch3.volume_code = (v >> 5) & 3,
            0xFF1D => self.ch3.freq = (self.ch3.freq & 0x700) | v as u16,
            0xFF1E => {
                self.ch3.freq = (self.ch3.freq & 0xFF) | ((v as u16 & 7) << 8);
                self.ch3.length_enable = v & 0x40 != 0;
                if v & 0x80 != 0 {
                    self.ch3.trigger();
                }
            }
            0xFF20 => self.ch4.length = 64 - (v & 0x3F) as u16,
            0xFF21 => {
                self.ch4.env.write(v);
                self.ch4.dac = v & 0xF8 != 0;
                if !self.ch4.dac {
                    self.ch4.enabled = false;
                }
            }
            0xFF22 => {
                self.ch4.shift = v >> 4;
                self.ch4.narrow = v & 0x08 != 0;
                self.ch4.divisor = v & 7;
            }
            0xFF23 => {
                self.ch4.length_enable = v & 0x40 != 0;
                if v & 0x80 != 0 {
                    self.ch4.trigger();
                }
            }
            _ => {}
        }
    }

    fn clock_frame_sequencer(&mut self) {
        match self.frame_step {
            0 | 4 => self.clock_lengths(),
            2 | 6 => {
                self.clock_lengths();
                self.ch1.clock_sweep();
            }
            7 => {
                self.ch1.env.clock();
                self.ch2.env.clock();
                self.ch4.env.clock();
            }
            _ => {}
        }
        self.frame_step = (self.frame_step + 1) & 7;
    }

    fn clock_lengths(&mut self) {
        self.ch1.clock_length();
        self.ch2.clock_length();
        self.ch3.clock_length();
        self.ch4.clock_length();
    }

    /// Advance the APU by `cycles` T-cycles (at normal speed).
    pub fn step(&mut self, cycles: u32) {
        if !self.output_enabled {
            self.step_fast(cycles);
            return;
        }
        let mut remaining = cycles;
        while remaining > 0 {
            let c = remaining.min(4);
            remaining -= c;
            self.step_m(c);
        }
    }

    fn step_fast(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            let c = remaining.min(8192 - self.frame_timer);
            remaining -= c;
            if !self.power {
                continue;
            }
            let ci = c as i32;
            self.ch1.step(ci);
            self.ch2.step(ci);
            self.ch3.step(ci);
            self.ch4.step(ci);
            self.frame_timer += c;
            if self.frame_timer >= 8192 {
                self.frame_timer -= 8192;
                self.clock_frame_sequencer();
            }
        }
    }

    fn step_m(&mut self, c: u32) {
        if self.power {
            self.frame_timer += c;
            if self.frame_timer >= 8192 {
                self.frame_timer -= 8192;
                self.clock_frame_sequencer();
            }
            let ci = c as i32;
            self.ch1.step(ci);
            self.ch2.step(ci);
            self.ch3.step(ci);
            self.ch4.step(ci);
        }
        self.mix(c);
    }

    fn mix(&mut self, c: u32) {
        let (mut l, mut r) = (0.0f32, 0.0f32);
        if self.power {
            let nr51 = self.regs[0x15];
            let outs = [
                self.ch1.output(),
                self.ch2.output(),
                self.ch3.output(),
                self.ch4.output(),
            ];
            for (i, o) in outs.iter().enumerate() {
                let Some(d) = *o else { continue };
                if self.mute[i] {
                    continue;
                }
                // DAC: digital 0..15 maps linearly onto an analog -1..1 range.
                let gain = self.gain[i];
                let a = (1.0 - d as f32 / 7.5) * gain;
                let lv = self.levels[i].max(d as f32 / 15.0 * gain);
                self.levels[i] = lv;
                if nr51 & (0x10 << i) != 0 {
                    l += a;
                }
                if nr51 & (0x01 << i) != 0 {
                    r += a;
                }
            }
            let nr50 = self.regs[0x14];
            l *= (((nr50 >> 4) & 7) + 1) as f32 / 8.0;
            r *= ((nr50 & 7) + 1) as f32 / 8.0;
        }
        let w = c as f32;
        self.acc_l += l * w;
        self.acc_r += r * w;
        self.acc_n += c;

        self.sample_clock += (c as u64) << 16;
        if self.sample_clock >= self.cycles_per_sample {
            self.sample_clock -= self.cycles_per_sample;
            let n = self.acc_n.max(1) as f32;
            let (sl, sr) = (self.acc_l / n / 4.0, self.acc_r / n / 4.0);
            self.acc_l = 0.0;
            self.acc_r = 0.0;
            self.acc_n = 0;
            let ol = sl - self.hp_l;
            self.hp_l = sl - ol * self.hp_factor;
            let or = sr - self.hp_r;
            self.hp_r = sr - or * self.hp_factor;
            self.out.push(ol);
            self.out.push(or);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_wave_produces_sound() {
        let mut apu = Apu::new(48_000);
        apu.write(0xFF26, 0x80);
        apu.write(0xFF24, 0x77);
        apu.write(0xFF25, 0xFF);
        apu.write(0xFF12, 0xF0);
        apu.write(0xFF13, 0x00);
        apu.write(0xFF14, 0x87); // 131072/(2048-1792) = 512 Hz
        apu.step(CPU_CLOCK / 10);
        let s = apu.take_samples();
        assert!((s.len() as i32 - 9600).abs() <= 2, "{}", s.len());
        let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.1, "peak {peak}");
        assert_eq!(apu.read(0xFF26) & 1, 1);
    }

    #[test]
    fn channel_gain_scales_its_output() {
        let peak = |gain: f32| {
            let mut apu = Apu::new(48_000);
            apu.gain[0] = gain;
            apu.write(0xFF26, 0x80);
            apu.write(0xFF24, 0x77);
            apu.write(0xFF25, 0xFF);
            apu.write(0xFF12, 0xF0);
            apu.write(0xFF13, 0x00);
            apu.write(0xFF14, 0x87);
            apu.step(CPU_CLOCK / 10);
            let s = apu.take_samples();
            (
                s.iter().fold(0.0f32, |m, v| m.max(v.abs())),
                apu.take_levels()[0],
            )
        };
        let (full, full_level) = peak(1.0);
        let (half, half_level) = peak(0.5);
        assert!((half / full - 0.5).abs() < 0.01, "{half} / {full}");
        assert!((half_level / full_level - 0.5).abs() < 0.01);
        assert_eq!(peak(0.0), (0.0, 0.0));
    }

    #[test]
    fn length_counter_disables_channel() {
        let mut apu = Apu::new(48_000);
        apu.write(0xFF26, 0x80);
        apu.write(0xFF21, 0xF0);
        apu.write(0xFF20, 0x3F); // length 1
        apu.write(0xFF23, 0xC0);
        assert_eq!(apu.read(0xFF26) & 8, 8);
        apu.step(8192 * 3);
        assert_eq!(apu.read(0xFF26) & 8, 0);
    }

    #[test]
    fn power_off_clears_registers() {
        let mut apu = Apu::new(48_000);
        apu.write(0xFF26, 0x80);
        apu.write(0xFF24, 0x77);
        apu.write(0xFF26, 0x00);
        assert_eq!(apu.read(0xFF24), 0x00);
        assert_eq!(apu.read(0xFF26), 0x70);
    }
}
