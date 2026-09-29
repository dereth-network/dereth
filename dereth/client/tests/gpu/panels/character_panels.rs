//! The character panels draw their real contents: the attributes page's six attributes and three
//! vitals, the skills page's skills in their training groups with SkillTable names and icons, and
//! the spellbook's known spells with SpellTable names and icons in display order, with a school
//! filter; filled lists change pixels only inside their own boxes. The allegiance and fellowship
//! census asserts which recordings carry `Allegiance_AllegianceUpdate 0x0020` rosters and
//! fellowship traffic, and the login description answers before the player's row exists.
//! Fixture: the recorded sessions (`first-login-walk-jump`'s `0x0013 Login_PlayerDescription` is
//! the oracle; nothing about the character is written here), the retail SkillTable `0x0E000004`
//! and spell table `0x0E00000E`, and a headless App on a software GPU device with offscreen
//! captures; pages are opened through the panel-visibility handler, not OS input.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use std::collections::{BTreeMap, BTreeSet};

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::LocalTime;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::skills::SkillGroup;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

// ---------------------------------------------------------------------------------------------
// Harness — the same one `inventory_and_tabs.rs` and `quickbar.rs` use.
// ---------------------------------------------------------------------------------------------

/// **An `expect`, never a skip.** A test that returns early is counted as a pass and
/// is invisible in the summary line; if the retail dats are not where `$DERETH_TEST_DAT_DIR` says,
/// the run is not a pass.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

fn app_in_gameplay(frames: u32) -> Option<App> {
    let cfg = Config {
        ui: true,
        ..base_config()
    };
    let mut app = App::new(cfg).unwrap_or_else(|e| panic!("a headless App: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the headless App's UI shell: {e}"));
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s)
        .unwrap_or_else(|e| panic!("the static scene: {e}"));
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    Some(app)
}

fn gameplay_screen(app: &mut App) -> Option<(&mut UiSystem, &mut GamePlayScreen)> {
    let shell = app.ui_mut()?;
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut()?;
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any.downcast_mut::<GamePlayScreen>()?;
    Some((ui, screen))
}

/// Every recording the corpus index names, in name order.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}
fn replay(session: &str) -> Vec<SessionEvent> {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push(e);
        }
    }
    events
}

fn events_in_world(events: &[SessionEvent]) -> &[SessionEvent] {
    let seen_desc = events
        .iter()
        .position(|e| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture never reached 0x0013");
    let end = events[seen_desc..]
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        dereth_client_net::client_session::SessionState::CharacterSelect
                            | dereth_client_net::client_session::SessionState::Disconnected(_)
                    )
            )
        })
        .map_or(events.len(), |i| seen_desc + i);
    &events[..end]
}

/// The capture's own `0x0013` — the only oracle for what the character knows.
fn player_description(
    events: &[SessionEvent],
) -> Option<&dereth_protocol::login::LoginPlayerDescription> {
    events.iter().find_map(|e| match e {
        SessionEvent::PlayerDescription(d) => Some(&**d),
        _ => None,
    })
}

/// An application in the game phase with a recorded session's `0x0013` applied.
fn app_with_capture(session: &str) -> Option<(App, Vec<SessionEvent>)> {
    let events = replay(session);
    let mut app = app_in_gameplay(4)?;
    let _ = app.apply_hud_events(events_in_world(&events));
    for _ in 0..4 {
        app.frame();
    }
    Some((app, events))
}

/// The text a live element holds, through the current glyph-list text query.
fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// One row's `(icon DataID, label, value)` — `InfoRegion`'s three children.
fn row_of(ui: &mut UiSystem, h: ElemHandle) -> (Option<u32>, String, String) {
    use dereth_ui_screens::panels::skills::row;
    let icon = ui
        .get_child_recursive(h, ElementId(row::ICON))
        .and_then(|c| ui.node(c))
        .and_then(|n| n.region.image.as_ref().map(|g| g.did.0));
    let label = ui
        .get_child_recursive(h, ElementId(row::LABEL))
        .map_or_else(String::new, |c| text_of(ui, c));
    let value = ui
        .get_child_recursive(h, ElementId(row::VALUE))
        .map_or_else(String::new, |c| text_of(ui, c));
    (icon, label, value)
}

// ---------------------------------------------------------------------------------------------
// 1. The skills page.
// ---------------------------------------------------------------------------------------------

