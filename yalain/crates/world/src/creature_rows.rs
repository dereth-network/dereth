//! Creature rows: the kinds a world gives none of (it has no concealing effects, healers of kin,
//! friend tables that never attack first or fight others, or creatures' rows of such a kind), the
//! fields of its friend tables' and creatures' rows none names, and the rhythm of a shooter the
//! rows name none for.

use yalain_content::creature_rows::{CreatureRows, Rhythm};

/// The rhythm of a shooter the rows give none of: the beetles' (Edge, Cutter and Sharp Cutter
/// Beetles).
pub const OTHER_SHOOTERS: Rhythm = Rhythm {
    gap: 1.63,
    windup: 0.55,
};

/// The kinds of creature row a world has none of.
#[must_use]
pub fn kinds_missing(rows: &CreatureRows) -> Vec<&'static str> {
    [
        ("concealing", rows.concealing.is_empty()),
        (
            "npc_table",
            rows.heals_kin.is_empty() && rows.skips.is_empty(),
        ),
        (
            "friend_table",
            rows.attacks_first.is_empty() && rows.fights.is_empty(),
        ),
        ("creature", no_creature_rows(rows)),
    ]
    .into_iter()
    .filter_map(|(kind, none)| none.then_some(kind))
    .collect()
}

fn no_creature_rows(rows: &CreatureRows) -> bool {
    rows.rhythms.is_empty()
        && rows.marks.is_empty()
        && rows.only_hated.is_empty()
        && rows.kept_out.is_empty()
        && rows.aligned.is_empty()
        && rows.no_victory.is_empty()
}

/// The fields of `[[npc_table]]`, `[[friend_table]]` and `[[creature]]` no row of a world that has
/// some of that kind names.
#[must_use]
pub fn fields_missing(rows: &CreatureRows) -> Vec<&'static str> {
    let tables = !rows.attacks_first.is_empty() || !rows.fights.is_empty();
    let npc_tables = !rows.heals_kin.is_empty() || !rows.skips.is_empty();
    let creatures = !no_creature_rows(rows);
    [
        (
            "[[npc_table]] heals_kin",
            npc_tables && rows.heals_kin.is_empty(),
        ),
        ("[[npc_table]] skips", npc_tables && rows.skips.is_empty()),
        ("[[friend_table]] fights", tables && rows.fights.is_empty()),
        (
            "[[creature]] shot_gap",
            creatures && rows.rhythms.is_empty(),
        ),
        ("[[creature]] marks", creatures && rows.marks.is_empty()),
        (
            "[[creature]] only_hated",
            creatures && rows.only_hated.is_empty(),
        ),
        (
            "[[creature]] players_kept_out",
            creatures && rows.kept_out.is_empty(),
        ),
        (
            "[[creature]] aligned_for",
            creatures && rows.aligned.is_empty(),
        ),
        (
            "[[creature]] no_victory",
            creatures && rows.no_victory.is_empty(),
        ),
    ]
    .into_iter()
    .filter_map(|(field, none)| none.then_some(field))
    .collect()
}
