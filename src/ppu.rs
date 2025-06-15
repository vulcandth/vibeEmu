pub struct Ppu {
    pub vram: [[u8; 0x2000]; 2],
    pub vram_bank: usize,
    pub oam: [u8; 0xA0],

    cgb: bool,

    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    pub dma: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,

    /// Internal window line counter
    win_line_counter: u8,

    bgpi: u8,
    bgpd: [u8; 0x40],
    obpi: u8,
    obpd: [u8; 0x40],
    /// Object priority mode register (OPRI)
    opri: u8,

    mode_clock: u16,
    pub mode: u8,

    pub framebuffer: [u32; 160 * 144],
    line_priority: [bool; 160],
    line_color_zero: [bool; 160],
    /// Latched sprites for the current scanline
    line_sprites: [Sprite; 10],
    sprite_count: usize,
    /// Indicates a completed frame is available in `framebuffer`
    frame_ready: bool,
    prev_stat_irq: u8,
}

/// Default DMG palette colors in 0x00RRGGBB order for `minifb`.
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
            vram: [[0; 0x2000]; 2],
            vram_bank: 0,
            oam: [0; 0xA0],
            cgb,
            lcdc: 0,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            dma: 0,
            bgp: 0,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
            win_line_counter: 0,
            bgpi: 0,
            bgpd: [0; 0x40],
            obpi: 0,
            obpd: [0; 0x40],
            opri: 0,
            mode_clock: 0,
            mode: 2,
            framebuffer: [0; 160 * 144],
            line_priority: [false; 160],
            line_color_zero: [false; 160],
            line_sprites: [Sprite::default(); 10],
            sprite_count: 0,
            frame_ready: false,
            prev_stat_irq: 0,
        }
    }

    /// Collect up to 10 sprites visible on the current scanline.
    fn oam_scan(&mut self) {
        let sprite_height: i16 = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        self.sprite_count = 0;
        for i in 0..40 {
            if self.sprite_count >= 10 {
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

    fn decode_cgb_color(lo: u8, hi: u8) -> u32 {
        let raw = ((hi as u16) << 8) | lo as u16;
        let r = ((raw & 0x1F) as u8) << 3 | ((raw & 0x1F) as u8 >> 2);
        let g = (((raw >> 5) & 0x1F) as u8) << 3 | (((raw >> 5) & 0x1F) as u8 >> 2);
        let b = (((raw >> 10) & 0x1F) as u8) << 3 | (((raw >> 10) & 0x1F) as u8 >> 2);
        ((r as u32) << 16) | ((g as u32) << 8) | b as u32
    }

    /// Initialize registers to the state expected after the boot ROM
    /// has finished executing.
    pub fn apply_boot_state(&mut self) {
        self.lcdc = 0x91;
        self.stat = 0x85;
        self.dma = 0xFF;
        self.bgp = 0xFC;
        self.mode = 1;
        self.win_line_counter = 0;
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
    pub fn framebuffer(&self) -> &[u32; 160 * 144] {
        &self.framebuffer
    }

    /// Clears the frame ready flag after a frame has been consumed.
    pub fn clear_frame_flag(&mut self) {
        self.frame_ready = false;
    }

    pub fn read_reg(&mut self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => {
                (self.stat & 0x78) | (self.mode & 0x03) | if self.ly == self.lyc { 0x04 } else { 0 }
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
            0xFF68 => self.bgpi,
            0xFF69 => {
                let val = self.bgpd[(self.bgpi & 0x3F) as usize];
                if self.bgpi & 0x80 != 0 {
                    self.bgpi = (self.bgpi & 0x80) | ((self.bgpi.wrapping_add(1)) & 0x3F);
                }
                val
            }
            0xFF6A => self.obpi,
            0xFF6B => {
                let val = self.obpd[(self.obpi & 0x3F) as usize];
                if self.obpi & 0x80 != 0 {
                    self.obpi = (self.obpi & 0x80) | ((self.obpi.wrapping_add(1)) & 0x3F);
                }
                val
            }
            0xFF6C => self.opri | 0xFE,
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF40 => self.lcdc = val,
            0xFF41 => self.stat = (self.stat & 0x07) | (val & 0xF8),
            0xFF42 => self.scy = val,
            0xFF43 => self.scx = val,
            0xFF44 => {}
            0xFF45 => self.lyc = val,
            0xFF46 => self.dma = val,
            0xFF47 => self.bgp = val,
            0xFF48 => self.obp0 = val,
            0xFF49 => self.obp1 = val,
            0xFF4A => self.wy = val,
            0xFF4B => self.wx = val,
            0xFF68 => self.bgpi = val,
            0xFF69 => {
                let idx = (self.bgpi & 0x3F) as usize;
                self.bgpd[idx] = val;
                if self.bgpi & 0x80 != 0 {
                    self.bgpi = (self.bgpi & 0x80) | ((idx as u8 + 1) & 0x3F);
                }
            }
            0xFF6A => self.obpi = val,
            0xFF6B => {
                let idx = (self.obpi & 0x3F) as usize;
                self.obpd[idx] = val;
                if self.obpi & 0x80 != 0 {
                    self.obpi = (self.obpi & 0x80) | ((idx as u8 + 1) & 0x3F);
                }
            }
            0xFF6C => self.opri = val & 0x01,
            _ => {}
        }
    }

    fn render_scanline(&mut self) {
        if self.lcdc & 0x80 == 0 || self.ly >= 144 {
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
            let idx = self.bgp & 0x03;
            DMG_PALETTE[idx as usize]
        };
        for x in 0..160usize {
            let idx = self.ly as usize * 160 + x;
            self.framebuffer[idx] = bg_color;
            self.line_color_zero[x] = true;
        }

        if bg_enabled {
            let tile_map_base = if self.lcdc & 0x08 != 0 {
                0x1C00
            } else {
                0x1800
            };
            let tile_data_base = if self.lcdc & 0x10 != 0 {
                0x0000
            } else {
                0x0800
            };

            // draw background
            for x in 0..160u16 {
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
                    let idx = (self.bgp >> (color_id * 2)) & 0x03;
                    (DMG_PALETTE[idx as usize], idx)
                };
                let idx = self.ly as usize * 160 + x as usize;
                self.framebuffer[idx] = color;
                self.line_priority[x as usize] = priority;
                self.line_color_zero[x as usize] = color_idx == 0;
            }

            // window
            let mut window_drawn = false;
            if self.lcdc & 0x20 != 0 && self.ly >= self.wy && self.wx <= 166 {
                let wx = self.wx.wrapping_sub(7) as u16;
                let window_map_base = if self.lcdc & 0x40 != 0 {
                    0x1C00
                } else {
                    0x1800
                };
                let window_y = self.win_line_counter as usize;
                for x in wx..160 {
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
                        let idx = (self.bgp >> (color_id * 2)) & 0x03;
                        (DMG_PALETTE[idx as usize], idx)
                    };
                    let idx = self.ly as usize * 160 + x as usize;
                    self.framebuffer[idx] = color;
                    if (x as usize) < 160 {
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
            let mut drawn = [false; 160];
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
                    if !(0i16..160i16).contains(&sx) || drawn[sx as usize] {
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
                        let idxc = (self.obp1 >> (color_id * 2)) & 0x03;
                        DMG_PALETTE[idxc as usize]
                    } else {
                        let idxc = (self.obp0 >> (color_id * 2)) & 0x03;
                        DMG_PALETTE[idxc as usize]
                    };
                    let idx = self.ly as usize * 160 + sx as usize;
                    self.framebuffer[idx] = color;
                    drawn[sx as usize] = true;
                }
            }
        }
    }

    pub fn step(&mut self, cycles: u16, if_reg: &mut u8) {
        let mut remaining = cycles;
        while remaining > 0 {
            let increment = remaining.min(4);
            remaining -= increment;
            if self.lcdc & 0x80 == 0 {
                self.mode = 0;
                self.ly = 0;
                self.mode_clock = 0;
                self.win_line_counter = 0;
                continue;
            }

            self.mode_clock += increment;

            match self.mode {
                0 => {
                    if self.mode_clock >= 204 {
                        self.mode_clock -= 204;
                        self.ly += 1;
                        if self.ly == 144 {
                            self.frame_ready = true;
                            self.mode = 1;
                            if self.stat & 0x10 != 0 {
                                *if_reg |= 0x02;
                            }
                            *if_reg |= 0x01;
                        } else {
                            self.mode = 2;
                            if self.stat & 0x20 != 0 {
                                *if_reg |= 0x02;
                            }
                        }
                    }
                }
                1 => {
                    if self.mode_clock >= 456 {
                        self.mode_clock -= 456;
                        self.ly += 1;
                        if self.ly > 153 {
                            self.ly = 0;
                            self.frame_ready = false;
                            self.win_line_counter = 0;
                            self.mode = 2;
                            if self.stat & 0x20 != 0 {
                                *if_reg |= 0x02;
                            }
                        }
                    }
                }
                2 => {
                    if self.mode_clock >= 80 {
                        self.mode_clock -= 80;
                        self.oam_scan();
                        self.mode = 3;
                    }
                }
                3 => {
                    if self.mode_clock >= 172 {
                        self.mode_clock -= 172;
                        self.render_scanline();
                        self.mode = 0;
                        if self.stat & 0x08 != 0 {
                            *if_reg |= 0x02;
                        }
                    }
                }
                _ => {}
            }

            self.update_stat_irq(if_reg);
        }
    }

    fn update_stat_irq(&mut self, if_reg: &mut u8) {
        let mut current = 0u8;
        if self.ly == self.lyc && self.stat & 0x40 != 0 {
            current |= 0x40;
        }
        match self.mode {
            0 => {
                if self.stat & 0x08 != 0 {
                    current |= 0x08;
                }
            }
            1 => {
                if self.stat & 0x10 != 0 {
                    current |= 0x10;
                }
            }
            2 => {
                if self.stat & 0x20 != 0 {
                    current |= 0x20;
                }
            }
            _ => {}
        }
        if current & !self.prev_stat_irq != 0 {
            *if_reg |= 0x02;
        }
        self.prev_stat_irq = current;
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}
