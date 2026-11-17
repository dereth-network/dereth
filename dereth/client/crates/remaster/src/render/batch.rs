//! The G-buffer's buckets as instanced draws, for a device whose draws take no count from a buffer
//! (Metal's, through its Vulkan layer). There the device does not list the blocks' parts, so every
//! item the walk keeps would be a draw of its own, and none would be left out for standing outside
//! the view. Here the items outside the view are left out, and the rest are grouped by the indices
//! they draw (one mesh's part, whatever instance draws it): one indexed draw a group, its instances
//! the group's items, each read by the vertex shader from a table in the order of the draws.
//!
//! The water's mirrored view has lists of its own, cut by its own view: what stands behind the eye
//! can be in the water in front of it.

use glam::{Mat4, Vec3, Vec4};

use super::scene::{Bucket, ItemDraw, CULLED_BUCKETS, GROUP_BUCKETS};

/// The words of one indexed indirect draw command on a Vulkan device.
pub const COMMAND_WORDS: usize = 5;

/// How far past its sphere an item may be drawn, meters: the lift onto the drawn ground and the
/// wind's sway.
const SLACK: f32 = 4.0;

/// One view's draws: the commands, the table each instance's item is read from, and each bucket's
/// first command and how many.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Batch {
    pub commands: Vec<u32>,
    pub remap: Vec<u32>,
    pub runs: [(u32, u32); CULLED_BUCKETS as usize],
}

impl Batch {
    /// How many draws it makes.
    #[must_use]
    pub fn draws(&self) -> usize {
        self.commands.len() / COMMAND_WORDS
    }

    /// `other` after this, in the same two buffers: its commands' first instances and its runs'
    /// first commands moved past this one's.
    #[must_use]
    pub fn then(mut self, other: &Self) -> (Self, [(u32, u32); CULLED_BUCKETS as usize]) {
        let first = u32::try_from(self.draws()).unwrap_or(u32::MAX);
        let remap_first = u32::try_from(self.remap.len()).unwrap_or(u32::MAX);
        for c in other.commands.chunks(COMMAND_WORDS) {
            self.commands
                .extend([c[0], c[1], c[2], c[3], c[4] + remap_first]);
        }
        self.remap.extend(&other.remap);
        let runs = other.runs.map(|(at, n)| (at + first, n));
        (self, runs)
    }
}

/// A sphere in the frame's space.
pub type Sphere = (Vec3, f32);

/// Each item of a block part's sphere in the frame's space, by the item's index; `None` for an
/// item that is no block part's, which no view leaves out. `blocks` gives each block's place, in
/// blocks from the frame's origin, and its parts: their spheres in their own space, their
/// transforms in the block's space and the items of each of their levels.
pub fn part_spheres<'a>(
    items: usize,
    blocks: impl IntoIterator<Item = ((i32, i32), &'a [super::scene::PartItems])>,
) -> Vec<Option<Sphere>> {
    let mut out = vec![None; items];
    for ((x, y), parts) in blocks {
        #[allow(clippy::cast_precision_loss)] // a few hundred blocks
        let offset = Vec3::new(x as f32 * 192.0, y as f32 * 192.0, 0.0);
        for part in parts {
            let sphere = sphere_of(part.transform, part.sphere, offset);
            for level in &part.levels {
                for item in level.clone() {
                    if let Some(s) = out.get_mut(item as usize) {
                        *s = Some(sphere);
                    }
                }
            }
        }
    }
    out
}

/// A part's sphere `(center, radius)` in its own space, through `transform` and moved by
/// `offset`, its radius grown by the transform's largest scale and [`SLACK`].
fn sphere_of(transform: Mat4, (center, radius): ([f32; 3], f32), offset: Vec3) -> Sphere {
    let scale = transform
        .x_axis
        .truncate()
        .length()
        .max(transform.y_axis.truncate().length())
        .max(transform.z_axis.truncate().length());
    (
        transform.transform_point3(Vec3::from(center)) + offset,
        radius * scale + SLACK,
    )
}

/// Whether a sphere is wholly outside one of a view's planes, as `shadow::frustum` makes them.
fn outside(planes: &[Vec4; 6], (center, radius): Sphere) -> bool {
    planes
        .iter()
        .any(|p| p.truncate().dot(center) + p.w < -radius)
}

