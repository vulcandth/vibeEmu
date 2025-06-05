// src/ppu.rs
use image::{ImageBuffer, ImageError};
use crate::interrupts::InterruptType; // Added for returning interrupt type

// DMG Palette Colors (RGB)
const COLOR_WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const COLOR_LIGHT_GRAY: [u8; 3] = [0xAA, 0xAA, 0xAA];
const COLOR_DARK_GRAY: [u8; 3] = [0x55, 0x55, 0x55];
const COLOR_BLACK: [u8; 3] = [0x00, 0x00, 0x00];

const DMG_PALETTE: [[u8; 3]; 4] = [
    COLOR_WHITE,
    COLOR_LIGHT_GRAY,
    COLOR_DARK_GRAY,
    COLOR_BLACK,
];

// PPU Mode timings
const OAM_SCAN_CYCLES: u32 = 80;
const DRAWING_CYCLES: u32 = 172;
const HBLANK_CYCLES: u32 = 204;
const SCANLINE_CYCLES: u32 = OAM_SCAN_CYCLES + DRAWING_CYCLES + HBLANK_CYCLES; // 456
const VBLANK_LINES: u8 = 10;
const LINES_PER_FRAME: u8 = 144 + VBLANK_LINES; // 154

// PPU Modes (for STAT register bits 0-1)
const MODE_HBLANK: u8 = 0;
const MODE_VBLANK: u8 = 1;
const MODE_OAM_SCAN: u8 = 2;
const MODE_DRAWING: u8 = 3;

pub struct Ppu {
    pub vram: [u8; 8192], // Video RAM
    pub oam: [u8; 160],   // Object Attribute Memory
    pub framebuffer: Vec<u8>, // Stores pixels for the current frame (160x144 pixels, 3 bytes per pixel for RGB)
    pub cycles: u32,      // Cycles elapsed in the current scanline
    pub lcdc: u8,         // LCD Control (0xFF40)
    pub stat: u8,         // LCD Status (0xFF41)
    // Bit 7: LCD Display Enable
    // Bit 6: Window Tile Map Display Select (0=9800-9BFF, 1=9C00-9FFF)
    // Bit 5: Window Display Enable
    // Bit 4: BG & Window Tile Data Select (0=8800-97FF, 1=8000-8FFF)
    // Bit 3: BG Tile Map Display Select (0=9800-9BFF, 1=9C00-9FFF)
    // Bit 2: OBJ (Sprite) Size (0=8x8, 1=8x16)
    // Bit 1: OBJ (Sprite) Display Enable
    // Bit 0: BG/Window Display Enable (0=Off, 1=On)

    // STAT Register (0xFF41)
    // Bits 0-1: PPU Mode (0: HBlank, 1: VBlank, 2: OAM Scan, 3: Drawing)
    // Bit 2: LYC == LY Flag
    // Bit 3: Mode 0 HBlank Interrupt Enable
    // Bit 4: Mode 1 VBlank Interrupt Enable
    // Bit 5: Mode 2 OAM Interrupt Enable
    // Bit 6: LYC == LY Interrupt Enable
    // Bit 7: Unused (Always 1?) - Some docs say this bit is fixed 1.

    pub scy: u8,          // Scroll Y (0xFF42)
    pub scx: u8,          // Scroll X (0xFF43)
    pub ly: u8,           // LCD Y Coordinate (0xFF44) - Current scanline being processed
    pub lyc: u8,          // LY Compare (0xFF45)
    pub bgp: u8,          // Background Palette Data (0xFF47)
    pub obp0: u8,         // Object Palette 0 Data (0xFF48)
    pub obp1: u8,         // Object Palette 1 Data (0xFF49)
    pub wy: u8,           // Window Y Position (0xFF4A)
    pub wx: u8,           // Window X Position (0xFF4B)
}

