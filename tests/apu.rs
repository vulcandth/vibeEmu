use vibeEmu::apu::Apu;
use vibeEmu::mmu::Mmu;

fn tick_machine(apu: &mut Apu, div: &mut u16, cycles: u16) {
    let prev = *div;
    *div = div.wrapping_add(cycles);
    apu.tick(prev, *div, false);
    apu.step(cycles);
}

#[test]
#[ignore]
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
#[ignore]
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
    let mut div = 0u16;
    for _ in 0..10 {
        for _ in 0..(95 / 4) {
            tick_machine(&mut apu, &mut div, 4);
        }
    }
    assert!(apu.pop_sample().is_some());
}
#[test]
#[ignore]
fn writes_ignored_when_disabled() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x00); // disable
    apu.write_reg(0xFF12, 0xF0);
    assert_eq!(apu.read_reg(0xFF12), 0xF0);
    apu.write_reg(0xFF26, 0x80); // enable
    apu.write_reg(0xFF12, 0xF0);
    assert_eq!(apu.read_reg(0xFF12) & 0xF0, 0xF0);
}

#[test]
#[ignore]
fn read_mask_unused_bits() {
    let apu = Apu::new();
    assert_eq!(apu.read_reg(0xFF11), 0xBF);
}

#[test]
#[ignore]
fn register_write_read_fidelity() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF10, 0x07);
    apu.write_reg(0xFF11, 0xA2);
    assert_eq!(apu.read_reg(0xFF10), 0x87);
    assert_eq!(apu.read_reg(0xFF11), 0xBF);
}

#[test]
#[ignore]
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
#[ignore]
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
#[ignore]
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
    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    } // step 1
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    } // step 2
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    } // step 3 (sweep clocked on previous step)
    assert_eq!(apu.ch1_frequency(), 0x6C0);
}

#[test]
fn sweep_disabled_when_period_zero() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable
    apu.write_reg(0xFF10, 0x11); // period=1, shift=1
    apu.write_reg(0xFF12, 0xF0); // DAC on
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x82); // trigger with freq=0x200
    assert_eq!(apu.ch1_frequency(), 0x300);
    // disable sweep by setting period to 0
    apu.write_reg(0xFF10, 0x01);
    let mut div = 0u16;
    for _ in 0..64 {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch1_frequency(), 0x300);
}

#[test]
fn sweep_subtraction_mode() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x19); // period=1, negate, shift=1
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x82); // freq=0x200, trigger
    assert_eq!(apu.ch1_frequency(), 0x100);
    let mut div = 0u16;
    for _ in 0..(8192 * 3 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch1_frequency(), 0x80);
}

#[test]
fn sweep_overflow_with_period_zero_disables_channel() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x01); // period=0, shift=1 (addition)
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0xF8); // freq high to overflow
    apu.write_reg(0xFF14, 0x87); // high bits=7, trigger
    // overflow should disable channel immediately
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x00);
}

#[test]
fn sweep_updates_frequency_registers() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x11); // period=1, shift=1
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x82); // trigger
    let mut div = 0u16;
    for _ in 0..(8192 * 3 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch1_frequency(), 0x480);
}

#[test]
fn sweep_trigger_sets_shadow_and_timer() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable
    apu.write_reg(0xFF10, 0x30); // period=3, shift=0
    apu.write_reg(0xFF12, 0xF0); // DAC on
    apu.write_reg(0xFF13, 0x56);
    apu.write_reg(0xFF14, 0x80); // trigger
    assert_eq!(apu.ch1_sweep_shadow(), 0x056);
    assert_eq!(apu.ch1_sweep_timer(), 3);
    assert!(apu.ch1_sweep_enabled());
}

#[test]
fn sweep_disabled_with_zero_params() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x00); // period=0, shift=0
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x80); // trigger
    assert_eq!(apu.ch1_sweep_timer(), 8);
    assert!(!apu.ch1_sweep_enabled());
}

