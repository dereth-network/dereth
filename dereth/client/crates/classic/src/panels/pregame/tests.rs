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
fn data() -> CreationData {
    let clothes = vec![
        Named {
            name: "First".into(),
            colors: vec![ClothingColor {
                key: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        Named {
            name: "Last".into(),
            ..Default::default()
        },
    ];
    CreationData {
        heritages: vec![Heritage {
            sexes: vec![Sex {
                attribute_credits: 330,
                skill_credits: 100,
                headgear: clothes.clone(),
                shirts: clothes,
                templates: vec![Template {
                    profiles: vec![Profile {
                        attributes: [50; 6],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }],
        skills: vec![
            Skill {
                id: 1,
                chargen: 0,
                ..Default::default()
            },
            Skill {
                id: 7,
                name: "First skill".into(),
                chargen: 1,
                trained: 4,
                specialized: 8,
                ..Default::default()
            },
            Skill {
                id: 8,
                name: "Second skill".into(),
                chargen: 1,
                trained: 6,
                specialized: 12,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}
#[test]
fn choosing_heritage_and_sex_enables_normal_next_navigation() {
    context(|c| {
        let mut p = Pregame::new("heritage", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("clothing", Ok(std::sync::Arc::new(data())));
        p.state.colors[0] = 5;
        p.event(
            ControlEvent::Select {
                id: "style-0".into(),
                index: 1,
            },
            c,
        );
        assert_eq!(p.state.styles[0], 0);
        assert_eq!(p.state.colors[0], 0);
        p.event(
            ControlEvent::Select {
                id: "style-0".into(),
                index: 2,
            },
            c,
        );
        assert_eq!(p.state.styles[0], 1);
        p.event(
            ControlEvent::Select {
                id: "style-0".into(),
                index: 0,
            },
            c,
        );
        assert_eq!(p.state.styles[0], usize::MAX);
        p.event(
            ControlEvent::Select {
                id: "style-1".into(),
                index: 1,
            },
            c,
        );
        assert_eq!(p.state.styles[1], 1);
    });
}
#[test]
fn attribute_slider_and_numeric_editor_have_distinct_routing_ids() {
    context(|c| {
        let mut p = Pregame::new("attributes", Ok(std::sync::Arc::new(data())));
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
        assert_eq!(p.state.attrs[0], 60);
        p.event(
            ControlEvent::Edit {
                id: "attr-value-0".into(),
                text: "75".into(),
            },
            c,
        );
        assert_eq!(p.state.attrs[0], 75);
    });
}
#[test]
fn selected_skill_uses_original_table_index_across_hidden_rows_and_regrouping() {
    context(|c| {
        let mut p = Pregame::new("skills", Ok(std::sync::Arc::new(data())));
        let row = presentation::skill_rows(p.data.as_ref().unwrap(), &p.state)
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
        assert_eq!(p.state.skills.get(&7), Some(&2));
        assert_eq!(p.state.skills.get(&8), Some(&1));
        assert_eq!(p.state.selected_skill, Some(1));
        p.event(ControlEvent::Activate("specialize".into()), c);
        assert_eq!(p.state.skills.get(&7), Some(&3));
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
        let mut p = Pregame::new("login", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("login", Ok(std::sync::Arc::new(data())));
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
            let mut p = Pregame::new("name-summary", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("name-summary", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("login", Ok(std::sync::Arc::new(data())));
        p.waiting = true;
        assert!(p.event(ControlEvent::Tick, &c).is_empty());
        assert!(!p.waiting);
        assert!(p.status.is_empty());
    });
}

#[test]
fn confirmed_login_quit_requests_process_exit_without_dropping_only_the_panel() {
    context(|c| {
        let mut p = Pregame::new("login", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("startup", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("enter-confirmation", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("startup", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("keyboard", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("keyboard", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("keyboard", Ok(std::sync::Arc::new(data())));
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
        let p = Pregame::new("attributes", Ok(std::sync::Arc::new(data())));
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
        let p = Pregame::new("heritage", Ok(std::sync::Arc::new(data())));
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
        let mut p = Pregame::new("clothing", Ok(std::sync::Arc::new(data())));
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
    let p = Pregame::new("login", Ok(std::sync::Arc::new(data())));
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
