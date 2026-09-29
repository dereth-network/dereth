//! Shared payload structures: the things more than one message carries.
//!
//! Sources: `docs/networking/messages/02-world-objects.md` (`ObjDesc`, `PhysicsDesc`,
//! `PublicWeenieDesc`, `AppraisalProfile`), `docs/networking/messages/01-login-and-character.md`
//! (`ACQualities`, `PlayerModule`, `CharacterSet`). The movement packs live in
//! [`crate::movement`] because they are only reachable from that family.

pub mod appraisal;
pub mod objdesc;
pub mod physicsdesc;
pub mod qualities;
pub mod space;
pub mod weeniedesc;

pub use appraisal::{
    AppraisalProfile, ArmorProfile, CreatureAppraisalProfile, HookAppraisalProfile, WeaponProfile,
};
pub use objdesc::{AnimPartChange, ObjDesc, Subpalette, TextureMapChange};
pub use physicsdesc::{PhysicsDesc, PhysicsEventStamp, PhysicsTimestamps};
pub use qualities::{
    AcBaseQualities, AcQualities, Attribute, AttributeCache, Enchantment, EnchantmentRegistry,
    PropertyTables, SecondaryAttribute, Skill, StatMod,
};
pub use space::{Frame, Origin, PositionWire, Quat, Vec3};
pub use weeniedesc::{ContentProfile, InventoryPlacement, PublicWeenieDesc, RestrictionDb};
