use vibe_emu_core::apu::Apu;
use vibe_emu_core::hardware::CgbRevision;
use vibe_emu_core::mmu::Mmu;

fn tick_machine(apu: &mut Apu, div: &mut u16, cycles: u16) {
    let prev = *div;
    *div = div.wrapping_add(cycles);
    apu.tick(prev, *div, false);
    apu.step(cycles);
}

#[test]
fn frame_sequencer_tick() {
    let mut apu = Apu::new();
    let mut div = 0u16;
    assert_eq!(apu.sequencer_step(), 0);
    for _ in 0..(16 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.sequencer_step(), 0);
    for _ in 0..(8192 * 7 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.sequencer_step(), 0);
}

#[test]
fn sample_generation() {
    let mut apu = Apu::new();
    let consumer = apu.enable_output(44_100);
    // enable sound and channel 2 with simple settings
    apu.write_reg(0xFF26, 0x80); // master enable
    apu.write_reg(0xFF24, 0x77); // max volume
    apu.write_reg(0xFF25, 0x22); // ch2 left+right
    apu.write_reg(0xFF16, 0); // length
    apu.write_reg(0xFF17, 0xF0); // envelope
    apu.write_reg(0xFF18, 0); // freq low
    apu.write_reg(0xFF19, 0x80); // trigger
    // step enough cycles for a few samples
    let mut div = 0u16;
    for _ in 0..10 {
        for _ in 0..(95 / 4) {
            tick_machine(&mut apu, &mut div, 4);
        }
    }
    assert!(consumer.pop_stereo().is_some());
}

#[test]
fn read_mask_unused_bits() {
    let mut apu = Apu::new();
    assert_eq!(apu.read_reg(0xFF11), 0xBF);
}

#[test]
fn register_write_read_fidelity() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF10, 0x07);
    apu.write_reg(0xFF11, 0xA2);
    assert_eq!(apu.read_reg(0xFF10), 0x87);
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
fn pcm_register_open_bus() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x00); // power off
    assert_eq!(apu.read_pcm(0xFF76), 0xFF);
    assert_eq!(apu.read_pcm(0xFF77), 0xFF);
}

#[test]
fn pcm_mmu_mapping() {
    let mut mmu = Mmu::new_with_mode(true);
    mmu.write_byte(0xFF26, 0x80);
    mmu.write_byte(0xFF11, 0x00); // duty 12.5%
    mmu.write_byte(0xFF12, 0xF0);
    mmu.write_byte(0xFF14, 0x80);
    mmu.write_byte(0xFF16, 0xC0);
    mmu.write_byte(0xFF17, 0xF0);
    mmu.write_byte(0xFF19, 0x80);
    let mut div = 0u16;
    for _ in 0..(8300 / 4) {
        tick_machine(&mut mmu.apu, &mut div, 4);
    }
    assert_eq!(mmu.read_byte(0xFF76), 0xF0);
    let mut dmg = Mmu::new();
    assert_eq!(dmg.read_byte(0xFF76), 0xFF);
}

#[test]
fn nr52_power_toggle() {
    let mut apu = Apu::new();
    // default power state should be on
    assert_eq!(apu.read_reg(0xFF26) & 0x80, 0x80);
    // power off
    apu.write_reg(0xFF26, 0x00);
    assert_eq!(apu.read_reg(0xFF26), 0x70);
    // power back on
    apu.write_reg(0xFF26, 0x80);
    assert_eq!(apu.read_reg(0xFF26), 0xF0);
    // writing channel bits should not change status
    apu.write_reg(0xFF26, 0x8F);
    assert_eq!(apu.read_reg(0xFF26), 0xF0);
}

#[test]
fn nr52_clears_registers_when_off() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // ensure enabled
    apu.write_reg(0xFF12, 0xF0);
    assert_eq!(apu.read_reg(0xFF12) & 0xF0, 0xF0);
    // power off clears registers
    apu.write_reg(0xFF26, 0x00);
    assert_eq!(apu.read_reg(0xFF12), 0x00);
    // writes ignored while off
    apu.write_reg(0xFF12, 0xF0);
    assert_eq!(apu.read_reg(0xFF12), 0x00);
    // power on again keeps cleared value
    apu.write_reg(0xFF26, 0x80);
    assert_eq!(apu.read_reg(0xFF12), 0x00);
}

