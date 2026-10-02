// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Managers/ContractManager.cs
//! Port of `Source/ACE.Server/WorldObjects/Managers/ContractManager.cs`.
//!
//! ACE's `ContractManager` belongs to one `Player` (`Player.ContractManager`). Its contracts live
//! in the player's shard `Character` (`CharacterPropertiesContractRegistry`); the manager itself
//! keeps only the quest flags each contract watches (`MonitoredQuestFlags`), so a quest update
//! can resend the contract's tracker. The manager is stored with the player
//! (`PlayerCharacterFields.contract_manager`) and every member takes the player's guid.

use dereth_assets::tables::Contract;
use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_dat::DatManager;
use empyrean_entity::enums::ChatMessageType;
use empyrean_entity::ObjectGuid;
use empyrean_store::models::shard::{Character, CharacterPropertiesContractRegistry};

use crate::network::game_event::events::game_event_send_client_contract_tracker::game_event_send_client_contract_tracker;
use crate::network::game_messages::game_message::{enqueue_send, BinaryWriter};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::contract_tracker::{self, ContractTracker};
use crate::network::structure::hash_comparer::{self, HashComparer};
use crate::network::structure::packable_hash_table;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: ContractManager.MaxContracts
const MAX_CONTRACTS: i32 = 100;

// ACE: ContractManager.Debug
/// `public static bool Debug = false;`
pub const DEBUG: bool = false;

/// The index of each quest flag in `Contract.strings` (the shared decoder keeps ACE's eleven
/// string fields in file order).
mod contract_strings {
    pub const NAME: usize = 0;
    pub const QUESTFLAG_STAMPED: usize = 5;
    pub const QUESTFLAG_STARTED: usize = 6;
    pub const QUESTFLAG_FINISHED: usize = 7;
    pub const QUESTFLAG_PROGRESS: usize = 8;
    pub const QUESTFLAG_TIMER: usize = 9;
    pub const QUESTFLAG_REPEAT_TIME: usize = 10;
}

// ACE: ContractManager
/// The manager's own state (the player is the owner of the field that holds it).
#[derive(Debug, Default, Clone)]
pub struct ContractManager {
    // ACE: ContractManager.MonitoredQuestFlags
    /// Contract id -> the lower-cased quest flags it watches, in .NET dictionary order (the order
    /// `NotifyOfQuestUpdate` resends trackers in).
    pub monitored_quest_flags: DotNetDict<u32, DotNetHashSet<String>>,
}

