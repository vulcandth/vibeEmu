use crate::models::GameBoyModel;

// DMG Palette Colors (example, can be adjusted)
// Using a common greenish palette.
pub const DMG_WHITE: [u8; 3] = [224, 248, 208];
pub const DMG_LIGHT_GRAY: [u8; 3] = [136, 192, 112];
pub const DMG_DARK_GRAY: [u8; 3] = [52, 104, 86];
pub const DMG_BLACK: [u8; 3] = [8, 24, 32];

// Constants for PPU
pub const VRAM_SIZE_CGB: usize = 16 * 1024; // 2 banks of 8KB
pub const VRAM_SIZE_DMG: usize = 8 * 1024;  // 1 bank of 8KB
pub const OAM_SIZE: usize = 160;           // Object Attribute Memory
pub const CGB_PALETTE_RAM_SIZE: usize = 64; // Background and Sprite palettes (32 palettes * 2 bytes/color * 4 colors/palette is not right, it's 64 bytes total for CRAM)

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

// PPU Cycle Constants
pub const OAM_SCAN_CYCLES: u32 = 80;
// pub const DRAWING_CYCLES: u32 = 172; // Placeholder, will vary // REMOVED
// pub const HBLANK_CYCLES: u32 = 204; // 456 - 80 - 172 = 204 // REMOVED
pub const SCANLINE_CYCLES: u32 = 456; // OAM_SCAN + DRAWING + HBLANK
pub const VBLANK_LINES: u8 = 10; // LY 144-153
pub const VBLANK_DURATION_CYCLES: u32 = SCANLINE_CYCLES * VBLANK_LINES as u32; // 4560 cycles
pub const TOTAL_LINES: u8 = 154; // 144 visible + 10 VBlank

// PPU Modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PpuMode {
    HBlank = 0, // Mode 0
    VBlank = 1, // Mode 1
    OamScan = 2, // Mode 2
    Drawing = 3, // Mode 3
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelSource {
    Background,
    Object { palette_register: u8 }, // 0 for OBP0, 1 for OBP1 (DMG)
}

pub struct Ppu {
    // Memory
    pub vram: Vec<u8>, // Video RAM (0x8000-0x9FFF) - Can be 1 or 2 banks for CGB
    pub oam: [u8; OAM_SIZE],  // Object Attribute Memory (0xFE00-0xFE9F)

    // CGB Specific Memory
    pub cgb_background_palette_ram: [u8; CGB_PALETTE_RAM_SIZE], // FF69 - BCPD/BGPD
    pub cgb_sprite_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],     // FF6B - OCPD/OBPD

    // Registers - values are typically stored here or directly in Bus's io_registers map
    // For now, let's assume the Bus will forward relevant register values to PPU methods,
    // but some core ones might be mirrored or have internal PPU state.
    pub lcdc: u8, // LCD Control (0xFF40)
    pub stat: u8, // LCD Status (0xFF41)
    pub scy: u8,  // Scroll Y (0xFF42)
    pub scx: u8,  // Scroll X (0xFF43)
    pub ly: u8,   // LCD Y Coordinate (0xFF44) - Current scanline
    pub lyc: u8,  // LY Compare (0xFF45)
    pub wy: u8,   // Window Y Position (0xFF4A)
    pub wx: u8,   // Window X Position minus 7 (0xFF4B)

    pub bgp: u8,  // BG Palette Data (0xFF47) - DMG
    pub obp0: u8, // Object Palette 0 Data (0xFF48) - DMG
    pub obp1: u8, // Object Palette 1 Data (0xFF49) - DMG

    // CGB Specific Registers (values might be passed from Bus or stored if PPU directly handles them)
    pub vbk: u8,      // VRAM Bank Select (0xFF4F) - CGB only (bit 0)
    pub bcps_bcpi: u8, // Background Palette Index (0xFF68) - CGB only
    pub bcpd_bgpd: u8, // Background Palette Data (0xFF69) - CGB only (written through bcps_bcpi auto-increment)
    pub ocps_ocpi: u8, // Sprite Palette Index (0xFF6A) - CGB only
    pub ocpd_obpd: u8, // Sprite Palette Data (0xFF6B) - CGB only (written through ocps_ocpi auto-increment)

    // PPU State
    pub current_mode: PpuMode, // Made public for bus/debugger inspection
    pub cycles_in_mode: u32, // Cycles accumulated in the current mode for the current line

    // Interrupt flags (to be read by the interrupt controller via Bus)
    pub vblank_interrupt_requested: bool,
    pub stat_interrupt_requested: bool, // General flag for all STAT conditions

    // STAT interrupt condition flags (internal tracking, helps generate stat_interrupt_requested)
    // These are not strictly necessary if logic in tick directly sets stat_interrupt_requested
    // but can be useful for debugging or more complex interrupt logic.
    // For now, we'll try to manage stat_interrupt_requested directly.

    // HDMA HBlank signal
    pub just_entered_hblank: bool,

    // Framebuffer to store rendered pixels
    pub framebuffer: Vec<u8>, // Stores RGB888, 160*144*3 bytes

    // Temporary buffer for the current scanline's raw 2-bit color indices
    // This will be filled by background, window, and sprite rendering logic.
    pub current_scanline_color_indices: [u8; SCREEN_WIDTH],

    // Internal state based on SameBoy for more detailed emulation later
    // For now, these are placeholders or simplified.

    // SameBoy's `GB_display_s` related:
    // gb->current_line; -> covered by ly (or an internal version if LY has specific read behavior)
    // gb->ly_for_comparison;
    // gb->wy_triggered;
    // gb->window_y;
    // gb->lcd_x; // internal x position for rendering

    // gb->fetcher_state;
    // gb->bg_fifo, gb->oam_fifo;
    // gb->n_visible_objs, gb->visible_objs[], gb->objects_x[], gb->objects_y[];

    // OAM Scan related
    visible_oam_entries: Vec<OamEntryData>, // Stores sprites visible on the current scanline
    line_sprite_data: Vec<FetchedSpritePixelLine>, // Stores fetched pixel data for sprites on the current line

    // Scanline composition buffer
    current_scanline_pixel_source: [PixelSource; SCREEN_WIDTH],

    // Mode 3 timing
    current_mode3_drawing_cycles: u32,

    model: GameBoyModel,
}

