//! `UserPreferences` — the value store an option control reads and writes, and the read seam this
//! workspace did not have.
//!
//! # What retail reads, and when
//!
//! Every option control's value reader queries the preference registry: the
//! slider asks for a float, the check box for a bool and the menu for a `ulong`. Each supported
//! type (bool, long, ulong, float, or string) has a separate query entry point. The query looks
//! up the preference by name and then dereferences the variable pointer supplied during
//! registration. The write (modify the preference, set its value, store the variable) is a store
//! through the same pointer. **The control's store *is* the subsystem's variable**, which is why
//! the sound manager's own preference registration can pass a null change callback and still be
//! correct.
//!
//! The typed query's type check is not a formality and is reproduced below: it walks the registry
//! twice, and **both** hops require the stored type to equal the requested `DataType`. A float
//! read of a bool preference returns `false` and writes nothing, rather than reinterpreting four
//! bytes.
//!
//! Three moments, and only three, read a preference in retail:
//!
//! | when | path |
//! |---|---|
//! | the page is **shown** | the page's visibility change -> the page saves each option's current value: current = saved = the registry's value |
//! | **Apply** is clicked | the same save-current-values path |
//! | the character logs in | the player-description handler (message `0x0013`) sends player-description-received and reload-options notices; every option's reload handler sets current = the registry's value, then refreshes |
//!
//! Each option control's UI-preference setup also consults the registry, but for **metadata** — the label
//! token, the help token, the range and the enum choices — not for the value. That half is
//! `super::preferences`.
//!
//! # Does reading already work by another route?
//!
//! Partly, and the honest answer is what makes this a defect rather than a tidy-up.
//!
//! * The **write** direction has a consumer: `UiRequest::SetPreference` reaches
//!   `dereth_client::audio::apply_preference_requests` and `dereth_audio::Prefs::set_named`, which is
//!   the address binding for the eight `Sound.*` names.
//! * The eight `Sound.*` values *are* readable in the running client — `dereth_audio::Prefs` holds
//!   them and `Prefs::from_ini` seeds them from `UserPreferences.ini` — and the four `Display.*`
//!   are readable from `dereth_client::config::Preferences`, which is a full by-name reader over the
//!   same file. **Neither is reachable from this crate**, and the remaining 22 attached
//!   preferences have no store at all in this build.
//! * So `PlayerOptionPage` seeded current = saved = default and its own snapshot stood in
//!   for the store. Within one page visit that is indistinguishable from the client; across a
//!   visit it is not, and the **three `SetDefault` values that disagree with the registration**
//!   (`super::config::DEFAULT_DISAGREEMENTS`) make the difference observable without a running
//!   client: retail opens the page showing 1024x768, adaptive degrades **on** and mouse
//!   sensitivity 0.25, and a page that seeds itself from `SetDefault` opens showing 800x600,
//!   degrades **off** and 0.55.
//!
//! # What is here, and what is not
//!
//! Here: the registry, its type check, `inq_value`/`set_value`, and the load's fifth step —
//! pushing an already-parsed `UserPreferences.ini` into the registered variables.
//!
//! **Not here: a change callback.** The fifth preference-registration argument is
//! a callback taking a borrowed narrow string and returning no value; eleven registrations use it to
//! notify rendering, font, UI-element, or camera-stiffness state; all eight
//! `Sound.*` pass null. This build routes the change outward as
//! [`crate::view::UiRequest::SetPreference`] instead, which the host already drains — so the
//! callback slot is deliberately absent rather than stubbed, and the subsystem that needs telling
//! is told by the request rather than by a function pointer.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::persist::preferences::UserPreferences;

use crate::view::PrefValue;

/// The stored type tag that every query and update checks before touching the variable.
///
/// These are the four types used by the 34 attached preferences. The unknown type is the wildcard
/// the registration check passes and is spelled `None` here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// Boolean — its own read and write entry points.
    Bool,
    /// Signed 32-bit integer — its own read and write entry points.
    Int,
    /// Unsigned 32-bit integer — its own read and write entry points. Every `kind == 2`
    /// (enumeration) preference is registered as this.
    UInt,
    /// 32-bit float — its own read and write entry points.
    Float,
}

impl DataType {
    /// The type a value carries, for the check both hops of the typed query perform.
    #[must_use]
    pub const fn of(v: &PrefValue) -> Option<Self> {
        Some(match v {
            PrefValue::Bool(_) => Self::Bool,
            PrefValue::Int(_) => Self::Int,
            PrefValue::Float(_) => Self::Float,
            // The string type; no attached preference is one, so it has no `DataType`.
            PrefValue::Text(_) => return None,
        })
    }

    /// Whether a value may be stored in a variable of this type.
    ///
    /// `UInt` accepts a `PrefValue::Int`: this crate has one integer `PrefValue` and the client has
    /// two integer types, and every enumeration preference is the unsigned one. The bit pattern
    /// is the same — `Display.Resolution`'s `0x04000300` is `width << 16 | height` either way —
    /// and reads it through the `ulong` overload.
    #[must_use]
    pub const fn accepts(self, v: &PrefValue) -> bool {
        matches!(
            (self, v),
            (Self::Bool, PrefValue::Bool(_))
                | (Self::Int | Self::UInt, PrefValue::Int(_))
                | (Self::Float, PrefValue::Float(_))
        )
    }
}

/// One registered option — a name, a type and the variable it is bound to.
///
/// In the client the variable is a pointer to a subsystem global. Here it is the value itself:
/// this crate cannot hold a `&mut` into `dereth_audio`, and the outward
/// [`crate::view::UiRequest::SetPreference`] is what carries the write to the subsystem that owns
/// the real variable. Named divergence, and it is the reason [`load`] exists — the client's store
/// is seeded by the preferences loader writing straight through those pointers, and this one has
/// to be handed the same file.
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub data_type: DataType,
    pub value: PrefValue,
}

