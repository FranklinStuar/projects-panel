// Evita una consola extra en Windows release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    wordpress_panel_lib::run()
}
