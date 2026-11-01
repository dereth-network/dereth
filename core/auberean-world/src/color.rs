//! Colors as the world computes them: four floats, read from packed words, converted to and from
//! hue, saturation and brightness, and interpolated the short way round the hue circle.

/// A color with red, green, blue and alpha in `[0, 1]` (wider while being computed).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// A packed word, alpha in the top byte, then red, green and blue, each byte over 255.
    #[must_use]
    pub fn from_argb(word: u32) -> Self {
        let c = |shift: u32| f32::from(((word >> shift) & 0xFF) as u8) * 0.003_921_569;
        Self::new(c(16), c(8), c(0), c(24))
    }

    /// The color packed back into an alpha-red-green-blue word, each channel truncated from
    /// `x * 255`.
    #[must_use]
    pub fn to_argb(self) -> u32 {
        let c = |v: f32| -> u32 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=255
            let b = (v.clamp(0.0, 1.0) * 255.0) as u32;
            b
        };
        (c(self.a) << 24) | (c(self.r) << 16) | (c(self.g) << 8) | c(self.b)
    }

    /// Each channel clamped to `[0, 1]`.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self::new(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
            self.a.clamp(0.0, 1.0),
        )
    }

    /// Hue in sextants `[0, 6)`, saturation `1 - min/max`, brightness `max`; a gray has hue and
    /// saturation 0.
    // D2-13246
    #[must_use]
    pub fn to_hsb(self) -> (f32, f32, f32) {
        let (r, g, b) = (self.r, self.g, self.b);
        if r == b && r == g {
            return (0.0, 0.0, r);
        }
        if g <= r {
            if r < b {
                return (4.0 + (r - g) / (b - g), 1.0 - g / b, b);
            }
            if b <= g {
                return ((g - b) / (r - b), 1.0 - b / r, r);
            }
            return (6.0 - (b - g) / (r - g), 1.0 - g / r, r);
        }
        if g < b {
            return (4.0 - (g - r) / (b - r), 1.0 - r / b, b);
        }
        if b <= r {
            return (2.0 - (r - b) / (g - b), 1.0 - b / g, g);
        }
        ((b - r) / (g - r) + 2.0, 1.0 - r / g, g)
    }

    /// The color of hue `h` (sextants), saturation `s` and brightness `v`, alpha 1, each channel
    /// clamped to `[0, 1]`. A saturation or brightness that is not positive gives the gray `v`.
    /// A hue above 6 becomes `6 * (h mod 6)` and a negative one `6 * (1 - (-h mod 6))`, exactly as
    /// the world's own conversion does.
    // D2-13247
    #[must_use]
    pub fn from_hsb(h: f32, s: f32, v: f32) -> Self {
        if !(s > 0.0 && v > 0.0) {
            return Self::new(v, v, v, 1.0).clamped();
        }
        let m = (1.0 - s) * v;
        let mut h = h;
        if h > 6.0 {
            h = (h % 6.0) * 6.0;
        }
        if h < 0.0 {
            h = (1.0 - (-h % 6.0)) * 6.0;
        }
        let span = v - m;
        let (r, g, b) = if h < 1.0 {
            (v, span * h + m, m)
        } else if h < 2.0 {
            ((2.0 - h) * span + m, v, m)
        } else if h < 3.0 {
            (m, v, (h - 2.0) * span + m)
        } else if h < 4.0 {
            (m, (4.0 - h) * span + m, v)
        } else if h < 5.0 {
            ((h - 4.0) * span + m, m, v)
        } else {
            (v, m, (6.0 - h) * span + m)
        };
        Self::new(r, g, b, 1.0).clamped()
    }
}

/// `a + (b - a) * r` on an angle with period `p`, going the short way round.
// D2-07572
#[must_use]
pub fn lerp_angle_closest(a: f32, b: f32, r: f32, p: f32) -> f32 {
    if a > b && (b - a) + p <= a - b {
        let v = a + ((b + p) - a) * r;
        if v >= p {
            v - p
        } else {
            v
        }
    } else if a <= b && (a - b) + p <= b - a {
        let v = a + ((b - p) - a) * r;
        if v < 0.0 {
            v + p
        } else {
            v
        }
    } else {
        a + (b - a) * r
    }
}

/// Two colors mixed at `r` through hue, saturation and brightness: saturation and brightness
/// linearly, hue the short way round, alpha 1.
// D2-07573
#[must_use]
pub fn lerp_color(a: Color, b: Color, r: f32) -> Color {
    let (ha, sa, va) = a.to_hsb();
    let (hb, sb, vb) = b.to_hsb();
    let h = lerp_angle_closest(ha, hb, r, 6.0);
    Color::from_hsb(h, sa + (sb - sa) * r, va + (vb - va) * r)
}
