# MBC Implementation TODO List

This list outlines potential enhancements and missing features in our MBC implementation, based on a comparison with SameBoy's MBC handling.

## High Priority:

*   **[ ] Implement Full MBC7 Functionality:**
    *   [ ] Add Accelerometer support (sensor input, latching).
    *   [ ] Add EEPROM support (serial communication logic).
    *   *Reason: Essential for playability of key titles like Kirby Tilt 'n' Tumble.*

*   **[ ] Enhance MBC1 for Wiring Variants (especially MBC1M):**
    *   [ ] Investigate and implement logic to differentiate standard MBC1 wiring from MBC1M (e.g., via ROM heuristics or explicit configuration).
    *   [ ] Adjust ROM/RAM bank calculations based on the detected wiring scheme, particularly for how upper bank select bits interact with lower bits and mode settings.
    *   *Reason: Important for compatibility with a subset of MBC1 games that rely on specific wiring behaviors.*

*   **[ ] Implement Accurate MBC30 ROM Banking (>2MB):**
    *   [ ] Modify MBC3/MBC30 ROM bank selection to allow more than 7 bits (i.e., support banks > 127).
    *   [ ] Ensure correct handling for ROMs larger than 2MB.
    *   *Reason: Crucial for compatibility with larger MBC3 games, ROM hacks, and translations using MBC30.*

## Medium Priority:

*   **[ ] Add MBC5 Rumble Support:**
    *   [ ] Re-integrate or add new logic to handle rumble motor control for MBC5 cartridges.
    *   [ ] This would likely involve checking a cartridge flag (similar to SameBoy's `has_rumble`) and reacting to specific register writes if applicable.
    *   *Reason: Enhances experience for supported games like Pokemon Pinball.*

*   **[ ] Add Support for HuC1 / HuC3 Mappers:**
    *   [ ] Implement `HuC1` (Hudson HuC-1) banking logic.
    *   [ ] Implement `HuC3` (Hudson HuC-3) banking logic, including its RTC features.
    *   *Reason: Increases compatibility with a notable set of (mostly Japanese) games.*

*   **[ ] Add Dedicated Game Boy Camera Support:**
    *   [ ] Create a specific MBC type or enhance existing ones to handle Game Boy Camera's I/O registers (0xA000-0xAFFF) and other specific behaviors.
    *   *Reason: For proper functionality of this popular peripheral.*

*   **[ ] Improve Cartridge Quirk Handling & Heuristics:**
    *   [ ] Implement logic to detect and correct common ROM header issues (e.g., NoMBC type with large ROM size should potentially be MBC3).
    *   [ ] Add heuristics to infer RAM presence if specified in header but not by type byte.
    *   *Reason: To better support problematic, mislabeled, or homebrew ROM files.*

## Lower Priority:

*   **[ ] Add Support for `GB_MMM01` (Multi-Game Cartridge):**
    *   [ ] Implement the complex banking and ROM layout logic for MMM01.
    *   *Reason: Niche, for specific multi-carts.*

*   **[ ] Add Support for `GB_TPP1` (Taito Pokemon Party Mini):**
    *   [ ] Implement banking and RAM enable logic specific to TPP1 cartridges.
    *   *Reason: Very game-specific.*

*   **[ ] Investigate MBC6:**
    *   [ ] Determine if a more accurate MBC6 implementation (beyond an MBC1 stub) is needed for any specific games.
    *   *Reason: Low occurrence, behavior not as clearly distinct as other MBCs.*

## General Considerations:

*   **[ ] Review RAM Initialization:** SameBoy initializes RAM to `0xFF`. Confirm if our `vec![0; ram_size]` default is universally acceptable or if `0xFF` is a better default for uninitialized RAM.
*   **[ ] Logging for Heuristics:** Consider adding more logging when cartridge quirks are detected and automatically handled.
```
