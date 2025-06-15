use std::{
    fs, io,
    path::{Path, PathBuf},
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
    },
    Mbc30 {
        rom_bank: u8,
        ram_bank: u8,
        ram_enable: bool,
    },
    Mbc5 {
        rom_bank: u16,
        ram_bank: u8,
        ram_enable: bool,
    },
    Unknown,
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
        let mbc = header.mbc_type();
        let cgb = header.cgb_supported();
        let title = header.title();

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
            },
            MbcType::Mbc30 => MbcState::Mbc30 {
                rom_bank: 1,
                ram_bank: 0,
                ram_enable: false,
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
            mbc_state,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match (&self.mbc_state, addr) {
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
            (MbcState::Mbc1 { ram_enable, .. }, 0xA000..=0xBFFF)
            | (MbcState::Mbc3 { ram_enable, .. }, 0xA000..=0xBFFF)
            | (MbcState::Mbc30 { ram_enable, .. }, 0xA000..=0xBFFF)
            | (MbcState::Mbc5 { ram_enable, .. }, 0xA000..=0xBFFF) => {
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
                *ram_bank = val & 0x03;
            }
            (MbcState::Mbc30 { ram_bank, .. }, 0x4000..=0x5FFF) => {
                *ram_bank = val & 0x07;
            }
            (
                MbcState::Mbc3 {
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
            (
                MbcState::Mbc30 {
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
                (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000
            }
            MbcState::Mbc30 { ram_bank, .. } => {
                (*ram_bank as usize) * 0x2000 + addr as usize - 0xA000
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

    pub fn save_ram(&self) -> io::Result<()> {
        if let (true, Some(path)) = (self.has_battery(), &self.save_path) {
            if !self.ram.is_empty() {
                fs::write(path, &self.ram)?;
            }
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
