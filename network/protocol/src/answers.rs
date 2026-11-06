//! The lines a world shows a player for the hub's answers to its requests, the same on every world
//! (`docs/network/03-world-behaviour.md` sections 4 and 9). They read no state: a world hands in the
//! request and the answer, and shows the line.

use crate::person::{PersonOp, PersonOutcome};
use crate::wire::{ChannelOp, ChannelOutcome};
use crate::Address;

/// What a character not linked to a person is told when it asks for something only a person has.
const LINK_FIRST: &str = "Link this character first: type @dn link.";

/// What a linked character adding a friend is told when its person lists as many friends across
/// the network as the hub allows.
pub const FRIENDS_FULL: &str =
    "You have as many friends on the Dereth Network as it allows. Remove one first.";

/// The line for a code the hub gave: the link that links the character, from the hub's page and
/// code, and how long it lasts. A hub whose page does not carry the code is answered with the code
/// to enter there.
#[must_use]
pub fn link_line(code: &str, url: &str, expires_secs: u32) -> String {
    let minutes = expires_secs.div_ceil(60).max(1);
    let lasts = if minutes == 1 {
        "1 minute".to_owned()
    } else {
        format!("{minutes} minutes")
    };
    if url.contains(code) {
        format!(
            "Open {url} within {lasts} to link this character to your Discord account. Keep the link to yourself."
        )
    } else {
        format!(
            "Your link code is {code}. Enter it at {url} within {lasts} to link this character to your Discord account. Keep the code to yourself."
        )
    }
}

/// The line for the answer to a person request a player asked for; none for an answer that shows
/// nothing (a code answered with no code or page, or a request only ever made quietly). `target`
/// is the character it names, as shown.
#[must_use]
pub fn person_line(
    op: PersonOp,
    outcome: PersonOutcome,
    target: &str,
    code: Option<&str>,
    url: Option<&str>,
    expires_secs: Option<u32>,
) -> Option<String> {
    let line = match (op, outcome) {
        (PersonOp::Code, PersonOutcome::Ok) => {
            return Some(link_line(code?, url?, expires_secs.unwrap_or(600)));
        }
        (PersonOp::Code, PersonOutcome::RateLimited) => {
            "You have asked for too many links. Wait a while and try again.".to_owned()
        }
        (_, PersonOutcome::Unavailable) => {
            "The Dereth Network's hub does not link characters.".to_owned()
        }
        (_, PersonOutcome::RateLimited) => {
            "The Dereth Network is holding back your requests for now. Wait a while and try again."
                .to_owned()
        }
        (PersonOp::Code, _) => "The Dereth Network gave no link.".to_owned(),
        (PersonOp::Unlink, PersonOutcome::Ok) => {
            "This character is no longer linked to a Discord account.".to_owned()
        }
        (PersonOp::Unlink, _) => "This character is not linked to a Discord account.".to_owned(),
        (_, PersonOutcome::NotLinked) => LINK_FIRST.to_owned(),
        // The hub's limit on the friends a person may list across the network.
        (PersonOp::Befriend, PersonOutcome::Refused) => FRIENDS_FULL.to_owned(),
        (PersonOp::Befriend, _) => format!("{target} has been added to your friends list."),
        // Only ever asked quietly; the other two no world of this version asks.
        (PersonOp::Unfriend | PersonOp::Carry | PersonOp::Alts | PersonOp::Characters, _) => {
            return None;
        }
    };
    Some(line)
}

/// The line a channel request's answer shows.
#[must_use]
pub fn channel_line(
    op: ChannelOp,
    outcome: ChannelOutcome,
    channel: &str,
    who: Option<&Address>,
    members: &[Address],
    channels: &[String],
) -> String {
    let address = format!("{channel}@channel");
    let names = |names: Vec<String>| names.join(", ");
    match (outcome, op) {
        (ChannelOutcome::Ok, ChannelOp::List) if channels.is_empty() => {
            "You are in no channel.".to_owned()
        }
        (ChannelOutcome::Ok, ChannelOp::List) => format!(
            "Your channels: {}.",
            names(channels.iter().map(|c| format!("{c}@channel")).collect())
        ),
        (ChannelOutcome::Ok, ChannelOp::Members) => format!(
            "Members of {address}: {}.",
            names(members.iter().map(ToString::to_string).collect())
        ),
        (ChannelOutcome::Ok, ChannelOp::Create) => format!(
            "You have made the channel {address}. Invite someone with @dn channel invite {channel} Name@world."
        ),
        (ChannelOutcome::Ok, ChannelOp::Invite) => format!(
            "{} is invited to {address}.",
            who.map_or_else(|| "That character".to_owned(), ToString::to_string)
        ),
        (ChannelOutcome::Ok, ChannelOp::Join) => {
            format!("You have joined {address}. Talk in it with @tell {address}, text.")
        }
        (ChannelOutcome::Ok, ChannelOp::Leave) => format!("You have left {address}."),
        (ChannelOutcome::Exists, _) => format!("{address} already exists."),
        (ChannelOutcome::NoChannel, _) => format!("There is no channel {address}."),
        (ChannelOutcome::NotMember, _) => format!("You are not a member of {address}."),
        (ChannelOutcome::NotInvited, _) => format!("You have not been invited to {address}."),
        (ChannelOutcome::AlreadyMember, _) => {
            format!("That character is already a member of {address}.")
        }
        (ChannelOutcome::Refused, _) => {
            format!("The Dereth Network refused that for {address}.")
        }
    }
}
