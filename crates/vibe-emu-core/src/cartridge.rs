use std::cell::Cell;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Memory Bank Controller type decoded from the cartridge header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbcType {
    /// No MBC; direct ROM access only.
    NoMbc,
    /// MBC1 mapper.
    Mbc1,
    /// MBC2 mapper (also includes built-in RAM).
    Mbc2,
    /// MBC3 mapper (may include RTC).
    Mbc3,
    /// MBC30 mapper (MBC3 variant with larger RAM).
    Mbc30,
    /// MBC5 mapper.
    Mbc5,
    /// TPP1 mapper.
    Tpp1,
    /// Unrecognized mapper type byte.
    Unknown(u8),
}

/// A loaded Game Boy cartridge including ROM, RAM, and MBC state.
#[derive(Debug)]
pub struct Cartridge {
    /// Raw ROM data.
    pub rom: Vec<u8>,
    /// Cartridge RAM (battery-backed save RAM).
    pub ram: Vec<u8>,
    /// The MBC type parsed from the cartridge header.
    pub mbc: MbcType,
    /// Whether this cartridge declares CGB compatibility.
    pub cgb: bool,
    /// Game title extracted from the cartridge header.
    pub title: String,
    cart_type: u8,
    save_path: Option<PathBuf>,
    rtc_path: Option<PathBuf>,
    mbc_state: MbcState,
    cart_bus: Cell<u8>,
}

