//! Putting a body somewhere: the cell the place belongs to, then either straight into it (when
//! forced, or for a body that does not move) or wherever a placement finds room nearest the place,
//! scattering around it when asked or when the place is taken.

use dereth_physics::cell::CellResolver;
use dereth_physics::transition::insert::find_valid_position;
use dereth_physics::transition::Transition;
use dereth_physics::PhysicsState;
use dereth_primitives::{CellId, Frame, Position, Vec3};

use super::Body;
use crate::collision::manager::BodyKey;
use crate::land::Land;
use crate::place::{flags, SetPosition};
use crate::rules::rules;
use crate::world::World;

/// How putting a body somewhere went.
pub mod error {
    pub const OK: u32 = 0;
    pub const GENERAL_FAILURE: u32 = 1;
    pub const NO_VALID_POSITION: u32 = 2;
    pub const NO_CELL: u32 = 3;
    pub const COLLIDED: u32 = 4;
}

/// How far a placement may move a body from the place, toward greater x and y, without leave to
/// slide.
const PLACEMENT_SLACK: f32 = 0.05;

/// The cell kinds a place may name: land cells, interior cells, and a whole landblock.
fn names_a_cell(cell: CellId) -> bool {
    let low = cell.0 & 0xFFFF;
    (1..=64).contains(&low) || (0x100..=0xFFFD).contains(&low) || low == 0xFFFF
}

/// The cell `pos` belongs to, `pos` brought into it: outside, the land cell its point falls in
/// (none when that is off the map or not loaded); inside, the cell (or one it sees) holding the
/// point `center` (in the body's frame) lands on, else, for a cell seen from outside, the land
/// cell outside. None when the place names no cell, or its interior cell is not loaded.
// D2-06047, D2-03593, D2-03472
pub fn adjust_position(land: &Land, pos: &mut Position, center: Vec3) -> Option<Option<CellId>> {
    if !names_a_cell(pos.cell) {
        return None;
    }
    let visible = |land: &Land, id: CellId| {
        let lb = id.landblock();
        (id.0 != 0 && land.block(lb.x(), lb.y()).is_some()).then_some(id)
    };
    if !crate::interior::is_interior(pos.cell) {
        if !crate::cells::adjust_to_outside(&mut pos.cell, &mut pos.frame.origin) {
            return None;
        }
        return Some(visible(land, pos.cell));
    }
    let home = land.cell(pos.cell)?;
    let resolver = CellResolver::new(land);
    let start = resolver.get_visible(pos.cell)?;
    let point = crate::geom::point_to_global(&pos.frame, center);
    if let Some(child) = resolver.find_visible_child_cell(&start, point) {
        pos.cell = child.id();
        return Some(Some(child.id()));
    }
    if !home.seen_outside {
        return None;
    }
    if !crate::cells::adjust_to_outside(&mut pos.cell, &mut pos.frame.origin) {
        return None;
    }
    Some(visible(land, pos.cell))
}

impl Body {
    /// Where `frame` of the body's own lands, brought into the cell it falls in; the place, and
    /// the cell it stands in when that is loaded. None when the place names no cell.
    // D2-06157
    #[must_use]
    pub fn local_to_global(
        &self,
        land: &Land,
        frame: &Frame,
    ) -> Option<(Position, Option<CellId>)> {
        let mut place = Position::new(
            self.position.cell,
            dereth_primitives::frame::combine(&self.position.frame, frame),
        );
        let cell = adjust_position(land, &mut place, Vec3::ZERO)?;
        Some((place, cell))
    }

    /// `pos` brought into the interior cell its point (or its first sphere's middle, turned with
    /// it) lies in, when any cell holds it.
    pub fn find_inside_position(&self, land: &Land, pos: &mut Position) -> bool {
        let point = self.spheres().first().map_or(pos.frame.origin, |s| {
            crate::geom::point_to_global(&pos.frame, s.center)
        });
        match land.find_in_visible_cells(point) {
            Some(cell) => {
                pos.cell = cell;
                true
            }
            None => false,
        }
    }

