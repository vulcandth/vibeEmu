mod common;
#[path = "common/mooneye.rs"]
mod suite;

fn main() {
    // These ROMs already have regression tests in mooneye_acceptance.rs.
    suite::main(
        false,
        include_str!("mooneye_extended_ignored.txt"),
        &[
            "emulator-only/mbc1/bits_bank2.gb",
            "emulator-only/mbc1/multicart_rom_8Mb.gb",
            "emulator-only/mbc1/ram_64kb.gb",
            "emulator-only/mbc2/bits_ramg.gb",
            "emulator-only/mbc2/bits_romb.gb",
            "emulator-only/mbc2/rom_1Mb.gb",
            "emulator-only/mbc5/rom_1Mb.gb",
            "misc/boot_div-cgb0.gb",
            "misc/boot_div-cgbABCDE.gb",
        ],
    );
}
