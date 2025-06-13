pub struct Cartridge {
    pub rom: Vec<u8>,
}

impl Cartridge {
    pub fn load(data: Vec<u8>) -> Self {
        Self { rom: data }
    }
}