/// Stores essential information for a sprite found during OAM scan
/// that is visible on the current scanline.
#[derive(Debug, Clone, Copy)]
pub struct OamEntryData {
    pub oam_index: usize,  // Original index in OAM (0-39)
    pub y_pos: u8,         // Sprite Y position from OAM
    pub x_pos: u8,         // Sprite X position from OAM
    pub tile_index: u8,    // Sprite tile index from OAM
    pub attributes: u8,    // Sprite attributes byte from OAM
}

/// Stores the processed pixel data for a single line of a sprite, ready for rendering.
#[derive(Debug, Clone)] // Clone needed if we copy it around
pub struct FetchedSpritePixelLine {
    pub x_pos: u8,         // Screen X-coordinate where the sprite begins
    pub attributes: u8,    // OAM attributes: palette, priority, flips, CGB bank
    pub pixels: [u8; 8],   // Array of 8 color indices (0-3) for the sprite line
    pub oam_index: usize,  // Original OAM index
}

// Conceptual BG Rendering Info
// This is a temporary struct to hold results for demonstration
#[derive(Debug, Default, Clone, Copy)]
pub struct BgTileInfo {
    pub tile_number: u8,
    pub tile_data_addr_plane1: u16,
    pub tile_data_addr_plane2: u16,
    pub attributes_addr: u16, // VRAM address for attributes (CGB)
    pub attributes: u8,       // Attributes byte (CGB)
    pub tile_number_map_addr: u16,
}

