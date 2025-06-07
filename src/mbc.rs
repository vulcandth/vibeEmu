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
    // has_rumble: bool,        // True if cartridge type indicates rumble. Field removed as unused.
}

impl MBC5 {
    pub fn new(rom_data: Vec<u8>, ram_data_size: usize, _cartridge_type_byte: u8) -> Self { // cartridge_type_byte changed to _cartridge_type_byte
        let mut num_rom_banks = if rom_data.is_empty() { 0 } else { rom_data.len() / (16 * 1024) };
        if num_rom_banks == 0 { num_rom_banks = 1; }
        // MBC5 can have up to 512 ROM banks (8MB)
        if num_rom_banks > 512 { num_rom_banks = 512; }


        let mut num_ram_banks = if ram_data_size == 0 { 0 } else { ram_data_size / (8 * 1024) };
        if ram_data_size > 0 && num_ram_banks == 0 { num_ram_banks = 1; }
        // MBC5 can have up to 128KB RAM (16 banks)
        if num_ram_banks > 16 { num_ram_banks = 16; }

        // let has_rumble = matches!(cartridge_type_byte, 0x1C | 0x1D | 0x1E); // Rumble detection removed as field is unused.

        MBC5 {
            rom_data,
            ram_data: vec![0; ram_data_size.min(128 * 1024)], // Cap RAM data vec at 128KB
            ram_enabled: false,
            selected_rom_bank_low: 0, // Bank 0 selected initially for 0x4000-0x7FFF
            selected_rom_bank_high: 0,
            selected_ram_bank: 0,
            num_rom_banks,
            num_ram_banks,
            // has_rumble, // Field removed
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
                if self.has_rumble && (ram_bank & 0x08) != 0 { /* TODO: activate rumble */ }
                else if self.has_rumble { /* TODO: deactivate rumble */ }
                // Rumble functionality removed, has_rumble field is gone.

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
            let _current_bank = if self.num_rom_banks > 0 { // Marked as unused
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
            let _actual_bank_idx = self.selected_rom_bank; // Marked as unused. This is 1-15 (or 0 if num_rom_banks is 1 and selected is 1%1=0?)
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
            let _bank_to_use = self.selected_rom_bank; // Marked as unused. This is 1-15.
            // Ensure it doesn't exceed available banks. Max 16 banks total (0-15).
            // If num_rom_banks is, say, 8 (128KB ROM), and bank_to_use is 10, it should wrap.
            // bank_to_use = bank_to_use % self.num_rom_banks; // This makes it 0-indexed.
            // If bank_to_use was 0, it becomes 1.
            // If bank_to_use is, e.g. 10, and num_rom_banks is 8. 10 % 8 = 2. This is bank 2. Correct.
            // But if bank_to_use is 8, num_rom_banks is 8. 8 % 8 = 0. This would map to bank 0.
            // This seems to be a common behavior: bank numbers on MBCs often alias with modulo.

            let _effective_bank_idx = self.selected_rom_bank % self.num_rom_banks; // Marked as unused
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
                    effective_bank0 = self.rom_bank_high_bits << 5;
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

        let effective_ram_bank;
        let relative_addr = addr as usize;

        if self.banking_mode == 1 { // RAM Mode
            effective_ram_bank = self.selected_ram_bank;
        } else { // ROM Mode
            effective_ram_bank = 0;
            // In ROM mode, only addresses 0x0000-0x1FFF in RAM bank 0 are accessible.
            if relative_addr >= (8 * 1024) {
                return 0xFF; // Address is out of bounds for ROM mode's 8KB window on bank 0
            }
        }
        // effective_ram_bank is already masked by num_ram_banks if banking_mode == 1.
        // For banking_mode == 0, it's always 0.

        let base_addr = effective_ram_bank * (8 * 1024);
        let final_addr = base_addr + relative_addr;

        // Final check against the actual allocated RAM vector size
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

        let effective_ram_bank;
        // addr is relative to 0xA000, so it's 0x0000-0x1FFF for an 8KB bank.
        let relative_addr = addr as usize;

        if self.banking_mode == 1 { // RAM Mode
            effective_ram_bank = self.selected_ram_bank;
        } else { // ROM Mode
            effective_ram_bank = 0;
            // In ROM mode, only addresses 0x0000-0x1FFF in RAM bank 0 are accessible.
            if relative_addr >= (8 * 1024) {
                return; // Address is out of bounds for ROM mode's 8KB window on bank 0
            }
        }
        // effective_ram_bank is already masked by num_ram_banks if banking_mode == 1.
        // For banking_mode == 0, it's always 0.

        let base_addr = effective_ram_bank * (8 * 1024);
        let final_addr = base_addr + relative_addr;

        // Final check against the actual allocated RAM vector size
        if final_addr < self.ram_data.len() {
            self.ram_data[final_addr] = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*; // To import MBC structs and MemoryBankController trait

    // Helper function to create ROM data with a predictable pattern
    fn create_rom(size_kb: usize, start_value: u8) -> Vec<u8> {
        let size_bytes = size_kb * 1024;
        let mut rom = vec![0; size_bytes];
        for i in 0..size_bytes {
            rom[i] = ((start_value as usize + i) % 256) as u8;
        }
        rom
    }

    // Helper function to create RAM data vector with a predictable pattern
    // Renamed from create_ram to avoid conflict if we were to create an MBC with RAM directly
    #[allow(dead_code)]
    fn create_ram_data(size_kb: usize, start_value: u8) -> Vec<u8> {
        if size_kb == 0 {
            return Vec::new();
        }
        let size_bytes = size_kb * 1024;
        let mut ram = vec![0; size_bytes];
        for i in 0..size_bytes {
            ram[i] = ((start_value as usize + i) % 256) as u8;
        }
        ram
    }

    // Helper to calculate number of banks
    #[allow(dead_code)]
    fn get_num_banks(total_size_bytes: usize, bank_size_bytes: usize) -> usize {
        if total_size_bytes == 0 || bank_size_bytes == 0 {
            return 0;
        }
        let num = total_size_bytes / bank_size_bytes;
        if total_size_bytes % bank_size_bytes != 0 {
            num + 1
        } else {
            num
        }
    }

    // Tests for NoMBC
    #[test]
    fn test_nom_mbc_rom_read() {
        let rom_data = create_rom(32, 0); // 32KB ROM
        let mbc = NoMBC::new(rom_data.clone(), 0); // No RAM

        // Read from ROM
        for i in 0..rom_data.len() {
            assert_eq!(mbc.read_rom(i as u16), rom_data[i], "Mismatch at ROM addr {}", i);
        }
        // Read out of bounds
        assert_eq!(mbc.read_rom(0x8000), 0xFF, "Out of bounds ROM read did not return 0xFF");
    }

    #[test]
    fn test_nom_mbc_ram_read_write() {
        let rom_data = create_rom(16, 0); // 16KB ROM
        let ram_size_kb = 2;

        // NoMBC constructor takes ram_size in bytes
        let mut mbc = NoMBC::new(rom_data, ram_size_kb * 1024);

        // Check initial RAM state (should be all 0s as NoMBC initializes its own RAM vec)
        for i in 0..(ram_size_kb * 1024) {
             assert_eq!(mbc.read_ram(i as u16), 0x00, "Initial RAM at {} not 0x00", i);
        }

        // Write to RAM
        let test_val1 = 0xAB;
        mbc.write_ram(0x0000, test_val1);
        assert_eq!(mbc.read_ram(0x0000), test_val1, "RAM write/read failed at 0x0000");

        let test_val2 = 0xCD;
        let ram_addr = (ram_size_kb * 1024 - 1) as u16; // Last address in RAM
        mbc.write_ram(ram_addr, test_val2);
        assert_eq!(mbc.read_ram(ram_addr), test_val2, "RAM write/read failed at last RAM addr {}", ram_addr);

        let out_of_bounds_ram_addr = (ram_size_kb * 1024) as u16;
        assert_eq!(mbc.read_ram(out_of_bounds_ram_addr), 0xFF, "Out of bounds RAM read did not return 0xFF");
        mbc.write_ram(out_of_bounds_ram_addr, 0xFF);
        assert_eq!(mbc.read_ram(out_of_bounds_ram_addr), 0xFF, "Read after out of bounds RAM write did not return 0xFF");
    }

    #[test]
    fn test_nom_mbc_no_ram() {
        let rom_data = create_rom(16, 0);
        let mut mbc = NoMBC::new(rom_data, 0); // 0 RAM size

        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from non-existent RAM did not return 0xFF");
        mbc.write_ram(0x0000, 0xAB);
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read after write to non-existent RAM did not return 0xFF");
    }

    #[test]
    fn test_nom_mbc_write_to_rom_area() {
        let rom_data = create_rom(32, 0);
        let rom_data_clone = rom_data.clone();
        let mut mbc = NoMBC::new(rom_data, 0);

        mbc.write_rom(0x1000, 0xAB);
        assert_eq!(mbc.read_rom(0x1000), rom_data_clone[0x1000], "ROM content changed after write_rom");

        mbc.write_rom(0x0000, 0xCD);
        assert_eq!(mbc.read_rom(0x0000), rom_data_clone[0x0000], "ROM content changed at 0x0000 after write_rom");
    }

    // Tests for MBC1
    #[test]
    fn test_mbc1_ram_enable_disable() {
        let rom = create_rom(256, 0); // 256KB ROM
        let mut mbc = MBC1::new(rom, 8 * 1024); // 8KB RAM

        // RAM should be disabled initially
        assert!(!mbc.ram_enabled, "RAM is not initially disabled");
        mbc.write_ram(0x0000, 0xFF);
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from disabled RAM should return 0xFF");

        // Enable RAM
        mbc.write_rom(0x0000, 0x0A);
        assert!(mbc.ram_enabled, "RAM is not enabled after writing 0x0A to 0x0000-0x1FFF");

        // Write and read from enabled RAM
        mbc.write_ram(0x0000, 0x55);
        assert_eq!(mbc.read_ram(0x0000), 0x55, "RAM read/write failed when enabled");

        // Disable RAM
        mbc.write_rom(0x0000, 0x00);
        assert!(!mbc.ram_enabled, "RAM is not disabled after writing 0x00 to 0x0000-0x1FFF");
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from re-disabled RAM should return 0xFF");
    }

    #[test]
    fn test_mbc1_rom_bank_switching_low_bits() {
        let rom_size_kb = 256; // 256KB ROM -> 16 banks (0-15)
        let rom = create_rom(rom_size_kb, 0);
        let mut mbc = MBC1::new(rom.clone(), 0); // No RAM for this test

        // Bank 0 (0x0000-0x3FFF) should always read from bank 0 of the ROM initially
        for i in 0..0x4000 {
            assert_eq!(mbc.read_rom(i as u16), rom[i], "Initial bank 0 read mismatch at {}", i);
        }

        // Switch to bank 1 (low bits)
        mbc.write_rom(0x2000, 1); // Bank 1
        assert_eq!(mbc.rom_bank_low_bits, 1);
        for i in 0..0x4000 { // Reading from 0x4000-0x7FFF
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 1 read mismatch at offset {}", i);
        }

        // Writing 0 to low bits should select bank 1
        mbc.write_rom(0x2000, 0);
        assert_eq!(mbc.rom_bank_low_bits, 1);
         for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 1 (after writing 0) read mismatch at offset {}", i);
        }

        // Switch to bank 5 (0b00101)
        mbc.write_rom(0x2000, 5);
        assert_eq!(mbc.rom_bank_low_bits, 5);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[5 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 5 read mismatch at offset {}", i);
        }

        // Switch to bank 15 (0b01111) (assuming rom_size_kb is large enough)
        mbc.write_rom(0x2000, 15);
        assert_eq!(mbc.rom_bank_low_bits, 15);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[15 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 15 read mismatch at offset {}", i);
        }

        // Test aliasing: if we select bank 17 (0b10001) on a 16-bank ROM, it should map to bank 1 (17 % 16 = 1)
        // Low bits only take 5 bits, so max value is 31 (0x1F).
        // MBC1 num_rom_banks is (rom_data.len() / (16 * 1024)). For 256KB, it's 16.
        // If bank_num is 17 (0x11), it should wrap to 17 % 16 = 1.
        mbc.write_rom(0x2000, 0x11); // bank 17
        assert_eq!(mbc.rom_bank_low_bits, 0x11); // internal low bits store 0x11
                                                 // effective bank for reading is (0x11 % 16) = 1
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i]; // Bank 1
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 17 (aliased to 1) read mismatch at offset {}", i);
        }
    }

    #[test]
    fn test_mbc1_rom_bank_switching_high_bits_rom_mode() {
        let rom_size_kb = 1024; // 1MB ROM -> 64 banks (0-63)
        let rom = create_rom(rom_size_kb, 0);
        let mut mbc = MBC1::new(rom.clone(), 0); // No RAM

        mbc.write_rom(0x6000, 0x00); // ROM banking mode
        assert_eq!(mbc.banking_mode, 0);

        // Select low bits: bank 1 (0b00001)
        mbc.write_rom(0x2000, 1);
        assert_eq!(mbc.rom_bank_low_bits, 1);

        // Select high bits: bank 0b01 (for banks 0x20-0x3F)
        // rom_bank_high_bits = 1
        mbc.write_rom(0x4000, 0x01);
        assert_eq!(mbc.rom_bank_high_bits, 1);

        // Effective bank = (high_bits << 5) | low_bits = (1 << 5) | 1 = 0b0100001 = 32 + 1 = 33
        // Read from 0x4000-0x7FFF should use bank 33
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[33 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 33 (1:1) read mismatch at offset {}", i);
        }

        // Test how bank 0 (0x0000-0x3FFF) is affected by high bits with 1MB ROM
        // effective_bank0 = (self.rom_bank_high_bits << 5) % self.num_rom_banks
        // Here, (1 << 5) % 64 = 32 % 64 = 32. So bank 0 should read from ROM bank 32.
        for i in 0..0x4000 {
            let expected_val = rom[32 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(i as u16), expected_val, "Bank 0 area (remapped to 32) read mismatch at {}", i);
        }

        // Select high bits: 0b10 (for banks 0x40-0x5F)
        // rom_bank_high_bits = 2
        mbc.write_rom(0x4000, 0x02);
        assert_eq!(mbc.rom_bank_high_bits, 2);
        // low_bits is still 1. Effective bank = (2 << 5) | 1 = 0b1000001 = 64 + 1 = 65
        // This will be aliased by num_rom_banks (64). So, 65 % 64 = 1.
        // Read from 0x4000-0x7FFF should use bank 1
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 65 (aliased to 1) read mismatch at offset {}", i);
        }
        // Bank 0 area mapping: (2 << 5) % 64 = 64 % 64 = 0. So bank 0 should read from ROM bank 0.
        for i in 0..0x4000 {
            let expected_val = rom[0 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(i as u16), expected_val, "Bank 0 area (remapped to 0) read mismatch at {}", i);
        }
    }

