//! Sound: the games' own software mixer, straight to the sound device.
//!
//! The originals mixed everything themselves - sixteen voices, a looped
//! ambient bed cross-fading between areas, a distance roll-off and a pan
//! from the listener's heading - and so does this, with the same curves. The mix is made at 22050 Hz,
//! the rate every sound in the games was recorded at, and the device's
//! own callback takes it, stepped up to
//! the device's rate with a band-limited (windowed sinc) filter.
//!
//! Bevy's audio is not used: it converts the stream again, linearly and
//! restarting the conversion every 512 samples, which clicks faintly all
//! through a loud, steady sound such as a call.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub type Pcm = Arc<[i16]>;

pub const RATE: u32 = 22050;
const MAX_CHANNEL: usize = 16;
const MIN_RADIUS: f32 = 512.0;
/// Frames mixed at a time. The original mixed 2048 and faded the ambient
/// 16/256 a block; smaller blocks answer sooner, and the fade is scaled.
const BLOCK: usize = 512;
const AMBIENT_FADE: i32 = 16 * BLOCK as i32 / 2048;

struct Channel {
    data: Pcm,
    pos: usize,
    at: Option<Vec3>,
    volume: i32,
}

#[derive(Default)]
struct AmbientCh {
    data: Option<Pcm>,
    pos: usize,
    volume: i32,
    avolume: i32,
}

pub struct MixerState {
    channels: Vec<Option<Channel>>,
    ambient: AmbientCh,
    ambient2: AmbientCh,
    /// A looped sound at a place, following it (the ship).
    loop3d: Option<(Pcm, usize, Vec3)>,
    cam: Vec3,
    cosa: f32,
    sina: f32,
    /// 0..1, from the volume option.
    pub master: f32,
    pub enabled: bool,
    mix: Vec<i32>,
}

impl Default for MixerState {
    fn default() -> Self {
        MixerState {
            channels: (0..MAX_CHANNEL).map(|_| None).collect(),
            ambient: AmbientCh::default(),
            ambient2: AmbientCh::default(),
            loop3d: None,
            cam: Vec3::ZERO,
            cosa: 1.0,
            sina: 0.0,
            master: 0.8,
            enabled: true,
            mix: vec![0; BLOCK * 2],
        }
    }
}

impl MixerState {
    /// Left and right gains, in 1/65536ths, for a voice.
    fn lr(&self, v0: i32, at: Option<Vec3>) -> (i32, i32) {
        let Some(p) = at else {
            return (v0 * 180, v0 * 180);
        };
        let v0 = (v0 * 200) as f32;
        let (x, y, z) = (p.x - self.cam.x, p.y - self.cam.y, p.z - self.cam.z);
        let xx = x * self.cosa + z * self.sina;
        let yy = y;
        let zz = (z * self.cosa - x * self.sina).abs();
        let xa = xx.abs();
        let mut d = (xx * xx + yy * yy + zz * zz).sqrt() - MIN_RADIUS;
        let mut k = if d <= 0.0 { 1.0 } else { 1224.0 / (1224.0 + d) };
        if d > 6000.0 {
            d -= 6000.0;
            k = k * (4000.0 - d) / 4000.0;
        }
        let k = k.max(0.0);
        let fi = xa.atan2(zz);
        let r = 0.7 + 0.3 * fi / (std::f32::consts::PI / 2.0);
        let l = 0.7 - 0.6 * fi / (std::f32::consts::PI / 2.0);
        if xx > 0.0 {
            ((v0 * l * k) as i32, (v0 * r * k) as i32)
        } else {
            ((v0 * r * k) as i32, (v0 * l * k) as i32)
        }
    }

    fn mix_ambient(mix: &mut [i32], a: &mut AmbientCh) {
        let Some(data) = &a.data else { return };
        if data.is_empty() {
            return;
        }
        let v = 32000 * a.volume * a.avolume / 256 / 256;
        for f in 0..BLOCK {
            if a.pos >= data.len() {
                a.pos = 0;
            }
            let s = (data[a.pos] as i32 * v) >> 16;
            mix[f * 2] += s;
            mix[f * 2 + 1] += s;
            a.pos += 1;
        }
    }

