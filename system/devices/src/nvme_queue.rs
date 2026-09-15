//! NVMe submission/completion queue and PRP command primitives.

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NvmeCommand {
    pub opcode: u8,
    pub flags: u8,
    pub command_id: u16,
    pub namespace_id: u32,
    pub reserved: u64,
    pub metadata: u64,
    pub prp1: u64,
    pub prp2: u64,
    pub command_specific: [u32; 6],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NvmeCompletion {
    pub result: u32,
    pub reserved: u32,
    pub sq_head: u16,
    pub sq_id: u16,
    pub command_id: u16,
    pub status: u16,
}

impl NvmeCompletion {
    pub fn phase(self) -> bool {
        self.status & 1 != 0
    }

    pub fn status_code(self) -> u16 {
        (self.status >> 1) & 0x7ff
    }

    pub fn success(self) -> bool {
        self.status_code() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrpMapping {
    pub prp1: u64,
    pub prp2: u64,
    pub pages: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrpError {
    Empty,
    Unaligned,
    TooManyPages,
}

/// Build the first two PRP entries for a physically contiguous request.
///
/// The caller must provide physical addresses belonging to an IOMMU domain
/// already authorized for the NVMe controller. Requests requiring a PRP list
/// are rejected here until a physically allocated PRP-list page is supplied.
pub fn map_contiguous(physical: u64, length: usize) -> Result<PrpMapping, PrpError> {
    if length == 0 {
        return Err(PrpError::Empty);
    }
    if physical & 3 != 0 {
        return Err(PrpError::Unaligned);
    }
    let first_offset = (physical & 0xfff) as usize;
    let pages = (first_offset + length).div_ceil(4096) as u32;
    if pages > 2 {
        return Err(PrpError::TooManyPages);
    }
    let prp2 = if pages == 2 { (physical & !0xfff) + 4096 } else { 0 };
    Ok(PrpMapping { prp1: physical, prp2, pages })
}

#[derive(Debug, Clone, Copy)]
pub struct QueueState {
    pub depth: u16,
    pub submission_tail: u16,
    pub completion_head: u16,
    pub phase: bool,
}

impl QueueState {
    pub fn new(depth: u16) -> Self {
        assert!(depth >= 2);
        Self { depth, submission_tail: 0, completion_head: 0, phase: true }
    }

    pub fn advance_submission(&mut self) {
        self.submission_tail = (self.submission_tail + 1) % self.depth;
    }

    pub fn advance_completion(&mut self) {
        self.completion_head = (self.completion_head + 1) % self.depth;
        if self.completion_head == 0 {
            self.phase = !self.phase;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_one_and_two_page_requests() {
        assert_eq!(map_contiguous(0x1000, 4096).unwrap().pages, 1);
        assert_eq!(map_contiguous(0x1ff0, 32).unwrap().pages, 2);
    }

    #[test]
    fn rejects_requests_needing_prp_list() {
        assert_eq!(map_contiguous(0x1000, 8193), Err(PrpError::TooManyPages));
    }
}
