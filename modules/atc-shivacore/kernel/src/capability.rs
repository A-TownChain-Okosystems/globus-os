// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore Kernel — Capability-System (Rust).
//!
//! Portiert capabilities.py (Python, 08.07.2026) nach Rust.
//! Jede Ressource (Speicher, IPC-Kanal, Beschleuniger) ist nur ueber eine
//! Capability zugaenglich. Capabilities koennen delegiert werden, aber nur
//! mit Einschraenkung (Attenuation) — niemals mit Erweiterung.
//!
//! Eigenschaften (durch Rusts Ownership/Typsystem erzwungen):
//! - Rechte-Monotonie: delegierte Cap kann nie mehr Rechte haben als das Original
//! - Keine Forge: Capabilities werden vom Kernel erzeugt, nicht von Prozessen
//! - Widerrufbar: free() entzieht alle abgeleiteten Capabilities
//! - Atomic: Erzeugung/Delegation/Widerruf sind atomare Operationen (Spinlock)

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::{BitAnd, BitOr};

use crate::rights::{cnode, ObjectRights};

/// Ressourcen-Typen im Kernel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Memory,
    IpcChannel,
    Accelerator,
    FileSystem,
    Network,
    KnowledgeGraph,
}

/// Zugriffsrechte als Bitfield
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rights(pub u8);

impl Rights {
    pub const NONE: Rights = Rights(0);
    pub const READ: Rights = Rights(1);
    pub const WRITE: Rights = Rights(2);
    pub const EXEC: Rights = Rights(4);
    pub const DELEGATE: Rights = Rights(8);
    /// Observing capability/metadata without mutating (query, list, inspect).
    pub const INSPECT: Rights = Rights(16);
    pub const ALL: Rights = Rights(1 | 2 | 4 | 8 | 16);

    pub fn has(self, other: Rights) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn from_bits_truncate(bits: u8) -> Rights {
        Rights(bits & Self::ALL.bits())
    }
    pub fn bits(self) -> u8 {
        self.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Rights {
    type Output = Rights;
    fn bitor(self, rhs: Rights) -> Rights {
        Rights(self.0 | rhs.0)
    }
}

impl BitAnd for Rights {
    type Output = Rights;
    fn bitand(self, rhs: Rights) -> Rights {
        Rights(self.0 & rhs.0)
    }
}

/// Eindeutige Capability-ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CapId(pub u64);

/// Pid wird aus ats1000 re-exportiert (einheitlicher Typ)
pub use crate::ats1000::Pid;

/// Eine Capability — das Herzstueck des Sicherheitsmodells
#[derive(Debug, Clone)]
pub struct Capability {
    pub id: CapId,
    pub resource_type: ResourceType,
    pub resource_id: u64,
    /// Legacy rights are present only for transitional, untyped consumers.
    /// Typed capabilities must be accessed through `object_rights` APIs.
    pub rights: Rights,
    /// SC-005 typed rights. `None` identifies a legacy transitional entry.
    pub object_rights: Option<ObjectRights>,
    pub owner: Pid,
    /// Eltern-Capability, falls delegiert. None = vom Kernel direkt erzeugt.
    pub parent: Option<CapId>,
}

/// Fehler bei Capability-Operationen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    NotFound,
    NotOwner,
    NoDelegateRight,
    RightsExceeded,
    InvalidTypedRights,
    MissingCNodeMintRight,
    WrongObjectType,
    TypedOperationRequired,
}

/// Capability-Tabelle — Kernel-intern, geschuetzt durch Spinlock
#[derive(Clone)]
struct CapabilityTable {
    caps: BTreeMap<CapId, Capability>,
    by_pid: BTreeMap<Pid, Vec<CapId>>,
    next_id: u64,
}

/// Shared kernel-owned capability authority handle.
///
/// Kernel subsystems receive clones of this handle; all operations serialize
/// through the same capability table. The table itself remains the sole store.
pub(crate) struct CapabilityAuthority {
    inner: Arc<spin::Mutex<CapabilityTable>>,
}

impl CapabilityAuthority {
    fn new() -> Self {
        Self {
            inner: Arc::new(spin::Mutex::new(CapabilityTable::new())),
        }
    }

