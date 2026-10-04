//! The selection cycle and its radar-range gate.
//!
//! [`dereth_client_model::range`](crate::range) carries the object-range *watch* and
//! `dereth_ui_screens::mapradar::radar::radar_range` carries the *producer* of the range itself.
//! This module carries two readers of that range:
//! `within_radar_range` and the `select_next` method, with the ordering
//! it is built on — `farther`, `get_2d_distance` and `weighted_z_distance`.
//!
//! **The two disagree about the boundary and that is deliberate.** `within_radar_range` refuses
//! `d == r`; `select_next` accepts it. See the table below, which is here so that a later
//! refactor does not route the five consumers of the one 75/25 radius through a single predicate
//! that would be wrong in three of them.
//!
//! The shared radius is **75.0 m outdoors** and **25.0 m indoors**. Selection uses the same
//! indoor/outdoor decision as the radar; the consumers below deliberately keep distinct
//! boundary predicates.
//!
//! # Five consumers of one range, and they do not agree about the boundary
//!
//! Each boundary includes or excludes equality as shown:
//!
//! | consumer | what it compares | `- 1`? | boundary |
//! |---|---|---|---|
//! | radar blip cull | squared distance vs `(r - 1)²` | **yes** | — |
//! | chat hearing | `d²` vs `r²` | no | **exclusive** |
//! | `within_radar_range` | `sqrt(x²+y²)` vs `r` | no | **exclusive** |
//! | selection cycle | `get_2d_distance` vs `r` | no | **inclusive** |
//! | object-range check | `distance` vs `r` | no | **inclusive** |
//!
//! So of the five, one subtracts a metre, two exclude the boundary and two include it. A single
//! shared "is it in radar range" helper would be wrong in three of the five places.
//!
//! # What is here and what is not
//!
//! The `select_last_attacker` method uses `within_radar_range` for input action `0x10000038`
//! `SelectionLastAttacker` (the action dispatch indexes from `0x1000002A`, and
//! `0x10000038 - 0x1000002A = 0x0E` selects case `0x0D`).
//!
//! The producer of that action is
//! `dereth_client::interaction::Interaction::on_actions`'s `action::SELECTION_LAST_ATTACKER`, beside
//! the sixteen `select_next` arms and the `SELECTION_EXAMINE` and `USE` arms; it
//! resolves the attacker's player-space offset through the same
//! `dereth_client::selection_geometry::SceneSelectionPhysics` snapshot the cycle uses, because
//! last-attacker selection starts with an id and needs its geometry resolved.
//! This is asserted through the wire on both sides of the range in the client's CPU selection tests.

use dereth_primitives::ObjectId;

use crate::qualities::{StatKey, StatType, StatValue};
use crate::{NoticeSink, World};

/// Player instance-id quality **`0x0B`** — "last attacker".
///
/// Explicit last-attacker selection and automatic targeting read the same quality.
pub const LAST_ATTACKER_IID: u32 = 0x0B;

/// Test the horizontal player-space distance against an exclusive radar radius.
///
/// Four details, each easy to invert:
///
/// * **`z` is not in the sum**, exactly as in chat hearing. The player-space offset is
///   cell-aware; an object directly above or below you is at distance zero.
/// * **The range carries no `- 1`.** Nothing is subtracted anywhere in it. The radar's own
///   blip cull *does* subtract `1.0`, so an
///   object at 74.5 m outdoors **can be selected as your last attacker and is not drawn on the
///   radar**.
/// * **The boundary is exclusive.** Both less-than and equal are refused, so `d == r` returns false — exactly on the radius the attacker is *not* re-selected.
///   A plain `sqrt(...) < r` reads like the whole story and is not: it is the *comparison* that
///   says the equal case is refused too.
/// * **The square root is really taken.** A square root then a compare against `r`, where chat hearing
///   compares `d²` against `r²` with no root at all. They agree mathematically and can differ in
///   the last ulp, which is why this is not written as `d2 < r*r`.
///
/// Everything above the compare happens in extended precision with no intermediate rounding
/// to `f32`, so `f64` is the faithful model of the arithmetic and the `f32`
/// inputs are the only rounding.
#[must_use]
pub fn within_radar_range(dx: f32, dy: f32, radar_radius: f32) -> bool {
    // Two separate products and then a sum, deliberately **not** a `mul_add`: retail rounds the
    // two squares and then adds them, which is two roundings, and a fused multiply-add is one. Both products of `f32` inputs are exact in `f64`, so the two
    // spellings agree everywhere this can be driven — but the faithful one is the one written.
    #[allow(clippy::suboptimal_flops)]
    let d = (f64::from(dx) * f64::from(dx) + f64::from(dy) * f64::from(dy)).sqrt();
    d < f64::from(radar_radius)
}

