//! The fixed form of the log lines a program driving the console reads: each command's answer
//! tagged as done or refused, and each character's entering and leaving, by name. Fixture: none
//! (the form itself).

use yalain_world::answers;

/// The tags programs read stay as they are, at the end of the line after the words for people:
/// `[ok <command>]` and `[refused <command>]`, `[entered "<name>" <id>]` and `[left "<name>"
/// <id>]` for a character, and `[listening]`, all from the `yalain` target, each one line; a
/// line's tag is read from its last ` [`, whatever brackets its words hold.
/// Behaviour: server.operator.console-answers-in-a-fixed-form
#[test]
fn the_lines_programs_read_keep_their_fixed_form() {
    assert_eq!(answers::TARGET, "yalain");
    let sent = answers::ok(
        "teleport",
        "Kara is on its way to 77B80006 [4.4, 100.8, 182.5]",
    );
    assert_eq!(
        sent,
        "Kara is on its way to 77B80006 [4.4, 100.8, 182.5] [ok teleport]"
    );
    assert_eq!(
        answers::read(&sent),
        Some((
            "ok teleport",
            "Kara is on its way to 77B80006 [4.4, 100.8, 182.5]"
        ))
    );
    assert_eq!(
        answers::refused(
            "deletecharacter",
            "deletecharacter: no character is named Kara"
        ),
        "deletecharacter: no character is named Kara [refused deletecharacter]"
    );
    let entered = answers::entered(
        "Hand Of Doom",
        0x2130_0000_0000_0007,
        "in the world at 77B80006",
    );
    assert_eq!(
        entered,
        "in the world at 77B80006 [entered \"Hand Of Doom\" 0x2130000000000007]"
    );
    assert_eq!(
        answers::read(&format!("2026-11-01T10:00:00Z  INFO yalain: {entered}\n")),
        Some((
            "entered \"Hand Of Doom\" 0x2130000000000007",
            "2026-11-01T10:00:00Z  INFO yalain: in the world at 77B80006"
        ))
    );
    assert_eq!(
        answers::left(
            "Hand Of Doom",
            0x2130_0000_0000_0007,
            "left the world at 77B80006"
        ),
        "left the world at 77B80006 [left \"Hand Of Doom\" 0x2130000000000007]"
    );
    assert_eq!(
        answers::listening("listening on 127.0.0.1:9000/9001 (login)"),
        "listening on 127.0.0.1:9000/9001 (login) [listening]"
    );
    assert_eq!(
        answers::refused("make-character", "a.toml: line 1\nline 2"),
        "a.toml: line 1 line 2 [refused make-character]",
        "one line"
    );
    assert_eq!(
        answers::read("stopping: 1 characters in the world kept"),
        None
    );
}