#[test]
fn nr52_channel_status_bits() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x0F, 0x00);
    // trigger channel 1
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01);
    // trigger channel 2
    apu.write_reg(0xFF17, 0xF0);
    apu.write_reg(0xFF19, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x03, 0x03);
    // trigger channel 3
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1E, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x07, 0x07);
    // trigger channel 4
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x0F, 0x0F);
}

#[test]
fn nr52_bits_ignore_dac_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF17, 0x08); // enable DAC on channel 2 without trigger
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x00);
}

#[test]
fn nr52_wave_ram_persist() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF30, 0x12);
    apu.write_reg(0xFF26, 0x00);
    assert_eq!(apu.read_reg(0xFF30), 0x12);
    apu.write_reg(0xFF30, 0x34);
    apu.write_reg(0xFF26, 0x80);
    assert_eq!(apu.read_reg(0xFF30), 0x34);
}

fn run_ch2_sample(pan: u8) -> (i16, i16) {
    let mut apu = Apu::new();
    let consumer = apu.enable_output(44_100);
    apu.write_reg(0xFF26, 0x80); // enable
    apu.write_reg(0xFF24, 0x77); // max volume
    apu.write_reg(0xFF25, pan); // panning
    apu.write_reg(0xFF16, 0); // length
    apu.write_reg(0xFF17, 0xF0); // envelope
    apu.write_reg(0xFF18, 0); // freq low
    apu.write_reg(0xFF19, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..25 {
        tick_machine(&mut apu, &mut div, 4);
    }
    consumer.pop_stereo().unwrap()
}

fn run_ch2_sample_with_nr50(pan: u8, nr50: u8) -> (i16, i16) {
    let mut apu = Apu::new();
    let consumer = apu.enable_output(44_100);
    apu.write_reg(0xFF26, 0x80); // enable
    apu.write_reg(0xFF24, nr50); // master volume
    apu.write_reg(0xFF25, pan); // panning
    apu.write_reg(0xFF16, 0); // length
    apu.write_reg(0xFF17, 0xF0); // envelope
    apu.write_reg(0xFF18, 0); // freq low
    apu.write_reg(0xFF19, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..25 {
        tick_machine(&mut apu, &mut div, 4);
    }
    consumer.pop_stereo().unwrap()
}

#[test]
fn nr51_ch2_left_only() {
    let (left, right) = run_ch2_sample(0x20);
    assert_ne!(left, 0);
    assert_eq!(right, 0);
}

#[test]
fn nr51_ch2_right_only() {
    let (left, right) = run_ch2_sample(0x02);
    assert_eq!(left, 0);
    assert_ne!(right, 0);
}

#[test]
fn nr51_ch2_center() {
    let (left, right) = run_ch2_sample(0x22);
    assert_ne!(left, 0);
    assert_eq!(left, right);
}

#[test]
fn nr51_ch2_off() {
    let (left, right) = run_ch2_sample(0x00);
    assert_eq!(left, 0);
    assert_eq!(right, 0);
}

#[test]
fn nr50_volume_zero_not_muted() {
    let (left, right) = run_ch2_sample_with_nr50(0x22, 0x00);
    assert_ne!(left, 0);
    assert_ne!(right, 0);
}

#[test]
fn nr50_left_vs_right_volume() {
    let (left, right) = run_ch2_sample_with_nr50(0x22, 0x70);
    assert!(left.abs() > right.abs());
    assert_ne!(right, 0);
}

#[test]
fn nr50_vin_bits_ignored() {
    let (l1, r1) = run_ch2_sample_with_nr50(0x22, 0x77);
    let (l2, r2) = run_ch2_sample_with_nr50(0x22, 0xF7);
    assert_eq!(l1, l2);
    assert_eq!(r1, r2);
}

#[test]
fn nr11_write_sets_duty_and_length() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    let val = 0xCA; // duty 3, length 0x0A
    apu.write_reg(0xFF11, val);
    assert_eq!(apu.ch1_duty(), 3);
    assert_eq!(apu.ch1_length(), 64 - (val & 0x3F));
    assert_eq!(apu.read_reg(0xFF11), val | 0x3F);
}

#[test]
fn nr12_zero_turns_off_dac() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF12, 0xF0); // DAC on
    apu.write_reg(0xFF14, 0x80); // trigger
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01);
    apu.write_reg(0xFF12, 0x00); // writing zero should disable DAC
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x00);
}

