# PPU Implementation TODO List

This document outlines the major components, functions, and logic sections that need to be implemented for the new PPU (Pixel Processing Unit) in `src/ppu.rs`, drawing inspiration from sources like SameBoy's PPU (`Core/display.c`) and general Game Boy PPU documentation.

## Core PPU Structure & State
- [x] Define main `Ppu` struct (`src/ppu.rs`).
- [x] Implement basic constructor (`new()`) initializing fields to power-up states.
- [x] VRAM (Video RAM): Implement banking for CGB.
- [x] OAM (Object Attribute Memory): Basic storage is present. Implement access restrictions based on PPU mode.
- [x] Framebuffer: Currently `Vec<u8>` for RGB888. Ensure correct dimensions and pixel format.
- [x] `current_scanline_color_indices`: Buffer for 2-bit color indices for the current line.
- [x] `current_scanline_pixel_source`: Buffer to track if pixel is BG or Sprite (and which sprite palette for DMG).

## PPU Modes & Timing
- [ ] **Mode Switching Logic:**
    - [-] **Implement accurate timing for Mode 0 (HBlank), Mode 1 (VBlank), Mode 2 (OAM Scan), Mode 3 (Drawing)** (Mode 3/HBlank timing is now semi-dynamic based on sprite count. Eventual goal: FIFO-driven cycle accuracy).
    - [x] Manage `cycles_in_mode` and transition between modes correctly.
    - [x] Update `STAT` register (Mode bits) upon mode changes.
- [ ] **STAT Register (0xFF41):**
    - [x] **Implement full read/write logic, respecting read-only bits** (Substantially complete: CPU write to enable bits 3-6, PPU controls mode bits 0-1 and LYC flag bit 2. Bit 7 is always 1).
    - [-] **Implement STAT interrupt generation based on enabled conditions (LYC=LY coincidence, Mode 0/1/2 IRQ)** (Partially complete: PPU sets internal `stat_interrupt_requested` flag, Bus integration for requesting CPU interrupt is done. Actual IF flag setting by CPU is separate).
    - [x] Handle LYC=LY flag updates.
- [ ] **LY Register (0xFF44):**
    - [x] Increment LY for each scanline.
    - [x] Reset LY during VBlank (or at the start of line 0 after VBlank).
    - [x] Handle LYC comparison and update STAT register.
- [ ] **LCDC Register (0xFF40):**
    - [x] Implement full read/write logic (via Bus to PPU fields).
    - [-] Use LCDC bits to control PPU operations:
        - [-] Bit 0 (BG Display Enable): Logic in place (rendering conditional, actual white screen if disabled is part of palette mapping).
        - [x] Bit 1 (OBJ Display Enable): Used for enabling/disabling sprite rendering pipeline.
        - [x] Bit 2 (OBJ Size): Used in OAM scan and sprite fetching for 8x8 vs 8x16 sprites.
        - [x] Bit 3 (BG Tile Map Display Select): Used by BG tile fetching.
        - [x] Bit 4 (BG & Window Tile Data Select): Used by BG tile fetching.
        - [-] Bit 5 (Window Display Enable): Check placeholder added.
        - [-] Bit 6 (Window Tile Map Display Select): Placeholder added.
        - [x] Bit 7 (LCD Display Enable): PPU tick sets LY=0, mode=HBlank, clears its interrupt flags, and STAT reflects this state when LCDC.7 is off.

