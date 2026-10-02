// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Allegiance.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Allegiance.cs`.
//!
//! A player's `Allegiance` is the guid of its allegiance object, and its `AllegianceNode` the
//! guid of the allegiance whose tree holds the player's node (the node's player is the player
//! itself): see `world_objects/allegiance.rs` and `entity/allegiance_node.rs`. The `IPlayer`
//! members ACE declares on both `Player` and `OfflinePlayer` are the `i_player_*` helpers here.

use std::cmp::Ordering;

use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::format::format;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_entity::enums::{
    AllegianceLockAction, AllegianceOfficerLevel, AllegiancePermissionLevel, CharacterOption,
    ChatMessageType, Gender, MotionCommand, MotionStance, PropertyBool, PropertyInstanceId,
    PropertyInt, PropertyInt64, ShareType, WeenieError, XpType,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::allegiance_node::{self, NodeRef};
use crate::entity::allegiance_rank;
use crate::entity::confirmation::Confirmation;
use crate::entity::i_player::{self, IPlayer};
use crate::managers::{allegiance_manager, player_manager};
use crate::network::game_event::events::{
    game_event_allegiance_info_response, game_event_allegiance_login_notification,
};
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{self, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::motion::movement_data::Motion;
use crate::network::structure::allegiance_profile;
use crate::world_objects::allegiance;
use crate::world_objects::managers::confirmation_manager;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Player_Allegiance.cs`.
#[derive(Debug, Default)]
pub struct PlayerAllegianceFields {
    /// The guid of the player's allegiance object.
    // ACE: Player.Allegiance
    pub allegiance: Option<ObjectGuid>,
    /// The guid of the allegiance whose tree holds this player's node.
    // ACE: Player.AllegianceNode
    pub allegiance_node: Option<ObjectGuid>,
}

// ---------------------------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------------------------

fn fields(w: &World, this: ObjectGuid) -> Option<&PlayerAllegianceFields> {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .map(|p| &p.player_allegiance)
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> Option<&mut PlayerAllegianceFields> {
    w.objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .map(|p| &mut p.player_allegiance)
}

/// `Allegiance` of the online player `this`.
fn player_allegiance(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    i_player_allegiance(w, IPlayer::Online(this))
}

/// `AllegianceNode` of the online player `this`.
fn player_allegiance_node(w: &World, this: ObjectGuid) -> Option<NodeRef> {
    i_player_allegiance_node(w, IPlayer::Online(this))
}

/// `Name`.
fn name(w: &World, this: ObjectGuid) -> String {
    i_player::name(w, IPlayer::Online(this)).unwrap_or_default()
}

fn i_name(w: &World, p: IPlayer) -> String {
    i_player::name(w, p).unwrap_or_default()
}

/// `Session.Network.EnqueueSend(msg)` for the online player `this`.
fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let session = player_manager::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    game_message::enqueue_send(w, session, msg);
}

/// `Session.Network.EnqueueSend(new GameMessageSystemChat(text, ChatMessageType.Broadcast))`.
fn send_chat(w: &mut World, this: ObjectGuid, text: &str) {
    send(
        w,
        this,
        game_message_system_chat(text, ChatMessageType::Broadcast),
    );
}

/// `Session.Network.EnqueueSend(new GameEventWeenieError(Session, error))`.
fn send_error(w: &mut World, this: ObjectGuid, error: WeenieError) {
    crate::world_objects::player_networking::send_weenie_error(w, this, error);
}

fn allegiance_obj(w: &World, allegiance: ObjectGuid) -> &WorldObject {
    w.objects
        .get(allegiance)
        .expect("ACE: Allegiance is null (NullReferenceException)")
}

fn get_character_option(w: &World, this: ObjectGuid, option: CharacterOption) -> bool {
    crate::world_objects::player_character::get_character_option(w, this, option)
}

/// `IsOlthoiPlayer` (`Player_Properties.cs`, as `SetEphemeralValues` sets it).
fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// The experience an oath costs `this` (not ACE's, V435): on a world with
/// `EraFeatures::swear_xp_cost`, a character that has broken from a patron pays five percent of its
/// next level's experience, held between 100 and 5,000, a quarter more for each break; past the
/// curve's end the next level counts as 4,294,967,295. Without the feature, or with no break, 0.
#[must_use]
pub fn swear_xp_cost(w: &World, this: ObjectGuid) -> u32 {
    if !w.era.features.swear_xp_cost {
        return 0;
    }
    let Some(o) = w.objects.get(this) else {
        return 0;
    };
    let breaks = u32::try_from(
        o.get_property(PropertyInt::NumAllegianceBreaks)
            .unwrap_or(0),
    )
    .unwrap_or(0);
    if breaks == 0 {
        return 0;
    }
    let level = o.level().unwrap_or(1);
    let max_level = crate::world_objects::player_xp::get_max_level(w);
    let here = crate::world_objects::player_xp::get_total_xp(w, level);
    let next = if u32::try_from(level).is_ok_and(|l| l < max_level) {
        crate::world_objects::player_xp::get_total_xp(w, level.wrapping_add(1))
    } else {
        u64::from(u32::MAX)
    };
    dereth_rules::allegiance::swear_xp_cost_after_breaks(next.saturating_sub(here), breaks)
}

/// Counts a break from `this`'s own patron (not ACE's, V436), on a world with
/// `EraFeatures::swear_xp_cost`: the count prices the next oath. Dismissing a vassal is not a
/// break of the vassal's.
fn count_break_from_patron(w: &mut World, this: ObjectGuid) {
    if !w.era.features.swear_xp_cost {
        return;
    }
    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    let breaks = o
        .get_property(PropertyInt::NumAllegianceBreaks)
        .unwrap_or(0)
        .saturating_add(1);
    o.set_property(PropertyInt::NumAllegianceBreaks, breaks);
    let msg = game_message_private_update_property_int(o, PropertyInt::NumAllegianceBreaks, breaks);
    send(w, this, msg);
}

// ---------------------------------------------------------------------------------------------
// IPlayer members shared with OfflinePlayer (not ACE: the interface dispatch)
// ---------------------------------------------------------------------------------------------

/// `IPlayer.Allegiance` (get).
#[must_use]
pub fn i_player_allegiance(w: &World, p: IPlayer) -> Option<ObjectGuid> {
    match p {
        IPlayer::Online(g) => fields(w, g).and_then(|f| f.allegiance),
        IPlayer::Offline(g) => w
            .player_manager
            .offline_players
            .get(&g.full())
            .and_then(|o| o.allegiance),
    }
}

/// `IPlayer.Allegiance` (set).
pub fn set_i_player_allegiance(w: &mut World, p: IPlayer, value: Option<ObjectGuid>) {
    match p {
        IPlayer::Online(g) => {
            if let Some(f) = fields_mut(w, g) {
                f.allegiance = value;
            }
        }
        IPlayer::Offline(g) => {
            if let Some(o) = w.player_manager.offline_players.get_mut(&g.full()) {
                o.allegiance = value;
            }
        }
    }
}

/// `IPlayer.AllegianceNode` (get): the node of the player in the allegiance recorded for it.
#[must_use]
pub fn i_player_allegiance_node(w: &World, p: IPlayer) -> Option<NodeRef> {
    let allegiance = match p {
        IPlayer::Online(g) => fields(w, g).and_then(|f| f.allegiance_node),
        IPlayer::Offline(g) => w
            .player_manager
            .offline_players
            .get(&g.full())
            .and_then(|o| o.allegiance_node),
    }?;
    Some(NodeRef {
        allegiance,
        player: p.guid(),
    })
}

/// `IPlayer.AllegianceNode` (set). A node is always the player's own.
pub fn set_i_player_allegiance_node(w: &mut World, p: IPlayer, value: Option<NodeRef>) {
    debug_assert!(
        value.is_none_or(|n| n.player == p.guid()),
        "a player's node is its own"
    );
    let value = value.map(|n| n.allegiance);
    match p {
        IPlayer::Online(g) => {
            if let Some(f) = fields_mut(w, g) {
                f.allegiance_node = value;
            }
        }
        IPlayer::Offline(g) => {
            if let Some(o) = w.player_manager.offline_players.get_mut(&g.full()) {
                o.allegiance_node = value;
            }
        }
    }
}

/// `IPlayer.AllegianceOfficerRank` (get).
#[must_use]
pub fn i_player_allegiance_officer_rank(w: &World, p: IPlayer) -> Option<i32> {
    i_player::get_property(w, p, PropertyInt::AllegianceOfficerRank)
}

/// `IPlayer.AllegianceOfficerRank` (set).
pub fn i_player_set_allegiance_officer_rank(w: &mut World, p: IPlayer, value: Option<i32>) {
    match value {
        Some(v) => i_player::set_property(w, p, PropertyInt::AllegianceOfficerRank, v),
        None => i_player::remove_property(w, p, PropertyInt::AllegianceOfficerRank),
    }
}

/// `IPlayer.AllegianceXPGenerated` (get).
#[must_use]
pub fn i_player_allegiance_xp_generated(w: &World, p: IPlayer) -> u64 {
    i_player::get_property(w, p, PropertyInt64::AllegianceXPGenerated)
        .unwrap_or(0)
        .cs_cast()
}

/// `IPlayer.AllegianceXPGenerated` (set): 0 removes the property.
pub fn i_player_set_allegiance_xp_generated(w: &mut World, p: IPlayer, value: u64) {
    if value == 0 {
        i_player::remove_property(w, p, PropertyInt64::AllegianceXPGenerated);
    } else {
        i_player::set_property(w, p, PropertyInt64::AllegianceXPGenerated, value.cs_cast());
    }
}

/// `IPlayer.AllegianceXPCached` (set): 0 removes the property.
pub fn i_player_set_allegiance_xp_cached(w: &mut World, p: IPlayer, value: u64) {
    if value == 0 {
        i_player::remove_property(w, p, PropertyInt64::AllegianceXPCached);
    } else {
        i_player::set_property(w, p, PropertyInt64::AllegianceXPCached, value.cs_cast());
    }
}

/// `IPlayer.ExistedBeforeAllegianceXpChanges` (set): true removes the property.
pub fn i_player_set_existed_before_allegiance_xp_changes(w: &mut World, p: IPlayer, value: bool) {
    if value {
        i_player::remove_property(w, p, PropertyBool::ExistedBeforeAllegianceXpChanges);
    } else {
        i_player::set_property(w, p, PropertyBool::ExistedBeforeAllegianceXpChanges, value);
    }
}

/// `IPlayer.GetCurrentLoyalty()`: the creature skill's current value online, the value saved
/// at log-off offline.
pub fn i_player_get_current_loyalty(w: &mut World, p: IPlayer) -> u32 {
    match p {
        IPlayer::Online(g) => crate::world_objects::creature_skills::get_current_loyalty(w, g),
        IPlayer::Offline(g) => w.player_manager.offline_players.get(&g.full()).map_or(
            0,
            crate::entity::offline_player::OfflinePlayer::get_current_loyalty,
        ),
    }
}

/// `IPlayer.GetCurrentLeadership()`.
pub fn i_player_get_current_leadership(w: &mut World, p: IPlayer) -> u32 {
    match p {
        IPlayer::Online(g) => crate::world_objects::creature_skills::get_current_leadership(w, g),
        IPlayer::Offline(g) => w.player_manager.offline_players.get(&g.full()).map_or(
            0,
            crate::entity::offline_player::OfflinePlayer::get_current_leadership,
        ),
    }
}

/// `IPlayer.UpdateProperty(PropertyInstanceId prop, uint? value, bool broadcast)`:
/// `OfflinePlayer`'s (set or remove), or `Player.UpdateProperty(prop, value, broadcast)`.
pub fn i_player_update_property_iid(
    w: &mut World,
    p: IPlayer,
    prop: PropertyInstanceId,
    value: Option<u32>,
    broadcast: bool,
) {
    match p {
        IPlayer::Offline(g) => {
            if let Some(o) = w.player_manager.offline_players.get_mut(&g.full()) {
                o.update_property(prop, value, broadcast);
            }
        }
        IPlayer::Online(g) => {
            crate::world_objects::player_properties::update_property_instance_id_self(
                w, g, prop, value, broadcast,
            );
        }
    }
}

/// How long a vassal has been sworn to its current patron, in whole seconds: the two times its
/// allegiance hierarchy record carries, and the time term of its XP pass-up.
///
/// Not ACE's (retail captures, V285): ACE tracks neither time (its record
/// sends 0 for both and its pass-up pins the time term at its maximum).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwornTime {
    /// In-game (played) seconds sworn: the character's age now less its age when it swore.
    pub time_online: u32,
    /// Real seconds sworn: the wall clock now less the moment it swore.
    pub allegiance_age: u32,
}

impl SwornTime {
    /// Real days sworn (`allegiance_age / 86,400`).
    #[must_use]
    pub fn real_days(self) -> f64 {
        f64::from(self.allegiance_age) / 86_400.0
    }

    /// In-game hours sworn (`time_online / 3,600`).
    #[must_use]
    pub fn game_hours(self) -> f64 {
        f64::from(self.time_online) / 3_600.0
    }
}

/// The time `p` has been sworn to its current patron (zero for a player with no patron, or one
/// sworn before these times were kept).
///
/// Not ACE's (retail captures, V285): the oath stamps the unix time
/// (`PropertyFloat.AllegianceSwearTimestamp`) and the character's `Age` in played seconds
/// (`PropertyInt.AllegianceSwearTimestamp`), both saved with the character. Real time is the wall
/// clock since the first stamp; in-game time is the character's `Age` (advanced while it is
/// online, saved at log-off) since the second. Breaking leaves the stamps in place, unread while
/// the player has no patron; the next oath, to any patron, restamps both, so the times restart at
/// zero.
#[must_use]
pub fn i_player_sworn_time(w: &World, p: IPlayer) -> SwornTime {
    use empyrean_entity::enums::PropertyFloat;
    if i_player::patron_id(w, p).is_none() {
        return SwornTime::default();
    }
    let real_stamp = i_player::get_property(w, p, PropertyFloat::AllegianceSwearTimestamp);
    let game_stamp = i_player::get_property(w, p, PropertyInt::AllegianceSwearTimestamp);
    let now = w.now.unix_time;
    let age = i_player::get_property(w, p, PropertyInt::Age).unwrap_or(0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // whole seconds, clamped to the u32 range first
    let allegiance_age = real_stamp.map_or(0, |s| {
        (now - s).floor().clamp(0.0, f64::from(u32::MAX)) as u32
    });
    let time_online = game_stamp.map_or(0, |s| {
        u32::try_from((i64::from(age) - i64::from(s)).max(0)).unwrap_or(u32::MAX)
    });
    SwornTime {
        time_online,
        allegiance_age,
    }
}

/// Stamps the moment `p` swears to a new patron: its sworn times restart at zero.
///
/// Not ACE's (retail captures, V285): see [`i_player_sworn_time`].
pub fn i_player_stamp_sworn_time(w: &mut World, p: IPlayer) {
    use empyrean_entity::enums::PropertyFloat;
    let now = w.now.unix_time;
    let age = i_player::get_property(w, p, PropertyInt::Age).unwrap_or(0);
    i_player::set_property(w, p, PropertyFloat::AllegianceSwearTimestamp, now);
    i_player::set_property(w, p, PropertyInt::AllegianceSwearTimestamp, age);
}

/// .NET's culture-sensitive string order (`OrderBy(i => i.Name)` in `en-US`, which .NET 5+
/// takes from ICU), for the characters player names hold: letters compare case-insensitively
/// first, with space and punctuation before digits and digits before letters; equal strings
/// then order lower case before upper case at the first difference. Checked against .NET in
/// the `allegiance/name_order` vectors.
#[must_use]
pub fn compare_culture(a: &str, b: &str) -> Ordering {
    fn primary(c: char) -> (u32, u32) {
        match c {
            ' ' => (0, 0),
            '_' => (0, 1),
            '-' => (0, 2),
            ',' => (0, 3),
            ';' => (0, 4),
            ':' => (0, 5),
            '!' => (0, 6),
            '?' => (0, 7),
            '.' => (0, 8),
            '\'' => (0, 9),
            '"' => (0, 10),
            '0'..='9' => (1, c as u32),
            'a'..='z' => (2, c as u32),
            'A'..='Z' => (2, c.to_ascii_lowercase() as u32),
            _ => (3, c as u32),
        }
    }
    let pa: Vec<(u32, u32)> = a.chars().map(primary).collect();
    let pb: Vec<(u32, u32)> = b.chars().map(primary).collect();
    pa.cmp(&pb).then_with(|| {
        for (x, y) in a.chars().zip(b.chars()) {
            match (x.is_ascii_uppercase(), y.is_ascii_uppercase()) {
                (false, true) => return Ordering::Less,
                (true, false) => return Ordering::Greater,
                _ => {}
            }
        }
        Ordering::Equal
    })
}

// ---------------------------------------------------------------------------------------------
// Player_Allegiance.cs
// ---------------------------------------------------------------------------------------------

/// `AllegianceNode?.Rank` (`null` when the player has no node).
#[must_use]
pub fn allegiance_node_rank(w: &World, this: ObjectGuid) -> Option<u32> {
    let node = player_allegiance_node(w, this)?;
    Some(
        allegiance::tree(w, node.allegiance)?
            .node(allegiance::member_node(w, node)?)
            .rank,
    )
}

// ACE: Player.HasAllegiance
#[must_use]
pub fn has_allegiance(w: &World, this: ObjectGuid) -> bool {
    let Some(allegiance) = player_allegiance(w, this) else {
        return false;
    };
    player_allegiance_node(w, this).is_some() && allegiance::total_members(w, allegiance) > 1
}

/// `Allegiance_MaxSwearDistance` (ACE once used 4.0).
// ACE: Player.Allegiance_MaxSwearDistance
pub const ALLEGIANCE_MAX_SWEAR_DISTANCE: f32 = 2.0;

// ACE: Player.HandleActionSwearAllegiance
/// Called when a player tries to Swear Allegiance to a target
pub fn handle_action_swear_allegiance(w: &mut World, this: ObjectGuid, target_guid: u32) {
    let Some(patron) = player_manager::get_online_player(w, target_guid) else {
        return;
    };

    if !is_pledgable(w, this, patron) {
        return;
    }

    // perform moveto / turnto
    let patron_guid = patron.full();
    crate::world_objects::player_move::create_move_to_chain(
        w,
        this,
        patron,
        Box::new(move |w: &mut World, success: bool| {
            swear_allegiance(w, this, patron_guid, success, false)
        }),
        Some(ALLEGIANCE_MAX_SWEAR_DISTANCE),
        true,
    );
}

// ACE: Player.SwearAllegiance
/// `confirmed` defaults to false in ACE: the patron is asked first.
pub fn swear_allegiance(
    w: &mut World,
    this: ObjectGuid,
    target_guid: u32,
    success: bool,
    confirmed: bool,
) {
    if !success {
        return;
    }

    let Some(patron) = player_manager::get_online_player(w, target_guid) else {
        return;
    };

    if !is_pledgable(w, this, patron) {
        return;
    }

    if !confirmed {
        let self_name = name(w, this);
        if !confirmation_manager::enqueue_send(
            w,
            patron,
            Confirmation::swear_allegiance(patron, this),
            &self_name,
        ) {
            let text = format!("{} is busy.", name(w, patron));
            send_chat(w, this, &text);
        }
        return;
    }

    // DIVERGE: V435, the oath's experience cost on a world that charges it (`is_pledgable`
    // checked the character can pay).
    let cost = swear_xp_cost(w, this);
    if cost > 0 && !crate::world_objects::player_xp::spend_xp(w, this, i64::from(cost), true) {
        return;
    }

    let level = |w: &World, g: ObjectGuid| w.objects.get(g).and_then(WorldObject::level);
    log::info!(
        "[ALLEGIANCE] {} ({}) swearing allegiance to {} ({})",
        name(w, this),
        level(w, this).map_or_else(String::new, |l| l.to_string()),
        name(w, patron),
        level(w, patron).map_or_else(String::new, |l| l.to_string())
    );

    let me = IPlayer::Online(this);
    i_player::set_patron_id(w, me, Some(target_guid));

    // Not ACE's (retail captures, V285): the oath starts the vassal's
    // sworn times (real and in-game) at zero; the save below keeps them.
    i_player_stamp_sworn_time(w, me);

    let monarch_guid = allegiance_manager::get_monarch(w, IPlayer::Online(patron))
        .guid()
        .full();

    i_player_update_property_iid(w, me, PropertyInstanceId::Monarch, Some(monarch_guid), true);

    let existed = level(w, patron).unwrap_or(1) >= level(w, this).unwrap_or(1);
    i_player_set_existed_before_allegiance_xp_changes(w, me, existed);

    // handle special case: monarch swearing into another allegiance
    if let Some(a) = player_allegiance(w, this) {
        if allegiance_obj(w, a).monarch_id() == Some(this.full()) {
            handle_monarch_swear(w, this);
        }
    }

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, this, true);

    //Console.WriteLine("Patron: " + PlayerManager.GetOfflinePlayerByGuidId(Patron.Value).Name);
    //Console.WriteLine("Monarch: " + PlayerManager.GetOfflinePlayerByGuidId(Monarch.Value).Name);

    // send message to patron:
    // %vassal% has sworn Allegiance to you.
    let text = format!("{} has sworn Allegiance to you.", name(w, this));
    send_chat(w, patron, &text);

    // send message to vassal:
    // %patron% has accepted your oath of Allegiance!
    // Motion_Kneel
    let text = format!("{} has accepted your oath of Allegiance!", name(w, patron));
    send_chat(w, this, &text);

    let motion = Motion::new(MotionStance::NonCombat, MotionCommand::Kneel, 1.0);
    crate::world_objects::world_object_networking::enqueue_broadcast_motion(
        w, this, &motion, None, None,
    );

    // rebuild allegiance tree structure
    allegiance_manager::on_swear_allegiance(w, this);

    if let Some(o) = w.objects.get_mut(this) {
        o.set_allegiance_xp_generated(0);
        o.set_allegiance_officer_rank(None);
    }

    // refresh ui panel
    let (a, n) = (player_allegiance(w, this), player_allegiance_node(w, this));
    allegiance::send_allegiance_update(w, this, a, n);

    if get_character_option(w, this, CharacterOption::ListenToAllegianceChat)
        && player_allegiance(w, this).is_some()
    {
        crate::world_objects::player_networking::join_turbine_chat_channel(w, this, "Allegiance");
    }
}

// ACE: Player.HandleMonarchSwear
/// Handle monarch swearing into another allegiance
pub fn handle_monarch_swear(w: &mut World, this: ObjectGuid) {
    // walk the allegiance tree from this node, update monarch ids
    let node = player_allegiance_node(w, this)
        .expect("ACE: AllegianceNode is null (NullReferenceException)");
    let tree = allegiance::tree(w, node.allegiance)
        .expect("loaded")
        .clone();
    let id = allegiance::member_node(w, node)
        .expect("ACE: AllegianceNode is null (NullReferenceException)");
    let monarch_id = w.objects.get(this).and_then(WorldObject::monarch_id);
    for n in tree.walk(id, true) {
        let node_player = allegiance_node::player(w, tree.node(n).player_guid)
            .expect("ACE: node.Player is null (NullReferenceException)");
        i_player_update_property_iid(
            w,
            node_player,
            PropertyInstanceId::Monarch,
            monarch_id,
            true,
        );

        i_player::save_biota_to_database(w, node_player, true);

        // update node.Player.House.Monarch, if not null?
    }

    // TODO: allegiance officers should probably be stored in their own table
    let allegiance =
        player_allegiance(w, this).expect("ACE: Allegiance is null (NullReferenceException)");
    let officers: Vec<ObjectGuid> = allegiance::fields_of(w, allegiance)
        .expect("loaded")
        .officers()
        .keys()
        .copied()
        .collect();
    for key in officers {
        let officer = player_manager::find_by_guid(w, key.full()).0;
        if let Some(officer) = officer {
            i_player_set_allegiance_officer_rank(w, officer, None);
        }

        let officer = officer.expect("ACE: officer is null (NullReferenceException)");
        i_player::save_biota_to_database(w, officer, true);
    }
}

// ACE: Player.HandleActionBreakAllegiance
/// Called when a player tries to break Allegiance to a target (its patron or a vassal).
pub fn handle_action_break_allegiance(w: &mut World, this: ObjectGuid, target_guid: u32) {
    if !is_breakable(w, this, target_guid) {
        return;
    }

    let (target, target_is_online) = player_manager::find_by_guid(w, target_guid);

    let Some(target) = target else { return };

    log::info!(
        "[ALLEGIANCE] {} breaking allegiance to {}",
        name(w, this),
        i_name(w, target)
    );

    let me = IPlayer::Online(this);

    // target can be either patron or vassal
    let _is_patron = i_player::patron_id(w, me) == Some(target.guid().full());
    let is_vassal = i_player::patron_id(w, target) == Some(this.full());

    // break ties
    if is_vassal {
        // patron breaking from vassal
        i_player::set_patron_id(w, target, None);

        let allegiance =
            player_allegiance(w, this).expect("ACE: Allegiance is null (NullReferenceException)");
        let tree = allegiance::tree(w, allegiance).expect("loaded").clone();
        let target_node = allegiance::member_node(
            w,
            NodeRef {
                allegiance,
                player: target.guid(),
            },
        )
        .expect("ACE: targetNode is null (NullReferenceException)");

        let monarch_id = tree.has_vassals(target_node).then(|| target.guid().full());

        i_player_update_property_iid(w, target, PropertyInstanceId::Monarch, monarch_id, true);

        // walk the allegiance tree from this node, update monarch ids
        for node in tree.walk(target_node, false) {
            let node_player = allegiance_node::player(w, tree.node(node).player_guid)
                .expect("ACE: node.Player is null (NullReferenceException)");
            i_player_update_property_iid(
                w,
                node_player,
                PropertyInstanceId::Monarch,
                Some(target.guid().full()),
                true,
            );

            i_player::save_biota_to_database(w, node_player, true);
        }

        i_player::save_biota_to_database(w, target, true);
    } else {
        // vassal breaking from patron
        // DIVERGE: V436, the break is counted on a world that prices oaths by it.
        count_break_from_patron(w, this);
        i_player::set_patron_id(w, me, None);
        i_player_update_property_iid(w, me, PropertyInstanceId::Monarch, None, true);

        // walk the allegiance tree from this node, update monarch ids
        let node = player_allegiance_node(w, this)
            .expect("ACE: AllegianceNode is null (NullReferenceException)");
        let tree = allegiance::tree(w, node.allegiance)
            .expect("loaded")
            .clone();
        let id = allegiance::member_node(w, node)
            .expect("ACE: AllegianceNode is null (NullReferenceException)");
        for n in tree.walk(id, false) {
            let node_player = allegiance_node::player(w, tree.node(n).player_guid)
                .expect("ACE: node.Player is null (NullReferenceException)");
            i_player_update_property_iid(
                w,
                node_player,
                PropertyInstanceId::Monarch,
                Some(this.full()),
                true,
            );

            i_player::save_biota_to_database(w, node_player, true);
        }

        crate::dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
    }

    // send message to target if online
    if target_is_online {
        if let Some(online_target) = player_manager::get_online_player(w, target_guid) {
            let text = format!("{} has broken their Allegiance to you!", name(w, this));
            send_chat(w, online_target, &text);
        }
    }

    // send message to self
    let text = format!("You have broken your Allegiance to {}!", i_name(w, target));
    send_chat(w, this, &text);

    // rebuild allegiance tree structures
    allegiance_manager::on_break_allegiance(w, Some(me), Some(target));

    if is_vassal {
        // patron broke from vassal
        check_allegiance_house(w, target.guid());

        let vassal_allegiance = allegiance_manager::get_allegiance(w, Some(target));
        if let Some(vassal_allegiance) = vassal_allegiance {
            allegiance_manager::walk_check_allegiance_house(w, vassal_allegiance);
        }
    } else {
        // vassal broke from patron
        check_allegiance_house(w, this);

        if let Some(node) = player_allegiance_node(w, this) {
            if let (Some(tree), Some(id)) = (
                allegiance::tree(w, node.allegiance).cloned(),
                allegiance::member_node(w, node),
            ) {
                for n in tree.walk(id, false) {
                    check_allegiance_house(w, tree.node(n).player_guid);
                }
            }
        }
    }

    // refresh ui panel

    // move this to function below?
    let (a, n) = (player_allegiance(w, this), player_allegiance_node(w, this));
    allegiance::send_allegiance_update(w, this, a, n);
}

// ACE: Player.CheckAllegianceHouse
/// After an allegiance change: boots an online player from an allegiance house it may no longer
/// use, and refreshes its allegiance chat room.
pub fn check_allegiance_house(w: &mut World, player_guid: ObjectGuid) {
    // handle player.House.Monarch updates?

    // instead of walking, would it be more appropriate for House
    // to boot any players who don't belong there?

    let Some(player) = player_manager::get_online_player(w, player_guid.full()) else {
        return;
    };

    if player_check_house(w, player) {
        send_chat(w, player, "You have been booted from the allegiance house.");
    }

    if get_character_option(w, player, CharacterOption::ListenToAllegianceChat) {
        if player_allegiance(w, player).is_some() {
            crate::world_objects::player_networking::leave_turbine_chat_channel(
                w,
                player,
                "Allegiance",
                true,
            );
            crate::world_objects::player_networking::join_turbine_chat_channel(
                w,
                player,
                "Allegiance",
            );
        } else {
            crate::world_objects::player_networking::leave_turbine_chat_channel(
                w,
                player,
                "Allegiance",
                false,
            );
        }
    } else {
        crate::world_objects::player_networking::send_turbine_chat_channels(w, player, false);
    }
}

// ACE: Player.IsPledgable
/// Returns TRUE if this player can swear to the target guid
pub fn is_pledgable(w: &mut World, this: ObjectGuid, target: ObjectGuid) -> bool {
    // the client doesn't seem to display most of these werrors,
    // so we also send similar messages as text

    // An Olthoi player cannot swear allegiance to another player
    if is_olthoi_player(w, this) {
        //Session.Network.EnqueueSend(new GameMessageSystemChat($"The Olthoi only have an allegiance to the Olthoi Queen!", ChatMessageType.Broadcast));
        send_error(w, this, WeenieError::OlthoiCannotJoinAllegiance);
        return false;
    }

    if is_olthoi_player(w, target) {
        send_chat(
            w,
            this,
            "The Olthoi have loyalty only to their Olthoi Queen!",
        );
        send_error(w, this, WeenieError::None);
        return false;
    }

    // check ignore allegiance requests
    if get_character_option(w, target, CharacterOption::IgnoreAllegianceRequests) {
        send_chat(w, this, "Your offer of allegiance was ignored.");
        send_error(w, this, WeenieError::YourOfferOfAllegianceWasIgnored);
        return false;
    }

    // player already sworn?
    if i_player::patron_id(w, IPlayer::Online(this)).is_some() {
        //Console.WriteLine(Name + " tried to swear to " + target.Name + ", but is already sworn to " + PlayerManager.GetOfflinePlayerByGuidId(Patron.Value).Name);
        send_chat(w, this, "You've already sworn allegiance.");
        send_error(w, this, WeenieError::YouveAlreadySwornAllegiance);
        return false;
    }

    // player can't swear to themselves
    if target == this {
        //Console.WriteLine(Name + " tried to swear to themselves");
        send_chat(w, this, "You cannot swear allegiance to yourself.");
        return false;
    }

    // DIVERGE: an era in which a patron could not be of lower level
    // (`EraFeatures::swear_to_lower_level` off) refuses, as the commented-out check below would
    // (ClassicACE's `IsPledgable` outside its end-of-retail ruleset).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Allegiance.cs
    if !w.era.features.swear_to_lower_level
        && w.objects.get(target).and_then(|o| o.level()).unwrap_or(0)
            < w.objects.get(this).and_then(|o| o.level()).unwrap_or(0)
    {
        send_chat(w, this, "You cannot swear to a lower level character.");
        send_error(w, this, WeenieError::AllegianceIllegalLevel);
        return false;
    }

    // DIVERGE: V435, on a world that charges for an oath, a character that cannot pay is refused.
    let cost = swear_xp_cost(w, this);
    if cost > 0
        && w.objects
            .get(this)
            .and_then(WorldObject::available_experience)
            .unwrap_or(0)
            < i64::from(cost)
    {
        send_chat(
            w,
            this,
            "You don't have enough experience available to swear Allegiance.",
        );
        send_error(w, this, WeenieError::CantSwearAllegianceInsufficientXp);
        return false;
    }

    // patron must currently be greater or equal level
    /*if (target.Level < Level)
    {
        //Console.WriteLine(Name + " tried to swear to a lower level character");
        Session.Network.EnqueueSend(new GameMessageSystemChat($"You cannot swear to a lower level character.", ChatMessageType.Broadcast));
        Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.AllegianceIllegalLevel));
        return false;
    }*/

    let self_node = player_allegiance_node(w, this)
        .and_then(|n| Some((n.allegiance, allegiance::member_node(w, n)?)));
    let target_node = player_allegiance_node(w, target)
        .and_then(|n| Some((n.allegiance, allegiance::member_node(w, n)?)));

    if let Some((target_allegiance, target_id)) = target_node {
        let target_name = name(w, target);
        let target_tree = allegiance::tree(w, target_allegiance).expect("loaded");
        let target_total_vassals = target_tree.total_vassals(target_id);
        let target_monarch_guid = target_tree
            .node(target_tree.node(target_id).monarch)
            .player_guid;

        // maximum # of direct vassals = 11
        if target_total_vassals >= 11 {
            //Console.WriteLine(target.Name + " already has the maximum # of vassals");
            send_chat(
                w,
                this,
                &format!("{target_name} already has the maximum # of vassals"),
            );
            return false;
        }

        // 2 players can't swear to each other
        // prevent any loops in the allegiance chain
        if let Some((self_allegiance, self_id)) = self_node {
            let self_tree = allegiance::tree(w, self_allegiance).expect("loaded");
            if self_tree.is_monarch(self_id)
                && self_tree.node(self_id).player_guid == target_monarch_guid
            {
                //Console.WriteLine(Name + " tried to swear to someone already in Allegiance: " + target.Name);
                send_chat(
                    w,
                    this,
                    &format!("You cannot swear allegiance to {target_name}."),
                );
                return false;
            }
        }

        let target_allegiance_obj = allegiance_obj(w, target_allegiance);
        let target_player = allegiance_node::player(w, target)
            .expect("ACE: targetNode.Player is null (NullReferenceException)");
        if target_allegiance_obj.is_locked()
            && !allegiance::has_approved_vassal(target_allegiance_obj, this.full())
            && i_player_allegiance_officer_rank(w, target_player).unwrap_or(0)
                < AllegianceOfficerLevel::Castellan.0.cast_signed()
        {
            //Console.WriteLine(Name + "tried to join locked allegiance, not in approved vassals list");
            send_chat(
                w,
                this,
                &format!("{target_name} is not accepting allegiance requests."),
            );
            return false;
        }

        if allegiance::is_banned(allegiance_obj(w, target_allegiance), this.full()) {
            //Console.WriteLine(Name + "tried to join allegiance, but was banned!");
            send_chat(
                w,
                this,
                &format!("You are banned from joining {target_name}'s allegiance."),
            );
            return false;
        }
    }

    // ensure this player doesn't own a monarch-only house
    if player_owns_monarch_only_house(w, this) {
        //Console.WriteLine(Name + "monarch tried to pledge allegiance, already owns a mansion");
        //Session.Network.EnqueueSend(new GameMessageSystemChat($"You cannot swear allegiance while owning a mansion.", ChatMessageType.Broadcast));
        send_error(
            w,
            this,
            WeenieError::CannotSwearAllegianceWhileOwningMansion,
        );
        return false;
    }

    true
}

// ACE: Player.IsBreakable
/// Returns TRUE if this player can break allegiance to the target guid
#[must_use]
pub fn is_breakable(w: &World, this: ObjectGuid, target_guid: u32) -> bool {
    // players can break from either vassals or patrons

    // ensure target player exists
    let Some(target) = player_manager::find_by_guid(w, target_guid).0 else {
        //Console.WriteLine(Name + " tried to break allegiance to an unknown player guid: " + targetGuid.Full.ToString("X8"));
        return false;
    };

    // verify patron or vassal
    let is_patron = i_player::patron_id(w, IPlayer::Online(this)) == Some(target.guid().full());
    let is_vassal = i_player::patron_id(w, target) == Some(this.full());

    if !is_patron && !is_vassal {
        //Console.WriteLine(Name + " tried to break allegiance from " + target.Name + ", but they aren't patron or vassal");
        return false;
    }
    true
}

// ACE: Player.HandleAllegianceOnLogin
/// Called when a player logs in to handle allegiance events on login
pub fn handle_allegiance_on_login(w: &mut World, this: ObjectGuid) {
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, 3.0);
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if let Some(allegiance) = player_allegiance(w, this) {
            let o = allegiance_obj(w, allegiance);
            if let Some(motd) = o.allegiance_motd() {
                let text = format!("\"{motd}\" -- {}", o.allegiance_motd_set_by().unwrap_or_default());
                send_chat(w, this, &text);
            }
        }

        let cached = w.objects.get(this).map_or(0, WorldObject::allegiance_xp_cached);
        if cached != 0 {
            let text = format!(
                "Your Vassals have produced experience points for you.\nTaking your skills as a leader into account, you gain {} xp.",
                format(cached, "N0")
            );
            send_chat(w, this, &text);
            add_allegiance_xp(w, this);
        }
    });
    action_chain.enqueue_chain(w);

    if let Some(allegiance) = player_allegiance(w, this) {
        for member in allegiance::online_players(w, allegiance) {
            if member != this
                && get_character_option(w, member, CharacterOption::ShowAllegianceLogons)
            {
                send_login_notification(w, member, this, true);
            }
        }
    }
}

fn send_login_notification(
    w: &mut World,
    member: ObjectGuid,
    this: ObjectGuid,
    is_logged_in: bool,
) {
    let session = player_manager::player_session(w, member)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = game_event_allegiance_login_notification::game_event_allegiance_login_notification(
        session_data(w, session),
        this.full(),
        is_logged_in,
    );
    game_message::enqueue_send(w, session, msg);
}

// ACE: Player.HandleAllegianceOnLogout
pub fn handle_allegiance_on_logout(w: &mut World, this: ObjectGuid) {
    if let Some(allegiance) = player_allegiance(w, this) {
        for member in allegiance::online_players(w, allegiance) {
            if member != this
                && get_character_option(w, member, CharacterOption::ShowAllegianceLogons)
            {
                send_login_notification(w, member, this, false);
            }
        }
    }
}

// ACE: Player.GetPrefix
#[must_use]
pub fn get_prefix(w: &World, this: ObjectGuid, allegiance_member: ObjectGuid) -> String {
    let node = player_allegiance_node(w, this)
        .expect("ACE: AllegianceNode is null (NullReferenceException)");
    let tree = allegiance::tree(w, node.allegiance)
        .expect("ACE: AllegianceNode is null (NullReferenceException)");
    let id = allegiance::member_node(w, node)
        .expect("ACE: AllegianceNode is null (NullReferenceException)");
    let n = tree.node(id);

    let mut prefix = "";

    if allegiance_member == tree.node(n.monarch).player_guid {
        prefix = "Your monarch ";
    } else if n
        .patron
        .is_some_and(|p| allegiance_member == tree.node(p).player_guid)
    {
        prefix = "Your patron ";
    } else if n
        .vassals
        .as_ref()
        .expect("ACE: AllegianceNode.Vassals is null (NullReferenceException)")
        .contains_key(&allegiance_member.full())
    {
        prefix = "Your vassal ";
    }

    prefix.to_owned()
}

// ACE: Player.AddAllegianceXP
/// For an online patron, adds the pending allegiance XP stored in AllegianceXPCached
/// to their total / unassigned xp
pub fn add_allegiance_xp(w: &mut World, this: ObjectGuid) {
    let cached = w
        .objects
        .get(this)
        .map_or(0, WorldObject::allegiance_xp_cached);
    if cached == 0 {
        return;
    }

    // TODO: handle ulong -> long?
    crate::world_objects::player_xp::grant_xp(
        w,
        this,
        cached.cs_cast(),
        XpType::Allegiance,
        ShareType::None,
    );

    if let Some(o) = w.objects.get_mut(this) {
        let cached = o.allegiance_xp_cached();
        let received = o.allegiance_xp_received().wrapping_add(cached);
        o.set_allegiance_xp_received(received);

        o.set_allegiance_xp_cached(0);
    }
}

/// `if (Allegiance == null) { YouAreNotInAllegiance; return }`: the allegiance, or `None` after
/// the error was sent.
fn require_allegiance(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    // check if player is in an allegiance
    let allegiance = player_allegiance(w, this);
    if allegiance.is_none() {
        send_error(w, this, WeenieError::YouAreNotInAllegiance);
    }
    allegiance
}

/// `if (AllegiancePermissionLevel < level) { YouDoNotHaveAuthorityInAllegiance; return }`.
fn require_permission(w: &mut World, this: ObjectGuid, level: AllegiancePermissionLevel) -> bool {
    if allegiance_permission_level(w, this) < level {
        send_error(w, this, WeenieError::YouDoNotHaveAuthorityInAllegiance);
        return false;
    }
    true
}

// ACE: Player.HandleActionQueryMotd
pub fn handle_action_query_motd(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionQueryMotd()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    let o = allegiance_obj(w, allegiance);
    let Some(motd) = o.allegiance_motd() else {
        send_chat(w, this, "Your allegiance has not set a message of the day.");
        return;
    };

    let msg = format!(
        "\"{motd}\" -- {}",
        o.allegiance_motd_set_by().unwrap_or_default()
    );

    send_chat(w, this, &msg);
}

/// `AllegianceTitle.GetTitle(HeritageGroup, (Gender)Gender, AllegianceNode.Rank) + " " + Name`.
fn motd_set_by(w: &World, this: ObjectGuid) -> String {
    let o = w.objects.get(this).expect("online");
    let heritage = o.heritage_group();
    let gender = Gender(
        o.gender()
            .expect("ACE: (Gender)Gender of null (InvalidOperationException)"),
    );
    let node = player_allegiance_node(w, this)
        .expect("ACE: AllegianceNode is null (NullReferenceException)");
    let rank = allegiance::tree(w, node.allegiance)
        .expect("loaded")
        .node(
            allegiance::member_node(w, node)
                .expect("ACE: AllegianceNode is null (NullReferenceException)"),
        )
        .rank;
    let rank = allegiance_rank::get_title(heritage, gender, rank);
    format!("{rank} {}", name(w, this))
}

// ACE: Player.HandleActionSetMotd
pub fn handle_action_set_motd(w: &mut World, this: ObjectGuid, motd: &str) {
    //Console.WriteLine($"{Name}.HandleActionSetMotd({motd})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Speaker) {
        return;
    }

    allegiance_mut(w, allegiance).set_allegiance_motd(Some(motd.to_owned()));

    let set_by = motd_set_by(w, this);
    allegiance_mut(w, allegiance).set_allegiance_motd_set_by(Some(set_by));

    save_allegiance(w, allegiance);

    send_chat(w, this, "Your message of the day has been set.");
}

fn allegiance_mut(w: &mut World, allegiance: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(allegiance)
        .expect("ACE: Allegiance is null (NullReferenceException)")
}

fn save_allegiance(w: &mut World, allegiance: ObjectGuid) {
    crate::dispatch::save_biota_to_database::save_biota_to_database(w, allegiance, true);
}

// ACE: Player.HandleActionClearMotd
pub fn handle_action_clear_motd(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionClearMotd()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Speaker) {
        return;
    }

    allegiance_mut(w, allegiance).set_allegiance_motd(None);

    let set_by = motd_set_by(w, this);
    allegiance_mut(w, allegiance).set_allegiance_motd_set_by(Some(set_by));

    save_allegiance(w, allegiance);

    send_chat(w, this, "Your message of the day has been cleared.");
}

// ACE: Player.HandleActionQueryAllegianceName
pub fn handle_action_query_allegiance_name(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionQueryAllegianceName()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    let Some(allegiance_name) = allegiance_obj(w, allegiance).allegiance_name() else {
        send_chat(w, this, "Your allegiance has not set a name.");
        return;
    };

    send_chat(w, this, &allegiance_name);
}

// ACE: Player.HandleActionSetAllegianceName
pub fn handle_action_set_allegiance_name(w: &mut World, this: ObjectGuid, allegiance_name: &str) {
    //Console.WriteLine($"{Name}.HandleActionSetAllegianceName({allegianceName}");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Castellan) {
        return;
    }

    // TODO: name verifications
    // - same name as current allegiance name
    // - no empty names
    // - allegiance name too long (40 chars max.)
    // - allegiance name already in use
    // - bad chars (space, single quote, hyphen, A-Z, a-z)
    // - banned words from portal.dat
    // - name change timer (1 day?)

    allegiance_mut(w, allegiance).set_allegiance_name(Some(allegiance_name.to_owned()));
    save_allegiance(w, allegiance);

    send_chat(w, this, "Your allegiance name has been set.");
}

// ACE: Player.HandleActionClearAllegianceName
pub fn handle_action_clear_allegiance_name(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionClearAllegianceName()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Castellan) {
        return;
    }

    allegiance_mut(w, allegiance).set_allegiance_name(None);
    save_allegiance(w, allegiance);

    send_chat(w, this, "Your allegiance name has been cleared.");
}

// ACE: Player.HandleActionListAllegianceOfficers
pub fn handle_action_list_allegiance_officers(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionListAllegianceOfficers()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    let monarch = allegiance_node::player(w, allegiance::monarch_player_guid(w, allegiance))
        .expect("ACE: Monarch.Player is null (NullReferenceException)");
    let mut officer_list = "Allegiance Officers:\n".to_owned();
    officer_list += &format!("{} (Monarch)\n", i_name(w, monarch));

    let f = allegiance::fields_of(w, allegiance).expect("loaded");
    let mut officers: Vec<IPlayer> = f
        .officers()
        .values()
        .map(|&n| {
            allegiance_node::player(w, f.tree.node(n).player_guid)
                .expect("ACE: node.Player is null (NullReferenceException)")
        })
        .collect();
    // `OrderBy(i => i.Name)`: a stable sort with the culture's comparer
    officers.sort_by(|a, b| compare_culture(&i_name(w, *a), &i_name(w, *b)));

    for officer in officers {
        let rank = i_player_allegiance_officer_rank(w, officer)
            .expect("ACE: (AllegianceOfficerLevel)null (InvalidOperationException)");
        let title = allegiance::get_officer_title(
            allegiance_obj(w, allegiance),
            AllegianceOfficerLevel(rank.cast_unsigned()),
        );
        officer_list += &format!("{} ({title})\n", i_name(w, officer));
    }

    send_chat(w, this, &officer_list);
}

// ACE: Player.HandleActionListAllegianceOfficerTitles
pub fn handle_action_list_allegiance_officer_titles(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionListAllegianceOfficerTitles()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    let o = allegiance_obj(w, allegiance);
    let speaker_title = o
        .allegiance_speaker_title()
        .unwrap_or_else(|| "Speaker".to_owned());
    let seneschal_title = o
        .allegiance_seneschal_title()
        .unwrap_or_else(|| "Seneschal".to_owned());
    let castellan_title = o
        .allegiance_castellan_title()
        .unwrap_or_else(|| "Castellan".to_owned());

    let mut officer_titles = "Allegiance Officer Titles:\n".to_owned();
    officer_titles += &format!("1. {speaker_title}\n");
    officer_titles += &format!("2. {seneschal_title}\n");
    officer_titles += &format!("3. {castellan_title}\n");

    send_chat(w, this, &officer_titles);
}

// ACE: Player.HandleActionSetAllegianceOfficerTitle
pub fn handle_action_set_allegiance_officer_title(
    w: &mut World,
    this: ObjectGuid,
    rank: u32,
    title: &str,
) {
    //Console.WriteLine($"{Name}.HandleActionSetAllegianceOfficerTitle({rank}, {title})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Castellan) {
        return;
    }

    if !(1..=3).contains(&rank) {
        send_chat(
            w,
            this,
            "Please specify a valid officer level as a number between 1 and 3.",
        );
        return;
    }

    let o = allegiance_mut(w, allegiance);
    match rank {
        1 => o.set_allegiance_speaker_title(Some(title.to_owned())),
        2 => o.set_allegiance_seneschal_title(Some(title.to_owned())),
        3 => o.set_allegiance_castellan_title(Some(title.to_owned())),
        _ => {}
    }

    send_chat(
        w,
        this,
        &format!(
            "Your allegiance {} title has been set.",
            AllegianceOfficerLevel(rank)
        ),
    );

    save_allegiance(w, allegiance);
}

// ACE: Player.HandleActionClearAllegianceOfficerTitles
pub fn handle_action_clear_allegiance_officer_titles(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionClearAllegianceOfficerTitles()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    // castellans can clear all titles?
    if !require_permission(w, this, AllegiancePermissionLevel::Castellan) {
        return;
    }

    let o = allegiance_mut(w, allegiance);
    o.set_allegiance_speaker_title(None);
    o.set_allegiance_seneschal_title(None);
    o.set_allegiance_castellan_title(None);

    save_allegiance(w, allegiance);

    send_chat(w, this, "Your allegiance officer titles have been cleared.");
}

// ACE: Player.HandleActionSetAllegianceOfficer
pub fn handle_action_set_allegiance_officer(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
    officer_level: u32,
) {
    //Console.WriteLine($"{Name}.HandleActionSetAllegianceOfficer({playerName}, {officerLevel})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    // TODO: also check officer permissions
    if !require_permission(w, this, AllegiancePermissionLevel::Seneschal) {
        return;
    }

    let Some(player) = player_manager::find_by_name(w, player_name).0 else {
        send_chat(w, this, &format!("{player_name} not found"));
        return;
    };

    if !allegiance::is_member(w, allegiance, player.guid()) {
        send_chat(
            w,
            this,
            &format!("{player_name} not found in this allegiance"),
        );
        return;
    }

    if allegiance_obj(w, allegiance).monarch_id() == Some(player.guid().full()) {
        send_error(w, this, WeenieError::YouDoNotHaveAuthorityInAllegiance);
        return;
    }

    // seneschals can only promote/demote speakers
    if allegiance_permission_level(w, this) == AllegiancePermissionLevel::Seneschal {
        let current: u32 = i_player_allegiance_officer_rank(w, player)
            .unwrap_or(0)
            .cs_cast();
        if officer_level > 1 || AllegianceOfficerLevel(current) > AllegianceOfficerLevel::Speaker {
            send_error(w, this, WeenieError::YouDoNotHaveAuthorityInAllegiance);
            return;
        }
    }

    if !(1..=3).contains(&officer_level) {
        send_chat(
            w,
            this,
            "Please specify a valid officer level as a number between 1 and 3.",
        );
        return;
    }

    i_player_set_allegiance_officer_rank(w, player, Some(officer_level.cast_signed()));
    let title = allegiance::get_officer_title(
        allegiance_obj(w, allegiance),
        AllegianceOfficerLevel(officer_level),
    );

    allegiance::build_officers_of(w, allegiance);

    send_chat(w, this, &format!("{} is now {title}.", i_name(w, player)));

    // send message to online target player?
}

// ACE: Player.HandleActionRemoveAllegianceOfficer
pub fn handle_action_remove_allegiance_officer(
    w: &mut World,
    this: ObjectGuid,
    officer_name: &str,
) {
    //Console.WriteLine($"{Name}.HandleActionRemoveAllegianceOfficer({officerName})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Seneschal) {
        return;
    }

    let Some(officer) = player_manager::find_by_name(w, officer_name).0 else {
        send_chat(w, this, &format!("{officer_name} not found"));
        return;
    };

    if !allegiance::is_member(w, allegiance, officer.guid()) {
        send_chat(
            w,
            this,
            &format!("{officer_name} not found in this allegiance"),
        );
        return;
    }

    if !allegiance::is_officer(w, allegiance, officer.guid()) {
        send_chat(
            w,
            this,
            &format!("{} not found in allegiance officers", i_name(w, officer)),
        );
        return;
    }

    // seneschals can only promote/demote speakers and non-officers
    if allegiance_permission_level(w, this) == AllegiancePermissionLevel::Seneschal {
        let current: u32 = i_player_allegiance_officer_rank(w, officer)
            .unwrap_or(0)
            .cs_cast();
        if AllegianceOfficerLevel(current) > AllegianceOfficerLevel::Speaker {
            send_error(w, this, WeenieError::YouDoNotHaveAuthorityInAllegiance);
            return;
        }
    }

    i_player_set_allegiance_officer_rank(w, officer, None);
    allegiance::build_officers_of(w, allegiance);

    send_chat(
        w,
        this,
        &format!(
            "{} has been removed from allegiance officers.",
            i_name(w, officer)
        ),
    );

    // send message to online target player?
}

// ACE: Player.HandleActionClearAllegianceOfficers
pub fn handle_action_clear_allegiance_officers(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionClearAllegianceOfficers()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    // could castellans perform this action?
    if !require_permission(w, this, AllegiancePermissionLevel::Monarch) {
        return;
    }

    let officers: Vec<ObjectGuid> = allegiance::fields_of(w, allegiance)
        .expect("loaded")
        .officers()
        .keys()
        .copied()
        .collect();
    for key in officers {
        if let Some(officer) = player_manager::find_by_guid(w, key.full()).0 {
            i_player_set_allegiance_officer_rank(w, officer, None);
        }
    }

    allegiance::build_officers_of(w, allegiance);

    send_chat(w, this, "The list of officers has been cleared.");
}

// ACE: Player.HandleActionAllegianceInfoRequest
pub fn handle_action_allegiance_info_request(w: &mut World, this: ObjectGuid, player_name: &str) {
    //Console.WriteLine($"{Name}.HandleActionRemoveAllegianceOfficer({playerName})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Seneschal) {
        return;
    }

    let Some(player) = player_manager::find_by_name(w, player_name).0 else {
        send_chat(w, this, &format!("{player_name} not found"));
        return;
    };

    let allegiance_node = NodeRef {
        allegiance,
        player: player.guid(),
    };
    if allegiance::member_node(w, allegiance_node).is_none() {
        send_chat(
            w,
            this,
            &format!("{player_name} not found in this allegiance"),
        );
        return;
    }
    let profile =
        allegiance_profile::allegiance_profile_new(w, Some(allegiance), Some(allegiance_node));

    let session = player_manager::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = game_event_allegiance_info_response::game_event_allegiance_info_response(
        session_data(w, session),
        player.guid().full(),
        &profile,
    );
    game_message::enqueue_send(w, session, msg);
}

// ACE: Player.HandleActionDoAllegianceLockAction
pub fn handle_action_do_allegiance_lock_action(
    w: &mut World,
    this: ObjectGuid,
    action: AllegianceLockAction,
) {
    //Console.WriteLine($"{Name}.HandleActionDoAllegianceLockAction({action})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    let lock_status = |w: &World| {
        if allegiance_obj(w, allegiance).is_locked() {
            "locked"
        } else {
            "unlocked"
        }
    };

    // no permissions for checks?
    if action == AllegianceLockAction::Check {
        let text = format!("The allegiance is currently {}.", lock_status(w));
        send_chat(w, this, &text);
        return;
    }

    if action == AllegianceLockAction::CheckApproved {
        let approved_vassals = allegiance::approved_vassals(allegiance_obj(w, allegiance));
        if approved_vassals.is_empty() {
            send_chat(w, this, "The approved vassals list is currently empty.");
            return;
        }

        let mut list = "Approved vassals:".to_owned();
        for key in approved_vassals.keys() {
            let Some(approved_vassal) = player_manager::find_by_guid(w, *key).0 else {
                // automatically remove?
                log::warn!("{}.HandleActionDoAllegianceLockAction({action}): couldn't find approved vassal {key:08X}", name(w, this));
                continue;
            };

            list += &format!("\n{}", i_name(w, approved_vassal));
        }

        send_chat(w, this, &list);
        return;
    }

    if !require_permission(w, this, AllegiancePermissionLevel::Seneschal) {
        return;
    }

    if action == AllegianceLockAction::ClearApproved {
        let approved_vassals: Vec<u32> =
            allegiance::approved_vassals(allegiance_obj(w, allegiance))
                .keys()
                .copied()
                .collect();
        for key in approved_vassals {
            allegiance::remove_approved_vassal(w, allegiance, key);
        }

        send_chat(w, this, "The approved vassals list has been cleared.");
        return;
    }

    let is_locked = allegiance_obj(w, allegiance).is_locked();
    if action == AllegianceLockAction::On && is_locked
        || action == AllegianceLockAction::Off && !is_locked
    {
        let text = format!("The allegiance is already {}.", lock_status(w));
        send_chat(w, this, &text);
        return;
    }

    let o = allegiance_mut(w, allegiance);
    if action == AllegianceLockAction::On {
        o.set_is_locked(true);
    } else if action == AllegianceLockAction::Off {
        o.set_is_locked(false);
    } else if action == AllegianceLockAction::Toggle {
        o.set_is_locked(!is_locked);
    }

    save_allegiance(w, allegiance);

    let text = format!("The allegiance is now {}.", lock_status(w));
    send_chat(w, this, &text);
}

// ACE: Player.HandleActionSetAllegianceApprovedVassal
pub fn handle_action_set_allegiance_approved_vassal(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
) {
    //Console.WriteLine($"{Name}.HandleActionSetAllegianceApprovedVassal({playerName})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Castellan) {
        return;
    }

    let Some(player) = player_manager::find_by_name(w, player_name).0 else {
        send_chat(w, this, &format!("{player_name} not found"));
        return;
    };

    if i_player_allegiance(w, player).is_some() {
        send_chat(
            w,
            this,
            &format!("{} is already in an allegiance.", i_name(w, player)),
        );
        return;
    }

    if allegiance::is_member(w, allegiance, player.guid()) {
        send_chat(
            w,
            this,
            &format!("{} is already in the allegiance.", i_name(w, player)),
        );
        return;
    }

    if allegiance::has_approved_vassal(allegiance_obj(w, allegiance), player.guid().full()) {
        send_chat(
            w,
            this,
            &format!("{} is already an approved vassal.", i_name(w, player)),
        );
        return;
    }

    allegiance::add_approved_vassal(w, allegiance, player.guid().full());

    send_chat(
        w,
        this,
        &format!("{} is now an approved vassal.", i_name(w, player)),
    );
}

/// The checks the chat boot and gag handlers share before the filter: the player, or `None`
/// once the refusal was sent. `self_text` and `monarch_text` are the two refusals that differ.
fn chat_filter_target(
    w: &mut World,
    this: ObjectGuid,
    allegiance: ObjectGuid,
    player_name: &str,
    self_text: &str,
    monarch_text: Option<&str>,
) -> Option<IPlayer> {
    let Some(player) = player_manager::find_by_name(w, player_name).0 else {
        send_chat(w, this, &format!("{player_name} not found"));
        return None;
    };

    if !allegiance::is_member(w, allegiance, player.guid()) {
        send_chat(
            w,
            this,
            &format!("{} not found in allegiance", i_name(w, player)),
        );
        return None;
    }

    if player.guid() == this {
        send_chat(w, this, self_text);
        return None;
    }

    if let Some(monarch_text) = monarch_text {
        if allegiance_obj(w, allegiance).monarch_id() == Some(player.guid().full()) {
            send_chat(w, this, monarch_text);
            return None;
        }
    }

    Some(player)
}

fn chat_filters(
    w: &mut World,
    allegiance: ObjectGuid,
) -> &mut DotNetDict<ObjectGuid, DotNetDateTime> {
    allegiance::fields_of_mut(w, allegiance)
        .expect("loaded")
        .chat_filters
        .as_mut()
        .expect("ACE: Allegiance.ChatFilters is null (NullReferenceException)")
}

// ACE: Player.HandleActionAllegianceChatBoot
pub fn handle_action_allegiance_chat_boot(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
    reason: &str,
) {
    //Console.WriteLine($"{Name}.HandleActionAllegianceChatBoot({playerName}, {reason})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Speaker) {
        return;
    }

    let Some(player) = chat_filter_target(
        w,
        this,
        allegiance,
        player_name,
        "You cannot boot yourself from allegiance chat.",
        Some("You cannot boot the monarch from allegiance chat."),
    ) else {
        return;
    };

    let player_name = i_name(w, player);
    let filters = chat_filters(w, allegiance);
    if let Some(&existing) = filters.get(&player.guid()) {
        if existing == DotNetDateTime::MAX_VALUE {
            send_chat(
                w,
                this,
                &format!("{player_name} has already been booted from allegiance chat."),
            );
            return;
        }

        filters.insert(player.guid(), DotNetDateTime::MAX_VALUE);
    } else {
        filters.add(player.guid(), DotNetDateTime::MAX_VALUE);
    }

    send_chat(
        w,
        this,
        &format!("{player_name} has been booted from allegiance chat."),
    );

    if let Some(online_player) = player_manager::get_online_player(w, player.guid().full()) {
        send_chat(
            w,
            online_player,
            &format!("You have been booted from Allegiance chat ({reason})"),
        );
    }
}

// ACE: Player.AllegianceChat_GagTime
/// `TimeSpan.FromMinutes(5)`.
#[must_use]
pub fn allegiance_chat_gag_time() -> TimeSpan {
    TimeSpan::from_minutes(5.0)
}

// ACE: Player.HandleActionAllegianceChatGag
pub fn handle_action_allegiance_chat_gag(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
    enabled: bool,
) {
    if enabled {
        handle_action_allegiance_chat_gag_enabled(w, this, player_name);
    } else {
        handle_action_allegiance_chat_gag_disabled(w, this, player_name);
    }
}

// ACE: Player.HandleActionAllegianceChatGag_Enabled
pub fn handle_action_allegiance_chat_gag_enabled(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
) {
    //Console.WriteLine($"{Name}.HandleActionAllegianceChatGag_Enabled({playerName})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Speaker) {
        return;
    }

    let Some(player) = chat_filter_target(
        w,
        this,
        allegiance,
        player_name,
        "You cannot gag yourself.",
        Some("You cannot gag the monarch."),
    ) else {
        return;
    };

    let player_name = i_name(w, player);
    let until = w.now.utc + allegiance_chat_gag_time();
    let filters = chat_filters(w, allegiance);
    if let Some(&existing) = filters.get(&player.guid()) {
        if existing == DotNetDateTime::MAX_VALUE {
            send_chat(
                w,
                this,
                &format!("{player_name} has already been booted from allegiance chat."),
            );
            return;
        }

        filters.insert(player.guid(), until);
    } else {
        filters.add(player.guid(), until);
    }

    send_chat(w, this, &format!("{player_name} has been gagged."));

    if let Some(online_player) = player_manager::get_online_player(w, player.guid().full()) {
        send_chat(w, online_player, "You have been gagged in allegiance chat.");
    }
}

// ACE: Player.HandleActionAllegianceChatGag_Disabled
pub fn handle_action_allegiance_chat_gag_disabled(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
) {
    //Console.WriteLine($"{Name}.HandleActionAllegianceChatGag_Disabled({playerName})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Speaker) {
        return;
    }

    let Some(player) = chat_filter_target(
        w,
        this,
        allegiance,
        player_name,
        "You cannot ungag yourself.",
        None,
    ) else {
        return;
    };

    let player_name = i_name(w, player);
    let Some(existing) = chat_filters(w, allegiance).get(&player.guid()).copied() else {
        send_chat(w, this, &format!("{player_name} has not been gagged."));
        return;
    };

    if existing == DotNetDateTime::MAX_VALUE {
        send_chat(
            w,
            this,
            &format!("{player_name} has been booted from allegiance chat."),
        );
        return;
    }

    chat_filters(w, allegiance).remove(&player.guid());

    send_chat(w, this, &format!("{player_name} has been ungagged."));

    if let Some(online_player) = player_manager::get_online_player(w, player.guid().full()) {
        send_chat(
            w,
            online_player,
            "You have been ungagged in allegiance chat.",
        );
    }
}

// ACE: Player.HandleActionListAllegianceBans
pub fn handle_action_list_allegiance_bans(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionListAllegianceBans()");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    let ban_list = allegiance::ban_list(allegiance_obj(w, allegiance));

    if ban_list.is_empty() {
        send_chat(w, this, "The ban list is currently empty.");
        return;
    }

    let mut list = "Allegiance ban list:".to_owned();
    for key in ban_list.keys() {
        let Some(player) = player_manager::find_by_guid(w, *key).0 else {
            // automatically remove?
            log::warn!(
                "{}.HandleActionListAllegianceBans(): couldn't find banned player {key:08X}",
                name(w, this)
            );
            continue;
        };

        list += &format!("\n{}", i_name(w, player));
    }

    send_chat(w, this, &list);
}

// ACE: Player.HandleActionAddAllegianceBan
pub fn handle_action_add_allegiance_ban(w: &mut World, this: ObjectGuid, player_name: &str) {
    //Console.WriteLine($"{Name}.HandleActionAddAllegianceBan({playerName})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Seneschal) {
        return;
    }

    let Some(player) = player_manager::find_by_name(w, player_name).0 else {
        send_chat(w, this, &format!("{player_name} not found"));
        return;
    };

    // any other restrictions?
    if allegiance_obj(w, allegiance).monarch_id() == Some(player.guid().full()) {
        send_chat(
            w,
            this,
            &format!(
                "{} cannot be banned from the allegiance!",
                i_name(w, player)
            ),
        );
        return;
    }

    if allegiance::is_banned(allegiance_obj(w, allegiance), player.guid().full()) {
        send_chat(
            w,
            this,
            &format!(
                "{} is already banned from the allegiance.",
                i_name(w, player)
            ),
        );
        return;
    }

    allegiance::add_ban(w, allegiance, player.guid().full());

    send_chat(
        w,
        this,
        &format!("{} has been banned from the allegiance.", i_name(w, player)),
    );

    // were they already a member? if so, boot them...
    if allegiance::is_member(w, allegiance, player.guid()) {
        let name = i_name(w, player);
        handle_action_break_allegiance_boot(w, this, &name, false);
    }
}

// ACE: Player.HandleActionRemoveAllegianceBan
pub fn handle_action_remove_allegiance_ban(w: &mut World, this: ObjectGuid, player_name: &str) {
    //Console.WriteLine($"{Name}.HandleActionRemoveAllegianceBan({playerName})");

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Seneschal) {
        return;
    }

    let Some(player) = player_manager::find_by_name(w, player_name).0 else {
        send_chat(w, this, &format!("{player_name} not found"));
        return;
    };

    if !allegiance::is_banned(allegiance_obj(w, allegiance), player.guid().full()) {
        send_chat(
            w,
            this,
            &format!("{} is not banned from the allegiance.", i_name(w, player)),
        );
        return;
    }

    allegiance::remove_ban(w, allegiance, player.guid().full());

    send_chat(
        w,
        this,
        &format!(
            "{} is no longer banned from the allegiance.",
            i_name(w, player)
        ),
    );
}

// ACE: Player.HandleActionBreakAllegianceBoot
pub fn handle_action_break_allegiance_boot(
    w: &mut World,
    this: ObjectGuid,
    player_name: &str,
    account_boot: bool,
) {
    log::info!(
        "[ALLEGIANCE] {}.HandleActionBreakAllegianceBoot({player_name}, {})",
        name(w, this),
        if account_boot { "True" } else { "False" }
    );

    // TODO: handle account boot

    let Some(allegiance) = require_allegiance(w, this) else {
        return;
    };

    if !require_permission(w, this, AllegiancePermissionLevel::Seneschal) {
        return;
    }

    let Some(player) = player_manager::find_by_name(w, player_name).0 else {
        send_chat(w, this, &format!("{player_name} not found"));
        return;
    };

    if !allegiance::is_member(w, allegiance, player.guid()) {
        send_chat(w, this, &format!("{player_name} not found in allegiance"));
        return;
    }

    if player.guid() == this {
        send_chat(w, this, "You cannot boot yourself from the allegiance.");
        return;
    }

    if allegiance_obj(w, allegiance).monarch_id() == Some(player.guid().full()) {
        send_chat(w, this, "You cannot boot the monarch from the allegiance!");
        return;
    }

    let patron_id = i_player::patron_id(w, player);
    let Some(patron) = player_manager::find_by_guid(w, patron_id.unwrap_or(0)).0 else {
        // DIVERGE: ACE writes this line to the console.
        log::warn!(
            "{}.HandleActionBreakAllegianceBoot({}, {}): couldn't find patron id {}",
            name(w, this),
            i_name(w, player),
            if account_boot { "True" } else { "False" },
            patron_id.map_or_else(String::new, |p| p.to_string())
        );
        return;
    };

    i_player::set_patron_id(w, player, None);
    i_player_update_property_iid(w, player, PropertyInstanceId::Monarch, None, true);

    // walk the allegiance tree from this node, update monarch ids
    let tree = allegiance::tree(w, allegiance).expect("loaded").clone();
    let target_node = allegiance::member_node(
        w,
        NodeRef {
            allegiance,
            player: player.guid(),
        },
    )
    .expect("ACE: targetNode is null (NullReferenceException)");

    for node in tree.walk(target_node, true) {
        let node_player = allegiance_node::player(w, tree.node(node).player_guid)
            .expect("ACE: node.Player is null (NullReferenceException)");
        i_player_update_property_iid(
            w,
            node_player,
            PropertyInstanceId::Monarch,
            Some(player.guid().full()),
            true,
        );

        i_player::save_biota_to_database(w, node_player, true);
    }

    // rebuild allegiance tree structures
    allegiance_manager::on_break_allegiance(w, Some(player), Some(patron));

    check_allegiance_house(w, player.guid());

    let new_allegiance = allegiance_manager::get_allegiance(w, Some(player));
    if let Some(new_allegiance) = new_allegiance {
        allegiance_manager::walk_check_allegiance_house(w, new_allegiance);
    }

    // update allegiance ui panels?

    send_chat(
        w,
        this,
        &format!(
            "{} has been removed from the allegiance.",
            i_name(w, player)
        ),
    );

    if let Some(online_player) = player_manager::get_online_player(w, player.guid().full()) {
        send_chat(
            w,
            online_player,
            "You have been booted from the allegiance!",
        );
    }
}

// ACE: Player.AllegiancePermissionLevel
/// <https://asheron.fandom.com/wiki/Allegiance_Officers>
///
/// Level 1: Speaker
/// - Allegiance chat kick.
/// - Allegiance chat gag.
/// - Allegiance broadcast.
/// - Set/clear the MOTD.
///
/// Level 2: Seneschal
/// - Promote/demote members under own rank (i.e. can promote/demote speakers)
/// - Allegiance boot.
/// - Allegiance ban.
/// - Access allegiance info.
/// - Lock/unlock the allegiance.
///
/// Level 3: Castellan
/// - Promote/demote members to any rank, including other Castellans.
/// - Change officer titles.
/// - Set/clear the allegiance name.
/// - Set allegiance bindstone.
/// - Change mansion allegiance access/storage permissions.
/// - Bypass allegiance lock with own vassals.
/// - Bypass allegiance lock by approving particular vassals.
#[must_use]
pub fn allegiance_permission_level(w: &World, this: ObjectGuid) -> AllegiancePermissionLevel {
    let Some(allegiance) = player_allegiance(w, this) else {
        return AllegiancePermissionLevel::None;
    };

    if allegiance_obj(w, allegiance).monarch_id() == Some(this.full()) {
        return AllegiancePermissionLevel::Monarch;
    }

    AllegiancePermissionLevel(
        w.objects
            .get(this)
            .and_then(WorldObject::allegiance_officer_rank)
            .unwrap_or(0),
    )
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members ported in other files, named after them.
// ---------------------------------------------------------------------------------------------

/// `player.CheckHouse()` (`Player_House.cs`, houses): answers no boot.
fn player_check_house(w: &mut World, player: ObjectGuid) -> bool {
    crate::world_objects::player_house::check_house(w, player)
}

/// `House != null && House.SlumLord.HouseRequiresMonarch && House.HouseOwner == Guid.Full`
/// (`Player_House.cs`, houses): answers no house.
fn player_owns_monarch_only_house(w: &World, this: ObjectGuid) -> bool {
    let Some(house) =
        crate::world_objects::player_house::house(w, this).filter(|&h| w.objects.contains(h))
    else {
        return false;
    };
    let slum_lord = crate::world_objects::house::slum_lord(w, house)
        .expect("System.NullReferenceException: House.SlumLord");
    w.objects
        .get(slum_lord)
        .is_some_and(WorldObject::house_requires_monarch)
        && w.objects.get(house).and_then(WorldObject::house_owner) == Some(this.full())
}
