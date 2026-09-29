// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesFriendList.cs
//! `CharacterPropertiesFriendList`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesFriendList
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesFriendList {
    // ACE: CharacterPropertiesFriendList.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesFriendList.FriendId
    pub friend_id: u32,
}
