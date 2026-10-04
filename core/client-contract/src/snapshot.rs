//! An **owned** [`GameView`], produced once per frame.
//!
//! `dereth_client::hud::HudView` is the right idea with the wrong lifetime for a real boundary:
//! it is `{ hud: &Hud, world: &World }`, so every `&str` and `&[T]` it hands back
//! points into the caller's own live state. In process that is free. Out of process, across a C
//! ABI, or one frame behind, it cannot exist at all.
//!
//! [`GameSnapshot`] is the same 115 answers with the borrows resolved: `String` where the trait
//! returns `&str`, `Vec<T>` where it returns `&[T]`, and a map per keyed method. It is built by
//! calling the methods once — [`GameSnapshot::from_view`] takes any `&dyn GameView` — so there is
//! no second transcription of the client's accessors to drift from the first, and it implements
//! [`GameView`] itself, so anything that takes `&dyn GameView` takes a snapshot unchanged.
//!
//! **Only a null-presentation test reads one so far**; the panels still read the live view.
//!
//! ## What a snapshot can and cannot hold
//!
//! Most of the methods take no argument beyond `&self`, and those are simply fields.
//!
//! The rest are keyed, and a snapshot can only carry a key it can *enumerate*. Four key sets are
//! enumerable from the view itself and are captured in full:
//!
//! | key set | where it comes from |
//! |---|---|
//! | object ids | [`GameSnapshot::object_ids`] — the player, the selection, the radar, the container tree, the equipment, the shop and trade rows, the shortcut bar, the endowment and the examine target |
//! | spell ids | [`GameView::spellbook`] plus the eight [`GameView::spell_tab`] bars |
//! | skill ids | [`GameView::skills`] |
//! | spell-component ids | [`GameView::spell_components`] |
//!
//! Three more are closed sets that are not on the view — [`PlayerOption::ALL`], the six attribute
//! ids, and the eighteen shortcut slots — and are captured over the constants below.
//!
//! **Ten methods are not captured**, because their arguments are open sets: an owned snapshot
//! would have to guess. [`GameSnapshot`] leaves them at the trait's own default body, which is
//! the same `None`/`0`/`false` an [`EmptyGameView`] gives, and a caller that needs one of them
//! needs the live view:
//!
//! * [`GameView::int_stat`] and [`GameView::gameplay_option_float`] — an arbitrary property id.
//! * [`GameView::chat_window_filter`] — the window ids come from the player module's placement
//!   array, which this crate cannot read.
//! * [`GameView::cooldown_remaining`] — takes `now`, so it is a function of the clock and not a
//!   fact about the frame (`GameView` carries no clock).
//! * [`GameView::salvage_item_suitable`] and
//!   [`GameView::external_container_drag_item_acceptable`] — a second open argument.
//! * [`GameView::slumlord_payment`], [`GameView::slumlord_needs_more`] and
//!   [`GameView::slumlord_pay`] — they take an arbitrary list of drops.
//!
//! [`GameView::pending_row`] is captured for the containers this snapshot knows and for the
//! `None` container; any other container falls through to the default.

use std::collections::BTreeMap;

use dereth_primitives::{DataId, ObjectId};

use crate::ctime::UtcOffsetSecs;
use crate::journal::JournalIdentity;
use crate::statmgmt::XpHeader;
use crate::view::{
    AllegianceRoster, AppraisalView, AttributeAdvancement, BarberView, BookView, CharacterInfo,
    CharacterTitles, CombatBar, ComponentCategory, ContractEntry, EffectEntry, EmptyGameView,
    EraView, FellowshipView, FriendEntry, GameView, HouseDataView, HousePurchaseView, MiniGameView,
    PkStatus, PlayerOption, RadarEntry, SelectionQueryFacts, ShopView, SkillAdvancement,
    SkillEntry, SlotDecoration, SlumlordView, SpellEntry, SpellExamineView, SquelchEntry,
    TradeView, VitaeDisplay, Vital,
};

/// `dereth_client_model::player::SHORTCUT_SLOTS` — the toolbar's fixed slot count.
///
/// Repeated here for the same reason [`crate::linkstatus::INITIAL_PACKET_LOSS`] is: the
/// definition belongs to the crate that owns the player module, and this crate does not depend on
/// it.
pub const SHORTCUT_SLOTS: u32 = 18;

/// `dereth_client_model::player::SPELL_TABS` — the spellbook's fixed bar count. See [`SHORTCUT_SLOTS`].
pub const SPELL_TABS: usize = 8;

/// The attribute ids accepted by both primary- and secondary-attribute queries: `1..=6`.
///
/// Primary is Strength…Self; secondary is the three vitals as current/max pairs, which is the
/// same `1..=6` space [`Vital::stats`] indexes.
pub const ATTRIBUTE_IDS: std::ops::RangeInclusive<u32> = 1..=6;