impl Ppu {
    pub fn new() -> Self {
        // Initialize STAT: Mode 2 (OAM Scan) typically, LYC=LY flag might be set if LYC=0.
        // Bit 7 is often read as 1.
        // For simplicity, start in Mode 0 (HBLANK) as LY=0, Cycles=0 will transition it.
        // Or start in Mode 2, as LY=0, Cycles=0 implies start of a scanline.
        // Let's set initial mode to OAM_SCAN (2) as if frame is just starting.
        // And LYC=LY flag (bit 2) if ly (0) == lyc (0).
        let initial_ly = 0;
        let initial_lyc = 0;
        let mut initial_stat = MODE_OAM_SCAN; // Mode 2
        if initial_ly == initial_lyc {
            initial_stat |= 1 << 2; // Set LYC=LY flag
        }
        // Bit 7 of STAT is often fixed to 1
        initial_stat |= 1 << 7;


        Ppu {
            vram: [0; 8192],
            oam: [0; 160],
            framebuffer: vec![0; 160 * 144 * 3], // Initialize with black pixels
            cycles: 0,
            lcdc: 0x91, // Typical boot value
            stat: initial_stat,
            scy: 0,
            scx: 0,
            ly: initial_ly,
            lyc: initial_lyc,
            bgp: 0xFC, // Typical boot value
            obp0: 0xFF, // Typical boot value
            obp1: 0xFF, // Typical boot value
            wy: 0,
            wx: 0,
        }
    }

    // Helper to set PPU mode in STAT register
    fn set_mode(&mut self, mode: u8) -> Option<InterruptType> {
        // Clear bits 0-1 and then set them to the new mode
        self.stat = (self.stat & 0b1111_1100) | mode;
        let mut stat_interrupt_to_request: Option<InterruptType> = None;
        // Check for STAT interrupt conditions based on the new mode
        // Note: Mode 1 VBlank (for STAT) is also handled here if enabled in STAT.
        // This is distinct from the main VBlank interrupt (InterruptType::VBlank).
        match mode {
            MODE_HBLANK if (self.stat & (1 << 3)) != 0 => { // Mode 0 HBlank Interrupt Enable
                stat_interrupt_to_request = Some(InterruptType::LcdStat);
            }
            MODE_VBLANK if (self.stat & (1 << 4)) != 0 => { // Mode 1 VBlank Interrupt Enable for STAT
                stat_interrupt_to_request = Some(InterruptType::LcdStat);
            }
            MODE_OAM_SCAN if (self.stat & (1 << 5)) != 0 => { // Mode 2 OAM Interrupt Enable
                stat_interrupt_to_request = Some(InterruptType::LcdStat);
            }
            _ => {}
        }
        stat_interrupt_to_request
    }

