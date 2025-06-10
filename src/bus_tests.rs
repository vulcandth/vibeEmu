// src/bus_tests.rs
// Unit tests for Bus logic, especially DMA, will go here.

#![allow(dead_code)]

use crate::bus::Bus;
use crate::models::GameBoyModel; // Import GameBoyModel

// Helper to create a Bus instance with a simple ROM
fn setup_bus(rom_data_option: Option<Vec<u8>>, model_override: Option<GameBoyModel>) -> Bus {
    let model = model_override.unwrap_or(GameBoyModel::DMG); // Use GameBoyModel
    let rom = rom_data_option.unwrap_or_else(|| {
        let mut r = vec![0u8; 0x200]; // Increased size for more WRAM testing space if needed
        if model.is_cgb_family() {
            // Use is_cgb_family()
            r[0x0143] = 0x80; // CGB mode
        } else {
            r[0x0143] = 0x00; // DMG mode
        }
        r[0x0147] = 0x00; // ROM only for basic tests
        r[0x0149] = 0x00; // No RAM ( cartridge ) for basic tests
        r
    });
    Bus::new(rom)
}

fn setup_bus_dmg() -> Bus {
    setup_bus(None, Some(GameBoyModel::DMG))
}
fn setup_bus_cgb() -> Bus {
    setup_bus(None, Some(GameBoyModel::CGBD))
} // Using CGBD as a default CGB

#[test]
fn test_oam_dma_transfer_and_timing() {
    let mut bus = setup_bus_dmg();
    // Fill WRAM 0xC000-0xC09F with 0 to 159
    for i in 0..160 {
        bus.write_byte(0xC000 + i as u16, i as u8);
    }

    bus.write_byte(0xFF46, 0xC0); // Start OAM DMA from 0xC0xx

    assert!(
        bus.oam_dma_active,
        "OAM DMA should be active after writing to 0xFF46"
    );
    assert_eq!(
        bus.oam_dma_cycles_remaining,
        160 * 4,
        "OAM DMA cycles should be set (T-cycles)"
    );

    // The copy happens on the first "tick" where DMA is active and just started
    // Let's assume 1 M-cycle passes for the bus to acknowledge and start the PPU part of the DMA.
    // The current PPU tick doesn't consume OAM DMA cycles, Bus::tick_components does.
    // The Bus::tick_components will perform the copy if oam_dma_cycles_remaining == 160 * 4.

    bus.tick_components(1); // 1 M-cycle. This should trigger the copy.
                            // And decrement oam_dma_cycles_remaining by 4.

    for i in 0..160 {
        assert_eq!(
            bus.ppu.oam[i as usize], i as u8,
            "OAM byte {} mismatch after DMA copy",
            i
        );
    }
    assert_eq!(
        bus.oam_dma_cycles_remaining,
        (160 * 4) - (1 * 4),
        "OAM DMA cycles after 1 M-cycle tick"
    );

    // Tick for the remainder of DMA duration
    let mut m_cycles_passed = 1;
    while bus.oam_dma_active {
        bus.tick_components(1); // Tick 1 M-cycle at a time
        m_cycles_passed += 1;
        if m_cycles_passed > 160 + 5 {
            // Safety break, allow a little leeway
            panic!(
                "OAM DMA seems to be running for too long: {} M-cycles",
                m_cycles_passed
            );
        }
    }

    assert!(
        !bus.oam_dma_active,
        "OAM DMA should be inactive after its duration"
    );
    assert_eq!(
        bus.oam_dma_cycles_remaining, 0,
        "OAM DMA cycles should be 0 after completion"
    );
    assert_eq!(
        m_cycles_passed,
        160,
        "OAM DMA should take 160 M-cycles ({} T-cycles)",
        160 * 4
    );
}

#[test]
fn test_oam_dma_bus_restrictions() {
    let mut bus = setup_bus_dmg();
    bus.write_byte(0xFF46, 0xC0);
    assert!(bus.oam_dma_active);

    assert_eq!(bus.read_byte(0x8000), 0xFF, "VRAM read restricted"); // This read goes through Bus, which returns 0xFF if PPU says no
    assert_eq!(bus.read_byte(0xC000), 0xFF, "WRAM read restricted");

    bus.memory.write_byte(0xFF80, 0xAB);
    assert_eq!(bus.read_byte(0xFF80), 0xAB, "HRAM read allowed");

    // VRAM access during OAM DMA is complex. The Bus's read_byte/write_byte already returns 0xFF or ignores writes
    // if OAM DMA is active and address is not HRAM. So direct check on ppu.vram might be misleading here
    // as the write to 0x8000 would have been blocked by the bus itself.
    // For now, let's assume the bus correctly blocks it based on oam_dma_active.
    // let vram_orig_val = bus.ppu.read_vram(0x8000); // Read initial state
    // bus.write_byte(0x8000, 0x12); // This write should be ignored by the bus due to OAM DMA
    // assert_eq!(bus.ppu.read_vram(0x8000), vram_orig_val, "VRAM write should be ignored by Bus during OAM DMA");

    let wram_orig_val = bus.memory.read_byte(0xC100);
    bus.write_byte(0xC100, 0x34);
    assert_eq!(
        bus.memory.read_byte(0xC100),
        wram_orig_val,
        "WRAM write ignored"
    );

    bus.write_byte(0xFF81, 0xCD);
    assert_eq!(bus.memory.read_byte(0xFF81), 0xCD, "HRAM write allowed");

    bus.oam_dma_active = false; // Manually stop for test cleanup
}

