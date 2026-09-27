//! Launch the real executable: unit-test threads have a larger stack than the
//! Windows main thread and did not catch the startup stack overflow.
use std::{
    fs,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[path = "../../vibe-emu-core/tests/common/sgb_rom.rs"]
mod sgb_rom;

#[test]
fn executable_starts_each_hardware_and_hybrid_mode() {
    let directory = std::env::temp_dir().join(format!(
        "vibeemu-startup-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let rom = directory.join("startup.gbc");
    fs::write(&rom, sgb_rom::initial_border_rom()).unwrap();

    for model in [
        "dmg",
        "mgb",
        "sgb",
        "sgb2",
        "cgb",
        "agb0",
        "agb",
        "sgb-cgb",
        "cgb-sgb-border",
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vibe-emu-ui"));
        command
            .arg(&rom)
            .args(["--headless", "--frames", "1", "--model", model])
            // Keep local boot ROM preferences out of this deterministic test.
            .env("APPDATA", &directory)
            .env("XDG_CONFIG_HOME", &directory)
            .current_dir(&directory)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("{model}: startup did not finish within 30 seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{model}: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_file(rom).unwrap();
    fs::remove_dir(directory).unwrap();
}
