//! Preserve panic diagnostics even in Windows GUI builds without a console.
use std::sync::Mutex;
use std::{backtrace::Backtrace, io::Write, path::PathBuf};
use vibe_emu_core::{gameboy::GameBoy, hardware::Model};

#[derive(Clone, Copy, Debug)]
struct MachineContext {
    operation: &'static str,
    model: Model,
    pc: u16,
    sp: u16,
    af_bc_de_hl: [u16; 4],
    ime: bool,
    cycles: u64,
    rom_bank: u16,
    /// Bytes starting at PC and SP, captured using passive inspection.
    instruction_bytes: [u8; 16],
    stack_bytes: [u8; 32],
    /// JOYP, IF, IE, LCDC, STAT, LY, DMA, DIV, TIMA, TAC.
    hardware: [u8; 10],
}

static HISTORY: Mutex<([Option<MachineContext>; 8], usize)> = Mutex::new(([None; 8], 0));
static ROM: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_rom(path: Option<&std::path::Path>) {
    if let Ok(mut rom) = ROM.lock() {
        *rom = path.map(std::path::Path::to_path_buf);
    }
    if let Ok(mut history) = HISTORY.lock() {
        *history = ([None; 8], 0);
    }
}

/// Bounded breadcrumbs, once per frame/step, without collecting execution traces.
/// The panic hook uses this independent buffer and never locks the emulator.
pub fn record(gb: &mut GameBoy, operation: &'static str) {
    let cpu = &gb.cpu;
    let rom_bank = gb
        .mmu
        .cart
        .as_ref()
        .map_or(0, |cart| cart.current_rom_bank());
    let mut peek = |address| gb.mmu.peek_byte(address);
    let context = MachineContext {
        operation,
        model: gb.model,
        pc: cpu.pc,
        sp: cpu.sp,
        af_bc_de_hl: [
            u16::from_be_bytes([cpu.a, cpu.f]),
            u16::from_be_bytes([cpu.b, cpu.c]),
            u16::from_be_bytes([cpu.d, cpu.e]),
            u16::from_be_bytes([cpu.h, cpu.l]),
        ],
        ime: cpu.ime,
        cycles: cpu.cycles,
        rom_bank,
        instruction_bytes: std::array::from_fn(|n| peek(cpu.pc.wrapping_add(n as u16))),
        stack_bytes: std::array::from_fn(|n| peek(cpu.sp.wrapping_add(n as u16))),
        hardware: [
            0xff00, 0xff0f, 0xffff, 0xff40, 0xff41, 0xff44, 0xff46, 0xff04, 0xff05, 0xff07,
        ]
        .map(peek),
    };
    if let Ok(mut history) = HISTORY.try_lock() {
        let index = history.1;
        history.0[index] = Some(context);
        history.1 = (index + 1) % history.0.len();
    }
}

fn diagnostics() -> String {
    let rom = ROM
        .try_lock()
        .ok()
        .map(|rom| format!("{rom:?}"))
        .unwrap_or_else(|| "unavailable".into());
    let mut result = format!("ROM: {rom}\nRecent checkpoints (oldest first; not save states):\n");
    if let Ok(history) = HISTORY.try_lock() {
        for n in 0..history.0.len() {
            if let Some(context) = history.0[(history.1 + n) % history.0.len()] {
                result.push_str(&format!("{context:#X?}\n"));
            }
        }
    } else {
        result.push_str("Checkpoint buffer unavailable\n");
    }
    result
}

pub fn install(config_path: &std::path::Path) {
    let directory = config_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("crashes");
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let thread = std::thread::current();
        let report = format!(
            "vibeEmu {}\nThread: {}\n{info}\n\n{}\n",
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
            format_args!("{}\n{}", diagnostics(), Backtrace::force_capture())
        );
        let name = format!("panic-{}-{timestamp}.log", std::process::id());
        for base in [
            directory.clone(),
            std::env::temp_dir().join("vibeemu-crashes"),
        ] {
            if write_report(base.join(&name), report.as_bytes()).is_ok() {
                break;
            }
        }
        previous(info);
    }));
}

fn write_report(path: PathBuf, report: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(report)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_probe() {
        let Some(directory) = std::env::var_os("VIBEEMU_PANIC_TEST_DIR") else {
            return;
        };
        install(&PathBuf::from(directory).join("ui.toml"));
        set_rom(Some(std::path::Path::new("diagnostic-test.gb")));
        let mut gb = GameBoy::new(Model::Sgb2);
        gb.cpu.pc = 0x7123;
        record(&mut gb, "regression panic checkpoint");
        let machine = Mutex::new(gb);
        let _held = machine.lock().unwrap();
        panic!("intentional panic probe while emulator is locked");
    }

    #[test]
    fn panic_report_contains_state_even_while_emulator_is_locked() {
        let directory =
            std::env::temp_dir().join(format!("vibeemu-panic-probe-{}", std::process::id()));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_report::tests::panic_probe", "--nocapture"])
            .env("VIBEEMU_PANIC_TEST_DIR", &directory)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let paths = std::fs::read_dir(directory.join("crashes")).unwrap();
        let mut found = false;
        for path in paths {
            let path = path.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(text.contains("intentional panic probe"));
            assert!(text.contains("diagnostic-test.gb"));
            assert!(text.contains("Sgb2"));
            assert!(text.contains("0x7123"));
            assert!(text.contains("regression panic checkpoint"));
            found = true;
            std::fs::remove_file(path).unwrap();
        }
        assert!(found);
        std::fs::remove_dir(directory.join("crashes")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn reports_never_replace_existing_evidence() {
        let path =
            std::env::temp_dir().join(format!("vibeemu-panic-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        write_report(path.clone(), b"original").unwrap();
        assert!(write_report(path.clone(), b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        std::fs::remove_file(path).unwrap();
    }
}
