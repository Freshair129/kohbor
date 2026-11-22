//! UI-013 show-app hotkey (owner decision DEC-SHOW-APP-HOTKEY, 2026-11-22).
//!
//! One global hotkey (default Ctrl+Alt+X) toggles the control window: show, restore and focus it
//! when it is hidden, minimized or behind another window; hide it when it is already the focused
//! window and give the foreground back to the window that had it before the show. The key is
//! registered with `RegisterHotKey` only: no keyboard hook, no synthesized input, no read of any
//! other process. It is on by default, has a Settings switch and can be remapped; a remap
//! registers the new key before it releases the old one, so a failed remap keeps the old key.

use crate::hotkeys::StartFailure;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub(crate) const MOD_ALT: u32 = 0x0001;
pub(crate) const MOD_CONTROL: u32 = 0x0002;
pub(crate) const MOD_SHIFT: u32 = 0x0004;
pub(crate) const MOD_WIN: u32 = 0x0008;
const MOD_NOREPEAT: u32 = 0x4000;
const MODIFIER_MASK: u32 = MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN;

const VK_F1: u32 = 0x70;
const VK_F12: u32 = 0x7B;

/// A modifier set plus one virtual key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    modifiers: u32,
    key: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingError {
    Empty,
    UnknownToken,
    DuplicateModifier,
    NoKey,
    TwoKeys,
    UnsupportedKey,
    NeedsModifier,
    Reserved,
}

impl BindingError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Empty => "SHOW_HOTKEY_EMPTY",
            Self::UnknownToken => "SHOW_HOTKEY_UNKNOWN_TOKEN",
            Self::DuplicateModifier => "SHOW_HOTKEY_DUPLICATE_MODIFIER",
            Self::NoKey => "SHOW_HOTKEY_NO_KEY",
            Self::TwoKeys => "SHOW_HOTKEY_TWO_KEYS",
            Self::UnsupportedKey => "SHOW_HOTKEY_UNSUPPORTED_KEY",
            Self::NeedsModifier => "SHOW_HOTKEY_NEEDS_MODIFIER",
            Self::Reserved => "SHOW_HOTKEY_RESERVED",
        }
    }
}

/// Keys that already belong to the baseline (UX-SPEC section 9) or to Windows.
const RESERVED: &[Binding] = &[
    Binding {
        modifiers: MOD_CONTROL | MOD_ALT,
        key: 0x53,
    }, // Ctrl+Alt+S overlay
    Binding {
        modifiers: MOD_CONTROL | MOD_ALT,
        key: 0x56,
    }, // Ctrl+Alt+V push-to-talk
    Binding {
        modifiers: MOD_ALT,
        key: 0x4D,
    }, // Alt+M mute
    Binding {
        modifiers: MOD_ALT,
        key: 0x73,
    }, // Alt+F4 close
];

impl Binding {
    pub(crate) const DEFAULT: Binding = Binding {
        modifiers: MOD_CONTROL | MOD_ALT,
        key: 0x58,
    };

    pub(crate) fn modifiers(self) -> u32 {
        self.modifiers
    }

    pub(crate) fn key(self) -> u32 {
        self.key
    }

