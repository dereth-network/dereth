//! Values many record types share: waveforms, world positions, packed colors.

use auberean_dat::reader::Result;
use auberean_dat::{Reader, Writer};
use auberean_primitives::{CellId, Frame};

/// How far apart two stored floats may be and still count as the same value.
pub const SAME_FLOAT: f32 = 0.0002;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < SAME_FLOAT
}

/// Whether two frames are the same place and rotation, to within [`SAME_FLOAT`] a component.
// D2-05770
#[must_use]
pub fn same_frame(a: &Frame, b: &Frame) -> bool {
    near(a.origin.x, b.origin.x)
        && near(a.origin.y, b.origin.y)
        && near(a.origin.z, b.origin.z)
        && near(a.rotation.w, b.rotation.w)
        && near(a.rotation.x, b.rotation.x)
        && near(a.rotation.y, b.rotation.y)
        && near(a.rotation.z, b.rotation.z)
}

/// A value that varies over time: a shape (`kind`), and a base, amplitude, phase, frequency and
/// roughness, each with the rate it changes at.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
// D2-03485
pub struct Waveform {
    /// The shape: 1 none, 2 speed, 3 noise, 4 sine, 5 square, 6 bounce, 7 Perlin, 8 fractal,
    /// 9 frame loop.
    pub kind: u32,
    pub base: f32,
    pub base_rate: f32,
    pub amplitude: f32,
    pub amplitude_rate: f32,
    pub phase: f32,
    pub phase_rate: f32,
    pub frequency: f32,
    pub frequency_rate: f32,
    pub roughness: f32,
    pub roughness_rate: f32,
}

/// The shapes' names, by kind (1 to 9).
const WAVEFORM_NAMES: [&str; 9] = [
    "None",
    "Speed",
    "Noise",
    "Sine",
    "Square",
    "Bounce",
    "Perlin",
    "Fractal",
    "FrameLoop",
];

impl Waveform {
    /// A steady value: no shape, base `base`, amplitude 1, frequency 1, every rate 0.
    // D2-13340
    #[must_use]
    pub fn steady(base: f32) -> Self {
        Self {
            base,
            ..Self::defaults()
        }
    }

    /// A shape of its parts, no roughness and every rate 0.
    // D2-13341
    #[must_use]
    pub fn shaped(kind: u32, base: f32, amplitude: f32, phase: f32, frequency: f32) -> Self {
        Self {
            kind,
            base,
            amplitude,
            phase,
            frequency,
            ..Self::default()
        }
    }

    /// A new waveform: no shape, base 0, amplitude 1, frequency 1, every rate 0.
    // D2-13335, D2-13339
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            kind: 1,
            amplitude: 1.0,
            frequency: 1.0,
            ..Self::default()
        }
    }

    /// The shape's name, `Unknown` for a kind no shape has.
    // D2-13342
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        usize::try_from(self.kind)
            .ok()
            .and_then(|k| k.checked_sub(1))
            .and_then(|k| WAVEFORM_NAMES.get(k))
            .copied()
            .unwrap_or("Unknown")
    }

    /// The most the waveform ever reaches, when that is known: a steady one its base; a speed its
    /// base while its amplitude is not positive; the other shapes their base plus their amplitude
    /// while the amplitude does not grow. `None` when it is not known (and for a shape none of the
    /// nine, whose bound the original leaves unwritten).
    // D2-13336
    #[must_use]
    pub fn upper_bound(&self) -> Option<f32> {
        match self.kind {
            1 => Some(self.base),
            2 => (self.amplitude <= 0.0).then_some(self.base),
            3..=9 => (self.amplitude_rate <= 0.0).then_some(self.base + self.amplitude),
            _ => None,
        }
    }

    /// Whether two waveforms are the same: the same shape, and every number within
    /// [`SAME_FLOAT`].
    // D2-16177
    #[must_use]
    pub fn same(&self, o: &Self) -> bool {
        self.kind == o.kind
            && near(self.base, o.base)
            && near(self.base_rate, o.base_rate)
            && near(self.amplitude, o.amplitude)
            && near(self.amplitude_rate, o.amplitude_rate)
            && near(self.phase, o.phase)
            && near(self.phase_rate, o.phase_rate)
            && near(self.frequency, o.frequency)
            && near(self.frequency_rate, o.frequency_rate)
            && near(self.roughness, o.roughness)
            && near(self.roughness_rate, o.roughness_rate)
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.kind);
        for v in [
            self.base,
            self.base_rate,
            self.amplitude,
            self.amplitude_rate,
            self.phase,
            self.phase_rate,
            self.frequency,
            self.frequency_rate,
            self.roughness,
            self.roughness_rate,
        ] {
            w.f32(v);
        }
    }

    // D2-06942
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            kind: r.u32()?,
            base: r.f32()?,
            base_rate: r.f32()?,
            amplitude: r.f32()?,
            amplitude_rate: r.f32()?,
            phase: r.f32()?,
            phase_rate: r.f32()?,
            frequency: r.f32()?,
            frequency_rate: r.f32()?,
            roughness: r.f32()?,
            roughness_rate: r.f32()?,
        })
    }
}

/// A place in the world: a cell and a frame in it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Position {
    pub cell: CellId,
    pub frame: Frame,
}

impl Position {
    // D2-13186
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            cell: CellId(r.u32()?),
            frame: r.frame()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.cell.0);
        w.frame(self.frame);
    }
}

/// A color packed in 32 bits, one byte a channel, stored as one word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Color32(pub u32);

impl Color32 {
    // D2-13248, D2-10449, D2-12517
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.u32()?))
    }

    pub fn write(self, w: &mut Writer) {
        w.u32(self.0);
    }

    /// The color as four channels from 0 to 1, red, green, blue, alpha: alpha is the top byte,
    /// then red, green and blue.
    // D2-03481
    #[must_use]
    pub fn channels(self) -> [f32; 4] {
        let c = |shift: u32| ((self.0 >> shift) & 0xFF) as f32 * 0.003_921_569;
        [c(16), c(8), c(0), c(24)]
    }
}
