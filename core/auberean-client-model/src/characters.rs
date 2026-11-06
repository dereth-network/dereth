//! The account's characters as the character list reads them: by slot (a character's place in the
//! list), each slot empty or holding a character.
//!
//! A slot past the end, or one whose character has no id, holds nothing: it has no identity, no
//! name, the invalid id (0) and no time greyed out. A character's slot is its place in the list, -1
//! for one not in it. How long a slot stays greyed out reads 0 as well for a slot past the number
//! of characters the account may have (when it may have any).

use auberean_protocol::login::CharacterList;
use auberean_protocol::text::WideString;
use auberean_protocol::types::CharacterIdentity;

/// Whether an identity names no character: its id is the invalid one.
// D2-01793
#[must_use]
pub fn invalid(identity: &CharacterIdentity) -> bool {
    identity.id == 0
}

/// Grey a character out for so many seconds.
// D2-15915
pub fn set_greyed_out_for_seconds(identity: &mut CharacterIdentity, seconds: u32) {
    identity.seconds_greyed_out = seconds;
}

/// The slot of the character with an id, or -1.
// D2-13096
#[must_use]
pub fn which_slot(list: &CharacterList, id: u64) -> i32 {
    list.characters
        .iter()
        .position(|c| c.id == id)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

/// Whether the list holds the character with an id.
// D2-13104
#[must_use]
pub fn includes(list: &CharacterList, id: u64) -> bool {
    which_slot(list, id) != -1
}

/// The character in a slot, if it holds one.
#[must_use]
pub fn identity(list: &CharacterList, slot: u32) -> Option<&CharacterIdentity> {
    list.characters
        .get(usize::try_from(slot).ok()?)
        .filter(|c| !invalid(c))
}

/// The name of the character in a slot.
#[must_use]
pub fn name(list: &CharacterList, slot: u32) -> Option<&WideString> {
    identity(list, slot).map(|c| &c.name)
}

/// The id of the character in a slot; the invalid id (0) for none.
#[must_use]
pub fn id(list: &CharacterList, slot: u32) -> u64 {
    identity(list, slot).map_or(0, |c| c.id)
}

/// Grey the character in a slot out for so many seconds; nothing for an empty slot.
// D2-13100
pub fn set_greyed_out_for(list: &mut CharacterList, slot: u32, seconds: u32) {
    let Ok(i) = usize::try_from(slot) else {
        return;
    };
    if let Some(c) = list.characters.get_mut(i).filter(|c| !invalid(c)) {
        set_greyed_out_for_seconds(c, seconds);
    }
}

/// How long the character in a slot stays greyed out: 0 for an empty slot, a negative one, or one
/// past the number of characters the account may have (when it may have any).
#[must_use]
pub fn greyed_out_for(list: &CharacterList, slot: i32) -> u32 {
    let Ok(s) = u32::try_from(slot) else {
        return 0;
    };
    let allowed = i32::try_from(list.slots).unwrap_or(i32::MAX);
    if !(slot < allowed || allowed < 1) {
        return 0;
    }
    identity(list, s).map_or(0, |c| c.seconds_greyed_out)
}
