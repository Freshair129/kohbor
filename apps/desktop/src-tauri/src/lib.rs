//! Kohbor desktop host (M1 slice): containment, game-state intake, runtime status.

// Demo/Lab-only account scaffolding: parts of the recovery/delete flow are not wired to a command yet.
#[cfg(feature = "demo")]
#[allow(dead_code)]
mod account_host;
mod account_supabase;
pub mod atomic;
mod audio;
mod audio_prefs;
mod capture_host;
mod clock;
mod commands;
pub mod containment;
mod draft_bar;
mod draft_roster;
mod hero_stats;
pub mod host;
mod hotkeys;
mod hud;
pub mod instance;
pub mod intake;
pub mod latency;
mod layout;
pub mod logging;
mod match_history;
mod media;
mod mixer;
mod news;
mod objectives;
mod opendota;
mod orb;
mod portraits;
mod replay_library;
mod analytics;
pub mod secrets;
pub mod settings;
mod show_hotkey;
pub mod status;
mod steam;
mod tray;
mod update;
mod version;

macro_rules! generate_handler_with_updater_commands {
    ($($(#[$meta:meta])* $command:path),* $(,)?) => {{
        #[cfg(feature = "updater")]
        {
            tauri::generate_handler![
                $($(#[$meta])* $command,)*
                commands::update_check,
                commands::update_download,
                commands::update_install
            ]
        }
        #[cfg(not(feature = "updater"))]
        {
            tauri::generate_handler![$($(#[$meta])* $command),*]
        }
    }};
}

/// Runs the application only after containment succeeds.
///
/// On failure, reports a sanitized error containing the failed containment step
/// and returns exit code 1 without launching the UI or sidecar path.
pub fn start_with(
    establish: impl FnOnce() -> Result<(), &'static str>,
    launch: impl FnOnce(),
    report: impl FnOnce(&str),
) -> i32 {
    match establish() {
        Ok(()) => {
            launch();
            0
        }
        Err(step) => {
            report(&format!("CPU_CONTAINMENT_FAILED ({step})"));
            1
        }
    }
}

use containment::{Sample, Sampler, Throttle};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter as TauriEmitter, Listener, Manager};

pub(crate) struct AppState {
    pub(crate) runtime: Arc<intake::Runtime>,
    pub(crate) host: host::Host,
    #[cfg(feature = "updater")]
    pub(crate) updater: update::UpdateController<update::plugin::PluginBackend>,
    #[cfg(not(feature = "updater"))]
    pub(crate) updater: update::UpdateController<update::NullBackend>,
    pub(crate) match_active: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(crate) settings_dir: PathBuf,
    pub(crate) audio_prefs: audio_prefs::AudioPrefsService,
    pub(crate) hotkeys: Mutex<hotkeys::Controller>,
    /// UI-013 show-app hotkey (on by default, remappable).
    pub(crate) show_hotkey: show_hotkey::Service,
    pub(crate) hud: Mutex<hud::Controller>,
    pub(crate) layout: layout::LayoutService,
    pub(crate) audition: commands::AuditionTracker,
    /// Serializes all commands that may replace user-settings.json.
    pub(crate) settings_gate: Mutex<()>,
    pub(crate) sample: Arc<Mutex<(Sample, bool)>>,
    pub(crate) revision: AtomicU64,
    /// MEDIA-009 session volume on its own COM thread.
    pub(crate) mixer: mixer::service::MixerService,
    /// UI-009 / UI-011 edge orb (off unless enabled in Settings).
    pub(crate) orb: orb::host::OrbHost,
    /// Default-off public statistics and the native draft column.
    pub(crate) hero_stats: hero_stats::host::HeroStatsHost,
    pub(crate) objectives: objectives::ObjectiveHost,
    /// OVERVIEW-002 local own-match history (written by intake, read by the Overview).
    pub(crate) match_history: match_history::MatchHistoryStore,
    /// Opt-in, own-account OpenDota cache; never populated while consent is off.
    pub(crate) opendota: opendota::OpenDotaService,
    /// OVERVIEW-003 community news (one fixed GitHub file, on by default, off switch).
    pub(crate) news: news::NewsService,
}

impl host::Emitter for tauri::AppHandle<tauri::Wry> {
    fn emit(&self, event: &str, payload: Value) {
        let _ = TauriEmitter::emit(self, event, payload);
    }
}

/// The application data directory. Demo builds may redirect it with `KOHBOR_DATA_DIR` (only together
/// with `KOHBOR_DEMO=1`) so the demo never touches the real packs; release builds ignore the variable.
fn data_dir(app: &tauri::App<tauri::Wry>) -> tauri::Result<std::path::PathBuf> {
    #[cfg(any(feature = "demo", debug_assertions))]
    {
        if let Some(dir) = demo_data_dir_override() {
            std::fs::create_dir_all(&dir)?;
            demo::seed_test_token(&dir);
            return Ok(dir);
        }
    }
    app.path().app_data_dir()
}

#[cfg(any(feature = "demo", debug_assertions))]
fn demo_data_dir_override() -> Option<std::path::PathBuf> {
    let demo = std::env::var("KOHBOR_DEMO").ok();
    let requested = std::env::var("KOHBOR_DATA_DIR").ok();
    demo::data_dir_override(demo.as_deref(), requested.as_deref())
}

#[cfg(not(any(feature = "demo", debug_assertions)))]
fn demo_data_dir_override() -> Option<std::path::PathBuf> {
    None
}

/// Initialise logging before the instance guard or Tauri setup can fail. On Windows,
/// `app.path().app_data_dir()` resolves the roaming data root plus the configured bundle
/// identifier; the demo override follows the same rule as `data_dir()` below.
pub fn initialize_logging() {
    let data_dir = demo_data_dir_override().or_else(|| {
        let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let identifier = context.config().identifier.clone();
        #[cfg(windows)]
        let root = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
        #[cfg(not(windows))]
        let root = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .map(|home| home.join(".local").join("share"))
            });
        root.map(|path| path.join(identifier))
    });
    logging::initialize(data_dir.as_deref());
}

/// Place the overlay on the primary monitor. Returns false (and the caller keeps the window hidden)
/// when the monitor or the geometry cannot be applied. Called again before every show so a changed
/// display layout is followed.
fn place_overlay_window(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    let monitor = match window.primary_monitor() {
        Ok(Some(monitor)) => monitor,
        Ok(None) => {
            crate::log_warn!("overlay", "placement_unavailable", "no primary monitor");
            return false;
        }
        Err(error) => {
            crate::log_warn!("overlay", "placement_unavailable", format!("{error}"));
            return false;
        }
    };
    let position = monitor.position();
    let size = monitor.size();
    // Fail closed when the (dev) layout disables the banner: the window stays hidden.
    let Some(rect) = overlay::place_overlay_with(
        &overlay::MonitorRect {
            x: position.x,
            y: position.y,
            w: size.width,
            h: size.height,
        },
        &overlay::effective_banner_layout(),
    ) else {
        return false;
    };
    let moved = window.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
    let sized = window.set_size(tauri::PhysicalSize::new(rect.w, rect.h));
    if let Err(error) = moved.as_ref().and(sized.as_ref()) {
        crate::log_warn!("overlay", "placement_failed", format!("{error}"));
        return false;
    }
    true
}

/// WebView2 arguments shared by every window (WebView2 requires equal arguments for windows that
/// share one data directory). They keep Chromium from throttling or pausing the Live minimap when the
/// window is behind the borderless game. Must equal `additionalBrowserArgs` in tauri.conf.json.
const WEBVIEW2_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows";

struct OverlayWindowBundle {
    window: tauri::WebviewWindow<tauri::Wry>,
    visibility: overlay::OverlayVisibilityMachine,
}

/// Tauri's `hwnd()` comes from its own `windows` crate version; rebuild it as this crate's `HWND`.
#[cfg(windows)]
fn overlay_hwnd(
    window: &tauri::WebviewWindow<tauri::Wry>,
) -> Option<windows::Win32::Foundation::HWND> {
    window
        .hwnd()
        .ok()
        .map(|hwnd| windows::Win32::Foundation::HWND(hwnd.0))
}

#[cfg(windows)]
fn overlay_has_no_activate_style(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };

    let Some(hwnd) = overlay_hwnd(window) else {
        return false;
    };
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
    ex_style & WS_EX_NOACTIVATE.0 != 0
}

#[cfg(windows)]
fn overlay_is_foreground(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    overlay_hwnd(window).is_some_and(|hwnd| unsafe { GetForegroundWindow() } == hwnd)
}

/// Map a transparent in-game window without making it the active window.
///
/// `WebviewWindow::show` uses `ShowWindow(SW_SHOW)`, which activates. Right after Kohbor starts, Windows lets the
/// new process take the foreground, so the overlay could steal keyboard focus from Dota at launch
/// (`activation_detected` on start-up). `SW_SHOWNOACTIVATE` shows the window and leaves the foreground alone.
#[cfg(windows)]
pub(crate) fn show_without_activation(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, ShowWindow, SW_SHOWNOACTIVATE};

    let Some(hwnd) = overlay_hwnd(window) else {
        return false;
    };
    unsafe {
        // The return value is the previous visibility, not an error.
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        IsWindowVisible(hwnd).as_bool()
    }
}

/// Non-Windows builds have no foreground race to avoid; keep the plain show.
#[cfg(not(windows))]
pub(crate) fn show_without_activation(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    window.show().is_ok()
}

#[cfg(windows)]
fn raise_overlay_without_activation(window: &tauri::WebviewWindow<tauri::Wry>) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    let Some(hwnd) = overlay_hwnd(window) else {
        return false;
    };
    // Keep the mapped transparent HWND above other topmost windows without activating it.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )
        .is_ok()
    }
}

