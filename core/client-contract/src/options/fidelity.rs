//! The remaster's options as a whole: registered with the value store in a build with the
//! `fidelity` feature ([`register`]), so the page reads and writes them like any other option and
//! the profile keeps them in its `[Fidelity]` section. Without it nothing here is registered and no
//! page shows them. The boxes themselves are [`super::remaster`]'s.

use super::names::fidelity;
use crate::PrefValue;

/// Register every box of the remaster's with the value store, each off. The store's load then reads the
/// `[Fidelity]` section of the profile into them, and the page reads and writes them.
pub fn register() -> usize {
    super::remaster::register()
}

/// Whether the store's save leaves `name` out of the profile: a box of the remaster's still as it
/// is at first that the profile does not name, so a profile that never moved one has no
/// `[Fidelity]` section, a box ticked at first by a later build is ticked for it, and a pack named
/// in it later still ticks the art pack box. One the player moved is kept.
#[must_use]
pub fn left_out_of_save(
    name: &str,
    value: &PrefValue,
    ini: &crate::persist::preferences::UserPreferences,
) -> bool {
    fidelity::NAMES
        .iter()
        .any(|n| n.eq_ignore_ascii_case(name) && *n != fidelity::REMASTER_SCALE)
        && *value == PrefValue::Bool(super::remaster::ticked_at_first(name))
        && !ini
            .entries
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the remaster's option bookkeeping; visual only).
    use super::*;

    #[test]
    fn the_art_pack_box_stays_out_of_a_profile_until_it_has_been_ticked() {
        use crate::persist::preferences::UserPreferences;
        let off = PrefValue::Bool(false);
        let name = fidelity::REMASTER_ART_PACK;
        let never = UserPreferences::parse(
            "[Fidelity]
ArtPack=packs/kept
",
        )
        .expect("parses");
        assert!(left_out_of_save(name, &off, &never));
        let ticked = UserPreferences::parse(
            "[Fidelity]
RemasterArtPack=True
",
        )
        .expect("parses");
        assert!(!left_out_of_save(name, &off, &ticked), "an untick is kept");
        assert!(!left_out_of_save(name, &PrefValue::Bool(true), &never));
    }
}
