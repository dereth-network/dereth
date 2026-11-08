//! Examination held to the January 2017 recordings: each monster's profile the 2017 servers sent
//! in answer to a request about an attackable target is the profile this server builds from the
//! monster's template in the September 2013 data, given the monster's health and vigor at the
//! time and the chance to hit it (which the server works out at run time).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use auberean_assets::entity::Entity;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::ServerMessage;
use auberean_script::native_object::{TextInfo, VariableValue};
use auberean_script::wire::In;
use rusqlite::{Connection, OpenFlags};
use yalain_content::{facts, FactIds, FACT_NAMES};
use yalain_world::calls::{self, Node, Value};
use yalain_world::examine::Vitals;
use yalain_world::profile::{self, var};

struct Install {
    dat: DatFile,
    ctx: Context,
    ids: FactIds,
    lib: auberean_script::library::Library,
}

fn installed() -> Option<&'static Install> {
    static I: OnceLock<Option<Install>> = OnceLock::new();
    I.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).ok()?;
        let ctx = Context::from_portal(&dat).ok()?;
        let id = |name: &str| {
            ctx.properties()
                .and_then(|l| l.names.names.iter().find(|(_, n)| n == name))
                .map_or(0, |(v, _)| *v)
        };
        let ids = FactIds {
            name: id(FACT_NAMES[0]),
            description: id(FACT_NAMES[1]),
            level: id(FACT_NAMES[2]),
            max_health: id(FACT_NAMES[3]),
            max_vigor: id(FACT_NAMES[4]),
            faction: id(FACT_NAMES[5]),
            group: id(FACT_NAMES[6]),
            min_level: id(FACT_NAMES[7]),
        };
        let lib = dat
            .read_checked(
                auberean_script::library::RECORD,
                auberean_script::library::Library::from_record,
            )
            .ok()?
            .value;
        Some(Install { dat, ctx, ids, lib })
    })
    .as_ref()
}

fn entity(i: &Install, id: u32) -> Option<Entity> {
    auberean_assets::decode(id, &i.dat.read(id).ok()?, &i.ctx).ok()
}

fn open(path: &Path) -> Connection {
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(uri, flags).expect("the capture index opens read-only")
}

/// A recorded profile's lines, read: each text line's variables, by id, as numbers.
fn numbers_in(bytes: &[u8], request_len: usize) -> Option<BTreeMap<u32, u32>> {
    let mut r = In::new(&bytes[request_len..]);
    let n = r.u32().ok()?;
    let mut out = BTreeMap::new();
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                let t = TextInfo::unpack(&mut r).ok()?;
                for v in t.variables {
                    if let VariableValue::Word(x) = v.value {
                        out.insert(v.id, x);
                    }
                }
            }
            0x4000_0006 => {}
            0x4000_0007 => {
                r.u32().ok()?;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Every monster's profile 2017 sent for an attackable target is the profile built from its
/// template, given its health and vigor and the chance to hit; the differences are named (a
/// monster named as unique, a champion, a quest's or a special effect's at run time; a level or
/// name the server set apart from the template).
/// Behaviour: server.examination.monster-profiles-as-2017
#[test]
fn monster_profiles_are_built_from_their_templates_as_2017_built_them() {
    let path = auberean_testkit::data::require_capture_index();
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.dir, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.mtype IN (65624, 65638) ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let mut templates: HashMap<u64, u32> = HashMap::new();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut cache: HashMap<u32, yalain_content::Facts> = HashMap::new();
    let mut session = -1;
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("session");
        if sid != session {
            templates.clear();
            session = sid;
        }
        let raw: Vec<u8> = r.get(2).expect("raw");
        let Ok(m) = ServerMessage::decode(&raw) else {
            continue;
        };
        let call = match m {
            ServerMessage::CreateObject(o) => {
                templates.insert(o.id, o.weenie.entity_did);
                continue;
            }
            ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => c,
            _ => continue,
        };
        let v = calls::values(&call.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        let req = profile::read_request(bytes);
        if req.kind != profile::request::TARGET {
            continue;
        }
        let Some(template) = templates.get(&req.object).copied() else {
            *tally
                .entry("the target's making not recorded".into())
                .or_default() += 1;
            continue;
        };
        let f = *cache
            .entry(template)
            .or_insert_with(|| facts(template, &|id| entity(inst, id), &inst.ids));
        let request = &bytes[..28];
        let Some(n) = numbers_in(bytes, 28) else {
            *tally.entry("does not read".into()).or_default() += 1;
            continue;
        };
        let vitals = Vitals {
            health: n.get(&var::HEALTH).copied().unwrap_or(0),
            max_health: n.get(&var::MAX_HEALTH).copied().unwrap_or(0),
            vigor: n.get(&var::VIGOR).copied().unwrap_or(0),
            max_vigor: n.get(&var::MAX_VIGOR).copied().unwrap_or(0),
            pk_damage: 0,
            pk_vigor_loss: 0,
        };
        let chance = n.get(&var::CHANCE).copied().unwrap_or(0);
        let ours = profile::monster(req.asked, &f, vitals, chance).packed(request);
        let what = if ours == *bytes {
            "as built from the template".to_string()
        } else {
            // Named with a look the server chose at run time (unique, champion, quest,
            // special effect)?
            let special = [0x0AB8_F014u32, 0x6005_FF6B, 0x3800_42FF, 0x7C78_01D4];
            let mut alt = None;
            for l in special {
                let mut p = profile::monster(req.asked, &f, vitals, chance);
                if let Some(profile::Node::Text { look: lk, .. }) = p.nodes.first_mut() {
                    *lk = l;
                }
                if p.packed(request) == *bytes {
                    alt = Some(l);
                }
            }
            if alt.is_some() {
                "named with a run-time look (unique, champion, quest, effect)".into()
            } else if n.get(&var::LEVEL).copied() != f.level.map(i32::cast_unsigned) {
                "a level the server set apart from the template".into()
            } else {
                "differs otherwise".into()
            }
        };
        *tally.entry(what).or_default() += 1;
    }
    println!("2017 monster profiles (attackable targets) against ours:");
    for (k, n) in &tally {
        println!("  {n:7}  {k}");
    }
    let built = tally
        .get("as built from the template")
        .copied()
        .unwrap_or(0);
    let other = tally.get("differs otherwise").copied().unwrap_or(0);
    assert!(built > 0);
    // The residue measured on 2026-10-27: six profiles whose name or lines the server set apart
    // from the template in ways the profile does not show.
    assert!(other <= 6, "{other} profiles differ unexplained");
}

/// Lines of item profiles this server does not build: a tool's stats, an effect's amount, the
/// quests and skills an item needs.
const TOOL_STATS: u32 = 0xEF43_9EA4;
const AMOUNT: u32 = 0x0CA6_14AD;
const QUEST_DONE: u32 = 0x510A_7F47;
const QUEST_UNDERWAY: u32 = 0xDB7C_0ACC;
const QUEST_ANY: u32 = 0x8D31_AAA4;
const SKILLS_NEEDED: u32 = 0xA83D_97BB;
const CRAFT_LEVEL: u32 = 0x7386_AE0C;
const ARCANE_LORE: u32 = 0x378E_61A2;

/// The string ids of a profile's text lines, in order.
fn strings_in(bytes: &[u8], request_len: usize) -> Vec<u32> {
    let mut out = Vec::new();
    let Some(rest) = bytes.get(request_len..) else {
        return out;
    };
    let mut r = In::new(rest);
    let Ok(n) = r.u32() else { return out };
    for _ in 0..n {
        let (Ok(ty), Ok(_)) = (r.u32(), r.u32()) else {
            return out;
        };
        let ok = match ty {
            0x4000_0001..=0x4000_0003 | 0x4000_000C => r.u32().and(r.u32()).is_ok(),
            0x4000_0004 => {
                r.u32().is_ok()
                    && TextInfo::unpack(&mut r).is_ok_and(|t| {
                        out.push(t.string_id);
                        true
                    })
            }
            0x4000_0005 | 0x4000_0007 => r.u32().is_ok(),
            0x4000_0006 => true,
            0x4000_0008 | 0x4000_000A => r.u32().and(r.u32()).and(r.u32()).is_ok(),
            0x4000_0009 => r.u32().is_ok_and(|k| (0..k * 3).all(|_| r.u32().is_ok())),
            _ => false,
        };
        if !ok {
            return out;
        }
    }
    out
}

/// A recorded item's create: its template, count, own value, look, and whether it was made with
/// a name of its own (a treasure item).
#[derive(Clone, Copy)]
struct ItemMade {
    template: u32,
    package: u32,
    quantity: u32,
    value: Option<u32>,
    look: u32,
    named: bool,
    /// Its durability now and at most, when the create gives it.
    durability: Option<(i32, i32)>,
}

/// How a profile shows each effect the templates name, read from the effect's saved state (the
/// singleton of the library's package of that name); none for an effect the library does not hold.
fn effect_shown(
    inst: &Install,
    cache: &mut HashMap<String, Option<profile::EffectShown>>,
    name: &str,
) -> Option<profile::EffectShown> {
    let lib = &inst.lib;
    *cache.entry(name.to_string()).or_insert_with(|| {
        let state = 0x7000_0000 | lib.package_id(name)?;
        let st: auberean_assets::script::ScriptState =
            auberean_assets::decode(state, &inst.dat.read(state).ok()?, &inst.ctx).ok()?;
        let g = auberean_script::graph::Graph::decode(&st.graph, lib).ok()?;
        let d = yalain_world::effect_defs::EffectDef::from_state(state, lib, &g).ok()?;
        profile::EffectShown::of(&d)
    })
}

/// The crafter a recorded profile names (a player's name: run-time state the template does not
/// hold), as its "Crafted by" line's variable.
fn crafter_in(bytes: &[u8], request_len: usize) -> Option<String> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 | 0x4000_000C => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                let t = TextInfo::unpack(&mut r).ok()?;
                if t.string_id == profile::item_line::CRAFTED_BY {
                    return t.variables.into_iter().find_map(|v| match v.value {
                        VariableValue::Nested(n) => n.literal.map(|l| String::from_utf16_lossy(&l)),
                        _ => None,
                    });
                }
            }
            0x4000_0005 | 0x4000_0007 => {
                r.u32().ok()?;
            }
            0x4000_0006 => {}
            0x4000_0008 | 0x4000_000A => {
                r.u32().ok()?;
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0009 => {
                let k = r.u32().ok()?;
                for _ in 0..k * 3 {
                    r.u32().ok()?;
                }
            }
            _ => return None,
        }
    }
    None
}

