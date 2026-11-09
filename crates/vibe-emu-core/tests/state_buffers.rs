//! Save-state buffers remain usable without filesystem-backed slot storage.
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

fn machine(model: Model) -> GameBoy {
    let mut rom = vec![0; 0x8000];
    rom[0x143] = if model.is_cgb() { 0x80 } else { 0 };
    rom[0x100..0x103].copy_from_slice(&[0xc3, 0x50, 0x01]);
    rom[0x150..0x159].copy_from_slice(&[0x3c, 0xea, 0x00, 0xc0, 0x00, 0xc3, 0x50, 0x01, 0]);
    let mut gb = GameBoy::new(model);
    gb.load_cart(Cartridge::from_bytes(rom));
    for (address, value) in [
        (0xff26, 0x80),
        (0xff24, 0x77),
        (0xff25, 0x11),
        (0xff12, 0xf3),
        (0xff13, 0xa5),
        (0xff14, 0x87),
        (0xff07, 5),
    ] {
        gb.mmu.write_byte(address, value);
    }
    gb
}

#[test]
fn buffers_restore_hardware_and_audio_without_a_state_store() {
    for model in [Model::default(), Model::Mgb, Model::Cgb(Default::default())] {
        let mut reference = machine(model);
        let expected_audio = reference.mmu.apu.enable_output(48_000);
        for _ in 0..37 {
            reference.cpu.step(&mut reference.mmu);
        }
        reference.mmu.write_byte(0xff46, 0xc0);
        reference.cpu.step(&mut reference.mmu);
        while expected_audio.pop_stereo().is_some() {}
        let checkpoint = reference.save_state().unwrap();
        let mut restored = machine(model);
        let audio = restored.mmu.apu.enable_output(48_000);
        restored.load_state(&checkpoint).unwrap();
        for iteration in 0..128 {
            let budget = [1, 17, 256, 1024][iteration % 4];
            reference.cpu.run_for_dots(&mut reference.mmu, budget);
            restored.cpu.run_for_dots(&mut restored.mmu, budget);
            assert_eq!(reference.cpu.debug_state(), restored.cpu.debug_state());
            assert_eq!(reference.mmu.if_reg, restored.mmu.if_reg);
            assert_eq!(reference.mmu.dot_div, restored.mmu.dot_div);
            assert_eq!(reference.mmu.wram, restored.mmu.wram);
            assert_eq!(reference.mmu.ppu.oam, restored.mmu.ppu.oam);
            assert_eq!(
                reference.mmu.ppu.framebuffer(),
                restored.mmu.ppu.framebuffer()
            );
            assert_eq!(
                format!("{:?}", reference.mmu.timer),
                format!("{:?}", restored.mmu.timer)
            );
            loop {
                let expected = expected_audio.pop_stereo();
                assert_eq!(audio.pop_stereo(), expected);
                if expected.is_none() {
                    break;
                }
            }
            reference.mmu.ppu.clear_frame_flag();
            restored.mmu.ppu.clear_frame_flag();
        }
        let before = restored.save_state().unwrap();
        assert!(
            restored
                .load_state(&checkpoint[..checkpoint.len() / 2])
                .is_err()
        );
        assert_eq!(restored.save_state().unwrap(), before);
    }
}
