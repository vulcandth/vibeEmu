# PPU Implementation TODO List

This document outlines the major components, functions, and logic sections that need to be implemented for the new PPU (Pixel Processing Unit) in `src/ppu.rs`, drawing inspiration from sources like SameBoy's PPU (`Core/display.c`) and general Game Boy PPU documentation.

## Core PPU Structure & State
- [x] Define main `Ppu` struct (`src/ppu.rs`).
- [x] Implement basic constructor (`new()`) initializing fields to power-up states.
- [x] VRAM (Video RAM): Implement banking for CGB.
- [x] OAM (Object Attribute Memory): Basic storage is present. Implement access restrictions based on PPU mode.
- [ ] Framebuffer: Currently `Vec<u8>` for RGB888. Ensure correct dimensions and pixel format.

## PPU Modes & Timing
- [ ] **Mode Switching Logic:**
    - Implement accurate timing for Mode 0 (HBlank), Mode 1 (VBlank), Mode 2 (OAM Scan), Mode 3 (Drawing).
    - Manage `cycles_in_mode` and transition between modes correctly.
    - Update `STAT` register (Mode bits) upon mode changes.
- [ ] **STAT Register (0xFF41):**
    - Implement full read/write logic, respecting read-only bits.
    - Implement STAT interrupt generation based on enabled conditions (LYC=LY coincidence, Mode 0/1/2 IRQ).
    - Handle LYC=LY flag updates.
- [ ] **LY Register (0xFF44):**
    - Increment LY for each scanline.
    - Reset LY during VBlank (or at the start of line 0 after VBlank).
    - Handle LYC comparison and update STAT register.
- [ ] **LCDC Register (0xFF40):**
    - Implement full read/write logic.
    - Use LCDC bits to control PPU operations (Display Enable, Window Tile Map, Window Enable, BG/OBJ Tile Data Select, OBJ Size, OBJ Enable, BG Enable).
    - Handle LCD enable/disable logic (e.g., PPU stops, screen clears, LY resets).

## Scanline Rendering Pipeline (Conceptual)
*(This section assumes a pipeline similar to hardware or SameBoy's FIFO-based approach. Adapt as necessary.)*

- [ ] **Pixel Fetcher:**
    - Fetch tile data for Background, Window.
    - Handle SCX/SCY scrolling for BG.
    - Handle WX/WY for Window positioning.
- [ ] **Pixel FIFO:**
    - Manage a First-In-First-Out queue for background/window pixels.
    - Manage a separate FIFO or merging logic for sprite pixels.
- [ ] **Sprite (OBJ) Processing:**
    - **OAM Scan (Mode 2):**
        - Scan OAM for sprites visible on the current scanline (up to 10 per line).
        - Consider X-priority for CGB when multiple sprites share the same X coordinate.
        - Store processed sprite data for Mode 3.
    - **Sprite Fetching:** During Mode 3, fetch tile data for visible sprites.
    - **Sprite Rendering:**
        - Handle sprite attributes: X/Y position, tile index, palette, priority, X/Y flip.
        - Implement 8x8 and 8x16 sprite sizes (controlled by LCDC.2).
        - Correctly mix sprite pixels with BG/Window pixels based on priority (OBJ-BG priority bit in OAM flags, CGB BG-to-OAM priority).
- [ ] **Background (BG) Rendering:**
    - Select correct tile map (LCDC.3).
    - Select correct tile data (LCDC.4).
    - Render BG pixels considering SCX/SCY scroll.
- [ ] **Window Rendering:**
    - Enable/disable based on LCDC.5.
    - Select correct tile map (LCDC.6).
    - Use WY/WX for window positioning.
    - Implement window internal line counter.

## Palettes
- [ ] **DMG Palette Management:**
    - Implement reads/writes to BGP (0xFF47), OBP0 (0xFF48), OBP1 (0xFF49).
    - Apply these palettes to BG/Window and Sprite pixels respectively.
- [ ] **CGB Palette Management:**
    - Implement CGB palette RAM (Background and Sprite).
    - Handle BCPS/BCPI (0xFF68) for BG palette index and auto-increment.
    - Handle BCPD/BGPD (0xFF69) for BG palette data writes.
    - Handle OCPS/OCPI (0xFF6A) for Sprite palette index and auto-increment.
    - Handle OCPD/OBPD (0xFF6B) for Sprite palette data writes.
    - Apply CGB palettes to pixels.
    - Implement CGB BG-to-OAM priority attribute from tile map attributes.

## Interrupts
- [ ] **VBlank Interrupt (Mode 1):**
    - Trigger VBlank interrupt when PPU enters Mode 1 (LY = 144).
    - Set VBlank flag in IF register (0xFF0F).
- [ ] **LCD STAT Interrupt:**
    - Trigger based on conditions enabled in STAT register (LYC=LY, Mode 0/1/2 HBlank/VBlank/OAM period).
    - Set LCD STAT flag in IF register (0xFF0F).

## CGB Specific Features
- [ ] **VRAM Banking (VBK Register - 0xFF4F):**
    - Implement VRAM bank switching for CGB mode. Reads/writes to 0x8000-0x9FFF should respect the selected bank.
- [ ] **HDMA/GDMA Interaction Points:**
    - While DMA logic is in `Bus`, the PPU needs to allow VRAM access during HDMA/GDMA.
    - Ensure VRAM access methods (`read_vram`, `write_vram`) are usable by DMA logic.
    - Consider any PPU state that might affect or be affected by HDMA (e.g., HBlank state for HDMA triggers).
- [ ] **CGB Tile Attributes:**
    - When fetching BG/Window tiles in CGB mode, read attributes from VRAM bank 1.
    - Apply attributes: BG-to-OAM priority, vertical/horizontal flip, VRAM bank selection for tile data, palette selection.

## Bus Integration
- [ ] **Register Access:**
    - Ensure all PPU-related I/O registers (0xFF40-0xFF4F, 0xFF68-0xFF6B) are correctly routed from `Bus` to `Ppu` methods or fields.
    - Implement side effects of register writes (e.g., writing to LCDC can change PPU state, STAT can trigger interrupts).
- [ ] **Memory Access:**
    - `Bus` should call `ppu.read_vram()`, `ppu.write_vram()`, `ppu.read_oam()`, `ppu.write_oam()`.
    - Implement PPU mode-based access restrictions for VRAM and OAM within PPU methods (e.g., CPU cannot access VRAM/OAM during certain PPU modes).
- [ ] **PPU Tick Function:**
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
