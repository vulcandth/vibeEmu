#![allow(non_snake_case)]
mod common;
use std::path::Path;
use std::time::{Duration, Instant};
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, Model},
};

const TIMEOUT: Duration = Duration::from_secs(10);
const FIB_SEQ: [u8; 6] = [3, 5, 8, 13, 21, 34];
fn parse_cgb_revision_from_path<P: AsRef<Path>>(rom_path: P) -> Option<CgbRevision> {
    let s = rom_path.as_ref().to_string_lossy().to_ascii_uppercase();
    if s.ends_with("-A.GB") {
        return Some(CgbRevision::RevA);
    }
    if let Some(i) = s.find("CGB") {
        let mut revs = String::new();
        for ch in s[i + 3..].chars() {
            if ch.is_ascii_alphanumeric() {
                revs.push(ch);
            } else {
                break;
            }
        }
        if !revs.is_empty() {
            let first = revs.chars().next().unwrap();
            return match first {
                '0' => Some(CgbRevision::Rev0),
                'A' => Some(CgbRevision::RevA),
                'B' => Some(CgbRevision::RevB),
                'C' => Some(CgbRevision::RevC),
                'D' => Some(CgbRevision::RevD),
                'E' => Some(CgbRevision::RevE),
                _ => None,
            };
        }
    }
    None
}

fn run_same_suite<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> bool {
    let rom = std::fs::read(&rom_path).expect("rom not found");
    let cart = Cartridge::from_bytes(rom);
    let mut gb = if let Some(rev) = parse_cgb_revision_from_path(&rom_path) {
        GameBoy::new(Model::Cgb(rev))
    } else {
        GameBoy::new(Model::from_cgb_flag(cart.cgb))
    };
    gb.mmu.load_cart(cart);
    let start = Instant::now();
    while gb.cpu.cycles < max_cycles {
        if start.elapsed() >= TIMEOUT {
            return false;
        }
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.serial.peek_output().len() >= 6 {
            break;
        }
    }
    let out = gb.mmu.serial.take_output();
    let ok = out.len() >= 6 && out[0..6] == FIB_SEQ;
    if !ok {
        eprintln!("same suite output: {:02X?}", out);

        // SameSuite stores results at $C000 and sets RESULT_CODE at $CFFE.
        // Dump a fixed window for quick diagnosis.
        let result_code = gb.mmu.read_byte(0xCFFE);
        let mut results = [0u8; 144];
        for (i, b) in results.iter_mut().enumerate() {
            *b = gb.mmu.read_byte(0xC000 + i as u16);
        }
        eprintln!("same suite RESULT_CODE: 0x{result_code:02X}");
        for row in 0..18 {
            let base = row * 8;
            eprintln!(
                "Row {:2}: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
                row,
                results[base],
                results[base + 1],
                results[base + 2],
                results[base + 3],
                results[base + 4],
                results[base + 5],
                results[base + 6],
                results[base + 7]
            );
        }
    }
    ok
}

fn run_same_suite_gb<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> GameBoy {
    let rom = std::fs::read(&rom_path).expect("rom not found");
    let cart = Cartridge::from_bytes(rom);
    let mut gb = if let Some(rev) = parse_cgb_revision_from_path(&rom_path) {
        GameBoy::new(Model::Cgb(rev))
    } else {
        GameBoy::new(Model::from_cgb_flag(cart.cgb))
    };
    gb.mmu.load_cart(cart);
    let start = Instant::now();
    while gb.cpu.cycles < max_cycles {
        if start.elapsed() >= TIMEOUT {
            panic!("same suite test timed out");
        }
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.serial.peek_output().len() >= 6 {
            break;
        }
    }
    gb
}