#[test]
fn sweep_frequency_write_lost() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x11); // period=1, shift=1
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x82); // trigger -> freq becomes 0x300
    assert_eq!(apu.ch1_sweep_shadow(), 0x300);

    // modify frequency while sweep active
    apu.write_reg(0xFF13, 0x00); // low bits
    apu.write_reg(0xFF14, 0x01); // high bits=1
    assert_eq!(apu.ch1_frequency(), 0x100);
    // shadow should remain unchanged
    assert_eq!(apu.ch1_sweep_shadow(), 0x300);

    let mut div = 0u16;
    for _ in 0..(8192 * 3 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    // frequency should be updated from shadow register, not current value
    assert_eq!(apu.ch1_frequency(), 0x480);
}

#[test]
fn pcm_register_open_bus() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x00); // power off
    assert_eq!(apu.read_pcm(0xFF76), 0xFF);
    assert_eq!(apu.read_pcm(0xFF77), 0xFF);
}

#[test]
fn pcm_register_sample_values() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable
    // ch1 low, ch2 high so PCM12 should be 0xC0
    apu.write_reg(0xFF11, 0x00); // duty 12.5%
    apu.write_reg(0xFF12, 0xF0); // max volume
    apu.write_reg(0xFF14, 0x80); // trigger

    apu.write_reg(0xFF16, 0xC0); // duty 75%
    apu.write_reg(0xFF17, 0xF0);
    apu.write_reg(0xFF19, 0x80); // trigger

    let mut div = 0u16;
    for _ in 0..(8300 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }

    assert_eq!(apu.read_pcm(0xFF76), 0xF0);
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
    {
        let mut apu = mmu.apu.lock().unwrap();
        let mut div = 0u16;
        for _ in 0..(8300 / 4) {
            tick_machine(&mut apu, &mut div, 4);
        }
    }
    assert_eq!(mmu.read_byte(0xFF76), 0xF0);
    let mut dmg = Mmu::new();
    assert_eq!(dmg.read_byte(0xFF76), 0xFF);
}

#[test]
fn pcm34_noise_output() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    // trigger noise channel with volume 15
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80);

    let mut div = 0u16;
    for _ in 0..32 {
        tick_machine(&mut apu, &mut div, 4);
    }

    assert_eq!(apu.read_pcm(0xFF77), 0x00);
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
    let left = apu.pop_sample().unwrap();
    let right = apu.pop_sample().unwrap();
    (left, right)
}

fn run_ch2_sample_with_nr50(pan: u8, nr50: u8) -> (i16, i16) {
    let mut apu = Apu::new();
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
    let left = apu.pop_sample().unwrap();
    let right = apu.pop_sample().unwrap();
    (left, right)
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
fn nr11_length_counter_expires() {
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
    // volume should remain unchanged until retrigger
    assert_eq!(apu.ch1_volume(), 0xF);
    apu.write_reg(0xFF14, 0x80); // retrigger
    assert_eq!(apu.ch1_volume(), 0x5);
}

#[test]
fn nr12_register_unchanged_after_envelope() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF11, 0x00);
    apu.write_reg(0xFF12, 0x8A); // init 8, increase, pace=2
    apu.write_reg(0xFF14, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..(196608 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF12), 0x8A);
    assert_ne!(apu.ch1_volume(), 8);
}

#[test]
fn envelope_zero_does_not_disable_channel() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF11, 0x00);
    apu.write_reg(0xFF12, 0x11); // volume 1, decrease, period=1
    apu.write_reg(0xFF14, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..(8192 * 16 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch1_volume(), 0);
    // Channel should still be on
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01);
    // Register should retain original value
    assert_eq!(apu.read_reg(0xFF12), 0x11);
}

#[test]
fn nr13_write_sets_frequency_low_bits_and_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF13, 0x34); // write low bits
    apu.write_reg(0xFF14, 0x82); // set high bits=2 and trigger
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

    apu.write_reg(0xFF13, 0x40); // set new low bits -> freq 0x540
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
    apu.write_reg(0xFF14, 0x40); // set length enable
    assert_eq!(apu.read_reg(0xFF14), 0xFF);
    apu.write_reg(0xFF14, 0x00); // clear length enable
    assert_eq!(apu.read_reg(0xFF14), 0xBF);
}