/// Everything [`GameView`] answers about one object, owned.
#[derive(Debug, Clone, Default)]
pub struct ObjectSnapshot {
    /// [`GameView::name`].
    pub name: Option<String>,
    /// [`GameView::icon`].
    pub icon: Option<DataId>,
    /// [`GameView::vital`], in [`Vital::ALL`] order.
    pub vitals: [Option<(u32, u32)>; 3],
    /// [`GameView::container_contents`].
    pub container_contents: Vec<ObjectId>,
    /// [`GameView::contained_containers`].
    pub contained_containers: Vec<ObjectId>,
    /// [`GameView::equipment`].
    pub equipment: Vec<(ObjectId, u32)>,
    /// [`GameView::items_capacity`].
    pub items_capacity: Option<i32>,
    /// [`GameView::containers_capacity`].
    pub containers_capacity: Option<i32>,
    /// [`GameView::item_waiting`].
    pub item_waiting: bool,
    /// [`GameView::selection_query_facts`].
    pub selection_query_facts: Option<SelectionQueryFacts>,
    /// [`GameView::slot_decoration`].
    pub slot_decoration: Option<SlotDecoration>,
    /// [`GameView::plural_name`].
    pub plural_name: Option<String>,
    /// [`GameView::appraisal`].
    pub appraisal: Option<AppraisalView>,
    /// [`GameView::inscription_mouse_facts`].
    pub inscription_mouse_facts: Option<(bool, u32)>,
    /// [`GameView::item_owned_by_player`].
    pub item_owned_by_player: bool,
    /// [`GameView::item_wcid`].
    pub item_wcid: u32,
    /// [`GameView::item_trade_note_value`].
    pub item_trade_note_value: Option<i32>,
    /// [`GameView::item_house_payment`].
    pub item_house_payment: i32,
    /// [`GameView::item_material_type`].
    pub item_material_type: u32,
    /// [`GameView::vendor_drag_item_accepted`].
    pub vendor_drag_item_accepted: bool,
    /// [`GameView::trade_drag_item_acceptable`].
    pub trade_drag_item_acceptable: bool,
    /// [`GameView::item_valid_locations`].
    pub item_valid_locations: Option<u32>,
    /// Shared equipment drag feedback.
    pub equipment_hover: crate::view::EquipmentHover,
    /// [`GameView::item_useable_self_target`].
    pub item_useable_self_target: bool,
    /// [`GameView::item_target_compatible`].
    pub item_target_compatible: bool,
    /// [`GameView::allegiance_has_member`].
    pub allegiance_has_member: bool,
    /// [`GameView::object_is_owned_component`].
    pub object_is_owned_component: Option<u32>,
    /// [`GameView::pending_row`] with this object as the container, for both list halves:
    /// `[containers_list = false, containers_list = true]`.
    pub pending_row: [Option<(ObjectId, u32)>; 2],
}

/// Everything [`GameView`] answers about one spell id, owned.
#[derive(Debug, Clone, Default)]
pub struct SpellSnapshot {
    /// [`GameView::spell`].
    pub entry: Option<SpellEntry>,
    /// [`GameView::is_spell_known`].
    pub known: bool,
    /// [`GameView::spell_examine`].
    pub examine: Option<SpellExamineView>,
    /// [`GameView::spell_is_untargeted`].
    pub untargeted: bool,
    /// [`GameView::spell_target_compatible`].
    pub target_compatible: bool,
}