impl World {
    /// Handle input action `0x10000038`, `SelectionLastAttacker`.
    ///
    /// The action reads quality [`LAST_ATTACKER_IID`], checks visibility, then checks range.
    ///
    /// * **Both gates are `&&` and both are cheap-first**: an id that is not in the
    ///   `visible_object_table` never reaches the distance test at all. The visible table is
    ///   the client's, rebuilt at 1 Hz — so an attacker
    ///   that became visible less than a second ago is refused even standing next to you.
    /// * Selection is not forced: re-selecting the same attacker takes
    ///   the selection setter's early return and raises no selection-change notice; the notice
    ///   does not escape the changed-id guard.
    /// * The action returns `false` only when there are no player qualities. A zero attacker id
    ///   or a failed visibility/range check still consumes the input event.
    ///
    /// `attacker_player_space` is the seam this crate cannot cross on its own: it is `None` when
    /// there is no physics object or its cell is absent. That prevents selection.
    /// Note the asymmetry with chat hearing, whose equivalent escape returns
    /// **true**: a speaker with no physics object is heard at any range, an attacker with no
    /// physics object is never re-selected. Two functions, one seam, opposite defaults.
    ///
    /// `radar_radius` is `radar_range(player_outside())`, passed in rather than recomputed so this
    /// code and the
    /// radar cannot disagree about which of `75.0`/`25.0` is in force.
    ///
    /// **Its production caller is `Interaction::on_actions`'s `SELECTION_LAST_ATTACKER` arm**,
    /// which is `case 0xD`'s equivalent.
    pub fn select_last_attacker(
        &mut self,
        attacker_player_space: Option<(f32, f32)>,
        radar_radius: f32,
        out: &mut dyn NoticeSink,
    ) -> bool {
        // Player instance-id qualities are absent before `0x0013 Login_PlayerDescription`.
        if self.player_qualities().is_none() {
            return false;
        }
        // The instance-id lookup of `0x0B` over a zeroed out-parameter: unset reads as 0.
        let attacker = self.last_attacker();
        if attacker.0 == 0 {
            return true;
        }
        if self.tables.visible.contains(&attacker)
            && attacker_player_space
                .is_some_and(|(dx, dy)| within_radar_range(dx, dy, radar_radius))
        {
            self.set_selected_object(Some(attacker), false, out);
        }
        true
    }

    /// Read [`LAST_ATTACKER_IID`] for explicit selection and automatic targeting.
    ///
    /// An absent quality and an explicit zero both mean "no last attacker", so both return
    /// `ObjectId(0)` rather than an `Option`.
    ///
    /// It exists so the two consumers share one read: last-attacker selection needs the
    /// id, and so does its **caller**, which has to resolve the same id to a player-space offset
    /// across the geometry seam this crate cannot cross. Two independent reads of one quality is
    /// the shape that lets a caller and a callee disagree about which object they are talking
    /// about.
    #[must_use]
    pub fn last_attacker(&self) -> ObjectId {
        match self
            .player_qualities()
            .and_then(|q| q.get(StatKey::new(StatType::Iid, LAST_ATTACKER_IID)))
        {
            Some(StatValue::Iid(id)) => id,
            _ => ObjectId(0),
        }
    }
}

// =================================================================================================
// Selection cycling and its ordering
// =================================================================================================

/// Which of the five filters the selection cycle runs.
///
/// The original switch indexes by `kind - 1`, so zero and anything above five take the
/// **default**, which is
/// the point the accepted arms jump to. An out-of-range selection type therefore
/// rejects nothing and every visible object is a candidate. That is why this enum has no
/// `Undef`: it is not a value any of the sixteen actions passes, and modelling it would invite
/// the reading that it filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SelectionType {
    /// 1 — Loot: **not** wielded, and either carrying no
    /// `RADAR_ENUM` at all or being one of the three [`COMPASS_ALWAYS`] fixtures.
    Item = 1,
    /// 2 — What the compass shows, minus your fellows and (in a combat stance)
    /// anything unattackable.
    CompassItem = 2,
    /// 3 — Attackable, not a vendor, showable on the radar, not a fellow.
    Monster = 3,
    /// 4 — A player showable on the radar.
    Player = 4,
    /// 5 — A corpse not yet opened this session.
    UnopenedCorpse = 5,
}

