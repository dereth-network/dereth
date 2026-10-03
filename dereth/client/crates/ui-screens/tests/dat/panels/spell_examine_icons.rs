//! The spell examination pane draws its pictures as the rest of the magic interface does: the
//! spell's icon composed as the spellbook composes it (the power level's background, the icon, the
//! wash and the badge its bitfield names), and each formula component's icon with its white
//! outline turned black, as the Components tab draws it.
//! Fixture: shipped layouts loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::DataId;
use dereth_ui::region::{IconRecipe, SurfaceOp};
use dereth_ui::{Screen, UiSystem};
use dereth_ui_screens::panels::spell_examine::{SpellExamineUi, SPELL_ICON};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, SpellExamineComponent, SpellExamineView};

const FLAME_BOLT: u32 = 0x5C;

#[derive(Debug)]
struct Book;

impl GameView for Book {
    fn spell_examine(&self, spell_id: u32) -> Option<SpellExamineView> {
        (spell_id == FLAME_BOLT).then(|| SpellExamineView {
            name: "Flame Bolt I".into(),
            description: "Shoots a bolt of flame.".into(),
            school: 1,
            icon: Some(DataId(0x0600_1386)),
            level: 1,
            bitfield: 0x10,
            components: vec![
                Some(SpellExamineComponent {
                    scid: 1,
                    name: "Lead Scarab".into(),
                    icon: Some(DataId(0x0600_13E7)),
                }),
                Some(SpellExamineComponent {
                    scid: 0x26,
                    name: "Red Taper".into(),
                    icon: Some(DataId(0x0600_1413)),
                }),
            ],
            ..SpellExamineView::default()
        })
    }
}

fn pane() -> (UiSystem, SpellExamineUi, dereth_ui::ElemHandle) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    let root = s.roots()[0];
    let mut p = SpellExamineUi::default();
    p.post_init(&mut ui, root);
    assert!(p.bound() && p.formula_list_bound(), "the shipped pane");
    (ui, p, root)
}

/// Behaviour: magic.examine.the-spell-pane-draws-its-icons-as-the-spellbook-and-components-tab-do
#[test]
fn the_examined_spells_icon_is_composed_and_its_components_have_black_outlines() {
    let (mut ui, mut p, root) = pane();
    p.examine_spell(&mut ui, &Book, FLAME_BOLT)
        .expect("the spell is drawn");

    let icon = ui.get_child_recursive(root, SPELL_ICON).expect("the icon");
    let g = ui
        .node(icon)
        .and_then(|n| n.region.image.clone())
        .expect("a picture");
    match g.op {
        Some(SurfaceOp::Icon(IconRecipe::Spell {
            background,
            icon,
            tint,
            ..
        })) => {
            assert_eq!(icon, Some(DataId(0x0600_1386)), "the spell's own icon");
            assert!(background.is_some(), "on its power level's background");
            assert!(tint.is_some(), "and the bitfield's wash");
        }
        other => panic!("the spell icon is a plain picture: {other:?}"),
    }

    for i in 0..2 {
        let row = p.component_row(i).expect("a component row");
        let op = ui
            .node(row)
            .and_then(|n| n.region.image.as_ref())
            .and_then(|g| g.op);
        assert_eq!(
            op,
            Some(SurfaceOp::ReplaceColor {
                from: SurfaceOp::OPAQUE_WHITE,
                to: SurfaceOp::OPAQUE_BLACK,
            }),
            "component {i} has a black outline"
        );
    }
}
