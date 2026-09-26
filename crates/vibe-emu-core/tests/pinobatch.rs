mod common;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

const FRAME_DOTS: u64 = 70_224;

fn machine(name: &str, model: Model) -> GameBoy {
    let mut gb = GameBoy::new(model);
    let rom = std::fs::read(common::rom_path(format!("little-things-gb/{name}.gb")))
        .expect("pinobatch ROM not found");
    gb.mmu.load_cart(Cartridge::from_bytes(rom));
    gb.mmu
        .ppu
        .set_dmg_palette([0xFFFFFF, 0xAAAAAA, 0x555555, 0]);
    gb
}

fn step(gb: &mut GameBoy) {
    gb.cpu.step(&mut gb.mmu);
    assert!(
        !gb.cpu.stopped && !gb.cpu.faulted,
        "unexpected stop at {:04X}",
        gb.cpu.pc
    );
}

fn run_dots(gb: &mut GameBoy, dots: u64) {
    let end = gb.cpu.cycles + dots;
    while gb.cpu.cycles < end {
        step(gb);
    }
}

fn matches_reference(gb: &GameBoy, name: &str) -> bool {
    let (width, height, pixels) =
        common::load_png_rgb(common::rom_path(format!("little-things-gb/{name}.png")));
    assert_eq!((width, height), (160, 144));
    gb.mmu
        .ppu
        .framebuffer()
        .iter()
        .zip(pixels.iter())
        .all(|(&actual, &[r, g, b])| {
            actual == (u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        })
}

fn firstwhite(model: Model) {
    let mut gb = machine("firstwhite", model);
    run_dots(&mut gb, FRAME_DOTS * 30);
    // Check repeated output frames: alternating LCD enable/disable must never
    // expose the error image drawn into VRAM by this ROM.
    let end = gb.cpu.cycles + FRAME_DOTS * 60;
    let mut frames = 0;
    gb.mmu.ppu.clear_frame_flag();
    while gb.cpu.cycles < end {
        step(&mut gb);
        if gb.mmu.ppu.frame_ready() {
            assert!(
                matches_reference(&gb, "firstwhite-dmg-cgb"),
                "{model:?}: firstwhite displayed the error image"
            );
            frames += 1;
            gb.mmu.ppu.clear_frame_flag();
        }
    }
    assert!(frames >= 20, "ROM stopped delivering frames");
}

fn tellinglys(model: Model, varied_phase: bool) {
    let mut gb = machine("tellinglys", model);
    run_dots(&mut gb, FRAME_DOTS * 30);
    // Script real button transitions at reproducible phases, without injecting
    // randomness into the emulator or changing ROM state. Include VBlank and
    // enough distinct LY bits to exercise the ROM's entropy calculation.
    for (button, ly) in [13, 31, 53, 71, 89, 107, 127, 149].into_iter().enumerate() {
        let target = if varied_phase { ly } else { 144 };
        let deadline = gb.cpu.cycles + FRAME_DOTS * 2;
        while gb.mmu.ppu.ly() != target {
            assert!(gb.cpu.cycles < deadline, "did not reach LY={target}");
            step(&mut gb);
        }
        gb.mmu
            .input
            .update_state(!(1 << button), &mut gb.mmu.if_reg);
        run_dots(&mut gb, FRAME_DOTS * 8);
        // The ROM disables joypad IRQs after accepting exactly one key, then
        // rearms only after release. Check that every scripted press was seen.
        assert_eq!(gb.mmu.ie_reg & 0x10, 0, "button {button} was not accepted");
        gb.mmu.input.update_state(0xFF, &mut gb.mmu.if_reg);
        run_dots(&mut gb, FRAME_DOTS * 8);
        if button < 7 {
            assert_ne!(
                gb.mmu.ie_reg & 0x10,
                0,
                "ROM did not rearm after button {button}"
            );
        }
    }
    run_dots(&mut gb, FRAME_DOTS * 300);
    let reference = if model.is_cgb() {
        "tellinglys-cgb"
    } else {
        "tellinglys-dmg"
    };
    assert_eq!(
        matches_reference(&gb, reference),
        varied_phase,
        "{model:?}: tellinglys result, varied_phase={varied_phase}"
    );
}

#[test]
fn firstwhite_dmg() {
    firstwhite(Model::default());
}

#[test]
fn firstwhite_cgb() {
    firstwhite(Model::Cgb(Default::default()));
}

#[test]
fn tellinglys_dmg() {
    tellinglys(Model::default(), true);
}

#[test]
fn tellinglys_cgb() {
    tellinglys(Model::Cgb(Default::default()), true);
}

#[test]
fn tellinglys_rejects_vblank_only_input() {
    tellinglys(Model::default(), false);
    tellinglys(Model::Cgb(Default::default()), false);
}
