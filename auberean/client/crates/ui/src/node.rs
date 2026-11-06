//! Tree nodes: elements joined as parents and children (a tree element's branches), and the
//! lines that join each node to its children.
//!
//! A node joins its parents at its "child connect" point and its children at its "parent
//! connect" point. Without an attribute saying where, a point is the middle of one of the
//! element's sides: laid out across (`across`), the right or left side's middle; laid out down,
//! the top or bottom side's middle; `flipped` chooses the other side.

use crate::system::{Handle, Ui};
use crate::Token;

mod name {
    use crate::Token;
    pub const CHILD_CONNECT_X: Token = Token::of("node_child_connect_x");
    pub const CHILD_CONNECT_Y: Token = Token::of("node_child_connect_y");
    pub const PARENT_CONNECT_X: Token = Token::of("node_parent_connect_x");
    pub const PARENT_CONNECT_Y: Token = Token::of("node_parent_connect_y");
    pub const AWAY_X: Token = Token::of("node_away_x");
    pub const AWAY_Y: Token = Token::of("node_away_y");
}

/// A node in a [`Nodes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// One node: its element, its parents and its children, in the order they were joined.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeNode {
    pub element: Option<Handle>,
    pub parents: Vec<NodeId>,
    pub children: Vec<NodeId>,
}

/// A line from a parent's point to a child's, with how far it first goes out from the parent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub x0: i32,
    pub y0: i32,
    pub away_x: i32,
    pub away_y: i32,
    pub x1: i32,
    pub y1: i32,
}

/// Nodes and how they are joined.
#[derive(Clone, Debug, Default)]
pub struct Nodes {
    nodes: Vec<Option<TreeNode>>,
}

impl Nodes {
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&TreeNode> {
        self.nodes.get(id.0)?.as_ref()
    }

    /// A node for an element, joined to nothing.
    // D2-11189
    pub fn add(&mut self, element: Option<Handle>) -> NodeId {
        self.nodes.push(Some(TreeNode {
            element,
            parents: Vec::new(),
            children: Vec::new(),
        }));
        NodeId(self.nodes.len() - 1)
    }

    /// Take a node away: it leaves each parent's children and each child's parents.
    // D2-11190
    pub fn remove(&mut self, id: NodeId) {
        let Some(n) = self.nodes.get_mut(id.0).and_then(Option::take) else {
            return;
        };
        for p in n.parents {
            if let Some(Some(pn)) = self.nodes.get_mut(p.0) {
                if let Some(i) = pn.children.iter().rposition(|c| *c == id) {
                    pn.children.remove(i);
                }
            }
        }
        for c in n.children {
            if let Some(Some(cn)) = self.nodes.get_mut(c.0) {
                if let Some(i) = cn.parents.iter().rposition(|p| *p == id) {
                    cn.parents.remove(i);
                }
            }
        }
    }

    /// Join a parent to a node (the node's side).
    // D2-11191
    pub fn add_parent(&mut self, id: NodeId, parent: NodeId) {
        if let Some(Some(n)) = self.nodes.get_mut(id.0) {
            n.parents.push(parent);
        }
    }

    /// Join a child to a node (the node's side).
    // D2-11192
    pub fn add_child(&mut self, id: NodeId, child: NodeId) {
        if let Some(Some(n)) = self.nodes.get_mut(id.0) {
            n.children.push(child);
        }
    }

    /// The node's element's name (none without an element).
    // D2-11185
    #[must_use]
    pub fn token(&self, ui: &Ui, id: NodeId) -> Token {
        self.get(id)
            .and_then(|n| n.element)
            .and_then(|e| ui.node(e))
            .map_or(Token(0), crate::system::Node::id)
    }

    /// Where a node joins its parents: its `node_child_connect_x` and `_y`, else across the
    /// middle of its left side (its right side when flipped) and down the middle of its bottom
    /// side (its top side when flipped).
    // D2-11186
    #[must_use]
    pub fn connection(
        &self,
        ui: &Ui,
        id: NodeId,
        across: bool,
        flipped: bool,
    ) -> Option<(i32, i32)> {
        let e = ui.node(self.get(id)?.element?)?;
        let r = e.rect;
        let x = e.int_attr(name::CHILD_CONNECT_X).unwrap_or(if !across {
            (r.x1 + r.x0 * 2 - r.x0) / 2
        } else if !flipped {
            r.x0
        } else {
            r.x1 - r.x0 + r.x0
        });
        let y = e.int_attr(name::CHILD_CONNECT_Y).unwrap_or(if across {
            (r.y0 * 2 - r.y0 + r.y1) / 2
        } else if flipped {
            r.y0
        } else {
            r.y1
        });
        Some((x, y))
    }

    /// The lines from a node to each of its children: from its `node_parent_connect_x` and `_y`
    /// (else the middle of its right side, its left when flipped, across; the middle of its top
    /// side, its bottom when flipped, down), going out first by its `node_away_x` and `_y`, to
    /// each child's point.
    #[must_use]
    pub fn child_lines(&self, ui: &Ui, id: NodeId, across: bool, flipped: bool) -> Vec<Line> {
        let Some(n) = self.get(id) else {
            return Vec::new();
        };
        let Some(e) = n.element.and_then(|h| ui.node(h)) else {
            return Vec::new();
        };
        let r = e.rect;
        let x0 = e.int_attr(name::PARENT_CONNECT_X).unwrap_or(if !across {
            (r.x1 + r.x0 * 2 - r.x0) / 2
        } else if !flipped {
            r.x1 - r.x0 + r.x0
        } else {
            r.x0
        });
        let y0 = e.int_attr(name::PARENT_CONNECT_Y).unwrap_or(if !across {
            if flipped {
                r.y1
            } else {
                r.y0
            }
        } else {
            (r.y0 * 2 - r.y0 + r.y1) / 2
        });
        let away_x = e.int_attr(name::AWAY_X).unwrap_or(0);
        let away_y = e.int_attr(name::AWAY_Y).unwrap_or(0);
        n.children
            .iter()
            .map(|c| {
                let (x1, y1) = self.connection(ui, *c, across, flipped).unwrap_or((0, 0));
                Line {
                    x0,
                    y0,
                    away_x,
                    away_y,
                    x1,
                    y1,
                }
            })
            .collect()
    }
}