thread_local! {
    /// A process singleton in the client, thread-local here
    /// for the same reason [`crate::env`] and [`crate::requests`] are.
    // ORDER-OK: a by-name registry; nothing iterates it for order.
    static REGISTRY: RefCell<BTreeMap<String, Variable>> = const { RefCell::new(BTreeMap::new()) };

    /// How many times a preference carrying the font-change callback has been written. See
    /// [`font_preference_epoch`].
    static FONT_PREFERENCE_EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The two preferences the client registers with the font-change callback.
///
/// Both are registered with five choices, the font callback, the choice-string list, the
/// preference name and the address of the client's own font variable — `UI.ChatFontFace` against
/// the font-face variable and `UI.ChatFontSize` against the font-size one. These are the only two
/// of the 34 attached preferences that carry that callback.
pub const FONT_PREFERENCE_NAMES: [&str; 2] = [
    crate::persist::preferences::keys::CHAT_FONT_FACE,
    crate::persist::preferences::keys::CHAT_FONT_SIZE,
];

/// The store's `if (callback) callback(name)`, as an edge a
/// screen can watch.
///
/// The client's callback is a synchronous call out of the store into the font-settings-changed
/// notice, which broadcasts to
/// every notice handler. This crate's store has no notice bus and no back-reference to a
/// screen, so what it publishes is the *fact of the write*: a counter every writer of either
/// [`FONT_PREFERENCE_NAMES`] bumps, which `crate::screens::gameplay::GamePlayScreen` compares
/// against the one it last answered. Same edge, one frame later at worst — and it catches the
/// three writers the notice catches, because all three go through [`set_value`]: a live menu
/// choice, *Cancel* (restoring the saved values) and *Restore Defaults*.
#[must_use]
pub fn font_preference_epoch() -> u64 {
    FONT_PREFERENCE_EPOCH.with(std::cell::Cell::get)
}

/// Bind a name to a variable holding `default`.
///
/// Returns `false` and changes nothing when the value is a `PString` (no attached preference is
/// one) or when the name is already registered, matching the registry's duplicate-key behavior.
pub fn register_preference(name: &str, default: PrefValue, data_type: DataType) -> bool {
    if !data_type.accepts(&default) {
        return false;
    }
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        if r.contains_key(name) {
            return false;
        }
        r.insert(
            name.to_string(),
            Variable {
                data_type,
                value: default,
            },
        );
        true
    })
}

/// Unregister a preference.
pub fn unregister_preference(name: &str) -> bool {
    REGISTRY.with(|r| r.borrow_mut().remove(name).is_some())
}

/// Whether a preference is registered at all, whatever its type.
#[must_use]
pub fn is_registered(name: &str) -> bool {
    REGISTRY.with(|r| r.borrow().contains_key(name))
}

/// The same query with a type, used by both hops of the typed query.
#[must_use]
pub fn is_registered_as(name: &str, want: DataType) -> bool {
    REGISTRY.with(|r| r.borrow().get(name).is_some_and(|v| v.data_type == want))
}

/// Read the variable the preference is bound to.
///
/// `None` for an unregistered name, which is the client's `false` return: the value reader's caller
/// must keep what it had rather than treat a failed read as a zero. (Retail's value reader really
/// does return the zero-initialised local in that case; keeping the current value instead is a
/// named divergence in the option page's value reader, because a rebuild whose registry is empty
/// must not silently zero every option on the page.)
#[must_use]
pub fn inq_value(name: &str) -> Option<PrefValue> {
    REGISTRY.with(|r| r.borrow().get(name).map(|v| v.value.clone()))
}

/// The typed form, which is what the three option-control value readers actually call.
///
/// Both of the typed query's hops compare the stored type against the requested [`DataType`], so a
/// read of the wrong type answers `false` — this returns `None` for it, and *not* a converted
/// value.
#[must_use]
pub fn inq_value_as(name: &str, want: DataType) -> Option<PrefValue> {
    REGISTRY.with(|r| {
        let r = r.borrow();
        let v = r.get(name)?;
        (v.data_type == want).then(|| v.value.clone())
    })
}

/// Preference updates pass through one entry point per type before reaching the stored variable.
///
/// The variable store's whole body is: if the stored type matches, store the value through the
/// variable pointer and call the change callback if there is one — so a type mismatch is a **silent no-op** in the client, and `false` here.
pub fn set_value(name: &str, v: PrefValue) -> bool {
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        let Some(slot) = r.get_mut(name) else {
            return false;
        };
        if !slot.data_type.accepts(&v) {
            return false;
        }
        slot.value = v;
        true
    }) && {
        // `if (callback) callback(name)` — the write happened, so the registered change callback
        // runs. Only the two font preferences have one.
        if FONT_PREFERENCE_NAMES.contains(&name) {
            FONT_PREFERENCE_EPOCH.with(|c| c.set(c.get().wrapping_add(1)));
        }
        true
    }
}

/// Register every preference [`super::preferences::UI_PREFERENCES`] names, with the value
/// preference registration gave it.
///
/// This stands in for the eight subsystem registrations that between them cover the 34 — the
/// sound manager, the renderer's start-up, the device's display-preference setup, the input
/// manager, the camera manager, the element manager and the client's own preference init. It
/// resets the registry, so a host that re-creates its
/// UI shell gets the registered defaults back rather than the previous shell's edits.
///
/// It also registers this client's two landscape options ([`super::landscape::register`]), which
/// are not retail's and are not in the count it returns.
pub fn init() -> usize {
    REGISTRY.with(|r| r.borrow_mut().clear());
    let mut n = 0;
    for p in super::preferences::UI_PREFERENCES {
        let (v, t) = match p.registered_default {
            super::config::PrefValueConst::Bool(b) => (PrefValue::Bool(b), DataType::Bool),
            super::config::PrefValueConst::Float(f) => (PrefValue::Float(f), DataType::Float),
            // Every `kind == 2` is the unsigned 32-bit type.
            super::config::PrefValueConst::Int(i) => (PrefValue::Int(i), DataType::UInt),
        };
        n += usize::from(register_preference(p.name, v, t));
    }
    super::landscape::register();
    n
}

