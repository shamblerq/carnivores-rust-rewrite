//! Shots against real creatures in a real area. Needs Carnivores 2's data:
//! CARN2_DATA=/path/to/game cargo test -p carn_engine --test shots
use std::path::PathBuf;

use bevy::math::Vec3;
use carn_engine::area::Area;
use carn_engine::creatures::{load_dinos, load_models};
use carn_engine::game::GameKind;
use carn_engine::paths::DataRoot;
use carn_engine::settings::Settings;
use carn_engine::sim::{Character, Hit, Sim};
use carn_formats::rsc::LoadOptions;

fn data() -> Option<DataRoot> {
    std::env::var_os("CARN2_DATA").map(|p| DataRoot(PathBuf::from(p)))
}

#[test]
fn shots_hit_and_kill() {
    let Some(root) = data() else {
        eprintln!("CARN2_DATA not set; skipped");
        return;
    };
    let kind = GameKind::Carnivores2;
    let mut area = Area::load(&root, kind, "HUNTDAT/AREAS/AREA1", LoadOptions::default()).unwrap();
    // A clearing, so nothing stands between the gun and the target.
    for cz in 290..312 {
        for cx in 290..312 {
            let i = area.idx(cx, cz);
            area.map.omap[i] = 255;
        }
    }
    let (dinos, _) = load_dinos(&root, kind).unwrap();
    let models = load_models(&root, kind, &dinos, &Settings::defaults(kind));
    let mut sim = Sim::new(kind, dinos, models);
    let (x, z) = (300.5 * 256.0, 300.5 * 256.0);
    let names: Vec<String> = sim.dinos.iter().map(|d| d.name.clone()).collect();
    for (t, name) in names.iter().enumerate().skip(1) {
        if sim.chinfo[t].is_none() {
            continue;
        }
        sim.chars.clear();
        let mut c = Character {
            ctype: t,
            ..Default::default()
        };
        c.pos = Vec3::new(x, area.land_h(x, z), z);
        sim.reset_character(&mut c);
        // Posed in its first animation, side on.
        c.alpha = 0.0;
        c.lookx = 1.0;
        c.lookz = 0.0;
        sim.chars.push(c.clone());
        let mut buf = Vec::new();
        sim.morph(&c, &mut buf);
        let top = buf.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let bottom = buf.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        // The middle of the body, in the world (the model faces +x here).
        let n = buf.len() as f32;
        let mid = buf.iter().fold(Vec3::ZERO, |a, p| a + Vec3::from(*p)) / n;
        let (sa, ca) = (std::f32::consts::FRAC_PI_2 - c.alpha).sin_cos();
        let aim = c.pos + Vec3::new(mid.x * ca + mid.z * sa, mid.y, mid.z * ca - mid.x * sa);
        let from = aim + Vec3::new(0.0, 60.0, 1500.0);
        let (hit, at) = sim.trace_shot(&area, from, from + (aim - from).normalize() * 4000.0);
        println!(
            "{name:16} height {bottom:.0}..{top:.0}: {hit:?} at {:.0} units",
            (at - from).length()
        );
        assert!(matches!(hit, Some(Hit::Char(0, _))), "{name} not hit");
        // Enough body shots kill it (the T-Rex takes the head).
        let mut n = 0;
        while sim.chars[0].health > 0 && n < 2000 {
            sim.shot_damage(&area, 0, false, 4.0);
            n += 1;
        }
        assert_eq!(sim.chars[0].health, 0);
    }
}
