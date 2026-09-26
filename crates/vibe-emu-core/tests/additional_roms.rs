//! Remaining bundled ROMs; protocols and diagnostic exceptions: rom_inventory.md.
mod common;

use libtest_mimic::{Arguments, Failed, Trial};
use std::{collections::BTreeMap, fs};
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision, Model},
};

const FRAME: u64 = 70_224;
const SECOND: u64 = 4_194_304;

#[derive(Clone)]
enum Check {
    // Aggregates must reach their final screen: a single subtest can publish
    // a passing RAM/serial result before the rest have run.
    Screenshot { png: String, dots: u64 },
    Rtc { menu: u8, seconds: u64, png: String },
    Statcount,
    Blargg { seconds: u64 },
    Bully,
    Diagnostic(&'static str),
}

struct Case {
    rom: String,
    cgb: bool,
    check: Check,
}

impl Case {
    fn name(&self) -> String {
        let suffix = match &self.check {
            Check::Rtc { menu, .. } => format!("/menu{menu}"),
            _ => String::new(),
        };
        format!(
            "{}{suffix}/{}",
            self.rom,
            if self.cgb { "cgb" } else { "dmg" }
        )
    }
}

fn run_dots(gb: &mut GameBoy, dots: u64) -> Result<(), String> {
    let end = gb.cpu.cycles + dots;
    while gb.cpu.cycles < end {
        gb.cpu.step(&mut gb.mmu);
        if gb.cpu.faulted || gb.cpu.stopped {
            return Err(format!("CPU fault/STOP at PC={:04X}", gb.cpu.pc));
        }
    }
    Ok(())
}

fn press(gb: &mut GameBoy, bit: u8) -> Result<(), String> {
    gb.mmu.input.update_state(!(1 << bit), &mut gb.mmu.if_reg);
    run_dots(gb, 2 * FRAME)?;
    gb.mmu.input.update_state(0xff, &mut gb.mmu.if_reg);
    // Release long enough for Scribbl's five-VBlank input cooldown, too.
    run_dots(gb, 8 * FRAME)
}

fn screenshot(gb: &mut GameBoy, png: &str) -> Result<(), String> {
    // Capture a completed frame, never the partially rendered working buffer.
    gb.mmu.ppu.clear_frame_flag();
    let deadline = gb.cpu.cycles + 2 * FRAME;
    while !gb.mmu.ppu.frame_ready() && gb.cpu.cycles < deadline {
        run_dots(gb, 4)?;
    }
    if !gb.mmu.ppu.frame_ready() {
        return Err("LCD produced no complete frame".into());
    }
    let (w, h, pixels) = common::load_png_rgb(common::rom_path(png));
    assert_eq!((w, h), (160, 144), "invalid reference dimensions");
    let differences: Vec<_> = gb
        .mmu
        .ppu
        .framebuffer()
        .iter()
        .zip(pixels.iter())
        .enumerate()
        .filter_map(|(i, (&actual, &[r, g, b]))| {
            let expected = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            // A few bundled PNGs use older display palettes or near-white
            // export artifacts instead of the colors specified by their how-to.
            // Normalize only those known colors; every pixel and shade must
            // still match exactly.
            let expected = match (png, expected) {
                ("scribbltests/scxly/scxly-cgb.png", 0x0f380f) => 0,
                ("scribbltests/scxly/scxly-cgb.png", 0x98c00f) => 0xffffff,
                ("mbc3-tester/mbc3-tester-cgb.png", 0x7bff4a) => 0x7bff31,
                ("blargg/oam_bug/oam_bug-dmg.png", 0xfefefe | 0xfdfdfd) => 0xffffff,
                _ => expected,
            };
            (actual != expected).then_some((i % 160, i / 160, actual, expected))
        })
        .collect();
    if differences.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} pixels differ from {png}; first (x,y,actual,expected): {:06X?}",
            differences.len(),
            &differences[..differences.len().min(8)]
        ))
    }
}

