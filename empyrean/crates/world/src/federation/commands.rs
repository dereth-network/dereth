//! Not ACE: what `/dn` (and `/derethnetwork`, the same command spelled out) does, and the
//! channel requests it makes.
//!
//! `/dn` is every player's: opting a character in and out, its three settings, its remote
//! friends, squelching a whole world, and channels. `/dn admin` is the operator's: blocking a
//! world or one remote character for everyone on this world. The command table
//! (`empyrean-command`) parses the line, checks who may use `admin`, and calls [`dn`] or
//! [`admin`].

use dereth_network_protocol::address::{is_channel_name, SEPARATOR};
use dereth_network_protocol::{
    Address, ChannelOp, ChannelOutcome, CharacterSettings, Listed, Message, Reachable, Visible,
};
use empyrean_entity::ObjectGuid;

use super::{
    enabled, fresh_id, own_address, persons, record_mut, send, social, tell_player, LinkState,
    PendingChannel,
};
use crate::managers::player_manager;
use crate::World;

const HELP: &str = "\
/dn                         your network status
/dn on | off                put this character on the Dereth Network, or take it off
/dn visible nobody|friends|channels|anyone   who sees you online
/dn reachable nobody|friends|anyone          who may send you a first tell
/dn listed on|off           kept for a later version: nothing is published yet
/dn friends                 your friends on other worlds
/dn friends add | remove <Name@world>        a friend on another world; while this character
                            is linked, the friend's person, on whichever character it plays if it
                            shows its alts
