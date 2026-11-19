//! The selection's marker any game's interface draws the same way: a ring laid on the ground under
//! the selected object, turning slowly, with a fainter inner layer turning the other way where the
//! art has one; and a single arrow pointing down at the selection from over its name, bobbing a
//! little. The ring reaches the world as ground markers ([`GroundMarker`]), which the game's world
//! renderer lays on the ground under the object; the arrow is drawn with the rest of the interface.

use dereth_primitives::{TextureData, TextureFormat};
use dereth_ui_overlay::{GroundMarker, OverlaySpace, OverlayTexture};

use crate::art::{Art, Sprite};
use crate::draw::Rect;

/// The ring's texture, among the interface's own.
pub const RING_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x5249_4E47,
};

/// The art piece the ring is drawn from, when the art has one.
pub const RING_PIECE: &str = "ground.ring";

/// The ring's inner layer among the interface's textures, and the art piece it is drawn from.
/// It lies under the outer ring and turns the other way, so the two move against each other.
pub const RING_INNER_TEXTURE: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x5249_4E49,
};
pub const RING_INNER_PIECE: &str = "ground.ring-inner";

/// How long the inner layer takes to turn once, the other way, in seconds.
pub const RING_INNER_TURN_SECONDS: f64 = 12.0;

/// How wide the ring is, as a multiple of the radius its object is selected within.
pub const RING_SCALE: f32 = 1.25;

/// How long the ring takes to turn once, in seconds.
pub const RING_TURN_SECONDS: f64 = 8.0;

/// The ring and the arrow drawn in the art's own colours.
pub const UNTINTED: u32 = 0xFFFF_FFFF;

/// The side of the drawn ring, in pixels, when the art has none.
const DRAWN_SIZE: u32 = 128;

/// The art piece the arrow over the selection is drawn from.
pub const ARROW_PIECE: &str = "target.marker";

/// The arrow's height, how far it stands over the top of what it points at, and how far it bobs,
/// in layout units; and how fast it bobs, in radians a second.
pub const ARROW_HEIGHT: f32 = 26.0;
pub const ARROW_GAP: f32 = 6.0;
pub const ARROW_BOB: f32 = 2.0;
pub const ARROW_BOB_RATE: f64 = 4.0;

/// The ground marker for the selection `target` (its object and the colour the ring is drawn in)
/// at `time` seconds: the ring, turned a little further each moment. `None` with nothing
/// selected.
#[must_use]
pub fn marker<O>(target: Option<(O, u32)>, time: f64) -> Option<GroundMarker<O>> {
    let (object, tint) = target?;
    let turns = (time / RING_TURN_SECONDS).fract();
    #[allow(clippy::cast_possible_truncation)]
    let turn = (turns * std::f64::consts::TAU) as f32;
    Some(GroundMarker {
        object,
        texture: RING_TEXTURE,
        uv: [0.0, 0.0, 1.0, 1.0],
        tint,
        scale: RING_SCALE,
        turn,
    })
}

/// The ground markers for the selection `target` (as for [`marker`]) at `time` seconds: the inner
/// layer first, when the art has one (`inner`), turning the other way, then the outer ring over
/// it. None with nothing selected.
#[must_use]
pub fn markers<O: Copy>(target: Option<(O, u32)>, time: f64, inner: bool) -> Vec<GroundMarker<O>> {
    let Some(outer) = marker(target, time) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(2);
    if inner {
        let turns = (time / RING_INNER_TURN_SECONDS).fract();
        #[allow(clippy::cast_possible_truncation)]
        let turn = -(turns * std::f64::consts::TAU) as f32;
        out.push(GroundMarker {
            texture: RING_INNER_TEXTURE,
            turn,
            ..outer
        });
    }
    out.push(outer);
    out
}

/// The inner layer's picture, when the art has one.
#[must_use]
pub fn inner_picture(art: &Art) -> Option<TextureData> {
    art.piece_picture(RING_INNER_PIECE)
}

/// The ring's picture: the art's piece when it has one, a white ring the tint colours, drawn
/// here, when it does not.
#[must_use]
pub fn picture(art: &Art) -> TextureData {
    art.piece_picture(RING_PIECE).unwrap_or_else(drawn)
}

/// A white ring on nothing, soft at both edges, with a fainter ring inside it.
#[must_use]
pub fn drawn() -> TextureData {
    let n = DRAWN_SIZE;
    #[allow(clippy::cast_precision_loss)]
    let half = n as f32 / 2.0;
    let band =
        |r: f32, centre: f32, width: f32| (1.0 - ((r - centre).abs() / width)).clamp(0.0, 1.0);
    let mut bgra = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let (dx, dy) = (
                (x as f32 + 0.5 - half) / half,
                (y as f32 + 0.5 - half) / half,
            );
            let r = (dx * dx + dy * dy).sqrt();
            let a = band(r, 0.88, 0.07).max(band(r, 0.72, 0.03) * 0.45);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let a = (a * 255.0).round() as u8;
            bgra.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    TextureData {
        width: n,
        height: n,
        format: TextureFormat::Bgra8,
        levels: vec![bgra],
    }
}

