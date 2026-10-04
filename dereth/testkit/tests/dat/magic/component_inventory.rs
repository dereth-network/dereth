use super::*;
pub fn the_components_page_is_the_census_of_what_the_player_carries() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let rows = census::drawn(&c);
    let tracked = census::tracker_rows(&c);
    let headers = strip::headers(&c);
    let non_empty = census::kinds_the_tracker_has(&c);

    // The premise, stated: the recording really did leave components in the pack.
    assert!(!rows.is_empty(), "the recorded pack carries components");
    assert!(!headers.is_empty(), "so the page has at least one heading");
    assert!(
        headers.len() < spellcomponent::CATEGORY_TITLES.len(),
        "and not one of every kind"
    );

    // The list itself: headings and rows and nothing else, a heading first.
    let ids = census::item_ids(&mut c);
    let header_count = ids
        .iter()
        .filter(|i| **i == spellcomponent::HEADER_ELEMENT)
        .count();
    let row_count = ids
        .iter()
        .filter(|i| **i == spellcomponent::ROW_ELEMENT)
        .count();
    let first_is_a_header = ids.first() == Some(&spellcomponent::HEADER_ELEMENT);

    // Row for row against the tracker, and then against the three columns on screen.
    let matches_tracker = rows
        .iter()
        .zip(tracked.iter())
        .all(|(r, (_, wcid, name, owned))| r.wcid == *wcid && &r.name == name && r.owned == *owned);
    let wanted = rows
        .iter()
        .all(|r| r.desired == c.view().world().player_system.desired_comp_level(r.wcid));
    let columns: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| {
            let e = r.element;
            (
                strip::child_text(&mut c, e, spellcomponent::row::NAME).unwrap_or_default(),
                strip::child_text(&mut c, e, spellcomponent::row::OWNED).unwrap_or_default(),
                strip::child_text(&mut c, e, spellcomponent::row::DESIRED).unwrap_or_default(),
            )
        })
        .collect();
    let want_columns: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| (r.name.clone(), r.owned.to_string(), r.desired.to_string()))
        .collect();

    // Every drawn row stands for a real component object of its own kind.
    let real = rows.iter().all(|r| {
        r.object.is_some_and(|o| {
            c.view()
                .world()
                .weenie(o)
                .is_some_and(|w| w.pwd.wcid == r.wcid && w.inq_type() & A_COMPONENT != 0)
        })
    });

    let row_len = rows.len();
    let tracked_len = tracked.len();
    let item_len = ids.len();

    c.assert_behaviour(
        "spell-components.strip.the-list-is-the-census-of-what-the-player-carries",
        move |_| {
            headers == non_empty
                && header_count == headers.len()
                && row_count == row_len
                && header_count + row_count == item_len
                && first_is_a_header
                && row_len == tracked_len
                && matches_tracker
                && wanted
                && columns == want_columns
                && real
        },
    );
    c.shutdown();
}

