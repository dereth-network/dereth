//! The classic interface's widget tree. No platform or renderer dependencies.
//!
//! It keeps the classic interface's rules: local window trees, inclusive pixel bounds,
//! inherited clips, newest-child hit priority, focus traversal, buttons and checkboxes that act
//! on a release inside them, and clamped scrolling. Single-owner capture stands in for the
//! classic interface's temporary event subscriptions.
//! Wheel units and non-BMP editing remain host adapters.

use crate::int::i32_from_i64;
pub type NodeId = usize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        self.w > 0
            && self.h > 0
            && i64::from(x) >= i64::from(self.x)
            && i64::from(y) >= i64::from(self.y)
            && i64::from(x) < i64::from(self.x) + i64::from(self.w)
            && i64::from(y) < i64::from(self.y) + i64::from(self.h)
    }

    pub fn intersect(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (i64::from(self.x) + i64::from(self.w.max(0)))
            .min(i64::from(other.x) + i64::from(other.w.max(0)));
        let bottom = (i64::from(self.y) + i64::from(self.h.max(0)))
            .min(i64::from(other.y) + i64::from(other.h.max(0)));
        (right > i64::from(x) && bottom > i64::from(y)).then_some(Self {
            x,
            y,
            w: i32_from_i64((right - i64::from(x)).clamp(0, i64::from(i32::MAX))),
            h: i32_from_i64((bottom - i64::from(y)).clamp(0, i64::from(i32::MAX))),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    Panel,
    Button,
    Checkbox {
        checked: bool,
    },
    Text,
    Edit {
        text: String,
        max_chars: usize,
    },
    /// Children move by -offset and remain clipped to this viewport.
    List {
        content_height: i32,
        offset: i32,
        row_height: i32,
        selected: Option<usize>,
    },
    /// Vertical range. `page` is a content page step; `step` controls keyboard/wheel.
    /// Art geometry is configured separately; retail thumbs have fixed image sizes.
    Scrollbar {
        min: i32,
        max: i32,
        value: i32,
        page: i32,
        step: i32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub rect: Rect,
    pub visible: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub kind: WidgetKind,
}

impl Node {
    pub fn new(kind: WidgetKind, rect: Rect) -> Self {
        let focusable = matches!(
            kind,
            WidgetKind::Button
                | WidgetKind::Checkbox { .. }
                | WidgetKind::Edit { .. }
                | WidgetKind::Scrollbar { .. }
        );
        Self {
            parent: None,
            rect,
            visible: true,
            enabled: true,
            focusable,
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Other,
    Escape,
    Left,
    Right,
    Delete,
    SelectAll,
    WordLeft,
    WordRight,
    PageUp,
    PageDown,
    Tab,
    Enter,
    Space,
    Backspace,
    Up,
    Down,
    Home,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    RightClick {
        x: i32,
        y: i32,
    },
    PointerMove {
        x: i32,
        y: i32,
    },
    PointerDown {
        x: i32,
        y: i32,
    },
    PointerUp {
        x: i32,
        y: i32,
    },
    /// Positive delta scrolls downward, in logical steps (host converts wheel units).
    Wheel {
        x: i32,
        y: i32,
        delta: i32,
    },
    Key {
        key: Key,
        shift: bool,
    },
    Text(String),
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Activate(NodeId),
    Toggle { id: NodeId, checked: bool },
    Focus(Option<NodeId>),
    Scroll { id: NodeId, value: i32 },
    Select { id: NodeId, index: usize },
    Edit { id: NodeId, text: String },
}

#[derive(Debug)]
struct EditLayout {
    layout: crate::text_edit::Layout,
    viewport: Rect,
    scroll: (i32, i32),
    desired_x: Option<i32>,
    blink: crate::text_edit::CaretBlink,
    smooth: Option<crate::text_edit::SmoothScroll>,
}

#[derive(Default, Debug)]
pub struct UiTree {
    edit_layouts: std::collections::BTreeMap<NodeId, EditLayout>,
    time: f64,
    edit_drag_origin: Option<(NodeId, i32, i32)>,
    edit_smooth: std::collections::BTreeSet<NodeId>,
    edits: std::collections::BTreeMap<NodeId, (usize, usize)>,
    multiline: std::collections::BTreeSet<NodeId>,
    select_on_focus: std::collections::BTreeSet<NodeId>,
    nodes: Vec<Node>,
    focus: Option<NodeId>,
    capture: Option<NodeId>,
    pointer: (i32, i32),
    active_root: Option<NodeId>,
    scrollbar_geometry: std::collections::BTreeMap<NodeId, (i32, i32)>,
    scrollbar_drag: Option<(NodeId, i32)>,
    scrollbar_repeat: Option<(NodeId, i32, f64)>,
    scrollbar_track: Option<(NodeId, i32, f64)>,
    scrollbar_flags: std::collections::BTreeMap<NodeId, u32>,
}

impl UiTree {
    /// Viewport is relative to the edit node. Reinstall after text/font/size changes.
    /// An unchanged layout preserves manual page scrolling.
    pub fn set_edit_layout(
        &mut self,
        id: NodeId,
        layout: crate::text_edit::Layout,
        viewport: Rect,
    ) {
        let cursor = self.edit_selection(id).map_or(0, |v| v.0);
        let previous = self.edit_layouts.remove(&id);
        let changed = previous
            .as_ref()
            .is_none_or(|old| old.viewport != viewport || old.layout != layout);
        let scroll = previous.as_ref().map_or((0, 0), |old| old.scroll);
        let scroll = if changed {
            layout.ensure_visible(cursor, viewport.w, viewport.h, scroll)
        } else {
            layout.clamp_scroll(viewport.w, viewport.h, scroll)
        };
        self.edit_layouts.insert(
            id,
            EditLayout {
                layout,
                viewport,
                scroll,
                desired_x: previous.as_ref().and_then(|old| old.desired_x),
                blink: previous.as_ref().map_or_else(
                    || crate::text_edit::CaretBlink::new(self.time),
                    |old| old.blink.clone(),
                ),
                smooth: previous.and_then(|old| old.smooth),
            },
        );
    }
    /// Double-click selection is confined to the clicked visual line.
    /// Coordinates are absolute, like Input::PointerDown.
    pub fn select_edit_word(&mut self, id: NodeId, x: i32, y: i32) {
        let Some(rect) = self.absolute_rect(id) else {
            return;
        };
        let Some(state) = self.edit_layouts.get(&id) else {
            return;
        };
        let local_x = x - rect.x - state.viewport.x + state.scroll.0;
        let local_y = y - rect.y - state.viewport.y + state.scroll.1;
        let (start, end) = state.layout.word_at(local_x, local_y);
        self.edits.insert(id, (end, start));
        self.capture = None;
        self.edit_drag_origin = None;
    }
    pub fn edit_layout(&self, id: NodeId) -> Option<&crate::text_edit::Layout> {
        self.edit_layouts.get(&id).map(|state| &state.layout)
    }
    pub fn edit_scroll(&self, id: NodeId) -> Option<(i32, i32)> {
        self.edit_layouts.get(&id).map(|state| state.scroll)
    }
    pub fn edit_viewport(&self, id: NodeId) -> Option<Rect> {
        self.edit_layouts.get(&id).map(|state| state.viewport)
    }
    fn reveal_edit_caret(&mut self, id: NodeId) {
        let Some((cursor, _)) = self.edit_selection(id) else {
            return;
        };
        if let Some(state) = self.edit_layouts.get_mut(&id) {
            state.smooth = None;
            state.scroll = state.layout.ensure_visible(
                cursor,
                state.viewport.w,
                state.viewport.h,
                state.scroll,
            );
        }
    }
    fn pointer_edit(&mut self, id: NodeId, x: i32, y: i32, extend: bool) {
        let Some(rect) = self.absolute_rect(id) else {
            return;
        };
        let Some(state) = self.edit_layouts.get(&id) else {
            return;
        };
        let viewport = Rect {
            x: rect.x + state.viewport.x,
            y: rect.y + state.viewport.y,
            ..state.viewport
        };
        let px = x.clamp(viewport.x, viewport.x + viewport.w.max(1) - 1);
        let py = y.clamp(viewport.y, viewport.y + viewport.h.max(1) - 1);
        let local_x = px - viewport.x + state.scroll.0;
        let local_y = py - viewport.y + state.scroll.1;
        let index = if extend {
            state.layout.hit_character(local_x, local_y)
        } else {
            state.layout.hit(local_x, local_y)
        };
        if extend {
            if let Some((origin, ox, oy)) = self.edit_drag_origin {
                if origin == id && (px - ox).abs() < 4 && (py - oy).abs() < 4 {
                    return;
                }
            }
            if let Some((_, anchor)) = self.edit_selection(id) {
                self.edits.insert(id, (index, anchor));
            }
        } else {
            self.place_caret(id, index, false);
            self.edit_drag_origin = Some((id, px, py));
        }
        if extend {
            let state = &self.edit_layouts[&id];
            let mut target = state.scroll;
            let distance = if y < viewport.y && state.scroll.1 > 0 {
                target.1 = 0;
                viewport.y - y
            } else if y >= viewport.y + viewport.h {
                target.1 = i32::MAX;
                y - (viewport.y + viewport.h - 1)
            } else if x < viewport.x && state.scroll.0 > 0 {
                target.0 = 0;
                viewport.x - x
            } else if x >= viewport.x + viewport.w {
                target.0 = i32::MAX;
                x - (viewport.x + viewport.w - 1)
            } else {
                0
            };
            if distance > 0 {
                let smooth = self.edit_smooth.contains(&id);
                let state = self.edit_layouts.get_mut(&id).unwrap();
                let target = state
                    .layout
                    .clamp_scroll(state.viewport.w, state.viewport.h, target);
                let speed = distance.saturating_mul(10).min(800);
                if let Some(animation) = &mut state.smooth {
                    animation.retarget(target, speed);
                } else if smooth {
                    state.smooth = Some(crate::text_edit::SmoothScroll::new(
                        state.scroll,
                        target,
                        self.time,
                        speed,
                    ));
                } else {
                    state.scroll = target;
                }
            } else if let Some(state) = self.edit_layouts.get_mut(&id) {
                state.smooth = None;
            }
        }
    }
    /// Seconds on a monotonic host clock. Call before input and on each frame.
    pub fn tick(&mut self, now: f64) -> Vec<Action> {
        if !now.is_finite() {
            return Vec::new();
        }
        self.time = now;
        let mut actions = Vec::new();
        self.sanitize(&mut actions);
        for (&id, state) in &mut self.edit_layouts {
            if self.focus == Some(id) {
                state.blink.tick(now);
            }
            if let Some(smooth) = &mut state.smooth {
                state.scroll =
                    state
                        .layout
                        .clamp_scroll(state.viewport.w, state.viewport.h, smooth.tick(now));
                if smooth.finished() {
                    state.smooth = None;
                }
            }
        }
        if let Some((id, direction, previous)) = self.scrollbar_repeat {
            if self.capture == Some(id) && self.available(id) {
                if let Some(rect) = self.absolute_rect(id) {
                    let arrow = self
                        .scrollbar_geometry
                        .get(&id)
                        .copied()
                        .unwrap_or((16, 16))
                        .0
                        .min(rect.h / 2);
                    let zone = Rect {
                        y: if direction < 0 {
                            rect.y
                        } else {
                            rect.y + rect.h - arrow
                        },
                        h: arrow,
                        ..rect
                    };
                    if zone.contains(self.pointer.0, self.pointer.1) && now - previous >= 0.1 {
                        self.scroll_delta(id, direction, &mut actions);
                        self.scrollbar_repeat = Some((id, direction, now));
                    }
                }
            } else {
                self.scrollbar_repeat = None;
            }
        }
        if let Some((id, direction, previous)) = self.scrollbar_track {
            if self.capture == Some(id) && self.available(id) {
                if let (Some(rect), Some(thumb)) =
                    (self.absolute_rect(id), self.scrollbar_thumb(id))
                {
                    if rect.contains(self.pointer.0, self.pointer.1) && now - previous >= 0.25 {
                        let current_direction = if self.pointer.1 < thumb.y + thumb.h / 2 {
                            -1
                        } else {
                            1
                        };
                        if current_direction == direction {
                            self.page_scrollbar(id, direction, &mut actions);
                        }
                        self.scrollbar_track = Some((id, direction, now));
                    }
                }
            } else {
                self.scrollbar_track = None;
            }
        }
        if let Some(id) = self.capture.filter(|id| self.edit_layouts.contains_key(id)) {
            self.pointer_edit(id, self.pointer.0, self.pointer.1, true);
        }
        actions
    }
    pub fn set_edit_smooth_scroll(&mut self, id: NodeId, enabled: bool) {
        if enabled {
            self.edit_smooth.insert(id);
        } else {
            self.edit_smooth.remove(&id);
        }
    }
    pub fn edit_caret_visible(&self, id: NodeId) -> bool {
        self.focus == Some(id)
            && self.edit_selection(id).is_some_and(|(a, b)| a == b)
            && self
                .edit_layouts
                .get(&id)
                .is_none_or(|state| state.blink.visible)
    }
    fn edit_activity(&mut self, id: NodeId) {
        if let Some(state) = self.edit_layouts.get_mut(&id) {
            state.blink.activity(self.time);
        }
    }
    /// Scroll bar flag 8 gives direct track positioning instead of repeated page steps.
    pub fn set_scrollbar_flags(&mut self, id: NodeId, flags: u32) {
        self.scrollbar_flags.insert(id, flags);
    }
    fn page_scrollbar(&mut self, id: NodeId, direction: i32, actions: &mut Vec<Action>) {
        if let WidgetKind::Scrollbar { value, page, .. } = self.nodes[id].kind {
            actions
                .extend(self.set_scroll(id, value.saturating_add(page.saturating_mul(direction))));
        }
    }
    pub fn focus_node(&mut self, id: NodeId) -> bool {
        if !self.focusable(id) {
            return false;
        }
        self.change_focus(Some(id), &mut Vec::new());
        true
    }
    /// Whether edit field `id` selects all its text when it gains the focus (by a click or by
    /// Tab); the click that gives it the focus then leaves that selection alone.
    pub fn set_select_on_focus(&mut self, id: NodeId, enabled: bool) {
        if enabled {
            self.select_on_focus.insert(id);
        } else {
            self.select_on_focus.remove(&id);
        }
    }
    pub fn set_multiline(&mut self, id: NodeId, enabled: bool) {
        if enabled {
            self.multiline.insert(id);
        } else {
            self.multiline.remove(&id);
        }
    }
    pub fn edit_selection(&self, id: NodeId) -> Option<(usize, usize)> {
        let WidgetKind::Edit { text, .. } = &self.nodes[id].kind else {
            return None;
        };
        let n = text.chars().count();
        let (cursor, anchor) = self.edits.get(&id).copied().unwrap_or((n, n));
        Some((cursor.min(n), anchor.min(n)))
    }
    pub fn place_caret(&mut self, id: NodeId, index: usize, extend: bool) {
        if let Some((_, anchor)) = self.edit_selection(id) {
            self.edits
                .insert(id, (index, if extend { anchor } else { index }));
            if let Some(state) = self.edit_layouts.get_mut(&id) {
                state.desired_x = None;
            }
            self.edit_activity(id);
            self.reveal_edit_caret(id);
        }
    }
    fn replace_selection(&mut self, id: NodeId, input: &str, actions: &mut Vec<Action>) {
        let Some((cursor, anchor)) = self.edit_selection(id) else {
            return;
        };
        let WidgetKind::Edit { text, max_chars } = &mut self.nodes[id].kind else {
            return;
        };
        let mut chars: Vec<char> = text.chars().collect();
        let (start, end) = (cursor.min(anchor), cursor.max(anchor));
        let remaining = max_chars.saturating_sub(chars.len() - (end - start));
        let inserted: Vec<_> = input
            .chars()
            .filter(|c| !c.is_control() || (*c == '\n' && self.multiline.contains(&id)))
            .take(remaining)
            .collect();
        let next = start + inserted.len();
        chars.splice(start..end, inserted);
        let replacement: String = chars.into_iter().collect();
        if *text != replacement {
            *text = replacement;
            actions.push(Action::Edit {
                id,
                text: text.clone(),
            });
        }
        self.edits.insert(id, (next, next));
        self.edit_activity(id);
        if let Some(state) = self.edit_layouts.get_mut(&id) {
            state.desired_x = None;
        }
    }
    fn edit_key(&mut self, id: NodeId, key: Key, _shift: bool, actions: &mut Vec<Action>) -> bool {
        let Some((cursor, anchor)) = self.edit_selection(id) else {
            return false;
        };
        let WidgetKind::Edit { text, .. } = &self.nodes[id].kind else {
            return false;
        };
        let chars: Vec<_> = text.chars().collect();
        let n = chars.len();
        // A selection collapses before the ordinary directional step, and Shift does not
        // extend the selection for these keys.
        let cursor = if cursor != anchor {
            if let Some(state) = self.edit_layouts.get_mut(&id) {
                state.desired_x = None;
            }
            match key {
                Key::Left | Key::Up | Key::WordLeft => cursor.min(anchor),
                Key::Right | Key::Down | Key::WordRight => cursor.max(anchor),
                _ => cursor,
            }
        } else {
            cursor
        };

        let next = match key {
            Key::SelectAll => {
                self.edits.insert(id, (n, 0));
                return true;
            }
            Key::Backspace | Key::Delete => {
                if cursor == anchor {
                    let end = if key == Key::Backspace {
                        cursor.saturating_sub(1)
                    } else {
                        (cursor + 1).min(n)
                    };
                    self.edits.insert(id, (cursor, end));
                }
                self.replace_selection(id, "", actions);
                return true;
            }
            Key::Left => cursor.saturating_sub(1),
            Key::Right => (cursor + 1).min(n),
            Key::Home => 0,
            Key::End => n,
            Key::WordLeft | Key::WordRight => {
                crate::text_edit::word(&chars, cursor, key == Key::WordRight)
            }
            Key::PageUp | Key::PageDown => {
                if let Some(state) = self.edit_layouts.get_mut(&id) {
                    let delta = if key == Key::PageUp {
                        -state.viewport.h
                    } else {
                        state.viewport.h
                    };
                    state.scroll = state.layout.clamp_scroll(
                        state.viewport.w,
                        state.viewport.h,
                        (state.scroll.0, state.scroll.1.saturating_add(delta)),
                    );
                }
                return true;
            }
            Key::Up | Key::Down if self.multiline.contains(&id) => {
                if let Some(state) = self.edit_layouts.get_mut(&id) {
                    let x = *state
                        .desired_x
                        .get_or_insert_with(|| state.layout.caret(cursor).x);
                    let next =
                        state
                            .layout
                            .vertical(cursor, x, if key == Key::Up { -1 } else { 1 });
                    self.edits.insert(id, (next, next));
                    self.edit_activity(id);
                    self.reveal_edit_caret(id);
                    return true;
                }
                let start = chars[..cursor]
                    .iter()
                    .rposition(|c| *c == '\n')
                    .map_or(0, |i| i + 1);
                let column = cursor - start;
                if key == Key::Up {
                    if start == 0 {
                        0
                    } else {
                        let previous = chars[..start - 1]
                            .iter()
                            .rposition(|c| *c == '\n')
                            .map_or(0, |i| i + 1);
                        (previous + column).min(start - 1)
                    }
                } else {
                    let end = chars[cursor..]
                        .iter()
                        .position(|c| *c == '\n')
                        .map_or(n, |i| cursor + i);
                    if end == n {
                        n
                    } else {
                        let next_end = chars[end + 1..]
                            .iter()
                            .position(|c| *c == '\n')
                            .map_or(n, |i| end + 1 + i);
                        (end + 1 + column).min(next_end)
                    }
                }
            }
            Key::Enter if self.multiline.contains(&id) => {
                self.replace_selection(id, "\n", actions);
                return true;
            }
            _ => return false,
        };
        self.edits.insert(id, (next, next));
        if let Some(state) = self.edit_layouts.get_mut(&id) {
            state.desired_x = None;
        }
        self.edit_activity(id);
        self.reveal_edit_caret(id);
        true
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, parent: Option<NodeId>, mut node: Node) -> NodeId {
        assert!(
            parent.is_none_or(|p| p < self.nodes.len()),
            "parent must exist"
        );
        node.parent = parent;
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }
    /// Parent edits must keep an acyclic tree; invalid chains are excluded safely.
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id]
    }
    pub fn focus(&self) -> Option<NodeId> {
        self.focus
    }
    /// A screen-specific control callback can relinquish keyboard focus.
    pub fn release_focus(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        self.change_focus(None, &mut actions);
        actions
    }
    pub fn capture(&self) -> Option<NodeId> {
        self.capture
    }

    /// Active-root filtering applies to focus. Hit testing still considers all roots.
    pub fn set_active_root(&mut self, root: Option<NodeId>) -> Vec<Action> {
        assert!(root.is_none_or(|id| self.nodes.get(id).is_some_and(|n| n.parent.is_none())));
        self.active_root = root;
        let mut actions = Vec::new();
        self.sanitize(&mut actions);
        actions
    }

    fn chain(&self, id: NodeId) -> Option<Vec<NodeId>> {
        let mut chain = Vec::new();
        let mut current = Some(id);
        while let Some(id) = current {
            let node = self.nodes.get(id)?;
            if chain.len() >= self.nodes.len() {
                return None;
            }
            chain.push(id);
            current = node.parent;
        }
        Some(chain)
    }

    pub fn absolute_rect(&self, id: NodeId) -> Option<Rect> {
        let chain = self.chain(id)?;
        let mut rect = self.nodes[id].rect;
        for &parent in chain.iter().skip(1) {
            let node = &self.nodes[parent];
            rect.x = rect.x.saturating_add(node.rect.x);
            rect.y = rect.y.saturating_add(node.rect.y);
            if let WidgetKind::List { offset, .. } = node.kind {
                rect.y = rect.y.saturating_sub(offset);
            }
        }
        Some(rect)
    }

    pub fn clip_rect(&self, id: NodeId) -> Option<Rect> {
        let mut clip = self.absolute_rect(id)?;
        for ancestor in self.chain(id)? {
            if !self.nodes[ancestor].visible {
                return None;
            }
            clip = clip.intersect(self.absolute_rect(ancestor)?)?;
        }
        Some(clip)
    }

    fn available(&self, id: NodeId) -> bool {
        self.clip_rect(id).is_some()
            && self
                .chain(id)
                .is_some_and(|chain| chain.iter().all(|&id| self.nodes[id].enabled))
    }

    fn focusable(&self, id: NodeId) -> bool {
        self.available(id)
            && self.nodes[id].focusable
            && self.active_root.is_none_or(|root| {
                self.chain(id)
                    .is_some_and(|chain| chain.last() == Some(&root))
            })
    }

    fn hit_branch(&self, id: NodeId, x: i32, y: i32, depth: usize) -> Option<NodeId> {
        if depth >= self.nodes.len() || !self.clip_rect(id)?.contains(x, y) {
            return None;
        }
        for child in (0..self.nodes.len())
            .rev()
            .filter(|&child| self.nodes[child].parent == Some(id))
        {
            if let Some(hit) = self.hit_branch(child, x, y, depth + 1) {
                return Some(hit);
            }
        }
        Some(id)
    }

    pub fn hit_test(&self, x: i32, y: i32) -> Option<NodeId> {
        for root in (0..self.nodes.len())
            .rev()
            .filter(|&id| self.nodes[id].parent.is_none())
        {
            if let Some(hit) = self.hit_branch(root, x, y, 0) {
                return Some(hit);
            }
        }
        None
    }

    fn input_target(&self, x: i32, y: i32) -> Option<NodeId> {
        let hit = self.hit_test(x, y)?;
        // Labels can be children of controls. A disabled hit occludes those behind it.
        if !self.available(hit) {
            return None;
        }
        self.chain(hit)?
            .into_iter()
            .find(|&id| !matches!(self.nodes[id].kind, WidgetKind::Panel | WidgetKind::Text))
    }

    /// Pending click visual: checkbox renderers XOR this with stored checked state.
    pub fn pressed(&self, id: NodeId) -> bool {
        self.capture == Some(id)
            && self.available(id)
            && self
                .clip_rect(id)
                .is_some_and(|r| r.contains(self.pointer.0, self.pointer.1))
    }

    fn change_focus(&mut self, focus: Option<NodeId>, actions: &mut Vec<Action>) {
        if self.focus != focus {
            self.focus = focus;
            if let Some(id) = focus {
                self.edit_activity(id);
                if self.select_on_focus.contains(&id) {
                    if let WidgetKind::Edit { text, .. } = &self.nodes[id].kind {
                        let n = text.chars().count();
                        self.edits.insert(id, (n, 0));
                    }
                }
            }
            actions.push(Action::Focus(focus));
        }
    }

    fn sanitize(&mut self, actions: &mut Vec<Action>) {
        if self.capture.is_some_and(|id| !self.available(id)) {
            if let Some(id) = self.capture {
                if let Some(state) = self.edit_layouts.get_mut(&id) {
                    state.smooth = None;
                }
            }
            self.capture = None;
            self.edit_drag_origin = None;
            self.scrollbar_drag = None;
            self.scrollbar_track = None;
            self.scrollbar_repeat = None;
        }
        if self.focus.is_some_and(|id| !self.focusable(id)) {
            self.change_focus(None, actions);
        }
    }

    fn cycle_focus(&mut self, backwards: bool, actions: &mut Vec<Action>) {
        let eligible: Vec<_> = (0..self.nodes.len())
            .filter(|&id| self.focusable(id))
            .collect();
        if eligible.is_empty() {
            self.change_focus(None, actions);
            return;
        }
        let current = self
            .focus
            .and_then(|id| eligible.iter().position(|&candidate| candidate == id));
        let index = match (current, backwards) {
            (None, false) => 0,
            (None, true) => eligible.len() - 1,
            (Some(i), false) => (i + 1) % eligible.len(),
            (Some(i), true) => (i + eligible.len() - 1) % eligible.len(),
        };
        self.change_focus(Some(eligible[index]), actions);
    }

    fn activate(&mut self, id: NodeId, actions: &mut Vec<Action>) {
        match &mut self.nodes[id].kind {
            WidgetKind::Button => actions.push(Action::Activate(id)),
            WidgetKind::Checkbox { checked } => {
                *checked = !*checked;
                actions.push(Action::Toggle {
                    id,
                    checked: *checked,
                });
            }
            _ => {}
        }
    }

    /// Updates list/range state and emits only actual changes. Screens bind ranges to lists.
    pub fn set_scroll(&mut self, id: NodeId, value: i32) -> Vec<Action> {
        let mut actions = Vec::new();
        let height = self.nodes[id].rect.h.max(0);
        match &mut self.nodes[id].kind {
            WidgetKind::List {
                content_height,
                offset,
                ..
            } => {
                let value = value.clamp(0, content_height.saturating_sub(height).max(0));
                if *offset != value {
                    *offset = value;
                    actions.push(Action::Scroll { id, value });
                }
            }
            WidgetKind::Scrollbar {
                min,
                max,
                value: old,
                ..
            } => {
                let value = value.clamp(*min, (*max).max(*min));
                if *old != value {
                    *old = value;
                    actions.push(Action::Scroll { id, value });
                }
            }
            _ => {}
        }
        self.sanitize(&mut actions);
        actions
    }

    fn scroll_delta(&mut self, id: NodeId, delta: i32, actions: &mut Vec<Action>) {
        let value = match self.nodes[id].kind {
            WidgetKind::List {
                offset, row_height, ..
            } => offset.saturating_add(delta.saturating_mul(row_height.max(1))),
            WidgetKind::Scrollbar { value, step, .. } => {
                value.saturating_add(delta.saturating_mul(step.max(1)))
            }
            _ => return,
        };
        actions.extend(self.set_scroll(id, value));
    }

    /// Art-sized arrows/thumb. Defaults are 16 pixels until the screen binds its art.
    pub fn set_scrollbar_geometry(&mut self, id: NodeId, arrow_height: i32, thumb_height: i32) {
        self.scrollbar_geometry
            .insert(id, (arrow_height.max(0), thumb_height.max(1)));
    }

    pub fn scrollbar_thumb(&self, id: NodeId) -> Option<Rect> {
        let rect = self.absolute_rect(id)?;
        let WidgetKind::Scrollbar {
            min, max, value, ..
        } = self.nodes[id].kind
        else {
            return None;
        };
        if rect.h <= 0 {
            return None;
        }
        let range = (i64::from(max) - i64::from(min)).max(0);
        let (arrow, height) = self
            .scrollbar_geometry
            .get(&id)
            .copied()
            .unwrap_or((16, 16));
        let arrow = arrow.min(rect.h / 2);
        let track_length = i64::from(rect.h) - 2 * i64::from(arrow) + 1;
        let height = i32_from_i64(i64::from(height).min(track_length.max(1)));
        // The track length is inclusive; the thumb's travel is scaled by the value.
        let travel = (track_length - i64::from(height)).max(0);
        let offset = if range == 0 {
            0
        } else {
            (i64::from(value).clamp(i64::from(min), i64::from(max)) - i64::from(min)) * travel
                / range
        };
        Some(Rect {
            y: rect
                .y
                .saturating_add(arrow)
                .saturating_add(i32_from_i64(offset)),
            h: height,
            ..rect
        })
    }

    fn drag_scrollbar(&mut self, id: NodeId, y: i32, actions: &mut Vec<Action>) {
        let Some((drag_id, grab_offset)) = self.scrollbar_drag else {
            return;
        };
        if id != drag_id {
            return;
        }
        let Some(rect) = self.absolute_rect(id) else {
            return;
        };
        let Some(thumb) = self.scrollbar_thumb(id) else {
            return;
        };
        let WidgetKind::Scrollbar { min, max, .. } = self.nodes[id].kind else {
            return;
        };
        let arrow = self
            .scrollbar_geometry
            .get(&id)
            .copied()
            .unwrap_or((16, 16))
            .0
            .min(rect.h / 2);
        let travel = (i64::from(rect.h) - 2 * i64::from(arrow) + 1 - i64::from(thumb.h)).max(0);
        let range = (i64::from(max) - i64::from(min)).max(0);
        let position =
            (i64::from(y) - i64::from(rect.y) - i64::from(arrow) - i64::from(grab_offset))
                .clamp(0, travel);
        let value = i64::from(min)
            + if travel == 0 {
                0
            } else {
                position * range / travel
            };
        actions.extend(self.set_scroll(id, i32_from_i64(value)));
    }

    pub fn handle(&mut self, input: Input) -> Vec<Action> {
        let mut actions = Vec::new();
        self.sanitize(&mut actions);
        match input {
            Input::PointerMove { x, y } => {
                self.pointer = (x, y);
                if let Some(id) = self.capture {
                    self.drag_scrollbar(id, y, &mut actions);
                    self.pointer_edit(id, x, y, true);
                }
            }
            Input::PointerDown { x, y } => {
                self.pointer = (x, y);
                self.capture = None;
                self.edit_drag_origin = None;
                self.scrollbar_drag = None;
                self.scrollbar_track = None;
                self.scrollbar_repeat = None;
                if let Some(id) = self.input_target(x, y) {
                    let selects_all = self.focus != Some(id) && self.select_on_focus.contains(&id);
                    if self.focusable(id) {
                        self.change_focus(Some(id), &mut actions);
                    }
                    if !selects_all {
                        self.capture = Some(id);
                        self.pointer_edit(id, x, y, false);
                    }
                    if let Some(thumb) = self.scrollbar_thumb(id) {
                        let rect = self.absolute_rect(id).unwrap();
                        let arrow = self
                            .scrollbar_geometry
                            .get(&id)
                            .copied()
                            .unwrap_or((16, 16))
                            .0
                            .min(rect.h / 2);
                        if y < rect.y.saturating_add(arrow) {
                            self.scroll_delta(id, -1, &mut actions);
                            self.scrollbar_repeat = Some((id, -1, self.time + (0.3 - 0.1)));
                        } else if i64::from(y) >= i64::from(rect.y) + i64::from(rect.h - arrow) {
                            self.scroll_delta(id, 1, &mut actions);
                            self.scrollbar_repeat = Some((id, 1, self.time + (0.3 - 0.1)));
                        } else if thumb.contains(x, y)
                            || self.scrollbar_flags.get(&id).copied().unwrap_or(0) & 8 != 0
                        {
                            let grab_offset = if thumb.contains(x, y) {
                                y - thumb.y
                            } else {
                                thumb.h / 2
                            };
                            self.scrollbar_drag = Some((id, grab_offset));
                            self.drag_scrollbar(id, y, &mut actions);
                        } else {
                            let direction = if y < thumb.y + thumb.h / 2 { -1 } else { 1 };
                            self.page_scrollbar(id, direction, &mut actions);
                            self.scrollbar_track = Some((id, direction, self.time));
                        }
                    }
                }
            }
            Input::PointerUp { x, y } => {
                self.pointer = (x, y);
                self.scrollbar_drag = None;
                self.scrollbar_track = None;
                self.scrollbar_repeat = None;
                if let Some(id) = self.capture.take() {
                    self.pointer_edit(id, x, y, true);
                    self.edit_drag_origin = None;
                    if let Some(state) = self.edit_layouts.get_mut(&id) {
                        state.smooth = None;
                    }
                    if self.available(id) && self.clip_rect(id).is_some_and(|r| r.contains(x, y)) {
                        self.activate(id, &mut actions);
                        let rect = self.absolute_rect(id).unwrap();
                        if let WidgetKind::List {
                            offset,
                            row_height,
                            content_height,
                            selected,
                        } = &mut self.nodes[id].kind
                        {
                            let local_y = i64::from(y) - i64::from(rect.y) + i64::from(*offset);
                            if *row_height > 0
                                && local_y >= 0
                                && local_y < i64::from(*content_height)
                            {
                                let index = usize::try_from(local_y / i64::from(*row_height))
                                    .unwrap_or(usize::MAX);
                                // A press on the row already chosen is reported too: a
                                // window may act on it (a pending break or dismissal) or
                                // let it go.
                                *selected = Some(index);
                                actions.push(Action::Select { id, index });
                            }
                        }
                    }
                }
            }
            Input::Wheel { x, y, delta } => {
                if let Some(hit) = self.hit_test(x, y).filter(|&id| self.available(id)) {
                    if let Some(id) = self.chain(hit).unwrap().into_iter().find(|&id| {
                        matches!(
                            self.nodes[id].kind,
                            WidgetKind::List { .. } | WidgetKind::Scrollbar { .. }
                        )
                    }) {
                        self.scroll_delta(id, delta, &mut actions);
                    }
                }
            }
            Input::Key {
                key: Key::Tab,
                shift,
            } => self.cycle_focus(shift, &mut actions),
            Input::Key { key, shift } => {
                if let Some(id) = self.focus {
                    if self.edit_key(id, key, shift, &mut actions) {
                        return actions;
                    }
                    match key {
                        Key::Enter => self.activate(id, &mut actions),
                        Key::Space => {} // Printable spaces arrive through Input::Text.
                        Key::Up => self.scroll_delta(id, -1, &mut actions),
                        Key::Down => self.scroll_delta(id, 1, &mut actions),
                        Key::Home | Key::End => actions.extend(
                            self.set_scroll(id, if key == Key::Home { i32::MIN } else { i32::MAX }),
                        ),
                        Key::Backspace
                        | Key::Delete
                        | Key::Left
                        | Key::Right
                        | Key::SelectAll
                        | Key::WordLeft
                        | Key::WordRight
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Escape => {}
                        Key::Tab => unreachable!(),
                        Key::Other => {}
                    }
                }
            }
            Input::Text(input) => {
                if let Some(id) = self.focus {
                    self.replace_selection(id, &input, &mut actions);
                }
            }
            Input::RightClick { .. } => {}
            Input::Cancel => {
                if let Some(id) = self.capture {
                    if let Some(state) = self.edit_layouts.get_mut(&id) {
                        state.smooth = None;
                    }
                }
                self.capture = None;
                self.edit_drag_origin = None;
                self.scrollbar_drag = None;
                self.scrollbar_track = None;
                self.scrollbar_repeat = None;
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;

    fn measured_edit(text: &str, advances: &[i32], width: i32, height: i32) -> (UiTree, NodeId) {
        let mut tree = UiTree::new();
        let id = tree.add(
            None,
            Node::new(
                WidgetKind::Edit {
                    text: text.into(),
                    max_chars: 100,
                },
                rect(10, 20, width + 4, height + 4),
            ),
        );
        tree.set_multiline(id, true);
        tree.set_edit_layout(
            id,
            crate::text_edit::Layout::new(text, advances, 15, width, true),
            rect(2, 2, width, height),
        );
        tree.place_caret(id, 0, false);
        tree.focus_node(id);
        (tree, id)
    }
    #[test]
    fn edit_drag_selects_measured_characters_across_wrapped_lines() {
        let (mut tree, id) = measured_edit("aa bb", &[4; 5], 13, 45);
        tree.handle(Input::PointerDown { x: 15, y: 23 });
        tree.handle(Input::PointerMove { x: 19, y: 38 });
        tree.handle(Input::PointerUp { x: 19, y: 38 });
        assert_eq!(tree.edit_selection(id), Some((4, 1)));
        assert_eq!(tree.edit_layout(id).unwrap().selection(1, 5).len(), 2);
    }
    #[test]
    fn a_click_that_focuses_a_prompt_field_selects_all_its_text() {
        let (mut tree, id) = measured_edit("Enter name", &[4; 10], 60, 15);
        tree.set_multiline(id, false);
        let other = tree.add(
            None,
            Node::new(
                WidgetKind::Edit {
                    text: String::new(),
                    max_chars: 10,
                },
                rect(200, 200, 20, 20),
            ),
        );
        assert!(tree.focus_node(other));
        tree.set_select_on_focus(id, true);
        tree.handle(Input::PointerDown { x: 20, y: 25 });
        tree.handle(Input::PointerUp { x: 20, y: 25 });
        assert_eq!(tree.edit_selection(id), Some((10, 0)));
        // A second click, with the focus already there, places the caret as usual.
        tree.handle(Input::PointerDown { x: 20, y: 25 });
        tree.handle(Input::PointerUp { x: 20, y: 25 });
        let (cursor, anchor) = tree.edit_selection(id).unwrap();
        assert_eq!(cursor, anchor);
    }
    #[test]
    fn ordinary_track_pages_while_direct_flag_drags_to_pointer() {
        let mut tree = UiTree::new();
        let id = tree.add(
            None,
            Node::new(
                WidgetKind::Scrollbar {
                    min: 0,
                    max: 100,
                    value: 0,
                    page: 20,
                    step: 1,
                },
                rect(0, 0, 16, 200),
            ),
        );
        tree.tick(0.0);
        assert_eq!(
            tree.handle(Input::PointerDown { x: 4, y: 160 }),
            vec![Action::Focus(Some(id)), Action::Scroll { id, value: 20 }]
        );
        assert_eq!(tree.tick(0.25), vec![Action::Scroll { id, value: 40 }]);
        tree.handle(Input::PointerMove { x: 4, y: 20 });
        assert!(tree.tick(0.5).is_empty());
        tree.handle(Input::PointerUp { x: 4, y: 20 });
        tree.set_scroll(id, 0);
        tree.set_scrollbar_flags(id, 8);
        tree.handle(Input::PointerDown { x: 4, y: 160 });
        assert!(matches!(
            tree.node(id).kind,
            WidgetKind::Scrollbar { value: 88, .. }
        ));
    }
    #[test]
    fn arrow_repeat_delays_then_pauses_outside_and_stops_on_release() {
        let mut tree = UiTree::new();
        let id = tree.add(
            None,
            Node::new(
                WidgetKind::Scrollbar {
                    min: 0,
                    max: 100,
                    value: 0,
                    page: 10,
                    step: 1,
                },
                rect(0, 0, 16, 100),
            ),
        );
        tree.tick(1.0);
        tree.handle(Input::PointerDown { x: 4, y: 90 });
        assert_eq!(tree.tick(1.29), Vec::<Action>::new());
        assert_eq!(tree.tick(1.31), vec![Action::Scroll { id, value: 2 }]);
        tree.handle(Input::PointerMove { x: 40, y: 90 });
        assert!(tree.tick(2.0).is_empty());
        tree.handle(Input::PointerMove { x: 4, y: 90 });
        assert_eq!(tree.tick(2.01), vec![Action::Scroll { id, value: 3 }]);
        tree.handle(Input::PointerUp { x: 4, y: 90 });
        assert!(tree.tick(3.0).is_empty());
    }
    #[test]
    fn edit_drag_scroll_speed_tracks_distance_and_cancel_stops_animation() {
        let (mut tree, id) = measured_edit("a\nb\nc\nd\ne", &[4; 9], 20, 15);
        tree.set_edit_smooth_scroll(id, true);
        tree.tick(1.0);
        tree.handle(Input::PointerDown { x: 12, y: 22 });
        tree.handle(Input::PointerMove { x: 12, y: 46 }); // 10 below bottom -> 100 px/s.
        tree.tick(1.1);
        assert_eq!(tree.edit_scroll(id), Some((0, 10)));
        tree.handle(Input::Cancel);
        tree.tick(2.0);
        assert_eq!(tree.edit_scroll(id), Some((0, 10)));
    }
    #[test]
    fn arrows_ignore_shift_and_step_after_collapsing_selection() {
        let (mut tree, id) = measured_edit("abcdef", &[4; 6], 40, 15);
        tree.place_caret(id, 2, false);
        tree.place_caret(id, 4, true);
        tree.handle(Input::Key {
            key: Key::Left,
            shift: true,
        });
        assert_eq!(tree.edit_selection(id), Some((1, 1)));
        tree.place_caret(id, 2, false);
        tree.place_caret(id, 4, true);
        tree.handle(Input::Key {
            key: Key::Right,
            shift: true,
        });
        assert_eq!(tree.edit_selection(id), Some((5, 5)));
    }
    #[test]
    fn double_click_selects_only_word_fragment_on_visual_line() {
        let (mut tree, id) = measured_edit("abcdef", &[4; 6], 13, 30);
        tree.select_edit_word(id, 14, 38);
        assert_eq!(tree.edit_selection(id), Some((6, 3)));
    }
    #[test]
    fn page_keys_scroll_without_moving_caret_and_home_end_use_document() {
        let (mut tree, id) = measured_edit("a\nb\nc\nd", &[4; 7], 20, 15);
        tree.handle(Input::Key {
            key: Key::PageDown,
            shift: false,
        });
        assert_eq!(tree.edit_selection(id), Some((0, 0)));
        assert_eq!(tree.edit_scroll(id), Some((0, 15)));
        let layout = tree.edit_layout(id).unwrap().clone();
        tree.set_edit_layout(id, layout, rect(2, 2, 20, 15));
        assert_eq!(tree.edit_scroll(id), Some((0, 15)));
        tree.handle(Input::Key {
            key: Key::End,
            shift: false,
        });
        assert_eq!(tree.edit_selection(id), Some((7, 7)));
        assert_eq!(tree.edit_scroll(id), Some((0, 45)));
        tree.handle(Input::Key {
            key: Key::Home,
            shift: false,
        });
        assert_eq!(tree.edit_selection(id), Some((0, 0)));
        assert_eq!(tree.edit_scroll(id), Some((0, 0)));
    }
    #[test]
    fn vertical_navigation_retains_pixel_column_through_short_line() {
        let (mut tree, id) = measured_edit("Wi\ni\niWi", &[10, 2, 0, 2, 0, 2, 10, 2], 40, 45);
        tree.place_caret(id, 1, false);
        tree.handle(Input::Key {
            key: Key::Down,
            shift: false,
        });
        assert_eq!(tree.edit_selection(id), Some((4, 4)));
        tree.handle(Input::Key {
            key: Key::Down,
            shift: false,
        });
        assert_eq!(tree.edit_selection(id), Some((7, 7)));
    }
    #[test]
    fn selection_replacement_and_delete_use_character_boundaries() {
        let mut tree = UiTree::new();
        let id = tree.add(
            None,
            Node::new(
                WidgetKind::Edit {
                    text: "aé🙂z".into(),
                    max_chars: 5,
                },
                rect(0, 0, 100, 20),
            ),
        );
        click(&mut tree, 2, 2);
        tree.handle(Input::Key {
            key: Key::Left,
            shift: false,
        });
        tree.place_caret(id, 2, true);
        assert_eq!(tree.edit_selection(id), Some((2, 3)));
        tree.handle(Input::Text("XY".into()));
        assert!(matches!(&tree.node(id).kind,WidgetKind::Edit{text,..} if text=="aéXYz"));
        tree.handle(Input::Key {
            key: Key::Delete,
            shift: false,
        });
        assert!(matches!(&tree.node(id).kind,WidgetKind::Edit{text,..} if text=="aéXY"));
        tree.handle(Input::Key {
            key: Key::SelectAll,
            shift: false,
        });
        tree.handle(Input::Text("1234567".into()));
        assert!(matches!(&tree.node(id).kind,WidgetKind::Edit{text,..} if text=="12345"));
    }
    #[test]
    fn multiline_enter_inserts_break_and_vertical_keys_keep_column() {
        let mut tree = UiTree::new();
        let id = tree.add(
            None,
            Node::new(
                WidgetKind::Edit {
                    text: "abc\ndefgh".into(),
                    max_chars: 40,
                },
                rect(0, 0, 100, 60),
            ),
        );
        tree.set_multiline(id, true);
        click(&mut tree, 2, 2);
        tree.handle(Input::Key {
            key: Key::Up,
            shift: false,
        });
        assert_eq!(tree.edit_selection(id), Some((3, 3)));
        tree.handle(Input::Key {
            key: Key::Enter,
            shift: false,
        });
        assert!(matches!(&tree.node(id).kind,WidgetKind::Edit{text,..} if text=="abc\n\ndefgh"));
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn check(tree: &mut UiTree, parent: Option<NodeId>, r: Rect) -> NodeId {
        tree.add(
            parent,
            Node::new(WidgetKind::Checkbox { checked: false }, r),
        )
    }
    fn click(tree: &mut UiTree, x: i32, y: i32) -> Vec<Action> {
        tree.handle(Input::PointerDown { x, y });
        tree.handle(Input::PointerUp { x, y })
    }

    #[test]
    fn nested_clips_and_inclusive_last_pixel_bound_hit_testing() {
        let mut tree = UiTree::new();
        let root = tree.add(None, Node::new(WidgetKind::Panel, rect(10, 20, 20, 20)));
        let child = check(&mut tree, Some(root), rect(15, 15, 10, 10));
        assert_eq!(tree.clip_rect(child), Some(rect(25, 35, 5, 5)));
        assert_eq!(tree.hit_test(29, 39), Some(child));
        assert_eq!(tree.hit_test(30, 39), None);
        assert!(!Rect {
            x: i32::MAX,
            y: 0,
            w: 2,
            h: 1
        }
        .contains(i32::MIN, 0));
    }

    #[test]
    fn newest_sibling_wins_but_a_later_grandchild_does_not_raise_its_parent() {
        let mut tree = UiTree::new();
        let first = tree.add(None, Node::new(WidgetKind::Panel, rect(0, 0, 20, 20)));
        let top = check(&mut tree, None, rect(0, 0, 20, 20));
        check(&mut tree, Some(first), rect(0, 0, 20, 20));
        assert_eq!(tree.hit_test(5, 5), Some(top));
        tree.node_mut(top).enabled = false;
        assert!(click(&mut tree, 5, 5).is_empty());
    }

    #[test]
    fn checkbox_commits_on_release_inside_and_restores_after_cancel() {
        let mut tree = UiTree::new();
        let id = check(&mut tree, None, rect(0, 0, 13, 13));
        tree.handle(Input::PointerDown { x: 1, y: 1 });
        tree.handle(Input::PointerMove { x: 30, y: 1 });
        assert_eq!(tree.capture(), Some(id));
        assert!(!tree.pressed(id));
        tree.handle(Input::PointerMove { x: 1, y: 1 });
        assert!(tree.pressed(id));
        assert_eq!(
            tree.handle(Input::PointerUp { x: 1, y: 1 }),
            vec![Action::Toggle { id, checked: true }]
        );
        tree.handle(Input::PointerDown { x: 1, y: 1 });
        assert!(tree.handle(Input::PointerUp { x: 13, y: 1 }).is_empty());
        tree.handle(Input::PointerDown { x: 1, y: 1 });
        tree.handle(Input::Cancel);
        assert!(tree.handle(Input::PointerUp { x: 1, y: 1 }).is_empty());
        assert_eq!(tree.node(id).kind, WidgetKind::Checkbox { checked: true });
    }

    #[test]
    fn hiding_ancestor_cancels_capture_and_focus_before_release() {
        let mut tree = UiTree::new();
        let root = tree.add(None, Node::new(WidgetKind::Panel, rect(0, 0, 20, 20)));
        check(&mut tree, Some(root), rect(0, 0, 13, 13));
        tree.handle(Input::PointerDown { x: 1, y: 1 });
        tree.node_mut(root).visible = false;
        assert_eq!(
            tree.handle(Input::PointerUp { x: 1, y: 1 }),
            vec![Action::Focus(None)]
        );
        assert_eq!(tree.capture(), None);
    }

    #[test]
    fn focus_wraps_in_registration_order_and_respects_active_root() {
        let mut tree = UiTree::new();
        let root = tree.add(None, Node::new(WidgetKind::Panel, rect(0, 0, 100, 100)));
        let a = check(&mut tree, Some(root), rect(0, 0, 13, 13));
        let b = check(&mut tree, Some(root), rect(20, 0, 13, 13));
        check(&mut tree, None, rect(100, 0, 13, 13));
        tree.set_active_root(Some(root));
        let tab = Input::Key {
            key: Key::Tab,
            shift: false,
        };
        tree.handle(tab.clone());
        assert_eq!(tree.focus(), Some(a));
        tree.handle(Input::Key {
            key: Key::Tab,
            shift: true,
        });
        assert_eq!(tree.focus(), Some(b));
        tree.handle(tab.clone());
        assert_eq!(tree.focus(), Some(a));
        tree.node_mut(b).enabled = false;
        assert!(tree.handle(tab).is_empty());
    }

    #[test]
    fn scrolling_translates_children_and_clamps_to_content_extent() {
        let mut tree = UiTree::new();
        let list = tree.add(
            None,
            Node::new(
                WidgetKind::List {
                    content_height: 900,
                    offset: 0,
                    row_height: 20,
                    selected: None,
                },
                rect(4, 16, 280, 264),
            ),
        );
        let child = check(&mut tree, Some(list), rect(0, 880, 13, 13));
        assert_eq!(tree.clip_rect(child), None);
        assert_eq!(
            tree.set_scroll(list, 1000),
            vec![Action::Scroll {
                id: list,
                value: 636
            }]
        );
        assert_eq!(tree.absolute_rect(child), Some(rect(4, 260, 13, 13)));
        assert_eq!(
            click(&mut tree, 5, 261),
            vec![Action::Toggle {
                id: child,
                checked: true
            }]
        );
        tree.handle(Input::Wheel {
            x: 5,
            y: 261,
            delta: -1,
        });
        assert!(matches!(
            tree.node(list).kind,
            WidgetKind::List { offset: 616, .. }
        ));
        assert_eq!(
            tree.set_scroll(list, -10)[0],
            Action::Scroll { id: list, value: 0 }
        );
    }

    #[test]
    fn scrollbar_drag_clamps_outside_and_thumb_keeps_its_art_size() {
        let mut tree = UiTree::new();
        let bar = tree.add(
            None,
            Node::new(
                WidgetKind::Scrollbar {
                    min: 0,
                    max: 636,
                    value: 0,
                    page: 264,
                    step: 20,
                },
                rect(0, 0, 16, 264),
            ),
        );
        tree.handle(Input::PointerDown { x: 5, y: 20 });
        tree.handle(Input::PointerMove { x: 50, y: 999 });
        assert!(matches!(
            tree.node(bar).kind,
            WidgetKind::Scrollbar { value: 636, .. }
        ));
        tree.handle(Input::PointerUp { x: 50, y: 999 });
        assert_eq!(tree.capture(), None);
        tree.node_mut(bar).kind = WidgetKind::Scrollbar {
            min: 0,
            max: 0,
            value: 0,
            page: 264,
            step: 20,
        };
        assert_eq!(tree.scrollbar_thumb(bar), Some(rect(0, 16, 16, 16)));
    }

    #[test]
    fn arrow_click_steps_once_and_thumb_grab_does_not_jump() {
        let mut tree = UiTree::new();
        let bar = tree.add(
            None,
            Node::new(
                WidgetKind::Scrollbar {
                    min: 0,
                    max: 636,
                    value: 0,
                    page: 264,
                    step: 20,
                },
                rect(0, 0, 16, 264),
            ),
        );
        tree.handle(Input::PointerDown { x: 5, y: 260 });
        assert!(matches!(
            tree.node(bar).kind,
            WidgetKind::Scrollbar { value: 20, .. }
        ));
        tree.handle(Input::PointerMove { x: 5, y: 100 });
        assert!(matches!(
            tree.node(bar).kind,
            WidgetKind::Scrollbar { value: 20, .. }
        ));
        tree.handle(Input::PointerUp { x: 5, y: 100 });
        tree.set_scroll(bar, 0);
        tree.handle(Input::PointerDown { x: 5, y: 17 });
        assert!(matches!(
            tree.node(bar).kind,
            WidgetKind::Scrollbar { value: 0, .. }
        ));
        tree.handle(Input::PointerMove { x: 5, y: 18 });
        assert!(matches!(
            tree.node(bar).kind,
            WidgetKind::Scrollbar { value: 2, .. }
        ));
    }

    #[test]
    fn enter_activates_focused_control_but_space_is_not_a_button_shortcut() {
        let mut tree = UiTree::new();
        let id = check(&mut tree, None, rect(0, 0, 13, 13));
        tree.handle(Input::Key {
            key: Key::Tab,
            shift: false,
        });
        assert!(tree
            .handle(Input::Key {
                key: Key::Space,
                shift: false
            })
            .is_empty());
        assert_eq!(
            tree.handle(Input::Key {
                key: Key::Enter,
                shift: false
            }),
            vec![Action::Toggle { id, checked: true }]
        );
    }

    #[test]
    fn list_selection_rejects_blank_tail_and_text_edit_respects_character_limit() {
        let mut tree = UiTree::new();
        let list = tree.add(
            None,
            Node::new(
                WidgetKind::List {
                    content_height: 40,
                    offset: 0,
                    row_height: 20,
                    selected: None,
                },
                rect(0, 0, 100, 100),
            ),
        );
        assert_eq!(
            click(&mut tree, 5, 25),
            vec![Action::Select { id: list, index: 1 }]
        );
        assert!(click(&mut tree, 5, 80).is_empty());
        assert_eq!(
            click(&mut tree, 5, 25),
            vec![Action::Select { id: list, index: 1 }],
            "a press on the chosen row is reported again"
        );
        let edit = tree.add(
            None,
            Node::new(
                WidgetKind::Edit {
                    text: String::new(),
                    max_chars: 3,
                },
                rect(100, 0, 100, 20),
            ),
        );
        click(&mut tree, 101, 1);
        tree.handle(Input::Text("a\nβ🦀z".into()));
        assert!(matches!(&tree.node(edit).kind, WidgetKind::Edit { text, .. } if text == "aβ🦀"));
        tree.handle(Input::Key {
            key: Key::Backspace,
            shift: false,
        });
        assert!(matches!(&tree.node(edit).kind, WidgetKind::Edit { text, .. } if text == "aβ"));
    }
}