/// Lifestone, portal and bindstone bits, combined as `0x8044000`.
///
/// The individual masks are 16384, 262144 and 134217728. They mark fixtures a player wants to
/// tab to and which are not creatures, so both arms treat them as always selectable: the compass
/// arm lets them past the showable-on-radar test and the item arm lets them past the `RADAR_ENUM`
/// test.
///
pub const COMPASS_ALWAYS: u32 =
    0x0000_4000 /* BF_LIFESTONE */ | 0x0004_0000 /* BF_PORTAL */ | 0x0800_0000 /* BF_BINDSTONE */;

/// Physics-state bit `0x100000`: a cloaked object is excluded from selection.
pub const CLOAKED_PS: u32 = 0x0010_0000;

/// Physics-state bit `0x200000`, tested by the [`SelectionType::CompassItem`] arm alone.
pub const REPORT_COLLISIONS_AS_ENVIRONMENT_PS: u32 = 0x0020_0000;

/// The selection scan's distance sentinel.
///
/// `0x40F2_0000_0000_0000` is exactly **73728.0** — 1.125 × 2¹⁶. It is not a distance the world
/// can produce (the radar radius caps candidates at 75.0 m and the z weight cannot lift a 75 m
/// pair past 73728), so it is an infinity for this ordering and nothing more. Both ends of the
/// scan use it: the *reference* takes it when there is nothing selected and the search is
/// outward, and the *best-so-far* takes it when the search is inward.
pub const SELECT_NEXT_SENTINEL: f64 = 73728.0;

/// The 2-D distance — `sqrt(x² + y²)`, and **no z**.
///
/// Three things worth pinning, because each is a place a plausible rewrite would differ:
///
/// * **The first argument is `x`, not `y`**, and the function squares it first. The sum is symmetric and nothing observable turns on it, but the order is
///   written out below because `within_radar_range` really does
///   square `y` first — the two functions differ here and a reader comparing them needs to know
///   which is which.
/// * **It is the same shape as `within_radar_range` and it is a different function.** Both take
///   a square root; chat hearing compares `d²` against `r²` and takes none.
///   Three transcriptions, deliberately not shared — see the boundary table in this module's
///   header.
/// * **Its result is stored as a 64-bit `double`** by the caller,
///   so the value that reaches the range comparison and the ordering key is exactly an `f64`.
///   There is no extended-precision residue in the boundary test.
#[must_use]
pub fn get_2d_distance(x: f32, y: f32) -> f64 {
    // `pow(v, 2.0)`, twice, then an add; two roundings and not a `mul_add`, for the reason
    // given at `within_radar_range`. Both products of `f32` inputs are exact in `f64`.
    #[allow(clippy::suboptimal_flops)]
    let sum = f64::from(x) * f64::from(x) + f64::from(y) * f64::from(y);
    sum.sqrt()
}

/// The weighted z distance — `1.2 × |z|`.
///
/// It negates `z` when `z < 0.0` and multiplies by 1.2.
///
/// The multiplier is a **`double`**, not a float, so it
/// is `f64`'s nearest value to 1.2 and not `f32`'s. Written out because the two differ at the
/// eleventh significant digit and a `1.2f32` transcription would be wrong in the last bits of
/// every ordering key.
///
/// The vertical axis is weighted **up**, not down: a target one metre above you sorts as 1.2 m
/// away while one metre in front sorts as 1.0 m. So the tab cycle prefers things on your own
/// level, which is what makes it usable on a dungeon stair. Note the asymmetry with
/// `within_radar_range` and with the range gate inside the selection cycle, **neither** of
/// which looks at z at all: z decides the *order* of the candidates and never their
/// *eligibility*.
#[must_use]
pub fn weighted_z_distance(z: f32) -> f64 {
    let z = f64::from(z);
    // Written as the client's compare-and-negate rather than as `.abs()`: they differ on a NaN's
    // sign bit and on nothing else, and this is the spelling that matches the client's
    // compare-and-negate described above.
    let magnitude = if z < 0.0 { -z } else { z };
    magnitude * 1.2
}

