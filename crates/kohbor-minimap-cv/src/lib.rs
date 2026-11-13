//! MVP-1 W6 scaffold: API frozen at W0 (docs: MVP-PLAN section e). Bodies are `todo!()` until W6.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Radiant,
    Dire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blob {
    pub x_milli: u16,
    pub y_milli: u16,
    pub area: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelReport {
    pub structural_ok: bool,
    pub enemy_blobs: Vec<Blob>,
}

/// `enemy` is the team whose markers are counted.
pub fn analyze(_bgra: &[u8], _width: u32, _height: u32, _stride: u32, _enemy: Team) -> PixelReport {
    todo!("W6")
}
