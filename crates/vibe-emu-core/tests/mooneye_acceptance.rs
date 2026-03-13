#![allow(non_snake_case)]
mod common;
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision, Model},
};
const FIB_SEQ: [u8; 6] = [3, 5, 8, 13, 21, 34];
const FAIL_SEQ: [u8; 6] = [0x42; 6];

#[allow(dead_code)]
fn capture_first_div_reads_cgb_seed(seed_div: u16) -> Vec<u8> {
    let rom = std::fs::read(common::rom_path(
        "mooneye-test-suite/misc/boot_div-cgbABCDE.gb",
    ))
    .expect("rom not found");
    let cart = Cartridge::from_bytes(rom);
    assert!(cart.cgb, "expected CGB ROM");

    let mut gb = GameBoy::new(Model::Cgb(CgbRevision::default()));
    gb.mmu.load_cart(cart);

    gb.mmu.timer.div = seed_div;
    gb.mmu.dot_div = seed_div;

    let mut div_reads: Vec<u8> = Vec::new();
    while gb.cpu.cycles < 200_000 {
        let pc = gb.cpu.pc;
        let opcode = gb.mmu.read_byte(pc);
        if opcode == 0xF0 {
            let imm = gb.mmu.read_byte(pc.wrapping_add(1));
            if imm == 0x04 {
                gb.cpu.step(&mut gb.mmu);
                div_reads.push(gb.cpu.a);
                if div_reads.len() == 6 {
                    return div_reads;
                }
                continue;
            }
        }
        gb.cpu.step(&mut gb.mmu);
    }
    div_reads
}

fn run_mooneye_quit_protocol<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> bool {
    run_mooneye_quit_protocol_with_dmg_revision(rom_path, max_cycles, DmgRevision::default())
}

fn run_mooneye_quit_protocol_with_dmg_revision<P: AsRef<std::path::Path>>(
    rom_path: P,
    max_cycles: u64,
    dmg_revision: DmgRevision,
) -> bool {
    let rom = std::fs::read(&rom_path).expect("rom not found");
    let cart = Cartridge::from_bytes(rom);
    let model = if cart.cgb {
        Model::Cgb(CgbRevision::default())
    } else {
        Model::Dmg(dmg_revision)
    };
    let mut gb = GameBoy::new(model);
    gb.mmu.load_cart(cart);

    while gb.cpu.cycles < max_cycles {
        let pc = gb.cpu.pc;
        let opcode = gb.mmu.read_byte(pc);

        // Mooneye quit protocol: a pass/fail is signaled by executing LD B,B (0x40)
        // with registers B/C/D/E/H/L containing either Fibonacci numbers or 0x42.
        if opcode == 0x40 {
            let regs = [gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l];
            if regs == FIB_SEQ {
                return true;
            }
            if regs == FAIL_SEQ {
                println!("mooneye quit protocol failed at pc={:04X}", pc);
                println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
                println!("serial output (partial): {:?}", gb.mmu.serial.peek_output());
                return false;
            }
        }

        gb.cpu.step(&mut gb.mmu);
    }

    println!("mooneye quit protocol: timeout");
    println!(
        "pc={:04X} opcode={:02X}",
        gb.cpu.pc,
        gb.mmu.read_byte(gb.cpu.pc)
    );
    println!(
        "af={:02X}{:02X} bc={:02X}{:02X} de={:02X}{:02X} hl={:02X}{:02X} sp={:04X}",
        gb.cpu.a, gb.cpu.f, gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l, gb.cpu.sp
    );
    println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
    println!("serial output: {:?}", gb.mmu.serial.take_output());
    false
}

