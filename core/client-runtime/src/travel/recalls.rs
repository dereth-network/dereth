//! The character's recalls, learned as the player uses them and from what sets their landing
//! without a use: where each recall the player sets off (a recall command typed, a recall spell
//! cast, an item that casts one used) lands the character, once it has; a portal tie's recall,
//! from the destination of the portal tied to; the lifestone recall's, from where the character
//! stood when a lifestone took its spirit; Lifestone Recall's, from the lifestone tied to. Each is
//! kept in this world's portal store for the character, with the portals.

use dereth_navigation::{global_distance, Recall};
use dereth_primitives::Position;

use super::{unix_now, PlaceSeen, Travel};

/// The linking spells and the recall each sets the landing of: Primary Portal Tie to Primary
/// Portal Recall, Secondary Portal Tie to Secondary Portal Recall, Lifestone Tie to Lifestone
/// Recall.
pub const TIES: [(u32, u32); 3] = [(47, 48), (2646, 2647), (2644, 1635)];

/// The `Communication_WeenieError` (`0x028A`) codes that say a tie took: to a portal, to a
/// lifestone.
const TOOK_PORTAL: u32 = 0x04A1;
const TOOK_LIFESTONE: u32 = 0x049D;

/// A chat line of the magic kind, which a lifestone's word to the character who used it is.
const MAGIC_LINE: u8 = 7;

/// The recall request `r` sets off, if any: a recall command (the lifestone, marketplace, house,
/// mansion or hometown recall), a spell cast on oneself that `recall_spell` says is a recall, or
/// an item used whose spell (`item_spell`, for an item used from the pack) is one.
#[must_use]
pub fn recall_set_off(
    r: &dereth_client_model::Request,
    recall_spell: &dyn Fn(u32) -> bool,
    item_spell: &dyn Fn(dereth_primitives::ObjectId) -> Option<u32>,
) -> Option<Recall> {
    use dereth_client_model::Request as R;
    match r {
        R::TeleToLifestone(_) => Some(Recall::Lifestone),
        R::TeleToMarketplace(_) => Some(Recall::Marketplace),
        R::TeleToHouse(_) => Some(Recall::House),
        R::TeleToMansion(_) => Some(Recall::Mansion),
        R::AllegianceRecallHometown(_) => Some(Recall::Allegiance),
        R::CastUntargetedSpell(m) if recall_spell(m.spell_id) => Some(Recall::Spell(m.spell_id)),
        R::UseEvent(u) => item_spell(u.object)
            .filter(|s| recall_spell(*s))
            .map(Recall::Spell),
        _ => None,
    }
}

/// The recall the player set off, a tie cast and a lifestone used, each until what follows from
/// it has been seen or its time is up.
#[derive(Debug, Default)]
pub(super) struct RecallWatch {
    /// The recall used and when, in seconds of the caller's clock.
    used: Option<(Recall, f64)>,
    /// The recall whose teleport is under way, and where the body stood as it began.
    going: Option<(Recall, Position)>,
    /// A tie cast: the recall it sets, where that recall will land, and when.
    tie: Option<(Recall, Position, f64)>,
    /// A lifestone used, and when.
    attune: Option<(Position, f64)>,
}

impl RecallWatch {
    /// How long after a recall is set off its teleport may begin, in seconds: a spell is cast,
    /// a command's recall played out, first.
    pub const WAIT: f64 = 30.0;
    /// How far the body must land from where it stood, in metres, for the teleport to have
    /// taken it.
    pub const MOVED: f32 = 15.0;
    /// How long after a tie is cast or a lifestone used the server's word of it may come, in
    /// seconds.
    pub const ANSWER: f64 = 15.0;
    /// How near a lifestone the body stands when it takes the character's spirit, in metres.
    pub const NEAR: f32 = 10.0;

    /// The player set `recall` off at `now`.
    pub fn used(&mut self, recall: Recall, now: f64) {
        self.used = Some((recall, now));
        self.going = None;
    }