/// Forget every registration. A test does this to prove the page falls back rather than crashes.
///
/// It takes the two run-time display lists with it: they belong to a device that
/// no longer has any registered variable to hang them on, and leaving them behind would make one
/// test's enumerated adapter visible to the next one on the same thread.
pub fn clear() {
    REGISTRY.with(|r| r.borrow_mut().clear());
    clear_display_choices();
}

/// How many variables are registered.
#[must_use]
pub fn len() -> usize {
    REGISTRY.with(|r| r.borrow().len())
}

/// The load's fifth step — push the parsed `UserPreferences.ini` into the
/// registered variables.
///
/// Returns `(applied, ignored)`: how many keys landed in a registered variable, and how many did
/// not. **Both**, because a file whose keys all miss and a file with no keys must not look alike.
///
/// A key the registry does not know is *not* an error — `Net.*`, `Input.KeymapFile` and the rest
/// of the 43 registered preferences are in the same file and are somebody else's — and neither is
/// a value that will not parse as the variable's type, which the client's global-variable setter
/// silently drops too.
///
/// The name comparison is **case-insensitive**: `Load` pushes every name through
/// through a case-folding registry, and `dereth_audio::Prefs::set_named`
/// already matches that way.
pub fn load(ini: &UserPreferences) -> (usize, usize) {
    let mut applied = 0;
    let mut ignored = 0;
    for (key, raw) in &ini.entries {
        let key = key.trim();
        let text = raw.trim();
        let found = REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let Some((name, slot)) = r.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) else {
                return false;
            };
            let parsed = match slot.data_type {
                DataType::Bool => parse_bool(text).map(PrefValue::Bool),
                DataType::Int | DataType::UInt => {
                    set_from_string_uint(name, text).map(PrefValue::Int)
                }
                DataType::Float => text.parse::<f32>().ok().map(PrefValue::Float),
            };
            match parsed {
                Some(v) => {
                    slot.value = v;
                    true
                }
                None => false,
            }
        });
        if found {
            applied += 1;
        } else {
            ignored += 1;
        }
    }
    (applied, ignored)
}

/// The value as the save writes it
/// into the `.ini`.
///
/// ```text
/// if the variable has choices:                // an enumeration writes its LABEL
///     i = the choice index for the value
///     if i is found and in range: write choices[i]; done
/// by type:
///   Bool:           "True" when the byte is 1, else "False"
///   Int32, UInt32:  "%i"
///   Float32:        "%.2f"
/// ```
///
/// Those are retail's four literals. `%.2f` and **not** `%f` is what
/// makes a saved profile look like the one a player already has, and `True`/`False` and not `1`/`0`
/// is what [`parse_bool`] has always had to accept.
///
/// **Named divergence, and it is the same one [`display_choice`] carries.**
/// `Display.Resolution` and `Display.RefreshRate` have choice lists in retail — built at run time
/// out of the adapter's modes — and this build has enumerated no adapter, so they take the numeric
/// arm and are written as `%i` (`67109632`) rather than as `1280x1024`. The value round-trips
/// exactly, because `set_from_string_uint`'s empty-choice-list arm is a plain integer parse;
/// what it costs is a line a human would recognise. It resolves itself the day an adapter is
/// enumerated.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> String {
    // This client's landscape options write one word per choice.
    if let Some(text) = super::landscape::convert_to_string(name, v) {
        return text;
    }
    // The two run-time lists are choice lists too, so once the device has
    // enumerated its modes this arm is reached for `Display.Resolution` and the file holds
    // `1280x720` rather than `83886800`. Before enumeration the list is empty and the numeric arm
    // below stands, which is the divergence this doc-comment's last paragraph describes.
    if let PrefValue::Int(x) = v {
        if let Some(row) = display_choices(name).into_iter().find(|c| c.value == *x) {
            return row.label;
        }
    }
    if let (Some(c), PrefValue::Int(x)) = (enum_choices(name), v) {
        if let Some(label) = c.label_for(*x) {
            return label.to_string();
        }
    }
    match v {
        PrefValue::Bool(true) => "True".to_string(),
        PrefValue::Bool(false) => "False".to_string(),
        PrefValue::Int(i) => format!("{i}"),
        PrefValue::Float(f) => format!("{f:.2}"),
        PrefValue::Text(s) => s.clone(),
    }
}

/// Write every registered preference into `ini`.
/// Without it, a setting the player changed would be lost at quit.
///
/// The choice lookup, with registry traversal and reference-count bookkeeping omitted:
///
/// ```text
/// path = the default preferences file;
/// for (v : global_registry)    // every variable object
///     if (v is a typed variable && v should be saved) list.push_back(v);
/// for (v : list) {
///     name = v's name;
///     i = last index of '.' in name;                             // a backwards scan
///     section = i < 1 ? "Default" : name.substring(0, i - 1);
///     key     = i < 1 ? name       : name.substring(i + 1);
///     value = v converted to a string;
///     WritePrivateProfileStringA(section, key, value, path);
/// }
/// ```
///
/// **The split is at the *last* dot and the fallback section is `Default`**, which is
/// [`crate::persist::preferences::SECTION`] — and no registered name in this build reaches it,
/// because all 43 contain a dot. The `%s.%s` the load re-joins them with is the same
/// split run backwards, which is what makes the round trip closed.
///
/// It is a **merge** and not a rewrite — see
/// the preference file's merge operation. Handing it the file
/// that was loaded at start-up is therefore the production call, and handing it a default
/// `UserPreferences` (which is what [`save`] does) is the first-run one.
///
/// Returns how many preferences were written.
pub fn save_into(ini: &mut UserPreferences) -> usize {
    // ORDER-OK: `Save` walks a hash table, so its order is unspecified; this walks the `BTreeMap`,
    // which groups a category's keys together and is therefore the *kinder* order for a file that
    // does not have that section yet.
    let rows: Vec<(String, String)> = REGISTRY.with(|r| {
        r.borrow()
            .iter()
            .map(|(name, var)| (name.clone(), convert_to_string(name, &var.value)))
            .collect()
    });
    for (name, value) in &rows {
        let (section, key) = match name.rsplit_once('.') {
            Some((s, k)) => (s, k),
            // `i < 1` — a name with no dot, or one that starts with it.
            None => (crate::persist::preferences::SECTION, name.as_str()),
        };
        ini.write_profile_string(section, key, value);
    }
    rows.len()
}