    /// Parse `Ctrl+Alt+X` style text. Modifiers: Ctrl, Alt, Shift, Win (any order, case-insensitive).
    /// Keys: A–Z, 0–9, F1–F12. At least one of Ctrl, Alt or Win is required.
    pub(crate) fn parse(text: &str) -> Result<Binding, BindingError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(BindingError::Empty);
        }
        let mut modifiers = 0u32;
        let mut key = None;
        for token in text.split('+') {
            let token = token.trim();
            let modifier = match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => Some(MOD_CONTROL),
                "alt" => Some(MOD_ALT),
                "shift" => Some(MOD_SHIFT),
                "win" | "meta" => Some(MOD_WIN),
                _ => None,
            };
            if let Some(bit) = modifier {
                if modifiers & bit != 0 {
                    return Err(BindingError::DuplicateModifier);
                }
                modifiers |= bit;
                continue;
            }
            let Some(code) = key_code(token) else {
                return Err(if token.is_empty() {
                    BindingError::UnknownToken
                } else if token.chars().all(|c| c.is_ascii_alphanumeric()) {
                    BindingError::UnsupportedKey
                } else {
                    BindingError::UnknownToken
                });
            };
            if key.is_some() {
                return Err(BindingError::TwoKeys);
            }
            key = Some(code);
        }
        let key = key.ok_or(BindingError::NoKey)?;
        if modifiers & (MOD_CONTROL | MOD_ALT | MOD_WIN) == 0 {
            return Err(BindingError::NeedsModifier);
        }
        let binding = Binding { modifiers, key };
        if RESERVED.contains(&binding) {
            return Err(BindingError::Reserved);
        }
        Ok(binding)
    }

    /// Canonical text: Ctrl, Alt, Shift, Win, then the key.
    pub(crate) fn label(self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (bit, name) in [
            (MOD_CONTROL, "Ctrl"),
            (MOD_ALT, "Alt"),
            (MOD_SHIFT, "Shift"),
            (MOD_WIN, "Win"),
        ] {
            if self.modifiers & bit != 0 {
                parts.push(name.to_owned());
            }
        }
        parts.push(key_name(self.key));
        parts.join("+")
    }

    /// Alt alone (or Alt+Shift) with a key is likely a Dota action: Alt is Dota's default
    /// self-cast modifier. The registration would take that key away from the game.
    pub(crate) fn may_conflict_with_dota(self) -> bool {
        self.modifiers & (MOD_CONTROL | MOD_WIN) == 0
    }

    /// Whether a WM_HOTKEY lParam (modifiers in the low word, key in the high word) is this key.
    pub(crate) fn matches(self, modifiers: u32, key: u32) -> bool {
        modifiers & MODIFIER_MASK == self.modifiers && key == self.key
    }
}

fn key_code(token: &str) -> Option<u32> {
    let upper = token.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() == 1 && (bytes[0].is_ascii_uppercase() || bytes[0].is_ascii_digit()) {
        return Some(u32::from(bytes[0]));
    }
    let number = upper.strip_prefix('F')?.parse::<u32>().ok()?;
    (1..=12).contains(&number).then(|| VK_F1 + number - 1)
}

fn key_name(key: u32) -> String {
    if (VK_F1..=VK_F12).contains(&key) {
        format!("F{}", key - VK_F1 + 1)
    } else {
        char::from_u32(key).map(String::from).unwrap_or_default()
    }
}

/// What one hotkey press does to the control window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToggleAction {
    Show,
    Hide,
}

/// Hide only when the window is visible, not minimized and focused; otherwise bring it up.
pub(crate) fn toggle_action(visible: bool, minimized: bool, focused: bool) -> ToggleAction {
    if visible && !minimized && focused {
        ToggleAction::Hide
    } else {
        ToggleAction::Show
    }
}

/// The window to give the foreground back to on hide: the one remembered at show time, if it is
/// not the control window and still exists.
pub(crate) fn restore_target(
    remembered: Option<isize>,
    own: Option<isize>,
    still_exists: impl Fn(isize) -> bool,
) -> Option<isize> {
    let target = remembered?;
    if target == 0 || Some(target) == own || !still_exists(target) {
        return None;
    }
    Some(target)
}

// ---------------------------------------------------------------------------------------------
// Preferences: `shell-prefs.json` in the data directory.

const SCHEMA_VERSION: u8 = 1;
pub(crate) const FILE_NAME: &str = "shell-prefs.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShellPrefs {
    pub(crate) show_hotkey_enabled: bool,
    pub(crate) show_hotkey: Binding,
}

