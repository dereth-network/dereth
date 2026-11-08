//! A fixed, seeded set of treasure drops of the install's own templates, and the digest of what it
//! drew: what the treasure numbers move, it moves. (Feature `retail-dats`: it reads the install.)

use yalain_world::treasure::TreasureTables;

use crate::authored::Portal;

/// The digest of the seeded drops below, as they were first recorded: any change to what a drop draws
/// moves it.
pub const TREASURE_DIGEST: u64 = 0x9055_fa8c_ab2c_4b6e;

/// The dropper levels the drops are made at: each band edge of the draws and a few between.
const TREASURE_LEVELS: [i32; 20] = [
    1, 4, 8, 9, 12, 16, 20, 27, 30, 35, 40, 44, 45, 49, 50, 51, 52, 56, 60, 65,
];

/// `of` of these templates that describe and are none of the classes left out, evenly spread over
/// them, in order: at each even step, the first from there that is so.
fn spread_templates(
    p: &Portal,
    mut ids: Vec<u32>,
    of: usize,
    left_out: &dyn Fn(u32) -> bool,
) -> Vec<yalain_content::Template> {
    ids.sort_unstable();
    let mut out: Vec<yalain_content::Template> = Vec::new();
    for i in 0..of {
        let from = i * ids.len() / of;
        let next = ids[from..]
            .iter()
            .filter(|t| out.iter().all(|o| o.id != **t))
            .find_map(|&t| p.described(t).filter(|x| !left_out(x.description.package)));
        out.extend(next);
    }
    out
}

/// The 64-bit FNV-1a hash of some text.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xCBF2_9CE4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3)
    })
}

/// What each potion effect gives its potion: its name table and its color's strength.
fn treasure_potion_looks(p: &Portal) -> std::collections::HashMap<u32, (u32, f32)> {
    use auberean_assets::script::ScriptState;
    use auberean_script::graph::Graph;
    let lib = p.library();
    TreasureTables::retail_2017()
        .potion_states()
        .filter_map(|state| {
            let st: ScriptState =
                auberean_assets::decode(state, &p.dat.read(state).ok()?, &p.ctx).ok()?;
            let g = Graph::decode(&st.graph, &lib).ok()?;
            let (_, table) = yalain_world::effect_defs::EffectDef::from_state(state, &lib, &g)
                .ok()?
                .name?;
            Some((
                state,
                (table, yalain_world::treasure::potion_color(&lib, &g)?),
            ))
        })
        .collect()
}

/// What a seeded set of drops drew: its text, one line a drop, and its digest.
#[derive(Debug, Clone)]
pub struct Drops {
    pub digest: u64,
    pub drops: usize,
    pub templates: usize,
    /// How many drew numbers, enchantments, an Arcane Lore, a look and a potion's effect.
    pub drew: [usize; 5],
    pub text: String,
}

impl Drops {
    /// Panics unless the drops digest to [`TREASURE_DIGEST`], with their text in a file of the
    /// temporary folder to compare.
    ///
    /// # Panics
    /// The digest moved.
    pub fn hold_recorded(&self) {
        eprintln!(
            "{} drops of {} templates: numbers {}, enchanted {}, lore {}, looks {}, potion effects {}",
            self.drops,
            self.templates,
            self.drew[0],
            self.drew[1],
            self.drew[2],
            self.drew[3],
            self.drew[4]
        );
        if self.digest != TREASURE_DIGEST {
            let out = std::env::temp_dir().join("yalain-treasure-drops.txt");
            let _ = std::fs::write(&out, &self.text);
            panic!(
                "{} drops of {} templates digest to {:#018x}, not {TREASURE_DIGEST:#018x}; their text is in {}",
                self.drops,
                self.templates,
                self.digest,
                out.display()
            );
        }
    }
}

