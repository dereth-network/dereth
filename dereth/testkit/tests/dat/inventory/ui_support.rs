use super::*;

/// The live gameplay screen's root, and the UI system it lives in.
pub(super) fn gameplay_root(app: &mut App) -> (&mut UiSystem, ElemHandle) {
    let shell = app.ui_mut().expect("the UI shell");
    let root = {
        let screen = shell.flow.current_mut().expect("a current screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("the gameplay screen's root")
    };
    (&mut shell.ui, root)
}

// The request queue a scenario reads: the client UI's own, handed in by the caller.

pub(super) fn clear_requests(requests: &mut dereth_ui_screens::requests::Outbox) {
    requests.clear();
}

pub(super) fn take_requests(
    requests: &mut dereth_ui_screens::requests::Outbox,
) -> Vec<dereth_client_contract::UiRequest> {
    requests.take()
}

/// The live UI system and the live gameplay screen, together -- the screen and not only a panel,
/// because the cooldown exemption is a method on the screen and driving the widget's own would
/// leave the wiring that carries it unasserted.
pub(super) fn parts(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell");
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let g = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    (&mut shell.ui, g)
}

pub(super) fn element_of(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let app = c.view().expect_app();
    let shell = app.ui().expect("the UI shell");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
    let root = any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .root()
        .expect("the gameplay root");
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the tree"))
}

/// Run frames and collect everything that reached the wire across them.
pub(super) fn pump(c: &mut HeadlessClient) -> Vec<Request> {
    let mut sent = Vec::new();
    for _ in 0..4 {
        c.tick(1);
        sent.extend(
            c.view()
                .expect_app()
                .interaction()
                .last_sent
                .iter()
                .cloned(),
        );
    }
    sent
}

pub(super) fn click(c: &mut HeadlessClient, id: ElementId) -> Vec<Request> {
    let h = element_of(c, id);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .broadcast_element_message(
            h,
            dereth_ui::msg::element::id::BUTTON_CLICKED,
            dereth_ui::focus::action::PRIMARY_CLICK,
            0,
        );
    pump(c)
}

pub(super) fn press(c: &mut HeadlessClient, h: ElemHandle) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .broadcast_element_message(
            h,
            dereth_ui::msg::element::id::MOUSE_PRESS,
            dereth_ui::focus::action::PRIMARY_CLICK,
            0,
        );
    let _ = pump(c);
}

pub(super) fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell is up");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen"),
    )
}

pub(super) fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let (ox, oy) = ui.screen_origin(h);
    let b = ui.node(h).expect("alive").region.box_;
    (ox + b.width() / 2, oy + b.height() / 2)
}

pub(super) fn text_of(c: &mut HeadlessClient, id: ElementId) -> String {
    let h = element_of(c, id);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// Where a live element is, as a pointer target.
///
/// A pack slot cannot be named by its element id: the grid builds a hundred of them from one
/// template, so every slot carries the same shipped id and `Target::Element` would always answer
/// the first. The point is the slot's own centre.
pub(super) fn point_of(c: &mut HeadlessClient, h: ElemHandle) -> Target {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let (x, y) = centre(ui, h);
    Target::Point(ScreenPoint::new(x, y))
}

pub(super) fn centre_of(c: &mut HeadlessClient, h: ElemHandle) -> (i32, i32) {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    centre(ui, h)
}
