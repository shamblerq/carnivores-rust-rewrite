// The binary for this game: everything lives in carn_engine.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    carn_engine::run(carn_engine::GameKind::IceAge);
}
