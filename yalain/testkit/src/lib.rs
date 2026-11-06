//! A whole server on a virtual clock and an in-memory network, with clients that speak the wire:
//! for the server's tests and the replay of recorded logins.
//!
//! **Depends on** the server's crates (`yalain-world`, `yalain-net`, `yalain-store`,
//! `yalain-content`), the messages (`auberean-protocol`), `auberean-primitives`, the shared math
//! (`dereth-primitives`) and the shared transport (`dereth-transport`); with feature `retail-dats`,
//! the client (`auberean-client-sdk`), the install's data (`auberean-dat`, `auberean-assets`,
//! `auberean-script`) and the world database (`yalain-world-db`) for a whole session of our own
//! client (`session`, on the world of `authored`, a world database compiled against the install);
//! with feature `network`, for the Dereth Network end to end, the friends stream's client half
//! (`auberean-friends`), the first game's client (`dereth-client-sdk`) and where the first game's
//! data is found (`dereth-dat`). **Used by** nothing but tests: no server crate depends on it.
//!
//! **Must never** wait on real time: tests step the clock themselves ([`Harness::step`]) or run
//! until something holds ([`Harness::run_until`]). The clients are [`bot::Bot`]s, or any client
//! the test holds ([`Peer`]); a session's steps take whatever drives them (`session::Drive`),
//! this harness's clock or a loopback test's.

#[cfg(feature = "retail-dats")]
pub mod authored;
pub mod bot;
pub mod faults;
pub mod invariants;
pub mod play;
#[cfg(feature = "retail-dats")]
pub mod session;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use yalain_net::memory::MemoryNet;
use yalain_net::{fixed_stream, LocalTime, NetConfig, Port, ServerNet, Service};
use yalain_store::{MemoryStore, Store, StoredPosition};
use yalain_world::start::StartArea;
use yalain_world::{CharGenRules, Clock, Server, ServerConfig, StartRules};

pub use bot::{Bot, Received};

/// The virtual clock's step.
pub const STEP: f64 = 0.05;

/// The login service's even port in the harness.
pub const LOGIN_PORT: u16 = 9000;
/// The world service's even port in the harness.
pub const WORLD_PORT: u16 = 9002;

/// The address of one of the server's sockets.
#[must_use]
pub fn port_addr(p: Port) -> SocketAddr {
    let base = match p.service {
        Service::Login => LOGIN_PORT,
        Service::World => WORLD_PORT,
    };
    SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::LOCALHOST,
        base + u16::from(p.odd),
    ))
}

/// The bodies the chargen rules start from in tests: one look id per race and sex, made up.
#[must_use]
pub fn test_rules() -> CharGenRules {
    let mut bodies = std::collections::BTreeMap::new();
    for (race, sex, entity) in yalain_world::chargen::BODY_ENTITIES {
        bodies.insert((race, sex), 0x1F00_0000 | (entity & 0xFFFF));
    }
    CharGenRules::with_bodies(bodies)
}

/// The start area kind the test rules give every race.
pub const TEST_START_AREA: u32 = 0x4000_0001;

/// Where the test rules start a character: two copies of a hall, in landblocks `0x0299` and
/// `0x0298`, facing a quarter turn.
#[must_use]
pub fn test_start() -> StartRules {
    let place = |lb: u32| StoredPosition {
        cell: lb << 16 | 0x0109,
        origin: [59.06, 240.2, -44.6],
        rotation: [
            core::f32::consts::FRAC_1_SQRT_2,
            0.0,
            0.0,
            core::f32::consts::FRAC_1_SQRT_2,
        ],
    };
    let mut r = StartRules::default();
    r.items
        .insert(1, vec![0x4700_051B, 0x4700_051C, 0x4700_051D]);
    r.items
        .insert(2, vec![0x4700_0515, 0x4700_0516, 0x4700_0517]);
    r.items
        .insert(4, vec![0x4700_0521, 0x4700_0522, 0x4700_0523]);
    r.areas.insert(
        TEST_START_AREA,
        StartArea {
            kind: TEST_START_AREA,
            name: "Hall".into(),
            places: vec![place(0x0299), place(0x0298)],
        },
    );
    for (race, _, _) in yalain_world::chargen::BODY_ENTITIES {
        r.by_race.insert(race, vec![TEST_START_AREA]);
        let mut skills = vec![(835, 0), (96, 0), (176, 0), (207, 0), (1, 1), (7, 1)];
        // The basic attacks every human holds, and a heal, a vigor heal and a buff as if trained.
        skills.extend(BASIC_ATTACKS.iter().map(|k| (*k, 1)));
        skills.extend([(EASE_SUFFERING, 0), (INNER_CALM, 0), (DASH, 0)]);
        r.skills.insert(race, skills);
    }
    r
}

/// A human's basic attacks: a stave's near and far, a sword's, a bow's near and far.
pub const BASIC_ATTACKS: [u32; 21] = [
    62, 63, 64, 65, 66, 67, 122, 123, 124, 125, 126, 194, 195, 196, 197, 198, 199, 200, 201, 202,
    203,
];
/// A heal, a vigor heal and a buff a human trains early.
pub const EASE_SUFFERING: u32 = 85;
pub const INNER_CALM: u32 = 93;
pub const DASH: u32 = 206;