/// An owned [`GameView`], taken once per frame. See the module header.
#[derive(Debug, Clone, Default)]
pub struct GameSnapshot {
    // ---- the nullary reads, one field each --------------------------------------------------
    /// [`GameView::player`].
    pub player: Option<ObjectId>,
    /// [`GameView::open_inventory_container`].
    pub open_inventory_container: Option<ObjectId>,
    /// [`GameView::era`].
    pub era: Option<EraView>,
    /// Resolved presentation facts, including facts that a view supplies directly.
    pub resolved_era_features: Option<dereth_primitives::EraFeatures>,
    pub resolved_era_ui: Option<crate::era::EraUiFacts>,
    pub aetheria_slots: u8,
    /// [`GameView::selection`].
    pub selection: Option<ObjectId>,
    /// [`GameView::selected_object`].
    pub selected_object: Option<ObjectId>,
    /// [`GameView::radar_objects`].
    pub radar_objects: Vec<RadarEntry>,
    /// [`GameView::radar_blank`].
    pub radar_blank: bool,
    /// [`GameView::load`].
    pub load: Option<f32>,
    /// [`GameView::enchantment_counts`].
    pub enchantment_counts: (u32, u32),
    /// [`GameView::active_effects`].
    pub active_effects: Vec<EffectEntry>,
    /// [`GameView::vitae`].
    pub vitae: Option<f32>,
    /// [`GameView::vitae_display`].
    pub vitae_display: Option<VitaeDisplay>,
    /// [`GameView::character_info`].
    pub character_info: Option<CharacterInfo>,
    /// [`GameView::now`].
    pub now: f64,
    /// [`GameView::ping_returns`].
    pub ping_returns: u64,
    /// [`GameView::packet_loss_percent`].
    pub packet_loss_percent: f32,
    /// [`GameView::portal_storm_level`].
    pub portal_storm_level: f32,
    /// [`GameView::link_status`].
    pub link_status: Option<f64>,
    /// [`GameView::selected_meters`].
    pub selected_meters: (Option<f32>, Option<f32>),
    /// [`GameView::player_coords`].
    pub player_coords: Option<(f32, f32)>,
    /// [`GameView::player_outside`].
    pub player_outside: bool,
    /// [`GameView::player_heading`].
    pub player_heading: f32,
    /// [`GameView::game_date_time`].
    pub game_date_time: Option<(String, String)>,
    /// [`GameView::utc_offset_secs`].
    pub utc_offset_secs: UtcOffsetSecs,
    /// [`GameView::skills`].
    pub skills: Vec<SkillEntry>,
    /// [`GameView::spellbook`].
    pub spellbook: Vec<SpellEntry>,
    /// [`GameView::spell_filters`].
    pub spell_filters: u32,
    /// [`GameView::spell_tab`], slots `0..`[`SPELL_TABS`].
    pub spell_tabs: Vec<Vec<u32>>,
    /// [`GameView::spell_components`].
    pub spell_components: Vec<ComponentCategory>,
    /// [`GameView::component_serial`].
    pub component_serial: u64,
    /// [`GameView::character_name`].
    pub character_name: Option<String>,
    /// [`GameView::journal_identity`].
    pub journal: crate::journal::JournalView,
    pub journal_identity: Option<JournalIdentity>,
    /// [`GameView::experience_header`].
    pub experience_header: Option<XpHeader>,
    /// [`GameView::gender_heritage_display`].
    pub gender_heritage_display: Option<String>,
    /// [`GameView::display_title`].
    pub display_title: Option<String>,
    /// [`GameView::character_titles`].
    pub character_titles: CharacterTitles,
    /// [`GameView::pk_status`].
    pub pk_status: PkStatus,
    /// [`GameView::luminance`].
    pub luminance: (i64, i64),
    /// [`GameView::skill_credits`].
    pub skill_credits: i64,
    /// [`GameView::available_experience`].
    pub available_experience: i64,
    /// [`GameView::endowment`].
    pub endowment: Option<(ObjectId, u32)>,
    /// [`GameView::examine_request`].
    pub examine_request: Option<(ObjectId, u64)>,
    /// [`GameView::open_book`].
    pub open_book: Option<BookView>,
    pub book_session: crate::book::BookSessionView,
    /// [`GameView::barber`].
    pub barber: Option<BarberView>,
    /// [`GameView::allegiance_roster`].
    pub oath_xp_cost: Option<u32>,
    pub allegiance_roster: AllegianceRoster,
    /// [`GameView::allegiance_update_aborts`].
    pub allegiance_update_aborts: u64,
    /// [`GameView::allegiance_monarch_quality`].
    pub allegiance_monarch_quality: Option<ObjectId>,
    /// [`GameView::fellowship`].
    pub fellowship: Option<FellowshipView>,
    /// [`GameView::friends`].
    pub friends: Vec<FriendEntry>,
    /// [`GameView::squelch_list`].
    pub squelch_list: Vec<SquelchEntry>,
    /// [`GameView::contracts`].
    pub contracts: Vec<ContractEntry>,
    /// [`GameView::house_status_notices`].
    pub house_status_notices: u64,
    /// [`GameView::house_data`].
    pub house_data: Option<HouseDataView>,
    /// [`GameView::house_data_notices`].
    pub house_data_notices: u64,
    /// [`GameView::house_purchase`].
    pub house_purchase: HousePurchaseView,
    /// [`GameView::shop`].
    pub shop: ShopView,
    /// [`GameView::trade`].
    pub trade: TradeView,
    /// [`GameView::slumlord`].
    pub payment_lists: crate::panels::slumlord::PaymentListsView,
    pub salvage_list: crate::panels::salvage::SalvageListView,
    pub slumlord: Option<SlumlordView>,
    /// [`GameView::slumlord_notices`].
    pub slumlord_notices: u64,
    /// [`GameView::minigame`].
    pub minigame: Option<MiniGameView>,
    /// [`GameView::max_split_size`].
    pub max_split_size: i32,
    /// [`GameView::split_size`].
    pub split_size: i32,
    /// [`GameView::combat_mode`].
    pub combat_mode: u32,
    /// [`GameView::advanced_combat_ui`].
    pub advanced_combat_ui: bool,
    /// [`GameView::combat_bar`].
    pub combat_bar: CombatBar,
    /// [`GameView::recklessness_advancement_class`].
    pub recklessness_advancement_class: u32,

    // ---- the keyed reads, one map each ------------------------------------------------------
    /// [`GameView::shortcut`] for slots `0..`[`SHORTCUT_SLOTS`].
    pub shortcuts: Vec<Option<ObjectId>>,
    /// Every object [`GameSnapshot::object_ids`] reached.
    pub objects: BTreeMap<ObjectId, ObjectSnapshot>,
    /// Every spell id the spellbook and the eight bars name.
    pub spells: BTreeMap<u32, SpellSnapshot>,
    /// [`GameView::skill_advancement`] for every id in [`GameView::skills`].
    pub skill_advancement: BTreeMap<u32, SkillAdvancement>,
    /// [`GameView::attribute`] over [`ATTRIBUTE_IDS`].
    pub attributes: BTreeMap<u32, i32>,
    /// [`GameView::attribute_advancement`] over [`ATTRIBUTE_IDS`] × `{primary, secondary}`.
    pub attribute_advancement: BTreeMap<(u32, bool), AttributeAdvancement>,
    /// [`GameView::player_option`] and [`GameView::player_option_default`] over
    /// [`PlayerOption::ALL`].
    pub player_options: BTreeMap<PlayerOption, (bool, Option<bool>)>,
    /// [`GameView::component_object_id`] and [`GameView::component_is_owned`] for every `wcid` in
    /// [`GameView::spell_components`].
    pub components: BTreeMap<u32, (Option<ObjectId>, bool)>,
    /// [`GameView::pending_row`] with no container, `[false, true]`.
    pub pending_row_no_container: [Option<(ObjectId, u32)>; 2],
}

