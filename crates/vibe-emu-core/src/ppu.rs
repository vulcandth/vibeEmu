use crate::hardware::DmgRevision;

// Screen resolution used by the Game Boy PPU
const SCREEN_WIDTH: usize = 160;
const SCREEN_HEIGHT: usize = 144;

// Timing constants per LCD mode in T-cycles
const MODE0_CYCLES: u16 = 204; // HBlank
const MODE1_CYCLES: u16 = 456; // One line during VBlank
const MODE2_CYCLES: u16 = 80; // OAM scan
const MODE3_CYCLES: u16 = 172; // Pixel transfer

// Number of lines spent in VBlank
const VBLANK_LINES: u8 = 10;

// Sprite limits
const MAX_SPRITES_PER_LINE: usize = 10;
const TOTAL_SPRITES: usize = 40;

// Internal memory sizes
const VRAM_BANK_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0xA0;
const PAL_RAM_SIZE: usize = 0x40;
const PAL_INDEX_MASK: u8 = 0x3F;
const PAL_UNUSED_BIT: u8 = 0x40;
const PAL_AUTO_INCREMENT_BIT: u8 = 0x80;

// Window X position is clipped if greater than this value
const WINDOW_X_MAX: u8 = 166;

// VRAM layout constants
const BG_MAP_0_BASE: usize = 0x1800;
const BG_MAP_1_BASE: usize = 0x1C00;
const TILE_DATA_0_BASE: usize = 0x0000;
const TILE_DATA_1_BASE: usize = 0x0800;

// LCD modes used in the `mode` field
const MODE_HBLANK: u8 = 0;
const MODE_VBLANK: u8 = 1;
const MODE_OAM: u8 = 2;
const MODE_TRANSFER: u8 = 3;

const BOOT_HOLD_CYCLES_DMG0: u16 = 8192;
const BOOT_HOLD_CYCLES_DMGA: u16 = 8192;

pub struct Ppu {
    pub vram: [[u8; VRAM_BANK_SIZE]; 2],
    pub vram_bank: usize,
    pub oam: [u8; OAM_SIZE],

    cgb: bool,

    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    lyc_eq_ly: bool,
    pub dma: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,

    /// Internal window line counter
    win_line_counter: u8,

    bgpi: u8,
    bgpd: [u8; PAL_RAM_SIZE],
    obpi: u8,
    obpd: [u8; PAL_RAM_SIZE],
    /// Object priority mode register (OPRI)
    opri: u8,

    mode_clock: u16,
    pub mode: u8,
    boot_hold_cycles: u16,

    pub framebuffer: [u32; SCREEN_WIDTH * SCREEN_HEIGHT],
    line_priority: [bool; SCREEN_WIDTH],
    line_color_zero: [bool; SCREEN_WIDTH],
    /// Latched sprites for the current scanline
    line_sprites: [Sprite; MAX_SPRITES_PER_LINE],
    sprite_count: usize,
    /// Indicates a completed frame is available in `framebuffer`
    frame_ready: bool,
    stat_irq_line: bool,
    dmg_mode2_vblank_irq_pending: bool,
    frame_counter: u64,
}

/// Default DMG palette colors in 0x00RRGGBB order for the `pixels` crate.
const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

#[derive(Copy, Clone, Default)]
struct Sprite {
    x: i16,
    y: i16,
    tile: u8,
    flags: u8,
    oam_index: usize,
}

impl Ppu {
    pub fn new_with_mode(cgb: bool) -> Self {
        Self {
            vram: [[0; VRAM_BANK_SIZE]; 2],
            vram_bank: 0,
            oam: [0; OAM_SIZE],
            cgb,
            lcdc: 0,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            lyc_eq_ly: false,
            dma: 0,
            bgp: 0,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
            win_line_counter: 0,
            bgpi: PAL_UNUSED_BIT,
            bgpd: [0; PAL_RAM_SIZE],
            obpi: PAL_UNUSED_BIT,
            obpd: [0; PAL_RAM_SIZE],
            opri: 0,
            mode_clock: 0,
            mode: MODE_OAM,
            boot_hold_cycles: 0,
            framebuffer: [0; SCREEN_WIDTH * SCREEN_HEIGHT],
            line_priority: [false; SCREEN_WIDTH],
            line_color_zero: [false; SCREEN_WIDTH],
            line_sprites: [Sprite::default(); MAX_SPRITES_PER_LINE],
            sprite_count: 0,
            frame_ready: false,
            stat_irq_line: false,
            dmg_mode2_vblank_irq_pending: false,
            frame_counter: 0,
        }
    }

