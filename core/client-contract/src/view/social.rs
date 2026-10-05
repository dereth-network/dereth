//! The fellowship, friends, squelch and allegiance read-outs.

use super::*;

/// One fellowship member projected for its panel row.
///
/// The panel never sees a `Fellowship`: the vitals are drawn from the six words directly and the
/// experience share is a **percentage the host computed**, because the two functions behind it —
/// the fellowship's experience-proportion sum and the per-member experience proportion — need
/// the `XpTable`, which is a dat object this crate must not know about. This is the same declared
/// crossing as [`SkillEntry`]'s name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FellowEntry {
    /// The `PackableHashTable` key, and the value of row attribute `0x1000000D`.
    pub id: ObjectId,
    /// The name variable of `ID_Fellowship_FellowName`.
    pub name: String,
    /// The level variable of `ID_Fellowship_FellowStats`.
    pub level: u32,
    /// The experience variable of `ID_Fellowship_FellowStats`, as a **whole percent**.
    ///
    /// The fellowship panel's stat update computes a float and then multiplies by `100.0f` and
    /// truncates, so the number in the box is truncated toward zero.
    /// The float is:
    ///
    /// * `0.0` when the share-XP flag is clear;
    /// * the even-split share for the current member count when `_even_xp_split` is set;
    /// * this member's level-based XP proportion divided by the sum of all members'
    ///   proportions otherwise.
    ///
    /// \[measured\]
    pub xp_percent: i32,
    pub current_health: u32,
    pub max_health: u32,
    pub current_stamina: u32,
    pub max_stamina: u32,
    pub current_mana: u32,
    pub max_mana: u32,
}

/// What the fellowship panel shows when a fellowship exists.
///
/// `None` from [`GameView::fellowship`] means that no current fellowship exists, which
/// is the whole of the client's first branch: the *Not In A Fellowship*
/// frame is shown, the *In A Fellowship* frame is hidden, and the panel registers for global
/// message 3 so it keeps polling.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FellowshipView {
    /// The fellowship name, written into the fellowship-name text (`0x10000276`) as a **literal**
    /// rather than a string-table token.
    pub name: String,
    /// The leader id. The panel compares it against the player's own id
    /// and that comparison decides five of the six buttons.
    pub leader: ObjectId,
    pub share_xp: bool,
    pub even_xp_split: bool,
    /// `_open_fellow` — which caption the Open/Close button carries, and whether a non-leader may
    /// recruit at all.
    pub open_fellow: bool,
    pub locked: bool,
    /// The members, in the order the panel walks them. Retail walks a `PackableHashTable` in
    /// bucket order; the mirror behind this is a `BTreeMap`, so this is **id order** and the rows
    /// are stably sorted rather than arbitrarily so.
    pub members: Vec<FellowEntry>,
}

/// The worn title and every earned title, with names already resolved for the
/// titles panel.
///
/// The join is the host's for the reason [`SkillEntry`]'s name is: it maps the title id through
/// enum `0x10000006`, then hashes the resulting token into string table `0x10000007`, i.e. two dat
/// objects this crate must not know about. Both are loaded at startup already — they are the same
/// pair `display_title()` uses.
///
/// A title whose lookup fails carries the **empty** name rather than being dropped, so that
/// `TitlesPanel::unresolved` can count it: an id the shipped tables do not carry and an id that
/// was never sent look identical once the row is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterTitles {
    /// The displayed title. `0` is "no title", which the title-name lookup refuses outright.
    pub display: u32,
    /// The earned titles, in **arrival** order — the panel does its own alphabetical insert.
    pub titles: Vec<(u32, String)>,
}

/// One friends-list row.
///
/// The received friend record has six fields, but the panel needs only three: name
/// for the text child, id for attribute `0x10000085`, and online state for the text
/// element state. The appear-offline flag and the two friendship-id lists remain
/// in `dereth_client_model::player::Friend`; the panel does not read them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FriendEntry {
    /// The value of row attribute `0x10000085`, which is what the Remove button reads back.
    pub id: ObjectId,
    /// The name written into the row's `0x1000051A` text child.
    pub name: String,
    /// Whether the friend's online flag is nonzero. Decides the row's text state -- `0x10000054` when true,
    /// `0x10000055` when false -- which is in turn what the Tell button and the `@friends online`
    /// listing both read back off the row.
    pub online: bool,
}

