use super::*;
pub fn a_component_row_shows_how_many_are_held_and_goes_away_at_none() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    let (scid, wcid) = (kinds[0].0, kinds[0].1);
    let a = dereth_primitives::ObjectId(0x8116_0501);
    let b = dereth_primitives::ObjectId(0x8116_0502);

    examine::carry(&mut c, a, scid, 7);
    c.tick(1);
    let seven = (strip::owned_text(&mut c, wcid), strip::rows(&c).len());

    // A second pile of the same kind is the same row, with the sum.
    examine::carry(&mut c, b, scid, 4);
    c.tick(1);
    let eleven = (strip::owned_text(&mut c, wcid), strip::rows(&c).len());

    examine::drop_it(&mut c, b);
    c.tick(1);
    let back_to_seven = strip::owned_text(&mut c, wcid);

    examine::drop_it(&mut c, a);
    c.tick(1);
    let gone = (strip::owned_text(&mut c, wcid), strip::rows(&c).is_empty());

    c.assert_behaviour(
        "spell-components.strip.a-row-shows-how-many-are-held-and-goes-away-at-none",
        move |_| {
            seven == (Some("7".to_owned()), 1)
                && eleven == (Some("11".to_owned()), 1)
                && back_to_seven.as_deref() == Some("7")
                && gone == (None, true)
        },
    );
    c.shutdown();
}

pub fn the_component_page_rebuilds_on_the_pack_and_not_every_frame() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    let scid = kinds[1].0;

    examine::carry(&mut c, dereth_primitives::ObjectId(0x8116_0601), scid, 3);
    c.tick(1);
    let after_add = strip::rebuilds(&c);

    c.tick(3);
    let after_idle = strip::rebuilds(&c);

    examine::carry(&mut c, dereth_primitives::ObjectId(0x8116_0602), scid, 1);
    c.tick(1);
    let after_second = strip::rebuilds(&c);
    let four = strip::owned_text(&mut c, kinds[1].1);

    c.assert_behaviour(
        "spell-components.strip.it-rebuilds-when-the-pack-changes-and-not-every-frame",
        move |_| {
            after_add > 0
                && after_idle == after_add
                && after_second > after_add
                && four.as_deref() == Some("4")
        },
    );
    c.shutdown();
}

pub fn a_header_is_drawn_only_for_a_kind_the_player_holds_something_of() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    c.tick(1);
    let empty = (strip::headers(&c), strip::rows(&c).len());

    let first = dereth_primitives::ObjectId(0x8116_0701);
    examine::carry(&mut c, first, kinds[0].0, 1);
    c.tick(1);
    let one = strip::headers(&c);

    // A second kind, in a different part of the order.
    let second = kinds
        .iter()
        .find(|k| k.3 != kinds[0].3)
        .expect("two kinds in two places")
        .clone();
    examine::carry(
        &mut c,
        dereth_primitives::ObjectId(0x8116_0702),
        second.0,
        1,
    );
    c.tick(1);
    let two = strip::headers(&c);
    let mut want_two = vec![kinds[0].3, second.3];
    want_two.sort_unstable();

    examine::drop_it(&mut c, first);
    c.tick(1);
    let back_to_one = strip::headers(&c);
    let want_first = vec![kinds[0].3];
    let want_second = vec![second.3];

    c.assert_behaviour(
        "spell-components.strip.a-header-is-drawn-only-for-a-kind-the-player-holds-something-of",
        move |_| {
            empty == (Vec::new(), 0)
                && one == want_first
                && two == want_two
                && back_to_one == want_second
        },
    );
    c.shutdown();
}

pub fn the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    for (i, k) in kinds.iter().enumerate() {
        let id =
            dereth_primitives::ObjectId(0x8116_0800 + u32::try_from(i).expect("a small formula"));
        examine::carry(&mut c, id, k.0, 1);
    }
    c.tick(1);

    let mut want_headers: Vec<u32> = kinds.iter().map(|k| k.3).collect();
    want_headers.sort_unstable();
    want_headers.dedup();
    let headers = strip::headers(&c);
    let drawn: Vec<u32> = strip::rows(&c).iter().map(|(w, _)| *w).collect();

    let mut want_rows: Vec<u32> = kinds.iter().map(|k| k.1).collect();
    want_rows.sort_by_key(|w| {
        kinds
            .iter()
            .find(|k| k.1 == *w)
            .map(|k| k.3)
            .unwrap_or(u32::MAX)
    });
    let kinds_len = kinds.len();

    c.assert_behaviour(
        "spell-components.strip.the-walk-is-kind-order-with-each-kinds-rows-under-its-header",
        move |_| headers == want_headers && drawn.len() == kinds_len && drawn == want_rows,
    );
    c.shutdown();
}

pub fn the_component_row_icon_is_the_shipped_tables_and_not_the_objects() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    let (scid, wcid, name, _) = kinds[0].clone();
    strip::carry_with_a_wrong_picture(&mut c, dereth_primitives::ObjectId(0x8116_0901), scid, 2);
    c.tick(1);

    let want = c
        .view()
        .world()
        .magic
        .catalogue
        .component_icon(wcid)
        .expect("the shipped table has a picture for this kind");
    assert_ne!(
        want, A_WRONG_PICTURE,
        "the premise: the two pictures differ"
    );

    let row = strip::rows(&c)
        .into_iter()
        .find(|(w, _)| *w == wcid)
        .expect("a row")
        .1;
    let drawn = strip::row_image(
        &c,
        row,
        dereth_ui_screens::panels::spellcomponent::row::ICON,
    );
    let drawn_name = strip::child_text(
        &mut c,
        row,
        dereth_ui_screens::panels::spellcomponent::row::NAME,
    );

    c.assert_behaviour(
        "spell-components.strip.the-icon-is-the-shipped-tables-and-not-the-objects",
        move |_| {
            drawn == Some(dereth_primitives::DataId(want))
                && drawn_name.as_deref() == Some(name.as_str())
        },
    );
    c.shutdown();
}