fn run_mooneye_acceptance_with_bootrom<P: AsRef<std::path::Path>>(
    rom_path: P,
    max_cycles: u64,
    dmg_revision: DmgRevision,
) -> bool {
    let rom = std::fs::read(&rom_path).expect("rom not found");
    let cart = Cartridge::from_bytes(rom);
    assert!(!cart.cgb, "expected DMG ROM");

    let boot = std::fs::read(common::dmg_boot_rom_path()).expect("boot ROM not found");

    let mut gb = GameBoy::new_power_on(Model::Dmg(dmg_revision));
    gb.mmu.load_boot_rom(boot);
    gb.mmu.load_cart(cart);

    while gb.cpu.cycles < max_cycles {
        let pc = gb.cpu.pc;
        let opcode = gb.mmu.read_byte(pc);

        if opcode == 0x40 {
            let regs = [gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l];
            if regs == FIB_SEQ {
                return true;
            }
            if regs == FAIL_SEQ {
                println!("mooneye quit protocol failed at pc={:04X}", pc);
                println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
                println!("serial output (partial): {:?}", gb.mmu.serial.peek_output());
                return false;
            }
        }

        gb.cpu.step(&mut gb.mmu);
    }

    println!("mooneye quit protocol: timeout");
    println!(
        "pc={:04X} opcode={:02X}",
        gb.cpu.pc,
        gb.mmu.read_byte(gb.cpu.pc)
    );
    println!(
        "af={:02X}{:02X} bc={:02X}{:02X} de={:02X}{:02X} hl={:02X}{:02X} sp={:04X}",
        gb.cpu.a, gb.cpu.f, gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l, gb.cpu.sp
    );
    println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
    println!("serial output: {:?}", gb.mmu.serial.take_output());
    false
}
fn run_mooneye_acceptance<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> bool {
    run_mooneye_acceptance_with_dmg_revision(rom_path, max_cycles, DmgRevision::default())
}

fn run_mooneye_acceptance_with_dmg_revision<P: AsRef<std::path::Path>>(
    rom_path: P,
    max_cycles: u64,
    dmg_revision: DmgRevision,
) -> bool {
    let rom = std::fs::read(&rom_path).expect("rom not found");
    let cart = Cartridge::from_bytes(rom);
    let model = if cart.cgb {
        Model::Cgb(CgbRevision::default())
    } else {
        Model::Dmg(dmg_revision)
    };
    let mut gb = GameBoy::new(model);
    gb.mmu.load_cart(cart);

    // Optional targeted trace: capture the first few reads of DIV (FF04) so
    // boot DIV/phase issues are easier to diagnose.
    let trace_div = rom_path
        .as_ref()
        .to_string_lossy()
        .replace('\\', "/")
        .ends_with("mooneye-test-suite/misc/boot_div-cgbABCDE.gb");
    let mut div_reads: Vec<u8> = Vec::new();
    while gb.cpu.cycles < max_cycles {
        let pc = gb.cpu.pc;
        let opcode = gb.mmu.read_byte(pc);

        if trace_div && opcode == 0xF0 {
            let imm = gb.mmu.read_byte(pc.wrapping_add(1));
            if imm == 0x04 {
                gb.cpu.step(&mut gb.mmu);
                div_reads.push(gb.cpu.a);
                continue;
            }
        }

        if opcode == 0x40 {
            let regs = [gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l];
            if regs == FIB_SEQ {
                return true;
            }
            if regs == FAIL_SEQ {
                println!("mooneye quit protocol failed at pc={:04X}", pc);
                if trace_div {
                    println!(
                        "captured DIV reads (LDH A,(FF04)): {:?} (len={})",
                        div_reads,
                        div_reads.len()
                    );
                    println!(
                        "initial timer.div high byte estimate now={:02X}",
                        gb.mmu.timer.read(0xFF04)
                    );
                }
                println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
                println!("serial output (partial): {:?}", gb.mmu.serial.peek_output());
                return false;
            }
        }

        gb.cpu.step(&mut gb.mmu);

        if gb.mmu.serial.peek_output().ends_with(&FIB_SEQ) {
            break;
        }
    }

    let out = gb.mmu.serial.take_output();
    let success = out.windows(FIB_SEQ.len()).any(|window| window == FIB_SEQ);
    if !success {
        println!("serial output: {:?}", out);
        println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
    }
    success
}

