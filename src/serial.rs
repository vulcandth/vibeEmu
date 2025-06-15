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
}

impl Serial {
    pub fn new(cgb: bool) -> Self {
        Self {
            sb: 0,
            sc: if cgb { 0x7F } else { 0x7E },
            out_buf: Vec::new(),
            port: Box::new(NullLinkPort::default()),
        }
    }

    pub fn connect(&mut self, port: Box<dyn LinkPort>) {
        self.port = port;
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF01 => self.sb,
            0xFF02 => self.sc,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8, if_reg: &mut u8) {
        match addr {
            0xFF01 => self.sb = val,
            0xFF02 => {
                self.sc = val;
                if val & 0x80 != 0 {
                    self.out_buf.push(self.sb);
                    let received = self.port.transfer(self.sb);
                    self.sb = received;
                    self.sc &= 0x7F;
                    *if_reg |= 0x08;
                }
            }
            _ => {}
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
}
