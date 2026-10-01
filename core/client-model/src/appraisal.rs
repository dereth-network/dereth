//! The assess profile and its highlight bitfields.
//!
//! `AppraisalProfile` is a **second, transient** property bag holding only what the server chose to
//! reveal. The protocol crate decodes it ([`dereth_protocol::types::AppraisalProfile`]); this
//! module owns the highlight lookup, the assess round trip and the two polling rules.

use crate::world::World;
use crate::{Notice, NoticeSink, RequestSink};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::types::AppraisalProfile;
use std::collections::BTreeMap;

/// The re-assess interval **while in combat mode** with a creature or character panel active:
/// re-sends `Item_Appraise` every **0.75 s**. Out of combat
/// it does not poll at all.
pub const COMBAT_REASSESS_INTERVAL: f64 = 0.75;

/// Which of the three highlight bitfields a property is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightField {
    Armor,
    Weapon,
    Resist,
}

/// One row of the profile's integer and float enchantment-mod queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    pub field: HighlightField,
    /// The low bit: "this stat is enchanted, draw it highlighted".
    pub low: u32,
    /// The high bit: "the enchantment is beneficial (green) rather than harmful (red)".
    pub high: u32,
}

/// Behavior: the three **int** properties with a highlight.
#[must_use]
pub fn int_highlight(property: u32) -> Option<Highlight> {
    use HighlightField::{Armor, Weapon};
    Some(match property {
        // ArmorLevel
        0x1C => Highlight {
            field: Armor,
            low: 0x1,
            high: 0x1_0000,
        },
        // Damage
        0x2C => Highlight {
            field: Weapon,
            low: 0x8,
            high: 0x8_0000,
        },
        // WeaponSkill
        0x31 => Highlight {
            field: Weapon,
            low: 0x4,
            high: 0x4_0000,
        },
        _ => return None,
    })
}

/// Behavior: the 27 **float** properties with a highlight.
///
/// The names for the weapon bits do not line up with the property the client maps them to;
/// this table is what the client *does* (the property names are not what the bits highlight).
#[must_use]
pub fn float_highlight(property: u32) -> Option<Highlight> {
    use HighlightField::{Armor, Resist, Weapon};
    let (field, low) = match property {
        0x0D => (Armor, 0x2),     // ArmorModVsSlash
        0x0E => (Armor, 0x4),     // ArmorModVsPierce
        0x0F => (Armor, 0x8),     // ArmorModVsBludgeon
        0x10 => (Armor, 0x10),    // ArmorModVsCold
        0x11 => (Armor, 0x20),    // ArmorModVsFire
        0x12 => (Armor, 0x40),    // ArmorModVsAcid
        0x13 => (Armor, 0x80),    // ArmorModVsElectric
        0xA5 => (Armor, 0x100),   // ArmorModVsNether
        0x16 => (Weapon, 0x10),   // DamageVariance
        0x1D => (Weapon, 0x2),    // DamageMod
        0x3E => (Weapon, 0x1),    // WeaponDefense
        0x3F => (Weapon, 0x20),   // WeaponMagicDefense
        0x40 => (Resist, 0x1),    // ResistSlash
        0x41 => (Resist, 0x2),    // ResistPierce
        0x42 => (Resist, 0x4),    // ResistBludgeon
        0x43 => (Resist, 0x8),    // ResistFire
        0x44 => (Resist, 0x10),   // ResistCold
        0x45 => (Resist, 0x20),   // ResistAcid
        0x46 => (Resist, 0x40),   // ResistElectric
        0x47 => (Resist, 0x80),   // ResistHealthBoost
        0x48 => (Resist, 0x100),  // ResistStaminaDrain
        0x49 => (Resist, 0x200),  // ResistStaminaBoost
        0x4A => (Resist, 0x400),  // ResistManaDrain
        0x4B => (Resist, 0x800),  // ResistManaBoost
        0x90 => (Resist, 0x1000), // ManaConversionMod
        0x98 => (Resist, 0x2000), // ElementalDamageMod
        0xA6 => (Resist, 0x4000), // ResistNether
        _ => return None,
    };
    Some(Highlight {
        field,
        low,
        high: low << 16,
    })
}

/// How a highlighted stat is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightState {
    /// Not enchanted; draw plainly.
    Plain,
    /// Enchanted and beneficial — green.
    Beneficial,
    /// Enchanted and harmful — red.
    Harmful,
}

