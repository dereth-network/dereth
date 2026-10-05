//! `Radar` — the palette, the blip shapes, the projection, the compass and the coordinates.
//!
//! The radar is a **CPU-rasterised** widget: every blip is at most 3×3 pixels, drawn with 1×1
//! area fills, no texture and no anti-aliasing. A GPU rebuild must still land pixels on the same
//! integer coordinates or blips will shimmer, because the projection truncates toward zero before
//! drawing.

use dereth_primitives::num::math;
use dereth_primitives::num::to_i32;

use crate::view::RadarEntry;
use dereth_client_contract::{options::interface::Interface, radar as shared};

// -------------------------------------------------------------------------------------------
// The palette
// -------------------------------------------------------------------------------------------

/// One RGBA colour constant built into the client. Alpha is 1.0 for every entry in the radar
/// palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarColor {
    pub rgb: (f32, f32, f32),
    /// The 8-bit form, `0xRRGGBB`.
    pub hex: u32,
}

const fn rc(rgb: (f32, f32, f32), hex: u32) -> RadarColor {
    RadarColor { rgb, hex }
}

/// The rgba color radar blue.
pub const BLUE: RadarColor = rc((0.25, 0.66, 1.00), 0x40A8FF);
/// The rgba color radar gold.
pub const GOLD: RadarColor = rc((1.00, 0.67, 0.00), 0xFFAB00);
/// The rgba color radar yellow.
pub const YELLOW: RadarColor = rc((1.00, 1.00, 0.50), 0xFFFF80);
/// The rgba color radar white.
pub const WHITE: RadarColor = rc((1.00, 1.00, 1.00), 0xFFFFFF);
/// The rgba color radar red.
pub const RED: RadarColor = rc((1.00, 0.25, 0.39), 0xFF4063);
/// The rgba color radar purple.
pub const PURPLE: RadarColor = rc((0.75, 0.39, 1.00), 0xBF63FF);
/// The rgba color radar pink.
pub const PINK: RadarColor = rc((1.00, 0.66, 0.75), 0xFFA8BF);
/// The rgba color radar green.
pub const GREEN: RadarColor = rc((0.00, 0.50, 0.25), 0x008040);
/// The rgba color radar cyan.
pub const CYAN: RadarColor = rc((0.00, 1.00, 1.00), 0x00FFFF);
/// The rgba color radar bright green.
pub const BRIGHT_GREEN: RadarColor = rc((0.00, 1.00, 0.00), 0x00FF00);

/// The ten base colours.
pub const BASE_PALETTE: [RadarColor; 10] = [
    BLUE,
    GOLD,
    YELLOW,
    WHITE,
    RED,
    PURPLE,
    PINK,
    GREEN,
    CYAN,
    BRIGHT_GREEN,
];

/// The thirteen semantic aliases, aliased at static-init time by the client.
///
/// Several aliases collide deliberately (Admin = Sentinel = Cyan, NPC = Vendor = Yellow,
/// Fellowship = FellowshipLeader = BrightGreen). Do not "fix" them.
pub mod semantic {
    use super::{RadarColor, BLUE, BRIGHT_GREEN, CYAN, GOLD, PINK, PURPLE, RED, WHITE, YELLOW};
    /// = White.
    pub const DEFAULT: RadarColor = WHITE;
    /// = Cyan.
    pub const ADMIN: RadarColor = CYAN;
    /// = Pink.
    pub const ADVOCATE: RadarColor = PINK;
    /// = Gold.
    pub const CREATURE: RadarColor = GOLD;
    /// = Blue.
    pub const LIFE_STONE: RadarColor = BLUE;
    /// = Yellow.
    pub const NPC: RadarColor = YELLOW;
    /// = Red.
    pub const PLAYER_KILLER: RadarColor = RED;
    /// = Purple.
    pub const PORTAL: RadarColor = PURPLE;
    /// = Cyan, the same colour as [`ADMIN`].
    pub const SENTINEL: RadarColor = CYAN;
    /// = Yellow, the same colour as [`NPC`].
    pub const VENDOR: RadarColor = YELLOW;
    /// = BrightGreen.
    pub const FELLOWSHIP: RadarColor = BRIGHT_GREEN;
    /// = BrightGreen, the same colour as [`FELLOWSHIP`].
    pub const FELLOWSHIP_LEADER: RadarColor = BRIGHT_GREEN;
    /// = Pink, the same colour as [`ADVOCATE`].
    pub const PK_LITE: RadarColor = PINK;
}

/// The object bitfield bits the radar reads.
///
/// Defined in [`dereth_client_contract::radar::bitfield`], because `dereth_client_runtime::hud` reads `PK`,
/// `PK_LITE` and `PLAYER` from here and both sides must name the same mask.
pub use dereth_client_contract::radar::bitfield;

// -------------------------------------------------------------------------------------------
// The filter
// -------------------------------------------------------------------------------------------

/// `RadarEnum`, the value an object's radar field carries.
///
/// Defined in [`dereth_client_contract::radar::radar_enum`], beside [`bitfield`], for the same
/// reason: `dereth_client_runtime::hud` reads `UNDEF`.
pub use dereth_client_contract::radar::radar_enum;

