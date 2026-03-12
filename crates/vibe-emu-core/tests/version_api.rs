#[test]
fn exposes_core_crate_version() {
    assert_eq!(vibe_emu_core::VERSION, env!("CARGO_PKG_VERSION"));
}