/// The human starting skills that are used, written out as the September 2013 data defines them
/// (their reach, waits, vigor, implements, behaviors and hooks), for the server's skill table in
/// tests without the install: Lifestone Recall, Mind Lash, Riposte and Deadeye; the basic attacks
/// of a bow, a stave and a sword; Ease Suffering, Inner Calm and Dash.
#[must_use]
pub fn test_skills() -> yalain_world::skills::SkillTable {
    use yalain_world::skills::{Hook, SkillDef, SkillTable, Usage};
    let opening = 0x7000_0725;
    let strike = Hook {
        damages: true,
        flags: 0x2002,
        add: 5.0,
        mult: 1.0,
        chance: 1.0,
        radius: 2.0,
        anim_hook: 1,
        target_behavior: [0x4100_007C, 0x4100_007F, 0x4100_007D],
        ..Hook::default()
    };
    let plays = |fx: u32| Hook {
        flags: 0x40,
        mult: 1.0,
        chance: 1.0,
        attacker_fx: [fx, 0, 0],
        ..Hook::default()
    };
    let special =
        |behavior: u32, recovery: f32, right: i32, ui_flags: u32, hooks: Vec<Hook>| Usage {
            min_range: -1.0,
            max_range: 1.0,
            recovery,
            min_use: 0.5,
            powerup_behavior: 0x4100_00BF,
            vigor_add: -32.0,
            min_vigor: 1.0,
            implements_right: right,
            implements_left: 1,
            valid_targets: 0x0030_0980,
            behavior_other: [behavior; 3],
            target_effects: vec![opening],
            hooks,
            ui_flags,
            ..Usage::default()
        };
    let deadeye = Usage {
        min_range: 2.0,
        max_range: 80.0,
        ..special(
            0x4100_007A,
            2.0,
            0x4,
            0x1480,
            vec![Hook {
                flags: 0x2804,
                missile: 0x4700_00BE,
                missile_speed: 35.0,
                radius: 0.0,
                ..strike.clone()
            }],
        )
    };
    let recall = Usage {
        recovery: 1.0,
        reset: 30.0,
        min_use: 0.5,
        powerup: 10.0,
        powerup_behavior: 0x4100_00BF,
        vigor_mult: 1.0,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 1,
        behavior_other: [0x4100_00D5; 3],
        behavior_self: [0x4100_00D5; 2],
        target_effects: vec![0x7000_0722],
        hooks: vec![
            plays(0x4100_048A),
            Hook {
                damages: true,
                flags: 0x2018,
                mult: 1.0,
                chance: 1.0,
                anim_hook: 1,
                ..Hook::default()
            },
        ],
        ui_flags: 0x72E0,
        ..Usage::default()
    };
    // The summonings are used as the recall is, at half the vigor.
    let summon = |effect: u32| Usage {
        vigor_mult: 0.5,
        target_effects: vec![effect],
        ui_flags: 0x32C0,
        ..recall.clone()
    };
    // Portal Recall, the cities' recalls (their recall and protection put on their user) and
    // Allegiance Recall are used as the lifestone's is.
    let recall_to = |effect: u32| Usage {
        target_effects: vec![effect],
        ui_flags: 0x22E0,
        ..recall.clone()
    };
    let city = |effect: u32| Usage {
        target_effects: Vec::new(),
        user_effects: vec![effect, RECALL_PROTECTION_EFFECT],
        ui_flags: 0x22E0,
        ..recall.clone()
    };
    let mut skills = std::collections::HashMap::new();
    for (id, usage) in [
        (871, summon(0x7000_0728)),
        (872, summon(0x7000_0729)),
        (870, recall_to(PORTAL_RECALL_EFFECT)),
        (925, city(CRAGSTONE_RECALL_EFFECT)),
        (1045, city(ARRAMORA_RECALL_EFFECT)),
        (
            999,
            Usage {
                target_effects: Vec::new(),
                user_effects: vec![ALLEGIANCE_RECALL_EFFECT],
                ..recall.clone()
            },
        ),
        (835, recall),
        (
            96,
            special(
                0x4100_00B9,
                5.0,
                0x4_0000,
                0xC81,
                vec![plays(0x8100_00CD), strike.clone()],
            ),
        ),
        (
            176,
            special(0x4100_0077, 1.0, 0xA_0062, 0xC81, vec![strike]),
        ),
        (207, deadeye),
    ] {
        skills.insert(
            id,
            SkillDef {
                id,
                cap: 1,
                table: 0x6F00_000B,
                flags: 0x44,
                active: true,
                usage: Some(usage),
                ..SkillDef::default()
            },
        );
    }
    let linear = |flags: u32, anim: u32| Hook {
        flags,
        mult: 0.0,
        chance: 1.0,
        radius: 2.0,
        anim_hook: anim,
        target_behavior: [0x4100_007C, 0x4100_007F, 0x4100_007D],
        ..Hook::default()
    };
    let plays = |fx: u32| Hook {
        flags: 0x40,
        chance: 1.0,
        attacker_fx: [fx, 0, 0],
        ..Hook::default()
    };
    let basic = |min: f32, max: f32, right: i32, left: i32, behavior: u32, ui: u32, hooks| Usage {
        min_range: min,
        max_range: max,
        implements_right: right,
        implements_left: left,
        valid_targets: 0x0030_0980,
        behavior_other: [behavior; 3],
        hooks,
        ui_flags: ui,
        ..Usage::default()
    };
    let arrow = Hook {
        missile: 0x4700_00BE,
        missile_speed: 35.0,
        radius: 0.0,
        ..linear(0x2804, 1)
    };
    let bolt = Hook {
        missile: 0x4700_0393,
        missile_speed: 30.0,
        ..linear(0x2004, 1)
    };
    let mut autos: Vec<(u32, Usage)> = Vec::new();
    for i in 0..3 {
        autos.push((
            62 + i,
            basic(
                -1.0,
                10.0,
                0x4_0000,
                1,
                0x4100_00B0 + i,
                0x14A0,
                vec![plays(0), linear(0x2018, 1)],
            ),
        ));
        autos.push((
            65 + i,
            basic(
                9.0,
                20.0,
                0x4_0000,
                1,
                0x4100_00B0 + i,
                0x14A0,
                vec![plays(0), bolt.clone()],
            ),
        ));
    }
    for i in 0..5 {
        autos.push((
            122 + i,
            basic(
                -1.0,
                1.0,
                0x8_0062,
                0x11,
                0x4100_0089 + i,
                0xCA1,
                vec![linear(0x2002, 1)],
            ),
        ));
        autos.push((
            194 + i,
            basic(
                -1.0,
                1.0,
                0x4,
                1,
                0x4100_0089 + i,
                0x14A1,
                vec![linear(0x2002, 1)],
            ),
        ));
        autos.push((
            199 + i,
            basic(
                1.0,
                80.0,
                0x4,
                1,
                0x4100_00E2 + i,
                0x14A0,
                vec![arrow.clone()],
            ),
        ));
    }
    for (id, usage) in autos {
        skills.insert(
            id,
            SkillDef {
                id,
                cap: 1,
                table: 0x6F00_000B,
                flags: 0x4A,
                active: true,
                auto_attack: true,
                usage: Some(usage),
                ..SkillDef::default()
            },
        );
    }
    let helpful = |behavior: u32, vigor: f32, effect: u32| Usage {
        min_range: -1.0,
        max_range: 40.0,
        recovery: 7.0,
        min_use: 0.5,
        powerup_behavior: 0x4100_00BF,
        vigor_add: -vigor,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 0x101,
        behavior_other: [behavior; 3],
        behavior_self: [behavior; 2],
        target_effects: vec![effect],
        hooks: vec![
            plays(0x8100_01D0),
            Hook {
                flags: 0x2018,
                mult: 1.0,
                chance: 1.0,
                radius: 2.0,
                anim_hook: 1,
                ..Hook::default()
            },
        ],
        ui_flags: 0x10_12C0,
        ..Usage::default()
    };
    let ease = helpful(0x4100_00C2, 100.0, EASE_SUFFERING_EFFECT);
    let calm = Usage {
        powerup: 1.0,
        ..helpful(0x4100_00C9, 160.0, INNER_CALM_EFFECT)
    };
    let dash = Usage {
        min_range: -1.0,
        max_range: -1.0,
        recovery: 300.0,
        powerup_behavior: 0x4100_010C,
        vigor_add: -1.0,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 1,
        behavior_other: [0x4100_00ED; 3],
        behavior_self: [0x4100_00ED; 2],
        target_effects: vec![DASH_EFFECT],
        hooks: vec![Hook {
            flags: 0x2018,
            mult: 1.0,
            chance: 1.0,
            anim_hook: 1,
            ..Hook::default()
        }],
        ui_flags: 0x22C1,
        ..Usage::default()
    };
    for (id, usage) in [(EASE_SUFFERING, ease), (INNER_CALM, calm), (DASH, dash)] {
        skills.insert(
            id,
            SkillDef {
                id,
                cap: 50,
                table: 0x6F00_000B,
                flags: 0x41,
                active: true,
                usage: Some(usage),
                ..SkillDef::default()
            },
        );
    }
    for (id, state, usage) in effect_skills() {
        skills.insert(
            id,
            SkillDef {
                id,
                state,
                name: (0xA860_7CEC, 0x2500_0000 | id),
                cap: 50,
                table: 0x6F00_000B,
                flags: 0x41,
                active: true,
                usage: Some(usage),
                ..SkillDef::default()
            },
        );
    }
    for (id, effect) in [
        (CREATURE_HEAL, CREATURE_HEAL_EFFECT),
        (CREATURE_ABSORB, CREATURE_ABSORB_EFFECT),
    ] {
        skills.insert(
            id,
            SkillDef {
                id,
                helps: true,
                usage: Some(creature_help(effect)),
                ..SkillDef::default()
            },
        );
    }
    let mut tables = std::collections::HashMap::new();
    tables.insert(0x6F00_000B, vec![0, 0]);
    SkillTable { skills, tables }
}

