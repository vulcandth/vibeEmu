use crate::{
    apu::Apu, cartridge::Cartridge, hardware::CgbRevision, input::Input, ppu::Ppu, serial::Serial,
    timer::Timer,
};
use std::sync::{Arc, Mutex};

const WRAM_BANK_SIZE: usize = 0x1000;

/// Transfer mode for CGB DMA operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DmaMode {
    /// General DMA (immediate)
    Gdma,
    /// HBlank DMA
    Hdma,
}

#[derive(Debug)]
struct HdmaState {
    /// 16-bit source pointer (upper 12 bits writable)
    src: u16,
    /// Destination in VRAM (0x8000 | (dst & 0x1FF0))
    dst: u16,
    /// Remaining 0x10-byte blocks (0-7F means 1-128 blocks)
    blocks: u8,
    /// Current DMA mode
    mode: DmaMode,
    /// HDMA active flag
    active: bool,
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
    hdma: HdmaState,
    pub key1: u8,
    pub rp: u8,
    pub dma_cycles: u16,
    dma_source: u16,
    pending_dma: Option<u16>,
    pending_delay: u16,
    /// Remaining stall cycles after a General DMA
    gdma_cycles: u32,
    cgb_mode: bool,
    cgb_revision: CgbRevision,
}

impl Mmu {
    pub fn new_with_mode(cgb: bool) -> Self {
        Self::new_with_config(cgb, CgbRevision::default())
    }

    pub fn new_with_config(cgb: bool, revision: CgbRevision) -> Self {
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
            apu: Arc::new(Mutex::new(Apu::new_with_config(cgb, revision))),
            timer,
            input: Input::new(),
            hdma: HdmaState {
                src: 0,
                dst: 0,
                blocks: 0,
                mode: DmaMode::Gdma,
                active: false,
            },
            key1: if cgb { 0x7E } else { 0 },
            rp: 0,
            dma_cycles: 0,
            dma_source: 0,
            pending_dma: None,
            pending_delay: 0,
            gdma_cycles: 0,
            cgb_mode: cgb,
            cgb_revision: revision,
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
        if let Some(cart) = &self.cart
            && let Err(e) = cart.save_ram()
        {
            eprintln!("Failed to save RAM: {e}");
        }
    }

    pub fn load_boot_rom(&mut self, data: Vec<u8>) {
        self.boot_rom = Some(data);
        self.boot_mapped = true;
    }

    fn read_byte_inner(&mut self, addr: u16, allow_dma: bool) -> u8 {
        if !allow_dma && self.dma_cycles > 0 {
            match addr {
                // allow ROM, WRAM and all I/O/HRAM during DMA
                0x0000..=0x7FFF | 0xC000..=0xFDFF | 0xFF00..=0xFFFF => {}
                // block accesses within OAM (and forbidden gap)
                0xFE00..=0xFEFF => return 0xFF,
                // block VRAM and other regions
                _ => return 0xFF,
            }
        }
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
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B | 0xFF68..=0xFF6B => self.ppu.read_reg(addr),
            0xFF46 => self.ppu.dma,
            0xFF51 => (self.hdma.src >> 8) as u8,
            0xFF52 => (self.hdma.src & 0x00F0) as u8,
            0xFF53 => ((self.hdma.dst & 0x1F00) >> 8) as u8,
            0xFF54 => (self.hdma.dst & 0x00F0) as u8,
            0xFF55 => {
                let remaining = self.hdma.blocks.saturating_sub(1) & 0x7F;
                if self.hdma.active {
                    remaining
                } else if self.hdma.mode == DmaMode::Hdma {
                    remaining | 0x80
                } else {
                    0xFF
                }
            }
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
            0xFF76 | 0xFF77 => {
                if self.cgb_mode {
                    self.apu.lock().unwrap().read_pcm(addr)
                } else {
                    0xFF
                }
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => 0xFF,
        }
    }

    pub fn read_byte(&mut self, addr: u16) -> u8 {
        self.read_byte_inner(addr, false)
    }

