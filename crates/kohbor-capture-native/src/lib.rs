//! Windows DXGI Desktop Duplication capture for one minimap region.
//! The whole output remains on the GPU; only the requested crop is read back.

mod capture_frame;
mod capture_recovery;
#[cfg(windows)]
mod dxgi;
#[cfg(any(windows, test))]
mod occlusion;
mod region;

use std::{fmt, marker::PhantomData, rc::Rc};

use kohbor_capture::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputInfo {
    pub output_id: u64,
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    NoGameWindow,
    Unsupported(String),
    Os(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoGameWindow => formatter.write_str("NoGameWindow"),
            Self::Unsupported(reason) => write!(formatter, "unsupported capture output: {reason}"),
            Self::Os(reason) => write!(formatter, "capture initialization failed: {reason}"),
        }
    }
}

impl std::error::Error for OpenError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grab {
    Frame {
        width: u32,
        height: u32,
        stride: u32,
        acquired_us: u64,
    },
    Timeout,
    AccessLost,
    Failed(String),
}

/// Lives on one thread and owns one DXGI duplication session.
pub struct MinimapGrabber {
    output: OutputInfo,
    diagnostic: Option<fn(&str)>,
    #[cfg(windows)]
    native: dxgi::NativeCapture,
    _thread_bound: PhantomData<Rc<()>>,
}

impl MinimapGrabber {
    pub fn open() -> Result<Self, OpenError> {
        #[cfg(windows)]
        {
            let game = dxgi::find_game_window()?;
            let (native, output) = dxgi::NativeCapture::open(game)?;
            Ok(Self {
                output,
                diagnostic: None,
                native,
                _thread_bound: PhantomData,
            })
        }

        #[cfg(not(windows))]
        {
            Err(OpenError::Unsupported(
                "DXGI Desktop Duplication requires Windows".to_owned(),
            ))
        }
    }

    pub fn output(&self) -> OutputInfo {
        self.output
    }

    /// Checks an output-local crop after translating it to desktop coordinates.
    /// This reuses the game window identified by capture startup/game_window().
    pub fn crop_occluded(&self, rect: Rect) -> Option<bool> {
        #[cfg(windows)]
        {
            self.native.screen_rect(rect).and_then(crop_occluded)
        }
        #[cfg(not(windows))]
        {
            let _ = rect;
            None
        }
    }

    /// Copies `rect` as tight BGRA rows into `out`.
    pub fn grab(&mut self, rect: Rect, timeout_ms: u32, out: &mut Vec<u8>) -> Grab {
        if !capture_frame::rect_fits_output(rect, self.output.width, self.output.height) {
            return Grab::Failed("capture rectangle is empty or outside the output".to_owned());
        }
        let Some(layout) = capture_frame::crop_layout(rect) else {
            return Grab::Failed("capture rectangle size overflows BGRA layout".to_owned());
        };

        #[cfg(windows)]
        {
            match self.native.grab(rect, layout, timeout_ms.min(15), out) {
                Ok(dxgi::NativeGrab::Frame { acquired_us }) => Grab::Frame {
                    width: rect.width,
                    height: rect.height,
                    stride: layout.stride,
                    acquired_us,
                },
                Ok(dxgi::NativeGrab::Timeout) => Grab::Timeout,
                Ok(dxgi::NativeGrab::AccessLost) => {
                    self.report("DXGI access lost; duplication will be recreated on the next grab");
                    Grab::AccessLost
                }
                Err(error) => {
                    self.report(&error);
                    Grab::Failed(error)
                }
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (timeout_ms, out, layout);
            Grab::Failed("DXGI Desktop Duplication requires Windows".to_owned())
        }
    }

    /// Installs an optional diagnostic callback without adding a logger dependency.
    pub fn set_diagnostic_callback(&mut self, callback: Option<fn(&str)>) {
        self.diagnostic = callback;
    }

    /// `IDXGIAdapter3::QueryVideoMemoryInfo` CurrentUsage for this process only.
    /// Monitoring only; unavailable measurements remain `None`.
    pub fn gpu_local_bytes(&self) -> Option<u64> {
        #[cfg(windows)]
        {
            self.native.gpu_local_bytes()
        }

        #[cfg(not(windows))]
        {
            None
        }
    }

    fn report(&self, message: &str) {
        if let Some(callback) = self.diagnostic {
            callback(message);
        }
    }
}

/// Returns a built-in candidate only for 1920x1080 at 96 DPI.
pub fn profile_rect(output: &OutputInfo) -> Option<(Rect, &'static str)> {
    region::profile_rect(output)
}

/// Screen-space rectangle of the visible top-level `dota2.exe` window, if present.
pub fn game_window() -> Option<Rect> {
    #[cfg(windows)]
    {
        dxgi::game_window()
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Checks visible top-level windows above the last game window identified by the
/// existing capture startup/game_window() lookup. `rect` uses desktop coordinates.
/// No identified game window, a changed owner, or a failed required query is unknown.
/// This checker never enumerates processes or reads window titles.
///
/// This is a rectangle/z-order check, not proof about the composited pixels. It
/// cannot see exclusive overlays or DWM visuals absent from EnumWindows, and can
/// over-count shaped/partially transparent windows. Unreadable layered alpha is
/// unknown; layered click-through and known zero-alpha windows are skipped.
pub fn crop_occluded(rect: Rect) -> Option<bool> {
    #[cfg(windows)]
    {
        occlusion::platform::crop_occluded(rect)
    }
    #[cfg(not(windows))]
    {
        let _ = rect;
        None
    }
}