/dn link | unlink           link this character to your Discord account on the site, or unlink it
/dn alts shown|hidden       whether others may learn that your linked characters are one person
/dn alts [Name@world]       the setting, or the characters of that character's person
/dn worlds                  the worlds on the network now
/dn global <text>           say a line in the Global room (also /dng <text>)
/dn squelch | unsquelch <world>              every character of a world
/dn channel create | join | leave | members <channel>
/dn channel invite <channel> <Name@world>
/dn channel list            your channels
Tell someone on another world with @tell Name@world, text. A channel is name@channel.";

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
        ["help"] => tell_player(w, player, HELP),
        ["on"] => {
            let name = crate::dispatch::name::name(w, player).unwrap_or_default();
            if name.contains(SEPARATOR) || own_address(w, player).is_none() {
                tell_player(
                    w,
                    player,
                    "This character's name cannot be a network address, so it cannot join the network.",
                );
                return;
            }
            let rec = record_mut(w, character);
            let first = !rec.settings.on && rec.settings == CharacterSettings::OFF;
            if first {
                rec.settings = CharacterSettings::OPTED_IN;
            } else {
                rec.settings.on = true;
            }
            w.federation.dirty = true;
            let address = own_address(w, player)
                .map(|a| a.to_string())
                .unwrap_or_default();
            tell_player(
                w,
                player,
                &format!(
                    "You are on the Dereth Network as {address}. Mutual friends see you online, and anyone on the network may tell you; /dn help for more."
                ),
            );
            social::on_login(w, player);
        }
        ["off"] => {
            if !super::is_on(w, character) {
                tell_player(w, player, "You are not on the Dereth Network.");
                return;
            }
            social::went_off(w, player);
            record_mut(w, character).settings.on = false;
            w.federation.dirty = true;
            tell_player(
                w,
                player,
                "You are off the Dereth Network. Nobody on another world sees or reaches you.",
            );
        }
        ["visible", v] => match v.parse::<Visible>() {
            Ok(v) => {
                record_mut(w, character).settings.visible = v;
                w.federation.dirty = true;
                tell_player(w, player, &format!("Visible: {v}."));
                social::settings_changed(w, player);
            }
            Err(()) => tell_player(w, player, "/dn visible nobody|friends|channels|anyone"),
        },
        ["reachable", v] => match v.parse::<Reachable>() {
            Ok(v) => {
                record_mut(w, character).settings.reachable = v;
                w.federation.dirty = true;
                tell_player(w, player, &format!("Reachable: {v}."));
            }
            Err(()) => tell_player(w, player, "/dn reachable nobody|friends|anyone"),
        },
        ["listed", v] => match v.parse::<Listed>() {
            Ok(v) => {
                record_mut(w, character).settings.listed = v;
                w.federation.dirty = true;
                tell_player(
                    w,
                    player,
                    &format!("Listed: {v}. Nothing is published yet: listing is kept for a later version."),
                );
            }
            Err(()) => tell_player(w, player, "/dn listed on|off"),
        },
        ["friends"] => {
            let rows = social::friend_rows(w, player);
            if rows.is_empty() {
                tell_player(
                    w,
                    player,
                    "You have no friends on other worlds. Add one with /dn friends add Name@world.",
                );
            } else {
                let lines: Vec<String> = rows
                    .iter()
                    .map(|(_, n, on)| format!("  {n}: {}", if *on { "online" } else { "offline" }))
                    .collect();
                tell_player(
                    w,
                    player,
                    &format!("Friends on other worlds:\n{}", lines.join("\n")),
                );
            }
        }
        ["link"] => persons::link(w, player),
        ["unlink"] => persons::unlink(w, player),
        ["alts"] => persons::alts(w, player),
        ["alts", v] if v.eq_ignore_ascii_case("shown") => persons::set_alts(w, player, true),
        ["alts", v] if v.eq_ignore_ascii_case("hidden") => persons::set_alts(w, player, false),
        ["alts", name @ ..] => persons::alts_of(w, player, &name.join(" ")),
        ["friends", "add", name @ ..] if !name.is_empty() => {
            // What `@friends add Name@world` does.
            if !social::add_friend(w, player, &name.join(" ")) {
                tell_player(
                    w,
                    player,
                    "/dn friends add Name@world: a character on another world.",
                );
            }
        }
        ["friends", "remove", name @ ..] if !name.is_empty() => {
            // What removing the friend's row in the friends panel does.
            let listed = Address::parse(&name.join(" ")).ok().and_then(|a| {
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
        ["worlds"] => {
            // The hub names the worlds: each name passes the text rules, and a slug that is not
            // a network slug is not shown at all.
            let up: Vec<String> = w
                .federation
                .worlds
                .values()
                .filter(|i| {
                    i.online
                        && i.slug != w.federation.settings.slug
                        && dereth_network_protocol::address::is_network_slug(&i.slug)
                })
                .map(|i| {
                    let name = dereth_network_protocol::text::sanitize_line(&i.name);
                    let name: String = name.chars().take(80).collect();
                    format!("  {} ({})", i.slug, name)
                })
                .collect();
            if up.is_empty() {
                tell_player(w, player, "No other world is on the network now.");
            } else {
                tell_player(
                    w,
                    player,
                    &format!("On the network now:\n{}", up.join("\n")),
                );
            }
        }
        ["squelch", world] | ["unsquelch", world] => {
            let world = world.to_ascii_lowercase();
            if !dereth_network_protocol::address::is_slug(&world)
                || world == w.federation.settings.slug
            {
                tell_player(
                    w,
                    player,
                    "/dn squelch <world>: a world on the network, by its short name.",
                );
                return;
            }
            let add = words[0] == "squelch";
            let held = w.federation.store.characters.get(&character).map(|r| {
                (
                    r.squelched_worlds.contains(&world),
                    r.squelched_worlds.len(),
                )
            });
            if add && held.is_some_and(|(already, n)| !already && n >= super::MAX_SQUELCHED_WORLDS)
            {
                tell_player(
                    w,
                    player,
                    &format!(
                        "You squelch {} worlds already; unsquelch one first.",
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
                (false, Some((true, _))) => {
                    record_mut(w, character).squelched_worlds.remove(&world)
                }
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
        ["channel", rest @ ..] => channel(w, player, rest),
        ["global", rest @ ..] => global(w, player, &rest.join(" ")),
        _ => tell_player(w, player, HELP),
    }
}

/// `/dng <text>`, and `/dn global <text>`, for `player`.
pub fn global(w: &mut World, player: ObjectGuid, text: &str) {
    if !enabled(w) {
        tell_player(w, player, "This world is not on the Dereth Network.");
        return;
    }
    super::rooms::say_global(w, player, text);
}

fn status(w: &mut World, player: ObjectGuid) {
    let rec = w
        .federation
        .store
        .characters
        .get(&player.full())
        .cloned()
        .unwrap_or_default();
    let link = match w.federation.link {
        LinkState::Up => "connected",
        LinkState::Greeting => "connecting",
        LinkState::Down => "not connected",
    };
    let s = rec.settings;
    let who = own_address(w, player)
        .map(|a| a.to_string())
        .unwrap_or_default();
    let linked = match persons::state(w, player.full()) {
        Some(st) if st.linked => " This character is linked to a Discord account.",
        _ => "",
    };
    let line = if s.on {
        format!(
            "You are on the Dereth Network as {who}. Visible: {}. Reachable: {}. Listed: {}. {} friend(s) on other worlds. The hub is {link}.{linked}",
            s.visible,
            s.reachable,
            s.listed,
            rec.friends.len()
        )
    } else {
        format!("You are not on the Dereth Network. Type /dn on to join it as {who}. The hub is {link}.")
    };
    tell_player(w, player, &line);
}

fn channel(w: &mut World, player: ObjectGuid, words: &[&str]) {
    if !super::is_on(w, player.full()) {
        tell_player(
            w,
            player,
            "You are not on the Dereth Network. Type /dn on to join it.",
        );
        return;
    }
    let (op, name, who) = match words {
        ["create", n] => (ChannelOp::Create, *n, None),
        ["join", n] => (ChannelOp::Join, *n, None),
        ["leave", n] => (ChannelOp::Leave, *n, None),
        ["members", n] => (ChannelOp::Members, *n, None),
        ["list"] => (ChannelOp::List, "list", None),
        ["invite", n, rest @ ..] if !rest.is_empty() => match Address::parse(&rest.join(" ")) {
            Ok(a) if !a.is_channel() => (ChannelOp::Invite, *n, Some(a)),
            _ => {
                tell_player(w, player, "/dn channel invite <channel> <Name@world>");
                return;
            }
        },
        _ => {
            tell_player(w, player, HELP);
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
    let address = format!("{channel}@channel");
    let line = match (outcome, p.op) {
        (ChannelOutcome::Ok, ChannelOp::List) if channels.is_empty() => "You belong to no channel.".to_owned(),
        (ChannelOutcome::Ok, ChannelOp::List) => format!(
            "Your channels: {}.",
            channels
                .iter()
                .map(|c| format!("{c}@channel"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (ChannelOutcome::Ok, ChannelOp::Members) => format!(
            "{address}: {}.",
            members
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (ChannelOutcome::Ok, ChannelOp::Create) => format!(
            "You have made the channel {address}. Invite with /dn channel invite {channel} Name@world."
        ),
        (ChannelOutcome::Ok, ChannelOp::Invite) => format!(
            "{} is invited to {address}.",
            p.who
                .as_ref()
                .map_or_else(|| "That character".to_owned(), ToString::to_string)
        ),
        (ChannelOutcome::Ok, ChannelOp::Join) => {
            format!("You have joined {address}. Tell the channel with @tell {address}, text.")
        }
        (ChannelOutcome::Ok, ChannelOp::Leave) => format!("You are not in {address} any more."),
        (ChannelOutcome::Exists, _) => format!("{address} already exists."),
        (ChannelOutcome::NoChannel, _) => format!("There is no channel {address}."),
        (ChannelOutcome::NotMember, _) => format!("You are not a member of {address}."),
        (ChannelOutcome::NotInvited, _) => format!("You have not been invited to {address}."),
        (ChannelOutcome::AlreadyMember, _) => {
            format!("That character is already a member of {address}.")
        }
        (ChannelOutcome::Refused, _) => format!("The network refused that for {address}."),
    };
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

/// An invitation for one of this world's characters. It is shown as a first tell from the
/// inviter would be: only to a character that inviter may reach, never past a squelch of the
/// inviter, of its world or of the channel, or the operator's blocks, and held to the pace a tell
/// from the inviter's world to that character is.
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
        || !super::tells::reachable(w, character, by, true)
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
            "{by_name} invites you to the channel {channel}@channel. Type /dn channel join {channel} to join it."
        ),
    );
}

/// `/dn admin <args>`: the operator's blocks.
pub fn admin(w: &mut World, args: &[String]) -> String {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    if !enabled(w) && matches!(words.as_slice(), ["block" | "unblock", _] | ["blocks"]) {
        // With the network off nothing is linked or saved: a block would be lost at the restart.
        return "This world is not on the Dereth Network.".to_owned();
    }
    match words.as_slice() {
        ["block", who] | ["unblock", who] => {
            let key = match Address::parse(who) {
                Ok(a) => a.key(),
                Err(_) if dereth_network_protocol::address::is_slug(&who.to_ascii_lowercase()) => {
                    who.to_ascii_lowercase()
                }
                Err(_) => return "/dn admin block|unblock <world> or <Name@world>".to_owned(),
            };
            let add = words[0] == "block";
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
        ["blocks"] => {
            let b = &w.federation.store.blocked;
            if b.is_empty() {
                "Nothing is blocked.".to_owned()
            } else {
                format!(
                    "Blocked: {}.",
                    b.iter().cloned().collect::<Vec<_>>().join(", ")
                )
            }
        }
        [] | ["status"] => {
            let f = &w.federation;
            format!(
                "Dereth Network: {} as {:?}; the hub is {:?}; {} world(s) online; {} queued; global room: {:?}.",
                if enabled(w) { "on" } else { "off" },
                f.settings.slug,
                f.link,
                f.worlds.values().filter(|i| i.online).count(),
                f.queued(),
                f.settings.global_room,
            )
        }
        _ => "/dn admin status | blocks | block <world or Name@world> | unblock <...>".to_owned(),
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
        _ => return "/dn verify <token>: the token the site gave you (dnv_...).".to_owned(),
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