pub fn the_held_count_follows_a_stack_size_the_shard_changes() {
    use dereth_protocol::items::ItemUpdateStackSize;
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let rows = census::drawn(&c);

    // A row whose object is a **stack**: an un-stacked one takes a different arm and could not
    // move.
    let row = rows
        .iter()
        .find(|r| {
            r.object.is_some_and(|o| {
                c.view()
                    .world()
                    .weenie(o)
                    .is_some_and(|w| w.pwd.stack_size.is_some())
            })
        })
        .cloned()
        .unwrap_or_else(|| panic!("the recorded pack carries a stacked component"));
    let object = row.object.expect("checked just above");
    let stack = c
        .view()
        .world()
        .weenie(object)
        .and_then(|w| w.pwd.stack_size)
        .expect("a stack");
    let before_text = strip::child_text(&mut c, row.element, spellcomponent::row::OWNED);
    let before_owned = row.owned;
    let rebuilds_before = strip::rebuilds(&c);
    let applied_before = c.view().interaction().stats.stack_sizes_applied;

    // The stamp gate is per property and eight bits: the next one after whatever the login left.
    let sequence = c
        .view()
        .world()
        .weenie(object)
        .and_then(|w| w.stamper.as_ref())
        .and_then(|s| s.stamp(ItemUpdateStackSize::STAMPER_KEY))
        .map_or(1, |s| s.wrapping_add(1));
    let new_value = u32::from(stack) + 7;
    c.when(dereth_testkit::Inbound::message(&ItemUpdateStackSize {
        sequence,
        item: object,
        amount: new_value,
        new_value,
    }));
    let applied = c.view().interaction().stats.stack_sizes_applied;
    c.tick(2);

    let owned_now = census::tracker_rows(&c)
        .iter()
        .find(|(_, w, _, _)| *w == row.wcid)
        .map(|(_, _, _, n)| *n)
        .unwrap_or_else(|| panic!("the row's kind is still tracked"));
    let rebuilds_after = strip::rebuilds(&c);
    let drawn_now = census::drawn(&c)
        .into_iter()
        .find(|r| r.wcid == row.wcid)
        .unwrap_or_else(|| panic!("the row is still drawn"));
    let after_text = strip::child_text(&mut c, drawn_now.element, spellcomponent::row::OWNED);
    let want = before_owned - i64::from(stack) + i64::from(new_value);

    c.assert_behaviour(
        "spell-components.strip.the-held-count-follows-a-stack-size-the-shard-changes",
        move |_| {
            before_text == Some(before_owned.to_string())
                && applied == applied_before + 1
                && owned_now == want
                && rebuilds_after > rebuilds_before
                && drawn_now.owned == owned_now
                && after_text == Some(owned_now.to_string())
                && after_text != before_text
        },
    );
    c.shutdown();
}

pub fn a_click_on_a_component_row_selects_that_component_in_the_world() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let row = census::drawn(&c).first().cloned().expect("a drawn row");
    let items = census::items(&c);
    let index = items
        .iter()
        .position(|h| *h == row.element)
        .expect("the row is in the list");
    let first_is_a_header =
        census::element_id(&c, items[0]) == spellcomponent::HEADER_ELEMENT && index >= 1;
    let is_a_row = census::element_id(&c, row.element) == spellcomponent::ROW_ELEMENT;

    // The object the row stands for is a real component object of its kind.
    let object = row.object.expect("a drawn row always has an object");
    let real = c
        .view()
        .world()
        .weenie(object)
        .is_some_and(|w| w.pwd.wcid == row.wcid && w.pwd.obj_type & A_COMPONENT != 0);

    // The recorded login leaves something selected. Clear it, so the press is measured from
    // nothing and a leftover cannot be mistaken for the row's.
    c.world_mut()
        .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    let cleared = c.view().world().selected.is_none();
    let selections_before = c.view().interaction().stats.selections;
    let sent_before = c.outbound().len();

    census::click(&mut c, row.element, 2_000);

    let picked = census::selected_index(&c);
    let selected = c.view().world().selected;
    let flagged = c
        .view()
        .world()
        .tables
        .weenies
        .get(object)
        .is_some_and(|w| w.selected);
    let selections = c.view().interaction().stats.selections;
    let asked = census::asked_for_an_action(&c, sent_before);

    c.assert_behaviour(
        "spell-components.strip.a-click-on-a-row-selects-that-component-in-the-world",
        move |_| {
            first_is_a_header
                && is_a_row
                && real
                && cleared
                && picked == Some(index)
                && selections == selections_before + 1
                && selected == Some(object)
                && flagged
                && !asked
        },
    );
    c.shutdown();
}

pub fn a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let row = census::drawn(&c).first().cloned().expect("a drawn row");
    let object = row.object.expect("a drawn row always has an object");
    census::click(&mut c, row.element, 2_000);
    let selected = c.view().world().selected == Some(object);

    // The first heading: the list takes the press and the world selection does not move.
    let first = census::items(&c)[0];
    let first_is_a_header = census::element_id(&c, first) == spellcomponent::HEADER_ELEMENT;
    let before = c.view().interaction().stats.selections;
    census::click(&mut c, first, 3_000);
    let at_the_first = (
        census::selected_index(&c),
        c.view().interaction().stats.selections,
        c.view().world().selected,
    );

    // Any other heading clears it -- and the notice the clearing raises takes the highlight the
    // press left on the heading off again.
    let (_, header) = census::a_header_past_the_first(&c)
        .unwrap_or_else(|| panic!("a heading past the first is on screen to be pressed"));
    census::click(&mut c, header, 4_000);
    let after = (
        c.view().interaction().stats.selections,
        c.view().world().selected,
        census::selected_index(&c),
        c.view()
            .world()
            .tables
            .weenies
            .get(object)
            .is_some_and(|w| w.selected),
    );

    c.assert_behaviour(
        "spell-components.strip.a-click-on-a-header-clears-the-selection-and-the-first-one-does-nothing",
        move |_| {
            selected
                && first_is_a_header
                && at_the_first.0 == Some(0)
                && at_the_first.1 == before
                && at_the_first.2 == Some(object)
                && after.0 == before + 1
                && after.1.is_none()
                && after.2.is_none()
                && !after.3
        },
    );
    c.shutdown();
}