/// Compare selection keys by distance, then by unsigned object id.
///
/// The cycle uses this comparison for both the reference and the best candidate, in either
/// direction. A NaN distance makes the comparison false rather than entering the id tiebreak.
///
/// So `farther((a, id_a), (b, id_b))` is `a > b || (a == b && id_a > id_b)`, with the id compared
/// **unsigned**. Object ids are `0x5xxxxxxx` and above in retail traffic, so a
/// signed transcription would invert the tiebreak for every dynamic object in the world.
///
/// **Why the tiebreak is the load-bearing half.** The tab-target cycle walks
/// a hash table — its bucket traversal order
/// is bucket order, which is a function of the ids and the table size and is not the order
/// anything else in the client would produce. Without the id tiebreak, two objects at exactly
/// equal distance would make the answer depend on that traversal, and repeated presses of the
/// same key could cycle between them or stick. With it the scan is a min (or max) over a
/// **total** order, so the result does not depend on the traversal at all — which is why this
/// crate may iterate a `BTreeSet` and still be faithful. The selection tests assert that
/// equivalence.
///
/// **Three compares of the same pair, and the third is not redundant.** Retail makes the
/// `a<b`, `a==b` and "ordered-equal" tests separately, which is how an **unordered** result
/// (either operand NaN) is refused: the first two fall through and the third takes the branch
/// to `FALSE`. Rust's `>` and `==` are false on NaN, so the expression below
/// agrees on every input including that one.
#[must_use]
pub fn farther(a: (f64, ObjectId), b: (f64, ObjectId)) -> bool {
    a.0 > b.0 || (a.0 == b.0 && a.1 .0 > b.1 .0)
}

/// The geometry and physics-state facts needed to select one object.
///
/// This is the same seam `dereth_client::object_range` is for the range watch and
/// `Hud::speaker_player_space` is for chat hearing, in the shape this function needs: it is
/// asked once per member of the `visible_object_table` and once for the reference object.
///
/// The geometry provider returns `None` when the player or target lacks a physics object, or
/// when the cell information needed for the conversion is absent. The same preconditions
/// apply to candidates and the current reference object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionPhysics {
    /// Player-space `(x, y, z)` — cell-aware, so it is a real offset
    /// across a landblock boundary and not a raw coordinate subtraction.
    pub player_space: (f32, f32, f32),
    /// `state & `[`CLOAKED_PS`].
    ///
    /// **This bit has a producer.** The seam,
    /// `dereth_client::selection_geometry::SceneSelectionPhysics`, reads the live physics-state
    /// word and passes the real bit;
    /// [`objects::PhysicsPresence::state`](crate::objects::PhysicsPresence::state) carries the
    /// whole word, is the single source that `0xF74B Item_SetState` writes, and is the **only**
    /// copy. A seam that passes a constant `false` for either of these two bits fails the
    /// selection tests.
    pub cloaked: bool,
    /// `state & `[`REPORT_COLLISIONS_AS_ENVIRONMENT_PS`]. The same live word as [`Self::cloaked`]
    /// — and the only reader is the [`SelectionType::CompassItem`] arm.
    pub reports_collisions_as_environment: bool,
}

impl World {
    /// The tab-target cycle's selection-type switch — **true when the candidate is rejected**,
    /// which is the polarity of the rejection flag the client sets and tests.
    ///
    /// Kept public and separate so the five filters can be asserted one at a time. Five filters
    /// asserted through the whole function in aggregate would pass with four of them dead, which
    /// is this project's dominant defect.
    ///
    /// The five filters:
    ///
    /// | kind | acceptance rule |
    /// |---|---|
    /// | [`SelectionType::Item`] | Not wielded; radar enum is zero or a [`COMPASS_ALWAYS`] bit is set |
    /// | [`SelectionType::CompassItem`] | A compass fixture or radar-visible; in melee/missile stance also attackable, not a fellow, not a vendor and not reporting collisions as environment |
    /// | [`SelectionType::Monster`] | Attackable, not a vendor, radar-visible, not a fellow |
    /// | [`SelectionType::Player`] | A radar-visible player |
    /// | [`SelectionType::UnopenedCorpse`] | A corpse not opened this session |
    ///
    /// **`SelectionType::Item`'s `_radar_enum` test reads backwards and is not.**
    /// It loads `_radar_enum`, jumps to the *accept* label when it is **zero**, and only a
    /// non-zero one has to prove itself with [`COMPASS_ALWAYS`]. So "Select Closest Item" wants
    /// the objects the radar was never told to show — ordinary loot on the ground — plus the
    /// three fixtures, and an NPC with `ShowAlways` is refused by it. That is the right
    /// behaviour and it is the opposite of what the arm looks like at a glance.
    ///
    /// **The combat-stance gate is a mode test, not a mask test.** It reads the current
    /// mode and compares it with `2` and then `4`; the combat mode is a
    /// bit mask whose members are `1/2/4/8`, so `== 2 || == 4` and `& 6 != 0` agree only because
    /// the field never holds a union. The client's spelling is kept.
    ///
    /// Radar visibility does not depend on a position vector, so nothing here passes one.
    /// `dereth_ui_screens::mapradar::radar::inq_showable_on_radar` is the same predicate over
    /// the radar's own row type; this reads it off the `Weenie` instead of routing a UI type
    /// through `dereth-client-model`.
    #[must_use]
    pub fn selection_type_rejects(
        &self,
        kind: SelectionType,
        id: ObjectId,
        phys: &SelectionPhysics,
    ) -> bool {
        use crate::weenie::bitfield;

        let Some(w) = self.weenie(id) else {
            return true;
        };
        // Require a physics object, then inspect the radar enum in this order:
        // it accepts 2, 4 and 3 — `ShowMovement`, `ShowAlways`,
        // `ShowAttacking`. `Undef` (0) and `ShowNever` (1) are the two that fall through.
        let radar_enum = w.pwd.radar_enum.unwrap_or(0);
        let showable_on_radar =
            w.has_phys_obj && (radar_enum == 2 || radar_enum == 4 || radar_enum == 3);
        let is_fellow = self
            .fellowship
            .as_ref()
            .is_some_and(|f| f.members.contains_key(&id));

        match kind {
            SelectionType::Item => {
                if w.pwd.wielder_id.is_some_and(|o| o.0 != 0) {
                    return true;
                }
                if w.pwd.radar_enum.unwrap_or(0) == 0 {
                    return false;
                }
                w.pwd.bitfield & COMPASS_ALWAYS == 0
            }
            SelectionType::CompassItem => {
                if w.pwd.bitfield & COMPASS_ALWAYS == 0 && !showable_on_radar {
                    return true;
                }
                let mode = self.combat.combat_mode;
                if mode != crate::combat::CombatMode::Melee
                    && mode != crate::combat::CombatMode::Missile
                {
                    return false;
                }
                if !self.object_is_attackable(id) {
                    return true;
                }
                !(!is_fellow
                    && w.pwd.bitfield & bitfield::VENDOR == 0
                    && !phys.reports_collisions_as_environment)
            }
            SelectionType::Monster => {
                if !self.object_is_attackable(id) {
                    return true;
                }
                if w.pwd.bitfield & bitfield::VENDOR != 0 {
                    return true;
                }
                if !showable_on_radar {
                    return true;
                }
                is_fellow
            }
            SelectionType::Player => !(w.is_player() && showable_on_radar),
            SelectionType::UnopenedCorpse => !w.is_corpse() || self.has_corpse_been_opened(id),
        }
    }

