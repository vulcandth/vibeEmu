//! GBMicrotest from the pinned c-sp bundle, using its documented HRAM protocol.
//! Upstream verifies DMG hardware; these cases run on DMG revision C.
mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use libtest_mimic::{Arguments, Failed, Trial};
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{DmgRevision, Model},
};

// Most cases finish within two frames. is_if_set_during_ime0 needs ~380 ms.
const MAX_CYCLES: u64 = 2_000_000;

fn run_case(path: &Path) -> Result<(), String> {
    let mut gb = GameBoy::new(Model::Dmg(DmgRevision::RevC));
    gb.mmu.load_cart(Cartridge::from_bytes(
        fs::read(path).map_err(|err| err.to_string())?,
    ));
    while gb.cpu.cycles < MAX_CYCLES {
        // Observe the result-store instruction, rather than accepting an
        // uninitialized HRAM byte or bytes copied there as DMA test code.
        // Both macros.inc:test_end2 and the MBC tests use LDH [$82],A.
        let result_store = !gb.cpu.halted
            && !gb.cpu.stopped
            && gb.mmu.read_byte(gb.cpu.pc) == 0xe0
            && gb.mmu.read_byte(gb.cpu.pc.wrapping_add(1)) == 0x82
            && matches!(gb.cpu.a, 1 | 0xff);
        gb.cpu.step(&mut gb.mmu);
        if result_store {
            // Only FF82 is authoritative: some failures intentionally store
            // identical actual/expected bytes in FF80 and FF81.
            return match gb.mmu.hram[2] {
                1 => Ok(()),
                0xff => Err(format!(
                    "reported failure: actual={:02X}, expected={:02X}, PC={:04X}, cycles={}",
                    gb.mmu.hram[0], gb.mmu.hram[1], gb.cpu.pc, gb.cpu.cycles
                )),
                value => Err(format!("result store did not complete: FF82={value:02X}")),
            };
        }
        if gb.cpu.stopped || gb.cpu.faulted {
            return Err(format!(
                "CPU {} at PC={:04X}",
                if gb.cpu.stopped { "stopped" } else { "faulted" },
                gb.cpu.pc
            ));
        }
    }
    Err(format!(
        "no completion after {MAX_CYCLES} cycles: PC={:04X}, HRAM={:02X?}",
        gb.cpu.pc,
        &gb.mmu.hram[..3]
    ))
}

fn main() {
    let args = Arguments::from_args();
    let root = common::rom_path("gbmicrotest");
    let ignored: BTreeSet<_> = include_str!("gbmicrotest_ignored.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let mut names = BTreeSet::new();
    let mut trials = Vec::new();
    for entry in fs::read_dir(root).expect("cannot read GBMicrotest ROM directory") {
        let path = entry.expect("cannot read GBMicrotest ROM entry").path();
        if !path.extension().is_some_and(|ext| ext == "gb") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        assert!(
            names.insert(name.clone()),
            "duplicate GBMicrotest case {name}"
        );
        let ignore = ignored.contains(name.as_str());
        trials.push(
            Trial::test(name, move || run_case(&path).map_err(Failed::from))
                .with_ignored_flag(ignore),
        );
    }
    assert!(!trials.is_empty(), "no GBMicrotest ROMs found");
    for name in ignored {
        assert!(
            names.contains(name),
            "stale GBMicrotest ignore entry: {name}"
        );
    }
    trials.sort_by(|a, b| a.name().cmp(b.name()));
    libtest_mimic::run(&args, trials).exit();
}
