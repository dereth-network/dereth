//! The dialogs: a question answered yes or no, please wait, death's one answer, a message to
//! read, and a question that answers itself no when its time runs out; and the factory that keeps
//! them, one up at a time.
//!
//! A panel asks for a dialog with the number it will know it by ([`new_context`]), its kind and
//! its text; the [`Dialogs`] puts it up at once when none is up. Otherwise it waits in line
//! behind the one up, unless it is asked for first: then the one up is taken down and waits at the
//! head of the line, and the new one goes up. When the one up closes, the next in line goes up.
//! Every dialog asked for in the world (up or waiting) keeps mouse look off until it closes; a
//! dialog of a screen before the world leaves mouse look alone (there is none).
//!
//! A dialog's answer is a notice to every panel ([`notice`]): a question closed with yes or no,
//! death's dialog answered, a message read. The dialog then closes itself. A panel may close a
//! dialog it asked for by its number, up or waiting.
//!
//! A dialog is a child framework of the panel that asked (or a screen of its own when the asker
//! has none), its window the play layout's: `ConfirmationField` (its question in `QuestionText`,
//! `YesButton` and `NoButton`), `WaitField`, `ResurrectField` (`ResurrectButton`), `MessageField`
//! (`OkButton`), their text in `TextField`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};

use auberean_text::{StringInfo, Value, Variable};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::gameplay::{self, Notice};

/// The layout the dialogs' windows are in, by enumeration value (the play layout's).
pub const DIALOG_LAYOUT_ENUM: u32 = gameplay::PLAY_LAYOUT_ENUM;

/// The dialogs' kinds.
pub mod kind {
    /// A question answered yes or no.
    pub const CONFIRMATION: u32 = 0x4000_0001;
    /// Please wait: no answer.
    pub const WAIT: u32 = 0x4000_0002;
    /// Death's: one answer, to be brought back.
    pub const RESURRECT: u32 = 0x4000_0003;
    /// A message to read: one answer.
    pub const MESSAGE: u32 = 0x4000_0004;
    /// A question that answers itself no when its seconds run out, saying how many are left.
    pub const TIMED: u32 = 0x4000_0005;
}

/// The notices the dialogs send every panel: the client's own, numbered below `0x10000`, apart
/// from the scripts' natives.
pub mod notice {
    /// A question closed: the dialog's number, then 1 for yes or 0 for no.
    pub const CLOSE_CONFIRMATION_DIALOG: u32 = 0x436;
    /// Death's dialog answered: the dialog's number.
    pub const CLOSE_RESURRECT_DIALOG: u32 = 0x44F;
    /// A message read: the dialog's number.
    pub const CLOSE_MESSAGE_DIALOG: u32 = 0x450;
    /// A dialog asked for (0) or gone (1): mouse look waits on every dialog asked for.
    pub const UPDATE_CAN_MOUSE_LOOK: u32 = 0x455;
}

/// How long a timed question waits for its answer, in seconds.
pub const TIMED_DELAY: i64 = 15;

mod name {
    use auberean_ui::Token;
    pub const CONFIRMATION_FIELD: Token = Token::of("ConfirmationField");
    pub const WAIT_FIELD: Token = Token::of("WaitField");
    pub const RESURRECT_FIELD: Token = Token::of("ResurrectField");
    pub const MESSAGE_FIELD: Token = Token::of("MessageField");
    pub const QUESTION_TEXT: Token = Token::of("QuestionText");
    pub const TEXT_FIELD: Token = Token::of("TextField");
    pub const YES_BUTTON: Token = Token::of("YesButton");
    pub const NO_BUTTON: Token = Token::of("NoButton");
    pub const OK_BUTTON: Token = Token::of("OkButton");
    pub const RESURRECT_BUTTON: Token = Token::of("ResurrectButton");
    /// The variable of a timed question's text: the seconds left.
    pub const TIME: Token = Token::of("time");
}

/// The last number given to a dialog, for every factory: a dialog's number is never 0.
static CONTEXT: AtomicU32 = AtomicU32::new(0);

