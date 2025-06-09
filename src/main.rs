// main.rs for Blargg test runner
use std::fs;
use std::path::Path;
use std::cell::RefCell;
use std::rc::Rc;

// Declare modules
mod apu;
mod bus;
mod cpu;
mod interrupts;
mod joypad;
mod mbc;
mod memory;
mod ppu;
mod timer;
// serial module might also be needed if Bus uses it directly and it's not public through Bus
// mod serial;

use crate::bus::{Bus, SystemMode};
use crate::cpu::Cpu;

pub struct TestGb {
    pub cpu: Cpu,
    pub bus: Rc<RefCell<Bus>>,
}

impl TestGb {
    pub fn new(rom_data: Vec<u8>, _mode_to_set: SystemMode) -> Self { // mode_to_set not strictly used if ROM header is correct
        let bus_rc = Rc::new(RefCell::new(Bus::new(rom_data)));
        // For DMG tests, Bus::new() should correctly determine SystemMode::DMG from ROM header.
        // We can assert this if needed:
        // assert_eq!(bus_rc.borrow().get_system_mode(), SystemMode::DMG, "ROM not detected as DMG!");
        let cpu = Cpu::new(Rc::clone(&bus_rc));
        TestGb { cpu, bus: bus_rc }
    }

    pub fn tick_cpu_and_components(&mut self) -> u32 {
        let m_cycles = self.cpu.step();
        self.bus.borrow_mut().tick_components(m_cycles);
        m_cycles
    }
}

fn main() {
    let rom_path = "roms/blargg/dmg_sound/dmg_sound.gb";
    let rom_data = match fs::read(Path::new(rom_path)) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to read ROM file: {:?}. Error: {}", rom_path, e);
            std::process::exit(1);
        }
    };

    let mut gb = TestGb::new(rom_data, SystemMode::DMG);

    if gb.bus.borrow().get_system_mode() != SystemMode::DMG {
        eprintln!(
            "Warning: ROM was not detected as DMG as expected. Detected mode: {:?}. Test results may be affected.",
            gb.bus.borrow().get_system_mode()
        );
    }

    let cycles_to_run: u64 = 35_000_000;
    let mut total_t_cycles_ran: u64 = 0;
    let mut last_instr_is_18 = false;

    loop {
        let m_cycles = gb.tick_cpu_and_components();
        let t_cycles = m_cycles * 4;
        total_t_cycles_ran += t_cycles as u64;

        let pc_before_step = gb.cpu.pc;
        let instr_at_pc = gb.bus.borrow().read_byte(pc_before_step);

        if last_instr_is_18 && instr_at_pc == 0xFE {
            break;
        }
        last_instr_is_18 = instr_at_pc == 0x18;

        if total_t_cycles_ran >= cycles_to_run {
            break;
        }
    }

    let serial_output = gb.bus.borrow().get_serial_output_string();
    println!("{}", serial_output);
}
