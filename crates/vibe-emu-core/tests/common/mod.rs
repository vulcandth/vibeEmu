use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
struct CachedPng {
    width: u32,
    height: u32,
    pixels: Arc<[[u8; 3]]>,
}

static PNG_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedPng>>> = OnceLock::new();

static INIT: OnceCell<()> = OnceCell::new();

fn ensure_test_roms() {
    INIT.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_roms");
        fs::create_dir_all(&dir).expect("failed to create test_roms directory");

        ensure_c_sp_test_rom_bundle(&dir);
        ensure_daid_test_roms(&dir);
        ensure_bullygb_test_roms(&dir);
        ensure_hacktix_test_roms(&dir);
        ensure_gb_emulator_shootout_cpp_test_roms(&dir);
    });
}

fn ensure_gb_emulator_shootout_cpp_test_roms(dir: &Path) {
    // Keep these under test_roms/gbeshootout/ to avoid collisions with the c-sp bundle.
    let base = dir.join("gbeshootout").join("cpp");

    let rom_path = base.join("rtc-invalid-banks-test.gb");
    let png_path = base.join("rtc-invalid-banks-test.png");

    if !rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/cpp/rtc-invalid-banks-test.gb",
            &rom_path,
        );
    }
    if !png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/cpp/rtc-invalid-banks-test.png",
            &png_path,
        );
    }

    let latch_rom_path = base.join("latch-rtc-test.gb");
    let latch_png_path = base.join("latch-rtc-test.png");

    if !latch_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/cpp/latch-rtc-test.gb",
            &latch_rom_path,
        );
    }
    if !latch_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/cpp/latch-rtc-test.png",
            &latch_png_path,
        );
    }
}

fn ensure_hacktix_test_roms(dir: &Path) {
    // Hacktix ROMs are hosted in GBEmulatorShootout and not included in the c-sp bundle.
    // Keep them under test_roms/hacktix/ to avoid collisions.
    let base = dir.join("hacktix");

    let rom_path = base.join("strikethrough.gb");
    let png_path = base.join("strikethrough.png");

    if !rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/hacktix/strikethrough.gb",
            &rom_path,
        );
    }
    if !png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/hacktix/strikethrough.png",
            &png_path,
        );
    }
}

fn ensure_bullygb_test_roms(dir: &Path) {
    // BullyGB is hosted separately from the c-sp suite bundle.
    // Keep it under test_roms/bullygb/ to avoid collisions.
    let base = dir.join("bullygb");
    let rom_path = base.join("bullygb-v1.2.gb");

    if !rom_path.exists() {
        download_file(
            "https://github.com/Ashiepaws/BullyGB/releases/download/v1.2/bully.gb",
            &rom_path,
        );
    }
}

fn ensure_c_sp_test_rom_bundle(dir: &Path) {
    // The repository intentionally does not check in ROM binaries; CI/dev machines
    // download a known bundle on-demand.
    // If the directory already contains extracted content, don't re-download.
    let has_core_tree = dir.join("blargg").exists()
        && dir.join("mooneye-test-suite").exists()
        && dir.join("gambatte").exists();
    if has_core_tree {
        return;
    }

    let url = "https://github.com/c-sp/game-boy-test-roms/releases/download/v7.0/game-boy-test-roms-v7.0.zip";
    let resp = reqwest::blocking::get(url).expect("failed to download test roms");
    let status = resp.status();
    if !status.is_success() {
        panic!("failed to download test roms: {status}");
    }
    let bytes = resp.bytes().expect("failed to read rom bytes");
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).expect("failed to open zip archive");
    archive.extract(dir).expect("failed to extract test roms");
}

fn download_file(url: &str, dest: &Path) {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).expect("failed to create destination directory");
    }

    let resp = reqwest::blocking::get(url).expect("failed to download file");
    let status = resp.status();
    if !status.is_success() {
        panic!("failed to download {url}: {status}");
    }

    let bytes = resp.bytes().expect("failed to read response body");
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, &bytes).expect("failed to write temporary file");
    fs::rename(&tmp, dest).unwrap_or_else(|_| {
        // On Windows rename can fail if the destination already exists.
        let _ = fs::remove_file(dest);
        fs::rename(&tmp, dest).expect("failed to move downloaded file into place")
    });
}

