use crate::{
    apu::Apu,
    cartridge::Cartridge,
    hardware::{CgbRevision, DmgRevision},
    input::Input,
    ppu::Ppu,
    serial::Serial,
    timer::Timer,
};

use crate::ppu::OamBugAccess;

fn env_flag_enabled(var: &str) -> bool {
    use std::sync::OnceLock;

    static CACHE: OnceLock<std::collections::HashMap<&'static str, bool>> = OnceLock::new();
    // Cache a small fixed set to avoid repeated env parsing.
    let cache = CACHE.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for key in ["VIBEEMU_TRACE_OAMBUG", "VIBEEMU_TRACE_LCDC"] {
            let enabled = std::env::var_os(key)
                .map(|v| {
                    let s = v.to_string_lossy();
                    !(s.is_empty() || s == "0" || s.eq_ignore_ascii_case("false"))
                })
                .unwrap_or(false);
            map.insert(key, enabled);
        }
        map
    });
    cache.get(var).copied().unwrap_or(false)
}

const WRAM_BANK_SIZE: usize = 0x1000;

fn power_on_wram_seed(cgb: bool, dmg_revision: DmgRevision, cgb_revision: CgbRevision) -> u32 {
    // Uninitialized WRAM contents are effectively random on real hardware.
    // We keep them deterministic for reproducible tests while ensuring the
    // contents are not trivially all $00/$FF.
    let mut seed: u32 = 0xC0DE_1BAD;
    seed ^= if cgb { 0x4347_4221 } else { 0x444D_4721 };
    seed ^= (dmg_revision as u32).wrapping_mul(0x9E37_79B9);
    seed ^= (cgb_revision as u32).wrapping_mul(0x85EB_CA6B);
    // Avoid the xorshift all-zero lockup state.
    if seed == 0 { 0xA5A5_5A5A } else { seed }
}

fn init_power_on_wram(seed: u32) -> [[u8; WRAM_BANK_SIZE]; 8] {
    let mut wram = [[0u8; WRAM_BANK_SIZE]; 8];
    let mut state = seed;

    for bank_wram in &mut wram {
        for byte in bank_wram.iter_mut() {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let mut v = state as u8;
            // Ensure we don't accidentally end up with all $00/$FF.
            if v == 0x00 || v == 0xFF {
                v ^= 0xA5;
            }
            *byte = v;
        }
    }

    wram
}

/// Transfer mode for CGB DMA operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DmaMode {
    /// General DMA (immediate)
    Gdma,
    /// HBlank DMA
    Hdma,
}

#[derive(Debug)]
struct HdmaState {
    /// 16-bit source pointer (upper 12 bits writable)
    src: u16,
    /// Destination in VRAM (0x8000 | (dst & 0x1FF0))
    dst: u16,
    /// Remaining 0x10-byte blocks (0-7F means 1-128 blocks)
    blocks: u8,
    /// Current DMA mode
    mode: DmaMode,
    /// HDMA active flag
    active: bool,
    /// Whether the previous transfer was explicitly cancelled (FF55 <- 0)
    cancelled: bool,
}

pub struct Mmu {
    pub wram: [[u8; WRAM_BANK_SIZE]; 8],
    pub wram_bank: usize,
    pub hram: [u8; 0x7F],
    pub cart: Option<Cartridge>,
    pub boot_rom: Option<Vec<u8>>,
    pub boot_mapped: bool,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub serial: Serial,
    pub ppu: Ppu,
    pub apu: Apu,
    pub timer: Timer,
    /// 16-bit divider counter in the LCD dot clock domain (used for APU/serial timing).
    /// In normal speed this matches the CPU divider progression; in CGB double-speed
    /// the CPU divider advances twice as fast as this dot counter.
    pub dot_div: u16,
    pub input: Input,
    hdma: HdmaState,
    pub key1: u8,
    pub rp: u8,
    undoc_ff72: u8,
    undoc_ff73: u8,
    undoc_ff74: u8,
    undoc_ff75: u8,
    pub dma_cycles: u16,
    dma_source: u16,
    pending_dma: Option<u16>,
    pending_delay: u16,
    /// Remaining stall cycles after a General DMA
    gdma_cycles: u32,
    cgb_mode: bool,
    cgb_revision: CgbRevision,
    dmg_revision: DmgRevision,
    /// Last CPU program counter observed when the CPU performed a memory
    /// operation. This is set by the `Cpu` helpers before calling into the
    /// MMU so logs can attribute blocked accesses to the originating PC.
    pub last_cpu_pc: Option<u16>,

    /// Last value driven on the CPU data bus by a CPU-visible read/write.
    /// Used to model open-bus behaviour for certain invalid/unused accesses.
    data_bus: u8,

    /// Last value observed on the "main" bus (everything except VRAM) for
    /// addresses below $FF00.
    ///
    /// Some open-bus scenarios on cartridge space depend on the previous value
    /// from this bus specifically rather than the most recent internal I/O/HRAM
    /// access.
    main_bus: u8,

    /// One-shot override for the next OAM access' corruption classification.
    ///
    /// Used for instructions like `LD A,[HL+]` / `LD A,[HL-]` that have a
    /// distinct corruption pattern on DMG.
    pub(crate) oam_bug_next_access: Option<OamBugAccess>,

