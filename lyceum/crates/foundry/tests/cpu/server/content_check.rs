//! Behaviour: none (tooling: the world editor's checks of a pack's server content)

use std::collections::BTreeSet;
use std::path::Path;

use lyceum_foundry::content::Schema;
use lyceum_foundry::content_check::{check, quest_key, unused_quests, Known};
use lyceum_foundry::Loc;

/// What the content may name: the pack's `guide`, the world's class 7 and quest `CowMilked`,
/// spell 1 in the table, and the server's names for int 1 and bool 1.
fn known() -> Known {
    Known {
        pack_classes: ["p:guide".to_owned()].into_iter().collect(),
        world_classes: Some([7].into_iter().collect()),
        quests: [quest_key("CowMilked")].into_iter().collect(),
        world_quests: true,
        spells: Some([1].into_iter().collect()),
        schema: Some(
            Schema::read(
                r#"{"properties": {"int": {"1": "ItemType"}, "bool": {"1": "Stuck"}}, "enums": {}}"#,
            )
            .unwrap(),
        ),
    }
}

/// The findings of `text`'s checks, as words, and the quests it uses.
fn findings(text: &str) -> (Vec<(bool, String)>, BTreeSet<String>) {
    let (mut used, mut out) = (BTreeSet::new(), Vec::new());
    check(Path::new("w.sql"), "p", text, &known(), &mut used, &mut out);
    (out.into_iter().map(|f| (f.warning, f.text)).collect(), used)
}

fn quiet(text: &str) {
    let (f, _) = findings(text);
    assert!(f.is_empty(), "{f:#?}\n{text}");
}

fn one(text: &str, warning: bool, words: &str) {
    let (f, _) = findings(text);
    assert_eq!(f.len(), 1, "{f:#?}\n{text}");
    assert_eq!(f[0].0, warning, "{f:#?}");
    assert!(f[0].1.contains(words), "{f:#?}");
}

#[test]
fn a_property_of_its_type_that_the_server_names_is_quiet_and_any_other_is_a_fault() {
    quiet("INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\nVALUES (5, 1, 16);");
    quiet(
        "INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)\nVALUES (5, 1, True);",
    );
    one(
        "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\nVALUES (5, 1, 'x');",
        false,
        "is not of its type",
    );
    one(
        "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\nVALUES (5, 777, 1);",
        false,
        "property 777 is not one the server has",
    );
}

#[test]
fn a_class_a_pack_or_the_world_has_is_quiet_and_any_other_is_a_fault() {
    for (table, what) in [
        ("weenie_properties_create_list", "a create-list row"),
        ("weenie_properties_generator", "a generator profile"),
        ("weenie_properties_emote_action", "an action"),
    ] {
        for class in ["7", "{{wcid:guide}}", "0", "NULL"] {
            quiet(&format!(
                "INSERT INTO `{table}` (`object_Id`, `weenie_Class_Id`)\nVALUES (5, {class});"
            ));
        }
        for class in ["8", "{{wcid:nobody}}"] {
            one(
                &format!(
                    "INSERT INTO `{table}` (`object_Id`, `weenie_Class_Id`)\nVALUES (5, {class});"
                ),
                false,
                &format!("{what} names class {class}, which no pack defines"),
            );
        }
    }
}

#[test]
fn a_spell_the_table_has_is_quiet_and_any_other_is_a_fault() {
    quiet("INSERT INTO `weenie_properties_spell_book` (`object_Id`, `spell`, `probability`)\nVALUES (5, 1, 2);");
    one(
        "INSERT INTO `weenie_properties_spell_book` (`object_Id`, `spell`, `probability`)\nVALUES (5, 99999, 2);",
        false,
        "spell 99999 is not in the spell table",
    );
    quiet("INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `spell_Id`)\nVALUES (@parent_id, 0, 19, 1);");
    one(
        "INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `spell_Id`)\nVALUES (@parent_id, 0, 19, 99999);",
        false,
        "names spell 99999",
    );
}

#[test]
fn a_quest_action_on_a_registered_quest_is_quiet_and_on_any_other_is_a_warning() {
    let action = |ty: u32, quest: &str| {
        format!(
            "INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `message`)\nVALUES (@parent_id, 0, {ty}, '{quest}');"
        )
    };
    // A stamp's own suffix is not part of the name; case is not either.
    for q in ["CowMilked", "cowmilked", "CowMilked@2"] {
        let (f, used) = findings(&action(22, q));
        assert!(f.is_empty(), "{f:#?}");
        assert_eq!(used, ["cowmilked".to_owned()].into_iter().collect());
    }
    one(
        &action(22, "CowKicked"),
        true,
        "names quest \"CowKicked\", which is not registered",
    );
    // An action that is not about quests says nothing of its message.
    quiet(&action(10, "CowKicked"));
}

#[test]
fn a_quest_registered_and_never_used_is_a_warning() {
    let at = Loc::new(Path::new("q.sql"), 3);
    let registered = vec![
        (at.clone(), "CowMilked".to_owned()),
        (at, "CowKicked".to_owned()),
    ];
    let used: BTreeSet<String> = [quest_key("CowMilked")].into_iter().collect();
    let w = unused_quests(&registered, &used);
    assert_eq!(w.len(), 1);
    assert!(
        w[0].warning
            && w[0]
                .text
                .contains("\"CowKicked\" is registered and nothing uses it")
    );
}
