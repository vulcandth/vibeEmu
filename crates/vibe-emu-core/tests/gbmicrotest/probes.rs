//! Source-specific adapters for ROMs predating the FF80..FF82 protocol.
//! None of these ROMs reads joypad input. See ../gbmicrotest_probes.md.
use std::path::Path;
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{DmgRevision, Model},
};

pub fn handles(name: &str) -> bool {
    vram_result(name).is_some()
        || screenshot(name).is_some()
        || matches!(
            name,
            "001-vram_unlocked.gb"
                | "007-lcd_on_stat.gb"
                | "400-dma.gb"
                | "audio_testbench.gb"
                | "cpu_bus_1.gb"
                | "dma_basic.gb"
                | "flood_vram.gb"
                | "poweron.gb"
                | "toggle_lcdc.gb"
                | "wave_write_to_0xC003.gb"
        )
}

fn vram_result(name: &str) -> Option<u8> {
    Some(match name {
        "000-oam_lock.gb" | "mode2_stat_int_to_oam_unlock.gb" => 0xff,
        "000-write_to_x8000.gb"
        | "004-tima_boot_phase.gb"
        | "004-tima_cycle_timer.gb"
        | "ppu_spritex_vs_scx.gb" => 0x55,
        "002-vram_locked.gb" => 0x84,
        "500-scx-timing.gb" | "minimal.gb" => 0x49,
        "lcdon_write_timing.gb" | "ly_while_lcd_off.gb" => 0,
        _ => return None,
    })
}

fn screenshot(name: &str) -> Option<&str> {
    match name {
        "800-ppu-latch-scx.gb"
        | "801-ppu-latch-scy.gb"
        | "802-ppu-latch-tileselect.gb"
        | "803-ppu-latch-bgdisplay.gb"
        | "oam_sprite_trashing.gb"
        | "ppu_scx_vs_bgp.gb"
        | "ppu_sprite_testbench.gb"
        | "ppu_win_vs_wx.gb"
        | "ppu_wx_early.gb" => name.strip_suffix(".gb"),
        _ => None,
    }
}

fn equal_bytes(label: &str, actual: &[u8], expected: &[u8]) -> Result<(), String> {
    if actual.len() != expected.len() {
        return Err(format!("{label}: length mismatch"));
    }
    if let Some((i, (a, e))) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .find(|(_, (a, e))| a != e)
    {
        return Err(format!("{label}[{i:#x}]: actual={a:02X}, expected={e:02X}"));
    }
    Ok(())
}

