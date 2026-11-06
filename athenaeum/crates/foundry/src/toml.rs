//! Rows written as the world database writes them: data ids in hexadecimal, places in meters,
//! provenance as an inline table, one `[[kind]]` table a row.

use toml_edit::{Array, InlineTable, Item, Table, Value};
use yalain_world_db::rows::{
    At, Camp, Clearing, Gate, Landing, Object, ObjectKind, Place, Prov, Route, StrRef,
};

fn parse(text: &str) -> Value {
    text.parse::<Value>()
        .unwrap_or_else(|_| Value::from(text.to_string()))
}

/// A data id, as `0x47002224`.
#[must_use]
pub fn hex(v: u32) -> Value {
    parse(&format!("0x{v:08X}"))
}

/// A world id, as `0x4077B70000000001`.
#[must_use]
pub fn hex64(v: u64) -> Value {
    parse(&format!("0x{v:016X}"))
}

/// A number of meters or degrees, to four places, without trailing zeros.
#[must_use]
pub fn number_text(v: f32) -> String {
    let mut s = format!("{:.4}", f64::from(v));
    while s.ends_with('0') && !s.ends_with(".0") {
        s.pop();
    }
    if s == "-0.0" {
        s = "0.0".into();
    }
    s
}

/// A number, as [`number_text`] writes it.
#[must_use]
pub fn number(v: f32) -> Value {
    parse(&number_text(v))
}

/// A point `[x, y, z]`, z `"ground"` where it is the land's height.
#[must_use]
pub fn point(at: At) -> Value {
    let z = at.z.map_or_else(|| "\"ground\"".to_string(), number_text);
    parse(&format!(
        "[{}, {}, {z}]",
        number_text(at.x),
        number_text(at.y)
    ))
}

fn quat(q: [f32; 4]) -> Value {
    parse(&format!(
        "[{}, {}, {}, {}]",
        number_text(q[0]),
        number_text(q[1]),
        number_text(q[2]),
        number_text(q[3])
    ))
}

fn strref(r: StrRef) -> Value {
    parse(&format!(
        "{{ table = 0x{:08X}, string = 0x{:08X} }}",
        r.table, r.string
    ))
}

fn ids(v: &[u32]) -> Value {
    let mut a = Array::new();
    for id in v {
        a.push(hex(*id));
    }
    Value::Array(a)
}

/// A provenance as an inline table.
#[must_use]
pub fn prov(p: &Prov) -> Value {
    let mut t = InlineTable::new();
    t.insert("from", Value::from(p.source.word()));
    if let Some(e) = &p.evidence {
        t.insert("evidence", Value::from(e.as_str()));
    }
    if let Some(r) = &p.reference {
        t.insert("ref", Value::from(r.as_str()));
    }
    if let Some(c) = p.confidence {
        t.insert("confidence", Value::from(c.word()));
    }
    if !p.sample.is_empty() {
        let mut s = InlineTable::new();
        for (k, n) in &p.sample {
            s.insert(k, Value::from(i64::from(*n)));
        }
        t.insert("sample", Value::InlineTable(s));
    }
    if let Some(r) = p.record {
        t.insert("record", hex(r));
    }
    if let Some(r) = &p.rule {
        t.insert("rule", Value::from(r.as_str()));
    }
    if let Some(b) = &p.by {
        t.insert("by", Value::from(b.as_str()));
    }
    if let Some(n) = &p.note {
        t.insert("note", Value::from(n.as_str()));
    }
    if !p.fields.is_empty() {
        let mut f = InlineTable::new();
        for (k, v) in &p.fields {
            f.insert(k, prov(v));
        }
        t.insert("fields", Value::InlineTable(f));
    }
    Value::InlineTable(t)
}

fn put(t: &mut Table, key: &str, v: Value) {
    t.insert(key, Item::Value(v));
}

/// A reference to a row of another pack: `pack:key`, unless it names a pack already.
#[must_use]
pub fn qualified(reference: &str, pack: &str) -> String {
    if reference.contains(':') {
        reference.to_string()
    } else {
        format!("{pack}:{reference}")
    }
}

