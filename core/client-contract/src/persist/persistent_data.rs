//! Persistent UI data that crosses a screen change.
//!
//! Because the whole main framework — and with it every element and every dialog, since framework
//! destruction resets the dialog factory — is destroyed on a mode switch, this is the
//! **only** thing that survives one. Keep that boundary sharp: it is what makes the
//! client's screen transitions leak-free.

use dereth_primitives::ObjectId;

/// One character in the account's list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterIdentity {
    pub id: ObjectId,
    pub name: String,
    /// Seconds until the pending deletion completes, 0 when not pending.
    pub seconds_grace_period: u32,
}

/// `CharacterSet` — a packable object the login reply fills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterSet {
    /// The live characters.
    pub set: Vec<CharacterIdentity>,
    /// Characters pending deletion.
    pub del_set: Vec<CharacterIdentity>,
    /// Account status flags.
    pub status: u32,
    /// The account's character-slot count, **initialised to 5** and overwritten by the server.
    pub num_allowed_characters: u32,
    /// The account name.
    pub account: String,
    pub is_dark_majesty: bool,
    pub is_throne_of_destiny: bool,
    pub pre_ordered_throne_of_destiny: bool,
}

impl Default for CharacterSet {
    fn default() -> Self {
        Self {
            set: Vec::new(),
            del_set: Vec::new(),
            status: 0,
            // The documented initialiser. The server overwrites it, but a client that has not
            // heard from the server yet shows five slots.
            num_allowed_characters: 5,
            account: String::new(),
            is_dark_majesty: false,
            is_throne_of_destiny: false,
            pre_ordered_throne_of_destiny: false,
        }
    }
}

/// The 80-byte notice handler owned by the UI flow.
#[derive(Debug, Clone, Default)]
pub struct UiPersistentData {
    /// The account's character set.
    pub char_set: CharacterSet,
    /// Whether a character set has arrived — false until the first `Login_LoginCharacterSet`
    /// (0xF658) arrives.
    pub received_set: bool,
    /// The selected avatar id.
    pub selected_avatar: ObjectId,
}

impl UiPersistentData {
    /// Copy an incoming character set into persistent state and mark it received.
    /// Return `true` to request a UI-flow notification.
    ///
    /// The caller performs that notification because the UI mode flow owns this object;
    /// this method only updates the stored character set and receipt state.
    pub fn on_character_set(&mut self, incoming: CharacterSet) -> bool {
        self.char_set = incoming;
        self.received_set = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the allowed-character count starts at five, and the character-set-received flag
    /// remains false until the first character-set login message (0xF658) arrives.
    #[test]
    fn the_character_set_starts_with_five_slots_and_no_data() {
        let d = UiPersistentData::default();
        assert_eq!(d.char_set.num_allowed_characters, 5);
        assert!(!d.received_set);
        assert!(d.char_set.set.is_empty());
    }

    /// Oracle: the three-line body.
    #[test]
    fn receiving_the_character_set_latches_the_flag_and_pokes_the_flow() {
        let mut d = UiPersistentData::default();
        let incoming = CharacterSet {
            set: vec![CharacterIdentity {
                id: ObjectId(0x5000_0001),
                name: "Kupo".into(),
                seconds_grace_period: 0,
            }],
            num_allowed_characters: 11,
            ..CharacterSet::default()
        };
        assert!(d.on_character_set(incoming));
        assert!(d.received_set);
        assert_eq!(d.char_set.set.len(), 1);
        assert_eq!(
            d.char_set.num_allowed_characters, 11,
            "the server overwrites the default"
        );
    }
}
