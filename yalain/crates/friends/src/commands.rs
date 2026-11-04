//! `@dn`, said in chat: the character's place on the Dereth Network.
//!
//! The client handles every `/` command itself, so the network's command is a line said without
//! one: `@dn` and its words (`HELP`). The world takes it before anyone hears it and answers in the
//! speaker's chat window with its own lines. What the hub answers later comes the same way
//! ([`yalain_federation::Friends::show_line`]).

use std::time::Duration;

use dereth_network_protocol::address::{is_channel_name, is_slug, looks_remote};
use dereth_network_protocol::say::without_mark;
use dereth_network_protocol::{Address, ChannelOp};
use yalain_federation::directory::page_line;
use yalain_federation::{
    Federation, Friends as _, LinkState, NOT_CARRIED, NOT_ON, PAUSED, WORLD_OFF,
};
use yalain_store::Store;

use crate::{secs, Service};

/// The commands, one line each, as `@dn help` shows them.
pub const HELP: [&str; 22] = [
    "@dn - show your Dereth Network status",
    "@dn on - put this character on the Dereth Network",
    "@dn off - take this character off the Dereth Network",
    "@dn friends - list your friends on other worlds",
    "@dn friends add <Name@world> - add a friend on another world",
    "@dn friends remove <Name@world> - remove a friend on another world",
    "@dng <text> - say a line in the Global room",
    "@dn global <text> - say a line in the Global room",
    "@dn squelch <world> - squelch every character of a world",
    "@dn unsquelch <world> - unsquelch a world",
    "@dn channel create <channel> - make a channel",
    "@dn channel invite <channel> <Name@world> - invite a character to a channel",
    "@dn channel join <channel> - join a channel you were invited to",
    "@dn channel leave <channel> - leave a channel",
    "@dn channel members <channel> - list a channel's members",
    "@dn channel list - list your channels",
    "@dn link - get a link that links this character to your Discord account",
    "@dn unlink - unlink this character from your Discord account",
    "@dn help - show these commands",
    "/tell <Name@world>, <text> - tell a character on another world",
    "/tell <channel>@channel, <text> - say a line in a channel",
    "/friend add <Name@world> - add a friend on another world",
];

/// The help line of the command that starts `command`, for a command given wrongly.
fn usage(command: &str) -> Vec<String> {
    HELP.iter()
        .find(|l| l.starts_with(command))
        .map_or_else(help, |l| vec![(*l).to_owned()])
}

/// One line for each of the player's commands.
fn help() -> Vec<String> {
    HELP.iter().map(|l| (*l).to_owned()).collect()
}

impl Service {
    /// A character said `@dn` and these words after it at `now`: the lines to answer it with.
    pub fn said(
        &mut self,
        now: Duration,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
        character: u64,
        name: &str,
        words: &str,
    ) -> Vec<String> {
        let Some(f) = fed.filter(|f| f.enabled()) else {
            return vec![WORLD_OFF.to_owned()];
        };
        self.now = now;
        let mut words = words.split_whitespace();
        let word = words.next().unwrap_or_default();
        let rest: Vec<&str> = words.collect();
        let address = Address::new(name, &f.settings().slug).ok();
        match word.to_ascii_lowercase().as_str() {
            "" | "status" => {
                let Some(a) = address else {
                    return vec![no_address()];
                };
                if !f.is_on(name) {
                    return vec![format!(
                        "You are not on the Dereth Network. Say @dn on to join it as {a}."
                    )];
                }
                let hub = match f.link() {
                    LinkState::Up => "connected",
                    LinkState::Greeting => "connecting",
                    LinkState::Down => "not connected",
                };
                let remote = f.remote_of(&*self, name).len();
                let friends = match remote {
                    0 => "You have no friends on other worlds.".to_owned(),
                    1 => "You have 1 friend on other worlds.".to_owned(),
                    n => format!("You have {n} friends on other worlds."),
                };
                let linked = if f.is_linked(name) {
                    " This character is linked to a Discord account."
                } else {
                    ""
                };
                let mut lines = vec![format!(
                    "You are on the Dereth Network as {a}. The hub is {hub}. {friends}{linked}"
                )];
                lines.extend(f.listing_page(name).map(page_line));
                lines
            }
            "on" => {
                let Some(a) = address else {
                    return vec![no_address()];
                };
                if f.is_on(name) {
                    return vec![format!("You are on the Dereth Network already, as {a}.")];
                }
                match f.set_on(store, self, character, name, true) {
                    Ok(_) => vec![
                        format!(
                            "You are on the Dereth Network as {a}. Anyone on the network can see \
                             when you are online and send you tells. Say @dn help for the commands."
                        ),
                        "Say @dn off to leave the network.".to_owned(),
                    ],
                    Err(e) => {
                        tracing::warn!("Dereth Network: the store: {e}");
                        vec!["The Dereth Network can't keep that now. Try again later.".to_owned()]
                    }
                }
            }
            "off" => {
                if !f.is_on(name) {
                    return vec!["You are not on the Dereth Network.".to_owned()];
                }
                let linked = f.is_linked(name);
                let shown: Vec<Address> = f
                    .remote_of(&*self, name)
                    .into_iter()
                    .filter(|a| f.shown(name, a) == Some(true))
                    .collect();
                match f.set_on(store, self, character, name, false) {
                    Ok(_) => {
                        // Nothing of another world is watched for it now: each such friend shown
                        // online shows offline, until its world answers a watch again after
                        // `@dn on`.
                        for friend in shown {
                            self.show_presence(name, &friend, false, true);
                        }
                        let mut lines = vec![
                            "You are off the Dereth Network. Nobody on another world can see you \
                              or tell you."
                                .to_owned(),
                        ];
                        if linked {
                            lines.push(PAUSED.to_owned());
                        }
                        lines
                    }
                    Err(e) => {
                        tracing::warn!("Dereth Network: the store: {e}");
                        vec!["The Dereth Network can't keep that now. Try again later.".to_owned()]
                    }
                }
            }
            "link" => f
                .link_person(store, secs(now), character, name)
                .into_iter()
                .collect(),
            "unlink" => f
                .unlink_person(secs(now), character, name)
                .into_iter()
                .collect(),
            "friends" => self.friends_said(store, f, character, name, &rest),
            "squelch" | "unsquelch" => {
                let word = word.to_ascii_lowercase();
                match rest.as_slice() {
                    [world]
                        if is_slug(&world.to_ascii_lowercase())
                            && !world.eq_ignore_ascii_case(&f.settings().slug) =>
                    {
                        let world = world.to_ascii_lowercase();
                        vec![self.squelch_world(character, name, &world, word == "squelch", store)]
                    }
                    _ => usage(&format!("@dn {word}")),
                }
            }
            "channel" => self.channel_said(f, character, name, &rest),
            "global" => {
                let Some(room) = f.global_room() else {
                    return vec![NOT_CARRIED.to_owned()];
                };
                let text = rest.join(" ");
                if text.is_empty() {
                    return usage("@dng");
                }
                if !f.is_on(name) {
                    return vec![NOT_ON.to_owned()];
                }
                f.say_global(character, room, text);
                Vec::new()
            }
            _ => help(),
        }
    }