/// Behaviour: panels.skills.the-skills-page-draws-the-captures-skills
///
/// **The skills page draws the character's own skills in the captured training groups**, with
/// names and icons from retail data and a numeric value in every row.
///
/// The oracle is `fixtures/packet-captures/first-login-walk-jump`'s own `0x0013`: every skill id and training grade
/// is read back out of the message and compared against the panel's group placement.
/// Nothing about the character is written in this file.
///
/// Deleting the panel update from the HUD frame path, pointing
/// [`dereth_ui_screens::panels::skills::PANEL`] at the attribute page's `0x1000022B` instead of
/// the skill page's `0x1000022C`, or removing the `skill_table` load from `Hud::load_tables` each
/// empties the list and fails the first assertion.
#[test]
fn the_skills_page_draws_the_captures_own_skills_with_their_names_and_values() {
    have_dats();
    let (mut app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");

    // ---- the oracle, read out of the capture -------------------------------------------------
    let desc = player_description(&events).expect("0x0013");
    let table = desc
        .qualities
        .skills
        .as_ref()
        .expect("the capture's 0x0013 carries skills");
    let want: BTreeMap<u32, u32> = table.entries.iter().map(|(k, s)| (*k, s.sac)).collect();
    assert!(!want.is_empty(), "the capture's skill table is empty");

    // ---- what the panel actually built -------------------------------------------------------
    let rows: Vec<(u32, SkillGroup, ElemHandle)> = app
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.group, r.element))
        .collect();
    let headers = app.hud().panels.skills.headers.len();
    assert_eq!(
        headers, 4,
        "the skill panel adds four group headers, one per template 1..4"
    );

    // **The rows are in the skill page's list box and not the attribute page's.** Both sub-panels
    // of the character page carry list element `0x1000023D`, so
    // "there are rows" is not enough: the list they hang off has to be a descendant of the skill
    // sub-panel `0x1000022C` (type `0x1000002B`) and not of the attribute one `0x1000022B`.
    //
    // The oracle for "which sub-panel" is the **element type in the layout**, not this module's
    // own `PANEL` constant: the character page's sub-panels are found by walking it for a child
    // whose element type is `ty::SKILL` (`0x1000002B`) or `ty::ATTRIBUTE` (`0x1000002A`), which comes
    // out of `client_local_English.dat`. Asserting against `skills::PANEL` would be circular —
    // moving that constant to the attribute page would move the assertion with it.
    {
        use dereth_ui_screens::element_types::ty;
        let list = app
            .hud()
            .panels
            .skills
            .list
            .as_ref()
            .expect("no list box was bound")
            .handle;
        let (ui, screen) = gameplay_screen(&mut app).expect("screen");
        let root = screen.root().expect("root");
        let page = ui
            .get_child_recursive(root, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
            .expect("the character page is in the shipped layout");
        let by_type = |t: dereth_ui::ElementType| -> Option<ElemHandle> {
            let mut stack = vec![page];
            while let Some(h) = stack.pop() {
                if ui.node(h).map(|n| n.desc.ty) == Some(t) {
                    return Some(h);
                }
                stack.extend(ui.children(h));
            }
            None
        };
        let skill_panel =
            by_type(ty::SKILL).expect("the skill panel is a child of the character page");
        let attr_panel =
            by_type(ty::ATTRIBUTE).expect("the attribute panel is a child of the character page");
        assert_ne!(skill_panel, attr_panel);
        let skill_list =
            ui.get_child_recursive(skill_panel, dereth_ui_screens::panels::skills::LIST_BOX);
        let attr_list =
            ui.get_child_recursive(attr_panel, dereth_ui_screens::panels::skills::LIST_BOX);
        assert!(
            attr_list.is_some(),
            "the attribute panel carries 0x1000023D too — that is the trap"
        );
        assert_ne!(
            attr_list, skill_list,
            "the two sub-panels share one list box"
        );
        assert_eq!(
            skill_list,
            Some(list),
            "the rows were built into the attribute panel's list box, not the skill panel's"
        );
    }
    assert!(!rows.is_empty(), "the skills page built no rows at all");

    // Every skill the message named has a row, and no row names a skill the SkillTable does not.
    // The value field is checked below for numeric shape; this test does not independently derive
    // the effective skill value displayed by the panel.
    let on_screen: BTreeSet<u32> = rows.iter().map(|(id, _, _)| *id).collect();
    for id in want.keys() {
        assert!(
            on_screen.contains(id),
            "skill {id:#04X} from the capture has no row"
        );
    }

    // The group each row landed in is the one the capture's own training grade selects.
    // `SkillGroup::of` is the mapping; the *inputs* are the message's.
    for (id, group, _) in &rows {
        let Some(sac) = want.get(id) else { continue };
        let expected = match sac {
            3 => SkillGroup::Specialized,
            2 => SkillGroup::Trained,
            // The untrained split needs the base record's minimum-level field, which is the dat's; the panel
            // is asserted against it below, so here only the two unambiguous grades are checked.
            _ => continue,
        };
        assert_eq!(
            *group, expected,
            "skill {id:#04X} with training grade {sac} landed in {group:?}"
        );
    }

    // The capture's specialised skills are exactly the rows in the first group.
    let want_spec: BTreeSet<u32> = want
        .iter()
        .filter(|(_, s)| **s == 3)
        .map(|(k, _)| *k)
        .collect();
    let got_spec: BTreeSet<u32> = rows
        .iter()
        .filter(|(_, g, _)| *g == SkillGroup::Specialized)
        .map(|(id, _, _)| *id)
        .collect();
    assert_eq!(
        got_spec, want_spec,
        "the Specialized group is not the capture's training-grade-3 set"
    );
    assert!(
        !want_spec.is_empty(),
        "the capture has no specialised skill to check against"
    );

    let want_trained: BTreeSet<u32> = want
        .iter()
        .filter(|(_, s)| **s == 2)
        .map(|(k, _)| *k)
        .collect();
    let got_trained: BTreeSet<u32> = rows
        .iter()
        .filter(|(_, g, _)| *g == SkillGroup::Trained)
        .map(|(id, _, _)| *id)
        .collect();
    assert_eq!(
        got_trained, want_trained,
        "the Trained group is not the capture's training-grade-2 set"
    );

    // ---- the rows carry real text and real icons ---------------------------------------------
    // The name and the icon come from the retail `SkillTable`, using the same id-to-base-record
    // lookup as the panel, so nothing about a name is written above.
    let skill_table = {
        use dereth_assets::Decode;
        let id = dereth_client::hud::SKILL_TABLE;
        let bytes = app.assets().read(id).expect("SkillTable 0x0E000004");
        dereth_assets::tables::SkillTable::decode_payload(id, &bytes).expect("SkillTable decodes")
    };
    let (ui, _) = gameplay_screen(&mut app).expect("the gameplay screen is up");
    let mut named = 0;
    for (id, _, h) in &rows {
        let (icon, label, value) = row_of(ui, *h);
        let base = skill_table
            .skills
            .get(id)
            .expect("every row is a SkillTable key");
        assert_eq!(
            &label, &base.name,
            "skill {id:#04X}'s label is not its SkillTable name"
        );
        assert!(!label.is_empty(), "skill {id:#04X} drew an empty label");
        if base.icon != 0 {
            assert_eq!(icon, Some(base.icon), "skill {id:#04X}'s icon");
        }
        assert!(
            value.parse::<i32>().is_ok(),
            "skill {id:#04X}'s value {value:?} is not a number"
        );
        named += 1;
    }
    assert_eq!(named, rows.len());

    // Inside a group the rows are in ascending name order, matching the wide-string comparison
    // used for sorted insertion.
    for g in SkillGroup::ALL {
        let names: Vec<String> = rows
            .iter()
            .filter(|(_, rg, _)| *rg == g)
            .map(|(_, _, h)| row_of(ui, *h).1)
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "{g:?} is not in name order");
    }
}