/// A template's properties with what its class says in code, as the server reads them.
fn with_class(inst: &Install, template: u32, p: yalain_content::Props) -> yalain_content::Props {
    let class = yalain_content::Props::class_of(template, &|id| entity(inst, id))
        .map_or_else(Vec::new, |c| {
            yalain_world_db::install::class_properties(&inst.lib, c)
        });
    p.with_class(&class)
}

/// Every item profile 2017 sent for an item made from a template (its create with no name of its
/// own), wherever the item was, is the profile built from the template's properties and look (or,
/// for the human starting weapons, their classes' facts), the effects the template names (read
/// from their saved states), the item's count, own value and durability, and who made it (a
/// player's name the recorded line gives), and whether its class makes it destroyed on use; the
/// differences are counted by the kind of line the server does not build yet (a tool's stats,
/// effects only the server's data names, the quests and skills an item needs, a stack's count
/// changed).
/// Behaviour: server.examination.item-profiles-as-2017
#[test]
fn item_profiles_are_built_from_their_templates_as_2017_built_them() {
    use auberean_protocol::types::WeenieDesc;
    use yalain_world::profile::ItemView;
    let path = auberean_testkit::data::require_capture_index();
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.dir = 's2c' AND m.mtype IN (65624, 65638) ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let prop_name = |id: u32| {
        inst.ctx
            .properties()
            .and_then(|l| l.names.name(id))
            .map(str::to_string)
    };
    let mut made: HashMap<u64, ItemMade> = HashMap::new();
    let mut props: HashMap<u32, yalain_content::Props> = HashMap::new();
    let mut icons: HashMap<u32, Vec<[u32; 3]>> = HashMap::new();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut bow = (0, 0);
    let mut shields = 0;
    let mut effects = HashMap::new();
    let mut session = -1;
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("session");
        if sid != session {
            made.clear();
            session = sid;
        }
        let raw: Vec<u8> = r.get(1).expect("raw");
        let Ok(m) = ServerMessage::decode(&raw) else {
            continue;
        };
        let call = match m {
            ServerMessage::CreateObject(o) => {
                let w = &o.weenie;
                let number = |bit: u32| {
                    WeenieDesc::NUMBER_BITS
                        .iter()
                        .position(|b| *b == bit)
                        .filter(|_| w.mask & bit != 0)
                        .map(|i| w.numbers[i].cast_unsigned())
                };
                made.insert(
                    o.id,
                    ItemMade {
                        template: w.entity_did,
                        package: w.package_id,
                        quantity: number(0x800).unwrap_or(1),
                        value: number(0x1000),
                        look: o.visual.parent,
                        named: w.mask & WeenieDesc::NAME != 0,
                        durability: number(0x1000_0000).filter(|m| *m > 0).map(|m| {
                            (
                                number(0x800_0000).unwrap_or(m).cast_signed(),
                                m.cast_signed(),
                            )
                        }),
                    },
                );
                continue;
            }
            ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => c,
            _ => continue,
        };
        let v = calls::values(&call.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        let req = profile::read_request(bytes);
        if req.kind != profile::request::OBJECT {
            continue;
        }
        let Some(i) = made.get(&req.object).copied() else {
            continue;
        };
        let shield = is_shield_class(&inst.lib, i.package);
        let p = props
            .entry(i.template)
            .or_insert_with(|| {
                let p = yalain_content::Props::of(i.template, &|id| entity(inst, id), &prop_name);
                if shield {
                    with_class(inst, i.template, p)
                } else {
                    p
                }
            })
            .clone();
        let is_item = profile::starting_weapon(i.template).is_some()
            || p.get("Item_Value").is_some()
            || p.get("Weapon_Damage").is_some();
        if !is_item || i.named {
            continue;
        }
        let mut view = profile::starting_weapon(i.template)
            .unwrap_or_else(|| ItemView::of(&p, i.quantity, i.value, shield));
        view.effects = profile::item_effects(&p, &mut |n| effect_shown(inst, &mut effects, n));
        if i.durability.is_some() {
            view.durability = i.durability;
        }
        view.crafter = crafter_in(bytes, 20);
        view.destroyed_on_use |= profile::destroyed_on_use_kind(
            i.package,
            |n| inst.lib.package_id(n),
            |a, b| inst.lib.derives_from(a, b),
        );
        view.icon = icons
            .entry(i.look)
            .or_insert_with(|| {
                yalain_content::icon_layers(i.look, &|did| {
                    auberean_assets::decode(did, &inst.dat.read(did).ok()?, &inst.ctx).ok()
                })
            })
            .clone();
        let request = &bytes[..20.min(bytes.len())];
        let ours = profile::item(req.asked, &view).packed(request);
        let exact = ours == *bytes;
        if i.template == 0x4700_051B {
            bow.1 += 1;
            bow.0 += usize::from(exact);
        }
        let what = if exact {
            "as built from the template".to_string()
        } else {
            let (theirs, mine) = (strings_in(bytes, 20), strings_in(&ours, 20));
            let more = |id: u32| {
                theirs.iter().filter(|x| **x == id).count()
                    > mine.iter().filter(|x| **x == id).count()
            };
            if more(TOOL_STATS) {
                "a tool's stats (set as it is made)".into()
            } else if more(profile::item_line::DESTROYED_ON_USE) {
                "destroyed on use by its kind (keys, potions)".into()
            } else if more(profile::item_line::EFFECT_NAME) || more(AMOUNT) {
                "effects only the server's data names".into()
            } else if more(QUEST_DONE) || more(QUEST_UNDERWAY) || more(QUEST_ANY) {
                "a quest needed to use or take it".into()
            } else if more(SKILLS_NEEDED) || more(CRAFT_LEVEL) || more(ARCANE_LORE) {
                "skills needed to use it".into()
            } else if more(profile::item_line::STACK) || more(profile::item_line::VALUE) {
                "a stack whose count changed after it was made".into()
            } else {
                "differs otherwise".into()
            }
        };
        if shield {
            let effects = what.starts_with("effects");
            assert!(
                exact || effects,
                "a plain shield {:#x}: {what}: {}",
                i.template,
                first_difference(bytes, &ours)
            );
            shields += usize::from(exact);
        }
        *tally.entry(what).or_default() += 1;
    }
    println!("2017 item profiles (template items) against ours:");
    for (k, n) in &tally {
        println!("  {n:7}  {k}");
    }
    println!("  the human starting bow: {} of {}", bow.0, bow.1);
    println!("  plain shields: {shields}");
    let built = tally
        .get("as built from the template")
        .copied()
        .unwrap_or(0);
    let other = tally.get("differs otherwise").copied().unwrap_or(0);
    assert!(built > 0);
    assert_eq!(bow.0, bow.1, "the starting bow's profiles");
    assert!(shields >= 6, "plain shields' profiles as built: {shields}");
    // The residue measured on 2026-10-29 (322 of 4,272): descriptions their tables lack, durability
    // worn by use, stacks' plural names, jewelry's and shields' stats.
    assert!(
        built > other * 4,
        "template items' profiles as built: {built} against {other}"
    );
}

/// The offset of the line break opening an inscription's lines in a profile, its words and who
/// wrote them.
fn inscription_in(bytes: &[u8], request_len: usize) -> Option<(usize, String, String)> {
    fn text_of(t: &TextInfo) -> Option<String> {
        t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
    }
    let rest = bytes.get(request_len..)?;
    let mut r = In::new(rest);
    let n = r.u32().ok()?;
    let mut start = None;
    let mut literals = Vec::new();
    for _ in 0..n {
        let at = rest.len() - r.remaining();
        let ty = r.u32().ok()?;
        let order = r.u32().ok()?;
        if order == profile::INSCRIPTION && start.is_none() {
            start = Some(at);
        }
        match ty {
            0x4000_0001..=0x4000_0003 | 0x4000_000C => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                let t = TextInfo::unpack(&mut r).ok()?;
                if order == profile::INSCRIPTION {
                    literals.extend(text_of(&t));
                }
            }
            0x4000_0005 | 0x4000_0007 => {
                r.u32().ok()?;
            }
            0x4000_0006 => {}
            0x4000_0008 | 0x4000_000A => {
                r.u32().ok()?;
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0009 => {
                let k = r.u32().ok()?;
                for _ in 0..k * 3 {
                    r.u32().ok()?;
                }
            }
            _ => return None,
        }
    }
    let [by, said] = literals.as_slice() else {
        return None;
    };
    Some((request_len + start?, said.clone(), by.clone()))
}

/// Every recorded profile holding an inscription ends with the lines this server builds from the
/// words and who wrote them, and that writer goes by its name in the world.
/// Behaviour: server.examination.an-inscription-is-shown
#[test]
fn inscriptions_are_shown_as_2017_showed_them() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.dir = 's2c' AND m.mtype = 65638",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let (mut seen, mut exact) = (0, 0);
    while let Some(r) = rows.next().expect("a row") {
        let raw: Vec<u8> = r.get(0).expect("raw");
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if c.function.0 != profile::EXAMINATION_PROFILE {
            continue;
        }
        let v = calls::values(&c.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        if profile::read_request(bytes).kind != profile::request::OBJECT {
            continue;
        }
        let Some((start, said, by)) = inscription_in(bytes, 20) else {
            continue;
        };
        seen += 1;
        let mut lines = profile::Profile::new(0x3FF);
        profile::inscription_lines(&mut lines, &said, &by);
        let lines = lines.packed(&[]);
        let shape = &lines[4..lines.len() - 4];
        if bytes[start..bytes.len() - 4] == *shape && by.ends_with(']') {
            exact += 1;
        }
    }
    println!("{exact} of {seen} recorded inscriptions as this server shows them");
    assert!(seen >= 400, "the recordings hold them: {seen}");
    assert_eq!(exact, seen);
}

