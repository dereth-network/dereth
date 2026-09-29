// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventFriendsListUpdate.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventFriendsListUpdate.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::social::{FriendData, SocialFriendsUpdate};
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

/// ACE's nested `GameEventFriendsListUpdate.FriendsUpdateTypeFlag` (`[Flags]`, underlying `int`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FriendsUpdateTypeFlag(pub i32);

#[allow(non_upper_case_globals)]
impl FriendsUpdateTypeFlag {
    pub const FullList: Self = Self(0x0000);
    pub const FriendAdded: Self = Self(0x0001);
    pub const FriendRemoved: Self = Self(0x0002);
    pub const FriendStatusChanged: Self = Self(0x0004);
}

// ACE: GameEventFriendsListUpdate.GameEventFriendsListUpdate
/// `GameEventFriendsListUpdate(Session session)`: the full list.
#[must_use]
pub fn game_event_friends_list_update(w: &mut World, session: SessionId) -> GameMessage {
    // 2,062 is the average seen in retail pcaps, 36,916 is the max seen in retail pcaps
    let mut msg = game_event_message_with_capacity(
        GameEventType::FriendsListUpdate,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        4096,
    );
    write_event_body(
        w,
        session,
        &mut msg,
        FriendsUpdateTypeFlag::FullList,
        None,
        false,
        false,
    );
    msg
}

/// `GameEventFriendsListUpdate(Session session, FriendsUpdateTypeFlag updateType,
/// CharacterPropertiesFriendList friend, bool overrideOnlineStatus = false, bool onlineStatusVal =
/// false)`. `friend_id` is the one field of the shard row the body reads (`FriendId`).
#[must_use]
pub fn game_event_friends_list_update_one(
    w: &mut World,
    session: SessionId,
    update_type: FriendsUpdateTypeFlag,
    friend_id: u32,
    override_online_status: bool,
    online_status_val: bool,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::FriendsListUpdate,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    write_event_body(
        w,
        session,
        &mut msg,
        update_type,
        Some(friend_id),
        override_online_status,
        online_status_val,
    );
    msg
}

// ACE: GameEventFriendsListUpdate.WriteEventBody
/// The friend list (the character's whole list, or the one friend), each with its online state
/// and name, then the update type.
#[allow(clippy::ptr_arg)]
fn write_event_body(
    w: &World,
    session: SessionId,
    msg: &mut GameMessage,
    update_type: FriendsUpdateTypeFlag,
    friend_id: Option<u32>,
    override_online_status: bool,
    online_status_val: bool,
) {
    use crate::managers::player_manager;

    let friend_list: Vec<u32> = if update_type == FriendsUpdateTypeFlag::FullList {
        let player = crate::network::game_event::game_event_message::session_player(w, session);
        w.objects
            .get(player)
            .and_then(crate::world_objects::world_object_networking::shims::player_character)
            .expect("ACE: Session.Player.Character is null (NullReferenceException)")
            .character_properties_friend_list
            .iter()
            .map(|f| f.friend_id)
            .collect()
    } else {
        vec![friend_id.expect("ACE: friend is null (NullReferenceException)")]
    };

    // `(uint)friendList.Count`, then each friend.
    let mut friends = Vec::with_capacity(friend_list.len());
    let mut names = Vec::with_capacity(friend_list.len());
    for f in friend_list {
        let (player, mut is_online) = player_manager::find_by_guid(w, f);
        let friend_name = player
            .and_then(|p| crate::entity::i_player::name(w, p))
            .unwrap_or_default();

        if override_online_status {
            is_online = online_status_val;
        } else if is_online {
            // Does this friend want to appear offline?
            if let Some(online_friend) = player_manager::get_online_player(w, f) {
                if crate::world_objects::player_character::get_appear_offline(w, online_friend) {
                    is_online = false;
                }
            }
        }

        friends.push(FriendData {
            id: ObjectId(f),                     // Friend's ID
            online: i32::from(is_online),        // Whether this friend is online
            appear_offline: 0,                   // Whether the friend should appear to be offline
            name: ace_str(friend_name.as_str()), // Name of the friend
            // send the list of friend's friends
            friends_list: Vec::new(), // TODO player.Character.CharacterPropertiesFriendList.Count
            // todo: send the inverse list of friend's friends
            friend_of_list: Vec::new(), // TODO playersFriend.Character.CharacterPropertiesFriendList.Count
        });
        names.push(friend_name);
    }

    let body = SocialFriendsUpdate {
        friends,
        update_type: update_type.0.cast_unsigned(),
    };
    let strings: Vec<&str> = names.iter().map(String::as_str).collect();
    msg.write_proto_strings(&body, &strings);
}