    fn mix_block(&mut self, out: &mut [i16]) {
        self.mix.iter_mut().for_each(|s| *s = 0);
        if self.enabled {
            for i in 0..MAX_CHANNEL {
                let Some(ch) = &self.channels[i] else {
                    continue;
                };
                let (lv, rv) = self.lr(ch.volume, ch.at);
                let ch = self.channels[i].as_mut().unwrap();
                let n = BLOCK.min(ch.data.len().saturating_sub(ch.pos));
                if lv != 0 || rv != 0 {
                    for f in 0..n {
                        let s = ch.data[ch.pos + f] as i32;
                        self.mix[f * 2] += (s * lv) >> 16;
                        self.mix[f * 2 + 1] += (s * rv) >> 16;
                    }
                }
                ch.pos += n;
                if ch.pos >= ch.data.len() {
                    self.channels[i] = None;
                }
            }
            if let Some((data, pos, at)) = self.loop3d.take() {
                let (lv, rv) = self.lr(256, Some(at));
                let mut p = pos;
                if !data.is_empty() {
                    for f in 0..BLOCK {
                        if p >= data.len() {
                            p = 0;
                        }
                        let s = data[p] as i32;
                        self.mix[f * 2] += (s * lv) >> 16;
                        self.mix[f * 2 + 1] += (s * rv) >> 16;
                        p += 1;
                    }
                }
                self.loop3d = Some((data, p, at));
            }
            Self::mix_ambient(&mut self.mix, &mut self.ambient);
            self.ambient.volume = (self.ambient.volume + AMBIENT_FADE).min(256);
            if self.ambient2.volume > 0 {
                Self::mix_ambient(&mut self.mix, &mut self.ambient2);
                self.ambient2.volume = (self.ambient2.volume - AMBIENT_FADE).max(0);
            }
        }
        let m = self.master;
        for (o, &s) in out.iter_mut().zip(self.mix.iter()) {
            *o = ((s as f32 * m) as i32).clamp(-32767, 32767) as i16;
        }
    }
}

#[derive(Resource, Clone, Default)]
pub struct Mixer(pub Arc<Mutex<MixerState>>);

impl Mixer {
    /// Plays a sound once: at a point in the world, or at the listener.
    pub fn play(&self, data: &Pcm, at: Option<Vec3>, volume: i32) {
        if data.is_empty() {
            return;
        }
        let Ok(mut m) = self.0.lock() else { return };
        if let Some(slot) = m.channels.iter_mut().find(|c| c.is_none()) {
            *slot = Some(Channel {
                data: data.clone(),
                pos: 0,
                at,
                volume,
            });
        }
    }

    /// The looped background, faded across from the one playing.
    pub fn set_ambient(&self, data: Option<&Pcm>, avolume: i32) {
        let Ok(mut m) = self.0.lock() else { return };
        let same = match (&m.ambient.data, data) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            m.ambient.avolume = avolume;
            return;
        }
        let old = std::mem::take(&mut m.ambient);
        m.ambient2 = old;
        m.ambient = AmbientCh {
            data: data.cloned(),
            pos: 0,
            volume: 0,
            avolume,
        };
    }

    /// Keeps a looped sound going at a place, or stops it with None.
    pub fn set_loop3d(&self, data: Option<&Pcm>, at: Vec3) {
        let Ok(mut m) = self.0.lock() else { return };
        match (data, m.loop3d.as_mut()) {
            (None, _) => m.loop3d = None,
            (Some(d), Some(l)) if Arc::ptr_eq(d, &l.0) => l.2 = at,
            (Some(d), _) => m.loop3d = Some((d.clone(), 0, at)),
        }
    }

    pub fn set_listener(&self, pos: Vec3, alpha: f32) {
        let Ok(mut m) = self.0.lock() else { return };
        m.cam = pos;
        m.cosa = alpha.cos();
        m.sina = alpha.sin();
    }

    pub fn set_volume(&self, master: f32, enabled: bool) {
        let Ok(mut m) = self.0.lock() else { return };
        m.master = master;
        m.enabled = enabled;
    }

    pub fn stop_all(&self) {
        let Ok(mut m) = self.0.lock() else { return };
        m.channels.iter_mut().for_each(|c| *c = None);
        m.ambient = AmbientCh::default();
        m.ambient2 = AmbientCh::default();
        m.loop3d = None;
    }
}

