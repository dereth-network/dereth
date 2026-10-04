use super::*;
pub fn a_click_on_a_component_icon_selects_the_one_in_the_pack() {
    use dereth_ui_screens::panels::examination;

    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[]);
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;
    let before = c.view().world().selected;

    // Each row really does carry the slot it stands for: the premise the click's own lookup is
    // handed, read off the live rows rather than off the panel's mirror.
    let stamped = examine::row_scids(&mut c, formula.len());

    let (x, y) = examine::component_icon_point(&mut c, 2);
    examine::press(&mut c, x, y, 200_000, examine::PRIMARY);

    let pane = examine::pane_facts(&mut c);
    let selected = c.view().world().selected;
    let up = c.ui_snapshot().is_visible(examination::WINDOW);
    let title = examine::title(&mut c);
    let want = examine::component_object(2);

    c.assert_behaviour(
        "spell-examine.components.a-click-on-an-icon-selects-the-one-in-the-pack",
        move |_| {
            before.is_none()
                && stamped == formula
                && pane.component_clicks == 1
                && pane.component_selections == 1
                && pane.self_selections_absorbed == 1
                && pane.examine_newly_selected_item
                && selected == Some(want)
                && up
                && title == name
        },
    );
    c.shutdown();
}

pub fn each_component_icon_names_its_own_slot() {
    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[]);
    let mut picked = Vec::new();
    for i in 0..formula.len() {
        let (x, y) = examine::component_icon_point(&mut c, i);
        let at = 300_000 + u32::try_from(i).expect("a small formula") * 1_000;
        examine::press(&mut c, x, y, at, examine::PRIMARY);
        picked.push(c.view().world().selected);
    }
    let want: Vec<Option<dereth_primitives::ObjectId>> = (0..formula.len())
        .map(|i| Some(examine::component_object(i)))
        .collect();
    let selections = examine::pane_facts(&mut c).component_selections as usize;

    c.assert_behaviour(
        "spell-examine.components.each-icon-names-its-own-slot",
        move |_| picked == want && selections == want.len(),
    );
    c.shutdown();
}

pub fn a_component_the_player_does_not_carry_selects_nothing() {
    use dereth_ui_screens::panels::examination;

    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[3]);
    let drawn = examine::pane_facts(&mut c).rows_drawn as usize;

    let (x, y) = examine::component_icon_point(&mut c, 3);
    examine::press(&mut c, x, y, 400_000, examine::PRIMARY);
    let refused = examine::pane_facts(&mut c);
    let nothing = c.view().world().selected;
    let up = c.ui_snapshot().is_visible(examination::WINDOW);

    // The control, in the same run: the slot beside it, which the player does carry.
    let (x, y) = examine::component_icon_point(&mut c, 4);
    examine::press(&mut c, x, y, 410_000, examine::PRIMARY);
    let selected = c.view().world().selected;
    let want = examine::component_object(4);

    c.assert_behaviour(
        "spell-examine.components.one-the-player-does-not-carry-selects-nothing",
        move |_| {
            drawn == formula.len()
                && refused.component_clicks == 1
                && refused.component_selections == 0
                && refused.self_selections_absorbed == 0
                && refused.examine_newly_selected_item
                && nothing.is_none()
                && up
                && selected == Some(want)
        },
    );
    c.shutdown();
}

pub fn the_secondary_click_does_nothing_on_a_component_icon() {
    let (mut c, _) = examine::a_pane_on(FLAME_BOLT, &[]);
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;

    let (x, y) = examine::component_icon_point(&mut c, 1);
    examine::press(&mut c, x, y, 500_000, examine::SECONDARY);
    let after_right = (
        examine::pane_facts(&mut c).component_clicks,
        c.view().world().selected,
        examine::title(&mut c),
    );

    examine::press(&mut c, x, y, 510_000, examine::PRIMARY);
    let after_left = c.view().world().selected;
    let want = examine::component_object(1);

    c.assert_behaviour(
        "spell-examine.components.the-secondary-click-does-nothing-on-an-icon",
        move |_| {
            after_right.0 == 0
                && after_right.1.is_none()
                && after_right.2 == name
                && after_left == Some(want)
        },
    );
    c.shutdown();
}

pub fn the_component_a_slot_stands_for_is_a_representative_of_its_kind() {
    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[]);
    let scid = formula[0];
    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, scid, 10);

    let first = c
        .view()
        .world()
        .component_object_id(scid)
        .expect("the kind is carried");
    let again = c.view().world().component_object_id(scid);
    let one_of_the_two =
        first == examine::component_object(0) || first == examine::A_SECOND_OF_ITS_KIND;

    // Take the answered one away; the kind is still owned, so the slot still answers.
    examine::drop_it(&mut c, first);
    let rest = c
        .view()
        .world()
        .component_object_id(scid)
        .expect("the other one is still here");

    // A slot number the shipped table does not know, and one whose kind is not carried.
    let unknown = c.view().world().component_object_id(0);
    let outside = c.view().world().component_object_id(0x00FF_FFFF);

    c.assert_behaviour(
        "spell-examine.components.the-answer-is-a-representative-of-the-class-and-not-one-object",
        move |_| {
            one_of_the_two
                && again == Some(first)
                && rest != first
                && unknown.is_none()
                && outside.is_none()
        },
    );
    c.shutdown();
}

