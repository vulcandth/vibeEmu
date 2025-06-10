use crate::models::GameBoyModel;

// Constants for PPU
pub const VRAM_SIZE_CGB: usize = 16 * 1024; // 2 banks of 8KB
pub const VRAM_SIZE_DMG: usize = 8 * 1024;  // 1 bank of 8KB
pub const OAM_SIZE: usize = 160;           // Object Attribute Memory
pub const CGB_PALETTE_RAM_SIZE: usize = 64; // Background and Sprite palettes (32 palettes * 2 bytes/color * 4 colors/palette is not right, it's 64 bytes total for CRAM)

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

// PPU Cycle Constants
pub const OAM_SCAN_CYCLES: u32 = 80;
pub const DRAWING_CYCLES: u32 = 172; // Placeholder, will vary
pub const HBLANK_CYCLES: u32 = 204; // 456 - 80 - 172 = 204
pub const SCANLINE_CYCLES: u32 = 456; // OAM_SCAN + DRAWING + HBLANK
pub const VBLANK_LINES: u8 = 10; // LY 144-153
pub const VBLANK_DURATION_CYCLES: u32 = SCANLINE_CYCLES * VBLANK_LINES as u32; // 4560 cycles
pub const TOTAL_LINES: u8 = 154; // 144 visible + 10 VBlank

// PPU Modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PpuMode {
    HBlank = 0, // Mode 0
    VBlank = 1, // Mode 1
    OamScan = 2, // Mode 2
    Drawing = 3, // Mode 3
}

pub struct Ppu {
    // Memory
    pub vram: Vec<u8>, // Video RAM (0x8000-0x9FFF) - Can be 1 or 2 banks for CGB
    pub oam: [u8; OAM_SIZE],  // Object Attribute Memory (0xFE00-0xFE9F)

    // CGB Specific Memory
    pub cgb_background_palette_ram: [u8; CGB_PALETTE_RAM_SIZE], // FF69 - BCPD/BGPD
    pub cgb_sprite_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],     // FF6B - OCPD/OBPD

    // Registers - values are typically stored here or directly in Bus's io_registers map
    // For now, let's assume the Bus will forward relevant register values to PPU methods,
    // but some core ones might be mirrored or have internal PPU state.
    pub lcdc: u8, // LCD Control (0xFF40)
    pub stat: u8, // LCD Status (0xFF41)
    pub scy: u8,  // Scroll Y (0xFF42)
    pub scx: u8,  // Scroll X (0xFF43)
    pub ly: u8,   // LCD Y Coordinate (0xFF44) - Current scanline
    pub lyc: u8,  // LY Compare (0xFF45)
    pub wy: u8,   // Window Y Position (0xFF4A)
    pub wx: u8,   // Window X Position minus 7 (0xFF4B)

    pub bgp: u8,  // BG Palette Data (0xFF47) - DMG
    pub obp0: u8, // Object Palette 0 Data (0xFF48) - DMG
    pub obp1: u8, // Object Palette 1 Data (0xFF49) - DMG

    // CGB Specific Registers (values might be passed from Bus or stored if PPU directly handles them)
    pub vbk: u8,      // VRAM Bank Select (0xFF4F) - CGB only (bit 0)
    pub bcps_bcpi: u8, // Background Palette Index (0xFF68) - CGB only
    pub bcpd_bgpd: u8, // Background Palette Data (0xFF69) - CGB only (written through bcps_bcpi auto-increment)
    pub ocps_ocpi: u8, // Sprite Palette Index (0xFF6A) - CGB only
    pub ocpd_obpd: u8, // Sprite Palette Data (0xFF6B) - CGB only (written through ocps_ocpi auto-increment)

    // PPU State
    pub current_mode: PpuMode, // Made public for bus/debugger inspection
    pub cycles_in_mode: u32, // Cycles accumulated in the current mode for the current line

    // Interrupt flags (to be read by the interrupt controller via Bus)
    pub vblank_interrupt_requested: bool,
    pub stat_interrupt_requested: bool, // General flag for all STAT conditions

    // STAT interrupt condition flags (internal tracking, helps generate stat_interrupt_requested)
    // These are not strictly necessary if logic in tick directly sets stat_interrupt_requested
    // but can be useful for debugging or more complex interrupt logic.
    // For now, we'll try to manage stat_interrupt_requested directly.

    // HDMA HBlank signal
    pub just_entered_hblank: bool,

    // Framebuffer to store rendered pixels
    pub framebuffer: Vec<u8>, // Stores RGB888, 160*144*3 bytes

    // Internal state based on SameBoy for more detailed emulation later
    // For now, these are placeholders or simplified.

    // SameBoy's `GB_display_s` related:
    // gb->current_line; -> covered by ly (or an internal version if LY has specific read behavior)
    // gb->ly_for_comparison;
    // gb->wy_triggered;
    // gb->window_y;
    // gb->lcd_x; // internal x position for rendering

    // gb->fetcher_state;
    // gb->bg_fifo, gb->oam_fifo;
    // gb->n_visible_objs, gb->visible_objs[], gb->objects_x[], gb->objects_y[];

    model: GameBoyModel,
}

