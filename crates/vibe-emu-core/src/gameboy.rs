use crate::{apu::ApuBootSnapshot, cpu::Cpu, hardware::Model, mmu::Mmu};

#[cfg(test)]
use crate::cartridge::Cartridge;

/// CPU register snapshot for boot-handoff parity checks.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuBootSnapshot {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub pc: u16,
    pub sp: u16,
    pub ime: bool,
    pub halted: bool,
    pub stopped: bool,
    pub double_speed: bool,
}

/// End-to-end machine snapshot captured at boot-ROM handoff.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootHandoffSnapshot {
    pub model: Model,
    pub cpu: CpuBootSnapshot,
    pub boot_mapped: bool,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub dot_div: u16,
    pub timer_div: u16,
    pub timer_tima: u8,
    pub timer_tma: u8,
    pub timer_tac: u8,
    pub wram_bank: usize,
    pub wram: [[u8; 0x1000]; 8],
    pub hram: [u8; 0x7F],
    pub ppu_vram_bank: usize,
    pub ppu_vram: [[u8; 0x2000]; 2],
    pub ppu_oam: [u8; 0xA0],
    pub ppu_mode: u8,
    pub ppu_mode_clock: u16,
    pub io: [u8; 0x80],
    pub apu: ApuBootSnapshot,
}

/// High-level emulator facade representing a single Game Boy / Game Boy Color.
///
/// `GameBoy` owns the CPU and MMU and provides constructors for common initial
/// states (post-boot vs. power-on) across DMG/CGB modes and hardware revisions.
pub struct GameBoy {
    /// CPU core.
    pub cpu: Cpu,
    /// Memory map and attached devices (PPU/APU/timer/cartridge/etc).
    pub mmu: Mmu,
    /// Hardware model and revision of the emulated system.
    pub model: Model,
}

impl GameBoy {
    /// Captures a machine snapshot used to compare boot-ROM handoff parity.
    #[doc(hidden)]
    pub fn capture_boot_handoff_snapshot(&mut self) -> BootHandoffSnapshot {
        let io = self.mmu.debug_io_snapshot();

        BootHandoffSnapshot {
            model: self.model,
            cpu: CpuBootSnapshot {
                a: self.cpu.a,
                f: self.cpu.f,
                b: self.cpu.b,
                c: self.cpu.c,
                d: self.cpu.d,
                e: self.cpu.e,
                h: self.cpu.h,
                l: self.cpu.l,
                pc: self.cpu.pc,
                sp: self.cpu.sp,
                ime: self.cpu.ime,
                halted: self.cpu.halted,
                stopped: self.cpu.stopped,
                double_speed: self.cpu.double_speed,
            },
            boot_mapped: self.mmu.boot_mapped,
            if_reg: self.mmu.if_reg,
            ie_reg: self.mmu.ie_reg,
            dot_div: self.mmu.dot_div,
            timer_div: self.mmu.timer.div,
            timer_tima: self.mmu.timer.tima,
            timer_tma: self.mmu.timer.tma,
            timer_tac: self.mmu.timer.tac,
            wram_bank: self.mmu.wram_bank,
            wram: self.mmu.wram,
            hram: self.mmu.hram,
            ppu_vram_bank: self.mmu.ppu.vram_bank,
            ppu_vram: self.mmu.ppu.vram,
            ppu_oam: self.mmu.ppu.oam,
            ppu_mode: self.mmu.ppu.mode(),
            ppu_mode_clock: self.mmu.ppu.mode_clock(),
            io,
            apu: self.mmu.apu.debug_boot_snapshot(),
        }
    }

    /// Creates a machine in the post-boot state for the given hardware model.
    ///
    /// # Examples
    ///
    /// ```
    /// use vibe_emu_core::gameboy::GameBoy;
    /// use vibe_emu_core::hardware::{Model, DmgRevision, CgbRevision};
    ///
    /// let dmg = GameBoy::new(Model::Dmg(DmgRevision::default()));
    /// let cgb = GameBoy::new(Model::Cgb(CgbRevision::default()));
    /// assert!(dmg.model.is_dmg());
    /// assert!(cgb.model.is_cgb());
    /// ```
    pub fn new(model: Model) -> Self {
        Self {
            cpu: Cpu::new(model),
            mmu: Mmu::new(model),
            model,
        }
    }

