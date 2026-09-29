//! What the player is told when the first connection to the shard fails, and what happens next.
//!
//! The initial connection is made before any screen exists. When it ends in a network error -- a
//! refusal the shard sends before the handshake (a wrong client version, a shutdown), or the
//! twenty unanswered login requests -- the retail client shows one modal error box and, when it is
//! closed, the process exits. There is no login screen to go back to.
//!
//! # The box
//!
//! * **Caption:** core string 11, `"Game Error"` ([`crate::corestrings::CAPTION_ERROR`]).
//! * **Button:** a single OK, with the error icon; the box is top-most, is brought to the
//!   foreground and has no owner window (`ERROR_BOX_STYLE`).
//! * **Text:** one sentence built around the error's row of the connection-error string table:
//!
//!   ```text
//!   Failed to establish connection to the server: (<row text>) (<table>:<string id>)
//!   ```
//!
//!   `<row text>` is the row the error names, in string table 8 of `client_local_English.dat`
//!   (table 8 is resolved to its `DataID` through the master enum map, group 4). `<table>` is that
//!   `DataID` in eight upper-case hex digits, `<string id>` the error's string id in eight
//!   zero-padded ones. So a wrong client version reads
//!   `Failed to establish connection to the server: (You do not have the current version of the
//!   client installed.) (23000010:00A7E948)` against the shipped dats, where table 8 is
//!   `0x23000010`. The row is unescaped as every string-table row is, so a stored `\n` is a line
//!   break.
//!
//!   When the row cannot be rendered the parenthesised text is replaced by
//!   `<could not render string: table 0x<table> token 0x<string id>. Reason = <n>>`, with the
//!   string-table failure reason: 2 for a missing row, 3 for a missing table, 6 for a row that
//!   needs a value nobody supplied.
//!
//! This module decides all of that from the assets alone. Showing the box is the platform's job
//! ([`crate::platform::dialog`]), and exiting is the frame loop's.

use dereth_primitives::{AssetSource, DataId};
use dereth_transport::conn::NetErrorCode;

use crate::corestrings::{DisplayStringMode, CAPTION_ERROR};

/// The master enum map's group for string tables: a table *enum* (the `8` a network error
/// carries) is looked up in this group to find the table's `DataID`.
pub const STRING_TABLE_GROUP: u32 = 4;

/// The error box's style bits: OK only (`0x0`), the stop icon (`0x10`), set foreground
/// (`0x1_0000`) and top-most (`0x4_0000`). No owner window.
pub const ERROR_BOX_STYLE: u32 = 0x0005_0010;

/// The string-table failure reasons the fallback sentence can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowFailure {
    /// The table is there and has no row with this id.
    MissingString = 2,
    /// The table enum resolves to nothing, or to something that is not a string table.
    MissingTable = 3,
    /// The row substitutes a value, and a network error carries none.
    MissingVariable = 6,
}

/// One modal error box: what the platform shows before the process ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorPopup {
    /// Which of the three box kinds. Always [`DisplayStringMode::Error`] here.
    pub mode: DisplayStringMode,
    /// The caption.
    pub caption: String,
    /// The body.
    pub text: String,
    /// The box's style bits, `ERROR_BOX_STYLE`.
    pub style: u32,
}

/// The decision for a login that ended before the link was up: show [`Self::popup`], and when
/// the player closes it, exit the process. There is no other branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectFailure {
    /// The error that ended the login.
    pub code: NetErrorCode,
    /// The box to show.
    pub popup: ErrorPopup,
}

/// A row of a connection-error table, looked up the way the error box looks it up.
///
/// Returns the table's `DataID` (zero when the enum resolves to none) and either the row's text
/// or why it could not be rendered.
pub fn resolve_row(
    assets: &dyn AssetSource,
    table_enum: u32,
    string_id: u32,
) -> (DataId, Result<String, RowFailure>) {
    use dereth_assets::Decode;
    let Some(table) = dereth_assets::did_by_enum(assets, STRING_TABLE_GROUP, table_enum) else {
        return (DataId(0), Err(RowFailure::MissingTable));
    };
    let Some(rows) = assets
        .read(table)
        .ok()
        .and_then(|bytes| dereth_assets::ui::StringTable::decode_payload(table, &bytes).ok())
    else {
        return (table, Err(RowFailure::MissingTable));
    };
    let Some((_, row)) = rows.strings.into_iter().find(|(id, _)| *id == string_id) else {
        return (table, Err(RowFailure::MissingString));
    };
    if !row.variables.is_empty() {
        return (table, Err(RowFailure::MissingVariable));
    }
    // The row is stored escaped (a line break is a backslash and an `n`); the lookup unescapes it.
    match row.strings.into_iter().next() {
        Some(text) => (table, Ok(dereth_assets::escape::unescape(text))),
        None => (table, Err(RowFailure::MissingString)),
    }
}

