//! A retail-shaped UserPreferences.ini fixture: 43 keys present, 34 match registered names, 34
//! apply; bare keys and [Default]-dotted keys match nothing; the 9 unmatched are the 9 with no
//! option row; enum labels/indices resolve case-insensitively.
//! Fixture: synthetic views and UI state; the catalogue check also reads production source files.

use dereth_ui::persist::preferences::UserPreferences;
use dereth_ui_screens::options::preferences::UI_PREFERENCES;
use dereth_ui_screens::options::store;
use dereth_ui_screens::PrefValue;

/// A `UserPreferences.ini` in the original client's shape: a section per category, named
/// by everything before the registered name's **last** dot, and the bare key after it.
const RETAIL_SHAPED_INI: &str = "[Net]\r\n\
     BindInterface=\r\n\
     ComputeUniquePort=True\r\n\
     UserSpecifiedPort=9042\r\n\
     [Input]\r\n\
     KeymapFile=acclient.keymap\r\n\
     MouseLookSensitivity=0.55\r\n\
     MouseLookSmoothingAmount=0.10\r\n\
     InvertMouseLookYAxis=True\r\n\
     UseMouseTurning=True\r\n\
     [UI]\r\n\
     ChatFontFace=Tahoma\r\n\
     ChatFontSize=Large\r\n\
     [International]\r\n\
     UseIME=True\r\n\
     [Misc]\r\n\
     TooltipDelay=0.75\r\n\
     TooltipEnable=False\r\n\
     [Render]\r\n\
     TextureFiltering=Anisotropic\r\n\
     LandscapeDetailTextures=True\r\n\
     BuildingDetailTextures=False\r\n\
     MultiPassAlpha=True\r\n\
     LandscapeTextureDetail=VeryHigh\r\n\
     EnvironmentTextureDetail=VeryLow\r\n\
     SceneryDrawDistance=High\r\n\
     LandscapeDrawDistance=Extreme\r\n\
     ScreenBrightness=0.20\r\n\
     AspectRatio=Wide\r\n\
     FieldOfView=120.00\r\n\
     AutomaticDegrades=False\r\n\
     GraphicsPerformance=0.30\r\n\
     DegradeDistance=75.00\r\n\
     DisplayAdapter=0\r\n\
     [Sound]\r\n\
     SoundVolume=0.80\r\n\
     AmbientSoundVolume=0.60\r\n\
     InterfaceSoundVolume=0.40\r\n\
     SoundFeatures=Mono\r\n\
     SoundDisabled=False\r\n\
     AmbientSoundDisabled=False\r\n\
     InterfaceSoundDisabled=False\r\n\
     PlaySoundOnlyWhenActive=False\r\n\
     [Display]\r\n\
     Resolution=1280x1024\r\n\
     FullScreen=False\r\n\
     RefreshRate=75hz\r\n\
     SyncToRefresh=True\r\n\
     [Camera]\r\n\
     AlignToSlope=False\r\n\
     Stiffness=0.80\r\n\
     AdjustmentSpeed=60.00\r\n";

/// Nine of the 43 registered preferences do
/// **not** attach to the options UI, so the registry has no variable for them and `load` must ignore them
/// without complaint. `Render.DisplayAdapter` is the odd one —
/// the original initialization registers it and then unregisters it, so it is read from the INI
/// and never written back.
const THE_NINE_WITHOUT_A_UI_ROW: [&str; 9] = [
    "Input.KeymapFile",
    "Input.MouseLookSmoothingAmount",
    "International.UseIME",
    "Net.BindInterface",
    "Net.ComputeUniquePort",
    "Net.UserSpecifiedPort",
    "Render.AspectRatio",
    "Render.DisplayAdapter",
    "Render.LandscapeDetailTextures",
];

/// **The number.** How many of the 34 matched keys carry a value the store can take.
///
/// `0` with the bare-key parser, `24` once the section is restored, `34` once enum labels convert.
/// It is a constant so that a regression reads as this number going *down* rather than as a suite
/// going red for an unrelated reason.
const EXPECT_APPLIED: usize = 34;

