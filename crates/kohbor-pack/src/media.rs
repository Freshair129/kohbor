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
    let invalid = || kind.clone();
    if bytes.get(..4) != Some(b"RIFF".as_slice()) || bytes.get(8..12) != Some(b"WAVE".as_slice()) {
        return Err(invalid());
    }
    let riff_size = read_u32_le(bytes, 4).ok_or_else(invalid)?;
    let riff_end = 8usize
        .checked_add(usize::try_from(riff_size).map_err(|_| invalid())?)
        .ok_or_else(invalid)?;
    if riff_end < 12 || riff_end > bytes.len() {
        return Err(invalid());
    }
    let length_matches = riff_end == bytes.len();
    let has_trailing_pad = riff_end.checked_add(1) == Some(bytes.len()) && bytes.last() == Some(&0);
    if !length_matches && !has_trailing_pad {
        return Err(invalid());
    }

    // This profile admits exactly fmt followed by data; all other chunks are
    // rejected so their contents cannot hide another stream or metadata payload.
    let fmt_header_end = 12usize.checked_add(8).ok_or_else(invalid)?;
    if bytes.get(12..16) != Some(b"fmt ".as_slice()) || fmt_header_end > riff_end {
        return Err(invalid());
    }
    let fmt_size = read_u32_le(bytes, 16).ok_or_else(invalid)?;
    if fmt_size != 16 {
        return Err(invalid());
    }
    let fmt_start = fmt_header_end;
    let fmt_end = fmt_start.checked_add(16).ok_or_else(invalid)?;
    if fmt_end > riff_end {
        return Err(invalid());
    }
    let format = read_u16_le(bytes, fmt_start).ok_or_else(invalid)?;
    let channels =
        read_u16_le(bytes, fmt_start.checked_add(2).ok_or_else(invalid)?).ok_or_else(invalid)?;
    let sample_rate =
        read_u32_le(bytes, fmt_start.checked_add(4).ok_or_else(invalid)?).ok_or_else(invalid)?;
    let byte_rate =
        read_u32_le(bytes, fmt_start.checked_add(8).ok_or_else(invalid)?).ok_or_else(invalid)?;
    let block_align =
        read_u16_le(bytes, fmt_start.checked_add(12).ok_or_else(invalid)?).ok_or_else(invalid)?;
    let bits_per_sample =
        read_u16_le(bytes, fmt_start.checked_add(14).ok_or_else(invalid)?).ok_or_else(invalid)?;
    if format != 1
        || channels != 1
        || sample_rate != 48_000
        || byte_rate != 96_000
        || block_align != 2
        || bits_per_sample != 16
    {
        return Err(invalid());
    }

    let data_header_end = fmt_end.checked_add(8).ok_or_else(invalid)?;
    let data_id_end = fmt_end.checked_add(4).ok_or_else(invalid)?;
    if data_header_end > riff_end || bytes.get(fmt_end..data_id_end) != Some(b"data".as_slice()) {
        return Err(invalid());
    }
    let data_size_offset = fmt_end.checked_add(4).ok_or_else(invalid)?;
    let data_size = read_u32_le(bytes, data_size_offset).ok_or_else(invalid)?;
    if data_size % 2 != 0 {
        return Err(invalid());
    }
    let data_start = data_header_end;
    let data_end = data_start
        .checked_add(usize::try_from(data_size).map_err(|_| invalid())?)
        .ok_or_else(invalid)?;
    let chunk_pad_end = data_end.checked_add(1);
    let has_chunk_pad = chunk_pad_end == Some(riff_end) && bytes.get(data_end) == Some(&0);
    if data_end != riff_end && !has_chunk_pad {
        return Err(invalid());
    }
    let samples = u64::from(data_size / 2);
    let duration_ms = samples.checked_mul(1_000).ok_or_else(invalid)? / 48_000;
    let duration_ms = u32::try_from(duration_ms).map_err(|_| invalid())?;
    if duration_ms > 15_000 {
        return Err(invalid());
    }
    Ok(WavInfo {
        samples,
        duration_ms,
    })
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
    match inspect_webm_inner(bytes) {
        Ok(info) => Ok(info),
        Err(WebmError::Invalid) => Err(RejectReason::VisualInvalid),
        Err(WebmError::AudioTrack) => Err(RejectReason::VisualHasAudioTrack),
    }
}

