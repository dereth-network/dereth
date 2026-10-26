//! Permissions: the names for what a plugin does. A manifest names the ones its plugin uses, with
//! a reason for each, so a player can see what it does. Every plugin may do what each covers; only
//! HTTPS needs its manifest to name `net.http` and the domains it reaches, and a world's policy
//! bounds automation.

use std::fmt;

/// The automation domains. A world's policy says, per domain, whether a plugin may act in it with
/// no gesture behind the action, only with one, or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AutoDomain {
    Loot,
    Buff,
    Heal,
    Combat,
    Nav,
    Vendor,
    Craft,
    Trade,
    Social,
    /// Beneficial spells cast on another character: a buffbot's buffs, a helper's protections.
    /// A heal asked for as one stays `Heal`, and a spell that harms stays `Combat`.
    Support,
}

impl AutoDomain {
    /// Every domain, in the spec's order.
    pub const ALL: [Self; 10] = [
        Self::Loot,
        Self::Buff,
        Self::Heal,
        Self::Combat,
        Self::Nav,
        Self::Vendor,
        Self::Craft,
        Self::Trade,
        Self::Social,
        Self::Support,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Loot => "loot",
            Self::Buff => "buff",
            Self::Heal => "heal",
            Self::Combat => "combat",
            Self::Nav => "nav",
            Self::Vendor => "vendor",
            Self::Craft => "craft",
            Self::Trade => "trade",
            Self::Social => "social",
            Self::Support => "support",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == s)
    }
}

/// One permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    /// Game state, events and the plugin's own UI. Always implied.
    Read,
    /// The text of tells to and from the player.
    ReadChatPrivate,
    /// The decoded message stream.
    Messages,
    /// Local text into the chat window, not sent anywhere.
    ChatPrint,
    /// Speech, tells, channels, emotes and commands sent to the server.
    ChatSend,
    /// Hiding or restyling incoming lines, and rewriting lines the player types.
    ChatFilter,
    /// Actions that follow a player gesture on the plugin's own surface.
    ActInteractive,
    /// Actions with no gesture behind them, in one domain.
    Auto(AutoDomain),
    /// Selecting a character and logging in or out without the player.
    SessionControl,
    /// Talking to the other clients on the same machine.
    PeersLocal,
    /// HTTPS to the domains the manifest lists.
    NetHttp,
    /// Calling the services other plugins in this client offer.
    PluginsCall,
    /// Reading a file the player picks.
    FilesImport,
    /// Moving and hiding the retail interface's panels.
    RetailUi,
}

/// What a plugin needs to do what a permission covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Nothing: every plugin may.
    Always,
    /// The world's policy: it says whether the domain is allowed with a gesture, without one, or
    /// not at all (a world that publishes none allows it).
    WorldPolicy,
    /// The player: each file is picked by the player when the plugin asks.
    PlayerPicks,
    /// Its manifest: `net.http` named, and the domains each request reaches listed.
    Declared,
}

impl Availability {
    /// What it needs, in words, for the generated reference.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Always => "every plugin",
            Self::WorldPolicy => "every plugin, as the world's policy allows",
            Self::PlayerPicks => "every plugin, a file the player picks each time",
            Self::Declared => "a plugin whose manifest names it and lists the domains",
        }
    }
}

impl Permission {
    /// Every permission of the current API with no domain, and then each automation domain.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut v = vec![
            Self::Read,
            Self::ReadChatPrivate,
            Self::Messages,
            Self::ChatPrint,
            Self::ChatSend,
            Self::ChatFilter,
            Self::ActInteractive,
        ];
        v.extend(AutoDomain::ALL.into_iter().map(Self::Auto));
        v.extend([
            Self::SessionControl,
            Self::PeersLocal,
            Self::NetHttp,
            Self::PluginsCall,
            Self::FilesImport,
            Self::RetailUi,
        ]);
        v
    }

    /// The name a manifest spells it with.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Auto(d) => format!("auto.{}", d.name()),
            other => other.fixed_name().to_owned(),
        }
    }

    fn fixed_name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadChatPrivate => "read.chat.private",
            Self::Messages => "messages",
            Self::ChatPrint => "chat.print",
            Self::ChatSend => "chat.send",
            Self::ChatFilter => "chat.filter",
            Self::ActInteractive => "act.interactive",
            Self::Auto(_) => "auto",
            Self::SessionControl => "session.control",
            Self::PeersLocal => "peers.local",
            Self::NetHttp => "net.http",
            Self::PluginsCall => "plugins.call",
            Self::FilesImport => "files.import",
            Self::RetailUi => "retail_ui",
        }
    }

    /// Parse a manifest's spelling. An unknown name is `None`: the manifest is then refused, so a
    /// typo cannot go unnoticed.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(d) = s.strip_prefix("auto.") {
            return AutoDomain::parse(d).map(Self::Auto);
        }
        Self::all()
            .into_iter()
            .find(|p| !matches!(p, Self::Auto(_)) && p.fixed_name() == s)
    }

    /// What it covers, in a sentence, for the generated reference.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Read => "Game state, events and the plugin's own UI. Always implied.",
            Self::ReadChatPrivate => "The text of tells to and from the player.",
            Self::Messages => "The decoded message stream.",
            Self::ChatPrint => "Local text into the chat window, not sent anywhere.",
            Self::ChatSend => "Speech, tells, channels, emotes and commands sent to the server.",
            Self::ChatFilter => {
                "Hiding or restyling incoming lines, and rewriting lines the player types."
            }
            Self::ActInteractive => {
                "Actions that follow a player gesture on the plugin's own surface."
            }
            Self::Auto(_) => "Actions with no gesture behind them, in one domain.",
            Self::SessionControl => {
                "Selecting a character and logging in or out without the player."
            }
            Self::PeersLocal => "Talking to the other clients on the same machine.",
            Self::NetHttp => "HTTPS to the domains the manifest lists.",
            Self::PluginsCall => "Calling the services other plugins in this client offer.",
            Self::FilesImport => "Reading a file the player picks.",
            Self::RetailUi => "Moving and hiding the retail interface's panels.",
        }
    }

    /// What a plugin needs to do what it covers.
    #[must_use]
    pub const fn availability(self) -> Availability {
        match self {
            Self::Auto(_) | Self::SessionControl => Availability::WorldPolicy,
            Self::FilesImport => Availability::PlayerPicks,
            Self::NetHttp => Availability::Declared,
            _ => Availability::Always,
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::{Availability, Permission};

    #[test]
    fn every_permission_round_trips_through_its_name() {
        for p in Permission::all() {
            assert_eq!(Permission::parse(&p.name()), Some(p), "{p}");
        }
    }

    #[test]
    fn unknown_and_misspelled_permissions_do_not_parse() {
        for bad in [
            "",
            "auto",
            "auto.",
            "auto.flying",
            "chat",
            "chat.sned",
            "Read",
        ] {
            assert_eq!(Permission::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn only_https_needs_its_manifest_to_name_it() {
        let declared: Vec<_> = Permission::all()
            .into_iter()
            .filter(|p| p.availability() == Availability::Declared)
            .collect();
        assert_eq!(declared, [Permission::NetHttp]);
    }
}