/// The matched keys that still do not apply. **Empty**, and it held the ten `kind == 2`
/// preferences — the drop-downs — for exactly one commit: the original client writes an
/// enumeration as its choice **label** (it takes the choice branch whenever the choice list is
/// non-empty) and an integer parser cannot read `Anisotropic`.
const EXPECT_NOT_APPLIED: &[&str] = &[];

/// `(present, matched, applied)` over one file, plus the matched names whose variable did not move.
///
/// "Did not move" is a sound proxy for "was not applied" **only** because every value in
/// [`RETAIL_SHAPED_INI`] differs from that preference's `registered_default`, which
/// [`each_of_the_matched_keys_moves_off_its_registered_default`] asserts directly.
fn census(text: &str) -> (usize, usize, usize, Vec<String>) {
    assert_eq!(
        store::init(),
        34,
        "the 34 attached preferences all register"
    );
    let ini = UserPreferences::parse(text).expect("the ini parses");
    let present = ini.entries.len();
    let matched: Vec<String> = ini
        .entries
        .iter()
        .map(|(k, _)| k.trim().to_string())
        .filter(|k| store::is_registered(k))
        .collect();
    let before: Vec<Option<PrefValue>> = matched.iter().map(|k| store::inq_value(k)).collect();
    let (applied, _ignored) = store::load(&ini);
    let missed: Vec<String> = matched
        .iter()
        .zip(&before)
        .filter(|(k, was)| store::inq_value(k).as_ref() == was.as_ref())
        .map(|(k, _)| k.clone())
        .collect();
    eprintln!(
        "Preference coverage: {present} present, {} matched, {applied} applied; not applied: {missed:?}",
        matched.len()
    );
    (present, matched.len(), applied, missed)
}

/// Behaviour: options.preferences-file.a-retail-shaped-file-applies-every-key-the-options-page-attaches
/// The census of a retail shaped profile.
#[test]
fn the_census_of_a_retail_shaped_profile() {
    let (present, matched, applied, missed) = census(RETAIL_SHAPED_INI);
    assert_eq!(present, 43, "the fixture is every registered preference");
    assert_eq!(matched, 34, "and 34 of them have a row on an option page");
    assert_eq!(
        missed, EXPECT_NOT_APPLIED,
        "{present} present, {matched} matched, {applied} applied"
    );
    assert_eq!(
        applied, EXPECT_APPLIED,
        "{present} present, {matched} matched, {applied} applied"
    );
}

/// The bare key as written in the file is not a registry name.
#[test]
fn the_bare_key_as_written_in_the_file_is_not_a_registry_name() {
    assert_eq!(store::init(), 34);
    let ini = UserPreferences::parse(RETAIL_SHAPED_INI).expect("parses");
    let bare: Vec<&str> = ini
        .entries
        .iter()
        .filter_map(|(k, _)| k.rsplit_once('.').map(|(_, b)| b))
        .collect();
    assert_eq!(
        bare.len(),
        43,
        "every qualified name has a section to strip back off"
    );
    for b in &bare {
        assert!(
            !store::is_registered(b),
            "{b} must not resolve without its section"
        );
    }
}

/// The nine that do not match are exactly the nine never attached to an options row, and the two
/// sets partition the 43 with nothing left over. A key the registry does not know is **not** an
/// error — it belongs to networking, input methods, input management, or rendering, all of which
/// read the same file — so `load`'s second return is a count and not a failure.
#[test]
fn the_nine_that_do_not_match_are_the_nine_with_no_option_row() {
    assert_eq!(store::init(), 34);
    let ini = UserPreferences::parse(RETAIL_SHAPED_INI).expect("parses");
    let mut unmatched: Vec<&str> = ini
        .entries
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !store::is_registered(k))
        .collect();
    unmatched.sort_unstable();
    assert_eq!(
        unmatched, THE_NINE_WITHOUT_A_UI_ROW,
        "43 = 34 with a row + 9 without"
    );
    let (_applied, ignored) = store::load(&ini);
    assert_eq!(
        ignored,
        9 + EXPECT_NOT_APPLIED.len(),
        "`load`'s second return conflates `somebody else owns this` with `this would not parse`"
    );
    for name in THE_NINE_WITHOUT_A_UI_ROW {
        assert!(
            !store::is_registered(name),
            "{name} must not be in the options registry"
        );
        assert_eq!(store::inq_value(name), None);
    }
}

