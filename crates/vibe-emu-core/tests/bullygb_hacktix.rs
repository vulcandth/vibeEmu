#![allow(non_snake_case)]

mod common;

use std::time::{Duration, Instant};
use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

const EXPECTED_SUCCESS: &[u8] = b"All tests OK!";

// Keep timeouts bounded so CI doesn't hang forever.
const MAX_WALL_TIME: Duration = Duration::from_secs(20);
const MAX_CYCLES: u64 = 80_000_000;

// BullyGB should emit some serial output early; this is just to confirm the ROM
// is using serial in our harness at all.
const SERIAL_PROBE_WALL_TIME: Duration = Duration::from_secs(20);
const SERIAL_PROBE_CYCLES: u64 = 50_000_000;

// If the ROM stops producing new serial characters for long enough,
// assume it has locked up (BullyGB does `jr @` after printing a result).
const SERIAL_IDLE_CYCLES: u64 = 20_000_000;

fn serial_contains_marker(serial: &[u8], checked_up_to: &mut usize, marker: &[u8]) -> bool {
    let lookbehind = marker.len().saturating_sub(1);
    let start = checked_up_to.saturating_sub(lookbehind).min(serial.len());
    let window = &serial[start..];
    let found = window.windows(marker.len()).any(|chunk| chunk == marker);
    *checked_up_to = serial.len();
    found
}

fn run_bullygb(mode_cgb: bool) {
    let rom_bytes = std::fs::read(common::rom_path("bullygb/bullygb-v1.2.gb"))
        .expect("bullygb-v1.2.gb not found (download should happen automatically)");

    let boot_rom_path = if mode_cgb {
        common::cgb_boot_rom_path()
    } else {
        common::dmg_boot_rom_path()
    };
    let boot_rom_bytes = std::fs::read(&boot_rom_path).expect("boot ROM not found");

    let cart = Cartridge::from_bytes(rom_bytes);

    // Force DMG/CGB mode explicitly (even if the ROM header is CGB-compatible)
    // and start from a power-on state so we can execute the real boot ROM.
    let mut gb = GameBoy::new_power_on(Model::from_cgb_flag(mode_cgb));
    gb.mmu.load_cart(cart);
    gb.mmu.load_boot_rom(boot_rom_bytes);

    let start = Instant::now();

    // 1) Confirm the ROM emits *something* on serial.
    // Boot ROM intros can delay the first serial output.
    let probe_cycles = SERIAL_PROBE_CYCLES.saturating_mul(4);
    let probe_wall_time = SERIAL_PROBE_WALL_TIME.saturating_mul(2);

    let probe_start = Instant::now();
    while gb.cpu.cycles < probe_cycles {
        if probe_start.elapsed() > probe_wall_time {
            break;
        }
        gb.cpu.step(&mut gb.mmu);
        if !gb.mmu.serial.peek_sb_output().is_empty() {
            break;
        }
    }

    if gb.mmu.serial.peek_sb_output().is_empty() {
        panic!(
            "BullyGB produced no serial output during probe (mode={}); pc={:04X} cycles={}\nThis likely means the ROM isn't emitting rSB (FF01) debug output, or writes to rSB aren't being captured.\nserial={:?}",
            if mode_cgb { "CGB" } else { "DMG" },
            gb.cpu.pc,
            gb.cpu.cycles,
            String::from_utf8_lossy(gb.mmu.serial.peek_sb_output())
        );
    }

    // 2) Run until success marker or we appear to have locked up.
    let mut checked_up_to = 0usize;
    let mut last_serial_len = gb.mmu.serial.peek_sb_output().len();
    let mut last_progress_cycle = gb.cpu.cycles;
    while gb.cpu.cycles < MAX_CYCLES {
        if start.elapsed() > MAX_WALL_TIME {
            break;
        }

        gb.cpu.step(&mut gb.mmu);

        let serial = gb.mmu.serial.peek_sb_output();
        if serial.len() != last_serial_len {
            last_serial_len = serial.len();
            last_progress_cycle = gb.cpu.cycles;
        } else if gb
            .cpu
            .cycles
            .saturating_sub(last_progress_cycle)
            .gt(&SERIAL_IDLE_CYCLES)
        {
            break;
        }
        if serial_contains_marker(serial, &mut checked_up_to, EXPECTED_SUCCESS) {
            return;
        }
    }

    let out = gb.mmu.serial.take_sb_output();
    panic!(
        "BullyGB did not report success before timeout (mode={}); pc={:04X} cycles={}\nserial=\n{}",
        if mode_cgb { "CGB" } else { "DMG" },
        gb.cpu.pc,
        gb.cpu.cycles,
        String::from_utf8_lossy(&out)
    );
}

#[test]
fn bullygb_hacktix__dmg__serial() {
    run_bullygb(false);
}

#[test]
fn bullygb_hacktix__cgb__serial() {
    run_bullygb(true);
}
