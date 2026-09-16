//! Audio device, media-session, codec, mixing, output, and playback pipeline boundary.

pub mod codec;
pub mod media_player;
pub mod media_pipeline;
pub mod mixer;
pub mod output;

pub use codec::{AudioDecoder, AudioFormat, AudioSpec, CodecError, PcmFrame};
pub use media_player::{MediaPlayer, MediaPlayerError, MediaQueue, MediaTrack, PlaybackState, RepeatMode};
pub use media_pipeline::{MediaPipeline, PipelineError};
pub use mixer::{MixerError, PcmMixer};
pub use output::{AudioOutput, OutputError, RingAudioOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDirection { Playback, Capture }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioSessionId(pub u64);
