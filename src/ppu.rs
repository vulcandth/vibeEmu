// src/ppu.rs
use image::{ImageBuffer, ImageError};
use crate::interrupts::InterruptType;

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

// Helper function to convert CGB 15-bit color to 24-bit RGB
fn cgb_color_to_rgb(color_val: u16) -> [u8; 3] {
    let r_5bit = (color_val & 0x1F) as u8;
    let g_5bit = ((color_val >> 5) & 0x1F) as u8;
    let b_5bit = ((color_val >> 10) & 0x1F) as u8;

    // Scale 5-bit values to 8-bit values
    // (val * 255) / 31 = val * 8.22...
    // A common way is to shift: (val << 3) | (val >> 2)
    let r_8bit = (r_5bit << 3) | (r_5bit >> 2);
    let g_8bit = (g_5bit << 3) | (g_5bit >> 2);
    let b_8bit = (b_5bit << 3) | (b_5bit >> 2);

    [r_8bit, g_8bit, b_8bit]
}

// PPU Mode timings
const OAM_SCAN_CYCLES: u32 = 80;
const DRAWING_CYCLES: u32 = 172;
const HBLANK_CYCLES: u32 = 204;
const SCANLINE_CYCLES: u32 = OAM_SCAN_CYCLES + DRAWING_CYCLES + HBLANK_CYCLES;
const VBLANK_LINES: u8 = 10;
const LINES_PER_FRAME: u8 = 144 + VBLANK_LINES;

// PPU Modes (for STAT register bits 0-1)
const MODE_HBLANK: u8 = 0;
const MODE_VBLANK: u8 = 1;
const MODE_OAM_SCAN: u8 = 2;
const MODE_DRAWING: u8 = 3;

pub struct Ppu {
    pub system_mode: crate::bus::SystemMode, // Added CGB mode tracking
    pub vram: [[u8; 8192]; 2], // Two VRAM banks for CGB
    pub oam: [u8; 160],
    pub framebuffer: Vec<u8>,
    pub cycles: u32,
    pub lcdc: u8,
    pub stat: u8,
    pub scy: u8,
    pub scx: u8,
    pub ly: u8,
    pub lyc: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,
    pub wy: u8,
    pub wx: u8,
    // CGB Palette RAM
    pub cgb_bg_palette_ram: [u8; 64],
    pub cgb_obj_palette_ram: [u8; 64],
    // CGB Palette Index/Specification Registers
    pub bcps: u8, // FF68 - BCPS - Background Color Palette Specification
    pub ocps: u8, // FF6A - OCPS - Object Color Palette Specification
    pub vbk: u8,  // FF4F - VBK - CGB VRAM Bank Select
    pub just_entered_hblank: bool, // Flag for HDMA triggering
}

impl Ppu {
    pub fn new(system_mode: crate::bus::SystemMode) -> Self {
        let initial_ly = 0;
        let initial_lyc = 0;
        let mut initial_stat = MODE_OAM_SCAN;
        if initial_ly == initial_lyc {
            initial_stat |= 1 << 2;
        }
        initial_stat |= 1 << 7;


        Ppu {
            vram: [[0; 8192]; 2], // Initialize both VRAM banks
            oam: [0; 160],
            framebuffer: vec![0; 160 * 144 * 3],
            cycles: 0,
            lcdc: 0x91,
            stat: initial_stat,
            scy: 0,
            scx: 0,
            ly: initial_ly,
            lyc: initial_lyc,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            system_mode, // Store the passed system_mode
            cgb_bg_palette_ram: [0xFF; 64], // Initialize CGB BG palettes (e.g., to 0xFF)
            cgb_obj_palette_ram: [0xFF; 64], // Initialize CGB OBJ palettes (e.g., to 0xFF)
            bcps: 0x00, // Initialize BCPS
            ocps: 0x00, // Initialize OCPS
            vbk: 0x00,  // Initialize VBK (Bank 0)
            just_entered_hblank: false,
        }
    }

    fn set_mode(&mut self, mode: u8) -> Option<InterruptType> {
        self.stat = (self.stat & 0b1111_1100) | mode;
        let mut stat_interrupt_to_request: Option<InterruptType> = None;

        self.just_entered_hblank = false; // Clear flag by default

        match mode {
            MODE_HBLANK => {
                self.just_entered_hblank = true; // Set flag when entering HBlank
                if (self.stat & (1 << 3)) != 0 {
                    stat_interrupt_to_request = Some(InterruptType::LcdStat);
                }
            }
            MODE_VBLANK if (self.stat & (1 << 4)) != 0 => {
                stat_interrupt_to_request = Some(InterruptType::LcdStat);
            }
            MODE_OAM_SCAN if (self.stat & (1 << 5)) != 0 => {
                stat_interrupt_to_request = Some(InterruptType::LcdStat);
            }
            _ => {}
        }
        stat_interrupt_to_request
    }

