// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/QuestManager.cs
//! Port of `Source/ACE.Server/Managers/QuestManager.cs`.
//!
//! ACE's `QuestManager` belongs to a `Creature` (almost always a `Player`) or to a `Fellowship`.
//! A player's quests live in its shard `Character` (`CharacterPropertiesQuestRegistry`, saved with
//! the character); any other owner keeps them in the manager's `runtimeQuests`.
//!
//! Here a manager is addressed through [`QuestOwner`]: `Creature(guid)` finds the creature in
//! `World.objects` (its lazily created manager is `CreatureFields.quest_manager`, ACE's
//! `Creature._questManager`), and `Fellowship(&mut QuestManager)` is a fellowship's own manager
//! (the fellowship's store entry holds the [`QuestManager`] and lends it).
//! Read-only members take `&World`: a creature's manager that was never created reads as an
//! empty one, which is what ACE's lazy getter would create.

use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_entity::enums::{ChatMessageType, WeenieError};
use empyrean_entity::ObjectGuid;
use empyrean_store::models::shard::{Character, CharacterPropertiesQuestRegistry};

use crate::network::game_event::events::game_event_inventory_server_save_failed::game_event_inventory_server_save_failed;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_messages::game_message::{enqueue_send, enqueue_send_many, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: QuestManager.Debug
/// `public static bool Debug = false;`
pub const DEBUG: bool = false;

/// ACE's `runtimeQuests`: a `HashSet<CharacterPropertiesQuestRegistry>` of reference-compared
/// rows. Its enumeration order is .NET's (dense entries, freed slots reused most recent first),
/// so the set is a [`DotNetDict`] keyed by each row's identity.
#[derive(Debug, Default, Clone)]
pub struct RuntimeQuests {
    rows: DotNetDict<u64, CharacterPropertiesQuestRegistry>,
    next_identity: u64,
}

impl RuntimeQuests {
    /// The rows in `HashSet` enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = &CharacterPropertiesQuestRegistry> {
        self.rows.values()
    }

    fn find(&self, quest_name: &str) -> Option<u64> {
        self.rows
            .iter()
            .find(|(_, q)| ordinal_ignore_case_eq(&q.quest_name, quest_name))
            .map(|(k, _)| *k)
    }

    fn add(&mut self, row: CharacterPropertiesQuestRegistry) -> u64 {
        let id = self.next_identity;
        self.next_identity += 1;
        self.rows.add(id, row);
        id
    }
}

// ACE: QuestManager
/// The state of one `QuestManager` object: a creature's (`CreatureFields.quest_manager`) or a
/// fellowship's.
#[derive(Debug, Default, Clone)]
pub struct QuestManager {
    // ACE: QuestManager.Fellowship
    /// `Fellowship.FellowshipName` for a fellowship's manager (read by `Name`); `None` for a
    /// creature's.
    pub fellowship_name: Option<String>,
    // ACE: QuestManager.runtimeQuests
    pub runtime_quests: RuntimeQuests,
}

impl QuestManager {
    // ACE: QuestManager.QuestManager
    /// `new QuestManager(Creature creature)`: the creature is the manager's owner
    /// ([`QuestOwner::Creature`]).
    #[must_use]
    pub fn new_creature() -> Self {
        Self::default()
    }

    // ACE: QuestManager.QuestManager
    /// `new QuestManager(Fellowship fellowship)`.
    #[must_use]
    pub fn new_fellowship(fellowship_name: &str) -> Self {
        Self {
            fellowship_name: Some(fellowship_name.to_owned()),
            runtime_quests: RuntimeQuests::default(),
        }
    }
}

// ACE: QuestManager.Creature
/// Which `QuestManager` a call addresses.
#[derive(Debug)]
pub enum QuestOwner<'a> {
    /// `creature.QuestManager`: this is almost always a Player, however there are some rare cases
    /// of Creatures having quests such as 'chickencrossingroad'.
    Creature(ObjectGuid),
    /// `fellowship.QuestManager`.
    Fellowship(&'a mut QuestManager),
}

