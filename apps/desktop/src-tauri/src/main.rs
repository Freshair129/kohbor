// Release builds use the Windows GUI subsystem (no console window).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kohbor_desktop_lib::{containment, instance};

fn main() {
    // A second instance would fight the first one for the intake port and could
    // rewrite the owned game configuration; hand over to the running one instead.
    if !instance::acquire(instance::INSTANCE_NAME) {
        eprintln!("Kohbor is already running; bringing the existing window forward.");
        instance::focus_existing();
        return;
    }
    // CORE-002: containment must exist before any WebView or helper process.
    if let Err(step) = containment::establish() {
        eprintln!("CPU_CONTAINMENT_FAILED ({step})");
        std::process::exit(1);
    }
    kohbor_desktop_lib::run();
}