/// **The observed untrained skills follow the base record's minimum-level split.**
///
/// The rebuild puts untrained records with minimum level above one in group 4 and the
/// rest in group 3. The oracle is the retail `SkillTable` joined to the capture's training grades;
/// both are read here and neither is written.
///
/// Plain untrained skills must equal group 3, and every gated untrained skill must occur in group
/// 4; group 4 can also contain other unusable skills. The capture's character carries both kinds,
/// and that is asserted, so neither arm passes over nothing.
/// Dropping the `min_level <= 1` guard from `SkillGroup::of` merges the observed arms and fails
/// the applicable assertion.
#[test]
fn an_untrained_skill_the_data_gates_falls_into_the_fourth_group_on_screen() {
    have_dats();
    let (app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let desc = player_description(&events).expect("0x0013");
    let sacs: BTreeMap<u32, u32> = desc
        .qualities
        .skills
        .as_ref()
        .expect("skills")
        .entries
        .iter()
        .map(|(k, s)| (*k, s.sac))
        .collect();
    let skill_table = {
        use dereth_assets::Decode;
        let id = dereth_client::hud::SKILL_TABLE;
        let bytes = app.assets().read(id).expect("SkillTable");
        dereth_assets::tables::SkillTable::decode_payload(id, &bytes).expect("decodes")
    };

    let gated: BTreeSet<u32> = sacs
        .iter()
        .filter(|(id, sac)| {
            **sac == 1 && skill_table.skills.get(id).is_some_and(|b| b.min_level > 1)
        })
        .map(|(id, _)| *id)
        .collect();
    let plain: BTreeSet<u32> = sacs
        .iter()
        .filter(|(id, sac)| {
            **sac == 1 && skill_table.skills.get(id).is_some_and(|b| b.min_level <= 1)
        })
        .map(|(id, _)| *id)
        .collect();

    let rows: Vec<(u32, SkillGroup)> = app
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.group))
        .collect();
    let in_group = |g: SkillGroup| -> BTreeSet<u32> {
        rows.iter()
            .filter(|(_, rg)| *rg == g)
            .map(|(id, _)| *id)
            .collect()
    };

    assert!(
        !plain.is_empty(),
        "the capture's character has no untrained skill of minimum level <= 1"
    );
    assert!(
        !gated.is_empty(),
        "the capture's character has no untrained skill of minimum level > 1"
    );
    assert_eq!(
        in_group(SkillGroup::Untrained),
        plain,
        "the third group is minimum level <= 1"
    );
    assert!(
        in_group(SkillGroup::Unusable).is_superset(&gated),
        "a minimum-level-gated untrained skill did not reach the fourth group"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The spellbook.
// ---------------------------------------------------------------------------------------------

/// **The spellbook draws the character's own spells**, with the name and icon from the retail spell
/// table and in that table's display order.
///
/// The oracle is `fixtures/packet-captures/first-login-walk-jump`'s own `0x0013` spell-book entries joined to the
/// retail spell table. No spell id and no name is written here.
///
/// Deleting `SpellbookPanel::post_init`'s `ItemListWidget::init` call (no slots, so nothing
/// draws), removing `ItemSlot::set_spell`'s text-child write (the names go empty), or dropping
/// `spell_table` from `Hud::load_tables` each fails this test.
#[test]
fn the_spellbook_draws_the_captures_own_spells_with_their_names_and_icons() {
    have_dats();
    let (mut app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");

    let desc = player_description(&events).expect("0x0013");
    let book = desc
        .qualities
        .spell_book
        .as_ref()
        .expect("the capture's 0x0013 carries a book");
    let want: BTreeSet<u32> = book.entries.iter().map(|(k, _)| *k).collect();
    assert!(!want.is_empty(), "the capture's spellbook is empty");

    let spell_table = {
        use dereth_assets::Decode;
        let id = dereth_client::hud::SPELL_TABLE;
        let bytes = app.assets().read(id).expect("SpellTable 0x0E00000E");
        dereth_assets::tables::SpellTable::decode_payload(id, &bytes).expect("SpellTable decodes")
    };

    let shown = app.hud().panels.spellbook.shown.clone();
    assert!(!shown.is_empty(), "the spellbook drew nothing");

    // Every spell the message named, and nothing else. The capture's spell-filter mask is
    // `0x3FFF` — every school and every level — so nothing is filtered out here; the filter
    // itself is exercised in `the_filter_hides_a_school_and_the_list_follows`.
    let filters = desc.player_module.spell_filters;
    let known: BTreeSet<u32> = want
        .iter()
        .copied()
        .filter(|id| spell_table.spells.contains_key(id))
        .collect();
    assert_eq!(
        shown.iter().copied().collect::<BTreeSet<u32>>(),
        known,
        "the list is not the capture's spellbook (filters {filters:#X})"
    );

    // Sorted insertion orders by each spell base record's display-order field.
    let orders: Vec<i32> = shown
        .iter()
        .map(|id| spell_table.spells[id].display_order)
        .collect();
    let mut sorted = orders.clone();
    sorted.sort_unstable();
    assert_eq!(orders, sorted, "the spellbook is not in display order");

    // Each slot holds its spell, its icon and its name.
    let slots: Vec<(Option<u32>, Option<ElemHandle>, Option<ElemHandle>)> = app
        .hud()
        .panels
        .spellbook
        .list
        .as_ref()
        .expect("the spell list was never bound")
        .slots
        .iter()
        .map(|s| (s.spell, s.icon, s.text))
        .collect();
    let (ui, _) = gameplay_screen(&mut app).expect("screen");
    let mut checked = 0;
    for (spell, icon_h, text_h) in &slots {
        let Some(id) = spell else { continue };
        let base = spell_table
            .spells
            .get(id)
            .expect("a drawn spell is a table key");
        let name = text_h.map_or_else(String::new, |h| text_of(ui, h));
        assert_eq!(
            &name, &base.name,
            "spell {id}'s row text is not its SpellTable name"
        );
        let icon = icon_h
            .and_then(|h| ui.node(h))
            .and_then(|n| n.region.image.as_ref().map(|g| g.did.0));
        if base.icon != 0 {
            assert_eq!(icon, Some(base.icon), "spell {id}'s icon");
        }
        checked += 1;
    }
    assert_eq!(checked, shown.len(), "a shown spell had no slot");
}

/// Behaviour: panels.spellbook.the-filter-hides-a-school
///
/// **The school filter's sorting helper really filters.** Clearing one school's bit from a cloned
/// panel filter removes exactly that school's spells from the sorted HUD entries and leaves the
/// rest. This test does not click the live page or exercise its rebuild trigger.
///
/// The oracle is again the capture's own book joined to the table: which spells belong to which
/// school is the spell base record's school field, and the panel's bit map is
/// [`dereth_ui_screens::panels::spellbook::School::magic_school`].
///
/// Making `SpellbookPanel::is_filtered_out` return `false` unconditionally fails this test.
#[test]
fn the_filter_hides_a_school_and_the_list_follows() {
    use dereth_ui_screens::panels::spellbook::{School, SpellFilter, SpellbookPanel};
    have_dats();
    let (app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let desc = player_description(&events).expect("0x0013");

    // The panel's own entries, which came from the capture and the table.
    let book: Vec<dereth_ui_screens::view::SpellEntry> = app.hud().spells.clone();
    assert!(!book.is_empty(), "no spells to filter");

    let all = SpellFilter::from_player_module(desc.player_module.spell_filters);
    let with_all = SpellbookPanel::sorted(all, &book);
    assert_eq!(
        with_all.len(),
        book.len(),
        "the capture's own filters hide nothing"
    );

    // Turn off each school in turn; the list loses exactly that school's spells.
    for s in School::ALL {
        let mut f = all;
        f.toggle_school(s);
        let got: BTreeSet<u32> = SpellbookPanel::sorted(f, &book)
            .iter()
            .map(|e| e.id)
            .collect();
        let want: BTreeSet<u32> = book
            .iter()
            .filter(|e| e.school != s.magic_school())
            .map(|e| e.id)
            .collect();
        assert_eq!(got, want, "turning {s:?} off");
    }

    // And the round trip through the wire form is lossless, which is what makes the panel's filter
    // and the captured player-module filter the same value.
    assert_eq!(all.to_player_module(), desc.player_module.spell_filters);
}

// ---------------------------------------------------------------------------------------------
// 3. What the capture does not carry.
// ---------------------------------------------------------------------------------------------

/// **Only the `fellowship-*` recordings carry fellowship traffic; every other recording is an
/// asserted absence control.**
///
/// Every recording is replayed through the production `ClientNetwork` / `ObjectStream` path and
/// its fellowship messages counted: every `fellowship-*` recording carries `0x02BE
/// Fellowship_FullUpdate`, every other recording carries none of `0x02BE`, `0x01C9
/// FellowUpdateDone` and `0x01CA FellowStatsDone`, and no recording carries `0x01CA` at all. A new
/// recording outside the fellowship family that carries one fails here.
///
/// Allegiance is not asserted absent: the corpus carries `0x0020 Allegiance_AllegianceUpdate`,
/// and every recording that enters the world carries it. Its contents are asserted by
/// [`every_recorded_allegiance_update_decodes_and_its_member_count_agrees_with_its_roster`] below.
/// One message set is asserted absent and one present, so neither can pass by accident.
#[test]
fn only_the_fellowship_recordings_carry_fellowship_traffic() {
    use dereth_protocol::Opcode;
    const FELLOWSHIP: [Opcode; 3] = [
        Opcode::FELLOWSHIP_FULL_UPDATE,
        Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE,
        Opcode::FELLOWSHIP_FELLOW_STATS_DONE,
    ];

    // "The corpus carries no Y" is only as strong as the set it is read over, and the point of
    // asserting an absence is that a new capture turns it red, so every recording is read here.
    let mut fellowship_sessions = 0usize;
    let mut other_sessions = 0usize;
    let mut entered_sessions = 0usize;
    for session in corpus_sessions() {
        let events = replay(&session);
        let ops: BTreeSet<u32> = events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::UiEvent { opcode, .. } => Some(opcode.0),
                _ => None,
            })
            .collect();
        let counts: [usize; 3] = std::array::from_fn(|i| {
            events
                .iter()
                .filter(|e| matches!(e, SessionEvent::UiEvent { opcode, .. } if *opcode == FELLOWSHIP[i]))
                .count()
        });
        if session.starts_with("fellowship-") {
            fellowship_sessions += 1;
            assert!(
                counts[0] > 0,
                "{session}: a fellowship recording carries no 0x02BE Fellowship_FullUpdate"
            );
        } else {
            other_sessions += 1;
            assert_eq!(
                counts, [0; 3],
                "{session}: (0x02BE, 0x01C9, 0x01CA) -- a recording outside the `fellowship-*` \
                 family carries fellowship traffic"
            );
        }
        assert_eq!(counts[2], 0, "{session} carries 0x01CA FellowStatsDone");
        // `0x0003 Allegiance_AllegianceUpdateAborted` is a *different* message from `0x0020` and
        // is absent; it stays asserted here rather than moving to the allegiance test, so that
        // the failure of one does not mask the other.
        assert!(
            !ops.contains(&Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED.0),
            "{session} carries 0x0003 Allegiance_AllegianceUpdateAborted"
        );
        // The other direction of the same control: a recording whose character enters the world
        // carries the allegiance message this test does not claim is absent, and a recording that
        // never enters (login-only) carries no ordered event at all, so an empty histogram is never
        // why an absence above passed.
        let entered = events
            .iter()
            .any(|e| matches!(e, SessionEvent::PlayerDescription(_)));
        if entered {
            entered_sessions += 1;
            assert!(
                ops.contains(&Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE.0),
                "{session} enters the world and carries no 0x0020 Allegiance_AllegianceUpdate"
            );
        } else {
            assert!(
                ops.is_empty(),
                "{session} never enters the world yet carries ordered events {ops:?}"
            );
        }
    }
    assert!(
        fellowship_sessions > 0 && other_sessions > 0 && entered_sessions > 0,
        "the corpus has fellowship recordings ({fellowship_sessions}), other recordings \
         ({other_sessions}) and recordings that enter the world ({entered_sessions})"
    );
}

/// **Every recorded allegiance update decodes, and its member count agrees with its roster.**
///
/// Every recording is replayed and every delivered `0x0020 Allegiance_AllegianceUpdate` decoded.
/// An empty hierarchy reports zero members and zero vassals; a populated one reports a positive
/// member count; every update is at allegiance version 11. The corpus carries both kinds, and each
/// `fellowship-*` recording carries a populated one (the shard's three-member hierarchy, `Two`
/// over `One` over `Three`), so the allegiance page has version/empty-tree controls and populated
/// roster examples.
///
/// The arm that applies these is `dereth_client::interaction::apply_events`' `0x0020` case, and the
/// handler under it is the world allegiance-update handler.
#[test]
fn every_recorded_allegiance_update_decodes_and_its_member_count_agrees_with_its_roster() {
    use dereth_protocol::{Message, Opcode};

    let mut total = 0usize;
    let mut with_members = 0usize;
    let mut versions: BTreeSet<u32> = BTreeSet::new();
    let mut per: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for session in corpus_sessions() {
        let events = replay(&session);
        let entry = per.entry(session.clone()).or_default();
        for e in &events {
            let SessionEvent::UiEvent { opcode, blob } = e else {
                continue;
            };
            if *opcode != Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE {
                continue;
            }
            let mut r = dereth_protocol::archive::Reader::new(blob.get(4..).unwrap_or_default());
            let m = dereth_protocol::social::AllegianceUpdate::read(&mut r)
                .expect("every `0x0020` in the corpus must decode");
            total += 1;
            entry.0 += 1;
            versions.insert(m.profile.hierarchy.version);
            if m.profile.hierarchy.members.is_empty() {
                assert_eq!(
                    m.profile.total_members, 0,
                    "{session}: total_members of an empty roster"
                );
                assert_eq!(
                    m.profile.total_vassals, 0,
                    "{session}: total_vassals of an empty roster"
                );
            } else {
                with_members += 1;
                entry.1 += 1;
                assert!(
                    m.profile.total_members > 0,
                    "{session}: a roster with a member count"
                );
            }
        }
    }
    assert!(
        total > with_members && with_members > 0,
        "the corpus carries both empty and populated allegiance updates: {total} `0x0020`, \
         {with_members} with a roster; by session {per:?}"
    );
    for (session, (_, rostered)) in per.iter().filter(|(s, _)| s.starts_with("fellowship-")) {
        assert!(
            *rostered > 0,
            "{session}: a fellowship recording carries no populated allegiance update"
        );
    }
    assert_eq!(
        versions,
        BTreeSet::from([11]),
        "all at allegiance version 11"
    );
}
// ---------------------------------------------------------------------------------------------
// 4. It draws, and only where it said it would.
// ---------------------------------------------------------------------------------------------

/// Two independently constructed applications receive the same captured description and differ
/// only in whether the spellbook list is flushed before capture. Every changed pixel must be inside
/// the spell list's own rectangle, with zero outside.
///
/// The spell list is used rather than the skills list because the spellbook is the spell page's
/// first sub-panel and comes up with the page; the skills list sits behind the attribute page on the
/// character page's tab table (the next test drives that tab).
///
/// Removing `set_spells` gives a zero differential; writing icons outside the list gives a
/// nonzero outside count.
#[test]
fn the_filled_spellbook_changes_pixels_and_only_inside_the_spell_lists_box() {
    have_dats();
    let events = replay("first-login-walk-jump");

    let shot = |flush: bool| -> Option<(u32, u32, Vec<u8>, dereth_ui::Box2D, usize)> {
        let mut app = app_in_gameplay(4)?;
        let _ = app.apply_hud_events(events_in_world(&events));
        {
            let (ui, screen) = gameplay_screen(&mut app)?;
            let page = screen
                .panels
                .pages
                .iter()
                .find(|p| p.element == dereth_ui_screens::panels::remaining::SPELL_PAGE)
                .copied()?;
            screen.recv_set_panel_visibility(ui, page.panel_id, true);
        }
        for _ in 0..6 {
            app.frame();
        }
        let (rect, filled) = {
            let w = app.hud().panels.spellbook.list.as_ref()?;
            let handle = w.handle;
            let filled = w.slots.iter().filter(|s| s.spell.is_some()).count();
            let (ui, _) = gameplay_screen(&mut app)?;
            (ui.screen_box(handle), filled)
        };
        if flush {
            // Flushing returns every slot to empty state `0x1000001C`.
            // `SpellbookPanel::update` guards on its own snapshot,
            // which has not changed, so it does not refill behind the flush.
            let mut panels = std::mem::take(&mut app.hud_mut().panels);
            {
                let (ui, _) = gameplay_screen(&mut app)?;
                panels.spellbook.list.as_mut()?.flush(ui);
            }
            app.hud_mut().panels = panels;
        }
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, rect, filled))
    };

    let (w, h, full, rect, filled) =
        shot(false).expect("a rendered frame: retail dats and a software GPU device");
    let (w2, h2, empty, _, _) =
        shot(true).expect("a rendered frame: retail dats and a software GPU device");
    assert_eq!((w, h), (w2, h2));
    assert!(filled > 0, "the capture's spells never reached the list");
    assert!(
        rect.width() > 0 && rect.height() > 0,
        "the spell list has no rectangle: {rect:?}"
    );

    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if empty[i..i + 4] == full[i..i + 4] {
                continue;
            }
            if x >= rect.x0 && x <= rect.x1 && y >= rect.y0 && y <= rect.y1 {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(inside > 0, "filling the spellbook changed no pixels at all");
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the spell list's box {rect:?}"
    );
}

