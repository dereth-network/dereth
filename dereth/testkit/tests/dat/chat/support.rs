use super::*;
/// The body of the first server-to-client blob of `ROOMS_SESSION` carrying `opcode`, starting at
/// the opcode dword.
///
/// The blobs are found by walking the recording rather than by index, so a re-locked corpus moves
/// nothing here.
pub(super) fn recorded(opcode: u32) -> Vec<u8> {
    let corpus = Corpus::load(ROOMS_SESSION)
        .expect("the corpus decodes")
        .expect("the corpus holds the recording");
    for b in &corpus.blobs {
        if b.dir != Direction::ServerToClient {
            continue;
        }
        if b.opcode == opcode {
            return b.payload.clone();
        }
        // A game event carries its own header before the message it wraps.
        if b.opcode == 0xF7B0 && b.payload.len() >= 16 {
            let sub = u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes"));
            if sub == opcode {
                return b.payload[12..].to_vec();
            }
        }
    }
    panic!("{ROOMS_SESSION} carries no server-to-client {opcode:#x}");
}

/// A client with the shell up and no screen settled yet.
pub(super) fn a_bare_client(shell: bool) -> HeadlessClient {
    HeadlessClient::new(ClientSpec {
        shell,
        ui_mode: None,
        settle_frames: 0,
        ..ClientSpec::retail()
    })
}

/// Hand the client a description of its own player with the two listening options as given.
pub(super) fn describe(c: &mut HeadlessClient, general: bool, allegiance: bool) {
    let mut module = dereth_protocol::login::PlayerModule::default();
    if general {
        module.options2 |= 0x100;
    } else {
        module.options2 &= !0x100;
    }
    if allegiance {
        module.options |= 0x4000_0000;
    } else {
        module.options &= !0x4000_0000;
    }
    c.when(Inbound::event(SessionEvent::PlayerDescription(Box::new(
        dereth_protocol::login::LoginPlayerDescription {
            player_module: module,
            ..Default::default()
        },
    ))));
}

/// Put the gameplay screen up and settle it.
pub(super) fn to_gameplay(c: &mut HeadlessClient) {
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
}

/// One element of the shipped gameplay tree, by id.
pub(super) fn element(c: &HeadlessClient, id: ElementId) -> ElemHandle {
    let app = c.view().expect_app();
    let shell = app.ui().expect("the UI shell is up");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen is up");
    let root = any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .root()
        .expect("the gameplay screen's root");
    shell
        .ui
        .get_child_recursive(root, id)
        .expect("the element is in the shipped layout")
}

/// Everything the live chat log holds, as text.
pub(super) fn log_text(c: &mut HeadlessClient) -> String {
    let h = element(c, dereth_ui_screens::chat::window::LOG);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Pick the general channel out of the talk-to menu, as a player does.
pub(super) fn pick_general(c: &mut HeadlessClient, hand: &mut Hand) {
    hand.click_element(c, TALK_TO_BUTTON);
    let row = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell is up");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen is up");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        let row = screen
            .main_chat
            .menu_item(&shell.ui, GENERAL_ROW)
            .expect("the general row");
        assert_ne!(
            shell.ui.node(row).expect("the row is alive").state,
            StateId(0xD),
            "the general row must be pickable, or this scenario measures nothing"
        );
        row
    };
    hand.click_handle(c, row);
    assert_eq!(
        c.view().world().chat.talk_focus,
        dereth_client_model::chat::TalkFocus::General,
        "picking the row must move the talk focus"
    );
}

pub(super) fn word(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

/// Put `lines` into the shipped chat window, through the window's own delivery.
pub(super) fn into_the_window(
    c: &mut HeadlessClient,
    lines: &[dereth_ui_screens::chat::interface::ChatMessage],
) {
    for (n, m) in lines.iter().enumerate() {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        assert!(
            !gameplay.recv_display_final_string_info(ui, m).is_empty(),
            "line {n} reached a shipped chat window"
        );
    }
}

/// How many requests the client has produced so far, to count the next window from.
pub(super) fn mark(c: &HeadlessClient) -> usize {
    c.view().outbound().len()
}

/// Run `f` against the shipped gameplay screen and the shell it was laid out in.
pub(super) fn with_screen<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui::UiSystem, &mut GamePlayScreen) -> T,
) -> T {
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    let gameplay = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    f(&mut shell.ui, gameplay)
}

