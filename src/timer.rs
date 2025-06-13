pub struct Timer {
    pub div: u16,
    pub tima: u8,
    pub tma: u8,
    pub tac: u8,
}

impl Timer {
    pub fn new() -> Self {
        Self { div: 0, tima: 0, tma: 0, tac: 0 }
    }
}