/// A creature's heal and its damage-absorbing buff, each used on itself or a friend.
pub const CREATURE_HEAL: u32 = 1818;
pub const CREATURE_ABSORB: u32 = 1815;
/// Their effects: the heal (2 at a rating of 1, 300 at 50) and the buff (ten seconds, its own
/// appearance); a player's debuff that takes from a creature's hits (5 at a rating of 1, 40 at 50,
/// thirty seconds).
pub const CREATURE_HEAL_EFFECT: u32 = 0x6F00_018C;
pub const CREATURE_ABSORB_EFFECT: u32 = 0x6F00_0186;
pub const BLOOD_LOATHER_EFFECT: u32 = 0x7000_0785;

/// A creature's heal or buff as the September 2013 data defines it: used on itself or a friend
/// within 30 m, its behavior on itself and on another, one hook, the effect it puts on.
fn creature_help(effect: u32) -> yalain_world::skills::Usage {
    use yalain_world::skills::{Hook, Usage};
    Usage {
        min_range: -1.0,
        max_range: 30.0,
        min_vigor: -1.0,
        implements_right: -1,
        implements_left: -1,
        valid_targets: 0x0030_0181,
        behavior_other: [0x4100_0075; 3],
        behavior_self: [0x4100_0074; 2],
        target_effects: vec![effect],
        hooks: vec![Hook {
            damages: true,
            flags: 0x2008,
            mult: 1.0,
            chance: 1.0,
            anim_hook: 1,
            ..Hook::default()
        }],
        ui_flags: 0x10A7,
        ..Usage::default()
    }
}

/// Skills that put effects on creatures, reach an area, combine, and toggle an aura.
pub const VACUUM: u32 = 121;
pub const PARALYZE: u32 = 138;
pub const CHARGED_AIR: u32 = 667;
pub const EYE_OF_THE_STORM: u32 = 671;
pub const SINEW_SNAP: u32 = 994;
pub const LILITHAS_VENGEANCE: u32 = 993;
pub const ESPIRIT: u32 = 670;

/// Vacuum, Paralyze, Charged Air, the Eye of the Storm, Sinew Snap, Lilitha's Vengeance and
/// Espirit as the September 2013 data defines them (their saved states, reach, waits, vigor,
/// behaviors, hooks and effects), but for Charged Air's and the Eye of the Storm's implement (any
/// here) and the effects of theirs the tests do not follow (left out).
#[must_use]
pub fn effect_skills() -> Vec<(u32, u32, yalain_world::skills::Usage)> {
    use yalain_world::skills::{Hook, Usage};
    let plays = |fx: u32| Hook {
        flags: 0x40,
        chance: 1.0,
        attacker_fx: [fx, 0, 0],
        ..Hook::default()
    };
    let area = |radius: f32, hit_fx: u32| Hook {
        flags: 0x2019,
        chance: 1.0,
        radius,
        anim_hook: 1,
        target_behavior: [0x4100_007C, 0x4100_007F, 0x4100_007D],
        target_fx: [hit_fx, 0x4100_04A9, hit_fx],
        ..Hook::default()
    };
    let arrow = |flags: u32, missile: u32| Hook {
        flags,
        chance: 1.0,
        radius: if flags & 0x800 != 0 { 0.0 } else { 2.0 },
        anim_hook: 1,
        missile,
        missile_speed: 35.0,
        target_behavior: [0x4100_007C, 0x4100_007F, 0x4100_007D],
        target_fx: [0x8100_025A, 0, 0x8100_025A],
        ..Hook::default()
    };
    let hostile = |max: f32, recovery: f32, vigor: f32, right: i32, behavior: u32, ui: u32| Usage {
        min_range: -1.0,
        max_range: max,
        recovery,
        powerup_behavior: 0x4100_00BF,
        vigor_add: -vigor,
        min_vigor: 1.0,
        implements_right: right,
        implements_left: 1,
        valid_targets: 0x0030_0980,
        behavior_other: [behavior; 3],
        behavior_self: [behavior; 2],
        ui_flags: ui,
        ..Usage::default()
    };
    let vacuum = Usage {
        powerup: 1.0,
        target_effects: vec![VACUUM_EFFECT],
        hooks: vec![plays(0x8100_01CF), area(10.0, 0x4100_0081)],
        ..hostile(40.0, 5.0, 250.0, 0x4_0000, 0x4100_00D2, 0x1480)
    };
    let paralyze = Usage {
        max_range: 1.0,
        powerup_behavior: 0x4100_0109,
        implements_left: 0x8_0073,
        behavior_self: [0; 2],
        target_effects: vec![PARALYZE_EFFECT],
        hooks: vec![Hook {
            flags: 0x2002,
            chance: 1.0,
            radius: 2.0,
            anim_hook: 1,
            target_behavior: [0x4100_007C, 0x4100_007F, 0x4100_007D],
            target_fx: [0x8100_025A, 0, 0x8100_025A],
            ..Hook::default()
        }],
        ..hostile(1.0, 5.0, 200.0, 0x8_0062, 0x4100_009E, 0xC81)
    };
    let charged_air = Usage {
        target_effects: vec![CHARGED_AIR_EFFECT],
        hooks: vec![plays(0x4100_0487), area(10.0, 0x4100_019C)],
        ..hostile(40.0, 7.0, 150.0, -1, 0x4100_00DE, 0x1480)
    };
    let eye = Usage {
        target_effects: vec![EYE_OF_THE_STORM_EFFECT],
        hooks: vec![plays(0x4100_0487), area(10.0, 0)],
        ..hostile(10.0, 7.0, 80.0, -1, 0x4100_00DE, 0x2480)
    };
    let sinew = Usage {
        max_range: 70.0,
        powerup_behavior: 0x4100_010C,
        behavior_self: [0; 2],
        target_effects: vec![SINEW_SNAP_LINK],
        hooks: vec![arrow(0x2004, 0x4700_00BE)],
        ..hostile(70.0, 4.0, 200.0, 0x4, 0x4100_007B, 0x1481)
    };
    let lilitha = Usage {
        min_range: 2.0,
        powerup_behavior: 0x4100_010C,
        behavior_self: [0; 2],
        target_effects: vec![LILITHAS_COMBO],
        hooks: vec![arrow(0x2804, 0x4700_04C9)],
        ..hostile(70.0, 5.0, 230.0, 0x4, 0x4100_00E8, 0x1481)
    };
    let espirit = Usage {
        min_range: -1.0,
        max_range: 10.0,
        recovery: 5.0,
        powerup: 1.0,
        powerup_behavior: 0x4100_00BF,
        vigor_add: -1.0,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 1,
        behavior_other: [0x4100_00C9; 3],
        behavior_self: [0x4100_00C6; 2],
        user_effects: vec![ESPIRIT_AURA],
        hooks: vec![
            plays(0x4100_0494),
            Hook {
                damages: true,
                flags: 0x2008,
                mult: 1.0,
                chance: 1.0,
                anim_hook: 1,
                target_fx: [0, 0x4100_04A8, 0],
                attacker_fx: [0, 0x4100_000B, 0],
                ..Hook::default()
            },
        ],
        ui_flags: 0x32C1,
        ..Usage::default()
    };
    vec![
        (VACUUM, 0x7000_175F, vacuum),
        (PARALYZE, 0x7000_1783, paralyze),
        (CHARGED_AIR, 0x7000_19C4, charged_air),
        (EYE_OF_THE_STORM, 0x7000_19C8, eye),
        (SINEW_SNAP, 0x7000_3450, sinew),
        (LILITHAS_VENGEANCE, 0x7000_344F, lilitha),
        (ESPIRIT, 0x7000_19C7, espirit),
    ]
}

