pub struct Apu {}

impl Apu {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}
