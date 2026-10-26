//! Reading VTank-format navigation routes (`.nav` files), for `dereth.nav`'s import.
//!
//! The format is plain text, one value a line (the layout as UtilityBelt's MIT-licensed route
//! reader reads it; see `docs/plugins/THIRD-PARTY.md`):
//!
//! ```text
//! uTank2 NAV 1.2        header
//! 1                     route kind: 1 circular, 2 linear, 3 follow, 4 once
//! 3                     waypoints (a follow route has a name and an object id instead)
//! 0                     the first waypoint's type, then its lines
//! …
//! ```
//!
//! Every waypoint begins with its place: east-west and north-south map coordinates, the height
//! in map units (240 m), and one unused line. Then, by type:
//!
//! | type | name | more lines |
//! |---|---|---|
//! | 0 | point | none |
//! | 1 | portal (by object) | the portal's object id |
//! | 6 | portal | the portal's name, its object class, an unused line, its exit's north-south, east-west and height |
//! | 2 | recall | the recall spell's id |
//! | 3 | pause | milliseconds |
//! | 4 | chat | the line to say or command to type |
//! | 5 | vendor | the vendor's object id, its name |
//! | 7 | use_npc | the NPC's name, its object class, an unused line, its east-west, north-south and height |
//! | 8 | checkpoint | none |
//! | 9 | jump | heading, whether shift is held, milliseconds |
//!
//! Any other type ends the reading with an error: its lines cannot be skipped without knowing them.

use dereth_plugin_api::Value;

