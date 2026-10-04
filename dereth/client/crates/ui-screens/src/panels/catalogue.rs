//! The panel catalogue: for every gameplay panel, the children its post-init binds and the element
//! messages it handles.
//!
//! The catalogue is transcribed from the retail client's gameplay, HUD, toolbar, panel, chat-window,
//! options, map, and radar screens.
//!
//! ## Why a table and not forty structs
//!
//! Every one of these panels is the same shape: its post-init looks up a dozen children into named
//! fields and registers a handful of notice handlers, and its element-message handler switches
//! over element ids. The *behaviour* that differs — the radar's palette, the chat
//! router, the panel stack's restore rule, the vitals arithmetic — lives in its own module and is
//! tested there. What is common is the binding, and a binding table is checkable against the retail
//! layouts, which is what the acceptance gate does: **every id in this file is looked up in the
//! real element tree built from `client_local_English.dat`, and a miss is a failure.**
//!
//! ## A row here is not a panel
//!
//! That acceptance gate proves the **ids**. It proves nothing about whether anything *builds* the
//! panel, and a row in this table is routinely mistaken for a working one — [`super::remaining`]'s
//! own header records the trap about `MessageLogPanel`, which "had **no constructor anywhere in the
//! workspace**" while its row here was green. The other half of the gate is a consumer test,
//! which fails on any spec that no production code outside this file names; its allow-list
//! carries one line per row-only panel with the reason.

use dereth_ui::ElementType;

use crate::bind::{child, child_under, ChildBinding};
use crate::element_types::ty;

/// One panel behavior.
#[derive(Debug, Clone, Copy)]
pub struct PanelSpec {
    /// The local behavior label used by [`spec`].
    pub class: &'static str,
    pub ty: ElementType,
    /// The children the post-init binds, with the field each lands in.
    pub children: &'static [ChildBinding],
    /// Element ids the class's element-message handler names, or that a list row is built from,
    /// which are **not** post-init bindings.
    ///
    /// The panel documents list these under "(plus `0x…`)" and "switch cases". They are real
    /// elements — every one is in a shipped layout — but they live on a *different root* of the
    /// panel's own layout: they are row and popup templates, instantiated when a row is created
    /// rather than when the panel is. A recursive child lookup from the panel root therefore
    /// returns null, which is why they are separated here.
    ///
    pub templates: &'static [ChildBinding],
    /// The element message ids the class's element-message handler switches on.
    pub element_messages: &'static [u32],
    /// The global message ids it subscribes to.
    pub global_messages: &'static [u32],
}

const fn p(
    class: &'static str,
    ty: ElementType,
    children: &'static [ChildBinding],
    element_messages: &'static [u32],
    global_messages: &'static [u32],
) -> PanelSpec {
    PanelSpec {
        class,
        ty,
        children,
        templates: NONE,
        element_messages,
        global_messages,
    }
}

/// [`p`] plus the panel's row/popup templates.
const fn pt(b: PanelSpec, templates: &'static [ChildBinding]) -> PanelSpec {
    PanelSpec {
        class: b.class,
        ty: b.ty,
        children: b.children,
        templates,
        element_messages: b.element_messages,
        global_messages: b.global_messages,
    }
}

// -------------------------------------------------------------------------------------------
// HUD.
// -------------------------------------------------------------------------------------------

/// The vitals panel's post-init.
const VITALS: &[ChildBinding] = &[
    child("health_meter", 0x1000_00E6),
    child("health_label", 0x1000_00EB),
    child("stamina_meter", 0x1000_00EC),
    child("stamina_label", 0x1000_00ED),
    child("mana_meter", 0x1000_00EE),
    child("mana_label", 0x1000_00EF),
];

/// `IndicatorStrip`. Its only behaviour is the log-out button.
const INDICATORS: &[ChildBinding] = &[child("logout_button", 0x1000_00FA)];

/// The eight border ids are in
/// [`crate::hud::floaty::SMART_BOX_CHROME`]; these are the two named non-border children.
const SMART_BOX: &[ChildBinding] = &[
    child("fps_display", 0x1000_0047),
    child("portal_space", 0x1000_0436),
];

/// The powerbar panel's post-init.
const POWERBAR: &[ChildBinding] = &[
    child("recklessness_field", 0x1000_05EE),
    child("bar", 0x1000_0034),
    child("secondary_bar", 0x1000_0035),
];

/// The combat panel's post-init.
///
/// The post-init treats each of the first three as a checkbox option and sets a player
/// option through it, so they are the cluster's three option checkboxes and **none of them is a
/// combat panel** — the `0x10000055` there is an element id that collides with
/// `CombatPanelStack`'s element type. The last four have no field in the class at all;
/// `CombatWindow` keeps exactly one element handle, the recklessness field.
const COMBAT: &[ChildBinding] = &[
    child("auto_repeat_attack_checkbox", 0x1000_0053),
    child("auto_target_checkbox", 0x1000_0054),
    child("view_combat_target_checkbox", 0x1000_0055),
    // `CombatWindow`'s one element handle is the recklessness field, and the panel's post-init
    // fills it from `0x100005EF` and then hides it.
    child("recklessness_field", 0x1000_05EF),
    child("attack_height", 0x1000_0056),
    child("actual_power", 0x1000_0050),
    child("desired_power", 0x1000_004F),
];

/// Its live instance is `<COMB>`
/// (`0x100006B5`), because `FloatingCombatStack` derives from this class; see
/// [`crate::element_types::COMBAT_PANEL_UNREGISTERED`].
///
/// The two "children" are its two **pages**: `0x1000005C` is `CombatWindow` and `0x10000061` is
/// `SpellcastingPanel`. Its child set-up reads
/// `0x10000029` off each and passes *should be default* as a **literal** `false` for both —
/// unlike the main panel stack's child set-up, which reads attribute `0x10000049`.
const COMBAT_PANEL: &[ChildBinding] = &[
    child("combat_ui", 0x1000_005C),
    child("spellcasting_ui", 0x1000_0061),
];

/// The five children are bound but stored in no named field, so what they display is unknown;
/// they are bound and display nothing.
const ENV_PANEL: &[ChildBinding] = &[
    child("child_5d", 0x1000_005D),
    child("child_5e", 0x1000_005E),
    child("child_5f", 0x1000_005F),
    child("child_60", 0x1000_0060),
    child("child_62", 0x1000_0062),
];

