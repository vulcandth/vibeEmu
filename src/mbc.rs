// src/mbc.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeType {
    NoMBC,
    MBC1,
    MBC2,
    MBC3,
    MBC5,
    MBC6,
    MBC7,
    MBC30, // Added for MBC30, potentially for MBC3 with larger RAM/Flash
    Unknown(u8),
}

impl CartridgeType {
    pub fn from_byte(byte: u8) -> CartridgeType {
        match byte {
            0x00 => CartridgeType::NoMBC,
            0x08 => CartridgeType::NoMBC, // ROM+RAM (usually NoMBC)
            0x09 => CartridgeType::NoMBC, // ROM+RAM+BATTERY (usually NoMBC)
            0x01..=0x03 => CartridgeType::MBC1,
            0x05..=0x06 => CartridgeType::MBC2,
            0x0F..=0x13 => CartridgeType::MBC3, // Standard MBC3 types
            // Hypothetical or less common type byte for MBC30.
            // If an official one exists, this should be updated.
            // For now, if a game uses MBC3 logic with >32KB RAM, it might still use one of the 0x0F-0x13 type bytes
            // and rely on the RAM size byte 0x0149. This MBC30 variant is for explicit distinction if ever needed.
            0x14 => CartridgeType::MBC30, // Hypothetical for MBC30
            0x19..=0x1E => CartridgeType::MBC5, // Includes RAM/BATTERY/RUMBLE variants
            0x20 => CartridgeType::MBC6,        // Often listed as MBC6+RAM+BATTERY
            0x22 => CartridgeType::MBC7,        // MBC7+SENSOR+RUMBLE+RAM+BATTERY
            // Add other specific mappings if necessary
            // For example, some sources list 0x1F for MBC5, which is covered by 0x19..=0x1E
            _ => CartridgeType::Unknown(byte),
        }
    }
}

pub struct MBC30 {
    // Internally, for stub purposes, use an MBC3 for its behavior.
    internal_mbc3: MBC3,
    // _cartridge_type_byte: u8, // Store if needed for future differentiation
}

impl MBC30 {
    pub fn new(rom_data: Vec<u8>, ram_data_size: usize, _cartridge_type_byte: u8) -> Self {
        println!("Warning: MBC30 cartridge type detected. Using MBC3 behavior as a stub. Ensure MBC3 correctly handles RAM size if this ROM expects >32KB (up to 64KB).");
        MBC30 {
            internal_mbc3: MBC3::new(rom_data, ram_data_size),
            // _cartridge_type_byte: cartridge_type_byte,
        }
    }
}

impl MemoryBankController for MBC30 {
    fn read_rom(&self, addr: u16) -> u8 {
        self.internal_mbc3.read_rom(addr)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        self.internal_mbc3.write_rom(addr, value)
    }

    fn read_ram(&self, addr: u16) -> u8 {
        self.internal_mbc3.read_ram(addr)
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        self.internal_mbc3.write_ram(addr, value)
    }
}

pub struct MBC7 {
    // Internally, for stub purposes, use NoMBC for basic ROM access.
    internal_nombs: NoMBC,
    // _cartridge_type_byte: u8, // Store if needed for future, e.g. for sensor/EEPROM type
}

impl MBC7 {
    pub fn new(rom_data: Vec<u8>, ram_data_size: usize, _cartridge_type_byte: u8) -> Self {
        println!("Warning: MBC7 cartridge type detected. Using NoMBC behavior as a stub. EEPROM and accelerometer are NOT implemented.");
        // MBC7 doesn't typically use RAM from the header for A000-BFFF.
        // It has a small serial EEPROM. Passing ram_data_size to NoMBC might be misleading,
        // but NoMBC handles ram_size = 0 correctly (no RAM allocated).
        // For a stub, this is acceptable.
        MBC7 {
            internal_nombs: NoMBC::new(rom_data, ram_data_size),
            // _cartridge_type_byte: cartridge_type_byte,
        }
    }
}

impl MemoryBankController for MBC7 {
    fn read_rom(&self, addr: u16) -> u8 {
        self.internal_nombs.read_rom(addr)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        // MBC7 has specific registers in 0x0000-0x7FFF, but for a NoMBC stub,
        // these writes would be ignored by internal_nombs.write_rom.
        self.internal_nombs.write_rom(addr, value)
    }

    fn read_ram(&self, addr: u16) -> u8 {
        // MBC7 uses A000-AFFF for EEPROM and sensor I/O, not traditional RAM.
        // NoMBC stub will likely return 0xFF if ram_data_size was 0.
        // This is an acceptable stub behavior for reads from this range.
        self.internal_nombs.read_ram(addr)
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        // Similar to read_ram, NoMBC stub will ignore writes if no RAM.
        self.internal_nombs.write_ram(addr, value)
    }
}

pub struct MBC6 {
    // Internally, for stub purposes, use an MBC1 for basic bank switching behavior.
    internal_mbc1: MBC1,
    // _cartridge_type_byte: u8, // Store if needed for future, unused by MBC1 delegate
}

