#![cfg_attr(
    any(feature = "bundled", feature = "system"),
    allow(unsafe_op_in_unsafe_fn)
)]

//! Safe wrapper for the Game Boy Mobile Adapter (libmobile).
//!
//! This crate provides a host-driven [`MobileAdapter`] that can be wired into the
//! emulator serial unit via [`MobileLinkPort`]. The underlying backend is enabled
//! by the `bundled` or `system` features.

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};

#[cfg(any(feature = "bundled", feature = "system"))]
use std::{ffi::CStr, os::raw::c_void};

use thiserror::Error;

use vibe_emu_core::serial::LinkPort;

#[cfg(any(feature = "bundled", feature = "system"))]
use vibe_emu_mobile_sys as sys;

mod std_host;

pub use std_host::StdMobileHost;

#[cfg(target_os = "android")]
pub fn install_android_log_sink() {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};

    const TAG: &str = "vibeEmu";

    unsafe extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    struct AndroidLogSink;

    impl vibe_emu_core::diagnostics::LogSink for AndroidLogSink {
        fn log(
            &self,
            level: vibe_emu_core::diagnostics::Level,
            target: &'static str,
            args: std::fmt::Arguments,
        ) {
            // Android priorities: VERBOSE=2, DEBUG=3, INFO=4, WARN=5.
            let prio = match level {
                vibe_emu_core::diagnostics::Level::Trace => 3,
                vibe_emu_core::diagnostics::Level::Info => 4,
                vibe_emu_core::diagnostics::Level::Warn => 5,
            };

            let tag = CString::new(TAG).ok();
            let mut message = String::new();
            let _ = std::fmt::write(&mut message, args);

            if message.contains('\0') {
                message = message.replace('\0', "?");
            }

            let message = if target.is_empty() {
                message
            } else {
                format!("[{target}] {message}")
            };

            let Some(tag) = tag else {
                return;
            };
            let Ok(message) = CString::new(message) else {
                return;
            };

            unsafe {
                let _ = __android_log_write(prio, tag.as_ptr(), message.as_ptr());
            }
        }
    }

    let _ = vibe_emu_core::diagnostics::try_set_log_sink(Box::new(AndroidLogSink));
}

#[cfg(not(target_os = "android"))]
pub fn install_android_log_sink() {}

/// Maximum number of concurrent libmobile connections.
pub const MOBILE_MAX_CONNECTIONS: usize = 2;
/// Maximum number of independent libmobile timers.
pub const MOBILE_MAX_TIMERS: usize = 4;
/// Size of the persisted configuration blob.
pub const MOBILE_CONFIG_SIZE: usize = 0x200;
/// Byte value returned by libmobile while the serial line is idle.
pub const MOBILE_SERIAL_IDLE_BYTE: u8 = 0xD2;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
/// Cosmetic device selection for the Mobile Adapter.
pub enum MobileAdapterDevice {
    #[default]
    Blue,
    Yellow,
    Green,
    Red,
}

/// Frontend-agnostic configuration overrides for libmobile.
///
/// These are applied on top of any persisted `MOBILE_CONFIG_SIZE` blob.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MobileConfig {
    pub device: MobileAdapterDevice,
    pub unmetered: bool,
    pub dns1: MobileAddr,
    pub dns2: MobileAddr,
    pub p2p_port: Option<u16>,
    pub relay: MobileAddr,
    pub relay_token: Option<[u8; 16]>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Which phone number (user or peer) is being updated.
pub enum MobileNumber {
    User,
    Peer,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Socket type requested by libmobile.
pub enum MobileSockType {
    Tcp,
    Udp,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
/// IP address + port representation used by libmobile.
pub enum MobileAddr {
    #[default]
    None,
    V4 {
        host: [u8; 4],
        port: u16,
    },
    V6 {
        host: [u8; 16],
        port: u16,
    },
}

impl MobileAddr {
    /// Converts this address into a standard library `SocketAddr`.
    ///
    /// Returns `None` for [`MobileAddr::None`].
    pub fn to_socket_addr(&self) -> Option<SocketAddr> {
        match self {
            MobileAddr::None => None,
            MobileAddr::V4 { host, port } => Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(host[0], host[1], host[2], host[3])),
                *port,
            )),
            MobileAddr::V6 { host, port } => {
                let ip = Ipv6Addr::from(*host);
                Some(SocketAddr::new(IpAddr::V6(ip), *port))
            }
        }
    }
}

