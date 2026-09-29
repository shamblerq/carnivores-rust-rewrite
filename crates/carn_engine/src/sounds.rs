//! What the hunt sounds like: the area's ambient bed and its random calls,
//! the hunter's steps, and whatever the creatures say.

use bevy::prelude::*;
use carn_formats::wav::Wave;

use crate::area::Area;
use crate::audio::{wave_to_pcm, Mixer, Pcm};
use crate::paths::DataRoot;
use crate::player::Player;
use crate::sim::{Rng, Sim, SoundRef};

#[derive(Resource, Default)]
pub struct SoundBank {
    pub steps: Vec<Pcm>,
    pub steps_water: Vec<Pcm>,
    pub screams: Vec<Pcm>,
    pub underwater: Option<Pcm>,
    pub random: Vec<Pcm>,
    pub ambients: Vec<Pcm>,
    /// Per creature type, its .CAR's sounds.
    pub chars: Vec<Vec<Pcm>>,
    pub ship: Vec<Pcm>,
    /// The dinosaur calls, by number: callN_a/b/c.wav.
    pub calls: std::collections::HashMap<i32, Vec<Pcm>>,
}

/// Countdown to each ambient's next random call.
#[derive(Resource, Default)]
pub struct AmbientTimers {
    pub rnd_time: Vec<i32>,
    pub rng: Option<Rng>,
}

fn load(root: &DataRoot, rel: &str) -> Option<Pcm> {
    let p = root.find(rel)?;
    match Wave::load(&p) {
        Ok(w) => Some(wave_to_pcm(&w)),
        Err(e) => {
            warn!("{e}");
            None
        }
    }
}

fn load_set(root: &DataRoot, names: &[&str]) -> Vec<Pcm> {
    names.iter().filter_map(|n| load(root, n)).collect()
}

