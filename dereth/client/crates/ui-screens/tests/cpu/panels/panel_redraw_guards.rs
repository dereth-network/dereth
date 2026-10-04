//! Vendor, trade, allegiance, spell-component and skills panels' early-out guards let through a
//! change to a field the rebuild reads while the id list is unchanged.
//! Fixture: synthetic views and UI state; the catalogue check also reads production source files.

use super::common::*;
use dereth_primitives::{DataId, ObjectId};
use dereth_ui_screens::panels::allegiance::AllegiancePanel;
use dereth_ui_screens::panels::skills::{self, SkillsPanel};
use dereth_ui_screens::panels::spellcomponent::SpellComponentPanel;
use dereth_ui_screens::panels::trade::TradePanel;
use dereth_ui_screens::panels::vendor::{Tab, VendorPanel};
use dereth_ui_screens::view::{
    AllegianceEntry, AllegianceRoster, ComponentCategory, ComponentRow, GameView, ShopRow,
    ShopView, SkillEntry, TradeRow, TradeView,
};

const ITEM: ObjectId = ObjectId(0x8000_0A6E);
const ICON: DataId = DataId(0x0600_103F);

// ---------------------------------------------------------------------------------------------
// The views. Each implements exactly the one accessor its panel reads.
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Shop(ShopView);
impl GameView for Shop {
    fn shop(&self) -> ShopView {
        self.0.clone()
    }
}

#[derive(Debug)]
struct Trade(TradeView);
impl GameView for Trade {
    fn trade(&self) -> TradeView {
        self.0.clone()
    }
}

#[derive(Debug)]
struct Roster(AllegianceRoster);
impl GameView for Roster {
    fn allegiance_roster(&self) -> AllegianceRoster {
        self.0.clone()
    }
}

#[derive(Debug)]
struct Comps(Vec<ComponentCategory>);
impl GameView for Comps {
    fn spell_components(&self) -> Vec<ComponentCategory> {
        self.0.clone()
    }
}

#[derive(Debug)]
struct Skills(Vec<SkillEntry>);
impl GameView for Skills {
    fn skills(&self) -> &[SkillEntry] {
        &self.0
    }
}

// ---------------------------------------------------------------------------------------------
// 1. vendor
// ---------------------------------------------------------------------------------------------

fn shop_with(icon: Option<DataId>, price: i32) -> Shop {
    Shop(ShopView {
        open: true,
        vendor: Some(ObjectId(0x8000_0001)),
        buy_list: vec![ShopRow {
            item: ITEM,
            name: "Oil of Rendering".into(),
            icon,
            amount: -1,
            // `TYPE_MISC`, so the row is a real one even where nothing tests it.
            obj_type: 0x0000_0080,
            max_stack_size: 0,
            contained_items: 0,
            contained_containers: 0,
            price,
            refusal: None,
        }],
        ..ShopView::default()
    })
}

/// Behaviour: panels.redraw.a-shops-stock-follows-a-picture-or-a-price-arriving-under-an-unchanged-row
/// `ShopRow` carries the icon and the guard compares the whole `ShopView`, so a stock row whose
/// picture resolves under an unchanged id **is** a snapshot change. That is the mechanism, and it
/// is the one the inventory did not have: its icon came from a closure the guard never saw.
///
/// Both directions are asserted; this does not inspect the slot icon element itself.
#[test]
fn the_vendor_guard_sees_an_icon_resolve_under_an_unchanged_stock_id() {
    let mut ui = ui();
    let mut p = VendorPanel::default();

    let blank = shop_with(None, 5);
    assert!(
        p.update(&mut ui, &blank),
        "the first drive is always a rebuild"
    );
    assert_eq!(p.rebuilds, 1);
    assert_eq!(
        p.rows(Tab::Buying).len(),
        1,
        "the premise: the row reached the panel"
    );
    assert_eq!(p.rows(Tab::Buying)[0].icon, None);

    assert!(
        !p.update(&mut ui, &blank),
        "the guard is live: an identical frame is a no-op"
    );
    assert_eq!(p.rebuilds, 1);

    let lit = shop_with(Some(ICON), 5);
    assert_eq!(
        lit.0.buy_list.iter().map(|r| r.item).collect::<Vec<_>>(),
        blank.0.buy_list.iter().map(|r| r.item).collect::<Vec<_>>(),
        "the id list is identical across the two stations -- that is the whole point"
    );
    assert!(
        p.update(&mut ui, &lit),
        "the descriptor arriving is itself a snapshot change"
    );
    assert_eq!(p.rebuilds, 2);
    assert_eq!(p.rows(Tab::Buying)[0].icon, Some(ICON));

    let repriced = shop_with(Some(ICON), 9);
    assert!(
        p.update(&mut ui, &repriced),
        "and so is a re-price under the same id"
    );
    assert_eq!(p.rows(Tab::Buying)[0].price, 9);
    assert_eq!(p.rebuilds, 3);
}