    fn check_lyc_ly(&mut self) -> Option<InterruptType> {
        let mut lyc_interrupt_to_request: Option<InterruptType> = None;
        if self.ly == self.lyc {
            self.stat |= 1 << 2;
            if (self.stat & (1 << 6)) != 0 {
                lyc_interrupt_to_request = Some(InterruptType::LcdStat);
            }
        } else {
            self.stat &= !(1 << 2);
        }
        lyc_interrupt_to_request
    }

    pub fn tick(&mut self, t_cycles: u32) -> Option<InterruptType> {
        let mut vblank_occurred_this_call = false;
        let mut lcd_stat_occurred_this_call = false;

        if (self.lcdc & (1 << 7)) == 0 { // LCD Display Enable is off
            if self.ly != 0 { // If LY is not already 0, reset it and PPU state
                 self.ly = 0;
                 self.cycles = 0; // Reset cycle count for the scanline timing
                 // Set mode to VBLANK (Mode 1) when LCD is disabled
                 if self.set_mode(MODE_VBLANK).is_some() { // This also checks for STAT interrupt trigger
                     lcd_stat_occurred_this_call = true;
                 }
            } else {
                // If LY is already 0 and LCD is off, ensure mode is VBLANK.
                // This handles the case where LCD was disabled while LY was already 0.
                // Or if LCD is kept off for multiple ticks.
                if (self.stat & 0b11) != MODE_VBLANK {
                    if self.set_mode(MODE_VBLANK).is_some() {
                        lcd_stat_occurred_this_call = true;
                    }
                }
            }
            // LYC=LY check still occurs even when LCD is off
            if self.check_lyc_ly().is_some() {
                 lcd_stat_occurred_this_call = true;
            }
            return if lcd_stat_occurred_this_call { Some(InterruptType::LcdStat) } else { None };
        }

        for _ in 0..t_cycles {
            let mut current_sub_cycle_vblank = false;
            let mut current_sub_cycle_lcd_stat = false;

            self.cycles += 1;

            if self.ly >= 144 {
                if (self.stat & 0b11) != MODE_VBLANK {
                    if self.set_mode(MODE_VBLANK).is_some() {
                        current_sub_cycle_lcd_stat = true;
                    }
                }

                if self.cycles >= SCANLINE_CYCLES {
                    self.cycles = 0;
                    self.ly += 1;
                    if self.ly >= LINES_PER_FRAME {
                        self.ly = 0;
                        if self.set_mode(MODE_OAM_SCAN).is_some() {
                            current_sub_cycle_lcd_stat = true;
                        }
                    }
                    if self.check_lyc_ly().is_some() {
                        current_sub_cycle_lcd_stat = true;
                    }
                }
            } else {
                let current_mode = self.stat & 0b11;

                if self.cycles < OAM_SCAN_CYCLES {
                    if current_mode != MODE_OAM_SCAN {
                        if self.set_mode(MODE_OAM_SCAN).is_some() {
                            current_sub_cycle_lcd_stat = true;
                        }
                    }
                } else if self.cycles < OAM_SCAN_CYCLES + DRAWING_CYCLES {
                    if current_mode != MODE_DRAWING {
                        self.stat = (self.stat & 0b1111_1100) | MODE_DRAWING;
                        if self.cycles == OAM_SCAN_CYCLES {
                            self.render_scanline();
                        }
                    }
                } else if self.cycles < SCANLINE_CYCLES {
                     if current_mode != MODE_HBLANK {
                        if self.set_mode(MODE_HBLANK).is_some() {
                            current_sub_cycle_lcd_stat = true;
                        }
                    }
                } else {
                    self.cycles = 0;
                    self.ly += 1;

                    if self.check_lyc_ly().is_some() {
                        current_sub_cycle_lcd_stat = true;
                    }

                    if self.ly == 144 {
                        current_sub_cycle_vblank = true;
                        if self.set_mode(MODE_VBLANK).is_some() {
                            current_sub_cycle_lcd_stat = true;
                        }
                    } else {
                        if self.set_mode(MODE_OAM_SCAN).is_some() {
                            current_sub_cycle_lcd_stat = true;
                        }
                    }
                }
            }

            if current_sub_cycle_vblank {
                vblank_occurred_this_call = true;
            }
            if current_sub_cycle_lcd_stat {
                lcd_stat_occurred_this_call = true;
            }
        }

        if vblank_occurred_this_call {
            return Some(InterruptType::VBlank);
        }
        if lcd_stat_occurred_this_call {
            return Some(InterruptType::LcdStat);
        }
        None
    }

