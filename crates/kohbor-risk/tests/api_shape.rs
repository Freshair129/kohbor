//! W0 scaffold: pins the frozen public API shape. W5 adds real tests beside this file.
use kohbor_risk::*;

#[test]
fn api_shape_is_frozen() {
    let _new: fn() -> RiskEngine = RiskEngine::new;
    let _tick: fn(&mut RiskEngine, &TickInput) -> RiskOutput = RiskEngine::tick;
}
