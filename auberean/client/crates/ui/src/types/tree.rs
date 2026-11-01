//! Trees: elements (nodes) joined parent to child by lines, either placed where the layout puts
//! them (each naming up to three children) or on a grid of node-sized cells (each naming up to
//! two parents by their cells), on a sheet the view scrolls over; one node selected.
//!
//! A tree grows up from its first rows at the bottom; `orient_high` makes it grow down;
//! `horizontal` makes it grow across, to the right (to the left with `orient_high`).
//! Lines run from a node's parent connection point to each child's child connection point,
//! straight or (with `straight_connect`) in three square segments.

use crate::system::{Handle, Ui, Value};
use crate::token::{attr, msg, Token};
use crate::types::button_id;
use crate::types::listbox::{name as lname, Scroll, SCROLLBAR_MESSAGES_H, SCROLLBAR_MESSAGES_V};

/// One node of a tree: its element, its cell, and the nodes it hangs from and that hang from it
/// (by their places in the tree's node list).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub element: Handle,
    pub gridx: i32,
    pub gridy: i32,
    pub parents: Vec<Handle>,
    pub children: Vec<Handle>,
}

impl TreeNode {
    /// A node for an element, at no cell, hanging from nothing.
    // D2-11189
    #[must_use]
    pub const fn new(element: Handle) -> Self {
        Self {
            element,
            gridx: 0,
            gridy: 0,
            parents: Vec::new(),
            children: Vec::new(),
        }
    }
}

/// A line from a parent to a child: from `(x0, y0)` (with the parent's away distances) to
/// `(x1, y1)`, in the tree's coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub x0: i32,
    pub y0: i32,
    pub away_x: i32,
    pub away_y: i32,
    pub x1: i32,
    pub y1: i32,
}

/// What a tree keeps between events.
#[derive(Clone, Debug, Default)]
pub struct TreeState {
    pub nodes: Vec<TreeNode>,
    pub lines: Vec<Line>,
    pub selected: Option<Handle>,
    pub paper_w: i32,
    pub paper_h: i32,
    pub x_view: i32,
    pub y_view: i32,
    /// The view is being set from a scroll bar.
    pub from_scrollbar: bool,
    pub node_w: i32,
    pub node_h: i32,
    /// The furthest cell used, across and down.
    pub max_x: i32,
    pub max_y: i32,
    /// Several nodes change at once: the layout waits.
    pub multi_op: bool,
}

mod name {
    use crate::Token;
    pub const NODE_WIDTH: Token = Token::of("node_width");
    pub const NODE_HEIGHT: Token = Token::of("node_height");
    pub const HORIZONTAL: Token = Token::of("horizontal");
    pub const ORIENT_HIGH: Token = Token::of("orient_high");
    pub const GRID: Token = Token::of("grid");
    pub const GRIDX: Token = Token::of("gridx");
    pub const GRIDY: Token = Token::of("gridy");
    pub const CHILDREN: [Token; 3] = [
        Token::of("child1"),
        Token::of("child2"),
        Token::of("child3"),
    ];
    pub const PARENTS: [(Token, Token); 2] = [
        (Token::of("Parent1x"), Token::of("Parent1y")),
        (Token::of("Parent2x"), Token::of("Parent2y")),
    ];
    pub const NO_LINES: Token = Token::of("no_lines");
    pub const LINE_COLOR_RED: Token = Token::of("line_color_red");
    pub const LINE_COLOR_GREEN: Token = Token::of("line_color_green");
    pub const LINE_COLOR_BLUE: Token = Token::of("line_color_blue");
    pub const STRAIGHT_CONNECT: Token = Token::of("straight_connect");
    pub const STRAIGHT_AWAY_PIX: Token = Token::of("straight_away_pix");
    pub const CHILD_CONNECT_X: Token = Token::of("node_child_connect_x");
    pub const CHILD_CONNECT_Y: Token = Token::of("node_child_connect_y");
    pub const PARENT_CONNECT_X: Token = Token::of("node_parent_connect_x");
    pub const PARENT_CONNECT_Y: Token = Token::of("node_parent_connect_y");
    pub const AWAY_X: Token = Token::of("node_away_x");
    pub const AWAY_Y: Token = Token::of("node_away_y");
    /// The tree's messages.
    pub const MOUSE_PRESS: Token = Token::of("mouse_press");
}

