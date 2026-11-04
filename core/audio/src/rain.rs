//! The weather's own sound: rain, and a storm's wind, made as it plays. **Not retail**: the game
//! has no rain sound. Nothing is a recording, so nothing loops: every drop falls at a moment and
//! a place of its own, for as long as it rains.
//!
//! The rain is three layers: a wash of pink noise, the hiss of countless far drops; a warm patter
//! on the ground under it; and drops near enough to hear one by one, each a soft splat of noise
//! or a crisp tap, and under some taps the low plunk of the bubble the drop leaves, gliding up as
//! the bubble shrinks (after van den Doel's bubble model of liquid sound). A storm brings more and
//! heavier drops, heavier still in its gusts, and a wind that swells and falls with them. Indoors
//! all of it is muffled.
//!
//! It is drawn at the device's own rate, beside the [`MIX_RATE`](crate::MIX_RATE) mix rather
//! than in it, so it keeps the whole of its range; nothing in it allocates once made.

use dereth_primitives::num::math::{expf, sinf};

/// What the weather asks of the sound, set each frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeatherSound {
    /// How much rain falls, 0 to 1.
    pub rain: f32,
    /// How far a storm has come in, 0 to 1.
    pub storm: f32,
    /// How hard the storm gusts now, 0 to 1.
    pub gust: f32,
    /// 1 indoors, where it is heard muffled through the walls.
    pub indoors: f32,
    /// How loud, from the sound preferences: 0 for none.
    pub gain: f32,
}

/// The overall level: a full storm peaks a little under full scale at gain 1.
const MASTER: f32 = 0.25;
/// How long a change of the weather takes to be heard in full, seconds.
const GLIDE_SECONDS: f32 = 0.4;
/// The most drops sounding at once, of each kind.
const MOST_SPLATS: usize = 512;
const MOST_TAPS: usize = 128;
const TAU: f32 = std::f32::consts::TAU;

/// A one-pole low pass.
#[derive(Debug, Clone, Copy)]
struct Low {
    a: f32,
    y: f32,
}

impl Low {
    fn new(cut: f32, rate: f32) -> Self {
        Self {
            a: 1.0 - expf(-TAU * cut / rate),
            y: 0.0,
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }
}

/// A band pass of two one-poles.
#[derive(Debug, Clone, Copy)]
struct Band {
    lo: Low,
    hi: Low,
}

impl Band {
    fn new(from: f32, to: f32, rate: f32) -> Self {
        Self {
            lo: Low::new(to, rate),
            hi: Low::new(from, rate),
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let l = self.lo.run(x);
        l - self.hi.run(l)
    }
}

/// Pink noise from white (Paul Kellet's economy filter).
#[derive(Debug, Clone, Copy, Default)]
struct Pink([f32; 3]);

impl Pink {
    fn run(&mut self, w: f32) -> f32 {
        let b = &mut self.0;
        b[0] = 0.997_65 * b[0] + w * 0.099_046;
        b[1] = 0.963 * b[1] + w * 0.296_516_4;
        b[2] = 0.57 * b[2] + w * 1.052_691_3;
        (b[0] + b[1] + b[2] + w * 0.1848) * 0.2
    }
}

/// A far drop: a soft splat of noise through its own low pass.
#[derive(Debug, Clone, Copy)]
struct Splat {
    amp: f32,
    rising: bool,
    attack: f32,
    env: f32,
    decay: f32,
    filt: [Low; 2],
    pan: f32,
}

/// A near drop: a tap of noise, and perhaps its bubble's plunk.
#[derive(Debug, Clone, Copy)]
struct Tap {
    amp: f32,
    env: f32,
    decay: f32,
    band: Band,
    plunk: f32,
    plunk_env: f32,
    plunk_decay: f32,
    plunk_wait: u32,
    phase: f32,
    freq: f32,
    pan: f32,
}

/// The generator.
#[derive(Debug)]
pub struct WeatherBed {
    rate: f32,
    seed: u64,
    asked: WeatherSound,
    now: WeatherSound,
    glide: f32,
    pink: [Pink; 4],
    wash: [[Low; 2]; 2],
    wash_floor: [Low; 2],
    body: [Band; 2],
    wind: [Band; 2],
    wind_swell: [Low; 2],
    air: [Low; 2],
    muffle: [[Low; 2]; 2],
    splats: Vec<Splat>,
    taps: Vec<Tap>,
}

impl WeatherBed {
    /// A silent generator for a device at `rate` frames a second.
    #[must_use]
    pub fn new(rate: u32) -> Self {
        #[allow(clippy::cast_precision_loss)] // a device rate
        let rate = rate.max(8000) as f32;
        let nyq = rate * 0.45;
        let wash_top = 3600.0f32.min(nyq);
        let air = 7000.0f32.min(nyq);
        Self {
            rate,
            seed: 0x9E37_79B9_7F4A_7C15,
            asked: WeatherSound::default(),
            now: WeatherSound::default(),
            glide: 1.0 - expf(-1.0 / (GLIDE_SECONDS * rate)),
            pink: [Pink::default(); 4],
            wash: [[Low::new(wash_top, rate); 2]; 2],
            wash_floor: [Low::new(180.0, rate); 2],
            body: [Band::new(140.0, 650.0, rate); 2],
            wind: [Band::new(90.0, 700.0, rate); 2],
            wind_swell: [Low::new(30.0, rate); 2],
            air: [Low::new(air, rate); 2],
            muffle: [[Low::new(650.0, rate); 2]; 2],
            splats: Vec::with_capacity(MOST_SPLATS),
            taps: Vec::with_capacity(MOST_TAPS),
        }
    }