/// Effects of the September 2013 data, by saved state: Ease Suffering's and Inner Calm's heals,
/// Dash's speed, the Sapphire Obelisk's blessing, the Arwic mines' recipe given and taken and their
/// quest moved on; and a change of health over time made up for the tests.
pub const EASE_SUFFERING_EFFECT: u32 = 0x7000_0772;
pub const INNER_CALM_EFFECT: u32 = 0x7000_077A;
pub const DASH_EFFECT: u32 = 0x7000_07E4;
pub const COURAGE_EFFECT: u32 = 0x6F00_1DF7;
pub const GRANT_MINES_RECIPE: u32 = 0x6F00_19EF;
pub const REMOVE_MINES_RECIPE: u32 = 0x6F00_19F0;
pub const MINES_PHASE_4: u32 = 0x6F00_11AA;
pub const TEST_OVER_TIME: u32 = 0x6FFF_0001;
/// Robustify: for three minutes, a combat health rate (0.02 at a rating of 1, 0.08 at 50) and the
/// peace health rate multiplied (1.1 to 2).
pub const ROBUSTIFY_EFFECT: u32 = 0x6F00_10F0;
/// A crafted item's bonus while worn: a combat health rate of 0.08 (its wearer shown it, a death
/// ending it).
pub const WORN_HEALTH_RATE_EFFECT: u32 = 0x6F00_1730;
/// The recalls' effects: the lifestone's and Portal Recall's (to the places a player keeps),
/// Cragstone's and Arramora's (to named places), Allegiance Recall's, and a city recall's
/// protection.
pub const LIFESTONE_RECALL_EFFECT: u32 = 0x7000_0722;
pub const PORTAL_RECALL_EFFECT: u32 = 0x7000_0727;
pub const CRAGSTONE_RECALL_EFFECT: u32 = 0x7000_1FC0;
pub const ARRAMORA_RECALL_EFFECT: u32 = 0x6F00_0222;
pub const ALLEGIANCE_RECALL_EFFECT: u32 = 0x7000_1EC5;
pub const RECALL_PROTECTION_EFFECT: u32 = 0x7000_2014;
/// The places Cragstone's and Arramora's recalls name.
pub const CRAGSTONE_PLACE: &str = "human_recall_cragstone_01";
pub const ARRAMORA_PLACE: &str = "arramora_start";
/// A creature's opening, the starting specials' combination that exploits it and what that does.
pub const OPENING_EFFECT: u32 = 0x7000_06D7;
pub const EXPLOIT_COMBO: u32 = 0x7000_0725;
pub const EXPLOIT: u32 = 0x7000_0726;
/// Vacuum's debuff, Paralyze's hold, Charged Air's and the Eye of the Storm's damage over time,
/// Sinew Snap's link, Lilitha's Vengeance's combination and what it does, Espirit's aura and the
/// effect its pulses put on.
pub const VACUUM_EFFECT: u32 = 0x7000_07A2;
pub const PARALYZE_EFFECT: u32 = 0x7000_07C4;
pub const CHARGED_AIR_EFFECT: u32 = 0x7000_08CD;
pub const EYE_OF_THE_STORM_EFFECT: u32 = 0x7000_08D1;
pub const SINEW_SNAP_LINK: u32 = 0x7000_1F5B;
pub const LILITHAS_COMBO: u32 = 0x7000_1F59;
pub const LILITHAS_STRIKE: u32 = 0x7000_1F58;
pub const ESPIRIT_AURA: u32 = 0x6F00_022B;
pub const ESPIRIT_PULSE: u32 = 0x6F00_1147;
/// Still Water's slowing of attacks and Heart of Ice's slowing of movement.
pub const STILL_WATER_EFFECT: u32 = 0x7000_07A0;
pub const HEART_OF_ICE_EFFECT: u32 = 0x7000_0777;

/// The Arwic mines' quest, its recipe (twenty iron into a short sword), the iron and the sword; a
/// starting recipe (a knife from nothing); and two craft skills (the quest recipes', tools').
pub const MINES_QUEST: u32 = 0x4000_00C1;
pub const MINES_RECIPE: u32 = 0x6F00_02A6;
pub const IRON: u32 = 0x4700_06C1;
pub const MINES_SWORD: u32 = 0x4700_0B21;
pub const KNIFE_RECIPE: u32 = 0x6F00_1071;
pub const KNIFE: u32 = 0x4700_0CBD;
pub const QUEST_RECIPES: u32 = 0x6F00_06A0;
pub const TOOLS: u32 = 0x6F00_06A7;

