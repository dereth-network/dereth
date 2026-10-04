//! A character's shared notebook. The host performs the requested file operations.

use dereth_client_contract::journal::{
    JournalAction, JournalField, JournalIdentity, JournalIo, JournalPage, JournalView,
};

#[derive(Debug, Clone, Default)]
pub struct JournalStore {
    state: JournalView,
    identity: Option<JournalIdentity>,
    io: Vec<JournalIo>,
    dirty: bool,
    load_pending: bool,
}

impl JournalStore {
    #[must_use]
    pub fn view(&self) -> JournalView {
        self.state.clone()
    }

    pub fn take_io(&mut self) -> Vec<JournalIo> {
        std::mem::take(&mut self.io)
    }

    /// Count successful host operations, including a save tagged to the previous character.
    pub fn record_io(
        &mut self,
        _identity: &JournalIdentity,
        _generation: u64,
        read: bool,
        success: bool,
    ) {
        if !success {
            return;
        }
        if read {
            self.state.page_loads = self.state.page_loads.wrapping_add(1);
        } else {
            self.state.page_saves = self.state.page_saves.wrapping_add(1);
        }
    }

    fn changed(&mut self) {
        self.state.revision = self.state.revision.wrapping_add(1);
    }

    fn blank_page(number: u32) -> JournalPage {
        JournalPage {
            page_number: number,
            ..Default::default()
        }
    }

    fn initialize(&mut self) {
        if self.state.pages.is_empty() {
            self.state.pages.push(Self::blank_page(1));
        }
        self.state.loaded = true;
        self.state.current_page = 1;
        self.state.draft = self.state.pages[0].clone();
        self.changed();
    }

    /// A late identity can name an untouched session; changing an established identity clears it.
    pub fn set_identity(&mut self, identity: Option<JournalIdentity>) {
        if !self.state.loaded {
            self.initialize();
        }
        if identity == self.identity {
            return;
        }
        if self.identity.is_some() {
            self.apply(JournalAction::Visibility(false), 0.0, None);
            let generation = self.state.generation.wrapping_add(1);
            let revision = self.state.revision.wrapping_add(1);
            self.state = JournalView {
                generation,
                revision,
                page_loads: self.state.page_loads,
                page_saves: self.state.page_saves,
                ..Default::default()
            };
            self.dirty = false;
            self.initialize();
        } else {
            self.state.generation = self.state.generation.wrapping_add(1);
        }
        self.identity = identity;
        self.load_pending = false;
        if let Some(identity) = &self.identity {
            if !self.dirty {
                self.io.push(JournalIo::Load {
                    identity: identity.clone(),
                    generation: self.state.generation,
                    revision: self.state.revision,
                });
                self.load_pending = true;
            }
        }
    }

    /// Returns whether a matching malformed file should produce the load complaint.
    pub fn complete_load(
        &mut self,
        identity: JournalIdentity,
        generation: u64,
        revision: u64,
        pages: Result<Vec<JournalPage>, ()>,
    ) -> bool {
        if self.identity.as_ref() != Some(&identity)
            || self.state.generation != generation
            || !self.load_pending
        {
            return false;
        }
        self.load_pending = false;
        if self.state.revision != revision || self.dirty {
            return false;
        }
        let complaint = pages.is_err();
        self.state.pages = pages.unwrap_or_default();
        self.initialize();
        complaint
    }

    fn capture(&mut self) {
        if let Some(page) = self
            .state
            .current_page
            .checked_sub(1)
            .and_then(|n| self.state.pages.get_mut(n as usize))
        {
            *page = self.state.draft.clone();
        }
    }

    fn goto(&mut self, number: u32) {
        let Some(page) = number
            .checked_sub(1)
            .and_then(|n| self.state.pages.get(n as usize))
        else {
            return;
        };
        self.state.current_page = number;
        self.state.draft = page.clone();
        self.changed();
    }

