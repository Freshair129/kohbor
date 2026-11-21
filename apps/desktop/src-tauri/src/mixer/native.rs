//! Windows Core Audio adapter for MEDIA-009: default render endpoint -> `IAudioSessionManager2` -> per session
//! `IAudioSessionControl2::GetProcessId` and `ISimpleAudioVolume`. Only volume and mute are touched. The owning
//! executable is resolved with a limited query handle (image name only): no process memory, files or input.

use super::{BackendError, DeviceRead, SessionBackend, SessionRead};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eConsole, eRender, AudioSessionStateExpired, IAudioSessionControl, IAudioSessionControl2,
    IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// COM initialisation for the mixer thread (multithreaded apartment), undone when dropped.
pub(crate) struct ComApartment {
    initialized: bool,
}

impl ComApartment {
    pub(crate) fn enter() -> Self {
        // S_OK and S_FALSE both need a matching CoUninitialize; RPC_E_CHANGED_MODE does not.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_err() {
            crate::log_warn!(
                "mixer",
                "com_unavailable",
                "COM could not be initialised for the mixer"
            );
        }
        Self {
            initialized: result.is_ok(),
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

#[derive(Default)]
pub(crate) struct CoreAudioBackend {
    enumerator: Option<IMMDeviceEnumerator>,
}

struct Session {
    exe: Option<String>,
    volume: Option<ISimpleAudioVolume>,
}

impl CoreAudioBackend {
    fn enumerator(&mut self) -> Result<IMMDeviceEnumerator, BackendError> {
        if let Some(enumerator) = &self.enumerator {
            return Ok(enumerator.clone());
        }
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(|_| BackendError::Unavailable)?;
        self.enumerator = Some(enumerator.clone());
        Ok(enumerator)
    }

    fn default_device(&mut self) -> Result<(IMMDevice, String), BackendError> {
        let enumerator = self.enumerator()?;
        let device = match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
            Ok(device) => device,
            Err(_) => {
                // A stale enumerator after an audio service restart: create a new one next time.
                self.enumerator = None;
                return Err(BackendError::Unavailable);
            }
        };
        let id = unsafe { device.GetId() }.map_err(|_| BackendError::Unavailable)?;
        let text = unsafe { id.to_string() };
        unsafe { CoTaskMemFree(Some(id.0 as *const core::ffi::c_void)) };
        let text = text.map_err(|_| BackendError::Unavailable)?;
        Ok((device, text))
    }

    fn sessions(device: &IMMDevice) -> Result<Vec<Session>, BackendError> {
        let manager: IAudioSessionManager2 =
            unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(|_| BackendError::Unavailable)?;
        let list =
            unsafe { manager.GetSessionEnumerator() }.map_err(|_| BackendError::Unavailable)?;
        let count = unsafe { list.GetCount() }.map_err(|_| BackendError::Unavailable)?;
        let mut sessions = Vec::new();
        for index in 0..count {
            let Ok(control) = (unsafe { list.GetSession(index) }) else {
                sessions.push(Session {
                    exe: None,
                    volume: None,
                });
                continue;
            };
            if let Some(session) = Self::session(&control) {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    /// `None` for sessions that are not candidates at all (expired, system sounds).
    fn session(control: &IAudioSessionControl) -> Option<Session> {
        if unsafe { control.GetState() }.is_ok_and(|state| state == AudioSessionStateExpired) {
            return None;
        }
        let control2: IAudioSessionControl2 = match control.cast() {
            Ok(control2) => control2,
            Err(_) => {
                return Some(Session {
                    exe: None,
                    volume: None,
                })
            }
        };
        let process_id = unsafe { control2.GetProcessId() }.unwrap_or(0);
        if process_id == 0 {
            // System sounds, or a session spanning several processes: never a mixer target.
            return None;
        }
        let exe = process_base_name(process_id);
        let volume = control.cast::<ISimpleAudioVolume>().ok();
        Some(Session { exe, volume })
    }
}

impl SessionBackend for CoreAudioBackend {
    fn read(&mut self) -> Result<DeviceRead, BackendError> {
        let (device, id) = self.default_device()?;
        let sessions = Self::sessions(&device)?
            .into_iter()
            .map(|session| {
                let (Some(exe), Some(volume)) = (session.exe.clone(), session.volume.as_ref())
                else {
                    return SessionRead::Unreadable { exe: session.exe };
                };
                let level = unsafe { volume.GetMasterVolume() };
                let muted = unsafe { volume.GetMute() };
                match (level, muted) {
                    (Ok(level), Ok(muted)) if level.is_finite() => SessionRead::Readable {
                        exe,
                        level,
                        muted: muted.as_bool(),
                    },
                    _ => SessionRead::Unreadable { exe: Some(exe) },
                }
            })
            .collect();
        Ok(DeviceRead {
            device: id,
            sessions,
        })
    }

    fn apply(
        &mut self,
        device_id: &str,
        exe: &str,
        level: Option<f32>,
        muted: Option<bool>,
    ) -> Result<usize, BackendError> {
        let (device, id) = self.default_device()?;
        if id != device_id {
            return Err(BackendError::DeviceChanged);
        }
        let mut changed = 0usize;
        for session in Self::sessions(&device)? {
            if session.exe.as_deref() != Some(exe) {
                continue;
            }
            let Some(volume) = session.volume else {
                continue;
            };
            // Read again in this same enumeration: a session that cannot be read is never set.
            if unsafe { volume.GetMasterVolume() }.is_err() {
                continue;
            }
            if let Some(level) = level {
                unsafe { volume.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null()) }
                    .map_err(|_| BackendError::SetFailed)?;
            }
            if let Some(muted) = muted {
                unsafe { volume.SetMute(muted, std::ptr::null()) }
                    .map_err(|_| BackendError::SetFailed)?;
            }
            changed += 1;
        }
        if changed == 0 {
            return Err(BackendError::Missing);
        }
        Ok(changed)
    }
}

/// Lower-case executable base name of a process (no path), or `None` when it cannot be queried.
pub(crate) fn process_base_name(process_id: u32) -> Option<String> {
    if process_id == 0 {
        return None;
    }
    let process =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()? };
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    let image = OsString::from_wide(&path[..usize::try_from(length).ok()?.min(path.len())]);
    std::path::Path::new(&image)
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
}
