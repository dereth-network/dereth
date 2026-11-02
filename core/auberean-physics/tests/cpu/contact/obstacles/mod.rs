//! A body meeting another object's spheres and upright cylinders: every arm of the two tests (the
//! placing and passing-through overlap, the step down, the walkable check, the settling after a
//! landing, the step up, the slides, the point collisions and the landing), and their wrappers
//! that place a shape at its object's position and scale, each held to the September 2013
//! client's own answers. Fixture: tables of cases, each a transition (the mover's spheres where the
//! step ends and where it began, its flags, its contact planes), the shape it meets, and the bits
//! of what the client's own code answered and left in the transition.

mod arms_cylinder_cases;
mod arms_edges_cases;
mod arms_placed_cases;
mod arms_sphere_cases;
mod slides_cases;

use auberean_physics::obstacle::{cylinder, sphere, StepUp};
use dereth_physics::geom::{CylSphere, Sphere};
use dereth_physics::transition::{collide, InsertType, Transition};
use dereth_physics::TransitionState;
use dereth_primitives::shape::Plane;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geometry::{check, Case};

/// The transition's step up, answering as the case says: it clears the contact plane, takes the
/// face's normal as the step-up normal, and is counted, as the client's own is where the cases
/// stood it in.
struct Answer {
    stands: bool,
    asked: Vec<Vec3>,
}

impl StepUp for Answer {
    fn step_up(&mut self, t: &mut Transition, normal: Vec3) -> bool {
        self.asked.push(normal);
        t.collision_info.clear_contact_plane();
        t.sphere_path.step_up_normal = normal;
        t.sphere_path.step_up = false;
        self.stands
    }
}

fn v3(x: &[f32]) -> Vec3 {
    Vec3::new(x[0], x[1], x[2])
}

fn state(r: TransitionState) -> u32 {
    match r {
        TransitionState::Ok => 1,
        TransitionState::Collided => 2,
        TransitionState::Adjusted => 3,
        TransitionState::Slid => 4,
        #[allow(unreachable_patterns)]
        _ => 0,
    }
}

/// The transition a case describes, and the step up it stands in, and where its shape's words
/// start.
fn transition(w: &[u32], x: &[f32]) -> (Transition, Answer, usize) {
    let mut t = Transition::default();
    t.set_rules(auberean_physics::rules::rules());
    t.object_info.state = w[0];
    let sp = &mut t.sphere_path;
    sp.num_sphere = w[1] as usize;
    let at = |cell: u32, o: Vec3, q: Quat| Position::new(CellId(cell), Frame::new(o, q));
    let f = &x[12..];
    t.object_info.step_up_height = f[0];
    sp.global_sphere = [
        Sphere::new(v3(&f[1..4]), f[4]),
        Sphere::new(v3(&f[5..8]), f[8]),
    ];
    sp.global_curr_center = [v3(&f[9..12]), v3(&f[12..15])];
    sp.curr_pos = at(w[2], Vec3::ZERO, Quat::IDENTITY);
    sp.check_pos = at(w[3], v3(&f[15..18]), Quat::IDENTITY);
    sp.global_low_point = v3(&f[18..21]);
    sp.walk_interp = f[21];
    sp.step_down_amt = f[22];
    sp.walkable_allowance = f[23];
    sp.insert_type = if w[4] == 1 {
        InsertType::Placement
    } else {
        InsertType::Transition
    };
    sp.step_down = w[5] != 0;
    sp.check_walkable = w[6] != 0;
    sp.collide = w[7] != 0;
    sp.obstruction_ethereal = w[8] != 0;
    sp.cell_array_valid = true;
    sp.localspace_pos = at(w[3], v3(&f[32..35]), Quat::new(f[35], f[36], f[37], f[38]));
    let ci = &mut t.collision_info;
    ci.contact_plane_valid = w[9] != 0;
    ci.contact_plane = Plane::new(v3(&f[24..27]), f[27]);
    ci.last_known_contact_plane_valid = w[10] != 0;
    ci.last_known_contact_plane = Plane::new(v3(&f[28..31]), f[31]);
    ci.contact_plane_is_water = true;
    ci.contact_plane_is_landscape = true;
    ci.contact_plane_is_environment = true;
    let up = Answer {
        stands: w[11] != 0,
        asked: Vec::new(),
    };
    (t, up, 51)
}

