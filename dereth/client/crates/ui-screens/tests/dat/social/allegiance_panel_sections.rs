//! Monarch/patron sections hide/show by roster (monarch sees neither, patron-is-monarch collapses,
//! lower patron shows both, unaffiliated hides both). Rendering does not mutate chat availability.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::ObjectId;
use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::allegiance::{
    MONARCH_FIELD, MONARCH_PATRON_BLOCK, PATRON_FIELD, PLAYER_RANK,
};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{AllegianceEntry, AllegianceRoster, GameView, UiRequest};

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

fn screen() -> (UiSystem, GamePlayScreen, RemainingPanels) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    let root = s.root().expect("the gameplay root");
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    assert!(
        panels.allegiance.bound(),
        "the allegiance panel's list box is in the shipped tree"
    );
    ui.requests.clear();
    (ui, s, panels)
}

fn find(ui: &UiSystem, s: &GamePlayScreen, id: ElementId) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped gameplay layout"))
}

/// The element's **own** visible bit — what `SetVisible` writes — rather than
/// the effective-visibility query's walk to the root, because the social page above these fields is
/// hidden until the player opens it and would mask both answers as "hidden".
fn own_visible(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).expect("a bound element").region.flags.visible
}

fn entry(id: u32, name: &str, on: bool) -> AllegianceEntry {
    AllegianceEntry {
        id: ObjectId(id),
        full_name: name.to_string(),
        logged_in: on,
        rank: 3,
        cp_cached: 100 * id,
    }
}

#[derive(Debug)]
struct Roster(AllegianceRoster);
impl GameView for Roster {
    fn allegiance_roster(&self) -> AllegianceRoster {
        self.0.clone()
    }
}

/// The `SetTalkFocusEnabled(n, on)` calls the panel raised, in order, keyed by `n`.
fn talk_focus_calls(requests: &[UiRequest]) -> Vec<(u32, bool)> {
    requests
        .iter()
        .filter_map(|r| match r {
            UiRequest::SetTalkFocusEnabled { focus, enabled } => Some((*focus, *enabled)),
            _ => None,
        })
        .collect()
}

/// Behaviour: allegiance.panel.a-monarch-has-no-monarch-row-and-no-patron-row
/// A monarch sees neither the monarch nor the patron section.
#[test]
fn a_monarch_sees_neither_the_monarch_nor_the_patron_section() {
    let (mut ui, s, mut panels) = screen();
    let me = entry(1, "High King Lark", true);
    let view = Roster(AllegianceRoster {
        allegiance_name: "The Hand".into(),
        total_members: 3,
        total_vassals: 1,
        own_cp_tithed: 0,
        subject: Some(me.clone()),
        player_rank_quality: 3,
        monarch: Some(me),
        patron: None,
        vassals: vec![
            entry(10, "Yeoman Dee", true),
            entry(11, "Yeoman Eve", false),
        ],
    });
    assert!(panels.allegiance.update(&mut ui, &view));
    let monarch_field = find(&ui, &s, MONARCH_FIELD);
    let patron_field = find(&ui, &s, PATRON_FIELD);
    assert!(
        !own_visible(&ui, monarch_field),
        "0x10000255 hidden: the monarch is us"
    );
    assert!(
        !own_visible(&ui, patron_field),
        "0x1000025A hidden: there is no patron"
    );

    assert!(
        talk_focus_calls(&ui.requests.take()).is_empty(),
        "rendering does not mutate shared focus state"
    );
}

/// A patron who is the monarch collapses into the monarch section.
#[test]
fn a_patron_who_is_the_monarch_collapses_into_the_monarch_section() {
    let (mut ui, s, mut panels) = screen();
    let bob = entry(9, "High King Bob", true);
    let view = Roster(AllegianceRoster {
        allegiance_name: "The Hand".into(),
        total_members: 2,
        total_vassals: 0,
        own_cp_tithed: 0,
        subject: Some(entry(1, "Baron Lark", true)),
        player_rank_quality: 3,
        monarch: Some(bob.clone()),
        patron: Some(bob),
        vassals: Vec::new(),
    });
    assert!(panels.allegiance.update(&mut ui, &view));
    assert!(
        own_visible(&ui, find(&ui, &s, MONARCH_FIELD)),
        "0x10000255 shown"
    );
    assert!(
        !own_visible(&ui, find(&ui, &s, PATRON_FIELD)),
        "0x1000025A hidden"
    );
    assert!(
        own_visible(&ui, find(&ui, &s, MONARCH_PATRON_BLOCK)),
        "0x10000490 shown"
    );

    assert!(
        talk_focus_calls(&ui.requests.take()).is_empty(),
        "rendering does not mutate shared focus state"
    );
}