/// A fixed, seeded set of drops, drawn by these treasure tables: weapons, clothing and armor,
/// shields, jewelry and potions of the install, and templates that draw a look, each at twenty
/// dropper levels with and without a loot quality, with the install's effects, potion looks and
/// Arcane Lore table.
///
/// # Panics
/// The install does not read.
#[must_use]
pub fn seeded_drops(p: &Portal, tables: TreasureTables) -> Drops {
    let classes: [(&str, usize); 5] = [
        ("WeaponTemplate", 12),
        ("ClothingTemplate", 12),
        ("ShieldTemplate", 5),
        ("JewelryTemplate", 5),
        ("PotionMagic", 4),
    ];
    let lib = p.library();
    let derives = |package: u32, names: &[&str]| {
        names
            .iter()
            .filter_map(|n| lib.package_id(n))
            .any(|b| lib.derives_from(package, b))
    };
    let mut templates: Vec<yalain_content::Template> = Vec::new();
    let looks = yalain_content::treasure_looks::TREASURE_LOOKS
        .iter()
        .map(|(t, ..)| *t)
        .collect();
    let lists = classes
        .iter()
        .map(|(class, of)| (p.templates_of_class(class), *of))
        .chain([(looks, 12)]);
    for (ids, of) in lists {
        let ids = ids
            .into_iter()
            .filter(|t| templates.iter().all(|x| x.id != *t))
            .collect();
        templates.extend(spread_templates(p, ids, of, &|pkg| {
            derives(pkg, &yalain_world::treasure::INSTRUMENTS)
        }));
    }
    let mut kinds = yalain_world::treasure::Kinds {
        potion_looks: treasure_potion_looks(p),
        lore: p.lore_table(),
        ..yalain_world::treasure::Kinds::default()
    };
    let mut h = crate::Harness::new();
    h.server.treasure_tables = tables;
    let ids: Vec<u32> = templates.iter().map(|t| t.id).collect();
    for template in templates {
        let package = template.description.package;
        if derives(package, &yalain_world::treasure::EQUIPMENT) {
            kinds.equipment.insert(package);
        }
        if derives(package, &yalain_world::treasure::SHIELDS) {
            kinds.shields.insert(package);
        }
        if derives(package, &yalain_world::treasure::JEWELRY) {
            kinds.jewelry.insert(package);
        }
        if derives(package, &yalain_world::treasure::POTIONS) {
            kinds.potions.insert(package);
        }
        h.server.catalog.insert(template);
    }
    let states: Vec<u32> = h
        .server
        .treasure_tables
        .states()
        .chain(h.server.treasure_tables.potion_states())
        .collect();
    h.server.effect_table = p.effects(&states);
    h.server.treasure = kinds;
    h.server.rng = yalain_world::rng::Rng::new(0x7EA5_0E1D);
    let mut text = String::new();
    let mut drops = 0;
    let mut drew = [0usize; 5];
    for &t in &ids {
        for level in TREASURE_LEVELS {
            for quality in [None, Some(i64::from(level + 3) * 10)] {
                let value = level.unsigned_abs() * 7;
                let item =
                    yalain_world::inventory::make_item(&mut h.server, t, 1, value).expect("made");
                yalain_world::treasure::endow(&mut h.server, item, level, quality);
                let thing = h.server.things.get(item).expect("made");
                text.push_str(&format!(
                    "{t:08x} {level} {quality:?} {:?} {:?} {:?}\n",
                    thing.own, thing.enchantments, thing.bonuses
                ));
                drops += 1;
                for (n, has) in drew.iter_mut().zip([
                    thing.own.made.is_some(),
                    !thing.enchantments.is_empty() || !thing.bonuses.is_empty(),
                    thing.own.arcane_lore.is_some(),
                    thing.own.look.is_some(),
                    thing.own.effect.is_some(),
                ]) {
                    *n += usize::from(has);
                }
            }
        }
    }
    Drops {
        digest: fnv1a(&text),
        drops,
        templates: ids.len(),
        drew,
        text,
    }
}