    /// Collect up to 10 sprites visible on the current scanline.
    fn oam_scan(&mut self) {
        let sprite_height: i16 = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        self.sprite_count = 0;
        for i in 0..TOTAL_SPRITES {
            if self.sprite_count >= MAX_SPRITES_PER_LINE {
                break;
            }
            let base = i * 4;
            let y = self.oam[base] as i16 - 16;
            if self.ly as i16 >= y && (self.ly as i16) < y + sprite_height {
                self.line_sprites[self.sprite_count] = Sprite {
                    x: self.oam[base + 1] as i16 - 8,
                    y,
                    tile: self.oam[base + 2],
                    flags: self.oam[base + 3],
                    oam_index: i,
                };
                self.sprite_count += 1;
            }
        }
        if self.cgb && self.opri & 0x01 == 0 {
            // CGB-style priority: use OAM order only
            self.line_sprites[..self.sprite_count].sort_by_key(|s| s.oam_index);
        } else {
            // DMG-style priority: sort by X position then OAM index
            self.line_sprites[..self.sprite_count].sort_by_key(|s| (s.x, s.oam_index));
        }
    }

    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    pub fn in_hblank(&self) -> bool {
        self.mode == MODE_HBLANK
    }

    fn decode_cgb_color(lo: u8, hi: u8) -> u32 {
        let raw = ((hi as u16) << 8) | lo as u16;
        let r = ((raw & 0x1F) as u8) << 3 | ((raw & 0x1F) as u8 >> 2);
        let g = (((raw >> 5) & 0x1F) as u8) << 3 | (((raw >> 5) & 0x1F) as u8 >> 2);
        let b = (((raw >> 10) & 0x1F) as u8) << 3 | (((raw >> 10) & 0x1F) as u8 >> 2);
        ((r as u32) << 16) | ((g as u32) << 8) | b as u32
    }

    /// Initialize registers to the state expected after the boot ROM
    /// has finished executing.
    pub fn apply_boot_state(&mut self, dmg_revision: Option<DmgRevision>) {
        self.lcdc = 0x91;
        self.dma = 0xFF;
        self.bgp = 0xFC;
        self.win_line_counter = 0;

        if self.cgb {
            self.stat = 0x85;
            self.mode = MODE_VBLANK;
            self.ly = 0;
            self.boot_hold_cycles = 0;
        } else {
            self.stat = 0x00;
            match dmg_revision.unwrap_or_default() {
                DmgRevision::Rev0 => {
                    self.mode = MODE_TRANSFER;
                    self.ly = 0x01;
                    self.boot_hold_cycles = BOOT_HOLD_CYCLES_DMG0;
                }
                DmgRevision::RevA | DmgRevision::RevB | DmgRevision::RevC => {
                    self.mode = MODE_HBLANK;
                    self.ly = 0x0A;
                    self.boot_hold_cycles = BOOT_HOLD_CYCLES_DMGA;
                }
            }
        }

        self.lyc_eq_ly = self.ly == self.lyc;
        self.stat_irq_line = false;
        self.dmg_mode2_vblank_irq_pending = false;
    }

    /// Load the default CGB palettes used when running a DMG cartridge in
    /// compatibility mode. These values are based on the behavior of the
    /// official boot ROM.
    pub fn apply_dmg_compatibility_palettes(&mut self) {
        const OBJ_PAL: [u16; 4] = [0x7FFF, 0x421F, 0x1CF2, 0x0000];
        const BG_PAL: [u16; 4] = [0x7FFF, 0x1BEF, 0x6180, 0x0000];

        let (obj0, rest) = self.obpd.split_at_mut(8);
        let (obj1, _) = rest.split_at_mut(8);
        Self::write_palette(obj0, OBJ_PAL);
        Self::write_palette(obj1, OBJ_PAL);

        let (bg0, _) = self.bgpd.split_at_mut(8);
        Self::write_palette(bg0, BG_PAL);

        self.bgp = 0xE4;
        self.obp0 = 0xD0;
        self.obp1 = 0xE0;
    }

    fn write_palette(slice: &mut [u8], pal: [u16; 4]) {
        for (i, &c) in pal.iter().enumerate() {
            slice[i * 2] = (c & 0xFF) as u8;
            slice[i * 2 + 1] = (c >> 8) as u8;
        }
    }

