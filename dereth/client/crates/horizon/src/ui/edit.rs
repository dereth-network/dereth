//! One line of text being edited: the caret and the selection, shared by the windows' text boxes
//! and the chat line.
//!
//! The arrows move the caret (Shift extends the selection, Control moves by words), Home and End go
//! to the ends, a press places the caret and a drag selects, a double-click selects a word. Typing
//! replaces the selection; Backspace and Delete remove it or the character beside the caret.
//! Control with A selects everything, with C copies the selection, with X cuts it and with V
//! pastes; Control-Insert, Shift-Delete and Shift-Insert do the same as C, X and V.
//!
//! A box of several lines (an inscription, a book's page, the notebook) edits the same way, with
//! Enter starting a new line, Up and Down moving between the rows as they are drawn, and Home and
//! End going to the ends of the caret's row.

use crate::draw::Rect;
use crate::ui::input::{vk, InputFrame};
use crate::ui::paint::{Painter, TextStyle};

/// The letter keys the editing shortcuts use, and Insert.
mod key {
    pub const A: usize = 0x41;
    pub const C: usize = 0x43;
    pub const V: usize = 0x56;
    pub const X: usize = 0x58;
    pub const INSERT: usize = 0x2D;
}

/// The selection, drawn over the text.
pub const SELECTION: u32 = 0x66C8_B27A;

/// The editing state of the one line that has the keyboard. Positions count characters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditState {
    /// The line the state belongs to; another line taking the keyboard starts afresh.
    owner: Option<u64>,
    /// The caret, and the other end of the selection (the same when nothing is selected).
    pub caret: usize,
    pub anchor: usize,
    /// The first character shown, when the line is wider than its box.
    first: usize,
    /// The text as this state last left it: text changed from outside puts the caret at its end.
    seen: String,
    /// A press in the line is being dragged.
    dragging: bool,
}

impl EditState {
    /// The selection as a range of characters, empty when nothing is selected.
    #[must_use]
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    /// No line has the keyboard.
    pub fn release(&mut self) {
        self.owner = None;
        self.dragging = false;
    }

    fn select_all(&mut self, len: usize) {
        self.anchor = 0;
        self.caret = len;
    }
}

/// A key for a line, from where its box is and the number its owner knows it by: it stays the
/// same while the box keeps the keyboard.
#[must_use]
pub fn line_key(r: Rect, id: u32) -> u64 {
    (u64::from(r.x.to_bits()) << 32) | u64::from(r.y.to_bits() ^ r.w.to_bits() ^ id)
}

/// Take the keyboard for line `key` holding `text`: a line new to the keyboard, or whose text
/// was changed from outside, has its caret at the end.
pub fn begin(input: &mut InputFrame, key: u64, text: &str) {
    let e = &mut input.edit;
    let len = text.chars().count();
    if e.owner != Some(key) || e.seen != text {
        e.owner = Some(key);
        e.caret = len;
        e.anchor = len;
        e.dragging = false;
        e.first = 0;
        e.seen = text.to_owned();
    }
    e.caret = e.caret.min(len);
    e.anchor = e.anchor.min(len);
}

/// This frame's typing and editing keys applied to `text`, kept to `max` characters. The keys
/// it does not answer (Enter, Escape, Tab, Up and Down) are left for the line's owner. `true`
/// when the text changed.
pub fn keys(input: &mut InputFrame, text: &mut String, max: usize) -> bool {
    keys_in(input, text, max, false)
}

