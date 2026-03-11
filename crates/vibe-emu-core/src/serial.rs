use crate::hardware::DmgRevision;

/// Clock information for an in-flight serial transfer.
///
/// This is provided to [`LinkPort`] implementations so external link cable
/// endpoints can make timing-aware decisions (e.g. scheduling/pacing) without
/// coupling core emulation code to a specific on-wire protocol.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SerialTransferClock {
    /// Dot cycles per serial bit for this transfer.
    pub dot_cycles_per_bit: u32,
    /// Whether the CGB high-speed serial mode is requested (SC bit1).
    pub high_speed: bool,
    /// Whether the system is currently in CGB double-speed mode.
    pub double_speed: bool,
}

impl Default for SerialTransferClock {
    fn default() -> Self {
        Self {
            dot_cycles_per_bit: 512,
            high_speed: false,
            double_speed: false,
        }
    }
}

/// Endpoint abstraction for the Game Boy link cable.
///
/// Implementations may simulate a remote peer or bridge to an external system.
pub trait LinkPort: Send {
    /// Transfer a byte over the link. Returns the byte received from the
    /// partner. Implementations may perform the transfer immediately.
    fn transfer(&mut self, byte: u8) -> u8;

    /// Attempt to transfer a byte over the link without blocking.
    ///
    /// Returns `Some(byte)` once the partner byte is available; otherwise
    /// returns `None` and the transfer should be retried later.
    fn try_transfer(&mut self, byte: u8) -> Option<u8> {
        Some(self.transfer(byte))
    }

    /// Attempt to transfer a byte over the link without blocking while also
    /// providing the active transfer clock information.
    fn try_transfer_with_clock(&mut self, byte: u8, _clock: SerialTransferClock) -> Option<u8> {
        self.try_transfer(byte)
    }

    /// Attempt to complete an externally-clocked transfer without blocking.
    ///
    /// This is used when the local GB is the serial slave (`SC` bit0 = 0). The
    /// default behavior mirrors [`LinkPort::try_transfer`].
    fn try_external_transfer(&mut self, byte: u8) -> Option<u8> {
        self.try_transfer(byte)
    }
}

/// A stub link port used when no cable is attached.
/// By default it emulates a "line dead" scenario where incoming bits are all 1,
/// so any transfer receives 0xFF. When `loopback` is true the sent byte is
/// echoed back instead.
#[derive(Default)]
pub struct NullLinkPort {
    loopback: bool,
}

impl NullLinkPort {
    /// Creates a new stub link port.
    ///
    /// If `loopback` is `true`, transferred bytes are echoed back. Otherwise the
    /// port behaves like an open line (incoming bits read as 1), returning `0xFF`.
    pub fn new(loopback: bool) -> Self {
        Self { loopback }
    }
}

impl LinkPort for NullLinkPort {
    fn transfer(&mut self, byte: u8) -> u8 {
        if self.loopback { byte } else { 0xFF }
    }
}

/// Represents the Game Boy serial registers.
/// This struct handles SB/SC behavior and raises the serial interrupt
/// when a transfer completes.
pub struct Serial {
    sb: u8,
    sc: u8,
    pub(crate) out_buf: Vec<u8>,
    sb_out_buf: Vec<u8>,
    port: Box<dyn LinkPort + Send>,
    transfer: Option<TransferState>,
    cgb_mode: bool,
    dmg_revision: DmgRevision,
}

struct TransferState {
    remaining_bits: u8,
    outgoing: u8,
    pending_in: u8,
    incoming_latched: bool,
    internal_clock: bool,
    fast_clock: bool,
}

impl TransferState {
    fn new(outgoing: u8, internal_clock: bool, fast_clock: bool) -> Self {
        Self {
            remaining_bits: 8,
            outgoing,
            pending_in: 0xFF,
            incoming_latched: false,
            internal_clock,
            fast_clock,
        }
    }

    fn latch_incoming(&mut self, incoming: u8) {
        if self.incoming_latched {
            return;
        }
        self.pending_in = incoming;
        self.incoming_latched = true;
    }

    fn shift(&mut self, sb: &mut u8) -> bool {
        if self.remaining_bits == 0 {
            return true;
        }

        let incoming_bit = (self.pending_in & 0x80) != 0;
        self.pending_in <<= 1;
        *sb = (*sb << 1) | incoming_bit as u8;
        self.remaining_bits -= 1;
        self.remaining_bits == 0
    }
}

