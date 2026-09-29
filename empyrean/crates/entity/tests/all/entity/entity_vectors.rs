//! Vectors: empyrean/fixtures/vectors/entity
//! ObjectGuid, LandblockId (from xy, transition, pair) and Position (constructors, distances, in-
//! front-of, rotate, current dir) equal ACE.Entity net10 vectors bit-exactly.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use std::cell::Cell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

use empyrean_common::vectors::{self, f32_of, f64_of, same_f32, throws, Case};
use empyrean_entity::{LandblockId, ObjectGuid, Position, Quaternion, Vector2, Vector3};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------------- harness

thread_local!(static QUIET: Cell<bool> = const { Cell::new(false) });
static HOOK: Once = Once::new();

/// Runs `f`, turning a panic (an ACE exception) into `Err`, without printing it.
fn catch<R>(f: impl FnOnce() -> R) -> Result<R, ()> {
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                previous(info);
            }
        }));
    });
    QUIET.with(|q| q.set(true));
    let r = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(false));
    r.map_err(|_| ())
}

struct Report {
    name: &'static str,
    total: usize,
    failures: Vec<String>,
}

impl Report {
    fn check(&mut self, case: &Case, ok: bool, got: impl std::fmt::Debug) {
        self.total += 1;
        if !ok {
            self.failures.push(format!(
                "in {} expected {} got {got:?}",
                case.input, case.output
            ));
        }
    }
}

fn replay(name: &'static str, mut one: impl FnMut(&Case, &mut Report)) {
    let file = vectors::load_named("entity", name);
    let mut r = Report {
        name,
        total: 0,
        failures: Vec::new(),
    };
    for case in &file.cases {
        one(case, &mut r);
    }
    assert!(r.total > 0, "{}: no cases ran", r.name);
    if !r.failures.is_empty() {
        let shown: Vec<_> = r.failures.iter().take(20).cloned().collect();
        panic!(
            "{}: {} of {} cases differ from ACE:\n  {}",
            r.name,
            r.failures.len(),
            r.total,
            shown.join("\n  ")
        );
    }
}

fn f(v: &Value) -> f32 {
    f32_of(v).unwrap_or_else(|| panic!("not a float: {v}"))
}

fn u(v: &Value) -> u32 {
    u32::try_from(v.as_u64().unwrap_or_else(|| panic!("not a uint: {v}"))).expect("u32")
}

/// A float as the harness writes it (its exact double; NaN and the infinities as tokens).
fn fj(x: f32) -> Value {
    if x.is_nan() {
        json!("NaN")
    } else if x.is_infinite() {
        json!(if x > 0.0 { "Infinity" } else { "-Infinity" })
    } else {
        json!(f64::from(x))
    }
}

/// Numbers compare bit-exactly as floats (both sides are floats written as doubles); the rest
/// structurally.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) if a.is_f64() || b.is_f64() => {
            same_f32(f32_of(a).unwrap_or(f32::NAN), f32_of(b).unwrap_or(f32::NAN))
                && f64_of(a).map(f64::to_bits) == f64_of(b).map(f64::to_bits)
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// The harness's `Pos(p)` object.
fn pos_json(p: &Position) -> Value {
    json!({
        "cell": p.cell(), "x": fj(p.position_x), "y": fj(p.position_y), "z": fj(p.position_z),
        "rw": fj(p.rotation_w), "rx": fj(p.rotation_x), "ry": fj(p.rotation_y), "rz": fj(p.rotation_z),
        "indoors": p.indoors(), "landblock": p.landblock(), "cell_x": p.cell_x(), "cell_y": p.cell_y(),
        "global_cell_x": p.global_cell_x(), "global_cell_y": p.global_cell_y(),
        "to_string": p.to_string(), "to_loc_string": p.to_loc_string(),
    })
}

/// The harness's `Rel(cell, x, y, z, rz, rw)` start position (`relativePos: true`).
fn rel(v: &Value) -> Position {
    Position::from_components(
        u(&v["cell"]),
        f(&v["x"]),
        f(&v["y"]),
        f(&v["z"]),
        0.0,
        0.0,
        f(&v["rz"]),
        f(&v["rw"]),
        true,
    )
}

