//! Pieces a HUD shares across games: the link line at the top of the screen (the world, the
//! player's own clock, the round trip and a lamp for how lately the server was heard), the
//! experience band (the level, the experience through it, and what the player has to spend), the
//! rings drawn on a radar, the log window's look (its translucent ground, its entry line set apart
//! from the lines by a thin rule, and the corner it is sized by), and the menu bar's plate with its
//! buttons close together along it.

use crate::art::Family;
use crate::draw::{with_alpha, Argb, Rect};
use crate::input::InputFrame;
use crate::paint::{Painter, TextStyle};
use crate::widgets::{self, Ctx};

/// The link lamp's colors: the server heard lately, a while ago, or not at all.
pub const LINK_GOOD: Argb = 0xFF6E_CB5C;
pub const LINK_SLOW: Argb = 0xFFE8_B84A;
pub const LINK_LOST: Argb = 0xFFE0_4040;

/// How many seconds of quiet turn the lamp amber, and then red.
pub const LINK_SLOW_SECONDS: f64 = 2.0;
pub const LINK_LOST_SECONDS: f64 = 5.0;

/// The link lamp's color for `quiet` seconds since the server was last heard: green under two
/// seconds, amber under five, red after that or with no link at all.
#[must_use]
pub fn link_lamp(quiet: Option<f64>) -> Argb {
    match quiet {
        Some(s) if s < LINK_SLOW_SECONDS => LINK_GOOD,
        Some(s) if s < LINK_LOST_SECONDS => LINK_SLOW,
        _ => LINK_LOST,
    }
}

/// What the link line shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Link {
    /// The world's name, when it has one.
    pub world: Option<String>,
    /// The player's own clock, as written.
    pub local_time: Option<String>,
    /// The last round trip, in milliseconds.
    pub round_trip_ms: Option<f64>,
    /// The share of packets lost, 0 to 1.
    pub packet_loss: f32,
    /// Seconds since the server was last heard; none with no link.
    pub quiet: Option<f64>,
    /// The lamp and its tooltip are shown (in the world).
    pub lamp: bool,
}

impl Link {
    /// The line's words: the world, the clock and the round trip, apart.
    #[must_use]
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if let Some(w) = &self.world {
            parts.push(w.clone());
        }
        if let Some(t) = &self.local_time {
            parts.push(t.clone());
        }
        if let Some(ms) = self.round_trip_ms {
            parts.push(format!("{ms:.0} ms"));
        }
        parts.join("  |  ")
    }

    /// The lamp's tooltip lines: the round trip, the loss and how long ago the server was heard.
    #[must_use]
    pub fn tip_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(ms) = self.round_trip_ms {
            lines.push(format!("Round trip: {ms:.0} ms"));
        }
        lines.push(format!("Packet loss: {:.1}%", self.packet_loss * 100.0));
        if let Some(s) = self.quiet {
            lines.push(format!("Last heard: {s:.1} s ago"));
        }
        lines
    }
}

/// Where the link lamp is for a line whose right edge stands at `right` and whose text starts
/// `top` down, on a painter at `k` pixels a layout unit, given the text's middle `mid` below its
/// top.
fn link_lamp_rect(right: f32, top: f32, mid: f32, k: f32) -> Rect {
    let d = 9.0 * k;
    Rect::new(right - 24.0 * k, top + mid - d / 2.0, d, d)
}

/// The link line, its text ending a little left of `right` with the lamp after it, `top` down:
/// the world, the clock and the round trip; then, while `link.lamp`, the lamp, centered on the
/// line's letters in a dark ring. The tooltip's lines while the pointer is over it.
pub fn link_line(
    p: &mut Painter<'_>,
    input: &InputFrame,
    right: f32,
    top: f32,
    link: &Link,
) -> Option<Vec<String>> {
    let k = p.scale;
    let style = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
    let text = link.text();
    let w = p.measure(&style, &text);
    p.text(&style, right - w - 30.0 * k, top, &text);
    if !link.lamp {
        return None;
    }
    let mid = p
        .letter_band(&style)
        .map_or(p.line_height(&style) / 2.0, |(t, base)| (t + base) / 2.0);
    let l = link_lamp_rect(right, top, mid, k);
    p.fill(l, 0xFF14_100C);
    p.fill(l.inset(1.5 * k), link_lamp(link.quiet));
    let hover = Rect::new(right - w - 30.0 * k, top + 3.0 * k, w + 30.0 * k, 20.0 * k);
    input.hover(&hover).then(|| link.tip_lines())
}