fn build_overlay_window(app: &mut tauri::App<tauri::Wry>) -> Option<OverlayWindowBundle> {
    let spec = overlay::overlay_window_spec();
    let window = match tauri::WebviewWindowBuilder::new(
        app,
        spec.label,
        tauri::WebviewUrl::App(spec.url.into()),
    )
    .transparent(spec.transparent)
    .decorations(spec.decorations)
    .always_on_top(spec.always_on_top)
    .skip_taskbar(spec.skip_taskbar)
    .focusable(spec.focusable)
    .focused(spec.focused)
    .shadow(spec.shadow)
    .resizable(spec.resizable)
    .visible(spec.visible)
    .additional_browser_args(WEBVIEW2_BROWSER_ARGS)
    .inner_size(
        f64::from(overlay::WINDOW_LOGICAL.0),
        f64::from(overlay::WINDOW_LOGICAL.1),
    )
    .build()
    {
        Ok(window) => window,
        Err(error) => {
            crate::log_warn!("overlay", "window_unavailable", format!("{error}"));
            return None;
        }
    };

    // Placement is repeated before every content show; the first Windows mapping also needs a valid target.
    let _placement_succeeded = place_overlay_window(&window);
    let mut visibility = overlay::OverlayVisibilityMachine::default();
    #[cfg(windows)]
    if !_placement_succeeded {
        crate::log_warn!(
            "overlay",
            "window_mapping_skipped",
            "overlay mapping skipped",
            reason = "PLACEMENT_UNAVAILABLE"
        );
        return None;
    }

    if let Err(error) = window.set_ignore_cursor_events(spec.ignore_cursor_events) {
        crate::log_warn!("overlay", "click_through_unavailable", format!("{error}"));
        #[cfg(windows)]
        return None;
    }

    #[cfg(windows)]
    {
        let action = visibility.request(overlay::OverlayVisibilityRequest::MapAtStartup);
        if action != overlay::OverlayVisibilityAction::MapWindow {
            crate::log_warn!(
                "overlay",
                "window_mapping_skipped",
                "overlay mapping skipped",
                reason = "INVALID_WINDOW_STATE"
            );
            return None;
        }
        if !overlay_has_no_activate_style(&window) {
            visibility.complete(action, false);
            crate::log_warn!(
                "overlay",
                "no_activate_unavailable",
                "overlay mapping skipped",
                reason = "WS_EX_NOACTIVATE_MISSING"
            );
            return None;
        }
        let failure = if !show_without_activation(&window) {
            Some("SHOW_FAILED")
        } else if !window.is_visible().unwrap_or(false) {
            Some("NOT_VISIBLE_AFTER_SHOW")
        } else if !overlay_has_no_activate_style(&window) {
            Some("WS_EX_NOACTIVATE_LOST")
        } else {
            None
        };
        if let Some(reason) = failure {
            visibility.complete(action, false);
            crate::log_warn!(
                "overlay",
                "window_map_failed",
                "overlay mapping failed",
                reason = reason
            );
            return None;
        }
        if overlay_is_foreground(&window) {
            // At app start Windows may hand the foreground to our newest window. That is not a
            // reason to give up on banners; record it so the live log shows it.
            crate::log_info!(
                "overlay",
                "mapped_while_foreground",
                "overlay mapped while it was the foreground window"
            );
        }
        visibility.complete(action, true);
        crate::log_info!(
            "overlay",
            "window_mapped",
            "overlay mapped once with no activation",
            no_activate = true,
            focused = false,
            focusable = false
        );
    }

    Some(OverlayWindowBundle { window, visibility })
}

#[cfg(windows)]
fn emit_overlay_content_visibility(window: &tauri::WebviewWindow<tauri::Wry>, visible: bool) {
    if TauriEmitter::emit(
        window,
        "overlay://content-visibility",
        serde_json::json!({ "visible": visible }),
    )
    .is_err()
    {
        crate::log_warn!(
            "overlay",
            "content_visibility_ack_failed",
            "renderer visibility acknowledgement failed",
            content_visible = visible
        );
    }
}

#[cfg(windows)]
static OVERLAY_VISIBILITY_DIAGNOSTIC_SHOWS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(windows)]
fn reserve_overlay_visibility_diagnostic() -> Option<usize> {
    use std::sync::atomic::Ordering;

    OVERLAY_VISIBILITY_DIAGNOSTIC_SHOWS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            (count < 20).then_some(count + 1)
        })
        .ok()
        .map(|count| count + 1)
}

#[cfg(windows)]
fn window_is_topmost(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOPMOST};

    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 }
}

