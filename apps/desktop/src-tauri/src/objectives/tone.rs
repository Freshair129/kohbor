use kohbor_audio::pcm::Pcm;
use std::f64::consts::TAU;
use std::sync::Arc;

const SAMPLE_RATE: usize = 48_000;
const BLIP_MS: usize = 90;
const GAP_MS: usize = 70;
const FADE_MS: usize = 10;
const AMPLITUDE: f64 = 0.055;

/// Synthesize a quiet 260 ms two-note cue with a short fade at each edge.
pub(crate) fn synthesize_soft_beep_beep() -> Pcm {
    let mut samples = blip(880.0);
    samples.extend(std::iter::repeat(0).take(GAP_MS * SAMPLE_RATE / 1_000));
    samples.extend(blip(1_046.5));
    Arc::from(samples.into_boxed_slice())
}

fn blip(frequency_hz: f64) -> Vec<i16> {
    let count = BLIP_MS * SAMPLE_RATE / 1_000;
    let fade = FADE_MS * SAMPLE_RATE / 1_000;
    (0..count)
        .map(|index| {
            let edge = index.min(count.saturating_sub(1).saturating_sub(index));
            let envelope = (edge as f64 / fade as f64).clamp(0.0, 1.0);
            let phase = TAU * frequency_hz * index as f64 / SAMPLE_RATE as f64;
            (phase.sin() * envelope * AMPLITUDE * i16::MAX as f64) as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{synthesize_soft_beep_beep, AMPLITUDE, SAMPLE_RATE};
    use kohbor_audio::pcm::duration_ms;

    #[test]
    fn cue_is_short_soft_and_fades_in_and_out() {
        let pcm = synthesize_soft_beep_beep();
        assert_eq!(duration_ms(&pcm), 250);
        assert_eq!(pcm.first(), Some(&0));
        assert_eq!(pcm.last(), Some(&0));
        let peak = pcm
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap();
        assert!(peak <= (AMPLITUDE * i16::MAX as f64) as u16 + 1);
        assert!(pcm[SAMPLE_RATE * 90 / 1_000..SAMPLE_RATE * 160 / 1_000]
            .iter()
            .all(|sample| *sample == 0));
    }
}
