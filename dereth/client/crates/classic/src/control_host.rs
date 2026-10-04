//! Persistent control routing and painting shared by all classic panel families.
use crate::int::{i32_from, i32_from_i64, u32_from};
use crate::{
    panels::*,
    widgets::{Action, Input, Node, NodeId, Rect, UiTree, WidgetKind},
    Command, Screen, TextAlign,
};
use std::collections::BTreeMap;

// The art dropdown: its face and popup rows (117x25, the highlighted row lit), the arrow beside
// the face (26x26, shut or open), and the popup's 8-pixel top and bottom edges.
const ART_FACE_WIDTH: i32 = 117;
const ART_ROW_HEIGHT: i32 = 25;
const ART_ARROW: i32 = 26;
const ART_EDGE: i32 = 8;
const ART_ROW: &str = "0600050A";
const ART_ROW_LIT: &str = "06000509";
const ART_ARROW_SHUT: &str = "060004EA";
const ART_ARROW_OPEN: &str = "060004EB";
const ART_POPUP_TOP: &str = "06001203";
const ART_POPUP_BOTTOM: &str = "060011F2";

// The chat destination list: 191-pixel rows 17 pixels tall, on row art for an ordinary row, the
// destination in use and the row under the pointer, with the text 18 pixels in.
const CHAT_POPUP_WIDTH: i32 = 191;
const CHAT_ROW_HEIGHT: i32 = 17;
const CHAT_ROW: &str = "0600124E";
const CHAT_ROW_CURRENT: &str = "0600124D";
const CHAT_ROW_LIT: &str = "0600124B";

/// An open dropdown's popup: all of it, the part its rows fill, and one row's height.
struct Popup {
    outer: Rect,
    rows: Rect,
    row_height: i32,
}

/// The row an arrow key moves a dropdown's highlight to: the next enabled row after (or before)
/// `from`, wrapping past either end; `from` itself when no other row is enabled.
fn cycle_choice(
    from: usize,
    count: usize,
    forward: bool,
    enabled: impl Fn(usize) -> bool,
) -> usize {
    if count == 0 {
        return from;
    }
    let from = from.min(count - 1);
    (1..count)
        .map(|k| {
            if forward {
                (from + k) % count
            } else {
                (from + count - k) % count
            }
        })
        .find(|&i| enabled(i))
        .unwrap_or(from)
}

/// A line of an art dropdown's text, inset on its row.
fn art_choice_text(out: &mut PanelFrame, row: Rect, text: &str, font: &str, color: u32) {
    out.text_box(
        rect(row.x + 4, row.y + 5, row.w - 8, row.h - 5),
        text,
        font,
        color,
        TextAlign::Left,
        false,
        Some([row.x, row.y, row.x + row.w, row.y + row.h]),
    );
}

/// The green square lit on the slot a dragged spell would land in.
const SLOT_HINT: &str = "060011F9";
#[derive(Debug, Default)]
pub struct ControlHost {
    fonts: crate::renderer::FontMetrics,
    tree: UiTree,
    ids: BTreeMap<String, NodeId>,
    controls: Vec<Control>,
    pointer: (i32, i32),
    dragging: Option<(String, DragPayload, (i32, i32))>,
    last_click: Option<(String, usize, dereth_primitives::LocalTime)>,
    slider: Option<String>,
    choice: Option<(String, usize)>,
    size: (u32, u32),
    edit_click: Option<(NodeId, (i32, i32), dereth_primitives::LocalTime)>,
    item_feedback: Option<(String, usize, u32)>,
    canvas_feedback: Option<(Rect, bool)>,
    /// The slot a dragged spell would land in, lit while the drag is over it.
    slot_hint: Option<Rect>,
    sounds: Vec<u32>,
    sound_scroll: Option<(String, bool, Rect)>,
    settings_slider_grab: Option<i32>,
}

