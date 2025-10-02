use std::collections::{HashSet, VecDeque};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};

use crate::debug::symbols::SymbolTable;
use crate::gameboy::GameBoy;

const SIGTRAP: u8 = 5;
const SIGINT: u8 = 2;

const TARGET_XML: &str = r#"<?xml version="1.0"?>
<!DOCTYPE target SYSTEM "gdb-target.dtd">
<target>
  <architecture>lr35902</architecture>
  <feature name="org.gnu.gdb.lr35902.core">
    <reg name="pc" bitsize="16" type="code_ptr"/>
    <reg name="sp" bitsize="16" type="data_ptr"/>
    <reg name="a" bitsize="8"/>
    <reg name="f" bitsize="8"/>
    <reg name="b" bitsize="8"/>
    <reg name="c" bitsize="8"/>
    <reg name="d" bitsize="8"/>
    <reg name="e" bitsize="8"/>
    <reg name="h" bitsize="8"/>
    <reg name="l" bitsize="8"/>
    <reg name="ime" bitsize="8"/>
  </feature>
</target>
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunState {
    Detached,
    Halted,
    Running,
    Step,
}

#[derive(Debug, Default)]
enum PacketState {
    #[default]
    Idle,
    Receiving {
        checksum: u8,
        data: Vec<u8>,
    },
    ChecksumFirst {
        checksum: u8,
        data: Vec<u8>,
    },
    ChecksumSecond {
        checksum: u8,
        data: Vec<u8>,
        first: u8,
    },
}

/// Implements a subset of the GDB remote serial protocol for debugging the
/// emulated ROM.
pub struct GdbServer {
    listener: TcpListener,
    client: Option<TcpStream>,
    state: RunState,
    packet_state: PacketState,
    breakpoints: HashSet<u16>,
    last_signal: u8,
    symbol_blob: Option<Vec<u8>>,
    pending_stop: bool,
    outgoing: VecDeque<Vec<u8>>,
}

impl GdbServer {
    pub fn new(port: u16, symbols: Option<SymbolTable>) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;

        let symbol_blob = symbols.map(|table| {
            let mut blob = Vec::new();
            for entry in table.iter() {
                let line = format!("{:02X}:{:04X} {}\n", entry.bank, entry.addr, entry.name);
                blob.extend_from_slice(line.as_bytes());
            }
            blob
        });

