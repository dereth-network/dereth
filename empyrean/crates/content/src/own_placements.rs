//! Not ACE (V565): a world's own placements, the things it puts in its landblocks beside (or, with
//! its own placements only, instead of) the world database's: a table of landblock instances, one
//! a line, tab-separated:
//!
//! ```text
//! guid  wcid  cell  x  y  z  qw  qx  qy  qz  links
//! 0x76E8F000  24037  0x6E8F0100  187  163.87  63.23  0.7071  0  0  0.7071  0x76E8F002
//! ```
//!
//! `links` is empty or the guids of the instance's link children, comma-separated, as ACE's
//! `landblock_instance_link` rows name them; a link child is an instance some other row links to.
//! A portal linked to a `portaldestination` instance takes that instance's place as its
//! destination, as ACE's placed portals do. A line starting `#`, a blank line and the header line
//! (`guid` and a tab) are skipped. Numbers may be written in hex with `0x`.
//!
//! A world's own weenies ([`parse_weenies`]) are the things it places that the world database has
//! no weenie for: each is a weenie of the world database (its base) with some of its properties
//! set otherwise, under a class id and name of its own, one a line, tab-separated:
//!
//! ```text
//! wcid  class_name  base  property  property ...
//! 15728641  kitdoor  278  did.1=0x0200F000  did.2=0x0900F000
//! ```
//!
//! A property is `kind.type=value`, the kind one of `bool`, `int`, `int64`, `float`, `did`, `iid`
//! and `string`, and the type its property's number; a string runs to the end of its field.

use std::collections::BTreeSet;

use crate::models::world::{
    LandblockInstance, LandblockInstanceLink, Weenie, WeeniePropertiesBool, WeeniePropertiesDID,
    WeeniePropertiesFloat, WeeniePropertiesIID, WeeniePropertiesInt, WeeniePropertiesInt64,
    WeeniePropertiesString,
};

/// One property a world's own weenie sets: its type (the property's number) and its value.
#[derive(Debug, Clone, PartialEq)]
pub enum OwnProperty {
    Bool(u16, bool),
    Int(u16, i32),
    Int64(u16, i64),
    Float(u16, f64),
    Did(u16, u32),
    Iid(u16, u32),
    String(u16, String),
}

/// A world's own weenie: `base` with `properties` set, as `class_id` named `class_name`.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnWeenie {
    pub class_id: u32,
    pub class_name: String,
    pub base: u32,
    pub properties: Vec<OwnProperty>,
}

/// Set the row of type `t` in `rows` with `put`, or add the row `new` makes.
fn set_row<R>(
    rows: &mut Vec<R>,
    t: u16,
    ty: impl Fn(&R) -> u16,
    put: impl FnOnce(&mut R),
    new: impl FnOnce() -> R,
) {
    match rows.iter_mut().find(|r| ty(r) == t) {
        Some(r) => put(r),
        None => rows.push(new()),
    }
}