    #[test]
    fn test_mbc1_ram_banking_mode() {
        let rom = create_rom(256, 0); // 256KB ROM
        // 32KB RAM -> 4 banks of 8KB
        let ram_size_bytes = 32 * 1024;
        let mut mbc = MBC1::new(rom.clone(), ram_size_bytes);

        // Enable RAM
        mbc.write_rom(0x0000, 0x0A);
        // Switch to RAM banking mode
        mbc.write_rom(0x6000, 0x01);
        assert_eq!(mbc.banking_mode, 1);

        // In RAM banking mode, writes to 0x4000-0x5FFF select RAM bank
        // Select RAM bank 0
        mbc.write_rom(0x4000, 0x00);
        assert_eq!(mbc.selected_ram_bank, 0);
        mbc.write_ram(0x0000, 0xA0);
        mbc.write_ram(0x0001, 0xA1);
        assert_eq!(mbc.read_ram(0x0000), 0xA0);
        assert_eq!(mbc.read_ram(0x0001), 0xA1);

        // Select RAM bank 1
        mbc.write_rom(0x4000, 0x01);
        assert_eq!(mbc.selected_ram_bank, 1);
        mbc.write_ram(0x0000, 0xB0); // This is offset 0 in RAM bank 1
        mbc.write_ram(0x0001, 0xB1);
        assert_eq!(mbc.read_ram(0x0000), 0xB0);
        assert_eq!(mbc.read_ram(0x0001), 0xB1);
        // Check that bank 0 data is still there
        mbc.write_rom(0x4000, 0x00); // Switch back to RAM bank 0
        assert_eq!(mbc.read_ram(0x0000), 0xA0);
        assert_eq!(mbc.read_ram(0x0001), 0xA1);


        // Select RAM bank 3 (max for 32KB RAM)
        mbc.write_rom(0x4000, 0x03);
        assert_eq!(mbc.selected_ram_bank, 3);
        mbc.write_ram(0x1FFF, 0xC3); // Last byte of RAM bank 3
        assert_eq!(mbc.read_ram(0x1FFF), 0xC3);

        // Test RAM bank aliasing if num_ram_banks < 4 (e.g. 16KB RAM = 2 banks)
        let mut mbc_small_ram = MBC1::new(create_rom(64,0), 16 * 1024); // 16KB RAM -> 2 banks
        mbc_small_ram.write_rom(0x0000, 0x0A); // Enable RAM
        mbc_small_ram.write_rom(0x6000, 0x01); // RAM banking mode

        mbc_small_ram.write_rom(0x4000, 0x00); // RAM bank 0
        mbc_small_ram.write_ram(0x0000, 0xAA);

        mbc_small_ram.write_rom(0x4000, 0x01); // RAM bank 1
        mbc_small_ram.write_ram(0x0000, 0xBB);

        // Writing 0x02 to select RAM bank should alias to 0x02 % num_ram_banks (2) = 0
        mbc_small_ram.write_rom(0x4000, 0x02);
        assert_eq!(mbc_small_ram.selected_ram_bank, 0);
        assert_eq!(mbc_small_ram.read_ram(0x0000), 0xAA, "RAM bank aliasing failed (0x02 -> bank 0)");

        mbc_small_ram.write_rom(0x4000, 0x03); // Should alias to 0x03 % 2 = 1
        assert_eq!(mbc_small_ram.selected_ram_bank, 1);
        assert_eq!(mbc_small_ram.read_ram(0x0000), 0xBB, "RAM bank aliasing failed (0x03 -> bank 1)");

        // In RAM banking mode, ROM bank 0 (0x0000-0x3FFF) is fixed to bank 0.
        // rom_bank_high_bits are ignored for bank 0 selection.
        mbc.write_rom(0x2000, 5); // low bits for ROM
        mbc.write_rom(0x4000, 0x01); // This sets RAM bank to 1, NOT rom_bank_high_bits

        // Check bank 0 of ROM
        for i in 0..0x4000 {
            assert_eq!(mbc.read_rom(i as u16), rom[i], "ROM Bank 0 not fixed in RAM mode, offset {}", i);
        }
        // Check switchable ROM area (0x4000-0x7FFF) - should use only rom_bank_low_bits (5)
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[5 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Switchable ROM bank mismatch in RAM mode, offset {}", i);
        }
    }

