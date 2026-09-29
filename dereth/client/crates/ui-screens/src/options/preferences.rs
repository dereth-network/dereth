//! The client's UI-preference registration — the registry every option label comes out of.
//!
//! `dereth_ui::persist::preferences::UiPreferences` is the registry; this module is its producer.
//! Without it the preference query can never answer and every control on every option page is
//! drawn with no text on it.
//!
//! # What the function actually is
//!
//! Straight-line registration, and nothing else. **34** attach calls
//!
//! ```text
//! attach(name, kind, 0x10000003, labelToken, helpToken)
//! ```
//!
//! with `kind` 2 = enum, 3 = float, 4 = bool; **11** range calls `(name, lo, hi)`; and **8**
//! enum-choice calls `(name, [tokens…])`. Every token is the string hash of an `"ID_…"` name
//! (`dereth_primitives::num::hash::str_hash`, which `dereth_ui::persist::preferences::token_of` already is), and
//! the string table enum is `0x10000003` on all 34 — [`STRING_TABLE_ENUM`].
//!
//! So [`init_ui_preferences`] supplies **four** things, not one:
//!
//! | | what | who consumes it |
//! |---|---|---|
//! | label token | `ID_Sound_EffectVolume` … | the option row's label |
//! | help token | `ID_Sound_EffectVolume_Help` … | the option row's tooltip |
//! | range | 11 of the 34 | the option slider's preference bind |
//! | enum choices | 8 of the 34 | the option menu's preference bind |
//!
//! The **range** column comes from here, through [`inq_preference_range`], as it does in the
//! client; `options::page::SLIDER_RANGES` is an independently-written oracle a test checks this
//! table against, which is the only way a transcription slip in either copy can be seen.
//!
//! # Attaching fails unless the preference is already in the global registry
//!
//! A UI preference item's first act on initialise is
//! a check that `name` is registered as a preference of any type, and it returns `false`
//! without attaching anything when that is not true. **The options metadata decorates
//! `UserPreferences`; it does not hold values.** That is why the value store lives in
//! [`super::store`] and this module holds only metadata, and it is why
//! [`init_ui_preferences`] registers the store first.
//!
//! # Which string table
//!
//! Enum `0x10000003` resolves through `DidMapper` **group 4** to `DataId(0x23000003)`, the
//! `Preference` table — 236 strings. **Measured, both directions**: all 63 `ID_*` tokens this
//! module and the option build name resolve in `0x23000003`, and **0 of 63** resolve in any of
//! the other fifteen shipped string tables, which is what makes the hit a measurement rather than
//! a coincidence.

use dereth_primitives::DataId;
use dereth_ui::persist::preferences::{token_of, UiPreferenceItem, UiPreferences};

/// The string table enum every one of the 34 attach calls passes.
pub const STRING_TABLE_ENUM: u32 = 0x1000_0003;

/// What [`STRING_TABLE_ENUM`] resolves to through `DidMapper` group 4 — the
/// `Preference` string table.
///
/// Resolved rather than hard-coded at the call site: see [`table`].
pub const STRING_TABLE: DataId = DataId(0x2300_0003);

/// The `DidMapper` group the string tables live in.
pub const STRING_TABLE_GROUP: u32 = 4;

/// The string table enum `0x10000003` names in `DidMapper` group 4, through the installed
/// environment.
///
/// Falls back to [`STRING_TABLE`] when no environment is installed, so a headless test that never
/// called [`crate::env::install`] still names the right table instead of `DataId(0)` — the
/// resolution is the *production* path and the constant is the documented answer it gives against
/// the shipped dats.
#[must_use]
pub fn table(ui: &dereth_ui::UiSystem) -> DataId {
    ui.env()
        .cloned()
        .and_then(|e| e.did_by_enum(STRING_TABLE_GROUP, STRING_TABLE_ENUM))
        .unwrap_or(STRING_TABLE)
}

/// [`UiPref`] and the 34-row table are plain data that [`super::store::init`] reads, so they live
/// in [`dereth_client_contract::options::preferences`] with the store and are re-exported here.
pub use dereth_client_contract::options::preferences::{UiPref, UI_PREFERENCES};