/// A profile's greatest damage: the line a weapon's damage is shown on, its high variable.
fn damage_in(bytes: &[u8], request_len: usize) -> Option<u32> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 | 0x4000_000C => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                let t = TextInfo::unpack(&mut r).ok()?;
                if t.string_id == profile::item_line::DAMAGE_VALUE {
                    return t.variables.iter().find_map(|v| match v.value {
                        VariableValue::Word(w) if v.id == profile::item_var::DAMAGE_HIGH => Some(w),
                        _ => None,
                    });
                }
            }
            0x4000_0005 | 0x4000_0007 => {
                r.u32().ok()?;
            }
            0x4000_0006 => {}
            0x4000_0008 | 0x4000_000A => {
                r.u32().ok()?;
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0009 => {
                let k = r.u32().ok()?;
                for _ in 0..k * 3 {
                    r.u32().ok()?;
                }
            }
            _ => return None,
        }
    }
    None
}

/// Most treasure weapons recorded below level 50, with no effects, have a greatest damage in the band
/// this server draws theirs from.
/// Behaviour: server.loot.treasure-numbers-by-level
#[test]
fn treasure_weapons_damage_falls_in_the_band_this_server_draws_from() {
    use yalain_world::treasure::WEAPON_DAMAGE_SHARE;
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.dir = 's2c' AND m.mtype = 65638",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let mut seen = std::collections::HashSet::new();
    let (mut weapons, mut within) = (0usize, 0usize);
    while let Some(r) = rows.next().expect("a row") {
        let raw: Vec<u8> = r.get(0).expect("raw");
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if c.function.0 != profile::EXAMINATION_PROFILE {
            continue;
        }
        let v = calls::values(&c.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        let req = profile::read_request(bytes);
        if req.kind != profile::request::OBJECT {
            continue;
        }
        let strings = strings_in(bytes, 20);
        let plain = !strings.iter().any(|x| {
            [
                profile::item_line::CRAFTED_BY,
                profile::item_line::EFFECT_NAME,
                profile::item_line::SHIELD_RATING,
                profile::item_line::ARMOR_RATING,
            ]
            .contains(x)
        });
        let (Some(most), Some(level), true) = (
            damage_in(bytes, 20),
            stat_in(bytes, 20, profile::line::MINIMUM_LEVEL),
            plain,
        ) else {
            continue;
        };
        if !(1..50).contains(&level) || !seen.insert(req.object) {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let share = most as f32 / yalain_world::made::damage(level) as f32;
        weapons += 1;
        within += usize::from(
            (WEAPON_DAMAGE_SHARE.0 - 0.005..=WEAPON_DAMAGE_SHARE.1 + 0.005).contains(&share),
        );
    }
    println!("treasure weapons within the band: {within} of {weapons}");
    assert!(weapons > 500, "the recordings hold them: {weapons}");
    assert!(within * 100 >= weapons * 90, "{within} of {weapons}");
}

/// A number the line after a label shows: the whole-number node, or the first variable of the
/// next text.
fn stat_in(bytes: &[u8], request_len: usize, label: u32) -> Option<u32> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    let mut armed = false;
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 | 0x4000_000C => {
                r.u32().ok()?;
                let v = r.u32().ok()?;
                if armed {
                    return Some(v);
                }
            }
            0x4000_0004 => {
                r.u32().ok()?;
                let t = TextInfo::unpack(&mut r).ok()?;
                if armed {
                    if let Some(VariableValue::Word(v)) = t.variables.first().map(|v| &v.value) {
                        return Some(*v);
                    }
                    armed = false;
                }
                if t.string_id == label {
                    armed = true;
                }
            }
            0x4000_0005 | 0x4000_0007 => {
                r.u32().ok()?;
            }
            0x4000_0006 => {}
            0x4000_0008 | 0x4000_000A => {
                r.u32().ok()?;
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0009 => {
                let c = r.u32().ok()? as usize;
                for _ in 0..c * 3 {
                    r.u32().ok()?;
                }
            }
            _ => return None,
        }
    }
    None
}

/// The strings and numbers two profiles differ in, the first of each, for the tally.
fn first_difference(theirs: &[u8], ours: &[u8]) -> String {
    let (a, b) = (strings_in(theirs, 20), strings_in(ours, 20));
    match a.iter().zip(&b).position(|(x, y)| x != y) {
        Some(i) => format!("line {:08X} where ours has {:08X}", a[i], b[i]),
        None if a.len() != b.len() => {
            let longer = if a.len() > b.len() { &a } else { &b };
            format!(
                "{} lines against {} (the first more: {:08X})",
                a.len(),
                b.len(),
                longer[a.len().min(b.len())]
            )
        }
        None => "a number".to_string(),
    }
}

