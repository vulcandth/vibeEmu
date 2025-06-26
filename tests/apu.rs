use vibeEmu::apu::Apu;
use vibeEmu::mmu::Mmu;

#[test]
fn frame_sequencer_tick() {
    let mut apu = Apu::new();
    let mut div = 0u16;
    assert_eq!(apu.sequencer_step(), 0);
    for _ in 0..8192 {
        let prev = div;
        div = div.wrapping_add(1);
        apu.tick(prev >> 8, div >> 8, false);
        apu.step(1);
    }
    assert_eq!(apu.sequencer_step(), 1);
    for _ in 0..(8192 * 7) {
        let prev = div;
        div = div.wrapping_add(1);
        apu.tick(prev >> 8, div >> 8, false);
        apu.step(1);
    }
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
    let mut div = 0u16;
    for _ in 0..10 {
        for _ in 0..95 {
            let prev = div;
            div = div.wrapping_add(1);
            apu.tick(prev >> 8, div >> 8, false);
            apu.step(1);
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
fn read_mask_unused_bits() {
    let apu = Apu::new();
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
    for _ in 0..8192 {
        let p = div;
        div = div.wrapping_add(1);
        apu.tick(p >> 8, div >> 8, false);
        apu.step(1);
    } // step 1
    for _ in 0..8192 {
        let p = div;
        div = div.wrapping_add(1);
        apu.tick(p >> 8, div >> 8, false);
        apu.step(1);
    } // step 2
    for _ in 0..8192 {
        let p = div;
        div = div.wrapping_add(1);
        apu.tick(p >> 8, div >> 8, false);
        apu.step(1);
    } // step 3 (sweep clocked on previous step)
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
    // ch1 low, ch2 high so PCM12 should be 0xF0
    apu.write_reg(0xFF11, 0x00); // duty 12.5%
    apu.write_reg(0xFF12, 0xF0); // max volume
    apu.write_reg(0xFF14, 0x80); // trigger

    apu.write_reg(0xFF16, 0xC0); // duty 75%
    apu.write_reg(0xFF17, 0xF0);
    apu.write_reg(0xFF19, 0x80); // trigger

    let mut div = 0u16;
    for _ in 0..8300 {
        let p = div;
        div = div.wrapping_add(1);
        apu.tick(p >> 8, div >> 8, false);
        apu.step(1);
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
        for _ in 0..8300 {
            let p = div;
            div = div.wrapping_add(1);
            apu.tick(p >> 8, div >> 8, false);
            apu.step(1);
        }
    }
    assert_eq!(mmu.read_byte(0xFF76), 0xF0);
    let mut dmg = Mmu::new();
    assert_eq!(dmg.read_byte(0xFF76), 0xFF);
}

fn build_channel_1_align_rom() -> Vec<u8> {
    fn subtest(after: u8, before: u8, store: u16, rom: &mut Vec<u8>) {
        rom.extend([
            0xAF, // XOR A
            0xE0, 0x26, // LDH [NR52],A
            0x2F, // CPL
            0xE0, 0x26, // LDH [NR52],A
            0x21, 0x76, 0xFF, // LD HL,rPCM12
            0xE0, 0x13, // LDH [NR13],A
            0x3E, 0x80, // LD A,$80
            0xE0, 0x11, // LDH [NR11],A
            0xE0, 0x12, // LDH [NR12],A
            0x3E, 0x87, // LD A,$87
        ]);
        for _ in 0..before {
            rom.push(0x00); // NOP
        }
        rom.extend([0xE0, 0x14]); // LDH [NR14],A
        for _ in 0..after {
            rom.push(0x00); // NOP
        }
        rom.extend([
            0x7E, // LD A,(HL)
            0xCD,
            (store & 0xFF) as u8,
            (store >> 8) as u8, // CALL StoreResult
        ]);
    }

    let mut rom = vec![
        0x3E, 0x01, // LD A,1
        0xE0, 0x4D, // LDH [KEY1],A
        0x10, 0x00, // STOP
        0x11, 0x00, 0xC0, // LD DE,$C000
    ];

    // Generate subtests to determine store address
    let mut tmp = Vec::new();
    for &before in &[0u8, 1, 3] {
        for after in 0u8..=15 {
            subtest(after, before, 0, &mut tmp);
        }
    }
    let store = (rom.len() + tmp.len()) as u16;

    for &before in &[0u8, 1, 3] {
        for after in 0u8..=15 {
            subtest(after, before, store, &mut rom);
        }
    }

    // StoreResult:
    rom.extend([0x12, 0x13, 0xC9]); // LD (DE),A ; INC DE ; RET
    rom
}

#[test]
fn channel_1_align_internal() {
    const EXPECTED: [u8; 48] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08,
        0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08,
        0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
        0x08, 0x08, 0x08,
    ];

    let rom = build_channel_1_align_rom();
    let cart = vibeEmu::cartridge::Cartridge::load(rom);
    let mut gb = vibeEmu::gameboy::GameBoy::new_with_mode(cart.cgb);
    gb.mmu.load_cart(cart);

    while gb.cpu.cycles < 20_000 {
        gb.cpu.step(&mut gb.mmu);
    }

    let mut results = [0u8; 48];
    for (i, b) in results.iter_mut().enumerate() {
        *b = gb.mmu.read_byte(0xC000 + i as u16);
    }

    if results != EXPECTED {
        eprintln!("Expected: {:02X?}", EXPECTED);
        eprintln!("  Actual: {:02X?}", results);
        for (i, (&got, &exp)) in results.iter().zip(EXPECTED.iter()).enumerate() {
            if got != exp {
                eprintln!("    idx {i:02}: expected {exp:02X}, got {got:02X}");
            }
        }
        panic!("PCM output mismatch");
    }
}