fn quat_json(q: Quaternion) -> Value {
    json!([fj(q.x), fj(q.y), fj(q.z), fj(q.w)])
}

fn vec3_json(v: Vector3) -> Value {
    json!([fj(v.x), fj(v.y), fj(v.z)])
}

fn exception(v: &Value) -> Value {
    json!({ "throws": v })
}

// ---------------------------------------------------------------------------------- ObjectGuid

#[test]
fn object_guid_matches_ace() {
    replay("object_guid", |case, r| {
        let full = u(&case.input["full"]);
        let o = ObjectGuid::new(full);
        let got = json!({
            "full": o.full(), "low": o.low(), "high": o.high(), "type": format!("{:?}", o.guid_type()),
            "is_player": o.is_player(), "is_static": o.is_static(), "is_dynamic": o.is_dynamic(),
            "static_is_player": ObjectGuid::is_player_guid(full), "static_is_static": ObjectGuid::is_static_guid(full),
            "static_is_dynamic": ObjectGuid::is_dynamic_guid(full), "to_string": o.to_string(),
        });
        r.check(case, same(&got, &case.output), got);
    });
}

// ---------------------------------------------------------------------------------- LandblockId

fn lb_or_throw(f: impl FnOnce() -> LandblockId) -> Value {
    catch(f).map_or_else(
        |()| exception(&json!("System.OverflowException")),
        |l| json!(l.raw()),
    )
}

#[test]
fn landblock_id_matches_ace() {
    replay("landblock_id", |case, r| {
        let id = LandblockId::new(u(&case.input["raw"]));
        let got = json!({
            "landblock": id.landblock(), "landblock_x": id.landblock_x(), "landblock_y": id.landblock_y(),
            "landcell": id.landcell(), "landcell_x": id.landcell_x(), "landcell_y": id.landcell_y(),
            "indoors": id.indoors(), "to_string": id.to_string(),
            "east": lb_or_throw(|| id.east()), "west": lb_or_throw(|| id.west()),
            "north": lb_or_throw(|| id.north()), "south": lb_or_throw(|| id.south()),
            "north_east": lb_or_throw(|| id.north_east()), "north_west": lb_or_throw(|| id.north_west()),
            "south_east": lb_or_throw(|| id.south_east()), "south_west": lb_or_throw(|| id.south_west()),
        });
        r.check(case, same(&got, &case.output), got);
    });
}

#[test]
fn landblock_id_from_xy_matches_ace() {
    replay("landblock_id_from_xy", |case, r| {
        let x = u8::try_from(u(&case.input["x"])).expect("byte");
        let y = u8::try_from(u(&case.input["y"])).expect("byte");
        let got = LandblockId::from_xy(x, y).raw();
        r.check(case, json!(got) == case.output, got);
    });
}

#[test]
fn landblock_id_transition_matches_ace() {
    replay("landblock_id_transition", |case, r| {
        let id = LandblockId::new(u(&case.input["raw"]));
        let off = i32::try_from(case.input["offset"].as_i64().expect("offset")).expect("i32");
        let t = if case.input["axis"] == "x" {
            id.transition_x(off)
        } else {
            id.transition_y(off)
        };
        let got = t.map_or(Value::Null, |l| json!(l.raw()));
        r.check(case, got == case.output, got);
    });
}

#[test]
fn landblock_id_pair_matches_ace() {
    replay("landblock_id_pair", |case, r| {
        let a = LandblockId::new(u(&case.input["a"]));
        let b = LandblockId::new(u(&case.input["b"]));
        let got = json!({ "adjacent": a.is_adjacent_to(b), "eq": a == b, "ne": a != b });
        r.check(case, got == case.output, got);
    });
}

// ---------------------------------------------------------------------------------- Position

