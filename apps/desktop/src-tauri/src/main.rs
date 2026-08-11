// Keep release builds free of an extra console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    convenient_window_lib::run()
}
