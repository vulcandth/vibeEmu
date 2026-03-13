//! Integration tests for the Game Boy serial port and link cable emulation.
//!
//! These tests verify:
//! 1. Serial register behavior (SB/SC)
//! 2. Internal vs external clock transfers
//! 3. Two-device link cable simulation

use std::collections::VecDeque;
use vibe_emu_core::hardware::Model;
use vibe_emu_core::serial::{LinkPort, NullLinkPort, Serial};

/// A link port that records all bytes sent and returns pre-programmed responses.
struct RecordingLinkPort {
    sent: Vec<u8>,
    responses: VecDeque<u8>,
    default_response: u8,
}

impl RecordingLinkPort {
    fn new(responses: impl IntoIterator<Item = u8>) -> Self {
        Self {
            sent: Vec::new(),
            responses: responses.into_iter().collect(),
            default_response: 0xFF,
        }
    }
}

impl LinkPort for RecordingLinkPort {
    fn transfer(&mut self, byte: u8) -> u8 {
        self.sent.push(byte);
        self.responses.pop_front().unwrap_or(self.default_response)
    }
}

// Note: This file previously contained scaffolding for a fully cross-linked
// two-device simulation. Keep integration tests focused on exercised behavior
// to satisfy strict clippy settings.

#[test]
fn null_link_port_returns_ff_by_default() {
    let mut port = NullLinkPort::new(false);
    assert_eq!(port.transfer(0x42), 0xFF);
    assert_eq!(port.transfer(0x00), 0xFF);
}

#[test]
fn null_link_port_loopback_echoes_byte() {
    let mut port = NullLinkPort::new(true);
    assert_eq!(port.transfer(0x42), 0x42);
    assert_eq!(port.transfer(0xAB), 0xAB);
}

#[test]
fn serial_sb_readable_writable() {
    let mut serial = Serial::new(Model::default());

    serial.write(0xFF01, 0x42);
    assert_eq!(serial.read(0xFF01), 0x42);

    serial.write(0xFF01, 0xAB);
    assert_eq!(serial.read(0xFF01), 0xAB);
}

#[test]
fn serial_sc_dmg_masks_unused_bits() {
    let mut serial = Serial::new(Model::default());

    serial.write(0xFF02, 0xFF);
    // DMG: bits 1-6 are unused and read as 1
    assert_eq!(serial.read(0xFF02), 0xFF);

    serial.write(0xFF02, 0x00);
    assert_eq!(serial.read(0xFF02), 0x7E);
}

#[test]
fn serial_sc_cgb_preserves_fast_clock_bit() {
    let mut serial = Serial::new(Model::from_cgb_flag(true));

    serial.write(0xFF02, 0x83); // bit7 + bit1 + bit0
    assert_eq!(serial.read(0xFF02), 0x83);

    serial.write(0xFF02, 0x02); // just fast clock bit
    // CGB returns raw SC value (unlike DMG which masks unused bits)
    assert_eq!(serial.read(0xFF02), 0x02);
}

#[test]
fn internal_clock_transfer_exchanges_bytes() {
    let responses = RecordingLinkPort::new([0xAB]);
    let mut serial = Serial::new(Model::default());
    serial.connect(Box::new(responses));

    serial.write(0xFF01, 0x12);
    serial.write(0xFF02, 0x81); // internal clock + start

    let mut if_reg = 0u8;
    serial.step(0, 4096, false, &mut if_reg);

    assert_eq!(serial.read(0xFF01), 0xAB);
    assert!(serial.read(0xFF02) & 0x80 == 0); // transfer complete
    assert!(if_reg & 0x08 != 0); // serial IRQ
}

