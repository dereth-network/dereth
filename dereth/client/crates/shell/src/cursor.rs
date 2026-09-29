//! The cursor-state decision and the dat cursors it chooses from.
//!
//! Sources: the cursor-state branch table, the target-compatibility predicate, the item-use
//! bitfield predicates, the UI manager's cursor setter and per-element override, and the shipped
//! `UICURSOR` mapper.
//!
//! # The shape of it
//!
//! The client has **41** cursors, all of them dat images. They live in `DidMapper 0x2500000F`,
//! which the master mapper reaches as group **6** (`UICURSOR`); the enum keys are `1..=41` with no
//! gaps and the values are `0x06xxxxxx` `RenderSurface` ids. [`update_cursor_state`] picks one of
//! them by **arithmetic on the enum key**, which is only legible once the shipped names are next
//! to it:
//!
//! | branch | enum | shipped name at `+0` / `+1` | hotspot |
//! |---|---|---|---|
//! | busy count `!= 0` | `0xE + h` | `Wait` / `Wait_OverObject` | (0,0) |
//! | `NONE`, non-combat | `1 + h` | `Default` / `Default_OverObject` | (0,0) |
//! | `NONE`, melee or missile | `3 + h` | `Combat` / `Combat_OverObject` | (0,0) |
//! | `NONE`, magic | `5 + h` | `Spellcast` / `Spellcast_OverObject` | (0,0) |
//! | `TargetMode::Examine` | `10 + h` | `Examine` / `Examine_OverObject` | (0,0) |
//! | `TargetMode::Use` | `0xC + h` | `Use` / `Use_OverObject` | **(14,14)** |
//! | `USE_TARGET`, nothing hovered | `0x27` | `TargetedUse` | **(14,14)** |
//! | `USE_TARGET`, legal target | `0x28` | `TargetedUse_OverObject` | **(14,14)** |
//! | `USE_TARGET`, illegal target | `0x29` | `TargetedUse_OverInvlaidObject` | **(14,14)** |
//!
//! `h` records whether the found-object id is nonzero — whether anything is under the pointer. The
//! `_OverObject` pairing is the confirmation that the `+ h` is a hover bit and not a coincidence,
//! and `TargetedUse_OverInvlaidObject` (the typo is retail's) is the confirmation for the last row.
//!
//! **The last two rows are a gameplay affordance, not decoration.** `0x29` minus the
//! target-compatibility answer is `0x28` for a legal target and `0x29` for an
//! illegal one, so retail tells the player *before they click* whether the thing under the pointer
//! can take the spell or item they are aiming. Getting it backwards is invisible to any pixel test
//! and actively misinforms, which is why [`is_target_compatible`] is transcribed arm for arm and
//! tested arm for arm rather than approximated.
//!
//! # Two hotspot facts worth stating separately
//!
//! Every hotspot is `(0,0)` **except** the three `TargetMode::Use`/`USE_TARGET` states, which are
//! `(14,14)` — the middle of a 32x32 cursor, because those cursors are a reticle rather than a
//! pointer. The UI manager's cache compares the **hotspot as well as the did**,
//! which matters because the shipped mapper aliases four pairs of enum keys onto the same image
//! (`Examine`/`Examine_OverObject` are both `0x06004D71`, `Use`/`Use_OverObject` both
//! `0x06004D72`, and two *pairs* of the movement cursors likewise, `0x06004D7F` for keys 25/26 and
//! `0x06004D81` for 28/29).
//!
//! # Why the did-only compare below is safe, and why it must not be widened
//!
//! [`CursorSystem::update_cursor_state`] reproduces the client's "did differs from the current
//! cursor's did" test exactly: it compares the **`DataId` alone** while pushing a hotspot beside
//! it. That is correct only if no
//! two enum keys this client can *choose* resolve to one image at two different hotspots.
//!
//! The cursor alias test asserts this: it drives all
//! **64** input combinations of [`update_cursor_state`], collapses them onto the **15** enum keys
//! the function can produce, resolves each through the shipped `UICURSOR` mapper, and requires
//! every group of keys sharing a `DataId` to share a hotspot. It also pins the four aliased pairs
//! above as literals. **Two** of those four are reachable from [`update_cursor_state`] (Examine
//! and Use); the movement pairs are reached only through the UI manager's
//! per-element override, which carries its own hotspot with the did.
//!
//! **Do not add a hotspot term to the compare.** This rebuild preserves the original assumption:
//! `UICURSOR` is the same shipped data reached by the same two mapper hops on both sides. Widening
//! the comparison would therefore change behavior without fixing an observed mismatch. If the
//! alias-consistency test ever fails, inspect the mapper rows first.

use std::collections::HashMap;