    /// `@dn friends` and the words after it.
    fn friends_said(
        &mut self,
        store: &mut impl Store,
        f: &mut Federation,
        character: u64,
        name: &str,
        words: &[&str],
    ) -> Vec<String> {
        let Some((first, named)) = words.split_first() else {
            let listed = f.remote_of(&*self, name);
            if listed.is_empty() {
                return vec![
                    "You have no friends on other worlds. Add one with @dn friends add Name@world."
                        .to_owned(),
                ];
            }
            let mut lines = vec!["Your friends on other worlds:".to_owned()];
            lines.extend(listed.iter().map(|a| {
                let state = if f.shown(name, a) == Some(true) {
                    "online"
                } else {
                    "offline"
                };
                format!("{a}: {state}")
            }));
            return lines;
        };
        let first = first.to_ascii_lowercase();
        let joined = named.join(" ");
        let friend = without_mark(&joined);
        match (first.as_str(), friend.is_empty()) {
            ("add", true) | ("remove", true) => usage(&format!("@dn friends {first}")),
            ("add", false) if !looks_remote(friend) => usage("@dn friends add"),
            ("add", false) => self.friend_said(character, name, friend, store, f),
            ("remove", false) => self.unfriend_said(character, name, friend, store, f),
            _ => help(),
        }
    }

    /// `@dn channel` and the words after it.
    fn channel_said(
        &mut self,
        f: &mut Federation,
        character: u64,
        name: &str,
        words: &[&str],
    ) -> Vec<String> {
        if !f.is_on(name) {
            return vec![NOT_ON.to_owned()];
        }
        let op = words.first().map(|w| w.to_ascii_lowercase());
        let args = words.get(1..).unwrap_or_default();
        let (op, channel, who) = match (op.as_deref(), args) {
            (Some("create"), [n]) => (ChannelOp::Create, *n, None),
            (Some("join"), [n]) => (ChannelOp::Join, *n, None),
            (Some("leave"), [n]) => (ChannelOp::Leave, *n, None),
            (Some("members"), [n]) => (ChannelOp::Members, *n, None),
            (Some("list"), _) => (ChannelOp::List, "list", None),
            (Some("invite"), [n, invited @ ..]) if !invited.is_empty() => {
                match Address::parse(without_mark(&invited.join(" "))) {
                    Ok(a) if !a.is_channel() => (ChannelOp::Invite, *n, Some(a)),
                    _ => return usage("@dn channel invite"),
                }
            }
            (Some(op @ ("create" | "join" | "leave" | "members" | "invite")), _) => {
                return usage(&format!("@dn channel {op}"));
            }
            _ => return help(),
        };
        let channel = channel.trim_end_matches("@channel").to_ascii_lowercase();
        if !is_channel_name(&channel) {
            return vec!["A channel's name is 2 to 20 letters, digits and hyphens.".to_owned()];
        }
        // A channel request carries text to other worlds' players: a gagged character makes none.
        if self.gagged.contains(&character) {
            return vec![crate::GAGGED.to_owned()];
        }
        match f.ask_channel(secs(self.now), character, name, op, &channel, who) {
            Ok(()) => Vec::new(),
            Err(why) => vec![why.to_owned()],
        }
    }
}

/// What a character whose name cannot be an address is told.
fn no_address() -> String {
    "This character's name cannot be a network address, so it cannot join the Dereth Network."
        .to_owned()
}