#[test]
fn nr14_trigger_resets_length_and_volume() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF11, 0x3F); // length = 1
    apu.write_reg(0xFF12, 0xF0); // volume 15
    apu.write_reg(0xFF14, 0x80); // trigger with length disabled

    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }

    apu.write_reg(0xFF12, 0x50); // new envelope params while active
    apu.write_reg(0xFF14, 0x40); // enable length
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x00); // channel disabled

    apu.write_reg(0xFF14, 0x80); // retrigger
    assert_eq!(apu.read_reg(0xFF26) & 0x01, 0x01); // channel enabled
    assert_eq!(apu.ch1_length(), 64); // length reloaded
    assert_eq!(apu.ch1_volume(), 0x5); // envelope reset
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
fn nr21_length_counter_expires() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF16, 0x3F); // length = 1
    apu.write_reg(0xFF17, 0xF0); // DAC on
    apu.write_reg(0xFF19, 0x80); // trigger, length disabled
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x02);

    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }

    apu.write_reg(0xFF19, 0x40); // enable length
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x00);
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
    assert_eq!(apu.ch2_volume(), 0xF);
    apu.write_reg(0xFF19, 0x80); // retrigger
    assert_eq!(apu.ch2_volume(), 0x5);
}

#[test]
fn nr22_register_unchanged_after_envelope() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF16, 0x00);
    apu.write_reg(0xFF17, 0x8A); // init 8, increase, pace=2
    apu.write_reg(0xFF19, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..(196608 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF17), 0x8A);
    assert_ne!(apu.ch2_volume(), 8);
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

    apu.write_reg(0xFF18, 0x40); // new low bits -> freq 0x540
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
    apu.write_reg(0xFF19, 0x40); // set length enable
    assert_eq!(apu.read_reg(0xFF19), 0xFF);
    apu.write_reg(0xFF19, 0x00); // clear length enable
    assert_eq!(apu.read_reg(0xFF19), 0xBF);
}

#[test]
fn nr24_trigger_resets_length_and_volume() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF16, 0x3F); // length = 1
    apu.write_reg(0xFF17, 0xF0); // volume 15
    apu.write_reg(0xFF19, 0x80); // trigger with length disabled

    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }

    apu.write_reg(0xFF17, 0x50); // new envelope params
    apu.write_reg(0xFF19, 0x40); // enable length
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x00);

    apu.write_reg(0xFF19, 0x80); // retrigger
    assert_eq!(apu.read_reg(0xFF26) & 0x02, 0x02);
    assert_eq!(apu.ch2_length(), 64);
    assert_eq!(apu.ch2_volume(), 0x5);
}

#[test]
fn wave_channel_outputs_wave_ram_data() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    for i in 0..0x10 {
        apu.write_reg(0xFF30 + i as u16, (i * 0x11) as u8);
    }
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, 0x20); // full volume
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87); // trigger with freq 2047
    let mut div = 0u16;
    let mut samples = [0u8; 8];
    for sample in &mut samples {
        tick_machine(&mut apu, &mut div, 4);
        *sample = apu.read_pcm(0xFF77) & 0x0F;
    }
    assert_eq!(samples, [0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn wave_channel_wraps_after_32_samples() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    for i in 0..0x10 {
        apu.write_reg(0xFF30 + i as u16, (i * 0x11) as u8);
    }
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87);
    let mut div = 0u16;
    for _ in 0..32 {
        tick_machine(&mut apu, &mut div, 4);
    }
    tick_machine(&mut apu, &mut div, 4);
    let sample = apu.read_pcm(0xFF77) & 0x0F;
    assert_eq!(sample, 0);
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
fn wave_channel_starts_at_index_one() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1D, 0xFF); // freq low
    apu.write_reg(0xFF1E, 0x87); // trigger with high freq (period = 2 cycles)
    assert_eq!(apu.ch3_position(), 0);
    let mut div = 0u16;
    tick_machine(&mut apu, &mut div, 2); // advance one sample
    assert_eq!(apu.ch3_position(), 1);
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
fn nr31_write_ignored_when_disabled() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x00); // disable APU
    let before = apu.ch3_length();
    apu.write_reg(0xFF1B, 0x40);
    assert_eq!(apu.ch3_length(), before);
}

