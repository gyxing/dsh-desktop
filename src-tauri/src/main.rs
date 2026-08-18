#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod desktop;
mod runtime;

fn main() {
    app::run();
}
