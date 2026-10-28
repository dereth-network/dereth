//! An edit session's core: the text of every source file it has read or changed, against the
//! store's, the history of its changes as text, and the save.
//!
//! An editor turns each change into new text for some of a pack's files ([`Texts`]), and keeps
//! the files' text here ([`Sources`]). One change is one [`Step`] of the [`History`]: each file's
//! text before and after. Undo puts the text before back, redo the text after; what the text
//! means (what to compile again, what to draw) is the editor's, between taking a step off the
//! history and putting it on the other side. Nothing reaches the store until [`Sources::save`],
//! which writes only the files whose text differs from what the store held, and removes a file
//! the session made and took away again.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::store::SourceStore;

/// Files' texts by path; `None` for no file.
pub type Texts = BTreeMap<PathBuf, Option<String>>;

/// Source files as a session holds them: each one's text now, and as the store last had it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources {
    /// Every file the session has read or changed: its text now.
    texts: Texts,
    /// Those files' text in the store, as last read or saved.
    stored: Texts,
}

impl Sources {
    /// The text of `path` as the session holds it, read from `store` the first time.
    pub fn text(&mut self, store: &dyn SourceStore, path: &Path) -> Option<String> {
        if let Some(t) = self.texts.get(path) {
            return t.clone();
        }
        let t = store.read_to_string(path).ok();
        self.texts.insert(path.to_path_buf(), t.clone());
        self.stored.insert(path.to_path_buf(), t.clone());
        t
    }

    /// The text of `path` as the store last had it (read or saved): `None` for a file it did not
    /// have.
    pub fn stored(&mut self, store: &dyn SourceStore, path: &Path) -> Option<String> {
        if let Some(d) = self.stored.get(path) {
            return d.clone();
        }
        self.text(store, path)
    }

    /// Put `text` in place of `path`'s, remembering what the store had first.
    pub fn put(&mut self, store: &dyn SourceStore, path: &Path, text: Option<String>) {
        if !self.stored.contains_key(path) {
            self.stored
                .insert(path.to_path_buf(), store.read_to_string(path).ok());
        }
        self.texts.insert(path.to_path_buf(), text);
    }

    /// Whether the session holds a text (or no file) for `path`: one it has read or changed.
    #[must_use]
    pub fn holds(&self, path: &Path) -> bool {
        self.texts.contains_key(path)
    }

    /// Whether the session's text of `path`, when it holds one, differs from the store's.
    #[must_use]
    pub fn differs(&self, path: &Path) -> bool {
        match (self.texts.get(path), self.stored.get(path)) {
            (Some(t), Some(d)) => t != d,
            _ => false,
        }
    }

    /// Whether any file differs from the store's.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        self.texts
            .iter()
            .any(|(p, t)| self.stored.get(p).is_some_and(|d| d != t))
    }

    /// Every file the session holds, with its text now.
    pub fn iter(&self) -> impl Iterator<Item = (&Path, Option<&str>)> + '_ {
        self.texts.iter().map(|(p, t)| (p.as_path(), t.as_deref()))
    }

    /// Write every file whose text differs from the store's, and remove one the session holds no
    /// file for that the store has. Answers the files written or removed, in path order.
    ///
    /// # Errors
    /// The path and the store's error for a file that will not write; files before it are
    /// written.
    pub fn save(&mut self, store: &dyn SourceStore) -> Result<Vec<PathBuf>, (PathBuf, io::Error)> {
        let mut written = Vec::new();
        for (path, text) in &self.texts {
            let stored = self.stored.get(path).cloned().flatten();
            if *text == stored {
                continue;
            }
            let result = match text {
                Some(t) => store.write(path, t.as_bytes()),
                None => store.remove(path),
            };
            if let Err(e) = result {
                for p in &written {
                    self.stored.insert(PathBuf::clone(p), self.texts[p].clone());
                }
                return Err((path.clone(), e));
            }
            written.push(path.clone());
        }
        for path in &written {
            self.stored.insert(path.clone(), self.texts[path].clone());
        }
        Ok(written)
    }
}

/// One step of the history: the text of each file it changed, before and after.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub before: Texts,
    pub after: Texts,
}

/// The steps a session can take back (undo) and do again (redo).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl History {
    /// A step taken: it goes on the undo side, and nothing can be done again.
    pub fn record(&mut self, step: Step) {
        self.undo.push(step);
        self.redo.clear();
    }

    /// A step taken inside a larger one still being taken: it goes on the undo side, and the
    /// redo side is left for the larger one to clear.
    pub fn push(&mut self, step: Step) {
        self.undo.push(step);
    }

    /// The last step taken, off the undo side, to be taken back; put it on the redo side with
    /// [`Self::undone`] once it is, or back with [`Self::push`] when it cannot be.
    pub fn take_undo(&mut self) -> Option<Step> {
        self.undo.pop()
    }

    /// A step taken back: it can be done again.
    pub fn undone(&mut self, step: Step) {
        self.redo.push(step);
    }

    /// The last step taken back, off the redo side, to be done again; put it on the undo side
    /// with [`Self::push`] once it is, or back with [`Self::undone`] when it cannot be.
    pub fn take_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    /// Nothing can be done again.
    pub fn clear_redo(&mut self) {
        self.redo.clear();
    }

    /// Only the first `len` steps taken back can be done again.
    pub fn truncate_redo(&mut self, len: usize) {
        self.redo.truncate(len);
    }

    /// How many steps undo and redo can take.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    /// How many steps undo can take.
    #[must_use]
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// Whether undo can take no step.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// The steps from `start` on, merged into one: each file's text before the first of them and
    /// after the last, so one undo takes them all back (the dabs of one brush stroke).
    pub fn merge_since(&mut self, start: usize) {
        if self.undo.len() <= start + 1 {
            return;
        }
        let steps: Vec<Step> = self.undo.drain(start..).collect();
        let mut merged = Step::default();
        for s in steps {
            for (p, t) in s.before {
                merged.before.entry(p).or_insert(t);
            }
            merged.after.extend(s.after);
        }
        self.undo.push(merged);
    }
}
