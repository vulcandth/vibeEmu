use std::{fs, io, net::SocketAddr, path::PathBuf};

use std::mem::MaybeUninit;

use socket2::{Domain, Protocol, Socket, Type};

use crate::{MOBILE_CONFIG_SIZE, MOBILE_MAX_CONNECTIONS, MobileAddr, MobileHost, MobileSockType};

#[derive(Debug)]
enum ConnState {
    Empty,
    Tcp(Socket, TcpState),
    Udp(Socket),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TcpState {
    Unconnected,
    Connecting,
    Connected,
    Listening,
}

/// A non-blocking std host implementation using `socket2`.
///
/// This is intended for desktop/headless use. Android wrappers can implement
/// `MobileHost` directly.
#[derive(Debug)]
pub struct StdMobileHost {
    config_path: PathBuf,
    config: [u8; MOBILE_CONFIG_SIZE],
    conns: [ConnState; MOBILE_MAX_CONNECTIONS],
}

impl StdMobileHost {
    pub fn new(config_path: PathBuf) -> Self {
        let mut config = [0u8; MOBILE_CONFIG_SIZE];
        if let Ok(bytes) = fs::read(&config_path) {
            let len = bytes.len().min(config.len());
            config[..len].copy_from_slice(&bytes[..len]);
        }

        Self {
            config_path,
            config,
            conns: [ConnState::Empty, ConnState::Empty],
        }
    }

    fn save_config(&self) {
        let _ = fs::write(&self.config_path, self.config);
    }

    fn idx(conn: u32) -> Option<usize> {
        let idx = conn as usize;
        if idx < MOBILE_MAX_CONNECTIONS {
            Some(idx)
        } else {
            None
        }
    }

    fn socket_addr_from_mobile(addr: &MobileAddr) -> io::Result<SocketAddr> {
        addr.to_socket_addr()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing addr"))
    }

    fn domain_for(addr: &MobileAddr) -> io::Result<Domain> {
        match addr {
            MobileAddr::V4 { .. } => Ok(Domain::IPV4),
            MobileAddr::V6 { .. } => Ok(Domain::IPV6),
            MobileAddr::None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "addr type none",
            )),
        }
    }

    fn bind_any(socket: &Socket, addr: &MobileAddr, bind_port: u16) -> io::Result<()> {
        let sock_addr = match addr {
            MobileAddr::V4 { .. } => SocketAddr::from(([0, 0, 0, 0], bind_port)),
            MobileAddr::V6 { .. } => SocketAddr::from(([0u16; 8], bind_port)),
            MobileAddr::None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "addr type none",
                ));
            }
        };

        socket.set_reuse_address(true).ok();
        socket.bind(&sock_addr.into())
    }

    fn would_block(err: &io::Error) -> bool {
        if err.kind() == io::ErrorKind::WouldBlock {
            return true;
        }

        matches!(
            err.raw_os_error(),
            Some(code) if code == libc::EINPROGRESS || code == libc::EWOULDBLOCK
        )
    }

    fn connect_in_progress(err: &io::Error) -> bool {
        Self::would_block(err) || matches!(err.raw_os_error(), Some(code) if code == libc::EALREADY)
    }

    fn already_connected(err: &io::Error) -> bool {
        matches!(err.raw_os_error(), Some(code) if code == libc::EISCONN)
    }

    fn recv_into(socket: &Socket, out: &mut [u8]) -> io::Result<usize> {
        let mut buf: Vec<MaybeUninit<u8>> = vec![MaybeUninit::uninit(); out.len()];
        let n = socket.recv(&mut buf)?;
        let init = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };
        out[..n].copy_from_slice(init);
        Ok(n)
    }

    fn recv_from_into(socket: &Socket, out: &mut [u8]) -> io::Result<(usize, socket2::SockAddr)> {
        let mut buf: Vec<MaybeUninit<u8>> = vec![MaybeUninit::uninit(); out.len()];
        let (n, addr) = socket.recv_from(&mut buf)?;
        let init = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };
        out[..n].copy_from_slice(init);
        Ok((n, addr))
    }
}

impl MobileHost for StdMobileHost {
    fn config_read(&mut self, dest: &mut [u8], offset: usize) -> bool {
        if offset + dest.len() > self.config.len() {
            return false;
        }
        dest.copy_from_slice(&self.config[offset..offset + dest.len()]);
        true
    }

    fn config_write(&mut self, src: &[u8], offset: usize) -> bool {
        if offset + src.len() > self.config.len() {
            return false;
        }
        self.config[offset..offset + src.len()].copy_from_slice(src);
        self.save_config();
        true
    }

    fn sock_open(
        &mut self,
        conn: u32,
        socktype: MobileSockType,
        addr: &MobileAddr,
        bind_port: u16,
    ) -> bool {
        let Some(idx) = Self::idx(conn) else {
            return false;
        };

        let domain = match Self::domain_for(addr) {
            Ok(d) => d,
            Err(_) => return false,
        };

        let state = match socktype {
            MobileSockType::Tcp => {
                let socket = match Socket::new(domain, Type::STREAM, Some(Protocol::TCP)) {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                if socket.set_nonblocking(true).is_err() {
                    return false;
                }
                if Self::bind_any(&socket, addr, bind_port).is_err() {
                    return false;
                }
                ConnState::Tcp(socket, TcpState::Unconnected)
            }
            MobileSockType::Udp => {
                let socket = match Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)) {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                if socket.set_nonblocking(true).is_err() {
                    return false;
                }
                if Self::bind_any(&socket, addr, bind_port).is_err() {
                    return false;
                }
                ConnState::Udp(socket)
            }
        };

        self.conns[idx] = state;
        true
    }