/// [`save_into`] over a fresh file — naming a path that does not
/// exist yet, which is a first run.
///
/// `crlf` is set because `fopen(path, "w")` is text mode on Windows, so a file the client wrote
/// has CRLF line endings and a re-save of it must not quietly convert them.
#[must_use]
pub fn save() -> UserPreferences {
    let mut ini = UserPreferences {
        crlf: true,
        ..UserPreferences::default()
    };
    save_into(&mut ini);
    ini
}

/// One registered option's paired choice labels and values.
///
/// **This is not [`super::preferences::UiPref::choices`].** That field is the eight compiled-in
/// lists of `ID_*` localisation tokens for the text the menu draws. This one is the **ASCII** list
/// the owning subsystem passes to preference registration, it lives on the variable rather than on
/// the UI item, and it is the list that decides what the INI holds: the value-to-string conversion
/// writes the matching label whenever the choice list is non-empty, and the string-to-value
/// conversion reads it back. The two lists are parallel and the same length, and confusing them
/// makes the drop-downs look like a UI problem.
#[derive(Debug, Clone, Copy)]
pub struct EnumChoices {
    /// The qualified preference name.
    pub name: &'static str,
    /// The choice labels, in registration order. Compared case-insensitively, which is
    /// case-**in**sensitive.
    pub labels: &'static [&'static str],
    /// The choice values. **Empty when the subsystem registered none**, and that is not the same as
    /// "the values are 0, 1, 2 …": the string-to-value conversion only maps index -> value when
    /// the two lists have the same length, so an absent array means the index *is* the
    /// value, and the two preferences whose array is present run backwards or in jumps.
    pub values: &'static [i32],
}

/// The five-step detail scale shared by the two texture-detail preferences.
const DETAIL_5: &[&str] = &["VeryLow", "Low", "Medium", "High", "VeryHigh"];

/// Every compiled-in choice list among the 34 attached preferences.
///
/// Sources, all retail's own lists rather than inferred:
///
/// | preference | choice labels | choice values |
/// |---|---|---|
/// | `Sound.SoundFeatures` | a compiled-in pair | none |
/// | `UI.ChatFontFace` | the chat-font-face choice table | none |
/// | `UI.ChatFontSize` | the chat-font-size choice table | none |
/// | `Render.TextureFiltering` | a compiled-in list | none |
/// | `Render.LandscapeTextureDetail` | a compiled-in list | `[4, 3, 2, 1, 0]` |
/// | `Render.EnvironmentTextureDetail` | a compiled-in list | `[4, 3, 2, 1, 0]` |
/// | `Render.SceneryDrawDistance` | a compiled-in list | none |
/// | `Render.LandscapeDrawDistance` | a compiled-in list | `[3, 5, 8, 11, 15, 25]` |
///
/// The three value arrays are the client's own.
///
/// **Nine lists exist in the client and only eight are here.** `Render.AspectRatio`'s
/// (`Auto`, `Normal`, `Wide`) is left out on purpose: the UI preference init never
/// attaches that preference, so nothing in this registry could ever consult the list and adding it
/// would be one more transcription with no caller.
///
/// `Display.Resolution` and `Display.RefreshRate` are the other two `kind == 2` preferences and
/// they are **not** here either, because their lists do not exist at compile time — see
/// [`display_choice`].
pub const ENUM_CHOICES: [EnumChoices; 8] = [
    EnumChoices {
        name: "Sound.SoundFeatures",
        labels: &["Stereo", "Mono"],
        values: &[],
    },
    EnumChoices {
        name: "UI.ChatFontFace",
        labels: &[
            "Arial",
            "CourierNew",
            "PalatinoLinotype",
            "Tahoma",
            "TimesNewRoman",
        ],
        values: &[],
    },
    EnumChoices {
        name: "UI.ChatFontSize",
        labels: &["Tiny", "Small", "Medium", "Large", "XL"],
        values: &[],
    },
    EnumChoices {
        name: "Render.TextureFiltering",
        labels: &["Bilinear", "Trilinear", "Sharp", "Anisotropic"],
        values: &[],
    },
    EnumChoices {
        name: "Render.LandscapeTextureDetail",
        labels: DETAIL_5,
        values: &[4, 3, 2, 1, 0],
    },
    EnumChoices {
        name: "Render.EnvironmentTextureDetail",
        labels: DETAIL_5,
        values: &[4, 3, 2, 1, 0],
    },
    EnumChoices {
        name: "Render.SceneryDrawDistance",
        labels: &["Low", "Medium", "High"],
        values: &[],
    },
    EnumChoices {
        name: "Render.LandscapeDrawDistance",
        labels: &["VeryLow", "Low", "Medium", "High", "VeryHigh", "Extreme"],
        values: &[3, 5, 8, 11, 15, 25],
    },
];

/// The choice list registered under `name`, if any. Case-insensitive on the name, as the registry
/// is.
#[must_use]
pub fn enum_choices(name: &str) -> Option<&'static EnumChoices> {
    ENUM_CHOICES
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name))
}

