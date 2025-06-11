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
pub const FAILSAFE_MAX_MODE3_CYCLES: u32 = 340; // Max cycles for Mode 3 before force quit if screen_x stuck

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

// struct FetchedTileLineData removed as it's unused
// struct FetchedTileLineData { plane1_byte: u8, plane2_byte: u8, attributes: u8 }

pub struct Ppu {
    pub vram: Vec<u8>, pub oam: [u8; OAM_SIZE],
    pub cgb_background_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],
    pub cgb_sprite_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],
    pub lcdc: u8, pub stat: u8, pub scy: u8, pub scx: u8, pub ly: u8, pub lyc: u8,
    pub wy: u8, pub wx: u8, pub bgp: u8, pub obp0: u8, pub obp1: u8,
    pub vbk: u8, pub bcps_bcpi: u8, /* pub bcpd_bgpd: u8, */ pub ocps_ocpi: u8, /* pub ocpd_obpd: u8, */ // bcpd_bgpd and ocpd_obpd removed
    pub current_mode: PpuMode, pub cycles_in_mode: u32,
    pub vblank_interrupt_requested: bool, pub stat_interrupt_requested: bool,
    pub just_entered_hblank: bool,
    pub framebuffer: Vec<u8>,
    pub current_scanline_color_indices: [u8; SCREEN_WIDTH],
    // For CGB mode, store the BG palette index (0-7) for each BG pixel on the line
    current_scanline_cgb_bg_palette_indices: [u8; SCREEN_WIDTH],
    visible_oam_entries: Vec<OamEntryData>,
    line_sprite_data: Vec<FetchedSpritePixelLine>,
    current_scanline_pixel_source: [PixelSource; SCREEN_WIDTH], // Still used for DMG OBJ vs BG priority
    current_mode3_drawing_cycles: u32,
    actual_drawing_duration_current_line: u32,
    model: GameBoyModel,
    pub bg_fifo: PixelFifo, pub fetcher_state: FetcherState,
    pub fetcher_map_x: u8, pub fetcher_map_y: u8, pub fetcher_tile_number: u8,
    pub fetcher_tile_data_base_addr: u16, #[allow(dead_code)] pub fetcher_tile_attributes: u8,
    pub fetcher_tile_line_data_bytes: [u8; 2], pub fetcher_state_cycle: u8,
    pub fetcher_pixel_push_idx: u8, pub screen_x: u8,
    pixels_to_discard_at_line_start: u8,
    // Sprite rendering fields
    sprite_fifo: PixelFifo,
    current_sprite_to_render_idx: Option<usize>,
    #[allow(dead_code)] // May not be fully used by placeholder, but good for future
    window_internal_line_counter: u8,
    sprite_fetcher_pixel_push_idx: u8,
    sprite_pixels_loaded_into_fifo: bool,
    // Window rendering specific fields
    fetcher_is_for_window: bool,
    window_rendering_begun_for_line: bool,
    pixels_to_discard_for_window_line_start: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct OamEntryData { pub oam_index: usize, pub y_pos: u8, pub x_pos: u8, pub tile_index: u8, pub attributes: u8 }

#[derive(Debug, Clone)]
pub struct FetchedSpritePixelLine { pub x_pos: u8, pub attributes: u8, pub pixels: [u8; 8] } // oam_index removed

// BgTileInfo struct removed as it's unused

