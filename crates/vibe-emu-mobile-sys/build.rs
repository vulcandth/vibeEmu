use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let bundled = env::var_os("CARGO_FEATURE_BUNDLED").is_some();
    let system = env::var_os("CARGO_FEATURE_SYSTEM").is_some();

    if bundled && system {
        panic!("vibe-emu-mobile-sys: enable only one of 'bundled' or 'system'");
    }

    if bundled {
        build_bundled();
    } else if system {
        link_system();
    } else {
        // Default: build nothing and link nothing.
        // The safe wrapper crate (`vibe-emu-mobile`) provides a stub implementation
        // unless one of its 'bundled'/'system' features is enabled.
    }
}

fn link_system() {
    if let Some(dir) = env::var_os("LIBMOBILE_LIB_DIR") {
        println!(
            "cargo:rustc-link-search=native={}",
            PathBuf::from(dir).display()
        );
    }

    // Windows: this expects mobile.lib (MSVC) or libmobile.a (GNU) in the search path.
    println!("cargo:rustc-link-lib=mobile");
}

fn build_bundled() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let mobile_crate_vendor_root = manifest_dir.join("..").join("vibe-emu-mobile").join("vendor");
    let workspace_vendor_root = manifest_dir.join("..").join("..").join("vendor");

    let vendor_dir = if let Some(src) = env::var_os("LIBMOBILE_SRC_DIR") {
        PathBuf::from(src)
    } else {
        // Prefer the vendored source that ships with vibe-emu-mobile.
        let v022 = mobile_crate_vendor_root.join("libmobile-0.2.2");
        if v022.exists() {
            v022
        } else if mobile_crate_vendor_root.join("libmobile").exists() {
            mobile_crate_vendor_root.join("libmobile")
        } else if workspace_vendor_root.join("libmobile-0.2.2").exists() {
            // Backward-compatible fallback for older layouts.
            workspace_vendor_root.join("libmobile-0.2.2")
        } else {
            workspace_vendor_root.join("libmobile")
        }
    };

    if !vendor_dir.exists() {
        panic!(
            "vibe-emu-mobile-sys (bundled): missing vendored libmobile source. Tried: {}\n\n\
Place libmobile at one of:\n\
- crates/vibe-emu-mobile/vendor/libmobile-0.2.2 (recommended)\n\
- crates/vibe-emu-mobile/vendor/libmobile\n\
- vendor/libmobile-0.2.2 (legacy fallback)\n\
- vendor/libmobile (legacy fallback)\n\
Or set LIBMOBILE_SRC_DIR to point at a libmobile source checkout.\n\n\
Then rebuild with: cargo build -p vibe-emu-mobile --features bundled\n",
            vendor_dir.display()
        );
    }

    // Sanity check: mobile.h should exist at the root of the checkout.
    if !vendor_dir.join("mobile.h").exists() {
        panic!(
            "vibe-emu-mobile-sys (bundled): {} does not look like a libmobile checkout (missing mobile.h)",
            vendor_dir.display()
        );
    }

    println!("cargo:rerun-if-changed={}", vendor_dir.display());
    println!("cargo:rerun-if-env-changed=LIBMOBILE_SRC_DIR");

    let mut build = cc::Build::new();
    build.include(&vendor_dir);

    // libmobile is a C library that uses sockets.
    // Link the standard Windows socket libraries when building on Windows.
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=iphlpapi");
    }

    // Compile all .c files under the selected libmobile source, excluding obvious non-library folders.
    let c_files = collect_c_files(&vendor_dir);
    if c_files.is_empty() {
        panic!(
            "vibe-emu-mobile-sys (bundled): found no C sources under {} (expected libmobile sources)",
            vendor_dir.display()
        );
    }

    for file in c_files {
        build.file(file);
    }

    // Favor a predictable build, but keep it portable.
    build.warnings(false);
    build.compile("mobile");
}

fn collect_c_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();

                // Heuristic skips: keep the build robust across libmobile layout changes.
                if name.contains("test")
                    || name.contains("example")
                    || name.contains("doc")
                    || name.contains("cmake")
                    || name == ".git"
                {
                    continue;
                }

                stack.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("c"))
            {
                out.push(path);
            }
        }
    }

    out.sort();
    out
}