fn run(case: Case) -> Result<(), String> {
    if let Check::Diagnostic(reason) = case.check {
        return Err(reason.into());
    }
    let model = if case.cgb {
        Model::Cgb(CgbRevision::RevD)
    } else {
        Model::Dmg(DmgRevision::RevC)
    };
    let boot = matches!(case.check, Check::Bully);
    let mut gb = if boot {
        GameBoy::new_power_on(model)
    } else {
        GameBoy::new(model)
    };
    gb.mmu.load_cart(Cartridge::from_bytes(
        fs::read(common::rom_path(&case.rom)).map_err(|e| e.to_string())?,
    ));
    gb.mmu
        .ppu
        .set_dmg_palette([0xffffff, 0xaaaaaa, 0x555555, 0]);
    if boot {
        let path = if case.cgb {
            common::cgb_boot_rom_path()
        } else {
            common::dmg_boot_rom_path()
        };
        gb.mmu
            .load_boot_rom(fs::read(path).map_err(|e| e.to_string())?);
    }
    match case.check {
        Check::Screenshot { png, dots } => {
            run_dots(&mut gb, dots)?;
            screenshot(&mut gb, &png)
        }
        Check::Rtc { menu, seconds, png } => {
            run_dots(&mut gb, 30 * FRAME)?;
            for _ in 0..menu {
                press(&mut gb, 3)?;
            }
            press(&mut gb, 4)?;
            run_dots(&mut gb, seconds * SECOND)?;
            screenshot(&mut gb, &png)
        }
        Check::Statcount => {
            run_dots(&mut gb, 30 * FRAME)?;
            press(&mut gb, 7)?;
            // statcount.sm83: Start runs the selected one-NOP probe and writes
            // ASCII "OK" to BG tiles $98A7/8 only after comparing StatTestData.
            // Inspect VRAM directly: CPU reads can be blocked during mode 3.
            let result = &gb.mmu.ppu.vram[0][0x18a7..0x18a9];
            if result == b"OK" {
                Ok(())
            } else {
                Err(format!(
                    "manual STAT probe did not report OK: {result:02X?}"
                ))
            }
        }
        Check::Blargg { seconds } => {
            let mut seen_running = false;
            let deadline = gb.cpu.cycles + seconds * SECOND;
            while gb.cpu.cycles < deadline {
                run_dots(&mut gb, FRAME)?;
                // Blargg's documented cartridge-RAM protocol: signature plus
                // $80 while running, then a final result (zero means pass).
                if [
                    gb.mmu.read_byte(0xa001),
                    gb.mmu.read_byte(0xa002),
                    gb.mmu.read_byte(0xa003),
                ] == [0xde, 0xb0, 0x61]
                {
                    let result = gb.mmu.read_byte(0xa000);
                    // The ROM writes the signature before setting $A000 to
                    // $80. Do not mistake that initialization window for a
                    // completed test with a zero result.
                    seen_running |= result == 0x80;
                    if seen_running && result < 0x80 {
                        if result == 0 {
                            return Ok(());
                        }
                        let message: Vec<_> = (0xa004..0xa400)
                            .map(|a| gb.mmu.read_byte(a))
                            .take_while(|&b| b != 0)
                            .collect();
                        return Err(format!(
                            "Blargg result {result}: {}",
                            String::from_utf8_lossy(&message)
                        ));
                    }
                }
                // Older CPU/memory ROMs use serial rather than the RAM protocol.
                let serial = String::from_utf8_lossy(gb.mmu.serial.peek_output());
                if serial.contains("Passed") {
                    return Ok(());
                }
                if serial.contains("Failed") {
                    return Err(serial.into_owned());
                }
            }
            let status: Vec<_> = (0xa000..0xa008).map(|a| gb.mmu.read_byte(a)).collect();
            let screen: String = gb.mmu.ppu.vram[0][0x1800..0x1c00]
                .chunks(32)
                .map(|row| {
                    row.iter()
                        .map(|&b| {
                            if (32..127).contains(&b) {
                                b as char
                            } else {
                                '.'
                            }
                        })
                        .collect::<String>()
                        + "\n"
                })
                .collect();
            Err(format!(
                "Blargg timeout at PC={:04X}, status={status:02X?}: {}\n{screen}",
                gb.cpu.pc,
                String::from_utf8_lossy(gb.mmu.serial.peek_output())
            ))
        }
        Check::Bully => {
            for _ in 0..600 {
                run_dots(&mut gb, FRAME)?;
                if gb
                    .mmu
                    .serial
                    .peek_sb_output()
                    .windows(13)
                    .any(|s| s == b"All tests OK!")
                {
                    return Ok(());
                }
            }
            Err(format!(
                "Bully did not pass: {}",
                String::from_utf8_lossy(gb.mmu.serial.peek_sb_output())
            ))
        }
        Check::Diagnostic(_) => unreachable!(),
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for cgb in [false, true] {
        let model = if cgb { "cgb" } else { "dmg" };
        let mut screen = |rom: String, png: String, frames| {
            cases.push(Case {
                rom,
                cgb,
                check: Check::Screenshot {
                    png,
                    dots: frames * FRAME,
                },
            })
        };
        for name in ["lycscx", "lycscy", "palettely", "scxly"] {
            let reference_model = if name.starts_with("lyc") {
                "cgb-dmg"
            } else {
                model
            };
            screen(
                format!("scribbltests/{name}/{name}.gb"),
                format!("scribbltests/{name}/{name}-{reference_model}.png"),
                30,
            );
        }
        screen(
            "scribbltests/statcount/statcount-auto.gb".into(),
            "scribbltests/statcount/statcount_auto-cgb-dmg.png".into(),
            300,
        );
        for name in ["window_y_trigger", "window_y_trigger_wx_offscreen"] {
            screen(
                format!("turtle-tests/{name}/{name}.gb"),
                format!("turtle-tests/{name}/{name}.png"),
                40,
            );
        }
        screen(
            "mbc3-tester/mbc3-tester.gb".into(),
            format!("mbc3-tester/mbc3-tester-{model}.png"),
            60,
        );
        cases.push(Case {
            rom: "scribbltests/statcount/statcount.gb".into(),
            cgb,
            check: Check::Statcount,
        });
        for (menu, (name, seconds)) in [
            ("basic-tests", 15),
            ("range-tests", 10),
            ("sub-second-writes", 28),
        ]
        .into_iter()
        .enumerate()
        {
            cases.push(Case {
                rom: "rtc3test/rtc3test.gb".into(),
                cgb,
                check: Check::Rtc {
                    menu: menu as u8,
                    seconds,
                    png: format!("rtc3test/rtc3test-{name}-{model}.png"),
                },
            });
        }
        for (rom, seconds) in [
            ("cpu_instrs/cpu_instrs.gb", 65),
            ("mem_timing/mem_timing.gb", 10),
            ("mem_timing-2/mem_timing.gb", 10),
            ("mem_timing-2/rom_singles/01-read_timing.gb", 10),
            ("mem_timing-2/rom_singles/02-write_timing.gb", 10),
            ("mem_timing-2/rom_singles/03-modify_timing.gb", 10),
        ] {
            let check = if rom.contains("rom_singles") {
                Check::Blargg { seconds }
            } else {
                let directory = rom.split('/').next().unwrap();
                let stem = if directory == "mem_timing-2" {
                    "mem_timing"
                } else {
                    directory
                };
                Check::Screenshot {
                    png: format!("blargg/{directory}/{stem}-dmg-cgb.png"),
                    dots: seconds * SECOND,
                }
            };
            cases.push(Case {
                rom: format!("blargg/{rom}"),
                cgb,
                check,
            });
        }
        cases.push(Case {
            rom: format!("blargg/{model}_sound/{model}_sound.gb"),
            cgb,
            check: Check::Screenshot {
                png: format!("blargg/{model}_sound/{model}_sound-{model}.png"),
                dots: 50 * SECOND,
            },
        });
        cases.push(Case {
            rom: "bully/bully.gb".into(),
            cgb,
            check: Check::Bully,
        });
        for (name, reason) in [
            (
                "fairylake",
                "animated graphics demo; no assertion or fixed reference frame",
            ),
            (
                "winpos",
                "interactive window-position debugger; no assertion or supplied reference image",
            ),
        ] {
            cases.push(Case {
                rom: format!("scribbltests/{name}/{name}.gb"),
                cgb,
                check: Check::Diagnostic(reason),
            });
        }
    }
    for rom in ["oam_bug.gb", "rom_singles/7-timing_effect.gb"] {
        cases.push(Case {
            rom: format!("blargg/oam_bug/{rom}"),
            cgb: false,
            check: if rom == "oam_bug.gb" {
                Check::Screenshot {
                    png: "blargg/oam_bug/oam_bug-dmg.png".into(),
                    dots: 30 * SECOND,
                }
            } else {
                Check::Blargg { seconds: 60 }
            },
        });
    }
    cases
}

fn main() {
    let args = Arguments::from_args();
    let mut ignores = BTreeMap::new();
    for line in include_str!("additional_roms_ignored.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (name, reason) = line.split_once(" # ").expect("ignore needs a reason");
        assert!(
            ignores.insert(name, reason).is_none(),
            "duplicate ignore: {name}"
        );
    }
    let trials = cases()
        .into_iter()
        .map(|case| {
            let name = case.name();
            let ignored = ignores.remove(name.as_str()).is_some();
            Trial::test(name, move || run(case).map_err(Failed::from)).with_ignored_flag(ignored)
        })
        .collect();
    assert!(ignores.is_empty(), "stale ignores: {ignores:?}");
    libtest_mimic::run(&args, trials).exit();
}
