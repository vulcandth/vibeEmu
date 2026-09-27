#[path = "common/sgb_rom.rs"]
mod sgb_rom;
use sgb_rom::initial_border_rom;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

fn machine(model: Model, enhanced: bool) -> GameBoy {
    let mut rom = vec![0; 0x8000];
    if enhanced {
        rom[0x146] = 3;
        rom[0x14b] = 0x33;
    }
    let mut gb = GameBoy::new(model);
    gb.load_cart(Cartridge::from_bytes(rom));
    gb
}

fn hybrid_machine() -> GameBoy {
    let mut gb = GameBoy::new(Model::from_cgb_flag(true));
    gb.enable_sgb_extensions();
    let mut rom = vec![0; 0x8000];
    rom[0x143] = 0x80;
    rom[0x146] = 3;
    rom[0x14b] = 0x33;
    gb.load_cart(Cartridge::from_bytes(rom));
    gb
}

fn command(gb: &mut GameBoy, id: u8, parameters: &[u8]) {
    let mut packet = [0; 16];
    packet[0] = id << 3 | 1;
    packet[1..1 + parameters.len()].copy_from_slice(parameters);
    gb.mmu.write_byte(0xff00, 0);
    gb.mmu.write_byte(0xff00, 0x30);
    for byte in packet {
        for bit in 0..8 {
            gb.mmu
                .write_byte(0xff00, if byte & (1 << bit) != 0 { 0x10 } else { 0x20 });
            gb.mmu.write_byte(0xff00, 0x30);
        }
    }
    gb.mmu.write_byte(0xff00, 0x20);
    gb.mmu.write_byte(0xff00, 0x30);
}

fn frame(gb: &mut GameBoy) {
    gb.mmu.ppu.clear_frame_flag();
    for _ in 0..18_000 {
        gb.mmu.ppu.step(4, &mut gb.mmu.if_reg);
        if gb.mmu.ppu.frame_ready() {
            return;
        }
    }
    panic!("PPU did not finish a frame");
}

#[test]
fn lcd_shades_survive_duplicate_user_colors_and_sprite_palettes() {
    for model in [Model::Sgb, Model::Sgb2] {
        let mut gb = machine(model, true);
        gb.mmu.write_byte(0xff40, 0);
        gb.mmu.write_byte(0xff42, 0);
        gb.mmu.write_byte(0xff43, 0);
        gb.mmu.write_byte(0xff47, 0xe4);
        gb.mmu.write_byte(0xff48, 0x0c); // sprite color 1 -> shade 3
        gb.mmu.ppu.set_dmg_palette([0x123456; 4]);
        for y in 0..8 {
            gb.mmu.write_byte(0x8000 + y * 2, 0xff); // BG color 1
            gb.mmu.write_byte(0x8001 + y * 2, 0);
        }
        for i in 0..1024 {
            gb.mmu.write_byte(0x9800 + i, 0);
        }
        for (i, value) in [16, 8, 0, 0].into_iter().enumerate() {
            gb.mmu.write_byte(0xfe00 + i as u16, value);
        }
        // Palette 0: black, red, green, blue.
        command(&mut gb, 0, &[0, 0, 0x1f, 0, 0xe0, 3, 0, 0x7c]);
        gb.mmu.write_byte(0xff40, 0x93);
        frame(&mut gb); // LCD startup frame suppresses output
        frame(&mut gb);
        let pixels = gb.mmu.ppu.display_framebuffer();
        assert_eq!(pixels[40 * 256 + 48], 0x0000ff, "{model:?}: sprite shade");
        assert_eq!(
            pixels[40 * 256 + 56],
            0xff0000,
            "{model:?}: background shade"
        );
        assert_eq!(gb.mmu.ppu.framebuffer()[8], 0x123456);
        assert!(gb.mmu.ppu.framebuffer().iter().all(|p| p >> 24 == 0));
    }
}