    /// Puts the body where `s` says. A transition with the body's spheres (see
    /// [`Body::moving_spheres`]) looks for room; the place is kept as the body's placement when `s`
    /// asks and it worked.
    // D2-06364
    pub fn set_position(
        &mut self,
        land: &Land,
        world: &mut World,
        s: &SetPosition,
        dice: &mut dyn FnMut() -> f32,
    ) -> u32 {
        let key = match self.key.filter(|k| world.position(*k).is_some()) {
            Some(k) => k,
            None => world.insert_body(land, 0, self),
        };
        let mut t = Transition::default();
        t.init();
        t.set_rules(rules());
        t.object_info.object = world.handle(key);
        t.object_info.scale = self.config.scale;
        t.object_info.step_up_height = self.step_up_height();
        t.object_info.step_down_height = self.step_down_height();
        t.object_info.step_down = t.object_info.step_down_height != 0.0;
        let (spheres, scale) = self.moving_spheres();
        t.sphere_path.init_sphere(spheres, scale);
        let r = self.set_position_ordered(land, world, key, s, &mut t, dice);
        if r == error::OK && s.flags & flags::PLACEMENT != 0 {
            self.set_placement_position(s.position);
        }
        r
    }

    /// Keeps `p` as the body's placement: the body turns at once to its heading, made to a 255th of
    /// a turn, and a change of mode under way finishes at once.
    // D2-06051
    pub fn set_placement_position(&mut self, p: Position) {
        self.placement = Some(p);
        self.minterp
            .turn_to(true, crate::geom::frame_heading(&p.frame));
        self.notices
            .push(super::Notice::Animate(super::Animate::SnapMode));
    }

    /// The body, set at `p` from where the world can't be reached, is placed there by its next step.
    pub(super) fn place_soon(&mut self, p: Position) {
        let was = self.placing.map_or(self.position, |(_, was)| was);
        self.placing = Some((p, was));
    }

    /// The body leaves the ground: when it was just set in a place not yet placed, what comes of
    /// that place is to stand under it.
    pub(super) fn note_leaving_while_placing(&mut self) {
        if self.placing.is_some() && self.left_while_placing.is_none() {
            self.left_while_placing = Some(self.contact);
        }
    }

    /// The place set since the last step is placed: the body takes a free spot nearby and touches
    /// only what is there, and with no room goes back where it stood.
    // D2-06369
    pub(super) fn take_placing(&mut self, land: &Land, world: &mut World, key: BodyKey) {
        let Some((p, was)) = self.placing.take() else {
            return;
        };
        // Placed through the entry this step moves it by, in the world it steps in.
        let entry = self.key.replace(key);
        // What was asked of the body after it was set there stands: its speed, and a jump or a
        // push that took it off the ground, which comes after the place and isn't a landing.
        let velocity = self.velocity;
        let left = self.left_while_placing.take();
        if let Some(touching) = left {
            self.contact = touching;
        }
        if self.set_position_simple(land, world, p, true) != error::OK {
            self.position = was;
        }
        if left.is_some() {
            self.contact = false;
            self.do_element(crate::motion::AIR);
        }
        self.velocity = velocity;
        self.key = entry;
        self.set_placement_position(p);
    }

    /// Puts the body at `p`, taking a free spot nearby when `slide`.
    // D2-06368
    pub fn set_position_simple(
        &mut self,
        land: &Land,
        world: &mut World,
        p: Position,
        slide: bool,
    ) -> u32 {
        self.set_position(land, world, &SetPosition::simple(p, slide), &mut || 0.0)
    }

    /// Scatters when asked; else tries the place itself and scatters when that fails and the
    /// order says to.
    // D2-06358
    fn set_position_ordered(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        s: &SetPosition,
        t: &mut Transition,
        dice: &mut dyn FnMut() -> f32,
    ) -> u32 {
        if s.flags & flags::SCATTER != 0 {
            return self.set_scatter_position(land, world, key, s, t, dice);
        }
        let mut pos = s.position;
        let r = self.set_position_at(land, world, key, &mut pos, s, t);
        if r != error::OK && s.flags & flags::SCATTER_IF_BLOCKED != 0 {
            return self.set_scatter_position(land, world, key, s, t, dice);
        }
        r
    }