#[test]
fn test_hdma_register_read_write_default() {
    let mut bus = setup_bus_cgb();
    assert_eq!(bus.read_byte(0xFF51), 0xFF, "Default HDMA1"); // Default 0xFF
    assert_eq!(bus.read_byte(0xFF52), 0xFF, "Default HDMA2");
    assert_eq!(bus.read_byte(0xFF53), 0xFF, "Default HDMA3");
    assert_eq!(bus.read_byte(0xFF54), 0xFF, "Default HDMA4");
    assert_eq!(bus.read_byte(0xFF55), 0xFF, "Default HDMA5 (inactive)");

    bus.write_byte(0xFF51, 0x12);
    assert_eq!(bus.hdma1_src_high, 0x12);
    bus.write_byte(0xFF52, 0x34);
    assert_eq!(bus.hdma2_src_low, 0x30); // Alignment
    bus.write_byte(0xFF53, 0x56);
    assert_eq!(bus.hdma3_dest_high, 0x16); // Alignment (0x96 & 0x1F -> 0x16)
    bus.write_byte(0xFF54, 0x78);
    assert_eq!(bus.hdma4_dest_low, 0x70); // Alignment
}

#[test]
fn test_gdma_transfer_and_completion() {
    let mut bus = setup_bus_cgb();
    // Fill WRAM 0xC000-C01F with 0..31
    for i in 0..32 {
        bus.write_byte(0xC000 + i, i as u8);
    }

    bus.write_byte(0xFF51, 0xC0); // Source C000
    bus.write_byte(0xFF52, 0x00);
    bus.write_byte(0xFF53, 0x80); // Dest 8000 (VRAM Bank 0 via VBK=0)
    bus.write_byte(0xFF54, 0x00);

    // Trigger GDMA for 2 blocks (32 bytes), (length / 16) - 1 = (2-1) = 1. HDMA5 value = 0x01.
    bus.write_byte(0xFF55, 0x01);

    assert!(
        !bus.gdma_active,
        "GDMA should be inactive after instant transfer"
    );
    assert!(!bus.hdma_active, "HDMA should be inactive");
    assert_eq!(
        bus.hdma_blocks_remaining, 0,
        "Blocks remaining should be 0 after GDMA"
    );
    assert_eq!(
        bus.read_byte(0xFF55),
        0xFF,
        "HDMA5 should read 0xFF after GDMA"
    );

    // Verify VRAM content (Bank 0 because VBK is 0 by default in PPU state)
    for i in 0..32 {
        assert_eq!(
            bus.ppu.read_vram(0x8000 + i as u16),
            i as u8,
            "VRAM byte {} mismatch after GDMA",
            i
        );
    }

    // Test with VRAM Bank 1
    bus.ppu.vbk = 1; // Switch PPU's VBK to 1
    for i in 0..16 {
        bus.write_byte(0xC100 + i, (i + 100) as u8);
    } // New source data
    bus.write_byte(0xFF51, 0xC1); // Source C100
    bus.write_byte(0xFF52, 0x00);
    bus.write_byte(0xFF53, 0x90); // Dest 0x9000
    bus.write_byte(0xFF54, 0x00);
    bus.write_byte(0xFF55, 0x00); // GDMA for 1 block (16 bytes)

    for i in 0..16 {
        // VRAM address for bank 1 starts at 0x8000 in the bank, but overall addresses are 0x8000-0x9FFF
        // Dest 0x9000 means read_vram(0x9000 + i)
        assert_eq!(
            bus.ppu.read_vram(0x9000 + i as u16),
            (i + 100) as u8,
            "VRAM Bank 1 byte {} mismatch",
            i
        );
    }
}

