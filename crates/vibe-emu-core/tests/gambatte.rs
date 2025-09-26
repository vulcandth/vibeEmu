#![allow(non_snake_case)]
mod common;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use libtest_mimic::{Arguments, Failed, Trial};
use vibe_emu_core::{
    cartridge::Cartridge,
    gameboy::GameBoy,
    hardware::{CgbRevision, DmgRevision},
};

const GB_WIDTH: usize = 160;
const GB_HEIGHT: usize = 144;
const TARGET_FRAMES: u32 = 15;
const MAX_FRAMES: u32 = 20;
const MAX_CYCLES: u64 = 2_000_000;

const IGNORED_LIST: &str = include_str!("gambatte_ignored.txt");

#[derive(Clone, Copy)]
enum Mode {
    Dmg,
    Cgb,
}

struct GambatteCase {
    rom: PathBuf,
    name: String,
    stem: String,
    dmg_out: Option<&'static str>,
    cgb_out: Option<&'static str>,
    dmg_png: Option<PathBuf>,
    cgb_png: Option<PathBuf>,
}

struct GambatteRun {
    frame: Vec<u32>,
    audio: Vec<i16>,
}

fn ignored_cases() -> &'static HashSet<&'static str> {
    static CACHE: OnceLock<HashSet<&'static str>> = OnceLock::new();
    CACHE.get_or_init(|| {
        IGNORED_LIST
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect::<HashSet<&'static str>>()
    })
}

fn should_ignore(name: &str) -> bool {
    ignored_cases().contains(name)
}

