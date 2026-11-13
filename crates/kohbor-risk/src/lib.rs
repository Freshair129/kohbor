//! MVP-1 W5 scaffold: API frozen at W0 (docs: MVP-PLAN section e). Bodies are `todo!()` until W5.

use kohbor_capture::Coverage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnGsi {
    pub alive: bool,
    pub hp_pct: Option<u8>,
    pub in_match: bool,
    pub age_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickInput {
    pub now_ms: u64,
    pub coverage: Coverage,
    pub visible: Option<u8>,
    pub own: Option<OwnGsi>,
    pub gsi_events: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meter {
    Unknown,
    Level(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskEvent {
    EnemyMissing,
    RiskAlert,
    RiskRevision,
    RiskUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskOutput {
    pub meter: Meter,
    pub gsi_source: bool,
    pub vision_source: bool,
    pub visible: Option<u8>,
    pub baseline: Option<u8>,
    pub events: Vec<RiskEvent>,
}

pub struct RiskEngine {
    _private: (),
}

impl RiskEngine {
    pub fn new() -> Self {
        todo!("W5")
    }
    pub fn tick(&mut self, _input: &TickInput) -> RiskOutput {
        todo!("W5")
    }
}

impl Default for RiskEngine {
    fn default() -> Self {
        Self::new()
    }
}
