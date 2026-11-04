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
        let p = props
            .entry(i.template)
            .or_insert_with(|| {
                yalain_content::Props::of(i.template, &|id| entity(inst, id), &prop_name)
            })
            .clone();
        let is_item = profile::starting_weapon(i.template).is_some()
            || p.get("Item_Value").is_some()
            || p.get("Weapon_Damage").is_some();
        if !is_item || i.named {
            continue;
        }
        let mut view = profile::starting_weapon(i.template)
            .unwrap_or_else(|| ItemView::of(&p, i.quantity, i.value));
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
        *tally.entry(what).or_default() += 1;
    }
    println!("2017 item profiles (template items) against ours:");
    for (k, n) in &tally {
        println!("  {n:7}  {k}");
    }
    println!("  the human starting bow: {} of {}", bow.0, bow.1);
    let built = tally
        .get("as built from the template")
        .copied()
        .unwrap_or(0);
    let other = tally.get("differs otherwise").copied().unwrap_or(0);
    assert!(built > 0);
    assert_eq!(bow.0, bow.1, "the starting bow's profiles");
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
        let props = yalain_content::Props::of(template, &|id| entity(inst, id), &prop_name);
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
            let Some(mut mine) = yalain_world::craft::made_of(
                &crafting, &def, out, ordinal, spinner, &props, &effect, &mut rng,
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
            let mut view = ItemView::of(&props, 1, (mine.value > 0).then_some(mine.value));
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
                view.tool_skills = made::tool_skills(template, &worn, &all_skills.skills)
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
    // The residue measured on 2026-11-04 (4 of 330): a chest piece a point off the curve, a helm
    // dyed since, a knife made as another making was asked for, a shield's races.
    assert!(
        compared == built || compared - built <= 4,
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

/// Of the treasure weapons the 2017 servers showed, by level band: how many have an enchantment, as
/// the counts this server draws from say.
/// Behaviour: server.loot.treasure-enchantments
#[test]
fn treasure_weapons_enchantments_follow_the_counts_this_server_draws_from() {
    use yalain_content::treasure_effects::WEAPON_COUNTS;
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
    let mut bands = [[0usize; 2]; 6];
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
        let weapon = !strings.iter().any(|x| {
            [
                profile::item_line::CRAFTED_BY,
                profile::item_line::SHIELD_RATING,
                profile::item_line::ARMOR_RATING,
            ]
            .contains(x)
        });
        let (Some(_), Some(level), true) = (
            damage_in(bytes, 20),
            stat_in(bytes, 20, profile::line::MINIMUM_LEVEL),
            weapon,
        ) else {
            continue;
        };
        if !seen.insert(req.object) {
            continue;
        }
        let band = match level {
            ..=9 => 0,
            10..=19 => 1,
            20..=29 => 2,
            30..=39 => 3,
            40..=49 => 4,
            _ => 5,
        };
        bands[band][0] += 1;
        bands[band][1] += usize::from(strings.contains(&profile::item_line::EFFECT_NAME));
    }
    println!("weapons with an enchantment by band: {bands:?}");
    for (band, [all, some]) in bands.iter().enumerate() {
        assert!(*all > 50, "{bands:?}");
        let c = WEAPON_COUNTS[band];
        #[allow(clippy::cast_precision_loss)]
        let theirs = *some as f32 / *all as f32;
        let ours = (c[1] + c[2] + c[3]) / (c[0] + c[1] + c[2] + c[3]);
        // Items with no value of their own count here and not in the counts, so a tenth either way.
        assert!((theirs - ours).abs() < 0.1, "{band}: {theirs} {ours}");
    }
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