thread_local! {
    /// The UI-preference registry — a file-static in the client
    /// (the client source), and thread-local here for the same reason
    /// [`crate::env`] and [`crate::requests`] are: the UI runs on one thread, as it does in the
    /// client, and a per-thread singleton keeps one test from seeing another's registry.
    static UI_PREFS: std::cell::RefCell<UiPreferences> =
        std::cell::RefCell::new(UiPreferences::new());
}

/// Register all 34, ranges and enum choices included.
///
/// Idempotent: the client calls it once, and
/// [`crate::register_all`] calls it once per host. Attaching refuses a name already in
/// the table (an existing entry makes it return `false`), so a second call is a no-op there too;
/// this rebuilds the table instead, which is the same end state and cannot leave a half-registered
/// registry behind if the table ever changes under a running host.
///
/// Registers the **value store** first, because attaching a UI preference refuses any name
/// the underlying preference registry does not already know.
pub fn init_ui_preferences() -> usize {
    super::store::init();
    let mut reg = UiPreferences::new();
    let mut n = 0usize;
    for p in UI_PREFERENCES {
        // Is `name` registered as a preference of any type — the gate initialisation fails on.
        if !super::store::is_registered(p.name) {
            continue;
        }
        let Ok(mut item) = UiPreferenceItem::new(
            p.name,
            p.kind,
            STRING_TABLE_ENUM,
            token_of(p.label),
            token_of(p.help),
        ) else {
            continue;
        };
        item.range = p.range;
        item.choices = p.choices.iter().map(|c| token_of(c)).collect();
        reg.attach(item);
        n += 1;
    }
    UI_PREFS.with(|r| *r.borrow_mut() = reg);
    n
}

/// Empty the registry. A test does this to prove a page fails loudly without one.
pub fn clear() {
    UI_PREFS.with(|r| *r.borrow_mut() = UiPreferences::new());
}

/// How many preferences are attached — the registry's element count.
#[must_use]
pub fn len() -> usize {
    UI_PREFS.with(|r| r.borrow().len())
}

/// The preference query: name in; string table, label token and tooltip token out.
///
/// The out-parameter order is the client's: the string table, then the label string id, then
/// the tooltip string id.
#[must_use]
pub fn inq_preference(name: &str) -> Option<(u32, u32, u32)> {
    UI_PREFS.with(|r| {
        let r = r.borrow();
        let i = r.inq(name)?;
        Some((i.string_table, i.token_name, i.token_tooltip))
    })
}

/// The preference range query — the float overload.
#[must_use]
pub fn inq_preference_range(name: &str) -> Option<(f32, f32)> {
    UI_PREFS.with(|r| r.borrow().inq(name)?.range)
}

/// The enum-choice inquiry — the string ids the enum-choice registration stored.
#[must_use]
pub fn items(name: &str) -> Vec<u32> {
    UI_PREFS.with(|r| {
        r.borrow()
            .inq(name)
            .map(|i| i.choices.clone())
            .unwrap_or_default()
    })
}

/// The table row for `name`, for a caller that wants the whole thing.
#[must_use]
pub fn find(name: &str) -> Option<&'static UiPref> {
    UI_PREFERENCES.iter().find(|p| p.name == name)
}

#[cfg(test)]
mod tests {
    use super::super::config::PrefValueConst::{Bool, Float, Int};
    use super::*;