    /// Ask for the weather `w`; the sound glides to it.
    pub fn set(&mut self, w: WeatherSound) {
        self.asked = w;
    }

    /// Whether it makes no sound and is asked for none.
    #[must_use]
    pub fn is_silent(&self) -> bool {
        let quiet = |w: &WeatherSound| w.gain < 1.0e-4 || (w.rain < 1.0e-4 && w.storm < 1.0e-4);
        quiet(&self.asked) && quiet(&self.now) && self.splats.is_empty() && self.taps.is_empty()
    }

    fn next(&mut self) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    fn unit(&mut self) -> f32 {
        #[allow(clippy::cast_precision_loss)] // 24 bits
        let u = (self.next() >> 40) as f32 / 16_777_216.0;
        u
    }

    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// Add the weather to `out`, interleaved with `channels` channels: the first two take the
    /// left and the right, a single channel both.
    pub fn add_to(&mut self, out: &mut [f32], channels: u16) {
        if self.is_silent() {
            self.now = self.asked;
            return;
        }
        let channels = usize::from(channels.max(1));
        for frame in out.chunks_mut(channels) {
            let [l, r] = self.frame();
            if channels == 1 {
                frame[0] += (l + r) * 0.5;
            } else {
                frame[0] += l;
                frame[1] += r;
            }
        }
    }

    /// One frame, left and right.
    fn frame(&mut self) -> [f32; 2] {
        let k = self.glide;
        let (a, n) = (self.asked, &mut self.now);
        for (v, to) in [
            (&mut n.rain, a.rain),
            (&mut n.storm, a.storm),
            (&mut n.gust, a.gust),
            (&mut n.indoors, a.indoors),
            (&mut n.gain, a.gain),
        ] {
            *v += (to - *v) * k;
        }
        let w = self.now;
        let mix = |x: f32, y: f32| x + (y - x) * w.storm;
        let g = w.gust;
        let heavy = mix(1.0, 0.7 + 0.6 * g);
        self.spawn(
            w.rain * mix(260.0, 700.0) * heavy,
            w.rain * mix(55.0, 160.0) * heavy,
        );
        let mut lr = self.drops();
        let wash_level = w.rain * mix(0.32, 0.55 + 0.35 * g);
        let body_level = w.rain * mix(0.3, 0.5 + 0.3 * g);
        for (c, v) in lr.iter_mut().enumerate() {
            let white = self.signed();
            let p = self.pink[c].run(white);
            let once = self.wash[c][0].run(p);
            let washed = self.wash[c][1].run(once);
            *v += (washed - self.wash_floor[c].run(washed)) * wash_level * 2.5;
            let white = self.signed();
            let warm = self.pink[c + 2].run(white);
            *v += self.body[c].run(warm) * body_level * 3.0;
            if w.storm > 0.0 {
                let s = self.signed() * 6.0;
                let swell = 0.6 + 0.4 * self.wind_swell[c].run(s).clamp(-1.0, 1.0);
                let noise = self.signed();
                *v += self.wind[c].run(noise) * swell * (0.15 + 0.85 * g * g) * 4.5 * w.storm;
            }
            let open = self.air[c].run(*v);
            let once = self.muffle[c][0].run(*v);
            let walled = self.muffle[c][1].run(once);
            let heard = open + (walled * 0.6 - open) * w.indoors;
            *v = heard * MASTER * w.gain;
        }
        lr
    }

