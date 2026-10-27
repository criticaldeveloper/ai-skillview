#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod dialog;
mod startup;
mod storage;

fn main() {
    // All Windows objects are owned by the UI thread and outlive its message loop.
    unsafe { app::run() }
}