    /// Oracle: the client's UI-preference registration, counted call by call — **34**
    /// preference attachments, **11** range settings, **8** enum-choice settings, and
    /// `0x10000003` on every one of the 34.
    ///
    /// The denominators are the point (the stated testability rule, *"assert the denominator"*): a table that
    /// silently lost a row would still pass every per-row assertion below it.
    #[test]
    fn the_registration_table_is_the_thirty_four_calls_the_client_makes() {
        assert_eq!(UI_PREFERENCES.len(), 34, "preference attachment call sites");
        assert_eq!(
            UI_PREFERENCES.iter().filter(|p| p.range.is_some()).count(),
            11,
            "preference range call sites"
        );
        assert_eq!(
            UI_PREFERENCES
                .iter()
                .filter(|p| !p.choices.is_empty())
                .count(),
            8,
            "enum-choice call sites"
        );
        assert_eq!(STRING_TABLE_ENUM, 0x1000_0003);
        assert_eq!(STRING_TABLE, DataId(0x2300_0003));
        assert_eq!(STRING_TABLE_GROUP, 4);
        // 2 = enum, 3 = float, 4 = bool; nothing else appears, and the counts are the client's.
        let k = |v: u32| UI_PREFERENCES.iter().filter(|p| p.kind == v).count();
        assert_eq!((k(2), k(3), k(4)), (10, 11, 13));
        assert_eq!(
            k(2) + k(3) + k(4),
            34,
            "every row is one of the three kinds"
        );
        // Every float has a range and nothing else does: the range setting's 12 call sites are
        // exactly the 12 `kind == 3` rows.
        for p in UI_PREFERENCES {
            assert_eq!(p.range.is_some(), p.kind == 3, "{}", p.name);
            assert!(
                p.choices.is_empty() || p.kind == 2,
                "{} has choices",
                p.name
            );
        }
        // No name is registered twice — attaching a preference would refuse the second.
        let mut names: Vec<&str> = UI_PREFERENCES.iter().map(|p| p.name).collect();
        names.sort_unstable();
        let n = names.len();
        names.dedup();
        assert_eq!(
            names.len(),
            n,
            "a duplicate name would be refused by the preference attachment"
        );
    }

    /// Oracle: the strings the tokens hash, as literals.
    ///
    /// **Spelled out rather than read back through the table**, because a test that reads a
    /// constant through the same symbol it writes it through cannot detect a wrong constant. The
    /// tooltip token is the label token with `_Help` appended in all 34 — asserted as a *rule*
    /// below, and the three spot rows here are the evidence the rule was read and not assumed.
    #[test]
    fn the_tokens_are_the_strings_the_client_hashes() {
        let by = |n: &str| find(n).unwrap_or_else(|| panic!("{n} is not registered"));
        assert_eq!(by("Sound.SoundVolume").label, "ID_Sound_EffectVolume");
        assert_eq!(by("Sound.SoundVolume").help, "ID_Sound_EffectVolume_Help");
        assert_eq!(
            by("Render.AutomaticDegrades").label,
            "ID_Graphics_AdaptiveDegrade"
        );
        assert_eq!(
            by("Render.GraphicsPerformance").label,
            "ID_Graphics_AdaptiveDegradeBias"
        );
        assert_eq!(by("Display.FullScreen").label, "ID_Rendering_FullScreen");
        assert_eq!(by("Camera.AlignToSlope").label, "ID_Camera_AlignToSlope");
        assert_eq!(
            by("Sound.SoundFeatures").choices,
            ["ID_Sound_Stereo", "ID_Sound_Mono"]
        );
        // The rule, over all 34.
        for p in UI_PREFERENCES {
            assert!(p.label.starts_with("ID_"), "{}", p.name);
            assert_eq!(p.help, format!("{}_Help", p.label), "{}", p.name);
        }
        // The client's string hash is `dereth_primitives::num::hash::str_hash`, and two different tokens
        // must not collide — a collision would silently caption a row with its neighbour's text.
        let mut hashes: Vec<u32> = UI_PREFERENCES
            .iter()
            .flat_map(|p| [token_of(p.label), token_of(p.help)])
            .collect();
        assert_eq!(hashes.len(), 68);
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(hashes.len(), 68, "two tokens hash to the same string id");
    }