/// Host callbacks required by [`MobileAdapter`].
///
/// The host provides persistence for the `MOBILE_CONFIG_SIZE` blob and implements
/// network socket I/O.
pub trait MobileHost: Send {
    /// Optional debug log line emitted by libmobile.
    fn debug_log(&mut self, _line: &str) {}
    /// Optional callback used to expose the user/peer phone numbers.
    fn update_number(&mut self, _which: MobileNumber, _number: Option<&str>) {}

    /// Reads bytes from the persisted config blob into `dest`.
    fn config_read(&mut self, dest: &mut [u8], offset: usize) -> bool;
    /// Writes bytes from `src` into the persisted config blob.
    fn config_write(&mut self, src: &[u8], offset: usize) -> bool;

    fn sock_open(
        &mut self,
        conn: u32,
        socktype: MobileSockType,
        addr: &MobileAddr,
        bind_port: u16,
    ) -> bool;
    /// Closes the socket associated with `conn`.
    fn sock_close(&mut self, conn: u32);
    /// Connects the socket for `conn` to `addr`.
    fn sock_connect(&mut self, conn: u32, addr: &MobileAddr) -> i32;
    /// Starts listening on the socket for `conn`.
    fn sock_listen(&mut self, conn: u32) -> bool;
    /// Accepts a pending connection for `conn`.
    fn sock_accept(&mut self, conn: u32) -> bool;

    fn sock_send(&mut self, conn: u32, data: &[u8], addr: Option<&MobileAddr>) -> i32;

    /// Returns:
    /// - `> 0` bytes received
    /// - `0` no data available (non-blocking)
    /// - `-2` remote disconnected
    /// - `-1` error
    fn sock_recv(
        &mut self,
        conn: u32,
        data: Option<&mut [u8]>,
        addr_out: Option<&mut MobileAddr>,
    ) -> i32;
}

#[derive(Error, Debug)]
pub enum MobileError {
    #[error("libmobile backend is not enabled (build with features 'bundled' or 'system')")]
    Unavailable,

    #[error("libmobile failed to allocate adapter")]
    AllocationFailed,

    #[error("invalid libmobile usage: {0}")]
    InvalidState(&'static str),
}

/// Safe-ish wrapper around a libmobile instance.
///
/// Design notes:
/// - `poll()` must be called regularly (<= ~100ms) to drive libmobile.
/// - `transfer_byte()` is intended to be called via the emulator serial unit.
pub struct MobileAdapter {
    #[cfg(any(feature = "bundled", feature = "system"))]
    inner: Option<Box<MobileAdapterInner>>,

    #[cfg(not(any(feature = "bundled", feature = "system")))]
    _unavailable: (),
}

impl fmt::Debug for MobileAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MobileAdapter").finish_non_exhaustive()
    }
}

impl MobileAdapter {
    /// Create a new adapter instance.
    ///
    /// `host` provides configuration persistence, socket I/O, and optional logging.
    ///
    /// Note: this does not call `start()`.
    pub fn new(host: Box<dyn MobileHost>) -> Result<Self, MobileError> {
        #[cfg(any(feature = "bundled", feature = "system"))]
        {
            let inner = MobileAdapterInner::new(host)?;
            Ok(Self { inner: Some(inner) })
        }

        #[cfg(not(any(feature = "bundled", feature = "system")))]
        {
            let _ = host;
            Err(MobileError::Unavailable)
        }
    }

    /// Convenience constructor for the built-in host.
    pub fn new_std(config_path: PathBuf) -> Result<Self, MobileError> {
        let host = Box::new(StdMobileHost::new(config_path));
        Self::new(host)
    }

    pub fn start(&mut self) -> Result<(), MobileError> {
        #[cfg(any(feature = "bundled", feature = "system"))]
        {
            let inner = self
                .inner
                .as_mut()
                .ok_or(MobileError::InvalidState("missing inner"))?;
            inner.start();
            Ok(())
        }

        #[cfg(not(any(feature = "bundled", feature = "system")))]
        {
            Err(MobileError::Unavailable)
        }
    }