impl Default for ShellPrefs {
    fn default() -> Self {
        Self {
            show_hotkey_enabled: true,
            show_hotkey: Binding::DEFAULT,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredShellPrefs {
    schema_version: u8,
    show_hotkey_enabled: bool,
    show_hotkey: String,
}

pub(crate) fn parse_prefs_or_default(bytes: &[u8]) -> ShellPrefs {
    let Ok(stored) = serde_json::from_slice::<StoredShellPrefs>(bytes) else {
        return ShellPrefs::default();
    };
    if stored.schema_version != SCHEMA_VERSION {
        return ShellPrefs::default();
    }
    let Ok(binding) = Binding::parse(&stored.show_hotkey) else {
        return ShellPrefs::default();
    };
    ShellPrefs {
        show_hotkey_enabled: stored.show_hotkey_enabled,
        show_hotkey: binding,
    }
}

pub(crate) fn encode_prefs(prefs: &ShellPrefs) -> Vec<u8> {
    serde_json::to_vec(&StoredShellPrefs {
        schema_version: SCHEMA_VERSION,
        show_hotkey_enabled: prefs.show_hotkey_enabled,
        show_hotkey: prefs.show_hotkey.label(),
    })
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Registration with transactional replacement.

/// Starts one registration on its own message thread and stops it again.
pub(crate) trait Registrar: Send {
    type Handle: Send;
    fn register(
        &mut self,
        binding: Binding,
        on_press: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<Self::Handle, StartFailure>;
    fn unregister(&mut self, handle: Self::Handle);
}

pub(crate) struct Controller<R: Registrar> {
    registrar: R,
    active: Option<(Binding, R::Handle)>,
}

impl<R: Registrar> Controller<R> {
    pub(crate) fn new(registrar: R) -> Self {
        Self {
            registrar,
            active: None,
        }
    }

    pub(crate) fn registered(&self) -> Option<Binding> {
        self.active.as_ref().map(|(binding, _)| *binding)
    }

    /// Make the registration match `(enabled, binding)`. A new key is registered before the old
    /// one is released; when that fails the old registration stays and the failure is returned.
    pub(crate) fn apply(
        &mut self,
        enabled: bool,
        binding: Binding,
        on_press: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<(), StartFailure> {
        if !enabled {
            if let Some((_, handle)) = self.active.take() {
                self.registrar.unregister(handle);
            }
            return Ok(());
        }
        if self.registered() == Some(binding) {
            return Ok(());
        }
        let handle = self.registrar.register(binding, on_press)?;
        if let Some((_, old)) = self.active.replace((binding, handle)) {
            self.registrar.unregister(old);
        }
        Ok(())
    }
}

impl<R: Registrar> Drop for Controller<R> {
    fn drop(&mut self) {
        if let Some((_, handle)) = self.active.take() {
            self.registrar.unregister(handle);
        }
    }
}

/// The Win32 registrar: one `RegisterHotKey` on a dedicated message thread per registration.
#[derive(Default)]
pub(crate) struct NativeRegistrar;

pub(crate) struct NativeHandle {
    #[cfg(windows)]
    worker: std::thread::JoinHandle<()>,
    #[cfg(windows)]
    thread_id: u32,
}

impl Registrar for NativeRegistrar {
    type Handle = NativeHandle;

    #[cfg(windows)]
    fn register(
        &mut self,
        binding: Binding,
        on_press: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<NativeHandle, StartFailure> {
        use std::sync::mpsc;
        use windows::Win32::Foundation::GetLastError;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, WM_HOTKEY,
        };

        const HOTKEY_ID: i32 = 1;
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("show-app-hotkey".to_owned())
            .spawn(move || unsafe {
                let thread_id = GetCurrentThreadId();
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                let modifiers = HOT_KEY_MODIFIERS(binding.modifiers() | MOD_NOREPEAT);
                if let Err(error) = RegisterHotKey(None, HOTKEY_ID, modifiers, binding.key()) {
                    let last = GetLastError().0;
                    let code = if last != 0 {
                        last
                    } else {
                        (error.code().0 as u32) & 0xffff
                    };
                    let _ = startup_tx.send(Err(code));
                    return;
                }
                if startup_tx.send(Ok(thread_id)).is_err() {
                    let _ = UnregisterHotKey(None, HOTKEY_ID);
                    return;
                }
                loop {
                    if GetMessageW(&mut message, None, 0, 0).0 <= 0 {
                        break;
                    }
                    if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
                        let lparam = message.lParam.0 as u32;
                        if binding.matches(lparam & 0xffff, (lparam >> 16) & 0xffff) {
                            on_press();
                        }
                    }
                }
                let _ = UnregisterHotKey(None, HOTKEY_ID);
            })
            .map_err(StartFailure::Thread)?;
        match startup_rx.recv() {
            Ok(Ok(thread_id)) => Ok(NativeHandle { worker, thread_id }),
            Ok(Err(code)) => {
                let _ = worker.join();
                Err(StartFailure::Registration(code))
            }
            Err(_) => {
                let _ = worker.join();
                Err(StartFailure::Thread(std::io::Error::other(
                    "show-app hotkey thread ended before registration completed",
                )))
            }
        }
    }

    #[cfg(not(windows))]
    fn register(
        &mut self,
        _binding: Binding,
        _on_press: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<NativeHandle, StartFailure> {
        Err(StartFailure::Thread(std::io::Error::other(
            "global hotkeys need Windows",
        )))
    }

    fn unregister(&mut self, handle: NativeHandle) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
            unsafe {
                let _ = PostThreadMessageW(handle.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            let _ = handle.worker.join();
        }
        #[cfg(not(windows))]
        let _ = handle;
    }
}

// ---------------------------------------------------------------------------------------------
// Service held in AppState.

/// The renderer contract for `shell_hotkey_get` / `shell_hotkey_set`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ShowHotkeyReply {
    pub(crate) enabled: bool,
    pub(crate) binding: String,
    pub(crate) registered: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) dota_conflict_risk: bool,
}

pub(crate) struct Service<R: Registrar = NativeRegistrar> {
    path: Option<PathBuf>,
    prefs: Mutex<ShellPrefs>,
    controller: Mutex<Controller<R>>,
    last_error: Mutex<Option<String>>,
    /// The foreground window remembered when the hotkey showed the control window.
    remembered_foreground: Mutex<Option<isize>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn failure_code(failure: &StartFailure) -> String {
    match failure {
        StartFailure::Registration(code) => format!("SHOW_HOTKEY_REGISTER_FAILED:{code}"),
        StartFailure::Thread(_) => "SHOW_HOTKEY_THREAD_START_FAILED".to_owned(),
    }
}

impl Service<NativeRegistrar> {
    pub(crate) fn open(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE_NAME);
        let prefs = fs::read(&path)
            .map(|bytes| parse_prefs_or_default(&bytes))
            .unwrap_or_default();
        Self::with(Some(path), prefs, NativeRegistrar)
    }
}

impl<R: Registrar> Service<R> {
    pub(crate) fn with(path: Option<PathBuf>, prefs: ShellPrefs, registrar: R) -> Self {
        Self {
            path,
            prefs: Mutex::new(prefs),
            controller: Mutex::new(Controller::new(registrar)),
            last_error: Mutex::new(None),
            remembered_foreground: Mutex::new(None),
        }
    }

    pub(crate) fn reply(&self) -> ShowHotkeyReply {
        let prefs = *lock(&self.prefs);
        let registered = lock(&self.controller).registered() == Some(prefs.show_hotkey);
        ShowHotkeyReply {
            enabled: prefs.show_hotkey_enabled,
            binding: prefs.show_hotkey.label(),
            registered: prefs.show_hotkey_enabled && registered,
            error_code: lock(&self.last_error).clone(),
            dota_conflict_risk: prefs.show_hotkey.may_conflict_with_dota(),
        }
    }

    /// Register the stored preference at start-up. A failure is kept for the Settings reply.
    pub(crate) fn start(&self, on_press: Arc<dyn Fn() + Send + Sync + 'static>) -> ShowHotkeyReply {
        let prefs = *lock(&self.prefs);
        let result =
            lock(&self.controller).apply(prefs.show_hotkey_enabled, prefs.show_hotkey, on_press);
        *lock(&self.last_error) = result.as_ref().err().map(failure_code);
        self.reply()
    }

    /// Transactional change: parse, register the new key while the old one stays, save, and only
    /// then release the old key. On any failure the prior registration and file stay as they were.
    pub(crate) fn set(
        &self,
        enabled: bool,
        binding_text: &str,
        on_press: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<ShowHotkeyReply, String> {
        let binding = Binding::parse(binding_text).map_err(|error| error.code().to_owned())?;
        let mut prefs = lock(&self.prefs);
        let previous = *prefs;
        let next = ShellPrefs {
            show_hotkey_enabled: enabled,
            show_hotkey: binding,
        };
        let mut controller = lock(&self.controller);
        if let Err(failure) = controller.apply(enabled, binding, on_press.clone()) {
            let code = failure_code(&failure);
            *lock(&self.last_error) = Some(code.clone());
            return Err(code);
        }
        if next != previous {
            if let Err(code) = self.save(&next) {
                // Put the previous registration back; the file still holds the previous value.
                let _ =
                    controller.apply(previous.show_hotkey_enabled, previous.show_hotkey, on_press);
                return Err(code);
            }
        }
        *prefs = next;
        *lock(&self.last_error) = None;
        drop(controller);
        drop(prefs);
        Ok(self.reply())
    }

    fn save(&self, prefs: &ShellPrefs) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        crate::atomic::write(path, &encode_prefs(prefs))
            .map_err(|_| "SHELL_PREFS_WRITE_FAILED".to_owned())
    }

    pub(crate) fn remember_foreground(&self, hwnd: Option<isize>) {
        *lock(&self.remembered_foreground) = hwnd;
    }

    pub(crate) fn take_remembered_foreground(&self) -> Option<isize> {
        lock(&self.remembered_foreground).take()
    }
}

// ---------------------------------------------------------------------------------------------
// Window side (Tauri + Win32).

#[cfg(windows)]
mod native {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };

    pub(super) fn foreground() -> Option<isize> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.0.is_null()).then_some(hwnd.0 as isize)
    }

    pub(super) fn exists(hwnd: isize) -> bool {
        unsafe { IsWindow(Some(HWND(hwnd as *mut core::ffi::c_void))) }.as_bool()
    }

    /// Give the foreground to a window; allowed because this process holds it at that moment.
    pub(super) fn set_foreground(hwnd: isize) -> bool {
        unsafe { SetForegroundWindow(HWND(hwnd as *mut core::ffi::c_void)) }.as_bool()
    }

    pub(super) fn exclusive_fullscreen() -> bool {
        unsafe { SHQueryUserNotificationState() }
            .is_ok_and(|state| state == QUNS_RUNNING_D3D_FULL_SCREEN)
    }

    pub(super) fn own(window: &tauri::WebviewWindow<tauri::Wry>) -> Option<isize> {
        window.hwnd().ok().map(|hwnd| hwnd.0 as isize)
    }
}

#[cfg(not(windows))]
mod native {
    pub(super) fn foreground() -> Option<isize> {
        None
    }
    pub(super) fn exists(_hwnd: isize) -> bool {
        false
    }
    pub(super) fn set_foreground(_hwnd: isize) -> bool {
        false
    }
    pub(super) fn exclusive_fullscreen() -> bool {
        false
    }
    pub(super) fn own(_window: &tauri::WebviewWindow<tauri::Wry>) -> Option<isize> {
        None
    }
}

/// Runs on the hotkey thread for each press.
pub(crate) fn toggle_control(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<crate::AppState>();
    if state.host.shutdown_started() {
        return;
    }
    let Some(window) = app.get_webview_window("control") else {
        return;
    };
    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);
    let own = native::own(&window);
    match toggle_action(visible, minimized, focused) {
        ToggleAction::Show => {
            let foreground = native::foreground().filter(|hwnd| Some(*hwnd) != own);
            state.show_hotkey.remember_foreground(foreground);
            let exclusive = native::exclusive_fullscreen();
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            crate::log_info!(
                "app",
                "shown_by_hotkey",
                "control window shown by the show-app hotkey",
                exclusive_fullscreen = exclusive
            );
        }
        ToggleAction::Hide => {
            if app.tray_by_id(crate::tray::TRAY_ID).is_some() {
                let _ = window.hide();
            } else {
                let _ = window.minimize();
            }
            let target = restore_target(
                state.show_hotkey.take_remembered_foreground(),
                own,
                native::exists,
            );
            let restored = target.is_some_and(native::set_foreground);
            crate::log_info!(
                "app",
                "hidden_by_hotkey",
                "control window hidden by the show-app hotkey",
                foreground_restored = restored
            );
        }
    }
}

