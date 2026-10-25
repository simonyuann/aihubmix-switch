#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--print-update-format") {
        println!("{}", aihubmix_switch_lib::update_format());
        return;
    }
    aihubmix_switch_lib::run();
}
