use super::*;
use crate::DisplayVariant::{Classic, Modern};
use dereth_client_contract::view::{AppraisalSpellView, ItemLevelView, WieldRequirementView};

fn runs(p: &AppraisalView, variant: crate::DisplayVariant) -> Vec<(String, bool, u8)> {
    item_description_runs_for(p, variant)
        .into_iter()
        .map(|r| (r.text, r.same_line, r.color))
        .collect()
}

/// Behaviour: appraisal.presentation.variants-preserve-order-and-world-facts
#[test]
fn variants_keep_ordered_runs_and_later_world_facts() {
    let mut p = AppraisalView {
        success: true,
        long_desc: Some("An item.".into()),
        ..Default::default()
    };
    p.special.imbued = Some(0xa000_4000);
    p.special.absorb_magic_damage = true;
    assert_eq!(
        runs(&p, Classic),
        vec![
            ("An item.".into(), false, 0),
            (
                "Special Properties: Nether Rending, Magic Absorbing, Phantasmal".into(),
                true,
                0
            ),
            ("This item cannot be further imbued.".into(), true, 0),
            ("".into(), true, 0),
        ]
    );
    assert_eq!(
        runs(&p, Modern),
        vec![
            ("Value: ???".into(), true, 0),
            ("Burden: Unknown".into(), true, 0),
            ("".into(), true, 0),
            ("".into(), true, 0),
            (
                "Properties: Nether Rending, Phantasmal, Magic Absorbing".into(),
                true,
                0
            ),
            ("This item cannot be further imbued.".into(), true, 0),
            ("".into(), true, 0),
            ("An item.".into(), false, 0),
        ]
    );
    p.wield_requirements = vec![
        WieldRequirementView {
            requirement: 7,
            difficulty: 100,
            subject: Some("level".into()),
        },
        WieldRequirementView {
            requirement: 1,
            difficulty: 200,
            subject: Some("Heavy Weapons".into()),
        },
    ];
    p.item_level = Some(ItemLevelView {
        total_xp: u64::MAX,
        base_xp: 1,
        max_level: 1,
        xp_style: 1,
    });
    for variant in [Classic, Modern] {
        let text = flatten_item_info(&item_description_runs_for(&p, variant));
        assert!(text.contains("Heavy Weapons"), "later requirement: {text}");
        assert!(text.contains("level"), "first requirement: {text}");
        assert!(
            text.contains("Item XP: 18,446,744,073,709,551,615 /"),
            "exact XP: {text}"
        );
    }
    p.special = Default::default();
    for (attack, classic, modern) in [
        (0x20, true, true),
        (0x800, false, true),
        (0x20000, false, false),
    ] {
        p.attack_type = Some(attack);
        for (variant, expected) in [(Classic, classic), (Modern, modern)] {
            assert_eq!(
                item_description_runs_for(&p, variant)
                    .iter()
                    .any(|r| r.text.contains("Multi-Strike")),
                expected
            );
        }
    }
    p.num_character_titles = Some(4);
    p.enlightenment = Some(2);
    let rows = char_misc_rows_for(&p, "Aerin", Classic);
    assert!(rows
        .iter()
        .any(|r| r.label == "Titles Earned:" && r.value == "4"));
    assert!(rows
        .iter()
        .any(|r| r.label == "Enlightenment:" && r.value == "2"));
}

/// Behaviour: appraisal.presentation.variants-preserve-order-and-world-facts
#[test]
fn armor_presence_and_level_control_both_classic_headings() {
    let mut p = AppraisalView {
        valid_locations: 1,
        ..Default::default()
    };
    p.enchantment_mods.insert(0x1c, true);
    for (level, profile, expected) in [
        (None, false, 1),
        (Some(0), true, 0),
        (Some(-1), true, 0),
        (Some(10), false, 1),
        (Some(10), true, 2),
    ] {
        p.armor_level = level;
        p.armor_mods = profile.then_some([1.0; 8]);
        let headings: Vec<_> = item_description_runs_for(&p, Classic)
            .into_iter()
            .filter(|r| r.text.contains("Armor Level:"))
            .map(|r| (r.text, r.color))
            .collect();
        assert_eq!(headings.len(), expected, "{level:?}/{profile}");
        if level == Some(10) && profile {
            assert_eq!(
                headings,
                [
                    ("Armor Level:  10".into(), 1),
                    ("\nArmor Level: 10".into(), 1)
                ]
            );
        }
    }
    p.valid_locations = equip::SHIELD;
    let rows = item_description_runs_for(&p, Classic);
    assert!(rows
        .iter()
        .any(|r| r.text == "Shield Level: 10" && r.color == 1));
    assert!(rows
        .iter()
        .any(|r| r.text == "\nArmor Level: 10" && r.color == 1));
}

/// Behaviour: appraisal.presentation.variants-preserve-order-and-world-facts
#[test]
fn magic_requirements_follow_resolved_normal_spells() {
    let mut p = AppraisalView {
        success: true,
        ..Default::default()
    };
    p.magic.spells = Some(vec![AppraisalSpellView {
        resolved: true,
        name: "Ward".into(),
        description: "Protects.".into(),
        enchantment: false,
        ..Default::default()
    }]);
    assert_eq!(
        runs(&p, Classic),
        vec![
            ("Casts the following spells: Ward".into(), false, 0),
            ("Activation Requirements: ".into(), false, 0),
            (
                "Spell Descriptions:\n\n     Ward (Protects.)".into(),
                false,
                0
            ),
        ]
    );
    p.activation_heritage = Some("Aluvian".into());
    p.activation_skill = Some(("Arcane Lore".into(), 50));
    p.activation_attribute = Some(("Focus".into(), 100));
    assert!(item_description_runs_for(&p, Classic)
        .iter()
        .any(|r| r.text == "Activation Requirements: Aluvian, Arcane Lore: 50, Focus: 100"));
    p.magic.spells.as_mut().unwrap()[0].enchantment = true;
    assert_eq!(
        runs(&p, Classic),
        vec![("Enchantments:\n\n     Ward (Protects.)".into(), false, 0)]
    );
    p.success = false;
    assert_eq!(
        runs(&p, Classic),
        vec![
            ("Spells: unknown.".into(), false, 0),
            ("Spells: unknown.".into(), false, 0)
        ]
    );
    p.success = true;
    p.magic.spells = Some(vec![AppraisalSpellView {
        raw_id: 999_999,
        ..Default::default()
    }]);
    assert!(runs(&p, Classic).is_empty());
    assert!(runs(&p, Modern)
        .iter()
        .any(|r| r.0 == "Spell Descriptions:\n~ : "));
    p.magic.spells.as_mut().unwrap()[0].resolved = true;
    assert!(
        runs(&p, Classic)
            .iter()
            .any(|r| r.0.starts_with("Activation Requirements:")),
        "an authored empty name still resolves"
    );
    p.magic.spells = None;
    assert!(runs(&p, Classic).is_empty());
}