/// What the arms answered and left, in the order the cases give it.
fn outcome(r: TransitionState, t: &Transition, up: &Answer) -> Vec<u32> {
    let b = |v: f32| v.to_bits();
    let vb = |v: Vec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
    let sp = &t.sphere_path;
    let ci = &t.collision_info;
    let mut out = vec![state(r)];
    out.extend(vb(sp.check_pos.frame.origin));
    out.extend(vb(sp.global_sphere[0].center));
    out.extend(vb(sp.global_sphere[1].center));
    out.extend(vb(sp.global_low_point));
    out.extend([
        b(sp.walk_interp),
        b(sp.walkable_allowance),
        u32::from(sp.collide),
    ]);
    out.extend(vb(sp.step_up_normal));
    out.extend([u32::from(sp.step_up), u32::from(sp.cell_array_valid)]);
    out.push(sp.backup_check_pos.cell.0);
    out.extend(vb(sp.backup_check_pos.frame.origin));
    out.push(u32::from(ci.contact_plane_valid));
    out.extend(vb(ci.contact_plane.normal));
    out.push(b(ci.contact_plane.d));
    out.push(ci.contact_plane_cell_id.0);
    out.extend([
        u32::from(ci.contact_plane_is_water),
        u32::from(ci.contact_plane_is_landscape),
        u32::from(ci.contact_plane_is_environment),
    ]);
    out.push(u32::from(ci.collision_normal_valid));
    out.extend(vb(ci.collision_normal));
    #[allow(clippy::cast_possible_truncation)]
    out.push(up.asked.len() as u32);
    out.extend(vb(up.asked.last().copied().unwrap_or(Vec3::ZERO)));
    out
}

/// `cases` and the built edge cases labeled `label`.
fn with_edges(cases: &[Case], label: &str) -> Vec<Case> {
    let edges = arms_edges_cases::CASES.iter().filter(|c| c.0 == label);
    cases.iter().chain(edges).copied().collect()
}

/// A body meeting a sphere answers, and leaves its transition, as the September 2013 client's own
/// code does, to the bit, in every arm: placed or passing through, stepping down onto it, checking
/// where it may walk, settling onto it after a landing, stepping up onto it or sliding along it on
/// the ground, meeting it as a point when it clips its path, and landing on it in the air.
///
/// Behaviour: transition.obstacle.a-sphere-is-met-as-the-client-meets-it
#[test]
fn a_sphere_is_met_as_the_client_meets_it() {
    let cases = with_edges(arms_sphere_cases::CASES, "sphere");
    check(&cases, |label, x, w| {
        if label != "sphere" {
            return None;
        }
        let (mut t, mut up, k) = transition(w, x);
        let o = Sphere::new(v3(&x[k..k + 3]), x[k + 3]);
        let r = sphere::meet(&mut t, &mut up, &o, w[k + 4] != 0);
        Some(outcome(r, &t, &up))
    });
}

/// A body meeting an upright cylinder answers, and leaves its transition, as the September 2013
/// client's own code does, to the bit, in every arm: placed or passing through, stepping down onto
/// its top, checking where it may walk, settling onto its top after a landing, stepping up onto it
/// or sliding along it on the ground, meeting it as a point (at a face or at its side) when it
/// clips its path, and landing on it in the air.
///
/// Behaviour: transition.obstacle.a-cylinder-is-met-as-the-client-meets-it
#[test]
fn a_cylinder_is_met_as_the_client_meets_it() {
    let cases = with_edges(arms_cylinder_cases::CASES, "cylinder");
    check(&cases, |label, x, w| {
        if label != "cylinder" {
            return None;
        }
        let (mut t, mut up, k) = transition(w, x);
        let c = CylSphere {
            low_pt: v3(&x[k..k + 3]),
            height: x[k + 3],
            radius: x[k + 4],
        };
        let r = cylinder::meet(&mut t, &mut up, &c);
        Some(outcome(r, &t, &up))
    });
}