fn keys_in(input: &mut InputFrame, text: &mut String, max: usize, lines: bool) -> bool {
    let before = text.clone();
    let mut chars: Vec<char> = text.chars().collect();
    let typed: String = input.chars.drain(..).collect();
    if !typed.is_empty() {
        insert(&mut input.edit, &mut chars, &typed, max, lines);
    }
    let (shift, ctrl) = (input.shift, input.ctrl);
    let pending = std::mem::take(&mut input.keys);
    for k in pending {
        let e = &mut input.edit;
        let sel = e.selection();
        let taken = match k {
            vk::LEFT | vk::RIGHT | vk::HOME | vk::END => {
                let len = chars.len();
                let to = match k {
                    vk::HOME => 0,
                    vk::END => len,
                    vk::LEFT if !shift && !sel.is_empty() => sel.start,
                    vk::RIGHT if !shift && !sel.is_empty() => sel.end,
                    vk::LEFT if ctrl => word_left(&chars, e.caret),
                    vk::RIGHT if ctrl => word_right(&chars, e.caret),
                    vk::LEFT => e.caret.saturating_sub(1),
                    _ => (e.caret + 1).min(len),
                };
                e.caret = to;
                if !shift {
                    e.anchor = to;
                }
                true
            }
            vk::DELETE if shift && !sel.is_empty() => {
                input.copied = Some(chars[sel.clone()].iter().collect());
                remove(e, &mut chars, sel);
                true
            }
            vk::BACK | vk::DELETE => {
                let range = if !sel.is_empty() {
                    sel
                } else if k == vk::BACK {
                    let from = if ctrl {
                        word_left(&chars, e.caret)
                    } else {
                        e.caret.saturating_sub(1)
                    };
                    from..e.caret
                } else {
                    let to = if ctrl {
                        word_right(&chars, e.caret)
                    } else {
                        (e.caret + 1).min(chars.len())
                    };
                    e.caret..to
                };
                remove(e, &mut chars, range);
                true
            }
            key::A if ctrl => {
                e.select_all(chars.len());
                true
            }
            key::C | key::INSERT if ctrl && !sel.is_empty() => {
                input.copied = Some(chars[sel].iter().collect());
                true
            }
            key::X if ctrl && !sel.is_empty() => {
                input.copied = Some(chars[sel.clone()].iter().collect());
                remove(e, &mut chars, sel);
                true
            }
            key::V if ctrl => paste(input, &mut chars, max, lines),
            key::INSERT if shift => paste(input, &mut chars, max, lines),
            _ => false,
        };
        if !taken {
            input.keys.push(k);
        }
    }
    let after: String = chars.into_iter().collect();
    input.edit.seen.clone_from(&after);
    *text = after;
    *text != before
}

/// The pointer on a line drawn from `x` along `area`, `shown` its characters from the first one
/// shown: a press in it (`pressed`, already taken by the box) places the caret, Shift extends the
/// selection to it, a double-click selects the word under it, and a drag selects to wherever the
/// pointer goes, the line scrolling when it goes past either end.
#[allow(clippy::too_many_arguments)]
pub fn pointer(
    p: &Painter<'_>,
    input: &mut InputFrame,
    style: &TextStyle,
    text: &str,
    area: Rect,
    pressed: bool,
    double: bool,
) {
    let chars: Vec<char> = text.chars().collect();
    let first = input.edit.first.min(chars.len());
    let mx = input.mouse.0;
    let at = |mx: f32| -> usize {
        if mx < area.x {
            return first.saturating_sub(1);
        }
        if mx > area.right() {
            return (last_shown(p, style, &chars, first, area.w) + 1).min(chars.len());
        }
        first + char_at(p, style, &chars[first..], mx - area.x)
    };
    let e = &mut input.edit;
    if pressed {
        let c = at(mx);
        if double {
            let start = if chars.get(c).copied().is_some_and(is_word) {
                word_left(&chars, c + 1)
            } else {
                word_left(&chars, c)
            };
            e.anchor = start;
            e.caret = word_end(&chars, start);
            e.dragging = false;
            return;
        }
        e.caret = c;
        if !input.shift {
            e.anchor = c;
        }
        e.dragging = true;
    } else if e.dragging {
        if input.down[0] {
            e.caret = at(mx);
        } else {
            e.dragging = false;
        }
    }
}

