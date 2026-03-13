use vibe_emu_core::{apu::Apu, hardware::Model};

fn tick_machine(apu: &mut Apu, div: &mut u16, cycles: u16) {
    let prev = *div;
    *div = div.wrapping_add(cycles);
    apu.tick(prev, *div, false);
    apu.step(cycles);
}

#[test]
fn noise_shift_15_freezes_lfsr() {
    let mut apu = Apu::new(Model::default());
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0xF0); // clock shift 15
    apu.write_reg(0xFF23, 0x80);
    let lfsr = apu.ch4_lfsr();
    let mut div = 0u16;
    for _ in 0..1024 {
        tick_machine(&mut apu, &mut div, 1);
    }
    assert_eq!(apu.ch4_lfsr(), lfsr);
}

#[test]
fn wave_retrigger_corrupts_ram() {
    let mut apu = Apu::new(Model::default());
    apu.write_reg(0xFF26, 0x80);
    for i in 0..0x10 {
        apu.write_reg(0xFF30 + i as u16, i as u8);
    }
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87);
    let mut div = 0u16;
    for _ in 0..18 {
        tick_machine(&mut apu, &mut div, 1);
    }
    let index = apu.ch3_position() / 2;
    apu.write_reg(0xFF1E, 0x87);
    let base = if index < 4 { index } else { index & !0x03 } as usize;
    for i in 0..4 {
        assert_eq!(
            apu.read_reg(0xFF30 + i as u16),
            apu.read_reg(0xFF30 + (base + i) as u16)
        );
    }
}

#[test]
fn zombie_mode_volume_change() {
    let mut apu = Apu::new(Model::default());
    apu.write_reg(0xFF26, 0x80);
    // Use increase mode with period 0 for a consistent volume increment
    apu.write_reg(0xFF12, 0x98); // initial volume 9
    apu.write_reg(0xFF14, 0x80);
    apu.write_reg(0xFF12, 0x08); // zombie write
    assert_eq!(apu.ch1_volume(), 10);
}
