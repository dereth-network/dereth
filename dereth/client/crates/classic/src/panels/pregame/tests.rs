//! Behaviour: none (classic front-end adapter; no retail behaviour claim).
use super::*;
#[derive(Debug)]
pub(super) struct EmptyView;
impl GameView for EmptyView {}
pub(super) fn context(f: impl FnOnce(&Context<'_>)) {
    let game = EmptyView;
    let pregame = dereth_client_contract::pregame::PregameView::default();
    let keyboard = KeyboardState::default();
    let settings = ClassicSettings::default();
    let classic = ClassicState::default();
    f(&Context {
        game: &game,
        pregame: &pregame,
        keyboard: &keyboard,
        settings: &settings,
        map_teleport_allowed: false,
        classic: &classic,
    });
}
pub(super) fn tables() -> dereth_chargen::CreationTables {
    use dereth_assets::tables::*;
    use std::collections::BTreeMap;
    use std::rc::Rc;
    let desc = ObjDesc {
        version: 0x11,
        palette: None,
        subpalettes: vec![],
        texture_changes: vec![],
        anim_part_changes: vec![],
    };
    let clothes = vec![
        GearItem {
            name: "First".into(),
            clothing_table: DataId(0x10000001),
            weenie_default: 0,
        },
        GearItem {
            name: "Last".into(),
            clothing_table: DataId(0x10000002),
            weenie_default: 0,
        },
    ];
    let sex = SexCg {
        naming_help: None,
        name: "Female".into(),
        scale: 100,
        setup: DataId(0x2000001),
        sound_table: DataId(0),
        icon: 0,
        base_palette: DataId(0),
        skin_palset: DataId(0),
        physics_table: DataId(0),
        motion_table: DataId(0),
        combat_table: DataId(0),
        base_objdesc: desc.clone(),
        hair_colors: vec![],
        hair_styles: vec![],
        eye_colors: vec![],
        eye_strips: vec![],
        nose_strips: vec![],
        mouth_strips: vec![],
        headgear: clothes.clone(),
        shirts: clothes,
        pants: vec![],
        footwear: vec![],
        clothing_colors: vec![1, 2, 3, 4, 5, 6],
    };
    let heritage = HeritageGroup {
        description: None,
        name: "Aluvian".into(),
        icon: 0,
        setup: DataId(0),
        environment_setup: DataId(0),
        attribute_credits: 330,
        skill_credits: 100,
        primary_start_areas: vec![0, 1],
        secondary_start_areas: vec![],
        skills: vec![],
        templates: vec![CharGenTemplate {
            name: "Adventurer".into(),
            icon: 0,
            title: 0,
            attributes: [50; 6],
            normal_skills: vec![],
            primary_skills: vec![],
        }],
        sex_table_marker: 0,
        sex_order: vec![2],
        sexes: BTreeMap::from([(2, sex)]),
        template_presentations: BTreeMap::new(),
    };
    let skills = BTreeMap::from([
        (1, ("Unused", 0, 0, 0)),
        (7, ("First skill", 1, 4, 8)),
        (8, ("Second skill", 1, 6, 12)),
    ])
    .into_iter()
    .map(
        |(id, (name, chargen_use, trained_cost, specialized_cost))| {
            (
                id,
                SkillBase {
                    description: String::new(),
                    name: name.into(),
                    icon: 0,
                    trained_cost,
                    specialized_cost,
                    category: 0,
                    chargen_use,
                    min_level: 1,
                    formula: SkillFormula {
                        w: 0,
                        x: 1,
                        y: 1,
                        z: 4,
                        attr1: 1,
                        attr2: 4,
                    },
                    upper_bound: 0.0,
                    lower_bound: 0.0,
                    learn_mod: 0.0,
                },
            )
        },
    )
    .collect();
    let clothing = (1..=2)
        .map(|n| {
            let id = DataId(0x10000000 + n);
            (
                id,
                dereth_assets::motion::ClothingTable {
                    id,
                    clothing_base_buckets: 0,
                    clothing_bases: BTreeMap::new(),
                    palette_template_buckets: 1,
                    palette_templates: if n == 1 {
                        (1..=6)
                            .map(|i| {
                                (
                                    i,
                                    dereth_assets::motion::PaletteTemplate {
                                        icon: DataId(0),
                                        subpalette_effects: vec![
                                            dereth_assets::motion::PaletteEffect {
                                                ranges: vec![],
                                                palette_set: DataId(0),
                                            },
                                        ],
                                    },
                                )
                            })
                            .collect()
                    } else {
                        BTreeMap::new()
                    },
                },
            )
        })
        .collect();
    dereth_chargen::CreationTables {
        chargen: CharGen {
            help_strings: vec![],
            id: DataId(0),
            second_data_id: DataId(0),
            starter_areas: vec![
                StarterArea {
                    name: "North".into(),
                    locations: vec![],
                },
                StarterArea {
                    name: "South".into(),
                    locations: vec![],
                },
            ],
            hg_table_marker: 0,
            heritage_order: vec![1],
            heritage_groups: BTreeMap::from([(1, heritage)]),
        },
        skills: SkillTable {
            id: DataId(0),
            buckets: 0,
            skills,
        },
        clothing: Rc::new(clothing),
    }
}
pub(super) fn data() -> CreationData {
    CreationData::from_tables(std::rc::Rc::new(tables()))
}
#[test]
fn choosing_heritage_and_sex_enables_normal_next_navigation() {
    context(|c| {
        let mut p = Pregame::new("heritage", Ok(std::rc::Rc::new(data())));
        assert!(
            !p.frame(c)
                .controls
                .iter()
                .find(|v| v.id == "next")
                .unwrap()
                .enabled
        );
        p.event(
            ControlEvent::Select {
                id: "heritage".into(),
                index: 0,
            },
            c,
        );
        assert!(
            p.frame(c)
                .controls
                .iter()
                .find(|v| v.id == "next")
                .unwrap()
                .enabled
        );
        p.event(ControlEvent::Activate("next".into()), c);
        assert_eq!(p.page, "sex");
        p.event(
            ControlEvent::Select {
                id: "sex".into(),
                index: 0,
            },
            c,
        );
        assert!(
            p.frame(c)
                .controls
                .iter()
                .find(|v| v.id == "next")
                .unwrap()
                .enabled
        );
        p.event(ControlEvent::Activate("next".into()), c);
        assert_eq!(p.page, "appearance");
    });
}
#[test]
fn headgear_choice_includes_none_and_the_last_style_and_clamps_colour() {
    context(|c| {
        let mut p = Pregame::new("clothing", Ok(std::rc::Rc::new(data())));
        p.state.headgear_color = 99;
        p.event(
            ControlEvent::Select {
                id: "style-0".into(),
                index: 1,
            },
            c,
        );
        assert_eq!(p.state.headgear_style, 0);
        assert_eq!(p.state.headgear_color, 5);
        p.event(
            ControlEvent::Select {
                id: "style-0".into(),
                index: 2,
            },
            c,
        );
        assert_eq!(p.state.headgear_style, 1);
        p.event(
            ControlEvent::Select {
                id: "style-0".into(),
                index: 0,
            },
            c,
        );
        assert_eq!(p.state.headgear_style, -1);
        p.event(
            ControlEvent::Select {
                id: "style-1".into(),
                index: 1,
            },
            c,
        );
        assert_eq!(p.state.shirt_style, 1);
    });
}
#[test]
fn attribute_slider_and_numeric_editor_have_distinct_routing_ids() {
    context(|c| {
        let mut p = Pregame::new("attributes", Ok(std::rc::Rc::new(data())));
        let f = p.frame(c);
        let mut ids = std::collections::BTreeSet::new();
        for control in &f.controls {
            assert!(ids.insert(&control.id), "duplicate {}", control.id);
        }
        assert!(ids.contains(&"attr-0".to_owned()));
        assert!(ids.contains(&"attr-value-0".to_owned()));
        p.event(
            ControlEvent::Value {
                id: "attr-0".into(),
                value: 60,
            },
            c,
        );
        assert_eq!(p.state.get(Attr::Strength), 60);
        p.event(
            ControlEvent::Edit {
                id: "attr-value-0".into(),
                text: "75".into(),
            },
            c,
        );
        assert_eq!(p.state.get(Attr::Strength), 75);
    });
}
#[test]
fn selected_skill_uses_original_table_index_across_hidden_rows_and_regrouping() {
    context(|c| {
        let mut p = Pregame::new("skills", Ok(std::rc::Rc::new(data())));
        let row =
            presentation::skill_rows(p.data.as_ref().unwrap(), &p.view(p.data.as_ref().unwrap()))
                .iter()
                .position(|r| r.skill == Some(1))
                .unwrap();
        p.event(
            ControlEvent::Select {
                id: "skills".into(),
                index: row,
            },
            c,
        );
        p.event(ControlEvent::Activate("train".into()), c);
        assert_eq!(p.state.skill_levels.get(7).map(|v| *v as i32), Some(2));
        assert_eq!(p.state.skill_levels.get(8).map(|v| *v as i32), Some(1));
        assert_eq!(p.selected_skill, Some(1));
        p.event(ControlEvent::Activate("specialize".into()), c);
        assert_eq!(p.state.skill_levels.get(7).map(|v| *v as i32), Some(3));
    });
}

#[test]
fn deletion_waits_only_after_confirmation_and_releases_on_character_set_notice() {
    context(|base| {
        use dereth_client_contract::persist::{CharacterIdentity, CharacterSet};
        let mut view = base.pregame.clone();
        view.character_set = Some(CharacterSet {
            set: vec![CharacterIdentity {
                id: ObjectId(42),
                name: "Test Character".into(),
                seconds_grace_period: 0,
            }],
            ..Default::default()
        });
        let mut c = Context {
            game: base.game,
            pregame: &view,
            keyboard: base.keyboard,
            settings: base.settings,
            map_teleport_allowed: base.map_teleport_allowed,
            classic: base.classic,
        };
        let mut p = Pregame::new("login", Ok(std::rc::Rc::new(data())));
        p.selected = Some(0);
        p.delete_name = "Test Character".into();
        let actions = p.event(ControlEvent::Activate("delete".into()), &c);
        assert!(!p.waiting);
        let [PanelAction::Confirm { accept, .. }] = actions.as_slice() else {
            panic!("confirmation")
        };
        let [PanelAction::Control(confirm)] = accept.as_slice() else {
            panic!("confirmation callback")
        };
        let actions = p.event(confirm.clone(), &c);
        assert!(matches!(
            actions.as_slice(),
            [PanelAction::Game(UiRequest::CharacterAction(
                CharacterAction::Delete(ObjectId(42))
            ))]
        ));
        assert!(p.waiting);
        assert_eq!(p.delete_name, super::DELETE_PLACEHOLDER);
        assert!(p
            .event(ControlEvent::Activate("enter".into()), &c)
            .is_empty());
        let mut next = view.clone();
        next.character_set_notices += 1;
        c.pregame = &next;
        p.event(ControlEvent::Tick, &c);
        assert!(!p.waiting);
    });
}

#[test]
fn more_characters_than_the_six_slots_scroll_in_the_six() {
    context(|base| {
        use dereth_client_contract::persist::{CharacterIdentity, CharacterSet};
        let mut view = base.pregame.clone();
        let names = ["Ann", "Bea", "Cid", "Dot", "Eve", "Fay", "Gus", "Hal"];
        view.character_set = Some(CharacterSet {
            set: names
                .iter()
                .zip(1..)
                .map(|(name, id)| CharacterIdentity {
                    id: ObjectId(id),
                    name: (*name).into(),
                    seconds_grace_period: 0,
                })
                .collect(),
            num_allowed_characters: 8,
            ..Default::default()
        });
        let c = Context {
            game: base.game,
            pregame: &view,
            keyboard: base.keyboard,
            settings: base.settings,
            map_teleport_allowed: base.map_teleport_allowed,
            classic: base.classic,
        };
        let mut p = Pregame::new("login", Ok(std::rc::Rc::new(data())));
        let shown = |p: &Pregame| {
            let f = p.frame(&c);
            let list = f
                .controls
                .iter()
                .find(|k| k.id == "characters")
                .map(|k| k.rect);
            let names: Vec<String> = f
                .screen
                .commands
                .iter()
                .filter_map(|cmd| match cmd {
                    Command::Text { text, y, .. } if (136..232).contains(y) => Some(text.clone()),
                    _ => None,
                })
                .filter(|t| names.contains(&t.as_str()))
                .collect();
            let scroll = f.controls.iter().any(|k| k.id == "characters-scroll");
            (list.map(|r| r.h), names, scroll)
        };
        let (h, first, scroll) = shown(&p);
        assert_eq!(h, Some(96), "six slots, every one the art frames");
        assert_eq!(first, ["Ann", "Bea", "Cid", "Dot", "Eve", "Fay"]);
        assert!(scroll);
        // The bar is the message box's width, and nothing is drawn over it.
        let f = p.frame(&c);
        let rect_of = |id: &str| f.controls.iter().find(|k| k.id == id).map(|k| k.rect);
        let bar = rect_of("characters-scroll").unwrap();
        let enter = rect_of("enter").unwrap();
        assert_eq!(bar.w, rect_of("message-scroll").map_or(20, |r| r.w));
        assert!(bar.x + bar.w <= enter.x, "{bar:?} under {enter:?}");
        p.event(
            ControlEvent::Scroll {
                id: "characters-scroll".into(),
                value: 32,
            },
            &c,
        );
        assert_eq!(shown(&p).1, ["Cid", "Dot", "Eve", "Fay", "Gus", "Hal"]);
    });
}

#[test]
fn character_list_refresh_keeps_wire_slot_when_display_sort_order_changes() {
    context(|base| {
        use dereth_client_contract::persist::{CharacterIdentity, CharacterSet};
        let mut view = base.pregame.clone();
        let char_at = |id, name: &str| CharacterIdentity {
            id: ObjectId(id),
            name: name.into(),
            seconds_grace_period: 0,
        };
        view.character_set = Some(CharacterSet {
            set: vec![char_at(1, "Zed"), char_at(2, "Amy")],
            ..Default::default()
        });
        let mut c = Context {
            game: base.game,
            pregame: &view,
            keyboard: base.keyboard,
            settings: base.settings,
            map_teleport_allowed: base.map_teleport_allowed,
            classic: base.classic,
        };
        let mut p = Pregame::new("login", Ok(std::rc::Rc::new(data())));
        p.event(
            ControlEvent::Select {
                id: "characters".into(),
                index: 1,
            },
            &c,
        );
        assert_eq!(p.selected_slot, Some(0));
        let mut next = view.clone();
        next.character_set.as_mut().unwrap().set[1].name = "Zoe".into();
        next.character_set_notices += 1;
        c.pregame = &next;
        p.event(ControlEvent::Tick, &c);
        assert_eq!(p.selected, Some(0));
        assert_eq!(p.selected_slot, Some(0));
        let actions = p.event(ControlEvent::Activate("enter".into()), &c);
        assert!(matches!(
            actions.as_slice(),
            [PanelAction::Game(UiRequest::CharacterAction(
                CharacterAction::LogOn(ObjectId(1))
            ))]
        ));
    });
}

#[test]
fn successful_creation_enters_from_verification_identity_without_new_list_notice() {
    context(|base| {
        use dereth_client_contract::persist::{CharacterIdentity, CharacterSet};
        for delayed_identity in [false, true] {
            let mut view = base.pregame.clone();
            view.character_set = Some(CharacterSet::default());
            view.chargen_response = Some(1);
            view.chargen_response_notices = 1;
            let identity = CharacterIdentity {
                id: ObjectId(42),
                name: "New Character".into(),
                seconds_grace_period: 0,
            };
            if !delayed_identity {
                view.character_set
                    .as_mut()
                    .unwrap()
                    .set
                    .push(identity.clone());
            }
            let mut c = Context {
                game: base.game,
                pregame: &view,
                keyboard: base.keyboard,
                settings: base.settings,
                map_teleport_allowed: base.map_teleport_allowed,
                classic: base.classic,
            };
            let mut p = Pregame::new("name-summary", Ok(std::rc::Rc::new(data())));
            p.created_name = Some("New Character".into());
            p.waiting = true;
            let mut actions = p.event(ControlEvent::Tick, &c);
            assert!(p.waiting);
            let mut updated = view.clone();
            if delayed_identity {
                assert!(actions.is_empty());
                updated.character_set.as_mut().unwrap().set.push(identity);
                c.pregame = &updated;
                actions = p.event(ControlEvent::Tick, &c);
            }
            assert_eq!(c.pregame.character_set_notices, 0);
            assert!(matches!(
                actions.as_slice(),
                [PanelAction::Game(UiRequest::CharGenAction(
                    CharGenAction::LogOn(ObjectId(42))
                ))]
            ));
            assert!(p.created_name.is_none());
            assert!(p.event(ControlEvent::Tick, &c).is_empty());
        }
    });
}

#[test]
fn successful_creation_enters_when_the_server_prefixes_a_privileged_name() {
    context(|base| {
        use dereth_client_contract::persist::{CharacterIdentity, CharacterSet};
        let mut view = base.pregame.clone();
        view.character_set = Some(CharacterSet {
            set: vec![CharacterIdentity {
                id: ObjectId(43),
                name: "+New Character".into(),
                seconds_grace_period: 0,
            }],
            ..Default::default()
        });
        view.chargen_response = Some(1);
        view.chargen_response_notices = 1;
        let c = Context {
            game: base.game,
            pregame: &view,
            keyboard: base.keyboard,
            settings: base.settings,
            map_teleport_allowed: base.map_teleport_allowed,
            classic: base.classic,
        };
        let mut p = Pregame::new("name-summary", Ok(std::rc::Rc::new(data())));
        p.created_name = Some("New Character".into());
        p.waiting = true;
        assert!(matches!(
            p.event(ControlEvent::Tick, &c).as_slice(),
            [PanelAction::Game(UiRequest::CharGenAction(
                CharGenAction::LogOn(ObjectId(43))
            ))]
        ));
    });
}

#[test]
fn successful_restore_clears_waiting_without_creation_status_or_logon() {
    context(|base| {
        let mut view = base.pregame.clone();
        view.chargen_response = Some(1);
        view.chargen_response_notices = 1;
        let c = Context {
            game: base.game,
            pregame: &view,
            keyboard: base.keyboard,
            settings: base.settings,
            map_teleport_allowed: base.map_teleport_allowed,
            classic: base.classic,
        };
        let mut p = Pregame::new("login", Ok(std::rc::Rc::new(data())));
        p.waiting = true;
        assert!(p.event(ControlEvent::Tick, &c).is_empty());
        assert!(!p.waiting);
        assert!(p.status.is_empty());
    });
}

#[test]
fn confirmed_login_quit_requests_process_exit_without_dropping_only_the_panel() {
    context(|c| {
        let mut p = Pregame::new("login", Ok(std::rc::Rc::new(data())));
        let actions = p.event(ControlEvent::Activate("quit".into()), c);
        let [PanelAction::Confirm { accept, .. }] = actions.as_slice() else {
            panic!("quit must confirm first")
        };
        assert_eq!(accept, &[PanelAction::Host(HostAction::Quit)]);
        assert_eq!(p.page, "login");
    });
}

#[test]
fn startup_cancel_exits_immediately_without_creation_abandon_prompt() {
    context(|c| {
        let mut p = Pregame::new("startup", Ok(std::rc::Rc::new(data())));
        assert_eq!(
            p.event(ControlEvent::Activate("cancel".into()), c),
            vec![PanelAction::Host(HostAction::Quit)]
        );
    });
}
#[test]
fn enter_confirmation_initializes_the_current_character_selection() {
    use dereth_client_contract::persist::{CharacterIdentity, CharacterSet};
    context(|base| {
        let mut view = base.pregame.clone();
        view.character_set = Some(CharacterSet {
            set: vec![CharacterIdentity {
                id: ObjectId(17),
                name: "Probe".into(),
                seconds_grace_period: 0,
            }],
            ..Default::default()
        });
        let c = Context {
            game: base.game,
            pregame: &view,
            keyboard: base.keyboard,
            settings: base.settings,
            map_teleport_allowed: false,
            classic: base.classic,
        };
        let mut p = Pregame::new("enter-confirmation", Ok(std::rc::Rc::new(data())));
        p.event(ControlEvent::Tick, &c);
        assert_eq!(p.selected, Some(0));
        assert!(p
            .frame(&c)
            .controls
            .iter()
            .any(|v| v.id == "enter" && v.enabled));
    });
}

#[test]
fn startup_error_blocks_advance_and_acknowledgement_exits_once() {
    context(|base| {
        let mut view = base.pregame.clone();
        view.error = Some("Connection failed".into());
        let c = Context {
            game: base.game,
            pregame: &view,
            keyboard: base.keyboard,
            settings: base.settings,
            map_teleport_allowed: false,
            classic: base.classic,
        };
        let mut p = Pregame::new("startup", Ok(std::rc::Rc::new(data())));
        let actions = p.event(ControlEvent::Tick, &c);
        assert!(
            matches!(actions.as_slice(),[PanelAction::Message {id,accept,..}]
            if id=="startup-error" && *accept==vec![PanelAction::Host(HostAction::Quit)])
        );
        assert_eq!(p.page, "startup");
        assert!(p.event(ControlEvent::Tick, &c).is_empty());
    });
}

#[test]
fn save_as_existing_name_only_emits_overwrite_after_confirmation() {
    context(|base| {
        let mut keyboard = base.keyboard.clone();
        keyboard.schemes = vec!["Default".into(), "Existing".into()];
        let c = Context {
            game: base.game,
            pregame: base.pregame,
            keyboard: &keyboard,
            settings: base.settings,
            map_teleport_allowed: false,
            classic: base.classic,
        };
        let mut p = Pregame::new("keyboard", Ok(std::rc::Rc::new(data())));
        p.save_scheme = Some("existing".into());
        let actions = p.event(ControlEvent::Activate("key-save-confirm".into()), &c);
        assert!(
            matches!(actions.as_slice(),[PanelAction::Question {id,accept,..}]
            if id=="overwrite-key-scheme" && *accept==vec![PanelAction::Host(HostAction::OverwriteKeyMap{name:"existing".into()})])
        );
    });
}

#[test]
fn dirty_keyboard_exit_and_switch_offer_save_and_preserve_pending_transition_until_saved() {
    context(|base| {
        let mut keyboard = base.keyboard.clone();
        keyboard.dirty = true;
        keyboard.schemes = vec!["Default".into(), "Named".into()];
        keyboard.scheme = 1;
        let mut c = Context {
            game: base.game,
            pregame: base.pregame,
            keyboard: &keyboard,
            settings: base.settings,
            map_teleport_allowed: false,
            classic: base.classic,
        };
        let mut p = Pregame::new("keyboard", Ok(std::rc::Rc::new(data())));
        let ask = p.event(ControlEvent::Activate("key-done".into()), &c);
        let [PanelAction::Question { accept, reject, .. }] = ask.as_slice() else {
            panic!("dirty exit asks")
        };
        assert!(matches!(
            reject.as_slice(),
            [
                PanelAction::Host(HostAction::RestoreBindings),
                PanelAction::Control(_)
            ]
        ));
        let PanelAction::Control(save) = &accept[0] else {
            panic!("save opens naming dialog")
        };
        p.event(save.clone(), &c);
        assert_eq!(p.save_scheme.as_deref(), Some("Named"));
        p.save_scheme = Some("New".into());
        assert!(matches!(
            p.event(ControlEvent::Activate("key-save-confirm".into()), &c)
                .as_slice(),
            [PanelAction::Host(HostAction::SaveKeyMapAs { .. })]
        ));
        p.event(ControlEvent::Tick, &c);
        assert_eq!(p.page, "keyboard");
        let mut saved = keyboard.clone();
        saved.dirty = false;
        c.keyboard = &saved;
        p.event(ControlEvent::Tick, &c);
        assert_eq!(p.page, "login");
        p.page = "keyboard";
        c.keyboard = &keyboard;
        let ask = p.event(
            ControlEvent::Select {
                id: "scheme".into(),
                index: 0,
            },
            &c,
        );
        assert!(
            matches!(ask.as_slice(),[PanelAction::Question{reject,..}] if matches!(reject.first(),Some(PanelAction::Host(HostAction::KeyboardScheme(0)))))
        );
    });
}

#[test]
fn same_key_or_cancel_completion_closes_capture_without_requiring_changed_labels() {
    context(|base| {
        let mut keyboard = base.keyboard.clone();
        keyboard.bindings = vec![KeyBinding {
            action: 5,
            map: 0,
            label: "Walk Forward".into(),
            keys: vec!["W".into()],
        }];
        keyboard.schemes = vec!["Default".into(), "Named".into()];
        keyboard.scheme = 1;
        let mut c = Context {
            game: base.game,
            pregame: base.pregame,
            keyboard: &keyboard,
            settings: base.settings,
            map_teleport_allowed: false,
            classic: base.classic,
        };
        let mut p = Pregame::new("keyboard", Ok(std::rc::Rc::new(data())));
        p.event(ControlEvent::Activate("key-save".into()), &c);
        assert_eq!(p.save_scheme.as_deref(), Some("Named"));
        assert!(matches!(
            p.keyboard(&c)
                .controls
                .iter()
                .find(|v| v.id == "scheme")
                .unwrap()
                .kind,
            ControlKind::Choice { .. }
        ));
        p.event(
            ControlEvent::Select {
                id: "scheme-saved-name".into(),
                index: 0,
            },
            &c,
        );
        assert_eq!(p.save_scheme.as_deref(), Some("Default"));
        p.event(ControlEvent::Activate("key-save-cancel".into()), &c);
        p.event(ControlEvent::Activate("binding-0-0".into()), &c);
        p.event(ControlEvent::Tick, &c);
        assert_eq!(p.page, "key-edit");
        let mut completed = keyboard.clone();
        completed.capture_revision += 1;
        c.keyboard = &completed;
        p.event(ControlEvent::Tick, &c);
        assert_eq!(p.page, "keyboard");
        assert_eq!(c.keyboard.bindings[0].keys, ["W"]);
    });
}
#[test]
fn credit_vial_shows_the_share_of_credits_left_above_the_attribute_minimums() {
    assert_eq!(vial_height(270, 330), 108);
    assert_eq!(vial_height(135, 330), 54);
    assert_eq!(vial_height(3, 330), 1);
    assert_eq!(vial_height(2, 330), 0);
    assert_eq!(vial_height(0, 330), 0);
    assert_eq!(vial_height(-5, 330), 0);
    assert_eq!(vial_height(10, 60), 0);
}
#[test]
fn attributes_page_draws_the_vial_from_the_bottom_with_no_balance_toggle() {
    context(|c| {
        let p = Pregame::new("attributes", Ok(std::rc::Rc::new(data())));
        let f = p.frame(c);
        assert!(!f.controls.iter().any(|v| v.id == "balance"));
        // 330 credits, 300 spent: 30 of the 270 spendable left fill 12 of the 108 rows.
        assert!(f.screen.commands.iter().any(|v| matches!(
            v,
            crate::Command::Image { did, x: 379, y: 141, clip: Some(clip), .. }
                if did == "060002CA" && *clip == [379, 237, 507, 249]
        )));
    });
}
#[test]
fn preview_rotate_and_zoom_controls_exist_before_there_is_a_model() {
    context(|c| {
        let p = Pregame::new("heritage", Ok(std::rc::Rc::new(data())));
        let f = p.frame(c);
        assert!(f.previews.is_empty());
        for id in ["rotate-left", "rotate-right", "zoom-face"] {
            assert!(f.controls.iter().any(|v| v.id == id), "{id}");
        }
    });
}
#[test]
fn clothing_and_town_dropdowns_use_the_art_face_at_their_screen_places() {
    context(|c| {
        let mut p = Pregame::new("clothing", Ok(std::rc::Rc::new(data())));
        let f = p.frame(c);
        let style = f.controls.iter().find(|v| v.id == "style-1").unwrap();
        assert!(style.choice_art);
        assert_eq!(style.rect, rect(368, 293, 143, 26));
        p.page = "name-summary";
        let f = p.frame(c);
        let area = f.controls.iter().find(|v| v.id == "area").unwrap();
        assert!(area.choice_art);
        assert_eq!((area.rect.x, area.rect.y), (616, 182));
    });
}

#[test]
fn the_trademark_sign_sits_after_the_title_not_over_it() {
    use dereth_classic_gdi::fonts::{rasterize, FontSpec};
    let p = Pregame::new("login", Ok(std::rc::Rc::new(data())));
    let mut boxes = vec![];
    context(|c| {
        for command in p.login(c).screen.commands {
            if let crate::Command::TextBox {
                text,
                rect,
                font,
                align,
                ..
            } = command
            {
                if text == "Asheron's Call" || text == "TM" {
                    boxes.push((text, rect, font, align));
                }
            }
        }
    });
    // Where the text's ink starts and ends in its box, measured with the font's own advances.
    let span = |(text, rect, font, align): &(String, [i32; 4], String, TextAlign)| {
        let (_, height, width, weight, face) = crate::art::FONTS
            .into_iter()
            .find(|f| f.0 == font.as_str())
            .unwrap();
        let spec = FontSpec {
            height,
            width,
            weight,
            italic: false,
            face: face.into(),
        };
        let atlas = rasterize(&spec).unwrap();
        let w: i32 = text
            .chars()
            .map(|c| atlas.glyphs[&(c as u32)].advance)
            .sum();
        let left = match align {
            TextAlign::Center => rect[0] + (rect[2] - w) / 2,
            TextAlign::Right => rect[0] + rect[2] - w,
            _ => rect[0],
        };
        (left, left + w)
    };
    let [title, tm] = boxes.as_slice() else {
        panic!("{boxes:?}")
    };
    assert!(
        span(tm).0 >= span(title).1,
        "title {:?}, sign {:?}",
        span(title),
        span(tm)
    );
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
fn allocated_credits_and_raw_name_edits_reach_the_real_finish_action() {
    context(|base| {
        let mut view = base.pregame.clone();
        view.connected = true;
        view.character_set = Some(dereth_client_contract::persist::CharacterSet::default());
        let c = Context {
            pregame: &view,
            ..*base
        };
        let mut p = Pregame::new("name-summary", Ok(std::rc::Rc::new(data())));
        p.attribute(0, 80);
        assert_eq!(p.state.remaining_atrb_credits, 0);
        assert_eq!(
            p.view(p.data.as_ref().unwrap())
                .remaining_attributes(p.data.as_ref().unwrap()),
            0
        );
        p.event(
            ControlEvent::Edit {
                id: "name".into(),
                text: "probe ".into(),
            },
            &c,
        );
        assert_eq!(p.name_edit, "probe ");
        p.event(
            ControlEvent::Edit {
                id: "name".into(),
                text: "probe walker".into(),
            },
            &c,
        );
        assert_eq!(p.name_edit, "probe walker");
        let actions = p.event(ControlEvent::Activate("create-submit".into()), &c);
        assert!(
            matches!(
                actions.as_slice(),
                [PanelAction::Game(UiRequest::CharGenAction(
                    CharGenAction::SendCharGenResult(_)
                ))]
            ),
            "{actions:?}"
        );
        p.waiting = false;
        p.event(
            ControlEvent::Edit {
                id: "name".into(),
                text: String::new(),
            },
            &c,
        );
        assert!(p.name_edit.is_empty());
        assert!(
            !p.frame(&c)
                .controls
                .iter()
                .find(|v| v.id == "create-submit")
                .unwrap()
                .enabled
        );
        assert!(p
            .event(ControlEvent::Activate("create-confirmed".into()), &c)
            .is_empty());
        assert_eq!(p.status, "Enter a character name.");
    });
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
fn later_appearance_choices_and_clothing_palette_order_are_reachable() {
    context(|c| {
        let mut t = tables();
        let sx = t
            .chargen
            .heritage_groups
            .get_mut(&1)
            .unwrap()
            .sexes
            .get_mut(&2)
            .unwrap();
        sx.hair_colors = (0..9).collect();
        sx.hair_styles = (0..14)
            .map(|_| dereth_assets::tables::HairStyle {
                icon: 0,
                bald: 0,
                alternate_setup: DataId(0),
                objdesc: sx.base_objdesc.clone(),
            })
            .collect();
        sx.clothing_colors.reverse();
        let mut p = Pregame::new(
            "appearance",
            Ok(std::rc::Rc::new(CreationData::from_tables(
                std::rc::Rc::new(t),
            ))),
        );
        p.event(
            ControlEvent::Value {
                id: "hair-color".into(),
                value: 8,
            },
            c,
        );
        assert_eq!(p.state.hair_color, 8);
        assert!(p
            .frame(c)
            .controls
            .iter()
            .any(|v| v.id == "hair-color-pick-8"));
        p.event(
            ControlEvent::Scroll {
                id: "hairstyles-scroll".into(),
                value: 96,
            },
            c,
        );
        let f = p.frame(c);
        let last = f
            .controls
            .iter()
            .find(|v| v.id == "hair-style-pick-13")
            .expect("last style");
        assert!(last.rect.y + last.rect.h <= 503);
        p.event(ControlEvent::Activate("hair-style-pick-13".into()), c);
        assert_eq!(p.state.hair_style, 13);
        p.page = "clothing";
        p.event(
            ControlEvent::Select {
                id: "style-1".into(),
                index: 0,
            },
            c,
        );
        let projected = p
            .view(p.data.as_ref().unwrap())
            .clothing_colors(p.data.as_ref().unwrap(), 1);
        assert_eq!(
            projected.iter().map(|v| v.key).collect::<Vec<_>>(),
            p.state.shirt_palette_template_ids
        );
        assert_eq!(projected.first().unwrap().key, 1);
        p.event(ControlEvent::Activate("color-pick-1-0".into()), c);
        assert_eq!(p.state.get_char_gen_result().shirt_color, 1);
    });
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "needs installed world and interface data"
)]
fn both_worlds_project_real_keys_face_pixels_preview_resources_and_results() {
    use dereth_assets::{
        tables::{CharGen, SkillTable},
        Decode,
    };
    use dereth_primitives::AssetSource;
    use std::{collections::BTreeMap, rc::Rc, sync::Arc};
    struct Fonts;
    impl dereth_classic_dat::fonts::FontSource for Fonts {
        fn rasterize(
            &self,
            _: &dereth_classic_dat::fonts::FontSpec,
        ) -> Result<dereth_classic_dat::fonts::FontAtlas, String> {
            Ok(Default::default())
        }
    }
    let portal = std::path::PathBuf::from(
        std::env::var_os("DERETH_CLASSIC_PORTAL").expect("interface portal"),
    );
    let art = Arc::new(
        crate::art::ClassicArt::new(
            dereth_classic_dat::ClassicPortal::open(&portal).unwrap(),
            &Fonts,
        )
        .unwrap(),
    );
    for old in [true, false] {
        let store = if old {
            dereth_dat::testing::open_pre_tod_store_or_fail()
        } else {
            dereth_dat::testing::open_store_or_fail()
        };
        let id = DataId(0x0e000002);
        let cg =
            CharGen::decode_payload_in(store.container_era_of(id), id, &store.read(id).unwrap())
                .unwrap();
        let id = DataId(0x0e000004);
        let skills =
            SkillTable::decode_payload_in(store.container_era_of(id), id, &store.read(id).unwrap())
                .unwrap();
        let mut clothing = BTreeMap::new();
        for sx in cg.heritage_groups.values().flat_map(|h| h.sexes.values()) {
            for item in sx
                .headgear
                .iter()
                .chain(&sx.shirts)
                .chain(&sx.pants)
                .chain(&sx.footwear)
            {
                let id = item.clothing_table;
                clothing.entry(id).or_insert_with(|| {
                    dereth_assets::motion::ClothingTable::decode_payload_in(
                        store.container_era_of(id),
                        id,
                        &store.read(id).unwrap(),
                    )
                    .unwrap()
                });
            }
        }
        let t = Rc::new(dereth_chargen::CreationTables {
            chargen: cg,
            skills,
            clothing: Rc::new(clothing),
        });
        let mut data = CreationData::load(Rc::clone(&t), &store);
        data.read_chrome(&art);
        for id in [0x31000020u32, 0x31000022] {
            assert!(!data.help_text[&id.to_string()].is_empty());
        }
        assert_eq!(
            data.heritages.iter().map(|h| h.key).collect::<Vec<_>>(),
            t.heritage_keys()
        );
        let mut canvas = crate::renderer::Canvas::new(Arc::clone(&art), (800, 600)).unwrap();
        let mut screen = crate::Screen {
            width: 800,
            height: 600,
            commands: vec![],
        };
        for h in &data.heritages {
            eprintln!(
                "world_old={old} heritage={} {} sexes={:?} towns={:?}",
                h.key,
                h.name,
                h.sexes.iter().map(|s| s.key).collect::<Vec<_>>(),
                h.primary_areas
            );
            for sx in &h.sexes {
                eprintln!(
                    "  sex={} hair_colors={} styles={}",
                    sx.key,
                    sx.hair_colors.len(),
                    sx.hair_styles.len()
                );
                for strip in sx.eyes.iter().chain(&sx.noses).chain(&sx.mouths) {
                    for did in [&strip.texture, &strip.bald_texture]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                    {
                        screen.commands.push(crate::Command::IndexedImage {
                            did: did.clone(),
                            palette: data.appearance.palettes[&format!("{:08X}", sx.base_palette)]
                                .clone(),
                            x: 0,
                            y: 0,
                            width: 64,
                            height: 48,
                            clip: None,
                            flip_x: false,
                        });
                    }
                }
                assert!(!store.read(DataId(h.animation)).unwrap().is_empty());
            }
        }
        canvas
            .load_runtime_images(&screen, &store)
            .expect("every face strip decodes through Canvas");
        canvas
            .compose(
                &mut dereth_client_runtime::present::NullPresentation::new(800, 600),
                &screen,
                &|_| None,
            )
            .expect("every face strip uploads with its complete palette");
        context(|base| {
            let mut view = base.pregame.clone();
            view.connected = true;
            view.character_set = Some(dereth_client_contract::persist::CharacterSet::default());
            let c = Context {
                pregame: &view,
                ..*base
            };
            let data = Rc::new(data);
            let mut p = Pregame::new("heritage", Ok(Rc::clone(&data)));
            for (hi, h) in data.heritages.iter().enumerate() {
                p.event(
                    ControlEvent::Select {
                        id: "heritage".into(),
                        index: hi,
                    },
                    &c,
                );
                for (si, sx) in h.sexes.iter().enumerate() {
                    p.event(
                        ControlEvent::Select {
                            id: "sex".into(),
                            index: si,
                        },
                        &c,
                    );
                    p.enter_page("name-summary");
                    let f = p.frame(&c);
                    let a = f.previews[0].appearance.as_ref().unwrap();
                    assert_eq!((a.state.heritage_group, a.state.gender), (h.key, sx.key));
                    assert_eq!(a.animation.0, h.animation);
                    assert!(!store
                        .read(a.state.get_setup_id(&t.chargen))
                        .unwrap()
                        .is_empty());
                    p.event(
                        ControlEvent::Edit {
                            id: "name".into(),
                            text: "World Tester".into(),
                        },
                        &c,
                    );
                    p.creation_slot = Some(6);
                    let out = p.event(ControlEvent::Activate("create-confirmed".into()), &c);
                    let [PanelAction::Game(UiRequest::CharGenAction(
                        CharGenAction::SendCharGenResult(result),
                    ))] = out.as_slice()
                    else {
                        panic!("missing result: {out:?}")
                    };
                    assert_eq!((result.heritage_group, result.gender), (h.key, sx.key));
                    assert_eq!(result.slot, 6);
                    p.waiting = false;
                }
            }
        });
    }
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
fn special_heritage_skip_route_starts_with_body_framing_and_keeps_explicit_zoom() {
    context(|c| {
        let mut t = tables();
        let heritage = t.chargen.heritage_groups[&1].clone();
        for key in [12, 13] {
            t.chargen.heritage_groups.insert(key, heritage.clone());
            t.chargen.heritage_order.push(key);
        }
        let data = std::rc::Rc::new(CreationData::from_tables(std::rc::Rc::new(t)));
        for (index, key) in [(1, 12), (2, 13)] {
            let mut p = Pregame::new("heritage", Ok(std::rc::Rc::clone(&data)));
            assert!(p.zoom_face, "ordinary initial framing");
            p.event(
                ControlEvent::Select {
                    id: "heritage".into(),
                    index,
                },
                c,
            );
            assert_eq!(p.state.heritage_group, key);
            p.event(ControlEvent::Activate("next".into()), c);
            assert_eq!(p.page, "sex");
            p.event(
                ControlEvent::Select {
                    id: "sex".into(),
                    index: 0,
                },
                c,
            );
            assert!(
                !p.frame(c).previews[0]
                    .appearance
                    .as_ref()
                    .unwrap()
                    .zoom_face
            );
            p.event(ControlEvent::Activate("next".into()), c);
            assert_eq!(p.page, "name-summary");
            assert!(
                !p.frame(c).previews[0]
                    .appearance
                    .as_ref()
                    .unwrap()
                    .zoom_face
            );
            p.event(
                ControlEvent::Check {
                    id: "zoom-face".into(),
                    checked: true,
                },
                c,
            );
            p.event(ControlEvent::Activate("back".into()), c);
            assert_eq!(p.page, "sex");
            p.event(
                ControlEvent::Select {
                    id: "sex".into(),
                    index: 0,
                },
                c,
            );
            p.event(ControlEvent::Activate("next".into()), c);
            assert!(
                p.frame(c).previews[0]
                    .appearance
                    .as_ref()
                    .unwrap()
                    .zoom_face,
                "explicit zoom survives skipped route"
            );
        }
    });
}

/// Behaviour: options.key-bindings.classic-wheel-covers-binding-columns
#[test]
fn the_real_keyboard_list_scrolls_over_names_and_each_binding_column() {
    context(|base| {
        let mut keyboard = base.keyboard.clone();
        keyboard.bindings = (0..50)
            .map(|i| KeyBinding {
                action: 5,
                map: 4,
                label: format!("Action {i}"),
                keys: vec!["W".into()],
            })
            .collect();
        let c = Context {
            keyboard: &keyboard,
            ..*base
        };
        for x in [150, 340, 490, 650] {
            let mut panel = Pregame::new("keyboard", Ok(std::rc::Rc::new(data())));
            let mut host = crate::control_host::ControlHost::default();
            host.sync(&panel.frame(&c));
            let events = host.handle(crate::widgets::Input::Wheel {
                x,
                y: 300,
                delta: 2,
            });
            assert!(!events.is_empty(), "column {x}");
            for e in events {
                panel.event(e, &c);
            }
            assert_eq!(panel.keyboard_scroll, 34, "column {x}");
            host.sync(&panel.frame(&c));
            host.handle(crate::widgets::Input::PointerDown { x: 340, y: 210 });
            for e in host.handle(crate::widgets::Input::PointerUp { x: 340, y: 210 }) {
                panel.event(e, &c);
            }
            assert_eq!(
                panel.page, "key-edit",
                "scrolling did not disable slot capture"
            );
        }
    });
}
