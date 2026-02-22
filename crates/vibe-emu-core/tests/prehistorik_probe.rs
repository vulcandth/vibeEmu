use std::path::{Path, PathBuf};

use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision},
};

const DEFAULT_ROM: &str =
    "C:/Users/matth/Downloads/Prehistorik Man (USA, Europe)/Prehistorik Man (USA, Europe).gb";
const SCREEN_W: u32 = 160;
const SCREEN_H: u32 = 144;

fn env_str(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_u32(name: &str, default: u32) -> u32 {
    env_str(name)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    match env_str(name).as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES") => true,
        Some("0") | Some("false") | Some("FALSE") | Some("no") | Some("NO") => false,
        Some(_) => default,
        None => default,
    }
}

fn parse_dmg_revision() -> DmgRevision {
    match env_str("VIBEEMU_PROBE_DMG_REV")
        .unwrap_or_else(|| "c".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "0" | "rev0" => DmgRevision::Rev0,
        "a" | "reva" => DmgRevision::RevA,
        "b" | "revb" => DmgRevision::RevB,
        "c" | "revc" | "blob" | "default" => DmgRevision::RevC,
        other => {
            panic!("invalid VIBEEMU_PROBE_DMG_REV='{other}', expected one of: 0,a,b,c,blob,default")
        }
    }
}

fn parse_cgb_revision() -> CgbRevision {
    match env_str("VIBEEMU_PROBE_CGB_REV")
        .unwrap_or_else(|| "e".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "0" | "rev0" => CgbRevision::Rev0,
        "a" | "reva" => CgbRevision::RevA,
        "b" | "revb" => CgbRevision::RevB,
        "c" | "revc" => CgbRevision::RevC,
        "d" | "revd" => CgbRevision::RevD,
        "e" | "reve" | "default" => CgbRevision::RevE,
        other => {
            panic!("invalid VIBEEMU_PROBE_CGB_REV='{other}', expected one of: 0,a,b,c,d,e,default")
        }
    }
}

fn frame_to_rgb(frame: &[u32]) -> Vec<u8> {
    let mut out = vec![0u8; frame.len() * 3];
    for (i, &px) in frame.iter().enumerate() {
        out[i * 3] = ((px >> 16) & 0xFF) as u8;
        out[i * 3 + 1] = ((px >> 8) & 0xFF) as u8;
        out[i * 3 + 2] = (px & 0xFF) as u8;
    }
    out
}

fn write_png_rgb(path: &Path, width: u32, height: u32, rgb: &[u8]) {
    assert_eq!(rgb.len(), (width * height * 3) as usize);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::File::create(path).expect("failed to create png file");
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("failed to write png header");
    writer
        .write_image_data(rgb)
        .expect("failed to write png data");
}

fn should_dump_frame(frame_idx: u32, dump_start: Option<u32>, dump_end: Option<u32>) -> bool {
    match (dump_start, dump_end) {
        (Some(s), Some(e)) => frame_idx >= s && frame_idx <= e,
        _ => false,
    }
}

#[test]
#[ignore = "manual probe; run with explicit env vars"]
fn prehistorik_capture_probe() {
    let rom_path =
        PathBuf::from(env_str("VIBEEMU_PROBE_ROM").unwrap_or_else(|| DEFAULT_ROM.into()));
    if !rom_path.exists() {
        panic!(
            "ROM not found at '{}'. Set VIBEEMU_PROBE_ROM to an absolute path.",
            rom_path.display()
        );
    }

    let mode = env_str("VIBEEMU_PROBE_MODE").unwrap_or_else(|| "dmg".into());
    let cgb_mode = mode.eq_ignore_ascii_case("cgb");

    let dmg_rev = parse_dmg_revision();
    let cgb_rev = parse_cgb_revision();

    let frames_to_run = env_u32("VIBEEMU_PROBE_FRAMES", 60 * 180);
    let dump_start = env_str("VIBEEMU_PROBE_DUMP_START").and_then(|v| v.parse::<u32>().ok());
    let dump_end = env_str("VIBEEMU_PROBE_DUMP_END").and_then(|v| v.parse::<u32>().ok());
    let dump_final = env_bool("VIBEEMU_PROBE_DUMP_FINAL", true);
    let dump_dir = PathBuf::from(
        env_str("VIBEEMU_PROBE_OUT_DIR").unwrap_or_else(|| "target/tmp/prehistorik-probe".into()),
    );
    let bootrom_path = env_str("VIBEEMU_PROBE_BOOTROM").map(PathBuf::from);

    println!(
        "probe config: rom='{}' mode={} dmg_rev={:?} cgb_rev={:?} frames={} dump_start={:?} dump_end={:?} dump_final={} out='{}' bootrom={}",
        rom_path.display(),
        if cgb_mode { "cgb" } else { "dmg" },
        dmg_rev,
        cgb_rev,
        frames_to_run,
        dump_start,
        dump_end,
        dump_final,
        dump_dir.display(),
        bootrom_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".into())
    );

    let rom = std::fs::read(&rom_path).expect("failed to read ROM");
    let cart = Cartridge::load(rom);

    let mut gb = if bootrom_path.is_some() {
        GameBoy::new_power_on_with_revisions(cgb_mode, dmg_rev, cgb_rev)
    } else {
        GameBoy::new_with_revisions(cgb_mode, dmg_rev, cgb_rev)
    };

    if let Some(path) = &bootrom_path {
        let boot = std::fs::read(path)
            .unwrap_or_else(|e| panic!("failed to read boot ROM '{}': {e}", path.display()));
        gb.mmu.load_boot_rom(boot);
    }

    gb.mmu.load_cart(cart);

    let mut completed = 0u32;
    while completed < frames_to_run {
        gb.mmu.ppu.clear_frame_flag();
        while !gb.mmu.ppu.frame_ready() {
            gb.cpu.step(&mut gb.mmu);
        }

        completed += 1;
        if should_dump_frame(completed, dump_start, dump_end) {
            let png_path = dump_dir.join(format!(
                "vibeemu_{}_frame_{:06}.png",
                if cgb_mode { "cgb" } else { "dmg" },
                completed
            ));
            let rgb = frame_to_rgb(gb.mmu.ppu.framebuffer());
            write_png_rgb(&png_path, SCREEN_W, SCREEN_H, &rgb);
        }
    }

    if dump_final {
        let png_path = dump_dir.join(format!(
            "vibeemu_{}_frame_{:06}_final.png",
            if cgb_mode { "cgb" } else { "dmg" },
            completed
        ));
        let rgb = frame_to_rgb(gb.mmu.ppu.framebuffer());
        write_png_rgb(&png_path, SCREEN_W, SCREEN_H, &rgb);
        println!("wrote {}", png_path.display());
    }

    println!("probe complete: frames={completed}");
}