    pub watchpoints: crate::watchpoints::WatchpointEngine,
}

impl Mmu {
    pub fn is_cgb(&self) -> bool {
        self.cgb_mode
    }

    pub fn new_with_mode(cgb: bool) -> Self {
        Self::new_with_revisions(cgb, DmgRevision::default(), CgbRevision::default())
    }

    pub fn new_with_config(cgb: bool, revision: CgbRevision) -> Self {
        Self::new_with_revisions(cgb, DmgRevision::default(), revision)
    }

    pub fn new_with_revisions(
        cgb: bool,
        dmg_revision: DmgRevision,
        cgb_revision: CgbRevision,
    ) -> Self {
        let mut timer = Timer::new();
        // Power-on DIV phase differs across hardware families/revisions.
        //
        // When running without a boot ROM, we start from the post-boot state.
        // These values match the phases measured by mooneye's boot_div tests so
        // the first post-boot instruction sequence observes the expected timing.
        timer.div = if cgb {
            match cgb_revision {
                // CGB A-E share a common phase for DIV after boot.
                CgbRevision::RevA
                | CgbRevision::RevB
                | CgbRevision::RevC
                | CgbRevision::RevD
                | CgbRevision::RevE => 0x2678,
                // CGB0 differs from CGB A-E (mooneye misc/boot_div-cgb0).
                CgbRevision::Rev0 => 0x2884,
            }
        } else {
            match dmg_revision {
                DmgRevision::Rev0 => 0x1830,
                DmgRevision::RevA | DmgRevision::RevB | DmgRevision::RevC => 0xABCC,
            }
        };

        let dot_div = timer.div;

        let mut ppu = Ppu::new_with_mode(cgb);
        ppu.apply_boot_state(if cgb { None } else { Some(dmg_revision) });

        Self {
            wram: [[0; WRAM_BANK_SIZE]; 8],
            wram_bank: 1,
            hram: [0; 0x7F],
            cart: None,
            boot_rom: None,
            boot_mapped: false,
            if_reg: 0xE1,
            ie_reg: 0,
            serial: Serial::new(cgb, dmg_revision),
            ppu,
            apu: Apu::new_with_revisions(cgb, dmg_revision, cgb_revision),
            timer,
            dot_div,
            input: Input::new(),
            hdma: HdmaState {
                src: 0,
                dst: Self::sanitize_vram_dma_dest(0),
                blocks: 0,
                mode: DmaMode::Gdma,
                active: false,
                cancelled: false,
            },
            key1: if cgb { 0x7E } else { 0 },
            rp: 0,
            undoc_ff72: 0,
            undoc_ff73: 0,
            undoc_ff74: 0,
            undoc_ff75: 0,
            dma_cycles: 0,
            dma_source: 0,
            pending_dma: None,
            pending_delay: 0,
            gdma_cycles: 0,
            cgb_mode: cgb,
            cgb_revision,
            dmg_revision,
            oam_bug_next_access: None,
            last_cpu_pc: None,
            data_bus: 0xFF,
            main_bus: 0xFF,
            watchpoints: crate::watchpoints::WatchpointEngine::default(),
        }
    }

    /// Create an MMU initialized to an approximate power-on state suitable for
    /// executing a boot ROM.
    ///
    /// This differs from `new_with_revisions`, which intentionally initializes
    /// the system to a *post-boot* state when running without a boot ROM.
    pub fn new_power_on_with_revisions(
        cgb: bool,
        dmg_revision: DmgRevision,
        cgb_revision: CgbRevision,
    ) -> Self {
        let mut timer = Timer::new();

        // Power-on DIV phase differs across hardware families/revisions.
        // When executing a real boot ROM, this initial phase affects the
        // divider value observed by early cart code (e.g. whichboot).
        if cgb && matches!(cgb_revision, CgbRevision::RevE) {
            // Seed chosen to match whichboot's timing reference for CGB
            // (LY=$90, DIV=$1E, frac=$28) when running the RevE boot ROM.
            timer.div = 0x0104;
        }

        let dot_div = timer.div;

        let ppu = Ppu::new_with_mode(cgb);

        let wram = init_power_on_wram(power_on_wram_seed(cgb, dmg_revision, cgb_revision));

        Self {
            wram,
            wram_bank: 1,
            hram: [0; 0x7F],
            cart: None,
            boot_rom: None,
            boot_mapped: false,
            if_reg: 0xE1,
            ie_reg: 0,
            serial: Serial::new(cgb, dmg_revision),
            ppu,
            apu: Apu::new_with_revisions(cgb, dmg_revision, cgb_revision),
            timer,
            dot_div,
            input: Input::new(),
            hdma: HdmaState {
                src: 0,
                dst: Self::sanitize_vram_dma_dest(0),
                blocks: 0,
                mode: DmaMode::Gdma,
                active: false,
                cancelled: false,
            },
            key1: if cgb { 0x7E } else { 0 },
            rp: 0,
            undoc_ff72: 0,
            undoc_ff73: 0,
            undoc_ff74: 0,
            undoc_ff75: 0,
            dma_cycles: 0,
            dma_source: 0,
            pending_dma: None,
            pending_delay: 0,
            gdma_cycles: 0,
            cgb_mode: cgb,
            cgb_revision,
            dmg_revision,
            oam_bug_next_access: None,
            last_cpu_pc: None,
            data_bus: 0xFF,
            main_bus: 0xFF,
            watchpoints: crate::watchpoints::WatchpointEngine::default(),
        }
    }