/// The number a new dialog is known by, the one after the last given.
#[must_use]
pub fn new_context() -> u32 {
    let n = CONTEXT.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    if n == 0 {
        CONTEXT.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    } else {
        n
    }
}

/// A question closed, as its notice says: the dialog's number and whether the answer was yes.
#[must_use]
pub fn confirmation_closed(n: &Notice) -> Option<(u32, bool)> {
    (n.native == notice::CLOSE_CONFIRMATION_DIALOG).then(|| {
        let w = |i: usize| n.words.get(i).copied().unwrap_or(0);
        (w(0), w(1) != 0)
    })
}

/// A dialog's notice to the panels.
fn said(native: u32, words: Vec<u32>) -> Notice {
    Notice {
        native,
        words,
        ..Notice::default()
    }
}

/// What a dialog is, with what its kind keeps.
#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Confirmation,
    Wait,
    Resurrect,
    Message,
    /// The string and its table (written with the seconds left), the last second seen and the
    /// second it answers no after.
    Timed {
        string: u32,
        table: u32,
        now: i64,
        end: i64,
    },
}

/// A dialog up: its framework, its window, its number.
#[derive(Debug)]
struct Dialog {
    frame: FrameId,
    root: Option<Handle>,
    context: u32,
    kind: Kind,
}

/// What a dialog's listener heard asks of the factory: an answer sent, the dialog closed.
struct Answer {
    notice: Notice,
    context: u32,
}

impl Dialog {
    /// A dialog's framework under its asker's (or a screen of its own) and its window, made from
    /// the play layout.
    fn frame(ui: &mut Ui, parent: Option<FrameId>, window: Token) -> (FrameId, Option<Handle>) {
        let frame = match parent.filter(|p| ui.frame_listener(*p).is_some()) {
            Some(p) => ui.new_child_frame(p),
            None => ui.new_main_frame(),
        };
        let root = ui.frame_create_root(frame, DIALOG_LAYOUT_ENUM, window);
        (frame, root)
    }

    /// Listen to buttons of the window by name.
    fn listen_to(ui: &mut Ui, frame: FrameId, buttons: &[Token]) {
        if let Some(l) = ui.frame_listener(frame) {
            for b in buttons {
                ui.listen_for_message(l, *b, msg::BUTTON_RELEASE_ON);
            }
        }
    }

    /// The text of one of the window's elements.
    fn text_of(ui: &mut Ui, root: Option<Handle>, element: Token, text: &str) {
        if let Some(t) = root.and_then(|r| ui.child_recursive(r, element)) {
            ui.set_text(t, text);
        }
    }

    /// A question: its window, its question, yes and no heard.
    // D2-03271
    fn confirmation(ui: &mut Ui, parent: Option<FrameId>, text: &str, context: u32) -> Self {
        let (frame, root) = Self::frame(ui, parent, name::CONFIRMATION_FIELD);
        Self::text_of(ui, root, name::QUESTION_TEXT, text);
        Self::listen_to(ui, frame, &[name::YES_BUTTON, name::NO_BUTTON]);
        Self {
            frame,
            root,
            context,
            kind: Kind::Confirmation,
        }
    }

    /// Please wait: its window and its text; nothing heard.
    // D2-03274
    fn wait(ui: &mut Ui, parent: Option<FrameId>, text: &str, context: u32) -> Self {
        let (frame, root) = Self::frame(ui, parent, name::WAIT_FIELD);
        Self::text_of(ui, root, name::TEXT_FIELD, text);
        Self {
            frame,
            root,
            context,
            kind: Kind::Wait,
        }
    }

    /// Death's: its window, its text, its one button heard.
    // D2-03278
    fn resurrect(ui: &mut Ui, parent: Option<FrameId>, text: &str, context: u32) -> Self {
        let (frame, root) = Self::frame(ui, parent, name::RESURRECT_FIELD);
        Self::text_of(ui, root, name::TEXT_FIELD, text);
        Self::listen_to(ui, frame, &[name::RESURRECT_BUTTON]);
        Self {
            frame,
            root,
            context,
            kind: Kind::Resurrect,
        }
    }