const EBML_HEADER: u32 = 0x1a45_dfa3;
const EBML_DOCTYPE: u32 = 0x4282;
const EBML_SEGMENT: u32 = 0x1853_8067;
const EBML_INFO: u32 = 0x1549_a966;
const EBML_TIMECODE_SCALE: u32 = 0x002a_d7b1;
const EBML_DURATION: u32 = 0x4489;
const EBML_TRACKS: u32 = 0x1654_ae6b;
const EBML_TRACK_ENTRY: u32 = 0xae;
const EBML_TRACK_TYPE: u32 = 0x83;
const EBML_CODEC_ID: u32 = 0x86;
const EBML_VIDEO: u32 = 0xe0;
const EBML_PIXEL_WIDTH: u32 = 0xb0;
const EBML_PIXEL_HEIGHT: u32 = 0xba;
const EBML_DEFAULT_DURATION: u32 = 0x0023_e383;
const DEFAULT_TIMECODE_SCALE: u64 = 1_000_000;
const MAX_EBML_DEPTH: usize = 8;
const MAX_EBML_ELEMENTS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebmError {
    Invalid,
    AudioTrack,
}

#[derive(Debug, Clone, Copy)]
struct EbmlElement {
    id: u32,
    data_start: usize,
    data_end: usize,
    next: usize,
}

#[derive(Default)]
struct ElementBudget {
    count: usize,
}

impl ElementBudget {
    fn consume(&mut self) -> Result<(), WebmError> {
        self.count = self.count.checked_add(1).ok_or(WebmError::Invalid)?;
        if self.count > MAX_EBML_ELEMENTS {
            return Err(WebmError::Invalid);
        }
        Ok(())
    }
}

#[derive(Default)]
struct SegmentInfo {
    timecode_scale: Option<u64>,
    duration: Option<f64>,
}

#[derive(Default)]
struct VideoTrack {
    track_type: Option<u64>,
    codec_id_range: Option<(usize, usize)>,
    width: Option<u64>,
    height: Option<u64>,
    default_duration: Option<u64>,
}

fn inspect_webm_inner(bytes: &[u8]) -> Result<WebmInfo, WebmError> {
    let mut budget = ElementBudget::default();
    let mut position = 0usize;
    let mut header_seen = false;
    let mut segment_seen = false;
    let mut info = SegmentInfo::default();
    let mut video: Option<VideoTrack> = None;

    while position < bytes.len() {
        let element = read_element(bytes, position, bytes.len(), 0, &mut budget)?;
        if !header_seen {
            if element.id != EBML_HEADER {
                return Err(WebmError::Invalid);
            }
            parse_header(bytes, element, 1, &mut budget)?;
            header_seen = true;
        } else if element.id == EBML_HEADER {
            return Err(WebmError::Invalid);
        } else if element.id == EBML_SEGMENT {
            if segment_seen {
                return Err(WebmError::Invalid);
            }
            parse_segment(bytes, element, 1, &mut budget, &mut info, &mut video)?;
            segment_seen = true;
        } else {
            // The accepted profile has one EBML header followed by one Segment.
            return Err(WebmError::Invalid);
        }
        position = element.next;
    }

    if !header_seen || !segment_seen {
        return Err(WebmError::Invalid);
    }
    let track = video.ok_or(WebmError::Invalid)?;
    if track.track_type != Some(1) {
        return Err(WebmError::Invalid);
    }
    let codec = match track.codec_id_range {
        Some((start, end)) => match bytes.get(start..end) {
            Some(b"V_VP9") => VideoCodec::Vp9,
            Some(b"V_VP8") => VideoCodec::Vp8,
            _ => return Err(WebmError::Invalid),
        },
        None => return Err(WebmError::Invalid),
    };
    let width = track.width.ok_or(WebmError::Invalid)?;
    let height = track.height.ok_or(WebmError::Invalid)?;
    if width == 0 || width > 1_920 || height == 0 || height > 1_080 {
        return Err(WebmError::Invalid);
    }
    if let Some(default_duration) = track.default_duration {
        if default_duration == 0
            || default_duration
                .checked_mul(60)
                .filter(|duration| *duration >= 1_000_000_000)
                .is_none()
        {
            return Err(WebmError::Invalid);
        }
    }

    let duration_ms = match info.duration {
        None => None,
        Some(duration) => {
            let scale = match info.timecode_scale {
                Some(value) => value,
                None => DEFAULT_TIMECODE_SCALE,
            };
            let duration_ns = duration * scale as f64;
            let duration_ms = (duration_ns / 1_000_000.0).floor();
            if !duration.is_finite()
                || duration <= 0.0
                || !duration_ns.is_finite()
                || !duration_ms.is_finite()
                || duration_ms < 0.0
                || duration_ms >= u64::MAX as f64
            {
                return Err(WebmError::Invalid);
            }
            Some(duration_ms as u64)
        }
    };

    Ok(WebmInfo {
        codec,
        width: u32::try_from(width).map_err(|_| WebmError::Invalid)?,
        height: u32::try_from(height).map_err(|_| WebmError::Invalid)?,
        duration_ms,
    })
}

