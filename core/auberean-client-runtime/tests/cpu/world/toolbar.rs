//! The shortcut bar's clean-up, which the client runs for every interface: which slots it empties,
//! by what the toolbar says of each slot and what the notices since its last look told.

use auberean_client_runtime::toolbar::{notice, Checks, Item, Skill, Toolbar};
use auberean_script::options::{kind, Shortcut};

/// Skills: one trained, one no longer; items: one the player has, one it no longer has; a recipe.
const STRIKE: u32 = 0x10;
const SHOOT: u32 = 0x12;
const SWORD: u64 = 0x7000_0000_0000_0001;
const GONE: u64 = 0x7000_0000_0000_0002;
const RECIPE: u32 = 0x5C00_0001;

/// How the inventory changed, as its notice says: everything, or one item.
const ALL: u32 = 0x4000_0001;
const ONE: u32 = 0x4000_0002;

fn shortcut(kind: u32) -> Shortcut {
    Shortcut {
        kind,
        ..Shortcut::default()
    }
}

/// The toolbar: slot 0 the trained skill, 1 the untrained one, 2 the sword, 3 the item gone, 4 an
/// emote, 5 the recipe, 6 a recipe naming none, 7 a line of text; the rest empty.
fn toolbar() -> Toolbar {
    let mut shortcuts = vec![Shortcut::default(); 100];
    shortcuts[0] = Shortcut {
        data_enum: STRIKE,
        ..shortcut(kind::SKILL)
    };
    shortcuts[1] = Shortcut {
        data_enum: SHOOT,
        ..shortcut(kind::SKILL)
    };
    shortcuts[2] = Shortcut {
        item: SWORD,
        ..shortcut(kind::ITEM)
    };
    shortcuts[3] = Shortcut {
        item: GONE,
        ..shortcut(kind::ITEM)
    };
    shortcuts[4] = Shortcut {
        data_enum: 0x4000_0010,
        ..shortcut(kind::EMOTE)
    };
    shortcuts[5] = Shortcut {
        data_id: RECIPE,
        ..shortcut(kind::RECIPE)
    };
    shortcuts[6] = shortcut(kind::RECIPE);
    shortcuts[7] = Shortcut {
        text: "hello".encode_utf16().collect(),
        ..shortcut(kind::TEXT)
    };
    let skill = |skill, trained| Skill {
        skill,
        trained,
        usable: true,
        waiting: None,
    };
    let item = |item, have| Item {
        item,
        have,
        here: Some(("A thing".into(), 0x0600_0001)),
    };
    Toolbar {
        version: 2,
        shortcuts,
        skills: vec![skill(STRIKE, true), skill(SHOOT, false)],
        items: vec![item(SWORD, true), item(GONE, false)],
        ..Toolbar::default()
    }
}

fn heard(notices: &[(u32, Vec<u32>)]) -> Checks {
    let mut c = Checks::default();
    for (native, words) in notices {
        c.hear(*native, words);
    }
    c
}

fn long(id: u64) -> [u32; 2] {
    [
        u32::try_from(id >> 32).expect("high"),
        u32::try_from(id & 0xFFFF_FFFF).expect("low"),
    ]
}

/// The clean-up empties a slot whose shortcut no longer belongs on the bar (an emote, a recipe
/// naming none, an item the player no longer has, a skill it no longer has trained) only when it
/// looks at the slot, as the bar looks again: every slot when the shortcuts or the options came
/// again; a skill's slots when it changed, every skill slot when the reset times did; an item's
/// slots or every item slot when the inventory changed (every skill slot too) or a container did;
/// a slot the scripts changed; an item that went from around the player. A recipe the scripts said
/// is gone is emptied whatever else; a line of text, the trained skill and the item the player has
/// always stay; nothing is looked at by itself at the first ask.
///
/// Behaviour: ui.shortcuts.the-clean-up-empties-what-no-longer-belongs
#[test]
fn the_clean_up_empties_what_no_longer_belongs_on_the_bar_when_it_looks() {
    let t = toolbar();
    let same = Some(&t);
    let empties =
        |notices: &[(u32, Vec<u32>)], before: Option<&Toolbar>| heard(notices).emptied(before, &t);
    assert_eq!(empties(&[], same), Vec::<u32>::new(), "nothing heard");
    assert_eq!(
        empties(&[], None),
        Vec::<u32>::new(),
        "the first ask looks at nothing by itself"
    );
    for reload in [
        notice::RELOAD_SHORTCUTS,
        notice::RELOAD_OPTIONS,
        notice::OPTIONS_CHANGED,
    ] {
        assert_eq!(
            empties(&[(reload, vec![])], same),
            [1, 3, 4, 6],
            "{reload:#x}"
        );
    }
    assert_eq!(
        empties(&[(notice::UPDATE_SKILL, vec![SHOOT])], same),
        [1],
        "the skill's slots"
    );
    assert_eq!(
        empties(&[(notice::UPDATE_SKILL, vec![STRIKE])], same),
        Vec::<u32>::new(),
        "a trained skill stays"
    );
    assert_eq!(empties(&[(notice::UPDATE_RESET_TIME, vec![])], same), [1]);
    let [hi, lo] = long(GONE);
    assert_eq!(
        empties(&[(notice::UPDATE_INVENTORY, vec![ONE, hi, lo])], same),
        [1, 3],
        "the item's slots, and every skill slot"
    );
    let [shi, slo] = long(SWORD);
    assert_eq!(
        empties(&[(notice::UPDATE_INVENTORY, vec![ONE, shi, slo])], same),
        [1],
        "another item's: only the skills"
    );
    assert_eq!(
        empties(&[(notice::UPDATE_CONTAINER, vec![0, 9, ALL])], same),
        [3],
        "a container: every item slot, no skill"
    );
    assert_eq!(
        empties(&[(notice::UPDATE_CONTAINER, vec![0, 9, ONE, hi, lo])], same),
        [3]
    );
    assert_eq!(
        empties(&[(notice::REMOVE_RECIPE, vec![RECIPE])], same),
        [5],
        "a recipe gone"
    );

    // Against the toolbar as it was: the slots the scripts changed, an item that went.
    let mut before = t.clone();
    before.shortcuts[4] = Shortcut::default();
    before.shortcuts[1] = Shortcut::default();
    assert_eq!(empties(&[], Some(&before)), [1, 4], "changed slots");
    let mut before = t.clone();
    let mut now = t.clone();
    now.items[1].here = None;
    now.items[0].here = None;
    assert_eq!(
        Checks::default().emptied(Some(&before), &now),
        [3],
        "the item that went and that the player no longer has"
    );
    before.items[1].here = None;
    assert_eq!(
        Checks::default().emptied(Some(&before), &now),
        Vec::<u32>::new(),
        "it was not here before"
    );
}
