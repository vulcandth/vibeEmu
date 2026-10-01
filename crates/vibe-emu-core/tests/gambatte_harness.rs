#[path = "common/gambatte_expectations.rs"]
mod expectations;
use expectations::{Mode, cgb_png_color, detect_out_string, is_silent};

#[test]
fn model_expectations_follow_upstream_markers() {
    for (stem, dmg, cgb) in [
        (
            "both_dmg08_cgb04c_out1",
            Some("dmg08_cgb04c_out"),
            Some("dmg08_cgb04c_out"),
        ),
        (
            "split_dmg08_out0_cgb04c_out1",
            Some("dmg08_out"),
            Some("cgb04c_out"),
        ),
        ("dmg_dmg08_out0", Some("dmg08_out"), None),
        (
            "dmg_dmg08_outaudio1_cgb_xoutaudio1lowpitch",
            Some("dmg08_out"),
            None,
        ),
        ("cgb_cgb04c_out1", None, Some("cgb04c_out")),
        ("legacy_out1", None, Some("_out")),
        ("png_only", None, None),
    ] {
        assert_eq!(detect_out_string(stem, Mode::Dmg), dmg, "{stem}");
        assert_eq!(detect_out_string(stem, Mode::Cgb), cgb, "{stem}");
    }
}

#[test]
fn cgb_png_colors_match_upstream_primary_color_vectors() {
    assert_eq!(cgb_png_color(0x000000), [0, 0, 0]);
    assert_eq!(cgb_png_color(0xffffff), [248, 248, 248]);
    assert_eq!(cgb_png_color(0xff0000), [200, 0, 40]);
    assert_eq!(cgb_png_color(0x00ff00), [24, 184, 24]);
    assert_eq!(cgb_png_color(0x0000ff), [8, 56, 168]);
}

#[test]
fn audio_requires_samples_and_checks_both_stereo_channels() {
    assert!(is_silent(&[]).is_err());
    assert_eq!(is_silent(&[(12, -3); 32]), Ok(true));
    assert_eq!(is_silent(&[(12, -3), (13, -3)]), Ok(false));
    assert_eq!(is_silent(&[(12, -3), (12, -2)]), Ok(false));
}
