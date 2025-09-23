use crate::hardware::DmgRevision;

pub trait LinkPort {
    /// Transfer a byte over the link. Returns the byte received from the
    /// partner. Implementations may perform the transfer immediately.
    fn transfer(&mut self, byte: u8) -> u8;
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
    port: Box<dyn LinkPort>,
    transfer: Option<TransferState>,
    cgb_mode: bool,
    dmg_revision: DmgRevision,
}

struct TransferState {
    remaining_bits: u8,
    outgoing: u8,
    incoming: u8,
    pending_in: u8,
    internal_clock: bool,
    fast_clock: bool,
}

impl TransferState {
    fn new(outgoing: u8, incoming: u8, internal_clock: bool, fast_clock: bool) -> Self {
        Self {
            remaining_bits: 8,
            outgoing,
            incoming,
            pending_in: incoming,
            internal_clock,
            fast_clock,
        }
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
    pub fn new(cgb: bool, dmg_revision: DmgRevision) -> Self {
        Self {
            sb: 0,
            sc: if cgb { 0x7F } else { 0x7E },
            out_buf: Vec::new(),
            port: Box::new(NullLinkPort::default()),
            transfer: None,
            cgb_mode: cgb,
            dmg_revision,
        }
    }

    pub fn connect(&mut self, port: Box<dyn LinkPort>) {
        self.port = port;
    }

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

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0xFF01 => self.sb = val,
            0xFF02 => {
                self.sc = val;
                if val & 0x80 != 0 {
                    let outgoing = self.sb;
                    let incoming = self.port.transfer(self.sb);
                    let internal_clock = val & 0x01 != 0;
                    let fast_clock = val & 0x02 != 0;
                    self.transfer = Some(TransferState::new(
                        outgoing,
                        incoming,
                        internal_clock,
                        fast_clock,
                    ));
                    // When using an external clock the transfer will only
                    // complete if the link partner supplies the necessary
                    // pulses, so we simply keep SC bit 7 asserted until the
                    // clock edges arrive.
                }
            }
            _ => {}
        }
    }

    pub fn step(&mut self, prev_div: u16, curr_div: u16, double_speed: bool, if_reg: &mut u8) {
        if self.transfer.is_none() {
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

        let mut complete = false;
        {
            let state = self.transfer.as_mut().unwrap();
            let mut div = prev_div;
            let steps = curr_div.wrapping_sub(prev_div);
            let mut prev_clock = ((div.wrapping_sub(phase) >> clock_bit) & 1) != 0;

            for _ in 0..steps {
                div = div.wrapping_add(1);
                let clock = ((div.wrapping_sub(phase) >> clock_bit) & 1) != 0;
                if state.internal_clock && prev_clock && !clock && state.shift(&mut self.sb) {
                    complete = true;
                    break;
                }
                prev_clock = clock;
            }
        }

        if complete {
            let state = self.transfer.take().unwrap();
            self.sb = state.incoming;
            self.out_buf.push(state.outgoing);
            self.sc &= 0x7F;
            *if_reg |= 0x08;
        }
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        let out = self.out_buf.clone();
        self.out_buf.clear();
        out
    }

    pub fn peek_output(&self) -> &[u8] {
        &self.out_buf
    }

    fn phase_adjust(&self, double_speed: bool, fast_clock: bool) -> u16 {
        if self.cgb_mode {
            return 0;
        }

        if double_speed || fast_clock {
            return 0;
        }

        match self.dmg_revision {
            DmgRevision::RevA | DmgRevision::RevB | DmgRevision::RevC => 0x34,
            DmgRevision::Rev0 => 0,
        }
    }
}

fn clock_bit_index(cgb_mode: bool, double_speed: bool, fast_clock: bool) -> u32 {
    if !cgb_mode {
        if double_speed { 7 } else { 8 }
    } else {
        match (fast_clock, double_speed) {
            (false, false) => 8,
            (false, true) => 7,
            (true, false) => 3,
            (true, true) => 2,
        }
    }
}
