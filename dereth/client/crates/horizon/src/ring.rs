//! The selection ring: the picture laid on the ground under the selected object, and the ground
//! marker the interface asks the world to draw each frame.

use dereth_client_contract::overlay::GroundMarker;
use dereth_primitives::ObjectId;
use dereth_ui_kit::selection;

pub use dereth_ui_kit::selection::{
    drawn, inner_picture, picture, RING_INNER_PIECE, RING_INNER_TEXTURE, RING_INNER_TURN_SECONDS,
    RING_PIECE, RING_SCALE, RING_TEXTURE, RING_TURN_SECONDS,
};

use crate::ui::game::Relation;

/// Whether a selection of `relation` is ringed: a player, a person or a creature. A thing (an
/// item, a door, something wielded) is marked by its name and the chevron alone.
#[must_use]
pub fn ringed(relation: Relation) -> bool {
    relation != Relation::Object
}

/// The ring's colour for `relation`, as the target bar names it: a creature to fight yellow, one
/// fighting the player orange, a person green, a player blue, anything else white.
#[must_use]
pub fn tint(relation: Relation) -> u32 {
    match relation {
        Relation::Hostile => 0xFFF3_D36C,
        Relation::Engaged => 0xFFFF_7B52,
        Relation::Npc => 0xFFA8_E0A0,
        Relation::Player => 0xFFB0_D8FF,
        Relation::Object => 0xFFFF_FFFF,
    }
}

/// The ground marker for the selection `target` (its object and how it stands to the player) at
/// `time` seconds: the ring, tinted for it and turned a little further each moment. `None` with
/// nothing selected.
#[must_use]
pub fn marker(target: Option<(ObjectId, Relation)>, time: f64) -> Option<GroundMarker> {
    selection::marker(target.map(|(o, r)| (o, tint(r))), time)
}

/// The ground markers for the selection `target` at `time` seconds: the inner layer first, when
/// the art has one (`inner`), turning the other way, then the outer ring over it. None with
/// nothing selected.
#[must_use]
pub fn markers(target: Option<(ObjectId, Relation)>, time: f64, inner: bool) -> Vec<GroundMarker> {
    selection::markers(target.map(|(o, r)| (o, tint(r))), time, inner)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    #[test]
    fn the_ring_lies_under_the_selection_tinted_for_it_and_none_without_one() {
        let grocer = ObjectId(0x7A9B_4024);
        let m = marker(Some((grocer, Relation::Npc)), 0.0).expect("a selection has a ring");
        assert_eq!(m.object, grocer);
        assert_eq!(m.tint, 0xFFA8_E0A0);
        assert_eq!(m.texture, RING_TEXTURE);
        assert!((m.scale - RING_SCALE).abs() < f32::EPSILON);
        assert_eq!(
            marker(Some((grocer, Relation::Hostile)), 0.0).unwrap().tint,
            0xFFF3_D36C
        );
        assert_eq!(marker(None, 3.0), None);
    }

    #[test]
    fn an_inner_layer_lies_under_the_ring_and_turns_the_other_way() {
        let grocer = Some((ObjectId(0x7A9B_4024), Relation::Npc));
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
        assert!(markers(None, 1.0, true).is_empty());
    }

    #[test]
    fn only_players_people_and_creatures_are_ringed() {
        for r in [
            Relation::Hostile,
            Relation::Engaged,
            Relation::Npc,
            Relation::Player,
        ] {
            assert!(ringed(r), "{r:?}");
        }
        assert!(!ringed(Relation::Object));
    }

    #[test]
    fn the_ring_turns_once_in_its_period() {
        let at = |t: f64| {
            marker(Some((ObjectId(1), Relation::Object)), t)
                .unwrap()
                .turn
        };
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
}
