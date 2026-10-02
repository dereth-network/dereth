// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Character.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Character.cs`.
//!
//! The Player members over its shard `Character` (`Player.Character`, in `PlayerFields`):
//! character options,
//! friends, shortcuts, spell bars, titles and the barber. `CharacterDatabaseLock` is not needed:
//! only the world thread touches the Character.

use dereth_assets::tables::{
    CharGen, EyeStrip, HairStyle, HeritageGroup as HeritageGroupCG, ObjDesc, SexCg,
};
use empyrean_common::dotnet::CsCast;
use empyrean_dat::file_types::{EnumMapper, PaletteSet};
use empyrean_entity::enums::{
    CharacterOption, CharacterTitle, ChatMessageType, Gender, HeritageGroup, PropertyDataId,
    SetupConst, SpellId,
};
use empyrean_entity::spell_bar_positions::SpellBarPositions;
use empyrean_entity::ObjectGuid;
use empyrean_store::models::shard::Character;

use crate::managers::player_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_friends_list_update::{
    game_event_friends_list_update_one, FriendsUpdateTypeFlag,
};
use crate::network::game_event::events::game_event_start_barber::game_event_start_barber;
use crate::network::game_event::events::game_event_update_title::game_event_update_title;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_obj_desc_event::game_message_obj_desc_event;
use crate::network::game_messages::messages::game_message_private_update_data_id::game_message_private_update_data_id;
use crate::network::structure::shortcut::Shortcut;
use crate::world_objects::managers::contract_manager::ContractManager;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// Non-property fields declared in `Player_Character.cs`.
#[derive(Debug, Default)]
pub struct PlayerCharacterFields {
    // ACE: Player.ContractManager
    /// HOSTED: ACE declares `public ContractManager ContractManager;` in `Player.cs` (the Player
    /// constructor builds it). Move it to `PlayerFields` when `Player.cs` takes it.
    pub contract_manager: ContractManager,
}

// ---- helpers ------------------------------------------------------------------------------------

fn object(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects.get(this).expect("ACE: this is null")
}

fn object_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(this).expect("ACE: this is null")
}

/// `Character` (a missing Character is ACE's `NullReferenceException`).
fn character(w: &World, this: ObjectGuid) -> &Character {
    shims::player_character(object(w, this))
        .expect("ACE: Player.Character is null (NullReferenceException)")
}

fn character_mut(w: &mut World, this: ObjectGuid) -> &mut Character {
    object_mut(w, this)
        .player
        .as_mut()
        .and_then(|p| p.player.character.as_mut())
        .expect("ACE: Player.Character is null (NullReferenceException)")
}

/// `CharacterChangesDetected = true;`
fn set_character_changes_detected(w: &mut World, this: ObjectGuid) {
    set_character_changes_detected_object(object_mut(w, this));
}

/// `CharacterChangesDetected = true;` on the player object itself.
fn set_character_changes_detected_object(o: &mut WorldObject) {
    o.player
        .as_mut()
        .expect("a player")
        .player_database
        .character_changes_detected = true;
}

/// Runs `f` over the player object and its `Character` together (the Character lives inside the
/// object, so it is lent out for the call).
fn with_character<R>(
    w: &mut World,
    this: ObjectGuid,
    f: impl FnOnce(&mut WorldObject, &mut Character) -> R,
) -> R {
    let o = object_mut(w, this);
    let mut character = o
        .player
        .as_mut()
        .and_then(|p| p.player.character.take())
        .expect("ACE: Player.Character is null (NullReferenceException)");
    let result = f(o, &mut character);
    o.player.as_mut().expect("a player").player.character = Some(character);
    result
}

/// `Session`.
fn session(w: &World, this: ObjectGuid) -> Option<empyrean_net::SessionId> {
    shims::player_session(w, this)
}