impl Ppu {
    pub fn new(model: GameBoyModel) -> Self {
        let vram_size = if model.is_cgb_family() { VRAM_SIZE_CGB } else { VRAM_SIZE_DMG };
        Ppu {
            vram: vec![0; vram_size],
            oam: [0; OAM_SIZE],
            cgb_background_palette_ram: [0; CGB_PALETTE_RAM_SIZE],
            cgb_sprite_palette_ram: [0; CGB_PALETTE_RAM_SIZE],

            // Default register values at power-up (these are often set by BIOS if one runs)
            // Values here are typical post-boot values or safe defaults.
            // The bus will typically write the true initial values from I/O registers.
            lcdc: 0x91, // Display ON, BG ON, OBJ ON, Window OFF, BG/Win Tile Data #0, BG Tile Map #0, OBJ size 8x8
            stat: 0x80, // Bits 7,0-1 are readable. Bit 7 is always 1. Mode bits will be set by logic. LYC=LY initially false.
            scy: 0x00,
            scx: 0x00,
            ly: 0x00,
            lyc: 0x00,
            wy: 0x00,
            wx: 0x00,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,

            vbk: 0xFE,
            bcps_bcpi: 0x00,
            bcpd_bgpd: 0x00,
            ocps_ocpi: 0x00,
            ocpd_obpd: 0x00,

            current_mode: PpuMode::OamScan, // Initial mode after power-on/reset often OAM Scan for line 0
            cycles_in_mode: 0,

            vblank_interrupt_requested: false,
            stat_interrupt_requested: false,
            just_entered_hblank: false,

            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT * 3], // RGB888
            current_scanline_color_indices: [0; SCREEN_WIDTH],

            visible_oam_entries: Vec::with_capacity(10), // Max 10 sprites per line
            line_sprite_data: Vec::with_capacity(10),    // Max 10 sprites per line

            current_scanline_pixel_source: [PixelSource::Background; SCREEN_WIDTH],

            current_mode3_drawing_cycles: 172, // Default/initial value

            model,
        }
    }

    // Basic read/write methods for VRAM and OAM (will be called by Bus)
    // These will need to respect PPU modes and CGB banking later.

    pub fn read_vram(&self, addr: u16) -> u8 {
        // VRAM access restrictions:
        // Inaccessible during Mode 3 (Drawing) if LCD is ON.
        if (self.lcdc & 0x80) != 0 && self.current_mode == PpuMode::Drawing {
            return 0xFF;
        }

        let vram_bank_offset = if self.model.is_cgb_family() && (self.vbk & 0x01) == 1 {
            VRAM_SIZE_DMG // Bank 1 is the second 8KB chunk
        } else {
            0 // Bank 0
        };
        // Address is 0x8000-0x9FFF, so subtract 0x8000 for index
        let index = (addr as usize - 0x8000) + vram_bank_offset;
        if index < self.vram.len() {
            self.vram[index]
        } else {
            0xFF // Should not happen if bank logic is correct
        }
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        // VRAM access restrictions:
        // Inaccessible during Mode 3 (Drawing) if LCD is ON.
        if (self.lcdc & 0x80) != 0 && self.current_mode == PpuMode::Drawing {
            return;
        }

        let vram_bank_offset = if self.model.is_cgb_family() && (self.vbk & 0x01) == 1 {
            VRAM_SIZE_DMG
        } else {
            0
        };
        let index = (addr as usize - 0x8000) + vram_bank_offset;
        if index < self.vram.len() {
            self.vram[index] = value;
        }
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        // OAM access restrictions:
        // Inaccessible during Mode 2 (OAM Scan) and Mode 3 (Drawing) if LCD is ON.
        if (self.lcdc & 0x80) != 0 &&
           (self.current_mode == PpuMode::OamScan || self.current_mode == PpuMode::Drawing) {
            return 0xFF;
        }
        // Address is 0xFE00-0xFE9F, so subtract 0xFE00 for index
        self.oam[addr as usize - 0xFE00]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        // OAM access restrictions for CPU access:
        // Inaccessible during Mode 2 (OAM Scan) and Mode 3 (Drawing) if LCD is ON.
        if (self.lcdc & 0x80) != 0 &&
           (self.current_mode == PpuMode::OamScan || self.current_mode == PpuMode::Drawing) {
            // println!("DEBUG: CPU OAM write to {:04X} blocked due to PPU mode {:?} (LCDC On)", addr, self.current_mode);
            return;
        }
        // println!("DEBUG: CPU OAM write to {:04X} with value {:02X} allowed. Mode: {:?}, LCDC: {:02X}", addr, value, self.current_mode, self.lcdc);
        self.oam[addr as usize - 0xFE00] = value;
    }

    // Special OAM write method for DMA, bypassing PPU mode restrictions.
    pub fn write_oam_dma(&mut self, addr: u16, value: u8) {
        // DMA has direct access to OAM, regardless of PPU mode.
        // Ensure address is within OAM range, though DMA logic should already handle this.
        let oam_index = addr as usize - 0xFE00;
        if oam_index < OAM_SIZE {
            self.oam[oam_index] = value;
        } else {
            // This should not happen if DMA source address and length are correct.
            // eprintln!("DMA OAM write out of bounds: {:04X}", addr);
        }
    }

    // TODO: Implement CGB palette RAM read/write methods
    // read_cgb_palette_ram(is_sprite_palette: bool, index: u8, auto_increment: bool) -> u8
    // write_cgb_palette_ram(is_sprite_palette: bool, index: u8, value: u8, auto_increment: bool)

    pub fn tick(&mut self, cpu_t_cycles: u32) {
        self.just_entered_hblank = false; // Reset at the beginning of each tick

        // If LCD is off, PPU is mostly idle
        if (self.lcdc & 0x80) == 0 { // LCDC Bit 7: LCD Display Enable
            self.ly = 0;
            self.cycles_in_mode = 0;
            self.current_mode = PpuMode::HBlank; // Or OAMScan; Pandocs: LY is reset, mode becomes Mode 0.
            // STAT mode bits should reflect HBlank (00). LYC=LY flag might change.
            // Preserve interrupt enable bits (3-6) and LYC flag (2). Bit 7 always 1.
            self.stat = (self.stat & 0xFC) | (PpuMode::HBlank as u8); // Update mode bits
            if self.ly == self.lyc { // Update LYC=LY coincidence flag
                self.stat |= 1 << 2;
            } else {
                self.stat &= !(1 << 2);
            }
            self.stat |= 0x80; // Ensure bit 7 is set

            self.vblank_interrupt_requested = false;
            self.stat_interrupt_requested = false; // No STAT interrupts when LCD is off (except maybe LYC during power off?)
                                               // For simplicity, clear STAT interrupts.
            return;
        }

        self.cycles_in_mode += cpu_t_cycles;
        let previous_stat_interrupt_line = self.eval_stat_interrupt_conditions();

        match self.current_mode {
            PpuMode::OamScan => {
                if self.cycles_in_mode >= OAM_SCAN_CYCLES {
                    // OAM Scan logic itself is performed "at once" conceptually at the start of Mode 2.
                    // The 80 cycles are for the PPU to complete this scan.
                    // The actual collection of sprites should happen when entering Mode 2 or at its very beginning.
                    // However, since the mode transition happens, and then this block is executed,
                    // we can perform the scan here, assuming it effectively happens before Drawing.

                    // The visible_oam_entries should be cleared upon *entering* OAMScan mode.
                    // This is handled in HBlank and VBlank transitions to OamScan.

                    // Perform the OAM scan
                    // LCDC Bit 1: OBJ Display Enable - if off, sprites are not processed/displayed.
                    // The scan might still happen and consume time, but results in no visible sprites.
                    // For now, we'll assume if LCDC Bit 1 is off, visible_oam_entries remains empty or is not used.
                    // The problem description implies we should collect them if LCD is on.
                    // The check for LCDC Bit 1 (OBJ enable) will be done during Drawing mode before rendering sprites.
                    // Here, we just gather potential sprites.

                    let sprite_height = if (self.lcdc & 0x04) != 0 { 16 } else { 8 }; // LCDC Bit 2: 0=8x8, 1=8x16

                    // Iterate through all 40 OAM entries. Each entry is 4 bytes.
                    for i in 0..40 {
                        if self.visible_oam_entries.len() >= 10 {
                            break; // Hardware limit: max 10 sprites per scanline
                        }

                        let entry_addr = i * 4;
                        // OAM is directly accessible here by the PPU, not via read_oam/write_oam CPU methods.
                        let y_pos = self.oam[entry_addr];      // Byte 0: Y position
                        let x_pos = self.oam[entry_addr + 1];  // Byte 1: X position
                        let tile_idx = self.oam[entry_addr + 2]; // Byte 2: Tile Index
                        let attributes = self.oam[entry_addr + 3]; // Byte 3: Attributes

                        // Sprite Y position is Y coordinate on screen + 16.
                        // A sprite is visible if current scanline (self.ly) is within its Y range.
                        // self.ly is 0-143. y_pos is sprite's top edge.
                        // Check if the sprite is on the current scanline:
                        // self.ly >= (y_pos - 16) && self.ly < (y_pos - 16 + sprite_height)
                        // Simplified: sprite_y_on_screen = y_pos - 16
                        // visible_on_line = (self.ly >= sprite_y_on_screen) && (self.ly < sprite_y_on_screen + sprite_height)

                        // Per Pandocs: "The Y coordinate is Y + 16, so to place a sprite at the top of the screen at Y=0, set its Y coordinate to 16."
                        // "A sprite is visible (takes part in the rendering process) if LY + 16 >= SpriteY AND LY + 16 < SpriteY + SpriteHeight."
                        // This means y_pos directly from OAM is the value to use in this formula.
                        let current_ly_plus_16 = self.ly.wrapping_add(16); // Use wrapping_add for robustness, though unlikely to wrap here.

                        if current_ly_plus_16 >= y_pos && current_ly_plus_16 < y_pos.wrapping_add(sprite_height) {
                            // Sprite is vertically on this scanline.
                            // Also check if sprite is on screen horizontally (X > 0). X=0 means offscreen to the left.
                            // Per Pandocs: "The X coordinate is X + 8, so to place a sprite at the left of the screen at X=0, set its X coordinate to 8."
                            // "Sprites with X coordinate 0 or >= 168 (160+8) are not visible."
                            // So, an x_pos of 1 to 7 means partially off-screen but potentially visible.
                            // x_pos = 0 means fully off-screen to the left.
                            // x_pos = 168 (or 160+8) means fully off-screen to the right.
                            // We only add sprites that are at least partially on screen. x_pos > 0 ensures this.
                            if x_pos > 0 { // Sprite is at least partially on screen horizontally
                                self.visible_oam_entries.push(OamEntryData {
                                    oam_index: i,
                                    y_pos, // Store original OAM Y
                                    x_pos, // Store original OAM X
                                    tile_index: tile_idx,
                                    attributes,
                                });
                            }
                        }
                    }

                    // Sort visible sprites:
                    // Primary sort key: X-coordinate (ascending).
                    // Secondary sort key: OAM index (ascending, for sprites with same X-coordinate on DMG).
                    // This sorting helps in pixel-drawing priority for DMG.
                    self.visible_oam_entries.sort_unstable_by(|a, b| {
                        if a.x_pos != b.x_pos {
                            a.x_pos.cmp(&b.x_pos)
                        } else {
                            a.oam_index.cmp(&b.oam_index)
                        }
                    });


                    // Calculate Mode 3 drawing duration based on visible sprites
                    // This should use visible_oam_entries, which are determined by the end of OAM scan.
                    let base_cycles = 172; // Minimum cycles for Mode 3
                    let penalty_per_sprite = 6; // Estimated additional cycles per sprite.
                                               // This is a simplification. Real PPU timing is more complex (FIFO, etc.)

                    // Max of 10 sprites are in visible_oam_entries due to OAM scan hardware limit
                    let num_visible_sprites = self.visible_oam_entries.len() as u32;

                    let mut calculated_drawing_cycles = base_cycles + (num_visible_sprites * penalty_per_sprite);

                    // Clamp the cycles. Max value of 289 is a rough upper bound, typical is less.
                    // Pan Docs: Mode 3 is "approx. 175 to 290 cycles".
                    // Smallest number of sprites (0) gives base_cycles.
                    // Largest (10 sprites * 6 = 60) + 172 = 232. This is well within bounds.
                    // Let's use a slightly more generous max clamp from some sources, like 289.
                    // Minimum is typically around 168-172.
                    calculated_drawing_cycles = calculated_drawing_cycles.max(172).min(289);
                    self.current_mode3_drawing_cycles = calculated_drawing_cycles;

                    // Transition to Drawing mode
                    self.cycles_in_mode -= OAM_SCAN_CYCLES; // Consume the cycles for OAM scan
                    self.current_mode = PpuMode::Drawing;
                    self.stat = (self.stat & 0xF8) | (PpuMode::Drawing as u8);
                }
            }
            PpuMode::Drawing => {
                // Actual pixel rendering would happen across these ~172+ cycles.
                // For now, this is a single block. The logic here represents the output for the *entire* scanline.

                // 1. Render Background (and Window, eventually) to current_scanline_color_indices
                if (self.lcdc & 0x01) != 0 { // LCDC Bit 0: BG Display Enable
                    self.render_scanline_bg();
                } else {
                    // BG is disabled. Fill scanline with color 0.
                    // This will typically map to white via BGP.
                    for i in 0..SCREEN_WIDTH {
                        self.current_scanline_color_indices[i] = 0;
                    }
                }

                // TODO: Window rendering would overwrite parts of current_scanline_color_indices here.
                // --- Window Rendering ---
                // LCDC Bit 5: Window Display Enable
                if (self.lcdc & (1 << 5)) != 0 {
                    // TODO: Window rendering logic would go here.
                    // This involves checking if the current scanline self.ly is >= self.wy.
                    // And if the current pixel's x coordinate is >= self.wx - 7.
                    // PPU_TODO: Window internal line counter should only increment if Window Display is enabled (LCDC Bit 5).
                    // TODO: LCDC Bit 6: Window Tile Map Display Select (0=9800-9BFF, 1=9C00-9FFF) would be used here for tile map.
                    // LCDC Bit 4 (BG & Window Tile Data Select) is also used for Window tile data.
                }

                // --- OBJ Data Fetching ---
                self.line_sprite_data.clear(); // Clear data from previous line

                // LCDC Bit 1: OBJ Display Enable
                if (self.lcdc & (1 << 1)) != 0 { // Check if sprites are enabled
                    // visible_oam_entries is already sorted by X-coordinate, then OAM index.
                    for oam_entry in &self.visible_oam_entries { // Iterate over a slice to avoid borrow issues if needed
                        // Create a copy of oam_entry to pass to fetch_sprite_line_data
                        // This is because visible_oam_entries might be borrowed if fetch_sprite_line_data needs &mut self
                        // However, fetch_sprite_line_data current takes &self, so direct ref should be fine.
                        // Let's try with a direct reference first.
                        // If `oam_entry` is needed later or there are borrow checker issues, clone it: `*oam_entry`
                        if let Some(fetched_data) = self.fetch_sprite_line_data(oam_entry, self.ly) {
                            self.line_sprite_data.push(fetched_data);
                        }
                        // Hardware typically can only fetch so many sprites in time, but we've already limited to 10 in OAM scan.
                    }
                }
                // Now self.line_sprite_data contains all sprite pixel lines for the current scanline.
                // This data will be used for mixing/rendering onto the framebuffer.

                // --- Sprite Pixel Rendering and Mixing ---
                if (self.lcdc & 0x02) != 0 { // OBJ Display Enable (LCDC Bit 1)
                    let is_cgb_mode = self.model.is_cgb_family();

                    for sprite_line_info in &self.line_sprite_data {
                        let sprite_attributes = sprite_line_info.attributes;
                        let sprite_x_pos_oam = sprite_line_info.x_pos; // This is OAM X (screen X is X-8)
                        let dmg_palette_reg_idx = (sprite_attributes >> 4) & 1; // Bit 4 for OBP0/OBP1

                        for pixel_idx_in_sprite in 0..8 {
                            // Determine actual pixel from sprite data considering X-flip (OAM attribute bit 5)
                            let current_sprite_pixel_x_in_tile = if (sprite_attributes & 0x20) != 0 { // X-flip
                                7 - pixel_idx_in_sprite
                            } else {
                                pixel_idx_in_sprite
                            };
                            let sprite_color_index = sprite_line_info.pixels[current_sprite_pixel_x_in_tile];

                            // Color index 0 for sprites is transparent
                            if sprite_color_index == 0 {
                                continue;
                            }

                            // Calculate screen X coordinate for the current sprite pixel
                            // sprite_x_pos_oam is OAM X. Screen X is OAM X - 8.
                            let screen_x = (sprite_x_pos_oam as i16 - 8) + pixel_idx_in_sprite as i16;

                            if screen_x < 0 || screen_x >= SCREEN_WIDTH as i16 {
                                continue; // Off-screen
                            }
                            let screen_x_usize = screen_x as usize;

                            // Priority Logic
                            let bg_priority_over_sprite = (sprite_attributes & 0x80) != 0; // OAM Attribute Bit 7
                            let bg_pixel_color_index = self.current_scanline_color_indices[screen_x_usize];
                            // Get current source to see if another higher-priority sprite (lower OAM index) already drew here
                            let current_pixel_source = self.current_scanline_pixel_source[screen_x_usize];


                            let mut can_draw_sprite = false;

                            // DMG implicit priority: lower OAM index (earlier in line_sprite_data) wins over higher OAM index.
                            // This is handled by processing sprites in order and only drawing if current pixel is BG.
                            // If current_pixel_source is already Object, a higher-priority sprite is there.
                            if matches!(current_pixel_source, PixelSource::Background) {
                                if is_cgb_mode {
                                    // CGB Priority (Simplified placeholder as per instructions)
                                    // Proper CGB: LCDC bit 0 (BG/Win master priority), OAM bit 7, BG Map Attr bit 7
                                    // If LCDC.0 (BG Display Enable / Master Priority) is 0, Sprites always win over BG.
                                    // If LCDC.0 is 1, then BG Map Attr bit 7 and OAM Bit 7 are checked.
                                    // For now: if BG is enabled (LCDC.0), use OAM bit 7. If BG disabled, sprite wins.
                                    if (self.lcdc & 0x01) != 0 { // BG Master Priority / Display Enable
                                        if bg_priority_over_sprite { // OAM bit 7 (BG has priority over this sprite)
                                            if bg_pixel_color_index == 0 { // BG is transparent color 0
                                                can_draw_sprite = true;
                                            }
                                        } else { // Sprite has priority over BG (OAM bit 7 is false)
                                            can_draw_sprite = true;
                                        }
                                    } else { // BG is disabled, sprite always wins
                                        can_draw_sprite = true;
                                    }
                                } else { // DMG Mode
                                    if bg_priority_over_sprite { // OAM Bit 7 (BG has priority)
                                        if bg_pixel_color_index == 0 { // BG is transparent color 0
                                            can_draw_sprite = true;
                                        }
                                    } else { // Sprite has priority
                                        can_draw_sprite = true;
                                    }
                                }
                            }
                            // If `can_draw_sprite` is still false here, it means either:
                            // 1. A higher-priority sprite (lower OAM index for same X) already drew here.
                            // 2. BG has priority and its pixel is not color 0.

                            if can_draw_sprite {
                                self.current_scanline_color_indices[screen_x_usize] = sprite_color_index;
                                self.current_scanline_pixel_source[screen_x_usize] = PixelSource::Object {
                                    palette_register: dmg_palette_reg_idx,
                                };
                            }
                        }
                    }
                }

                // 2. Apply Palettes to Framebuffer (using the mixed scanline data)
                // This function will now use current_scanline_color_indices and current_scanline_pixel_source
                self.apply_dmg_palette_to_framebuffer();
                // Note: The CGB path inside apply_dmg_palette_to_framebuffer needs to be aware of this change or be CGB specific.
                // The instructions cover adding a CGB check to bypass this new logic for now.


                if self.cycles_in_mode >= self.current_mode3_drawing_cycles {
                    self.cycles_in_mode -= self.current_mode3_drawing_cycles;
                    self.current_mode = PpuMode::HBlank;
                    self.stat = (self.stat & 0xF8) | (PpuMode::HBlank as u8);
                    self.just_entered_hblank = true;
                    // TODO: HBlank HDMA Hook Placeholder.
                    // Actual transfer to an external screen buffer might happen here or after VBLANK.
                }
            }
            PpuMode::HBlank => {
                // HBlank duration is what's left of the scanline after OAM scan and Drawing.
                let expected_hblank_cycles = SCANLINE_CYCLES - OAM_SCAN_CYCLES - self.current_mode3_drawing_cycles;
                // Ensure expected_hblank_cycles is not negative if current_mode3_drawing_cycles was unexpectedly large.
                // Though current_mode3_drawing_cycles is clamped, so OAM_SCAN_CYCLES + current_mode3_drawing_cycles should not exceed SCANLINE_CYCLES.
                // Max current_mode3_drawing_cycles = 289. OAM_SCAN_CYCLES = 80. Sum = 369. SCANLINE_CYCLES = 456. So, hblank is at least 87.

                if self.cycles_in_mode >= expected_hblank_cycles {
                    self.cycles_in_mode -= expected_hblank_cycles; // Remainder cycles carry over
                    self.ly += 1;

                    if self.ly < SCREEN_HEIGHT as u8 { // 0-143
                        self.current_mode = PpuMode::OamScan;
                        self.stat = (self.stat & 0xF8) | (PpuMode::OamScan as u8);
                        self.visible_oam_entries.clear(); // Clear for the new scanline's OAM scan
                    } else { // ly == 144, transition to VBlank
                        self.current_mode = PpuMode::VBlank;
                        self.stat = (self.stat & 0xF8) | (PpuMode::VBlank as u8);
                        self.vblank_interrupt_requested = true;
                        // VBlank interrupt is requested once when entering VBlank mode
                    }
                }
            }
            PpuMode::VBlank => {
                if self.cycles_in_mode >= SCANLINE_CYCLES {
                    self.cycles_in_mode -= SCANLINE_CYCLES;
                    self.ly += 1;

                    if self.ly == TOTAL_LINES { // LY reaches 154 (144-153 are VBlank lines)
                        self.ly = 0; // Reset LY
                        self.current_mode = PpuMode::OamScan; // Start new frame
                        self.stat = (self.stat & 0xF8) | (PpuMode::OamScan as u8);
                        self.visible_oam_entries.clear(); // Clear for the new frame's first OAM scan (LY=0)
                    }
                    // LY remains 144 through 153 during VBlank.
                    // No mode change until LY wraps around.
                }
            }
        }

        // Update LYC=LY coincidence flag (STAT bit 2)
        if self.ly == self.lyc {
            self.stat |= 1 << 2;
        } else {
            self.stat &= !(1 << 2);
        }

        // Check and request STAT interrupts
        let current_stat_interrupt_line = self.eval_stat_interrupt_conditions();
        if !previous_stat_interrupt_line && current_stat_interrupt_line {
            self.stat_interrupt_requested = true;
        }
        // Note: The exact timing of stat_interrupt_requested going false needs to be handled
        // by the CPU/interrupt controller when the interrupt is acknowledged/serviced.
        // For now, we set it true if any enabled condition becomes active.
        // A more precise model might only set it true for one M-cycle or if the line rises.
    }

    // Helper function to evaluate STAT interrupt conditions
    // Returns true if any enabled STAT interrupt condition is met.
    fn eval_stat_interrupt_conditions(&self) -> bool {
        let mode_interrupt_enabled = match self.current_mode {
            PpuMode::HBlank => (self.stat & (1 << 3)) != 0, // Mode 0 HBlank Interrupt Enable
            PpuMode::VBlank => (self.stat & (1 << 4)) != 0, // Mode 1 VBlank Interrupt Enable (for STAT, not the main VBlank int)
            PpuMode::OamScan => (self.stat & (1 << 5)) != 0, // Mode 2 OAM Interrupt Enable
            PpuMode::Drawing => false, // Mode 3 has no specific STAT interrupt enable bit
        };

        let lyc_interrupt_enabled = (self.stat & (1 << 6)) != 0; // LYC=LY Coincidence Interrupt Enable
        let lyc_coincidence = (self.stat & (1 << 2)) != 0; // LYC=LY Flag is set

        // STAT interrupt is triggered if:
        // 1. LYC=LY coincidence is enabled AND LYC=LY flag is set
        // 2. Mode-specific interrupt is enabled AND the PPU is in that mode
        //    (Note: For VBlank mode, this is a STAT interrupt, distinct from the main VBlank interrupt.
        //     It can trigger on each line of VBlank if enabled.)
        if lyc_interrupt_enabled && lyc_coincidence {
            return true;
        }
        if mode_interrupt_enabled {
             // For Mode 1 (VBlank), the STAT interrupt can trigger on each line of VBlank if bit 4 is set.
             // For Mode 0 (HBlank) and Mode 2 (OAM), it triggers when entering the mode if enabled.
            return true;
        }
        false
    }

    // TODO: Interrupt request clear methods (called by Bus after servicing)
    pub fn clear_vblank_interrupt_request(&mut self) {
        self.vblank_interrupt_requested = false;
    }

    pub fn clear_stat_interrupt_request(&mut self) {
        self.stat_interrupt_requested = false;
    }

    pub fn write_stat(&mut self, value: u8) {
        // CPU can write to bits 3, 4, 5, 6 of STAT.
        // Bits 0, 1, 2 are read-only for CPU (set by PPU). Bit 7 is always 1.
        let old_stat_interrupt_line = self.eval_stat_interrupt_conditions();

        self.stat = (value & 0b01111000) | (self.stat & 0b10000111);
        // self.stat |= 0x80; // Ensure bit 7 is always 1 - already handled by PPU logic / initial value

        // Re-evaluate STAT interrupt conditions if relevant bits changed
        // An interrupt should be triggered if a condition becomes true AND its enable bit is set.
        // If an enable bit is cleared, and the condition was true, the interrupt line might go low.
        let new_stat_interrupt_line = self.eval_stat_interrupt_conditions();
        if !old_stat_interrupt_line && new_stat_interrupt_line {
            self.stat_interrupt_requested = true;
        }
        // Note: The specification for STAT interrupt generation upon writing to STAT is complex.
        // For example, if LYC=LY interrupt is enabled, and LYC is written to match LY,
        // an interrupt occurs. This logic is part of eval_stat_interrupt_conditions.
        // If writing to STAT disables an interrupt that was active, stat_interrupt_requested should ideally go false
        // if no other conditions are met. For now, this only sets it true on a rising edge.
        // A more robust solution might re-evaluate `self.stat_interrupt_requested = new_stat_interrupt_line;`
        // but this could lead to missed interrupts if not careful about edge vs. level triggering.
        // The current model in tick handles the rising edge for mode changes and LYC=LY.
        // Writing to STAT itself should also check this.
    }

    // Helper function for conceptual BG rendering logic
    // Takes x_on_screen (0-159) to determine which tile column to access.
    // y_on_screen is implicitly self.ly
    // This function assumes LCDC Bit 0 (BG Display Enable) is already checked and true.
    // fn get_bg_tile_info(&self, x_on_screen: u8) -> BgTileInfo { // Original function signature
    // This function is being replaced by fetch_tile_line_data and render_scanline_bg
    // The logic from get_bg_tile_info is being integrated into fetch_tile_line_data.
    // }

    // Helper to read from a specific VRAM bank, bypassing the PPU's current vbk state.
    // This is useful for fetching CGB attributes or specific tile data banks.
    fn read_vram_bank_agnostic(&self, addr: u16, bank: u8) -> u8 {
        if !self.model.is_cgb_family() && bank > 0 {
            // DMG only has bank 0
            return 0xFF;
        }

        let vram_bank_offset = if bank == 1 {
            VRAM_SIZE_DMG
        } else {
            0
        };

        let index = (addr as usize - 0x8000) + vram_bank_offset;
        if index < self.vram.len() {
            self.vram[index]
        } else {
             // This case should ideally not be hit if addresses and banks are calculated correctly.
            // eprintln!("Warning: VRAM read out of bounds. Addr: {:#06X}, Bank: {}, Index: {}, VRAM size: {}", addr, bank, index, self.vram.len());
            0xFF
        }
    }
}

