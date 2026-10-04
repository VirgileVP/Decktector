// Keep the console window from opening alongside the app on Windows release
// builds; `cargo run` in a terminal still prints, because debug keeps it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    decktector_lib::run()
}