// ---------------------------------------------------------------------------------------------
// 2. trade
// ---------------------------------------------------------------------------------------------

fn trade_with(name: &str, icon: Option<DataId>) -> Trade {
    Trade(TradeView {
        open: true,
        partner: Some(ObjectId(0x8000_0002)),
        partner_name: "Alba".into(),
        self_rows: Vec::new(),
        partner_rows: vec![TradeRow {
            item: ITEM,
            name: name.to_owned(),
            icon,
        }],
        accepted: false,
        partner_accepted: false,
        self_removed: Vec::new(),
        acceptance_darkened: false,
    })
}

/// Behaviour: panels.redraw.the-partners-side-of-the-trade-follows-a-row-being-filled-in-later
/// The transition `trade_view.rs`'s own header predicts: *"the partner's items arrive as ordinary
/// `0xF745` creates and the `0x0200` that names them can beat the create"*, so a partner row can
/// exist with an empty name and no icon and fill in a datagram later. `TradeRow` carries both, and
/// the guard compares the whole `TradeView`.
#[test]
fn the_trade_guard_sees_a_partner_rows_weenie_arrive_after_the_row() {
    let mut ui = ui();
    let mut p = TradePanel::default();

    let before = trade_with("", None);
    assert!(
        p.update(&mut ui, &before),
        "the first drive is always a rebuild"
    );
    assert_eq!(p.rebuilds, 1);
    assert_eq!(
        p.rows(true).len(),
        1,
        "the premise: the row reached the panel"
    );
    assert_eq!(p.rows(true)[0].name, "");

    assert!(!p.update(&mut ui, &before), "the guard is live");
    assert_eq!(p.rebuilds, 1);

    let after = trade_with("Oil of Rendering", Some(ICON));
    assert_eq!(
        after
            .0
            .partner_rows
            .iter()
            .map(|r| r.item)
            .collect::<Vec<_>>(),
        before
            .0
            .partner_rows
            .iter()
            .map(|r| r.item)
            .collect::<Vec<_>>(),
        "the id list is identical across the two stations"
    );
    assert!(
        p.update(&mut ui, &after),
        "the create landing is itself a snapshot change"
    );
    assert_eq!(p.rebuilds, 2);
    assert_eq!(p.rows(true)[0].name, "Oil of Rendering");
    assert_eq!(p.rows(true)[0].icon, Some(ICON));
}

// ---------------------------------------------------------------------------------------------
// 3. allegiance
// ---------------------------------------------------------------------------------------------

fn roster_with(logged_in: bool, cp: u32) -> Roster {
    Roster(AllegianceRoster {
        allegiance_name: "The Hand of Dereth".into(),
        total_members: 6,
        total_vassals: 1,
        own_cp_tithed: 0,
        subject: None,
        player_rank_quality: 0,
        monarch: None,
        patron: None,
        vassals: vec![AllegianceEntry {
            id: ObjectId(10),
            full_name: "Yeoman Dee".into(),
            logged_in,
            rank: 1,
            cp_cached: cp,
        }],
    })
}

/// Behaviour: panels.redraw.the-vassal-list-follows-a-members-own-fields-changing-under-an-unchanged-id
/// The vassal-data draw writes two per-node fields that move without the roster's shape
/// moving — the logged-in flag and the cached CP — and an allegiance update is a **full
/// replace**, so the same vassal arrives again with the same id. Both are `Eq` members of
/// `AllegianceEntry` and the guard compares the whole `AllegianceRoster`.
#[test]
fn the_allegiance_guard_sees_a_vassals_fields_move_under_an_unchanged_id() {
    let mut ui = ui();
    let mut p = AllegiancePanel::default();

    let before = roster_with(true, 100);
    assert!(
        p.update(&mut ui, &before),
        "the first drive is always a rebuild"
    );
    assert_eq!(p.rebuilds, 1);
    assert!(!p.update(&mut ui, &before), "the guard is live");
    assert_eq!(p.rebuilds, 1);

    let after = roster_with(false, 250);
    assert_eq!(
        after.0.vassals.iter().map(|v| v.id).collect::<Vec<_>>(),
        before.0.vassals.iter().map(|v| v.id).collect::<Vec<_>>(),
        "the id list is identical across the two stations"
    );
    assert!(
        p.update(&mut ui, &after),
        "logging out and passing up experience are both changes"
    );
    assert_eq!(p.rebuilds, 2);
}

// ---------------------------------------------------------------------------------------------
// 4. spellcomponent
// ---------------------------------------------------------------------------------------------

const SCARAB_WCID: u32 = 33835;

