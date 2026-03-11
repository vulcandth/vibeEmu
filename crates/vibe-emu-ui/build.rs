use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../../gfx/vibeEmu.ico");
    println!("cargo:rerun-if-changed=LICENSE");
    println!("cargo:rerun-if-changed=THIRD_PARTY_LICENSES.md");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../../gfx/vibeEmu.ico");
        res.compile().expect("failed to embed Windows icon");
    }

    copy_legal_files_to_binary_output_dir();
}

fn copy_legal_files_to_binary_output_dir() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be available"),
    );
    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be available during build"));
    let binary_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR should include target profile directory");

    copy_file(&manifest_dir.join("LICENSE"), &binary_dir.join("LICENSE"));

    let third_party_md_path = manifest_dir.join("THIRD_PARTY_LICENSES.md");
    let third_party_md = fs::read_to_string(&third_party_md_path)
        .expect("failed to read THIRD_PARTY_LICENSES.md from vibe-emu-ui crate");
    copy_file(
        &third_party_md_path,
        &binary_dir.join("THIRD_PARTY_LICENSES.md"),
    );
    fs::write(
        binary_dir.join("THIRD_PARTY_LICENSES.html"),
        markdown_html_document(&third_party_md),
    )
    .expect("failed to write THIRD_PARTY_LICENSES.html next to executable output");
}

fn copy_file(source: &Path, destination: &Path) {
    fs::copy(source, destination).unwrap_or_else(|error| {
        panic!(
            "failed to copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    });
}

fn markdown_html_document(markdown: &str) -> String {
    let escaped = markdown
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Third-party licenses</title></head><body><pre>{escaped}</pre></body></html>"
    )
}
