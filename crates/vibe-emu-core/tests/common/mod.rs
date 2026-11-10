mod bootrom_download;

use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
struct CachedPng {
    width: u32,
    height: u32,
    pixels: Arc<[[u8; 3]]>,
}

static PNG_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedPng>>> = OnceLock::new();
static HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

static INIT: OnceCell<()> = OnceCell::new();

// Share the selected hardware capture with the assertion. Otherwise a warm
// local cache can hide a missing fixture that fails on a clean CI runner.
pub const DAID_SCANLINE_BGP_DMG_PNG: &str = "daid/ppu_scanline_bgp_1.dmg.png";

// Keep retries bounded to avoid overly long test startup while still handling transient outages.
const DOWNLOAD_ATTEMPTS: usize = 5;
const INITIAL_BACKOFF_MS: u64 = 500;
const MAX_BACKOFF_MS: u64 = 30_000;

fn http_client() -> &'static reqwest::blocking::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .http1_only()
            .build()
            .expect("failed to create HTTP client")
    })
}

fn download_bytes(url: &str) -> Vec<u8> {
    try_download_bytes(url).unwrap_or_else(|error| panic!("{error}"))
}

fn try_download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut last_error = String::new();

    for attempt in 1..=DOWNLOAD_ATTEMPTS {
        let response = http_client().get(url).send();
        match response {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.bytes() {
                        Ok(bytes) => return Ok(bytes.to_vec()),
                        Err(err) => {
                            last_error = format!("failed to read response body: {err}");
                        }
                    }
                } else {
                    last_error = format!("unexpected status {status}");
                }
            }
            Err(err) => {
                last_error = err.to_string();
            }
        }

        if attempt < DOWNLOAD_ATTEMPTS {
            let backoff_ms = (INITIAL_BACKOFF_MS << (attempt - 1)).min(MAX_BACKOFF_MS);
            thread::sleep(Duration::from_millis(backoff_ms));
        }
    }

    Err(format!(
        "failed to download {url} after {DOWNLOAD_ATTEMPTS} attempts: {last_error}"
    ))
}

fn ensure_test_roms() {
    INIT.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_roms");
        fs::create_dir_all(&dir).expect("failed to create test_roms directory");

        ensure_c_sp_test_rom_bundle(&dir);
        ensure_daid_test_roms(&dir);
        ensure_bullygb_test_roms(&dir);
        ensure_hacktix_test_roms(&dir);
        ensure_gb_emulator_shootout_cpp_test_roms(&dir);
        ensure_little_things_test_roms(&dir);
    });
}

fn ensure_little_things_test_roms(dir: &Path) {
    let base = dir.join("little-things-gb");
    for (release, name) in [
        ("Double-halt-cancel-v1.0", "double-halt-cancel.gb"),
        ("Double-halt-cancel-v1.0", "double-halt-cancel-gbconly.gb"),
        ("Win-desync-v1.0", "windesync-validate.gb"),
    ] {
        let path = base.join(name);
        if !path.exists() {
            download_file(
                &format!(
                    "https://github.com/nitro2k01/little-things-gb/releases/download/{release}/{name}"
                ),
                &path,
            );
        }
    }
    let reference = base.join("windesync-reference-sgb.png");
    if !reference.exists() {
        download_file(
            "https://raw.githubusercontent.com/nitro2k01/little-things-gb/ce015ca2949b4db2babd03fe387b8b1999a8f60a/windesync-validate/images/windesync-reference-sgb.png",
            &reference,
        );
    }
}

fn ensure_gb_emulator_shootout_cpp_test_roms(dir: &Path) {
    // Keep these under test_roms/gbeshootout/ to avoid collisions with the c-sp bundle.
    let base = dir.join("gbeshootout").join("cpp");

    let rom_path = base.join("rtc-invalid-banks-test.gb");
    let png_path = base.join("rtc-invalid-banks-test.png");

    if !rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/cpp/rtc-invalid-banks-test.gb",
            &rom_path,
        );
    }
    if !png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/cpp/rtc-invalid-banks-test.png",
            &png_path,
        );
    }

    let latch_rom_path = base.join("latch-rtc-test.gb");
    let latch_png_path = base.join("latch-rtc-test.png");

    if !latch_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/cpp/latch-rtc-test.gb",
            &latch_rom_path,
        );
    }
    if !latch_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/cpp/latch-rtc-test.png",
            &latch_png_path,
        );
    }
}