#[derive(Debug)]
enum MbcState {
    NoMbc,
    Mbc1 {
        rom_bank: u8,
        ram_bank: u8,
        mode: u8,
        ram_enable: bool,
        multicart: bool,
    },
    Mbc2 {
        rom_bank: u8,
        ram_enable: bool,
    },
    Mbc3 {
        rom_bank: u8,
        ram_bank: u8,
        ram_enable: bool,
        rtc: Option<Mbc3Rtc>,
    },
    Mbc30 {
        rom_bank: u8,
        ram_bank: u8,
        ram_enable: bool,
        rtc: Option<Mbc3Rtc>,
    },
    Mbc5 {
        rom_bank: u16,
        ram_bank: u8,
        ram_enable: bool,
    },
    Tpp1 {
        mr0: u8,
        mr1: u8,
        mr2: u8,
        mapping: Tpp1Mapping,
        rumble_speed: u8,
        has_rumble: bool,
        has_multi_rumble: bool,
        has_battery: bool,
        rtc: Option<Tpp1Rtc>,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tpp1Mapping {
    ControlRegisters,
    SramReadOnly,
    SramReadWrite,
    RtcLatched,
}

#[derive(Debug, Clone, Copy, Default)]
struct RtcRegisters {
    seconds: u8,
    minutes: u8,
    hours: u8,
    days: u16,
    halt: bool,
    carry: bool,
}

#[derive(Debug, Clone)]
struct Mbc3Rtc {
    regs: RtcRegisters,
    latched: RtcRegisters,
    latched_active: bool,
    last_update: SystemTime,
    subsecond_cycles: u32,
}

const RTC_CYCLES_PER_SECOND: u32 = 4_194_304;

const RTC_FILE_MAGIC: &[u8; 4] = b"RTC1";
const RTC_FILE_VERSION: u8 = 1;

impl RtcRegisters {
    fn control_byte(&self) -> u8 {
        let mut out = ((self.days >> 8) as u8) & 0x01;
        if self.halt {
            out |= 0x40;
        }
        if self.carry {
            out |= 0x80;
        }
        out
    }
}

impl Mbc3Rtc {
    fn new(now: SystemTime) -> Self {
        let regs = RtcRegisters::default();
        Self {
            regs,
            latched: regs,
            latched_active: false,
            last_update: now,
            subsecond_cycles: 0,
        }
    }

    fn latch(&mut self) {
        self.latched = self.regs;
        self.latched_active = true;
    }

    fn read_register(&self, reg: u8) -> u8 {
        let regs = if self.latched_active {
            &self.latched
        } else {
            &self.regs
        };

        match reg {
            0x08 => regs.seconds & 0x3F,
            0x09 => regs.minutes & 0x3F,
            0x0A => regs.hours & 0x1F,
            0x0B => (regs.days & 0x00FF) as u8,
            0x0C => regs.control_byte(),
            _ => 0xFF,
        }
    }

    fn read_latched(&self, reg: u8) -> u8 {
        self.read_register(reg)
    }

    fn write_register(&mut self, reg: u8, value: u8) {
        match reg {
            0x08 => {
                self.regs.seconds = value & 0x3F;
                self.subsecond_cycles = 0;
            }
            0x09 => {
                self.regs.minutes = value & 0x3F;
            }
            0x0A => {
                self.regs.hours = value & 0x1F;
            }
            0x0B => {
                self.regs.days = (self.regs.days & 0x0100) | value as u16;
            }
            0x0C => {
                self.regs.days = (self.regs.days & 0x00FF) | (((value & 0x01) as u16) << 8);
                self.regs.halt = value & 0x40 != 0;
                self.regs.carry = value & 0x80 != 0;
            }
            _ => {}
        }
    }

    fn step(&mut self, cpu_cycles: u64) {
        if self.regs.halt {
            return;
        }

        self.add_cycles(cpu_cycles);
    }

    fn sync_wall(&mut self, now: SystemTime) {
        let elapsed = now.duration_since(self.last_update).unwrap_or_default();
        self.last_update = now;
        if self.regs.halt {
            return;
        }

        let elapsed_cycles = (elapsed.as_secs() as u128)
            .saturating_mul(RTC_CYCLES_PER_SECOND as u128)
            .saturating_add(
                (elapsed.subsec_nanos() as u128).saturating_mul(RTC_CYCLES_PER_SECOND as u128)
                    / 1_000_000_000u128,
            );
        self.add_cycles(elapsed_cycles.min(u64::MAX as u128) as u64);
    }

    fn mark_persisted(&mut self, now: SystemTime) {
        self.last_update = now;
    }

    fn add_cycles(&mut self, cycles: u64) {
        debug_assert!(self.subsecond_cycles < RTC_CYCLES_PER_SECOND);

        let mut seconds = cycles / RTC_CYCLES_PER_SECOND as u64;
        let rem = (cycles % RTC_CYCLES_PER_SECOND as u64) as u32;

        let mut sub = self.subsecond_cycles + rem;
        if sub >= RTC_CYCLES_PER_SECOND {
            sub -= RTC_CYCLES_PER_SECOND;
            seconds += 1;
        }
        self.subsecond_cycles = sub;

        if seconds > 0 {
            self.advance_seconds(seconds);
        }
    }

    fn advance_seconds(&mut self, mut seconds: u64) {
        while seconds > 0 {
            let until_minute_tick = self.seconds_until_minute_tick();
            if seconds < until_minute_tick {
                self.regs.seconds = ((self.regs.seconds as u64 + seconds) & 0x3F) as u8;
                return;
            }

            seconds -= until_minute_tick;
            self.regs.seconds = 0;
            self.minute_tick();
        }
    }

    fn seconds_until_minute_tick(&self) -> u64 {
        let sec = self.regs.seconds as u64;
        if sec <= 59 {
            60 - sec
        } else {
            (63 - sec + 1) + 60
        }
    }

    fn minute_tick(&mut self) {
        let overflow = self.regs.minutes == 59;
        self.regs.minutes = ((self.regs.minutes as u16 + 1) & 0x3F) as u8;
        if overflow {
            self.regs.minutes = 0;
            self.hour_tick();
        }
    }

    fn hour_tick(&mut self) {
        let overflow = self.regs.hours == 23;
        self.regs.hours = ((self.regs.hours as u16 + 1) & 0x1F) as u8;
        if overflow {
            self.regs.hours = 0;
            self.day_tick();
        }
    }

    fn day_tick(&mut self) {
        if self.regs.days >= 0x01FF {
            self.regs.days = 0;
            self.regs.carry = true;
        } else {
            self.regs.days = (self.regs.days + 1) & 0x01FF;
        }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(4 + 1 + 8 + 4 + 1 + 1 + 1 + 2 + 1);
        data.extend_from_slice(RTC_FILE_MAGIC);
        data.push(RTC_FILE_VERSION);

        let saved_time = self
            .last_update
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        data.extend_from_slice(&saved_time.to_le_bytes());

        let subsecond_nanos = ((self.subsecond_cycles as u128).saturating_mul(1_000_000_000u128)
            / (RTC_CYCLES_PER_SECOND as u128))
            .min(u32::MAX as u128) as u32;
        data.extend_from_slice(&subsecond_nanos.to_le_bytes());
        data.push(self.regs.seconds & 0x3F);
        data.push(self.regs.minutes & 0x3F);
        data.push(self.regs.hours & 0x1F);
        data.extend_from_slice(&(self.regs.days & 0x01FF).to_le_bytes());

        let mut flags = 0u8;
        if self.regs.halt {
            flags |= 0x01;
        }
        if self.regs.carry {
            flags |= 0x02;
        }
        data.push(flags);

        data
    }

    fn load_from_bytes(&mut self, data: &[u8]) -> bool {
        if data.len() < 23 || &data[..4] != RTC_FILE_MAGIC || data[4] != RTC_FILE_VERSION {
            return false;
        }

        let secs = u64::from_le_bytes(data[5..13].try_into().unwrap());
        let nanos = u32::from_le_bytes(data[13..17].try_into().unwrap()).min(999_999_999);

        self.last_update = UNIX_EPOCH + Duration::from_secs(secs);
        self.subsecond_cycles = ((nanos as u128).saturating_mul(RTC_CYCLES_PER_SECOND as u128)
            / 1_000_000_000u128)
            .min((RTC_CYCLES_PER_SECOND - 1) as u128) as u32;
        self.regs.seconds = data[17] & 0x3F;
        self.regs.minutes = data[18] & 0x3F;
        self.regs.hours = data[19] & 0x1F;
        self.regs.days = u16::from_le_bytes([data[20], data[21]]) & 0x01FF;

        let flags = data[22];
        self.regs.halt = flags & 0x01 != 0;
        self.regs.carry = flags & 0x02 != 0;
        self.latched = self.regs;
        self.latched_active = false;
        true
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Tpp1RtcRegisters {
    rtcw: u8,
    rtcdh: u8,
    rtcm: u8,
    rtcs: u8,
}

#[derive(Debug, Clone)]
struct Tpp1Rtc {
    regs: Tpp1RtcRegisters,
    latched: Tpp1RtcRegisters,
    running: bool,
    overflow: bool,
    last_update: SystemTime,
    subsecond_cycles: u32,
}

const TPP1_RTC_FILE_MAGIC: &[u8; 4] = b"TPP1";
const TPP1_RTC_FILE_VERSION: u8 = 1;

impl Tpp1RtcRegisters {
    fn seconds(&self) -> u8 {
        self.rtcs & 0x3F
    }

    fn minutes(&self) -> u8 {
        self.rtcm & 0x3F
    }

    fn hours(&self) -> u8 {
        self.rtcdh & 0x1F
    }

    fn day_of_week(&self) -> u8 {
        (self.rtcdh >> 5) & 0x07
    }
}

impl Tpp1Rtc {
    fn new(now: SystemTime) -> Self {
        Self {
            regs: Tpp1RtcRegisters::default(),
            latched: Tpp1RtcRegisters::default(),
            running: false,
            overflow: false,
            last_update: now,
            subsecond_cycles: 0,
        }
    }

    fn latch(&mut self) {
        self.latched = self.regs;
    }

    fn set_from_latch(&mut self) {
        self.regs = self.latched;
        self.subsecond_cycles = 0;
    }

    fn read_latched(&self, addr: u16) -> u8 {
        match addr & 0x03 {
            0 => self.latched.rtcw,
            1 => self.latched.rtcdh,
            2 => self.latched.rtcm,
            3 => self.latched.rtcs,
            _ => unreachable!(),
        }
    }

    fn write_latched(&mut self, addr: u16, val: u8) {
        match addr & 0x03 {
            0 => self.latched.rtcw = val,
            1 => self.latched.rtcdh = val,
            2 => self.latched.rtcm = val,
            3 => self.latched.rtcs = val,
            _ => unreachable!(),
        }
    }

    fn mr4_bits(&self) -> u8 {
        let mut bits = 0u8;
        if self.running {
            bits |= 0x04;
        }
        if self.overflow {
            bits |= 0x08;
        }
        bits
    }

    fn step(&mut self, cpu_cycles: u64) {
        if !self.running {
            return;
        }
        self.add_cycles(cpu_cycles);
    }

    fn sync_wall(&mut self, now: SystemTime) {
        let elapsed = now.duration_since(self.last_update).unwrap_or_default();
        self.last_update = now;
        if !self.running {
            return;
        }

        let elapsed_cycles = (elapsed.as_secs() as u128)
            .saturating_mul(RTC_CYCLES_PER_SECOND as u128)
            .saturating_add(
                (elapsed.subsec_nanos() as u128).saturating_mul(RTC_CYCLES_PER_SECOND as u128)
                    / 1_000_000_000u128,
            );
        self.add_cycles(elapsed_cycles.min(u64::MAX as u128) as u64);
    }

    fn mark_persisted(&mut self, now: SystemTime) {
        self.last_update = now;
    }

    fn add_cycles(&mut self, cycles: u64) {
        let mut seconds = cycles / RTC_CYCLES_PER_SECOND as u64;
        let rem = (cycles % RTC_CYCLES_PER_SECOND as u64) as u32;

        let mut sub = self.subsecond_cycles + rem;
        if sub >= RTC_CYCLES_PER_SECOND {
            sub -= RTC_CYCLES_PER_SECOND;
            seconds += 1;
        }
        self.subsecond_cycles = sub;

        if seconds > 0 {
            self.advance_seconds(seconds);
        }
    }

    fn advance_seconds(&mut self, mut seconds: u64) {
        while seconds > 0 {
            let current_sec = self.regs.seconds();
            let until_next = if current_sec < 60 {
                (60 - current_sec) as u64
            } else {
                1
            };

            if seconds < until_next {
                self.regs.rtcs = ((current_sec as u64 + seconds) & 0x3F) as u8;
                return;
            }

            seconds -= until_next;
            self.regs.rtcs = 0;
            self.minute_tick();
        }
    }

    fn minute_tick(&mut self) {
        let min = self.regs.minutes();
        if min >= 59 {
            self.regs.rtcm = 0;
            self.hour_tick();
        } else {
            self.regs.rtcm = min + 1;
        }
    }

    fn hour_tick(&mut self) {
        let hour = self.regs.hours();
        let dow = self.regs.day_of_week();
        if hour >= 23 {
            self.regs.rtcdh = dow << 5;
            self.day_tick();
        } else {
            self.regs.rtcdh = (dow << 5) | (hour + 1);
        }
    }

    fn day_tick(&mut self) {
        let dow = self.regs.day_of_week();
        let hour = self.regs.hours();
        if dow >= 6 {
            self.regs.rtcdh = hour;
            self.week_tick();
        } else {
            self.regs.rtcdh = ((dow + 1) << 5) | hour;
        }
    }

    fn week_tick(&mut self) {
        if self.regs.rtcw == 0xFF {
            self.regs.rtcw = 0;
            self.overflow = true;
        } else {
            self.regs.rtcw += 1;
        }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(22);
        data.extend_from_slice(TPP1_RTC_FILE_MAGIC);
        data.push(TPP1_RTC_FILE_VERSION);

        let saved_time = self
            .last_update
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        data.extend_from_slice(&saved_time.to_le_bytes());

        let subsecond_nanos = ((self.subsecond_cycles as u128).saturating_mul(1_000_000_000u128)
            / (RTC_CYCLES_PER_SECOND as u128))
            .min(u32::MAX as u128) as u32;
        data.extend_from_slice(&subsecond_nanos.to_le_bytes());

        data.push(self.regs.rtcw);
        data.push(self.regs.rtcdh);
        data.push(self.regs.rtcm);
        data.push(self.regs.rtcs);

        let mut flags = 0u8;
        if self.running {
            flags |= 0x01;
        }
        if self.overflow {
            flags |= 0x02;
        }
        data.push(flags);

        data
    }

    fn load_from_bytes(&mut self, data: &[u8]) -> bool {
        if data.len() < 22 || &data[..4] != TPP1_RTC_FILE_MAGIC || data[4] != TPP1_RTC_FILE_VERSION
        {
            return false;
        }

        let secs = u64::from_le_bytes(data[5..13].try_into().unwrap());
        let nanos = u32::from_le_bytes(data[13..17].try_into().unwrap()).min(999_999_999);

        self.last_update = UNIX_EPOCH + Duration::from_secs(secs);
        self.subsecond_cycles = ((nanos as u128).saturating_mul(RTC_CYCLES_PER_SECOND as u128)
            / 1_000_000_000u128)
            .min((RTC_CYCLES_PER_SECOND - 1) as u128) as u32;

        self.regs.rtcw = data[17];
        self.regs.rtcdh = data[18];
        self.regs.rtcm = data[19];
        self.regs.rtcs = data[20];

        let flags = data[21];
        self.running = flags & 0x01 != 0;
        self.overflow = flags & 0x02 != 0;

        self.latched = self.regs;
        true
    }
}

impl Cartridge {
    fn bus_read(cart_bus: &Cell<u8>, value: u8) -> u8 {
        cart_bus.set(value);
        value
    }

    fn bus_write(cart_bus: &Cell<u8>, value: u8) {
        cart_bus.set(value);
    }

    fn open_bus(cart_bus: &Cell<u8>) -> u8 {
        cart_bus.get()
    }

    /// Returns the currently selected ROM bank for the $4000-$7FFF window.
    ///
    /// This is intended for UI/debugger tooling; core emulation should not rely
    /// on this for correctness.
    pub fn current_rom_bank(&self) -> u16 {
        match &self.mbc_state {
            MbcState::NoMbc => 1,
            MbcState::Mbc1 { rom_bank, .. } => (*rom_bank).into(),
            MbcState::Mbc2 { rom_bank, .. } => (*rom_bank).into(),
            MbcState::Mbc3 { rom_bank, .. } => (*rom_bank).into(),
            MbcState::Mbc30 { rom_bank, .. } => (*rom_bank).into(),
            MbcState::Mbc5 { rom_bank, .. } => *rom_bank,
            MbcState::Tpp1 { mr0, mr1, .. } => ((*mr1 as u16) << 8) | *mr0 as u16,
            MbcState::Unknown => 1,
        }
    }

    /// Returns the currently selected external RAM bank (if the mapper supports banking).
    ///
    /// This is intended for UI/debugger tooling.
    pub fn current_ram_bank(&self) -> u8 {
        match &self.mbc_state {
            MbcState::NoMbc => 0,
            MbcState::Mbc1 { ram_bank, .. } => *ram_bank,
            MbcState::Mbc2 { .. } => 0,
            MbcState::Mbc3 { ram_bank, .. } => *ram_bank,
            MbcState::Mbc30 { ram_bank, .. } => *ram_bank,
            MbcState::Mbc5 { ram_bank, .. } => *ram_bank,
            MbcState::Tpp1 { mr2, .. } => *mr2,
            MbcState::Unknown => 0,
        }
    }

    /// Returns whether external RAM / RTC is currently enabled for the $A000-$BFFF window.
    ///
    /// This is intended for UI/debugger tooling.
    pub fn ram_enabled(&self) -> bool {
        match &self.mbc_state {
            MbcState::NoMbc => true,
            MbcState::Mbc1 { ram_enable, .. } => *ram_enable,
            MbcState::Mbc2 { ram_enable, .. } => *ram_enable,
            MbcState::Mbc3 { ram_enable, .. } => *ram_enable,
            MbcState::Mbc30 { ram_enable, .. } => *ram_enable,
            MbcState::Mbc5 { ram_enable, .. } => *ram_enable,
            MbcState::Tpp1 { mapping, .. } => matches!(
                mapping,
                Tpp1Mapping::SramReadOnly | Tpp1Mapping::SramReadWrite
            ),
            MbcState::Unknown => false,
        }
    }

    /// Advance the real-time clock by `cpu_cycles` CPU cycles (MBC3/MBC30/TPP1 only).
    pub fn step_rtc(&mut self, cpu_cycles: u16) {
        match &mut self.mbc_state {
            MbcState::Mbc3 { rtc: Some(rtc), .. } | MbcState::Mbc30 { rtc: Some(rtc), .. } => {
                rtc.step(cpu_cycles as u64)
            }
            MbcState::Tpp1 { rtc: Some(rtc), .. } => rtc.step(cpu_cycles as u64),
            _ => {}
        }
    }

    /// Load a cartridge from raw bytes and pre-allocate `ram_size` bytes of RAM.
    pub fn from_bytes_with_ram(data: Vec<u8>, ram_size: usize) -> Self {
        let mut c = Self::load(data);
        c.ram = vec![0; ram_size];
        c
    }

    /// Load a cartridge from a file path, automatically loading battery-backed save data.
    pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let data = fs::read(&path)?;
        let mut cart = Self::load(data);

        if cart.has_battery() {
            let mut save = PathBuf::from(path.as_ref());
            save.set_extension("sav");
            cart.save_path = Some(save.clone());
            if let Ok(bytes) = fs::read(&save) {
                for (d, s) in cart.ram.iter_mut().zip(bytes.iter()) {
                    *d = *s;
                }
            }
        }

        if cart.has_rtc() {
            let mut rtc_path = PathBuf::from(path.as_ref());
            rtc_path.set_extension("rtc");
            cart.rtc_path = Some(rtc_path.clone());
            match &mut cart.mbc_state {
                MbcState::Mbc3 { rtc: Some(rtc), .. } | MbcState::Mbc30 { rtc: Some(rtc), .. } => {
                    if let Ok(bytes) = fs::read(&rtc_path)
                        && !rtc.load_from_bytes(&bytes)
                    {
                        core_warn!(
                            target: "vibe_emu_core::cartridge",
                            "Failed to parse RTC data from {}",
                            rtc_path.display()
                        );
                    }
                    let now = SystemTime::now();
                    rtc.sync_wall(now);
                    rtc.latch();
                }
                MbcState::Tpp1 { rtc: Some(rtc), .. } => {
                    if let Ok(bytes) = fs::read(&rtc_path)
                        && !rtc.load_from_bytes(&bytes)
                    {
                        core_warn!(
                            target: "vibe_emu_core::cartridge",
                            "Failed to parse RTC data from {}",
                            rtc_path.display()
                        );
                    }
                    let now = SystemTime::now();
                    rtc.sync_wall(now);
                    rtc.latch();
                }
                _ => {}
            }
        }

        core_info!(
            target: "vibe_emu_core::cartridge",
            "Loaded ROM: {} (MBC: {:?}, CGB: {})",
            cart.title,
            cart.mbc,
            if cart.cgb { "yes" } else { "no" }
        );
        Ok(cart)
    }

    /// Parse a cartridge from raw ROM bytes.
    pub fn load(data: Vec<u8>) -> Self {
        let header = Header::parse(&data);
        let ram_size = header.ram_size();

        let cart_type = header.cart_type();
        let has_rtc = header.has_rtc();
        let mbc = header.mbc_type();
        let cgb = header.cgb_supported();
        let title = header.title();
        let now = SystemTime::now();

        let mbc_state = match mbc {
            MbcType::NoMbc => MbcState::NoMbc,
            MbcType::Mbc1 => MbcState::Mbc1 {
                rom_bank: 1,
                ram_bank: 0,
                mode: 0,
                ram_enable: false,
                multicart: detect_mbc1_multicart(&data),
            },
            MbcType::Mbc2 => MbcState::Mbc2 {
                rom_bank: 1,
                ram_enable: false,
            },
            MbcType::Mbc3 => MbcState::Mbc3 {
                rom_bank: 1,
                ram_bank: 0,
                ram_enable: false,
                rtc: has_rtc.then(|| Mbc3Rtc::new(now)),
            },
            MbcType::Mbc30 => MbcState::Mbc30 {
                rom_bank: 1,
                ram_bank: 0,
                ram_enable: false,
                rtc: has_rtc.then(|| Mbc3Rtc::new(now)),
            },
            MbcType::Mbc5 => MbcState::Mbc5 {
                rom_bank: 1,
                ram_bank: 0,
                ram_enable: false,
            },
            MbcType::Tpp1 => {
                let features = header.tpp1_features();
                MbcState::Tpp1 {
                    mr0: 1,
                    mr1: 0,
                    mr2: 0,
                    mapping: Tpp1Mapping::ControlRegisters,
                    rumble_speed: 0,
                    has_rumble: features & 0x01 != 0,
                    has_multi_rumble: features & 0x02 != 0,
                    has_battery: features & 0x08 != 0,
                    rtc: has_rtc.then(|| Tpp1Rtc::new(now)),
                }
            }
            // Fallback: treat unsupported mappers as ROM-only so homebrew/test
            // harnesses (and some misheadered dumps) still run.
            MbcType::Unknown(_) => MbcState::NoMbc,
        };

        Self {
            rom: data,
            // Many titles assume powered-on cart RAM reads as $FF.
            ram: vec![0xFF; ram_size],
            mbc,
            cgb,
            title,
            cart_type,
            save_path: None,
            rtc_path: None,
            mbc_state,
            cart_bus: Cell::new(0xFF),
        }
    }

    /// Read a byte from the cartridge bus, updating the open-bus latch.
    pub fn read(&mut self, addr: u16) -> u8 {
        let open_bus = Self::open_bus(&self.cart_bus);
        self.read_with_open_bus(addr, open_bus)
    }

    /// Read a byte from the cartridge bus using a caller-supplied open-bus value.
    pub fn read_with_open_bus(&mut self, addr: u16, open_bus: u8) -> u8 {
        let rom_bank_count = (self.rom.len() / 0x4000).max(1);
        let cart_bus = &self.cart_bus;
        match (&mut self.mbc_state, addr) {
            (MbcState::NoMbc, 0x0000..=0x7FFF) => Self::bus_read(
                cart_bus,
                self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            ),
            (MbcState::Mbc2 { .. }, 0x0000..=0x3FFF) => Self::bus_read(
                cart_bus,
                self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            ),
            (MbcState::Mbc2 { rom_bank, .. }, 0x4000..=0x7FFF) => {
                let mut bank = (*rom_bank & 0x0F) as usize;
                if bank == 0 {
                    bank = 1;
                }
                bank %= rom_bank_count;
                let offset = bank * 0x4000 + (addr as usize - 0x4000);
                Self::bus_read(cart_bus, self.rom.get(offset).copied().unwrap_or(0xFF))
            }
            (
                MbcState::Mbc1 {
                    ram_bank,
                    mode,
                    multicart,
                    ..
                },
                0x0000..=0x3FFF,
            ) => {
                let bank = if *mode == 0 {
                    0
                } else if *multicart {
                    (((*ram_bank as usize) & 0x03) << 4) % rom_bank_count
                } else {
                    (((*ram_bank as usize) & 0x03) << 5) % rom_bank_count
                };
                let offset = bank * 0x4000 + addr as usize;
                Self::bus_read(cart_bus, self.rom.get(offset).copied().unwrap_or(0xFF))
            }
            (
                MbcState::Mbc1 {
                    rom_bank,
                    ram_bank,
                    mode: _,
                    multicart,
                    ..
                },
                0x4000..=0x7FFF,
            ) => {
                let bank = if *multicart {
                    let high = ((*ram_bank as usize) & 0x03) << 4;
                    let raw = *rom_bank as usize & 0x1F;
                    let low4 = raw & 0x0F;
                    let bit4 = (raw & 0x10) != 0;
                    let low = if low4 == 0 && !bit4 { 1 } else { low4 };
                    (high | low) % rom_bank_count
                } else {
                    let high = ((*ram_bank as usize) & 0x03) << 5;
                    let mut bank = high | (*rom_bank as usize & 0x1F);
                    if bank & 0x1F == 0 {
                        bank += 1;
                    }
                    bank % rom_bank_count
                };
                let offset = bank * 0x4000 + (addr as usize - 0x4000);
                Self::bus_read(cart_bus, self.rom.get(offset).copied().unwrap_or(0xFF))
            }
            (MbcState::Mbc3 { .. }, 0x0000..=0x3FFF)
            | (MbcState::Mbc30 { .. }, 0x0000..=0x3FFF) => Self::bus_read(
                cart_bus,
                self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            ),
            (MbcState::Mbc3 { rom_bank, .. }, 0x4000..=0x7FFF)
            | (MbcState::Mbc30 { rom_bank, .. }, 0x4000..=0x7FFF) => {
                let mut bank = (*rom_bank as usize) % rom_bank_count;
                if bank == 0 && rom_bank_count > 1 {
                    bank = 1;
                }
                let offset = bank * 0x4000 + (addr as usize - 0x4000);
                Self::bus_read(cart_bus, self.rom.get(offset).copied().unwrap_or(0xFF))
            }
            (MbcState::Mbc5 { .. }, 0x0000..=0x3FFF) => Self::bus_read(
                cart_bus,
                self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            ),
            (MbcState::Mbc5 { rom_bank, .. }, 0x4000..=0x7FFF) => {
                let bank = (*rom_bank as usize) % rom_bank_count;
                let offset = bank * 0x4000 + (addr as usize - 0x4000);
                Self::bus_read(cart_bus, self.rom.get(offset).copied().unwrap_or(0xFF))
            }
            (MbcState::NoMbc, 0xA000..=0xBFFF) => {
                let idx = self.ram_index(addr);
                Self::bus_read(cart_bus, self.ram.get(idx).copied().unwrap_or(0xFF))
            }
            (MbcState::Mbc2 { ram_enable, .. }, 0xA000..=0xBFFF) => {
                if !*ram_enable {
                    0xFF
                } else {
                    // MBC2 has 512x4-bit internal RAM, mirrored across 0xA000-0xBFFF.
                    let idx = (addr as usize - 0xA000) & 0x01FF;
                    let nibble = self.ram.get(idx).copied().unwrap_or(0x0F) & 0x0F;
                    Self::bus_read(cart_bus, 0xF0 | nibble)
                }
            }
            (MbcState::Mbc1 { ram_enable, .. }, 0xA000..=0xBFFF) => {
                if !*ram_enable {
                    0xFF
                } else {
                    let idx = self.ram_index(addr);
                    Self::bus_read(cart_bus, self.ram.get(idx).copied().unwrap_or(0xFF))
                }
            }
            (
                MbcState::Mbc3 {
                    ram_enable,
                    ram_bank,
                    rtc,
                    ..
                },
                0xA000..=0xBFFF,
            ) => {
                if !*ram_enable {
                    return Self::bus_read(cart_bus, 0xFF);
                }

                match *ram_bank {
                    0x00..=0x03 => {
                        if self.ram.is_empty() {
                            Self::bus_read(cart_bus, open_bus)
                        } else {
                            let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                            Self::bus_read(cart_bus, self.read_ram_wrapped(idx))
                        }
                    }
                    0x04..=0x07 => Self::bus_read(cart_bus, 0xFF),
                    0x08..=0x0C => {
                        if let Some(rtc) = rtc.as_ref() {
                            Self::bus_read(cart_bus, rtc.read_latched(*ram_bank))
                        } else {
                            Self::bus_read(cart_bus, 0xFF)
                        }
                    }
                    _ => Self::bus_read(cart_bus, 0xFF),
                }
            }
            (
                MbcState::Mbc30 {
                    ram_enable,
                    ram_bank,
                    rtc,
                    ..
                },
                0xA000..=0xBFFF,
            ) => {
                if !*ram_enable {
                    return Self::bus_read(cart_bus, 0xFF);
                }

                match *ram_bank {
                    0x00..=0x07 => {
                        if self.ram.is_empty() {
                            Self::bus_read(cart_bus, open_bus)
                        } else {
                            let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                            Self::bus_read(cart_bus, self.read_ram_wrapped(idx))
                        }
                    }
                    0x08..=0x0C => {
                        if let Some(rtc) = rtc.as_ref() {
                            Self::bus_read(cart_bus, rtc.read_latched(*ram_bank))
                        } else {
                            Self::bus_read(cart_bus, 0xFF)
                        }
                    }
                    _ => Self::bus_read(cart_bus, 0xFF),
                }
            }
            (MbcState::Mbc5 { ram_enable, .. }, 0xA000..=0xBFFF) => {
                if !*ram_enable {
                    0xFF
                } else {
                    let idx = self.ram_index(addr);
                    Self::bus_read(cart_bus, self.ram.get(idx).copied().unwrap_or(0xFF))
                }
            }
            (MbcState::Tpp1 { .. }, 0x0000..=0x3FFF) => Self::bus_read(
                cart_bus,
                self.rom.get(addr as usize).copied().unwrap_or(0xFF),
            ),
            (MbcState::Tpp1 { mr0, mr1, .. }, 0x4000..=0x7FFF) => {
                let bank = ((*mr1 as usize) << 8) | *mr0 as usize;
                let bank = if rom_bank_count > 0 {
                    bank % rom_bank_count
                } else {
                    0
                };
                let offset = bank * 0x4000 + (addr as usize - 0x4000);
                Self::bus_read(cart_bus, self.rom.get(offset).copied().unwrap_or(0xFF))
            }
            (
                MbcState::Tpp1 {
                    mapping,
                    mr0,
                    mr1,
                    mr2,
                    rumble_speed,
                    rtc,
                    ..
                },
                0xA000..=0xBFFF,
            ) => match mapping {
                Tpp1Mapping::ControlRegisters => {
                    let val = match addr & 0x03 {
                        0 => *mr0,
                        1 => *mr1,
                        2 => *mr2,
                        3 => {
                            let mut mr4 = 0xF0;
                            mr4 |= *rumble_speed & 0x03;
                            if let Some(rtc) = rtc.as_ref() {
                                mr4 |= rtc.mr4_bits();
                            }
                            mr4
                        }
                        _ => unreachable!(),
                    };
                    Self::bus_read(cart_bus, val)
                }
                Tpp1Mapping::SramReadOnly | Tpp1Mapping::SramReadWrite => {
                    let idx = (*mr2 as usize) * 0x2000 + (addr as usize - 0xA000);
                    Self::bus_read(cart_bus, self.read_ram_wrapped(idx))
                }
                Tpp1Mapping::RtcLatched => {
                    if let Some(rtc) = rtc.as_ref() {
                        Self::bus_read(cart_bus, rtc.read_latched(addr))
                    } else {
                        Self::bus_read(cart_bus, 0xFF)
                    }
                }
            },
            _ => 0xFF,
        }
    }

    /// Write a byte to the cartridge bus (MBC register or RAM write).
    pub fn write(&mut self, addr: u16, val: u8) {
        let cart_bus = &self.cart_bus;
        // CPU drives the cart data bus on writes too.
        if matches!(addr, 0x0000..=0x7FFF | 0xA000..=0xBFFF) {
            Self::bus_write(cart_bus, val);
        }
        match (&mut self.mbc_state, addr) {
            (MbcState::NoMbc, 0xA000..=0xBFFF) => {
                let idx = self.ram_index(addr);
                if let Some(b) = self.ram.get_mut(idx) {
                    *b = val;
                }
            }
            (
                MbcState::Mbc2 {
                    rom_bank,
                    ram_enable,
                },
                0x0000..=0x3FFF,
            ) => {
                // MBC2 uses address bit 8 to select between RAMG and ROMB across the
                // entire 0x0000-0x3FFF range:
                // - bit8=0: RAM enable (RAMG)
                // - bit8=1: ROM bank select (ROMB)
                if (addr & 0x0100) == 0 {
                    *ram_enable = val & 0x0F == 0x0A;
                } else {
                    *rom_bank = val & 0x0F;
                    if *rom_bank == 0 {
                        *rom_bank = 1;
                    }
                }
            }
            (MbcState::Mbc2 { ram_enable, .. }, 0xA000..=0xBFFF) => {
                if *ram_enable {
                    let idx = (addr as usize - 0xA000) & 0x01FF;
                    if let Some(b) = self.ram.get_mut(idx) {
                        *b = val & 0x0F;
                    }
                }
            }
            (MbcState::Mbc1 { ram_enable, .. }, 0x0000..=0x1FFF) => {
                *ram_enable = val & 0x0F == 0x0A;
            }
            (MbcState::Mbc1 { rom_bank, .. }, 0x2000..=0x3FFF) => {
                *rom_bank = val & 0x1F;
                if *rom_bank == 0 {
                    *rom_bank = 1;
                }
            }
            (MbcState::Mbc1 { ram_bank, .. }, 0x4000..=0x5FFF) => {
                *ram_bank = val & 0x03;
            }
            (MbcState::Mbc1 { mode, .. }, 0x6000..=0x7FFF) => {
                *mode = val & 0x01;
            }
            (
                MbcState::Mbc1 {
                    ram_enable,
                    ram_bank: _,
                    mode: _,
                    ..
                },
                0xA000..=0xBFFF,
            ) => {
                if *ram_enable {
                    // For small RAM sizes (e.g. 2KB/8KB), MBC1 always maps to the
                    // single available bank regardless of bank register writes.
                    // ram_index() handles wrapping.
                    let idx = self.ram_index(addr);
                    if let Some(b) = self.ram.get_mut(idx) {
                        *b = val;
                    }
                }
            }
            (MbcState::Mbc3 { ram_enable, .. }, 0x0000..=0x1FFF)
            | (MbcState::Mbc30 { ram_enable, .. }, 0x0000..=0x1FFF) => {
                *ram_enable = val & 0x0F == 0x0A;
            }
            (MbcState::Mbc3 { rom_bank, .. }, 0x2000..=0x3FFF) => {
                *rom_bank = val & 0x7F;
                if *rom_bank == 0 {
                    *rom_bank = 1;
                }
            }
            (MbcState::Mbc30 { rom_bank, .. }, 0x2000..=0x3FFF) => {
                *rom_bank = val;
                if *rom_bank == 0 {
                    *rom_bank = 1;
                }
            }
            (MbcState::Mbc3 { ram_bank, .. }, 0x4000..=0x5FFF) => {
                *ram_bank = val & 0x0F;
            }
            (MbcState::Mbc30 { ram_bank, .. }, 0x4000..=0x5FFF) => {
                *ram_bank = val & 0x0F;
            }
            // Latch on any write to $6000-$7FFF.
            (MbcState::Mbc3 { rtc: Some(rtc), .. }, 0x6000..=0x7FFF) => rtc.latch(),
            (MbcState::Mbc3 { rtc: None, .. }, 0x6000..=0x7FFF) => {}
            (MbcState::Mbc30 { rtc: Some(rtc), .. }, 0x6000..=0x7FFF) => rtc.latch(),
            (MbcState::Mbc30 { rtc: None, .. }, 0x6000..=0x7FFF) => {}
            (
                MbcState::Mbc3 {
                    ram_enable,
                    ram_bank,
                    rtc,
                    ..
                },
                0xA000..=0xBFFF,
            ) => {
                if *ram_enable {
                    match *ram_bank {
                        0x00..=0x03 => {
                            let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                            if !self.ram.is_empty() {
                                let wrapped = idx % self.ram.len();
                                self.ram[wrapped] = val;
                            }
                        }
                        0x08..=0x0C => {
                            if let Some(rtc) = rtc.as_mut() {
                                rtc.write_register(*ram_bank, val);
                            }
                        }
                        _ => {}
                    }
                }
            }
            (
                MbcState::Mbc30 {
                    ram_enable,
                    ram_bank,
                    rtc,
                    ..
                },
                0xA000..=0xBFFF,
            ) => {
                if *ram_enable {
                    match *ram_bank {
                        0x00..=0x07 => {
                            let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                            if !self.ram.is_empty() {
                                let wrapped = idx % self.ram.len();
                                self.ram[wrapped] = val;
                            }
                        }
                        0x08..=0x0C => {
                            if let Some(rtc) = rtc.as_mut() {
                                rtc.write_register(*ram_bank, val);
                            }
                        }
                        _ => {}
                    }
                }
            }
            (MbcState::Mbc5 { ram_enable, .. }, 0x0000..=0x1FFF) => {
                // This mapper expects the enable value to be exactly $0A.
                *ram_enable = val == 0x0A;
            }
            (MbcState::Mbc5 { rom_bank, .. }, 0x2000..=0x2FFF) => {
                *rom_bank = (*rom_bank & 0x100) | val as u16;
            }
            (MbcState::Mbc5 { rom_bank, .. }, 0x3000..=0x3FFF) => {
                *rom_bank = (*rom_bank & 0xFF) | (((val & 0x01) as u16) << 8);
            }
            (MbcState::Mbc5 { ram_bank, .. }, 0x4000..=0x5FFF) => {
                *ram_bank = val & 0x0F;
            }
            (
                MbcState::Mbc5 {
                    ram_enable,
                    ram_bank,
                    ..
                },
                0xA000..=0xBFFF,
            ) => {
                if *ram_enable {
                    let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                    if !self.ram.is_empty() {
                        let wrapped = idx % self.ram.len();
                        self.ram[wrapped] = val;
                    }
                }
            }
            (
                MbcState::Tpp1 {
                    mr0,
                    mr1,
                    mr2,
                    mapping,
                    rumble_speed,
                    has_rumble,
                    has_multi_rumble,
                    rtc,
                    ..
                },
                0x0000..=0x3FFF,
            ) => match addr & 0x03 {
                0 => *mr0 = val,
                1 => *mr1 = val,
                2 => *mr2 = val,
                3 => match val {
                    0x00 => *mapping = Tpp1Mapping::ControlRegisters,
                    0x02 => *mapping = Tpp1Mapping::SramReadOnly,
                    0x03 => *mapping = Tpp1Mapping::SramReadWrite,
                    0x05 => *mapping = Tpp1Mapping::RtcLatched,
                    0x10 => {
                        if let Some(rtc) = rtc.as_mut() {
                            rtc.latch();
                        }
                    }
                    0x11 => {
                        if let Some(rtc) = rtc.as_mut() {
                            rtc.set_from_latch();
                        }
                    }
                    0x14 => {
                        if let Some(rtc) = rtc.as_mut() {
                            rtc.overflow = false;
                        }
                    }
                    0x18 => {
                        if let Some(rtc) = rtc.as_mut() {
                            rtc.running = false;
                        }
                    }
                    0x19 => {
                        if let Some(rtc) = rtc.as_mut() {
                            rtc.running = true;
                        }
                    }
                    0x20 => *rumble_speed = 0,
                    0x21..=0x23 => {
                        if !*has_rumble {
                            *rumble_speed = 0;
                        } else if !*has_multi_rumble {
                            *rumble_speed = 1;
                        } else {
                            *rumble_speed = val - 0x20;
                        }
                    }
                    _ => {}
                },
                _ => unreachable!(),
            },
            (
                MbcState::Tpp1 {
                    mapping, mr2, rtc, ..
                },
                0xA000..=0xBFFF,
            ) => match mapping {
                Tpp1Mapping::SramReadWrite => {
                    let idx = (*mr2 as usize) * 0x2000 + (addr as usize - 0xA000);
                    if !self.ram.is_empty() {
                        let wrapped = idx % self.ram.len();
                        self.ram[wrapped] = val;
                    }
                }
                Tpp1Mapping::RtcLatched => {
                    if let Some(rtc) = rtc.as_mut() {
                        rtc.write_latched(addr, val);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn wrap_ram_index(&self, idx: usize) -> usize {
        if self.ram.is_empty() {
            0
        } else {
            idx % self.ram.len()
        }
    }

    fn read_ram_wrapped(&self, idx: usize) -> u8 {
        if self.ram.is_empty() {
            0xFF
        } else {
            self.ram[self.wrap_ram_index(idx)]
        }
    }

    fn ram_index(&self, addr: u16) -> usize {
        let ram_bank_count = if self.ram.is_empty() {
            0
        } else {
            (self.ram.len().saturating_add(0x1FFF)) / 0x2000
        };
        let idx = match &self.mbc_state {
            MbcState::NoMbc => addr as usize - 0xA000,
            MbcState::Mbc2 { .. } => (addr as usize - 0xA000) & 0x01FF,
            MbcState::Mbc1 { ram_bank, mode, .. } => {
                if *mode == 0 {
                    addr as usize - 0xA000
                } else {
                    let bank = if ram_bank_count == 0 {
                        0
                    } else {
                        (*ram_bank as usize) % ram_bank_count
                    };
                    bank * 0x2000 + addr as usize - 0xA000
                }
            }
            MbcState::Mbc3 { ram_bank, .. } => {
                ((*ram_bank as usize) & 0x03) * 0x2000 + addr as usize - 0xA000
            }
            MbcState::Mbc30 { ram_bank, .. } => {
                ((*ram_bank as usize) & 0x07) * 0x2000 + addr as usize - 0xA000
            }
            MbcState::Mbc5 { ram_bank, .. } => {
                (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000
            }
            MbcState::Tpp1 { mr2, .. } => (*mr2 as usize) * 0x2000 + addr as usize - 0xA000,
            MbcState::Unknown => addr as usize - 0xA000,
        };
        self.wrap_ram_index(idx)
    }

    fn has_battery(&self) -> bool {
        match &self.mbc_state {
            MbcState::Tpp1 { has_battery, .. } => *has_battery,
            _ => matches!(
                self.cart_type,
                0x03 | 0x06 | 0x09 | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E
            ),
        }
    }

    fn has_rtc(&self) -> bool {
        match &self.mbc_state {
            MbcState::Tpp1 { rtc, .. } => rtc.is_some(),
            _ => matches!(self.cart_type, 0x0F | 0x10 | 0x13),
        }
    }

    fn rtc_mut(&mut self) -> Option<&mut Mbc3Rtc> {
        match &mut self.mbc_state {
            MbcState::Mbc3 { rtc: Some(rtc), .. } | MbcState::Mbc30 { rtc: Some(rtc), .. } => {
                Some(rtc)
            }
            _ => None,
        }
    }

    /// Persist battery-backed RAM (and RTC state) to disk if a save path is set.
    pub fn save_ram(&mut self) -> io::Result<()> {
        if let (true, Some(path)) = (self.has_battery(), &self.save_path)
            && !self.ram.is_empty()
        {
            fs::write(path, &self.ram)?;
        }

        if let Some(rtc_path) = self.rtc_path.clone() {
            match &mut self.mbc_state {
                MbcState::Mbc3 { rtc: Some(rtc), .. } | MbcState::Mbc30 { rtc: Some(rtc), .. } => {
                    rtc.mark_persisted(SystemTime::now());
                    fs::write(rtc_path, rtc.serialize())?;
                }
                MbcState::Tpp1 { rtc: Some(rtc), .. } => {
                    rtc.mark_persisted(SystemTime::now());
                    fs::write(rtc_path, rtc.serialize())?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn detect_mbc1_multicart(rom: &[u8]) -> bool {
    // Mooneye's MBC1 multicart test targets the common 8 Mbit (64 bank) wiring.
    // This hardware variant can't be reliably detected from the header alone,
    // so we use a conservative heuristic: many multicart dumps include a copy
    // of the header logo in multiple banks (bank0+bank1+bank2...).
    let bank_count = rom.len() / 0x4000;
    if bank_count != 64 {
        return false;
    }

    let logo0 = match rom.get(0x0104..0x0134) {
        Some(s) if !s.iter().all(|&b| b == 0) => s,
        _ => return false,
    };

    for bank in 1..=2 {
        let start = bank * 0x4000 + 0x0104;
        let end = start + 0x30;
        match rom.get(start..end) {
            Some(s) if s == logo0 => {}
            _ => return false,
        }
    }

    true
}

struct Header<'a> {
    data: &'a [u8],
}

impl<'a> Header<'a> {
    fn parse(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn title(&self) -> String {
        let end = 0x0143.min(self.data.len());
        let mut slice = &self.data[0x0134.min(self.data.len())..end];
        if let Some(pos) = slice.iter().position(|&b| b == 0) {
            slice = &slice[..pos];
        }
        String::from_utf8_lossy(slice).trim().to_string()
    }

    fn cgb_supported(&self) -> bool {
        self.data.get(0x0143).copied().unwrap_or(0) & 0x80 != 0
    }

    fn is_tpp1(&self) -> bool {
        self.data.len() >= 0x154
            && self.data.get(0x0147).copied() == Some(0xBC)
            && self.data.get(0x0149).copied() == Some(0xC1)
            && self.data.get(0x014A).copied() == Some(0x65)
    }

    fn tpp1_features(&self) -> u8 {
        self.data.get(0x0153).copied().unwrap_or(0)
    }

    fn mbc_type(&self) -> MbcType {
        if self.data.len() < 0x150 {
            return MbcType::NoMbc;
        }
        if self.is_tpp1() {
            return MbcType::Tpp1;
        }
        let cart = self.data.get(0x0147).copied().unwrap_or(0);
        match cart {
            0x00 => MbcType::NoMbc,
            0x01..=0x03 => MbcType::Mbc1,
            0x05 | 0x06 => MbcType::Mbc2,
            0x0F..=0x13 => {
                // Treat large MBC3 carts (ROM > 2MB or RAM > 32KB) as MBC30.
                if self.data.len() > 0x200000 || self.ram_size() > 0x8000 {
                    MbcType::Mbc30
                } else {
                    MbcType::Mbc3
                }
            }
            0x19..=0x1E => MbcType::Mbc5,
            other => MbcType::Unknown(other),
        }
    }

    fn cart_type(&self) -> u8 {
        if self.data.len() < 0x150 {
            return 0x00;
        }
        self.data.get(0x0147).copied().unwrap_or(0)
    }

    fn has_rtc(&self) -> bool {
        if self.is_tpp1() {
            return self.tpp1_features() & 0x04 != 0;
        }
        matches!(self.cart_type(), 0x0F | 0x10 | 0x13)
    }

    fn ram_size(&self) -> usize {
        if self.data.len() < 0x150 {
            return 0x2000;
        }

        if self.is_tpp1() {
            let val = self.data.get(0x0152).copied().unwrap_or(0);
            return if val == 0 {
                0
            } else {
                // Shift count: 1..=9 → 1, 2, 4, 8, 16, 32, 64, 128, 256 banks
                (1usize << (val as u32 - 1)) * 0x2000
            };
        }

        // MBC2 has 512x4-bit internal RAM regardless of header RAM size.
        if matches!(self.cart_type(), 0x05 | 0x06) {
            return 0x200;
        }

        match self.data.get(0x0149).copied().unwrap_or(0) {
            0x00 => 0,
            0x01 => 0x800,   // 2KB
            0x02 => 0x2000,  // 8KB
            0x03 => 0x8000,  // 32KB (4 banks)
            0x04 => 0x20000, // 128KB (16 banks)
            0x05 => 0x10000, // 64KB (8 banks)
            _ => 0x2000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms_to_cycles(ms: u64) -> u32 {
        ((ms as u128).saturating_mul(RTC_CYCLES_PER_SECOND as u128) / 1000u128) as u32
    }

    #[test]
    fn rtc_ticks_through_invalid_values() {
        let now = SystemTime::UNIX_EPOCH;
        let mut rtc = Mbc3Rtc::new(now);

        rtc.regs.seconds = 59;
        rtc.regs.minutes = 60;
        rtc.advance_seconds(1);
        assert_eq!(rtc.regs.seconds, 0);
        assert_eq!(rtc.regs.minutes, 61);

        rtc.regs.seconds = 63;
        rtc.regs.minutes = 5;
        rtc.advance_seconds(1);
        assert_eq!(rtc.regs.seconds, 0);
        assert_eq!(rtc.regs.minutes, 5);

        rtc.regs.seconds = 59;
        rtc.regs.minutes = 59;
        rtc.regs.hours = 24;
        rtc.advance_seconds(1);
        assert_eq!(rtc.regs.seconds, 0);
        assert_eq!(rtc.regs.minutes, 0);
        assert_eq!(rtc.regs.hours, 25);
    }

    #[test]
    fn rtc_halt_preserves_subseconds() {
        let start = SystemTime::UNIX_EPOCH;
        let mut rtc = Mbc3Rtc::new(start);
        rtc.subsecond_cycles = RTC_CYCLES_PER_SECOND - 10_000;

        rtc.write_register(0x0C, 0x40);
        rtc.step(RTC_CYCLES_PER_SECOND as u64 * 2);
        assert_eq!(rtc.regs.seconds, 0);

        rtc.write_register(0x0C, 0x00);
        rtc.step(9_999);
        assert_eq!(rtc.regs.seconds, 0);
        rtc.step(1);
        assert_eq!(rtc.regs.seconds, 1);
    }

    #[test]
    fn rtc_seconds_write_resets_phase() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let mut rtc = Mbc3Rtc::new(now);
        rtc.subsecond_cycles = ms_to_cycles(750);

        rtc.step(ms_to_cycles(10) as u64);

        rtc.write_register(0x09, 0x01);
        assert_eq!(rtc.subsecond_cycles, ms_to_cycles(760));

        rtc.write_register(0x08, 0x02);
        assert_eq!(rtc.subsecond_cycles, 0);
    }

    #[test]
    fn rtc_day_overflow_sets_carry() {
        let mut rtc = Mbc3Rtc::new(SystemTime::UNIX_EPOCH);
        rtc.regs.seconds = 59;
        rtc.regs.minutes = 59;
        rtc.regs.hours = 23;
        rtc.regs.days = 0x01FF;

        rtc.advance_seconds(1);
        assert_eq!(rtc.regs.days, 0);
        assert!(rtc.regs.carry);
    }

    #[test]
    fn cart_ram_initializes_to_ff() {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x03; // MBC1 + RAM + BAT
        rom[0x0149] = 0x02; // 8KB RAM

        let cart = Cartridge::load(rom);
        assert!(!cart.ram.is_empty());
        assert!(cart.ram.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn no_mbc_small_ram_mirrors() {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00; // No MBC
        rom[0x0149] = 0x01; // 2KB RAM

        let mut cart = Cartridge::load(rom);
        // Writes to $A800 should mirror to $A000 for 2KB RAM.
        cart.write(0xA800, 0x12);
        assert_eq!(cart.read(0xA000), 0x12);
    }

    #[test]
    fn mbc3_rom_bank_wraps() {
        // 2 ROM banks: bank0 is 0x00 bytes, bank1 is 0x11 bytes.
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x11; // MBC3
        rom[0x0149] = 0x00;
        for b in &mut rom[0x4000..0x8000] {
            *b = 0x11;
        }

        let mut cart = Cartridge::load(rom);
        // Select an out-of-range bank; should wrap to bank 1.
        cart.write(0x2000, 5);
        assert_eq!(cart.read(0x4000), 0x11);
    }

    fn make_tpp1_rom(ram_shift: u8, features: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0xBC;
        rom[0x0148] = 0x01; // 2 << 1 = 4 banks ROM, but we only have 2 in the vec
        rom[0x0149] = 0xC1;
        rom[0x014A] = 0x65;
        rom[0x0150] = 0x01; // major version
        rom[0x0151] = 0x00; // minor version
        rom[0x0152] = ram_shift;
        rom[0x0153] = features;
        for b in &mut rom[0x4000..0x8000] {
            *b = 0xAA;
        }
        rom
    }

    #[test]
    fn tpp1_header_detected() {
        let rom = make_tpp1_rom(1, 0x0C);
        let header = Header::parse(&rom);
        assert!(header.is_tpp1());
        assert_eq!(header.mbc_type(), MbcType::Tpp1);
    }

    #[test]
    fn tpp1_ram_size_parsing() {
        let rom0 = make_tpp1_rom(0, 0);
        let header0 = Header::parse(&rom0);
        assert_eq!(header0.ram_size(), 0);

        let rom1 = make_tpp1_rom(1, 0);
        let header1 = Header::parse(&rom1);
        assert_eq!(header1.ram_size(), 0x2000);

        let rom3 = make_tpp1_rom(3, 0);
        let header3 = Header::parse(&rom3);
        assert_eq!(header3.ram_size(), 4 * 0x2000);
    }

    #[test]
    fn tpp1_features_battery_and_rtc() {
        let rom = make_tpp1_rom(1, 0x0C);
        let cart = Cartridge::load(rom);
        assert!(cart.has_battery());
        assert!(cart.has_rtc());
    }

    #[test]
    fn tpp1_initial_state() {
        let rom = make_tpp1_rom(1, 0x08);
        let mut cart = Cartridge::load(rom);
        assert_eq!(cart.current_rom_bank(), 1);
        assert_eq!(cart.current_ram_bank(), 0);
        // Initial mapping is control registers; reading A000 returns MR0 = 1
        assert_eq!(cart.read(0xA000), 1);
        // MR1 = 0
        assert_eq!(cart.read(0xA001), 0);
    }

    #[test]
    fn tpp1_rom_banking() {
        let rom = make_tpp1_rom(0, 0);
        let mut cart = Cartridge::load(rom);
        // Bank 0 home area reads from 0x0000
        assert_eq!(cart.read(0x0000), 0x00);
        // Bank 1 initially mapped to 0x4000-0x7FFF
        assert_eq!(cart.read(0x4000), 0xAA);
        // Switch to bank 0
        cart.write(0x0000, 0); // MR0 = 0
        cart.write(0x0001, 0); // MR1 = 0
        assert_eq!(cart.read(0x4000), 0x00);
    }

    #[test]
    fn tpp1_sram_read_write() {
        let rom = make_tpp1_rom(1, 0x08);
        let mut cart = Cartridge::load(rom);
        // Enable SRAM read/write
        cart.write(0x0003, 0x03);
        assert!(cart.ram_enabled());
        // Write to SRAM
        cart.write(0xA000, 0x42);
        assert_eq!(cart.read(0xA000), 0x42);
        // Switch to read-only; writes should be ignored
        cart.write(0x0003, 0x02);
        cart.write(0xA000, 0x99);
        assert_eq!(cart.read(0xA000), 0x42);
    }

    #[test]
    fn tpp1_control_register_readback() {
        let rom = make_tpp1_rom(1, 0);
        let mut cart = Cartridge::load(rom);
        // Map control registers
        cart.write(0x0003, 0x00);
        // Write MR0 = 0x05, MR1 = 0x02, MR2 = 0x03
        cart.write(0x0000, 0x05);
        cart.write(0x0001, 0x02);
        cart.write(0x0002, 0x03);
        // Read them back via A000-BFFF
        assert_eq!(cart.read(0xA000), 0x05);
        assert_eq!(cart.read(0xA001), 0x02);
        assert_eq!(cart.read(0xA002), 0x03);
        // MR4 should have rumble=0, no RTC bits
        let mr4 = cart.read(0xA003);
        assert_eq!(mr4 & 0x0F, 0x00);
    }

    #[test]
    fn tpp1_rtc_latch_and_readback() {
        let rom = make_tpp1_rom(0, 0x04);
        let mut cart = Cartridge::load(rom);
        // Start the RTC
        cart.write(0x0003, 0x19);
        // Map RTC latched registers
        cart.write(0x0003, 0x05);
        // Write latch registers directly
        cart.write(0xA000, 10); // RTCW = 10
        cart.write(0xA001, 0x45); // RTCDH = day2 hour5
        cart.write(0xA002, 30); // RTCM = 30
        cart.write(0xA003, 15); // RTCS = 15
        // Set RTC from latch
        cart.write(0x0003, 0x11);
        // Latch RTC back
        cart.write(0x0003, 0x10);
        // Map RTC and read back
        cart.write(0x0003, 0x05);
        assert_eq!(cart.read(0xA000), 10);
        assert_eq!(cart.read(0xA001), 0x45);
        assert_eq!(cart.read(0xA002), 30);
        assert_eq!(cart.read(0xA003), 15);
    }

    #[test]
    fn tpp1_rtc_ticks_seconds() {
        let now = SystemTime::UNIX_EPOCH;
        let mut rtc = Tpp1Rtc::new(now);
        rtc.running = true;
        rtc.regs.rtcs = 58;
        rtc.regs.rtcm = 0;
        rtc.advance_seconds(1);
        assert_eq!(rtc.regs.seconds(), 59);
        rtc.advance_seconds(1);
        assert_eq!(rtc.regs.seconds(), 0);
        assert_eq!(rtc.regs.minutes(), 1);
    }

    #[test]
    fn tpp1_rtc_full_rollover() {
        let now = SystemTime::UNIX_EPOCH;
        let mut rtc = Tpp1Rtc::new(now);
        rtc.running = true;
        rtc.regs.rtcs = 59;
        rtc.regs.rtcm = 59;
        // Day 6, hour 23
        rtc.regs.rtcdh = (6 << 5) | 23;
        rtc.regs.rtcw = 0xFF;
        rtc.advance_seconds(1);
        assert_eq!(rtc.regs.seconds(), 0);
        assert_eq!(rtc.regs.minutes(), 0);
        assert_eq!(rtc.regs.hours(), 0);
        assert_eq!(rtc.regs.day_of_week(), 0);
        assert_eq!(rtc.regs.rtcw, 0);
        assert!(rtc.overflow);
    }

    #[test]
    fn tpp1_rtc_serialize_roundtrip() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let mut rtc = Tpp1Rtc::new(now);
        rtc.running = true;
        rtc.overflow = true;
        rtc.regs.rtcw = 42;
        rtc.regs.rtcdh = (3 << 5) | 12;
        rtc.regs.rtcm = 30;
        rtc.regs.rtcs = 45;
        let data = rtc.serialize();
        let mut rtc2 = Tpp1Rtc::new(SystemTime::UNIX_EPOCH);
        assert!(rtc2.load_from_bytes(&data));
        assert_eq!(rtc2.regs.rtcw, 42);
        assert_eq!(rtc2.regs.rtcdh, (3 << 5) | 12);
        assert_eq!(rtc2.regs.rtcm, 30);
        assert_eq!(rtc2.regs.rtcs, 45);
        assert!(rtc2.running);
        assert!(rtc2.overflow);
    }

    #[test]
    fn tpp1_rumble_speed_clamped() {
        // has_rumble=true, has_multi_rumble=false
        let rom = make_tpp1_rom(0, 0x01);
        let mut cart = Cartridge::load(rom);
        // Map control registers
        cart.write(0x0003, 0x00);
        // Request speed 3
        cart.write(0x0003, 0x23);
        // Should clamp to speed 1
        let mr4 = cart.read(0xA003);
        assert_eq!(mr4 & 0x03, 1);
    }
}