#[test]
fn external_clock_transfer_waits_for_pulses() {
    let responses = RecordingLinkPort::new([0xAB]);
    let mut serial = Serial::new(Model::default());
    serial.connect(Box::new(responses));

    serial.write(0xFF01, 0x12);
    serial.write(0xFF02, 0x80); // external clock + start

    let mut if_reg = 0u8;

    // Time passes but no clock pulses
    serial.step(0, 60000, false, &mut if_reg);
    assert!(serial.read(0xFF02) & 0x80 != 0); // still pending
    assert!(if_reg & 0x08 == 0); // no IRQ

    // Now deliver clock pulses
    serial.external_clock_pulse(8, &mut if_reg);

    assert_eq!(serial.read(0xFF01), 0xAB);
    assert!(serial.read(0xFF02) & 0x80 == 0); // transfer complete
    assert!(if_reg & 0x08 != 0); // serial IRQ
}

#[test]
fn external_clock_partial_pulses() {
    let responses = RecordingLinkPort::new([0xAB]);
    let mut serial = Serial::new(Model::default());
    serial.connect(Box::new(responses));

    serial.write(0xFF01, 0x12);
    serial.write(0xFF02, 0x80); // external clock + start

    let mut if_reg = 0u8;

    // Deliver 4 pulses - not enough
    serial.external_clock_pulse(4, &mut if_reg);
    assert!(serial.read(0xFF02) & 0x80 != 0); // still pending
    assert!(if_reg & 0x08 == 0); // no IRQ

    // Deliver 3 more - still not enough (7 total)
    serial.external_clock_pulse(3, &mut if_reg);
    assert!(serial.read(0xFF02) & 0x80 != 0);
    assert!(if_reg & 0x08 == 0);

    // Final pulse completes the transfer
    serial.external_clock_pulse(1, &mut if_reg);
    assert_eq!(serial.read(0xFF01), 0xAB);
    assert!(serial.read(0xFF02) & 0x80 == 0);
    assert!(if_reg & 0x08 != 0);
}

#[test]
fn has_external_clock_transfer_pending_works() {
    let mut serial = Serial::new(Model::default());

    assert!(!serial.has_external_clock_transfer_pending());

    // Start internal clock transfer
    serial.write(0xFF01, 0x12);
    serial.write(0xFF02, 0x81);
    assert!(!serial.has_external_clock_transfer_pending());

    // Start external clock transfer
    let mut serial2 = Serial::new(Model::default());
    serial2.write(0xFF01, 0x12);
    serial2.write(0xFF02, 0x80);
    assert!(serial2.has_external_clock_transfer_pending());
}

#[test]
fn pending_external_clock_outgoing_returns_byte() {
    let mut serial = Serial::new(Model::default());

    assert!(serial.pending_external_clock_outgoing().is_none());

    serial.write(0xFF01, 0x42);
    serial.write(0xFF02, 0x80); // external clock + start

    assert_eq!(serial.pending_external_clock_outgoing(), Some(0x42));
}

#[test]
fn transfer_cancelled_by_clearing_sc_bit7() {
    let responses = RecordingLinkPort::new([0xAB]);
    let mut serial = Serial::new(Model::default());
    serial.connect(Box::new(responses));

    serial.write(0xFF01, 0x12);
    serial.write(0xFF02, 0x81); // start

    // Cancel by clearing bit7
    serial.write(0xFF02, 0x01);

    let mut if_reg = 0u8;
    serial.step(0, 10000, false, &mut if_reg);

    // Transfer should not complete
    assert!(if_reg & 0x08 == 0);
}

#[test]
fn multiple_sequential_transfers() {
    let responses = RecordingLinkPort::new([0x11, 0x22, 0x33]);
    let mut serial = Serial::new(Model::default());
    serial.connect(Box::new(responses));

    for (send, expect) in [(0xAA, 0x11), (0xBB, 0x22), (0xCC, 0x33)] {
        serial.write(0xFF01, send);
        serial.write(0xFF02, 0x81);

        let mut if_reg = 0u8;
        serial.step(0, 4096, false, &mut if_reg);

        assert_eq!(serial.read(0xFF01), expect);
        assert!(serial.read(0xFF02) & 0x80 == 0);
        assert!(if_reg & 0x08 != 0);
    }
}