use dereth_primitives::DataId;

/// The master `DidMapper`'s group for `UICURSOR` — second-level mapper `0x2500000F`, 41 entries.
///
/// The master `DidMapper` provides this group. Reached through
/// [`crate::assets::enum_did`], the same seam `UIASSET` (group 7) uses: **never
/// hard-code a cursor's `DataID`**, a DDD patch can move any of them.
pub const UICURSOR_GROUP: u32 = 6;

/// The enum keys [`update_cursor_state`] computes, named from the shipped mapper's `enum_to_name`.
pub mod cursor_enum {
    /// Enum 1 — the cursor the manager starts with.
    pub const DEFAULT: u32 = 1;
    pub const DEFAULT_OVER_OBJECT: u32 = 2;
    pub const COMBAT: u32 = 3;
    pub const SPELLCAST: u32 = 5;
    pub const EXAMINE: u32 = 10;
    pub const USE: u32 = 0x0C;
    pub const WAIT: u32 = 0x0E;
    pub const TARGETED_USE: u32 = 0x27;
    pub const TARGETED_USE_OVER_OBJECT: u32 = 0x28;
    /// The mapper's own spelling of "invalid".
    pub const TARGETED_USE_OVER_INVLAID_OBJECT: u32 = 0x29;
    /// The highest key in the mapper; the keys are `1..=41` with no gaps.
    pub const LAST: u32 = 41;
}

/// The hotspot the three targeting states use. Every other state is `(0, 0)`.
pub const TARGETING_HOTSPOT: (i32, i32) = (14, 14);

/// The client's four-value targeting mode.
///
/// All four modes are produced by Interaction. UseTarget retains the source separately from the
/// current selection so compatibility can be evaluated against both objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetMode {
    #[default]
    None = 0,
    Use = 1,
    Examine = 2,
    UseTarget = 3,
}

impl From<crate::interaction::TargetMode> for TargetMode {
    fn from(m: crate::interaction::TargetMode) -> Self {
        match m {
            crate::interaction::TargetMode::None => Self::None,
            crate::interaction::TargetMode::Use => Self::Use,
            crate::interaction::TargetMode::Examine => Self::Examine,
            crate::interaction::TargetMode::UseTarget => Self::UseTarget,
        }
    }
}

/// Everything [`update_cursor_state`] reads, as data.
///
/// The function itself reads four globals and calls two singletons. Making them arguments is what
/// lets all nine arms be driven in a table test, which is the only way to check a cursor: it is one
/// small sprite and a frame differential will not catch a wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorInputs {
    /// The busy-count depth (raised and lowered around busy work). Non-zero
    /// beats everything else.
    pub busy: u32,
    /// The current targeting mode.
    pub target_mode: TargetMode,
    /// The active combat mode.
    pub combat_mode: dereth_client_model::combat::CombatMode,
    /// Whether the found-object id is nonzero — the `h` of the table above.
    pub hovering: bool,
    /// Whether the found object is compatible with the targeting object, read **only**
    /// on the `USE_TARGET` + hovering arm. See [`is_target_compatible`].
    pub target_compatible: bool,
}

/// What [`update_cursor_state`] resolved to: an enum key and the hotspot to push it at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorChoice {
    /// The `UICURSOR` enum key, 1..=41.
    pub enum_value: u32,
    /// The hotspot passed to the UI manager's cursor setter.
    pub hot: (i32, i32),
}