    fn dma_read_byte(&mut self, addr: u16) -> u8 {
        self.read_byte_inner(addr, true)
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        if self.dma_cycles > 0 {
            match addr {
                // allow writes to ROM, WRAM and all I/O/HRAM during DMA
                0x0000..=0x7FFF | 0xC000..=0xFDFF | 0xFF00..=0xFFFF => {}
                // block writes within OAM (and forbidden gap)
                0xFE00..=0xFEFF => return,
                // block VRAM and other regions
                _ => return,
            }
        }

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
                if !self.hdma.active {
                    self.hdma.src = (val as u16) << 8 | (self.hdma.src & 0x00FF);
                }
            }
            0xFF52 => {
                if !self.hdma.active {
                    self.hdma.src = (self.hdma.src & 0xFF00) | (val & 0xF0) as u16;
                }
            }
            0xFF53 => {
                if !self.hdma.active {
                    let vram_hi = (val & 0x1F) as u16;
                    self.hdma.dst = 0x8000 | (vram_hi << 8) | (self.hdma.dst & 0x00FF);
                }
            }
            0xFF54 => {
                if !self.hdma.active {
                    self.hdma.dst = (self.hdma.dst & 0xFF00) | (val & 0xF0) as u16;
                }
            }
            0xFF55 => {
                if !self.cgb_mode {
                    return;
                }
                let requested_blocks = (val & 0x7F) + 1;
                if self.hdma.active && (val & 0x80) == 0 {
                    // Abort ongoing HDMA
                    self.hdma.active = false;
                    self.hdma.blocks = 0;
                } else if val & 0x80 == 0 {
                    self.start_gdma(requested_blocks);
                } else {
                    self.hdma.mode = DmaMode::Hdma;
                    self.hdma.blocks = requested_blocks;
                    self.hdma.active = true;
                    if self.ppu.in_hblank() {
                        self.hdma_hblank_transfer();
                    }
                }
            }
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
                self.pending_dma = Some(src);
                // DMA starts after two M-cycles (8 cycles)
                self.pending_delay = 8;
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

    /// Write to VRAM bypassing mode checks (used by DMA transfers)
    fn vram_dma_write(&mut self, addr: u16, val: u8) {
        self.ppu.vram[self.ppu.vram_bank][(addr - 0x8000) as usize] = val;
    }

    pub fn take_serial(&mut self) -> Vec<u8> {
        self.serial.take_output()
    }

    /// Advance the ongoing OAM DMA transfer if active.
    pub fn dma_step(&mut self, cycles: u16) {
        for _ in 0..cycles {
            if self.pending_delay > 0 {
                self.pending_delay -= 1;
                if self.pending_delay == 0
                    && let Some(src) = self.pending_dma.take()
                {
                    self.dma_source = src;
                    // 160 bytes * 4 cycles each
                    self.dma_cycles = 640;
                }
            }

            if self.dma_cycles == 0 {
                continue;
            }

            let elapsed = 640 - self.dma_cycles;
            if elapsed.is_multiple_of(4) {
                let idx: u16 = elapsed / 4;
                if idx < 0xA0 {
                    let byte = self.dma_read_byte(self.dma_source.wrapping_add(idx));
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

    /// Return true if a General or HBlank DMA stall is in progress.
    pub fn gdma_active(&self) -> bool {
        self.gdma_cycles > 0
    }

    /// Decrement the GDMA stall counter by the given number of m-cycles.
    pub fn gdma_step(&mut self, cycles: u16) {
        if self.gdma_cycles > 0 {
            self.gdma_cycles = self.gdma_cycles.saturating_sub(cycles as u32);
        }
    }

    /// Perform a General DMA transfer immediately, consuming CPU cycles.
    fn start_gdma(&mut self, blocks: u8) {
        let total_bytes = blocks as usize * 0x10;
        let mut src = self.hdma.src;
        let mut dst = self.hdma.dst;

        for _ in 0..total_bytes {
            let byte = self.read_byte(src);
            self.vram_dma_write(dst, byte);
            src = src.wrapping_add(1);
            dst = 0x8000 | ((dst.wrapping_add(1)) & 0x1FFF);
        }

        self.hdma.src = src;
        self.hdma.dst = dst & 0xFFF0;
        self.hdma.active = false;
        self.hdma.blocks = 0;
        self.gdma_cycles = blocks as u32 * 8;
    }

    /// Execute a single 0x10-byte HDMA burst during H-Blank.
    pub fn hdma_hblank_transfer(&mut self) {
        if !(self.hdma.active && self.hdma.mode == DmaMode::Hdma) {
            return;
        }

        for _ in 0..0x10 {
            let byte = self.read_byte(self.hdma.src);
            self.vram_dma_write(self.hdma.dst, byte);
            self.hdma.src = self.hdma.src.wrapping_add(1);
            self.hdma.dst = 0x8000 | ((self.hdma.dst.wrapping_add(1)) & 0x1FFF);
        }

        self.hdma.blocks = self.hdma.blocks.saturating_sub(1);
        if self.hdma.blocks == 0 {
            self.hdma.active = false;
        }

        self.hdma.dst &= 0xFFF0;
        self.gdma_cycles += 8;
    }

    fn tick(&mut self, m_cycles: u32) {
        let hw_cycles = if self.key1 & 0x80 != 0 {
            2 * m_cycles as u16
        } else {
            4 * m_cycles as u16
        };
        let prev_div = self.timer.div;
        self.timer.step(hw_cycles, &mut self.if_reg);
        let curr_div = self.timer.div;
        {
            let mut apu = self.apu.lock().unwrap();
            // Advance 2 MHz domain before 1 MHz staging to match APU internal ordering
            apu.step(hw_cycles);
            apu.tick(prev_div, curr_div, self.key1 & 0x80 != 0);
        }
        let _ = self.ppu.step(hw_cycles, &mut self.if_reg);
    }
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}
