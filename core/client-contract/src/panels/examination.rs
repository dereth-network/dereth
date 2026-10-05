//! Property identifiers shared by the appraisal view and its producers.

/// `GearMaxHealth`, the vitality sentence's property; it is read after the thirteen terms and
/// held in the slot after them.
pub const GEAR_MAX_HEALTH: u32 = 0x17B;

/// `PropertyInt.PortalBitmask` — the literal the examine block pushes, ACE's 111.
pub const PORTAL_BITMASK: u32 = 0x6F;

/// The three presence-gated integer keys at the head of the item-examine description block.
pub const LIFESPAN: u32 = 0x10B;
pub const CREATION_TIMESTAMP: u32 = 0x62;
pub const REMAINING_LIFESPAN: u32 = 0x10C;
/// The client's fallback string key.
pub const SHORT_DESC: u32 = 0x0F;
