#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod studio;
fn main() {
    studio::run();
}
