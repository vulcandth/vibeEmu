//! Shared Mooneye runner. See mooneye_suites.md and each bundle's how-to.
use libtest_mimic::{Arguments, Failed, Trial};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision, Model},
};

const PASS: [u8; 6] = [3, 5, 8, 13, 21, 34];
const FAIL: [u8; 6] = [0x42; 6];
// Upstream permits 120 emulated seconds, including large MBC/RAM sweeps.
const MAX_CYCLES: u64 = 120 * 4_194_304;

fn collect(root: &Path, directory: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(directory).expect("read ROM directory") {
        let path = entry.expect("read ROM entry").path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.extension().is_some_and(|ext| ext == "gb") {
            files.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/"),
            );
        }
    }
}

fn run(
    path: &Path,
    name: &str,
    wilbertpol: bool,
    screenshot_model: Option<bool>,
) -> Result<(), String> {
    // Dumpers and electrical logic-analyzer workloads are not assertions.
    // Keep them visible in the inventory, even when --ignored is requested.
    if name.starts_with("utils/") || name.starts_with("logic-analysis/") {
        return Err("diagnostic workload: no pass/fail assertion or supplied reference".into());
    }
    if name.contains("mgb_")
        || name.ends_with("-mgb.gb")
        || name.ends_with("-A.gb")
        || name.ends_with("-S.gb")
        || name.ends_with("-sgb.gb")
        || name.ends_with("-sgb2.gb")
    {
        return Err("requires an unimplemented MGB, SGB, SGB2 or AGB hardware model".into());
    }
    let cart = Cartridge::from_bytes(fs::read(path).map_err(|e| e.to_string())?);
    let cgb = screenshot_model
        .unwrap_or_else(|| name.contains("-cgb") || name.ends_with("-C.gb") || cart.cgb);
    let model = if cgb {
        Model::Cgb(if name.contains("-cgb0") {
            CgbRevision::Rev0
        } else {
            CgbRevision::RevE
        })
    } else {
        Model::Dmg(DmgRevision::RevC)
    };
    let boot = name.contains("/boot_");
    let mut gb = if boot {
        GameBoy::new_power_on(model)
    } else {
        GameBoy::new(model)
    };
    if boot {
        let boot_path = if cgb {
            crate::common::cgb_boot_rom_path()
        } else {
            crate::common::dmg_boot_rom_path()
        };
        gb.mmu
            .load_boot_rom(fs::read(boot_path).map_err(|e| e.to_string())?);
    }
    gb.mmu.load_cart(cart);
    gb.mmu
        .ppu
        .set_dmg_palette([0xffffff, 0xaaaaaa, 0x555555, 0]);
    let completion = if wilbertpol { 0xed } else { 0x40 };
    while gb.cpu.cycles < MAX_CYCLES {
        let pc = gb.cpu.pc;
        if !gb.cpu.halted && gb.mmu.read_byte(pc) == completion {
            if screenshot_model.is_some() {
                // The old suite breaks immediately after LCD enable; the new
                // suite waits two VBlanks first. Freeze CPU execution at the
                // marker and give the PPU two complete frames in either case.
                gb.mmu.ppu.clear_frame_flag();
                let mut frames = 0;
                for _ in 0..3 * 154 {
                    gb.mmu.ppu.step(456, &mut gb.mmu.if_reg);
                    if gb.mmu.ppu.frame_ready() {
                        gb.mmu.ppu.clear_frame_flag();
                        frames += 1;
                        if frames == 3 {
                            break;
                        }
                    }
                }
                if frames != 3 {
                    return Err("screenshot did not finish rendering".into());
                }
                let reference = path.with_file_name(format!(
                    "sprite_priority-{}.png",
                    if cgb { "cgb" } else { "dmg" }
                ));
                let (w, h, expected) = crate::common::load_png_rgb(reference);
                assert_eq!((w, h), (160, 144));
                for (i, (&actual, &[r, g, b])) in gb
                    .mmu
                    .ppu
                    .framebuffer()
                    .iter()
                    .zip(expected.iter())
                    .enumerate()
                {
                    let color = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                    if actual != color {
                        return Err(format!(
                            "pixel ({},{}): actual={actual:06X}, expected={color:06X}",
                            i % 160,
                            i / 160
                        ));
                    }
                }
                return Ok(());
            }
            let regs = [gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l];
            if regs == PASS {
                return Ok(());
            }
            if wilbertpol || regs == FAIL {
                return Err(format!(
                    "failure at PC={pc:04X}, cycles={}, regs={regs:02X?}, HRAM={:02X?}, WRAM={:02X?}",
                    gb.cpu.cycles,
                    &gb.mmu.hram[..48],
                    &gb.mmu.wram[0][..64]
                ));
            }
        }
        gb.cpu.step(&mut gb.mmu);
        if gb.cpu.faulted || gb.cpu.stopped {
            return Err(format!("CPU fault/STOP at PC={:04X}", gb.cpu.pc));
        }
    }
    Err(format!(
        "timeout after {MAX_CYCLES} dots at PC={:04X}, HRAM={:02X?}",
        gb.cpu.pc,
        &gb.mmu.hram[..48]
    ))
}

pub fn main(wilbertpol: bool, ignores: &str, already_covered: &[&str]) {
    let root: PathBuf = crate::common::rom_path(if wilbertpol {
        "mooneye-test-suite-wilbertpol"
    } else {
        "mooneye-test-suite"
    });
    let mut ignored: BTreeMap<&str, &str> = ignores
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .map(|s| s.split_once(" # ").expect("ignore must explain its reason"))
        .collect();
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();
    let mut trials = Vec::new();
    for name in files {
        if !wilbertpol
            && (name.starts_with("acceptance/") || already_covered.contains(&name.as_str()))
        {
            continue;
        }
        let profiles = if name == "manual-only/sprite_priority.gb" {
            vec![Some(false), Some(true)]
        } else {
            vec![None]
        };
        for profile in profiles {
            let case = match profile {
                Some(cgb) => format!("{name}/{}", if cgb { "cgb" } else { "dmg" }),
                None => name.clone(),
            };
            let skip = ignored.remove(case.as_str()).is_some();
            let path = root.join(&name);
            let name = name.clone();
            trials.push(
                Trial::test(case, move || {
                    run(&path, &name, wilbertpol, profile).map_err(Failed::from)
                })
                .with_ignored_flag(skip),
            );
        }
    }
    assert!(!trials.is_empty(), "no Mooneye ROMs found");
    assert!(ignored.is_empty(), "stale ignore entries: {ignored:?}");
    libtest_mimic::run(&Arguments::from_args(), trials).exit();
}
