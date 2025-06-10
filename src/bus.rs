// src/bus.rs

use crate::models; // Use models from crate root

use crate::apu::Apu;
use crate::interrupts::InterruptType;
use crate::joypad::Joypad; // Added Joypad
use crate::mbc::{
    CartridgeType, MemoryBankController, NoMBC, MBC1, MBC2, MBC3, MBC30, MBC5, MBC6, MBC7,
}; // Added MBC30 import
use crate::memory::Memory;
use crate::ppu::Ppu; // Added new PPU
use crate::timer::Timer;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

// Helper function to determine RAM size from cartridge header
fn get_ram_size_from_header(ram_header_byte: u8) -> usize {
    match ram_header_byte {
        0x00 => 0,          // No RAM
        0x01 => 2 * 1024,   // 2KB
        0x02 => 8 * 1024,   // 8KB
        0x03 => 32 * 1024,  // 32KB (4 banks of 8KB)
        0x04 => 128 * 1024, // 128KB (16 banks of 8KB)
        0x05 => 64 * 1024,  // 64KB (8 banks of 8KB)
        _ => {
            println!(
                "Warning: Unknown RAM size code 0x{:02X}. Defaulting to 0 RAM.",
                ram_header_byte
            );
            0 // Default to no RAM for unknown codes
        }
    }
}

pub struct Bus {
    pub mbc: Box<dyn MemoryBankController>, // MBC trait object
    pub memory: Memory,
    pub ppu: Ppu, // Added new PPU field
    pub apu: Apu,
    pub joypad: Joypad, // Added joypad field
    pub timer: Timer,
    // pub system_mode: SystemMode, // Replaced by model
    pub model: models::GameBoyModel, // Added model field
    pub is_double_speed: bool,
    pub key1_prepare_speed_switch: bool,
    // rom_data is now primarily owned by the MBC. Bus might not need its own copy
    // if all ROM access goes via MBC. For now, Bus::new still receives it for header parsing.
    // Let's remove rom_data from Bus struct if MBC handles it.
    // pub rom_data: Vec<u8>,
    pub serial_output: Vec<u8>,        // Added for serial output capture
    pub interrupt_enable_register: u8, // IE Register (0xFFFF)
    pub if_register: u8,               // Interrupt Flag Register (0xFF0F)
    pub oam_dma_active: bool,
    pub oam_dma_cycles_remaining: u32,
    pub oam_dma_source_address_upper: u8,
    // HDMA/GDMA Registers
    pub hdma1_src_high: u8,
    pub hdma2_src_low: u8,
    pub hdma3_dest_high: u8,
    pub hdma4_dest_low: u8,
    pub hdma5: u8,
    // HDMA/GDMA Internal State
    pub hdma_active: bool,
    pub gdma_active: bool, // May not be strictly needed if GDMA is instant, but good for state tracking
    hdma_current_src: u16,
    hdma_current_dest: u16,
    pub hdma_blocks_remaining: u8,
    pub hblank_hdma_pending: bool, // Flag to perform one HDMA block transfer
}

impl Bus {
    // Placeholder for GDMA transfer logic
    fn perform_gdma_transfer(&mut self) {
        // Actual transfer logic will be implemented in the next step.
        // This function will be called when a GDMA is initiated via HDMA5.
        // For now, it will just consume the blocks conceptually.

        // The PPU's VRAM, VBK register, etc. will be accessed via the new PPU module.
        let num_blocks_to_transfer = self.hdma_blocks_remaining;
        if !self.gdma_active || num_blocks_to_transfer == 0 {
            return;
        }

        // println!(
        //     "GDMA: Transferring {} blocks from 0x{:04X} to 0x{:04X} (VRAM Bank {})",
        //     num_blocks_to_transfer, self.hdma_current_src, self.hdma_current_dest, self.ppu.vbk
        // );

        for _block in 0..num_blocks_to_transfer {
            for i in 0..16 {
                let byte_to_transfer =
                    self.read_byte_internal(self.hdma_current_src.wrapping_add(i));
                // Destination is VRAM (0x8000-0x9FFF)
                // PPU will handle VRAM banking internally if needed.
                self.ppu
                    .write_vram(self.hdma_current_dest.wrapping_add(i), byte_to_transfer);
            }
            self.hdma_current_src = self.hdma_current_src.wrapping_add(16);
            self.hdma_current_dest = self.hdma_current_dest.wrapping_add(16);
            // Destination address should wrap around in the 0x8000-0x9FFF range (0x1FF0 effectively for 16-byte alignment)
            self.hdma_current_dest = 0x8000 | (self.hdma_current_dest & 0x1FF0);
        }

        // GDMA finishes, update state
        self.hdma_blocks_remaining = 0; // All blocks transferred
        self.hdma5 = 0xFF; // HDMA5 reflects completion
        self.gdma_active = false;
        self.hdma_active = false; // GDMA also stops any pending HDMA
    }