impl Ppu {
    pub fn new(model: GameBoyModel) -> Self {
        let vram_size = if model.is_cgb_family() { VRAM_SIZE_CGB } else { VRAM_SIZE_DMG };
        Ppu {
            vram: vec![0; vram_size], oam: [0; OAM_SIZE],
            cgb_background_palette_ram: [0; CGB_PALETTE_RAM_SIZE], // Initialized to 0s, boot ROM does FF. TODO: Verify if this should be FF.
            cgb_sprite_palette_ram: [0; CGB_PALETTE_RAM_SIZE],   // Initialized to 0s, boot ROM does FF.
            lcdc: 0x91, stat: 0x80, scy: 0x00, scx: 0x00, ly: 0x00, lyc: 0x00,
            wy: 0x00, wx: 0x00, bgp: 0xFC, obp0: 0xFF, obp1: 0xFF,
            vbk: 0xFE, bcps_bcpi: 0x00, /* bcpd_bgpd: 0x00, */ ocps_ocpi: 0x00, /* ocpd_obpd: 0x00, */ // bcpd_bgpd and ocpd_obpd removed
            current_mode: PpuMode::OamScan, cycles_in_mode: 0,
            vblank_interrupt_requested: false, stat_interrupt_requested: false,
            just_entered_hblank: false,
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT * 3],
            current_scanline_color_indices: [0; SCREEN_WIDTH],
            current_scanline_cgb_bg_palette_indices: [0; SCREEN_WIDTH], // Initialize CGB palette indices
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
            pixels_to_discard_at_line_start: 0,
            // Sprite rendering fields
            sprite_fifo: PixelFifo::new(8), // Max 8 pixels for one sprite
            current_sprite_to_render_idx: None,
            window_internal_line_counter: 0,
            sprite_fetcher_pixel_push_idx: 0,
            sprite_pixels_loaded_into_fifo: false,
            // Initialize new window fields
            fetcher_is_for_window: false,
            window_rendering_begun_for_line: false,
            pixels_to_discard_for_window_line_start: 0,
        }
    }

    // CGB Palette Access Methods
    pub fn read_bcps_bcpi(&self) -> u8 {
        self.bcps_bcpi
    }

    pub fn write_bcps_bcpi(&mut self, value: u8) {
        if self.model.is_cgb_family() {
            self.bcps_bcpi = value & 0xBF;
        }
    }

    pub fn read_bcpd(&self) -> u8 {
        if !self.model.is_cgb_family() { return 0xFF; }
        if (self.lcdc & 0x80) != 0 && (self.current_mode == PpuMode::Drawing || self.current_mode == PpuMode::OamScan) {
           return 0xFF;
        }
        let index = (self.bcps_bcpi & 0x3F) as usize;
        self.cgb_background_palette_ram[index]
    }

    pub fn write_bcpd(&mut self, value: u8) {
        if !self.model.is_cgb_family() { return; }
        if (self.lcdc & 0x80) != 0 && (self.current_mode == PpuMode::Drawing || self.current_mode == PpuMode::OamScan) {
           return;
        }
        let index = (self.bcps_bcpi & 0x3F) as usize;
        self.cgb_background_palette_ram[index] = value;
        if self.bcps_bcpi & 0x80 != 0 {
            let next_index = (index + 1) & 0x3F;
            self.bcps_bcpi = (self.bcps_bcpi & 0x80) | (next_index as u8);
        }
    }

    pub fn read_ocps_ocpi(&self) -> u8 {
        self.ocps_ocpi
    }

    pub fn write_ocps_ocpi(&mut self, value: u8) {
        if self.model.is_cgb_family() {
            self.ocps_ocpi = value & 0xBF;
        }
    }

    pub fn read_ocpd(&self) -> u8 {
        if !self.model.is_cgb_family() { return 0xFF; }
        if (self.lcdc & 0x80) != 0 && (self.current_mode == PpuMode::Drawing || self.current_mode == PpuMode::OamScan) {
           return 0xFF;
        }
        let index = (self.ocps_ocpi & 0x3F) as usize;
        self.cgb_sprite_palette_ram[index]
    }

    pub fn write_ocpd(&mut self, value: u8) {
        if !self.model.is_cgb_family() { return; }
        if (self.lcdc & 0x80) != 0 && (self.current_mode == PpuMode::Drawing || self.current_mode == PpuMode::OamScan) {
           return;
        }
        let index = (self.ocps_ocpi & 0x3F) as usize;
        self.cgb_sprite_palette_ram[index] = value;
        if self.ocps_ocpi & 0x80 != 0 {
            let next_index = (index + 1) & 0x3F;
            self.ocps_ocpi = (self.ocps_ocpi & 0x80) | (next_index as u8);
        }
    }
    // End CGB Palette Access Methods

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
        self.pixels_to_discard_at_line_start = self.scx % 8;
        self.bg_fifo.clear();
        self.sprite_fifo.clear();
        self.current_sprite_to_render_idx = None;
        self.sprite_fetcher_pixel_push_idx = 0;
        self.sprite_pixels_loaded_into_fifo = false;
        self.actual_drawing_duration_current_line = 0;
        self.visible_oam_entries.clear();
        self.line_sprite_data.clear();
        self.current_scanline_color_indices = [0; SCREEN_WIDTH];
        self.current_scanline_pixel_source = [PixelSource::Background; SCREEN_WIDTH];
        self.current_scanline_cgb_bg_palette_indices = [0; SCREEN_WIDTH]; // Reset CGB attributes for the line

        // Initialize window-related scanline state
        self.fetcher_is_for_window = false;
        self.window_rendering_begun_for_line = false;
        self.pixels_to_discard_for_window_line_start = 0;
    }

    pub fn tick(&mut self, cpu_t_cycles: u32) {
        self.just_entered_hblank = false;

        if (self.lcdc & 0x80) == 0 {
            self.ly = 0; self.cycles_in_mode = 0; self.current_mode = PpuMode::HBlank;
            self.stat = (self.stat & 0xFC) | (PpuMode::HBlank as u8);
            if self.ly == self.lyc { self.stat |= 1 << 2; } else { self.stat &= !(1 << 2); }
            self.stat |= 0x80;
            self.vblank_interrupt_requested = false; self.stat_interrupt_requested = false;
            self.reset_scanline_state(); // Resets screen_x, fetcher states, fifos
            // If LCD turns off, window counter should also reset for a clean start.
            self.window_internal_line_counter = 0;
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
                // Window Activation Logic (before pixel processing and fetcher ticks for the current screen_x)
                let window_display_enabled = (self.lcdc & (1 << 5)) != 0; // LCDC Bit 5
                let window_is_on_current_line = self.ly >= self.wy;
                let window_start_x_coord = self.wx.saturating_sub(7);

                if window_display_enabled && window_is_on_current_line &&
                   !self.window_rendering_begun_for_line && self.screen_x >= window_start_x_coord {

                    // Check if BG/Window master switch is on, otherwise window doesn't really "activate" over BG
                    if (self.lcdc & 0x01) != 0 { // LCDC Bit 0 - BG/Window Display Enable
                        self.fetcher_is_for_window = true;
                        self.window_rendering_begun_for_line = true;
                        self.bg_fifo.clear();
                        self.fetcher_state = FetcherState::GetTile;
                        self.fetcher_state_cycle = 0; // Allow fetcher to run immediately for the new context
                        self.fetcher_pixel_push_idx = 0;
                        // fetcher_map_y for window is the number of lines *into* the window we are.
                        self.fetcher_map_y = self.window_internal_line_counter;
                        // fetcher_map_x needs to be the tile column index for the window tile map.
                        // window_start_x_coord is the screen x where the window begins.
                        // We need to find the first tile column in the window's tile map that corresponds to this.
                        // Example: wx=7 (window_start_x_coord=0). fetcher_map_x should be 0.
                        // wx=10 (window_start_x_coord=3). fetcher_map_x should be 0.
                        // wx=15 (window_start_x_coord=8). fetcher_map_x should be 8.
                        // This implies map_x is aligned to the tile grid based on where fetching starts.
                        // The crucial part is that fetcher_map_x is used to calculate tile_col_in_map later.
                        // For the window, the "map" effectively starts at x=0 from wx-7.
                        // So, if window_start_x_coord is 3, we fetch tile 0, but discard 3 pixels.
                        // self.fetcher_map_x = (self.screen_x / 8) * 8; // This would be if window started at screen_x
                        // The fetcher_map_x should be relative to the window's own coordinate system for its tile map.
                        // If window starts at wx=7 (screen_x=0), fetcher_map_x is 0.
                        // If window starts at wx=10 (screen_x=3), fetcher_map_x is still 0 for the first tile.
                        // The pixels to discard handle the offset *within* the first tile.
                        self.fetcher_map_x = 0; // Window's map always starts at its column 0.
                                                // Discard logic will handle the initial pixel offset.

                        self.pixels_to_discard_for_window_line_start = window_start_x_coord % 8;

                        // The current fetcher might have already fetched some BG tiles for this scanline.
                        // If the window starts mid-scanline, those prior BG pixels are used up to window_start_x_coord.
                        // Then, the fetcher switches to window mode.
                    }
                }

                // Sprite Data Loading Logic
                if (self.lcdc & (1 << 1)) != 0 { // OBJ display enabled
                    if self.current_sprite_to_render_idx.is_none() {
                        for (idx, sprite_data) in self.line_sprite_data.iter().enumerate() {
                            // sprite_data.x_pos is screen coordinate (1-168, 0 means off-screen)
                            // We start loading when screen_x is at or past where the sprite begins (x_pos - 8)
                            if sprite_data.x_pos > 0 && self.screen_x >= sprite_data.x_pos.saturating_sub(8) {
                                self.current_sprite_to_render_idx = Some(idx);
                                self.sprite_pixels_loaded_into_fifo = false;
                                self.sprite_fetcher_pixel_push_idx = 0;
                                self.sprite_fifo.clear(); // Clear for the new sprite
                                break;
                            }
                        }
                    }

                    if let Some(sprite_idx) = self.current_sprite_to_render_idx {
                        if !self.sprite_pixels_loaded_into_fifo {
                            let sprite_data = &self.line_sprite_data[sprite_idx];
                            // Condition to ensure we are in the range to load this sprite.
                            // x_pos is 1-indexed effectively for screen coordinates.
                            // An x_pos of 8 means the first pixel is at screen_x = 0.
                            // An x_pos of 1 means the first pixel is at screen_x = -7 (off-screen).
                            if sprite_data.x_pos > 0 && self.screen_x >= sprite_data.x_pos.saturating_sub(8) {
                                while self.sprite_fetcher_pixel_push_idx < 8 && !self.sprite_fifo.is_full() {
                                    let pixel_color_index = sprite_data.pixels[self.sprite_fetcher_pixel_push_idx as usize];
                                    // For DMG, non-zero color index means visible pixel.
                                    // PixelData only stores color_index. Palette is handled at mix time.
                                    let pixel_data = PixelData { color_index: pixel_color_index };
                                    // Horizontal flip for sprites is handled when decoding pixels in fetch_sprite_line_data
                                    // So, pixels array is already in correct screen order.
                                    if self.sprite_fifo.push(pixel_data).is_ok() {
                                        self.sprite_fetcher_pixel_push_idx += 1;
                                    } else {
                                        break; // FIFO full, wait for next cycle
                                    }
                                }
                                if self.sprite_fetcher_pixel_push_idx == 8 {
                                    self.sprite_pixels_loaded_into_fifo = true;
                                }
                            }
                        }
                    }
                }

                // Pixel Popping and Mixing Logic
                let mut bg_pixel_data: Option<PixelData> = None;
                let mut sprite_pixel_data: Option<PixelData> = None;
                let mut final_pixel_data: Option<PixelData> = None;
                let mut final_pixel_source: PixelSource = PixelSource::Background; // Default

                let master_bg_win_enable = (self.lcdc & 0x01) != 0; // LCDC Bit 0

                // Determine if fetcher should run for BG/Window
                // Fetcher runs if:
                // 1. BG/Window display is enabled (master_bg_win_enable)
                // 2. AND (either fetcher is for window OR it's for BG and window is not currently covering this spot)
                // This logic is simplified because the fetcher_is_for_window flag handles the switch.
                // If master_bg_win_enable is false, the fetcher effectively doesn't run for output,
                // but it might still need to be ticked to keep it "warm" if it were to be enabled mid-scanline (not typical).
                // For simplicity here, we gate ticking based on master_bg_win_enable.
                if master_bg_win_enable {
                    if !self.bg_fifo.is_full() { // Fetch if there's any space.
                        self.tick_fetcher();
                    }
                }


                // Pixel Output Logic (incorporating master_bg_win_enable)
                if self.screen_x < SCREEN_WIDTH as u8 {
                    if !master_bg_win_enable {
                        // LCDC Bit 0 is off: BG and Window are replaced by white
                        self.current_scanline_color_indices[self.screen_x as usize] = 0; // White
                        self.current_scanline_pixel_source[self.screen_x as usize] = PixelSource::Background; // Uses BGP0
                        self.screen_x += 1;
                        // Sprite processing still happens below, over this white background
                    } else {
                        // master_bg_win_enable is true, so try to get BG/Window pixel
                        // self.fetcher_is_for_window determines if FIFO contains BG or Window pixels
                        bg_pixel_data = self.bg_fifo.pop();
                        // If bg_pixel_data is None here, it means FIFO is empty, and PPU should stall for BG/Win.
                        // The advancement of screen_x will be handled inside the sprite mixing logic if a pixel is available.
                    }
                }


                // Try to pop Sprite pixel (this logic can remain largely the same)
                if (self.lcdc & (1 << 1)) != 0 { // OBJ display enabled
                    if let Some(sprite_idx) = self.current_sprite_to_render_idx {
                        let sprite_data = &self.line_sprite_data[sprite_idx];
                        // Only pop if screen_x is at or past the sprite's first visible pixel column
                        // and the sprite is not completely off-screen to the left (x_pos > 0)
                        // and sprite pixels have been loaded.
                        if sprite_data.x_pos > 0 && self.screen_x >= sprite_data.x_pos.saturating_sub(8) && self.sprite_pixels_loaded_into_fifo {
                            // Also check if sprite is "active" for this specific screen_x
                            // A sprite at x_pos=8 is visible from screen_x=0 to screen_x=7
                            // A sprite at x_pos=X is visible from screen_x=X-8 to screen_x=X-1
                            if self.screen_x < sprite_data.x_pos { // screen_x is within the 8 pixels of the sprite
                                sprite_pixel_data = self.sprite_fifo.pop();
                                if sprite_pixel_data.is_some() && sprite_data.x_pos == 0 { // This check seems redundant due to x_pos > 0 above
                                    sprite_pixel_data = None; // Effectively hide sprite if x_pos somehow became 0
                                }
                            } else {
                                // screen_x is past the current sprite's range, it should have been consumed or reset.
                                // This can happen if the BG FIFO was empty and we advanced screen_x with sprite pixels,
                                // then the sprite finished.
                            }
                        }
                    }
                }


                // Mixing Decision
                let s_pixel_opt = sprite_pixel_data;
                let b_pixel_opt = bg_pixel_data;

                if s_pixel_opt.is_some() && s_pixel_opt.as_ref().unwrap().color_index != 0 { // Sprite pixel exists and is not transparent
                    let sprite_data = &self.line_sprite_data[self.current_sprite_to_render_idx.unwrap()];
                    let sprite_attr = sprite_data.attributes;
                    let sprite_has_priority = (sprite_attr & (1 << 7)) == 0; // DMG: 0=OBJ Above BG, 1=OBJ Behind BG
                    let dmg_palette_reg = if (sprite_attr & (1 << 4)) != 0 { 1 } else { 0 }; // Bit 4: OBP1 if set, OBP0 if clear

                    if sprite_has_priority || (b_pixel_opt.is_some() && b_pixel_opt.as_ref().unwrap().color_index == 0) { // Sprite is above BG or BG is transparent
                        final_pixel_data = s_pixel_opt;
                        final_pixel_source = PixelSource::Object { palette_register: dmg_palette_reg };
                    } else if b_pixel_opt.is_some() { // Sprite is behind non-transparent BG
                        final_pixel_data = b_pixel_opt;
                        final_pixel_source = PixelSource::Background;
                    } else { // BG pixel is None (e.g. BG disabled or FIFO empty), and sprite is behind BG.
                             // Treat as if BG was transparent color 0.
                        final_pixel_data = s_pixel_opt; // Sprite shows over "nothing"
                        final_pixel_source = PixelSource::Object { palette_register: dmg_palette_reg };
                    }
                } else if b_pixel_opt.is_some() { // No sprite pixel (or transparent sprite), use BG
                    final_pixel_data = b_pixel_opt;
                    final_pixel_source = PixelSource::Background;
                } else if s_pixel_opt.is_some() && s_pixel_opt.as_ref().unwrap().color_index == 0 { // Transparent sprite over no BG
                     final_pixel_data = Some(PixelData { color_index: 0 }); // Show background color 0
                     final_pixel_source = PixelSource::Background;
                }
                // If both are None, final_pixel_data remains None. Pushing to framebuffer should handle None.

                // Revised Pixel Output / screen_x advancement logic
                if self.screen_x < SCREEN_WIDTH as u8 {
                    if !master_bg_win_enable {
                        // BG/Window are off (replaced by white earlier). Sprites can still render on top.
                        if s_pixel_opt.is_some() && s_pixel_opt.as_ref().unwrap().color_index != 0 { // Sprite pixel is not transparent
                            // Only sprite matters now. BG is white (color index 0, source Background)
                            let sprite_data_for_mix = &self.line_sprite_data[self.current_sprite_to_render_idx.unwrap()];
                            let dmg_palette_reg = if (sprite_data_for_mix.attributes & (1 << 4)) != 0 { 1 } else { 0 };
                            final_pixel_data = s_pixel_opt;
                            final_pixel_source = PixelSource::Object { palette_register: dmg_palette_reg };
                        } else { // No sprite, or transparent sprite. BG is white.
                            final_pixel_data = Some(PixelData { color_index: 0 }); // White from disabled BG/Win
                            final_pixel_source = PixelSource::Background;
                        }
                        // screen_x was already advanced when master_bg_win_enable was false.
                        // Here we just determine the final pixel color if a sprite overlaps.
                        self.current_scanline_color_indices[self.screen_x as usize -1] = final_pixel_data.as_ref().unwrap().color_index; // screen_x already advanced
                        self.current_scanline_pixel_source[self.screen_x as usize -1] = final_pixel_source;

                        // Sprite consumption logic (copied and adapted for this path)
                        if s_pixel_opt.is_some() {
                            if self.sprite_fifo.is_empty() && self.sprite_pixels_loaded_into_fifo {
                                if let Some(sprite_idx) = self.current_sprite_to_render_idx {
                                    let sprite_data = &self.line_sprite_data[sprite_idx];
                                    if self.screen_x >= sprite_data.x_pos { // screen_x is already advanced
                                        self.current_sprite_to_render_idx = None;
                                        self.sprite_pixels_loaded_into_fifo = false;
                                        self.sprite_fetcher_pixel_push_idx = 0;
                                    }
                                }
                            }
                        }

                    } else if final_pixel_data.is_some() { // master_bg_win_enable is true AND we have a mixed pixel (from BG/Win + Sprite)
                        let final_idx = self.screen_x as usize;
                        self.current_scanline_color_indices[final_idx] = final_pixel_data.as_ref().unwrap().color_index;
                        self.current_scanline_pixel_source[final_idx] = final_pixel_source;

                        // Store CGB BG palette index if the final pixel is from BG/Window
                        if self.model.is_cgb_family() {
                            match final_pixel_source {
                                PixelSource::Background => {
                                    // fetcher_tile_attributes contains the attributes for the BG/Window tile
                                    // that produced this pixel. Bits 0-2 are the palette number.
                                    self.current_scanline_cgb_bg_palette_indices[final_idx] = self.fetcher_tile_attributes & 0x07;
                                }
                                PixelSource::Object { .. } => {
                                    // If an object pixel wins, BG palette index is not directly applicable here.
                                    // Could store a default or a marker if needed for debugging.
                                    // For now, let's assume it might carry over from a BG pixel that was previously there,
                                    // or default to 0 if sprite is over "nothing".
                                    // This depends on exact sprite priority interaction with this array.
                                    // Safest for now if OBJ wins: don't update or set to a known "OBJ" marker if needed.
                                    // However, if the goal is for BG pixels to *always* have their CGB palette stored
                                    // regardless of OBJ priority, this needs to be recorded when b_pixel_opt is processed.
                                    // Let's try recording it when b_pixel_opt is confirmed.
                                }
                            }
                        }

                        self.screen_x += 1;

                        // Sprite consumption logic (original position)
                        if s_pixel_opt.is_some() {
                            if self.sprite_fifo.is_empty() && self.sprite_pixels_loaded_into_fifo {
                                if let Some(sprite_idx) = self.current_sprite_to_render_idx {
                                    let sprite_data = &self.line_sprite_data[sprite_idx];
                                    if self.screen_x >= sprite_data.x_pos {
                                        self.current_sprite_to_render_idx = None;
                                        self.sprite_pixels_loaded_into_fifo = false;
                                        self.sprite_fetcher_pixel_push_idx = 0;
                                    }
                                }
                            }
                        }
                    } else if bg_pixel_data.is_none() && master_bg_win_enable {
                        // Master BG/Win is enabled, but bg_fifo was empty. PPU stalls. screen_x does not advance.
                        // Sprite FIFO might have pixels, but we wait for BG/Win pixel first.
                        eprintln!("PPU STALL: LY={}, screen_x={}, fifo_len={}, fetcher_state={:?}, cycles_in_mode={}", self.ly, self.screen_x, self.bg_fifo.len(), self.fetcher_state, self.cycles_in_mode);
                    }
                    // The case for "(self.lcdc & (1 << 1)) == 0 && master_bg_win_enable && bg_pixel_data.is_some() && s_pixel_opt.is_none()"
                    // (i.e., OBJ disabled, BG/Win enabled, BG pixel available, no sprite)
                    // is naturally handled by the final_pixel_data.is_some() block, as final_pixel_data would be the bg_pixel_data.
                }


                let mut transition_to_hblank = false;
                if self.screen_x >= SCREEN_WIDTH as u8 {
                    // Normal completion by drawing all pixels
                    self.actual_drawing_duration_current_line = self.cycles_in_mode; // Record actual cycles spent
                    transition_to_hblank = true;
                } else if self.cycles_in_mode >= FAILSAFE_MAX_MODE3_CYCLES {
                    // Failsafe: screen_x is stuck, and we've exceeded a generous cycle limit.
                    self.actual_drawing_duration_current_line = self.cycles_in_mode; // Record actual cycles spent
                    eprintln!("PPU FAILSAFE: LY={}, screen_x={}, cycles_in_mode={}", self.ly, self.screen_x, self.cycles_in_mode);
                    transition_to_hblank = true;
                    // eprintln!("PPU Warning: Mode 3 force quit at LY={}, screen_x={}, cycles={}", self.ly, self.screen_x, self.cycles_in_mode);
                }

                if transition_to_hblank {
                    self.apply_palette_to_framebuffer_line();
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

                    // Manage window_internal_line_counter for the *next* line's context (which starts after this HBlank)
                    // This counter should increment for each scanline the window is visible on.
                    if (self.lcdc & (1 << 5)) != 0 && self.ly >= self.wy && self.ly < (SCREEN_HEIGHT as u8).saturating_sub(1) {
                        // If window is enabled, and current LY was part of the window (and not the very last visible line)
                        // Condition for incrementing: current line is WY, or (current line > WY AND previous line was not WY start)
                        // This logic aims to increment for each line the window is displayed on.
                        // It resets to 0 if the window becomes active on this line (self.ly == self.wy).
                        // Or if the window became active on a previous line and this is a continuation.
                        if self.ly == self.wy { // First line of window
                            self.window_internal_line_counter = 0;
                        } else { // Subsequent lines of window
                             // Check if previous line was still in window, to avoid incrementing if WY changed mid-frame making this the new "first" line
                            if self.ly > self.wy && self.ly.saturating_sub(1) >= self.wy { // ensure ly-1 didn't wrap and was also >= wy
                                self.window_internal_line_counter = self.window_internal_line_counter.saturating_add(1);
                            } else { // This implies ly > wy but ly-1 < wy, meaning WY must have changed to be self.ly. So treat as first line.
                                self.window_internal_line_counter = 0;
                            }
                        }
                    } // If window not enabled, or LY not in window range, counter state persists or is reset at frame boundary/WY change by other logic.


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
                        self.window_internal_line_counter = 0; // Reset for new frame
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

        //let bg_tile_map_base_addr = if (self.lcdc >> 3) & 0x01 == 0 { 0x9800 } else { 0x9C00 }; // Original BG only
        let tile_data_select = (self.lcdc >> 4) & 0x01; // LCDC.4 for BG/Win tile data source is common
        let is_cgb = self.model.is_cgb_family();

        match self.fetcher_state {
            FetcherState::GetTile => {
                let tile_map_base_addr: u16;
                // effective_map_x and effective_map_y are derived from self.fetcher_map_x and self.fetcher_map_y.
                // For BG: self.fetcher_map_x = self.scx + pixels_processed_in_bg; self.fetcher_map_y = self.scy + self.ly;
                // For Win: self.fetcher_map_x = pixels_processed_in_win_line; self.fetcher_map_y = self.window_internal_line_counter;
                // These are divided by 8 to get tile column/row.

                if self.fetcher_is_for_window {
                    tile_map_base_addr = if (self.lcdc >> 6) & 0x01 == 0 { 0x9800 } else { 0x9C00 }; // LCDC.6 for Window Tile Map
                    // self.fetcher_map_x is already 0-indexed for the window's first tile to be fetched.
                    // self.fetcher_map_y is self.window_internal_line_counter.
                } else { // Fetching for Background
                    tile_map_base_addr = if (self.lcdc >> 3) & 0x01 == 0 { 0x9800 } else { 0x9C00 }; // LCDC.3 for BG Tile Map
                    // self.fetcher_map_x is self.scx.wrapping_add(current_x_offset_for_bg_fetcher)
                    // self.fetcher_map_y is self.scy.wrapping_add(self.ly)
                }

                let tile_row_in_map = (self.fetcher_map_y / 8) as u16;
                let tile_col_in_map = (self.fetcher_map_x / 8) as u16;

                let tile_number_map_addr = tile_map_base_addr + (tile_row_in_map * 32) + tile_col_in_map;
                self.fetcher_tile_number = self.read_vram_bank_agnostic(tile_number_map_addr, 0);
                self.fetcher_tile_attributes = if is_cgb { self.read_vram_bank_agnostic(tile_number_map_addr, 1) } else { 0 };

                if tile_data_select == 1 { // Common for BG and Window: 0x8000 addressing mode (unsigned index)
                    self.fetcher_tile_data_base_addr = 0x8000 + (self.fetcher_tile_number as u16 * 16);
                } else { // Common for BG and Window: 0x8800 addressing mode (signed index relative to 0x9000)
                    self.fetcher_tile_data_base_addr = 0x9000u16.wrapping_add(((self.fetcher_tile_number as i8) as i16 * 16) as u16);
                }
                self.fetcher_state = FetcherState::GetDataLow;
            }
            FetcherState::GetDataLow => {
                // self.fetcher_map_y is already correctly set for either BG (scy.wrapping_add(ly))
                // or Window (self.window_internal_line_counter) context.
                let mut line_in_tile = (self.fetcher_map_y % 8) as u16;
                // CGB vertical flip: uses attribute bit 2 (0x04) as specified in task.
                // Pan Docs: BG Map Attr Bit 6 for VFlip. Task specifies to keep existing (1<<2) for now.
                if is_cgb && (self.fetcher_tile_attributes & (1 << 2)) != 0 {
                    line_in_tile = 7 - line_in_tile;
                }
                let tile_bytes_offset = line_in_tile * 2;
                let data_addr_plane1 = self.fetcher_tile_data_base_addr + tile_bytes_offset;
                // CGB VRAM bank for tile data: uses attribute bit 3 (0x08)
                let tile_data_vram_bank = if is_cgb { (self.fetcher_tile_attributes >> 3) & 0x01 } else { 0 };
                self.fetcher_tile_line_data_bytes[0] = self.read_vram_bank_agnostic(data_addr_plane1, tile_data_vram_bank);
                self.fetcher_state = FetcherState::GetDataHigh;
            }
            FetcherState::GetDataHigh => {
                let mut line_in_tile = (self.fetcher_map_y % 8) as u16;
                // CGB vertical flip: uses attribute bit 2 (0x04)
                if is_cgb && (self.fetcher_tile_attributes & (1 << 2)) != 0 {
                    line_in_tile = 7 - line_in_tile;
                }
                let tile_bytes_offset = line_in_tile * 2;
                let data_addr_plane2 = self.fetcher_tile_data_base_addr + tile_bytes_offset + 1;
                // CGB VRAM bank for tile data: uses attribute bit 3 (0x08)
                let tile_data_vram_bank = if is_cgb { (self.fetcher_tile_attributes >> 3) & 0x01 } else { 0 };
                self.fetcher_tile_line_data_bytes[1] = self.read_vram_bank_agnostic(data_addr_plane2, tile_data_vram_bank);
                self.fetcher_state = FetcherState::PushToFifo;
            }
            FetcherState::PushToFifo => {
                // Pixel Discard Logic (MUST be processed first)
                if self.fetcher_is_for_window {
                    if self.pixels_to_discard_for_window_line_start > 0 {
                        self.pixels_to_discard_for_window_line_start -= 1;
                        self.fetcher_pixel_push_idx += 1;
                        if self.fetcher_pixel_push_idx == 8 {
                            self.fetcher_pixel_push_idx = 0;
                            self.fetcher_map_x = self.fetcher_map_x.wrapping_add(8);
                            self.fetcher_state = FetcherState::GetTile;
                        }
                        return; 
                    }
                } else { 
                    if self.pixels_to_discard_at_line_start > 0 {
                        self.pixels_to_discard_at_line_start -= 1;
                        self.fetcher_pixel_push_idx += 1;
                        if self.fetcher_pixel_push_idx == 8 {
                            self.fetcher_pixel_push_idx = 0;
                            self.fetcher_map_x = self.fetcher_map_x.wrapping_add(8);
                            self.fetcher_state = FetcherState::GetTile;
                        }
                        return; 
                    }
                }
                
                eprintln!("FETCHER PUSH ATTEMPT (LY={}, X={}): fetch_px_idx={}, fifo_len={}, discard_bg={}, discard_win={}, fetcher_is_win={}", self.ly, self.screen_x, self.fetcher_pixel_push_idx, self.bg_fifo.len(), self.pixels_to_discard_at_line_start, self.pixels_to_discard_for_window_line_start, self.fetcher_is_for_window);

                if self.bg_fifo.is_full() { 
                    return; 
                }

                let is_cgb = self.model.is_cgb_family();
                let pixels = self.decode_tile_line_to_pixels(
                    self.fetcher_tile_line_data_bytes[0], self.fetcher_tile_line_data_bytes[1],
                    self.fetcher_tile_attributes, is_cgb);

                while self.fetcher_pixel_push_idx < 8 {
                    if self.bg_fifo.is_full() {
                        eprintln!("FETCHER PUSH LOOP STALLED (FIFO FULL): LY={}, X={}, fetch_px_idx={}, fifo_len={}", self.ly, self.screen_x, self.fetcher_pixel_push_idx, self.bg_fifo.len());
                        return; 
                    }

                    let pixel_data = PixelData { color_index: pixels[self.fetcher_pixel_push_idx as usize] };
                    if self.bg_fifo.push(pixel_data).is_ok() {
                        eprintln!("FETCHER PUSHED OK (LOOP): LY={}, X={}, pushed_idx={}, new_next_idx={}, fifo_len={}", self.ly, self.screen_x, self.fetcher_pixel_push_idx, self.fetcher_pixel_push_idx + 1, self.bg_fifo.len());
                        self.fetcher_pixel_push_idx += 1;
                    } else {
                        eprintln!("FETCHER PUSH FAILED IN LOOP (UNEXPECTED - FIFO BECAME FULL): LY={}, X={}, fetch_px_idx={}, fifo_len={}", self.ly, self.screen_x, self.fetcher_pixel_push_idx, self.bg_fifo.len());
                        return; 
                    }
                }

                if self.fetcher_pixel_push_idx == 8 {
                    self.fetcher_pixel_push_idx = 0;
                    self.fetcher_map_x = self.fetcher_map_x.wrapping_add(8); 
                    self.fetcher_state = FetcherState::GetTile;
                     eprintln!("FETCHER TILE COMPLETE: LY={}, X={}, next_state=GetTile, new_map_x={}", self.ly, self.screen_x, self.fetcher_map_x);
                }
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

    pub fn update_lyc(&mut self, value: u8) {
        // Capture interrupt condition state *before* any changes to LYC or STAT bit 2
        let previous_stat_interrupt_line = self.eval_stat_interrupt_conditions();
        self.lyc = value;

        // Update STAT bit 2 (LYC=LY flag) based on the new LYC
        if self.ly == self.lyc {
            self.stat |= 1 << 2;
        } else {
            self.stat &= !(1 << 2);
        }

        // Check if this change (to LYC and subsequently STAT bit 2) triggered an interrupt condition
        // This check covers cases where LYC=LY becomes true AND LYC interrupts are enabled,
        // OR if LYC=LY was already true but another condition (e.g. mode interrupt) that was false just became true
        // due to the STAT bit 2 update (though less likely for STAT bit 2 to affect other mode conditions).
        // The key is the overall interrupt line state change.
        let current_stat_interrupt_line = self.eval_stat_interrupt_conditions();
        if !previous_stat_interrupt_line && current_stat_interrupt_line {
            self.stat_interrupt_requested = true;
        }
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
        Some(FetchedSpritePixelLine { x_pos: oam_entry.x_pos, attributes: oam_entry.attributes, pixels }) // oam_index removed
    }

    // Removed fetch_tile_line_data as it was unused (#[allow(dead_code)]) and related to BgTileInfo
    // fn fetch_tile_line_data( &self, tile_map_addr_base: u16, tile_data_addr_base_select: u8, map_x: u8, map_y: u8,) -> FetchedTileLineData {
    //     let mut output = FetchedTileLineData::default();
    //     let is_cgb = self.model.is_cgb_family();
    //     let tile_row_in_map = (map_y / 8) as u16; let tile_col_in_map = (map_x / 8) as u16;
    //     let tile_number_map_addr = tile_map_addr_base + (tile_row_in_map * 32) + tile_col_in_map;
    //     let tile_number = self.read_vram_bank_agnostic(tile_number_map_addr, 0);
    //     let mut tile_data_vram_bank = 0;
    //     if is_cgb { output.attributes = self.read_vram_bank_agnostic(tile_number_map_addr, 1); tile_data_vram_bank = (output.attributes >> 3) & 0x01; }
    //     let tile_data_start_addr: u16 = if tile_data_addr_base_select == 1 { 0x8000 + (tile_number as u16 * 16) }
    //                                   else { 0x9000u16.wrapping_add(((tile_number as i8) as i16 * 16) as u16) };
    //     let mut line_offset_in_tile = (map_y % 8) as u16;
    //     if is_cgb && (output.attributes & (1 << 2)) != 0 { line_offset_in_tile = 7 - line_offset_in_tile; }
    //     let tile_bytes_offset = line_offset_in_tile * 2;
    //     output.plane1_byte = self.read_vram_bank_agnostic(tile_data_start_addr + tile_bytes_offset, tile_data_vram_bank);
    //     output.plane2_byte = self.read_vram_bank_agnostic(tile_data_start_addr + tile_bytes_offset + 1, tile_data_vram_bank);
    //     output
    // }

    fn decode_tile_line_to_pixels( &self, plane1_byte: u8, plane2_byte: u8, cgb_attributes: u8, is_cgb: bool,) -> [u8; 8] {
        let mut pixels = [0u8; 8];
        // CGB horizontal flip: uses attribute bit 1 (0x02) as specified in task.
        // Pan Docs: BG Map Attr Bit 5 for HFlip. Task specifies to keep existing (1<<1) for now.
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

    fn cgb_color_to_rgb(lo_byte: u8, hi_byte: u8) -> [u8; 3] {
        let bgr555 = ((hi_byte as u16) << 8) | (lo_byte as u16);
        let r5 = (bgr555 & 0x001F) as u8;
        let g5 = ((bgr555 & 0x03E0) >> 5) as u8;
        let b5 = ((bgr555 & 0x7C00) >> 10) as u8;

        // Scale 5-bit colors to 8-bit (val * 255 / 31 or (val << 3) | (val >> 2))
        let r8 = (r5 << 3) | (r5 >> 2);
        let g8 = (g5 << 3) | (g5 >> 2);
        let b8 = (b5 << 3) | (b5 >> 2);
        [r8, g8, b8]
    }

    fn apply_palette_to_framebuffer_line(&mut self) {
        if self.ly >= SCREEN_HEIGHT as u8 { return; }

        if self.model.is_cgb_family() {
            // CGB Rendering Path
            // Priority Master (LCDC Bit 0) for CGB determines if BG/Win lose to sprites.
            // If LCDC Bit 0 is 0, BG/Win are "blank" (color 0 of their respective palettes) unless OBJ is behind them.
            // This logic is implicitly handled by current_scanline_color_indices and current_scanline_pixel_source.
            // If BG/Win is disabled by LCDC.0, color_index should be 0 and source should be Background.

            for x in 0..SCREEN_WIDTH {
                let fb_idx = (self.ly as usize * SCREEN_WIDTH + x) * 3;
                if fb_idx + 2 >= self.framebuffer.len() { continue; }

                let dmg_color_idx = self.current_scanline_color_indices[x]; // 0-3
                // TODO: Handle CGB sprite palettes and priorities correctly.
                // For now, this path focuses on CGB Background pixels.
                // If PixelSource is Object, we'd need to use cgb_sprite_palette_ram and OAM attributes.

                match self.current_scanline_pixel_source[x] {
                    PixelSource::Background => {
                        let cgb_palette_num = self.current_scanline_cgb_bg_palette_indices[x]; // 0-7
                        let palette_addr = (cgb_palette_num as usize * 8) + (dmg_color_idx as usize * 2);

                        let lo_byte = self.cgb_background_palette_ram[palette_addr.min(CGB_PALETTE_RAM_SIZE - 2)];
                        let hi_byte = self.cgb_background_palette_ram[(palette_addr + 1).min(CGB_PALETTE_RAM_SIZE - 1)];

                        let rgb_color = Self::cgb_color_to_rgb(lo_byte, hi_byte);
                        self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&rgb_color);
                    }
                    PixelSource::Object { palette_register } => {
                        // Placeholder for CGB Object Palette rendering
                        // For now, use DMG-like rendering for CGB sprites as a fallback
                        // This will look wrong but prevents crashes.
                        // let sprite_palette_num = if self.model.is_cgb_family() { oam_attributes & 0x07 } else { palette_register };
                        // let palette_addr = (sprite_palette_num * 8) + (dmg_color_idx * 2);
                        // lo = self.cgb_sprite_palette_ram[addr]; hi = self.cgb_sprite_palette_ram[addr+1];
                        // rgb = cgb_color_to_rgb(lo, hi);
                        // Fallback to DMG palette for objects for now
                        let shade = if palette_register == 0 {
                            (self.obp0 >> (dmg_color_idx * 2)) & 0x03
                        } else {
                            (self.obp1 >> (dmg_color_idx * 2)) & 0x03
                        };
                        let rgb_color = match shade {
                            0 => DMG_WHITE, 1 => DMG_LIGHT_GRAY, 2 => DMG_DARK_GRAY, _ => DMG_BLACK,
                        };
                        self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&rgb_color);
                    }
                }
            }
        } else {
            // DMG Rendering Path
            for x in 0..SCREEN_WIDTH {
                let fb_idx = (self.ly as usize * SCREEN_WIDTH + x) * 3;
                if fb_idx + 2 >= self.framebuffer.len() { continue; }

                let color_index = self.current_scanline_color_indices[x];
                let pixel_source = self.current_scanline_pixel_source[x];
                let shade: u8;
                match pixel_source {
                    PixelSource::Background => { shade = (self.bgp >> (color_index * 2)) & 0x03; }
                    PixelSource::Object { palette_register } => {
                        if palette_register == 0 { shade = (self.obp0 >> (color_index * 2)) & 0x03; }
                        else { shade = (self.obp1 >> (color_index * 2)) & 0x03; }
                    }
                }
                let rgb_color = match shade { 0 => DMG_WHITE, 1 => DMG_LIGHT_GRAY, 2 => DMG_DARK_GRAY, 3 => DMG_BLACK, _ => unreachable!(), };
                self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&rgb_color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GameBoyModel;

    fn setup_ppu_cgb() -> Ppu {
        Ppu::new(GameBoyModel::GenericCGB)
    }

    #[test]
    fn test_bcps_register_access() {
        let mut ppu = setup_ppu_cgb();

        // Test writing index and auto-increment flag
        ppu.write_bcps_bcpi(0x8A); // Index 10 (0x0A), auto-increment enabled
        assert_eq!(ppu.read_bcps_bcpi(), 0x8A & 0xBF, "BCPS write 0x8A (auto-inc ON, index 0x0A)");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x0A, "BCPS index check");
        assert_ne!(ppu.bcps_bcpi & 0x80, 0, "BCPS auto-increment flag check");

        ppu.write_bcps_bcpi(0x05); // Index 5 (0x05), auto-increment disabled
        assert_eq!(ppu.read_bcps_bcpi(), 0x05, "BCPS write 0x05 (auto-inc OFF, index 0x05)");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x05, "BCPS index check");
        assert_eq!(ppu.bcps_bcpi & 0x80, 0, "BCPS auto-increment flag check");

        // Test writing to bit 6 (should be ignored / masked out)
        ppu.write_bcps_bcpi(0xFF); // All bits set (index 0x3F, auto-inc ON, try to set bit 6)
        assert_eq!(ppu.read_bcps_bcpi(), 0xBF, "BCPS write 0xFF (bit 6 masked)");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x3F, "BCPS index check for 0xFF write");
        assert_ne!(ppu.bcps_bcpi & 0x80, 0, "BCPS auto-increment flag for 0xFF write");

        ppu.write_bcps_bcpi(0x40); // Try to write only bit 6
        assert_eq!(ppu.read_bcps_bcpi(), 0x00, "BCPS write 0x40 (only bit 6, should be masked)");
    }

    #[test]
    fn test_bcpd_ram_access_and_increment() {
        let mut ppu = setup_ppu_cgb();
        ppu.lcdc = 0x80; // Ensure LCD is "on" for mode restrictions to apply if relevant (not for H/VBlank)
        ppu.current_mode = PpuMode::HBlank; // Palette access allowed

        // Test writing without auto-increment
        ppu.write_bcps_bcpi(0x0A); // Index 0x0A, auto-increment OFF
        ppu.write_bcpd(0x12);
        ppu.write_bcpd(0x34); // Should overwrite 0x12 as index doesn't change
        assert_eq!(ppu.cgb_background_palette_ram[0x0A], 0x34, "BCPD write no auto-inc");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x0A, "BCPS index unchanged after BCPD write (no auto-inc)");

        // Test reading without auto-increment
        let val = ppu.read_bcpd();
        assert_eq!(val, 0x34, "BCPD read no auto-inc");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x0A, "BCPS index unchanged after BCPD read");

        // Test writing with auto-increment
        ppu.write_bcps_bcpi(0x80); // Index 0, auto-increment ON
        ppu.write_bcpd(0xAA);
        assert_eq!(ppu.cgb_background_palette_ram[0x00], 0xAA, "BCPD write auto-inc (val @ 0x00)");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x01, "BCPS index incremented to 0x01");
        assert_ne!(ppu.bcps_bcpi & 0x80, 0, "BCPS auto-inc flag should remain set");

        ppu.write_bcpd(0xBB);
        assert_eq!(ppu.cgb_background_palette_ram[0x01], 0xBB, "BCPD write auto-inc (val @ 0x01)");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x02, "BCPS index incremented to 0x02");

        // Test auto-increment wrapping around 0x3F
        ppu.write_bcps_bcpi(0x80 | 0x3F); // Index 0x3F, auto-increment ON
        ppu.write_bcpd(0xCC);
        assert_eq!(ppu.cgb_background_palette_ram[0x3F], 0xCC, "BCPD write auto-inc (val @ 0x3F)");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x00, "BCPS index wrapped to 0x00");
        assert_ne!(ppu.bcps_bcpi & 0x80, 0, "BCPS auto-inc flag still set after wrap");

        // Test reading with auto-increment flag set (should not increment)
        ppu.write_bcps_bcpi(0x80 | 0x05); // Index 0x05, auto-inc ON
        ppu.cgb_background_palette_ram[0x05] = 0xDD;
        let val_read_auto_inc_on = ppu.read_bcpd();
        assert_eq!(val_read_auto_inc_on, 0xDD, "BCPD read with auto-inc ON");
        assert_eq!(ppu.bcps_bcpi & 0x3F, 0x05, "BCPS index unchanged after BCPD read (auto-inc ON)");
    }

    #[test]
    fn test_cgb_palette_mode_restrictions() {
        let mut ppu = setup_ppu_cgb();
        ppu.lcdc = 0x80; // LCD ON

        let test_addr_idx: u8 = 0x10;
        ppu.write_bcps_bcpi(test_addr_idx); // Index 0x10, auto-inc OFF
        ppu.cgb_background_palette_ram[test_addr_idx as usize] = 0xAB; // Pre-fill value

        // Mode 2 (OAM Scan) - Restricted
        ppu.current_mode = PpuMode::OamScan;
        assert_eq!(ppu.read_bcpd(), 0xFF, "BCPD read during OAMScan should be 0xFF");
        ppu.write_bcpd(0xCD); // Attempt write
        assert_eq!(ppu.cgb_background_palette_ram[test_addr_idx as usize], 0xAB, "BCPD write during OAMScan should be ignored");

        // Mode 3 (Drawing) - Restricted
        ppu.current_mode = PpuMode::Drawing;
        assert_eq!(ppu.read_bcpd(), 0xFF, "BCPD read during Drawing should be 0xFF");
        ppu.write_bcpd(0xEF); // Attempt write
        assert_eq!(ppu.cgb_background_palette_ram[test_addr_idx as usize], 0xAB, "BCPD write during Drawing should be ignored");

        // Mode 0 (HBlank) - Allowed
        ppu.current_mode = PpuMode::HBlank;
        assert_eq!(ppu.read_bcpd(), 0xAB, "BCPD read during HBlank should be allowed");
        ppu.write_bcpd(0x11);
        assert_eq!(ppu.cgb_background_palette_ram[test_addr_idx as usize], 0x11, "BCPD write during HBlank should be allowed");
        ppu.cgb_background_palette_ram[test_addr_idx as usize] = 0xAB; // Reset for next test

        // Mode 1 (VBlank) - Allowed
        ppu.current_mode = PpuMode::VBlank;
        assert_eq!(ppu.read_bcpd(), 0xAB, "BCPD read during VBlank should be allowed");
        ppu.write_bcpd(0x22);
        assert_eq!(ppu.cgb_background_palette_ram[test_addr_idx as usize], 0x22, "BCPD write during VBlank should be allowed");

        // LCD OFF - Allowed (current_mode check is inside LCD ON check)
        ppu.lcdc = 0x00; // LCD OFF
        ppu.current_mode = PpuMode::Drawing; // Arbitrary mode, should not matter if LCD is off
        ppu.cgb_background_palette_ram[test_addr_idx as usize] = 0x55;
        assert_eq!(ppu.read_bcpd(), 0x55, "BCPD read when LCD OFF should be allowed");
        ppu.write_bcpd(0x66);
        assert_eq!(ppu.cgb_background_palette_ram[test_addr_idx as usize], 0x66, "BCPD write when LCD OFF should be allowed");
    }

    #[test]
    fn test_color_conversion_cgb() {
        // Black: R=0, G=0, B=0 -> Lo=0x00, Hi=0x00
        assert_eq!(Ppu::cgb_color_to_rgb(0x00, 0x00), [0, 0, 0], "CGB Black to RGB");

        // White: R=31, G=31, B=31 -> Lo=0xFF, Hi=0x7F (0b01111111_11111111)
        // R5=(0xFF&0x1F)=31 -> R8=(31<<3)|(31>>2) = 248|7 = 255
        // G5=((0xFF&0xE0)>>5)|((0x7F&0x03)<<3) = (7<<0)|(3<<3) = 7|24 = 31 -> G8=255
        // B5=((0x7F&0x7C)>>2) = (0b01111100>>2) = 0b00011111 = 31 -> B8=255
        assert_eq!(Ppu::cgb_color_to_rgb(0xFF, 0x7F), [255, 255, 255], "CGB White to RGB");

        // Pure Red: R=31, G=0, B=0 -> Lo=0x1F, Hi=0x00 (0b00000000_00011111)
        assert_eq!(Ppu::cgb_color_to_rgb(0x1F, 0x00), [255, 0, 0], "CGB Red to RGB");

        // Pure Green: R=0, G=31, B=0 -> Lo=0xE0, Hi=0x03 (0b00000011_11100000)
        assert_eq!(Ppu::cgb_color_to_rgb(0xE0, 0x03), [0, 255, 0], "CGB Green to RGB");

        // Pure Blue: R=0, G=0, B=31 -> Lo=0x00, Hi=0x7C (0b01111100_00000000)
        assert_eq!(Ppu::cgb_color_to_rgb(0x00, 0x7C), [0, 0, 255], "CGB Blue to RGB");

        // Mid-Red: R=15, G=0, B=0 -> Lo=0x0F, Hi=0x00
        // R5=15 -> (15<<3)|(15>>2) = 120|3 = 123
        assert_eq!(Ppu::cgb_color_to_rgb(0x0F, 0x00), [123, 0, 0], "CGB Mid-Red to RGB");

        // Gray (16,16,16): R=16,G=16,B=16 -> Lo=0x00, Hi=0x42 (0b01000010_00000000 is wrong)
        // R=16 (0x10), G=16 (0x10), B=16 (0x10)
        // BGR555: BBBBB GGGGG RRRRR
        // Color: 0b10000_10000_10000 = 0x4210
        // Lo = 0x10, Hi = 0x42
        // R5=16 -> (16<<3)|(16>>2) = 128|4 = 132
        // G5=16 -> 132
        // B5=16 -> 132
        assert_eq!(Ppu::cgb_color_to_rgb(0x10, 0x42), [132, 132, 132], "CGB Gray(16,16,16) to RGB");
    }

    #[test]
    fn test_cgb_background_rendering_path() {
        let mut ppu = setup_ppu_cgb();
        ppu.ly = 0; // Test for scanline 0

        // Setup palette 0, color 1 (DMG index 1) to be bright red
        // R=31, G=0, B=0 -> Lo=0x1F, Hi=0x00
        ppu.cgb_background_palette_ram[0 * 8 + 1 * 2 + 0] = 0x1F; // Palette 0, Color 1, Lo byte
        ppu.cgb_background_palette_ram[0 * 8 + 1 * 2 + 1] = 0x00; // Palette 0, Color 1, Hi byte
        let expected_red_rgb = Ppu::cgb_color_to_rgb(0x1F, 0x00); // Should be [255,0,0]

        // Setup palette 1, color 2 (DMG index 2) to be bright green
        // R=0, G=31, B=0 -> Lo=0xE0, Hi=0x03
        ppu.cgb_background_palette_ram[1 * 8 + 2 * 2 + 0] = 0xE0; // Palette 1, Color 2, Lo byte
        ppu.cgb_background_palette_ram[1 * 8 + 2 * 2 + 1] = 0x03; // Palette 1, Color 2, Hi byte
        let expected_green_rgb = Ppu::cgb_color_to_rgb(0xE0, 0x03); // Should be [0,255,0]

        // Pixel 0: Use CGB palette 0, DMG color index 1 (should be red)
        ppu.current_scanline_color_indices[0] = 1; // DMG color index
        ppu.current_scanline_cgb_bg_palette_indices[0] = 0; // CGB BG palette index
        ppu.current_scanline_pixel_source[0] = PixelSource::Background;

        // Pixel 1: Use CGB palette 1, DMG color index 2 (should be green)
        ppu.current_scanline_color_indices[1] = 2; // DMG color index
        ppu.current_scanline_cgb_bg_palette_indices[1] = 1; // CGB BG palette index
        ppu.current_scanline_pixel_source[1] = PixelSource::Background;

        // Pixel 2: DMG color index 0 (from palette 0, should be whatever 0,0 is - black if not set)
        ppu.cgb_background_palette_ram[0 * 8 + 0 * 2 + 0] = 0x00;
        ppu.cgb_background_palette_ram[0 * 8 + 0 * 2 + 1] = 0x00;
        let expected_black_rgb = Ppu::cgb_color_to_rgb(0x00, 0x00);

        ppu.current_scanline_color_indices[2] = 0;
        ppu.current_scanline_cgb_bg_palette_indices[2] = 0;
        ppu.current_scanline_pixel_source[2] = PixelSource::Background;


        ppu.apply_palette_to_framebuffer_line();

        assert_eq!(&ppu.framebuffer[0..3], &expected_red_rgb, "Pixel 0 (Red)");
        assert_eq!(&ppu.framebuffer[3..6], &expected_green_rgb, "Pixel 1 (Green)");
        assert_eq!(&ppu.framebuffer[6..9], &expected_black_rgb, "Pixel 2 (Black)");
    }
}
// [end of src/ppu.rs] <-- This was the duplicated marker, now removed.