/// Every value in the fixture differs from that preference's `registered_default`, so the headline
/// cannot pass by agreement, and every key the census counts as applied really did move.
#[test]
fn each_of_the_matched_keys_moves_off_its_registered_default() {
    assert_eq!(store::init(), 34);
    let defaults: Vec<(&str, PrefValue)> = UI_PREFERENCES
        .iter()
        .map(|p| (p.name, store::inq_value(p.name).expect("registered")))
        .collect();
    let ini = UserPreferences::parse(RETAIL_SHAPED_INI).expect("parses");
    store::load(&ini);
    for (name, was) in &defaults {
        let now = store::inq_value(name).expect("still registered");
        if EXPECT_NOT_APPLIED.contains(name) {
            assert_eq!(
                &now, was,
                "{name} is on the not-yet-applied list and must not have moved"
            );
        } else {
            assert_ne!(
                &now, was,
                "{name} still holds its registered default {was:?}; the file was not applied to it"
            );
        }
    }
    // One of each of the other two kinds, to show a float and a bool both survive the trip.
    assert_eq!(
        store::inq_value("Render.FieldOfView"),
        Some(PrefValue::Float(120.0))
    );
    assert_eq!(
        store::inq_value("Camera.Stiffness"),
        Some(PrefValue::Float(0.8))
    );
    assert_eq!(
        store::inq_value("Display.FullScreen"),
        Some(PrefValue::Bool(false))
    );
    assert_eq!(
        store::inq_value("Misc.TooltipEnable"),
        Some(PrefValue::Bool(false))
    );
}

/// **The polarity, verified rather than assumed.** `Sound.SoundDisabled` controls whether
/// effect sounds are enabled. Its registered default is `true`, and the ambient path
/// plays when the variable is true — so the name is backwards in retail's own file format and
/// `SoundDisabled=True` means sound is **playing**.
///
/// The store must carry the value through **uninverted**; inverting it here to make the label read
/// the way a human expects would put the page one negation away from the subsystem that owns the
/// variable. The fixture says `False` for all three, which is the unusual case (silent), and that
/// is what the page must then show.
#[test]
fn the_disabled_names_pass_through_the_loader_uninverted() {
    assert_eq!(store::init(), 34);
    let three = [
        "Sound.SoundDisabled",
        "Sound.AmbientSoundDisabled",
        "Sound.InterfaceSoundDisabled",
    ];
    for name in three {
        assert_eq!(
            store::inq_value(name),
            Some(PrefValue::Bool(true)),
            "{name} is registered enabled — the compiled-in default is sound ON"
        );
    }
    let ini = UserPreferences::parse(RETAIL_SHAPED_INI).expect("parses");
    store::load(&ini);
    for name in three {
        assert_eq!(
            store::inq_value(name),
            Some(PrefValue::Bool(false)),
            "{name}=False in the file must arrive as False, not as the inverse of False"
        );
    }
    // A file that says `True` on all three is sound ON, and a page showing the boxes ticked agrees
    // with it. That configuration is not a config-page defect.
    let ini = UserPreferences::parse(
        "[Sound]\r\nSoundDisabled=True\r\nAmbientSoundDisabled=True\r\nInterfaceSoundDisabled=True\r\n",
    )
    .expect("parses");
    assert_eq!(store::load(&ini), (3, 0));
    for name in three {
        assert_eq!(store::inq_value(name), Some(PrefValue::Bool(true)));
    }
}