    pub fn new(rom_data: Vec<u8>) -> Self {
        // Determine GameBoyModel based on ROM header (0x0143)
        let cgb_flag = if rom_data.len() > 0x0143 {
            rom_data[0x0143]
        } else {
            0x00
        };
        let determined_model = if cgb_flag == 0x80 || cgb_flag == 0xC0 {
            // TODO: Further differentiate CGB revisions based on more info if available/needed
            // For now, CGB0-CGBE are distinct values, but detection might be complex.
            // Defaulting to a generic CGB or a specific common one like CGB-D/E.
            models::GameBoyModel::GenericCGB
        } else {
            // Could check for SGB indicator (0x0146 == 0x03 and 0x014B == 0x33) here too.
            // For now, default non-CGB to DMG.
            models::GameBoyModel::DMG
        };

        let cartridge_type_byte = if rom_data.len() > 0x0147 {
            // Corrected index check
            rom_data[0x0147]
        } else {
            0x00 // Default to ROM ONLY if header is too short
        };

        let mbc_type = CartridgeType::from_byte(cartridge_type_byte);

        let ram_header_byte = if rom_data.len() > 0x0149 {
            rom_data[0x0149]
        } else {
            0x00 // Default to no RAM if header is too short
        };
        let ram_size = get_ram_size_from_header(ram_header_byte);

        let mbc: Box<dyn MemoryBankController> = match mbc_type {
            CartridgeType::NoMBC => Box::new(NoMBC::new(rom_data.clone(), ram_size)),
            CartridgeType::MBC1 => Box::new(MBC1::new(rom_data.clone(), ram_size)),
            CartridgeType::MBC2 => Box::new(MBC2::new(rom_data.clone())),
            CartridgeType::MBC3 => Box::new(MBC3::new(rom_data.clone(), ram_size)),
            CartridgeType::MBC5 => {
                Box::new(MBC5::new(rom_data.clone(), ram_size, cartridge_type_byte))
            }
            CartridgeType::MBC6 => {
                Box::new(MBC6::new(rom_data.clone(), ram_size, cartridge_type_byte))
            }
            CartridgeType::MBC7 => {
                Box::new(MBC7::new(rom_data.clone(), ram_size, cartridge_type_byte))
            }
            CartridgeType::MBC30 => {
                Box::new(MBC30::new(rom_data.clone(), ram_size, cartridge_type_byte))
            }
            CartridgeType::Unknown(byte) => {
                println!(
                    "Warning: Unknown cartridge type 0x{:02X}. Defaulting to NoMBC.",
                    byte
                );
                Box::new(NoMBC::new(rom_data.clone(), ram_size))
            }
        };

        Self {
            mbc,
            memory: Memory::new(),
            ppu: Ppu::new(determined_model), // Initialize new PPU
            apu: Apu::new(determined_model), // Pass GameBoyModel to Apu
            joypad: Joypad::new(),
            timer: Timer::new(),
            model: determined_model,
            is_double_speed: false,
            key1_prepare_speed_switch: false,
            // rom_data field removed from Bus struct, MBC is the owner now
            // cartridge_type_byte, // Field removed
            serial_output: Vec::new(),    // Initialize serial_output
            interrupt_enable_register: 0, // Default value for IE
            if_register: 0x00,            // Default value for IF
            oam_dma_active: false,
            oam_dma_cycles_remaining: 0,
            oam_dma_source_address_upper: 0,
            // HDMA/GDMA Registers Init
            hdma1_src_high: 0xFF,
            hdma2_src_low: 0xFF,
            hdma3_dest_high: 0xFF,
            hdma4_dest_low: 0xFF,
            hdma5: 0xFF,
            // HDMA/GDMA Internal State Init
            hdma_active: false,
            gdma_active: false,
            hdma_current_src: 0,
            hdma_current_dest: 0,
            hdma_blocks_remaining: 0,
            hblank_hdma_pending: false,
        }
    }

    pub fn tick_components(&mut self, m_cycles: u32) {
        // Note: Cycle accounting for DMA (GDMA/HDMA) halting the CPU is not yet implemented here.
        // GDMA effectively happens "instantly" from the CPU's perspective after the write to FF55.
        // HDMA blocks also happen "instantly" during an HBlank from CPU perspective.
        // Correct cycle modeling would involve the Bus consuming cycles for DMA here.

        let t_cycles = m_cycles * 4;

        // Tick PPU
        self.ppu.tick(t_cycles);

        // Handle PPU Interrupt Requests
        if self.ppu.vblank_interrupt_requested {
            self.request_interrupt(InterruptType::VBlank);
            self.ppu.clear_vblank_interrupt_request();
        }
        if self.ppu.stat_interrupt_requested {
            self.request_interrupt(InterruptType::LcdStat);
            self.ppu.clear_stat_interrupt_request();
        }

        // Check for HDMA trigger based on PPU state (just_entered_hblank is managed by PPU)
        if self.model.is_cgb_family() && self.hdma_active && self.ppu.just_entered_hblank {
            self.hblank_hdma_pending = true;
        }

        // Handle HDMA transfer if pending (one block per HBlank)
        if self.hblank_hdma_pending {
            if self.hdma_blocks_remaining > 0 {
                // Perform one 16-byte block transfer for HDMA
                // println!(
                //     "HDMA: Transferring 1 block from 0x{:04X} to 0x{:04X} (VRAM Bank {}), {} blocks left",
                //     self.hdma_current_src, self.hdma_current_dest, self.ppu.vbk, self.hdma_blocks_remaining -1
                // );
                for i in 0..16 {
                    let byte_to_transfer =
                        self.read_byte_internal(self.hdma_current_src.wrapping_add(i));
                    self.ppu
                        .write_vram(self.hdma_current_dest.wrapping_add(i), byte_to_transfer);
                }
                self.hdma_current_src = self.hdma_current_src.wrapping_add(16);
                self.hdma_current_dest =
                    0x8000 | (self.hdma_current_dest.wrapping_add(16) & 0x1FF0);

                self.hdma_blocks_remaining -= 1;

                if self.hdma_blocks_remaining == 0 {
                    self.hdma_active = false;
                    self.hdma5 = 0xFF; // HDMA finished
                }
            }
            self.hblank_hdma_pending = false;
        }

        // Tick Timer
        self.timer.tick(t_cycles, &mut self.if_register);

        // OAM DMA Transfer Logic (this is separate from HDMA/GDMA)
        if self.oam_dma_active {
            // TODO: New PPU interaction needed here - OAM DMA writes to PPU's OAM
            if self.oam_dma_cycles_remaining == 160 * 4 {
                // Check if just initiated
                let source_base_address = (self.oam_dma_source_address_upper as u16) << 8;
                for i in 0..160 {
                    // 0xA0 bytes
                    let byte_to_copy = self.read_byte_internal(source_base_address + i as u16);
                    self.ppu.write_oam_dma(0xFE00 + i as u16, byte_to_copy); // Write to OAM via PPU DMA method
                }
            }

            if self.oam_dma_cycles_remaining <= t_cycles {
                self.oam_dma_cycles_remaining = 0;
                self.oam_dma_active = false;
            } else {
                self.oam_dma_cycles_remaining -= t_cycles;
            }
        }

        // TODO: Tick APU
        // self.apu.tick(t_cycles);
    }