    /// The tab-target cycle.
    ///
    /// Sixteen input actions reach it, through twenty-six of its twenty-seven call sites;
    /// the twenty-seventh is the automatic-target path. All are
    /// listed at the end of this comment. **All twenty-seven exist in this build** — the
    /// twenty-six through the sixteen `on_actions` arms, the twenty-seventh through automatic
    /// targeting; see *the wire* below.
    ///
    /// # The scan
    ///
    /// Make one pass over the visible-object table, keeping the extreme candidate under
    /// `farther`'s order on the correct side of a *reference* key:
    ///
    /// * `closer == false` (**Next**, outward): keep the **minimum** key strictly greater than the
    ///   reference: `!farther(cand, best) && farther(cand, reference)`, with `best`
    ///   seeded at [`SELECT_NEXT_SENTINEL`].
    /// * `closer == true` (**Previous**, inward): keep the **maximum** key not greater than the
    ///   reference: `farther(cand, best) && !farther(cand, reference)`, with `best`
    ///   seeded at `0.0`.
    ///
    /// The key is `(get_2d_distance(x, y) + weighted_z_distance(z), id)`.
    ///
    /// # The reference, and the flip that produces the wrap-around
    ///
    /// The reference object is the selected id, or the previous selected id when none is selected.
    /// If `ignore_current` is set, or that object has no physics object,
    /// or it is in no cell, the client **inverts `closer`**
    /// and puts the reference at the sentinel on the
    /// far side: `closer` after the flip picks `73728.0`, `!closer` picks `0.0`.
    ///
    /// That single flip is the whole wrap-around, and it is worth stating in both directions
    /// because only one of them is obvious:
    ///
    /// * The action handler's "Closest" arms pass `closer = true, ignore_current = true` and get
    ///   `closer = false` with a reference of `0.0`, i.e. the **nearest** object. That is what
    ///   makes `SelectionClosestMonster` closest.
    /// * Its "Previous"/"Next" arms call once against the real selection and then, **only if
    ///   the selected id did not move**, call again with `ignore_current = true` and `closer`
    ///   already inverted, which the flip inverts back — so "Next" past the farthest object wraps
    ///   to the nearest and "Previous" past the nearest wraps to the farthest.
    /// * The same flip fires when *nothing is selected at all*, and there it is surprising:
    ///   `SelectionNextItem` with an empty selection asks for `closer = false`, the flip makes it
    ///   `true` with a reference of `73728.0`, and the client selects the **farthest** item in
    ///   radar range rather than the nearest. This is faithful, it is pinned by
    ///   the selection tests, and it is the reason "Closest" is a separate action rather than
    ///   an alias for "Next".
    ///
    /// # The boundary — INCLUSIVE, and its two nearest neighbours are not
    ///
    /// The radar radius (a float) is compared against the 2-D distance, and the candidate is
    /// skipped only when the radius is **strictly less**. The equal case is not refused, so
    /// `d == r` falls through and the object **is** a candidate. `within_radar_range` above
    /// refuses the equal case, as does chat hearing.
    /// One radius, five consumers, three treatments, deliberately —
    /// this module's header carries the table so that nobody routes them
    /// through one helper later. Do not borrow a neighbour's predicate here; it is one comparison
    /// different and the difference is the point.
    ///
    /// Two more things the same comparison settles. The comparison is against the **2-D** distance,
    /// before `weighted_z_distance` is added — so z decides the order and never
    /// the eligibility, and an object 74 m away and 200 m below you is still selectable. And
    /// there is no `- 1`: the radar's own blip cull subtracts a metre and this does not,
    /// so there is a one-metre band in which an object can be
    /// tabbed to and is not drawn on the radar.
    ///
    /// # The gates, in the client's own order
    ///
    /// 1. Zero id (including the redundant always-zero merge-source field described below).
    /// 2. Missing game object or physics object, or failed player-space conversion.
    /// 3. UI-hidden bit `0x80` in the low byte.
    /// 4. An owned wielded item when `exclude_own_wielded` is set.
    /// 5. An object being removed.
    /// 6. Rejection by the selection-type filter.
    /// 7. Missing cell or the [`CLOAKED_PS`] bit.
    /// 8. Failure of the inclusive horizontal range test.
    /// 9. The reference object, unless `ignore_current` is set.
    /// 10. The player itself.
    ///
    /// **The merge-source gate cannot fire independently.** The field has one writer, which
    /// stores zero, and one reader, which is this gate. So the
    /// field is always zero and the gate is the `id == 0` gate one line above it. It is transcribed as
    /// a comment rather than as code, because a constant-`0` comparison would read as a live
    /// filter.
    ///
    /// # Deliberately not modelled here
    ///
    /// * **The radar radius is re-read inside the loop**, once per candidate. It is
    ///   passed in once instead, exactly as `select_last_attacker` does, so that
    ///   this and the radar cannot disagree about which of `75.0`/`25.0` is in force. Its value
    ///   cannot change during the scan.
    /// * The original path converts to player space twice per candidate, once for the filter
    ///   and once for the distance, with the same arguments and answer.
    /// * **The empty-table early exit** skips setting the selection
    ///   entirely. With no candidates `best` is `None` and the call does not happen anyway.
    /// * **Retail keeps `1.2 × |z| + d2` in extended precision** and rounds once, at
    ///   a 64-bit store; `f64` rounds twice. `d2` itself is stored as a
    ///   64-bit `double` first, so **the boundary test is exact** and only the ordering key can differ,
    ///   in its last bit, between two candidates whose weighted distances agree to 16 digits.
    ///
    /// # The wire — declared, not forgotten
    ///
    /// **Every caller exists.** The sixteen action arms below are
    /// `dereth_client::interaction::Interaction::on_actions`'s, and the client tests assert them
    /// one arm at a time; the twenty-seventh call site is the automatic-target fallback, whose
    /// production caller is at the end of the combat-mode change.
    ///
    /// Input handling dispatches these actions by their offset from `0x1000002A`. The arms, with
    /// the arguments each one passes and the retry it makes when the selected id did not move:
    ///
    /// | action | id | first call | retry |
    /// |---|---|---|---|
    /// | `SelectionClosestCompassItem` | `0x1000002F` | `(true, true, CompassItem, false)` | — |
    /// | `SelectionPreviousCompassItem` | `0x10000030` | `(true, false, CompassItem, false)` | `(false, true, …)` |
    /// | `SelectionNextCompassItem` | `0x10000031` | `(false, false, CompassItem, false)` | `(true, true, …)` |
    /// | `SelectionClosestItem` | `0x10000032` | `(true, true, Item, `**`true`**`)` | — |
    /// | `SelectionPreviousItem` | `0x10000033` | `(true, false, Item, false)` | `(false, true, …)` |
    /// | `SelectionNextItem` | `0x10000034` | `(false, false, Item, false)` | `(true, true, …)` |
    /// | `SelectionClosestMonster` | `0x10000035` | `(true, true, Monster, false)` | — |
    /// | `SelectionPreviousMonster` | `0x10000036` | `(true, false, Monster, false)` | `(false, true, …)` |
    /// | `SelectionNextMonster` | `0x10000037` | `(false, false, Monster, false)` | `(true, true, …)` |
    /// | `SelectionClosestPlayer` | `0x10000039` | `(true, true, Player, false)` | — |
    /// | `SelectionPreviousPlayer` | `0x1000003A` | `(true, false, Player, false)` | `(false, true, …)` |
    /// | `SelectionNextPlayer` | `0x1000003B` | `(false, false, Player, false)` | `(true, true, …)` |
    /// | `SelectionUseClosestUnopenedCorpse` | `0x1000003E` | `(true, true, UnopenedCorpse, false)` | — |
    /// | `SelectionUseNextUnopenedCorpse` | `0x1000003F` | `(false, false, UnopenedCorpse, false)` | `(true, true, …)` |
    /// | `SelectionClosestUnopenedCorpse` | `0x10000121` | `(true, true, UnopenedCorpse, false)` | — |
    /// | `SelectionNextUnopenedCorpse` | `0x10000122` | `(false, false, UnopenedCorpse, false)` | `(true, true, …)` |
    ///
    /// `0x1000003E`/`0x1000003F` then use the new selection if it is a corpse;
    /// `0x10000121`/`0x10000122` select and stop. **`exclude_own_wielded`
    /// is `true` at exactly one of the twenty-six sites**, `SelectionClosestItem` — and, measured,
    /// **it changes nothing there**.
    ///
    /// It is not true that "select the closest item" skips your own drawn weapon while "select
    /// the next item" picks it: **neither picks it.** The only `kind` that
    /// ships with the flag is [`SelectionType::Item`], whose arm
    /// rejects **any** object with a non-zero wielder id, yours included — and the
    /// `exclude_own_wielded` gate is a strict subset of that for every non-zero
    /// player id. So the flag is faithfully transcribed and **inert at the only pairing that
    /// ships**: the second measured dead gate in this function, after the merge-source gate.
    /// Tests assert both settings: `true` and `false` differ for
    /// `Monster` and agree for `Item`.
    ///
    /// A parameter that varies between call sites is not evidence that it changes anything: the
    /// arm it feeds may already reject the case.
    ///
    /// Automatic targeting falls back to
    /// `(true, true, CompassItem, false)` when it has no live last-attacker to
    /// re-target — i.e. **auto-target is "closest compass item"**.
    // Eight parameters: the client's own four, the geometry seam, the radar radius that the
    // radar's producer owns, and the notice sink. Bundling them would hide which four are the
    // shipped signature, which is the one thing a reader has to be able to check.
    #[allow(clippy::too_many_arguments)]
    pub fn select_next(
        &mut self,
        closer: bool,
        ignore_current: bool,
        kind: SelectionType,
        exclude_own_wielded: bool,
        phys: &dyn Fn(ObjectId) -> Option<SelectionPhysics>,
        radar_radius: f32,
        out: &mut dyn NoticeSink,
    ) {
        use crate::weenie::bitfield;

        // Selected id, else previous selected id. Both may be absent, and
        // the id is still used as `farther`'s tiebreak below even when the object is gone.
        let reference_id = match self.selected {
            Some(id) if id.0 != 0 => id,
            _ => self.prev_selected.unwrap_or(ObjectId(0)),
        };
        let reference_phys = phys(reference_id);

        let (closer, reference_distance) = match reference_phys {
            Some(p) if !ignore_current => {
                let (x, y, z) = p.player_space;
                (closer, get_2d_distance(x, y) + weighted_z_distance(z))
            }
            // `closer = !closer`, then the sentinel on the far side.
            _ => {
                let closer = !closer;
                (closer, if closer { SELECT_NEXT_SENTINEL } else { 0.0 })
            }
        };
        let reference = (reference_distance, reference_id);

        let mut best = (if closer { 0.0 } else { SELECT_NEXT_SENTINEL }, ObjectId(0));

        // The player id is a raw `ulong` and is 0 before login, which is the same value
        // the wielder id carries for "not wielded" — so retail's wielder-equals-player test is
        // true for every unwielded object on a player system that has no player yet. Unreachable
        // (the one action that passes `exclude_own_wielded` is in-game only) and written out
        // rather than guarded, to preserve that equality's behavior.
        let player_id = self.player.unwrap_or(ObjectId(0));

        for &id in &self.tables.visible {
            // The redundant merge-source comparison also sees zero: its field is initialized
            // once to zero and never changed. One zero-id check covers both gates.
            if id.0 == 0 {
                continue;
            }
            let Some(w) = self.weenie(id) else { continue };
            let Some(p) = phys(id) else { continue };

            // the sign bit of the LOW BYTE of the bitfield, i.e. the UI-hidden flag
            // (`0x80`), not bit 31. Tested before the reject flag, and the only gate that is.
            if w.pwd.bitfield & bitfield::UI_HIDDEN != 0 {
                continue;
            }
            if exclude_own_wielded && w.pwd.wielder_id.unwrap_or(ObjectId(0)) == player_id {
                continue;
            }
            if w.being_removed {
                continue;
            }
            if self.selection_type_rejects(kind, id, &p) {
                continue;
            }
            if p.cloaked {
                continue;
            }

            let (x, y, z) = p.player_space;
            let d2 = get_2d_distance(x, y);
            // INCLUSIVE: only strictly-less skips, so `d2 == radius` falls through.
            //
            // The negated `<=` is not a stylistic choice and must not be rewritten as `d2 >`:
            // retail also skips an **unordered** result, as well as strictly-less, so a NaN
            // distance is skipped, and `d2 > radius` is false on a NaN and would keep it.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(d2 <= f64::from(radar_radius)) {
                continue;
            }
            let key = (d2 + weighted_z_distance(z), id);

            if id == reference_id && !ignore_current {
                continue;
            }
            if id == player_id {
                continue;
            }

            let take = if closer {
                farther(key, best) && !farther(key, reference)
            } else {
                !farther(key, best) && farther(key, reference)
            };
            if take {
                best = key;
            }
        }