impl OwnWeenie {
    /// The weenie this one is: `base` (its base's weenie) under this one's class id and name, each
    /// of its properties replacing the base's of that type or added after them.
    #[must_use]
    pub fn made_from(&self, base: &Weenie) -> Weenie {
        let mut w = base.clone();
        let id = self.class_id;
        w.class_id = id;
        w.class_name.clone_from(&self.class_name);
        for p in &self.properties {
            match p {
                OwnProperty::Bool(t, v) => set_row(
                    &mut w.weenie_properties_bool,
                    *t,
                    |r| r.r#type,
                    |r| r.value = *v,
                    || WeeniePropertiesBool {
                        object_id: id,
                        r#type: *t,
                        value: *v,
                        ..Default::default()
                    },
                ),
                OwnProperty::Int(t, v) => set_row(
                    &mut w.weenie_properties_int,
                    *t,
                    |r| r.r#type,
                    |r| r.value = *v,
                    || WeeniePropertiesInt {
                        object_id: id,
                        r#type: *t,
                        value: *v,
                        ..Default::default()
                    },
                ),
                OwnProperty::Int64(t, v) => set_row(
                    &mut w.weenie_properties_int64,
                    *t,
                    |r| r.r#type,
                    |r| r.value = *v,
                    || WeeniePropertiesInt64 {
                        object_id: id,
                        r#type: *t,
                        value: *v,
                        ..Default::default()
                    },
                ),
                OwnProperty::Float(t, v) => set_row(
                    &mut w.weenie_properties_float,
                    *t,
                    |r| r.r#type,
                    |r| r.value = *v,
                    || WeeniePropertiesFloat {
                        object_id: id,
                        r#type: *t,
                        value: *v,
                        ..Default::default()
                    },
                ),
                OwnProperty::Did(t, v) => set_row(
                    &mut w.weenie_properties_did,
                    *t,
                    |r| r.r#type,
                    |r| r.value = *v,
                    || WeeniePropertiesDID {
                        object_id: id,
                        r#type: *t,
                        value: *v,
                        ..Default::default()
                    },
                ),
                OwnProperty::Iid(t, v) => set_row(
                    &mut w.weenie_properties_iid,
                    *t,
                    |r| r.r#type,
                    |r| r.value = *v,
                    || WeeniePropertiesIID {
                        object_id: id,
                        r#type: *t,
                        value: *v,
                        ..Default::default()
                    },
                ),
                OwnProperty::String(t, v) => set_row(
                    &mut w.weenie_properties_string,
                    *t,
                    |r| r.r#type,
                    |r| r.value.clone_from(v),
                    || WeeniePropertiesString {
                        object_id: id,
                        r#type: *t,
                        value: v.clone(),
                        ..Default::default()
                    },
                ),
            }
        }
        w
    }
}

/// The weenies a world's own weenies table names.
///
/// # Errors
/// A line that does not read (naming it), a class id given twice, or a property whose kind, type
/// or value does not read.
pub fn parse_weenies(text: &str) -> Result<Vec<OwnWeenie>, String> {
    let mut out: Vec<OwnWeenie> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("wcid\t") {
            continue;
        }
        let bad = |what: &str| format!("weenies line {}: {what}: `{line}`", n + 1);
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 {
            return Err(bad("expected a class id, a class name and a base"));
        }
        let int = |s: &str| {
            let s = s.trim();
            match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(h) => u32::from_str_radix(h, 16),
                None => s.parse::<u32>(),
            }
            .map_err(|_| bad(&format!("`{s}` is not a number")))
        };
        let class_name = f[1].trim();
        if class_name.is_empty() {
            return Err(bad("the class name is empty"));
        }
        let mut properties = Vec::new();
        for field in f.iter().skip(3).filter(|s| !s.trim().is_empty()) {
            properties
                .push(property(field).ok_or_else(|| bad(&format!("`{field}` is not a property")))?);
        }
        let w = OwnWeenie {
            class_id: int(f[0])?,
            class_name: class_name.to_owned(),
            base: int(f[2])?,
            properties,
        };
        if out.iter().any(|o| o.class_id == w.class_id) {
            return Err(format!("weenies: class id {} is given twice", w.class_id));
        }
        out.push(w);
    }
    Ok(out)
}

/// One `kind.type=value` field of a weenies table.
fn property(field: &str) -> Option<OwnProperty> {
    let (key, value) = field.split_once('=')?;
    let (kind, t) = key.trim().split_once('.')?;
    let t = t.parse::<u16>().ok()?;
    let v = value.trim();
    let whole = |s: &str| match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => s.parse::<i64>().ok(),
    };
    Some(match kind {
        "bool" => OwnProperty::Bool(t, v.parse().ok()?),
        "int" => OwnProperty::Int(t, i32::try_from(whole(v)?).ok()?),
        "int64" => OwnProperty::Int64(t, whole(v)?),
        "float" => OwnProperty::Float(t, v.parse().ok()?),
        "did" => OwnProperty::Did(t, u32::try_from(whole(v)?).ok()?),
        "iid" => OwnProperty::Iid(t, u32::try_from(whole(v)?).ok()?),
        "string" => OwnProperty::String(t, value.to_owned()),
        _ => return None,
    })
}