/// The spew box panel's post-init.
const SPEW_BOX: &[ChildBinding] = &[child("list_box", 0x1000_0049)];

/// The shared base of `SkillsPanel` and `AttributesPanel`.
const STAT_MANAGEMENT: &[ChildBinding] = &[
    child("name_text", 0x1000_0231),
    child("heritage_text", 0x1000_0232),
    child("pk_status_text", 0x1000_0233),
    child("total_xp_text", 0x1000_0235),
    child("xp_to_level_meter", 0x1000_0236),
    child("xp_to_level_text", 0x1000_0238),
    child("level_text", 0x1000_023B),
    child("list_box", 0x1000_023D),
    child("luminance_label_text", 0x1000_05C5),
    child("luminance_text", 0x1000_05C6),
];

// -------------------------------------------------------------------------------------------
// Toolbar and panel stack.
// -------------------------------------------------------------------------------------------

/// The toolbar panel's post-init.
const TOOLBAR: &[ChildBinding] = &[
    child("use_object_button", 0x1000_019D),
    child("examine_object_button", 0x1000_01A5),
    child("sel_object_field", 0x1000_019E),
    child("sel_object_name", 0x1000_019F),
    child("sel_object_health_meter", 0x1000_01A1),
    child("sel_object_mana_meter", 0x1000_01A2),
    child("stack_size_entry_box", 0x1000_01A3),
    child("stack_size_slider", 0x1000_01A4),
    child("inventory_button_drag_overlay", 0x1000_046C),
];

/// The seven toolbar **panel buttons**, in the order the post-init reads them into its
/// button-info array. `0x100001B1` is the inventory button and the drag-and-drop target.
pub const TOOLBAR_PANEL_BUTTONS: [u32; 7] = [
    0x1000_0197,
    0x1000_0198,
    0x1000_0199,
    0x1000_055A,
    0x1000_019A,
    0x1000_019B,
    0x1000_01B1,
];

/// The four combat-mode toggle buttons, each mapped to the combat-mode toggle action.
pub const TOOLBAR_COMBAT_BUTTONS: [u32; 4] = [0x1000_0192, 0x1000_0193, 0x1000_0194, 0x1000_0195];

/// The client's sixteen page containers **in call order**.
///
/// Which page is which panel is layout data (attribute `0x10000029`), not code. Nothing in this
/// crate pairs an id with a panel name.
pub const PANEL_PAGES: [u32; 16] = [
    0x1000_018B,
    0x1000_018F,
    0x1000_018E,
    0x1000_0559,
    0x1000_018C,
    0x1000_0182,
    0x1000_018D,
    0x1000_0190,
    0x1000_0184,
    0x1000_0185,
    0x1000_0181,
    0x1000_0189,
    0x1000_0183,
    0x1000_018A,
    0x1000_0187,
    0x1000_0188,
];

/// The client's five page containers, in call order.
///
/// Same shape as [`PANEL_PAGES`]: each is looked up, its `0x10000029` read into its child info,
/// and then **all five are hidden** by a trailing hide loop (the visibility is a literal here,
/// unlike `PanelStack`'s).
pub const ENV_PANEL_PAGES: [u32; 5] = [
    0x1000_005D,
    0x1000_005E,
    0x1000_005F,
    0x1000_0060,
    0x1000_0062,
];

/// The client's two page containers, and the same trailing hide loop.
pub const COMBAT_PANEL_PAGES: [u32; 2] = [0x1000_005C, 0x1000_0061];

/// The inventory panel's post-init.
const INVENTORY: &[ChildBinding] = &[
    child("paper_doll_ui", 0x1000_01CD),
    // **The backpack and 3-D items sub-panel handles are a route, and this build takes it from
    // one level up.**
    //
    // The client binds all four, casting three of them to their panel types (`0x10000024`
    // EquipmentPanel, `0x10000022` BackpackPanel, `0x10000021` ItemsPanel; the title text
    // uncast). **Every** use of the two middle handles in retail is a step to an item list inside
    // the sub-panel — the backpack's top container and container list, and the items panel's
    // item list — or the backpack panel's load-level write. Nothing shows, hides, moves or restates either
    // sub-panel element.
    //
    // `panels::inventory::InventoryPanels` is one struct for all four classes and resolves every
    // widget from the **inventory page** `0x1000018B`, which is the two sub-panels' own parent, so
    // it reaches the same three lists through the same two elements without stopping on them.
    // A test asserts the two routes land on the same handles.
    child("backpack_ui", 0x1000_01CE),
    child("3d_items_ui", 0x1000_01CF),
    child("title_text", 0x1000_01D3),
];

/// The backpack panel's post-init.
const BACKPACK: &[ChildBinding] = &[
    child("top_container", 0x1000_01C9),
    child("container_list", 0x1000_01CA),
    child("burden_text", 0x1000_01D8),
    child("burden_meter", 0x1000_01D9),
];

/// The 3 d items panel's post-init.
const ITEMS_3D: &[ChildBinding] = &[
    child("contents_text", 0x1000_01C5),
    child("item_list", 0x1000_01C6),
];

/// Twenty-four equipment slots plus five
/// non-slot children.
const PAPER_DOLL: &[ChildBinding] = &[
    child("paper_doll", 0x1000_01D5),
    child("paper_doll_drag_mask", 0x1000_01D6),
    child("paper_doll_drag_overlay", 0x1000_046D),
    child("show_slots_checkbox", 0x1000_05BE),
    child("slot_neck", 0x1000_01DA),
    child("slot_left_wrist", 0x1000_01DB),
    child("slot_left_ring", 0x1000_01DC),
    child("slot_right_wrist", 0x1000_01DD),
    child("slot_right_ring", 0x1000_01DE),
    child("slot_weapon_ready", 0x1000_01DF),
    child("slot_ammo_ready", 0x1000_01E0),
    child("slot_shield_ready", 0x1000_01E1),
    child("slot_shirt", 0x1000_01E2),
    child("slot_pants", 0x1000_01E3),
    child("slot_trinket", 0x1000_058E),
    child("slot_sigil_1", 0x1000_0595),
    child("slot_sigil_2", 0x1000_0596),
    child("slot_sigil_3", 0x1000_0597),
    child("slot_head", 0x1000_05AB),
    child("slot_chest", 0x1000_05AC),
    child("slot_abdomen", 0x1000_05AD),
    child("slot_upper_arm", 0x1000_05AE),
    child("slot_lower_arm", 0x1000_05AF),
    child("slot_hand", 0x1000_05B0),
    child("slot_upper_leg", 0x1000_05B1),
    child("slot_lower_leg", 0x1000_05B2),
    child("slot_foot", 0x1000_05B3),
    child("slot_cloak", 0x1000_05E9),
];