/// The effects the tests put on players (see the constants above).
#[must_use]
pub fn test_effects() -> yalain_world::effect_defs::EffectTable {
    use yalain_world::effect_defs::{
        EffectDef, EffectTable, Kind, Recall, Scale, REMOVED_ON_DEATH,
    };
    let def = |state: u32, kind: Kind| EffectDef {
        state,
        kind,
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: REMOVED_ON_DEATH,
        enum_value: 0,
        eq_class: 0,
        name: None,
    };
    let mult = |stat: u32, from: f32, to: f32| EffectDef {
        ..def(
            0,
            Kind::Numeric {
                flags: 0xA,
                stats: vec![stat],
                magnitude: Scale(vec![(1.0, from), (50.0, to)]),
            },
        )
    };
    let mut t = EffectTable::default();
    for d in [
        def(
            EASE_SUFFERING_EFFECT,
            Kind::InstantVital {
                flags: 0x21,
                change: Scale(vec![(1.0, 25.0), (50.0, 250.0)]),
            },
        ),
        def(
            INNER_CALM_EFFECT,
            Kind::InstantVital {
                flags: 0x22,
                change: Scale(vec![(1.0, 200.0), (50.0, 600.0)]),
            },
        ),
        EffectDef {
            duration: Scale(vec![(1.0, 30.0), (50.0, 120.0)]),
            priority: Scale(vec![(0.0, 2.0)]),
            fx: 0x4100_04CB,
            eq_class: 19,
            ..def(
                DASH_EFFECT,
                Kind::Chained(vec![mult(0x3_0127, 1.08, 1.4), mult(0x3_0128, 1.05, 1.1)]),
            )
        },
        EffectDef {
            duration: Scale(vec![(0.0, 7200.0)]),
            priority: Scale::default(),
            eq_class: 11,
            internal_flags: 0,
            name: Some((0xA860_7CEC, 0x2500_10E8)),
            ..def(
                COURAGE_EFFECT,
                Kind::Numeric {
                    flags: 0x200A5,
                    stats: vec![0x4_0107],
                    magnitude: Scale(vec![(0.0, 30.0)]),
                },
            )
        },
        EffectDef {
            fx: 0x4100_0499,
            internal_flags: 0,
            name: Some((0xA860_7CEC, 0x2500_1070)),
            ..def(GRANT_MINES_RECIPE, Kind::GrantRecipe(MINES_RECIPE))
        },
        // The recalls: their links, places and allegiance's hall, named in their own tables.
        EffectDef {
            name: Some((0x5A43_338D, 0x2500_115A)),
            ..def(
                LIFESTONE_RECALL_EFFECT,
                Kind::Recall(Recall::Link(0x4000_0001)),
            )
        },
        EffectDef {
            name: Some((0x5A43_338D, 0x2500_115B)),
            ..def(
                PORTAL_RECALL_EFFECT,
                Kind::Recall(Recall::Link(0x4000_0004)),
            )
        },
        EffectDef {
            name: Some((0xA860_7CEC, 0x2500_1239)),
            ..def(
                CRAGSTONE_RECALL_EFFECT,
                Kind::Recall(Recall::Named {
                    names: vec![CRAGSTONE_PLACE.into()],
                    told: true,
                }),
            )
        },
        EffectDef {
            name: Some((0xA860_7CEC, 0x2500_1890)),
            ..def(
                ARRAMORA_RECALL_EFFECT,
                Kind::Recall(Recall::Named {
                    names: vec![ARRAMORA_PLACE.into()],
                    told: false,
                }),
            )
        },
        EffectDef {
            name: Some((0xA860_7CEC, 0x2500_114F)),
            ..def(ALLEGIANCE_RECALL_EFFECT, Kind::Recall(Recall::Allegiance))
        },
        EffectDef {
            duration: Scale(vec![(0.0, 30.0)]),
            fx: 0x4100_000F,
            ..def(RECALL_PROTECTION_EFFECT, Kind::Other)
        },
        EffectDef {
            internal_flags: 0,
            ..def(REMOVE_MINES_RECIPE, Kind::RemoveRecipe(MINES_RECIPE))
        },
        EffectDef {
            internal_flags: 0,
            ..def(
                MINES_PHASE_4,
                Kind::Quest {
                    quest: MINES_QUEST,
                    update: 0x2000_0004,
                },
            )
        },
        EffectDef {
            duration: Scale(vec![(1.0, 120.0)]),
            ..def(
                TEST_OVER_TIME,
                Kind::VitalOverTime {
                    flags: 0x1,
                    per_second: Scale(vec![(1.0, 3.0), (50.0, 25.0)]),
                },
            )
        },
        EffectDef {
            duration: Scale(vec![(0.0, 7.5)]),
            priority: Scale(vec![(0.0, 1.0)]),
            fx: 0x4100_0003,
            external_flags: 0x5,
            internal_flags: 0x140_0715,
            enum_value: 211,
            eq_class: 3,
            ..def(
                OPENING_EFFECT,
                Kind::Numeric {
                    flags: 0x5,
                    stats: Vec::new(),
                    magnitude: Scale::default(),
                },
            )
        },
        EffectDef {
            external_flags: 0x6,
            internal_flags: 0x4340_0513,
            ..def(
                EXPLOIT_COMBO,
                Kind::Combo(Box::new(yalain_world::effect_defs::Combo {
                    types: vec![211, 6101],
                    if_present: Some(EXPLOIT),
                    ..Default::default()
                })),
            )
        },
        EffectDef {
            external_flags: 0x21,
            internal_flags: 0x4140_0513,
            enum_value: 0x3086,
            fx: 0x4100_0033,
            ..def(
                CREATURE_HEAL_EFFECT,
                Kind::InstantVital {
                    flags: 0x21,
                    change: Scale(vec![(1.0, 2.0), (50.0, 300.0), (100.0, 1200.0)]),
                },
            )
        },
        EffectDef {
            duration: Scale(vec![(1.0, 10.0), (50.0, 10.0)]),
            external_flags: 0x6102,
            internal_flags: 0x1140_1713,
            enum_value: 0x3083,
            fx: 0x4100_0518,
            ..def(CREATURE_ABSORB_EFFECT, Kind::Other)
        },
        EffectDef {
            duration: Scale(vec![(1.0, 30.0), (50.0, 30.0)]),
            external_flags: 0x6,
            internal_flags: 0x340_0713,
            enum_value: 0x19C,
            eq_class: 0x20,
            fx: 0x4100_0254,
            ..def(
                BLOOD_LOATHER_EFFECT,
                Kind::Numeric {
                    flags: 6,
                    stats: vec![0x4_0127],
                    magnitude: Scale(vec![(1.0, -5.0), (50.0, -40.0), (100.0, -100.0)]),
                },
            )
        },
        EffectDef {
            duration: Scale(vec![(0.0, 180.0)]),
            priority: Scale(vec![(0.0, 1.0)]),
            eq_class: 5,
            ..def(
                ROBUSTIFY_EFFECT,
                Kind::Chained(vec![
                    def(
                        0,
                        Kind::Numeric {
                            flags: 0x6,
                            stats: vec![0x3_010B],
                            magnitude: Scale(vec![(1.0, 0.02), (50.0, 0.08)]),
                        },
                    ),
                    mult(0x3_0106, 1.1, 2.0),
                ]),
            )
        },
        EffectDef {
            priority: Scale(vec![(0.0, 1.0)]),
            external_flags: 0x5,
            internal_flags: 0x0140_0783,
            eq_class: 1577,
            ..def(
                WORN_HEALTH_RATE_EFFECT,
                Kind::Numeric {
                    flags: 0x5,
                    stats: vec![0x3_010B],
                    magnitude: Scale(vec![(0.0, 0.08)]),
                },
            )
        },
        EffectDef {
            external_flags: 0x9,
            internal_flags: 0x4340_0513,
            ..def(
                EXPLOIT,
                Kind::InstantVital {
                    flags: 0x9,
                    change: Scale(vec![(0.0, -20.0)]),
                },
            )
        },
    ] {
        t.effects.insert(d.state, d);
    }
    for d in creature_effects() {
        t.effects.insert(d.state, d);
    }
    t
}

/// The butchery craft skill, a gathering skill.
pub const BUTCHERY: u32 = 0x6F00_068F;

/// The butchery skill's advancement table's first steps (its data's).
fn butchery_advancement() -> Vec<u64> {
    vec![0, 0, 10, 20, 30, 40, 50, 60, 71, 82, 93, 104, 128, 152]
}

