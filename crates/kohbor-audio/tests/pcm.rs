mod common;
use kohbor_audio::pcm::{decode_wav, duration_ms};
#[test]
fn valid_samples() {
    let s = [i16::MIN, -123, 0, 123, i16::MAX];
    assert_eq!(&*decode_wav(&common::wav(&s)).unwrap(), &s);
}
macro_rules! invalid {
    ($name:ident,$change:expr) => {
        #[test]
        fn $name() {
            let mut b = common::wav(&[1, 2]);
            ($change)(&mut b);
            assert!(decode_wav(&b).is_none(), stringify!($name));
        }
    };
}
invalid!(not_riff, |b: &mut Vec<u8>| b[0] = 0);
invalid!(wrong_format, |b: &mut Vec<u8>| b[20] = 3);
invalid!(stereo, |b: &mut Vec<u8>| b[22] = 2);
invalid!(rate_44100, |b: &mut Vec<u8>| b[24..28]
    .copy_from_slice(&44_100u32.to_le_bytes()));
invalid!(bits_8, |b: &mut Vec<u8>| b[34] = 8);
invalid!(bits_24, |b: &mut Vec<u8>| b[34] = 24);
invalid!(extra_chunk, |b: &mut Vec<u8>| {
    b.extend_from_slice(b"JUNK\0\0\0\0");
    let n = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&n.to_le_bytes());
});
invalid!(two_data_chunks, |b: &mut Vec<u8>| {
    b.extend_from_slice(b"data\0\0\0\0");
    let n = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&n.to_le_bytes());
});
invalid!(truncated_data, |b: &mut Vec<u8>| {
    b.pop();
});
invalid!(odd_data, |b: &mut Vec<u8>| {
    b.pop();
    b[4..8].copy_from_slice(&39u32.to_le_bytes());
    b[40..44].copy_from_slice(&3u32.to_le_bytes());
});
invalid!(chunk_past_buffer, |b: &mut Vec<u8>| b[40..44]
    .copy_from_slice(&u32::MAX.to_le_bytes()));
#[test]
fn empty_input() {
    assert!(decode_wav(&[]).is_none());
}
#[test]
fn all_prefixes_do_not_panic() {
    let b = common::wav(&[1, -1]);
    for n in 0..b.len() {
        let _ = decode_wav(&b[..n]);
    }
}
#[test]
fn duration_floor() {
    assert_eq!(duration_ms(&vec![0; 47_999]), 999);
}
#[test]
fn duration_second() {
    assert_eq!(duration_ms(&vec![0; 48_000]), 1000);
}
#[test]
fn duration_zero() {
    assert_eq!(duration_ms(&[]), 0);
}
#[test]
fn duration_ten_seconds() {
    assert_eq!(duration_ms(&vec![0; 480_000]), 10_000);
}