/// One squelch-list row.
///
/// The wire `SquelchInfo` has three fields; this view carries only the name and
/// account flag that determine text child `0x10000542` and row state. The iterator
/// uses the 128-bit channel mask only to test emptiness before yielding a row.
/// The mask stays in `dereth_client_model::chat::SquelchEntry` for the chat router.
///
/// There is **no id**, deliberately. The row attribute `0x1000008F` is written with a literal
/// zero by the panel's one production caller and nothing ever reads it back; the Remove button
/// identifies a row by its text. See `dereth_ui_screens::panels::squelch`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SquelchEntry {
    /// The name written into the row's `0x10000542` text child.
    pub name: String,
    /// Whether the zone-squelch flag is nonzero — which ACE calls `Account` and which is what it is.
    /// Decides the row's text state — `0x10000057` when true, `0x10000056` when false — which is
    /// in turn the only thing the Remove button reads to choose between `0x0059` and `0x0058`.
    pub account: bool,
}

/// One allegiance member projected for the panel.
///
/// The host walks the hierarchy's monarch, patron and direct vassals and supplies
/// the four fields the panel reads from each node. The rest of `AllegianceData`
/// stays behind the seam.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllegianceEntry {
    pub id: ObjectId,
    /// The allegiance full name — the rank title, a space, then the name, or the
    /// bare name when the title lookup fails. **Already joined**, because the title tables live in
    /// `dereth-client-model` and this crate does not depend on it.
    pub full_name: String,
    /// The is-logged-in bit — `_bitfield & 1`. The vassals update
    /// reads it twice: once to decide the row's `0x100004AA` state and once, across the whole
    /// list, to enable talk focus 6 when any vassal is logged in.
    pub logged_in: bool,
    /// The number `ID_Allegiance_Rank` prints beside the title.
    pub rank: u16,
    /// The experience this vassal has passed up, the
    /// value variable of `ID_Allegiance_VassalExperiencePassedUp` on element `0x10000269`.
    pub cp_cached: u32,
}

/// The allegiance header, monarch, patron and direct vassals.
///
/// An allegiance the player is not in is [`Self::default`] — `total == 0`, no monarch, no patron,
/// no vassals — which is a state the panel must render *correctly* rather than merely render
/// emptily, and is the only state the capture corpus can witness (see
/// `dereth_ui_screens::panels::allegiance`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllegianceRoster {
    /// The allegiance name, present in version 8 and later and empty when unnamed.
    pub allegiance_name: String,
    /// The total-member count -- the size of the whole allegiance as the
    /// server counts it, monarch included. The monarch row shows **this minus one**
    /// as the monarch's Followers.
    /// It is not the count of records in the message, which is only the player's neighbourhood
    /// and which no panel update reads.
    pub total_members: u32,
    /// The player's total-vassal count for the whole follower subtree. The player row
    /// shows it unchanged.
    pub total_vassals: u32,
    /// The player's own tithed experience. Unlike a vassal row's
    /// [`AllegianceEntry::cp_cached`], this is the value the patron-is-monarch arm puts in child
    /// `0x10000492`.
    pub own_cp_tithed: u32,
    /// The player's own node, when the tree holds one.
    pub subject: Option<AllegianceEntry>,
    /// The player's allegiance-rank quality (int property 30) as the ordinary quality read answers
    /// it: the stored value passed through the int enchantments, since the quality filter lists this
    /// property; **0** when the player carries none, the value the panel seeds its read with.
    ///
    /// The rank line compares it with the tree's rank for [`Self::subject`]: equal (or `-1`)
    /// prints the plain rank, anything else the buffed form with the difference. A spell that
    /// raises the allegiance rank is the only way the two differ, because the shard sends the
    /// stored rank.
    pub player_rank_quality: i32,
    pub monarch: Option<AllegianceEntry>,
    pub patron: Option<AllegianceEntry>,
    /// Newest first: the client inserts at the head of the vassal list
    /// and its first-vassal query returns that head.
    pub vassals: Vec<AllegianceEntry>,
}