/// `string.IsNullOrWhiteSpace`: .NET's white space is Unicode `White_Space`, which Rust's
/// `char::is_whitespace` matches.
fn is_null_or_white_space(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// `string.ToLower()` under ACE's en-US culture.
fn to_lower(s: &str) -> String {
    s.to_lowercase()
}

/// `Player.Character` (a missing Character is ACE's `NullReferenceException`).
fn character(w: &World, player: ObjectGuid) -> &Character {
    w.objects
        .get(player)
        .and_then(shims::player_character)
        .expect("ACE: Player.Character is null (NullReferenceException)")
}

fn character_mut(w: &mut World, player: ObjectGuid) -> &mut Character {
    w.objects
        .get_mut(player)
        .and_then(|o| o.player.as_mut())
        .and_then(|p| p.player.character.as_mut())
        .expect("ACE: Player.Character is null (NullReferenceException)")
}

/// `Player.ContractManager`.
fn manager_mut(w: &mut World, player: ObjectGuid) -> &mut ContractManager {
    &mut w
        .objects
        .get_mut(player)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: Player is null (NullReferenceException)")
        .player_character
        .contract_manager
}

/// `Player.CharacterChangesDetected = true;`
fn set_character_changes_detected(w: &mut World, player: ObjectGuid) {
    if let Some(p) = w.objects.get_mut(player).and_then(|o| o.player.as_mut()) {
        p.player_database.character_changes_detected = true;
    }
}

/// `Player.Session`.
fn session_of(w: &World, player: ObjectGuid) -> empyrean_net::SessionId {
    shims::player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `Player.Session.Network.EnqueueSend(new GameEventSendClientContractTracker(Player.Session, contract))`.
fn send_tracker(w: &mut World, player: ObjectGuid, contract: &CharacterPropertiesContractRegistry) {
    let session = session_of(w, player);
    let msg = game_event_send_client_contract_tracker(w, session, contract);
    enqueue_send(w, session, msg);
}

// ACE: ContractManager.ContractTrackerTable
/// `Player.Character.GetContractsIds(..).ToDictionary(c => c, c => new ContractTracker(Player, c))`,
/// in the dictionary's (insertion) order.
#[must_use]
pub fn contract_tracker_table(w: &World, player: ObjectGuid) -> Vec<(u32, ContractTracker)> {
    let contract_ids = character(w, player).get_contracts_ids();

    contract_ids
        .into_iter()
        .map(|c| {
            (
                c,
                contract_tracker::contract_tracker_from_id(w, Some(player), c),
            )
        })
        .collect()
}

// ACE: ContractManager.ContractManager
/// Constructs a new ContractManager for a Player: `RefreshMonitoredQuestFlags` over the player's
/// Character.
#[must_use]
pub fn contract_manager_new(dats: &DatManager, character: &Character) -> ContractManager {
    let mut manager = ContractManager::default();
    refresh_monitored_quest_flags_from(&mut manager, dats, character);
    manager
}

fn refresh_monitored_quest_flags_from(
    manager: &mut ContractManager,
    dats: &DatManager,
    character: &Character,
) {
    use contract_strings as cs;

    let monitored_quest_flags = &mut manager.monitored_quest_flags;
    monitored_quest_flags.clear();

    let contracts = character.get_contracts();

    for contract in contracts {
        let dat_contract = get_contract_from_dat_in(dats, contract.contract_id)
            .expect("ACE: datContract is null (NullReferenceException)");

        if !monitored_quest_flags.contains_key(&contract.contract_id) {
            monitored_quest_flags.add(contract.contract_id, DotNetHashSet::new());
        }

        let flags = monitored_quest_flags
            .get_mut(&dat_contract.contract_id)
            .expect("ACE: MonitoredQuestFlags[datContract.ContractId] (KeyNotFoundException)");
        for index in [
            cs::QUESTFLAG_FINISHED,
            cs::QUESTFLAG_PROGRESS,
            cs::QUESTFLAG_REPEAT_TIME,
            cs::QUESTFLAG_STAMPED,
            cs::QUESTFLAG_STARTED,
            cs::QUESTFLAG_TIMER,
        ] {
            let flag = &dat_contract.strings[index];
            if !is_null_or_white_space(flag) {
                flags.insert(to_lower(flag));
            }
        }
    }
}

// ACE: ContractManager.RefreshMonitoredQuestFlags
fn refresh_monitored_quest_flags(w: &mut World, player: ObjectGuid) {
    let dats = std::sync::Arc::clone(&w.dats);
    let character = character(w, player).clone();
    refresh_monitored_quest_flags_from(manager_mut(w, player), &dats, &character);
}

// ACE: ContractManager.GetContractTracker
#[must_use]
pub fn get_contract_tracker(
    w: &World,
    player: ObjectGuid,
    contract_id: u32,
) -> Option<ContractTracker> {
    if get_contract(w, player, contract_id).is_some() {
        return Some(contract_tracker::contract_tracker_from_id(
            w,
            Some(player),
            contract_id,
        ));
    }

    None
}

fn get_contract_from_dat_in(dats: &DatManager, contract_id: u32) -> Option<&Contract> {
    // A dat set from before Throne of Destiny has no contract table: no contract is found.
    dats.portal_dat()
        .try_contract_table()?
        .contracts
        .get(&contract_id)
}

// ACE: ContractManager.GetContractFromDat
/// Returns a contract from the portal dat file
#[must_use]
pub fn get_contract_from_dat(w: &World, contract_id: u32) -> Option<&Contract> {
    get_contract_from_dat_in(&w.dats, contract_id)
}

// ACE: ContractManager.GetContract
/// Returns an active or completed contract for this player
#[must_use]
pub fn get_contract(
    w: &World,
    player: ObjectGuid,
    contract_id: u32,
) -> Option<CharacterPropertiesContractRegistry> {
    character(w, player).get_contract(contract_id).cloned()
}

// ACE: ContractManager.IsFull
/// Returns TRUE if at max capacity for Contracts for this player
#[must_use]
pub fn is_full(w: &World, player: ObjectGuid) -> bool {
    character(w, player).get_contracts_count() >= MAX_CONTRACTS
}

// ACE: ContractManager.Add
/// `Add(int contractId)`: `Convert.ToUInt32` throws `OverflowException` for a negative id.
pub fn add_int(w: &mut World, player: ObjectGuid, contract_id: i32) -> bool {
    add(
        w,
        player,
        u32::try_from(contract_id).expect("ACE: Convert.ToUInt32 (OverflowException)"),
    )
}

// ACE: ContractManager.Add
/// Adds a new contract to the player's registry
pub fn add(w: &mut World, player: ObjectGuid, contract_id: u32) -> bool {
    let Some(dat_contract) = get_contract_from_dat(w, contract_id) else {
        if DEBUG {
            empyrean_common::console_write_line!(debug: "ContractManager.Add({contract_id}): Contract not found in DAT file.");
        }
        return false;
    };
    let contract_name = dat_contract.strings[contract_strings::NAME].clone();

    if is_full(w, player) {
        //Player.Session.Network.EnqueueSend(new GameEventWeenieError(Player.Session, WeenieError.ContractError));
        //what happened here in retail?

        let session = session_of(w, player);
        let msg = game_message_system_chat(
            &format!("You currently have the maximum amount of contracts for this character and cannot take on another! You must abandon at least one contract before you can accept the contract for {contract_name}."),
            ChatMessageType::Broadcast,
        );
        enqueue_send(w, session, msg);
        return false;
    }

    let (contract, contract_was_created) =
        character_mut(w, player).get_or_create_contract(contract_id);

    if contract_was_created {
        // add new contract entry
        contract.delete_contract = false;
        contract.set_as_display_contract = false;
        let contract = contract.clone();

        if DEBUG {
            empyrean_common::console_write_line!(debug: "ContractManager.Add({contract_id}): added contract: {contract_name}");
        }

        set_character_changes_detected(w, player);
        send_tracker(w, player, &contract);

        refresh_monitored_quest_flags(w, player);
    } else if DEBUG {
        empyrean_common::console_write_line!(debug: "ContractManager.Add({contract_id}): contract for {contract_name} already exists in registry.");

        // contracts dupes are also successful without actually duping into registry.
        //return false;
    }

    true
}

// ACE: ContractManager.HasContract
/// Returns TRUE if a player has a particular contract
#[must_use]
pub fn has_contract(w: &World, player: ObjectGuid, contract_id: u32) -> bool {
    let has_contract = get_contract(w, player, contract_id).is_some();

    if DEBUG {
        empyrean_common::console_write_line!(debug: "HasContracts({contract_id}): {has_contract}");
    }

    has_contract
}

// ACE: ContractManager.Abandon
/// Abandon a contract in the Player's registry
pub fn abandon(w: &mut World, player: ObjectGuid, contract_id: u32) {
    erase(w, player, contract_id);
}

// ACE: ContractManager.Erase
/// Erase a contract in the Player's registry
pub fn erase(w: &mut World, player: ObjectGuid, contract_id: u32) {
    if DEBUG {
        empyrean_common::console_write_line!(debug: "ContractManager.Erase({contract_id})");
    }

    let contract_erased = character_mut(w, player).erase_contract(contract_id);

    if let Some(mut contract_erased) = contract_erased {
        contract_erased.delete_contract = true;

        set_character_changes_detected(w, player);
        send_tracker(w, player, &contract_erased);

        refresh_monitored_quest_flags(w, player);
    }
}

// ACE: ContractManager.EraseAll
/// Erase all contracts in registry
pub fn erase_all(w: &mut World, player: ObjectGuid) {
    if DEBUG {
        empyrean_common::console_write_line!(debug: "ContractManager.EraseAll");
    }

    let erased_contracts = character_mut(w, player).erase_all_contracts();

    for mut contract in erased_contracts {
        contract.delete_contract = true;

        set_character_changes_detected(w, player);
        send_tracker(w, player, &contract);
    }

    refresh_monitored_quest_flags(w, player);
}

// ACE: ContractManager.NotifyOfQuestUpdate
pub fn notify_of_quest_update(w: &mut World, player: ObjectGuid, quest_name: &str) {
    let quest_name = to_lower(quest_name);
    let contracts: Vec<u32> = manager_mut(w, player)
        .monitored_quest_flags
        .iter()
        .filter(|(_, flags)| flags.contains(quest_name.as_str()))
        .map(|(k, _)| *k)
        .collect();
    for contract_id in contracts {
        update(w, player, contract_id);
    }
}

// ACE: ContractManager.Update
fn update(w: &mut World, player: ObjectGuid, contract_id: u32) {
    if DEBUG {
        empyrean_common::console_write_line!(debug: "ContractManager.Update");
    }

    if let Some(contract) = get_contract(w, player, contract_id) {
        send_tracker(w, player, &contract);
    }
}

// ACE: ContractManagerExtensions.hashComparer
/// static table size from retail pcaps
const HASH_COMPARER: HashComparer = HashComparer::new(32);

// ACE: ContractManagerExtensions.Write
/// `writer.Write(ContractManager contractManager)`: its `ContractTrackerTable`.
pub fn write(writer: &mut Vec<u8>, w: &World, player: ObjectGuid) {
    let mut table = contract_tracker_table(w, player);
    write_table(writer, w, &mut table);
}

// ACE: ContractManagerExtensions.Write
/// `writer.Write(Dictionary<uint, ContractTracker> contractTable)`: the header, then the trackers
/// in the hash comparer's order.
pub fn write_table(writer: &mut Vec<u8>, w: &World, contract_table: &mut [(u32, ContractTracker)]) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // `Dictionary.Count`
    packable_hash_table::write_header(
        writer,
        contract_table.len() as i32,
        i32::from(HASH_COMPARER.num_buckets),
    );

    let contract_trackers = hash_comparer::sorted(contract_table.iter().cloned(), &HASH_COMPARER);

    for (key, mut value) in contract_trackers {
        writer.write_u32(key);
        contract_tracker::write(writer, w, &mut value);
    }
}