impl Ui {
    /// A node's name.
    // D2-11185
    fn node_token(&self, e: Handle) -> u32 {
        self.node(e).map_or(0, |n| n.id.raw())
    }

    fn tree(&self, h: Handle) -> Option<&TreeState> {
        self.node(h).map(|n| &n.lists.tree)
    }

    fn tree_mut(&mut self, h: Handle) -> Option<&mut TreeState> {
        self.node_mut(h).map(|n| &mut n.lists.tree)
    }

    fn bool_of(&self, h: Handle, a: Token) -> bool {
        self.node(h).is_some_and(|n| n.bool_attr(a))
    }

    /// A new tree: the view at the sheet's top left; listening to its buttons and scroll bars.
    // D2-11463, D2-11465, D2-11466
    pub(crate) fn tree_created(&mut self, h: Handle) {
        self.default_attr(h, lname::X_VIEW, Value::Int(0));
        self.default_attr(h, lname::Y_VIEW, Value::Int(0));
        for a in [
            lname::UP_BUTTON,
            lname::DOWN_BUTTON,
            attr::LEFT_BUTTON,
            attr::RIGHT_BUTTON,
            attr::VERTICAL_SCROLLBAR,
            lname::HORIZONTAL_SCROLLBAR,
        ] {
            if let Some(id) = self.node(h).and_then(|n| n.id_attr(a)) {
                self.tree_id_changed(h, a, id);
            }
        }
        let (x, y) = (
            self.int_or(h, lname::X_VIEW, 0),
            self.int_or(h, lname::Y_VIEW, 0),
        );
        if let Some(t) = self.tree_mut(h) {
            t.x_view = x;
            t.y_view = y;
        }
        self.tree_set_state(h, self.node(h).map_or(Token(0), |n| n.state));
    }

    /// The sheet's size for a grid reaching cell `(max_x, max_y)`: that many nodes plus one each
    /// way, never smaller than the tree.
    // D2-11426
    fn tree_paper_size(&mut self, h: Handle, max_x: i32, max_y: i32) {
        let nw = self.int_or(h, name::NODE_WIDTH, 0);
        let nh = self.int_or(h, name::NODE_HEIGHT, 0);
        let Some(n) = self.node_mut(h) else { return };
        let (w, hh) = (n.rect.width(), n.rect.height());
        let t = &mut n.lists.tree;
        t.node_w = nw;
        t.node_h = nh;
        t.paper_w = ((max_x + 1) * nw).max(w);
        t.paper_h = ((max_y + 1) * nh).max(hh);
    }

    /// Start a change of several nodes: the tree waits to lay itself out.
    // D2-11427
    pub fn tree_begin_changes(&mut self, h: Handle) {
        if let Some(t) = self.tree_mut(h) {
            t.multi_op = true;
        }
    }

    /// A change of several nodes done: laid out now.
    // D2-11454
    pub fn tree_changes_done(&mut self, h: Handle) {
        if let Some(t) = self.tree_mut(h) {
            t.multi_op = false;
        }
        self.tree_update_layout(h);
        self.tree_update_controls(h);
    }

