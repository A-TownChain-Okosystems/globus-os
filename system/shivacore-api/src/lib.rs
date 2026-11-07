//! Stable, versioned userspace ABI exposed by ShivaCore.
//!
//! This crate contains only wire-safe identifiers and request/response types.
//! Kernel implementations remain private to ShivaCore.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod acpi_devices;
pub mod madt;

pub use globus_ipc::{Endpoint, Message, MessageHeader};

/// ABI 2.0.0: Der Syscall-Aufruf uebergibt keine losen Registerargumente mehr,
/// sondern einen versionierten Argumentblock im Userspace-Speicher.
///
/// Warum eine Major-Version: In 1.x lagen Opcode, Cap-Slot, vier Argumentwoerter
/// UND die Nutzlaenge in sechs Registern — `R8` war gleichzeitig Argumentwort
/// und Laengenfeld, ein Pufferzeiger hatte keinen Platz. Ein solcher Aufruf
/// kann nicht wachsen. Der Block traegt seine eigene Groesse, der Kernel liest
/// nur die Felder, die er kennt.
pub const ABI_MAJOR: u16 = 2;
pub const ABI_MINOR: u16 = 0;
pub const ABI_VERSION: u32 = ((ABI_MAJOR as u32) << 16) | ABI_MINOR as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Yield = 0x0001,
    IpcSend = 0x0010,
    IpcReceive = 0x0011,
    CapabilityQuery = 0x0020,
    HandleClose = 0x0030,
    MonotonicTime = 0x0040,
}