impl ControlHost {
    pub fn new(fonts: crate::renderer::FontMetrics) -> Self {
        Self {
            fonts,
            ..Self::default()
        }
    }
    pub fn set_fonts(&mut self, fonts: crate::renderer::FontMetrics) {
        self.fonts = fonts;
    }
    /// Sound types from the classic interface's sound table (0x2000004B). The host applies the
    /// UI sound preference and plays these against its local audio assets.
    pub fn take_sounds(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.sounds)
    }
    fn alternate_states(&self) -> BTreeMap<String, bool> {
        let mut states = BTreeMap::new();
        for c in &self.controls {
            if !c.enabled {
                continue;
            }
            let node = self.ids[&c.id];
            if self.tree.clip_rect(node).is_none() {
                continue;
            }
            let state = match self.tree.node(node).kind {
                WidgetKind::Checkbox { checked } if matches!(c.kind, ControlKind::Check { .. }) => {
                    checked ^ self.tree.pressed(node)
                }
                WidgetKind::Button if matches!(c.kind, ControlKind::Button { .. }) => {
                    self.tree.pressed(node)
                }
                _ => continue,
            };
            states.insert(c.id.clone(), state);
        }
        if let Some((id, thumb, rect)) = &self.sound_scroll {
            if !thumb && self.controls.iter().any(|c| c.id == *id && c.enabled) {
                states.insert(
                    format!("{id}:arrow"),
                    rect.contains(self.pointer.0, self.pointer.1),
                );
            }
        }
        states
    }
    fn scroll_sound_capture(&self, x: i32, y: i32) -> Option<(String, bool, Rect)> {
        let c = self.control_at(x, y).filter(|c| c.enabled && !c.silent)?;
        // The options Client page's sliders have no arrows and jump to the pointer, so a
        // press on the track drags the thumb as if it had been pressed on it.
        if matches!(c.kind, ControlKind::Slider { .. }) && Self::settings_slider(c) {
            return Some((c.id.clone(), true, c.rect));
        }
        let ControlKind::ScrollBar {
            min,
            max,
            value,
            vertical,
            arrow_size,
            thumb_size,
            ..
        } = c.kind
        else {
            return None;
        };
        let r = c.rect;
        let extent = if vertical { r.h } else { r.w };
        let arrow = arrow_size.min(extent / 2).max(0);
        let first = if vertical {
            rect(r.x, r.y, r.w, arrow)
        } else {
            rect(r.x, r.y, arrow, r.h)
        };
        let last = if vertical {
            rect(r.x, r.y + r.h - arrow, r.w, arrow)
        } else {
            rect(r.x + r.w - arrow, r.y, arrow, r.h)
        };
        for button in [first, last] {
            if button.contains(x, y) {
                return Some((c.id.clone(), false, button));
            }
        }
        let thumb = if vertical {
            self.tree.scrollbar_thumb(self.ids[&c.id])?
        } else {
            let travel = (r.w - 2 * arrow + 1 - thumb_size).max(0);
            let offset = if max > min {
                i32_from_i64(i64::from(value - min) * i64::from(travel) / i64::from(max - min))
            } else {
                0
            };
            rect(r.x + arrow + offset, r.y, thumb_size, r.h)
        };
        thumb.contains(x, y).then(|| (c.id.clone(), true, thumb))
    }
    pub fn tick(&mut self, now: f64) -> Vec<ControlEvent> {
        self.tree
            .tick(now)
            .into_iter()
            .filter_map(|action| {
                let Action::Scroll { id, value } = action else {
                    return None;
                };
                let id = self
                    .ids
                    .iter()
                    .find_map(|(name, node)| (*node == id).then_some(name.clone()))?;
                Some(ControlEvent::Scroll { id, value })
            })
            .collect()
    }
    pub fn has_focus(&self) -> bool {
        self.tree.focus().is_some()
    }
    /// Release a hidden/destroyed pane's editing owner and transient input. The focused
    /// control loses focus, and an edit field that had it reports its commit.
    pub fn deactivate(&mut self, now: dereth_primitives::LocalTime) -> Vec<ControlEvent> {
        let edit = self.tree.focus().and_then(|node| {
            self.controls
                .iter()
                .find(|c| self.ids[&c.id] == node && matches!(c.kind, ControlKind::Edit { .. }))
                .map(|c| c.id.clone())
        });
        let mut events = self.handle(Input::Cancel, now);
        self.tree.release_focus();
        self.choice = None;
        self.edit_click = None;
        self.item_feedback = None;
        self.canvas_feedback = None;
        if let Some(id) = edit {
            events.push(ControlEvent::Commit { id });
        }
        events
    }
    pub fn focused_control(&self) -> Option<&str> {
        let focus = self.tree.focus()?;
        self.ids
            .iter()
            .find_map(|(name, id)| (*id == focus).then_some(name.as_str()))
    }
    pub fn focus_control(&mut self, id: &str) -> bool {
        if id.is_empty() {
            self.tree.release_focus();
            return true;
        }
        self.ids
            .get(id)
            .copied()
            .is_some_and(|id| self.tree.focus_node(id))
    }
    pub fn place_caret(&mut self, id: &str, cursor: usize) {
        if let Some(node) = self.ids.get(id) {
            self.tree.place_caret(*node, cursor, false);
        }
    }
    pub fn selected_text(&self) -> Option<String> {
        let id = self.tree.focus()?;
        let (cursor, anchor) = self.tree.edit_selection(id)?;
        let WidgetKind::Edit { text, .. } = &self.tree.node(id).kind else {
            return None;
        };
        (cursor != anchor).then(|| {
            text.chars()
                .skip(cursor.min(anchor))
                .take(cursor.abs_diff(anchor))
                .collect()
        })
    }
    pub fn editing(&self) -> bool {
        self.tree
            .focus()
            .is_some_and(|id| matches!(self.tree.node(id).kind, WidgetKind::Edit { .. }))
    }
    pub fn sync(&mut self, frame: &PanelFrame) {
        self.size = (frame.screen.width, frame.screen.height);
        for n in self.ids.values() {
            self.tree.node_mut(*n).visible = false;
        }
        for c in &frame.controls {
            let kind = match &c.kind {
                ControlKind::ScrollBar {
                    min,
                    max,
                    value,
                    page,
                    step,
                    vertical,
                    ..
                } => {
                    if *vertical {
                        WidgetKind::Scrollbar {
                            min: *min,
                            max: *max,
                            value: *value,
                            page: *page,
                            step: *step,
                        }
                    } else {
                        WidgetKind::Button
                    }
                }
                ControlKind::Button { .. } => WidgetKind::Button,
                ControlKind::Check { checked, .. } => WidgetKind::Checkbox { checked: *checked },
                ControlKind::Edit {
                    text, max_chars, ..
                } => WidgetKind::Edit {
                    text: text.clone(),
                    max_chars: *max_chars,
                },
                ControlKind::List {
                    rows,
                    selected,
                    row_height,
                } => WidgetKind::List {
                    content_height: i32_from(rows.len()) * row_height,
                    offset: 0,
                    row_height: *row_height,
                    selected: *selected,
                },
                ControlKind::HitList {
                    row_count,
                    row_height,
                    selected,
                    offset,
                } => WidgetKind::List {
                    content_height: i32_from(*row_count) * row_height,
                    offset: *offset,
                    row_height: *row_height,
                    selected: *selected,
                },
                ControlKind::Choice { .. }
                | ControlKind::ItemStrip { .. }
                | ControlKind::Slider { .. }
                | ControlKind::Items { .. } => WidgetKind::Button,
            };
            let id = *self
                .ids
                .entry(c.id.clone())
                .or_insert_with(|| self.tree.add(None, Node::new(kind.clone(), c.rect)));
            let node = self.tree.node_mut(id);
            let kind = match (&node.kind, kind) {
                (
                    WidgetKind::List { offset, .. },
                    WidgetKind::List {
                        content_height,
                        row_height,
                        selected,
                        ..
                    },
                ) if matches!(c.kind, ControlKind::List { .. }) => WidgetKind::List {
                    content_height,
                    row_height,
                    selected,
                    offset: (*offset).clamp(0, (content_height - c.rect.h).max(0)),
                },
                (_, kind) => kind,
            };
            node.kind = kind;
            node.rect = c.rect;
            node.visible = true;
            node.enabled = c.enabled;
            self.tree.set_select_on_focus(id, c.select_on_focus);
            self.tree.set_multiline(
                id,
                matches!(
                    c.kind,
                    ControlKind::Edit {
                        multiline: true,
                        ..
                    }
                ),
            );
            if let ControlKind::Edit {
                text, multiline, ..
            } = &c.kind
            {
                let advances: Vec<_> = text
                    .chars()
                    .map(|ch| self.fonts.text_width(&c.font, &ch.to_string()).unwrap_or(6))
                    .collect();
                let layout = crate::text_edit::Layout::new(
                    text,
                    &advances,
                    self.fonts.line_height(&c.font).unwrap_or(15),
                    (c.rect.w - 4).max(1),
                    *multiline,
                );
                self.tree.set_edit_layout(
                    id,
                    layout,
                    rect(2, 1, (c.rect.w - 4).max(1), (c.rect.h - 2).max(1)),
                );
                self.tree.set_edit_smooth_scroll(id, c.smooth_scroll);
            }
            if let ControlKind::ScrollBar {
                arrow_size,
                thumb_size,
                ..
            } = c.kind
            {
                self.tree.set_scrollbar_geometry(id, arrow_size, thumb_size);
            }
        }
        self.controls = frame.controls.clone();
    }
    fn control_at(&self, x: i32, y: i32) -> Option<&Control> {
        self.controls
            .iter()
            .rev()
            .find(|c| c.enabled && c.rect.contains(x, y))
    }
    pub fn contains_control(&self, x: i32, y: i32) -> bool {
        self.popup_open() || self.control_at(x, y).is_some()
    }
    pub fn item_at(&self, x: i32, y: i32) -> Option<ObjectId> {
        let control = self.control_at(x, y)?;
        let index = Self::item_slot(control, x, y)?;
        match &control.kind {
            ControlKind::Items { entries, .. } | ControlKind::ItemStrip { entries, .. } => {
                entries.get(index).map(|item| item.id)
            }
            _ => None,
        }
    }
    pub fn popup_open(&self) -> bool {
        self.choice.is_some()
    }
    /// Where an open dropdown's popup lies: below its face, or above it when it would run off
    /// the bottom of the screen.
    fn choice_popup(&self, c: &Control, count: usize) -> Popup {
        let (width, row_height, face_height, edge, bottom_edge) = if let Some(skin) = c.list_skin {
            (
                c.rect.w - skin.arrow_width,
                c.rect.h,
                c.rect.h,
                skin.top.map_or(0, |t| t.1),
                skin.bottom.map_or(0, |b| b.1),
            )
        } else if c.choice_art {
            (
                ART_FACE_WIDTH,
                ART_ROW_HEIGHT,
                ART_ROW_HEIGHT,
                ART_EDGE,
                ART_EDGE,
            )
        } else if c.chat_popup {
            (CHAT_POPUP_WIDTH, CHAT_ROW_HEIGHT, c.rect.h, 0, 0)
        } else {
            (c.rect.w, c.rect.h, c.rect.h, 0, 0)
        };
        let height = row_height * i32_from(count) + edge + bottom_edge;
        let below = c.rect.y + face_height;
        // Below the face when it fits, else above it, else as low as keeps every row on screen.
        let screen = self.size.1 as i32;
        let y = if below + height <= screen {
            below
        } else if c.rect.y - height >= 0 {
            c.rect.y - height
        } else {
            (screen - height).max(0)
        };
        Popup {
            outer: rect(c.rect.x, y, width, height),
            rows: rect(c.rect.x, y + edge, width, row_height * i32_from(count)),
            row_height: row_height.max(1),
        }
    }
    pub fn drop_event(&self, x: i32, y: i32, payload: DragPayload) -> Option<ControlEvent> {
        let c = self.control_at(x, y)?;
        Some(ControlEvent::Drop {
            id: c.id.clone(),
            payload,
            slot: u32_from(Self::item_slot(c, x, y).unwrap_or(0)),
        })
    }
    fn row_at(&self, c: &Control, x: i32, y: i32) -> Option<usize> {
        Self::item_slot(c, x, y).or_else(|| {
            if let WidgetKind::List {
                offset,
                row_height,
                content_height,
                ..
            } = self.tree.node(self.ids[&c.id]).kind
            {
                let relative = y - c.rect.y + offset;
                (row_height > 0 && relative >= 0 && relative < content_height)
                    .then(|| (relative / row_height) as usize)
            } else {
                None
            }
        })
    }
    /// Coordinates are local to this host. None clears feedback after drag/capture ends.
    /// Light the slot under a dragged spell (`pointer` is in this window's coordinates).
    pub fn update_slot_hint(&mut self, pointer: Option<(i32, i32)>) {
        self.slot_hint = pointer
            .and_then(|(x, y)| self.control_at(x, y))
            .filter(|c| c.slot)
            .map(|c| c.rect);
    }
    pub fn update_item_drop_preview(
        &mut self,
        g: &dyn dereth_client_contract::view::GameView,
        dragged: Option<ObjectId>,
        pointer: Option<(i32, i32)>,
        request_ready: bool,
        forced_mode: u32,
    ) {
        let previous = self.item_feedback.take();
        let previous_canvas = self.canvas_feedback.take();
        let (Some(dragged), Some((x, y))) = (dragged, pointer) else {
            return;
        };
        let Some(control) = self.control_at(x, y) else {
            return;
        };
        if control.drop_equipment_canvas {
            self.canvas_feedback = match g.equipment_hover(dragged).canvas {
                Some(accept) => Some((control.rect, accept)),
                None => previous_canvas.filter(|(rect, _)| *rect == control.rect),
            };
            return;
        }
        let Some(index) = Self::item_slot(control, x, y) else {
            return;
        };
        let (entries, slot_size) = match &control.kind {
            ControlKind::Items {
                entries, slot_size, ..
            }
            | ControlKind::ItemStrip {
                entries, slot_size, ..
            } => (entries, *slot_size),
            _ => return,
        };
        let Some(entry) = entries.get(index) else {
            return;
        };
        if let Some(filter) = control.drop_filter {
            let takes = match filter {
                crate::panels::DropFilter::Salvage { material } => {
                    g.item_owned_by_player(dragged) && g.salvage_item_suitable(dragged, material)
                }
                crate::panels::DropFilter::Trade => g.trade_drag_item_acceptable(dragged),
            };
            let art = if takes { 0x060011f9 } else { 0x060011f8 };
            self.item_feedback = Some((control.id.clone(), index, art));
            return;
        }
        if let Some(location) = control.drop_location {
            if let Some(art) = g
                .equipment_hover(dragged)
                .at_location(location)
                .map(|accept| if accept { 0x060011f9 } else { 0x060011f8 })
            {
                self.item_feedback = Some((control.id.clone(), index, art));
            } else {
                self.item_feedback =
                    previous.filter(|(id, slot, _)| *id == control.id && *slot == index);
            }
            return;
        }
        let art = crate::item_art::drop_feedback(
            g,
            dragged,
            entry.id,
            slot_size == 36,
            forced_mode,
            request_ready,
        );
        self.item_feedback = Some((control.id.clone(), index, art));
    }
    fn item_feedback(&self, id: &str, index: usize) -> Option<u32> {
        self.item_feedback
            .as_ref()
            .filter(|(control, slot, _)| control == id && *slot == index)
            .map(|(_, _, art)| *art)
    }
    fn item_slot(c: &Control, x: i32, y: i32) -> Option<usize> {
        if let ControlKind::ItemStrip {
            slot_size, offset, ..
        } = c.kind
        {
            return (slot_size > 0 && c.rect.contains(x, y))
                .then(|| ((x - c.rect.x + offset) / slot_size).max(0) as usize);
        }
        if let ControlKind::Items {
            columns, slot_size, ..
        } = c.kind
        {
            if slot_size > 0 && columns > 0 && c.rect.contains(x, y) {
                return Some(
                    ((y - c.rect.y) / slot_size) as usize * columns as usize
                        + ((x - c.rect.x) / slot_size) as usize,
                );
            }
        }
        None
    }
    fn slider_event(c: &Control, x: i32, y: i32) -> Option<ControlEvent> {
        // A horizontal scroll bar's arrows step it by one step (one cell of a strip); the rest
        // of the bar places it where it was clicked.
        if let ControlKind::ScrollBar {
            min,
            max,
            value,
            step,
            arrow_size,
            vertical: false,
            ..
        } = c.kind
        {
            let arrow = arrow_size.min(c.rect.w / 2).max(0);
            let delta = if x < c.rect.x + arrow {
                Some(-step.max(1))
            } else if x >= c.rect.x + c.rect.w - arrow {
                Some(step.max(1))
            } else {
                None
            };
            if let Some(delta) = delta {
                return Some(ControlEvent::Scroll {
                    id: c.id.clone(),
                    value: value.saturating_add(delta).clamp(min, max.max(min)),
                });
            }
        }
        let (min, max, step) = match c.kind {
            ControlKind::Slider { min, max, step, .. } => (min, max, step),
            ControlKind::ScrollBar {
                min,
                max,
                step,
                vertical: false,
                ..
            } => (min, max, step),
            _ => return None,
        };
        let (mut position, mut length) = if c.rect.w >= c.rect.h {
            (x - c.rect.x, c.rect.w)
        } else {
            (y - c.rect.y, c.rect.h)
        };
        if Self::settings_slider(c) {
            position -= 3;
            length -= 5;
        }
        let raw = min as i64
            + (max as i64 - min as i64) * i64::from(position.clamp(0, (length - 1).max(0)))
                / i64::from((length - 1).max(1));
        let step = step.max(1) as i64;
        let value = i32_from_i64(
            (min as i64 + ((raw - min as i64 + step / 2) / step) * step)
                .clamp(min as i64, max.max(min) as i64),
        );
        Some(if matches!(c.kind, ControlKind::ScrollBar { .. }) {
            ControlEvent::Scroll {
                id: c.id.clone(),
                value,
            }
        } else {
            ControlEvent::Value {
                id: c.id.clone(),
                value,
            }
        })
    }
    fn settings_slider(c: &Control) -> bool {
        c.images
            .as_ref()
            .is_some_and(|images| images[1] == "06001286")
    }
    fn slider_event_at(&self, c: &Control, x: i32, y: i32) -> Option<ControlEvent> {
        let x = if Self::settings_slider(c) {
            x + 3 - self.settings_slider_grab.unwrap_or(3)
        } else {
            x
        };
        Self::slider_event(c, x, y)
    }
    pub fn handle(&mut self, input: Input, now: dereth_primitives::LocalTime) -> Vec<ControlEvent> {
        let before = self.alternate_states();
        let release = matches!(input, Input::PointerUp { .. });
        let cancel = matches!(input, Input::Cancel);
        let keyboard = matches!(
            input,
            Input::Key {
                key: crate::widgets::Key::Enter,
                ..
            }
        );
        if let Input::PointerDown { x, y } = input {
            self.settings_slider_grab = self
                .control_at(x, y)
                .filter(|c| c.enabled && Self::settings_slider(c))
                .and_then(|c| {
                    let ControlKind::Slider {
                        min, max, value, ..
                    } = c.kind
                    else {
                        return None;
                    };
                    let offset = if max > min {
                        i32_from_i64(
                            i64::from(value - min) * i64::from((c.rect.w - 6).max(0))
                                / i64::from(max - min),
                        )
                    } else {
                        0
                    };
                    let left = c.rect.x + offset;
                    Some(if x >= left && x < left + 7 {
                        x - left
                    } else {
                        3
                    })
                });
            self.sound_scroll = if self.choice.is_none() {
                self.scroll_sound_capture(x, y)
            } else {
                None
            };
        }
        let thumb_release = release
            && self.sound_scroll.as_ref().is_some_and(|(id, thumb, _)| {
                *thumb && self.controls.iter().any(|c| c.id == *id && c.enabled)
            });
        let input = self.wheel_target(input);
        let events = self.handle_inner(input, now);
        if release || cancel {
            self.sound_scroll = None;
            self.settings_slider_grab = None;
        }
        for (id, alternate) in self.alternate_states() {
            if alternate
                && !before.get(&id).copied().unwrap_or(false)
                && self.controls.iter().any(|c| {
                    !c.silent
                        && (c.id == id
                            || (matches!(c.kind, ControlKind::ScrollBar { .. })
                                && id.strip_suffix(":arrow") == Some(c.id.as_str())))
                })
            {
                self.sounds.push(0x72);
            }
        }
        // Enter performs both visual transitions inside the native handler;
        // its final state alone cannot retain the pressed-edge sound.
        if keyboard {
            for event in &events {
                if let ControlEvent::Activate(id) = event {
                    if self.controls.iter().any(|c| {
                        c.id == *id
                            && c.enabled
                            && !c.silent
                            && matches!(c.kind, ControlKind::Button { .. })
                    }) {
                        self.sounds.push(0x72);
                    }
                }
            }
        }
        if thumb_release {
            self.sounds.push(0x74);
        }
        events
    }
    /// The wheel over a window scrolls what is under the pointer; over anything else in a
    /// window with one vertical scroll bar, it scrolls that bar, as it would over the bar itself.
    fn wheel_target(&self, input: Input) -> Input {
        let Input::Wheel { x, y, delta } = input else {
            return input;
        };
        // An open drop-down takes the wheel itself.
        if self.choice.is_some() {
            return input;
        }
        if self.control_at(x, y).is_some_and(|c| {
            matches!(
                c.kind,
                ControlKind::List { .. }
                    | ControlKind::HitList { .. }
                    | ControlKind::ScrollBar { vertical: true, .. }
            )
        }) {
            return input;
        }
        let mut bars = self.controls.iter().filter(|c| {
            c.enabled && matches!(c.kind, ControlKind::ScrollBar { vertical: true, .. })
        });
        match (bars.next(), bars.next()) {
            (Some(bar), None) => Input::Wheel {
                x: bar.rect.x + bar.rect.w / 2,
                y: bar.rect.y + bar.rect.h / 2,
                delta,
            },
            _ => input,
        }
    }
    fn handle_inner(
        &mut self,
        input: Input,
        now: dereth_primitives::LocalTime,
    ) -> Vec<ControlEvent> {
        use crate::widgets::Key;
        let edit_pointer = match input {
            Input::PointerDown { x, y } => self
                .control_at(x, y)
                .filter(|c| matches!(c.kind, ControlKind::Edit { .. }))
                .map(|c| (self.ids[&c.id], x, y)),
            _ => None,
        };
        if let Some((id, mut selected)) = self.choice.clone() {
            if let Some(c) = self.controls.iter().find(|c| c.id == id) {
                if let ControlKind::Choice { options, .. } = &c.kind {
                    let count = options.len();
                    let enabled = |i: usize| {
                        c.choice_enabled
                            .as_ref()
                            .is_none_or(|rows| rows.get(i).copied().unwrap_or(false))
                    };
                    let Popup {
                        outer,
                        rows: popup,
                        row_height,
                    } = self.choice_popup(c, count);
                    match input {
                        Input::Key {
                            key: Key::Escape, ..
                        }
                        | Input::Cancel => {
                            self.choice = None;
                            return vec![];
                        }
                        Input::Key { key: Key::Up, .. } => {
                            selected = cycle_choice(selected, count, false, enabled);
                        }
                        Input::Key { key: Key::Down, .. } => {
                            selected = cycle_choice(selected, count, true, enabled);
                        }
                        Input::Key {
                            key: Key::Enter, ..
                        } => {
                            self.choice = None;
                            if !enabled(selected) {
                                return vec![];
                            }
                            return vec![ControlEvent::Select {
                                id,
                                index: selected,
                            }];
                        }
                        Input::PointerMove { x, y } if popup.contains(x, y) => {
                            let row = ((y - popup.y) / row_height) as usize;
                            if enabled(row) {
                                selected = row;
                            }
                        }
                        Input::PointerUp { x, y } if popup.contains(x, y) => {
                            let row = ((y - popup.y) / row_height) as usize;
                            if !enabled(row) {
                                return vec![];
                            }
                            self.choice = None;
                            return vec![ControlEvent::Select { id, index: row }];
                        }
                        Input::PointerDown { x, y } if !outer.contains(x, y) => {
                            self.choice = None;
                            return vec![];
                        }
                        _ => {}
                    }
                    self.choice = Some((id, selected));
                    return vec![];
                }
            }
            self.choice = None;
        }
        if let Input::RightClick { x, y } = input {
            return self
                .control_at(x, y)
                .and_then(|c| {
                    self.row_at(c, x, y).map(|index| ControlEvent::RightClick {
                        id: c.id.clone(),
                        index,
                    })
                })
                .into_iter()
                .collect();
        }
        let old_focus = self.tree.focus();
        let enter = matches!(
            input,
            Input::Key {
                key: crate::widgets::Key::Enter,
                ..
            }
        );
        let mut result = vec![];
        match &input {
            Input::PointerMove { x, y }
            | Input::PointerDown { x, y }
            | Input::PointerUp { x, y } => self.pointer = (*x, *y),
            _ => {}
        }
        if let Input::PointerDown { x, y } = input {
            if let Some(c) = self.control_at(x, y).cloned() {
                if let Some(e) = self.slider_event_at(&c, x, y) {
                    self.slider = Some(c.id.clone());
                    result.push(e);
                }
                if self.dragging.is_none() {
                    if let Some(index) = self.row_at(&c, x, y).or(c.slot.then_some(0)) {
                        self.dragging =
                            Some((c.id.clone(), DragPayload::Text(index.to_string()), (x, y)));
                    }
                }
                if let (
                    Some(slot),
                    ControlKind::Items { entries, .. } | ControlKind::ItemStrip { entries, .. },
                ) = (Self::item_slot(&c, x, y), &c.kind)
                {
                    if let Some(item) = entries
                        .get(slot)
                        .filter(|item| !item.disabled && item.id.0 != 0)
                    {
                        self.dragging = Some((c.id.clone(), DragPayload::Object(item.id), (x, y)));
                    }
                }
            }
        }
        if let Input::PointerMove { x, y } = input {
            if let Some((id, _, start)) = self.dragging.clone() {
                if (x - start.0).abs() + (y - start.1).abs() > 4 {
                    if let Some(c) = self.controls.iter().find(|c| c.id == id) {
                        if let Some(index) =
                            self.row_at(c, start.0, start.1).or(c.slot.then_some(0))
                        {
                            result.push(ControlEvent::DragStart { id, index });
                        }
                    }
                    self.dragging = None;
                    self.tree.handle(Input::Cancel);
                }
            }
        }
        if let Input::PointerMove { x, y } | Input::PointerUp { x, y } = input {
            if let Some(c) = self
                .slider
                .as_ref()
                .and_then(|id| self.controls.iter().find(|c| &c.id == id))
            {
                if let Some(e) = self.slider_event_at(c, x, y) {
                    result.push(e);
                }
            }
        }
        let released = matches!(input, Input::PointerUp { .. });
        let mut dropped = false;
        if released {
            self.slider = None;
            if let Some((_, payload, start)) = self.dragging.take() {
                let (x, y) = self.pointer;
                if (x - start.0).abs() + (y - start.1).abs() > 4 {
                    if let Some(c) = self.control_at(x, y) {
                        result.push(ControlEvent::Drop {
                            id: c.id.clone(),
                            payload,
                            slot: u32_from(Self::item_slot(c, x, y).unwrap_or(0)),
                        });
                        dropped = true;
                    }
                }
            }
        }
        if matches!(input, Input::Cancel) {
            self.dragging = None;
            self.slider = None;
        }
        let edge = match &input {
            Input::PointerDown { x, y } => self
                .control_at(*x, *y)
                .filter(|c| c.capture_edges && c.enabled)
                .map(|c| (c.id.clone(), true)),
            Input::PointerUp { .. } | Input::Cancel => self
                .tree
                .capture()
                .and_then(|n| {
                    self.controls
                        .iter()
                        .find(|c| self.ids[&c.id] == n && c.capture_edges)
                })
                .map(|c| (c.id.clone(), false)),
            _ => None,
        };
        if let Some((id, pressed)) = edge {
            result.push(ControlEvent::Held { id, pressed });
        }
        for action in self.tree.handle(input) {
            let (node, event) = match action {
                Action::Activate(node) => (node, 0),
                Action::Toggle { id, .. } => (id, 1),
                Action::Scroll { id, .. } => (id, 2),
                Action::Select { id, .. } => (id, 3),
                Action::Edit { id, .. } => (id, 4),
                Action::Focus(_) => continue,
            };
            let Some(c) = self.controls.iter().find(|c| self.ids[&c.id] == node) else {
                continue;
            };
            let id = c.id.clone();
            match (event, action) {
                (0, _) if !dropped => {
                    if let Some(index) = Self::item_slot(c, self.pointer.0, self.pointer.1) {
                        if let ControlKind::Items { entries, .. }
                        | ControlKind::ItemStrip { entries, .. } = &c.kind
                        {
                            if entries.get(index).is_none_or(|item| item.id.0 == 0) {
                                continue;
                            }
                        }
                        result.push(ControlEvent::Select {
                            id: id.clone(),
                            index,
                        });
                        self.double_click(&id, index, now, &mut result);
                    } else if let ControlKind::Choice { options, selected } = &c.kind {
                        if !options.is_empty() {
                            self.choice = Some((id, *selected));
                        }
                    } else if !matches!(
                        c.kind,
                        ControlKind::Slider { .. } | ControlKind::Edit { .. }
                    ) {
                        result.push(ControlEvent::Activate(id.clone()));
                        if c.slot {
                            self.double_click(&id, 0, now, &mut result);
                        }
                    }
                }
                (1, Action::Toggle { checked, .. }) => {
                    result.push(ControlEvent::Check { id, checked })
                }
                (2, Action::Scroll { value, .. }) => {
                    result.push(ControlEvent::Scroll { id, value })
                }
                (3, Action::Select { index, .. }) => {
                    result.push(ControlEvent::Select { id, index })
                }
                (4, Action::Edit { text, .. }) => result.push(ControlEvent::Edit { id, text }),
                _ => {}
            }
        }
        if let Some((node, x, y)) = edit_pointer {
            if self.edit_click.is_some_and(|(old, (px, py), time)| {
                old == node
                    && crate::clock::milliseconds(now, time) <= 500
                    && (px - x).abs() + (py - y).abs() <= 4
            }) {
                self.tree.select_edit_word(node, x, y);
                self.edit_click = None;
            } else {
                self.edit_click = Some((node, (x, y), now));
            }
        }
        // A second click on an already selected row does not change selection.
        if released && !dropped {
            if let Some(c) = self.control_at(self.pointer.0, self.pointer.1).cloned() {
                if let WidgetKind::List {
                    offset,
                    row_height,
                    content_height,
                    ..
                } = self.tree.node(self.ids[&c.id]).kind
                {
                    let y = self.pointer.1 - c.rect.y + offset;
                    if row_height > 0 && y >= 0 && y < content_height {
                        self.double_click(&c.id, (y / row_height) as usize, now, &mut result);
                    }
                }
            }
        }
        if old_focus != self.tree.focus() || enter {
            if let Some(c) = old_focus.and_then(|node| {
                self.controls
                    .iter()
                    .find(|c| self.ids[&c.id] == node && matches!(c.kind, ControlKind::Edit { .. }))
            }) {
                if old_focus != self.tree.focus()
                    || !matches!(
                        c.kind,
                        ControlKind::Edit {
                            multiline: true,
                            ..
                        }
                    )
                {
                    result.push(ControlEvent::Commit { id: c.id.clone() });
                    if enter
                        && !matches!(
                            c.kind,
                            ControlKind::Edit {
                                multiline: true,
                                ..
                            }
                        )
                    {
                        result.push(ControlEvent::Submit { id: c.id.clone() });
                    }
                }
            }
        }
        result
    }
    fn double_click(
        &mut self,
        id: &str,
        index: usize,
        now: dereth_primitives::LocalTime,
        out: &mut Vec<ControlEvent>,
    ) {
        if self.last_click.as_ref().is_some_and(|(old, row, t)| {
            old == id && *row == index && crate::clock::milliseconds(now, *t) < 500
        }) {
            out.push(ControlEvent::DoubleClick {
                id: id.into(),
                index,
            });
            self.last_click = None;
        } else {
            self.last_click = Some((id.into(), index, now));
        }
    }
    pub fn draw(&self, frame: &PanelFrame) -> Screen {
        let mut out = frame.clone();
        if let Some((r, accept)) = self.canvas_feedback {
            out.image_native(
                if accept { "060011F9" } else { "060011F8" },
                r.x + (r.w - 32) / 2,
                r.y + (r.h - 32) / 2,
                r,
                true,
            );
        }
        if let Some(r) = self.slot_hint {
            out.image_native(SLOT_HINT, r.x, r.y, r, true);
        }
        for c in &self.controls {
            if !c.paint {
                continue;
            }
            let node = self.ids[&c.id];
            let pressed = self.tree.pressed(node);
            let state = if !c.enabled {
                2
            } else if pressed {
                1
            } else {
                0
            };
            let color = if c.enabled { c.color } else { 0xff646464 };
            let r = c.rect;
            let clip = Some([r.x, r.y, r.x + r.w, r.y + r.h]);
            match &c.kind {
                ControlKind::ScrollBar {
                    min,
                    max,
                    value,
                    vertical,
                    arrow_size,
                    thumb_size,
                    ..
                } => {
                    // Two sets: the 16-pixel one (a 16x16 thumb) and the 20-pixel one (a 20x28
                    // thumb), each with a gradient track tiled along the bar.
                    // Horizontal bars have their own arrows and thumb (28x20 or 16x16) and a
                    // track that is the vertical one turned on its side.
                    let narrow = if *vertical { r.w <= 16 } else { r.h <= 16 };
                    let (up, down, track, thumb) = match (*vertical, narrow) {
                        (true, true) => ("06001261", "0600125E", "06001260", "06001263"),
                        (true, false) => ("0600125A", "06001257", "06001259", "0600125C"),
                        (false, true) => ("06001295", "06001298", "06001297", "0600129A"),
                        (false, false) => ("06001250", "06001253", "06001252", "06001255"),
                    };
                    out.image(track, r, true, false);
                    if *vertical {
                        out.image(up, rect(r.x, r.y, r.w, *arrow_size), false, false);
                        out.image(
                            down,
                            rect(r.x, r.y + r.h - arrow_size, r.w, *arrow_size),
                            false,
                            false,
                        );
                        if let Some(t) = self.tree.scrollbar_thumb(node) {
                            out.image(thumb, t, false, false);
                        }
                    } else {
                        let travel = (r.w - 2 * arrow_size + 1 - thumb_size).max(0);
                        let p = if max > min {
                            i32_from_i64(
                                i64::from(value - min) * i64::from(travel) / i64::from(max - min),
                            )
                        } else {
                            0
                        };
                        out.image(up, rect(r.x, r.y, *arrow_size, r.h), false, false);
                        out.image(
                            down,
                            rect(r.x + r.w - arrow_size, r.y, *arrow_size, r.h),
                            false,
                            false,
                        );
                        out.image(
                            thumb,
                            rect(r.x + arrow_size + p, r.y, *thumb_size, r.h),
                            false,
                            false,
                        );
                    }
                }
                ControlKind::Choice { options, selected } if c.list_skin.is_some() => {
                    let skin = c.list_skin.unwrap_or(crate::panels::ListSkin::OPTIONS);
                    let open = self.choice.as_ref().is_some_and(|(id, _)| *id == c.id);
                    let face = rect(r.x, r.y, r.w - skin.arrow_width, r.h);
                    out.image_native(skin.row, face.x, face.y, face, false);
                    skin_text(
                        &mut out,
                        face,
                        options.get(*selected).map_or("", String::as_str),
                        &c.font,
                        color,
                    );
                    let arrow = rect(face.x + face.w, r.y, skin.arrow_width, skin.arrow_height);
                    out.image_native(
                        if open { skin.arrow_pressed } else { skin.arrow },
                        arrow.x,
                        arrow.y,
                        arrow,
                        false,
                    );
                }
                ControlKind::Choice { options, selected } if c.choice_art => {
                    let open = self.choice.as_ref().is_some_and(|(id, _)| *id == c.id);
                    let face = rect(r.x, r.y, ART_FACE_WIDTH, ART_ROW_HEIGHT);
                    out.image_native(ART_ROW, r.x, r.y, face, false);
                    art_choice_text(
                        &mut out,
                        face,
                        options.get(*selected).map_or("", String::as_str),
                        &c.font,
                        color,
                    );
                    let arrow = rect(r.x + ART_FACE_WIDTH, r.y, ART_ARROW, ART_ARROW);
                    out.image_native(
                        if open { ART_ARROW_OPEN } else { ART_ARROW_SHUT },
                        arrow.x,
                        arrow.y,
                        arrow,
                        false,
                    );
                }
                ControlKind::Choice { options, selected } => {
                    out.fill(r, 0xff000000);
                    out.label(
                        r.x + 2,
                        r.y + 1,
                        options.get(*selected).map_or("", String::as_str),
                        &c.font,
                        color,
                        clip,
                    );
                }
                ControlKind::ItemStrip {
                    entries,
                    slot_size,
                    selected,
                    offset,
                } => {
                    for (i, item) in entries.iter().enumerate() {
                        let x = r.x + i32_from(i) * slot_size - offset;
                        if x + slot_size <= r.x || x >= r.x + r.w {
                            continue;
                        }
                        crate::item_art::paint_with_feedback(
                            &self.fonts,
                            &mut out,
                            item,
                            rect(x, r.y, *slot_size, *slot_size),
                            *selected == Some(item.id),
                            clip,
                            self.item_feedback(&c.id, i),
                        );
                    }
                }
                ControlKind::Button { caption } => {
                    // A theme button is its body (300 pixels wide, cut to the button's width
                    // less the cap) and its 32-pixel right cap, both at their own size. A picture
                    // button is one image at its own size, cut to the button.
                    const CAP: i32 = 32;
                    let bodies = ["06001207".into(), "06001208".into(), "0600120A".into()];
                    let caps = ["06001206".into(), "06001209".into(), "06001205".into()];
                    let themed = c.images.is_none();
                    let art = c.images.as_ref().unwrap_or(&bodies);
                    let cap = c.endcaps.as_ref().or(themed.then_some(&caps));
                    if let Some(cap) = cap {
                        let body = rect(r.x, r.y, (r.w - CAP).max(0), r.h);
                        out.image_native(&art[state], r.x, r.y, body, c.keyed);
                        out.image_native(&cap[state], r.x + r.w - CAP, r.y, r, c.keyed);
                    } else {
                        out.image_native(&art[state], r.x, r.y, r, c.keyed);
                    }
                    let lines = i32_from(caption.lines().count().max(1));
                    out.text_box(
                        rect(
                            r.x + i32::from(pressed),
                            r.y + (r.h - 16 * lines) / 2 + i32::from(pressed),
                            r.w,
                            16 * lines,
                        ),
                        caption,
                        &c.font,
                        color,
                        TextAlign::Center,
                        false,
                        clip,
                    );
                }
                ControlKind::Check { caption, checked } => {
                    let did = if *checked ^ pressed {
                        "0600128B"
                    } else {
                        "0600128D"
                    };
                    out.image(did, rect(r.x, r.y, 13, 13), false, true);
                    out.label(r.x + 19, r.y, caption, &c.font, color, clip);
                }
                ControlKind::Edit { .. } => {
                    if let Some(background) = c.background {
                        out.fill(r, background);
                    }
                    if let Some(images) = &c.images {
                        out.image(
                            &images[usize::from(self.tree.focus() == Some(node))],
                            r,
                            true,
                            false,
                        );
                    }
                    if let Some(layout) = self.tree.edit_layout(node) {
                        let viewport = self.tree.edit_viewport(node).unwrap();
                        let scroll = self.tree.edit_scroll(node).unwrap_or_default();
                        let origin = (r.x + viewport.x - scroll.0, r.y + viewport.y - scroll.1);
                        let text_clip =
                            rect(r.x + viewport.x, r.y + viewport.y, viewport.w, viewport.h)
                                .intersect(r)
                                .unwrap_or_default();
                        if self.tree.focus() == Some(node) {
                            if let Some((cursor, anchor)) = self.tree.edit_selection(node) {
                                for area in layout.selection(cursor, anchor) {
                                    if let Some(area) =
                                        rect(origin.0 + area.x, origin.1 + area.y, area.w, area.h)
                                            .intersect(text_clip)
                                    {
                                        out.fill(area, 0xff274657);
                                    }
                                }
                                let caret = layout.caret(cursor);
                                if self.tree.edit_caret_visible(node) {
                                    if let Some(area) =
                                        rect(origin.0 + caret.x, origin.1 + caret.y, 1, caret.h)
                                            .intersect(text_clip)
                                    {
                                        out.fill(area, color);
                                    }
                                }
                            }
                        }
                        for line in &layout.lines {
                            out.label(
                                origin.0 + line.x,
                                origin.1 + line.y,
                                &line.text,
                                &c.font,
                                color,
                                Some([
                                    text_clip.x,
                                    text_clip.y,
                                    text_clip.x + text_clip.w,
                                    text_clip.y + text_clip.h,
                                ]),
                            );
                        }
                    }
                }
                ControlKind::List {
                    rows, row_height, ..
                } => {
                    let WidgetKind::List {
                        offset, selected, ..
                    } = self.tree.node(node).kind
                    else {
                        unreachable!()
                    };
                    for (i, row) in rows.iter().enumerate() {
                        let y = r.y + i32_from(i) * row_height - offset;
                        if y + row_height <= r.y || y >= r.y + r.h {
                            continue;
                        }
                        if selected == Some(i) {
                            if let Some(b) = rect(r.x, y, r.w, *row_height).intersect(r) {
                                out.fill(b, 0xff274657);
                            }
                        }
                        let inset = if let Some(icon) = row.icon {
                            out.screen.commands.push(Command::Image {
                                did: format!("{:08X}", icon.0),
                                x: r.x,
                                y,
                                width: *row_height as u32,
                                height: *row_height as u32,
                                clip,
                                color_key: Some([0, 0, 0]),
                                key_bits: Some([5, 6, 5]),
                                tile: false,
                            });
                            *row_height + 2
                        } else {
                            2
                        };
                        out.label(r.x + inset, y, &row.text, &c.font, row.color, clip);
                    }
                }
                ControlKind::HitList { .. } => {}
                ControlKind::Slider {
                    min, max, value, ..
                } => {
                    let horizontal = r.w >= r.h;
                    let thumb = if Self::settings_slider(c) { 7 } else { 16 };
                    let travel = (if horizontal { r.w } else { r.h }
                        + i32::from(Self::settings_slider(c))
                        - thumb)
                        .max(0);
                    let p = if max > min {
                        i32_from_i64(
                            i64::from(value - min) * i64::from(travel) / i64::from(max - min),
                        )
                    } else {
                        0
                    };
                    if let Some(images) = &c.images {
                        out.image(&images[0], r, true, true);
                    } else {
                        out.fill(r, 0xff151515);
                    }
                    out.image(
                        c.images
                            .as_ref()
                            .map_or("06001263", |images| images[1].as_str()),
                        if horizontal {
                            rect(r.x + p, r.y, thumb, r.h)
                        } else {
                            rect(r.x, r.y + p, r.w, thumb)
                        },
                        false,
                        true,
                    );
                    if Self::settings_slider(c) {
                        if let Some(Command::Image {
                            clip: image_clip, ..
                        }) = out.screen.commands.last_mut()
                        {
                            *image_clip = clip;
                        }
                    }
                }
                ControlKind::Items {
                    entries,
                    columns,
                    slot_size,
                    selected,
                } => {
                    if *columns == 0 {
                        continue;
                    }
                    for (i, item) in entries.iter().enumerate() {
                        let cell = rect(
                            r.x + (u32_from(i) % columns) as i32 * slot_size,
                            r.y + (u32_from(i) / columns) as i32 * slot_size,
                            *slot_size,
                            *slot_size,
                        );
                        if cell.intersect(r).is_none() {
                            continue;
                        }
                        if c.drop_location.is_some() && item.id.0 == 0 {
                            if let Some(did) = self.item_feedback(&c.id, i) {
                                out.image(&format!("{did:08X}"), cell, false, true);
                            }
                        } else {
                            crate::item_art::paint_with_feedback(
                                &self.fonts,
                                &mut out,
                                item,
                                cell,
                                *selected == Some(item.id),
                                clip,
                                self.item_feedback(&c.id, i),
                            );
                        }
                    }
                }
            }
        }
        if let Some((id, selected)) = &self.choice {
            if let Some(c) = self.controls.iter().find(|c| &c.id == id) {
                if let ControlKind::Choice {
                    options,
                    selected: current,
                } = &c.kind
                {
                    let enabled = |i: usize| {
                        c.choice_enabled
                            .as_ref()
                            .is_none_or(|rows| rows.get(i).copied().unwrap_or(false))
                    };
                    let Popup {
                        outer,
                        rows: popup,
                        row_height,
                    } = self.choice_popup(c, options.len());
                    for (i, option) in options.iter().enumerate() {
                        let row = rect(
                            popup.x,
                            popup.y + row_height * i32_from(i),
                            popup.w,
                            row_height,
                        );
                        if c.chat_popup {
                            let art = if i == *selected {
                                CHAT_ROW_LIT
                            } else if i == *current {
                                CHAT_ROW_CURRENT
                            } else {
                                CHAT_ROW
                            };
                            out.image_native(art, row.x, row.y, row, false);
                            out.label(
                                row.x + 18,
                                row.y + 1,
                                option,
                                &c.font,
                                if enabled(i) { c.color } else { 0xff646464 },
                                None,
                            );
                        } else if let Some(skin) = c.list_skin {
                            out.image_native(
                                if i == *selected {
                                    skin.row_lit
                                } else {
                                    skin.row
                                },
                                row.x,
                                row.y,
                                row,
                                false,
                            );
                            skin_text(
                                &mut out,
                                row,
                                option,
                                &c.font,
                                if enabled(i) { c.color } else { 0xff646464 },
                            );
                        } else if c.choice_art {
                            out.image_native(
                                if i == *selected { ART_ROW_LIT } else { ART_ROW },
                                row.x,
                                row.y,
                                row,
                                false,
                            );
                            art_choice_text(
                                &mut out,
                                row,
                                option,
                                &c.font,
                                if enabled(i) { c.color } else { 0xff646464 },
                            );
                        } else {
                            out.fill(
                                row,
                                if i == *selected {
                                    0xff274657
                                } else {
                                    0xff000000
                                },
                            );
                            out.label(
                                row.x + 2,
                                row.y + 1,
                                option,
                                &c.font,
                                if enabled(i) { c.color } else { 0xff606060 },
                                None,
                            );
                        }
                    }
                    if let Some(skin) = c.list_skin {
                        if let Some((did, h)) = skin.top {
                            let top = rect(outer.x, outer.y, outer.w, h);
                            out.image_native(did, top.x, top.y, top, false);
                        }
                        if let Some((did, h)) = skin.bottom {
                            let bottom = rect(outer.x, popup.y + popup.h, outer.w, h);
                            out.image_native(did, bottom.x, bottom.y, bottom, false);
                        }
                    } else if c.choice_art {
                        let top = rect(outer.x, outer.y, outer.w, ART_EDGE);
                        out.image_native(ART_POPUP_TOP, top.x, top.y, top, false);
                        let bottom = rect(outer.x, popup.y + popup.h, outer.w, ART_EDGE);
                        out.image_native(ART_POPUP_BOTTOM, bottom.x, bottom.y, bottom, false);
                    }
                }
            }
        }
        out.screen
    }
}