/// The box's body for an error, given what [`resolve_row`] found.
#[must_use]
pub fn connect_failure_text(
    string_id: u32,
    table: DataId,
    row: &Result<String, RowFailure>,
) -> String {
    let rendered = match row {
        Ok(text) => format!("({text})"),
        Err(reason) => format!(
            "<could not render string: table 0x{:08X} token 0x{string_id:08X}. Reason = {}>",
            table.0, *reason as u32
        ),
    };
    format!(
        "Failed to establish connection to the server: {rendered} ({:8X}:{string_id:08X})",
        table.0
    )
}

/// What a login that failed before the link was up shows, resolved from `assets`.
#[must_use]
pub fn connect_failure(code: NetErrorCode, assets: &dyn AssetSource) -> ConnectFailure {
    let string_id = code.string_id();
    let table_enum = u32::try_from(dereth_transport::conn::NET_ERROR_TABLE_ID).unwrap_or(0);
    let (table, row) = resolve_row(assets, table_enum, string_id);
    ConnectFailure {
        code,
        popup: ErrorPopup {
            mode: DisplayStringMode::Error,
            caption: CAPTION_ERROR.to_owned(),
            text: connect_failure_text(string_id, table, &row),
            style: ERROR_BOX_STYLE,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendered_row_is_parenthesised_and_followed_by_the_table_and_the_id() {
        let text = connect_failure_text(
            NetErrorCode::NetVersionMismatch.string_id(),
            DataId(0x2300_0010),
            &Ok("You do not have the current version of the client installed.".to_owned()),
        );
        assert_eq!(
            text,
            "Failed to establish connection to the server: (You do not have the current version \
             of the client installed.) (23000010:00A7E948)"
        );
    }

    #[test]
    fn a_row_that_will_not_render_says_why_in_its_place() {
        let text = connect_failure_text(
            NetErrorCode::ServerFull.string_id(),
            DataId(0x2300_0010),
            &Err(RowFailure::MissingString),
        );
        assert_eq!(
            text,
            "Failed to establish connection to the server: <could not render string: table \
             0x23000010 token 0x00F9982C. Reason = 2> (23000010:00F9982C)"
        );
    }

    #[test]
    fn a_table_with_no_id_is_padded_to_eight_columns() {
        let text = connect_failure_text(0x0A7_E948, DataId(0), &Err(RowFailure::MissingTable));
        assert!(text.ends_with("Reason = 3> (       0:00A7E948)"), "{text}");
    }

    /// With no assets at all the box still comes up, with the error caption and the stop icon.
    #[test]
    fn with_no_assets_the_box_still_names_the_error() {
        struct Empty;
        impl AssetSource for Empty {
            fn read(&self, id: DataId) -> Result<Vec<u8>, dereth_primitives::AssetError> {
                Err(dereth_primitives::AssetError::NotFound(id))
            }
            fn exists(&self, _: DataId) -> bool {
                false
            }
            fn iter_type(
                &self,
                _: dereth_primitives::DataType,
            ) -> Box<dyn Iterator<Item = DataId> + '_> {
                Box::new(std::iter::empty())
            }
        }
        let f = connect_failure(NetErrorCode::NetVersionMismatch, &Empty);
        assert_eq!(f.popup.caption, "Game Error");
        assert_eq!(f.popup.style, 0x0005_0010);
        assert_eq!(f.popup.mode, DisplayStringMode::Error);
        assert_eq!(
            f.popup.text,
            "Failed to establish connection to the server: <could not render string: table \
             0x00000000 token 0x00A7E948. Reason = 3> (       0:00A7E948)"
        );
    }
}
