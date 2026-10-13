//! Not ACE: what `/dn` (and `/derethnetwork`, the same command spelled out) does, and the
//! channel requests it makes.
//!
//! `/dn` is every player's: putting a character on the network and taking it off, its remote
//! friends, linking it to its player's Discord account, squelching a whole world, and channels.
//! `/dn admin` is the operator's: blocking a world or one remote character for everyone on this
//! world, and where the world carries the Global room. The command table (`empyrean-command`)
//! parses the line, checks who may use `admin`, and calls [`dn`] or [`admin_by`].
//!
//! Help is one line per command, `@dn on - put this character on the Dereth Network`; a command
//! given wrongly is answered with its own line.
//!
//! A name or address a command takes may be given with the mark a line said from Discord shows
//! beside its speaker (`Ann@coldeve (Discord)`), as a player copies it: the mark is taken off.

use dereth_network_protocol::address::{is_channel_name, SEPARATOR};
use dereth_network_protocol::say::without_mark;
use dereth_network_protocol::{Address, ChannelOp, ChannelOutcome, CharacterSettings, Message};
use empyrean_entity::ObjectGuid;

use super::{
    directory, enabled, fresh_id, own_address, persons, record_mut, send, social, tell_player,
    LinkState, PendingChannel, NOT_ON,
};
use crate::managers::player_manager;
use crate::World;

/// The player's commands, each with what it does: `/dn help` shows one line for each,
/// `@dn on - put this character on the Dereth Network`.
pub const COMMANDS: &[(&str, &str)] = &[
    ("@dn", "show your Dereth Network status"),
    ("@dn on", "put this character on the Dereth Network"),
    ("@dn off", "take this character off the Dereth Network"),
    ("@dn friends", "list your friends on other worlds"),
    (
        "@dn friends add <Name@world>",
        "add a friend on another world",
    ),
    (
        "@dn friends remove <Name@world>",
        "remove a friend on another world",
    ),
    ("@dng <text>", "say a line in the Global room"),
    ("@dn global <text>", "say a line in the Global room"),
    ("@dn squelch <world>", "squelch every character of a world"),
    ("@dn unsquelch <world>", "unsquelch a world"),
    ("@dn channel create <channel>", "make a channel"),
    (
        "@dn channel invite <channel> <Name@world>",
        "invite a character to a channel",
    ),
    (
        "@dn channel join <channel>",
        "join a channel you were invited to",
    ),
    ("@dn channel leave <channel>", "leave a channel"),
    ("@dn channel members <channel>", "list a channel's members"),
    ("@dn channel list", "list your channels"),
    (
        "@dn link",
        "get a link that links this character to your Discord account",
    ),
    (
        "@dn unlink",
        "unlink this character from your Discord account",
    ),
    ("@dn help", "show these commands"),
    (
        "@tell <Name@world>, <text>",
        "tell a character on another world",
    ),
    ("@tell <channel>@channel, <text>", "say a line in a channel"),
];

/// The operator's commands, as [`COMMANDS`] (`@dn` replaced by `prefix`: the console types `dn`).
pub const ADMIN_COMMANDS: &[(&str, &str)] = &[
    (
        "@dn admin block <world or Name@world>",
        "block a world or a character for everyone on this world",
    ),
    ("@dn admin unblock <world or Name@world>", "lift a block"),
    ("@dn admin block", "show the blocks and the network's state"),
    (
        "@dn admin global <general|trade|lfg|roleplay|off>",
        "carry the Global room in another of this world's rooms",
    ),
    (
        "@dn admin global",
        "show where this world carries the Global room",
    ),
    (
        "@dn verify <token>",
        "verify this world with the token the site gave you",
    ),
];

/// One command's help line: `@dn on - put this character on the Dereth Network`.
fn line(command: &str, does: &str) -> String {
    format!("{command} - {does}")
}