    /// Creates a machine initialized to an approximate power-on state.
    ///
    /// This is intended for executing a boot ROM. If you are skipping the boot
    /// ROM, prefer [`Self::new`].
    pub fn new_power_on(model: Model) -> Self {
        Self {
            cpu: Cpu::new_power_on(),
            mmu: Mmu::new_power_on(model),
            model,
        }
    }

    /// Resets to the post-boot state, preserving cartridge and boot ROM.
    ///
    /// # Examples
    ///
    /// ```
    /// use vibe_emu_core::gameboy::GameBoy;
    /// use vibe_emu_core::cartridge::Cartridge;
    /// use vibe_emu_core::hardware::Model;
    ///
    /// let mut gb = GameBoy::new(Model::default());
    /// gb.mmu.load_cart(Cartridge::from_bytes(vec![0u8; 0x8000]));
    /// gb.cpu.step(&mut gb.mmu);
    /// let pc_before = gb.cpu.pc;
    ///
    /// gb.reset();
    ///
    /// assert_eq!(gb.cpu.pc, 0x0100); // PC restored to post-boot entry point
    /// assert!(gb.mmu.cart.is_some()); // cartridge preserved
    /// ```
    pub fn reset(&mut self) {
        let cart = self.mmu.cart.take();
        let boot = self.mmu.boot_rom.take();
        self.cpu = Cpu::new(self.model);
        self.mmu.reset_post_boot_in_place(self.model);
        if let Some(c) = cart {
            self.mmu.load_cart(c);
        }
        if let Some(b) = boot {
            self.mmu.boot_rom = Some(b);
            self.mmu.boot_mapped = false;
        }
    }

    /// Resets to the power-on state, preserving cartridge and boot ROM.
    ///
    /// This is useful when you want to re-run the boot ROM sequence.
    pub fn reset_power_on(&mut self) {
        let cart = self.mmu.cart.take();
        let boot = self.mmu.boot_rom.take();
        self.cpu = Cpu::new_power_on();
        self.mmu.reset_power_on_in_place(self.model);
        if let Some(c) = cart {
            self.mmu.load_cart(c);
        }
        if let Some(b) = boot {
            self.mmu.load_boot_rom(b);
        }
    }
}

impl Default for GameBoy {
    fn default() -> Self {
        Self::new(Model::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{CgbRevision, DmgRevision};

    fn dummy_rom(cgb: bool) -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        rom[0x0143] = if cgb { 0x80 } else { 0x00 };
        rom
    }

    #[test]
    fn reset_preserves_cart_and_boot_rom() {
        let mut gb = GameBoy::new(Model::Cgb(CgbRevision::RevC));
        gb.mmu
            .load_cart(Cartridge::from_bytes_with_ram(dummy_rom(true), 0));
        gb.mmu.load_boot_rom(vec![0xEA; 0x900]);

        gb.reset();

        assert!(gb.mmu.cart.is_some());
        assert!(gb.mmu.boot_rom.is_some());
        assert!(!gb.mmu.boot_mapped);
        assert_eq!(gb.cpu.pc, 0x0100);
        assert!(gb.mmu.is_cgb());
    }

    #[test]
    fn reset_power_on_preserves_cart_and_boot_rom() {
        let mut gb = GameBoy::new(Model::Dmg(DmgRevision::RevA));
        gb.mmu
            .load_cart(Cartridge::from_bytes_with_ram(dummy_rom(false), 0));
        gb.mmu.load_boot_rom(vec![0x00; 0x100]);

        gb.reset_power_on();

        assert!(gb.mmu.cart.is_some());
        assert!(gb.mmu.boot_rom.is_some());
        assert_eq!(gb.cpu.pc, 0x0000);
        assert!(!gb.mmu.is_cgb());
    }
}