    /// Stops libmobile.
    ///
    /// Stopping is idempotent; if the backend isn't running, this returns `Ok(())`.
    pub fn stop(&mut self) -> Result<(), MobileError> {
        #[cfg(any(feature = "bundled", feature = "system"))]
        {
            let Some(inner) = self.inner.as_mut() else {
                return Ok(());
            };
            inner.stop();
            Ok(())
        }

        #[cfg(not(any(feature = "bundled", feature = "system")))]
        {
            Err(MobileError::Unavailable)
        }
    }

    /// Advance emulated time and drive libmobile.
    ///
    /// Call this at least once per frame/tick, ensuring the wall interval between calls
    /// is never too large (libmobile recommends <= 100ms).
    pub fn poll(&mut self, delta_ms: u32) -> Result<(), MobileError> {
        #[cfg(any(feature = "bundled", feature = "system"))]
        {
            let inner = self
                .inner
                .as_mut()
                .ok_or(MobileError::InvalidState("missing inner"))?;
            inner.emulated_time_ms = inner.emulated_time_ms.saturating_add(delta_ms as u64);
            inner.poll();
            Ok(())
        }

        #[cfg(not(any(feature = "bundled", feature = "system")))]
        {
            let _ = delta_ms;
            Err(MobileError::Unavailable)
        }
    }

    /// Transfers a single serial byte through libmobile.
    ///
    /// This is intended to be called from the emulator serial unit.
    pub fn transfer_byte(&mut self, byte: u8) -> Result<u8, MobileError> {
        #[cfg(any(feature = "bundled", feature = "system"))]
        {
            let inner = self
                .inner
                .as_mut()
                .ok_or(MobileError::InvalidState("missing inner"))?;
            Ok(inner.transfer(byte))
        }

        #[cfg(not(any(feature = "bundled", feature = "system")))]
        {
            let _ = byte;
            Err(MobileError::Unavailable)
        }
    }

    /// Apply configuration overrides via libmobile's public config setters.
    ///
    /// This is optional: games can also write adapter configuration over the on-wire protocol.
    pub fn apply_config(&mut self, config: &MobileConfig) -> Result<(), MobileError> {
        #[cfg(any(feature = "bundled", feature = "system"))]
        {
            let inner = self
                .inner
                .as_mut()
                .ok_or(MobileError::InvalidState("missing inner"))?;
            inner.apply_config(config);
            Ok(())
        }

        #[cfg(not(any(feature = "bundled", feature = "system")))]
        {
            let _ = config;
            Err(MobileError::Unavailable)
        }
    }

    /// Returns the idle filler byte used by the mobile serial protocol.
    pub fn idle_byte() -> u8 {
        MOBILE_SERIAL_IDLE_BYTE
    }
}

/// A `LinkPort` adapter that forwards each byte to libmobile.
///
/// Note: `transfer()` is synchronous by design (matches `LinkPort`). libmobile itself
/// is designed for this; its broader networking/event processing is driven by `MobileAdapter::poll()`.
pub struct MobileLinkPort {
    adapter: std::sync::Arc<std::sync::Mutex<MobileAdapter>>,
}

impl MobileLinkPort {
    /// Wraps a [`MobileAdapter`] behind a mutex and exposes it as a `LinkPort`.
    pub fn new(adapter: std::sync::Arc<std::sync::Mutex<MobileAdapter>>) -> Self {
        Self { adapter }
    }
}

impl LinkPort for MobileLinkPort {
    fn transfer(&mut self, byte: u8) -> u8 {
        let mut adapter = self.adapter.lock().expect("mobile adapter mutex poisoned");
        adapter.transfer_byte(byte).unwrap_or(0xFF)
    }
}

#[cfg(any(feature = "bundled", feature = "system"))]
struct MobileAdapterInner {
    adapter: *mut sys::mobile_adapter,
    host: Box<dyn MobileHost>,
    emulated_time_ms: u64,
    timers_latched_ms: [u64; MOBILE_MAX_TIMERS],
    serial_enabled: bool,
    mode_32bit: bool,
}

// Safety: `MobileAdapterInner` is only accessed through `MobileAdapter`, which is intended
// to be used behind a synchronization primitive (e.g. `Mutex`). libmobile's non-thread-safe
// entrypoints (`mobile_loop`, config operations) are serialized by that outer locking.
#[cfg(any(feature = "bundled", feature = "system"))]
unsafe impl Send for MobileAdapterInner {}

