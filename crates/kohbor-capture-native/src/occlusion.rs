use kohbor_capture::Rect;

#[derive(Debug, Clone, Copy)]
struct WindowLayer {
    rect: Rect,
    visible: bool,
    iconic: bool,
    transparent: bool,
    cloaked: bool,
    is_game: bool,
}

fn intersects_crop(crop: Rect, window: Rect) -> bool {
    let overlap_x = (i64::from(crop.x) + i64::from(crop.width))
        .min(i64::from(window.x) + i64::from(window.width))
        - i64::from(crop.x).max(i64::from(window.x));
    let overlap_y = (i64::from(crop.y) + i64::from(crop.height))
        .min(i64::from(window.y) + i64::from(window.height))
        - i64::from(crop.y).max(i64::from(window.y));
    overlap_x > 1 && overlap_y > 1
}

/// Input is EnumWindows order (topmost first). Seeing an occluder without
/// finding the identified game window does not prove its relative z-order.
fn crop_occluded_by_windows(crop: Rect, windows: &[WindowLayer]) -> Option<bool> {
    if crop.width == 0 || crop.height == 0 {
        return None;
    }
    let game_index = windows.iter().position(|window| window.is_game)?;
    let game = windows[game_index];
    if !game.visible || game.iconic || game.cloaked || game.rect.width == 0 || game.rect.height == 0
    {
        return None;
    }
    Some(windows[..game_index].iter().any(|window| {
        window.visible
            && !window.iconic
            && !window.transparent
            && !window.cloaked
            && intersects_crop(crop, window.rect)
    }))
}

pub(crate) fn to_screen_rect(rect: Rect, origin_x: i32, origin_y: i32) -> Option<Rect> {
    Some(Rect {
        x: rect.x.checked_add(origin_x)?,
        y: rect.y.checked_add(origin_y)?,
        ..rect
    })
}

#[cfg(windows)]
pub(crate) mod platform {
    use super::{crop_occluded_by_windows, intersects_crop, Rect, WindowLayer};
    use std::mem::size_of;
    use windows::{
        core::BOOL,
        Win32::{
            Foundation::{GetLastError, SetLastError, ERROR_SUCCESS, HWND, LPARAM, RECT},
            Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
            UI::WindowsAndMessaging::{
                EnumWindows, GetLayeredWindowAttributes, GetWindowLongPtrW, GetWindowRect,
                GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE,
                LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, WS_EX_LAYERED, WS_EX_TRANSPARENT,
            },
        },
    };

    struct Scan {
        game: HWND,
        crop: Rect,
        windows: Vec<WindowLayer>,
        found_game: bool,
        failed: bool,
    }

    pub(crate) fn crop_occluded(crop: Rect) -> Option<bool> {
        let (handle, expected_owner) = crate::dxgi::identified_game_window()?;
        let game = HWND(handle as *mut core::ffi::c_void);
        let mut owner = 0;
        // SAFETY: the API validates the cached handle; owner is caller-owned.
        let thread = unsafe { GetWindowThreadProcessId(game, Some(&mut owner)) };
        if thread == 0 || owner != expected_owner {
            return None;
        }
        let mut scan = Scan {
            game,
            crop,
            windows: Vec::new(),
            found_game: false,
            failed: false,
        };
        // SAFETY: EnumWindows is synchronous; the callback never retains the
        // stack-owned Scan. False deliberately stops at the game or a query error.
        let _ =
            unsafe { EnumWindows(Some(enum_window), LPARAM((&mut scan as *mut Scan) as isize)) };
        if scan.failed || !scan.found_game {
            return None;
        }
        crop_occluded_by_windows(crop, &scan.windows)
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam is the live Scan pointer supplied by crop_occluded.
        let scan = unsafe { &mut *(lparam.0 as *mut Scan) };
        let is_game = hwnd == scan.game;
        match inspect_window(hwnd, scan.crop, is_game) {
            Ok(Some(window)) => scan.windows.push(window),
            Ok(None) => {}
            Err(()) => {
                scan.failed = true;
                return BOOL(0);
            }
        }
        if is_game {
            scan.found_game = true;
            BOOL(0)
        } else {
            BOOL(1)
        }
    }

