// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventCharacterTitle.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventCharacterTitle.cs`.

use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventCharacterTitle.GameEventCharacterTitle
/// The title table: version 1, the displayed title, then the character's titles in its title
/// book's order (setting `NumCharacterTitles`).
#[must_use]
pub fn game_event_character_title(w: &mut World, session: SessionId) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::CharacterTitle,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );
    let player = crate::network::game_event::game_event_message::session_player(w, session);
    let o = w
        .objects
        .get_mut(player)
        .expect("ACE: session.Player is null (NullReferenceException)");
    let display_title = o.character_title_id().unwrap_or(0);

    let titles: Vec<u32> =
        crate::world_objects::world_object_networking::shims::player_character(o)
            .expect("ACE: Player.Character is null (NullReferenceException)")
            .character_properties_title_book
            .iter()
            .map(|t| t.title_id)
            .collect();

    o.set_num_character_titles(Some(i32::try_from(titles.len()).unwrap_or(i32::MAX)));
    // `NumCharacterTitles` is the count just set, which is the list's own length.
    debug_assert_eq!(
        o.num_character_titles()
            .and_then(|n| usize::try_from(n).ok()),
        Some(titles.len())
    );
    let body = dereth_protocol::social::CharacterTitlesMessage {
        version: 1,
        display_title: display_title.cast_unsigned(),
        titles,
    };
    msg.write_proto(&body);
    msg
}
