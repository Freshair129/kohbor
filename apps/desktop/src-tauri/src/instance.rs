//! Single-instance guard.
//!
//! A second instance cannot bind the persisted intake port. With the game closed
//! it would then pick another port and rewrite the owned game configuration
//! (GSI-INTAKE-SPEC §2.4), silently orphaning the first instance. Only one
//! instance per user session may therefore reach intake startup.

/// Named-mutex identity, derived from `app_id` (contracts/product-identity.json).
pub const INSTANCE_NAME: &str = "Local\\io.github.freshair129.kohbor.instance";

/// Must equal the control window title in `tauri.conf.json`; used only to bring
/// the running instance forward. A mismatch degrades to "no focus", never to a
/// second instance.
#[cfg(windows)]
const CONTROL_WINDOW_TITLE: windows::core::PCWSTR = windows::core::w!("Kohbor — Command Deck");

/// Claim the instance name. Returns false when another process already holds it.
/// The handle is intentionally kept for the life of the process.
#[cfg(windows)]
pub fn acquire(name: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        match CreateMutexW(None, false, PCWSTR(wide.as_ptr())) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = windows::Win32::Foundation::CloseHandle(handle);
                    false
                } else {
                    true
                }
            }
            // Unknown is not treated as "free": refuse rather than risk two instances.
            Err(_) => false,
        }
    }
}

/// Bring the already-running control window to the foreground, if it can be found.
#[cfg(windows)]
pub fn focus_existing() {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, CONTROL_WINDOW_TITLE) {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn acquire(_: &str) -> bool {
    true
}

#[cfg(not(windows))]
pub fn focus_existing() {}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn second_claim_of_the_same_name_is_refused() {
        let name = format!("Local\\kohbor-instance-test-{}", std::process::id());
        assert!(acquire(&name));
        assert!(!acquire(&name));
        assert!(acquire(&format!("{name}-other")));
    }
}
