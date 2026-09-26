mod common;
#[path = "common/mooneye.rs"]
mod suite;

fn main() {
    suite::main(true, include_str!("wilbertpol_ignored.txt"), &[]);
}
