//! An object held by another: a sword in a hand, a lantern on a hook.
//!
//! The holder keeps a list of what it holds, each child with where it hangs: a holding location
//! of the holder's setup (or the holder's own frame), a bone named by its kind, or a bone by its
//! index, with an offset from there and whether the child keeps its own turn. A child leaves the
//! world when it is taken up (it is drawn with its holder, not moved by physics), changes to the
//! holder's mode, and fades in; let go, it is taken off the holder's list and goes back to the
//! game's default mode.
//!
//! Each time the holder is drawn, every child is put where it hangs: the holder's drawn place,
//! the hanging point's matrix scaled by the holder's scale, then the child's offset. When the
//! holder's element has changed since it was last drawn, a child in its first two holding
//! locations fades out in water and back in out of it.
//!
//! A body reaches another only through [`Notice`]s ([`Notice::RemoveChild`] to the holder it
//! leaves); taking a child up is done with both bodies at hand ([`set_parent`]), the host saying
//! whether the holder's setup has the holding location and what mode its figure is in.

use dereth_physics::V3;
use dereth_primitives::{Frame, Position, Quat, Vec3};

use super::missile::Objects;
use super::{Animate, Body, Notice};
use crate::geom;
use crate::land::Land;
use crate::motion::WATER;
use crate::world::World;

/// The holding location that is the holder's own frame: every holder has it.
pub const OWN_FRAME: u32 = 0x4000_0003;
/// The slider that fades an object in and out.
pub const FADE_SLIDER: u32 = 0x4000_0015;
/// How long a fade takes, seconds.
pub const FADE_TIME: f32 = 0.5;

/// Where a child hangs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChildKind {
    #[default]
    None,
    /// A holding location of the holder's setup (or [`OWN_FRAME`]).
    HoldingLocation,
    /// A bone named by its kind.
    BoneName,
    /// A bone by its index in the holder's root part group.
    BoneIndex,
}

/// One child: which object, where it hangs, its offset from there, and whether it drops the
/// hanging point's turn (keeping only where it is).
// D2-06690, D2-06691
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChildInfo {
    pub id: u64,
    pub kind: ChildKind,
    pub location: u32,
    pub offset: Frame,
    pub remove_rotation: bool,
}

/// What a holder holds, in the order taken up.
// D2-06692, D2-06693
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChildList {
    pub children: Vec<ChildInfo>,
}

impl ChildList {
    /// Adds a child at the end.
    // D2-06700
    pub fn add_child(&mut self, info: ChildInfo) -> bool {
        self.children.push(info);
        true
    }

    /// Takes child `id` off (the last one takes its place); whether it was there.
    // D2-06698
    pub fn remove_child(&mut self, id: u64) -> bool {
        match self.children.iter().position(|c| c.id == id) {
            Some(i) => {
                self.children.swap_remove(i);
                true
            }
            None => false,
        }
    }
}

/// A frame's place and turn combined with an offset frame scaled by `scale`: the offset's place
/// scaled, then turned and moved by the frame; the turns multiplied.
// D2-06110
#[must_use]
pub fn combine_scaled(a: &Frame, b: &Frame, scale: Vec3) -> Frame {
    let o = Vec3::new(
        b.origin.x * scale.x,
        b.origin.y * scale.y,
        b.origin.z * scale.z,
    );
    combine_at(a, o, b.rotation)
}

/// A frame combined with an offset frame: the offset's place turned and moved by the frame; the
/// turns multiplied.
// D2-06109
#[must_use]
pub fn combine(a: &Frame, b: &Frame) -> Frame {
    combine_at(a, b.origin, b.rotation)
}

fn combine_at(a: &Frame, o: Vec3, q: Quat) -> Frame {
    let m = geom::rotation_axes(a.rotation);
    let mut f = Frame::new(
        Vec3::new(
            o.y * m[3] + o.z * m[6] + o.x * m[0] + a.origin.x,
            o.y * m[4] + o.z * m[7] + o.x * m[1] + a.origin.y,
            o.y * m[5] + o.z * m[8] + o.x * m[2] + a.origin.z,
        ),
        a.rotation,
    );
    geom::set_rotate(&mut f, quat_mul(a.rotation, q));
    f
}

