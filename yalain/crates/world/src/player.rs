//! The player's object and qualities, as the world server sends them when a character enters.
//!
//! Every player is an object of one template (the player entity, its script package and its
//! qualities record), drawn with the character's own look. The world names it with its name and
//! a mark the client's text grammar reads: `[M]` or `[F]` for its sex, with `v` added when the
//! name begins with a vowel.

use auberean_primitives::{Frame, Quat, Vec3};
use auberean_protocol::login::{PlayerObject, PlayerQualities};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::text::StringInfo;
use auberean_protocol::types::{
    PhysicsDesc, Placement, Qualities, SliderSetting, VisualDesc, WeenieDesc,
};
use auberean_protocol::ServerMessage;
use yalain_store::StoredPosition;

/// The player template: its entity, its qualities record and its script package.
pub const PLAYER_ENTITY: u32 = 0x4700_0530;
pub const PLAYER_QUALITIES: u32 = 0x8100_0530;
pub const PLAYER_PACKAGE: u32 = 0x37F;

/// The admin template: the same three for an admin character. A client gives a player of this
/// template the admin's commands.
pub const ADMIN_ENTITY: u32 = 0x4700_0531;
pub const ADMIN_QUALITIES: u32 = 0x8100_0531;
pub const ADMIN_PACKAGE: u32 = 0x298;

/// The player template's physics type, movement ethereality and placement ethereality (low
/// words; the high words are 0), from its qualities record.
pub const PLAYER_NUMBERS: [i32; 6] = [0x0480_0020, 0, 0x0620_0010, 0, 0x03E0_0090, 0];

/// The world's region number, the one every 2017 world server sent.
pub const REGION: u32 = 1;

/// The physics mode every object is created in (standing).
pub const MODE_STANDING: u32 = 0x4000_0001;

/// The slider every object is created with, at 1.
pub const SLIDER: u32 = 0x4000_000A;

/// The visual-ordering stamp the player's object is created with at an entry; its visual events
/// count on from there.
pub const PLAYER_VISUAL_STAMP: u16 = 8;

/// Qualities of a player (the property list's numbers).
pub mod quality {
    /// Current health and vigor, and their greatest values (the property list's
    /// `Health_CurrentLevel`, `Vigor_CurrentLevel`, `Health_CachedMax`, `Vigor_CachedMax`).
    pub const HEALTH: u32 = 258;
    pub const VIGOR: u32 = 259;
    pub const HEALTH_MAX: u32 = 263;
    pub const VIGOR_MAX: u32 = 264;
    /// Current focus and its greatest value (`Focus_CurrentLevel`, `Focus_Max`).
    pub const FOCUS: u32 = 266;
    pub const FOCUS_MAX: u32 = 267;
    pub const RACE: u32 = 290;
    pub const SEX: u32 = 291;
    pub const SPECIES: u32 = 292;
    pub const KIND: u32 = 280;
    pub const UNSET: u32 = 950;
    pub const HEALTH_RATE: u32 = 262;
    pub const VIGOR_RATE: u32 = 263;
    pub const BODY: u32 = 1;
}

/// The species word the world gives each race (race 1 human, 2 Lugian, 4 Tumerok, 8 Drudge, 16
/// Empyrean), as every 2017 player description pairs them.
#[must_use]
pub const fn species(race: u32) -> u32 {
    match race {
        1 => 2,
        2 => 1,
        4 => 4,
        8 => 16,
        16 => 8,
        other => other,
    }
}

/// The name the world shows: the character's name and its grammar mark.
#[must_use]
pub fn world_name(name: &str, sex: u32) -> String {
    let s = if sex == crate::chargen::FEMALE {
        "F"
    } else {
        "M"
    };
    let vowel = name
        .chars()
        .next()
        .is_some_and(|c| "AEIOUaeiou".contains(c));
    format!("{name} [{s}{}]", if vowel { "v" } else { "" })
}

/// The quality a player's name is, as an object's name is.
pub const NAME_QUALITY: u32 = 256;

/// A character's name changed in the world, for the host's services that know it by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub character: u64,
    pub old: String,
    pub new: String,
}

/// Make a player's encoded qualities name it.
pub fn set_name(qualities: &mut Vec<u8>, name: &str, sex: u32) {
    let mut q = crate::quals::read(qualities);
    q.weenie.name = StringInfo::literal(&world_name(name, sex));
    *qualities = crate::quals::write(&q);
}

