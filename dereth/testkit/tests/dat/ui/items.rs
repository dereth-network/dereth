//! UI fixtures and scenarios for items.

use super::*;
// =============================================================================================
// ui.selection-strip.* and ui.item-cell.* -- the toolbar's selection strip and the pack's cells
//
// The green highlight fades, the split slider shows only for a stack, the player's own cell draws
// a backpack, the drag image is the icon without the slot's backdrop, and a no-op drop leaves no
// slot greyed. Six scenarios, six rows.
// =============================================================================================

/// The strip's plate: the child of the selection field that carries the highlight picture.
const SEL_PLATE: ElementId = ElementId(0x1000_01A0);
/// The name beside it, which the fade must not touch.
const SEL_NAME: ElementId = ElementId(0x1000_019F);
/// The two meters that live in the rectangle the highlight vacates.
const SEL_HEALTH: ElementId = ElementId(0x1000_01A1);
const SEL_MANA: ElementId = ElementId(0x1000_01A2);

/// The picture an element is drawing, if any.
fn hud_image(c: &HeadlessClient, h: ElemHandle) -> Option<dereth_primitives::DataId> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("alive")
        .region
        .image
        .as_ref()
        .map(|g| g.did)
}

/// The clock the screen's own timed tracks are measured against. **Not the wall clock**: a
/// headless frame advances this by a fixed step and runs far faster than real time, so a
/// wall-clock reading of an authored quarter second says only how quick the machine is.
fn hud_clock(c: &HeadlessClient) -> f64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .now
        .0
}

/// Frames until `f`, or until `limit` seconds of that clock have gone by; answers how much of it
/// went by.
fn wait_on_the_clock(
    c: &mut HeadlessClient,
    limit: f64,
    mut f: impl FnMut(&HeadlessClient) -> bool,
) -> f64 {
    let t0 = hud_clock(c);
    loop {
        c.tick(1);
        if f(c) || hud_clock(c) - t0 > limit {
            return hud_clock(c) - t0;
        }
    }
}

