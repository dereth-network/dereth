//! Divergence: V334
//! Every buildable object description is compared with the client property mirror.
//! The known mismatch categories remain explicit.
//! Fixture: isolated world state, retail dats or world.pack in the real-content tier.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use dereth_primitives::ObjectId;
use dereth_protocol::objects::ItemCreateObject;
use dereth_protocol::types::weeniedesc::PublicWeenieDesc;
use dereth_rules::pwd_mirror::{apply_pwd_field, MirrorStat, MirrorValue, PwdField, PWD_MIRROR};
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_content::PackContent;
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::{PropertyBool, PropertyDataId, PropertyInstanceId, PropertyInt};
use empyrean_entity::ObjectGuid;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::network::game_messages::messages::game_message_create_object::game_message_create_object;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::World;

fn world() -> World {
    let dir = dereth_dat::testing::dat_dir();
    let source = RealDats::open(&dir).unwrap_or_else(|e| {
        panic!(
            "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
            dir.display()
        )
    });
    let dats = DatManager::initialize(Arc::new(source)).expect("retail dats");
    let path = empyrean_common::test_paths::world_pack();
    let pack = PackContent::open(&path).unwrap_or_else(|e| {
        panic!(
            "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
            path.display()
        )
    });
    let clock = VirtualClock::default();
    let timers = TimersState::new(&clock);
    let mut w = World::new(ClockSnapshot::take(&clock, timers.portal_year_ticks), dats);
    w.timers = timers;
    w.content = Arc::new(pack);
    w
}

/// The object's own value for a mirrored property, read as the server reads it.
fn property(o: &WorldObject, stat: MirrorStat, id: u32) -> Option<MirrorValue> {
    let id = u16::try_from(id).ok()?;
    match stat {
        MirrorStat::Int => o.get_property(PropertyInt(id)).map(MirrorValue::Int),
        MirrorStat::DataId => o.get_property(PropertyDataId(id)).map(MirrorValue::DataId),
        MirrorStat::InstanceId => o
            .get_property(PropertyInstanceId(id))
            .map(|v| MirrorValue::InstanceId(ObjectId(v))),
        MirrorStat::Bool => o.get_property(PropertyBool(id)).map(MirrorValue::Bool),
    }
}

/// The field as written, for the report.
fn field(p: &PublicWeenieDesc, f: PwdField) -> String {
    use dereth_rules::weenie::bitfield as b;
    match f {
        PwdField::ObjType => format!("{:#x}", p.obj_type),
        PwdField::Priority => format!("{:?}", p.priority),
        PwdField::ItemsCapacity => format!("{:?}", p.items_capacity),
        PwdField::ContainersCapacity => format!("{:?}", p.containers_capacity),
        PwdField::ValidLocations => format!("{:?}", p.valid_locations),
        PwdField::Location => format!("{:?}", p.location),
        PwdField::MaxStackSize => format!("{:?}", p.max_stack_size),
        PwdField::StackSize => format!("{:?}", p.stack_size),
        PwdField::Useability => format!("{:?}", p.useability),
        PwdField::Effects => format!("{:?}", p.effects),
        PwdField::Value => format!("{:?}", p.value),
        PwdField::AmmoType => format!("{:?}", p.ammo_type),
        PwdField::CombatUse => format!("{:?}", p.combat_use),
        PwdField::MaxStructure => format!("{:?}", p.max_structure),
        PwdField::Structure => format!("{:?}", p.structure),
        PwdField::BlipColor => format!("{:?}", p.blip_color),
        PwdField::RadarEnum => format!("{:?}", p.radar_enum),
        PwdField::PlayerKillerStatus => {
            format!(
                "bits {:#x}",
                p.bitfield & (b::PLAYER_KILLER | b::IMPENETRABLE | b::PK_LITE)
            )
        }
        PwdField::HookType => format!("{:?}", p.hook_type),
        PwdField::HookItemTypes => format!("{:?}", p.hook_item_types),
        PwdField::IconId => format!("{:#x}", p.icon_id),
        PwdField::PScript => format!("{:?}", p.pscript),
        PwdField::IconOverlay => format!("{:?}", p.icon_overlay_id),
        PwdField::IconUnderlay => format!("{:?}", p.icon_underlay_id),
        PwdField::Container => format!("{:?}", p.container_id),
        PwdField::Wielder => format!("{:?}", p.wielder_id),
        PwdField::Monarch => format!("{:?}", p.monarch),
        PwdField::HouseOwner => format!("{:?}", p.house_owner_iid),
        PwdField::PetOwner => format!("{:?}", p.pet_owner),
        PwdField::Bit { mask, .. } => format!("bit {}", u8::from(p.bitfield & mask != 0)),
    }
}