/// `/dn help`: one line for each of the player's commands.
#[must_use]
pub fn help() -> String {
    COMMANDS
        .iter()
        .map(|(c, d)| line(c, d))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The operator's help, one line for each of its commands, as typed in game (`@dn`) or at the
/// console (`dn`).
#[must_use]
pub fn admin_help(prefix: &str) -> String {
    ADMIN_COMMANDS
        .iter()
        .map(|(c, d)| line(&c.replacen("@dn", prefix, 1), d))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The help line of the command that starts `command`, for a command given wrongly.
#[must_use]
pub fn usage(command: &str) -> String {
    COMMANDS
        .iter()
        .chain(ADMIN_COMMANDS)
        .find(|(c, _)| c.starts_with(command))
        .map_or_else(help, |(c, d)| line(c, d))
}

/// `n` of `thing`, in words: "1 friend", "3 friends".
fn count(n: usize, thing: &str) -> String {
    if n == 1 {
        format!("1 {thing}")
    } else {
        format!("{n} {thing}s")
    }
}

/// `/dn <args>` for `player`.
pub fn dn(w: &mut World, player: ObjectGuid, args: &[String]) {
    if !enabled(w) {
        tell_player(w, player, "This world is not on the Dereth Network.");
        return;
    }
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let character = player.full();
    match words.as_slice() {
        [] | ["status"] => status(w, player),
        ["help"] => tell_player(w, player, &help()),
        ["on"] => on(w, player),
        ["off"] => {
            if !super::is_on(w, character) {
                tell_player(w, player, "You are not on the Dereth Network.");
                return;
            }
            let linked = persons::linked(w, player);
            go_off(w, player);
            tell_player(
                w,
                player,
                "You are off the Dereth Network. Nobody on another world can see you or tell you, and your profile is out of its directory.",
            );
            if linked {
                tell_player(w, player, PAUSED);
            }
        }
        ["friends"] => {
            let rows = social::friend_rows(w, player);
            if rows.is_empty() {
                tell_player(
                    w,
                    player,
                    "You have no friends on other worlds. Add one with @dn friends add Name@world.",
                );
            } else {
                let lines: Vec<String> = rows
                    .iter()
                    .map(|(_, n, on)| format!("{n}: {}", if *on { "online" } else { "offline" }))
                    .collect();
                tell_player(
                    w,
                    player,
                    &format!("Your friends on other worlds:\n{}", lines.join("\n")),
                );
            }
        }
        ["link"] => persons::link(w, player),
        ["unlink"] => persons::unlink(w, player),
        ["friends", "add", name @ ..] if !name.is_empty() => {
            // What `@friends add Name@world` does.
            if !social::add_friend(w, player, without_mark(&name.join(" "))) {
                tell_player(w, player, &usage("@dn friends add"));
            }
        }
        ["friends", "remove", name @ ..] if !name.is_empty() => {
            // What removing the friend's row in the friends panel does.
            let listed = Address::parse(without_mark(&name.join(" ")))
                .ok()
                .and_then(|a| {
                    w.federation
                        .store
                        .characters
                        .get(&character)
                        .and_then(|r| r.friends.get(&a.key()).cloned())
                });
            match listed.and_then(|a| super::stand_in(w, &a)) {
                Some(id) => {
                    social::remove_friend(w, player, id);
                }
                None => tell_player(w, player, "That character is not in your friends list!"),
            }
        }
        ["friends", "add" | "remove"] => {
            tell_player(w, player, &usage(&format!("@dn friends {}", words[1])));
        }
        ["squelch", world] | ["unsquelch", world] => squelch_world(w, player, words[0], world),
        ["channel", rest @ ..] => channel(w, player, rest),
        ["global", rest @ ..] => global(w, player, &rest.join(" ")),
        _ => tell_player(w, player, &help()),
    }
}

/// What a linked character going off the network is told: its link pauses, and `/dn on` resumes
/// it with nothing to redo.
pub const PAUSED: &str =
    "This character's link to your Discord account is paused until you type @dn on.";

/// `player`'s character goes off the network, as `/dn off` takes it: its watchers see it offline,
/// the rows its person friends listed leave its friends panel, and it is out of the directory at
/// once.
pub(super) fn go_off(w: &mut World, player: ObjectGuid) {
    social::went_off(w, player);
    record_mut(w, player.full()).settings = CharacterSettings::OFF;
    w.federation.dirty = true;
    directory::went_off(w, player);
}

/// `/dn on`: the character goes on the network as `Name@thisworld`, seen online by anyone, told by
/// anyone and listed in the directory.
fn on(w: &mut World, player: ObjectGuid) {
    let name = crate::dispatch::name::name(w, player).unwrap_or_default();
    let Some(address) = own_address(w, player).filter(|_| !name.contains(SEPARATOR)) else {
        tell_player(
            w,
            player,
            "This character's name cannot be a network address, so it cannot join the Dereth Network.",
        );
        return;
    };
    let character = player.full();
    if super::is_on(w, character) {
        tell_player(
            w,
            player,
            &format!("You are on the Dereth Network already, as {address}."),
        );
        return;
    }
    record_mut(w, character).settings = CharacterSettings::ON;
    w.federation.dirty = true;
    tell_player(
        w,
        player,
        &format!(
            "You are on the Dereth Network as {address}. Anyone on the network can see when you are online, send you tells and find your profile in its directory. Type @dn help for the commands."
        ),
    );
    tell_player(
        w,
        player,
        "Your profile shows your level, skills, allegiance and more, and changes as you play. Type @dn off to leave the network.",
    );
    directory::announce(w, player);
    social::on_login(w, player);
    directory::back_on(w, player);
}

/// `/dn squelch <world>` and `/dn unsquelch <world>`.
fn squelch_world(w: &mut World, player: ObjectGuid, word: &str, world: &str) {
    let character = player.full();
    let world = world.to_ascii_lowercase();
    if !dereth_network_protocol::address::is_slug(&world) || world == w.federation.settings.slug {
        tell_player(w, player, &usage(&format!("@dn {word}")));
        return;
    }
    let add = word == "squelch";
    let held = w.federation.store.characters.get(&character).map(|r| {
        (
            r.squelched_worlds.contains(&world),
            r.squelched_worlds.len(),
        )
    });
    if add && held.is_some_and(|(already, n)| !already && n >= super::MAX_SQUELCHED_WORLDS) {
        tell_player(
            w,
            player,
            &format!(
                "You squelch {} worlds already. Unsquelch one first.",
                super::MAX_SQUELCHED_WORLDS
            ),
        );
        return;
    }
    let changed = match (add, held) {
        (true, Some((true, _))) | (false, None | Some((false, _))) => false,
        (true, _) => record_mut(w, character)
            .squelched_worlds
            .insert(world.clone()),
        (false, Some((true, _))) => record_mut(w, character).squelched_worlds.remove(&world),
    };
    w.federation.dirty |= changed;
    let line = match (add, changed) {
        (true, true) => format!("Every character of {world} has been squelched."),
        (true, false) => format!("{world} is already squelched."),
        (false, true) => format!("{world} has been unsquelched."),
        (false, false) => format!("{world} is not squelched."),
    };
    tell_player(w, player, &line);
}

/// `/dng <text>`, and `/dn global <text>`, for `player`.
pub fn global(w: &mut World, player: ObjectGuid, text: &str) {
    if !enabled(w) {
        tell_player(w, player, "This world is not on the Dereth Network.");
        return;
    }
    super::rooms::say_global(w, player, text);
}

/// The hub's state, as the status lines say it.
fn hub_state(w: &World) -> &'static str {
    match w.federation.link {
        LinkState::Up => "connected",
        LinkState::Greeting => "connecting",
        LinkState::Down => "not connected",
    }
}

/// `/dn`: whether the character is on the network, as what, the hub, its friends on other worlds,
/// whether it is linked, and its page in the directory once the hub has named one.
fn status(w: &mut World, player: ObjectGuid) {
    let rec = w
        .federation
        .store
        .characters
        .get(&player.full())
        .cloned()
        .unwrap_or_default();
    let who = own_address(w, player)
        .map(|a| a.to_string())
        .unwrap_or_default();
    if !rec.settings.on {
        tell_player(
            w,
            player,
            &format!("You are not on the Dereth Network. Type @dn on to join it as {who}."),
        );
        return;
    }
    let friends = match rec.friends.len() {
        0 => "You have no friends on other worlds.".to_owned(),
        n => format!("You have {} on other worlds.", count(n, "friend")),
    };
    let linked = match persons::state(w, player.full()) {
        Some(st) if st.linked => " This character is linked to a Discord account.",
        _ => "",
    };
    tell_player(
        w,
        player,
        &format!(
            "You are on the Dereth Network as {who}. The hub is {}. {friends}{linked}",
            hub_state(w)
        ),
    );
    if let Some(page) = rec.listing.as_ref().and_then(|l| l.page.as_deref()) {
        tell_player(w, player, &format!("Your directory page: {page}"));
    }
}

fn channel(w: &mut World, player: ObjectGuid, words: &[&str]) {
    if !super::is_on(w, player.full()) {
        tell_player(w, player, NOT_ON);
        return;
    }
    let (op, name, who) = match words {
        ["create", n] => (ChannelOp::Create, *n, None),
        ["join", n] => (ChannelOp::Join, *n, None),
        ["leave", n] => (ChannelOp::Leave, *n, None),
        ["members", n] => (ChannelOp::Members, *n, None),
        ["list"] => (ChannelOp::List, "list", None),
        ["invite", n, rest @ ..] if !rest.is_empty() => {
            match Address::parse(without_mark(&rest.join(" "))) {
                Ok(a) if !a.is_channel() => (ChannelOp::Invite, *n, Some(a)),
                _ => {
                    tell_player(w, player, &usage("@dn channel invite"));
                    return;
                }
            }
        }
        [op @ ("create" | "join" | "leave" | "members" | "invite"), ..] => {
            tell_player(w, player, &usage(&format!("@dn channel {op}")));
            return;
        }
        _ => {
            tell_player(w, player, &help());
            return;
        }
    };
    let name = name.trim_end_matches("@channel").to_ascii_lowercase();
    if !is_channel_name(&name) {
        tell_player(
            w,
            player,
            "A channel's name is 2 to 20 letters, digits and hyphens.",
        );
        return;
    }
    // A channel request carries text to other worlds' players (an invitation, a channel's name): a
    // gagged or too new character may not make one, and each counts against the hub's pace.
    if w.objects
        .get(player)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_gagged)
    {
        crate::world_objects::player::send_gag_error(w, player);
        return;
    }
    if super::too_new(w, player) {
        tell_player(w, player, super::tells::TOO_NEW);
        return;
    }
    if !super::may_send(w, player, super::Sends::ChannelRequests) {
        tell_player(w, player, super::tells::TOO_QUICKLY);
        return;
    }
    request(w, player, op, &name, who);
}