/// Butchery as the data defines it, its tables cut to their first levels: a cap of 150, a
/// hundredth to the player's pool, 120 a day before uses pay less, 2 a use.
#[must_use]
pub fn butchery_skill() -> yalain_world::craft::CraftSkillDef {
    use yalain_world::craft::{CraftSkillDef, LevelMap};
    CraftSkillDef {
        did: BUTCHERY,
        cap: 150,
        name: (0x020C_2386, 0x2500_13CA),
        advancement: butchery_advancement(),
        daily: LevelMap {
            first: 1,
            values: vec![120; 10],
        },
        usage_xp: Some(LevelMap {
            first: 1,
            values: vec![2; 10],
        }),
        pool_share: 0.01,
        cannot_raise: false,
        bonus_stat: 0,
        common_name: (0, 0),
    }
}

/// The effects of [`effect_skills`] as the September 2013 data defines them.
#[must_use]
pub fn creature_effects() -> Vec<yalain_world::effect_defs::EffectDef> {
    use yalain_world::effect_defs::{Aura, Combo, EffectDef, Kind, Scale};
    let def = |state: u32, kind: Kind, enum_value: u32, eq_class: u32| EffectDef {
        state,
        kind,
        duration: Scale::default(),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x340_0713,
        enum_value,
        eq_class,
        name: None,
    };
    let over_time = |state: u32, rate: Vec<(f32, f32)>, enum_value: u32| EffectDef {
        duration: Scale(vec![(0.0, 15.0)]),
        fx: 0x4100_0031,
        external_flags: 0x11,
        internal_flags: 0x2340_0713,
        ..def(
            state,
            Kind::VitalOverTime {
                flags: 0x11,
                per_second: Scale(rate),
            },
            enum_value,
            0x71,
        )
    };
    vec![
        EffectDef {
            duration: Scale(vec![(0.0, 30.0)]),
            fx: 0x4100_0271,
            external_flags: 0x6,
            ..def(
                VACUUM_EFFECT,
                Kind::Numeric {
                    flags: 0x6,
                    stats: vec![0x4_0125],
                    magnitude: Scale(vec![(1.0, -10.0), (50.0, -100.0), (150.0, -180.0)]),
                },
                0x1B9,
                0x16,
            )
        },
        EffectDef {
            duration: Scale(vec![(1.0, 3.0), (50.0, 10.0)]),
            fx: 0x4100_0290,
            internal_flags: 0x340_0743,
            ..def(
                PARALYZE_EFFECT,
                Kind::Chained(
                    [0x3_0127, 0x3_0129]
                        .map(|stat| {
                            def(
                                0,
                                Kind::Numeric {
                                    flags: 0x9,
                                    stats: vec![stat],
                                    magnitude: Scale(vec![(0.0, 0.0)]),
                                },
                                0,
                                0,
                            )
                        })
                        .to_vec(),
                ),
                0x1DB,
                0x37,
            )
        },
        over_time(
            CHARGED_AIR_EFFECT,
            vec![(1.0, -2.0), (50.0, -8.0), (150.0, -24.0)],
            0x2E3,
        ),
        over_time(
            EYE_OF_THE_STORM_EFFECT,
            vec![(1.0, -2.0), (50.0, -8.0)],
            0x2E7,
        ),
        EffectDef {
            duration: Scale(vec![(1.0, 5.0), (50.0, 10.0)]),
            external_flags: 0x5,
            internal_flags: 0x340_0743,
            ..def(
                SINEW_SNAP_LINK,
                Kind::Numeric {
                    flags: 0x5,
                    stats: Vec::new(),
                    magnitude: Scale::default(),
                },
                0x12D3,
                0x441,
            )
        },
        EffectDef {
            priority: Scale::default(),
            external_flags: 0x6,
            internal_flags: 0x4340_0513,
            ..def(
                LILITHAS_COMBO,
                Kind::Combo(Box::new(Combo {
                    types: vec![0x12D3],
                    if_present: Some(LILITHAS_STRIKE),
                    ..Combo::default()
                })),
                0x12D6,
                0,
            )
        },
        EffectDef {
            priority: Scale::default(),
            fx: 0x4100_02CB,
            external_flags: 0x21,
            internal_flags: 0x4340_0513,
            ..def(
                LILITHAS_STRIKE,
                Kind::InstantVital {
                    flags: 0x21,
                    change: Scale(vec![(1.0, -20.0), (50.0, -200.0), (150.0, -600.0)]),
                },
                0x12D5,
                0,
            )
        },
        EffectDef {
            internal_flags: 0x102_0783,
            ..def(
                ESPIRIT_AURA,
                Kind::Aura(Aura {
                    pulsed: vec![ESPIRIT_PULSE],
                    targets: 0x1,
                    range: 300.0,
                }),
                0x23D3,
                0x590,
            )
        },
        EffectDef {
            duration: Scale(vec![(0.0, 180.0)]),
            fx: 0x4100_038F,
            external_flags: 0x6,
            internal_flags: 0x140_0713,
            ..def(
                ESPIRIT_PULSE,
                Kind::Numeric {
                    flags: 0x6,
                    stats: vec![0x4_0128],
                    magnitude: Scale(vec![(1.0, -5.0), (50.0, -50.0)]),
                },
                0x2E6,
                0x3B,
            )
        },
        EffectDef {
            duration: Scale(vec![(1.0, 15.0), (50.0, 30.0)]),
            fx: 0x4100_026F,
            external_flags: 0xA,
            internal_flags: 0x340_0743,
            ..def(
                STILL_WATER_EFFECT,
                Kind::Numeric {
                    flags: 0xA,
                    stats: vec![0x3_0129],
                    magnitude: Scale(vec![(1.0, 0.9), (50.0, 0.5)]),
                },
                0x1B7,
                0x36,
            )
        },
        EffectDef {
            duration: Scale(vec![(1.0, 15.0), (50.0, 30.0)]),
            fx: 0x4100_0242,
            internal_flags: 0x340_0743,
            ..def(
                HEART_OF_ICE_EFFECT,
                Kind::Chained(
                    [(0x3_0127, 0.9, 0.6), (0x3_0128, 0.95, 0.9)]
                        .map(|(stat, from, to)| {
                            def(
                                0,
                                Kind::Numeric {
                                    flags: 0xA,
                                    stats: vec![stat],
                                    magnitude: Scale(vec![(1.0, from), (50.0, to)]),
                                },
                                0,
                                0,
                            )
                        })
                        .to_vec(),
                ),
                0x18A,
                0x34,
            )
        },
    ]
}

/// An effect that only lasts on its holder, doing nothing (a workshop's, say).
#[must_use]
pub fn plain_effect(state: u32) -> yalain_world::effect_defs::EffectDef {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    EffectDef {
        state,
        kind: Kind::Other,
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: 0,
        enum_value: 0,
        eq_class: 0,
        name: None,
    }
}

