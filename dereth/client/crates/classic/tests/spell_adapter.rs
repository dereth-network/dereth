use super::chat_tests::{batch, fixture};
use dereth_primitives::{EraId, ObjectId};

/// Behaviour: spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "requires retail DATs")]
fn classic_and_absent_interfaces_prune_only_authoritative_unknown_visible_favorites() {
    for no_ui in [false, true] {
        let (mut app, mut shell) = fixture();
        shell.no_ui = no_ui;
        app.probe_mut().hud_mut().era.era = EraId::Infiltration;
        app.probe_mut().objects_mut().world.player_system.spell_tabs[0] = vec![900, 901];
        app.probe_mut().objects_mut().world.player_system.spell_tabs[7] = vec![902];
        batch(&mut app, &mut shell, vec![]);
        assert_eq!(app.objects().world.player_system.spell_tabs[0], [900, 901]);
        assert_eq!(app.interaction().stats.spell_favorites_changed, 0);
        let q = dereth_client_model::Qualities {
            spell_book: Some([(901, Default::default())].into_iter().collect()),
            ..Default::default()
        };
        if no_ui {
            app.probe_mut().objects_mut().world.set_player_desc(q);
            assert!(
                app.objects().world.player_qualities().is_none(),
                "description is parked before its object arrives"
            );
        } else {
            app.probe_mut()
                .objects_mut()
                .world
                .seed_player_desc(ObjectId(1), q);
        }
        app.probe_mut().hud_mut().player_desc_received = true;
        batch(&mut app, &mut shell, vec![]);
        assert_eq!(app.objects().world.player_system.spell_tabs[0], [901]);
        assert_eq!(app.objects().world.player_system.spell_tabs[7], [902]);
        assert_eq!(app.interaction().stats.spell_favorites_changed, 1);
        assert!(
            app.hud().spells.is_empty(),
            "known without metadata remains stored"
        );
        batch(&mut app, &mut shell, vec![]);
        assert_eq!(app.interaction().stats.spell_favorites_changed, 1);
        app.probe_mut()
            .objects_mut()
            .world
            .seed_player_desc(ObjectId(1), Default::default());
        batch(&mut app, &mut shell, vec![]);
        assert!(
            app.objects().world.player_system.spell_tabs[0].is_empty(),
            "an authoritative empty book prunes"
        );
        assert_eq!(app.interaction().stats.spell_favorites_changed, 2);
        app.probe_mut().hud_mut().era.era = EraId::Eor;
        batch(&mut app, &mut shell, vec![]);
        assert!(app.objects().world.player_system.spell_tabs[7].is_empty());
        assert_eq!(app.interaction().stats.spell_favorites_changed, 3);
        batch(&mut app, &mut shell, vec![]);
        assert_eq!(app.interaction().stats.spell_favorites_changed, 3);
        app.shutdown(&mut shell);
    }
}