    // Helper to check LYC=LY condition and update STAT
    fn check_lyc_ly(&mut self) -> Option<InterruptType> {
        let mut lyc_interrupt_to_request: Option<InterruptType> = None;
        if self.ly == self.lyc {
            self.stat |= 1 << 2; // Set LYC=LY flag (Bit 2)
            if (self.stat & (1 << 6)) != 0 { // LYC=LY Interrupt Enable
                lyc_interrupt_to_request = Some(InterruptType::LcdStat);
            }
        } else {
            self.stat &= !(1 << 2); // Clear LYC=LY flag
        }
        lyc_interrupt_to_request
    }

pub fn tick(&mut self) -> Option<InterruptType> {
        // Tick is assumed to be called for each PPU cycle (T-cycle)
    let mut interrupt_to_request: Option<InterruptType> = None;

        // LCD Power Check - if LCD is off, PPU does nothing, LY resets to 0, mode to 0 (HBlank)
        if self.lcdc & (1 << 7) == 0 { // Bit 7 is LCD Display Enable
            self.cycles = 0;
            self.ly = 0;
            // Mode is set to HBLANK. A STAT interrupt for HBLANK mode is not typical when LCD is just turned off.
            // LYC=LY check, however, should occur and can request a STAT interrupt.
            self.stat = (self.stat & 0b1111_1100) | MODE_HBLANK; // Set mode directly
            if let Some(stat_irq) = self.check_lyc_ly() {
                interrupt_to_request = Some(stat_irq); // LYC=LY check can still cause STAT IRQ
            }
        // Framebuffer might be cleared or show white.
        return interrupt_to_request; // Return potential STAT from LYC match, or None
        }

        self.cycles += 1; // Assuming 1 tick call = 1 PPU cycle. Adjust if PPU gets more cycles at once.

        if self.ly >= 144 { // VBlank period (Lines 144-153)
            // In VBlank period, mode is MODE_VBLANK (1)
            // This mode setting occurs once when LY first becomes 144.
            if (self.stat & 0b11) != MODE_VBLANK { // Ensure mode is VBLANK
                if let Some(stat_irq) = self.set_mode(MODE_VBLANK) {
                    // Only set LcdStat if VBlank is not already being requested this tick
                    if interrupt_to_request != Some(InterruptType::VBlank) {
                        interrupt_to_request = Some(stat_irq);
                    }
                }
            }

            if self.cycles >= SCANLINE_CYCLES {
                self.cycles = 0;
                self.ly += 1;
                if self.ly >= LINES_PER_FRAME { // End of VBlank period
                    self.ly = 0; // Reset for new frame
                    // Transition to Mode 2 (OAM Scan) for the new frame's first line.
                    if let Some(stat_irq) = self.set_mode(MODE_OAM_SCAN) {
                         if interrupt_to_request != Some(InterruptType::VBlank) { // Should not be VBlank here
                            interrupt_to_request = Some(stat_irq);
                        }
                    }
                }
                // LYC check after LY changes
                if let Some(stat_irq) = self.check_lyc_ly() {
                    if interrupt_to_request != Some(InterruptType::VBlank) { // VBlank is primary
                        interrupt_to_request = Some(stat_irq);
                    }
                }
            }
        } else { // Scanlines 0-143 (Visible frame)
            let current_mode = self.stat & 0b11;

            if self.cycles < OAM_SCAN_CYCLES { // Mode 2 - OAM Scan
                if current_mode != MODE_OAM_SCAN {
                    if let Some(stat_irq) = self.set_mode(MODE_OAM_SCAN) {
                        // VBlank should not be pending here.
                        interrupt_to_request = Some(stat_irq);
                    }
                }
            } else if self.cycles < OAM_SCAN_CYCLES + DRAWING_CYCLES { // Mode 3 - Drawing
                if current_mode != MODE_DRAWING {
                    // Mode 3 (Drawing) itself doesn't trigger a STAT interrupt by changing to it.
                    self.stat = (self.stat & 0b1111_1100) | MODE_DRAWING; // Directly set mode
                    // Render scanline on first cycle of Mode 3 (if cycles just crossed into this range)
                    // This check might need to be more precise (e.g. self.cycles == OAM_SCAN_CYCLES)
                    if self.cycles == OAM_SCAN_CYCLES { // First cycle of Mode 3
                        self.render_scanline();
                    }
                }
            } else if self.cycles < SCANLINE_CYCLES { // Mode 0 - HBlank
                 if current_mode != MODE_HBLANK {
                    if let Some(stat_irq) = self.set_mode(MODE_HBLANK) {
                        // VBlank should not be pending here.
                        interrupt_to_request = Some(stat_irq);
                    }
                }
            } else { // End of a scanline (cycles == SCANLINE_CYCLES)
                self.cycles = 0;
                self.ly += 1;

                // LYC check after LY changes, before potential VBlank transition
                if let Some(stat_irq) = self.check_lyc_ly() {
                    // If VBlank is not about to be requested, set LcdStat
                    if self.ly != 144 { // Avoid overwriting if VBlank is next
                        interrupt_to_request = Some(stat_irq);
                    } else { // ly is 144, VBlank will be requested. Only set LcdStat if VBlank itself is not set.
                        // This case is tricky: VBlank takes precedence.
                        // If check_lyc_ly returns LcdStat, and VBlank is also triggered, VBlank wins.
                        // The VBlank assignment below will handle this.
                        // So, if we are here and ly is 144, we can tentatively set LcdStat.
                        if interrupt_to_request == None { // Tentatively set if nothing else yet
                           interrupt_to_request = Some(stat_irq);
                        }
                    }
                }

                if self.ly == 144 { // Transition to VBlank
                    // VBlank interrupt (IF bit 0) is primary for ly=144. This overwrites any prior LcdStat for this tick.
                    interrupt_to_request = Some(InterruptType::VBlank);
                    // set_mode for VBLANK can also trigger a STAT interrupt (for STAT bit 4).
                    // This is secondary to the main VBlank interrupt.
                    // If VBlank (InterruptType::VBlank) is set, bus logic handles IF bit 0.
                    // If STAT bit 4 is also set, bus logic ORs IF bit 1 (LcdStat).
                    // Our PPU tick should signal *both* if conditions met.
                    // However, main.rs only takes one InterruptType.
                    // The problem implies VBlank takes precedence for return type if both conditions met.
                    // So if set_mode(MODE_VBLANK) also implies LcdStat, VBlank is still returned.
                    // The bus.request_interrupt will handle the specific type.
                    // Let's ensure the mode is set.
                    let _ = self.set_mode(MODE_VBLANK); // Ensure mode is set; its LcdStat return is ignored if VBlank is primary.

                    // Save framebuffer at the start of VBlank
                    // This is done only once per frame, when ly first becomes 144.
                    // Ensure this is the *actual* first cycle of this line in VBlank.
                    // The mode transition to VBLANK happens right before this check.
                    // The `cycles` would have just been reset to 0 from the previous line's completion.
                    // Screenshotting is now triggered by main.rs test framework logic
                    // Ensure framebuffer is updated at this point if it's a new frame
                    if self.cycles == 0 { // Should be true if we just reset cycles
                         // self.framebuffer_internal = self.framebuffer.clone(); // Assuming this was for testing
                         // self.new_frame_ready = true;
                    }

                } else { // Still in visible scanlines (0-143), transition to Mode 2 for the next line
                    if let Some(stat_irq) = self.set_mode(MODE_OAM_SCAN) {
                        // VBlank should not be pending here.
                        // If LYC=LY caused an LcdStat already, this might overwrite it if it's also LcdStat. This is fine.
                        interrupt_to_request = Some(stat_irq);
                    }
                }
            }
        }

    interrupt_to_request // Return any requested interrupt
    }

