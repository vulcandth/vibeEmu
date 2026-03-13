mod common;

use vibe_emu_core::{cartridge::Cartridge, gameboy::GameBoy, hardware::Model};

const DMG_PALETTE: [u32; 4] = [0x009BBC0F, 0x008BAC0F, 0x00306230, 0x000F380F];

fn run_for_frames(gb: &mut GameBoy, frames: u32) {
    let mut completed = 0u32;
    while completed < frames {
        gb.cpu.step(&mut gb.mmu);
        if gb.mmu.ppu.frame_ready() {
            gb.mmu.ppu.clear_frame_flag();
            completed += 1;
        }
    }
}

fn pack_rgb([r, g, b]: [u8; 3]) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | (b as u32)
}

fn unpack_rgb(color: u32) -> [u8; 3] {
    [
        ((color >> 16) & 0xFF) as u8,
        ((color >> 8) & 0xFF) as u8,
        (color & 0xFF) as u8,
    ]
}

fn expected_pixel_to_frame_color(expected: [u8; 3]) -> u32 {
    match expected {
        [0x00, 0x00, 0x00] => DMG_PALETTE[3],
        [0x55, 0x55, 0x55] => DMG_PALETTE[2],
        [0xAA, 0xAA, 0xAA] => DMG_PALETTE[1],
        [0xFF, 0xFF, 0xFF] => DMG_PALETTE[0],
        other => pack_rgb(other),
    }
}

fn luminance(rgb: [u8; 3]) -> u16 {
    rgb[0] as u16 + rgb[1] as u16 + rgb[2] as u16
}

fn dump_mismatches(expected: &[[u8; 3]], actual: &[u32]) {
    let mut mismatches = 0usize;
    let mut min_x = 160u32;
    let mut min_y = 144u32;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    let mut sample = Vec::new();

    for (idx, &exp_rgb) in expected.iter().enumerate() {
        let exp = expected_pixel_to_frame_color(exp_rgb);
        let act = actual[idx];
        if exp != act {
            mismatches += 1;
            let x = (idx as u32) % 160;
            let y = (idx as u32) / 160;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            if sample.len() < 40 {
                sample.push((x, y, exp_rgb, unpack_rgb(act)));
            }
        }
    }

    println!(
        "strikethrough mismatch: {mismatches} pixels differ (bbox x={min_x}..={max_x}, y={min_y}..={max_y})"
    );

    if !sample.is_empty() {
        println!("sample mismatches (x,y expected->actual):");
        for (x, y, exp, act) in sample {
            println!("  ({x:3},{y:3}) {:?} -> {:?}", exp, act);
        }
    }

    // Focused span report for the (horizontal) strikethrough.
    // We classify mismatches into:
    // - extra_dark: actual darker than expected (line too long)
    // - missing_dark: expected darker than actual (line too short)
    let mut per_row_extra = vec![RowSpan::default(); 144];
    let mut per_row_missing = vec![RowSpan::default(); 144];

    for (idx, &exp_rgb) in expected.iter().enumerate() {
        let exp = expected_pixel_to_frame_color(exp_rgb);
        let act = actual[idx];
        if exp == act {
            continue;
        }

        let x = (idx as u32) % 160;
        let y = (idx as u32) / 160;
        let exp_l = luminance(exp_rgb);
        let act_l = luminance(unpack_rgb(act));

        // A fairly forgiving threshold: we care about a dark line vs lighter glyph/background.
        if act_l + 40 < exp_l {
            per_row_extra[y as usize].add(x);
        } else if exp_l + 40 < act_l {
            per_row_missing[y as usize].add(x);
        }
    }

    fn best_row(spans: &[RowSpan]) -> Option<(usize, RowSpan)> {
        spans
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, s)| s.count > 0)
            .max_by_key(|(_, s)| s.count)
    }

    if let Some((y, span)) = best_row(&per_row_extra) {
        println!(
            "extra-dark mismatch row y={y}: x={}..={} (count={})",
            span.min_x, span.max_x, span.count
        );

        let exp_spans = darkest_spans_in_row_expected(expected, y);
        let act_spans = darkest_spans_in_row_actual(actual, y);
        println!("row y={y} expected palette[3] spans: {exp_spans}");
        println!("row y={y} actual   palette[3] spans: {act_spans}");
    }
    if let Some((y, span)) = best_row(&per_row_missing) {
        println!(
            "missing-dark mismatch row y={y}: x={}..={} (count={})",
            span.min_x, span.max_x, span.count
        );
    }

    // Print the top few candidate rows to make it easy to see where the line starts/ends.
    let mut rows: Vec<(usize, RowSpan, RowSpan)> = (0..144)
        .map(|y| (y, per_row_extra[y], per_row_missing[y]))
        .filter(|(_, a, b)| a.count > 0 || b.count > 0)
        .collect();
    rows.sort_by_key(|(_, a, b)| a.count + b.count);
    rows.reverse();

    println!("top mismatch rows (extra-dark / missing-dark spans):");
    for (y, a, b) in rows.into_iter().take(12) {
        let a_str = if a.count == 0 {
            "-".to_string()
        } else {
            format!("{}..{} (n={})", a.min_x, a.max_x, a.count)
        };
        let b_str = if b.count == 0 {
            "-".to_string()
        } else {
            format!("{}..{} (n={})", b.min_x, b.max_x, b.count)
        };
        println!("  y={y:3}: extra={a_str:<20} missing={b_str}");
    }
}