/// **The radar's filter**, and the reason a
/// retail radar plots a handful of blips in a scene holding hundreds of objects.
///
/// The client's test is two conditions and nothing else: the object has a physics object, and
/// its radar field is `ShowMovement`, `ShowAlways` or `ShowAttacking`.
///
/// The client calls it before it will build a [`RadarEntry`]'s radar-info record
/// at all, so a rejected object never enters the radar's info list and can never be drawn, hovered or
/// tooltipped.
///
/// **Measured.** In the recorded training-dungeon session at its most populated
/// instant, 285 objects carry a `PublicWeenieDesc`; 179 of them were sent with no
/// `RADAR_ENUM` field and 5 more with `Undef`, so **184 of 285 are filtered here** before the range
/// cull runs at all. The 101 survivors are the NPCs, vendors, creatures, portals, life stones and
/// bind stones — exactly the categories the retail radar shows.
///
/// "Has a physics object" maps onto [`RadarEntry::in_world`], which is the same "the server has
/// placed this object" fact our object stream carries; the draw loop re-tests the cell for the same
/// reason.
#[must_use]
pub fn inq_showable_on_radar(o: &RadarEntry) -> bool {
    shared::showable(o)
}

// -------------------------------------------------------------------------------------------
// The blip colour
// -------------------------------------------------------------------------------------------

/// The radar panel's blip colour, in the client's exact decision order.
#[must_use]
pub fn get_blip_color(o: Option<&RadarEntry>) -> RadarColor {
    match shared::color_role(o, Interface::Modern).index() {
        1 => BLUE,
        2 => GOLD,
        3 => WHITE,
        4 => PURPLE,
        5 => RED,
        6 => PINK,
        7 => GREEN,
        8 => YELLOW,
        9 => CYAN,
        10 => BRIGHT_GREEN,
        _ => semantic::DEFAULT,
    }
}

// -------------------------------------------------------------------------------------------
// Blip shape
// -------------------------------------------------------------------------------------------

/// The radar blip shape — two names per value, geometric and semantic; the *semantic* names are
/// what the code uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlipShape {
    /// 0 `Undef` / `Undef` — drawn as nothing.
    Undef = 0,
    /// 1 `Circle` / `Circle` — the draw point.
    Circle = 1,
    /// 2 `Box` / `AllegianceMember` — the draw hollow.
    AllegianceMember = 2,
    /// 3 `X` / `Threat` — the draw x.
    Threat = 3,
    /// 4 `Plus` / `Default` — the draw cross.
    Default = 4,
    /// 5 `Triangle` / `FellowshipLeader` — the draw triangle.
    FellowshipLeader = 5,
    /// 6 `InvertedTriangle` / `Fellowship` — the draw inverted triangle.
    Fellowship = 6,
    /// 7 `XBox` / `ThreatAllegiance` — the draw x box.
    ThreatAllegiance = 7,
}

impl BlipShape {
    /// The *geometric* name, for cross-referencing the enum page.
    #[must_use]
    pub const fn geometric(self) -> &'static str {
        match self {
            Self::Undef => "Undef",
            Self::Circle => "Circle",
            Self::AllegianceMember => "Box",
            Self::Threat => "X",
            Self::Default => "Plus",
            Self::FellowshipLeader => "Triangle",
            Self::Fellowship => "InvertedTriangle",
            Self::ThreatAllegiance => "XBox",
        }
    }

    /// The pixels one blip covers, relative to its centre.
    ///
    /// Every one is a 1×1 area fill; the list is in the order the drawing function
    /// issues them, which is the order a pixel-exact comparison needs.
    #[must_use]
    pub fn pixels(self) -> Vec<(i32, i32)> {
        const EDGES: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        const CORNERS: [(i32, i32); 4] = [(1, 1), (-1, -1), (-1, 1), (1, -1)];
        match self {
            Self::Undef => Vec::new(),
            Self::Circle => vec![(0, 0)],
            Self::Default => std::iter::once((0, 0)).chain(EDGES).collect(),
            Self::Threat => std::iter::once((0, 0)).chain(CORNERS).collect(),
            Self::AllegianceMember => EDGES.into_iter().chain(CORNERS).collect(),
            Self::ThreatAllegiance => EDGES
                .into_iter()
                .chain(CORNERS)
                .chain(std::iter::once((0, 0)))
                .collect(),
            Self::FellowshipLeader => vec![(0, 0), (-1, 1), (0, 1), (1, 1)],
            Self::Fellowship => vec![(0, 0), (-1, -1), (0, -1), (1, -1)],
        }
    }
}

/// The selection brackets — "four 5-pixel brackets at ±3: the horizontal runs (x−2 … x+2, y±3) and the
/// vertical runs (x±3, y−2 … y+2)". A 7×7 open square around the blip.
#[must_use]
pub fn selected_pixels() -> Vec<(i32, i32)> {
    let mut v = Vec::with_capacity(20);
    for y in [-3, 3] {
        for x in -2..=2 {
            v.push((x, y));
        }
    }
    for x in [-3, 3] {
        for y in -2..=2 {
            v.push((x, y));
        }
    }
    v
}

/// The radar panel's blip shape, in the client's exact decision order.
///
/// `player` is the player's own entry, or `None` before `PlayerDescReceived`.
#[must_use]
pub fn get_blip_shape(o: Option<&RadarEntry>, player: Option<&RadarEntry>) -> BlipShape {
    let viewer = player.map(|p| shared::Viewer {
        pk: p.is_pk,
        pk_lite: p.is_pk_lite,
    });
    match shared::shape_role(o, viewer, Interface::Modern) {
        shared::ShapeRole::Hidden => BlipShape::Undef,
        shared::ShapeRole::Ordinary => BlipShape::Default,
        shared::ShapeRole::Allegiance => BlipShape::AllegianceMember,
        shared::ShapeRole::Threat => BlipShape::Threat,
        shared::ShapeRole::FellowshipLeader => BlipShape::FellowshipLeader,
        shared::ShapeRole::Fellowship => BlipShape::Fellowship,
    }
}

// -------------------------------------------------------------------------------------------
// Projection, culling and range
// -------------------------------------------------------------------------------------------

