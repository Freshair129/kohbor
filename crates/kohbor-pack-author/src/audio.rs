use std::convert::TryFrom;

const TARGET_RATE: u32 = 48_000;
const MAX_SAMPLES: usize = 48_000 * 15;
const SINC_RADIUS: i64 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedAudio {
    pub wav: Vec<u8>,
    pub samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AudioError {}

/// Decode PCM/IEEE-float RIFF WAV and convert it to mono 48 kHz PCM16.
/// Resampling uses a 48-tap Hann-windowed sinc low-pass, with a reduced cutoff
/// when downsampling. Output length is the nearest integer source duration.
pub fn convert_wav(bytes: &[u8]) -> Result<ConvertedAudio, AudioError> {
    let input = decode_wav(bytes)?;
    let mono = match input.channels {
        1 => input.samples,
        2 => input
            .samples
            .chunks_exact(2)
            .map(|pair| (pair[0] + pair[1]) * 0.5)
            .collect(),
        _ => {
            return Err(AudioError(
                "only mono or stereo WAV is supported".to_owned(),
            ))
        }
    };
    if mono.is_empty() {
        return Err(AudioError("WAV data chunk is empty".to_owned()));
    }

    let out_len = output_length(mono.len(), input.sample_rate)?;
    if out_len == 0 {
        return Err(AudioError("converted WAV has no samples".to_owned()));
    }
    if out_len > MAX_SAMPLES {
        return Err(AudioError("converted audio exceeds 15000 ms".to_owned()));
    }

    let converted = if input.sample_rate == TARGET_RATE {
        mono.into_iter().map(to_pcm16).collect()
    } else {
        resample(&mono, input.sample_rate, out_len)
    };
    let wav = encode_pcm16(&converted)?;
    Ok(ConvertedAudio {
        wav,
        samples: converted.len(),
    })
}

struct DecodedWav {
    channels: u16,
    sample_rate: u32,
    samples: Vec<f64>,
}

fn decode_wav(bytes: &[u8]) -> Result<DecodedWav, AudioError> {
    if bytes.get(..4) != Some(b"RIFF".as_slice()) || bytes.get(8..12) != Some(b"WAVE".as_slice()) {
        return Err(AudioError("not a RIFF/WAVE file".to_owned()));
    }
    let riff_len = usize::try_from(read_u32(bytes, 4)?).map_err(|_| invalid_wav())?;
    let riff_end = 8usize.checked_add(riff_len).ok_or_else(invalid_wav)?;
    if riff_end < 12 || riff_end != bytes.len() {
        return Err(AudioError(
            "RIFF size does not match file length".to_owned(),
        ));
    }

    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut position = 12usize;
    while position < riff_end {
        let header_end = position.checked_add(8).ok_or_else(invalid_wav)?;
        let header = bytes.get(position..header_end).ok_or_else(invalid_wav)?;
        let chunk_size =
            usize::try_from(read_u32(bytes, position + 4)?).map_err(|_| invalid_wav())?;
        let chunk_start = header_end;
        let chunk_end = chunk_start
            .checked_add(chunk_size)
            .ok_or_else(invalid_wav)?;
        let padded_end = chunk_end
            .checked_add(chunk_size & 1)
            .ok_or_else(invalid_wav)?;
        if padded_end > riff_end {
            return Err(AudioError("WAV chunk exceeds RIFF boundary".to_owned()));
        }
        match header.get(..4) {
            Some(b"fmt ") => {
                if fmt.is_some() || chunk_size < 16 {
                    return Err(AudioError("WAV must have one valid fmt chunk".to_owned()));
                }
                let format = read_u16(bytes, chunk_start)?;
                let channels = read_u16(bytes, chunk_start + 2)?;
                let sample_rate = read_u32(bytes, chunk_start + 4)?;
                let block_align = read_u16(bytes, chunk_start + 12)?;
                let bits = read_u16(bytes, chunk_start + 14)?;
                if !matches!(channels, 1 | 2) || sample_rate == 0 {
                    return Err(AudioError(
                        "WAV must be mono or stereo with a nonzero sample rate".to_owned(),
                    ));
                }
                let valid_format = match format {
                    1 => matches!(bits, 8 | 16 | 24),
                    3 => bits == 32,
                    _ => false,
                };
                if !valid_format {
                    return Err(AudioError(
                        "unsupported WAV encoding or sample width".to_owned(),
                    ));
                }
                let bytes_per_sample = usize::from(bits / 8);
                let expected_align = usize::from(channels)
                    .checked_mul(bytes_per_sample)
                    .ok_or_else(invalid_wav)?;
                if usize::from(block_align) != expected_align {
                    return Err(AudioError("WAV block alignment is inconsistent".to_owned()));
                }
                fmt = Some((format, channels, sample_rate, bits));
            }
            Some(b"data") => {
                if data.is_some() {
                    return Err(AudioError("WAV must have one data chunk".to_owned()));
                }
                data = Some(bytes.get(chunk_start..chunk_end).ok_or_else(invalid_wav)?);
            }
            _ => {}
        }
        position = padded_end;
    }
    let (format, channels, sample_rate, bits) =
        fmt.ok_or_else(|| AudioError("WAV fmt chunk is missing".to_owned()))?;
    let data = data.ok_or_else(|| AudioError("WAV data chunk is missing".to_owned()))?;
    let bytes_per_sample = usize::from(bits / 8);
    let frame_bytes = usize::from(channels)
        .checked_mul(bytes_per_sample)
        .ok_or_else(invalid_wav)?;
    if data.is_empty() || data.len() % frame_bytes != 0 {
        return Err(AudioError(
            "WAV data is empty or has an incomplete sample frame".to_owned(),
        ));
    }

    let frame_count = data.len() / frame_bytes;
    let converted_count = output_length(frame_count, sample_rate)?;
    if converted_count == 0 {
        return Err(AudioError("converted WAV has no samples".to_owned()));
    }
    if converted_count > MAX_SAMPLES {
        return Err(AudioError("converted audio exceeds 15000 ms".to_owned()));
    }

    let sample_count = frame_count
        .checked_mul(usize::from(channels))
        .ok_or_else(invalid_wav)?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(sample_count)
        .map_err(|_| AudioError("could not allocate WAV decode buffer".to_owned()))?;
    for sample_bytes in data.chunks_exact(bytes_per_sample) {
        let sample = match (format, bits) {
            (1, 8) => (f64::from(sample_bytes[0]) - 128.0) / 128.0,
            (1, 16) => {
                let raw = i16::from_le_bytes([sample_bytes[0], sample_bytes[1]]);
                f64::from(raw) / 32_768.0
            }
            (1, 24) => {
                let low = i32::from(sample_bytes[0]);
                let middle = i32::from(sample_bytes[1]) << 8;
                let high = i32::from(sample_bytes[2]) << 16;
                let raw = (low | middle | high) << 8 >> 8;
                f64::from(raw) / 8_388_608.0
            }
            (3, 32) => {
                let raw = f32::from_le_bytes([
                    sample_bytes[0],
                    sample_bytes[1],
                    sample_bytes[2],
                    sample_bytes[3],
                ]);
                if !raw.is_finite() {
                    return Err(AudioError(
                        "WAV contains a non-finite float sample".to_owned(),
                    ));
                }
                f64::from(raw)
            }
            _ => return Err(AudioError("unsupported WAV encoding".to_owned())),
        };
        samples.push(sample);
    }
    Ok(DecodedWav {
        channels,
        sample_rate,
        samples,
    })
}

fn output_length(input_len: usize, input_rate: u32) -> Result<usize, AudioError> {
    let len = u128::try_from(input_len).map_err(|_| invalid_wav())?;
    let numerator = len
        .checked_mul(u128::from(TARGET_RATE))
        .ok_or_else(invalid_wav)?;
    let rounded = numerator
        .checked_add(u128::from(input_rate / 2))
        .ok_or_else(invalid_wav)?
        / u128::from(input_rate);
    usize::try_from(rounded)
        .map_err(|_| AudioError("converted audio length is too large".to_owned()))
}

fn resample(input: &[f64], input_rate: u32, output_len: usize) -> Vec<i16> {
    let ratio = f64::from(TARGET_RATE) / f64::from(input_rate);
    let cutoff = ratio.min(1.0) * 0.95;
    let mut output = Vec::with_capacity(output_len);
    for output_index in 0..output_len {
        let source_position = output_index as f64 / ratio;
        let center = source_position.floor() as i64;
        let mut weighted = 0.0f64;
        let mut weight_sum = 0.0f64;
        for offset in -SINC_RADIUS + 1..=SINC_RADIUS {
            let input_index = center + offset;
            if input_index < 0 {
                continue;
            }
            let Some(sample) = usize::try_from(input_index)
                .ok()
                .and_then(|index| input.get(index))
            else {
                continue;
            };
            let distance = source_position - input_index as f64;
            let window_position = distance / SINC_RADIUS as f64;
            if window_position.abs() >= 1.0 {
                continue;
            }
            let window = 0.5 * (1.0 + (std::f64::consts::PI * window_position).cos());
            let argument = distance * cutoff;
            let sinc = if argument.abs() < f64::EPSILON {
                1.0
            } else {
                (std::f64::consts::PI * argument).sin() / (std::f64::consts::PI * argument)
            };
            let weight = cutoff * sinc * window;
            weighted += *sample * weight;
            weight_sum += weight;
        }
        let value = if weight_sum.abs() > f64::EPSILON {
            weighted / weight_sum
        } else {
            0.0
        };
        output.push(to_pcm16(value));
    }
    output
}

fn to_pcm16(sample: f64) -> i16 {
    let clipped = sample.clamp(-1.0, 1.0);
    let scaled = if clipped >= 1.0 {
        32_767.0
    } else {
        clipped * 32_768.0
    };
    scaled.round() as i16
}

fn encode_pcm16(samples: &[i16]) -> Result<Vec<u8>, AudioError> {
    let data_len = samples.len().checked_mul(2).ok_or_else(invalid_wav)?;
    let data_len_u32 = u32::try_from(data_len).map_err(|_| invalid_wav())?;
    let riff_len = 36u32.checked_add(data_len_u32).ok_or_else(invalid_wav)?;
    let mut bytes = Vec::new();
    let capacity = data_len.checked_add(44).ok_or_else(invalid_wav)?;
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| AudioError("could not allocate converted WAV".to_owned()))?;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_len.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&TARGET_RATE.to_le_bytes());
    bytes.extend_from_slice(&96_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len_u32.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(bytes)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, AudioError> {
    let pair = bytes
        .get(offset..offset.checked_add(2).ok_or_else(invalid_wav)?)
        .ok_or_else(invalid_wav)?;
    let first = *pair.first().ok_or_else(invalid_wav)?;
    let second = *pair.get(1).ok_or_else(invalid_wav)?;
    Ok(u16::from_le_bytes([first, second]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, AudioError> {
    let slice = bytes
        .get(offset..offset.checked_add(4).ok_or_else(invalid_wav)?)
        .ok_or_else(invalid_wav)?;
    let first = *slice.first().ok_or_else(invalid_wav)?;
    let second = *slice.get(1).ok_or_else(invalid_wav)?;
    let third = *slice.get(2).ok_or_else(invalid_wav)?;
    let fourth = *slice.get(3).ok_or_else(invalid_wav)?;
    Ok(u32::from_le_bytes([first, second, third, fourth]))
}

fn invalid_wav() -> AudioError {
    AudioError("invalid or truncated WAV".to_owned())
}