#[cfg(any(feature = "bundled", feature = "system"))]
impl MobileAdapterInner {
    fn new(host: Box<dyn MobileHost>) -> Result<Box<Self>, MobileError> {
        let mut inner = Box::new(Self {
            adapter: std::ptr::null_mut(),
            host,
            emulated_time_ms: 0,
            timers_latched_ms: [0; MOBILE_MAX_TIMERS],
            serial_enabled: false,
            mode_32bit: false,
        });

        let user_ptr: *mut c_void = (&mut *inner) as *mut _ as *mut c_void;

        unsafe {
            let adapter = sys::mobile_new(user_ptr);
            if adapter.is_null() {
                return Err(MobileError::AllocationFailed);
            }
            inner.adapter = adapter;

            // Install callbacks.
            sys::mobile_def_debug_log(adapter, Some(cb_debug_log));
            sys::mobile_def_serial_disable(adapter, Some(cb_serial_disable));
            sys::mobile_def_serial_enable(adapter, Some(cb_serial_enable));
            sys::mobile_def_config_read(adapter, Some(cb_config_read));
            sys::mobile_def_config_write(adapter, Some(cb_config_write));
            sys::mobile_def_time_latch(adapter, Some(cb_time_latch));
            sys::mobile_def_time_check_ms(adapter, Some(cb_time_check_ms));
            sys::mobile_def_sock_open(adapter, Some(cb_sock_open));
            sys::mobile_def_sock_close(adapter, Some(cb_sock_close));
            sys::mobile_def_sock_connect(adapter, Some(cb_sock_connect));
            sys::mobile_def_sock_listen(adapter, Some(cb_sock_listen));
            sys::mobile_def_sock_accept(adapter, Some(cb_sock_accept));
            sys::mobile_def_sock_send(adapter, Some(cb_sock_send));
            sys::mobile_def_sock_recv(adapter, Some(cb_sock_recv));
            sys::mobile_def_update_number(adapter, Some(cb_update_number));

            // Load config early so config_set_* can be called by frontends if desired.
            sys::mobile_config_load(adapter);
        }

        Ok(inner)
    }

    fn start(&mut self) {
        unsafe { sys::mobile_start(self.adapter) };
    }

    fn stop(&mut self) {
        unsafe { sys::mobile_stop(self.adapter) };
    }

    fn poll(&mut self) {
        unsafe { sys::mobile_loop(self.adapter) };
    }

    fn transfer(&mut self, byte: u8) -> u8 {
        if !self.serial_enabled {
            return MOBILE_SERIAL_IDLE_BYTE;
        }
        if self.mode_32bit {
            // Not expected for GB/GBC. Keep behavior safe.
            return MOBILE_SERIAL_IDLE_BYTE;
        }

        unsafe { sys::mobile_transfer(self.adapter, byte) }
    }

    fn apply_config(&mut self, config: &MobileConfig) {
        let device = match config.device {
            MobileAdapterDevice::Blue => sys::mobile_adapter_device::MOBILE_ADAPTER_BLUE,
            MobileAdapterDevice::Yellow => sys::mobile_adapter_device::MOBILE_ADAPTER_YELLOW,
            MobileAdapterDevice::Green => sys::mobile_adapter_device::MOBILE_ADAPTER_GREEN,
            MobileAdapterDevice::Red => sys::mobile_adapter_device::MOBILE_ADAPTER_RED,
        };

        let mut dns1 = sys::mobile_addr {
            type_: sys::mobile_addrtype::MOBILE_ADDRTYPE_NONE,
        };
        let mut dns2 = sys::mobile_addr {
            type_: sys::mobile_addrtype::MOBILE_ADDRTYPE_NONE,
        };
        let mut relay = sys::mobile_addr {
            type_: sys::mobile_addrtype::MOBILE_ADDRTYPE_NONE,
        };

        unsafe {
            encode_addr(&mut dns1, &config.dns1);
            encode_addr(&mut dns2, &config.dns2);
            encode_addr(&mut relay, &config.relay);

            sys::mobile_config_set_device(self.adapter, device, config.unmetered);
            sys::mobile_config_set_dns(self.adapter, &dns1, &dns2);

            if let Some(port) = config.p2p_port {
                sys::mobile_config_set_p2p_port(self.adapter, port as u32);
            }

            sys::mobile_config_set_relay(self.adapter, &relay);

            if let Some(token) = config.relay_token {
                sys::mobile_config_set_relay_token(self.adapter, token.as_ptr());
            }
        }
    }
}