#[test]
fn controllers_are_independent_and_reset_returns_to_one_player() {
    for model in [Model::Sgb, Model::Sgb2] {
        let mut gb = machine(model, true);
        command(&mut gb, 0x11, &[3]);
        assert_eq!(gb.mmu.ppu.sgb.as_ref().unwrap().player_count(), 4);
        for player in 0..4 {
            gb.mmu.input.set_player_state(player, !(1 << player));
        }
        for player in 0..4 {
            assert_eq!(gb.mmu.read_byte(0xff00), 0xff - player);
            gb.mmu.write_byte(0xff00, 0x20);
            assert_eq!(gb.mmu.read_byte(0xff00) & 15, 15 ^ (1 << player));
            gb.mmu.write_byte(0xff00, 0x10);
            assert_eq!(gb.mmu.read_byte(0xff00) & 15, 15);
            gb.mmu.write_byte(0xff00, 0x30);
        }
        gb.mmu.if_reg = 0;
        gb.mmu
            .input
            .update_player_state(3, 0xe7, &mut gb.mmu.if_reg);
        assert_eq!(gb.mmu.if_reg, 0x10);
        gb.mmu.if_reg = 0;
        gb.mmu
            .input
            .update_player_state(3, 0xff, &mut gb.mmu.if_reg);
        assert_eq!(gb.mmu.if_reg, 0);
        gb.reset();
        assert_eq!(gb.mmu.ppu.sgb.as_ref().unwrap().player_count(), 1);
        assert_eq!(gb.mmu.ppu.display_dimensions(), (256, 224));
    }
}

#[test]
fn header_gates_sgb_commands_and_other_models_keep_native_video() {
    let mut gb = machine(Model::Sgb, false);
    command(&mut gb, 0x11, &[3]);
    assert_eq!(gb.mmu.ppu.sgb.as_ref().unwrap().player_count(), 1);
    for model in [Model::default(), Model::Mgb, Model::Agb, Model::Agb0] {
        let mut gb = machine(model, true);
        command(&mut gb, 0x11, &[3]);
        assert!(gb.mmu.ppu.sgb.is_none());
        assert_eq!(gb.mmu.ppu.display_dimensions(), (160, 144));
        assert_eq!(gb.mmu.ppu.display_framebuffer(), gb.mmu.ppu.framebuffer());
    }
}

#[test]
fn border_download_through_lcd_preserves_tiles_and_palette_data() {
    fn upload(gb: &mut GameBoy, id: u8, data: &[u8; 4096]) {
        gb.mmu.write_byte(0xff40, 0);
        gb.mmu.write_byte(0xff42, 0);
        gb.mmu.write_byte(0xff43, 0);
        gb.mmu.write_byte(0xff47, 0xe4);
        for (offset, value) in data.iter().enumerate() {
            gb.mmu.write_byte(0x8000 + offset as u16, *value);
        }
        for tile in 0..256u16 {
            gb.mmu
                .write_byte(0x9800 + tile / 20 * 32 + tile % 20, tile as u8);
        }
        gb.mmu.write_byte(0xff40, 0x91);
        command(gb, id, &[0]);
        for _ in 0..3 {
            frame(gb);
        }
    }

    for model in [Model::Sgb, Model::Sgb2, Model::from_cgb_flag(true)] {
        let mut gb = if model.is_cgb() {
            hybrid_machine()
        } else {
            machine(model, true)
        };
        let mut tiles = [0; 4096];
        // SNES tile 1, all pixels color 1. Tile 0 remains transparent.
        for row in 0..8 {
            tiles[32 + row * 2] = 0xff;
        }
        command(&mut gb, 0x17, &[2]); // transfers continue behind a black GB mask
        upload(&mut gb, 0x13, &tiles);
        let mut map = [0; 4096];
        for y in 0..28 {
            for x in 0..32 {
                let entry = if (6..26).contains(&x) && (5..23).contains(&y) {
                    0x1000u16
                } else {
                    0x1001
                };
                map[(y * 32 + x) * 2..(y * 32 + x) * 2 + 2].copy_from_slice(&entry.to_le_bytes());
            }
        }
        map[2050..2052].copy_from_slice(&0x03e0u16.to_le_bytes()); // green border
        upload(&mut gb, 0x14, &map);
        let output = gb.mmu.ppu.display_framebuffer();
        for y in 0..224 {
            for x in 0..256 {
                let expected = if (48..208).contains(&x) && (40..184).contains(&y) {
                    0
                } else {
                    0x00ff00
                };
                assert_eq!(output[y * 256 + x], expected, "{model:?} pixel ({x}, {y})");
            }
        }
    }
}

