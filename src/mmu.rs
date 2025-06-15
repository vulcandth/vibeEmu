use crate::{apu::Apu, cartridge::Cartridge, input::Input, ppu::Ppu, serial::Serial, timer::Timer};
use std::sync::{Arc, Mutex};

const WRAM_BANK_SIZE: usize = 0x1000;

pub struct Mmu {
    pub wram: [[u8; WRAM_BANK_SIZE]; 8],
    pub wram_bank: usize,
    pub hram: [u8; 0x7F],
    pub cart: Option<Cartridge>,
    pub boot_rom: Option<Vec<u8>>,
    pub boot_mapped: bool,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub serial: Serial,
    pub ppu: Ppu,
    pub apu: Arc<Mutex<Apu>>,
    pub timer: Timer,
    pub input: Input,
    pub key1: u8,
    pub rp: u8,
    cgb_mode: bool,
}

impl Mmu {
    pub fn new_with_mode(cgb: bool) -> Self {
        let mut timer = Timer::new();
        timer.div = 0xAB00;

        let mut ppu = Ppu::new_with_mode(cgb);
        ppu.apply_boot_state();

        Self {
            wram: [[0; WRAM_BANK_SIZE]; 8],
            wram_bank: 1,
            hram: [0; 0x7F],
            cart: None,
            boot_rom: None,
            boot_mapped: false,
            if_reg: 0xE1,
            ie_reg: 0,
            serial: Serial::new(cgb),
            ppu,
            apu: Arc::new(Mutex::new(Apu::new())),
            timer,
            input: Input::new(),
            key1: if cgb { 0x7E } else { 0 },
            rp: 0,
            cgb_mode: cgb,
        }
    }

    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    pub fn load_cart(&mut self, cart: Cartridge) {
        self.cart = Some(cart);
    }

    pub fn save_cart_ram(&self) {
        if let Some(cart) = &self.cart {
            if let Err(e) = cart.save_ram() {
                eprintln!("Failed to save RAM: {e}");
            }
        }
    }

    pub fn load_boot_rom(&mut self, data: Vec<u8>) {
        self.boot_rom = Some(data);
        self.boot_mapped = true;
    }

    pub fn read_byte(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x00FF if self.boot_mapped => self
                .boot_rom
                .as_ref()
                .and_then(|b| b.get(addr as usize).copied())
                .unwrap_or(0xFF),
            0x0000..=0x7FFF => self.cart.as_ref().map(|c| c.read(addr)).unwrap_or(0xFF),
            0x8000..=0x9FFF => {
                if self.ppu.mode == 3 {
                    0xFF
                } else {
                    self.ppu.vram[self.ppu.vram_bank][(addr - 0x8000) as usize]
                }
            }
            0xA000..=0xBFFF => self.cart.as_ref().map(|c| c.read(addr)).unwrap_or(0xFF),
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.wram[self.wram_bank][(addr - 0xD000) as usize],
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize],
            0xF000..=0xFDFF => self.wram[self.wram_bank][(addr - 0xF000) as usize],
            0xFE00..=0xFE9F => {
                if self.ppu.mode == 2 || self.ppu.mode == 3 {
                    0xFF
                } else {
                    self.ppu.oam[(addr - 0xFE00) as usize]
                }
            }
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00 => self.input.read(),
            0xFF01 | 0xFF02 => self.serial.read(addr),
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_reg,
            0xFF10..=0xFF3F => self.apu.lock().unwrap().read_reg(addr),
            0xFF40..=0xFF4B | 0xFF68..=0xFF6B => self.ppu.read_reg(addr),
            0xFF4D => {
                if self.cgb_mode {
                    (self.key1 & 0x81) | 0x7E
                } else {
                    0xFF
                }
            }
            0xFF56 => {
                if self.cgb_mode {
                    self.rp | 0xC0
                } else {
                    0xFF
                }
            }
            0xFF4F => self.ppu.vram_bank as u8,
            0xFF70 => self.wram_bank as u8,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        match addr {
            0x8000..=0x9FFF => {
                if self.ppu.mode != 3 {
                    self.ppu.vram[self.ppu.vram_bank][(addr - 0x8000) as usize] = val;
                }
            }
            0x0000..=0x7FFF | 0xA000..=0xBFFF => {
                if let Some(cart) = self.cart.as_mut() {
                    cart.write(addr, val);
                }
            }
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize] = val,
            0xD000..=0xDFFF => self.wram[self.wram_bank][(addr - 0xD000) as usize] = val,
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize] = val,
            0xF000..=0xFDFF => self.wram[self.wram_bank][(addr - 0xF000) as usize] = val,
            0xFE00..=0xFE9F => {
                if self.ppu.mode != 2 && self.ppu.mode != 3 {
                    self.ppu.oam[(addr - 0xFE00) as usize] = val;
                }
            }
            0xFEA0..=0xFEFF => {}
            0xFF00 => self.input.write(val),
            0xFF01 | 0xFF02 => self.serial.write(addr, val, &mut self.if_reg),
            0xFF04..=0xFF07 => self.timer.write(addr, val, &mut self.if_reg),
            0xFF0F => self.if_reg = (val & 0x1F) | (self.if_reg & 0xE0),
            0xFF10..=0xFF3F => self.apu.lock().unwrap().write_reg(addr, val),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B | 0xFF68..=0xFF6B => self.ppu.write_reg(addr, val),
            0xFF4D => {
                if self.cgb_mode {
                    self.key1 = (self.key1 & 0x80) | (val & 0x01);
                }
            }
            0xFF56 => {
                if self.cgb_mode {
                    self.rp = val & 0xC1;
                }
            }
            0xFF4F => self.ppu.vram_bank = (val & 0x01) as usize,
            0xFF46 => {
                self.ppu.dma = val;
                let src = (val as u16) << 8;
                for i in 0..0xA0 {
                    let byte = self.read_byte(src.wrapping_add(i));
                    self.ppu.oam[i as usize] = byte;
                }
            }
            0xFF50 => self.boot_mapped = false,
            0xFF70 => {
                let bank = (val & 0x07) as usize;
                self.wram_bank = if bank == 0 { 1 } else { bank };
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = (val & 0x1F) | (self.ie_reg & 0xE0),
            _ => {}
        }
    }

    pub fn take_serial(&mut self) -> Vec<u8> {
        self.serial.take_output()
    }
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}
