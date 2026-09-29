// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Allegiance.cs
//! Port of `Source/ACE.Server/WorldObjects/Allegiance.cs`.
//!
//! # Allegiance instances (DIVERGE arch)
//!
//! An `Allegiance` is a world object that no landblock holds: `AllegianceManager.Allegiances` and
//! the players do. Here the allegiances in `AllegianceManager.Allegiances` live in `w.objects`
//! under their guid, and every reference to an allegiance (a player's `Allegiance`, a node's
//! `Allegiance`) is that guid. ACE rebuilds an allegiance by constructing a new object with the
//! same guid from the shard (`AllegianceManager.GetAllegiance`) and replacing the dictionary
//! entry; here the new object replaces the old one in `w.objects`, so a reference that ACE would
//! leave on the superseded instance reads the current one. The one place ACE observably acts on a
//! superseded instance (`OnSwearAllegiance`'s approved-vassal removal) acts on the current one
//! here (a fix, V262; see `allegiance_manager.rs`).
//!
//! The tree (`Monarch`, `Members`, `Officers`) is an arena of nodes in [`AllegianceFields`]
//! (`entity/allegiance_node.rs`).

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{AllegianceOfficerLevel, PropertyInt};
use empyrean_entity::models::properties_allegiance_extensions as pae;
use empyrean_entity::models::PropertiesAllegiance;
use empyrean_entity::{LandblockId, ObjectGuid};

