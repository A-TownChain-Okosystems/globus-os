//! Kernel-owned authenticated session registry.
//!
//! Session handles are tagged capability IDs, not bearer secrets: the canonical
//! CapabilityAuthority binds every handle to its owning PID and resource type.
//! Session metadata and expiry remain kernel-owned; userspace receives no UserId
//! or mutable session object.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::ats1000::Pid;
use crate::cap_authority::CapabilityAuthority;
use crate::capability::{CapId, ResourceType, Rights};

/// Marker distinguishing an opaque session capability from ordinary result codes.
pub const SESSION_HANDLE_TAG: u64 = libshivacore::IDENTITY_SESSION_HANDLE_TAG;
/// Default five-minute session lifetime at the 100 Hz kernel timer rate.
pub const DEFAULT_SESSION_TTL_TICKS: u64 = 5 * 60 * 100;

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static SESSIONS: Mutex<BTreeMap<u64, KernelSession>> = Mutex::new(BTreeMap::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    InvalidLifetime,
    Exhausted,
    InvalidHandle,
    NotFound,
    WrongOwner,
    Locked,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Locked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KernelSession {
    session_id: u64,
    user_id: String,
    owner_pid: Pid,
    issued_at_tick: u64,
    expires_at_tick: u64,
    root_capability: CapId,
    state: SessionState,
}

/// Issue one PID-bound session capability after successful credential verification.
pub fn issue(
    authority: &CapabilityAuthority,
    owner_pid: Pid,
    user_id: String,
    now_tick: u64,
    ttl_ticks: u64,
) -> Result<u64, SessionError> {
    if user_id.is_empty() || ttl_ticks == 0 {
        return Err(SessionError::InvalidLifetime);
    }
    let expires_at_tick = now_tick
        .checked_add(ttl_ticks)
        .ok_or(SessionError::InvalidLifetime)?;

    let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    if session_id == 0 || session_id >= SESSION_HANDLE_TAG {
        return Err(SessionError::Exhausted);
    }

    // A Greeter process may hold at most one live login session.
    revoke_for_pid(owner_pid, authority);
    let capability = authority.create(
        owner_pid,
        ResourceType::IdentitySession,
        session_id,
        Rights::READ,
    );
    if capability.0 >= SESSION_HANDLE_TAG {
        authority.revoke(capability);
        return Err(SessionError::Exhausted);
    }
    SESSIONS.lock().insert(
        session_id,
        KernelSession {
            session_id,
            user_id,
            owner_pid,
            issued_at_tick: now_tick,
            expires_at_tick,
            root_capability: capability,
            state: SessionState::Active,
        },
    );
    Ok(SESSION_HANDLE_TAG | capability.0)
}

fn capability_for(
    authority: &CapabilityAuthority,
    caller_pid: Pid,
    opaque_handle: u64,
) -> Result<(CapId, u64), SessionError> {
    if opaque_handle & SESSION_HANDLE_TAG == 0 {
        return Err(SessionError::InvalidHandle);
    }
    let cap_id = CapId(opaque_handle & !SESSION_HANDLE_TAG);
    let capability = authority
        .capability(cap_id)
        .ok_or(SessionError::NotFound)?;
    if capability.owner != caller_pid {
        return Err(SessionError::WrongOwner);
    }
    if capability.resource_type != ResourceType::IdentitySession
        || !capability.rights.has(Rights::READ)
    {
        return Err(SessionError::WrongOwner);
    }
    Ok((cap_id, capability.resource_id))
}

/// Resolve a live session to its stable UserId. The caller must own a valid
/// session capability; delegated child capabilities resolve to the same session.
pub fn resolve(
    authority: &CapabilityAuthority,
    caller_pid: Pid,
    opaque_handle: u64,
    now_tick: u64,
) -> Result<String, SessionError> {
    let (_cap_id, session_id) = capability_for(authority, caller_pid, opaque_handle)?;
    let mut expired_root = None;
    let result = {
        let mut sessions = SESSIONS.lock();
        let session = sessions.get(&session_id).ok_or(SessionError::NotFound)?;
        if now_tick >= session.expires_at_tick {
            expired_root = Some(session.root_capability);
            sessions.remove(&session_id);
            Err(SessionError::Expired)
        } else if session.state == SessionState::Locked {
            Err(SessionError::Locked)
        } else if session.session_id != session_id || session_id == 0 {
            Err(SessionError::NotFound)
        } else {
            Ok(session.user_id.clone())
        }
    };
    if let Some(root) = expired_root {
        authority.revoke(root);
    }
    result
}

/// Lock a session without exposing or changing its principal.
pub fn lock(
    authority: &CapabilityAuthority,
    caller_pid: Pid,
    opaque_handle: u64,
) -> Result<(), SessionError> {
    let (_, session_id) = capability_for(authority, caller_pid, opaque_handle)?;
    let mut sessions = SESSIONS.lock();
    let session = sessions.get_mut(&session_id).ok_or(SessionError::NotFound)?;
    session.state = SessionState::Locked;
    Ok(())
}

/// Revoke a session and every capability derived from its root.
pub fn revoke(
    authority: &CapabilityAuthority,
    caller_pid: Pid,
    opaque_handle: u64,
) -> Result<(), SessionError> {
    let (_, session_id) = capability_for(authority, caller_pid, opaque_handle)?;
    let root = SESSIONS
        .lock()
        .remove(&session_id)
        .ok_or(SessionError::NotFound)?
        .root_capability;
    authority.revoke(root);
    Ok(())
}

/// Remove all root sessions owned by an exiting process. CapabilityAuthority
/// revokes each root and its delegated descendants.
pub fn revoke_for_pid(owner_pid: Pid, authority: &CapabilityAuthority) -> usize {
    let roots = {
        let mut sessions = SESSIONS.lock();
        let ids: alloc::vec::Vec<u64> = sessions
            .iter()
            .filter_map(|(id, session)| (session.owner_pid == owner_pid).then_some(*id))
            .collect();
        ids.into_iter()
            .filter_map(|id| sessions.remove(&id).map(|session| session.root_capability))
            .collect::<alloc::vec::Vec<_>>()
    };
    let count = roots.len();
    for root in roots {
        authority.revoke(root);
    }
    count
}

/// Transfer principal ownership after the kernel has created an authorized
/// target process. The new root capability belongs to the target PID; the
/// Greeter's prior root is revoked before the transfer completes.
pub fn transfer(
    authority: &CapabilityAuthority,
    source_pid: Pid,
    target_pid: Pid,
    opaque_handle: u64,
    now_tick: u64,
) -> Result<u64, SessionError> {
    if source_pid == target_pid {
        return Err(SessionError::WrongOwner);
    }
    let user_id = resolve(authority, source_pid, opaque_handle, now_tick)?;
    let (_, session_id) = capability_for(authority, source_pid, opaque_handle)?;
    let remaining = {
        let sessions = SESSIONS.lock();
        let session = sessions.get(&session_id).ok_or(SessionError::NotFound)?;
        session.expires_at_tick.saturating_sub(now_tick)
    };
    if remaining == 0 {
        return Err(SessionError::Expired);
    }
    let replacement = issue(authority, target_pid, user_id, now_tick, remaining)?;
    let root = {
        let mut sessions = SESSIONS.lock();
        sessions
            .remove(&session_id)
            .ok_or(SessionError::NotFound)?
            .root_capability
    };
    authority.revoke(root);
    Ok(replacement)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(value: u32) -> Pid {
        Pid(value)
    }

    #[test]
    fn session_is_pid_bound_and_resolves_canonical_user_id() {
        let authority = CapabilityAuthority::new();
        let handle = issue(&authority, pid(10), String::from("user-alice"), 100, 500).unwrap();
        assert_eq!(
            resolve(&authority, pid(10), handle, 101),
            Ok(String::from("user-alice"))
        );
        assert_eq!(
            resolve(&authority, pid(11), handle, 101),
            Err(SessionError::WrongOwner)
        );
    }

    #[test]
    fn expired_session_is_removed_and_its_capability_revoked() {
        let authority = CapabilityAuthority::new();
        let handle = issue(&authority, pid(20), String::from("user-bob"), 100, 10).unwrap();
        assert_eq!(
            resolve(&authority, pid(20), handle, 110),
            Err(SessionError::Expired)
        );
        assert_eq!(
            resolve(&authority, pid(20), handle, 111),
            Err(SessionError::NotFound)
        );
    }

    #[test]
    fn locked_session_cannot_be_resolved() {
        let authority = CapabilityAuthority::new();
        let handle = issue(&authority, pid(30), String::from("user-carol"), 100, 500).unwrap();
        lock(&authority, pid(30), handle).unwrap();
        assert_eq!(
            resolve(&authority, pid(30), handle, 101),
            Err(SessionError::Locked)
        );
    }

    #[test]
    fn revocation_and_process_exit_remove_session_authority() {
        let authority = CapabilityAuthority::new();
        let first = issue(&authority, pid(40), String::from("user-dave"), 100, 500).unwrap();
        revoke(&authority, pid(40), first).unwrap();
        assert_eq!(
            resolve(&authority, pid(40), first, 101),
            Err(SessionError::NotFound)
        );

        let second = issue(&authority, pid(41), String::from("user-erin"), 100, 500).unwrap();
        assert_eq!(revoke_for_pid(pid(41), &authority), 1);
        assert_eq!(
            resolve(&authority, pid(41), second, 101),
            Err(SessionError::NotFound)
        );
    }

    #[test]
    fn session_transfer_rebinds_the_root_capability_to_the_target_pid() {
        let authority = CapabilityAuthority::new();
        let original = issue(&authority, pid(50), String::from("user-frank"), 100, 500).unwrap();
        let moved = transfer(&authority, pid(50), pid(51), original, 150).unwrap();
        assert_eq!(
            resolve(&authority, pid(51), moved, 151),
            Ok(String::from("user-frank"))
        );
        assert_eq!(
            resolve(&authority, pid(50), original, 151),
            Err(SessionError::NotFound)
        );
    }

    #[test]
    fn zero_and_overflowing_lifetimes_are_rejected() {
        let authority = CapabilityAuthority::new();
        assert_eq!(
            issue(&authority, pid(60), String::from("user-test"), 1, 0),
            Err(SessionError::InvalidLifetime)
        );
        assert_eq!(
            issue(&authority, pid(60), String::from("user-test"), u64::MAX, 1),
            Err(SessionError::InvalidLifetime)
        );
    }
}
