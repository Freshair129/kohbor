mod common;
use kohbor_pack::media::inspect_cover;
use kohbor_pack::{media::inspect_wav, RejectReason};
#[test]
fn wav_helper_length() {
    assert_eq!(common::wav(5).len(), 54);
}
#[test]
fn valid_wav_durations() {
    for (samples, ms) in [(1, 0), (47_999, 999), (48_000, 1000), (720_000, 15_000)] {
        let info = inspect_wav(&common::wav(samples), RejectReason::MixedAudioInvalid).unwrap();
        assert_eq!((info.samples, info.duration_ms), (samples as u64, ms));
    }
}
#[test]
fn rejects_invalid_wavs_with_requested_kind() {
    let mut cases = vec![vec![], b"not wave".to_vec()];
    for (offset, bytes) in [
        (20, vec![3, 0]),
        (20, vec![254, 255]),
        (22, vec![2, 0]),
        (24, 44_100u32.to_le_bytes().to_vec()),
        (34, vec![8, 0]),
        (34, vec![24, 0]),
    ] {
        let mut wav = common::wav(1);
        wav[offset..offset + bytes.len()].copy_from_slice(&bytes);
        cases.push(wav);
    }
    let mut truncated = common::wav(1);
    truncated.pop();
    cases.push(truncated);
    for kind in [
        RejectReason::MixedAudioInvalid,
        RejectReason::IdentityAudioInvalid,
    ] {
        for wav in &cases {
            assert_eq!(inspect_wav(wav, kind.clone()).unwrap_err(), kind);
        }
    }
}
#[test]
fn rejects_long_wav() {
    assert_eq!(
        inspect_wav(&common::wav(720_048), RejectReason::MixedAudioInvalid).unwrap_err(),
        RejectReason::MixedAudioInvalid
    );
}

fn png_cover(width: u32, height: u32, extra: Option<(&[u8; 4], &[u8])>) -> Vec<u8> {
    fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        output.extend_from_slice(&(data.len() as u32).to_be_bytes());
        output.extend_from_slice(kind);
        output.extend_from_slice(data);
        output.extend_from_slice(&[0; 4]);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    if let Some((kind, data)) = extra {
        chunk(&mut out, kind, data);
    }
    chunk(&mut out, b"IDAT", &[]);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn webp_lossless_cover(width: u32, height: u32) -> Vec<u8> {
    let width = width - 1;
    let height = height - 1;
    let payload = [
        0x2f,
        width as u8,
        ((width >> 8) as u8 & 0x3f) | ((height as u8 & 0x03) << 6),
        (height >> 2) as u8,
        (height >> 10) as u8 & 0x0f,
    ];
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"WEBPVP8L");
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out.push(0);
    let riff_size = u32::try_from(out.len() - 8).unwrap();
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());
    out
}