/// The hook appraisal bitfield: what a hooked item can be used as.
pub mod hook_appraisal {
    pub const INSCRIBABLE: u32 = 0x1;
    pub const HEALER: u32 = 0x2;
    pub const FOOD: u32 = 0x4;
    pub const LOCKPICK: u32 = 0x8;
}

/// The enchantment bit index — the creature profile's nine attribute bits.
pub mod creature_enchantment {
    pub const STRENGTH: u32 = 0x1;
    pub const ENDURANCE: u32 = 0x2;
    pub const QUICKNESS: u32 = 0x4;
    pub const COORDINATION: u32 = 0x8;
    pub const FOCUS: u32 = 0x10;
    pub const SELF: u32 = 0x20;
    pub const MAX_HEALTH: u32 = 0x40;
    pub const MAX_STAMINA: u32 = 0x80;
    pub const MAX_MANA: u32 = 0x100;
}

/// `AppraisalLongDescDecorations` — the int property of the same name.
pub mod long_desc_decoration {
    pub const PREPEND_WORKMANSHIP: u32 = 0x1;
    pub const PREPEND_MATERIAL: u32 = 0x2;
    pub const APPEND_GEM_INFO: u32 = 0x4;
}

/// The transient appraisal cache: one profile per object the player has assessed.
#[derive(Debug, Clone, Default)]
pub struct AppraisalCache {
    profiles: BTreeMap<ObjectId, AppraisalProfile>,
    /// Which delivery each cached profile came in on.
    ///
    /// The client needs no such thing: its set-appraise-info path is
    /// *called by* the reply, so "a reply arrived" is the call itself. This build's panel is driven
    /// by a per-frame pull off `GameView` (`dereth_ui_screens::panels::examination`), and a pull
    /// cannot otherwise tell a second `0x00C9` carrying an identical profile -- which is exactly
    /// what the client's 0.75-second combat re-poll produces -- from the
    /// same profile being offered again on the next frame. This counter is that difference and
    /// nothing else; it is a seam artefact, declared rather than smuggled.
    deliveries: BTreeMap<ObjectId, u64>,
    /// How many `0x00C9` this cache has taken, in total.
    pub delivered: u64,
    /// The id the player has just asked about and whose reply is still outstanding. Written only
    /// by the examine-object path.
    pub examining: Option<ObjectId>,
    /// How many examine-object requests have started, in total.
    ///
    /// The client needs no such counter: it raises notice `0x5100013` synchronously, so the notice
    /// itself is the call. This
    /// build's identify panel lives in `dereth-ui-screens` and is driven by a per-frame pull off
    /// `GameView`, and a pull comparing `Option<ObjectId>` cannot see **the same object examined
    /// twice** — which is the case that matters, because the examine-object notice handler
    /// clears the current appraisal id, and that is what re-opens a panel the player closed on a second
    /// look at the same thing.
    ///
    /// It is the same seam artefact as [`Self::deliveries`], for the same reason and in the other
    /// direction, and it is declared rather than smuggled.
    pub examine_serial: u64,
    /// When the combat poll last fired.
    last_poll: Option<LocalTime>,
    /// The examined id whose answer has not arrived yet: set by the examine-object path, and
    /// cleared by that object's `0x00C9` or by a cancel. Raising it from none puts the busy
    /// cursor up and clearing it takes it down, so it is the busy cursor's half of an examine.
    pub awaiting_answer: Option<ObjectId>,
}

impl AppraisalCache {
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<&AppraisalProfile> {
        self.profiles.get(&id)
    }

    /// `Item_SetAppraiseInfo` (0x00C9) arrived.
    pub fn set(&mut self, id: ObjectId, p: AppraisalProfile) {
        self.profiles.insert(id, p);
        self.delivered += 1;
        self.deliveries.insert(id, self.delivered);
    }

    /// Which delivery the cached profile for `id` came in on, or 0 for an object never appraised.
    /// See the field.
    #[must_use]
    pub fn delivery(&self, id: ObjectId) -> u64 {
        self.deliveries.get(&id).copied().unwrap_or(0)
    }

    pub fn clear(&mut self, id: ObjectId) {
        self.profiles.remove(&id);
        self.deliveries.remove(&id);
    }