    fn render_scanline(&mut self) {
        if (self.lcdc & (1 << 7)) == 0 {
            return;
        }

        // DMG Mode Specific: If LCDC Bit 0 is off, BG/Window are white (color 0 of BGP). Sprites can still draw.
        // In CGB Mode, LCDC Bit 0 is BG-to-OBJ Master Priority, not a display disable for BG/Win.
        let bg_is_blank_dmg = self.system_mode == crate::bus::SystemMode::DMG && (self.lcdc & (1 << 0)) == 0;

        if bg_is_blank_dmg {
            let dmg_color_0_idx = (self.bgp >> (0 * 2)) & 0b11; // Color for index 0 from BGP
            let dmg_actual_color_0 = DMG_PALETTE[dmg_color_0_idx as usize];
            for x_on_screen in 0..160 {
                let fb_idx = (self.ly as usize * 160 + x_on_screen as usize) * 3;
                if fb_idx + 2 < self.framebuffer.len() {
                    self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&dmg_actual_color_0);
                }
            }
            // Do not return; sprites should still render over this background.
        }

        // Actual Background & Window Rendering (if not blanked in DMG by LCDC.0)
        if !bg_is_blank_dmg {
            let tile_map_base_addr = if (self.lcdc & (1 << 3)) == 0 { 0x9800 } else { 0x9C00 };
            // ... rest of BG rendering logic ...
            // ... Window rendering logic needs to be inside this `if !bg_is_blank_dmg` block too ...
            // This means the previous change that put Window rendering outside the BG loop needs adjustment
            // or window rendering needs its own `if !bg_is_blank_dmg` check.
            // For simplicity, let's assume the existing BG loop handles its part,
            // and the window part will also be conditional on !bg_is_blank_dmg.
            // The structure from previous diff for BG rendering:
            // for x_on_screen in 0..160 { ... BG pixel logic ... }
            // Then for Window:
            // if (self.lcdc & (1 << 5)) != 0 && self.ly >= self.wy && self.wx < 167 { ... Window pixel logic ... }
            // This structure means the window logic needs its own check for `!bg_is_blank_dmg` if it's separate.
            // Or, if window rendering is inside the `for x_on_screen in 0..160` loop (which it typically is for pixel overwrite),
            // then the outer `if !bg_is_blank_dmg` covers it.
            // Let's assume the current BG loop will be wrapped, and the Window loop will also be wrapped.
            // The provided diff seems to put the BG loop inside `if !bg_is_blank_dmg`.
            // The Window loop, if separate, will also need this condition.
            // My current `render_scanline` has BG, then Window, then Sprites sequentially.
            // So, BG rendering will be wrapped. Window rendering will also need to be wrapped.
        } // This closes `if !bg_is_blank_dmg` for BG. Window needs its own check or be included.

        // The previous diff for render_scanline:
        // if !bg_is_blank_dmg { BG_LOOP } WINDOW_LOGIC SPRITE_LOGIC
        // This is not quite right. Window logic should also be skipped if bg_is_blank_dmg.
        // The correct structure should be:
        // if bg_is_blank_dmg { FILL_WHITE_DMG }
        // else { BG_RENDERING; WINDOW_RENDERING_IF_ENABLED; }
        // SPRITE_RENDERING;

        // So, the change below will wrap the existing BG and Window rendering sections.
        // --- Search for the start of BG rendering's tile_map_base_addr ---
        // This is a bit tricky with replace_with_git_merge_diff for large structural changes.
        // I will apply the if condition around the existing BG and Window rendering blocks.
        // The current diff utility is not ideal for wrapping large existing blocks.
        // I will make two changes:
        // 1. The initial blanking logic.
        // 2. Add the `if !bg_is_blank_dmg` condition around the existing BG and Window rendering parts.

        // For now, this first diff block only introduces the blanking logic
        // and the start of the conditional block for actual rendering.
        // The end of this conditional block will be added in a subsequent diff.
        // This is a limitation of applying large structural changes with the current diff tool.
        //
        // The search block below should be the original start of the BG rendering section.
        let tile_map_base_addr = if (self.lcdc & (1 << 3)) == 0 { 0x9800 } else { 0x9C00 };
        let tile_data_base_addr = if (self.lcdc & (1 << 4)) == 0 { 0x8800 } else { 0x8000 };
        let signed_tile_addressing = (self.lcdc & (1 << 4)) == 0;

        let current_scanline_y = self.ly;
        let scroll_y = self.scy;
        let scroll_x = self.scx;

