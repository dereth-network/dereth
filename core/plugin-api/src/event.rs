//! Events: what a plugin hears. Each is named `domain.event` and carries one argument, a map.
//!
//! Events are delivered in the order the client applied the messages behind them, at one fixed
//! point of the frame, so a replayed recording delivers the same events in the same order.

use crate::value::Value;

/// One event.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub name: String,
    pub data: Value,
}

impl Event {
    #[must_use]
    pub fn new(name: &str, data: Value) -> Self {
        Self {
            name: name.to_owned(),
            data,
        }
    }
}

/// The events of API 0.6, with the fields each one's argument carries.
///
/// The spec's event table (`docs/plugins/events.md`) is this list; a test holds the two together.
pub const EVENTS: &[(&str, &str)] = &[
    (
        "session.pregame",
        "characters (a list of { id, name }), world, account",
    ),
    ("session.login", "character (name), id"),
    ("session.login_failed", "code, reason"),
    ("session.enter_world", "id"),
    ("session.portal_enter", ""),
    ("session.portal_exit", ""),
    ("session.logout", ""),
    ("session.disconnect", ""),
    (
        "session.world_policy",
        "published, rules (each domain and session to its rule)",
    ),
    ("world.object_created", "id, name, class"),
    (
        "world.object_changed",
        "id, changed (the keys that changed, among name, stack, value, burden, container, \
         wielder, icon, slot, uses, flags, appraisal, health)",
    ),
    (
        "world.object_moved",
        "id, position, moved (metres since it was last said to move)",
    ),
    ("world.object_removed", "id"),
    (
        "world.selection_changed",
        "id (nil when nothing is selected), previous",
    ),
    (
        "character.vital",
        "vital (health, stamina, mana), current, max, previous",
    ),
    ("character.xp", "total, unassigned, gained"),
    ("character.luminance", "earned, max, gained"),
    (
        "character.death",
        "deaths (the character's count of deaths)",
    ),
    (
        "character.enchantment_added",
        "spell, name, remaining (seconds; nil when permanent)",
    ),
    ("character.enchantment_removed", "spell, name"),
    (
        "character.fellowship",
        "change (joined, left, or changed: a fellow came or went, or the leader or name changed), \
         name, members (their ids)",
    ),
    (
        "inventory.container_opened",
        "id (the corpse, chest or other container opened)",
    ),
    ("inventory.container_closed", "id"),
    ("inventory.item_added", "id, container"),
    ("inventory.item_removed", "id"),
    ("inventory.item_moved", "id, container, previous"),
    ("items.appraised", "id"),
    ("commerce.vendor_opened", "id"),
    ("commerce.vendor_closed", "id"),
    ("commerce.trade_opened", "partner"),
    ("commerce.trade_changed", "partner"),
    ("commerce.trade_closed", ""),
    ("chat.line", "text, kind, window, prefix"),
    ("ui.clicked", "window, id"),
    ("ui.changed", "window, id, value"),
    ("ui.selected", "window, id, rows"),
    ("ui.closed", "window"),
    ("ui.opened", "window"),
    ("ui.resized", "window, width, height"),
    (
        "map.clicked",
        "position ({ ns, ew }: the place under the pointer), button (left, right or middle), \
         modifiers ({ ctrl, shift, alt })",
    ),
    ("action.completed", "id, action, value"),
    (
        "service.call",
        "id, from (the calling plugin), service, args, gesture (whether the call followed the \
         player's gesture)",
    ),
    ("action.failed", "id, action, reason"),
    (
        "nav.journey_planned",
        "id (the journey's promise), legs, seconds, reason (why it was planned again, or nil)",
    ),
    (
        "nav.journey_leg",
        "id, index (from 1), kind (walk, door, portal, recall or hand_back), to, and by kind path, \
         object, name, class, from, source, recall, item, token",
    ),
    (
        "nav.journey_learned",
        "id, kind (portal, refusal or recall), name, to, reason, recall",
    ),
    (
        "peers.message",
        "from (the sending client's character), channel, data",
    ),
    ("plugin.loaded", ""),
    (
        "context.begin",
        "context (now active in), previous (the context before, or nil)",
    ),
    ("context.end", "context (no longer active in), next"),
];

/// The events a plugin hears whether or not it is active: its own life, its contexts beginning
/// and ending, and the session's comings and goings. Every other event reaches only a plugin
/// active in the context the session is in.
pub const LIFECYCLE: &[&str] = &[
    "plugin.loaded",
    "context.begin",
    "context.end",
    "session.pregame",
    "session.login",
    "session.login_failed",
    "session.logout",
    "session.disconnect",
    "session.world_policy",
];

/// The version of the message stream (`messages.<name>` events), which follows the protocol's
/// decoders rather than the API: it rises when a decoded message's fields change.
pub const MESSAGES_VERSION: u32 = 1;

/// The prefix of the message stream's events: `messages.<name>` for one message by its name in
/// the master opcode table, `messages.any` for every one. Which names exist is the host's to say.
pub const MESSAGES_PREFIX: &str = "messages.";

/// Whether `name` is an event of the current API: one of [`EVENTS`], or a message stream event.
#[must_use]
pub fn is_known(name: &str) -> bool {
    EVENTS.iter().any(|(n, _)| *n == name)
        || name
            .strip_prefix(MESSAGES_PREFIX)
            .is_some_and(|rest| !rest.is_empty())
}