/// The radar radius: 75 units outdoors, 25 indoors. The pixel radius does
/// not change, only the scale.
///
/// Defined in [`dereth_client_contract::radar`]: it is the radius the *selection* and
/// *speech* sweeps in `dereth_client_runtime::{hud, interaction}` use, at seven call sites, and it is one
/// branch over a `bool`.
pub use dereth_client_contract::radar::radar_range;

/// The refresh gate: the next update is due at `now + 0.025`, i.e. the radar panel's per-frame
/// step runs at most every 25 ms.
pub const UPDATE_INTERVAL_SECONDS: f32 = 0.025;

/// See [`DIM_HEIGHT`].
pub use shared::DIM_FACTOR;
/// The vertical threshold above and below which a blip is dimmed, and the factor it is dimmed by.
///
/// "Objects more than **5.0 units** above or below the player are drawn at **65 %** brightness."
pub use shared::DIM_HEIGHT;

/// The mouse-over test radius, squared: `dist² < 0x25`, i.e. strictly inside 6 pixels.
pub const HOVER_DIST_SQ: i32 = 0x25;

/// One blip, ready to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blip {
    pub index: usize,
    /// Absolute pixel position inside the radar element.
    pub x: i32,
    pub y: i32,
    pub color: RadarColor,
    pub shape: BlipShape,
    /// 1.0, or [`DIM_FACTOR`] when the object is more than [`DIM_HEIGHT`] units above or below.
    pub dim: f32,
    pub selected: bool,
}

impl Blip {
    ///  multiplies **r, g, b** by `dim` (alpha untouched) before dispatching
    /// on the shape.
    #[must_use]
    pub fn dimmed_rgb(&self) -> (f32, f32, f32) {
        (
            self.color.rgb.0 * self.dim,
            self.color.rgb.1 * self.dim,
            self.color.rgb.2 * self.dim,
        )
    }
}

/// The radar geometry read from the layout during panel initialization.
pub use shared::Geometry as RadarGeometry;

/// The whole loop, minus the surface writes.
///
/// The five behaviours that must be reproduced exactly are all here: the `(range − 1)²` cull, the
/// truncation toward zero on both axes, `+Y` up on screen (the Y term is *subtracted*), the square
/// bounding-box reject after the round cull, and the 5.0/0.65 dimming.
///
/// `radar_blank` is the radar-blank flag — the `/radar off` state, which blanks the
/// blips without hiding the widget.
#[must_use]
pub fn draw_objects(
    objects: &[RadarEntry],
    player: Option<&RadarEntry>,
    geom: RadarGeometry,
    range: f32,
    selected: Option<dereth_primitives::ObjectId>,
    radar_blank: bool,
) -> Vec<Blip> {
    let mut out = Vec::new();
    if radar_blank {
        return out;
    }
    for (i, o) in objects.iter().enumerate() {
        let Some(projected) = shared::project(o, geom, range, Interface::Modern) else {
            continue;
        };
        let shape = get_blip_shape(Some(o), player);
        out.push(Blip {
            index: i,
            x: projected.x,
            y: projected.y,
            color: get_blip_color(Some(o)),
            shape,
            dim: if projected.bright { 1.0 } else { DIM_FACTOR },
            selected: selected == Some(o.id),
        });
    }
    out
}

/// The pixels of the **player's own marker** — the green cross at the centre of the ring —
/// relative to the centre point, in the order the radar issues them.
///
/// The radar's child-drawing pass is the only place it is drawn, and it is drawn
/// *after* [`draw_objects`] rather than as part of it, because the player is not in the radar's
/// info list at all (building an entry starts by rejecting the local player). The
/// sequence is: the centre pixel, then the client's four orthogonal neighbours, then
/// four more at ±2 — nine single-pixel fills, all in the radar's bright green.
#[must_use]
pub fn center_marker_pixels() -> Vec<(i32, i32)> {
    vec![
        // The centre pixel, in bright green.
        (0, 0),
        // The four edge neighbours, in retail's issue order: y-1, y+1, x-1, x+1.
        (0, -1),
        (0, 1),
        (-1, 0),
        (1, 0),
        // The four arms.
        (-2, 0),
        (2, 0),
        (0, -2),
        (0, 2),
    ]
}

/// The colour of that marker: the radar's bright green, the constant the child-drawing pass uses
/// for all nine fills.
pub const CENTER_MARKER_COLOR: RadarColor = BRIGHT_GREEN;

/// [`center_marker_pixels`] placed at the radar's centre point and turned into fills.
///
/// The centre is truncated exactly as the blips are (both axes, before each fill), so it
/// truncates toward zero and lands on the same integer lattice.
#[must_use]
pub fn center_marker_fills(geom: RadarGeometry) -> Vec<dereth_ui::UiFill> {
    let cx = to_i32(geom.center.0);
    let cy = to_i32(geom.center.1);
    let c = CENTER_MARKER_COLOR.rgb;
    let byte = |v: f32| -> u32 {
        let n = dereth_primitives::num::to_i32(v * 255.0).clamp(0, 255);
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: clamped to 0..=255 on the line above.
        let n = n as u32;
        n
    };
    let color = 0xFF00_0000 | (byte(c.0) << 16) | (byte(c.1) << 8) | byte(c.2);
    center_marker_pixels()
        .into_iter()
        .map(|(dx, dy)| dereth_ui::UiFill::point(cx + dx, cy + dy, color))
        .collect()
}

