use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbcType {
    NoMbc,
    Mbc1,
    Mbc3,
    Mbc30,
    Mbc5,
    Unknown(u8),
}

#[derive(Debug)]
pub struct Cartridge {
    pub rom: Vec<u8>,
    pub ram: Vec<u8>,
    pub mbc: MbcType,
    pub cgb: bool,
    pub title: String,
    cart_type: u8,
    save_path: Option<PathBuf>,
    rtc_path: Option<PathBuf>,
    mbc_state: MbcState,
}

#[derive(Debug)]
enum MbcState {
    NoMbc,
    Mbc1 {
        rom_bank: u8,
        ram_bank: u8,
        mode: u8,
        ram_enable: bool,
    },
    Mbc3 {
        rom_bank: u8,
        ram_bank: u8,
        ram_enable: bool,
        rtc: Option<Mbc3Rtc>,
        latch_pending: bool,
    },
    Mbc30 {
        rom_bank: u8,
        ram_bank: u8,
        ram_enable: bool,
        rtc: Option<Mbc3Rtc>,
        latch_pending: bool,
    },
    Mbc5 {
        rom_bank: u16,
        ram_bank: u8,
        ram_enable: bool,
    },
    Unknown,
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
    last_update: SystemTime,
    subsecond: Duration,
}

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
            last_update: now,
            subsecond: Duration::ZERO,
        }
    }

    fn latch(&mut self, now: SystemTime) {
        self.update(now);
        self.refresh_latch();
    }

    fn refresh_latch(&mut self) {
        self.latched = self.regs;
    }

    fn read_latched(&self, reg: u8) -> u8 {
        match reg {
            0x08 => self.latched.seconds & 0x3F,
            0x09 => self.latched.minutes & 0x3F,
            0x0A => self.latched.hours & 0x1F,
            0x0B => (self.latched.days & 0x00FF) as u8,
            0x0C => self.latched.control_byte(),
            _ => 0xFF,
        }
    }

    fn write_register(&mut self, reg: u8, value: u8, now: SystemTime) {
        self.update(now);
        match reg {
            0x08 => {
                self.regs.seconds = value & 0x3F;
                self.subsecond = Duration::ZERO;
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
        self.refresh_latch();
    }

    fn update(&mut self, now: SystemTime) {
        let elapsed = now.duration_since(self.last_update).unwrap_or_default();
        self.last_update = now;
        if self.regs.halt {
            return;
        }

        let total = elapsed + self.subsecond;
        let seconds = total.as_secs();
        self.subsecond = total - Duration::from_secs(seconds);
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
        data.extend_from_slice(&self.subsecond.subsec_nanos().to_le_bytes());
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
        self.subsecond = Duration::from_nanos(nanos as u64);
        self.regs.seconds = data[17] & 0x3F;
        self.regs.minutes = data[18] & 0x3F;
        self.regs.hours = data[19] & 0x1F;
        self.regs.days = u16::from_le_bytes([data[20], data[21]]) & 0x01FF;

        let flags = data[22];
        self.regs.halt = flags & 0x01 != 0;
        self.regs.carry = flags & 0x02 != 0;
        self.refresh_latch();
        true
    }
}

impl Cartridge {
    pub fn from_bytes_with_ram(data: Vec<u8>, ram_size: usize) -> Self {
        let mut c = Self::load(data);
        c.ram = vec![0; ram_size];
        c
    }

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
            if let Some(rtc) = cart.rtc_mut() {
                if let Ok(bytes) = fs::read(&rtc_path)
                    && !rtc.load_from_bytes(&bytes)
                {
                    eprintln!("Failed to parse RTC data from {}", rtc_path.display());
                }
                rtc.latch(SystemTime::now());
            }
        }

        println!(
            "Loaded ROM: {} (MBC: {:?}, CGB: {})",
            cart.title,
            cart.mbc,
            if cart.cgb { "yes" } else { "no" }
        );
        Ok(cart)
    }

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
            },
            MbcType::Mbc3 => MbcState::Mbc3 {
                rom_bank: 1,
                ram_bank: 0,
                ram_enable: false,
                rtc: has_rtc.then(|| Mbc3Rtc::new(now)),
                latch_pending: false,
            },
            MbcType::Mbc30 => MbcState::Mbc30 {
                rom_bank: 1,
                ram_bank: 0,
                ram_enable: false,
                rtc: has_rtc.then(|| Mbc3Rtc::new(now)),
                latch_pending: false,
            },
            MbcType::Mbc5 => MbcState::Mbc5 {
                rom_bank: 1,
                ram_bank: 0,
                ram_enable: false,
            },
            MbcType::Unknown(_) => MbcState::Unknown,
        };

        Self {
            rom: data,
            ram: vec![0; ram_size],
            mbc,
            cgb,
            title,
            cart_type,
            save_path: None,
            rtc_path: None,
            mbc_state,
        }
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match (&mut self.mbc_state, addr) {
            (MbcState::NoMbc, 0x0000..=0x7FFF) => {
                self.rom.get(addr as usize).copied().unwrap_or(0xFF)
            }
            (MbcState::Mbc1 { ram_bank, mode, .. }, 0x0000..=0x3FFF) => {
                let bank = if *mode == 0 {
                    0
                } else {
                    (*ram_bank as usize) << 5
                };
                let offset = bank * 0x4000 + addr as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            (
                MbcState::Mbc1 {
                    rom_bank,
                    ram_bank,
                    mode,
                    ..
                },
                0x4000..=0x7FFF,
            ) => {
                let high = if *mode == 0 {
                    (*ram_bank as usize) << 5
                } else {
                    0
                };
                let mut bank = high | (*rom_bank as usize & 0x1F);
                if bank & 0x1F == 0 {
                    bank += 1;
                }
                let offset = bank * 0x4000 + (addr as usize - 0x4000);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            (MbcState::Mbc3 { .. }, 0x0000..=0x3FFF)
            | (MbcState::Mbc30 { .. }, 0x0000..=0x3FFF) => {
                self.rom.get(addr as usize).copied().unwrap_or(0xFF)
            }
            (MbcState::Mbc3 { rom_bank, .. }, 0x4000..=0x7FFF)
            | (MbcState::Mbc30 { rom_bank, .. }, 0x4000..=0x7FFF) => {
                let bank = if *rom_bank == 0 { 1 } else { *rom_bank } as usize;
                let offset = bank * 0x4000 + (addr as usize - 0x4000);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            (MbcState::Mbc5 { .. }, 0x0000..=0x3FFF) => {
                self.rom.get(addr as usize).copied().unwrap_or(0xFF)
            }
            (MbcState::Mbc5 { rom_bank, .. }, 0x4000..=0x7FFF) => {
                let offset = (*rom_bank as usize) * 0x4000 + (addr as usize - 0x4000);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            (MbcState::NoMbc, 0xA000..=0xBFFF) => {
                let idx = self.ram_index(addr);
                self.ram.get(idx).copied().unwrap_or(0xFF)
            }
            (MbcState::Mbc1 { ram_enable, .. }, 0xA000..=0xBFFF) => {
                if !*ram_enable {
                    0xFF
                } else {
                    let idx = self.ram_index(addr);
                    self.ram.get(idx).copied().unwrap_or(0xFF)
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
                    0xFF
                } else {
                    match *ram_bank {
                        0x00..=0x03 => {
                            let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                            self.ram.get(idx).copied().unwrap_or(0xFF)
                        }
                        0x08..=0x0C => rtc
                            .as_ref()
                            .map(|r| r.read_latched(*ram_bank))
                            .unwrap_or(0xFF),
                        _ => 0xFF,
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
                if !*ram_enable {
                    0xFF
                } else {
                    match *ram_bank {
                        0x00..=0x07 => {
                            let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                            self.ram.get(idx).copied().unwrap_or(0xFF)
                        }
                        0x08..=0x0C => rtc
                            .as_ref()
                            .map(|r| r.read_latched(*ram_bank))
                            .unwrap_or(0xFF),
                        _ => 0xFF,
                    }
                }
            }
            (MbcState::Mbc5 { ram_enable, .. }, 0xA000..=0xBFFF) => {
                if !*ram_enable {
                    0xFF
                } else {
                    let idx = self.ram_index(addr);
                    self.ram.get(idx).copied().unwrap_or(0xFF)
                }
            }
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match (&mut self.mbc_state, addr) {
            (MbcState::NoMbc, 0xA000..=0xBFFF) => {
                let idx = addr as usize - 0xA000;
                if let Some(b) = self.ram.get_mut(idx) {
                    *b = val;
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
                    ram_bank,
                    mode,
                    ..
                },
                0xA000..=0xBFFF,
            ) => {
                if *ram_enable {
                    let idx = if *mode == 0 {
                        addr as usize - 0xA000
                    } else {
                        (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000
                    };
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
                *ram_bank = val;
            }
            (MbcState::Mbc30 { ram_bank, .. }, 0x4000..=0x5FFF) => {
                *ram_bank = val & 0x0F;
            }
            (
                MbcState::Mbc3 {
                    latch_pending, rtc, ..
                },
                0x6000..=0x7FFF,
            ) => {
                if val == 0 {
                    *latch_pending = true;
                } else if val == 1 && *latch_pending {
                    if let Some(rtc) = rtc {
                        rtc.latch(SystemTime::now());
                    }
                    *latch_pending = false;
                } else {
                    *latch_pending = false;
                }
            }
            (
                MbcState::Mbc30 {
                    latch_pending, rtc, ..
                },
                0x6000..=0x7FFF,
            ) => {
                if val == 0 {
                    *latch_pending = true;
                } else if val == 1 && *latch_pending {
                    if let Some(rtc) = rtc {
                        rtc.latch(SystemTime::now());
                    }
                    *latch_pending = false;
                } else {
                    *latch_pending = false;
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
                if *ram_enable {
                    match *ram_bank {
                        0x00..=0x03 => {
                            let idx = (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000;
                            if let Some(b) = self.ram.get_mut(idx) {
                                *b = val;
                            }
                        }
                        0x08..=0x0C => {
                            if let Some(rtc) = rtc.as_mut() {
                                rtc.write_register(*ram_bank, val, SystemTime::now());
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
                            if let Some(b) = self.ram.get_mut(idx) {
                                *b = val;
                            }
                        }
                        0x08..=0x0C => {
                            if let Some(rtc) = rtc.as_mut() {
                                rtc.write_register(*ram_bank, val, SystemTime::now());
                            }
                        }
                        _ => {}
                    }
                }
            }
            (MbcState::Mbc5 { ram_enable, .. }, 0x0000..=0x1FFF) => {
                *ram_enable = val & 0x0F == 0x0A;
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
                    if let Some(b) = self.ram.get_mut(idx) {
                        *b = val;
                    }
                }
            }
            _ => {}
        }
    }

    fn ram_index(&self, addr: u16) -> usize {
        match &self.mbc_state {
            MbcState::NoMbc => addr as usize - 0xA000,
            MbcState::Mbc1 { ram_bank, mode, .. } => {
                if *mode == 0 {
                    addr as usize - 0xA000
                } else {
                    (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000
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
            MbcState::Unknown => addr as usize - 0xA000,
        }
    }

    fn has_battery(&self) -> bool {
        matches!(
            self.cart_type,
            0x03 | 0x06 | 0x09 | 0x0F | 0x10 | 0x13 | 0x1B | 0x1E
        )
    }

    fn has_rtc(&self) -> bool {
        matches!(self.cart_type, 0x0F | 0x10 | 0x13)
    }

    fn rtc_mut(&mut self) -> Option<&mut Mbc3Rtc> {
        match &mut self.mbc_state {
            MbcState::Mbc3 { rtc: Some(rtc), .. } | MbcState::Mbc30 { rtc: Some(rtc), .. } => {
                Some(rtc)
            }
            _ => None,
        }
    }

    pub fn save_ram(&mut self) -> io::Result<()> {
        if let (true, Some(path)) = (self.has_battery(), &self.save_path)
            && !self.ram.is_empty()
        {
            fs::write(path, &self.ram)?;
        }

        let rtc_path = self.rtc_path.clone();
        if let (Some(path), Some(rtc)) = (rtc_path, self.rtc_mut()) {
            rtc.update(SystemTime::now());
            fs::write(path, rtc.serialize())?;
        }
        Ok(())
    }
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

    fn mbc_type(&self) -> MbcType {
        if self.data.len() < 0x150 {
            return MbcType::NoMbc;
        }
        let cart = self.data.get(0x0147).copied().unwrap_or(0);
        let ram_code = self.data.get(0x0149).copied().unwrap_or(0);
        match cart {
            0x00 => MbcType::NoMbc,
            0x01..=0x03 => MbcType::Mbc1,
            0x0F..=0x13 => {
                if ram_code == 0x05 {
                    MbcType::Mbc30
                } else {
                    MbcType::Mbc3
                }
            }
            0x19..=0x1E => MbcType::Mbc5,
            _ => MbcType::NoMbc,
        }
    }

    fn cart_type(&self) -> u8 {
        if self.data.len() < 0x150 {
            return 0x00;
        }
        self.data.get(0x0147).copied().unwrap_or(0)
    }

    fn has_rtc(&self) -> bool {
        matches!(self.cart_type(), 0x0F | 0x10 | 0x13)
    }

    fn ram_size(&self) -> usize {
        if self.data.len() < 0x150 {
            return 0x2000;
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
        rtc.subsecond = Duration::from_millis(600);

        rtc.write_register(0x0C, 0x40, start);
        rtc.update(start + Duration::from_secs(2));
        assert_eq!(rtc.regs.seconds, 0);

        let resume_time = start + Duration::from_secs(2);
        rtc.write_register(0x0C, 0x00, resume_time);
        rtc.update(resume_time + Duration::from_millis(300));
        assert_eq!(rtc.regs.seconds, 0);
        rtc.update(resume_time + Duration::from_millis(400));
        assert_eq!(rtc.regs.seconds, 1);
    }

    #[test]
    fn rtc_seconds_write_resets_phase() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let mut rtc = Mbc3Rtc::new(now);
        rtc.subsecond = Duration::from_millis(750);

        rtc.write_register(0x09, 0x01, now + Duration::from_millis(10));
        assert_eq!(rtc.subsecond, Duration::from_millis(760));

        rtc.write_register(0x08, 0x02, now + Duration::from_millis(20));
        assert_eq!(rtc.subsecond, Duration::ZERO);
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
}