fn run_mooneye_acceptance_force_cgb_revision<P: AsRef<std::path::Path>>(
    rom_path: P,
    max_cycles: u64,
    cgb_revision: CgbRevision,
) -> bool {
    let rom = std::fs::read(&rom_path).expect("rom not found");
    let cart = Cartridge::from_bytes(rom);
    let mut gb = GameBoy::new(Model::Cgb(cgb_revision));
    gb.mmu.load_cart(cart);

    while gb.cpu.cycles < max_cycles {
        let pc = gb.cpu.pc;
        let opcode = gb.mmu.read_byte(pc);

        if opcode == 0x40 {
            let regs = [gb.cpu.b, gb.cpu.c, gb.cpu.d, gb.cpu.e, gb.cpu.h, gb.cpu.l];
            if regs == FIB_SEQ {
                return true;
            }
            if regs == FAIL_SEQ {
                println!("mooneye quit protocol failed at pc={:04X}", pc);
                println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
                println!("serial output (partial): {:?}", gb.mmu.serial.peek_output());
                return false;
            }
        }

        gb.cpu.step(&mut gb.mmu);

        if gb.mmu.serial.peek_output().ends_with(&FIB_SEQ) {
            break;
        }
    }

    let out = gb.mmu.serial.take_output();
    let success = out.windows(FIB_SEQ.len()).any(|window| window == FIB_SEQ);
    if !success {
        println!("serial output: {:?}", out);
        println!("hram (first 40 bytes): {:?}", &gb.mmu.hram[..40]);
    }
    success
}

#[test]
fn add_sp_e_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/add_sp_e_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn bits__mem_oam_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/bits/mem_oam.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn bits__reg_f_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/bits/reg_f.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn bits__unused_hwio_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/bits/unused_hwio-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn bits__unused_hwio_GS_gb__bootrom() {
    let passed = run_mooneye_acceptance_with_bootrom(
        common::rom_path("mooneye-test-suite/acceptance/bits/unused_hwio-GS.gb"),
        50_000_000,
        DmgRevision::default(),
    );
    assert!(passed, "test failed (with DMG boot ROM)");
}