/// Whether the server wrote `f` at all (an optional field present; a non-zero type or icon). The
/// bits and the player-killer status are always on the wire.
fn written(p: &PublicWeenieDesc, f: PwdField) -> Option<bool> {
    Some(match f {
        PwdField::ObjType => p.obj_type != 0,
        PwdField::IconId => p.icon_id != 0,
        PwdField::Bit { .. } | PwdField::PlayerKillerStatus => return None,
        _ => !field(p, f).starts_with("None"),
    })
}

fn label(stat: MirrorStat, id: u32, f: PwdField) -> String {
    format!("{stat:?} {id} -> {f:?}")
}

#[derive(Default)]
struct Bucket {
    count: usize,
    examples: Vec<String>,
}

#[test]
fn the_servers_create_object_description_against_the_clients_property_mirror() {
    let mut w = world();
    let mut wcids: Vec<u32> = w.content.get_all_weenie_names().keys().copied().collect();
    wcids.sort_unstable();
    assert!(!wcids.is_empty(), "world.pack: {} weenies", wcids.len());

    let mut found: BTreeMap<(String, &'static str), Bucket> = BTreeMap::new();
    let (mut compared, mut skipped) = (0usize, 0usize);
    let mut next = 0x8000_0001u32;
    // an object whose constructor or writer reaches an unported path is counted, not printed
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for wcid in wcids {
        let Some(weenie) = w.content.get_cached_weenie(wcid) else {
            continue;
        };
        let guid = ObjectGuid::new(next);
        next += 1;
        let built = catch_unwind(AssertUnwindSafe(|| {
            CtorEnv::with_world(&w, |env| {
                factory::create_world_object(env, Some(weenie), guid)
            })
        }));
        let Ok(Some(o)) = built else {
            skipped += 1;
            continue;
        };
        if w.objects.insert(o).is_err() {
            skipped += 1;
            continue;
        }
        let msg = catch_unwind(AssertUnwindSafe(|| {
            game_message_create_object(&mut w, guid, false, false)
        }));
        let decoded = msg
            .ok()
            .and_then(|m| dereth_protocol::read_body_padded::<ItemCreateObject>(&m.data[4..]).ok());
        let Some(decoded) = decoded else {
            w.objects.remove(guid);
            skipped += 1;
            continue;
        };
        let pwd = decoded.0.wdesc;
        let o = w.objects.get(guid).expect("inserted");
        compared += 1;
        for r in PWD_MIRROR {
            let kind = match property(o, r.stat, r.id) {
                Some(v) => {
                    let mut projected = pwd.clone();
                    apply_pwd_field(&mut projected, r.field, v);
                    if projected == pwd {
                        continue;
                    }
                    let example = format!(
                        "wcid {wcid}: property {v:?}, ACE wrote {}, the client would hold {}",
                        field(&pwd, r.field),
                        field(&projected, r.field)
                    );
                    ("differs", example)
                }
                None => {
                    if written(&pwd, r.field) != Some(true) {
                        continue;
                    }
                    (
                        "sent without the property",
                        format!("wcid {wcid}: ACE wrote {}", field(&pwd, r.field)),
                    )
                }
            };
            let b = found
                .entry((label(r.stat, r.id, r.field), kind.0))
                .or_default();
            b.count += 1;
            if b.examples.len() < 3 {
                b.examples.push(kind.1);
            }
        }
        w.objects.remove(guid);
    }

    std::panic::set_hook(hook);
    println!("{compared} objects compared, {skipped} not built or not written");
    for ((row, kind), b) in &found {
        println!(
            "PWDMIRROR {row} | {kind} | {} | {}",
            b.count,
            b.examples.join("; ")
        );
    }
    assert!(compared > 0, "only {compared} objects compared");
    let got: Vec<(String, &str)> = found.keys().cloned().collect();
    let known: Vec<(String, &str)> = KNOWN.iter().map(|(r, k)| ((*r).to_owned(), *k)).collect();
    assert_eq!(
        got, known,
        "the ACE-vs-retail categories changed (left: found, right: known)"
    );
}

// V334.
const KNOWN: &[(&str, &str)] = &[
    ("Bool 3 -> Bit { mask: 1, inverted: true }", "differs"),
    ("DataId 8 -> IconId", "sent without the property"),
    ("Int 19 -> Value", "differs"),
    ("Int 6 -> ItemsCapacity", "differs"),
    ("Int 7 -> ContainersCapacity", "differs"),
];
