// Release builds use the Windows GUI subsystem (no console window).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kohbor_desktop_lib::{containment, instance};

fn main() {
    kohbor_desktop_lib::initialize_logging();
    // A second instance would fight the first one for the intake port and could
    // rewrite the owned game configuration; hand over to the running one instead.
    if !instance::acquire(instance::INSTANCE_NAME) {
        eprintln!("Kohbor is already running; bringing the existing window forward.");
        kohbor_desktop_lib::log_info!("app", "second_instance", "existing app instance focused");
        kohbor_desktop_lib::logging::flush();
        instance::focus_existing();
        return;
    }
    // CORE-002: containment must exist before any WebView or helper process.
    let code = kohbor_desktop_lib::start_with(
        containment::establish,
        kohbor_desktop_lib::run,
        |message| {
            eprintln!("{message}");
            kohbor_desktop_lib::log_error!("app", "containment_failed", message);
        },
    );
    kohbor_desktop_lib::logging::flush();
    if code != 0 {
        std::process::exit(code);
    }
}
