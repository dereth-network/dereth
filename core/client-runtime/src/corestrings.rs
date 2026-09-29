//! The `corestrings.dll` string table, as a `const` table.
//!
//! This replaces the startup-error resource DLL with a 33-entry constant table.
//! The entries below were extracted from the shipped DLL.
//!
//! Only the entries this application can actually raise are here; the graphics-layer ones
//! (101–105, 108, 110–111, 121–129) belong to whoever raises them and are not transcribed twice.
//!
//! Display substitution replaces `%1`/`%2`/`%3` with its three arguments;
//! `display_string` performs that substitution.

/// Error / warning / information — the display mode, the third argument of the client's string
/// display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayStringMode {
    Error = 0,
    Warning = 1,
    Information = 2,
}

/// The dialog captions, ids 11–13.
pub const CAPTION_ERROR: &str = "Game Error";
pub const CAPTION_WARNING: &str = "Game Warning";
pub const CAPTION_INFORMATION: &str = "Game Information";

/// Database initialization uses this when the portal dat will not open; server-interrogation
/// handling uses it when the iteration list will not load.
pub const ID_CANT_OPEN_DATA_FILES: u32 = 201;
/// The dat layer, when a named local dat is missing.
pub const ID_CANT_FIND_LOCAL_DATA_FILE: u32 = 202;
/// The portal's out-of-memory handler.
pub const ID_OUT_OF_MEMORY: u32 = 203;
/// The client's already-running check uses this name.
pub const ID_ALREADY_RUNNING: u32 = 204;
/// **every** command-line error surfaces as this one string.
pub const ID_LAUNCHER_ONLY: u32 = 205;
/// The dat layer, when a container is structurally bad.
pub const ID_DATA_FILE_CORRUPT: u32 = 206;
/// Device initialization uses this when `RegisterClass` or `CreateWindowEx` fails.
pub const ID_FATAL_WINDOWS_API: u32 = 127;

/// The entries transcribed from the shipped `corestrings.dll` resource.
const TABLE: &[(u32, &str)] = &[
    (1, "OK"),
    (2, "Cancel"),
    (3, "Ignore"),
    (11, CAPTION_ERROR),
    (12, CAPTION_WARNING),
    (13, CAPTION_INFORMATION),
    (
        127,
        "The game encountered a fatal Windows API issue while attempting to start. Try rebooting \
         your machine and starting the game again.",
    ),
    (
        201,
        "Can't open the data files. Check that they exist and that you have permission to write to \
         them. The program will now exit.",
    ),
    (202, "Can't find the local data file '%1'."),
    (203, "Could not allocate needed memory. The game will now end."),
    (
        204,
        "There is a client already running on this machine. It is not possible to run two clients \
         on the same machine. Please use the already running client.",
    ),
    (205, "You can only run the game from the launcher program."),
    (206, "The data file '%1' appears to be corrupted!"),
    (207, "Failed to save data. Out of disk space? The game will now end."),
];

/// The text the client displays for a core-string id, with `%1`/`%2`/`%3` substituted.
///
/// When the string table cannot supply the id the client prints
/// `<corestrings.dll not found. Tried to print stringID %d>`; a missing id here is the same
/// situation and produces the same sentence.
#[must_use]
pub fn display_string(id: u32, args: &[&str]) -> String {
    let Some((_, template)) = TABLE.iter().find(|(k, _)| *k == id) else {
        return format!("<corestrings.dll not found. Tried to print stringID {id}>");
    };
    let mut out = (*template).to_string();
    for (i, a) in args.iter().enumerate().take(3) {
        out = out.replace(&format!("%{}", i + 1), a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the string table extracted from the shipped corestrings.dll.
    #[test]
    fn the_dat_open_failure_is_string_201_verbatim() {
        assert_eq!(
            display_string(ID_CANT_OPEN_DATA_FILES, &[]),
            "Can't open the data files. Check that they exist and that you have permission to \
             write to them. The program will now exit."
        );
    }

    #[test]
    fn every_command_line_error_is_string_205() {
        assert_eq!(
            display_string(ID_LAUNCHER_ONLY, &[]),
            "You can only run the game from the launcher program."
        );
    }

    #[test]
    fn the_substitution_fills_percent_one() {
        assert_eq!(
            display_string(ID_CANT_FIND_LOCAL_DATA_FILE, &["client_local_English.dat"]),
            "Can't find the local data file 'client_local_English.dat'."
        );
    }

    #[test]
    fn an_unknown_id_reports_itself_the_way_the_client_does() {
        assert_eq!(
            display_string(999, &[]),
            "<corestrings.dll not found. Tried to print stringID 999>"
        );
    }
}
