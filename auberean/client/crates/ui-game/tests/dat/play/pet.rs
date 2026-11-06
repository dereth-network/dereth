//! The pet panel on the retail layouts: a pet added, its mode turned, commanded, attacked, gone.

use auberean_text::StringInfo;
use auberean_ui::Token;
use auberean_ui_game::gameplay::{Record, Request};
use auberean_ui_game::pet::{
    next_mode, notice, previous_mode, shown_name, PetRequest, Pets, HIDE_PET_UI, MODES,
};

use super::stage::{long, said, World, ICON};

const PET: u64 = 0x5000_0000_0000_0301;
const FOE: u64 = 0x5000_0000_0000_0302;

fn pets(out: &[Request]) -> Vec<PetRequest> {
    out.iter()
        .filter_map(|r| match r {
            Request::Pet(p) => Some(p.clone()),
            _ => None,
        })
        .collect()
}

/// The modes turn through their nine in order and back round; a mode not among them goes to the
/// first going on and the last going back. A pet's name loses its owner's part in parentheses.
///
/// Behaviour: ui.pet.the-modes-turn-in-order-and-names-lose-their-owner
#[test]
fn the_modes_turn_in_order_and_names_lose_their_owner() {
    for (i, (m, _)) in MODES.iter().enumerate() {
        assert_eq!(next_mode(*m), MODES[(i + 1) % 9].0);
        assert_eq!(previous_mode(*m), MODES[(i + 8) % 9].0);
    }
    assert_eq!(next_mode(7), MODES[0].0);
    assert_eq!(previous_mode(7), MODES[8].0);
    assert_eq!(shown_name("Rufus (Ann's pet)"), "Rufus ");
    assert_eq!(shown_name("(Rufus)"), "(Rufus)");
    assert_eq!(shown_name("Rufus"), "Rufus");
}

/// A pet shows the panel on a cleared profile; the bit of the player's options set hides it.
///
/// Behaviour: ui.pet.the-options-bit-hides-the-pet-panel
#[test]
fn the_options_bit_hides_the_pet_panel() {
    for (bits, shown) in [(0, true), (HIDE_PET_UI, false)] {
        let mut w = World::new();
        w.options.bits |= bits;
        w.tick(&[]);
        let panel = w.el("PetPanel");
        let name = Some(Record::Text(StringInfo::literal("Rufus (Ann's pet)")));
        let [hi, lo] = long(PET);
        w.tick(&[said(
            notice::ADD_PET,
            vec![hi, lo, 0, ICON, MODES[0].0],
            vec![None, None, name],
        )]);
        assert_eq!(w.shown(panel), shown, "bits {bits:#x}");
    }
}

/// The mode buttons step from the mode last asked, not the one the pet has taken: Next twice before
/// the pet answers is two steps on, and Previous then one back.
///
/// Behaviour: ui.pet.the-mode-buttons-step-from-the-mode-asked
#[test]
fn the_mode_buttons_step_from_the_mode_asked() {
    let mut w = World::new();
    w.tick(&[]);
    let panel = w.el("PetPanel");
    let name = Some(Record::Text(StringInfo::literal("Rufus (Ann's pet)")));
    let [hi, lo] = long(PET);
    w.tick(&[said(
        notice::ADD_PET,
        vec![hi, lo, 0, ICON, MODES[0].0],
        vec![None, None, name],
    )]);
    let list = w.under(panel, "PetPanelListBox");
    let row = w.ui.list_items(list)[0];
    let (x, y) = w.middle(row);
    w.ui.pointer_move(x, y);
    w.click(w.under(row, "NextMode"));
    w.ui.pointer_move(x, y);
    w.click(w.under(row, "NextMode"));
    assert_eq!(w.state(w.under(row, "Mode")), Token::of(MODES[2].1));
    w.ui.pointer_move(x, y);
    w.click(w.under(row, "PreviousMode"));
    assert_eq!(w.state(w.under(row, "Mode")), Token::of(MODES[1].1));
}

/// With the player's option on, a pet the scripts add shows in a row (its name without its
/// owner's part, its mode's look) and the panel shows, its commands lit. The next-mode button
/// shows the next mode at once and asks the scripts for it within five seconds, again every five
/// seconds until they say the pet took it. Attack commands the pets at the selection. A pet under
/// attack shows unsafe until five seconds after; a pet gone takes its row, and with none the
/// panel hides.
///
/// Behaviour: ui.pet.the-pet-panel-lists-pets-and-commands-them
#[test]
fn the_pet_panel_lists_pets_and_commands_them() {
    let mut w = World::new();
    w.tick(&[]);
    let panel = w.el("PetPanel");
    assert!(!w.shown(panel), "no pets");

    let name = Some(Record::Text(StringInfo::literal("Rufus (Ann's pet)")));
    let [hi, lo] = long(PET);
    w.tick(&[said(
        notice::ADD_PET,
        vec![hi, lo, 0, ICON, MODES[0].0],
        vec![None, None, name],
    )]);
    assert!(w.shown(panel), "a pet");
    let list = w.under(panel, "PetPanelListBox");
    let rows = w.ui.list_items(list);
    assert_eq!(rows.len(), 1);
    assert_eq!(w.text(w.under(rows[0], "Name")), "Rufus ");
    assert_eq!(w.state(w.under(rows[0], "Mode")), Token::of("Aggressive"));
    let attack = w.under(panel, "AttackButton");
    assert!(!w.ghosted(attack));

    // The next mode: shown at once, asked within five seconds, again until taken.
    let (x, y) = w.middle(rows[0]);
    w.ui.pointer_move(x, y);
    let out = w.click(w.under(rows[0], "NextMode"));
    assert!(pets(&out).is_empty(), "{out:?}");
    assert_eq!(
        w.state(w.under(rows[0], "Mode")),
        Token::of("Opportunistic")
    );
    let asked = PetRequest::Mode {
        pet: PET,
        mode: MODES[1].0,
    };
    let out = w.wait(5.5);
    assert_eq!(pets(&out), vec![asked.clone()]);
    let out = w.wait(5.5);
    assert_eq!(pets(&out), vec![asked], "asked again");
    w.tick(&[said(
        notice::UPDATE_PET_MODE,
        vec![hi, lo, MODES[1].0],
        Vec::new(),
    )]);
    let p = w.play.child::<Pets>().expect("the pet panel");
    assert_eq!(
        p.pet(PET).map(|x| (x.1, x.2)),
        Some((MODES[1].0, MODES[1].0))
    );
    let out = w.wait(5.5);
    assert!(pets(&out).is_empty(), "taken: not asked again");

    // Attack at the selection.
    let [fh, fl] = long(FOE);
    w.tick(&[said(notice::SELECTION_CHANGED, vec![fh, fl], Vec::new())]);
    let out = w.click(attack);
    assert_eq!(pets(&out), vec![PetRequest::Attack(FOE)]);

    // Under attack, then safe again.
    w.tick(&[said(notice::PET_IS_UNDER_ATTACK, vec![hi, lo], Vec::new())]);
    let name_field = w.under(rows[0], "Name");
    assert_eq!(w.state(name_field), Token::of("unsafe"));
    w.wait(11.0);
    assert_eq!(w.state(name_field), Token::of("safe"));

    // Gone: no pets, hidden.
    w.tick(&[said(notice::REMOVE_PET, vec![hi, lo], Vec::new())]);
    assert!(w.ui.list_items(list).is_empty());
    assert!(!w.shown(panel), "no pets");
}