        if best.1 .0 != 0 {
            // Do not force a notice when the chosen id is already selected.
            self.set_selected_object(Some(best.1), false, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::range::{RADAR_RADIUS_INDOORS, RADAR_RADIUS_OUTDOORS};

    /// (45, 60) is 75 and (15, 20) is 25 — Pythagorean triples, so `d == r` holds bit-exactly in
    /// `f32` rather than to within a tolerance, and both are 36.87° off axis so the station is not
    /// a cardinal one.
    #[test]
    fn the_boundary_is_exclusive_at_both_ranges() {
        assert!(
            !within_radar_range(45.0, 60.0, RADAR_RADIUS_OUTDOORS),
            "d == r is NOT in range"
        );
        assert!(
            !within_radar_range(15.0, 20.0, RADAR_RADIUS_INDOORS),
            "d == r is NOT in range"
        );
        // ...and the exactness of the triples is itself asserted, so a mutation of the station
        // cannot quietly make this test a statement about a tolerance.
        assert_eq!(
            (45.0f64 * 45.0 + 60.0 * 60.0).sqrt(),
            f64::from(RADAR_RADIUS_OUTDOORS)
        );
        assert_eq!(
            (15.0f64 * 15.0 + 20.0 * 20.0).sqrt(),
            f64::from(RADAR_RADIUS_INDOORS)
        );
    }

    #[test]
    fn both_sides_of_both_boundaries() {
        // Just inside, on the same 36.87° heading.
        assert!(within_radar_range(44.99, 59.99, RADAR_RADIUS_OUTDOORS));
        assert!(within_radar_range(14.99, 19.99, RADAR_RADIUS_INDOORS));
        // Just outside.
        assert!(!within_radar_range(45.01, 60.01, RADAR_RADIUS_OUTDOORS));
        assert!(!within_radar_range(15.01, 20.01, RADAR_RADIUS_INDOORS));
        // The pair that separates the two arms: one offset, two answers.
        assert!(
            within_radar_range(24.0, 32.0, RADAR_RADIUS_OUTDOORS),
            "40 m is inside 75"
        );
        assert!(
            !within_radar_range(24.0, 32.0, RADAR_RADIUS_INDOORS),
            "40 m is outside 25"
        );
    }

    /// The radar's blip cull subtracts a metre and this does not, so there is a one-metre band in
    /// which an object is selectable and undrawn. Asserted rather than described.
    #[test]
    fn the_range_carries_no_minus_one_unlike_the_radar_cull() {
        // 74.5 m on the same non-cardinal heading: 3/4/5 scaled by 14.9.
        let (dx, dy) = (44.7_f32, 59.6_f32);
        assert!(within_radar_range(dx, dy, RADAR_RADIUS_OUTDOORS));
        assert!(
            !within_radar_range(dx, dy, RADAR_RADIUS_OUTDOORS - 1.0),
            "with the radar's `- 1` it would be out; within_radar_range subtracts nothing"
        );
    }

    /// `z` is not in the sum: an object straight overhead is at distance zero.
    #[test]
    fn the_distance_is_xy_only() {
        assert!(within_radar_range(0.0, 0.0, RADAR_RADIUS_INDOORS));
    }
}