#[cfg(windows)]
fn overlay_window_above_description(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindow, GetWindowThreadProcessId, GW_HWNDPREV,
    };

    let above = match unsafe { GetWindow(hwnd, GW_HWNDPREV) } {
        Ok(above) => above,
        Err(_) => return "unavailable".to_owned(),
    };
    if above == HWND::default() {
        return "none".to_owned();
    }

    let topmost = window_is_topmost(above);
    let mut process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(above, Some(&mut process_id));
    }
    if process_id != std::process::id() {
        return format!("other,topmost={topmost}");
    }

    let mut class_name = [0u16; 128];
    let length = unsafe { GetClassNameW(above, &mut class_name) }.max(0) as usize;
    let class_name = if length == 0 {
        "unknown".to_owned()
    } else {
        String::from_utf16_lossy(&class_name[..length])
    };
    format!("own:{class_name},topmost={topmost}")
}

#[cfg(windows)]
fn overlay_visible_topmost_rank(target: windows::Win32::Foundation::HWND) -> Option<usize> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetTopWindow, GetWindow, IsWindowVisible, GW_HWNDNEXT,
    };

    let mut current = unsafe { GetTopWindow(None).ok() }?;
    let mut rank = 0usize;
    // A broken z-order chain must not turn diagnostics into an unbounded walk.
    for _ in 0..4096 {
        if unsafe { IsWindowVisible(current) }.as_bool() && window_is_topmost(current) {
            rank += 1;
            if current == target {
                return Some(rank);
            }
        }
        current = match unsafe { GetWindow(current, GW_HWNDNEXT) } {
            Ok(next) if next != windows::Win32::Foundation::HWND::default() => next,
            _ => break,
        };
    }
    None
}

#[cfg(windows)]
fn log_overlay_visibility_snapshot(
    window: &tauri::WebviewWindow<tauri::Wry>,
    sample: usize,
    at: &'static str,
) {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowLongPtrW, IsWindowVisible, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };

    let Some(hwnd) = overlay_hwnd(window) else {
        crate::log_info!(
            "overlay",
            "visibility_snapshot",
            format!("sample={sample} at={at} visible=unknown cloaked=unknown foreground=unknown visible_topmost_rank=unknown above=unknown no_activate=unknown")
        );
        return;
    };
    let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
    let foreground = unsafe { GetForegroundWindow() } == hwnd;
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
    let no_activate = ex_style & WS_EX_NOACTIVATE.0 != 0;
    let mut cloaked_value = 0u32;
    let cloaked = if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked_value as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    }
    .is_ok()
    {
        if cloaked_value == 0 {
            "false"
        } else {
            "true"
        }
    } else {
        "unknown"
    };
    let rank = overlay_visible_topmost_rank(hwnd)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "not_found".to_owned());
    let above = overlay_window_above_description(hwnd);

    crate::log_info!(
        "overlay",
        "visibility_snapshot",
        format!(
            "sample={sample} at={at} visible={visible} cloaked={cloaked} foreground={foreground} visible_topmost_rank={rank} above={above} no_activate={no_activate}"
        )
    );
}

#[cfg(windows)]
fn schedule_overlay_visibility_diagnostics(window: &tauri::WebviewWindow<tauri::Wry>) {
    let Some(sample) = reserve_overlay_visibility_diagnostic() else {
        return;
    };
    log_overlay_visibility_snapshot(window, sample, "content-show");

    let delayed_window = window.clone();
    if std::thread::Builder::new()
        .name("overlay-visibility-snapshot".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            log_overlay_visibility_snapshot(&delayed_window, sample, "content-show+500ms");
        })
        .is_err()
    {
        crate::log_warn!(
            "overlay",
            "visibility_snapshot_schedule_failed",
            "delayed visibility snapshot unavailable",
            sample = sample
        );
    }
}

/// Place the HUD strip at the monitor origin and size it to the monitor's physical width.
pub(crate) fn place_hud_window(
    window: &tauri::WebviewWindow<tauri::Wry>,
    layouts: &[(hud::HudModule, hud::HudModuleLayout); 6],
) -> bool {
    place_hud_window_with_card(
        window,
        layouts,
        layout::WidgetLayout {
            x_percent: 62.5,
            y_percent: 6.5,
            scale: 1.0,
        },
    )
}

pub(crate) fn place_hud_window_with_card(
    window: &tauri::WebviewWindow<tauri::Wry>,
    layouts: &[(hud::HudModule, hud::HudModuleLayout); 6],
    card: layout::WidgetLayout,
) -> bool {
    let monitor = match window.primary_monitor() {
        Ok(Some(monitor)) => monitor,
        Ok(None) | Err(_) => {
            crate::log_warn!("hud", "placement_unavailable", "HUD monitor unavailable");
            return false;
        }
    };
    let position = monitor.position();
    let size = monitor.size();
    let rect = hud::place_hud_window_with_card(
        &overlay::MonitorRect {
            x: position.x,
            y: position.y,
            w: size.width,
            h: size.height,
        },
        layouts,
        card,
    );
    let moved = window.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
    let sized = window.set_size(tauri::PhysicalSize::new(rect.w, rect.h));
    if moved.is_err() || sized.is_err() {
        crate::log_warn!("hud", "placement_failed", "HUD placement failed");
        return false;
    }
    true
}

fn build_hud_window(app: &mut tauri::App<tauri::Wry>) -> Option<tauri::WebviewWindow<tauri::Wry>> {
    #[cfg(any(feature = "lab", debug_assertions))]
    lab::initialize_saved(app.handle());
    let spec = hud::hud_window_spec();
    let window = match tauri::WebviewWindowBuilder::new(
        app,
        spec.label,
        tauri::WebviewUrl::App(spec.url.into()),
    )
    .transparent(spec.transparent)
    .decorations(spec.decorations)
    .always_on_top(spec.always_on_top)
    .skip_taskbar(spec.skip_taskbar)
    .focusable(spec.focusable)
    .focused(spec.focused)
    .shadow(spec.shadow)
    .resizable(spec.resizable)
    .visible(spec.visible)
    .additional_browser_args(WEBVIEW2_BROWSER_ARGS)
    .inner_size(1920.0, 108.0)
    .build()
    {
        Ok(window) => window,
        Err(_) => {
            crate::log_warn!("hud", "window_unavailable", "HUD window unavailable");
            return None;
        }
    };

    let layouts = hud::effective_hud_layouts();
    let _ = place_hud_window(&window, &layouts);
    if window
        .set_ignore_cursor_events(spec.ignore_cursor_events)
        .is_err()
    {
        let _ = window.hide();
        crate::log_warn!("hud", "click_through_unavailable", "HUD remains hidden");
    }
    Some(window)
}

