//! SC-005 IMPL-001 v1.0.0 typed rights encoding.
//! Rights are valid only as the pair (ObjectType, RightsMask<u16>).
//! This module defines no wire, G7, syscall, or persistent-storage ABI.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ObjectType {
    Endpoint = 1,
    Notification = 2,
    Frame = 3,
    AddressSpace = 4,
    Thread = 5,
    CNode = 6,
    UntypedMemory = 7,
    Timer = 8,
    DeviceIrq = 9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightsError {
    UnknownObjectType,
    InvalidMask,
    CrossObjectType,
    RightsExpansion,
}

impl TryFrom<u8> for ObjectType {
    type Error = RightsError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Endpoint),
            2 => Ok(Self::Notification),
            3 => Ok(Self::Frame),
            4 => Ok(Self::AddressSpace),
            5 => Ok(Self::Thread),
            6 => Ok(Self::CNode),
            7 => Ok(Self::UntypedMemory),
            8 => Ok(Self::Timer),
            9 => Ok(Self::DeviceIrq),
            _ => Err(RightsError::UnknownObjectType),
        }
    }
}

impl ObjectType {
    pub const fn allowed_mask(self) -> u16 {
        match self {
            Self::Endpoint | Self::Frame => 0x000f,
            Self::Notification | Self::Timer => 0x0003,
            Self::AddressSpace | Self::Thread | Self::CNode | Self::DeviceIrq => 0x0007,
            Self::UntypedMemory => 0x0001,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RightsMask(u16);

impl RightsMask {
    pub const fn bits(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectRights {
    object_type: ObjectType,
    mask: RightsMask,
}

impl ObjectRights {
    pub const fn new(object_type: ObjectType, raw: u16) -> Result<Self, RightsError> {
        if raw & !object_type.allowed_mask() != 0 {
            return Err(RightsError::InvalidMask);
        }
        Ok(Self {
            object_type,
            mask: RightsMask(raw),
        })
    }

    pub fn decode(raw_type: u8, raw_mask: u16) -> Result<Self, RightsError> {
        Self::new(ObjectType::try_from(raw_type)?, raw_mask)
    }

    const fn defined(object_type: ObjectType, mask: u16) -> Self {
        Self {
            object_type,
            mask: RightsMask(mask),
        }
    }

    pub const fn object_type(self) -> ObjectType {
        self.object_type
    }

    pub const fn bits(self) -> u16 {
        self.mask.bits()
    }

    pub const fn encode(self) -> (u8, u16) {
        (self.object_type as u8, self.bits())
    }

    pub const fn contains(self, required: Self) -> Result<bool, RightsError> {
        if self.object_type as u8 != required.object_type as u8 {
            return Err(RightsError::CrossObjectType);
        }
        Ok(self.bits() & required.bits() == required.bits())
    }

    pub const fn attenuate(self, child: Self) -> Result<Self, RightsError> {
        if self.object_type as u8 != child.object_type as u8 {
            return Err(RightsError::CrossObjectType);
        }
        if child.bits() & !self.bits() != 0 {
            return Err(RightsError::RightsExpansion);
        }
        Ok(child)
    }
}

pub mod endpoint {
    use super::*;
    pub const SEND: ObjectRights = ObjectRights::defined(ObjectType::Endpoint, 1);
    pub const RECV: ObjectRights = ObjectRights::defined(ObjectType::Endpoint, 2);
    pub const DELEGATE: ObjectRights = ObjectRights::defined(ObjectType::Endpoint, 4);
    pub const NOTIFY: ObjectRights = ObjectRights::defined(ObjectType::Endpoint, 8);
}

pub mod notification {
    use super::*;
    pub const SIGNAL: ObjectRights = ObjectRights::defined(ObjectType::Notification, 1);
    pub const WAIT: ObjectRights = ObjectRights::defined(ObjectType::Notification, 2);
}

pub mod frame {
    use super::*;
    pub const READ: ObjectRights = ObjectRights::defined(ObjectType::Frame, 1);
    pub const WRITE: ObjectRights = ObjectRights::defined(ObjectType::Frame, 2);
    pub const EXECUTE: ObjectRights = ObjectRights::defined(ObjectType::Frame, 4);
    pub const GRANT: ObjectRights = ObjectRights::defined(ObjectType::Frame, 8);
}

pub mod address_space {
    use super::*;
    pub const MAP: ObjectRights = ObjectRights::defined(ObjectType::AddressSpace, 1);
    pub const UNMAP: ObjectRights = ObjectRights::defined(ObjectType::AddressSpace, 2);
    pub const SWITCH: ObjectRights = ObjectRights::defined(ObjectType::AddressSpace, 4);
}

pub mod thread {
    use super::*;
    pub const SUSPEND: ObjectRights = ObjectRights::defined(ObjectType::Thread, 1);
    pub const RESUME: ObjectRights = ObjectRights::defined(ObjectType::Thread, 2);
    pub const CONFIG: ObjectRights = ObjectRights::defined(ObjectType::Thread, 4);
}

pub mod cnode {
    use super::*;
    pub const DUPLICATE: ObjectRights = ObjectRights::defined(ObjectType::CNode, 1);
    pub const MINT: ObjectRights = ObjectRights::defined(ObjectType::CNode, 2);
    pub const MOVE: ObjectRights = ObjectRights::defined(ObjectType::CNode, 4);
}

pub mod untyped_memory {
    use super::*;
    pub const RETYPE: ObjectRights = ObjectRights::defined(ObjectType::UntypedMemory, 1);
}

pub mod timer {
    use super::*;
    pub const SET_DEADLINE: ObjectRights = ObjectRights::defined(ObjectType::Timer, 1);
    pub const ACK: ObjectRights = ObjectRights::defined(ObjectType::Timer, 2);
}

pub mod device_irq {
    use super::*;
    pub const ACCESS: ObjectRights = ObjectRights::defined(ObjectType::DeviceIrq, 1);
    pub const SUBSCRIBE: ObjectRights = ObjectRights::defined(ObjectType::DeviceIrq, 2);
    pub const ACK: ObjectRights = ObjectRights::defined(ObjectType::DeviceIrq, 4);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_named_rights_match_v1() {
        let all = [
            endpoint::SEND,
            endpoint::RECV,
            endpoint::DELEGATE,
            endpoint::NOTIFY,
            notification::SIGNAL,
            notification::WAIT,
            frame::READ,
            frame::WRITE,
            frame::EXECUTE,
            frame::GRANT,
            address_space::MAP,
            address_space::UNMAP,
            address_space::SWITCH,
            thread::SUSPEND,
            thread::RESUME,
            thread::CONFIG,
            cnode::DUPLICATE,
            cnode::MINT,
            cnode::MOVE,
            untyped_memory::RETYPE,
            timer::SET_DEADLINE,
            timer::ACK,
            device_irq::ACCESS,
            device_irq::SUBSCRIBE,
            device_irq::ACK,
        ];
        assert_eq!(all.len(), 25);
        for right in all {
            let (object_type, mask) = right.encode();
            assert_eq!(ObjectRights::decode(object_type, mask), Ok(right));
        }
    }

    #[test]
    fn invalid_and_unknown_are_rejected() {
        for object_type in [
            ObjectType::Endpoint,
            ObjectType::Notification,
            ObjectType::Frame,
            ObjectType::AddressSpace,
            ObjectType::Thread,
            ObjectType::CNode,
            ObjectType::UntypedMemory,
            ObjectType::Timer,
            ObjectType::DeviceIrq,
        ] {
            for bit in 0..16 {
                let mask = 1u16 << bit;
                if object_type.allowed_mask() & mask == 0 {
                    assert_eq!(
                        ObjectRights::new(object_type, mask),
                        Err(RightsError::InvalidMask)
                    );
                }
            }
        }
        assert_eq!(
            ObjectRights::decode(0, 0),
            Err(RightsError::UnknownObjectType)
        );
    }

    #[test]
    fn typed_comparison_and_attenuation() {
        assert_eq!(
            endpoint::SEND.contains(frame::READ),
            Err(RightsError::CrossObjectType)
        );
        let parent = ObjectRights::new(ObjectType::Frame, 3).unwrap();
        let child = ObjectRights::new(ObjectType::Frame, 1).unwrap();
        assert_eq!(parent.attenuate(child), Ok(child));
        let expanded = ObjectRights::new(ObjectType::Frame, 15).unwrap();
        assert_eq!(
            child.attenuate(expanded),
            Err(RightsError::RightsExpansion)
        );
    }
}
