use crate::{apu::Apu, cartridge::Cartridge, input::Input, ppu::Ppu, serial::Serial, timer::Timer};
use std::sync::{Arc, Mutex};

const WRAM_BANK_SIZE: usize = 0x1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DmaState {
    Idle,
    Gdma { blocks_left: u8, cycles_left: u32 },
    Hdma { src: u16, dst: u16, blocks_left: u8 },
}

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
    pub dma_cycles: u16,
    dma_source: u16,
    pending_dma: Option<u16>,
    pending_delay: u16,
    hdma1: u8,
    hdma2: u8,
    hdma3: u8,
    hdma4: u8,
    hdma5: u8,
    dma_state: DmaState,
    dma_busy: u32,
    hdma_stop: bool,
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
            dma_cycles: 0,
            dma_source: 0,
            pending_dma: None,
            pending_delay: 0,
            hdma1: 0,
            hdma2: 0,
            hdma3: 0,
            hdma4: 0,
            hdma5: 0xFF,
            dma_state: DmaState::Idle,
            dma_busy: 0,
            hdma_stop: false,
            cgb_mode: cgb,
        }
    }

    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    pub fn load_cart(&mut self, cart: Cartridge) {
        let is_dmg = !cart.cgb;
        self.cart = Some(cart);
        if self.cgb_mode && is_dmg {
            self.ppu.apply_dmg_compatibility_palettes();
        }
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
            0xFF51 => self.hdma1 & 0xF0,
            0xFF52 => self.hdma2 & 0xF0,
            0xFF53 => self.hdma3 & 0xF0,
            0xFF54 => self.hdma4 & 0xF0,
            0xFF55 => match self.dma_state {
                DmaState::Idle => 0xFF,
                DmaState::Gdma { blocks_left, .. } | DmaState::Hdma { blocks_left, .. } => {
                    blocks_left.wrapping_sub(1) & 0x7F
                }
            },
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
            0xFF51 => {
                if matches!(self.dma_state, DmaState::Idle) {
                    self.hdma1 = val;
                }
            }
            0xFF52 => {
                if matches!(self.dma_state, DmaState::Idle) {
                    self.hdma2 = val;
                }
            }
            0xFF53 => {
                if matches!(self.dma_state, DmaState::Idle) {
                    self.hdma3 = val;
                }
            }
            0xFF54 => {
                if matches!(self.dma_state, DmaState::Idle) {
                    self.hdma4 = val;
                }
            }
            0xFF55 => self.handle_hdma_write(val),
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
                if self.dma_active() {
                    self.pending_dma = Some(src);
                    self.pending_delay = 4;
                } else {
                    self.dma_source = src;
                    // 1 M-cycle delay (4 cycles) then 160 bytes * 4 cycles each
                    self.dma_cycles = 644;
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

    /// Advance the ongoing OAM DMA transfer if active.
    pub fn dma_step(&mut self, cycles: u16) {
        for _ in 0..cycles {
            if self.pending_delay > 0 {
                self.pending_delay -= 1;
                if self.pending_delay == 0 {
                    if let Some(src) = self.pending_dma.take() {
                        self.dma_source = src;
                        self.dma_cycles = 644;
                    }
                }
            }

            if self.dma_cycles == 0 {
                continue;
            }

            let elapsed = 644 - self.dma_cycles;
            if elapsed >= 4 {
                let progress = elapsed - 4;
                if progress % 4 == 0 && progress / 4 < 0xA0 {
                    let idx: u16 = progress / 4;
                    let byte = self.read_byte(self.dma_source.wrapping_add(idx));
                    self.ppu.oam[idx as usize] = byte;
                }
            }

            self.dma_cycles -= 1;
        }
    }

    /// Return true if a DMA transfer is in progress.
    pub fn dma_active(&self) -> bool {
        self.dma_cycles > 0 || self.pending_delay > 0
    }

    /// Return true if a CGB DMA (GDMA/HDMA) burst is stalling the CPU.
    pub fn cgb_dma_active(&self) -> bool {
        self.dma_busy > 0
    }

    /// Advance timing for the active GDMA or HDMA burst.
    pub fn cgb_dma_step(&mut self, cycles: u16) {
        if self.dma_busy >= cycles as u32 {
            self.dma_busy -= cycles as u32;
        } else {
            self.dma_busy = 0;
        }
        if let DmaState::Gdma {
            ref mut cycles_left,
            ref mut blocks_left,
        } = self.dma_state
        {
            if *cycles_left >= cycles as u32 {
                *cycles_left -= cycles as u32;
            } else {
                *cycles_left = 0;
            }
            let completed_blocks =
                (blocks_left.wrapping_mul(32) as u32).saturating_sub(*cycles_left) / 32;
            let remaining = blocks_left.saturating_sub(completed_blocks as u8);
            if remaining == 0 && *cycles_left == 0 {
                self.dma_state = DmaState::Idle;
                self.hdma5 = 0xFF;
            } else {
                self.hdma5 = remaining.wrapping_sub(1);
                *blocks_left = remaining;
            }
        } else if self.dma_busy == 0 && matches!(self.dma_state, DmaState::Hdma { .. }) {
            // HDMA block finished; nothing to update here
        }
    }

    fn handle_hdma_write(&mut self, val: u8) {
        match self.dma_state {
            DmaState::Hdma { .. } => {
                if val & 0x80 == 0 {
                    // manual stop
                    self.hdma_stop = true;
                    self.hdma5 = 0x7F;
                }
            }
            DmaState::Gdma { .. } => {}
            DmaState::Idle => {
                let blocks = (val & 0x7F).wrapping_add(1);
                let src = ((self.hdma1 as u16) << 8) | (self.hdma2 as u16 & 0xF0);
                let dst = 0x8000 | (((self.hdma3 as u16) << 8) | (self.hdma4 as u16 & 0xF0));

                if val & 0x80 == 0 {
                    // GDMA
                    for i in 0..(blocks as u16 * 0x10) {
                        let b = self.read_byte(src.wrapping_add(i));
                        let addr = dst.wrapping_add(i).wrapping_sub(0x8000);
                        self.ppu.vram[self.ppu.vram_bank][(addr & 0x1FFF) as usize] = b;
                    }
                    self.dma_state = DmaState::Gdma {
                        blocks_left: blocks,
                        cycles_left: blocks as u32 * 32,
                    };
                    self.dma_busy = blocks as u32 * 32;
                    self.hdma5 = blocks.wrapping_sub(1);
                } else {
                    // HDMA
                    self.dma_state = DmaState::Hdma {
                        src,
                        dst,
                        blocks_left: blocks,
                    };
                    self.hdma5 = blocks.wrapping_sub(1);
                }
            }
        }
    }

    /// Called at the start of each H-Blank period.
    pub fn on_hblank(&mut self) {
        if let DmaState::Hdma {
            src,
            dst,
            blocks_left,
        } = self.dma_state
        {
            if blocks_left == 0 {
                return;
            }
            let mut s = src;
            let mut d = dst;
            for _ in 0..16 {
                let b = self.read_byte(s);
                self.ppu.vram[self.ppu.vram_bank][((d - 0x8000) & 0x1FFF) as usize] = b;
                s = s.wrapping_add(1);
                d = d.wrapping_add(1);
            }
            let left = blocks_left - 1;
            self.dma_busy = 32;
            if self.hdma_stop || left == 0 {
                self.dma_state = DmaState::Idle;
                self.hdma5 = 0xFF;
                self.hdma_stop = false;
            } else {
                self.dma_state = DmaState::Hdma {
                    src: s,
                    dst: d,
                    blocks_left: left,
                };
                self.hdma5 = left - 1;
            }
        }
    }
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}