/// Zero crossings of the interpolating sinc on each side of the point.
const HALF_TAPS: usize = 32;
/// Filter rows kept per input sample; the ones between are interpolated.
const PHASES: usize = 128;
/// Where the filter starts cutting, as a fraction of 11025 Hz. The
/// sounds hold next to nothing that high.
const CUTOFF: f64 = 0.91;
/// The Kaiser window's shape: about 80 dB down past the band.
const KAISER_BETA: f64 = 8.0;

/// The interpolation filter: row p is the taps for a point p/PHASES of the
/// way between two input samples, each row summing to one.
fn sinc_table() -> Vec<f32> {
    fn bessel_i0(x: f64) -> f64 {
        let (mut sum, mut term) = (1.0, 1.0);
        for k in 1..50 {
            term *= (x / (2.0 * k as f64)).powi(2);
            sum += term;
        }
        sum
    }
    let taps = HALF_TAPS * 2;
    let mut t = vec![0.0f32; (PHASES + 1) * taps];
    for p in 0..=PHASES {
        let frac = p as f64 / PHASES as f64;
        let row = &mut t[p * taps..(p + 1) * taps];
        let mut w = vec![0.0f64; taps];
        for (j, w) in w.iter_mut().enumerate() {
            // Distance from input sample j of the window to the point.
            let x = frac + (HALF_TAPS - 1) as f64 - j as f64;
            let u = x / HALF_TAPS as f64;
            if u.abs() >= 1.0 {
                continue;
            }
            let y = std::f64::consts::PI * CUTOFF * x;
            let sinc = if y.abs() < 1e-9 { 1.0 } else { y.sin() / y };
            let win = bessel_i0(KAISER_BETA * (1.0 - u * u).sqrt()) / bessel_i0(KAISER_BETA);
            *w = CUTOFF * sinc * win;
        }
        let sum: f64 = w.iter().sum();
        for (o, w) in row.iter_mut().zip(&w) {
            *o = (w / sum) as f32;
        }
    }
    t
}

/// The mix taken from 22050 Hz to the output's rate.
struct Resampler {
    rate: u32,
    table: Vec<f32>,
    /// Mixed frames not yet left behind; the next output point lies
    /// `acc/rate` of the way past frame `idx + HALF_TAPS - 1`.
    buf: Vec<[f32; 2]>,
    idx: usize,
    acc: u32,
}

impl Resampler {
    fn new(rate: u32) -> Resampler {
        Resampler {
            rate,
            table: if rate == RATE {
                Vec::new()
            } else {
                sinc_table()
            },
            // Silence before the first frame, for the taps reaching back.
            buf: vec![[0.0; 2]; HALF_TAPS - 1],
            idx: 0,
            acc: 0,
        }
    }

    /// The next output frame, mixing more when it needs to.
    fn next_frame(&mut self, mut mix: impl FnMut(&mut Vec<[f32; 2]>)) -> [f32; 2] {
        let taps = HALF_TAPS * 2;
        while self.buf.len() < self.idx + taps {
            mix(&mut self.buf);
        }
        let out = if self.table.is_empty() {
            self.buf[self.idx + HALF_TAPS - 1]
        } else {
            let f = self.acc as f32 / self.rate as f32 * PHASES as f32;
            let p = (f as usize).min(PHASES - 1);
            let t = f - p as f32;
            let a = &self.table[p * taps..(p + 1) * taps];
            let b = &self.table[(p + 1) * taps..(p + 2) * taps];
            let src = &self.buf[self.idx..self.idx + taps];
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for j in 0..taps {
                let w = a[j] + (b[j] - a[j]) * t;
                l += src[j][0] * w;
                r += src[j][1] * w;
            }
            [l, r]
        };
        self.acc += RATE;
        while self.acc >= self.rate {
            self.acc -= self.rate;
            self.idx += 1;
        }
        if self.idx >= 4096 {
            self.buf.drain(..self.idx);
            self.idx = 0;
        }
        out
    }
}