    /// Returns true if a full frame has been rendered and is ready to display.
    pub fn frame_ready(&self) -> bool {
        self.frame_ready
    }

    /// Returns the current value of the internal window line counter.
    pub fn window_line_counter(&self) -> u8 {
        self.win_line_counter
    }

    /// Returns the current framebuffer. Call `frame_ready()` to check if a
    /// frame is complete. After presenting, call `clear_frame_flag()`.
    pub fn framebuffer(&self) -> &[u32; SCREEN_WIDTH * SCREEN_HEIGHT] {
        &self.framebuffer
    }

    /// Clears the frame ready flag after a frame has been consumed.
    pub fn clear_frame_flag(&mut self) {
        self.frame_ready = false;
    }

    /// Returns the number of frames that have been completed since power on.
    pub fn frames(&self) -> u64 {
        self.frame_counter
    }

    /// Returns true if the PPU is running in Game Boy Color mode.
    pub fn is_cgb(&self) -> bool {
        self.cgb
    }

    /// Get a CGB background palette color as 0x00RRGGBB.
    pub fn bg_palette_color(&self, palette: usize, color_id: usize) -> u32 {
        let off = palette * 8 + color_id * 2;
        Self::decode_cgb_color(self.bgpd[off], self.bgpd[off + 1])
    }

    /// Return a 0x00RRGGBB colour from **OBJ** palette RAM.
    ///
    /// * `palette` – CGB OBJ palette index (0-7)
    /// * `color_id` – colour within that palette (0-3)
    ///
    /// This is identical to `bg_palette_color` but uses the object-palette
    /// data (OBPD) instead of BGPD.
    pub fn ob_palette_color(&self, palette: usize, color_id: usize) -> u32 {
        let off = palette * 8 + color_id * 2;
        Self::decode_cgb_color(self.obpd[off], self.obpd[off + 1])
    }

    fn sanitize_palette_index(value: u8) -> u8 {
        (value & (PAL_AUTO_INCREMENT_BIT | PAL_INDEX_MASK)) | PAL_UNUSED_BIT
    }

    fn palette_ram_index(index: u8) -> usize {
        (index & PAL_INDEX_MASK) as usize
    }

    fn step_palette_index(index: &mut u8) {
        let current = *index;
        let idx = current & PAL_INDEX_MASK;
        let next_idx = if current & PAL_AUTO_INCREMENT_BIT != 0 {
            idx.wrapping_add(1) & PAL_INDEX_MASK
        } else {
            idx
        };
        let auto = current & PAL_AUTO_INCREMENT_BIT;
        *index = auto | PAL_UNUSED_BIT | next_idx;
    }

    fn update_lyc_compare(&mut self) {
        if self.lcdc & 0x80 != 0 {
            self.lyc_eq_ly = self.ly == self.lyc;
        }
    }