    /// A message: its window, its text, its one button heard.
    // D2-03282
    fn message(ui: &mut Ui, parent: Option<FrameId>, text: &str, context: u32) -> Self {
        let (frame, root) = Self::frame(ui, parent, name::MESSAGE_FIELD);
        Self::text_of(ui, root, name::TEXT_FIELD, text);
        Self::listen_to(ui, frame, &[name::OK_BUTTON]);
        Self {
            frame,
            root,
            context,
            kind: Kind::Message,
        }
    }

    /// A timed question: a question's window, yes and no and the clock heard, its string and
    /// table kept, and its seconds started.
    // D2-03287
    fn timed(ui: &mut Ui, parent: Option<FrameId>, info: &StringInfo, context: u32) -> Self {
        let (frame, root) = Self::frame(ui, parent, name::CONFIRMATION_FIELD);
        Self::listen_to(ui, frame, &[name::YES_BUTTON, name::NO_BUTTON]);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_global_message(l, global::TIME);
        }
        let mut me = Self {
            frame,
            root,
            context,
            kind: Kind::Timed {
                string: info.string_id,
                table: info.table,
                now: 0,
                end: 0,
            },
        };
        me.set_delay(ui, TIMED_DELAY);
        me
    }

    /// The clock's whole second.
    #[allow(clippy::cast_possible_truncation)]
    fn second(ui: &Ui) -> i64 {
        ui.now().floor() as i64
    }

    /// A timed question's text: its string with the seconds left.
    fn timed_text(ui: &Ui, string: u32, table: u32, left: i64) -> String {
        let mut info = StringInfo::entry(table, string);
        info.variables.push(Variable {
            id: name::TIME.raw(),
            value: Value::Int(i32::try_from(left).unwrap_or(i32::MAX)),
        });
        ui.assets().write(&info).unwrap_or_default()
    }

    /// A timed question's seconds started: it answers no `seconds` from now, and says so.
    // D2-03286
    fn set_delay(&mut self, ui: &mut Ui, seconds: i64) {
        let t = Self::second(ui);
        let Kind::Timed {
            string,
            table,
            now,
            end,
        } = &mut self.kind
        else {
            return;
        };
        *now = t;
        *end = t + seconds;
        let text = Self::timed_text(ui, *string, *table, *end - *now);
        self.set_text(ui, &text);
    }

    /// Its text, in the element its kind writes in.
    // D2-03270, D2-03273, D2-03277, D2-03281, D2-03289
    fn set_text(&self, ui: &mut Ui, text: &str) {
        let element = match self.kind {
            Kind::Confirmation | Kind::Timed { .. } => name::QUESTION_TEXT,
            Kind::Wait | Kind::Resurrect | Kind::Message => name::TEXT_FIELD,
        };
        Self::text_of(ui, self.root, element, text);
    }

    /// What its listener heard: a question's buttons answer it (yes for its yes button, no for
    /// any other), a message's and death's their one button; a timed question counts its seconds
    /// down and answers no when they have run out.
    // D2-03269, D2-03276, D2-03280, D2-03284, D2-03285
    fn listen(&mut self, ui: &mut Ui, mail: &Mail) -> Option<Answer> {
        let c = self.context;
        let answer = |native, words| {
            Some(Answer {
                notice: said(native, words),
                context: c,
            })
        };
        match (*mail, &mut self.kind) {
            (
                Mail::Element {
                    element,
                    what: msg::BUTTON_RELEASE_ON,
                    ..
                },
                Kind::Confirmation | Kind::Timed { .. },
            ) => answer(
                notice::CLOSE_CONFIRMATION_DIALOG,
                vec![c, u32::from(element == name::YES_BUTTON)],
            ),
            (
                Mail::Element {
                    element: name::OK_BUTTON,
                    what: msg::BUTTON_RELEASE_ON,
                    ..
                },
                Kind::Message,
            ) => answer(notice::CLOSE_MESSAGE_DIALOG, vec![c]),
            (
                Mail::Element {
                    element: name::RESURRECT_BUTTON,
                    what: msg::BUTTON_RELEASE_ON,
                    ..
                },
                Kind::Resurrect,
            ) => answer(notice::CLOSE_RESURRECT_DIALOG, vec![c]),
            (
                Mail::Global {
                    what: global::TIME, ..
                },
                Kind::Timed {
                    string,
                    table,
                    now,
                    end,
                },
            ) => {
                let t = Self::second(ui);
                if *now >= t {
                    return None;
                }
                *now = t;
                if *end < t {
                    return answer(notice::CLOSE_CONFIRMATION_DIALOG, vec![c, 0]);
                }
                let text = Self::timed_text(ui, *string, *table, *end - *now);
                self.set_text(ui, &text);
                None
            }
            _ => None,
        }
    }

    /// It goes: its window off the screen and gone, its framework and listening gone.
    // D2-03272, D2-03275, D2-03279, D2-03283, D2-03288
    fn close(self, ui: &mut Ui) {
        if let Some(r) = self.root {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
        }
        ui.drop_frame(self.frame);
    }
}