/// The line drawn along `area` with its letters' line starting `top` down, the caret `caret_top`
/// down: the selection under the text, the text scrolled so the caret is in view, and the caret
/// while `blink` shows it. The width of the text drawn.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    p: &mut Painter<'_>,
    edit: &mut EditState,
    style: &TextStyle,
    text: &str,
    area: Rect,
    top: f32,
    caret_top: f32,
    blink: bool,
) -> f32 {
    let chars: Vec<char> = text.chars().collect();
    let width = |p: &Painter<'_>, from: usize, to: usize| -> f32 {
        let s: String = chars[from..to].iter().collect();
        p.measure(style, &s)
    };
    let caret = edit.caret.min(chars.len());
    // Scrolled so the caret shows, and no further than it needs to be.
    let mut first = edit.first.min(caret);
    while first < caret && width(p, first, caret) > area.w {
        first += 1;
    }
    while first > 0 && width(p, first - 1, chars.len()) <= area.w {
        first -= 1;
    }
    edit.first = first;
    let sel = edit.selection();
    let lh = p.line_height(style);
    p.list.push_clip(area);
    let shown: String = chars[first..].iter().collect();
    let w = p.text(style, area.x.round(), top, &shown);
    // The selection over the text, which shows through it: under the letters' dark edges it
    // would hardly show.
    if !sel.is_empty() {
        let (s, e) = (sel.start.max(first), sel.end.max(first));
        let x0 = area.x.round() + width(p, first, s);
        let x1 = area.x.round() + width(p, first, e);
        p.fill(
            Rect::new(x0, caret_top + lh * 0.14, x1 - x0, lh * 0.74),
            SELECTION,
        );
    }
    p.list.pop_clip();
    if blink {
        let x = area.x.round() + width(p, first, caret) + 1.0 * p.scale;
        crate::ui::kit::caret(p, style, x, caret_top);
    }
    w
}

/// Which character a point `x` along `chars` (drawn from 0) is before: the nearest gap.
#[must_use]
pub fn char_at(p: &Painter<'_>, style: &TextStyle, chars: &[char], x: f32) -> usize {
    let mut prev = 0.0;
    let mut s = String::new();
    for (i, c) in chars.iter().enumerate() {
        s.push(*c);
        let w = p.measure(style, &s);
        if x < (prev + w) / 2.0 {
            return i;
        }
        prev = w;
    }
    chars.len()
}

/// The last character of `chars` from `first` that fits in `w`.
fn last_shown(p: &Painter<'_>, style: &TextStyle, chars: &[char], first: usize, w: f32) -> usize {
    let mut s = String::new();
    let mut last = first;
    for (i, c) in chars.iter().enumerate().skip(first) {
        s.push(*c);
        if p.measure(style, &s) > w {
            break;
        }
        last = i;
    }
    last
}

/// `s` typed over the selection, as much as `max` leaves room for; a line break is kept in a box
/// of several `lines` and is a space in a one-line box.
fn insert(e: &mut EditState, chars: &mut Vec<char>, s: &str, max: usize, lines: bool) {
    let sel = e.selection();
    remove(e, chars, sel);
    let room = max.saturating_sub(chars.len());
    let s = s.replace("\r\n", "\n");
    let new: Vec<char> = s
        .chars()
        .map(|c| match c {
            '\n' if lines => '\n',
            '\n' | '\t' => ' ',
            c => c,
        })
        .filter(|c| *c == '\n' || !c.is_control())
        .take(room)
        .collect();
    let n = new.len();
    chars.splice(e.caret..e.caret, new);
    e.caret += n;
    e.anchor = e.caret;
}

fn remove(e: &mut EditState, chars: &mut Vec<char>, range: std::ops::Range<usize>) {
    let range = range.start.min(chars.len())..range.end.min(chars.len());
    let start = range.start;
    chars.drain(range);
    e.caret = start;
    e.anchor = start;
}

fn paste(input: &mut InputFrame, chars: &mut Vec<char>, max: usize, lines: bool) -> bool {
    if let Some(text) = input.paste.take() {
        // Pasted into one line, a run of lines joins into one: the line ends become spaces.
        let text = text.replace("\r\n", "\n");
        let text = if lines {
            text.as_str()
        } else {
            text.trim_end_matches('\n')
        };
        insert(&mut input.edit, chars, text, max, lines);
    }
    true
}

