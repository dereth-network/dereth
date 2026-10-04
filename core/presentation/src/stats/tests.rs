use super::*;
fn skill(id: u32, name: &str, sac: u32, min_level: u32, effective: i32) -> SkillEntry {
    SkillEntry {
        id,
        name: name.into(),
        sac,
        min_level,
        level: 10,
        effective,
        vitae: 0,
        icon: None,
    }
}
/// Behaviour: stats.presentation.preserves-display-variants-and-fractional-vitae
#[test]
fn fractional_vitae_uses_variant_rounding_without_narrowing() {
    for (m, classic, modern) in [
        (1.0, 0, 0),
        (0.999, 1, 0),
        (0.986, 1, 1),
        (0.984, 2, 2),
        (0.995, 1, 0),
        (0.985, 1, 1),
    ] {
        let d = VitaeDisplay {
            multiplier: m,
            cp_pool: 12,
            threshold: 10,
        };
        assert_eq!(
            vitae_content(d, DisplayVariant::Classic),
            VitaeContent {
                penalty: classic,
                experience: -2
            }
        );
        assert_eq!(vitae_content(d, DisplayVariant::Modern).penalty, modern);
    }
    for (m, expected) in [
        (f32::from_bits(0.985f32.to_bits() - 1), 2),
        (f32::from_bits(0.985f32.to_bits() + 1), 1),
    ] {
        let d = VitaeDisplay {
            multiplier: m,
            cp_pool: 0,
            threshold: 1,
        };
        for v in [DisplayVariant::Classic, DisplayVariant::Modern] {
            assert_eq!(vitae_content(d, v).penalty, expected);
        }
    }
    assert!(VitaeContent {
        penalty: 1,
        experience: 1
    }
    .classic_text()
    .ends_with("1 more experience point.\n"));
    assert!(VitaeContent {
        penalty: 1,
        experience: -2
    }
    .classic_text()
    .ends_with("-2 more experience points.\n"));
}
/// Behaviour: stats.presentation.preserves-display-variants-and-fractional-vitae
#[test]
fn skill_membership_uses_the_selected_convention_and_current_value() {
    let mut skills = vec![
        skill(1, "beta", 1, 0, 0),
        skill(2, "Alpha", 1, 2, 12),
        skill(3, "alpha", 0, 0, 0),
        skill(4, "Later", 3, 9, 0),
        skill(5, "A", 2, 9, 0),
    ];
    let ids = |v| {
        skill_groups(&skills, v)
            .into_iter()
            .map(|(g, rows)| (g, rows.into_iter().map(|s| s.id).collect::<Vec<_>>()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(DisplayVariant::Classic)
            .into_iter()
            .map(|r| r.1)
            .collect::<Vec<_>>(),
        vec![vec![4], vec![5], vec![2], vec![1]]
    );
    assert_eq!(
        ids(DisplayVariant::Modern)
            .into_iter()
            .map(|r| r.1)
            .collect::<Vec<_>>(),
        vec![vec![4], vec![5], vec![1], vec![2, 3]]
    );
    skills[0].effective = 1;
    assert_eq!(
        skill_groups(&skills, DisplayVariant::Classic)[2]
            .1
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert_eq!(value_font(98, 100, -5), 1);
    assert_eq!(value_font(95, 100, -5), 0);
    assert_eq!(value_font(75, 100, -5), 2);
}
/// Behaviour: stats.presentation.preserves-display-variants-and-fractional-vitae
#[test]
fn titles_filter_missing_names_and_sort_stably_by_case_sensitive_utf16() {
    let rows = title_rows(&[
        (0, "zero".into()),
        (1, "beta".into()),
        (2, "Alpha".into()),
        (3, "Alpha".into()),
        (4, "".into()),
        (5, "alpha".into()),
        (6, "\u{10000}".into()),
        (7, "\u{e000}".into()),
    ]);
    assert_eq!(
        rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![2, 3, 5, 1, 6, 7]
    );
    assert!(!can_set_title(None, 2, &rows));
    assert!(!can_set_title(Some(2), 2, &rows));
    assert!(!can_set_title(Some(9), 2, &rows));
    assert!(can_set_title(Some(3), 2, &rows));
}
/// Behaviour: stats.presentation.preserves-display-variants-and-fractional-vitae
#[test]
fn headers_preserve_luminance_gates_precision_and_raise_affordability() {
    let mut h = HeaderInputs {
        xp: Some(XpHeader {
            level: 200,
            ..Default::default()
        }),
        luminance: (i64::MAX, i64::MAX),
        ..Default::default()
    };
    assert_eq!(
        h.luminance_line().1,
        "9,223,372,036,854,775,807 / 9,223,372,036,854,775,807"
    );
    h.xp.as_mut().unwrap().level = 199;
    assert!(h.luminance_line().0.is_empty());
    h.xp.as_mut().unwrap().level = 200;
    h.luminance.1 = 0;
    assert!(h.luminance_line().0.is_empty());
    assert!(!can_raise(0, 100));
    assert!(!can_raise(11, 10));
    assert!(can_raise(10, 10));
    assert_eq!(meter_level(1, 0), 0.0);
    assert_eq!(meter_level(1, 2), 0.5);
    assert_eq!(classic_meter_ratio(2, 1), 1.0);
}