pub(crate) fn press_callback(app: tauri::AppHandle) -> Arc<dyn Fn() + Send + Sync + 'static> {
    Arc::new(move || toggle_control(&app))
}

pub(crate) fn log_state(reply: &ShowHotkeyReply) {
    crate::log_info!(
        "app",
        "show_hotkey_state",
        "show-app hotkey state",
        enabled = reply.enabled,
        registered = reply.registered,
        failed = reply.error_code.is_some()
    );
}

/// Register the stored preference during setup.
pub(crate) fn start(app: &tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let reply = state.show_hotkey.start(press_callback(app.clone()));
    if let Some(code) = &reply.error_code {
        crate::log_warn!(
            "app",
            "show_hotkey_register_failed",
            "show-app hotkey registration failed",
            code = code.clone()
        );
    }
    log_state(&reply);
}

#[tauri::command(async)]
pub(crate) fn shell_hotkey_get(state: tauri::State<'_, crate::AppState>) -> ShowHotkeyReply {
    state.show_hotkey.reply()
}

#[tauri::command(async)]
pub(crate) fn shell_hotkey_set(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    enabled: bool,
    binding: String,
) -> Result<ShowHotkeyReply, String> {
    let result = state
        .show_hotkey
        .set(enabled, &binding, press_callback(app.clone()));
    match &result {
        Ok(reply) => log_state(reply),
        Err(code) => crate::log_warn!(
            "app",
            "show_hotkey_change_failed",
            "show-app hotkey change kept the prior state",
            code = code.clone()
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn noop() -> Arc<dyn Fn() + Send + Sync + 'static> {
        Arc::new(|| {})
    }

    #[test]
    fn default_is_ctrl_alt_x_and_enabled() {
        let prefs = ShellPrefs::default();
        assert!(prefs.show_hotkey_enabled);
        assert_eq!(prefs.show_hotkey.label(), "Ctrl+Alt+X");
        assert!(!prefs.show_hotkey.may_conflict_with_dota());
    }

    #[test]
    fn parse_is_case_and_order_insensitive_and_round_trips() {
        let binding = Binding::parse(" alt + CTRL + x ").unwrap();
        assert_eq!(binding, Binding::DEFAULT);
        assert_eq!(Binding::parse(&binding.label()).unwrap(), binding);
        assert_eq!(
            Binding::parse("Ctrl+Shift+F9").unwrap().label(),
            "Ctrl+Shift+F9"
        );
        assert_eq!(Binding::parse("Win+Alt+7").unwrap().label(), "Alt+Win+7");
    }

    #[test]
    fn parse_rejects_bad_text() {
        assert_eq!(Binding::parse(""), Err(BindingError::Empty));
        assert_eq!(Binding::parse("Ctrl+Alt"), Err(BindingError::NoKey));
        assert_eq!(Binding::parse("Ctrl+X+Y"), Err(BindingError::TwoKeys));
        assert_eq!(
            Binding::parse("Ctrl+Ctrl+X"),
            Err(BindingError::DuplicateModifier)
        );
        assert_eq!(
            Binding::parse("Ctrl+Tab"),
            Err(BindingError::UnsupportedKey)
        );
        assert_eq!(
            Binding::parse("Ctrl+F13"),
            Err(BindingError::UnsupportedKey)
        );
        assert_eq!(Binding::parse("Ctrl+?"), Err(BindingError::UnknownToken));
        assert_eq!(Binding::parse("Ctrl++"), Err(BindingError::UnknownToken));
    }

    #[test]
    fn parse_requires_ctrl_alt_or_win() {
        assert_eq!(Binding::parse("X"), Err(BindingError::NeedsModifier));
        assert_eq!(Binding::parse("Shift+X"), Err(BindingError::NeedsModifier));
        assert!(Binding::parse("Alt+X").is_ok());
    }

    #[test]
    fn parse_rejects_baseline_and_windows_keys() {
        for text in ["Ctrl+Alt+S", "Ctrl+Alt+V", "Alt+M", "Alt+F4"] {
            assert_eq!(Binding::parse(text), Err(BindingError::Reserved), "{text}");
        }
    }

    #[test]
    fn alt_only_keys_carry_the_dota_warning() {
        assert!(Binding::parse("Alt+X").unwrap().may_conflict_with_dota());
        assert!(Binding::parse("Alt+Shift+X")
            .unwrap()
            .may_conflict_with_dota());
        assert!(!Binding::parse("Ctrl+X").unwrap().may_conflict_with_dota());
        assert!(!Binding::parse("Win+Alt+X")
            .unwrap()
            .may_conflict_with_dota());
    }

    #[test]
    fn matches_ignores_norepeat_and_checks_exact_modifiers() {
        let binding = Binding::DEFAULT;
        assert!(binding.matches(MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 0x58));
        assert!(!binding.matches(MOD_CONTROL | MOD_ALT | MOD_SHIFT, 0x58));
        assert!(!binding.matches(MOD_CONTROL | MOD_ALT, 0x59));
    }

    #[test]
    fn toggle_hides_only_a_visible_focused_unminimized_window() {
        assert_eq!(toggle_action(true, false, true), ToggleAction::Hide);
        assert_eq!(toggle_action(true, false, false), ToggleAction::Show);
        assert_eq!(toggle_action(true, true, true), ToggleAction::Show);
        assert_eq!(toggle_action(false, false, false), ToggleAction::Show);
        assert_eq!(toggle_action(false, true, false), ToggleAction::Show);
    }

    #[test]
    fn restore_target_skips_own_missing_and_gone_windows() {
        assert_eq!(restore_target(Some(42), Some(7), |_| true), Some(42));
        assert_eq!(restore_target(None, Some(7), |_| true), None);
        assert_eq!(restore_target(Some(7), Some(7), |_| true), None);
        assert_eq!(restore_target(Some(0), None, |_| true), None);
        assert_eq!(restore_target(Some(42), Some(7), |_| false), None);
    }

    #[test]
    fn prefs_round_trip_and_bad_files_fall_back_to_default() {
        let prefs = ShellPrefs {
            show_hotkey_enabled: false,
            show_hotkey: Binding::parse("Ctrl+Shift+K").unwrap(),
        };
        assert_eq!(parse_prefs_or_default(&encode_prefs(&prefs)), prefs);
        let default = ShellPrefs::default();
        assert_eq!(parse_prefs_or_default(b"not json"), default);
        assert_eq!(
            parse_prefs_or_default(
                br#"{"schema_version":2,"show_hotkey_enabled":false,"show_hotkey":"Ctrl+K"}"#
            ),
            default
        );
        assert_eq!(
            parse_prefs_or_default(
                br#"{"schema_version":1,"show_hotkey_enabled":false,"show_hotkey":"Alt+M"}"#
            ),
            default
        );
        assert_eq!(
            parse_prefs_or_default(br#"{"schema_version":1,"show_hotkey_enabled":true,"show_hotkey":"Ctrl+Alt+X","x":1}"#),
            default
        );
    }

    /// A fake registrar: keys in `taken` fail as if another program held them.
    #[derive(Default)]
    struct Fake {
        taken: Vec<Binding>,
        live: Arc<Mutex<Vec<Binding>>>,
        next: usize,
    }

    impl Registrar for Fake {
        type Handle = (usize, Binding);
        fn register(
            &mut self,
            binding: Binding,
            _on_press: Arc<dyn Fn() + Send + Sync + 'static>,
        ) -> Result<(usize, Binding), StartFailure> {
            if self.taken.contains(&binding) {
                return Err(StartFailure::Registration(1409));
            }
            self.next += 1;
            lock(&self.live).push(binding);
            Ok((self.next, binding))
        }
        fn unregister(&mut self, handle: (usize, Binding)) {
            lock(&self.live).retain(|binding| *binding != handle.1);
        }
    }

    fn service(taken: &[&str], prefs: ShellPrefs) -> (Service<Fake>, Arc<Mutex<Vec<Binding>>>) {
        let live = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            taken: taken.iter().map(|t| Binding::parse(t).unwrap()).collect(),
            live: live.clone(),
            next: 0,
        };
        (Service::with(None, prefs, fake), live)
    }

    #[test]
    fn start_registers_the_default() {
        let (service, live) = service(&[], ShellPrefs::default());
        let reply = service.start(noop());
        assert!(reply.enabled && reply.registered);
        assert_eq!(reply.binding, "Ctrl+Alt+X");
        assert_eq!(reply.error_code, None);
        assert_eq!(*lock(&live), vec![Binding::DEFAULT]);
    }

    #[test]
    fn start_failure_is_reported_not_hidden() {
        let (service, live) = service(&["Ctrl+Alt+X"], ShellPrefs::default());
        let reply = service.start(noop());
        assert!(reply.enabled && !reply.registered);
        assert_eq!(
            reply.error_code.as_deref(),
            Some("SHOW_HOTKEY_REGISTER_FAILED:1409")
        );
        assert!(lock(&live).is_empty());
    }

    #[test]
    fn remap_replaces_the_old_key_after_the_new_one_registers() {
        let (service, live) = service(&[], ShellPrefs::default());
        service.start(noop());
        let reply = service.set(true, "Ctrl+Shift+K", noop()).unwrap();
        assert_eq!(reply.binding, "Ctrl+Shift+K");
        assert!(reply.registered);
        assert_eq!(*lock(&live), vec![Binding::parse("Ctrl+Shift+K").unwrap()]);
    }

    #[test]
    fn failed_remap_keeps_the_prior_working_key() {
        let (service, live) = service(&["Ctrl+Shift+K"], ShellPrefs::default());
        service.start(noop());
        let error = service.set(true, "Ctrl+Shift+K", noop()).unwrap_err();
        assert_eq!(error, "SHOW_HOTKEY_REGISTER_FAILED:1409");
        let reply = service.reply();
        assert_eq!(reply.binding, "Ctrl+Alt+X");
        assert!(reply.registered);
        assert_eq!(
            reply.error_code.as_deref(),
            Some("SHOW_HOTKEY_REGISTER_FAILED:1409")
        );
        assert_eq!(*lock(&live), vec![Binding::DEFAULT]);
    }

    #[test]
    fn invalid_text_changes_nothing() {
        let (service, live) = service(&[], ShellPrefs::default());
        service.start(noop());
        assert_eq!(
            service.set(true, "Alt+M", noop()).unwrap_err(),
            "SHOW_HOTKEY_RESERVED"
        );
        assert_eq!(service.reply().binding, "Ctrl+Alt+X");
        assert_eq!(*lock(&live), vec![Binding::DEFAULT]);
    }

    #[test]
    fn disable_releases_and_enable_registers_again() {
        let (service, live) = service(&[], ShellPrefs::default());
        service.start(noop());
        let off = service.set(false, "Ctrl+Alt+X", noop()).unwrap();
        assert!(!off.enabled && !off.registered);
        assert!(lock(&live).is_empty());
        let on = service.set(true, "Ctrl+Alt+X", noop()).unwrap();
        assert!(on.enabled && on.registered);
        assert_eq!(*lock(&live), vec![Binding::DEFAULT]);
    }

    #[test]
    fn a_success_clears_an_earlier_error() {
        let (service, _live) = service(&["Ctrl+Alt+X"], ShellPrefs::default());
        assert!(service.start(noop()).error_code.is_some());
        let reply = service.set(true, "Ctrl+Alt+K", noop()).unwrap();
        assert!(reply.registered);
        assert_eq!(reply.error_code, None);
    }

    #[test]
    fn controller_drop_releases_the_registration() {
        let live = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        {
            let mut controller = Controller::new(Fake {
                live: live.clone(),
                ..Fake::default()
            });
            let counter = calls.clone();
            controller
                .apply(
                    true,
                    Binding::DEFAULT,
                    Arc::new(move || {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }),
                )
                .unwrap();
            assert_eq!(lock(&live).len(), 1);
        }
        assert!(lock(&live).is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    /// Native: a real RegisterHotKey on an unlikely key, a second registration of the same key is
    /// refused (the conflict path users hit), and release frees it again.
    #[cfg(windows)]
    #[test]
    fn native_registration_detects_conflict_and_releases() {
        let binding = Binding::parse("Ctrl+Alt+Shift+F11").unwrap();
        let mut first = NativeRegistrar;
        let mut second = NativeRegistrar;
        let handle = first.register(binding, noop()).expect("first registration");
        match second.register(binding, noop()) {
            Err(StartFailure::Registration(code)) => assert_eq!(code, 1409),
            Err(StartFailure::Thread(error)) => panic!("thread error: {error}"),
            Ok(extra) => {
                second.unregister(extra);
                panic!("a held key registered twice");
            }
        }
        first.unregister(handle);
        let again = second
            .register(binding, noop())
            .expect("released key registers again");
        second.unregister(again);
    }

    #[test]
    fn remembered_foreground_is_taken_once() {
        let (service, _live) = service(&[], ShellPrefs::default());
        service.remember_foreground(Some(42));
        assert_eq!(service.take_remembered_foreground(), Some(42));
        assert_eq!(service.take_remembered_foreground(), None);
    }

    #[test]
    fn save_writes_the_canonical_file() {
        let dir = std::env::temp_dir().join(format!("kohbor-shell-prefs-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(FILE_NAME);
        let _ = fs::remove_file(&path);
        let service = Service::with(Some(path.clone()), ShellPrefs::default(), Fake::default());
        service.start(noop());
        service.set(true, "ctrl+shift+k", noop()).unwrap();
        let stored = parse_prefs_or_default(&fs::read(&path).unwrap());
        assert_eq!(stored.show_hotkey.label(), "Ctrl+Shift+K");
        let _ = fs::remove_dir_all(&dir);
    }
}