/// A route read into the value `dereth.nav` works with: `{ kind, points = { … } }` or, for a
/// follow route, `{ kind = "follow", target = name, target_id = id }`.
///
/// # Errors
/// Not a route, or a line that does not read as its field.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut lines = text.lines().map(str::trim);
    let mut next = |what: &str| {
        lines
            .next()
            .ok_or_else(|| format!("the file ends before {what}"))
    };
    let header = next("the header")?;
    if !header.starts_with("uTank2 NAV") {
        return Err(format!("{header:?} is not a route's header"));
    }
    let num = |s: &str, what: &str| -> Result<f64, String> {
        s.parse::<f64>()
            .map_err(|_| format!("{s:?} is not a number ({what})"))
    };
    let int = |s: &str, what: &str| -> Result<i64, String> {
        s.parse::<i64>()
            .map_err(|_| format!("{s:?} is not a whole number ({what})"))
    };
    let kind = match int(next("the route kind")?, "the route kind")? {
        1 => "circular",
        2 => "linear",
        3 => {
            let name = next("the target's name")?.to_owned();
            let id = int(next("the target's id")?, "the target's id")?;
            return Ok(Value::map()
                .with("kind", "follow")
                .with("target", name)
                .with("target_id", id));
        }
        4 => "once",
        k => return Err(format!("{k} is not a route kind")),
    };
    let count = int(next("the waypoint count")?, "the waypoint count")?;
    if !(0..=100_000).contains(&count) {
        return Err(format!("{count} waypoints is not a route"));
    }
    let mut points = Vec::new();
    for i in 0..count {
        let what = format!("waypoint {}", i + 1);
        let ty = int(next(&what)?, &what)?;
        let ew = num(next(&what)?, "east-west")?;
        let ns = num(next(&what)?, "north-south")?;
        let z = num(next(&what)?, "height")?;
        next(&what)?;
        let position = Value::map()
            .with("ns", ns)
            .with("ew", ew)
            .with("z", z * crate::places::UNIT);
        let mut p = Value::map().with("position", position);
        let place = |ns: f64, ew: f64, z: f64| {
            Value::map()
                .with("ns", ns)
                .with("ew", ew)
                .with("z", z * crate::places::UNIT)
        };
        p = match ty {
            0 => p.with("kind", "point"),
            8 => p.with("kind", "checkpoint"),
            // The older portal waypoint names the portal by its object id alone, as the public
            // routes that carry one show.
            1 => p
                .with("kind", "portal")
                .with("id", int(next(&what)?, "portal object id")?),
            6 => {
                let name = next(&what)?.to_owned();
                let class = int(next(&what)?, "object class")?;
                next(&what)?;
                let ns = num(next(&what)?, "exit north-south")?;
                let ew = num(next(&what)?, "exit east-west")?;
                let z = num(next(&what)?, "exit height")?;
                p.with("kind", "portal")
                    .with("name", name)
                    .with("class", class)
                    .with("exit", place(ns, ew, z))
            }
            2 => p
                .with("kind", "recall")
                .with("spell", int(next(&what)?, "recall spell")?),
            3 => p
                .with("kind", "pause")
                .with("ms", int(next(&what)?, "pause")?),
            4 => p.with("kind", "chat").with("text", next(&what)?.to_owned()),
            5 => {
                let id = int(next(&what)?, "vendor id")?;
                p.with("kind", "vendor")
                    .with("id", id)
                    .with("name", next(&what)?.to_owned())
            }
            7 => {
                let name = next(&what)?.to_owned();
                let class = int(next(&what)?, "object class")?;
                next(&what)?;
                let ew = num(next(&what)?, "NPC east-west")?;
                let ns = num(next(&what)?, "NPC north-south")?;
                let z = num(next(&what)?, "NPC height")?;
                p.with("kind", "use_npc")
                    .with("name", name)
                    .with("class", class)
                    .with("at", place(ns, ew, z))
            }
            9 => {
                let heading = num(next(&what)?, "jump heading")?;
                let shift = next(&what)?.eq_ignore_ascii_case("true");
                let ms = num(next(&what)?, "jump time")?;
                p.with("kind", "jump")
                    .with("heading", heading)
                    .with("shift", shift)
                    .with("ms", ms)
            }
            other => return Err(format!("{what}: type {other} is not one this reader knows")),
        };
        points.push(p);
    }
    Ok(Value::map()
        .with("kind", kind)
        .with("points", Value::List(points)))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn a_route_reads_its_kind_and_each_waypoint_by_type() {
        let text = "uTank2 NAV 1.2\n2\n3\n0\n33.8\n42.2\n0.05\n0\n3\n33.9\n42.3\n0\n0\n1500\n\
                    4\n34\n42.4\n0\n0\n/say hi\n";
        let r = parse(text).expect("reads");
        assert_eq!(r.get("kind").as_str(), Some("linear"));
        let pts = r.get("points").as_list().expect("points");
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0].get("kind").as_str(), Some("point"));
        assert_eq!(pts[0].get("position").get("ns").as_f64(), Some(42.2));
        assert!((pts[0].get("position").get("z").as_f64().unwrap_or(0.0) - 12.0).abs() < 1e-9);
        assert_eq!(pts[1].get("ms").as_i64(), Some(1500));
        assert_eq!(pts[2].get("text").as_str(), Some("/say hi"));
    }

    #[test]
    fn an_older_portal_waypoint_names_its_portal_by_object_id() {
        let text = "uTank2 NAV 1.2
4
2
2
95.4
2.3
0.24
0
2931
                    1
95.4
2.3
0.24
0
2137530392
";
        let r = parse(text).expect("reads");
        let pts = r.get("points").as_list().expect("points");
        assert_eq!(pts[0].get("spell").as_i64(), Some(2931));
        assert_eq!(pts[1].get("kind").as_str(), Some("portal"));
        assert_eq!(pts[1].get("id").as_i64(), Some(2_137_530_392));
    }

    #[test]
    fn a_follow_route_names_its_target_and_a_broken_file_says_where() {
        let r = parse("uTank2 NAV 1.2\n3\nLeader\n1342177281\n").expect("reads");
        assert_eq!(r.get("target").as_str(), Some("Leader"));
        assert!(parse("uTank2 NAV 1.2\n1\n1\n0\n1\n").is_err());
        assert!(parse("not a route").is_err());
        assert!(parse("uTank2 NAV 1.2\n1\n1\n99\n0\n0\n0\n0\n").is_err());
    }
}