/// The product of two turns: `a` after `b`.
fn quat_mul(a: Quat, b: Quat) -> Quat {
    Quat::new(
        a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
        a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
    )
}

/// A frame from a matrix whose rows are its axes (each made unit length) and its place.
// D2-13161
#[must_use]
pub fn frame_from_matrix(m: &[[f32; 4]; 4]) -> Frame {
    let unit = |r: &[f32; 4]| {
        let n = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if n > 0.0 {
            [r[0] / n, r[1] / n, r[2] / n]
        } else {
            [0.0, 0.0, 0.0]
        }
    };
    let (x, y, z) = (unit(&m[0]), unit(&m[1]), unit(&m[2]));
    let axes = [x[0], x[1], x[2], y[0], y[1], y[2], z[0], z[1], z[2]];
    Frame::new(
        Vec3::new(m[3][0], m[3][1], m[3][2]),
        geom::rotation_from_axes(&axes),
    )
}

impl Body {
    /// How many children it holds.
    #[must_use]
    pub fn num_children(&self) -> usize {
        self.children.as_ref().map_or(0, |c| c.children.len())
    }

    /// Its `i`th child, when it holds that many.
    // D2-06341
    #[must_use]
    pub fn child(&self, i: usize) -> Option<&ChildInfo> {
        self.children.as_ref()?.children.get(i)
    }

    /// Whether it holds object `id` itself.
    // D2-06228
    #[must_use]
    pub fn look_up_in_children(&self, id: u64) -> bool {
        self.children
            .as_ref()
            .is_some_and(|c| c.children.iter().any(|i| i.id == id))
    }

    /// Whether object `id` is among what it holds or, in turn, what they hold (`children_of` gives
    /// another body's children).
    #[must_use]
    pub fn look_up_in_all_children(
        &self,
        id: u64,
        children_of: &dyn Fn(u64) -> Option<ChildList>,
    ) -> bool {
        let Some(list) = &self.children else {
            return false;
        };
        list.children
            .iter()
            .any(|c| c.id == id || held_by(c.id, id, children_of, 1))
    }

    /// Takes up object `child` (not itself, nor its own holder) where it hangs; the child, unless
    /// it is a temporary object, fades in.
    // D2-06221
    pub fn add_child_internal(
        &mut self,
        child: &mut Body,
        kind: ChildKind,
        location: u32,
        offset: Frame,
        remove_rotation: bool,
        child_is_temporary: bool,
    ) -> bool {
        if child.id == self.id || child.id == self.parent {
            return false;
        }
        self.children
            .get_or_insert_with(ChildList::default)
            .add_child(ChildInfo {
                id: child.id,
                kind,
                location,
                offset,
                remove_rotation,
            });
        if !child_is_temporary {
            child.notices.push(Notice::Animate(Animate::Slider {
                context: 0,
                slider: FADE_SLIDER,
                value: 1.0,
                time: FADE_TIME,
            }));
        }
        true
    }

    /// Takes up object `child` at holding location `location` (none: at the bone of kind `bone`),
    /// `offset` from there.
    // D2-06247
    pub fn add_child(
        &mut self,
        child: &mut Body,
        location: u32,
        bone: u32,
        offset: Vec3,
        remove_rotation: bool,
        child_is_temporary: bool,
    ) -> bool {
        let frame = Frame::new(offset, Quat::IDENTITY);
        if location == 0 {
            self.add_child_internal(
                child,
                ChildKind::BoneName,
                bone,
                frame,
                remove_rotation,
                child_is_temporary,
            )
        } else {
            self.add_child_internal(
                child,
                ChildKind::HoldingLocation,
                location,
                frame,
                remove_rotation,
                child_is_temporary,
            )
        }
    }

    /// Lets go of object `id`.
    // D2-06248
    pub fn remove_child(&mut self, id: u64) -> bool {
        if let Some(c) = self.children.as_mut() {
            c.remove_child(id);
        }
        true
    }