    fn updates_main_bus(addr: u16) -> bool {
        addr < 0xFF00 && !(0x8000..=0x9FFF).contains(&addr)
    }

    pub fn new() -> Self {
        Self::new_with_mode(false)
    }

    pub fn load_cart(&mut self, cart: Cartridge) {
        let is_dmg = !cart.cgb;
        self.cart = Some(cart);
        if self.cgb_mode && is_dmg {
            self.ppu.apply_dmg_compatibility_palettes();
        }
    }

    pub fn save_cart_ram(&mut self) {
        if let Some(cart) = &mut self.cart
            && let Err(e) = cart.save_ram()
        {
            log::warn!(target: "vibe_emu_core::cartridge", "Failed to save RAM: {e}");
        }
    }

    pub fn load_boot_rom(&mut self, data: Vec<u8>) {
        self.boot_rom = Some(data);
        self.boot_mapped = true;
    }

    fn read_byte_inner(&mut self, addr: u16, allow_dma: bool) -> u8 {
        if !allow_dma && self.dma_cycles > 0 {
            match addr {
                // If the DMA engine is sourcing from the ROM bus, CPU reads from ROM can
                // observe the DMA-transferred byte (bus conflict).
                0x0000..=0x7FFF => {
                    if self.oam_dma_source_in_rom() {
                        return self.oam_dma_bus_conflict_byte();
                    }
                }
                // Allow ROM, WRAM/Echo and all I/O/HRAM accesses during the transfer.
                0xC000..=0xFDFF | 0xFF00..=0xFFFF => {}
                // OAM/VRAM buses are blocked.
                0xFE00..=0xFEFF => return 0xFF,
                _ => return 0xFF,
            }
        }
        match addr {
            // When the boot ROM is mapped, overlay it on the lower
            // portion of the address space. On DMG this covers
            // 0x0000-0x00FF. On CGB the internal boot ROM also maps
            // 0x0200-0x08FF while leaving the cartridge header at
            // 0x0100-0x01FF visible.
            0x0000..=0x00FF if self.boot_mapped => self
                .boot_rom
                .as_ref()
                .and_then(|b| b.get(addr as usize).copied())
                .unwrap_or(0xFF),
            0x0200..=0x08FF if self.boot_mapped && self.cgb_mode => self
                .boot_rom
                .as_ref()
                .and_then(|b| b.get(addr as usize).copied())
                .unwrap_or(0xFF),
            0x0000..=0x7FFF => self
                .cart
                .as_mut()
                .map(|c| c.read_with_open_bus(addr, self.main_bus))
                .unwrap_or(0xFF),
            0x8000..=0x9FFF => {
                let accessible = self.ppu.vram_read_accessible();
                if accessible {
                    let value = self.ppu.vram[self.ppu.vram_bank][(addr - 0x8000) as usize];
                    #[cfg(feature = "ppu-trace")]
                    {
                        let (stage, cycle, mode, mode_clock) = self.ppu.debug_startup_snapshot();
                        let pc_str = self
                            .last_cpu_pc
                            .map(|p| format!("{:04X}", p))
                            .unwrap_or_else(|| "<none>".to_string());
                        log::trace!(
                            target: "vibe_emu_core::ppu",
                            "VRAM read allow addr={:04X} val={:02X} bank={} pc={} stage={:?} cycle={:?} mode={} mode_clock={}",
                            addr,
                            value,
                            self.ppu.vram_bank,
                            pc_str,
                            stage,
                            cycle,
                            mode,
                            mode_clock
                        );
                    }
                    value
                } else {
                    #[cfg(feature = "ppu-trace")]
                    {
                        let (stage, cycle, mode, mode_clock) = self.ppu.debug_startup_snapshot();
                        let pc_str = self
                            .last_cpu_pc
                            .map(|p| format!("{:04X}", p))
                            .unwrap_or_else(|| "<none>".to_string());
                        log::trace!(
                            target: "vibe_emu_core::ppu",
                            "VRAM read blocked addr={:04X} bank={} pc={} stage={:?} cycle={:?} mode={} mode_clock={}",
                            addr,
                            self.ppu.vram_bank,
                            pc_str,
                            stage,
                            cycle,
                            mode,
                            mode_clock
                        );
                    }
                    0xFF
                }
            }
            0xA000..=0xBFFF => self
                .cart
                .as_mut()
                .map(|c| c.read_with_open_bus(addr, self.main_bus))
                .unwrap_or(0xFF),
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.wram[self.wram_bank][(addr - 0xD000) as usize],
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize],
            0xF000..=0xFDFF => self.wram[self.wram_bank][(addr - 0xF000) as usize],
            0xFE00..=0xFE9F => {
                if self.ppu.oam_read_accessible() {
                    self.oam_bug_next_access = None;
                    let val = self.ppu.oam[(addr - 0xFE00) as usize];
                    if env_flag_enabled("VIBEEMU_TRACE_OAMBUG") && self.ppu.lcd_enabled() {
                        let pc_str = self
                            .last_cpu_pc
                            .map(|p| format!("{:04X}", p))
                            .unwrap_or_else(|| "<none>".to_string());
                        let (mode, mode_clock, accessed_row, row) =
                            self.ppu.debug_oam_bug_snapshot();
                        let (stage, cycle, _, _) = self.ppu.debug_startup_snapshot();
                        log::trace!(
                            target: "vibe_emu_core::oambug",
                            "read ok pc={} addr={:04X} val={:02X} ppu_mode={} mode_clock={} accessed_oam_row={:?} row={:?} dmg_stage={:?} dmg_cycle={:?}",
                            pc_str,
                            addr,
                            val,
                            mode,
                            mode_clock,
                            accessed_row,
                            row,
                            stage,
                            cycle
                        );
                    }
                    val
                } else {
                    if !self.cgb_mode {
                        let access = self
                            .oam_bug_next_access
                            .take()
                            .unwrap_or(OamBugAccess::Read);
                        if env_flag_enabled("VIBEEMU_TRACE_OAMBUG") {
                            let pc_str = self
                                .last_cpu_pc
                                .map(|p| format!("{:04X}", p))
                                .unwrap_or_else(|| "<none>".to_string());
                            let (mode, mode_clock, accessed_row, row) =
                                self.ppu.debug_oam_bug_snapshot();
                            let (stage, cycle, _, _) = self.ppu.debug_startup_snapshot();
                            log::trace!(
                                target: "vibe_emu_core::oambug",
                                "read blocked pc={} addr={:04X} access={:?} ppu_mode={} mode_clock={} accessed_oam_row={:?} row={:?} dmg_stage={:?} dmg_cycle={:?}",
                                pc_str,
                                addr,
                                access,
                                mode,
                                mode_clock,
                                accessed_row,
                                row,
                                stage,
                                cycle
                            );
                        }
                        self.ppu.oam_bug_access(addr, access);
                    } else {
                        self.oam_bug_next_access = None;
                    }
                    0xFF
                }
            }
            0xFEA0..=0xFEFF => {
                if !self.cgb_mode && !self.ppu.oam_read_accessible() {
                    let access = self
                        .oam_bug_next_access
                        .take()
                        .unwrap_or(OamBugAccess::Read);
                    self.ppu.oam_bug_access(addr, access);
                } else {
                    self.oam_bug_next_access = None;
                }
                0xFF
            }
            0xFF00 => self.input.read(),
            0xFF01 | 0xFF02 => self.serial.read(addr),
            0xFF04..=0xFF07 => self.timer.read(addr),
            // IF: upper 3 bits are unused and read back as 1 on hardware.
            0xFF0F => self.if_reg | 0xE0,
            0xFF10..=0xFF3F => self.apu.read_reg(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B | 0xFF68..=0xFF6B => self.ppu.read_reg(addr),
            0xFF46 => self.ppu.dma,
            0xFF51 => {
                if self.cgb_mode {
                    (self.hdma.src >> 8) as u8
                } else {
                    0xFF
                }
            }
            0xFF52 => {
                if self.cgb_mode {
                    (self.hdma.src & 0x00F0) as u8
                } else {
                    0xFF
                }
            }
            0xFF53 => {
                if self.cgb_mode {
                    ((self.hdma.dst & 0x1F00) >> 8) as u8
                } else {
                    0xFF
                }
            }
            0xFF54 => {
                if self.cgb_mode {
                    (self.hdma.dst & 0x00F0) as u8
                } else {
                    0xFF
                }
            }
            0xFF55 => {
                if !self.cgb_mode {
                    0xFF
                } else if self.hdma.active {
                    // Busy flag (bit 7) is cleared while the DMA is running.
                    self.hdma.blocks.saturating_sub(1) & 0x7F
                } else if self.hdma.cancelled {
                    // After cancellation the hardware reports bit 7 set with the lower bits cleared.
                    0x80
                } else {
                    // Hardware returns 0xFF once HDMA/GDMA has completed or no transfer is pending.
                    0xFF
                }
            }
            0xFF4D => {
                if self.cgb_mode {
                    (self.key1 & 0x81) | 0x7E
                } else {
                    0xFF
                }
            }
            0xFF56 => {
                if self.cgb_mode {
                    self.rp | 0xC0
                } else {
                    0xFF
                }
            }
            0xFF4F => {
                if self.cgb_mode {
                    self.ppu.vram_bank as u8
                } else {
                    0xFF
                }
            }
            0xFF70 => {
                if self.cgb_mode {
                    self.wram_bank as u8
                } else {
                    0xFF
                }
            }
            0xFF72 => {
                if self.cgb_mode {
                    self.undoc_ff72
                } else {
                    0xFF
                }
            }
            0xFF73 => {
                if self.cgb_mode {
                    self.undoc_ff73
                } else {
                    0xFF
                }
            }
            0xFF74 => {
                if self.cgb_mode {
                    self.undoc_ff74
                } else {
                    // DMG: read-only, locked to $FF.
                    0xFF
                }
            }
            0xFF75 => {
                if self.cgb_mode {
                    // Only bits 4-6 are readable/writable; other bits read high.
                    (self.undoc_ff75 & 0x70) | 0x8F
                } else {
                    0xFF
                }
            }
            0xFF76 | 0xFF77 => {
                if self.cgb_mode {
                    self.apu.read_pcm(addr)
                } else {
                    0xFF
                }
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => 0xFF,
        }
    }

    pub fn read_byte(&mut self, addr: u16) -> u8 {
        let value = self.read_byte_inner(addr, false);
        self.data_bus = value;
        if Self::updates_main_bus(addr) {
            self.main_bus = value;
        }
        self.watchpoints.note_read(self.last_cpu_pc, addr, value);
        value
    }

    fn dma_read_byte(&mut self, addr: u16) -> u8 {
        let addr = if !self.cgb_mode && (0xFE00..=0xFF9F).contains(&addr) {
            addr.wrapping_sub(0x2000)
        } else {
            addr
        };

        self.read_byte_inner(addr, true)
    }

    pub(crate) fn oam_dma_in_progress(&self) -> bool {
        self.dma_cycles > 0
    }

    pub(crate) fn oam_dma_source_in_rom(&self) -> bool {
        (0x0000..=0x7FFF).contains(&self.dma_source)
    }

    pub(crate) fn oam_dma_bus_conflict_byte(&mut self) -> u8 {
        // Approximate the value visible on the CPU data bus during OAM DMA.
        // The BullyGB `dmabusconflict` test expects ROM reads to reflect the
        // DMA-transferred byte rather than always reading as $FF.
        let (per_byte, initial): (u16, u16) = if self.key1 & 0x80 != 0 {
            (2, 320)
        } else {
            (4, 640)
        };

        let elapsed = initial.saturating_sub(self.dma_cycles);
        let idx = elapsed / per_byte;
        if idx >= 0xA0 {
            return 0xFF;
        }

        self.dma_read_byte(self.dma_source.wrapping_add(idx))
    }

    pub fn write_byte(&mut self, addr: u16, val: u8) {
        self.data_bus = val;
        if Self::updates_main_bus(addr) {
            self.main_bus = val;
        }
        self.watchpoints.note_write(self.last_cpu_pc, addr, val);
        if self.dma_cycles > 0 {
            match addr {
                0x0000..=0x7FFF | 0xC000..=0xFDFF | 0xFF00..=0xFFFF => {}
                0xFE00..=0xFEFF => return,
                _ => return,
            }
        }

        match addr {
            0x8000..=0x9FFF => {
                let allow = self.ppu.vram_write_accessible();
                if env_flag_enabled("VIBEEMU_TRACE_LCDC") && val == 0x81 {
                    let pc_str = self
                        .last_cpu_pc
                        .map(|p| format!("{:04X}", p))
                        .unwrap_or_else(|| "<none>".to_string());
                    let (stage, cycle, mode, mode_clock) = self.ppu.debug_startup_snapshot();
                    log::trace!(
                        target: "vibe_emu_core::lcdc",
                        "VRAM write pc={} addr={:04X} val={:02X} bank={} allow={} dmg_stage={:?} dmg_cycle={:?} ppu_mode={} mode_clock={}",
                        pc_str,
                        addr,
                        val,
                        self.ppu.vram_bank,
                        allow,
                        stage,
                        cycle,
                        mode,
                        mode_clock
                    );
                }

                if allow {
                    self.ppu.vram[self.ppu.vram_bank][(addr - 0x8000) as usize] = val;
                } else {
                    #[cfg(feature = "ppu-trace")]
                    {
                        let pc_str = self
                            .last_cpu_pc
                            .map(|p| format!("{:04X}", p))
                            .unwrap_or_else(|| "<none>".to_string());
                        log::trace!(
                            target: "vibe_emu_core::ppu",
                            "VRAM write blocked addr={:04X} val={:02X} bank={} pc={}",
                            addr,
                            val,
                            self.ppu.vram_bank,
                            pc_str
                        );
                    }
                }
            }
            0x0000..=0x7FFF | 0xA000..=0xBFFF => {
                if let Some(cart) = self.cart.as_mut() {
                    cart.write(addr, val);
                }
            }
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize] = val,
            0xD000..=0xDFFF => self.wram[self.wram_bank][(addr - 0xD000) as usize] = val,
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize] = val,
            0xF000..=0xFDFF => self.wram[self.wram_bank][(addr - 0xF000) as usize] = val,
            0xFE00..=0xFE9F => {
                let allow = self.ppu.oam_write_accessible();
                if env_flag_enabled("VIBEEMU_TRACE_LCDC") && val == 0x81 {
                    let pc_str = self
                        .last_cpu_pc
                        .map(|p| format!("{:04X}", p))
                        .unwrap_or_else(|| "<none>".to_string());
                    let (stage, cycle, mode, mode_clock) = self.ppu.debug_startup_snapshot();
                    log::trace!(
                        target: "vibe_emu_core::lcdc",
                        "OAM write pc={} addr={:04X} val={:02X} allow={} dmg_stage={:?} dmg_cycle={:?} ppu_mode={} mode_clock={}",
                        pc_str,
                        addr,
                        val,
                        allow,
                        stage,
                        cycle,
                        mode,
                        mode_clock
                    );
                }

                if allow {
                    self.oam_bug_next_access = None;
                    self.ppu.oam[(addr - 0xFE00) as usize] = val;
                } else if !self.cgb_mode {
                    let access = self
                        .oam_bug_next_access
                        .take()
                        .unwrap_or(OamBugAccess::Write);
                    if env_flag_enabled("VIBEEMU_TRACE_OAMBUG") {
                        let pc_str = self
                            .last_cpu_pc
                            .map(|p| format!("{:04X}", p))
                            .unwrap_or_else(|| "<none>".to_string());
                        let (mode, mode_clock, accessed_row, row) =
                            self.ppu.debug_oam_bug_snapshot();
                        let (stage, cycle, _, _) = self.ppu.debug_startup_snapshot();
                        log::trace!(
                            target: "vibe_emu_core::oambug",
                            "write blocked pc={} addr={:04X} val={:02X} access={:?} ppu_mode={} mode_clock={} accessed_oam_row={:?} row={:?} dmg_stage={:?} dmg_cycle={:?}",
                            pc_str,
                            addr,
                            val,
                            access,
                            mode,
                            mode_clock,
                            accessed_row,
                            row,
                            stage,
                            cycle
                        );
                    }
                    self.ppu.oam_bug_access(addr, access);
                } else {
                    self.oam_bug_next_access = None;
                }
            }
            0xFEA0..=0xFEFF => {
                // Unusable region: ignore writes, but the CPU still drives the address bus.
                // On DMG, blocked CPU accesses in $FE00-$FEFF during mode 2 can trigger
                // the OAM corruption bug even if the address is in the unusable subrange.
                if !self.cgb_mode && !self.ppu.oam_write_accessible() {
                    let access = self
                        .oam_bug_next_access
                        .take()
                        .unwrap_or(OamBugAccess::Write);
                    // No extra tracing here; just route the blocked write to the PPU OAM-bug
                    // handler. Tracing was removed to keep logs clean.
                    self.ppu.oam_bug_access(addr, access);
                } else {
                    self.oam_bug_next_access = None;
                }
            }
            0xFF00 => self.input.write(val),
            0xFF01 | 0xFF02 => self.serial.write(addr, val),
            0xFF04 => {
                self.reset_div();
            }
            0xFF05..=0xFF07 => self.timer.write(addr, val, &mut self.if_reg),
            // IF: only bits 0-4 are writable; bits 5-7 remain set.
            0xFF0F => self.if_reg = (val & 0x1F) | 0xE0,
            0xFF10..=0xFF3F => self.apu.write_reg(addr, val),
            0xFF40 => {
                let lcd_was_on = self.ppu.lcd_enabled();
                if env_flag_enabled("VIBEEMU_TRACE_LCDC") {
                    let pc_str = self
                        .last_cpu_pc
                        .map(|p| format!("{:04X}", p))
                        .unwrap_or_else(|| "<none>".to_string());
                    let old = self.ppu.read_reg(0xFF40);
                    log::trace!(
                        target: "vibe_emu_core::lcdc",
                        "LCDC write pc={} old={:02X} new={:02X}",
                        pc_str,
                        old,
                        val
                    );
                }
                self.ppu.write_reg(addr, val);
                if lcd_was_on && !self.ppu.lcd_enabled() {
                    self.complete_active_hdma();
                }
            }
            0xFF41..=0xFF45 | 0xFF47..=0xFF4B | 0xFF68..=0xFF6B => self.ppu.write_reg(addr, val),
            0xFF51 => {
                if self.cgb_mode && !self.hdma.active {
                    self.hdma.src = (val as u16) << 8 | (self.hdma.src & 0x00FF);
                }
            }
            0xFF52 => {
                if self.cgb_mode && !self.hdma.active {
                    self.hdma.src = (self.hdma.src & 0xFF00) | (val & 0xF0) as u16;
                }
            }
            0xFF53 => {
                if self.cgb_mode && !self.hdma.active {
                    let vram_hi = (val & 0x1F) as u16;
                    let raw = (vram_hi << 8) | (self.hdma.dst & 0x00F0);
                    self.hdma.dst = Self::sanitize_vram_dma_dest(raw);
                }
            }
            0xFF54 => {
                if self.cgb_mode && !self.hdma.active {
                    let raw = (self.hdma.dst & 0x1F00) | (val as u16 & 0x00F0);
                    self.hdma.dst = Self::sanitize_vram_dma_dest(raw);
                }
            }
            0xFF55 => {
                if !self.cgb_mode {
                    return;
                }
                self.hdma.dst = Self::sanitize_vram_dma_dest(self.hdma.dst);
                let requested_blocks = (val & 0x7F) + 1;
                if self.hdma.active && (val & 0x80) == 0 {
                    // Abort ongoing HDMA. Hardware reports remaining blocks in FF55 when
                    // polled after cancellation, so keep the current block count.
                    self.hdma.active = false;
                    self.hdma.blocks = 0;
                    self.hdma.cancelled = true;
                } else if val & 0x80 == 0 {
                    self.start_gdma(requested_blocks);
                } else {
                    self.hdma.mode = DmaMode::Hdma;
                    self.hdma.blocks = requested_blocks;
                    self.hdma.active = true;
                    self.hdma.cancelled = false;
                    if !self.ppu.lcd_enabled() || self.ppu.in_hblank() {
                        self.hdma_hblank_transfer();
                    }
                }
            }
            0xFF4D => {
                if self.cgb_mode {
                    self.key1 = (self.key1 & 0x80) | (val & 0x01);
                }
            }
            0xFF56 => {
                if self.cgb_mode {
                    self.rp = val & 0xC1;
                }
            }
            0xFF4F => {
                if self.cgb_mode {
                    self.ppu.vram_bank = (val & 0x01) as usize;
                }
            }
            0xFF46 => {
                self.ppu.dma = val;
                let src = (val as u16) << 8;
                self.pending_dma = Some(src);
                // DMA starts after two M-cycles. `pending_delay` is tracked
                // in T-cycles (dots). In double-speed mode an M-cycle is 2
                // T-cycles instead of 4, so halve the delay there.
                self.pending_delay = if self.key1 & 0x80 != 0 { 4 } else { 8 };
                #[cfg(feature = "ppu-trace")]
                {
                    let region = if (0x0000..=0x7FFF).contains(&src) {
                        "ROM"
                    } else if (0xA000..=0xBFFF).contains(&src) {
                        "CARTRAM"
                    } else if (0xC000..=0xDFFF).contains(&src) || (0xE000..=0xFDFF).contains(&src) {
                        "WRAM"
                    } else {
                        "OTHER"
                    };
                    let pc_str = self
                        .last_cpu_pc
                        .map(|p| format!("{:04X}", p))
                        .unwrap_or_else(|| "<none>".to_string());
                    log::trace!(
                        target: "vibe_emu_core::dma",
                        "pending OAM DMA scheduled src={:04X} region={} pending_delay=8 pc={}",
                        src,
                        region,
                        pc_str
                    );
                }
            }
            0xFF50 => self.boot_mapped = false,
            0xFF70 => {
                if self.cgb_mode {
                    let bank = (val & 0x07) as usize;
                    self.wram_bank = if bank == 0 { 1 } else { bank };
                }
            }
            0xFF72 => {
                if self.cgb_mode {
                    self.undoc_ff72 = val;
                }
            }
            0xFF73 => {
                if self.cgb_mode {
                    self.undoc_ff73 = val;
                }
            }
            0xFF74 => {
                if self.cgb_mode {
                    self.undoc_ff74 = val;
                }
            }
            0xFF75 => {
                if self.cgb_mode {
                    self.undoc_ff75 = val & 0x70;
                }
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = val,
            _ => {}
        }
    }

