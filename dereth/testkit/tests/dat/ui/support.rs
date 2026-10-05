//! UI fixtures and scenarios for support.

use super::*;
// =============================================================================================
// options.support.* and urgent-assistance.* -- the two asks that reached nothing
//
// The plumbing's own good manners -- an arm that claims its own ask and hands every other one
// back -- is a leg of the first scenario rather than a row.
//
// The window is built out of the shipped data and driven with a real press on the control's own
// rectangle; what it asks for is then handed to the client's own consumer. Nothing calls a
// handler by hand. **No browser is opened, no box is put on a real screen and no datagram leaves
// the process**: the door to the desktop is stubbed before any press, and the one send goes to a
// transport that keeps what it is given.
// =============================================================================================

/// The shipped element tree, with the real text tables behind it.
fn a_shipped_tree() -> (
    UiSystem,
    dereth_ui_screens::screens::gameplay::GamePlayScreen,
    dereth_ui_screens::panels::remaining::RemainingPanels,
) {
    use dereth_primitives::AssetSource as _;
    use dereth_ui::framework::{DidMapperResolver, Screen as _};

    let dir = dereth_dat::testing::dat_dir();
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail data files open");
    let master_id = dereth_primitives::DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("the master property record");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let source = std::rc::Rc::new(store);
    let resolver =
        std::rc::Rc::new(DidMapperResolver::load_via_master(source.as_ref()).expect("the mapper"));
    dereth_ui_screens::env::install(&mut ui, source, resolver);

    let mut s = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let root = s.root().expect("the gameplay root");
    let mut panels = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    panels.post_init(&mut ui, root);
    ui.drain_outbox();
    ui.requests.clear();
    (ui, s, panels)
}

/// Show this element and every ancestor: a page comes up hidden and a hidden ancestor cannot be
/// pressed.
fn reveal_for_press(ui: &mut UiSystem, mut h: ElemHandle) {
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    ui.drain_outbox();
}

/// One frame of the message pass, to both of the things that answer one.
fn pump_messages(
    ui: &mut UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    panels: &mut dereth_ui_screens::panels::remaining::RemainingPanels,
) {
    use dereth_ui::framework::Screen as _;
    let view = dereth_ui_screens::view::EmptyGameView;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let dereth_ui::Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                panels.on_element_message(ui, &msg, &view);
            }
        }
    }
}

/// A real press in the middle of a control's own rectangle, and what it asked for.
fn press_and_take(
    ui: &mut UiSystem,
    s: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    panels: &mut dereth_ui_screens::panels::remaining::RemainingPanels,
    parent: ElemHandle,
    id: ElementId,
) -> Vec<dereth_ui_screens::UiRequest> {
    let h = ui
        .get_child_recursive(parent, id)
        .unwrap_or_else(|| panic!("{:#010X} is in the shipped tree", id.0));
    reveal_for_press(ui, h);
    let b = ui.node(h).expect("the control").region.box_;
    let (ox, oy) = ui.screen_origin(h);
    let (x, y) = (ox + b.width() / 2, oy + b.height() / 2);
    ui.requests.clear();
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump_messages(ui, s, panels);
    ui.requests.take()
}

/// The one options page that carries the support buttons.
fn the_support_page(ui: &UiSystem) -> ElemHandle {
    let all: Vec<ElemHandle> = ui
        .element_list()
        .iter()
        .copied()
        .filter(|h| {
            ui.node(*h)
                .is_some_and(|n| n.ty() == dereth_ui_screens::element_types::ty::GAMEPLAY_OPTIONS)
        })
        .collect();
    assert_eq!(
        all.len(),
        1,
        "the shipped tree carries exactly one of those pages"
    );
    all[0]
}

// ---------------------------------------------------------------------------------------------
// options.support.each-support-button-opens-its-in-game-form
// ---------------------------------------------------------------------------------------------

