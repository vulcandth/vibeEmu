use imgui::*;
use imgui_wgpu::Renderer;
use wgpu::{Device, Queue, TextureFormat};

use crate::mmu::Mmu;
use crate::ppu::Ppu;

/// Size of a single Game Boy tile in pixels (always 8×8).
const TILE_PX: usize = 8;
/// Background map is 32×32 tiles.
const BG_SIZE_TILES: usize = 32;
/// Final BG texture is 256×256 px
const BG_PX: usize = BG_SIZE_TILES * TILE_PX;

/// A texture we hand to ImGui that mirrors the current BG map.
struct BgTexture {
    id: TextureId,
    /// Increment this when VRAM changes so [`Renderer::update_texture`] runs O(n)
    version: u64,
}

/// High-level state for the VRAM viewer window.
pub struct VramViewer {
    pub open: bool,
    /// Which tab is currently visible.
    tab: usize,
    /// User toggles.
    show_grid: bool,
    scx_scxy_overlay: bool,
    show_palette: bool,
    /// Cached BG map texture (only recreated when VRAM changes).
    bg: Option<BgTexture>,
}

impl VramViewer {
    pub fn new() -> Self {
        Self {
            open: false,
            tab: 0,
            show_grid: true,
            scx_scxy_overlay: true,
            show_palette: true,
            bg: None,
        }
    }

    // Call every frame from the main UI loop.
    pub fn ui(
        &mut self,
        ui: &Ui,
        ppu: &Ppu,
        mmu: &Mmu,
        device: &Device,
        queue: &Queue,
        renderer: &mut Renderer,
    ) {
        if !self.open {
            return;
        }
        ui.window("VRAM Viewer")
            .size([560.0, 430.0], Condition::FirstUseEver)
            .build(|| {
                Self::top_tabs(ui, |tab| self.tab = tab);
                match self.tab {
                    0 => self.bg_map_tab(ui, ppu, mmu, device, queue, renderer),
                    1 => self.tiles_tab(ui),
                    2 => self.oam_tab(ui),
                    3 => self.palettes_tab(ui),
                    _ => {}
                }
            });
    }

    /// Render tab bar.
    fn top_tabs(ui: &Ui, mut pick: impl FnMut(usize)) {
        TabBar::new("vram_tabs").build(ui, || {
            if TabItem::new("BG map").build(ui, || {}).is_some() {
                pick(0);
            }
            if TabItem::new("Tiles").build(ui, || {}).is_some() {
                pick(1);
            }
            if TabItem::new("OAM").build(ui, || {}).is_some() {
                pick(2);
            }
            if TabItem::new("Palettes").build(ui, || {}).is_some() {
                pick(3);
            }
        });
    }

    // ────────────────────────────── BG MAP TAB ─────────────────────────────
    fn bg_map_tab(
        &mut self,
        ui: &Ui,
        ppu: &Ppu,
        mmu: &Mmu,
        device: &Device,
        queue: &Queue,
        renderer: &mut Renderer,
    ) {
        // Ensure texture is up-to-date
        let vram_revision = ppu.vram_revision(); // <- expose this getter
        let needs_upload = self
            .bg
            .as_ref()
            .map(|b| b.version != vram_revision)
            .unwrap_or(true);

        if needs_upload {
            let tex = self.upload_bg_texture(ppu, mmu, device, queue, renderer);
            self.bg = Some(BgTexture {
                id: tex,
                version: vram_revision,
            });
        }

        // Split window: left = map, right = info sidebar
        ui.columns(2, "bg_columns", true);
        {
            // --- Left: big texture preview ---
            let avail = ui.content_region_avail();
            let size = avail[0].min(avail[1]);
            if let Some(bg) = &self.bg {
                Image::new(bg.id, [size, size]).build(ui);
                if self.show_grid {
                    // Overlay grid lines
                    let draw = ui.get_window_draw_list();
                    let top_left = ui.cursor_screen_pos();
                    let step = size / BG_SIZE_TILES as f32;
                    for i in 0..=BG_SIZE_TILES {
                        let x = top_left[0] + i as f32 * step;
                        let y = top_left[1] + i as f32 * step;
                        let _ = draw
                            .add_line(
                                [top_left[0], y],
                                [top_left[0] + size, y],
                                [1.0, 1.0, 1.0, 0.25],
                            )
                            .thickness(1.0);
                        let _ = draw
                            .add_line(
                                [x, top_left[1]],
                                [x, top_left[1] + size],
                                [1.0, 1.0, 1.0, 0.25],
                            )
                            .thickness(1.0);
                    }
                }
                // TODO: mouse-over tile details (see sidebar sketch below)
            }
        }
        ui.next_column();
        {
            // --- Right: Details panel ---
            ui.text("Details");
            ui.separator();
            ui.checkbox("Grid", &mut self.show_grid);
            ui.checkbox("scx/y", &mut self.scx_scxy_overlay);
            ui.checkbox("pal", &mut self.show_palette);
            ui.separator();
            ui.text(format!("Map base: ${:04X}", ppu.bg_map_base()));
            ui.text(format!("Tileset base: ${:04X}", ppu.bg_tiles_base()));
            // (Add more fields here as needed)
        }
        ui.columns(1, "", false);
    }

    /// Generate a 256×256 RGBA texture representing the full BG map.
    ///
    /// NOTE: *Slow but easy to reason about.* Feel free to SIMD/parallelise later.
    fn upload_bg_texture(
        &self,
        ppu: &Ppu,
        mmu: &Mmu,
        device: &Device,
        queue: &Queue,
        renderer: &mut Renderer,
    ) -> TextureId {
        let mut rgba = vec![0u8; BG_PX * BG_PX * 4];
        let map_base = ppu.bg_map_base();

        for ty in 0..BG_SIZE_TILES {
            for tx in 0..BG_SIZE_TILES {
                let map_index = (ty * BG_SIZE_TILES + tx) as u16;
                let tile_no = mmu.vram_read(map_base + map_index);
                let attr = if mmu.is_cgb() {
                    mmu.vram_read(map_base + 0x2000 + map_index)
                } else {
                    0
                };

                // Decode tile into 8×8 px RGBA
                let tile_rgba = ppu.decode_bg_tile_rgba(tile_no, attr, mmu);
                let dst_y = ty * TILE_PX;
                let dst_x = tx * TILE_PX;
                for py in 0..TILE_PX {
                    let dst_offset = ((dst_y + py) * BG_PX + dst_x) * 4;
                    rgba[dst_offset..dst_offset + TILE_PX * 4]
                        .copy_from_slice(&tile_rgba[py * TILE_PX * 4..(py + 1) * TILE_PX * 4]);
                }
            }
        }

        // Upload / update ImGui texture
        let tex = imgui_wgpu::Texture::new(
            device,
            renderer,
            imgui_wgpu::TextureConfig {
                size: wgpu::Extent3d {
                    width: BG_PX as u32,
                    height: BG_PX as u32,
                    depth_or_array_layers: 1,
                },
                format: Some(TextureFormat::Rgba8UnormSrgb),
                ..Default::default()
            },
        );
        tex.write(queue, &rgba, BG_PX as u32, BG_PX as u32);
        renderer.textures.insert(tex)
    }

    // ────────────────────────────── STUB TABS ──────────────────────────────
    fn tiles_tab(&self, ui: &Ui) {
        ui.text("Tiles tab – TBD");
    }
    fn oam_tab(&self, ui: &Ui) {
        ui.text("OAM tab – TBD");
    }
    fn palettes_tab(&self, ui: &Ui) {
        ui.text("Palettes tab – TBD");
    }
}