/// The UI cursor-state decision, as a pure function.
///
/// With `h` = "the found-object id is nonzero", the client starts from `1 + h` at hotspot (0,0).
/// A non-zero busy count gives `0xE + h` and nothing else is consulted. Otherwise: no target mode
/// gives `3 + h` in melee or missile and `5 + h` in magic; Use gives `0xC + h`; Examine gives
/// `10 + h`; UseTarget gives `0x29` minus the compatibility answer when hovering and `0x27` when
/// not. The Use and UseTarget arms set the (14,14) hotspot.
///
/// Note what the fall-through means: no target mode with **`UNDEF`** or `NONCOMBAT` combat mode
/// keeps the `1 + h` the function opened with, so `Default` is the answer for both. The other
/// arms are exhaustive over [`TargetMode`], so there is no other fall-through.
#[must_use]
pub fn update_cursor_state(i: CursorInputs) -> CursorChoice {
    use dereth_client_model::combat::CombatMode;

    let h = u32::from(i.hovering);
    let zero = (0, 0);
    if i.busy != 0 {
        return CursorChoice {
            enum_value: cursor_enum::WAIT + h,
            hot: zero,
        };
    }
    match i.target_mode {
        TargetMode::None => {
            let base = match i.combat_mode {
                CombatMode::Melee | CombatMode::Missile => cursor_enum::COMBAT,
                CombatMode::Magic => cursor_enum::SPELLCAST,
                CombatMode::Undef | CombatMode::NonCombat => cursor_enum::DEFAULT,
            };
            CursorChoice {
                enum_value: base + h,
                hot: zero,
            }
        }
        TargetMode::Use => CursorChoice {
            enum_value: cursor_enum::USE + h,
            hot: TARGETING_HOTSPOT,
        },
        TargetMode::Examine => CursorChoice {
            enum_value: cursor_enum::EXAMINE + h,
            hot: zero,
        },
        TargetMode::UseTarget => {
            let e = if i.hovering {
                // `0x29` minus the target-compatibility answer.
                cursor_enum::TARGETED_USE_OVER_INVLAID_OBJECT - u32::from(i.target_compatible)
            } else {
                cursor_enum::TARGETED_USE
            };
            CursorChoice {
                enum_value: e,
                hot: TARGETING_HOTSPOT,
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Target compatibility
// ---------------------------------------------------------------------------------------------

/// `ITEM_USEABLE` and the two item-use predicates live in the model crate, which the HUD's view
/// reads them from; re-exported at their historical paths.
pub use dereth_client_runtime::cursor::{item_useable, ItemUses};

/// What the target-compatibility check reads off the two weenies, as data.
///
/// The retained targeting object is the item or spell being aimed, while the *target* is
/// whatever `WorldObjects` found under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TargetCompat {
    /// The targeting object's useability.
    pub source_useability: u32,
    /// The targeting object's target type, an `ITEM_TYPE` mask.
    pub source_target_type: u32,
    /// Whether the targeting object is owned by the player.
    pub source_owned_by_player: bool,
    /// The target's trade state. Only `== 1` matters (the item is on the trade window).
    pub target_trade_state: u32,
    /// Whether the target is owned by the player.
    pub target_owned_by_player: bool,
    /// Whether the target is the player's own object.
    pub target_is_the_player: bool,
    /// The target's item type, an `ITEM_TYPE` mask.
    pub target_item_type: u32,
}

/// Evaluates target compatibility, arm for arm.
///
/// In order, the answer is `false` when:
///
/// 1. either id is zero, or either object is unknown;
/// 2. the source is not the player's and its least-limited source use is contained (it is in
///    someone else's pack) or wielded (someone else is wielding it);
/// 3. the target is on the trade window (trade state 1);
/// 4. the target is not the player's and the source's least-limited target use is contained —
///    unless the target is the player and the source is self-target useable — or wielded;
/// 5. the target is the player and the source is not self-target useable.
///
/// Otherwise it is whether the source's target-type mask intersects the target's item type
/// (`TYPE_SELF` is 0). That last test is the one that decides most cases: the aiming object
/// declares which `ITEM_TYPE`s it may be used on, and the target must be one of them. `TYPE_SELF`
/// is `0`, so `!= TYPE_SELF` is "the masks intersect".
///
/// Two traps worth naming, because both invert the answer:
///
/// * **The trade-state-1 arm falls out of the function**, it does not skip to the return: it
///   answers `false` outright.
/// * **The least-limited target use returns one bit**, so `t & USEABLE_CONTAINED` is `t ==
///   USEABLE_CONTAINED`, and the `else if` on `USEABLE_WIELDED` is genuinely exclusive. Writing it
///   as two independent tests would let a contained-and-wielded item take both arms.
#[must_use]
pub fn is_target_compatible(c: TargetCompat) -> bool {
    let uses = ItemUses(c.source_useability);
    if !c.source_owned_by_player {
        let u = uses.least_limited_source_use();
        if u & item_useable::CONTAINED != 0 {
            return false;
        }
        if u & item_useable::WIELDED != 0 {
            return false;
        }
    }
    if c.target_trade_state == 1 {
        return false;
    }
    if !c.target_owned_by_player {
        let t = uses.least_limited_target_use();
        if t & item_useable::CONTAINED != 0 {
            if !c.target_is_the_player {
                return false;
            }
            if !uses.is_useable_self_target() {
                return false;
            }
        } else if t & item_useable::WIELDED != 0 {
            return false;
        }
    }
    if c.target_is_the_player && !uses.is_useable_self_target() {
        return false;
    }
    c.source_target_type & c.target_item_type != 0
}

/// Gather [`TargetCompat`] off the object tables, then answer.
///
/// The two object-lookup null tests are the `Option`s: a target or targeting object this client
/// has never been told about is not compatible, which is the client's answer too.
#[must_use]
pub fn is_target_compatible_with_targeting_object(
    world: &dereth_client_model::World,
    targeting_object: dereth_primitives::ObjectId,
    target: dereth_primitives::ObjectId,
) -> bool {
    world.target_compatible_with_object(target, targeting_object)
}

// ---------------------------------------------------------------------------------------------
// The runtime half: enum -> DataID -> RenderSurface -> HCURSOR -> SetCursor
// ---------------------------------------------------------------------------------------------

/// What the cursor path did, for the log line and for the tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorStats {
    /// Cursor-state updates. **This is the per-frame wire**: if it does not climb, nothing is
    /// choosing a cursor.
    pub updates: u64,
    /// Times the resolved did differed from the current cursor's did, so the cursor was set.
    pub state_changes: u64,
    /// Pushes that got past the manager's last-cursor did and reached the device path.
    pub device_pushes: u64,
    /// `HCURSOR`s actually built from a dat surface (the cache misses).
    pub icons_built: u64,
    /// Enum keys that did not resolve to a `DataID`, or whose surface would not decode.
    pub failures: u64,
    /// Pushes where `SetCursor` was followed by a `GetCursor()` that agreed -- i.e. the dat-built
    /// `HCURSOR` really is the process's current cursor. Windowed runs only.
    pub device_installs: u64,
}

/// Which cursor image: the dat surface and its hotspot.
pub type CursorKey = (DataId, i32, i32);

/// What putting a built cursor on the window found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorInstall {
    /// There is a window to put it on.
    pub window: bool,
    /// The host holds a cursor built for the key.
    pub icon: bool,
    /// The host reports the cursor current after putting it there.
    pub took: bool,
}

/// The host's cursor images: built from a cursor's icon bits and put on the window.
///
/// The desktop builds a system cursor from the bits and installs it as the window's; a host that
/// cannot set the pointer's image (a window system without a custom-cursor call) builds nothing
/// and installs nothing, and [`PortableCursors`] is that host.
pub trait CursorImages {
    /// Build the cursor for `key` from its icon bits, keeping it for [`Self::install`]; `false`
    /// when the host could not.
    fn build(&mut self, key: CursorKey, bits: &dereth_render::cursor::IconBits) -> bool;
    /// Put the cursor built for `key` on the window. `None` where the host installs no cursors.
    fn install(&mut self, key: CursorKey) -> Option<CursorInstall>;
}

/// Cursor images that are resolved and cached and never installed: the pointer stays the system
/// arrow. A declared gap wherever it is the host's; a software cursor is the way to close it.
#[derive(Debug, Default, Clone, Copy)]
pub struct PortableCursors;

impl CursorImages for PortableCursors {
    fn build(&mut self, _key: CursorKey, _bits: &dereth_render::cursor::IconBits) -> bool {
        true
    }

    fn install(&mut self, _key: CursorKey) -> Option<CursorInstall> {
        None
    }
}

/// Cursor state projected from UI interactions and the device.
///
/// Holds the current cursor's did (the enum-level cache) and which cursor images were built.
/// The last-cursor did (the device-level cache) lives where the client keeps it, on
/// [`dereth_ui::UiSystem`].
pub struct CursorSystem {
    /// `INVALID_DID` at construction.
    current: Option<DataId>,
    /// The two cursor lines go out at most once per
    /// [`dereth_client_runtime::report_gate::REPORT_INTERVAL`]; the cursor itself changes every time.
    report_gate: dereth_client_runtime::report_gate::ReportGate,
    /// Every cursor image built so far, keyed by what the host was given. `false` is a
    /// remembered failure, so a surface that will not decode is not retried every frame.
    // ORDER-OK: a cache, only ever looked up.
    built: HashMap<CursorKey, bool>,
    /// The host's cursor images, which hold the built cursors and the window they go on.
    images: Box<dyn CursorImages>,
    pub stats: CursorStats,
}

impl Default for CursorSystem {
    fn default() -> Self {
        Self::with_images(Box::new(PortableCursors))
    }
}

impl std::fmt::Debug for CursorSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorSystem")
            .field("current", &self.current)
            .field("built", &self.built.len())
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl CursorSystem {
    /// A cursor system whose images are resolved and cached and never installed; `_hwnd` is the
    /// window a host that installs cursors would put them on, which this one does not.
    #[must_use]
    pub fn new(_hwnd: Option<isize>) -> Self {
        Self::default()
    }

    /// A cursor system over the host's cursor images.
    #[must_use]
    pub fn with_images(images: Box<dyn CursorImages>) -> Self {
        Self {
            current: None,
            report_gate: dereth_client_runtime::report_gate::ReportGate::default(),
            built: HashMap::new(),
            images,
            stats: CursorStats::default(),
        }
    }

    /// Returns the currently installed cursor data id for tests and diagnostics.
    #[must_use]
    pub fn current(&self) -> Option<DataId> {
        self.current
    }

    /// Runs the complete cursor-state update.
    ///
    /// It resolves the chosen enum key through group 6 (`UICURSOR`); if the did differs from the
    /// current one it records it and, when there is a UI manager, sets the cursor with the hotspot
    /// and `true`.
    ///
    /// The `true` is what makes this the **default** cursor, i.e. the one the cursor check falls
    /// back to when no element overrides it. Returns the enum key it chose, so a caller can log it.
    /// `ui` represents the optional UI manager: **the whole state machine runs whether or
    /// not there is a manager**, and only setting the cursor is gated on it, exactly as retail
    /// has it. That is not pedantry: it means
    /// the current did still advances on a frame with no UI, so the cursor is not
    /// re-pushed the moment one appears.
    pub fn update_cursor_state(
        &mut self,
        assets: &dyn dereth_primitives::AssetSource,
        ui: Option<&mut dereth_ui::UiSystem>,
        inputs: CursorInputs,
    ) -> CursorChoice {
        self.stats.updates += 1;
        let choice = update_cursor_state(inputs);
        let did = crate::assets::enum_did(assets, UICURSOR_GROUP, choice.enum_value);
        if did.is_none() {
            self.stats.failures += 1;
        }
        if self.current != did {
            self.current = did;
            self.stats.state_changes += 1;
            // A line per change, rate-limited, so a windowed run *says* which cursor it chose
            // rather than leaving it to be inferred from a screenshot that cannot capture the
            // pointer at all.
            if self.report_gate.ready() {
                tracing::debug!(
                    "cursor -> UICURSOR {} at ({}, {}) = {}",
                    choice.enum_value,
                    choice.hot.0,
                    choice.hot.1,
                    did.map_or_else(|| "INVALID_DID".to_string(), |d| format!("{:#010X}", d.0))
                );
            }
            if let Some(ui) = ui {
                // Setting `INVALID_DID` as the default is what the client does when the enum
                // does not resolve: the default is recorded and nothing is pushed.
                // `DataId(0)` is `INVALID_DID`.
                let d = did.unwrap_or(DataId(0));
                ui.set_cursor(d, choice.hot.0, choice.hot.1, true);
            }
        }
        choice
    }

    /// The cursor-update tail loads the image by qualified data id, then sets the cursor from the
    /// image surface and hotspot.
    ///
    /// Drains whatever the manager let through, so it also carries the pushes
    /// [`dereth_ui::UiSystem::check_cursor`] made for a per-element override and the one
    /// the refresh re-made after a device reset. Returns what it applied.
    pub fn apply_pending(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        ui: &mut dereth_ui::UiSystem,
    ) -> Option<(DataId, i32, i32)> {
        let (did, hx, hy) = ui.take_pending_cursor()?;
        self.stats.device_pushes += 1;
        self.install(store, did, hx, hy);
        Some((did, hx, hy))
    }

    /// Build (or reuse) the cursor image for one did+hotspot and put it on the window.
    fn install(&mut self, store: &dereth_dat::RetailDatStore, did: DataId, hx: i32, hy: i32) {
        let key = (did, hx, hy);
        if !self.built.contains_key(&key) {
            let bits = Self::icon_bits(store, did, hx, hy);
            if bits.is_none() {
                self.stats.failures += 1;
            }
            let ok = bits.is_some_and(|b| self.images.build(key, &b));
            if ok {
                self.stats.icons_built += 1;
            }
            self.built.insert(key, ok);
        }
        if let Some(CursorInstall { window, icon, took }) = self.images.install(key) {
            if took {
                self.stats.device_installs += 1;
            }
            // It names each of the three things that can be missing rather than silently doing
            // nothing; a headless run legitimately has no window. Printed on a failure always,
            // and on a success at most once per report interval.
            if !(took && window && icon) || self.report_gate.ready() {
                tracing::debug!(
                    "({hx}, {hy}, {did:#010X}) -- \
                     window: {window}, icon: {icon}, GetCursor() agrees: {took}",
                    did = did.0
                );
            }
        }
    }

    /// Resolves a cursor data id to pixels and then to 32x32 icon bits.
    fn icon_bits(
        store: &dereth_dat::RetailDatStore,
        did: DataId,
        hx: i32,
        hy: i32,
    ) -> Option<dereth_render::cursor::IconBits> {
        let tex = crate::textures::TextureStore::new(store);
        let img = tex.bgra8(did).ok()?;
        dereth_render::cursor::build(
            img.width,
            img.height,
            &img.pixels,
            u32::try_from(hx).unwrap_or(0),
            u32::try_from(hy).unwrap_or(0),
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_client_model::combat::CombatMode;

    /// The whole of [`update_cursor_state`]'s branch table, driven to the enum **and** the hotspot,
    /// with the shipped mapper's name for each so a wrong arm is legible rather than a number.
    ///
    /// Oracle: the cursor-state branch table and the shipped mapper's `enum_to_name` entries.
    #[allow(clippy::type_complexity)]
    fn table() -> Vec<(&'static str, CursorInputs, u32, (i32, i32))> {
        let n = CursorInputs::default;
        vec![
            ("Default", n(), 1, (0, 0)),
            (
                "Default_OverObject",
                CursorInputs {
                    hovering: true,
                    ..n()
                },
                2,
                (0, 0),
            ),
            (
                "Combat (melee)",
                CursorInputs {
                    combat_mode: CombatMode::Melee,
                    ..n()
                },
                3,
                (0, 0),
            ),
            (
                "Combat_OverObject (missile)",
                CursorInputs {
                    combat_mode: CombatMode::Missile,
                    hovering: true,
                    ..n()
                },
                4,
                (0, 0),
            ),
            (
                "Spellcast",
                CursorInputs {
                    combat_mode: CombatMode::Magic,
                    ..n()
                },
                5,
                (0, 0),
            ),
            (
                "Spellcast_OverObject",
                CursorInputs {
                    combat_mode: CombatMode::Magic,
                    hovering: true,
                    ..n()
                },
                6,
                (0, 0),
            ),
            (
                "Examine",
                CursorInputs {
                    target_mode: TargetMode::Examine,
                    ..n()
                },
                10,
                (0, 0),
            ),
            (
                "Examine_OverObject",
                CursorInputs {
                    target_mode: TargetMode::Examine,
                    hovering: true,
                    ..n()
                },
                11,
                (0, 0),
            ),
            (
                "Use",
                CursorInputs {
                    target_mode: TargetMode::Use,
                    ..n()
                },
                12,
                (14, 14),
            ),
            (
                "Use_OverObject",
                CursorInputs {
                    target_mode: TargetMode::Use,
                    hovering: true,
                    ..n()
                },
                13,
                (14, 14),
            ),
            ("Wait", CursorInputs { busy: 1, ..n() }, 14, (0, 0)),
            (
                "Wait_OverObject",
                CursorInputs {
                    busy: 1,
                    hovering: true,
                    ..n()
                },
                15,
                (0, 0),
            ),
            (
                "TargetedUse",
                CursorInputs {
                    target_mode: TargetMode::UseTarget,
                    ..n()
                },
                0x27,
                (14, 14),
            ),
            (
                "TargetedUse_OverObject",
                CursorInputs {
                    target_mode: TargetMode::UseTarget,
                    hovering: true,
                    target_compatible: true,
                    ..n()
                },
                0x28,
                (14, 14),
            ),
            (
                "TargetedUse_OverInvlaidObject",
                CursorInputs {
                    target_mode: TargetMode::UseTarget,
                    hovering: true,
                    target_compatible: false,
                    ..n()
                },
                0x29,
                (14, 14),
            ),
        ]
    }

    #[test]
    fn every_branch_resolves_to_the_enum_and_hotspot_the_client_resolves() {
        for (name, i, want_enum, want_hot) in table() {
            let got = update_cursor_state(i);
            assert_eq!(got.enum_value, want_enum, "{name}: enum, from {i:?}");
            assert_eq!(got.hot, want_hot, "{name}: hotspot, from {i:?}");
        }
    }

    // Oracle: the retail hotspot, which is (0,0) everywhere except the three
    // `TargetMode::Use` / `TargetMode::UseTarget` arms.
    #[test]
    fn exactly_three_states_use_the_fourteen_fourteen_hotspot() {
        let hot: Vec<&str> = table()
            .into_iter()
            .filter(|r| r.3 == (14, 14))
            .map(|r| r.0)
            .collect();
        assert_eq!(
            hot,
            vec![
                "Use",
                "Use_OverObject",
                "TargetedUse",
                "TargetedUse_OverObject",
                "TargetedUse_OverInvlaidObject"
            ],
            "the USE and USE_TARGET families, and nothing else"
        );
        for (name, i, _, _) in table() {
            let is_targeting =
                matches!(i.target_mode, TargetMode::Use | TargetMode::UseTarget) && i.busy == 0;
            assert_eq!(
                update_cursor_state(i).hot == (14, 14),
                is_targeting,
                "{name} hotspot class"
            );
        }
    }

    // Oracle: retail tests the busy count first and answers `0xE + h` when it is non-zero; nothing
    // else is consulted.
    #[test]
    fn busy_outranks_every_other_input() {
        for (_, base, _, _) in table() {
            let i = CursorInputs { busy: 3, ..base };
            let got = update_cursor_state(i);
            assert_eq!(
                got.enum_value,
                14 + u32::from(i.hovering),
                "busy wins over {base:?}"
            );
            assert_eq!(got.hot, (0, 0), "and the Wait cursor's hotspot is (0,0)");
        }
    }

    // Oracle: the `+ h` in every arithmetic arm. The five `_OverObject` pairs in the shipped mapper
    // are the confirmation that the increment is the hover bit.
    #[test]
    fn hovering_adds_exactly_one_to_every_arm_that_is_not_the_targeting_pair() {
        for mode in [TargetMode::None, TargetMode::Use, TargetMode::Examine] {
            for combat in [CombatMode::NonCombat, CombatMode::Melee, CombatMode::Magic] {
                for busy in [0, 1] {
                    let off = CursorInputs {
                        busy,
                        target_mode: mode,
                        combat_mode: combat,
                        hovering: false,
                        target_compatible: false,
                    };
                    let on = CursorInputs {
                        hovering: true,
                        ..off
                    };
                    assert_eq!(
                        update_cursor_state(on).enum_value,
                        update_cursor_state(off).enum_value + 1,
                        "{off:?}"
                    );
                    assert_eq!(update_cursor_state(on).hot, update_cursor_state(off).hot);
                }
            }
        }
    }

    // Oracle: `0x29` minus the compatibility answer. The direction of this is the unit's whole
    // point: a compatible target must show 0x28 (`TargetedUse_OverObject`) and an incompatible one 0x29
    // (`TargetedUse_OverInvlaidObject`). Backwards, it is invisible to any pixel test and tells the
    // player the opposite of the truth.
    #[test]
    fn the_compatible_target_gets_the_lower_of_the_two_targeting_cursors() {
        let base = CursorInputs {
            target_mode: TargetMode::UseTarget,
            hovering: true,
            ..CursorInputs::default()
        };
        let legal = update_cursor_state(CursorInputs {
            target_compatible: true,
            ..base
        });
        let illegal = update_cursor_state(CursorInputs {
            target_compatible: false,
            ..base
        });
        assert_eq!(legal.enum_value, 0x28);
        assert_eq!(illegal.enum_value, 0x29);
        assert_eq!(
            illegal.enum_value - legal.enum_value,
            1,
            "0x29 - compatible"
        );
        // And `target_compatible` is read on **no other** arm, exactly as the client reads it only
        // on the hovering branch of the `USE_TARGET` case.
        for (name, i, want, _) in table() {
            if i.target_mode == TargetMode::UseTarget && i.hovering {
                continue;
            }
            let flipped = CursorInputs {
                target_compatible: !i.target_compatible,
                ..i
            };
            assert_eq!(
                update_cursor_state(flipped).enum_value,
                want,
                "{name} ignores compat"
            );
        }
    }

    // Both target-use predicates operate on the **high half** of the bitfield, shifted down sixteen.
    #[test]
    fn the_target_side_item_uses_read_the_high_half_of_the_bitfield() {
        // Low-half bits must not reach either.
        let low = ItemUses(0x0000_FFFF);
        assert_eq!(low.least_limited_target_use(), 0);
        assert!(!low.is_useable_self_target());
        // The priority order, one bit at a time in the high half.
        for (bit, want) in [
            (item_useable::REMOTE, item_useable::REMOTE),
            (item_useable::VIEWED, item_useable::VIEWED),
            (item_useable::CONTAINED, item_useable::CONTAINED),
            (item_useable::WIELDED, item_useable::WIELDED),
            (item_useable::SELF, item_useable::SELF),
            (item_useable::OBJSELF, item_useable::OBJSELF),
        ] {
            assert_eq!(
                ItemUses(bit << 16).least_limited_target_use(),
                want,
                "{bit:#x}"
            );
        }
        // Least-limited really means least limited: remote beats everything below it.
        let all = item_useable::REMOTE | item_useable::VIEWED | item_useable::CONTAINED;
        assert_eq!(
            ItemUses(all << 16).least_limited_target_use(),
            item_useable::REMOTE
        );
        assert!(ItemUses(item_useable::SELF << 16).is_useable_self_target());
        assert!(!ItemUses(item_useable::WIELDED << 16).is_useable_self_target());
    }

    /// A compatible pair: the source may be used remotely on a target of `TYPE_MISC`, the target is
    /// a `TYPE_MISC` object owned by nobody in particular.
    fn compatible() -> TargetCompat {
        TargetCompat {
            source_useability: (item_useable::REMOTE << 16) | item_useable::REMOTE,
            source_target_type: 0x80, // TYPE_MISC
            source_owned_by_player: true,
            target_trade_state: 0,
            target_owned_by_player: true,
            target_is_the_player: false,
            target_item_type: 0x80,
        }
    }

    // Each compatibility-refusal arm has one assertion.
    #[test]
    fn every_refusal_in_is_target_compatible_is_reachable_and_only_by_its_own_arm() {
        assert!(
            is_target_compatible(compatible()),
            "the control case must pass"
        );

        // Arm 1: the source is not the player's and its least-limited *source* use is CONTAINED.
        assert!(!is_target_compatible(TargetCompat {
            source_owned_by_player: false,
            source_useability: (item_useable::REMOTE << 16) | item_useable::CONTAINED,
            ..compatible()
        }));
        // Arm 2: ... or WIELDED.
        assert!(!is_target_compatible(TargetCompat {
            source_owned_by_player: false,
            source_useability: (item_useable::REMOTE << 16) | item_useable::WIELDED,
            ..compatible()
        }));
        // Both are skipped entirely when the source *is* owned by the player.
        assert!(is_target_compatible(TargetCompat {
            source_owned_by_player: true,
            source_useability: (item_useable::REMOTE << 16) | item_useable::CONTAINED,
            ..compatible()
        }));

        // Arm 3: the target is on the trade window. This one falls out of the function.
        assert!(!is_target_compatible(TargetCompat {
            target_trade_state: 1,
            ..compatible()
        }));
        assert!(is_target_compatible(TargetCompat {
            target_trade_state: 2,
            ..compatible()
        }));

        // Arm 4: the target is not the player's, the *target* use is CONTAINED, and the target is
        // not the player.
        let contained_target = (item_useable::CONTAINED << 16) | item_useable::REMOTE;
        assert!(!is_target_compatible(TargetCompat {
            target_owned_by_player: false,
            source_useability: contained_target,
            ..compatible()
        }));
        // Arm 5: ... it *is* the player, but the source cannot be used on the self.
        assert!(!is_target_compatible(TargetCompat {
            target_owned_by_player: false,
            target_is_the_player: true,
            source_useability: contained_target,
            ..compatible()
        }));
        // ... and passes once SELF is set in the high half too.
        assert!(is_target_compatible(TargetCompat {
            target_owned_by_player: false,
            target_is_the_player: true,
            source_useability: ((item_useable::CONTAINED | item_useable::SELF) << 16)
                | item_useable::REMOTE,
            ..compatible()
        }));
        // Arm 6: the target use is WIELDED (the `else if`), which refuses outright.
        assert!(!is_target_compatible(TargetCompat {
            target_owned_by_player: false,
            source_useability: (item_useable::WIELDED << 16) | item_useable::REMOTE,
            ..compatible()
        }));

        // Arm 7: aiming at yourself with something that has no self-target bit, *even when* the
        // whole `!target_owned_by_player` block was skipped.
        assert!(!is_target_compatible(TargetCompat {
            target_is_the_player: true,
            target_owned_by_player: true,
            ..compatible()
        }));
        assert!(is_target_compatible(TargetCompat {
            target_is_the_player: true,
            target_owned_by_player: true,
            source_useability: ((item_useable::REMOTE | item_useable::SELF) << 16)
                | item_useable::REMOTE,
            ..compatible()
        }));

        // Arm 8, the last line: the type masks must intersect. `TYPE_SELF` is 0.
        assert!(!is_target_compatible(TargetCompat {
            source_target_type: 0x10, // TYPE_CREATURE
            target_item_type: 0x80,   // TYPE_MISC
            ..compatible()
        }));
        assert!(!is_target_compatible(TargetCompat {
            source_target_type: 0,
            ..compatible()
        }));
        assert!(is_target_compatible(TargetCompat {
            source_target_type: 0x90,
            target_item_type: 0x10,
            ..compatible()
        }));
    }

    // Oracle: the function's first two lines. A zero id on either side is refused before anything
    // is looked up, which is why a build with no targeting-object writer shows
    // `TargetedUse_OverInvlaidObject` rather than a wrong answer.
    #[test]
    fn a_zero_id_on_either_side_is_incompatible_without_consulting_the_world() {
        let w = dereth_client_model::World::new();
        use dereth_primitives::ObjectId;
        assert!(!is_target_compatible_with_targeting_object(
            &w,
            ObjectId(0),
            ObjectId(7)
        ));
        assert!(!is_target_compatible_with_targeting_object(
            &w,
            ObjectId(7),
            ObjectId(0)
        ));
        // And an id the client has never heard of is refused at the object-lookup null test.
        assert!(!is_target_compatible_with_targeting_object(
            &w,
            ObjectId(7),
            ObjectId(8)
        ));
    }
}
