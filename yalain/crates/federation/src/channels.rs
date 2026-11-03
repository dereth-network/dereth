//! Channels (`docs/network/03-world-behaviour.md` section 4): the hub keeps them, the world asks on
//! its characters' behalf and shows what comes. A channel is a tell address, so a line to one goes
//! as a tell ([`Federation::tell`]), and a line from one is shown as a tell from its address.

use dereth_network_protocol::address::CHANNEL_WORLD;
use dereth_network_protocol::answers::channel_line;
use dereth_network_protocol::text::{sanitize_line, sanitize_name};
use dereth_network_protocol::{Address, ChannelOp, ChannelOutcome, Message};

use crate::{Ask, AskKind, Federation, Friends, NOT_ON, TOO_NEW, TOO_QUICKLY};

impl Federation {
    /// A channel request of a character on the network, within its pace of requests. Refused, the
    /// line it is told.
    ///
    /// # Errors
    /// The line the character is told when the request is refused here.
    pub fn ask_channel(
        &mut self,
        t: f64,
        character: u64,
        name: &str,
        op: ChannelOp,
        channel: &str,
        who: Option<Address>,
    ) -> Result<(), &'static str> {
        if !self.is_on(name) {
            return Err(NOT_ON);
        }
        if self.too_new(name) {
            return Err(TOO_NEW);
        }
        if !self.may_ask(t, name) {
            return Err(TOO_QUICKLY);
        }
        self.request_channel(t, character, name, op, channel, who);
        Ok(())
    }

    /// A character squelched a channel's address, which leaves it. Not held to its pace: it only
    /// takes away.
    pub fn leave_channel(&mut self, t: f64, character: u64, name: &str, channel: &str) {
        if self.is_on(name) {
            self.request_channel(t, character, name, ChannelOp::Leave, channel, None);
        }
    }

    fn request_channel(
        &mut self,
        t: f64,
        character: u64,
        name: &str,
        op: ChannelOp,
        channel: &str,
        who: Option<Address>,
    ) {
        let Some(by) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return;
        };
        let id = self.fresh_id('c');
        self.asks.insert(
            id.clone(),
            Ask {
                character,
                kind: AskKind::Channel {
                    op,
                    who: who.clone(),
                },
                sent: None,
            },
        );
        self.send(
            t,
            Message::Channel {
                id,
                op,
                channel: channel.to_owned(),
                by,
                who,
            },
        );
    }

    /// The answer to a channel request.
    pub(crate) fn on_channel_result(
        &mut self,
        friends: &mut impl Friends,
        id: &str,
        outcome: ChannelOutcome,
        channel: &str,
        members: &[Address],
        channels: &[String],
    ) {
        let Some(a) = self.asks.remove(id) else {
            return;
        };
        let AskKind::Channel { op, who } = a.kind else {
            return;
        };
        let line = channel_line(op, outcome, channel, who.as_ref(), members, channels);
        friends.show_line(a.character, &sanitize_line(&line));
    }

    /// The present character a channel's message names, when it may be shown one from `from`: on
    /// the network, ignoring neither `from` (nor its world) nor the channel, the operator blocking
    /// neither, within the pace of a tell from `from`'s world to it. Its name as the world writes
    /// it, its id, and the channel's address.
    fn channel_recipient(
        &mut self,
        friends: &impl Friends,
        t: f64,
        channel: &str,
        from: &Address,
        to: &Address,
    ) -> Option<(String, u64, Address)> {
        if to.world() != self.settings.slug {
            return None;
        }
        let on = self.on.get(&to.name().to_lowercase())?.clone();
        let address = Address::new(channel, CHANNEL_WORLD).ok()?;
        if friends.present(&on.name).is_none()
            || self.blocked(from)
            || friends.ignores(&on.name, from)
            || friends.ignores(&on.name, &address)
        {
            return None;
        }
        let rate = self.limits.tells_per_recipient;
        self.may_receive(t, from.world(), &to.key(), rate)
            .then_some((on.name, on.id, address))
    }

    /// An invitation for one of this world's characters, shown as a tell from the inviter would be.
    pub(crate) fn on_channel_invite(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        channel: &str,
        by: &Address,
        to: &Address,
    ) {
        let Ok(by_name) = sanitize_name(&by.to_string()) else {
            return;
        };
        if let Some((_, id, _)) = self.channel_recipient(friends, t, channel, by, to) {
            friends.show_line(
                id,
                &format!(
                    "{by_name} invites you to the channel {channel}@channel. Say @dn channel join \
                     {channel} to join it."
                ),
            );
        }
    }

    /// A channel's line for a member on this world: a tell from the channel's own address, with the
    /// speaker's address at the start of the text.
    pub(crate) fn on_channel_line(
        &mut self,
        friends: &mut impl Friends,
        t: f64,
        channel: &str,
        from: &Address,
        to: &Address,
        text: &str,
    ) {
        let Ok(speaker) = sanitize_name(&from.to_string()) else {
            return;
        };
        if let Some((name, _, address)) = self.channel_recipient(friends, t, channel, from, to) {
            let line = sanitize_line(&format!("{speaker}: {}", sanitize_line(text)));
            friends.show_tell(&name, &address, &line);
        }
    }
}