    pub fn read_reg(&mut self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => {
                (self.stat & 0x78)
                    | 0x80
                    | (self.mode & 0x03)
                    | if self.lyc_eq_ly { 0x04 } else { 0 }
            }
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF46 => self.dma,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            0xFF68 => {
                if self.cgb {
                    self.bgpi
                } else {
                    0xFF
                }
            }
            0xFF69 => {
                if self.cgb {
                    let val = self.bgpd[Self::palette_ram_index(self.bgpi)];
                    Self::step_palette_index(&mut self.bgpi);
                    val
                } else {
                    0xFF
                }
            }
            0xFF6A => {
                if self.cgb {
                    self.obpi
                } else {
                    0xFF
                }
            }
            0xFF6B => {
                if self.cgb {
                    let val = self.obpd[Self::palette_ram_index(self.obpi)];
                    Self::step_palette_index(&mut self.obpi);
                    val
                } else {
                    0xFF
                }
            }
            0xFF6C => {
                if self.cgb {
                    self.opri | 0xFE
                } else {
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF40 => {
                let was_on = self.lcdc & 0x80 != 0;
                self.lcdc = val;
                if was_on && self.lcdc & 0x80 == 0 {
                    self.mode = MODE_HBLANK;
                    self.mode_clock = 0;
                    self.win_line_counter = 0;
                    self.ly = 0;
                }
                if self.lcdc & 0x80 != 0 {
                    self.update_lyc_compare();
                }
            }
            0xFF41 => self.stat = (self.stat & 0x07) | (val & 0xF8),
            0xFF42 => self.scy = val,
            0xFF43 => self.scx = val,
            0xFF44 => {}
            0xFF45 => {
                self.lyc = val;
                self.update_lyc_compare();
            }
            0xFF46 => self.dma = val,
            0xFF47 => self.bgp = val,
            0xFF48 => self.obp0 = val,
            0xFF49 => self.obp1 = val,
            0xFF4A => self.wy = val,
            0xFF4B => self.wx = val,
            0xFF68 => {
                if self.cgb {
                    self.bgpi = Self::sanitize_palette_index(val);
                }
            }
            0xFF69 => {
                if self.cgb {
                    let idx = Self::palette_ram_index(self.bgpi);
                    self.bgpd[idx] = val;
                    Self::step_palette_index(&mut self.bgpi);
                }
            }
            0xFF6A => {
                if self.cgb {
                    self.obpi = Self::sanitize_palette_index(val);
                }
            }
            0xFF6B => {
                if self.cgb {
                    let idx = Self::palette_ram_index(self.obpi);
                    self.obpd[idx] = val;
                    Self::step_palette_index(&mut self.obpi);
                }
            }
            0xFF6C => {
                if self.cgb {
                    self.opri = val & 0x01;
                }
            }
            _ => {}
        }
    }

    #[inline(always)]
    fn dmg_shade(palette: u8, color_id: u8) -> u8 {
        (palette >> (color_id * 2)) & 0x03
    }

    fn render_scanline(&mut self) {
        if self.lcdc & 0x80 == 0 || self.ly as usize >= SCREEN_HEIGHT {
            return;
        }

        self.line_priority.fill(false);
        self.line_color_zero.fill(false);

        let bg_enabled = if self.cgb {
            true
        } else {
            self.lcdc & 0x01 != 0
        };
        let master_priority = if self.cgb {
            self.lcdc & 0x01 != 0
        } else {
            true
        };

        // Pre-fill the scanline. When the background is disabled via LCDC bit 0
        // in DMG mode, the Game Boy outputs color 0 for every pixel and sprites
        // treat the line as having color 0. The framebuffer is initialized with
        // this color so sprite rendering can overlay on top.
        let bg_color = if self.cgb {
            Self::decode_cgb_color(self.bgpd[0], self.bgpd[1])
        } else {
            let idx = Self::dmg_shade(self.bgp, 0);
            DMG_PALETTE[idx as usize]
        };
        for x in 0..SCREEN_WIDTH {
            let idx = self.ly as usize * SCREEN_WIDTH + x;
            self.framebuffer[idx] = bg_color;
            self.line_color_zero[x] = true;
        }

        if bg_enabled {
            let tile_map_base = if self.lcdc & 0x08 != 0 {
                BG_MAP_1_BASE
            } else {
                BG_MAP_0_BASE
            };
            let tile_data_base = if self.lcdc & 0x10 != 0 {
                TILE_DATA_0_BASE
            } else {
                TILE_DATA_1_BASE
            };

            // draw background
            for x in 0..SCREEN_WIDTH as u16 {
                let scx = self.scx as u16;
                let px = x.wrapping_add(scx) & 0xFF;
                let tile_col = (px / 8) as usize;
                let tile_row = (((self.ly as u16 + self.scy as u16) & 0xFF) / 8) as usize;
                let mut tile_y = (((self.ly as u16 + self.scy as u16) & 0xFF) % 8) as usize;

                let tile_index = self.vram[0][tile_map_base + tile_row * 32 + tile_col];
                let addr = if self.lcdc & 0x10 != 0 {
                    tile_data_base + tile_index as usize * 16
                } else {
                    tile_data_base + ((tile_index as i8 as i16 + 128) as usize) * 16
                };
                let mut bit = 7 - (px % 8) as usize;
                let mut priority = false;
                let mut palette = 0usize;
                let mut bank = 0usize;
                if self.cgb {
                    let attr = self.vram[1][tile_map_base + tile_row * 32 + tile_col];
                    palette = (attr & 0x07) as usize;
                    bank = if attr & 0x08 != 0 { 1 } else { 0 };
                    if attr & 0x20 != 0 {
                        bit = (px % 8) as usize;
                    }
                    if attr & 0x40 != 0 {
                        tile_y = 7 - tile_y;
                    }
                    priority = attr & 0x80 != 0;
                }
                let lo = self.vram[bank][addr + tile_y * 2];
                let hi = self.vram[bank][addr + tile_y * 2 + 1];
                let color_id = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                let (color, color_idx) = if self.cgb {
                    let off = palette * 8 + color_id as usize * 2;
                    (
                        Self::decode_cgb_color(self.bgpd[off], self.bgpd[off + 1]),
                        color_id,
                    )
                } else {
                    let idx = Self::dmg_shade(self.bgp, color_id);
                    (DMG_PALETTE[idx as usize], idx)
                };
                let idx = self.ly as usize * SCREEN_WIDTH + x as usize;
                self.framebuffer[idx] = color;
                self.line_priority[x as usize] = priority;
                self.line_color_zero[x as usize] = color_idx == 0;
            }

            // window
            let mut window_drawn = false;
            if self.lcdc & 0x20 != 0 && self.ly >= self.wy && self.wx <= WINDOW_X_MAX {
                let wx = self.wx.wrapping_sub(7) as u16;
                let window_map_base = if self.lcdc & 0x40 != 0 {
                    BG_MAP_1_BASE
                } else {
                    BG_MAP_0_BASE
                };
                let window_y = self.win_line_counter as usize;
                for x in wx..SCREEN_WIDTH as u16 {
                    let window_x = (x - wx) as usize;
                    let tile_col = window_x / 8;
                    let tile_row = window_y / 8;
                    let mut tile_y = window_y % 8;
                    let tile_x = window_x % 8;
                    let tile_index = self.vram[0][window_map_base + tile_row * 32 + tile_col];
                    let addr = if self.lcdc & 0x10 != 0 {
                        tile_data_base + tile_index as usize * 16
                    } else {
                        tile_data_base + ((tile_index as i8 as i16 + 128) as usize) * 16
                    };
                    let mut bit = 7 - tile_x;
                    let mut priority = false;
                    let mut palette = 0usize;
                    let mut bank = 0usize;
                    if self.cgb {
                        let attr = self.vram[1][window_map_base + tile_row * 32 + tile_col];
                        palette = (attr & 0x07) as usize;
                        bank = if attr & 0x08 != 0 { 1 } else { 0 };
                        if attr & 0x20 != 0 {
                            bit = tile_x;
                        }
                        if attr & 0x40 != 0 {
                            tile_y = 7 - tile_y;
                        }
                        priority = attr & 0x80 != 0;
                    }
                    let lo = self.vram[bank][addr + tile_y * 2];
                    let hi = self.vram[bank][addr + tile_y * 2 + 1];
                    let color_id = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                    let (color, color_idx) = if self.cgb {
                        let off = palette * 8 + color_id as usize * 2;
                        (
                            Self::decode_cgb_color(self.bgpd[off], self.bgpd[off + 1]),
                            color_id,
                        )
                    } else {
                        let idx = Self::dmg_shade(self.bgp, color_id);
                        (DMG_PALETTE[idx as usize], idx)
                    };
                    let idx = self.ly as usize * SCREEN_WIDTH + x as usize;
                    self.framebuffer[idx] = color;
                    if (x as usize) < SCREEN_WIDTH {
                        self.line_priority[x as usize] = priority;
                        self.line_color_zero[x as usize] = color_idx == 0;
                    }
                }
                window_drawn = true;
            }
            if window_drawn {
                self.win_line_counter = self.win_line_counter.wrapping_add(1);
            }
        }

        // sprites
        if self.lcdc & 0x02 != 0 {
            let sprite_height: i16 = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
            let mut drawn = [false; SCREEN_WIDTH];
            for s in &self.line_sprites[..self.sprite_count] {
                let mut tile = s.tile;
                if sprite_height == 16 {
                    tile &= 0xFE;
                }
                let mut line_idx = self.ly as i16 - s.y;
                if s.flags & 0x40 != 0 {
                    line_idx = sprite_height - 1 - line_idx;
                }
                let bank = if self.cgb {
                    ((s.flags >> 3) & 0x01) as usize
                } else {
                    0
                };
                for px in 0..8 {
                    let bit = if s.flags & 0x20 != 0 { px } else { 7 - px };
                    let addr = (tile + ((line_idx as usize) >> 3) as u8) as usize * 16
                        + (line_idx as usize & 7) * 2;
                    let lo = self.vram[bank][addr];
                    let hi = self.vram[bank][addr + 1];
                    let color_id = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                    if color_id == 0 {
                        continue;
                    }
                    let sx = s.x + px as i16;
                    if !(0i16..SCREEN_WIDTH as i16).contains(&sx) || drawn[sx as usize] {
                        continue;
                    }
                    let bg_zero = if !bg_enabled {
                        true
                    } else {
                        self.line_color_zero[sx as usize]
                    };
                    if master_priority {
                        if self.cgb && self.line_priority[sx as usize] && !bg_zero {
                            continue;
                        }
                        if s.flags & 0x80 != 0 && !bg_zero {
                            continue;
                        }
                    }
                    let color = if self.cgb {
                        let palette = (s.flags & 0x07) as usize;
                        let off = palette * 8 + color_id as usize * 2;
                        Self::decode_cgb_color(self.obpd[off], self.obpd[off + 1])
                    } else if s.flags & 0x10 != 0 {
                        let idxc = Self::dmg_shade(self.obp1, color_id);
                        DMG_PALETTE[idxc as usize]
                    } else {
                        let idxc = Self::dmg_shade(self.obp0, color_id);
                        DMG_PALETTE[idxc as usize]
                    };
                    let idx = self.ly as usize * SCREEN_WIDTH + sx as usize;
                    self.framebuffer[idx] = color;
                    drawn[sx as usize] = true;
                }
            }
        }
    }

    pub fn step(&mut self, cycles: u16, if_reg: &mut u8) -> bool {
        let mut remaining = cycles;
        if self.boot_hold_cycles > 0 {
            let consume = remaining.min(self.boot_hold_cycles);
            self.boot_hold_cycles -= consume;
            remaining -= consume;
            if remaining == 0 {
                return false;
            }
        }
        let mut hblank_triggered = false;
        while remaining > 0 {
            let increment = remaining.min(4);
            remaining -= increment;
            if self.lcdc & 0x80 == 0 {
                self.mode = MODE_HBLANK;
                self.ly = 0;
                self.mode_clock = 0;
                self.win_line_counter = 0;
                self.dmg_mode2_vblank_irq_pending = false;
                continue;
            }

            self.update_lyc_compare();

            self.mode_clock += increment;

            match self.mode {
                MODE_HBLANK => {
                    if self.mode_clock >= MODE0_CYCLES {
                        self.mode_clock -= MODE0_CYCLES;
                        self.ly += 1;
                        self.update_lyc_compare();
                        if self.ly == SCREEN_HEIGHT as u8 {
                            self.frame_ready = true;
                            self.mode = MODE_VBLANK;
                            if !self.cgb {
                                self.dmg_mode2_vblank_irq_pending = true;
                            }
                            *if_reg |= 0x01;
                        } else {
                            self.mode = MODE_OAM;
                        }
                    }
                }
                MODE_VBLANK => {
                    if self.mode_clock >= MODE1_CYCLES {
                        self.mode_clock -= MODE1_CYCLES;
                        self.ly += 1;
                        self.update_lyc_compare();
                        if self.ly > SCREEN_HEIGHT as u8 + VBLANK_LINES - 1 {
                            self.ly = 0;
                            self.frame_ready = false;
                            self.win_line_counter = 0;
                            self.frame_counter = self.frame_counter.wrapping_add(1);
                            self.mode = MODE_OAM;
                            self.update_lyc_compare();
                        }
                    }
                }
                MODE_OAM => {
                    if self.mode_clock >= MODE2_CYCLES {
                        self.mode_clock -= MODE2_CYCLES;
                        self.oam_scan();
                        self.mode = MODE_TRANSFER;
                    }
                }
                MODE_TRANSFER => {
                    if self.mode_clock >= MODE3_CYCLES {
                        self.mode_clock -= MODE3_CYCLES;
                        self.render_scanline();
                        self.mode = MODE_HBLANK;
                        hblank_triggered = true;
                    }
                }
                _ => {}
            }

            self.update_stat_irq(if_reg);
        }
        hblank_triggered
    }

    fn update_stat_irq(&mut self, if_reg: &mut u8) {
        let coincidence = self.lyc_eq_ly && self.stat & 0x40 != 0;
        let mode_signal = match self.mode {
            MODE_HBLANK => self.stat & 0x08 != 0,
            MODE_VBLANK => self.stat & 0x10 != 0,
            MODE_OAM => self.stat & 0x20 != 0,
            _ => false,
        };
        let glitch_pending = if self.cgb {
            false
        } else {
            self.dmg_mode2_vblank_irq_pending
        };
        let glitch = glitch_pending && self.stat & 0x20 != 0;
        self.dmg_mode2_vblank_irq_pending = false;
        let current = coincidence || mode_signal;
        if (current && !self.stat_irq_line) || glitch {
            *if_reg |= 0x02;
        }
        self.stat_irq_line = current || glitch;
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}