// Struct to hold fetched tile data bytes and attributes for a single tile line
#[derive(Debug, Default, Clone, Copy)]
struct FetchedTileLineData {
    plane1_byte: u8,
    plane2_byte: u8,
    attributes: u8, // CGB attributes, 0 for DMG
}

impl Ppu {
    // ... (ensure all previous Ppu methods like new, read_vram, tick, etc. are preserved above this line)

    /// Fetches the pixel data for a single line of a given sprite.
    fn fetch_sprite_line_data(&self, oam_entry: &OamEntryData, current_ly: u8) -> Option<FetchedSpritePixelLine> {
        let sprite_height = if (self.lcdc & 0x04) != 0 { 16 } else { 8 }; // LCDC Bit 2: OBJ Size

        // Determine which line of the sprite we are rendering.
        // Sprite Y position (oam_entry.y_pos) is its top edge + 16.
        // current_ly is the current scanline (0-143).
        // y_on_screen_coord = current_ly
        // sprite_top_on_screen_coord = oam_entry.y_pos - 16
        // line_in_sprite_pixels = y_on_screen_coord - sprite_top_on_screen_coord
        // line_in_sprite_pixels = current_ly - (oam_entry.y_pos - 16)
        // line_in_sprite_pixels = current_ly + 16 - oam_entry.y_pos
        // This matches the OAM scan visibility check: LY + 16 >= SpriteY AND LY + 16 < SpriteY + SpriteHeight
        // So, line_in_sprite is effectively (LY + 16 - SpriteY)

        let y_on_screen_for_compare = current_ly.wrapping_add(16);
        // This check should be redundant if visible_oam_entries was populated correctly,
        // but it's good for safety / clarity.
        if !(y_on_screen_for_compare >= oam_entry.y_pos && y_on_screen_for_compare < oam_entry.y_pos.wrapping_add(sprite_height)) {
            return None; // Sprite not on this line (should have been filtered by OAM scan)
        }

        let mut line_in_sprite = y_on_screen_for_compare.wrapping_sub(oam_entry.y_pos);

        // Handle Y-flip (OAM attribute bit 6)
        if (oam_entry.attributes & (1 << 6)) != 0 { // Bit 6: Y flip
            line_in_sprite = (sprite_height - 1) - line_in_sprite;
        }

        let mut actual_tile_index = oam_entry.tile_index;
        if sprite_height == 16 {
            if line_in_sprite < 8 { // Top tile of 8x16 sprite
                actual_tile_index &= 0xFE; // Clear bit 0
            } else { // Bottom tile of 8x16 sprite
                actual_tile_index |= 0x01;  // Set bit 0
                line_in_sprite -= 8;      // Adjust line index for the 8x8 tile
            }
        }
        // For 8x8 sprites, actual_tile_index is just oam_entry.tile_index and line_in_sprite is already correct.

        let tile_vram_bank = if self.model.is_cgb_family() {
            (oam_entry.attributes >> 3) & 0x01 // Bit 3: Tile VRAM Bank (CGB only)
        } else {
            0 // DMG always uses VRAM bank 0 for sprites
        };

        // Sprites always use tile data from $8000-$8FFF.
        let tile_data_start_addr = 0x8000u16.wrapping_add(actual_tile_index as u16 * 16);
        let tile_line_offset_bytes = line_in_sprite as u16 * 2; // 2 bytes per line of pixels

        let plane1_byte_addr = tile_data_start_addr.wrapping_add(tile_line_offset_bytes);
        let plane2_byte_addr = plane1_byte_addr.wrapping_add(1);

        // Fetch tile data bytes
        // Note: PPU has direct VRAM access during Mode 3, bypassing normal read_vram CPU restrictions.
        // read_vram_bank_agnostic is suitable here.
        let plane1_byte = self.read_vram_bank_agnostic(plane1_byte_addr, tile_vram_bank);
        let plane2_byte = self.read_vram_bank_agnostic(plane2_byte_addr, tile_vram_bank);

        // Decode the 2 bytes into 8 pixel color indices
        let pixels = self.decode_sprite_tile_line(plane1_byte, plane2_byte);

        Some(FetchedSpritePixelLine {
            x_pos: oam_entry.x_pos, // This is OAM X (screen X + 8)
            attributes: oam_entry.attributes,
            pixels,
            oam_index: oam_entry.oam_index,
        })
    }

