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
        loot_tables: Some([4, 37].into_iter().collect()),
        loot_symbols: ["p:rich".to_owned()].into_iter().collect(),
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
fn a_loot_table_the_packs_or_the_world_have_is_quiet_and_any_other_is_a_warning() {
    let generator = |class: u32, place: u32| {
        format!(
            "INSERT INTO `weenie_properties_generator` (`object_Id`, `weenie_Class_Id`, `where_Create`)
VALUES (5, {class}, {place});"
        )
    };
    // A treasure entry names a loot table, not a class.
    quiet(&generator(4, 72));
    one(
        &generator(9, 72),
        true,
        "a generator's treasure entry names loot table 9, which no pack has",
    );
    one(&generator(9, 2), false, "a generator profile names class 9");
    // A pack's own table by its symbol: one a pack holds.
    let token = |name: &str| {
        format!(
            "INSERT INTO `weenie_properties_generator` (`object_Id`, `weenie_Class_Id`, `where_Create`)
VALUES (5, {{{{treasure:{name}}}}}, 72);"
        )
    };
    quiet(&token("rich"));
    one(
        &token("poor"),
        true,
        "names loot table p:poor, which no pack has",
    );
    let did = |ty: u32, value: u32| {
        format!(
            "INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (5, {ty}, {value});"
        )
    };
    // Without the server's names, so its data ids are not looked up.
    let found = |text: &str| {
        let known = Known {
            schema: None,
            ..known()
        };
        let (mut used, mut out) = (BTreeSet::new(), Vec::new());
        check(Path::new("w.sql"), "p", text, &known, &mut used, &mut out);
        out.into_iter()
            .map(|f| (f.warning, f.text))
            .collect::<Vec<_>>()
    };
    assert!(found(&did(35, 4)).is_empty());
    assert!(found(&did(32, 37)).is_empty());
    let five = found(&did(35, 5));
    assert!(
        five.len() == 1 && five[0].0 && five[0].1.contains("its death treasure names loot table 5"),
        "{five:?}"
    );
    let other = found(&did(32, 38));
    assert!(
        other.len() == 1
            && other[0]
                .1
                .contains("its wielded treasure names loot table 38"),
        "{other:?}"
    );
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
    // Its type written as an export writes it, its name in a comment after it.
    one(
        &action(22, "CowKicked").replace(", 22, ", ", 22 /* StampQuest */, "),
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

/// The findings of a weenie of weenie type `ty` (class 9) with `rows` after its `weenie` row: each
/// warning or fault, its words and its line.
fn weenie_findings(ty: u32, rows: &str) -> Vec<(bool, String, usize)> {
    let text = format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (9, 'thing', {ty}, '2005-02-09 10:00:00');\n{rows}"
    );
    let known = Known {
        schema: None,
        ..known()
    };
    let (mut used, mut out) = (BTreeSet::new(), Vec::new());
    check(Path::new("w.sql"), "p", &text, &known, &mut used, &mut out);
    out.into_iter()
        .map(|f| (f.warning, f.text, f.at.line))
        .collect()
}

/// Int property rows of class 9.
fn ints(rows: &[(u32, i64)]) -> String {
    let rows: Vec<String> = rows.iter().map(|(t, v)| format!("(9, {t}, {v})")).collect();
    format!(
        "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)\nVALUES {};\n",
        rows.join("\n     , ")
    )
}

#[test]
fn a_generator_making_more_at_the_start_than_it_keeps_at_most_is_warned_of_on_its_starts_line() {
    let f = weenie_findings(1, &ints(&[(81, 3), (82, 5)]));
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(f[0].0 && f[0].1.contains("makes 5 at the start"), "{f:#?}");
    assert_eq!(f[0].2, 5, "the start's own row");
    for quiet in [
        ints(&[(81, 5), (82, 5)]),
        ints(&[(81, 5), (82, 1)]),
        ints(&[(82, 5)]),
    ] {
        let f = weenie_findings(1, &quiet);
        assert!(f.is_empty(), "{f:#?}");
    }
}

#[test]
fn a_portal_with_no_destination_is_warned_of_and_one_with_any_destination_is_quiet() {
    let f = weenie_findings(7, "");
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(f[0].0 && f[0].1.contains("no destination"), "{f:#?}");
    assert_eq!(f[0].2, 2, "the weenie's own row");
    // A destination, or one set from where it stands, is quiet; another kind needs none.
    for ty in [2, 26] {
        let f = weenie_findings(
            7,
            &format!(
                "INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)\nVALUES (9, {ty}, 0xA9B40001, 1, 1, 1, 1, 0, 0, 0);\n"
            ),
        );
        assert!(f.is_empty(), "{f:#?}");
    }
    assert!(weenie_findings(10, "").is_empty());
}

#[test]
fn a_wield_requirement_with_no_minimum_is_warned_of_but_one_that_names_what_it_must_be_is_not() {
    // A level of at least nothing, and a second requirement of a skill of at least 0.
    let f = weenie_findings(6, &ints(&[(158, 7), (270, 1), (272, 0)]));
    assert_eq!(f.len(), 2, "{f:#?}");
    assert!(f.iter().all(|(w, t, _)| *w && t.contains("no minimum")));
    assert_eq!((f[0].2, f[1].2), (4, 5));
    let quiet = [
        ints(&[(158, 7), (160, 30)]),
        // A creature kind or heritage of 0 is one a player must be, not a minimum.
        ints(&[(158, 11), (159, 0)]),
        ints(&[(158, 12)]),
        ints(&[(158, 0)]),
    ];
    for q in quiet {
        let f = weenie_findings(6, &q);
        assert!(f.is_empty(), "{f:#?}");
    }
}

#[test]
fn a_creatures_set_drawn_by_chance_past_100_percent_is_warned_of_at_the_entry_that_passes_it() {
    let creates = |rows: &[(u32, f64)]| {
        let rows: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, (d, shade))| format!("(9, {d}, {}, 1, 0, {shade}, False)", i + 100))
            .collect();
        format!(
            "INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)\nVALUES {};\n",
            rows.join("\n     , ")
        )
    };
    // A wield set of 60% and 60%: the second is made 40% of the time.
    let f: Vec<_> = weenie_findings(10, &creates(&[(2, 0.0), (10, 0.6), (10, 0.6), (10, 1.0)]))
        .into_iter()
        .filter(|(w, ..)| *w)
        .collect();
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(
        f[0].1.contains("adds up to 120%") && f[0].1.contains("made 40% of the time, not 60%"),
        "{f:#?}"
    );
    assert_eq!(f[0].2, 6, "the second entry's row");
    // Sets that fill 100% in tenths, or fall short of it, are quiet; and what is not a creature's
    // is made whole, not drawn.
    let quiet = creates(&[(9, 0.1); 10]);
    let f: Vec<_> = weenie_findings(10, &quiet)
        .into_iter()
        .filter(|(w, ..)| *w)
        .collect();
    assert!(f.is_empty(), "{f:#?}");
    let f: Vec<_> = weenie_findings(20, &creates(&[(9, 0.7), (9, 0.7)]))
        .into_iter()
        .filter(|(w, ..)| *w)
        .collect();
    assert!(f.is_empty(), "{f:#?}");
}