impl GameSnapshot {
    /// The object ids a snapshot copies per-object answers for.
    ///
    /// Deliberately a *reachability* walk and not "every object in the world": what a frame of UI
    /// reads is the player, what he is carrying and wearing, what is on the radar and the
    /// shortcut bar, what he has selected or is examining, and the rows of whatever vendor or
    /// trade window is open. An id outside that set is one no panel asks about, and an owned
    /// snapshot that copied the whole object table would be paying for the landscape.
    #[must_use]
    pub fn object_ids(view: &dyn GameView) -> Vec<ObjectId> {
        let mut ids: Vec<ObjectId> = Vec::new();
        fn push(ids: &mut Vec<ObjectId>, id: ObjectId) {
            if id.0 != 0 && !ids.contains(&id) {
                ids.push(id);
            }
        }
        for id in [
            view.player(),
            view.open_inventory_container(),
            view.selection(),
            view.selected_object(),
        ]
        .into_iter()
        .flatten()
        {
            push(&mut ids, id);
        }
        if let Some((id, _)) = view.endowment() {
            push(&mut ids, id);
        }
        if let Some((id, _)) = view.examine_request() {
            push(&mut ids, id);
        }
        for e in view.radar_objects() {
            push(&mut ids, e.id);
        }
        // The container tree: the pack the player is, the side packs, and what is in each.
        if let Some(p) = view.player() {
            let mut holders = vec![p];
            holders.extend_from_slice(view.contained_containers(p));
            for h in holders {
                push(&mut ids, h);
                for id in view.container_contents(h) {
                    push(&mut ids, *id);
                }
                for (id, _) in view.equipment(h) {
                    push(&mut ids, *id);
                }
            }
        }
        let shop = view.shop();
        for row in shop
            .stock
            .iter()
            .chain(&shop.buy_list)
            .chain(&shop.sell_list)
        {
            push(&mut ids, row.item);
        }
        if let Some(v) = shop.vendor {
            push(&mut ids, v);
        }
        let trade = view.trade();
        for row in trade.self_rows.iter().chain(&trade.partner_rows) {
            push(&mut ids, row.item);
        }
        if let Some(p) = trade.partner {
            push(&mut ids, p);
        }
        for slot in 0..SHORTCUT_SLOTS {
            if let Some(id) = view.shortcut(slot) {
                push(&mut ids, id);
            }
        }
        ids
    }
}

