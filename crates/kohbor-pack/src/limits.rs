//! Package limits (MEDIA-PACK-SPEC MP-03, MP-04). `Limits::default()` is the
//! specified production limit; tests inject smaller limits to keep hostile
//! fixtures small. A limit larger than the specified maximum is never accepted
//! by [`Limits::clamped`].

pub const MIB: u64 = 1024 * 1024;

pub const MAX_CONTAINER_BYTES: u64 = 512 * MIB;
pub const MAX_EXPANDED_BYTES: u64 = 512 * MIB;
pub const MAX_PAYLOAD_BYTES: u64 = 32 * MIB;
pub const MAX_DESCRIPTOR_BYTES: u64 = 2 * MIB;
pub const MAX_PAYLOAD_COUNT: usize = 2048;
pub const MAX_BINDINGS: usize = 128;
pub const MAX_AUDIO_MS: u32 = 15_000;
pub const MAX_VISUAL_TAIL_MS: u32 = 3_200;
/// Required mixed-audio sample rate (EVENT-AUDIO-SPEC section 6).
pub const AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub container_bytes: u64,
    /// Aggregate of all expanded entries including the descriptor.
    pub expanded_bytes: u64,
    pub payload_bytes: u64,
    pub descriptor_bytes: u64,
    pub payload_count: usize,
    pub bindings: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            container_bytes: MAX_CONTAINER_BYTES,
            expanded_bytes: MAX_EXPANDED_BYTES,
            payload_bytes: MAX_PAYLOAD_BYTES,
            descriptor_bytes: MAX_DESCRIPTOR_BYTES,
            payload_count: MAX_PAYLOAD_COUNT,
            bindings: MAX_BINDINGS,
        }
    }
}

impl Limits {
    /// Every field is the minimum of the given value and the specified maximum.
    pub fn clamped(self) -> Limits {
        todo!()
    }
}