// Conceptual BG Rendering Info
// This is a temporary struct to hold results for demonstration
#[derive(Debug, Default, Clone, Copy)]
pub struct BgTileInfo {
    pub tile_number: u8,
    pub tile_data_addr_plane1: u16,
    pub tile_data_addr_plane2: u16,
    pub attributes_addr: u16, // VRAM address for attributes (CGB)
    pub attributes: u8,       // Attributes byte (CGB)
    pub tile_number_map_addr: u16,
}

impl Ppu {
    pub fn new(model: GameBoyModel) -> Self {
        let vram_size = if model.is_cgb_family() { VRAM_SIZE_CGB } else { VRAM_SIZE_DMG };
        Ppu {
            vram: vec![0; vram_size],
            oam: [0; OAM_SIZE],
            cgb_background_palette_ram: [0; CGB_PALETTE_RAM_SIZE],
            cgb_sprite_palette_ram: [0; CGB_PALETTE_RAM_SIZE],

            // Default register values at power-up (these are often set by BIOS if one runs)
            // Values here are typical post-boot values or safe defaults.
            // The bus will typically write the true initial values from I/O registers.
            lcdc: 0x91, // Display ON, BG ON, OBJ ON, Window OFF, BG/Win Tile Data #0, BG Tile Map #0, OBJ size 8x8
            stat: 0x80, // Bits 7,0-1 are readable. Bit 7 is always 1. Mode bits will be set by logic. LYC=LY initially false.
            scy: 0x00,
            scx: 0x00,
            ly: 0x00,
            lyc: 0x00,
            wy: 0x00,
            wx: 0x00,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,

            vbk: 0xFE,
            bcps_bcpi: 0x00,
            bcpd_bgpd: 0x00,
            ocps_ocpi: 0x00,
            ocpd_obpd: 0x00,

            current_mode: PpuMode::OamScan, // Initial mode after power-on/reset often OAM Scan for line 0
            cycles_in_mode: 0,

            vblank_interrupt_requested: false,
            stat_interrupt_requested: false,
            just_entered_hblank: false,

            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT * 3], // RGB888

            model,
        }
    }

    // Basic read/write methods for VRAM and OAM (will be called by Bus)
    // These will need to respect PPU modes and CGB banking later.

    pub fn read_vram(&self, addr: u16) -> u8 {
        let vram_bank_offset = if self.model.is_cgb_family() && (self.vbk & 0x01) == 1 {
            VRAM_SIZE_DMG // Bank 1 is the second 8KB chunk
        } else {
            0 // Bank 0
        };
        // Address is 0x8000-0x9FFF, so subtract 0x8000 for index
        let index = (addr as usize - 0x8000) + vram_bank_offset;
        if index < self.vram.len() {
            self.vram[index]
        } else {
            0xFF // Should not happen if bank logic is correct
        }
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        let vram_bank_offset = if self.model.is_cgb_family() && (self.vbk & 0x01) == 1 {
            VRAM_SIZE_DMG
        } else {
            0
        };
        let index = (addr as usize - 0x8000) + vram_bank_offset;
        if index < self.vram.len() {
            self.vram[index] = value;
        }
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        // Address is 0xFE00-0xFE9F, so subtract 0xFE00 for index
        self.oam[addr as usize - 0xFE00]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        self.oam[addr as usize - 0xFE00] = value;
    }

    // TODO: Implement CGB palette RAM read/write methods
    // read_cgb_palette_ram(is_sprite_palette: bool, index: u8, auto_increment: bool) -> u8
    // write_cgb_palette_ram(is_sprite_palette: bool, index: u8, value: u8, auto_increment: bool)

    pub fn tick(&mut self, cpu_t_cycles: u32) {
        self.just_entered_hblank = false; // Reset at the beginning of each tick

        // If LCD is off, PPU is mostly idle
        if (self.lcdc & 0x80) == 0 { // LCDC Bit 7: LCD Display Enable
            self.ly = 0;
            self.cycles_in_mode = 0;
            self.current_mode = PpuMode::HBlank; // Or OAMScan; Pandocs: LY is reset, mode becomes Mode 0.
            // STAT mode bits should reflect HBlank (00). LYC=LY flag might change.
            // Preserve interrupt enable bits (3-6) and LYC flag (2). Bit 7 always 1.
            self.stat = (self.stat & 0xFC) | (PpuMode::HBlank as u8); // Update mode bits
            if self.ly == self.lyc { // Update LYC=LY coincidence flag
                self.stat |= 1 << 2;
            } else {
                self.stat &= !(1 << 2);
            }
            self.stat |= 0x80; // Ensure bit 7 is set

            self.vblank_interrupt_requested = false;
            self.stat_interrupt_requested = false; // No STAT interrupts when LCD is off (except maybe LYC during power off?)
                                               // For simplicity, clear STAT interrupts.
            return;
        }

        self.cycles_in_mode += cpu_t_cycles;
        let previous_stat_interrupt_line = self.eval_stat_interrupt_conditions();

        match self.current_mode {
            PpuMode::OamScan => {
                if self.cycles_in_mode >= OAM_SCAN_CYCLES {
                    self.cycles_in_mode -= OAM_SCAN_CYCLES;
                    self.current_mode = PpuMode::Drawing;
                    self.stat = (self.stat & 0xF8) | (PpuMode::Drawing as u8);
                }
            }
            PpuMode::Drawing => {
                if self.cycles_in_mode >= DRAWING_CYCLES {
                    self.cycles_in_mode -= DRAWING_CYCLES;
                    self.current_mode = PpuMode::HBlank;
                    self.stat = (self.stat & 0xF8) | (PpuMode::HBlank as u8);
                    self.just_entered_hblank = true;
                    // TODO: HBlank HDMA Hook Placeholder / Render scanline

                    // Conceptual call for the first pixel of the scanline
                    if (self.lcdc & 0x01) != 0 { // If BG is enabled (Bit 0: BG and Window Display)
                        // We are not iterating all pixels yet, just showing conceptual logic for pixel (0, self.ly)
                        let _bg_info = self.get_bg_tile_info(0); // x_on_screen = 0
                        // println!(
                        //     "LY: {}, Mode: {:?}, CyclesInMode: {} - Fetch BG Tile for (0, {}): TileNum: {}, DataAddr1: {:#06X}, AttrAddr: {:#06X}, Attr: {:#04X}",
                        //     self.ly, self.current_mode, self.cycles_in_mode, self.ly,
                        //     _bg_info.tile_number, _bg_info.tile_data_addr_plane1,
                        //     _bg_info.attributes_addr, _bg_info.attributes
                        // );
                    }
                }
            }
            PpuMode::HBlank => {
                if self.cycles_in_mode >= HBLANK_CYCLES {
                    self.cycles_in_mode -= HBLANK_CYCLES; // Remainder cycles carry over
                    self.ly += 1;

                    if self.ly < SCREEN_HEIGHT as u8 { // 0-143
                        self.current_mode = PpuMode::OamScan;
                        self.stat = (self.stat & 0xF8) | (PpuMode::OamScan as u8);
                    } else { // ly == 144, transition to VBlank
                        self.current_mode = PpuMode::VBlank;
                        self.stat = (self.stat & 0xF8) | (PpuMode::VBlank as u8);
                        self.vblank_interrupt_requested = true;
                        // VBlank interrupt is requested once when entering VBlank mode
                    }
                }
            }
            PpuMode::VBlank => {
                if self.cycles_in_mode >= SCANLINE_CYCLES {
                    self.cycles_in_mode -= SCANLINE_CYCLES;
                    self.ly += 1;

                    if self.ly == TOTAL_LINES { // LY reaches 154 (144-153 are VBlank lines)
                        self.ly = 0; // Reset LY
                        self.current_mode = PpuMode::OamScan; // Start new frame
                        self.stat = (self.stat & 0xF8) | (PpuMode::OamScan as u8);
                    }
                    // LY remains 144 through 153 during VBlank.
                    // No mode change until LY wraps around.
                }
            }
        }

        // Update LYC=LY coincidence flag (STAT bit 2)
        if self.ly == self.lyc {
            self.stat |= 1 << 2;
        } else {
            self.stat &= !(1 << 2);
        }

        // Check and request STAT interrupts
        let current_stat_interrupt_line = self.eval_stat_interrupt_conditions();
        if !previous_stat_interrupt_line && current_stat_interrupt_line {
            self.stat_interrupt_requested = true;
        }
        // Note: The exact timing of stat_interrupt_requested going false needs to be handled
        // by the CPU/interrupt controller when the interrupt is acknowledged/serviced.
        // For now, we set it true if any enabled condition becomes active.
        // A more precise model might only set it true for one M-cycle or if the line rises.
    }

    // Helper function to evaluate STAT interrupt conditions
    // Returns true if any enabled STAT interrupt condition is met.
    fn eval_stat_interrupt_conditions(&self) -> bool {
        let mode_interrupt_enabled = match self.current_mode {
            PpuMode::HBlank => (self.stat & (1 << 3)) != 0, // Mode 0 HBlank Interrupt Enable
            PpuMode::VBlank => (self.stat & (1 << 4)) != 0, // Mode 1 VBlank Interrupt Enable (for STAT, not the main VBlank int)
            PpuMode::OamScan => (self.stat & (1 << 5)) != 0, // Mode 2 OAM Interrupt Enable
            PpuMode::Drawing => false, // Mode 3 has no specific STAT interrupt enable bit
        };

        let lyc_interrupt_enabled = (self.stat & (1 << 6)) != 0; // LYC=LY Coincidence Interrupt Enable
        let lyc_coincidence = (self.stat & (1 << 2)) != 0; // LYC=LY Flag is set

        // STAT interrupt is triggered if:
        // 1. LYC=LY coincidence is enabled AND LYC=LY flag is set
        // 2. Mode-specific interrupt is enabled AND the PPU is in that mode
        //    (Note: For VBlank mode, this is a STAT interrupt, distinct from the main VBlank interrupt.
        //     It can trigger on each line of VBlank if enabled.)
        if lyc_interrupt_enabled && lyc_coincidence {
            return true;
        }
        if mode_interrupt_enabled {
             // For Mode 1 (VBlank), the STAT interrupt can trigger on each line of VBlank if bit 4 is set.
             // For Mode 0 (HBlank) and Mode 2 (OAM), it triggers when entering the mode if enabled.
            return true;
        }
        false
    }

    // TODO: Interrupt request clear methods (called by Bus after servicing)
    pub fn clear_vblank_interrupt_request(&mut self) {
        self.vblank_interrupt_requested = false;
    }

    pub fn clear_stat_interrupt_request(&mut self) {
        self.stat_interrupt_requested = false;
    }

    pub fn write_stat(&mut self, value: u8) {
        // CPU can write to bits 3, 4, 5, 6 of STAT.
        // Bits 0, 1, 2 are read-only for CPU (set by PPU). Bit 7 is always 1.
        let old_stat_interrupt_line = self.eval_stat_interrupt_conditions();

        self.stat = (value & 0b01111000) | (self.stat & 0b10000111);
        // self.stat |= 0x80; // Ensure bit 7 is always 1 - already handled by PPU logic / initial value

        // Re-evaluate STAT interrupt conditions if relevant bits changed
        // An interrupt should be triggered if a condition becomes true AND its enable bit is set.
        // If an enable bit is cleared, and the condition was true, the interrupt line might go low.
        let new_stat_interrupt_line = self.eval_stat_interrupt_conditions();
        if !old_stat_interrupt_line && new_stat_interrupt_line {
            self.stat_interrupt_requested = true;
        }
        // Note: The specification for STAT interrupt generation upon writing to STAT is complex.
        // For example, if LYC=LY interrupt is enabled, and LYC is written to match LY,
        // an interrupt occurs. This logic is part of eval_stat_interrupt_conditions.
        // If writing to STAT disables an interrupt that was active, stat_interrupt_requested should ideally go false
        // if no other conditions are met. For now, this only sets it true on a rising edge.
        // A more robust solution might re-evaluate `self.stat_interrupt_requested = new_stat_interrupt_line;`
        // but this could lead to missed interrupts if not careful about edge vs. level triggering.
        // The current model in tick handles the rising edge for mode changes and LYC=LY.
        // Writing to STAT itself should also check this.
    }

    // Helper function for conceptual BG rendering logic
    // Takes x_on_screen (0-159) to determine which tile column to access.
    // y_on_screen is implicitly self.ly
    fn get_bg_tile_info(&self, x_on_screen: u8) -> BgTileInfo {
        let mut info = BgTileInfo::default();

        // 1. Read LCDC bits
        // bg_display_enable: (self.lcdc & 0x01) != 0; // Checked before calling this function
        let bg_tile_map_select = (self.lcdc >> 3) & 0x01; // Bit 3: BG Tile Map Display Select (0=9800-9BFF, 1=9C00-9FFF)
        let bg_tile_data_select = (self.lcdc >> 4) & 0x01; // Bit 4: BG & Window Tile Data Select (0=8800-97FF, 1=8000-8FFF)

        // 2. Calculate coordinates
        let bg_map_y = self.ly.wrapping_add(self.scy); // Y coordinate in the full 256x256 BG map
        let bg_map_x = x_on_screen.wrapping_add(self.scx); // X coordinate in the full 256x256 BG map

        let tile_row_in_map = (bg_map_y / 8) as u16; // Which row of tiles in the 32x32 tile map
        let tile_col_in_map = (bg_map_x / 8) as u16; // Which column of tiles in the 32x32 tile map

        // 3. Determine Tile Map Address (where the tile number is stored)
        let tile_map_base_addr: u16 = if bg_tile_map_select == 0 { 0x9800 } else { 0x9C00 };
        info.tile_number_map_addr = tile_map_base_addr + (tile_row_in_map * 32) + tile_col_in_map;

        // 4. Fetch Tile Number
        // read_vram already handles VRAM banking for CGB based on self.vbk (which should be 0 for BG map numbers)
        // For CGB attributes, a different bank is used.
        let _current_vbk = self.vbk; // Save current VBK if we need to tamper with it for attributes
        // Ensure VBK is 0 for reading tile numbers and BG tile data (unless attributes change it, which is later)
        // This is a bit of a simplification; actual CGB hardware might have dedicated paths or priorities.
        // For now, assume BG tile numbers are always in bank 0.
        // self.vbk = 0; // Temporarily set VBK to 0 if it were mutable here. PPU read_vram uses self.vbk.
        // Since read_vram is &self, we can't change self.vbk here.
        // This implies that for BG tile numbers, self.vbk should be 0.
        // And for BG tile data, it should also be 0 unless CGB attributes specify bank 1 for tile data.

        // If CGB mode, BG Map Attributes determine VRAM bank for tile data (and tile number itself is bank 0)
        // For now, we assume tile_number is from VRAM Bank 0.
        info.tile_number = self.read_vram_bank_agnostic(info.tile_number_map_addr, 0);


        // 5. CGB Tile Attributes
        if self.model.is_cgb_family() {
            // Attributes are in VRAM Bank 1, at the same offset as tile numbers in Bank 0.
            // The address info.tile_number_map_addr is already 0x9800-0x9FFF.
            // No need to add VRAM_SIZE_DMG if read_vram_bank_agnostic handles it.
            info.attributes_addr = info.tile_number_map_addr; // The address is the same, bank is different.
            info.attributes = self.read_vram_bank_agnostic(info.attributes_addr, 1);
        }

        // For CGB, the attributes byte contains:
        // Bit 0: BG-to-OAM Priority (0=Normal, 1=BG Priority)
        // Bit 1: Vertical Flip
        // Bit 2: Horizontal Flip
        // Bit 3: Tile VRAM Bank (0=Bank 0, 1=Bank 1) - for the tile data itself
        // Bits 4-6: BG Palette Number
        // Bit 7: Not used (or BG-to-OAM Priority for DMG compatibility, but CGB uses bit 0)

        let _tile_data_vram_bank = if self.model.is_cgb_family() {
            (info.attributes >> 3) & 0x01 // Use bank from attributes for CGB
        } else {
            0 // DMG always uses bank 0 for tile data
        };

        // 6. Determine Tile Data Address
        let tile_data_start_addr: u16;
        if bg_tile_data_select == 1 { // Method 0x8000 (unsigned tile numbers)
            tile_data_start_addr = 0x8000 + (info.tile_number as u16 * 16);
        } else { // Method 0x8800 (signed tile numbers, base is 0x9000 for positive, 0x8800 for negative)
            // tile_number is u8. As i8, it's -128 to 127.
            // Effective address: 0x9000 + (tile_number_as_i8 * 16)
            tile_data_start_addr = 0x9000u16.wrapping_add(((info.tile_number as i8) as i16 * 16) as u16);
        }

        // Calculate specific line data address within the tile
        let mut line_offset_in_tile = (bg_map_y % 8) as u16;
        if self.model.is_cgb_family() && (info.attributes & 0x04) != 0 { // Vertical flip for CGB
            line_offset_in_tile = 7 - line_offset_in_tile;
        }
        let tile_bytes_offset = line_offset_in_tile * 2; // 2 bytes per line (plane1 and plane2)

        info.tile_data_addr_plane1 = tile_data_start_addr + tile_bytes_offset;
        info.tile_data_addr_plane2 = info.tile_data_addr_plane1 + 1;

        // The actual read of tile data bytes would use:
        // byte1 = self.read_vram_bank_agnostic(info.tile_data_addr_plane1, tile_data_vram_bank);
        // byte2 = self.read_vram_bank_agnostic(info.tile_data_addr_plane2, tile_data_vram_bank);
        // Horizontal flip (info.attributes & 0x02 != 0 for CGB) would be applied when forming pixels from these bytes.

        // Restore VBK if it was changed - not needed due to read_vram_bank_agnostic
        // if self.model.is_cgb_family() { self.vbk = current_vbk; }

        info
    }

    // Helper to read from a specific VRAM bank, bypassing the PPU's current vbk state.
    // This is useful for fetching CGB attributes or specific tile data banks.
    fn read_vram_bank_agnostic(&self, addr: u16, bank: u8) -> u8 {
        if !self.model.is_cgb_family() && bank > 0 {
            // DMG only has bank 0
            return 0xFF;
        }

        let vram_bank_offset = if bank == 1 {
            VRAM_SIZE_DMG
        } else {
            0
        };

        let index = (addr as usize - 0x8000) + vram_bank_offset;
        if index < self.vram.len() {
            self.vram[index]
        } else {
             // This case should ideally not be hit if addresses and banks are calculated correctly.
            // eprintln!("Warning: VRAM read out of bounds. Addr: {:#06X}, Bank: {}, Index: {}, VRAM size: {}", addr, bank, index, self.vram.len());
            0xFF
        }
    }
}

// Helper to determine if PPU is accessing VRAM based on current mode