impl QuestOwner<'_> {
    /// `Creature`, or `None` for a fellowship.
    #[must_use]
    pub fn creature(&self) -> Option<ObjectGuid> {
        match self {
            QuestOwner::Creature(g) => Some(*g),
            QuestOwner::Fellowship(_) => None,
        }
    }
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`: each character upper-cased on its
/// own (the simple mapping), never expanded.
fn ordinal_ignore_case_eq(a: &str, b: &str) -> bool {
    fn fold(c: char) -> char {
        let mut upper = c.to_uppercase();
        match (upper.next(), upper.next()) {
            (Some(u), None) => u,
            _ => c,
        }
    }
    a.chars().map(fold).eq(b.chars().map(fold))
}

/// `Creature is Player player`: the player's guid.
fn player_of(w: &World, owner: &QuestOwner<'_>) -> Option<ObjectGuid> {
    let g = owner.creature()?;
    w.objects.get(g).filter(|o| o.is_player()).map(|_| g)
}

/// `player.Character` (a missing Character is ACE's `NullReferenceException`).
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

/// `player.CharacterChangesDetected = true;`
fn set_character_changes_detected(w: &mut World, player: ObjectGuid) {
    if let Some(p) = w.objects.get_mut(player).and_then(|o| o.player.as_mut()) {
        p.player_database.character_changes_detected = true;
    }
}

/// `player.ContractManager.NotifyOfQuestUpdate(questName)`.
fn notify_of_quest_update(w: &mut World, player: ObjectGuid, quest_name: &str) {
    crate::world_objects::managers::contract_manager::notify_of_quest_update(w, player, quest_name);
}

/// The manager's `runtimeQuests` for reading (a creature whose manager was never created has
/// none).
fn runtime<'w>(w: &'w World, owner: &'w QuestOwner<'_>) -> Option<&'w RuntimeQuests> {
    match owner {
        QuestOwner::Creature(g) => w
            .objects
            .get(*g)
            .expect("ACE: QuestManager.Creature is not in the world")
            .creature
            .as_ref()
            .expect("ACE: QuestManager.Creature is a Creature")
            .creature
            .quest_manager
            .as_ref()
            .map(|q| &q.runtime_quests),
        QuestOwner::Fellowship(qm) => Some(&qm.runtime_quests),
    }
}

// ACE: Creature.QuestManager
/// The manager's `runtimeQuests` for writing (`Creature.QuestManager` creates the manager on
/// first use).
fn runtime_mut<'w>(w: &'w mut World, owner: &'w mut QuestOwner<'_>) -> &'w mut RuntimeQuests {
    match owner {
        QuestOwner::Creature(g) => {
            &mut w
                .objects
                .get_mut(*g)
                .expect("ACE: QuestManager.Creature is not in the world")
                .creature
                .as_mut()
                .expect("ACE: QuestManager.Creature is a Creature")
                .creature
                .quest_manager
                .get_or_insert_with(QuestManager::new_creature)
                .runtime_quests
        }
        QuestOwner::Fellowship(qm) => &mut qm.runtime_quests,
    }
}

/// `(uint)Time.GetUnixTime()`.
fn unix_time_now(w: &World) -> u32 {
    w.now.unix_time.cs_cast()
}

// ACE: QuestManager.Name
#[must_use]
pub fn name(w: &World, owner: &QuestOwner<'_>) -> String {
    match owner {
        QuestOwner::Creature(g) => crate::dispatch::name::name(w, *g).unwrap_or_default(),
        QuestOwner::Fellowship(qm) => {
            format!(
                "Fellowship({})",
                qm.fellowship_name.as_deref().unwrap_or_default()
            )
        }
    }
}

// ACE: QuestManager.IDtoUseForQuestRegistry
#[must_use]
pub fn id_to_use_for_quest_registry(owner: &QuestOwner<'_>) -> u32 {
    match owner {
        QuestOwner::Creature(g) => g.full(),
        //return Fellowship.FellowshipLeaderGuid;
        QuestOwner::Fellowship(_) => 1,
    }
}

// ACE: QuestManager.GetQuests
/// This will return a clone of the quests collection. You should not mutate the results.
/// This is mostly used for information/debugging
#[must_use]
pub fn get_quests(w: &World, owner: &QuestOwner<'_>) -> Vec<CharacterPropertiesQuestRegistry> {
    if let Some(player) = player_of(w, owner) {
        return character(w, player).get_quests();
    }

    // Not a player
    runtime(w, owner)
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

// ACE: QuestManager.HasQuest
/// Returns TRUE if a player has started a particular quest
#[must_use]
pub fn has_quest(w: &World, owner: &QuestOwner<'_>, quest_format: &str) -> bool {
    let quest_name = get_quest_name(quest_format);
    let has_quest = get_quest(w, owner, quest_name).is_some();

    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.HasQuest({quest_format}): {has_quest}",
            name(w, owner)
        );
    }

    has_quest
}

/// `Int32.TryParse(s, out _)` (`NumberStyles.Integer`, en-US): optional white space around an
/// optional sign and decimal digits; trailing NULs are ignored.
fn int32_try_parse(s: &str) -> Option<i32> {
    let is_white = |c: char| matches!(c, '\u{9}'..='\u{D}' | ' ');
    let s = s.trim_end_matches('\0').trim_matches(is_white);
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i32>().ok()
}

// ACE: QuestManager.HasQuestCompletes
#[must_use]
pub fn has_quest_completes(w: &World, owner: &QuestOwner<'_>, quest_name: &str) -> bool {
    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.HasQuestCompletes({quest_name})",
            name(w, owner)
        );
    }

    if !quest_name.contains('@') {
        return has_quest(w, owner, quest_name);
    }

    let pieces: Vec<&str> = quest_name.split('@').collect();
    if pieces.len() != 2 {
        empyrean_common::console_write_line!(
            "{}.QuestManager.HasQuestCompletes({quest_name}): error parsing quest name",
            name(w, owner)
        );
        return false;
    }
    let name_only = pieces[0];
    let Some(num_completes) = int32_try_parse(pieces[1]) else {
        empyrean_common::console_write_line!(
            "{}.QuestManager.HasQuestCompletes({quest_name}): unknown quest format",
            name(w, owner)
        );
        return has_quest(w, owner, quest_name);
    };
    let Some(quest) = get_quest(w, owner, name_only) else {
        return false;
    };

    let success = quest.num_times_completed == num_completes; // minimum or exact?
    if DEBUG {
        empyrean_common::console_write_line!(debug: "{success}");
    }
    success
}

// ACE: QuestManager.GetQuest
/// Returns an active or completed quest for this player
#[must_use]
pub fn get_quest(
    w: &World,
    owner: &QuestOwner<'_>,
    quest_name: &str,
) -> Option<CharacterPropertiesQuestRegistry> {
    if let Some(player) = player_of(w, owner) {
        return character(w, player).get_quest(quest_name).cloned();
    }

    // Not a player
    let r = runtime(w, owner)?;
    r.find(quest_name).and_then(|k| r.rows.get(&k)).cloned()
}

/// A registry row, found by name, to update in place (the player's `Character` or
/// `runtimeQuests`).
fn quest_row_mut<'w>(
    w: &'w mut World,
    owner: &'w mut QuestOwner<'_>,
    player: Option<ObjectGuid>,
    quest_name: &str,
) -> Option<&'w mut CharacterPropertiesQuestRegistry> {
    if let Some(player) = player {
        let c = character_mut(w, player);
        let i = c
            .character_properties_quest_registry
            .iter()
            .position(|q| ordinal_ignore_case_eq(&q.quest_name, quest_name))?;
        return c.character_properties_quest_registry.get_mut(i);
    }
    let r = runtime_mut(w, owner);
    let k = r.find(quest_name)?;
    r.rows.get_mut(&k)
}

// ACE: QuestManager.GetOrCreateQuest
/// The row (to update in place) and whether it was created.
fn get_or_create_quest<'w>(
    w: &'w mut World,
    owner: &'w mut QuestOwner<'_>,
    quest_name: &str,
) -> (&'w mut CharacterPropertiesQuestRegistry, bool) {
    if let Some(player) = player_of(w, owner) {
        return character_mut(w, player).get_or_create_quest(quest_name);
    }

    // Not a player
    let r = runtime_mut(w, owner);
    let (k, quest_registry_was_created) = match r.find(quest_name) {
        Some(k) => (k, false),
        None => (
            r.add(CharacterPropertiesQuestRegistry {
                quest_name: quest_name.to_owned(),
                ..Default::default()
            }),
            true,
        ),
    };
    (
        r.rows.get_mut(&k).expect("the row just found or added"),
        quest_registry_was_created,
    )
}

// ACE: QuestManager.Update
/// Adds or updates a quest completion to the player's registry
pub fn update(w: &mut World, owner: &mut QuestOwner<'_>, quest_format: &str) {
    let quest_name = get_quest_name(quest_format);
    let now = unix_time_now(w);
    let id = id_to_use_for_quest_registry(owner);
    let player = player_of(w, owner);

    let (quest, quest_registry_was_created) = get_or_create_quest(w, owner, quest_name);

    let registered_name = if quest_registry_was_created {
        quest.last_time_completed = now;
        quest.num_times_completed = 1; // initial add / first solve

        quest.character_id = id;
        let registered_name = quest.quest_name.clone();

        if DEBUG {
            empyrean_common::console_write_line!(debug: "{}.QuestManager.Update({registered_name}): added quest",
                name(w, owner)
            );
        }
        registered_name
    } else {
        if is_max_solves(w, owner, quest_name) {
            if DEBUG {
                empyrean_common::console_write_line!(debug: "{}.QuestManager.Update({quest_name}): can not update existing quest. IsMaxSolves({quest_name}) is true.", name(w, owner));
            }
            return;
        }

        // update existing quest
        let quest = quest_row_mut(w, owner, player, quest_name).expect("the existing quest");
        quest.last_time_completed = now;
        quest.num_times_completed = quest.num_times_completed.wrapping_add(1);
        let (registered_name, n) = (quest.quest_name.clone(), quest.num_times_completed);

        if DEBUG {
            empyrean_common::console_write_line!(debug: "{}.QuestManager.Update({registered_name}): updated quest ({n})",
                name(w, owner)
            );
        }
        registered_name
    };

    if let Some(player) = player {
        set_character_changes_detected(w, player);

        notify_of_quest_update(w, player, &registered_name);
    }
}

// ACE: QuestManager.SetQuestCompletions
/// Initialize a quest completion with the provided number to the player's registry
/// (`questCompletions` defaults to 0 in ACE).
///
/// # Panics
/// `Math.Abs(int.MinValue)` (`OverflowException`) for a quest with no solve limit.
pub fn set_quest_completions(
    w: &mut World,
    owner: &mut QuestOwner<'_>,
    quest_format: &str,
    quest_completions: i32,
) {
    let quest_name = get_quest_name(quest_format);

    let max_solves = get_max_solves(w, quest_name);

    let num_times_completed = if max_solves > -1 {
        quest_completions.min(max_solves)
    } else {
        quest_completions
            .checked_abs()
            .expect("ACE: Math.Abs(int.MinValue) (OverflowException)")
    };

    let now = unix_time_now(w);
    let id = id_to_use_for_quest_registry(owner);
    let player = player_of(w, owner);

    let (quest, quest_registry_was_created) = get_or_create_quest(w, owner, quest_name);

    if quest_registry_was_created {
        quest.last_time_completed = now;
        quest.num_times_completed = num_times_completed; // initialize the quest to the given completions

        quest.character_id = id;
    } else {
        // update existing quest
        quest.last_time_completed = now;
        quest.num_times_completed = num_times_completed;
    }
    let registered_name = quest.quest_name.clone();

    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.SetQuestCompletions({quest_format}): initialized quest to {num_times_completed}", name(w, owner));
    }

    if let Some(player) = player {
        set_character_changes_detected(w, player);

        notify_of_quest_update(w, player, &registered_name);
    }
}

// ACE: QuestManager.CanSolve
/// Returns TRUE if player can solve this quest now
#[must_use]
pub fn can_solve(w: &World, owner: &QuestOwner<'_>, quest_format: &str) -> bool {
    let quest_name = get_quest_name(quest_format);

    // verify max solves / quest timer
    let next_solve_time = get_next_solve_time(w, owner, quest_name);

    let can_solve = next_solve_time == TimeSpan::MIN_VALUE;
    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.CanSolve({quest_name}): {can_solve}",
            name(w, owner)
        );
    }
    can_solve
}

// ACE: QuestManager.IsMaxSolves
/// Returns TRUE if player has reached the maximum # of solves for this quest
#[must_use]
pub fn is_max_solves(w: &World, owner: &QuestOwner<'_>, quest_name: &str) -> bool {
    let Some(quest) = w.content.get_cached_quest(quest_name) else {
        return false;
    };

    let Some(player_quest) = get_quest(w, owner, quest_name) else {
        return false; // player hasn't completed this quest yet
    };

    // return TRUE if quest has solve limit, and it has been reached
    quest.max_solves > -1 && player_quest.num_times_completed >= quest.max_solves
}

// ACE: QuestManager.GetMaxSolves
/// Returns the maximum # of solves for this quest (0 for a quest the world database lacks).
#[must_use]
pub fn get_max_solves(w: &World, quest_format: &str) -> i32 {
    let quest_name = get_quest_name(quest_format);

    let Some(quest) = w.content.get_cached_quest(quest_name) else {
        return 0;
    };

    quest.max_solves
}

// ACE: QuestManager.GetCurrentSolves
/// Returns the current # of solves for this quest
#[must_use]
pub fn get_current_solves(w: &World, owner: &QuestOwner<'_>, quest_format: &str) -> i32 {
    let quest_name = get_quest_name(quest_format);

    let Some(quest) = get_quest(w, owner, quest_name) else {
        return 0;
    };

    quest.num_times_completed
}

// ACE: QuestManager.CanScaleQuestMinDelta
/// Some quests we do not want to scale MinDelta if "quest_mindelta_rate" has been set.
/// They may be things that are races against time, like Colo
#[must_use]
pub fn can_scale_quest_min_delta(quest: &empyrean_content::models::world::Quest) -> bool {
    // `string.StartsWith(string)` is culture-sensitive (en-US); the prefix is plain ASCII.
    if quest.name.starts_with("ColoArena") {
        return false;
    }

    true
}

// ACE: QuestManager.GetNextSolveTime
/// Returns the time remaining until the player can solve this quest again: `TimeSpan.MinValue`
/// when it can be solved now, `TimeSpan.MaxValue` when never.
#[must_use]
pub fn get_next_solve_time(w: &World, owner: &QuestOwner<'_>, quest_format: &str) -> TimeSpan {
    let quest_name = get_quest_name(quest_format);

    let Some(quest) = w.content.get_cached_quest(quest_name) else {
        return TimeSpan::MAX_VALUE; // world quest not found - cannot solve it
    };

    let Some(player_quest) = get_quest(w, owner, quest_name) else {
        return TimeSpan::MIN_VALUE; // player hasn't completed this quest yet - can solve immediately
    };

    if quest.max_solves > -1 && player_quest.num_times_completed >= quest.max_solves {
        return TimeSpan::MAX_VALUE; // cannot solve this quest again - max solves reached / exceeded
    }

    let current_time = unix_time_now(w);

    let next_solve_time: u32 = if can_scale_quest_min_delta(&quest) {
        let rate =
            crate::managers::property_manager::get_double(w, "quest_mindelta_rate", 1.0, true).item;
        let scaled: u32 = (f64::from(quest.min_delta) * rate).cs_cast();
        player_quest.last_time_completed.wrapping_add(scaled)
    } else {
        player_quest
            .last_time_completed
            .wrapping_add(quest.min_delta)
    };

    if current_time >= next_solve_time {
        return TimeSpan::MIN_VALUE; // can solve again now - next solve time expired
    }

    // return the time remaining on the player's quest timer
    TimeSpan::from_seconds(f64::from(next_solve_time - current_time))
}

// ACE: QuestManager.Increment
/// Increment the number of times completed for a quest (`amount` defaults to 1).
pub fn increment(w: &mut World, owner: &mut QuestOwner<'_>, quest_name: &str, amount: i32) {
    for _ in 0..amount {
        update(w, owner, quest_name);
    }
}

// ACE: QuestManager.Decrement
/// Decrement the number of times completed for a quest (`amount` defaults to 1).
pub fn decrement(w: &mut World, owner: &mut QuestOwner<'_>, quest: &str, amount: i32) {
    let quest_name = get_quest_name(quest);
    let now = unix_time_now(w);
    let player = player_of(w, owner);

    let Some(existing) = quest_row_mut(w, owner, player, quest_name) else {
        return;
    };

    //if (existing.NumTimesCompleted == 0)
    //{
    //    if (Debug) Console.WriteLine($"{Name}.QuestManager.Decrement({quest}): can not Decrement existing quest. {questName}.NumTimesCompleted is already 0.");
    //    return;
    //}

    // update existing quest
    existing.last_time_completed = now;
    existing.num_times_completed = existing.num_times_completed.wrapping_sub(amount);
    let (registered_name, n) = (existing.quest_name.clone(), existing.num_times_completed);

    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.Decrement({quest}): updated quest ({n})",
            name(w, owner)
        );
    }

    if let Some(player) = player {
        set_character_changes_detected(w, player);
        notify_of_quest_update(w, player, &registered_name);
    }
}

// ACE: QuestManager.Erase
/// Removes an existing quest from the Player's registry
pub fn erase(w: &mut World, owner: &mut QuestOwner<'_>, quest_format: &str) {
    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.Erase({quest_format})", name(w, owner));
    }

    let quest_name = get_quest_name(quest_format);

    if let Some(player) = player_of(w, owner) {
        if character_mut(w, player).erase_quest(quest_name) {
            set_character_changes_detected(w, player);

            notify_of_quest_update(w, player, quest_name);
        }
    } else {
        // Not a player
        let r = runtime_mut(w, owner);
        let quests: Vec<u64> = r
            .rows
            .iter()
            .filter(|(_, q)| ordinal_ignore_case_eq(&q.quest_name, quest_name))
            .map(|(k, _)| *k)
            .collect();
        for quest in quests {
            r.rows.remove(&quest);
        }
    }
}

// ACE: QuestManager.EraseAll
/// Removes an all quests from registry
pub fn erase_all(w: &mut World, owner: &mut QuestOwner<'_>) {
    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.EraseAll", name(w, owner));
    }

    if let Some(player) = player_of(w, owner) {
        let quest_names_erased = character_mut(w, player).erase_all_quests();

        if !quest_names_erased.is_empty() {
            set_character_changes_detected(w, player);

            for quest_name in quest_names_erased {
                notify_of_quest_update(w, player, &quest_name);
            }
        }
    } else {
        // Not a player
        runtime_mut(w, owner).rows.clear();
    }
}

// ACE: QuestManager.ShowQuests
/// Shows the current quests in progress for a Player (on the console).
pub fn show_quests(w: &World, owner: &QuestOwner<'_>, _player: ObjectGuid) {
    empyrean_common::console_write_line!("ShowQuests");

    let quests = get_quests(w, owner);

    if quests.is_empty() {
        empyrean_common::console_write_line!("No quests in progress for {}", name(w, owner));
        return;
    }

    for quest in quests {
        empyrean_common::console_write_line!("Quest Name: {}", quest.quest_name);
        empyrean_common::console_write_line!("Times Completed: {}", quest.num_times_completed);
        empyrean_common::console_write_line!("Last Time Completed: {}", quest.last_time_completed);
        empyrean_common::console_write_line!("Player ID: {:08X}", quest.character_id);
        empyrean_common::console_write_line!("----");
    }
}

// ACE: QuestManager.Stamp
pub fn stamp(w: &mut World, owner: &mut QuestOwner<'_>, quest_format: &str) {
    let quest_name = get_quest_name(quest_format);
    update(w, owner, quest_name); // ??
}

// ACE: QuestManager.GetQuestName
/// Returns the quest name without the @ comment
#[must_use]
pub fn get_quest_name(quest_format: &str) -> &str {
    // strip comment
    match quest_format.find('@') {
        None => quest_format,
        Some(idx) => &quest_format[..idx],
    }
}

// ACE: QuestManager.HasQuestSolves
/// Returns TRUE if player has solved this quest between min-max times
#[must_use]
pub fn has_quest_solves(
    w: &World,
    owner: &QuestOwner<'_>,
    quest_format: &str,
    min: Option<i32>,
    max: Option<i32>,
) -> bool {
    let quest_name = get_quest_name(quest_format); // strip optional @comment

    let quest = get_quest(w, owner, quest_name);
    let num_solves = quest.map_or(0, |q| q.num_times_completed);

    let min_v = min.unwrap_or(i32::MIN); // use defaults?
    let max_v = max.unwrap_or(i32::MAX);

    let has_quest_solves = num_solves >= min_v && num_solves <= max_v; // verify: can either of these be -1?
    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.HasQuestSolves({quest_format}, {min:?}, {max:?}): {has_quest_solves}",
            name(w, owner)
        );
    }

    has_quest_solves
}

/// `player.Session`: a player without one is ACE's `NullReferenceException`.
fn session_of(w: &World, player: ObjectGuid) -> empyrean_net::SessionId {
    shims::player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `new GameEventInventoryServerSaveFailed(player.Session, itemGuid, errorType)`.
fn save_failed_event(
    w: &mut World,
    player: ObjectGuid,
    item_guid: u32,
    error_type: WeenieError,
) -> GameMessage {
    let session = session_of(w, player);
    let data = w
        .sessions
        .get_mut(session)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    game_event_inventory_server_save_failed(data, item_guid, error_type)
}

/// `new GameEventWeenieError(player.Session, errorType)`.
fn weenie_error_event(w: &mut World, player: ObjectGuid, error_type: WeenieError) -> GameMessage {
    let session = session_of(w, player);
    let data = w
        .sessions
        .get_mut(session)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    game_event_weenie_error(data, error_type)
}

// ACE: QuestManager.HandleNoQuestError
/// Called when a player hasn't started a quest yet
pub fn handle_no_quest_error(w: &mut World, owner: &QuestOwner<'_>, wo: ObjectGuid) {
    let Some(player) = player_of(w, owner) else {
        return;
    };

    let error = save_failed_event(
        w,
        player,
        wo.full(),
        WeenieError::ItemRequiresQuestToBePickedUp,
    );
    let session = session_of(w, player);
    enqueue_send(w, session, error);
}

// ACE: QuestManager.HandlePortalQuestError
pub fn handle_portal_quest_error(w: &mut World, owner: &QuestOwner<'_>, quest_name: &str) {
    let Some(player) = player_of(w, owner) else {
        return;
    };

    if !has_quest(w, owner, quest_name) {
        let error = weenie_error_event(w, player, WeenieError::YouMustCompleteQuestToUsePortal);
        let session = session_of(w, player);
        enqueue_send(w, session, error);
    } else if can_solve(w, owner, quest_name) {
        let error = weenie_error_event(w, player, WeenieError::QuestSolvedTooLongAgo);
        let text = game_message_system_chat(
            "You completed the quest this portal requires too long ago!",
            ChatMessageType::Magic,
        ); // This msg wasn't sent in retail PCAP, leading to a completely silent fail when using the portal with an expired flag.
        let session = session_of(w, player);
        enqueue_send_many(w, session, [text, error]);
    }
}

// ACE: QuestManager.HandleSolveError
/// Called when either the player has completed the quest too recently, or max solves has been
/// reached.
pub fn handle_solve_error(w: &mut World, owner: &QuestOwner<'_>, quest_name: &str) {
    let Some(player) = player_of(w, owner) else {
        return;
    };

    let session = session_of(w, player);
    if is_max_solves(w, owner, quest_name) {
        let error = save_failed_event(
            w,
            player,
            0,
            WeenieError::YouHaveSolvedThisQuestTooManyTimes,
        );
        let text = game_message_system_chat(
            "You have solved this quest too many times!",
            ChatMessageType::Broadcast,
        );
        enqueue_send_many(w, session, [text, error]);
    } else {
        let error = save_failed_event(w, player, 0, WeenieError::YouHaveSolvedThisQuestTooRecently);
        let text = game_message_system_chat(
            "You have solved this quest too recently!",
            ChatMessageType::Broadcast,
        );

        let remain_str = empyrean_common::extensions::time_span_extensions::get_friendly_string(
            get_next_solve_time(w, owner, quest_name),
        );
        let remain = game_message_system_chat(
            &format!("You may complete this quest again in {remain_str}."),
            ChatMessageType::Broadcast,
        );
        enqueue_send_many(w, session, [text, remain, error]);
    }
}

// ACE: QuestManager.HandleKillTask
/// Increments the counter for a kill task for a player (`killed_creature` is `None` for ACE's
/// null input).
pub fn handle_kill_task(
    w: &mut World,
    owner: &mut QuestOwner<'_>,
    kill_quest_name: &str,
    killed_creature: Option<ObjectGuid>,
) {
    let Some(player) = player_of(w, owner) else {
        return;
    };

    // http://acpedia.org/wiki/Announcements_-_2012/12_-_A_Growing_Twilight#Release_Notes

    let Some(killed_creature) = killed_creature else {
        log::error!(
            "{}.QuestManager.HandleKillTask({kill_quest_name}): input object is null!",
            name(w, owner)
        );
        return;
    };

    let quest_name = get_quest_name(kill_quest_name);
    let Some(quest) = w.content.get_cached_quest(quest_name) else {
        log::error!("{}.QuestManager.HandleKillTask({kill_quest_name}): couldn't find kill task {quest_name} in database", name(w, owner));
        return;
    };

    if !has_quest(w, owner, quest_name) {
        return;
    }

    stamp(w, owner, kill_quest_name);

    let Some(player_quest) = get_quest(w, owner, quest_name) else {
        // this should be impossible
        log::error!("{}.QuestManager.HandleKillTask({kill_quest_name}): couldn't find kill task {quest_name} in player quests", name(w, owner));
        return;
    };

    let mut msg = format!(
        "You have killed {} {}!",
        player_quest.num_times_completed,
        crate::world_objects::world_object::get_plural_name(w, killed_creature)
    );

    if is_max_solves(w, owner, quest_name) {
        msg += " Your task is complete!";
    } else {
        msg += &format!(" You must kill {} to complete your task.", quest.max_solves);
    }

    let session = session_of(w, player);
    enqueue_send(
        w,
        session,
        game_message_system_chat(&msg, ChatMessageType::Broadcast),
    );
}

// ACE: QuestManager.OnDeath
/// Called when a player kills Creature
pub fn on_death(_w: &mut World, _owner: &mut QuestOwner<'_>, _killer: Option<ObjectGuid>) {}

// ACE: QuestManager.HasQuestBits
#[must_use]
pub fn has_quest_bits(w: &World, owner: &QuestOwner<'_>, quest_format: &str, bits: i32) -> bool {
    let quest_name = get_quest_name(quest_format);

    let Some(quest) = get_quest(w, owner, quest_name) else {
        return false;
    };

    let has_quest_bits = (quest.num_times_completed & bits) == bits;

    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.HasQuestBits({quest_format}, 0x{bits:X}): {has_quest_bits}",
            name(w, owner)
        );
    }

    has_quest_bits
}

// ACE: QuestManager.HasNoQuestBits
#[must_use]
pub fn has_no_quest_bits(w: &World, owner: &QuestOwner<'_>, quest_format: &str, bits: i32) -> bool {
    let quest_name = get_quest_name(quest_format);

    let Some(quest) = get_quest(w, owner, quest_name) else {
        return true;
    };

    let has_no_quest_bits = (quest.num_times_completed & bits) == 0;

    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.HasNoQuestBits({quest_format}, 0x{bits:X}): {has_no_quest_bits}",
            name(w, owner)
        );
    }

    has_no_quest_bits
}

// ACE: QuestManager.SetQuestBits
/// `on` defaults to true in ACE.
pub fn set_quest_bits(
    w: &mut World,
    owner: &mut QuestOwner<'_>,
    quest_format: &str,
    bits: i32,
    on: bool,
) {
    let quest_name = get_quest_name(quest_format);

    let quest = get_quest(w, owner, quest_name);

    let mut quest_bits = 0;

    if let Some(quest) = quest {
        quest_bits = quest.num_times_completed;
    }

    if on {
        quest_bits |= bits;
    } else {
        quest_bits &= !bits;
    }

    if DEBUG {
        empyrean_common::console_write_line!(debug: "{}.QuestManager.SetQuestBits({quest_format}, 0x{bits:X}): {on}",
            name(w, owner)
        );
    }

    set_quest_completions(w, owner, quest_format, quest_bits);
}