    fn inspect_window(hwnd: HWND, crop: Rect, is_game: bool) -> Result<Option<WindowLayer>, ()> {
        // SAFETY: hwnd came from the synchronous EnumWindows callback.
        let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
        let iconic = unsafe { IsIconic(hwnd).as_bool() };
        if !visible || iconic {
            return if is_game { Err(()) } else { Ok(None) };
        }
        let mut bounds = RECT::default();
        // SAFETY: bounds is caller-owned writable storage.
        unsafe { GetWindowRect(hwnd, &mut bounds) }.map_err(|_| ())?;
        let rect = Rect {
            x: bounds.left,
            y: bounds.top,
            width: u32::try_from(i64::from(bounds.right) - i64::from(bounds.left))
                .map_err(|_| ())?,
            height: u32::try_from(i64::from(bounds.bottom) - i64::from(bounds.top))
                .map_err(|_| ())?,
        };
        if !is_game && !intersects_crop(crop, rect) {
            return Ok(None);
        }
        let mut cloaked = 0_u32;
        // SAFETY: DWMWA_CLOAKED writes one DWORD into correctly sized storage.
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
        }
        .map_err(|_| ())?;
        if cloaked != 0 && !is_game {
            return Ok(None);
        }
        let mut transparent = false;
        if !is_game && cloaked == 0 {
            // Zero is also a valid style, so distinguish it from an API failure.
            unsafe { SetLastError(ERROR_SUCCESS) };
            let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
            if style == 0 && unsafe { GetLastError() } != ERROR_SUCCESS {
                return Err(());
            }
            let layered = style as u32 & WS_EX_LAYERED.0 != 0;
            transparent = layered && style as u32 & WS_EX_TRANSPARENT.0 != 0;
            if layered && !transparent {
                let mut alpha = 255;
                let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS::default();
                // SAFETY: alpha/flags are caller-owned; no pixel or window text is read.
                unsafe {
                    GetLayeredWindowAttributes(hwnd, None, Some(&mut alpha), Some(&mut flags))
                }
                .map_err(|_| ())?;
                transparent = flags.contains(LWA_ALPHA) && alpha == 0;
            }
        }
        Ok(Some(WindowLayer {
            rect,
            visible,
            iconic,
            transparent,
            cloaked: cloaked != 0,
            is_game,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crop() -> kohbor_capture::Rect {
        kohbor_capture::Rect {
            x: 0,
            y: 800,
            width: 280,
            height: 280,
        }
    }

    fn window(rect: kohbor_capture::Rect) -> WindowLayer {
        WindowLayer {
            rect,
            visible: true,
            iconic: false,
            transparent: false,
            cloaked: false,
            is_game: false,
        }
    }

    fn game() -> WindowLayer {
        WindowLayer {
            is_game: true,
            ..window(kohbor_capture::Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            })
        }
    }

    #[test]
    fn taskbar_band_above_game_occludes_the_crop() {
        let taskbar = window(kohbor_capture::Rect {
            x: 0,
            y: 1032,
            width: 1920,
            height: 48,
        });
        assert_eq!(
            crop_occluded_by_windows(crop(), &[taskbar, game()]),
            Some(true)
        );
    }

    #[test]
    fn window_below_game_does_not_occlude() {
        assert_eq!(
            crop_occluded_by_windows(crop(), &[game(), window(crop())]),
            Some(false)
        );
    }

    #[test]
    fn game_window_itself_does_not_occlude() {
        assert_eq!(crop_occluded_by_windows(crop(), &[game()]), Some(false));
    }

    #[test]
    fn missing_game_is_unknown_even_if_an_intersecting_window_exists() {
        assert_eq!(crop_occluded_by_windows(crop(), &[window(crop())]), None);
    }

    #[test]
    fn hidden_windows_do_not_occlude() {
        let hidden = WindowLayer {
            visible: false,
            ..window(crop())
        };
        assert_eq!(
            crop_occluded_by_windows(crop(), &[hidden, game()]),
            Some(false)
        );
    }

    #[test]
    fn minimised_windows_do_not_occlude() {
        let iconic = WindowLayer {
            iconic: true,
            ..window(crop())
        };
        assert_eq!(
            crop_occluded_by_windows(crop(), &[iconic, game()]),
            Some(false)
        );
    }

    #[test]
    fn transparent_overlays_do_not_occlude() {
        let transparent = WindowLayer {
            transparent: true,
            ..window(crop())
        };
        assert_eq!(
            crop_occluded_by_windows(crop(), &[transparent, game()]),
            Some(false)
        );
    }

    #[test]
    fn cloaked_windows_do_not_occlude() {
        let cloaked = WindowLayer {
            cloaked: true,
            ..window(crop())
        };
        assert_eq!(
            crop_occluded_by_windows(crop(), &[cloaked, game()]),
            Some(false)
        );
    }

    #[test]
    fn one_pixel_overlap_in_either_axis_does_not_occlude() {
        for rect in [
            kohbor_capture::Rect {
                x: 279,
                y: 800,
                width: 100,
                height: 280,
            },
            kohbor_capture::Rect {
                x: 0,
                y: 1079,
                width: 280,
                height: 100,
            },
        ] {
            assert_eq!(
                crop_occluded_by_windows(crop(), &[window(rect), game()]),
                Some(false)
            );
        }
    }

    #[test]
    fn two_pixel_overlap_in_both_axes_occludes() {
        let rect = kohbor_capture::Rect {
            x: 278,
            y: 1078,
            width: 100,
            height: 100,
        };
        assert_eq!(
            crop_occluded_by_windows(crop(), &[window(rect), game()]),
            Some(true)
        );
    }

    #[test]
    fn touching_edges_and_disjoint_windows_do_not_occlude() {
        for rect in [
            kohbor_capture::Rect {
                x: 280,
                y: 800,
                width: 100,
                height: 280,
            },
            kohbor_capture::Rect {
                x: -100,
                y: 800,
                width: 100,
                height: 280,
            },
            kohbor_capture::Rect {
                x: 500,
                y: 0,
                width: 100,
                height: 100,
            },
        ] {
            assert_eq!(
                crop_occluded_by_windows(crop(), &[window(rect), game()]),
                Some(false)
            );
        }
    }

    #[test]
    fn negative_desktop_coordinates_are_supported() {
        let crop = kohbor_capture::Rect {
            x: -1920,
            y: -280,
            width: 280,
            height: 280,
        };
        assert_eq!(
            crop_occluded_by_windows(crop, &[window(crop), game()]),
            Some(true)
        );
    }

    #[test]
    fn large_rectangles_do_not_overflow_intersection_math() {
        let huge = kohbor_capture::Rect {
            x: i32::MIN,
            y: i32::MIN,
            width: u32::MAX,
            height: u32::MAX,
        };
        assert_eq!(
            crop_occluded_by_windows(crop(), &[window(huge), game()]),
            Some(true)
        );
    }

    #[test]
    fn empty_crop_or_unavailable_game_is_unknown() {
        let empty = kohbor_capture::Rect { width: 0, ..crop() };
        assert_eq!(crop_occluded_by_windows(empty, &[game()]), None);
        for game in [
            WindowLayer {
                visible: false,
                ..game()
            },
            WindowLayer {
                iconic: true,
                ..game()
            },
            WindowLayer {
                cloaked: true,
                ..game()
            },
        ] {
            assert_eq!(crop_occluded_by_windows(crop(), &[game]), None);
        }
    }

    #[test]
    fn output_local_crop_is_translated_to_desktop_coordinates() {
        let local = crop();
        assert_eq!(
            to_screen_rect(local, -1920, -1080),
            Some(Rect {
                x: -1920,
                y: -280,
                ..local
            })
        );
    }

    #[test]
    fn desktop_translation_overflow_is_unknown() {
        assert_eq!(
            to_screen_rect(
                Rect {
                    x: 1,
                    y: 0,
                    width: 2,
                    height: 2
                },
                i32::MAX,
                0
            ),
            None
        );
    }
}