    /// Lets go of its holder, if it has one: the holder takes it off its list, and it goes back
    /// to the game's default mode.
    // D2-06223, D2-06340
    pub fn unset_parent(&mut self) {
        if self.parent == 0 {
            return;
        }
        self.notices.push(Notice::RemoveChild {
            parent: self.parent,
            child: self.id,
        });
        self.parent = 0;
        self.has_parent = false;
        if !self.is_animation_frozen() {
            self.set_animation_active(true);
            self.notices.push(Notice::Animate(Animate::DefaultMode));
        }
    }

    /// Lets go of every child, the first each time: the host has each let go of it
    /// (`let_go`, which calls the child's [`Body::unset_parent`]) and it comes off the list.
    pub fn unparent_children(&mut self, mut let_go: impl FnMut(u64)) {
        while let Some(first) = self.child(0).map(|c| c.id) {
            let_go(first);
            self.remove_child(first);
        }
    }

    /// The object at the top of its holders (`parent_of` gives another body's holder; 0 for
    /// none); none when it is held by nothing.
    #[must_use]
    pub fn top_parent(&self, parent_of: &dyn Fn(u64) -> u64) -> Option<u64> {
        let mut top = None;
        let mut at = self.parent;
        for _ in 0..MAX_DEPTH {
            if at == 0 {
                break;
            }
            top = Some(at);
            at = parent_of(at);
        }
        top
    }

    /// Where child `info` of this holder, drawn at `drawn` at scale `scale`, hangs: `hanging` is
    /// the matrix of its hanging point in the holder's frame (the holding location's transform,
    /// with its scale, then its bone's; the bone's; or none for the holder's own frame). Its cell
    /// is the holder's.
    // D2-06182
    #[must_use]
    pub fn child_place(
        drawn: &Position,
        scale: Vec3,
        info: &ChildInfo,
        hanging: Option<&[[f32; 4]; 4]>,
    ) -> Position {
        let at = match hanging {
            Some(m) if info.remove_rotation => {
                Frame::new(Vec3::new(m[3][0], m[3][1], m[3][2]), Quat::IDENTITY)
            }
            Some(m) => frame_from_matrix(m),
            None => Frame::new(Vec3::ZERO, Quat::IDENTITY),
        };
        let hung = combine_scaled(&drawn.frame, &at, scale);
        Position::new(drawn.cell, combine(&hung, &info.offset))
    }

    /// The fade a child takes as the holder is drawn after its element changed: a child in the
    /// holder's first holding location, or in its second whatever it hangs from, fades out while
    /// the holder is headed into water and back in otherwise; a temporary child never.
    // D2-06182
    #[must_use]
    pub fn child_fade(&self, info: &ChildInfo, child_is_temporary: bool) -> Option<f32> {
        if !self.element_changed || child_is_temporary {
            return None;
        }
        let first = info.kind == ChildKind::HoldingLocation && info.location == 1;
        if !(first || info.location == 2) {
            return None;
        }
        Some(if self.desired_element() == WATER {
            0.0
        } else {
            1.0
        })
    }

    /// The holder has been drawn with its children: its element change is no longer news.
    pub fn children_drawn(&mut self) {
        if self.children.is_some() {
            self.element_changed = false;
        }
    }

    /// The server says the body is let go of (or put in a container, which only the player
    /// hears), with its position stamp `stamp`; the host has checked the instance stamp is the
    /// body's. When the stamp is newer it lets go of its holder and leaves the world. Whether the
    /// stamp was newer.
    pub fn receive_deparent(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        stamp: u16,
    ) -> bool {
        let newer = self.is_newer_than_update(super::POSITION_STAMP, stamp);
        if newer {
            self.unset_parent();
            self.leave_world(land, world, objects, None);
        }
        newer
    }
}

/// How deep holders may hold holders.
const MAX_DEPTH: usize = 64;

fn held_by(
    holder: u64,
    id: u64,
    children_of: &dyn Fn(u64) -> Option<ChildList>,
    depth: usize,
) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    children_of(holder).is_some_and(|l| {
        l.children
            .iter()
            .any(|c| c.id == id || held_by(c.id, id, children_of, depth + 1))
    })
}

/// Whether a holder holds things at `location`: always at its own frame, else when its setup has
/// that holding location (`setup_has`, the host's answer).
// D2-05996
#[must_use]
pub const fn holds_at(location: u32, setup_has: bool) -> bool {
    location == OWN_FRAME || setup_has
}

