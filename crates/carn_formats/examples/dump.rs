//! Loads an area or character and prints a summary: a quick check of the readers.
//! Usage: dump c1|c2 FILE...
use carn_formats::{
    car::CharacterInfo,
    map::Map,
    rsc::{LoadOptions, Rsc},
    Engine,
};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let engine = if args[1] == "c1" {
        Engine::C1
    } else {
        Engine::C2
    };
    for f in &args[2..] {
        let p = Path::new(f);
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        match ext.as_str() {
            "RSC" => match Rsc::load(p, engine, LoadOptions::default()) {
                Ok(r) => {
                    println!(
                        "{f}: {} textures, {} objects ({} animated), {} fogs, {} sounds, {} ambients, {} waters, fade {:?}",
                        r.textures.len(), r.objects.len(),
                        r.objects.iter().filter(|o| o.anim.is_some()).count(),
                        r.fogs.len() - 1, r.random_sounds.len(), r.ambients.len(), r.waters.len(),
                        r.sky_fade(LoadOptions::default()));
                    for fg in &r.fogs[1..] {
                        println!(
                            "   fog {:06X} yb {} transp {} limit {} mortal {}",
                            fg.rgb, fg.y_begin, fg.transp, fg.limit, fg.mortal
                        );
                    }
                    for w in &r.waters {
                        println!(
                            "   water tex {} level {} transp {} rgb {:06X}",
                            w.tindex, w.level, w.transp, w.fog_rgb
                        );
                    }
                }
                Err(e) => println!("{f}: ERROR {e}"),
            },
            "MAP" => match Map::load(p, engine, 1) {
                Ok(m) => {
                    let objs = m.omap.iter().filter(|&&o| o < 254).count();
                    let land = m.omap.iter().filter(|&&o| o == 254).count();
                    let water = (0..m.size * m.size).filter(|&i| m.water(i)).count();
                    let maxt = m.tmap1.iter().max().unwrap();
                    println!("{f}: {} cells, {objs} objects, {land} landings, {water} water cells, max tex {maxt}", m.size);
                }
                Err(e) => println!("{f}: ERROR {e}"),
            },
            "CAR" => match CharacterInfo::load(p, None) {
                Ok(c) => {
                    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for v in &c.model.vertices {
                        for k in 0..3 {
                            lo[k] = lo[k].min(v.pos[k]);
                            hi[k] = hi[k].max(v.pos[k]);
                        }
                    }
                    println!(
                        "{f}: '{}' {} verts {} faces, anims {:?}, {} sounds, bounds {:?}..{:?}",
                        c.name,
                        c.model.vertices.len(),
                        c.model.faces.len(),
                        c.animations
                            .iter()
                            .map(|a| format!("{}:{}f/{}ms", a.name, a.frames, a.ani_time))
                            .collect::<Vec<_>>(),
                        c.sounds.len(),
                        lo,
                        hi
                    )
                }
                Err(e) => println!("{f}: ERROR {e}"),
            },
            _ => println!("{f}: ?"),
        }
    }
}