        Ok(Self {
            listener,
            client: None,
            state: RunState::Detached,
            packet_state: PacketState::Idle,
            breakpoints: HashSet::new(),
            last_signal: SIGTRAP,
            symbol_blob,
            pending_stop: false,
            outgoing: VecDeque::new(),
        })
    }

    fn accept_connection(&mut self) {
        if self.client.is_some() {
            return;
        }
        match self.listener.accept() {
            Ok((stream, addr)) => {
                let _ = stream.set_nonblocking(true);
                let _ = stream.set_nodelay(true);
                println!("GDB connected from {addr}");
                self.client = Some(stream);
                self.state = RunState::Halted;
                self.packet_state = PacketState::Idle;
                self.last_signal = SIGTRAP;
                self.pending_stop = true;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => {
                eprintln!("GDB listener error: {err}");
            }
        }
    }

    fn disconnect(&mut self) {
        if self.client.is_some() {
            println!("GDB disconnected");
        }
        self.client = None;
        self.state = RunState::Detached;
        self.packet_state = PacketState::Idle;
        self.breakpoints.clear();
        self.outgoing.clear();
    }

    fn write_all(&mut self, data: &[u8]) {
        if let Some(stream) = self.client.as_mut()
            && let Err(err) = stream.write_all(data)
            && err.kind() != io::ErrorKind::WouldBlock
        {
            eprintln!("GDB write error: {err}");
            self.disconnect();
        }
    }

    fn queue_packet(&mut self, payload: &[u8]) {
        if self.client.is_none() {
            return;
        }

        let mut packet = Vec::with_capacity(payload.len() + 4);
        packet.push(b'$');
        packet.extend_from_slice(payload);
        packet.push(b'#');
        let checksum = payload.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
        packet.push(HEX[usize::from((checksum >> 4) & 0x0F)]);
        packet.push(HEX[usize::from(checksum & 0x0F)]);
        self.outgoing.push_back(packet);
    }

    fn send_ack(&mut self, ack: u8) {
        if self.client.is_some() {
            self.write_all(&[ack]);
        }
    }

    fn handle_outgoing(&mut self) {
        while let Some(packet) = self.outgoing.pop_front() {
            self.write_all(&packet);
        }
    }

    fn process_incoming_byte(&mut self, byte: u8, gb: &mut GameBoy) {
        match &mut self.packet_state {
            PacketState::Idle => match byte {
                b'$' => {
                    self.packet_state = PacketState::Receiving {
                        checksum: 0,
                        data: Vec::new(),
                    };
                }
                0x03 => {
                    self.interrupt();
                }
                b'+' | b'-' => {
                    // ignore acknowledgements from GDB
                }
                _ => {}
            },
            PacketState::Receiving { checksum, data } => {
                if byte == b'#' {
                    let payload = std::mem::take(data);
                    self.packet_state = PacketState::ChecksumFirst {
                        checksum: *checksum,
                        data: payload,
                    };
                } else {
                    *checksum = checksum.wrapping_add(byte);
                    data.push(byte);
                }
            }
            PacketState::ChecksumFirst { checksum, data } => {
                if let Some(nibble) = from_hex(byte) {
                    let payload = std::mem::take(data);
                    self.packet_state = PacketState::ChecksumSecond {
                        checksum: *checksum,
                        data: payload,
                        first: nibble,
                    };
                } else {
                    self.packet_state = PacketState::Idle;
                    self.send_ack(b'-');
                }
            }
            PacketState::ChecksumSecond {
                checksum,
                data,
                first,
            } => {
                if let Some(second_nibble) = from_hex(byte) {
                    let packet_checksum = (*first << 4) | second_nibble;
                    let payload = std::mem::take(data);
                    if packet_checksum == *checksum {
                        self.send_ack(b'+');
                        self.handle_command(payload, gb);
                    } else {
                        self.send_ack(b'-');
                    }
                } else {
                    self.send_ack(b'-');
                }
                self.packet_state = PacketState::Idle;
            }
        }
    }

    fn handle_command(&mut self, data: Vec<u8>, gb: &mut GameBoy) {
        if data.is_empty() {
            return;
        }
        let cmd = data[0];
        match cmd {
            b'?' => self.queue_packet(format!("S{:02X}", self.last_signal).as_bytes()),
            b'g' => self.send_registers(gb),
            b'G' => self.write_registers(&data[1..], gb),
            b'p' => self.read_register(&data[1..], gb),
            b'P' => self.write_register(&data[1..], gb),
            b'm' => self.read_memory(&data[1..], gb),
            b'M' => self.write_memory(&data[1..], gb),
            b'c' => self.continue_exec(&data[1..], gb),
            b's' => self.step_exec(&data[1..], gb),
            b'Z' => self.set_breakpoint(&data[1..]),
            b'z' => self.clear_breakpoint(&data[1..]),
            b'H' => self.queue_packet(b"OK"),
            b'D' => {
                self.queue_packet(b"OK");
                self.disconnect();
            }
            b'k' => {
                self.disconnect();
            }
            b'q' => self.handle_query(&data[1..]),
            b'v' => self.handle_v_packet(&data[1..]),
            b'Q' => self.queue_packet(b""),
            _ => self.queue_packet(b""),
        }
    }

    fn interrupt(&mut self) {
        if matches!(self.state, RunState::Running | RunState::Step) {
            self.state = RunState::Halted;
            self.last_signal = SIGINT;
            self.queue_packet(b"S02");
        }
    }

    fn send_registers(&mut self, gb: &GameBoy) {
        let mut data = Vec::with_capacity(22);
        data.extend_from_slice(&gb.cpu.pc.to_le_bytes());
        data.extend_from_slice(&gb.cpu.sp.to_le_bytes());
        data.push(gb.cpu.a);
        data.push(gb.cpu.f);
        data.push(gb.cpu.b);
        data.push(gb.cpu.c);
        data.push(gb.cpu.d);
        data.push(gb.cpu.e);
        data.push(gb.cpu.h);
        data.push(gb.cpu.l);
        data.push(if gb.cpu.ime { 1 } else { 0 });
        self.queue_packet(hex_encode(&data).as_bytes());
    }

    fn write_registers(&mut self, payload: &[u8], gb: &mut GameBoy) {
        if !payload.len().is_multiple_of(2) {
            self.queue_packet(b"E01");
            return;
        }
        let bytes = match hex_decode(payload) {
            Some(v) => v,
            None => {
                self.queue_packet(b"E01");
                return;
            }
        };
        if bytes.len() < 14 {
            self.queue_packet(b"E01");
            return;
        }
        gb.cpu.pc = u16::from_le_bytes([bytes[0], bytes[1]]);
        gb.cpu.sp = u16::from_le_bytes([bytes[2], bytes[3]]);
        gb.cpu.a = bytes[4];
        gb.cpu.f = bytes[5];
        gb.cpu.b = bytes[6];
        gb.cpu.c = bytes[7];
        gb.cpu.d = bytes[8];
        gb.cpu.e = bytes[9];
        gb.cpu.h = bytes[10];
        gb.cpu.l = bytes[11];
        gb.cpu.ime = bytes[12] & 1 != 0;
        self.queue_packet(b"OK");
    }

    fn read_register(&mut self, payload: &[u8], gb: &GameBoy) {
        let reg_idx = match parse_hex_u32(payload) {
            Some(idx) => idx,
            None => {
                self.queue_packet(b"E01");
                return;
            }
        } as usize;
        let value = match reg_idx {
            0 => gb.cpu.pc.to_le_bytes().to_vec(),
            1 => gb.cpu.sp.to_le_bytes().to_vec(),
            2 => vec![gb.cpu.a],
            3 => vec![gb.cpu.f],
            4 => vec![gb.cpu.b],
            5 => vec![gb.cpu.c],
            6 => vec![gb.cpu.d],
            7 => vec![gb.cpu.e],
            8 => vec![gb.cpu.h],
            9 => vec![gb.cpu.l],
            10 => vec![if gb.cpu.ime { 1 } else { 0 }],
            _ => {
                self.queue_packet(b"E00");
                return;
            }
        };
        self.queue_packet(hex_encode(&value).as_bytes());
    }

    fn write_register(&mut self, payload: &[u8], gb: &mut GameBoy) {
        let Some((reg_bytes, value_bytes)) = split_once_byte(payload, b'=') else {
            self.queue_packet(b"E01");
            return;
        };
        let reg_idx = match parse_hex_u32(reg_bytes) {
            Some(idx) => idx as usize,
            None => {
                self.queue_packet(b"E01");
                return;
            }
        };
        let value = match hex_decode(value_bytes) {
            Some(v) => v,
            None => {
                self.queue_packet(b"E01");
                return;
            }
        };
        let mut iter = value.into_iter();
        let result = match reg_idx {
            0 => iter
                .next()
                .zip(iter.next())
                .map(|(lo, hi)| gb.cpu.pc = u16::from_le_bytes([lo, hi])),
            1 => iter
                .next()
                .zip(iter.next())
                .map(|(lo, hi)| gb.cpu.sp = u16::from_le_bytes([lo, hi])),
            2 => iter.next().map(|v| gb.cpu.a = v),
            3 => iter.next().map(|v| gb.cpu.f = v),
            4 => iter.next().map(|v| gb.cpu.b = v),
            5 => iter.next().map(|v| gb.cpu.c = v),
            6 => iter.next().map(|v| gb.cpu.d = v),
            7 => iter.next().map(|v| gb.cpu.e = v),
            8 => iter.next().map(|v| gb.cpu.h = v),
            9 => iter.next().map(|v| gb.cpu.l = v),
            10 => iter.next().map(|v| gb.cpu.ime = v & 1 != 0),
            _ => None,
        };
        if result.is_some() {
            self.queue_packet(b"OK");
        } else {
            self.queue_packet(b"E02");
        }
    }

    fn read_memory(&mut self, payload: &[u8], gb: &mut GameBoy) {
        let Some((addr_bytes, len_bytes)) = split_once_byte(payload, b',') else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(addr) = parse_hex_u16(addr_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(len) = parse_hex_u32(len_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        let len = len.min(1024) as usize;
        let mut data = Vec::with_capacity(len);
        for offset in 0..len {
            data.push(gb.mmu.read_byte(addr.wrapping_add(offset as u16)));
        }
        self.queue_packet(hex_encode(&data).as_bytes());
    }

    fn write_memory(&mut self, payload: &[u8], gb: &mut GameBoy) {
        let Some((addr_len, data_bytes)) = split_once_byte(payload, b':') else {
            self.queue_packet(b"E01");
            return;
        };
        let Some((addr_bytes, len_bytes)) = split_once_byte(addr_len, b',') else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(addr) = parse_hex_u16(addr_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(len) = parse_hex_u32(len_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(bytes) = hex_decode(data_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        if bytes.len() != len as usize {
            self.queue_packet(b"E02");
            return;
        }
        for (offset, value) in bytes.into_iter().enumerate() {
            gb.mmu.write_byte(addr.wrapping_add(offset as u16), value);
        }
        self.queue_packet(b"OK");
    }

    fn continue_exec(&mut self, payload: &[u8], gb: &mut GameBoy) {
        if !payload.is_empty()
            && let Some(addr) = parse_hex_u16(payload)
        {
            gb.cpu.pc = addr;
        }
        self.last_signal = SIGTRAP;
        self.state = RunState::Running;
        self.pending_stop = false;
    }

    fn step_exec(&mut self, payload: &[u8], gb: &mut GameBoy) {
        if !payload.is_empty()
            && let Some(addr) = parse_hex_u16(payload)
        {
            gb.cpu.pc = addr;
        }
        self.last_signal = SIGTRAP;
        self.state = RunState::Step;
        self.pending_stop = false;
    }

    fn set_breakpoint(&mut self, payload: &[u8]) {
        if payload.first() != Some(&b'0') {
            self.queue_packet(b"E01");
            return;
        }
        let rest = &payload[1..];
        let Some((addr_bytes, len_bytes)) = split_once_byte(rest, b',') else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(addr) = parse_hex_u16(addr_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(len) = parse_hex_u32(len_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        if len == 0 {
            self.queue_packet(b"E02");
            return;
        }
        self.breakpoints.insert(addr);
        self.queue_packet(b"OK");
    }

    fn clear_breakpoint(&mut self, payload: &[u8]) {
        if payload.first() != Some(&b'0') {
            self.queue_packet(b"E01");
            return;
        }
        let rest = &payload[1..];
        let Some((addr_bytes, _len_bytes)) = split_once_byte(rest, b',') else {
            self.queue_packet(b"E01");
            return;
        };
        let Some(addr) = parse_hex_u16(addr_bytes) else {
            self.queue_packet(b"E01");
            return;
        };
        if self.breakpoints.remove(&addr) {
            self.queue_packet(b"OK");
        } else {
            self.queue_packet(b"E02");
        }
    }

    fn handle_query(&mut self, payload: &[u8]) {
        let s = String::from_utf8_lossy(payload);
        if s.starts_with("Supported") {
            self.queue_packet(b"PacketSize=4000;swbreak+;qXfer:features:read+");
        } else if s.starts_with("TStatus") {
            self.queue_packet(b"T0");
        } else if s.starts_with("Attached") {
            self.queue_packet(b"1");
        } else if s.starts_with("Offsets") {
            self.queue_packet(b"Text=0;Data=0;Bss=0");
        } else if s.starts_with("fThreadInfo") {
            self.queue_packet(b"m1");
        } else if s.starts_with("sThreadInfo") {
            self.queue_packet(b"l");
        } else if s.starts_with("C") {
            self.queue_packet(b"QC1");
        } else if s.starts_with("Xfer:features:read:target.xml:") {
            self.handle_target_xml(&s);
        } else if s.starts_with("Xfer:symbols:read") {
            self.handle_symbol_read(&s);
        } else if s.starts_with("HostInfo") {
            self.queue_packet(b"triple:gb-unknown-none;endian=little");
        } else if s.starts_with("Rcmd,") {
            self.queue_packet(b"OK");
        } else {
            self.queue_packet(b"");
        }
    }

    fn handle_v_packet(&mut self, payload: &[u8]) {
        let s = String::from_utf8_lossy(payload);
        if s.starts_with("Cont?") {
            self.queue_packet(b"vCont;c;s");
        } else if s.starts_with("Cont;c") {
            self.queue_packet(b"");
            self.state = RunState::Running;
            self.pending_stop = false;
        } else if s.starts_with("Cont;s") {
            self.queue_packet(b"");
            self.state = RunState::Step;
            self.pending_stop = false;
        } else {
            self.queue_packet(b"");
        }
    }

    fn handle_target_xml(&mut self, query: &str) {
        if let Some((_, rest)) = query.split_once(':')
            && let Some((offset_len, _)) = rest.split_once(':')
            && let Some((offset_str, len_str)) = offset_len.split_once(',')
            && let (Some(offset), Some(length)) = (
                parse_hex_u32(offset_str.as_bytes()),
                parse_hex_u32(len_str.as_bytes()),
            )
        {
            let offset = offset as usize;
            let length = length as usize;
            let bytes = TARGET_XML.as_bytes();
            if offset >= bytes.len() {
                self.queue_packet(b"l");
            } else {
                let end = (offset + length).min(bytes.len());
                let chunk = &bytes[offset..end];
                let mut response = Vec::with_capacity(chunk.len() + 1);
                response.push(if end < bytes.len() { b'm' } else { b'l' });
                response.extend_from_slice(chunk);
                self.queue_packet(&response);
            }
            return;
        }
        self.queue_packet(b"E01");
    }

    fn handle_symbol_read(&mut self, query: &str) {
        let Some(blob) = self.symbol_blob.as_ref() else {
            self.queue_packet(b"l");
            return;
        };
        if let Some((_, rest)) = query.split_once(':')
            && let Some((offset_len, _)) = rest.split_once(':')
            && let Some((offset_str, len_str)) = offset_len.split_once(',')
            && let (Some(offset), Some(length)) = (
                parse_hex_u32(offset_str.as_bytes()),
                parse_hex_u32(len_str.as_bytes()),
            )
        {
            let offset = offset as usize;
            let length = length as usize;
            if offset >= blob.len() {
                self.queue_packet(b"l");
                return;
            }
            let end = (offset + length).min(blob.len());
            let mut response = Vec::with_capacity(end - offset + 1);
            response.push(if end < blob.len() { b'm' } else { b'l' });
            response.extend_from_slice(&blob[offset..end]);
            self.queue_packet(&response);
            return;
        }
        self.queue_packet(b"E01");
    }

    pub fn poll(&mut self, gb: &mut GameBoy) {
        self.accept_connection();
        self.handle_outgoing();

        while self.client.is_some() {
            let mut buf = [0u8; 256];
            let result = {
                let stream = self.client.as_mut().unwrap();
                stream.read(&mut buf)
            };
            match result {
                Ok(0) => {
                    self.disconnect();
                    break;
                }
                Ok(n) => {
                    for &byte in &buf[..n] {
                        self.process_incoming_byte(byte, gb);
                        if self.client.is_none() {
                            break;
                        }
                    }
                    if self.client.is_none() {
                        break;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => {
                    eprintln!("GDB read error: {err}");
                    self.disconnect();
                    break;
                }
            }
        }

        if self.client.is_none() {
            self.state = RunState::Detached;
        }

        if self.pending_stop {
            self.queue_packet(format!("S{:02X}", self.last_signal).as_bytes());
            self.pending_stop = false;
        }

        self.handle_outgoing();
    }

    pub fn before_step(&mut self, gb: &GameBoy) -> bool {
        match self.state {
            RunState::Detached => false,
            RunState::Halted => true,
            RunState::Running => {
                if self.breakpoints.contains(&gb.cpu.pc) {
                    self.state = RunState::Halted;
                    self.last_signal = SIGTRAP;
                    self.queue_packet(b"S05");
                    self.handle_outgoing();
                    true
                } else {
                    false
                }
            }
            RunState::Step => false,
        }
    }

    pub fn after_step(&mut self) {
        if matches!(self.state, RunState::Step) {
            self.state = RunState::Halted;
            self.last_signal = SIGTRAP;
            self.queue_packet(b"S05");
        }
    }

    pub fn is_executing(&self) -> bool {
        matches!(self.state, RunState::Running | RunState::Step)
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(char::from(HEX[usize::from((b >> 4) & 0x0F)]));
        s.push(char::from(HEX[usize::from(b & 0x0F)]));
    }
    s
}

fn hex_decode(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut iter = bytes.chunks_exact(2);
    for chunk in iter.by_ref() {
        let hi = from_hex(chunk[0])?;
        let lo = from_hex(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn parse_hex_u16(bytes: &[u8]) -> Option<u16> {
    let mut value = 0u16;
    for &b in bytes {
        let nib = from_hex(b)? as u16;
        value = value.wrapping_mul(16).wrapping_add(nib);
    }
    Some(value)
}

fn parse_hex_u32(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    for &b in bytes {
        let nib = from_hex(b)? as u32;
        value = value.wrapping_mul(16).wrapping_add(nib);
    }
    Some(value)
}

fn split_once_byte(slice: &[u8], needle: u8) -> Option<(&[u8], &[u8])> {
    slice
        .iter()
        .position(|&b| b == needle)
        .map(|idx| (&slice[..idx], &slice[idx + 1..]))
}
