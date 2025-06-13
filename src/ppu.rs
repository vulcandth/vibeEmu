pub struct Ppu {
    pub vram: [[u8; 0x2000]; 2],
    pub vram_bank: usize,
    pub oam: [u8; 0xA0],

    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    dma: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,

    bgpi: u8,
    bgpd: [u8; 0x40],
    obpi: u8,
    obpd: [u8; 0x40],

    mode_clock: u16,
    mode: u8,

    pub framebuffer: [u8; 160 * 144],
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            vram: [[0; 0x2000]; 2],
            vram_bank: 0,
            oam: [0; 0xA0],
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
            bgpi: 0,
            bgpd: [0; 0x40],
            obpi: 0,
            obpd: [0; 0x40],
            mode_clock: 0,
            mode: 2,
            framebuffer: [0; 160 * 144],
        }
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
            0xFF69 => self.bgpd[(self.bgpi & 0x3F) as usize],
            0xFF6A => self.obpi,
            0xFF6B => self.obpd[(self.obpi & 0x3F) as usize],
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
            _ => {}
        }
    }

    fn render_scanline(&mut self) {
        if self.lcdc & 0x80 == 0 {
            return;
        }

        if self.ly >= 144 {
            return;
        }

        let bg_enable = self.lcdc & 0x01 != 0 || self.bgp != 0;
        if !bg_enable {
            return;
        }

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

        let y = self.ly.wrapping_add(self.scy) as u16;
        let tile_row = (y / 8) as usize;
        let tile_y = (y % 8) as usize;

        for x in 0..160u16 {
            let scx = self.scx as u16;
            let px = x.wrapping_add(scx) & 0xFF;
            let tile_col = (px / 8) as usize;
            let tile_x = (px % 8) as usize;

            let tile_index = self.vram[0][tile_map_base + tile_row * 32 + tile_col];
            let addr = if self.lcdc & 0x10 != 0 {
                tile_data_base + tile_index as usize * 16
            } else {
                tile_data_base + ((tile_index as i8 as i16 + 128) as usize) * 16
            };

            let lo = self.vram[0][addr + tile_y * 2];
            let hi = self.vram[0][addr + tile_y * 2 + 1];
            let bit = 7 - tile_x;
            let color_id = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
            let color = (self.bgp >> (color_id * 2)) & 0x03;

            let idx = self.ly as usize * 160 + x as usize;
            self.framebuffer[idx] = color;
        }
    }

    pub fn step(&mut self, cycles: u16, if_reg: &mut u8) {
        let mut remaining = cycles;
        while remaining > 0 {
            let increment = remaining.min(4);
            remaining -= increment;
            self.mode_clock += increment;

            match self.mode {
                0 => {
                    if self.mode_clock >= 204 {
                        self.mode_clock -= 204;
                        self.ly += 1;
                        if self.ly == 144 {
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

            if self.ly == self.lyc && self.stat & 0x40 != 0 {
                *if_reg |= 0x02;
            }
        }
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}