/// Open the toolbar panel that holds an option page, then that page's own tab, by pressing what a
/// player presses.
pub(super) fn open_the_options_page(
    c: &mut HeadlessClient,
    hand: &mut Hand,
    page_element: ElementId,
) {
    let (panel_id, container, page) = with_screen(c, |ui, s| {
        let root = s.root().expect("the screen root");
        let page = ui
            .get_child_recursive(root, page_element)
            .expect("the page is in the shipped tree");
        let info = s
            .panels
            .pages
            .iter()
            .copied()
            .find(|p| {
                let mut h = Some(page);
                while let Some(cur) = h {
                    if cur == p.handle {
                        return true;
                    }
                    h = ui.parent(cur);
                }
                false
            })
            .expect("the page sits inside a toolbar panel");
        (info.panel_id, info.handle, page)
    });
    let button = with_screen(c, |_, s| {
        s.toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == panel_id)
            .unwrap_or_else(|| panic!("the shipped toolbar has no button for panel {panel_id}"))
            .handle
    });
    hand.click_handle(c, button);
    let tab = with_screen(c, |ui, _| {
        let tab = ui
            .node(container)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .and_then(|p| p.page_to_tab.get(&page_element).copied())
            .expect("the options panel's tab table names the page");
        ui.get_child_recursive(container, tab)
            .expect("the tab caption element")
    });
    hand.click_handle(c, tab);
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .is_visible(page),
        "the tab must be up"
    );
}

// ---------------------------------------------------------------------------------------------
// Where the caret goes after a line is sent
// ---------------------------------------------------------------------------------------------
//
// **Both directions, every time.** Asserting only "the entry has lost the caret" passes on a build
// where it can never have it at all, so each arm below asks where a keystroke goes on **both**
// sides of the edge: with the box blurred a movement key walks the character, and with it focused
// the same character lands in the box.

/// The key the host resolves a named key to.
pub(super) fn key_of(code: winit::keyboard::KeyCode) -> dereth_client::platform::keys::Key {
    dereth_client::platform::window::key_from_key_code(code)
        .expect("the host has a scan code for this key")
}

pub(super) fn focus_of(c: &HeadlessClient) -> Option<ElemHandle> {
    c.view()
        .expect_app()
        .ui()
        .expect("the shell is up")
        .ui
        .focus_element()
}

/// The main chat window's own history, which is the client's evidence that a line really left the
/// box: an empty entry is not remembered, so a line in the history is a line that was sent.
pub(super) fn chat_history(c: &mut HeadlessClient) -> Vec<String> {
    c.view()
        .world()
        .chat
        .entries
        .get(&8)
        .map_or_else(Vec::new, |entry| entry.history().to_vec())
}

/// The log's letters as runs of one colour -- what a player actually sees, read off the element
/// rather than off the colour table.
pub(super) fn colour_runs(c: &mut HeadlessClient) -> Vec<(String, u32)> {
    let log = element(c, dereth_ui_screens::chat::window::LOG);
    with_screen(c, |ui, _| {
        let t = ui.text_element_mut(log).expect("the log is a text element");
        let mut out: Vec<(String, u32)> = Vec::new();
        for g in &t.glyphs.glyphs {
            let ch = char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}');
            match out.last_mut() {
                Some((s, col)) if *col == g.color => s.push(ch),
                _ => out.push((ch.to_string(), g.color)),
            }
        }
        out
    })
}

/// The whole drawn log, and the colour of the run carrying `needle`.
pub(super) fn drawn_line(c: &mut HeadlessClient, needle: &str) -> (String, Option<u32>) {
    let all = colour_runs(c);
    let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
    let colour = all
        .iter()
        .find(|(s, _)| s.contains(needle))
        .map(|(_, col)| *col);
    (joined, colour)
}

/// A client in the world with its player described, ready to be spoken to.
pub(super) fn a_client_listening() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    // A client that is in the world has had a description of its player, and the chat windows take
    // their stored settings off it.
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    c.tick(1);
    c
}

/// One message, through the production sender over a mock transport. **Nothing is sent.**
pub(super) fn on_the_wire<M: dereth_protocol::Message>(m: &M) -> (NetQueue, Vec<u8>) {
    let mut s = Session::new(MockTransport::new());
    s.send_action(m).expect("the message encodes");
    assert_eq!(s.transport.sent.len(), 1, "one action, one datagram");
    let b = &s.transport.sent[0];
    (b.queue, b.payload.clone())
}

/// The main chat window's entry -- the one the tell fills.
pub(super) fn main_chat_entry(c: &HeadlessClient) -> ElemHandle {
    let app = c.view().expect_app();
    let any: &dyn std::any::Any = app
        .ui()
        .expect("the shell")
        .flow
        .current()
        .expect("a screen");
    any.downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .chat_windows
        .first()
        .and_then(|w| w.entry)
        .expect("the main window's entry")
}

/// The text of one element.
pub(super) fn element_text_of(c: &mut HeadlessClient, h: ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}