    #[test]
    fn test_mbc1_rom_mode_ram_access() {
        let rom = create_rom(256, 0);
        let mut mbc = MBC1::new(rom.clone(), 8 * 1024); // 8KB RAM (1 bank)

        mbc.write_rom(0x0000, 0x0A); // Enable RAM
        mbc.write_rom(0x6000, 0x00); // ROM banking mode
        assert_eq!(mbc.banking_mode, 0);

        // In ROM banking mode, only RAM bank 0 is accessible
        // Writes to 0x4000-0x5FFF control rom_bank_high_bits, not RAM bank
        mbc.write_rom(0x4000, 0x01); // Set rom_bank_high_bits = 1

        // Write to RAM (should go to bank 0)
        mbc.write_ram(0x0000, 0xDD);
        mbc.write_ram(0x1FFF, 0xEE); // Last byte of 8KB RAM

        assert_eq!(mbc.read_ram(0x0000), 0xDD);
        assert_eq!(mbc.read_ram(0x1FFF), 0xEE);

        // If we had more RAM banks, they wouldn't be accessible here.
        // Let's test with 32KB RAM (4 banks)
        let mut mbc_32kb_ram = MBC1::new(create_rom(256,0), 32 * 1024);
        mbc_32kb_ram.write_rom(0x0000, 0x0A); // Enable RAM
        mbc_32kb_ram.write_rom(0x6000, 0x00); // ROM banking mode

        // Write to RAM bank 0
        mbc_32kb_ram.write_ram(0x0000, 0x11);
        mbc_32kb_ram.write_ram(0x1FFF, 0x22); // end of bank 0

        // Attempt to select a different RAM bank via 0x4000 (this sets rom_bank_high_bits)
        mbc_32kb_ram.write_rom(0x4000, 0x01); // rom_bank_high_bits = 1, RAM bank should remain 0

        // Try writing to where bank 1 would be, it should still go to bank 0
        // Or rather, reading from bank 0 should still yield bank 0's data
        assert_eq!(mbc_32kb_ram.read_ram(0x0000), 0x11, "RAM bank changed in ROM mode");
        assert_eq!(mbc_32kb_ram.read_ram(0x1FFF), 0x22, "RAM bank changed in ROM mode (end)");

        // Write to offset 0x2000 (start of where bank 1 would map)
        // This address is relative to A000. So A000 + 0x2000 = C000.
        // If RAM is 32KB, it spans A000-BFFF (bank 0), C000-DFFF (bank 1) etc.
        // But read_ram/write_ram take relative offsets.
        // In ROM mode, only the first 8KB (0x0000-0x1FFF relative) of RAM is accessible.
        // So writing to relative addr 0x2000 should be out of bounds for the accessible RAM window.
        mbc_32kb_ram.write_ram(0x2000, 0x33);
        assert_eq!(mbc_32kb_ram.read_ram(0x2000), 0xFF, "Accessing RAM beyond bank 0 in ROM mode should fail");
    }

    #[test]
    fn test_mbc1_rom_bank_0_remapping_for_large_roms() {
        // Test with 1MB ROM (64 banks)
        let rom_1mb_data = create_rom(1024, 0);
        let mut mbc_1mb = MBC1::new(rom_1mb_data.clone(), 0);
        mbc_1mb.write_rom(0x6000, 0x00); // ROM mode

        // Set rom_bank_high_bits to 0b01 (selects banks 0x20-0x3F for 0x0000-0x3FFF area)
        mbc_1mb.write_rom(0x4000, 0x01); // rom_bank_high_bits = 1
        // Fixed bank 0 (0000-3FFF) should now map to ROM bank (1 << 5) = 32
        for i in 0..0x4000 {
            let expected = rom_1mb_data[32 * (16 * 1024) + i];
            assert_eq!(mbc_1mb.read_rom(i as u16), expected, "1MB ROM bank 0 remap (to 32) failed at {}", i);
        }

        // Set rom_bank_high_bits to 0b10 (selects banks 0x40-0x5F for 0x0000-0x3FFF area)
        mbc_1mb.write_rom(0x4000, 0x02); // rom_bank_high_bits = 2
        // Fixed bank 0 (0000-3FFF) should now map to ROM bank (2 << 5) = 64.
        // Since there are 64 banks (0-63), this will be 64 % 64 = 0.
        for i in 0..0x4000 {
            let expected = rom_1mb_data[0 * (16 * 1024) + i]; // Bank 0
            assert_eq!(mbc_1mb.read_rom(i as u16), expected, "1MB ROM bank 0 remap (to 0 via 64) failed at {}", i);
        }

        // Test with 512KB ROM (32 banks) - Bank 0 remapping should NOT occur
        let rom_512kb_data = create_rom(512, 50);
        let mut mbc_512kb = MBC1::new(rom_512kb_data.clone(), 0);
        mbc_512kb.write_rom(0x6000, 0x00); // ROM mode

        mbc_512kb.write_rom(0x4000, 0x01); // rom_bank_high_bits = 1
        // num_rom_banks = 32. Condition `self.num_rom_banks >= 64` is false.
        // So, effective_bank0 should remain 0.
        for i in 0..0x4000 {
            let expected = rom_512kb_data[0 * (16 * 1024) + i]; // Should always be bank 0
            assert_eq!(mbc_512kb.read_rom(i as u16), expected, "512KB ROM bank 0 remap (should not happen) failed at {}", i);
        }
    }