#[test]
fn nr31_length_counter_expires() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1B, 0xFF); // length = 1
    apu.write_reg(0xFF1E, 0x80); // trigger, length disabled
    assert_eq!(apu.read_reg(0xFF26) & 0x04, 0x04);

    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }

    apu.write_reg(0xFF1E, 0x40); // enable length
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x04, 0x00);
}

fn run_ch3_sample(nr32: u8) -> u8 {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    for addr in 0xFF30..=0xFF3F {
        apu.write_reg(addr, 0xCC); // sample data
    }
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, nr32); // volume
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87); // trigger
    let mut div = 0u16;
    tick_machine(&mut apu, &mut div, 4);
    tick_machine(&mut apu, &mut div, 4);
    apu.read_pcm(0xFF77) & 0x0F
}

#[test]
fn nr32_volume_control() {
    let mute = run_ch3_sample(0x00);
    let full = run_ch3_sample(0x20);
    let half = run_ch3_sample(0x40);
    let quarter = run_ch3_sample(0x60);
    assert_eq!(mute, 0);
    assert_eq!(full, 0xC);
    assert_eq!(half, full >> 1);
    assert_eq!(quarter, full >> 2);
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
fn nr33_period_change_delayed_until_sample_end() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1D, 0x00);
    apu.write_reg(0xFF1E, 0x85); // freq=0x500 trigger

    let mut div = 0u16;
    for _ in 0..10 {
        tick_machine(&mut apu, &mut div, 4);
    }
    let timer_before = apu.ch3_timer();

    apu.write_reg(0xFF1D, 0x40); // new low bits -> freq 0x540
    assert_eq!(apu.ch3_frequency(), 0x540);
    assert_eq!(apu.ch3_timer(), timer_before);

    for _ in 0..(timer_before + 10) {
        tick_machine(&mut apu, &mut div, 1);
    }
    let timer_after = apu.ch3_timer();
    let expected = (2048 - 0x540) * 2;
    eprintln!("timer_after={} expected={}", timer_after, expected);
    assert!(timer_after <= expected + 16 && timer_after + 16 >= expected);
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
    apu.write_reg(0xFF1E, 0x40); // set length enable
    assert_eq!(apu.read_reg(0xFF1E), 0xFF);
    apu.write_reg(0xFF1E, 0x00); // clear length enable
    assert_eq!(apu.read_reg(0xFF1E), 0xBF);
}

#[test]
fn nr34_trigger_resets_length() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1B, 0xFF); // length = 1
    apu.write_reg(0xFF1E, 0x80); // trigger with length disabled

    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }

    apu.write_reg(0xFF1E, 0x40); // enable length
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x04, 0x00); // channel disabled

    apu.write_reg(0xFF1E, 0x80); // retrigger
    assert_eq!(apu.read_reg(0xFF26) & 0x04, 0x04);
    assert_eq!(apu.ch3_length(), 256);
}
#[test]
fn nr34_trigger_reload_timer_and_freq() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1D, 0xAA); // low bits
    apu.write_reg(0xFF1E, 0x85); // high bits=5, trigger
    let expected = (2048 - 0x5AA) * 2;
    assert_eq!(apu.ch3_frequency(), 0x5AA);
    assert_eq!(apu.ch3_timer(), expected);

    apu.write_reg(0xFF1D, 0x00); // new low bits
    apu.write_reg(0xFF1E, 0x80); // high bits=0, trigger
    let expected2 = 2048 * 2;
    assert_eq!(apu.ch3_frequency(), 0x000);
    assert_eq!(apu.ch3_timer(), expected2);
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
fn wave_buffer_cleared_on_power_on() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    for addr in 0xFF30..=0xFF3F {
        apu.write_reg(addr, 0xCC);
    }
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87);
    let mut div = 0u16;
    tick_machine(&mut apu, &mut div, 4);
    tick_machine(&mut apu, &mut div, 4);
    assert_eq!(apu.read_pcm(0xFF77) & 0x0F, 0xC);

    apu.write_reg(0xFF26, 0x00); // power off
    apu.write_reg(0xFF26, 0x80); // power on
    apu.write_reg(0xFF1A, 0x80); // DAC on
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87);
    div = 0;
    tick_machine(&mut apu, &mut div, 4);
    let sample = apu.read_pcm(0xFF77) & 0x0F;
    assert_eq!(sample, 0);
}