/// A number with its thousands marked.
#[must_use]
pub fn grouped(n: impl Into<u64>) -> String {
    let digits = n.into().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The experience band's fill and the color of what the player has to spend.
pub const EXPERIENCE: Argb = 0xFFE8_B84A;
pub const TO_SPEND: Argb = 0xFF9A_D0FF;

/// What the experience band shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Experience {
    /// The level; none while it is not known.
    pub level: Option<u32>,
    /// The experience past the level's start, and what the next level takes from there.
    pub into: Option<u64>,
    pub span: Option<u64>,
    /// The experience earned in all, written where the level's span is not known.
    pub total: u64,
    /// What the player has to spend, and its name ("Unassigned"); none at nothing.
    pub to_spend: Option<(&'static str, u64)>,
}

impl Experience {
    /// How far through its level the player is, 0 to 1 (0 while the span is not known).
    #[must_use]
    pub fn fill(&self) -> f32 {
        match (self.into, self.span) {
            (Some(into), Some(span)) if span > 0 => {
                #[allow(clippy::cast_precision_loss)]
                let f = into as f32 / span as f32;
                f
            }
            _ => 0.0,
        }
    }

    /// The level's words: `Lv21`; none while the level is not known.
    #[must_use]
    pub fn level_text(&self) -> Option<String> {
        self.level.map(|l| format!("Lv{l}"))
    }

    /// The experience's words: through the level of its span, else the total.
    #[must_use]
    pub fn experience_text(&self) -> String {
        match self.span {
            Some(span) if span > 0 => {
                format!("EXP {}/{}", grouped(self.into.unwrap_or(0)), grouped(span))
            }
            _ => format!("EXP {}", grouped(self.total)),
        }
    }

    /// What there is to spend, in words, when there is any.
    #[must_use]
    pub fn to_spend_text(&self) -> Option<String> {
        self.to_spend
            .filter(|(_, n)| *n > 0)
            .map(|(name, n)| format!("{name} {}", grouped(n)))
    }
}

/// Where the experience band's gauge is on a screen of `screen` pixels at `k` pixels a layout
/// unit: centered along the top, 620 units wide at most and `clear` units clear of each side,
/// 200 at least.
#[must_use]
pub fn experience_band_rect(screen: (f32, f32), k: f32, clear: f32) -> Rect {
    let w = (620.0 * k).min(screen.0 - 2.0 * clear * k).max(200.0 * k);
    Rect::new(screen.0 / 2.0 - w / 2.0, 20.0 * k, w, 8.0 * k)
}

/// The experience band in `bar`: the gauge filled as far as the player is through its level, over
/// it the level and the experience on the left and what there is to spend on the right. Whether
/// the pointer is over it.
pub fn experience_band(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, bar: Rect, e: &Experience) -> bool {
    let k = p.scale;
    let (x, y, w) = (bar.x, bar.y, bar.w);
    let fill = e.fill();
    // The gauge: the track, the fill inside its rounded ends, and the frame.
    if !widgets::own_gauge(p, bar, fill, EXPERIENCE) {
        p.fill(Rect::new(x, y, w * fill.clamp(0.0, 1.0), bar.h), EXPERIENCE);
    }
    let label = TextStyle::new(Family::Numerals, 12.0, 0xFFEE_E1C5).edge(0xFF00_0000);
    let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
    let mut ex = x;
    if let Some(lv) = e.level_text() {
        p.text(&label, x, y - 18.0 * k, &lv);
        ex += p.measure(&label, &lv) + 16.0 * k;
    }
    p.text(&body, ex, y - 18.0 * k, &e.experience_text());
    if let Some(spend) = e.to_spend_text() {
        let uw = p.measure(&body, &spend);
        p.text(&body.colour(TO_SPEND), x + w - uw, y - 18.0 * k, &spend);
    }
    ctx.over(&Rect::new(x, y - 20.0 * k, w, 30.0 * k))
}

/// A ring round `centre`, `radius` pixels to its outer edge and `width` thick, in `colour`: drawn
/// a pixel row at a time, the row's one or two runs.
pub fn ring(p: &mut Painter<'_>, centre: (f32, f32), radius: f32, width: f32, colour: Argb) {
    if radius <= 0.0 || width <= 0.0 {
        return;
    }
    let inner = (radius - width).max(0.0);
    let top = (centre.1 - radius).floor();
    let mut y = top;
    while y < centre.1 + radius {
        let dy = y + 0.5 - centre.1;
        if dy.abs() < radius {
            let outer = (radius * radius - dy * dy).sqrt();
            let hole = if dy.abs() < inner {
                (inner * inner - dy * dy).sqrt()
            } else {
                0.0
            };
            if hole <= 0.0 {
                p.fill(Rect::new(centre.0 - outer, y, 2.0 * outer, 1.0), colour);
            } else {
                let run = outer - hole;
                p.fill(Rect::new(centre.0 - outer, y, run, 1.0), colour);
                p.fill(Rect::new(centre.0 + hole, y, run, 1.0), colour);
            }
        }
        y += 1.0;
    }
}

/// A log window's ground before its opacity is taken into account, and the opacity it has until
/// the player sets one.
pub const LOG_GROUND: Argb = 0xFF10_1010;
pub const LOG_OPACITY: f32 = 144.0 / 255.0;

/// A log window's ground at `opacity` (0 to 1).
#[must_use]
pub fn log_ground(opacity: f32) -> Argb {
    with_alpha(LOG_GROUND, opacity)
}

/// The entry line's ground, and the thin rule along its top that sets it apart from the lines.
pub const ENTRY_GROUND: Argb = 0xE020_1C14;
pub const ENTRY_RULE: Argb = 0xFFC8_B27A;

/// The entry line's ground over `r` and the rule along its top, a layout unit thick.
pub fn entry_line(p: &mut Painter<'_>, r: Rect) {
    p.fill(r, ENTRY_GROUND);
    p.fill(Rect::new(r.x, r.y, r.w, 1.0 * p.scale), ENTRY_RULE);
}

/// The side of the corner a log window is sized by, in layout units.
pub const GRIP: f32 = 12.0;

/// The sizing corner's mark, and the mark lit under the pointer or while it is held.
pub const GRIP_MARK: Argb = 0xA0C8_B27A;
pub const GRIP_LIT: Argb = 0xFFE8_D29A;

/// Where the corner a log window showing as `shown` is sized by is, at `k` pixels a layout unit:
/// its bottom right.
#[must_use]
pub fn grip_rect(shown: Rect, k: f32) -> Rect {
    let g = GRIP * k;
    Rect::new(shown.right() - g, shown.bottom() - g, g, g)
}

/// The mark of the sizing corner `grip`: a fine angle along its foot and its right, lit while
/// `lit`.
pub fn grip_mark(p: &mut Painter<'_>, grip: Rect, lit: bool) {
    let k = p.scale;
    let colour = if lit { GRIP_LIT } else { GRIP_MARK };
    let t = 2.0 * k;
    p.fill(
        Rect::new(grip.x + 3.0 * k, grip.bottom() - t - k, grip.w - 4.0 * k, t),
        colour,
    );
    p.fill(
        Rect::new(grip.right() - t - k, grip.y + 3.0 * k, t, grip.h - 4.0 * k),
        colour,
    );
}

/// A menu bar button's side, the gap between buttons, the plate's room round them (across, then
/// up and down) and its usual margin from the screen's right edge and foot, in layout units.
pub const MENU_BUTTON: f32 = 34.0;
pub const MENU_GAP: f32 = 2.0;
pub const MENU_PAD: (f32, f32) = (6.0, 7.0);
pub const MENU_MARGIN: f32 = 14.0;

/// Where the menu bar's plate is on `screen`, at `k` pixels a layout unit, holding `count`
/// buttons: at the bottom right, `margin` layout units from the screen's right edge and from its
/// foot, on whole pixels.
#[must_use]
pub fn menu_plate(screen: (f32, f32), k: f32, count: usize, margin: f32) -> Rect {
    let size = (MENU_BUTTON * k).round();
    let gap = (MENU_GAP * k).round().max(1.0);
    let (pad_x, pad_y) = ((MENU_PAD.0 * k).round(), (MENU_PAD.1 * k).round());
    let margin = (margin * k).round();
    #[allow(clippy::cast_precision_loss)]
    let n = count as f32;
    let w = n * size + (n - 1.0).max(0.0) * gap + 2.0 * pad_x;
    let h = size + 2.0 * pad_y;
    Rect::new(screen.0 - margin - w, screen.1 - margin - h, w, h)
}

/// Where the `n`th button (from the left) on the menu bar's `plate` is, at `k` pixels a layout
/// unit.
#[must_use]
pub fn menu_button(plate: Rect, k: f32, n: usize) -> Rect {
    let size = (MENU_BUTTON * k).round();
    let gap = (MENU_GAP * k).round().max(1.0);
    #[allow(clippy::cast_precision_loss)]
    let x = plate.x + (MENU_PAD.0 * k).round() + n as f32 * (size + gap);
    Rect::new(x, plate.y + (MENU_PAD.1 * k).round(), size, size)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own interface, not a behaviour of the retail client)
    use super::*;
    use crate::art::Art;
    use crate::draw::DrawList;

    #[test]
    fn the_link_lamp_is_green_then_amber_after_two_seconds_and_red_after_five() {
        assert_eq!(link_lamp(Some(0.0)), LINK_GOOD);
        assert_eq!(link_lamp(Some(1.99)), LINK_GOOD);
        assert_eq!(link_lamp(Some(2.0)), LINK_SLOW);
        assert_eq!(link_lamp(Some(4.99)), LINK_SLOW);
        assert_eq!(link_lamp(Some(5.0)), LINK_LOST);
        assert_eq!(link_lamp(None), LINK_LOST);
    }

    #[test]
    fn the_link_line_reads_the_world_the_clock_and_the_round_trip_apart() {
        let mut l = Link {
            world: Some("Coldeve".into()),
            local_time: Some("9:57 PM".into()),
            round_trip_ms: Some(104.6),
            ..Link::default()
        };
        assert_eq!(l.text(), "Coldeve  |  9:57 PM  |  105 ms");
        l.round_trip_ms = None;
        assert_eq!(l.text(), "Coldeve  |  9:57 PM");
        l.quiet = Some(1.3);
        l.packet_loss = 0.031;
        assert_eq!(
            l.tip_lines(),
            ["Packet loss: 3.1%", "Last heard: 1.3 s ago"]
        );
    }

    #[test]
    fn the_experience_band_reads_through_the_level_else_the_total_and_what_is_to_spend() {
        let mut e = Experience {
            level: Some(21),
            into: Some(218_435),
            span: Some(274_170),
            total: 9_000_000,
            to_spend: Some(("Unassigned", 138_195)),
        };
        assert_eq!(e.level_text().as_deref(), Some("Lv21"));
        assert_eq!(e.experience_text(), "EXP 218,435/274,170");
        assert_eq!(e.to_spend_text().as_deref(), Some("Unassigned 138,195"));
        assert!((e.fill() - 218_435.0 / 274_170.0).abs() < 1e-6);
        e.span = None;
        e.to_spend = Some(("Unassigned", 0));
        assert_eq!(e.experience_text(), "EXP 9,000,000");
        assert_eq!(e.to_spend_text(), None);
        assert!(e.fill().abs() < f32::EPSILON);
    }

    /// The farthest a row of a ring reaches from its center, and the nearest a row's run comes
    /// to it along the center's row.
    #[test]
    fn a_ring_reaches_its_radius_and_leaves_its_middle_empty() {
        let art = Art::empty();
        let mut list = DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (200.0, 200.0),
            fade: 1.0,
        };
        ring(&mut p, (100.0, 100.0), 40.0, 2.0, 0xFF00_0000);
        let quads = &p.list.quads;
        assert!(!quads.is_empty());
        let left = quads.iter().map(|q| q.dst.x).fold(f32::MAX, f32::min);
        let right = quads.iter().map(|q| q.dst.right()).fold(f32::MIN, f32::max);
        assert!((100.0 - left - 40.0).abs() < 0.5, "{left}");
        assert!((right - 100.0 - 40.0).abs() < 0.5, "{right}");
        let middle: Vec<_> = quads
            .iter()
            .filter(|q| q.dst.y <= 100.0 && q.dst.bottom() > 100.0)
            .collect();
        assert_eq!(middle.len(), 2, "two runs across the middle row");
        for q in middle {
            assert!(q.dst.w <= 2.5, "a run as thick as the ring: {:?}", q.dst);
        }
    }

    #[test]
    fn the_menu_plate_holds_every_button_with_room_round_them() {
        for k in [1.0, 1.5, 2.0] {
            let plate = menu_plate((1920.0, 1080.0), k, 9, MENU_MARGIN);
            let first = menu_button(plate, k, 0);
            let last = menu_button(plate, k, 8);
            assert!(
                first.x > plate.x && first.y > plate.y,
                "{first:?} in {plate:?}"
            );
            assert!(
                (plate.right() - last.right() - (first.x - plate.x)).abs() < 0.5,
                "as much room at the right as at the left"
            );
            assert!(
                (plate.bottom() - first.bottom() - (first.y - plate.y)).abs() < 0.5,
                "as much room below as above"
            );
            assert!(
                (1920.0 - plate.right() - (1080.0 - plate.bottom())).abs() < 0.5,
                "as far from the right edge as from the foot"
            );
        }
    }

    #[test]
    fn the_sizing_corner_sits_in_the_window_s_bottom_right() {
        let shown = Rect::new(20.0, 600.0, 400.0, 200.0);
        let g = grip_rect(shown, 2.0);
        assert_eq!(g, Rect::new(396.0, 776.0, 24.0, 24.0));
    }
}
