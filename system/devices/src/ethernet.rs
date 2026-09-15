//! Ethernet NIC DMA ring contracts.
//!
//! Hardware-specific descriptor formats stay in the NIC driver. This layer
//! defines the ownership and buffer invariants shared with the network stack.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError { Empty, TooLarge, InvalidBuffer, DeviceOwnsBuffer }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaBuffer { pub address: u64, pub length: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxDescriptor { pub buffer: DmaBuffer, pub length: u16, pub owned_by_device: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxDescriptor { pub buffer: DmaBuffer, pub capacity: u16, pub owned_by_device: bool }

pub const MAX_FRAME_SIZE: u16 = 1518;

pub fn validate_tx(desc: TxDescriptor) -> Result<(), RingError> {
    if desc.buffer.length == 0 || desc.length == 0 || desc.length as u32 > desc.buffer.length || desc.length > MAX_FRAME_SIZE {
        return Err(RingError::InvalidBuffer);
    }
    if desc.owned_by_device { return Err(RingError::DeviceOwnsBuffer); }
    Ok(())
}

pub fn validate_rx(desc: RxDescriptor) -> Result<(), RingError> {
    if desc.buffer.length == 0 || desc.capacity == 0 || desc.capacity as u32 > desc.buffer.length || desc.capacity > MAX_FRAME_SIZE {
        return Err(RingError::InvalidBuffer);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn tx_cannot_be_reused_while_device_owns_it() {
        let d = TxDescriptor { buffer: DmaBuffer { address: 0x1000, length: 1500 }, length: 1500, owned_by_device: true };
        assert_eq!(validate_tx(d), Err(RingError::DeviceOwnsBuffer));
    }
}