fn ensure_daid_test_roms(dir: &Path) {
    // Daid's ROMs are hosted in the GBEmulatorShootout repo and aren't included
    // in the c-sp bundle we download above.
    //
    // Keep these downloaded ROMs under test_roms/daid/ so they don't collide with
    // other suites.
    let base = dir.join("daid");

    let rom_path = base.join("speed_switch_timing_div.gbc");
    let png_path = base.join("speed_switch_timing_div.png");

    if !rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/speed_switch_timing_div.gbc",
            &rom_path,
        );
    }
    if !png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/speed_switch_timing_div.png",
            &png_path,
        );
    }

    let ly_rom_path = base.join("speed_switch_timing_ly.gbc");
    if !ly_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/speed_switch_timing_ly.gbc",
            &ly_rom_path,
        );
    }

    let stat_rom_path = base.join("speed_switch_timing_stat.gbc");
    let stat_png_path = base.join("speed_switch_timing_stat.png");
    if !stat_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/speed_switch_timing_stat.gbc",
            &stat_rom_path,
        );
    }
    if !stat_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/speed_switch_timing_stat.png",
            &stat_png_path,
        );
    }

    let stop_rom_path = base.join("stop_instr.gb");
    let stop_dmg_png_path = base.join("stop_instr.dmg.png");
    let stop_cgb_png_path = base.join("stop_instr.gbc.png");
    if !stop_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/stop_instr.gb",
            &stop_rom_path,
        );
    }
    if !stop_dmg_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/stop_instr.dmg.png",
            &stop_dmg_png_path,
        );
    }
    if !stop_cgb_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/daid/GBEmulatorShootout/main/testroms/daid/stop_instr.gbc.png",
            &stop_cgb_png_path,
        );
    }

    let stop_mode3_rom_path = base.join("stop_instr_gbc_mode3.gb");
    let stop_mode3_png_path = base.join("stop_instr_gbc_mode3.png");
    if !stop_mode3_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/daid/stop_instr_gbc_mode3.gb",
            &stop_mode3_rom_path,
        );
    }
    if !stop_mode3_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/daid/stop_instr_gbc_mode3.png",
            &stop_mode3_png_path,
        );
    }

    let scanline_bgp_rom_path = base.join("ppu_scanline_bgp.gb");
    let scanline_bgp_gbc_png_path = base.join("ppu_scanline_bgp.gbc.png");
    let scanline_bgp_dmg_png_path = base.join("ppu_scanline_bgp_0.dmg.png");
    if !scanline_bgp_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/daid/ppu_scanline_bgp.gb",
            &scanline_bgp_rom_path,
        );
    }
    if !scanline_bgp_gbc_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/daid/ppu_scanline_bgp.gbc.png",
            &scanline_bgp_gbc_png_path,
        );
    }
    if !scanline_bgp_dmg_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/vulcandth/GBEmulatorShootout/main/testroms/daid/ppu_scanline_bgp_0.dmg.png",
            &scanline_bgp_dmg_png_path,
        );
    }
}

pub fn roms_dir() -> PathBuf {
    ensure_test_roms();
    Path::new(env!("CARGO_MANIFEST_DIR")).join("test_roms")
}

#[allow(dead_code)]
pub fn rom_path<P: AsRef<Path>>(relative: P) -> PathBuf {
    roms_dir().join(relative)
}

#[allow(dead_code)]
pub fn workspace_root() -> PathBuf {
    let mut ancestors = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors();
    // current dir
    ancestors.next();
    // crates/vibe-emu-core
    let crates_dir = ancestors
        .next()
        .expect("crate directory should have a parent");
    // workspace root
    ancestors.next().unwrap_or(crates_dir).to_path_buf()
}

#[allow(dead_code)]
pub fn bootroms_dir() -> PathBuf {
    let dir = workspace_root().join("bootroms");
    fs::create_dir_all(&dir).expect("failed to create bootroms directory");
    dir
}