/// How a holder takes up a child: where it hangs, whether the host says the holder's setup has
/// that holding location, the holder's figure's mode, and whether the child is a temporary object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TakeUp {
    pub location: u32,
    pub holds: bool,
    pub parent_mode: u32,
    pub child_is_temporary: bool,
}

/// Body `child`, an object an effect made, hangs from `parent` at holding location `location` (none:
/// at the bone of kind `bone`), `offset` from there, keeping only the place of that point when
/// `remove_rotation`: the holder adds it, then it lets go of any holder it had and leaves the world.
/// It takes no mode from the holder. Whether it was taken up.
// D2-05997
#[allow(clippy::too_many_arguments)]
pub fn fx_set_parent(
    child: &mut Body,
    parent: &mut Body,
    location: u32,
    bone: u32,
    offset: Vec3,
    remove_rotation: bool,
    land: &Land,
    world: &mut World,
    objects: &mut dyn Objects,
) -> bool {
    if !parent.add_child(child, location, bone, offset, remove_rotation, true) {
        return false;
    }
    child.unset_parent();
    child.leave_world(land, world, objects, None);
    child.parent = parent.id;
    child.has_parent = true;
    true
}

/// Body `child` is taken up by `parent` (see [`TakeUp`]; the holder's own frame always, another
/// holding location only when the holder's setup has it): the holder adds it, then it lets go of
/// any holder it had and leaves the world, and changes to the holder's mode. Whether it was
/// taken up.
// D2-05998, D2-06146
pub fn set_parent(
    child: &mut Body,
    parent: &mut Body,
    how: TakeUp,
    land: &Land,
    world: &mut World,
    objects: &mut dyn Objects,
) -> bool {
    if how.location != OWN_FRAME && !how.holds {
        return false;
    }
    if !parent.add_child(
        child,
        how.location,
        0,
        Vec3::ZERO,
        false,
        how.child_is_temporary,
    ) {
        return false;
    }
    child.unset_parent();
    child.leave_world(land, world, objects, None);
    crate::motion::Owner::do_mode(child, 0, how.parent_mode);
    child.parent = parent.id;
    child.has_parent = true;
    true
}

/// Body `child` is taken up by `parent` as a part group of its own asks: only when the child has
/// a part group of key `group`, and then at location 0, whatever frame was asked ([`set_parent`];
/// `how` says whether the holder holds things there).
#[allow(clippy::too_many_arguments)]
pub fn set_parent_in_group(
    child: &mut Body,
    parent: &mut Body,
    group: u32,
    how: TakeUp,
    land: &Land,
    world: &mut World,
    objects: &mut dyn Objects,
) -> bool {
    if child
        .part_group()
        .and_then(|g| g.part_group(group))
        .is_none()
    {
        return false;
    }
    set_parent(
        child,
        parent,
        TakeUp { location: 0, ..how },
        land,
        world,
        objects,
    )
}

/// Body `child` sticks to `parent` where it stands: held by the bone of the holder's root part
/// group nearest it, at its place and turn as seen from the holder's frame (the place dropped),
/// it then lets go of any holder it had and leaves the world. False when the holder has no part
/// group or no bones.
// D2-06222, D2-06111
pub fn sticky_set_parent(
    child: &mut Body,
    parent: &mut Body,
    child_is_temporary: bool,
    land: &Land,
    world: &mut World,
    objects: &mut dyn Objects,
) -> bool {
    let Some(parts) = parent.part_group() else {
        return false;
    };
    let at = Frame::new(
        parent
            .position
            .frame
            .origin
            .add(crate::geom::offset(&parent.position, &child.position)),
        child.position.frame.rotation,
    );
    let seen = crate::parts::relative(&at, &parent.position.frame);
    let mut bone = 0;
    let mut offset = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    if !parts.array.nearest_bone(&seen, &mut bone, &mut offset) {
        return false;
    }
    parent.add_child_internal(
        child,
        ChildKind::BoneIndex,
        bone,
        offset,
        false,
        child_is_temporary,
    );
    child.unset_parent();
    child.leave_world(land, world, objects, None);
    child.parent = parent.id;
    child.has_parent = true;
    true
}