/// An effect that attunes the item it is on to whoever holds it while it runs.
#[must_use]
pub fn attuning_effect(state: u32) -> yalain_world::effect_defs::EffectDef {
    use yalain_world::effect_defs::Scale;
    yalain_world::effect_defs::EffectDef {
        duration: Scale(vec![(0.0, 1800.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        eq_class: 114,
        ..plain_effect(state)
    }
}

/// What the tests craft (see the constants above), as the September 2013 data defines it.
#[must_use]
pub fn test_crafting() -> yalain_world::craft::Crafting {
    use yalain_world::craft::{
        Action, Check, CraftSkillDef, Crafting, Ingredient, LevelMap, Outcome, RecipeDef,
    };
    let mines = RecipeDef {
        did: MINES_RECIPE,
        skill: QUEST_RECIPES,
        category: 560,
        charges: -1,
        animation: 0x4100_00A4,
        cycles: 2,
        ingredients: vec![Ingredient {
            ordinal: 1,
            quantity: 20,
            items: vec![IRON],
            ..Ingredient::default()
        }],
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: vec![
                    Action::Produce {
                        template: MINES_SWORD,
                        quantity: 1,
                        ordinal: 10_001,
                    },
                    Action::Other,
                    Action::Other,
                    Action::Effect {
                        state: MINES_PHASE_4,
                        spellcraft: 1.0,
                    },
                    Action::Destroy {
                        ordinal: 1,
                        quantity: 20,
                        by_spinner: None,
                        spinners: None,
                    },
                    Action::Text {
                        text: (0xE7E3_18D9, 0x2500_1AEF),
                        kind: 8,
                    },
                ],
            }],
        }],
        ..RecipeDef::default()
    };
    let knife = RecipeDef {
        did: KNIFE_RECIPE,
        skill: TOOLS,
        category: 410,
        charges: -1,
        recovery: 10_800.0,
        animation: 0x4100_014B,
        cycles: 3,
        checks: vec![Check {
            upto: 2.0,
            outcomes: vec![Outcome {
                upto: 1000.0,
                xp_mod: 1.0,
                actions: vec![
                    Action::Produce {
                        template: KNIFE,
                        quantity: 1,
                        ordinal: 10_001,
                    },
                    Action::Text {
                        text: (0x53FE_2D25, 0x2500_1758),
                        kind: 8,
                    },
                ],
            }],
        }],
        ..RecipeDef::default()
    };
    let mut c = Crafting {
        skills: vec![
            CraftSkillDef {
                did: QUEST_RECIPES,
                cap: 1,
                name: (0xA860_7CEC, 0x2500_13CA),
                advancement: butchery_advancement(),
                daily: LevelMap {
                    first: 1,
                    values: vec![1728],
                },
                pool_share: 0.2,
                ..CraftSkillDef::default()
            },
            CraftSkillDef {
                did: TOOLS,
                cap: 500,
                name: (0x2A1C_5B7E, 0x2500_13CA),
                advancement: vec![0, 0, 40, 80, 120, 160, 200, 240, 280, 320, 360, 400],
                daily: LevelMap {
                    first: 1,
                    values: vec![1728],
                },
                pool_share: 0.2,
                ..CraftSkillDef::default()
            },
            butchery_skill(),
        ],
        starting_recipes: vec![KNIFE_RECIPE],
        ..Crafting::default()
    };
    c.recipes.insert(MINES_RECIPE, mines);
    c.recipes.insert(KNIFE_RECIPE, knife);
    c
}

/// A made-up creature template of the test world: a level 2 beetle with 46 health, worth 11
/// experience, that notices players within 20 m and gives up 40 m from home.
pub const TEST_CREATURE: u32 = 0x4700_2224;
/// A made-up item template a test creature drops, and the test loot profile's value and name.
pub const TEST_ITEM: u32 = 0x4700_2108;
pub const TEST_PROFILE: u32 = 0x19B;
pub const TEST_PROFILE_NAME: &str = "GP_TestBeetleHoard";

/// A template of the test world with made-up looks.
#[must_use]
pub fn test_template(
    id: u32,
    package: u32,
    props: yalain_content::Props,
) -> yalain_content::Template {
    yalain_content::Template {
        id,
        description: yalain_content::Description {
            name_text: None,
            package,
            look: 0x1F00_0000 | (id & 0xFFFF),
            enter_fx: 0x4100_0010,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "made up for the tests".into(),
        },
        facts: yalain_content::Facts {
            level: props
                .int("GameplayStatistics_Level")
                .and_then(|v| i32::try_from(v).ok()),
            max_health: props
                .int("GameplayStatistics_MaxHealth")
                .and_then(|v| i32::try_from(v).ok()),
            ..yalain_content::Facts::default()
        },
        props,
    }
}

/// What the test world plays: one beetle template, the item it drops, and its loot profile.
#[must_use]
pub fn test_catalog() -> yalain_content::Catalog {
    use yalain_content::Prop;
    let mut beetle = yalain_content::Props::default();
    beetle
        .set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(46))
        .set("GameplayStatistics_MaxVigor", Prop::Int(46))
        .set("GameplayStatistics_DeathExperience", Prop::Int(11))
        .set("AICombat_NaturalDamage", Prop::Float(10.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(20.0))
        .set("AI_HomesickRadius", Prop::Float(40.0))
        .set("NPC_DeathLootProfile", Prop::Word(TEST_PROFILE));
    let mut c = yalain_content::Catalog::default();
    c.insert(test_template(TEST_CREATURE, 0x1169, beetle));
    c.insert(test_template(
        TEST_ITEM,
        0x200,
        yalain_content::Props::default(),
    ));
    c.profiles.insert(TEST_PROFILE, TEST_PROFILE_NAME.into());
    c
}

/// A camp of the test world: `n` beetles at spots of the test hall's first copy, beside where a
/// character starts, replaced 60 s after each death.
#[must_use]
pub fn test_camp(n: u32) -> yalain_content::Camp {
    yalain_content::Camp {
        id: "test".into(),
        cell: 0x0299_0109,
        origin: [65.0, 240.0, -44.6],
        radius: 0.0,
        population: n,
        regen: 60.0,
        templates: vec![(TEST_CREATURE, 1)],
        spots: vec![
            (0x0299_0109, [65.0, 240.0, -44.6]),
            (0x0299_0109, [66.0, 244.0, -44.6]),
        ],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
    }
}

/// The test world's loot: the beetle's profile always leaves one test item.
#[must_use]
pub fn test_loot() -> yalain_content::LootTable {
    yalain_content::LootTable {
        profiles: vec![yalain_content::LootProfile {
            name: TEST_PROFILE_NAME.into(),
            counts: vec![0.0, 1.0],
            items: vec![(TEST_ITEM, 1)],
            extras: Vec::new(),
            placed: "authored for the tests".into(),
            confidence: "authored".into(),
            chest: None,
        }],
    }
}

/// Ground made up for a test: no land, and the interior cells each listed cell sees.
#[derive(Debug, Default)]
pub struct TestGround {
    pub sees: std::collections::BTreeMap<u32, Vec<u32>>,
}

impl TestGround {
    /// Ground whose cells see the others as listed: `(cell, the cells it sees)`.
    #[must_use]
    pub fn seeing(cells: &[(u32, &[u32])]) -> Self {
        Self {
            sees: cells.iter().map(|(c, v)| (*c, v.to_vec())).collect(),
        }
    }
}

impl yalain_world::Ground for TestGround {
    fn height(&self, _cell: u32, _origin: [f32; 3]) -> Option<f32> {
        None
    }

    fn sees(&self, cell: u32) -> Option<&[u32]> {
        self.sees.get(&cell).map(Vec::as_slice)
    }
}

