//! The loot profile reader and matcher, over hand-built items.
//!
//! Behaviour: plugins.loot.profile-rules (the rules are tried in file order and the first whose
//! requirements all hold decides; an item needs an appraisal only when no earlier rule decides it
//! from what every client sees).

use dereth_client_model::{Weenie, World};
use dereth_primitives::ObjectId;
use dereth_protocol::archive::PackedHash;
use dereth_protocol::types::AppraisalProfile;
use dereth_rules::weenie::item_type;

use super::{Character, LootAction, Profile};

/// A rule: its name, its action code, and its requirements, each a type and its body's lines.
type RuleSpec<'a> = (&'a str, i32, &'a [(i32, &'a [&'a str])]);

/// A version-1 profile of `rules`, with the length of each requirement's body before it.
fn utl(rules: &[RuleSpec<'_>]) -> String {
    let mut out = format!("UTL\r\n1\r\n{}\r\n", rules.len());
    for (name, action, reqs) in rules {
        out.push_str(&format!("{name}\r\n\r\n{};{action}", 0));
        for (ty, _) in *reqs {
            out.push_str(&format!(";{ty}"));
        }
        out.push_str("\r\n");
        for (_, lines) in *reqs {
            let body: String = lines.iter().map(|l| format!("{l}\r\n")).collect();
            out.push_str(&format!("{}\r\n{body}", body.len()));
        }
    }
    out
}

fn item(w: &mut World, id: u32, name: &str, ty: u32, value: u32) -> ObjectId {
    let oid = ObjectId(id);
    let mut it = Weenie::new(oid);
    it.pwd.name = name.to_owned();
    it.pwd.obj_type = ty;
    it.pwd.value = Some(value);
    it.valid = true;
    w.tables.weenies.insert(oid, it);
    oid
}

fn appraise(w: &mut World, id: ObjectId, ints: &[(u32, i32)], spells: &[u32]) {
    let p = AppraisalProfile {
        success_flag: 1,
        spell_book: Some(spells.to_vec()),
        tables: dereth_protocol::types::PropertyTables {
            ints: Some(PackedHash {
                entries: ints.to_vec(),
                ..PackedHash::default()
            }),
            ..Default::default()
        },
        ..AppraisalProfile::default()
    };
    w.appraisal.set(id, p);
}

#[test]
fn the_first_rule_an_item_meets_decides_and_an_item_no_rule_meets_is_left() {
    let text = utl(&[
        // Value (19) at least 5000: keep.
        ("Valuable", 1, &[(3, &["5000", "19"])]),
        // Any melee weapon (class 1) with workmanship (105) at least 8: salvage.
        ("Good salvage", 2, &[(7, &["1"]), (3, &["8", "105"])]),
        // Any gem: keep up to 3.
        ("Gems", 10, &[(7, &["11"])]),
    ]);
    let text = text.replacen("Gems\r\n\r\n0;10;7\r\n", "Gems\r\n\r\n0;10;7\r\n3\r\n", 1);
    let p = Profile::read(&text).expect("the profile reads");
    assert_eq!(p.rules.len(), 3);
    let mut w = World::new();
    let ch = Character::default();
    let rich = item(&mut w, 1, "Gold Ring", item_type::JEWELRY, 9000);
    let sword = item(&mut w, 2, "Sword", item_type::MELEE_WEAPON, 100);
    let gem = item(&mut w, 3, "Opal", item_type::GEM, 10);
    let junk = item(&mut w, 4, "Rag", item_type::CLOTHING, 5);
    assert_eq!(
        p.classify(&w, &ch, rich).map(|r| r.action),
        Some(LootAction::Keep)
    );
    assert_eq!(
        p.classify(&w, &ch, gem).map(|r| r.action),
        Some(LootAction::KeepUpTo(3))
    );
    assert!(p.classify(&w, &ch, junk).is_none());
    // The sword's workmanship is known only from an appraisal.
    assert!(p.classify(&w, &ch, sword).is_none());
    assert!(p.needs_appraisal(&w, &ch, sword));
    appraise(&mut w, sword, &[(105, 9)], &[]);
    assert!(!p.needs_appraisal(&w, &ch, sword));
    assert_eq!(
        p.classify(&w, &ch, sword).map(|r| r.name.as_str()),
        Some("Good salvage")
    );
}

#[test]
fn an_item_decided_from_what_every_client_sees_is_never_appraised() {
    let p = Profile::read(&utl(&[
        ("Valuable", 1, &[(3, &["5000", "19"])]),
        ("Spells", 1, &[(0, &["Epic"])]),
    ]))
    .expect("reads");
    let mut w = World::new();
    let ch = Character::default();
    let rich = item(&mut w, 1, "Idol", item_type::MISC, 6000);
    let plain = item(&mut w, 2, "Stick", item_type::MISC, 1);
    assert!(!p.needs_appraisal(&w, &ch, rich), "the first rule decides");
    assert!(
        !p.needs_appraisal(&w, &ch, plain),
        "not magical, so no spell can match"
    );
    let mut magic = w.weenie(plain).cloned().expect("item");
    magic.pwd.effects = Some(1);
    w.tables.weenies.insert(plain, magic);
    assert!(
        p.needs_appraisal(&w, &ch, plain),
        "a magical item's spells need an appraisal"
    );
}

#[test]
fn spell_and_character_requirements_read_the_named_spells_and_the_character() {
    let p = Profile::read(&utl(&[
        ("Two majors", 1, &[(9, &["^Major", "Armor", "2"])]),
        ("High level only", 3, &[(1002, &["100"]), (7, &["11"])]),
    ]))
    .expect("reads");
    let mut w = World::new();
    let wand = item(&mut w, 1, "Wand", item_type::CASTER, 1);
    appraise(&mut w, wand, &[], &[10, 11, 12]);
    let mut ch = Character::default();
    ch.spell_names.insert(10, "Major Strength".into());
    ch.spell_names.insert(11, "Major Armor".into());
    ch.spell_names.insert(12, "Major Focus".into());
    assert!(
        p.classify(&w, &ch, wand).is_some(),
        "two majors that are not Armor"
    );
    ch.spell_names.insert(12, "Minor Focus".into());
    assert!(p.classify(&w, &ch, wand).is_none());
    let gem = item(&mut w, 2, "Opal", item_type::GEM, 1);
    assert!(p.classify(&w, &ch, gem).is_none());
    ch.level = 120;
    assert_eq!(
        p.classify(&w, &ch, gem).map(|r| r.action),
        Some(LootAction::Sell)
    );
}

#[test]
fn a_version_zero_profile_and_an_unknown_requirement_read_and_a_broken_one_does_not() {
    let v0 = "1\r\nOld\r\n0;1;3\r\n100\r\n19\r\n";
    let p = Profile::read(v0).expect("version 0 reads");
    assert_eq!(p.rules[0].name, "Old");
    // A requirement type the reader does not know is skipped by its length and never matches.
    let text = utl(&[("Future", 1, &[(4242, &["x", "y"])])]);
    let p = Profile::read(&text).expect("an unknown requirement is skipped");
    let mut w = World::new();
    let any = item(&mut w, 1, "Thing", item_type::MISC, 1);
    assert!(p.classify(&w, &Character::default(), any).is_none());
    assert!(
        Profile::read("UTL\r\n2\r\n0\r\n").is_err(),
        "a newer file version"
    );
    assert!(Profile::read("rules\r\n").is_err());
}

#[test]
fn a_profile_whose_line_ends_were_changed_reads_the_same() {
    // An unknown requirement among them is skipped by the same lines either way.
    let crlf = utl(&[
        ("Valuable", 1, &[(3, &["5000", "19"]), (4242, &["10"])]),
        ("Gems", 2, &[(7, &["11"]), (0, &["Major"])]),
    ]);
    let lf = crlf.replace("\r\n", "\n");
    let a = Profile::read(&crlf).expect("reads");
    let b = Profile::read(&lf).expect("reads with bare line feeds");
    let names = |p: &Profile| {
        p.rules
            .iter()
            .map(|r| (r.name.clone(), r.action, r.reqs.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&a), names(&b));
}

/// Each rule's requirements read back in plain words, in the profile's order, for a window that
/// shows the profile: a value, a class, a spell's name, the character's level; a rule switched
/// on in the editor adds nothing, and a colour, which is not read, says so.
#[test]
fn a_rules_requirements_read_back_in_plain_words() {
    let text = utl(&[
        ("Valuables", 1, &[(3, &["2500", "19"])]),
        ("Gems", 1, &[(7, &["11"]), (9999, &["false"])]),
        (
            "Majors",
            1,
            &[
                (0, &["Major"]),
                (1002, &["50"]),
                (14, &["1", "2", "3", "4", "5"]),
            ],
        ),
    ]);
    let p = Profile::read(&text).expect("the profile reads");
    let words: Vec<Vec<String>> = p.rules.iter().map(super::Rule::conditions).collect();
    assert_eq!(
        words,
        [
            vec!["its value at least 2500".to_owned()],
            vec!["its class is gem".to_owned()],
            vec![
                "a spell whose name matches \"Major\"".to_owned(),
                "the character's level at least 50".to_owned(),
                "a colour (not read by this client: never matches)".to_owned(),
            ],
        ]
    );
}

/// A profile's salvage combining block, after its rules, reads as the default ranges and each
/// material's own (a single workmanship standing for itself); a profile with an empty block has
/// no ranges, and one with none has no salvage combining.
#[test]
fn a_profiles_salvage_combine_ranges_read_after_its_rules() {
    let rules = utl(&[("Salvage", 2, &[(7, &["1"])])]);
    let body = "1\r\n1-9,10\r\n2\r\n64\r\n1-6, 7,8,9,10\r\n57\r\n3-1\r\n";
    let text = format!("{rules}SalvageCombine\r\n{}\r\n{body}", body.len());
    let p = Profile::read(&text).expect("the profile reads");
    assert_eq!(p.rules.len(), 1);
    let s = p.salvage_combine.expect("a salvage combining block");
    assert_eq!(s.default, [(1, 9), (10, 10)]);
    assert_eq!(
        s.materials,
        [
            (64, vec![(1, 6), (7, 7), (8, 8), (9, 9), (10, 10)]),
            (57, vec![(1, 3)]),
        ]
    );
    let empty = format!("{rules}SalvageCombine\r\n8\r\n1\r\n\r\n0\r\n");
    let s = Profile::read(&empty)
        .expect("reads")
        .salvage_combine
        .expect("a block");
    assert!(s.default.is_empty() && s.materials.is_empty());
    assert!(Profile::read(&rules)
        .expect("reads")
        .salvage_combine
        .is_none());
}
