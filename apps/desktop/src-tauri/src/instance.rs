//! Single-instance guard.
//!
//! A second instance cannot bind the persisted intake port. With the game closed
//! it would then pick another port and rewrite the owned game configuration
//! (GSI-INTAKE-SPEC §2.4), silently orphaning the first instance. Only one
//! instance per user session may therefore reach intake startup.

/// Named-mutex identity, derived from `app_id` (contracts/product-identity.json).
pub const INSTANCE_NAME: &str = "Local\\io.github.freshair129.kohbor.instance";

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

/// Bring the already-running control window to the foreground, if it can be found. The title
/// carries the app version (`version::control_window_title`), so windows are matched by the
/// `version::is_control_window_title` prefix rule. A miss degrades to "no focus", never to a
/// second instance.
#[cfg(windows)]
pub fn focus_existing() {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
        SW_RESTORE, SW_SHOW,
    };

    unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> BOOL {
        let mut buffer = [0u16; 160];
        // SAFETY: the buffer is local and the call receives its length.
        let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        // A control window hidden to the tray is not visible but must still be found and shown.
        if length > 0
            && crate::version::is_control_window_title(&String::from_utf16_lossy(
                &buffer[..length as usize],
            ))
        {
            // SAFETY: `found` points at the local `Option<HWND>` below for the whole enumeration.
            unsafe { *(found.0 as *mut Option<HWND>) = Some(hwnd) };
            return BOOL::from(false);
        }
        BOOL::from(true)
    }

    let mut found: Option<HWND> = None;
    // SAFETY: `found` outlives the synchronous enumeration.
    let _ = unsafe {
        EnumWindows(
            Some(visit),
            LPARAM(&mut found as *mut Option<HWND> as isize),
        )
    };
    if let Some(hwnd) = found {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else if !IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_SHOW);
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