use crate::entity::allegiance_node::{self, AllegianceTree, NodeId, NodeRef};
use crate::entity::i_player::{self, IPlayer};
use crate::managers::{allegiance_manager, landblock_manager, player_manager};
use crate::network::game_event::events::game_event_allegiance_update;
use crate::network::game_messages::game_message;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::structure::allegiance_data::{AllegianceData, AllegianceDataNode};
use crate::network::structure::allegiance_profile::{
    AllegianceNodeView, AllegiancePatronView, AllegianceView,
};
use crate::world_objects::kinds::KindData;
use crate::world_objects::player_allegiance as pa;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Allegiance.cs`.
#[derive(Debug, Default)]
pub struct AllegianceFields {
    /// The top of the AllegianceNode tree
    // ACE: Allegiance.Monarch
    pub monarch: Option<NodeId>,
    /// The nodes `Monarch` links (not ACE: the arena that stands for ACE's node objects).
    pub tree: AllegianceTree,
    /// A lookup table of Players => AllegianceNodes
    // ACE: Allegiance.Members
    pub members: Option<DotNetDict<ObjectGuid, NodeId>>,
    // ACE: Allegiance.Officers
    pub officers: Option<DotNetDict<ObjectGuid, NodeId>>,
    /// Handles booting players from allegiance chat
    // ACE: Allegiance.ChatFilters
    pub chat_filters: Option<DotNetDict<ObjectGuid, DotNetDateTime>>,
}

impl AllegianceFields {
    /// `Monarch` (`NullReferenceException` when null).
    #[must_use]
    pub fn monarch_node(&self) -> NodeId {
        self.monarch
            .expect("ACE: Allegiance.Monarch is null (NullReferenceException)")
    }

    /// `Members` (`NullReferenceException` when null).
    #[must_use]
    pub fn members(&self) -> &DotNetDict<ObjectGuid, NodeId> {
        self.members
            .as_ref()
            .expect("ACE: Allegiance.Members is null (NullReferenceException)")
    }

    /// `Officers` (`NullReferenceException` when null).
    #[must_use]
    pub fn officers(&self) -> &DotNetDict<ObjectGuid, NodeId> {
        self.officers
            .as_ref()
            .expect("ACE: Allegiance.Officers is null (NullReferenceException)")
    }

    // ACE: Allegiance.TotalMembers
    /// The total # of players in the Allegiance
    #[must_use]
    pub fn total_members(&self) -> i32 {
        i32::try_from(self.members().len()).unwrap_or(i32::MAX)
    }
}

/// The allegiance fields of an `Allegiance` object.
#[must_use]
pub fn fields(o: &WorldObject) -> Option<&AllegianceFields> {
    match &o.kind {
        KindData::Allegiance(d) => Some(&d.allegiance),
        _ => None,
    }
}

/// The allegiance fields of an `Allegiance` object, mutably.
pub fn fields_mut(o: &mut WorldObject) -> Option<&mut AllegianceFields> {
    match &mut o.kind {
        KindData::Allegiance(d) => Some(&mut d.allegiance),
        _ => None,
    }
}

/// The fields of the allegiance `allegiance` (an object in `w.objects`).
#[must_use]
pub fn fields_of(w: &World, allegiance: ObjectGuid) -> Option<&AllegianceFields> {
    w.objects.get(allegiance).and_then(fields)
}

/// The fields of the allegiance `allegiance`, mutably.
pub fn fields_of_mut(w: &mut World, allegiance: ObjectGuid) -> Option<&mut AllegianceFields> {
    w.objects.get_mut(allegiance).and_then(fields_mut)
}

/// The node tree of the allegiance `allegiance`.
#[must_use]
pub fn tree(w: &World, allegiance: ObjectGuid) -> Option<&AllegianceTree> {
    fields_of(w, allegiance).map(|f| &f.tree)
}

/// Resolves a node reference: the node of `node.player` in `node.allegiance`'s `Members`.
#[must_use]
pub fn member_node(w: &World, node: NodeRef) -> Option<NodeId> {
    fields_of(w, node.allegiance)?
        .members
        .as_ref()?
        .get(&node.player)
        .copied()
}

/// `allegiance.Monarch.PlayerGuid`.
///
/// # Panics
/// When the allegiance is not loaded or has no tree (ACE's `NullReferenceException`).
#[must_use]
pub fn monarch_player_guid(w: &World, allegiance: ObjectGuid) -> ObjectGuid {
    let f = fields_of(w, allegiance).expect("ACE: Allegiance is null (NullReferenceException)");
    f.tree.node(f.monarch_node()).player_guid
}

/// `allegiance.Members`, as `(player, node)` pairs in dictionary order.
#[must_use]
pub fn members(w: &World, allegiance: ObjectGuid) -> Vec<(ObjectGuid, NodeId)> {
    fields_of(w, allegiance)
        .expect("ACE: Allegiance is null (NullReferenceException)")
        .members()
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect()
}

/// `allegiance.TotalMembers`.
#[must_use]
pub fn total_members(w: &World, allegiance: ObjectGuid) -> i32 {
    fields_of(w, allegiance)
        .expect("ACE: Allegiance is null (NullReferenceException)")
        .total_members()
}

// ---- constructors and SetEphemeralValues ----

/// `new Allegiance(weenie, guid)` / `new Allegiance(biota)`. The biota constructor stops early
/// when the record has no monarch; otherwise it initializes the allegiance tree (`Init`).
// ACE: Allegiance.Allegiance
pub fn allegiance_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    let from_biota = src.is_biota();
    crate::world_objects::world_object::world_object_ctor(o, env, src);

    if !from_biota {
        //Console.WriteLine($"Allegiance({weenie.ClassId}, {guid}): weenie constructor");

        allegiance_initialize_property_dictionaries(o);
        return;
    }

    //Console.WriteLine($"Allegiance({biota.Id:X8}): biota constructor");

    let Some(monarch_id) = o.monarch_id() else {
        // DIVERGE: ACE writes this line to the console.
        log::info!(
            "Allegiance({:08X}): constructor called with no monarch",
            o.biota.id
        );
        return;
    };

    allegiance_initialize_property_dictionaries(o);
    init(env.w, o, ObjectGuid::new(monarch_id));
}

/// `new Allegiance(ObjectGuid monarch)`: the parameterless `WorldObject()` base (no biota, no
/// ephemeral values), then `Init(monarch)`. Its guid is 0: it is only ever counted.
// ACE: Allegiance.Allegiance
#[must_use]
pub fn allegiance_from_monarch(
    w: &World,
    monarch: ObjectGuid,
) -> crate::world_objects::world_object::WorldObject {
    let mut o = crate::world_objects::world_object::WorldObject::allocate(
        crate::dispatch::Class::Allegiance,
    );
    //Console.WriteLine($"Allegiance({monarch}): monarch constructor");
    init(w, &mut o, monarch);
    o
}

// ACE: Allegiance.InitializePropertyDictionaries
fn allegiance_initialize_property_dictionaries(
    o: &mut crate::world_objects::world_object::WorldObject,
) {
    if o.biota.properties_allegiance.is_none() {
        o.biota.properties_allegiance = Some(empyrean_common::dotnet::DotNetDict::new());
    }
}

// ACE: Allegiance.Init
// Not ACE's (a fix, V261): `ChatFilters` starts empty here, but a rebuild of the same allegiance carries the previous instance's boots and gags over (`AllegianceManager.GetAllegiance`'s store), so a swear or break elsewhere in the allegiance no longer lifts them.
/// Constructs a new Allegiance from a Monarch. `o` is the allegiance object (not yet, or no
/// longer, borrowed from `w.objects`).
pub fn init(w: &World, o: &mut WorldObject, monarch: ObjectGuid) {
    let allegiance = o.guid;
    let mut tree = AllegianceTree::default();
    let monarch_node = tree.new_node(monarch, allegiance, None, None);

    // find all players with this monarch
    let members = allegiance_manager::find_all_players(w, monarch);

    let patron_vassals = build_patron_vassals(w, &members);

    tree.build_chain(monarch_node, allegiance, &patron_vassals);

    let f = fields_mut(o).expect("an Allegiance");
    f.monarch = Some(monarch_node);
    f.tree = tree;
    build_members(f, monarch_node);

    //Console.WriteLine("TotalMembers: " + TotalMembers);
    build_officers(w, f);

    f.chat_filters = Some(DotNetDict::new());
}

// ACE: Allegiance.BuildPatronVassals
/// Build a mapping of patron guids => vassal guids
#[must_use]
pub fn build_patron_vassals(w: &World, members: &[IPlayer]) -> DotNetDict<u32, Vec<IPlayer>> {
    let mut patron_vassals: DotNetDict<u32, Vec<IPlayer>> = DotNetDict::new();

    for &member in members {
        let Some(patron_id) = i_player::patron_id(w, member) else {
            continue;
        };

        patron_vassals
            .get_or_insert_with(patron_id, Vec::new)
            .push(member);
    }
    patron_vassals
}

// ACE: Allegiance.BuildMembers
/// Builds the lookup table of Players => AllegianceNodes
pub fn build_members(f: &mut AllegianceFields, node: NodeId) {
    let player_guid = f.tree.node(node).player_guid;
    if f.tree.node(f.monarch_node()).player_guid == player_guid {
        f.members = Some(DotNetDict::new());
    }

    f.members
        .as_mut()
        .expect("ACE: Allegiance.Members is null (NullReferenceException)")
        .add(player_guid, node);

    for vassal in f.tree.vassals(node) {
        build_members(f, vassal);
    }
}

// ACE: Allegiance.BuildOfficers
/// `Members.Where(i => i.Value.Player.AllegianceOfficerRank != null)`, in member order.
pub fn build_officers(w: &World, f: &mut AllegianceFields) {
    let mut officers = DotNetDict::new();
    for (&guid, &node) in f.members().iter() {
        let player = allegiance_node::player(w, f.tree.node(node).player_guid)
            .expect("ACE: AllegianceNode.Player is null (NullReferenceException)");
        if pa::i_player_allegiance_officer_rank(w, player).is_some() {
            officers.add(guid, node);
        }
    }
    f.officers = Some(officers);
}

/// [`build_officers`] on a loaded allegiance.
pub fn build_officers_of(w: &mut World, allegiance: ObjectGuid) {
    let Some(mut o) = w.objects.remove(allegiance) else {
        return;
    };
    if let Some(f) = fields_mut(&mut o) {
        build_officers(w, f);
    }
    w.objects.insert(o).expect("just removed");
}

// ACE: Allegiance.Equals
/// An allegiance is defined by its monarch
#[must_use]
pub fn equals(w: &World, this: ObjectGuid, obj: ObjectGuid) -> bool {
    let (Some(a), Some(b)) = (fields_of(w, this), fields_of(w, obj)) else {
        return false;
    };
    a.tree.node(a.monarch_node()).player_guid.full()
        == b.tree.node(b.monarch_node()).player_guid.full()
}

// ACE: Allegiance.GetHashCode
/// `Monarch.PlayerGuid.Full.GetHashCode()`: a `uint`'s hash is its value.
#[must_use]
pub fn get_hash_code(w: &World, this: ObjectGuid) -> i32 {
    monarch_player_guid(w, this).full().cast_signed()
}

// ACE: Allegiance.ApprovedVassals
/// Approved vassals for adding to locked allegiances
#[must_use]
pub fn approved_vassals(o: &WorldObject) -> DotNetDict<u32, PropertiesAllegiance> {
    pae::get_approved_vassals(o.biota.properties_allegiance.as_ref())
}

// ACE: Allegiance.BanList
/// A list of players who are banned from joining.
#[must_use]
pub fn ban_list(o: &WorldObject) -> DotNetDict<u32, PropertiesAllegiance> {
    pae::get_ban_list(o.biota.properties_allegiance.as_ref())
}

// ACE: Allegiance.OnlinePlayers
/// Returns the list of allegiance members who are currently online
#[must_use]
pub fn online_players(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    members(w, this)
        .into_iter()
        .filter_map(|(member, _)| player_manager::get_online_player(w, member.full()))
        .collect()
}

// ACE: Allegiance.IsMember
/// Returns TRUE if playerGuid is a member
#[must_use]
pub fn is_member(w: &World, this: ObjectGuid, player_guid: ObjectGuid) -> bool {
    fields_of(w, this)
        .expect("ACE: Allegiance is null (NullReferenceException)")
        .members()
        .contains_key(&player_guid)
}

// ACE: Allegiance.IsOfficer
/// Returns TRUE if playerGuid is an officer
#[must_use]
pub fn is_officer(w: &World, this: ObjectGuid, player_guid: ObjectGuid) -> bool {
    fields_of(w, this)
        .expect("ACE: Allegiance is null (NullReferenceException)")
        .officers()
        .contains_key(&player_guid)
}

// ACE: Allegiance.IsOfficerRank
/// Returns TRUE if playerGuid is an officer of minimum rank
#[must_use]
pub fn is_officer_rank(
    w: &World,
    this: ObjectGuid,
    player_guid: ObjectGuid,
    officer_rank: i32,
) -> bool {
    let f = fields_of(w, this).expect("ACE: Allegiance is null (NullReferenceException)");
    let Some(&node) = f.officers().get(&player_guid) else {
        return false;
    };
    let player = allegiance_node::player(w, f.tree.node(node).player_guid)
        .expect("ACE: AllegianceNode.Player is null (NullReferenceException)");
    pa::i_player_allegiance_officer_rank(w, player).is_some_and(|r| r >= officer_rank)
}

// ACE: Allegiance.IsSpeaker
#[must_use]
pub fn is_speaker(w: &World, this: ObjectGuid, player_guid: ObjectGuid) -> bool {
    is_officer_rank(w, this, player_guid, 1)
}

// ACE: Allegiance.IsSeneschal
#[must_use]
pub fn is_seneschal(w: &World, this: ObjectGuid, player_guid: ObjectGuid) -> bool {
    is_officer_rank(w, this, player_guid, 2)
}

// ACE: Allegiance.IsCastellan
#[must_use]
pub fn is_castellan(w: &World, this: ObjectGuid, player_guid: ObjectGuid) -> bool {
    is_officer_rank(w, this, player_guid, 3)
}

// ACE: Allegiance.GetOfficerTitle
#[must_use]
pub fn get_officer_title(o: &WorldObject, officer_rank: AllegianceOfficerLevel) -> String {
    let custom = |t: Option<String>, default: &str| match t {
        Some(t) if !t.is_empty() => t,
        _ => default.to_owned(),
    };
    match officer_rank {
        AllegianceOfficerLevel::Speaker => custom(o.allegiance_speaker_title(), "Speaker"),
        AllegianceOfficerLevel::Seneschal => custom(o.allegiance_seneschal_title(), "Seneschal"),
        AllegianceOfficerLevel::Castellan => custom(o.allegiance_castellan_title(), "Castellan"),
        _ => String::new(),
    }
}

// ACE: Allegiance.HasCustomTitles
#[must_use]
pub fn has_custom_titles(o: &WorldObject) -> bool {
    o.allegiance_speaker_title().is_some()
        || o.allegiance_seneschal_title().is_some()
        || o.allegiance_castellan_title().is_some()
}

// ACE: Allegiance.GetHouse
/// The monarch's house: the loaded object when its landblock is loaded, else an offline copy
/// (`House.Load`, houses' unit: a pointer answering none).
pub fn get_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    let monarch = allegiance_node::player(w, monarch_player_guid(w, this))
        .expect("ACE: AllegianceNode.Player is null (NullReferenceException)");
    let house_instance = i_player::house_instance(w, monarch)?;

    // is landblock loaded?
    let house_guid = house_instance;
    #[allow(clippy::cast_possible_truncation)] // `(ushort)((houseGuid >> 12) & 0xFFFF)`
    let landblock = ((house_guid >> 12) & 0xFFFF) as u16;

    let landblock_id = LandblockId::new(u32::from(landblock) << 16 | 0xFFFF);
    let is_loaded = landblock_manager::is_loaded(w, landblock_id);

    if is_loaded {
        let loaded = landblock_manager::get_landblock(w, landblock_id, false, false);
        let house =
            crate::entity::landblock::get_object(w, loaded, ObjectGuid::new(house_guid), true)?;
        return w
            .objects
            .get(house)
            .is_some_and(WorldObject::is_house)
            .then_some(house);
    }

    // load an offline copy
    crate::world_objects::house::load(w, house_guid, false)
}

// ACE: Allegiance.IsFiltered
/// Returns TRUE if input player guid has an active chat filter (an expired one is removed).
pub fn is_filtered(w: &mut World, this: ObjectGuid, player_guid: ObjectGuid) -> bool {
    let now = w.now.utc;
    let f = fields_of_mut(w, this).expect("ACE: Allegiance is null (NullReferenceException)");
    let filters = f
        .chat_filters
        .as_mut()
        .expect("ACE: Allegiance.ChatFilters is null (NullReferenceException)");
    let Some(&filter) = filters.get(&player_guid) else {
        return false;
    };

    if filter > now {
        return true;
    }

    // filter has expired
    filters.remove(&player_guid);

    false
}

// ACE: Allegiance.UpdateProperties
/// Updates any dynamic properties if they have changed for allegiance members
pub fn update_properties(w: &mut World, this: ObjectGuid) {
    for (member_key, member_node) in members(w, this) {
        let player = player_manager::find_by_guid(w, member_key.full()).0;
        let online_player = player_manager::get_online_player(w, member_key.full());

        let Some(player) = player else { continue };

        let mut updated = false;

        // `member.Value.Allegiance.MonarchId`: the node's allegiance is this one
        let allegiance_monarch_id = w.objects.get(this).and_then(WorldObject::monarch_id);

        // if changed, update monarch id
        if Some(i_player::monarch_id(w, player).unwrap_or(0)) != allegiance_monarch_id {
            pa::i_player_update_property_iid(
                w,
                player,
                empyrean_entity::enums::PropertyInstanceId::Monarch,
                allegiance_monarch_id,
                true,
            );

            updated = true;
        }

        // if changed, update rank (`int` against `uint`: compared as `long`)
        let rank = fields_of(w, this)
            .expect("loaded")
            .tree
            .node(member_node)
            .rank;
        if i64::from(i_player::allegiance_rank(w, player).unwrap_or(0)) != i64::from(rank) {
            let new_rank = rank.cast_signed();
            i_player::set_allegiance_rank(w, player, Some(new_rank));

            if let Some(online_player) = online_player {
                let session = player_manager::player_session(w, online_player)
                    .expect("ACE: Player.Session is null (NullReferenceException)");
                let msg = game_message_private_update_property_int(
                    w.objects.get_mut(online_player).expect("online"),
                    PropertyInt::AllegianceRank,
                    new_rank,
                );
                game_message::enqueue_send(w, session, msg);
            }

            updated = true;
        }

        if updated {
            i_player::save_biota_to_database(w, player, true);
        }

        if let Some(online_player) = online_player {
            send_allegiance_update(
                w,
                online_player,
                Some(this),
                Some(NodeRef {
                    allegiance: this,
                    player: member_key,
                }),
            );
        }
    }
}

/// `Session.Network.EnqueueSend(new GameEventAllegianceUpdate(Session, allegiance, node))` for the
/// online `player`. Not ACE's (retail, V255): ACE followed every update with an
/// AllegianceUpdateDone; the retail server did not pair them, and sent that event with the
/// player's own 4-6 s Age update instead (`player_tick`; V277), plus once with the update the
/// client requests at login. Not ACE's (retail captures, V289): the view it
/// shows is kept as the baseline of the player's later changes.
pub fn send_allegiance_update(
    w: &mut World,
    player: ObjectGuid,
    allegiance: Option<ObjectGuid>,
    node: Option<NodeRef>,
) {
    let session = player_manager::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let (update, view) = game_event_allegiance_update::game_event_allegiance_update_with_view(
        w, session, allegiance, node,
    );
    game_message::enqueue_send(w, session, update);
    crate::world_objects::player_tick::note_allegiance_update_sent(w, player, view);
}

// ACE: Allegiance.ShowMembers
/// The member list (DIVERGE log: ACE writes it to the console; here it is logged).
pub fn show_members(w: &World, this: ObjectGuid) {
    let members = members(w, this);
    log::info!("Total members: {}", members.len());

    for (member, _) in members {
        let (player, is_online) = player_manager::find_by_guid(w, member.full());
        let prefix = if is_online { "* " } else { "" };

        let name = player
            .and_then(|p| i_player::name(w, p))
            .expect("ACE: player is null (NullReferenceException)");
        log::info!("{prefix}{name}");
    }
}

// ACE: Allegiance.ShowInfo
/// The tree (DIVERGE log: ACE writes it to the console; here it is logged).
pub fn show_info(w: &World, this: ObjectGuid) {
    let f = fields_of(w, this).expect("ACE: Allegiance is null (NullReferenceException)");
    for line in f.tree.show_info(w, f.monarch_node(), 0) {
        log::info!("{line}");
    }
}

/// `Biota.PropertiesAllegiance` of the allegiance (`NullReferenceException` when null).
fn properties_allegiance(o: &mut WorldObject) -> &mut DotNetDict<u32, PropertiesAllegiance> {
    o.biota
        .properties_allegiance
        .as_mut()
        .expect("ACE: Biota.PropertiesAllegiance is null (NullReferenceException)")
}

fn allegiance_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .expect("ACE: Allegiance is null (NullReferenceException)")
}

fn save(w: &mut World, this: ObjectGuid) {
    crate::dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
}

// ACE: Allegiance.AddBan
pub fn add_ban(w: &mut World, this: ObjectGuid, player_guid: u32) {
    let pa_dict = properties_allegiance(allegiance_mut(w, this));
    let entity = pae::get_first_or_default_by_character_id(Some(pa_dict), player_guid).cloned();

    match entity {
        None => pae::add_or_update_allegiance(pa_dict, player_guid, true, false),
        Some(entity) => {
            pae::add_or_update_allegiance(pa_dict, player_guid, true, entity.approved_vassal)
        }
    }

    // ChangesDetected = true doesn't work here,
    // since the Allegiance WO is not associated with a landblock

    save(w, this);
}

// ACE: Allegiance.RemoveBan
pub fn remove_ban(w: &mut World, this: ObjectGuid, player_guid: u32) -> bool {
    let result = remove_ban_from(properties_allegiance(allegiance_mut(w, this)), player_guid);
    if result {
        save(w, this);
    }
    result
}

/// The body of `RemoveBan` on the dictionary: true when the entry was changed (ACE then saves
/// and returns true), false when there was nothing to remove.
fn remove_ban_from(pa_dict: &mut DotNetDict<u32, PropertiesAllegiance>, player_guid: u32) -> bool {
    let Some(entity) =
        pae::get_first_or_default_by_character_id(Some(pa_dict), player_guid).cloned()
    else {
        return false;
    };

    if entity.approved_vassal {
        pae::add_or_update_allegiance(pa_dict, player_guid, false, true);
        return true;
    }

    pae::try_remove_allegiance(Some(pa_dict), player_guid)
}

// ACE: Allegiance.IsBanned
#[must_use]
pub fn is_banned(o: &WorldObject, player_guid: u32) -> bool {
    ban_list(o).contains_key(&player_guid)
}

// ACE: Allegiance.AddApprovedVassal
pub fn add_approved_vassal(w: &mut World, this: ObjectGuid, player_guid: u32) {
    let pa_dict = properties_allegiance(allegiance_mut(w, this));
    let entity = pae::get_first_or_default_by_character_id(Some(pa_dict), player_guid).cloned();

    match entity {
        None => pae::add_or_update_allegiance(pa_dict, player_guid, false, true),
        Some(entity) => pae::add_or_update_allegiance(pa_dict, player_guid, entity.banned, true),
    }

    // ChangesDetected = true doesn't work here,
    // since the Allegiance WO is not associated with a landblock

    save(w, this);
}

// ACE: Allegiance.RemoveApprovedVassal
pub fn remove_approved_vassal(w: &mut World, this: ObjectGuid, player_guid: u32) -> bool {
    let result =
        remove_approved_vassal_from(properties_allegiance(allegiance_mut(w, this)), player_guid);
    if result {
        save(w, this);
    }
    result
}

/// The body of `RemoveApprovedVassal` on the dictionary: true when the entry was changed (ACE
/// then saves and returns true), false when there was nothing to remove.
pub(crate) fn remove_approved_vassal_from(
    pa_dict: &mut DotNetDict<u32, PropertiesAllegiance>,
    player_guid: u32,
) -> bool {
    let Some(entity) =
        pae::get_first_or_default_by_character_id(Some(pa_dict), player_guid).cloned()
    else {
        return false;
    };

    if entity.banned {
        pae::add_or_update_allegiance(pa_dict, player_guid, true, false);
        return true;
    }

    pae::try_remove_allegiance(Some(pa_dict), player_guid)
}

// ACE: Allegiance.HasApprovedVassal
#[must_use]
pub fn has_approved_vassal(o: &WorldObject, player_guid: u32) -> bool {
    approved_vassals(o).contains_key(&player_guid)
}

// ---------------------------------------------------------------------------------------------
// Not ACE: what the network structures read of an allegiance and its nodes (AllegianceData,
// AllegianceProfile, AllegianceHierarchy and AppraiseInfo read them inside `Write`; see
// `network/structure/allegiance_profile.rs`).
// ---------------------------------------------------------------------------------------------

/// What `AllegianceDataExtensions.Write` reads of the node `id` of `allegiance`'s tree and its
/// player (`PlayerManager.FindByGuid(node.PlayerGuid, out playerIsOnline)`).
///
/// # Panics
/// When the player is not found, or its Gender, Heritage or Level is null (ACE's
/// `NullReferenceException` / `InvalidOperationException`).
pub fn node_data(w: &mut World, tree: &AllegianceTree, id: NodeId) -> AllegianceDataNode {
    let node = tree.node(id);
    let (player, player_is_online) = player_manager::find_by_guid(w, node.player_guid.full());
    let player = player.expect("ACE: AllegianceData player is null (NullReferenceException)");

    AllegianceDataNode {
        character_id: player.guid().full(),
        allegiance_xp_cached: i_player::allegiance_xp_cached(w, player),
        allegiance_xp_generated: pa::i_player_allegiance_xp_generated(w, player),
        player_is_online,
        is_monarch: tree.is_monarch(id),
        existed_before_allegiance_xp_changes: i_player::existed_before_allegiance_xp_changes(w, player),
        gender: i_player::gender(w, player).expect("ACE: (Gender)player.Gender of null (InvalidOperationException)"),
        heritage: i_player::heritage(w, player).expect("ACE: (HeritageGroup)player.Heritage of null (InvalidOperationException)"),
        rank: node.rank,
        level: i_player::level(w, player).expect("ACE: (uint)player.Level of null (InvalidOperationException)"),
        #[allow(clippy::cast_possible_wrap)] // stored as the C# `(ushort)` cast's input
        loyalty: pa::i_player_get_current_loyalty(w, player).cast_signed(),
        #[allow(clippy::cast_possible_wrap)]
        leadership: pa::i_player_get_current_leadership(w, player).cast_signed(),
        name: i_player::name(w, player).unwrap_or_default(),
        // Not ACE's (retail captures, V285): the tracked times sworn.
        time_online: pa::i_player_sworn_time(w, player).time_online,
        allegiance_age: pa::i_player_sworn_time(w, player).allegiance_age,
    }
}

/// The `Allegiance` an `AllegianceProfile` reads.
pub fn allegiance_view(w: &mut World, allegiance: ObjectGuid) -> AllegianceView {
    let f = fields_of(w, allegiance).expect("ACE: Allegiance is null (NullReferenceException)");
    let tree = f.tree.clone();
    let monarch = f.monarch_node();
    let o = w.objects.get(allegiance).expect("loaded");
    let allegiance_name = o.allegiance_name();
    let biota_id = o.biota.id;
    let sanctuary = o.sanctuary();
    let monarch_player = allegiance_node::player(w, tree.node(monarch).player_guid)
        .expect("ACE: AllegianceNode.Player is null (NullReferenceException)");
    let monarch_player_name = i_player::name(w, monarch_player).unwrap_or_default();
    let monarch = AllegianceData {
        node: Some(node_data(w, &tree, monarch)),
    };
    // Not ACE's (retail captures, V289): an unnamed allegiance's name time is
    // the current time.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // Unix seconds fit a u32 until 2106
    let unnamed_name_time = if allegiance_name.is_none() {
        w.now.unix_time.floor().clamp(0.0, f64::from(u32::MAX)) as u32
    } else {
        0
    };
    AllegianceView {
        allegiance_name,
        monarch_player_name,
        biota_id,
        sanctuary,
        monarch,
        unnamed_name_time,
    }
}

/// The `AllegianceNode` an `AllegianceProfile` reads, or `None` for a node no longer in its
/// allegiance.
pub fn allegiance_node_view(w: &mut World, node: NodeRef) -> Option<AllegianceNodeView> {
    let id = member_node(w, node)?;
    let tree = tree(w, node.allegiance)?.clone();
    let n = tree.node(id).clone();

    let patron = n.patron.map(|p| AllegiancePatronView {
        is_monarch: tree.is_monarch(p),
        player_guid: tree.node(p).player_guid,
        data: AllegianceData {
            node: Some(node_data(w, &tree, p)),
        },
    });
    let data = AllegianceData {
        node: Some(node_data(w, &tree, id)),
    };
    let vassals = tree
        .vassals(id)
        .into_iter()
        .map(|v| AllegianceData {
            node: Some(node_data(w, &tree, v)),
        })
        .collect();

    Some(AllegianceNodeView {
        player_guid: n.player_guid,
        is_monarch: tree.is_monarch(id),
        total_followers: tree.total_followers(id),
        total_vassals: tree.total_vassals(id),
        monarch_total_followers: tree.total_followers(n.monarch),
        monarch_player_guid: tree.node(n.monarch).player_guid,
        patron,
        data,
        vassals,
    })
}

/// `GameEventAllegianceUpdate`'s `node == null ? 0 : node.Rank`.
#[must_use]
pub fn node_rank(w: &World, node: Option<NodeRef>) -> u32 {
    node.and_then(|n| Some(tree(w, n.allegiance)?.node(member_node(w, n)?).rank))
        .unwrap_or(0)
}

/// What `AppraiseInfo.BuildProperties` reads of `player.Allegiance` and `player.AllegianceNode`
/// (`None` unless both are set).
#[must_use]
pub fn appraisal_view(
    w: &World,
    player: ObjectGuid,
) -> Option<crate::world_objects::world_object_networking::shims::AllegianceAppraisalView> {
    use crate::world_objects::world_object_networking::shims::{
        AllegianceAppraisalView, AllegianceMemberView,
    };

    let p = IPlayer::Online(player);
    let allegiance = pa::i_player_allegiance(w, p)?;
    let node = pa::i_player_allegiance_node(w, p)?;
    let id = member_node(w, node)?;
    let node_tree = tree(w, node.allegiance)?;

    let member_view = |tree: &AllegianceTree, n: NodeId| {
        let player = allegiance_node::player(w, tree.node(n).player_guid)
            .expect("ACE: node.Player is null (NullReferenceException)");
        AllegianceMemberView {
            heritage: i_player::heritage(w, player),
            gender: i_player::gender(w, player),
            rank: tree.node(n).rank,
            name: i_player::name(w, player).unwrap_or_default(),
        }
    };

    let is_monarch = node_tree.is_monarch(id);
    let mut view = AllegianceAppraisalView {
        allegiance_name: w.objects.get(allegiance)?.allegiance_name(),
        is_monarch,
        total_followers: node_tree.total_followers(id),
        ..AllegianceAppraisalView::default()
    };
    if !is_monarch {
        let f = fields_of(w, allegiance).expect("ACE: Allegiance is null (NullReferenceException)");
        view.monarch = member_view(&f.tree, f.monarch_node());
        view.patron = member_view(
            node_tree,
            node_tree.node(id).patron.expect("not the monarch"),
        );
    }
    Some(view)
}