/// A skinned list's text: one label at the left, or, for a row of two parts separated by a tab
/// (a book's "Page n" and its author), the second part from 80 pixels in.
fn skin_text(out: &mut PanelFrame, r: Rect, text: &str, font: &str, color: u32) {
    let clip = Some([r.x, r.y, r.x + r.w, r.y + r.h]);
    match text.split_once('\t') {
        Some((first, second)) => {
            out.label(r.x + 5, r.y + 1, first, font, color, clip);
            out.label(r.x + 80, r.y + 1, second, font, color, clip);
        }
        None => out.label(r.x + 5, r.y + 1, text, font, color, clip),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    /// Behaviour: classic.paper-doll.centered-drop-feedback
    #[test]
    fn equipment_canvas_hint_keeps_unchanged_state_and_clears_on_leave() {
        #[derive(Debug)]
        struct View(Option<bool>);
        impl dereth_client_contract::GameView for View {
            fn equipment_hover(&self, _: ObjectId) -> dereth_client_contract::view::EquipmentHover {
                dereth_client_contract::view::EquipmentHover {
                    slot_mask: None,
                    canvas: self.0,
                }
            }
        }
        let mut frame = PanelFrame::new(100, 200);
        frame
            .button("canvas", rect(0, 0, 80, 180), "", true)
            .drop_equipment_canvas = true;
        let mut host = ControlHost::default();
        host.sync(&frame);
        for accept in [true, false] {
            host.update_item_drop_preview(
                &View(Some(accept)),
                Some(ObjectId(3)),
                Some((10, 10)),
                true,
                0,
            );
            let expected = Some((rect(0, 0, 80, 180), accept));
            assert_eq!(host.canvas_feedback, expected);
            host.update_item_drop_preview(&View(None), Some(ObjectId(3)), Some((10, 10)), true, 0);
            assert_eq!(host.canvas_feedback, expected);
            let drawing = host.draw(&frame);
            assert_eq!(drawing.commands.iter().filter(|c| matches!(c, Command::Image { did, x: 24, y: 74, .. } if did == if accept { "060011F9" } else { "060011F8" })).count(), 1);
            assert!(!drawing.commands.iter().any(|c| matches!(
                c,
                Command::Fill {
                    color: 0xff00ff00 | 0xffff0000,
                    ..
                }
            )));
            host.update_item_drop_preview(&View(None), Some(ObjectId(3)), None, true, 0);
            assert_eq!(host.canvas_feedback, None);
        }
    }
    #[test]
    fn settings_slider_uses_seven_pixel_thumb_preserves_grab_and_sounds_on_release() {
        let mut frame = PanelFrame::new(150, 30);
        frame
            .slider("volume", rect(0, 0, 120, 12), 0, 100, 0, 1)
            .images = Some(["06001285", "06001286", "06001286"].map(String::from));
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert!(host
            .draw(&frame)
            .commands
            .iter()
            .any(|c| matches!(c,Command::Image{did,width:7,height:12,..} if did=="06001286")));
        assert!(host
            .handle(
                Input::PointerDown { x: 2, y: 5 },
                dereth_primitives::LocalTime(0.0)
            )
            .contains(&ControlEvent::Value {
                id: "volume".into(),
                value: 0
            }));
        assert!(host
            .handle(
                Input::PointerMove { x: 59, y: 5 },
                dereth_primitives::LocalTime(0.0)
            )
            .contains(&ControlEvent::Value {
                id: "volume".into(),
                value: 50
            }));
        assert!(host.take_sounds().is_empty());
        host.handle(
            Input::PointerUp { x: 59, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(host.take_sounds(), vec![0x74]);
        host.handle(
            Input::PointerDown { x: 60, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host
            .handle(
                Input::PointerUp { x: 60, y: 5 },
                dereth_primitives::LocalTime(0.0)
            )
            .contains(&ControlEvent::Value {
                id: "volume".into(),
                value: 50
            }));
    }
    #[test]
    fn silent_external_controls_keep_callbacks_without_classic_sounds() {
        let mut frame = PanelFrame::new(100, 160);
        frame
            .button("print", rect(0, 0, 50, 20), "Print", true)
            .silent = true;
        frame
            .control(
                "scroll",
                rect(60, 0, 16, 150),
                ControlKind::ScrollBar {
                    min: 0,
                    max: 100,
                    value: 0,
                    page: 20,
                    step: 1,
                    vertical: true,
                    arrow_size: 16,
                    thumb_size: 16,
                },
                true,
            )
            .silent = true;
        let mut host = ControlHost::default();
        host.sync(&frame);
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host
            .handle(
                Input::PointerUp { x: 5, y: 5 },
                dereth_primitives::LocalTime(0.0)
            )
            .contains(&ControlEvent::Activate("print".into())));
        assert!(host
            .handle(
                Input::Key {
                    key: crate::widgets::Key::Enter,
                    shift: false
                },
                dereth_primitives::LocalTime(0.0)
            )
            .contains(&ControlEvent::Activate("print".into())));
        host.handle(
            Input::PointerDown { x: 65, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 65, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerDown { x: 65, y: 20 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 90, y: 155 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
    }
    #[test]
    fn button_sound_follows_pressed_edges_and_keyboard_activation() {
        let mut frame = PanelFrame::new(100, 100);
        frame.button("button", rect(0, 0, 40, 20), "Test", true);
        frame.button("disabled", rect(0, 30, 40, 20), "Disabled", false);
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert!(host.take_sounds().is_empty());
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(host.take_sounds(), vec![0x72]);
        host.handle(
            Input::PointerMove { x: 6, y: 6 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerMove { x: 60, y: 6 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
        host.handle(
            Input::PointerMove { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(host.take_sounds(), vec![0x72]);
        host.handle(
            Input::PointerUp { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
        host.handle(
            Input::Key {
                key: crate::widgets::Key::Enter,
                shift: false,
            },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(host.take_sounds(), vec![0x72]);
        host.handle(
            Input::PointerDown { x: 5, y: 35 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 5, y: 35 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
    }
    #[test]
    fn checkbox_sound_uses_alternate_state_not_each_toggle() {
        let mut frame = PanelFrame::new(100, 100);
        frame.check("check", rect(0, 0, 40, 20), "Test", false, true);
        let mut host = ControlHost::default();
        host.sync(&frame);
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(host.take_sounds(), vec![0x72]);
        host.handle(
            Input::PointerUp { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
    }
    #[test]
    fn scrollbar_arrows_press_and_thumb_releases_have_distinct_sounds() {
        let mut frame = PanelFrame::new(100, 160);
        frame.control(
            "scroll",
            rect(0, 0, 16, 150),
            ControlKind::ScrollBar {
                min: 0,
                max: 100,
                value: 0,
                page: 20,
                step: 1,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        let mut host = ControlHost::default();
        host.sync(&frame);
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(host.take_sounds(), vec![0x72]);
        host.handle(
            Input::PointerUp { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
        host.handle(
            Input::PointerDown { x: 5, y: 20 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
        host.handle(
            Input::PointerUp { x: 80, y: 155 },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(host.take_sounds(), vec![0x74]);
        host.handle(
            Input::PointerDown { x: 5, y: 90 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 5, y: 90 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.take_sounds().is_empty());
    }
    #[test]
    fn a_slot_button_can_be_dragged_from_and_double_clicked() {
        let mut frame = PanelFrame::new(200, 100);
        frame.button("spell:0", rect(10, 10, 32, 32), "", true).slot = true;
        let mut host = ControlHost::default();
        host.sync(&frame);
        host.handle(
            Input::PointerDown { x: 20, y: 20 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host
            .handle(
                Input::PointerMove { x: 30, y: 20 },
                dereth_primitives::LocalTime(0.0)
            )
            .contains(&ControlEvent::DragStart {
                id: "spell:0".into(),
                index: 0
            }));
        host.handle(
            Input::PointerUp { x: 30, y: 20 },
            dereth_primitives::LocalTime(0.0),
        );
        let mut events = vec![];
        for _ in 0..2 {
            host.handle(
                Input::PointerDown { x: 20, y: 20 },
                dereth_primitives::LocalTime(0.0),
            );
            events.extend(host.handle(
                Input::PointerUp { x: 20, y: 20 },
                dereth_primitives::LocalTime(0.0),
            ));
        }
        assert!(events.contains(&ControlEvent::DoubleClick {
            id: "spell:0".into(),
            index: 0
        }));
    }
    #[test]
    fn supplied_click_times_keep_list_and_edit_millisecond_boundaries() {
        use dereth_primitives::LocalTime;
        for (elapsed, list_double, edit_double) in [
            (0.499, true, true),
            (0.4999, true, true),
            (0.5, false, true),
            (0.5009, false, true),
            (0.501, false, false),
            (0.5011, false, false),
            (-1.0, true, true),
        ] {
            let mut frame = PanelFrame::new(200, 100);
            frame.list("rows", rect(0, 0, 100, 40), vec!["row".into()], None, 20);
            let mut host = ControlHost::default();
            host.sync(&frame);
            let mut second = Vec::new();
            for time in [10.0, 10.0 + elapsed] {
                host.handle(Input::PointerDown { x: 5, y: 5 }, LocalTime(time));
                second = host.handle(Input::PointerUp { x: 5, y: 5 }, LocalTime(time));
            }
            assert_eq!(
                second
                    .iter()
                    .any(|e| matches!(e, ControlEvent::DoubleClick { .. })),
                list_double,
                "list elapsed={elapsed}"
            );

            let mut frame = PanelFrame::new(200, 100);
            frame.edit("body", rect(0, 0, 180, 30), "alpha beta", 100, false, true);
            let mut host = ControlHost::default();
            host.sync(&frame);
            for time in [10.0, 10.0 + elapsed] {
                host.handle(Input::PointerDown { x: 12, y: 10 }, LocalTime(time));
                host.handle(Input::PointerUp { x: 12, y: 10 }, LocalTime(time));
            }
            assert_eq!(
                host.selected_text().as_deref(),
                edit_double.then_some("alpha"),
                "edit elapsed={elapsed}"
            );
        }
    }
    #[test]
    fn a_choice_list_taller_than_the_space_above_and_below_stays_on_screen() {
        let mut frame = PanelFrame::new(800, 600);
        let options: Vec<String> = (0..20).map(|i| format!("{i}")).collect();
        frame.control(
            "resolution",
            rect(130, 300, 120, 18),
            ControlKind::Choice {
                options,
                selected: 0,
            },
            true,
        );
        let mut host = ControlHost::default();
        host.sync(&frame);
        let popup = host.choice_popup(&frame.controls[0], 20);
        assert_eq!((popup.outer.y, popup.outer.h), (240, 360));
        // The last row is reachable.
        host.handle(
            Input::PointerDown { x: 135, y: 305 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 135, y: 305 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.popup_open());
        assert_eq!(
            host.handle(
                Input::PointerUp { x: 135, y: 590 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Select {
                id: "resolution".into(),
                index: 19
            }]
        );
    }
    #[test]
    fn disabled_choice_rows_reject_pointer_and_keyboard_selection() {
        let mut frame = PanelFrame::new(200, 150);
        frame
            .control(
                "destination",
                rect(0, 0, 100, 20),
                ControlKind::Choice {
                    options: vec!["Chat".into(), "Unavailable".into(), "Fellowship".into()],
                    selected: 0,
                },
                true,
            )
            .choice_enabled = Some(vec![true, false, true]);
        let mut host = ControlHost::default();
        host.sync(&frame);
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host
            .handle(
                Input::PointerUp { x: 5, y: 45 },
                dereth_primitives::LocalTime(0.0)
            )
            .is_empty());
        assert!(host.popup_open());
        host.handle(
            Input::Key {
                key: crate::widgets::Key::Down,
                shift: false,
            },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(
            host.handle(
                Input::Key {
                    key: crate::widgets::Key::Enter,
                    shift: false
                },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Select {
                id: "destination".into(),
                index: 2
            }]
        );
    }
    #[test]
    fn choice_arrow_keys_wrap_past_either_end_and_skip_disabled_rows() {
        let all = |_: usize| true;
        assert_eq!(cycle_choice(2, 3, true, all), 0);
        assert_eq!(cycle_choice(0, 3, false, all), 2);
        assert_eq!(cycle_choice(1, 3, true, all), 2);
        let some = |i: usize| i != 0;
        assert_eq!(cycle_choice(2, 3, true, some), 1);
        assert_eq!(cycle_choice(1, 3, false, some), 2);
        assert_eq!(cycle_choice(1, 3, true, |i: usize| i == 1), 1);
        assert_eq!(cycle_choice(0, 0, true, all), 0);
    }
    #[test]
    fn art_choice_draws_row_art_and_arrow_and_escape_closes_its_popup() {
        let mut frame = PanelFrame::new(800, 600);
        let c = frame.control(
            "style",
            rect(368, 221, 143, 26),
            ControlKind::Choice {
                options: vec!["Cap".into(), "Hood".into(), "Hat".into()],
                selected: 2,
            },
            true,
        );
        c.choice_art = true;
        let mut host = ControlHost::default();
        host.sync(&frame);
        let images = |host: &ControlHost| -> Vec<(String, i32, i32)> {
            host.draw(&frame)
                .commands
                .iter()
                .filter_map(|c| match c {
                    Command::Image { did, x, y, .. } => Some((did.clone(), *x, *y)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            images(&host),
            [("0600050A".into(), 368, 221), ("060004EA".into(), 485, 221)]
        );
        host.handle(
            Input::PointerDown { x: 490, y: 230 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 490, y: 230 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.popup_open());
        let open = images(&host);
        assert!(open.contains(&("060004EB".into(), 485, 221)));
        // Top edge, three rows (the current one lit), bottom edge, below the 25-pixel face.
        assert!(open.ends_with(&[
            ("0600050A".into(), 368, 254),
            ("0600050A".into(), 368, 279),
            ("06000509".into(), 368, 304),
            ("06001203".into(), 368, 246),
            ("060011F2".into(), 368, 329),
        ]));
        // Down from the last row wraps to the first.
        host.handle(
            Input::Key {
                key: crate::widgets::Key::Down,
                shift: false,
            },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(
            host.handle(
                Input::Key {
                    key: crate::widgets::Key::Enter,
                    shift: false
                },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Select {
                id: "style".into(),
                index: 0
            }]
        );
        host.handle(
            Input::PointerDown { x: 490, y: 230 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 490, y: 230 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.popup_open());
        // A click on the popup's top edge picks nothing; the second row is 25 pixels down.
        assert!(host
            .handle(
                Input::PointerUp { x: 400, y: 250 },
                dereth_primitives::LocalTime(0.0)
            )
            .is_empty());
        assert_eq!(
            host.handle(
                Input::PointerUp { x: 400, y: 280 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Select {
                id: "style".into(),
                index: 1
            }]
        );
        host.handle(
            Input::PointerDown { x: 490, y: 230 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 490, y: 230 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host
            .handle(
                Input::Key {
                    key: crate::widgets::Key::Escape,
                    shift: false
                },
                dereth_primitives::LocalTime(0.0)
            )
            .is_empty());
        assert!(!host.popup_open());
    }
    #[test]
    fn bottom_edge_choice_opens_upward_and_captures_outside_clicks() {
        let mut frame = PanelFrame::new(200, 100);
        frame.control(
            "destination",
            rect(0, 80, 100, 20),
            ControlKind::Choice {
                options: vec!["Chat".into(), "Tell".into(), "Fellowship".into()],
                selected: 0,
            },
            true,
        );
        let mut host = ControlHost::default();
        host.sync(&frame);
        host.handle(
            Input::PointerDown { x: 5, y: 85 },
            dereth_primitives::LocalTime(0.0),
        );
        host.handle(
            Input::PointerUp { x: 5, y: 85 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host.popup_open());
        assert!(host.contains_control(180, 5));
        assert_eq!(
            host.handle(
                Input::PointerUp { x: 5, y: 45 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Select {
                id: "destination".into(),
                index: 1,
            }]
        );
        assert!(!host.popup_open());
        assert!(!host.contains_control(180, 5));
    }
    #[test]
    fn combat_button_releases_capture_outside_without_click_activation() {
        let mut frame = PanelFrame::new(100, 100);
        frame
            .button("attack", rect(10, 10, 30, 30), "", true)
            .capture_edges = true;
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert_eq!(
            host.handle(
                Input::PointerDown { x: 20, y: 20 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Held {
                id: "attack".into(),
                pressed: true
            }]
        );
        assert_eq!(
            host.handle(
                Input::PointerUp { x: 80, y: 80 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Held {
                id: "attack".into(),
                pressed: false
            }]
        );
    }
    #[test]
    fn pane_deactivation_commits_edit_once_and_releases_held_capture() {
        let mut frame = PanelFrame::new(200, 100);
        frame.edit("body", rect(0, 0, 100, 20), "draft", 100, true, true);
        frame
            .button("held", rect(0, 40, 100, 20), "Hold", true)
            .capture_edges = true;
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert!(host.focus_control("body"));
        assert_eq!(
            host.deactivate(dereth_primitives::LocalTime(0.0)),
            [ControlEvent::Commit { id: "body".into() }]
        );
        assert!(!host.editing());
        assert!(host
            .deactivate(dereth_primitives::LocalTime(0.0))
            .is_empty());
        host.handle(
            Input::PointerDown { x: 5, y: 45 },
            dereth_primitives::LocalTime(0.0),
        );
        assert_eq!(
            host.deactivate(dereth_primitives::LocalTime(0.0)),
            [ControlEvent::Held {
                id: "held".into(),
                pressed: false
            }]
        );
        assert!(host
            .handle(
                Input::PointerUp { x: 5, y: 45 },
                dereth_primitives::LocalTime(0.0)
            )
            .is_empty());
    }
    #[test]
    fn chat_submission_is_enter_only_and_blur_commits_without_sending() {
        let mut frame = PanelFrame::new(200, 100);
        frame.edit("chat", rect(0, 0, 100, 20), "hello", 100, false, true);
        frame.button("other", rect(0, 40, 100, 20), "Other", true);
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert!(host.focus_control("chat"));
        let events = host.handle(
            Input::Key {
                key: crate::widgets::Key::Enter,
                shift: false,
            },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(events.contains(&ControlEvent::Submit { id: "chat".into() }));
        let blur = host.handle(
            Input::PointerDown { x: 5, y: 45 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(blur.contains(&ControlEvent::Commit { id: "chat".into() }));
        assert!(!blur
            .iter()
            .any(|e| matches!(e, ControlEvent::Submit { .. })));
    }
    #[test]
    fn refreshing_game_data_during_a_press_keeps_capture_and_release_behavior() {
        let mut frame = PanelFrame::new(100, 100);
        frame.button("use", rect(10, 10, 40, 20), "Use", true);
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert!(host
            .handle(
                Input::PointerDown { x: 20, y: 20 },
                dereth_primitives::LocalTime(0.0)
            )
            .is_empty());
        host.sync(&frame);
        assert_eq!(
            host.handle(
                Input::PointerUp { x: 20, y: 20 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Activate("use".into())]
        );
        host.handle(
            Input::PointerDown { x: 20, y: 20 },
            dereth_primitives::LocalTime(0.0),
        );
        frame.controls[0].enabled = false;
        host.sync(&frame);
        assert!(host
            .handle(
                Input::PointerUp { x: 20, y: 20 },
                dereth_primitives::LocalTime(0.0)
            )
            .is_empty());
    }
    #[test]
    fn second_click_on_selected_row_emits_double_click_and_refresh_keeps_scroll() {
        let mut frame = PanelFrame::new(100, 100);
        frame.list(
            "rows",
            rect(0, 0, 100, 40),
            (0..10).map(|i| i.to_string().into()).collect(),
            None,
            20,
        );
        let mut host = ControlHost::default();
        host.sync(&frame);
        host.handle(
            Input::Wheel {
                x: 5,
                y: 5,
                delta: 2,
            },
            dereth_primitives::LocalTime(0.0),
        );
        host.sync(&frame);
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        let first = host.handle(
            Input::PointerUp { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(first
            .iter()
            .any(|e| matches!(e, ControlEvent::Select { index: 2, .. })));
        host.handle(
            Input::PointerDown { x: 5, y: 5 },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(host
            .handle(
                Input::PointerUp { x: 5, y: 5 },
                dereth_primitives::LocalTime(0.0)
            )
            .iter()
            .any(|e| matches!(e, ControlEvent::DoubleClick { index: 2, .. })));
    }
    #[test]
    fn the_wheel_anywhere_over_a_window_with_one_scroll_bar_scrolls_it() {
        let scroll = |value| ControlKind::ScrollBar {
            min: 0,
            max: 200,
            value,
            page: 100,
            step: 20,
            vertical: true,
            arrow_size: 16,
            thumb_size: 16,
        };
        let mut frame = PanelFrame::new(300, 300);
        frame.check("row", rect(14, 30, 13, 13), "", false, true);
        frame.control("scroll", rect(284, 16, 16, 264), scroll(0), true);
        let mut host = ControlHost::default();
        host.sync(&frame);
        // Over a row of the page, not the bar: two notches down move the bar two steps.
        let events = host.handle(
            Input::Wheel {
                x: 100,
                y: 100,
                delta: 2,
            },
            dereth_primitives::LocalTime(0.0),
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ControlEvent::Scroll { id, value: 40 } if id == "scroll")),
            "{events:?}"
        );
        // With two bars there is no telling which one the wheel means: it scrolls neither.
        frame.control("other", rect(0, 16, 16, 264), scroll(0), true);
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert!(!host
            .handle(
                Input::Wheel {
                    x: 100,
                    y: 100,
                    delta: 2,
                },
                dereth_primitives::LocalTime(0.0)
            )
            .iter()
            .any(|e| matches!(e, ControlEvent::Scroll { .. })));
    }
    #[test]
    fn a_horizontal_scroll_bars_arrows_step_by_one_cell() {
        let mut frame = PanelFrame::new(200, 40);
        frame.control(
            "strip",
            rect(0, 0, 128, 20),
            ControlKind::ScrollBar {
                min: 0,
                max: 96,
                value: 32,
                page: 128,
                step: 32,
                vertical: false,
                arrow_size: 20,
                thumb_size: 28,
            },
            true,
        );
        let c = &frame.controls[0];
        let value = |x| match ControlHost::slider_event(c, x, 10) {
            Some(ControlEvent::Scroll { value, .. }) => value,
            other => panic!("{other:?}"),
        };
        assert_eq!(value(5), 0);
        assert_eq!(value(120), 64);
    }
    #[test]
    fn horizontal_slider_drag_clamps_even_when_pointer_leaves_track() {
        let mut frame = PanelFrame::new(120, 50);
        frame.slider("level", rect(10, 10, 101, 16), 0, 100, 50, 5);
        let mut host = ControlHost::default();
        host.sync(&frame);
        assert_eq!(
            host.handle(
                Input::PointerDown { x: 60, y: 15 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Value {
                id: "level".into(),
                value: 50
            }]
        );
        assert_eq!(
            host.handle(
                Input::PointerMove { x: 250, y: 80 },
                dereth_primitives::LocalTime(0.0)
            ),
            vec![ControlEvent::Value {
                id: "level".into(),
                value: 100
            }]
        );
    }
}
