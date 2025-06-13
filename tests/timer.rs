use vibeEmu::timer::Timer;

#[test]
fn div_increment() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    t.step(256, &mut if_reg);
    assert_eq!(t.read(0xFF04), 1);
    assert_eq!(if_reg, 0);
}

#[test]
fn div_reset_edge_tick() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    t.div = 0x0200; // timer bit high
    t.write(0xFF07, 0x04, &mut if_reg); // enable, freq 4096Hz (bit9)
    t.write(0xFF04, 0, &mut if_reg); // reset DIV causes falling edge
    assert_eq!(t.tima, 1);
    assert_eq!(if_reg, 0);
}

#[test]
fn tac_disable_edge_tick() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    t.div = 0x0200; // bit9 high
    t.write(0xFF07, 0x04, &mut if_reg); // enable
    t.write(0xFF07, 0x00, &mut if_reg); // disable -> falling edge
    assert_eq!(t.tima, 1);
    assert_eq!(if_reg, 0);
}

#[test]
fn tima_increment_and_overflow() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    // enable timer, freq 00 (4096 Hz -> bit 9)
    t.write(0xFF07, 0x04, &mut if_reg); // enable
    t.step(1024, &mut if_reg);
    assert_eq!(t.tima, 1);
    assert_eq!(if_reg, 0);

    t.tima = 0xFF;
    t.tma = 0xAB;
    t.step(1024, &mut if_reg);
    assert_eq!(t.tima, 0xAB);
    assert_eq!(if_reg & 0x04, 0x04);
}
