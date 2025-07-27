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
fn div_resets_on_write() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    t.div = 0xABCD;
    t.write(0xFF04, 0x12, &mut if_reg);
    assert_eq!(t.read(0xFF04), 0);
    assert_eq!(t.div, 0);
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

#[test]
fn tma_write_same_cycle_overflow() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;

    // Prepare for falling edge on bit 9
    t.div = 0x03FF; // bit9 high
    t.write(0xFF07, 0x04, &mut if_reg); // enable timer (freq 4096Hz)

    t.tima = 0xFF;
    t.tma = 0xAA; // old value

    // Write new TMA in same cycle as overflow
    t.write(0xFF06, 0xBB, &mut if_reg);

    // Next cycle triggers falling edge and overflow
    t.step(1, &mut if_reg);

    assert_eq!(t.tma, 0xBB);
    assert_eq!(t.tima, 0xAA); // old value should be loaded
    assert_eq!(if_reg & 0x04, 0x04);
}