fn session_expect(w: &World, this: ObjectGuid) -> empyrean_net::SessionId {
    session(w, this).expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `Player.SpellIsKnown(spellId)` (`Player_Spells.cs`).
fn spell_is_known(w: &World, this: ObjectGuid, spell_id: u32) -> bool {
    crate::world_objects::player_spells::spell_is_known(w, this, spell_id)
}

// =====================================
// Character Options
// =====================================

// ACE: Player.GetCharacterOption
#[must_use]
pub fn get_character_option(w: &World, this: ObjectGuid, option: CharacterOption) -> bool {
    if let Some(option1) = option.character_options1() {
        return get_character_options1(w, this, option1.0);
    }

    get_character_options2(
        w,
        this,
        option
            .character_options2()
            .expect("ACE: GetCharacterOptions2Attribute is null (NullReferenceException)")
            .0,
    )
}

// ACE: Player.GetCharacterOptions1
fn get_character_options1(w: &World, this: ObjectGuid, option: u32) -> bool {
    (character(w, this).character_options_1 & option.cast_signed()) != 0
}

// ACE: Player.GetCharacterOptions2
fn get_character_options2(w: &World, this: ObjectGuid, option: u32) -> bool {
    (character(w, this).character_options_2 & option.cast_signed()) != 0
}

// ACE: Player.SetCharacterOption
pub fn set_character_option(w: &mut World, this: ObjectGuid, option: CharacterOption, value: bool) {
    if let Some(option1) = option.character_options1() {
        set_character_options1_option(w, this, option1.0, value);
    } else {
        set_character_options2_option(
            w,
            this,
            option
                .character_options2()
                .expect("ACE: GetCharacterOptions2Attribute is null (NullReferenceException)")
                .0,
            value,
        );
    }
}

// ACE: Player.SetCharacterOptions1
/// `SetCharacterOptions1(CharacterOptions1 option, bool value)`.
fn set_character_options1_option(w: &mut World, this: ObjectGuid, option: u32, value: bool) {
    let mut options = character(w, this).character_options_1;

    if value {
        options |= option.cast_signed();
    } else {
        options &= !option.cast_signed();
    }

    set_character_options1(w, this, options);
}

// ACE: Player.SetCharacterOptions2
/// `SetCharacterOptions2(CharacterOptions2 option, bool value)`.
fn set_character_options2_option(w: &mut World, this: ObjectGuid, option: u32, value: bool) {
    let mut options = character(w, this).character_options_2;

    if value {
        options |= option.cast_signed();
    } else {
        options &= !option.cast_signed();
    }

    set_character_options2(w, this, options);
}

// ACE: Player.SetCharacterOptions1
/// `SetCharacterOptions1(int value)`.
pub fn set_character_options1(w: &mut World, this: ObjectGuid, value: i32) {
    character_mut(w, this).character_options_1 = value;
    set_character_changes_detected(w, this);
}

// ACE: Player.SetCharacterOptions2
/// `SetCharacterOptions2(int value)`.
pub fn set_character_options2(w: &mut World, this: ObjectGuid, value: i32) {
    character_mut(w, this).character_options_2 = value;
    set_character_changes_detected(w, this);
}

// ACE: Player.SetCharacterGameplayOptions
pub fn set_character_gameplay_options(w: &mut World, this: ObjectGuid, value: Vec<u8>) {
    character_mut(w, this).gameplay_options = Some(value);
    set_character_changes_detected(w, this);
}

// =====================================
// Friends
// =====================================

/// `string.Equals(a, b, StringComparison.CurrentCultureIgnoreCase)` (en-US): case-insensitive by
/// the Unicode lower-case mapping.
fn current_culture_ignore_case_eq(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

// ACE: Player.HandleActionAddFriend
/// Adds a friend and updates the database.
pub fn handle_action_add_friend(w: &mut World, this: ObjectGuid, friend_name: &str) {
    let session = session(w, this);
    let name = crate::dispatch::name::name(w, this).unwrap_or_default();
    if current_culture_ignore_case_eq(friend_name, &name) {
        crate::network::chat_packet::send_server_message(
            w,
            session,
            "Sorry, but you can't be friends with yourself.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    // get friend player info
    let (friend, _) = player_manager::find_by_name(w, friend_name);

    let Some(friend) = friend else {
        crate::network::chat_packet::send_server_message(
            w,
            session,
            "That character does not exist",
            ChatMessageType::Broadcast,
        );
        return;
    };
    let friend_guid = friend.guid().full();
    let friend_display_name = crate::entity::i_player::name(w, friend).unwrap_or_default();

    let (new_friend, friend_already_exists) = {
        let (row, exists) = character_mut(w, this).add_friend(friend_guid);
        (row.friend_id, exists)
    };

    if friend_already_exists {
        crate::network::chat_packet::send_server_message(
            w,
            session,
            "That character is already in your friends list",
            ChatMessageType::Broadcast,
        );
        return;
    }

    set_character_changes_detected(w, this);

    // send network message
    let s = session_expect(w, this);
    let msg = game_event_friends_list_update_one(
        w,
        s,
        FriendsUpdateTypeFlag::FriendAdded,
        new_friend,
        false,
        false,
    );
    enqueue_send(w, s, msg);

    crate::network::chat_packet::send_server_message(
        w,
        session,
        &format!("{friend_display_name} has been added to your friends list."),
        ChatMessageType::Broadcast,
    );
}

// ACE: Player.HandleActionRemoveFriend
/// Remove a single friend and update the database.
pub fn handle_action_remove_friend(w: &mut World, this: ObjectGuid, friend_guid: u32) {
    let session = session(w, this);
    let Some(friend_to_remove) = character_mut(w, this).try_remove_friend(friend_guid) else {
        crate::network::chat_packet::send_server_message(
            w,
            session,
            "That character is not in your friends list!",
            ChatMessageType::Broadcast,
        );
        return;
    };

    set_character_changes_detected(w, this);

    // send network message
    let s = session_expect(w, this);
    let msg = game_event_friends_list_update_one(
        w,
        s,
        FriendsUpdateTypeFlag::FriendRemoved,
        friend_to_remove.friend_id,
        false,
        false,
    );
    enqueue_send(w, s, msg);

    // get friend player info
    let (friend, _) = player_manager::find_by_guid(w, friend_to_remove.friend_id);

    match friend {
        // shouldn't happen
        None => crate::network::chat_packet::send_server_message(
            w,
            session,
            "Friend has been removed from your friends list.",
            ChatMessageType::Broadcast,
        ),
        Some(friend) => {
            let friend_name = crate::entity::i_player::name(w, friend).unwrap_or_default();
            crate::network::chat_packet::send_server_message(
                w,
                session,
                &format!("{friend_name} has been removed from your friends list."),
                ChatMessageType::Broadcast,
            );
        }
    }
}

// ACE: Player.HandleActionRemoveAllFriends
/// Delete all friends and update the database.
pub fn handle_action_remove_all_friends(w: &mut World, this: ObjectGuid) {
    // Remove all from DB
    if character_mut(w, this).clear_all_friends() {
        //ChatPacket.SendServerMessage(Session, "Your friends list has been cleared.", ChatMessageType.Broadcast);
        set_character_changes_detected(w, this);
    }
}

// ACE: Player.GetAppearOffline
#[must_use]
pub fn get_appear_offline(w: &World, this: ObjectGuid) -> bool {
    get_character_option(w, this, CharacterOption::AppearOffline)
}

// ACE: Player.SetAppearOffline
/// Set the AppearOffline option to the provided value. It will also send out an update to all
/// online clients that have this player as a friend. This option does not save to the database.
pub fn set_appear_offline(w: &mut World, this: ObjectGuid, appear_offline: bool) {
    let previous_appear_offline = get_appear_offline(w, this);
    set_character_option(w, this, CharacterOption::AppearOffline, appear_offline);
    let now_appear_offline = get_appear_offline(w, this);
    crate::world_objects::player_networking::send_friend_status_updates(
        w,
        this,
        !previous_appear_offline,
        !now_appear_offline,
    );
}

// =====================================
// CharacterPropertiesShortcutBar
// =====================================

// ACE: Player.GetShortcuts
#[must_use]
pub fn get_shortcuts(w: &World, this: ObjectGuid) -> Vec<Shortcut> {
    let mut shortcuts = Vec::new();

    for shortcut in character(w, this).get_shortcuts() {
        shortcuts.push(Shortcut::from_shortcut_bar(&shortcut));
    }

    shortcuts
}

// ACE: Player.HandleActionAddShortcut
/// Handles the adding of items to 1-9 shortcut bar in lower-right corner. Note that there are two
/// rows. The top row is 1-9, the bottom row has no hotkeys.
///
/// `shortcut` is `dereth-protocol`'s decode of ACE's `Shortcut` (same 12 bytes; ACE splits the last
/// dword into the spell id and layer); only `Index` and `ObjectId` are read.
pub fn handle_action_add_shortcut(
    w: &mut World,
    this: ObjectGuid,
    shortcut: dereth_protocol::login::ShortCutData,
) {
    // When a shortcut is added on top of an existing item, the client automatically sends the RemoveShortcut command for that existing item first, then will add the new item, and re-add the existing item to the appropriate place.

    let index: u32 = shortcut.index.cs_cast();
    character_mut(w, this).add_or_update_shortcut(index, shortcut.object_id.0);
    set_character_changes_detected(w, this);
}

// ACE: Player.HandleActionRemoveShortcut
/// Handles the removing of items from 1-9 shortcut bar in lower-right corner
pub fn handle_action_remove_shortcut(w: &mut World, this: ObjectGuid, index: u32) {
    if character_mut(w, this).try_remove_shortcut(index).is_some() {
        set_character_changes_detected(w, this);
    }
}

// =====================================
// Spell Bar
// =====================================

// ACE: Player.GetSpellsInSpellBar
/// Will return the spells in the bar, sorted by their position
#[must_use]
pub fn get_spells_in_spell_bar(w: &World, this: ObjectGuid, bar_id: i32) -> Vec<SpellBarPositions> {
    let mut spells = Vec::new();

    let results = character(w, this).get_spells_in_bar(bar_id);

    for result in results {
        let entity = SpellBarPositions::new(
            result.spell_bar_number,
            result.spell_bar_index,
            result.spell_id,
        );

        spells.push(entity);
    }

    //spells.Sort((a, b) => a.SpellBarPositionId.CompareTo(b.SpellBarPositionId));

    spells
}

// ACE: Player.HandleActionAddSpellFavorite
/// This method implements player spell bar management for - adding a spell to a specific spell
/// bar (0 based) at a specific slot (0 based).
pub fn handle_action_add_spell_favorite(
    w: &mut World,
    this: ObjectGuid,
    spell_id: u32,
    spell_bar_position_id: u32,
    spell_bar_id: u32,
) {
    if spell_bar_id > 7
        || spell_bar_position_id > SpellId::NumSpells.0
        || !spell_is_known(w, this, spell_id)
    {
        return;
    }

    if character_mut(w, this).add_spell_to_bar(spell_bar_id, spell_bar_position_id, spell_id) {
        set_character_changes_detected(w, this);
    }
}

// ACE: Player.HandleActionRemoveSpellFavorite
/// This method implements player spell bar management for - removing a spell to a specific spell
/// bar (0 based)
pub fn handle_action_remove_spell_favorite(
    w: &mut World,
    this: ObjectGuid,
    spell_id: u32,
    spell_bar_id: u32,
) {
    if character_mut(w, this)
        .try_remove_spell_from_bar(spell_bar_id, spell_id)
        .is_some()
    {
        set_character_changes_detected(w, this);
    }
}

// =====================================
// CharacterPropertiesTitleBook
// =====================================

// ACE: Player.AddTitle
/// Add Title to Title Registry. `set_as_display_title` (default false): make this the player's
/// current title.
pub fn add_title(w: &mut World, this: ObjectGuid, title_id: u32, set_as_display_title: bool) {
    if !CharacterTitle(title_id).is_defined() {
        return;
    }

    // DIVERGE: a world without titles (`EraFeatures::titles`) grants none and sets none as the
    // one shown, whether an emote, character creation or the player's choice asks (V427).
    if !w.era.features.titles {
        return;
    }

    let (send_msg, notify_new_title) = with_character(w, this, |o, character| {
        add_title_registry(o, character, title_id, set_as_display_title)
    });

    if send_msg && object(w, this).first_enter_world_done() {
        let s = session_expect(w, this);
        let data = w
            .sessions
            .get_mut(s)
            .expect("ACE: Player.Session is null (NullReferenceException)");
        let msg = game_event_update_title(data, title_id, set_as_display_title);
        enqueue_send(w, s, msg);

        if notify_new_title {
            let data = w
                .sessions
                .get_mut(s)
                .expect("ACE: Player.Session is null (NullReferenceException)");
            let msg = game_event_communication_transient_string(
                data,
                "You have been granted a new title.",
            );
            enqueue_send(w, s, msg);
        }
    }
}

/// `AddTitle`'s body up to its messages: the registry, `NumCharacterTitles` and the display
/// title. Answers `(sendMsg, notifyNewTitle)`.
fn add_title_registry(
    o: &mut WorldObject,
    character: &mut Character,
    title_id: u32,
    set_as_display_title: bool,
) -> (bool, bool) {
    let (title_already_exists, num_character_titles) = character.add_title_to_registry(title_id);

    let mut send_msg = false;
    let mut notify_new_title = false;

    if !title_already_exists {
        set_character_changes_detected_object(o);

        o.set_num_character_titles(Some(num_character_titles));

        send_msg = true;
        notify_new_title = true;
    }

    // `CharacterTitleId != titleId`: an `int?` against a `uint`, lifted to `long?`.
    if set_as_display_title && o.character_title_id().map(i64::from) != Some(i64::from(title_id)) {
        o.set_character_title_id(Some(title_id.cast_signed()));
        send_msg = true;
    }

    (send_msg, notify_new_title)
}

// ACE: Player.AddTitle
/// `AddTitle(CharacterTitle title, bool setAsDisplayTitle = false)`.
pub fn add_title_enum(
    w: &mut World,
    this: ObjectGuid,
    title: CharacterTitle,
    set_as_display_title: bool,
) {
    add_title(w, this, title.0, set_as_display_title);
}

// ACE: Player.HandleActionSetTitle
pub fn handle_action_set_title(w: &mut World, this: ObjectGuid, title: u32) {
    add_title(w, this, title, true);
}

// ACE: Player.SetTitle
pub fn set_title(w: &mut World, this: ObjectGuid, title: CharacterTitle) {
    add_title_enum(w, this, title, true);
}

/// `EnumMapper_CharacterTitle_FileID`.
pub const ENUM_MAPPER_CHARACTER_TITLE_FILE_ID: u32 = 0x2200_0041;

// ACE: Player.GetTitle
/// The title's display text, or `None`.
#[must_use]
pub fn get_title(w: &World, title: CharacterTitle) -> Option<String> {
    // ReadFromDat hands back an empty mapper for a missing file (no entries): `None` here.
    let title_enums = w
        .dats
        .portal_dat()
        .read_from_dat::<EnumMapper>(ENUM_MAPPER_CHARACTER_TITLE_FILE_ID)?;
    // `IdToStringMap.TryGetValue`: the map's keys are unique.
    let title_enum = title_enums
        .id_to_string
        .iter()
        .find(|(id, _)| *id == title.0)
        .map(|(_, s)| s)?;

    let hash = empyrean_dat::file_types::spell_table::compute_hash(title_enum);

    let entry = w
        .dats
        .language_dat()
        .try_character_titles()?
        .strings
        .iter()
        .find(|(id, _)| *id == hash)
        .map(|(_, e)| e)?;

    entry.strings.first().cloned()
}

// =====================================
// Barber
// =====================================

// ACE: Player.StartBarber
pub fn start_barber(w: &mut World, this: ObjectGuid) {
    object_mut(w, this).set_barber_active(true);
    let s = session_expect(w, this);
    let msg = game_event_start_barber(w, s);
    enqueue_send(w, s, msg);
}

/// `int?` to a dictionary key: `(uint)Heritage` / `(int)Gender` on a null value is ACE's
/// `InvalidOperationException`.
fn nullable_value(v: Option<i32>, what: &str) -> i32 {
    v.unwrap_or_else(|| {
        panic!("ACE: {what} is null (InvalidOperationException: Nullable object must have a value)")
    })
}

/// `Heritage == (int)HeritageGroup.X || ...` for the three heritages whose "hair style" is a body
/// style with no head object or hair texture.
fn is_bodystyle_heritage(heritage: Option<i32>) -> bool {
    heritage == Some(HeritageGroup::Gearknight.0)
        || heritage == Some(HeritageGroup::Olthoi.0)
        || heritage == Some(HeritageGroup::OlthoiAcid.0)
}

// ACE: Player.HandleActionFinishBarber
/// ACE hands over the `ClientMessage`: this method reads the barber settings from the payload.
#[allow(clippy::too_many_lines)]
pub fn handle_action_finish_barber(
    w: &mut World,
    this: ObjectGuid,
    message: &mut crate::network::managers::inbound_message_manager::Payload<'_>,
) -> crate::network::managers::inbound_message_manager::HandlerResult {
    if !object(w, this).barber_active() {
        return Ok(());
    }

    // Read the payload sent from the client...
    let requested_palette_base_id = message.read_u32()?;
    let requested_head_object_did = message.read_u32()?;
    let requested_character_hair_texture = message.read_u32()?;
    let requested_character_default_hair_texture = message.read_u32()?;
    let requested_eyes_texture_did = message.read_u32()?;
    let requested_default_eyes_texture_did = message.read_u32()?;
    let requested_nose_texture_did = message.read_u32()?;
    let requested_default_nose_texture_did = message.read_u32()?;
    let requested_mouth_texture_did = message.read_u32()?;
    let requested_default_mouth_texture_did = message.read_u32()?;
    let requested_skin_palette_did = message.read_u32()?;
    let requested_hair_palette_did = message.read_u32()?;
    let requested_eyes_palette_did = message.read_u32()?;
    let requested_setup_table_id = message.read_u32()?;

    let option_bound = message.read_u32()?; // Supress Levitation - Empyrean Only
    let _option_unk = message.read_u32()?; // Unknown - Possibly set aside for future use?

    let dats = std::sync::Arc::clone(&w.dats);
    let char_gen: &CharGen = dats.portal_dat().char_gen();
    let heritage = object(w, this).heritage();
    let gender = object(w, this).gender();
    let heritage_key: u32 = nullable_value(heritage, "Heritage").cs_cast();
    let heritage_group = char_gen
        .heritage_groups
        .get(&heritage_key)
        .unwrap_or_else(|| panic!("ACE: HeritageGroups[{heritage_key}] (KeyNotFoundException)"));
    let gender_key: u32 = nullable_value(gender, "Gender").cs_cast();
    let sex = heritage_group
        .sexes
        .get(&gender_key)
        .unwrap_or_else(|| panic!("ACE: Genders[{gender_key}] (KeyNotFoundException)"));

    let valid_palette_base = sex.base_palette.0 == requested_palette_base_id;

    let (valid_head_object, validated_hair_style) =
        validate_hair_style(heritage, requested_head_object_did, &sex.hair_styles);
    let is_bald = validated_hair_style.is_some_and(|h| h.bald != 0);

    let valid_eyes_texture =
        validate_eye_texture(requested_eyes_texture_did, &sex.eye_strips, false, is_bald);

    let valid_default_eyes_texture = validate_eye_texture(
        requested_default_eyes_texture_did,
        &sex.eye_strips,
        true,
        is_bald,
    );

    let valid_nose_texture =
        validate_face_texture(requested_nose_texture_did, &sex.nose_strips, false);

    let valid_default_nose_texture =
        validate_face_texture(requested_default_nose_texture_did, &sex.nose_strips, true);

    let valid_mouth_texture =
        validate_face_texture(requested_mouth_texture_did, &sex.mouth_strips, false);

    let valid_default_mouth_texture =
        validate_face_texture(requested_default_mouth_texture_did, &sex.mouth_strips, true);

    let valid_character_hair_texture = validate_hair_texture(
        heritage,
        requested_character_hair_texture,
        validated_hair_style,
        false,
    );

    let valid_character_default_hair_texture = validate_hair_texture(
        heritage,
        requested_character_default_hair_texture,
        validated_hair_style,
        true,
    );

    let valid_skin_palette =
        validate_skin_palette(w, requested_skin_palette_did, sex.skin_palset.0);

    let valid_eyes_palette = validate_eyes_palette(requested_eyes_palette_did, &sex.eye_colors);

    let valid_hair_palette = validate_hair_palette(w, requested_hair_palette_did, &sex.hair_colors);

    let valid_setup_table = validate_setup_table(
        heritage,
        gender,
        requested_setup_table_id,
        heritage_group,
        sex,
        validated_hair_style,
    );

    let valid_change_requested = valid_palette_base
        && valid_head_object
        && valid_eyes_texture
        && valid_default_eyes_texture
        && valid_eyes_palette
        && valid_nose_texture
        && valid_default_nose_texture
        && valid_mouth_texture
        && valid_default_mouth_texture
        && valid_character_hair_texture
        && valid_character_default_hair_texture
        && valid_hair_palette
        && valid_skin_palette
        && valid_setup_table;

    //var previousSetupTableId = SetupTableId;
    //var previousMotionTableId = MotionTableId;

    if !valid_change_requested {
        // Don't know what, if anything, to send to player, so silently failing for now.
        //SendTransientError("The barber cannot do what you requested.");
        object_mut(w, this).set_barber_active(false);
        return Ok(());
    }

    let o = object_mut(w, this);
    if requested_palette_base_id > 0 {
        o.set_palette_base_id(Some(requested_palette_base_id));
    }

    if requested_head_object_did > 0 {
        o.set_head_object_did(Some(requested_head_object_did));
    }

    if requested_character_hair_texture > 0 {
        character_mut(w, this).hair_texture = requested_character_hair_texture;
        set_character_changes_detected(w, this);
    }
    if requested_character_default_hair_texture > 0 {
        character_mut(w, this).default_hair_texture = requested_character_default_hair_texture;
        set_character_changes_detected(w, this);
    }

    let o = object_mut(w, this);
    if requested_eyes_texture_did > 0 {
        o.set_eyes_texture_did(Some(requested_eyes_texture_did));
    }
    if requested_default_eyes_texture_did > 0 {
        o.set_default_eyes_texture_did(Some(requested_default_eyes_texture_did));
    }

    if requested_nose_texture_did > 0 {
        o.set_nose_texture_did(Some(requested_nose_texture_did));
    }
    if requested_default_nose_texture_did > 0 {
        o.set_default_nose_texture_did(Some(requested_default_nose_texture_did));
    }

    if requested_mouth_texture_did > 0 {
        o.set_mouth_texture_did(Some(requested_mouth_texture_did));
    }
    if requested_default_mouth_texture_did > 0 {
        o.set_default_mouth_texture_did(Some(requested_default_mouth_texture_did));
    }

    if requested_skin_palette_did > 0 {
        o.set_skin_palette_did(Some(requested_skin_palette_did));
    }

    if requested_hair_palette_did > 0 {
        o.set_hair_palette_did(Some(requested_hair_palette_did));
    }

    if requested_eyes_palette_did > 0 {
        o.set_eyes_palette_did(Some(requested_eyes_palette_did));
    }

    if requested_setup_table_id > 0 {
        o.set_setup_table_id(requested_setup_table_id);
    }

    // Check if Character is Empyrean, and if we need to set/change/send new motion table
    if heritage == Some(HeritageGroup::Empyrean.0) {
        // These are the motion tables for Empyrean float and not-float (one for each gender). They are hard-coded into the client.
        const EMPYREAN_MALE_FLOAT_MOTION_DID: u32 = 0x0900_020B;
        const EMPYREAN_FEMALE_FLOAT_MOTION_DID: u32 = 0x0900_020A;
        const EMPYREAN_MALE_MOTION_DID: u32 = 0x0900_020E;
        const EMPYREAN_FEMALE_MOTION_DID: u32 = 0x0900_020D;

        // Check for the Levitation option for Empyrean. Shadow crown and Undead flames are handled by client.
        let (motion_did, float_motion_did) = if gender == Some(Gender::Male.0) {
            (EMPYREAN_MALE_MOTION_DID, EMPYREAN_MALE_FLOAT_MOTION_DID) // Male
        } else {
            (EMPYREAN_FEMALE_MOTION_DID, EMPYREAN_FEMALE_FLOAT_MOTION_DID) // Female
        };
        let motion_table_id = object(w, this).motion_table_id();
        let new_motion_table_id = if option_bound == 1 && motion_table_id != motion_did {
            Some(motion_did)
        } else if option_bound == 0 && motion_table_id != float_motion_did {
            Some(float_motion_did)
        } else {
            None
        };
        if let Some(id) = new_motion_table_id {
            let o = object_mut(w, this);
            o.set_motion_table_id(id);
            let value = o.motion_table_id();
            let msg = game_message_private_update_data_id(o, PropertyDataId::MotionTable, value);
            let s = session_expect(w, this);
            enqueue_send(w, s, msg);
        }
    }

    // Broadcast updated character appearance
    let msg = game_message_obj_desc_event(w, this);
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);

    // The following code provides for updated [no]flame/[no]crown/[no]hover setups to be seen by others immediately without need for the player to log out/in
    // however it creates movement desync which must be the result of using UpdateObject message.
    // We don't have (that I could find) pcaps of barber changes from observer perspective so I'm uncertain if the visual desync between setup changes was resolved by some other method
    //
    //if (previousSetupTableId != SetupTableId || previousMotionTableId != MotionTableId)
    //{
    //    EnqueueBroadcast(false, new GameMessageUpdateObject(this));
    //    Session.Network.EnqueueSend(new GameMessageObjDescEvent(this));
    //}
    //else
    //    EnqueueBroadcast(new GameMessageObjDescEvent(this));

    object_mut(w, this).set_barber_active(false);
    Ok(())
}

// ACE: Player.ValidateSetupTable
#[allow(clippy::if_same_then_else)] // ACE's shape
fn validate_setup_table(
    heritage: Option<i32>,
    gender: Option<i32>,
    requested_setup_table_id: u32,
    _heritage_group: &HeritageGroupCG,
    sex: &SexCg,
    valid_hair_style: Option<&HairStyle>,
) -> bool {
    if valid_hair_style
        .is_some_and(|h| h.alternate_setup.0 > 0 && h.alternate_setup.0 == requested_setup_table_id)
    {
        return true;
    } else if sex.setup.0 == requested_setup_table_id {
        return true;
    }
    //else if (heritageGroup.SetupID == requestedSetupTableId)
    //    return true;

    let heritage = HeritageGroup(nullable_value(heritage, "Heritage"));
    let gender = Gender(nullable_value(gender, "Gender"));
    valid_heritage_setups(heritage, gender)
        .is_some_and(|setups| setups.contains(&requested_setup_table_id))
}

/// `ValidHeritageSetups`: Valid Setups defined in acclient that are not defined in the dat files.
fn valid_heritage_setups(heritage: HeritageGroup, gender: Gender) -> Option<&'static [u32]> {
    const SHADOWBOUND_MALE: &[u32] = &[
        SetupConst::UmbraenMaleCrown.0,
        SetupConst::UmbraenMaleNoCrown.0,
    ];
    const SHADOWBOUND_FEMALE: &[u32] = &[
        SetupConst::UmbraenFemaleCrown.0,
        SetupConst::UmbraenFemaleNoCrown.0,
    ];
    const PENUMBRAEN_MALE: &[u32] = &[
        SetupConst::PenumbraenMaleCrown.0,
        SetupConst::PenumbraenMaleNoCrown.0,
    ];
    const PENUMBRAEN_FEMALE: &[u32] = &[
        SetupConst::PenumbraenFemaleCrown.0,
        SetupConst::PenumbraenFemaleNoCrown.0,
    ];
    const UNDEAD_MALE: &[u32] = &[
        SetupConst::UndeadMaleSkeleton.0,
        SetupConst::UndeadMaleSkeletonNoFlame.0,
        SetupConst::UndeadMaleZombie.0,
        SetupConst::UndeadMaleZombieNoFlame.0,
    ];
    const UNDEAD_FEMALE: &[u32] = &[
        SetupConst::UndeadFemaleSkeleton.0,
        SetupConst::UndeadFemaleSkeletonNoFlame.0,
        SetupConst::UndeadFemaleZombie.0,
        SetupConst::UndeadFemaleZombieNoFlame.0,
    ];

    match (heritage, gender) {
        (HeritageGroup::Shadowbound, Gender::Male) => Some(SHADOWBOUND_MALE),
        (HeritageGroup::Shadowbound, Gender::Female) => Some(SHADOWBOUND_FEMALE),
        (HeritageGroup::Penumbraen, Gender::Male) => Some(PENUMBRAEN_MALE),
        (HeritageGroup::Penumbraen, Gender::Female) => Some(PENUMBRAEN_FEMALE),
        (HeritageGroup::Undead, Gender::Male) => Some(UNDEAD_MALE),
        (HeritageGroup::Undead, Gender::Female) => Some(UNDEAD_FEMALE),
        _ => None,
    }
}

// ACE: Player.ValidateHairStyle
/// `(valid, validHairStyle)`.
fn validate_hair_style(
    heritage: Option<i32>,
    requested_head_object_did: u32,
    hair_style_list: &[HairStyle],
) -> (bool, Option<&HairStyle>) {
    //var validHairStyles = hairStyleList.Where(h => h.ObjDesc.AnimPartChanges[0].PartID == requestedHeadObjectDID).ToList();

    if requested_head_object_did == 0 && is_bodystyle_heritage(heritage) {
        return (true, None);
    }

    for hair_style in hair_style_list {
        for anim_part_change in &hair_style.objdesc.anim_part_changes {
            if anim_part_change.1 .0 == requested_head_object_did {
                return (true, Some(hair_style));
            }
        }
    }

    (false, None)
}

/// `TextureChanges` holds `requested` as the old (`compareOldTexture`) or new texture.
#[allow(clippy::if_same_then_else)] // ACE's shape
fn texture_changes_contain(obj_desc: &ObjDesc, requested: u32, compare_old_texture: bool) -> bool {
    for texture_map_change in &obj_desc.texture_changes {
        let (_, old_texture, new_texture) = texture_map_change;
        if compare_old_texture && old_texture.0 == requested {
            return true;
        } else if !compare_old_texture && new_texture.0 == requested {
            return true;
        }
    }
    false
}

// ACE: Player.ValidateEyeTexture
fn validate_eye_texture(
    requested_eyes_texture_did: u32,
    eye_strip_list: &[EyeStrip],
    compare_old_texture: bool,
    is_bald: bool,
) -> bool {
    for eye_strip in eye_strip_list {
        let obj_desc = if is_bald {
            &eye_strip.objdesc_bald
        } else {
            &eye_strip.objdesc
        };
        if texture_changes_contain(obj_desc, requested_eyes_texture_did, compare_old_texture) {
            return true;
        }
    }

    false
}

// ACE: Player.ValidateFaceTexture
fn validate_face_texture(
    requested_face_texture_did: u32,
    face_strip_list: &[(u32, ObjDesc)],
    compare_old_texture: bool,
) -> bool {
    for (_, obj_desc) in face_strip_list {
        if texture_changes_contain(obj_desc, requested_face_texture_did, compare_old_texture) {
            return true;
        }
    }

    false
}

// ACE: Player.ValidateHairTexture
fn validate_hair_texture(
    heritage: Option<i32>,
    requested_hair_texture_did: u32,
    hair_style: Option<&HairStyle>,
    compare_old_texture: bool,
) -> bool {
    if requested_hair_texture_did == 0 {
        if is_bodystyle_heritage(heritage) {
            return true;
        }
    } else if hair_style.is_none() {
        return false;
    }

    // ACE-BUG: a request with hair texture 0 and no valid hair style, from a heritage other than
    // Gearknight/Olthoi, reaches `hairStyle.ObjDesc` with a null `hairStyle`: the handler throws a
    // NullReferenceException (a crafted FinishBarber; the barber stays active).
    let hair_style = hair_style.expect("ACE: hairStyle is null (NullReferenceException)");
    texture_changes_contain(
        &hair_style.objdesc,
        requested_hair_texture_did,
        compare_old_texture,
    )
}

// ACE: Player.ValidateSkinPalette
fn validate_skin_palette(w: &World, requested_skin_palette_did: u32, palette_set: u32) -> bool {
    // ReadFromDat hands back an empty set for a missing file.
    let skin_pal_set = w.dats.portal_dat().read_from_dat::<PaletteSet>(palette_set);
    skin_pal_set.is_some_and(|s| {
        s.palette_ids
            .iter()
            .any(|p| p.0 == requested_skin_palette_did)
    })
}

// ACE: Player.ValidateEyesPalette
fn validate_eyes_palette(requested_eyes_palette_did: u32, eye_color_list: &[u32]) -> bool {
    eye_color_list.contains(&requested_eyes_palette_did)
}

// ACE: Player.ValidateHairPalette
fn validate_hair_palette(
    w: &World,
    requested_hair_palette_did: u32,
    hair_color_list: &[u32],
) -> bool {
    for hair_palette in hair_color_list {
        let hair_pal_set = w
            .dats
            .portal_dat()
            .read_from_dat::<PaletteSet>(*hair_palette);
        if hair_pal_set.is_some_and(|s| {
            s.palette_ids
                .iter()
                .any(|p| p.0 == requested_hair_palette_did)
        }) {
            return true;
        }
    }

    false
}