fn parse_header(
    bytes: &[u8],
    master: EbmlElement,
    depth: usize,
    budget: &mut ElementBudget,
) -> Result<(), WebmError> {
    if depth > MAX_EBML_DEPTH {
        return Err(WebmError::Invalid);
    }
    let mut position = master.data_start;
    let mut doctype_seen = false;
    let mut webm_doctype = false;
    while position < master.data_end {
        let child = read_element(bytes, position, master.data_end, depth, budget)?;
        if child.id == EBML_DOCTYPE {
            if doctype_seen {
                return Err(WebmError::Invalid);
            }
            doctype_seen = true;
            webm_doctype = bytes.get(child.data_start..child.data_end) == Some(b"webm".as_slice());
        }
        position = child.next;
    }
    if !doctype_seen || !webm_doctype {
        return Err(WebmError::Invalid);
    }
    Ok(())
}

fn parse_segment(
    bytes: &[u8],
    master: EbmlElement,
    depth: usize,
    budget: &mut ElementBudget,
    info: &mut SegmentInfo,
    video: &mut Option<VideoTrack>,
) -> Result<(), WebmError> {
    if depth > MAX_EBML_DEPTH {
        return Err(WebmError::Invalid);
    }
    let mut position = master.data_start;
    let mut info_seen = false;
    let mut tracks_seen = false;
    while position < master.data_end {
        let child = read_element(bytes, position, master.data_end, depth, budget)?;
        match child.id {
            EBML_INFO => {
                if info_seen {
                    return Err(WebmError::Invalid);
                }
                parse_info(
                    bytes,
                    child,
                    depth.checked_add(1).ok_or(WebmError::Invalid)?,
                    budget,
                    info,
                )?;
                info_seen = true;
            }
            EBML_TRACKS => {
                if tracks_seen {
                    return Err(WebmError::Invalid);
                }
                parse_tracks(
                    bytes,
                    child,
                    depth.checked_add(1).ok_or(WebmError::Invalid)?,
                    budget,
                    video,
                )?;
                tracks_seen = true;
            }
            _ => {}
        }
        position = child.next;
    }
    if !tracks_seen {
        return Err(WebmError::Invalid);
    }
    Ok(())
}

fn parse_info(
    bytes: &[u8],
    master: EbmlElement,
    depth: usize,
    budget: &mut ElementBudget,
    info: &mut SegmentInfo,
) -> Result<(), WebmError> {
    if depth > MAX_EBML_DEPTH {
        return Err(WebmError::Invalid);
    }
    let mut position = master.data_start;
    while position < master.data_end {
        let child = read_element(bytes, position, master.data_end, depth, budget)?;
        match child.id {
            EBML_TIMECODE_SCALE => {
                if info.timecode_scale.is_some() {
                    return Err(WebmError::Invalid);
                }
                let value = read_ebml_uint(bytes, child)?;
                if value == 0 {
                    return Err(WebmError::Invalid);
                }
                info.timecode_scale = Some(value);
            }
            EBML_DURATION => {
                if info.duration.is_some() {
                    return Err(WebmError::Invalid);
                }
                info.duration = Some(read_ebml_float(bytes, child)?);
            }
            _ => {}
        }
        position = child.next;
    }
    Ok(())
}