#[test]
fn hybrid_keeps_cgb_colors_timing_and_host_across_resets() {
    let mut gb = hybrid_machine();
    assert!(gb.model.is_cgb());
    assert_eq!(gb.model.clock_hz(), 4_194_304);
    assert_eq!(gb.cpu.a, 0x11);
    gb.mmu.write_byte(0xff40, 0);
    gb.mmu.write_byte(0xff42, 0);
    gb.mmu.write_byte(0xff43, 0);
    gb.mmu.write_byte(0xff47, 0); // BGP does not recolor native CGB pixels
    for row in 0..8 {
        gb.mmu.write_byte(0x8000 + row * 2, 0xff);
        gb.mmu.write_byte(0x8001 + row * 2, 0);
    }
    for tile in 0..1024 {
        gb.mmu.write_byte(0x9800 + tile, 0);
    }
    gb.mmu.write_byte(0xff68, 0x82); // BG palette 0, color 1 = green
    gb.mmu.write_byte(0xff69, 0xe0);
    gb.mmu.write_byte(0xff69, 3);
    command(&mut gb, 0, &[0, 0, 0x1f, 0]); // SGB color 1 = red
    command(&mut gb, 0x11, &[1]);
    assert_eq!(gb.mmu.ppu.sgb.as_ref().unwrap().player_count(), 2);
    gb.mmu.write_byte(0xff40, 0x91);
    frame(&mut gb);
    frame(&mut gb);
    assert_eq!(gb.mmu.ppu.display_framebuffer()[40 * 256 + 48], 0x00ff00);
    assert_eq!(gb.mmu.ppu.framebuffer()[0], 0x00ff00);
    assert_eq!(gb.mmu.ppu.bg_palette_color(0, 1), 0x00ff00);
    command(&mut gb, 0x17, &[1]);
    gb.mmu.write_byte(0xff40, 0);
    gb.mmu.write_byte(0xff68, 0x82);
    gb.mmu.write_byte(0xff69, 0x1f);
    gb.mmu.write_byte(0xff69, 0);
    gb.mmu.write_byte(0xff40, 0x91);
    frame(&mut gb);
    frame(&mut gb);
    assert_eq!(gb.mmu.ppu.display_framebuffer()[40 * 256 + 48], 0x00ff00);
    command(&mut gb, 0x17, &[0]);
    frame(&mut gb);
    assert_eq!(gb.mmu.ppu.display_framebuffer()[40 * 256 + 48], 0xff0000);
    gb.reset();
    assert!(gb.model.is_cgb());
    assert_eq!(gb.cpu.a, 0x11);
    assert_eq!(gb.mmu.ppu.display_dimensions(), (256, 224));
    assert_eq!(gb.mmu.ppu.sgb.as_ref().unwrap().player_count(), 1);
    gb.reset_power_on();
    assert!(gb.model.is_cgb());
    assert!(gb.mmu.ppu.sgb.is_some());
    assert_eq!(gb.cpu.pc, 0);
}

