#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod desktop;
mod runtime;
mod updater;

fn main() {
    app::run();
}
