use super::*;
use dereth_client_contract::book::{author_label, BookAction};

#[derive(Debug, Default)]
pub struct Book {}
/// The page text's font.
const PAGE_FONT: &str = "15-6";

/// Whether page text fits on the page: wrapped to the page's width inside its edges, no taller
/// than the page.
fn fits_on_page(text: &str) -> bool {
    crate::renderer::measure_text_height(PAGE_FONT, text, 253).is_none_or(|h| h <= 243)
}

impl Panel for Book {
    fn id(&self) -> &'static str {
        "book"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let height = crate::panels::side_height();
        let mut f = PanelFrame::new(300, height);
        f.image("06001398", rect(0, 0, 300, height as i32), true, false);
        for (did, r) in [
            ("06001279", rect(0, 0, 276, 30)),
            ("06001273", rect(0, 30, 300, 33)),
            ("06001270", rect(0, 306, 300, 32)),
            ("06001271", rect(0, 63, 22, 243)),
            ("06001272", rect(279, 63, 21, 243)),
            ("0600126E", rect(0, 338, 8, 18)),
            ("06001265", rect(0, 356, 300, 6)),
            ("0600126F", rect(22, 63, 257, 243)),
        ] {
            f.image(did, r, true, false);
        }
        image_button(
            &mut f,
            "close",
            rect(276, 0, 24, 30),
            [0x0600126C, 0x0600126D, 0x0600126C],
            true,
        );
        if let Some(b) = c.game.open_book() {
            let state = c.game.book_session();
            // The title is shown, never edited.
            label_color(&mut f, rect(22, 6, 234, 19), &b.title, "15-6", 0xff080808);
            image_button(
                &mut f,
                "previous",
                rect(0, 30, 54, 33),
                [0x06001269, 0x0600126A, 0x0600126B],
                state.current_page > 0 && !state.pending,
            );
            image_button(
                &mut f,
                "next",
                rect(235, 306, 65, 32),
                [0x06001266, 0x06001267, 0x06001268],
                state.current_page + 1 < b.max_num_pages as i32 && !state.pending,
            );
            if !state.pending && state.editable {
                // Written on the page's paper, not on a black field.
                let page = f.edit(
                    "text",
                    rect(22, 63, 257, 243),
                    &state.draft,
                    usize::MAX,
                    true,
                    true,
                );
                page.color = 0xff080808;
                page.background = None;
                page.font = PAGE_FONT.into();
            } else {
                label_color(
                    &mut f,
                    rect(22, 63, 257, 243),
                    &state.draft,
                    "15-6",
                    0xff080808,
                );
            }
            // The page list: "Page n" and the page's author, on the book's list art.
            f.control(
                "page",
                rect(7, 338, 293, 18),
                ControlKind::Choice {
                    options: (0..b.max_num_pages as usize)
                        .map(|i| {
                            let author = b
                                .pages
                                .get(i)
                                .map(|p| {
                                    author_label(&p.author_name, &p.author_account, b.viewer_is_psr)
                                })
                                .unwrap_or("(blank)".into());
                            format!("Page {}\t{author}", i + 1)
                        })
                        .collect(),
                    selected: usize::try_from(state.current_page).unwrap_or(0),
                },
                true,
            )
            .list_skin = Some(crate::panels::ListSkin::BOOK);
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let Some(b) = c.game.open_book() else {
            return if matches!(e, ControlEvent::Activate(ref id) if id == "close") {
                vec![PanelAction::Close]
            } else {
                vec![]
            };
        };
        let state = c.game.book_session();
        let book = b.book_id;
        let action = match e {
            ControlEvent::Edit { id, text } if id == "text" && state.editable && !state.pending => {
                if !fits_on_page(&text) {
                    return vec![];
                }
                BookAction::Edit {
                    book,
                    page: state.current_page,
                    text,
                }
            }
            ControlEvent::Select { id, index } if id == "page" => BookAction::Turn {
                book,
                page: i32::try_from(index).unwrap_or(i32::MAX),
            },
            ControlEvent::Value { id, value } if id == "page" => {
                BookAction::Turn { book, page: value }
            }
            ControlEvent::Activate(id) if id == "previous" => BookAction::Turn {
                book,
                page: state.current_page - 1,
            },
            ControlEvent::Activate(id) if id == "next" => BookAction::Turn {
                book,
                page: state.current_page + 1,
            },
            ControlEvent::Activate(id) if id == "close" => {
                return vec![
                    PanelAction::Game(UiRequest::Book(BookAction::Close { book })),
                    PanelAction::Close,
                ]
            }
            _ => return vec![],
        };
        vec![PanelAction::Game(UiRequest::Book(action))]
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use dereth_client_contract::view::BookView;
    #[test]
    fn the_title_is_shown_but_never_edited() {
        #[derive(Debug)]
        struct Open(BookView);
        impl dereth_client_contract::view::GameView for Open {
            fn open_book(&self) -> Option<BookView> {
                Some(self.0.clone())
            }
        }
        let game = Open(BookView {
            book_id: ObjectId(5),
            player_id: ObjectId(7),
            max_num_pages: 2,
            title: "Letter From Home".into(),
            ..Default::default()
        });
        let (state, pregame, keyboard, settings) = Default::default();
        let c = Context {
            now: dereth_primitives::LocalTime(0.0),
            game: &game,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &state,
        };
        let mut book = Book::default();
        book.event(ControlEvent::Tick, &c);
        let frame = book.frame(&c);
        assert!(frame.controls.iter().all(|control| control.id != "title"));
        assert!(frame.screen.commands.iter().any(|command| matches!(
            command,
            crate::Command::Text { text, .. } | crate::Command::TextBox { text, .. }
                if text == "Letter From Home"
        )));
        let edit = ControlEvent::Edit {
            id: "title".into(),
            text: "Renamed".into(),
        };
        assert!(book.event(edit, &c).is_empty());
        assert!(book
            .event(ControlEvent::Commit { id: "title".into() }, &c)
            .is_empty());
    }
    #[test]
    fn page_editability_comes_from_the_shared_session_without_a_privileged_bypass() {
        #[derive(Debug)]
        struct View(bool);
        impl GameView for View {
            fn open_book(&self) -> Option<BookView> {
                Some(BookView {
                    book_id: ObjectId(5),
                    max_num_pages: 1,
                    ..Default::default()
                })
            }
            fn book_session(&self) -> dereth_client_contract::book::BookSessionView {
                dereth_client_contract::book::BookSessionView {
                    current_page: 0,
                    draft: "Read me".into(),
                    editable: self.0,
                    ..Default::default()
                }
            }
        }
        for editable in [false, true] {
            let game = View(editable);
            let mut state = crate::panels::ClassicState::default();
            let (pregame, keyboard, settings) = Default::default();
            state.book_edit_privileged = true;
            let c = Context {
                now: dereth_primitives::LocalTime(0.0),
                game: &game,
                pregame: &pregame,
                keyboard: &keyboard,
                settings: &settings,
                map_teleport_allowed: false,
                classic: &state,
            };
            let mut panel = Book {};
            let out = panel.event(
                ControlEvent::Edit {
                    id: "text".into(),
                    text: "Changed".into(),
                },
                &c,
            );
            assert_eq!(!out.is_empty(), editable);
            assert_eq!(
                panel.frame(&c).controls.iter().any(|v| v.id == "text"),
                editable
            );
        }
    }
}
