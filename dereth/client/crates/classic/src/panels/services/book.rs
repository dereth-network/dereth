use super::*;
use crate::int::u32_from;

#[derive(Debug, Default)]
pub struct Book {
    opening: Option<(ObjectId, u64)>,
    last: Option<BookView>,
    pages: Option<Vec<dereth_client_contract::view::BookPageView>>,
    page: usize,
    text: String,
    dirty: bool,
    pending: bool,
    data_edge: u64,
    add_edge: u64,
}
fn blank(s: &str) -> bool {
    s.bytes().all(|c| matches!(c, b' ' | b'\n' | 0))
}
fn editable(b: &BookView, p: usize, privileged: bool) -> bool {
    b.pages
        .get(p)
        .is_some_and(|p| p.author_id == b.player_id || privileged || p.ignore_author != 0)
}
impl Book {
    fn effective(&self, mut b: BookView) -> BookView {
        if self.opening == Some((b.book_id, b.opening)) {
            if let Some(p) = &self.pages {
                b.pages = p.clone();
            }
        }
        b
    }

    fn load(&mut self, b: &BookView, out: &mut Vec<PanelAction>) {
        self.dirty = false;
        if let Some(p) = b.pages.get(self.page) {
            if let Some(t) = &p.text {
                self.text = t.clone();
                self.pending = false;
            } else {
                self.text.clear();
                self.pending = true;
                out.push(PanelAction::Game(UiRequest::BookPageData {
                    book: b.book_id,
                    page: u32_from(self.page),
                }));
            }
        } else {
            self.text.clear();
            self.pending = true;
            out.push(PanelAction::Game(UiRequest::BookAddPage {
                book: b.book_id,
            }));
        }
    }
    fn save(&mut self, b: &BookView, privileged: bool, out: &mut Vec<PanelAction>) -> bool {
        if self.pending || !editable(b, self.page, privileged) {
            return false;
        }
        if blank(&self.text) && b.pages[self.page].author_id == b.player_id {
            out.push(PanelAction::Game(UiRequest::BookDeletePage {
                book: b.book_id,
                page: u32_from(self.page),
            }));
            self.dirty = false;
            return true;
        }
        if self.dirty {
            out.push(PanelAction::Game(UiRequest::BookModifyPage {
                book: b.book_id,
                page: u32_from(self.page),
                text: self.text.clone(),
            }));
            if let Some(p) = self.pages.as_mut().and_then(|p| p.get_mut(self.page)) {
                p.text = Some(self.text.clone());
            }
            self.dirty = false;
        }
        false
    }
    fn turn(&mut self, b: &BookView, mut next: usize, privileged: bool) -> Vec<PanelAction> {
        if self.pending || next == self.page || next >= b.max_num_pages as usize {
            return vec![];
        }
        if next > b.pages.len()
            || (next > self.page
                && self.page + 1 == b.pages.len()
                && blank(&self.text)
                && b.pages
                    .get(self.page)
                    .is_some_and(|p| p.author_id == b.player_id))
        {
            return vec![PanelAction::Host(HostAction::LocalFeedback {
                severity: crate::panels::FeedbackSeverity::Warning,
                text: format!("{} is already open to a blank page", b.object_name),
            })];
        }
        let mut out = vec![];
        let mut working = b.clone();
        if self.save(b, privileged, &mut out) {
            working.pages.remove(self.page);
            if self.page < next {
                next -= 1;
            }
        }
        if working.pages.len() == b.pages.len() {
            if let Some(p) = working.pages.get_mut(self.page) {
                if editable(b, self.page, privileged) {
                    p.text = Some(self.text.clone());
                }
            }
        }
        self.page = next;
        self.pages = Some(working.pages.clone());
        self.load(&working, &mut out);
        out
    }
}
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
        if let Some(b) = c.game.open_book().map(|b| self.effective(b)) {
            // The title is shown, never edited.
            label_color(&mut f, rect(22, 6, 234, 19), &b.title, "15-6", 0xff080808);
            image_button(
                &mut f,
                "previous",
                rect(0, 30, 54, 33),
                [0x06001269, 0x0600126A, 0x0600126B],
                self.page > 0 && !self.pending,
            );
            image_button(
                &mut f,
                "next",
                rect(235, 306, 65, 32),
                [0x06001266, 0x06001267, 0x06001268],
                self.page + 1 < (b.max_num_pages as usize) && !self.pending,
            );
            if !self.pending && editable(&b, self.page, c.classic.book_edit_privileged) {
                // Written on the page's paper, not on a black field.
                let page = f.edit(
                    "text",
                    rect(22, 63, 257, 243),
                    &self.text,
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
                    &self.text,
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
                                    if b.viewer_is_psr {
                                        format!("- {} <{}>", p.author_name, p.author_account)
                                    } else if p.author_name.is_empty() {
                                        String::new()
                                    } else {
                                        format!("- {}", p.author_name)
                                    }
                                })
                                .unwrap_or("(blank)".into());
                            format!("Page {}\t{author}", i + 1)
                        })
                        .collect(),
                    selected: self.page,
                },
                true,
            )
            .list_skin = Some(crate::panels::ListSkin::BOOK);
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let Some(server) = c.game.open_book() else {
            return if matches!(e,ControlEvent::Activate(ref id) if id=="close") {
                let mut out = vec![];
                if let Some(old) = self.last.clone() {
                    let old = self.effective(old);
                    self.save(&old, c.classic.book_edit_privileged, &mut out);
                }
                out.push(PanelAction::Game(UiRequest::UnregisterBookRange));
                out.push(PanelAction::Close);
                out
            } else {
                vec![]
            };
        };
        let b = self.effective(server.clone());
        match e {
            ControlEvent::Tick => {
                let mut out = vec![];
                if self.opening != Some((b.book_id, b.opening)) {
                    let same = self.opening.is_some_and(|(id, _)| id == b.book_id);
                    if let Some(old) = self.last.clone() {
                        let old = self.effective(old);
                        self.save(&old, c.classic.book_edit_privileged, &mut out);
                    }
                    self.last = Some(server.clone());
                    self.opening = Some((b.book_id, b.opening));
                    self.page = if same {
                        self.page.min(b.pages.len().saturating_sub(1))
                    } else {
                        0
                    };
                    self.pages = Some(b.pages.clone());
                    self.data_edge = b.page_data_applied;
                    self.add_edge = b.add_page_responses;
                    self.load(&b, &mut out);
                } else {
                    if self.data_edge != server.page_data_applied {
                        self.data_edge = server.page_data_applied;
                        if self.pending {
                            if let Some(p) =
                                server.pages.get(self.page).filter(|p| p.text.is_some())
                            {
                                if let Some(slot) =
                                    self.pages.as_mut().and_then(|p| p.get_mut(self.page))
                                {
                                    *slot = p.clone();
                                }
                                self.text = p.text.clone().unwrap();
                                self.pending = false;
                                self.dirty = false;
                            }
                        }
                    }
                    if self.add_edge != server.add_page_responses {
                        self.add_edge = server.add_page_responses;
                        if let Some(a) = &server.add_page {
                            if !a.success || a.page as usize != self.page {
                                if a.success {
                                    self.page = a.page as usize;
                                }
                                out.push(PanelAction::Game(UiRequest::BookData {
                                    book: b.book_id,
                                }));
                            } else {
                                let p = dereth_client_contract::view::BookPageView {
                                    author_id: a.author_id,
                                    author_name: a.author_name.clone(),
                                    text: Some(String::new()),
                                    ..Default::default()
                                };
                                let pages = self.pages.get_or_insert_with(|| b.pages.clone());
                                pages.insert(self.page.min(pages.len()), p);
                                self.text.clear();
                                self.pending = false;
                                self.dirty = false;
                            }
                        }
                    }
                }
                out
            }
            ControlEvent::Edit { id, text }
                if id == "text"
                    && !self.pending
                    && editable(&b, self.page, c.classic.book_edit_privileged) =>
            {
                // A page holds what fits on it: typing that would run past its foot is refused.
                if !fits_on_page(&text) {
                    return vec![];
                }
                self.text = text;
                self.dirty = true;
                vec![]
            }
            ControlEvent::Select { id, index } if id == "page" => {
                self.turn(&b, index.min(b.pages.len()), c.classic.book_edit_privileged)
            }
            ControlEvent::Value { id, value } if id == "page" => {
                self.turn(&b, value.max(0) as usize, c.classic.book_edit_privileged)
            }
            ControlEvent::Activate(id) if id == "previous" => self.turn(
                &b,
                self.page.saturating_sub(1),
                c.classic.book_edit_privileged,
            ),
            ControlEvent::Activate(id) if id == "next" => {
                self.turn(&b, self.page + 1, c.classic.book_edit_privileged)
            }
            ControlEvent::Activate(id) if id == "close" => {
                let mut out = vec![];
                self.save(&b, c.classic.book_edit_privileged, &mut out);
                out.push(PanelAction::Game(UiRequest::UnregisterBookRange));
                out.push(PanelAction::Close);
                out
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn blank_page_deletion_does_not_treat_tab_or_carriage_return_as_blank() {
        assert!(blank(" \n\0"));
        assert!(!blank("\t"));
        assert!(!blank("\r"));
    }
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
    fn other_authors_editability_uses_nonzero_ignore_author() {
        let mut b = BookView {
            player_id: ObjectId(7),
            ..Default::default()
        };
        b.pages.push(dereth_client_contract::view::BookPageView {
            author_id: ObjectId(9),
            ignore_author: -1,
            ..Default::default()
        });
        assert!(editable(&b, 0, false));
        b.pages[0].ignore_author = 0;
        assert!(!editable(&b, 0, false));
        b.viewer_is_psr = true; // Includes advocate status at the public seam.
        assert!(!editable(&b, 0, false));
        assert!(editable(&b, 0, true));
    }
}
