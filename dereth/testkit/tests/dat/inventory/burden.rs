use dereth_testkit::HeadlessClient;
use dereth_ui::widgets::meter::{Meter, CHILD_IMAGE};
use dereth_ui::{ElemHandle, UiSystem};
use dereth_ui_screens::panels::inventory::load_text;

/// The glyphs an element composed.
fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// The character's own three inputs, read as **raw properties** off the recorded description
/// -- deliberately not through the burden module the panel uses, so the two arithmetics
/// cannot share a mistake. The strength is the enchanted one, which is what the client asks
/// for.
fn inputs(c: &HeadlessClient) -> (i32, i32, i32) {
    let q = c
        .view()
        .world()
        .player_qualities()
        .expect("the premise: the recording carries the character's own description");
    let strength = dereth_rules::attributes::inq_attribute(q, 1, false)
        .map_or(10, |s| i32::try_from(s).unwrap_or(i32::MAX));
    (strength, q.inq_int(5), q.inq_int(230))
}

/// The capacity rule, the load rule and the bar's own scaling, written out here from the
/// client's own arithmetic so that the expectation comes from the recording's numbers and not
/// from the code under test.
fn expected(strength: i32, encumbrance: i32, augs: i32) -> (f32, i32) {
    let capacity = if strength < 1 {
        0
    } else {
        (augs * 30).clamp(0, 150) * strength + strength * 150
    };
    #[allow(clippy::cast_precision_loss)]
    let load: f32 = if capacity < 1 {
        3.0
    } else if encumbrance < 0 {
        0.0
    } else {
        encumbrance as f32 / capacity as f32
    };
    let p = (f64::from(load) * 0.333_333_333_333_333_3).clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation)]
    {
        (p as f32, (p * 300.0).floor() as i32)
    }
}

/// What the player can see: the pair the panel worked out, the level the live bar carries and
/// the words under it. The first two are asserted to agree here rather than in the claim,
/// because a panel whose record and whose element disagreed would be a different defect.
fn drawn(c: &mut HeadlessClient) -> (f32, i32, String) {
    let (ui, screen) = super::parts(c.app_mut());
    let pair = screen
        .inventory
        .burden
        .expect("the panel always writes the pair");
    let meter = screen
        .inventory
        .burden_meter
        .expect("the shipped burden bar");
    let text = screen
        .inventory
        .burden_text
        .expect("the shipped burden text");
    let live = ui
        .node(meter)
        .expect("the bar is alive")
        .merged_properties()
        .get_float(dereth_ui_screens::bind::attr::METER_LEVEL)
        .expect("the level was written");
    assert!(
        (live - pair.0).abs() < 1e-6,
        "the bar on screen reads {live} and the panel's own pair says {}",
        pair.0
    );
    let words = text_of(ui, text);
    (pair.0, pair.1, words)
}

/// The bar and the number are the character's own load, and they follow it.
///
/// The recording is the premise: it carries a real character's strength, encumbrance and
/// augmentations. The second half moves the encumbrance the way the shard would have, and
/// the claim is that the panel reads it again rather than keeping what login said.
pub fn the_burden_bar_and_number_are_what_the_character_is_carrying() {
    let mut c = super::a_client_at_the_shop();
    let (strength, carried, augs) = inputs(&c);
    let (want_pos, want_percent) = expected(strength, carried, augs);
    let shown = drawn(&mut c);
    let agrees = (shown.0 - want_pos).abs() < 1e-6
        && shown.1 == want_percent
        && shown.2 == load_text(want_percent);

    // The character picks up a great deal more.
    let heavier = carried + 3_000;
    {
        let q = c
            .world_mut()
            .player_qualities_mut()
            .expect("the character's own description");
        q.ints
            .get_or_insert_with(std::collections::BTreeMap::new)
            .insert(5, heavier);
    }
    c.tick(2);
    let (pos_now, percent_now) = expected(strength, heavier, augs);
    let after = drawn(&mut c);
    let followed = percent_now != want_percent
        && (after.0 - pos_now).abs() < 1e-6
        && after.1 == percent_now
        && after.2 == load_text(percent_now);

    c.assert_behaviour(
        "inventory.burden.the-bar-and-the-number-are-what-the-character-is-carrying",
        move |_| agrees && followed,
    );
    c.shutdown();
}

