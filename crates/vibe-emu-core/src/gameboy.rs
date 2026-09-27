use crate::{apu::ApuBootSnapshot, cpu::Cpu, hardware::Model, mmu::Mmu};

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
#[derive(Debug)]
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

    /// Enable the optional SGB command host on the selected hardware.
    /// On CGB this creates a fictional CGB + SGB hybrid: CGB CPU/PPU timing and
    /// colors with SGB borders, masks and multiplayer. Reset preserves the mode.
    pub fn enable_sgb_extensions(&mut self) {
        self.mmu.ppu.enable_sgb_extensions();
        if let Some(cart) = &self.mmu.cart {
            self.mmu
                .ppu
                .sgb
                .as_mut()
                .unwrap()
                .set_cartridge_header(&cart.rom);
        }
    }

    /// Run an isolated SGB startup and retain only its first completed border.
    ///
    /// This implements "GBC + initial SGB border": the main machine's CPU,
    /// RAM, cartridge saves, RTC and boot ROM are not advanced or changed.
    /// The donor has no save paths, audio output, input or serial connection.
    /// `boot_rom` is the SGB (not CGB) boot ROM; `None` skips its execution.
    /// A 600-frame limit matches SameBoy's border-borrowing window. The dot
    /// budget also bounds broken ROMs that never reach a frame boundary.
    /// Returns false for an ineligible ROM, timeout or CPU fault.
    pub fn borrow_sgb_border(&mut self, boot_rom: Option<&[u8]>, max_frames: u32) -> bool {
        let Some(cart) = self.mmu.cart.as_ref() else {
            return false;
        };
        if !self.model.is_cgb()
            || !cart.cgb
            || cart.rom.get(0x146) != Some(&3)
            || cart.rom.get(0x14b) != Some(&0x33)
        {
            return false;
        }
        let mut donor_cart = Cartridge::from_bytes(cart.rom.clone());
        donor_cart.ram.clone_from(&cart.ram);
        let mut donor = if let Some(boot) = boot_rom {
            let mut donor = Self::new_power_on(Model::Sgb);
            donor.mmu.load_boot_rom(boot.to_vec());
            donor
        } else {
            Self::new(Model::Sgb)
        };
        donor.load_cart(donor_cart);
        let deadline = u64::from(max_frames) * 70_224;
        while donor.cpu.cycles < deadline {
            let before = donor.cpu.cycles;
            donor.cpu.run_for_dots(&mut donor.mmu, 4096);
            if donor.cpu.faulted || donor.cpu.cycles == before {
                return false;
            }
            if donor.mmu.ppu.frame_ready() {
                donor.mmu.ppu.clear_frame_flag();
                if let Some(border) = donor.mmu.ppu.sgb.as_ref().and_then(|sgb| sgb.border()) {
                    self.mmu.ppu.set_sgb_border(border);
                    return true;
                }
            }
        }
        false
    }

    /// Load a cartridge and apply header-dependent CPU state when skipping boot.
    /// Prefer this to loading directly into the MMU for AGB compatibility mode.
    pub fn load_cart(&mut self, cart: Cartridge) {
        if !self.mmu.boot_mapped && self.cpu.pc == 0x100 && self.cpu.cycles == 0 {
            if self.model == Model::Mgb {
                self.cpu.f = if cart.rom.get(0x14d) == Some(&0) {
                    0x80
                } else {
                    0xb0
                };
            }
            if self.model.is_agb() && !cart.cgb {
                let nintendo = cart.rom.get(0x14b) == Some(&1)
                    || (cart.rom.get(0x14b) == Some(&0x33)
                        && cart.rom.get(0x144..0x146) == Some(b"01"));
                let checksum = if nintendo {
                    cart.rom
                        .get(0x134..0x144)
                        .unwrap_or(&[])
                        .iter()
                        .fold(0u8, |sum, b| sum.wrapping_add(*b))
                } else {
                    0
                };
                self.cpu.b = checksum.wrapping_add(1);
                self.cpu.f = (if self.cpu.b == 0 { 0x80 } else { 0 })
                    | (if checksum & 0xf == 0xf { 0x20 } else { 0 });
                self.cpu.d = 0;
                self.cpu.e = 8;
                let hl: u16 = if matches!(checksum, 0x43 | 0x58) {
                    0x991a
                } else {
                    0x007c
                };
                self.cpu.h = (hl >> 8) as u8;
                self.cpu.l = hl as u8;
            }
        }
        self.mmu.load_cart(cart);
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
        let sgb_extensions = self
            .mmu
            .ppu
            .sgb
            .as_ref()
            .is_some_and(|sgb| sgb.is_command_host());
        let border = self
            .mmu
            .ppu
            .sgb
            .as_ref()
            .filter(|sgb| !sgb.is_command_host())
            .and_then(|sgb| sgb.border());
        let cart = self.mmu.cart.take();
        let boot = self.mmu.boot_rom.take();
        self.cpu = Cpu::new(self.model);
        self.mmu.reset_post_boot_in_place(self.model);
        if sgb_extensions {
            self.enable_sgb_extensions();
        }
        if let Some(border) = border {
            self.mmu.ppu.set_sgb_border(border);
        }
        if let Some(c) = cart {
            self.load_cart(c);
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
        let sgb_extensions = self
            .mmu
            .ppu
            .sgb
            .as_ref()
            .is_some_and(|sgb| sgb.is_command_host());
        let border = self
            .mmu
            .ppu
            .sgb
            .as_ref()
            .filter(|sgb| !sgb.is_command_host())
            .and_then(|sgb| sgb.border());
        let cart = self.mmu.cart.take();
        let boot = self.mmu.boot_rom.take();
        self.cpu = Cpu::new_power_on();
        self.mmu.reset_power_on_in_place(self.model);
        if sgb_extensions {
            self.enable_sgb_extensions();
        }
        if let Some(border) = border {
            self.mmu.ppu.set_sgb_border(border);
        }
        if let Some(c) = cart {
            self.load_cart(c);
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

    #[test]
    fn resets_preserve_the_external_serial_device_but_clear_transfers() {
        let mut gb = GameBoy::new(Model::Sgb);
        gb.mmu
            .ppu
            .set_dmg_palette([0xffffff, 0xaaaaaa, 0x555555, 0]);
        gb.mmu
            .serial
            .connect(Box::new(crate::serial::NullLinkPort::new(true)));
        for power_on in [false, true] {
            gb.mmu.serial.write(0xff01, 0x12);
            gb.mmu.serial.write(0xff02, 0x81);
            if power_on {
                gb.reset_power_on();
            } else {
                gb.reset();
            }
            assert_eq!(gb.mmu.serial.read(0xff02) & 0x80, 0);
            assert_eq!(gb.mmu.ppu.dmg_palette(), [0xffffff, 0xaaaaaa, 0x555555, 0]);
            gb.mmu.serial.write(0xff01, 0x42);
            gb.mmu.serial.write(0xff02, 0x81);
            gb.mmu.serial.step(0, 4096, false, &mut gb.mmu.if_reg);
            assert_eq!(gb.mmu.serial.read(0xff01), 0x42);
        }
    }

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
