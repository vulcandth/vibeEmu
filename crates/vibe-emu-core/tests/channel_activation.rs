use vibe_emu_core::apu::Apu;

fn tick_machine(apu: &mut Apu, div: &mut u16, cycles: u16) {
    let prev = *div;
    *div = div.wrapping_add(cycles);
    apu.tick(prev, *div, false);
    apu.step(cycles);
}

#[test]
fn channel1_triggers_when_dac_on() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF12, 0xF0); // DAC on
    apu.write_reg(0xFF14, 0x80); // trigger channel 1
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01);
}

#[test]
fn channel1_trigger_ignored_when_dac_off() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF12, 0x00); // DAC off
    apu.write_reg(0xFF14, 0x80); // attempt trigger
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x00);
}

#[test]
fn channel1_disabled_by_length_timer() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF11, 0x3F); // length = 1
    apu.write_reg(0xFF12, 0xF0); // DAC on
    apu.write_reg(0xFF14, 0x80); // trigger, length disabled
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01);
    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    apu.write_reg(0xFF14, 0x40); // enable length
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x00);
}

#[test]
#[ignore]
fn sweep_overflow_disables_channel1() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x01); // period=0, shift=1 (addition)
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0xF8);
    apu.write_reg(0xFF14, 0x87); // high bits=7, trigger -> overflow
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x00);
}
