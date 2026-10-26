//! A file the player picks for a plugin (`files.import`): each use asks the player in the host's
//! question ([`HostQuestion`]), listing the files the player has put in the import folder (the
//! host's own data folder's `import/`); what the player gives is the plugin's to read, and nothing
//! else is.

use std::collections::VecDeque;

use dereth_client_runtime::dialogs::HostQuestion;

/// The import folder, inside the host's own data folder.
pub const FOLDER: &str = "import/";

/// The most bytes a file given to a plugin may hold.
pub const MAX_BYTES: usize = 4 << 20;

/// How long, in seconds, a plugin's next request waits after the player refuses one; each refusal
/// after doubles it.
pub const REFUSED_WAIT: f64 = 30.0;

/// The refusals in a session after which a plugin is asked for no more files until it is
/// reloaded.
pub const REFUSALS: u32 = 5;

/// One request waiting for the player.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub plugin: String,
    pub name: String,
    pub id: i64,
    pub prompt: String,
    /// The extensions it takes (`"met"`, `"nav"`), none for any.
    pub extensions: Vec<String>,
}

/// The requests, and the files the one in front is offered.
#[derive(Debug, Default)]
pub struct Import {
    pub queue: VecDeque<Request>,
    /// The files the question lists, in order.
    pub files: Vec<String>,
    /// Counts the questions put, so that an answer given to one reaches no other: a new request
    /// in front, or other files.
    pub serial: u64,
    /// The request and the files the question was last put for.
    asked: Option<(String, i64, Vec<String>)>,
    /// Requests held back after their plugin's refusal: when each may be put to the player.
    held: Vec<(f64, Request)>,
    /// Each plugin's refusals this session, and when its next request may be put.
    refused: std::collections::BTreeMap<String, (u32, f64)>,
}

impl Import {
    /// Take the requests `asked`, made by now (`now`, seconds): each queued for the player, held
    /// back while its plugin waits after a refusal, or turned away with why: a plugin with a
    /// request already waiting, or one refused [`REFUSALS`] times this session.
    pub fn take(&mut self, asked: Vec<Request>, now: f64) -> Vec<(Request, String)> {
        let mut turned = Vec::new();
        for r in asked {
            let (count, after) = self.refused.get(&r.plugin).copied().unwrap_or((0, 0.0));
            let waiting = self.queue.iter().any(|q| q.plugin == r.plugin)
                || self.held.iter().any(|(_, q)| q.plugin == r.plugin);
            if count >= REFUSALS {
                turned.push((
                    r,
                    "refused: no more files are asked for it until it is reloaded".to_owned(),
                ));
            } else if waiting {
                turned.push((r, "a file is already asked for".to_owned()));
            } else if after > now {
                self.held.push((after, r));
            } else {
                self.queue.push_back(r);
            }
        }
        turned
    }

    /// Move the requests held back whose time has come into the queue: whether any did.
    pub fn release_held(&mut self, now: f64) -> bool {
        let before = self.held.len();
        let (due, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|(at, _)| *at <= now);
        self.held = held;
        self.queue.extend(due.into_iter().map(|(_, r)| r));
        self.held.len() != before
    }

    /// The player refused plugin `plugin`'s request at `now`: its next waits [`REFUSED_WAIT`]
    /// seconds, doubled for each refusal before this one.
    pub fn refused(&mut self, plugin: &str, now: f64) {
        let entry = self.refused.entry(plugin.to_owned()).or_insert((0, 0.0));
        entry.0 += 1;
        let doubled = 1_u32 << entry.0.saturating_sub(1).min(16);
        entry.1 = now + REFUSED_WAIT * f64::from(doubled);
    }

    /// Plugin `plugin` is unloaded: its requests go, and its refusals are forgotten.
    pub fn forget(&mut self, plugin: &str) {
        self.held.retain(|(_, r)| r.plugin != plugin);
        self.refused.remove(plugin);
    }

    /// The files in the import folder that the request in front takes.
    #[must_use]
    pub fn matching(all: &[String], r: &Request) -> Vec<String> {
        let mut out: Vec<String> = all
            .iter()
            .filter_map(|p| p.strip_prefix(FOLDER))
            .filter(|p| {
                r.extensions.is_empty()
                    || r.extensions.iter().any(|e| {
                        p.rsplit_once('.')
                            .is_some_and(|(_, x)| x.eq_ignore_ascii_case(e))
                    })
            })
            .map(str::to_owned)
            .collect();
        out.sort();
        out
    }

    /// Put the request in front before the player, with `files` to pick from: a new question
    /// when the request in front or its files change. None in front takes the question down.
    pub fn put(&mut self, files: Vec<String>) {
        let Some(r) = self.queue.front() else {
            self.files.clear();
            self.asked = None;
            return;
        };
        let asked = (r.plugin.clone(), r.id, files.clone());
        if self.asked.as_ref() != Some(&asked) {
            self.asked = Some(asked);
            self.serial += 1;
        }
        self.files = files;
    }

    /// The question about the request in front: who asks and what for, the files to pick one
    /// of, and the answers, give the one picked or refuse. With no file to give it says where
    /// to put one, and only a refusal can answer it.
    #[must_use]
    pub fn question(&self, asker: String) -> Option<HostQuestion> {
        let r = self.queue.front()?;
        let mut text = format!(
            "{} asks for a file: {}",
            crate::shown::bounded(&r.name, crate::shown::NAME),
            crate::shown::bounded(&r.prompt, crate::shown::PROMPT)
        );
        if self.files.is_empty() {
            text.push_str(
                "\n\nNo file to give: put it in the import folder of the plugin data folder.",
            );
        }
        Some(HostQuestion {
            id: self.serial,
            title: "A file for a plugin".to_owned(),
            asker,
            text,
            picks: self.files.clone(),
            needs_pick: true,
            yes: "Give it".to_owned(),
            no: Some("Refuse".to_owned()),
        })
    }
}