    /// The examination window's per-frame poll: while in combat mode and with a creature or character
    /// panel active, re-send `Item_Appraise` for the current target every **0.75 s**. Out of combat
    /// it does not poll.
    #[must_use]
    pub fn should_repoll(&mut self, in_combat: bool, creature_panel: bool, now: LocalTime) -> bool {
        if !in_combat || !creature_panel || self.examining.is_none() {
            return false;
        }
        match self.last_poll {
            Some(t) if now.0 - t.0 < COMBAT_REASSESS_INTERVAL => false,
            _ => {
                self.last_poll = Some(now);
                true
            }
        }
    }
}

/// How a property is highlighted, given the profile's three bitfields.
#[must_use]
pub fn highlight_state(p: &AppraisalProfile, h: Highlight) -> HighlightState {
    let field = match h.field {
        HighlightField::Armor => p.armor_enchantment,
        HighlightField::Weapon => p.weapon_enchantment,
        HighlightField::Resist => p.resist_enchantment,
    };
    let Some(bits) = field else {
        return HighlightState::Plain;
    };
    if bits & h.low == 0 {
        return HighlightState::Plain;
    }
    if bits & h.high == 0 {
        HighlightState::Harmful
    } else {
        HighlightState::Beneficial
    }
}

/// The assess panel's *strings* — the weapon-speed, clothing-priority and material-name
/// formatters and the rest of the appraisal system's formatting table — are **not** here.
///
/// Two of them are: [`crate::combat::weapon_time_to_string`] and
/// [`crate::combat::damage_type_to_string`], because their tables are known.
/// For the others only the function is known, not its table, so there is nothing to transcribe
/// and nothing is invented in their place; the panel belongs to the presentation layer and needs
/// the tables observed from retail first. What this module owns is the *data*: which properties
/// the profile carries and how each is highlighted.
impl World {
    /// The examine-object notice — store the id and send
    /// `Item_Appraise` (0x00C8).
    ///
    /// Examination is **not** an inventory request and is not gated by the request lock.
    ///
    /// ```text
    /// the examine-object notice:
    ///   if (id == 0) return;
    ///   if (awaiting_appraisal == 0) increment the UI busy count;
    ///   awaiting_appraisal = id;
    ///   send the appraise request for id;
    ///   current_appraisal = 0;
    /// ```
    ///
    /// All four examine routes come through here rather than sending the appraisal request
    /// directly; otherwise `examining` is never written and the request goes out with nothing
    /// waiting for the answer. This and `attempt_appraise` are otherwise **identical**:
    /// `attempt_appraise` is the single line
    /// `req.send(Request::Appraise(ItemAppraise { target }))` and neither is gated by the inventory
    /// lock, so this is that one line plus the two the client's notice handler adds.
    ///
    /// The `id == 0` guard is the client's first line and matters: `ObjectId(0)` is what a pick
    /// that found nothing carries.
    pub fn examine_object(&mut self, req: &mut dyn RequestSink, id: ObjectId) {
        if id.0 == 0 {
            return;
        }
        // The busy cursor goes up for the first outstanding examine only; a second look before
        // the first is answered replaces the id it waits for.
        if self.appraisal.awaiting_answer.is_none() {
            self.magic.busy_count += 1;
        }
        self.appraisal.awaiting_answer = Some(id);
        self.appraisal.examining = Some(id);
        self.appraisal.examine_serial += 1;
        // `current_appraisal = 0` — the client's last line, and here it has to reach the cache
        // as well as the panel. The client holds no profile store: the set-appraise-info path runs
        // **by** the reply, so a profile from an earlier look at this object cannot possibly answer
        // the new question. This build's panel *pulls*, so a stale cached profile would answer it —
        // and would re-open the window on the frame of the notice, before the request had even left
        // the machine. Dropping the entry is what makes the pull see what the client sees: nothing,
        // until the new `0x00C9` lands.
        self.appraisal.clear(id);
        self.attempt_appraise(req, id);
    }