/// One view's draws of the walk's `visible` items, the interior's from `split` on: the items
/// `planes` leave out dropped, the rest grouped in their buckets by the indices they draw, the
/// groups in the order of their indices and each group's items in the walk's order, so a frame
/// that keeps the same items draws them in the same order.
#[must_use]
pub fn batch(
    visible: &[u32],
    split: usize,
    buckets: &[Bucket],
    draws: &[ItemDraw],
    spheres: &[Option<Sphere>],
    planes: &[Vec4; 6],
) -> Batch {
    let mut by_bucket: [Vec<(u32, u32, u32)>; CULLED_BUCKETS as usize] = Default::default();
    for (k, &item) in visible.iter().enumerate() {
        if let Some(Some(s)) = spheres.get(item as usize) {
            if outside(planes, *s) {
                continue;
            }
        }
        let Some(draw) = draws.get(item as usize) else {
            continue;
        };
        let own = buckets.get(item as usize).map_or(0, |b| *b as u32);
        let bucket = own + if k >= split { GROUP_BUCKETS } else { 0 };
        if let Some(list) = by_bucket.get_mut(bucket as usize) {
            list.push((draw.first_index, draw.index_count, item));
        }
    }
    let mut out = Batch {
        remap: Vec::with_capacity(visible.len()),
        ..Batch::default()
    };
    for (b, list) in by_bucket.iter_mut().enumerate() {
        // Stable: within a group the walk's order stays.
        list.sort_by_key(|&(first, count, _)| (first, count));
        let start = out.draws();
        for group in list.chunk_by(|a, b| (a.0, a.1) == (b.0, b.1)) {
            let (first, count, _) = group[0];
            let base = u32::try_from(out.remap.len()).unwrap_or(u32::MAX);
            out.remap.extend(group.iter().map(|g| g.2));
            let n = u32::try_from(group.len()).unwrap_or(u32::MAX);
            // `VkDrawIndexedIndirectCommand`: the indices, the instances, the first index, no
            // vertex offset (the item carries its vertices' first), and the first instance.
            out.commands.extend([count, n, first, 0, base]);
        }
        let start = u32::try_from(start).unwrap_or(u32::MAX);
        let end = u32::try_from(out.draws()).unwrap_or(u32::MAX);
        out.runs[b] = (start, end - start);
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn draw(first_index: u32, index_count: u32) -> ItemDraw {
        ItemDraw {
            index_count,
            first_index,
            two_sided: false,
            edge: false,
            vertex_base: 0,
            material: 0,
            alpha_test: false,
        }
    }

    /// A view down +y from the frame's origin, as the remaster draws one: left-handed, 90 degrees.
    fn looking_north() -> Mat4 {
        let view = Mat4::look_at_lh(Vec3::ZERO, Vec3::Y, Vec3::Z);
        Mat4::perspective_lh(std::f32::consts::FRAC_PI_2, 1.0, 0.25, 900.0) * view
    }

    /// Items of one mesh are drawn as one draw, each its own instance, in the walk's order; another
    /// mesh's are another draw; the interior's are in buckets of their own.
    #[test]
    fn the_items_of_one_mesh_are_one_draw_in_the_walks_order() {
        let draws = [
            draw(0, 36),
            draw(36, 12),
            draw(0, 36),
            draw(0, 36),
            draw(36, 12),
        ];
        let buckets = [Bucket::Opaque; 5];
        let planes = gfx_planes(looking_north());
        let b = batch(&[3, 1, 0, 4, 2], 5, &buckets, &draws, &[], &planes);
        assert_eq!(b.draws(), 2);
        assert_eq!(b.commands, vec![36, 3, 0, 0, 0, 12, 2, 36, 0, 3]);
        assert_eq!(
            b.remap,
            vec![3, 0, 2, 1, 4],
            "each group in the walk's order"
        );
        assert_eq!(b.runs, [(0, 2), (2, 0), (2, 0), (2, 0)]);

        // From the third on, the interior's: their own buckets.
        let b = batch(&[3, 1, 0, 4, 2], 2, &buckets, &draws, &[], &planes);
        assert_eq!(b.runs, [(0, 2), (2, 0), (2, 2), (4, 0)]);
        assert_eq!(b.remap, vec![3, 1, 0, 2, 4]);
    }

    /// A part behind the eye is left out of the view's draws and kept in the mirrored view's,
    /// which looks back at it; an item that is no block part's is never left out.
    #[test]
    fn a_part_behind_the_eye_is_drawn_only_in_the_view_that_sees_it() {
        let draws = [draw(0, 36), draw(0, 36), draw(0, 36)];
        let buckets = [Bucket::Opaque; 3];
        let spheres = [
            Some((Vec3::new(0.0, 50.0, 0.0), 1.0)),
            Some((Vec3::new(0.0, -50.0, 0.0), 1.0)),
            None,
        ];
        let view = gfx_planes(looking_north());
        let back = Mat4::from_rotation_z(std::f32::consts::PI);
        let mirrored = gfx_planes(looking_north() * back);
        let seen = batch(&[0, 1, 2], 3, &buckets, &draws, &spheres, &view);
        assert_eq!(seen.remap, vec![0, 2]);
        let in_the_water = batch(&[0, 1, 2], 3, &buckets, &draws, &spheres, &mirrored);
        assert_eq!(in_the_water.remap, vec![1, 2]);

        // Both in one pair of buffers: the mirrored view's after the view's.
        let (both, runs) = seen.then(&in_the_water);
        assert_eq!(both.remap, vec![0, 2, 1, 2]);
        assert_eq!(both.commands, vec![36, 2, 0, 0, 0, 36, 2, 0, 0, 2]);
        assert_eq!(runs, [(1, 1), (2, 0), (2, 0), (2, 0)]);
    }

    /// A part's sphere is its own sphere through its transform, grown by its largest scale and
    /// the slack, in the frame's space.
    #[test]
    fn a_parts_sphere_follows_its_transform_and_its_block() {
        let t = Mat4::from_scale_rotation_translation(
            Vec3::new(1.0, 2.0, 1.0),
            glam::Quat::IDENTITY,
            Vec3::new(10.0, 0.0, 0.0),
        );
        let (c, r) = sphere_of(t, ([1.0, 1.0, 0.0], 3.0), Vec3::new(192.0, 0.0, 0.0));
        assert_eq!(c, Vec3::new(203.0, 2.0, 0.0));
        assert!((r - (6.0 + SLACK)).abs() < 1e-5);
    }

    fn gfx_planes(m: Mat4) -> [Vec4; 6] {
        super::super::shadow::frustum(m)
    }
}