/// One row of a box of several lines as drawn: the characters of the text it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
}

/// `text` laid out in rows no wider than `width`: each line as typed (its spaces and all), a line
/// too wide broken at its spaces as [`Painter::wrap`] breaks it.
#[must_use]
pub fn rows(p: &Painter<'_>, style: &TextStyle, text: &str, width: f32) -> Vec<Row> {
    let mut out = Vec::new();
    let mut at = 0;
    for para in text.split('\n') {
        let chars: Vec<char> = para.chars().collect();
        if chars.is_empty() || p.measure(style, para) <= width {
            out.push(Row {
                start: at,
                end: at + chars.len(),
            });
        } else {
            // Each piece found where it starts in the line: the breaks drop spaces.
            let mut from = 0;
            for piece in p.wrap(style, para, width) {
                let piece: Vec<char> = piece.chars().collect();
                while from < chars.len() && !chars[from..].starts_with(&piece) {
                    from += 1;
                }
                let len = piece.len().min(chars.len() - from);
                out.push(Row {
                    start: at + from,
                    end: at + from + len,
                });
                from += len;
            }
        }
        at += chars.len() + 1;
    }
    out
}

/// The row the caret at `c` is drawn on.
#[must_use]
pub fn row_of(rows: &[Row], c: usize) -> usize {
    rows.iter().rposition(|r| r.start <= c).unwrap_or(0)
}

/// A box of several lines: this frame's keys applied to `text` as [`keys`] applies them, with
/// Enter starting a new line (counted as one character of `max`), Up and Down moving between
/// the rows as `rows` lays them out at `width`, and Home and End going to the ends of the caret's
/// row (with Control, of the whole text). `true` when the text changed.
pub fn lines_keys(
    p: &Painter<'_>,
    style: &TextStyle,
    input: &mut InputFrame,
    text: &mut String,
    max: usize,
    width: f32,
) -> bool {
    let laid = rows(p, style, text, width);
    let chars: Vec<char> = text.chars().collect();
    let (shift, ctrl) = (input.shift, input.ctrl);
    let mut newline = false;
    let pending = std::mem::take(&mut input.keys);
    for k in pending {
        let e = &mut input.edit;
        let r = row_of(&laid, e.caret);
        let row = laid[r];
        let to = match k {
            vk::ENTER => {
                newline = true;
                None
            }
            vk::UP | vk::DOWN => {
                let before: String = chars[row.start..e.caret.clamp(row.start, row.end)]
                    .iter()
                    .collect();
                let x = p.measure(style, &before);
                let other = if k == vk::UP {
                    r.checked_sub(1)
                } else {
                    Some(r + 1).filter(|n| *n < laid.len())
                };
                Some(match other {
                    Some(n) => {
                        let o = laid[n];
                        o.start + char_at(p, style, &chars[o.start..o.end], x)
                    }
                    None if k == vk::UP => 0,
                    None => chars.len(),
                })
            }
            vk::HOME if !ctrl => Some(row.start),
            vk::END if !ctrl => Some(row.end),
            _ => {
                input.keys.push(k);
                continue;
            }
        };
        if let Some(to) = to {
            e.caret = to;
            if !shift {
                e.anchor = to;
            }
        }
    }
    if newline {
        input.chars.push('\n');
    }
    keys_in(input, text, max, true)
}