fn register_overlay_events(
    app: tauri::AppHandle<tauri::Wry>,
    window: Option<OverlayWindowBundle>,
    last_status: Arc<Mutex<Value>>,
    audition: commands::AuditionTracker,
) {
    #[cfg(windows)]
    let (visible_window, visibility_state) = match window {
        Some(bundle) => (Some(bundle.window), Arc::new(Mutex::new(bundle.visibility))),
        None => (
            None,
            Arc::new(Mutex::new(overlay::OverlayVisibilityMachine::default())),
        ),
    };
    #[cfg(not(windows))]
    let visible_window = window.map(|bundle| bundle.window);

    app.listen("overlay://visible", move |event| {
        let Ok(payload) = serde_json::from_str::<Value>(event.payload()) else {
            return;
        };
        let Some(values) = payload.as_object() else {
            return;
        };
        if values.len() != 1 {
            return;
        }
        let Some(visible) = values.get("visible").and_then(Value::as_bool) else {
            return;
        };
        let Some(window) = visible_window.as_ref() else {
            if visible {
                crate::log_info!(
                    "overlay",
                    "show_skipped",
                    "overlay window show skipped",
                    reason = "WINDOW_UNAVAILABLE"
                );
            } else {
                #[cfg(windows)]
                crate::log_info!(
                    "overlay",
                    "content_hidden",
                    "overlay content hidden; window unavailable",
                    reason = "WINDOW_UNAVAILABLE"
                );
                #[cfg(not(windows))]
                crate::log_info!(
                    "overlay",
                    "hide_skipped",
                    "overlay window hide skipped",
                    reason = "WINDOW_UNAVAILABLE"
                );
            }
            return;
        };
        if !visible {
            #[cfg(windows)]
            {
                let action = visibility_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .request(overlay::OverlayVisibilityRequest::ContentHidden);
                if action == overlay::OverlayVisibilityAction::ClearContent {
                    emit_overlay_content_visibility(window, false);
                }
                crate::log_info!(
                    "overlay",
                    "content_hidden",
                    "overlay content is empty; transparent window remains mapped"
                );
            }
            #[cfg(not(windows))]
            {
                if let Err(error) = window.hide() {
                    crate::log_warn!("overlay", "hide_failed", format!("{error}"));
                    crate::log_info!(
                        "overlay",
                        "window_hide_failed",
                        "overlay window hide failed",
                        reason = "WINDOW_OPERATION_FAILED"
                    );
                } else {
                    crate::log_info!("overlay", "window_hidden", "overlay window hidden");
                }
            }
            return;
        }
        // Always place and verify click-through before the native raise/show operation.
        if !place_overlay_window(window) {
            #[cfg(windows)]
            {
                emit_overlay_content_visibility(window, false);
            }
            #[cfg(not(windows))]
            {
                let _ = window.hide();
            }
            crate::log_info!(
                "overlay",
                "show_skipped",
                "overlay window show skipped",
                reason = "PLACEMENT_UNAVAILABLE"
            );
            return;
        }
        if window.set_ignore_cursor_events(true).is_err() {
            #[cfg(windows)]
            {
                emit_overlay_content_visibility(window, false);
            }
            #[cfg(not(windows))]
            {
                let _ = window.hide();
            }
            crate::log_info!(
                "overlay",
                "show_skipped",
                "overlay window show skipped",
                reason = "CLICK_THROUGH_UNAVAILABLE"
            );
            return;
        }

        #[cfg(windows)]
        {
            let action = visibility_state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .request(overlay::OverlayVisibilityRequest::ContentShown);
            if action != overlay::OverlayVisibilityAction::RaiseWindow {
                emit_overlay_content_visibility(window, false);
                crate::log_info!(
                    "overlay",
                    "show_skipped",
                    "overlay content show skipped; keeping the failure closed",
                    reason = "WINDOW_NOT_MAPPED"
                );
                return;
            }
            if !overlay_has_no_activate_style(window) {
                visibility_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .complete(action, false);
                emit_overlay_content_visibility(window, false);
                crate::log_warn!(
                    "overlay",
                    "no_activate_lost",
                    "overlay no-activate style is missing"
                );
                crate::log_info!(
                    "overlay",
                    "content_show_failed",
                    "overlay content show failed",
                    reason = "WS_EX_NOACTIVATE_MISSING"
                );
                return;
            }
            if !window.is_visible().unwrap_or(false) {
                visibility_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .complete(action, false);
                emit_overlay_content_visibility(window, false);
                crate::log_warn!(
                    "overlay",
                    "window_map_lost",
                    "mapped overlay window is no longer visible"
                );
                crate::log_info!(
                    "overlay",
                    "content_show_failed",
                    "overlay content show failed",
                    reason = "WINDOW_NOT_MAPPED"
                );
                return;
            }
            if !raise_overlay_without_activation(window) {
                visibility_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .complete(action, false);
                emit_overlay_content_visibility(window, false);
                crate::log_warn!(
                    "overlay",
                    "zorder_reassert_failed",
                    "overlay raise failed",
                    reason = "SET_WINDOW_POS_FAILED"
                );
                crate::log_info!(
                    "overlay",
                    "content_show_failed",
                    "overlay content show failed",
                    reason = "ZORDER_REASSERT_FAILED"
                );
                return;
            }
            if overlay_is_foreground(window) {
                visibility_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .complete(action, false);
                emit_overlay_content_visibility(window, false);
                crate::log_warn!(
                    "overlay",
                    "activation_detected",
                    "overlay became the foreground window"
                );
                crate::log_info!(
                    "overlay",
                    "content_show_failed",
                    "overlay content show failed",
                    reason = "OVERLAY_FOREGROUND"
                );
                return;
            }
            visibility_state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .complete(action, true);
            emit_overlay_content_visibility(window, true);
            crate::log_info!("overlay", "content_shown", "overlay content is visible");
            schedule_overlay_visibility_diagnostics(window);
        }

        #[cfg(not(windows))]
        {
            if let Err(error) = window.show() {
                crate::log_warn!("overlay", "show_failed", format!("{error}"));
                let _ = window.hide();
                crate::log_info!(
                    "overlay",
                    "window_show_failed",
                    "overlay window show failed",
                    reason = "WINDOW_OPERATION_FAILED"
                );
                return;
            }
            if window.set_ignore_cursor_events(true).is_err() {
                let _ = window.hide();
                crate::log_warn!(
                    "overlay",
                    "click_through_failed",
                    "window hidden after failure"
                );
                crate::log_info!(
                    "overlay",
                    "window_show_failed",
                    "overlay window show failed",
                    reason = "CLICK_THROUGH_UNAVAILABLE"
                );
            } else {
                crate::log_info!("overlay", "window_shown", "overlay window shown");
            }
        }
    });

    let playback_audition = audition.clone();
    app.listen("playback://event", move |event| {
        if let Ok(payload) = serde_json::from_str::<Value>(event.payload()) {
            playback_audition.observe_playback(&payload);
        }
    });

    app.listen("overlay://video-diagnostic", move |event| {
        let Ok(payload) = serde_json::from_str::<Value>(event.payload()) else {
            return;
        };
        let Some(diagnostic) = overlay::parse_and_log_video_diagnostic(&payload, |diagnostic| {
            let event_type = diagnostic.event_type.as_deref().unwrap_or("unknown");
            match diagnostic.outcome {
                overlay::VideoDiagnosticOutcome::Started => crate::log_info!(
                    "overlay",
                    diagnostic.log_event_name(),
                    diagnostic.log_message(),
                    playback_id = diagnostic.playback_id.as_str(),
                    event_type = event_type,
                    reason_code = diagnostic.reason_code
                ),
                overlay::VideoDiagnosticOutcome::Failed => crate::log_info!(
                    "overlay",
                    diagnostic.log_event_name(),
                    diagnostic.log_message(),
                    playback_id = diagnostic.playback_id.as_str(),
                    event_type = event_type,
                    reason_code = diagnostic.reason_code,
                    error_code_known = diagnostic.error_code.is_some(),
                    error_code = diagnostic.error_code.unwrap_or_default()
                ),
            }
        }) else {
            return;
        };
        audition.observe_visual(diagnostic);
    });

    let ready_handle = app.clone();
    app.listen("overlay://ready", move |event| {
        let Ok(payload) = serde_json::from_str::<Value>(event.payload()) else {
            return;
        };
        if !payload.as_object().is_some_and(|values| values.is_empty()) {
            return;
        }
        let payload = intake::lock(&last_status).clone();
        if let Err(error) = ready_handle.emit("overlay://status", payload) {
            crate::log_warn!("overlay", "status_reply_failed", format!("{error}"));
        }
    });
}

