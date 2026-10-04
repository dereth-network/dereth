//! The classic in-game Help viewer, over a Help book the player supplies
//! (`DERETH_CLASSIC_HELP_BOOK`); none ships with this client.
use crate::{
    panels::{
        self, Context, ControlEvent, ControlKind, HostAction, Panel, PanelAction, PanelFrame,
    },
    widgets::{Input, Key, Rect},
    Command, Screen,
};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, Deserialize)]
pub struct Link {
    pub rect: [i32; 4],
    #[serde(default)]
    pub target: Option<u32>,
    #[serde(default)]
    pub action: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Page {
    pub title: String,
    pub screen: Screen,
    /// Height of the independently laid out non-scrolling topic section.
    #[serde(default)]
    pub fixed_height: i32,
    pub links: Vec<Link>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Book {
    pub schema: u32,
    pub contexts: BTreeMap<u32, BTreeMap<u32, Page>>,
}
/// Decode and validate the supplied help pages without opening any host resource.
pub fn decode(data: &[u8]) -> Result<Book, String> {
    let book: Book = serde_json::from_slice(data).map_err(|e| e.to_string())?;
    validate(&book)?;
    Ok(book)
}
fn validate(book: &Book) -> Result<(), String> {
    if book.schema != 1 {
        return Err("unsupported help schema".into());
    }
    for (context, initial, width) in [(50, 0xCCCB685E, 273), (51, 0xA5D680F2, 290)] {
        let pages = book.contexts.get(&context).ok_or("missing help context")?;
        if !pages.contains_key(&initial) {
            return Err("missing initial help topic".into());
        }
        for page in pages.values() {
            if page.screen.width != width
                || page.fixed_height < 0
                || page.fixed_height as u32 > page.screen.height
            {
                return Err("invalid help page geometry".into());
            }
            for link in &page.links {
                if link.rect[2] < 0
                    || link.rect[3] < 0
                    || link.target.is_some_and(|id| !pages.contains_key(&id))
                {
                    return Err("invalid help hyperlink".into());
                }
            }
        }
    }
    Ok(())
}
pub fn make(id: &str, book: Option<&Arc<Book>>) -> Option<Box<dyn Panel>> {
    let context = match id {
        "help-chargen" => 50,
        "help-game" => 51,
        _ => return None,
    };
    Some(Box::new(Help::new(book?.clone(), context)))
}
#[derive(Debug)]
struct Help {
    book: Arc<Book>,
    context: u32,
    topic: u32,
    scroll: i32,
    selected_link: Option<usize>,
    back: Vec<(u32, i32)>,
    forward: Vec<(u32, i32)>,
}
impl Help {
    fn new(book: Arc<Book>, context: u32) -> Self {
        Self {
            book,
            context,
            topic: if context == 50 {
                0xCCCB685E
            } else {
                0xA5D680F2
            },
            scroll: 0,
            selected_link: None,
            back: vec![],
            forward: vec![],
        }
    }
    fn page(&self) -> &Page {
        &self.book.contexts[&self.context][&self.topic]
    }
    fn viewport(&self) -> Rect {
        if self.context == 50 {
            panels::rect(43, 41, 273, 400)
        } else {
            panels::rect(5, 5, 290, 352)
        }
    }
    fn navigate(&mut self, topic: u32) {
        if self.book.contexts[&self.context].contains_key(&topic) {
            self.back.push((self.topic, self.scroll));
            self.forward.clear();
            self.topic = topic;
            self.scroll = 0;
            self.selected_link = None;
        }
    }
    fn action(&mut self, action: &str) -> Vec<PanelAction> {
        match action.to_ascii_lowercase().trim_end_matches("()") {
            "back" => {
                if let Some((topic, scroll)) = self.back.pop() {
                    self.forward.push((self.topic, self.scroll));
                    self.topic = topic;
                    self.scroll = scroll;
                    self.selected_link = None;
                }
            }
            "forward" => {
                if let Some((topic, scroll)) = self.forward.pop() {
                    self.back.push((self.topic, self.scroll));
                    self.topic = topic;
                    self.scroll = scroll;
                    self.selected_link = None;
                }
            }
            "exit" | "close" => return vec![PanelAction::Close],
            "printtopic" => {
                return vec![PanelAction::Host(HostAction::PrintLegacyHelp {
                    context: self.context,
                    topic: self.topic,
                })]
            }
            _ => {}
        }
        vec![]
    }
    fn follow_link(&mut self, index: usize) -> Vec<PanelAction> {
        if let Some(link) = self.page().links.get(index).cloned() {
            if let Some(topic) = link.target {
                self.navigate(topic);
            } else if let Some(action) = link.action {
                return self.action(&action);
            }
        }
        vec![]
    }
    fn cycle_link(&mut self) -> Vec<PanelAction> {
        let page = self.page();
        let viewport = self.viewport();
        let visible: Vec<usize> = page
            .links
            .iter()
            .enumerate()
            .filter_map(|(i, link)| {
                let [_, y, _, h] = link.rect;
                let fixed = y < page.fixed_height;
                let shifted = y - if fixed { 0 } else { self.scroll };
                let minimum = if fixed { 0 } else { page.fixed_height };
                let maximum = if fixed { page.fixed_height } else { viewport.h };
                (shifted + h > minimum && shifted < maximum).then_some(i)
            })
            .collect();
        if visible.is_empty() {
            return vec![PanelAction::Close];
        }
        let current = self
            .selected_link
            .and_then(|i| visible.iter().position(|j| *j == i));
        self.selected_link = Some(
            visible[match current {
                Some(i) if i > 0 => i - 1,
                _ => visible.len() - 1,
            }],
        );
        vec![]
    }
    fn compose(&self) -> PanelFrame {
        let (w, h, bg) = if self.context == 50 {
            (322, 574, 114)
        } else {
            (300, 362, 123)
        };
        let mut out = PanelFrame::new(w, h);
        out.image(
            &format!("help:2004-04-08:resource:{bg}"),
            panels::rect(0, 0, w as i32, h as i32),
            false,
            false,
        );
        let viewport = self.viewport();
        let page = self.page();
        // Each text/image command belongs wholly to one section; the exporter splits
        // paragraphs at the topic section boundary before laying out either pane.
        for original in &page.screen.commands {
            let y = command_y(original);
            let fixed = y < page.fixed_height;
            let shift = if fixed { 0 } else { self.scroll };
            let clip = if fixed {
                [
                    viewport.x,
                    viewport.y,
                    viewport.x + viewport.w,
                    viewport.y + page.fixed_height,
                ]
            } else {
                [
                    viewport.x,
                    viewport.y + page.fixed_height,
                    viewport.x + viewport.w,
                    viewport.y + viewport.h,
                ]
            };
            let mut command =
                crate::desktop::translate_command(original.clone(), viewport.x, viewport.y - shift);
            clip_command(&mut command, clip);
            out.screen.commands.push(command);
        }
        let c = out.control(
            "body",
            viewport,
            ControlKind::HitList {
                row_count: page.screen.height as usize,
                row_height: 1,
                selected: None,
                offset: self.scroll,
            },
            true,
        );
        c.paint = false;
        c.silent = true;
        for (index, link) in page.links.iter().enumerate() {
            let [x, y, w, h] = link.rect;
            let fixed = y < page.fixed_height;
            let min_y = viewport.y + if fixed { 0 } else { page.fixed_height };
            let max_y = viewport.y + if fixed { page.fixed_height } else { viewport.h };
            let y = viewport.y + y - if fixed { 0 } else { self.scroll };
            let top = y.max(min_y);
            let bottom = (y + h).min(max_y);
            if bottom <= top {
                continue;
            }
            let c = out.button(
                format!("link:{index}"),
                panels::rect(viewport.x + x, top, w, bottom - top),
                "",
                true,
            );
            c.paint = false;
            c.silent = true;
        }
        if self.context == 50 {
            for (id, x, y, art, enabled) in [
                ("exit", 26, 480, [115, 116, 115], true),
                ("back", 53, 521, [121, 112, 168], !self.back.is_empty()),
                (
                    "forward",
                    221,
                    521,
                    [117, 118, 170],
                    !self.forward.is_empty(),
                ),
                ("printtopic", 248, 480, [120, 111, 171], true),
            ] {
                let c = out.button(id, panels::rect(x, y, 70, 38), "", enabled);
                c.silent = true;
                c.images = Some(art.map(|n| format!("help:2004-04-08:resource:{n}")));
            }
        }
        out
    }
}
fn command_y(command: &Command) -> i32 {
    match command {
        Command::Text { y, .. } | Command::Image { y, .. } | Command::Fill { y, .. } => *y,
        Command::TextBox { rect, .. } | Command::RichTextBox { rect, .. } => rect[1],
        _ => 0,
    }
}
fn clip_command(command: &mut Command, bounds: [i32; 4]) {
    match command {
        Command::Text { clip, .. }
        | Command::Image { clip, .. }
        | Command::TextBox { clip, .. }
        | Command::RichTextBox { clip, .. } => {
            *clip = Some(match *clip {
                Some(old) => [
                    old[0].max(bounds[0]),
                    old[1].max(bounds[1]),
                    old[2].min(bounds[2]),
                    old[3].min(bounds[3]),
                ],
                None => bounds,
            });
        }
        Command::Fill {
            x,
            y,
            width,
            height,
            ..
        } => {
            let right = (*x + *width as i32).min(bounds[2]);
            let bottom = (*y + *height as i32).min(bounds[3]);
            *x = (*x).max(bounds[0]);
            *y = (*y).max(bounds[1]);
            *width = (right - *x).max(0) as u32;
            *height = (bottom - *y).max(0) as u32;
        }
        _ => {}
    }
}
impl Panel for Help {
    fn pointer_art(&self, x: i32, y: i32) -> Option<(String, u32, u32)> {
        let viewport = self.viewport();
        let page = self.page();
        let link = viewport.contains(x, y)
            && page.links.iter().any(|link| {
                let [lx, ly, w, h] = link.rect;
                let fixed = ly < page.fixed_height;
                let minimum = viewport.y + if fixed { 0 } else { page.fixed_height };
                let maximum = viewport.y + if fixed { page.fixed_height } else { viewport.h };
                let top = viewport.y + ly - if fixed { 0 } else { self.scroll };
                y >= minimum
                    && y < maximum
                    && panels::rect(viewport.x + lx, top, w, h).contains(x, y)
            });
        Some((
            format!("help:2004-04-08:cursor:{}", if link { 159 } else { 156 }),
            32,
            32,
        ))
    }
    fn input(&mut self, input: &Input, _: &Context<'_>) -> Option<Vec<PanelAction>> {
        match input {
            Input::RightClick { .. } => Some(vec![PanelAction::Close]),
            Input::Text(text) if text.eq_ignore_ascii_case("p") => Some(self.action("PrintTopic")),
            Input::Text(text) if !text.is_empty() => Some(vec![PanelAction::Close]),
            Input::Key {
                key: Key::Tab | Key::Up | Key::Down,
                ..
            } => Some(self.cycle_link()),
            Input::Key {
                key: Key::Enter, ..
            } => Some(match self.selected_link {
                Some(index) => self.follow_link(index),
                None => vec![PanelAction::Close],
            }),
            Input::Key { .. } => Some(vec![PanelAction::Close]),
            _ => None,
        }
    }
    fn id(&self) -> &'static str {
        if self.context == 50 {
            "help-chargen"
        } else {
            "help-game"
        }
    }
    fn frame(&self, _: &Context<'_>) -> PanelFrame {
        self.compose()
    }
    fn event(&mut self, event: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
        match event {
            ControlEvent::Scroll { id, value } if id == "body" => {
                self.scroll = value.clamp(
                    0,
                    (self.page().screen.height as i32 - self.viewport().h).max(0),
                )
            }
            ControlEvent::Activate(id) => {
                if let Some(index) = id
                    .strip_prefix("link:")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    return self.follow_link(index);
                } else {
                    return self.action(&id);
                }
            }
            _ => {}
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn book() -> Arc<Book> {
        let mut contexts = BTreeMap::new();
        for (context, initial, width) in [(50, 0xCCCB685E, 273), (51, 0xA5D680F2, 290)] {
            let page = Page {
                title: "Synthetic".into(),
                screen: Screen {
                    width,
                    height: 800,
                    commands: vec![
                        Command::Text {
                            text: "Heading".into(),
                            x: 2,
                            y: 0,
                            font: "synthetic".into(),
                            color: 0xffffffff,
                            clip: None,
                        },
                        Command::Text {
                            text: "Body".into(),
                            x: 2,
                            y: 60,
                            font: "synthetic".into(),
                            color: 0xffffffff,
                            clip: None,
                        },
                    ],
                },
                fixed_height: 44,
                links: vec![Link {
                    rect: [0, 60, 40, 10],
                    target: Some(1),
                    action: None,
                }],
            };
            contexts.insert(
                context,
                BTreeMap::from([(initial, page.clone()), (1, page)]),
            );
        }
        Arc::new(Book {
            schema: 1,
            contexts,
        })
    }
    #[test]
    fn help_navigation_restores_scroll_and_new_links_discard_forward_history() {
        let mut h = Help::new(book(), 51);
        h.scroll = 123;
        h.navigate(1);
        assert_eq!(h.scroll, 0);
        h.scroll = 44;
        h.action("Back()");
        assert_eq!((h.topic, h.scroll), (0xA5D680F2, 123));
        h.action("Forward()");
        assert_eq!((h.topic, h.scroll), (1, 44));
        h.action("Back()");
        h.navigate(1);
        assert!(h.forward.is_empty());
        let history = h.back.clone();
        h.navigate(999);
        assert_eq!(h.back, history);
    }
    #[test]
    fn fixed_header_stays_put_while_body_and_link_clip_to_the_scroll_pane() {
        let mut h = Help::new(book(), 51);
        h.scroll = 30;
        let f = h.compose();
        assert!(matches!(
            &f.screen.commands[1],
            Command::Text {
                y: 5,
                clip: Some([5, 5, 295, 49]),
                ..
            }
        ));
        assert!(matches!(
            &f.screen.commands[2],
            Command::Text {
                y: 35,
                clip: Some([5, 49, 295, 357]),
                ..
            }
        ));
        assert!(!f.controls.iter().any(|c| c.id == "link:0"));
        assert!(f.controls.iter().all(|c| c.silent));
    }
    #[test]
    fn book_rejects_dangling_links_and_wrong_context_widths() {
        let valid = book();
        assert!(validate(&valid).is_ok());
        let mut broken = (*valid).clone();
        broken
            .contexts
            .get_mut(&51)
            .unwrap()
            .get_mut(&1)
            .unwrap()
            .links[0]
            .target = Some(999);
        assert!(validate(&broken).is_err());
        let mut broken = (*valid).clone();
        broken
            .contexts
            .get_mut(&51)
            .unwrap()
            .get_mut(&1)
            .unwrap()
            .screen
            .width = 273;
        assert!(validate(&broken).is_err());
    }
    #[test]
    fn help_cycles_visible_links_backwards_and_uses_the_link_cursor() {
        let mut data = (*book()).clone();
        data.contexts
            .get_mut(&51)
            .unwrap()
            .get_mut(&0xA5D680F2)
            .unwrap()
            .links
            .push(Link {
                rect: [0, 100, 40, 10],
                target: Some(1),
                action: None,
            });
        let mut h = Help::new(Arc::new(data), 51);
        assert!(h.cycle_link().is_empty());
        assert_eq!(h.selected_link, Some(1));
        h.cycle_link();
        assert_eq!(h.selected_link, Some(0));
        assert!(h.pointer_art(6, 65).unwrap().0.ends_with(":159"));
        assert!(h.pointer_art(6, 90).unwrap().0.ends_with(":156"));
        h.scroll = 200;
        assert_eq!(h.cycle_link(), vec![PanelAction::Close]);
        assert!(h.pointer_art(6, 65).unwrap().0.ends_with(":156"));
    }
    #[test]
    fn supplied_help_bytes_are_decoded_and_validated_without_a_path() {
        let page = |width| serde_json::json!({"title":"Start", "screen":{"width":width,"height":100,"commands":[]},"links":[]});
        let bytes = serde_json::to_vec(&serde_json::json!({"schema":1,"contexts":{
            "50":{(0xCCCB685Eu32.to_string()):page(273)},
            "51":{(0xA5D680F2u32.to_string()):page(290)}
        }}))
        .unwrap();
        let book = decode(&bytes).unwrap();
        assert_eq!(book.contexts[&51][&0xA5D680F2].screen.width, 290);
        assert!(decode(b"not json").is_err());
        assert_eq!(
            decode(br#"{"schema":0,"contexts":{}}"#).unwrap_err(),
            "unsupported help schema"
        );
    }
}
