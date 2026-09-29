//! Vectors: empyrean/fixtures/vectors/enums
//! Every generated enum's GetValues order, GetName and ToString (and aliased Flags pair ORs)
//! equal net10 vectors; alias lookup rules.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use std::collections::HashMap;

use empyrean_common::vectors::{self, u64_of};
use empyrean_entity::enums::*;
use serde_json::Value;

use super::enum_list::for_each_ace_enum;

fn strings(v: &Value) -> Vec<Option<String>> {
    v.as_array()
        .expect("an array")
        .iter()
        .map(|s| s.as_str().map(str::to_owned))
        .collect()
}

fn keys(v: &Value) -> Vec<u64> {
    v.as_array()
        .expect("an array")
        .iter()
        .map(|k| u64_of(k).expect("a key"))
        .collect()
}

/// One enum's generated tables and lookups, by key.
struct Probe {
    names: Vec<String>,
    keys: Vec<u64>,
    get_name: Vec<Option<String>>,
    to_string: Vec<Option<String>>,
    to_string_of: fn(u64) -> String,
}

fn probe<E: AceEnum>(to_string_of: fn(u64) -> String) -> Probe {
    let mut distinct = Vec::new();
    let mut get_name = Vec::new();
    let mut to_string = Vec::new();
    for &m in E::MEMBERS {
        if !distinct.contains(&m.key()) {
            distinct.push(m.key());
            get_name.push(m.name().map(str::to_owned));
            to_string.push(Some(m.to_dotnet_string()));
        }
    }
    Probe {
        names: E::MEMBER_NAMES.iter().map(|&n| n.to_owned()).collect(),
        keys: E::MEMBERS.iter().map(|m| m.key()).collect(),
        get_name,
        to_string,
        to_string_of,
    }
}

fn probes() -> HashMap<&'static str, Probe> {
    let mut all = HashMap::new();
    macro_rules! add {
        ($t:ident) => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            all.insert(
                stringify!($t),
                probe::<$t>(|k| $t(k as _).to_dotnet_string()),
            );
        };
    }
    for_each_ace_enum!(add);
    all
}

/// One divergence-adjusted run: the enum, ACE's members and the retail members that replace
/// them, as `(name, key)`.
type RuledRun = (
    &'static str,
    &'static [(&'static str, u64)],
    &'static [(&'static str, u64)],
);

/// The member runs that follow the retail client (the generator's
/// `RETAIL_RULED_RUNS`). The `alias_order` vectors are recorded from ACE's own enums, so each run
/// is swapped for the client's in the expected tables before comparing.
///
/// `MotionCommand` (V331): ACE numbers the UI target-selection block the 2013 way; the
/// client has `CombatEat`/`CombatDrink` at 0x10000110/1 and the block at 0x09000112..0x09000117.
///
/// `ItemType` (V327): ACE numbers `CraftFletchingBase` 0x2000000; the client's fletching
/// base is 0x1000000, and the client names nothing at 0x2000000.
///
/// `CharacterOptions2` (V384): ACE's `Default` is 0x948700; the target client's is 0x2948700.
const RETAIL_RULED_RUNS: &[RuledRun] = &[
    (
        "CharacterOptions2",
        &[("Default", 0x0094_8700)],
        &[("Default", 0x0294_8700)],
    ),
    (
        "ItemType",
        &[("CraftFletchingBase", 0x0200_0000)],
        &[("CraftFletchingBase", 0x0100_0000)],
    ),
    (
        "MotionCommand",
        &[
            ("PreviousMonster", 0x0900_0110),
            ("ClosestMonster", 0x0900_0111),
            ("NextPlayer", 0x0900_0112),
            ("PreviousPlayer", 0x0900_0113),
            ("ClosestPlayer", 0x0900_0114),
        ],
        &[
            ("CombatEat", 0x1000_0110),
            ("CombatDrink", 0x1000_0111),
            ("NextMonster", 0x0900_0112),
            ("PreviousMonster", 0x0900_0113),
            ("ClosestMonster", 0x0900_0114),
            ("NextPlayer", 0x0900_0115),
            ("PreviousPlayer", 0x0900_0116),
            ("ClosestPlayer", 0x0900_0117),
        ],
    ),
];

/// .NET's tables for one enum: `GetValues` names and keys, and `GetName` / `ToString` per
/// distinct key.
struct Expected {
    names: Vec<String>,
    keys: Vec<u64>,
    get_name: Vec<Option<String>>,
    to_string: Vec<Option<String>>,
}

