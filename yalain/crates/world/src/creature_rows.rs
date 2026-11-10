//! Creature rows: the kinds a world gives none of (it has no concealing effects, healers of kin,
//! friend tables that never attack first or creatures' rows of such a kind), the fields of its
//! creatures' rows none names, and the rhythm of a shooter the rows name none for.

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
        ("npc_table", rows.heals_kin.is_empty()),
        ("friend_table", rows.attacks_first.is_empty()),
        (
            "creature",
            rows.rhythms.is_empty() && rows.marks.is_empty() && rows.only_hated.is_empty(),
        ),
    ]
    .into_iter()
    .filter_map(|(kind, none)| none.then_some(kind))
    .collect()
}

/// The fields of `[[creature]]` no row of a world that has some names.
#[must_use]
pub fn fields_missing(rows: &CreatureRows) -> Vec<&'static str> {
    if rows.rhythms.is_empty() && rows.marks.is_empty() && rows.only_hated.is_empty() {
        return Vec::new();
    }
    [
        ("[[creature]] shot_gap", rows.rhythms.is_empty()),
        ("[[creature]] marks", rows.marks.is_empty()),
        ("[[creature]] only_hated", rows.only_hated.is_empty()),
    ]
    .into_iter()
    .filter_map(|(field, none)| none.then_some(field))
    .collect()
}