fn darkest_spans_in_row_expected(expected: &[[u8; 3]], y: usize) -> String {
    darkest_spans(0..160, |x| {
        let idx = y * 160 + x;
        expected_pixel_to_frame_color(expected[idx]) == DMG_PALETTE[3]
    })
}

fn darkest_spans_in_row_actual(actual: &[u32], y: usize) -> String {
    darkest_spans(0..160, |x| {
        let idx = y * 160 + x;
        actual[idx] == DMG_PALETTE[3]
    })
}

fn darkest_spans<I, F>(xs: I, mut is_dark: F) -> String
where
    I: IntoIterator<Item = usize>,
    F: FnMut(usize) -> bool,
{
    let mut spans = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for x in xs {
        if is_dark(x) {
            match current {
                None => current = Some((x, x)),
                Some((start, _)) => current = Some((start, x)),
            }
        } else if let Some(span) = current.take() {
            spans.push(span);
        }
    }
    if let Some(span) = current {
        spans.push(span);
    }
    if spans.is_empty() {
        return "<none>".to_string();
    }
    spans
        .into_iter()
        .map(|(a, b)| format!("{a}..{b}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Copy, Default)]
struct RowSpan {
    min_x: u32,
    max_x: u32,
    count: u32,
}

impl RowSpan {
    fn add(&mut self, x: u32) {
        if self.count == 0 {
            self.min_x = x;
            self.max_x = x;
        } else {
            self.min_x = self.min_x.min(x);
            self.max_x = self.max_x.max(x);
        }
        self.count += 1;
    }
}

#[test]
fn strikethrough_hacktix_png() {
    let mut gb = GameBoy::new(Model::default());

    let rom = std::fs::read(common::rom_path("hacktix/strikethrough.gb")).expect("rom not found");
    gb.mmu.load_cart(Cartridge::from_bytes(rom));

    // The ROM is fully visual; give it time to reach a steady screen.
    run_for_frames(&mut gb, 120);

    let (width, height, expected) =
        common::load_png_rgb(common::rom_path("hacktix/strikethrough.png"));
    assert_eq!(width, 160);
    assert_eq!(height, 144);

    let frame = gb.mmu.ppu.framebuffer();

    let mut mismatches = 0usize;
    for (idx, exp_rgb) in expected.iter().copied().enumerate() {
        let expected_color = expected_pixel_to_frame_color(exp_rgb);
        if frame[idx] != expected_color {
            mismatches += 1;
        }
    }

    if mismatches != 0 {
        dump_mismatches(&expected, frame);

        // Sprite segment around x≈71 should come from the 7th sprite in the initial
        // strikethrough chain: STRIKETHROUGH_START_X(23) + 6*8 = 71.
        // Dump a small window of OAM so we can see what DMA produced.
        let oam = &gb.mmu.ppu.oam;
        let sprite_idx = 6usize;
        let base = sprite_idx * 4;
        println!(
            "oam sprite[{sprite_idx}] y={:02X} x={:02X} tile={:02X} flags={:02X}",
            oam[base],
            oam[base + 1],
            oam[base + 2],
            oam[base + 3]
        );
        for i in sprite_idx.saturating_sub(2)..=(sprite_idx + 2).min(39) {
            let b = i * 4;
            println!(
                "  oam[{i:02}] y={:02X} x={:02X} tile={:02X} flags={:02X}",
                oam[b],
                oam[b + 1],
                oam[b + 2],
                oam[b + 3]
            );
        }

        // The ROM triggers DMA from HIGH(wShadowOAM), which should land on the 0xC0xx page.
        // Check whether the source bytes contain any $00 at all.
        let mut zeros = Vec::new();
        for off in 0u16..0xA0 {
            if gb.mmu.read_byte(0xC000 + off) == 0 {
                zeros.push(off);
            }
        }
        if zeros.is_empty() {
            println!("shadow oam page 0xC000..0xC09F: no $00 bytes found");
        } else {
            println!(
                "shadow oam page 0xC000..0xC09F: $00 offsets (count={}): {:?}",
                zeros.len(),
                zeros
            );
        }
    }

    assert_eq!(
        mismatches, 0,
        "strikethrough.png mismatch: {mismatches} pixels differ"
    );
}
