use kohbor_audio::pcm::Pcm;
use kohbor_events::ObjectiveId;
use std::f64::consts::TAU;
use std::sync::Arc;

const SAMPLE_RATE: usize = 48_000;
const BLIP_MS: usize = 62;
const GAP_MS: usize = 34;
const FADE_MS: usize = 8;
const AMPLITUDE: f64 = 0.055;

/// Generate a short, quiet cue locally. Each objective has its own pitch pattern.
pub(crate) fn synthesize(objective: ObjectiveId) -> Pcm {
    let (frequencies, count) = match objective {
        ObjectiveId::WaterRune => ([440.0, 554.37, 0.0], 2),
        ObjectiveId::PowerRune => ([587.33, 783.99, 0.0], 2),
        ObjectiveId::BountyRune => ([880.0, 880.0, 0.0], 2),
        ObjectiveId::WisdomRune => ([659.25, 783.99, 987.77], 3),
        ObjectiveId::Tormentor => ([329.63, 392.0, 493.88], 3),
    };
    let mut samples = Vec::with_capacity(count * BLIP_MS * SAMPLE_RATE / 1_000);
    for (index, frequency) in frequencies.into_iter().take(count).enumerate() {
        if index > 0 {
            samples.extend(std::iter::repeat(0).take(GAP_MS * SAMPLE_RATE / 1_000));
        }
        samples.extend(blip(frequency));
    }
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
    use super::{synthesize, AMPLITUDE};
    use kohbor_audio::pcm::duration_ms;
    use kohbor_events::ObjectiveId;

    #[test]
    fn every_objective_has_a_distinct_soft_short_synthesized_cue() {
        let objectives = [
            ObjectiveId::WaterRune,
            ObjectiveId::PowerRune,
            ObjectiveId::BountyRune,
            ObjectiveId::WisdomRune,
            ObjectiveId::Tormentor,
        ];
        let cues: Vec<_> = objectives.into_iter().map(synthesize).collect();
        for cue in &cues {
            assert!(duration_ms(cue) < 400);
            let peak = cue
                .iter()
                .map(|sample| sample.unsigned_abs())
                .max()
                .unwrap();
            assert!(peak <= (AMPLITUDE * i16::MAX as f64) as u16 + 1);
            assert_eq!(cue.first(), Some(&0));
            assert_eq!(cue.last(), Some(&0));
        }
        for left in 0..cues.len() {
            for right in left + 1..cues.len() {
                assert_ne!(cues[left].as_ref(), cues[right].as_ref());
            }
        }
    }
}
