//! Save-state format v1. Host paths, debugger settings and external devices are
//! never read from a snapshot. Decode and validate a candidate before replacing
//! a running machine; cartridge RAM is always part of the restored state.

use crate::{gameboy::GameBoy, hardware::Model};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io;

const MAGIC: &[u8; 8] = b"VIBESTAT";
/// Current on-disk format version.
pub const VERSION: u32 = 1;
/// Upper bound for imported files, including JSON payload and header.
pub const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;
const HEADER_LEN: usize = 44;

/// Portable metadata shown by state browsers. Timestamps are UTC Unix seconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    /// State format version.
    pub version: u32,
    /// Save time as UTC Unix seconds.
    pub created_unix: u64,
    /// Cartridge header title.
    pub title: String,
    /// SHA-256 identity of all ROM bytes.
    pub rom_sha256: String,
    /// Hardware model and revision.
    pub model: Model,
    /// CPU cycles at capture.
    pub cycles: u64,
}

#[derive(Serialize, Deserialize)]
struct Snapshot<T> {
    metadata: Metadata,
    boot_sha256: Option<String>,
    machine: T,
}

pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Content identity of the currently loaded ROM (including debugger patches).
pub fn rom_identity(gb: &GameBoy) -> io::Result<String> {
    let cart = gb
        .mmu
        .cart
        .as_ref()
        .ok_or_else(|| invalid("No ROM loaded"))?;
    Ok(digest(&cart.rom))
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn payload(bytes: &[u8]) -> io::Result<&[u8]> {
    if bytes.len() < HEADER_LEN || bytes.len() > MAX_STATE_BYTES || &bytes[..8] != MAGIC {
        return Err(invalid("Not a vibeEmu save state, or file size is invalid"));
    }
    if bytes[8..12] != VERSION.to_le_bytes() {
        return Err(invalid("Unsupported save-state version"));
    }
    let payload = &bytes[HEADER_LEN..];
    if Sha256::digest(payload)[..] != bytes[12..HEADER_LEN] {
        return Err(invalid("Save-state checksum failed"));
    }
    Ok(payload)
}

/// Inspect metadata without restoring the machine.
pub fn metadata(bytes: &[u8]) -> io::Result<Metadata> {
    #[derive(Deserialize)]
    struct Header {
        metadata: Metadata,
    }
    let header: Header = serde_json::from_slice(payload(bytes)?).map_err(io::Error::other)?;
    Ok(header.metadata)
}

impl GameBoy {
    /// Capture the entire machine at an instruction boundary, including SRAM,
    /// RTC fractions, pending hardware events, and SGB/hybrid border state.
    pub fn save_state(&self) -> io::Result<Vec<u8>> {
        if !self.mmu.serial.state_supported() {
            return Err(invalid(
                "Disconnect link cable / Mobile Adapter before using save states",
            ));
        }
        let cart = self
            .mmu
            .cart
            .as_ref()
            .ok_or_else(|| invalid("No ROM loaded"))?;
        let snapshot = Snapshot {
            metadata: Metadata {
                version: VERSION,
                created_unix: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                title: cart.title.clone(),
                rom_sha256: rom_identity(self)?,
                model: self.model,
                cycles: self.cpu.cycles,
            },
            boot_sha256: self.mmu.boot_rom.as_deref().map(digest),
            machine: self,
        };
        let payload = serde_json::to_vec(&snapshot).map_err(io::Error::other)?;
        if payload.len() > MAX_STATE_BYTES - HEADER_LEN {
            return Err(invalid("Save state exceeds size limit"));
        }
        let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(&payload));
        bytes.extend_from_slice(&payload);
        Ok(bytes)
    }

    /// Decode and check a state without mutating this machine or its battery file.
    /// The returned candidate has no host resources or persistence paths.
    pub fn prepare_state(&self, bytes: &[u8]) -> io::Result<Box<GameBoy>> {
        if !self.mmu.serial.state_supported() {
            return Err(invalid(
                "Disconnect link cable / Mobile Adapter before loading a state",
            ));
        }
        // Serde constructs the large fixed PPU/SGB arrays by value. Use a
        // bounded worker stack rather than the caller's UI/JNI/default stack.
        let data = payload(bytes)?.to_vec();
        let snapshot: Snapshot<Box<GameBoy>> = std::thread::Builder::new()
            .name("state-decoder".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || serde_json::from_slice(&data).map_err(io::Error::other))?
            .join()
            .map_err(|_| invalid("State decoder failed"))??;
        let mut next = snapshot.machine;
        if snapshot.metadata.version != VERSION
            || snapshot.metadata.rom_sha256 != rom_identity(self)?
        {
            return Err(invalid(
                "Save state belongs to a different ROM or format version",
            ));
        }
        if snapshot.metadata.model != self.model
            || next.model != self.model
            || snapshot.boot_sha256 != self.mmu.boot_rom.as_deref().map(digest)
            || next.mmu.ppu.sgb.as_ref().map(|s| s.is_command_host())
                != self.mmu.ppu.sgb.as_ref().map(|s| s.is_command_host())
        {
            return Err(invalid(
                "Save state uses a different hardware model, boot ROM or SGB mode",
            ));
        }
        let current = self
            .mmu
            .cart
            .as_ref()
            .ok_or_else(|| invalid("No ROM loaded"))?;
        let cart = next
            .mmu
            .cart
            .as_mut()
            .ok_or_else(|| invalid("State has no cartridge"))?;
        if !cart.validate_state(current)
            || !next.cpu.validate_state()
            || !next.mmu.validate_state(self.model)
        {
            return Err(invalid("Save state contains invalid machine data"));
        }
        next.mmu.cart.as_mut().unwrap().rom = current.rom.clone();
        next.mmu.boot_rom = self.mmu.boot_rom.clone();
        Ok(next)
    }

    /// Restore a checked state atomically in memory. Callers needing durable
    /// undo should persist `save_state()` before calling this method.
    pub fn load_state(&mut self, bytes: &[u8]) -> io::Result<()> {
        let next = self.prepare_state(bytes)?;
        self.install_state(next);
        Ok(())
    }

    pub(crate) fn install_state(&mut self, mut next: Box<GameBoy>) {
        next.mmu
            .cart
            .as_mut()
            .unwrap()
            .inherit_save_paths(self.mmu.cart.as_ref().unwrap());
        next.mmu
            .apu
            .restore_output_state(self.mmu.apu.take_output_state());
        next.mmu.watchpoints = std::mem::take(&mut self.mmu.watchpoints);
        next.mmu.watchpoints.clear_hit();
        *self = *next;
    }
}