    /// The examine-spell's first block — an appraise request for object `0`,
    /// the **cancel**. It clears the current appraisal id and sends an
    /// appraise request for `0`.
    ///
    /// Retail raises the appraise request from exactly **three** places, and this is the only one
    /// that passes a literal zero: the per-frame poll guards `current_appraisal != 0` and
    /// the examine-object notice returns on a zero id before it gets there. So
    /// `Request::Appraise(0)` has one producer, and this is its game half.
    ///
    /// **It is not [`Self::examine_object`] with a zero id, and it must not become it.** The two
    /// zero-id meanings are different functions in retail: the UI system's examine-object
    /// reads a zero as *arm the examine target mode* and never reaches
    /// the appraise packer at all, while the examine-spell path calls the packer **directly**,
    /// bypassing the examine-object notice.
    ///
    /// ACE answers it as a real cancel and sends nothing back:
    /// `Player::HandleActionIdentifyObject`'s `if (objectGuid == 0)` arm drops both
    /// `RequestedAppraisalTarget` and `CurrentAppraisalTarget` and returns, which also resets the
    /// 5 s repeat-appraisal rate limit for the next look.
    ///
    /// The caller decides *whether* to cancel — that guard is the panel's, and it is
    /// about the **panel's** two ids, which live on the far side of the `GameView` seam
    /// (`dereth_ui_screens::panels::examination::ExaminationPanel`). This function is the
    /// unconditional half, exactly as [`Self::attempt_appraise`] is.
    pub fn cancel_appraisal(&mut self, req: &mut dyn RequestSink) {
        // `awaiting_appraisal = 0`, the client's own line. Here it also stops
        // `AppraisalCache::should_repoll` — the examination panel's 0.75 s combat re-poll —
        // from asking again about the object the player just walked away from.
        self.appraisal.examining = None;
        // An outstanding examine is given up on, so its busy cursor comes down.
        if self.appraisal.awaiting_answer.take().is_some() {
            self.magic.busy_count = self.magic.busy_count.saturating_sub(1);
        }
        self.attempt_appraise(req, ObjectId(0));
    }