/// A sphere or cylinder given in its object's frame is placed at the object's position and scale
/// along each axis (a sphere's radius by the scale's height, a cylinder's by the mean of its two
/// widths) before it is met, as the September 2013 client places it, to the bit.
///
/// Behaviour: transition.obstacle.a-shape-is-placed-at-its-objects-position-and-scale
#[test]
fn a_shape_is_placed_at_its_objects_position_and_scale() {
    check(arms_placed_cases::CASES, |label, x, w| {
        let (mut t, mut up, k) = transition(w, x);
        let (r, j) = match label {
            "sphere_placed" => {
                let s = Sphere::new(v3(&x[k..k + 3]), x[k + 3]);
                (Some((s, w[k + 4] != 0)), k + 5)
            }
            "cylinder_placed" => (None, k + 5),
            _ => return None,
        };
        let pos = Position::new(
            CellId(w[j]),
            Frame::new(
                v3(&x[j + 1..j + 4]),
                Quat::new(x[j + 4], x[j + 5], x[j + 6], x[j + 7]),
            ),
        );
        let scale = v3(&x[j + 8..j + 11]);
        let r = match r {
            Some((s, creature)) => sphere::meet_placed(&mut t, &mut up, &s, &pos, scale, creature),
            None => {
                let c = CylSphere {
                    low_pt: v3(&x[k..k + 3]),
                    height: x[k + 3],
                    radius: x[k + 4],
                };
                cylinder::meet_placed(&mut t, &mut up, &c, &pos, scale)
            }
        };
        Some(outcome(r, &t, &up))
    });
}

/// A body slides along a surface as the September 2013 client slides it, to the bit, wherever the
/// transition slides it: its sphere's slide along a normal (refused as parallel to the plane it
/// stands on only within 0.0002 squared, the normal it collides with turned back along the move
/// and written back), the slide along the step-up normal when it cannot step up, and a physics
/// tree's slide and step up, the tree's normal turned into the world as the client turns it.
///
/// Behaviour: transition.obstacle.a-body-slides-as-the-client-slides-it
#[test]
fn a_body_slides_as_the_client_slides_it() {
    check(slides_cases::CASES, |label, x, w| {
        let (mut t, mut up, _) = transition(&w[2..], &x[2..]);
        let given = v3(&x[53..56]);
        let mut n = given;
        let turn = auberean_physics::rules::SLIDE.turn;
        let r = match label {
            "slide" => {
                let i = w[1] as usize;
                let (s, curr) = (
                    t.sphere_path.global_sphere[i],
                    t.sphere_path.global_curr_center[i],
                );
                collide::slide(&mut t, s, &mut n, curr)
            }
            "step_up_slide" => {
                t.sphere_path.step_up = true;
                t.sphere_path.step_up_normal = given;
                collide::step_up_slide(&mut t)
            }
            "tree_slide" => {
                let mut turned = turn(&t.sphere_path.localspace_pos.frame, given);
                let (s, curr) = (
                    t.sphere_path.global_sphere[0],
                    t.sphere_path.global_curr_center[0],
                );
                collide::slide(&mut t, s, &mut turned, curr)
            }
            "tree_step_up" => {
                let turned = turn(&t.sphere_path.localspace_pos.frame, given);
                if up.step_up(&mut t, turned) {
                    TransitionState::Ok
                } else {
                    collide::step_up_slide(&mut t)
                }
            }
            _ => return None,
        };
        let mut out = outcome(r, &t, &up);
        out.extend([n.x.to_bits(), n.y.to_bits(), n.z.to_bits()]);
        Some(out)
    });
}