    // Tests for MBC2
    #[test]
    fn test_mbc2_ram_enable_disable() {
        let rom = create_rom(128, 0); // 128KB ROM
        let mut mbc = MBC2::new(rom);

        // RAM should be disabled initially
        assert!(!mbc.ram_enabled, "RAM is not initially disabled");
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from disabled RAM should be 0xFF (or 0xF0 | stored_nibble)");

        // Enable RAM: Write 0x0A to an address in 0x0000-0x1FFF where addr bit 8 is 0.
        // Example: 0x0000, 0x00FF, 0x0200 (but not 0x0100, 0x0300)
        mbc.write_rom(0x0000, 0x0A); // Valid addr for RAM enable
        assert!(mbc.ram_enabled, "RAM not enabled after writing 0x0A to 0x0000");

        // Write and read from enabled RAM (checking nibble behavior)
        mbc.write_ram(0x0000, 0xBC); // Write 0xBC, only 0x0C should be stored
        assert_eq!(mbc.read_ram(0x0000), 0xFC, "RAM read/write incorrect (expected 0xFC for value 0x0C)");

        // Test another RAM address
        mbc.write_ram(0x01AF, 0x12); // Store 0x02
        assert_eq!(mbc.read_ram(0x01AF), 0xF2, "RAM read/write incorrect (expected 0xF2 for value 0x02)");


        // Disable RAM: Write something else (e.g. 0x00) to RAM enable address
        mbc.write_rom(0x0000, 0x00);
        assert!(!mbc.ram_enabled, "RAM not disabled after writing 0x00 to 0x0000");
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from re-disabled RAM should return 0xFF");

        // Test other addresses for RAM enable control
        // Address bit 8 must be 0.
        mbc.write_rom(0x0250, 0x0A); // Bit 8 is 0 (0x250 & 0x100 == 0)
        assert!(mbc.ram_enabled, "RAM not enabled via addr 0x0250");
        mbc.write_rom(0x0250, 0x00);
        assert!(!mbc.ram_enabled, "RAM not disabled via addr 0x0250");

        // Address bit 8 is 1 - should NOT affect RAM enable
        mbc.write_rom(0x0150, 0x0A); // Bit 8 is 1 (0x150 & 0x100 != 0) -> This is ROM bank select
        assert!(!mbc.ram_enabled, "RAM affected by write to ROM bank select area (addr bit 8 was 1)");
    }

    #[test]
    fn test_mbc2_rom_bank_switching() {
        let rom_size_kb = 256; // 256KB ROM -> 16 banks (0-15)
        let rom = create_rom(rom_size_kb, 0);
        let mut mbc = MBC2::new(rom.clone());

        // Bank 0 (0x0000-0x3FFF) is fixed
        for i in 0..0x4000 {
            assert_eq!(mbc.read_rom(i as u16), rom[i], "Initial bank 0 read mismatch at {}", i);
        }

        // Switch to bank 1: Write to addr 0x2000-0x3FFF where addr bit 8 is 1.
        // Example: 0x2100, 0x21FF, 0x2300 (but not 0x2000, 0x2200)
        // Value's lower 4 bits determine bank. 0 maps to 1.
        mbc.write_rom(0x2100, 1); // Select bank 1
        assert_eq!(mbc.selected_rom_bank, 1);
        for i in 0..0x4000 { // Reading from 0x4000-0x7FFF
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 1 read mismatch at offset {}", i);
        }

        // Writing 0 should select bank 1
        mbc.write_rom(0x2100, 0);
        assert_eq!(mbc.selected_rom_bank, 1);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 1 (after writing 0) read mismatch at offset {}", i);
        }

        // Switch to bank 5 (0b0101)
        mbc.write_rom(0x2350, 5); // Addr bit 8 is 1
        assert_eq!(mbc.selected_rom_bank, 5);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[5 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 5 read mismatch at offset {}", i);
        }

        // Switch to bank 15 (0b1111) (max for MBC2's 4-bit selection)
        mbc.write_rom(0x3F00, 15); // Addr bit 8 is 1
        assert_eq!(mbc.selected_rom_bank, 15);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[15 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 15 read mismatch at offset {}", i);
        }

        // Test aliasing: MBC2 has max 16 banks (e.g. 256KB ROM). selected_rom_bank is 1-15.
        // If ROM is smaller, e.g., 64KB (4 banks: 0,1,2,3), selecting bank 5 should map to 5 % 4 = 1.
        let rom_64kb = create_rom(64, 100); // 4 banks
        let mut mbc_small_rom = MBC2::new(rom_64kb.clone());
        // num_rom_banks should be 4.
        assert_eq!(mbc_small_rom.num_rom_banks, 4);