    // Internal read method that bypasses DMA locks, for use by DMA itself.
    fn read_byte_internal(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.mbc.read_rom(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr), // VRAM (DMA can read from VRAM)
            0xA000..=0xBFFF => self.mbc.read_ram(addr - 0xA000), // Cartridge RAM
            0xC000..=0xDFFF => self.memory.read_byte(addr), // WRAM
            0xE000..=0xFDFF => {
                // Echo RAM
                let mirrored_addr = addr - 0x2000;
                self.memory.read_byte(mirrored_addr)
            }
            // OAM (0xFE00-0xFE9F) and I/O registers (0xFF00-0xFF7F) are generally not DMA sources
            // or require special handling. HRAM (0xFF80-0xFFFE) can be a source.
            // For simplicity, this internal read covers common source areas.
            // If DMA could source from OAM or I/O, those cases would need to be added.
            // Reading from 0xFE00..=0xFEFF (OAM and unusable)
            0xFE00..=0xFEFF => {
                // This range includes OAM (FE00-FE9F) and Unusable (FEA0-FEFF)
                if addr <= 0xFE9F {
                    self.ppu.read_oam(addr) // OAM
                } else {
                    0xFF // Unusable memory
                }
            }
            // I/O Registers. Some games might try to DMA from weird sources.
            // Generally, DMA sources are ROM, WRAM, HRAM.
            // Let's assume for now DMA from I/O regs returns 0xFF or specific values if ever needed.
            // This part matches the main read_byte for I/O for consistency if any are readable.
            0xFF00..=0xFF7F => {
                match addr {
                    0xFF00 => self.joypad.read_p1(),
                    0xFF01..=0xFF02 => 0xFF, // Serial placeholder
                    0xFF04..=0xFF07 => self.timer.read_byte(addr),
                    0xFF0F => self.if_register | 0xE0,
                    0xFF10..=0xFF3F => self.apu.read_byte(addr),
                    0xFF40 => self.ppu.lcdc,
                    0xFF41 => self.ppu.stat, // PPU ensures bit 7 is set.
                    0xFF42 => self.ppu.scy,
                    0xFF43 => self.ppu.scx,
                    0xFF44 => self.ppu.ly,
                    0xFF45 => self.ppu.lyc,
                    // 0xFF46 is DMA register, Bus handles its write side. Read is often just last written value.
                    0xFF46 => self.oam_dma_source_address_upper, // Or a specific DMA read behavior if necessary
                    0xFF47 => self.ppu.bgp,
                    0xFF48 => self.ppu.obp0,
                    0xFF49 => self.ppu.obp1,
                    0xFF4A => self.ppu.wy,
                    0xFF4B => self.ppu.wx,
                    0xFF4D => {
                        // KEY1
                        let speed_bit = if self.is_double_speed { 0x80 } else { 0x00 };
                        let prepare_bit = if self.key1_prepare_speed_switch {
                            0x01
                        } else {
                            0x00
                        };
                        speed_bit | prepare_bit | 0x7E
                    }
                    0xFF4C | 0xFF4E..=0xFF4F => 0xFF,
                    _ => 0xFF, // Default for other I/O
                }
            }
            0xFF80..=0xFFFE => self.memory.read_byte(addr), // HRAM
            0xFFFF => self.interrupt_enable_register,       // IE Register
                                                             // _ => 0xFF, // Default for any unmapped reads
        }
    }

    // pub fn get_system_mode(&self) -> SystemMode { // Replaced by get_model
    //     self.system_mode
    // }
    pub fn get_model(&self) -> models::GameBoyModel {
        self.model
    }

    #[allow(dead_code)] // Added to address unused method warning
    pub fn get_is_double_speed(&self) -> bool {
        self.is_double_speed
    }

    pub fn toggle_speed_mode(&mut self) {
        self.is_double_speed = !self.is_double_speed;
    }

    pub fn get_key1_prepare_speed_switch(&self) -> bool {
        self.key1_prepare_speed_switch
    }

    pub fn set_key1_prepare_speed_switch(&mut self, prepared: bool) {
        self.key1_prepare_speed_switch = prepared;
    }

    pub fn load_save_files(&mut self, rom_path: &str) {
        let save_path = Path::new(rom_path).with_extension("sav");
        if let Ok(mut file) = File::open(&save_path) {
            let mut data = Vec::new();
            if file.read_to_end(&mut data).is_ok() {
                if let Some(ram) = self.mbc.get_ram_mut() {
                    let len = ram.len().min(data.len());
                    ram[..len].copy_from_slice(&data[..len]);
                }
            }
        }

        let rtc_path = Path::new(rom_path).with_extension("rtc");
        if let Ok(mut file) = File::open(&rtc_path) {
            let mut buf = [0u8; 5];
            if file.read_exact(&mut buf).is_ok() {
                if let Some(rtc) = self.mbc.get_rtc_mut() {
                    rtc.seconds = buf[0];
                    rtc.minutes = buf[1];
                    rtc.hours = buf[2];
                    rtc.day_counter_low = buf[3];
                    rtc.day_counter_high = buf[4];
                }
            }
        }
    }

    pub fn save_save_files(&self, rom_path: &str) {
        if let Some(ram) = self.mbc.get_ram() {
            let save_path = Path::new(rom_path).with_extension("sav");
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(save_path)
            {
                let _ = file.write_all(ram);
            }
        }

        if let Some(rtc) = self.mbc.get_rtc() {
            let rtc_path = Path::new(rom_path).with_extension("rtc");
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(rtc_path)
            {
                let buf = [
                    rtc.seconds,
                    rtc.minutes,
                    rtc.hours,
                    rtc.day_counter_low,
                    rtc.day_counter_high,
                ];
                let _ = file.write_all(&buf);
            }
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        // DMA Access Restrictions:
        // If OAM DMA is active, most of the bus is inaccessible, except for HRAM.
        // Reads from non-HRAM addresses return 0xFF during OAM DMA.
        if self.oam_dma_active {
            if !(addr >= 0xFF80 && addr <= 0xFFFE) {
                // Allow reads from HRAM (0xFF80 - 0xFFFE)
                // For addresses outside HRAM, reads return 0xFF during OAM DMA
                return 0xFF;
            }
        }

        // println!("Bus read at 0x{:04X}", addr);
        match addr {
            0x0000..=0x7FFF => self.mbc.read_rom(addr), // ROM area, handled by MBC
            0x8000..=0x9FFF => self.ppu.read_vram(addr), // VRAM
            0xA000..=0xBFFF => self.mbc.read_ram(addr - 0xA000), // Cartridge RAM (External RAM)
            0xC000..=0xDFFF => self.memory.read_byte(addr), // WRAM
            0xE000..=0xFDFF => {
                // Echo RAM (mirror of 0xC000 - 0xDDFF)
                let mirrored_addr = addr - 0x2000;
                self.memory.read_byte(mirrored_addr)
            }
            0xFE00..=0xFE9F => self.ppu.read_oam(addr), // OAM
            0xFEA0..=0xFEFF => {
                // Unusable memory
                0xFF
            }
            0xFF00..=0xFF7F => {
                // I/O Registers
                match addr {
                    0xFF00 => self.joypad.read_p1(), // Joypad read
                    0xFF01..=0xFF02 => {
                        // Serial - Placeholder
                        0xFF
                    }
                    0xFF04..=0xFF07 => self.timer.read_byte(addr), // Route to Timer
                    0xFF0F => self.if_register | 0xE0,             // IF - Interrupt Flag Register
                    0xFF10..=0xFF3F => self.apu.read_byte(addr),   // APU registers
                    0xFF40 => self.ppu.lcdc,
                    0xFF41 => self.ppu.stat, // PPU ensures bit 7 is set.
                    0xFF42 => self.ppu.scy,
                    0xFF43 => self.ppu.scx,
                    0xFF44 => self.ppu.ly,
                    0xFF45 => self.ppu.lyc,
                    0xFF46 => self.oam_dma_source_address_upper, // Read DMA register
                    0xFF47 => self.ppu.bgp,
                    0xFF48 => self.ppu.obp0,
                    0xFF49 => self.ppu.obp1,
                    0xFF4A => self.ppu.wy,
                    0xFF4B => self.ppu.wx,
                    0xFF4F => self.ppu.vbk | 0xFE, // VBK - bit 0 is bank, others read as 1
                    0xFF68 => self.ppu.bcps_bcpi,  // BCPS/BCPI
                    0xFF69 => self.ppu.bcpd_bgpd, // BCPD/BGPD - TODO: PPU should handle read from current index in bcps_bcpi
                    0xFF6A => self.ppu.ocps_ocpi, // OCPS/OCPI
                    0xFF6B => self.ppu.ocpd_obpd, // OCPD/OBPD - TODO: PPU should handle read from current index in ocps_ocpi
                    0xFF4D => {
                        // KEY1
                        // KEY1 - CGB Speed Switch
                        let speed_bit = if self.is_double_speed { 0x80 } else { 0x00 };
                        let prepare_bit = if self.key1_prepare_speed_switch {
                            0x01
                        } else {
                            0x00
                        };
                        speed_bit | prepare_bit | 0x7E // Other bits are 1
                    }
                    // 0xFF4C is defined as "Unused" by Pandocs for DMG, CGB.
                    // 0xFF4E is defined as "Unused" by Pandocs for DMG, (KEY0 for CGB BIOS).
                    // For now, treating them as unmapped is fine.
                    0xFF4C | 0xFF4E => 0xFF,
                    // HDMA Registers
                    0xFF51 => self.hdma1_src_high,
                    0xFF52 => self.hdma2_src_low,
                    0xFF53 => self.hdma3_dest_high,
                    0xFF54 => self.hdma4_dest_low,
                    0xFF55 => {
                        // HDMA5 - DMA Status
                        if self.hdma_active {
                            // Active HDMA
                            // Bit 7 is 0, lower 7 bits are (blocks_remaining - 1)
                            (self.hdma_blocks_remaining.saturating_sub(1)) & 0x7F
                        } else {
                            // Inactive HDMA (or after GDMA)
                            0xFF
                        }
                    }
                    // Other I/O registers (0xFF56-0xFF67, 0xFF6C-0xFF7F)
                    0xFF56..=0xFF67 | 0xFF6C..=0xFF7F => {
                        // SVBK (FF70) etc. would be here too if not part of a larger unmapped range
                        0xFF // Placeholder for other CGB I/O regs
                    }
                    _ => 0xFF, // Default for unmapped I/O in 0xFFxx range
                }
            }
            0xFF80..=0xFFFE => self.memory.read_byte(addr), // HRAM
            0xFFFF => self.interrupt_enable_register,       // IE Register
                                                             // _ => {
                                                             //     // This should ideally not be reached if all ranges are covered
                                                             //     // panic!("Read from unhandled Bus address: {:#04X}", addr);
                                                             //     0xFF // Default for any unmapped reads not explicitly handled
                                                             // }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        // DMA Access Restrictions:
        // If OAM DMA is active, most of the bus is inaccessible, except for HRAM.
        if self.oam_dma_active {
            if !(addr >= 0xFF80 && addr <= 0xFFFE) {
                // Allow writes to HRAM (0xFF80 - 0xFFFE)
                // For addresses outside HRAM, writes are ignored during OAM DMA
                return;
            }
        }

        // println!("Bus write at 0x{:04X} with value 0x{:02X}", addr, value);
        match addr {
            0x0000..=0x7FFF => self.mbc.write_rom(addr, value), // ROM area, handled by MBC
            0x8000..=0x9FFF => self.ppu.write_vram(addr, value), // VRAM
            0xA000..=0xBFFF => self.mbc.write_ram(addr - 0xA000, value), // Cartridge RAM
            0xC000..=0xDFFF => self.memory.write_byte(addr, value), // WRAM
            0xE000..=0xFDFF => {
                // Echo RAM (mirror of 0xC000 - 0xDDFF)
                let mirrored_addr = addr - 0x2000;
                self.memory.write_byte(mirrored_addr, value)
            }
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, value), // OAM
            0xFEA0..=0xFEFF => {
                // Unusable memory - Do nothing
            }
            0xFF00..=0xFF7F => {
                // I/O Registers
                match addr {
                    0xFF00 => self.joypad.write_p1(value), // Joypad write
                    0xFF01..=0xFF02 => {
                        // Serial Data Transfer
                        if addr == 0xFF01 {
                            // SB: Serial Transfer Data
                            // For now, we just append the byte to our serial_output vector
                            // This allows us to capture and inspect what the game is trying to "print"
                            // A full implementation would involve timing, control bits from 0xFF02 (SC), etc.
                            println!(
                                "Serial port (0xFF01) received byte: 0x{:02X} ('{}')",
                                value, value as char
                            );
                            self.serial_output.push(value);
                        }
                        // 0xFF02 (SC - Serial Transfer Control) is not fully handled here yet.
                        // Writes to SC might clear serial_output or trigger other behavior in a full system.
                        // For now, we only capture data written to SB (0xFF01).
                    }
                    0xFF04..=0xFF07 => self.timer.write_byte(addr, value), // Route to Timer
                    0xFF0F => {
                        self.if_register = value & 0x1F;
                    } // IF - Interrupt Flag Register
                    0xFF10..=0xFF3F => self.apu.write_byte(addr, value),   // APU registers
                    0xFF40 => self.ppu.lcdc = value, // TODO: Consider LCDC write side effects (e.g., turning LCD off/on)
                    0xFF41 => {
                        // STAT
                        self.ppu.write_stat(value);
                    }
                    0xFF42 => self.ppu.scy = value,
                    0xFF43 => self.ppu.scx = value,
                    // 0xFF44 (LY) is read-only for CPU
                    0xFF45 => self.ppu.lyc = value,
                    0xFF46 => {
                        // DMA - OAM DMA Start Register
                        self.oam_dma_source_address_upper = value;
                        self.oam_dma_active = true;
                        self.oam_dma_cycles_remaining = 160 * 4;
                    }
                    0xFF47 => self.ppu.bgp = value,
                    0xFF48 => self.ppu.obp0 = value,
                    0xFF49 => self.ppu.obp1 = value,
                    0xFF4A => self.ppu.wy = value,
                    0xFF4B => self.ppu.wx = value,
                    0xFF4F => self.ppu.vbk = value & 0x01, // VBK - CGB VRAM Bank Select (only bit 0 is writable)
                    0xFF68 => self.ppu.bcps_bcpi = value,  // BCPS/BCPI
                    0xFF69 => {
                        // BCPD/BGPD
                        self.ppu.bcpd_bgpd = value;
                        // TODO: Write to cgb_background_palette_ram at index from bcps_bcpi
                        // if (self.ppu.bcps_bcpi & 0x80) { self.ppu.bcps_bcpi = (self.ppu.bcps_bcpi + 1) & 0xBF; } // Auto-increment
                    }
                    0xFF6A => self.ppu.ocps_ocpi = value, // OCPS/OCPI
                    0xFF6B => {
                        // OCPD/OBPD
                        self.ppu.ocpd_obpd = value;
                        // TODO: Write to cgb_sprite_palette_ram at index from ocps_ocpi
                        // if (self.ppu.ocps_ocpi & 0x80) { self.ppu.ocps_ocpi = (self.ppu.ocps_ocpi + 1) & 0xBF; } // Auto-increment
                    }
                    0xFF4D => {
                        // KEY1
                        // KEY1 - CGB Speed Switch
                        self.key1_prepare_speed_switch = (value & 0x01) != 0;
                    }
                    0xFF4C | 0xFF4E => { /* Unmapped or Read-only */ }
                    // HDMA Registers
                    0xFF51 => self.hdma1_src_high = value,
                    0xFF52 => self.hdma2_src_low = value & 0xF0, // Lower 4 bits ignored
                    0xFF53 => self.hdma3_dest_high = value & 0x1F, // Upper 3 bits ignored (dest in 0x8000-0x9FFF)
                    0xFF54 => self.hdma4_dest_low = value & 0xF0,  // Lower 4 bits ignored
                    0xFF55 => {
                        // HDMA5 - DMA Control/Start
                        if !self.model.is_cgb_family() {
                            return;
                        } // CGB Only feature

                        // Source address
                        self.hdma_current_src =
                            ((self.hdma1_src_high as u16) << 8) | (self.hdma2_src_low as u16);
                        self.hdma_current_src &= 0xFFF0; // Align to 16 bytes (lower 4 bits are zero)
                                                         // Source must be in ROM or RAM (0x0000-0x7FFF or 0xA000-0xDFFF)

                        // Destination address in VRAM
                        // HDMA3 (FF53) provides bits 12-8 of VRAM address (0x1F range).
                        // HDMA4 (FF54) provides bits 7-4 of VRAM address (0xF0 range).
                        // Lowest 4 bits are implicitly zero. Resulting offset is 0x0000-0x1FF0.
                        let dest_offset = (((self.hdma3_dest_high & 0x1F) as u16) << 8)
                            | ((self.hdma4_dest_low & 0xF0) as u16);
                        self.hdma_current_dest = 0x8000 | dest_offset;

                        self.hdma_blocks_remaining = (value & 0x7F) + 1; // Number of 16-byte blocks

                        if (value & 0x80) == 0 {
                            // GDMA (General Purpose DMA)
                            if self.hdma_active {
                                // Writing 0 to bit 7 of HDMA5 when HDMA is active should have no effect (HDMA continues)
                                // This interpretation might vary. Some sources say it might stop HDMA.
                                // Pandocs: "writing to FF55 can start a new transfer, or terminate an active HDMA transfer."
                                // "If HDMA is active, writing to FF55 with bit 7 cleared will end the HDMA transfer."
                                // This means if HDMA is active, and we write a new value with bit 7 = 0 for GDMA, HDMA stops.
                                self.hdma_active = false;
                            }
                            self.gdma_active = true;
                            self.perform_gdma_transfer(); // Execute GDMA immediately
                                                          // perform_gdma_transfer will set hdma5 to 0xFF and gdma_active to false.
                        } else {
                            // HDMA (H-Blank DMA)
                            if self.hdma_active {
                                // Request to stop current HDMA
                                self.hdma_active = false;
                                // HDMA5 read will now show remaining length with bit 7 as 1.
                                // The value written to HDMA5 (value & 0x7F) is the new "length" for HDMA5 reads.
                                // However, hdma_blocks_remaining still holds the actual blocks for a potential restart.
                                // For readback, we need to store the value written if we want HDMA5 to reflect (value & 0x7F) | 0x80.
                                // Pandocs: "Reading $FF55 returns ... $FF if the HDMA is inactive. Bit 7 is ... 1 otherwise."
                                // So if we stop it, HDMA5 should read as 0xFF.
                                // Let's ensure self.hdma5 reflects this for reads.
                                self.hdma5 = 0xFF; // When HDMA is stopped, it reads as inactive.
                            } else {
                                // Start new HDMA
                                self.hdma_active = true;
                                self.hdma5 = value; // Store for readback (active flag will be based on hdma_active)
                                                    // Transfer will occur in HBlank periods.
                            }
                        }
                    }
                    // Other I/O (0xFF56-0xFF67, 0xFF6C-0xFF7F)
                    _ => { /* Writes to other unhandled I/O regs are ignored */ }
                }
            }
            0xFF80..=0xFFFE => self.memory.write_byte(addr, value), // HRAM
            0xFFFF => self.interrupt_enable_register = value,       // IE Register
                                                                     // _ => {
                                                                     //     // This should ideally not be reached if all ranges are covered
                                                                     //     // panic!("Write to unhandled Bus address: {:#04X}", addr);
                                                                     // }
        }
    }

    // Method to get the captured serial output as a String
    pub fn get_serial_output_string(&self) -> String {
        String::from_utf8_lossy(&self.serial_output).into_owned()
    }

    pub fn request_interrupt(&mut self, interrupt: InterruptType) {
        self.if_register |= 1 << interrupt.bit();
    }

    // This might be called by the CPU when an interrupt is serviced
    pub fn clear_interrupt_flag(&mut self, interrupt_bit: u8) {
        self.if_register &= !(1 << interrupt_bit);
    }
}