## Scanline Rendering Pipeline (Conceptual)
*(This section assumes a pipeline similar to hardware or SameBoy's FIFO-based approach. Adapt as necessary.)*

- [ ] **Pixel Fetcher:**
    - [x] **Conceptual BG Tile Info Fetching:** Implemented concrete BG tile data fetching (tile number, attributes, pixel data bytes via `fetch_tile_line_data`).
    - [-] Fetch tile data for Background (done), Window (pending).
    - [x] Handle SCX/SCY scrolling for BG (as part of render_scanline_bg - Fine X scroll now handled by discarding initial pixels from fetcher/FIFO).
    - [ ] Handle WX/WY for Window positioning.
    - [-] Implement dedicated BG/Window pixel fetcher logic (state machine for tile#, data, attributes - BG fetcher improved with fine SCX handling).
    - [-] Implement dedicated Sprite pixel fetcher logic (Current sprite pipeline loads pre-fetched line data into sprite FIFO; a more traditional cycle-accurate fetcher is future work).
- [ ] **Pixel FIFO:**
    - [x] Implement BG Pixel FIFO.
    - [x] Implement Sprite Pixel FIFO.
    - [-] Implement FIFO mixing/merging logic for BG and Sprite pixels (Basic DMG mixing implemented; CGB and more complex scenarios pending).
    - [ ] Drive PPU Mode 3 timing based on Pixel Fetcher and FIFO states.
- [ ] **Sprite (OBJ) Processing:**
    - **OAM Scan (Mode 2):**
        - [x] Scan OAM for sprites visible on the current scanline (up to 10 per line, Y-check, X>0, sorted by X then OAM index).
        - [ ] Consider X-priority for CGB when multiple sprites share the same X coordinate. (Currently sorts by X for DMG, CGB needs OAM X field check)
        - [x] Store OAM data (index, X, Y, tile, attributes) of visible sprites for Mode 3 processing.
    - **Sprite Fetching:**
        - [x] During Mode 3, fetch tile data (pixel color indices 0-3) for visible sprites (handles 8x8/8x16, Y-flip, CGB VRAM bank).
    - **Sprite Rendering:**
        - [x] Handle sprite attributes: X-position, tile index, DMG palettes (OBP0/1 via OAM attr), DMG priority (OAM attr), X/Y flip.
        - [x] Implemented 8x8 and 8x16 sprite sizes (controlled by LCDC.2) during OAM scan and fetching.
        - [x] Correctly mix DMG sprite pixels with BG/Window pixels based on OAM priority attribute and transparency (DMG mixing now actively handled in Mode 3; CGB BG-to-OAM priority pending full CGB palette/attribute integration in scanline buffer).
- [ ] **Background (BG) Rendering:**
    - [x] Select correct tile map (LCDC.3). (Used in `render_scanline_bg`)
    - [x] Select correct tile data (LCDC.4). (Used in `render_scanline_bg`)
    - [x] Render BG pixels considering SCX/SCY scroll (writes 2-bit color indices to scanline buffer).
- [ ] **Window Rendering:**
    - [-] Enable/disable based on LCDC.5 (placeholder check added).
    - [-] Select correct tile map (LCDC.6) (placeholder added).
    - [ ] Use WY/WX for window positioning.
    - [ ] Implement window internal line counter.

## Palettes
- [ ] **DMG Palette Management:**
    - [x] BGP (0xFF47) is read for BG palette mapping. OBP0 (0xFF48), OBP1 (0xFF49) are read for sprite palette mapping. Writes handled by Bus.
    - [x] Apply DMG BGP, OBP0, OBP1 palettes to BG & Sprite pixels respectively (writes to framebuffer using combined pixel source buffer). Window palettes pending.
- [ ] **CGB Palette Management:**
    - [ ] Implement CGB palette RAM (Background and Sprite).
    - [ ] Handle BCPS/BCPI (0xFF68) for BG palette index and auto-increment.
    - [ ] Handle BCPD/BGPD (0xFF69) for BG palette data writes.
    - [ ] Handle OCPS/OCPI (0xFF6A) for Sprite palette index and auto-increment.
    - [ ] Handle OCPD/OBPD (0xFF6B) for Sprite palette data writes.
    - [ ] Apply CGB palettes to pixels.
    - [ ] Implement CGB BG-to-OAM priority attribute from tile map attributes.

## Interrupts
- [ ] **VBlank Interrupt (Mode 1):**
    - [x] Trigger VBlank interrupt when PPU enters Mode 1 (LY = 144). (PPU sets internal flag, Bus requests interrupt)
    - [x] Set VBlank flag in IF register (0xFF0F). (Done by Bus)
- [ ] **LCD STAT Interrupt:**
    - [x] Trigger based on conditions enabled in STAT register (LYC=LY, Mode 0/1/2 HBlank/VBlank/OAM period). (PPU sets internal flag, Bus requests interrupt)
    - [x] Set LCD STAT flag in IF register (0xFF0F). (Done by Bus)

## CGB Specific Features
- [x] **VRAM Banking (VBK Register - 0xFF4F):** (Basic read/write via Bus to PPU field. PPU's `read_vram` uses it. `fetch_tile_line_data` uses `read_vram_bank_agnostic` for specific bank access.)
    - Implement VRAM bank switching for CGB mode. Reads/writes to 0x8000-0x9FFF should respect the selected bank.
- [ ] **HDMA/GDMA Interaction Points:**
    - [x] While DMA logic is in `Bus`, the PPU needs to allow VRAM access during HDMA/GDMA. (PPU VRAM access methods are public)
    - [x] Ensure VRAM access methods (`read_vram`, `write_vram`) are usable by DMA logic.
    - [x] Consider any PPU state that might affect or be affected by HDMA (e.g., HBlank state for HDMA triggers). (PPU sets `just_entered_hblank`, Bus uses it)
- [x] **CGB Tile Attributes:** (Fetching implemented in `fetch_tile_line_data`)
    - When fetching BG/Window tiles in CGB mode, read attributes from VRAM bank 1.
    - Apply attributes: BG-to-OAM priority, vertical/horizontal flip, VRAM bank selection for tile data, palette selection. (Vertical flip and bank selection for tile data used in `fetch_tile_line_data`; H-flip used in `decode_tile_line_to_pixels`)

## Bus Integration
- [x] **Register Access:**
    - Ensure all PPU-related I/O registers (0xFF40-0xFF4F, 0xFF68-0xFF6B) are correctly routed from `Bus` to `Ppu` methods or fields.
    - Implement side effects of register writes (e.g., writing to LCDC can change PPU state, STAT can trigger interrupts). (STAT write effects partially handled via `ppu.write_stat()`)
- [x] **Memory Access:**
    - `Bus` should call `ppu.read_vram()`, `ppu.write_vram()`, `ppu.read_oam()`, `ppu.write_oam()`.
    - [x] Implement PPU mode-based access restrictions for VRAM and OAM within PPU methods (for CPU access; DMA bypasses implemented).
- [x] **PPU Tick Function:**
    - Implement `ppu.tick(cycles)` method to advance PPU state based on passed CPU M-cycles (or T-cycles).
    - This function will manage internal cycle counters, mode transitions, scanline rendering, and interrupt flags.
    - `Bus::tick_components` will call `ppu.tick()`.

## Miscellaneous & Glitches (Advanced)
- [ ] **OAM Bug:** Emulate OAM memory corruption if accessed incorrectly during specific PPU states (DMG).
- [ ] **STAT Register Glitches:** Research and implement various STAT register behavior quirks (e.g., STAT blocking, LYC=LY coincidence timing).
- [ ] **WX=0 / WX=166 Glitches:** Handle specific window behavior at these WX values.
- [ ] **SCX Fractional Scrolling Glitch:** Some specific SCX behaviors.
- [ ] **LCD Disable/Enable Timing:** Precise timing and state changes when LCDC.7 is toggled.

This list is comprehensive and items can be implemented iteratively.
Core functionality (modes, basic rendering, DMG palettes, VBlank IRQ) should be prioritized.
CGB features and more obscure glitches can be implemented later.
