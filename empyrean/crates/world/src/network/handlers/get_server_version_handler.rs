// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/GetServerVersionHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/GetServerVersionHandler.cs`.

use empyrean_entity::enums::ChatMessageType;
use empyrean_net::SessionId;

use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::managers::inbound_message_manager::{HandlerResult, Payload};
use crate::World;

// ACE: GetServerVersionHandler.GetServerVersion
/// An admin's `@version`: the client forwards it and the server answers with its build.
pub fn get_server_version(
    w: &mut World,
    _message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    // @version command is native to client. Client responds with the following (if using end of retail client/data):

    // Using Turbine Chat
    // Client version 00.00.11.6096.r Portal: compiled Fri Jun 12 04:16:27 2015 : RETAIL

    // If client connects with admin account, it will forward version request to server and will respond with the following:

    let msg = server_build_info_get_version_info(w);

    enqueue_send(
        w,
        session,
        game_message_system_chat(&msg, ChatMessageType::WorldBroadcast),
    );
    Ok(())
}

/// `ServerBuildInfo.GetVersionInfo()` (`ServerBuildInfo_Static.cs`), over the world database's
/// version row (a missing row is ACE's `NullReferenceException`).
fn server_build_info_get_version_info(w: &World) -> String {
    let v = w
        .content
        .get_version()
        .expect("ACE: DatabaseManager.World.GetVersion() is null (NullReferenceException)");
    empyrean_common::server_build_info::get_version_info(
        v.base_version.as_deref(),
        v.patch_version.as_deref(),
        v.last_modified,
    )
}