/// The instances a placements table names.
///
/// # Errors
/// A line that does not read (naming it), a guid outside the static range ACE's world database
/// keeps (below `0x8000_0000`), a guid given twice, or a link to a guid no line gives.
pub fn parse(text: &str) -> Result<Vec<LandblockInstance>, String> {
    let mut out: Vec<LandblockInstance> = Vec::new();
    let mut links: Vec<(usize, Vec<u32>)> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("guid\t") {
            continue;
        }
        let bad = |what: &str| format!("placements line {}: {what}: `{line}`", n + 1);
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 10 && f.len() != 11 {
            return Err(bad("expected 10 or 11 tab-separated fields"));
        }
        let int = |s: &str| {
            let s = s.trim();
            match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(h) => u32::from_str_radix(h, 16),
                None => s.parse::<u32>(),
            }
            .map_err(|_| bad(&format!("`{s}` is not a number")))
        };
        let num = |s: &str| {
            s.trim()
                .parse::<f32>()
                .map_err(|_| bad(&format!("`{}` is not a number", s.trim())))
        };
        let guid = int(f[0])?;
        if guid >= 0x8000_0000 {
            return Err(bad("a guid outside the static range"));
        }
        let cell = int(f[2])?;
        let children = match f.get(10).map(|s| s.trim()) {
            None | Some("") => Vec::new(),
            Some(s) => s
                .split(',')
                .map(int)
                .collect::<Result<Vec<u32>, String>>()?,
        };
        links.push((out.len(), children));
        out.push(LandblockInstance {
            guid,
            landblock: Some(i32::try_from(cell >> 16).unwrap_or(0)),
            weenie_class_id: int(f[1])?,
            obj_cell_id: cell,
            origin_x: num(f[3])?,
            origin_y: num(f[4])?,
            origin_z: num(f[5])?,
            angles_w: num(f[6])?,
            angles_x: num(f[7])?,
            angles_y: num(f[8])?,
            angles_z: num(f[9])?,
            ..LandblockInstance::default()
        });
    }
    let mut seen = BTreeSet::new();
    for i in &out {
        if !seen.insert(i.guid) {
            return Err(format!("placements: guid 0x{:08X} is given twice", i.guid));
        }
    }
    let mut next_link = 1;
    for (k, children) in links {
        for child in children {
            let Some(c) = out.iter_mut().find(|i| i.guid == child) else {
                return Err(format!(
                    "placements: 0x{:08X} links to 0x{child:08X}, which no line gives",
                    out[k].guid
                ));
            };
            c.is_link_child = true;
            let parent = out[k].guid;
            out[k].landblock_instance_link.push(LandblockInstanceLink {
                id: next_link,
                parent_guid: parent,
                child_guid: child,
                ..LandblockInstanceLink::default()
            });
            next_link += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this server's own file format, the world's own placements)
    use super::*;

    const TABLE: &str = "guid\twcid\tcell\tx\ty\tz\tqw\tqx\tqy\tqz\tlinks\n\
        # the hall's portal, to the spot at the landing\n\
        0x76E8F000\t24037\t0x6E8F0100\t187\t163.87\t63.23\t0.7071\t0\t0\t0.7071\t0x76E8F002\n\
        0x76E8F002\t10762\t0x6E8F017C\t182.8\t55\t18.4\t0.7071\t0\t0\t0.7071\n";

    #[test]
    fn a_placements_table_reads_as_landblock_instances_with_their_links() {
        let v = parse(TABLE).expect("reads");
        assert_eq!(v.len(), 2);
        let (portal, spot) = (&v[0], &v[1]);
        assert_eq!(
            (portal.guid, portal.weenie_class_id, portal.obj_cell_id),
            (0x76E8_F000, 24037, 0x6E8F_0100)
        );
        assert_eq!(portal.landblock, Some(0x6E8F));
        assert!(
            (portal.origin_y - 163.87).abs() < 1e-4
                && portal.angles_z > 0.7
                && portal.angles_w > 0.7
        );
        assert!(!portal.is_link_child && spot.is_link_child);
        assert_eq!(portal.landblock_instance_link.len(), 1);
        assert_eq!(portal.landblock_instance_link[0].child_guid, 0x76E8_F002);
        assert_eq!(portal.landblock_instance_link[0].parent_guid, 0x76E8_F000);
    }

    #[test]
    fn a_placements_table_with_a_bad_line_a_dynamic_guid_or_a_missing_link_is_refused() {
        assert!(parse("0x76E8F000\t24037\t0x6E8F0100\t1\t2\n").is_err());
        assert!(parse("0x86E8F000\t24037\t0x6E8F0100\t1\t2\t3\t1\t0\t0\t0\n").is_err());
        assert!(parse("0x76E8F000\t24037\t0x6E8F0100\t1\t2\t3\t1\t0\t0\t0\t0x76E8F009\n").is_err());
        let twice = "0x76E8F000\t1\t0x6E8F0100\t1\t2\t3\t1\t0\t0\t0\n";
        assert!(parse(&format!("{twice}{twice}")).is_err());
    }

    #[test]
    fn a_weenies_table_reads_as_bases_with_their_own_properties() {
        let v = parse_weenies(
            "wcid\tclass_name\tbase\tproperties\n\
             # the door\n\
             15728641\tkitdoor\t278\tdid.1=0x0200F000\tdid.2=0x0900F000\tfloat.11=300\tbool.1=true\tstring.1=Kit door\n\
             0xF00002\tbare\t1\n",
        )
        .expect("reads");
        assert_eq!(v.len(), 2);
        assert_eq!(
            (v[0].class_id, v[0].class_name.as_str(), v[0].base),
            (15_728_641, "kitdoor", 278)
        );
        assert_eq!(
            v[0].properties,
            [
                OwnProperty::Did(1, 0x0200_F000),
                OwnProperty::Did(2, 0x0900_F000),
                OwnProperty::Float(11, 300.0),
                OwnProperty::Bool(1, true),
                OwnProperty::String(1, "Kit door".into()),
            ]
        );
        assert_eq!((v[1].class_id, v[1].properties.len()), (0xF0_0002, 0));
    }

    #[test]
    fn a_weenies_table_with_a_bad_property_a_missing_base_or_a_class_id_twice_is_refused() {
        assert!(parse_weenies("1\tkitdoor\t278\tdid.1\n").is_err());
        assert!(parse_weenies("1\tkitdoor\t278\tcolor.1=3\n").is_err());
        assert!(parse_weenies("1\tkitdoor\t278\tint.1=x\n").is_err());
        assert!(parse_weenies("1\tkitdoor\n").is_err());
        assert!(parse_weenies("1\tkitdoor\t278\n1\tother\t278\n").is_err());
    }

    #[test]
    fn an_own_weenie_is_its_base_with_its_properties_set_or_added_under_its_own_class() {
        let base = Weenie {
            class_id: 278,
            class_name: "door".into(),
            r#type: 19,
            weenie_properties_did: vec![
                WeeniePropertiesDID {
                    object_id: 278,
                    r#type: 1,
                    value: 0x0200_024F,
                    ..Default::default()
                },
                WeeniePropertiesDID {
                    object_id: 278,
                    r#type: 3,
                    value: 0x2000_0022,
                    ..Default::default()
                },
            ],
            ..Weenie::default()
        };
        let own = OwnWeenie {
            class_id: 900,
            class_name: "kitdoor".into(),
            base: 278,
            properties: vec![
                OwnProperty::Did(1, 0x0200_F000),
                OwnProperty::Did(2, 0x0900_F000),
            ],
        };
        let w = own.made_from(&base);
        assert_eq!(
            (w.class_id, w.class_name.as_str(), w.r#type),
            (900, "kitdoor", 19)
        );
        let dids: Vec<(u16, u32)> = w
            .weenie_properties_did
            .iter()
            .map(|r| (r.r#type, r.value))
            .collect();
        assert_eq!(dids, [(1, 0x0200_F000), (3, 0x2000_0022), (2, 0x0900_F000)]);
    }
}
