#![allow(non_snake_case)]
mod common;
use vibeEmu::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision},
};
const FIB_SEQ: [u8; 6] = [3, 5, 8, 13, 21, 34];
fn run_mooneye_acceptance<P: AsRef<std::path::Path>>(rom_path: P, max_cycles: u64) -> bool {
    run_mooneye_acceptance_with_dmg_revision(rom_path, max_cycles, DmgRevision::default())
}

fn run_mooneye_acceptance_with_dmg_revision<P: AsRef<std::path::Path>>(
    rom_path: P,
    max_cycles: u64,
    dmg_revision: DmgRevision,
) -> bool {
    let rom = std::fs::read(&rom_path).expect("rom not found");
    let cart = Cartridge::load(rom);
    let mut gb = GameBoy::new_with_revisions(cart.cgb, dmg_revision, CgbRevision::default());
    gb.mmu.load_cart(cart);
    while gb.cpu.cycles < max_cycles {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.serial.peek_output().len() >= 6 {
            break;
        }
    }
    let out = gb.mmu.serial.take_output();
    out.len() >= 6 && out[0..6] == FIB_SEQ
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
#[ignore]
fn bits__unused_hwio_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/bits/unused_hwio-GS.gb"),
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
#[ignore]
fn boot_div_dmg0_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_div-dmg0.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn boot_div_dmgABCmgb_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/boot_div-dmgABCmgb.gb"),
        20_000_000,
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
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
fn ppu__lcdon_timing_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/lcdon_timing-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn ppu__lcdon_write_timing_GS_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/lcdon_write_timing-GS.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn ppu__stat_irq_blocking_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/stat_irq_blocking.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn ppu__stat_lyc_onoff_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/ppu/stat_lyc_onoff.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
fn timer__tima_write_reloading_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tima_write_reloading.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}

#[test]
#[ignore]
fn timer__tma_write_reloading_gb() {
    let passed = run_mooneye_acceptance(
        common::rom_path("mooneye-test-suite/acceptance/timer/tma_write_reloading.gb"),
        20_000_000,
    );
    assert!(passed, "test failed");
}
