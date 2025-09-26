use once_cell::sync::OnceCell;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

static INIT: OnceCell<()> = OnceCell::new();

fn ensure_test_roms() {
    INIT.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_roms");
        if dir.exists() {
            return;
        }
        fs::create_dir_all(&dir).expect("failed to create test_roms directory");
        let url = "https://github.com/c-sp/game-boy-test-roms/releases/download/v7.0/game-boy-test-roms-v7.0.zip";
        let resp = reqwest::blocking::get(url).expect("failed to download test roms");
        let status = resp.status();
        if !status.is_success() {
            panic!("failed to download test roms: {status}");
        }
        let bytes = resp.bytes().expect("failed to read rom bytes");
        let reader = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader).expect("failed to open zip archive");
        archive.extract(&dir).expect("failed to extract test roms");
    });
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
pub fn load_png_rgb<P: AsRef<Path>>(path: P) -> (u32, u32, Vec<[u8; 3]>) {
    let file = File::open(path.as_ref()).expect("failed to open png");
    let reader = BufReader::new(file);
    let mut decoder = png::Decoder::new(reader);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut png_reader = decoder.read_info().expect("failed to read png info");
    let mut buf = vec![0; png_reader.output_buffer_size()];
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