impl Serial {
    /// Creates a new serial unit.
    pub fn new(cgb: bool, dmg_revision: DmgRevision) -> Self {
        Self {
            sb: 0,
            sc: if cgb { 0x7F } else { 0x7E },
            out_buf: Vec::new(),
            sb_out_buf: Vec::new(),
            port: Box::new(NullLinkPort::default()),
            transfer: None,
            cgb_mode: cgb,
            dmg_revision,
        }
    }

    /// Attaches a link cable endpoint.
    pub fn connect(&mut self, port: Box<dyn LinkPort + Send>) {
        self.port = port;
    }

    /// Reads the SB/SC registers.
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF01 => self.sb,
            0xFF02 => {
                if self.cgb_mode {
                    self.sc
                } else {
                    self.sc | 0x7E
                }
            }
            _ => 0xFF,
        }
    }

    /// Writes the SB/SC registers.
    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF01 => {
                self.sb = val;
                // Many test ROMs (and some emulator test harnesses) treat writes to SB (rSB)
                // as a debug output channel even when a full serial transfer is never started.
                // Keep this separate from `out_buf` (completed transfers) so callers can
                // choose the appropriate interpretation.
                self.sb_out_buf.push(val);
            }
            0xFF02 => {
                if let Some(state) = self.transfer.as_mut() {
                    // Mid-transfer SC writes:
                    // - If bit7 is cleared, cancel the transfer.
                    // - If bit7 remains set, treat the write as a (re)start
                    //   request: restart the transfer using the current SB
                    //   value, and apply clock mode bits.
                    if val & 0x80 == 0 {
                        self.sc = val;
                        self.transfer = None;
                        return;
                    }

                    self.sc = val;
                    state.remaining_bits = 8;
                    state.outgoing = self.sb;
                    state.pending_in = 0xFF;
                    state.incoming_latched = false;
                    state.internal_clock = (val & 0x01) != 0;
                    state.fast_clock = (val & 0x02) != 0;
                    return;
                }

                self.sc = val;
                if val & 0x80 != 0 {
                    let internal_clock = val & 0x01 != 0;
                    let fast_clock = val & 0x02 != 0;
                    let state = TransferState::new(self.sb, internal_clock, fast_clock);
                    self.transfer = Some(state);
                }
            }
            _ => {}
        }
    }

    /// Deliver external clock pulses to the serial unit.
    ///
    /// Each pulse clocks one bit. This is only meaningful when the transfer
    /// is in external clock mode (SC bit0 = 0).
    pub fn external_clock_pulse(&mut self, count: u8, if_reg: &mut u8) {
        if self.transfer.is_none() {
            return;
        }

        let Some(state) = self.transfer.as_ref() else {
            return;
        };
        if state.internal_clock {
            return;
        }

        // In slave mode, wait until the partner byte is available before
        // clocking bits so SB reflects real incoming bits as it shifts.
        if !self.poll_transfer_byte(false, false) {
            return;
        }

        let mut transfer_complete = false;
        let mut completed_outgoing = 0;
        {
            let state = self.transfer.as_mut().unwrap();
            for _ in 0..count {
                if state.shift(&mut self.sb) {
                    transfer_complete = true;
                    completed_outgoing = state.outgoing;
                    break;
                }
            }
        }

        if transfer_complete {
            self.finish_transfer(completed_outgoing, if_reg);
        }
    }

    /// Advance the serial unit by the difference between `prev_div` and `curr_div`.
    pub fn step(&mut self, prev_div: u16, curr_div: u16, double_speed: bool, if_reg: &mut u8) {
        self.step_steps(
            prev_div,
            curr_div.wrapping_sub(prev_div),
            double_speed,
            if_reg,
        );
    }

    /// Advance the serial unit by an explicit number of divider `steps`.
    pub fn step_steps(&mut self, prev_div: u16, steps: u16, double_speed: bool, if_reg: &mut u8) {
        if self.transfer.is_none() {
            return;
        }

        let Some(state) = self.transfer.as_ref() else {
            return;
        };
        if !state.internal_clock {
            return;
        }

        // In master mode, defer clocking until we have the partner byte.
        if !self.poll_transfer_byte(true, double_speed) {
            return;
        }

        let (clock_bit, phase) = if let Some(state) = self.transfer.as_ref() {
            (
                clock_bit_index(self.cgb_mode, double_speed, state.fast_clock),
                self.phase_adjust(double_speed, state.fast_clock),
            )
        } else {
            return;
        };

        let mut transfer_complete = false;
        let mut completed_outgoing = 0;
        {
            let state = self.transfer.as_mut().unwrap();
            let mut div = prev_div;
            let mut prev_clock = ((div.wrapping_sub(phase) >> clock_bit) & 1) != 0;

            for _ in 0..steps {
                div = div.wrapping_add(1);
                let clock = ((div.wrapping_sub(phase) >> clock_bit) & 1) != 0;
                if prev_clock && !clock && state.shift(&mut self.sb) {
                    transfer_complete = true;
                    completed_outgoing = state.outgoing;
                    break;
                }
                prev_clock = clock;
            }
        }

        if transfer_complete {
            self.finish_transfer(completed_outgoing, if_reg);
        }
    }

    /// Take and return all bytes that have been received since the last call.
    pub fn take_output(&mut self) -> Vec<u8> {
        let out = self.out_buf.clone();
        self.out_buf.clear();
        out
    }

    /// Borrow the receive buffer without consuming it.
    pub fn peek_output(&self) -> &[u8] {
        &self.out_buf
    }

    /// Take the captured stream of raw writes to SB (rSB / FF01).
    pub fn take_sb_output(&mut self) -> Vec<u8> {
        let out = self.sb_out_buf.clone();
        self.sb_out_buf.clear();
        out
    }

    /// Peek the captured stream of raw writes to SB (rSB / FF01).
    pub fn peek_sb_output(&self) -> &[u8] {
        &self.sb_out_buf
    }

    /// Returns `true` if there is a pending transfer using external clock.
    ///
    /// This is useful for link cable emulation where the remote side needs to
    /// deliver clock pulses to complete the transfer.
    pub fn has_external_clock_transfer_pending(&self) -> bool {
        self.transfer
            .as_ref()
            .is_some_and(|state| !state.internal_clock)
    }

    /// Returns the outgoing byte for a pending external clock transfer.
    ///
    /// Returns `None` if no external clock transfer is pending.
    pub fn pending_external_clock_outgoing(&self) -> Option<u8> {
        self.transfer
            .as_ref()
            .filter(|state| !state.internal_clock)
            .map(|state| state.outgoing)
    }

    fn phase_adjust(&self, double_speed: bool, fast_clock: bool) -> u16 {
        if self.cgb_mode {
            return 0;
        }

        if double_speed || fast_clock {
            return 0;
        }

        match self.dmg_revision {
            DmgRevision::RevA | DmgRevision::RevB | DmgRevision::RevC => 0,
            DmgRevision::Rev0 => 0,
        }
    }

    fn poll_transfer_byte(&mut self, internal_clock: bool, double_speed: bool) -> bool {
        let Some(state) = self.transfer.as_ref() else {
            return false;
        };
        if state.incoming_latched {
            return true;
        }

        let outgoing = state.outgoing;
        let incoming = if internal_clock {
            let clock = if self.cgb_mode {
                SerialTransferClock {
                    dot_cycles_per_bit: serial_dot_cycles_per_bit(state.fast_clock, double_speed),
                    high_speed: state.fast_clock,
                    double_speed,
                }
            } else {
                // DMG has no double-speed mode and no high-speed serial.
                SerialTransferClock::default()
            };
            self.port.try_transfer_with_clock(outgoing, clock)
        } else {
            self.port.try_external_transfer(outgoing)
        };

        let Some(incoming) = incoming else {
            return false;
        };

        if let Some(state) = self.transfer.as_mut() {
            state.latch_incoming(incoming);
            true
        } else {
            false
        }
    }

    fn finish_transfer(&mut self, outgoing: u8, if_reg: &mut u8) {
        self.out_buf.push(outgoing);
        self.sc &= 0x7F;
        *if_reg |= 0x08;
        self.transfer = None;
    }
}