/// `[[object]]`; `to` as given (qualify it first when the row moves to another pack).
#[must_use]
pub fn object(o: &Object, to: Option<&str>, p: &Prov) -> Table {
    let mut t = Table::new();
    put(&mut t, "key", Value::from(o.key.as_str()));
    put(
        &mut t,
        "kind",
        Value::from(match o.kind {
            ObjectKind::Npc => "npc",
            ObjectKind::Static => "static",
        }),
    );
    if let Some(id) = o.id {
        put(&mut t, "id", hex64(id));
    }
    put(&mut t, "template", hex(o.template));
    if let Some(c) = o.cell {
        put(&mut t, "cell", hex(c));
    }
    put(&mut t, "at", point(o.at));
    if let Some(q) = o.rotation {
        put(&mut t, "rotation", quat(q));
    } else if let Some(h) = o.heading {
        put(&mut t, "heading", number(h));
    }
    if let Some(s) = o.scale {
        put(&mut t, "scale", number(s));
    }
    if let Some(n) = o.name {
        put(&mut t, "name", strref(n));
    }
    if let Some(n) = &o.name_text {
        put(&mut t, "name_text", Value::from(n.as_str()));
    }
    if let Some(to) = to {
        put(&mut t, "to", Value::from(to));
    }
    if !o.landings.is_empty() {
        put(&mut t, "landings", landings(&o.landings));
    }
    if let Some(scene) = o.scene {
        put(&mut t, "scene", hex(scene));
    }
    if o.locked {
        put(&mut t, "locked", Value::from(true));
    }
    if let Some(k) = o.lock {
        put(&mut t, "lock", hex(k));
    }
    if let Some(r) = &o.route {
        put(&mut t, "route", Value::from(r.as_str()));
        if o.route_start != 0 {
            put(&mut t, "route_start", Value::from(i64::from(o.route_start)));
        }
        if o.route_backward {
            put(&mut t, "route_direction", Value::from("backward"));
        }
    }
    if let Some((quest, gate)) = o.requires {
        let mut g = InlineTable::new();
        g.insert("quest", hex(quest));
        match gate {
            Gate::Done => g.insert("done", Value::from(true)),
            Gate::UnderWay => g.insert("underway", Value::from(true)),
            Gate::Phase(n) => g.insert("phase", Value::from(i64::from(n))),
        };
        put(&mut t, "requires", Value::InlineTable(g));
    }
    if o.recall {
        put(&mut t, "recall", Value::from(true));
    }
    if o.summon {
        put(&mut t, "summon", Value::from(true));
    }
    put(&mut t, "prov", prov(p));
    t
}

fn landings(ls: &[Landing]) -> Value {
    let mut a = Array::new();
    for l in ls {
        let mut t = InlineTable::new();
        t.insert("cell", hex(l.cell));
        t.insert("at", point(l.at));
        if let Some(h) = l.heading {
            t.insert("heading", number(h));
        }
        if let Some(s) = l.seen {
            t.insert("seen", Value::from(i64::from(s)));
        }
        a.push(Value::InlineTable(t));
    }
    Value::Array(a)
}

