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
fn tac_change_clock_select_edge_tick() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    // bit9 high, bit3 low
    t.div = 0x0200;
    t.write(0xFF07, 0x04, &mut if_reg); // enable freq 4096Hz (bit9)
    // switch to freq 262144Hz (bit3) which is currently low -> falling edge
    t.write(0xFF07, 0x05, &mut if_reg);
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
    // overflow occurred: TIMA should be 0 for one cycle
    assert_eq!(t.tima, 0);
    assert_eq!(if_reg, 0);

    // remain at zero until the delayed reload
    t.step(3, &mut if_reg);
    assert_eq!(t.tima, 0);
    assert_eq!(if_reg, 0);

    t.step(1, &mut if_reg);
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
    assert_eq!(t.tima, 0); // in overflow state

    // Reload occurs after the delayed window with the old value
    for _ in 0..3 {
        t.step(1, &mut if_reg);
        assert_eq!(t.tima, 0);
    }

    t.step(1, &mut if_reg);

    assert_eq!(t.tma, 0xBB);
    assert_eq!(t.tima, 0xAA); // old value should be loaded
    assert_eq!(if_reg & 0x04, 0x04);
}

#[test]
fn tac_clock_select_262khz() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    // enable timer, freq 01 (262144 Hz -> bit 3)
    t.write(0xFF07, 0x05, &mut if_reg);
    t.step(16, &mut if_reg);
    assert_eq!(t.tima, 1);
    assert_eq!(if_reg, 0);
}

#[test]
fn tac_clock_select_65khz() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    // enable timer, freq 10 (65536 Hz -> bit 5)
    t.write(0xFF07, 0x06, &mut if_reg);
    t.step(64, &mut if_reg);
    assert_eq!(t.tima, 1);
    assert_eq!(if_reg, 0);
}

#[test]
fn tac_clock_select_16khz() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    // enable timer, freq 11 (16384 Hz -> bit 7)
    t.write(0xFF07, 0x07, &mut if_reg);
    t.step(256, &mut if_reg);
    assert_eq!(t.tima, 1);
    assert_eq!(if_reg, 0);
}

#[test]
fn tima_overflow_delay_and_reload() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    t.div = 0x03FF; // bit9 high
    t.write(0xFF07, 0x04, &mut if_reg); // enable timer
    t.tma = 0xAA;
    t.tima = 0xFF;

    // trigger overflow
    t.step(1, &mut if_reg);
    assert_eq!(t.tima, 0);
    assert_eq!(if_reg, 0);

    // stay at zero for the remaining delay cycles
    for _ in 0..3 {
        t.step(1, &mut if_reg);
        assert_eq!(t.tima, 0);
        assert_eq!(if_reg, 0);
    }

    // then TMA is loaded and IF set
    t.step(1, &mut if_reg);
    assert_eq!(t.tima, 0xAA);
    assert_eq!(if_reg & 0x04, 0x04);
}

#[test]
fn tima_write_during_overflow_cancels_reload() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    t.div = 0x03FF; // bit9 high
    t.write(0xFF07, 0x04, &mut if_reg);
    t.tma = 0xAA;
    t.tima = 0xFF;

    // overflow
    t.step(1, &mut if_reg);
    assert_eq!(t.tima, 0);

    // write during cycle A
    t.write(0xFF05, 0x55, &mut if_reg);

    // subsequent cycles should not reload or set IF
    t.step(4, &mut if_reg);
    assert_eq!(t.tima, 0x55);
    assert_eq!(if_reg, 0);
}

#[test]
fn tima_write_during_reload_ignored() {
    let mut t = Timer::new();
    let mut if_reg = 0u8;
    t.div = 0x03FF; // bit9 high
    t.write(0xFF07, 0x04, &mut if_reg);
    t.tma = 0xAA;
    t.tima = 0xFF;

    // overflow
    t.step(1, &mut if_reg);
    // advance to the final cycle before reload (reload_delay == 0)
    for _ in 0..3 {
        t.step(1, &mut if_reg);
    }

    // write TIMA during the reload window
    t.write(0xFF05, 0x55, &mut if_reg);

    // reload should overwrite our write on the next cycle
    t.step(1, &mut if_reg);
    assert_eq!(t.tima, 0xAA);
    assert_eq!(if_reg & 0x04, 0x04);
}