fn comps_with(owned: i64, icon: Option<DataId>) -> Comps {
    Comps(vec![ComponentCategory {
        category: 0,
        rows: vec![ComponentRow {
            wcid: SCARAB_WCID,
            name: "Lead Scarab".into(),
            icon,
            owned,
            desired: 5,
            object: Some(ITEM),
        }],
    }])
}

/// Behaviour: panels.redraw.the-component-list-follows-a-count-changing-under-an-unchanged-component
/// The component region is re-synced to rewrite `0x1000046A` when a component is
/// spent or bought, and spending a scarab you already own moves no wcid. `ComponentRow` derives
/// `Eq` over all six fields — the same six `write_row` writes — and the guard compares the whole
/// `Vec<ComponentCategory>`.
#[test]
fn the_spellcomponent_guard_sees_an_owned_count_move_under_an_unchanged_wcid() {
    let mut ui = ui();
    let mut p = SpellComponentPanel::default();

    let before = comps_with(12, None);
    assert!(
        p.update(&mut ui, &before),
        "the first drive is always a rebuild"
    );
    assert_eq!(p.rebuilds, 1);
    assert!(!p.update(&mut ui, &before), "the guard is live");
    assert_eq!(p.rebuilds, 1);

    let after = comps_with(7, Some(ICON));
    assert_eq!(
        after.0[0].rows[0].wcid, before.0[0].rows[0].wcid,
        "the same wcid, both stations"
    );
    assert!(
        p.update(&mut ui, &after),
        "spending three scarabs is a snapshot change"
    );
    assert_eq!(p.rebuilds, 2);
}

fn skill(level: i32, effective: i32, vitae: i32) -> Skills {
    Skills(vec![SkillEntry {
        id: 6,
        name: "Melee Defense".into(),
        icon: None,
        min_level: 0,
        sac: skills::sac::TRAINED,
        level,
        effective,
        vitae,
    }])
}

/// Behaviour: panels.redraw.a-skill-row-follows-the-enchanted-number-and-the-death-penalty-each-on-their-own
/// The skills guard sees the enchanted value it draws.
#[test]
fn the_skills_guard_sees_the_enchanted_value_it_draws() {
    let mut ui = ui();
    let mut p = SkillsPanel::default();

    let plain = skill(72, 72, 0);
    assert!(
        p.update(&mut ui, &plain),
        "the first drive is always a rebuild"
    );
    assert!(
        !p.update(&mut ui, &plain),
        "the guard is live: an identical frame is a no-op"
    );

    // 1. `vitae` alone, entered from `plain`: `level` and `effective` both held at 72.
    //
    // It **isolates** the field rather than modelling a character: a live -5 modifier means
    // the non-raw skill value already carries the penalty, so in the world `effective` would have
    // moved with it. This file measures the refresh guard rather than drawn elements.
    let vitae = skill(72, 72, -5);
    assert_eq!(
        vitae
            .0
            .iter()
            .map(|s| (s.id, s.level, s.effective))
            .collect::<Vec<_>>(),
        plain
            .0
            .iter()
            .map(|s| (s.id, s.level, s.effective))
            .collect::<Vec<_>>(),
        "the id list, the raw level and the enchanted total all hold -- only `vitae` moves"
    );
    assert!(
        p.update(&mut ui, &vitae),
        "a death penalty arriving is a snapshot change"
    );
    assert!(
        !p.update(&mut ui, &vitae),
        "and the guard closes again behind it"
    );

    // 2. And back: the same one field, moving the other way. A guard is a claim about an edge, and
    // an edge has two of them.
    assert!(
        p.update(&mut ui, &plain),
        "and recovering from it is a change too"
    );
    assert!(
        !p.update(&mut ui, &plain),
        "and the guard closes again behind it"
    );

    // 3. `effective` alone, entered from `plain`: `level` at 72 and `vitae` at 0 in both frames.
    let buffed = skill(72, 77, 0);
    assert_eq!(
        buffed
            .0
            .iter()
            .map(|s| (s.id, s.level, s.vitae))
            .collect::<Vec<_>>(),
        plain
            .0
            .iter()
            .map(|s| (s.id, s.level, s.vitae))
            .collect::<Vec<_>>(),
        "the id list, the raw level and the vitae modifier all hold -- only `effective` moves"
    );
    assert!(
        p.update(&mut ui, &buffed),
        "a buff landing is a snapshot change"
    );
    assert!(
        !p.update(&mut ui, &buffed),
        "and the guard closes again behind it"
    );

    // 4. Both at once, which is a real frame -- a debuff landing while vitae is up.
    let both = skill(72, 60, -5);
    assert!(
        p.update(&mut ui, &both),
        "effective and vitae moving together"
    );

    let raised = skill(73, 78, 0);
    assert!(
        p.update(&mut ui, &raised),
        "a raw-level change is inside the guard and redraws"
    );
}