// Serde's built-in array support stops at 32. Deserialize through bounded
// visitors rather than trusting a length prefix or allocating an arbitrary Vec.
pub(crate) mod arrays {
    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{Error, SeqAccess, Visitor},
    };
    use std::{fmt, marker::PhantomData};
    pub fn serialize<T: Serialize, S: Serializer, const N: usize>(
        value: &[T; N],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_slice().serialize(s)
    }
    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>, const N: usize>(
        d: D,
    ) -> Result<[T; N], D::Error> {
        struct Array<T, const N: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Array<T, N> {
            type Value = [T; N];
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "an array of {N} elements")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::with_capacity(N);
                for i in 0..N {
                    values.push(
                        seq.next_element()?
                            .ok_or_else(|| A::Error::invalid_length(i, &self))?,
                    );
                }
                if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(A::Error::invalid_length(N + 1, &self));
                }
                values
                    .try_into()
                    .map_err(|_| A::Error::custom("array length"))
            }
        }
        d.deserialize_seq(Array::<T, N>(PhantomData))
    }
}

pub(crate) mod arrays2 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    #[derive(Serialize, Deserialize)]
    #[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
    struct Row<T, const N: usize>(#[serde(with = "super::arrays")] [T; N]);
    pub fn serialize<T: Serialize, S: Serializer, const N: usize, const M: usize>(
        value: &[[T; N]; M],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(Some(M))?;
        for row in value {
            seq.serialize_element(row.as_slice())?;
        }
        seq.end()
    }
    pub fn deserialize<
        'de,
        T: Deserialize<'de>,
        D: Deserializer<'de>,
        const N: usize,
        const M: usize,
    >(
        d: D,
    ) -> Result<[[T; N]; M], D::Error> {
        let rows: [Row<T, N>; M] = super::arrays::deserialize(d)?;
        Ok(rows.map(|r| r.0))
    }
}