pub fn build(root: &DataRoot, area: &Area, sim: Option<&Sim>) -> (SoundBank, AmbientTimers) {
    let bank = SoundBank {
        steps: load_set(
            root,
            &[
                "HUNTDAT/SOUNDFX/STEPS/hwalk1.wav",
                "HUNTDAT/SOUNDFX/STEPS/hwalk2.wav",
                "HUNTDAT/SOUNDFX/STEPS/hwalk3.wav",
            ],
        ),
        steps_water: load_set(
            root,
            &[
                "HUNTDAT/SOUNDFX/STEPS/footw1.wav",
                "HUNTDAT/SOUNDFX/STEPS/footw2.wav",
                "HUNTDAT/SOUNDFX/STEPS/footw3.wav",
            ],
        ),
        screams: load_set(
            root,
            &[
                "HUNTDAT/SOUNDFX/hum_die1.wav",
                "HUNTDAT/SOUNDFX/hum_die2.wav",
                "HUNTDAT/SOUNDFX/hum_die3.wav",
                "HUNTDAT/SOUNDFX/hum_die4.wav",
            ],
        ),
        underwater: load(root, "HUNTDAT/SOUNDFX/a_underw.wav"),
        random: area
            .rsc
            .random_sounds
            .iter()
            .map(|s| Pcm::from(s.as_slice()))
            .collect(),
        ambients: area
            .rsc
            .ambients
            .iter()
            .map(|a| Pcm::from(a.sound.as_slice()))
            .collect(),
        chars: sim
            .map(|s| {
                s.chinfo
                    .iter()
                    .map(|ci| {
                        ci.as_ref()
                            .map(|ci| ci.sounds.iter().map(|x| Pcm::from(x.as_slice())).collect())
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ship: sim
            .and_then(|s| s.ship_info.as_ref())
            .map(|ci| ci.sounds.iter().map(|x| Pcm::from(x.as_slice())).collect())
            .unwrap_or_default(),
        calls: sim
            .map(|s| {
                s.callable()
                    .iter()
                    .filter_map(|&t| s.call_number(t))
                    .map(|n| {
                        let v = ["a", "b", "c"]
                            .iter()
                            .filter_map(|x| {
                                load(root, &format!("HUNTDAT/SOUNDFX/CALLS/call{n}_{x}.wav"))
                            })
                            .collect();
                        (n, v)
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut rng = Rng(0x1234_5678 ^ area.size as u32);
    let rnd_time = area
        .rsc
        .ambients
        .iter()
        .map(|a| {
            let f = a.rdata.first().map(|r| r.freq).unwrap_or(0);
            if a.count > 0 {
                (f / 2 + rng.r(f)) * 1000
            } else {
                0
            }
        })
        .collect();
    (
        bank,
        AmbientTimers {
            rnd_time,
            rng: Some(rng),
        },
    )
}

/// Night keeps only the ambient calls not marked as daytime ones
/// (time of day 2).
fn ambient_calls(area: &Area, i: usize) -> Vec<carn_formats::rsc::Trd> {
    let Some(a) = area.rsc.ambients.get(i) else {
        return Vec::new();
    };
    let mut v: Vec<_> = a.rdata.iter().take(a.count).copied().collect();
    if area.opt.day_night == 2 && area.kind.has_time_of_day() {
        v.retain(|r| r.flags == 0);
    }
    v
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mixer: Res<Mixer>,
    bank: Option<Res<SoundBank>>,
    mut timers: ResMut<AmbientTimers>,
    mut player: ResMut<Player>,
    sim: Option<ResMut<Sim>>,
    area: Res<Area>,
    time: Res<Time>,
    view: Res<crate::ViewState>,
) {
    let Some(bank) = bank else { return };
    mixer.set_listener(player.cam, player.cam_alpha);
    if view.paused {
        return;
    }
    let dt = (time.delta_secs() * 1000.0) as i32;
    let rt = player.real_time as i32;

    if let Some((water, vol)) = player.footstep.take() {
        let set = if water && !bank.steps_water.is_empty() {
            &bank.steps_water
        } else {
            &bank.steps
        };
        if !set.is_empty() {
            mixer.play(
                &set[rt.unsigned_abs() as usize % set.len()],
                None,
                vol.clamp(0, 256),
            );
        }
    }

    if let Some(mut sim) = sim {
        // The ship's engine follows it about.
        if sim.ship.state != -1 {
            mixer.set_loop3d(bank.ship.first(), sim.ship.pos);
        } else {
            mixer.set_loop3d(None, Vec3::ZERO);
        }
        for ev in sim.sounds.drain(..) {
            match ev.sound {
                SoundRef::Char(t, i) => {
                    if let Some(p) = bank.chars.get(t).and_then(|v| v.get(i)) {
                        mixer.play(p, ev.pos, ev.volume);
                    }
                }
                SoundRef::Ship(i) => {
                    if let Some(p) = bank.ship.get(i) {
                        mixer.play(p, ev.pos, ev.volume);
                    }
                }
                SoundRef::Call(n, v) => {
                    if let Some(p) = bank.calls.get(&n).and_then(|c| c.get(v)) {
                        mixer.play(p, ev.pos, ev.volume);
                    }
                }
                SoundRef::Scream => {
                    if !bank.screams.is_empty() {
                        let i = (rt.unsigned_abs() as usize / 7) % bank.screams.len();
                        mixer.play(&bank.screams[i], None, ev.volume);
                    }
                }
            }
        }
    }

    if player.underwater {
        mixer.set_ambient(bank.underwater.as_ref(), 240);
        return;
    }
    let zone = area.ambient_zone(player.cam.x, player.cam.z);
    let Some(amb) = area.rsc.ambients.get(zone) else {
        mixer.set_ambient(None, 0);
        return;
    };
    mixer.set_ambient(bank.ambients.get(zone), amb.volume);

    let calls = ambient_calls(&area, zone);
    if calls.is_empty() || zone >= timers.rnd_time.len() {
        return;
    }
    timers.rnd_time[zone] -= dt;
    if timers.rnd_time[zone] <= 0 {
        let f = amb.rdata.first().map(|r| r.freq).unwrap_or(0);
        let rng = timers.rng.get_or_insert(Rng(1));
        let next = (f / 2 + rng.r(f)) * 1000;
        let rr = rng.rand() as usize % calls.len();
        let (ox, oy, oz) = (rng.si(4096), rng.si(256), rng.si(4096));
        timers.rnd_time[zone] = next.max(1000);
        let c = calls[rr];
        if let Some(p) = bank.random.get(c.number.max(0) as usize) {
            let at = player.cam + Vec3::new(ox as f32, oy as f32, oz as f32);
            mixer.play(p, Some(at), c.volume);
        }
    }
}