    /// Write to VRAM bypassing mode checks (used by DMA transfers)
    fn vram_dma_write(&mut self, addr: u16, val: u8) {
        self.ppu.vram[self.ppu.vram_bank][(addr - 0x8000) as usize] = val;
    }

    pub fn take_serial(&mut self) -> Vec<u8> {
        self.serial.take_output()
    }

    /// Advance the ongoing OAM DMA transfer if active.
    pub fn dma_step(&mut self, cycles: u16) {
        for _ in 0..cycles {
            // Only expose a bus value to the PPU on the specific dot where the
            // DMA engine is transferring a byte. On other dots, treat it as not
            // observable.
            self.ppu.oam_dma_write = None;
            if self.pending_delay > 0 {
                self.pending_delay -= 1;
                if self.pending_delay == 0
                    && let Some(src) = self.pending_dma.take()
                {
                    self.dma_source = src;
                    // Number of T-cycles for the OAM DMA transfer: 160 M-cycles.
                    // In normal speed M-cycle = 4 T-cycles => 160 * 4 = 640.
                    // In double-speed M-cycle = 2 T-cycles => 160 * 2 = 320.
                    self.dma_cycles = if self.key1 & 0x80 != 0 { 320 } else { 640 };
                    #[cfg(feature = "ppu-trace")]
                    {
                        let region = if (0x0000..=0x7FFF).contains(&src) {
                            "ROM"
                        } else if (0xA000..=0xBFFF).contains(&src) {
                            "CARTRAM"
                        } else if (0xC000..=0xDFFF).contains(&src)
                            || (0xE000..=0xFDFF).contains(&src)
                        {
                            "WRAM"
                        } else {
                            "OTHER"
                        };
                        log::trace!(
                            target: "vibe_emu_core::dma",
                            "OAM DMA started src={:04X} region={} dma_cycles={}",
                            src,
                            region,
                            self.dma_cycles
                        );
                    }
                }
            }

            if self.dma_cycles == 0 {
                continue;
            }

            #[cfg(feature = "ppu-trace")]
            {
                self.last_cpu_pc = None;
            }
            let per_byte = if self.key1 & 0x80 != 0 { 2 } else { 4 };
            let initial = if self.key1 & 0x80 != 0 { 320 } else { 640 };
            let elapsed = initial - self.dma_cycles;
            if elapsed.is_multiple_of(per_byte) {
                let idx: u16 = elapsed / per_byte;
                if idx < 0xA0 {
                    // Clear `last_cpu_pc` to avoid attributing DMA-originated reads
                    // to the last CPU instruction that happened to run. DMA
                    // engine operations are independent of the CPU and should
                    // not surface a CPU PC in logs.
                    #[cfg(feature = "ppu-trace")]
                    {
                        self.last_cpu_pc = None;
                    }
                    let byte = self.dma_read_byte(self.dma_source.wrapping_add(idx));
                    self.ppu.oam_dma_write = Some((idx as u8, byte));
                    self.ppu.oam[idx as usize] = byte;
                }
            }

            self.dma_cycles -= 1;
        }
    }