/// The bar is drawn on a scale of three full loads, and both it and the number stop there.
pub fn the_burden_bar_and_number_top_out_together_at_three_times_a_full_load() {
    let mut c = super::a_gameplay_client();
    let mut seen: Vec<(f32, i32, String)> = Vec::new();
    for load in [0.0_f32, 0.69, 1.0, 3.0, 30.0, -1.0] {
        let (ui, screen) = super::parts(c.app_mut());
        screen.inventory.set_load_level(ui, load);
        let pair = screen
            .inventory
            .burden
            .expect("the panel always writes the pair");
        let text = screen
            .inventory
            .burden_text
            .expect("the shipped burden text");
        let words = text_of(ui, text);
        seen.push((pair.0, pair.1, words));
    }

    let nothing_carried = seen[0] == (0.0, 0, "0%".to_owned());
    // An ordinary character at 69% of a full load fills not quite a quarter of the frame,
    // and the number reads 68 rather than 69 -- the client's own rounding, which a plainer
    // one would get wrong. Both arms are here so the assertion is seen to discriminate.
    #[allow(clippy::cast_possible_truncation)]
    let plainer = (0.69_f32 * 100.0) as i32;
    let ordinary = seen[1].1 == 68
        && plainer != seen[1].1
        && (seen[1].0 - 0.23).abs() < 1e-6
        && seen[1].2 == "68%";
    let a_full_load = (seen[2].0 - 1.0 / 3.0).abs() < 1e-6 && seen[2].1 == 100;
    let topped_out =
        seen[3] == (1.0, 300, "300%".to_owned()) && seen[4] == (1.0, 300, "300%".to_owned());
    let never_below_nothing = seen[5] == (0.0, 0, "0%".to_owned());

    c.assert_behaviour(
        "inventory.burden.the-bar-and-the-number-top-out-together-at-three-times-a-full-load",
        move |_| nothing_carried && ordinary && a_full_load && topped_out && never_below_nothing,
    );
    c.shutdown();
}

/// The bar fills by uncovering part of its own picture, from the foot up, and moves nothing.
pub fn the_burden_bar_fills_from_its_foot_by_covering_part_of_its_frame() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    let meter = screen
        .inventory
        .burden_meter
        .expect("the shipped burden bar");
    let child = ui
        .get_child_recursive(meter, CHILD_IMAGE)
        .expect("the bar's own picture is inside it");
    let frame = ui.node(meter).expect("the bar is alive").region.box_;
    let tall = frame.height() > 1;

    screen.inventory.set_load_level(ui, 0.0);
    let low = screen.inventory.burden.expect("the pair").0;
    let low_child = ui.node(child).expect("alive").region.box_;
    screen.inventory.set_load_level(ui, 2.4);
    let high = screen.inventory.burden.expect("the pair").0;
    let high_child = ui.node(child).expect("alive").region.box_;
    let nothing_moved = high > low && low_child == high_child;

    let direction = match ui
        .node(meter)
        .expect("alive")
        .merged_properties()
        .get(dereth_ui::widgets::meter::attr::CHILD_DIRECTION)
    {
        Some(dereth_ui::PropertyValue::Enum(e)) => *e,
        other => panic!("the shipped burden bar declares no direction of its own: {other:?}"),
    };
    let from_the_foot = direction == dereth_ui::widgets::meter::direction::BOTTOM;

    let clip = |p: f32| {
        Meter {
            position: p,
            direction,
            ..Meter::default()
        }
        .child_clip(CHILD_IMAGE, frame)
    };
    let empty = clip(low).expect("a continuous bar clips its own picture");
    let part = clip(high).expect("a continuous bar clips its own picture");
    let covered = !empty.is_valid()
        && part.is_valid()
        && empty != part
        && part.height() < frame.height()
        && part.x0 >= frame.x0
        && part.x1 <= frame.x1
        && part.y0 >= frame.y0
        && part.y1 <= frame.y1
        && clip(1.0) == Some(frame);

    c.assert_behaviour(
        "inventory.burden.the-bar-fills-from-its-foot-by-covering-part-of-the-frame",
        move |_| tall && nothing_moved && from_the_foot && covered,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_the_burden_bar_and_number_are_what_the_character_is_carrying => the_burden_bar_and_number_are_what_the_character_is_carrying ["inventory.burden.the-bar-and-the-number-are-what-the-character-is-carrying"],
    scenario_the_burden_bar_and_number_top_out_together_at_three_times_a_full_load => the_burden_bar_and_number_top_out_together_at_three_times_a_full_load ["inventory.burden.the-bar-and-the-number-top-out-together-at-three-times-a-full-load"],
    scenario_the_burden_bar_fills_from_its_foot_by_covering_part_of_its_frame => the_burden_bar_fills_from_its_foot_by_covering_part_of_its_frame ["inventory.burden.the-bar-fills-from-its-foot-by-covering-part-of-the-frame"],
}