/// The pointer on a box of several lines whose rows start at `(x, top)`, `line_h` apart: a
/// press (`pressed`, already taken by the box) places the caret, Shift extends the selection to
/// it, a double-click selects the word under it, and a drag selects to wherever it goes.
#[allow(clippy::too_many_arguments)]
pub fn lines_pointer(
    p: &Painter<'_>,
    input: &mut InputFrame,
    style: &TextStyle,
    text: &str,
    laid: &[Row],
    (x, top): (f32, f32),
    line_h: f32,
    pressed: bool,
) {
    let chars: Vec<char> = text.chars().collect();
    let (mx, my) = input.mouse;
    let at = || -> usize {
        if laid.is_empty() {
            return 0;
        }
        if my < top {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((my - top) / line_h) as usize;
        let Some(row) = laid.get(n) else {
            return chars.len();
        };
        row.start + char_at(p, style, &chars[row.start..row.end], mx - x)
    };
    let double = pressed && input.double;
    let e = &mut input.edit;
    if pressed {
        let c = at();
        if double {
            let start = if chars.get(c).copied().is_some_and(is_word) {
                word_left(&chars, c + 1)
            } else {
                word_left(&chars, c)
            };
            e.anchor = start;
            e.caret = word_end(&chars, start);
            e.dragging = false;
            return;
        }
        e.caret = c;
        if !input.shift {
            e.anchor = c;
        }
        e.dragging = true;
    } else if e.dragging {
        if input.down[0] {
            e.caret = at();
        } else {
            e.dragging = false;
        }
    }
}

/// The scroll that shows the caret's row in a view `view_h` tall, from `scroll`.
#[must_use]
pub fn follow_caret(laid: &[Row], caret: usize, line_h: f32, view_h: f32, scroll: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let top = row_of(laid, caret) as f32 * line_h;
    if top < scroll {
        top
    } else if top + line_h > scroll + view_h {
        top + line_h - view_h
    } else {
        scroll
    }
}

/// The rows of a box of several lines drawn from `(x, top)`, `line_h` apart, clipped to
/// `clip`: the text, the selection over it, and the caret while `edit` is given and `blink`
/// shows it.
#[allow(clippy::too_many_arguments)]
pub fn draw_lines(
    p: &mut Painter<'_>,
    edit: Option<&EditState>,
    style: &TextStyle,
    text: &str,
    laid: &[Row],
    (x, top): (f32, f32),
    line_h: f32,
    clip: Rect,
    blink: bool,
) {
    let chars: Vec<char> = text.chars().collect();
    let piece = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };
    let sel = edit.map(EditState::selection).unwrap_or_default();
    let caret_row = edit.map(|e| row_of(laid, e.caret.min(chars.len())));
    p.list.push_clip(clip);
    for (n, row) in laid.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = top + line_h * n as f32;
        if y + line_h < clip.y || y > clip.bottom() {
            continue;
        }
        p.text(style, x, y, &piece(row.start, row.end));
        let (s, e) = (sel.start.max(row.start), sel.end.min(row.end));
        // A selection running on past the row's end lights a little room for the line break.
        let past = sel.end > row.end && sel.start <= row.end;
        if s < e || past {
            let x0 = x + p.measure(style, &piece(row.start, s.min(row.end)));
            let x1 = x + p.measure(style, &piece(row.start, e.max(s).min(row.end)));
            let x1 = if past { x1 + 4.0 * p.scale } else { x1 };
            p.fill(
                Rect::new(x0, y + line_h * 0.14, x1 - x0, line_h * 0.74),
                SELECTION,
            );
        }
        if blink && caret_row == Some(n) {
            if let Some(edit) = edit {
                let c = edit.caret.clamp(row.start, row.end);
                let cx = x + p.measure(style, &piece(row.start, c)) + 1.0 * p.scale;
                crate::ui::kit::caret(p, style, cx, y);
            }
        }
    }
    p.list.pop_clip();
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

