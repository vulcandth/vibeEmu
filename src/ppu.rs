pub struct Ppu {
    pub vram: [u8; 0x2000],
}

impl Ppu {
    pub fn new() -> Self {
        Self { vram: [0; 0x2000] }
    }
}