fn ensure_hacktix_test_roms(dir: &Path) {
    // Upstream renamed Hacktix to Ashiepaws. Keep the existing local cache path,
    // but fetch from the new upstream path at the same pinned revision as the
    // other GBEmulatorShootout assets, so future moves cannot break clean CI.
    let base = dir.join("hacktix");

    let rom_path = base.join("strikethrough.gb");
    let png_path = base.join("strikethrough.png");

    if !rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/ashiepaws/strikethrough.gb",
            &rom_path,
        );
    }
    if !png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/ashiepaws/strikethrough.png",
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
        && dir.join("mooneye-test-suite-wilbertpol").exists()
        && dir.join("age-test-roms").exists()
        && dir.join("gbmicrotest").exists()
        && dir.join("scribbltests").exists()
        && dir.join("turtle-tests").exists()
        && dir.join("mbc3-tester").exists()
        && dir.join("rtc3test").exists()
        && dir.join("bully").exists()
        && dir.join("gambatte").exists();
    if has_core_tree {
        return;
    }

    let url = "https://github.com/c-sp/game-boy-test-roms/releases/download/v7.0/game-boy-test-roms-v7.0.zip";
    let bytes = download_bytes(url);
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).expect("failed to open zip archive");
    archive.extract(dir).expect("failed to extract test roms");
}

fn download_file(url: &str, dest: &Path) {
    let bytes = download_bytes(url);
    write_downloaded_bytes(&bytes, dest);
}