impl Syscall {
    pub const fn id(self) -> u16 {
        self as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CapabilityHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ObjectHandle(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityRight {
    Read,
    Write,
    Execute,
    Grant,
    Revoke,
    Inspect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AbiError {
    InvalidSyscall = 1,
    InvalidHandle = 2,
    PermissionDenied = 3,
    InvalidPayload = 4,
    EndpointUnavailable = 5,
    ResourceExhausted = 6,
    AbiVersionMismatch = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiHandshake {
    pub major: u16,
    pub minor: u16,
}

impl AbiHandshake {
    pub const CURRENT: Self = Self {
        major: ABI_MAJOR,
        minor: ABI_MINOR,
    };

    pub const fn compatible(self) -> bool {
        self.major == ABI_MAJOR
    }
}

/// Kleinste Blockgroesse, die ABI 2.0.0 versteht: sieben 64-Bit-Felder.
pub const SYSCALL_BLOCK_MIN_SIZE: usize = 56;

/// Groesster Block, den der Kernel liest. Begrenzt die Kopie aus dem
/// Userspace-Speicher, bevor irgendetwas interpretiert wird.
pub const SYSCALL_BLOCK_MAX_SIZE: usize = 256;

/// Flags im Opcode-Feld. Unbekannte Bits werden abgelehnt: ein Aufruf darf
/// nicht stillschweigend anders gemeint sein, als der Aufrufer denkt.
pub const SYSCALL_FLAG_NONE: u16 = 0;

/// Aufbau des Argumentblocks (alles little endian, 64 Bit):
///
/// ```text
/// Offset  0: size        Gesamtgroesse des Blocks, mindestens 56
/// Offset  8: opcode u16 | flags u16 | reserviert u32
/// Offset 16: capability  Cap-Handle, 0 = keine
/// Offset 24: arg0
/// Offset 32: arg1
/// Offset 40: buffer_ptr  Zeiger auf Nutzdaten im Userspace, 0 = keine
/// Offset 48: buffer_len  Laenge der Nutzdaten
/// ```
///
/// Erweiterbarkeit: Ein spaeterer Minor-Stand haengt Felder ab Offset 56 an und
/// erhoeht `size`. Der Kernel liest `min(size, was er kennt)` und ignoriert den
/// Rest; ein alter Kernel akzeptiert einen neuen Block also weiterhin. Ein
/// neuer Kernel akzeptiert alte Bloecke, weil `size` klein bleiben darf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallBlock {
    pub size: usize,
    pub opcode: Syscall,
    pub flags: u16,
    pub capability: Option<CapabilityHandle>,
    pub arg0: u64,
    pub arg1: u64,
    pub buffer_ptr: u64,
    pub buffer_len: usize,
}

impl SyscallBlock {
    /// Baut einen Block ohne Nutzdaten.
    pub const fn new(opcode: Syscall, capability: Option<CapabilityHandle>) -> Self {
        Self {
            size: SYSCALL_BLOCK_MIN_SIZE,
            opcode,
            flags: SYSCALL_FLAG_NONE,
            capability,
            arg0: 0,
            arg1: 0,
            buffer_ptr: 0,
            buffer_len: 0,
        }
    }

    /// Liest einen Block aus rohen Bytes. Geprueft wird die Struktur, nicht die
    /// Absicht: Groesse, Opcode, Flags und Laengen. Die Rechtepruefung bleibt
    /// beim Dispatcher.
    pub fn parse(bytes: &[u8]) -> Result<Self, AbiError> {
        if bytes.len() < 8 || bytes.len() > SYSCALL_BLOCK_MAX_SIZE {
            return Err(AbiError::InvalidPayload);
        }
        let size = u64::from_le_bytes(read8(bytes, 0)?);
        if size < SYSCALL_BLOCK_MIN_SIZE as u64
            || size > bytes.len() as u64
            || size > SYSCALL_BLOCK_MAX_SIZE as u64
        {
            return Err(AbiError::InvalidPayload);
        }
        let word = u64::from_le_bytes(read8(bytes, 8)?);
        let opcode_id = (word & 0xFFFF) as u16;
        let flags = ((word >> 16) & 0xFFFF) as u16;
        let opcode = match opcode_id {
            x if x == Syscall::Yield.id() => Syscall::Yield,
            x if x == Syscall::IpcSend.id() => Syscall::IpcSend,
            x if x == Syscall::IpcReceive.id() => Syscall::IpcReceive,
            x if x == Syscall::CapabilityQuery.id() => Syscall::CapabilityQuery,
            x if x == Syscall::HandleClose.id() => Syscall::HandleClose,
            x if x == Syscall::MonotonicTime.id() => Syscall::MonotonicTime,
            _ => return Err(AbiError::InvalidSyscall),
        };
        if flags != SYSCALL_FLAG_NONE {
            return Err(AbiError::InvalidPayload);
        }
        let capability = match u64::from_le_bytes(read8(bytes, 16)?) {
            0 => None,
            handle => Some(CapabilityHandle(handle)),
        };
        let arg0 = u64::from_le_bytes(read8(bytes, 24)?);
        let arg1 = u64::from_le_bytes(read8(bytes, 32)?);
        let buffer_ptr = u64::from_le_bytes(read8(bytes, 40)?);
        let buffer_len = u64::from_le_bytes(read8(bytes, 48)?);
        if buffer_len > MAX_SYSCALL_PAYLOAD as u64 {
            return Err(AbiError::InvalidPayload);
        }
        if buffer_len > 0 && buffer_ptr == 0 {
            return Err(AbiError::InvalidPayload);
        }
        Ok(Self {
            size: size as usize,
            opcode,
            flags,
            capability,
            arg0,
            arg1,
            buffer_ptr,
            buffer_len: buffer_len as usize,
        })
    }

    /// Schreibt den Block so, wie ihn der Kernel erwartet. Wird vom
    /// Userspace-Compiler und von Tests genutzt.
    pub fn write_to(&self, out: &mut [u8]) -> Result<usize, AbiError> {
        let size = self.size.max(SYSCALL_BLOCK_MIN_SIZE);
        if size > out.len() || size > SYSCALL_BLOCK_MAX_SIZE {
            return Err(AbiError::InvalidPayload);
        }
        out[..size].fill(0);
        out[0..8].copy_from_slice(&(size as u64).to_le_bytes());
        let word = u64::from(self.opcode.id()) | (u64::from(self.flags) << 16);
        out[8..16].copy_from_slice(&word.to_le_bytes());
        let cap = self.capability.map(|c| c.0).unwrap_or(0);
        out[16..24].copy_from_slice(&cap.to_le_bytes());
        out[24..32].copy_from_slice(&self.arg0.to_le_bytes());
        out[32..40].copy_from_slice(&self.arg1.to_le_bytes());
        out[40..48].copy_from_slice(&self.buffer_ptr.to_le_bytes());
        out[48..56].copy_from_slice(&(self.buffer_len as u64).to_le_bytes());
        Ok(size)
    }
}

const fn read8(bytes: &[u8], offset: usize) -> Result<[u8; 8], AbiError> {
    if offset + 8 > bytes.len() {
        return Err(AbiError::InvalidPayload);
    }
    let mut out = [0u8; 8];
    let mut i = 0;
    while i < 8 {
        out[i] = bytes[offset + i];
        i += 1;
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcSendRequest {
    pub endpoint: Endpoint,
    pub capability: CapabilityHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcReceiveRequest {
    pub endpoint: Endpoint,
    pub capability: CapabilityHandle,
    pub max_payload: u32,
}

/// Stable 32-byte digest exchanged across the kernel/service boundary.
pub type Hash32 = [u8; 32];

/// Deterministic compatibility hash used by existing Service-Space records.
///
/// This is deliberately kept as a contract primitive so callers do not import
/// the kernel security implementation. It is not a cryptographic hash.
pub fn simple_hash(data: &[u8]) -> Hash32 {
    let mut hash: u32 = 0x811c9dc5;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    let mut result = [0u8; 32];
    result[..4].copy_from_slice(&hash.to_be_bytes());
    for i in (4..32).step_by(4) {
        hash ^= result[i - 4] as u32;
        hash = hash.wrapping_mul(0x01000193);
        result[i..i + 4].copy_from_slice(&hash.to_be_bytes());
    }
    result
}

/// Public capability rights representation. Kernel storage remains private.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CapabilityRights(pub u8);

impl CapabilityRights {
    pub const NONE: Self = Self(0);
    pub const READ: Self = Self(1);
    pub const WRITE: Self = Self(2);
    pub const EXECUTE: Self = Self(4);
    pub const DELEGATE: Self = Self(8);
    pub const INSPECT: Self = Self(16);

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// Stable boot phases visible to services without exposing KernelState.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KernelBootPhase {
    Heap = 0,
    Capabilities = 1,
    Processes = 2,
    Scheduler = 3,
    Ipc = 4,
    FileSystem = 5,
    Security = 6,
    Ready = 7,
}

pub const MAX_SYSCALL_PAYLOAD: usize = 1024 * 1024;

pub const fn validate_abi_version(version: u32) -> bool {
    (version >> 16) as u16 == ABI_MAJOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_version_is_stable() {
        assert_eq!(ABI_VERSION, 0x0002_0000);
        // Minor bleibt kompatibel, Major nicht.
        assert!(validate_abi_version(0x0002_ffff));
        assert!(!validate_abi_version(0x0001_0000));
        assert!(!validate_abi_version(0x0003_0000));
    }

    #[test]
    fn block_round_trips_through_bytes() {
        let mut block = SyscallBlock::new(Syscall::IpcReceive, Some(CapabilityHandle(7)));
        block.arg0 = 1;
        block.arg1 = 64;
        block.buffer_ptr = 0x400000;
        block.buffer_len = 64;
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        let written = block.write_to(&mut bytes).expect("Block schreiben");
        assert_eq!(written, SYSCALL_BLOCK_MIN_SIZE);
        assert_eq!(SyscallBlock::parse(&bytes[..written]), Ok(block));
    }

    #[test]
    fn block_must_declare_at_least_the_minimum_size() {
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        bytes[0..8].copy_from_slice(&8u64.to_le_bytes());
        assert_eq!(SyscallBlock::parse(&bytes), Err(AbiError::InvalidPayload));
    }

    #[test]
    fn block_may_not_claim_more_bytes_than_it_occupies() {
        // size = 200, geliefert werden 56 Byte: der Aufrufer luegt ueber seinen
        // eigenen Puffer, bevor der Kernel irgendetwas liest.
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        bytes[0..8].copy_from_slice(&200u64.to_le_bytes());
        assert_eq!(
            SyscallBlock::parse(&bytes[..SYSCALL_BLOCK_MIN_SIZE]),
            Err(AbiError::InvalidPayload)
        );
    }

    #[test]
    fn unknown_opcode_is_rejected() {
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        bytes[0..8].copy_from_slice(&(SYSCALL_BLOCK_MIN_SIZE as u64).to_le_bytes());
        bytes[8..16].copy_from_slice(&0x7777u64.to_le_bytes());
        assert_eq!(SyscallBlock::parse(&bytes), Err(AbiError::InvalidSyscall));
    }

    #[test]
    fn unknown_flags_are_rejected_instead_of_ignored() {
        let mut block = SyscallBlock::new(Syscall::Yield, None);
        block.flags = 0b10;
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        block.write_to(&mut bytes).expect("schreiben");
        assert_eq!(SyscallBlock::parse(&bytes), Err(AbiError::InvalidPayload));
    }

    #[test]
    fn newer_block_with_extra_fields_stays_readable() {
        // Erweiterbarkeit: ein spaeterer Minor-Stand haengt Felder an und
        // erhoeht size. Ein 2.0-Kernel muss den Block weiterhin verstehen.
        let mut block = SyscallBlock::new(Syscall::MonotonicTime, None);
        block.size = SYSCALL_BLOCK_MIN_SIZE + 40;
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        let written = block.write_to(&mut bytes).expect("schreiben");
        assert_eq!(written, SYSCALL_BLOCK_MIN_SIZE + 40);
        let parsed = SyscallBlock::parse(&bytes[..written]).expect("lesbar");
        assert_eq!(parsed.opcode, Syscall::MonotonicTime);
        assert_eq!(parsed.size, SYSCALL_BLOCK_MIN_SIZE + 40);
    }

    #[test]
    fn buffer_length_without_pointer_is_rejected() {
        let mut block = SyscallBlock::new(Syscall::IpcSend, Some(CapabilityHandle(1)));
        block.buffer_len = 16;
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        block.write_to(&mut bytes).expect("schreiben");
        assert_eq!(
            SyscallBlock::parse(&bytes[..SYSCALL_BLOCK_MIN_SIZE]),
            Err(AbiError::InvalidPayload)
        );
    }

    #[test]
    fn oversized_buffer_is_rejected() {
        let mut block = SyscallBlock::new(Syscall::IpcSend, None);
        block.buffer_ptr = 0x400000;
        block.buffer_len = MAX_SYSCALL_PAYLOAD + 1;
        let mut bytes = [0u8; SYSCALL_BLOCK_MAX_SIZE];
        block.write_to(&mut bytes).expect("schreiben");
        assert_eq!(
            SyscallBlock::parse(&bytes[..SYSCALL_BLOCK_MIN_SIZE]),
            Err(AbiError::InvalidPayload)
        );
    }

    #[test]
    fn block_does_not_write_past_the_caller_buffer() {
        let block = SyscallBlock::new(Syscall::Yield, None);
        let mut small = [0u8; 32];
        assert_eq!(block.write_to(&mut small), Err(AbiError::InvalidPayload));
    }

    #[test]
    fn syscall_ids_are_stable() {
        assert_eq!(Syscall::IpcSend.id(), 0x0010);
        assert_eq!(Syscall::MonotonicTime.id(), 0x0040);
    }

    #[test]
    fn simple_hash_is_stable() {
        assert_eq!(simple_hash(b"abc"), simple_hash(b"abc"));
        assert_ne!(simple_hash(b"abc"), simple_hash(b"abd"));
    }

    #[test]
    fn capability_rights_are_monotonic() {
        let rights = CapabilityRights::READ;
        assert!(rights.contains(CapabilityRights::READ));
        assert!(!rights.contains(CapabilityRights::WRITE));
    }

    #[test]
    fn handshake_accepts_minor_changes() {
        assert!(
            AbiHandshake {
                major: ABI_MAJOR,
                minor: 99
            }
            .compatible()
        );
        assert!(!AbiHandshake { major: 1, minor: 0 }.compatible());
        assert!(
            AbiHandshake {
                major: 2,
                minor: 42
            }
            .compatible()
        );
    }
}