    /// Oracle: the recovered preference registrations' *default* column and
    /// `super::super::config::DEFAULT_DISAGREEMENTS`, independently cross-checked against the
    /// recovered options behavior.
    ///
    /// The historical three disagreements plus texture filtering (registered 0 at startup,
    /// 1 on *Restore Defaults*):
    /// different registered/UI values prove that a page seeded itself from the store.
    #[test]
    fn the_registered_defaults_disagree_with_the_ui_defaults_in_exactly_four_places() {
        use super::super::config::{CONFIG_PAGE, DEFAULT_DISAGREEMENTS};
        let mut differing = Vec::new();
        for row in CONFIG_PAGE {
            let Some(p) = find(row.preference) else {
                panic!(
                    "{} is a page row with no preference attachment",
                    row.preference
                )
            };
            if p.registered_default != row.ui_default {
                differing.push(row.preference);
            }
        }
        differing.sort_unstable();
        let mut named: Vec<&str> = DEFAULT_DISAGREEMENTS.iter().map(|d| d.preference).collect();
        named.sort_unstable();
        assert_eq!(differing, named, "the disagreements this table produces");
        // …and the values themselves, from §5.4, as literals.
        assert_eq!(
            find("Display.Resolution").unwrap().registered_default,
            Int(0x0400_0300)
        );
        assert_eq!(
            find("Render.AutomaticDegrades").unwrap().registered_default,
            Bool(true)
        );
        assert_eq!(
            find("Input.MouseLookSensitivity")
                .unwrap()
                .registered_default,
            Float(0.25)
        );
        for d in DEFAULT_DISAGREEMENTS {
            assert_eq!(
                find(d.preference).unwrap().registered_default,
                d.registered,
                "{} — §5.4 and §2 must agree on the registered value",
                d.preference
            );
        }
    }

    /// Every range agrees with the independently transcribed slider table.
    #[test]
    fn every_range_agrees_with_the_independently_transcribed_slider_table() {
        for (name, want) in super::super::page::SLIDER_RANGES {
            let got = find(name)
                .unwrap_or_else(|| panic!("{name} not registered"))
                .range;
            assert_eq!(got, Some(want), "{name}");
        }
        assert_eq!(find("Misc.TooltipDelay").unwrap().range, Some((0.0, 10.0)));
        assert_eq!(super::super::page::SLIDER_RANGES.len(), 10);
        assert_eq!(
            UI_PREFERENCES.iter().filter(|p| p.range.is_some()).count(),
            11
        );
    }

    /// The registry itself: attach all 34, then read three of them back the way
    /// the option slider's preference bind does.
    #[test]
    fn the_registry_answers_inq_preference_once_the_producer_has_run() {
        clear();
        assert_eq!(len(), 0);
        assert_eq!(
            inq_preference("Sound.SoundVolume"),
            None,
            "nothing is attached yet"
        );
        assert_eq!(init_ui_preferences(), 34);
        assert_eq!(len(), 34);
        let (table, label, help) =
            inq_preference("Sound.SoundVolume").expect("the preference inquiry answers");
        assert_eq!(table, 0x1000_0003);
        assert_eq!(
            label,
            dereth_primitives::num::hash::str_hash(b"ID_Sound_EffectVolume")
        );
        assert_eq!(
            help,
            dereth_primitives::num::hash::str_hash(b"ID_Sound_EffectVolume_Help")
        );
        assert_ne!(label, help);
        assert_eq!(inq_preference_range("Sound.SoundVolume"), Some((0.0, 1.0)));
        assert_eq!(
            inq_preference_range("Camera.AdjustmentSpeed"),
            Some((5.0, 80.0))
        );
        assert_eq!(
            inq_preference_range("Camera.AlignToSlope"),
            None,
            "a bool has no range"
        );
        assert_eq!(
            items("Sound.SoundFeatures"),
            vec![
                dereth_primitives::num::hash::str_hash(b"ID_Sound_Stereo"),
                dereth_primitives::num::hash::str_hash(b"ID_Sound_Mono")
            ]
        );
        assert!(items("Sound.SoundVolume").is_empty());
        // A name nobody registered answers `false`, not a default.
        assert_eq!(
            inq_preference("Net.BindInterface"),
            None,
            "Net.* is not attached to the UI"
        );
        assert_eq!(inq_preference("Render.DisplayAdapter"), None);
        assert_eq!(inq_preference("Input.MouseLookSmoothingAmount"), None);
    }
}