pub fn the_components_the_player_lacks_are_the_ones_that_are_marked() {
    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();
    let marked = examine::marked(&mut c, slots);
    let pane = examine::pane_facts(&mut c);

    assert_eq!(slots, 8, "the eight-slot formula this scenario is about");

    c.assert_behaviour(
        "spell-examine.marks.the-components-the-player-lacks-are-the-ones-that-are-marked",
        move |_| {
            marked == missing
                && pane.rows_marked_missing == missing.len()
                // The fill marks the rows itself: the moves that filled the pack each ran the
                // marking pass over a list that did not exist yet, so the run that marked these
                // eight rows is one more than the notices.
                && pane.components_marked > pane.notices_pulled
        },
    );
    c.shutdown();
}

pub fn everything_carried_marks_nothing_and_nothing_carried_marks_everything() {
    let (mut c, formula) = examine::a_pane_on(ACID_STREAM_V, &[]);
    let none_marked = examine::marked(&mut c, formula.len());
    let none_counted = examine::pane_facts(&mut c).rows_marked_missing;
    c.shutdown();

    let all: Vec<usize> = (0..formula.len()).collect();
    let (mut c, formula) = examine::a_pane_on(ACID_STREAM_V, &all);
    let slots = formula.len();
    let every = examine::marked(&mut c, slots);
    let every_counted = examine::pane_facts(&mut c).rows_marked_missing;

    c.assert_behaviour(
        "spell-examine.marks.everything-carried-marks-nothing-and-nothing-carried-marks-everything",
        move |_| {
            none_marked.is_empty() && none_counted == 0 && every == all && every_counted == slots
        },
    );
    c.shutdown();
}

pub fn a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back() {
    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();
    let at_first = examine::marked(&mut c, slots);
    let notices_before = examine::pane_facts(&mut c).notices_pulled;

    let slot = missing[0];
    examine::carry(&mut c, examine::component_object(slot), formula[slot], 10);
    c.tick(1);
    let after_arrival = examine::marked(&mut c, slots);
    let live = examine::pane_facts(&mut c);

    examine::drop_it(&mut c, examine::component_object(slot));
    c.tick(1);
    let after_departure = examine::marked(&mut c, slots);
    let still_missing = vec![missing[1]];

    c.assert_behaviour(
        "spell-examine.marks.a-component-arriving-clears-its-mark-and-one-leaving-brings-it-back",
        move |_| {
            at_first == missing
                && after_arrival == still_missing
                && live.notices_pulled > notices_before
                && live.filled == 1
                && after_departure == missing
        },
    );
    c.shutdown();
}

pub fn a_closed_window_still_re_marks_its_rows() {
    use dereth_ui_screens::panels::examination;

    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();

    let closed = {
        let (ui, screen) = examine::parts(&mut c);
        screen.examination.close_from_action(ui)
    };
    c.tick(1);
    let shut = !c.ui_snapshot().is_visible(examination::WINDOW);

    let slot = missing[1];
    examine::carry(&mut c, examine::component_object(slot), formula[slot], 10);
    c.tick(1);
    let after = examine::marked(&mut c, slots);
    let want = vec![missing[0]];

    c.assert_behaviour(
        "spell-examine.marks.a-closed-window-still-re-marks-its-rows",
        move |_| closed && shut && after == want,
    );
    c.shutdown();
}

pub fn a_second_of_the_same_kind_keeps_the_mark_off() {
    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();
    let slot = (0..slots)
        .find(|i| !missing.contains(i))
        .expect("a slot the player carries");

    let plain_at_first = !examine::marked(&mut c, slots).contains(&slot);

    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, formula[slot], 10);
    c.tick(1);
    let two_of_them = !examine::marked(&mut c, slots).contains(&slot);

    examine::drop_it(&mut c, examine::component_object(slot));
    c.tick(1);
    let one_left = !examine::marked(&mut c, slots).contains(&slot);
    let still_owned = c.view().world().spell_component_is_owned(formula[slot]);

    examine::drop_it(&mut c, examine::A_SECOND_OF_ITS_KIND);
    c.tick(1);
    let none_left = examine::marked(&mut c, slots).contains(&slot);
    let owned_now = c.view().world().spell_component_is_owned(formula[slot]);

    c.assert_behaviour(
        "spell-examine.marks.a-second-of-the-same-kind-keeps-the-mark-off",
        move |_| {
            plain_at_first && two_of_them && one_left && still_owned && none_left && !owned_now
        },
    );
    c.shutdown();
}

pub fn the_notice_moves_for_a_component_and_for_nothing_else() {
    let (mut c, formula, _) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);

    let before = c.view().world().magic.component_serial;
    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, formula[0], 10);
    let after_component = c.view().world().magic.component_serial;

    // An ordinary item in the same pack: it is not a component, so it raises nothing.
    examine::carry_a_plain_item(&mut c, examine::A_PLAIN_ITEM);
    let after_item = c.view().world().magic.component_serial;

    c.tick(2);
    let after_idle_frames = c.view().world().magic.component_serial;

    c.assert_behaviour(
        "spell-examine.marks.the-notice-moves-for-a-component-and-for-nothing-else",
        move |_| {
            after_component > before
                && after_item == after_component
                && after_idle_frames == after_component
        },
    );
    c.shutdown();
}
