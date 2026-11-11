//! Local secrets protected with Windows DPAPI in current-user scope
//! (GSI-INTAKE-SPEC §7). Plaintext never touches disk except the intake token
//! inside the owned game configuration, which the game requires.

use std::fs;
use std::path::Path;

#[cfg(windows)]
fn protect(plain: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: plain.len() as u32,
            pbData: plain.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(
            &input,
            windows::core::PCWSTR::null(),
            None,
            None,
            None,
            0,
            &mut out,
        )
        .ok()?;
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
        Some(bytes)
    }
}

#[cfg(windows)]
fn unprotect(blob: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, None, None, None, 0, &mut out).ok()?;
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
        Some(bytes)
    }
}

#[cfg(not(windows))]
fn protect(_: &[u8]) -> Option<Vec<u8>> {
    None
}
#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Option<Vec<u8>> {
    None
}

/// Load a protected secret, creating it from `generate` on first use.
/// Returns None when DPAPI is unavailable (GSI_SECRET_UNAVAILABLE).
pub fn load_or_create(path: &Path, generate: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    if let Ok(blob) = fs::read(path) {
        return unprotect(&blob);
    }
    let plain = generate()?;
    let blob = protect(&plain)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &blob).ok()?;
    fs::rename(&tmp, path).ok()?;
    Some(plain)
}

/// Replace a protected secret (token rotation).
pub fn store(path: &Path, plain: &[u8]) -> Option<()> {
    let blob = protect(plain)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &blob).ok()?;
    fs::rename(&tmp, path).ok()
}

pub fn random<const N: usize>() -> Option<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).ok()?;
    Some(b)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip_and_persisted_blob_is_not_plaintext() {
        let dir = std::env::temp_dir().join(format!("kohbor-secret-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.bin");
        let first = load_or_create(&path, || Some(b"plain-secret-value".to_vec())).unwrap();
        let blob = fs::read(&path).unwrap();
        assert!(!blob.windows(5).any(|w| w == b"plain"));
        let again = load_or_create(&path, || panic!("must not regenerate")).unwrap();
        assert_eq!(first, again);
        let _ = fs::remove_dir_all(&dir);
    }
}
