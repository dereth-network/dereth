//! The tree of squares the client sorts objects into across the ground by their boxes: a square over
//! all the boxes, split into quarters while a square holds more than one object and is larger than
//! the smallest size allowed; an object whose box straddles a square's middle stays in that square.

use dereth_primitives::Vec3;

use super::{BBox, BoxOps};

fn w(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn n(x: f64) -> f32 {
    x as f32
}

/// A square of the tree: the objects straddling its middle (or all of them, when it is not
/// split), and its four quarters (bit 0 east, bit 1 north). Dropped, the quarters below it go too.
// D2-03778, D2-03785, D2-03766
#[derive(Clone, Debug, PartialEq)]
pub struct TreeSquare<T> {
    pub objects: Vec<(T, BBox)>,
    pub children: [Option<Box<TreeSquare<T>>>; 4],
}

impl<T> Default for TreeSquare<T> {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            children: [None, None, None, None],
        }
    }
}

/// Objects sorted by their boxes across the ground. Dropped, its squares go too.
// D2-03774
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectTree<T> {
    /// The objects added, with their boxes.
    pub objects: Vec<(T, BBox)>,
    pub root: Option<Box<TreeSquare<T>>>,
    /// The box around every object added (squared off by [`ObjectTree::generate`]).
    pub bounds: BBox,
    /// The middle of the root square.
    pub center: (f32, f32),
    /// The smallest square that is split.
    pub min_size: f32,
}

impl<T: Copy> Default for ObjectTree<T> {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
            root: None,
            bounds: BBox::for_adjustment(),
            center: (0.0, 0.0),
            min_size: 0.0,
        }
    }
}

/// Which quarter of the square centered on `c` a box falls in (bit 0 east, bit 1 north), or none
/// when it straddles the middle on either axis. A box touching the middle line from the east or
/// north counts as west or south.
// D2-03768
#[must_use]
pub fn child_quad(c: (f32, f32), b: &BBox) -> Option<usize> {
    let mut q = 0;
    let mut straddles = false;
    if c.0 <= b.min.x || b.max.x <= c.0 {
        if c.0 < b.min.x && c.0 < b.max.x {
            q = 1;
        }
    } else {
        straddles = true;
    }
    if c.1 <= b.min.y || b.max.y <= c.1 {
        if c.1 < b.min.y && c.1 < b.max.y {
            q |= 2;
        }
        if !straddles {
            return Some(q);
        }
    }
    None
}

/// The middle of quarter `q` of the square of side `size` centered on `c`.
// D2-03765
#[must_use]
pub fn child_center(c: (f32, f32), size: f32, q: usize) -> (f32, f32) {
    let half = n(w(size) * 0.5);
    let mut x = n(w(c.0) - w(half));
    let mut y = n(w(c.1) - w(half));
    if q & 1 != 0 {
        x = n(w(x) + w(half));
    }
    if q & 2 != 0 {
        y = n(w(y) + w(half));
    }
    let quarter = w(half) * 0.5;
    (n(quarter + w(x)), n(quarter + w(y)))
}

impl<T: Copy> ObjectTree<T> {
    /// Empties the tree: no objects, no squares, a box ready to grow.
    // D2-03773
    pub fn reset(&mut self) {
        self.root = None;
        self.bounds = BBox::for_adjustment();
        self.objects.clear();
    }

    /// Empties the tree, sets the smallest square that is split, and makes room for `n` objects.
    // D2-03786
    pub fn initialize(&mut self, count: usize, min_size: f32) {
        self.reset();
        self.min_size = min_size;
        self.objects.reserve(count);
    }

    /// Adds an object with its box, growing the box around them all.
    // D2-03790
    pub fn add_object(&mut self, object: T, b: &BBox) {
        self.objects.push((object, *b));
        b.build_bounding_box(&mut self.bounds);
    }

    /// Builds the squares: the root square from the box's south-west corner, as wide as the box's
    /// longer side across the ground (none for no objects: a square of no size at the origin).
    // D2-03791
    pub fn generate(&mut self) {
        self.root = None;
        let size = if self.objects.is_empty() {
            self.bounds = BBox::new(Vec3::ZERO, Vec3::ZERO);
            self.center = (0.0, 0.0);
            0.0
        } else {
            let b = &mut self.bounds;
            let dx = b.max.x - b.min.x;
            let dy = b.max.y - b.min.y;
            let size = if dy <= dx { dx } else { dy };
            b.max.x = size + b.min.x;
            b.max.y = size + b.min.y;
            let c = b.center();
            self.center = (c.x, c.y);
            size
        };
        let mut root = Box::default();
        let objects = self.objects.clone();
        self.split(&mut root, self.center, size, objects);
        self.root = Some(root);
    }

    /// Fills `node`, a square of side `size` centered on `c`, with `objects`: split into
    /// quarters while it holds more than one and is larger than the smallest size.
    // D2-03788
    fn split(&self, node: &mut TreeSquare<T>, c: (f32, f32), size: f32, objects: Vec<(T, BBox)>) {
        node.children = [None, None, None, None];
        if objects.is_empty() {
            return;
        }
        let larger = (size < self.min_size) == (size == self.min_size);
        if !(larger && objects.len() > 1) {
            node.objects = objects;
            return;
        }
        let half = n(w(size) * 0.5);
        let mut quarters: [Vec<(T, BBox)>; 4] = Default::default();
        for o in objects {
            match child_quad(c, &o.1) {
                Some(q) => quarters[q].push(o),
                None => node.objects.push(o),
            }
        }
        for (q, list) in quarters.into_iter().enumerate() {
            if list.is_empty() {
                continue;
            }
            let mut child = Box::default();
            self.split(&mut child, child_center(c, size, q), half, list);
            node.children[q] = Some(child);
        }
    }
}