    /// The view's share of the way down (and across) the sheet's hidden part.
    // D2-11428
    fn tree_percentage_v(&self, h: Handle) -> f32 {
        let Some(n) = self.node(h) else { return 0.0 };
        let t = &n.lists.tree;
        let height = n.rect.height();
        if t.paper_h <= height {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let p = t.y_view as f32 / (t.paper_h - height) as f32;
        p.clamp(0.0, 1.0)
    }

    // D2-11429
    fn tree_percentage_h(&self, h: Handle) -> f32 {
        let Some(n) = self.node(h) else { return 0.0 };
        let t = &n.lists.tree;
        let width = n.rect.width();
        if t.paper_w <= width {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let p = t.x_view as f32 / (t.paper_w - width) as f32;
        p.clamp(0.0, 1.0)
    }

    /// A scroll by node or page, as a goal (reached at once): a page is the view's length less a
    /// node; never before the start or past the last full view.
    // D2-11430, D2-11431, D2-11432, D2-11433, D2-11434, D2-11435, D2-11436, D2-11437
    fn tree_scroll(&mut self, h: Handle, s: Scroll) {
        let Some(n) = self.node(h) else { return };
        let (w, hh) = (n.rect.width(), n.rect.height());
        let t = &n.lists.tree;
        let vertical = matches!(
            s,
            Scroll::VPageDec | Scroll::VPageInc | Scroll::VLineDec | Scroll::VLineInc
        );
        let (view, step, len, paper, goal) = if vertical {
            (t.y_view, t.node_h, hh, t.paper_h, lname::GOAL_Y_VIEW)
        } else {
            (t.x_view, t.node_w, w, t.paper_w, lname::GOAL_X_VIEW)
        };
        let last = paper - len;
        let v = match s {
            Scroll::VPageDec | Scroll::HPageDec => (view + step - len).max(0),
            Scroll::VPageInc | Scroll::HPageInc => (view - step + len).min(last),
            Scroll::VLineDec | Scroll::HLineDec => (view - step).max(0),
            Scroll::VLineInc | Scroll::HLineInc => (view + step).min(last),
        };
        self.set_attr(h, goal, Value::Int(v));
    }

    /// The view as far along the sheet as it goes.
    // D2-11438
    pub fn tree_end(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let t = &n.lists.tree;
        let x = (t.paper_w - n.rect.width()).max(0);
        let y = (t.paper_h - n.rect.height()).max(0);
        self.set_attr(h, lname::X_VIEW, Value::Int(x));
        self.set_attr(h, lname::Y_VIEW, Value::Int(y));
    }

    /// The node at a cell: the first in the list there; the search stops at a node past the cell
    /// both ways.
    // D2-11440
    fn tree_node_at(&self, h: Handle, x: i32, y: i32) -> Option<usize> {
        let t = self.tree(h)?;
        for (i, n) in t.nodes.iter().enumerate() {
            if n.gridx == x && n.gridy == y {
                return Some(i);
            }
            if x < n.gridx && y < n.gridy {
                return None;
            }
        }
        None
    }

    /// The element at a cell.
    // D2-11446
    #[must_use]
    pub fn tree_item(&self, h: Handle, x: i32, y: i32) -> Option<Handle> {
        let i = self.tree_node_at(h, x, y)?;
        self.tree(h).map(|t| t.nodes[i].element)
    }

    /// Whether a cell holds a node.
    // D2-11450
    #[must_use]
    pub fn tree_is_occupied(&self, h: Handle, x: i32, y: i32) -> bool {
        self.tree_item(h, x, y).is_some()
    }

    /// Hang a node from another: each records the other.
    // D2-11191, D2-11192
    fn tree_link(&mut self, h: Handle, parent: usize, child: usize) {
        let Some(t) = self.tree_mut(h) else { return };
        let (pe, ce) = (t.nodes[parent].element, t.nodes[child].element);
        if !t.nodes[child].parents.contains(&pe) {
            t.nodes[child].parents.push(pe);
        }
        if !t.nodes[parent].children.contains(&ce) {
            t.nodes[parent].children.push(ce);
        }
    }

    /// A node placed by the layout: the nodes it names as its children (`child1` to `child3`, by
    /// their names) hang from it.
    // D2-11441
    fn tree_static_links(&mut self, h: Handle, i: usize) {
        let Some(e) = self.tree(h).map(|t| t.nodes[i].element) else {
            return;
        };
        for c in name::CHILDREN {
            let Some(id) = self.node(e).and_then(|n| n.id_attr(c)) else {
                return;
            };
            let found = self.tree(h).and_then(|t| {
                t.nodes
                    .iter()
                    .position(|n| self.node_token(n.element) == id)
            });
            if let Some(f) = found {
                self.tree_link(h, i, f);
            }
        }
    }

    /// A node on the grid: it hangs from the nodes at the cells it names as its parents
    /// (`Parent1x`/`Parent1y`, `Parent2x`/`Parent2y`).
    // D2-11442
    fn tree_grid_links(&mut self, h: Handle, i: usize) {
        let Some(e) = self.tree(h).map(|t| t.nodes[i].element) else {
            return;
        };
        for (ax, ay) in name::PARENTS {
            let (Some(px), Some(py)) = (
                self.node(e).and_then(|n| n.int_attr(ax)),
                self.node(e).and_then(|n| n.int_attr(ay)),
            ) else {
                return;
            };
            if let Some(p) = self.tree_node_at(h, px, py) {
                self.tree_link(h, p, i);
            }
        }
    }

    /// Add a node to the grid at the cell its element names (`gridx`, `gridy`), unless it names
    /// none or the cell is taken; kept in order of cells; the sheet grows to the furthest cell.
    // D2-11449
    fn tree_add_grid_node(&mut self, h: Handle, e: Handle) -> bool {
        let (Some(x), Some(y)) = (
            self.node(e).and_then(|n| n.int_attr(name::GRIDX)),
            self.node(e).and_then(|n| n.int_attr(name::GRIDY)),
        ) else {
            return false;
        };
        if self.tree_node_at(h, x, y).is_some() {
            return false;
        }
        let Some(t) = self.tree_mut(h) else {
            return false;
        };
        let mut at = t.nodes.len();
        for (i, n) in t.nodes.iter().enumerate() {
            t.max_x = t.max_x.max(n.gridx);
            t.max_y = t.max_y.max(n.gridy);
            if x < n.gridx && y < n.gridy {
                at = i;
                break;
            }
        }
        t.max_x = t.max_x.max(x);
        t.max_y = t.max_y.max(y);
        let (mx, my) = (t.max_x, t.max_y);
        t.nodes.insert(
            at,
            TreeNode {
                gridx: x,
                gridy: y,
                ..TreeNode::new(e)
            },
        );
        self.tree_paper_size(h, mx, my);
        true
    }

    /// Where a node's lines to its children start: its parent connection point (by attributes,
    /// else the middle of its leading edge: its bottom middle for a tree growing down, the top
    /// middle when it grows up, the right or left middle across), with its away distances; each
    /// line ends at the child's child connection point (the middle of the edge facing its parent).
    // D2-11451
    pub(crate) fn tree_update_layout(&mut self, h: Handle) {
        let horizontal = self.bool_of(h, name::HORIZONTAL);
        let high = self.bool_of(h, name::ORIENT_HIGH);
        let grid = self.bool_of(h, name::GRID);
        if let Some(t) = self.tree_mut(h) {
            t.lines.clear();
        }
        if grid {
            let nw = self.int_or(h, name::NODE_WIDTH, 0);
            let nh = self.int_or(h, name::NODE_HEIGHT, 0);
            let Some(t) = self.tree_mut(h) else { return };
            t.node_w = nw;
            t.node_h = nh;
            let (pw, ph, xv, yv) = (t.paper_w, t.paper_h, t.x_view, t.y_view);
            let nodes: Vec<(Handle, i32, i32)> = t
                .nodes
                .iter()
                .map(|n| (n.element, n.gridx, n.gridy))
                .collect();
            for (e, gx, gy) in nodes {
                let mut x = gx * nw;
                if high && horizontal {
                    x = pw - nw - x;
                }
                let mut y = gy * nh;
                if !high && !horizontal {
                    y = ph - nh - y;
                }
                self.place_at(e, x - xv, y - yv);
            }
        }
        let nodes = self.tree(h).map(|t| t.nodes.clone()).unwrap_or_default();
        let mut lines = Vec::new();
        for n in &nodes {
            self.node_child_lines(n, horizontal, high, &mut lines);
        }
        if let Some(t) = self.tree_mut(h) {
            t.lines = lines;
        }
    }

    /// A node's lines to each of its children.
    // D2-11187
    fn node_child_lines(&self, n: &TreeNode, horizontal: bool, high: bool, out: &mut Vec<Line>) {
        let Some(en) = self.node(n.element) else {
            return;
        };
        let r = en.rect;
        let px = en
            .int_attr(name::PARENT_CONNECT_X)
            .unwrap_or(if !horizontal {
                (r.x0 + r.x1) / 2
            } else if !high {
                r.x1
            } else {
                r.x0
            });
        let py = en
            .int_attr(name::PARENT_CONNECT_Y)
            .unwrap_or(if horizontal {
                (r.y0 + r.y1) / 2
            } else if !high {
                r.y0
            } else {
                r.y1
            });
        let ax = en.int_attr(name::AWAY_X).unwrap_or(0);
        let ay = en.int_attr(name::AWAY_Y).unwrap_or(0);
        for c in &n.children {
            let (x1, y1) = self.node_child_connection(*c, horizontal, high);
            out.push(Line {
                x0: px,
                y0: py,
                away_x: ax,
                away_y: ay,
                x1,
                y1,
            });
        }
    }

    /// Where a line to a child ends: its child connection point (by attributes, else the middle
    /// of the edge facing its parent).
    // D2-11186
    fn node_child_connection(&self, e: Handle, horizontal: bool, high: bool) -> (i32, i32) {
        let Some(en) = self.node(e) else {
            return (0, 0);
        };
        let r = en.rect;
        let x = en
            .int_attr(name::CHILD_CONNECT_X)
            .unwrap_or(if !horizontal {
                (r.x0 + r.x1) / 2
            } else if !high {
                r.x0
            } else {
                r.x1
            });
        let y = en.int_attr(name::CHILD_CONNECT_Y).unwrap_or(if horizontal {
            (r.y0 + r.y1) / 2
        } else if high {
            r.y0
        } else {
            r.y1
        });
        (x, y)
    }

    /// The segments a tree draws, in its own coordinates, and their color: each line straight,
    /// or with `straight_connect` square, out from the parent by its away distance and
    /// `straight_away_pix`, across, then into the child; none with `no_lines`.
    // D2-11445
    #[must_use]
    pub fn tree_segments(&self, h: Handle) -> (Vec<[i32; 4]>, [f32; 3]) {
        let Some(n) = self.node(h) else {
            return (Vec::new(), [0.0; 3]);
        };
        let color = [
            n.float_attr(name::LINE_COLOR_RED).unwrap_or(0.0),
            n.float_attr(name::LINE_COLOR_GREEN).unwrap_or(0.0),
            n.float_attr(name::LINE_COLOR_BLUE).unwrap_or(0.0),
        ];
        let t = &n.lists.tree;
        if t.lines.is_empty() || n.bool_attr(name::NO_LINES) {
            return (Vec::new(), color);
        }
        let straight = n.bool_attr(name::STRAIGHT_CONNECT);
        let pix = n.int_attr(name::STRAIGHT_AWAY_PIX).unwrap_or(0);
        let horizontal = n.bool_attr(name::HORIZONTAL);
        let high = n.bool_attr(name::ORIENT_HIGH);
        let mut out = Vec::new();
        for l in &t.lines {
            if !straight {
                out.push([l.x0, l.y0, l.x1, l.y1]);
                continue;
            }
            let (mx, my) = if !horizontal {
                (
                    l.away_x + l.x0,
                    if high {
                        l.away_y + l.y0 + pix
                    } else {
                        l.y0 - l.away_y - pix
                    },
                )
            } else if !high {
                (l.away_x + l.x0 + pix, l.away_y + l.y0)
            } else {
                (l.x0 - l.away_x - pix, l.away_y + l.y0)
            };
            out.push([l.x0, l.y0, mx, my]);
            let (bx, by) = if horizontal { (mx, l.y1) } else { (l.x1, my) };
            out.push([mx, my, bx, by]);
            out.push([bx, by, l.x1, l.y1]);
        }
        (out, color)
    }

    /// The tree's controls in step with its view: each scroll bar (unless it moved the view) sized
    /// to the share in view, at the view's place, disabled when all of that way shows; the arrow
    /// buttons ghosted at the sheet's ends.
    // D2-11443
    pub(crate) fn tree_update_controls(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let (w, hh) = (n.rect.width(), n.rect.height());
        let t = n.lists.tree.clone();
        if !t.from_scrollbar {
            for (bar_attr, vertical) in [
                (attr::VERTICAL_SCROLLBAR, true),
                (lname::HORIZONTAL_SCROLLBAR, false),
            ] {
                let Some(sb) = self.named_element(h, bar_attr) else {
                    continue;
                };
                let (len, paper) = if vertical {
                    (hh, t.paper_h)
                } else {
                    (w, t.paper_w)
                };
                if len < paper {
                    self.set_attr(sb, attr::ALLOW_UPDATING, Value::Bool(false));
                    self.set_attr(sb, attr::DISABLED, Value::Bool(false));
                    #[allow(clippy::cast_precision_loss)]
                    let share = (len as f32 / paper as f32).min(1.0);
                    let (size, pos, at) = if vertical {
                        (
                            attr::Y_WIDGET_SIZE,
                            attr::Y_POSITION,
                            self.tree_percentage_v(h),
                        )
                    } else {
                        (
                            attr::X_WIDGET_SIZE,
                            attr::X_POSITION,
                            self.tree_percentage_h(h),
                        )
                    };
                    self.set_attr(sb, size, Value::Float(share));
                    self.set_attr(sb, pos, Value::Float(at));
                    self.set_attr(sb, attr::ALLOW_UPDATING, Value::Bool(true));
                    self.lay_out_scrollbar(sb);
                } else {
                    self.set_attr(sb, attr::DISABLED, Value::Bool(true));
                }
            }
        }
        self.listbox_ghost_button(h, lname::UP_BUTTON, t.y_view == 0);
        self.listbox_ghost_button(h, lname::DOWN_BUTTON, t.paper_h <= hh + t.y_view);
        self.listbox_ghost_button(h, attr::LEFT_BUTTON, t.x_view == 0);
        self.listbox_ghost_button(h, attr::RIGHT_BUTTON, t.paper_w <= t.x_view + w);
    }

    /// What the tree's buttons and scroll bars say: an arrow's hot click scrolls a node (with
    /// `unique_msg`, only an arrow in the tree's window); a scroll bar's new position moves the
    /// view to that share of the hidden part; its lines and pages scroll.
    // D2-11444
    pub(crate) fn tree_hear(&mut self, h: Handle, from: Handle, what: Token, param: u32) {
        let unique = self.bool_of(h, lname::UNIQUE_MSG);
        let sender = self.node(from).map_or(0, |n| n.id.raw());
        if what == msg::HOT_CLICK && (!unique || self.root_of(from) == self.root_of(h)) {
            for (a, s) in [
                (lname::UP_BUTTON, Scroll::VLineDec),
                (lname::DOWN_BUTTON, Scroll::VLineInc),
                (attr::LEFT_BUTTON, Scroll::HLineDec),
                (attr::RIGHT_BUTTON, Scroll::HLineInc),
            ] {
                if self.node(h).and_then(|n| n.id_attr(a)) == Some(sender) {
                    self.tree_scroll(h, s);
                }
            }
        }
        let mine = !unique || param == self.root_of(h).index();
        for (bar_attr, vertical) in [
            (attr::VERTICAL_SCROLLBAR, true),
            (lname::HORIZONTAL_SCROLLBAR, false),
        ] {
            if self.node(h).and_then(|n| n.id_attr(bar_attr)) != Some(sender) || !mine {
                continue;
            }
            if what == msg::NEW_POSITION {
                let (pos, view) = if vertical {
                    (attr::Y_POSITION, lname::Y_VIEW)
                } else {
                    (attr::X_POSITION, lname::X_VIEW)
                };
                let p = self.float_or(from, pos, 0.0);
                let Some(n) = self.node(h) else { return };
                let t = &n.lists.tree;
                let hidden = if vertical {
                    t.paper_h - n.rect.height()
                } else {
                    t.paper_w - n.rect.width()
                };
                if let Some(t) = self.tree_mut(h) {
                    t.from_scrollbar = true;
                }
                #[allow(clippy::cast_precision_loss)]
                let v = dereth_primitives::num::to_i32(p * hidden as f32);
                self.set_attr(h, view, Value::Int(v));
                if let Some(t) = self.tree_mut(h) {
                    t.from_scrollbar = false;
                }
            } else if let Some(s) = Scroll::of_message(what) {
                let along = matches!(
                    s,
                    Scroll::VPageDec | Scroll::VPageInc | Scroll::VLineDec | Scroll::VLineInc
                );
                if along == vertical {
                    self.tree_scroll(h, s);
                }
            }
        }
    }

    /// After an attribute naming another element changed: arrow buttons are listened to for
    /// their hot clicks, scroll bars for their positions, lines and pages.
    // D2-11448
    pub(crate) fn tree_id_changed(&mut self, h: Handle, a: Token, id: u32) {
        if a == lname::UP_BUTTON
            || a == lname::DOWN_BUTTON
            || a == attr::LEFT_BUTTON
            || a == attr::RIGHT_BUTTON
        {
            self.register_for_message(h, Token(id), msg::HOT_CLICK);
        } else if a == lname::HORIZONTAL_SCROLLBAR {
            for m in SCROLLBAR_MESSAGES_H {
                self.register_for_message(h, Token(id), m);
            }
        } else if a == attr::VERTICAL_SCROLLBAR {
            for m in SCROLLBAR_MESSAGES_V {
                self.register_for_message(h, Token(id), m);
            }
        } else {
            return;
        }
        self.tree_update_controls(h);
    }

    /// Select a node's element (none when it is not one of the tree's own): with
    /// `selected_item_state_change`, the one before takes the normal state and the new one the
    /// selected state; with `notify`, the tree says so (with the element's name); then laid out.
    // D2-11452
    pub fn tree_select(&mut self, h: Handle, item: Option<Handle>, notify: bool) {
        let before = self.tree(h).and_then(|t| t.selected);
        if before == item {
            return;
        }
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        let now = item.filter(|i| kids.contains(i));
        if let Some(t) = self.tree_mut(h) {
            t.selected = now;
        }
        if self.bool_of(h, attr::SELECTED_ITEM_STATE_CHANGE) {
            let (normal, selected) = self.node(h).map_or((None, None), |n| {
                (
                    n.id_attr(attr::ITEM_NORMAL_STATE),
                    n.id_attr(attr::ITEM_SELECTED_STATE),
                )
            });
            if let Some(b) = before {
                self.set_state(b, Token(normal.unwrap_or(0)));
            }
            if let Some(a) = now {
                self.set_state(a, Token(selected.unwrap_or(0)));
            }
            self.flush_pending();
        }
        if notify {
            let id = now.and_then(|i| self.node(i)).map_or(0, |n| n.id.raw());
            self.broadcast(h, msg::NEW_SELECTION, id);
        }
        self.tree_update_layout(h);
    }

    /// The selected element.
    #[must_use]
    pub fn tree_selected(&self, h: Handle) -> Option<Handle> {
        self.tree(h).and_then(|t| t.selected)
    }

    /// Hang `item` from the node at a cell.
    // D2-11453
    pub fn tree_set_parent(&mut self, h: Handle, item: Handle, x: i32, y: i32) -> bool {
        let Some(c) = self
            .tree(h)
            .and_then(|t| t.nodes.iter().position(|n| n.element == item))
        else {
            return false;
        };
        let Some(p) = self.tree_node_at(h, x, y) else {
            return false;
        };
        self.tree_link(h, p, c);
        if !self.tree(h).is_some_and(|t| t.multi_op) {
            self.tree_update_layout(h);
        }
        true
    }

    /// Add an element as a node: put inside the tree; on a grid tree at the cell given (when one
    /// is), else first; every node's links worked out again; then laid out.
    // D2-11455
    pub fn tree_add(&mut self, h: Handle, item: Handle, x: i32, y: i32) -> bool {
        let grid = self.bool_of(h, name::GRID);
        self.set_parent(item, h);
        self.place(item);
        if grid && x != -1 && y != -1 {
            self.set_attr(item, name::GRIDX, Value::Int(x));
            self.set_attr(item, name::GRIDY, Value::Int(y));
        }
        if grid {
            self.tree_add_grid_node(h, item);
        } else if let Some(t) = self.tree_mut(h) {
            t.nodes.insert(0, TreeNode::new(item));
        }
        self.tree_relink(h, grid);
        if !self.tree(h).is_some_and(|t| t.multi_op) {
            self.tree_update_layout(h);
            self.tree_update_controls(h);
        }
        true
    }

    fn tree_relink(&mut self, h: Handle, grid: bool) {
        let count = self.tree(h).map_or(0, |t| t.nodes.len());
        for i in 0..count {
            if grid {
                self.tree_grid_links(h, i);
            } else {
                self.tree_static_links(h, i);
            }
        }
    }

    /// A tree going away lets its nodes go and stops listening to its buttons and scroll bars.
    // D2-11458, D2-11464, D2-11457, D2-11190, D2-11188
    pub(crate) fn tree_deleting(&mut self, h: Handle) {
        if let Some(t) = self.tree_mut(h) {
            t.nodes.clear();
            t.lines.clear();
        }
        for a in [
            lname::UP_BUTTON,
            lname::DOWN_BUTTON,
            attr::LEFT_BUTTON,
            attr::RIGHT_BUTTON,
        ] {
            if let Some(id) = self.node(h).and_then(|n| n.id_attr(a)) {
                self.unregister_for_message(h, Token(id), msg::HOT_CLICK);
            }
        }
        for (a, messages) in [
            (lname::HORIZONTAL_SCROLLBAR, SCROLLBAR_MESSAGES_H),
            (attr::VERTICAL_SCROLLBAR, SCROLLBAR_MESSAGES_V),
        ] {
            if let Some(id) = self.node(h).and_then(|n| n.id_attr(a)) {
                for m in messages {
                    self.unregister_for_message(h, Token(id), m);
                }
            }
        }
    }

    /// Empty the tree: every node and its element gone, the view at the top left.
    // D2-11456
    pub fn tree_flush(&mut self, h: Handle) {
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        if let Some(t) = self.tree_mut(h) {
            t.nodes.clear();
            t.selected = None;
            t.max_x = 0;
            t.max_y = 0;
        }
        for k in kids {
            self.remove(k);
        }
        self.set_attr(h, lname::X_VIEW, Value::Int(0));
        self.set_attr(h, lname::Y_VIEW, Value::Int(0));
        self.tree_update_layout(h);
        self.tree_update_controls(h);
    }

    /// After a yes-or-no attribute changed: a new direction lays the tree out.
    // D2-11459
    pub(crate) fn tree_bool_changed(&mut self, h: Handle, a: Token) {
        if a == name::HORIZONTAL || a == name::ORIENT_HIGH {
            self.tree_update_layout(h);
            self.tree_update_controls(h);
        }
    }

    /// After a whole number changed: a new node size lays the tree out; a new view is applied; a
    /// goal view is reached at once.
    // D2-11460
    pub(crate) fn tree_int_set(&mut self, h: Handle, a: Token, v: i32, changed: bool) {
        if !changed {
            return;
        }
        if a == name::NODE_HEIGHT || a == name::NODE_WIDTH {
            self.tree_update_layout(h);
            self.tree_update_controls(h);
        }
        if a == lname::X_VIEW || a == lname::Y_VIEW {
            if let Some(t) = self.tree_mut(h) {
                if a == lname::X_VIEW {
                    t.x_view = v;
                } else {
                    t.y_view = v;
                }
            }
            self.tree_update_layout(h);
            self.tree_update_controls(h);
        }
        if a == lname::GOAL_X_VIEW {
            self.set_attr(h, lname::X_VIEW, Value::Int(v));
        } else if a == lname::GOAL_Y_VIEW {
            self.set_attr(h, lname::Y_VIEW, Value::Int(v));
        }
    }

    /// A press on the tree: it says so; the left button selects the child under the pointer; the
    /// wheel goes to the tree's scroll bar when the pointer is not on it.
    // D2-11461
    pub(crate) fn tree_mouse_down(
        &mut self,
        h: Handle,
        x: i32,
        y: i32,
        b: u32,
        handled: &mut bool,
    ) {
        self.element_mouse_down(h, x, y, b, handled);
        if self.node(h).is_some_and(|n| n.moving || n.resizing) {
            return;
        }
        self.broadcast(h, name::MOUSE_PRESS, b);
        if (b == button_id::LEFT || b == button_id::DOUBLE_LEFT)
            && self.node(h).is_some_and(|n| n.over)
        {
            let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
            if let Some(k) = kids
                .into_iter()
                .find(|k| self.node(*k).is_some_and(|n| n.over))
            {
                self.tree_select(h, Some(k), true);
            }
        }
        if b != button_id::WHEEL_UP && b != button_id::WHEEL_DOWN {
            return;
        }
        let bar = if self
            .node(h)
            .and_then(|n| n.id_attr(attr::VERTICAL_SCROLLBAR))
            .is_some()
        {
            self.named_element(h, attr::VERTICAL_SCROLLBAR)
        } else {
            self.named_element(h, lname::HORIZONTAL_SCROLLBAR)
        };
        if let Some(sb) = bar {
            if !self.node(sb).is_some_and(|n| n.over) {
                let mut handled = false;
                self.mouse_down(sb, x, y, b, &mut handled);
            }
        }
    }

    /// A new state: the tree's nodes are its children again (on a grid by their cells, else in
    /// the reverse of their order), their links worked out again, then laid out.
    // D2-11462
    pub(crate) fn tree_set_state(&mut self, h: Handle, s: Token) -> bool {
        let r = self.set_state_base(h, s);
        let grid = self.bool_of(h, name::GRID);
        if let Some(t) = self.tree_mut(h) {
            t.nodes.clear();
        }
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        for k in kids {
            if grid {
                self.tree_add_grid_node(h, k);
            } else if let Some(t) = self.tree_mut(h) {
                t.nodes.insert(0, TreeNode::new(k));
            }
        }
        self.tree_relink(h, grid);
        self.tree_update_layout(h);
        self.tree_update_controls(h);
        r
    }
}
