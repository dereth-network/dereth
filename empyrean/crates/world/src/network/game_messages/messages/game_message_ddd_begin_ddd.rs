// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDBeginDDD.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageDDDBeginDDD.cs`.

use dereth_protocol::admin as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_common::dotnet::DotNetDict;
use empyrean_dat::DatDatabaseType;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

pub use crate::managers::ddd_manager::HI_FI_STRING_AS_INT;

// ACE: GameMessageDDDBeginDDD.GameMessageDDDBeginDDD
/// `missingIterations` is only probed with `ContainsKey`; each inner dictionary is enumerated, in
/// .NET order.
#[must_use]
pub fn game_message_ddd_begin_ddd(
    total_missing_iterations: u32,
    total_file_size: u32,
    missing_iterations: &std::collections::HashMap<DatDatabaseType, DotNetDict<u32, Vec<u32>>>,
) -> GameMessage {
    let mut revisions = Vec::new();

    // PCAPs show iterations list order was Portal, Language, Cell
    for ty in [
        DatDatabaseType::Portal,
        DatDatabaseType::Language,
        DatDatabaseType::Cell,
        DatDatabaseType::HighRes,
    ] {
        if let Some(iterations) = missing_iterations.get(&ty) {
            write_iterations(&mut revisions, ty, iterations);
        }
    }

    // `totalMissingIterations` counts the iterations the lists hold (DDDManager adds one per entry).
    debug_assert_eq!(
        usize::try_from(total_missing_iterations).ok(),
        Some(revisions.len())
    );
    let body = proto::DddBeginDdd {
        data_expected: total_file_size, // Total Size of Patches to Download
        revisions,                      // Number of MissingIterations, then each
    };
    GameMessage::from_proto(
        GameMessageOpcode::DDD_BeginDDD,
        GameMessageGroup::DatabaseQueue,
        &body,
    )
}

// ACE: GameMessageDDDBeginDDD.WriteIterations
/// Each iteration of one dat as a patch revision, in the dictionary's order.
fn write_iterations(
    revisions: &mut Vec<proto::PatchRevision>,
    dat_database_type: DatDatabaseType,
    iterations: &DotNetDict<u32, Vec<u32>>,
) {
    for (key, value) in iterations.iter() {
        let (dat_file_type, dat_file_id): (i32, i32) = match dat_database_type {
            DatDatabaseType::Portal => (0, 1),
            DatDatabaseType::Cell => (1, 2),
            DatDatabaseType::Language => (1, 3),
            DatDatabaseType::HighRes => (HI_FI_STRING_AS_INT, 1), // HiFi
        };

        let iteration: i32 = (*key).cs_cast();

        let (ids_to_download, ids_to_purge) = if dat_database_type == DatDatabaseType::Cell {
            // PCAPs show Cell updates were purges only, while the other two were downloads
            (Vec::new(), value.clone()) // IDsToDownload Count 0, then IDsToPurge
        } else {
            (value.clone(), Vec::new()) // IDsToDownload, then IDsToPurge Count 0
        };
        revisions.push(proto::PatchRevision {
            dat_file_type: dat_file_type.cast_unsigned(),
            dat_file_id: dat_file_id.cast_unsigned(),
            iteration: iteration.cast_unsigned(), // iteration
            ids_to_download,
            ids_to_purge,
        });
    }
}