/// `CharacterInfoPanel`.
const CHARACTER_INFO: &[ChildBinding] = &[child("info_text", 0x1000_011D)];

/// The character title panel's post-init.
const CHARACTER_TITLE: &[ChildBinding] = &[
    child("display_title_text", 0x1000_052F),
    child("title_list_box", 0x1000_0532),
    child("display_button", 0x1000_0535),
];

/// Five school filters and eight level filters.
const SPELLBOOK: &[ChildBinding] = &[
    child("spell_list", 0x1000_0295),
    child("btn_school_creature", 0x1000_0298),
    child("btn_school_item", 0x1000_0299),
    child("btn_school_life", 0x1000_029A),
    child("btn_school_war", 0x1000_029B),
    child("btn_school_void", 0x1000_05C0),
    child("btn_level_1", 0x1000_029C),
    child("btn_level_2", 0x1000_029D),
    child("btn_level_3", 0x1000_029E),
    child("btn_level_4", 0x1000_029F),
    child("btn_level_5", 0x1000_02A0),
    child("btn_level_6", 0x1000_02A1),
    child("btn_level_7", 0x1000_02A2),
    child("btn_level_8", 0x1000_054E),
];

/// The spell component panel's post-init.
const SPELL_COMPONENT: &[ChildBinding] = &[child("component_list_box", 0x1000_0464)];

/// The spellcasting panel's post-init.
///
/// **The last three are nested.** They are not "the ring around the armed spell on the bar and
/// its two layers". The post-init looks all three up **inside the endowment icon**, and the
/// field names here carry that:
///
/// Find and cache icon `0x100000B1`. Only if it exists, look up its three children
/// `0x10000453`, `0x10000452` and `0x10000454`, in that order. A missing icon skips all three.
///
/// So `0x10000452` carries the endowment **spell's** composite icon, `0x10000453` the endowed
/// **item's** own drag icon over it, and `0x10000454` the ring around that icon — shown exactly
/// while the open sub-menu has its endowment selected. The ring around an armed *spell row* is the
/// row's own selected-icon element on its `UiItemWidget` and is a different element entirely.
///
/// [`child_under`] reproduces the post-init two-step lookup and null-parent guard: a layout with no
/// `0x100000B1` reports all three as **missing**, which is what the client's null-parent skip
/// leaves behind.
const SPELLCASTING: &[ChildBinding] = &[
    child("spellcast_background", 0x1000_00A0),
    child("spellcast_panel", 0x1000_00A2),
    child("endowment_icon", 0x1000_00B1),
    child("spellcast_button", 0x1000_00B2),
    child_under(0x1000_00B1, "endowment_icon_underlay", 0x1000_0452),
    child_under(0x1000_00B1, "endowment_icon_overlay", 0x1000_0453),
    child_under(0x1000_00B1, "endowment_icon_selected", 0x1000_0454),
    child("spell_name", 0x1000_048B),
];

/// The allegiance panel's post-init.
const ALLEGIANCE: &[ChildBinding] = &[
    child("allegiance_name", 0x1000_0251),
    child("player_followers", 0x1000_0252),
    child("player_rank", 0x1000_0253),
    child("monarch_field", 0x1000_0255),
    child("monarch_label", 0x1000_0256),
    child("monarch_name", 0x1000_0257),
    child("monarch_followers", 0x1000_0258),
    child("patron_field", 0x1000_025A),
    child("patron_name", 0x1000_025C),
    child("vassal_list_box", 0x1000_0260),
    child("swear_button", 0x1000_0263),
    child("break_button", 0x1000_0264),
    child("kick_button", 0x1000_0265),
];

/// The fellowship panel's post-init.
const FELLOWSHIP: &[ChildBinding] = &[
    child("not_in_a_fellowship_frame", 0x1000_026B),
    child("fellowship_name_entry_box", 0x1000_026F),
    child("create_fellowship_button", 0x1000_0274),
    child("in_a_fellowship_frame", 0x1000_0275),
    child("fellowship_name", 0x1000_0276),
    child("fellows_list_box", 0x1000_0279),
    child("fellow_leader_button", 0x1000_027B),
    child("fellow_quit_button", 0x1000_027C),
    child("fellow_open_button", 0x1000_027D),
    child("fellow_recruit_button", 0x1000_027E),
    child("fellow_dismiss_button", 0x1000_027F),
    child("fellow_disband_button", 0x1000_0280),
];

/// The friends panel's post-init.
const FRIENDS: &[ChildBinding] = &[
    child("add_button", 0x1000_0514),
    child("remove_button", 0x1000_0515),
    child("tell_button", 0x1000_0516),
    child("friends_list_box", 0x1000_0517),
    child("friend_name_edit_box", 0x1000_051B),
    child("child_52c", 0x1000_052C),
];

/// The squelch panel's post-init.
const SQUELCH: &[ChildBinding] = &[
    child("squelch_list_box", 0x1000_053E),
    child("squelch_name_edit_box", 0x1000_0540),
    child("remove_button", 0x1000_0547),
    child("squelch_character_button", 0x1000_054B),
    child("squelch_account_button", 0x1000_054C),
];

/// The journal panel's post-init.
const JOURNAL: &[ChildBinding] = &[
    child("label_edit_box", 0x1000_0569),
    child("title_edit_box", 0x1000_056B),
    child("notes_edit_box", 0x1000_056D),
    child("page_number_static_text", 0x1000_0570),
    child("location_static_text", 0x1000_0573),
    child("days_edit_box", 0x1000_0576),
    child("days_static_text", 0x1000_0577),
    child("hours_edit_box", 0x1000_0578),
    child("hours_static_text", 0x1000_0579),
    child("minutes_edit_box", 0x1000_057A),
    child("minutes_static_text", 0x1000_057B),
    child("timer_static_text", 0x1000_057C),
    child("start_timer_button", 0x1000_057D),
];

/// The page list panel's post-init.
const PAGE_LIST: &[ChildBinding] = &[
    child("page_list_box", 0x1000_0583),
    child("search_edit_box", 0x1000_0587),
];