fn ensure_bootrom(url: &str, filename: &str) -> PathBuf {
    let path = bootroms_dir().join(filename);
    if !path.exists() {
        download_file(url, &path);
    }
    path
}

#[allow(dead_code)]
pub fn dmg_boot_rom_path() -> PathBuf {
    ensure_bootrom(
        "https://gbdev.gg8.se/files/roms/bootroms/dmg_boot.bin",
        "dmg_boot.bin",
    )
}

#[allow(dead_code)]
pub fn cgb_boot_rom_path() -> PathBuf {
    ensure_bootrom(
        "https://gbdev.gg8.se/files/roms/bootroms/cgb_boot.bin",
        "cgb_boot.bin",
    )
}

#[allow(dead_code)]
pub fn load_png_rgb<P: AsRef<Path>>(path: P) -> (u32, u32, Arc<[[u8; 3]]>) {
    let path = path.as_ref();

    let cache = PNG_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .expect("failed to lock PNG cache")
        .get(path)
        .cloned()
    {
        return (cached.width, cached.height, Arc::clone(&cached.pixels));
    }

    let file = File::open(path).expect("failed to open png");
    let reader = BufReader::new(file);
    let mut decoder = png::Decoder::new(reader);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut png_reader = decoder.read_info().expect("failed to read png info");
    let buffer_size = png_reader
        .output_buffer_size()
        .expect("failed to get png output buffer size");
    let mut buf = vec![0; buffer_size];
    let info = png_reader
        .next_frame(&mut buf)
        .expect("failed to decode png frame");
    let data = &buf[..info.buffer_size()];
    let pixel_count = (info.width as usize) * (info.height as usize);
    let mut pixels = Vec::with_capacity(pixel_count);
    match png_reader.info().color_type {
        png::ColorType::Rgb => {
            for chunk in data.chunks_exact(3) {
                pixels.push([chunk[0], chunk[1], chunk[2]]);
            }
        }
        png::ColorType::Rgba => {
            for chunk in data.chunks_exact(4) {
                pixels.push([chunk[0], chunk[1], chunk[2]]);
            }
        }
        png::ColorType::Grayscale => {
            for &gray in data {
                pixels.push([gray, gray, gray]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for chunk in data.chunks_exact(2) {
                let g = chunk[0];
                pixels.push([g, g, g]);
            }
        }
        png::ColorType::Indexed => {
            if data.len() == pixel_count * 3 {
                for chunk in data.chunks_exact(3) {
                    pixels.push([chunk[0], chunk[1], chunk[2]]);
                }
            } else if data.len() == pixel_count * 4 {
                for chunk in data.chunks_exact(4) {
                    pixels.push([chunk[0], chunk[1], chunk[2]]);
                }
            } else {
                panic!("unexpected palette expansion");
            }
        }
    }
    let pixels: Arc<[[u8; 3]]> = Arc::from(pixels);

    let cached = CachedPng {
        width: info.width,
        height: info.height,
        pixels: Arc::clone(&pixels),
    };

    let mut cache_guard = cache.lock().expect("failed to lock PNG cache");
    if let Some(existing) = cache_guard.get(path) {
        return (
            existing.width,
            existing.height,
            Arc::clone(&existing.pixels),
        );
    }
    cache_guard.insert(path.to_path_buf(), cached);

    (info.width, info.height, pixels)
}

#[allow(dead_code)]
pub fn serial_contains_result(serial: &[u8], checked_up_to: &mut usize) -> bool {
    const PASSED: &[u8] = b"Passed";
    const FAILED: &[u8] = b"Failed";

    let max_marker_len = PASSED.len().max(FAILED.len());
    let lookbehind = max_marker_len.saturating_sub(1);
    let start = checked_up_to.saturating_sub(lookbehind).min(serial.len());
    let window = &serial[start..];

    let found = window.windows(PASSED.len()).any(|chunk| chunk == PASSED)
        || window.windows(FAILED.len()).any(|chunk| chunk == FAILED);

    *checked_up_to = serial.len();
    found
}
