//! Audio/video synchronization policy.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction { Present, Hold, Drop }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvSyncPolicy { pub hold_threshold_us: u64, pub drop_threshold_us: u64 }

impl Default for AvSyncPolicy { fn default() -> Self { Self { hold_threshold_us: 20_000, drop_threshold_us: 80_000 } } }

impl AvSyncPolicy {
    pub fn action(&self, audio_pts_us: u64, video_pts_us: u64) -> SyncAction {
        let delta = video_pts_us as i128 - audio_pts_us as i128;
        if delta > self.hold_threshold_us as i128 { SyncAction::Hold }
        else if delta < -(self.drop_threshold_us as i128) { SyncAction::Drop }
        else { SyncAction::Present }
    }
}
