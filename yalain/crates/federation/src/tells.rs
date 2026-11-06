//! Tells to and from other worlds' characters.
//!
//! A present character's tell to another world's address is checked here (the world on the
//! network, the teller on it, the operator's blocks, the teller's own pace), goes to the hub, and
//! ends when the hub answers, with the line the teller is shown. A tell from another world is
//! shown to a present character on the network, after its ignore list and the operator's blocks,
//! within the pace that world is held to for it, and answered at once.
//!
//! A tell to an away, linked character of this world goes to the hub too, so that it may reach its
//! player's Discord ([`Federation::away_linked`]); the hub's answer to it ends it as for any
//! character not online. The player's reply comes back as a tell from that character's own address.

use dereth_network_protocol::text::{sanitize_line, sanitize_name, MAX_TEXT_CHARS};
use dereth_network_protocol::{Address, Message, Outcome};

use crate::{
    Ended, Federation, Friends, LinkState, Pending, State, TellEnd, TellId, AWAY_TEXT,
    DO_NOT_DISTURB_TEXT, NOT_ON, TOO_NEW, TOO_QUICKLY, UNAVAILABLE, WORLD_OFF,
};

/// What a teller is told when a limit the whole world shares held its tell back (the hub's, or
/// the receiving world's).
fn held_back(to: &Address) -> String {
    format!(
        "The Dereth Network is holding back tells from this world to {to} for now. Wait a moment \
         and try again."
    )
}

/// What a teller is told when the recipient's world cannot be reached: the hub answers so, or
/// does not answer in time.
fn cannot_reach(to: &Address) -> String {
    format!("The Dereth Network can't reach {} right now.", to.world())
}

/// The away line a teller sees: always `<to> is away: <text>`, built here from the away text
/// alone, so another world cannot put a line of its own choosing in front of the teller. A text
/// that repeats the start of the line itself is taken without it.
fn away_line(to: &Address, message: Option<&str>) -> String {
    let text = message.map(sanitize_line).unwrap_or_default();
    let prefix = format!("{to} is away:");
    let text = if text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(&prefix)
    {
        text[prefix.len()..].trim().to_owned()
    } else {
        text
    };
    let text = if text.is_empty() {
        AWAY_TEXT.to_owned()
    } else {
        text
    };
    sanitize_line(&format!("{to} is away: {text}"))
}

/// The end of a tell to `to` with `outcome` (and the away text the receiving world sent).
fn answered(to: &Address, outcome: Outcome, message: Option<&str>) -> TellEnd {
    let (ended, line) = match outcome {
        Outcome::Delivered | Outcome::Ok => (Ended::Delivered, None),
        Outcome::Afk => (Ended::Away, Some(away_line(to, message))),
        Outcome::Squelched => (Ended::Squelched, None),
        Outcome::Offline => (Ended::Offline, None),
        Outcome::NotMember => (
            Ended::NotMember,
            Some(format!("You are not a member of {to}.")),
        ),
        Outcome::Unavailable => (Ended::Unavailable, Some(cannot_reach(to))),
        Outcome::NotOnNetwork => (
            Ended::NotOnNetwork,
            Some(format!("{} isn't on the network.", to.world())),
        ),
        // The teller's own pace was kept here before the tell went, so a refusal is the world's
        // share, or the loop guard's.
        Outcome::RateLimited => (Ended::RateLimited, Some(held_back(to))),
    };
    TellEnd { ended, line }
}

/// An end decided here, before the hub was asked.
fn refused(ended: Ended, line: Option<&str>) -> TellEnd {
    TellEnd {
        ended,
        line: line.map(str::to_owned),
    }
}

impl Federation {
    /// The address of the character of this world named `name`, when a tell to it from `teller`
    /// goes to the hub: the link is up, the teller is on the network, and the character is not in
    /// the world but is on the network and asked for its link, so the tell may reach its player's
    /// Discord. `None` for any other, whose tell stays the world's own.
    #[must_use]
    pub fn away_linked(&self, friends: &impl Friends, teller: &str, name: &str) -> Option<Address> {
        if !self.enabled() || self.link != LinkState::Up || !self.is_on(teller) {
            return None;
        }
        let on = self.on.get(&name.to_lowercase())?;
        if !self.asked_link.contains(&on.id) || friends.in_world(&on.name) {
            return None;
        }
        self.own_address(&on.name)
    }

    /// A present character's tell to another world's character, or to an away, linked character of
    /// this world, `tell` being the friends service's mark for it. It ends through [`Friends::tell_ended`]: at once when it is refused
    /// here, else when the hub answers, or when it is given up on ([`Federation::tick`]).
    ///
    /// Refused here: from a world off the network; from a character off the network; to a
    /// world or character the operator blocks (not shown, as an unknown name); from a character
    /// younger than the operator asks; past the teller's own pace. The text is cut to what a link message carries.
    pub fn tell(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        tell: TellId,
        from: &str,
        to: &Address,
        text: &str,
    ) {
        if !self.enabled() {
            friends.tell_ended(tell, refused(Ended::WorldOff, Some(WORLD_OFF)));
            return;
        }
        let Some(name) = self.on_name(from).map(str::to_owned) else {
            friends.tell_ended(tell, refused(Ended::NotOn, Some(NOT_ON)));
            return;
        };
        let own_world = to.world() == self.settings.slug;
        if (own_world && self.away_linked(&*friends, &name, to.name()).is_none())
            || self.blocked(to)
        {
            friends.tell_ended(tell, refused(Ended::Offline, None));
            return;
        }
        if self.too_new(&name) {
            friends.tell_ended(tell, refused(Ended::TooNew, Some(TOO_NEW)));
            return;
        }
        let Some(own) = self.own_address(&name) else {
            friends.tell_ended(tell, refused(Ended::Offline, None));
            return;
        };
        let rate = self.limits.tells_per_character;
        if !self
            .sends
            .entry(name.to_lowercase())
            .or_insert_with(|| dereth_network_protocol::limits::Bucket::new(rate, t))
            .take(t)
        {
            friends.tell_ended(tell, refused(Ended::TooQuickly, Some(TOO_QUICKLY)));
            return;
        }
        // Whether the teller lists the recipient: worlds of the first version let a character
        // take tells from its friends alone, and read it.
        let friend = self.remote_of(&*friends, &name).iter().any(|f| f.same(to));
        let id = self.fresh_id('t');
        self.pending.insert(
            id.clone(),
            Pending {
                tell,
                to: to.clone(),
                sent: None,
            },
        );
        self.send(
            t,
            Message::Tell {
                id,
                from: own,
                to: to.clone(),
                text: text.chars().take(MAX_TEXT_CHARS).collect(),
                friend,
            },
        );
    }