/// Every profile 2017 sent of an item made at a spinner is the profile built from the template,
/// the recipe's outcome and the item level: its name, its least level, its damage or armor, its
/// slots, its effects with their amounts, that it binds. What differs between items of a level
/// (its delay, vigor cost and value) is taken from the recording, and is checked apart to lie
/// within the rule's bands.
/// Behaviour: server.craft.made-item-profiles-as-2017
#[test]
fn made_items_profiles_are_built_from_their_recipes_item_level_as_2017_built_them() {
    use auberean_protocol::types::WeenieDesc;
    use auberean_protocol::ClientMessage;
    use yalain_world::craft::{Action, Crafting};
    use yalain_world::made;
    use yalain_world::profile::{ItemView, When};
    let path = auberean_testkit::data::require_capture_index();
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let lib = &inst.lib;
    let graph = |state: u32| -> Option<auberean_script::graph::Graph> {
        let st: auberean_assets::script::ScriptState =
            auberean_assets::decode(state, &inst.dat.read(state).ok()?, &inst.ctx).ok()?;
        auberean_script::graph::Graph::decode(&st.graph, lib).ok()
    };
    let effect = |state: u32| {
        yalain_world::effect_defs::EffectDef::from_state(state, lib, &graph(state)?).ok()
    };
    let prop_name = |id: u32| {
        inst.ctx
            .properties()
            .and_then(|l| l.names.name(id))
            .map(str::to_string)
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.raw, m.dir FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.mtype IN (65619, 65624, 65638) ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let mut crafting = Crafting::default();
    let every_skill: Vec<u32> = (0x6F00_068F..=0x6F00_06AA).collect();
    let all_skills = Crafting::read(lib, &every_skill, &[], &[], &graph);
    let mut read: HashMap<u32, bool> = HashMap::new();
    let mut session = -1;
    let mut asked: Option<(u32, u32)> = None;
    let mut made_here: HashMap<u64, (u32, u32, u32, u32, u32)> = HashMap::new();
    let mut seen: HashMap<u64, bool> = HashMap::new();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let (mut built, mut compared) = (0usize, 0usize);
    let mut rng = yalain_world::rng::Rng::new(1);
    let mut icons: HashMap<u32, Vec<[u32; 3]>> = HashMap::new();
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("session");
        if sid != session {
            session = sid;
            asked = None;
            made_here.clear();
        }
        let raw: Vec<u8> = r.get(1).expect("raw");
        let dir: String = r.get(2).expect("dir");
        if dir == "c2s" {
            if let Ok(ClientMessage::ServerCall(c)) = ClientMessage::decode(&raw) {
                if c.function.0 == yalain_world::craft::function::EXECUTE {
                    let v = calls::values(&c.args.0).unwrap_or_default();
                    if let [_, _, Value::Int(spinner), Value::Int(recipe)] = v.as_slice() {
                        asked = (*spinner > 0).then_some((*recipe, *spinner));
                        if let Some((recipe, _)) = asked {
                            if read.insert(recipe, true).is_none() {
                                let one = Crafting::read(lib, &[], &[recipe], &[], &graph);
                                crafting.recipes.extend(one.recipes);
                                crafting.spinner_tables.extend(one.spinner_tables);
                            }
                        }
                    }
                }
            }
            continue;
        }
        let Ok(m) = ServerMessage::decode(&raw) else {
            continue;
        };
        let call = match m {
            ServerMessage::CreateObject(o) => {
                let w = &o.weenie;
                if let Some((recipe, spinner)) = asked {
                    let product = crafting.recipes.get(&recipe).and_then(|d| {
                        d.checks
                            .iter()
                            .flat_map(|c| &c.outcomes)
                            .flat_map(|x| &x.actions)
                            .find_map(|a| match a {
                                Action::Produce {
                                    template, ordinal, ..
                                } if *template == w.entity_did => Some(*ordinal),
                                _ => None,
                            })
                    });
                    let tool = crafting.recipes.get(&recipe).is_some_and(|d| {
                        d.checks
                            .iter()
                            .flat_map(|c| &c.outcomes)
                            .flat_map(|x| &x.actions)
                            .any(|a| {
                                matches!(a, Action::Mutate { table: Some(t), .. }
                                    if *t == made::GATHERING_TOOL_TABLE
                                        || *t == made::WORKING_TOOL_TABLE)
                            })
                    });
                    if product.is_some() && (tool || w.mask & WeenieDesc::NAME != 0) {
                        made_here.insert(
                            o.id,
                            (recipe, spinner, w.entity_did, o.visual.parent, w.package_id),
                        );
                        asked = None;
                    }
                }
                continue;
            }
            ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => c,
            _ => continue,
        };
        let v = calls::values(&call.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        let req = profile::read_request(bytes);
        if req.kind != profile::request::OBJECT {
            continue;
        }
        let Some(&(recipe, spinner, template, look, package)) = made_here.get(&req.object) else {
            continue;
        };
        if seen.insert(req.object, true).is_some() {
            continue;
        }
        let def = crafting.recipes.get(&recipe).cloned().expect("read");
        let props = with_class(
            inst,
            template,
            yalain_content::Props::of(template, &|id| entity(inst, id), &prop_name),
        );
        let facts_of = facts(template, &|id| entity(inst, id), &inst.ids);
        compared += 1;
        // The outcome it came from: of those making this template, whichever builds what was sent.
        let mut best: Option<Vec<u8>> = None;
        let mut exact = false;
        for out in def.checks.iter().flat_map(|c| &c.outcomes) {
            let Some(ordinal) = out.actions.iter().find_map(|a| match a {
                Action::Produce {
                    template: t,
                    ordinal,
                    ..
                } if *t == template => Some(*ordinal),
                _ => None,
            }) else {
                continue;
            };
            let shield = is_shield_class(lib, package);
            let Some(mut mine) = yalain_world::craft::made_of(
                &crafting, &def, out, ordinal, spinner, &props, shield, &effect, &mut rng,
            ) else {
                continue;
            };
            let delay = stat_in(bytes, 20, profile::item_line::COMBAT_DELAY);
            let vigor = stat_in(bytes, 20, profile::item_line::VIGOR_COST);
            let value = stat_in(bytes, 20, profile::item_line::VALUE_LABEL);
            if mine.delay.is_some() {
                mine.delay = delay.map(u32::cast_signed);
            }
            if mine.vigor.is_some() {
                mine.vigor = vigor.map(u32::cast_signed);
            }
            if mine.damage.is_some() || mine.armor.is_some() {
                mine.value = value.unwrap_or(mine.value);
            }
            let mut view = ItemView::of(&props, 1, (mine.value > 0).then_some(mine.value), shield);
            made::apply_to_view(&mine, &facts_of, &mut view);
            if mine.damage.is_some() || mine.armor.is_some() {
                view.min_level = spinner.cast_signed();
            }
            view.crafter = crafter_in(bytes, 20);
            // What a tool has worn down to is the recording's.
            if let (Some(now), Some(d)) = (
                stat_in(bytes, 20, profile::item_line::DURABILITY),
                view.durability.as_mut(),
            ) {
                d.0 = now.cast_signed();
            }
            let worn: Vec<_> = out
                .actions
                .iter()
                .filter_map(|a| match a {
                    Action::ItemEffect {
                        ordinal: o,
                        state,
                        kind: yalain_world::craft::item_effect::WORN,
                        ..
                    } if *o == ordinal => effect(*state),
                    _ => None,
                })
                .collect();
            if mine.craft_needed.is_some() {
                let knife = yalain_content::Props::class_of(template, &|id| entity(inst, id))
                    .zip(inst.lib.package_id(yalain_world::butchery::TOOL_CLASS))
                    .is_some_and(|(c, k)| inst.lib.derives_from(c, k));
                view.tool_skills = made::tool_skills(template, knife, &worn, &all_skills.skills)
                    .into_iter()
                    .map(|(id, table)| profile::string(table, id))
                    .collect();
            }
            view.binds = out.actions.iter().any(|a| {
                matches!(a, Action::Bind { ordinal: o, spinners }
                    if *o == ordinal && (spinners.0..=spinners.1).contains(&spinner))
            });
            view.destroyed_on_use |= profile::destroyed_on_use_kind(
                package,
                |n| lib.package_id(n),
                |a, b| lib.derives_from(a, b),
            );
            view.icon = icons
                .entry(look)
                .or_insert_with(|| {
                    yalain_content::icon_layers(look, &|did| {
                        auberean_assets::decode(did, &inst.dat.read(did).ok()?, &inst.ctx).ok()
                    })
                })
                .clone();
            for a in &out.actions {
                if let Action::ItemEffect {
                    ordinal: o,
                    state,
                    kind,
                    spellcraft,
                    by_spinner,
                } = a
                {
                    let when = match *kind {
                        yalain_world::craft::item_effect::STATIC => When::Permanent,
                        yalain_world::craft::item_effect::WORN => When::Equipped,
                        _ => continue,
                    };
                    if *o == ordinal {
                        let sc = crafting.spellcraft_at(*spellcraft, *by_spinner, spinner);
                        if let Some(e) = effect(*state)
                            .as_ref()
                            .and_then(|d| made::effect_shown(d, when, sc))
                        {
                            view.effects.push(e);
                        }
                    }
                }
            }
            let request = &bytes[..20.min(bytes.len())];
            let ours = profile::item(req.asked, &view).packed(request);
            if ours == *bytes {
                exact = true;
                break;
            }
            let same = |o: &[u8]| o.iter().zip(bytes.iter()).filter(|(a, b)| a == b).count();
            if best.as_ref().is_none_or(|b| same(&ours) > same(b)) {
                best = Some(ours);
            }
        }
        if exact {
            built += 1;
            *tally.entry("exact".into()).or_default() += 1;
        } else if let Some(ours) = best {
            *tally.entry(first_difference(bytes, &ours)).or_default() += 1;
        } else {
            *tally.entry("no outcome".into()).or_default() += 1;
        }
    }
    println!("made items' profiles: {built} of {compared} byte for byte");
    let mut list: Vec<_> = tally.into_iter().collect();
    list.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in &list {
        println!("  {n:6}  {k}");
    }
    // The residue measured on 2026-11-05 (3 of 330): a chest piece a point off the curve, a helm
    // dyed since, a knife made as another making was asked for.
    assert!(
        compared == built || compared - built <= 3,
        "made items' profiles built: {built} of {compared}"
    );
}

/// Every piece of armor dyed in the recordings holds its plant's value in the three modifiers the
/// next time the recordings make it, and its icon shows that value from the dye on.
/// Behaviour: server.craft.dyed-armor-as-2017
#[test]
fn armor_dyed_holds_its_plants_value_when_next_made() {
    use auberean_protocol::ClientMessage;
    use yalain_world::craft::{Crafting, Custom};
    use yalain_world::equip::DYE_KEYS;
    let path = auberean_testkit::data::require_capture_index();
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let graph = |state: u32| -> Option<auberean_script::graph::Graph> {
        let st: auberean_assets::script::ScriptState =
            auberean_assets::decode(state, &inst.dat.read(state).ok()?, &inst.ctx).ok()?;
        auberean_script::graph::Graph::decode(&st.graph, &inst.lib).ok()
    };
    let prop_name = |id: u32| {
        inst.ctx
            .properties()
            .and_then(|l| l.names.name(id))
            .map(str::to_string)
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.raw, m.dir FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.mtype IN (65619, 65624, 65638) ORDER BY s.t_first + m.t, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let mut dyeing: HashMap<u32, bool> = HashMap::new();
    let mut template: HashMap<u64, u32> = HashMap::new();
    let mut dyed: HashMap<u64, f32> = HashMap::new();
    let mut making: Option<(u64, f32)> = None;
    let (mut asked, mut held) = (0usize, 0usize);
    // Since its dye, before it was next made: its look, and the value.
    let mut looks: HashMap<u64, u32> = HashMap::new();
    let mut since: HashMap<u64, f32> = HashMap::new();
    let (mut examined, mut colored, mut tabled) = (0usize, 0usize, 0usize);
    while let Some(r) = rows.next().expect("a row") {
        let raw: Vec<u8> = r.get(0).expect("raw");
        let dir: String = r.get(1).expect("dir");
        if dir == "c2s" {
            let Ok(ClientMessage::ServerCall(c)) = ClientMessage::decode(&raw) else {
                continue;
            };
            if c.function.0 != yalain_world::craft::function::EXECUTE {
                continue;
            }
            let v = calls::values(&c.args.0).unwrap_or_default();
            let [_, Value::Graph(g), _, Value::Int(recipe)] = v.as_slice() else {
                continue;
            };
            let is_dyeing = *dyeing.entry(*recipe).or_insert_with(|| {
                Crafting::read(&inst.lib, &[], &[*recipe], &[], &graph)
                    .recipes
                    .get(recipe)
                    .is_some_and(|d| d.custom == Custom::Dye)
            });
            if !is_dyeing {
                continue;
            }
            let slots = yalain_world::craft::slots_in(g);
            let of = |o: u32| slots.iter().find(|(x, _)| *x == o).map(|(_, i)| *i);
            let (Some(armor), Some(plant)) = (of(1), of(2)) else {
                continue;
            };
            let Some(t) = template.get(&plant) else {
                continue;
            };
            let props = yalain_content::Props::of(*t, &|id| entity(inst, id), &prop_name);
            if let Some(value) = props.float("Craft_DyePlant_Mod") {
                making = Some((armor, value));
            }
            continue;
        }
        let o = match ServerMessage::decode(&raw) {
            Ok(ServerMessage::PlayerCall(c)) => {
                // It takes the color with the answer.
                if c.function.0 == yalain_world::craft::function::EXECUTED {
                    if let Some((armor, value)) = making.take() {
                        let v = calls::values(&c.args.clone().map(|a| a.0).unwrap_or_default())
                            .unwrap_or_default();
                        if v.get(1) == Some(&Value::Int(0)) {
                            dyed.insert(armor, value);
                            since.insert(armor, value);
                            asked += 1;
                        }
                    }
                }
                if c.function.0 == profile::EXAMINATION_PROFILE {
                    let v = calls::values(&c.args.clone().map(|a| a.0).unwrap_or_default())
                        .unwrap_or_default();
                    let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
                        continue;
                    };
                    let object = profile::read_request(bytes).object;
                    let (Some(value), Some(look), Some(t), Some(theirs)) = (
                        since.get(&object),
                        looks.get(&object),
                        template.get(&object),
                        icon_in(bytes, 20),
                    ) else {
                        continue;
                    };
                    let props = yalain_content::Props::of(*t, &|id| entity(inst, id), &prop_name)
                        .with_worn_of(*t);
                    let Some(table) = yalain_world::equip::dye_table(&props) else {
                        continue;
                    };
                    let tables = yalain_content::icon_tables(table, &|did| {
                        auberean_assets::decode(did, &inst.dat.read(did).ok()?, &inst.ctx).ok()
                    });
                    let mut mine = yalain_content::icon_layers(*look, &|did| {
                        auberean_assets::decode(did, &inst.dat.read(did).ok()?, &inst.ctx).ok()
                    });
                    for key in DYE_KEYS {
                        yalain_content::apply_icon_value(&mut mine, &tables, key, *value);
                    }
                    examined += 1;
                    colored += usize::from(mine == theirs);
                }
                continue;
            }
            Ok(ServerMessage::CreateObject(o)) => o,
            _ => continue,
        };
        template.insert(o.id, o.weenie.entity_did);
        looks.insert(o.id, o.visual.parent);
        since.remove(&o.id);
        if let Some(value) = dyed.remove(&o.id) {
            let holds = o.visual.global.appearance.entries.iter().any(|(_, t)| {
                DYE_KEYS
                    .iter()
                    .all(|k| t.get(*k).is_some_and(|v| (v - value).abs() < 0.0001))
            });
            held += usize::from(holds);
            let props =
                yalain_content::Props::of(o.weenie.entity_did, &|id| entity(inst, id), &prop_name)
                    .with_worn_of(o.weenie.entity_did);
            let table = yalain_world::equip::dye_table(&props);
            tabled += usize::from(
                o.visual
                    .global
                    .appearance
                    .entries
                    .iter()
                    .any(|(t, _)| Some(*t) == table),
            );
            assert!(holds, "{:x} after a dye of {value}", o.id);
        }
    }
    println!("armor dyed: {asked}; held the plant's value at their next making: {held}");
    println!("in the table the server writes: {tabled}");
    println!("examined since their dye, before their next making: {colored} of {examined}");
    assert_eq!(tabled, held, "a dye's table");
    assert_eq!(colored, examined, "a dyed item's icon is its plant's");
    assert!(held > 0 || asked == 0);
}

/// What a recorded dye plant profile shows: its lines' string ids in order, its countdown, the
/// numbers of its lines' variables.
fn plant_lines(bytes: &[u8], request_len: usize) -> Option<(Vec<u32>, Option<f64>)> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    let mut strings = Vec::new();
    let mut countdown = None;
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                strings.push(TextInfo::unpack(&mut r).ok()?.string_id);
            }
            0x4000_0006 => {}
            0x4000_0008 => {
                r.u32().ok()?;
                countdown = Some(f64::from_bits(r.u64().ok()?));
            }
            _ => return None,
        }
    }
    Some((strings, countdown))
}

