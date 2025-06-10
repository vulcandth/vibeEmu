use crate::models::GameBoyModel;
use std::collections::VecDeque;

// DMG Palette Colors
pub const DMG_WHITE: [u8; 3] = [224, 248, 208];
pub const DMG_LIGHT_GRAY: [u8; 3] = [136, 192, 112];
pub const DMG_DARK_GRAY: [u8; 3] = [52, 104, 86];
pub const DMG_BLACK: [u8; 3] = [8, 24, 32];

// PPU Constants
pub const VRAM_SIZE_CGB: usize = 16 * 1024;
pub const VRAM_SIZE_DMG: usize = 8 * 1024;
pub const OAM_SIZE: usize = 160;
pub const CGB_PALETTE_RAM_SIZE: usize = 64;
pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;
pub const OAM_SCAN_CYCLES: u32 = 80;
pub const SCANLINE_CYCLES: u32 = 456;
// pub const VBLANK_LINES: u8 = 10; // Not directly used, TOTAL_LINES implies this
// pub const VBLANK_DURATION_CYCLES: u32 = SCANLINE_CYCLES * VBLANK_LINES as u32; // Not directly used
pub const TOTAL_LINES: u8 = 154;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PpuMode { HBlank = 0, VBlank = 1, OamScan = 2, Drawing = 3 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelSource { Background, Object { palette_register: u8 } }

#[derive(Debug, Clone, Copy, Default)]
pub struct PixelData { pub color_index: u8 }

#[derive(Debug)]
pub struct PixelFifo { pixels: VecDeque<PixelData>, max_size: usize }

impl PixelFifo {
    pub fn new(max_size: usize) -> Self { PixelFifo { pixels: VecDeque::with_capacity(max_size), max_size } }
    pub fn len(&self) -> usize { self.pixels.len() }
    #[allow(dead_code)] pub fn is_empty(&self) -> bool { self.pixels.is_empty() }
    pub fn is_full(&self) -> bool { self.pixels.len() >= self.max_size }
    pub fn push(&mut self, pixel_data: PixelData) -> Result<(), ()> {
        if self.is_full() { Err(()) } else { self.pixels.push_back(pixel_data); Ok(()) }
    }
    pub fn pop(&mut self) -> Option<PixelData> { self.pixels.pop_front() }
    #[allow(dead_code)] pub fn clear(&mut self) { self.pixels.clear(); }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetcherState { GetTile, GetDataLow, GetDataHigh, PushToFifo }

#[derive(Debug, Default, Clone, Copy)]
struct FetchedTileLineData { plane1_byte: u8, plane2_byte: u8, attributes: u8 }

pub struct Ppu {
    pub vram: Vec<u8>, pub oam: [u8; OAM_SIZE],
    pub cgb_background_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],
    pub cgb_sprite_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],
    pub lcdc: u8, pub stat: u8, pub scy: u8, pub scx: u8, pub ly: u8, pub lyc: u8,
    pub wy: u8, pub wx: u8, pub bgp: u8, pub obp0: u8, pub obp1: u8,
    pub vbk: u8, pub bcps_bcpi: u8, pub bcpd_bgpd: u8, pub ocps_ocpi: u8, pub ocpd_obpd: u8,
    pub current_mode: PpuMode, pub cycles_in_mode: u32,
    pub vblank_interrupt_requested: bool, pub stat_interrupt_requested: bool,
    pub just_entered_hblank: bool,
    pub framebuffer: Vec<u8>,
    pub current_scanline_color_indices: [u8; SCREEN_WIDTH],
    visible_oam_entries: Vec<OamEntryData>,
    line_sprite_data: Vec<FetchedSpritePixelLine>,
    current_scanline_pixel_source: [PixelSource; SCREEN_WIDTH],
    current_mode3_drawing_cycles: u32,
    actual_drawing_duration_current_line: u32,
    model: GameBoyModel,
    pub bg_fifo: PixelFifo, pub fetcher_state: FetcherState,
    pub fetcher_map_x: u8, pub fetcher_map_y: u8, pub fetcher_tile_number: u8,
    pub fetcher_tile_data_base_addr: u16, #[allow(dead_code)] pub fetcher_tile_attributes: u8,
    pub fetcher_tile_line_data_bytes: [u8; 2], pub fetcher_state_cycle: u8,
    pub fetcher_pixel_push_idx: u8, pub screen_x: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct OamEntryData { pub oam_index: usize, pub y_pos: u8, pub x_pos: u8, pub tile_index: u8, pub attributes: u8 }

#[derive(Debug, Clone)]
pub struct FetchedSpritePixelLine { pub x_pos: u8, pub attributes: u8, pub pixels: [u8; 8], pub oam_index: usize }

#[derive(Debug, Default, Clone, Copy)]
pub struct BgTileInfo { pub tile_number: u8, pub tile_data_addr_plane1: u16, pub tile_data_addr_plane2: u16, pub attributes_addr: u16, pub attributes: u8, pub tile_number_map_addr: u16 }

impl Ppu {
    pub fn new(model: GameBoyModel) -> Self {
        let vram_size = if model.is_cgb_family() { VRAM_SIZE_CGB } else { VRAM_SIZE_DMG };
        Ppu {
            vram: vec![0; vram_size], oam: [0; OAM_SIZE],
            cgb_background_palette_ram: [0; CGB_PALETTE_RAM_SIZE],
            cgb_sprite_palette_ram: [0; CGB_PALETTE_RAM_SIZE],
            lcdc: 0x91, stat: 0x80, scy: 0x00, scx: 0x00, ly: 0x00, lyc: 0x00,
            wy: 0x00, wx: 0x00, bgp: 0xFC, obp0: 0xFF, obp1: 0xFF,
            vbk: 0xFE, bcps_bcpi: 0x00, bcpd_bgpd: 0x00, ocps_ocpi: 0x00, ocpd_obpd: 0x00,
            current_mode: PpuMode::OamScan, cycles_in_mode: 0,
            vblank_interrupt_requested: false, stat_interrupt_requested: false,
            just_entered_hblank: false,
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT * 3],
            current_scanline_color_indices: [0; SCREEN_WIDTH],
            visible_oam_entries: Vec::with_capacity(10),
            line_sprite_data: Vec::with_capacity(10),
            current_scanline_pixel_source: [PixelSource::Background; SCREEN_WIDTH],
            current_mode3_drawing_cycles: 172,
            actual_drawing_duration_current_line: 0,
            model,
            bg_fifo: PixelFifo::new(16),
            fetcher_state: FetcherState::GetTile,
            fetcher_map_x: 0, fetcher_map_y: 0, fetcher_tile_number: 0,
            fetcher_tile_data_base_addr: 0, fetcher_tile_attributes: 0,
            fetcher_tile_line_data_bytes: [0; 2], fetcher_state_cycle: 0,
            fetcher_pixel_push_idx: 0, screen_x: 0,
        }
    }

    pub fn read_vram(&self, addr: u16) -> u8 {
        if (self.lcdc & 0x80) != 0 && self.current_mode == PpuMode::Drawing { return 0xFF; }
        let bank = if self.model.is_cgb_family() && (self.vbk & 0x01) == 1 { 1 } else { 0 };
        let index = (addr as usize - 0x8000) + (bank * VRAM_SIZE_DMG);
        if index < self.vram.len() { self.vram[index] } else { 0xFF }
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        if (self.lcdc & 0x80) != 0 && self.current_mode == PpuMode::Drawing { return; }
        let bank = if self.model.is_cgb_family() && (self.vbk & 0x01) == 1 { 1 } else { 0 };
        let index = (addr as usize - 0x8000) + (bank * VRAM_SIZE_DMG);
        if index < self.vram.len() { self.vram[index] = value; }
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        if (self.lcdc & 0x80) != 0 && (self.current_mode == PpuMode::OamScan || self.current_mode == PpuMode::Drawing) { return 0xFF; }
        self.oam[addr as usize - 0xFE00]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        if (self.lcdc & 0x80) != 0 && (self.current_mode == PpuMode::OamScan || self.current_mode == PpuMode::Drawing) { return; }
        self.oam[addr as usize - 0xFE00] = value;
    }

    pub fn write_oam_dma(&mut self, addr: u16, value: u8) {
        let oam_index = addr as usize - 0xFE00;
        if oam_index < OAM_SIZE { self.oam[oam_index] = value; }
    }

    fn reset_scanline_state(&mut self) {
        self.screen_x = 0;
        self.fetcher_map_y = self.scy.wrapping_add(self.ly);
        self.fetcher_map_x = self.scx;
        self.fetcher_state = FetcherState::GetTile;
        self.fetcher_state_cycle = 0;
        self.fetcher_pixel_push_idx = 0;
        self.bg_fifo.clear();
        self.actual_drawing_duration_current_line = 0;
        self.visible_oam_entries.clear();
        self.line_sprite_data.clear();
        self.current_scanline_color_indices = [0; SCREEN_WIDTH];
        self.current_scanline_pixel_source = [PixelSource::Background; SCREEN_WIDTH];
    }

    pub fn tick(&mut self, cpu_t_cycles: u32) {
        self.just_entered_hblank = false;

        if (self.lcdc & 0x80) == 0 {
            self.ly = 0; self.cycles_in_mode = 0; self.current_mode = PpuMode::HBlank;
            self.stat = (self.stat & 0xFC) | (PpuMode::HBlank as u8);
            if self.ly == self.lyc { self.stat |= 1 << 2; } else { self.stat &= !(1 << 2); }
            self.stat |= 0x80;
            self.vblank_interrupt_requested = false; self.stat_interrupt_requested = false;
            self.reset_scanline_state();
            return;
        }

        self.cycles_in_mode += cpu_t_cycles;
        let previous_stat_interrupt_line = self.eval_stat_interrupt_conditions();

        match self.current_mode {
            PpuMode::OamScan => {
                if self.cycles_in_mode >= OAM_SCAN_CYCLES {
                    self.visible_oam_entries.clear();
                    let sprite_height = if (self.lcdc & 0x04) != 0 { 16 } else { 8 };
                    for i in 0..40 {
                        if self.visible_oam_entries.len() >= 10 { break; }
                        let entry_addr = i * 4;
                        let y_pos = self.oam[entry_addr]; let x_pos = self.oam[entry_addr + 1];
                        let tile_idx = self.oam[entry_addr + 2]; let attributes = self.oam[entry_addr + 3];
                        let current_ly_plus_16 = self.ly.wrapping_add(16);
                        if current_ly_plus_16 >= y_pos && current_ly_plus_16 < y_pos.wrapping_add(sprite_height) && x_pos > 0 {
                            self.visible_oam_entries.push(OamEntryData { oam_index: i, y_pos, x_pos, tile_index: tile_idx, attributes });
                        }
                    }
                    self.visible_oam_entries.sort_unstable_by(|a, b| if a.x_pos != b.x_pos { a.x_pos.cmp(&b.x_pos) } else { a.oam_index.cmp(&b.oam_index) });
                    let base_cycles = 172; let penalty_per_sprite = 6;
                    let num_sprites = self.visible_oam_entries.len() as u32;
                    self.current_mode3_drawing_cycles = (base_cycles + num_sprites * penalty_per_sprite).max(172).min(289);

                    let cycles_spillover = self.cycles_in_mode.saturating_sub(OAM_SCAN_CYCLES); // Use saturating_sub
                    self.cycles_in_mode = cycles_spillover;
                    self.current_mode = PpuMode::Drawing;
                    self.stat = (self.stat & 0xF8) | (PpuMode::Drawing as u8);

                    self.line_sprite_data.clear();
                    if (self.lcdc & (1 << 1)) != 0 {
                        for oam_entry in &self.visible_oam_entries {
                            if let Some(fetched_data) = self.fetch_sprite_line_data(oam_entry, self.ly) {
                                self.line_sprite_data.push(fetched_data);
                            }
                        }
                    }
                }
            }
            PpuMode::Drawing => {
                if (self.lcdc & 0x01) != 0 && self.bg_fifo.len() <= 8 {
                    self.tick_fetcher();
                }

                if self.screen_x < SCREEN_WIDTH as u8 {
                    if let Some(pixel_data) = self.bg_fifo.pop() {
                        self.current_scanline_color_indices[self.screen_x as usize] = pixel_data.color_index;
                        self.current_scanline_pixel_source[self.screen_x as usize] = PixelSource::Background;
                        self.screen_x += 1;
                    }
                }

                let mut transition_to_hblank = false;
                if self.screen_x >= SCREEN_WIDTH as u8 {
                    self.actual_drawing_duration_current_line = self.cycles_in_mode;
                    transition_to_hblank = true;
                } else if self.cycles_in_mode >= self.current_mode3_drawing_cycles {
                    self.actual_drawing_duration_current_line = self.current_mode3_drawing_cycles;
                    transition_to_hblank = true;
                }

                if transition_to_hblank {
                    self.apply_dmg_palette_to_framebuffer();
                    let cycles_spillover = self.cycles_in_mode.saturating_sub(self.actual_drawing_duration_current_line);
                    self.cycles_in_mode = cycles_spillover;

                    self.current_mode = PpuMode::HBlank;
                    self.stat = (self.stat & 0xF8) | (PpuMode::HBlank as u8);
                    self.just_entered_hblank = true;
                }
            }
            PpuMode::HBlank => {
                let expected_hblank_cycles = SCANLINE_CYCLES
                    .saturating_sub(OAM_SCAN_CYCLES)
                    .saturating_sub(self.actual_drawing_duration_current_line);

                if self.cycles_in_mode >= expected_hblank_cycles {
                    let cycles_spillover = self.cycles_in_mode.saturating_sub(expected_hblank_cycles);
                    self.cycles_in_mode = cycles_spillover;

                    self.ly += 1;
                    if self.ly < SCREEN_HEIGHT as u8 {
                        self.reset_scanline_state();
                        self.current_mode = PpuMode::OamScan;
                        self.stat = (self.stat & 0xF8) | (PpuMode::OamScan as u8);
                    } else {
                        self.current_mode = PpuMode::VBlank;
                        self.stat = (self.stat & 0xF8) | (PpuMode::VBlank as u8);
                        self.vblank_interrupt_requested = true;
                    }
                }
            }
            PpuMode::VBlank => {
                if self.cycles_in_mode >= SCANLINE_CYCLES {
                    let cycles_spillover = self.cycles_in_mode.saturating_sub(SCANLINE_CYCLES);
                    self.cycles_in_mode = cycles_spillover;
                    self.ly += 1;

                    if self.ly == TOTAL_LINES {
                        self.ly = 0;
                        self.reset_scanline_state();
                        self.current_mode = PpuMode::OamScan;
                        self.stat = (self.stat & 0xF8) | (PpuMode::OamScan as u8);
                    }
                }
            }
        }

        if self.ly == self.lyc { self.stat |= 1 << 2; } else { self.stat &= !(1 << 2); }
        let current_stat_interrupt_line = self.eval_stat_interrupt_conditions();
        if !previous_stat_interrupt_line && current_stat_interrupt_line {
            self.stat_interrupt_requested = true;
        }
    }

    fn tick_fetcher(&mut self) {
        self.fetcher_state_cycle += 1;
        if self.fetcher_state_cycle < 2 { return; }
        self.fetcher_state_cycle = 0;

        let bg_tile_map_base_addr = if (self.lcdc >> 3) & 0x01 == 0 { 0x9800 } else { 0x9C00 };
        let bg_win_tile_data_select = (self.lcdc >> 4) & 0x01;
        let is_cgb = self.model.is_cgb_family();

        match self.fetcher_state {
            FetcherState::GetTile => {
                let tile_row_in_map = (self.fetcher_map_y / 8) as u16;
                let tile_col_in_map = (self.fetcher_map_x / 8) as u16;
                let tile_number_map_addr = bg_tile_map_base_addr + (tile_row_in_map * 32) + tile_col_in_map;
                self.fetcher_tile_number = self.read_vram_bank_agnostic(tile_number_map_addr, 0);
                self.fetcher_tile_attributes = if is_cgb { self.read_vram_bank_agnostic(tile_number_map_addr, 1) } else { 0 };
                if bg_win_tile_data_select == 1 { self.fetcher_tile_data_base_addr = 0x8000 + (self.fetcher_tile_number as u16 * 16); }
                else { self.fetcher_tile_data_base_addr = 0x9000u16.wrapping_add(((self.fetcher_tile_number as i8) as i16 * 16) as u16); }
                self.fetcher_state = FetcherState::GetDataLow;
            }
            FetcherState::GetDataLow => {
                let mut line_in_tile = (self.fetcher_map_y % 8) as u16;
                if is_cgb && (self.fetcher_tile_attributes & (1 << 2)) != 0 { line_in_tile = 7 - line_in_tile; }
                let tile_bytes_offset = line_in_tile * 2;
                let data_addr_plane1 = self.fetcher_tile_data_base_addr + tile_bytes_offset;
                let tile_data_vram_bank = if is_cgb { (self.fetcher_tile_attributes >> 3) & 0x01 } else { 0 };
                self.fetcher_tile_line_data_bytes[0] = self.read_vram_bank_agnostic(data_addr_plane1, tile_data_vram_bank);
                self.fetcher_state = FetcherState::GetDataHigh;
            }
            FetcherState::GetDataHigh => {
                let mut line_in_tile = (self.fetcher_map_y % 8) as u16;
                if is_cgb && (self.fetcher_tile_attributes & (1 << 2)) != 0 { line_in_tile = 7 - line_in_tile; }
                let tile_bytes_offset = line_in_tile * 2;
                let data_addr_plane2 = self.fetcher_tile_data_base_addr + tile_bytes_offset + 1;
                let tile_data_vram_bank = if is_cgb { (self.fetcher_tile_attributes >> 3) & 0x01 } else { 0 };
                self.fetcher_tile_line_data_bytes[1] = self.read_vram_bank_agnostic(data_addr_plane2, tile_data_vram_bank);
                self.fetcher_state = FetcherState::PushToFifo;
            }
            FetcherState::PushToFifo => {
                if self.bg_fifo.is_full() { return; }
                let pixels = self.decode_tile_line_to_pixels(
                    self.fetcher_tile_line_data_bytes[0], self.fetcher_tile_line_data_bytes[1],
                    self.fetcher_tile_attributes, is_cgb);
                let pixel_data = PixelData { color_index: pixels[self.fetcher_pixel_push_idx as usize] };
                if self.bg_fifo.push(pixel_data).is_ok() {
                    self.fetcher_pixel_push_idx += 1;
                    if self.fetcher_pixel_push_idx == 8 {
                        self.fetcher_pixel_push_idx = 0;
                        self.fetcher_map_x = self.fetcher_map_x.wrapping_add(8);
                        self.fetcher_state = FetcherState::GetTile;
                    }
                } else { return; }
            }
        }
    }

    fn eval_stat_interrupt_conditions(&self) -> bool {
        let mode_int_enabled = match self.current_mode {
            PpuMode::HBlank => (self.stat & (1<<3)) != 0, PpuMode::VBlank => (self.stat & (1<<4)) != 0,
            PpuMode::OamScan => (self.stat & (1<<5)) != 0, PpuMode::Drawing => false,
        };
        let lyc_int_enabled = (self.stat & (1<<6)) != 0;
        let lyc_coincidence = (self.stat & (1<<2)) != 0;
        (lyc_int_enabled && lyc_coincidence) || mode_int_enabled
    }

    pub fn clear_vblank_interrupt_request(&mut self) { self.vblank_interrupt_requested = false; }
    pub fn clear_stat_interrupt_request(&mut self) { self.stat_interrupt_requested = false; }

    pub fn write_stat(&mut self, value: u8) {
        let old_stat_int_line = self.eval_stat_interrupt_conditions();
        self.stat = (value & 0b01111000) | (self.stat & 0b10000111);
        let new_stat_int_line = self.eval_stat_interrupt_conditions();
        if !old_stat_int_line && new_stat_int_line { self.stat_interrupt_requested = true; }
    }

    fn read_vram_bank_agnostic(&self, addr: u16, bank: u8) -> u8 {
        if !self.model.is_cgb_family() && bank > 0 { return 0xFF; }
        let vram_bank_offset = if bank == 1 { VRAM_SIZE_DMG } else { 0 };
        let index = (addr as usize - 0x8000) + vram_bank_offset;
        if index < self.vram.len() { self.vram[index] } else { 0xFF }
    }

    fn fetch_sprite_line_data( &self, oam_entry: &OamEntryData, current_ly: u8,) -> Option<FetchedSpritePixelLine> {
        let sprite_height = if (self.lcdc & 0x04) != 0 { 16 } else { 8 };
        let y_on_screen_for_compare = current_ly.wrapping_add(16);
        if !(y_on_screen_for_compare >= oam_entry.y_pos && y_on_screen_for_compare < oam_entry.y_pos.wrapping_add(sprite_height)) { return None; }
        let mut line_in_sprite = y_on_screen_for_compare.wrapping_sub(oam_entry.y_pos);
        if (oam_entry.attributes & (1 << 6)) != 0 { line_in_sprite = (sprite_height - 1) - line_in_sprite; }
        let mut actual_tile_index = oam_entry.tile_index;
        if sprite_height == 16 {
            if line_in_sprite < 8 { actual_tile_index &= 0xFE; }
            else { actual_tile_index |= 0x01; line_in_sprite -= 8; }
        }
        let tile_vram_bank = if self.model.is_cgb_family() { (oam_entry.attributes >> 3) & 0x01 } else { 0 };
        let tile_data_start_addr = 0x8000u16.wrapping_add(actual_tile_index as u16 * 16);
        let tile_line_offset_bytes = line_in_sprite as u16 * 2;
        let plane1_byte_addr = tile_data_start_addr.wrapping_add(tile_line_offset_bytes);
        let plane2_byte_addr = plane1_byte_addr.wrapping_add(1);
        let plane1_byte = self.read_vram_bank_agnostic(plane1_byte_addr, tile_vram_bank);
        let plane2_byte = self.read_vram_bank_agnostic(plane2_byte_addr, tile_vram_bank);
        let pixels = self.decode_sprite_tile_line(plane1_byte, plane2_byte);
        Some(FetchedSpritePixelLine { x_pos: oam_entry.x_pos, attributes: oam_entry.attributes, pixels, oam_index: oam_entry.oam_index, })
    }

    #[allow(dead_code)]
    fn fetch_tile_line_data( &self, tile_map_addr_base: u16, tile_data_addr_base_select: u8, map_x: u8, map_y: u8,) -> FetchedTileLineData {
        let mut output = FetchedTileLineData::default();
        let is_cgb = self.model.is_cgb_family();
        let tile_row_in_map = (map_y / 8) as u16; let tile_col_in_map = (map_x / 8) as u16;
        let tile_number_map_addr = tile_map_addr_base + (tile_row_in_map * 32) + tile_col_in_map;
        let tile_number = self.read_vram_bank_agnostic(tile_number_map_addr, 0);
        let mut tile_data_vram_bank = 0;
        if is_cgb { output.attributes = self.read_vram_bank_agnostic(tile_number_map_addr, 1); tile_data_vram_bank = (output.attributes >> 3) & 0x01; }
        let tile_data_start_addr: u16 = if tile_data_addr_base_select == 1 { 0x8000 + (tile_number as u16 * 16) }
                                      else { 0x9000u16.wrapping_add(((tile_number as i8) as i16 * 16) as u16) };
        let mut line_offset_in_tile = (map_y % 8) as u16;
        if is_cgb && (output.attributes & (1 << 2)) != 0 { line_offset_in_tile = 7 - line_offset_in_tile; }
        let tile_bytes_offset = line_offset_in_tile * 2;
        output.plane1_byte = self.read_vram_bank_agnostic(tile_data_start_addr + tile_bytes_offset, tile_data_vram_bank);
        output.plane2_byte = self.read_vram_bank_agnostic(tile_data_start_addr + tile_bytes_offset + 1, tile_data_vram_bank);
        output
    }

    fn decode_tile_line_to_pixels( &self, plane1_byte: u8, plane2_byte: u8, cgb_attributes: u8, is_cgb: bool,) -> [u8; 8] {
        let mut pixels = [0u8; 8];
        let horizontal_flip = is_cgb && (cgb_attributes & (1 << 1)) != 0;
        for i in 0..8 {
            let bit_pos = if horizontal_flip { i } else { 7 - i };
            let lsb = (plane1_byte >> bit_pos) & 1;
            let msb = (plane2_byte >> bit_pos) & 1;
            pixels[i as usize] = (msb << 1) | lsb;
        }
        pixels
    }

    fn decode_sprite_tile_line(&self, plane1_byte: u8, plane2_byte: u8) -> [u8; 8] {
        let mut pixels = [0u8; 8];
        for i in 0..8 {
            let bit_pos = 7 - i;
            let lsb = (plane1_byte >> bit_pos) & 1;
            let msb = (plane2_byte >> bit_pos) & 1;
            pixels[i as usize] = (msb << 1) | lsb;
        }
        pixels
    }

    #[allow(dead_code)]
    fn render_scanline_bg(&mut self) { /* Dead code */ }

    fn apply_dmg_palette_to_framebuffer(&mut self) {
        if self.ly >= SCREEN_HEIGHT as u8 { return; }
        if self.model.is_cgb_family() {
            for x in 0..SCREEN_WIDTH {
                let color_index = self.current_scanline_color_indices[x];
                let shade_val = match color_index { 0 => DMG_WHITE, 1 => DMG_LIGHT_GRAY, 2 => DMG_DARK_GRAY, _ => DMG_BLACK, };
                let fb_idx = (self.ly as usize * SCREEN_WIDTH + x) * 3;
                if fb_idx + 2 < self.framebuffer.len() { self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&shade_val); }
            }
            return;
        }
        for x in 0..SCREEN_WIDTH {
            let color_index = self.current_scanline_color_indices[x];
            let pixel_source = self.current_scanline_pixel_source[x];
            let mut shade = 0;
            match pixel_source {
                PixelSource::Background => { shade = (self.bgp >> (color_index * 2)) & 0x03; }
                PixelSource::Object { palette_register } => {
                    if palette_register == 0 { shade = (self.obp0 >> (color_index * 2)) & 0x03; }
                    else { shade = (self.obp1 >> (color_index * 2)) & 0x03; }
                }
            }
            let rgb_color = match shade { 0 => DMG_WHITE, 1 => DMG_LIGHT_GRAY, 2 => DMG_DARK_GRAY, 3 => DMG_BLACK, _ => unreachable!(), };
            let fb_idx = (self.ly as usize * SCREEN_WIDTH + x) * 3;
            if fb_idx + 2 < self.framebuffer.len() { self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&rgb_color); }
        }
    }
}