    /// Tries up to the order's tries places around its place, each moved by a roll between -1 and
    /// 1 times the first radius along x, then one times the second along y; the first that works
    /// ends it.
    // D2-06345
    fn set_scatter_position(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        s: &SetPosition,
        t: &mut Transition,
        dice: &mut dyn FnMut() -> f32,
    ) -> u32 {
        let mut r = error::GENERAL_FAILURE;
        for _ in 0..s.tries {
            let mut pos = s.position;
            let dx = dice() * s.radius_min;
            let dy = dice() * s.radius_max;
            pos.frame.origin.x += dx;
            pos.frame.origin.y += dy;
            r = self.set_position_at(land, world, key, &mut pos, s, t);
            if r == error::OK {
                return error::OK;
            }
        }
        r
    }

    /// Puts the body at `pos`: no cell, no place; forced (or a body that does not move), straight
    /// in; else where a placement finds room, or nowhere (having hit something or not).
    // D2-06268
    fn set_position_at(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        pos: &mut Position,
        s: &SetPosition,
        t: &mut Transition,
    ) -> u32 {
        let center = t.sphere_path.local_sphere[0].center;
        let Some(Some(cell)) = adjust_position(land, pos, center) else {
            return error::NO_CELL;
        };
        if s.flags & flags::FORCE != 0 || !self.is_mobile() {
            return self.force_into_cell(land, world, key, cell, pos);
        }
        if !self.check_position(land, world, key, cell, pos, s, t) {
            return if self.handle_all_collisions(land, world, key, t, false) {
                error::COLLIDED
            } else {
                error::NO_VALID_POSITION
            };
        }
        if t.sphere_path.curr_cell.is_none() {
            return error::NO_CELL;
        }
        let was = (self.contact, self.on_walkable);
        self.take_transition(land, world, key, t, was, None);
        world.place(land, key, self.position);
        error::OK
    }

    /// Whether a placement finds room for the body at `pos` in `cell`: without leave to slide, only
    /// within 5 cm of the place toward greater x and y and in the same cell, the place then taking
    /// the spot found.
    // D2-06048
    #[allow(clippy::too_many_arguments)]
    fn check_position(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        cell: CellId,
        pos: &mut Position,
        s: &SetPosition,
        t: &mut Transition,
    ) -> bool {
        t.sphere_path.init_path(Some(cell), None, pos);
        let slide = s.flags & flags::SLIDE != 0;
        if !slide {
            t.sphere_path.placement_allows_sliding = false;
        }
        world.add_scenery(land, cell);
        world.begin_attempts(t);
        let view = world.view(land, Some(key), false);
        let found = find_valid_position(&world.ctx(&view), t, PhysicsState::DEFAULT);
        world.apply(view.into_found());
        world.end_attempts(t);
        if !found {
            return false;
        }
        if slide {
            return true;
        }
        let at = t.sphere_path.curr_pos;
        if at.frame.origin.x - pos.frame.origin.x < PLACEMENT_SLACK
            && at.frame.origin.y - pos.frame.origin.y < PLACEMENT_SLACK
            && at.cell == cell
        {
            pos.frame.origin = at.frame.origin;
            return true;
        }
        false
    }

    /// Puts the body straight into `cell` at `pos`, its entry in the cells it reaches there.
    // D2-06266, D2-05978
    fn force_into_cell(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        cell: CellId,
        pos: &Position,
    ) -> u32 {
        self.position = Position::new(cell, pos.frame);
        world.place(land, key, self.position);
        let at = self.position;
        self.update_cell_material(land, world, key, &at);
        self.water = land.water_plane(cell);
        error::OK
    }
}