/// The same two-application differential for the **skills page**, which needs one more step: the
/// skill page is not the character page's default tab, so the tab that maps to it receives a direct
/// click-message broadcast first.
///
/// The tab is not named here — it is read from the live panel's `tab_to_page` table, the
/// table built from attribute `0x2E`, by looking for the entry whose
/// page is the skill page: the panel's tab mechanism, driven rather than re-implemented.
///
/// Deleting `panels.update(...)` from the HUD update path leaves the list empty and the
/// differential at zero pixels.
///
/// **Not** falsified by removing `write_row`: a row element carries
/// its own frame from the `0x21000045` template, so creating forty-two blank rows already changes
/// pixels. This test therefore proves *that the list is populated and draws inside its own
/// rectangle*, and the row **contents** are proved by
/// [`the_skills_page_draws_the_captures_own_skills_with_their_names_and_values`], which fails
/// when `write_row` goes.
#[test]
fn the_filled_skills_page_changes_pixels_and_only_inside_the_list_boxs_box() {
    have_dats();
    let events = replay("first-login-walk-jump");

    let shot = |flush: bool| -> Option<(u32, u32, Vec<u8>, dereth_ui::Box2D, usize)> {
        let mut app = app_in_gameplay(4)?;
        let _ = app.apply_hud_events(events_in_world(&events));
        // Open the character page through the panel stack, the way the toolbar button does.
        let page = dereth_ui_screens::panels::remaining::CHARACTER_PAGE;
        {
            let (ui, screen) = gameplay_screen(&mut app)?;
            let panel_id = screen
                .panels
                .pages
                .iter()
                .find(|p| p.element == page)
                .map(|p| p.panel_id)?;
            screen.recv_set_panel_visibility(ui, panel_id, true);
        }
        app.frame();
        // Broadcast a click to the tab whose page is the skill page, read from the live tab table.
        {
            let (ui, screen) = gameplay_screen(&mut app)?;
            let root = screen.root()?;
            let h = ui.get_child_recursive(root, page)?;
            let tab = {
                let n = ui.node(h)?;
                let b = n.behaviour.as_ref()?;
                let p = (**b)
                    .as_any()
                    .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())?;
                p.tab_to_page
                    .iter()
                    .find(|(_, page)| **page == dereth_ui_screens::panels::skills::PANEL)
                    .map(|(t, _)| *t)?
            };
            let th = ui.get_child_recursive(root, tab)?;
            ui.broadcast_element_message(th, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
        }
        for _ in 0..5 {
            app.frame();
        }
        let (rect, rows) = {
            let w = app.hud().panels.skills.list.as_ref()?;
            let handle = w.handle;
            let rows = app.hud().panels.skills.rows.len();
            let (ui, _) = gameplay_screen(&mut app)?;
            (ui.screen_box(handle), rows)
        };
        if flush {
            // Flushing deletes every skill row. `SkillsPanel::update` guards on its own
            // snapshot, which has not changed, so it does not rebuild behind the flush.
            let mut panels = std::mem::take(&mut app.hud_mut().panels);
            {
                let (ui, _) = gameplay_screen(&mut app)?;
                panels.skills.list.as_mut()?.flush(ui);
            }
            app.hud_mut().panels = panels;
        }
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, rect, rows))
    };

    let (w, h, full, rect, rows) =
        shot(false).expect("a rendered frame: retail dats and a software GPU device");
    let (w2, h2, empty, _, _) =
        shot(true).expect("a rendered frame: retail dats and a software GPU device");
    assert_eq!((w, h), (w2, h2));
    assert!(rows > 0, "the capture's skills never reached the list");
    assert!(
        rect.width() > 0 && rect.height() > 0,
        "the skills list has no rectangle: {rect:?}"
    );

    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if empty[i..i + 4] == full[i..i + 4] {
                continue;
            }
            if x >= rect.x0 && x <= rect.x1 && y >= rect.y0 && y <= rect.y1 {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(
        inside > 0,
        "filling the skills page changed no pixels at all"
    );
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the skills list's box {rect:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 5. The attributes page — the character panel's default tab.
// ---------------------------------------------------------------------------------------------

/// **The attributes page draws its nine rows with the character's own numbers.**
///
/// The evidence is split by risk: *that* the page draws nine labelled rows instead of an empty box
/// is the cheap tier — the frame is the test and the row
/// count is asserted here rather than proved by a deletion harness. **The numbers are not.** An
/// attributes page with the wrong Strength in it looks perfectly plausible, so every value is
/// checked against `fixtures/packet-captures/first-login-walk-jump`'s own `0x0013` primary and vital tables —
/// `init_level + level_from_cp` per primary and `current_level` per vital, read out of the message
/// and never written here.
///
/// That split is deliberate: `GameView::attribute` goes through
/// `dereth_client_model::attributes::inq_attribute`, which adds the enchantment stack on top of the stored
/// ranks. The capture's character carries no attribute enchantment, which the test asserts rather
/// than assumes — if a later capture does, this fails loudly instead of silently comparing the
/// wrong two numbers.
#[test]
fn the_attributes_page_draws_the_captures_own_attributes_and_vitals() {
    have_dats();
    let (app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let desc = player_description(&events).expect("0x0013");
    let cache = desc
        .qualities
        .attribute_cache
        .as_ref()
        .expect("the capture carries attributes");

    // ---- the cheap half: the page is not empty chrome ---------------------------------------
    let shown = app.hud().panels.attributes.shown();
    assert_eq!(
        shown.len(),
        9,
        "attribute-panel initialization builds six attribute rows and three vital rows, got {shown:?}"
    );
    let stats: Vec<u32> = shown.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        stats,
        vec![1, 2, 4, 3, 5, 6, 2, 4, 6],
        "the attribute-panel initialization order"
    );
    assert!(
        shown.iter().all(|(_, v)| !v.is_empty() && v != "???"),
        "a row drew no value at all: {shown:?}"
    );

    // ---- the expensive half: every number, against the capture -------------------------------
    // The effective attribute query is `init_level + level_from_cp` plus the enchantment stack.
    // The capture has no attribute enchantment, so the two must agree exactly; assert that premise.
    let ench = desc
        .qualities
        .enchantments
        .as_ref()
        .map(|e| {
            e.multiplicative.as_ref().map_or(0, Vec::len) + e.additive.as_ref().map_or(0, Vec::len)
        })
        .unwrap_or(0);
    assert_eq!(
        ench, 0,
        "the capture now carries enchantments; this oracle needs the stack too"
    );

    let want = |id: u32| -> Option<u32> {
        let a = match id {
            1 => cache.strength,
            2 => cache.endurance,
            3 => cache.quickness,
            4 => cache.coordination,
            5 => cache.focus,
            6 => cache.self_,
            _ => None,
        }?;
        Some(a.init_level.wrapping_add(a.level_from_cp))
    };
    let mut checked = 0;
    for (stat, value) in shown.iter().take(6) {
        let expect = want(*stat).expect("the capture names all six attributes");
        assert_eq!(
            value,
            &expect.to_string(),
            "attribute {stat} drew {value:?}, the capture says {expect}"
        );
        assert!(
            expect > 0,
            "attribute {stat} is zero in the capture, which proves nothing"
        );
        checked += 1;
    }
    assert_eq!(checked, 6);

    // The three vitals render `"%d/%d"` and their *current* half is the capture's own
    // `current_level`. The maximum is `Attribute2ndTable`'s formula, which `tests/gpu/panels/gameplay_hud.rs` already
    // checks against this same message for the vitals bar; here only the shape and the current
    // value are re-asserted, so the two tests do not restate one another.
    for (i, (stat, value)) in shown.iter().skip(6).enumerate() {
        let s = match stat {
            2 => cache.health,
            4 => cache.stamina,
            6 => cache.mana,
            _ => None,
        }
        .expect("the capture names all three vitals");
        let (cur, max) = value.split_once('/').unwrap_or_else(|| {
            panic!("vital row {i} drew {value:?}, which is not the \"%d/%d\" shape")
        });
        assert_eq!(
            cur,
            s.current_level.to_string(),
            "vital {stat}'s current value"
        );
        assert!(
            max.parse::<u32>().is_ok_and(|m| m > 0),
            "vital {stat}'s maximum {max:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 6. The order the two halves of a login arrive in.
// ---------------------------------------------------------------------------------------------

/// **The three panels above are joined once, on the `0x0013`, so the description has to answer
/// at that moment -- row or no row.**
///
/// The player-description install path puts the login qualities onto the player's row and
/// **returns silently when there is no row yet**, which is the order the shard actually sends in:
/// ACE enqueues the player-description event before the create-object message, and the two travel
/// on different queues (`long-solo-play` carries them at
/// idx 9 on queue 9 and idx 22 on queue 10). `Hud::rebuild_panel_tables` runs on the message and
/// on nothing else, so a `Hud::player_desc` that could only read the row would answer `None` at
/// the one moment it is ever asked and the skills page, the spellbook and the attribute page
/// would stay empty for the whole session; it falls back to the parked login description.
///
/// This is a direct model-level statement of the ordering exercised through the screen above: it
/// needs no dats and no device, and it names the ordering instead of relying on a harness that
/// happens not to build a weenie.
///
/// **Falsified by** deleting the `.or_else(|| world.login_player_desc())` from `Hud::player_desc`
/// -- the first `expect` below fires.
///
/// **No datagram leaves this process.** The capture is replayed from disk into the client's own
/// model and every assertion is against that model.
#[test]
fn the_login_description_answers_before_the_players_own_row_exists() {
    let events = replay("first-login-walk-jump");
    let desc = player_description(&events).expect("0x0013");
    let want: BTreeMap<u32, u32> = desc
        .qualities
        .skills
        .as_ref()
        .expect("the capture's 0x0013 carries skills")
        .entries
        .iter()
        .map(|(k, s)| (*k, s.sac))
        .collect();
    let (id, sac) = want
        .iter()
        .map(|(k, v)| (*k, *v))
        .next()
        .expect("the capture names at least one skill");

    // No `0xF746`/`0xF745` is replayed here **on purpose**: this is exactly the window the park
    // exists for, and asserting the premise is what stops the test passing for the wrong reason.
    let mut hud = dereth_client::hud::Hud::default();
    let mut world = dereth_client_model::World::new();
    hud.apply_events(events_in_world(&events), &mut world);
    assert!(
        world.player_qualities().is_none(),
        "the premise: no weenie row, so the description has nowhere to have been installed"
    );

    let parked = hud
        .player_desc(&world)
        .expect("the parked description answers the join");
    assert_eq!(
        parked.skill(id).map(|s| s.sac),
        Some(sac),
        "skill {id:#04X}: the answer is the capture's own skill-stat entry"
    );

    // Manually create the player's row and adopt it. This verifies that the same description then
    // answers from the row. The test does not drive a later stat-update message.
    let player = world
        .player
        .or(hud.player)
        .expect("`0xF746` named the player");
    world.player = None;
    world
        .tables
        .weenies
        .insert(player, dereth_client_model::weenie::Weenie::new(player));
    assert!(world.set_player(player), "the identity is adopted once");
    let installed = world
        .player_qualities()
        .expect("`set_player` replayed the park onto the row");
    assert_eq!(installed.skill(id).map(|s| s.sac), Some(sac));
    assert_eq!(
        hud.player_desc(&world)
            .and_then(|q| q.skill(id))
            .map(|s| s.sac),
        Some(sac)
    );
}
