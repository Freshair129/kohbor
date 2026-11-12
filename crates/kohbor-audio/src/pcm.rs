//! In-memory PCM for prewarmed critical media.

use std::sync::Arc;

/// Mono 48,000 Hz signed 16-bit samples.
pub type Pcm = Arc<[i16]>;

/// Decode the package WAV profile (RIFF/WAVE, PCM, mono, 48,000 Hz, 16-bit, one
/// `fmt ` and one `data` chunk, as accepted by `kohbor_pack::media::inspect_wav`)
/// into samples. `None` for anything else (never panics; bounded by the slice).
pub fn decode_wav(bytes: &[u8]) -> Option<Pcm> {
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }

    let riff_size = usize::try_from(read_u32_le(bytes, 4)?).ok()?;
    let riff_end = 8usize.checked_add(riff_size)?;
    if riff_end < 12 || riff_end > bytes.len() {
        return None;
    }
    let exact_length = riff_end == bytes.len();
    let trailing_pad = riff_end.checked_add(1) == Some(bytes.len()) && bytes.last() == Some(&0);
    if !exact_length && !trailing_pad {
        return None;
    }

    let fmt_header_end = 12usize.checked_add(8)?;
    if fmt_header_end > riff_end || bytes.get(12..16)? != b"fmt " {
        return None;
    }
    if read_u32_le(bytes, 16)? != 16 {
        return None;
    }
    let fmt_start = fmt_header_end;
    let fmt_end = fmt_start.checked_add(16)?;
    if fmt_end > riff_end {
        return None;
    }
    if read_u16_le(bytes, fmt_start)? != 1
        || read_u16_le(bytes, fmt_start.checked_add(2)?)? != 1
        || read_u32_le(bytes, fmt_start.checked_add(4)?)? != 48_000
        || read_u32_le(bytes, fmt_start.checked_add(8)?)? != 96_000
        || read_u16_le(bytes, fmt_start.checked_add(12)?)? != 2
        || read_u16_le(bytes, fmt_start.checked_add(14)?)? != 16
    {
        return None;
    }

    let data_header_end = fmt_end.checked_add(8)?;
    let data_id_end = fmt_end.checked_add(4)?;
    if data_header_end > riff_end || bytes.get(fmt_end..data_id_end)? != b"data" {
        return None;
    }
    let data_size = read_u32_le(bytes, fmt_end.checked_add(4)?)?;
    if data_size % 2 != 0 {
        return None;
    }
    let data_start = data_header_end;
    let data_len = usize::try_from(data_size).ok()?;
    let data_end = data_start.checked_add(data_len)?;
    let padded_end = data_end.checked_add(1);
    let has_data_pad = padded_end == Some(riff_end) && bytes.get(data_end) == Some(&0);
    if data_end != riff_end && !has_data_pad {
        return None;
    }

    let sample_count = usize::try_from(data_size / 2).ok()?;
    let duration_numerator = u64::from(data_size / 2).checked_mul(1_000)?;
    if duration_numerator / 48_000 > 15_000 {
        return None;
    }

    let mut samples = Vec::new();
    samples.try_reserve_exact(sample_count).ok()?;
    for sample_index in 0..sample_count {
        let offset = sample_index.checked_mul(2)?.checked_add(data_start)?;
        samples.push(read_i16_le(bytes, offset)?);
    }
    Some(Arc::from(samples.into_boxed_slice()))
}

/// `samples * 1000 / 48_000`, floor.
pub fn duration_ms(pcm: &[i16]) -> u32 {
    let numerator = (pcm.len() as u128).saturating_mul(1_000);
    let duration = numerator / 48_000;
    duration.min(u128::from(u32::MAX)) as u32
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let raw = <[u8; 2]>::try_from(bytes.get(offset..end)?).ok()?;
    Some(u16::from_le_bytes(raw))
}

fn read_i16_le(bytes: &[u8], offset: usize) -> Option<i16> {
    let end = offset.checked_add(2)?;
    let raw = <[u8; 2]>::try_from(bytes.get(offset..end)?).ok()?;
    Some(i16::from_le_bytes(raw))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let raw = <[u8; 4]>::try_from(bytes.get(offset..end)?).ok()?;
    Some(u32::from_le_bytes(raw))
}
