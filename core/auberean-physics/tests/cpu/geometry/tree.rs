use dereth_primitives::Vec3;

use super::tree_cases::CASES;
use super::{bits, check};
use auberean_physics::geom::tree::{ObjectTree, TreeSquare};
use auberean_physics::geom::BBox;

fn walk(node: &TreeSquare<u32>, out: &mut Vec<u32>) {
    out.push(u32::try_from(node.objects.len()).unwrap_or(0));
    out.extend(node.objects.iter().map(|o| o.0));
    let mask = (0..4).fold(0, |m, q| m | (u32::from(node.children[q].is_some()) << q));
    out.push(mask);
    for c in node.children.iter().flatten() {
        walk(c, out);
    }
}

/// An object tree built from objects' boxes answers as the September 2013 client's own code does:
/// the root square's middle and box to the bit, and the same objects held in the same squares.
///
/// Behaviour: geometry.tree.object-trees-split-as-the-client-splits-them
#[test]
fn object_trees_split_as_the_client_splits_them() {
    check(CASES, |label, x, inp| {
        if label != "generate" {
            return None;
        }
        let k = inp[0] as usize;
        let mut t = ObjectTree::default();
        t.initialize(k, x[1]);
        for (i, id) in (101..).take(k).enumerate() {
            let a = 2 + 6 * i;
            let b = BBox::new(
                Vec3::new(x[a], x[a + 1], x[a + 2]),
                Vec3::new(x[a + 3], x[a + 4], x[a + 5]),
            );
            t.add_object(id, &b);
        }
        t.generate();
        let b = &t.bounds;
        let mut out = bits(&[
            t.center.0, t.center.1, b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z,
        ]);
        walk(t.root.as_deref()?, &mut out);
        Some(out)
    });
}