#[test]
fn nr12_bit3_enables_dac() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0x08); // volume 0, envelope add -> DAC should be on
    apu.write_reg(0xFF14, 0x80); // trigger channel 1
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01);
}

#[test]
fn nr12_write_requires_retrigger() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF0); // initial volume 15
    apu.write_reg(0xFF14, 0x80); // trigger
    assert_eq!(apu.ch1_volume(), 0xF);
    // write new envelope while channel active
    apu.write_reg(0xFF12, 0x50); // initial volume 5
    // zombie mode should update volume immediately
    assert_eq!(apu.ch1_volume(), 0x0);
    apu.write_reg(0xFF14, 0x80); // retrigger
    assert_eq!(apu.ch1_volume(), 0x5);
}

#[test]
fn nr13_write_sets_frequency_low_bits_and_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF13, 0x34); // write low bits
    apu.write_reg(0xFF14, 0x82); // trigger at frequency 0x234
    assert_eq!(apu.ch1_frequency(), 0x234);
    assert_eq!(apu.read_reg(0xFF13), 0xFF); // NR13 is write-only
}

#[test]
fn nr13_period_change_delayed_until_sample_end() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF11, 0x00);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x85); // freq=0x500 trigger

    let mut div = 0u16;
    for _ in 0..10 {
        tick_machine(&mut apu, &mut div, 4);
    }
    let timer_before = apu.ch1_timer();

    apu.write_reg(0xFF13, 0x40); // update low bits; frequency becomes 0x540
    assert_eq!(apu.ch1_frequency(), 0x540);
    assert_eq!(apu.ch1_timer(), timer_before); // timer unchanged immediately

    for _ in 0..(timer_before + 10) {
        tick_machine(&mut apu, &mut div, 1);
    }
    let timer_after = apu.ch1_timer();
    let expected = (2048 - 0x540) * 4;
    assert!(timer_after <= expected + 16 && timer_after + 16 >= expected);
}

#[test]
fn retrigger_preserves_timer_low_bits_ch1() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..10 {
        tick_machine(&mut apu, &mut div, 1);
    }
    let low = apu.ch1_timer() & 3;
    apu.write_reg(0xFF14, 0x80); // retrigger
    assert_eq!(apu.ch1_timer() & 3, low);
}

#[test]
fn nr14_write_sets_frequency_high_bits_and_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF13, 0xAA); // low bits
    apu.write_reg(0xFF14, 0x05); // high bits = 5, no trigger
    assert_eq!(apu.ch1_frequency(), 0x5AA);
    // period bits and trigger bit are write only
    assert_eq!(apu.read_reg(0xFF14), 0xBF);
}

#[test]
fn nr14_length_enable_read_write() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF14, 0x40); // enable length counter
    assert_eq!(apu.read_reg(0xFF14), 0xFF);
    apu.write_reg(0xFF14, 0x00); // disable length counter
    assert_eq!(apu.read_reg(0xFF14), 0xBF);
}

#[test]
fn nr21_write_sets_duty_and_length() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    let val = 0xCA; // duty 3, length 0x0A
    apu.write_reg(0xFF16, val);
    assert_eq!(apu.ch2_duty(), 3);
    assert_eq!(apu.ch2_length(), 64 - (val & 0x3F));
    assert_eq!(apu.read_reg(0xFF16), val | 0x3F);
}

