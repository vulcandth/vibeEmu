//! Portable, atomic slot storage shared by desktop and Android frontends.
use crate::{
    gameboy::GameBoy,
    save_state::{self, MAX_STATE_BYTES, Metadata},
};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

/// Ten numbered slots plus quick-save and pre-load recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// User slot, numbered 1 through 10.
    Number(u8),
    /// Dedicated quick-save slot.
    Quick,
    /// Previous machine, written before every successful load.
    Recovery,
}

impl Slot {
    /// Display order of the twelve slots.
    pub const ALL: [Self; 12] = [
        Self::Number(1),
        Self::Number(2),
        Self::Number(3),
        Self::Number(4),
        Self::Number(5),
        Self::Number(6),
        Self::Number(7),
        Self::Number(8),
        Self::Number(9),
        Self::Number(10),
        Self::Quick,
        Self::Recovery,
    ];

    /// User-facing slot name.
    pub fn label(self) -> String {
        match self {
            Self::Number(n) => format!("Slot {n}"),
            Self::Quick => "Quick".into(),
            Self::Recovery => "Recovery / undo load".into(),
        }
    }
    fn file_name(self) -> io::Result<String> {
        match self {
            Self::Number(n @ 1..=10) => Ok(format!("slot-{n}.vstate")),
            Self::Quick => Ok("quick.vstate".into()),
            Self::Recovery => Ok("recovery.vstate".into()),
            _ => Err(save_state::invalid("Slot must be between 1 and 10")),
        }
    }
}

/// A state directory scoped by SHA-256 ROM identity, never by an imported path.
pub struct StateStore {
    directory: PathBuf,
}

impl StateStore {
    /// Resolve this ROM beneath a frontend-owned state root.
    pub fn new(root: &Path, gb: &GameBoy) -> io::Result<Self> {
        Ok(Self {
            directory: root.join(save_state::rom_identity(gb)?),
        })
    }

    /// Resolve a validated slot filename.
    pub fn path(&self, slot: Slot) -> io::Result<PathBuf> {
        Ok(self.directory.join(slot.file_name()?))
    }

    /// Read slot metadata; missing slots return None.
    pub fn metadata(&self, slot: Slot) -> io::Result<Option<Metadata>> {
        match read(&self.path(slot)?) {
            Ok(bytes) => save_state::metadata(&bytes).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Atomically save a numbered or quick slot.
    pub fn save(&self, gb: &GameBoy, slot: Slot) -> io::Result<()> {
        if slot == Slot::Recovery {
            return Err(save_state::invalid(
                "Recovery is written only before a load",
            ));
        }
        write_atomic(&self.path(slot)?, &gb.save_state()?)
    }

    /// Load a slot after persisting the pre-load machine in Recovery.
    pub fn load(&self, gb: &mut GameBoy, slot: Slot) -> io::Result<()> {
        self.load_bytes(gb, &read(&self.path(slot)?)?)
    }

    /// A failed decode or recovery write leaves both machine and SRAM unchanged.
    /// Loading Recovery swaps it with the current state, permitting repeated undo.
    pub fn load_bytes(&self, gb: &mut GameBoy, bytes: &[u8]) -> io::Result<()> {
        let candidate = gb.prepare_state(bytes)?;
        let recovery = gb.save_state()?;
        write_atomic(&self.path(Slot::Recovery)?, &recovery)?;
        gb.install_state(candidate);
        Ok(())
    }

    /// Load an external state with the same validation and recovery guarantees.
    pub fn import(&self, gb: &mut GameBoy, path: &Path) -> io::Result<()> {
        self.load_bytes(gb, &read(path)?)
    }

    /// Export the current machine to an atomic state file.
    pub fn export(&self, gb: &GameBoy, path: &Path) -> io::Result<()> {
        write_atomic(path, &gb.save_state()?)
    }
}

/// Bounded reading also handles files that grow while being read.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_STATE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(save_state::invalid("Save state exceeds size limit"));
    }
    Ok(bytes)
}

/// Same-directory temporary file, flush, and atomic replacement. The old state
/// survives failed writes; temporary files are removed on ordinary error paths.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