/// The choice values for one registered variable.
///
/// **The two failure modes are different answers and the caller branches on the second one.**
/// The gate is "found by name, a typed variable, and its stored type is
/// the unsigned 32-bit type", so an unregistered name, or one registered as anything but the
/// unsigned type, answers `false` — `None` here. A variable that *is* registered and whose
/// subsystem passed no value array answers `true` with an empty array — `Some(&[])` here — and the
/// menu control, on a failed query or an empty array, fills `0 .. choices.len()` in for it.
/// Collapsing the two into one `None` would put the six menus that have no value array on the same
/// branch as a preference that is not an enumeration at all.
#[must_use]
pub fn inq_choice_values(name: &str) -> Option<&'static [i32]> {
    if !is_registered_as(name, DataType::UInt) {
        return None;
    }
    Some(enum_choices(name).map_or(&[][..], |c| c.values))
}

/// The choice labels, behind the same
/// unsigned-32-bit type gate as [`inq_choice_values`].
///
/// `Some(&[])` for `Display.Resolution` and `Display.RefreshRate`: both are registered, and their
/// list is the one the device's display-preference setup builds out of the adapter's
/// own modes, which this build has not enumerated. See [`display_choice`].
#[must_use]
pub fn inq_choice_strings(name: &str) -> Option<&'static [&'static str]> {
    if !is_registered_as(name, DataType::UInt) {
        return None;
    }
    Some(enum_choices(name).map_or(&[][..], |c| c.labels))
}

impl EnumChoices {
    /// The string-to-value conversion, the non-empty-choice-list arm.
    ///
    /// > ```text
    /// > i = the first label equal to s, case-insensitively
    /// > if none matched: i = strtol(s, 0, 0)
    /// > value = the lists have the same length ? values[i] : i
    /// > store value
    /// > ```
    ///
    /// Two consequences that are easy to get backwards and are asserted in this module's tests:
    ///
    /// 1. **A number in the file is an *index*, not a value.** `LandscapeDrawDistance=3` is the
    ///    fourth choice `High` and lands as **11**; the value 3 is what `VeryLow` lands as. Retail
    ///    only ever writes the label, so this arm is reached by a hand-edited file.
    /// 2. **An index that is negative or past the end selects 0**, per the conversion's own bounds
    ///    check — the same reading `dereth_client::render_prefs::filtering_choice`
    ///    already took for `Render.TextureFiltering`.
    ///
    /// Resolve a stored value back to its choice index, the half
    /// the value-to-string conversion needs.
    ///
    /// With no choice values the value **is** the index, bounds-checked; with one, it is the
    /// position of the value in that array. `None` is the client's `-1`, which falls through to
    /// the numeric arm rather than writing a wrong label.
    #[must_use]
    pub fn label_for(&self, value: i32) -> Option<&'static str> {
        let idx = if self.values.len() == self.labels.len() {
            self.values.iter().position(|v| *v == value)?
        } else {
            usize::try_from(value).ok()?
        };
        self.labels.get(idx).copied()
    }

    #[must_use]
    pub fn resolve(&self, text: &str) -> i32 {
        let text = text.trim();
        let idx = self
            .labels
            .iter()
            .position(|l| l.eq_ignore_ascii_case(text))
            .or_else(|| {
                let n = parse_int(text)?;
                (n >= 0 && (n as usize) < self.labels.len()).then_some(n as usize)
            })
            .unwrap_or(0);
        if self.values.len() == self.labels.len() {
            self.values[idx]
        } else {
            i32::try_from(idx).unwrap_or(0)
        }
    }
}

/// The two `kind == 2` preferences whose choice list the device's display-preference setup
/// builds **at run time** from the enumerated adapter modes, so there is no table to
/// transcribe — only the two label formats, which are string constants in that function.
///
/// | preference | label | choice value |
/// |---|---|---|
/// | `Display.Resolution` | `"%ix%i"` — `1024x768` | `width << 16 \| height` |
/// | `Display.RefreshRate` | `"%ihz"`, and `Auto` for 0 | the rate |
///
/// **Named divergence.** Retail resolves these through the same choice-list search as the other
/// eight, over a list holding only the modes *this adapter* reports; a mode the adapter does not
/// offer therefore falls through to `strtol` and is read as an index. This build has not enumerated
/// any adapter by the time the profile is loaded, so it decodes the label's own arithmetic instead.
/// The end value is the same for every mode the adapter does offer, and for one it does not the
/// divergence is in the safe direction: retail would pick an unrelated mode by index, this keeps
/// the width and height the file asked for. A value that is not in either shape falls through to
/// `parse_int`, which is the empty-choice-list arm and is what this build's empty list
/// literally is.
#[must_use]
pub fn display_choice(name: &str, text: &str) -> Option<i32> {
    let text = text.trim();
    if name.eq_ignore_ascii_case("Display.Resolution") {
        let (w, h) = text.split_once(['x', 'X'])?;
        let w: u32 = w.trim().parse().ok()?;
        let h: u32 = h.trim().parse().ok()?;
        return Some(i32::from_ne_bytes((w << 16 | h).to_ne_bytes()));
    }
    if name.eq_ignore_ascii_case("Display.RefreshRate") {
        if text.eq_ignore_ascii_case("Auto") {
            return Some(0);
        }
        let n = text
            .strip_suffix("hz")
            .or_else(|| text.strip_suffix("Hz"))?;
        return n.trim().parse().ok();
    }
    None
}

