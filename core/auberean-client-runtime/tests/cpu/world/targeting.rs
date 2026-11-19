//! The selection keys' rules: what each key of a kind selects among the objects around the player.

use auberean_client_contract::SelectKind;
use auberean_client_runtime::targeting::{kind, pick, Candidate};

const ME: u64 = 0x2130_0000_0000_00A1;
/// The radar's ranges, indoors and outdoors.
const INSIDE: f32 = 50.0;
const OUTSIDE: f32 = 75.0;

/// An object of these kind bits that may be selected, `x` meters right and `y` ahead of the
/// player.
fn thing(id: u64, kind: u32, x: f32, y: f32) -> Candidate {
    Candidate {
        id,
        kind,
        from_player: [x, y, 0.0],
        selectable: true,
        ..Candidate::default()
    }
}

/// The selection keys pick by kind among the objects in the radar's range, ordered by how far
/// each stands (along the ground, and a fifth more for each meter up or down), then by id: the
/// next after the selection, the previous before it, round again past the last; the nearest; a
/// foe is one the player may attack that is no fellow's; an item is passed over by the creature
/// keys; what a body holds is found by the item keys, at the body; indoors the range is the
/// shorter.
///
/// Behaviour: ui.selection.the-selection-keys-pick-by-kind-and-distance
#[test]
fn the_selection_keys_pick_by_kind_and_distance() {
    let mut near = thing(10, kind::CREATURE, 0.0, 3.0);
    near.attackable = true;
    let mut mid = thing(11, kind::CREATURE, 4.0, 0.0);
    // Four meters along the ground and a meter up: 5.2.
    mid.from_player[2] = 1.0;
    let far = thing(12, kind::CREATURE, 0.0, -6.0);
    // 5.1 along the ground: nearer than the one a meter up.
    let level = thing(16, kind::CREATURE, 5.1, 0.0);
    let item = thing(13, kind::ITEM[0], 0.0, 1.0);
    let out_of_range = thing(14, kind::CREATURE, 80.0, 0.0);
    // What a body holds stands where the body does.
    let held = thing(15, kind::ITEM[0], 0.0, 0.5);
    let me = Candidate {
        id: ME,
        kind: kind::PLAYER,
        selectable: true,
        ..Candidate::default()
    };
    let mut w = vec![me, far, item, mid, near, out_of_range, held, level];
    let creature = SelectKind::Creature;
    let at = |w: &[Candidate], selection: u64, kind: SelectKind, step: i32| {
        pick(w, ME, selection, OUTSIDE, kind, step)
    };

    assert_eq!(at(&w, 0, creature, 0), Some(10));
    assert_eq!(
        at(&w, 0, creature, 1),
        Some(10),
        "no selection: the nearest"
    );
    assert_eq!(at(&w, 10, creature, 1), Some(16));
    assert_eq!(
        at(&w, 16, creature, 1),
        Some(11),
        "a meter up counts a fifth more"
    );
    assert_eq!(at(&w, 11, creature, 1), Some(12));
    assert_eq!(
        at(&w, 12, creature, 1),
        Some(10),
        "round again past the last"
    );
    assert_eq!(at(&w, 12, creature, -1), Some(11));
    assert_eq!(
        at(&w, 10, creature, -1),
        Some(12),
        "round again past the first"
    );
    assert_eq!(
        at(&w, 0, SelectKind::Item, 0),
        Some(15),
        "what a body holds counts too, at the body"
    );
    assert_eq!(
        at(&w, 0, SelectKind::Foe, 0),
        Some(10),
        "only the one it may attack"
    );
    assert_eq!(at(&w, 0, SelectKind::Player, 0), None, "not itself");
    w.iter_mut()
        .filter(|t| t.id == 10)
        .for_each(|t| t.fellow = true);
    assert_eq!(at(&w, 0, SelectKind::Foe, 0), None, "not a fellow");
    assert_eq!(
        at(&w, 0, SelectKind::Fellow, 0),
        None,
        "a creature is no fellow"
    );
    w.iter_mut()
        .filter(|t| t.id == 12)
        .for_each(|t| t.from_player = [0.0, -60.0, 0.0]);
    w.retain(|t| t.id != 16);
    assert_eq!(
        pick(&w, ME, 11, INSIDE, creature, 1),
        Some(10),
        "indoors the shorter range: the far one is out of it"
    );
    w.iter_mut()
        .filter(|t| t.id == 11)
        .for_each(|t| t.owner = Some(ME));
    assert_eq!(
        at(&w, 0, SelectKind::Pet, 0),
        Some(11),
        "the player's own pet"
    );
    w.iter_mut()
        .filter(|t| t.id == 11)
        .for_each(|t| t.selectable = false);
    assert_eq!(
        at(&w, 0, SelectKind::Pet, 0),
        None,
        "one that may not be selected"
    );
}
