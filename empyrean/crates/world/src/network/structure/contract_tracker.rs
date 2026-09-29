// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/ContractTracker.cs
//! Port of `Source/ACE.Server/Network/Structure/ContractTracker.cs`.

use dereth_assets::tables::Contract;
use empyrean_entity::ObjectGuid;

pub use empyrean_store::models::shard::CharacterPropertiesContractRegistry;

use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: ContractStage
/// `enum ContractStage` (`int`). `ProgressCounter + progress` produces values past the declared
/// members, so it is a plain `i32`.
#[allow(non_snake_case, non_upper_case_globals)]
pub mod ContractStage {
    pub const Available: i32 = 0x1;
    pub const InProgress: i32 = 0x2;
    pub const DoneOrPendingRepeat: i32 = 0x3;
    pub const ProgressCounter: i32 = 0x4;
}

/// The index of each quest flag in `Contract.strings` (the shared decoder keeps ACE's eleven
/// string fields in file order).
mod contract_strings {
    pub const QUESTFLAG_STAMPED: usize = 5;
    pub const QUESTFLAG_STARTED: usize = 6;
    pub const QUESTFLAG_FINISHED: usize = 7;
    pub const QUESTFLAG_PROGRESS: usize = 8;
    pub const QUESTFLAG_TIMER: usize = 9;
    pub const QUESTFLAG_REPEAT_TIME: usize = 10;
}

