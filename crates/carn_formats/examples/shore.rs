//! Prints a few dry cells with deep water a few cells to the east, for
//! pointing a test run at water. Usage: shore c1|c2 FILE.MAP
use carn_formats::{map::Map, Engine};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let engine = if args[1] == "c1" {
        Engine::C1
    } else {
        Engine::C2
    };
    let m = Map::load(Path::new(&args[2]), engine, 1).expect("map");
    let n = m.size;
    let mut found = 0;
    for y in (64..n - 64).step_by(7) {
        for x in 64..n - 64 {
            let dry = (0..3).all(|d| !m.water(m.idx(x - d, y)));
            let wet = (4..12).all(|d| m.deep_water(m.idx(x + d, y)));
            if dry && wet {
                println!("x={} y={}", x, y);
                found += 1;
                break;
            }
        }
        if found >= 5 {
            break;
        }
    }
}