    fn render_scanline(&mut self) {
        // If LCDC bit 7 is off (LCD disabled), do nothing or fill with white.
        if (self.lcdc & (1 << 7)) == 0 {
            for x in 0..160 {
                let fb_idx = (self.ly as usize * 160 + x) * 3;
                self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&COLOR_WHITE);
            }
            return;
        }

        // If LCDC bit 0 is off (BG/Window display disabled), fill with white.
        // Note: Some emulators might make this distinction (BG off vs LCD off)
        // For now, if BG is off, we render white.
        if (self.lcdc & (1 << 0)) == 0 {
             for x in 0..160 {
                let fb_idx = (self.ly as usize * 160 + x) * 3;
                self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&COLOR_WHITE);
            }
            return;
        }

        let tile_map_base_addr = if (self.lcdc & (1 << 3)) == 0 { 0x9800 } else { 0x9C00 };
        let tile_data_base_addr = if (self.lcdc & (1 << 4)) == 0 { 0x8800 } else { 0x8000 }; // 0x8800 uses signed indexing from 0x9000
        let signed_tile_addressing = (self.lcdc & (1 << 4)) == 0;

        let current_scanline_y = self.ly; // Y coordinate on the screen (0-143)
        let scroll_y = self.scy;
        let scroll_x = self.scx;

        for x_on_screen in 0..160 { // For each pixel on the current scanline (0-159)
            let map_x = (scroll_x.wrapping_add(x_on_screen)) as u16; // X position in the 256x256 BG map
            let map_y = (scroll_y.wrapping_add(current_scanline_y)) as u16; // Y position in the 256x256 BG map

            let tile_col_idx = map_x / 8; // Which column of tiles in the map
            let tile_row_idx = map_y / 8; // Which row of tiles in the map

            // Get tile index from tile map
            // Each map is 32x32 tiles.
            let tile_map_offset = tile_row_idx * 32 + tile_col_idx;
            let tile_index_addr_in_vram = (tile_map_base_addr + tile_map_offset - 0x8000) as usize;
            let tile_index = self.vram[tile_index_addr_in_vram];

            // Calculate address of the tile's data
            let tile_data_start_addr_in_vram = if signed_tile_addressing {
                // Tile data from 0x8800 to 0x97FF. Indices are signed (-128 to 127), relative to 0x9000.
                // tile_index is u8. Cast to i8 for signed arithmetic.
                // Effective address = 0x9000 + (tile_index as i8 as i16) * 16
                // Base for vram array = (0x9000 - 0x8000) + (tile_index as i8 as i16) * 16
                let base_offset = 0x1000; // 0x9000 - 0x8000
                (base_offset as i16 + (tile_index as i8 as i16) * 16) as usize
            } else {
                // Tile data from 0x8000 to 0x8FFF. Indices are unsigned (0 to 255).
                // tile_index is u8.
                // Effective address = 0x8000 + (tile_index as u16) * 16
                // Base for vram array = (tile_index as u16) * 16
                (tile_data_base_addr - 0x8000 + (tile_index as u16) * 16) as usize
            };

            let pixel_y_in_tile = (map_y % 8) as usize; // Which row of pixels in the tile (0-7)

            // Each row of pixels in a tile takes 2 bytes
            let byte1_vram_addr = tile_data_start_addr_in_vram + pixel_y_in_tile * 2;
            let byte2_vram_addr = byte1_vram_addr + 1;

            // Ensure addresses are within VRAM bounds (0x0000 to 0x1FFF for the self.vram array)
            // This check is mostly for the signed addressing which can go "below" 0x8800 if not careful
            // but since tile_data_start_addr_in_vram is usize, it should be fine.
            // However, let's be safe. Max address for tile data is 0x97FF for signed, 0x8FFF for unsigned.
            // Max index in self.vram is 8191 (0x1FFF)
            if byte2_vram_addr >= 8192 { // VRAM size
                // This case should ideally not happen with correct logic but indicates an error if it does.
                // Fill with a debug color like magenta if it happens.
                let fb_idx = (current_scanline_y as usize * 160 + x_on_screen as usize) * 3;
                self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&[0xFF,0,0xFF]); // Magenta
                continue;
            }

            let byte1 = self.vram[byte1_vram_addr];
            let byte2 = self.vram[byte2_vram_addr];

            // Pixel data is stored from right to left. bit 7 is leftmost pixel, bit 0 is rightmost.
            let pixel_x_in_tile_bit_pos = 7 - (map_x % 8) as u8;

            let color_bit1 = (byte1 >> pixel_x_in_tile_bit_pos) & 1; // Low bit of color id
            let color_bit0 = (byte2 >> pixel_x_in_tile_bit_pos) & 1; // High bit of color id
            let color_num = (color_bit0 << 1) | color_bit1; // gb palette index (0-3)

            // Map through BGP register
            // color_num 00, 01, 10, 11 corresponds to bits 1-0, 3-2, 5-4, 7-6 of BGP
            let output_color_num = (self.bgp >> (color_num * 2)) & 0b11;
            let final_color = DMG_PALETTE[output_color_num as usize];

            let fb_idx = (current_scanline_y as usize * 160 + x_on_screen as usize) * 3;
            self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&final_color);
        }
    }


    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => { // VRAM
                // VRAM is readable depending on PPU mode. For now, allow reads.
                let relative_addr = addr - 0x8000;
                self.vram[relative_addr as usize]
            }
            0xFE00..=0xFE9F => { // OAM
                // OAM is readable depending on PPU mode. For now, allow reads.
                let relative_addr = addr - 0xFE00;
                self.oam[relative_addr as usize]
            }
            0xFF40 => self.lcdc,
            0xFF41 => self.stat | 0x80, // Bit 7 is often reported as high, ensure it is.
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _ => {
                // println!("PPU read from unmapped address: {:#04X}", addr);
                0xFF // Default for unmapped PPU addresses
            }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        // Check if LCD is off; if so, many PPU registers/memory might not be writable
        // or behave differently. For now, we assume they are writable unless specified.
        // if (self.lcdc & (1 << 7)) == 0 { /* LCD is off */ }

        match addr {
            0x8000..=0x9FFF => { // VRAM
                // VRAM is writable depending on PPU mode. For now, allow writes.
                let relative_addr = addr - 0x8000;
                self.vram[relative_addr as usize] = value;
            }
            0xFE00..=0xFE9F => { // OAM
                // OAM is writable depending on PPU mode. For now, allow writes.
                // TODO: Implement OAM write restrictions (e.g., not writable during Mode 2 and 3).
                let relative_addr = addr - 0xFE00;
                self.oam[relative_addr as usize] = value;
            }
            0xFF40 => { // LCDC
                let old_lcd_enabled = (self.lcdc & (1 << 7)) != 0;
                self.lcdc = value;
                let new_lcd_enabled = (self.lcdc & (1 << 7)) != 0;
                if old_lcd_enabled && !new_lcd_enabled {
                    // LCD turned off: LY resets to 0, PPU clock stops, PPU enters Mode 0.
                    self.ly = 0;
                    self.cycles = 0;
                    self.set_mode(MODE_HBLANK); // Or specific off mode if any
                    // Framebuffer might be cleared to white.
                    for i in 0..self.framebuffer.len() { self.framebuffer[i] = 0xFF; }
                }
                // If LCD is turned on, PPU starts its sequence.
                // This is implicitly handled by tick() being called.
            }
            0xFF41 => { // STAT
                // Bits 0-2 (Mode) are read-only from CPU side.
                // Bit 7 is read-only (always 1).
                // CPU can write to bits 3-6 (Interrupt enables).
                self.stat = (value & 0b0111_1000) | (self.stat & 0b1000_0111);
                // Bit 7 needs to be preserved as 1 (or whatever hardware dictates)
                self.stat |= 0x80;
            }
            0xFF42 => self.scy = value,
            0xFF43 => self.scx = value,
            0xFF44 => { /* LY is Read-Only */
                // Writing to LY on some DMG models resets LY to 0.
                // For now, treat as read-only (no effect).
                // self.ly = 0; // If emulating LY reset behavior
            }
            0xFF45 => { // LYC
                self.lyc = value;
                // Writing to LYC should also potentially trigger STAT interrupt if LYC=LY and enabled.
                // The check_lyc_ly() call here updates STAT bit 2, but doesn't return interrupt from write_byte.
                // For full accuracy, write_byte could also return Option<InterruptType>.
                // For now, interrupt is checked on next PPU tick.
                let _ = self.check_lyc_ly(); // Update STAT flag, ignore potential IRQ for now from here
            }
            0xFF47 => self.bgp = value, // BGP
            0xFF48 => self.obp0 = value, // OBP0
            0xFF49 => self.obp1 = value, // OBP1
            0xFF4A => self.wy = value,  // WY
            0xFF4B => self.wx = value,  // WX
            _ => {
                // println!("PPU write to unmapped address: {:#04X} with value: {:#02X}", addr, value);
            }
        }
    }
}

pub fn save_framebuffer_to_png(
    framebuffer: &[u8],
    width: u32,
    height: u32,
    filepath: &str,
) -> Result<(), ImageError> {
    if framebuffer.len() != (width * height * 3) as usize {
        // This is a simplistic error. A custom error type might be better.
        return Err(ImageError::Parameter(
            image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ),
        ));
    }

    // Create an RgbImage directly from the flat RGB data.
    // The `image` crate expects data in row-major order (R, G, B, R, G, B, ...).
    // Our framebuffer is already in this format.
    match ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(width, height, framebuffer.to_vec()) {
        Some(image_buffer) => {
            image_buffer.save(filepath) // This returns Result<(), ImageError>
        }
        None => {
            // This case should ideally not be reached if the length check above is correct,
            // but from_raw can fail for other reasons (e.g. internal allocation issues).
            Err(ImageError::Parameter(
                image::error::ParameterError::from_kind(
                    image::error::ParameterErrorKind::Generic(
                        "Failed to create image buffer from raw data".to_string(),
                    ),
                ),
            ))
        }
    }
}