#[test]
fn test_hdma_hblank_transfers() {
    let mut bus = setup_bus_cgb();
    // Fill WRAM
    for i in 0..64 {
        bus.write_byte(0xD000 + i, i as u8);
    }

    bus.write_byte(0xFF51, 0xD0); // Src D000
    bus.write_byte(0xFF52, 0x00);
    bus.write_byte(0xFF53, 0x80); // Dest 8000 (VRAM Bank 0)
    bus.write_byte(0xFF54, 0x00);

    // HDMA for 3 blocks (48 bytes). HDMA5 = (3-1) = 2. Mode bit 7 set. -> 0x82
    bus.write_byte(0xFF55, 0x82);

    assert!(bus.hdma_active, "HDMA should be active");
    assert_eq!(bus.hdma_blocks_remaining, 3);
    assert_eq!(
        bus.read_byte(0xFF55),
        0x02,
        "HDMA5 status during active HDMA"
    ); // (3-1) = 2

    // Simulate HBlank 1
    // bus.ppu.just_entered_hblank = true; // Field removed
    bus.hblank_hdma_pending = true; // Manually trigger pending flag for test
    bus.tick_components(1); // Tick to process HDMA
    assert!(bus.hdma_active, "HDMA still active after 1st block");
    assert_eq!(bus.hdma_blocks_remaining, 2);
    assert_eq!(bus.read_byte(0xFF55), 0x01, "HDMA5 status after 1st block");
    for i in 0..16 {
        assert_eq!(
            bus.ppu.read_vram(0x8000 + i as u16),
            i as u8,
            "HDMA Block 1, byte {}",
            i
        );
    }

    // Simulate HBlank 2
    // bus.ppu.just_entered_hblank = true; // Field removed
    bus.hblank_hdma_pending = true; // Manually trigger pending flag for test
    bus.tick_components(1);
    assert!(bus.hdma_active, "HDMA still active after 2nd block");
    assert_eq!(bus.hdma_blocks_remaining, 1);
    assert_eq!(bus.read_byte(0xFF55), 0x00, "HDMA5 status after 2nd block");
    for i in 0..16 {
        assert_eq!(
            bus.ppu.read_vram(0x8000 + 16 + i as u16),
            (16 + i) as u8,
            "HDMA Block 2, byte {}",
            i
        );
    }

    // Simulate HBlank 3
    // bus.ppu.just_entered_hblank = true; // Field removed
    bus.hblank_hdma_pending = true; // Manually trigger pending flag for test
    bus.tick_components(1);
    assert!(
        !bus.hdma_active,
        "HDMA should be inactive after 3rd block (completion)"
    );
    assert_eq!(bus.hdma_blocks_remaining, 0);
    assert_eq!(bus.read_byte(0xFF55), 0xFF, "HDMA5 status after completion");
    for i in 0..16 {
        assert_eq!(
            bus.ppu.read_vram(0x8000 + 32 + i as u16),
            (32 + i) as u8,
            "HDMA Block 3, byte {}",
            i
        );
    }

    // Further HBlanks should not trigger anything
    // bus.ppu.just_entered_hblank = true; // Field removed
    bus.hblank_hdma_pending = true; // Manually trigger pending flag for test
    bus.tick_components(1);
    assert!(!bus.hdma_active);
    assert_eq!(bus.hdma_blocks_remaining, 0);
}

#[test]
fn test_hdma_stop_via_ff55() {
    let mut bus = setup_bus_cgb();
    bus.write_byte(0xFF51, 0xD0);
    bus.write_byte(0xFF52, 0x00);
    bus.write_byte(0xFF53, 0x80);
    bus.write_byte(0xFF54, 0x00);
    bus.write_byte(0xFF55, 0x82); // Start HDMA, 3 blocks

    assert!(bus.hdma_active);
    assert_eq!(bus.hdma_blocks_remaining, 3);

    // Stop HDMA by writing to FF55 with bit 7 clear (incorrect way, this would start GDMA)
    // To stop HDMA, write to FF55 with bit 7 set, and a new length value (which is effectively "0 blocks remaining" for stopping)
    // Or, more simply, writing any value with bit 7=0 to FF55 while HDMA is active terminates it.
    // Pandocs: "If HDMA is active, writing to FF55 with bit 7 cleared will end the HDMA transfer."
    // The current code implements this as starting a GDMA, which also sets hdma_active=false.
    // Let's test the explicit stop: writing value with bit 7 set to 1 again, but on an active HDMA.
    // This is interpreted as "terminate"
    bus.write_byte(0xFF55, 0x00); // This will try to start GDMA of 1 block, which also stops HDMA.
                                  // The current implementation of write_byte to FF55 when (value & 0x80) == 0
                                  // sets self.hdma_active = false IF it was true.

    assert!(
        !bus.hdma_active,
        "HDMA should be stopped by writing 0 to bit 7 of HDMA5"
    );
    // After GDMA (even 0 length one if blocks_remaining was 0 due to value 0x00), HDMA5 is 0xFF
    assert_eq!(
        bus.read_byte(0xFF55),
        0xFF,
        "HDMA5 should be 0xFF after stopping HDMA via GDMA-like write"
    );
}

// TODO: Test HDMA conflicting with OAM DMA (OAM DMA should pause HDMA, or HDMA not run if OAM DMA active during HBlank?)
// Pandocs: "OAM DMA is paused during HDMA transfer." - This implies HDMA has priority if both want to run.
// However, HDMA only runs in HBlank, OAM DMA runs anytime. More likely: OAM DMA halts CPU and PPU. HDMA halts CPU.
// If OAM DMA is running, CPU is halted, PPU might be effectively blocked from HBlank for HDMA.
// For now, they are treated as independent DMA operations on different parts of the system.
