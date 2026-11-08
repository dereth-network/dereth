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
//! The race and sex tables are the character-generation data of the September 2013 data files
//! (the physique map below; each body's look is read from its body entity by the host).

use std::collections::BTreeMap;

use auberean_primitives::Vec3;
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::types::{PartGroupDesc, VisualDesc};

pub use auberean_world::chargen::{BUILD, FEMALE, HEIGHT, MALE};
/// The key of the change applied to a whole avatar.
pub const AVATAR_KEY: u32 = 0x4000_0002;
/// The modifier of a choice among parts, which also turns on [`SHOWN`].
const CHOICE: u32 = 0x2;
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

/// Which appearance each physique type changes: `(race, sex, physique type, modifier,
/// appearance)`.
pub const PHYSIQUE_MAP: [(u32, u32, u32, u32, u32); 67] = [
    (1, MALE, 0x4, 0x1, 0x2000_000C),
    (1, MALE, 0x10, 0x3, 0x2000_000D),
    (1, MALE, 0x20, 0xC, 0x2000_000D),
    (1, MALE, 0x40, 0xB, 0x2000_000D),
    (1, MALE, 0x100, 0x2, 0x2000_0016),
    (1, MALE, 0x200, 0x2, 0x2000_0014),
    (1, MALE, 0x400, 0x2, 0x2000_0019),
    (1, MALE, 0x800, 0x4100_001C, 0x2000_000E),
    (1, FEMALE, 0x4, 0x1, 0x2000_000F),
    (1, FEMALE, 0x10, 0x3, 0x2000_0010),
    (1, FEMALE, 0x40, 0xB, 0x2000_0010),
    (1, FEMALE, 0x100, 0x2, 0x2000_0017),
    (1, FEMALE, 0x200, 0x2, 0x2000_0013),
    (1, FEMALE, 0x400, 0x2, 0x2000_001A),
    (1, FEMALE, 0x800, 0x4100_001C, 0x2000_0011),
    (2, MALE, 0x4, 0x1, 0x2000_0000),
    (2, MALE, 0x8, 0xF, 0x2000_0002),
    (2, MALE, 0x10, 0x3, 0x2000_0001),
    (2, MALE, 0x20, 0xC, 0x2000_0001),
    (2, MALE, 0x40, 0xB, 0x2000_0001),
    (2, MALE, 0x100, 0x2, 0x2000_00DD),
    (2, MALE, 0x200, 0x2, 0x2000_00DE),
    (2, MALE, 0x400, 0x2, 0x2000_001C),
    (2, MALE, 0x800, 0x4100_001C, 0x2000_0002),
    (2, FEMALE, 0x4, 0x1, 0x2000_0003),
    (2, FEMALE, 0x8, 0xF, 0x2000_0005),
    (2, FEMALE, 0x10, 0x3, 0x2000_0004),
    (2, FEMALE, 0x40, 0xB, 0x2000_0004),
    (2, FEMALE, 0x100, 0x2, 0x2000_00DF),
    (2, FEMALE, 0x200, 0x2, 0x2000_00E0),
    (2, FEMALE, 0x400, 0x2, 0x2000_0043),
    (2, FEMALE, 0x800, 0x4100_001C, 0x2000_0005),
    (4, MALE, 0x4, 0x1, 0x2000_0006),
    (4, MALE, 0x8, 0xF, 0x2000_0008),
    (4, MALE, 0x10, 0x3, 0x2000_0007),
    (4, MALE, 0x20, 0xC, 0x2000_0007),
    (4, MALE, 0x40, 0xB, 0x2000_0007),
    (4, MALE, 0x100, 0x2, 0x2000_0038),
    (4, MALE, 0x200, 0x2, 0x2000_0039),
    (4, MALE, 0x800, 0x4100_001C, 0x2000_0008),
    (4, FEMALE, 0x4, 0x1, 0x2000_0009),
    (4, FEMALE, 0x8, 0xF, 0x2000_000B),
    (4, FEMALE, 0x10, 0x3, 0x2000_000A),
    (4, FEMALE, 0x40, 0xB, 0x2000_000A),
    (4, FEMALE, 0x80, 0x4100_0003, 0x2000_000A),
    (4, FEMALE, 0x100, 0x2, 0x2000_0038),
    (4, FEMALE, 0x200, 0x2, 0x2000_0039),
    (4, FEMALE, 0x800, 0x4100_001C, 0x2000_000B),
    (8, MALE, 0x4, 0x1, 0x2000_00ED),
    (8, FEMALE, 0x4, 0x1, 0x2000_00ED),
    (16, MALE, 0x4, 0x1, 0x2000_00E1),
    (16, MALE, 0x10, 0x3, 0x2000_00E4),
    (16, MALE, 0x20, 0xC, 0x2000_00E4),
    (16, MALE, 0x40, 0xB, 0x2000_00E4),
    (16, MALE, 0x100, 0x2, 0x2000_00E6),
    (16, MALE, 0x200, 0x2, 0x2000_00E9),
    (16, MALE, 0x400, 0x2, 0x2000_00E7),
    (16, MALE, 0x800, 0x4100_001C, 0x2000_00E1),
    (16, MALE, 0x1000, 0x4100_002E, 0x2000_00E3),
    (16, FEMALE, 0x4, 0x1, 0x2000_00E2),
    (16, FEMALE, 0x10, 0x3, 0x2000_00E5),
    (16, FEMALE, 0x40, 0xB, 0x2000_00E5),
    (16, FEMALE, 0x100, 0x2, 0x2000_00EA),
    (16, FEMALE, 0x200, 0x2, 0x2000_00EC),
    (16, FEMALE, 0x400, 0x2, 0x2000_00EB),
    (16, FEMALE, 0x800, 0x4100_001C, 0x2000_00E2),
    (16, FEMALE, 0x1000, 0x4100_002E, 0x2000_00E3),
];

/// One physique type's effect on the look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysiqueEffect {
    pub physique_type: u32,
    pub modifier: u32,
    pub appearance: u32,
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
    /// The rules with the physique map of the September 2013 data and the bodies' looks given
    /// (as read from the body entities, [`BODY_ENTITIES`]).
    #[must_use]
    pub fn with_bodies(bodies: BTreeMap<(u32, u32), u32>) -> Self {
        let mut physique: BTreeMap<(u32, u32), Vec<PhysiqueEffect>> = BTreeMap::new();
        for (race, sex, physique_type, modifier, appearance) in PHYSIQUE_MAP {
            physique
                .entry((race, sex))
                .or_default()
                .push(PhysiqueEffect {
                    physique_type,
                    modifier,
                    appearance,
                });
        }
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
            let mods = by_appearance.entry(e.appearance).or_default();
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