/// One [`Blip`] represented as flat-colour fills in the radar surface.
///
/// Every shape is drawn with 1×1 area fills into the element's surface; there is no texture and
/// no anti-aliasing. So a blip is
/// literally a handful of one-pixel fills, and [`dereth_ui::UiFill`] is that primitive. The order is
/// the order the blip drawer issues them: the shape first, then the selection brackets.
///
/// The colour is `dim`-multiplied **rgb with the alpha untouched**, which is what the blip drawer
/// does, and every entry in the palette carries alpha 1.0 (all thirteen colour constants do).
#[must_use]
pub fn blip_fills(b: &Blip) -> Vec<dereth_ui::UiFill> {
    let (r, g, bl) = b.dimmed_rgb();
    let byte = |v: f32| -> u32 {
        let n = dereth_primitives::num::to_i32(v * 255.0).clamp(0, 255);
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: clamped to 0..=255 on the line above.
        let n = n as u32;
        n
    };
    let color = 0xFF00_0000 | (byte(r) << 16) | (byte(g) << 8) | byte(bl);
    let mut out: Vec<dereth_ui::UiFill> = b
        .shape
        .pixels()
        .into_iter()
        .map(|(dx, dy)| dereth_ui::UiFill::point(b.x + dx, b.y + dy, color))
        .collect();
    if b.selected {
        out.extend(
            selected_pixels()
                .into_iter()
                .map(|(dx, dy)| dereth_ui::UiFill::point(b.x + dx, b.y + dy, color)),
        );
    }
    out
}

/// The object under the mouse — **which blip the pointer is on**, and the whole of what
/// makes a click on the radar mean something.
///
/// `mouse` is already relative to the element's screen origin — the same space [`Blip::x`] and [`Blip::y`] are in. The answer is the entry's
/// [`Blip::index`], i.e. the index into the caller's own object list.
///
/// The test, inside the client's loop and immediately before that object's blip is drawn: the
/// squared distance from the element-local mouse point to the blip is taken, and when it is
/// below `0x25` (37, so d <= 6 px) and below the best so far, that object becomes the best and
/// its name becomes the radar's tooltip (with the tooltip flag set). After the loop a non-zero
/// best becomes the object under the mouse, and when there is none the tooltip flag and the
/// tooltip are cleared.
///
/// **The tooltip is not decoration.**
/// The element base's tooltip write ends by recomputing the element's mouse visibility through
/// the "should be mouse visible" test: it has a context menu or valid tooltip text. The
/// radar sets no mouse-visible attribute anywhere and the shipped `0x21000005` layout gives
/// `0x100006D2` none, so **the radar body is transparent to the pointer except while a blip is
/// under it** — the tooltip is what makes it hit-testable, and clearing it is what makes it
/// transparent again. That is why the radar's select arm can be
/// guarded on nothing more than "there is an object under the mouse": a click on empty radar never
/// reaches the radar at all, it falls through to the game view behind it.
///
/// Three properties of the rule, each invisible if wrong:
///
/// * **It runs over the *drawn* set.** The object under the mouse is zeroed at the top of
///   the object-drawing pass and can only be set by an object that survived the showable filter, the round
///   range cull, the square bounding-box reject and the `Undef` shape skip — exactly
///   [`draw_objects`]'s output, in its order. A hidden or out-of-range object cannot be selected
///   by clicking where it would have been.
/// * **Nearest wins, ties to the earlier entry**, because the comparison is a strict `<` against
///   the running best. Blips overlap constantly at range 25 indoors.
/// * **The radius is 6 pixels, not 5.** `0x25` is 37 and the test is `<`, so the largest accepted
///   square distance is 36.
///
/// The rule (nearest blip within `0x25`) is certain; the tie-break is the less certain part.
#[must_use]
pub fn object_under_mouse(blips: &[Blip], mouse: (i32, i32)) -> Option<usize> {
    let mut best: Option<(i32, usize)> = None;
    for b in blips {
        let dx = mouse.0 - b.x;
        let dy = mouse.1 - b.y;
        let d = dx * dx + dy * dy;
        if d < HOVER_DIST_SQ && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, b.index));
        }
    }
    best.map(|(_, i)| i)
}

// -------------------------------------------------------------------------------------------
// The compass
// -------------------------------------------------------------------------------------------

/// The four compass tokens, and the quarter-turn each is offset by.
///
/// North uses `sin(h + π)` / `cos(h + π)`. The other quarter-turn offsets put
/// South opposite North and East to North's right in the heading-up display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compass {
    North,
    South,
    East,
    West,
}

impl Compass {
    /// The layout attribute that names this token's child element.
    #[must_use]
    pub const fn attribute(self) -> u32 {
        match self {
            Self::North => 0x1000_0031,
            Self::South => 0x1000_0032,
            Self::East => 0x1000_0033,
            Self::West => 0x1000_0034,
        }
    }

    /// The quarter-turn offset added to `heading + π`.
    #[must_use]
    pub fn phase(self) -> f32 {
        use std::f32::consts::FRAC_PI_2;
        match self {
            Self::North => 0.0,
            Self::South => 2.0 * FRAC_PI_2,
            Self::East => -FRAC_PI_2,
            Self::West => FRAC_PI_2,
        }
    }

    pub const ALL: [Self; 4] = [Self::North, Self::South, Self::East, Self::West];
}

/// The degrees→radians constant the client multiplies the heading by, as it appears in
/// the client's compass-token update: `0.017453292`.
pub const DEG_TO_RAD: f32 = 0.017_453_292;

/// The position for one token: the orbit position, before the `- width/2` centring the move
/// applies.
///
/// ```text
/// h = get_heading(player) * 0.017453292
/// x = centre.x + sin(h + π) * magnitude
/// y = centre.y + cos(h + π) * magnitude
/// ```
#[must_use]
pub fn compass_token_position(
    center: (f32, f32),
    heading_degrees: f32,
    magnitude: f32,
    which: Compass,
) -> (f32, f32) {
    let h = heading_degrees * DEG_TO_RAD + std::f32::consts::PI + which.phase();
    (
        center.0 + math::sinf(h) * magnitude,
        center.1 + math::cosf(h) * magnitude,
    )
}

