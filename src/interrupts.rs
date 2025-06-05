// src/interrupts.rs
pub const VBLANK_IRQ_BIT: u8 = 0;
pub const LCD_STAT_IRQ_BIT: u8 = 1;
pub const TIMER_IRQ_BIT: u8 = 2;
pub const SERIAL_IRQ_BIT: u8 = 3;
pub const JOYPAD_IRQ_BIT: u8 = 4;

#[derive(Debug, Clone, Copy)]
pub enum InterruptType {
    VBlank,
    LcdStat,
    Timer,
    Serial,
    Joypad,
}

impl InterruptType {
    pub fn bit(&self) -> u8 {
        match self {
            InterruptType::VBlank => VBLANK_IRQ_BIT,
            InterruptType::LcdStat => LCD_STAT_IRQ_BIT,
            InterruptType::Timer => TIMER_IRQ_BIT,
            InterruptType::Serial => SERIAL_IRQ_BIT,
            InterruptType::Joypad => JOYPAD_IRQ_BIT,
        }
    }
}
