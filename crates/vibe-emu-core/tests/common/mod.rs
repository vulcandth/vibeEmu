use once_cell::sync::OnceCell;
use std::fs;
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
