//! MVP-1 W2 scaffold: API frozen at W0 (docs: MVP-PLAN section e). Bodies are `todo!()` until W2.

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

/// Lives on one thread (may be `!Send`).
pub struct MinimapGrabber {
    _private: (),
}

impl MinimapGrabber {
    pub fn open() -> Result<Self, OpenError> {
        todo!("W2")
    }
    pub fn output(&self) -> OutputInfo {
        todo!("W2")
    }
    /// Copies `rect` as tight BGRA rows into `out`.
    pub fn grab(&mut self, _rect: Rect, _timeout_ms: u32, _out: &mut Vec<u8>) -> Grab {
        todo!("W2")
    }
    /// `IDXGIAdapter3::QueryVideoMemoryInfo` CurrentUsage for this process only. Monitoring only.
    pub fn gpu_local_bytes(&self) -> Option<u64> {
        todo!("W2")
    }
}

/// `Some` only for 1920x1080 at 96 dpi.
pub fn profile_rect(_out: &OutputInfo) -> Option<(Rect, &'static str)> {
    todo!("W2")
}

pub fn game_window() -> Option<Rect> {
    todo!("W2")
}