#[test]
fn nr22_zero_turns_off_dac() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF17, 0xF0);
    apu.write_reg(0xFF19, 0x80); // trigger
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x02);
    apu.write_reg(0xFF17, 0x00); // turn DAC off
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x00);
}

#[test]
fn nr22_bit3_enables_dac() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF17, 0x08); // volume 0, envelope add
    apu.write_reg(0xFF19, 0x80); // trigger channel 2
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x02);
}

#[test]
fn nr22_write_requires_retrigger() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF17, 0xF0); // initial volume 15
    apu.write_reg(0xFF19, 0x80); // trigger
    assert_eq!(apu.ch2_volume(), 0xF);
    apu.write_reg(0xFF17, 0x50); // new envelope while active
    assert_eq!(apu.ch2_volume(), 0x0);
    apu.write_reg(0xFF19, 0x80); // retrigger
    assert_eq!(apu.ch2_volume(), 0x5);
}

#[test]
fn nr23_write_sets_frequency_low_bits_and_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF18, 0x34);
    apu.write_reg(0xFF19, 0x82);
    assert_eq!(apu.ch2_frequency(), 0x234);
    assert_eq!(apu.read_reg(0xFF18), 0xFF);
}

#[test]
fn nr23_period_change_delayed_until_sample_end() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF16, 0x00);
    apu.write_reg(0xFF17, 0xF0);
    apu.write_reg(0xFF18, 0x00);
    apu.write_reg(0xFF19, 0x85); // freq=0x500 trigger

    let mut div = 0u16;
    for _ in 0..10 {
        tick_machine(&mut apu, &mut div, 4);
    }
    let timer_before = apu.ch2_timer();

    apu.write_reg(0xFF18, 0x40); // update low bits; frequency becomes 0x540
    assert_eq!(apu.ch2_frequency(), 0x540);
    assert_eq!(apu.ch2_timer(), timer_before);

    for _ in 0..(timer_before + 10) {
        tick_machine(&mut apu, &mut div, 1);
    }
    let timer_after = apu.ch2_timer();
    let expected = (2048 - 0x540) * 4;
    assert!(timer_after <= expected + 16 && timer_after + 16 >= expected);
}

#[test]
fn nr24_write_sets_frequency_high_bits_and_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF18, 0xAA);
    apu.write_reg(0xFF19, 0x05);
    assert_eq!(apu.ch2_frequency(), 0x5AA);
    assert_eq!(apu.read_reg(0xFF19), 0xBF);
}

#[test]
fn nr24_length_enable_read_write() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF19, 0x40); // enable length counter
    assert_eq!(apu.read_reg(0xFF19), 0xFF);
    apu.write_reg(0xFF19, 0x00); // disable length counter
    assert_eq!(apu.read_reg(0xFF19), 0xBF);
}

#[test]
fn wave_ram_accessible_with_dac_on_when_inactive() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF1A, 0x80); // DAC on but channel inactive
    apu.write_reg(0xFF30, 0xAB);
    assert_eq!(apu.read_reg(0xFF30), 0xAB);
    apu.write_reg(0xFF30, 0xCD);
    assert_eq!(apu.read_reg(0xFF30), 0xCD);
}

#[test]
fn wave_ram_locked_read_returns_latched_nibble_on_dmg() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    let first_byte = 0x9C; // high nibble 9, low nibble C
    apu.write_reg(0xFF30, first_byte);
    for addr in 0xFF31..=0xFF3F {
        apu.write_reg(addr, 0x00);
    }
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, 0x20); // full volume
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87); // trigger playback

    let mut div = 0u16;
    let mut latched = None;
    for _ in 0..256 {
        tick_machine(&mut apu, &mut div, 1);
        let value = apu.read_reg(0xFF30);
        if value != 0xFF {
            latched = Some(value);
            break;
        }
    }

    let value = latched.expect("expected latched wave sample on DMG-compatible hardware");
    assert_eq!(
        value & 0x0F,
        value >> 4,
        "locked read should return repeated nibble"
    );
    let nibble = value & 0x0F;
    assert!(
        nibble == (first_byte >> 4) || nibble == (first_byte & 0x0F),
        "latched nibble should match one of the waveform nibbles"
    );
}

