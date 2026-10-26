//! The marks' glyphs and the world's labels: the arrow's lit point, a crowd of labels kept
//! readable, and a label shown or hidden whole by the depth at its anchor.

#[test]
fn the_arrow_lights_the_point_toward_its_target() {
    let lit = |turn: f64| {
        super::arrow_glyphs(100, 100, 18, turn, 0xFF_D7_00)
            .into_iter()
            .find_map(|g| match g {
                super::Glyph::Dot { x, y, size: 5, .. } => Some((x, y)),
                _ => None,
            })
    };
    assert_eq!(lit(0.0), Some((100, 82)), "ahead is up");
    assert_eq!(lit(90.0), Some((118, 100)), "to the right");
}

#[test]
fn a_crowd_of_labels_keeps_the_nearest_whole_and_fades_or_drops_the_rest() {
    let spot = |x: i32, text: &str, distance: f64, rank: u8| super::LabelSpot {
        x,
        y: 100,
        text: text.into(),
        distance,
        rank,
    };
    let spots = vec![
        spot(100, "Drudge Skulker", 30.0, 1),
        spot(102, "Drudge Slinker", 10.0, 1),
        spot(175, "Tirenia", 40.0, 1),
        spot(300, "Alcott", 50.0, 1),
        spot(104, "Camp", 60.0, 2),
    ];
    let shown = super::declutter(&spots);
    let alpha = |i: usize| shown.iter().find(|(j, _)| *j == i).map(|(_, a)| *a);
    assert_eq!(
        alpha(4),
        Some(0xFF),
        "a plugin's own text goes before nametags"
    );
    assert_eq!(
        alpha(1),
        Some(0x80),
        "the nearest name clips that text: half strength"
    );
    assert_eq!(
        alpha(0),
        None,
        "a farther name mostly under nearer ones is left out"
    );
    assert_eq!(
        alpha(2),
        Some(0x80),
        "a name only clipping another is still shown"
    );
    assert_eq!(
        alpha(3),
        Some(0xFF),
        "a name clear of the others is drawn whole"
    );
    assert_eq!(
        shown.last().map(|(i, _)| *i),
        Some(4),
        "drawn farthest first, so the first placed is drawn last, over the rest"
    );
}

#[test]
fn a_label_is_shown_or_hidden_whole_by_the_depth_at_its_anchor() {
    let mut o = super::LabelOcclusion::default();
    assert!(!o.visible("Alcott"), "no answer yet: not shown");
    let first = o.ask(vec![("Alcott".into(), 0.5), ("Tirenia".into(), 0.9)]);
    let second = o.ask(vec![("Alcott".into(), 0.5)]);
    // The world at Alcott's anchor is farther than the anchor; at Tirenia's, a wall is nearer.
    o.answer(first, &[0.8, 0.6]);
    assert!(o.visible("Alcott"), "nothing nearer at the anchor: shown");
    assert!(
        !o.visible("Tirenia"),
        "something nearer at the anchor: hidden"
    );
    o.answer(second, &[0.4]);
    assert!(!o.visible("Alcott"), "the newest answer decides");
    assert!(
        !o.visible("Tirenia"),
        "a label the newest answer did not cover is not shown"
    );
    o.answer(first, &[1.0, 1.0]);
    assert!(
        !o.visible("Alcott"),
        "an answer to a request already passed over is ignored"
    );
}