fn write_downloaded_bytes(bytes: &[u8], dest: &Path) {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).expect("failed to create destination directory");
    }

    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, bytes).expect("failed to write temporary file");
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
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/speed_switch_timing_div.gbc",
            &rom_path,
        );
    }
    if !png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/speed_switch_timing_div.png",
            &png_path,
        );
    }

    let ly_rom_path = base.join("speed_switch_timing_ly.gbc");
    if !ly_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/speed_switch_timing_ly.gbc",
            &ly_rom_path,
        );
    }

    let stat_rom_path = base.join("speed_switch_timing_stat.gbc");
    let stat_png_path = base.join("speed_switch_timing_stat.png");
    if !stat_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/speed_switch_timing_stat.gbc",
            &stat_rom_path,
        );
    }
    if !stat_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/speed_switch_timing_stat.png",
            &stat_png_path,
        );
    }

    let stop_rom_path = base.join("stop_instr.gb");
    let stop_dmg_png_path = base.join("stop_instr.dmg.png");
    let stop_cgb_png_path = base.join("stop_instr.gbc.png");
    if !stop_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/stop_instr.gb",
            &stop_rom_path,
        );
    }
    if !stop_dmg_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/stop_instr.dmg.png",
            &stop_dmg_png_path,
        );
    }
    if !stop_cgb_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/stop_instr.gbc.png",
            &stop_cgb_png_path,
        );
    }

    let stop_mode3_rom_path = base.join("stop_instr_gbc_mode3.gb");
    let stop_mode3_png_path = base.join("stop_instr_gbc_mode3.png");
    if !stop_mode3_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/stop_instr_gbc_mode3.gb",
            &stop_mode3_rom_path,
        );
    }
    if !stop_mode3_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/stop_instr_gbc_mode3.png",
            &stop_mode3_png_path,
        );
    }

    let scanline_bgp_rom_path = base.join("ppu_scanline_bgp.gb");
    let scanline_bgp_gbc_png_path = base.join("ppu_scanline_bgp.gbc.png");
    let scanline_bgp_dmg_png_path = dir.join(DAID_SCANLINE_BGP_DMG_PNG);
    if !scanline_bgp_rom_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/ppu_scanline_bgp.gb",
            &scanline_bgp_rom_path,
        );
    }
    if !scanline_bgp_gbc_png_path.exists() {
        download_file(
            "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/daid/ppu_scanline_bgp.gbc.png",
            &scanline_bgp_gbc_png_path,
        );
    }
    if !scanline_bgp_dmg_png_path.exists() {
        download_file(
            &format!(
                "https://raw.githubusercontent.com/gbdev/GBEmulatorShootout/38b926bdbc26993d1b4c43e97979ecc66287bf02/testroms/{DAID_SCANLINE_BGP_DMG_PNG}"
            ),
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
pub fn test_assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/reference_screenshots")
}

#[allow(dead_code)]
pub fn bootroms_dir() -> PathBuf {
    let dir = roms_dir().join("bootroms");
    fs::create_dir_all(&dir).expect("failed to create bootroms directory");
    dir
}

fn ensure_bootrom(filename: &str) -> PathBuf {
    // The original gbdev.gg8.se host can be unavailable on clean CI runners.
    // Pin the mirror revision AND the original published SHA-256 values from
    // https://gbdev.gg8.se/files/roms/bootroms/readme.txt. These are identical
    // hardware dumps, not replacement boot ROMs with different startup timing.
    let (remote_path, expected) = match filename {
        "dmg_boot.bin" => (
            "Game%20Boy/GB_dmg.bin",
            "cf053eccb4ccafff9e67339d4e78e98dce7d1ed59be819d2a1ba2232c6fce1c7",
        ),
        "dmg0_boot.bin" => (
            "Game%20Boy/GB_dmg0.bin",
            "26e71cf01e301e5dc40e987cd2ecbf6d0276245890ac829db2a25323da86818e",
        ),
        "mgb_boot.bin" => (
            "Game%20Boy/GB_mgb.bin",
            "a8cb5f4f1f16f2573ed2ecd8daedb9c5d1dd2c30a481f9b179b5d725d95eafe2",
        ),
        "sgb_boot.bin" => (
            "Game%20Boy/GB_sgb.bin",
            "0e4ddff32fc9d1eeaae812a157dd246459b00c9e14f2f61751f661f32361e360",
        ),
        "sgb2_boot.bin" => (
            "Game%20Boy/GB_sgb2.bin",
            "fd243c4fb27008986316ce3df29e9cfbcdc0cd52704970555a8bb76edbec3988",
        ),
        "cgb_boot.bin" => (
            "Game%20Boy%20Color/GBC_cgb.bin",
            "b4f2e416a35eef52cba161b159c7c8523a92594facb924b3ede0d722867c50c7",
        ),
        "cgb0_boot.bin" => (
            "Game%20Boy%20Color/GBC_cgb0.bin",
            "3a307a41689bee99a9a32ea021bf45136906c86b2e4f06c806738398e4f92e45",
        ),
        "cgb_agb0_boot.bin" => (
            "Game%20Boy%20Color/GBC_agb0.bin",
            "fe2d45405531756d87622abde6127c804bd675cb968081b2c052497a470ffeb2",
        ),
        "cgb_agb_boot.bin" => (
            "Game%20Boy%20Color/GBC_agb.bin",
            "fe3cceb79930c4cb6c6f62f742c2562fd4c96b827584ef8ea89d49b387bd6860",
        ),
        _ => panic!("unknown boot ROM fixture: {filename}"),
    };
    // Several tests can request the same missing boot ROM at once. Keep
    // verification and atomic cache publication together within this process.
    static BOOT_ROM_LOCK: Mutex<()> = Mutex::new(());
    let _guard = BOOT_ROM_LOCK.lock().expect("boot ROM cache lock poisoned");
    let path = bootroms_dir().join(filename);
    let cached = path.exists();
    let bytes = if cached {
        fs::read(&path).expect("failed to read cached boot ROM")
    } else {
        bootrom_download::download_verified(
            &format!("https://gbdev.gg8.se/files/roms/bootroms/{filename}"),
            &format!("https://raw.githubusercontent.com/abdess/retrobios/e4f8bb336a35e2869de4b6039e79cca4deb35643/bios/Nintendo/{remote_path}"),
            expected,
            try_download_bytes,
        ).unwrap_or_else(|error| panic!("failed to fetch {filename}: {error}"))
    };
    bootrom_download::verify(&bytes, expected)
        .unwrap_or_else(|error| panic!("{filename}: {error}"));
    if !cached {
        write_downloaded_bytes(&bytes, &path);
    }
    path
}

#[allow(dead_code)]
pub fn dmg_boot_rom_path() -> PathBuf {
    ensure_bootrom("dmg_boot.bin")
}

#[allow(dead_code)]
pub fn cgb_boot_rom_path() -> PathBuf {
    ensure_bootrom("cgb_boot.bin")
}

/// Fetch the boot ROM matching the actual silicon/boot revision.
#[allow(dead_code)]
pub fn model_boot_rom_path(model: vibe_emu_core::hardware::Model) -> PathBuf {
    use vibe_emu_core::hardware::{CgbRevision, DmgRevision, Model};
    let filename = match model {
        Model::Dmg(DmgRevision::Rev0) => "dmg0_boot.bin",
        Model::Dmg(_) => "dmg_boot.bin",
        Model::Cgb(CgbRevision::Rev0) => "cgb0_boot.bin",
        Model::Cgb(_) => "cgb_boot.bin",
        Model::Mgb => "mgb_boot.bin",
        Model::Sgb => "sgb_boot.bin",
        Model::Sgb2 => "sgb2_boot.bin",
        Model::Agb0 => "cgb_agb0_boot.bin",
        Model::Agb => "cgb_agb_boot.bin",
    };
    ensure_bootrom(filename)
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

    let file = File::open(path)
        .unwrap_or_else(|error| panic!("failed to open reference PNG {}: {error}", path.display()));
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
            for chunk in data.as_chunks::<3>().0 {
                pixels.push([chunk[0], chunk[1], chunk[2]]);
            }
        }
        png::ColorType::Rgba => {
            for chunk in data.as_chunks::<4>().0 {
                pixels.push([chunk[0], chunk[1], chunk[2]]);
            }
        }
        png::ColorType::Grayscale => {
            for &gray in data {
                pixels.push([gray, gray, gray]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for chunk in data.as_chunks::<2>().0 {
                let g = chunk[0];
                pixels.push([g, g, g]);
            }
        }
        png::ColorType::Indexed => {
            if data.len() == pixel_count * 3 {
                for chunk in data.as_chunks::<3>().0 {
                    pixels.push([chunk[0], chunk[1], chunk[2]]);
                }
            } else if data.len() == pixel_count * 4 {
                for chunk in data.as_chunks::<4>().0 {
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