fn main() {
    let args = Arguments::from_args();

    let rom_root = common::rom_path("gambatte");
    let roms = match collect_roms(&rom_root) {
        Ok(roms) => roms,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    let mut trials = Vec::new();
    for rom in roms {
        match build_case(&rom, &rom_root) {
            Ok(Some(case)) => {
                let name = case.name.clone();
                let ignore = should_ignore(&name);
                let mut trial = Trial::test(name, move || run_case(&case).map_err(Failed::from));
                if ignore {
                    trial = trial.with_ignored_flag(true);
                }
                trials.push(trial);
            }
            Ok(None) => {}
            Err(err) => {
                let name = rom
                    .strip_prefix(&rom_root)
                    .unwrap_or(&rom)
                    .to_string_lossy()
                    .replace('\\', "/");
                let ignore = should_ignore(&name);
                let err_msg = err.clone();
                let mut trial = Trial::test(name, move || Err(Failed::from(err_msg.clone())));
                if ignore {
                    trial = trial.with_ignored_flag(true);
                }
                trials.push(trial);
            }
        }
    }

    if trials.is_empty() {
        eprintln!("no Gambatte ROMs found; run `cargo test` once to download them");
        std::process::exit(1);
    }

    libtest_mimic::run(&args, trials).exit();
}

fn build_case(rom: &Path, rom_root: &Path) -> Result<Option<GambatteCase>, String> {
    let relative = rom
        .strip_prefix(rom_root)
        .map_err(|_| format!("{} is not under {}", rom.display(), rom_root.display()))?;
    let name = relative.to_string_lossy().replace('\\', "/");
    let stem = rom
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid ROM filename: {}", rom.display()))?
        .to_owned();

    let dmg_out = detect_out_string(&stem, Mode::Dmg);
    let cgb_out = detect_out_string(&stem, Mode::Cgb);

    let png_shared = rom.with_file_name(format!("{}_dmg08_cgb04c.png", stem));
    let png_dmg = rom.with_file_name(format!("{}_dmg08.png", stem));
    let png_cgb = rom.with_file_name(format!("{}_cgb04c.png", stem));

    let shared_exists = png_shared.exists();
    let dmg_png = if shared_exists {
        Some(png_shared.clone())
    } else if png_dmg.exists() {
        Some(png_dmg.clone())
    } else {
        None
    };
    let cgb_png = if shared_exists {
        Some(png_shared)
    } else if png_cgb.exists() {
        Some(png_cgb)
    } else {
        None
    };

    if dmg_out.is_none() && dmg_png.is_none() && cgb_out.is_none() && cgb_png.is_none() {
        return Ok(None);
    }

    Ok(Some(GambatteCase {
        rom: rom.to_path_buf(),
        name,
        stem,
        dmg_out,
        cgb_out,
        dmg_png,
        cgb_png,
    }))
}

fn run_case(case: &GambatteCase) -> Result<(), String> {
    let run_dmg = case.dmg_out.is_some() || case.dmg_png.is_some();
    let run_cgb = case.cgb_out.is_some() || case.cgb_png.is_some();

    if let Some(result) = execute_mode(&case.rom, Mode::Dmg, run_dmg)? {
        if let Some(out) = case.dmg_out {
            verify_result(&result, &case.stem, out, Mode::Dmg)?;
        }
        if let Some(png) = &case.dmg_png {
            verify_png(&result, &case.stem, png, Mode::Dmg)?;
        }
    }

    if let Some(result) = execute_mode(&case.rom, Mode::Cgb, run_cgb)? {
        if let Some(out) = case.cgb_out {
            verify_result(&result, &case.stem, out, Mode::Cgb)?;
        }
        if let Some(png) = &case.cgb_png {
            verify_png(&result, &case.stem, png, Mode::Cgb)?;
        }
    }

    Ok(())
}

fn collect_roms(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut roms = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            fs::read_dir(&dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
        for entry in entries {
            let entry =
                entry.map_err(|err| format!("failed to read entry in {}: {err}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("gb") | Some("gbc")
            ) {
                roms.push(path);
            }
        }
    }
    roms.sort();
    Ok(roms)
}

fn execute_mode(rom: &Path, mode: Mode, should_run: bool) -> Result<Option<GambatteRun>, String> {
    if !should_run {
        return Ok(None);
    }

    let rom_data =
        fs::read(rom).map_err(|err| format!("failed to read {}: {err}", rom.display()))?;
    let cart = Cartridge::load(rom_data);
    let mut gb = match mode {
        Mode::Dmg => {
            GameBoy::new_with_revisions(false, DmgRevision::default(), CgbRevision::default())
        }
        Mode::Cgb => {
            GameBoy::new_with_revisions(true, DmgRevision::default(), CgbRevision::default())
        }
    };
    gb.mmu.load_cart(cart);

    let mut frames = 0u32;
    let mut frame = vec![0u32; GB_WIDTH * GB_HEIGHT];
    let start_cycles = gb.cpu.cycles;
    while frames < TARGET_FRAMES && gb.cpu.cycles - start_cycles <= MAX_CYCLES {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            frame.copy_from_slice(gb.mmu.ppu.framebuffer());
            gb.mmu.ppu.clear_frame_flag();
            frames += 1;
            if frames >= MAX_FRAMES {
                break;
            }
        }
    }

    if frames < TARGET_FRAMES {
        return Err(format!(
            "{}: {:?} mode rendered only {frames} frames before hitting the cycle limit",
            rom.display(),
            mode
        ));
    }

    let mut samples = Vec::new();
    let mut apu = gb
        .mmu
        .apu
        .lock()
        .map_err(|_| format!("failed to lock APU for {}", rom.display()))?;
    while let Some(sample) = apu.pop_sample() {
        samples.push(sample);
    }
    drop(apu);

    Ok(Some(GambatteRun {
        frame,
        audio: samples,
    }))
}

fn detect_out_string(stem: &str, mode: Mode) -> Option<&'static str> {
    if stem.contains("dmg08_cgb04c_out") {
        return Some("dmg08_cgb04c_out");
    }
    match mode {
        Mode::Dmg => {
            if stem.contains("dmg08_out") {
                Some("dmg08_out")
            } else {
                None
            }
        }
        Mode::Cgb => {
            if stem.contains("cgb04c_out") {
                Some("cgb04c_out")
            } else if stem.contains("_out") {
                Some("_out")
            } else {
                None
            }
        }
    }
}

fn verify_result(run: &GambatteRun, stem: &str, out_str: &str, mode: Mode) -> Result<(), String> {
    let out_pos = stem
        .find(out_str)
        .ok_or_else(|| format!("expected substring {out_str} in {stem}"))?;
    let tail = &stem[out_pos + out_str.len()..];

    if tail.starts_with("audio0") {
        if !is_silent(&run.audio) {
            return Err(format!("{stem}: expected silence in {:?} mode", mode));
        }
    } else if tail.starts_with("audio1") {
        if is_silent(&run.audio) {
            return Err(format!("{stem}: expected audio output in {:?} mode", mode));
        }
    } else if !frame_buffer_matches(&run.frame, tail, mode) {
        return Err(format!("{stem}: framebuffer mismatch for {:?} mode", mode));
    }

    Ok(())
}

