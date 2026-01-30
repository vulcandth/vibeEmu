use imgui::{self, TextureId};
use imgui_wgpu::{Renderer, Texture, TextureConfig};
use wgpu::{Extent3d, TextureFormat};

use crate::ui::snapshot::PpuSnapshot;

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
    bg_map_buf: Vec<u8>,

    /* OAM */
    oam_sprite_textures: Vec<Option<TextureId>>,
    oam_sprite_bufs: Vec<Vec<u8>>,
    oam_selected: usize,
    oam_last_frame: u64,
    oam_sprite_h: u8,

    /* Tiles */
    tiles_tex: Option<TextureId>,
    tiles_buf: Vec<u8>,
    tiles_banks: u8,

    /* Palettes */
    palette_sel_is_bg: bool,
    palette_sel_pal: u8,
    palette_sel_col: u8,

    last_frame: u64,
    tiles_last_frame: u64,
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
            bg_map_buf: vec![0; 256 * 256 * 4],

            /* OAM - 40 sprites */
            oam_sprite_textures: vec![None; 40],
            oam_sprite_bufs: (0..40).map(|_| vec![0u8; 8 * 16 * 4]).collect(),
            oam_selected: 0,
            oam_last_frame: 0,
            oam_sprite_h: 0,

            /* Tiles */
            tiles_tex: None,
            tiles_buf: vec![0; MAX_TILES_W * TILES_H * 4],
            tiles_banks: 1,

            /* Palettes — 32 × 128 px, 4 RGBA bytes each */
            palette_sel_is_bg: true,
            palette_sel_pal: 0,
            palette_sel_col: 0,

            last_frame: 0,
            tiles_last_frame: 0,
        }
    }

    fn rgb888_to_gb_word(rgb: u32) -> u16 {
        let r5 = ((rgb >> 16) & 0xFF) as u16 >> 3;
        let g5 = ((rgb >> 8) & 0xFF) as u16 >> 3;
        let b5 = (rgb & 0xFF) as u16 >> 3;
        r5 | (g5 << 5) | (b5 << 10)
    }

    fn palette_color_rgb(ppu: &PpuSnapshot, is_bg: bool, pal: usize, col: usize) -> u32 {
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        if ppu.cgb {
            if is_bg {
                ppu.cgb_bg_colors[pal][col]
            } else {
                ppu.cgb_ob_colors[pal][col]
            }
        } else {
            let reg = if is_bg {
                ppu.bgp
            } else if pal == 0 {
                ppu.obp(0)
            } else {
                ppu.obp(1)
            };
            let shade = ((reg >> (col * 2)) & 0x03) as usize;
            DMG_COLORS[shade]
        }
    }

    fn palette_color_word(ppu: &PpuSnapshot, is_bg: bool, pal: usize, col: usize) -> u16 {
        Self::rgb888_to_gb_word(Self::palette_color_rgb(ppu, is_bg, pal, col))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &imgui::Ui,
        ppu: &PpuSnapshot,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let display = ui.io().display_size;
        let flags = imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_COLLAPSE;

        ui.window("VRAM")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size(display, imgui::Condition::Always)
            .flags(flags)
            .build(|| {
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
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn tab_item<F>(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &PpuSnapshot,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        which: VramTab,
        label: &str,
        draw: F,
    ) where
        F: Fn(&mut Self, &imgui::Ui, &mut Renderer, &PpuSnapshot, &wgpu::Device, &wgpu::Queue),
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
        ppu: &PpuSnapshot,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let frame = ppu.frame_counter;
        if self.bg_map_tex.is_none() {
            self.bg_map_tex = Some(self.build_bg_map_texture(ppu, renderer, device, queue));
            self.last_frame = frame;
        } else if frame != self.last_frame {
            if let Some(tex_id) = self.bg_map_tex {
                if !self.update_bg_map_texture(tex_id, ppu, renderer, queue) {
                    self.bg_map_tex = Some(self.build_bg_map_texture(ppu, renderer, device, queue));
                }
            } else {
                self.bg_map_tex = Some(self.build_bg_map_texture(ppu, renderer, device, queue));
            }
            self.last_frame = frame;
        }

        let Some(tex_id) = self.bg_map_tex else {
            return;
        };
        let size = [256.0, 256.0];
        let avail = ui.content_region_avail();
        let scale = (avail[0] / size[0]).min(2.0);
        let draw_size = [size[0] * scale, size[1] * scale];

        let cursor = ui.cursor_screen_pos();
        imgui::Image::new(tex_id, draw_size).build(ui);

        let scx = ppu.scx;
        let scy = ppu.scy;
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

    fn fill_bg_map_buf(&mut self, ppu: &PpuSnapshot) {
        const MAP_W: usize = 32;
        const MAP_H: usize = 32;
        const TILE: usize = 8;
        const IMG_W: usize = MAP_W * TILE;
        const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let rgba = &mut self.bg_map_buf;
        rgba.fill(0);

        let lcdc = ppu.lcdc;
        let map_base = if lcdc & 0x08 != 0 { 0x1C00 } else { 0x1800 };
        // When bit 4 of LCDC is clear we use signed tile indices relative to
        // address 0x9000 (offset 0x1000 in VRAM). Otherwise indices are
        // unsigned from 0x8000.
        let signed_mode = lcdc & 0x10 == 0;
        let bgp = ppu.bgp;
        let cgb = ppu.cgb;

        for tile_y in 0..MAP_H {
            for tile_x in 0..MAP_W {
                let tile_idx = ppu.vram0[map_base + tile_y * MAP_W + tile_x];
                let attr = if cgb {
                    ppu.vram1[map_base + tile_y * MAP_W + tile_x]
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
                let vram = ppu.vram_bank(bank);
                if tile_addr + 16 > vram.len() {
                    continue;
                }
                for row in 0..TILE {
                    let lo = vram[tile_addr + row * 2];
                    let hi = vram[tile_addr + row * 2 + 1];
                    for col in 0..TILE {
                        let bit = 7 - col;
                        let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);
                        let color = if cgb {
                            let pal = (attr & 0x07) as usize;
                            ppu.cgb_bg_colors[pal][idx as usize]
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
    }

    fn build_bg_map_texture(
        &mut self,
        ppu: &PpuSnapshot,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> TextureId {
        const IMG_W: usize = 256;
        const IMG_H: usize = 256;

        self.fill_bg_map_buf(ppu);

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
        texture.write(queue, &self.bg_map_buf, IMG_W as u32, IMG_H as u32);
        renderer.textures.insert(texture)
    }

    fn update_bg_map_texture(
        &mut self,
        tex_id: TextureId,
        ppu: &PpuSnapshot,
        renderer: &mut Renderer,
        queue: &wgpu::Queue,
    ) -> bool {
        const IMG_W: u32 = 256;
        const IMG_H: u32 = 256;

        let Some(texture) = renderer.textures.get_mut(tex_id) else {
            return false;
        };

        self.fill_bg_map_buf(ppu);
        texture.write(queue, &self.bg_map_buf, IMG_W, IMG_H);
        true
    }

    fn draw_tiles(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &PpuSnapshot,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let frame = ppu.frame_counter;
        let banks = if ppu.cgb { 2 } else { 1 };
        if banks != self.tiles_banks {
            self.tiles_banks = banks;
            self.tiles_tex = None;
            self.tiles_last_frame = 0;
        }

        if self.tiles_tex.is_none() {
            self.tiles_tex = Some(self.build_tiles_texture(ppu, renderer, device, queue));
            self.tiles_last_frame = frame;
        } else if frame != self.tiles_last_frame {
            if let Some(tex_id) = self.tiles_tex {
                if !self.update_tiles_texture(tex_id, ppu, renderer, queue) {
                    self.tiles_tex = Some(self.build_tiles_texture(ppu, renderer, device, queue));
                }
            } else {
                self.tiles_tex = Some(self.build_tiles_texture(ppu, renderer, device, queue));
            }
            self.tiles_last_frame = frame;
        }

        let banks = self.tiles_banks;
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
        ppu: &PpuSnapshot,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> TextureId {
        const TILE_W: usize = 8;
        const TILE_H: usize = 8;
        const TILES_PER_ROW: usize = 16; // 16 × 8 px  = 128 px per bank
        const ROWS: usize = 24; // 24 × 8 px  = 192 px
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let banks = if ppu.cgb { 2 } else { 1 };
        let img_w = TILES_PER_ROW * TILE_W * banks;
        let img_h = ROWS * TILE_H;

        let buf = &mut self.tiles_buf[..img_w * img_h * 4];
        buf.fill(0);

        let bgp = ppu.bgp;

        for bank in 0..banks {
            for tile_idx in 0..384 {
                let col = tile_idx % TILES_PER_ROW;
                let row = tile_idx / TILES_PER_ROW;

                let tile_addr = tile_idx * 16; // 16 bytes per tile
                for y in 0..TILE_H {
                    let vram = ppu.vram_bank(bank);
                    let lo = vram[tile_addr + y * 2];
                    let hi = vram[tile_addr + y * 2 + 1];

                    for x in 0..TILE_W {
                        let bit = 7 - x;
                        let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

                        // Palette choose: CGB → BG palette 0, DMG → BGP shades
                        let rgb = if ppu.cgb {
                            ppu.cgb_bg_colors[0][idx as usize]
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

    fn update_tiles_texture(
        &mut self,
        tex_id: TextureId,
        ppu: &PpuSnapshot,
        renderer: &mut Renderer,
        queue: &wgpu::Queue,
    ) -> bool {
        let Some(texture) = renderer.textures.get_mut(tex_id) else {
            return false;
        };

        // Recompute tiles into the existing buffer and write into the existing texture.
        let banks = self.tiles_banks as usize;
        let img_w = 16 * 8 * banks;
        let img_h = 24 * 8;
        let buf = &mut self.tiles_buf[..img_w * img_h * 4];
        buf.fill(0);

        let bgp = ppu.bgp;
        const TILE_W: usize = 8;
        const TILE_H: usize = 8;
        const TILES_PER_ROW: usize = 16;
        const ROWS: usize = 24;
        const DMG_COLORS: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        for bank in 0..banks {
            for tile_idx in 0..384 {
                let col = tile_idx % TILES_PER_ROW;
                let row = tile_idx / TILES_PER_ROW;

                let tile_addr = tile_idx * 16;
                for y in 0..TILE_H {
                    let vram = ppu.vram_bank(bank);
                    let lo = vram[tile_addr + y * 2];
                    let hi = vram[tile_addr + y * 2 + 1];

                    for x in 0..TILE_W {
                        let bit = 7 - x;
                        let idx = ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1);

                        let rgb = if ppu.cgb {
                            ppu.cgb_bg_colors[0][idx as usize]
                        } else {
                            let shade = (bgp >> (idx * 2)) & 0x03;
                            DMG_COLORS[shade as usize]
                        };

                        let px = (bank * 128) + (col * TILE_W) + x;
                        let py = row * TILE_H + y;
                        let off = (py * img_w + px) * 4;

                        buf[off] = ((rgb >> 16) & 0xFF) as u8;
                        buf[off + 1] = ((rgb >> 8) & 0xFF) as u8;
                        buf[off + 2] = (rgb & 0xFF) as u8;
                        buf[off + 3] = 0xFF;
                    }
                }
            }
        }

        texture.write(queue, buf, img_w as u32, img_h as u32);
        true
    }

    fn fill_sprite_buf(&mut self, sprite_idx: usize, ppu: &PpuSnapshot) -> (u32, u32) {
        const TILE_W: usize = 8;
        const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

        let cgb = ppu.cgb;
        let lcdc = ppu.lcdc;
        let sprite_h: usize = if lcdc & 0x04 != 0 { 16 } else { 8 };

        let buf = &mut self.oam_sprite_bufs[sprite_idx];
        let needed = TILE_W * sprite_h * 4;
        if buf.len() != needed {
            buf.resize(needed, 0);
        }
        buf.fill(0);

        let base = sprite_idx * 4;
        let mut tile_num = ppu.oam[base + 2];
        let attr = ppu.oam[base + 3];

        let pal_idx_cgb = (attr & 0x07) as usize;
        let dmg_pal = if attr & 0x10 != 0 {
            ppu.obp(1)
        } else {
            ppu.obp(0)
        };
        let bank = if cgb && attr & 0x08 != 0 { 1 } else { 0 };
        let x_flip = attr & 0x20 != 0;
        let y_flip = attr & 0x40 != 0;

        if sprite_h == 16 {
            tile_num &= 0xFE;
        }

        for row in 0..sprite_h {
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
            let vram = ppu.vram_bank(bank);
            let lo = vram.get(tile_addr).copied().unwrap_or(0);
            let hi = vram.get(tile_addr + 1).copied().unwrap_or(0);

            for col in 0..TILE_W {
                let src_col = if x_flip { col } else { 7 - col };
                let bit = 7 - src_col;
                let color_id = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);

                let color = if color_id == 0 {
                    0x00000000
                } else if cgb {
                    ppu.cgb_ob_colors[pal_idx_cgb][color_id as usize]
                } else {
                    let shade = (dmg_pal >> (color_id * 2)) & 0x03;
                    DMG_PALETTE[shade as usize]
                };

                let off = (row * TILE_W + col) * 4;
                if color_id == 0 {
                    buf[off] = 0x20;
                    buf[off + 1] = 0x20;
                    buf[off + 2] = 0x30;
                    buf[off + 3] = 0xFF;
                } else {
                    buf[off] = ((color >> 16) & 0xFF) as u8;
                    buf[off + 1] = ((color >> 8) & 0xFF) as u8;
                    buf[off + 2] = (color & 0xFF) as u8;
                    buf[off + 3] = 0xFF;
                }
            }
        }

        (TILE_W as u32, sprite_h as u32)
    }

    fn build_sprite_texture(
        &mut self,
        sprite_idx: usize,
        ppu: &PpuSnapshot,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> TextureId {
        let (img_w, img_h) = self.fill_sprite_buf(sprite_idx, ppu);
        let rgba = &self.oam_sprite_bufs[sprite_idx];
        let config = TextureConfig {
            size: Extent3d {
                width: img_w,
                height: img_h,
                depth_or_array_layers: 1,
            },
            label: Some("OAM_sprite"),
            format: Some(TextureFormat::Rgba8UnormSrgb),
            ..Default::default()
        };
        let texture = Texture::new(device, renderer, config);
        texture.write(queue, rgba, img_w, img_h);
        renderer.textures.insert(texture)
    }

    fn update_sprite_texture(
        &mut self,
        sprite_idx: usize,
        tex_id: TextureId,
        ppu: &PpuSnapshot,
        renderer: &mut Renderer,
        queue: &wgpu::Queue,
    ) -> bool {
        let Some(texture) = renderer.textures.get_mut(tex_id) else {
            return false;
        };
        let (img_w, img_h) = self.fill_sprite_buf(sprite_idx, ppu);
        let rgba = &self.oam_sprite_bufs[sprite_idx];
        texture.write(queue, rgba, img_w, img_h);
        true
    }

    fn draw_oam(
        &mut self,
        ui: &imgui::Ui,
        renderer: &mut Renderer,
        ppu: &PpuSnapshot,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let frame = ppu.frame_counter;
        let sprite_h = if ppu.lcdc & 0x04 != 0 { 16u8 } else { 8u8 };

        if self.oam_sprite_h != sprite_h {
            self.oam_sprite_h = sprite_h;
            for tex in &mut self.oam_sprite_textures {
                *tex = None;
            }
            self.oam_last_frame = 0;
        }

        let needs_update = frame != self.oam_last_frame;
        if needs_update {
            self.oam_last_frame = frame;
        }

        for i in 0..40 {
            if self.oam_sprite_textures[i].is_none() {
                self.oam_sprite_textures[i] =
                    Some(self.build_sprite_texture(i, ppu, renderer, device, queue));
            } else if needs_update
                && let Some(tex_id) = self.oam_sprite_textures[i]
                && !self.update_sprite_texture(i, tex_id, ppu, renderer, queue)
            {
                self.oam_sprite_textures[i] =
                    Some(self.build_sprite_texture(i, ppu, renderer, device, queue));
            }
        }

        let layout_flags = imgui::TableFlags::SIZING_STRETCH_PROP
            | imgui::TableFlags::NO_SAVED_SETTINGS
            | imgui::TableFlags::BORDERS_INNER_V;

        if let Some(_layout) = ui.begin_table_with_flags("oam_layout", 2, layout_flags) {
            ui.table_setup_column_with(imgui::TableColumnSetup {
                name: "sprites",
                flags: imgui::TableColumnFlags::WIDTH_STRETCH,
                init_width_or_weight: 0.75,
                user_id: imgui::Id::default(),
            });
            ui.table_setup_column_with(imgui::TableColumnSetup {
                name: "details",
                flags: imgui::TableColumnFlags::WIDTH_STRETCH,
                init_width_or_weight: 0.25,
                user_id: imgui::Id::default(),
            });

            ui.table_next_row();

            ui.table_next_column();
            self.draw_oam_grid(ui, ppu, sprite_h);

            ui.table_next_column();
            self.draw_oam_details(ui, ppu, sprite_h);
        }
    }

    fn draw_oam_grid(&mut self, ui: &imgui::Ui, ppu: &PpuSnapshot, sprite_h: u8) {
        const COLS: usize = 10;
        const TOTAL: usize = 40;

        let sprite_h_f = sprite_h as f32;
        let scale = 2.0f32;
        let tile_draw_w = 8.0 * scale;
        let tile_draw_h = sprite_h_f * scale;

        let cell_w = tile_draw_w + 8.0;
        let cell_h = tile_draw_h + 48.0;

        let avail = ui.content_region_avail();
        ui.child_window("oam_grid")
            .size([avail[0], avail[1]])
            .build(|| {
                for i in 0..TOTAL {
                    let col = i % COLS;

                    if col != 0 {
                        ui.same_line_with_pos(col as f32 * cell_w + 4.0);
                    }

                    let base = i * 4;
                    let y_pos = ppu.oam[base];
                    let x_pos = ppu.oam[base + 1];
                    let tile_num = ppu.oam[base + 2];
                    let attr = ppu.oam[base + 3];

                    let pos0 = ui.cursor_screen_pos();

                    ui.group(|| {
                        if let Some(tex_id) = self.oam_sprite_textures[i] {
                            imgui::Image::new(tex_id, [tile_draw_w, tile_draw_h]).build(ui);
                        } else {
                            ui.dummy([tile_draw_w, tile_draw_h]);
                        }

                        ui.text(format!("{:02X}", tile_num));
                        ui.text(format!("{:02X}", y_pos));
                        ui.text(format!("{:02X}", x_pos));
                        ui.text(format!("{:02X}", attr));
                    });

                    let pos1 = [pos0[0] + cell_w - 4.0, pos0[1] + cell_h - 4.0];

                    let clicked = ui.is_item_clicked();
                    if clicked {
                        self.oam_selected = i;
                    }

                    if self.oam_selected == i {
                        let draw_list = ui.get_window_draw_list();
                        draw_list
                            .add_rect(
                                [pos0[0] - 2.0, pos0[1] - 2.0],
                                [pos1[0] + 2.0, pos1[1] + 2.0],
                                imgui::ImColor32::from_rgb(0xFF, 0xFF, 0x00),
                            )
                            .thickness(2.0)
                            .build();
                    }

                    if col == COLS - 1 {
                        ui.dummy([0.0, 4.0]);
                    }
                }
            });
    }

    fn draw_oam_details(&mut self, ui: &imgui::Ui, ppu: &PpuSnapshot, sprite_h: u8) {
        let i = self.oam_selected;
        let base = i * 4;
        let y_pos = ppu.oam[base];
        let x_pos = ppu.oam[base + 1];
        let mut tile_num = ppu.oam[base + 2];
        let attr = ppu.oam[base + 3];

        if sprite_h == 16 {
            tile_num &= 0xFE;
        }

        let x_flip = attr & 0x20 != 0;
        let y_flip = attr & 0x40 != 0;
        let priority = attr & 0x80 != 0;

        let (pal_str, bank) = if ppu.cgb {
            let pal_idx = attr & 0x07;
            let bank = if attr & 0x08 != 0 { 1 } else { 0 };
            (format!("OBJ {}", pal_idx), bank)
        } else {
            let pal = if attr & 0x10 != 0 { 1 } else { 0 };
            (format!("OBJ {}", pal), 0u8)
        };

        let oam_addr = 0xFE00 + (i as u16 * 4);
        let tile_addr = (tile_num as u16) * 16 + 0x8000;

        ui.text("Details");
        ui.separator();

        if let Some(tex_id) = self.oam_sprite_textures[i] {
            let scale = 4.0f32;
            let draw_w = 8.0 * scale;
            let draw_h = sprite_h as f32 * scale;
            imgui::Image::new(tex_id, [draw_w, draw_h]).build(ui);
        }

        ui.separator();

        if let Some(_t) = ui.begin_table_with_flags(
            "oam_details",
            2,
            imgui::TableFlags::SIZING_FIXED_FIT | imgui::TableFlags::NO_SAVED_SETTINGS,
        ) {
            let row = |ui: &imgui::Ui, label: &str, value: String| {
                ui.table_next_row();
                ui.table_next_column();
                ui.text(label);
                ui.table_next_column();
                ui.text(value);
            };

            row(ui, "X-loc", format!("{:02X}", x_pos));
            row(ui, "Y-loc", format!("{:02X}", y_pos));
            row(ui, "Tile No", format!("{:02X}", tile_num));
            row(ui, "Attribute", format!("{:02X}", attr));
            row(ui, "OAM addr", format!("{:04X}", oam_addr));
            row(ui, "Tile Address", format!("{:X}:{:04X}", bank, tile_addr));

            ui.table_next_row();
            ui.table_next_column();
            ui.text("X-flip");
            ui.table_next_column();
            let mut x_flip_val = x_flip;
            ui.checkbox("##xflip", &mut x_flip_val);

            ui.table_next_row();
            ui.table_next_column();
            ui.text("Y-flip");
            ui.table_next_column();
            let mut y_flip_val = y_flip;
            ui.checkbox("##yflip", &mut y_flip_val);

            ui.table_next_row();
            ui.table_next_column();
            ui.text("Priority");
            ui.table_next_column();
            let mut priority_val = priority;
            ui.checkbox("##priority", &mut priority_val);

            ui.table_next_row();
            ui.table_next_column();
            ui.text("Palette");
            ui.table_next_column();
            ui.text(&pal_str);
        }
    }
    fn draw_palettes(
        &mut self,
        ui: &imgui::Ui,
        _renderer: &mut Renderer,
        ppu: &PpuSnapshot,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) {
        let bg_pals = if ppu.cgb { 8 } else { 1 };
        let ob_pals = if ppu.cgb { 8 } else { 2 };

        // Keep selection within valid ranges.
        if self.palette_sel_is_bg {
            self.palette_sel_pal = self.palette_sel_pal.min((bg_pals - 1) as u8);
        } else {
            self.palette_sel_pal = self.palette_sel_pal.min((ob_pals - 1) as u8);
        }
        self.palette_sel_col = self.palette_sel_col.min(3);

        let draw_swatch = |ui: &imgui::Ui,
                           this: &mut Self,
                           ppu: &PpuSnapshot,
                           is_bg: bool,
                           pal: usize,
                           col: usize| {
            let rgb = Self::palette_color_rgb(ppu, is_bg, pal, col);
            let word = Self::rgb888_to_gb_word(rgb);
            let r = ((rgb >> 16) & 0xFF) as u8;
            let g = ((rgb >> 8) & 0xFF) as u8;
            let b = (rgb & 0xFF) as u8;

            let pos0 = ui.cursor_screen_pos();
            let square = [22.0, 22.0];
            let id = format!(
                "##pal_{}_{}_{}_{}",
                if is_bg { 'b' } else { 'o' },
                pal,
                col,
                0
            );
            let clicked = ui.invisible_button(id, square);
            let pos1 = [pos0[0] + square[0], pos0[1] + square[1]];

            let selected = this.palette_sel_is_bg == is_bg
                && this.palette_sel_pal as usize == pal
                && this.palette_sel_col as usize == col;
            let draw_list = ui.get_window_draw_list();
            draw_list
                .add_rect(pos0, pos1, imgui::ImColor32::from_rgb(r, g, b))
                .filled(true)
                .build();
            draw_list
                .add_rect(pos0, pos1, imgui::ImColor32::from_rgb(0xFF, 0xFF, 0xFF))
                .build();
            if selected {
                draw_list
                    .add_rect(
                        [pos0[0] - 1.0, pos0[1] - 1.0],
                        [pos1[0] + 1.0, pos1[1] + 1.0],
                        imgui::ImColor32::from_rgb(0xFF, 0xFF, 0xFF),
                    )
                    .thickness(2.0)
                    .build();
            }

            let text_pos = [pos0[0], pos0[1] + square[1] + 2.0];
            ui.set_cursor_screen_pos(text_pos);
            ui.text(format!("{:04X}", word));

            if clicked {
                this.palette_sel_is_bg = is_bg;
                this.palette_sel_pal = pal as u8;
                this.palette_sel_col = col as u8;
            }
        };

        let layout_flags = imgui::TableFlags::SIZING_STRETCH_PROP
            | imgui::TableFlags::NO_SAVED_SETTINGS
            | imgui::TableFlags::BORDERS_INNER_V;

        if let Some(_layout) = ui.begin_table_with_flags("pal_layout", 3, layout_flags) {
            ui.table_next_row();

            // BG palettes
            ui.table_next_column();
            let pal_table_flags = imgui::TableFlags::SIZING_FIXED_FIT
                | imgui::TableFlags::NO_SAVED_SETTINGS
                | imgui::TableFlags::BORDERS_INNER_V;
            if let Some(_bg) = ui.begin_table_with_flags("bg_pals", 5, pal_table_flags) {
                for pal in 0..bg_pals {
                    ui.table_next_row();
                    ui.table_next_column();
                    ui.text(format!("BG {pal}"));
                    for col in 0..4 {
                        ui.table_next_column();
                        draw_swatch(ui, self, ppu, true, pal, col);
                    }
                }
            }

            // OBJ palettes
            ui.table_next_column();
            if let Some(_ob) = ui.begin_table_with_flags("ob_pals", 5, pal_table_flags) {
                for pal in 0..ob_pals {
                    ui.table_next_row();
                    ui.table_next_column();
                    ui.text(format!("OBJ {pal}"));
                    for col in 0..4 {
                        ui.table_next_column();
                        draw_swatch(ui, self, ppu, false, pal, col);
                    }
                }
            }

            // Selected color details
            ui.table_next_column();
            let is_bg = self.palette_sel_is_bg;
            let pal = self.palette_sel_pal as usize;
            let col = self.palette_sel_col as usize;
            let rgb = Self::palette_color_rgb(ppu, is_bg, pal, col);
            let word = Self::rgb888_to_gb_word(rgb);
            let r5 = (word & 0x1F) as u8;
            let g5 = ((word >> 5) & 0x1F) as u8;
            let b5 = ((word >> 10) & 0x1F) as u8;

            ui.text(if is_bg { "BG" } else { "OBJ" });
            ui.same_line();
            ui.text(format!("{}", pal));
            ui.same_line();
            ui.text(format!("[{}]", col));
            ui.separator();

            // Preview swatch
            let pos0 = ui.cursor_screen_pos();
            let square = [40.0, 40.0];
            let r = ((rgb >> 16) & 0xFF) as u8;
            let g = ((rgb >> 8) & 0xFF) as u8;
            let b = (rgb & 0xFF) as u8;
            let pos1 = [pos0[0] + square[0], pos0[1] + square[1]];
            let draw_list = ui.get_window_draw_list();
            draw_list
                .add_rect(pos0, pos1, imgui::ImColor32::from_rgb(r, g, b))
                .filled(true)
                .build();
            draw_list
                .add_rect(pos0, pos1, imgui::ImColor32::from_rgb(0xFF, 0xFF, 0xFF))
                .build();
            ui.dummy(square);

            ui.text(format!("{:04X}", word));
            ui.separator();

            if let Some(_t) = ui.begin_table("rgb", 2) {
                ui.table_next_row();
                ui.table_next_column();
                ui.text("Red:");
                ui.table_next_column();
                ui.text(format!("{:02X}", r5));

                ui.table_next_row();
                ui.table_next_column();
                ui.text("Green:");
                ui.table_next_column();
                ui.text(format!("{:02X}", g5));

                ui.table_next_row();
                ui.table_next_column();
                ui.text("Blue:");
                ui.table_next_column();
                ui.text(format!("{:02X}", b5));
            }

            ui.separator();
            if ui.button("copy dw") {
                let vals: Vec<String> = (0..4)
                    .map(|c| format!("${:04X}", Self::palette_color_word(ppu, is_bg, pal, c)))
                    .collect();
                let text = format!("dw {}", vals.join(", "));
                ui.set_clipboard_text(text);
            }
        }
    }
}
