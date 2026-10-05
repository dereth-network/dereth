use super::*;

// ---------------------------------------------------------------------------------------------
// The salvage window's drag-out.
// ---------------------------------------------------------------------------------------------

pub(super) const SALVAGE_PLAYER: ObjectId = ObjectId(0x5000_0001);
pub(super) const SALVAGE_TOOL: ObjectId = ObjectId(0x5000_0002);
pub(super) const SALVAGE_RING: ObjectId = ObjectId(0x5000_0003);

pub(super) fn seed_salvage(w: &mut dereth_client_model::World) {
    use dereth_rules::weenie::item_type;

    w.set_player(SALVAGE_PLAYER);
    let mut me = dereth_client_model::Weenie::new(SALVAGE_PLAYER);
    me.valid = true;
    me.pwd.bitfield |= dereth_rules::weenie::bitfield::PLAYER;
    me.pwd.items_capacity = Some(24);
    w.tables.weenies.insert(SALVAGE_PLAYER, me);

    let mut tool = dereth_client_model::Weenie::new(SALVAGE_TOOL);
    tool.valid = true;
    tool.pwd.name = "Iron Salvaging Kit".into();
    tool.pwd.icon_id = 0x0600_1234;
    tool.pwd.obj_type = item_type::TINKERING_TOOL;
    tool.pwd.container_id = Some(SALVAGE_PLAYER);
    w.tables.weenies.insert(SALVAGE_TOOL, tool);

    let mut ring = dereth_client_model::Weenie::new(SALVAGE_RING);
    ring.valid = true;
    ring.pwd.name = "Black Opal Ring".into();
    ring.pwd.icon_id = 0x0600_1234;
    ring.pwd.obj_type = item_type::JEWELRY;
    ring.pwd.material_type = Some(0x10);
    ring.pwd.structure = Some(40);
    ring.pwd.container_id = Some(SALVAGE_PLAYER);
    w.tables.weenies.insert(SALVAGE_RING, ring);

    w.tables.inventories.insert(
        SALVAGE_PLAYER,
        dereth_client_model::objects::ObjectInventory {
            container: ObjectId(0),
            items: vec![SALVAGE_TOOL, SALVAGE_RING],
            containers: Vec::new(),
            placements: Vec::new(),
        },
    );
}

pub(super) fn pack_slot(c: &mut HeadlessClient, item: ObjectId) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the pack grid")
        .slots
        .iter()
        .find(|slot| slot.item == Some(item))
        .unwrap_or_else(|| panic!("{item:?} has no pack tile"))
        .handle
}

fn salvage_tile(c: &HeadlessClient) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .salvage
        .list
        .as_ref()
        .expect("the salvage list is bound")
        .slots
        .first()
        .expect("the salvage list has a row")
        .handle
}

pub(super) fn open_pack(c: &mut HeadlessClient) {
    let panel_id = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .panels
            .pages
            .iter()
            .find(|page| page.element == INVENTORY_PAGE)
            .map(|page| page.panel_id)
            .expect("the inventory page is present")
    };
    let (ui, screen) = gameplay_screen(c.app_mut());
    screen.recv_set_panel_visibility(ui, panel_id, true);
    c.tick(2);
}

pub(super) fn use_the_tool(c: &mut HeadlessClient) {
    let tile = pack_slot(c, SALVAGE_TOOL);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let (x, y) = centre(ui, tile);
    ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
    ui.mouse_down(0x0A, x, y);
    c.tick(3);
}