fn request(w: &mut World, player: ObjectGuid, op: ChannelOp, name: &str, who: Option<Address>) {
    let Some(by) = own_address(w, player) else {
        return;
    };
    let id = fresh_id(w, 'c');
    w.federation.pending_channels.insert(
        id.clone(),
        PendingChannel {
            player,
            op,
            who: who.clone(),
            sent: None,
        },
    );
    send(
        w,
        Message::Channel {
            id,
            op,
            channel: name.to_owned(),
            by,
            who,
        },
    );
}

/// Leaves a channel (a squelch of the channel's address).
pub(super) fn leave_channel(w: &mut World, player: ObjectGuid, name: &str) {
    request(w, player, ChannelOp::Leave, name, None);
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

/// The answer to a channel request.
pub(super) fn on_channel_result(
    w: &mut World,
    id: &str,
    outcome: ChannelOutcome,
    channel: &str,
    members: &[Address],
    channels: &[String],
) {
    let Some(p) = w.federation.pending_channels.remove(id) else {
        return;
    };
    if player_manager::get_online_player(w, p.player.full()).is_none() {
        return;
    }
    let line = channel_line(p.op, outcome, channel, p.who.as_ref(), members, channels);
    tell_player(w, p.player, &line);
}

/// A channel request that will not be answered.
pub(super) fn give_up_channel(w: &mut World, id: &str) {
    if let Some(p) = w.federation.pending_channels.remove(id) {
        if player_manager::get_online_player(w, p.player.full()).is_some() {
            tell_player(w, p.player, "The Dereth Network is unavailable.");
        }
    }
}

/// An invitation for one of this world's characters. It is shown as a tell from the inviter would
/// be: never past a squelch of the inviter, of its world or of the channel, or the operator's
/// blocks, and held to the pace a tell from the inviter's world to that character is.
pub(super) fn on_channel_invite(w: &mut World, channel: &str, by: &Address, to: &Address) {
    if to.world() != w.federation.settings.slug {
        return;
    }
    let Some(target) = player_manager::get_online_player_by_name(w, to.name()) else {
        return;
    };
    let character = target.full();
    let channel_address =
        Address::new(channel, dereth_network_protocol::address::CHANNEL_WORLD).ok();
    if !super::is_on(w, character)
        || super::blocked(w, by)
        || super::tells::squelches(w, character, by)
        || channel_address
            .as_ref()
            .is_none_or(|c| super::tells::squelches(w, character, c))
    {
        return;
    }
    let Ok(by_name) = dereth_network_protocol::text::sanitize_name(&by.to_string()) else {
        return;
    };
    let rate = w.federation.limits.tells_per_recipient;
    if !super::may_receive(w, by.world(), &to.key(), rate) {
        return;
    }
    tell_player(
        w,
        target,
        &format!(
            "{by_name} invites you to the channel {channel}@channel. Type @dn channel join {channel} to join it."
        ),
    );
}

/// `/dn admin <args>` at the console: [`admin_by`] with nobody to tell but the log.
pub fn admin(w: &mut World, args: &[String]) -> String {
    admin_by(w, None, args)
}

/// What the operator's bare `/dn admin block` and `/dn admin unblock` show: the network's state on
/// this world, then what is blocked.
fn admin_status(w: &World) -> String {
    let f = &w.federation;
    let mirrored: Vec<&str> = super::mirrored_slots(w)
        .into_iter()
        .map(super::RoomSlot::name)
        .collect();
    let online = f.worlds.values().filter(|i| i.online).count();
    let queued = f.queued();
    let blocked = if f.store.blocked.is_empty() {
        "Nothing is blocked.".to_owned()
    } else {
        format!(
            "Blocked: {}.",
            f.store
                .blocked
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "This world is on the Dereth Network as {}. The hub is {}. {} online. {} waiting for the hub. The Global room is carried in {}. Rooms mirrored into Discord: {}.\n{blocked}",
        f.settings.slug,
        hub_state(w),
        if online == 1 {
            "1 world is".to_owned()
        } else {
            format!("{online} worlds are")
        },
        if queued == 1 {
            "1 message is".to_owned()
        } else {
            format!("{queued} messages are")
        },
        super::global_slot(w).map_or("no room", super::RoomSlot::name),
        if mirrored.is_empty() {
            "none".to_owned()
        } else {
            mirrored.join(", ")
        },
    )
}

/// `/dn admin <args>`, typed by `who` (the console when none): the operator's blocks, and where
/// the world carries the Global room, whose answer from the hub goes to `who`. The line returned
/// is the immediate one. `/dn admin block` or `/dn admin unblock` without a target shows the
/// network's state and the blocks.
pub fn admin_by(w: &mut World, who: Option<ObjectGuid>, args: &[String]) -> String {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let prefix = if who.is_some() { "@dn" } else { "dn" };
    if !matches!(words.as_slice(), ["block" | "unblock" | "global", ..]) {
        return admin_help(prefix);
    }
    if !enabled(w) {
        // With the network off nothing is linked or saved: a block would be lost at the restart.
        return "This world is not on the Dereth Network.".to_owned();
    }
    match words.as_slice() {
        ["global", rest @ ..] => super::rooms::admin_global(w, who, rest),
        ["block" | "unblock"] => admin_status(w),
        [op, rest @ ..] => {
            let named = without_mark(&rest.join(" ")).to_owned();
            let key = match Address::parse(&named) {
                Ok(a) => a.key(),
                Err(_)
                    if dereth_network_protocol::address::is_slug(&named.to_ascii_lowercase()) =>
                {
                    named.to_ascii_lowercase()
                }
                Err(_) => {
                    return usage(&format!("@dn admin {op} <")).replacen("@dn", prefix, 1);
                }
            };
            let add = *op == "block";
            let changed = if add {
                let new = !w.federation.store.blocked.contains(&key);
                if new {
                    // What the block covers is told this world's characters are offline first.
                    social::block_beginning(w, &key);
                }
                w.federation.store.blocked.insert(key.clone())
            } else {
                let removed = w.federation.store.blocked.remove(&key);
                if removed {
                    social::block_ended(w, &key);
                }
                removed
            };
            w.federation.dirty |= changed;
            match (add, changed) {
                (true, true) => format!("{key} is blocked for everyone on this world."),
                (true, false) => format!("{key} was already blocked."),
                (false, true) => format!("{key} is no longer blocked."),
                (false, false) => format!("{key} was not blocked."),
            }
        }
        [] => admin_help(prefix),
    }
}

/// Whether `token` has the shape of the hub's verification token: `dnv_` and 32 characters of
/// base64url.
#[must_use]
pub fn is_verification_token(token: &str) -> bool {
    token.len() == 36
        && token.starts_with("dnv_")
        && token[4..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `/dn verify <token>`: the operator proves to the hub that this world is the one registered or
/// claimed on the site. The host sends the token to the hub with the world's key; the hub then
/// challenges the world's game port, and the world answers while the verification waits. The
/// hub's answer comes back to `who` (the console when none). The line returned is the immediate
/// one.
pub fn verify(w: &mut World, who: Option<ObjectGuid>, by: &str, args: &[String]) -> String {
    if !enabled(w) {
        return "This world is not on the Dereth Network.".to_owned();
    }
    let token = match args {
        [t] if is_verification_token(t.trim()) => t.trim().to_owned(),
        _ => {
            let prefix = if who.is_some() { "@dn" } else { "dn" };
            return usage("@dn verify").replacen("@dn", prefix, 1);
        }
    };
    if w.federation.verifying {
        return "A verification is on its way to the hub already.".to_owned();
    }
    w.federation.verifying = true;
    w.federation.verify_asked_at = Some(super::now(w));
    w.federation.answered_nonces.clear();
    w.federation.verify_requests.push(super::VerifyRequest {
        token,
        by: by.chars().take(32).collect(),
        who,
    });
    "Asking the Dereth Network's hub to verify this world...".to_owned()
}

/// Whether the link would carry a message now (for the host's status line).
#[must_use]
pub fn link_up(w: &World) -> bool {
    w.federation.link == LinkState::Up
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Divergence: V451
    ///
    /// Help is one line per command, the command, a dash and what it does, with no padding, no
    /// line split mid-sentence and no command the network no longer has.
    #[test]
    fn help_is_one_plain_line_per_command() {
        let help = help();
        let lines: Vec<&str> = help.lines().collect();
        assert_eq!(lines.len(), COMMANDS.len());
        assert_eq!(
            lines[1],
            "@dn on - put this character on the Dereth Network"
        );
        for l in lines.iter().copied().chain(admin_help("@dn").lines()) {
            assert!(l.starts_with('@'), "{l}");
            assert_eq!(l.matches(" - ").count(), 1, "{l}");
            assert!(!l.contains("  ") && l.trim() == l, "{l}");
            for gone in ["visible", "reachable", "listed", "alts", "worlds", "status"] {
                assert!(!l.starts_with(&format!("@dn {gone}")), "{l}");
            }
        }
        assert_eq!(
            usage("@dn friends add"),
            "@dn friends add <Name@world> - add a friend on another world"
        );
        assert!(admin_help("dn").starts_with("dn admin block <world or Name@world> - "));
    }
}