fn parse_tracks(
    bytes: &[u8],
    master: EbmlElement,
    depth: usize,
    budget: &mut ElementBudget,
    video: &mut Option<VideoTrack>,
) -> Result<(), WebmError> {
    if depth > MAX_EBML_DEPTH {
        return Err(WebmError::Invalid);
    }
    let mut position = master.data_start;
    let mut track_count = 0usize;
    while position < master.data_end {
        let child = read_element(bytes, position, master.data_end, depth, budget)?;
        if child.id == EBML_TRACK_ENTRY {
            track_count = track_count.checked_add(1).ok_or(WebmError::Invalid)?;
            let track = parse_track(
                bytes,
                child,
                depth.checked_add(1).ok_or(WebmError::Invalid)?,
                budget,
            )?;
            match track.track_type {
                Some(2) => return Err(WebmError::AudioTrack),
                Some(1) => {
                    if video.replace(track).is_some() {
                        return Err(WebmError::Invalid);
                    }
                }
                _ => return Err(WebmError::Invalid),
            }
        }
        position = child.next;
    }
    if track_count != 1 {
        return Err(WebmError::Invalid);
    }
    Ok(())
}

fn parse_track(
    bytes: &[u8],
    master: EbmlElement,
    depth: usize,
    budget: &mut ElementBudget,
) -> Result<VideoTrack, WebmError> {
    if depth > MAX_EBML_DEPTH {
        return Err(WebmError::Invalid);
    }
    let mut track = VideoTrack::default();
    let mut video_seen = false;
    let mut position = master.data_start;
    while position < master.data_end {
        let child = read_element(bytes, position, master.data_end, depth, budget)?;
        match child.id {
            EBML_TRACK_TYPE => {
                if track.track_type.is_some() {
                    return Err(WebmError::Invalid);
                }
                track.track_type = Some(read_ebml_uint(bytes, child)?);
            }
            EBML_CODEC_ID => {
                if track.codec_id_range.is_some() {
                    return Err(WebmError::Invalid);
                }
                bytes
                    .get(child.data_start..child.data_end)
                    .ok_or(WebmError::Invalid)?;
                track.codec_id_range = Some((child.data_start, child.data_end));
            }
            EBML_VIDEO => {
                if video_seen {
                    return Err(WebmError::Invalid);
                }
                parse_video(
                    bytes,
                    child,
                    depth.checked_add(1).ok_or(WebmError::Invalid)?,
                    budget,
                    &mut track,
                )?;
                video_seen = true;
            }
            EBML_DEFAULT_DURATION => {
                if track.default_duration.is_some() {
                    return Err(WebmError::Invalid);
                }
                track.default_duration = Some(read_ebml_uint(bytes, child)?);
            }
            _ => {}
        }
        position = child.next;
    }
    Ok(track)
}

fn parse_video(
    bytes: &[u8],
    master: EbmlElement,
    depth: usize,
    budget: &mut ElementBudget,
    track: &mut VideoTrack,
) -> Result<(), WebmError> {
    if depth > MAX_EBML_DEPTH {
        return Err(WebmError::Invalid);
    }
    let mut position = master.data_start;
    while position < master.data_end {
        let child = read_element(bytes, position, master.data_end, depth, budget)?;
        match child.id {
            EBML_PIXEL_WIDTH => {
                if track.width.is_some() {
                    return Err(WebmError::Invalid);
                }
                track.width = Some(read_ebml_uint(bytes, child)?);
            }
            EBML_PIXEL_HEIGHT => {
                if track.height.is_some() {
                    return Err(WebmError::Invalid);
                }
                track.height = Some(read_ebml_uint(bytes, child)?);
            }
            _ => {}
        }
        position = child.next;
    }
    Ok(())
}