        for x_on_screen in 0..160 {
            let map_x = (scroll_x.wrapping_add(x_on_screen)) as u16;
            let map_y = (scroll_y.wrapping_add(current_scanline_y)) as u16;

            let tile_col_idx = map_x / 8;
            let tile_row_idx = map_y / 8;

            let tile_map_offset = tile_row_idx * 32 + tile_col_idx;
            let tile_index_map_addr = (tile_map_base_addr + (tile_map_offset % 1024) - 0x8000) as usize;

            if tile_index_map_addr >= 8192 { continue; } // Check offset against bank size

            // BG Tilemap is always in VRAM Bank 0
            let tile_index = self.vram[0][tile_index_map_addr];

            let mut bg_cgb_palette_idx: u8 = 0;
            let mut bg_tile_data_bank: usize = 0;
            let mut bg_x_flip: bool = false;
            let mut bg_y_flip: bool = false;
            // let mut _bg_to_oam_priority: bool = false; // For later fine-grained priority

            if self.system_mode == crate::bus::SystemMode::CGB {
                // BG Attributes are always in VRAM Bank 1
                let tile_attributes = self.vram[1][tile_index_map_addr];
                bg_cgb_palette_idx = tile_attributes & 0x07;
                bg_tile_data_bank = if (tile_attributes & (1 << 3)) != 0 { 1 } else { 0 };
                bg_x_flip = (tile_attributes & (1 << 5)) != 0;
                bg_y_flip = (tile_attributes & (1 << 6)) != 0;
                // _bg_to_oam_priority = (tile_attributes & (1 << 7)) != 0;
            }

            let tile_data_offset_in_bank = if signed_tile_addressing {
                // 0x8800 mode: tile_index is signed i8, data is at 0x9000 + index*16
                // 0x1000 in bank terms (0x9000-0x8000)
                (0x1000 as i16 + (tile_index as i8 as i16) * 16) as usize
            } else {
                // 0x8000 mode: tile_index is unsigned u8, data is at 0x8000 + index*16
                ((tile_data_base_addr - 0x8000) + (tile_index as u16) * 16) as usize
            };

            let mut pixel_y_in_tile = (map_y % 8) as u8;
            if bg_y_flip { // Apply Y-flip for CGB BG tiles
                pixel_y_in_tile = 7 - pixel_y_in_tile;
            }

            let byte1_addr_in_bank = tile_data_offset_in_bank + (pixel_y_in_tile as usize * 2);
            let byte2_addr_in_bank = byte1_addr_in_bank + 1;

            if byte2_addr_in_bank >= 8192 { // Check offset against bank size
                let fb_idx = (current_scanline_y as usize * 160 + x_on_screen as usize) * 3;
                if fb_idx + 2 < self.framebuffer.len() {
                     self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&[0xFF,0,0xFF]); // Magenta for error
                }
                continue;
            }

            let byte1 = self.vram[bg_tile_data_bank][byte1_addr_in_bank];
            let byte2 = self.vram[bg_tile_data_bank][byte2_addr_in_bank];

            let mut pixel_x_in_tile_bit_pos = 7 - (map_x % 8) as u8;
            if bg_x_flip { // Apply X-flip for CGB BG tiles
                pixel_x_in_tile_bit_pos = 7 - pixel_x_in_tile_bit_pos;
            }

            let color_bit1 = (byte1 >> pixel_x_in_tile_bit_pos) & 1;
            let color_bit0 = (byte2 >> pixel_x_in_tile_bit_pos) & 1;
            let color_num = (color_bit0 << 1) | color_bit1;

            let final_color: [u8; 3];
            if self.system_mode == crate::bus::SystemMode::CGB {
                let palette_ram_base_idx = (bg_cgb_palette_idx as usize * 8) + (color_num as usize * 2);
                if palette_ram_base_idx + 1 < self.cgb_bg_palette_ram.len() {
                    let color_byte1 = self.cgb_bg_palette_ram[palette_ram_base_idx];
                    let color_byte2 = self.cgb_bg_palette_ram[palette_ram_base_idx + 1];
                    let cgb_color_val = ((color_byte2 as u16) << 8) | (color_byte1 as u16);
                    final_color = cgb_color_to_rgb(cgb_color_val);
                } else {
                    final_color = COLOR_WHITE; // Fallback
                }
            } else {
                // DMG Background Rendering
                let output_color_num = (self.bgp >> (color_num * 2)) & 0b11;
                final_color = DMG_PALETTE[output_color_num as usize];
            }