    /// The answer to one of this world's tells.
    pub(crate) fn on_result(
        &mut self,
        friends: &mut impl Friends,
        id: &str,
        outcome: Outcome,
        message: Option<&str>,
    ) {
        let Some(p) = self.pending.remove(id) else {
            return;
        };
        friends.tell_ended(p.tell, answered(&p.to, outcome, message));
    }

    /// A tell that will not be answered: it waited too long for a link (the network is
    /// unavailable), or the hub took too long to answer it (the recipient's world cannot be
    /// reached, as an `unavailable` answer says).
    pub(crate) fn give_up(&mut self, friends: &mut impl Friends, id: &str) {
        let Some(p) = self.pending.remove(id) else {
            return;
        };
        let line = if p.sent.is_some() {
            cannot_reach(&p.to)
        } else {
            UNAVAILABLE.to_owned()
        };
        friends.tell_ended(
            p.tell,
            TellEnd {
                ended: Ended::NetworkUnavailable,
                line: Some(line),
            },
        );
    }

    /// The answer owed to `world` for its tell `id`.
    fn answer(
        &mut self,
        t: f64,
        world: &str,
        id: String,
        outcome: Outcome,
        message: Option<String>,
    ) {
        self.owe(
            t,
            world,
            &Message::TellResult {
                id,
                outcome,
                message,
            },
        );
    }

    /// A tell from another world for a character of this one.
    pub(crate) fn on_tell(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        id: String,
        from: &Address,
        to: &Address,
        text: &str,
    ) {
        if self.link != LinkState::Up {
            return;
        }
        let origin = from.world().to_owned();
        // A world already owed its share of answers is not answered again until it is owed
        // fewer; the hub answers this one for it.
        if !self.may_owe(&origin) {
            tracing::debug!(
                "Dereth Network: {origin} is owed its share of answers; a tell is not taken"
            );
            return;
        }
        // A tell from this world's own address is a reply from elsewhere, or it is refused: its
        // teller must be one of this world's characters not in the world, on the network and linked
        // by itself, so that only its player can have answered, from their Discord.
        let own_teller = origin == self.settings.slug;
        if own_teller
            && (friends.in_world(from.name())
                || !self
                    .on
                    .get(&from.name().to_lowercase())
                    .is_some_and(|o| self.asked_link.contains(&o.id)))
        {
            self.answer(t, &origin, id, Outcome::Offline, None);
            return;
        }
        let target = (to.world() == self.settings.slug)
            .then(|| friends.present(to.name()))
            .flatten();
        let Some(target) = target else {
            self.answer(t, &origin, id, Outcome::Offline, None);
            return;
        };
        // The sender's name passes the text rules, or the tell is not shown.
        let shown_as = sanitize_name(&from.to_string())
            .ok()
            .and_then(|n| Address::parse(&n).ok());
        let Some(shown_as) = shown_as else {
            self.answer(t, &origin, id, Outcome::Offline, None);
            return;
        };
        if !self.is_on(&target.name) || self.blocked(from) {
            self.answer(t, &origin, id, Outcome::Offline, None);
            return;
        }
        if friends.ignores(&target.name, from)
            || (own_teller && friends.ignores_here(&target.name, from.name()))
        {
            self.answer(t, &origin, id, Outcome::Squelched, None);
            return;
        }
        // Only a tell that would be shown counts against the pace, so a refusal says nothing a
        // delivered tell would not. One world's tells to one character are held to the
        // per-recipient pace here too.
        let rate = self.limits.tells_per_recipient;
        if !self.may_receive(t, &origin, &to.key(), rate) {
            self.answer(t, &origin, id, Outcome::RateLimited, None);
            return;
        }
        friends.show_tell(&target.name, &shown_as, &sanitize_line(text));
        // Only the away text goes back: the sender's world writes the line around it. The wire
        // carries at most the network's text limit, so a longer one is cut to it.
        let away = |text: Option<String>, default: &str| {
            Some(match text {
                Some(m) if !m.chars().all(char::is_whitespace) => {
                    m.chars().take(MAX_TEXT_CHARS).collect()
                }
                _ => default.to_owned(),
            })
        };
        match target.state {
            State::Available => self.answer(t, &origin, id, Outcome::Delivered, None),
            State::Away(text) => {
                let m = away(text, AWAY_TEXT);
                self.answer(t, &origin, id, Outcome::Afk, m);
            }
            State::DoNotDisturb(text) => {
                let m = away(text, DO_NOT_DISTURB_TEXT);
                self.answer(t, &origin, id, Outcome::Afk, m);
            }
        }
    }
}