/// Parse a global variable for the unsigned and signed integer arms, which between them are every
/// `kind == 2` preference.
///
/// The order is the client's: the choice list first — when there is one it is **authoritative**,
/// and the integer parse below is reachable only as the string-to-value conversion's own
/// no-label-matched fallback, inside [`EnumChoices::resolve`] — then the two runtime-built display
/// lists, then a plain integer, which is the empty-choice-list arm.
fn set_from_string_uint(name: &str, text: &str) -> Option<i32> {
    // This client's landscape options read their words, their captions and the older spellings.
    if super::landscape::Landscape::of(name).is_some() {
        return super::landscape::parse_value(name, text);
    }
    if let Some(c) = enum_choices(name) {
        return Some(c.resolve(text));
    }
    // Choice labels first for the two run-time lists as well — the string-to-value conversion
    // searches the labels before it reaches `strtol`, and after `initialize_display_preferences`
    // these two have labels. `display_choice` below is still the fall-back and still decodes a
    // `WxH` the adapter does not offer, which is the named divergence on that function.
    let t = text.trim();
    if let Some(row) = display_choices(name)
        .into_iter()
        .find(|c| c.label.eq_ignore_ascii_case(t))
    {
        return Some(row.value);
    }
    display_choice(name, text).or_else(|| parse_int(text))
}

// =================================================================================================
// The device's display-preference setup — the two run-time choice lists.
// =================================================================================================

/// The two preference names whose choice labels and values are built at run time.
pub const DISPLAY_RESOLUTION: &str = "Display.Resolution";
/// As [`DISPLAY_RESOLUTION`].
pub const DISPLAY_REFRESH_RATE: &str = "Display.RefreshRate";

/// One entry in the display-mode array that display-preference setup walks.
///
/// The four fields are the ones the walk reads: width, height, format and refresh rate.
///
/// The format is a pixel-format id there and only its **depth** is consulted
/// (a depth of at least 32 bits), so this
/// carries the depth directly rather than a format enum this crate has no table for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    /// The refresh rate. `0` is the adapter's "unspecified", which becomes the `Auto` row.
    pub refresh_rate: u32,
    /// The format's bits per pixel.
    pub bits_per_pixel: u32,
}

/// One row of a run-time choice list: the formatted label and the
/// choice value beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub value: i32,
}

/// The display-preference setup's minimum mode width.
pub const MIN_MODE_WIDTH: u32 = 800;
/// As [`MIN_MODE_WIDTH`].
pub const MIN_MODE_HEIGHT: u32 = 600;
/// The minimum colour depth a mode must offer to be listed.
pub const MIN_MODE_DEPTH: u32 = 32;
/// The mode the walk appends unconditionally, and whose packed descriptor becomes the display
/// preferences' resolution.
pub const FALLBACK_MODE: (u32, u32) = (1024, 768);

thread_local! {
    /// The choice labels and values the variable registration is handed for
    /// `Display.Resolution` and `Display.RefreshRate`.
    ///
    /// Separate from [`REGISTRY`] because in the client they live on the variable object and are
    /// installed by the **device**, not by the UI: the device's own init runs
    /// the display-preference setup long before the client attaches any UI preference.
    /// [`init`] therefore does not clear them — a host that rebuilds its UI
    /// shell has not re-enumerated its adapter.
    // ORDER-OK: a by-name map; each list keeps its own order, which is the sorted one.
    static DISPLAY_CHOICES: RefCell<BTreeMap<String, Vec<Choice>>> =
        const { RefCell::new(BTreeMap::new()) };
}

/// The display-preference setup, whole, minus the four
/// variable registrations [`init`] already makes.
///
/// ```text
/// for i in 0..=mode_count:                             ; note the `<=`
///     if i < mode_count:
///         skip a mode under 800 wide, under 600 high, or under 32 bits per pixel
///         (w, h, r) = that mode's width, height and refresh rate
///     else: (w, h, r) = (1024, 768, 0)                  ; ALWAYS appended
///     size = w * h; desc = w << 16 | h
///     if no kept mode has this (w, h): keep it          ; linear scan
///     add r to the rates if absent                      ; even for a duplicate mode
/// sort the modes by size; sort the rates
/// for each mode: label "%ix%i", value desc; the 1024x768 one is the default
/// for each rate: label "%ihz", or "Auto" for 0; value the rate
/// display prefs: full screen, resolution = the default, refresh rate 0
/// ```
///
/// Three details of retail's walk, each load-bearing:
///
/// 1. **The loop runs one extra time.** The loop bound is the mode count plus one, so the final iteration takes the `else` and
///    appends **1024x768 at refresh 0** whatever the adapter reported. It is
///    subject to the same `(w, h)` dedupe, and it is the reason the default resolution is always
///    findable. A machine with no adapter modes at all therefore still gets a one-row list.
/// 2. **The dedupe is on `(width, height)` only** —
///    so a 60 Hz and a 144 Hz 1920x1080 collapse into one row while **both** refresh rates are
///    kept: the rate list's add-unique runs on the duplicate path too.
/// 3. **The sort key is the area, `width * height`**, not the width and not the packed
///    descriptor. Ties keep their enumeration order here; retail's sort is not
///    stable, but the client only reaches it above sixteen entries and no pair of real modes with
///    equal area has ever been observed to differ in the shipped lists.
///
/// Returns the display preferences' resolution — the packed descriptor of the 1024x768 row, which by (1) is
/// always [`FALLBACK_MODE`] packed.
pub fn initialize_display_preferences(modes: &[DisplayMode]) -> u32 {
    // The kept modes and the refresh rates.
    let mut kept: Vec<(u32, u32)> = Vec::new();
    let mut rates: Vec<u32> = Vec::new();
    for i in 0..=modes.len() {
        let (w, h, r) = match modes.get(i) {
            Some(m) => {
                if m.width < MIN_MODE_WIDTH
                    || m.height < MIN_MODE_HEIGHT
                    || m.bits_per_pixel < MIN_MODE_DEPTH
                {
                    continue;
                }
                (m.width, m.height, m.refresh_rate)
            }
            // The appended fallback. Reached exactly once, at `i == modes.len()`.
            None => (FALLBACK_MODE.0, FALLBACK_MODE.1, 0),
        };
        if !kept.iter().any(|(kw, kh)| *kw == w && *kh == h) {
            kept.push((w, h));
        }
        // The rate is added if new, whether or not the mode itself was a duplicate.
        if !rates.contains(&r) {
            rates.push(r);
        }
    }
    // ORDER-OK: retail sorts the modes by area.
    kept.sort_by_key(|(w, h)| w.saturating_mul(*h));
    rates.sort_unstable();

    let resolutions: Vec<Choice> = kept
        .iter()
        .map(|(w, h)| Choice {
            label: format!("{w}x{h}"),
            value: mode_desc(*w, *h),
        })
        .collect();
    let refresh: Vec<Choice> = rates
        .iter()
        .map(|r| Choice {
            label: if *r == 0 {
                "Auto".to_string()
            } else {
                format!("{r}hz")
            },
            value: i32::try_from(*r).unwrap_or(i32::MAX),
        })
        .collect();
    DISPLAY_CHOICES.with(|c| {
        let mut c = c.borrow_mut();
        c.insert(DISPLAY_RESOLUTION.to_string(), resolutions);
        c.insert(DISPLAY_REFRESH_RATE.to_string(), refresh);
    });
    // The client's own rule: the 1024x768 row's descriptor is the default, seeded to
    // `0xFFFFFFFF`. (1) makes the seed unreachable, and the assertion says so rather than hiding it.
    let packed = mode_desc(FALLBACK_MODE.0, FALLBACK_MODE.1);
    u32::from_ne_bytes(packed.to_ne_bytes())
}