#[test]
fn wave_sample_index_matches_frequency() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0x00); // low bits
    apu.write_reg(0xFF1E, 0x84); // freq high bits=4, trigger (freq=0x400)
    assert_eq!(apu.ch3_position(), 0);
    let mut div = 0u16;
    for _ in 0..512 {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch3_position(), 1);
}

#[test]
fn nr32_volume_change_mid_playback() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    for addr in 0xFF30..=0xFF3F {
        apu.write_reg(addr, 0xCC);
    }
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20); // full volume
    apu.write_reg(0xFF1D, 0xFF);
    apu.write_reg(0xFF1E, 0x87); // trigger
    let mut div = 0u16;
    tick_machine(&mut apu, &mut div, 4); // 0
    tick_machine(&mut apu, &mut div, 4); // sample 0xC
    let full = apu.read_pcm(0xFF77) & 0x0F;
    apu.write_reg(0xFF1C, 0x40); // half volume
    tick_machine(&mut apu, &mut div, 4);
    let shifted = apu.read_pcm(0xFF77) & 0x0F;
    assert_eq!(shifted, full >> 1);
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
fn nr41_write_ignored_when_disabled() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x00);
    let before = apu.ch4_length();
    apu.write_reg(0xFF20, 0x40);
    assert_eq!(apu.ch4_length(), before);
}

#[test]
fn nr41_length_counter_expires() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF20, 0x3F);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x08);
    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    apu.write_reg(0xFF23, 0x40);
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x00);
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
    assert_eq!(apu.ch4_volume(), 0xF);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.ch4_volume(), 0x5);
}

#[test]
fn nr42_register_unchanged_after_envelope() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF20, 0x00);
    apu.write_reg(0xFF21, 0x8A);
    apu.write_reg(0xFF23, 0x80);
    let mut div = 0u16;
    for _ in 0..(196608 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF21), 0x8A);
    assert_ne!(apu.ch4_volume(), 8);
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
fn nr43_period_calculation() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.ch4_timer(), 8);
    apu.write_reg(0xFF22, 0x31);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.ch4_timer(), 128);
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
fn nr43_width7_mode() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80);
    let mut div = 0u16;
    for _ in 0..8 {
        tick_machine(&mut apu, &mut div, 1);
    }
    let lfsr15 = apu.ch4_lfsr();
    assert_eq!(lfsr15, 0x4000);
    apu.write_reg(0xFF22, 0x08);
    apu.write_reg(0xFF23, 0x80);
    div = 0;
    for _ in 0..8 {
        tick_machine(&mut apu, &mut div, 1);
    }
    let lfsr7 = apu.ch4_lfsr();
    assert_eq!(lfsr7, 0x4040);
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
fn nr44_trigger_resets_length_and_volume() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF20, 0x3F);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF23, 0x80);
    let mut div = 0u16;
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    apu.write_reg(0xFF21, 0x50);
    apu.write_reg(0xFF23, 0x40);
    for _ in 0..(2 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x00);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x08);
    assert_eq!(apu.ch4_length(), 64);
    assert_eq!(apu.ch4_volume(), 0x5);
}

#[test]
fn nr44_trigger_resets_lfsr_and_envelope_timer() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0); // initial volume 15, period 0 => timer 8
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80); // trigger
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x08);
    assert_eq!(apu.ch4_lfsr(), 0x0000);
    assert_eq!(apu.ch4_envelope_timer(), 8);

    let mut div = 0u16;
    for _ in 0..(16 * 8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_ne!(apu.ch4_lfsr(), 0x0000);
    assert!(apu.ch4_envelope_timer() < 8);

    apu.write_reg(0xFF23, 0x80); // retrigger
    assert_eq!(apu.ch4_lfsr(), 0x0000);
    assert_eq!(apu.ch4_envelope_timer(), 8);
    assert_eq!(apu.ch4_volume(), 0xF);
}
#[test]
fn nr43_register_fields() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF22, 0x5F); // shift=5, width7=1, divisor=7
    assert_eq!(apu.read_reg(0xFF22), 0x5F);
    assert_eq!(apu.ch4_clock_shift(), 5);
    assert!(apu.ch4_width7());
    assert_eq!(apu.ch4_divisor(), 7);
}