impl GameSnapshot {
    /// Take the snapshot: call each captured method once and keep the answer.
    #[must_use]
    #[allow(clippy::too_many_lines)] // One statement per method; splitting it would hide the list.
    pub fn from_view(view: &dyn GameView) -> Self {
        let shop = view.shop();
        let trade = view.trade();
        let skills = view.skills().to_vec();
        let spellbook = view.spellbook().to_vec();
        let spell_tabs: Vec<Vec<u32>> = (0..SPELL_TABS)
            .map(|t| view.spell_tab(t).to_vec())
            .collect();
        let spell_components = view.spell_components();

        let mut objects = BTreeMap::new();
        for id in Self::object_ids(view) {
            let mut vitals = [None; 3];
            for (i, which) in Vital::ALL.into_iter().enumerate() {
                vitals[i] = view.vital(id, which);
            }
            objects.insert(
                id,
                ObjectSnapshot {
                    name: view.name(id).map(str::to_owned),
                    icon: view.icon(id),
                    vitals,
                    container_contents: view.container_contents(id).to_vec(),
                    contained_containers: view.contained_containers(id).to_vec(),
                    equipment: view.equipment(id).to_vec(),
                    items_capacity: view.items_capacity(id),
                    containers_capacity: view.containers_capacity(id),
                    item_waiting: view.item_waiting(id),
                    selection_query_facts: view.selection_query_facts(id),
                    slot_decoration: view.slot_decoration(id),
                    plural_name: view.plural_name(id).map(str::to_owned),
                    appraisal: view.appraisal(id),
                    inscription_mouse_facts: view.inscription_mouse_facts(id),
                    item_owned_by_player: view.item_owned_by_player(id),
                    item_wcid: view.item_wcid(id),
                    item_trade_note_value: view.item_trade_note_value(id),
                    item_house_payment: view.item_house_payment(id),
                    item_material_type: view.item_material_type(id),
                    vendor_drag_item_accepted: view.vendor_drag_item_accepted(id),
                    trade_drag_item_acceptable: view.trade_drag_item_acceptable(id),
                    item_valid_locations: view.item_valid_locations(id),
                    equipment_hover: view.equipment_hover(id),
                    item_useable_self_target: view.item_useable_self_target(id),
                    item_target_compatible: view.item_target_compatible(id),
                    allegiance_has_member: view.allegiance_has_member(id),
                    object_is_owned_component: view.object_is_owned_component(id),
                    pending_row: [
                        view.pending_row(Some(id), false),
                        view.pending_row(Some(id), true),
                    ],
                },
            );
        }

        let mut spells: BTreeMap<u32, SpellSnapshot> = BTreeMap::new();
        let spell_ids = spellbook
            .iter()
            .map(|s| s.id)
            .chain(spell_tabs.iter().flatten().copied())
            .chain(view.endowment().map(|(_, spell)| spell));
        for sid in spell_ids {
            spells.entry(sid).or_insert_with(|| SpellSnapshot {
                entry: view.spell(sid),
                known: view.is_spell_known(sid),
                examine: view.spell_examine(sid),
                untargeted: view.spell_is_untargeted(sid),
                target_compatible: view.spell_target_compatible(sid),
            });
        }

        let mut skill_advancement = BTreeMap::new();
        for s in &skills {
            if let Some(a) = view.skill_advancement(s.id) {
                skill_advancement.insert(s.id, a);
            }
        }

        let mut attributes = BTreeMap::new();
        let mut attribute_advancement = BTreeMap::new();
        for id in ATTRIBUTE_IDS {
            if let Some(v) = view.attribute(id) {
                attributes.insert(id, v);
            }
            for secondary in [false, true] {
                if let Some(a) = view.attribute_advancement(id, secondary) {
                    attribute_advancement.insert((id, secondary), a);
                }
            }
        }

        let mut player_options = BTreeMap::new();
        for o in PlayerOption::ALL {
            player_options.insert(o, (view.player_option(o), view.player_option_default(o)));
        }

        let mut components = BTreeMap::new();
        for row in spell_components.iter().flat_map(|c| &c.rows) {
            components.insert(
                row.wcid,
                (
                    view.component_object_id(row.wcid),
                    view.component_is_owned(row.wcid),
                ),
            );
        }

        Self {
            era: view.era().cloned(),
            resolved_era_features: Some(view.era_features()),
            resolved_era_ui: Some(view.era_ui()),
            aetheria_slots: view.aetheria_slots(),
            player: view.player(),
            open_inventory_container: view.open_inventory_container(),
            selection: view.selection(),
            selected_object: view.selected_object(),
            radar_objects: view.radar_objects().to_vec(),
            radar_blank: view.radar_blank(),
            load: view.load(),
            enchantment_counts: view.enchantment_counts(),
            active_effects: view.active_effects(),
            vitae: view.vitae(),
            vitae_display: view.vitae_display(),
            character_info: view.character_info(),
            now: view.now(),
            ping_returns: view.ping_returns(),
            packet_loss_percent: view.packet_loss_percent(),
            portal_storm_level: view.portal_storm_level(),
            link_status: view.link_status(),
            selected_meters: view.selected_meters(),
            player_coords: view.player_coords(),
            player_outside: view.player_outside(),
            player_heading: view.player_heading(),
            game_date_time: view.game_date_time(),
            utc_offset_secs: view.utc_offset_secs(),
            skills,
            spellbook,
            spell_filters: view.spell_filters(),
            spell_tabs,
            spell_components,
            component_serial: view.component_serial(),
            character_name: view.character_name().map(str::to_owned),
            journal: view.journal(),
            journal_identity: view.journal_identity(),
            experience_header: view.experience_header(),
            gender_heritage_display: view.gender_heritage_display(),
            display_title: view.display_title(),
            character_titles: view.character_titles(),
            pk_status: view.pk_status(),
            luminance: view.luminance(),
            skill_credits: view.skill_credits(),
            available_experience: view.available_experience(),
            endowment: view.endowment(),
            examine_request: view.examine_request(),
            open_book: view.open_book(),
            book_session: view.book_session(),
            barber: view.barber(),
            oath_xp_cost: view.oath_xp_cost(),
            allegiance_roster: view.allegiance_roster(),
            allegiance_update_aborts: view.allegiance_update_aborts(),
            allegiance_monarch_quality: view.allegiance_monarch_quality(),
            fellowship: view.fellowship(),
            friends: view.friends(),
            squelch_list: view.squelch_list(),
            contracts: view.contracts(),
            house_status_notices: view.house_status_notices(),
            house_data: view.house_data(),
            house_data_notices: view.house_data_notices(),
            house_purchase: view.house_purchase(),
            shop,
            trade,
            payment_lists: view.payment_lists(),
            salvage_list: view.salvage_list(),
            slumlord: view.slumlord(),
            slumlord_notices: view.slumlord_notices(),
            minigame: view.minigame(),
            max_split_size: view.max_split_size(),
            split_size: view.split_size(),
            combat_mode: view.combat_mode(),
            advanced_combat_ui: view.advanced_combat_ui(),
            combat_bar: view.combat_bar(),
            recklessness_advancement_class: view.recklessness_advancement_class(),
            shortcuts: (0..SHORTCUT_SLOTS).map(|s| view.shortcut(s)).collect(),
            objects,
            spells,
            skill_advancement,
            attributes,
            attribute_advancement,
            player_options,
            components,
            pending_row_no_container: [view.pending_row(None, false), view.pending_row(None, true)],
        }
    }

    /// The snapshot of an [`EmptyGameView`] — "nothing is loaded", taken the same way.
    #[must_use]
    pub fn empty() -> Self {
        Self::from_view(&EmptyGameView)
    }

    fn object(&self, id: ObjectId) -> Option<&ObjectSnapshot> {
        self.objects.get(&id)
    }
}