    /// `Item_SetAppraiseInfo` (0x00C9) arrived; `Item_AppraiseDone` (0x01CB) follows.
    pub fn set_appraise_info(
        &mut self,
        id: ObjectId,
        p: AppraisalProfile,
        out: &mut dyn NoticeSink,
    ) {
        // The answer to the outstanding examine takes the busy cursor down; an answer about
        // anything else leaves it.
        if id.0 != 0 && self.appraisal.awaiting_answer == Some(id) {
            self.appraisal.awaiting_answer = None;
            self.magic.busy_count = self.magic.busy_count.saturating_sub(1);
        }
        self.appraisal.set(id, p);
        out.emit(Notice::AppraisalReady(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `02-qualities-and-properties.md` §10's highlight table, all 30 rows.
    #[test]
    fn every_documented_property_has_its_bit_pair() {
        // The three ints.
        assert_eq!(
            int_highlight(0x1C),
            Some(Highlight {
                field: HighlightField::Armor,
                low: 0x1,
                high: 0x1_0000
            })
        );
        assert_eq!(
            int_highlight(0x2C),
            Some(Highlight {
                field: HighlightField::Weapon,
                low: 0x8,
                high: 0x8_0000
            })
        );
        assert_eq!(
            int_highlight(0x31),
            Some(Highlight {
                field: HighlightField::Weapon,
                low: 0x4,
                high: 0x4_0000
            })
        );
        assert_eq!(int_highlight(0x99), None);

        // 27 floats, and the high bit is always the low bit shifted 16.
        let floats: Vec<u32> = (0..=0xFFu32)
            .filter(|p| float_highlight(*p).is_some())
            .collect();
        assert_eq!(floats.len(), 27, "{floats:?}");
        for p in floats {
            let h = float_highlight(p).unwrap();
            assert_eq!(h.high, h.low << 16, "property 0x{p:X}");
        }

        // Spot-check the three fields against the document.
        assert_eq!(float_highlight(0x0D).unwrap().field, HighlightField::Armor);
        assert_eq!(
            float_highlight(0xA5).unwrap(),
            Highlight {
                field: HighlightField::Armor,
                low: 0x100,
                high: 0x100_0000
            }
        );
        assert_eq!(
            float_highlight(0x3E).unwrap(),
            Highlight {
                field: HighlightField::Weapon,
                low: 0x1,
                high: 0x1_0000
            }
        );
        assert_eq!(
            float_highlight(0xA6).unwrap(),
            Highlight {
                field: HighlightField::Resist,
                low: 0x4000,
                high: 0x4000_0000
            }
        );
    }

    /// Oracle: §10 — the **low** 16 bits say "highlighted", the **high** 16 say "beneficial".
    #[test]
    fn the_two_halves_of_a_bitfield_mean_highlight_and_beneficial() {
        let h = float_highlight(0x40).unwrap(); // ResistSlash
        let mut p = AppraisalProfile::default();
        assert_eq!(
            highlight_state(&p, h),
            HighlightState::Plain,
            "no bitfield at all"
        );

        p.resist_enchantment = Some(0);
        assert_eq!(highlight_state(&p, h), HighlightState::Plain);
        p.resist_enchantment = Some(h.low);
        assert_eq!(highlight_state(&p, h), HighlightState::Harmful);
        p.resist_enchantment = Some(h.low | h.high);
        assert_eq!(highlight_state(&p, h), HighlightState::Beneficial);
        // The high bit alone does not highlight.
        p.resist_enchantment = Some(h.high);
        assert_eq!(highlight_state(&p, h), HighlightState::Plain);
        // A neighbouring bit does not leak.
        p.resist_enchantment = Some(float_highlight(0x41).unwrap().low);
        assert_eq!(highlight_state(&p, h), HighlightState::Plain);
    }

    /// Oracle: the assess poll — it runs every 0.75 s, in combat mode only.
    #[test]
    fn the_assess_poll_runs_only_in_combat_and_only_every_three_quarters_of_a_second() {
        let mut c = AppraisalCache {
            examining: Some(ObjectId(5)),
            ..Default::default()
        };
        assert!(
            !c.should_repoll(false, true, LocalTime(0.0)),
            "out of combat it does not poll"
        );
        assert!(
            !c.should_repoll(true, false, LocalTime(0.0)),
            "no creature panel, no poll"
        );
        assert!(c.should_repoll(true, true, LocalTime(0.0)));
        assert!(!c.should_repoll(true, true, LocalTime(0.74)));
        assert!(c.should_repoll(true, true, LocalTime(0.75)));
        c.examining = None;
        assert!(!c.should_repoll(true, true, LocalTime(10.0)));
    }

    /// Oracle: §10's sub-profile bit lists.
    #[test]
    fn the_sub_profile_bit_names_match() {
        assert_eq!(hook_appraisal::LOCKPICK, 0x8);
        assert_eq!(creature_enchantment::MAX_MANA, 0x100);
        assert_eq!(long_desc_decoration::APPEND_GEM_INFO, 0x4);
    }

    /// Oracle: the examine-object notice raises the busy count only when no examine is waiting;
    /// the set-appraise-info handler lowers it for the awaited id alone; the examine-spell cancel
    /// lowers it when one is waiting.
    ///
    /// **The busy cursor is up while an examine waits for its answer**, once however many looks
    /// pile up, and an answer about something else leaves it up.
    #[test]
    fn an_examine_holds_the_busy_count_until_its_own_answer_or_a_cancel() {
        let mut w = World::new();
        let mut req = crate::RecordingRequests::default();
        let mut out = crate::RecordingSink::default();
        let (a, b) = (ObjectId(0x8000_0001), ObjectId(0x8000_0002));
        w.examine_object(&mut req, ObjectId(0));
        assert_eq!(w.magic.busy_count, 0, "a look at nothing asks nothing");
        w.examine_object(&mut req, a);
        assert_eq!(w.magic.busy_count, 1);
        w.examine_object(&mut req, b);
        assert_eq!(
            w.magic.busy_count, 1,
            "a second look waits on the same raise"
        );
        w.set_appraise_info(a, AppraisalProfile::default(), &mut out);
        assert_eq!(
            w.magic.busy_count, 1,
            "the answer about the first look is not awaited"
        );
        w.set_appraise_info(b, AppraisalProfile::default(), &mut out);
        assert_eq!(w.magic.busy_count, 0, "the awaited answer takes it down");
        w.examine_object(&mut req, a);
        w.cancel_appraisal(&mut req);
        assert_eq!(w.magic.busy_count, 0, "a cancel gives the wait up");
        w.cancel_appraisal(&mut req);
        assert_eq!(
            w.magic.busy_count, 0,
            "and a second cancel has nothing to give up"
        );
    }
}