#[test]
fn cover_images_are_bounded_still_png_or_webp_with_dimensions_and_mime() {
    let png = inspect_cover(&png_cover(1920, 1080, None)).unwrap();
    assert_eq!(
        (png.width, png.height, png.content_type),
        (1920, 1080, "image/png")
    );
    let webp = inspect_cover(&webp_lossless_cover(640, 360)).unwrap();
    assert_eq!(
        (webp.width, webp.height, webp.content_type),
        (640, 360, "image/webp")
    );
    assert_eq!(
        inspect_cover(&png_cover(1921, 1080, None)),
        Err(RejectReason::CoverInvalid)
    );
    assert_eq!(
        inspect_cover(&webp_lossless_cover(640, 1081)),
        Err(RejectReason::CoverInvalid)
    );
    assert_eq!(
        inspect_cover(&png_cover(640, 360, Some((b"acTL", &[1, 0, 0, 0])))),
        Err(RejectReason::CoverInvalid)
    );
    let mut truncated = png_cover(640, 360, None);
    truncated.pop();
    assert_eq!(inspect_cover(&truncated), Err(RejectReason::CoverInvalid));
    let mut animated = b"RIFF".to_vec();
    animated.extend_from_slice(&0u32.to_le_bytes());
    animated.extend_from_slice(b"WEBPVP8X");
    animated.extend_from_slice(&10u32.to_le_bytes());
    animated.extend_from_slice(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let size = u32::try_from(animated.len() - 8).unwrap();
    animated[4..8].copy_from_slice(&size.to_le_bytes());
    assert_eq!(inspect_cover(&animated), Err(RejectReason::CoverInvalid));
    let mut oversized = png_cover(1, 1, None);
    oversized.resize(1_048_577, 0);
    assert_eq!(inspect_cover(&oversized), Err(RejectReason::CoverInvalid));
}

use kohbor_pack::media::{inspect_webm, VideoCodec};
#[test]
fn valid_webm_profiles() {
    for (codec, expected) in [("V_VP9", VideoCodec::Vp9), ("V_VP8", VideoCodec::Vp8)] {
        for duration in [None, Some(1000.0)] {
            let info = inspect_webm(&common::webm(codec, 640, 360, None, duration, None)).unwrap();
            assert_eq!(
                (info.codec, info.width, info.height, info.duration_ms),
                (expected, 640, 360, duration.map(|v| v as u64))
            );
        }
    }
}
macro_rules! webm_reject {
    ($name:ident,$bytes:expr,$reason:expr) => {
        #[test]
        fn $name() {
            assert_eq!(inspect_webm(&$bytes).unwrap_err(), $reason);
        }
    };
}
webm_reject!(
    webm_not_ebml,
    b"not ebml".as_slice(),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_audio_track,
    common::webm("V_VP9", 640, 360, Some(2), None, None),
    RejectReason::VisualHasAudioTrack
);
webm_reject!(
    webm_subtitle_track,
    common::webm("V_VP9", 640, 360, Some(17), None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_two_video_tracks,
    common::webm("V_VP9", 640, 360, Some(1), None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_av1,
    common::webm("V_AV1", 640, 360, None, None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_avc,
    common::webm("V_MPEG4/ISO/AVC", 640, 360, None, None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_wide,
    common::webm("V_VP9", 1921, 1080, None, None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_tall,
    common::webm("V_VP9", 1920, 1081, None, None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_zero_width,
    common::webm("V_VP9", 0, 360, None, None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_zero_height,
    common::webm("V_VP9", 640, 0, None, None, None),
    RejectReason::VisualInvalid
);
webm_reject!(
    webm_61fps,
    common::webm("V_VP9", 640, 360, None, None, Some(1_000_000_000 / 61)),
    RejectReason::VisualInvalid
);
#[test]
fn webm_wrong_doctype() {
    let mut b = common::webm("V_VP9", 640, 360, None, None, None);
    let i = b.windows(4).position(|w| w == b"webm").unwrap();
    b[i..i + 4].copy_from_slice(b"mkv!");
    assert_eq!(inspect_webm(&b).unwrap_err(), RejectReason::VisualInvalid);
}
#[test]
fn webm_truncated() {
    let mut b = common::webm("V_VP9", 640, 360, None, None, None);
    b.pop();
    assert_eq!(inspect_webm(&b).unwrap_err(), RejectReason::VisualInvalid);
}
#[test]
fn webm_size_past_end() {
    let mut b = common::webm("V_VP9", 640, 360, None, None, None);
    b[4] = 0xfe;
    assert_eq!(inspect_webm(&b).unwrap_err(), RejectReason::VisualInvalid);
}
#[test]
fn webm_no_video() {
    let mut b = common::webm("V_VP9", 640, 360, None, None, None);
    let i = b.windows(3).position(|w| w == [0x83, 0x81, 1]).unwrap();
    b[i + 2] = 17;
    assert_eq!(inspect_webm(&b).unwrap_err(), RejectReason::VisualInvalid);
}
macro_rules! wav_mutation {
    ($name:ident,$change:expr) => {
        #[test]
        fn $name() {
            let mut b = common::wav(1);
            ($change)(&mut b);
            for kind in [
                RejectReason::MixedAudioInvalid,
                RejectReason::IdentityAudioInvalid,
            ] {
                assert_eq!(inspect_wav(&b, kind.clone()).unwrap_err(), kind);
            }
        }
    };
}
wav_mutation!(wav_no_data, |b: &mut Vec<u8>| {
    b.truncate(36);
    b[4..8].copy_from_slice(&28u32.to_le_bytes());
});
wav_mutation!(wav_two_data, |b: &mut Vec<u8>| {
    let extra = b[36..].to_vec();
    b.extend(extra);
    let n = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&n.to_le_bytes());
});
wav_mutation!(wav_extra_chunk, |b: &mut Vec<u8>| {
    b.extend_from_slice(b"JUNK\x00\x00\x00\x00");
    let n = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&n.to_le_bytes());
});
wav_mutation!(wav_fmt_after_data, |b: &mut Vec<u8>| {
    let fmt = b[12..36].to_vec();
    let data = b[36..].to_vec();
    b.truncate(12);
    b.extend(data);
    b.extend(fmt);
});
wav_mutation!(wav_chunk_size_past_buffer, |b: &mut Vec<u8>| {
    b[40..44].copy_from_slice(&100u32.to_le_bytes());
});
wav_mutation!(wav_odd_data, |b: &mut Vec<u8>| {
    b[40..44].copy_from_slice(&1u32.to_le_bytes());
});
