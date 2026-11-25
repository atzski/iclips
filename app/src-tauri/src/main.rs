// Pas de console derrière l'app en version publiée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    iclips_lib::run()
}