/// A player's name changed: its qualities hold the new one, and its client and every client that
/// knows it are told, as they are of a pet's new name.
pub fn show_name<S: yalain_store::Store>(
    s: &mut crate::Server<S>,
    net: &mut yalain_net::ServerNet,
    world: yalain_net::ConnId,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let name = p.shown_name();
    let shown = StringInfo::literal(&world_name(&name, p.sex));
    set_name(&mut p.qualities, &name, p.sex);
    let m = ServerMessage::UpdateStringInfoVisual(VisualQuality {
        object: crate::places::player_event(p, ()).sender,
        quality: NAME_QUALITY,
        value: shown,
    });
    let me = p.character;
    crate::progress::private(net, world, &m);
    crate::places::to_knowers(s, net, me, &m);
}

/// Make a player's encoded qualities name the admin template's record, or the player's.
pub fn set_template(qualities: &mut Vec<u8>, admin: bool) {
    let mut q = crate::quals::read(qualities);
    let did = if admin {
        ADMIN_QUALITIES
    } else {
        PLAYER_QUALITIES
    };
    if q.did != did {
        q.did = did;
        *qualities = crate::quals::write(&q);
    }
}

/// The weenie numbers' mask bits, in [`WeenieDesc::NUMBER_BITS`] order from the physics type's
/// low word: six words.
pub(crate) const NUMBERS_FROM: usize = 5;

/// A weenie description with the six physics words set (the mask bits and the words).
#[must_use]
pub fn weenie_with_numbers(mut w: WeenieDesc, numbers: [i32; 6]) -> WeenieDesc {
    for (i, n) in numbers.into_iter().enumerate() {
        w.mask |= WeenieDesc::NUMBER_BITS[NUMBERS_FROM + i];
        w.numbers[NUMBERS_FROM + i] = n;
    }
    w
}

/// A new character's qualities, as the 2017 servers described the one character created in the
/// recordings: the player template's kind, its race, sex and species, its body, full health and
/// vigor of 100, both rates 1.
#[must_use]
pub fn new_qualities(name: &str, race: u32, sex: u32, body: u32) -> Qualities {
    let weenie = weenie_with_numbers(
        WeenieDesc {
            mask: WeenieDesc::NAME,
            name: StringInfo::literal(&world_name(name, sex)),
            ..WeenieDesc::default()
        },
        PLAYER_NUMBERS,
    );
    #[allow(clippy::cast_possible_wrap)]
    let ints = Table::from_inserted(
        3,
        [
            (quality::KIND, 78),
            (quality::RACE, race as i32),
            (quality::SEX, sex as i32),
            (quality::SPECIES, species(race) as i32),
            (quality::UNSET, -1),
            (quality::HEALTH, 100),
            (quality::HEALTH_MAX, 100),
            (quality::VIGOR, 100),
            (quality::VIGOR_MAX, 100),
        ],
    );
    Qualities {
        header: Qualities::WEENIE | Qualities::INTS | Qualities::FLOATS | Qualities::DATA_IDS,
        did: PLAYER_QUALITIES,
        weenie,
        ints,
        floats: Table::from_inserted(
            SMALL,
            [(quality::HEALTH_RATE, 1.0), (quality::VIGOR_RATE, 1.0)],
        ),
        data_ids: Table::from_inserted(SMALL, [(quality::BODY, body)]),
        ..Qualities::default()
    }
}

/// A stored position as the wire places it.
#[must_use]
pub fn placement(p: &StoredPosition) -> Placement {
    Placement {
        cell: p.cell,
        frame: Frame {
            origin: Vec3::new(p.origin[0], p.origin[1], p.origin[2]),
            rotation: Quat {
                w: p.rotation[0],
                x: p.rotation[1],
                y: p.rotation[2],
                z: p.rotation[3],
            },
        },
    }
}