/// The packed descriptor, `width << 16 | height` — a shift and an or, as the `i32`
/// this crate's [`PrefValue::Int`] carries.
#[must_use]
pub fn mode_desc(width: u32, height: u32) -> i32 {
    i32::from_ne_bytes((width << 16 | (height & 0xFFFF)).to_ne_bytes())
}

/// Split a packed display-mode description back into the stored resolution's width and height.
#[must_use]
pub const fn mode_size(desc: i32) -> (u32, u32) {
    let d = desc as u32;
    (d >> 16, d & 0xFFFF)
}

/// Forget both run-time lists. The device going away, and what a test does between cases.
pub fn clear_display_choices() {
    DISPLAY_CHOICES.with(|c| c.borrow_mut().clear());
}

/// The run-time choice labels and values for `name`, empty for every other preference.
#[must_use]
pub fn display_choices(name: &str) -> Vec<Choice> {
    DISPLAY_CHOICES.with(|c| {
        c.borrow()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    })
}

/// The choice strings and values together, as the owned pair a run-time list needs.
///
/// The `&'static` [`inq_choice_strings`] and [`inq_choice_values`] answer the eight compiled-in
/// lists and are unchanged; this is the form the menu control's user-preference setup needs,
/// because the two display lists are owned strings formatted at device initialization and cannot be
/// `&'static str`. `None` keeps the choice queries' "not registered, or not the unsigned 32-bit
/// type" answer, which is a different thing from a registered preference with an empty list.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<Choice>> {
    if !is_registered_as(name, DataType::UInt) {
        return None;
    }
    // This client's landscape options list literal captions too.
    if let Some(rows) = super::landscape::choice_rows(name) {
        return Some(rows);
    }
    let runtime = display_choices(name);
    if !runtime.is_empty() {
        return Some(runtime);
    }
    // Registered, and the subsystem passed no list: the choice-values query answers `true` with
    // an empty array, which is a different thing from the `false` above. Filling the literal entries then
    // runs zero times — a drop-down with a popup and no rows, which is exactly what
    // `Display.Resolution` is before a device has enumerated anything.
    let Some(c) = enum_choices(name) else {
        return Some(Vec::new());
    };
    let values: Vec<i32> = if c.values.len() == c.labels.len() {
        c.values.to_vec()
    } else {
        (0..c.labels.len())
            .map(|k| i32::try_from(k).unwrap_or(0))
            .collect()
    };
    Some(
        c.labels
            .iter()
            .zip(values)
            .map(|(l, value)| Choice {
                label: (*l).to_string(),
                value,
            })
            .collect(),
    )
}

/// `KW_TRUE` / `KW_FALSE` are the literals `"True"` and `"False"`, and the numeric spelling is
/// accepted too — the same way `dereth_audio::Prefs::from_ini` and
/// `dereth_client::config::Preferences::bool` resolve it.
///
/// **The write side is settled**: the value-to-string conversion's boolean
/// arm writes "False" unless the stored byte is 1, and "True" otherwise, from two string
/// constants, so `True` / `False` **is** what the client writes. The
/// read side still accepts both spellings, which costs nothing.
fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "1" | "true" | "True" | "TRUE" => Some(true),
        "0" | "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

