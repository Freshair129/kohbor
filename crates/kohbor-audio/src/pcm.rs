//! In-memory PCM for prewarmed critical media.

use std::sync::Arc;

/// Mono 48,000 Hz signed 16-bit samples.
pub type Pcm = Arc<[i16]>;

/// Decode the package WAV profile (RIFF/WAVE, PCM, mono, 48,000 Hz, 16-bit, one
/// `fmt ` and one `data` chunk, as accepted by `kohbor_pack::media::inspect_wav`)
/// into samples. `None` for anything else (never panics; bounded by the slice).
pub fn decode_wav(bytes: &[u8]) -> Option<Pcm> {
    todo!()
}

/// `samples * 1000 / 48_000`, floor.
pub fn duration_ms(pcm: &[i16]) -> u32 {
    todo!()
}