fn verify_png(run: &GambatteRun, stem: &str, png_path: &Path, mode: Mode) -> Result<(), String> {
    let (width, height, expected) = common::load_png_rgb(png_path);
    if width as usize != GB_WIDTH {
        return Err(format!("unexpected PNG width for {stem}"));
    }
    if height as usize != GB_HEIGHT {
        return Err(format!("unexpected PNG height for {stem}"));
    }

    for (idx, pixel) in expected.iter().enumerate() {
        let expected_pixel = normalize_pixel(pixel, mode);
        let actual = normalize_color(run.frame[idx], mode);
        if actual != expected_pixel {
            return Err(format!(
                "{stem}: pixel mismatch at index {idx} for {:?} mode (expected {:02X?}, got {:02X?})",
                mode, expected_pixel, actual
            ));
        }
    }
    Ok(())
}

fn is_silent(samples: &[i16]) -> bool {
    samples
        .first()
        .map(|first| samples.iter().all(|&s| s == *first))
        .unwrap_or(true)
}

fn frame_buffer_matches(frame: &[u32], pattern: &str, mode: Mode) -> bool {
    let mut tile_index = 0usize;
    for ch in pattern.chars() {
        let Some(tile) = tile_from_char(ch) else {
            break;
        };
        let start_x = tile_index * 8;
        if start_x + 8 > GB_WIDTH {
            return false;
        }
        for y in 0..8 {
            for x in 0..8 {
                let idx = y * GB_WIDTH + start_x + x;
                let pixel = sanitize_color(frame[idx], mode);
                if pixel != tile[y * 8 + x] {
                    return false;
                }
            }
        }
        tile_index += 1;
    }
    tile_index > 0
}

fn tile_from_char(c: char) -> Option<&'static [u32; 64]> {
    let idx = if c.is_ascii_digit() {
        c as usize - '0' as usize
    } else {
        let upper = c.to_ascii_uppercase();
        if ('A'..='F').contains(&upper) {
            10 + upper as usize - 'A' as usize
        } else {
            return None;
        }
    };
    TILE_PATTERNS.get(idx)
}

fn sanitize_color(color: u32, mode: Mode) -> u32 {
    match mode {
        Mode::Cgb => color & 0x00F8F8F8,
        Mode::Dmg => {
            let shade = grayscale_shade(color);
            if shade >= 128 { 0x00F8F8F8 } else { 0x000000 }
        }
    }
}

fn normalize_pixel(pixel: &[u8; 3], mode: Mode) -> [u8; 3] {
    match mode {
        Mode::Cgb => {
            let r = pixel[0] & 0xF8;
            let g = pixel[1] & 0xF8;
            let b = pixel[2] & 0xF8;
            [r, g, b]
        }
        Mode::Dmg => {
            let shade = grayscale_from_rgb(pixel[0], pixel[1], pixel[2]);
            [shade, shade, shade]
        }
    }
}

fn normalize_color(color: u32, mode: Mode) -> [u8; 3] {
    match mode {
        Mode::Cgb => [
            ((color >> 16) as u8) & 0xF8,
            ((color >> 8) as u8) & 0xF8,
            (color as u8) & 0xF8,
        ],
        Mode::Dmg => {
            let shade = grayscale_shade(color);
            [shade, shade, shade]
        }
    }
}

fn grayscale_shade(color: u32) -> u8 {
    let r = (color >> 16) & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = color & 0xFF;
    grayscale_from_rgb(r as u8, g as u8, b as u8)
}

fn grayscale_from_rgb(r: u8, g: u8, b: u8) -> u8 {
    let brightness = (u32::from(r) * 30 + u32::from(g) * 59 + u32::from(b) * 11) / 100;
    const SHADES: [u8; 4] = [0, 85, 170, 255];
    let mut best = SHADES[0];
    let mut best_diff = u32::MAX;
    for &shade in &SHADES {
        let diff = brightness.abs_diff(u32::from(shade));
        if diff < best_diff {
            best_diff = diff;
            best = shade;
        }
    }
    best
}

impl core::fmt::Debug for Mode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Mode::Dmg => write!(f, "DMG"),
            Mode::Cgb => write!(f, "CGB"),
        }
    }
}

#[rustfmt::skip]
const TILE_PATTERNS: [[u32; 64]; 16] = [
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0x000000,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0xF8F8F8,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    ],
    [
        0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
        0xF8F8F8, 0x000000, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8, 0xF8F8F8,
    ]
];