/// The contracts panel's post-init.
const CONTRACTS: &[ChildBinding] = &[
    child("contracts_box", 0x1000_05CF),
    child("notes_text", 0x1000_05DE),
    child("progress_text", 0x1000_05DF),
    child("contact_text", 0x1000_05E0),
    child("contact_loc_text", 0x1000_05E1),
    child("area_text", 0x1000_05E2),
    child("timed_text", 0x1000_05E3),
];

/// The book panel's post-init.
const BOOK: &[ChildBinding] = &[
    child("title_text", 0x1000_010F),
    child("page_text", 0x1000_0111),
    child("prev_button", 0x1000_0114),
    child("next_button", 0x1000_0115),
    child("page_menu", 0x1000_0470),
    child("menu_selection_page_num_text", 0x1000_047B),
];

/// `AbusePanel`.
const ABUSE: &[ChildBinding] = &[
    child("name_box", 0x1000_0105),
    child("entry_box", 0x1000_0107),
    child("continue_button", 0x1000_0109),
    child("result_text", 0x1000_010B),
];

/// `UrgentAssistancePanel`.
const URGENT: &[ChildBinding] = &[
    child("entry_box", 0x1000_01BA),
    child("continue_button", 0x1000_01BD),
];

/// `MiniGamePanel`.
const MINI_GAME: &[ChildBinding] = &[
    child("child_174", 0x1000_0174),
    child("resign_button", 0x1000_0175),
    child("pass_button", 0x1000_0176),
    child("stalemate_button", 0x1000_0177),
];

/// `HousePanel`.
const HOUSE: &[ChildBinding] = &[child("text_box", 0x1000_01E6)];

/// `LinkStatusPanel`.
const LINK_STATUS: &[ChildBinding] = &[child("info_text", 0x1000_0169)];

/// `VitaePanel`.
const VITAE: &[ChildBinding] = &[child("vitae_text", 0x1000_01C3)];

/// `EffectsPanel`.
const EFFECTS: &[ChildBinding] = &[
    child("effect_list", 0x1000_0123),
    child("effect_item", 0x1000_0126),
];

/// `ExternalContainerPanel`.
const EXTERNAL_CONTAINER: &[ChildBinding] = &[
    child("top_container", 0x1000_0064),
    child("container_list", 0x1000_0067),
    child("close_button", 0x1000_0068),
    child("item_list", 0x1000_006A),
];

/// `TradePanel`.
const SECURE_TRADE: &[ChildBinding] = &[
    child("other_player_name", 0x1000_007E),
    child("other_trade_status_indicator", 0x1000_007F),
    child("other_total_items_label", 0x1000_0080),
    child("other_items_list", 0x1000_0081),
    child("self_player_name", 0x1000_0085),
    child("trade_button", 0x1000_0086),
    child("self_total_items_label", 0x1000_0087),
    child("self_items_list", 0x1000_0088),
    child("child_8a", 0x1000_008A),
];

/// `SalvagePanel`.
const SALVAGE: &[ChildBinding] = &[
    child("salvage_list", 0x1000_0074),
    child("salvage_button", 0x1000_0076),
    child("child_78", 0x1000_0078),
];

/// `HousingPanel`, checked against its initializer and element-message handler.
///
/// `0x10000090` is not the close button: the post-init never binds it, the element-message handler treats it as the **Buy tab page**'s
/// visibility, and the close button is `0x1000009E`. The eight children carry their real roles
/// and types.
const SLUMLORD: &[ChildBinding] = &[
    child("buy_requirements_text", 0x1000_0091),
    child("buy_house_owner_text", 0x1000_0093),
    child("buy_button", 0x1000_0094),
    child("buy_item_list", 0x1000_0095),
    child("rent_requirements_text", 0x1000_0098),
    child("rent_house_owner_text", 0x1000_009A),
    child("rent_button", 0x1000_009B),
    child("rent_item_list", 0x1000_009C),
];

/// `ExaminationPanel`. Only the two named children are listed here; the other thirty are
/// bound by [`super::examination`].
const EXAMINATION: &[ChildBinding] = &[
    child("displayed_name_text", 0x1000_012D),
    child("creature_display_name", 0x1000_014E),
];

/// The barber panel's page initialisation.
const BARBER: &[ChildBinding] = &[
    child("grad_circle", 0x1000_030E),
    child("shade_scroll", 0x1000_0321),
    child("viewport", 0x1000_059B),
    // **Inert in retail, and deliberately unconsumed here.**
    //
    // Retail stores the barber's handle to this element once and never reads it. `BarberPanel` is
    // `AppearancePage` with the Face/Clothes tab pair removed; the char-gen page's own
    // face-choices handle *is* read twice, to swap the two row groups, and the barber has no
    // clothes tab, so its copy has nothing to swap.
    //
    // The element is a plain `Field` whose five children are exactly the five part rows
    // below (`0x1000059E`, `0x1000059F`, `0x100005A0`, `0x100005A1`, `0x100005A2`) — the frame
    // around them, not a list of faces. A test pins both facts.
    child("face_choices", 0x1000_059D),
    child("hair_spin", 0x1000_059E),
    child("eyes_spin", 0x1000_059F),
    child("nose_spin", 0x1000_05A0),
    child("mouth_spin", 0x1000_05A1),
    child("skin_spin", 0x1000_05A2),
    child("rotate_left", 0x1000_05A4),
    child("rotate_right", 0x1000_05A5),
    child("apply_button", 0x1000_05A6),
    child("cancel_button", 0x1000_05A7),
    child("option_1", 0x1000_05C9),
    child("option_2", 0x1000_05CA),
    child("option_3", 0x1000_05CB),
];