/// `string.IsNullOrWhiteSpace`: .NET's white space is Unicode `White_Space`, which Rust's
/// `char::is_whitespace` matches.
fn is_null_or_white_space(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

// ACE: ContractTracker
#[derive(Debug, Clone, PartialEq)]
pub struct ContractTracker {
    // ACE: ContractTracker.ContractId
    pub contract_id: u32,
    // ACE: ContractTracker.Stage
    pub stage: i32,
    // ACE: ContractTracker.TimeWhenDone
    pub time_when_done: f64,
    // ACE: ContractTracker.TimeWhenRepeats
    pub time_when_repeats: f64,

    // ACE: ContractTracker.DeleteContract
    /// Not sure if retail servers always kept contracts and used this to hide them, but we
    /// discard so this exists only to force client to remove it from list.
    pub delete_contract: bool,
    // ACE: ContractTracker.SetAsDisplayContract
    /// depreciated?
    pub set_as_display_contract: bool,

    /// `_contract`, the cached `Contract` (not sent in network structure).
    contract: Option<Contract>,
}

impl Default for ContractTracker {
    // ACE: ContractTracker.ContractTracker
    /// `new ContractTracker()`: `Stage = Available`.
    fn default() -> Self {
        ContractTracker {
            contract_id: 0,
            stage: ContractStage::Available,
            time_when_done: 0.0,
            time_when_repeats: 0.0,
            delete_contract: false,
            set_as_display_contract: false,
            contract: None,
        }
    }
}

impl ContractTracker {
    // ACE: ContractTracker.Contract
    /// The contract from the portal dat's `ContractTable`, cached on first read (`None` when the
    /// table has no such contract).
    pub fn contract(&mut self, w: &World) -> Option<&Contract> {
        if self.contract.is_none() {
            if let Some(contract_data) = w
                .dats
                .portal_dat()
                .contract_table()
                .contracts
                .get(&self.contract_id)
            {
                self.contract = Some(contract_data.clone());
            }
        }
        self.contract.as_ref()
    }

    // ACE: ContractTracker.Version
    /// `Contract.Version`: a missing contract is ACE's `NullReferenceException`.
    pub fn version(&mut self, w: &World) -> u32 {
        self.contract(w)
            .expect("ACE: Contract is null (NullReferenceException)")
            .version
    }

    /// `Contract.<questflag>`: a missing contract is ACE's `NullReferenceException`.
    fn questflag(&mut self, w: &World, index: usize) -> String {
        self.contract(w)
            .expect("ACE: Contract is null (NullReferenceException)")
            .strings[index]
            .clone()
    }

    // ACE: ContractTracker.Init
    /// `Init(uint contractId)`.
    fn init(&mut self, contract_id: u32) {
        self.contract_id = contract_id;
    }

    // ACE: ContractTracker.Init
    /// `Init(uint contractId, Player player)`: the stage from the player's quest flags.
    fn init_player(&mut self, w: &World, contract_id: u32, player: Option<ObjectGuid>) {
        use contract_strings as cs;

        self.init(contract_id);

        let Some(player) = player else { return };

        // Started, Stamped, Timer or Progress
        self.check_and_set_stage(w, player);

        let finished = self.questflag(w, cs::QUESTFLAG_FINISHED);
        if !is_null_or_white_space(&finished)
            && shims::quest_manager_has_quest(w, player, &finished)
        {
            self.stage = ContractStage::DoneOrPendingRepeat;
        }

        let repeat_time = self.questflag(w, cs::QUESTFLAG_REPEAT_TIME);
        if !is_null_or_white_space(&repeat_time)
            && shims::quest_manager_has_quest(w, player, &repeat_time)
        {
            self.time_when_repeats =
                shims::quest_manager_get_next_solve_time_total_seconds(w, player, &repeat_time);

            if self.time_when_repeats > 0.0 {
                self.stage = ContractStage::DoneOrPendingRepeat;
            } else {
                self.stage = ContractStage::Available;

                // Recheck for Started, Stamped, Timer or Progress and update accordingly
                self.check_and_set_stage(w, player);
            }
        }
    }

    // ACE: ContractTracker.CheckAndSetStage
    fn check_and_set_stage(&mut self, w: &World, player: ObjectGuid) {
        use contract_strings as cs;

        let started = self.questflag(w, cs::QUESTFLAG_STARTED);
        if !is_null_or_white_space(&started) && shims::quest_manager_has_quest(w, player, &started)
        {
            self.stage = ContractStage::InProgress;
        }

        let stamped = self.questflag(w, cs::QUESTFLAG_STAMPED);
        if !is_null_or_white_space(&stamped) && shims::quest_manager_has_quest(w, player, &stamped)
        {
            self.stage = ContractStage::InProgress;
        }

        let timer = self.questflag(w, cs::QUESTFLAG_TIMER);
        if !is_null_or_white_space(&timer) && shims::quest_manager_has_quest(w, player, &timer) {
            self.time_when_done =
                shims::quest_manager_get_next_solve_time_total_seconds(w, player, &timer);

            self.stage = ContractStage::InProgress;
        }

        let progress_flag = self.questflag(w, cs::QUESTFLAG_PROGRESS);
        if !is_null_or_white_space(&progress_flag)
            && shims::quest_manager_has_quest(w, player, &progress_flag)
        {
            let progress =
                shims::quest_manager_get_quest_num_times_completed(w, player, &progress_flag);

            self.stage = if progress > 0 {
                ContractStage::ProgressCounter.wrapping_add(progress)
            } else {
                ContractStage::InProgress
            };
        }
    }
}

// ACE: ContractTracker.ContractTracker
/// `new ContractTracker(Player player, CharacterPropertiesContractRegistry contract)`.
pub fn contract_tracker_new(
    w: &World,
    player: ObjectGuid,
    contract: &CharacterPropertiesContractRegistry,
) -> ContractTracker {
    let mut t = ContractTracker::default();
    t.init_player(w, contract.contract_id, Some(player));

    t.delete_contract = contract.delete_contract;
    t.set_as_display_contract = contract.set_as_display_contract;
    t
}

// ACE: ContractTracker.ContractTracker
/// `new ContractTracker(Player player, uint contractId)`; a `None` player only sets the id.
pub fn contract_tracker_from_id(
    w: &World,
    player: Option<ObjectGuid>,
    contract_id: u32,
) -> ContractTracker {
    let mut t = ContractTracker::default();
    t.init_player(w, contract_id, player);
    t
}

// ACE: ContractTrackerExtensions.Write
/// `writer.Write(ContractTracker contractTracker)`. `Version` reads the contract, so the tracker
/// is borrowed mutably (its cache may fill).
pub fn write(writer: &mut Vec<u8>, w: &World, contract_tracker: &mut ContractTracker) {
    let record = record(w, contract_tracker);
    write_record(writer, &[], |w| record.write(w));

    // This is not written here.
    //writer.Write(Convert.ToUInt32(contractTracker.DeleteContract));
    //writer.Write(Convert.ToUInt32(contractTracker.SetAsDisplayContract));
}

/// The dereth-protocol record the `Write` extension below writes, field for field. `Version` reads the contract, so the tracker is borrowed mutably (its cache may fill).
#[must_use]
pub fn record(
    w: &World,
    contract_tracker: &mut ContractTracker,
) -> dereth_protocol::social::ContractTracker {
    dereth_protocol::social::ContractTracker {
        version: contract_tracker.version(w),
        contract_id: contract_tracker.contract_id,
        contract_stage: contract_tracker.stage.cast_unsigned(),
        time_when_done: contract_tracker.time_when_done,
        time_when_repeats: contract_tracker.time_when_repeats,
    }
}