    // Fetches tile data bytes (plane1, plane2) and CGB attributes for a specific tile line.
    // map_x, map_y are coordinates in the 256x256 BG/Window map.
    fn fetch_tile_line_data(&self,
                            tile_map_addr_base: u16,      // e.g., 0x9800 or 0x9C00 for BG/Window
                            tile_data_addr_base_select: u8, // From LCDC bit 4 (0 or 1)
                            map_x: u8,
                            map_y: u8) -> FetchedTileLineData {
        let mut output = FetchedTileLineData::default();
        let is_cgb = self.model.is_cgb_family();

        let tile_row_in_map = (map_y / 8) as u16;
        let tile_col_in_map = (map_x / 8) as u16;

        let tile_number_map_addr = tile_map_addr_base + (tile_row_in_map * 32) + tile_col_in_map;

        // Tile number is always read from VRAM Bank 0 for BG/Window map
        let tile_number = self.read_vram_bank_agnostic(tile_number_map_addr, 0);

        let mut tile_data_vram_bank = 0;
        if is_cgb {
            // Attributes are in VRAM Bank 1, at the same offset as tile numbers in Bank 0.
            output.attributes = self.read_vram_bank_agnostic(tile_number_map_addr, 1);
            tile_data_vram_bank = (output.attributes >> 3) & 0x01; // Bit 3: Tile VRAM Bank (CGB)
        }

        let tile_data_start_addr: u16;
        if tile_data_addr_base_select == 1 { // Method 0x8000 (unsigned tile numbers)
            tile_data_start_addr = 0x8000 + (tile_number as u16 * 16);
        } else { // Method 0x8800 (signed tile numbers, base is 0x9000)
            tile_data_start_addr = 0x9000u16.wrapping_add(((tile_number as i8) as i16 * 16) as u16);
        }

        let mut line_offset_in_tile = (map_y % 8) as u16;
        if is_cgb && (output.attributes & (1 << 2)) != 0 { // Bit 2: Vertical Flip (CGB)
            line_offset_in_tile = 7 - line_offset_in_tile;
        }
        let tile_bytes_offset = line_offset_in_tile * 2; // 2 bytes per line

        let data_addr_plane1 = tile_data_start_addr + tile_bytes_offset;
        let data_addr_plane2 = data_addr_plane1 + 1;

        output.plane1_byte = self.read_vram_bank_agnostic(data_addr_plane1, tile_data_vram_bank);
        output.plane2_byte = self.read_vram_bank_agnostic(data_addr_plane2, tile_data_vram_bank);

        output
    }