#[test]
fn initial_border_runs_sgb_startup_then_keeps_cgb_state_and_joyp() {
    for boot in [false, true] {
        let mut gb = GameBoy::new(Model::from_cgb_flag(true));
        gb.load_cart(Cartridge::from_bytes(initial_border_rom()));
        gb.mmu.cart.as_mut().unwrap().ram[0] = 0x57;
        gb.mmu.wram[0][0] = 0x23;
        // Snapshot reads include palette data; turn off its auto-increment.
        gb.mmu.write_byte(0xff68, 0);
        gb.mmu.write_byte(0xff6a, 0);
        let before = gb.capture_boot_handoff_snapshot();
        // Distinct SGB boot ROM: jump to the standard unmap at 00FE.
        let mut sgb_boot = vec![0; 256];
        sgb_boot[..3].copy_from_slice(&[0xc3, 0xfc, 0]);
        sgb_boot[252..].copy_from_slice(&[0x3e, 1, 0xe0, 0x50]);
        assert!(gb.borrow_sgb_border(boot.then_some(sgb_boot.as_slice()), 120));
        assert_eq!(gb.capture_boot_handoff_snapshot(), before);
        assert_eq!(gb.mmu.cart.as_ref().unwrap().ram[0], 0x57);
        let host = gb.mmu.ppu.sgb.as_ref().unwrap();
        assert!(!host.is_command_host());
        assert_eq!(host.player_count(), 1);
        assert_eq!(gb.mmu.ppu.display_framebuffer()[0], 0x00ff00);
        assert_eq!(
            gb.mmu.ppu.display_framebuffer()[40 * 256 + 48],
            gb.mmu.ppu.framebuffer()[0],
            "capture must display native pixels before the first CGB frame"
        );
        command(&mut gb, 0x11, &[3]);
        command(&mut gb, 0x17, &[2]); // CGB writes cannot mask gameplay
        assert_eq!(gb.mmu.ppu.sgb.as_ref().unwrap().player_count(), 1);
        gb.cpu.run_for_dots(&mut gb.mmu, 1024);
        assert_eq!(gb.mmu.wram[0][0], 0x42, "CGB code path");
        frame(&mut gb);
        frame(&mut gb);
        for y in 0..144 {
            for x in 0..160 {
                assert_eq!(
                    gb.mmu.ppu.display_framebuffer()[(y + 40) * 256 + x + 48],
                    gb.mmu.ppu.framebuffer()[y * 160 + x],
                    "borrowed border must keep native CGB pixels"
                );
            }
        }
        gb.mmu.write_byte(0xff40, 0);
        frame(&mut gb);
        assert_eq!(
            gb.mmu.ppu.display_framebuffer()[40 * 256 + 48],
            0xffffff,
            "LCD-off in initial-border mode must blank like CGB, not freeze like SGB"
        );
        assert_eq!(gb.mmu.ppu.display_framebuffer()[0], 0x00ff00);
        for reset in [false, true] {
            if reset {
                gb.reset_power_on();
            } else {
                gb.reset();
            }
            assert!(!gb.mmu.ppu.sgb.as_ref().unwrap().is_command_host());
            assert_eq!(gb.mmu.ppu.display_framebuffer()[0], 0x00ff00);
            assert_eq!(
                gb.mmu.ppu.display_framebuffer()[40 * 256 + 48],
                gb.mmu.ppu.framebuffer()[0]
            );
            let mut plain = GameBoy::new(Model::from_cgb_flag(true));
            for value in [0x00, 0x10, 0x20, 0x30] {
                gb.mmu.write_byte(0xff00, value);
                plain.mmu.write_byte(0xff00, value);
                assert_eq!(gb.mmu.read_byte(0xff00), plain.mmu.read_byte(0xff00));
            }
        }
    }
}

#[test]
fn initial_border_times_out_and_skips_ineligible_or_faulted_roms() {
    for kind in 0..5 {
        let mut rom = initial_border_rom();
        match kind {
            0 => rom[0x143] = 0,
            1 => rom[0x146] = 0,
            2 => rom[0x14b] = 0,
            3 => rom[0x100..0x102].copy_from_slice(&[0x18, 0xfe]),
            _ => rom[0x100] = 0xd3,
        }
        let mut gb = GameBoy::new(Model::from_cgb_flag(true));
        gb.load_cart(Cartridge::from_bytes(rom));
        gb.mmu.write_byte(0xff68, 0);
        gb.mmu.write_byte(0xff6a, 0);
        let before = gb.capture_boot_handoff_snapshot();
        assert!(!gb.borrow_sgb_border(None, 2));
        assert_eq!(gb.capture_boot_handoff_snapshot(), before);
        assert!(gb.mmu.ppu.sgb.is_none());
    }
}

#[test]
fn simultaneous_hybrid_does_not_run_the_sgb_only_startup_path() {
    let mut gb = GameBoy::new(Model::from_cgb_flag(true));
    gb.enable_sgb_extensions();
    gb.load_cart(Cartridge::from_bytes(initial_border_rom()));
    gb.cpu.run_for_dots(&mut gb.mmu, 1024);
    assert_eq!(gb.mmu.wram[0][0], 0x42);
    assert!(gb.mmu.ppu.sgb.as_ref().unwrap().border().is_none());
    assert!(gb.mmu.ppu.sgb.as_ref().unwrap().is_command_host());
}