#[test]
fn wave_ram_locked_read_redirects_on_cgb_e() {
    // CGB (all revisions through E) redirects wave RAM reads while CH3 is
    // active to the byte at the current playback position, regardless of
    // which wave RAM address was requested.
    let mut apu = Apu::new_with_config(true, CgbRevision::RevE);
    apu.write_reg(0xFF26, 0x80); // enable APU
    // Write a recognisable pattern into wave RAM so that different positions
    // produce different values and we can verify redirection.
    for i in 0u8..16 {
        apu.write_reg(0xFF30 + i as u16, (i << 4) | i);
    }
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87);

    let mut div = 0u16;
    // Run for a while and verify that reading two different wave RAM
    // addresses in the same cycle returns the same value (redirect).
    let mut saw_redirect = false;
    for _ in 0..256 {
        tick_machine(&mut apu, &mut div, 1);
        let val_a = apu.read_reg(0xFF30); // address 0
        let val_b = apu.read_reg(0xFF35); // address 5
        // Both should return the byte at the current position (same byte)
        assert_eq!(
            val_a, val_b,
            "CGB-E should redirect both reads to the same byte"
        );
        if val_a != 0x00 {
            saw_redirect = true;
        }
    }
    assert!(saw_redirect, "should have seen non-zero redirected values");
}

#[test]
fn wave_ram_locked_write_commits_after_byte_advance() {
    let mut apu = Apu::new_with_config(true, CgbRevision::RevC);
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF30, 0x21);
    for addr in 0xFF31..=0xFF3F {
        apu.write_reg(addr, 0x00);
    }
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, 0x20); // full volume
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87); // trigger playback

    let mut div = 0u16;
    for _ in 0..16 {
        tick_machine(&mut apu, &mut div, 1);
    }

    apu.write_reg(0xFF30, 0xF0); // locked write while channel active
    let mask = apu.wave_pending_mask();
    assert_ne!(mask, 0, "locked write should stage pending data");
    let target = mask.trailing_zeros() as usize;
    assert_eq!(apu.wave_shadow_byte(target), 0xF0);

    for _ in 0..64 {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.wave_pending_mask(), 0);

    apu.write_reg(0xFF1A, 0x00); // disable DAC to unlock wave RAM
    assert_eq!(apu.read_reg(0xFF30 + target as u16), 0xF0);
}

#[test]
fn nr30_dac_off_disables_channel() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF1A, 0x80); // enable DAC
    apu.write_reg(0xFF1E, 0x80); // trigger channel 3
    assert_eq!(apu.read_reg(0xFF26) & 0x04, 0x04);
    apu.write_reg(0xFF1A, 0x00); // disable DAC
    assert_eq!(apu.read_reg(0xFF26) & 0x04, 0x00);
}

#[test]
fn nr31_write_sets_length() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // ensure enabled
    apu.write_reg(0xFF1B, 0x20);
    assert_eq!(apu.ch3_length(), 256 - 0x20);
}

#[test]
fn nr31_write_updates_length_when_disabled_on_dmg() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x00); // disable APU
    apu.write_reg(0xFF1B, 0x40);
    // On DMG, NR31 length writes are allowed even when the APU is off
    assert_eq!(apu.ch3_length(), 256 - 0x40);
}

#[test]
fn nr33_write_sets_frequency_low_bits_and_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF1D, 0x34); // write low bits
    apu.write_reg(0xFF1E, 0x05); // high bits=5, no trigger
    assert_eq!(apu.ch3_frequency(), 0x534);
    assert_eq!(apu.read_reg(0xFF1D), 0xFF); // NR33 is write-only
}

#[test]
fn nr34_write_sets_frequency_high_bits_and_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF1D, 0xAA);
    apu.write_reg(0xFF1E, 0x05); // high bits=5
    assert_eq!(apu.ch3_frequency(), 0x5AA);
    assert_eq!(apu.read_reg(0xFF1E), 0xBF);
}

