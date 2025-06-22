fn main() {
    #[cfg(target_os = "windows")]
    {
        use std::path::Path;
        let mut res = winres::WindowsResource::new();
        res.set_icon("gfx/vibeEmu.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=winres failed: {}", e);
        }
        println!(
            "cargo:rerun-if-changed={}",
            Path::new("gfx/vibeEmu.ico").display()
        );
    }
}
