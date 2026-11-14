//! Playback pipeline coordinating the player, decoder, mixer, and output.

use crate::{
    AudioDecoder, AudioOutput, CodecError, MediaPlayer, MixerError, OutputError, PcmMixer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Failures of the playback pipeline.
pub enum PipelineError {
    /// The queue has no current track.
    NoTrack,
    /// The decoder failed.
    Decoder(CodecError),
    /// The mixer failed.
    Mixer(MixerError),
    /// The output failed.
    Output(OutputError),
}

impl From<CodecError> for PipelineError {
    fn from(e: CodecError) -> Self {
        Self::Decoder(e)
    }
}
impl From<MixerError> for PipelineError {
    fn from(e: MixerError) -> Self {
        Self::Mixer(e)
    }
}
impl From<OutputError> for PipelineError {
    fn from(e: OutputError) -> Self {
        Self::Output(e)
    }
}

/// Player, decoder, mixer and output as one unit.
pub struct MediaPipeline<D, O> {
    /// Player owning queue, position and volume.
    pub player: MediaPlayer,
    decoder: D,
    mixer: PcmMixer,
    output: O,
}

impl<D: AudioDecoder, O: AudioOutput> MediaPipeline<D, O> {
    /// Wires the parts together with a fresh mixer.
    pub fn new(player: MediaPlayer, decoder: D, output: O) -> Self {
        Self {
            player,
            decoder,
            mixer: PcmMixer::new(),
            output,
        }
    }

    /// Mixer of the pipeline.
    pub fn mixer(&self) -> &PcmMixer {
        &self.mixer
    }
    /// Mixer of the pipeline, mutable.
    pub fn mixer_mut(&mut self) -> &mut PcmMixer {
        &mut self.mixer
    }
    /// Output of the pipeline.
    pub fn output(&self) -> &O {
        &self.output
    }
    /// Output of the pipeline, mutable.
    pub fn output_mut(&mut self) -> &mut O {
        &mut self.output
    }

    /// Opens the output with the decoder spec.
    pub fn open(&mut self) -> Result<(), PipelineError> {
        self.output.open(self.decoder.spec())?;
        Ok(())
    }

    /// Starts the output and the player.
    pub fn start(&mut self) -> Result<(), PipelineError> {
        self.output.start()?;
        self.player.play().map_err(|_| PipelineError::NoTrack)
    }

    /// Decodes, mixes and writes at most `max_frames` frames.
    pub fn pump(&mut self, max_frames: usize) -> Result<usize, PipelineError> {
        if self.player.queue().current().is_none() {
            return Err(PipelineError::NoTrack);
        }
        let frame = self.decoder.decode(max_frames)?;
        let frames = frame.frames();
        if frames == 0 {
            return Ok(0);
        }
        let mixed = self.mixer.mix(frame.spec, &[frame])?;
        self.output.write(&mixed)?;
        Ok(frames)
    }

    /// Stops the output and the player.
    pub fn stop(&mut self) -> Result<(), PipelineError> {
        self.output.stop()?;
        self.player.stop();
        Ok(())
    }
}
