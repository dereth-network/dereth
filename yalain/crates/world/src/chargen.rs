//! How a new character looks: its species' body, scaled by its build and height, with the
//! appearance its physique choices select.
//!
//! The client sends its choices as a table of physique values, one per physique type. The
//! server answers with the character's look, as the January 2017 servers did:
//!
//! - the body's look as the parent;
//! - a scale of (build × height, build × height, height), from physique types 1 and 2;
//! - one global change keyed `0x40000002` holding, per appearance the race and sex map a
//!   physique type to, that type's modifier at the chosen value; a modifier keyed 2 (a choice
//!   among parts) also sets modifier 16 to 1.
//!
//! The race and sex tables are the character-generation data of the September 2013 data files:
//! which appearance each physique type changes is read from its physique table
//! ([`physique_of_matrix`]), and each body's look from its body entity, by the host.

use std::collections::BTreeMap;

use auberean_primitives::Vec3;
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::types::{PartGroupDesc, VisualDesc};
use auberean_script::graph::Graph;
use auberean_script::library::Library;

pub use auberean_world::chargen::{BUILD, FEMALE, HEIGHT, MALE};
/// The key of the change applied to a whole avatar.
pub const AVATAR_KEY: u32 = 0x4000_0002;
/// The modifier of a choice among parts, which also turns on [`SHOWN`].
pub const CHOICE: u32 = 0x2;
/// The modifier set to 1 beside every choice.
const SHOWN: u32 = 0x10;

/// The body entity of each race and sex: `(race, sex, entity)`.
pub const BODY_ENTITIES: [(u32, u32, u32); 10] = [
    (1, MALE, 0x4700_00CD),
    (1, FEMALE, 0x4700_00CE),
    (2, MALE, 0x4700_00CF),
    (2, FEMALE, 0x4700_00D1),
    (4, MALE, 0x4700_00D0),
    (4, FEMALE, 0x4700_00D2),
    (8, MALE, 0x4700_0514),
    (8, FEMALE, 0x4700_0514),
    (16, MALE, 0x4700_0512),
    (16, FEMALE, 0x4700_0513),
];

/// One physique type's effect on the look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysiqueEffect {
    pub physique_type: u32,
    pub modifier: u32,
    pub appearance: u32,
}

/// Which appearance each physique type changes, per race and sex, from the character generation
/// data's graph: its physique table lists each race's, sex's and physique type's choices, each
/// naming the appearance it changes and the modifier it sets (the same for all of one type); a
/// type that names no appearance (build and height) is left out. Each list is in the order of
/// the physique types.
///
/// # Errors
/// The table, or a field it should hold, is missing or of another shape.
pub fn physique_of_matrix(
    lib: &Library,
    g: &Graph,
) -> Result<BTreeMap<(u32, u32), Vec<PhysiqueEffect>>, String> {
    use crate::start::Nav;
    let nav = Nav::new(lib, g);
    let root = nav.object(g.root().ok_or("an empty graph")?)?;
    let mut out: BTreeMap<(u32, u32), Vec<PhysiqueEffect>> = BTreeMap::new();
    for (race, by_sex) in nav.entries(nav.field(root, "m_PhysiqueTypeModifierTable")?)? {
        let race = Nav::word(race)?;
        for (sex, by_type) in nav.entries(Nav::reference(by_sex)?)? {
            let sex = Nav::word(sex)?;
            let mut effects = Vec::new();
            for (physique_type, choices) in nav.entries(Nav::reference(by_type)?)? {
                let physique_type = Nav::word(physique_type)?;
                for c in nav.items(Nav::reference(choices)?)? {
                    let c = nav.object(Nav::reference(c)?)?;
                    let appearance = nav.field(c, "appFileDID")?;
                    if appearance != 0 {
                        effects.push(PhysiqueEffect {
                            physique_type,
                            modifier: nav.field(c, "appKey")?,
                            appearance,
                        });
                        break;
                    }
                }
            }
            effects.sort_by_key(|e| e.physique_type);
            if !effects.is_empty() {
                out.insert((race, sex), effects);
            }
        }
    }
    Ok(out)
}

/// The rules for making a character's look.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharGenRules {
    /// Per (race, sex): the look the body starts from.
    pub bodies: BTreeMap<(u32, u32), u32>,
    /// Per (race, sex): what each physique type changes.
    pub physique: BTreeMap<(u32, u32), Vec<PhysiqueEffect>>,
}

impl CharGenRules {
    /// The rules with these bodies' looks (as read from the body entities, [`BODY_ENTITIES`]) and
    /// what each physique type changes (as read from the character generation data,
    /// [`physique_of_matrix`]).
    #[must_use]
    pub fn new(
        bodies: BTreeMap<(u32, u32), u32>,
        physique: BTreeMap<(u32, u32), Vec<PhysiqueEffect>>,
    ) -> Self {
        Self { bodies, physique }
    }

    /// Does the race and sex exist?
    #[must_use]
    pub fn knows(&self, race: u32, sex: u32) -> bool {
        self.bodies.contains_key(&(race, sex))
    }

    /// The look of a new character of this race and sex with these physique values; `None` for
    /// a race and sex there is no body for.
    #[must_use]
    pub fn look(&self, race: u32, sex: u32, physique: &Table<f32>) -> Option<VisualDesc> {
        self.look_wearing(race, sex, physique, &[])
    }

    /// The look of a new character wearing the clothes it starts in: each garment's appearance,
    /// given as the physique type that colors it and the appearance it gives this body, stands in
    /// for the one that physique type names.
    #[must_use]
    pub fn look_wearing(
        &self,
        race: u32,
        sex: u32,
        physique: &Table<f32>,
        garments: &[(u32, u32)],
    ) -> Option<VisualDesc> {
        let body = *self.bodies.get(&(race, sex))?;
        let value = |t: u32| physique.get(t).copied();
        let build = value(BUILD).unwrap_or(1.0);
        let height = value(HEIGHT).unwrap_or(1.0);
        let wide = build * height;
        // Per appearance, its modifiers, in the order the physique types are listed.
        let mut by_appearance: BTreeMap<u32, Vec<(u32, f32)>> = BTreeMap::new();
        for e in self
            .physique
            .get(&(race, sex))
            .map_or(&[][..], Vec::as_slice)
        {
            let Some(v) = value(e.physique_type) else {
                continue;
            };
            let appearance = garments
                .iter()
                .find(|(color, _)| *color == e.physique_type && e.modifier == CHOICE)
                .map_or(e.appearance, |(_, worn)| *worn);
            let mods = by_appearance.entry(appearance).or_default();
            mods.push((e.modifier, v));
            if e.modifier == CHOICE {
                mods.push((SHOWN, 1.0));
            }
        }
        let appearance = Table::from_inserted(
            SMALL,
            by_appearance.into_iter().map(|(app, mut mods)| {
                mods.sort_by_key(|(k, _)| *k);
                (app, Table::from_inserted(SMALL, mods))
            }),
        );
        let global = PartGroupDesc {
            bits: PartGroupDesc::KEY | PartGroupDesc::APPEARANCE,
            key: AVATAR_KEY,
            appearance,
            ..PartGroupDesc::default()
        };
        Some(VisualDesc {
            bits: VisualDesc::PARENT | VisualDesc::SCALE | VisualDesc::GLOBAL,
            parent: body,
            scale: Vec3::new(wide, wide, height),
            global,
            ..VisualDesc::default()
        })
    }
}