/// Where the arrow `piece` stands at `k` pixels a layout unit and `time` seconds: centred over
/// the span `width` pixels wide from `left` whose top is `top` (a name as it is drawn), its foot
/// [`ARROW_GAP`] over it, bobbing up by as much as [`ARROW_BOB`].
#[must_use]
pub fn arrow_rect(piece: &Sprite, left: f32, top: f32, width: f32, k: f32, time: f64) -> Rect {
    let mh = ARROW_HEIGHT * k;
    let mw = mh * piece.w / piece.h.max(1.0);
    let bob = ARROW_BOB * k * crate::wave(time, ARROW_BOB_RATE);
    Rect::new(
        left + (width - mw) / 2.0,
        top - mh - ARROW_GAP * k - bob,
        mw,
        mh,
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    #[test]
    fn the_ring_lies_under_the_selection_in_the_colour_asked_and_none_without_one() {
        let m = marker(Some((7u64, 0xFFA8_E0A0)), 0.0).expect("a selection has a ring");
        assert_eq!(m.object, 7);
        assert_eq!(m.tint, 0xFFA8_E0A0);
        assert_eq!(m.texture, RING_TEXTURE);
        assert!((m.scale - RING_SCALE).abs() < f32::EPSILON);
        assert_eq!(marker(Some((7u64, UNTINTED)), 0.0).unwrap().tint, UNTINTED);
        assert_eq!(marker::<u64>(None, 3.0), None);
    }

    #[test]
    fn an_inner_layer_lies_under_the_ring_and_turns_the_other_way() {
        let grocer = Some((7u64, UNTINTED));
        assert_eq!(markers(grocer, 1.0, false).len(), 1, "the ring alone");
        let both = markers(grocer, 1.0, true);
        assert_eq!(
            both.iter().map(|m| m.texture).collect::<Vec<_>>(),
            [RING_INNER_TEXTURE, RING_TEXTURE],
            "the inner layer first, under the ring"
        );
        assert!(
            both[0].turn < 0.0 && both[1].turn > 0.0,
            "they turn against each other"
        );
        assert_eq!(both[0].tint, both[1].tint);
        assert!(markers::<u64>(None, 1.0, true).is_empty());
    }

    #[test]
    fn the_ring_turns_once_in_its_period() {
        let at = |t: f64| marker(Some((1u32, UNTINTED)), t).unwrap().turn;
        assert!(at(0.0).abs() < 1e-6);
        assert!((at(RING_TURN_SECONDS / 4.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!(
            at(RING_TURN_SECONDS).abs() < 1e-5,
            "a whole turn is back where it began"
        );
    }

    #[test]
    fn the_drawn_ring_shows_at_its_rim_and_nowhere_at_its_centre_or_corners() {
        let t = drawn();
        let n = t.width as usize;
        let alpha = |x: usize, y: usize| t.levels[0][(y * n + x) * 4 + 3];
        assert_eq!(alpha(n / 2, n / 2), 0, "the centre is clear");
        assert_eq!(alpha(0, 0), 0, "the corners are clear");
        // On the rim, 0.88 of the way out from the centre.
        let rim = n / 2 + n * 88 / 200;
        assert!(alpha(rim, n / 2) > 200);
    }

    #[test]
    fn the_arrow_stands_centred_over_the_top_it_points_at_and_bobs_up() {
        let piece = Sprite {
            tex: 0,
            src: Rect::new(0.0, 0.0, 13.0, 26.0),
            w: 13.0,
            h: 26.0,
            scale: 1.0,
        };
        let (k, left, top, width) = (2.0, 100.0, 300.0, 80.0);
        // At the wave's lowest the arrow's foot is the gap over the top.
        let low = (3.0 * std::f64::consts::FRAC_PI_2) / ARROW_BOB_RATE;
        let r = arrow_rect(&piece, left, top, width, k, low);
        assert!((r.h - ARROW_HEIGHT * k).abs() < 1e-4);
        assert!((r.w - r.h / 2.0).abs() < 1e-4, "the piece's own shape");
        assert!(
            (r.x + r.w / 2.0 - (left + width / 2.0)).abs() < 1e-4,
            "centred"
        );
        assert!((r.bottom() - (top - ARROW_GAP * k)).abs() < 1e-3, "{r:?}");
        let high = std::f64::consts::FRAC_PI_2 / ARROW_BOB_RATE;
        let up = arrow_rect(&piece, left, top, width, k, high);
        assert!((r.y - up.y - ARROW_BOB * k).abs() < 1e-3, "bobs up: {up:?}");
    }
}