/// The device's side: the mix, stepped up to the device's rate.
struct Output {
    state: Arc<Mutex<MixerState>>,
    block: Vec<i16>,
    rs: Resampler,
}

impl Output {
    fn new(state: Arc<Mutex<MixerState>>, rate: u32) -> Output {
        Output {
            state,
            block: vec![0; BLOCK * 2],
            rs: Resampler::new(rate),
        }
    }

    fn next_frame(&mut self) -> [f32; 2] {
        let (state, block) = (&self.state, &mut self.block);
        let [l, r] = self.rs.next_frame(|buf| {
            if let Ok(mut m) = state.lock() {
                m.mix_block(block);
            } else {
                block.iter_mut().for_each(|s| *s = 0);
            }
            buf.extend(
                block
                    .chunks_exact(2)
                    .map(|f| [f[0] as f32 / 32768.0, f[1] as f32 / 32768.0]),
            );
        });
        [l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0)]
    }
}

fn build<T>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    mut out: Output,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let ch = cfg.channels.max(1) as usize;
    dev.build_output_stream(
        cfg,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for f in data.chunks_mut(ch) {
                let [l, r] = out.next_frame();
                if ch == 1 {
                    f[0] = <T as cpal::Sample>::from_sample((l + r) * 0.5);
                    continue;
                }
                f[0] = <T as cpal::Sample>::from_sample(l);
                f[1] = <T as cpal::Sample>::from_sample(r);
                for s in &mut f[2..] {
                    *s = <T as cpal::Sample>::EQUILIBRIUM;
                }
            }
        },
        |e| warn!("sound: {e}"),
        None,
    )
}

/// The device's configuration, at 48 kHz where it takes that: the rate
/// sound servers run at, so nothing converts it again on the way.
fn pick_config(
    dev: &cpal::Device,
    def: cpal::SupportedStreamConfig,
) -> cpal::SupportedStreamConfig {
    const WANT: u32 = 48_000;
    if def.sample_rate().0 == WANT {
        return def;
    }
    dev.supported_output_configs()
        .ok()
        .and_then(|mut all| {
            all.find(|r| {
                r.channels() == def.channels()
                    && r.sample_format() == def.sample_format()
                    && r.min_sample_rate().0 <= WANT
                    && WANT <= r.max_sample_rate().0
            })
        })
        .map(|r| r.with_sample_rate(cpal::SampleRate(WANT)))
        .unwrap_or(def)
}

/// Opens the sound device and starts the mix playing on it: the system's
/// default, or where that will not open, the first other device that
/// will, in the order the system lists them (the sound server's own come
/// first) - the same choice Bevy's audio made.
///
/// The list is walked one device at a time and left at the first that
/// works: listing a device opens it (ALSA, both directions), and holding
/// every one open at once - the sound card's own among them - takes the
/// card from the sound server and leaves the whole system silent.
fn open_output(state: &Arc<Mutex<MixerState>>) -> Option<(cpal::Stream, String, u32, u16)> {
    let host = cpal::default_host();
    if let Some(found) = host
        .default_output_device()
        .and_then(|d| start_on(&d, state))
    {
        return Some(found);
    }
    let devices = host.output_devices().ok()?;
    for dev in devices {
        if let Some(found) = start_on(&dev, state) {
            return Some(found);
        }
    }
    None
}

