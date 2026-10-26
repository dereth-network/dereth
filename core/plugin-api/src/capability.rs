//! Host capabilities: what a host can do. A plugin lists the ones it needs and checks the optional
//! ones at run time.

use std::fmt;

/// One host capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostCapability {
    /// Reads, events, chat, actions, the store and tasks. Every host has it.
    Core,
    /// Declared windows.
    UiWindow,
    /// Always-on-screen declared HUDs.
    UiHud,
    /// Icons from the game's data by id.
    UiIcons,
    /// Markers and labels on objects or positions in the world.
    OverlayWorld,
    /// A direction arrow toward a target.
    OverlayArrow,
    /// Extra radar blips.
    OverlayRadar,
    /// Points and lines on the world or dungeon map.
    OverlayMap,
    /// Where the player presses the game's world map, heard as `map.clicked`.
    MapClicks,
    /// The client's own move-to machine.
    NavMoveto,
    /// Journeys planned and carried out by the client: walking, portals and recalls.
    NavJourney,
    /// The client's walking routes and lines of sight, answered without moving.
    NavPath,
    /// The character list, and logging in and out.
    SessionPregame,
    /// The bus between the clients on one machine.
    PeersLocal,
    /// The plugin's own HTTPS fetches.
    NetHttp,
    /// The read-only decoded message stream.
    Messages,
    /// Moving and hiding the retail interface's panels.
    RetailUi,
    /// The game's own data, read only: landblocks and dungeon cells.
    GameData,
}

impl HostCapability {
    pub const ALL: [Self; 18] = [
        Self::Core,
        Self::UiWindow,
        Self::UiHud,
        Self::UiIcons,
        Self::OverlayWorld,
        Self::OverlayArrow,
        Self::OverlayRadar,
        Self::OverlayMap,
        Self::MapClicks,
        Self::NavMoveto,
        Self::NavJourney,
        Self::NavPath,
        Self::SessionPregame,
        Self::PeersLocal,
        Self::NetHttp,
        Self::Messages,
        Self::RetailUi,
        Self::GameData,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::UiWindow => "ui.window",
            Self::UiHud => "ui.hud",
            Self::UiIcons => "ui.icons",
            Self::OverlayWorld => "overlay.world",
            Self::OverlayArrow => "overlay.arrow",
            Self::OverlayRadar => "overlay.radar",
            Self::OverlayMap => "overlay.map",
            Self::MapClicks => "map.clicks",
            Self::NavMoveto => "nav.moveto",
            Self::NavJourney => "nav.journey",
            Self::NavPath => "nav.path",
            Self::SessionPregame => "session.pregame",
            Self::PeersLocal => "peers.local",
            Self::NetHttp => "net.http",
            Self::Messages => "messages",
            Self::RetailUi => "retail_ui",
            Self::GameData => "game.data",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == s)
    }

    /// What it is, in a sentence, for the generated reference.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Core => "Reads, events, chat, actions, the store and tasks. Every host has it.",
            Self::UiWindow => "Declared windows.",
            Self::UiHud => "Always-on-screen declared HUDs.",
            Self::UiIcons => "Icons from the game's data by id.",
            Self::OverlayWorld => "Markers and labels on objects or positions in the world.",
            Self::OverlayArrow => "A direction arrow toward a target.",
            Self::OverlayRadar => "Extra radar blips.",
            Self::OverlayMap => "Points and lines on the world or dungeon map.",
            Self::MapClicks => {
                "Where the player presses the game's world map, heard as `map.clicked`."
            }
            Self::NavMoveto => "The client's own move-to machine.",
            Self::NavJourney => {
                "Journeys the client plans and carries out: walking round what is in the way, \
                 portals and the character's recalls."
            }
            Self::NavPath => {
                "The client's walking route to a place or an object, round walls and through                  doors, and whether a straight line between two places is clear: answered                  without moving."
            }
            Self::SessionPregame => "The character list, and logging in and out.",
            Self::PeersLocal => "The bus between the clients on one machine.",
            Self::NetHttp => "The plugin's own HTTPS fetches.",
            Self::Messages => "The read-only decoded message stream.",
            Self::RetailUi => "Moving and hiding the retail interface's panels.",
            Self::GameData => "The game's own data, read only: landblocks and dungeon cells.",
        }
    }
}

impl fmt::Display for HostCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::HostCapability;

    #[test]
    fn every_capability_round_trips_through_its_name() {
        for c in HostCapability::ALL {
            assert_eq!(HostCapability::parse(c.name()), Some(c));
        }
        assert_eq!(HostCapability::parse("ui"), None);
    }
}