/// A dialog asked for: its text (and the string it was written from, for a timed question), its
/// kind, the dialog once made, the asker's framework, its number.
#[derive(Debug)]
struct Asked {
    text: String,
    info: StringInfo,
    kind: u32,
    dialog: Option<Dialog>,
    parent: Option<FrameId>,
    context: u32,
}

/// What a panel asks for: a dialog's number, kind and text (or the string it is written from),
/// and whether it goes up first.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub context: u32,
    pub kind: u32,
    pub text: String,
    pub info: Option<StringInfo>,
    pub first: bool,
}

impl From<&gameplay::Dialog> for Request {
    fn from(d: &gameplay::Dialog) -> Self {
        Self {
            context: d.id,
            kind: d.kind,
            text: d.text.clone(),
            info: None,
            first: d.modal,
        }
    }
}

/// The dialogs asked for: the one up, and those waiting in line.
#[derive(Debug, Default)]
pub struct Dialogs {
    cur: Option<Asked>,
    queue: VecDeque<Asked>,
}

impl Dialogs {
    /// A dialog asked for made by its kind (a kind not known makes none).
    // D2-03259
    fn create_dialog_internal(ui: &mut Ui, info: &mut Asked) {
        let (p, c) = (info.parent, info.context);
        info.dialog = match info.kind {
            kind::CONFIRMATION => Some(Dialog::confirmation(ui, p, &info.text, c)),
            kind::WAIT => Some(Dialog::wait(ui, p, &info.text, c)),
            kind::RESURRECT => Some(Dialog::resurrect(ui, p, &info.text, c)),
            kind::MESSAGE => Some(Dialog::message(ui, p, &info.text, c)),
            kind::TIMED => Some(Dialog::timed(ui, p, &info.info, c)),
            _ => None,
        };
    }

    /// A dialog asked for under the asker's framework: up at once when none is up; else in line,
    /// or (asked for first) up in place of the one up, which waits at the head of the line. Mouse
    /// look waits on it. Returns its number.
    // D2-03265
    pub fn make_dialog(
        &mut self,
        ui: &mut Ui,
        parent: Option<FrameId>,
        r: &Request,
        out: &mut Vec<Notice>,
    ) -> u32 {
        let text = match &r.info {
            Some(i) if r.text.is_empty() => ui.assets().write(i).unwrap_or_default(),
            _ => r.text.clone(),
        };
        let mut info = Asked {
            text,
            info: r.info.clone().unwrap_or_default(),
            kind: r.kind,
            dialog: None,
            parent,
            context: r.context,
        };
        let up = self.cur.as_ref().is_some_and(|c| c.dialog.is_some());
        if !up {
            Self::create_dialog_internal(ui, &mut info);
            self.cur = Some(info);
        } else if !r.first {
            self.queue.push_back(info);
        } else if let Some(mut old) = self.cur.take() {
            if let Some(d) = old.dialog.take() {
                d.close(ui);
            }
            self.queue.push_front(old);
            Self::create_dialog_internal(ui, &mut info);
            self.cur = Some(info);
        }
        if parent.is_some() {
            out.push(said(notice::UPDATE_CAN_MOUSE_LOOK, vec![0]));
        }
        r.context
    }