/// The orbit radius the constructor records per token:
/// `|centre point − tokenCentre|`, where `tokenCentre` is
/// `((box.x1 + box.x0) / 2, (box.y1 + box.y0) / 2)`.
#[must_use]
pub fn token_magnitude(center: (f32, f32), token_box: dereth_ui::Box2D) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let tc = (
        (token_box.x1 + token_box.x0) as f32 / 2.0,
        (token_box.y1 + token_box.y0) as f32 / 2.0,
    );
    let dx = center.0 - tc.0;
    let dy = center.1 - tc.1;
    (dx * dx + dy * dy).sqrt()
}

// -------------------------------------------------------------------------------------------
// Coordinates
// -------------------------------------------------------------------------------------------

/// The coordinate text: the shared presentation rule.
pub use dereth_presentation::coordinates::{format_coordinate, update_coordinates, Coordinates};

/// The radar's two fixed children.
pub mod child {
    use dereth_ui::ElementId;
    /// The lock-UI button; element message `0x19` toggles the UI lock and broadcasts
    /// global message `0x0D`.
    pub const LOCK_BUTTON: ElementId = ElementId(0x1000_0619);
    /// The drag button, hidden at init and hidden again whenever the UI is locked.
    pub const DRAG_BUTTON: ElementId = ElementId(0x1000_06A3);
}

/// The two states the lock button switches between.
///
/// **These are `StateId`s in the element's own state map, not property values.** The shipped `client_local_English.dat` layout gives element `0x10000619` a state
/// map with exactly these two keys, and each carries one `MediaDesc`:
///
/// | state | media | |
/// |---|---|---|
/// | `0x10000063` | image `0x060074B7`, draw mode 3 | the closed padlock |
/// | `0x10000064` | image `0x060074B8`, draw mode 3 | the open padlock |
///
/// Its default state is **0**, and the base media list is **empty**, so an element
/// that is never told a state has no picture at all and leaves the upper-left of the ring blank.
/// The radar's initialisation and its lock toggle both end by setting the lock button's state to
/// one of these; writing them as a `MEDIA_STATE` enum **attribute** instead changes no media and
/// draws nothing even after the player toggles the lock. against the layout.
pub mod lock_state {
    /// Locked — the closed padlock, image `0x060074B7`.
    pub const LOCKED: u32 = 0x1000_0063;
    /// Unlocked — the open padlock, image `0x060074B8`.
    pub const UNLOCKED: u32 = 0x1000_0064;
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::ObjectId;

    fn obj(bitfield: u32) -> RadarEntry {
        RadarEntry {
            bitfield,
            in_world: true,
            ..Default::default()
        }
    }