    /// Whether a DMA transfer is in progress.
    pub fn dma_active(&self) -> bool {
        self.dma_cycles > 0 || self.pending_delay > 0
    }

    /// Whether a General DMA stall is in progress.
    pub fn gdma_active(&self) -> bool {
        self.gdma_cycles > 0
    }

    /// Advances the GDMA stall countdown by the given number of m-cycles.
    pub fn gdma_step(&mut self, cycles: u16) {
        if self.gdma_cycles > 0 {
            self.gdma_cycles = self.gdma_cycles.saturating_sub(cycles as u32);
        }
    }

    #[inline]
    fn sanitize_vram_dma_dest(addr: u16) -> u16 {
        0x8000 | (addr & 0x1FF0)
    }

    /// Perform a General DMA transfer immediately, consuming CPU cycles.
    fn start_gdma(&mut self, blocks: u8) {
        let total_bytes = blocks as usize * 0x10;
        let mut src = self.hdma.src;
        let mut dst = Self::sanitize_vram_dma_dest(self.hdma.dst);

        // Clear last_cpu_pc so these DMA-driven reads/writes are not
        // misattributed to the last executing CPU instruction in logs.
        #[cfg(feature = "ppu-trace")]
        {
            self.last_cpu_pc = None;
        }
        for _ in 0..total_bytes {
            // Read source using the DMA-aware reader so this GDMA operation
            // can proceed even if an OAM DMA (`dma_cycles`) is active.
            let byte = self.dma_read_byte(src);
            self.vram_dma_write(dst, byte);
            src = src.wrapping_add(1);
            dst = 0x8000 | ((dst.wrapping_add(1)) & 0x1FFF);
        }

        self.hdma.src = src;
        self.hdma.dst = Self::sanitize_vram_dma_dest(dst);
        self.hdma.active = false;
        self.hdma.blocks = 0;
        self.hdma.cancelled = false;
        self.gdma_cycles = blocks as u32 * self.hdma_block_cycle_cost();
    }

