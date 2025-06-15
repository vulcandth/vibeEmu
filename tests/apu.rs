use vibeEmu::apu::Apu;

#[test]
fn frame_sequencer_tick() {
    let mut apu = Apu::new();
    assert_eq!(apu.sequencer_step(), 0);
    apu.step(8192);
    assert_eq!(apu.sequencer_step(), 1);
    apu.step(8192 * 7);
    assert_eq!(apu.sequencer_step(), 0);
}

#[test]
fn sample_generation() {
    let mut apu = Apu::new();
    // enable sound and channel 2 with simple settings
    apu.write_reg(0xFF26, 0x80); // master enable
    apu.write_reg(0xFF24, 0x77); // max volume
    apu.write_reg(0xFF25, 0x22); // ch2 left+right
    apu.write_reg(0xFF16, 0); // length
    apu.write_reg(0xFF17, 0xF0); // envelope
    apu.write_reg(0xFF18, 0); // freq low
    apu.write_reg(0xFF19, 0x80); // trigger
    // step enough cycles for a few samples
    for _ in 0..10 {
        apu.step(95);
    }
    assert!(apu.pop_sample().is_some());
}
#[test]
fn writes_ignored_when_disabled() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x00); // disable
    apu.write_reg(0xFF12, 0xF0);
    assert_eq!(apu.read_reg(0xFF12), 0x00);
    apu.write_reg(0xFF26, 0x80); // enable
    apu.write_reg(0xFF12, 0xF0);
    assert_eq!(apu.read_reg(0xFF12) & 0xF0, 0xF0);
}

#[test]
fn read_mask_unused_bits() {
    let apu = Apu::new();
    assert_eq!(apu.read_reg(0xFF11), 0xBF);
}

#[test]
fn wave_ram_access() {
    let mut apu = Apu::new();
    // write while channel 3 inactive
    apu.write_reg(0xFF30, 0x12);
    assert_eq!(apu.read_reg(0xFF30), 0x12);

    // start channel 3
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1E, 0x80); // trigger
    apu.write_reg(0xFF30, 0x34); // should be ignored
    assert_eq!(apu.read_reg(0xFF30), 0xFF);

    // disable DAC while length counter still running
    apu.write_reg(0xFF1A, 0x00);
    apu.write_reg(0xFF30, 0x56);
    assert_eq!(apu.read_reg(0xFF30), 0x56);

    // power cycle should not clear wave RAM
    apu.write_reg(0xFF26, 0x00);
    apu.write_reg(0xFF26, 0x80);
    assert_eq!(apu.read_reg(0xFF30), 0x56);
}

#[test]
fn dac_off_disables_channel() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable
    apu.write_reg(0xFF12, 0xF0); // envelope with volume
    apu.write_reg(0xFF14, 0x80); // trigger channel 1
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01);
    apu.write_reg(0xFF12, 0x00); // turn DAC off
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x00);
}

#[test]
fn sweep_trigger_and_step() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // master enable
    apu.write_reg(0xFF10, 0x11); // period=1, shift=1
    apu.write_reg(0xFF12, 0xF0); // envelope (DAC on)
    // set frequency 0x200
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x82); // high bits=2, trigger
    // immediately applied sweep -> freq should be 0x300
    assert_eq!(apu.ch1_frequency(), 0x300);
    // advance until the sequencer clocks sweep (step 2)
    apu.step(8192); // advance to step 1
    apu.step(8192); // advance to step 2
    apu.step(8192); // advance to step 3 (sweep clocked on previous step)
    assert_eq!(apu.ch1_frequency(), 0x480);
}