/// `exp` with the enum's ruled runs replaced by the retail members (plain members, so `GetName` and
/// `ToString` of each are its name), still in `GetValues` (ascending key) order. Each of ACE's
/// members must be in the vectors exactly once, so a re-recorded ACE that moved them fails here.
fn retail_ruled(name: &str, mut exp: Expected) -> Expected {
    for &(_, ace, retail) in RETAIL_RULED_RUNS.iter().filter(|r| r.0 == name) {
        let mut members: Vec<(String, u64)> = exp.names.into_iter().zip(exp.keys).collect();
        let mut distinct: Vec<u64> = Vec::new();
        for &(_, k) in &members {
            if !distinct.contains(&k) {
                distinct.push(k);
            }
        }
        let mut per_key: Vec<(u64, Option<String>, Option<String>)> = distinct
            .into_iter()
            .zip(exp.get_name)
            .zip(exp.to_string)
            .map(|((k, g), t)| (k, g, t))
            .collect();
        for &(n, k) in ace {
            let hits = members.iter().filter(|m| m.1 == k).count();
            assert_eq!(hits, 1, "{name}.{n}: ACE's vectors hold {k:#x} once");
            let at = members
                .iter()
                .position(|m| m.0 == n && m.1 == k)
                .unwrap_or_else(|| panic!("{name}.{n} = {k:#x} in ACE's vectors"));
            members.remove(at);
            per_key.retain(|p| p.0 != k);
        }
        for &(n, k) in retail {
            assert!(
                members.iter().all(|m| m.1 != k),
                "{name}.{n}: {k:#x} is free in ACE's enum"
            );
            members.push((n.to_owned(), k));
            per_key.push((k, Some(n.to_owned()), Some(n.to_owned())));
        }
        members.sort_by_key(|m| m.1);
        per_key.sort_by_key(|p| p.0);
        exp = Expected {
            names: members.iter().map(|m| m.0.clone()).collect(),
            keys: members.iter().map(|m| m.1).collect(),
            get_name: per_key.iter().map(|p| p.1.clone()).collect(),
            to_string: per_key.into_iter().map(|p| p.2).collect(),
        };
    }
    exp
}

#[test]
fn enum_alias_order_and_names_match_dotnet() {
    let file = vectors::load_named("enums", "alias_order");
    let probes = probes();
    let (mut total, mut aliased, mut failures) = (0, 0, Vec::new());
    for case in &file.cases {
        let name = case.input["name"].as_str().expect("a name");
        let Some(p) = probes.get(name) else {
            failures.push(format!("{name}: not generated"));
            continue;
        };
        total += 1;
        if case.input["aliased"].as_bool() == Some(true) {
            aliased += 1;
        }
        let out = &case.output;
        let exp = retail_ruled(
            name,
            Expected {
                names: strings(&out["names"])
                    .into_iter()
                    .map(|s| s.expect("a name"))
                    .collect(),
                keys: keys(&out["keys"]),
                get_name: strings(&out["get_name"]),
                to_string: strings(&out["to_string"]),
            },
        );
        if p.names != exp.names {
            failures.push(format!(
                "{name}: GetValues order {:?}, .NET {:?}",
                p.names, exp.names
            ));
        }
        if p.keys != exp.keys {
            failures.push(format!("{name}: keys differ"));
        }
        if p.get_name != exp.get_name {
            failures.push(format!(
                "{name}: GetName {:?}, .NET {:?}",
                p.get_name, exp.get_name
            ));
        }
        if p.to_string != exp.to_string {
            failures.push(format!(
                "{name}: ToString {:?}, .NET {:?}",
                p.to_string, exp.to_string
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} enums differ from .NET:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
    assert_eq!(total, probes.len(), "every generated enum has a case");
    assert!(aliased > 0, "recorded aliased enums are covered");
}

#[test]
fn flag_combinations_of_aliased_flags_enums_format_as_dotnet() {
    let file = vectors::load_named("enums", "flag_combos");
    let probes = probes();
    let (mut total, mut failures) = (0, Vec::new());
    for case in &file.cases {
        let name = case.input["name"].as_str().expect("a name");
        let p = &probes[name];
        for pair in case.output["cases"].as_array().expect("cases") {
            let k = u64_of(&pair[0]).expect("a key");
            let expected = pair[1].as_str().expect("a string");
            total += 1;
            let got = (p.to_string_of)(k);
            if got != expected {
                failures.push(format!("{name}({k:#x}): {got:?}, .NET {expected:?}"));
            }
        }
    }
    assert!(total > 0, "flag cases ran ({total})");
    assert!(
        failures.is_empty(),
        "{} differ:\n  {}",
        failures.len(),
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn aliases_follow_dotnet_lookup_rules() {
    // StatType is declared DataID, InstanceID, DID, IID: .NET's introsort leaves DID before DataID
    // and InstanceID before IID, and a linear scan (18 members) names the first.
    assert_eq!(StatType(6).name(), Some("DID"));
    assert_eq!(StatType(7).to_dotnet_string(), "InstanceID");
    // More than 32 members: a binary search lands on the later alias.
    assert_eq!(SurfacePixelFormat(500).name(), Some("PFID_CUSTOM_RAW_JPEG"));
    // [Flags]: ToString names a defined value by its last alias; GetName by FindDefinedIndex.
    assert_eq!(
        EquipMask(0x7FFF_FFFF).to_dotnet_string(),
        "CanGoInReadySlot"
    );
    assert_eq!(EquipMask(0x7FFF_FFFF).name(), Some("All"));
    assert_eq!(
        ExperienceHandlingType(0x10).to_dotnet_string(),
        "AdminRaiseXP"
    );
    assert_eq!(ExperienceHandlingType(0x10).name(), Some("ApplyToVitae"));
    assert_eq!(ExperienceHandlingType(0).to_dotnet_string(), "Undef");
}