fn start_overlay_status_mirror(
    app: tauri::AppHandle<tauri::Wry>,
    last_status: Arc<Mutex<Value>>,
    initial_status: Value,
) {
    let app_for_thread = app.clone();
    let last_for_thread = last_status.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("overlay-status-mirror".into())
        .spawn(move || {
            let mut previous = initial_status;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let state = app_for_thread.state::<AppState>();
                if state.host.shutdown_started() {
                    break;
                }
                let (media_state, reason) = state.host.media_status();
                let current = overlay::status_payload(media_state, reason);

                if current != previous {
                    previous = current.clone();
                    *intake::lock(&last_for_thread) = current.clone();
                    if let Err(error) = app_for_thread.emit("overlay://status", current) {
                        crate::log_warn!("overlay", "status_update_failed", format!("{error}"));
                    }
                }
            }
        })
    {
        crate::log_warn!("overlay", "status_mirror_unavailable", format!("{error}"));
    }
    // Deliver the current mirror once at startup. `overlay://ready` replays it
    // after the overlay webview has installed its listener.
    if let Err(error) = app.emit("overlay://status", intake::lock(&last_status).clone()) {
        crate::log_warn!(
            "overlay",
            "initial_status_update_failed",
            format!("{error}")
        );
    }
}

/// Closing the main (control) window hides it to the tray when the tray icon exists (owner request
/// 2026-11-19; see `tray.rs`); without a tray icon it ends the process. The hidden overlay window
/// must never keep the app alive.
fn closing_control_window_exits_app(label: &str) -> bool {
    label == "control"
}

#[cfg(test)]
mod window_close_tests {
    use super::closing_control_window_exits_app;

    #[test]
    fn only_the_control_window_close_ends_the_app() {
        assert!(closing_control_window_exits_app("control"));
        assert!(!closing_control_window_exits_app("overlay"));
        assert!(!closing_control_window_exits_app(""));
    }

    #[test]
    fn the_exit_path_logs_a_final_process_exit_line_after_shutdown() {
        let source = include_str!("lib.rs");
        let shutdown = source.rfind("host.shutdown();").unwrap();
        let line = source.rfind("\"process_exit\"").unwrap();
        assert!(line > shutdown);
    }

    #[test]
    fn the_control_window_label_matches_the_tauri_config() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(config["app"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|window| window["label"] == "control"));
    }
}