fn read_element(
    bytes: &[u8],
    position: usize,
    parent_end: usize,
    depth: usize,
    budget: &mut ElementBudget,
) -> Result<EbmlElement, WebmError> {
    if depth > MAX_EBML_DEPTH || parent_end > bytes.len() || position >= parent_end {
        return Err(WebmError::Invalid);
    }
    let first = *bytes.get(position).ok_or(WebmError::Invalid)?;
    let id_length = vint_length(first, 4)?;
    let id_end = position
        .checked_add(id_length)
        .filter(|end| *end <= parent_end)
        .ok_or(WebmError::Invalid)?;
    let mut id = 0u32;
    for offset in 0..id_length {
        let byte = *bytes
            .get(position.checked_add(offset).ok_or(WebmError::Invalid)?)
            .ok_or(WebmError::Invalid)?;
        id = id.checked_shl(8).ok_or(WebmError::Invalid)? | u32::from(byte);
    }

    let size_first = *bytes.get(id_end).ok_or(WebmError::Invalid)?;
    let size_length = vint_length(size_first, 8)?;
    let size_end = id_end
        .checked_add(size_length)
        .filter(|end| *end <= parent_end)
        .ok_or(WebmError::Invalid)?;
    let first_data_bits = 8usize.checked_sub(size_length).ok_or(WebmError::Invalid)?;
    let first_mask = if first_data_bits == 0 {
        0
    } else {
        (1u8.checked_shl(u32::try_from(first_data_bits).map_err(|_| WebmError::Invalid)?)
            .ok_or(WebmError::Invalid)?)
            - 1
    };
    let mut size = u64::from(size_first & first_mask);
    for offset in 1..size_length {
        let byte = *bytes
            .get(id_end.checked_add(offset).ok_or(WebmError::Invalid)?)
            .ok_or(WebmError::Invalid)?;
        size = size.checked_shl(8).ok_or(WebmError::Invalid)? | u64::from(byte);
    }
    let size_bits = size_length.checked_mul(7).ok_or(WebmError::Invalid)?;
    let unknown_size = (1u64
        .checked_shl(u32::try_from(size_bits).map_err(|_| WebmError::Invalid)?)
        .ok_or(WebmError::Invalid)?)
        - 1;
    if size == unknown_size {
        return Err(WebmError::Invalid);
    }
    let payload_size = usize::try_from(size).map_err(|_| WebmError::Invalid)?;
    let data_end = size_end
        .checked_add(payload_size)
        .filter(|end| *end <= parent_end)
        .ok_or(WebmError::Invalid)?;
    budget.consume()?;
    Ok(EbmlElement {
        id,
        data_start: size_end,
        data_end,
        next: data_end,
    })
}

fn vint_length(first: u8, maximum: usize) -> Result<usize, WebmError> {
    if first == 0 {
        return Err(WebmError::Invalid);
    }
    for length in 1..=maximum {
        let marker = 0x80u8 >> (length - 1);
        if first & marker != 0 {
            return Ok(length);
        }
    }
    Err(WebmError::Invalid)
}

fn read_ebml_uint(bytes: &[u8], element: EbmlElement) -> Result<u64, WebmError> {
    let length = element
        .data_end
        .checked_sub(element.data_start)
        .filter(|length| (1..=8).contains(length))
        .ok_or(WebmError::Invalid)?;
    let mut value = 0u64;
    for offset in 0..length {
        let index = element
            .data_start
            .checked_add(offset)
            .filter(|index| *index < element.data_end)
            .ok_or(WebmError::Invalid)?;
        let byte = *bytes.get(index).ok_or(WebmError::Invalid)?;
        value = value.checked_shl(8).ok_or(WebmError::Invalid)? | u64::from(byte);
    }
    Ok(value)
}

fn read_ebml_float(bytes: &[u8], element: EbmlElement) -> Result<f64, WebmError> {
    let value = match element.data_end.checked_sub(element.data_start) {
        Some(4) => {
            let slice = bytes
                .get(element.data_start..element.data_end)
                .ok_or(WebmError::Invalid)?;
            let array = <[u8; 4]>::try_from(slice).map_err(|_| WebmError::Invalid)?;
            f64::from(f32::from_be_bytes(array))
        }
        Some(8) => {
            let slice = bytes
                .get(element.data_start..element.data_end)
                .ok_or(WebmError::Invalid)?;
            let array = <[u8; 8]>::try_from(slice).map_err(|_| WebmError::Invalid)?;
            f64::from_be_bytes(array)
        }
        _ => return Err(WebmError::Invalid),
    };
    if !value.is_finite() || value <= 0.0 {
        return Err(WebmError::Invalid);
    }
    Ok(value)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let array = <[u8; 2]>::try_from(bytes.get(offset..end)?).ok()?;
    Some(u16::from_le_bytes(array))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let array = <[u8; 4]>::try_from(bytes.get(offset..end)?).ok()?;
    Some(u32::from_le_bytes(array))
}
