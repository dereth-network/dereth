//! Binary space partition trees: the physics part of meshes and cell meshes, and the cells' own
//! trees.
//!
//! A tree is stored as its nodes in depth-first order, front child before back child. Every node
//! starts with a four-character tag word saying what it is and which children follow:
//!
//! | tag word | what |
//! |---|---|
//! | `BP-N` (`0x42502D4E`) | a split with both children |
//! | `BP--` (`0x42502D2D`) | a split with only its front child |
//! | `B--N` (`0x422D2D4E`) | a split with only its back child |
//! | `BPFL` (`0x4250464C`) | a split with no children |
//! | `LEAF` (`0x4C454146`) | a leaf of a physics tree; in a cell tree, a split with no children |
//!
//! A split stores its plane, then its children, then (physics trees only) its bounding sphere. A
//! physics leaf stores a solid flag, its bounding sphere, a counted list of 16-bit polygon indices
//! and alignment.

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use auberean_primitives::{Sphere, Vec3};

/// The tag word of a split with both children.
pub const BOTH: u32 = 0x4250_2D4E;
/// The tag word of a split with only its front child.
pub const FRONT_ONLY: u32 = 0x4250_2D2D;
/// The tag word of a split with only its back child.
pub const BACK_ONLY: u32 = 0x422D_2D4E;
/// The tag word of a split with no children.
pub const NO_CHILDREN: u32 = 0x4250_464C;
/// The tag word of a leaf.
pub const LEAF: u32 = 0x4C45_4146;

/// How deep a stored tree may go before it is taken for a broken one.
const MAX_DEPTH: usize = 4096;

/// Which kind of tree a stream holds: the kind decides what a node stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeKind {
    /// Collision: splits carry bounding spheres, leaves carry polygons.
    Physics,
    /// A cell's inside test: splits only.
    Cell,
}

/// A plane: its normal and its distance term (`normal . p + distance = 0` on the plane).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,
}

impl Plane {
    // D2-13135
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            normal: r.vec3()?,
            distance: r.f32()?,
        })
    }
}

/// One node of a tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A physics tree's leaf: whether it is solid, its bounds, and the polygons in it (indices into
    /// the mesh's polygons the tree was built over).
    Leaf {
        solid: u32,
        sphere: Sphere,
        polygons: Vec<u16>,
    },
    /// A split by a plane, with its children; physics splits also carry their bounds.
    Split {
        tag: u32,
        plane: Plane,
        front: Option<Box<Node>>,
        back: Option<Box<Node>>,
        sphere: Option<Sphere>,
    },
}

/// A whole tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub kind: TreeKind,
    pub root: Node,
}

impl Tree {
    /// A tree of the given kind: its nodes from the root, depth first.
    // D2-04121
    pub fn read(r: &mut Reader<'_>, kind: TreeKind) -> Result<Self> {
        Ok(Self {
            kind,
            root: read_node(r, kind, 0)?,
        })
    }

    /// How many nodes the tree has.
    #[must_use]
    pub fn node_count(&self) -> usize {
        fn count(n: &Node) -> usize {
            match n {
                Node::Leaf { .. } => 1,
                Node::Split { front, back, .. } => {
                    1 + front.as_deref().map_or(0, count) + back.as_deref().map_or(0, count)
                }
            }
        }
        count(&self.root)
    }
}

// D2-04134, D2-04146, D2-04160
fn read_node(r: &mut Reader<'_>, kind: TreeKind, depth: usize) -> Result<Node> {
    r.ensure(depth < MAX_DEPTH, || {
        format!("tree deeper than {MAX_DEPTH}")
    })?;
    let at = r.pos();
    let tag = r.u32()?;
    if tag == LEAF && kind == TreeKind::Physics {
        let solid = r.u32()?;
        let sphere = r.sphere()?;
        let n = r.count(2)?;
        let mut polygons = Vec::with_capacity(n);
        for _ in 0..n {
            polygons.push(r.u16()?);
        }
        r.align()?;
        return Ok(Node::Leaf {
            solid,
            sphere,
            polygons,
        });
    }
    let plane = Plane::read(r)?;
    let (has_front, has_back) = match tag {
        BOTH => (true, true),
        FRONT_ONLY => (true, false),
        BACK_ONLY => (false, true),
        NO_CHILDREN | LEAF => (false, false),
        _ => {
            return Err(auberean_dat::DecodeError {
                at,
                what: format!("tree node tag {tag:#010x}"),
            });
        }
    };
    let front = if has_front {
        Some(Box::new(read_node(r, kind, depth + 1)?))
    } else {
        None
    };
    let back = if has_back {
        Some(Box::new(read_node(r, kind, depth + 1)?))
    } else {
        None
    };
    let sphere = if kind == TreeKind::Physics {
        Some(r.sphere()?)
    } else {
        None
    };
    Ok(Node::Split {
        tag,
        plane,
        front,
        back,
        sphere,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tree stream mechanics on a synthetic tree)
    use super::*;

    fn words(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn sphere() -> Vec<u32> {
        vec![0, 0, 0, 1.0f32.to_bits()]
    }

    #[test]
    fn a_physics_split_reads_its_plane_children_then_sphere() {
        let mut b = words(&[BOTH, 0, 0, 1.0f32.to_bits(), 0]);
        b.extend(words(&[LEAF, 1]));
        b.extend(words(&sphere()));
        b.extend(words(&[3]));
        b.extend([7, 0, 8, 0, 9, 0, 0, 0]);
        b.extend(words(&[LEAF, 0]));
        b.extend(words(&sphere()));
        b.extend(words(&[0]));
        b.extend(words(&sphere()));
        let mut r = Reader::new(&b);
        let t = Tree::read(&mut r, TreeKind::Physics).unwrap();
        r.finish().unwrap();
        assert_eq!(t.node_count(), 3);
        let Node::Split {
            front: Some(f),
            sphere: Some(_),
            ..
        } = &t.root
        else {
            panic!("{t:?}")
        };
        assert_eq!(
            **f,
            Node::Leaf {
                solid: 1,
                sphere: Sphere {
                    radius: 1.0,
                    ..Sphere::default()
                },
                polygons: vec![7, 8, 9]
            }
        );
    }

    #[test]
    fn in_a_cell_tree_a_leaf_tag_is_a_split_with_no_children() {
        let mut b = words(&[FRONT_ONLY, 0, 0, 1.0f32.to_bits(), 0]);
        b.extend(words(&[LEAF, 1.0f32.to_bits(), 0, 0, 0]));
        let mut r = Reader::new(&b);
        let t = Tree::read(&mut r, TreeKind::Cell).unwrap();
        r.finish().unwrap();
        assert_eq!(t.node_count(), 2);
    }

    #[test]
    fn an_unknown_tag_fails_where_it_was_read() {
        let b = words(&[0x1234_5678, 0, 0, 0, 0]);
        let e = Tree::read(&mut Reader::new(&b), TreeKind::Cell).unwrap_err();
        assert_eq!(e.at, 0);
    }
}
