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
    pub vram: [u8; 8192],
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
}

impl Ppu {
    pub fn new() -> Self {
        let initial_ly = 0;
        let initial_lyc = 0;
        let mut initial_stat = MODE_OAM_SCAN;
        if initial_ly == initial_lyc {
            initial_stat |= 1 << 2;
        }
        initial_stat |= 1 << 7;


        Ppu {
            vram: [0; 8192],
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
        }
    }

    fn set_mode(&mut self, mode: u8) -> Option<InterruptType> {
        self.stat = (self.stat & 0b1111_1100) | mode;
        let mut stat_interrupt_to_request: Option<InterruptType> = None;
        match mode {
            MODE_HBLANK if (self.stat & (1 << 3)) != 0 => {
                stat_interrupt_to_request = Some(InterruptType::LcdStat);
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
            if self.ly != 0 {
                 self.ly = 0;
                 self.cycles = 0;
                 // When LCD is disabled, STAT mode bits are typically HBlank or VBlank.
                 // Pandocs says "Mode HBlank or Mode VBlank" - typically Mode HBlank.
                 self.stat = (self.stat & !0b11) | MODE_HBLANK;
            }
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

        if (self.lcdc & (1 << 0)) == 0 {
             for x_on_screen in 0..160 {
                let fb_idx = (self.ly as usize * 160 + x_on_screen as usize) * 3;
                 if fb_idx + 2 < self.framebuffer.len() {
                    self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&COLOR_WHITE);
                }
            }
            return;
        }

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
            let tile_index_addr_in_vram = (tile_map_base_addr + (tile_map_offset % 1024) - 0x8000) as usize;

            if tile_index_addr_in_vram >= self.vram.len() { continue; }
            let tile_index = self.vram[tile_index_addr_in_vram];

            let tile_data_start_addr_in_vram = if signed_tile_addressing {
                let base_offset = 0x1000;
                (base_offset as i16 + (tile_index as i8 as i16) * 16) as usize
            } else {
                (tile_data_base_addr - 0x8000 + (tile_index as u16) * 16) as usize
            };

            let pixel_y_in_tile = (map_y % 8) as usize;

            let byte1_vram_addr = tile_data_start_addr_in_vram + pixel_y_in_tile * 2;
            let byte2_vram_addr = byte1_vram_addr + 1;

            if byte2_vram_addr >= self.vram.len() {
                let fb_idx = (current_scanline_y as usize * 160 + x_on_screen as usize) * 3;
                if fb_idx + 2 < self.framebuffer.len() {
                     self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&[0xFF,0,0xFF]);
                }
                continue;
            }

            let byte1 = self.vram[byte1_vram_addr];
            let byte2 = self.vram[byte2_vram_addr];

            let pixel_x_in_tile_bit_pos = 7 - (map_x % 8) as u8;

            let color_bit1 = (byte1 >> pixel_x_in_tile_bit_pos) & 1;
            let color_bit0 = (byte2 >> pixel_x_in_tile_bit_pos) & 1;
            let color_num = (color_bit0 << 1) | color_bit1;

            let output_color_num = (self.bgp >> (color_num * 2)) & 0b11;
            let final_color = DMG_PALETTE[output_color_num as usize];

            let fb_idx = (current_scanline_y as usize * 160 + x_on_screen as usize) * 3;
            if fb_idx + 2 < self.framebuffer.len() {
                self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&final_color);
            }
        }
    }


    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                let relative_addr = addr - 0x8000;
                self.vram[relative_addr as usize]
            }
            0xFE00..=0xFE9F => {
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
            _ => {
                0xFF
            }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9FFF => {
                let relative_addr = addr - 0x8000;
                self.vram[relative_addr as usize] = value;
            }
            0xFE00..=0xFE9F => {
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
            _ => { }
        }
    }
}

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