/// The point of the type: a snapshot *is* a `GameView`, so every existing consumer takes one.
///
/// The ten open-keyed methods listed in the module header are not overridden and therefore answer
/// with the trait's own default body.
impl GameView for GameSnapshot {
    fn era(&self) -> Option<&EraView> {
        self.era.as_ref()
    }
    fn era_features(&self) -> dereth_primitives::EraFeatures {
        self.resolved_era_features.unwrap_or_else(|| {
            self.era.as_ref().map_or(
                dereth_primitives::EraFeatures::END_OF_RETAIL,
                EraView::features,
            )
        })
    }
    fn era_ui(&self) -> crate::era::EraUiFacts {
        self.resolved_era_ui.unwrap_or_else(|| {
            crate::era::EraUiFacts::for_profile(
                self.era
                    .as_ref()
                    .map_or(dereth_primitives::EraId::Eor, |e| e.era),
            )
        })
    }
    fn aetheria_slots(&self) -> u8 {
        self.aetheria_slots
    }
    fn player(&self) -> Option<ObjectId> {
        self.player
    }
    fn open_inventory_container(&self) -> Option<ObjectId> {
        self.open_inventory_container
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        self.object(id)?.name.as_deref()
    }
    fn icon(&self, id: ObjectId) -> Option<DataId> {
        self.object(id)?.icon
    }
    fn vital(&self, id: ObjectId, which: Vital) -> Option<(u32, u32)> {
        let i = Vital::ALL.iter().position(|v| *v == which)?;
        self.object(id)?.vitals[i]
    }
    fn container_contents(&self, id: ObjectId) -> &[ObjectId] {
        self.object(id)
            .map_or(&[], |o| o.container_contents.as_slice())
    }
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        self.object(id)
            .map_or(&[], |o| o.contained_containers.as_slice())
    }
    fn equipment(&self, id: ObjectId) -> &[(ObjectId, u32)] {
        self.object(id).map_or(&[], |o| o.equipment.as_slice())
    }
    fn items_capacity(&self, id: ObjectId) -> Option<i32> {
        self.object(id)?.items_capacity
    }
    fn containers_capacity(&self, id: ObjectId) -> Option<i32> {
        self.object(id)?.containers_capacity
    }
    fn item_waiting(&self, id: ObjectId) -> bool {
        self.object(id).is_some_and(|o| o.item_waiting)
    }
    fn pending_row(
        &self,
        container: Option<ObjectId>,
        containers_list: bool,
    ) -> Option<(ObjectId, u32)> {
        let i = usize::from(containers_list);
        match container {
            None => self.pending_row_no_container[i],
            Some(id) => self.object(id)?.pending_row[i],
        }
    }
    fn selection(&self) -> Option<ObjectId> {
        self.selection
    }
    fn radar_objects(&self) -> &[RadarEntry] {
        &self.radar_objects
    }
    fn load(&self) -> Option<f32> {
        self.load
    }
    fn enchantment_counts(&self) -> (u32, u32) {
        self.enchantment_counts
    }
    fn active_effects(&self) -> Vec<EffectEntry> {
        self.active_effects.clone()
    }
    fn vitae(&self) -> Option<f32> {
        self.vitae
    }
    fn vitae_display(&self) -> Option<VitaeDisplay> {
        self.vitae_display
    }
    fn character_info(&self) -> Option<CharacterInfo> {
        self.character_info.clone()
    }
    fn now(&self) -> f64 {
        self.now
    }
    fn ping_returns(&self) -> u64 {
        self.ping_returns
    }
    fn packet_loss_percent(&self) -> f32 {
        self.packet_loss_percent
    }
    fn portal_storm_level(&self) -> f32 {
        self.portal_storm_level
    }
    fn player_option(&self, o: PlayerOption) -> bool {
        self.player_options.get(&o).is_some_and(|(v, _)| *v)
    }
    fn player_option_default(&self, o: PlayerOption) -> Option<bool> {
        self.player_options.get(&o).and_then(|(_, d)| *d)
    }
    fn selection_query_facts(&self, id: ObjectId) -> Option<SelectionQueryFacts> {
        self.object(id)?.selection_query_facts
    }
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        self.selected_meters
    }
    fn player_coords(&self) -> Option<(f32, f32)> {
        self.player_coords
    }
    fn game_date_time(&self) -> Option<(String, String)> {
        self.game_date_time.clone()
    }
    fn player_outside(&self) -> bool {
        self.player_outside
    }
    fn player_heading(&self) -> f32 {
        self.player_heading
    }
    fn radar_blank(&self) -> bool {
        self.radar_blank
    }
    fn link_status(&self) -> Option<f64> {
        self.link_status
    }
    fn shortcut(&self, slot: u32) -> Option<ObjectId> {
        *self.shortcuts.get(usize::try_from(slot).ok()?)?
    }
    fn skills(&self) -> &[SkillEntry] {
        &self.skills
    }
    fn spellbook(&self) -> &[SpellEntry] {
        &self.spellbook
    }
    fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
        self.spells.get(&spell_id)?.entry.clone()
    }
    fn is_spell_known(&self, spell_id: u32) -> bool {
        self.spells.get(&spell_id).is_some_and(|s| s.known)
    }
    fn spell_examine(&self, spell_id: u32) -> Option<SpellExamineView> {
        self.spells.get(&spell_id)?.examine.clone()
    }
    fn component_object_id(&self, scid: u32) -> Option<ObjectId> {
        self.components.get(&scid)?.0
    }
    fn component_is_owned(&self, scid: u32) -> bool {
        self.components.get(&scid).is_some_and(|c| c.1)
    }
    fn component_serial(&self) -> u64 {
        self.component_serial
    }
    fn attribute(&self, id: u32) -> Option<i32> {
        self.attributes.get(&id).copied()
    }
    fn skill_advancement(&self, id: u32) -> Option<SkillAdvancement> {
        self.skill_advancement.get(&id).cloned()
    }
    fn attribute_advancement(&self, id: u32, secondary: bool) -> Option<AttributeAdvancement> {
        self.attribute_advancement.get(&(id, secondary)).cloned()
    }
    fn character_name(&self) -> Option<&str> {
        self.character_name.as_deref()
    }
    fn journal(&self) -> crate::journal::JournalView {
        self.journal.clone()
    }
    fn journal_identity(&self) -> Option<JournalIdentity> {
        self.journal_identity.clone()
    }
    fn experience_header(&self) -> Option<XpHeader> {
        self.experience_header
    }
    fn gender_heritage_display(&self) -> Option<String> {
        self.gender_heritage_display.clone()
    }
    fn display_title(&self) -> Option<String> {
        self.display_title.clone()
    }
    fn character_titles(&self) -> CharacterTitles {
        self.character_titles.clone()
    }
    fn pk_status(&self) -> PkStatus {
        self.pk_status
    }
    fn luminance(&self) -> (i64, i64) {
        self.luminance
    }
    fn skill_credits(&self) -> i64 {
        self.skill_credits
    }
    fn available_experience(&self) -> i64 {
        self.available_experience
    }
    fn spell_filters(&self) -> u32 {
        self.spell_filters
    }
    fn spell_tab(&self, tab: usize) -> &[u32] {
        self.spell_tabs.get(tab).map_or(&[], Vec::as_slice)
    }
    fn endowment(&self) -> Option<(ObjectId, u32)> {
        self.endowment
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        self.object(id)?.slot_decoration
    }
    fn plural_name(&self, id: ObjectId) -> Option<&str> {
        self.object(id)?.plural_name.as_deref()
    }
    fn appraisal(&self, id: ObjectId) -> Option<AppraisalView> {
        self.object(id)?.appraisal.clone()
    }
    fn inscription_mouse_facts(&self, id: ObjectId) -> Option<(bool, u32)> {
        self.object(id)?.inscription_mouse_facts
    }
    fn examine_request(&self) -> Option<(ObjectId, u64)> {
        self.examine_request
    }
    fn book_session(&self) -> crate::book::BookSessionView {
        self.book_session.clone()
    }
    fn open_book(&self) -> Option<BookView> {
        self.open_book.clone()
    }
    fn barber(&self) -> Option<BarberView> {
        self.barber
    }
    fn oath_xp_cost(&self) -> Option<u32> {
        self.oath_xp_cost
    }
    fn allegiance_roster(&self) -> AllegianceRoster {
        self.allegiance_roster.clone()
    }
    fn allegiance_update_aborts(&self) -> u64 {
        self.allegiance_update_aborts
    }
    fn fellowship(&self) -> Option<FellowshipView> {
        self.fellowship.clone()
    }
    fn friends(&self) -> Vec<FriendEntry> {
        self.friends.clone()
    }
    fn squelch_list(&self) -> Vec<SquelchEntry> {
        self.squelch_list.clone()
    }
    fn contracts(&self) -> Vec<ContractEntry> {
        self.contracts.clone()
    }
    fn house_status_notices(&self) -> u64 {
        self.house_status_notices
    }
    fn house_data(&self) -> Option<HouseDataView> {
        self.house_data.clone()
    }
    fn house_data_notices(&self) -> u64 {
        self.house_data_notices
    }
    fn house_purchase(&self) -> HousePurchaseView {
        self.house_purchase
    }
    fn utc_offset_secs(&self) -> UtcOffsetSecs {
        self.utc_offset_secs
    }
    fn shop(&self) -> ShopView {
        self.shop.clone()
    }
    fn trade(&self) -> TradeView {
        self.trade.clone()
    }
    fn item_owned_by_player(&self, item: ObjectId) -> bool {
        self.object(item).is_some_and(|o| o.item_owned_by_player)
    }
    fn payment_lists(&self) -> crate::panels::slumlord::PaymentListsView {
        self.payment_lists.clone()
    }
    fn salvage_list(&self) -> crate::panels::salvage::SalvageListView {
        self.salvage_list.clone()
    }
    fn slumlord(&self) -> Option<SlumlordView> {
        self.slumlord.clone()
    }
    fn minigame(&self) -> Option<MiniGameView> {
        self.minigame
    }
    fn slumlord_notices(&self) -> u64 {
        self.slumlord_notices
    }
    fn item_wcid(&self, item: ObjectId) -> u32 {
        self.object(item).map_or(0, |o| o.item_wcid)
    }
    fn item_trade_note_value(&self, item: ObjectId) -> Option<i32> {
        self.object(item)?.item_trade_note_value
    }
    fn item_house_payment(&self, item: ObjectId) -> i32 {
        self.object(item).map_or(0, |o| o.item_house_payment)
    }
    fn max_split_size(&self) -> i32 {
        self.max_split_size
    }
    fn item_material_type(&self, item: ObjectId) -> u32 {
        self.object(item).map_or(0, |o| o.item_material_type)
    }
    fn vendor_drag_item_accepted(&self, item: ObjectId) -> bool {
        self.object(item)
            .is_some_and(|o| o.vendor_drag_item_accepted)
    }
    fn trade_drag_item_acceptable(&self, item: ObjectId) -> bool {
        self.object(item)
            .is_some_and(|o| o.trade_drag_item_acceptable)
    }
    fn item_valid_locations(&self, item: ObjectId) -> Option<u32> {
        self.object(item)?.item_valid_locations
    }
    fn equipment_hover(&self, item: ObjectId) -> crate::view::EquipmentHover {
        self.object(item)
            .map_or_else(Default::default, |o| o.equipment_hover)
    }
    fn spell_is_untargeted(&self, spell_id: u32) -> bool {
        self.spells.get(&spell_id).is_some_and(|s| s.untargeted)
    }
    fn spell_target_compatible(&self, spell_id: u32) -> bool {
        self.spells
            .get(&spell_id)
            .is_some_and(|s| s.target_compatible)
    }
    fn item_useable_self_target(&self, item: ObjectId) -> bool {
        self.object(item)
            .is_some_and(|o| o.item_useable_self_target)
    }
    fn item_target_compatible(&self, item: ObjectId) -> bool {
        self.object(item).is_some_and(|o| o.item_target_compatible)
    }
    fn selected_object(&self) -> Option<ObjectId> {
        self.selected_object
    }
    fn allegiance_has_member(&self, id: ObjectId) -> bool {
        self.object(id).is_some_and(|o| o.allegiance_has_member)
    }
    fn allegiance_monarch_quality(&self) -> Option<ObjectId> {
        self.allegiance_monarch_quality
    }
    fn split_size(&self) -> i32 {
        self.split_size
    }
    fn spell_components(&self) -> Vec<ComponentCategory> {
        self.spell_components.clone()
    }
    fn object_is_owned_component(&self, obj: ObjectId) -> Option<u32> {
        self.object(obj)?.object_is_owned_component
    }
    fn combat_mode(&self) -> u32 {
        self.combat_mode
    }
    fn advanced_combat_ui(&self) -> bool {
        self.advanced_combat_ui
    }
    fn combat_bar(&self) -> CombatBar {
        self.combat_bar
    }
    fn recklessness_advancement_class(&self) -> u32 {
        self.recklessness_advancement_class
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: [`EmptyGameView`] is the "nothing is loaded" view every panel-construction test
    /// uses. A snapshot of it has to answer the same way, or a snapshot is not a `GameView`.
    #[test]
    fn a_snapshot_of_the_empty_view_answers_like_the_empty_view() {
        let snap = GameSnapshot::empty();
        let empty = EmptyGameView;
        assert_eq!(snap.player(), empty.player());
        assert_eq!(snap.character_name(), empty.character_name());
        assert_eq!(snap.combat_mode(), empty.combat_mode());
        assert_eq!(snap.spell_filters(), empty.spell_filters());
        assert_eq!(snap.packet_loss_percent(), empty.packet_loss_percent());
        assert_eq!(snap.utc_offset_secs(), empty.utc_offset_secs());
        assert!(snap.radar_objects().is_empty());
        assert!(snap.objects.is_empty(), "an empty view reaches no object");
    }

    /// Oracle: the fixed key sets are the ones the module header names, and a snapshot carries
    /// every one of them whether or not the view had anything to say.
    #[test]
    fn the_closed_key_sets_are_captured_whole() {
        let snap = GameSnapshot::empty();
        assert_eq!(snap.shortcuts.len(), SHORTCUT_SLOTS as usize);
        assert_eq!(snap.spell_tabs.len(), SPELL_TABS);
        assert_eq!(snap.player_options.len(), PlayerOption::ALL.len());
    }

    /// Oracle: the whole point is that the borrows are gone. A snapshot outlives the view it was
    /// taken from — this would not compile if any field were still a `&str` or a `&[T]`.
    #[test]
    fn a_snapshot_outlives_the_view_it_was_taken_from() {
        let snap = {
            let view = EmptyGameView;
            GameSnapshot::from_view(&view)
        };
        assert_eq!(snap.player(), None);
        assert_eq!(snap.character_name(), None);
    }
    /// Behaviour: spellbar.caption.selecting-a-spell-or-a-wand-names-it
    #[test]
    fn an_unlearned_endowment_keeps_its_spell_and_quiet_target_facts_in_a_snapshot() {
        #[derive(Debug)]
        struct Wand;
        impl GameView for Wand {
            fn endowment(&self) -> Option<(ObjectId, u32)> {
                Some((ObjectId(2), 99))
            }
            fn selected_object(&self) -> Option<ObjectId> {
                Some(ObjectId(3))
            }
            fn name(&self, _: ObjectId) -> Option<&str> {
                Some("Wand")
            }
            fn item_target_compatible(&self, _: ObjectId) -> bool {
                true
            }
            fn spell_target_compatible(&self, _: u32) -> bool {
                true
            }
            fn spell(&self, id: u32) -> Option<crate::SpellEntry> {
                (id == 99).then(|| crate::SpellEntry {
                    id,
                    name: "Endowed spell".into(),
                    icon: None,
                    school: 3,
                    level: 8,
                    icon_power: 10,
                    display_order: 0,
                    bitfield: 0,
                })
            }
            fn spell_examine(&self, _: u32) -> Option<crate::SpellExamineView> {
                Some(crate::SpellExamineView {
                    name: "Endowed spell".into(),
                    level: 8,
                    icon_power: 10,
                    ..Default::default()
                })
            }
        }
        let snapshot = GameSnapshot::from_view(&Wand);
        assert!(!snapshot.is_spell_known(99));
        assert_eq!(snapshot.spell(99), Wand.spell(99));
        assert_eq!(snapshot.spell_examine(99), Wand.spell_examine(99));
        assert!(snapshot.spell_target_compatible(99));
        assert_eq!(
            crate::spellbook::readiness(&snapshot, 0, Some(ObjectId(2))),
            crate::spellbook::SpellReadiness::ReadyOnTarget
        );
        assert_eq!(
            crate::spellbook::readiness(&snapshot, 99, None),
            crate::spellbook::SpellReadiness::ReadyOnTarget
        );
    }
}
