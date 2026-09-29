// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesQuestRegistry.cs
//! `CharacterPropertiesQuestRegistry`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesQuestRegistry
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesQuestRegistry {
    // ACE: CharacterPropertiesQuestRegistry.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesQuestRegistry.QuestName
    pub quest_name: String,
    // ACE: CharacterPropertiesQuestRegistry.LastTimeCompleted
    pub last_time_completed: u32,
    // ACE: CharacterPropertiesQuestRegistry.NumTimesCompleted
    pub num_times_completed: i32,
}
