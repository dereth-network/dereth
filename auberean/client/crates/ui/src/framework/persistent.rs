//! What lives across screens: the account's characters as the server last listed them, the
//! avatars made during character creation (by race and sex), which one is being made, and the
//! sound playing across the screens before the world.
//!
//! The types are the game's: `C` the character list, `R` an avatar being made.

/// Data kept from screen to screen.
#[derive(Clone, Debug)]
pub struct Persistent<C, R> {
    /// The account's characters, once the server listed them.
    pub characters: Option<C>,
    /// The avatars made so far, by their key (race and sex), in the order first made.
    results: Vec<(u32, R)>,
    /// Character creation is under way, and the avatar chosen.
    pub in_creation: bool,
    pub current: u32,
    /// The sound playing across screens (0 for none).
    pub playing_sound: u32,
}

impl<C, R> Default for Persistent<C, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C, R> Persistent<C, R> {
    /// Nothing kept yet.
    // D2-11874
    #[must_use]
    pub const fn new() -> Self {
        Self {
            characters: None,
            results: Vec::new(),
            in_creation: false,
            current: 0,
            playing_sound: 0,
        }
    }

    /// The server listed the account's characters: kept. Returns true, so the caller tells the
    /// screen up.
    // D2-11860
    pub fn set_characters(&mut self, c: C) -> bool {
        self.characters = Some(c);
        true
    }

    /// Keep an avatar being made under its key (replacing one there).
    // D2-11869
    pub fn set_result(&mut self, key: u32, r: R) {
        match self.results.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = r,
            None => self.results.push((key, r)),
        }
    }

    /// The avatar kept under a key.
    // D2-11870
    #[must_use]
    pub fn result(&self, key: u32) -> Option<&R> {
        self.results.iter().find(|(k, _)| *k == key).map(|(_, r)| r)
    }

    /// The avatar kept under a key, to change.
    pub fn result_mut(&mut self, key: u32) -> Option<&mut R> {
        self.results
            .iter_mut()
            .find(|(k, _)| *k == key)
            .map(|(_, r)| r)
    }

    /// The avatar chosen.
    // D2-11861
    #[must_use]
    pub fn current_result(&self) -> Option<&R> {
        self.result(self.current)
    }

    /// Every avatar kept, copied.
    // D2-11871
    #[must_use]
    pub fn results(&self) -> Vec<(u32, R)>
    where
        R: Clone,
    {
        self.results.clone()
    }

    /// Throw the avatars away: creation is over.
    // D2-11863, D2-11862, D2-11864
    pub fn clear_results(&mut self) {
        self.results.clear();
        self.in_creation = false;
        self.current = 0;
    }

    /// Keep the sound playing across screens.
    // D2-11867, D2-11868
    pub fn set_playing_sound(&mut self, handle: u32) {
        self.playing_sound = handle;
    }

    /// Everything goes: the avatars thrown away, the sound stopped (its handle cleared).
    // D2-11865
    pub fn close(&mut self) {
        self.clear_results();
        self.playing_sound = 0;
    }
}