    /// The dialog up is done: it goes, mouse look no longer waits on it, and the next in line
    /// goes up.
    // D2-03266
    fn dialog_done(&mut self, ui: &mut Ui, out: &mut Vec<Notice>) {
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let Some(d) = cur.dialog.take() else {
            return;
        };
        d.close(ui);
        if cur.parent.is_some() {
            out.push(said(notice::UPDATE_CAN_MOUSE_LOOK, vec![1]));
        }
        if let Some(mut next) = self.queue.pop_front() {
            if next.dialog.is_none() {
                Self::create_dialog_internal(ui, &mut next);
                self.cur = Some(next);
            }
        }
    }

    /// Close a dialog by its number (0: none), while one is up: the one up (the next in line goes
    /// up), or one waiting (it leaves the line; mouse look no longer waits on it).
    // D2-03268
    pub fn close_dialog(&mut self, ui: &mut Ui, context: u32, out: &mut Vec<Notice>) {
        let Some(up) = self.cur.as_ref().and_then(|c| c.dialog.as_ref()) else {
            return;
        };
        if context == 0 {
            return;
        }
        if context == up.context {
            self.dialog_done(ui, out);
            return;
        }
        if let Some(i) = self.queue.iter().position(|d| d.context == context) {
            if self.queue.remove(i).is_some_and(|d| d.parent.is_some()) {
                out.push(said(notice::UPDATE_CAN_MOUSE_LOOK, vec![1]));
            }
        }
    }

    /// What the dialog up heard, acted on: an answer goes to every panel and the dialog closes.
    /// A dialog whose asker has gone goes with it, as the asker would have closed it.
    pub fn listen(&mut self, ui: &mut Ui, out: &mut Vec<Notice>) {
        self.drop_orphans(ui, out);
        let Some(d) = self.cur.as_mut().and_then(|c| c.dialog.as_mut()) else {
            return;
        };
        let mail = ui
            .frame_listener(d.frame)
            .map(|l| ui.take_mail(l))
            .unwrap_or_default();
        for m in mail {
            let Some(d) = self.cur.as_mut().and_then(|c| c.dialog.as_mut()) else {
                return;
            };
            if let Some(a) = d.listen(ui, &m) {
                out.push(a.notice);
                self.close_dialog(ui, a.context, out);
                return;
            }
        }
    }

    /// Dialogs whose asker's framework has gone, closed.
    fn drop_orphans(&mut self, ui: &mut Ui, out: &mut Vec<Notice>) {
        let gone = |ui: &Ui, d: &Asked| d.parent.is_some_and(|p| ui.frame_listener(p).is_none());
        let orphans: Vec<u32> = self
            .cur
            .iter()
            .filter(|c| c.dialog.is_some())
            .chain(self.queue.iter())
            .filter(|d| gone(ui, d))
            .map(|d| d.context)
            .collect();
        for c in orphans {
            self.close_dialog(ui, c, out);
        }
    }

    /// The number of the dialog up, if one is.
    #[must_use]
    pub fn up(&self) -> Option<u32> {
        self.cur
            .as_ref()
            .and_then(|c| c.dialog.as_ref())
            .map(|d| d.context)
    }

    /// The numbers of the dialogs waiting in line, first first.
    #[must_use]
    pub fn waiting(&self) -> Vec<u32> {
        self.queue.iter().map(|d| d.context).collect()
    }

    /// The window of the dialog up.
    #[must_use]
    pub fn window(&self) -> Option<Handle> {
        self.cur.as_ref()?.dialog.as_ref()?.root
    }

    /// The line emptied: the dialogs waiting forgotten (the one up stays).
    // D2-03267
    pub fn flush(&mut self) {
        self.queue.clear();
    }
}
