use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dmg,
    Cgb,
}

impl fmt::Debug for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dmg => "DMG",
            Self::Cgb => "CGB",
        })
    }
}

// Follow the upstream testrunner's precedence. A DMG-only marker does not
// imply a CGB expectation; a later explicit CGB marker still applies.
pub fn detect_out_string(stem: &str, mode: Mode) -> Option<&'static str> {
    if stem.contains("dmg08_cgb04c_out") {
        return Some("dmg08_cgb04c_out");
    }
    match mode {
        Mode::Dmg => stem.contains("dmg08_out").then_some("dmg08_out"),
        Mode::Cgb => {
            if stem.contains("cgb04c_out") {
                Some("cgb04c_out")
            } else if !stem.contains("dmg08_out") && stem.contains("_out") {
                Some("_out")
            } else {
                None
            }
        }
    }
}

// Upstream PNGs are generated with this RGB555 conversion, whereas the core
// exposes expanded RGB555. Convert actual pixels only; never alter fixtures.
pub fn cgb_png_color(color: u32) -> [u8; 3] {
    let r = (color >> 19) & 31;
    let g = (color >> 11) & 31;
    let b = (color >> 3) & 31;
    [
        (((r * 13 + g * 2 + b) / 2) as u8) & 0xf8,
        (((g * 3 + b) * 2) as u8) & 0xf8,
        (((r * 3 + g * 2 + b * 11) / 2) as u8) & 0xf8,
    ]
}

pub fn is_silent(samples: &[(i16, i16)]) -> Result<bool, &'static str> {
    let first = samples.first().ok_or("no audio samples captured")?;
    Ok(samples.iter().all(|sample| sample == first))
}
