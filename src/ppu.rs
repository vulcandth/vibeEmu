use crate::models::GameBoyModel;

// Constants for PPU
pub const VRAM_SIZE_CGB: usize = 16 * 1024; // 2 banks of 8KB
pub const VRAM_SIZE_DMG: usize = 8 * 1024;  // 1 bank of 8KB
pub const OAM_SIZE: usize = 160;           // Object Attribute Memory
pub const CGB_PALETTE_RAM_SIZE: usize = 64; // Background and Sprite palettes (32 palettes * 2 bytes/color * 4 colors/palette is not right, it's 64 bytes total for CRAM)

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

// PPU Modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpuMode {
    HBlank = 0, // Mode 0
    VBlank = 1, // Mode 1
    OamScan = 2, // Mode 2
    Drawing = 3, // Mode 3
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
    current_mode: PpuMode,
    cycles_in_mode: u32, // Cycles accumulated in the current mode for the current line

    // Framebuffer to store rendered pixels
    pub framebuffer: Vec<u8>, // Stores RGB888, 160*144*3 bytes

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

    model: GameBoyModel,
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
            lcdc: 0x91, // Common initial value (Display ON, BG ON, OBJ ON, etc.)
            stat: 0x85, // Mode 1 (VBlank) + LYC=LY coincidence flag often set initially
            scy: 0x00,
            scx: 0x00,
            ly: 0x00,   // Typically starts at 0 or 144 (if VBlank first)
            lyc: 0x00,
            wy: 0x00,
            wx: 0x00,
            bgp: 0xFC,  // Default grayscale palette
            obp0: 0xFF, // Or 0x00, depends on interpretation. SameBoy uses FF.
            obp1: 0xFF,

            vbk: 0xFE, // Bit 0 is bank, other bits readable as 1. So 0xFE if bank 0, 0xFF if bank 1. Default bank 0.
            bcps_bcpi: 0x00, // TODO: Verify initial CGB palette register states
            bcpd_bgpd: 0x00,
            ocps_ocpi: 0x00,
            ocpd_obpd: 0x00,

            current_mode: PpuMode::OamScan, // Or VBlank, depending on initial state (often starts in Mode 2 or Mode 1)
            cycles_in_mode: 0,

            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT * 3], // RGB888

            model,
        }
    }

    // Basic read/write methods for VRAM and OAM (will be called by Bus)
    // These will need to respect PPU modes and CGB banking later.

    pub fn read_vram(&self, addr: u16) -> u8 {
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
        // Address is 0xFE00-0xFE9F, so subtract 0xFE00 for index
        self.oam[addr as usize - 0xFE00]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        self.oam[addr as usize - 0xFE00] = value;
    }

    // TODO: Implement CGB palette RAM read/write methods
    // read_cgb_palette_ram(is_sprite_palette: bool, index: u8, auto_increment: bool) -> u8
    // write_cgb_palette_ram(is_sprite_palette: bool, index: u8, value: u8, auto_increment: bool)

    // TODO: Implement PPU tick logic
    // pub fn tick(&mut self, cycles: u32) -> Option<InterruptType>
}

// Helper to determine if PPU is accessing VRAM based on current mode
// This is a simplified placeholder. Real access depends on more factors.
// fn is_vram_accessible(mode: PpuMode) -> bool {
//     match mode {
//         PpuMode::Drawing => false, // VRAM generally inaccessible by CPU
//         _ => true,
//     }
// }

// Helper to determine if PPU is accessing OAM based on current mode
// fn is_oam_accessible(mode: PpuMode) -> bool {
//     match mode {
//         PpuMode::OamScan | PpuMode::Drawing => false, // OAM generally inaccessible by CPU
//         _ => true,
//     }
// }
