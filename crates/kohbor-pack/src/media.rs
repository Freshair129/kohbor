//! Media profile inspection on in-memory bytes (EVENT-AUDIO-SPEC section 6).
//! Native checks only: WAV is fully decoded to count samples; WebM is parsed as
//! EBML/Matroska for its track list and duration without decoding any codec.
//! The renderer decode probe for video is a separate, later step and is not
//! part of this crate.

use crate::error::RejectReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    pub samples: u64,
    /// `samples * 1000 / 48_000`, floor.
    pub duration_ms: u32,
}

/// RIFF/WAVE, `WAVE_FORMAT_PCM` (1), mono, 48,000 Hz, 16-bit, exactly one `fmt `
/// and one `data` chunk (extra chunks are rejected, a trailing odd pad byte is
/// allowed), chunk sizes consistent with the byte length, data length a multiple
/// of 2, duration <= 15,000 ms. The error is `kind` (`MixedAudioInvalid` or
/// `IdentityAudioInvalid`) so one function serves both roles.
pub fn inspect_wav(bytes: &[u8], kind: RejectReason) -> Result<WavInfo, RejectReason> {
    todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    Vp8,
    Vp9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebmInfo {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    /// Segment duration in ms from `Duration` x `TimecodeScale`; `None` when the
    /// file does not declare one.
    pub duration_ms: Option<u64>,
}

/// EBML header with DocType `webm`, a Segment with Tracks holding exactly one
/// video track (`V_VP9` or `V_VP8`, width and height 1..=1920 x 1..=1080) and no
/// audio track. A track of any other type (audio, subtitle, ...) makes the file
/// `VisualHasAudioTrack` when it is audio, otherwise `VisualInvalid`. Truncated or
/// inconsistent element sizes -> `VisualInvalid`. The 60 fps ceiling is checked
/// only when the track declares `DefaultDuration`.
pub fn inspect_webm(bytes: &[u8]) -> Result<WebmInfo, RejectReason> {
    todo!()
}