    /// Execute a single 0x10-byte HDMA burst during H-Blank.
    pub fn hdma_hblank_transfer(&mut self) {
        if !(self.hdma.active && self.hdma.mode == DmaMode::Hdma) {
            return;
        }
        self.perform_hdma_block();
    }

    fn perform_hdma_block(&mut self) {
        self.hdma.dst = Self::sanitize_vram_dma_dest(self.hdma.dst);
        // Clear last_cpu_pc so HDMA transfers don't get logged with the
        // previously executing CPU PC.
        #[cfg(feature = "ppu-trace")]
        {
            self.last_cpu_pc = None;
        }
        for _ in 0..0x10 {
            // HDMA source reads should also bypass the DMA blocking checks
            // so HDMA can transfer data even if an OAM DMA is currently
            // active.
            let byte = self.dma_read_byte(self.hdma.src);
            self.vram_dma_write(self.hdma.dst, byte);
            self.hdma.src = self.hdma.src.wrapping_add(1);
            self.hdma.dst = 0x8000 | ((self.hdma.dst.wrapping_add(1)) & 0x1FFF);
        }

        self.hdma.blocks = self.hdma.blocks.saturating_sub(1);
        if self.hdma.blocks == 0 {
            self.hdma.active = false;
            self.hdma.cancelled = false;
        }

        self.hdma.dst = Self::sanitize_vram_dma_dest(self.hdma.dst);
        self.gdma_cycles += self.hdma_block_cycle_cost();
    }