impl MBC6 {
    pub fn new(rom_data: Vec<u8>, ram_data_size: usize, _cartridge_type_byte: u8) -> Self {
        println!("Warning: MBC6 cartridge type detected. Using MBC1 behavior as a stub. Proper MBC6 behavior is not yet implemented.");
        MBC6 {
            internal_mbc1: MBC1::new(rom_data, ram_data_size),
            // _cartridge_type_byte: cartridge_type_byte,
        }
    }
}

impl MemoryBankController for MBC6 {
    fn read_rom(&self, addr: u16) -> u8 {
        self.internal_mbc1.read_rom(addr)
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        self.internal_mbc1.write_rom(addr, value)
    }

    fn read_ram(&self, addr: u16) -> u8 {
        self.internal_mbc1.read_ram(addr)
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        self.internal_mbc1.write_ram(addr, value)
    }
}

pub struct MBC5 {
    rom_data: Vec<u8>,
    ram_data: Vec<u8>,
    ram_enabled: bool,
    selected_rom_bank_low: u8,  // Lower 8 bits of ROM bank number
    selected_rom_bank_high: u8, // 9th bit of ROM bank number (0 or 1)
    selected_ram_bank: u8,      // 4 bits for RAM bank (0-15)
    num_rom_banks: usize,       // Max 512 for 8MB ROM
    num_ram_banks: usize,       // Max 16 for 128KB RAM
    has_rumble: bool,           // True if cartridge type indicates rumble
}

impl MBC5 {
    pub fn new(rom_data: Vec<u8>, ram_data_size: usize, cartridge_type_byte: u8) -> Self {
        let mut num_rom_banks = if rom_data.is_empty() { 0 } else { rom_data.len() / (16 * 1024) };
        if num_rom_banks == 0 { num_rom_banks = 1; }
        // MBC5 can have up to 512 ROM banks (8MB)
        if num_rom_banks > 512 { num_rom_banks = 512; }


        let mut num_ram_banks = if ram_data_size == 0 { 0 } else { ram_data_size / (8 * 1024) };
        if ram_data_size > 0 && num_ram_banks == 0 { num_ram_banks = 1; }
        // MBC5 can have up to 128KB RAM (16 banks)
        if num_ram_banks > 16 { num_ram_banks = 16; }

        let has_rumble = matches!(cartridge_type_byte, 0x1C | 0x1D | 0x1E);

        MBC5 {
            rom_data,
            ram_data: vec![0; ram_data_size.min(128 * 1024)], // Cap RAM data vec at 128KB
            ram_enabled: false,
            selected_rom_bank_low: 0, // Bank 0 selected initially for 0x4000-0x7FFF
            selected_rom_bank_high: 0,
            selected_ram_bank: 0,
            num_rom_banks,
            num_ram_banks,
            has_rumble,
        }
    }
}

impl MemoryBankController for MBC5 {
    fn read_rom(&self, addr: u16) -> u8 {
        let addr_usize = addr as usize;
        if addr_usize < 0x4000 { // ROM Bank 00 (fixed)
            if addr_usize < self.rom_data.len() {
                self.rom_data[addr_usize]
            } else {
                0xFF
            }
        } else { // Switchable ROM bank area (0x4000 - 0x7FFF)
            let full_rom_bank = ((self.selected_rom_bank_high as usize) << 8) | (self.selected_rom_bank_low as usize);

            let actual_bank = if self.num_rom_banks > 0 {
                full_rom_bank % self.num_rom_banks
            } else {
                0 // Should not happen
            };

            let base_addr = actual_bank * (16 * 1024);
            let final_addr = base_addr + (addr_usize - 0x4000);

            if final_addr < self.rom_data.len() {
                self.rom_data[final_addr]
            } else {
                0xFF
            }
        }
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => { // RAM Enable
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x2FFF => { // ROM Bank Number (Lower 8 bits)
                self.selected_rom_bank_low = value;
            }
            0x3000..=0x3FFF => { // ROM Bank Number (9th bit)
                self.selected_rom_bank_high = value & 0x01;
            }
            0x4000..=0x5FFF => { // RAM Bank Number
                let ram_bank = value & 0x0F; // Bits 0-3 for RAM Bank
                // Rumble motor control is often tied to bit 3 of this register
                // if self.has_rumble && (ram_bank & 0x08) != 0 { /* TODO: activate rumble */ }
                // else if self.has_rumble { /* TODO: deactivate rumble */ }

                // For MBC5, num_ram_banks can be up to 16.
                // So, no modulo is needed if ram_bank is already 0-15 from (value & 0x0F).
                // However, if num_ram_banks is less (e.g. 4 banks for 32KB), then it should alias.
                if self.num_ram_banks > 0 {
                    self.selected_ram_bank = ram_bank % (self.num_ram_banks as u8);
                } else {
                    self.selected_ram_bank = 0; // No RAM banks to select.
                }

            }
            0x6000..=0x7FFF => { // Unused by MBC5 for banking
                // Some sources say this area might be used by some MBC5 variants (e.g., for rumble intensity),
                // but generally, it's ignored for basic banking.
            }
            _ => {}
        }
    }