/// `[[camp]]`; `parent` as given.
#[must_use]
pub fn camp(c: &Camp, parent: Option<&str>, p: &Prov) -> Table {
    let mut t = Table::new();
    put(&mut t, "key", Value::from(c.key.as_str()));
    if let Some(cell) = c.cell {
        put(&mut t, "cell", hex(cell));
    }
    put(&mut t, "at", point(c.at));
    put(&mut t, "radius", number(c.radius));
    if !c.spots.is_empty() {
        let mut a = Array::new();
        for (cell, at) in &c.spots {
            let mut s = InlineTable::new();
            s.insert("cell", hex(*cell));
            s.insert("at", point(*at));
            a.push(Value::InlineTable(s));
        }
        put(&mut t, "spots", Value::Array(a));
    }
    put(&mut t, "population", Value::from(i64::from(c.population)));
    #[allow(clippy::cast_possible_truncation)]
    put(&mut t, "regen", number(c.regen as f32));
    let mut a = Array::new();
    for (tpl, w) in &c.spawns {
        let mut s = InlineTable::new();
        s.insert("template", hex(*tpl));
        s.insert("weight", Value::from(i64::from(*w)));
        a.push(Value::InlineTable(s));
    }
    put(&mut t, "spawns", Value::Array(a));
    if let Some(v) = c.profile {
        put(&mut t, "profile", parse(&format!("0x{v:X}")));
    }
    if let Some(n) = &c.profile_name {
        put(&mut t, "profile_name", Value::from(n.as_str()));
    }
    if let Some(parent) = parent {
        put(&mut t, "parent", Value::from(parent));
    }
    let m = &c.moves;
    if let Some(k) = m.kind {
        put(&mut t, "movement", Value::from(k.word()));
    }
    if let Some(w) = m.wanders {
        put(&mut t, "wanders", Value::from(w));
    }
    for (name, v) in [
        ("wander_range", m.wander_range),
        ("wander_chance", m.wander_chance),
        ("homesick", m.homesick),
        ("perception", m.perception),
    ] {
        if let Some(v) = v {
            put(&mut t, name, number(v));
        }
    }
    if let Some(d) = m.uses_doors {
        put(&mut t, "uses_doors", Value::from(d));
    }
    if let Some(r) = &c.route {
        put(&mut t, "route", Value::from(r.as_str()));
        if c.route_start != 0 {
            put(&mut t, "route_start", Value::from(i64::from(c.route_start)));
        }
        if c.route_backward {
            put(&mut t, "route_direction", Value::from("backward"));
        }
    }
    put(&mut t, "prov", prov(p));
    t
}

/// `[[route]]`.
#[must_use]
pub fn route(r: &Route, p: &Prov) -> Table {
    let mut t = Table::new();
    put(&mut t, "key", Value::from(r.key.as_str()));
    let mut a = Array::new();
    for w in &r.waypoints {
        let mut s = InlineTable::new();
        s.insert("cell", hex(w.cell));
        s.insert("at", point(w.at));
        if let Some(rest) = w.rest {
            #[allow(clippy::cast_possible_truncation)]
            s.insert("rest", number(rest as f32));
        }
        a.push(Value::InlineTable(s));
    }
    put(&mut t, "waypoints", Value::Array(a));
    put(&mut t, "shape", Value::from(r.shape.word()));
    put(&mut t, "gait", Value::from(r.gait.word()));
    if let Some(v) = r.speed {
        put(&mut t, "speed", number(v));
    }
    if r.rest > 0.0 {
        #[allow(clippy::cast_possible_truncation)]
        put(&mut t, "rest", number(r.rest as f32));
    }
    put(&mut t, "prov", prov(p));
    t
}

/// `[[clearing]]`.
#[must_use]
pub fn clearing(c: &Clearing, p: &Prov) -> Table {
    let mut t = Table::new();
    put(&mut t, "key", Value::from(c.key.as_str()));
    put(&mut t, "at", point(At { z: None, ..c.at }));
    if let Some(r) = c.radius {
        put(&mut t, "radius", number(r));
    }
    if let Some([x, y]) = c.size {
        let mut a = Array::new();
        a.push(number(x));
        a.push(number(y));
        put(&mut t, "size", Value::Array(a));
    }
    if let Some(h) = c.heading {
        put(&mut t, "heading", number(h));
    }
    put(&mut t, "prov", prov(p));
    t
}

/// `[[place]]`.
#[must_use]
pub fn place(pl: &Place, p: &Prov) -> Table {
    let mut t = Table::new();
    put(&mut t, "key", Value::from(pl.key.as_str()));
    if let Some(c) = pl.cell {
        put(&mut t, "cell", hex(c));
    }
    put(&mut t, "at", point(pl.at));
    if let Some(q) = pl.rotation {
        put(&mut t, "rotation", quat(q));
    } else if let Some(h) = pl.heading {
        put(&mut t, "heading", number(h));
    }
    if !pl.named_by.is_empty() {
        put(&mut t, "named_by", ids(&pl.named_by));
    }
    if let Some(d) = &pl.destination {
        put(&mut t, "destination", Value::from(d.as_str()));
    }
    put(&mut t, "prov", prov(p));
    t
}