fn dump_dma_debug_info<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) {
    if std::env::var_os("VIBEEMU_LOG_SERIAL").is_none() {
        return;
    }
    let mut gb = run_same_suite_gb(rom_path, max_cycles);
    let mut vram = [0u8; 32];
    for (i, byte) in vram.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0x8800 + i as u16);
    }
    let hdma_src_hi = gb.mmu.read_byte(0xFF51);
    let hdma_src_lo = gb.mmu.read_byte(0xFF52);
    let hdma_dst_hi = gb.mmu.read_byte(0xFF53);
    let hdma_dst_lo = gb.mmu.read_byte(0xFF54);
    let hdma_len = gb.mmu.read_byte(0xFF55);
    eprintln!(
        "VRAM[8800..8820]: {:02X?} | HDMA src {:02X}{:02X} dst {:02X}{:02X} len {:02X}",
        vram, hdma_src_hi, hdma_src_lo, hdma_dst_hi, hdma_dst_lo, hdma_len
    );
}

#[test]
fn same_suite__apu__channel_1__channel_1_align_gb() {
    const EXPECTED: [u8; 48] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08,
        0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08,
        0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
        0x08, 0x08, 0x08,
    ];
    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_1/channel_1_align.gb"),
        20_000_000,
    );
    let mut results = [0u8; EXPECTED.len()];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    if results != EXPECTED {
        println!("correct: {:02X?}", EXPECTED);
        println!("actual : {:02X?}", results);
        let matches = results
            .iter()
            .zip(EXPECTED.iter())
            .filter(|(a, b)| a == b)
            .count();
        let percent = matches as f32 / EXPECTED.len() as f32 * 100.0;
        println!("match {:.2}%", percent);
        panic!("test failed");
    }
}

#[test]
fn same_suite__apu__channel_1__channel_1_align_cpu_gb() {
    const EXPECTED: [u8; 48] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08,
        0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
        0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x08, 0x08,
    ];
    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_1/channel_1_align_cpu.gb"),
        20_000_000,
    );
    let mut results = [0u8; EXPECTED.len()];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    if results != EXPECTED {
        println!("correct: {:02X?}", EXPECTED);
        println!("actual : {:02X?}", results);
        let matches = results
            .iter()
            .zip(EXPECTED.iter())
            .filter(|(a, b)| a == b)
            .count();
        let percent = matches as f32 / EXPECTED.len() as f32 * 100.0;
        println!("match {:.2}%", percent);
        panic!("test failed");
    }
}

#[test]
fn same_suite__apu__channel_1__channel_1_delay_gb() {
    const EXPECTED: [u8; 32] = [
        0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00,
    ];
    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_1/channel_1_delay.gb"),
        20_000_000,
    );
    let mut results = [0u8; EXPECTED.len()];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    if results != EXPECTED {
        println!("correct: {:02X?}", EXPECTED);
        println!("actual : {:02X?}", results);
        let matches = results
            .iter()
            .zip(EXPECTED.iter())
            .filter(|(a, b)| a == b)
            .count();
        let percent = matches as f32 / EXPECTED.len() as f32 * 100.0;
        println!("match {:.2}%", percent);
        panic!("test failed");
    }
}

#[test]
fn same_suite__apu__channel_1__channel_1_duty_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_duty.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_duty_delay_gb() {
    // Expected results (16 rows x 8 columns = 128 bytes)
    const EXPECTED: [u8; 128] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x08, 0x08,
        0x08, 0x08, 0x08, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x08, 0x08,
        0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x08,
        0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_1/channel_1_duty_delay.gb"),
        20_000_000,
    );
    let mut results = [0u8; EXPECTED.len()];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    if results != EXPECTED {
        println!("correct: {:02X?}", EXPECTED);
        println!("actual : {:02X?}", results);
        let matches = results
            .iter()
            .zip(EXPECTED.iter())
            .filter(|(a, b)| a == b)
            .count();
        let percent = matches as f32 / EXPECTED.len() as f32 * 100.0;
        println!("match {:.2}%", percent);
        panic!("test failed");
    }
}

