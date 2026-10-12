//! A web address in a chat line is a link in each of the modern interface's five chat windows: its
//! letters are drawn in the color a clickable name is drawn in, the rest of the line keeps its
//! own color, a click on it asks for the address to be opened, and a copy of the line is its
//! plain text.
//! Fixture: the shipped gameplay layout loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::UiRequest;
use dereth_ui::framework::Screen;
use dereth_ui::text::tag::TagKind;
use dereth_ui::UiSystem;

use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const URL: &str = "https://example.com/news?id=7";

fn screen() -> (UiSystem, GamePlayScreen) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let _ = ui.drain_outbox();
    ui.requests.clear();
    (ui, s)
}

/// Behaviour: chat.links.a-web-address-in-the-modern-chat-log-is-a-link
#[test]
fn a_web_address_in_a_chat_line_is_a_link_in_every_window_and_a_click_asks_to_open_it() {
    let (mut ui, mut s) = screen();
    let line = format!("Ana says, \"read {URL}.\"");
    for i in 0..s.chat_windows.len() {
        let w = s.chat_windows[i];
        let m = ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: 2,
            body: line.clone(),
            prefix: None,
            window: s.chat[i].window_id,
        };
        w.recv_display_final_string_info(&mut ui, &mut s.chat[i], &m);
        let log = w.log.expect("the window's log");
        let t = ui.text_element_mut(log).expect("the log's text");
        assert_eq!(t.glyphs.inq_text(false), line, "window {i}");
        let start = line.find("https").expect("the address");
        let tag_color = t.tag_font_color;
        for (g, glyph) in t.glyphs.glyphs.iter().enumerate() {
            let in_link = (start..start + URL.len()).contains(&g);
            assert_eq!(
                t.tag_at(g).map(|tag| (tag.kind, tag.data.clone())),
                in_link.then(|| (TagKind::WebLink, URL.to_owned())),
                "window {i}, glyph {g}"
            );
            assert_eq!(
                glyph.color == tag_color,
                in_link,
                "window {i}, glyph {g}: only the link takes the tag color"
            );
        }
    }

    // The main window: a press and release over the link asks to open it, whole.
    let log = s.chat_windows[0].log.expect("the main window's log");
    let screen = ui.screen_box(log);
    let t = ui.text_element_mut(log).expect("the log's text");
    let over = (screen.y0..screen.y1)
        .flat_map(|y| (screen.x0..screen.x1).map(move |x| (x, y)))
        .find(|&(x, y)| t.web_link_at_xy(screen, x, y).is_some())
        .expect("the link is drawn in the log");
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, over.0, over.1);
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        over.0,
        over.1,
        false,
    );
    let _ = ui.drain_outbox();
    assert_eq!(
        ui.requests.take(),
        [UiRequest::ChatLinkClicked(URL.to_owned())]
    );

    // Selected and copied, the line is its plain text.
    let t = ui.text_element_mut(log).expect("the log's text");
    t.bits.set_selectable(true);
    t.select_all();
    assert_eq!(t.selected_text(), line);
}