    fn complete_active_hdma(&mut self) {
        while self.hdma.active && self.hdma.mode == DmaMode::Hdma {
            self.perform_hdma_block();
        }
    }

    fn hdma_block_cycle_cost(&self) -> u32 {
        if self.key1 & 0x80 != 0 { 16 } else { 8 }
    }

    pub fn reset_div(&mut self) {
        // rDIV reset affects the CPU divider (timer.div). The PPU/APU dot clock
        // domain keeps running and is not reset by writes to FF04.
        let prev_div = self.timer.div;

        // DIV/TIMA are derived from the CPU clock domain.
        self.timer.reset_div(&mut self.if_reg);

        let double_speed = self.key1 & 0x80 != 0;
        self.apu.on_div_reset(prev_div, double_speed);
    }

    fn tick(&mut self, m_cycles: u32) {
        let dot_cycles = if self.key1 & 0x80 != 0 {
            2 * m_cycles as u16
        } else {
            4 * m_cycles as u16
        };

        let prev_dot_div = self.dot_div;
        self.dot_div = self.dot_div.wrapping_add(dot_cycles);
        let curr_dot_div = self.dot_div;

        // CPU clock cycles: always 4 cycles per M-cycle regardless of CGB speed.
        let cpu_cycles = 4u16.saturating_mul(m_cycles as u16);

        self.timer.step(cpu_cycles, &mut self.if_reg);
        // Advance 2 MHz domain before 1 MHz staging to match APU internal ordering
        self.apu.step(dot_cycles);
        self.apu
            .tick(prev_dot_div, curr_dot_div, self.key1 & 0x80 != 0);
        let _ = self.ppu.step(dot_cycles, &mut self.if_reg);
    }
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}