#[test]
fn same_suite__apu__channel_1__channel_1_extra_length_clocking_cgb0B_gb() {
    const EXPECTED: [u8; 24] = [
        0xF1, 0xF1, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF1, 0xF1, 0xF1, 0xF1, 0xF1, 0xF1, 0xF1,
        0xF1, 0xF1, 0xF1, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0,
    ];
    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_1/channel_1_extra_length_clocking-cgb0B.gb"),
        20_000_000,
    );
    let mut results = [0u8; EXPECTED.len()];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    if results != EXPECTED {
        println!("correct: {:02X?}", EXPECTED);
        println!("actual : {:02X?}", results);
        let matches = results
            .iter()
            .zip(EXPECTED.iter())
            .filter(|(a, b)| a == b)
            .count();
        let percent = matches as f32 / EXPECTED.len() as f32 * 100.0;
        println!("match {:.2}%", percent);
        panic!("test failed");
    }
}

#[test]
fn same_suite__apu__channel_1__channel_1_freq_change_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_freq_change.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn same_suite__apu__channel_1__channel_1_freq_change_timing_A_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_freq_change_timing-A.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn same_suite__apu__channel_1__channel_1_freq_change_timing_cgb0BC_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_freq_change_timing-cgb0BC.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn same_suite__apu__channel_1__channel_1_freq_change_timing_cgbDE_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_freq_change_timing-cgbDE.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_nrx2_glitch_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_nrx2_glitch.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_nrx2_speed_change_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_nrx2_speed_change.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_restart_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_restart.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_restart_nrx2_glitch_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_restart_nrx2_glitch.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_stop_div_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_stop_div.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_stop_restart_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_stop_restart.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_sweep_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_sweep.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_sweep_restart_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_sweep_restart.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_sweep_restart_2_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_sweep_restart_2.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_volume_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_volume.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_1__channel_1_volume_div_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_1/channel_1_volume_div.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_align_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_align.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_align_cpu_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_align_cpu.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_duty_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_duty.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_duty_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_duty_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_extra_length_clocking_cgb0B_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_extra_length_clocking-cgb0B.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_freq_change_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_freq_change.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_nrx2_glitch_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_nrx2_glitch.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_nrx2_speed_change_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_nrx2_speed_change.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_restart_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_restart.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_restart_nrx2_glitch_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_restart_nrx2_glitch.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_stop_div_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_stop_div.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_stop_restart_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_stop_restart.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_2__channel_2_volume_gb() {
    const EXPECTED: [u8; 8] = [0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x80, 0x80];
    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_2/channel_2_volume.gb"),
        20_000_000,
    );
    let mut results = [0u8; EXPECTED.len()];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    if results != EXPECTED {
        println!("correct: {:02X?}", EXPECTED);
        println!("actual : {:02X?}", results);
        let matches = results
            .iter()
            .zip(EXPECTED.iter())
            .filter(|(a, b)| a == b)
            .count();
        let percent = matches as f32 / EXPECTED.len() as f32 * 100.0;
        println!("match {:.2}%", percent);
        panic!("test failed");
    }
}