    fn sock_close(&mut self, conn: u32) {
        if let Some(idx) = Self::idx(conn) {
            self.conns[idx] = ConnState::Empty;
        }
    }

    fn sock_connect(&mut self, conn: u32, addr: &MobileAddr) -> i32 {
        let Some(idx) = Self::idx(conn) else {
            return -1;
        };

        let target = match Self::socket_addr_from_mobile(addr) {
            Ok(a) => a,
            Err(_) => return -1,
        };

        match &mut self.conns[idx] {
            ConnState::Tcp(socket, state) => {
                if *state == TcpState::Connected {
                    return 1;
                }

                match socket.connect(&target.into()) {
                    Ok(()) => {
                        *state = TcpState::Connected;
                        1
                    }
                    Err(e) if Self::already_connected(&e) => {
                        *state = TcpState::Connected;
                        1
                    }
                    Err(e) if Self::connect_in_progress(&e) => {
                        *state = TcpState::Connecting;
                        0
                    }
                    Err(_) => -1,
                }
            }
            ConnState::Udp(socket) => match socket.connect(&target.into()) {
                Ok(()) => 1,
                Err(_) => -1,
            },
            ConnState::Empty => -1,
        }
    }

    fn sock_listen(&mut self, conn: u32) -> bool {
        let Some(idx) = Self::idx(conn) else {
            return false;
        };

        match &mut self.conns[idx] {
            ConnState::Tcp(socket, state) if socket.listen(1).is_ok() => {
                *state = TcpState::Listening;
                true
            }
            _ => false,
        }
    }

    fn sock_accept(&mut self, conn: u32) -> bool {
        let Some(idx) = Self::idx(conn) else {
            return false;
        };

        let ConnState::Tcp(socket, state) = &mut self.conns[idx] else {
            return false;
        };
        if *state != TcpState::Listening {
            return false;
        }

        match socket.accept() {
            Ok((accepted, _addr)) => {
                let _ = accepted.set_nonblocking(true);
                self.conns[idx] = ConnState::Tcp(accepted, TcpState::Connected);
                true
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => false,
            Err(_) => false,
        }
    }

    fn sock_send(&mut self, conn: u32, data: &[u8], addr: Option<&MobileAddr>) -> i32 {
        let Some(idx) = Self::idx(conn) else {
            return -1;
        };

        match &self.conns[idx] {
            ConnState::Tcp(socket, _state) => match socket.send(data) {
                Ok(n) => n as i32,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
                Err(_) => -1,
            },
            ConnState::Udp(socket) => {
                if let Some(addr) = addr {
                    let target = match Self::socket_addr_from_mobile(addr) {
                        Ok(a) => a,
                        Err(_) => return -1,
                    };
                    match socket.send_to(data, &target.into()) {
                        Ok(n) => n as i32,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
                        Err(_) => -1,
                    }
                } else {
                    match socket.send(data) {
                        Ok(n) => n as i32,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
                        Err(_) => -1,
                    }
                }
            }
            ConnState::Empty => -1,
        }
    }

    fn sock_recv(
        &mut self,
        conn: u32,
        data: Option<&mut [u8]>,
        addr_out: Option<&mut MobileAddr>,
    ) -> i32 {
        let Some(idx) = Self::idx(conn) else {
            return -1;
        };

        match &self.conns[idx] {
            ConnState::Tcp(socket, _state) => match data {
                None => {
                    let mut one = [MaybeUninit::<u8>::uninit(); 1];
                    match socket.peek(&mut one) {
                        Ok(0) => -2,
                        Ok(_) => 0,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
                        Err(_) => -1,
                    }
                }
                Some(buf) => match Self::recv_into(socket, buf) {
                    Ok(0) => -2,
                    Ok(n) => n as i32,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
                    Err(_) => -1,
                },
            },
            ConnState::Udp(socket) => {
                let Some(buf) = data else {
                    return 0;
                };

                if let Some(addr_out) = addr_out {
                    match Self::recv_from_into(socket, buf) {
                        Ok((n, addr)) => {
                            if let Some(sa) = addr.as_socket() {
                                match sa {
                                    SocketAddr::V4(v4) => {
                                        *addr_out = MobileAddr::V4 {
                                            host: v4.ip().octets(),
                                            port: v4.port(),
                                        };
                                    }
                                    SocketAddr::V6(v6) => {
                                        *addr_out = MobileAddr::V6 {
                                            host: v6.ip().octets(),
                                            port: v6.port(),
                                        };
                                    }
                                }
                            }
                            n as i32
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
                        Err(_) => -1,
                    }
                } else {
                    match Self::recv_into(socket, buf) {
                        Ok(n) => n as i32,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
                        Err(_) => -1,
                    }
                }
            }
            ConnState::Empty => -1,
        }
    }
}

// Minimal libc constants for EINPROGRESS on Windows are different; use libc for portability.
// This is a small dependency and already widely used.
mod libc {
    #[cfg(windows)]
    pub const EINPROGRESS: i32 = 10036; // WSAEINPROGRESS

    #[cfg(windows)]
    pub const EWOULDBLOCK: i32 = 10035; // WSAEWOULDBLOCK

    #[cfg(windows)]
    pub const EALREADY: i32 = 10037; // WSAEALREADY

    #[cfg(windows)]
    pub const EISCONN: i32 = 10056; // WSAEISCONN

    #[cfg(not(windows))]
    pub const EINPROGRESS: i32 = 115;

    #[cfg(not(windows))]
    pub const EWOULDBLOCK: i32 = 11;

    #[cfg(not(windows))]
    pub const EALREADY: i32 = 114;

    #[cfg(not(windows))]
    pub const EISCONN: i32 = 106;
}