            let fb_idx = (current_scanline_y as usize * 160 + x_on_screen as usize) * 3;
            if fb_idx + 2 < self.framebuffer.len() {
                self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&final_color);
            }
        }

        // Window Rendering
        // Condition: Window Display Enable (LCDC bit 5), LY >= WY, WX < 167
        if (self.lcdc & (1 << 5)) != 0 && self.ly >= self.wy && self.wx < 167 {
            let win_tile_map_base_addr = if (self.lcdc & (1 << 6)) == 0 { 0x9800 } else { 0x9C00 };
            // Tile data addressing (shared with BG) - these are fine for CGB too
            let tile_data_base_addr_win = if (self.lcdc & (1 << 4)) == 0 { 0x8800 } else { 0x8000 };
            let signed_addressing_win = (self.lcdc & (1 << 4)) == 0;

            let win_y_in_map = self.ly - self.wy; // Y-coordinate within the window's internal map

            for x_on_screen_win in 0..160u8 { // Renamed to avoid conflict with outer loop var
                // WX is screen X coord + 7. So window starts at WX-7.
                let window_start_x = self.wx.saturating_sub(7);
                if x_on_screen_win >= window_start_x { // Use x_on_screen_win here
                    let win_x_in_map = x_on_screen_win - window_start_x; // Use x_on_screen_win here

                    let win_tile_x = win_x_in_map / 8;
                    let win_tile_y = win_y_in_map / 8;

                    let tile_map_offset = win_tile_y as u16 * 32 + win_tile_x as u16;
                    let tile_index_map_addr_win = (win_tile_map_base_addr + (tile_map_offset % 1024) - 0x8000) as usize;

                    if tile_index_map_addr_win >= 8192 { continue; }

                    // Window Tilemap is always in VRAM Bank 0
                    let tile_index_win = self.vram[0][tile_index_map_addr_win];

                    let mut win_cgb_palette_idx: u8 = 0;
                    let mut win_tile_data_bank: usize = 0;
                    let mut win_x_flip: bool = false;
                    let mut win_y_flip: bool = false;

                    if self.system_mode == crate::bus::SystemMode::CGB {
                        // Window Attributes are always in VRAM Bank 1
                        let win_attributes = self.vram[1][tile_index_map_addr_win];
                        win_cgb_palette_idx = win_attributes & 0x07;
                        win_tile_data_bank = if (win_attributes & (1 << 3)) != 0 { 1 } else { 0 };
                        win_x_flip = (win_attributes & (1 << 5)) != 0;
                        win_y_flip = (win_attributes & (1 << 6)) != 0;
                        // BG-to-OAM priority also applies to window
                    }

                    let tile_data_offset_in_bank_win = if signed_addressing_win {
                        (0x1000 as i16 + (tile_index_win as i8 as i16) * 16) as usize
                    } else {
                        ((tile_data_base_addr_win - 0x8000) + (tile_index_win as u16) * 16) as usize
                    };

                    let mut pixel_y_in_tile_win = (win_y_in_map % 8) as u8;
                    if win_y_flip { // Apply Y-flip for CGB Window tiles
                        pixel_y_in_tile_win = 7 - pixel_y_in_tile_win;
                    }

                    let mut pixel_x_in_tile_bit_pos_win = 7 - (win_x_in_map % 8);
                    if win_x_flip { // Apply X-flip for CGB Window tiles
                       pixel_x_in_tile_bit_pos_win = 7 - pixel_x_in_tile_bit_pos_win;
                    }

                    let byte1_addr_in_bank_win = tile_data_offset_in_bank_win + (pixel_y_in_tile_win as usize * 2);
                    let byte2_addr_in_bank_win = byte1_addr_in_bank_win + 1;

                    if byte2_addr_in_bank_win >= 8192 { continue; }

                    let byte1_win = self.vram[win_tile_data_bank][byte1_addr_in_bank_win];
                    let byte2_win = self.vram[win_tile_data_bank][byte2_addr_in_bank_win];

                    let color_num_win = (((byte2_win >> pixel_x_in_tile_bit_pos_win) & 1) << 1) | ((byte1_win >> pixel_x_in_tile_bit_pos_win) & 1);

                    let final_color_win: [u8; 3];
                    if self.system_mode == crate::bus::SystemMode::CGB {
                        let palette_ram_base_idx = (win_cgb_palette_idx as usize * 8) + (color_num_win as usize * 2);
                        if palette_ram_base_idx + 1 < self.cgb_bg_palette_ram.len() {
                            let color_byte1 = self.cgb_bg_palette_ram[palette_ram_base_idx];
                            let color_byte2 = self.cgb_bg_palette_ram[palette_ram_base_idx + 1];
                            let cgb_color_val = ((color_byte2 as u16) << 8) | (color_byte1 as u16);
                            final_color_win = cgb_color_to_rgb(cgb_color_val);
                        } else {
                            final_color_win = COLOR_WHITE; // Fallback
                        }
                    } else {
                        // DMG Window Rendering
                        let output_color_num = (self.bgp >> (color_num_win * 2)) & 0b11;
                        final_color_win = DMG_PALETTE[output_color_num as usize];
                    }

                    // Draw pixel, overwriting background
                    let fb_idx = (self.ly as usize * 160 + x_on_screen_win as usize) * 3;
                    if fb_idx + 2 < self.framebuffer.len() {
                        self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&final_color_win);
                    }
                }
            }
        } // This closes the Window rendering condition: if (self.lcdc & (1 << 5)) != 0 ...
    } // This closes the new main condition: if !bg_is_blank_dmg

        // Sprite Rendering
        // This part is outside the `if !bg_is_blank_dmg` block, so sprites always render if enabled by LCDC.1
        if (self.lcdc & (1 << 1)) != 0 { // OBJ Display Enable
            let obj_size_8x16 = (self.lcdc & (1 << 2)) != 0;
            let sprite_height = if obj_size_8x16 { 16 } else { 8 };
            let mut visible_sprites_on_line = 0;

            for oam_idx in (0..self.oam.len()).step_by(4) {
                if visible_sprites_on_line >= 10 { // Max 10 sprites per scanline
                    break;
                }

                let sprite_y_oam = self.oam[oam_idx];     // Y position on screen + 16
                let sprite_x_oam = self.oam[oam_idx + 1]; // X position on screen + 8
                let mut tile_index = self.oam[oam_idx + 2];
                let attributes = self.oam[oam_idx + 3];

                let x_flip = (attributes & (1 << 5)) != 0;
                let y_flip = (attributes & (1 << 6)) != 0;
                let palette_select_obp1 = (attributes & (1 << 4)) != 0;
                let sprite_has_priority_over_bg = (attributes & (1 << 7)) == 0;

                // Sprite Visibility & Y-Coordinate Check
                if sprite_y_oam == 0 || sprite_y_oam >= 160 { // Sprite is off-screen or effectively disabled (Y OAM coord is Y screen + 16. 144+16=160)
                    continue;
                }
                let screen_y_top = sprite_y_oam.saturating_sub(16);

                if self.ly >= screen_y_top && self.ly < screen_y_top + sprite_height {
                    // Sprite is on this scanline
                    if sprite_x_oam == 0 || sprite_x_oam >= 168 { // Sprite is off-screen horizontally (X OAM coord is X screen + 8. 160+8=168)
                        continue;
                    }
                    visible_sprites_on_line += 1;

                    let scanline_row_in_sprite = self.ly - screen_y_top; // 0 to sprite_height-1

                    // Adjust tile index for 8x16 sprites
                    if obj_size_8x16 {
                        if scanline_row_in_sprite < 8 { // Top tile
                            tile_index &= 0xFE; // Ignore LSB for top tile
                        } else { // Bottom tile
                            tile_index |= 0x01; // Set LSB for bottom tile
                        }
                    }

                    // Apply Y-flip
                    let mut tile_row_y_for_vram = if y_flip {
                        sprite_height - 1 - scanline_row_in_sprite
                    } else {
                        scanline_row_in_sprite
                    };
                    tile_row_y_for_vram %= 8; // Each tile part is 8 rows high

                    // Tile data for sprites is always fetched from 0x8000-0x8FFF tile area in VRAM
                    let mut sprite_tile_data_bank: usize = 0;
                    if self.system_mode == crate::bus::SystemMode::CGB {
                        sprite_tile_data_bank = ((attributes >> 3) & 1) as usize; // Bit 3 of OAM attributes selects VRAM bank
                    }

                    let tile_data_offset_in_bank_sprite = (tile_index as usize * 16); // Relative to 0x8000 in its bank
                    let tile_row_data_addr_in_bank_sprite = tile_data_offset_in_bank_sprite + (tile_row_y_for_vram as usize * 2);

                    if tile_row_data_addr_in_bank_sprite + 1 >= 8192 {
                        continue;
                    }

                    let byte1 = self.vram[sprite_tile_data_bank][tile_row_data_addr_in_bank_sprite];
                    let byte2 = self.vram[sprite_tile_data_bank][tile_row_data_addr_in_bank_sprite + 1];

                    let chosen_obp = if palette_select_obp1 { self.obp1 } else { self.obp0 }; // Still used for DMG mode

                    for x_in_sprite_tile in 0..8u8 { // Iterate 8 pixels horizontally for the sprite tile
                        let screen_pixel_x = sprite_x_oam.saturating_sub(8).wrapping_add(x_in_sprite_tile);

                        if screen_pixel_x >= 160 { // Pixel is off-screen horizontally
                            continue;
                        }

                        let bit_pos = if x_flip {
                            x_in_sprite_tile
                        } else {
                            7 - x_in_sprite_tile
                        };

                        let color_num_sprite = (((byte2 >> bit_pos) & 1) << 1) | ((byte1 >> bit_pos) & 1);

                        if color_num_sprite == 0 { // Color 0 is transparent for sprites
                            continue;
                        }

                        let final_sprite_color_rgb: [u8; 3];
                        if self.system_mode == crate::bus::SystemMode::CGB {
                            // CGB Sprite Rendering
                            let cgb_obj_palette_idx = attributes & 0x07; // Bits 0-2 for CGB palette number
                            let palette_ram_base_idx = (cgb_obj_palette_idx as usize * 8) + (color_num_sprite as usize * 2);

                            if palette_ram_base_idx + 1 < self.cgb_obj_palette_ram.len() {
                                let color_byte1 = self.cgb_obj_palette_ram[palette_ram_base_idx];
                                let color_byte2 = self.cgb_obj_palette_ram[palette_ram_base_idx + 1];
                                let cgb_color_val = ((color_byte2 as u16) << 8) | (color_byte1 as u16);
                                final_sprite_color_rgb = cgb_color_to_rgb(cgb_color_val);
                            } else {
                                final_sprite_color_rgb = COLOR_BLACK; // Fallback for bad CGB OBJ palette access
                            }
                        } else {
                            // DMG Sprite Rendering (already uses chosen_obp for OBP0/OBP1)
                            let output_color_num_from_chosen_obp = (chosen_obp >> (color_num_sprite * 2)) & 0b11;
                            final_sprite_color_rgb = DMG_PALETTE[output_color_num_from_chosen_obp as usize];
                        }

                        let bg_win_display_enabled = (self.lcdc & (1 << 0)) != 0;
                        let mut draw_sprite_pixel = true;

                        if bg_win_display_enabled {
                            if !sprite_has_priority_over_bg {
                                let fb_idx_check = (self.ly as usize * 160 + screen_pixel_x as usize) * 3;
                                if fb_idx_check + 2 < self.framebuffer.len() {
                                    let bg_pixel_color = [
                                        self.framebuffer[fb_idx_check],
                                        self.framebuffer[fb_idx_check + 1],
                                        self.framebuffer[fb_idx_check + 2],
                                    ];
                                    if bg_pixel_color != DMG_PALETTE[0] { // If BG is not white (color index 0)
                                        draw_sprite_pixel = false;
                                    }
                                }
                            }
                        }

                        if draw_sprite_pixel {
                            let fb_idx_draw = (self.ly as usize * 160 + screen_pixel_x as usize) * 3;
                            if fb_idx_draw + 2 < self.framebuffer.len() {
                                self.framebuffer[fb_idx_draw..fb_idx_draw + 3].copy_from_slice(&final_sprite_color_rgb);
                            }
                        }
                    }
                }
            }
        }
    }


    pub fn read_byte(&self, addr: u16) -> u8 {
        let mode = self.stat & 0b11; // Current PPU mode

        // OAM Read Restrictions
        if addr >= 0xFE00 && addr <= 0xFE9F {
            if mode == MODE_OAM_SCAN || mode == MODE_DRAWING {
                return 0xFF; // OAM not accessible during Mode 2 or 3
            }
        }

        // VRAM Read Restrictions
        if addr >= 0x8000 && addr <= 0x9FFF {
            if mode == MODE_DRAWING {
                return 0xFF; // VRAM not accessible during Mode 3
            }
        }

        match addr {
            0x8000..=0x9FFF => {
                // Already checked for VRAM restrictions above if mode == MODE_DRAWING
                let relative_addr = (addr - 0x8000) as usize;
                if self.system_mode == crate::bus::SystemMode::CGB {
                    let bank = self.vbk as usize;
                    self.vram[bank][relative_addr]
                } else {
                    self.vram[0][relative_addr] // DMG always uses bank 0
                }
            }
            0xFE00..=0xFE9F => {
                // Already checked for OAM restrictions above if mode == MODE_OAM_SCAN || mode == MODE_DRAWING
                let relative_addr = addr - 0xFE00;
                self.oam[relative_addr as usize]
            }
            0xFF40 => self.lcdc,
            0xFF41 => self.stat | 0x80,
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            0xFF4F => self.vbk | 0xFE, // VBK - CGB VRAM Bank Select (only bit 0 used, others read as 1)
            // CGB Palette Registers
            0xFF68 => self.bcps,
            0xFF69 => { // BCPD - Background Color Palette Data
                // Note: Reading BCPD should only occur if self.system_mode == SystemMode::CGB
                // However, for now, we allow the read even in DMG to simplify.
                // A real DMG would ignore or behave differently.
                let index = (self.bcps & 0x3F) as usize;
                if index < 64 { self.cgb_bg_palette_ram[index] } else { 0xFF }
            }
            0xFF6A => self.ocps,
            0xFF6B => { // OCPD - Object Color Palette Data
                // Note: Similar CGB mode consideration as BCPD.
                let index = (self.ocps & 0x3F) as usize;
                if index < 64 { self.cgb_obj_palette_ram[index] } else { 0xFF }
            }
            _ => {
                0xFF
            }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        let mode = self.stat & 0b11; // Current PPU mode

        // OAM Write Restrictions
        if addr >= 0xFE00 && addr <= 0xFE9F {
            if mode == MODE_OAM_SCAN || mode == MODE_DRAWING {
                return; // Ignore write to OAM during Mode 2 or 3
            }
        }

        // VRAM Write Restrictions
        if addr >= 0x8000 && addr <= 0x9FFF {
            if mode == MODE_DRAWING {
                return; // Ignore write to VRAM during Mode 3
            }
        }

        match addr {
            0x8000..=0x9FFF => {
                // Already checked for VRAM restrictions above if mode == MODE_DRAWING
                let relative_addr = (addr - 0x8000) as usize;
                if self.system_mode == crate::bus::SystemMode::CGB {
                    let bank = self.vbk as usize;
                    self.vram[bank][relative_addr] = value;
                } else {
                    self.vram[0][relative_addr] = value; // DMG always uses bank 0
                }
            }
            0xFE00..=0xFE9F => {
                // Already checked for OAM restrictions above if mode == MODE_OAM_SCAN || mode == MODE_DRAWING
                let relative_addr = addr - 0xFE00;
                self.oam[relative_addr as usize] = value;
            }
            0xFF40 => {
                let old_lcd_enabled = (self.lcdc & (1 << 7)) != 0;
                self.lcdc = value;
                let new_lcd_enabled = (self.lcdc & (1 << 7)) != 0;
                if old_lcd_enabled && !new_lcd_enabled {
                    self.ly = 0;
                    self.cycles = 0;
                    self.stat = (self.stat & !0b11) | MODE_HBLANK;
                }
            }
            0xFF41 => {
                self.stat = (value & 0b0111_1000) | (self.stat & 0b1000_0111);
                self.stat |= 0x80;
            }
            0xFF42 => self.scy = value,
            0xFF43 => self.scx = value,
            0xFF44 => {  }
            0xFF45 => {
                self.lyc = value;
                let _ = self.check_lyc_ly();
            }
            0xFF47 => self.bgp = value,
            0xFF48 => self.obp0 = value,
            0xFF49 => self.obp1 = value,
            0xFF4A => self.wy = value,
            0xFF4B => self.wx = value,
            0xFF4F => { // VBK - CGB VRAM Bank Select
                if self.system_mode == crate::bus::SystemMode::CGB {
                    self.vbk = value & 1; // Only bit 0 is writable
                }
                // In DMG mode, writes to VBK are ignored.
            }
            // CGB Palette Registers
            0xFF68 => { // BCPS - Background Color Palette Specification
                // Note: Writing BCPS should ideally only occur if self.system_mode == SystemMode::CGB
                self.bcps = value;
            }
            0xFF69 => { // BCPD - Background Color Palette Data
                // Note: Similar CGB mode consideration.
                // DMG hardware would ignore writes to 0xFF69.
                // For CGB, perform write if PPU not in mode 3? Some docs say these are accessible always.
                // For now, let's assume they are writable if address matches.
                let index = (self.bcps & 0x3F) as usize;
                if index < 64 {
                    self.cgb_bg_palette_ram[index] = value;
                }
                if (self.bcps & 0x80) != 0 { // Auto-increment BCPS if bit 7 is set
                    self.bcps = 0x80 | (((index + 1) & 0x3F) as u8);
                }
            }
            0xFF6A => { // OCPS - Object Color Palette Specification
                self.ocps = value;
            }
            0xFF6B => { // OCPD - Object Color Palette Data
                let index = (self.ocps & 0x3F) as usize;
                if index < 64 {
                    self.cgb_obj_palette_ram[index] = value;
                }
                if (self.ocps & 0x80) != 0 { // Auto-increment OCPS if bit 7 is set
                    self.ocps = 0x80 | (((index + 1) & 0x3F) as u8);
                }
            }
            _ => { }
        }
    }
}

#[cfg(test)]
mod ppu_tests;

#[allow(dead_code)]
pub fn save_framebuffer_to_png(
    framebuffer: &[u8],
    width: u32,
    height: u32,
    filepath: &str,
) -> Result<(), ImageError> {
    if framebuffer.len() != (width * height * 3) as usize {
        return Err(ImageError::Parameter(
            image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ),
        ));
    }
    match ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(width, height, framebuffer.to_vec()) {
        Some(image_buffer) => {
            image_buffer.save(filepath)
        }
        None => {
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
