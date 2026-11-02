//! The preferences file's form: what is read from it and where what is written lands.

use auberean_shell::profile::Profile;

/// A key is read from the first section of its name only, letter case ignored on both names; a
/// header left open still names its section, and its name loses the spaces at its ends. A line
/// is a key's when it has an `=` and starts with neither `;` nor `[`; the key and the value lose
/// the spaces and tabs at their ends, and a value quoted at both ends by the same mark loses the
/// marks (one mark alone stays). Lines before the first header are in no section. Asked with a
/// default, a missing key gives the default less its trailing spaces, cut to the buffer.
///
/// Behaviour: desktop.preferences.the-file-reads-as-the-clients-system-reads-it
#[test]
fn the_file_reads_as_the_clients_system_reads_it() {
    let p = Profile::new(
        "early=0\r\n [ Camera ] \r\n  Key  =  \"quoted value\"  \r\nkey=second\r\n\
         noequals\r\n ;c=1\r\n\tt\t=\tv\t\r\nk==v\r\nq=\"\"\r\nr= ' x ' \r\nh=\"half\r\n\
         [camera]\r\nOther=1\r\n[Open\r\nx=1\r\n",
    );
    assert_eq!(p.value("CAMERA", "key"), Some("quoted value"));
    assert_eq!(
        p.value("Camera", "Other"),
        None,
        "a second section is not read"
    );
    assert_eq!(p.value("Camera", "noequals"), None);
    assert_eq!(p.value("Camera", ";c"), None);
    assert_eq!(p.value("Camera", "t"), Some("v"));
    assert_eq!(p.value("Camera", "k"), Some("=v"));
    assert_eq!(p.value("Camera", "q"), Some(""));
    assert_eq!(p.value("Camera", "r"), Some(" x "));
    assert_eq!(p.value("Camera", "h"), Some("\"half"));
    assert_eq!(p.value("Open", "x"), Some("1"));
    assert_eq!(p.value("", "early"), None);
    assert_eq!(p.read("Camera", "none", "dflt   ", 100), "dflt");
    assert_eq!(p.read("Camera", "none", "0123456789", 5), "0123");
    assert_eq!(p.read("Camera", "t", "D", 1), "", "found, cut to nothing");
}

/// A value written for a key already there replaces what follows its `=` up to the value's end:
/// the key as written, the spaces around the `=` and after the value stay, and so does the line's
/// own ending. A new key goes after its section's last key line (comments and other lines stay
/// below it), or at the very end when its section is the last and ends in a blank line; a new
/// section goes at the end. New lines end in a carriage return and a line feed, and before any
/// change a last line with no ending loses its trailing spaces and is ended. A key taken out
/// leaves the spaces before it.
///
/// Behaviour: desktop.preferences.a-value-written-lands-where-the-clients-system-puts-it
#[test]
fn a_value_written_lands_where_the_clients_system_puts_it() {
    let write = |text: &str, section: &str, key: &str, value: Option<&str>| {
        let mut p = Profile::new(text);
        p.write(section, key, value);
        p.text().to_owned()
    };
    assert_eq!(
        write("[A]\r\nx = 1 \r\n", "A", "X", Some("9")),
        "[A]\r\nx =9 \r\n"
    );
    assert_eq!(
        write("[A]\r\n  x  =  old value  ;c\r\n", "A", "x", Some("n")),
        "[A]\r\n  x  =n\r\n"
    );
    assert_eq!(write("[A]\nx=1\n", "A", "x", Some("22")), "[A]\nx=22\n");
    assert_eq!(
        write("[A]\nx=1\n\n\n[B]\ny=2", "A", "z", Some("3")),
        "[A]\nx=1\nz=3\r\n\n\n[B]\ny=2\r\n"
    );
    assert_eq!(
        write("[A]\r\nx=1\r\n;c\r\n", "A", "y", Some("2")),
        "[A]\r\nx=1\r\ny=2\r\n;c\r\n"
    );
    assert_eq!(
        write("[A]\r\nx=1\r\n;c\r\n\r\n", "A", "y", Some("2")),
        "[A]\r\nx=1\r\n;c\r\n\r\ny=2\r\n"
    );
    assert_eq!(
        write("[A]\r\n;c\r\n\r\n[B]\r\n", "A", "y", Some("2")),
        "[A]\r\ny=2\r\n;c\r\n\r\n[B]\r\n"
    );
    assert_eq!(write("", "A", "x", Some("1")), "[A]\r\nx=1\r\n");
    assert_eq!(
        write("[A]\r\nx=1   ", "B", "y", Some("2")),
        "[A]\r\nx=1\r\n[B]\r\ny=2\r\n"
    );
    assert_eq!(
        write("[A]\r\nx=1\r\n y=2\r\nz=3\r\n", "A", "y", None),
        "[A]\r\nx=1\r\n z=3\r\n"
    );
    let mut p = Profile::new("[A]\r\nx=1\r\n;c\r\n[B]\r\ny=2\r\n");
    p.remove_section("a");
    assert_eq!(
        p.text(),
        ";c\r\n[B]\r\ny=2\r\n",
        "a comment past the last key stays"
    );
}