/// The instrument must be able to report the defect it was built for. A file in the shape three of
/// this workspace's own fixtures used — `[Default]` with already-dotted keys — matches **nothing**,
/// because `Load` qualifies unconditionally and the names become `Default.Sound.SoundVolume`.
#[test]
fn a_default_section_full_of_dotted_keys_is_the_shape_that_matches_nothing() {
    assert_eq!(store::init(), 34);
    let text = "[Default]\r\nSound.SoundVolume=0.375\r\nRender.FieldOfView=120.00\r\n";
    let ini = UserPreferences::parse(text).expect("parses");
    assert_eq!(ini.entries[0].0, "Default.Sound.SoundVolume");
    assert_eq!(
        store::load(&ini),
        (0, 2),
        "no key in this shape names a registered variable"
    );
    assert_eq!(
        store::inq_value("Sound.SoundVolume"),
        Some(PrefValue::Float(1.0)),
        "the default"
    );
}

/// Each drop down resolves its label to the registered choice value.
#[test]
fn each_drop_down_resolves_its_label_to_the_registered_choice_value() {
    assert_eq!(store::init(), 34);
    let enums: Vec<&str> = UI_PREFERENCES
        .iter()
        .filter(|p| p.kind == 2)
        .map(|p| p.name)
        .collect();
    assert_eq!(enums.len(), 10, "ten `kind == 2` preferences: {enums:?}");

    let ini = UserPreferences::parse(RETAIL_SHAPED_INI).expect("parses");
    store::load(&ini);

    // (name, the label in the fixture, the value it must resolve to)
    let want: [(&str, &str, i32); 10] = [
        // The sound-features choices = Stereo, Mono. No value array, so the index is the value.
        ("Sound.SoundFeatures", "Mono", 1),
        // The chat-font-face choices = Arial, CourierNew, PalatinoLinotype, Tahoma, TimesNewRoman.
        ("UI.ChatFontFace", "Tahoma", 3),
        // The chat-font-size choices = Tiny, Small, Medium, Large, XL.
        ("UI.ChatFontSize", "Large", 3),
        // The texture-filtering choices = Bilinear, Trilinear, Sharp, Anisotropic.
        ("Render.TextureFiltering", "Anisotropic", 3),
        // The landscape-texture-detail value array is [4, 3, 2, 1, 0]: the scale runs
        // backwards, so `VeryHigh` is **0** and `VeryLow` is 4.
        ("Render.LandscapeTextureDetail", "VeryHigh", 0),
        ("Render.EnvironmentTextureDetail", "VeryLow", 4),
        // No value array: Low, Medium, High are 0, 1, 2.
        ("Render.SceneryDrawDistance", "High", 2),
        // The landscape-draw-distance value array is [3, 5, 8, 11, 15, 25]; `Extreme` is
        // the sixth label and therefore **25**, not 5.
        ("Render.LandscapeDrawDistance", "Extreme", 25),
        // Built from the adapter modes at run time, labelled `"%ix%i"` with the value
        // `width << 16 | height`: `1280x1024` is `0x05000400`.
        ("Display.Resolution", "1280x1024", 0x0500_0400),
        // Labelled `"%ihz"`, with 0 labelled `Auto`.
        ("Display.RefreshRate", "75hz", 75),
    ];
    for (name, label, v) in want {
        assert!(enums.contains(&name), "{name} is one of the ten");
        assert!(
            RETAIL_SHAPED_INI.contains(&format!("={label}\r\n")),
            "the fixture says {label}"
        );
        assert_eq!(
            store::inq_value(name),
            Some(PrefValue::Int(v)),
            "{name}={label}"
        );
    }
}