        mbc_small_rom.write_rom(0x2100, 5); // Select bank 5. Effective bank should be 5 % 4 = 1.
        assert_eq!(mbc_small_rom.selected_rom_bank, 5); // Internal register stores 5
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom_64kb[1 * (16 * 1024) + i]; // Bank 1 data
            assert_eq!(mbc_small_rom.read_rom(rom_addr as u16), expected_val, "Bank 5 (aliased to 1 on 64KB ROM) read mismatch at offset {}", i);
        }

        // Selecting bank 4 on 4-bank ROM (4%4=0).
        mbc_small_rom.write_rom(0x2100, 4);
        assert_eq!(mbc_small_rom.selected_rom_bank, 4);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom_64kb[0 * (16 * 1024) + i]; // Bank 0 data
            assert_eq!(mbc_small_rom.read_rom(rom_addr as u16), expected_val, "Bank 4 (aliased to 0 on 64KB ROM) read mismatch at offset {}", i);
        }

        // Test that writes to addresses where bit 8 is 0 do NOT affect ROM bank
        let initial_bank = mbc.selected_rom_bank;
        mbc.write_rom(0x2000, 10); // Addr bit 8 is 0 -> RAM enable/disable
        assert_eq!(mbc.selected_rom_bank, initial_bank, "ROM bank changed by write to RAM enable area");
    }

    #[test]
    fn test_mbc2_ram_storage() {
        let rom = create_rom(32, 0);
        let mut mbc = MBC2::new(rom);

        // Enable RAM
        mbc.write_rom(0x0000, 0x0A);

        // Test RAM limits (512 nibbles, addr 0x000 to 0x1FF)
        // Write to first RAM location
        mbc.write_ram(0x000, 0x1A); // Store 0xA
        assert_eq!(mbc.read_ram(0x000), 0xFA, "RAM 0x000 read/write failed");

        // Write to last RAM location
        mbc.write_ram(0x1FF, 0xB4); // Store 0x4
        assert_eq!(mbc.read_ram(0x1FF), 0xF4, "RAM 0x1FF read/write failed");

        // Verify only lower 4 bits are stored
        mbc.write_ram(0x0A0, 0x79); // Store 0x9
        assert_eq!(mbc.ram_data[0x0A0], 0x09, "RAM internal storage error, expected 0x09");
        assert_eq!(mbc.read_ram(0x0A0), 0xF9, "RAM 0x0A0 read error, expected 0xF9");

        // Reading out of RAM bounds (0x200 and above) should return 0xFF
        assert_eq!(mbc.read_ram(0x200), 0xFF, "Out of bounds MBC2 RAM read (0x200) did not return 0xFF");

        // Writing out of RAM bounds should not panic and ideally not corrupt
        mbc.write_ram(0x200, 0xAB);
        assert_eq!(mbc.read_ram(0x200), 0xFF, "Read after out of bounds MBC2 RAM write (0x200) was not 0xFF");
        // Check if last valid RAM loc was corrupted
        assert_eq!(mbc.read_ram(0x1FF), 0xF4, "Last valid RAM loc corrupted by out of bounds write");
    }

    // Tests for MBC3
    #[test]
    fn test_mbc3_ram_rtc_enable_disable() {
        let rom = create_rom(512, 0); // 512KB ROM
        let mut mbc = MBC3::new(rom, 32 * 1024); // 32KB RAM (4 banks)

        // RAM & RTC should be disabled initially
        assert!(!mbc.ram_and_rtc_enabled, "RAM/RTC is not initially disabled");
        mbc.write_ram(0x0000, 0xFF); // Attempt write to RAM bank 0
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from disabled RAM should return 0xFF");

        // Select RTC register for seconds (0x08)
        mbc.write_rom(0x4000, 0x08);
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from disabled RTC register should return 0xFF");

        // Enable RAM & RTC
        mbc.write_rom(0x0000, 0x0A);
        assert!(mbc.ram_and_rtc_enabled, "RAM/RTC is not enabled after writing 0x0A");

        // Write and read from enabled RAM (bank 0)
        mbc.write_rom(0x4000, 0x00); // Select RAM bank 0
        mbc.write_ram(0x0000, 0x55);
        assert_eq!(mbc.read_ram(0x0000), 0x55, "RAM read/write failed for bank 0 when enabled");

        // Read from RTC register (should not be 0xFF now, but might be 0 if unlatched/default)
        // Latch RTC first
        mbc.write_rom(0x6000, 0x00);
        mbc.write_rom(0x6000, 0x01);
        mbc.write_rom(0x4000, 0x08); // Select RTC seconds
        // Default RTC values are 0.
        assert_eq!(mbc.read_ram(0x0000), 0x00, "RTC read for seconds after enable did not return default 0");


        // Disable RAM & RTC
        mbc.write_rom(0x0000, 0x00);
        assert!(!mbc.ram_and_rtc_enabled, "RAM/RTC is not disabled after writing 0x00");
        mbc.write_rom(0x4000, 0x00); // Select RAM bank 0
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from re-disabled RAM should return 0xFF");
        mbc.write_rom(0x4000, 0x08); // Select RTC seconds
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from re-disabled RTC register should return 0xFF");
    }

    #[test]
    fn test_mbc3_rom_bank_switching() {
        let rom_size_kb = 2048; // 2MB ROM -> 128 banks (0-127)
        let rom = create_rom(rom_size_kb, 0);
        let mut mbc = MBC3::new(rom.clone(), 0); // No RAM for this test

        // Bank 0 (0x0000-0x3FFF) is fixed
        for i in 0..0x4000 {
            assert_eq!(mbc.read_rom(i as u16), rom[i], "Initial bank 0 read mismatch at {}", i);
        }

        // Switch to bank 1 (writing 1 to 0x2000-0x3FFF)
        mbc.write_rom(0x2000, 1);
        assert_eq!(mbc.selected_rom_bank, 1);
        for i in 0..0x4000 { // Reading from 0x4000-0x7FFF
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 1 read mismatch at offset {}", i);
        }

        // Writing 0 to select ROM bank should map to bank 1
        mbc.write_rom(0x2000, 0);
        assert_eq!(mbc.selected_rom_bank, 1);
         for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i]; // Still bank 1
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 1 (after writing 0) read mismatch at offset {}", i);
        }

        // Switch to bank 70 (0x46)
        mbc.write_rom(0x3000, 0x46);
        assert_eq!(mbc.selected_rom_bank, 0x46);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[0x46 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 0x46 read mismatch at offset {}", i);
        }

        // Switch to bank 127 (0x7F, max value for 7-bit register)
        mbc.write_rom(0x2ABC, 0x7F);
        assert_eq!(mbc.selected_rom_bank, 0x7F);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[0x7F * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 0x7F read mismatch at offset {}", i);
        }

        // Test aliasing: ROM has 128 banks (0-127). `num_rom_banks` is 128.
        // Writing value > 127 (e.g. 0x80, which is 128) to the 7-bit register will be masked to 0.
        // If 0 is written, it maps to bank 1.
        // So, writing 0x80 (128) -> masked to 0 -> maps to bank 1.
        mbc.write_rom(0x2000, 0x80); // value & 0x7F = 0. Then 0 maps to 1.
        assert_eq!(mbc.selected_rom_bank, 1);
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[1 * (16 * 1024) + i]; // Bank 1
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 0x80 (aliased to 1) read mismatch at offset {}", i);
        }

        // If ROM is smaller, e.g. 512KB (32 banks, 0-31)
        let rom_512kb = create_rom(512, 50);
        let mut mbc_small_rom = MBC3::new(rom_512kb.clone(), 0);
        assert_eq!(mbc_small_rom.num_rom_banks, 32);

        // Select bank 33 (0x21). Masked by 0x7F is still 0x21.
        // Effective bank = 0x21 % num_rom_banks (32) = 1.
        mbc_small_rom.write_rom(0x2000, 0x21);
        assert_eq!(mbc_small_rom.selected_rom_bank, 0x21); // Internal register stores 0x21
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom_512kb[1 * (16 * 1024) + i]; // Bank 1 data
            assert_eq!(mbc_small_rom.read_rom(rom_addr as u16), expected_val, "Bank 0x21 (aliased to 1 on 512KB ROM) read mismatch at offset {}", i);
        }
    }

    #[test]
    fn test_mbc3_ram_banking() {
        let rom = create_rom(256, 0);
        // 32KB RAM -> 4 banks (0-3)
        let ram_size_bytes = 32 * 1024;
        let mut mbc = MBC3::new(rom, ram_size_bytes);
        assert_eq!(mbc.num_ram_banks, 4);

        mbc.write_rom(0x0000, 0x0A); // Enable RAM/RTC

        // Select RAM bank 0
        mbc.write_rom(0x4000, 0x00);
        assert_eq!(mbc.selected_ram_bank_or_rtc_reg, 0x00);
        mbc.write_ram(0x0001, 0xA1);
        assert_eq!(mbc.read_ram(0x0001), 0xA1);

        // Select RAM bank 1
        mbc.write_rom(0x4000, 0x01);
        assert_eq!(mbc.selected_ram_bank_or_rtc_reg, 0x01);
        mbc.write_ram(0x0002, 0xB2);
        assert_eq!(mbc.read_ram(0x0002), 0xB2);
        // Check bank 0 data is still there
        mbc.write_rom(0x4000, 0x00); // Switch back to RAM bank 0
        assert_eq!(mbc.read_ram(0x0001), 0xA1);


        // Select RAM bank 3 (max for 32KB RAM)
        mbc.write_rom(0x5000, 0x03);
        assert_eq!(mbc.selected_ram_bank_or_rtc_reg, 0x03);
        mbc.write_ram(0x1FFF, 0xC3); // Last byte of RAM bank 3
        assert_eq!(mbc.read_ram(0x1FFF), 0xC3);

        // Selections 0x04-0x07 for RAM bank are invalid, should read as 0xFF
        mbc.write_rom(0x4000, 0x04);
        assert_eq!(mbc.selected_ram_bank_or_rtc_reg, 0x04);
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from invalid RAM bank selection 0x04 should be 0xFF");
        mbc.write_ram(0x0000, 0xDD); // Write should be ignored
        assert_eq!(mbc.read_ram(0x0000), 0xFF);


        // Test with smaller RAM: 8KB (1 bank)
        let mut mbc_8kb_ram = MBC3::new(create_rom(64,0), 8 * 1024);
        assert_eq!(mbc_8kb_ram.num_ram_banks, 1);
        mbc_8kb_ram.write_rom(0x0000, 0x0A); // Enable RAM

        mbc_8kb_ram.write_rom(0x4000, 0x00); // RAM bank 0
        mbc_8kb_ram.write_ram(0x0000, 0xAA);
        assert_eq!(mbc_8kb_ram.read_ram(0x0000), 0xAA);

        // Try selecting RAM bank 1 (0x01) on 1-bank setup. Should be out of bounds.
        mbc_8kb_ram.write_rom(0x4000, 0x01);
        assert_eq!(mbc_8kb_ram.selected_ram_bank_or_rtc_reg, 0x01);
        assert_eq!(mbc_8kb_ram.read_ram(0x0000), 0xFF, "Read from RAM bank 1 (non-existent) should be 0xFF");
        mbc_8kb_ram.write_ram(0x0000, 0xBB); // Write should be ignored
        assert_eq!(mbc_8kb_ram.read_ram(0x0000), 0xFF);
    }

    #[test]
    fn test_mbc3_rtc_latching_and_read() {
        let rom = create_rom(64, 0);
        let mut mbc = MBC3::new(rom, 8 * 1024);
        mbc.write_rom(0x0000, 0x0A); // Enable RAM/RTC

        // Set some initial RTC values directly (for testing)
        mbc.rtc_registers.seconds = 10;
        mbc.rtc_registers.minutes = 20;
        mbc.rtc_registers.hours = 5;
        mbc.rtc_registers.day_counter_low = 100;
        mbc.rtc_registers.day_counter_high = 0x01; // Day MSB=1, Halt=0, Carry=0

        // Attempt to read RTC registers before latching - should get default (or uninitialized) latched values
        // Default latched values are all 0.
        mbc.write_rom(0x4000, 0x08); // Select RTC Seconds
        assert_eq!(mbc.read_ram(0x0000), 0, "RTC seconds before latch should be default 0");

        // Latch sequence: write 0x00 then 0x01 to 0x6000-0x7FFF
        mbc.write_rom(0x6000, 0x00);
        mbc.write_rom(0x7000, 0x01); // Address doesn't matter in 0x6000-0x7FFF range for the value itself

        // Verify latched values
        mbc.write_rom(0x4000, 0x08); // Select RTC Seconds
        assert_eq!(mbc.read_ram(0x0000), 10, "Latched RTC seconds mismatch");
        mbc.write_rom(0x4000, 0x09); // Select RTC Minutes
        assert_eq!(mbc.read_ram(0x0000), 20, "Latched RTC minutes mismatch");
        mbc.write_rom(0x4000, 0x0A); // Select RTC Hours
        assert_eq!(mbc.read_ram(0x0000), 5, "Latched RTC hours mismatch");
        mbc.write_rom(0x4000, 0x0B); // Select RTC Day Low
        assert_eq!(mbc.read_ram(0x0000), 100, "Latched RTC Day Low mismatch");
        mbc.write_rom(0x4000, 0x0C); // Select RTC Day High
        assert_eq!(mbc.read_ram(0x0000), 0x01, "Latched RTC Day High mismatch");

        // Change RTC registers again
        mbc.rtc_registers.seconds = 55;
        // Read again without re-latching - should still get old latched values
        mbc.write_rom(0x4000, 0x08); // Select RTC Seconds
        assert_eq!(mbc.read_ram(0x0000), 10, "RTC seconds changed without re-latch");

        // Re-latch
        mbc.write_rom(0x6000, 0x00);
        mbc.write_rom(0x6000, 0x01);
        assert_eq!(mbc.read_ram(0x0000), 55, "RTC seconds not updated after re-latch");

        // Test incorrect latch sequence (e.g., 0x01 then 0x00) - should not latch
        mbc.rtc_registers.minutes = 33;
        mbc.write_rom(0x6000, 0x01); // Wrong start to sequence
        mbc.write_rom(0x6000, 0x00);
        mbc.write_rom(0x4000, 0x09); // Select RTC Minutes
        assert_eq!(mbc.read_ram(0x0000), 20, "RTC minutes changed with incorrect latch sequence");
    }

    #[test]
    fn test_mbc3_rtc_write() {
        let rom = create_rom(64, 0);
        let mut mbc = MBC3::new(rom, 8 * 1024);
        mbc.write_rom(0x0000, 0x0A); // Enable RAM/RTC

        // Write to RTC seconds
        mbc.write_rom(0x4000, 0x08); // Select RTC Seconds
        mbc.write_ram(0x1000, 30);   // Write 30 seconds
        assert_eq!(mbc.rtc_registers.seconds, 30);
        // Value written > 59 should be wrapped
        mbc.write_ram(0x1000, 70); // 70 % 60 = 10
        assert_eq!(mbc.rtc_registers.seconds, 10);


        // Write to RTC minutes
        mbc.write_rom(0x4000, 0x09); // Select RTC Minutes
        mbc.write_ram(0x1000, 45);
        assert_eq!(mbc.rtc_registers.minutes, 45);
        mbc.write_ram(0x1000, 65); // 65 % 60 = 5
        assert_eq!(mbc.rtc_registers.minutes, 5);

        // Write to RTC hours
        mbc.write_rom(0x4000, 0x0A); // Select RTC Hours
        mbc.write_ram(0x1000, 12);
        assert_eq!(mbc.rtc_registers.hours, 12);
        mbc.write_ram(0x1000, 25); // 25 % 24 = 1
        assert_eq!(mbc.rtc_registers.hours, 1);

        // Write to RTC Day Low
        mbc.write_rom(0x4000, 0x0B); // Select RTC Day Low
        mbc.write_ram(0x1000, 200);
        assert_eq!(mbc.rtc_registers.day_counter_low, 200);

        // Write to RTC Day High / Control
        mbc.write_rom(0x4000, 0x0C); // Select RTC Day High
        // Initial Day High: 0 (MSB=0, Halt=0, Carry=0)
        // Write to set Day MSB (bit 0) and Halt (bit 6)
        // Value 0x41 -> Day MSB=1, Halt=1
        mbc.write_ram(0x1000, 0x41);
        assert_eq!(mbc.rtc_registers.day_counter_high & 0x01, 0x01, "Day MSB not set"); // Day MSB
        assert_eq!(mbc.rtc_registers.day_counter_high & 0x40, 0x40, "Halt bit not set");   // Halt bit

        // Clear Day MSB, keep Halt
        // Value 0x40 -> Day MSB=0, Halt=1
        mbc.write_ram(0x1000, 0x40);
        assert_eq!(mbc.rtc_registers.day_counter_high & 0x01, 0x00, "Day MSB not cleared");
        assert_eq!(mbc.rtc_registers.day_counter_high & 0x40, 0x40, "Halt bit incorrect (should be set)");

        // Carry bit (bit 7) is read-only, should not be affected by writes
        mbc.rtc_registers.day_counter_high = 0x80; // Manually set carry for testing
        mbc.write_ram(0x1000, 0x01); // Attempt to write 0x01 (Day MSB=1, Halt=0, Carry=0)
                                     // Internal day_counter_high should become (0x80 & !0x41) | (0x01 & 0x41)
                                     // = (0x80) | (0x01) = 0x81
        assert_eq!(mbc.rtc_registers.day_counter_high & 0x80, 0x80, "Carry bit affected by write");
        assert_eq!(mbc.rtc_registers.day_counter_high & 0x01, 0x01, "Day MSB not set after carry test");
    }

    // Tests for MBC5
    #[test]
    fn test_mbc5_ram_enable_disable() {
        let rom = create_rom(1024, 0); // 1MB ROM
        // Cartridge type 0x1B for MBC5+RAM+BATTERY
        let mut mbc = MBC5::new(rom, 32 * 1024, 0x1B);

        // RAM should be disabled initially
        assert!(!mbc.ram_enabled, "RAM is not initially disabled");
        mbc.write_ram(0x0000, 0xFF);
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from disabled RAM should return 0xFF");

        // Enable RAM
        mbc.write_rom(0x0000, 0x0A); // Any address in 0x0000-0x1FFF
        assert!(mbc.ram_enabled, "RAM is not enabled after writing 0x0A");

        // Write and read from enabled RAM
        mbc.write_ram(0x0000, 0x55);
        assert_eq!(mbc.read_ram(0x0000), 0x55, "RAM read/write failed when enabled");

        // Disable RAM
        mbc.write_rom(0x1000, 0x00); // Any address in 0x0000-0x1FFF
        assert!(!mbc.ram_enabled, "RAM is not disabled after writing 0x00");
        assert_eq!(mbc.read_ram(0x0000), 0xFF, "Read from re-disabled RAM should return 0xFF");
    }

    #[test]
    fn test_mbc5_rom_bank_switching() {
        let rom_size_kb = 8 * 1024; // 8MB ROM -> 512 banks (0-511)
        let rom = create_rom(rom_size_kb, 0);
        // Cartridge type 0x19 for MBC5 plain
        let mut mbc = MBC5::new(rom.clone(), 0, 0x19);

        // Bank 0 (0x0000-0x3FFF) is fixed
        for i in 0..0x4000 {
            assert_eq!(mbc.read_rom(i as u16), rom[i], "Initial bank 0 read mismatch at {}", i);
        }

        // Switch to bank 1 (low=1, high=0)
        mbc.write_rom(0x2000, 1); // Lower 8 bits of ROM bank
        mbc.write_rom(0x3000, 0); // Higher 1 bit of ROM bank
        assert_eq!(mbc.selected_rom_bank_low, 1);
        assert_eq!(mbc.selected_rom_bank_high, 0);
        let expected_bank_1 = 1;
        for i in 0..0x4000 { // Reading from 0x4000-0x7FFF
            let rom_addr = 0x4000 + i;
            let expected_val = rom[expected_bank_1 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 1 read mismatch at offset {}", i);
        }

        // Switch to bank 255 (low=255, high=0)
        mbc.write_rom(0x2ABC, 255);
        mbc.write_rom(0x3DEF, 0);
        assert_eq!(mbc.selected_rom_bank_low, 255);
        assert_eq!(mbc.selected_rom_bank_high, 0);
        let expected_bank_255 = 255;
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[expected_bank_255 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 255 read mismatch at offset {}", i);
        }

        // Switch to bank 256 (low=0, high=1)
        mbc.write_rom(0x2000, 0);
        mbc.write_rom(0x3000, 1);
        assert_eq!(mbc.selected_rom_bank_low, 0);
        assert_eq!(mbc.selected_rom_bank_high, 1);
        let expected_bank_256 = 256; // (1 << 8) | 0
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[expected_bank_256 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 256 read mismatch at offset {}", i);
        }

        // Switch to bank 511 (low=255, high=1) (max for 9-bit)
        mbc.write_rom(0x2123, 255);
        mbc.write_rom(0x3123, 1);
        assert_eq!(mbc.selected_rom_bank_low, 255);
        assert_eq!(mbc.selected_rom_bank_high, 1);
        let expected_bank_511 = 511; // (1 << 8) | 255
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom[expected_bank_511 * (16 * 1024) + i];
            assert_eq!(mbc.read_rom(rom_addr as u16), expected_val, "Bank 511 read mismatch at offset {}", i);
        }

        // Test aliasing with smaller ROM: 1MB ROM (64 banks, 0-63)
        let rom_1mb = create_rom(1024, 50);
        let mut mbc_small_rom = MBC5::new(rom_1mb.clone(), 0, 0x19);
        assert_eq!(mbc_small_rom.num_rom_banks, 64);

        // Select bank 65 (low=1, high=0, but full_rom_bank = 1). This is not how it works.
        // full_rom_bank = (high << 8) | low
        // Select bank 65: low=65 (0x41), high=0.  full_rom_bank = 65.
        // Effective bank = 65 % num_rom_banks (64) = 1.
        mbc_small_rom.write_rom(0x2000, 65);
        mbc_small_rom.write_rom(0x3000, 0); // high bit is 0
        assert_eq!(mbc_small_rom.selected_rom_bank_low, 65);
        assert_eq!(mbc_small_rom.selected_rom_bank_high, 0);
        let aliased_bank_1 = 1;
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom_1mb[aliased_bank_1 * (16 * 1024) + i];
            assert_eq!(mbc_small_rom.read_rom(rom_addr as u16), expected_val, "Bank 65 (aliased to 1 on 1MB ROM) read mismatch at offset {}", i);
        }

        // Select bank 256 (low=0, high=1) on 1MB ROM. full_rom_bank = 256.
        // Effective bank = 256 % 64 = 0.
        mbc_small_rom.write_rom(0x2000, 0);
        mbc_small_rom.write_rom(0x3000, 1);
        assert_eq!(mbc_small_rom.selected_rom_bank_low, 0);
        assert_eq!(mbc_small_rom.selected_rom_bank_high, 1);
        let aliased_bank_0 = 0;
        for i in 0..0x4000 {
            let rom_addr = 0x4000 + i;
            let expected_val = rom_1mb[aliased_bank_0 * (16 * 1024) + i];
            assert_eq!(mbc_small_rom.read_rom(rom_addr as u16), expected_val, "Bank 256 (aliased to 0 on 1MB ROM) read mismatch at offset {}", i);
        }
    }

    #[test]
    fn test_mbc5_ram_banking() {
        let rom = create_rom(256, 0);
        // 128KB RAM -> 16 banks (0-15)
        let ram_size_bytes = 128 * 1024;
        let mut mbc = MBC5::new(rom, ram_size_bytes, 0x1B); // MBC5+RAM+BATTERY
        assert_eq!(mbc.num_ram_banks, 16);

        mbc.write_rom(0x0000, 0x0A); // Enable RAM

        // Select RAM bank 0
        mbc.write_rom(0x4000, 0x00); // value & 0x0F = 0
        assert_eq!(mbc.selected_ram_bank, 0);
        mbc.write_ram(0x0001, 0xA1);
        assert_eq!(mbc.read_ram(0x0001), 0xA1);

        // Select RAM bank 7
        mbc.write_rom(0x4000, 0x07);
        assert_eq!(mbc.selected_ram_bank, 7);
        mbc.write_ram(0x1002, 0xB2);
        assert_eq!(mbc.read_ram(0x1002), 0xB2);
        // Check bank 0 data is still there
        mbc.write_rom(0x4000, 0x00); // Switch back to RAM bank 0
        assert_eq!(mbc.read_ram(0x0001), 0xA1);

        // Select RAM bank 15 (0x0F) (max for 128KB RAM)
        mbc.write_rom(0x5000, 0x0F);
        assert_eq!(mbc.selected_ram_bank, 15);
        mbc.write_ram(0x1FFF, 0xC3); // Last byte of RAM bank 15
        assert_eq!(mbc.read_ram(0x1FFF), 0xC3);

        // Writing value > 0x0F should be masked. E.g. 0x13 -> 0x03
        // This is incorrect, the spec says "value & 0x0F". So 0x13 would be bank 3.
        // The current code does `ram_bank % (self.num_ram_banks as u8)`
        // If num_ram_banks is 16, then 0x13 (19) & 0x0F = 3. 3 % 16 = 3. So bank 3.
        // If num_ram_banks is 8, then 0x0F (15) & 0x0F = 15. 15 % 8 = 7. So bank 7.
        // Let's test the actual behavior based on `value & 0x0F` then modulo `num_ram_banks`.
        mbc.write_rom(0x4000, 0x13); // value & 0x0F = 0x03. 0x03 % 16 = 3.
        assert_eq!(mbc.selected_ram_bank, 3);


        // Test with smaller RAM: 32KB (4 banks, 0-3)
        let mut mbc_32kb_ram = MBC5::new(create_rom(64,0), 32 * 1024, 0x1B);
        assert_eq!(mbc_32kb_ram.num_ram_banks, 4);
        mbc_32kb_ram.write_rom(0x0000, 0x0A); // Enable RAM

        mbc_32kb_ram.write_rom(0x4000, 0x00); // RAM bank 0
        mbc_32kb_ram.write_ram(0x0000, 0xAA);
        assert_eq!(mbc_32kb_ram.read_ram(0x0000), 0xAA);

        // Select RAM bank 0x05. value & 0x0F = 0x05.
        // selected_ram_bank = 0x05 % num_ram_banks (4) = 1.
        mbc_32kb_ram.write_rom(0x4000, 0x05);
        assert_eq!(mbc_32kb_ram.selected_ram_bank, 1);
        mbc_32kb_ram.write_ram(0x0001, 0xBB);
        assert_eq!(mbc_32kb_ram.read_ram(0x0001), 0xBB);

        // Check data in bank 0 is still there
        mbc_32kb_ram.write_rom(0x4000, 0x00); // bank 0
        assert_eq!(mbc_32kb_ram.read_ram(0x0000), 0xAA);


        // Test with no RAM. selected_ram_bank should be 0, reads/writes ineffective.
        let mut mbc_no_ram = MBC5::new(create_rom(64,0), 0, 0x19); // No RAM
        assert_eq!(mbc_no_ram.num_ram_banks, 0);
        mbc_no_ram.write_rom(0x0000, 0x0A); // "Enable" RAM (flag is set)
        assert!(mbc_no_ram.ram_enabled);

        mbc_no_ram.write_rom(0x4000, 0x05); // Try to select bank 5
        assert_eq!(mbc_no_ram.selected_ram_bank, 0, "Selected RAM bank should be 0 if no RAM");
        assert_eq!(mbc_no_ram.read_ram(0x0000), 0xFF, "Read from non-existent RAM should be 0xFF");
        mbc_no_ram.write_ram(0x0000, 0xCC); // Should do nothing
        assert_eq!(mbc_no_ram.read_ram(0x0000), 0xFF);
    }

    #[test]
    fn test_mbc5_rumble_flag() {
        let rom = create_rom(64, 0);
        // Cartridge types indicating rumble: 0x1C, 0x1D, 0x1E
        let mbc_rumble1 = MBC5::new(rom.clone(), 8*1024, 0x1C); // MBC5+RUMBLE+RAM+BATTERY
        assert!(mbc_rumble1.has_rumble, "has_rumble should be true for type 0x1C");

        let mbc_rumble2 = MBC5::new(rom.clone(), 8*1024, 0x1D); // MBC5+RUMBLE+RAM
        assert!(mbc_rumble2.has_rumble, "has_rumble should be true for type 0x1D");

        let mbc_rumble3 = MBC5::new(rom.clone(), 8*1024, 0x1E); // MBC5+RUMBLE
        assert!(mbc_rumble3.has_rumble, "has_rumble should be true for type 0x1E");

        // Cartridge types not indicating rumble for MBC5
        let mbc_no_rumble1 = MBC5::new(rom.clone(), 8*1024, 0x19); // MBC5
        assert!(!mbc_no_rumble1.has_rumble, "has_rumble should be false for type 0x19");

        let mbc_no_rumble2 = MBC5::new(rom.clone(), 8*1024, 0x1A); // MBC5+RAM
        assert!(!mbc_no_rumble2.has_rumble, "has_rumble should be false for type 0x1A");

        let mbc_no_rumble3 = MBC5::new(rom.clone(), 8*1024, 0x1B); // MBC5+RAM+BATTERY
        assert!(!mbc_no_rumble3.has_rumble, "has_rumble should be false for type 0x1B");
    }

    // Tests for Stubbed MBCs

    #[test]
    fn test_mbc6_stub_behavior() { // MBC6 uses MBC1 internally
        let rom_data = create_rom(128, 10); // 128KB ROM
        let ram_size_bytes = 8 * 1024;    // 8KB RAM
        // MBC6 cartridge type byte example 0x20
        let mut mbc6 = MBC6::new(rom_data.clone(), ram_size_bytes, 0x20);

        // Compare with a manually created MBC1
        let mut mbc1_equivalent = MBC1::new(rom_data.clone(), ram_size_bytes);

        // Test initial ROM read (bank 0)
        assert_eq!(mbc6.read_rom(0x1000), mbc1_equivalent.read_rom(0x1000), "MBC6 ROM read (bank 0) mismatch with MBC1");

        // Enable RAM (for both)
        mbc6.write_rom(0x0000, 0x0A);
        mbc1_equivalent.write_rom(0x0000, 0x0A);
        assert!(mbc6.internal_mbc1.ram_enabled, "MBC6 internal MBC1 RAM not enabled");

        // Test RAM write/read
        mbc6.write_ram(0x0100, 0xAB);
        mbc1_equivalent.write_ram(0x0100, 0xAB);
        assert_eq!(mbc6.read_ram(0x0100), mbc1_equivalent.read_ram(0x0100), "MBC6 RAM r/w mismatch with MBC1");
        assert_eq!(mbc6.read_ram(0x0100), 0xAB, "MBC6 RAM value incorrect");


        // Test ROM bank switching (low bits)
        mbc6.write_rom(0x2000, 5); // Switch to bank 5
        mbc1_equivalent.write_rom(0x2000, 5);
        assert_eq!(mbc6.internal_mbc1.rom_bank_low_bits, 5, "MBC6 internal MBC1 ROM bank low bits not set");

        assert_eq!(mbc6.read_rom(0x4010), mbc1_equivalent.read_rom(0x4010), "MBC6 ROM read (bank 5) mismatch with MBC1");
        let expected_val_bank5 = rom_data[5 * (16*1024) + 0x0010];
        assert_eq!(mbc6.read_rom(0x4010), expected_val_bank5, "MBC6 ROM value incorrect for bank 5");
    }

    #[test]
    fn test_mbc7_stub_behavior() { // MBC7 uses NoMBC internally
        let rom_data = create_rom(64, 20);  // 64KB ROM
        let ram_size_bytes = 0; // MBC7 typically doesn't use external RAM via A000-BFFF like this
                                  // The stub passes this to NoMBC. Let's test with 0.
        // MBC7 cartridge type byte example 0x22
        let mut mbc7 = MBC7::new(rom_data.clone(), ram_size_bytes, 0x22);
        let mut nombc_equivalent = NoMBC::new(rom_data.clone(), ram_size_bytes);

        // Test ROM read
        assert_eq!(mbc7.read_rom(0x1234), nombc_equivalent.read_rom(0x1234), "MBC7 ROM read mismatch with NoMBC");
        assert_eq!(mbc7.read_rom(0x1234), rom_data[0x1234], "MBC7 ROM value incorrect");

        // Test RAM read (should be 0xFF as no RAM for NoMBC)
        assert_eq!(mbc7.read_ram(0x0000), nombc_equivalent.read_ram(0x0000), "MBC7 RAM read mismatch with NoMBC");
        assert_eq!(mbc7.read_ram(0x0000), 0xFF, "MBC7 RAM read expected 0xFF");

        // Test RAM write (should be ignored by NoMBC with 0 RAM size)
        mbc7.write_ram(0x0000, 0xAA);
        nombc_equivalent.write_ram(0x0000, 0xAA); // NoMBC would ignore this
        assert_eq!(mbc7.read_ram(0x0000), 0xFF, "MBC7 RAM read after write expected 0xFF");

        // Test write to ROM area (ignored by NoMBC)
        let initial_rom_val = mbc7.read_rom(0x1000);
        mbc7.write_rom(0x1000, 0xCC);
        assert_eq!(mbc7.read_rom(0x1000), initial_rom_val, "MBC7 ROM content changed by write_rom, NoMBC should ignore");
    }

    #[test]
    fn test_mbc30_stub_behavior() { // MBC30 uses MBC3 internally
        let rom_data = create_rom(1024, 30); // 1MB ROM
        let ram_size_bytes = 32 * 1024;     // 32KB RAM
        // MBC30 hypothetical type byte 0x14 (or an MBC3 type byte)
        let mut mbc30 = MBC30::new(rom_data.clone(), ram_size_bytes, 0x14);
        let mut mbc3_equivalent = MBC3::new(rom_data.clone(), ram_size_bytes);

        // Test initial ROM read (bank 0)
        assert_eq!(mbc30.read_rom(0x0500), mbc3_equivalent.read_rom(0x0500), "MBC30 ROM read (bank 0) mismatch with MBC3");

        // Enable RAM/RTC
        mbc30.write_rom(0x0000, 0x0A);
        mbc3_equivalent.write_rom(0x0000, 0x0A);
        assert!(mbc30.internal_mbc3.ram_and_rtc_enabled, "MBC30 internal MBC3 RAM/RTC not enabled");

        // Select RAM bank 1 and test RAM write/read
        mbc30.write_rom(0x4000, 0x01); // Select RAM bank 1
        mbc3_equivalent.write_rom(0x4000, 0x01);

        mbc30.write_ram(0x0200, 0xBB);
        mbc3_equivalent.write_ram(0x0200, 0xBB);
        assert_eq!(mbc30.read_ram(0x0200), mbc3_equivalent.read_ram(0x0200), "MBC30 RAM r/w mismatch with MBC3");
        assert_eq!(mbc30.read_ram(0x0200), 0xBB, "MBC30 RAM value incorrect");

        // Test ROM bank switching
        mbc30.write_rom(0x2000, 10); // Switch to bank 10
        mbc3_equivalent.write_rom(0x2000, 10);
        assert_eq!(mbc30.internal_mbc3.selected_rom_bank, 10, "MBC30 internal MBC3 ROM bank not set");

        assert_eq!(mbc30.read_rom(0x4020), mbc3_equivalent.read_rom(0x4020), "MBC30 ROM read (bank 10) mismatch with MBC3");
        let expected_val_bank10 = rom_data[10 * (16*1024) + 0x0020];
        assert_eq!(mbc30.read_rom(0x4020), expected_val_bank10, "MBC30 ROM value incorrect for bank 10");

        // Test RTC latch (basic check, RTC tests are more thorough in MBC3 tests)
        mbc30.internal_mbc3.rtc_registers.seconds = 5; // Set directly for test
        mbc3_equivalent.rtc_registers.seconds = 5;

        mbc30.write_rom(0x6000, 0x00);
        mbc30.write_rom(0x6000, 0x01);
        mbc3_equivalent.write_rom(0x6000, 0x00);
        mbc3_equivalent.write_rom(0x6000, 0x01);

        mbc30.write_rom(0x4000, 0x08); // Select RTC seconds
        mbc3_equivalent.write_rom(0x4000, 0x08);
        assert_eq!(mbc30.read_ram(0x0000), mbc3_equivalent.read_ram(0x0000), "MBC30 RTC read mismatch with MBC3");
        assert_eq!(mbc30.read_ram(0x0000), 5, "MBC30 RTC read incorrect value");
    }

    // Placeholder for future tests
    #[test]
    fn placeholder_test() {
        assert_eq!(2 + 2, 4);
    }
}