#[test]
fn nr34_length_enable_read_write() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF1E, 0x40); // enable length counter
    assert_eq!(apu.read_reg(0xFF1E), 0xFF);
    apu.write_reg(0xFF1E, 0x00); // disable length counter
    assert_eq!(apu.read_reg(0xFF1E), 0xBF);
}

#[test]
fn nr34_retrigger_resets_wave_position() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    for i in 0..0x10 {
        apu.write_reg(0xFF30 + i as u16, (i * 0x11) as u8);
    }
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, 0x20); // full volume
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87); // trigger
    let mut div = 0u16;
    tick_machine(&mut apu, &mut div, 4); // discard initial old sample
    let mut first = [0u8; 8];
    for s in &mut first {
        tick_machine(&mut apu, &mut div, 4);
        *s = apu.read_pcm(0xFF77) & 0x0F;
    }

    for _ in 0..8 {
        tick_machine(&mut apu, &mut div, 4);
    }

    apu.write_reg(0xFF1E, 0x87); // retrigger
    tick_machine(&mut apu, &mut div, 4); // discard old sample
    let mut second = [0u8; 8];
    for s in &mut second {
        tick_machine(&mut apu, &mut div, 4);
        *s = apu.read_pcm(0xFF77) & 0x0F;
    }
    assert_eq!(second, first);
}

#[test]
fn wave_retrigger_emits_last_sample() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    for addr in 0xFF30..=0xFF3F {
        apu.write_reg(addr, 0x11); // sample value 0x1 in all nibbles
    }
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, 0x20); // full volume
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87); // trigger
    let mut div = 0u16;
    tick_machine(&mut apu, &mut div, 4); // output 0
    tick_machine(&mut apu, &mut div, 4); // output 1
    let last = apu.read_pcm(0xFF77) & 0x0F;
    apu.write_reg(0xFF1E, 0x87); // retrigger
    tick_machine(&mut apu, &mut div, 4); // first sample after retrigger
    let first = apu.read_pcm(0xFF77) & 0x0F;
    assert_eq!(first, last);
}

#[test]
fn nr41_write_sets_length() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF20, 0x20);
    assert_eq!(apu.ch4_length(), 64 - (0x20 & 0x3F));
    assert_eq!(apu.read_reg(0xFF20), 0xFF);
}

#[test]
fn nr41_zero_sets_full_length() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF20, 0x00);
    assert_eq!(apu.ch4_length(), 64);
}

#[test]
fn nr41_high_bits_ignored() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF20, 0xFF);
    assert_eq!(apu.ch4_length(), 64 - 0x3F);
    assert_eq!(apu.read_reg(0xFF20), 0xFF);
}

#[test]
fn nr42_zero_turns_off_dac() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x08);
    apu.write_reg(0xFF21, 0x00);
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x00);
}

#[test]
fn nr42_bit3_enables_dac() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0x08); // volume 0, envelope add
    apu.write_reg(0xFF23, 0x80); // trigger noise
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x08);
}

#[test]
fn nr42_write_requires_retrigger() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.ch4_volume(), 0xF);
    apu.write_reg(0xFF21, 0x50);
    assert_eq!(apu.ch4_volume(), 0x0);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.ch4_volume(), 0x5);
}

#[test]
fn nr42_writes_ignored_when_disabled() {
    let mut apu = Apu::new();
    // disable the entire APU
    apu.write_reg(0xFF26, 0x00);
    // attempt to set envelope params while powered off
    apu.write_reg(0xFF21, 0xF0);
    // read value should remain the default power-on value
    assert_eq!(apu.read_reg(0xFF21), 0x00);
    // enable the APU and write again
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    assert_eq!(apu.read_reg(0xFF21) & 0xF0, 0xF0);
}

#[test]
fn nr43_lfsr_first_step() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..8 {
        tick_machine(&mut apu, &mut div, 1);
    }
    assert_eq!(apu.ch4_lfsr(), 0x4000);
}