/// The physics an object is created with: standing, the slider at 1, at its place, its
/// position stamp 1; with the effect it enters with, if any.
#[must_use]
pub fn physics_at(
    at: Placement,
    instance_stamp: u16,
    visual_stamp: u16,
    enter_fx: u32,
) -> PhysicsDesc {
    let mut p = PhysicsDesc {
        bits: PhysicsDesc::SLIDERS | PhysicsDesc::MODE | PhysicsDesc::POSITION,
        instance_stamp,
        visual_ordering_stamp: visual_stamp,
        mode: MODE_STANDING,
        sliders: vec![SliderSetting {
            slider: SLIDER,
            value: 1.0,
            time: 0.0,
        }],
        position: at,
        stamps: [1, 0, 0, 0],
        ..PhysicsDesc::default()
    };
    if enter_fx != 0 {
        p.bits |= PhysicsDesc::FX;
        p.fx = vec![auberean_protocol::types::RunningFx {
            fx: enter_fx,
            scalar: 1.0,
            target: 0,
        }];
    }
    p
}

/// What a player's object shows of its qualities: its kingdom (none for a character that never
/// joined one), its six physics words, and whether it is of the admin template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marks {
    pub kingdom: Option<u32>,
    pub numbers: [i32; 6],
    pub admin: bool,
}

impl Default for Marks {
    fn default() -> Self {
        Self {
            kingdom: None,
            numbers: PLAYER_NUMBERS,
            admin: false,
        }
    }
}

impl Marks {
    /// A player's marks, as its qualities keep them.
    #[must_use]
    pub fn of(q: &Qualities) -> Self {
        let mut numbers = PLAYER_NUMBERS;
        for (i, n) in numbers.iter_mut().enumerate() {
            if q.weenie.mask & WeenieDesc::NUMBER_BITS[NUMBERS_FROM + i] != 0 {
                *n = q.weenie.numbers[NUMBERS_FROM + i];
            }
        }
        Self {
            kingdom: crate::kingdoms::of_qualities(q),
            numbers,
            admin: q.did == ADMIN_QUALITIES,
        }
    }
}

/// The player's object.
#[must_use]
pub fn player_object(
    id: u64,
    name: &str,
    sex: u32,
    look: VisualDesc,
    at: Placement,
    instance_stamp: u16,
    marks: &Marks,
) -> CreateObject {
    let mut weenie = weenie_with_numbers(
        WeenieDesc {
            mask: WeenieDesc::PACKAGE | WeenieDesc::NAME | WeenieDesc::ENTITY,
            package_id: if marks.admin {
                ADMIN_PACKAGE
            } else {
                PLAYER_PACKAGE
            },
            entity_did: if marks.admin {
                ADMIN_ENTITY
            } else {
                PLAYER_ENTITY
            },
            name: StringInfo::literal(&world_name(name, sex)),
            ..WeenieDesc::default()
        },
        marks.numbers,
    );
    if let Some(k) = marks.kingdom {
        weenie.mask |= WeenieDesc::NUMBER_BITS[2];
        weenie.numbers[2] = k.cast_signed();
    }
    CreateObject {
        id,
        visual: look,
        physics: physics_at(at, instance_stamp, PLAYER_VISUAL_STAMP, 0),
        weenie,
    }
}

/// The message that makes the client's player of an object.
#[must_use]
pub const fn create_player(id: u64) -> PlayerObject {
    PlayerObject { id, region: REGION }
}

/// The player's description: its qualities.
#[must_use]
pub fn description(q: Qualities) -> PlayerQualities {
    PlayerQualities(q)
}

/// An object of the content pack as the world creates it.
#[must_use]
pub fn content_object(id: u64, o: &yalain_content::ContentObject) -> CreateObject {
    let d = &o.description;
    let mut weenie = weenie_with_numbers(
        WeenieDesc {
            mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY,
            package_id: d.package,
            entity_did: o.template,
            ..WeenieDesc::default()
        },
        d.numbers,
    );
    if let Some(s) = d.scale {
        weenie.mask |= WeenieDesc::SCALE;
        weenie.scale = s;
    }
    if let Some((string_id, table_id)) = d.name {
        weenie.mask |= WeenieDesc::NAME;
        weenie.name = StringInfo {
            string_id,
            table_id,
            ..StringInfo::default()
        };
    }
    let at = placement(&StoredPosition {
        cell: o.cell,
        origin: o.origin,
        rotation: o.rotation,
    });
    CreateObject {
        id,
        visual: VisualDesc {
            bits: VisualDesc::PARENT,
            parent: d.look,
            ..VisualDesc::default()
        },
        // Already in the world: no entry effect (the 2017 servers showed it only on a spawn).
        physics: physics_at(at, 1, 1, 0),
        weenie,
    }
}
