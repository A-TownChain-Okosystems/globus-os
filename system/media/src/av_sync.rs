//! Audio/video synchronization policy with bounded resynchronization.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Entscheidung fuer das naechste Bild.
pub enum SyncAction {
    /// Bild zeigen; der Versatz liegt im Toleranzband.
    Present,
    /// Bild halten; das Video ist dem Audio voraus.
    Hold,
    /// Bild verwerfen; das Video haengt zurueck.
    Drop,
    /// Neu synchronisieren; der Versatz ist zu gross.
    Resync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Schwellen, ab denen Audio und Video gegeneinander korrigiert werden.
pub struct AvSyncPolicy {
    /// Ab diesem Versatz wird das Bild gehalten.
    pub hold_threshold_us: u64,
    /// Ab diesem Versatz wird das Bild verworfen.
    pub drop_threshold_us: u64,
    /// Ab diesem Versatz wird neu synchronisiert.
    pub resync_threshold_us: u64,
}

impl Default for AvSyncPolicy {
    fn default() -> Self {
        Self {
            hold_threshold_us: 20_000,
            drop_threshold_us: 80_000,
            resync_threshold_us: 250_000,
        }
    }
}

impl AvSyncPolicy {
    /// Entscheidet anhand des Versatzes, was mit dem Bild geschieht.
    pub fn action(&self, audio_pts_us: u64, video_pts_us: u64) -> SyncAction {
        let delta = video_pts_us as i128 - audio_pts_us as i128;
        let magnitude = delta.unsigned_abs() as u64;
        if magnitude >= self.resync_threshold_us {
            SyncAction::Resync
        } else if delta > self.hold_threshold_us as i128 {
            SyncAction::Hold
        } else if delta < -(self.drop_threshold_us as i128) {
            SyncAction::Drop
        } else {
            SyncAction::Present
        }
    }

    /// True, wenn das Video hinter dem Audio liegt.
    pub fn is_late(&self, audio_pts_us: u64, video_pts_us: u64) -> bool {
        self.action(audio_pts_us, video_pts_us) == SyncAction::Drop
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Entscheidung samt den Zeitstempeln, auf denen sie beruht.
pub struct SyncDecision {
    /// Zeitstempel des Audios.
    pub audio_pts_us: u64,
    /// Zeitstempel des Videos.
    pub video_pts_us: u64,
    /// Getroffene Entscheidung.
    pub action: SyncAction,
}

impl AvSyncPolicy {
    /// Liefert die Entscheidung zusammen mit ihren Eingaben.
    pub fn decide(&self, audio_pts_us: u64, video_pts_us: u64) -> SyncDecision {
        SyncDecision {
            audio_pts_us,
            video_pts_us,
            action: self.action(audio_pts_us, video_pts_us),
        }
    }
}
