//! The identifiers the retail client asks for: the class it creates, and the ten interfaces it
//! queries the object for, in the order it queries them.

use core::fmt;

/// A COM identifier, laid out as Windows lays it out.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl Guid {
    /// An identifier from its four parts.
    #[must_use]
    pub const fn new(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self {
        Self {
            data1,
            data2,
            data3,
            data4,
        }
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.data4;
        write!(
            f,
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            self.data1, self.data2, self.data3, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
        )
    }
}

/// The tail the client's friends interfaces share but two.
const FRIENDS_TAIL: [u8; 8] = [0x81, 0x57, 0xCD, 0x07, 0xA1, 0x38, 0xB4, 0x9F];

/// The class the client asks the library's class factory for.
pub const CLASS: Guid = Guid::new(
    0x7663_3332,
    0x52A2,
    0x4460,
    [0xB0, 0x89, 0x35, 0x41, 0xA8, 0x44, 0x3C, 0x9A],
);

/// `IUnknown`.
pub const UNKNOWN: Guid = Guid::new(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// `IClassFactory`.
pub const CLASS_FACTORY: Guid = Guid::new(1, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// The library's ten interfaces, in the order the client queries them: the first is the one it
/// creates the object for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// The session: set up, connect, disconnect, and the pump that hands back what came.
    Connection,
    /// Accounts: never called.
    Accounts,
    /// News: never called.
    News,
    /// Tells (and rooms, which the client never uses).
    Chat,
    /// The friends and ignore lists and the player's own state.
    Friends,
    /// Hosting a game: never called.
    Hosting,
    /// The match browser: never called.
    Browser,
    /// Personas: never called.
    Personas,
    /// The login ticket.
    Ticket,
    /// Help calls.
    Support,
}

impl Face {
    /// Every interface, in the order the client queries them.
    pub const ALL: [Self; 10] = [
        Self::Connection,
        Self::Accounts,
        Self::News,
        Self::Chat,
        Self::Friends,
        Self::Hosting,
        Self::Browser,
        Self::Personas,
        Self::Ticket,
        Self::Support,
    ];

    /// The interface's identifier.
    #[must_use]
    pub const fn id(self) -> Guid {
        match self {
            Self::Connection => Guid::new(0xEEEE_EE21, 0x85EA, 0x40F7, FRIENDS_TAIL),
            Self::Accounts => Guid::new(0xEEEE_EE23, 0x85EA, 0x40F7, FRIENDS_TAIL),
            Self::News => Guid::new(0xEEEE_EE22, 0x85EA, 0x40F7, FRIENDS_TAIL),
            Self::Chat => Guid::new(0xEEEE_EE25, 0x85EA, 0x40F7, FRIENDS_TAIL),
            Self::Friends => Guid::new(0xEEEE_EE24, 0x85EA, 0x40F7, FRIENDS_TAIL),
            Self::Hosting => Guid::new(0xEEEE_EE26, 0x85EA, 0x40F7, FRIENDS_TAIL),
            Self::Browser => Guid::new(0xEEEE_EE27, 0x85EA, 0x40F7, FRIENDS_TAIL),
            Self::Personas => Guid::new(
                0xEB60_BA8A,
                0x54A6,
                0x4A6D,
                [0x94, 0x32, 0x83, 0x37, 0xDF, 0xFF, 0x76, 0x12],
            ),
            Self::Ticket => Guid::new(
                0xC4EF_31F6,
                0x6D0A,
                0x4E84,
                [0xA6, 0x7B, 0xCF, 0x97, 0xA2, 0x2C, 0x57, 0xA2],
            ),
            Self::Support => Guid::new(0xEEEE_EE29, 0x85EA, 0x40F7, FRIENDS_TAIL),
        }
    }

    /// The interface's place in [`Face::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The interface an identifier names; `IUnknown` names the first.
    #[must_use]
    pub fn of(id: &Guid) -> Option<Self> {
        if *id == UNKNOWN {
            return Some(Self::Connection);
        }
        Self::ALL.into_iter().find(|f| f.id() == *id)
    }

    /// How many entries its table has, the three every interface starts with included: as many as
    /// the original library's, so that no entry the client could reach is missing.
    #[must_use]
    pub const fn slots(self) -> usize {
        match self {
            Self::Connection => 9,
            Self::Accounts => 10,
            Self::News => 4,
            Self::Chat => 23,
            Self::Friends => 13,
            Self::Hosting => 14,
            Self::Browser => 7,
            Self::Personas => 8,
            Self::Ticket => 11,
            Self::Support => 5,
        }
    }
}