/// Select something, or nothing, the way the world tells the toolbar about it.
fn select(c: &mut HeadlessClient, id: Option<dereth_primitives::ObjectId>) {
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(
            id,
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
    c.tick(1);
}

/// Put a thing in the world and hand it to the strip. The name is this scenario's own invention,
/// which is what "private" means here.
fn a_selected_thing(
    c: &mut HeadlessClient,
    id: u32,
    obj_type: u32,
    stack: Option<u16>,
) -> dereth_primitives::ObjectId {
    let id = dereth_primitives::ObjectId(id);
    let mut w = dereth_client_model::weenie::Weenie::new(id);
    w.pwd.name = "a private thing".into();
    w.pwd.obj_type = obj_type;
    w.pwd.stack_size = stack;
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .tables
        .weenies
        .insert(id, w);
    select(c, Some(id));
    id
}

// ---------------------------------------------------------------------------------------------
// ui.selection-strip.the-highlight-comes-down-by-itself-a-quarter-second-later
// ---------------------------------------------------------------------------------------------

/// The green plate comes down by itself rather than staying up for the rest of the session. The
/// authored number is a quarter of a second, and it is read off the clock the track is run against.
pub(super) fn the_highlight_comes_down_by_itself_a_quarter_second_later() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let field = hud_find(&c, SEL_OBJECT_FIELD);
    let plate = hud_find(&c, SEL_PLATE);
    let name = hud_find(&c, SEL_NAME);

    let nothing_is_lit = hud_state(&c, field) == StateId(0) && hud_image(&c, plate).is_none();

    let thing = a_selected_thing(&mut c, 0x7100_0031, 1, None);
    // The selection edge lights the strip and the plate follows it there.
    let it_lit_up = hud_state(&c, field) == StateId(0x1000_000B);
    let green = hud_image(&c, plate).expect("the lit state authors a picture on the plate");
    let a_real_picture = green.0 != 0;

    // ...and it comes down by itself. Three seconds is generous head-room and is still far inside
    // "never".
    let took = wait_on_the_clock(&mut c, 3.0, |c| hud_image(c, plate).is_none());
    let it_came_down = hud_image(&c, plate).is_none() && (0.25..0.35).contains(&took);
    let the_child_was_told =
        hud_state(&c, field) == StateId(0) && hud_state(&c, plate) == StateId(0);

    // Only the plate goes: the name beside it and the strip itself are untouched.
    let the_rest_stayed = hud_visible(&c, name) && hud_visible(&c, field);

    // And it is a track and not a one-shot.
    select(&mut c, None);
    select(&mut c, Some(thing));
    let it_lights_again = hud_image(&c, plate) == Some(green);

    c.assert_behaviour(
        "ui.selection-strip.the-highlight-comes-down-by-itself-a-quarter-second-later",
        move |_| {
            nothing_is_lit
                && it_lit_up
                && a_real_picture
                && it_came_down
                && the_child_was_told
                && the_rest_stayed
                && it_lights_again
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.selection-strip.the-meters-start-down-and-a-creatures-answer-brings-its-bar-up
// ---------------------------------------------------------------------------------------------

/// What replaces the highlight for a creature, driven through a whole live frame rather than the
/// model alone.
pub(super) fn the_meters_start_down_and_a_creatures_answer_brings_its_bar_up() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let health = hud_find(&c, SEL_HEALTH);
    let mana = hud_find(&c, SEL_MANA);
    let both_start_down = !hud_visible(&c, health) && !hud_visible(&c, mana);

    let creature = a_selected_thing(&mut c, 0x7100_0032, 0x10, None);
    let nothing_yet = !hud_visible(&c, health);

    // The answer about its health, through the same door the wire arm goes through.
    let answered = c
        .app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .update_object_health(creature, 0.375);
    c.tick(1);
    let the_bar_came_up = answered && hud_visible(&c, health);
    let and_only_that_one = !hud_visible(&c, mana);

    c.assert_behaviour(
        "ui.selection-strip.the-meters-start-down-and-a-creatures-answer-brings-its-bar-up",
        move |_| both_start_down && nothing_yet && the_bar_came_up && and_only_that_one,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.selection-strip.empty-a-single-item-and-a-stack-are-three-different-strips
// ---------------------------------------------------------------------------------------------

/// The three states are asserted against each other in one scenario, because a build that hides
/// the splitter always passes any one of them alone.
pub(super) fn empty_a_single_item_and_a_stack_are_three_different_strips() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let field = hud_find(&c, SEL_OBJECT_FIELD);
    let entry = hud_find(&c, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let slider = hud_find(&c, dereth_ui_screens::toolbar::splitter::SLIDER);

    // 1. Nothing selected. The strip's own background is up; the splitter is not -- and the
    // shipped layout authors both of those two visible, so something took them down.
    let empty = hud_state(&c, field) == StateId(0)
        && hud_visible(&c, field)
        && !hud_visible(&c, entry)
        && !hud_visible(&c, slider);

    // 2. One thing. A different state, still no splitter.
    let thing = a_selected_thing(&mut c, 0x7100_0033, 1, None);
    let single = hud_state(&c, field) == StateId(0x1000_000B)
        && !hud_visible(&c, entry)
        && !hud_visible(&c, slider);

    // 3. A stack of them. A third state, and the splitter comes up.
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .tables
        .weenies
        .get_mut(thing)
        .expect("alive")
        .pwd
        .stack_size = Some(7);
    c.tick(1);
    let stacked = hud_state(&c, field) == StateId(0x1000_000C)
        && hud_visible(&c, entry)
        && hud_visible(&c, slider);

    // ...and dropping the selection takes the splitter down again.
    select(&mut c, None);
    let empty_again =
        hud_state(&c, field) == StateId(0) && !hud_visible(&c, entry) && !hud_visible(&c, slider);

    c.assert_behaviour(
        "ui.selection-strip.empty-a-single-item-and-a-stack-are-three-different-strips",
        move |_| empty && single && stacked && empty_again,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop
// ---------------------------------------------------------------------------------------------

/// The backing and the backpack on it both draw. The two ways of
/// reading the pair of numbers that names the picture both resolve, which is why reading them the
/// wrong way round was silent -- so the pixels are what settle it.
pub(super) fn the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));

    // The two readings, through the client's own two-hop resolver, which needs a client up.
    let right = dereth_ui_screens::env::did_by_enum(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        7,
        0x1000_0004,
    )
    .expect("the pair the client forwards resolves");
    let wrong = dereth_ui_screens::env::did_by_enum(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        0x1000_0004,
        7,
    )
    .expect("and so does the reversed one, which is why the mistake was silent");
    let two_different_pictures = right != wrong;

    // The production recipe, taken off the real cell rather than from the tables.
    let d = dereth_ui_screens::view::SlotDecoration {
        is_player: true,
        obj_type: 0x10,
        icon_id: 0x0600_13A5,
        ..dereth_ui_screens::view::SlotDecoration::default()
    };
    let recipe = dereth_ui_screens::items::widget::object_recipe(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        &d,
    );
    let dereth_ui::region::IconRecipe::Object {
        background, icon, ..
    } = recipe
    else {
        panic!("the cell composites a thing")
    };
    let it_names_the_backpack = icon == Some(right);
    let the_tile_is_its_own_row = background
        == dereth_ui_screens::env::did_by_enum(
            &c.view().expect_app().ui().expect("the UI shell is up").ui,
            0x1000_0004,
            10,
        );
    let not_the_backdrop_again = icon != background;

    // The pixels. An opaque second backdrop laid over the tile changes *every* pixel of it; the
    // real picture has a clear field, so it changes some and leaves the rest of the tile showing.
    // That count is the discriminator, and nothing else here is.
    let store = c.dat_store().expect("the retail dats are open").clone();
    let textures = dereth_scene::textures::TextureStore::new(&store);
    let f = |id: dereth_primitives::DataId| textures.texture_data(id).ok();
    let composed =
        dereth_client_shell::ui_draw::composite(recipe, &f).expect("the cell composites");
    let tile_only = dereth_client_shell::ui_draw::composite(
        dereth_ui::region::IconRecipe::Object {
            background,
            effects: None,
            icon: None,
            overlay: None,
            underlay: None,
        },
        &f,
    )
    .expect("the tile on its own composites");
    let a = composed.levels.first().expect("one level");
    let b = tile_only.levels.first().expect("one level");
    let pixels = a.len() / 4;
    let changed = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count();
    let something_was_drawn = changed > 0;
    let and_not_the_whole_tile = changed < pixels;

    c.assert_behaviour(
        "ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop",
        move |_| {
            two_different_pictures
                && it_names_the_backpack
                && the_tile_is_its_own_row
                && not_the_backdrop_again
                && something_was_drawn
                && and_not_the_whole_tile
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The pack, with one thing in it
// ---------------------------------------------------------------------------------------------

/// A player, one loose thing in the open pack, and the inventory page up -- the shipped list
/// filled the way the client fills it when the shard sends a container's contents.
fn a_pack_with_one_thing_in_it() -> (HeadlessClient, dereth_primitives::ObjectId) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let me = dereth_primitives::ObjectId(0x5000_0001);
    let thing = dereth_primitives::ObjectId(0x7100_0041);
    {
        let w = c.app_mut().probe_mut().objects_mut();
        let mut p = dereth_client_model::weenie::Weenie::new(me);
        p.pwd.name = "a private player".into();
        p.pwd.obj_type = 0x10;
        p.pwd.items_capacity = Some(10);
        p.pwd.containers_capacity = Some(2);
        w.world.tables.weenies.insert(me, p);
        let mut i = dereth_client_model::weenie::Weenie::new(thing);
        i.pwd.name = "a private thing".into();
        i.pwd.obj_type = 1;
        i.pwd.container_id = Some(me);
        i.pwd.icon_id = 0x0600_13A5;
        i.pwd.effects = Some(0);
        w.world.tables.weenies.insert(thing, i);
        w.world.player = Some(me);
        w.world.open_container = Some(me);
        w.world.view_object_contents(
            me,
            &[dereth_protocol::types::ContentProfile {
                iid: thing,
                container_properties: 0,
            }],
            &mut dereth_client_model::RecordingSink::default(),
        );
    }
    c.tick(1);
    let panel = {
        let (_, s) = hud_gameplay(&mut c);
        s.panels
            .pages
            .iter()
            .find(|q| q.element == ElementId(0x1000_018B))
            .map(|q| q.panel_id)
            .expect("the inventory page is in the panel stack")
    };
    {
        let (ui, s) = hud_gameplay(&mut c);
        s.recv_set_panel_visibility(ui, panel, true);
    }
    c.tick(1);
    (c, thing)
}

// ---------------------------------------------------------------------------------------------
// ui.item-cell.what-follows-the-cursor-is-the-icon-alone-and-not-the-lifted-cell
// ---------------------------------------------------------------------------------------------

/// A real drag off a real slot of the shipped list, and then the same rule stated over a whole
/// five-layer recipe and its pixels.
pub(super) fn what_follows_the_cursor_is_the_icon_alone() {
    let (mut c, thing) = a_pack_with_one_thing_in_it();

    let (cell_recipe, proxy_recipe) = {
        let (ui, s) = hud_gameplay(&mut c);
        let w = s
            .inventory
            .item_list
            .as_mut()
            .expect("the shipped item list");
        let i = w
            .slots
            .iter()
            .position(|q| q.item == Some(thing))
            .expect("the thing has a slot");
        let cell_recipe = w.slots[i]
            .icon_recipe(ui)
            .expect("the slot draws a composite");
        let (ox, oy) = ui.screen_origin(w.handle);
        let b = ui.node(w.slots[i].handle).expect("alive").region.box_;
        let start = w
            .begin_drag(ui, ox + b.x0 + b.width() / 2, oy + b.y0 + b.height() / 2)
            .expect("a filled slot starts a drag");
        let proxy = ui
            .node(start.drag_icon)
            .expect("the thing following the cursor is alive")
            .region
            .image
            .clone()
            .expect("and it has been given a picture");
        let Some(dereth_ui::region::SurfaceOp::Icon(r)) = proxy.op else {
            panic!("the proxy carries no recipe at all")
        };
        (cell_recipe, r)
    };
    let not_the_cells_own = proxy_recipe != cell_recipe;
    let dereth_ui::region::IconRecipe::Object {
        background,
        underlay,
        icon,
        ..
    } = proxy_recipe
    else {
        panic!("the proxy composites a thing")
    };
    let no_tile_under_the_cursor = background.is_none() && underlay.is_none();
    let the_things_own_picture = icon == Some(dereth_primitives::DataId(0x0600_13A5));

    // The same rule over a whole five-layer recipe: two layers are dropped and three are kept.
    let cell = dereth_ui::region::IconRecipe::Object {
        background: Some(dereth_primitives::DataId(0x0600_11CE)),
        effects: Some(dereth_primitives::DataId(0x0600_11C5)),
        icon: Some(dereth_primitives::DataId(0x0600_13A5)),
        overlay: Some(dereth_primitives::DataId(0x0600_1234)),
        underlay: Some(dereth_primitives::DataId(0x0600_5678)),
    };
    let drag = cell.drag_surface();
    let a_different_surface = drag != cell;
    let dereth_ui::region::IconRecipe::Object {
        background,
        effects,
        icon,
        overlay,
        underlay,
    } = drag
    else {
        panic!("a thing")
    };
    let the_right_three = background.is_none()
        && underlay.is_none()
        && icon == Some(dereth_primitives::DataId(0x0600_13A5))
        && overlay == Some(dereth_primitives::DataId(0x0600_1234))
        && effects == Some(dereth_primitives::DataId(0x0600_11C5));

    // The pixels: what follows the cursor is not the cell's picture, and it is clear somewhere,
    // which is what makes it read as a floating icon rather than as a lifted cell.
    let store = c.dat_store().expect("the retail dats are open").clone();
    let textures = dereth_scene::textures::TextureStore::new(&store);
    let f = |id: dereth_primitives::DataId| textures.texture_data(id).ok();
    let cell_px = dereth_client_shell::ui_draw::composite(cell, &f).expect("the cell composites");
    let drag_px = dereth_client_shell::ui_draw::composite(drag, &f).expect("the proxy composites");
    let different_pixels = cell_px.levels.first() != drag_px.levels.first();
    let clear_somewhere = drag_px
        .levels
        .first()
        .expect("one level")
        .chunks_exact(4)
        .any(|px| px[3] == 0);

    c.assert_behaviour(
        "ui.item-cell.what-follows-the-cursor-is-the-icon-alone-and-not-the-lifted-cell",
        move |_| {
            not_the_cells_own
                && no_tile_under_the_cursor
                && the_things_own_picture
                && a_different_surface
                && the_right_three
                && different_pixels
                && clear_somewhere
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.item-cell.dropping-a-thing-back-on-its-own-slot-sends-nothing-and-ghosts-nothing
// ---------------------------------------------------------------------------------------------

/// Both halves of the decline: nothing is asked of the shard, and nothing is left greyed. The
/// first is what stops a fix that merely un-greys afterwards from passing.
pub(super) fn dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing() {
    let (mut c, thing) = a_pack_with_one_thing_in_it();

    // Pick it up first. The lift greys the slot it came from, so the grey overlay is already
    // there before the drop -- a scenario that only dropped could not see it left behind.
    let (source, target, the_lift_greyed_it) = {
        let (ui, s) = hud_gameplay(&mut c);
        let (target, at) = {
            let w = s
                .inventory
                .item_list
                .as_mut()
                .expect("the shipped item list");
            let i = w
                .slots
                .iter()
                .position(|q| q.item == Some(thing))
                .expect("the thing has a slot");
            let target = w.slots[i].handle;
            let (ox, oy) = ui.screen_origin(w.handle);
            let b = ui.node(target).expect("alive").region.box_;
            (
                target,
                (ox + b.x0 + b.width() / 2, oy + b.y0 + b.height() / 2),
            )
        };
        // The screen's own arm, and not the widget's method: that is where the write that greys
        // the object crosses the seam.
        let start = s
            .begin_item_drag(ui, target, at.0, at.1)
            .expect("the slot starts a drag");
        let w = s
            .inventory
            .item_list
            .as_ref()
            .expect("the shipped item list");
        let i = w
            .slots
            .iter()
            .position(|q| q.item == Some(thing))
            .expect("the thing has a slot");
        (start.drag_icon, target, w.slots[i].waiting)
    };
    c.tick(1);

    // The greying is on the **object**, which is what survives the list being rebuilt.
    let greyed_on_the_object = c
        .view()
        .expect_app()
        .objects()
        .world
        .tables
        .weenies
        .get(thing)
        .map(|w| w.waiting)
        == Some(true);
    let sent_before = c.view().expect_app().interaction().stats.requests_sent;

    // The drop, on the very slot it came from.
    {
        let (ui, s) = hud_gameplay(&mut c);
        s.handle_drop_release(ui, target, source);
    }
    c.tick(1);

    let nothing_was_asked = c.view().expect_app().interaction().stats.requests_sent == sent_before;
    let the_object_came_back = c
        .view()
        .expect_app()
        .objects()
        .world
        .tables
        .weenies
        .get(thing)
        .map(|w| w.waiting)
        == Some(false);
    let the_slot_came_back = {
        let (_, s) = hud_gameplay(&mut c);
        let w = s
            .inventory
            .item_list
            .as_ref()
            .expect("the shipped item list");
        !w.slots
            .iter()
            .find(|q| q.item == Some(thing))
            .expect("the thing kept its slot")
            .waiting
    };

    c.assert_behaviour(
        "ui.item-cell.dropping-a-thing-back-on-its-own-slot-sends-nothing-and-ghosts-nothing",
        move |_| {
            the_lift_greyed_it
                && greyed_on_the_object
                && nothing_was_asked
                && the_object_came_back
                && the_slot_came_back
        },
    );
    c.shutdown();
}
