use vibe_emu_core::gameboy::GameBoy;

#[derive(Clone, Copy, Debug, Default)]
pub struct CpuSnapshot {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    pub ime: bool,
    pub cycles: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibe_emu_core::{cartridge::Cartridge, hardware::Model, watchpoints::Watchpoint};

    #[test]
    fn repeated_inspection_does_not_change_execution_or_fire_watchpoints() {
        for model in [Model::default(), Model::Cgb(Default::default())] {
            let make_machine = || {
                let mut gb = GameBoy::new(model);
                let mut rom = vec![0; 0x8000];
                rom[0x143] = 0x80;
                // Repeatedly increment WRAM while video/audio hardware runs.
                rom[0x100..0x109]
                    .copy_from_slice(&[0xFA, 0x00, 0xC0, 0x3C, 0xEA, 0x00, 0xC0, 0x18, 0xF7]);
                gb.mmu.load_cart(Cartridge::from_bytes(rom));
                gb.mmu.watchpoints.set_watchpoints(vec![Watchpoint {
                    id: 1,
                    enabled: true,
                    range: 0xFE00..=0xFFFF,
                    on_read: true,
                    on_write: false,
                    on_execute: false,
                    on_jump: false,
                    value_match: None,
                    message: None,
                }]);
                gb
            };
            let mut inspected = make_machine();
            let mut reference = make_machine();
            for step in 0..5000 {
                inspected.cpu.step(&mut inspected.mmu);
                reference.cpu.step(&mut reference.mmu);
                if step % 97 == 0 {
                    UiSnapshot::from_gb(&mut inspected, step % 2 == 0);
                    assert!(inspected.mmu.watchpoints.take_hit().is_none());
                }
            }
            assert_eq!(
                format!("{:?}", inspected.cpu),
                format!("{:?}", reference.cpu)
            );
            assert_eq!(inspected.mmu.wram, reference.mmu.wram);
            assert_eq!(inspected.mmu.ppu.oam, reference.mmu.ppu.oam);
            assert_eq!(
                inspected.mmu.ppu.framebuffer(),
                reference.mmu.ppu.framebuffer()
            );
            assert_eq!(
                inspected.mmu.apu.debug_state(),
                reference.mmu.apu.debug_state()
            );
            assert_eq!(
                inspected.mmu.apu.debug_boot_snapshot(),
                reference.mmu.apu.debug_boot_snapshot()
            );
            for addr in 0xFF00..=0xFFFF {
                assert_eq!(
                    inspected.mmu.peek_byte(addr),
                    reference.mmu.peek_byte(addr),
                    "register {addr:04X}"
                );
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PpuSnapshot {
    pub frame_counter: u64,
    pub cgb: bool,

    pub lcdc: u8,
    pub stat: u8,
    pub scy: u8,
    pub scx: u8,
    pub ly: u8,
    pub bgp: u8,
    pub obp0: u8,
    pub obp1: u8,

    pub vram0: Vec<u8>,
    pub vram1: Vec<u8>,
    pub oam: Vec<u8>,
    pub framebuffer: Vec<u32>,

    /// CGB BG palette colors as 0x00RRGGBB.
    pub cgb_bg_colors: [[u32; 4]; 8],
    /// CGB OBJ palette colors as 0x00RRGGBB.
    pub cgb_ob_colors: [[u32; 4]; 8],
}

impl PpuSnapshot {
    pub fn vram_bank(&self, bank: usize) -> &[u8] {
        match bank {
            0 => &self.vram0,
            _ => &self.vram1,
        }
    }

    pub fn obp(&self, which: u8) -> u8 {
        match which {
            0 => self.obp0,
            _ => self.obp1,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiSnapshot {
    pub cpu: CpuSnapshot,
    pub ppu: PpuSnapshot,
    pub debugger: DebuggerSnapshot,
}

#[derive(Clone, Debug)]
pub struct DebuggerSnapshot {
    pub paused: bool,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub active_rom_bank: u16,
    pub disassembly_base: u16,
    pub disassembly_bytes: Vec<u8>,
    pub stack_base: u16,
    pub stack_bytes: Vec<u8>,
    pub cgb_mode: bool,
    pub vram_bank: u8,
    pub wram_bank: u8,
    pub sram_bank: u8,
    pub sram_enabled: bool,

    /// Full $0000-$FFFF memory image while paused. This is used for the virtualized
    /// full-address-space disassembly view.
    pub mem_image: Option<Box<[u8; 0x10000]>>,
}

impl Default for DebuggerSnapshot {
    fn default() -> Self {
        Self {
            paused: true,
            if_reg: 0,
            ie_reg: 0,
            active_rom_bank: 1,
            disassembly_base: 0,
            disassembly_bytes: vec![0; 0x200],
            stack_base: 0,
            stack_bytes: vec![0; 0x40],
            cgb_mode: false,
            vram_bank: 0,
            wram_bank: 1,
            sram_bank: 0,
            sram_enabled: false,
            mem_image: None,
        }
    }
}

impl UiSnapshot {
    /// Capture passive inspection data; this must never perform CPU bus reads.
    pub fn from_gb(gb: &mut GameBoy, paused: bool) -> Self {
        let cpu = CpuSnapshot {
            a: gb.cpu.a,
            f: gb.cpu.f,
            b: gb.cpu.b,
            c: gb.cpu.c,
            d: gb.cpu.d,
            e: gb.cpu.e,
            h: gb.cpu.h,
            l: gb.cpu.l,
            sp: gb.cpu.sp,
            pc: gb.cpu.pc,
            ime: gb.cpu.ime,
            cycles: gb.cpu.cycles,
        };

        let ppu = &mut gb.mmu.ppu;
        let mut cgb_bg_colors = [[0u32; 4]; 8];
        let mut cgb_ob_colors = [[0u32; 4]; 8];
        if ppu.is_cgb() {
            for pal in 0..8 {
                for col in 0..4 {
                    cgb_bg_colors[pal][col] = ppu.bg_palette_color(pal, col);
                    cgb_ob_colors[pal][col] = ppu.ob_palette_color(pal, col);
                }
            }
        }

        let ppu_snap = PpuSnapshot {
            frame_counter: ppu.frames(),
            cgb: ppu.is_cgb(),
            lcdc: ppu.read_reg(0xFF40),
            stat: ppu.read_reg(0xFF41),
            scy: ppu.read_reg(0xFF42),
            scx: ppu.read_reg(0xFF43),
            ly: ppu.read_reg(0xFF44),
            bgp: ppu.read_reg(0xFF47),
            obp0: ppu.read_reg(0xFF48),
            obp1: ppu.read_reg(0xFF49),
            vram0: ppu.vram[0].to_vec(),
            vram1: ppu.vram[1].to_vec(),
            oam: ppu.oam.to_vec(),
            framebuffer: ppu.framebuffer().to_vec(),
            cgb_bg_colors,
            cgb_ob_colors,
        };

        let active_rom_bank = gb
            .mmu
            .cart
            .as_ref()
            .map(|c| c.current_rom_bank())
            .unwrap_or(1);

        let sram_bank = gb
            .mmu
            .cart
            .as_ref()
            .map(|c| c.current_ram_bank())
            .unwrap_or(0);

        let sram_enabled = gb
            .mmu
            .cart
            .as_ref()
            .map(|c| c.ram_enabled())
            .unwrap_or(false);

        let cgb_mode = gb.mmu.is_cgb();
        let vram_bank = gb.mmu.ppu.vram_bank as u8;
        let wram_bank = gb.mmu.wram_bank as u8;

        let mem_image = if paused {
            let mut mem = Box::new([0u8; 0x10000]);
            for (addr, b) in mem.iter_mut().enumerate() {
                *b = gb.mmu.peek_byte(addr as u16);
            }
            Some(mem)
        } else {
            None
        };

        let disassembly_base = cpu.pc.saturating_sub(0x40);
        let mut disassembly_bytes = vec![0u8; 0x200];
        for (i, b) in disassembly_bytes.iter_mut().enumerate() {
            *b = gb.mmu.peek_byte(disassembly_base.wrapping_add(i as u16));
        }

        let stack_base = cpu.sp;
        let mut stack_bytes = vec![0u8; 0x40];
        for (i, b) in stack_bytes.iter_mut().enumerate() {
            *b = gb.mmu.peek_byte(stack_base.wrapping_add(i as u16));
        }

        let dbg = DebuggerSnapshot {
            paused,
            if_reg: gb.mmu.if_reg,
            ie_reg: gb.mmu.ie_reg,
            active_rom_bank,
            disassembly_base,
            disassembly_bytes,
            stack_base,
            stack_bytes,
            cgb_mode,
            vram_bank,
            wram_bank,
            sram_bank,
            sram_enabled,
            mem_image,
        };

        Self {
            cpu,
            ppu: ppu_snap,
            debugger: dbg,
        }
    }
}