// This closes the `impl Bus` block. The test module should be outside.

#[cfg(test)]
mod tests {
    use super::*;
    // Make sure Bus is in scope, usually true with `super::*` if Bus is at the crate/module root.
    // If Bus is not found, it might be due to module structure.
    // For this specific project structure, Bus is defined in src/bus.rs,
    // and this test module is also in src/bus.rs. So `super::Bus` or `Bus` (via `super::*`) should work.
    use crate::cpu::Cpu; // Assuming cpu.rs is in crate root
    use std::cell::RefCell;
    use std::env;
    use std::rc::Rc;

    fn setup_test_env() -> (Cpu, Rc<RefCell<Bus>>) {
        // Provide dummy ROM data for Bus creation
        let rom_data = vec![0; 0x100]; // Example: 256 bytes of ROM
        let bus = Rc::new(RefCell::new(Bus::new(rom_data)));
        let cpu = Cpu::new(bus.clone());
        (cpu, bus)
    }

    #[test]
    fn test_cpu_write_to_wram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        // Test LD A, n
        // cpu.ld_a_n(0xAB) is not a method in the current cpu.rs, it takes parameters from opcode
        // Direct CPU method call for LD A, 0xAB (0x3E, 0xAB)
        // For this test, we'll just set cpu.a directly.
        cpu.a = 0xAB;
        assert_eq!(cpu.a, 0xAB);

