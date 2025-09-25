use vibe_emu_core::apu::Apu;

fn tick_machine(apu: &mut Apu, div: &mut u16, cycles: u16) {
    let prev = *div;
    *div = div.wrapping_add(cycles);
    apu.tick(prev, *div, false);
    apu.step(cycles);
}

#[test]
fn extra_length_clocking_disables_channel() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF11, 0x3F); // length =1
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80); // trigger with length disabled
    assert_eq!(apu.ch1_length(), 1);
    apu.write_reg(0xFF14, 0x40); // enable length
    assert_eq!(apu.ch1_length(), 0);
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0);
}

#[test]
fn trigger_length_set_to_63_when_zero() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF11, 0x3F); // length=1
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80); // trigger with length disabled
    apu.write_reg(0xFF14, 0x40); // enable length -> length becomes 0
    assert_eq!(apu.ch1_length(), 0);
    apu.write_reg(0xFF14, 0xC0); // retrigger when next step=1
    assert_eq!(apu.ch1_length(), 63);
}

#[test]
fn trigger_envelope_timer_plus_one() {
    let mut apu = Apu::new();
    let mut div = 0u16;
    for _ in 0..(6 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.sequencer_step(), 6);
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF1); // period=1
    apu.write_reg(0xFF14, 0x80);
    assert_eq!(apu.ch1_envelope_timer(), 2);
}

#[test]
fn noise_shift_15_freezes_lfsr() {
    let mut apu = Apu::new();
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
fn sweep_negate_clear_disables() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x19); // subtract mode
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x82); // trigger
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 1);
    apu.write_reg(0xFF10, 0x11); // clear negate
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0);
}

#[test]
fn wave_retrigger_corrupts_ram() {
    let mut apu = Apu::new();
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
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    // Use increase mode with period 0 for a consistent volume increment
    apu.write_reg(0xFF12, 0x98); // initial volume 9
    apu.write_reg(0xFF14, 0x80);
    apu.write_reg(0xFF12, 0x08); // zombie write
    assert_eq!(apu.ch1_volume(), 10);
}
