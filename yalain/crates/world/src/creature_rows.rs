//! The server's own creature rows, for a world that gives none of a kind: whom the concealing
//! effects spare or make hated, which healers heal their kin, which friend tables never attack a
//! player first, and the start area's shooters' rhythms.

use yalain_content::creature_rows::{CreatureRows, Regard, Rhythm};

/// The concealing effects whose reach is known: each one's saved states, the class of creature it
/// is for (none: every creature; an effect for two classes has a row for each) and what they make of
/// its holder. The install says so of those with a state and its copy. The amulet of peace's
/// description names its moarsmen (2017: under it, 0 of 8 moarsmen within 20 m came at the player;
/// without it, 24 of 140 came at it first).
///
/// The last rows are read from words alone: the fanatics, the slave spawn, the Archon forces and
/// those loyal to Aerfalle and Geraine, whose classes hold those names and hardly any other. No
/// recording shows any of them held. The other effects of the kind do nothing: one names
/// no one, one names people who share their class with others, one is a guard's mark too seldom
/// recorded to read, and under the rabbits' disguise those that attack first went on doing so (5 of
/// 5).
const CONCEALING: [(u32, Option<u32>, Regard); 27] = [
    (0x6F00_1C46, Some(0x17), Regard::Spares),
    (0x6F00_1D4A, Some(0x1D), Regard::Spares),
    (0x7000_0701, Some(0x1D), Regard::Spares),
    (0x6F00_1EEB, Some(0x0E), Regard::Spares),
    (0x7000_0713, Some(0x0E), Regard::Spares),
    (0x6F00_0DC9, Some(0x3C), Regard::Spares),
    (0x7000_1D76, Some(0x3C), Regard::Spares),
    (0x6F00_1D4C, Some(0x16), Regard::Hates),
    (0x7000_0703, Some(0x16), Regard::Hates),
    (0x6F00_1EE8, Some(0x0E), Regard::Hates),
    (0x7000_0710, Some(0x0E), Regard::Hates),
    (0x6F00_1D2E, Some(0x6F), Regard::Hates),
    (0x7000_1DAE, Some(0x6F), Regard::Hates),
    (0x6F00_20BC, Some(0x06), Regard::Hates),
    (0x7000_1EB8, Some(0x06), Regard::Hates),
    (0x6F00_0D8C, None, Regard::Hates),
    (0x7000_1CB3, None, Regard::Hates),
    (0x6F00_19F1, Some(0x59), Regard::Spares),
    (0x6F00_1C48, Some(0x78), Regard::Hates),
    (0x6F00_1C77, Some(0x46), Regard::Hates),
    (0x6F00_1C77, Some(0x43), Regard::Hates),
    (0x6F00_1C78, Some(0x45), Regard::Hates),
    (0x6F00_1C78, Some(0x44), Regard::Hates),
    (0x6F00_2049, Some(0x46), Regard::Spares),
    (0x6F00_2049, Some(0x43), Regard::Spares),
    (0x6F00_204B, Some(0x45), Regard::Spares),
    (0x6F00_204B, Some(0x44), Regard::Spares),
];

/// The behavior tables of the healers, which heal their kin.
const HEALERS: [&str; 5] = [
    "NPCTable_DrudgeGloom",
    "NPCTable_BurunLeader",
    "NPCTable_MoarsmanHealer",
    "NPCTable_MimbuHealer",
    "NPCTable_Crone",
];

/// Friend tables whose creatures never attack a player first (2017: rabbits 1 of 386 players in
/// reach, the peaceful 8 of 291, peaceful drudges 1 of 64, one side of a war 0 of 17), with those
/// friendly to players.
const NEVER_FIRST: [u32; 5] = [
    yalain_content::props::friend_table::PLAYER_FRIENDLY,
    0x0E,
    0x23,
    0x38,
    0x35,
];

/// The start area's shooters' rhythms by template (2017: the gap is about constant times the
/// shot's time scale, and the windup measured at the median time scale 1.296): the drudges'
/// (Outcast, Downtrodden: 2.37, 0.79); the wasps' (Green Phyntos, Red Soldier: 1.33, 0.45); the
/// Reedshark Runt's windup 0.98; the Sandy Armoredillo's (3.58, 1.14).
const RHYTHMS: [(u32, f32, f32); 6] = [
    (0x4700_2398, 2.37, 0.79),
    (0x4700_23A0, 2.37, 0.79),
    (0x4700_2963, 1.33, 0.45),
    (0x4700_296C, 1.33, 0.45),
    (0x4700_29EE, 1.63, 0.98),
    (0x4700_21BE, 3.58, 1.14),
];

/// The rhythm of a shooter the rows give none of: the beetles' (Edge, Cutter and Sharp Cutter
/// Beetles).
pub const OTHER_SHOOTERS: Rhythm = Rhythm {
    gap: 1.63,
    windup: 0.55,
};

/// The server's own creature rows, every kind of them.
#[must_use]
pub fn fallback() -> CreatureRows {
    CreatureRows {
        concealing: CONCEALING
            .iter()
            .map(|(state, class, how)| ((*state, *class), *how))
            .collect(),
        heals_kin: HEALERS.iter().map(|n| ((*n).to_string(), true)).collect(),
        attacks_first: NEVER_FIRST.iter().map(|t| (*t, false)).collect(),
        rhythms: RHYTHMS
            .iter()
            .map(|(t, gap, windup)| {
                (
                    *t,
                    Rhythm {
                        gap: *gap,
                        windup: *windup,
                    },
                )
            })
            .collect(),
    }
}

/// A world's creature rows, each kind its rows or, with none, the server's own (never the two
/// merged), and the kinds that fell back.
#[must_use]
pub fn world_creature_rows(rows: CreatureRows) -> (CreatureRows, Vec<&'static str>) {
    let mut out = rows;
    let own = fallback();
    let mut fell_back = Vec::new();
    if out.concealing.is_empty() {
        out.concealing = own.concealing;
        fell_back.push("concealing");
    }
    if out.heals_kin.is_empty() {
        out.heals_kin = own.heals_kin;
        fell_back.push("npc_table");
    }
    if out.attacks_first.is_empty() {
        out.attacks_first = own.attacks_first;
        fell_back.push("friend_table");
    }
    if out.rhythms.is_empty() {
        out.rhythms = own.rhythms;
        fell_back.push("creature");
    }
    (out, fell_back)
}