/// An unsigned decimal, or `0x`-prefixed hex — `strtol(s, 0, 0)`, less its octal arm, which no
/// value in this file format can reach. This is the string-to-value conversion's fallback, not the
/// normal path: every `kind == 2` preference is written as a choice label, and `%.2f` / `True` /
/// `False` cover the rest.
fn parse_int(s: &str) -> Option<i32> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok().map(|v| v as i32);
    }
    s.parse::<i32>()
        .ok()
        .or_else(|| s.parse::<u32>().ok().map(|v| v as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A write occurs only when the requested type matches the variable's stored type, and both
    /// query hops apply the same guard.
    ///
    /// The type check is asserted in **both** directions: the right type round-trips, the wrong
    /// type reads `None` and writes nothing. A store that converted instead would make a bool
    /// preference settable from a slider, which is a defect no screenshot could show.
    #[test]
    fn a_variable_is_read_and_written_only_through_its_own_type() {
        clear();
        assert!(register_preference(
            "Sound.SoundVolume",
            PrefValue::Float(1.0),
            DataType::Float
        ));
        assert!(register_preference(
            "Camera.AlignToSlope",
            PrefValue::Bool(true),
            DataType::Bool
        ));
        assert!(register_preference(
            "UI.ChatFontSize",
            PrefValue::Int(1),
            DataType::UInt
        ));

        assert_eq!(
            inq_value_as("Sound.SoundVolume", DataType::Float),
            Some(PrefValue::Float(1.0))
        );
        assert_eq!(
            inq_value_as("Sound.SoundVolume", DataType::Bool),
            None,
            "wrong data type"
        );
        assert!(
            !set_value("Sound.SoundVolume", PrefValue::Bool(false)),
            "wrong type writes nothing"
        );
        assert_eq!(inq_value("Sound.SoundVolume"), Some(PrefValue::Float(1.0)));
        assert!(set_value("Sound.SoundVolume", PrefValue::Float(0.25)));
        assert_eq!(inq_value("Sound.SoundVolume"), Some(PrefValue::Float(0.25)));

        assert_eq!(
            inq_value_as("UI.ChatFontSize", DataType::UInt),
            Some(PrefValue::Int(1))
        );
        assert_eq!(
            inq_value_as("UI.ChatFontSize", DataType::Int),
            None,
            "UInt32 is not Int32"
        );

        // An unregistered name is a `false` return, not a default and not a panic.
        assert_eq!(inq_value("Net.BindInterface"), None);
        assert!(!set_value("Net.BindInterface", PrefValue::Bool(true)));
        assert!(!is_registered("Net.BindInterface"));
        // A duplicate registration is refused and leaves the first value alone.
        assert!(!register_preference(
            "Sound.SoundVolume",
            PrefValue::Float(0.9),
            DataType::Float
        ));
        assert_eq!(inq_value("Sound.SoundVolume"), Some(PrefValue::Float(0.25)));
        assert!(unregister_preference("Sound.SoundVolume"));
        assert!(!is_registered("Sound.SoundVolume"));
    }

    /// Oracle: step 5, and §5.4's default column.
    ///
    /// Both counts are asserted, per *"assert the denominator"*: an INI whose keys all miss must
    /// not look like an INI that was applied.
    ///
    /// The fixture is in the shape the client writes — **a section per category, bare keys
    /// inside it**. A `[Default]` section with already-dotted keys is a shape the client cannot
    /// produce, and it would pass over a parser that dropped the section entirely (building the
    /// bare `SoundVolume`), because the fixture would hand it the qualified name ready-made.
    /// Against a real file such a parser matches nothing at all.
    #[test]
    fn loading_a_preferences_file_overwrites_only_the_registered_names() {
        assert_eq!(init(), 34, "the 34 attached preferences all register");
        // ...beside this client's two landscape options.
        assert_eq!(len(), 36);
        // The registration defaults are in force before any file is read.
        assert_eq!(
            inq_value("Input.MouseLookSensitivity"),
            Some(PrefValue::Float(0.25))
        );
        assert_eq!(
            inq_value("Render.AutomaticDegrades"),
            Some(PrefValue::Bool(true))
        );
        assert_eq!(
            inq_value("Display.Resolution"),
            Some(PrefValue::Int(0x0400_0300))
        );

        let text = "[Sound]\r\nSoundVolume=0.50\r\nSoundVolume2=1\r\n\
                    [Camera]\r\nAlignToSlope=False\r\n\
                    [Display]\r\nResolution=52429400\r\n\
                    [UI]\r\nChatFontSize=3\r\n\
                    [Net]\r\nBindInterface=\r\n[Input]\r\nKeymapFile=\r\n";
        let ini = UserPreferences::parse(text).expect("the ini parses");
        let (applied, ignored) = load(&ini);
        assert_eq!(applied, 4, "four keys name a registered variable");
        assert_eq!(
            ignored, 3,
            "Net.BindInterface, Input.KeymapFile and the typo'd name"
        );
        assert_eq!(inq_value("Sound.SoundVolume"), Some(PrefValue::Float(0.5)));
        assert_eq!(
            inq_value("Camera.AlignToSlope"),
            Some(PrefValue::Bool(false))
        );
        assert_eq!(inq_value("UI.ChatFontSize"), Some(PrefValue::Int(3)));
        // 52429400 == 0x03200258 == 800 << 16 | 600, the decimal spelling writes
        // when no choice list is registered to give the value a label.
        assert_eq!(
            inq_value("Display.Resolution"),
            Some(PrefValue::Int(0x0320_0258))
        );
        assert_eq!(0x0320_0258 >> 16, 800);
        // Untouched keys keep the registration default.
        assert_eq!(
            inq_value("Input.MouseLookSensitivity"),
            Some(PrefValue::Float(0.25))
        );

        // Case-insensitively, as the registry does — on the **section** as
        // well as on the key, because the name it lower-cases is the qualified one.
        let ini = UserPreferences::parse("[sound]\r\nsoundvolume=0.125\r\n").unwrap();
        assert_eq!(load(&ini), (1, 0));
        assert_eq!(
            inq_value("Sound.SoundVolume"),
            Some(PrefValue::Float(0.125))
        );

        // A value that will not parse as the variable's type is dropped, not coerced.
        let ini = UserPreferences::parse("[Sound]\r\nSoundVolume=loud\r\n").unwrap();
        assert_eq!(load(&ini), (0, 1));
        assert_eq!(
            inq_value("Sound.SoundVolume"),
            Some(PrefValue::Float(0.125))
        );
        clear();
    }

    /// The polarity trap, asserted here as well as in `dereth_audio`: `Sound.SoundDisabled=True`
    /// means sound is **on**, and this store must not invert it on the way through.
    #[test]
    fn the_disabled_names_are_not_inverted_by_the_store() {
        assert_eq!(init(), 34);
        assert_eq!(
            inq_value("Sound.SoundDisabled"),
            Some(PrefValue::Bool(true))
        );
        let ini = UserPreferences::parse("[Sound]\r\nSoundDisabled=False\r\n").unwrap();
        assert_eq!(load(&ini), (1, 0));
        assert_eq!(
            inq_value("Sound.SoundDisabled"),
            Some(PrefValue::Bool(false)),
            "False in the file must arrive as False"
        );
        clear();
    }
}