    /// The player cast a tie at `now` that, once it takes, lands `recall` at `landing`.
    pub fn tied(&mut self, recall: Recall, landing: Position, now: f64) {
        self.tie = Some((recall, landing, now));
    }

    /// The server's error code `code` at `now`: the code that says the tie cast took answers
    /// its recall and landing.
    pub fn error(&mut self, code: u32, now: f64) -> Option<(Recall, Position)> {
        let (recall, _, when) = self.tie.as_ref()?;
        if now - when > Self::ANSWER {
            self.tie = None;
            return None;
        }
        let took = if *recall == Recall::Spell(1635) {
            TOOK_LIFESTONE
        } else {
            TOOK_PORTAL
        };
        if code != took {
            return None;
        }
        let (recall, landing, _) = self.tie.take()?;
        Some((recall, landing))
    }

    /// The player used the lifestone standing at `at`, at `now`.
    pub fn attuning(&mut self, at: Position, now: f64) {
        self.attune = Some((at, now));
    }

    /// A line from the server (`ty` its kind) at `now`, the body at `body`: a lifestone's word
    /// after it was used answers the lifestone recall, landing where the body stands.
    pub fn line(
        &mut self,
        ty: u8,
        body: Option<&Position>,
        now: f64,
    ) -> Option<(Recall, Position)> {
        if self
            .attune
            .as_ref()
            .is_some_and(|(_, when)| now - when > Self::ANSWER)
        {
            self.attune = None;
        }
        if ty == MAGIC_LINE {
            let (stone, _) = self.attune.as_ref()?;
            let body = body?;
            if global_distance(body, stone) <= Self::NEAR {
                self.attune = None;
                return Some((Recall::Lifestone, *body));
            }
        }
        None
    }

    /// The body at `at` at `now`, in portal space or not. Answers the recall set off and where
    /// it landed the character, once it has.
    pub fn frame(
        &mut self,
        at: &Position,
        in_portal_space: bool,
        now: f64,
    ) -> Option<(Recall, Position)> {
        if let Some((_, before)) = &self.going {
            if in_portal_space {
                return None;
            }
            let landed = global_distance(at, before) > Self::MOVED;
            let (recall, _) = self.going.take()?;
            return landed.then_some((recall, *at));
        }
        let (_, when) = self.used.as_ref()?;
        if now - when > Self::WAIT {
            self.used = None;
            return None;
        }
        if in_portal_space {
            let (recall, _) = self.used.take()?;
            self.going = Some((recall, *at));
        }
        None
    }
}

impl Travel {
    /// The player set off `recall` (a command typed, a spell cast, an item that casts one used)
    /// at `now`, by the game clock: where it lands the character is learned once it has.
    pub fn recall_used(&mut self, recall: Recall, now: f64) {
        self.recall_watch.used(recall, now);
    }

    /// The player cast `spell` at `target` at `now`: a portal tie at a portal whose destination
    /// this world's store knows, or the lifestone tie at a lifestone, sets the landing of the
    /// recall it ties once the server says it took.
    pub fn spell_cast_at(&mut self, spell: u32, target: &PlaceSeen, now: f64) {
        let Some(&(_, recall)) = TIES.iter().find(|(tie, _)| *tie == spell) else {
            return;
        };
        let Some(at) = target.at else { return };
        let landing = match target.kind() {
            Some(dereth_navigation::PlaceKind::Lifestone) if recall == 1635 => Some(at),
            Some(dereth_navigation::PlaceKind::Portal) if recall != 1635 => {
                let wcid = (target.wcid != 0).then_some(target.wcid);
                self.portals
                    .portals()
                    .iter()
                    .filter(|e| e.retired.is_none() && e.same_portal(wcid, &at))
                    .filter_map(|e| e.to.map(|to| (e.source, e.verified, to)))
                    .max_by_key(|(source, verified, _)| (*source, *verified))
                    .map(|(_, _, to)| to)
            }
            _ => None,
        };
        if let Some(landing) = landing {
            self.recall_watch.tied(Recall::Spell(recall), landing, now);
        }
    }