    /// File saves happen at visibility boundaries, never on each edit or timer redraw.
    pub fn apply(&mut self, action: JournalAction, now: f64, coords: Option<(f32, f32)>) {
        if !self.state.loaded {
            return;
        }
        match action {
            JournalAction::Edit {
                generation,
                page,
                mut draft,
            } => {
                if generation != self.state.generation || page != self.state.current_page {
                    return;
                }
                draft.page_number = page;
                draft.timer_running = self.state.draft.timer_running;
                draft.timer_stamp = self.state.draft.timer_stamp;
                draft.location_set = self.state.draft.location_set;
                draft.ns = self.state.draft.ns;
                draft.ew = self.state.draft.ew;
                if draft == self.state.draft {
                    return;
                }
                self.state.draft = draft;
                self.dirty = true;
                self.capture();
                self.changed();
            }
            JournalAction::SetField {
                generation,
                page,
                field,
            } => {
                if generation != self.state.generation || page != self.state.current_page {
                    return;
                }
                match field {
                    JournalField::Label(s) => self.state.draft.label = s,
                    JournalField::Title(s) => self.state.draft.title = s,
                    JournalField::Notes(s) => self.state.draft.notes = s,
                    JournalField::Days(n) => self.state.draft.days = n,
                    JournalField::Hours(n) => self.state.draft.hours = n,
                    JournalField::Minutes(n) => self.state.draft.minutes = n,
                }
                self.dirty = true;
                self.capture();
                self.changed();
            }
            JournalAction::Turn(delta) => {
                self.capture();
                self.goto(self.state.current_page.saturating_add_signed(delta));
            }
            JournalAction::Goto(page) => {
                self.capture();
                self.goto(page);
            }
            JournalAction::NewPage => {
                self.capture();
                let Some(number) = u32::try_from(self.state.pages.len())
                    .ok()
                    .and_then(|n| n.checked_add(1))
                else {
                    return;
                };
                self.state.pages.push(Self::blank_page(number));
                self.dirty = true;
                self.goto(number);
            }
            JournalAction::Delete(page) => {
                if page == 0 || page as usize > self.state.pages.len() {
                    return;
                }
                self.capture();
                if self.state.pages.len() == 1 {
                    self.state.pages[0] = Self::blank_page(1);
                } else {
                    self.state.pages.remove(page as usize - 1);
                    for (i, p) in self.state.pages.iter_mut().enumerate() {
                        p.page_number = u32::try_from(i + 1).unwrap_or(u32::MAX);
                    }
                }
                self.dirty = true;
                self.goto(1);
            }
            JournalAction::StampLocation => {
                let (ns, ew) = coords.unwrap_or_default();
                self.state.draft.ns = f64::from(ns);
                self.state.draft.ew = f64::from(ew);
                self.state.draft.location_set = coords.is_some();
                self.dirty = true;
                self.capture();
                self.changed();
            }
            JournalAction::ToggleTimer => {
                let p = &mut self.state.draft;
                p.timer_running = !p.timer_running;
                p.timer_stamp = if p.timer_running {
                    now + f64::from(p.days) * 86_400.0
                        + f64::from(p.hours) * 3_600.0
                        + f64::from(p.minutes) * 60.0
                } else {
                    0.0
                };
                self.dirty = true;
                self.capture();
                self.changed();
            }
            JournalAction::Visibility(visible) => {
                self.capture();
                if visible {
                    self.state.pages.sort_by_key(|p| p.page_number);
                }
                if !self.load_pending {
                    if let Some(identity) = &self.identity {
                        self.io.push(JournalIo::Save {
                            identity: identity.clone(),
                            generation: self.state.generation,
                            pages: self.state.pages.clone(),
                        });
                    }
                }
            }
            JournalAction::Sort(by) => {
                if by > 3 {
                    return;
                }
                if self.state.sort == by {
                    self.state.reverse = !self.state.reverse;
                } else {
                    self.state.sort = by;
                    self.state.reverse = false;
                }
                self.changed();
            }
            JournalAction::Search(needle) => {
                self.state.search = needle;
                self.state.filtered = true;
                self.changed();
            }
            JournalAction::ResetSearch => {
                self.state.search.clear();
                self.state.filtered = false;
                self.changed();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(character: &str) -> JournalIdentity {
        JournalIdentity {
            directory: "settings".into(),
            world: "World".into(),
            character: character.into(),
        }
    }

    fn edit(store: &mut JournalStore, title: &str) {
        let view = store.view();
        let mut draft = view.draft;
        draft.title = title.into();
        draft.minutes = 2;
        store.apply(
            JournalAction::Edit {
                generation: view.generation,
                page: view.current_page,
                draft,
            },
            0.0,
            None,
        );
    }

    /// Behaviour: journal.shared-session-identity-and-save-boundaries
    #[test]
    fn identity_reads_and_edits_keep_their_pages_and_save_boundaries() {
        let mut store = JournalStore::default();
        store.set_identity(None);
        edit(&mut store, "Anonymous draft");
        store.apply(JournalAction::Visibility(false), 0.0, None);
        assert!(store.take_io().is_empty());
        store.set_identity(Some(identity("First")));
        assert!(
            store.take_io().is_empty(),
            "a late identity does not load over an edited anonymous draft"
        );
        store.apply(JournalAction::Visibility(false), 0.0, None);
        let saves = store.take_io();
        assert!(
            matches!(saves.as_slice(), [JournalIo::Save { identity: id, pages, .. }] if id.character == "First" && pages[0].title == "Anonymous draft")
        );

        store.set_identity(Some(identity("Second")));
        let io = store.take_io();
        assert!(
            matches!(io.first(), Some(JournalIo::Save { identity: id, pages, .. }) if id.character == "First" && pages[0].title == "Anonymous draft")
        );
        let Some(JournalIo::Load {
            identity: id,
            generation,
            revision,
        }) = io.last().cloned()
        else {
            panic!("second character load")
        };
        edit(&mut store, "New draft before read");
        assert!(!store.complete_load(
            id.clone(),
            generation,
            revision,
            Ok(vec![JournalPage {
                title: "Old file".into(),
                ..Default::default()
            }])
        ));
        assert_eq!(store.view().draft.title, "New draft before read");
        assert!(
            store.take_io().is_empty(),
            "edits never request disk writes"
        );
        store.apply(JournalAction::ToggleTimer, 100.0, None);
        store.apply(JournalAction::StampLocation, 100.0, Some((12.5, -33.25)));
        let view = store.view();
        assert_eq!(view.draft.timer_stamp, 220.0);
        assert!(view.draft.timer_running && view.draft.location_set);
        let generation = view.generation;
        store.apply(
            JournalAction::SetField {
                generation,
                page: 1,
                field: JournalField::Label("Quest".into()),
            },
            100.0,
            None,
        );
        store.apply(
            JournalAction::SetField {
                generation,
                page: 1,
                field: JournalField::Notes("Two queued field edits".into()),
            },
            100.0,
            None,
        );
        assert_eq!(store.view().draft.label, "Quest");
        assert_eq!(store.view().draft.notes, "Two queued field edits");
        let mut stale_controls = store.view().draft;
        stale_controls.timer_running = false;
        stale_controls.timer_stamp = 0.0;
        stale_controls.location_set = false;
        stale_controls.ns = 0.0;
        stale_controls.ew = 0.0;
        store.apply(
            JournalAction::Edit {
                generation,
                page: 1,
                draft: stale_controls,
            },
            100.0,
            None,
        );
        assert_eq!(store.view().draft.timer_stamp, 220.0);
        assert_eq!(
            (store.view().draft.ns, store.view().draft.ew),
            (12.5, -33.25)
        );
        assert!(store.view().draft.timer_running && store.view().draft.location_set);
        store.apply(JournalAction::NewPage, 100.0, None);
        edit(&mut store, "Second page");
        store.apply(JournalAction::Goto(1), 100.0, None);
        assert_eq!(store.view().draft.title, "New draft before read");
        assert_eq!(store.view().draft.timer_stamp, 220.0);
        assert!(
            store.take_io().is_empty(),
            "page and timer actions capture shared content, not files"
        );
        store.apply(JournalAction::Visibility(false), 100.0, None);
        assert_eq!(store.take_io().len(), 1);
        let unchanged = store.view();
        store.set_identity(Some(identity("Second")));
        assert_eq!(
            store.view(),
            unchanged,
            "an interface change keeps page, draft and timer"
        );
        assert!(store.take_io().is_empty());
        store.apply(JournalAction::Delete(1), 100.0, None);
        assert_eq!(store.view().draft.title, "Second page");
        assert_eq!(store.view().pages[0].page_number, 1);
        store.set_identity(Some(identity("Third")));
        assert_eq!(store.view().draft.title, "");
        store.record_io(&id, generation, false, true);
        store.record_io(&id, generation, true, false);
        assert_eq!((store.view().page_loads, store.view().page_saves), (0, 1));
        assert!(
            !store.complete_load(id, generation, revision, Err(())),
            "a stale malformed load raises no complaint"
        );
        let stale = unchanged;
        store.apply(
            JournalAction::Edit {
                generation: stale.generation,
                page: 1,
                draft: stale.draft,
            },
            0.0,
            None,
        );
        assert_eq!(store.view().draft.title, "");
    }
}
