//! The games a plugin can be written for, and each one's part of the interface: the host
//! capabilities, the events and the host operations its hosts offer, and the WIT package its
//! compiled plugins target.
//!
//! The types of the interface (manifests, permissions, policy, the UI tree, values, the engine
//! boundary) are the same whatever the game. What a plugin reads of the game, hears from it and
//! asks of it is the game's own. Dereth's is the whole of [`schema::OPS`], [`event::EVENTS`] and
//! [`HostCapability::ALL`]; Auberean's is so far the part of them that names no game's content
//! ([`SHARED_NAMESPACES`], [`SHARED_EVENT_DOMAINS`], [`AUBEREAN_CAPABILITIES`]).

use std::fmt;

use crate::capability::HostCapability;
use crate::event;
use crate::schema::{self, Op};

/// A game, as a manifest's `game` key names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Game {
    /// Asheron's Call: the game a manifest is for when it names none.
    #[default]
    Dereth,
    /// Asheron's Call 2.
    Auberean,
}

/// The namespaces of host operations that name no game's content: every game's hosts offer them.
pub const SHARED_NAMESPACES: &[&str] = &[
    "console",
    "events",
    "chat",
    "commands",
    "task",
    "store",
    "files",
    "act",
    "session",
    "peers",
    "services",
    "policy",
    "permissions",
    "ui",
    "host",
    "time",
];

/// The domains of events that name no game's content: every game's hosts deliver them.
pub const SHARED_EVENT_DOMAINS: &[&str] = &[
    "session", "chat", "ui", "action", "service", "peers", "plugin", "context",
];

/// The host capabilities an Auberean host can offer.
pub const AUBEREAN_CAPABILITIES: &[HostCapability] = &[
    HostCapability::Core,
    HostCapability::UiWindow,
    HostCapability::UiHud,
    HostCapability::SessionPregame,
    HostCapability::PeersLocal,
    HostCapability::NetHttp,
    HostCapability::Messages,
];

/// The namespace of an operation's or an event's name: what comes before its first dot.
fn domain(name: &str) -> &str {
    name.split_once('.').map_or(name, |(d, _)| d)
}

impl Game {
    pub const ALL: [Self; 2] = [Self::Dereth, Self::Auberean];

    /// The game's name as a manifest spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dereth => "dereth",
            Self::Auberean => "auberean",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.name() == s)
    }

    /// The game's name as a player reads it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Dereth => "Dereth",
            Self::Auberean => "Auberean",
        }
    }

    /// The WIT package its compiled plugins' interface is published as; its version is the API
    /// version ([`schema::WIT_VERSION`]).
    #[must_use]
    pub const fn wit_package(self) -> &'static str {
        match self {
            Self::Dereth => schema::WIT_PACKAGE,
            Self::Auberean => "auberean:plugin",
        }
    }

    /// The host capabilities its hosts can offer.
    #[must_use]
    pub fn capabilities(self) -> &'static [HostCapability] {
        match self {
            Self::Dereth => &HostCapability::ALL,
            Self::Auberean => AUBEREAN_CAPABILITIES,
        }
    }

    /// Whether `op` is one of its host operations.
    #[must_use]
    pub fn offers(self, op: &Op) -> bool {
        match self {
            Self::Dereth => true,
            Self::Auberean => SHARED_NAMESPACES.contains(&domain(op.name)),
        }
    }

    /// Its host operations, in the order its WIT interface lists them.
    pub fn ops(self) -> impl Iterator<Item = &'static Op> {
        schema::OPS.iter().filter(move |op| self.offers(op))
    }

    /// Its events, with the fields each one's argument carries, in the order its spec lists them.
    pub fn events(self) -> impl Iterator<Item = &'static (&'static str, &'static str)> {
        event::EVENTS.iter().filter(move |(name, _)| match self {
            Self::Dereth => true,
            Self::Auberean => SHARED_EVENT_DOMAINS.contains(&domain(name)),
        })
    }

    /// Why a host of this game does not run a plugin written for `plugin`; `None` when it is
    /// written for this game.
    #[must_use]
    pub fn refuses(self, plugin: Self) -> Option<String> {
        (plugin != self).then(|| {
            format!(
                "it is a plugin for {}, not {}",
                plugin.title(),
                self.title()
            )
        })
    }
}

impl fmt::Display for Game {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the interface's per-game tables as data; the conformance suite pins
    //! Dereth's, and the hosts' tests the refusal).

    use super::{Game, SHARED_EVENT_DOMAINS, SHARED_NAMESPACES};
    use crate::capability::HostCapability;
    use crate::event::EVENTS;
    use crate::schema::OPS;

    #[test]
    fn dereths_part_is_the_whole_interface_as_it_was() {
        assert_eq!(Game::Dereth.ops().count(), OPS.len());
        assert!(Game::Dereth.ops().zip(OPS).all(|(a, b)| a == b));
        assert_eq!(Game::Dereth.events().count(), EVENTS.len());
        assert_eq!(Game::Dereth.capabilities(), HostCapability::ALL);
        assert_eq!(Game::Dereth.wit_package(), "dereth:plugin");
        assert_eq!(Game::default(), Game::Dereth);
    }

    #[test]
    fn auberean_has_only_the_parts_that_name_no_games_content() {
        let ops: Vec<&str> = Game::Auberean.ops().map(|o| o.name).collect();
        assert!(ops.contains(&"task.timer") && ops.contains(&"ui.window"));
        assert!(ops.contains(&"act.request") && ops.contains(&"store.get"));
        for dereths_own in ["object.get", "magic.spell", "nav.journey", "retail_ui.set"] {
            assert!(!ops.contains(&dereths_own), "{dereths_own}");
        }
        for op in Game::Auberean.ops() {
            let ns = op.name.split('.').next().unwrap_or_default();
            assert!(SHARED_NAMESPACES.contains(&ns), "{}", op.name);
        }
        let events: Vec<&str> = Game::Auberean.events().map(|(n, _)| *n).collect();
        assert!(events.contains(&"plugin.loaded") && events.contains(&"chat.line"));
        assert!(events.contains(&"action.completed") && events.contains(&"ui.clicked"));
        for dereths_own in ["character.luminance", "items.appraised", "nav.journey_leg"] {
            assert!(!events.contains(&dereths_own), "{dereths_own}");
        }
        for e in &events {
            let d = e.split('.').next().unwrap_or_default();
            assert!(SHARED_EVENT_DOMAINS.contains(&d), "{e}");
        }
        assert!(Game::Auberean
            .capabilities()
            .iter()
            .all(|c| Game::Dereth.capabilities().contains(c)));
        assert!(!Game::Auberean
            .capabilities()
            .contains(&HostCapability::RetailUi));
        assert_eq!(Game::Auberean.wit_package(), "auberean:plugin");
    }

    #[test]
    fn a_game_round_trips_through_its_name_and_refuses_the_others_plugins() {
        for g in Game::ALL {
            assert_eq!(Game::parse(g.name()), Some(g));
            assert_eq!(g.refuses(g), None);
        }
        assert_eq!(Game::parse("Dereth"), None);
        assert_eq!(
            Game::Dereth.refuses(Game::Auberean).as_deref(),
            Some("it is a plugin for Auberean, not Dereth")
        );
        assert_eq!(
            Game::Auberean.refuses(Game::Dereth).as_deref(),
            Some("it is a plugin for Dereth, not Auberean")
        );
    }
}