fn pick_up(c: &mut HeadlessClient, tile: ElemHandle, t: f64) {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let (x, y) = centre(ui, tile);
    ui.mouse_move(dereth_primitives::LocalTime(t), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_move(dereth_primitives::LocalTime(t + 0.05), x + 8, y + 8);
    c.tick(1);
}

fn put_the_ring_in(c: &mut HeadlessClient) {
    let from = pack_slot(c, SALVAGE_RING);
    let root = gameplay_screen(c.app_mut()).1.root().expect("a root");
    let to = gameplay_screen(c.app_mut())
        .0
        .get_child_recursive(root, salvage::LIST)
        .expect("the salvage list is in the shipped tree");
    pick_up(c, from, 2.0);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let (x, y) = centre(ui, to);
    ui.mouse_move(dereth_primitives::LocalTime(2.1), x, y);
    c.tick(1);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    c.tick(2);
    assert_eq!(
        c.view().expect_app().hud().panels.salvage.displayed,
        vec![SALVAGE_RING],
        "the premise: the ring is in the salvage window"
    );
}

/// A client with the salvage window open and one row in it.
fn a_client_with_a_salvage_row() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    seed_salvage(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    use_the_tool(&mut c);
    put_the_ring_in(&mut c);
    c
}

/// A row dragged out of the salvage window comes off it, and a click does not.
pub(super) fn a_salvage_row_dragged_out_leaves_the_list() {
    let mut c = a_client_with_a_salvage_row();

    let tile = salvage_tile(&c);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let (x, y) = centre(ui, tile);
    ui.mouse_move(dereth_primitives::LocalTime(3.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    c.tick(1);
    let a_click_is_not_a_drag =
        c.view().expect_app().hud().panels.salvage.displayed == vec![SALVAGE_RING];

    let tile = salvage_tile(&c);
    pick_up(&mut c, tile, 4.0);
    let (item, flags) = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let proxy = ui
            .drag_state()
            .element
            .expect("the salvage row started a drag");
        let info = inq_drop_icon_info(ui, proxy);
        (info.item, info.flags)
    };
    let button = c
        .view()
        .expect_app()
        .hud()
        .panels
        .salvage
        .salvage_button
        .expect("bound");
    let button_state = gameplay_screen(c.app_mut())
        .0
        .node(button)
        .expect("alive")
        .state;
    let app = c.view().expect_app();
    let taken_off = item == Some(SALVAGE_RING)
        && flags & drag_flags::IS_SALVAGE == drag_flags::IS_SALVAGE
        && app.hud().panels.salvage.displayed.is_empty()
        && app.hud().panels.salvage.items_removed == 1
        && app.hud().panels.salvage.button == SalvageButton::Disabled
        && button_state == dereth_ui::StateId(0x0D)
        && app.interaction().last_sent.is_empty();

    c.assert_behaviour(
        "salvage.row.dragging-one-out-takes-it-off-the-list-and-sends-nothing",
        { move |_| a_click_is_not_a_drag && taken_off },
    );
    c.shutdown();
}

/// A refused drag naming the whole list starts nothing and removes nothing.
pub(super) fn a_refusal_naming_the_list_starts_nothing() {
    let mut c = a_client_with_a_salvage_row();
    let slot = salvage_tile(&c);
    let list = c
        .view()
        .expect_app()
        .hud()
        .panels
        .salvage
        .list
        .as_ref()
        .expect("the salvage list is bound")
        .handle;

    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let (x, y) = centre(ui, slot);
        let (ox, oy) = ui.screen_origin(list);
        let point = dereth_ui::msg::element::MessagePoint {
            window: (x, y),
            element: (x - ox, y - oy),
        };
        ui.broadcast_element_message_at(
            list,
            dereth_ui::msg::element::id::DRAG_REJECTED,
            0,
            0,
            point,
        );
    }
    c.tick(1);

    let app = c.view().expect_app();
    let untouched = app.hud().panels.salvage.displayed == vec![SALVAGE_RING]
        && app.hud().panels.salvage.items_removed == 0;
    let no_drag = gameplay_screen(c.app_mut())
        .0
        .drag_state()
        .element
        .is_none();

    c.assert_behaviour(
        "salvage.row.a-message-naming-the-whole-list-starts-no-drag",
        { move |_| untouched && no_drag },
    );
    c.shutdown();
}

/// A drag the shell refuses still takes the row off, and letting go does not put it back.
///
/// The premise is asserted first: the row the shipped screen actually ships **cannot** reach that
/// refusal, which is why the scenario has to force it and why forcing it is not inventing a state
/// the client could not be in from the other direction.
pub(super) fn a_refused_drag_still_takes_the_salvage_row_off() {
    let mut c = a_client_with_a_salvage_row();

    {
        let slot = salvage_tile(&c);
        let icon = c
            .view()
            .expect_app()
            .hud()
            .panels
            .salvage
            .list
            .as_ref()
            .expect("bound")
            .slots
            .first()
            .expect("a row")
            .drag_icon
            .expect("the row has a drag icon");
        let (ui, _screen) = gameplay_screen(c.app_mut());
        assert!(
            ui.node(icon).expect("alive").flags.dragable(),
            "the premise: the shipped drag icon is draggable, so the shell accepts it"
        );
        assert_eq!(
            ui.node(slot)
                .expect("alive")
                .merged_properties()
                .get_bool(dereth_ui::props::attr::NO_DRAG_PROXY),
            Some(false),
            "the premise: the shipped row does not forbid the proxy"
        );
    }

    let slot = salvage_tile(&c);
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.set_attribute_bool(slot, dereth_ui::props::attr::NO_DRAG_PROXY, true);
    }
    let slot = salvage_tile(&c);
    pick_up(&mut c, slot, 4.0);

    let nothing_on_the_cursor = gameplay_screen(c.app_mut())
        .0
        .drag_state()
        .element
        .is_none();
    let app = c.view().expect_app();
    let gone = app.hud().panels.salvage.displayed.is_empty()
        && app.hud().panels.salvage.items_removed == 1
        && app.hud().panels.salvage.button == SalvageButton::Disabled
        && app.interaction().last_sent.is_empty();

    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, 10, 10, false);
    }
    c.tick(1);
    let stays_gone = c
        .view()
        .expect_app()
        .hud()
        .panels
        .salvage
        .displayed
        .is_empty();

    c.assert_behaviour(
        "salvage.row.a-drag-the-shell-refuses-still-takes-the-row-off-and-letting-go-does-not-put-it-back",
        move |_| nothing_on_the_cursor && gone && stays_gone,
    );
    c.shutdown();
}