/// The mix playing on one device, if it will take it.
fn start_on(
    dev: &cpal::Device,
    state: &Arc<Mutex<MixerState>>,
) -> Option<(cpal::Stream, String, u32, u16)> {
    let def = dev.default_output_config().ok()?;
    let cfg = pick_config(dev, def);
    let rate = cfg.sample_rate().0;
    let sc = cfg.config();
    let out = Output::new(state.clone(), rate);
    use cpal::SampleFormat as F;
    let stream = match cfg.sample_format() {
        F::F32 => build::<f32>(dev, &sc, out),
        F::I16 => build::<i16>(dev, &sc, out),
        F::U16 => build::<u16>(dev, &sc, out),
        F::I32 => build::<i32>(dev, &sc, out),
        F::F64 => build::<f64>(dev, &sc, out),
        F::I8 => build::<i8>(dev, &sc, out),
        F::U8 => build::<u8>(dev, &sc, out),
        _ => return None,
    }
    .ok()?;
    stream.play().ok()?;
    Some((stream, dev.name().unwrap_or_default(), rate, sc.channels))
}

/// The playing stream; it stops when dropped.
struct SoundOut(#[allow(dead_code)] cpal::Stream);

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Mixer>()
            .add_systems(Startup, start_stream)
            .add_systems(Update, apply_volume);
    }
}

fn start_stream(world: &mut World) {
    let state = world.resource::<Mixer>().0.clone();
    match open_output(&state) {
        Some((stream, name, rate, ch)) => {
            info!("sound: {name}, {rate} Hz, {ch} channels (mixed at {RATE} Hz)");
            world.insert_non_send_resource(SoundOut(stream));
        }
        None => warn!("sound: no output device"),
    }
}

fn apply_volume(settings: Res<crate::settings::Settings>, mixer: Res<Mixer>) {
    if settings.is_changed() {
        mixer.set_volume(settings.volume as f32 / 255.0, settings.sound);
    }
}