/// Every dye plant's profile 2017 sent is the profile this server builds for a plant with the
/// needs, pollen and ripening time the profile shows: the plant's name, an empty description, its
/// water, fertilizer, pollen and countdown (or that it is mature), its level, kingdom (its
/// planter's), health, vigor and armor.
/// Behaviour: server.plants.examined
#[test]
fn dye_plant_profiles_are_built_as_2017_built_them() {
    use yalain_world::plants::{self, text, Growing, Shown};
    let path = auberean_testkit::data::require_capture_index();
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let con = open(&path);
    // The sessions that saw a dye plant made.
    let made: Vec<String> = plants::SPECIES
        .iter()
        .map(|s| {
            let b = s.plant.to_le_bytes();
            format!(
                "instr(m.raw, X'{:02X}{:02X}{:02X}{:02X}') > 0",
                b[0], b[1], b[2], b[3]
            )
        })
        .collect();
    let sessions: Vec<i64> = con
        .prepare(&format!(
            "SELECT DISTINCT m.session_id FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.mtype = 65624 AND ({})",
            made.join(" OR ")
        ))
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("runs")
        .filter_map(Result::ok)
        .collect();
    let name_of = |p: u32| {
        inst.ctx
            .properties()
            .and_then(|l| l.names.name(p))
            .map(str::to_string)
    };
    let (mut built, mut differ) = (0, 0);
    for sid in sessions {
        let mut st = con
            .prepare(
                "SELECT m.raw FROM message m WHERE m.session_id = ?1 AND m.status = 'ac2' \
                 AND m.mtype IN (65624, 65638) ORDER BY m.idx",
            )
            .expect("prepares");
        let mut rows = st.query([sid]).expect("runs");
        let mut templates: HashMap<u64, u32> = HashMap::new();
        while let Some(r) = rows.next().expect("a row") {
            let raw: Vec<u8> = r.get(0).expect("raw");
            let call = match ServerMessage::decode(&raw) {
                Ok(ServerMessage::CreateObject(o)) => {
                    templates.insert(o.id, o.weenie.entity_did);
                    continue;
                }
                Ok(ServerMessage::PlayerCall(c))
                    if c.function.0 == profile::EXAMINATION_PROFILE =>
                {
                    c
                }
                _ => continue,
            };
            let v = calls::values(&call.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
            let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
                continue;
            };
            let req = profile::read_request(bytes);
            let Some(&template) = templates.get(&req.object) else {
                continue;
            };
            if !plants::SPECIES.iter().any(|s| s.plant == template) {
                continue;
            }
            let len = if req.kind == profile::request::OBJECT {
                20
            } else {
                28
            };
            let (Some((strings, countdown)), Some(n)) =
                (plant_lines(bytes, len), numbers_in_any(bytes, len))
            else {
                differ += 1;
                continue;
            };
            let after = |label: u32| {
                strings
                    .iter()
                    .position(|s| *s == label)
                    .and_then(|i| strings.get(i + 1))
                    .copied()
            };
            let level_of = |id: Option<u32>| {
                id.and_then(|x| plants::NEED_LEVELS.iter().position(|l| *l == x))
                    .and_then(|i| i32::try_from(i).ok())
            };
            let growing = match (
                level_of(after(text::WATER)),
                level_of(after(text::FERTILIZER)),
                after(text::POLLEN),
                countdown,
            ) {
                (Some(water), Some(fertilizer), Some(pollen), Some(ripe_game)) => Some(Growing {
                    water,
                    fertilizer,
                    pollen,
                    ripe_game,
                }),
                _ => None,
            };
            let kingdom = [1, 2, 4]
                .into_iter()
                .find(|f| strings.contains(&profile::kingdom(*f)))
                .unwrap_or(0);
            let f = facts(template, &|id| entity(inst, id), &inst.ids);
            let props = yalain_content::Props::of(template, &|id| entity(inst, id), &name_of);
            let shown = Shown {
                name: f.name,
                level: f.level.unwrap_or(1),
                kingdom,
                vitals: Vitals {
                    health: n.get(&var::HEALTH).copied().unwrap_or(0),
                    max_health: n.get(&var::MAX_HEALTH).copied().unwrap_or(0),
                    vigor: n.get(&var::VIGOR).copied().unwrap_or(0),
                    max_vigor: n.get(&var::MAX_VIGOR).copied().unwrap_or(0),
                    pk_damage: 0,
                    pk_vigor_loss: 0,
                },
                armor: props
                    .int("NPC_ArmorLevel")
                    .and_then(|a| i32::try_from(a).ok())
                    .unwrap_or(0),
            };
            let ours = plants::profile_of(req.asked, &shown, growing).packed(&bytes[..len]);
            if ours == *bytes {
                built += 1;
            } else {
                differ += 1;
            }
        }
    }
    println!("2017 dye plant profiles: {built} as built, {differ} not");
    assert!(built > 0);
    assert_eq!(differ, 0);
}