    /// Start the drops due this frame: far splats at `splats` a second, near taps at `taps`.
    fn spawn(&mut self, splats: f32, taps: f32) {
        let rate = self.rate;
        let nyq = rate * 0.45;
        if self.unit() < splats / rate && self.splats.len() < MOST_SPLATS {
            let size = self.unit().powi(2) * self.unit().sqrt();
            let cut = ((2600.0 - 2100.0 * size) * (0.7 + 0.6 * self.unit())).min(nyq * 0.9);
            let pan = self.unit();
            self.splats.push(Splat {
                amp: 0.05 + 0.5 * size,
                rising: true,
                attack: 1.0 / (0.0015 * rate),
                env: 0.0,
                decay: expf(-1.0 / ((0.006 + 0.03 * size) * rate)),
                filt: [Low::new(cut, rate); 2],
                pan,
            });
        }
        if self.unit() < taps / rate && self.taps.len() < MOST_TAPS {
            let size = self.unit().powi(2);
            let top = (4500.0 + 2500.0 * self.unit()).min(nyq * 0.9);
            let from = 600.0 + 500.0 * self.unit();
            let plunk = if self.unit() < 0.35 {
                0.35 + 0.4 * self.unit()
            } else {
                0.0
            };
            let plunk_life = 0.012 + 0.014 * self.unit();
            let freq = 450.0 + 850.0 * self.unit() * (1.0 - 0.5 * size);
            let pan = self.unit();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few ms
            let plunk_wait = (0.002 * rate) as u32;
            self.taps.push(Tap {
                amp: 0.12 + 0.55 * size,
                env: 1.0,
                decay: expf(-1.0 / ((0.002 + 0.006 * size) * rate)),
                band: Band::new(from, top, rate),
                plunk,
                plunk_env: 1.0,
                plunk_decay: expf(-1.0 / (plunk_life * rate)),
                plunk_wait,
                phase: 0.0,
                freq,
                pan,
            });
        }
    }

    /// Every sounding drop's share of this frame, left and right; the spent ones go.
    fn drops(&mut self) -> [f32; 2] {
        let (mut l, mut r) = (0.0, 0.0);
        let rate = self.rate;
        let mut k = 0;
        while k < self.taps.len() {
            let noise = self.signed();
            let d = &mut self.taps[k];
            let mut s = d.band.run(noise * d.env) * d.amp * 3.0;
            d.env *= d.decay;
            if d.plunk > 0.0 {
                if d.plunk_wait > 0 {
                    d.plunk_wait -= 1;
                } else {
                    s += sinf(d.phase) * d.plunk * d.amp * d.plunk_env;
                    d.phase = (d.phase + TAU * d.freq / rate) % TAU;
                    d.freq *= 1.0 + 12.0 / rate;
                    d.plunk_env *= d.plunk_decay;
                }
            }
            l += s * (1.0 - d.pan).sqrt();
            r += s * d.pan.sqrt();
            if d.env > 0.002 || (d.plunk > 0.0 && d.plunk_env > 0.002) {
                k += 1;
            } else {
                self.taps.swap_remove(k);
            }
        }
        let mut k = 0;
        while k < self.splats.len() {
            let noise = self.signed();
            let d = &mut self.splats[k];
            if d.rising {
                d.env += d.attack;
                if d.env >= 1.0 {
                    d.env = 1.0;
                    d.rising = false;
                }
            } else {
                d.env *= d.decay;
            }
            let once = d.filt[0].run(noise * d.env * d.amp);
            let s = d.filt[1].run(once) * 2.0;
            l += s * (1.0 - d.pan).sqrt();
            r += s * d.pan.sqrt();
            if d.rising || d.env > 0.002 {
                k += 1;
            } else {
                self.splats.swap_remove(k);
            }
        }
        [l, r]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// `seconds` of `w`, stereo, after a few seconds for the glide to settle.
    fn render(w: WeatherSound, seconds: f32) -> Vec<f32> {
        let mut bed = WeatherBed::new(RATE);
        bed.set(w);
        let mut settle = vec![0.0; RATE as usize * 2 * 3];
        bed.add_to(&mut settle, 2);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mut out = vec![0.0; (RATE as f32 * seconds) as usize * 2];
        bed.add_to(&mut out, 2);
        out
    }

    fn rms(v: &[f32]) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let n = v.len().max(1) as f32;
        (v.iter().map(|s| s * s).sum::<f32>() / n).sqrt()
    }