pub fn run() {
    initialize_logging();
    let app_version = version::AppVersion::current();
    let commit = app_version
        .git_commit
        .as_deref()
        .map(|value| value.chars().take(7).collect::<String>());
    crate::log_info!(
        "app",
        "started",
        "Kohbor started",
        version = app_version.version,
        commit = commit,
        profile = app_version.profile,
        demo = app_version.demo,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH
    );
    let builder = tauri::Builder::default()
        .on_window_event(|window, event| {
            // Exit-path breadcrumbs (fixed strings only): a log that ends without any of these
            // means the process was terminated from outside, not closed by the app.
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if closing_control_window_exits_app(window.label())
                        && window.app_handle().tray_by_id(tray::TRAY_ID).is_some()
                    {
                        api.prevent_close();
                        let _ = window.hide();
                        crate::log_info!(
                            "app",
                            "hidden_to_tray",
                            "control window hidden to the tray"
                        );
                        return;
                    }
                    crate::log_info!("app", "app_stopping", "window close requested");
                    logging::flush();
                    // The overlay is a second, hidden window: without an explicit exit it keeps
                    // the event loop (and the audio engine) alive after the control window closes.
                    if closing_control_window_exits_app(window.label()) {
                        window.app_handle().exit(0);
                    }
                }
                tauri::WindowEvent::Destroyed => {
                    let _ = window;
                    crate::log_info!("app", "app_stopping", "window destroyed");
                }
                _ => {}
            }
        })
        .register_asynchronous_uri_scheme_protocol("kohbor-media", commands::media_protocol_handler)
        .setup(|app| {
            // The control window title carries the version (and ` · LAB` in Lab builds) so the owner
            // can tell which exe is running; tauri.conf.json keeps the plain title as the fallback.
            // instance.rs finds the existing window by the title prefix for second-instance focus.
            if let Some(window) = app.get_webview_window("control") {
                let title = version::control_window_title(
                    version::AppVersion::current().version,
                    cfg!(feature = "lab"),
                );
                if window.set_title(&title).is_err() {
                    crate::log_warn!("app", "title_unavailable", "control window title not set");
                }
                if tray::install(app, &title).is_err() {
                    // Without a tray icon, closing the control window ends the process as before.
                    crate::log_warn!("app", "tray_unavailable", "tray icon not created");
                }
            }
            let dir = data_dir(app).inspect_err(|_error| {
                crate::log_error!("app", "setup_failed", "APP_DATA_DIR_UNAVAILABLE");
            })?;
            let settings_dir = dir.clone();
            let layout = layout::LayoutService::open(&dir);
            let orb_host = orb::host::OrbHost::open(&dir);
            let orb_prefs = orb_host.prefs();
            let mixer_dir = dir.clone();
            // Starts the mixer thread; originals left by an abnormal exit are restored before any command.
            let mixer_service = mixer::service::MixerService::start(move || {
                let mut mixer = mixer::Mixer::new(
                    mixer::platform_backend(),
                    Box::new(mixer::JsonOriginalStore::in_data_dir(&mixer_dir)),
                );
                let _ = mixer.configure(orb_prefs.slots, orb_prefs.focus_percent);
                mixer
            });
            // Load volume preferences before Host::start creates the audio engine.
            let audio_prefs = audio_prefs::AudioPrefsService::open(&dir);
            let show_hotkey = show_hotkey::Service::open(&dir);
            let objective_host = objectives::ObjectiveHost::open(&dir, &dir.join("logs"));
            let objective_wiring = objective_host.intake_wiring();
            // Account storage remains lazy so an unavailable account root or
            // provider cannot delay the independent local core startup.
            #[cfg(feature = "demo")]
            app.manage(account_host::DesktopAccountHost::new(settings_dir.clone()));
            let (runtime, objective_alert_receiver) =
                intake::Runtime::with_objectives(dir.clone(), objective_wiring);
            let sample = Arc::new(Mutex::new((
                Sample {
                    cpu_percent: None,
                    working_set_bytes: None,
                    process_count: None,
                },
                true,
            )));
            let emitter: Arc<dyn host::Emitter> = Arc::new(app.handle().clone());
            let host = host::Host::start(dir, runtime.clone(), sample.clone(), emitter)
                .inspect_err(|_error| {
                    crate::log_error!("app", "setup_failed", "HOST_SETUP_FAILED");
                })?;
            let overlay_window = build_overlay_window(app);
            let hud_window = build_hud_window(app);
            let hud_runtime = runtime.clone();
            let (initial_media_state, initial_reason) = host.media_status();
            let initial_overlay_status =
                overlay::status_payload(initial_media_state, initial_reason);
            let sink = sample.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("resource-sampler".into())
                .spawn(move || {
                    let mut sampler = Sampler::new();
                    let mut throttle = Throttle::default();
                    let mut window = containment::ResourceWindow::default();
                    loop {
                        let s = sampler.sample();
                        let t = throttle.update(&s);
                        *intake::lock(&sink) = (s, t);
                        if let Some(r) = window.push(&s, t) {
                            // Diagnostic only (owner request 2026-11-19): job-wide CPU against the
                            // 10% hard cap, to explain capture fps drops. Never used to admit work.
                            crate::log_info!(
                                "resources",
                                "summary",
                                "resource 30 second summary",
                                samples = r.samples,
                                cpu_missing = r.cpu_missing,
                                cpu_mean = r.cpu_mean,
                                cpu_max = r.cpu_max,
                                cpu_cap_percent = f64::from(containment::CPU_RATE_HARD_CAP) / 100.0,
                                near_cap_samples = r.near_cap_samples,
                                throttled_samples = r.throttled_samples,
                                working_set_mb_max = r.working_set_mb_max,
                                process_count_max = r.process_count_max
                            );
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                })
            {
                crate::log_warn!("resources", "sampler_unavailable", format!("{error}"));
            }
            let gsi_runtime = runtime.clone();
            #[cfg(feature = "lab")]
            let gsi = capture_host::GsiView::with_minimap_recorder(move || {
                let now = std::time::Instant::now();
                let shared = intake::lock(&gsi_runtime.shared);
                let snapshot = shared.capture_gsi_snapshot_at(now);
                let recorder = crate::lab::minimap_recorder::recording_enabled()
                    .then(|| shared.minimap_recorder_snapshot_at(now))
                    .flatten();
                (snapshot, recorder)
            });
            #[cfg(not(feature = "lab"))]
            let gsi = capture_host::GsiView::new(move || {
                intake::lock(&gsi_runtime.shared).capture_gsi_snapshot_at(std::time::Instant::now())
            });
            let resource_sample = sample.clone();
            let resources = capture_host::ResourceView::new(move || {
                let (s, _) = *intake::lock(&resource_sample);
                capture_host::ResourceSample {
                    cpu_percent: s.cpu_percent,
                    working_set_bytes: s.working_set_bytes,
                }
            });
            capture_host::start(app.handle().clone(), gsi, resources, settings_dir.clone());
            // The reader exposes only the exact GSI screen and the fields needed by its gate.
            let draft_window = hero_stats::host::create_window(app);
            let draft_runtime = runtime.clone();
            let hero_stats_host = hero_stats::host::HeroStatsHost::start(
                app.handle().clone(),
                draft_window,
                &settings_dir,
                Arc::new(move || {
                    intake::lock(&draft_runtime.shared)
                        .draft_column_observation_at(std::time::Instant::now())
                }),
                Arc::new(capture_host::draft_bar_snapshot),
            );
            let steam_runtime = runtime.clone();
            // Keep replay analysis blocked through post-game; automatic analysis is allowed only
            // after the runtime returns to its no-match phase, as well as by manual request.
            let update_match_active: Arc<dyn Fn() -> bool + Send + Sync> = {
                let runtime = runtime.clone();
                Arc::new(move || {
                    matches!(
                        intake::lock(&runtime.shared).phase,
                        Some(
                            kohbor_gsi::admission::Phase::Draft
                                | kohbor_gsi::admission::Phase::Pregame
                                | kohbor_gsi::admission::Phase::Live
                                | kohbor_gsi::admission::Phase::PostGame
                        )
                    )
                })
            };
            let account_auth = account_supabase::SupabaseAuth::new(
                settings_dir.clone(),
                update_match_active.clone(),
            );
            {
                let bundled_engine = {
                    let mut candidates = Vec::new();
                    if let Ok(resources) = app.path().resource_dir() {
                        candidates.push(resources.join("binaries").join("replay-engine-rs.exe"));
                        candidates.push(resources.join("replay-engine-rs.exe"));
                    }
                    if let Ok(executable) = std::env::current_exe() {
                        if let Some(directory) = executable.parent() {
                            candidates
                                .push(directory.join("binaries").join("replay-engine-rs.exe"));
                            candidates.push(directory.join("replay-engine-rs.exe"));
                        }
                    }
                    candidates.into_iter().find(|path| path.is_file())
                };
                let event_app = app.handle().clone();
                let library = replay_library::ReplayLibrary::open(
                    settings_dir.join("replays"),
                    bundled_engine,
                    update_match_active.clone(),
                    Arc::new(move || {
                        let _ = event_app.emit(
                            replay_library::EVENT_NAME,
                            serde_json::json!({ "changed": true }),
                        );
                    }),
                    replay_library::ReplayLibraryLimits::default(),
                );
                app.manage(library);
            }
            app.manage(account_auth.clone());
            account_auth.start_refresh_loop();
            let match_history = intake::lock(&runtime.shared).match_history();
            let install_key = secrets::load_or_create(
                &settings_dir.join("install-key.bin"),
                || -> Option<Vec<u8>> { None },
            )
            .and_then(|bytes| bytes.try_into().ok());
            let opendota = opendota::OpenDotaService::open(&settings_dir, install_key);
            {
                let analytics = analytics::AnalyticsService::open(
                    &settings_dir,
                    install_key,
                    Arc::new(analytics::WinHttpTransport::default()),
                );
                if let Ok(prefs) = analytics.preferences() {
                    let library = app.state::<replay_library::ReplayLibrary>();
                    library.set_auto_analyse(prefs.auto_analyse);
                    let _ = library.set_limits(replay_library::ReplayLibraryLimits {
                        max_matches: prefs.keep_newest,
                        max_bytes: prefs.max_bytes,
                    });
                }
                app.manage(analytics);
            }
            let news = news::NewsService::open(&settings_dir);
            news.start();
            app.manage(AppState {
                runtime,
                host,
                #[cfg(feature = "updater")]
                updater: update::UpdateController::new(
                    update::UPDATE_ENDPOINT,
                    update::UPDATE_PUBKEY,
                    update::plugin::PluginBackend::new(app.handle().clone()),
                    version::AppVersion::current().version,
                ),
                #[cfg(not(feature = "updater"))]
                updater: update::UpdateController::new(
                    update::UPDATE_ENDPOINT,
                    update::UPDATE_PUBKEY,
                    update::NullBackend,
                    version::AppVersion::current().version,
                ),
                match_active: update_match_active,
                settings_dir,
                audio_prefs,
                hotkeys: Mutex::new(hotkeys::Controller::default()),
                show_hotkey,
                hud: Mutex::new(hud::Controller::default()),
                layout,
                audition: commands::AuditionTracker::default(),
                settings_gate: Mutex::new(()),
                sample,
                revision: AtomicU64::new(0),
                mixer: mixer_service,
                orb: orb_host,
                hero_stats: hero_stats_host,
                objectives: objective_host,
                match_history,
                opendota,
                news,
            });
            orb::host::register_critical_listener(app.handle());
            app.state::<AppState>().orb.start_if_enabled(app.handle());
            let hud_controller =
                hud::Controller::start(app.handle().clone(), hud_runtime, hud_window);
            *intake::lock(&app.state::<AppState>().hud) = hud_controller;
            commands::start_hotkeys(app.handle(), &app.state::<AppState>());
            show_hotkey::start(app.handle());
            steam::start_status_probe(app.handle().clone(), steam_runtime);
            let app_handle = app.handle().clone();
            let last_overlay_status = Arc::new(Mutex::new(initial_overlay_status.clone()));
            register_overlay_events(
                app_handle.clone(),
                overlay_window,
                last_overlay_status.clone(),
                app.state::<AppState>().audition.clone(),
            );
            start_overlay_status_mirror(app_handle, last_overlay_status, initial_overlay_status);
            objectives::start_alert_worker(app.handle().clone(), objective_alert_receiver);
            crate::log_info!("app", "setup_complete", "application setup completed");
            Ok(())
        });
    #[cfg(feature = "updater")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(any(feature = "demo", debug_assertions))]
    let builder = builder.invoke_handler(generate_handler_with_updater_commands![
        commands::app_version,
        commands::draft_stats_prefs_get,
        commands::draft_stats_prefs_set,
        commands::draft_stats_retry,
        commands::draft_column_view,
        commands::update_status,
        commands::update_open_release_page,
        portraits::hero_portrait,
        steam::launch_dota,
        steam::steam_status,
        commands::log_tail,
        commands::log_dir,
        commands::log_open_folder,
        commands::log_frontend,
        commands::log_set_level,
        commands::runtime_get_status,
        commands::own_snapshot,
        commands::objective_read_model,
        commands::gsi_setup,
        commands::settings_get,
        commands::settings_patch,
        commands::objective_prefs_get,
        commands::objective_prefs_patch,
        commands::layout_get,
        commands::layout_patch,
        commands::layout_reset,
        commands::media_inspect,
        commands::media_package_summary,
        commands::media_package_size_limit,
        commands::media_package_summary_bytes,
        commands::media_install,
        commands::media_install_bytes,
        commands::media_activate,
        commands::media_cancel_pending_activation,
        commands::media_list,
        commands::media_pack_summaries,
        commands::media_cabinet,
        commands::media_event_map,
        commands::media_installed_event_map,
        commands::media_audition,
        commands::media_audition_variant,
        commands::media_audition_installed_variant,
        commands::media_variant_visual_url,
        commands::media_probe_report,
        commands::media_probe_pending,
        commands::audio_history,
        commands::match_recap,
        commands::clear_match_recap,
        commands::match_history_get,
        commands::match_history_clear,
        commands::opendota_prefs_get,
        commands::opendota_prefs_set,
        commands::opendota_view_get,
        commands::opendota_refresh,
        commands::news_feed_get,
        commands::news_refresh,
        commands::news_prefs_get,
        commands::news_prefs_set,
        commands::replay_library_list,
        commands::replay_library_pick_file,
        commands::replay_library_open,
        commands::replay_library_parse,
        commands::replay_library_remove,
        commands::replay_library_clear,
        commands::replay_library_pin,
        commands::replay_library_open_folder,
        commands::analytics_key_status,
        commands::analytics_key_set,
        commands::analytics_key_delete,
        commands::analytics_settings_get,
        commands::analytics_settings_patch,
        commands::analytics_consent_set,
        commands::analytics_ledger_list,
        commands::analytics_notes_list,
        commands::analytics_note_read,
        commands::analytics_note_delete,
        commands::analytics_note_open_folder,
        commands::analytics_own_hero_set,
        commands::analytics_chat_submit,
        commands::analytics_chat_cancel,
        commands::news_open_link,
        commands::set_muted,
        commands::audio_prefs_get,
        commands::audio_set_volume,
        commands::audio_set_hotkeys,
        show_hotkey::shell_hotkey_get,
        show_hotkey::shell_hotkey_set,
        account_supabase::account_status,
        account_supabase::account_sign_in,
        account_supabase::account_cancel_sign_in,
        account_supabase::account_sign_out,
        capture_host::minimap_subscribe,
        capture_host::draft_bar_read,
        capture_host::capture_status,
        capture_host::capture_invalidate,
        capture_host::capture_retry,
        capture_host::capture_get_config,
        capture_host::capture_set_config,
        capture_host::capture_reset_crop,
        capture_host::capture_propose_crop,
        capture_host::capture_accept_proposal,
        capture_host::capture_confirm_crop,
        orb::host::orb_prefs_get,
        orb::host::orb_prefs_set,
        orb::host::orb_click,
        orb::host::orb_panel_height,
        orb::host::mixer_get,
        orb::host::mixer_set,
        orb::host::mixer_focus_toggle,
        #[cfg(feature = "demo")]
        account_host::account_fake_native,
        commands::demo_available,
        commands::demo_trigger,
        lab::dev::lab_status,
        lab::dev::lab_describe,
        lab::dev::lab_last_trace,
        lab::dev::lab_export,
        lab::dev::lab_apply_edits,
        lab::dev::lab_replay,
        lab::dev::lab_layout_get,
        lab::dev::lab_layout_preview,
        lab::dev::lab_layout_apply,
        lab::dev::lab_layout_export,
        lab::dev::lab_hud_layout_get,
        lab::dev::lab_hud_layout_apply,
        lab::dev::lab_hud_layout_reset,
        lab::dev::lab_hud_preview,
        lab::dev::lab_reference_image,
        lab::dev::lab_minimap_recorder_status,
        lab::dev::lab_minimap_recorder_set,
        lab::dev::lab_minimap_recorder_delete,
        lab::dev::lab_minimap_recorder_open_folder
    ]);
    #[cfg(not(any(feature = "demo", debug_assertions)))]
    let builder = builder.invoke_handler(generate_handler_with_updater_commands![
        commands::app_version,
        commands::draft_stats_prefs_get,
        commands::draft_stats_prefs_set,
        commands::draft_stats_retry,
        commands::draft_column_view,
        commands::update_status,
        commands::update_open_release_page,
        portraits::hero_portrait,
        steam::launch_dota,
        steam::steam_status,
        commands::log_tail,
        commands::log_dir,
        commands::log_open_folder,
        commands::log_frontend,
        commands::log_set_level,
        commands::runtime_get_status,
        commands::own_snapshot,
        commands::objective_read_model,
        commands::gsi_setup,
        commands::settings_get,
        commands::settings_patch,
        commands::objective_prefs_get,
        commands::objective_prefs_patch,
        commands::layout_get,
        commands::layout_patch,
        commands::layout_reset,
        commands::media_inspect,
        commands::media_package_summary,
        commands::media_package_size_limit,
        commands::media_package_summary_bytes,
        commands::media_install,
        commands::media_install_bytes,
        commands::media_activate,
        commands::media_cancel_pending_activation,
        commands::media_list,
        commands::media_pack_summaries,
        commands::media_cabinet,
        commands::media_event_map,
        commands::media_installed_event_map,
        commands::media_audition,
        commands::media_audition_variant,
        commands::media_audition_installed_variant,
        commands::media_variant_visual_url,
        commands::media_probe_report,
        commands::media_probe_pending,
        commands::audio_history,
        commands::match_recap,
        commands::clear_match_recap,
        commands::match_history_get,
        commands::match_history_clear,
        commands::opendota_prefs_get,
        commands::opendota_prefs_set,
        commands::opendota_view_get,
        commands::opendota_refresh,
        commands::news_feed_get,
        commands::news_refresh,
        commands::news_prefs_get,
        commands::news_prefs_set,
        commands::replay_library_list,
        commands::replay_library_pick_file,
        commands::replay_library_open,
        commands::replay_library_parse,
        commands::replay_library_remove,
        commands::replay_library_clear,
        commands::replay_library_pin,
        commands::replay_library_open_folder,
        commands::analytics_key_status,
        commands::analytics_key_set,
        commands::analytics_key_delete,
        commands::analytics_settings_get,
        commands::analytics_settings_patch,
        commands::analytics_consent_set,
        commands::analytics_ledger_list,
        commands::analytics_notes_list,
        commands::analytics_note_read,
        commands::analytics_note_delete,
        commands::analytics_note_open_folder,
        commands::analytics_own_hero_set,
        commands::analytics_chat_submit,
        commands::analytics_chat_cancel,
        commands::news_open_link,
        commands::set_muted,
        commands::audio_prefs_get,
        commands::audio_set_volume,
        commands::audio_set_hotkeys,
        show_hotkey::shell_hotkey_get,
        show_hotkey::shell_hotkey_set,
        account_supabase::account_status,
        account_supabase::account_sign_in,
        account_supabase::account_cancel_sign_in,
        account_supabase::account_sign_out,
        capture_host::minimap_subscribe,
        capture_host::draft_bar_read,
        capture_host::capture_status,
        capture_host::capture_invalidate,
        capture_host::capture_retry,
        capture_host::capture_get_config,
        capture_host::capture_set_config,
        capture_host::capture_reset_crop,
        capture_host::capture_propose_crop,
        capture_host::capture_accept_proposal,
        capture_host::capture_confirm_crop,
        orb::host::orb_prefs_get,
        orb::host::orb_prefs_set,
        orb::host::orb_click,
        orb::host::orb_panel_height,
        orb::host::mixer_get,
        orb::host::mixer_set,
        orb::host::mixer_focus_toggle,
        commands::demo_available
    ]);
    let app = builder.build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|handle, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                crate::log_info!("app", "app_stopping", "exit requested");
                logging::flush();
            }
            if matches!(event, tauri::RunEvent::Exit) {
                capture_host::stop();
                crate::log_info!("app", "exit", "Kohbor is exiting");
                handle.state::<analytics::AnalyticsService>().cancel_all();
                let state = handle.state::<AppState>();
                handle.state::<account_supabase::SupabaseAuth>().shutdown();
                // Orb first (no new clicks), then put every touched audio session back (MEDIA-009).
                state.orb.stop_for_exit();
                state.hero_stats.stop_for_exit();
                state.opendota.stop_for_exit();
                state.mixer.shutdown();
                intake::lock(&state.hud).stop();
                intake::lock(&state.hotkeys).stop();
                state.host.shutdown();
                crate::log_info!("app", "process_exit", "all host threads stopped");
                logging::flush();
            }
        }),
        Err(_error) => crate::log_error!("app", "build_failed", "TAURI_BUILD_FAILED"),
    }
    logging::flush();
}

