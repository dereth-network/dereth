// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/EventSQLWriter.cs
//! ACE's `EventSQLWriter`: one game event.

use std::ops::{Deref, DerefMut};

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::enums::GameEventState;

use super::sql_writer::*;
use crate::models::world::Event;

/// ACE's `EventSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: EventSQLWriter
#[derive(Debug, Clone, Default)]
pub struct EventSQLWriter {
    pub base: SQLWriter,
}

impl Deref for EventSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for EventSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

/// `DateTimeOffset.FromUnixTimeSeconds(seconds).DateTime.ToString(CultureInfo.InvariantCulture)`
/// (an `int` is always within `FromUnixTimeSeconds`' range).
fn unix_time_text(seconds: i32) -> String {
    DotNetDateTime::new(1970, 1, 1)
        .add_ticks(i64::from(seconds) * 10_000_000)
        .format("MM/dd/yyyy HH:mm:ss")
}

impl EventSQLWriter {
    /// Default is formed from: input.name
    // ACE: EventSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &Event) -> String {
        let mut file_name = replace_illegal_in_file_name(&input.name);
        file_name += ".sql";

        file_name
    }

    // ACE: EventSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &Event, writer: &mut SqlOut) {
        writer.write_line(&format!(
            "DELETE FROM `event` WHERE `name` = {};",
            s(SQLWriter::get_sql_string(Some(&input.name)).as_deref())
        ));
    }

    // ACE: EventSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(&self, input: &Event, writer: &mut SqlOut) {
        writer.write_line(
            "INSERT INTO `event` (`name`, `start_Time`, `end_Time`, `state`, `last_Modified`)",
        );

        let time = |t: i32| {
            if t == -1 {
                t.to_string()
            } else {
                format!("{t} /* {} */", unix_time_text(t))
            }
        };

        let mut output = format!(
            "VALUES ({}, {}, {}, {} /* GameEventState.{} */, '{}');",
            s(SQLWriter::get_sql_string(Some(&input.name)).as_deref()),
            time(input.start_time),
            time(input.end_time),
            input.state,
            GameEventState(input.state).to_dotnet_string(),
            date(input.last_modified)
        );

        output = SQLWriter::fix_null_fields(&output);

        writer.write_line(&output);
    }
}
