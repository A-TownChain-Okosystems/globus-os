//! Timed codec playback session: packet timestamps -> frame queue -> sync decision.

use crate::{
    AvSyncPolicy, FrameQueue, MediaError, SyncAction, TimedVideoFrame, VideoCodecDecoder,
    VideoFrame,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
/// Zaehler einer Wiedergabesitzung.
pub struct PlaybackStats {
    /// Eingereichte Pakete.
    pub submitted: u64,
    /// Dekodierte Bilder.
    pub decoded: u64,
    /// Angezeigte Bilder.
    pub presented: u64,
    /// Verworfene Bilder.
    pub dropped: u64,
    /// Gehaltene Bilder.
    pub held: u64,
    /// Neu synchronisierte Stellen.
    pub resyncs: u64,
}

#[derive(Debug)]
/// Verbindet Dekoder, Bildwarteschlange und Synchronisation.
pub struct CodecVideoSession<D> {
    decoder: D,
    queue: FrameQueue,
    sync: AvSyncPolicy,
    stats: PlaybackStats,
    next_audio_pts_us: u64,
}

impl<D: VideoCodecDecoder> CodecVideoSession<D> {
    /// Legt eine Sitzung mit Dekoder, Queue-Kapazitaet und Sync-Politik an.
    pub fn new(decoder: D, queue_capacity: usize, sync: AvSyncPolicy) -> Self {
        Self {
            decoder,
            queue: FrameQueue::new(queue_capacity),
            sync,
            stats: PlaybackStats::default(),
            next_audio_pts_us: 0,
        }
    }

    /// Aktuelle Zaehler.
    pub fn stats(&self) -> PlaybackStats {
        self.stats
    }
    /// Anzahl wartender Bilder.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }
    /// Setzt die Audiuhr, an der sich das Video orientiert.
    pub fn set_audio_clock(&mut self, pts_us: u64) {
        self.next_audio_pts_us = pts_us;
    }

    /// Gibt einen Datenblock an den Dekoder.
    pub fn submit_packet(
        &mut self,
        packet: &[u8],
        pts_us: u64,
        dts_us: u64,
        keyframe: bool,
    ) -> Result<(), MediaError> {
        self.decoder.submit(packet, pts_us, dts_us, keyframe)?;
        self.stats.submitted = self.stats.submitted.saturating_add(1);
        Ok(())
    }

    /// Holt alle fertigen Bilder in die Warteschlange und liefert deren Anzahl.
    pub fn drain_decoder(&mut self) -> Result<usize, MediaError> {
        let mut count = 0;
        while let Some(TimedVideoFrame { frame, pts_us, .. }) = self.decoder.receive()? {
            self.queue.push(pts_us, frame);
            self.stats.decoded = self.stats.decoded.saturating_add(1);
            count += 1;
        }
        Ok(count)
    }

    /// Naechstes Bild, das jetzt gezeigt werden soll.
    pub fn next_for_presentation(&mut self) -> Option<(u64, VideoFrame)> {
        let pts = self.queue.peek_pts()?;
        match self.sync.action(self.next_audio_pts_us, pts) {
            SyncAction::Present => {
                let frame = self.queue.pop()?.1;
                self.stats.presented = self.stats.presented.saturating_add(1);
                Some((pts, frame))
            }
            SyncAction::Hold => {
                self.stats.held = self.stats.held.saturating_add(1);
                None
            }
            SyncAction::Drop => {
                let _ = self.queue.pop();
                self.stats.dropped = self.stats.dropped.saturating_add(1);
                None
            }
            SyncAction::Resync => {
                let _ = self.queue.pop();
                self.stats.resyncs = self.stats.resyncs.saturating_add(1);
                None
            }
        }
    }

    /// Verwirft verspaetete Bilder und liefert deren Anzahl.
    pub fn drop_late(&mut self, threshold_us: u64) -> usize {
        let dropped = self.queue.drop_late(self.next_audio_pts_us, threshold_us);
        self.stats.dropped = self.stats.dropped.saturating_add(dropped as u64);
        dropped
    }
}
