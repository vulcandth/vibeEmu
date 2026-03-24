use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const README_ABOUT_EXCERPT_START: &str = "<!-- about-page-excerpt:start -->";
const README_ABOUT_EXCERPT_END: &str = "<!-- about-page-excerpt:end -->";

fn main() {
    println!("cargo:rerun-if-changed=../../gfx/vibeEmu.ico");
    println!("cargo:rerun-if-changed=../../README.md");
    println!("cargo:rerun-if-changed=LICENSE");
    println!("cargo:rerun-if-changed=THIRD_PARTY_LICENSES.md");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../../gfx/vibeEmu.ico");
        res.compile().expect("failed to embed Windows icon");
    }

    copy_legal_files_to_binary_output_dir();
    generate_about_assets();
}

fn generate_about_assets() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be available"),
    );
    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be available during build"));
    let readme = fs::read_to_string(manifest_dir.join("../../README.md"))
        .expect("failed to read workspace README.md for about page excerpt");
    let license = fs::read_to_string(manifest_dir.join("LICENSE"))
        .expect("failed to read LICENSE from vibe-emu-ui crate");
    let third_party = fs::read_to_string(manifest_dir.join("THIRD_PARTY_LICENSES.md"))
        .expect("failed to read THIRD_PARTY_LICENSES.md from vibe-emu-ui crate");
    let about_excerpt = extract_about_excerpt(&readme);
    let about_assets = format!(
        concat!(
            "pub const AUTHOR_STATEMENT_EXCERPT: &str = ",
            "{:?};\n",
            "pub const LICENSE_TEXT: &str = {:?};\n",
            "pub const THIRD_PARTY_LICENSES_TEXT: &str = {:?};\n",
        ),
        about_excerpt, license, third_party
    );

    fs::write(out_dir.join("about_assets.rs"), about_assets)
        .expect("failed to write generated about assets for vibe-emu-ui");
}

fn copy_legal_files_to_binary_output_dir() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be available"),
    );
    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be available during build"));
    let binary_dir = cargo_profile_output_dir(&out_dir)
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
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Third-party licenses</title></head><body><pre>{escaped}</pre></body></html>"
    )
}

fn extract_about_excerpt(readme: &str) -> String {
    let excerpt = readme
        .split_once(README_ABOUT_EXCERPT_START)
        .and_then(|(_, rest)| {
            rest.split_once(README_ABOUT_EXCERPT_END)
                .map(|(excerpt, _)| excerpt)
        })
        .map(str::trim)
        .filter(|excerpt| !excerpt.is_empty())
        .expect("README about-page excerpt markers should contain content");

    excerpt
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let line = line.strip_prefix('>').map(str::trim_start).unwrap_or(line);
            if line.is_empty() {
                None
            } else {
                Some(line.replace("**", "").replace('*', ""))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Cargo exposes `OUT_DIR` as a nested path under `target/<profile>/build/.../out`.
/// We climb to the `build` segment and use its parent so legal files are written
/// directly into the same profile directory where the executable is emitted.
fn cargo_profile_output_dir(out_dir: &Path) -> Option<&Path> {
    out_dir.ancestors().find_map(|ancestor| {
        if ancestor.file_name().is_some_and(|name| name == "build") {
            ancestor.parent()
        } else {
            None
        }
    })
}