/// Both buttons, each to its form, and neither to the desktop's browser.
pub(super) fn each_support_button_opens_its_in_game_form() {
    use {
        dereth_client_runtime::app::apply_open_url_requests,
        dereth_client_runtime::app::record_shell_calls,
    };

    let (mut ui, mut s, mut panels) = a_shipped_tree();
    let page = the_support_page(&ui);
    let mut opened = true;
    for (id, form) in [
        (
            support_button::SUPPORT_TICKET_UPPER,
            dereth_ui_screens::panels::urgent_assistance::PANEL_TYPE,
        ),
        (
            support_button::SUPPORT_TICKET_LOWER,
            dereth_ui_screens::panels::abuse::PANEL_TYPE,
        ),
    ] {
        let asked = press_and_take(&mut ui, &mut s, &mut panels, page, id);
        opened &= !asked
            .iter()
            .any(|r| matches!(r, dereth_ui_screens::UiRequest::OpenUrl(_)));
        // The form is the one element of its type.
        opened &= ui
            .element_list()
            .iter()
            .any(|h| ui.node(*h).is_some_and(|n| n.ty() == form) && ui.is_visible(*h));
        // Nothing reaches the desktop.
        record_shell_calls();
        let (_, calls) = apply_open_url_requests(asked);
        opened &= calls.is_empty();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.support.each-support-button-opens-its-in-game-form",
        move |_| opened,
    );
}

// ---------------------------------------------------------------------------------------------
// options.support.a-browser-that-will-not-open-says-so-in-a-box-with-the-address-in-it
// ---------------------------------------------------------------------------------------------

/// The other half: a desktop that refuses.
pub(super) fn a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it() {
    use {
        dereth_client_runtime::app::apply_open_url_requests,
        dereth_client_runtime::app::record_shell_calls_answering,
        dereth_client_runtime::app::shell_error_text, dereth_client_runtime::app::ShellCall,
    };

    const URL: &str = "https://example.invalid/support";
    let request = || vec![dereth_ui_screens::UiRequest::OpenUrl(URL)];

    record_shell_calls_answering(0);
    let (_, calls) = apply_open_url_requests(request());
    let the_box = calls
        == vec![
            ShellCall::Open {
                url: URL.to_owned(),
                result: 0,
            },
            ShellCall::ErrorBox {
                title: "Asheron's Call Error".to_owned(),
                text: format!(
                    "An error occurred while trying to launch your web browser. \
                     (Error code 0)\nThe web site to submit an urgent assistance request is \
                     listed below. Please go there to complete your request.\n{URL}\n"
                ),
            },
        ]
        && SHELL_EXECUTE_ERROR_TITLE == "Asheron's Call Error"
        && shell_error_text(0, URL)
            == match &calls[1] {
                ShellCall::ErrorBox { text, .. } => text.clone(),
                other => panic!("{other:?}"),
            };

    // The line between a refusal and a success, read on both sides of it so an off-by-one cannot
    // hide: the value below it puts the box up and the value above it does not.
    record_shell_calls_answering(32);
    let refuses = apply_open_url_requests(request()).1.len() == 2;
    record_shell_calls_answering(33);
    let succeeds = apply_open_url_requests(request()).1.len() == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.support.a-browser-that-will-not-open-says-so-in-a-box-with-the-address-in-it",
        move |_| the_box && refuses && succeeds,
    );
}

// ---------------------------------------------------------------------------------------------
// urgent-assistance.send.the-report-goes-out-on-the-help-channel
// ---------------------------------------------------------------------------------------------

/// The bytes a report really puts on the wire, written out here rather than read back through the
/// same writer that produced them.
const HELP_ME: [u8; 28] = [
    0xB1, 0xF7, 0x00, 0x00, // the ordered action envelope
    0x01, 0x00, 0x00, 0x00, // the first action of the session
    0x47, 0x01, 0x00, 0x00, // what the message is
    0x00, 0x04, 0x00, 0x00, // the help channel
    0x07, 0x00, // seven characters
    b'h', b'e', b'l', b'p', b' ', b'm', b'e', //
    0x00, 0x00, 0x00, // padding to the next four
];