    // Decodes a line of tile data (2 bytes) into 8 pixel color indices (0-3).
    // Handles CGB horizontal flip.
    fn decode_tile_line_to_pixels(&self, plane1_byte: u8, plane2_byte: u8, cgb_attributes: u8, is_cgb: bool) -> [u8; 8] {
        let mut pixels = [0u8; 8];
        let horizontal_flip = is_cgb && (cgb_attributes & (1 << 1)) != 0; // Bit 1: Horizontal Flip (CGB)

        for i in 0..8 {
            let bit_pos = if horizontal_flip { i } else { 7 - i };
            let lsb = (plane1_byte >> bit_pos) & 1;
            let msb = (plane2_byte >> bit_pos) & 1;
            pixels[if horizontal_flip { 7-i } else { i } as usize] = (msb << 1) | lsb;
        }
        pixels
    }

    /// Decodes a line of sprite tile data (2 bytes) into 8 pixel color indices (0-3).
    /// Sprite horizontal flip is handled during rendering/mixing, not here.
    fn decode_sprite_tile_line(&self, plane1_byte: u8, plane2_byte: u8) -> [u8; 8] {
        let mut pixels = [0u8; 8];
        for i in 0..8 {
            // Read bits from left to right (MSB to LSB for pixel order)
            let bit_pos = 7 - i;
            let lsb = (plane1_byte >> bit_pos) & 1;
            let msb = (plane2_byte >> bit_pos) & 1;
            pixels[i as usize] = (msb << 1) | lsb;
        }
        pixels
    }

