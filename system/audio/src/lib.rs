//! Audio device, media-session, codec, mixing, output, routing, and playback pipeline boundary.

pub mod codec;
/// Routing of audio sessions to output devices.
pub mod device_policy;
/// Player, decoder, mixer and output wired together.
pub mod media_pipeline;
/// Playback state machine and queue.
pub mod media_player;
/// PCM summing with bounded gain.
pub mod mixer;
/// Output device abstraction and ring-buffer backend.
pub mod output;

pub use codec::{AudioDecoder, AudioFormat, AudioSpec, CodecError, PcmFrame};
pub use device_policy::{AudioOutputKind, AudioRoute, AudioRouteTable, RouteError};
pub use media_pipeline::{MediaPipeline, PipelineError};
pub use media_player::{
    MediaPlayer, MediaPlayerError, MediaQueue, MediaTrack, PlaybackState, RepeatMode,
};
pub use mixer::{MixerError, PcmMixer};
pub use output::{AudioOutput, OutputError, RingAudioOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Direction of an audio stream.
pub enum AudioDirection {
    /// Output towards a device.
    Playback,
    /// Input from a microphone or another source.
    Capture,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifier of an audio session.
pub struct AudioSessionId(pub u64);
