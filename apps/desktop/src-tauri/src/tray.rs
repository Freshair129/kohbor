//! Notification-area (tray) icon. Owner request 2026-11-19: closing the control window hides it
//! to the tray instead of ending the process; the tray icon reopens the window or quits Kohbor.
//! If the tray icon cannot be created, closing the window ends the process as before.

use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

pub const TRAY_ID: &str = "kohbor-main";
const SHOW_ID: &str = "tray-show";
const QUIT_ID: &str = "tray-quit";
const SHOW_LABEL: &str = "เปิด Kohbor";
const QUIT_LABEL: &str = "ออกจาก Kohbor";

/// What a tray menu item does.
#[derive(Debug, PartialEq, Eq)]
pub enum MenuAction {
    Show,
    Quit,
    Ignore,
}

pub fn menu_action(id: &str) -> MenuAction {
    match id {
        SHOW_ID => MenuAction::Show,
        QUIT_ID => MenuAction::Quit,
        _ => MenuAction::Ignore,
    }
}

/// Whether a tray icon event reopens the control window: a completed left click.
pub fn reopens_on(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } | TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        }
    )
}

/// Show, restore and focus the control window.
pub fn show_control<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("control") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        crate::log_info!(
            "app",
            "shown_from_tray",
            "control window shown from the tray"
        );
    }
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match menu_action(event.id().as_ref()) {
        MenuAction::Show => show_control(app),
        MenuAction::Quit => {
            crate::log_info!("app", "app_stopping", "quit chosen from the tray");
            crate::logging::flush();
            app.exit(0);
        }
        MenuAction::Ignore => {}
    }
}

/// Create the tray icon. `tooltip` is the control window title (name, version and Lab marker).
pub fn install<R: Runtime>(app: &tauri::App<R>, tooltip: &str) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, SHOW_ID, SHOW_LABEL, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, QUIT_LABEL, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if reopens_on(&event) {
                show_control(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::tray::TrayIconId;
    use tauri::{PhysicalPosition, Rect};

    fn click(button: MouseButton, state: MouseButtonState) -> TrayIconEvent {
        TrayIconEvent::Click {
            id: TrayIconId::new(TRAY_ID),
            position: PhysicalPosition::new(0.0, 0.0),
            rect: Rect::default(),
            button,
            button_state: state,
        }
    }

    #[test]
    fn menu_ids_map_to_show_and_quit_only() {
        assert_eq!(menu_action(SHOW_ID), MenuAction::Show);
        assert_eq!(menu_action(QUIT_ID), MenuAction::Quit);
        assert_eq!(menu_action("other"), MenuAction::Ignore);
        assert_ne!(SHOW_LABEL, QUIT_LABEL);
    }

    #[test]
    fn only_a_completed_left_click_reopens_the_window() {
        assert!(reopens_on(&click(MouseButton::Left, MouseButtonState::Up)));
        assert!(!reopens_on(&click(
            MouseButton::Left,
            MouseButtonState::Down
        )));
        assert!(!reopens_on(&click(
            MouseButton::Right,
            MouseButtonState::Up
        )));
    }
}