/// The start of the word before `at`.
fn word_left(chars: &[char], at: usize) -> usize {
    let mut i = at.min(chars.len());
    while i > 0 && !is_word(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word(chars[i - 1]) {
        i -= 1;
    }
    i
}

/// The start of the word after `at`.
fn word_right(chars: &[char], at: usize) -> usize {
    let mut i = at.min(chars.len());
    while i < chars.len() && is_word(chars[i]) {
        i += 1;
    }
    while i < chars.len() && !is_word(chars[i]) {
        i += 1;
    }
    i
}

/// The end of the word `at` is in.
fn word_end(chars: &[char], at: usize) -> usize {
    let mut i = at.min(chars.len());
    while i < chars.len() && is_word(chars[i]) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own text editing)
    use super::*;

    /// A frame's keys on `text`, with Shift and Control as given.
    fn press(input: &mut InputFrame, text: &mut String, keys: &[usize], shift: bool, ctrl: bool) {
        input.shift = shift;
        input.ctrl = ctrl;
        input.keys = keys.to_vec();
        keys_frame(input, text);
    }

    fn keys_frame(input: &mut InputFrame, text: &mut String) {
        begin(input, 1, text);
        keys(input, text, 64);
        input.next_frame();
        input.text_focus = true;
    }

    fn typed(input: &mut InputFrame, text: &mut String, s: &str) {
        input.chars = s.chars().collect();
        keys_frame(input, text);
    }

    #[test]
    fn the_arrows_move_the_caret_and_typing_goes_in_at_it() {
        let mut input = InputFrame::default();
        let mut text = String::new();
        typed(&mut input, &mut text, "helo");
        press(&mut input, &mut text, &[vk::LEFT], false, false);
        typed(&mut input, &mut text, "l");
        assert_eq!(text, "hello");
        press(&mut input, &mut text, &[vk::HOME], false, false);
        typed(&mut input, &mut text, ">");
        press(&mut input, &mut text, &[vk::END], false, false);
        typed(&mut input, &mut text, "!");
        assert_eq!(text, ">hello!");
        press(&mut input, &mut text, &[vk::HOME, vk::DELETE], false, false);
        assert_eq!(text, "hello!");
        press(&mut input, &mut text, &[vk::END, vk::BACK], false, false);
        assert_eq!(text, "hello");
    }

    #[test]
    fn shift_arrows_select_and_typing_replaces_the_selection() {
        let mut input = InputFrame::default();
        let mut text = String::new();
        typed(&mut input, &mut text, "good day");
        press(
            &mut input,
            &mut text,
            &[vk::LEFT, vk::LEFT, vk::LEFT],
            true,
            false,
        );
        assert_eq!(input.edit.selection(), 5..8);
        typed(&mut input, &mut text, "night");
        assert_eq!(text, "good night");
        // An arrow without Shift drops the selection at its end.
        press(&mut input, &mut text, &[vk::HOME], true, false);
        press(&mut input, &mut text, &[vk::LEFT], false, false);
        assert_eq!((input.edit.caret, input.edit.anchor), (0, 0));
    }

    #[test]
    fn control_a_c_x_and_v_select_copy_cut_and_paste() {
        let mut input = InputFrame::default();
        let mut text = String::new();
        typed(&mut input, &mut text, "Holtburg");
        press(&mut input, &mut text, &[key::A, key::C], false, true);
        assert_eq!(input.copied.take().as_deref(), Some("Holtburg"));
        assert_eq!(text, "Holtburg", "copying leaves the text");
        press(&mut input, &mut text, &[key::X], false, true);
        assert_eq!(input.copied.take().as_deref(), Some("Holtburg"));
        assert!(text.is_empty());
        input.paste = Some("Yaraq\r\n".into());
        press(&mut input, &mut text, &[key::V], false, true);
        assert_eq!(text, "Yaraq");
        // Nothing selected: Control-C copies nothing and leaves the key for others.
        press(&mut input, &mut text, &[key::C], false, true);
        assert!(input.copied.is_none());
    }

    #[test]
    fn a_line_kept_to_its_length_takes_only_what_fits_of_a_paste() {
        let mut input = InputFrame::default();
        let mut text = "a".repeat(60);
        begin(&mut input, 1, &text);
        input.paste = Some("bcdefgh".into());
        input.ctrl = true;
        input.keys = vec![key::V];
        keys(&mut input, &mut text, 64);
        assert_eq!(text.chars().count(), 64);
        assert!(text.ends_with("bcde"));
    }

    #[test]
    fn control_moves_and_deletes_by_words() {
        let mut input = InputFrame::default();
        let mut text = String::new();
        typed(&mut input, &mut text, "tell Cora hello");
        press(&mut input, &mut text, &[vk::LEFT], false, true);
        assert_eq!(input.edit.caret, 10);
        press(&mut input, &mut text, &[vk::BACK], false, true);
        assert_eq!(text, "tell hello");
    }

    #[test]
    fn text_changed_from_outside_puts_the_caret_at_its_end() {
        let mut input = InputFrame::default();
        let mut text = String::new();
        typed(&mut input, &mut text, "abc");
        press(&mut input, &mut text, &[vk::HOME], false, false);
        text = "recalled line".into();
        typed(&mut input, &mut text, "!");
        assert_eq!(text, "recalled line!");
    }

    #[test]
    fn the_keys_a_line_does_not_answer_are_left_for_its_owner() {
        let mut input = InputFrame::default();
        let mut text = String::from("x");
        begin(&mut input, 1, &text);
        input.keys = vec![vk::ENTER, vk::LEFT, vk::ESCAPE, vk::UP];
        keys(&mut input, &mut text, 64);
        assert_eq!(input.keys, vec![vk::ENTER, vk::ESCAPE, vk::UP]);
    }

    /// A frame of a box of several lines over `text`, its rows its lines (no faces, so nothing
    /// is wider than the box).
    fn lines_frame(input: &mut InputFrame, text: &mut String, keys: &[usize], max: usize) {
        let art = crate::art::Art::empty();
        let mut list = crate::draw::DrawList::default();
        let p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        let style = TextStyle::new(crate::art::Family::Body, 12.0, 0xFFFF_FFFF);
        begin(input, 2, text);
        input.keys = keys.to_vec();
        lines_keys(&p, &style, input, text, max, 300.0);
        input.next_frame();
        input.text_focus = true;
    }

    #[test]
    fn in_a_box_of_several_lines_enter_breaks_the_line_and_up_down_home_end_go_by_rows() {
        let mut input = InputFrame::default();
        let mut text = String::from("first\nsecond");
        lines_frame(&mut input, &mut text, &[], 300);
        assert_eq!(input.edit.caret, 12);
        lines_frame(&mut input, &mut text, &[vk::HOME], 300);
        assert_eq!(input.edit.caret, 6, "Home is the row's start");
        lines_frame(&mut input, &mut text, &[vk::UP], 300);
        assert!(input.edit.caret <= 5, "Up is on the first row");
        lines_frame(&mut input, &mut text, &[vk::END], 300);
        assert_eq!(
            input.edit.caret, 5,
            "End is the row's end, before its line break"
        );
        lines_frame(&mut input, &mut text, &[vk::ENTER], 300);
        assert_eq!(text, "first\n\nsecond");
        lines_frame(&mut input, &mut text, &[vk::DOWN, vk::DOWN], 300);
        assert_eq!(
            input.edit.caret,
            text.chars().count(),
            "Down past the last row is the end"
        );
        input.shift = true;
        lines_frame(&mut input, &mut text, &[vk::UP], 300);
        input.shift = false;
        assert!(!input.edit.selection().is_empty(), "Shift-Up selects");
        input.ctrl = true;
        lines_frame(&mut input, &mut text, &[vk::HOME], 300);
        input.ctrl = false;
        assert_eq!(input.edit.caret, 0, "Control-Home is the text's start");
    }

    #[test]
    fn a_box_of_several_lines_keeps_pasted_line_breaks_and_counts_each_as_a_character() {
        let mut input = InputFrame::default();
        let mut text = String::new();
        input.paste = Some("one\r\ntwo\nthree".into());
        input.ctrl = true;
        lines_frame(&mut input, &mut text, &[key::V], 9);
        assert_eq!(
            text, "one\ntwo\nt",
            "nine characters, the two breaks among them"
        );
    }
}