    // Renders the background for the current scanline (self.ly) into self.current_scanline_color_indices.
    fn render_scanline_bg(&mut self) {
        // Reset pixel source to Background for the entire line before BG rendering
        for i in 0..SCREEN_WIDTH {
            self.current_scanline_pixel_source[i] = PixelSource::Background;
        }

        // LCDC Bit 3: BG Tile Map Display Select (0=9800-9BFF, 1=9C00-9FFF)
        let bg_tile_map_base_addr = if (self.lcdc >> 3) & 0x01 == 0 { 0x9800 } else { 0x9C00 };
        // LCDC Bit 4: BG & Window Tile Data Select (0=8800-97FF, 1=8000-8FFF)
        let bg_win_tile_data_select = (self.lcdc >> 4) & 0x01;

        let is_cgb = self.model.is_cgb_family();
        let mut current_tile_pixels = [0u8; 8];
        // Keep track of the map X coordinate for which the current_tile_pixels were fetched.
        // Initialize to a value that ensures the first tile is fetched.
        let mut fetched_tile_map_x_start = 255 - 7; // Ensures (map_x / 8) is different initially or forces fetch.

        for x_screen in 0..SCREEN_WIDTH {
            let map_x = self.scx.wrapping_add(x_screen as u8);
            let map_y = self.scy.wrapping_add(self.ly);

            // Check if we need to fetch a new tile's line data.
            // This is true if x_screen is 0 (start of line), or if the current map_x
            // has crossed into a new 8-pixel tile boundary compared to the last fetch.
            if x_screen == 0 || (map_x / 8) != (fetched_tile_map_x_start / 8) {
                 // If map_x has wrapped (e.g. from 255 to 0) and we are not at x_screen == 0,
                 // this also indicates a new tile segment from the beginning of the map.
                 // This condition (map_x < fetched_tile_map_x_start && x_screen > 0) is implicitly handled
                 // by (map_x / 8) != (fetched_tile_map_x_start / 8) due to integer division behavior with wrapping.

                let fetched_data = self.fetch_tile_line_data(
                    bg_tile_map_base_addr,
                    bg_win_tile_data_select,
                    map_x, map_y, // Use map_x for fetching, which corresponds to the start of the current/next tile
                );
                current_tile_pixels = self.decode_tile_line_to_pixels(
                    fetched_data.plane1_byte,
                    fetched_data.plane2_byte,
                    fetched_data.attributes,
                    is_cgb
                );
                fetched_tile_map_x_start = map_x; // Record the map_x for which these pixels were fetched.
                                                 // More accurately, this is the map_x of the first pixel in current_tile_pixels.
            }

            // Select the pixel from the 8 fetched pixels of the current tile.
            // (map_x % 8) gives the horizontal index (0-7) within the current tile.
            let pixel_in_tile_index = (map_x % 8) as usize;
            self.current_scanline_color_indices[x_screen] = current_tile_pixels[pixel_in_tile_index];
        }
    }