#[test]
fn same_suite__apu__channel_2__channel_2_volume_div_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_2/channel_2_volume_div.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_and_glitch_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_and_glitch.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_extra_length_clocking_cgb0_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_extra_length_clocking-cgb0.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn same_suite__apu__channel_3__channel_3_extra_length_clocking_cgbB_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_extra_length_clocking-cgbB.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_first_sample_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_first_sample.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_freq_change_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_freq_change_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_restart_delay_gb() {
    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_3/channel_3_restart_delay.gb"),
        20_000_000,
    );
    let mut results = [0u8; 8];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    println!("restart_delay results: {:02X?}", results);
    let expected = [0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0E, 0x0E, 0x0E];
    assert_eq!(results, expected, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_restart_during_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_restart_during_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_restart_stop_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_restart_stop_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_shift_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_shift_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_shift_skip_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_shift_skip_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_stop_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_stop_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_stop_div_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_stop_div.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_wave_ram_dac_on_rw_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_wave_ram_dac_on_rw.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_3__channel_3_wave_ram_locked_write_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_wave_ram_locked_write.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn same_suite__apu__channel_3__channel_3_wave_ram_sync_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_3/channel_3_wave_ram_sync.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_align_gb() {
    const EXPECTED: [u8; 64] = [
        // Row 0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Row 1
        0x00, 0x00, 0x00, 0x00, 0xF0, 0xF0, 0xF0, 0xF0, // Row 2
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Row 3
        0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0xF0, 0xF0, // Row 4
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Row 5
        0x00, 0x00, 0x00, 0x00, 0xF0, 0xF0, 0xF0, 0xF0, // Row 6
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Row 7
        0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0xF0, 0xF0,
    ];
    let mut gb = run_same_suite_gb(
        common::rom_path("same-suite/apu/channel_4/channel_4_align.gb"),
        20_000_000,
    );
    let mut results = [0u8; EXPECTED.len()];
    for (i, byte) in results.iter_mut().enumerate() {
        *byte = gb.mmu.read_byte(0xC000 + i as u16);
    }
    if results != EXPECTED {
        println!("correct: {:02X?}", EXPECTED);
        println!("actual : {:02X?}", results);
        let matches = results
            .iter()
            .zip(EXPECTED.iter())
            .filter(|(a, b)| a == b)
            .count();
        let percent = matches as f32 / EXPECTED.len() as f32 * 100.0;
        println!("match {:.2}%", percent);
        panic!("test failed");
    }
}

#[test]
fn same_suite__apu__channel_4__channel_4_delay_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_delay.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_equivalent_frequencies_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_equivalent_frequencies.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_extra_length_clocking_cgb0B_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_extra_length_clocking-cgb0B.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_freq_change_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_freq_change.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_frequency_alignment_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_frequency_alignment.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_lfsr_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_lfsr.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_lfsr15_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_lfsr15.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_lfsr_15_7_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_lfsr_15_7.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_lfsr_7_15_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_lfsr_7_15.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_lfsr_restart_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_lfsr_restart.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_lfsr_restart_fast_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_lfsr_restart_fast.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__channel_4__channel_4_volume_div_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/channel_4/channel_4_volume_div.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__div_trigger_volume_10_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/div_trigger_volume_10.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__div_write_trigger_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/div_write_trigger.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__div_write_trigger_10_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/div_write_trigger_10.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__div_write_trigger_volume_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/div_write_trigger_volume.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__apu__div_write_trigger_volume_10_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/apu/div_write_trigger_volume_10.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__dma__gbc_dma_cont_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/dma/gbc_dma_cont.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__dma__gdma_addr_mask_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/dma/gdma_addr_mask.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__dma__hdma_lcd_off_gb() {
    let rom = common::rom_path("same-suite/dma/hdma_lcd_off.gb");
    let passed = run_same_suite(&rom, 20_000_000);
    if !passed {
        dump_dma_debug_info(&rom, 20_000_000);
    }
    assert!(passed, "test failed");
}

#[test]
fn same_suite__dma__hdma_mode0_gb() {
    let rom = common::rom_path("same-suite/dma/hdma_mode0.gb");
    let passed = run_same_suite(&rom, 20_000_000);
    if !passed {
        dump_dma_debug_info(&rom, 20_000_000);
    }
    assert!(passed, "test failed");
}

#[test]
fn same_suite__interrupt__ei_delay_halt_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/interrupt/ei_delay_halt.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn same_suite__ppu__blocking_bgpi_increase_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/ppu/blocking_bgpi_increase.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn same_suite__sgb__command_mlt_req_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/sgb/command_mlt_req.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn same_suite__sgb__command_mlt_req_1_incrementing_gb() {
    let passed = run_same_suite(
        common::rom_path("same-suite/sgb/command_mlt_req_1_incrementing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}
