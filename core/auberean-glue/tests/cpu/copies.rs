//! Copies of the game's native values: each a copy of every part, owned apart from the original.

use auberean_script::native_object::{EffectNode, NativeObject, QuestInfo, TextInfo};
use auberean_script::options::Shortcut;
use auberean_script::story::{ActInfo, Keyframe, SceneInfo};

/// A copy of a quest, a list of quests, an effect, a shortcut, a keyframe, a scene and an act
/// holds every part of the original, and changing the original afterwards leaves it as it was;
/// a new adaptor of the social and shop kinds holds 0 in every word.
///
/// Behaviour: script.glue.a-copy-of-a-game-value-is-its-own
#[test]
fn a_copy_of_a_game_value_holds_every_part_and_is_its_own() {
    let text = |id: u32| TextInfo {
        string_id: id,
        table: 0x2300_0001,
        ..TextInfo::default()
    };
    let mut quest = QuestInfo {
        id: 7,
        name: text(1),
        journal: text(2),
        phase: 3,
        bestowed: 1.5f64.to_bits(),
        ..QuestInfo::default()
    };
    let copy = quest.clone();
    quest.name = text(9);
    quest.phase = 4;
    assert_eq!((copy.id, copy.phase, copy.name.string_id), (7, 3, 1));
    assert_eq!(copy.journal.string_id, 2);
    let mut list = NativeObject::QuestList(vec![copy.clone(), quest.clone()]);
    let listed = list.clone();
    if let NativeObject::QuestList(q) = &mut list {
        q.clear();
    }
    assert_eq!(listed, NativeObject::QuestList(vec![copy, quest]));

    let mut effect = EffectNode {
        eid: 5,
        name: "Haste".encode_utf16().collect(),
        ..EffectNode::default()
    };
    let e = effect.clone();
    effect.name.clear();
    assert_eq!(e.eid, 5);
    assert_eq!(String::from_utf16(&e.name).unwrap(), "Haste");

    let mut shortcut = Shortcut {
        kind: 0x4000_0003,
        text: "/wave".encode_utf16().collect(),
        ..Shortcut::default()
    };
    let s = shortcut.clone();
    shortcut.text.clear();
    assert_eq!(s.kind, 0x4000_0003);
    assert_eq!(String::from_utf16(&s.text).unwrap(), "/wave");

    let key = Keyframe {
        scene: 2,
        number: 1,
        subtitle: text(4),
        ..Keyframe::default()
    };
    let mut scene = SceneInfo {
        id: 2,
        name: text(5),
        keyframes: vec![key.clone()],
        ..SceneInfo::default()
    };
    let mut act = ActInfo {
        id: 1,
        scenes: vec![scene.clone()],
        ..ActInfo::default()
    };
    let (scene_copy, act_copy) = (scene.clone(), act.clone());
    scene.keyframes.clear();
    act.scenes[0].keyframes.clear();
    assert_eq!(scene_copy.keyframes, vec![key.clone()]);
    assert_eq!(act_copy.scenes[0].keyframes, vec![key]);

    // Allegiance members and profiles, fellows and fellowships, shops and their sales.
    for ty in [0x82, 0x83, 0x93, 0x94, 0x1AD6, 0x1AD7] {
        for field in ["ID", "Level", "Ordinal", "Status", "StockQuantity"] {
            assert_eq!(auberean_glue::adaptors::new_word(ty, field), 0);
        }
    }
}
