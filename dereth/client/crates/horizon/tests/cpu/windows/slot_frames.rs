//! Every slot's frame stands whole where it is drawn: the frames stand a little outside their
//! slots, and no list or grid cuts them off at its edges. Drawn with the interface's own art, as
//! a player sees it.
//!
//! Behaviour: none (experimental Horizon interface)

use std::sync::Arc;

use dereth_client_contract::combat_mode::MAGIC;
use dereth_client_contract::view::{ComponentCategory, ComponentRow};
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::{GameState, Item};
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

const ME: ObjectId = ObjectId(0x5000_0001);
const CHEST: ObjectId = ObjectId(0x8000_0020);
const TOOL: ObjectId = ObjectId(0x8000_0030);

/// The interface at 1920x1080 with the pieces and fonts the client carries.
fn harness() -> Harness {
    let mut h = Harness::new(Default::default());
    h.ui = dereth_horizon::ui::HorizonUi::new(
        Arc::new(dereth_horizon::art::Art::new(Arc::new(
            dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
        ))),
        Default::default(),
    );
    h
}

fn item(id: u32, name: &str) -> Item {
    Item {
        id: ObjectId(id),
        name: name.into(),
        stack: Some(1),
        ..Item::default()
    }
}

/// A caster in the world with spell research, carrying twelve kinds of component, three packs,
/// a pack of things, a chest open and things gathered for salvage.
fn carrying() -> GameState {
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        combat_mode: MAGIC,
        player_id: Some(ME),
        main_pack: Some(item(ME.0, "Tester")),
        pack: (0..20).map(|n| item(0x8000_0100 + n, "Potion")).collect(),
        side_packs: (0..3)
            .map(|n| {
                (
                    Item {
                        container: true,
                        ..item(0x8000_0200 + n, "Sack")
                    },
                    Vec::new(),
                )
            })
            .collect(),
        loot: Some((
            CHEST,
            "Chest".into(),
            (0..6).map(|n| item(0x8000_0300 + n, "Gem")).collect(),
        )),
        salvage: Some((
            TOOL,
            (0..4).map(|n| item(0x8000_0400 + n, "Sword")).collect(),
        )),
        components: vec![ComponentCategory {
            category: 0,
            rows: (0..12)
                .map(|n| ComponentRow {
                    wcid: 690 + n,
                    name: format!("Scarab {n}"),
                    icon: None,
                    owned: 3,
                    desired: 0,
                    object: Some(ObjectId(0x8000_0500 + n)),
                })
                .collect(),
        }],
        ..GameState::default()
    };
    state.world_services.features.spell_research = true;
    state.world_services.era_ui.void_magic = true;
    state.world_services.spell_filters = dereth_client_contract::spellbook::DEFAULT_SPELL_FILTERS;
    state
}

/// `windows` opened over `state`, settled.
fn open(state: &GameState, windows: &[WindowId]) -> Harness {
    let mut h = harness();
    h.frame(state);
    for w in windows {
        h.ui.windows.open(*w, -1.0);
    }
    for _ in 0..4 {
        h.frame(state);
    }
    h
}

/// The Spellbook open on its last tab, Create Spell.
fn create_spell(state: &GameState) -> Harness {
    let mut h = open(state, &[WindowId::Actions]);
    let snap = h.input.nav.last.clone();
    let bar = snap
        .tabs_in(snap.layers.len())
        .expect("the Spellbook's tabs are noted");
    // Round from the first tab to the last.
    h.input.pad.tab_step = Some((bar, -1));
    h.frame(state);
    h.frame(state);
    h
}

/// The slots' frames drawn this frame, each with the clip it was drawn under.
fn frames(h: &Harness) -> Vec<(Rect, Option<Rect>)> {
    let scale = h.ui.layout_scale(h.screen);
    let pieces: Vec<_> = ["slot.empty", "slot.hover", "slot.pressed"]
        .iter()
        .filter_map(|name| h.ui.art.piece(name, scale))
        .collect();
    assert_eq!(pieces.len(), 3, "the slot's frames are built in");
    h.list
        .quads
        .iter()
        .filter(|q| {
            pieces
                .iter()
                .any(|s| q.tex == Some(s.tex) && q.src == s.src)
        })
        .map(|q| (q.dst, q.clip))
        .collect()
}

/// Whether `frame` lies whole within `clip`.
fn whole(frame: Rect, clip: Option<Rect>) -> bool {
    clip.is_none_or(|c| {
        frame.x >= c.x - 0.01
            && frame.y >= c.y - 0.01
            && frame.right() <= c.right() + 0.01
            && frame.bottom() <= c.bottom() + 0.01
    })
}

#[test]
fn the_create_spell_component_list_shows_its_first_slot_s_frame_whole() {
    let state = carrying();
    let h = create_spell(&state);
    // The component list is the one grid there drawn under a clip.
    let listed: Vec<(Rect, Rect)> = frames(&h)
        .into_iter()
        .filter_map(|(f, c)| Some((f, c?)))
        .collect();
    assert!(
        listed.len() >= 12,
        "the twelve components' slots are drawn in the list: {}",
        listed.len()
    );
    let (first, area) = listed
        .iter()
        .copied()
        .min_by(|a, b| (a.0.y, a.0.x).partial_cmp(&(b.0.y, b.0.x)).unwrap())
        .unwrap();
    assert!(
        whole(first, Some(area)),
        "the first slot's frame {first:?} is cut by the list's edge {area:?}"
    );
    for (frame, clip) in listed {
        assert!(whole(frame, Some(clip)), "{frame:?} cut by {clip:?}");
    }
}

#[test]
fn no_window_s_grid_cuts_off_its_slots_frames() {
    let state = carrying();
    let mut checked = Vec::new();
    for (what, h) in [
        ("the inventory", open(&state, &[WindowId::Inventory])),
        ("the character", open(&state, &[WindowId::Character])),
        ("a chest", open(&state, &[WindowId::Loot])),
        ("the salvage window", open(&state, &[WindowId::Salvage])),
        ("the HUD's bars", open(&state, &[])),
        ("Create Spell", create_spell(&state)),
    ] {
        let drawn = frames(&h);
        assert!(!drawn.is_empty(), "{what}: slots drawn");
        let cut: Vec<_> = drawn.iter().filter(|(f, c)| !whole(*f, *c)).collect();
        assert!(cut.is_empty(), "{what}: frames cut off: {cut:?}");
        checked.push(what);
    }
    assert_eq!(checked.len(), 6);
}
