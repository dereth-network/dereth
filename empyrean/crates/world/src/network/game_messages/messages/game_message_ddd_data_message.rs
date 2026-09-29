// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDDataMessage.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDDataMessage.cs`.

use dereth_protocol::admin as proto;
use empyrean_dat::DatDatabaseType;
use empyrean_net::GameMessageGroup;

use crate::managers::ddd_manager::{self, HI_FI_STRING_AS_INT};
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::World;

// ACE: GameMessageDDDDataMessage.GameMessageDDDDataMessage
/// One dat file for the client: which dat, the file's type, id and iteration, whether it is
/// compressed, and its contents (compressed ones lead with their uncompressed size).
///
/// Not ACE's (a fix, V290): a file `GetFileType` does not know is sent with
/// the type the client gives it. ACE cast its `null` `DatFileType?` to `uint`, which threw out of
/// the session's `TickOutbound` (and ended the session) once such a file's turn in the DDD queue
/// came. The one such file in the retail portal, language and high-res dats is the iteration
/// file `0xFFFF0001`, which ACE lists under iteration 1. The client takes it like any other
/// download: it keys each id `BeginDDD` names by the type its own id ranges give, which for an id
/// in none of them is 6 in the portal and high-res dats and 37 in the language dat (1 for every
/// cell-dat id), waits for a `DataMessage` of that type and id, and saves it; its own iteration
/// list is written back as each iteration completes. For every other file in those dats
/// `GetFileType` and the client agree.
///
/// # Panics
/// As `DDDManager.TryGetDatFileContentsForTransmission`.
#[must_use]
pub fn game_message_ddd_data_message(
    w: &mut World,
    dat_file_id: u32,
    dat_database_type: DatDatabaseType,
) -> GameMessage {
    let mut msg = GameMessage::new(
        GameMessageOpcode::DDD_DataMessage,
        GameMessageGroup::DatabaseQueue,
    );

    let (dat_file_type, dat_file_id_of_dat): (i32, i32) = match dat_database_type {
        DatDatabaseType::Portal => (0, 1),
        DatDatabaseType::Cell => (1, 2),
        DatDatabaseType::Language => (1, 3),
        DatDatabaseType::HighRes => (HI_FI_STRING_AS_INT, 1), // HiFi
    };

    let dats = std::sync::Arc::clone(&w.dats);
    let Some(contents) = ddd_manager::try_get_dat_file_contents_for_transmission(
        &mut w.ddd_manager,
        &dats,
        dat_file_id,
        dat_database_type,
    ) else {
        // do something here?
        return msg;
    };

    let dat_file = contents.dat_file;
    let file_id = dat_file.object_id;
    let file_type = ddd_manager::dat_file_get_file_type(dat_file.object_id, dat_database_type)
        .unwrap_or_else(|| client_fallback_file_type(dat_database_type));

    let length = i32::try_from(contents.contents.len()).unwrap_or(i32::MAX);
    let body = proto::DddData {
        dat_file_type: dat_file_type.cast_unsigned(), // DatFileType
        dat_file_id: dat_file_id_of_dat.cast_unsigned(), // DatFileID
        resource_type: file_type,
        resource_id: file_id,
        iteration: dat_file.iteration,
        compressed: u8::from(contents.is_compressed),
        version: 3,                                        // version
        data_size: length.wrapping_add(4).cast_unsigned(), // length + size of this message
        data: contents.contents,
    };
    msg.write_proto(&body);
    msg
}

/// Not ACE's (a fix, V290): the type the client keys a downloaded id by when its id ranges give it none,
/// by the dat the id is patched into.
fn client_fallback_file_type(dat_database_type: DatDatabaseType) -> u32 {
    use ddd_manager::dat_file_type as t;
    match dat_database_type {
        DatDatabaseType::Portal | DatDatabaseType::HighRes => t::GRAPHICS_OBJECT,
        DatDatabaseType::Language => t::STRING_TABLE,
        DatDatabaseType::Cell => t::LAND_BLOCK,
    }
}