#[test]
fn sb_output_captures_writes() {
    let mut serial = Serial::new(Model::default());

    serial.write(0xFF01, 0x41); // 'A'
    serial.write(0xFF01, 0x42); // 'B'
    serial.write(0xFF01, 0x43); // 'C'

    let output = serial.take_sb_output();
    assert_eq!(output, vec![0x41, 0x42, 0x43]);

    // Should be cleared after take
    assert!(serial.peek_sb_output().is_empty());
}

#[test]
fn out_buf_captures_completed_transfers() {
    let responses = RecordingLinkPort::new([0x11, 0x22]);
    let mut serial = Serial::new(Model::default());
    serial.connect(Box::new(responses));

    serial.write(0xFF01, 0xAA);
    serial.write(0xFF02, 0x81);
    let mut if_reg = 0u8;
    serial.step(0, 4096, false, &mut if_reg);

    serial.write(0xFF01, 0xBB);
    serial.write(0xFF02, 0x81);
    serial.step(4096, 8192, false, &mut if_reg);

    let output = serial.take_output();
    assert_eq!(output, vec![0xAA, 0xBB]);
}

#[test]
fn cgb_fast_clock_completes_faster() {
    let responses = RecordingLinkPort::new([0xAB]);
    let mut serial = Serial::new(Model::from_cgb_flag(true));
    serial.connect(Box::new(responses));

    serial.write(0xFF01, 0x12);
    serial.write(0xFF02, 0x83); // internal + fast clock + start

    let mut if_reg = 0u8;
    // Fast clock uses DIV bit 3: 8 bits -> 8 * 16 = 128 increments
    serial.step(0, 128, false, &mut if_reg);

    assert_eq!(serial.read(0xFF01), 0xAB);
    assert!(serial.read(0xFF02) & 0x80 == 0);
    assert!(if_reg & 0x08 != 0);
}

/// Simulates the Pokemon link cable handshake sequence.
/// This test uses a simple recording port since cross-linked ports
/// need careful synchronization that's handled by the network layer.
#[test]
fn pokemon_style_handshake() {
    // Simulate master sending 0x01 and receiving 0x02
    let master_responses = RecordingLinkPort::new([0x02]);
    let mut master = Serial::new(Model::default());
    master.connect(Box::new(master_responses));

    master.write(0xFF01, 0x01);
    master.write(0xFF02, 0x81); // internal clock

    let mut if_reg = 0u8;
    master.step(0, 4096, false, &mut if_reg);

    // Master sent 0x01, got back 0x02
    assert_eq!(master.read(0xFF01), 0x02);
    assert!(if_reg & 0x08 != 0);

    // Simulate slave receiving 0x01 and sending 0x02 via external clock
    let slave_responses = RecordingLinkPort::new([0x01]);
    let mut slave = Serial::new(Model::default());
    slave.connect(Box::new(slave_responses));

    slave.write(0xFF01, 0x02);
    slave.write(0xFF02, 0x80); // external clock

    let mut if_reg = 0u8;
    slave.external_clock_pulse(8, &mut if_reg);

    // Slave sent 0x02, received 0x01
    assert_eq!(slave.read(0xFF01), 0x01);
    assert!(if_reg & 0x08 != 0);
}

/// Tests the scenario where slave sends SERIAL_NO_DATA_BYTE (0xFE)
/// to indicate it's not ready yet.
#[test]
fn serial_no_data_byte_handling() {
    // Slave responds with 0xFE (not ready) three times, then 0x42
    let responses = RecordingLinkPort::new([0xFE, 0xFE, 0xFE, 0x42]);
    let mut serial = Serial::new(Model::default());
    serial.connect(Box::new(responses));

    // First three transfers get 0xFE
    for _ in 0..3 {
        serial.write(0xFF01, 0x01);
        serial.write(0xFF02, 0x81);
        let mut if_reg = 0u8;
        serial.step(0, 4096, false, &mut if_reg);
        assert_eq!(serial.read(0xFF01), 0xFE);
    }

    // Fourth transfer gets real data
    serial.write(0xFF01, 0x01);
    serial.write(0xFF02, 0x81);
    let mut if_reg = 0u8;
    serial.step(0, 4096, false, &mut if_reg);
    assert_eq!(serial.read(0xFF01), 0x42);
}
