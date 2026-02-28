mod common;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy};

const NINTENDO_LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

fn build_minimal_bootable_rom(cgb: bool) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];

    rom[0x0100] = 0x00;
    rom[0x0101] = 0xC3;
    rom[0x0102] = 0x00;
    rom[0x0103] = 0x01;

    rom[0x0104..0x0134].copy_from_slice(&NINTENDO_LOGO);

    let title = if cgb {
        b"BOOTSTATE-CGB"
    } else {
        b"BOOTSTATE-DMG"
    };
    rom[0x0134..0x0134 + title.len()].copy_from_slice(title);

    rom[0x0143] = if cgb { 0x80 } else { 0x00 };
    rom[0x0144] = 0x30;
    rom[0x0145] = 0x31;
    rom[0x0146] = 0x00;
    rom[0x0147] = 0x00;
    rom[0x0148] = 0x00;
    rom[0x0149] = 0x00;
    rom[0x014A] = 0x01;
    rom[0x014B] = 0x33;
    rom[0x014C] = 0x00;

    let mut header_checksum = 0u8;
    for byte in &rom[0x0134..=0x014C] {
        header_checksum = header_checksum.wrapping_sub(*byte).wrapping_sub(1);
    }
    rom[0x014D] = header_checksum;

    rom
}

fn capture_bootrom_handoff_snapshot(cgb: bool) -> vibe_emu_core::gameboy::BootHandoffSnapshot {
    let boot_rom_path = if cgb {
        common::cgb_boot_rom_path()
    } else {
        common::dmg_boot_rom_path()
    };
    let boot_rom = std::fs::read(boot_rom_path).expect("failed to read boot ROM");

    let mut gb = GameBoy::new_power_on_with_revisions(cgb, Default::default(), Default::default());
    gb.mmu.load_boot_rom(boot_rom);
    gb.mmu.load_cart(Cartridge::from_bytes_with_ram(
        build_minimal_bootable_rom(cgb),
        0,
    ));

    let max_steps = 20_000_000usize;
    for _ in 0..max_steps {
        if !gb.mmu.boot_mapped {
            return gb.capture_boot_handoff_snapshot();
        }
        gb.cpu.step(&mut gb.mmu);
    }

    panic!(
        "boot ROM did not hand off within step limit (pc={:04X}, a={:02X}, f={:02X}, div={:04X})",
        gb.cpu.pc, gb.cpu.a, gb.cpu.f, gb.mmu.timer.div
    );
}

fn capture_no_boot_snapshot(cgb: bool) -> vibe_emu_core::gameboy::BootHandoffSnapshot {
    let mut gb = GameBoy::new_with_mode(cgb);
    gb.mmu.load_cart(Cartridge::from_bytes_with_ram(
        build_minimal_bootable_rom(cgb),
        0,
    ));
    gb.capture_boot_handoff_snapshot()
}

fn first_diff_indices<const N: usize>(left: &[u8; N], right: &[u8; N], limit: usize) -> Vec<usize> {
    left.iter()
        .zip(right.iter())
        .enumerate()
        .filter_map(|(index, (l, r))| if l == r { None } else { Some(index) })
        .take(limit)
        .collect()
}

fn first_diff_details<const N: usize>(
    left: &[u8; N],
    right: &[u8; N],
    limit: usize,
) -> Vec<(usize, u8, u8)> {
    left.iter()
        .zip(right.iter())
        .enumerate()
        .filter_map(
            |(index, (l, r))| {
                if l == r { None } else { Some((index, *l, *r)) }
            },
        )
        .take(limit)
        .collect()
}

fn count_diff_slices(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right.iter())
        .filter(|(l, r)| l != r)
        .count()
}

