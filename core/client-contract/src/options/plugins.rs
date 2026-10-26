//! Plugins (experimental): whether the client loads plugins at all.
//!
//! Plugins are experimental, and none loads until the player turns this on and then ticks each
//! plugin they want. It is this client's own option, not a retail one, and only a host that runs
//! plugins offers it: such a host registers it, off, as the client starts and before the profile
//! is read ([`register`]), so the profile keeps it and a profile written before it existed reads
//! as off. A host that does not offer it never registers it, and the options pages leave its row
//! out ([`super::sheet::Row::hosted`]). Every interface that shows the row shows it on the Client
//! Options page under Plugins; the plugin host reads it every frame ([`turned_on`]), so a change
//! applies at once.

use crate::view::PrefValue;

/// `[Plugins] Experimental`: whether plugins are turned on.
pub const PLUGINS: &str = crate::options::names::PLUGINS_EXPERIMENTAL;

/// The heading the row stands under.
pub const HEADING: &str = "Plugins";

/// The option's caption, literal text: no string table has one.
pub const CAPTION: &str = "Plugins (experimental)";

/// What the option does, where a page explains it.
pub const NOTE: &str = "Type /plugin to see the plugins and turn each on.";

/// Register the option in the option value store, off: what a host that runs plugins does as the
/// client starts, before the profile is read. A store that already has it keeps what it holds.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        PLUGINS,
        PrefValue::Bool(false),
        super::store::DataType::Bool,
    ))
}

/// Whether plugins are turned on: only where the store holds the option on, so a store without
/// it (a host that does not offer it, a profile that never had it) reads as off.
#[must_use]
pub fn turned_on() -> bool {
    matches!(
        super::store::inq_value(PLUGINS),
        Some(PrefValue::Bool(true))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::preferences::UserPreferences;

    /// The option is off at first, and a profile saved before it existed reads as off; turned on,
    /// it is saved in the profile's `[Plugins]` section and a profile read back turns it on again.
    ///
    /// Behaviour: plugins.load.plugins-are-off-until-the-player-turns-them-on
    #[test]
    fn plugins_are_off_at_first_and_in_an_old_profile_and_a_saved_one_brings_them_back_on() {
        super::super::store::clear();
        assert!(!turned_on(), "a store without the option is off");
        super::super::store::init();
        register();
        assert_eq!(
            super::super::store::inq_value(PLUGINS),
            Some(PrefValue::Bool(false))
        );
        assert!(!turned_on());

        // A profile written before the option existed.
        let old = UserPreferences::parse("[Display]\r\nFullScreen=True\r\n").expect("parses");
        super::super::store::load(&old);
        assert!(!turned_on(), "an old profile reads as off");

        assert!(super::super::store::set_value(
            PLUGINS,
            PrefValue::Bool(true)
        ));
        assert!(turned_on());
        let text = super::super::store::save().to_text();
        let section = text
            .split("[Plugins]\r\n")
            .nth(1)
            .and_then(|s| s.split("\r\n[").next())
            .unwrap_or_else(|| panic!("no [Plugins] section: {text}"));
        assert!(
            section.lines().any(|l| l.trim_end() == "Experimental=True"),
            "{section}"
        );

        // A fresh start is off again, until the saved profile is read.
        super::super::store::init();
        register();
        assert!(!turned_on());
        let ini = UserPreferences::parse(&text).expect("the saved profile parses");
        super::super::store::load(&ini);
        assert!(turned_on());
        super::super::store::clear();
    }
}