/// A client on the harness's wire that the caller holds (a whole client, say), stepped with the
/// server by [`Harness::step_with`].
pub trait Peer {
    /// Its address on the wire.
    fn addr(&self) -> SocketAddr;
    /// A datagram for it.
    fn on_datagram(&mut self, from: SocketAddr, bytes: &[u8], now: LocalTime);
    /// Its turn at `now`: what it sends, and where.
    fn tick(&mut self, now: LocalTime) -> Vec<(SocketAddr, Vec<u8>)>;
}

/// The server, its network, its clients and the clock.
#[derive(Debug)]
pub struct Harness<S: Store = MemoryStore> {
    pub net: ServerNet,
    pub server: Server<S>,
    pub wire: MemoryNet,
    pub bots: Vec<Bot>,
    /// The transport's clock.
    pub now: LocalTime,
    /// Seconds since 1970, as the server's store sees them.
    pub unix: i64,
    unix_carry: f64,
}

impl Harness<MemoryStore> {
    /// A server with an in-memory store, the default settings and the test bodies.
    #[must_use]
    pub fn new() -> Self {
        Self::with(
            ServerConfig::default(),
            MemoryStore::default(),
            test_rules(),
        )
    }
}

impl Default for Harness<MemoryStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Store> Harness<S> {
    /// A server with these settings, store and rules.
    #[must_use]
    pub fn with(cfg: ServerConfig, store: S, rules: CharGenRules) -> Self {
        let now = LocalTime(1000.0);
        let world = SocketAddrV4::new(Ipv4Addr::LOCALHOST, WORLD_PORT);
        // A test puts a player where it needs one by a report; those that test the holding say so.
        let cfg = ServerConfig {
            holds_reports: false,
            ..cfg
        };
        let mut server = Server::new(cfg, store, rules)
            .expect("the store sets ids aside")
            .with_world(test_start(), yalain_content::ContentPack::default());
        server.skill_table = test_skills();
        server.effect_table = test_effects();
        server.crafting = test_crafting();
        Self {
            net: ServerNet::new(
                NetConfig::retail_ids(world),
                fixed_stream(0x5EED),
                1.25e8,
                now,
            ),
            server,
            wire: MemoryNet::default(),
            bots: Vec::new(),
            now,
            unix: 1_700_000_000,
            unix_carry: 0.0,
        }
    }

    /// A skill every character holds from now on, as if each had trained it: tests of what a skill
    /// does use those of the test world's table, whoever uses them.
    pub fn all_hold(&mut self, skill: u32) {
        for list in self.server.start.skills.values_mut() {
            if !list.iter().any(|(s, _)| *s == skill) {
                list.push((skill, 0));
            }
        }
    }

    /// Give the server the test world's creatures: a camp of `n` beetles beside the start, and
    /// their loot.
    pub fn add_creatures(&mut self, n: u32) {
        let camp = test_camp(n);
        self.server.catalog = test_catalog();
        self.server.loot = test_loot();
        self.server.spawns.camps.push(camp);
        self.server
            .camps
            .push(yalain_world::spawn::CampState::default());
    }

    /// A new client; returns its index.
    pub fn add_bot(&mut self) -> usize {
        self.add_bot_from(Ipv4Addr::LOCALHOST)
    }

    /// A new client at another address; returns its index.
    pub fn add_bot_from(&mut self, ip: Ipv4Addr) -> usize {
        let i = self.bots.len();
        let port = 50_000 + u16::try_from(i).unwrap_or(0);
        let addr = SocketAddr::V4(SocketAddrV4::new(ip, port));
        self.bots.push(Bot::new(addr, port_addr(Port::LOGIN_EVEN)));
        i
    }

    /// The clock as the server sees it.
    #[must_use]
    pub fn clock(&self) -> Clock {
        Clock {
            local: self.now,
            unix: self.unix,
        }
    }

    /// One step: deliver what is in flight, let the server handle it, run every connection,
    /// advance the clock by [`STEP`].
    pub fn step(&mut self) {
        self.step_by(STEP);
    }

    /// One step that advances the clock by `dt` seconds.
    pub fn step_by(&mut self, dt: f64) {
        self.step_with(dt, &mut []);
    }

    /// One step with other clients on the wire besides the bots ([`Peer`]s the caller holds):
    /// deliver what is in flight, let the server handle it, run every connection and every
    /// client, advance the clock by `dt` seconds.
    pub fn step_with(&mut self, dt: f64, peers: &mut [&mut dyn Peer]) {
        for d in self.wire.take() {
            if let Some(port) = Port::ALL.into_iter().find(|p| port_addr(*p) == d.to) {
                self.net.on_datagram(port, d.from, &d.bytes, self.now);
            } else if let Some(b) = self.bots.iter_mut().find(|b| b.addr == d.to) {
                b.on_datagram(d.from, &d.bytes, self.now);
            } else if let Some(p) = peers.iter_mut().find(|p| p.addr() == d.to) {
                p.on_datagram(d.from, &d.bytes, self.now);
            }
        }
        let clock = self.clock();
        self.server.pump(&mut self.net, clock);
        self.net.tick(self.now);
        for d in self.net.take_datagrams() {
            self.wire.send(port_addr(d.from), d.to, d.bytes);
        }
        for b in &mut self.bots {
            for (to, bytes) in b.tick(self.now) {
                self.wire.send(b.addr, to, bytes);
            }
        }
        for p in peers.iter_mut() {
            let from = p.addr();
            for (to, bytes) in p.tick(self.now) {
                self.wire.send(from, to, bytes);
            }
        }
        self.advance(dt);
    }

    /// Step until the clock reads `t`, at most a second at a time, so the connections keep
    /// talking (acknowledgements, clock and echo) through a long wait.
    pub fn run_to(&mut self, t: LocalTime) {
        while self.now.0 < t.0 {
            self.step_by((t.0 - self.now.0).clamp(STEP, 1.0));
        }
    }

    /// Move the clock on without stepping.
    pub fn advance(&mut self, seconds: f64) {
        self.now = LocalTime(self.now.0 + seconds);
        self.unix_carry += seconds;
        #[allow(clippy::cast_possible_truncation)]
        let whole = self.unix_carry.floor() as i64;
        self.unix += whole;
        #[allow(clippy::cast_precision_loss)]
        let back = whole as f64;
        self.unix_carry -= back;
    }

    /// Step until `done` holds, at most `max_steps` times. Returns whether it held.
    pub fn run_until(&mut self, max_steps: usize, mut done: impl FnMut(&Self) -> bool) -> bool {
        for _ in 0..max_steps {
            if done(self) {
                return true;
            }
            self.step();
        }
        done(self)
    }

    /// Step `n` times.
    pub fn run(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }
}

/// Every test of the world holds this at its end: each client took every object's visual messages
/// in turn, none waiting for a stamp that never came, and none carrying a visual stamp outside the
/// visual kind.
impl<S: Store> Drop for Harness<S> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        for (b, bot) in self.bots.iter().enumerate() {
            let mut order = invariants::VisualOrder::new();
            for r in bot.inbox.iter().filter(|r| r.world) {
                order.see(r.blob_id, &r.bytes);
            }
            assert_eq!(
                (order.held(), order.misrouted()),
                (Vec::new(), Vec::new()),
                "client {b}: the visual messages it holds, and those sent outside the visual kind"
            );
        }
    }
}
