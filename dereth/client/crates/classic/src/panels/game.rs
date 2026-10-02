//! The classic interface's character and item panels.
mod appraisal;
mod common;
pub mod equipment_drop;
mod examine;
mod items;
mod magic;
mod research;
mod systems;
pub use magic::research_on;
pub mod shortcut_drop;
mod shortcuts;
mod stats;
use super::Panel;
pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    Some(match id {
        "attributes" | "character-stats" => Box::new(stats::Stats::new(false)),
        "skills" => Box::new(stats::Stats::new(true)),
        "spellbook" => Box::new(magic::Spellbook::new(false)),
        "components" => Box::new(magic::Spellbook::new(true)),
        "spell-research" => Box::new(research::SpellResearch::default()),
        "titles" => Box::new(systems::Titles::default()),
        "contracts" => Box::new(systems::Contracts::default()),
        "journal" => Box::new(systems::Journal::default()),
        "beneficial-effects" => Box::new(magic::Effects::new(true)),
        "harmful-effects" => Box::new(magic::Effects::new(false)),
        "vitae" => Box::new(magic::Vitae),
        "inventory" | "equipment" => Box::new(items::Inventory::default()),
        "external-container" => Box::new(items::ExternalContainer::default()),
        "examine" | "examine-item" | "examine-creature" | "examine-character" | "examine-spell" => {
            Box::new(examine::Examine::default())
        }
        "shortcuts" => Box::new(shortcuts::Shortcuts),
        "spell-favorites" => Box::new(shortcuts::Favorites::default()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic panel adapter; no retail behaviour claim).
    use super::super::*;
    use super::*;
    use dereth_client_contract::view::{
        AttributeAdvancement, ComponentCategory, ComponentRow, SkillAdvancement, SkillEntry,
        SpellEntry,
    };
    #[derive(Debug, Default)]
    struct World {
        spells: Vec<SpellEntry>,
        skills: Vec<SkillEntry>,
        filter: u32,
        components: Vec<ComponentCategory>,
        favorites: Vec<u32>,
        xp: i64,
        credits: i64,
        adv: SkillAdvancement,
        attr: AttributeAdvancement,
        appraisal: Option<dereth_client_contract::view::AppraisalView>,
        shortcuts: Vec<ObjectId>,
        contents: Vec<ObjectId>,
        equipment: Vec<(ObjectId, u32)>,
        valid_locations: Vec<(ObjectId, u32)>,
    }
    impl GameView for World {
        fn item_valid_locations(&self, id: ObjectId) -> Option<u32> {
            self.valid_locations
                .iter()
                .find(|(object, _)| *object == id)
                .map(|(_, mask)| *mask)
        }
        fn equipment(&self, _: ObjectId) -> &[(ObjectId, u32)] {
            &self.equipment
        }
        fn container_contents(&self, _: ObjectId) -> &[ObjectId] {
            &self.contents
        }
        fn shortcut(&self, slot: u32) -> Option<ObjectId> {
            self.shortcuts.get(slot as usize).copied()
        }
        fn appraisal(&self, _: ObjectId) -> Option<dereth_client_contract::view::AppraisalView> {
            self.appraisal.clone()
        }
        fn character_name(&self) -> Option<&str> {
            Some("Aerin")
        }
        fn player(&self) -> Option<ObjectId> {
            Some(ObjectId(7))
        }
        fn spellbook(&self) -> &[SpellEntry] {
            &self.spells
        }
        fn spell(&self, id: u32) -> Option<SpellEntry> {
            self.spells.iter().find(|s| s.id == id).cloned()
        }
        fn is_spell_known(&self, id: u32) -> bool {
            self.spells.iter().any(|s| s.id == id)
        }
        fn spell_filters(&self) -> u32 {
            self.filter
        }
        fn skills(&self) -> &[SkillEntry] {
            &self.skills
        }
        fn skill_advancement(&self, _: u32) -> Option<SkillAdvancement> {
            Some(self.adv)
        }
        fn attribute_advancement(&self, _: u32, _: bool) -> Option<AttributeAdvancement> {
            Some(self.attr)
        }
        fn available_experience(&self) -> i64 {
            self.xp
        }
        fn skill_credits(&self) -> i64 {
            self.credits
        }
        fn spell_components(&self) -> Vec<ComponentCategory> {
            self.components.clone()
        }
        fn spell_tab(&self, _: usize) -> &[u32] {
            &self.favorites
        }
    }
    fn with_context<T>(world: &World, run: impl FnOnce(&Context<'_>) -> T) -> T {
        run(&Context {
            game: world,
            pregame: &Default::default(),
            keyboard: &Default::default(),
            settings: &Default::default(),
            map_teleport_allowed: false,
            classic: &Default::default(),
        })
    }
    fn spell(id: u32, school: u32, level: u32) -> SpellEntry {
        SpellEntry {
            id,
            name: format!("Spell{id}"),
            icon: None,
            school,
            level,
            display_order: id as i32,
            bitfield: 0,
        }
    }
    fn click(id: &str) -> ControlEvent {
        ControlEvent::Activate(id.into())
    }
    fn select(index: usize) -> ControlEvent {
        ControlEvent::Select {
            id: "rows".into(),
            index,
        }
    }
    #[test]
    fn every_game_family_constructs_without_world_data() {
        let world = World::default();
        with_context(&world, |ctx| {
            for id in [
                "attributes",
                "skills",
                "inventory",
                "external-container",
                "spellbook",
                "components",
                "beneficial-effects",
                "harmful-effects",
                "vitae",
                "examine",
                "shortcuts",
                "spell-favorites",
            ] {
                let f = make(id).unwrap().frame(ctx);
                assert!(f.screen.width > 0 && f.screen.height > 0, "{id}");
            }
        });
    }
    #[test]
    fn filters_map_school_bits_including_void_and_the_eighth_level() {
        assert!(magic::visible_spell(&spell(1, 1, 1), 8 | 0x10));
        assert!(!magic::visible_spell(&spell(1, 1, 1), 1 | 0x10));
        assert!(magic::visible_spell(&spell(1, 4, 7), 1 | 0x400));
        // Void (school 5) and the eighth level have filters of their own.
        assert!(magic::visible_spell(&spell(1, 5, 1), 0x2000 | 0x10));
        assert!(!magic::visible_spell(&spell(1, 5, 1), 0x0f | 0x10));
        assert!(magic::visible_spell(&spell(1, 1, 8), 8 | 0x800));
        assert!(!magic::visible_spell(&spell(1, 1, 9), 0xffffffff));
    }
    #[test]
    fn filter_checkbox_preserves_unrelated_bits() {
        let w = World {
            filter: 0x80000000 | 0x10,
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("spellbook").unwrap();
            assert_eq!(
                p.event(
                    ControlEvent::Check {
                        id: "filter:8".into(),
                        checked: true
                    },
                    ctx
                ),
                vec![PanelAction::Game(UiRequest::SetSpellbookFilter {
                    mask: 0x80000018
                })]
            );
        });
    }
    #[test]
    fn training_request_is_affordable_and_not_repeated_before_value_changes() {
        let w = World {
            xp: 100,
            attr: AttributeAdvancement {
                cost_to_raise: 50,
                effective: 10,
                ..Default::default()
            },
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("attributes").unwrap();
            p.event(select(0), ctx);
            assert_eq!(
                p.event(click("raise"), ctx),
                vec![PanelAction::Game(UiRequest::TrainAttribute {
                    attribute: 1,
                    xp: 50
                })]
            );
            assert!(p.event(click("raise"), ctx).is_empty());
        });
    }
    #[test]
    fn unaffordable_training_never_emits_a_request() {
        let w = World {
            xp: 49,
            attr: AttributeAdvancement {
                cost_to_raise: 50,
                ..Default::default()
            },
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("attributes").unwrap();
            p.event(select(0), ctx);
            assert!(p.event(click("raise"), ctx).is_empty());
        });
    }
    #[test]
    fn skill_credit_confirmation_can_be_reopened_after_cancel() {
        let w = World {
            credits: 4,
            skills: vec![SkillEntry {
                id: 12,
                name: "Magic".into(),
                icon: None,
                min_level: 0,
                sac: 1,
                level: 0,
                effective: 1,
                vitae: 0,
            }],
            adv: SkillAdvancement {
                sac: 1,
                cost_to_raise: 4,
                ..Default::default()
            },
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("skills").unwrap();
            p.event(select(3), ctx);
            let first = p.event(click("raise"), ctx);
            assert!(
                matches!(first.as_slice(),[PanelAction::Confirm{accept,..}] if accept==&vec![PanelAction::Game(UiRequest::TrainSkillAdvancementClass{skill:12,credits:4})])
            );
            assert_eq!(p.event(click("raise"), ctx), first);
        });
    }
    #[test]
    fn spell_deletion_requires_confirmation_and_preserves_authoritative_book() {
        let w = World {
            spells: vec![spell(17, 1, 1)],
            filter: 0x18,
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("spellbook").unwrap();
            p.event(select(0), ctx);
            assert!(
                matches!(p.event(click("delete"),ctx).as_slice(),[PanelAction::Confirm{accept,..}] if accept==&vec![PanelAction::Game(UiRequest::RemoveSpell{spell_id:17})])
            );
            assert_eq!(ctx.game.spellbook().len(), 1);
        });
    }
    #[test]
    fn a_creature_with_a_face_shows_its_face_and_one_without_its_icon() {
        let w = World {
            appraisal: Some(dereth_client_contract::view::AppraisalView {
                creature: true,
                success: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut faces = ClassicState::default();
        faces.portraits.insert(
            ObjectId(5),
            ClassicPortrait {
                textures: [1, 2, 3],
                palettes: [4, 5, 6],
            },
        );
        let frame = |state: &ClassicState| {
            let mut p = make("examine").unwrap();
            p.set_object(ObjectId(5));
            p.frame(&Context {
                game: &w,
                pregame: &Default::default(),
                keyboard: &Default::default(),
                settings: &Default::default(),
                map_teleport_allowed: false,
                classic: state,
            })
        };
        // The face's frame edge, where a character's is.
        let face_edge = |f: &PanelFrame| {
            f.screen.commands.iter().any(
                |c| matches!(c, crate::Command::Image { did, x: 172, .. } if did == "060012C6"),
            )
        };
        assert!(face_edge(&frame(&faces)));
        assert!(!face_edge(&frame(&ClassicState::default())));
    }
    #[test]
    fn desired_component_count_waits_for_commit_and_rejects_out_of_range() {
        let w = World {
            components: vec![ComponentCategory {
                category: 0,
                rows: vec![ComponentRow {
                    wcid: 3,
                    name: "Scarab".into(),
                    icon: None,
                    owned: 1,
                    desired: 2,
                    object: None,
                }],
            }],
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("components").unwrap();
            assert!(p
                .event(
                    ControlEvent::Edit {
                        id: "desired:3".into(),
                        text: "5001".into()
                    },
                    ctx
                )
                .is_empty());
            assert!(p
                .event(
                    ControlEvent::Commit {
                        id: "desired:3".into()
                    },
                    ctx
                )
                .is_empty());
            p.event(
                ControlEvent::Edit {
                    id: "desired:3".into(),
                    text: "5000".into(),
                },
                ctx,
            );
            assert_eq!(
                p.event(
                    ControlEvent::Commit {
                        id: "desired:3".into()
                    },
                    ctx
                ),
                vec![PanelAction::Game(UiRequest::SetDesiredComponentLevel {
                    wcid: 3,
                    level: 5000
                })]
            );
        });
    }
    #[test]
    fn equipment_drop_preserves_the_source_location_mask() {
        let world = World {
            valid_locations: vec![(ObjectId(12), 0x8000), (ObjectId(13), 4)],
            ..Default::default()
        };
        with_context(&world, |ctx| {
            let mut p = make("inventory").unwrap();
            assert_eq!(
                p.event(
                    ControlEvent::Drop {
                        id: "equip:0".into(),
                        payload: DragPayload::Object(ObjectId(12)),
                        slot: 0
                    },
                    ctx
                ),
                vec![PanelAction::Host(HostAction::Equip {
                    object: ObjectId(12),
                    location: 0x8000,
                    slot: 0
                })]
            );
            assert!(p
                .event(
                    ControlEvent::Drop {
                        id: "equip:0".into(),
                        payload: DragPayload::Object(ObjectId(13)),
                        slot: 0
                    },
                    ctx
                )
                .is_empty());
            assert_eq!(
                p.event(
                    ControlEvent::Drop {
                        id: "paperdoll".into(),
                        payload: DragPayload::Object(ObjectId(13)),
                        slot: 0
                    },
                    ctx
                ),
                [PanelAction::Host(HostAction::Wear(ObjectId(13)))]
            );
            assert!(p
                .event(
                    ControlEvent::Drop {
                        id: "paperdoll".into(),
                        payload: DragPayload::Object(ObjectId(12)),
                        slot: 0
                    },
                    ctx
                )
                .is_empty());
        });
    }
    #[test]
    fn shortcut_bar_exposes_exactly_nine_slots() {
        with_context(&World::default(), |ctx| {
            assert_eq!(make("shortcuts").unwrap().frame(ctx).controls.len(), 9)
        });
    }
    #[test]
    fn moving_existing_favorite_emits_remove_then_insert() {
        let w = World {
            spells: vec![spell(17, 1, 1)],
            favorites: vec![17, 18, 19],
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("spell-favorites").unwrap();
            assert_eq!(
                p.event(
                    ControlEvent::Drop {
                        id: "spell:2".into(),
                        payload: DragPayload::Spell(17),
                        slot: 0
                    },
                    ctx
                ),
                vec![
                    PanelAction::Game(UiRequest::RemoveSpellFavorite {
                        spell_id: 17,
                        tab: 0
                    }),
                    PanelAction::Game(UiRequest::AddSpellFavorite {
                        spell_id: 17,
                        index: 1,
                        tab: 0
                    })
                ]
            );
        });
    }
    #[test]
    fn zero_cost_training_is_disabled_even_with_experience() {
        let w = World {
            xp: 1000,
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("attributes").unwrap();
            p.event(select(0), ctx);
            assert!(p.event(click("raise"), ctx).is_empty());
            assert!(
                !p.frame(ctx)
                    .controls
                    .iter()
                    .find(|c| c.id == "raise")
                    .unwrap()
                    .enabled
            );
        });
    }
    #[test]
    fn character_footer_uses_classic_order_and_fellowship_color() {
        use dereth_client_contract::view::AppraisalView;
        let a = AppraisalView {
            allegiance_rank: Some(1),
            monarch_title: Some("M".into()),
            patron_title: Some("M".into()),
            fellowship: Some("Friends".into()),
            age: Some(90061),
            num_deaths: Some(0),
            ..Default::default()
        };
        let rows = appraisal::character(&a, "Aerin");
        assert_eq!(rows.iter().map(|r|r.text.as_str()).collect::<String>(),"Monarch/Patron: M\nFellowship: Friends\nTime in Dereth: 1d 1h 1m 1s\nAerin has never died.");
        assert_eq!(rows[1].color, 0xff00ff00);
        assert_eq!(rows[0].color, 0xffd2d2c8);
    }
    #[test]
    fn inscription_commit_normalizes_only_single_space_or_newline_and_sends_once() {
        use dereth_client_contract::view::AppraisalView;
        let w = World {
            appraisal: Some(AppraisalView {
                inscribable: true,
                owned_by_player: true,
                scribe_name: Some("aErIn".into()),
                inscription: Some("Old".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        with_context(&w, |ctx| {
            let mut p = make("examine").unwrap();
            p.set_object(ObjectId(8));
            p.event(
                ControlEvent::Edit {
                    id: "inscription".into(),
                    text: " ".into(),
                },
                ctx,
            );
            assert_eq!(
                p.event(
                    ControlEvent::Commit {
                        id: "inscription".into()
                    },
                    ctx
                ),
                vec![PanelAction::Game(UiRequest::SetInscription {
                    object: ObjectId(8),
                    text: String::new()
                })]
            );
            assert!(p
                .event(
                    ControlEvent::Commit {
                        id: "inscription".into()
                    },
                    ctx
                )
                .is_empty());
            p.event(
                ControlEvent::Edit {
                    id: "inscription".into(),
                    text: "  ".into(),
                },
                ctx,
            );
            assert_eq!(
                p.event(
                    ControlEvent::Commit {
                        id: "inscription".into()
                    },
                    ctx
                ),
                vec![PanelAction::Game(UiRequest::SetInscription {
                    object: ObjectId(8),
                    text: "  ".into()
                })]
            );
        });
    }
    #[test]
    fn inscription_requires_ownership_and_matching_scribe_unless_privileged() {
        use dereth_client_contract::view::AppraisalView;
        for (owned, scribe, privileged, allowed) in [
            (true, "Someone", false, false),
            (false, "", false, false),
            (true, "aerin", false, true),
            (false, "Someone", true, true),
        ] {
            let w = World {
                appraisal: Some(AppraisalView {
                    inscribable: true,
                    owned_by_player: owned,
                    scribe_name: Some(scribe.into()),
                    viewer_is_psr: privileged,
                    ..Default::default()
                }),
                ..Default::default()
            };
            with_context(&w, |ctx| {
                let mut p = make("examine").unwrap();
                p.set_object(ObjectId(8));
                p.event(
                    ControlEvent::Edit {
                        id: "inscription".into(),
                        text: "New".into(),
                    },
                    ctx,
                );
                assert_eq!(
                    !p.event(
                        ControlEvent::Commit {
                            id: "inscription".into()
                        },
                        ctx
                    )
                    .is_empty(),
                    allowed
                );
            });
        }
    }

    #[test]
    fn effect_timer_quantizes_hundredths_of_minutes_and_hides_expired_values() {
        assert_eq!(magic::effect_timer(59., false), "0:58");
        assert_eq!(magic::effect_timer(61., false), "1:00");
        assert_eq!(magic::effect_timer(-0.1, false), "");
        assert_eq!(magic::effect_timer(61., true), "");
    }
    #[test]
    fn portrait_palette_uses_original_indices_in_three_distinct_ranges() {
        let sources = [
            Some(vec![[1, 0, 0, 255]; 256]),
            Some(vec![[2, 0, 0, 255]; 256]),
            Some(vec![[3, 0, 0, 255]; 256]),
        ];
        let palette = examine::portrait_palette(&sources);
        assert!(palette[..24].iter().all(|c| c[0] == 3));
        assert!(palette[24..32].iter().all(|c| c[0] == 2));
        assert!(palette[32..40].iter().all(|c| c[0] == 1));
        assert_eq!(palette[40], [0, 0, 0, 255]);
    }

    #[test]
    fn shortcut_drag_removes_alias_at_pickup_and_retains_its_source_slot() {
        let world = World {
            shortcuts: vec![ObjectId(99)],
            ..Default::default()
        };
        with_context(&world, |ctx| {
            let mut panel = make("shortcuts").unwrap();
            assert!(matches!(
                panel
                    .event(
                        ControlEvent::DragStart {
                            id: "slot:0".into(),
                            index: 0
                        },
                        ctx
                    )
                    .as_slice(),
                [
                    PanelAction::Game(UiRequest::RemoveShortcut(ObjectId(99))),
                    PanelAction::BeginDrag(DragPayload::Shortcut {
                        object: ObjectId(99),
                        from: 0
                    })
                ]
            ));
        });
    }

    #[test]
    fn favorite_drag_removes_only_the_source_tab_at_pickup() {
        let world = World {
            favorites: vec![42, 43],
            ..Default::default()
        };
        with_context(&world, |ctx| {
            let mut panel = make("spell-favorites").unwrap();
            panel.event(ControlEvent::Activate("tab:3".into()), ctx);
            assert_eq!(
                panel.event(
                    ControlEvent::DragStart {
                        id: "spell:1".into(),
                        index: 0
                    },
                    ctx
                ),
                vec![
                    PanelAction::Game(UiRequest::RemoveSpellFavorite {
                        spell_id: 43,
                        tab: 3
                    }),
                    PanelAction::BeginDrag(DragPayload::Spell(43)),
                ]
            );
        });
    }
    #[test]
    fn shortcut_alias_drop_uses_the_remembered_source_for_displacement() {
        with_context(&World::default(), |ctx| {
            assert_eq!(
                make("shortcuts").unwrap().event(
                    ControlEvent::Drop {
                        id: "slot:4".into(),
                        payload: DragPayload::Shortcut {
                            object: ObjectId(99),
                            from: 2
                        },
                        slot: 0,
                    },
                    ctx
                ),
                vec![PanelAction::Host(HostAction::ClassicShortcutDrop {
                    object: ObjectId(99),
                    slot: 4,
                    from: Some(2)
                })]
            );
        });
    }
    #[test]
    fn equipment_drop_rejects_the_object_already_retained_in_that_slot() {
        let world = World {
            equipment: vec![(ObjectId(12), 0x8000)],
            valid_locations: vec![(ObjectId(12), 0x8000)],
            ..Default::default()
        };
        let mut panel = make("inventory").unwrap();
        with_context(&world, |ctx| {
            assert!(panel
                .event(
                    ControlEvent::Drop {
                        id: "equip:0".into(),
                        payload: DragPayload::Object(ObjectId(12)),
                        slot: 0
                    },
                    ctx
                )
                .is_empty());
        });
    }
    #[test]
    fn external_container_reads_live_rows_and_closes_the_current_object() {
        fn rows(frame: &PanelFrame) -> Vec<ObjectId> {
            match &frame
                .controls
                .iter()
                .find(|c| c.id == "items")
                .unwrap()
                .kind
            {
                ControlKind::ItemStrip { entries, .. } => entries.iter().map(|e| e.id).collect(),
                _ => panic!("expected the native horizontal item strip"),
            }
        }
        let mut world = World {
            contents: vec![ObjectId(100), ObjectId(101)],
            ..Default::default()
        };
        let mut panel = make("external-container").unwrap();
        panel.set_object(ObjectId(40));
        with_context(&world, |ctx| {
            assert_eq!(rows(&panel.frame(ctx)), [ObjectId(100), ObjectId(101)]);
            assert_eq!(
                panel.event(
                    ControlEvent::DragStart {
                        id: "items".into(),
                        index: 1
                    },
                    ctx
                ),
                [PanelAction::BeginDrag(DragPayload::Object(ObjectId(101)))]
            );
        });
        world.contents = vec![ObjectId(102)];
        with_context(&world, |ctx| {
            assert_eq!(rows(&panel.frame(ctx)), [ObjectId(102)]);
            assert!(panel
                .event(
                    ControlEvent::Select {
                        id: "items".into(),
                        index: 1
                    },
                    ctx
                )
                .is_empty());
            assert_eq!(
                panel.event(
                    ControlEvent::Select {
                        id: "items".into(),
                        index: 0
                    },
                    ctx
                ),
                [PanelAction::Game(UiRequest::Select(ObjectId(102)))]
            );
        });
        panel.set_object(ObjectId(41));
        with_context(&world, |ctx| {
            assert_eq!(
                panel.event(click("close"), ctx),
                [PanelAction::Game(UiRequest::CloseExternalContainer(
                    ObjectId(41)
                ))]
            );
            assert!(panel.event(click("close"), ctx).is_empty());
            assert!(
                !panel
                    .frame(ctx)
                    .controls
                    .iter()
                    .find(|c| c.id == "close")
                    .unwrap()
                    .enabled
            );
            panel.set_object(ObjectId(42));
            assert!(
                panel
                    .frame(ctx)
                    .controls
                    .iter()
                    .find(|c| c.id == "close")
                    .unwrap()
                    .enabled
            );
            assert_eq!(
                panel.event(click("close"), ctx),
                [PanelAction::Game(UiRequest::CloseExternalContainer(
                    ObjectId(42)
                ))]
            );
        });
    }
    #[test]
    fn a_shortcut_dragged_off_the_toolbar_lifts_it_from_its_slot() {
        let world = World {
            shortcuts: vec![ObjectId(99)],
            ..Default::default()
        };
        with_context(&world, |ctx| {
            let mut hud = crate::panels::hud::make("hud").unwrap();
            assert_eq!(
                hud.event(
                    ControlEvent::DragStart {
                        id: "shortcut:slot:0".into(),
                        index: 0
                    },
                    ctx
                ),
                vec![
                    PanelAction::Game(UiRequest::RemoveShortcut(ObjectId(99))),
                    PanelAction::BeginDrag(DragPayload::Shortcut {
                        object: ObjectId(99),
                        from: 0
                    }),
                ]
            );
        });
    }
    #[test]
    fn item_right_click_selects_and_examines_in_each_inventory_host() {
        let world = World {
            contents: vec![ObjectId(99)],
            shortcuts: vec![ObjectId(99)],
            ..Default::default()
        };
        with_context(&world, |ctx| {
            for (host, control) in [
                ("inventory", "items"),
                ("external-container", "items"),
                ("shortcuts", "slot:0"),
            ] {
                let mut panel = make(host).unwrap();
                if host == "external-container" {
                    panel.set_object(ObjectId(7));
                }
                assert_eq!(
                    panel.event(
                        ControlEvent::DoubleClick {
                            id: control.into(),
                            index: 0
                        },
                        ctx
                    ),
                    if host == "shortcuts" {
                        vec![]
                    } else {
                        vec![PanelAction::Game(UiRequest::Use(ObjectId(99)))]
                    },
                    "a double click on an item uses it: {host}"
                );
                assert_eq!(
                    panel.event(
                        ControlEvent::RightClick {
                            id: control.into(),
                            index: 0
                        },
                        ctx
                    ),
                    vec![
                        PanelAction::Game(UiRequest::Select(ObjectId(99))),
                        PanelAction::OpenObject {
                            id: "examine".into(),
                            object: ObjectId(99)
                        },
                    ],
                    "{host}"
                );
            }
        });
    }
    #[test]
    fn item_left_click_acts_on_the_armed_cursor_mode() {
        with_context(&World::default(), |ctx| {
            for (mode, expected) in [
                (
                    1,
                    Some(vec![PanelAction::Game(UiRequest::Use(ObjectId(99)))]),
                ),
                (
                    4,
                    Some(vec![PanelAction::Game(UiRequest::ExecuteTargetItem(
                        ObjectId(99),
                    ))]),
                ),
                (0, None),
            ] {
                let classic = ClassicState {
                    cursor_mode: mode,
                    ..Default::default()
                };
                let context = Context {
                    classic: &classic,
                    ..*ctx
                };
                assert_eq!(
                    items::item_click(ObjectId(99), items::Click::Left, &context),
                    expected
                );
            }
        });
    }
    #[test]
    fn inventory_drag_resolves_visible_row_against_scrolled_backpack_contents() {
        let world = World {
            contents: (100..120).map(ObjectId).collect(),
            ..Default::default()
        };
        with_context(&world, |ctx| {
            let mut panel = make("inventory").unwrap();
            panel.event(
                ControlEvent::Scroll {
                    id: "items-scroll".into(),
                    value: 32,
                },
                ctx,
            );
            assert!(matches!(
                panel
                    .event(
                        ControlEvent::DragStart {
                            id: "items".into(),
                            index: 1
                        },
                        ctx
                    )
                    .as_slice(),
                [PanelAction::BeginDrag(DragPayload::Object(ObjectId(107)))]
            ));
        });
    }
    #[test]
    fn paperdoll_pick_prefers_armor_over_clothing_and_examines_on_right_click() {
        let world = World {
            equipment: vec![(ObjectId(10), 4), (ObjectId(11), 0x400)],
            ..Default::default()
        };
        with_context(&world, |ctx| {
            let mut panel = make("inventory").unwrap();
            let actions = panel.event(
                ControlEvent::PreviewHit {
                    object_index: 0,
                    part_index: 0,
                    equipment_mask: 0x404,
                    right_click: true,
                    double_click: false,
                },
                ctx,
            );
            assert!(matches!(
                actions.as_slice(),
                [
                    PanelAction::Game(UiRequest::Select(ObjectId(11))),
                    PanelAction::OpenObject {
                        object: ObjectId(11),
                        ..
                    }
                ]
            ));
            let actions = panel.event(
                ControlEvent::PreviewHit {
                    object_index: 0,
                    part_index: 16,
                    equipment_mask: 1,
                    right_click: false,
                    double_click: false,
                },
                ctx,
            );
            assert!(matches!(
                actions.as_slice(),
                [PanelAction::Game(UiRequest::Select(ObjectId(7)))]
            ));
        });
    }
}