pub fn run(name: &str, rom: Vec<u8>) -> Result<(), String> {
    let dma_rom_source = rom[0x200..0x2a0].to_vec();
    let mut gb = GameBoy::new(Model::Dmg(DmgRevision::RevC));
    gb.mmu.load_cart(Cartridge::from_bytes(rom));
    gb.mmu
        .ppu
        .set_dmg_palette([0xffffff, 0xaaaaaa, 0x555555, 0]);
    let expected_byte = vram_result(name);
    let mut frames = 0;
    let mut noise_levels = 0u16;
    while gb.cpu.cycles < super::MAX_CYCLES {
        let pc = gb.cpu.pc;
        // These ROMs hold their result in A and display it with an endless
        // LD [$8000],A / JR loop. In particular the sprite/scroll sweep uses
        // separate $55 success and $FF failure loops, not HRAM status bytes.
        let display_result = expected_byte.is_some()
            && !gb.cpu.halted
            && (0..5).map(|i| gb.mmu.read_byte(pc.wrapping_add(i))).eq([
                0xea,
                0x00,
                0x80,
                0x18,
                if name == "000-write_to_x8000.gb" {
                    0xf9
                } else {
                    0xfb
                },
            ]);
        let result = gb.cpu.a;
        gb.cpu.step(&mut gb.mmu);
        if gb.cpu.faulted || gb.cpu.stopped {
            return Err(format!("CPU fault/STOP at PC={:04X}", gb.cpu.pc));
        }
        if display_result {
            return equal_bytes("VRAM display result", &[result], &[expected_byte.unwrap()]);
        }
        if name == "audio_testbench.gb" && gb.cpu.pc == 0x178 {
            noise_levels |= 1 << gb.mmu.apu.pcm_samples()[3];
        }
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
            if expected_byte.is_none() && frames == 6 {
                break;
            }
        }
    }
    if frames < 6 || expected_byte.is_some() {
        return Err(format!(
            "probe did not reach its observation point: PC={:04X}",
            gb.cpu.pc
        ));
    }
    if let Some(image) = screenshot(name) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/reference_screenshots/gbmicrotest")
            .join(format!("{image}.png"));
        let (w, h, expected) = super::common::load_png_rgb(path);
        assert_eq!((w, h), (160, 144));
        for (i, (&actual, &[r, g, b])) in gb
            .mmu
            .ppu
            .framebuffer()
            .iter()
            .zip(expected.iter())
            .enumerate()
        {
            let color = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            if actual != color {
                return Err(format!(
                    "pixel ({},{}): actual={actual:06X}, expected={color:06X}",
                    i % 160,
                    i / 160
                ));
            }
        }
        return Ok(());
    }
    match name {
        "001-vram_unlocked.gb" => {
            equal_bytes("VRAM after OAM ISR", &gb.mmu.ppu.vram[0][..1], &[0x55])
        }
        "007-lcd_on_stat.gb" => {
            if gb.cpu.pc != 0x4d {
                return Err("LCD reset ISR did not finish".into());
            }
            equal_bytes(
                "LY sampled after LCD restart",
                &[gb.cpu.a, gb.mmu.ppu.vram[0][0]],
                &[0, 0],
            )
        }
        "400-dma.gb" => equal_bytes("ROM DMA", &gb.mmu.ppu.oam, &dma_rom_source),
        "dma_basic.gb" => equal_bytes(
            "VRAM DMA",
            &gb.mmu.ppu.oam,
            include_bytes!("../reference_data/gbmicrotest/dma_basic.bin"),
        ),
        "flood_vram.gb" => equal_bytes(
            "VRAM write sweep",
            &gb.mmu.ppu.vram[0],
            include_bytes!("../reference_data/gbmicrotest/flood_vram.bin"),
        ),
        "cpu_bus_1.gb" => equal_bytes("HRAM write loop", &gb.mmu.hram[..1], &[0x55]),
        "wave_write_to_0xC003.gb" => equal_bytes("WRAM write loop", &gb.mmu.wram[0][3..4], &[0x55]),
        "poweron.gb" => equal_bytes(
            "NR10 read-mask display",
            &gb.mmu.ppu.vram[0][..256],
            &[0x80; 256],
        ),
        "toggle_lcdc.gb" => {
            if gb.cpu.pc != 0x1a0 {
                return Err("LCDC toggle sequence did not finish".into());
            }
            equal_bytes(
                "LCD off state",
                &[
                    gb.mmu.read_byte(0xff40),
                    gb.mmu.read_byte(0xff44),
                    gb.mmu.ppu.mode(),
                ],
                &[0, 0, 0],
            )
        }
        "audio_testbench.gb" => {
            let state = gb.mmu.apu.debug_state();
            if !state.ch4_enabled
                || !state.ch4_dac_enabled
                || state.ch4_length_enable
                || noise_levels != 0x8001
            {
                return Err(format!(
                    "noise channel did not run: state={state:?}, levels={noise_levels:04X}"
                ));
            }
            equal_bytes(
                "noise setup",
                &[
                    gb.mmu.read_byte(0xff21),
                    gb.mmu.read_byte(0xff22),
                    gb.mmu.read_byte(0xff24),
                    gb.mmu.read_byte(0xff25),
                ],
                &[0xf0, 0x5f, 0x77, 0xff],
            )
        }
        _ => unreachable!("unregistered probe {name}"),
    }
}
