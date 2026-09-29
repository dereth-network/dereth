// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/SquelchDB.cs
//! Port of `Source/ACE.Server/Network/Structure/SquelchDB.cs`.

use dereth_protocol::archive::PackedHash;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{ChatMessageType, SquelchMask};
use empyrean_entity::ObjectGuid;
use empyrean_store::models::shard::CharacterPropertiesSquelch;

use super::hash_comparer::{sorted, HashComparer};
use super::squelch_info::{self, SquelchInfo};
use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: SquelchDB
#[derive(Debug, Clone, Default)]
pub struct SquelchDB {
    // ACE: SquelchDB.Accounts
    /// Account squelches.
    ///
    /// This is defined by the network protocol, but appears to have always been empty in retail
    /// pcaps (possibly for security reasons?). When sending the SquelchDB to the player, the
    /// account squelches were sent in the Characters table below as SquelchInfo.Account=true.
    pub accounts: DotNetDict<String, u32>,

    // ACE: SquelchDB.Characters
    /// Character squelches. Even though this is called Characters, it contains both the Character
    /// and Account squelches (denoted by SquelchInfo.Account).
    pub characters: DotNetDict<u32, SquelchInfo>,

    // ACE: SquelchDB.Globals
    /// Global squelches.
    pub globals: SquelchInfo,

    /// `PlayerManager.FindByGuid(guid)?.Name` for each account squelch's guid, as
    /// `CharactersPlus` reads it. ACE looks the players up again when the table is written; the
    /// table is built and written in one synchronous step, so they are looked up once, here
    /// (DIVERGE arch). `None`: the player was not found and the entry is skipped.
    pub account_player_names: DotNetDict<u32, Option<String>>,
}