        // Test LD (nn), A where nn is a WRAM address
        // Let nn = 0xC100
        let wram_addr = 0xC100;
        cpu.pc = 0x0100; // Dummy PC for the instruction itself for PC increment logic

        // Simulate LD (0xC100), A
        // The method ld_nn_mem_a takes addr_lo, addr_hi
        cpu.ld_nn_mem_a((wram_addr & 0xFF) as u8, (wram_addr >> 8) as u8);

        assert_eq!(
            bus.borrow().read_byte(wram_addr),
            0xAB,
            "Value in WRAM via bus is incorrect"
        );
        assert_eq!(cpu.pc, 0x0100 + 3, "PC increment for LD (nn),A is wrong");
    }

    #[test]
    fn test_cpu_read_from_wram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        let wram_addr = 0xC200;
        let expected_val = 0xCD;
        bus.borrow_mut().write_byte(wram_addr, expected_val);

        // Test LD A, (nn) where nn is a WRAM address
        cpu.pc = 0x0150; // Dummy PC
        cpu.ld_a_nn_mem((wram_addr & 0xFF) as u8, (wram_addr >> 8) as u8);

        assert_eq!(
            cpu.a, expected_val,
            "Value read into A from WRAM via bus is incorrect"
        );
        assert_eq!(cpu.pc, 0x0150 + 3, "PC increment for LD A,(nn) is wrong");
    }

    #[test]
    fn test_cpu_write_to_hram_via_bus() {
        let (mut cpu, bus) = setup_test_env();
        cpu.a = 0xBE;

        // Test LD (HL), A where HL points to HRAM
        // Let HL = 0xFF80 (start of HRAM)
        cpu.h = 0xFF;
        cpu.l = 0x80;
        let hram_addr = 0xFF80;
        cpu.pc = 0x0200;

        cpu.ld_hl_mem_a(); // This uses the write_hl_mem helper

        assert_eq!(
            bus.borrow().read_byte(hram_addr),
            0xBE,
            "Value in HRAM via bus is incorrect"
        );
        assert_eq!(cpu.pc, 0x0200 + 1, "PC increment for LD (HL),A is wrong");
    }

    #[test]
    fn test_cpu_read_from_hram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        let hram_addr = 0xFF8A;
        let expected_val = 0xEF;
        bus.borrow_mut().write_byte(hram_addr, expected_val);

        // Test LD A, (HL) where HL points to HRAM
        cpu.h = (hram_addr >> 8) as u8;
        cpu.l = (hram_addr & 0xFF) as u8;
        cpu.pc = 0x0250;

        cpu.ld_a_hl_mem(); // This uses the read_hl_mem helper

        assert_eq!(
            cpu.a, expected_val,
            "Value read into A from HRAM via bus is incorrect"
        );
        assert_eq!(cpu.pc, 0x0250 + 1, "PC increment for LD A,(HL) is wrong");
    }

    #[test]
    fn test_cpu_stack_operations_on_wram_via_bus() {
        let (mut cpu, bus) = setup_test_env();

        cpu.sp = 0xDFFF; // Top of WRAM
        cpu.b = 0x12;
        cpu.c = 0x34;
        cpu.pc = 0x0300;

        cpu.push_bc(); // Pushes B then C. SP becomes 0xDFFD.
                       // Memory at 0xDFFE should be B (0x12)
                       // Memory at 0xDFFD should be C (0x34)

        assert_eq!(cpu.sp, 0xDFFD, "SP after PUSH BC is wrong");
        assert_eq!(
            bus.borrow().read_byte(0xDFFE),
            0x12,
            "Value for B on stack (WRAM) is incorrect"
        );
        assert_eq!(
            bus.borrow().read_byte(0xDFFD),
            0x34,
            "Value for C on stack (WRAM) is incorrect"
        );
        assert_eq!(cpu.pc, 0x0300 + 1);

        // Now POP DE (values should be what was pushed for BC)
        cpu.pc = 0x0301;
        cpu.pop_de(); // D should get value from stack (0x12), E from (0x34)

        assert_eq!(cpu.d, 0x12, "D after POP DE is incorrect");
        assert_eq!(cpu.e, 0x34, "E after POP DE is incorrect");
        assert_eq!(cpu.sp, 0xDFFF, "SP after POP DE is wrong");
        assert_eq!(cpu.pc, 0x0301 + 1);
    }

    #[test]
    fn test_ppu_io_read_placeholder() {
        // This test just checks if the bus routes to the PPU placeholder
        // It doesn't check for correct PPU behavior, only that the PPU's read_byte is called.
        let (mut cpu, _bus) = setup_test_env(); // bus is not directly used for assert here

        let ppu_lcdc_addr = 0xFF40; // LCDC register

        // LD A, (HL) where HL = 0xFF40
        cpu.h = (ppu_lcdc_addr >> 8) as u8;
        cpu.l = (ppu_lcdc_addr & 0xFF) as u8;

        // The PPU placeholder read_byte returns 0xFF and prints a message.
        // We can't easily check the println! here without more complex test setup.
        // So we'll just check the returned value.
        // The PPU now returns the actual LCDC value (0x91 by default) - This will change with stubbed PPU
        cpu.ld_a_hl_mem();
        // TODO: Update this test once new PPU is integrated. For now, it should read the placeholder value from Bus.
        assert_eq!(
            cpu.a,
            0x91, // Placeholder value for LCDC from Bus read_byte
            "Reading from PPU LCDC register should return its default value from Bus stub"
        );
    }

    #[test]
    fn test_apu_io_write_placeholder() {
        // Similar to PPU, checks routing to APU placeholder.
        let (mut cpu, bus) = setup_test_env();

        let apu_ch1_vol_addr = 0xFF12; // NR12 - Channel 1 Volume & Envelope
        cpu.a = 0xF3; // Value to write

        // LD (HL), A where HL = 0xFF12
        cpu.h = (apu_ch1_vol_addr >> 8) as u8;
        cpu.l = (apu_ch1_vol_addr & 0xFF) as u8;

        // The APU placeholder write_byte prints a message.
        // We can't check the println! easily. This test mainly ensures no panic and it completes.
        // A more advanced test would involve a mock APU or capturing stdout.
        cpu.ld_hl_mem_a();

        // To make the test somewhat useful, we can try reading back.
        // With the current APU implementation the write is ignored because the
        // APU is powered off by default, so the register reads as 0.
        let read_back_val = bus.borrow().read_byte(apu_ch1_vol_addr);
        assert_eq!(
            read_back_val, 0x00,
            "Reading NR12 after write while APU is disabled should return 0"
        );
    }

    #[test]
    fn test_read_from_rom_area() {
        // ROM data for this specific test
        let mut test_rom_data = vec![0; 0x200]; // 512 bytes ROM
        test_rom_data[0x00] = 0xAA;
        test_rom_data[0xFF] = 0xBB; // Last byte of the initial 0x100 dummy ROM in setup_test_env
                                    // This will be overwritten by the new bus instance's ROM.
        test_rom_data[0x1FE] = 0xCC; // Second to last byte of our 512 byte ROM
        test_rom_data[0x1FF] = 0xDD; // Last byte of our 512 byte ROM

        let bus_with_specific_rom = Rc::new(RefCell::new(Bus::new(test_rom_data.clone()))); // Use clone if test_rom_data is needed later for asserts

        // 1. Reading from an address within the bounds of rom_data returns the correct byte.
        assert_eq!(
            bus_with_specific_rom.borrow().read_byte(0x0000),
            0xAA,
            "Read from ROM start incorrect"
        );

        // We used 0x100 in setup_test_env, but this test creates its own Bus instance.
        // Let's re-evaluate the address for 0xBB based on test_rom_data.
        // If we want to test the specific rom_data[0xFF] = 0xBB, we need to ensure it's set in test_rom_data.
        // The previous rom_data[0xFF] = 0xBB was a bit confusing as it mixed setup_test_env's ROM
        // with this test's specific ROM.
        // Let's make it clear:
        let _specific_addr_ff = 0x00FF;
        // Ensure test_rom_data has a value at 0x00FF if we are to test it.
        // The current test_rom_data is initialized with 0s, then specific values.
        // So test_rom_data[0x00FF] would be 0 unless we set it.
        // Let's assume the intention was to read a value we explicitly set in test_rom_data for this test.
        // The original rom_data[0xFF] = 0xBB would have been for the `bus` from `setup_test_env()`,
        // not `bus_with_specific_rom`.

        // Let's pick a different address for clarity with test_rom_data.
        let mid_rom_addr = 0x00A5;
        // test_rom_data is currently all zeros except for 0x00, 0x1FE, 0x1FF.
        // So, reading from 0x00A5 should return 0.
        assert_eq!(
            bus_with_specific_rom.borrow().read_byte(mid_rom_addr),
            0x00,
            "Read from middle of ROM (unset byte) incorrect"
        );

        // Let's set a value in the middle of test_rom_data and test it
        // We need to recreate the bus if we modify test_rom_data after Bus::new
        // Or, modify test_rom_data before Bus::new
        // For simplicity, let's just use the values already set.

        // 2. Reading from an address just at the end of rom_data returns the correct byte.
        assert_eq!(
            bus_with_specific_rom.borrow().read_byte(0x01FE),
            0xCC,
            "Read from ROM near end incorrect"
        );
        assert_eq!(
            bus_with_specific_rom.borrow().read_byte(0x01FF),
            0xDD,
            "Read from ROM end incorrect"
        );

        // 3. Reading from an address within 0x0000..=0x7FFF but outside the bounds of loaded rom_data returns 0xFF.
        // test_rom_data has size 0x200 (512 bytes). So addresses from 0x0200 up to 0x7FFF are out of bounds.
        assert_eq!(
            bus_with_specific_rom.borrow().read_byte(0x0200),
            0xFF,
            "Read from ROM out of bounds (start) incorrect"
        );
        assert_eq!(
            bus_with_specific_rom.borrow().read_byte(0x3000),
            0xFF,
            "Read from ROM out of bounds (middle) incorrect"
        );
        assert_eq!(
            bus_with_specific_rom.borrow().read_byte(0x7FFF),
            0xFF,
            "Read from ROM out of bounds (end of range) incorrect"
        );

        // Test reading from original setup_test_env bus to ensure its rom_data is used.
        let (_cpu, bus_from_setup) = setup_test_env(); // This bus has rom_data vec![0; 0x100]
                                                       // So, bus_from_setup.rom_data[0] should be 0.
                                                       // And bus_from_setup.rom_data[0xFF] should be 0.
                                                       // And bus_from_setup.rom_data[0x100] should be out of bounds (0xFF).
        assert_eq!(
            bus_from_setup.borrow().read_byte(0x0000),
            0x00,
            "Read from setup_test_env ROM (start) incorrect"
        );
        assert_eq!(
            bus_from_setup.borrow().read_byte(0x00FF),
            0x00,
            "Read from setup_test_env ROM (end) incorrect"
        );
        assert_eq!(
            bus_from_setup.borrow().read_byte(0x0100),
            0xFF,
            "Read from setup_test_env ROM (out of bounds) incorrect"
        );
    }

    #[test]
    fn test_serial_output_capture() {
        let rom_data = vec![0; 0x100]; // Dummy ROM
        let mut bus = Bus::new(rom_data); // Not using Rc<RefCell<Bus>> here as we need direct mutable access for this test.

        // Write "Test" to serial port (0xFF01)
        bus.write_byte(0xFF01, b'T');
        bus.write_byte(0xFF01, b'e');
        bus.write_byte(0xFF01, b's');
        bus.write_byte(0xFF01, b't');

        assert_eq!(
            bus.get_serial_output_string(),
            "Test",
            "Serial output string incorrect after initial write"
        );

        // Write more bytes
        bus.write_byte(0xFF01, b' ');
        bus.write_byte(0xFF01, b'1');
        bus.write_byte(0xFF01, b'2');
        bus.write_byte(0xFF01, b'3');

        assert_eq!(
            bus.get_serial_output_string(),
            "Test 123",
            "Serial output string incorrect after further writes"
        );

        // Check internal Vec<u8> directly
        assert_eq!(
            bus.serial_output,
            vec![b'T', b'e', b's', b't', b' ', b'1', b'2', b'3']
        );
    }

    #[test]
    fn test_bus_system_mode_selection() {
        // Test CGB mode selection (0x80)
        let mut rom_cgb1 = vec![0u8; 0x150];
        rom_cgb1[0x0143] = 0x80;
        let bus_cgb1 = Bus::new(rom_cgb1);
        assert_eq!(
            bus_cgb1.get_model(),
            models::GameBoyModel::GenericCGB,
            "Failed CGB mode (0x80)"
        );

        // Test CGB mode selection (0xC0)
        let mut rom_cgb2 = vec![0u8; 0x150];
        rom_cgb2[0x0143] = 0xC0;
        let bus_cgb2 = Bus::new(rom_cgb2);
        assert_eq!(
            bus_cgb2.get_model(),
            models::GameBoyModel::GenericCGB,
            "Failed CGB mode (0xC0)"
        );

        // Test DMG mode selection (0x00)
        let mut rom_dmg = vec![0u8; 0x150];
        rom_dmg[0x0143] = 0x00;
        let bus_dmg = Bus::new(rom_dmg);
        assert_eq!(
            bus_dmg.get_model(),
            models::GameBoyModel::DMG,
            "Failed DMG mode (0x00)"
        );

        // Test DMG mode selection (other value)
        let mut rom_dmg_other = vec![0u8; 0x150];
        rom_dmg_other[0x0143] = 0x40; // Some other non-CGB value
        let bus_dmg_other = Bus::new(rom_dmg_other);
        assert_eq!(
            bus_dmg_other.get_model(),
            models::GameBoyModel::DMG,
            "Failed DMG mode (other)"
        );

        // Test short ROM (less than 0x0144 bytes) defaults to DMG
        let short_rom = vec![0u8; 0x100];
        let bus_short_rom = Bus::new(short_rom);
        assert_eq!(
            bus_short_rom.get_model(),
            models::GameBoyModel::DMG,
            "Short ROM should default to DMG"
        );
    }

    #[test]
    fn test_key1_register_read_write() {
        let rom_data = vec![0u8; 0x150]; // Generic ROM
        let mut bus = Bus::new(rom_data);

        // Initial state
        assert!(
            !bus.get_is_double_speed(),
            "Initial is_double_speed should be false"
        );
        assert!(
            !bus.get_key1_prepare_speed_switch(),
            "Initial key1_prepare_speed_switch should be false"
        );
        // KEY1 read: speed_bit (0) | prepare_bit (0) | 0x7E = 0x7E
        assert_eq!(bus.read_byte(0xFF4D), 0x7E, "Initial KEY1 read incorrect");

        // Write to KEY1 to set prepare_speed_switch
        bus.write_byte(0xFF4D, 0x01); // Bit 0 set
        assert!(
            bus.get_key1_prepare_speed_switch(),
            "key1_prepare_speed_switch should be true after writing 0x01"
        );
        // KEY1 read: speed_bit (0) | prepare_bit (1) | 0x7E = 0x7F
        assert_eq!(
            bus.read_byte(0xFF4D),
            0x7F,
            "KEY1 read after setting prepare bit incorrect"
        );

        // Write to KEY1 to clear prepare_speed_switch
        bus.write_byte(0xFF4D, 0xFE); // Bit 0 clear (value & 0x01 == 0)
        assert!(
            !bus.get_key1_prepare_speed_switch(),
            "key1_prepare_speed_switch should be false after writing 0xFE"
        );
        // KEY1 read: speed_bit (0) | prepare_bit (0) | 0x7E = 0x7E
        assert_eq!(
            bus.read_byte(0xFF4D),
            0x7E,
            "KEY1 read after clearing prepare bit incorrect"
        );

        // Toggle speed mode (internal state change)
        bus.toggle_speed_mode();
        assert!(
            bus.get_is_double_speed(),
            "is_double_speed should be true after toggle"
        );
        // KEY1 read: speed_bit (0x80) | prepare_bit (0) | 0x7E = 0xFE
        assert_eq!(
            bus.read_byte(0xFF4D),
            0xFE,
            "KEY1 read after toggling speed mode incorrect"
        );

        // Set prepare switch again while in double speed
        bus.write_byte(0xFF4D, 0x01);
        assert!(
            bus.get_key1_prepare_speed_switch(),
            "key1_prepare_speed_switch should be true again"
        );
        // KEY1 read: speed_bit (0x80) | prepare_bit (1) | 0x7E = 0xFF
        assert_eq!(
            bus.read_byte(0xFF4D),
            0xFF,
            "KEY1 read with double speed and prepare bit set incorrect"
        );
    }

    #[test]
    fn test_save_and_load_files() {
        let mut rom_data = vec![0u8; 0x150];
        rom_data[0x0147] = 0x00; // NoMBC
        rom_data[0x0149] = 0x02; // 8KB RAM
        let mut bus = Bus::new(rom_data);
        let temp_dir = env::temp_dir();
        let rom_path = temp_dir.join("test_rom.gb");

        bus.write_byte(0xA000, 0x42);
        bus.save_save_files(rom_path.to_str().unwrap());

        if let Some(ram) = bus.mbc.get_ram_mut() {
            for b in ram.iter_mut() {
                *b = 0;
            }
        }

        bus.load_save_files(rom_path.to_str().unwrap());
        assert_eq!(bus.read_byte(0xA000), 0x42);

        let _ = std::fs::remove_file(rom_path.with_extension("sav"));
        let _ = std::fs::remove_file(rom_path.with_extension("rtc"));
    }
}