#[test]
fn nr43_bit15_copies_to_bit7_in_short_mode() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x08); // short mode
    apu.write_reg(0xFF23, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..8 {
        tick_machine(&mut apu, &mut div, 1);
    }
    let lfsr = apu.ch4_lfsr();
    assert_eq!((lfsr >> 14) & 1, (lfsr >> 6) & 1);
}

#[test]
fn nr44_length_enable_read_write() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF23, 0x40);
    assert_eq!(apu.read_reg(0xFF23), 0xFF);
    apu.write_reg(0xFF23, 0x00);
    assert_eq!(apu.read_reg(0xFF23), 0xBF);
}

#[test]
fn nr12_period_zero_sets_timer_to_8() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF0); // period 0
    apu.write_reg(0xFF14, 0x80); // trigger
    assert_eq!(apu.ch1_envelope_timer(), 8);
}

#[test]
fn nr22_period_zero_sets_timer_to_8() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF17, 0xF0); // period 0
    apu.write_reg(0xFF19, 0x80); // trigger
    assert_eq!(apu.ch2_envelope_timer(), 8);
}

#[test]
fn nr43_output_depends_on_lfsr() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0); // volume 15
    apu.write_reg(0xFF22, 0x00); // period 8
    apu.write_reg(0xFF23, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..8 {
        tick_machine(&mut apu, &mut div, 1);
    }
    let lfsr1 = apu.ch4_lfsr();
    let sample1 = if lfsr1 & 1 == 0 { apu.ch4_volume() } else { 0 };
    assert_eq!(sample1, 0xF);
    for _ in 0..112 {
        tick_machine(&mut apu, &mut div, 1);
    }
    let lfsr2 = apu.ch4_lfsr();
    let sample2 = if lfsr2 & 1 == 0 { apu.ch4_volume() } else { 0 };
    assert_eq!(sample2, 0);
}

#[test]
fn duty_step_reset_when_apu_powered_off() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80);

    let mut div = 0u16;
    let first_timer = apu.ch1_timer();
    for _ in 0..first_timer {
        tick_machine(&mut apu, &mut div, 1);
    }
    assert_eq!(apu.ch1_duty_pos(), 0);

    apu.write_reg(0xFF26, 0x00); // power off
    apu.write_reg(0xFF26, 0x80); // power on
    assert_eq!(apu.ch1_duty_pos(), 0);
}

#[test]
fn pcm_mask_glitch_releases_after_first_sample() {
    let mut apu = Apu::new_with_config(true, CgbRevision::RevC);
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF24, 0x77);
    apu.write_reg(0xFF25, 0x11);
    apu.write_reg(0xFF11, 0x80);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0xFF);
    apu.write_reg(0xFF14, 0x87);
    assert_eq!(apu.pcm_mask()[0] & 0x0F, 0x00);
    let mut div = 0u16;
    let mut mask_released = false;
    for _ in 0..4096 {
        tick_machine(&mut apu, &mut div, 4);
        if apu.pcm_samples()[0] > 0 && apu.pcm_mask()[0] & 0x0F == 0x0F {
            mask_released = true;
            break;
        }
    }
    assert!(
        mask_released,
        "expected PCM mask to release after activation"
    );
}

#[test]
fn double_speed_preserves_lf_div_phase() {
    let mut normal = Apu::new_with_config(true, CgbRevision::RevE);
    let mut div = 0u16;
    for _ in 0..64 {
        let prev = div;
        div = div.wrapping_add(4);
        normal.tick(prev, div, false);
        normal.step(4);
    }

    let mut fast = Apu::new_with_config(true, CgbRevision::RevE);
    let mut div_fast = 0u16;
    for _ in 0..64 {
        let prev = div_fast;
        div_fast = div_fast.wrapping_add(2);
        fast.tick(prev, div_fast, true);
        fast.step(2);
    }

    assert_eq!(normal.lf_div_phase(), fast.lf_div_phase());
}

#[test]
fn pcm_mask_defaults_to_full_on_reve() {
    let mut apu = Apu::new_with_config(true, CgbRevision::RevE);
    assert_eq!(apu.pcm_mask()[0], 0xFF);
    assert_eq!(apu.pcm_mask()[1], 0xFF);
}