/// `VendorPanel`: 31 elements under `0x100000B8`.
///
/// **They are not post-init bindings, and `VendorPanel` has no field for thirty of them.** The
/// post-init resolves **exactly one** element id:
///
/// Find child `0x100000B8`, cast it to panel type 8 and cache it as the vendor panel.
///
/// Everything else it does is construction — the inventory view, the buy view and the sell view
/// sub-panels — followed by eight notice-handler registrations, clamping the game-view edge
/// (`3`), and two registrations with a global service, `(1, 0x14)` and `(1, 5)`. The class holds
/// the vendor panel, the three views and the two profile lists, and nothing named after an id.
///
/// The rows are therefore the shipped **layout** subtree of `0x100000B8`, which is still exactly
/// what the acceptance gate should check; each row is named for what the element is in that
/// tree and in [`super::vendor`].
///
/// # The Selling page, and the parents
///
/// The Selling page's ten, `0x100000CD`–`0x100000D6`, are not decoration. The element-message handler's button dispatch covers exactly the
/// 21 ids `0x100000C2..=0x100000D6`, and its domain *ends* at the last of them. Each id selects
/// one of twelve cases: `0xC2` and `0xC3` cases 0 and 1, `0xC4`–`0xC8` the shared case 11,
/// `0xC9`–`0xCC` cases 2–5, `0xCD`–`0xD1` case 11 again, and `0xD2`–`0xD6` cases 6–10. So the
/// Selling page resolves as
///
/// | id | case | action | [`super::vendor`] |
/// |---|---|---|---|
/// | `0x100000D2` | 6 | sell the selected item | `BTN_SELL_ITEM` |
/// | `0x100000D3` | 7 | sell the whole list | `BTN_SELL_ALL` |
/// | `0x100000D4` | 8 | remove the item from the list | `BTN_SELL_CLEAR_ITEM` |
/// | `0x100000D5` | 9 | clear the sell list | `BTN_SELL_CLEAR_LIST` |
/// | `0x100000D6` | 10 | close — the only case that can raise a dialog | `BTN_CLOSE` |
/// | `0xCD` `0xCE` `0xCF` `0xD0` `0xD1` | 11 | the shared no-op | page, list, its bar, two texts |
///
/// Case 11 is *not* "unhandled": the buy page's own five non-button ids (`0xC4`–`0xC8`) take the
/// same case. A page, a list, a scrollbar and two texts do not send button clicks.
///
/// **Parents.** Every row carries the parent the shipped layout gives it, read out of the
/// live tree. The three tab labels and the three pages
/// hang off `0x100000B8`; each page's eight members hang off that page; and `btn_close` hangs off
/// **`0x10000062` — `VendorPanel` itself, a *sibling* of `0x100000B8`, not a descendant of it**,
/// which is why [`super::vendor::VendorPanel::post_init`] cannot bind it from `TABS` and why it
/// arrives only as an element-message id. Recording the parents is what makes the three pages
/// distinguishable: they are the same shape at the same box, and their scrollbars share the
/// thumb/arrow ids `0x00000001`, `0x1000036B` and `0x1000036C` three times over.
///
/// **And seven of the ten are bound by literal id, in retail, by the sub-UI.** The sell view's
/// constructor looks each of them up with a recursive child lookup.
///
/// The three it does **not** bind are the three with a different route, and they are exactly the
/// three rows above that a reader might otherwise think unreached: the page `0x100000CD` (the tab
/// element's open page), the scrollbar `0x100000CF` (its list's `H_SCROLLBAR`) and `btn_close`
/// `0x100000D6`, which retail never resolves at all — it only ever arrives as the subject of the
/// button dispatch.
///
/// Engine types, measured: `0x08` `Panel` (`0xB8`), `0x0C` `TextElement` (the three
/// tabs and the four `*_text` rows), `0x03` `Field` (the three pages), `0x10000031`
/// `ItemListWidget` (the three lists), `0x0B` `Scrollbar` (the three bars), `0x06`
/// `Menu` (`0xBF`), `0x01` `Button` (every `btn_*`).
const VENDOR: &[ChildBinding] = &[
    child("vendor_panel", 0x1000_00B8),
    child_under(0x1000_00B8, "tab_items", 0x1000_00B9),
    child_under(0x1000_00B8, "tab_buying", 0x1000_00BA),
    child_under(0x1000_00B8, "tab_selling", 0x1000_00BB),
    // ---- Items, the inventory view --------------------------------------------------------------
    child_under(0x1000_00B8, "page_items", 0x1000_00BC),
    child_under(0x1000_00BC, "stock_list", 0x1000_00BD),
    // The stock list's **horizontal scrollbar**, and one of the three ids of this spec that no
    // code in the client or in this workspace names — which is not a gap, as with
    // `VitalsPanel`'s meters. `0x100000BD` carries `H_SCROLLBAR` (attribute `0x71`) = `0x100000BE`
    // as layout data, which is how retail finds it and how
    // `dereth_ui::scrollable::Scrollable::scrollbar` finds it here. Retail never names
    // `0x100000BE` by id. A test drives it from a pointer. The same pair repeats on the other
    // two pages: `0xC5`->`0xC6`, `0xCE`->`0xCF`; all three are accounted for as layout rows by
    // the consumer test.
    child_under(0x1000_00BC, "stock_list_scrollbar", 0x1000_00BE),
    child_under(0x1000_00BC, "item_type_menu", 0x1000_00BF),
    child_under(0x1000_00BC, "item_name_text", 0x1000_00C0),
    child_under(0x1000_00BC, "item_cost_text", 0x1000_00C1),
    child_under(0x1000_00BC, "btn_buy", 0x1000_00C2),
    child_under(0x1000_00BC, "btn_add_to_list", 0x1000_00C3),
    // ---- Buying, the buy view -------------------------------------------------------------------
    child_under(0x1000_00B8, "page_buy", 0x1000_00C4),
    child_under(0x1000_00C4, "buy_list", 0x1000_00C5),
    child_under(0x1000_00C4, "buy_list_scrollbar", 0x1000_00C6),
    child_under(0x1000_00C4, "buy_list_text", 0x1000_00C7),
    child_under(0x1000_00C4, "buy_purse_text", 0x1000_00C8),
    child_under(0x1000_00C4, "btn_buy_item", 0x1000_00C9),
    child_under(0x1000_00C4, "btn_buy_all", 0x1000_00CA),
    child_under(0x1000_00C4, "btn_buy_clear_item", 0x1000_00CB),
    child_under(0x1000_00C4, "btn_buy_clear_list", 0x1000_00CC),
    // ---- Selling, the sell view -----------------------------------------------------------------
    child_under(0x1000_00B8, "page_sell", 0x1000_00CD),
    child_under(0x1000_00CD, "sell_list", 0x1000_00CE),
    // The sell basket's bar. `0x100000CE` carries `H_SCROLLBAR` = `0x100000CF` and no
    // `V_SCROLLBAR`, the same as the other two lists; retail never names it by id and neither
    // does production Rust, so it is an `ID_ACCOUNTED_FOR` `LAYOUT` row, not a consumer gap.
    child_under(0x1000_00CD, "sell_list_scrollbar", 0x1000_00CF),
    child_under(0x1000_00CD, "sell_list_text", 0x1000_00D0),
    child_under(0x1000_00CD, "sell_purse_text", 0x1000_00D1),
    child_under(0x1000_00CD, "btn_sell_item", 0x1000_00D2),
    child_under(0x1000_00CD, "btn_sell_all", 0x1000_00D3),
    child_under(0x1000_00CD, "btn_sell_clear_item", 0x1000_00D4),
    child_under(0x1000_00CD, "btn_sell_clear_list", 0x1000_00D5),
    // **Not under `0x100000B8`.** The Close button is a sibling of the tab element, directly under
    // `0x10000062` (`VendorPanel`), which is why it is the one row of this spec whose parent is not
    // inside the tab subtree and why `VendorPanel::post_init` never binds it.
    child_under(0x1000_0062, "btn_close", 0x1000_00D6),
];