/// Behaviour: options.preferences-file.drop-down-values-load-by-label-or-index
/// The two halves of the preference reader's choice arm that a fixture of real labels cannot
/// reach, because retail never writes them: the label match is **case-insensitive**
/// and a *number* in the file is an **index** into the choice list
/// rather than a value, with anything negative or past the end selecting 0.
///
/// `Render.LandscapeDrawDistance` is the one preference where those two readings give visibly
/// different answers, so it is the one asserted: `=3` is the fourth label `High` and must land as
/// **11**, and the value 3 belongs to `VeryLow`.
#[test]
fn a_number_in_the_file_is_an_index_into_the_choice_list_and_a_label_matches_case_insensitively() {
    assert_eq!(store::init(), 34);
    let name = "Render.LandscapeDrawDistance";

    let ini = UserPreferences::parse("[Render]\r\nLandscapeDrawDistance=3\r\n").unwrap();
    assert_eq!(store::load(&ini), (1, 0));
    assert_eq!(
        store::inq_value(name),
        Some(PrefValue::Int(11)),
        "index 3 is `High` = 11"
    );

    let ini = UserPreferences::parse("[Render]\r\nLandscapeDrawDistance=VERYLOW\r\n").unwrap();
    assert_eq!(store::load(&ini), (1, 0));
    assert_eq!(
        store::inq_value(name),
        Some(PrefValue::Int(3)),
        "and `VeryLow` is the 3"
    );

    // Past the end of a six-entry list, and negative: both select 0, which is `VeryLow`.
    for text in ["99", "-1"] {
        let ini = UserPreferences::parse(&format!("[Render]\r\nLandscapeDrawDistance={text}\r\n"))
            .unwrap();
        assert_eq!(store::load(&ini), (1, 0));
        assert_eq!(
            store::inq_value(name),
            Some(PrefValue::Int(3)),
            "{text} selects choice 0"
        );
    }

    // A label that is not in the list and is not a number is still an index of 0 — `strtol` returns
    // 0 for it — so an enumeration never fails to apply. That is why `load` counts all 34.
    let ini = UserPreferences::parse("[Render]\r\nTextureFiltering=Quadrilinear\r\n").unwrap();
    assert_eq!(store::load(&ini), (1, 0));
    assert_eq!(
        store::inq_value("Render.TextureFiltering"),
        Some(PrefValue::Int(0)),
        "Bilinear"
    );
}

/// The choice table is the **variable's**, not the option item's, and the two are different lists
/// of the same length. Asserted because conflating them is what made this look like a menu defect:
/// [`UI_PREFERENCES`]'s `choices` are `ID_*` localisation tokens for the text a popup draws, and
/// `store::ENUM_CHOICES` are the ASCII strings that appear in the INI.
#[test]
fn the_ini_choice_labels_are_not_the_menus_localisation_tokens() {
    for c in store::ENUM_CHOICES {
        let p = UI_PREFERENCES
            .iter()
            .find(|p| p.name == c.name)
            .expect("an attached preference");
        assert_eq!(p.kind, 2, "{} is an enumeration", c.name);
        assert_eq!(
            p.choices.len(),
            c.labels.len(),
            "{} — parallel lists",
            c.name
        );
        for (token, label) in p.choices.iter().zip(c.labels) {
            assert!(token.starts_with("ID_"), "{token} is a string id");
            assert_ne!(token, label, "{} — the INI never holds a token", c.name);
        }
        assert!(
            c.values.is_empty() || c.values.len() == c.labels.len(),
            "{} — setting from a string only maps index to value when the counts agree",
            c.name
        );
    }
    // The two `kind == 2` preferences with no compiled-in list are the two the client builds
    // from the adapter's display modes at run time.
    let tabled: Vec<&str> = store::ENUM_CHOICES.iter().map(|c| c.name).collect();
    let untabled: Vec<&str> = UI_PREFERENCES
        .iter()
        .filter(|p| p.kind == 2 && !tabled.contains(&p.name))
        .map(|p| p.name)
        .collect();
    assert_eq!(untabled, ["Display.Resolution", "Display.RefreshRate"]);
}