#[test]
fn emulator_only__mbc1__bits_bank2_gb() {
    let passed = run_mooneye_quit_protocol(
        common::rom_path("mooneye-test-suite/emulator-only/mbc1/bits_bank2.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn emulator_only__mbc1__multicart_rom_8Mb_gb() {
    let passed = run_mooneye_quit_protocol(
        common::rom_path("mooneye-test-suite/emulator-only/mbc1/multicart_rom_8Mb.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn emulator_only__mbc1__ram_64kb_gb() {
    let passed = run_mooneye_quit_protocol(
        common::rom_path("mooneye-test-suite/emulator-only/mbc1/ram_64kb.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn emulator_only__mbc2__bits_ramg_gb() {
    let passed = run_mooneye_quit_protocol(
        common::rom_path("mooneye-test-suite/emulator-only/mbc2/bits_ramg.gb"),
        60_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn emulator_only__mbc2__bits_romb_gb() {
    let passed = run_mooneye_quit_protocol(
        common::rom_path("mooneye-test-suite/emulator-only/mbc2/bits_romb.gb"),
        60_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn emulator_only__mbc2__rom_1Mb_gb() {
    let passed = run_mooneye_quit_protocol(
        common::rom_path("mooneye-test-suite/emulator-only/mbc2/rom_1Mb.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn emulator_only__mbc5__rom_1Mb_gb() {
    let passed = run_mooneye_quit_protocol(
        common::rom_path("mooneye-test-suite/emulator-only/mbc5/rom_1Mb.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn boot_div_S_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_div-S.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn boot_div_dmg0_gb() {
    let passed = run_mooneye_acceptance_with_dmg_revision(
        common::rom_path("mooneye-test-suite/acceptance/boot_div-dmg0.gb"),
        20_000_000,
        DmgRevision::Rev0,
    );
    assert!(passed, "test failed");
}

#[test]
fn boot_div_dmgABCmgb_gb() {
    let passed = run_mooneye_acceptance_with_dmg_revision(
        common::rom_path("mooneye-test-suite/acceptance/boot_div-dmgABCmgb.gb"),
        20_000_000,
        DmgRevision::RevC,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn boot_div2_S_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_div2-S.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn boot_hwio_S_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_hwio-S.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn boot_hwio_dmg0_gb() {
    let passed = run_mooneye_acceptance_with_dmg_revision(
        common::rom_path("mooneye-test-suite/acceptance/boot_hwio-dmg0.gb"),
        20_000_000,
        DmgRevision::Rev0,
    );
    assert!(passed, "test failed");
}

#[test]
fn boot_hwio_dmgABCmgb_gb() {
    let passed = run_mooneye_acceptance_with_dmg_revision(
        common::rom_path("mooneye-test-suite/acceptance/boot_hwio-dmgABCmgb.gb"),
        20_000_000,
        DmgRevision::RevC,
    );
    assert!(passed, "test failed");
}

#[test]
fn misc__boot_div_cgbABCDE_gb() {
    let passed = run_mooneye_acceptance_force_cgb_revision(
        common::rom_path("mooneye-test-suite/misc/boot_div-cgbABCDE.gb"),
        20_000_000,
        CgbRevision::RevE,
    );
    assert!(passed, "test failed");
}

#[test]
fn misc__boot_div_cgb0_gb() {
    let passed = run_mooneye_acceptance_force_cgb_revision(
        common::rom_path("mooneye-test-suite/misc/boot_div-cgb0.gb"),
        20_000_000,
        CgbRevision::Rev0,
    );
    assert!(passed, "test failed");
}

#[test]
fn boot_regs_dmg0_gb() {
    let passed = run_mooneye_acceptance_with_dmg_revision(
        common::rom_path("mooneye-test-suite/acceptance/boot_regs-dmg0.gb"),
        20_000_000,
        DmgRevision::Rev0,
    );
    assert!(passed, "test failed");
}

#[test]
fn boot_regs_dmgABC_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_regs-dmgABC.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn boot_regs_mgb_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_regs-mgb.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn boot_regs_sgb_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_regs-sgb.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn boot_regs_sgb2_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_regs-sgb2.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn call_cc_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/call_cc_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn call_cc_timing2_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/call_cc_timing2.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn call_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/call_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn call_timing2_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/call_timing2.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn di_timing_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/di_timing-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn div_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/div_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ei_sequence_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ei_sequence.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ei_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ei_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn halt_ime0_ei_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/halt_ime0_ei.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn halt_ime0_nointr_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/halt_ime0_nointr_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn halt_ime1_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/halt_ime1_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn halt_ime1_timing2_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/halt_ime1_timing2-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn if_ie_registers_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/if_ie_registers.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn instr__daa_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/instr/daa.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn interrupts__ie_push_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/interrupts/ie_push.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn intr_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/intr_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn jp_cc_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/jp_cc_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn jp_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/jp_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ld_hl_sp_e_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ld_hl_sp_e_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn oam_dma__basic_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/oam_dma/basic.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn oam_dma__reg_read_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/oam_dma/reg_read.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn oam_dma__sources_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/oam_dma/sources-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn oam_dma_restart_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/oam_dma_restart.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn oam_dma_start_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/oam_dma_start.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn oam_dma_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/oam_dma_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn pop_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/pop_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__hblank_ly_scx_timing_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/hblank_ly_scx_timing-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__intr_1_2_timing_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/intr_1_2_timing-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__intr_2_0_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/intr_2_0_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__intr_2_mode0_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/intr_2_mode0_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__intr_2_mode0_timing_sprites_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/intr_2_mode0_timing_sprites.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__intr_2_mode3_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/intr_2_mode3_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__intr_2_oam_ok_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/intr_2_oam_ok_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__lcdon_timing_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/lcdon_timing-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__lcdon_write_timing_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/lcdon_write_timing-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__stat_irq_blocking_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/stat_irq_blocking.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__stat_lyc_onoff_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/stat_lyc_onoff.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ppu__vblank_stat_intr_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/vblank_stat_intr-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn push_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/push_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn rapid_di_ei_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/rapid_di_ei.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ret_cc_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ret_cc_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn ret_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ret_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn reti_intr_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/reti_intr_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn reti_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/reti_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn rst_timing_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/rst_timing.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn serial__boot_sclk_align_dmgABCmgb_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/serial/boot_sclk_align-dmgABCmgb.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__div_write_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/div_write.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__rapid_toggle_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/rapid_toggle.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim00_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim00.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim00_div_trigger_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim00_div_trigger.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim01_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim01.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim01_div_trigger_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim01_div_trigger.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim10_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim10.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim10_div_trigger_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim10_div_trigger.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim11_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim11.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tim11_div_trigger_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tim11_div_trigger.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tima_reload_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tima_reload.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tima_write_reloading_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tima_write_reloading.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
fn timer__tma_write_reloading_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tma_write_reloading.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}
