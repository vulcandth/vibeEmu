mod common;
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

const CGB_PALETTE: [u32; 4] = [0xFFFFFFFF, 0xFFC0C0C0, 0xFF808080, 0xFF000000];

#[test]
fn interrupt_time_cgb() {
    let mut gb = GameBoy::new_with_mode(true);
    let rom = std::fs::read(common::rom_path("blargg/interrupt_time/interrupt_time.gb"))
        .expect("rom not found");
    gb.mmu.load_cart(Cartridge::load(rom));

    let mut frames = 0u32;
    let start_cycles = gb.cpu.cycles;
    while frames < 600 {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
            if frames % 100 == 0 {
                eprintln!("Frame {}, cycles: {}", frames, gb.cpu.cycles - start_cycles);
            }
        }
    }

    let (width, height, expected) =
        common::load_png_rgb(common::rom_path("blargg/interrupt_time/interrupt_time-cgb.png"));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let frame = gb.mmu.ppu.framebuffer();
    
    // Debug: Print a text representation of the screen
    eprintln!("\nActual screen output:");
    for y in 0..18 {  // Show 18 rows
        for x in 0..20 {  // Show 20 columns (each is 8 pixels)
            let px = x * 8;
            let py = y * 8 + 4;  // Middle of the character
            let idx = py * 160 + px + 4;
            let color = frame[idx];
            let c = match color & 0x00FFFFFF {
                0x000000 => '#',  // Black
                0xFFFFFF => ' ',  // White
                _ => '.',  // Gray
            };
            eprint!("{}", c);
        }
        eprintln!();
    }
    
    let mut mismatches = 0;
    for (idx, pixel) in expected.iter().enumerate() {
        let pixel = *pixel;
        let expected_color = match pixel {
            [0x00, 0x00, 0x00] => CGB_PALETTE[3],
            [0x55, 0x55, 0x55] => CGB_PALETTE[2],
            [0xAA, 0xAA, 0xAA] => CGB_PALETTE[1],
            [0xFF, 0xFF, 0xFF] => CGB_PALETTE[0],
            _ => {
                continue;
            }
        };
        // Compare RGB only, ignoring alpha channel
        let actual_rgb = frame[idx] & 0x00FFFFFF;
        let expected_rgb = expected_color & 0x00FFFFFF;
        if actual_rgb != expected_rgb {
            mismatches += 1;
        }
    }
    
    if mismatches > 0 {
        eprintln!("\nFound {} pixel mismatches", mismatches);
    }
    assert_eq!(mismatches, 0, "Found {} pixel mismatches", mismatches);
}