#[test]
fn position_new_matches_ace() {
    replay("position_new", |case, r| {
        let i = &case.input;
        let p = Position::from_components(
            u(&i["cell"]),
            f(&i["x"]),
            f(&i["y"]),
            f(&i["z"]),
            f(&i["rx"]),
            f(&i["ry"]),
            f(&i["rz"]),
            f(&i["rw"]),
            i["relative"].as_bool().expect("relative"),
        );
        let got = pos_json(&p);
        r.check(case, same(&got, &case.output), got);
    });
}

#[test]
fn position_from_coords_matches_ace() {
    replay("position_from_coords", |case, r| {
        let got =
            Position::from_coordinates(f(&case.input["north_south"]), f(&case.input["east_west"]))
                .map_or_else(|_| exception(&json!("System.Exception")), |p| pos_json(&p));
        r.check(case, same(&got, &case.output), got);
    });
}

#[test]
fn position_from_vector2_matches_ace() {
    replay("position_from_vector2", |case, r| {
        let v = Vector2::new(f(&case.input["x"]), f(&case.input["y"]));
        let got = catch(|| Position::from_map_coordinates(v))
            .map_or_else(|()| json!({"throws": "?"}), |p| pos_json(&p));
        r.check(
            case,
            throws(&case.output).is_none() && same(&got, &case.output),
            got,
        );
    });
}

#[test]
fn position_distances_match_ace() {
    replay("position_distance", |case, r| {
        let (a, b) = (rel(&case.input["a"]), rel(&case.input["b"]));
        let got = json!({
            "distance_to": fj(a.distance_to(&b)), "distance_2d": fj(a.distance_2d(&b)),
            "distance_2d_squared": fj(a.distance_2d_squared(&b)), "squared_distance_to": fj(a.squared_distance_to(&b)),
            "offset": vec3_json(a.get_offset(&b)),
        });
        r.check(case, same(&got, &case.output), got);
    });
}

#[test]
fn position_in_front_of_matches_ace() {
    replay("position_in_front_of", |case, r| {
        let p = rel(&case.input["start"]);
        let dist = f64_of(&case.input["distance"]).expect("distance");
        let got = pos_json(&p.in_front_of(dist, case.input["rotate180"].as_bool().expect("bool")));
        r.check(
            case,
            throws(&case.output).is_none() && same(&got, &case.output),
            got,
        );
    });
}

/// Position in front of dense matches ace.
#[test]
fn position_in_front_of_dense_matches_ace() {
    replay("position_in_front_of_dense", |case, r| {
        let i = &case.input;
        let p = Position::from_components(
            0x7D64_0012,
            60.0,
            60.0,
            10.0,
            0.0,
            0.0,
            f(&i["rz"]),
            f(&i["rw"]),
            true,
        );
        let dist = f64_of(&i["distance"]).expect("distance");
        let o = p.in_front_of(dist, i["rotate180"].as_bool().expect("bool"));
        let got = json!([
            o.cell(),
            fj(o.position_x),
            fj(o.position_y),
            fj(o.position_z),
            fj(o.rotation_x),
            fj(o.rotation_y),
            fj(o.rotation_z),
            fj(o.rotation_w)
        ]);
        r.check(case, same(&got, &case.output), got);
    });
}

/// Position rotate matches ace.
#[test]
fn position_rotate_matches_ace() {
    replay("position_rotate", |case, r| {
        let mut p = Position::new();
        p.rotate(Vector3::new(f(&case.input["x"]), f(&case.input["y"]), 0.5));
        let got = quat_json(p.rotation());
        r.check(case, same(&got, &case.output), got);
    });
}

/// Position get current dir matches ace.
#[test]
fn position_get_current_dir_matches_ace() {
    replay("position_get_current_dir", |case, r| {
        let q = &case.input["q"];
        let p = Position::from_components(
            0x7D64_0012,
            10.0,
            10.0,
            0.0,
            f(&q[0]),
            f(&q[1]),
            f(&q[2]),
            f(&q[3]),
            true,
        );
        let got = vec3_json(p.get_current_dir());
        r.check(case, same(&got, &case.output), got);
    });
}