/// A recorded profile's text lines' number variables, by id, reading past countdowns too.
fn numbers_in_any(bytes: &[u8], request_len: usize) -> Option<BTreeMap<u32, u32>> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    let mut out = BTreeMap::new();
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                for v in TextInfo::unpack(&mut r).ok()?.variables {
                    if let VariableValue::Word(x) = v.value {
                        out.insert(v.id, x);
                    }
                }
            }
            0x4000_0006 => {}
            0x4000_0008 => {
                r.u32().ok()?;
                r.u64().ok()?;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Every recorded profile's icon is its look's icon with what its item's appearance values give it,
/// through their tables and those tables' base tables. The residue measured on 2026-11-04 is 6 of
/// 8,038: five pieces of armor examined after a dye, their last making from before it (the dye's
/// own test holds those).
/// Behaviour: server.examination.an-items-icon-takes-its-appearance
#[test]
fn an_items_icon_takes_what_its_appearance_gives_as_2017() {
    let path = auberean_testkit::data::require_capture_index();
    let Some(inst) = installed() else {
        println!("no September 2013 install here");
        return;
    };
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.dir = 's2c' AND m.mtype IN (65624, 65638) ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    // Each object's look and its appearance values, by table.
    type Values = Vec<(u32, Vec<(u32, f32)>)>;
    let mut made: HashMap<u64, (u32, Values)> = HashMap::new();
    let mut tables: HashMap<u32, Vec<yalain_content::IconTable>> = HashMap::new();
    let mut session = -1;
    let (mut seen, mut built, mut changed) = (0usize, 0usize, 0usize);
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("session");
        if sid != session {
            made.clear();
            session = sid;
        }
        let raw: Vec<u8> = r.get(1).expect("raw");
        let Ok(m) = ServerMessage::decode(&raw) else {
            continue;
        };
        let call = match m {
            ServerMessage::CreateObject(o) => {
                let values = o
                    .visual
                    .global
                    .appearance
                    .entries
                    .iter()
                    .map(|(t, k)| (*t, k.entries.clone()))
                    .collect();
                made.insert(o.id, (o.visual.parent, values));
                continue;
            }
            ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => c,
            _ => continue,
        };
        let v = calls::values(&call.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        let req = profile::read_request(bytes);
        if req.kind != profile::request::OBJECT {
            continue;
        }
        let (Some((look, values)), Some(theirs)) = (made.get(&req.object), icon_in(bytes, 20))
        else {
            continue;
        };
        let visual =
            |did: u32| auberean_assets::decode(did, &inst.dat.read(did).ok()?, &inst.ctx).ok();
        let plain = yalain_content::icon_layers(*look, &visual);
        let mut mine = plain.clone();
        for (table, keys) in values {
            let t = tables.entry(*table).or_insert_with(|| {
                yalain_content::icon_tables(*table, &|did| {
                    auberean_assets::decode(did, &inst.dat.read(did).ok()?, &inst.ctx).ok()
                })
            });
            for (key, value) in keys {
                yalain_content::apply_icon_value(&mut mine, t, *key, *value);
            }
        }
        seen += 1;
        built += usize::from(mine == theirs);
        changed += usize::from(plain != theirs);
    }
    println!("icons built: {built} of {seen}; other than their look's own: {changed}");
    assert!(
        changed > 2_000,
        "the recordings hold changed icons: {changed}"
    );
    assert!(seen - built <= 6, "icons built: {built} of {seen}");
}

/// An item a 2017 examination showed: its least level, how many enchantments, and whether its
/// rating is a shield's; a shield's heading and numbers.
#[derive(Clone, Copy)]
struct Examined {
    least: Option<u32>,
    enchantments: usize,
    shield: bool,
    shield_heading: bool,
    rating: Option<u32>,
    damage: Option<(u32, u32)>,
    delay: Option<u32>,
    vigor: Option<u32>,
}

/// A profile's damage line: its least and greatest.
fn damage_span_in(bytes: &[u8], request_len: usize) -> Option<(u32, u32)> {
    let lines = strings_and_texts(bytes, request_len)?;
    let t = lines
        .iter()
        .find(|t| t.string_id == profile::item_line::DAMAGE_VALUE)?;
    let var = |id: u32| {
        t.variables.iter().find_map(|v| match v.value {
            VariableValue::Word(w) if v.id == id => Some(w),
            _ => None,
        })
    };
    Some((
        var(profile::item_var::DAMAGE_LOW)?,
        var(profile::item_var::DAMAGE_HIGH)?,
    ))
}

/// A profile's text lines, passing over every other kind of node.
fn strings_and_texts(bytes: &[u8], request_len: usize) -> Option<Vec<TextInfo>> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 | 0x4000_000C => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                out.push(TextInfo::unpack(&mut r).ok()?);
            }
            0x4000_0005 | 0x4000_0007 => {
                r.u32().ok()?;
            }
            0x4000_0006 => {}
            0x4000_0008 | 0x4000_000A => {
                r.u32().ok()?;
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0009 => {
                let k = r.u32().ok()?;
                for _ in 0..k * 3 {
                    r.u32().ok()?;
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Every item not made by a player that the January 2017 recordings show examined, once, by its
/// object id.
fn examined_items(con: &Connection) -> HashMap<u64, Examined> {
    let mut st = con
        .prepare(
            "SELECT m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.dir = 's2c' AND m.mtype = 65638",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let mut out = HashMap::new();
    while let Some(r) = rows.next().expect("a row") {
        let raw: Vec<u8> = r.get(0).expect("raw");
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if c.function.0 != profile::EXAMINATION_PROFILE {
            continue;
        }
        let v = calls::values(&c.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        let req = profile::read_request(bytes);
        if req.kind != profile::request::OBJECT {
            continue;
        }
        let strings = strings_in(bytes, 20);
        if strings.contains(&profile::item_line::CRAFTED_BY) {
            continue;
        }
        out.entry(req.object).or_insert(Examined {
            least: stat_in(bytes, 20, profile::line::MINIMUM_LEVEL),
            enchantments: strings
                .iter()
                .filter(|x| **x == profile::item_line::EFFECT_NAME)
                .count(),
            shield: strings.contains(&profile::item_line::SHIELD_RATING),
            shield_heading: strings.contains(&profile::item_line::SHIELD_STATS),
            rating: stat_in(bytes, 20, profile::item_line::SHIELD_RATING),
            damage: damage_span_in(bytes, 20),
            delay: stat_in(bytes, 20, profile::item_line::COMBAT_DELAY),
            vigor: stat_in(bytes, 20, profile::item_line::VIGOR_COST),
        });
    }
    out
}

/// A weapon, shield or piece of armor a 2017 corpse held at its first opening: its template, whether
/// its template has a weapon's damage, whether its class is a shield's, its own value, its dropper's
/// level and loot quality, and its examination if a player examined it.
struct Dropped {
    template: u32,
    weapon: bool,
    shield: bool,
    value: u32,
    level: i32,
    quality: Option<i64>,
    examined: Option<Examined>,
}

/// Does a script package derive from the shields' class?
fn is_shield_class(lib: &auberean_script::library::Library, package: u32) -> bool {
    yalain_world::treasure::SHIELDS
        .iter()
        .filter_map(|n| lib.package_id(n))
        .any(|b| lib.derives_from(package, b))
}

/// A template made known to a harness's server as treasure equipment, as the server reads the
/// install: a shield when its class is a shield's.
fn treasure_template(
    h: &mut yalain_testkit::Harness,
    portal: &yalain_testkit::authored::Portal,
    lib: &auberean_script::library::Library,
    template: u32,
) {
    let t = portal.template(template);
    let package = t.description.package;
    h.server.treasure.equipment.insert(package);
    if is_shield_class(lib, package) {
        h.server.treasure.shields.insert(package);
    }
    h.server.catalog.insert(t);
}

/// Every weapon, shield and piece of armor with a value of its own in the corpses the January 2017
/// recordings show opened, of creatures with a loot profile.
fn equipment_dropped() -> &'static [Dropped] {
    static D: OnceLock<Vec<Dropped>> = OnceLock::new();
    D.get_or_init(|| {
        let portal = yalain_testkit::authored::Portal::open();
        let lib = portal.library();
        let corpses = super::world_db::opened_corpses(&lib);
        let examined = examined_items(&open(&auberean_testkit::data::require_capture_index()));
        let mut stats = HashMap::new();
        let mut equipment = HashMap::new();
        let mut out = Vec::new();
        for c in corpses {
            let Some(s) = stats
                .entry(c.creature)
                .or_insert_with(|| {
                    yalain_content::CreatureStats::from_props(&portal.template(c.creature).props)
                        .filter(|s| s.loot_profile.is_some())
                })
                .clone()
            else {
                continue;
            };
            for (&(template, value), id) in c.items.iter().zip(&c.ids) {
                let (weapon, armor, shield) = *equipment.entry(template).or_insert_with(|| {
                    let t = portal.template(template);
                    (
                        t.props.int("Weapon_Damage").is_some(),
                        t.props.int("Item_ArmorLevel").is_some(),
                        is_shield_class(&lib, t.description.package),
                    )
                });
                if template == 0 || value == 0 || !(weapon || armor) {
                    continue;
                }
                out.push(Dropped {
                    template,
                    weapon,
                    shield,
                    value,
                    level: s.level,
                    quality: s.loot_quality,
                    examined: examined.get(id).copied(),
                });
            }
        }
        out
    })
}

/// Weapons, shields and armor whose templates set their own least level were treasure like any
/// other in 2017: an examined drop's least level was its dropper's base and 0 to 3 more, at most
/// 50, many such templates showed several, and some were enchanted. This server's drops of the same
/// templates, from the same droppers, draw theirs the same way, with numbers and enchantments.
/// Behaviour: server.loot.treasure-set-least-level-drawn
#[test]
fn equipment_whose_template_sets_its_least_level_drops_as_any_other_treasure() {
    use std::collections::{BTreeSet, HashSet};
    use yalain_world::treasure::{base_of, endow, MOST_LEVEL};
    let portal = yalain_testkit::authored::Portal::open();
    let mut set = HashMap::new();
    let drops: Vec<&Dropped> = equipment_dropped()
        .iter()
        .filter(|d| {
            *set.entry(d.template)
                .or_insert_with(|| portal.template(d.template).props.sets_least_level())
        })
        .collect();
    // 2017: each examined drop's least level against its dropper's base.
    let rule = |least: i32, base: i32| {
        if base >= MOST_LEVEL {
            least == MOST_LEVEL
        } else {
            (base..=base + 3).contains(&least)
        }
    };
    let (mut examined, mut within, mut enchanted) = (0usize, 0usize, 0usize);
    let mut levels: HashMap<u32, BTreeSet<u32>> = HashMap::new();
    for d in &drops {
        let Some(e) = d.examined else { continue };
        let Some(least) = e.least else { continue };
        examined += 1;
        within += usize::from(rule(
            i32::try_from(least).unwrap_or(0),
            base_of(d.level, d.quality),
        ));
        enchanted += usize::from(e.enchantments > 0);
        levels.entry(d.template).or_default().insert(least);
    }
    let several = levels.values().filter(|l| l.len() > 1).count();
    println!(
        "2017: {} drops of {} templates; {examined} examined, {within} at their dropper's least \
         level, {enchanted} enchanted; {several} of {} templates at two or more least levels",
        drops.len(),
        drops
            .iter()
            .map(|d| d.template)
            .collect::<HashSet<_>>()
            .len(),
        levels.len()
    );
    assert!(examined >= 100, "the recordings hold them: {examined}");
    assert!(within * 100 >= examined * 95, "{within} of {examined}");
    assert!(several >= 10, "{several} of {}", levels.len());
    assert!(enchanted >= 10, "{enchanted} of {examined}");
    // Ours: every drop again, from its own dropper.
    let mut h = yalain_testkit::Harness::new();
    let lib = portal.library();
    let (mut ours, mut drawn, mut numbered, mut ours_enchanted) = (0usize, 0usize, 0usize, 0usize);
    for d in &drops {
        treasure_template(&mut h, &portal, &lib, d.template);
        let item = yalain_world::inventory::make_item(&mut h.server, d.template, 1, d.value)
            .expect("made");
        endow(&mut h.server, item, d.level, d.quality);
        let thing = h.server.things.get(item).expect("made");
        ours += 1;
        drawn += usize::from(
            thing
                .own
                .min_level
                .is_some_and(|l| rule(l, base_of(d.level, d.quality))),
        );
        numbered += usize::from(thing.own.made.is_some());
        ours_enchanted += usize::from(!thing.enchantments.is_empty() || !thing.bonuses.is_empty());
    }
    println!("ours: {drawn} of {ours} at their dropper's least level, {numbered} with numbers, {ours_enchanted} enchanted");
    assert_eq!((drawn, numbered), (ours, ours));
    assert!(
        ours_enchanted * 100 >= ours * 5,
        "{ours_enchanted} of {ours}"
    );
}

/// The step of a least level 2017's values go by: five levels each from 1, then 46 to 49, then 50.
fn value_step(least: i32) -> usize {
    match least {
        50.. => 10,
        46..=49 => 9,
        _ => usize::try_from((least.max(1) - 1) / 5).unwrap_or(0),
    }
}

/// The level band treasure's enchantment counts go by.
fn count_band(level: i32) -> usize {
    usize::try_from(level.clamp(0, 50) / 10).unwrap_or(0)
}

/// Treasure weapons, shields and armor are enchanted as often as 2017's drops were, by sort and level
/// band from 10: each drop's count is read from its own value, a plain one's at its least level (the
/// median of the examined drops' values over one more than their counts) times one more than its
/// count, at the commonest least level, its dropper's base and one. The reading agrees with the
/// examinations; the counts this server draws from give the same shares; and this server's drops,
/// endowed again from the same droppers, are enchanted as often.
/// Behaviour: server.loot.treasure-enchantments-as-dropped
#[test]
fn treasure_equipment_is_enchanted_as_often_as_2017s_drops() {
    use std::collections::HashSet;
    use yalain_content::treasure_effects::{ARMOR_COUNTS, SHIELD_COUNTS, WEAPON_COUNTS};
    use yalain_world::treasure::{base_of, endow, MOST_LEVEL};
    const SORTS: [&str; 3] = ["weapons", "armor", "shields"];
    let drops = equipment_dropped();
    let mut steps: Vec<Vec<f64>> = vec![Vec::new(); 11];
    for d in drops {
        if let Some(Examined {
            least: Some(least),
            enchantments,
            ..
        }) = d.examined
        {
            #[allow(clippy::cast_precision_loss)]
            steps[value_step(i32::try_from(least).unwrap_or(0))]
                .push(f64::from(d.value) / (1 + enchantments) as f64);
        }
    }
    let unit: Vec<f64> = steps
        .iter_mut()
        .map(|v| {
            v.sort_by(f64::total_cmp);
            v.get(v.len() / 2).copied().unwrap_or(1.0)
        })
        .collect();
    let read = |d: &Dropped| {
        let level = (base_of(d.level, d.quality) + 1).min(MOST_LEVEL);
        #[allow(clippy::cast_possible_truncation)]
        let n = (f64::from(d.value) / unit[value_step(level)]).round() as i64 - 1;
        (level, n.clamp(0, 3))
    };
    let (mut joined, mut agree) = (0usize, 0usize);
    for d in drops {
        if let Some(e) = d.examined.filter(|e| e.least.is_some()) {
            joined += 1;
            agree += usize::from((read(d).1 > 0) == (e.enchantments > 0));
        }
    }
    println!("read from value, as examined: {agree} of {joined}");
    assert!(joined > 1_500, "the recordings hold them: {joined}");
    assert!(agree * 100 >= joined * 95, "{agree} of {joined}");
    let shields: HashSet<u32> = drops
        .iter()
        .filter(|d| d.examined.is_some_and(|e| e.shield))
        .map(|d| d.template)
        .collect();
    let sort = |d: &Dropped| {
        if d.weapon {
            0
        } else if shields.contains(&d.template) {
            2
        } else {
            1
        }
    };
    // [sort][band]: drops, enchanted.
    let mut theirs = [[[0usize; 2]; 6]; 3];
    for d in drops {
        let (level, n) = read(d);
        let at = &mut theirs[sort(d)][count_band(level)];
        at[0] += 1;
        at[1] += usize::from(n > 0);
    }
    #[allow(clippy::cast_precision_loss)]
    let share = |x: [usize; 2]| x[1] as f32 / x[0].max(1) as f32;
    for (s, counts) in [WEAPON_COUNTS, ARMOR_COUNTS, SHIELD_COUNTS]
        .iter()
        .enumerate()
    {
        for band in 1..6 {
            let c = counts[band];
            let ours = (c[1] + c[2] + c[3]) / (c[0] + c[1] + c[2] + c[3]);
            let t = theirs[s][band];
            println!(
                "{} {}0s: 2017 {:.3} of {}, the counts {ours:.3}",
                SORTS[s],
                band,
                share(t),
                t[0]
            );
            assert!(t[0] >= 50, "{} {band}: {t:?}", SORTS[s]);
            assert!(
                (share(t) - ours).abs() <= 0.04,
                "{} {band}: {} {ours}",
                SORTS[s],
                share(t)
            );
        }
    }
    // This server's drops of the same templates from the same droppers, by the dropper's band.
    const AGAIN: usize = 5;
    let portal = yalain_testkit::authored::Portal::open();
    let lib = portal.library();
    let mut h = yalain_testkit::Harness::new();
    let mut known = HashSet::new();
    // [weapons, armor and shields][dropper's band]: 2017's drops, enchanted; ours, enchanted.
    let mut by = [[[0usize; 4]; 6]; 2];
    for d in drops {
        if known.insert(d.template) {
            treasure_template(&mut h, &portal, &lib, d.template);
        }
        let at = &mut by[usize::from(!d.weapon)][count_band(base_of(d.level, d.quality))];
        at[0] += 1;
        at[1] += usize::from(read(d).1 > 0);
        for _ in 0..AGAIN {
            let item = yalain_world::inventory::make_item(&mut h.server, d.template, 1, d.value)
                .expect("made");
            endow(&mut h.server, item, d.level, d.quality);
            let thing = h.server.things.remove(item).expect("made");
            at[2] += 1;
            at[3] += usize::from(!thing.enchantments.is_empty() || !thing.bonuses.is_empty());
        }
    }
    for (s, name) in ["weapons", "armor and shields"].iter().enumerate() {
        for (band, at) in by[s].iter().enumerate() {
            let (t, o) = (share([at[0], at[1]]), share([at[2], at[3]]));
            println!(
                "{name} from {band}0s droppers: 2017 {t:.3} of {}, ours {o:.3} of {}",
                at[0], at[2]
            );
            if band > 0 {
                assert!((t - o).abs() <= 0.05, "{name} {band}: 2017 {t} ours {o}");
            }
        }
    }
}

/// Dropped shields were shields in 2017 whatever their templates hold: every one examined showed a
/// shield's heading, damage, rating, combat delay and vigor cost. This server's numbers for a shield
/// hold most examined plain ones' within what it draws at their item level, and its drops of the same
/// templates from the same droppers carry a shield's numbers, enchantments only of the shields'
/// families and a shield's word.
/// Behaviour: server.loot.treasure-shields-by-class
#[test]
fn dropped_shields_carry_a_shields_numbers_as_2017s_did() {
    use std::collections::HashSet;
    use yalain_content::treasure_effects as t;
    use yalain_content::treasure_words as w;
    use yalain_world::treasure::{base_of, endow, numbers, Sort};
    const DRAWS: usize = 200;
    const NUMBERS: [&str; 5] = [
        "rating",
        "greatest damage",
        "least damage",
        "delay",
        "vigor cost",
    ];
    let drops: Vec<&Dropped> = equipment_dropped().iter().filter(|d| d.shield).collect();
    let examined: Vec<(&Dropped, Examined)> = drops
        .iter()
        .filter_map(|d| d.examined.filter(|e| e.least.is_some()).map(|e| (*d, e)))
        .collect();
    let shown = examined
        .iter()
        .filter(|(_, e)| {
            e.shield
                && e.shield_heading
                && e.rating.is_some()
                && e.damage.is_some()
                && e.delay.is_some()
                && e.vigor.is_some()
        })
        .count();
    println!(
        "2017: {} dropped shields, {} examined, {shown} shown as shields",
        drops.len(),
        examined.len()
    );
    assert!(
        examined.len() >= 150,
        "the recordings hold them: {}",
        examined.len()
    );
    assert_eq!(shown, examined.len());
    // Each plain one's numbers against what this server draws at its item level.
    let portal = yalain_testkit::authored::Portal::open();
    let lib = portal.library();
    let mut rng = yalain_world::rng::Rng::new(753);
    let (mut plain, mut within) = (0usize, [0usize; 5]);
    for (d, e) in examined.iter().filter(|(_, e)| e.enchantments == 0) {
        let least = i32::try_from(e.least.unwrap_or(0)).unwrap_or(0);
        let level = least.max(base_of(d.level, d.quality));
        let props = portal.template(d.template).props;
        let (mut low, mut high) = ([i32::MAX; 5], [i32::MIN; 5]);
        for _ in 0..DRAWS {
            let m = numbers(&mut rng, &props, Sort::Shield, level, d.value).expect("a shield's");
            let (most, span) = m.damage.expect("its damage");
            #[allow(clippy::cast_precision_loss)]
            let fewest = dereth_primitives::num::floor_to_i32(most as f32 * (1.0 - span));
            let drawn = [
                m.armor.unwrap_or(0),
                most,
                fewest,
                m.delay.unwrap_or(0),
                m.vigor.unwrap_or(0),
            ];
            for i in 0..5 {
                low[i] = low[i].min(drawn[i]);
                high[i] = high[i].max(drawn[i]);
            }
        }
        let (fewest, most) = e.damage.unwrap_or_default();
        let theirs = [
            e.rating.unwrap_or(0),
            most,
            fewest,
            e.delay.unwrap_or(0),
            e.vigor.unwrap_or(0),
        ];
        plain += 1;
        for i in 0..5 {
            let n = i32::try_from(theirs[i]).unwrap_or(-1);
            within[i] += usize::from((low[i]..=high[i]).contains(&n));
        }
    }
    for (name, n) in NUMBERS.iter().zip(within) {
        println!("2017's plain shields' {name} within what this server draws: {n} of {plain}");
    }
    assert!(plain >= 150, "{plain}");
    for (name, n) in NUMBERS.iter().zip(within) {
        assert!(n * 100 >= plain * 88, "{name}: {n} of {plain}");
    }
    // This server's drops of the same templates from the same droppers.
    let of = |families: &[t::Family], state: u32| {
        families.iter().any(|f| {
            f.tiers
                .iter()
                .any(|x| x.state == state || x.puts_on == state)
        })
    };
    let mut h = yalain_testkit::Harness::new();
    let mut known = HashSet::new();
    let (mut shields, mut enchanted, mut theirs_only) = (0usize, 0usize, 0usize);
    for d in &drops {
        if known.insert(d.template) {
            treasure_template(&mut h, &portal, &lib, d.template);
        }
        let item = yalain_world::inventory::make_item(&mut h.server, d.template, 1, d.value)
            .expect("made");
        endow(&mut h.server, item, d.level, d.quality);
        let thing = h.server.things.remove(item).expect("made");
        let m = thing.own.made.unwrap_or_default();
        shields += usize::from(
            m.damage.is_some()
                && m.armor.is_some()
                && m.vigor.is_some()
                && w::SHIELD_WORDS.iter().any(|x| x.word == m.prefix),
        );
        let held: Vec<u32> = thing
            .enchantments
            .iter()
            .chain(&thing.bonuses)
            .map(|e| e.0)
            .collect();
        enchanted += usize::from(!held.is_empty());
        theirs_only += usize::from(held.iter().all(|s| of(t::SHIELD_FAMILIES, *s)));
    }
    println!(
        "ours: {shields} of {} with a shield's numbers and word, {enchanted} enchanted, {theirs_only} \
         with the shields' enchantments only",
        drops.len()
    );
    assert_eq!((shields, theirs_only), (drops.len(), drops.len()));
    assert!(
        enchanted * 100 >= drops.len() * 5,
        "{enchanted} of {}",
        drops.len()
    );
}

/// Treasure items of the templates known to draw their look, as the 2017 servers made them in
/// corpses: the same table and keys, the values within what each key draws.
/// Behaviour: server.loot.treasure-look-drawn
#[test]
fn treasure_items_look_values_fall_within_their_rows() {
    use yalain_content::treasure_looks::{GLOW, TREASURE_LOOKS};
    const CORPSE: u32 = 0x4700_0539;
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut st = con
        .prepare(
            "SELECT m.session_id, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE c.file LIKE '2017-%' AND m.status = 'ac2' \
             AND m.dir = 's2c' AND m.mtype = 65624 ORDER BY m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let rows_by: HashMap<u32, yalain_content::treasure_looks::Row> =
        TREASURE_LOOKS.iter().map(|row| (row.0, *row)).collect();
    let mut corpses = std::collections::HashSet::new();
    let mut session = -1;
    let (mut values, mut within, mut plain_glow) = (0usize, 0usize, 0usize);
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("session");
        if sid != session {
            corpses.clear();
            session = sid;
        }
        let raw: Vec<u8> = r.get(1).expect("raw");
        let Ok(ServerMessage::CreateObject(o)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if o.weenie.entity_did == CORPSE {
            corpses.insert(o.id);
            continue;
        }
        if !corpses.contains(&o.weenie.objects[0]) {
            continue;
        }
        let Some((_, table, keys)) = rows_by.get(&o.weenie.entity_did) else {
            continue;
        };
        for (t, kv) in &o.visual.global.appearance.entries {
            for (k, v) in &kv.entries {
                values += 1;
                within += usize::from(
                    t == table && keys.iter().any(|(key, top)| key == k && *v <= *top + 1e-4),
                );
                plain_glow += usize::from(*k == GLOW && *v <= yalain_world::treasure::GLOW_DRAW);
            }
        }
    }
    println!("look values within their rows: {within} of {values}; plain glows {plain_glow}");
    assert!(values > 4_000, "the recordings hold them: {values}");
    assert!(values - within <= values / 50, "{within} of {values}");
}

fn icon_in(bytes: &[u8], request_len: usize) -> Option<Vec<[u32; 3]>> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0001..=0x4000_0003 | 0x4000_000C => {
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0004 => {
                r.u32().ok()?;
                TextInfo::unpack(&mut r).ok()?;
            }
            0x4000_0005 | 0x4000_0007 => {
                r.u32().ok()?;
            }
            0x4000_0006 => {}
            0x4000_0008 | 0x4000_000A => {
                r.u32().ok()?;
                r.u32().ok()?;
                r.u32().ok()?;
            }
            0x4000_0009 => {
                let k = r.u32().ok()?;
                let mut out = Vec::new();
                for _ in 0..k {
                    out.push([r.u32().ok()?, r.u32().ok()?, r.u32().ok()?]);
                }
                return Some(out);
            }
            _ => return None,
        }
    }
    None
}

/// A recorded corpse's profile, read: its text lines.
fn text_lines(bytes: &[u8], request_len: usize) -> Option<Vec<TextInfo>> {
    let mut r = In::new(bytes.get(request_len..)?);
    let n = r.u32().ok()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let ty = r.u32().ok()?;
        r.u32().ok()?;
        match ty {
            0x4000_0004 => {
                r.u32().ok()?;
                out.push(TextInfo::unpack(&mut r).ok()?);
            }
            0x4000_0006 => {}
            _ => return None,
        }
    }
    Some(out)
}

fn literal_of(v: &auberean_script::native_object::Variable) -> Option<String> {
    let VariableValue::Nested(t) = &v.value else {
        return None;
    };
    t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
}

/// Every recorded examination of a corpse is, byte for byte, what this server builds from who
/// killed it, who butchered and first opened it, whether the one asking may loot it, and how much
/// it is known to hold.
/// Behaviour: server.loot.an-examination-as-2017
#[test]
fn corpse_profiles_are_built_as_2017_built_them() {
    use profile::corpse_line as l;
    use profile::line;
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut st = con
        .prepare("SELECT raw FROM message WHERE status = 'ac2' AND dir = 's2c' AND mtype = 65638")
        .expect("prepares");
    let mut rows = st.query([]).expect("runs");
    let (mut seen, mut exact) = (0, 0);
    while let Some(row) = rows.next().expect("a row") {
        let raw: Vec<u8> = row.get(0).expect("raw");
        let Ok(ServerMessage::PlayerCall(c)) = ServerMessage::decode(&raw) else {
            continue;
        };
        if c.function.0 != profile::EXAMINATION_PROFILE {
            continue;
        }
        let v = calls::values(&c.args.map(|a| a.0).unwrap_or_default()).unwrap_or_default();
        let Some(Node::Native { bytes, .. }) = v.first().and_then(Value::root) else {
            continue;
        };
        let Some(lines) = text_lines(bytes, 20) else {
            continue;
        };
        let has = |id: u32| {
            lines
                .iter()
                .find(|t| t.string_id == id && t.table == line::TABLE)
        };
        let may_loot = has(l::YOU_CAN_LOOT).is_some();
        if !may_loot && has(l::YOU_CANNOT_LOOT).is_none() {
            continue;
        }
        seen += 1;
        let name_in = |id: u32, var: u32| -> Option<String> {
            has(id)?
                .variables
                .iter()
                .find(|v| v.id == var)
                .and_then(literal_of)
        };
        let killed_by = has(l::KILLED_BY)
            .and_then(|t| t.variables.iter().find(|v| v.id == l::KILLER))
            .and_then(|v| match &v.value {
                VariableValue::Nested(t) => Some((**t).clone()),
                _ => None,
            });
        let holds = has(l::CONTAINS).and_then(|t| {
            t.variables.iter().find_map(|v| match (v.id, &v.value) {
                (l::HOW_MANY, VariableValue::Word(n)) => Some(*n),
                _ => None,
            })
        });
        let view = profile::CorpseView {
            name: lines[0].clone(),
            killed_by: killed_by.unwrap_or_default(),
            butchered_by: name_in(l::BUTCHERED_BY, l::NAMED),
            first_looted_by: name_in(l::FIRST_LOOTED_BY, l::NAMED),
            may_loot,
            holds,
        };
        let req = profile::read_request(bytes);
        if profile::corpse(req.asked, &view).packed(&bytes[..20]) == bytes[..] {
            exact += 1;
        }
    }
    println!("{exact} of {seen} recorded corpse examinations as this server builds them");
    assert!(seen >= 13_000, "the recordings hold them: {seen}");
    assert!(exact * 1000 >= seen * 995, "{exact} of {seen}");
}

#[derive(Default)]
struct CorpseTold {
    born: f64,
    creature: u64,
    touched: bool,
    gone: Option<f64>,
}

/// A corpse's life in one session, where it was told appearing and leaving and nobody opened it.
fn corpse_lives(con: &Connection, session: i64) -> Vec<(u32, f64)> {
    let mut st = con
        .prepare(
            "SELECT t, raw FROM message WHERE session_id = ?1 AND dir = 's2c' AND status = 'ac2'
             AND mtype IN (65624, 65727, 65644, 65652) ORDER BY idx",
        )
        .expect("prepares");
    let mut rows = st.query([session]).expect("runs");
    let mut templates: HashMap<u64, u32> = HashMap::new();
    let mut corpses: HashMap<u64, CorpseTold> = HashMap::new();
    while let Some(row) = rows.next().expect("a row") {
        let t: f64 = row.get(0).expect("t");
        let raw: Vec<u8> = row.get(1).expect("raw");
        match ServerMessage::decode(&raw) {
            Ok(ServerMessage::CreateObject(c)) => {
                let template = c.weenie.entity_did;
                templates.insert(c.id, template);
                if template == yalain_world::death::CORPSE_TEMPLATE {
                    corpses.entry(c.id).or_default().born = t;
                }
            }
            Ok(ServerMessage::ObjectLeaves(e)) => {
                if let Some(c) = corpses.get_mut(&e.sender.id) {
                    c.gone.get_or_insert(t);
                }
            }
            Ok(ServerMessage::UpdateInstanceIdVisual(v))
                if v.quality == yalain_world::death::q::ORIGINATOR =>
            {
                if let Some(c) = corpses.get_mut(&v.object.id) {
                    c.creature = v.value;
                }
            }
            Ok(ServerMessage::UpdateBoolVisual(v))
                if v.quality == yalain_world::inventory::q::OPEN =>
            {
                if let Some(c) = corpses.get_mut(&v.object.id) {
                    c.touched = true;
                }
            }
            _ => {}
        }
    }
    corpses
        .values()
        .filter(|c| !c.touched)
        .filter_map(|c| Some((*templates.get(&c.creature)?, c.gone? - c.born)))
        .collect()
}

/// Corpses nobody opened: of creatures with a loot profile they last the time for one untouched,
/// of those with none the time for an empty one, whatever that creature's template makes it.
/// Those that left a client's sight sooner count as neither. (No profile: 1,096 of 1,159. A
/// profile: 706 at the time for one untouched, 11 at the time for an empty one.)
/// Behaviour: server.corpse.recorded-times
#[test]
fn corpses_nobody_opened_lasted_their_templates_times_in_2017() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let sessions: Vec<i64> = con
        .prepare(
            "SELECT DISTINCT s.session_id FROM session s JOIN capture c USING (capture_id)
             WHERE s.game = 'ac2' AND c.file LIKE '2017-%'",
        )
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("runs")
        .map(|r| r.expect("a session"))
        .collect();
    let p = yalain_testkit::authored::Portal::open();
    let mut of: HashMap<u32, (bool, yalain_content::CorpseRot)> = HashMap::new();
    let (mut soon, mut soon_all, mut long, mut long_at_short) = (0, 0, 0, 0);
    for session in sessions {
        for (template, life) in corpse_lives(&con, session) {
            let (profile, rot) = *of.entry(template).or_insert_with(|| {
                let props = p.template(template).props;
                (
                    props.word("NPC_DeathLootProfile").is_some(),
                    yalain_content::CorpseRot::from_props(&props),
                )
            });
            let near = |want: f64| (life - want).abs() < 1.0;
            if profile {
                long += i32::from(near(rot.untouched));
                long_at_short += i32::from(near(rot.empty));
            } else {
                soon_all += 1;
                soon += i32::from(near(rot.empty));
            }
        }
    }
    assert!(soon * 100 >= soon_all * 90, "{soon} of {soon_all}");
    assert!(soon > 500, "{soon}");
    assert!(long > 500, "{long}");
    assert!(long_at_short * 50 <= long, "{long_at_short} against {long}");
}