/// The map panel's post-init.
const MAP: &[ChildBinding] = &[
    child("date_time_text", 0x1000_01EB),
    child("map", 0x1000_01EC),
    child("player_location_icon", 0x1000_01ED),
    child("house_location_icon", 0x1000_01EE),
    child("coordinate_text", 0x1000_01EF),
];

/// The client's two **fixed** children. The nine geometry and
/// compass-token children are named indirectly through attributes `0x1000002D`–`0x10000038` and
/// are resolved at run time by [`crate::mapradar::radar`].
const RADAR: &[ChildBinding] = &[
    child("lock_ui_button", 0x1000_0619),
    child("drag_button", 0x1000_06A3),
];

/// Shared by all five chat windows.
const CHAT_INTERFACE: &[ChildBinding] = &[
    child("chat_entry", 0x1000_0011),
    child("chat_log", 0x1000_048C),
    child("opacity_source", 0x1000_0016),
];

/// The config panel's post-init.
const CONFIG: &[ChildBinding] = &[child("option_box", 0x1000_0200)];

/// The character settings panel's post-init.
const CHARACTER_SETTINGS: &[ChildBinding] = &[child("option_box", 0x1000_01FA)];

/// The chat options panel's post-init.
const CHAT_OPTIONS: &[ChildBinding] = &[child("option_box", 0x1000_050D)];

/// The keyboard panel's post-init.
const KEYBOARD: &[ChildBinding] = &[
    child("child_211", 0x1000_0211),
    child("child_49d", 0x1000_049D),
    child("child_49f", 0x1000_049F),
    child("child_4a1", 0x1000_04A1),
    child("child_4a3", 0x1000_04A3),
    child("keyboard_load_keymap_button", 0x1000_04A5),
];

/// See [`PanelSpec::templates`].
const CHARACTER_TITLE_TEMPLATES: &[ChildBinding] = &[child("child_537", 0x1000_0537)];

/// See [`PanelSpec::templates`].
const SPELL_COMPONENT_TEMPLATES: &[ChildBinding] = &[
    child("child_468", 0x1000_0468),
    child("child_469", 0x1000_0469),
    child("child_46a", 0x1000_046A),
    child("search_box", 0x1000_046B),
];

/// See [`PanelSpec::templates`].
const FRIENDS_TEMPLATES: &[ChildBinding] = &[child("child_51a", 0x1000_051A)];

/// See [`PanelSpec::templates`].
const SQUELCH_TEMPLATES: &[ChildBinding] = &[child("child_542", 0x1000_0542)];

/// See [`PanelSpec::templates`].
const PAGE_LIST_TEMPLATES: &[ChildBinding] = &[
    child("child_58a", 0x1000_058A),
    child("child_58b", 0x1000_058B),
    child("child_58c", 0x1000_058C),
    child("child_58d", 0x1000_058D),
];

/// See [`PanelSpec::templates`].
const CONTRACTS_TEMPLATES: &[ChildBinding] = &[
    child("child_5d1", 0x1000_05D1),
    child("child_5d2", 0x1000_05D2),
];

/// See [`PanelSpec::templates`].
const BOOK_TEMPLATES: &[ChildBinding] = &[child("child_479", 0x1000_0479)];

/// See [`PanelSpec::templates`].
const MINI_GAME_TEMPLATES: &[ChildBinding] = &[
    // Checked against the game board grid's constructor: not `0x10000179`, which retail never
    // uses. That constructor adds
    // **sixty-four** rows from template **`0x10000178`**, which is the chessboard.
    child("piece_cell_template", 0x1000_0178),
];

/// Panels whose post-init binds no child at all.
const NONE: &[ChildBinding] = &[];