    // Applies DMG palettes (BG, OBP0, OBP1) to the mixed scanline data and writes to framebuffer.
    // For CGB, this function currently uses a temporary grayscale mapping.
    fn apply_dmg_palette_to_framebuffer(&mut self) {
        // Ensure self.ly is within the valid screen height to prevent panic
        if self.ly >= SCREEN_HEIGHT as u8 {
            return; // Should not happen during normal drawing modes
        }

        // CGB path (temporary grayscale, bypasses new PixelSource logic)
        if self.model.is_cgb_family() {
            // TODO: Implement proper CGB palette rendering.
            // This current CGB path is a placeholder and will render CGB BG/Sprite indices as grayscale.
            // It does not use PixelSource and assumes current_scanline_color_indices holds direct CGB color indices for BG.
            // Sprites are not yet rendered in this CGB path.
            for x in 0..SCREEN_WIDTH {
                let color_index = self.current_scanline_color_indices[x]; // Assumes this is BG index for CGB
                let shade_val = match color_index {
                    0 => DMG_WHITE, // Placeholder, should use CGB palettes
                    1 => DMG_LIGHT_GRAY,
                    2 => DMG_DARK_GRAY,
                    _ => DMG_BLACK,
                };
                let fb_idx = (self.ly as usize * SCREEN_WIDTH + x) * 3;
                if fb_idx + 2 < self.framebuffer.len() {
                    self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&shade_val);
                }
            }
            return; // End CGB path for now
        }

        // DMG Path (uses PixelSource)
        for x in 0..SCREEN_WIDTH {
            let color_index = self.current_scanline_color_indices[x]; // Value from 0-3
            let pixel_source = self.current_scanline_pixel_source[x];
            let mut shade = 0; // Default to white

            match pixel_source {
                PixelSource::Background => {
                    shade = (self.bgp >> (color_index * 2)) & 0x03;
                }
                PixelSource::Object { palette_register } => {
                    if palette_register == 0 { // OBP0
                        shade = (self.obp0 >> (color_index * 2)) & 0x03;
                    } else { // OBP1
                        shade = (self.obp1 >> (color_index * 2)) & 0x03;
                    }
                }
            }

            let rgb_color = match shade {
                0 => DMG_WHITE,
                1 => DMG_LIGHT_GRAY,
                2 => DMG_DARK_GRAY,
                3 => DMG_BLACK,
                _ => unreachable!(), // shade is always 0-3
            };

            let fb_idx = (self.ly as usize * SCREEN_WIDTH + x) * 3;
            if fb_idx + 2 < self.framebuffer.len() {
                self.framebuffer[fb_idx..fb_idx + 3].copy_from_slice(&rgb_color);
            } else {
                // This should ideally not happen if framebuffer is correctly sized
                // and ly/x are within bounds.
                // eprintln!("Framebuffer write out of bounds: LY={}, X={}, fb_idx={}", self.ly, x, fb_idx);
            }
        }
    }
}

// Helper to determine if PPU is accessing VRAM based on current mode