    fn rain(rain: f32, storm: f32) -> WeatherSound {
        WeatherSound {
            rain,
            storm,
            gust: 0.5,
            indoors: 0.0,
            gain: 1.0,
        }
    }

    #[test]
    fn no_rain_or_no_gain_is_silence_and_costs_nothing() {
        for w in [
            rain(0.0, 0.0),
            WeatherSound {
                gain: 0.0,
                ..rain(1.0, 1.0)
            },
        ] {
            let mut bed = WeatherBed::new(RATE);
            bed.set(w);
            let mut out = vec![0.25; 4096];
            bed.add_to(&mut out, 2);
            assert!(out.iter().all(|s| *s == 0.25), "{w:?} added to the mix");
            assert!(bed.is_silent());
        }
    }

    #[test]
    fn rain_is_heard_a_storm_louder_and_never_past_full_scale() {
        let light = rms(&render(rain(1.0, 0.0), 4.0));
        let storm_out = render(
            WeatherSound {
                gust: 1.0,
                ..rain(1.0, 1.0)
            },
            4.0,
        );
        let storm = rms(&storm_out);
        assert!(light > 0.01, "rain at {light}");
        assert!(storm > light * 1.5, "storm {storm} against rain {light}");
        let peak = storm_out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            peak < 1.0 && storm_out.iter().all(|s| s.is_finite()),
            "peak {peak}"
        );
    }

    #[test]
    fn indoors_the_rain_is_quieter_and_its_hiss_is_gone() {
        let out = render(rain(1.0, 0.0), 3.0);
        let inside = render(
            WeatherSound {
                indoors: 1.0,
                ..rain(1.0, 0.0)
            },
            3.0,
        );
        // The hiss: what changes from one frame to the next.
        let hiss = |v: &[f32]| {
            rms(&v
                .windows(3)
                .step_by(2)
                .map(|w| w[2] - w[0])
                .collect::<Vec<_>>())
        };
        assert!(rms(&inside) < rms(&out) * 0.8);
        assert!(
            hiss(&inside) < hiss(&out) * 0.3,
            "{} {}",
            hiss(&inside),
            hiss(&out)
        );
    }

    #[test]
    fn it_never_repeats_itself() {
        let out = render(rain(1.0, 0.0), 2.0);
        let (a, b) = out.split_at(out.len() / 2);
        assert!(a.iter().zip(b).filter(|(x, y)| x == y).count() < 16);
    }

    #[test]
    fn the_rain_glides_in_rather_than_switching_on() {
        let mut bed = WeatherBed::new(RATE);
        bed.set(rain(1.0, 0.0));
        let mut first = vec![0.0; 2 * 480];
        bed.add_to(&mut first, 2);
        let mut later = vec![0.0; 2 * RATE as usize];
        bed.add_to(&mut later, 2);
        assert!(
            rms(&first) < rms(&later) * 0.2,
            "{} {}",
            rms(&first),
            rms(&later)
        );
    }

    #[test]
    fn a_mono_device_takes_both_sides() {
        let mut bed = WeatherBed::new(RATE);
        bed.set(rain(1.0, 0.0));
        let mut out = vec![0.0; RATE as usize];
        bed.add_to(&mut out, 1);
        assert!(rms(&out) > 0.005);
    }
}