fn assert_snapshot_eq(
    expected: &vibe_emu_core::gameboy::BootHandoffSnapshot,
    actual: &vibe_emu_core::gameboy::BootHandoffSnapshot,
) {
    let mut mismatches = Vec::new();

    if expected.cpu != actual.cpu {
        mismatches.push(format!(
            "cpu: expected {:?}, got {:?}",
            expected.cpu, actual.cpu
        ));
    }
    if expected.boot_mapped != actual.boot_mapped {
        mismatches.push(format!(
            "boot_mapped: expected {}, got {}",
            expected.boot_mapped, actual.boot_mapped
        ));
    }
    if expected.if_reg != actual.if_reg {
        mismatches.push(format!(
            "if_reg: expected {:02X}, got {:02X}",
            expected.if_reg, actual.if_reg
        ));
    }
    if expected.ie_reg != actual.ie_reg {
        mismatches.push(format!(
            "ie_reg: expected {:02X}, got {:02X}",
            expected.ie_reg, actual.ie_reg
        ));
    }
    // timer_div, dot_div, ppu_mode, and ppu_mode_clock are excluded from
    // strict comparison. The skip-boot initializer sets these to
    // mooneye-verified values tuned for what the first game instruction
    // should observe (with boot_hold_cycles freezing the PPU). These
    // differ from the exact handoff-moment values captured here because
    // the boot ROM's last instruction advances the timer/PPU before this
    // snapshot is taken.
    if expected.timer_tima != actual.timer_tima {
        mismatches.push(format!(
            "tima: expected {:02X}, got {:02X}",
            expected.timer_tima, actual.timer_tima
        ));
    }
    if expected.timer_tma != actual.timer_tma {
        mismatches.push(format!(
            "tma: expected {:02X}, got {:02X}",
            expected.timer_tma, actual.timer_tma
        ));
    }
    if expected.timer_tac != actual.timer_tac {
        mismatches.push(format!(
            "tac: expected {:02X}, got {:02X}",
            expected.timer_tac, actual.timer_tac
        ));
    }
    if expected.wram_bank != actual.wram_bank {
        mismatches.push(format!(
            "wram_bank: expected {}, got {}",
            expected.wram_bank, actual.wram_bank
        ));
    }
    if expected.ppu_vram_bank != actual.ppu_vram_bank {
        mismatches.push(format!(
            "ppu_vram_bank: expected {}, got {}",
            expected.ppu_vram_bank, actual.ppu_vram_bank
        ));
    }

    for (bank, (expected_bank, actual_bank)) in
        expected.wram.iter().zip(actual.wram.iter()).enumerate()
    {
        let diff_count = count_diff_slices(expected_bank, actual_bank);
        if diff_count != 0 {
            let first = first_diff_indices(expected_bank, actual_bank, 8);
            let details = first_diff_details(expected_bank, actual_bank, 8);
            mismatches.push(format!(
                "wram bank {bank}: {diff_count} byte diffs, first indices {first:?}, first details {details:?}"
            ));
        }
    }

    let hram_diffs = count_diff_slices(&expected.hram, &actual.hram);
    if hram_diffs != 0 {
        let first = first_diff_indices(&expected.hram, &actual.hram, 16);
        let details = first_diff_details(&expected.hram, &actual.hram, 16);
        mismatches.push(format!(
            "hram: {hram_diffs} byte diffs, first indices {first:?}, first details {details:?}"
        ));
    }

    for (bank, (expected_bank, actual_bank)) in expected
        .ppu_vram
        .iter()
        .zip(actual.ppu_vram.iter())
        .enumerate()
    {
        let diff_count = count_diff_slices(expected_bank, actual_bank);
        if diff_count != 0 {
            let first = first_diff_indices(expected_bank, actual_bank, 8);
            let details = first_diff_details(expected_bank, actual_bank, 8);
            mismatches.push(format!(
                "ppu vram bank {bank}: {diff_count} byte diffs, first indices {first:?}, first details {details:?}"
            ));
        }
    }

    let oam_diffs = count_diff_slices(&expected.ppu_oam, &actual.ppu_oam);
    if oam_diffs != 0 {
        let first = first_diff_indices(&expected.ppu_oam, &actual.ppu_oam, 8);
        let details = first_diff_details(&expected.ppu_oam, &actual.ppu_oam, 8);
        mismatches.push(format!(
            "ppu oam: {oam_diffs} byte diffs, first indices {first:?}, first details {details:?}"
        ));
    }

    // IO comparison excludes timing-dependent registers whose skip-boot
    // values are deliberately set to mooneye-verified values rather than
    // exact handoff values:
    //   FF04 (DIV)  = index 0x04
    //   FF41 (STAT) = index 0x41
    //   FF44 (LY)   = index 0x44
    const TIMING_IO_INDICES: &[usize] = &[0x04, 0x41, 0x44];
    let io_diffs: usize = expected
        .io
        .iter()
        .zip(actual.io.iter())
        .enumerate()
        .filter(|(i, (l, r))| !TIMING_IO_INDICES.contains(i) && l != r)
        .count();
    if io_diffs != 0 {
        let details: Vec<_> = expected
            .io
            .iter()
            .zip(actual.io.iter())
            .enumerate()
            .filter(|(i, (l, r))| !TIMING_IO_INDICES.contains(i) && l != r)
            .map(|(i, (l, r))| (i, *l, *r))
            .take(16)
            .collect();
        mismatches.push(format!(
            "io: {io_diffs} byte diffs (excluding timing regs), details {details:?}"
        ));
    }

    if expected.apu != actual.apu {
        mismatches.push(format!(
            "apu snapshot mismatch: expected {:?}, got {:?}",
            expected.apu, actual.apu
        ));
    }

    assert!(
        mismatches.is_empty(),
        "boot handoff mismatch ({} differences):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[test]
fn dmg_no_boot_matches_bootrom_handoff() {
    let bootrom_handoff = capture_bootrom_handoff_snapshot(false);
    let no_boot = capture_no_boot_snapshot(false);

    assert_snapshot_eq(&bootrom_handoff, &no_boot);
}

#[test]
fn cgb_no_boot_matches_bootrom_handoff() {
    let bootrom_handoff = capture_bootrom_handoff_snapshot(true);
    let no_boot = capture_no_boot_snapshot(true);

    assert_snapshot_eq(&bootrom_handoff, &no_boot);
}
