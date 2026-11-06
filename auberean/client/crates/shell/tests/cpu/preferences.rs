//! The preferences the client's startup asks for: set for the run, else read from the file.

use auberean_shell::preferences::{key, section, EnumNames, Names, Preferences};
use auberean_shell::profile::Profile;

fn names() -> Names {
    Names {
        sections: EnumNames::of(&[
            (section::NET, "Net"),
            (section::INTERNATIONAL, "International"),
        ]),
        keys: EnumNames::of(&[
            (key::COMPUTE_UNIQUE_PORT, "ComputeUniquePort"),
            (key::USER_SPECIFIED_PORT, "UserSpecifiedPort"),
            (key::LANGUAGE, "Language"),
            (key::USE_INPUT_METHOD_EDITOR, "UseInputMethodEditor"),
        ]),
    }
}

/// A preference is asked for by its section and key, which the data names: a value set for the
/// run (from the command line) comes first, and is kept as its decimal text; else the file's, by
/// those names. A number reads only when the whole value is one (decimal, hexadecimal after `0x`,
/// octal after `0`, a minus sign wrapping as the C library wraps it). An empty
/// value is none, and so is a preference the data does not name.
///
/// Behaviour: desktop.preferences.a-value-set-for-the-run-comes-before-the-files
#[test]
fn a_value_set_for_the_run_comes_before_the_files() {
    let file = Profile::new(
        "[Net]\r\nComputeUniquePort=1\r\nUserSpecifiedPort=0x2328\r\n\
         [International]\r\nLanguage=  Deutsch \r\nUseInputMethodEditor=\r\n",
    );
    let mut p = Preferences::startup(file, names());
    assert_eq!(
        p.inq_uint32(section::NET, key::USER_SPECIFIED_PORT),
        Some(9000)
    );
    assert_eq!(
        p.inq_bool(section::NET, key::COMPUTE_UNIQUE_PORT),
        Some(true)
    );
    assert_eq!(
        p.inq_string(section::INTERNATIONAL, key::LANGUAGE)
            .as_deref(),
        Some("Deutsch")
    );
    assert_eq!(
        p.inq_string(section::INTERNATIONAL, key::USE_INPUT_METHOD_EDITOR),
        None,
        "empty is none"
    );
    assert_eq!(p.inq_uint32(section::INTERNATIONAL, key::LANGUAGE), None);
    assert_eq!(
        p.inq_string(section::NET, 0x4000_0071 + 99),
        None,
        "unnamed"
    );
    p.set_bool(section::NET, key::COMPUTE_UNIQUE_PORT, 0);
    p.set_bool(section::NET, key::USER_SPECIFIED_PORT, 9010);
    assert_eq!(
        p.inq_bool(section::NET, key::COMPUTE_UNIQUE_PORT),
        Some(false)
    );
    assert_eq!(
        p.lookup(section::NET, key::USER_SPECIFIED_PORT),
        Some("9010")
    );
    assert_eq!(
        p.inq_uint32(section::NET, key::USER_SPECIFIED_PORT),
        Some(9010)
    );
    p.set_bool(section::INTERNATIONAL, key::LANGUAGE, -1);
    assert_eq!(
        p.inq_string(section::INTERNATIONAL, key::LANGUAGE)
            .as_deref(),
        Some("-1")
    );
    assert_eq!(
        p.inq_uint32(section::INTERNATIONAL, key::LANGUAGE),
        Some(u32::MAX),
        "as the C library reads -1 unsigned"
    );
    let mut q = Preferences::startup(Profile::new("[Net]\r\nUserSpecifiedPort=9000\r\n"), names());
    q.set_bool(section::NET, key::USER_SPECIFIED_PORT, 1);
    assert_eq!(
        q.inq_uint32(section::NET, key::USER_SPECIFIED_PORT),
        Some(1)
    );
}

/// The language to run in is the command line's, else the preferences' `[International] Language`
/// (its ends trimmed), as its name in the languages enumeration names it, letter case ignored;
/// English when neither names one there.
///
/// Behaviour: desktop.preferences.the-language-is-the-lines-else-the-preferences-else-english
#[test]
fn the_language_is_the_lines_else_the_preferences_else_english() {
    use auberean_shell::preferences::{language, ENGLISH, LANGUAGES};
    let languages = EnumNames::of(&[
        (ENGLISH, "English"),
        (0x8100_0001, "Francais"),
        (0x8100_0003, "Deutsch"),
    ]);
    assert_eq!(LANGUAGES, 0x4000_002A);
    let prefs = |text: &str| Preferences::startup(Profile::new(text), names());
    let german = prefs("[International]\r\nLanguage=  deutsch \r\n");
    assert_eq!(language(None, &german, &languages), 0x8100_0003);
    assert_eq!(language(Some("Francais"), &german, &languages), 0x8100_0001);
    assert_eq!(
        language(Some(""), &german, &languages),
        0x8100_0003,
        "an empty line names none"
    );
    assert_eq!(language(Some("Klingon"), &german, &languages), ENGLISH);
    assert_eq!(language(None, &prefs(""), &languages), ENGLISH);
}