    /// Behaviour: radar.shared-roles-and-projection-variants
    #[test]
    fn modern_radar_uses_shared_roles_with_its_own_pixels_and_picking() {
        let mut entry = obj(0x8000_4000);
        entry.id = ObjectId(2);
        entry.radar_enum = 4;
        entry.player_space = (-1.0, 1.0, 0.0);
        let geometry = RadarGeometry {
            radius: 50,
            center: (60.0, 60.0),
        };
        let rows = draw_objects(&[entry], None, geometry, 75.0, None, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].x, rows[0].y, rows[0].color.hex, rows[0].shape),
            (59, 59, 0xffffff, BlipShape::Default)
        );
        assert_eq!(object_under_mouse(&rows, (59, 59)), Some(0));
        entry.blip_color = 1;
        let rows = draw_objects(&[entry], None, geometry, 75.0, None, false);
        let fills = blip_fills(&rows[0]);
        assert_eq!(fills[0], dereth_ui::UiFill::point(59, 59, 0xff3fa8ff));
        entry.blip_color = 99;
        assert_eq!(get_blip_color(Some(&entry)).hex, 0xffffff);
        entry.bitfield |= 0x80;
        let rows = draw_objects(&[entry], None, geometry, 75.0, None, false);
        assert!(rows.is_empty());
        assert_eq!(object_under_mouse(&rows, (59, 59)), None);
    }

    /// Oracle: the ten base-colour RGB values and thirteen semantic aliases, including the three
    /// deliberate collisions.
    #[test]
    fn the_palette_is_ten_base_colours_and_thirteen_aliases_with_the_documented_collisions() {
        assert_eq!(BASE_PALETTE.len(), 10);
        let hexes: Vec<u32> = BASE_PALETTE.iter().map(|c| c.hex).collect();
        assert_eq!(
            hexes,
            vec![
                0x40A8FF, 0xFFAB00, 0xFFFF80, 0xFFFFFF, 0xFF4063, 0xBF63FF, 0xFFA8BF, 0x008040,
                0x00FFFF, 0x00FF00
            ]
        );
        assert_eq!(semantic::DEFAULT.hex, WHITE.hex);
        assert_eq!(semantic::LIFE_STONE.hex, BLUE.hex);
        assert_eq!(semantic::PORTAL.hex, PURPLE.hex);
        assert_eq!(semantic::PLAYER_KILLER.hex, RED.hex);
        assert_eq!(semantic::CREATURE.hex, GOLD.hex);
        // The three deliberate collisions.
        assert_eq!(
            semantic::ADMIN.hex,
            semantic::SENTINEL.hex,
            "Admin = Sentinel = Cyan"
        );
        assert_eq!(
            semantic::NPC.hex,
            semantic::VENDOR.hex,
            "NPC = Vendor = Yellow"
        );
        assert_eq!(
            semantic::FELLOWSHIP.hex,
            semantic::FELLOWSHIP_LEADER.hex,
            "Fellowship = FellowshipLeader = BrightGreen"
        );
        assert_eq!(
            semantic::ADVOCATE.hex,
            semantic::PK_LITE.hex,
            "Advocate = PKLite = Pink"
        );
    }

    /// Blip colors follow the rule's branch order.
    #[test]
    fn get_blip_color_follows_the_documented_decision_order() {
        // No object at all.
        assert_eq!(get_blip_color(None).hex, semantic::DEFAULT.hex);
        // The hidden bit beats everything, including an explicit blip colour.
        let mut hidden = obj(bitfield::HIDDEN | bitfield::PORTAL);
        hidden.blip_color = 5;
        assert_eq!(get_blip_color(Some(&hidden)).hex, semantic::DEFAULT.hex);

        // An explicit `blip_color` beats the bitfield.
        for (v, want) in [
            (1u8, BLUE),
            (2, GOLD),
            (3, WHITE),
            (4, PURPLE),
            (5, RED),
            (6, PINK),
            (7, GREEN),
            (8, YELLOW),
            (9, CYAN),
            (10, BRIGHT_GREEN),
        ] {
            let mut o = obj(bitfield::PORTAL);
            o.blip_color = v;
            assert_eq!(get_blip_color(Some(&o)).hex, want.hex, "blip_color {v}");
        }
        // An out-of-range `blip_color` falls to the default and does *not* fall through to the
        // bitfield tests.
        let mut o = obj(bitfield::PORTAL);
        o.blip_color = 99;
        assert_eq!(get_blip_color(Some(&o)).hex, semantic::DEFAULT.hex);

        // Portal before vendor, vendor before creature.
        assert_eq!(
            get_blip_color(Some(&obj(bitfield::PORTAL | bitfield::VENDOR))).hex,
            semantic::PORTAL.hex
        );
        assert_eq!(
            get_blip_color(Some(&obj(bitfield::VENDOR))).hex,
            semantic::VENDOR.hex
        );

        // Creature needs the bit *and* attackable *and* not a player.
        let mut c = obj(bitfield::CREATURE);
        c.is_attackable = true;
        assert_eq!(get_blip_color(Some(&c)).hex, semantic::CREATURE.hex);
        c.is_attackable = false;
        assert_eq!(get_blip_color(Some(&c)).hex, semantic::DEFAULT.hex);

        // A non-player that matched nothing is the default.
        assert_eq!(get_blip_color(Some(&obj(0))).hex, semantic::DEFAULT.hex);
    }

    /// Oracle: the player branch of the same rule — the admin test needs `0x100000` set and
    /// `0x40` clear, and the two fellowship tests run *after* the base is chosen and override it.
    #[test]
    fn the_player_branch_of_get_blip_color_layers_fellowship_over_the_base() {
        let player = |f: u32| {
            let mut o = obj(f);
            o.is_player = true;
            o
        };
        assert_eq!(get_blip_color(Some(&player(0))).hex, semantic::DEFAULT.hex);
        assert_eq!(
            get_blip_color(Some(&player(bitfield::ADMIN))).hex,
            semantic::ADMIN.hex
        );
        assert_eq!(
            get_blip_color(Some(&player(bitfield::ADMIN | bitfield::NOT_ADMIN))).hex,
            semantic::DEFAULT.hex,
            "0x40 suppresses the admin colour"
        );

        let mut pk = player(0);
        pk.is_pk = true;
        assert_eq!(get_blip_color(Some(&pk)).hex, semantic::PLAYER_KILLER.hex);
        let mut lite = player(0);
        lite.is_pk_lite = true;
        assert_eq!(get_blip_color(Some(&lite)).hex, semantic::PK_LITE.hex);
        assert_eq!(
            get_blip_color(Some(&player(bitfield::PLAYER_CREATURE))).hex,
            semantic::CREATURE.hex
        );

        // Fellowship overrides whatever base was chosen, leader first.
        let mut f = pk;
        f.is_fellow = true;
        assert_eq!(get_blip_color(Some(&f)).hex, semantic::FELLOWSHIP.hex);
        f.is_fellowship_leader = true;
        assert_eq!(
            get_blip_color(Some(&f)).hex,
            semantic::FELLOWSHIP_LEADER.hex
        );
    }

    /// Blip shapes follow the value table, including the
    /// "fellowship shape minus is-leader" arithmetic.
    #[test]
    fn get_blip_shape_follows_the_documented_decision_order_and_values() {
        assert_eq!(BlipShape::Undef as i32, 0);
        assert_eq!(BlipShape::Circle as i32, 1);
        assert_eq!(BlipShape::AllegianceMember as i32, 2);
        assert_eq!(BlipShape::Threat as i32, 3);
        assert_eq!(BlipShape::Default as i32, 4);
        assert_eq!(BlipShape::FellowshipLeader as i32, 5);
        assert_eq!(BlipShape::Fellowship as i32, 6);
        assert_eq!(BlipShape::ThreatAllegiance as i32, 7);
        assert_eq!(BlipShape::AllegianceMember.geometric(), "Box");
        assert_eq!(BlipShape::Default.geometric(), "Plus");
        // "Fellowship - isLeader": 6 - 1 = 5.
        assert_eq!(
            BlipShape::Fellowship as i32 - 1,
            BlipShape::FellowshipLeader as i32
        );

        assert_eq!(get_blip_shape(None, None), BlipShape::Undef);
        assert_eq!(
            get_blip_shape(Some(&obj(bitfield::HIDDEN)), None),
            BlipShape::Undef
        );

        let mut fellow = obj(0);
        fellow.is_fellow = true;
        assert_eq!(get_blip_shape(Some(&fellow), None), BlipShape::Fellowship);
        fellow.is_fellowship_leader = true;
        assert_eq!(
            get_blip_shape(Some(&fellow), None),
            BlipShape::FellowshipLeader
        );

        // The allegiance / threat tests need a player object to compare against.
        let mut ally = obj(0);
        ally.is_allegiance_member = true;
        assert_eq!(
            get_blip_shape(Some(&ally), None),
            BlipShape::Default,
            "no player, no test"
        );
        assert_eq!(
            get_blip_shape(Some(&ally), Some(&obj(0))),
            BlipShape::AllegianceMember
        );

        let mut pk = obj(0);
        pk.is_pk = true;
        let mut me = obj(0);
        me.is_pk = true;
        assert_eq!(get_blip_shape(Some(&pk), Some(&me)), BlipShape::Threat);
        assert_eq!(
            get_blip_shape(Some(&pk), Some(&obj(0))),
            BlipShape::Default,
            "a PK is only a Threat to a PK"
        );
        assert_eq!(
            get_blip_shape(Some(&obj(0)), Some(&obj(0))),
            BlipShape::Default
        );
    }

    /// the exact pixel offsets of all eight shapes, and the
    /// "every blip is at most 3×3" consequence.
    #[test]
    fn the_eight_blip_shapes_cover_the_documented_pixels() {
        let s = |sh: BlipShape| {
            let mut v = sh.pixels();
            v.sort_unstable();
            v
        };
        assert_eq!(s(BlipShape::Undef), Vec::<(i32, i32)>::new());
        assert_eq!(s(BlipShape::Circle), vec![(0, 0)]);
        // A cross = centre + the four edge neighbours.
        assert_eq!(
            s(BlipShape::Default),
            vec![(-1, 0), (0, -1), (0, 0), (0, 1), (1, 0)]
        );
        // An X = centre + the four corners.
        assert_eq!(
            s(BlipShape::Threat),
            vec![(-1, -1), (-1, 1), (0, 0), (1, -1), (1, 1)]
        );
        // A hollow box = edges + corners: a hollow 3x3 box, eight pixels, no centre.
        assert_eq!(s(BlipShape::AllegianceMember).len(), 8);
        assert!(!s(BlipShape::AllegianceMember).contains(&(0, 0)));
        // A filled box = the hollow box + the centre.
        assert_eq!(s(BlipShape::ThreatAllegiance).len(), 9);
        assert!(s(BlipShape::ThreatAllegiance).contains(&(0, 0)));
        assert_eq!(
            s(BlipShape::FellowshipLeader),
            vec![(-1, 1), (0, 0), (0, 1), (1, 1)]
        );
        assert_eq!(
            s(BlipShape::Fellowship),
            vec![(-1, -1), (0, -1), (0, 0), (1, -1)]
        );

        // "every blip is at most 3×3 pixels".
        for sh in [
            BlipShape::Circle,
            BlipShape::AllegianceMember,
            BlipShape::Threat,
            BlipShape::Default,
            BlipShape::FellowshipLeader,
            BlipShape::Fellowship,
            BlipShape::ThreatAllegiance,
        ] {
            for (x, y) in sh.pixels() {
                assert!((-1..=1).contains(&x) && (-1..=1).contains(&y), "{sh:?}");
            }
        }
    }

    /// "four 5-pixel brackets at ±3", i.e. a 7×7 open square.
    #[test]
    fn the_selected_marker_is_a_seven_by_seven_open_square_of_twenty_pixels() {
        let p = selected_pixels();
        assert_eq!(p.len(), 20);
        assert!(p.contains(&(-2, -3)) && p.contains(&(2, 3)));
        assert!(p.contains(&(-3, -2)) && p.contains(&(3, 2)));
        assert!(!p.contains(&(-3, -3)), "the corners are open");
        assert!(!p.contains(&(0, 0)));
    }

    /// 75 outdoors, 25 indoors, refreshed at most every 25 ms.
    #[test]
    fn the_range_is_seventy_five_outdoors_and_twenty_five_indoors() {
        assert_eq!(radar_range(true), 75.0);
        assert_eq!(radar_range(false), 25.0);
        assert_eq!(UPDATE_INTERVAL_SECONDS, 0.025);
    }

    /// the `(range − 1)²` cull, the axis signs, the
    /// bounding-box reject and the 5.0/0.65 dimming.
    #[test]
    fn draw_objects_culls_at_range_minus_one_squared_and_projects_with_y_up() {
        let geom = RadarGeometry {
            radius: 50,
            center: (60.0, 60.0),
        };
        let at = |x: f32, y: f32, z: f32| RadarEntry {
            id: ObjectId(1),
            player_space: (x, y, z),
            in_world: true,
            radar_enum: radar_enum::SHOW_ALWAYS,
            ..Default::default()
        };
        // range 75 -> cull at 74² = 5476. 74 units out is culled, 73 is not.
        let objs = [at(73.0, 0.0, 0.0), at(74.0, 0.0, 0.0)];
        let b = draw_objects(&objs, None, geom, 75.0, None, false);
        assert_eq!(
            b.len(),
            1,
            "the cull is one unit tighter than the drawn radius"
        );
        assert_eq!(b[0].index, 0);

        // scale = 50/75; +Y is up on screen, so a positive player-space y lands *above* centre.
        let objs = [at(0.0, 30.0, 0.0)];
        let b = draw_objects(&objs, None, geom, 75.0, None, false);
        assert_eq!(b[0].x, 60);
        assert_eq!(b[0].y, to_i32(60.0 - 30.0 * (50.0 / 75.0)));
        assert!(b[0].y < 60, "+Y is up");

        // The dimming threshold is strict: exactly 5.0 dims.
        let objs = [at(0.0, 0.0, 4.999), at(0.0, 0.0, 5.0), at(0.0, 0.0, -6.0)];
        let b = draw_objects(&objs, None, geom, 75.0, None, false);
        assert_eq!(b[0].dim, 1.0);
        assert_eq!(b[1].dim, DIM_FACTOR);
        assert_eq!(b[2].dim, DIM_FACTOR, "below the player dims too");

        // An object with no cell is skipped, and so is a hidden one.
        let mut nowhere = at(0.0, 0.0, 0.0);
        nowhere.in_world = false;
        let mut hidden = at(0.0, 0.0, 0.0);
        hidden.bitfield = bitfield::HIDDEN;
        assert!(draw_objects(&[nowhere, hidden], None, geom, 75.0, None, false).is_empty());
    }

    /// "returns immediately when the radar-blank flag is
    /// set", and "multiplies **r, g, b** by `dim` (alpha untouched)".
    #[test]
    fn radar_blank_draws_nothing_and_dim_scales_only_rgb() {
        let geom = RadarGeometry {
            radius: 50,
            center: (60.0, 60.0),
        };
        let o = RadarEntry {
            player_space: (0.0, 0.0, 0.0),
            in_world: true,
            radar_enum: radar_enum::SHOW_ALWAYS,
            ..Default::default()
        };
        assert!(
            !draw_objects(&[o], None, geom, 75.0, None, false).is_empty(),
            "drawn when not blank"
        );
        assert!(draw_objects(&[o], None, geom, 75.0, None, true).is_empty());

        let b = Blip {
            index: 0,
            x: 0,
            y: 0,
            color: GOLD,
            shape: BlipShape::Default,
            dim: DIM_FACTOR,
            selected: false,
        };
        let (r, g, bl) = b.dimmed_rgb();
        assert!((r - 1.00 * 0.65).abs() < 1e-6);
        assert!((g - 0.67 * 0.65).abs() < 1e-6);
        assert!((bl - 0.0).abs() < 1e-6);
    }

    /// "Mouse-over radius is `sqrt(36) = 6` pixels (`dist² < 0x25`); the closest
    /// object within it wins."
    #[test]
    fn the_mouse_over_test_is_strictly_inside_six_pixels_and_the_closest_wins() {
        let blip = |i: usize, x: i32, y: i32| Blip {
            index: i,
            x,
            y,
            color: WHITE,
            shape: BlipShape::Default,
            dim: 1.0,
            selected: false,
        };
        let bs = [blip(0, 10, 10), blip(1, 12, 10)];
        assert_eq!(
            object_under_mouse(&bs, (11, 10)),
            Some(0),
            "1 away beats 1 away, first wins"
        );
        assert_eq!(object_under_mouse(&bs, (13, 10)), Some(1));
        // 0x25 = 37, so a distance² of 36 (6 pixels) is inside and 37 is not.
        assert_eq!(object_under_mouse(&[blip(0, 0, 0)], (6, 0)), Some(0));
        assert_eq!(
            object_under_mouse(&[blip(0, 0, 0)], (6, 1)),
            None,
            "36+1 = 37 is not < 0x25"
        );
    }

    /// the tokens orbit the centre at their recorded magnitude and stay on the
    /// circle, and each token's magnitude is the distance from the centre to the token's midpoint.
    #[test]
    fn the_compass_letters_orbit_the_centre_and_sit_at_the_cardinal_points() {
        let center = (60.0, 60.0);
        let mag = token_magnitude(center, dereth_ui::Box2D::new(50, 10, 70, 30));
        // token centre is (60, 20); the distance is 40.
        assert!((mag - 40.0).abs() < 1e-4, "{mag}");

        // Heading 0: `sin(π) = 0`, `cos(π) = -1`, so N sits *above* the centre.
        let (x, y) = compass_token_position(center, 0.0, 40.0, Compass::North);
        assert!((x - 60.0).abs() < 1e-3, "{x}");
        assert!(
            (y - 20.0).abs() < 1e-3,
            "N is at the top when facing north, got {y}"
        );
        let (_, ys) = compass_token_position(center, 0.0, 40.0, Compass::South);
        assert!((ys - 100.0).abs() < 1e-3, "S is opposite N, got {ys}");
        let (xe, _) = compass_token_position(center, 0.0, 40.0, Compass::East);
        assert!((xe - 100.0).abs() < 1e-3, "E is to the right, got {xe}");
        let (xw, _) = compass_token_position(center, 0.0, 40.0, Compass::West);
        assert!((xw - 20.0).abs() < 1e-3, "W is to the left, got {xw}");

        // Turning 90 degrees rotates the whole ring but keeps every token on the circle.
        for c in Compass::ALL {
            let (x, y) = compass_token_position(center, 90.0, 40.0, c);
            let d = ((x - 60.0).powi(2) + (y - 60.0).powi(2)).sqrt();
            assert!((d - 40.0).abs() < 1e-3, "{c:?} left the circle: {d}");
        }
        assert_eq!(Compass::North.attribute(), 0x1000_0031);
        assert_eq!(Compass::West.attribute(), 0x1000_0034);
    }

    /// the two fixed children and the two lock media states.
    #[test]
    fn the_lock_button_and_its_two_media_states_are_the_documented_ids() {
        assert_eq!(child::LOCK_BUTTON, dereth_ui::ElementId(0x1000_0619));
        assert_eq!(child::DRAG_BUTTON, dereth_ui::ElementId(0x1000_06A3));
        assert_eq!(lock_state::LOCKED, 0x1000_0063);
        assert_eq!(lock_state::UNLOCKED, 0x1000_0064);
    }
}
