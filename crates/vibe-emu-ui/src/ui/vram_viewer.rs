use imgui::{self, TextureId};
use imgui_wgpu::{Renderer, Texture, TextureConfig};
use wgpu::{Extent3d, TextureFormat};

use vibe_emu_core::ppu::Ppu;

#[derive(Copy, Clone, PartialEq, Eq)]
enum VramTab {
    BgMap,
    Tiles,
    Oam,
    Palettes,
}

pub struct VramViewerWindow {
    current: VramTab,
    /* BG-map */
    bg_map_tex: Option<TextureId>,
    oam_tex: Option<TextureId>,
    bg_map_buf: Vec<u8>,
    oam_buf: Vec<u8>,

    /* Tiles */
    tiles_tex: Option<TextureId>,
    tiles_buf: Vec<u8>,

    /* Palettes */
    palettes_tex: Option<TextureId>,
    palettes_buf: Vec<u8>,

    last_frame: u64,
    oam_last_frame: u64,
    palettes_last_frame: u64,
}

impl VramViewerWindow {
    pub fn new() -> Self {
        // Maximum canvas is 256×192 (CGB shows both banks side-by-side)
        const MAX_TILES_W: usize = 256;
        const TILES_H: usize = 192;

        Self {
            current: VramTab::BgMap,

            /* BG-map */
            bg_map_tex: None,
            oam_tex: None,
            bg_map_buf: vec![0; 256 * 256 * 4],
            // 80 × (sprite_height max 16) = 80 × 16 × 4 bytes – will be
            // resized at runtime if OBJ size flag changes.
            oam_buf: Vec::new(),

            /* Tiles */
            tiles_tex: None,
            tiles_buf: vec![0; MAX_TILES_W * TILES_H * 4],

            /* Palettes — 32 × 128 px, 4 RGBA bytes each */
            palettes_tex: None,
            palettes_buf: vec![0; 32 * 128 * 4],

            last_frame: 0,
            oam_last_frame: 0,
            palettes_last_frame: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &imgui::Ui,
        ppu: &mut Ppu,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if let Some(_bar) = imgui::TabBar::new("VRAMTabs").begin(ui) {
            self.tab_item(
                ui,
                renderer,
                ppu,
                device,
                queue,
                VramTab::BgMap,
                "BG Map",
                Self::draw_bg_map,
            );
            self.tab_item(
                ui,
                renderer,
                ppu,
                device,
                queue,
                VramTab::Tiles,
                "Tiles",
                Self::draw_tiles,
            );
            self.tab_item(
                ui,
                renderer,
                ppu,
                device,
                queue,
                VramTab::Oam,
                "OAM",
                Self::draw_oam,
            );
            self.tab_item(
                ui,
                renderer,
                ppu,
                device,
                queue,
                VramTab::Palettes,
                "Palettes",
                Self::draw_palettes,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tab_item<F>(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &mut Ppu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        which: VramTab,
        label: &str,
        draw: F,
    ) where
        F: Fn(&mut Self, &imgui::Ui, &mut Renderer, &mut Ppu, &wgpu::Device, &wgpu::Queue),
    {
        if let Some(_tab) = imgui::TabItem::new(label).begin(ui) {
            self.current = which;
            draw(self, ui, renderer, ppu, device, queue);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_bg_map(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &mut Ppu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let frame = ppu.frames();
        if self.bg_map_tex.is_none() || frame != self.last_frame {
            self.bg_map_tex = Some(self.build_bg_map_texture(ppu, renderer, device, queue));
            self.last_frame = frame;
        }

        let tex_id = self.bg_map_tex.unwrap();
        let size = [256.0, 256.0];
        let avail = ui.content_region_avail();
        let scale = (avail[0] / size[0]).min(2.0);
        let draw_size = [size[0] * scale, size[1] * scale];

        let cursor = ui.cursor_screen_pos();
        imgui::Image::new(tex_id, draw_size).build(ui);

        let scx = ppu.read_reg(0xFF43);
        let scy = ppu.read_reg(0xFF42);
        let mut sx = scx as f32;
        if sx > 160.0 {
            sx -= 256.0;
        }
        sx = sx.clamp(-96.0, 160.0);
        let mut sy = scy as f32;
        if sy > 144.0 {
            sy -= 256.0;
        }
        sy = sy.clamp(-112.0, 144.0);
        let tl = [cursor[0] + sx * scale, cursor[1] + sy * scale];
        let br = [tl[0] + 160.0 * scale, tl[1] + 144.0 * scale];

        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(tl, br, imgui::ImColor32::from_rgb(255, 0, 0))
            .thickness(1.0)
            .build();

        if scx > 96 {
            draw_list
                .add_rect(
                    [cursor[0] + (sx - 256.0) * scale, tl[1]],
                    [cursor[0] + (sx - 96.0) * scale, br[1]],
                    imgui::ImColor32::from_rgb(255, 0, 0),
                )
                .thickness(1.0)
                .build();
        }
        if scy > 112 {
            draw_list
                .add_rect(
                    [tl[0], cursor[1] + (sy - 256.0) * scale],
                    [br[0], cursor[1] + (sy - 112.0) * scale],
                    imgui::ImColor32::from_rgb(255, 0, 0),
                )
                .thickness(1.0)
                .build();
        }
    }

    fn build_bg_map_texture(
        &mut self,
        ppu: &mut Ppu,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> TextureId {
        const MAP_W: usize = 32;
        const MAP_H: usize = 32;
        const TILE: usize = 8;
        const IMG_W: usize = MAP_W * TILE;
        const IMG_H: usize = MAP_H * TILE;
        const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let rgba = &mut self.bg_map_buf;
        rgba.fill(0);

        let lcdc = ppu.read_reg(0xFF40);
        let map_base = if lcdc & 0x08 != 0 { 0x1C00 } else { 0x1800 };
        // When bit 4 of LCDC is clear we use signed tile indices relative to
        // address 0x9000 (offset 0x1000 in VRAM). Otherwise indices are
        // unsigned from 0x8000.
        let signed_mode = lcdc & 0x10 == 0;
        let bgp = ppu.read_reg(0xFF47);
        let cgb = ppu.is_cgb();

        for tile_y in 0..MAP_H {
            for tile_x in 0..MAP_W {
                let tile_idx = ppu.vram[0][map_base + tile_y * MAP_W + tile_x];
                let attr = if cgb {
                    ppu.vram[1][map_base + tile_y * MAP_W + tile_x]
                } else {
                    0
                };
                let tile_num = if signed_mode {
                    tile_idx as i8 as i16
                } else {
                    tile_idx as i16
                };

                // Compute address safely for signed indices. Unsigned mode uses
                // 0x8000 as the base, while signed mode is relative to 0x9000
                // (offset 0x1000 in VRAM).
                let tile_addr = if signed_mode {
                    (0x1000i32 + (tile_num as i32) * 16) as usize
                } else {
                    (tile_num as usize) * 16
                };

                let bank = if cgb && attr & 0x08 != 0 { 1 } else { 0 };
                if tile_addr + 16 > ppu.vram[bank].len() {
                    continue;
                }
                for row in 0..TILE {
                    let lo = ppu.vram[bank][tile_addr + row * 2];
                    let hi = ppu.vram[bank][tile_addr + row * 2 + 1];
                    for col in 0..TILE {
                        let bit = 7 - col;
                        let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                        let color = if cgb {
                            let pal = (attr & 0x07) as usize;
                            ppu.bg_palette_color(pal, idx as usize)
                        } else {
                            let shade = (bgp >> (idx * 2)) & 0x03;
                            DMG_PALETTE[shade as usize]
                        };
                        let x = tile_x * TILE + col;
                        let y = tile_y * TILE + row;
                        let off = (y * IMG_W + x) * 4;
                        rgba[off] = ((color >> 16) & 0xFF) as u8;
                        rgba[off + 1] = ((color >> 8) & 0xFF) as u8;
                        rgba[off + 2] = (color & 0xFF) as u8;
                        rgba[off + 3] = 0xFF;
                    }
                }
            }
        }

        let config = TextureConfig {
            size: Extent3d {
                width: IMG_W as u32,
                height: IMG_H as u32,
                depth_or_array_layers: 1,
            },
            label: Some("BG-Map"),
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        };
        let texture = Texture::new(device, renderer, config);
        texture.write(queue, rgba, IMG_W as u32, IMG_H as u32);
        renderer.textures.insert(texture)
    }

    fn draw_tiles(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &mut Ppu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let frame = ppu.frames();
        if self.tiles_tex.is_none() || frame != self.last_frame {
            self.tiles_tex = Some(self.build_tiles_texture(ppu, renderer, device, queue));
            self.last_frame = frame;
        }

        let banks = if ppu.is_cgb() { 2 } else { 1 };
        let size = [128.0 * banks as f32, 192.0];
        let avail = ui.content_region_avail();
        let scale = (avail[0] / size[0]).min(3.0);
        let draw_size = [size[0] * scale, size[1] * scale];

        if let Some(tex) = self.tiles_tex {
            imgui::Image::new(tex, draw_size).build(ui);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_tiles_texture(
        &mut self,
        ppu: &mut Ppu,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> TextureId {
        const TILE_W: usize = 8;
        const TILE_H: usize = 8;
        const TILES_PER_ROW: usize = 16; // 16 × 8 px  = 128 px per bank
        const ROWS: usize = 24; // 24 × 8 px  = 192 px
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let banks = if ppu.is_cgb() { 2 } else { 1 };
        let img_w = TILES_PER_ROW * TILE_W * banks;
        let img_h = ROWS * TILE_H;

        let buf = &mut self.tiles_buf[..img_w * img_h * 4];
        buf.fill(0);

        let bgp = ppu.read_reg(0xFF47);

        for bank in 0..banks {
            for tile_idx in 0..384 {
                let col = tile_idx % TILES_PER_ROW;
                let row = tile_idx / TILES_PER_ROW;

                let tile_addr = tile_idx * 16; // 16 bytes per tile
                for y in 0..TILE_H {
                    let lo = ppu.vram[bank][tile_addr + y * 2];
                    let hi = ppu.vram[bank][tile_addr + y * 2 + 1];

                    for x in 0..TILE_W {
                        let bit = 7 - x;
                        let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

                        // Palette choose: CGB → BG palette 0, DMG → BGP shades
                        let rgb = if ppu.is_cgb() {
                            ppu.bg_palette_color(0, idx as usize)
                        } else {
                            let shade = (bgp >> (idx * 2)) & 0x03;
                            DMG_COLORS[shade as usize]
                        };

                        let px = (bank * 128) + (col * TILE_W) + x; // bank 1 sits to the right
                        let py = row * TILE_H + y;
                        let off = (py * img_w + px) * 4;

                        buf[off] = ((rgb >> 16) & 0xFF) as u8; // R
                        buf[off + 1] = ((rgb >> 8) & 0xFF) as u8; // G
                        buf[off + 2] = (rgb & 0xFF) as u8; // B
                        buf[off + 3] = 0xFF; // A
                    }
                }
            }
        }

        let tex_cfg = TextureConfig {
            size: Extent3d {
                width: img_w as u32,
                height: img_h as u32,
                depth_or_array_layers: 1,
            },
            label: Some("VRAM-Tiles"),
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        };
        let texture = Texture::new(device, renderer, tex_cfg);
        texture.write(queue, buf, img_w as u32, img_h as u32);
        renderer.textures.insert(texture)
    }
    fn build_oam_texture(
        &mut self,
        ppu: &mut Ppu,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> TextureId {
        const COLS: usize = 10; // 10 sprites per row → 4 rows
        const TOTAL: usize = 40;
        const TILE_W: usize = 8;
        const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let cgb = ppu.is_cgb();
        let lcdc = ppu.read_reg(0xFF40);
        let sprite_h: usize = if lcdc & 0x04 != 0 { 16 } else { 8 };
        let rows = TOTAL / COLS;
        let img_w = COLS * TILE_W;
        let img_h = rows * sprite_h;

        let needed = img_w * img_h * 4;
        if self.oam_buf.len() != needed {
            self.oam_buf.resize(needed, 0);
        }
        let rgba = &mut self.oam_buf;
        rgba.fill(0);

        for i in 0..TOTAL {
            let base = i * 4;
            let mut tile_num = ppu.oam[base + 2];
            let attr = ppu.oam[base + 3];

            let pal_idx_cgb = (attr & 0x07) as usize;
            let dmg_pal = if attr & 0x10 != 0 {
                ppu.read_reg(0xFF49)
            } else {
                ppu.read_reg(0xFF48)
            };
            let bank = if cgb && attr & 0x08 != 0 { 1 } else { 0 };
            let x_flip = attr & 0x20 != 0;
            let y_flip = attr & 0x40 != 0;

            if sprite_h == 16 {
                tile_num &= 0xFE; // ignore LSB for 8×16 sprites
            }

            let tile_x = i % COLS;
            let tile_y = i / COLS;

            for row in 0..sprite_h {
                // Handle Y-flip and second tile for 8×16 mode
                let mut src_row = if y_flip { sprite_h - 1 - row } else { row };
                let tile_offset = if sprite_h == 16 && src_row >= 8 {
                    tile_num as usize + 1
                } else {
                    tile_num as usize
                };
                if sprite_h == 16 && src_row >= 8 {
                    src_row -= 8;
                }
                let tile_addr = tile_offset * 16 + src_row * 2;

                let lo = ppu.vram[bank][tile_addr];
                let hi = ppu.vram[bank][tile_addr + 1];

                for col in 0..TILE_W {
                    let src_col = if x_flip { col } else { 7 - col };
                    let bit = 7 - src_col;
                    let color_id = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);

                    // Transparent?
                    if color_id == 0 {
                        continue;
                    }

                    let color = if cgb {
                        ppu.ob_palette_color(pal_idx_cgb, color_id as usize)
                    } else {
                        let shade = (dmg_pal >> (color_id * 2)) & 0x03;
                        DMG_PALETTE[shade as usize]
                    };

                    let x = tile_x * TILE_W + col;
                    let y = tile_y * sprite_h + row;
                    let off = (y * img_w + x) * 4;
                    rgba[off] = ((color >> 16) & 0xFF) as u8;
                    rgba[off + 1] = ((color >> 8) & 0xFF) as u8;
                    rgba[off + 2] = (color & 0xFF) as u8;
                    rgba[off + 3] = 0xFF;
                }
            }
        }

        let config = TextureConfig {
            size: Extent3d {
                width: img_w as u32,
                height: img_h as u32,
                depth_or_array_layers: 1,
            },
            label: Some("OAM"),
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        };
        let texture = Texture::new(device, renderer, config);
        texture.write(queue, rgba, img_w as u32, img_h as u32);
        renderer.textures.insert(texture)
    }

    fn draw_oam(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &mut Ppu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let frame = ppu.frames();
        if self.oam_tex.is_none() || frame != self.oam_last_frame {
            self.oam_tex = Some(self.build_oam_texture(ppu, renderer, device, queue));
            self.oam_last_frame = frame;
        }

        let tex_id = self.oam_tex.unwrap();
        // Logical size of the generated bitmap – recompute every call
        let sprite_h = if ppu.read_reg(0xFF40) & 0x04 != 0 {
            16.0
        } else {
            8.0
        };
        let size = [80.0, 4.0 * sprite_h];

        let avail = ui.content_region_avail();
        let scale = (avail[0] / size[0]).min(4.0);
        let draw_size = [size[0] * scale, size[1] * scale];

        imgui::Image::new(tex_id, draw_size).build(ui);
    }
    fn draw_palettes(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &mut Ppu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let frame = ppu.frames();
        if self.palettes_tex.is_none() || frame != self.palettes_last_frame {
            self.palettes_tex = Some(self.build_palettes_texture(ppu, renderer, device, queue));
            self.palettes_last_frame = frame;
        }

        let tex_id = self.palettes_tex.unwrap();
        let size = [32.0, 128.0];
        let avail = ui.content_region_avail();
        let scale = (avail[0] / size[0]).clamp(1.0, 4.0);
        let draw_size = [size[0] * scale, size[1] * scale];
        let cursor = ui.cursor_screen_pos();

        imgui::Image::new(tex_id, draw_size).build(ui);

        let draw_list = ui.get_window_draw_list();
        for row in 0..16 {
            let label = if row < 8 {
                format!("BG{row}")
            } else {
                format!("OBJ{}", row - 8)
            };
            let pos = [
                cursor[0] + draw_size[0] + 6.0,
                cursor[1] + row as f32 * 8.0 * scale + 2.0,
            ];
            draw_list.add_text(pos, imgui::ImColor32::from_rgb(255, 255, 255), label);
        }
    }

    fn build_palettes_texture(
        &mut self,
        ppu: &mut Ppu,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> TextureId {
        const SWATCH: usize = 8;
        const COLORS: usize = 4;
        const PALETTES: usize = 16;
        const IMG_W: usize = SWATCH * COLORS;
        const IMG_H: usize = SWATCH * PALETTES;
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let buf = &mut self.palettes_buf[..IMG_W * IMG_H * 4];
        buf.fill(0);

        let cgb = ppu.is_cgb();
        for row in 0..PALETTES {
            let (is_bg, pal_idx) = if row < 8 {
                (true, row)
            } else {
                (false, row - 8)
            };
            for col in 0..COLORS {
                let rgb = if cgb {
                    if is_bg {
                        ppu.bg_palette_color(pal_idx, col)
                    } else {
                        ppu.ob_palette_color(pal_idx, col)
                    }
                } else {
                    let reg = if is_bg {
                        ppu.read_reg(0xFF47)
                    } else if pal_idx == 0 {
                        ppu.read_reg(0xFF48)
                    } else {
                        ppu.read_reg(0xFF49)
                    };
                    let shade = ((reg >> (col * 2)) & 0x03) as usize;
                    DMG_COLORS[shade]
                };

                let r = ((rgb >> 16) & 0xFF) as u8;
                let g = ((rgb >> 8) & 0xFF) as u8;
                let b = (rgb & 0xFF) as u8;

                let x0 = col * SWATCH;
                let y0 = row * SWATCH;
                for y in 0..SWATCH {
                    for x in 0..SWATCH {
                        let off = ((y0 + y) * IMG_W + (x0 + x)) * 4;
                        buf[off] = r;
                        buf[off + 1] = g;
                        buf[off + 2] = b;
                        buf[off + 3] = 0xFF;
                    }
                }
            }
        }

        let config = TextureConfig {
            size: Extent3d {
                width: IMG_W as u32,
                height: IMG_H as u32,
                depth_or_array_layers: 1,
            },
            label: Some("Palettes"),
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        };

        let texture = Texture::new(device, renderer, config);
        texture.write(queue, buf, IMG_W as u32, IMG_H as u32);
        renderer.textures.insert(texture)
    }
}
