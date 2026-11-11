// Release builds use the Windows GUI subsystem (no console window).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // CORE-002: containment must exist before any WebView or helper process.
    if let Err(step) = kohbor_desktop_lib::containment::establish() {
        eprintln!("CPU_CONTAINMENT_FAILED ({step})");
        std::process::exit(1);
    }
    kohbor_desktop_lib::run();
}