#[cfg(any(feature = "bundled", feature = "system"))]
impl Drop for MobileAdapterInner {
    fn drop(&mut self) {
        // Ensure libmobile is stopped before freeing.
        if !self.adapter.is_null() {
            unsafe {
                sys::mobile_stop(self.adapter);
                // libmobile allocates with malloc; free() is the right pairing.
                // This is safe because mobile_new() documents free() usage.
                libc_free(self.adapter as *mut c_void);
            }
        }
    }
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe fn libc_free(ptr: *mut c_void) {
    unsafe extern "C" {
        fn free(ptr: *mut c_void);
    }

    free(ptr);
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe fn with_user<'a>(user: *mut c_void) -> &'a mut MobileAdapterInner {
    &mut *(user as *mut MobileAdapterInner)
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_debug_log(user: *mut c_void, line: *const std::os::raw::c_char) {
    if user.is_null() || line.is_null() {
        return;
    }

    let inner = with_user(user);
    let Ok(line) = CStr::from_ptr(line).to_str() else {
        return;
    };
    inner.host.debug_log(line);
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_serial_disable(user: *mut c_void) {
    if user.is_null() {
        return;
    }

    let inner = with_user(user);
    inner.serial_enabled = false;
    inner.mode_32bit = false;
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_serial_enable(user: *mut c_void, mode_32bit: bool) {
    if user.is_null() {
        return;
    }

    let inner = with_user(user);
    inner.serial_enabled = true;
    inner.mode_32bit = mode_32bit;
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_config_read(
    user: *mut c_void,
    dest: *mut c_void,
    offset: usize,
    size: usize,
) -> bool {
    if user.is_null() {
        return false;
    }

    let inner = with_user(user);

    if dest.is_null() {
        return false;
    }

    let buf = core::slice::from_raw_parts_mut(dest as *mut u8, size);
    inner.host.config_read(buf, offset)
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_config_write(
    user: *mut c_void,
    src: *const c_void,
    offset: usize,
    size: usize,
) -> bool {
    if user.is_null() {
        return false;
    }

    let inner = with_user(user);

    if src.is_null() {
        return false;
    }

    let buf = core::slice::from_raw_parts(src as *const u8, size);
    inner.host.config_write(buf, offset)
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_time_latch(user: *mut c_void, timer: u32) {
    if user.is_null() {
        return;
    }

    let inner = with_user(user);
    let Some(slot) = inner.timers_latched_ms.get_mut(timer as usize) else {
        return;
    };
    *slot = inner.emulated_time_ms;
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_time_check_ms(user: *mut c_void, timer: u32, ms: u32) -> bool {
    if user.is_null() {
        return false;
    }

    let inner = with_user(user);
    let Some(&latched) = inner.timers_latched_ms.get(timer as usize) else {
        return false;
    };
    inner.emulated_time_ms.saturating_sub(latched) >= ms as u64
}

#[cfg(any(feature = "bundled", feature = "system"))]
fn decode_addr(addr: *const sys::mobile_addr) -> MobileAddr {
    if addr.is_null() {
        return MobileAddr::None;
    }

    unsafe {
        match (*addr).type_ {
            sys::mobile_addrtype::MOBILE_ADDRTYPE_IPV4 => {
                let a4 = (*addr)._addr4;
                MobileAddr::V4 {
                    host: a4.host,
                    port: (a4.port as u16),
                }
            }
            sys::mobile_addrtype::MOBILE_ADDRTYPE_IPV6 => {
                let a6 = (*addr)._addr6;
                MobileAddr::V6 {
                    host: a6.host,
                    port: (a6.port as u16),
                }
            }
            _ => MobileAddr::None,
        }
    }
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe fn encode_addr(out: &mut sys::mobile_addr, addr: &MobileAddr) {
    match addr {
        MobileAddr::None => {
            out.type_ = sys::mobile_addrtype::MOBILE_ADDRTYPE_NONE;
        }
        MobileAddr::V4 { host, port } => {
            out._addr4 = sys::mobile_addr4 {
                type_: sys::mobile_addrtype::MOBILE_ADDRTYPE_IPV4,
                port: *port as u32,
                host: *host,
            };
        }
        MobileAddr::V6 { host, port } => {
            out._addr6 = sys::mobile_addr6 {
                type_: sys::mobile_addrtype::MOBILE_ADDRTYPE_IPV6,
                port: *port as u32,
                host: *host,
            };
        }
    }
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_sock_open(
    user: *mut c_void,
    conn: u32,
    socktype: sys::mobile_socktype,
    addrtype: sys::mobile_addrtype,
    bindport: u32,
) -> bool {
    if user.is_null() {
        return false;
    }

    let inner = with_user(user);

    let st = match socktype {
        sys::mobile_socktype::MOBILE_SOCKTYPE_TCP => MobileSockType::Tcp,
        sys::mobile_socktype::MOBILE_SOCKTYPE_UDP => MobileSockType::Udp,
    };

    let addr = match addrtype {
        sys::mobile_addrtype::MOBILE_ADDRTYPE_IPV4 => MobileAddr::V4 {
            host: [0, 0, 0, 0],
            port: bindport as u16,
        },
        sys::mobile_addrtype::MOBILE_ADDRTYPE_IPV6 => MobileAddr::V6 {
            host: [0; 16],
            port: bindport as u16,
        },
        _ => MobileAddr::None,
    };

    inner.host.sock_open(conn, st, &addr, bindport as u16)
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_sock_close(user: *mut c_void, conn: u32) {
    if user.is_null() {
        return;
    }

    let inner = with_user(user);
    inner.host.sock_close(conn);
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_sock_connect(
    user: *mut c_void,
    conn: u32,
    addr: *const sys::mobile_addr,
) -> i32 {
    if user.is_null() {
        return -1;
    }

    let inner = with_user(user);
    let addr = decode_addr(addr);
    inner.host.sock_connect(conn, &addr)
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_sock_listen(user: *mut c_void, conn: u32) -> bool {
    if user.is_null() {
        return false;
    }

    let inner = with_user(user);
    inner.host.sock_listen(conn)
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_sock_accept(user: *mut c_void, conn: u32) -> bool {
    if user.is_null() {
        return false;
    }

    let inner = with_user(user);
    inner.host.sock_accept(conn)
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_sock_send(
    user: *mut c_void,
    conn: u32,
    data: *const c_void,
    size: u32,
    addr: *const sys::mobile_addr,
) -> i32 {
    if user.is_null() {
        return -1;
    }

    if data.is_null() {
        return -1;
    }

    let inner = with_user(user);
    let buf = core::slice::from_raw_parts(data as *const u8, size as usize);
    let addr_decoded = if addr.is_null() {
        None
    } else {
        Some(decode_addr(addr))
    };
    inner.host.sock_send(conn, buf, addr_decoded.as_ref())
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_sock_recv(
    user: *mut c_void,
    conn: u32,
    data: *mut c_void,
    size: u32,
    addr_out: *mut sys::mobile_addr,
) -> i32 {
    if user.is_null() {
        return -1;
    }

    let inner = with_user(user);

    let mut addr_tmp = MobileAddr::None;

    let recv_res = if data.is_null() {
        if addr_out.is_null() {
            inner.host.sock_recv(conn, None, None)
        } else {
            inner.host.sock_recv(conn, None, Some(&mut addr_tmp))
        }
    } else {
        let buf = core::slice::from_raw_parts_mut(data as *mut u8, size as usize);
        if addr_out.is_null() {
            inner.host.sock_recv(conn, Some(buf), None)
        } else {
            inner.host.sock_recv(conn, Some(buf), Some(&mut addr_tmp))
        }
    };

    if recv_res > 0 && !addr_out.is_null() {
        encode_addr(&mut *addr_out, &addr_tmp);
    }

    recv_res
}

#[cfg(any(feature = "bundled", feature = "system"))]
unsafe extern "C" fn cb_update_number(
    user: *mut c_void,
    number_type: sys::mobile_number,
    number: *const std::os::raw::c_char,
) {
    if user.is_null() {
        return;
    }

    let inner = with_user(user);
    let which = match number_type {
        sys::mobile_number::MOBILE_NUMBER_USER => MobileNumber::User,
        sys::mobile_number::MOBILE_NUMBER_PEER => MobileNumber::Peer,
    };

    if number.is_null() {
        inner.host.update_number(which, None);
        return;
    }

    let Ok(s) = CStr::from_ptr(number).to_str() else {
        return;
    };
    inner.host.update_number(which, Some(s));
}