    /// The server's error codes this frame, at `now`: the tie they say took is learned for
    /// `character`.
    pub fn server_errors(&mut self, codes: &[u32], now: f64, character: &str) {
        for code in codes {
            if let Some((recall, landing)) = self.recall_watch.error(*code, now) {
                self.learn_recall_for(character, recall, &landing);
            }
        }
    }

    /// The player used the lifestone standing at `at`, at `now`: the lifestone recall lands
    /// where the character stands when the lifestone takes its spirit.
    pub fn lifestone_used(&mut self, at: Position, now: f64) {
        self.recall_watch.attuning(at, now);
    }

    /// The kinds of the server's lines this frame, at `now`, the body at `body`: what a lifestone
    /// said took is learned for `character`.
    pub fn server_lines(
        &mut self,
        kinds: &[u8],
        body: Option<&Position>,
        now: f64,
        character: &str,
    ) {
        for ty in kinds {
            if let Some((recall, landing)) = self.recall_watch.line(*ty, body, now) {
                self.learn_recall_for(character, recall, &landing);
            }
        }
    }

    /// Where the recall the player set off landed the character, once it has.
    pub(super) fn recall_landing(
        &mut self,
        at: &Position,
        portal_space: bool,
        now: f64,
        character: &str,
    ) {
        if let Some((recall, landing)) = self.recall_watch.frame(at, portal_space, now) {
            self.learn_recall_for(character, recall, &landing);
        }
    }

    fn learn_recall_for(&mut self, character: &str, recall: Recall, landing: &Position) {
        if character.is_empty() || self.portals.world.is_empty() {
            return;
        }
        tracing::info!(
            target: "travel",
            "{} lands at 0x{:08X} {:?}; learned",
            recall.word(),
            landing.cell.0,
            landing.frame.origin
        );
        self.portals
            .learn_recall(character, recall, landing, unix_now());
        self.save_store();
    }

    /// The positions the server keeps for the character, by type, as its qualities carry them:
    /// the lifestone it last used (`Sanctuary`, 4) is where the lifestone command lands it, the
    /// lifestone it tied to (`LinkedLifestone`, 15) Lifestone Recall's, and the last portal it
    /// took (`LastPortal`, 9) Portal Recall's. Learned for `character` where they differ from
    /// what the store holds.
    pub fn recall_positions(&mut self, character: &str, positions: &[(u32, Position)]) {
        /// The position types and the recall each is the landing of.
        const LANDINGS: [(u32, Recall); 3] = [
            (4, Recall::Lifestone),
            (15, Recall::Spell(1635)),
            (9, Recall::Spell(super::PORTAL_RECALL)),
        ];
        for (kind, at) in positions {
            let Some((_, recall)) = LANDINGS.iter().find(|(k, _)| k == kind) else {
                continue;
            };
            if self.portals.recall_destination(character, *recall) != Some(*at) {
                self.learn_recall_for(character, *recall, at);
            }
        }
    }

    /// The Marketplace recall lands every character in one place, the Marketplace itself: where
    /// this world's places know it, the character's Marketplace recall lands there, until a use
    /// says otherwise.
    pub(super) fn marketplace_from_places(&mut self) {
        let name = self.character.name.clone();
        if name.is_empty()
            || self
                .portals
                .recall_destination(&name, Recall::Marketplace)
                .is_some()
        {
            return;
        }
        let Some(at) = self
            .places
            .store()
            .places()
            .iter()
            .find(|p| {
                p.kind == dereth_navigation::PlaceKind::Landmark
                    && p.name.eq_ignore_ascii_case("Marketplace")
            })
            .map(|p| p.at)
        else {
            return;
        };
        self.portals
            .learn_recall(&name, Recall::Marketplace, &at, 0);
    }
}

#[cfg(test)]
#[path = "recalls_tests.rs"]
mod tests;
