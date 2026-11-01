//! Watches and presence answers waiting for the link, each about one local character (a watch's
//! owner, a presence's subject) and for one other world. See [`crate::MAX_CHATTER`]. They go a
//! world at a time in turn, each world's at most at its share of the pace, so a world that floods
//! this one with watches delays only its own answers.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use dereth_network_protocol::limits::{Bucket, Rate};
use dereth_network_protocol::Message;

use crate::MAX_CHATTER;

#[derive(Debug, Default)]
pub(crate) struct Chatter {
    next: u64,
    /// In order: the local character's identity key, the pair's slot, the world it goes to, and
    /// the message.
    queue: BTreeMap<u64, (String, String, String, Message)>,
    /// The waiting message for each pair, by slot.
    slots: HashMap<String, u64>,
    /// What waits for each local character.
    by_local: HashMap<String, BTreeSet<u64>>,
    /// What waits for each other world, in order.
    by_world: BTreeMap<String, BTreeSet<u64>>,
    /// The world whose message went last.
    turn: String,
    /// What each other world may still be sent now.
    paces: HashMap<String, Bucket>,
}

impl Chatter {
    pub(crate) fn len(&self) -> usize {
        self.queue.len()
    }

    pub(crate) fn clear(&mut self) {
        self.queue.clear();
        self.slots.clear();
        self.by_local.clear();
        self.by_world.clear();
        self.paces.clear();
    }

    /// The local character a message is about, the pair it is for, and the world it goes to.
    fn keys(m: &Message) -> Option<(String, String, String)> {
        match m {
            Message::Watch { owner, target, .. } => Some((
                owner.key(),
                format!("w {} {}", owner.key(), target.key()),
                target.world().to_owned(),
            )),
            Message::Presence { to, of, .. } => Some((
                of.key(),
                format!("p {} {}", of.key(), to.key()),
                to.world().to_owned(),
            )),
            _ => None,
        }
    }

    /// Queues a watch or a presence answer: a newer one for the same pair takes the waiting one's
    /// place; past [`MAX_CHATTER`] the local character with the most waiting loses its oldest.
    pub(crate) fn push(&mut self, m: Message) {
        let Some((local, slot, world)) = Self::keys(&m) else {
            return;
        };
        if let Some(seq) = self.slots.get(&slot) {
            if let Some(waiting) = self.queue.get_mut(seq) {
                waiting.3 = m;
            }
            return;
        }
        if self.queue.len() >= MAX_CHATTER {
            let most = self
                .by_local
                .iter()
                .max_by_key(|(_, s)| s.len())
                .and_then(|(_, s)| s.first().copied());
            if let Some(seq) = most {
                self.remove(seq);
            }
        }
        let seq = self.next;
        self.next += 1;
        self.slots.insert(slot.clone(), seq);
        self.by_local.entry(local.clone()).or_default().insert(seq);
        self.by_world.entry(world.clone()).or_default().insert(seq);
        self.queue.insert(seq, (local, slot, world, m));
    }

    fn remove(&mut self, seq: u64) -> Option<Message> {
        let (local, slot, world, m) = self.queue.remove(&seq)?;
        self.slots.remove(&slot);
        if let Some(s) = self.by_local.get_mut(&local) {
            s.remove(&seq);
            if s.is_empty() {
                self.by_local.remove(&local);
            }
        }
        if let Some(s) = self.by_world.get_mut(&world) {
            s.remove(&seq);
            if s.is_empty() {
                self.by_world.remove(&world);
            }
        }
        Some(m)
    }

    /// The next message for the world after the one served last that is within its pace at `t`
    /// (`rate`), if any is.
    pub(crate) fn pop_ready(&mut self, t: f64, rate: Rate) -> Option<Message> {
        let worlds: Vec<String> = self
            .by_world
            .range::<str, _>((
                std::ops::Bound::Excluded(self.turn.as_str()),
                std::ops::Bound::Unbounded,
            ))
            .chain(self.by_world.range::<str, _>((
                std::ops::Bound::Unbounded,
                std::ops::Bound::Included(self.turn.as_str()),
            )))
            .map(|(k, _)| k.clone())
            .collect();
        for world in worlds {
            let pace = self
                .paces
                .entry(world.clone())
                .or_insert_with(|| Bucket::new(rate, t));
            if !pace.take(t) {
                continue;
            }
            let seq = *self.by_world.get(&world)?.first()?;
            self.turn = world;
            return self.remove(seq);
        }
        None
    }

    /// Forgets the paces of worlds that have nothing waiting and have refilled.
    pub(crate) fn settle(&mut self, t: f64) {
        let waiting = &self.by_world;
        self.paces
            .retain(|w, b| waiting.contains_key(w) || !b.full_at(t));
    }
}