/// The complete panel catalogue.
///
/// `AdminPropertiesPanel` is present with an empty binding table on purpose: it has no recovered
/// child bindings, notices or handlers, so it is constructed from its layout and left inert.
pub const PANELS: &[PanelSpec] = &[
    // ---- HUD --------------------------------------------------------------------------------
    p("VitalsPanel", ty::VITALS, VITALS, &[0x1C], &[]),
    p("IndicatorStrip", ty::INDICATORS, INDICATORS, &[1], &[]),
    p(
        "WorldView",
        ty::SMART_BOX,
        SMART_BOX,
        &[0x15, 0x21],
        &[3, 0x0D],
    ),
    p("PowerBar", ty::POWERBAR, POWERBAR, &[], &[]),
    p("CombatWindow", ty::COMBAT, COMBAT, &[], &[]),
    p("EnvironmentPanelStack", ty::ENV_PANEL, ENV_PANEL, &[], &[]),
    p(
        "MessageLogPanel",
        ty::SPEW_BOX,
        SPEW_BOX,
        &[0x1000_0003],
        &[3],
    ),
    p("LinkStatusPanel", ty::LINK_STATUS, LINK_STATUS, &[], &[3]),
    p("VitaePanel", ty::VITAE, VITAE, &[], &[]),
    p("EffectsPanel", ty::EFFECTS, EFFECTS, &[], &[]),
    p(
        "CharacterInfoPanel",
        ty::CHARACTER_INFO,
        CHARACTER_INFO,
        &[],
        &[],
    ),
    // ---- toolbar and panel stack -------------------------------------------------------------
    p(
        "Toolbar",
        ty::TOOLBAR,
        TOOLBAR,
        &[1, 0x0A, 0x15, 0x2F, 0x3E],
        &[1],
    ),
    p("PanelStack", ty::PANEL, NONE, &[0x18], &[1]),
    // ---- inventory ---------------------------------------------------------------------------
    p("InventoryPanelStack", ty::INVENTORY, INVENTORY, &[], &[]),
    p("BackpackPanel", ty::BACKPACK, BACKPACK, &[], &[]),
    p("EquipmentPanel", ty::PAPER_DOLL, PAPER_DOLL, &[], &[0x0B]),
    p("ItemsPanel", ty::ITEMS_3D, ITEMS_3D, &[], &[]),
    p(
        "ExternalContainerPanel",
        ty::EXTERNAL_CONTAINER,
        EXTERNAL_CONTAINER,
        &[1],
        &[],
    ),
    // ---- character and skills ----------------------------------------------------------------
    p("AttributesPanel", ty::ATTRIBUTE, STAT_MANAGEMENT, &[1], &[]),
    p("SkillsPanel", ty::SKILL, STAT_MANAGEMENT, &[1], &[]),
    pt(
        p(
            "TitlesPanel",
            ty::CHARACTER_TITLE,
            CHARACTER_TITLE,
            &[1],
            &[],
        ),
        CHARACTER_TITLE_TEMPLATES,
    ),
    // ---- spells ------------------------------------------------------------------------------
    p("SpellbookPanel", ty::SPELLBOOK, SPELLBOOK, &[1, 0x1C], &[]),
    pt(
        p(
            "SpellComponentPanel",
            ty::SPELL_COMPONENT,
            SPELL_COMPONENT,
            &[0x2F],
            &[],
        ),
        SPELL_COMPONENT_TEMPLATES,
    ),
    p(
        "SpellcastingPanel",
        ty::SPELLCASTING,
        SPELLCASTING,
        &[1],
        &[],
    ),
    // ---- social ------------------------------------------------------------------------------
    p("AllegiancePanel", ty::ALLEGIANCE, ALLEGIANCE, &[1, 4], &[1]),
    p("FellowshipPanel", ty::FELLOWSHIP, FELLOWSHIP, &[1, 4], &[]),
    pt(
        p("FriendsPanel", ty::FRIENDS, FRIENDS, &[1, 4], &[]),
        FRIENDS_TEMPLATES,
    ),
    pt(
        p("SquelchPanel", ty::SQUELCH, SQUELCH, &[1, 4], &[]),
        SQUELCH_TEMPLATES,
    ),
    // ---- quests, journal, books --------------------------------------------------------------
    p("JournalPanel", ty::JOURNAL, JOURNAL, &[1], &[3, 0x0B]),
    pt(
        p("PageListPanel", ty::PAGE_LIST, PAGE_LIST, &[1], &[3]),
        PAGE_LIST_TEMPLATES,
    ),
    pt(
        p("ContractsPanel", ty::CONTRACTS, CONTRACTS, &[1, 4], &[3]),
        CONTRACTS_TEMPLATES,
    ),
    pt(p("BookPanel", ty::BOOK, BOOK, &[1], &[]), BOOK_TEMPLATES),
    // ---- utility and environment -------------------------------------------------------------
    p("AbusePanel", ty::ABUSE, ABUSE, &[1], &[]),
    p(
        "UrgentAssistancePanel",
        ty::URGENT_ASSISTANCE,
        URGENT,
        &[1],
        &[],
    ),
    pt(
        p(
            "MiniGamePanel",
            ty::MINI_GAME,
            MINI_GAME,
            // Checked against the mini game panel's element-message handler, which compares against
            // messages 1 and `0x1C`; the latter arm is the mouse-press handling, i.e. **every
            // move in the game**. A panel
            // catalogued as listening to clicks only could never have been played.
            &[1, 0x1C],
            &[],
        ),
        MINI_GAME_TEMPLATES,
    ),
    p("HousePanel", ty::HOUSE, HOUSE, &[], &[]),
    p("AdminPropertiesPanel", ty::ADMIN_QUALITIES, NONE, &[], &[]),
    // Checked against the panel's post-init and its element-message handler.
    //
    // The handler has **three** element-message arms: `0x1C`
    // (a double-click on a row), `1` (the two buttons) and `0x15` (the drop handling).
    p("SalvagePanel", ty::SALVAGE, SALVAGE, &[1, 0x15, 0x1C], &[]),
    p("TradePanel", ty::SECURE_TRADE, SECURE_TRADE, &[1], &[]),
    // The housing panel's element-message handler switches on `1`, `0x15` **and `0x18`**; the
    // `0x18` arm is the tab-visibility arm that sets the current house operation.
    p(
        "HousingPanel",
        ty::SLUMLORD,
        SLUMLORD,
        &[1, 0x15, 0x18],
        &[],
    ),
    p("VendorPanel", ty::VENDOR, VENDOR, &[1], &[3]),
    p("ExaminationPanel", ty::EXAMINATION, EXAMINATION, &[], &[3]),
    p("BarberPanel", ty::BARBER, BARBER, &[1], &[]),
    // ---- map and radar -----------------------------------------------------------------------
    p("MapPanel", ty::MAP, MAP, &[0x18, 0x1C], &[3]),
    p("Radar", ty::RADAR, RADAR, &[0x18, 0x19], &[3, 0x0D]),
    // ---- chat --------------------------------------------------------------------------------
    p(
        "MainChat",
        ty::MAIN_CHAT,
        CHAT_INTERFACE,
        &[1, 7, 0x12, 0x1B, 0x1F, 0x29, 0x2A, 0x2F],
        &[1, 3, 0x0B],
    ),
    p(
        "FloatingChat",
        ty::FLOATY_CHAT,
        CHAT_INTERFACE,
        &[1, 0x12, 0x1B, 0x1F, 0x29, 0x2A, 0x2F],
        &[1, 3],
    ),
    // ---- options -----------------------------------------------------------------------------
    p("ClientOptionsPanel", ty::CONFIG, CONFIG, &[1], &[0x0C]),
    p(
        "CharacterSettingsPanel",
        ty::CHARACTER_SETTINGS,
        CHARACTER_SETTINGS,
        &[1],
        &[],
    ),
    p(
        "ChatOptionsPanel",
        ty::CHAT_OPTIONS,
        CHAT_OPTIONS,
        &[1],
        &[],
    ),
    // `NONE` here is retail, not an unfinished transcription: `GameplayOptionsPanel`
    // has **no post-init** — type identity, construction, destruction, registration and the
    // element-message handler are the whole class — so it binds no child
    // and holds no handle. Its seven shipped buttons live in the layout under page `0x10000212`;
    // five are answered by the element-message handler and two by `BUTTON_INPUT_ACTION`. The module
    // is `crate::options::gameplay`.
    p(
        "GameplayOptionsPanel",
        ty::GAMEPLAY_OPTIONS,
        NONE,
        &[1],
        &[],
    ),
    p("KeyboardPanel", ty::KEYBOARD, KEYBOARD, &[1, 0x19], &[]),
];