/// Behaviour: allegiance.panel.a-monarch-has-no-monarch-row-and-no-patron-row
/// A patron below the monarch: both sections shown, `0x10000490` hidden, and each
/// channel follows its own person's online bit — here the monarch is offline and the patron is
/// online, so 5 is off and 4 is on.
#[test]
fn a_patron_below_the_monarch_shows_both_sections_and_follows_each_online_bit() {
    let (mut ui, s, mut panels) = screen();
    let view = Roster(AllegianceRoster {
        allegiance_name: "The Hand".into(),
        total_members: 4,
        total_vassals: 2,
        own_cp_tithed: 0,
        subject: Some(entry(1, "Baron Lark", true)),
        player_rank_quality: 3,
        monarch: Some(entry(9, "High King Bob", false)),
        patron: Some(entry(5, "Count Cid", true)),
        vassals: vec![entry(11, "Yeoman Eve", false)],
    });
    assert!(panels.allegiance.update(&mut ui, &view));
    assert!(
        own_visible(&ui, find(&ui, &s, MONARCH_FIELD)),
        "0x10000255 shown"
    );
    assert!(
        own_visible(&ui, find(&ui, &s, PATRON_FIELD)),
        "0x1000025A shown"
    );
    assert!(
        !own_visible(&ui, find(&ui, &s, MONARCH_PATRON_BLOCK)),
        "0x10000490 hidden"
    );

    assert!(
        talk_focus_calls(&ui.requests.take()).is_empty(),
        "rendering does not mutate shared focus state"
    );

    // Flipping online bits changes the projected roster without emitting channel updates.
    let view = Roster(AllegianceRoster {
        allegiance_name: "The Hand".into(),
        total_members: 4,
        total_vassals: 2,
        own_cp_tithed: 0,
        subject: Some(entry(1, "Baron Lark", true)),
        player_rank_quality: 3,
        monarch: Some(entry(9, "High King Bob", true)),
        patron: Some(entry(5, "Count Cid", false)),
        vassals: vec![entry(11, "Yeoman Eve", true)],
    });
    assert!(panels.allegiance.update(&mut ui, &view));
    assert!(
        talk_focus_calls(&ui.requests.take()).is_empty(),
        "rendering does not mutate shared focus state"
    );
}

/// The state every capture witnesses: no allegiance at all. The monarch is 0 and the
/// patron lookup fails, so both sections are hidden and all three channels are off.
#[test]
fn an_unaffiliated_character_has_both_sections_hidden() {
    let (mut ui, s, mut panels) = screen();
    let view = Roster(AllegianceRoster::default());
    assert!(panels.allegiance.update(&mut ui, &view));
    assert!(!own_visible(&ui, find(&ui, &s, MONARCH_FIELD)));
    assert!(!own_visible(&ui, find(&ui, &s, PATRON_FIELD)));
    assert!(talk_focus_calls(&ui.requests.take()).is_empty());
}

/// [`screen`] with the shipped string tables installed, so the rank line is the text a player reads.
fn screen_with_strings() -> (UiSystem, GamePlayScreen, RemainingPanels) {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui.strings = Some(std::rc::Rc::new(crate::common::layout::Strings(store)));
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    let root = s.root().expect("the gameplay root");
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    ui.requests.clear();
    (ui, s, panels)
}

/// The text on the rank line, `0x10000253`.
fn rank_line(ui: &mut UiSystem, s: &GamePlayScreen) -> String {
    let h = find(ui, s, PLAYER_RANK);
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// A baron at stored rank 3 whose enchanted rank read answered `rank_quality`.
fn baron(rank_quality: i32) -> Roster {
    Roster(AllegianceRoster {
        allegiance_name: "The Hand".into(),
        total_members: 3,
        total_vassals: 0,
        own_cp_tithed: 0,
        subject: Some(entry(1, "Baron Lark", true)),
        player_rank_quality: rank_quality,
        monarch: Some(entry(9, "High King Bob", true)),
        patron: Some(entry(9, "High King Bob", true)),
        vassals: vec![],
    })
}

/// Behaviour: allegiance.panel.a-rank-buff-shows-the-buffed-rank-and-how-far-it-moved
#[test]
fn a_rank_buff_shows_the_buffed_rank_and_how_far_it_moved() {
    let (mut ui, s, mut panels) = screen_with_strings();
    assert!(panels.allegiance.update(&mut ui, &baron(3)));
    let plain = rank_line(&mut ui, &s);
    assert_eq!(
        plain, "Rank: Baron [3]",
        "the enchanted rank equals the tree's"
    );

    let (mut ui, s, mut panels) = screen_with_strings();
    assert!(panels.allegiance.update(&mut ui, &baron(4)));
    let buffed = rank_line(&mut ui, &s);
    assert_eq!(buffed, "Rank: Baron [4 (+1)]", "one rank above the tree's");

    let (mut ui, s, mut panels) = screen_with_strings();
    assert!(panels.allegiance.update(&mut ui, &baron(2)));
    // The shipped row carries a literal `+` before the difference, so a lowered rank reads `+-1`.
    assert_eq!(
        rank_line(&mut ui, &s),
        "Rank: Baron [2 (+-1)]",
        "one rank below the tree's"
    );

    let (mut ui, s, mut panels) = screen_with_strings();
    assert!(panels.allegiance.update(&mut ui, &baron(-1)));
    assert_eq!(rank_line(&mut ui, &s), plain, "-1 takes the plain form too");
}

/// Behaviour: allegiance.panel.a-rank-buff-changing-redraws-the-players-line-alone
#[test]
fn a_rank_buff_changing_redraws_the_players_line_and_nothing_else() {
    let (mut ui, s, mut panels) = screen_with_strings();
    assert!(panels.allegiance.update(&mut ui, &baron(3)));
    assert_eq!(panels.allegiance.rebuilds, 1);
    let plain = rank_line(&mut ui, &s);
    ui.requests.clear();
    // An update request is out: the busy latch is armed until the shard answers.
    panels.allegiance.awaiting_update = true;

    assert!(
        panels.allegiance.update(&mut ui, &baron(4)),
        "the rank line was rewritten"
    );
    assert_ne!(rank_line(&mut ui, &s), plain, "it shows the buff at once");
    assert_eq!(
        panels.allegiance.rebuilds, 1,
        "not an allegiance update: the four blocks were not rebuilt"
    );
    assert!(
        panels.allegiance.awaiting_update,
        "and the busy latch still waits for the shard's answer"
    );
    assert_eq!(
        talk_focus_calls(&ui.requests.take()),
        vec![],
        "the channels were not re-decided"
    );
    assert!(
        !panels.allegiance.update(&mut ui, &baron(4)),
        "the same buff again changes nothing"
    );
}
