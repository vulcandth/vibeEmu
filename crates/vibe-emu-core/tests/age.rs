//! AGE ROMs from the pinned c-sp bundle. See age-test-roms/game-boy-test-roms-howto.md.
mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use libtest_mimic::{Arguments, Failed, Trial};
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision, Model},
};

const MAX_CYCLES: u64 = 30_000_000;
const FIBONACCI: [u8; 6] = [3, 5, 8, 13, 21, 34];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Device {
    family: &'static str,
    revision: char,
    model: Model,
}

// Only explicitly verified devices are selected, including every revision in a
// suffix such as dmgC-cgbBCE. "ncm" means a DMG cartridge on CGB hardware.
fn devices(stem: &str) -> Vec<Device> {
    let mut result = Vec::new();
    for part in stem.split('-') {
        for family in ["dmg", "cgb", "ncm"] {
            if let Some(revisions) = part.strip_prefix(family) {
                for revision in revisions.chars() {
                    let model = match (family, revision) {
                        ("dmg", 'C') => Model::Dmg(DmgRevision::RevC),
                        ("cgb" | "ncm", 'B') => Model::Cgb(CgbRevision::RevB),
                        ("cgb" | "ncm", 'C') => Model::Cgb(CgbRevision::RevC),
                        ("cgb" | "ncm", 'E') => Model::Cgb(CgbRevision::RevE),
                        _ => panic!("unsupported AGE device in {stem}"),
                    };
                    result.push(Device {
                        family,
                        revision,
                        model,
                    });
                }
            }
        }
    }
    result
}

fn collect_roms(dir: &Path, roms: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("cannot read AGE ROM directory") {
        let path = entry.expect("cannot read AGE ROM entry").path();
        if path.is_dir() {
            collect_roms(&path, roms);
        } else if path.extension().is_some_and(|ext| ext == "gb") {
            roms.push(path);
        }
    }
}

fn run_case(rom: &Path, device: Device, png: Option<&Path>) -> Result<(), String> {
    let cart = Cartridge::from_bytes(fs::read(rom).map_err(|err| err.to_string())?);
    if (device.family == "ncm" && cart.cgb) || (device.family == "cgb" && !cart.cgb) {
        return Err("ROM header does not match the requested AGE mode".into());
    }
    let mut gb = GameBoy::new(device.model);
    gb.mmu.load_cart(cart);
    gb.mmu
        .ppu
        .set_dmg_palette([0xFFFFFF, 0xAAAAAA, 0x555555, 0]);

    while gb.cpu.cycles < MAX_CYCLES {
        // AGE's LD B,B completion marker is used by both numerical and visual
        // tests. Any non-Fibonacci register result is a failure, not a timeout.
        if !gb.cpu.halted && !gb.cpu.stopped && gb.mmu.read_byte(gb.cpu.pc) == 0x40 {
            if let Some(png) = png {
                return compare_screen(&gb, png);
            }
            let registers = [gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l];
            return if registers == FIBONACCI {
                Ok(())
            } else {
                Err(format!(
                    "AGE failure at PC={:04X}, BC/DE/HL={registers:02X?}; WRAM results={:02X?}",
                    gb.cpu.pc,
                    &gb.mmu.wram[0][..129]
                ))
            };
        }
        gb.cpu.step(&mut gb.mmu);
        if gb.cpu.stopped {
            return Err(format!("unexpected STOP at PC={:04X}", gb.cpu.pc));
        }
        if gb.cpu.faulted {
            return Err(format!("CPU fault at PC={:04X}", gb.cpu.pc));
        }
    }
    Err(format!(
        "AGE timed out after {MAX_CYCLES} cycles at PC={:04X}",
        gb.cpu.pc
    ))
}

fn compare_screen(gb: &GameBoy, png: &Path) -> Result<(), String> {
    let (width, height, expected) = common::load_png_rgb(png);
    if (width, height) != (160, 144) {
        return Err(format!("invalid reference dimensions: {width}x{height}"));
    }
    let mismatches: Vec<_> = gb
        .mmu
        .ppu
        .framebuffer()
        .iter()
        .zip(expected.iter())
        .enumerate()
        .filter_map(|(index, (&actual, &[r, g, b]))| {
            let expected = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
            (actual != expected).then_some((index, actual, expected))
        })
        .collect();
    if let Some(&(index, actual, expected)) = mismatches.first() {
        return Err(format!(
            "{}: {} pixel mismatches; first at ({}, {}): {actual:06X}, expected {expected:06X}",
            png.display(),
            mismatches.len(),
            index % 160,
            index / 160
        ));
    }
    Ok(())
}

fn main() {
    let args = Arguments::from_args();
    let root = common::rom_path("age-test-roms");
    let mut roms = Vec::new();
    collect_roms(&root, &mut roms);
    roms.sort();
    assert!(!roms.is_empty(), "no AGE ROMs found");
    let ignored: BTreeSet<_> = include_str!("age_ignored.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let mut names = BTreeSet::new();
    let mut trials = Vec::new();
    for rom in roms {
        let stem = rom.file_stem().unwrap().to_str().unwrap();
        let relative = rom
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let mut cases: Vec<(Device, Option<PathBuf>)> = devices(stem)
            .into_iter()
            .map(|device| (device, None))
            .collect();
        if cases.is_empty() {
            // Visual ROM names omit hardware suffixes; their matching PNGs
            // declare the supported devices instead. Parse only the suffix so
            // m3-bg-scx.gb cannot accidentally claim m3-bg-scx-ds-cgbBCE.png.
            let prefix = format!("{stem}-");
            for entry in fs::read_dir(rom.parent().unwrap()).unwrap() {
                let png = entry.unwrap().path();
                if png.extension().is_some_and(|ext| ext == "png") {
                    let png_stem = png.file_stem().unwrap().to_str().unwrap();
                    if let Some(suffix) = png_stem.strip_prefix(&prefix)
                        && suffix.split('-').all(|part| {
                            ["dmg", "cgb", "ncm"]
                                .iter()
                                .any(|family| part.starts_with(family))
                        })
                    {
                        for device in devices(suffix) {
                            cases.push((device, Some(png.clone())));
                        }
                    }
                }
            }
        }
        assert!(
            !cases.is_empty(),
            "no verified devices or screenshots for {relative}"
        );
        for (device, png) in cases {
            let name = format!("{relative}::{}{}", device.family, device.revision);
            assert!(names.insert(name.clone()), "duplicate AGE case {name}");
            let ignore = ignored.contains(name.as_str());
            let rom = rom.clone();
            trials.push(
                Trial::test(name, move || {
                    run_case(&rom, device, png.as_deref()).map_err(Failed::from)
                })
                .with_ignored_flag(ignore),
            );
        }
    }
    for name in ignored {
        assert!(names.contains(name), "stale AGE ignore entry: {name}");
    }
    trials.sort_by(|a, b| a.name().cmp(b.name()));
    libtest_mimic::run(&args, trials).exit();
}