    fn read_ram(&self, addr: u16) -> u8 { // addr is 0x0000-0x1FFF relative to 0xA000
        if !self.ram_enabled || self.ram_data.is_empty() {
            return 0xFF;
        }
        // self.selected_ram_bank is already masked by num_ram_banks on write.
        let base_addr = (self.selected_ram_bank as usize) * (8 * 1024);
        let final_addr = base_addr + (addr as usize);

        if final_addr < self.ram_data.len() {
            self.ram_data[final_addr]
        } else {
            0xFF
        }
    }

    fn write_ram(&mut self, addr: u16, value: u8) { // addr is 0x0000-0x1FFF relative to 0xA000
        if !self.ram_enabled || self.ram_data.is_empty() {
            return;
        }
        // self.selected_ram_bank is already masked.
        let base_addr = (self.selected_ram_bank as usize) * (8 * 1024);
        let final_addr = base_addr + (addr as usize);

        if final_addr < self.ram_data.len() {
            self.ram_data[final_addr] = value;
        }
        // If self.has_rumble, RAM writes might also affect rumble motor state.
        // For now, this is just a plain RAM write.
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct RtcRegisters {
    seconds: u8, // 0-59
    minutes: u8, // 0-59
    hours: u8,   // 0-23
    day_counter_low: u8, // Lower 8 bits of day counter
    day_counter_high: u8, // Bit 0: MSB of day counter (bit 8)
                         // Bit 6: Halt (0=active, 1=halted)
                         // Bit 7: Day counter carry (1=overflow)
}

pub struct MBC3 {
    rom_data: Vec<u8>,
    ram_data: Vec<u8>,
    ram_and_rtc_enabled: bool,
    selected_rom_bank: usize, // Range 1-127. Value 0 written to register becomes 1.
    selected_ram_bank_or_rtc_reg: u8, // RAM:0x00-0x07 (max 4 banks for MBC3, so 0-3), RTC:0x08-0x0C
    rtc_registers: RtcRegisters,
    latched_rtc_registers: RtcRegisters,
    rtc_latch_state: u8, // Stores the previous value written to 0x6000-0x7FFF for latching
    num_rom_banks: usize,
    num_ram_banks: usize, // Max 4 RAM banks (0-3) for MBC3 (32KB)
}

impl MBC3 {
    pub fn new(rom_data: Vec<u8>, ram_data_size: usize) -> Self {
        let mut num_rom_banks = if rom_data.is_empty() { 0 } else { rom_data.len() / (16 * 1024) };
        if num_rom_banks == 0 { num_rom_banks = 1; }

        // MBC3 typically supports up to 32KB RAM, which is 4 banks of 8KB.
        let mut num_ram_banks = if ram_data_size == 0 { 0 } else { ram_data_size / (8 * 1024) };
        if ram_data_size > 0 && num_ram_banks == 0 { num_ram_banks = 1; }
        if num_ram_banks > 4 { num_ram_banks = 4; } // Cap at 4 RAM banks for MBC3


        MBC3 {
            rom_data,
            ram_data: vec![0; ram_data_size.min(32 * 1024)], // Cap RAM data vec at 32KB
            ram_and_rtc_enabled: false,
            selected_rom_bank: 1, // Default to bank 1
            selected_ram_bank_or_rtc_reg: 0,
            rtc_registers: RtcRegisters::default(),
            latched_rtc_registers: RtcRegisters::default(),
            rtc_latch_state: 0xFF, // Initial non-zero state for latch sequence
            num_rom_banks,
            num_ram_banks,
        }
    }
}

impl MemoryBankController for MBC3 {
    fn read_rom(&self, addr: u16) -> u8 {
        let addr_usize = addr as usize;
        if addr_usize < 0x4000 { // ROM Bank 00 (fixed)
            // Always reads from the first 16KB of the ROM.
            if addr_usize < self.rom_data.len() {
                self.rom_data[addr_usize]
            } else {
                0xFF // Should generally not happen if addr is < 0x4000 and ROM is valid
            }
        } else { // ROM Bank 01-7F (switchable)
            // selected_rom_bank is 1-127.
            // Max ROM for MBC3 is 2MB (128 banks).
            // Bank N means Nth 16KB block.
            // Aliasing: selected_rom_bank can be up to 127. num_rom_banks can be up to 128.
            // If selected_rom_bank = 100, num_rom_banks = 64. 100 % 64 = 36.
            let bank_to_use = if self.num_rom_banks > 0 {
                self.selected_rom_bank % self.num_rom_banks
            } else {
                0 // Should not happen
            };
            // This bank_to_use is 0-indexed. If selected_rom_bank was a multiple of num_rom_banks, it becomes 0.
            // This is standard aliasing.

            let base_addr = bank_to_use * (16 * 1024);
            let final_addr = base_addr + (addr_usize - 0x4000);

            if final_addr < self.rom_data.len() {
                self.rom_data[final_addr]
            } else {
                0xFF
            }
        }
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => { // RAM and RTC Enable
                self.ram_and_rtc_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => { // ROM Bank Number
                let mut bank = value & 0x7F; // 7 bits for ROM bank
                if bank == 0 {
                    bank = 1; // Bank 0 cannot be selected, maps to bank 1
                }
                self.selected_rom_bank = bank as usize;
            }
            0x4000..=0x5FFF => { // RAM Bank Number or RTC Register Select
                self.selected_ram_bank_or_rtc_reg = value;
            }
            0x6000..=0x7FFF => { // Latch Clock Data
                // Latch on 0x00 -> 0x01 sequence
                if self.rtc_latch_state == 0x00 && value == 0x01 {
                    self.latched_rtc_registers = self.rtc_registers; // TODO: Actual RTC update logic is missing
                }
                self.rtc_latch_state = value;
            }
            _ => {} // Other ROM area writes are ignored
        }
    }

    fn read_ram(&self, addr: u16) -> u8 { // addr is 0x0000-0x1FFF relative to 0xA000
        if !self.ram_and_rtc_enabled {
            return 0xFF;
        }

        let selection = self.selected_ram_bank_or_rtc_reg;

        if selection >= 0x08 && selection <= 0x0C { // RTC Register Read
            match selection {
                0x08 => self.latched_rtc_registers.seconds,
                0x09 => self.latched_rtc_registers.minutes,
                0x0A => self.latched_rtc_registers.hours,
                0x0B => self.latched_rtc_registers.day_counter_low,
                0x0C => self.latched_rtc_registers.day_counter_high,
                _ => 0xFF, // Should not happen due to range check
            }
        } else if selection <= 0x03 { // RAM Bank Read (MBC3 has max 4 RAM banks: 0-3)
            if self.ram_data.is_empty() || (selection as usize) >= self.num_ram_banks {
                return 0xFF; // Accessing non-existent RAM bank
            }
            let base_addr = (selection as usize) * (8 * 1024);
            let final_addr = base_addr + (addr as usize);

            if final_addr < self.ram_data.len() {
                self.ram_data[final_addr]
            } else {
                0xFF // Address out of bounds for this RAM bank
            }
        } else { // Invalid RAM bank selection (0x04-0x07)
            0xFF
        }
    }

    fn write_ram(&mut self, addr: u16, value: u8) { // addr is 0x0000-0x1FFF relative to 0xA000
        if !self.ram_and_rtc_enabled {
            return;
        }

        let selection = self.selected_ram_bank_or_rtc_reg;

        if selection >= 0x08 && selection <= 0x0C { // RTC Register Write
            // TODO: Implement actual RTC updates if Halt bit is not set
            // For now, just update internal registers.
            // Consider read-only nature of some bits if Halt is not active.
            match selection {
                0x08 => self.rtc_registers.seconds = value % 60, // Ensure value is within valid range
                0x09 => self.rtc_registers.minutes = value % 60,
                0x0A => self.rtc_registers.hours = value % 24,
                0x0B => self.rtc_registers.day_counter_low = value,
                0x0C => { // Day Counter High & Control
                    // Only update relevant bits: Day MSB (bit 0), Halt (bit 6), Carry (bit 7 read-only)
                    // For now, allow writing to Halt and Day MSB. Carry is not written by game.
                    self.rtc_registers.day_counter_high = (self.rtc_registers.day_counter_high & !0x41) | (value & 0x41);
                }
                _ => {} // Should not happen
            }
        } else if selection <= 0x03 { // RAM Bank Write
            if self.ram_data.is_empty() || (selection as usize) >= self.num_ram_banks {
                return; // Writing to non-existent RAM bank
            }
            let base_addr = (selection as usize) * (8 * 1024);
            let final_addr = base_addr + (addr as usize);

            if final_addr < self.ram_data.len() {
                self.ram_data[final_addr] = value;
            }
        }
        // Writes to invalid RAM bank selections (0x04-0x07) are ignored.
    }
}

pub trait MemoryBankController {
    fn read_rom(&self, addr: u16) -> u8;
    fn write_rom(&mut self, addr: u16, value: u8); // For control registers, bank switching etc.
    fn read_ram(&self, addr: u16) -> u8;
    fn write_ram(&mut self, addr: u16, value: u8);
}

// Basic MBC implementations

pub struct NoMBC {
    rom_data: Vec<u8>,
    ram_data: Vec<u8>,
    ram_enabled: bool, // Typically true if ram_data.len() > 0 for NoMBC
}

impl NoMBC {
    pub fn new(rom_data: Vec<u8>, ram_size: usize) -> Self {
        NoMBC {
            rom_data,
            ram_data: vec![0; ram_size],
            ram_enabled: ram_size > 0,
        }
    }
}

impl MemoryBankController for NoMBC {
    fn read_rom(&self, addr: u16) -> u8 {
        // ROM addresses are typically 0x0000-0x7FFF.
        // The bus should ensure addr is within this range for ROM reads.
        // Here, we check against the actual size of the loaded ROM data.
        if (addr as usize) < self.rom_data.len() {
            self.rom_data[addr as usize]
        } else {
            // This case might indicate an attempt to read from a ROM address
            // that is valid (e.g., 0x7000) but beyond the end of a smaller ROM.
            0xFF // Standard return for out-of-bounds or non-existent memory
        }
    }

    fn write_rom(&mut self, _addr: u16, _value: u8) {
        // For NoMBC, writes to ROM address space are typically ignored.
        // Some specific cartridge types (not type 0x00 NoMBC) might use
        // writes to 0x0000-0x7FFF for RAM enable or bank switching.
        // For standard NoMBC, this does nothing.
        // eprintln!("Attempted to write to ROM address: {:#04X} with value {:#02X}", addr, value);
    }

    fn read_ram(&self, addr: u16) -> u8 {
        // RAM addresses are typically 0xA000-0xBFFF on the bus.
        // The MBC receives the address relative to the start of the RAM window (e.g., 0x0000 for 0xA000).
        if self.ram_enabled && (addr as usize) < self.ram_data.len() {
            self.ram_data[addr as usize]
        } else {
            // Attempt to read from disabled RAM or out-of-bounds RAM address
            0xFF
        }
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if self.ram_enabled && (addr as usize) < self.ram_data.len() {
            self.ram_data[addr as usize] = value;
        }
        // Else, ignore write to disabled RAM or out-of-bounds RAM address
    }
}

pub struct MBC2 {
    rom_data: Vec<u8>,
    // MBC2 has 512 x 4 bits of RAM. We use a Vec<u8> of size 512,
    // storing each 4-bit nibble in the lower 4 bits of a u8.
    ram_data: Vec<u8>,
    ram_enabled: bool,
    selected_rom_bank: usize, // Stores 4-bit value (1-15, value 0 written maps to 1)
    num_rom_banks: usize,     // Total number of 16KB ROM banks
}

impl MBC2 {
    pub fn new(rom_data: Vec<u8>) -> Self { // ram_data_size is not needed
        let mut num_rom_banks = if rom_data.is_empty() { 0 } else { rom_data.len() / (16 * 1024) };
        if num_rom_banks == 0 { num_rom_banks = 1; }

        MBC2 {
            rom_data,
            ram_data: vec![0; 512], // 512 nibbles, address range 0xA000-0xA1FF
            ram_enabled: false,
            selected_rom_bank: 1, // Default ROM bank is 1
            num_rom_banks,
        }
    }
}

impl MemoryBankController for MBC2 {
    fn read_rom(&self, addr: u16) -> u8 {
        let addr_usize = addr as usize;
        if addr_usize < 0x4000 { // ROM Bank 00 (fixed)
            // Always reads from the first 16KB of the ROM.
            if addr_usize < self.rom_data.len() {
                self.rom_data[addr_usize]
            } else {
                0xFF // Should not happen if ROM is at least 16KB and addr is < 0x4000
            }
        } else { // ROM Bank 01-0F (switchable)
            // selected_rom_bank is 1-15. Mask with num_rom_banks.
            // num_rom_banks for MBC2 can be up to 16 (for 256KB ROMs).
            // If selected_rom_bank is 1, it's the bank at rom_data offset 1 * 16KB.
            let current_bank = if self.num_rom_banks > 0 {
                self.selected_rom_bank % self.num_rom_banks
            } else {
                0 // Should not occur
            };
            // If selected_rom_bank is, for example, 16 and num_rom_banks is 16, then 16 % 16 = 0.
            // This means bank 0 would be selected here, which is fine for MBC2 as banks are 0-indexed internally for calculation.
            // However, selected_rom_bank is 1-15. If it's 1, index 1.
            // A common model is that bank numbers are 0-indexed for array access.
            // If selected_rom_bank stores 1-15, then (selected_rom_bank) could be used if num_rom_banks handles aliasing.
            // Or, (selected_rom_bank -1) if num_rom_banks is the count.
            // Given selected_rom_bank is 1-15, and max 16 banks for MBC2 (0-15).
            // So, if selected_rom_bank = 1, maps to bank_idx 1.
            // If selected_rom_bank = 15, maps to bank_idx 15.
            // The modulo might not be entirely correct if selected_rom_bank can exceed num_rom_banks
            // and should alias. For MBC2, max ROM is 256KB (16 banks). selected_rom_bank (4 bits) is 1-15.
            // So, aliasing by modulo is generally how it's handled.
            // Let's use `self.selected_rom_bank` directly as it's 1-15, and it should map to bank 1 to 15.
            // Bank 0 is fixed. So selected_rom_bank N maps to Nth 16KB block.
            let actual_bank_idx = self.selected_rom_bank; // This is 1-15 (or 0 if num_rom_banks is 1 and selected is 1%1=0?)
                                                        // No, selected_rom_bank is 1-15.
                                                        // If selected_rom_bank is 1, it's bank 1.
                                                        // If num_rom_banks = 1, then 1 % 1 = 0. This would map to bank 0.
                                                        // This is not right.
            // The selected_rom_bank is 1-15. If rom has only 8 banks (0-7), then bank 9 (0b1001) should map to bank 1 (9 % 8 = 1).
            // So, `(self.selected_rom_bank % self.num_rom_banks)` is the 0-indexed bank.
            // But if selected_rom_bank is 1 and num_rom_banks is 16, 1%16 = 1. Correct.
            // If selected_rom_bank is 15 and num_rom_banks is 16, 15%16 = 15. Correct.
            // If selected_rom_bank is 4 and num_rom_banks is 2 (32KB ROM), 4%2 = 0. Correct (maps to bank 0 of the switchable area, which is overall bank 0).
            // This means the switchable bank can map to bank 0 if selected_rom_bank is a multiple of num_rom_banks.
            // This is not typical; usually bank 0 is fixed and switchable banks are from bank 1 upwards.
            // For MBC2, the selected_rom_bank (1-15) directly chooses the bank. Max 16 banks.
            // So, bank N means Nth 16KB block.
            let bank_to_use = self.selected_rom_bank; // This is 1-15.
            // Ensure it doesn't exceed available banks. Max 16 banks total (0-15).
            // If num_rom_banks is, say, 8 (128KB ROM), and bank_to_use is 10, it should wrap.
            // bank_to_use = bank_to_use % self.num_rom_banks; // This makes it 0-indexed.
            // If bank_to_use was 0, it becomes 1.
            // If bank_to_use is, e.g. 10, and num_rom_banks is 8. 10 % 8 = 2. This is bank 2. Correct.
            // But if bank_to_use is 8, num_rom_banks is 8. 8 % 8 = 0. This would map to bank 0.
            // This seems to be a common behavior: bank numbers on MBCs often alias with modulo.

            let effective_bank_idx = self.selected_rom_bank % self.num_rom_banks;
            // However, if `self.selected_rom_bank` is, say, 16 (not possible with 4 bits, max 15),
            // and `num_rom_banks` is 16, `16 % 16 = 0`. This would map to bank 0.
            // Let's use the simpler direct mapping if value is within bounds.
            // MBC2 has max 16 banks (256KB). selected_rom_bank is 1-15.
            // It seems `selected_rom_bank` directly indicates the bank number (1-indexed for conceptual bank).
            // So, bank `N` is at `N * 16KB` offset.
            let base_addr = self.selected_rom_bank * (16 * 1024);
            let final_addr = base_addr + (addr_usize - 0x4000);

            if final_addr < self.rom_data.len() {
                self.rom_data[final_addr]
            } else {
                // This might happen if selected_rom_bank points beyond the actual ROM size
                // (e.g. ROM is 32KB (2 banks), selected_rom_bank is 5).
                // MBCs usually mirror or return 0xFF. Modulo behavior is common.
                // Let's refine with modulo for safety, ensuring bank index is always valid.
                let wrapped_bank = if self.num_rom_banks > 0 {
                                       self.selected_rom_bank % self.num_rom_banks
                                   } else { 0 };
                // If wrapped_bank is 0 here (e.g. selected 16, num_banks 16), and bank 0 is fixed,
                // this implies an issue. However, selected_rom_bank is 1-15.
                // So `self.selected_rom_bank % self.num_rom_banks` will be 0 only if selected_rom_bank is a multiple of num_rom_banks.
                // e.g. num_rom_banks = 4 (64KB). selected_rom_bank can be 1,2,3,4,5..15.
                // if selected_rom_bank = 4, 4%4=0. if selected_rom_bank = 8, 8%4=0.
                // This means it maps to bank 0. This is a common behavior.
                let base_addr_wrapped = wrapped_bank * (16 * 1024);
                let final_addr_wrapped = base_addr_wrapped + (addr_usize - 0x4000);
                if final_addr_wrapped < self.rom_data.len() {
                    self.rom_data[final_addr_wrapped]
                } else {
                    0xFF
                }
            }
        }
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        // For MBC2, only bit 8 of the address matters for selecting the register.
        if (addr & 0x0100) == 0 { // Bit 8 is 0: RAM Enable/Disable (addr range 0x0000-0x1FFF, but check bit 8)
                                  // More accurately, any address in 0x0000-3FFF.
                                  // The check should be for the region, then the bit.
                                  // Let's assume addr is already confirmed to be 0x0000-0x3FFF by the bus for this type of write.
            if addr < 0x2000 { // Typically 0x0000-0x1FFF for RAM enable by convention, but it's bit 8.
                               // Let's follow prompt: (addr & 0x0100) == 0
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
             // If it's 0x2000-0x3FFF but bit 8 is 0 (e.g. addr 0x2000), it's still RAM enable.
             // The prompt is: if (addr & 0x0100) == 0 for RAM enable,
             // and if (addr & 0x0100) != 0 for ROM bank select.
             // This means any write to 0x0000-0x00FF, 0x0200-0x02FF ... 0x1E00-0x1EFF is RAM enable.
             // And any write to 0x0100-0x01FF, 0x0300-0x03FF ... 0x1F00-0x1FFF is ROM bank select.
             // This is the correct interpretation of "only bit 8 of address matters".
        } else { // Bit 8 is 1: ROM Bank Select (addr range 0x2000-0x3FFF, but check bit 8)
            if addr < 0x4000 { // Ensure it's within the control register area
                let mut bank = value & 0x0F; // Lower 4 bits
                if bank == 0 {
                    bank = 1;
                }
                self.selected_rom_bank = bank as usize;
            }
        }
        // Writes to 0x4000-0x7FFF are ignored.
    }

    fn read_ram(&self, addr: u16) -> u8 { // addr is 0x0000-0x01FF for MBC2 RAM
        if !self.ram_enabled {
            return 0xFF; // Open bus, typically 0xFF
        }
        // MBC2 RAM is 512 nibbles, addressed from A000-A1FF.
        // addr is relative here, so 0x000-0x1FF.
        if (addr as usize) < self.ram_data.len() { // ram_data.len() is 512
            // Return lower 4 bits, upper 4 bits are read as 1s.
            (self.ram_data[addr as usize] & 0x0F) | 0xF0
        } else {
            0xFF // Should not happen if bus correctly maps A000-A1FF to 0x000-0x1FF for MBC2 RAM
        }
    }

    fn write_ram(&mut self, addr: u16, value: u8) { // addr is 0x0000-0x01FF
        if !self.ram_enabled {
            return;
        }
        if (addr as usize) < self.ram_data.len() {
            self.ram_data[addr as usize] = value & 0x0F; // Store only lower 4 bits
        }
    }
}

pub struct MBC1 {
    rom_data: Vec<u8>,
    ram_data: Vec<u8>,
    ram_enabled: bool,
    rom_bank_low_bits: usize,  // Stores the 5 low bits for ROM bank (1-31, 0 maps to 1)
    rom_bank_high_bits: usize, // Stores the 2 high bits for ROM bank or RAM bank
    selected_ram_bank: usize,  // Effective RAM bank number (0-3)
    banking_mode: u8,          // 0 for ROM banking mode, 1 for RAM banking mode
    num_rom_banks: usize,      // Total number of 16KB ROM banks
    num_ram_banks: usize,      // Total number of 8KB RAM banks
}

impl MBC1 {
    pub fn new(rom_data: Vec<u8>, ram_data_size: usize) -> Self {
        let mut num_rom_banks = if rom_data.is_empty() { 0 } else { rom_data.len() / (16 * 1024) };
        if num_rom_banks == 0 { num_rom_banks = 1; } // Ensure at least 1 ROM bank even for <16KB ROMs (though rare)

        // Calculate num_ram_banks. If ram_data_size is > 0 but less than 8KB, it's still one bank.
        let mut num_ram_banks = if ram_data_size == 0 { 0 } else { ram_data_size / (8 * 1024) };
        if ram_data_size > 0 && num_ram_banks == 0 { num_ram_banks = 1; }


        MBC1 {
            rom_data,
            ram_data: vec![0; ram_data_size],
            ram_enabled: false,        // RAM is initially disabled
            rom_bank_low_bits: 1,      // Default ROM bank is 1
            rom_bank_high_bits: 0,     // Default high bits are 0
            selected_ram_bank: 0,      // Default RAM bank is 0
            banking_mode: 0,           // Default to ROM banking mode
            num_rom_banks,
            num_ram_banks,
        }
    }
}

impl MemoryBankController for MBC1 {
    fn read_rom(&self, addr: u16) -> u8 {
        let addr_usize = addr as usize;

        if addr_usize < 0x4000 { // ROM Bank 00 (fixed, or affected by high bits in large ROM mode)
            let mut effective_bank0 = 0;
            if self.banking_mode == 0 { // ROM Banking Mode
                // For larger ROMs (>=1MB), bits A13-A14 of rom_bank_high_bits can affect bank 00.
                // This is when rom_bank_high_bits are set via 0x4000-0x5FFF writes.
                // The high bits (from rom_bank_high_bits) select banks $00, $20, $40, $60.
                // This only applies if num_rom_banks is large enough (e.g. 64 for 1MB, 128 for 2MB)
                // For MBC1, max ROM is 2MB (128 banks). Max RAM is 32KB (4 banks).
                // If num_rom_banks >= 64 (1MB), then high bits affect bank 0.
                // This is often simplified: if banking_mode is 0, high bits select ROM bank part.
                // The problem description implies this logic:
                 if self.num_rom_banks >= 64 { // Simplified: apply high bits if ROM is 1MB or more
                    effective_bank0 = (self.rom_bank_high_bits << 5);
                 }
            }
            // No matter what, bank 0 is always bank 0 for smaller ROMs or when high bits are 0.
            // The effective_bank0 needs to be masked by the actual number of ROM banks.
            if self.num_rom_banks > 0 {
                effective_bank0 %= self.num_rom_banks;
            } else {
                effective_bank0 = 0; // Should not happen
            }

            let base_addr = effective_bank0 * (16 * 1024);
            let final_addr = base_addr + addr_usize;

            if final_addr < self.rom_data.len() {
                self.rom_data[final_addr]
            } else {
                0xFF // Address out of bounds for the ROM data vector
            }
        } else { // ROM Bank 01-7F (switchable)
            let mut current_rom_bank = self.rom_bank_low_bits; // This is 1-31
            // In ROM banking mode, the high bits are combined.
            if self.banking_mode == 0 {
                current_rom_bank |= self.rom_bank_high_bits << 5;
            }
            // The resulting bank number can be $00-$7F, but $00, $20, $40, $60 are special
            // and effectively aliases of $01, $21, $41, $61 for some MBC1 revisions/interpretations.
            // However, simpler model: rom_bank_low_bits cannot be 0. If 0 is written, it's treated as 1.
            // So current_rom_bank will be at least 1.
            // The bank number must be masked by the actual number of banks.
            let actual_bank = if self.num_rom_banks > 0 {
                 current_rom_bank % self.num_rom_banks
            } else {
                0 // Should not happen
            };
            // If actual_bank becomes 0 due to modulo (e.g. current_rom_bank is 64, num_rom_banks is 64),
            // it means bank 0. Some MBC1 variants map bank 0, 20, 40, 60 to 1, 21, 41, 61.
            // Let's assume `rom_bank_low_bits` is already 1-31, so `current_rom_bank` is never 0, 20, 40, 60
            // unless `rom_bank_high_bits` makes it so.
            // A common simplification: if bank number is 0, use 1.
            // But `rom_bank_low_bits` is already handled to be >= 1.
            // The `current_rom_bank` is effectively 0-indexed for calculation after this.
            // Example: bank $01 selected -> actual_bank = 1. Offset for bank 1.
            // If bank $00 is selected (e.g. low_bits=0 which becomes 1, high_bits=0), then actual_bank = 1.
            // If the logic is that `effective_bank` is 0-indexed for calculation:
            // `let actual_bank_for_offset = if self.num_rom_banks > 0 { effective_bank % self.num_rom_banks } else { 0 };`
            // The provided logic was `effective_bank % self.num_rom_banks`.
            // Let's stick to the idea that `current_rom_bank` is the bank number (1-indexed style from registers)
            // and then we map it to 0-indexed for calculation.
            // So, if current_rom_bank is 1, it's the first *switchable* bank, which is the 2nd 16KB block.
            // So, `(actual_bank_value_if_1_indexed - 1)` if base is 0. Or simply use `actual_bank_value_if_0_indexed`.
            // The code `actual_bank_for_offset = effective_bank % self.num_rom_banks` from prompt is 0-indexed.
            // So this `actual_bank` can be used directly.

            let base_addr = actual_bank * (16 * 1024);
            let final_addr = base_addr + (addr_usize - 0x4000);

            if final_addr < self.rom_data.len() {
                self.rom_data[final_addr]
            } else {
                0xFF
            }
        }
    }

    fn write_rom(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => { // RAM Enable
                self.ram_enabled = (value & 0x0F) == 0x0A;
            }
            0x2000..=0x3FFF => { // ROM Bank Number (lower 5 bits)
                let mut bank_num = value & 0x1F;
                if bank_num == 0 { // Bank 0 is not selectable here, maps to bank 1
                    bank_num = 1;
                }
                self.rom_bank_low_bits = bank_num as usize;
            }
            0x4000..=0x5FFF => { // RAM Bank Number or ROM Bank Number (upper 2 bits)
                let data = (value & 0x03) as usize;
                if self.banking_mode == 0 { // ROM Banking Mode: data is upper 2 bits of ROM bank
                    self.rom_bank_high_bits = data;
                } else { // RAM Banking Mode: data is RAM bank number
                    // self.selected_ram_bank = data; // This was the direct assignment
                    // Mask by num_ram_banks if RAM banks are fewer than 4
                    if self.num_ram_banks > 0 {
                        self.selected_ram_bank = data % self.num_ram_banks;
                    } else {
                        self.selected_ram_bank = 0; // No RAM banks to select
                    }
                }
            }
            0x6000..=0x7FFF => { // Banking Mode Select
                self.banking_mode = value & 0x01;
            }
            _ => {} // Writes to other ROM areas are ignored
        }
    }

    fn read_ram(&self, addr: u16) -> u8 {
        if !self.ram_enabled || self.ram_data.is_empty() {
            return 0xFF;
        }

        let effective_ram_bank = if self.banking_mode == 1 { // RAM Mode
            self.selected_ram_bank
        } else { // ROM Mode
            0 // Only RAM Bank 0 is accessible in ROM Mode
        };

        // selected_ram_bank is already masked by num_ram_banks on write.
        // effective_ram_bank here will be valid if num_ram_banks > 0.
        // If num_ram_banks is 0, ram_data.is_empty() check handles it.

        let base_addr = effective_ram_bank * (8 * 1024);
        let final_addr = base_addr + (addr as usize); // addr is 0x0000-0x1FFF from bus

        if final_addr < self.ram_data.len() {
            self.ram_data[final_addr]
        } else {
            0xFF // Address out of bounds for RAM
        }
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        if !self.ram_enabled || self.ram_data.is_empty() {
            return;
        }

        let effective_ram_bank = if self.banking_mode == 1 { // RAM Mode
            self.selected_ram_bank
        } else { // ROM Mode
            0 // Only RAM Bank 0 is accessible
        };

        let base_addr = effective_ram_bank * (8 * 1024);
        let final_addr = base_addr + (addr as usize);

        if final_addr < self.ram_data.len() {
            self.ram_data[final_addr] = value;
        }
    }
}