// ACE: SquelchDB.SquelchDB
/// Constructs a new SquelchDB for network sending, from the character's squelch rows and its
/// global mask.
pub fn squelch_db_new(
    w: &World,
    squelches: &[CharacterPropertiesSquelch],
    globals: SquelchMask,
) -> SquelchDB {
    let mut db = SquelchDB {
        accounts: DotNetDict::new(),
        characters: DotNetDict::new(),
        globals: SquelchInfo::new(),
        account_player_names: DotNetDict::new(),
    };

    for squelch in squelches {
        let mut squelch_player =
            shims::player_manager_find_by_guid(w, squelch.squelch_character_id);
        if squelch_player.is_none() && squelch.squelch_account_id == 0 {
            log::warn!(
                "BuildSquelchDB(): couldn't find character 0x{:08X}",
                squelch.squelch_character_id
            );
            continue;
        }

        if squelch.squelch_account_id == 0 {
            // chracter squelch
            let squelch_player = squelch_player.expect("checked above");
            let squelch_info =
                SquelchInfo::from_filter(SquelchMask(squelch.r#type), &squelch_player.name, false);

            // `Dictionary.Add`: a duplicate key throws in ACE.
            db.characters
                .add(squelch.squelch_character_id, squelch_info);
        } else {
            // account squelch
            if squelch_player.is_none() {
                let squelched_account_players =
                    shims::player_manager_get_account_players(w, squelch.squelch_account_id);

                // `OrderByDescending(LoginTimestamp ?? 0).FirstOrDefault()`: the first of the
                // latest (a stable sort).
                let most_recent = squelched_account_players.and_then(|players| {
                    let mut players: Vec<_> = players.into_iter().collect();
                    // `double` keys through the default comparer.
                    players.sort_by(|a, b| {
                        let ta = a.1.login_timestamp.unwrap_or(0.0);
                        let tb = b.1.login_timestamp.unwrap_or(0.0);
                        tb.partial_cmp(&ta).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    players.into_iter().next()
                });

                let Some(most_recent) = most_recent else {
                    log::warn!(
                        "BuildSquelchDB(): couldn't find character 0x{:08X} and account {} has no other characters",
                        squelch.squelch_character_id,
                        squelch.squelch_account_id
                    );
                    continue;
                };

                squelch_player = Some(most_recent.1);
            }

            let squelch_player = squelch_player.expect("set above");
            let account_name = squelch_player
                .account_name
                .clone()
                .expect("ACE: squelchPlayer.Account is null");
            db.accounts.add(account_name, squelch_player.guid.full());
        }
    }

    // global squelch
    if globals != SquelchMask::None {
        db.globals.filters.push(globals);
    }

    // `CharactersPlus`'s lookups (see `account_player_names`).
    let guids: Vec<u32> = db.accounts.values().copied().collect();
    for guid in guids {
        let name = shims::player_manager_find_by_guid(w, guid).map(|p| p.name);
        db.account_player_names.insert(guid, name);
    }
    db
}

impl SquelchDB {
    // ACE: SquelchDB.CharactersPlus
    /// Merges accounts + character for sending to clients.
    #[must_use]
    pub fn characters_plus(&self) -> DotNetDict<u32, SquelchInfo> {
        let mut characters_plus = self.characters.clone();

        for (_, guid) in self.accounts.iter() {
            let guid = *guid;
            let Some(Some(account_player_name)) = self.account_player_names.get(&guid) else {
                continue;
            };

            if let Some(existing) = characters_plus.get_mut(&guid) {
                existing.account = true;
            } else {
                characters_plus.add(
                    guid,
                    SquelchInfo::from_filter(SquelchMask::AllChannels, account_player_name, true),
                );
            }
        }
        characters_plus
    }

    // ACE: SquelchDB.Contains
    /// Returns TRUE if the input player is filtered by this player. `message_type` defaults to
    /// `ChatMessageType.AllChannels` in ACE.
    #[must_use]
    pub fn contains(&self, w: &World, source: ObjectGuid, message_type: ChatMessageType) -> bool {
        // ensure this channel can be squelched
        if message_type != ChatMessageType::AllChannels
            && !shims::squelch_manager_is_legal_channel(message_type)
        {
            return false;
        }

        let squelch_mask = message_type.to_mask();

        if w.objects
            .get(source)
            .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
        {
            // check account squelches

            // the client forces account squelches to be AllMessages,
            // so for these, channel mask is not required

            let account =
                shims::player_session_account(w, source).expect("ACE: player.Session is null");
            if self.accounts.contains_key(&account) {
                return true;
            }

            // check character squelches
            let squelch_info = self.characters.get(&source.full());

            if let Some(squelch_info) = squelch_info {
                if squelch_info.filters[0].contains(squelch_mask) {
                    return true;
                }
            }
        }

        // check global squelches
        if !self.globals.filters.is_empty() && self.globals.filters[0].contains(squelch_mask) {
            return true;
        }

        false
    }
}

/// retail used either 32 or 128 here, but i could find no fully consistent pattern to discern
/// them; seems to be 128 in client constructor?
// ACE: SquelchDBExtensions.HashComparer
pub const HASH_COMPARER: HashComparer = HashComparer::new(32);

// ACE: SquelchDBExtensions.Write
/// `writer.Write(SquelchDB squelches)`. The account table is always written empty: it is empty in
/// retail pcaps, even with account squelches (perhaps for security reasons, the account squelches
/// are sent in the characters section with the account bool).
pub fn write(writer: &mut Vec<u8>, squelches: &SquelchDB) {
    //writer.Write(squelches.Accounts);
    let characters = squelches.characters_plus();
    let names: Vec<&str> = characters
        .iter()
        .map(|(_, v)| v.player_name.as_deref().unwrap_or(""))
        .collect();
    let mut strings = names;
    strings.push(squelches.globals.player_name.as_deref().unwrap_or(""));
    let record = dereth_protocol::comms::SquelchDb {
        account_hash: account_hash_record(&DotNetDict::new()),
        character_hash: character_hash_record(&characters),
        global_squelch_info: squelch_info::record(&squelches.globals),
    };
    write_record(writer, &strings, |w| record.write(w));
}

// ACE: SquelchDBExtensions.Write
/// `writer.Write(Dictionary<string, uint> accountHash)`: unused in retail, so always an empty
/// header with 0 buckets.
pub fn write_account_hash(writer: &mut Vec<u8>, _account_hash: &DotNetDict<String, u32>) {
    write_record(writer, &[], |w| {
        w.packed_hash(&account_hash_record(_account_hash), |w, k, v| {
            w.pstring(k)?;
            w.u32(*v);
            Ok(())
        })
    });
}

// ACE: SquelchDBExtensions.Write
/// `writer.Write(Dictionary<uint, SquelchInfo> characterHash)`, in bucket order.
pub fn write_character_hash(writer: &mut Vec<u8>, character_hash: &DotNetDict<u32, SquelchInfo>) {
    write_record(writer, &[], |w| {
        w.packed_hash(&character_hash_record(character_hash), |w, k, v| {
            w.u32(*k);
            v.write(w)
        })
    });
}

/// The account table as dereth-protocol's record: unused in retail, so always an empty header with 0
/// buckets.
#[must_use]
pub fn account_hash_record(_account_hash: &DotNetDict<String, u32>) -> PackedHash<String, u32> {
    // unused in retail
    PackedHash {
        table_size: 0,
        entries: Vec::new(),
    }

    /*PHashTable.WriteHeader(writer, accountHash.Count);    // verify

    foreach (var kvp in accountHash)
    {
        writer.WriteString16L(kvp.Key);
        writer.Write(kvp.Value);
    }*/
}

/// The character table as dereth-protocol's record: [`HASH_COMPARER`]'s bucket count and bucket order.
#[must_use]
pub fn character_hash_record(
    character_hash: &DotNetDict<u32, SquelchInfo>,
) -> PackedHash<u32, dereth_protocol::comms::SquelchInfo> {
    let sorted = sorted(
        character_hash.iter().map(|(k, v)| (*k, v.clone())),
        &HASH_COMPARER,
    );
    PackedHash {
        table_size: u32::from(HASH_COMPARER.num_buckets),
        entries: sorted
            .into_iter()
            .map(|(key, value)| (key, squelch_info::record(&value)))
            .collect(),
    }
}