pub fn selecting_a_component_in_the_world_highlights_its_row() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();

    // A second object of a kind the page already draws, made by replaying the recording's own
    // create for one of them with the object id changed -- so the kind-not-object rule below has
    // something to prove. Nothing is invented but the id.
    let source = census::drawn(&c)[0]
        .object
        .expect("every drawn row has an object");
    census::a_second_pile_of(&mut c, source, dereth_primitives::ObjectId(0x8116_F5F5));
    let rows = census::drawn(&c);

    c.world_mut()
        .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    c.tick(2);
    let cleared = (census::selected_index(&c), strip::panel(&c).selected_object);

    // Every component object the player holds, by kind.
    let owned: Vec<(dereth_primitives::ObjectId, u32)> = {
        let w = c.view().world();
        w.exhaustive_contained_items(A_RECORDED_PLAYER)
            .into_iter()
            .filter_map(|o| {
                w.magic
                    .components
                    .object_is_owned_component(o)
                    .map(|k| (o, k))
            })
            .collect()
    };
    assert!(
        !owned.is_empty(),
        "the recorded pack holds tracked component objects"
    );

    // The object must **not** be its row's own representative: matching on the object rather than
    // on the kind would answer correctly for exactly those, and this scenario has to be able to
    // see that mistake.
    let (object, wcid) = owned
        .iter()
        .find(|(o, k)| rows.iter().any(|r| r.wcid == *k && r.object != Some(*o)))
        .copied()
        .unwrap_or_else(|| panic!("a drawn kind with a second object"));
    let row = rows
        .iter()
        .find(|r| r.wcid == wcid)
        .cloned()
        .expect("its row");
    let index = census::items(&c)
        .iter()
        .position(|h| *h == row.element)
        .expect("it is listed");

    c.world_mut()
        .set_selected_object(Some(object), false, &mut dereth_client_model::NullSink);
    c.tick(2);
    let lit = (
        census::selected_index(&c),
        census::element_id(&c, census::items(&c)[index]),
        strip::panel(&c).selected_object,
        strip::panel(&c).selection_notices,
        c.view().world().selected,
        strip::panel(&c).broadcast_selection,
    );

    // The same selection again does nothing at all.
    let notices = strip::panel(&c).selection_notices;
    c.tick(3);
    let unchanged = strip::panel(&c).selection_notices;

    // Something that is not a component clears the row...
    c.world_mut().set_selected_object(
        Some(A_RECORDED_PLAYER),
        false,
        &mut dereth_client_model::NullSink,
    );
    c.tick(2);
    let after_a_non_component = (census::selected_index(&c), strip::panel(&c).selected_object);

    // ...and with nothing remembered it does nothing rather than clearing again.
    let notices_now = strip::panel(&c).selection_notices;
    c.world_mut()
        .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    c.tick(2);
    let quiet = strip::panel(&c).selection_notices;

    c.assert_behaviour(
        "spell-components.strip.selecting-a-component-in-the-world-highlights-its-row",
        move |_| {
            cleared == (None, None)
                && lit.0 == Some(index)
                && lit.1 == spellcomponent::ROW_ELEMENT
                && lit.2 == Some(object)
                && lit.3 == 1
                && lit.4 == Some(object)
                && lit.5
                && unchanged == notices
                && after_a_non_component == (None, None)
                && quiet == notices_now
        },
    );
    c.shutdown();
}