/// The whole window, from the warning page to the datagram.
pub(super) fn the_urgent_assistance_report_goes_out_on_the_help_channel() {
    use dereth_client_runtime::interaction::Interaction;
    use dereth_client_runtime::requests::send_request;

    let (mut ui, mut s, mut panels) = a_shipped_tree();
    let panel = panels
        .urgent_assistance
        .panel
        .expect("the window is in the tree");
    ui.set_state(panel, ua::PAGE_WARNING);
    let _ = press_and_take(&mut ui, &mut s, &mut panels, panel, ua::NEXT_BUTTON);

    // Type the report into the window's own box and raise what a typed box raises.
    let box_h = ui
        .get_child_recursive(panel, ua::ENTRY_BOX)
        .expect("the box is a child of the window");
    if let Some(t) = ui.text_element_mut(box_h) {
        t.set_text("help me");
    }
    ui.broadcast_element_message(box_h, dereth_ui::msg::element::id::TEXT_CHANGED, 0, 0);
    pump_messages(&mut ui, &mut s, &mut panels);

    let asked = press_and_take(&mut ui, &mut s, &mut panels, panel, ua::CONTINUE_BUTTON);
    let queued = asked.iter().any(|r| {
        matches!(
            r,
            dereth_ui_screens::UiRequest::ChannelBroadcast { channel, text }
                if *channel == ua::HELP_CHANNEL && text == "help me"
        )
    }) && ua::HELP_CHANNEL == 0x0400;

    // ...and on to the client's own consumer, and then its own sender.
    let mut inter = Interaction::new();
    let mut game = dereth_client_model::World::new();
    inter.queue(Vec::new(), asked);
    let claimed = inter
        .run_ui_requests(&mut game, false, dereth_primitives::ServerTime(100.0))
        .is_empty();
    let requests = inter.take_pending_requests();
    let one_request = requests.len() == 1
        && matches!(&requests[0], dereth_client_model::Request::ChannelBroadcast(m)
            if m.channel == 0x0400 && m.message == "help me");

    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let sent = send_request(&mut session, &requests[0]);
    let on_the_wire = sent
        && session.transport.sent.len() == 1
        && (
            session.transport.sent[0].queue,
            session.transport.sent[0].ordered,
        ) == (dereth_primitives::NetQueue::Weenie, true)
        && session.transport.sent[0].payload == HELP_ME;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "urgent-assistance.send.the-report-goes-out-on-the-help-channel",
        move |_| queued && claimed && one_request && on_the_wire,
    );
}

// ---------------------------------------------------------------------------------------------
// urgent-assistance.send.the-refusal-that-applies-to-a-typed-command-does-not-apply-here
// ---------------------------------------------------------------------------------------------

/// A trap the window must not fall into: the help channel is refused to a typed command, which is
/// what makes asking for help a command rather than a channel -- and the window is the one thing
/// in the client that is allowed to send on it.
pub(super) fn the_refusal_for_a_typed_command_does_not_apply_to_the_window() {
    use dereth_client_runtime::interaction::Interaction;

    let still_refused = !dereth_client_model::chat::channel_command_broadcasts_on(0x0400);

    let mut inter = Interaction::new();
    let mut game = dereth_client_model::World::new();
    inter.queue(
        Vec::new(),
        vec![dereth_ui_screens::UiRequest::ChannelBroadcast {
            channel: 0x0400,
            text: "help me".into(),
        }],
    );
    let claimed = inter
        .run_ui_requests(&mut game, false, dereth_primitives::ServerTime(100.0))
        .is_empty();
    let not_refused = inter.take_pending_requests().len() == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "urgent-assistance.send.the-refusal-that-applies-to-a-typed-command-does-not-apply-here",
        move |_| still_refused && claimed && not_refused,
    );
}