#[cfg(test)]
mod containment_failure_tests {
    use super::{containment, start_with};
    use std::cell::{Cell, RefCell};

    const STEPS: [&str; 4] = ["create", "cpu-rate", "limits", "assign"];

    struct FakeBackend {
        fail_at: Option<&'static str>,
        calls: Vec<&'static str>,
    }

    impl FakeBackend {
        fn step(&mut self, name: &'static str) -> Result<(), ()> {
            self.calls.push(name);
            if self.fail_at == Some(name) {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    impl containment::JobBackend for FakeBackend {
        fn create(&mut self) -> Result<(), ()> {
            self.step("create")
        }
        fn configure_cpu_rate(&mut self) -> Result<(), ()> {
            self.step("cpu-rate")
        }
        fn configure_limits(&mut self) -> Result<(), ()> {
            self.step("limits")
        }
        fn assign(&mut self) -> Result<(), ()> {
            self.step("assign")
        }
    }

    #[test]
    fn establishment_stops_at_each_failure() {
        for (index, stage) in STEPS.iter().copied().enumerate() {
            let mut backend = FakeBackend {
                fail_at: Some(stage),
                calls: vec![],
            };
            assert_eq!(containment::establish_with(&mut backend), Err(stage));
            assert_eq!(backend.calls, STEPS[..=index], "failure at {stage}");
        }
    }

    #[test]
    fn establishment_succeeds_in_order() {
        let mut backend = FakeBackend {
            fail_at: None,
            calls: vec![],
        };
        assert_eq!(containment::establish_with(&mut backend), Ok(()));
        assert_eq!(backend.calls, STEPS);
    }

    #[test]
    fn startup_reports_only_the_safe_failure_and_never_launches() {
        for stage in STEPS {
            let launches = Cell::new(0);
            let reports = RefCell::new(Vec::new());
            let code = start_with(
                || Err(stage),
                || launches.set(launches.get() + 1),
                |message| reports.borrow_mut().push(message.to_owned()),
            );
            assert_eq!(code, 1);
            assert_eq!(launches.get(), 0);
            assert_eq!(
                *reports.borrow(),
                [format!("CPU_CONTAINMENT_FAILED ({stage})")]
            );
        }
    }

    #[test]
    fn startup_launches_once_after_success_without_reporting() {
        let launches = Cell::new(0);
        let reports = Cell::new(0);
        let code = start_with(
            || Ok(()),
            || launches.set(launches.get() + 1),
            |_| reports.set(reports.get() + 1),
        );
        assert_eq!(code, 0);
        assert_eq!(launches.get(), 1);
        assert_eq!(reports.get(), 0);
    }

    #[test]
    fn backend_failures_prevent_startup_launch() {
        for stage in STEPS {
            let mut backend = FakeBackend {
                fail_at: Some(stage),
                calls: vec![],
            };
            let launches = Cell::new(0);
            let reports = RefCell::new(Vec::new());
            let code = start_with(
                || containment::establish_with(&mut backend),
                || launches.set(launches.get() + 1),
                |message| reports.borrow_mut().push(message.to_owned()),
            );
            assert_eq!(code, 1);
            assert_eq!(launches.get(), 0);
            assert_eq!(
                *reports.borrow(),
                [format!("CPU_CONTAINMENT_FAILED ({stage})")]
            );
        }
    }
}

mod demo;
mod lab;
mod overlay;

#[cfg(test)]
mod webview_args_tests {
    #[test]
    fn overlay_and_control_windows_use_the_same_browser_args() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(
            conf["app"]["windows"][0]["additionalBrowserArgs"].as_str(),
            Some(super::WEBVIEW2_BROWSER_ARGS)
        );
    }
}

#[cfg(test)]
mod no_activate_show_tests {
    /// Regression guard for start-up `activation_detected`: the overlay and the HUD must be mapped with
    /// `SW_SHOWNOACTIVATE`, never with `WebviewWindow::show` (which uses `SW_SHOW` and activates).
    #[test]
    fn in_game_windows_are_mapped_without_activation() {
        let lib = include_str!("lib.rs");
        let hud = include_str!("hud.rs");
        // Each needle also appears once in this test, so the code itself must hold a second copy.
        assert!(lib.matches("ShowWindow(hwnd, SW_SHOWNOACTIVATE)").count() >= 2);
        assert!(
            lib.matches("let failure = if !show_without_activation(&window) {")
                .count()
                >= 2
        );
        assert!(hud.contains("if !crate::show_without_activation(window) {"));
        assert!(
            !hud.contains("window.show()"),
            "the HUD must not use the activating show"
        );
    }
}