/// Returns dot cycles per serial bit for the requested speed mode.
///
/// This matches the standard CGB serial timing table:
/// - 8192 Hz   -> 512 dot cycles/bit
/// - 16384 Hz  -> 256 dot cycles/bit (double-speed mode)
/// - 262144 Hz -> 16 dot cycles/bit (high speed)
/// - 524288 Hz -> 8 dot cycles/bit (high speed + double-speed mode)
pub fn serial_dot_cycles_per_bit(high_speed: bool, double_speed: bool) -> u32 {
    match (high_speed, double_speed) {
        (false, false) => 512,
        (false, true) => 256,
        (true, false) => 16,
        (true, true) => 8,
    }
}

fn clock_bit_index(cgb_mode: bool, double_speed: bool, fast_clock: bool) -> u32 {
    if !cgb_mode {
        // DMG hardware has no double-speed mode.
        8
    } else {
        match (fast_clock, double_speed) {
            (false, false) => 8,
            (false, true) => 7,
            (true, false) => 3,
            (true, true) => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LinkPort, Serial, serial_dot_cycles_per_bit};
    use crate::hardware::DmgRevision;

    struct FixedInLinkPort {
        ret: u8,
        calls: usize,
        last_out: Option<u8>,
    }

    impl FixedInLinkPort {
        fn new(ret: u8) -> Self {
            Self {
                ret,
                calls: 0,
                last_out: None,
            }
        }
    }

    impl LinkPort for FixedInLinkPort {
        fn transfer(&mut self, byte: u8) -> u8 {
            self.calls += 1;
            self.last_out = Some(byte);
            self.ret
        }
    }

    struct DelayedTryPort {
        incoming: u8,
        polls_before_ready: usize,
        polls: usize,
    }

    impl DelayedTryPort {
        fn new(incoming: u8, polls_before_ready: usize) -> Self {
            Self {
                incoming,
                polls_before_ready,
                polls: 0,
            }
        }
    }

    impl LinkPort for DelayedTryPort {
        fn transfer(&mut self, _byte: u8) -> u8 {
            panic!("DelayedTryPort::transfer should not be used in this test");
        }

        fn try_transfer(&mut self, _byte: u8) -> Option<u8> {
            self.polls += 1;
            if self.polls > self.polls_before_ready {
                Some(self.incoming)
            } else {
                None
            }
        }
    }

    struct ExternalTryPort {
        incoming: u8,
    }

    impl ExternalTryPort {
        fn new(incoming: u8) -> Self {
            Self { incoming }
        }
    }

    impl LinkPort for ExternalTryPort {
        fn transfer(&mut self, _byte: u8) -> u8 {
            panic!("ExternalTryPort::transfer should not be used in this test");
        }

        fn try_transfer(&mut self, _byte: u8) -> Option<u8> {
            panic!("ExternalTryPort::try_transfer should not be used in this test");
        }

        fn try_external_transfer(&mut self, _byte: u8) -> Option<u8> {
            Some(self.incoming)
        }
    }

    #[test]
    fn sc_write_during_active_transfer_does_not_cancel() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80 | 0x01);

        // Attempt to clear SC mid-transfer. The transfer should be cancelled
        // and must not complete later.
        serial.write(0xFF02, 0x00);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);

        let mut if_reg = 0u8;
        serial.step(0, 4096, false, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);
    }

    #[test]
    fn internal_clock_transfer_completes_and_requests_irq() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        // For DMG internal clock, we clock on DIV bit 8 falling edges.
        // 8 bits = 8 falling edges = 8 * 512 DIV increments.
        serial.step(0, 4096, false, &mut if_reg);

        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn internal_clock_waits_for_partner_byte_before_clocking() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(DelayedTryPort::new(0x34, 1)));

        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        serial.step(0, 4096, false, &mut if_reg);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x12);

        serial.step(4096, 8192, false, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn internal_clock_shifts_bits_into_sb() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x80)));

        serial.write(0xFF01, 0x00);
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        serial.step(0, 512, false, &mut if_reg);
        assert_eq!(serial.read(0xFF01), 0x01);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        serial.step(512, 4096, false, &mut if_reg);
        assert_eq!(serial.read(0xFF01), 0x80);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
    }

    #[test]
    fn external_clock_stalls_without_pulses() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80);

        let mut if_reg = 0u8;
        serial.step(0, 60000, false, &mut if_reg);

        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);
    }

    #[test]
    fn external_clock_completes_with_pulses() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80);

        let mut if_reg = 0u8;
        serial.external_clock_pulse(7, &mut if_reg);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        serial.external_clock_pulse(1, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn external_clock_latches_and_shifts_partner_byte() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(ExternalTryPort::new(0xA5)));

        serial.write(0xFF01, 0x00);
        serial.write(0xFF02, 0x80);

        let mut if_reg = 0u8;
        serial.external_clock_pulse(1, &mut if_reg);
        assert_eq!(serial.read(0xFF01), 0x01);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        serial.external_clock_pulse(7, &mut if_reg);
        assert_eq!(serial.read(0xFF01), 0xA5);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
    }

    #[test]
    fn internal_clock_irq_only_on_final_bit_dmg() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        // 7 bits worth of falling edges: 7 * 512 DIV increments.
        serial.step(0, 3584, false, &mut if_reg);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        // Final bit.
        serial.step(3584, 4096, false, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn internal_clock_rate_cgb_normal_speed() {
        let mut serial = Serial::new(true, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        // CGB internal, normal clock.
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        // CGB normal clock uses DIV bit 8: 8 bits -> 8 * 512 increments.
        serial.step(0, 4095, false, &mut if_reg);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        serial.step(4095, 4096, false, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn internal_clock_rate_cgb_double_speed() {
        let mut serial = Serial::new(true, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        // CGB internal, normal clock (fast bit clear) in double-speed mode.
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        // In double-speed, CGB normal clock uses DIV bit 7: 8 bits -> 8 * 256.
        serial.step(0, 2047, true, &mut if_reg);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        serial.step(2047, 2048, true, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn internal_clock_rate_cgb_fast_clock() {
        let mut serial = Serial::new(true, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        // CGB internal + fast clock (SC bit1).
        serial.write(0xFF02, 0x80 | 0x01 | 0x02);

        let mut if_reg = 0u8;
        // Fast clock uses DIV bit 3 in normal speed: falling edges every 16.
        // 8 bits -> 8 * 16 = 128 increments.
        serial.step(0, 127, false, &mut if_reg);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        serial.step(127, 128, false, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn internal_clock_rate_cgb_fast_clock_double_speed() {
        let mut serial = Serial::new(true, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        // CGB internal + fast clock (SC bit1) in double-speed mode.
        serial.write(0xFF02, 0x80 | 0x01 | 0x02);

        let mut if_reg = 0u8;
        // Fast clock in double-speed uses 8 dot cycles per bit.
        serial.step(0, 63, true, &mut if_reg);
        assert_ne!(serial.read(0xFF02) & 0x80, 0);
        assert_eq!(if_reg & 0x08, 0);

        serial.step(63, 64, true, &mut if_reg);
        assert_eq!(serial.read(0xFF02) & 0x80, 0);
        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0x34);
    }

    #[test]
    fn serial_dot_cycles_match_speed_table() {
        assert_eq!(serial_dot_cycles_per_bit(false, false), 512);
        assert_eq!(serial_dot_cycles_per_bit(false, true), 256);
        assert_eq!(serial_dot_cycles_per_bit(true, false), 16);
        assert_eq!(serial_dot_cycles_per_bit(true, true), 8);
    }

    #[test]
    fn open_bus_no_partner_internal_clock_receives_ff() {
        let mut serial = Serial::new(false, DmgRevision::default());
        // No connect(): uses NullLinkPort which shifts in 1s.
        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        serial.step(0, 4096, false, &mut if_reg);

        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0xFF);
    }

    #[test]
    fn open_bus_no_partner_external_clock_receives_ff() {
        let mut serial = Serial::new(false, DmgRevision::default());
        // No connect(): uses NullLinkPort which shifts in 1s.
        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80);

        let mut if_reg = 0u8;
        serial.external_clock_pulse(8, &mut if_reg);

        assert_ne!(if_reg & 0x08, 0);
        assert_eq!(serial.read(0xFF01), 0xFF);
    }

    #[test]
    fn sc_write_with_bit7_restarts_transfer_using_current_sb() {
        let mut serial = Serial::new(false, DmgRevision::default());
        serial.connect(Box::new(FixedInLinkPort::new(0x34)));

        serial.write(0xFF01, 0x12);
        serial.write(0xFF02, 0x80 | 0x01);

        let mut if_reg = 0u8;
        // Advance one bit.
        serial.step(0, 512, false, &mut if_reg);
        assert_eq!(if_reg & 0x08, 0);

        // Update SB and restart.
        serial.write(0xFF01, 0x55);
        serial.write(0xFF02, 0x80 | 0x01);

        // Complete transfer.
        serial.step(512, 512 + 4096, false, &mut if_reg);
        assert_ne!(if_reg & 0x08, 0);
        // Output buffer records the outgoing byte that was actually used.
        assert_eq!(serial.peek_output().last().copied(), Some(0x55));
    }
}
