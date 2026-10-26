//! The plugins' marks as every interface draws them: the labels standing in the world (placed on
//! screen already), the blips on the radar, the points and lines on the map, and the arrow toward
//! a target. The host works out what each mark is; each interface puts the radar's and the map's
//! where its own radar and map are, by their own projection.

use crate::{arrow_glyphs, line_glyphs, Glyph};

/// A blip on the radar: where it is from the player, in metres to the right and ahead (the
/// radar's own heading-up frame), and its colour, `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarMark {
    pub right: f32,
    pub ahead: f32,
    pub color: u32,
}

/// Points on the map, or a line through them: each point's north-south and east-west
/// coordinates (north and east positive), the label shown by the first, and the colour.
#[derive(Debug, Clone, PartialEq)]
pub struct MapMark {
    pub points: Vec<(f64, f64)>,
    pub line: bool,
    pub label: String,
    pub color: u32,
}

/// The arrow toward a target: its bearing in degrees clockwise from the camera's straight ahead,
/// how far the target is in metres, what the arrow says, and its colour.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowMark {
    pub turn: f64,
    pub distance: f64,
    pub label: String,
    pub color: u32,
}

/// Every mark this frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// The labels standing in the world, on screen: decluttered, and only those the world does
    /// not hide.
    pub labels: Vec<Glyph>,
    pub radar: Vec<RadarMark>,
    /// The radar's reach where the player stands, in metres.
    pub radar_range: f32,
    pub map: Vec<MapMark>,
    pub arrow: Option<ArrowMark>,
}

impl Marks {
    /// Whether there is nothing to draw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
            && self.radar.is_empty()
            && self.map.is_empty()
            && self.arrow.is_none()
    }
}

/// The blips on a heading-up radar centred at `centre`, `radius` pixels to its rim at `range`
/// metres: each one inside the rim as a dot. `turn` (radians clockwise) turns them for a radar
/// that stands otherwise than heading up.
#[must_use]
pub fn radar_glyphs(
    marks: &[RadarMark],
    centre: (f32, f32),
    radius: f32,
    range: f32,
    turn: f32,
) -> Vec<Glyph> {
    if range <= 0.0 || radius <= 0.0 {
        return Vec::new();
    }
    let (s, c) = (
        dereth_primitives::num::math::sinf(turn),
        dereth_primitives::num::math::cosf(turn),
    );
    marks
        .iter()
        .filter(|m| m.right * m.right + m.ahead * m.ahead < range * range)
        .map(|m| {
            let (x, y) = (m.right * radius / range, -m.ahead * radius / range);
            let (x, y) = (x * c - y * s, y * c + x * s);
            Glyph::Dot {
                x: dereth_primitives::num::to_i32(centre.0 + x),
                y: dereth_primitives::num::to_i32(centre.1 + y),
                size: 3,
                color: m.color,
            }
        })
        .collect()
}

/// The points and lines on a map, each point where `place` puts its coordinates (`None` off the
/// map), with each mark's label over its first point.
#[must_use]
pub fn map_glyphs(marks: &[MapMark], place: &dyn Fn(f64, f64) -> Option<(i32, i32)>) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    for m in marks {
        let pts: Vec<(i32, i32)> = m
            .points
            .iter()
            .filter_map(|(ns, ew)| place(*ns, *ew))
            .collect();
        if m.line {
            for w in pts.windows(2) {
                glyphs.extend(line_glyphs(w[0], w[1], 2, m.color));
            }
        } else {
            glyphs.extend(pts.iter().map(|&(x, y)| Glyph::Dot {
                x,
                y,
                size: 4,
                color: m.color,
            }));
        }
        if let (Some(&(x, y)), false) = (pts.first(), m.label.is_empty()) {
            glyphs.push(Glyph::Text {
                x,
                y: y - 16,
                text: m.label.clone(),
                color: m.color,
            });
        }
    }
    glyphs
}

/// The arrow at `(cx, cy)`: its ring and spoke, and under it what it says and how far.
#[must_use]
pub fn arrow_mark_glyphs(arrow: &ArrowMark, cx: i32, cy: i32) -> Vec<Glyph> {
    let mut glyphs = arrow_glyphs(cx, cy, 18, arrow.turn, arrow.color);
    let d = arrow.distance;
    let far = if d >= 1000.0 {
        format!("{:.1} km", d / 1000.0)
    } else {
        format!("{d:.0} m")
    };
    glyphs.push(Glyph::Text {
        x: cx,
        y: cy + 24,
        text: if arrow.label.is_empty() {
            far
        } else {
            format!("{}  {far}", arrow.label)
        },
        color: arrow.color,
    });
    glyphs
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own drawing of the plugins' marks)
    use super::*;

    /// A blip ahead is drawn above the radar's middle, one behind the rim not at all, and a radar
    /// standing north up turns them by the player's heading.
    #[test]
    fn a_blip_ahead_stands_above_the_middle_and_one_past_the_rim_is_left_out() {
        let marks = [
            RadarMark {
                right: 0.0,
                ahead: 50.0,
                color: 0xFF_00_00,
            },
            RadarMark {
                right: 0.0,
                ahead: 150.0,
                color: 0xFF_00_00,
            },
        ];
        let g = radar_glyphs(&marks, (100.0, 100.0), 50.0, 100.0, 0.0);
        assert_eq!(g.len(), 1);
        assert!(matches!(g[0], Glyph::Dot { x: 100, y: 75, .. }), "{g:?}");
        // Turned a quarter clockwise, ahead points right.
        let g = radar_glyphs(
            &marks[..1],
            (100.0, 100.0),
            50.0,
            100.0,
            std::f32::consts::FRAC_PI_2,
        );
        assert!(matches!(g[0], Glyph::Dot { x: 125, y: 100, .. }), "{g:?}");
    }

    /// A map line is drawn through the places its points fall, and its label over the first.
    #[test]
    fn a_map_line_runs_through_its_points_and_is_named_at_the_first() {
        let marks = [MapMark {
            points: vec![(0.0, 0.0), (10.0, 10.0)],
            line: true,
            label: "Path".into(),
            color: 0x00_FF_00,
        }];
        let g = map_glyphs(&marks, &|ns, ew| {
            #[allow(clippy::cast_possible_truncation)]
            Some(((ew * 2.0) as i32, (-ns * 2.0) as i32))
        });
        assert!(g.iter().any(|g| matches!(g, Glyph::Dot { x: 0, y: 0, .. })));
        assert!(g
            .iter()
            .any(|g| matches!(g, Glyph::Dot { x: 20, y: -20, .. })));
        assert!(g
            .iter()
            .any(|g| matches!(g, Glyph::Text { text, .. } if text == "Path")));
    }
}
