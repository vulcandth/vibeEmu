use crate::{
    apu::ApuBootSnapshot,
    cpu::Cpu,
    hardware::{CgbRevision, DmgRevision},
    mmu::Mmu,
};

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
    pub cgb: bool,
    pub dmg_revision: DmgRevision,
    pub cgb_revision: CgbRevision,
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
    /// Whether the machine is running in CGB mode.
    pub cgb: bool,
    /// DMG CPU/board revision used for revision-specific quirks.
    pub dmg_revision: DmgRevision,
    /// CGB revision used for revision-specific quirks.
    pub cgb_revision: CgbRevision,
}

impl GameBoy {
    /// Captures a machine snapshot used to compare boot-ROM handoff parity.
    #[doc(hidden)]
    pub fn capture_boot_handoff_snapshot(&mut self) -> BootHandoffSnapshot {
        let io = self.mmu.debug_io_snapshot();

        BootHandoffSnapshot {
            cgb: self.cgb,
            dmg_revision: self.dmg_revision,
            cgb_revision: self.cgb_revision,
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

    /// Creates a DMG-mode machine in the post-boot state.
    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    /// Creates a machine in the post-boot state.
    ///
    /// When `cgb` is `true`, the machine runs in CGB mode with default revisions.
    pub fn new_with_mode(cgb: bool) -> Self {
        Self::new_with_revisions(cgb, DmgRevision::default(), CgbRevision::default())
    }

    /// Creates a machine in the post-boot state with an explicit CGB revision.
    pub fn new_with_revision(cgb: bool, revision: CgbRevision) -> Self {
        Self::new_with_revisions(cgb, DmgRevision::default(), revision)
    }

    /// Creates a machine in the post-boot state with explicit DMG + CGB revisions.
    pub fn new_with_revisions(
        cgb: bool,
        dmg_revision: DmgRevision,
        cgb_revision: CgbRevision,
    ) -> Self {
        Self {
            cpu: Cpu::new_with_mode_and_revision(cgb, dmg_revision),
            mmu: Mmu::new_with_revisions(cgb, dmg_revision, cgb_revision),
            cgb,
            dmg_revision,
            cgb_revision,
        }
    }

    /// Creates a machine initialized to an approximate power-on state.
    ///
    /// This is intended for executing a boot ROM. If you are skipping the boot
    /// ROM, prefer [`Self::new_with_revisions`].
    pub fn new_power_on_with_revisions(
        cgb: bool,
        dmg_revision: DmgRevision,
        cgb_revision: CgbRevision,
    ) -> Self {
        Self {
            cpu: Cpu::new_power_on_with_revision(cgb, dmg_revision),
            mmu: Mmu::new_power_on_with_revisions(cgb, dmg_revision, cgb_revision),
            cgb,
            dmg_revision,
            cgb_revision,
        }
    }

    /// Creates a power-on machine with an explicit CGB revision.
    pub fn new_power_on_with_revision(cgb: bool, revision: CgbRevision) -> Self {
        Self::new_power_on_with_revisions(cgb, DmgRevision::default(), revision)
    }

    /// Resets to the post-boot state, preserving cartridge and boot ROM.
    pub fn reset(&mut self) {
        let cart = self.mmu.cart.take();
        let boot = self.mmu.boot_rom.take();
        self.cpu = Cpu::new_with_mode_and_revision(self.cgb, self.dmg_revision);
        self.mmu = Mmu::new_with_revisions(self.cgb, self.dmg_revision, self.cgb_revision);
        if let Some(c) = cart {
            self.mmu.load_cart(c);
        }
        if let Some(b) = boot {
            self.mmu.load_boot_rom(b);
        }
    }

    /// Resets to the power-on state, preserving cartridge and boot ROM.
    ///
    /// This is useful when you want to re-run the boot ROM sequence.
    pub fn reset_power_on(&mut self) {
        let cart = self.mmu.cart.take();
        let boot = self.mmu.boot_rom.take();
        self.cpu = Cpu::new_power_on_with_revision(self.cgb, self.dmg_revision);
        self.mmu = Mmu::new_power_on_with_revisions(self.cgb, self.dmg_revision, self.cgb_revision);
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
        Self::new()
    }
}