/// A sound file as 22050 Hz samples, resampled if it was recorded at
/// another rate.
pub fn wave_to_pcm(w: &carn_formats::wav::Wave) -> Pcm {
    if w.rate == RATE || w.rate == 0 {
        return w.samples.clone().into();
    }
    let n = (w.samples.len() as u64 * RATE as u64 / w.rate as u64) as usize;
    let step = w.rate as f64 / RATE as f64;
    (0..n)
        .map(|i| {
            let t = i as f64 * step;
            let j = t as usize;
            let f = (t - j as f64) as f32;
            let a = w.samples.get(j).copied().unwrap_or(0) as f32;
            let b = w.samples.get(j + 1).copied().unwrap_or(0) as f32;
            (a + (b - a) * f) as i16
        })
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(m: &Mixer) -> Vec<i16> {
        let mut out = vec![0i16; BLOCK * 2];
        m.0.lock().unwrap().mix_block(&mut out);
        out
    }

    #[test]
    fn voice_at_listener_is_centred() {
        let m = Mixer::default();
        m.set_volume(1.0, true);
        let tone: Pcm = vec![10000i16; 4000].into();
        m.play(&tone, None, 256);
        let out = block(&m);
        // 10000 * 256*180 >> 16
        assert_eq!(out[0], ((10000 * 256 * 180) >> 16) as i16);
        assert_eq!(out[0], out[1]);
    }

    #[test]
    fn distant_voice_is_quieter_and_panned() {
        let m = Mixer::default();
        m.set_volume(1.0, true);
        m.set_listener(Vec3::ZERO, 0.0);
        let tone: Pcm = vec![10000i16; 4000].into();
        // Off to the right (+x with the listener facing -z).
        m.play(&tone, Some(Vec3::new(3000.0, 0.0, 0.0)), 256);
        let out = block(&m);
        assert!(out[1] > out[0], "right louder: {} {}", out[0], out[1]);
        assert!((out[1] as i32) < (10000 * 256 * 180) >> 16);
        // Past 10000 units, silence.
        let m = Mixer::default();
        m.play(&tone, Some(Vec3::new(20000.0, 0.0, 0.0)), 256);
        assert!(block(&m).iter().all(|&s| s == 0));
    }

    /// Strength of a frequency in a signal (Goertzel), as an amplitude.
    fn tone_level(x: &[f32], freq: f32, rate: f32) -> f32 {
        let w = 2.0 * std::f32::consts::PI * freq / rate;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &v in x {
            let s0 = v + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - c * s1 * s2).sqrt() * 2.0 / x.len() as f32
    }

    fn upsample(input: &[f32], rate: u32, n: usize) -> Vec<f32> {
        let mut rs = Resampler::new(rate);
        let mut at = 0;
        (0..n)
            .map(|_| {
                rs.next_frame(|buf| {
                    for _ in 0..BLOCK {
                        let v = input.get(at).copied().unwrap_or(0.0);
                        buf.push([v, v]);
                        at += 1;
                    }
                })[0]
            })
            .collect()
    }

    #[test]
    fn upsampling_keeps_the_tone_and_mirrors_nothing() {
        // A bright 9 kHz tone, the kind a call is full of.
        let f = 9000.0;
        let input: Vec<f32> = (0..22050)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * f * i as f32 / RATE as f32).sin())
            .collect();
        for rate in [44100u32, 48000] {
            let out = upsample(&input, rate, rate as usize / 2);
            // Past the filter's start-up.
            let out = &out[rate as usize / 10..];
            let tone = tone_level(out, f, rate as f32);
            // Where a linear step up mirrors it: 22050 - 9000 Hz.
            let image = tone_level(out, RATE as f32 - f, rate as f32);
            assert!((tone - 0.5).abs() < 0.03, "{rate}: tone {tone}");
            assert!(image < 0.5 * 0.003, "{rate}: image {image}");
        }
        // At the mix's own rate it is passed through untouched.
        let out = upsample(&input, RATE, 2000);
        assert_eq!(&out[HALF_TAPS..1000], &input[HALF_TAPS..1000]);
    }

    #[test]
    fn a_long_call_plays_without_a_click() {
        // Five seconds of a steady 1 kHz tone, played as a call is (at the
        // listener), through the mixer's blocks and the device's side.
        let m = Mixer::default();
        m.set_volume(1.0, true);
        let tone: Pcm = (0..RATE as usize * 5)
            .map(|i| {
                (10000.0 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / RATE as f32).sin())
                    as i16
            })
            .collect::<Vec<_>>()
            .into();
        m.play(&tone, None, 256);
        for rate in [44100u32, 48000] {
            let mut out = Output::new(m.0.clone(), rate);
            let x: Vec<f32> = (0..rate as usize * 2)
                .map(|_| out.next_frame()[0])
                .collect();
            // A sine's second difference stays under A*(2 pi f / rate)^2; a
            // dropped, doubled or stepped sample stands far out of it.
            let a = 10000.0 * (256.0 * 180.0 / 65536.0) / 32768.0;
            let w = 2.0 * std::f32::consts::PI * 1000.0 / rate as f32;
            let limit = a * w * w * 1.3 + 2e-4;
            let x = &x[100..];
            let worst = x
                .windows(3)
                .map(|v| (v[2] - 2.0 * v[1] + v[0]).abs())
                .fold(0.0f32, f32::max);
            assert!(worst < limit, "{rate}: {worst} >= {limit}");
        }
    }

    #[test]
    fn ambient_fades_in_and_loops() {
        let m = Mixer::default();
        m.set_volume(1.0, true);
        let bed: Pcm = vec![8000i16; 300].into();
        m.set_ambient(Some(&bed), 256);
        let first = block(&m);
        assert_eq!(first[0], 0, "starts silent");
        for _ in 0..70 {
            block(&m);
        }
        let later = block(&m);
        assert!(
            later[0] > 3000 && later[BLOCK * 2 - 1] > 3000,
            "faded in and looping"
        );
    }
}