/// The `CombatPanelStack` spec, kept out of [`PANELS`] because its type is never registered.
pub const COMBAT_PANEL_SPEC: PanelSpec = p(
    "CombatPanelStack",
    crate::element_types::COMBAT_PANEL_UNREGISTERED,
    COMBAT_PANEL,
    &[0x18],
    &[],
);

/// Look one panel up by its local behavior label.
#[must_use]
pub fn spec(class: &str) -> Option<&'static PanelSpec> {
    PANELS.iter().find(|p| p.class == class)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// every panel type in the catalogue must be a type the
    /// client actually registers, and no type may appear twice.
    #[test]
    fn every_catalogued_panel_is_a_registered_game_element_type() {
        let registered: BTreeSet<u32> = crate::element_types::REGISTRATION_ORDER
            .iter()
            .map(|r| r.ty.0)
            .collect();
        let mut seen = BTreeSet::new();
        for s in PANELS {
            assert!(
                registered.contains(&s.ty.0),
                "{} type {:?} is not registered",
                s.class,
                s.ty
            );
            assert!(seen.insert(s.ty.0), "{} duplicates a type", s.class);
        }
        assert!(!registered.contains(&COMBAT_PANEL_SPEC.ty.0));
    }

    /// Oracle: the toolbar and panel behavior: sixteen page containers and seven buttons,
    /// all distinct, and the inventory button is the last of the seven.
    #[test]
    fn the_panel_stack_and_toolbar_button_tables_are_the_documented_ones() {
        assert_eq!(PANEL_PAGES.len(), 16);
        let mut pages = PANEL_PAGES.to_vec();
        pages.sort_unstable();
        pages.dedup();
        assert_eq!(pages.len(), 16, "the sixteen page ids are distinct");
        assert_eq!(PANEL_PAGES[0], 0x1000_018B);
        assert_eq!(PANEL_PAGES[15], 0x1000_0188);

        assert_eq!(TOOLBAR_PANEL_BUTTONS.len(), 7);
        assert_eq!(
            *TOOLBAR_PANEL_BUTTONS.last().unwrap(),
            0x1000_01B1,
            "the inventory button"
        );
        assert_eq!(TOOLBAR_COMBAT_BUTTONS.len(), 4);
    }

    /// "24 slot elements plus the rendered figure".
    #[test]
    fn the_paper_doll_binds_twenty_four_slots() {
        let slots = PAPER_DOLL
            .iter()
            .filter(|c| c.field.starts_with("slot_"))
            .count();
        assert_eq!(slots, 24);
        assert_eq!(
            PAPER_DOLL.len(),
            24 + 4,
            "plus the doll, the mask, the overlay and the checkbox"
        );
    }

    /// The spellbook has five schools and exactly eight level buttons.
    #[test]
    fn the_spellbook_has_five_schools_and_exactly_eight_level_buttons() {
        let schools = SPELLBOOK
            .iter()
            .filter(|c| c.field.starts_with("btn_school_"))
            .count();
        let levels = SPELLBOOK
            .iter()
            .filter(|c| c.field.starts_with("btn_level_"))
            .count();
        assert_eq!(schools, 5);
        assert_eq!(levels, 8);
    }

    /// Oracle: `client_local_English.dat` — the eight panels whose documents list "(plus `0x…`)"
    /// ids that are **not** post-init bindings but row and popup templates on a sibling root of
    /// the same layout. The acceptance gate proved it: every one of these resolves nowhere under
    /// its panel's root, and every one of them is a root (or a root's child) elsewhere in the same
    /// layout.
    #[test]
    fn the_eight_panels_with_row_templates_keep_them_out_of_their_post_init_tables() {
        let with_templates: Vec<&str> = PANELS
            .iter()
            .filter(|p| !p.templates.is_empty())
            .map(|p| p.class)
            .collect();
        assert_eq!(
            with_templates,
            vec![
                "TitlesPanel",
                "SpellComponentPanel",
                "FriendsPanel",
                "SquelchPanel",
                "PageListPanel",
                "ContractsPanel",
                "BookPanel",
                "MiniGamePanel",
            ]
        );
        // A template id is never also a post-init binding.
        for p in PANELS {
            for t in p.templates {
                assert!(
                    !p.children.iter().any(|c| c.id == t.id),
                    "{}: {} is in both tables",
                    p.class,
                    t.field
                );
            }
        }
        assert_eq!(spec("SpellComponentPanel").unwrap().templates.len(), 4);
        assert_eq!(spec("PageListPanel").unwrap().templates.len(), 4);
    }

    /// The endowment icon's three children are named and nested.
    #[test]
    fn the_endowment_icons_three_children_are_named_and_nested() {
        use dereth_ui::ElementId;
        let s = spec("SpellcastingPanel").expect("SpellcastingPanel is catalogued");
        let icon = ElementId(0x1000_00B1);
        for (field, id) in [
            ("endowment_icon_underlay", 0x1000_0452u32),
            ("endowment_icon_overlay", 0x1000_0453),
            ("endowment_icon_selected", 0x1000_0454),
        ] {
            let b = s
                .children
                .iter()
                .find(|c| c.field == field)
                .unwrap_or_else(|| panic!("{field} is in the spec under its field name"));
            assert_eq!(b.id, ElementId(id), "{field}");
            assert_eq!(
                b.parent,
                Some(icon),
                "{field} is looked up inside the endowment icon"
            );
        }
        // The other five are flat: the post-init searches the panel root for each.
        for field in [
            "spellcast_background",
            "spellcast_panel",
            "endowment_icon",
            "spellcast_button",
            "spell_name",
        ] {
            let b = s
                .children
                .iter()
                .find(|c| c.field == field)
                .expect("catalogued");
            assert_eq!(b.parent, None, "{field}");
        }
        assert_eq!(s.children.len(), 8);
        assert!(
            !s.children
                .iter()
                .any(|c| matches!(c.field, "underlay" | "overlay" | "selection_ring")),
            "the three mis-transcribed names are gone"
        );
    }
}