#[test]
fn nr43_lfsr_lockup_and_retrigger() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00); // 15-bit mode
    apu.write_reg(0xFF23, 0x80); // trigger
    let mut div = 0u16;
    for _ in 0..512 {
        tick_machine(&mut apu, &mut div, 1);
    }
    assert_eq!(apu.ch4_lfsr() & 0x7F, 0x7F);
    apu.write_reg(0xFF22, 0x08); // switch to width7
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x00);
    apu.write_reg(0xFF23, 0x80); // retrigger
    assert_eq!(apu.read_reg(0xFF26) & 0x08, 0x08);
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
fn div_apu_envelope_clock() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80); // enable APU
    apu.write_reg(0xFF16, 0x00); // length 64
    apu.write_reg(0xFF17, 0xF1); // initial vol 15, decrease, period=1
    apu.write_reg(0xFF19, 0x80); // trigger channel 2
    let mut div = 0u16;
    let initial = apu.ch2_volume();
    for _ in 0..(8192 * 16 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch2_volume(), initial - 1);
}

#[test]
fn div_apu_length_clock() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF16, 0x3E); // length=2
    apu.write_reg(0xFF17, 0xF0); // DAC on
    apu.write_reg(0xFF19, 0xC0); // length enable + trigger
    let mut div = 0u16;
    let initial = apu.ch2_length();
    for _ in 0..(8192 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch2_length(), initial - 1);
}

#[test]
fn div_apu_sweep_clock() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x11); // period=1, shift=1
    apu.write_reg(0xFF12, 0xF0); // DAC on
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x82); // trigger freq=0x200
    assert_eq!(apu.ch1_frequency(), 0x300);
    let mut div = 0u16;
    for _ in 0..(8192 * 3 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch1_frequency(), 0x480);
    for _ in 0..(8192 * 4 / 4) {
        tick_machine(&mut apu, &mut div, 4);
    }
    assert_eq!(apu.ch1_frequency(), 0x6C0);
}

#[test]
fn duty_step_advances_each_period() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF0); // DAC on
    apu.write_reg(0xFF13, 0xFF); // freq low
    apu.write_reg(0xFF14, 0x87); // high bits=7, trigger

    let mut div = 0u16;
    let first_timer = apu.ch1_timer();
    for _ in 0..first_timer {
        tick_machine(&mut apu, &mut div, 1);
    }
    assert_eq!(apu.ch1_duty_pos(), 0);

    let period = (2048 - apu.ch1_frequency()) * 4;
    for _ in 0..period {
        tick_machine(&mut apu, &mut div, 1);
    }
    assert_eq!(apu.ch1_duty_pos(), 1);
}

#[test]
fn duty_step_not_reset_on_retrigger() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0xFF);
    apu.write_reg(0xFF14, 0x87);

    let mut div = 0u16;
    let first_timer = apu.ch1_timer();
    for _ in 0..first_timer {
        tick_machine(&mut apu, &mut div, 1);
    }
    let period = (2048 - apu.ch1_frequency()) * 4;
    for _ in 0..period {
        tick_machine(&mut apu, &mut div, 1);
    }
    assert_eq!(apu.ch1_duty_pos(), 1);

    apu.write_reg(0xFF14, 0x80); // retrigger
    assert_eq!(apu.ch1_duty_pos(), 1);
    assert!(apu.ch1_timer() > period as i32);
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
fn first_sample_after_trigger_is_zero() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80);

    let mut div = 0u16;
    tick_machine(&mut apu, &mut div, 1);
    assert_eq!(apu.read_pcm(0xFF76) & 0x0F, 0);
}