    /// Only the kernel owner retains this mutation facade. Subsystems receive scoped ports.
    pub(crate) fn owner(&self) -> CapabilityAuthorityOwner {
        CapabilityAuthorityOwner {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn check(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        required: Rights,
    ) -> bool {
        self.inner
            .lock()
            .check(pid, resource_type, resource_id, required)
    }
    pub(crate) fn check_any(&self, pid: Pid, cap_id: u64, required: Rights) -> bool {
        self.inner.lock().check_any(pid, cap_id, required)
    }
    pub(crate) fn check_typed(
        &self,
        pid: Pid,
        cap_id: CapId,
        resource_id: u64,
        required: ObjectRights,
    ) -> bool {
        self.inner
            .lock()
            .check_typed(pid, cap_id, resource_id, required)
    }
    pub(crate) fn check_legacy_handle(&self, pid: Pid, cap_id: CapId, required: Rights) -> bool {
        self.inner.lock().check_legacy_handle(pid, cap_id, required)
    }
    pub(crate) fn find_for_resource(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
    ) -> Option<CapId> {
        self.inner
            .lock()
            .find_for_resource(pid, resource_type, resource_id)
    }
    pub(crate) fn find_for_resource_with_rights(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        required: Rights,
    ) -> Option<CapId> {
        self.inner
            .lock()
            .find_for_resource_with_rights(pid, resource_type, resource_id, required)
    }
    pub(crate) fn count(&self) -> usize {
        self.inner.lock().count()
    }
}

/// Privileged mutation surface retained by KernelState / the test-harness owner only.
pub(crate) struct CapabilityAuthorityOwner {
    inner: Arc<spin::Mutex<CapabilityTable>>,
}
impl CapabilityAuthorityOwner {
    pub(crate) fn create(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        rights: Rights,
    ) -> CapId {
        self.inner
            .lock()
            .create(pid, resource_type, resource_id, rights)
    }
    pub(crate) fn create_typed(&self, pid: Pid, resource_id: u64, rights: ObjectRights) -> CapId {
        self.inner.lock().create_typed(pid, resource_id, rights)
    }
    pub(crate) fn mint_typed(
        &self,
        source_pid: Pid,
        source_cap: CapId,
        cnode_cap: CapId,
        target_pid: Pid,
        rights: ObjectRights,
    ) -> Result<CapId, CapabilityError> {
        self.inner
            .lock()
            .mint_typed(source_pid, source_cap, cnode_cap, target_pid, rights)
    }
    pub(crate) fn delegate(
        &self,
        source_pid: Pid,
        source_cap: CapId,
        target_pid: Pid,
        rights: Rights,
    ) -> Result<CapId, CapabilityError> {
        self.inner
            .lock()
            .delegate(source_pid, source_cap, target_pid, rights)
    }
    pub(crate) fn revoke(&self, cap_id: CapId) {
        self.inner.lock().revoke(cap_id);
    }
    pub(crate) fn revoke_for_resource(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
    ) {
        self.inner
            .lock()
            .revoke_for_resource(pid, resource_type, resource_id);
    }
    pub(crate) fn revoke_for_pid(&self, pid: Pid) {
        self.inner.lock().revoke_for_pid(pid);
    }
}

impl CapabilityAuthorityOwner {
    pub(crate) fn memory_port(&self) -> MemoryCapabilityPort {
        MemoryCapabilityPort {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn process_port(&self) -> ProcessCapabilityPort {
        ProcessCapabilityPort {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn ipc_port(&self) -> IpcCapabilityPort {
        IpcCapabilityPort {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn vfs_port(&self) -> VfsCapabilityPort {
        VfsCapabilityPort {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn syscall_port(&self) -> SyscallCapabilityPort {
        SyscallCapabilityPort {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn knowledge_graph_port(&self, principal: Pid) -> KnowledgeGraphCapabilityPort {
        KnowledgeGraphCapabilityPort {
            inner: self.inner.clone(),
            principal,
        }
    }
}

/// Public contract consumed by the service-space Knowledge Graph.
/// The concrete port is issued only by the kernel authority owner.
pub trait KnowledgeGraphCapabilityAccess {
    fn create_entity_capability(
        &self,
        creator: Pid,
        resource_id: u64,
    ) -> Result<CapId, CapabilityError>;
    fn check_entity_capability(&self, caller: Pid, resource_id: u64, required: Rights) -> bool;
    fn revoke_entity_capabilities(
        &self,
        owner: Pid,
        resource_id: u64,
    ) -> Result<(), CapabilityError>;
    fn delegate_entity_read(
        &self,
        owner: Pid,
        resource_id: u64,
        target: Pid,
    ) -> Result<CapId, CapabilityError>;
}

#[derive(Clone)]
pub struct KnowledgeGraphCapabilityPort {
    inner: Arc<spin::Mutex<CapabilityTable>>,
    principal: Pid,
}

impl KnowledgeGraphCapabilityAccess for KnowledgeGraphCapabilityPort {
    fn create_entity_capability(
        &self,
        creator: Pid,
        resource_id: u64,
    ) -> Result<CapId, CapabilityError> {
        if creator != self.principal {
            return Err(CapabilityError::NotOwner);
        }
        Ok(self.inner.lock().create(
            self.principal,
            ResourceType::KnowledgeGraph,
            resource_id,
            Rights::READ | Rights::WRITE,
        ))
    }

    fn check_entity_capability(&self, caller: Pid, resource_id: u64, required: Rights) -> bool {
        caller == self.principal
            && self.inner
                .lock()
                .check(caller, ResourceType::KnowledgeGraph, resource_id, required)
    }

    fn revoke_entity_capabilities(
        &self,
        owner: Pid,
        resource_id: u64,
    ) -> Result<(), CapabilityError> {
        if owner != self.principal {
            return Err(CapabilityError::NotOwner);
        }
        let mut table = self.inner.lock();
        let root = table
            .find_for_resource_with_rights(
                owner,
                ResourceType::KnowledgeGraph,
                resource_id,
                Rights::WRITE,
            )
            .ok_or(CapabilityError::NotOwner)?;
        table.revoke(root);
        Ok(())
    }

    fn delegate_entity_read(
        &self,
        owner: Pid,
        resource_id: u64,
        target: Pid,
    ) -> Result<CapId, CapabilityError> {
        if owner != self.principal {
            return Err(CapabilityError::NotOwner);
        }
        let mut table = self.inner.lock();
        let source = table
            .find_for_resource_with_rights(
                owner,
                ResourceType::KnowledgeGraph,
                resource_id,
                Rights::WRITE | Rights::DELEGATE,
            )
            .ok_or(CapabilityError::NoDelegateRight)?;
        table.delegate(owner, source, target, Rights::READ)
    }
}

#[derive(Clone)]
pub(crate) struct MemoryCapabilityPort {
    inner: Arc<spin::Mutex<CapabilityTable>>,
}

impl MemoryCapabilityPort {
    pub(crate) fn create_region(&self, pid: Pid, region_id: u64) -> CapId {
        self.inner.lock().create(
            pid,
            ResourceType::Memory,
            region_id,
            Rights::READ | Rights::WRITE,
        )
    }

    pub(crate) fn check_region(&self, pid: Pid, region_id: u64, required: Rights) -> bool {
        self.inner
            .lock()
            .check(pid, ResourceType::Memory, region_id, required)
    }

    pub(crate) fn revoke_region(&self, pid: Pid, region_id: u64) {
        self.inner
            .lock()
            .revoke_for_resource(pid, ResourceType::Memory, region_id);
    }
}

#[derive(Clone)]
pub(crate) struct ProcessCapabilityPort {
    inner: Arc<spin::Mutex<CapabilityTable>>,
}

impl ProcessCapabilityPort {
    pub(crate) fn create_address_space(&self, pid: Pid, resource_id: u64) -> CapId {
        self.inner.lock().create(
            pid,
            ResourceType::Memory,
            resource_id,
            Rights::READ | Rights::WRITE,
        )
    }

    pub(crate) fn revoke_process(&self, pid: Pid) {
        self.inner.lock().revoke_for_pid(pid);
    }

    pub(crate) fn check_address_space(&self, pid: Pid, resource_id: u64, required: Rights) -> bool {
        self.inner
            .lock()
            .check(pid, ResourceType::Memory, resource_id, required)
    }
}

#[derive(Clone)]
pub(crate) struct IpcCapabilityPort {
    inner: Arc<spin::Mutex<CapabilityTable>>,
}

impl IpcCapabilityPort {
    pub(crate) fn create_channel_send_capability(&self, pid: Pid, channel_id: u64) -> CapId {
        self.inner.lock().create(
            pid,
            ResourceType::IpcChannel,
            channel_id,
            Rights::WRITE | Rights::DELEGATE,
        )
    }
    pub(crate) fn create_channel_receive_capability(&self, pid: Pid, channel_id: u64) -> CapId {
        self.inner.lock().create(
            pid,
            ResourceType::IpcChannel,
            channel_id,
            Rights::READ | Rights::DELEGATE,
        )
    }

    pub(crate) fn check_channel(&self, pid: Pid, channel_id: u64, required: Rights) -> bool {
        self.inner
            .lock()
            .check(pid, ResourceType::IpcChannel, channel_id, required)
    }

    pub(crate) fn revoke_channel(&self, pid: Pid, channel_id: u64) {
        self.inner
            .lock()
            .revoke_for_resource(pid, ResourceType::IpcChannel, channel_id);
    }

    pub(crate) fn find_channel_with_rights(
        &self,
        pid: Pid,
        channel_id: u64,
        required: Rights,
    ) -> Option<CapId> {
        self.inner.lock().find_for_resource_with_rights(
            pid,
            ResourceType::IpcChannel,
            channel_id,
            required,
        )
    }

    pub(crate) fn delegate_channel(
        &self,
        source_pid: Pid,
        source_cap: CapId,
        target_pid: Pid,
        rights: Rights,
    ) -> Result<CapId, CapabilityError> {
        let mut table = self.inner.lock();
        let source = table
            .caps
            .get(&source_cap)
            .ok_or(CapabilityError::NotFound)?;
        if source.resource_type != ResourceType::IpcChannel {
            return Err(CapabilityError::WrongObjectType);
        }
        table.delegate(source_pid, source_cap, target_pid, rights)
    }
}

#[derive(Clone)]
pub(crate) struct VfsCapabilityPort {
    inner: Arc<spin::Mutex<CapabilityTable>>,
}

impl VfsCapabilityPort {
    pub(crate) fn check_legacy_handle(&self, pid: Pid, cap_id: CapId, required: Rights) -> bool {
        self.inner.lock().check_legacy_handle(pid, cap_id, required)
    }
}

#[derive(Clone)]
pub(crate) struct SyscallCapabilityPort {
    inner: Arc<spin::Mutex<CapabilityTable>>,
}

impl SyscallCapabilityPort {
    pub(crate) fn check_typed(
        &self,
        pid: Pid,
        cap_id: CapId,
        resource_id: u64,
        required: ObjectRights,
    ) -> bool {
        self.inner
            .lock()
            .check_typed(pid, cap_id, resource_id, required)
    }

    pub(crate) fn check_legacy_handle(&self, pid: Pid, cap_id: CapId, required: Rights) -> bool {
        self.inner.lock().check_legacy_handle(pid, cap_id, required)
    }

    pub(crate) fn check_any(&self, pid: Pid, cap_id: u64, required: Rights) -> bool {
        self.inner.lock().check_any(pid, cap_id, required)
    }
}

pub(crate) fn new_authority() -> CapabilityAuthority {
    CapabilityAuthority::new()
}

impl Default for CapabilityTable {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityTable {
    pub const fn new() -> Self {
        Self {
            caps: BTreeMap::new(),
            by_pid: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Erzeugt eine neue Capability fuer einen Prozess.
    pub fn create(
        &mut self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        rights: Rights,
    ) -> CapId {
        let id = CapId(self.next_id);
        self.next_id += 1;
        let cap = Capability {
            id,
            resource_type,
            resource_id,
            rights,
            object_rights: None,
            owner: pid,
            parent: None,
        };
        self.caps.insert(id, cap);
        self.by_pid.entry(pid).or_default().push(id);
        id
    }

    /// Delegiert eine Capability an einen anderen Prozess.
    /// Rechte koennen nur EINGESCHRAENKT werden (Attenuation).
    /// Kernel-owner delegation for a graph entity. Unlike process delegation,
    /// this operation is not exposed to the entity owner and does not grant DELEGATE.
    fn delegate_by_kernel_owner(
        &mut self,
        source_pid: Pid,
        source_cap: CapId,
        target_pid: Pid,
        attenuated_rights: Rights,
    ) -> Result<CapId, CapabilityError> {
        let parent_cap = self
            .caps
            .get(&source_cap)
            .ok_or(CapabilityError::NotFound)?
            .clone();
        if parent_cap.object_rights.is_some() {
            return Err(CapabilityError::TypedOperationRequired);
        }
        if parent_cap.owner != source_pid {
            return Err(CapabilityError::NotOwner);
        }
        if (attenuated_rights.0 & !parent_cap.rights.0) != 0 {
            return Err(CapabilityError::RightsExceeded);
        }
        let id = CapId(self.next_id);
        self.next_id += 1;
        let child = Capability {
            id,
            resource_type: parent_cap.resource_type,
            resource_id: parent_cap.resource_id,
            rights: attenuated_rights,
            object_rights: None,
            owner: target_pid,
            parent: Some(source_cap),
        };
        self.caps.insert(id, child);
        self.by_pid.entry(target_pid).or_default().push(id);
        Ok(id)
    }

    pub fn delegate(
        &mut self,
        source_pid: Pid,
        source_cap: CapId,
        target_pid: Pid,
        attenuated_rights: Rights,
    ) -> Result<CapId, CapabilityError> {
        let parent_cap = self
            .caps
            .get(&source_cap)
            .ok_or(CapabilityError::NotFound)?
            .clone();
        if parent_cap.object_rights.is_some() {
            return Err(CapabilityError::TypedOperationRequired);
        }
        if parent_cap.owner != source_pid {
            return Err(CapabilityError::NotOwner);
        }
        if !parent_cap.rights.has(Rights::DELEGATE) {
            return Err(CapabilityError::NoDelegateRight);
        }
        if (attenuated_rights.0 & !parent_cap.rights.0) != 0 {
            return Err(CapabilityError::RightsExceeded);
        }
        let id = CapId(self.next_id);
        self.next_id += 1;
        let child = Capability {
            id,
            resource_type: parent_cap.resource_type,
            resource_id: parent_cap.resource_id,
            rights: attenuated_rights,
            object_rights: None,
            owner: target_pid,
            parent: Some(source_cap),
        };
        self.caps.insert(id, child);
        self.by_pid.entry(target_pid).or_default().push(id);
        Ok(id)
    }

    /// Creates a capability with the owner-approved SC-005 typed rights.
    /// This is a kernel-authority operation; legacy lookup paths reject it.
    pub fn create_typed(
        &mut self,
        pid: Pid,
        resource_id: u64,
        object_rights: ObjectRights,
    ) -> CapId {
        let id = CapId(self.next_id);
        self.next_id += 1;
        let cap = Capability {
            id,
            // Compatibility placeholder; typed operations never inspect this field.
            resource_type: ResourceType::Memory,
            resource_id,
            rights: Rights::NONE,
            object_rights: Some(object_rights),
            owner: pid,
            parent: None,
        };
        self.caps.insert(id, cap);
        self.by_pid.entry(pid).or_default().push(id);
        id
    }

    /// Mints a typed child after validating CNode.MINT and same-type attenuation.
    pub fn mint_typed(
        &mut self,
        source_pid: Pid,
        source_cap: CapId,
        cnode_cap: CapId,
        target_pid: Pid,
        child_rights: ObjectRights,
    ) -> Result<CapId, CapabilityError> {
        let cnode = self.caps.get(&cnode_cap).ok_or(CapabilityError::NotFound)?;
        if cnode.owner != source_pid {
            return Err(CapabilityError::NotOwner);
        }
        let cnode_rights = cnode
            .object_rights
            .ok_or(CapabilityError::InvalidTypedRights)?;
        if cnode_rights.contains(cnode::MINT) != Ok(true) {
            return Err(CapabilityError::MissingCNodeMintRight);
        }

        let parent = self
            .caps
            .get(&source_cap)
            .ok_or(CapabilityError::NotFound)?;
        if parent.owner != source_pid {
            return Err(CapabilityError::NotOwner);
        }
        let parent_rights = parent
            .object_rights
            .ok_or(CapabilityError::InvalidTypedRights)?;
        parent_rights
            .attenuate(child_rights)
            .map_err(|error| match error {
                crate::rights::RightsError::CrossObjectType => CapabilityError::WrongObjectType,
                crate::rights::RightsError::RightsExpansion => CapabilityError::RightsExceeded,
                _ => CapabilityError::InvalidTypedRights,
            })?;

        let resource_id = parent.resource_id;
        let id = CapId(self.next_id);
        self.next_id += 1;
        let child = Capability {
            id,
            // Compatibility placeholder; typed operations never inspect this field.
            resource_type: ResourceType::Memory,
            resource_id,
            rights: Rights::NONE,
            object_rights: Some(child_rights),
            owner: target_pid,
            parent: Some(source_cap),
        };
        self.caps.insert(id, child);
        self.by_pid.entry(target_pid).or_default().push(id);
        Ok(id)
    }

    /// Checks owner, resource identity, object type and SC-005 rights together.
    pub fn check_typed(
        &self,
        pid: Pid,
        cap_id: CapId,
        resource_id: u64,
        required: ObjectRights,
    ) -> bool {
        self.caps
            .get(&cap_id)
            .map(|cap| {
                cap.owner == pid
                    && cap.resource_id == resource_id
                    && cap
                        .object_rights
                        .and_then(|rights| rights.contains(required).ok())
                        == Some(true)
            })
            .unwrap_or(false)
    }

    /// Prueft, ob ein Prozess eine bestimmte Capability mit den geforderten Rechten besitzt.
    pub fn check(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        required: Rights,
    ) -> bool {
        self.by_pid
            .get(&pid)
            .map(|caps| {
                caps.iter().any(|&cap_id| {
                    if let Some(cap) = self.caps.get(&cap_id) {
                        cap.object_rights.is_none()
                            && cap.resource_type == resource_type
                            && cap.resource_id == resource_id
                            && cap.rights.has(required)
                    } else {
                        false
                    }
                })
            })
            .unwrap_or(false)
    }

    /// Check capability by cap_id only (any resource type).
    pub fn check_any(&self, pid: Pid, cap_id: u64, required: Rights) -> bool {
        self.caps
            .get(&CapId(cap_id))
            .map(|cap| cap.object_rights.is_none() && cap.owner == pid && cap.rights.has(required))
            .unwrap_or(false)
    }

    /// Checks a legacy handle without exposing the stored capability record.
    pub fn check_legacy_handle(&self, pid: Pid, cap_id: CapId, required: Rights) -> bool {
        self.caps
            .get(&cap_id)
            .map(|cap| cap.object_rights.is_none() && cap.owner == pid && cap.rights.has(required))
            .unwrap_or(false)
    }

    /// Revokes all legacy capabilities for one resource identity.
    pub fn revoke_for_resource(&mut self, pid: Pid, resource_type: ResourceType, resource_id: u64) {
        let ids: Vec<CapId> = self
            .by_pid
            .get(&pid)
            .into_iter()
            .flatten()
            .filter_map(|id| self.caps.get(id))
            .filter(|cap| cap.resource_type == resource_type && cap.resource_id == resource_id)
            .map(|cap| cap.id)
            .collect();
        for id in ids {
            self.revoke(id);
        }
    }

    pub fn revoke_for_pid(&mut self, pid: Pid) {
        let ids: Vec<CapId> = self.by_pid.get(&pid).cloned().unwrap_or_default();
        for id in ids {
            self.revoke(id);
        }
    }

    pub fn get(&self, id: CapId) -> Option<&Capability> {
        self.caps.get(&id)
    }

    /// Widerruft eine Capability und alle davon abgeleiteten (rekursiv).
    pub fn revoke(&mut self, cap_id: CapId) {
        let mut to_remove = Vec::new();
        self.collect_descendants(cap_id, &mut to_remove);
        for id in &to_remove {
            self.caps.remove(id);
        }
        for pid_caps in self.by_pid.values_mut() {
            pid_caps.retain(|c| !to_remove.contains(c));
        }
    }

    fn collect_descendants(&self, cap_id: CapId, out: &mut Vec<CapId>) {
        out.push(cap_id);
        for (id, cap) in &self.caps {
            if cap.parent == Some(cap_id) {
                self.collect_descendants(*id, out);
            }
        }
    }

    pub fn list_for(&self, pid: Pid) -> Vec<&Capability> {
        self.by_pid
            .get(&pid)
            .map(|caps| caps.iter().filter_map(|id| self.caps.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn find_for_resource(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
    ) -> Option<CapId> {
        self.by_pid.get(&pid)?.iter().find_map(|id| {
            self.caps.get(id).and_then(|cap| {
                (cap.resource_type == resource_type && cap.resource_id == resource_id)
                    .then_some(cap.id)
            })
        })
    }

    pub fn find_for_resource_with_rights(
        &self,
        pid: Pid,
        resource_type: ResourceType,
        resource_id: u64,
        required: Rights,
    ) -> Option<CapId> {
        self.by_pid.get(&pid)?.iter().find_map(|id| {
            self.caps.get(id).and_then(|cap| {
                (cap.object_rights.is_none()
                    && cap.resource_type == resource_type
                    && cap.resource_id == resource_id
                    && cap.rights.has(required))
                .then_some(cap.id)
            })
        })
    }

    pub fn count(&self) -> usize {
        self.caps.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pid(n: u32) -> Pid {
        Pid(n)
    }

    #[test]
    fn memory_port_issues_only_read_write_rights() {
        let authority = new_authority();
        let port = authority.owner().memory_port();
        port.create_region(pid(1), 0x1000);
        assert!(authority.check(pid(1), ResourceType::Memory, 0x1000, Rights::READ));
        assert!(authority.check(pid(1), ResourceType::Memory, 0x1000, Rights::WRITE));
        assert!(!authority.check(pid(1), ResourceType::Memory, 0x1000, Rights::EXEC));
        assert!(!authority.check(pid(1), ResourceType::Memory, 0x1000, Rights::DELEGATE));
    }
    #[test]
    fn owner_delegation_rejects_wrong_owner_and_missing_delegate_right() {
        let authority = new_authority();
        let owner = authority.owner();
        let cap = owner.create(
            pid(1),
            ResourceType::Memory,
            0x2000,
            Rights::READ | Rights::WRITE,
        );
        assert_eq!(
            owner.delegate(pid(2), cap, pid(3), Rights::READ),
            Err(CapabilityError::NotOwner)
        );
        assert_eq!(
            owner.delegate(pid(1), cap, pid(3), Rights::READ),
            Err(CapabilityError::NoDelegateRight)
        );
    }
    #[test]
    fn owner_revocation_removes_descendants() {
        let authority = new_authority();
        let owner = authority.owner();
        let root = owner.create(
            pid(1),
            ResourceType::Memory,
            0x3000,
            Rights::READ | Rights::DELEGATE,
        );
        let _child = owner.delegate(pid(1), root, pid(2), Rights::READ).unwrap();
        owner.revoke(root);
        assert!(!authority.check(pid(1), ResourceType::Memory, 0x3000, Rights::READ));
        assert!(!authority.check(pid(2), ResourceType::Memory, 0x3000, Rights::READ));
        assert_eq!(authority.count(), 0);
    }

    #[test]
    fn knowledge_graph_port_uses_least_authority_and_owner_delegation() {
        let authority = new_authority();
        let port = authority.owner().knowledge_graph_port();
        let root = port.create_entity_capability(pid(1), 0x4000);
        assert!(port.check_entity_capability(pid(1), 0x4000, Rights::READ));
        assert!(port.check_entity_capability(pid(1), 0x4000, Rights::WRITE));
        assert!(!port.check_entity_capability(pid(1), 0x4000, Rights::EXEC));
        assert!(!port.check_entity_capability(pid(1), 0x4000, Rights::DELEGATE));
        let child = port.delegate_entity_read(pid(1), 0x4000, pid(2)).unwrap();
        assert!(port.check_entity_capability(pid(2), 0x4000, Rights::READ));
        assert!(!port.check_entity_capability(pid(2), 0x4000, Rights::WRITE));
        port.revoke_entity_capabilities(pid(1), 0x4000);
        assert!(!port.check_entity_capability(pid(1), 0x4000, Rights::READ));
        assert!(!port.check_entity_capability(pid(2), 0x4000, Rights::READ));
        let _ = (root, child);
    }

    #[test]
    fn knowledge_graph_port_rejects_unauthorized_delegate_and_revoke() {
        let authority = new_authority();
        let port = authority.owner().knowledge_graph_port();
        port.create_entity_capability(pid(1), 0x5000);
        assert_eq!(
            port.delegate_entity_read(pid(2), 0x5000, pid(3)),
            Err(CapabilityError::NotOwner)
        );
        port.revoke_entity_capabilities(pid(2), 0x5000);
        assert!(port.check_entity_capability(pid(1), 0x5000, Rights::READ));
    }

    #[test]
    fn typed_capability_checks_are_type_bound_and_fail_closed() {
        let mut table = CapabilityTable::new();
        let frame = table.create_typed(pid(1), 0x2000, crate::rights::frame::READ);
        assert!(table.check_typed(pid(1), frame, 0x2000, crate::rights::frame::READ));
        assert!(!table.check_typed(pid(1), frame, 0x2000, crate::rights::frame::WRITE));
        assert!(!table.check_typed(pid(2), frame, 0x2000, crate::rights::frame::READ));
        assert!(!table.check_typed(pid(1), frame, 0x3000, crate::rights::frame::READ));
        assert!(!table.check(pid(1), ResourceType::Memory, 0x2000, Rights::NONE));
        assert!(!table.check_any(pid(1), frame.0, Rights::NONE));
    }

    #[test]
    fn typed_mint_requires_cnode_mint_and_attenuates() {
        let mut table = CapabilityTable::new();
        let parent_rights =
            crate::rights::ObjectRights::new(crate::rights::ObjectType::Frame, 0b0011).unwrap();
        let expanded =
            crate::rights::ObjectRights::new(crate::rights::ObjectType::Frame, 0b0111).unwrap();
        let frame = table.create_typed(pid(1), 0x4000, parent_rights);
        let cnode = table.create_typed(pid(1), 7, crate::rights::cnode::MINT);
        let child = table
            .mint_typed(pid(1), frame, cnode, pid(2), crate::rights::frame::READ)
            .unwrap();
        assert!(table.check_typed(pid(2), child, 0x4000, crate::rights::frame::READ));
        assert!(!table.check_typed(pid(2), child, 0x4000, crate::rights::frame::WRITE));
        assert_eq!(
            table.mint_typed(pid(1), frame, cnode, pid(3), expanded),
            Err(CapabilityError::RightsExceeded)
        );
    }

    #[test]
    fn typed_mint_rejects_missing_cnode_authority_and_cross_type_rights() {
        let mut table = CapabilityTable::new();
        let frame = table.create_typed(pid(1), 0x5000, crate::rights::frame::READ);
        let wrong_cnode = table.create_typed(pid(1), 8, crate::rights::cnode::DUPLICATE);
        assert_eq!(
            table.mint_typed(
                pid(1),
                frame,
                wrong_cnode,
                pid(2),
                crate::rights::frame::READ
            ),
            Err(CapabilityError::MissingCNodeMintRight)
        );
        let cnode = table.create_typed(pid(1), 9, crate::rights::cnode::MINT);
        assert_eq!(
            table.mint_typed(pid(1), frame, cnode, pid(2), crate::rights::endpoint::SEND),
            Err(CapabilityError::WrongObjectType)
        );
    }

    #[test]
    fn typed_revocation_removes_all_derived_capabilities() {
        let mut table = CapabilityTable::new();
        let root_rights =
            crate::rights::ObjectRights::new(crate::rights::ObjectType::Frame, 0b1001).unwrap();
        let root = table.create_typed(pid(1), 0x7000, root_rights);
        let cnode = table.create_typed(pid(1), 10, crate::rights::cnode::MINT);
        let child = table
            .mint_typed(pid(1), root, cnode, pid(2), crate::rights::frame::READ)
            .unwrap();

        assert!(table.check_typed(pid(1), root, 0x7000, crate::rights::frame::READ));
        assert!(table.check_typed(pid(2), child, 0x7000, crate::rights::frame::READ));
        table.revoke(root);
        assert!(!table.check_typed(pid(1), root, 0x7000, crate::rights::frame::READ));
        assert!(!table.check_typed(pid(2), child, 0x7000, crate::rights::frame::READ));
        assert_eq!(table.count(), 1);
    }

    #[test]
    fn legacy_operations_cannot_consume_typed_capabilities() {
        let mut table = CapabilityTable::new();
        let typed = table.create_typed(pid(1), 0x6000, crate::rights::frame::READ);
        assert_eq!(
            table.delegate(pid(1), typed, pid(2), Rights::READ),
            Err(CapabilityError::TypedOperationRequired)
        );
        assert!(!table.check(pid(1), ResourceType::Memory, 0x6000, Rights::NONE));
    }

    #[test]
    fn test_create_and_check() {
        let mut table = CapabilityTable::new();
        let cap = table.create(
            pid(1),
            ResourceType::Memory,
            0x1000,
            Rights::READ | Rights::WRITE,
        );
        assert!(cap.0 > 0);
        assert!(table.check(pid(1), ResourceType::Memory, 0x1000, Rights::READ));
        assert!(table.check(pid(1), ResourceType::Memory, 0x1000, Rights::WRITE));
        assert!(!table.check(pid(1), ResourceType::Memory, 0x1000, Rights::EXEC));
        assert!(!table.check(pid(2), ResourceType::Memory, 0x1000, Rights::READ));
    }

    #[test]
    fn test_delegate_attenuation() {
        let mut table = CapabilityTable::new();
        let parent = table.create(
            pid(1),
            ResourceType::IpcChannel,
            42,
            Rights::READ | Rights::WRITE | Rights::DELEGATE,
        );
        let child = table
            .delegate(pid(1), parent, pid(2), Rights::READ)
            .unwrap();
        assert!(table.check(pid(2), ResourceType::IpcChannel, 42, Rights::READ));
        assert!(!table.check(pid(2), ResourceType::IpcChannel, 42, Rights::WRITE));
    }

    #[test]
    fn test_delegate_rejects_rights_expansion() {
        let mut table = CapabilityTable::new();
        let parent = table.create(
            pid(1),
            ResourceType::Memory,
            0,
            Rights::READ | Rights::DELEGATE,
        );
        let result = table.delegate(pid(1), parent, pid(2), Rights::READ | Rights::WRITE);
        assert_eq!(result, Err(CapabilityError::RightsExceeded));
    }

    #[test]
    fn test_delegate_requires_delegate_right() {
        let mut table = CapabilityTable::new();
        let parent = table.create(
            pid(1),
            ResourceType::Memory,
            0,
            Rights::READ | Rights::WRITE,
        );
        let result = table.delegate(pid(1), parent, pid(2), Rights::READ);
        assert_eq!(result, Err(CapabilityError::NoDelegateRight));
    }

    #[test]
    fn test_delegate_wrong_owner() {
        let mut table = CapabilityTable::new();
        let parent = table.create(pid(1), ResourceType::Memory, 0, Rights::ALL);
        let result = table.delegate(pid(2), parent, pid(3), Rights::READ);
        assert_eq!(result, Err(CapabilityError::NotOwner));
    }

    #[test]
    fn test_revoke_cascade() {
        let mut table = CapabilityTable::new();
        let root = table.create(pid(1), ResourceType::Memory, 0, Rights::ALL);
        let child = table
            .delegate(pid(1), root, pid(2), Rights::READ | Rights::DELEGATE)
            .unwrap();
        let grandchild = table.delegate(pid(2), child, pid(3), Rights::READ).unwrap();
        assert!(table.check(pid(1), ResourceType::Memory, 0, Rights::READ));
        assert!(table.check(pid(2), ResourceType::Memory, 0, Rights::READ));
        assert!(table.check(pid(3), ResourceType::Memory, 0, Rights::READ));
        table.revoke(root);
        assert!(!table.check(pid(1), ResourceType::Memory, 0, Rights::READ));
        assert!(!table.check(pid(2), ResourceType::Memory, 0, Rights::READ));
        assert!(!table.check(pid(3), ResourceType::Memory, 0, Rights::READ));
        assert_eq!(table.count(), 0);
    }

    #[test]
    fn test_list_for_process() {
        let mut table = CapabilityTable::new();
        table.create(pid(1), ResourceType::Memory, 0, Rights::READ);
        table.create(pid(1), ResourceType::IpcChannel, 5, Rights::WRITE);
        table.create(pid(2), ResourceType::Memory, 1, Rights::READ);
        let pid1_caps = table.list_for(pid(1));
        assert_eq!(pid1_caps.len(), 2);
        let pid2_caps = table.list_for(pid(2));
        assert_eq!(pid2_caps.len(), 1);
    }

    #[test]
    fn test_rights_operations() {
        let r = Rights::READ | Rights::WRITE;
        assert!(r.has(Rights::READ));
        assert!(r.has(Rights::WRITE));
        assert!(!r.has(Rights::EXEC));
        assert!(!Rights::NONE.has(Rights::READ));
        assert!(Rights::ALL.has(Rights::READ | Rights::WRITE | Rights::EXEC | Rights::DELEGATE));
    }

    #[test]
    fn test_from_bits_truncate_preserves_known_rights_and_discards_unknown_bits() {
        assert_eq!(
            Rights::from_bits_truncate(Rights::READ.bits()),
            Rights::READ
        );
        assert_eq!(
            Rights::from_bits_truncate(Rights::WRITE.bits()),
            Rights::WRITE
        );
        assert_eq!(
            Rights::from_bits_truncate(Rights::EXEC.bits()),
            Rights::EXEC
        );
        assert_eq!(
            Rights::from_bits_truncate(Rights::DELEGATE.bits()),
            Rights::DELEGATE
        );
        assert_eq!(
            Rights::from_bits_truncate(Rights::INSPECT.bits()),
            Rights::INSPECT
        );
        assert_eq!(Rights::from_bits_truncate(Rights::ALL.bits()), Rights::ALL);
        assert_eq!(Rights::from_bits_truncate(0x20), Rights::NONE);
        assert_eq!(Rights::from_bits_truncate(0xFF), Rights::ALL);
    }
}
