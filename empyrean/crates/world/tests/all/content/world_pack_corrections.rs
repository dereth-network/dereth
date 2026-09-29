//! Divergence: V335, V337, V338
//! Tests of world pack corrections.
//! Fixture: isolated world state and the shared area fixtures.

#[cfg(feature = "real-content")]
mod recorded_content_real {
    mod real_content {
        use crate::support::creature_world::real_content::*;

        /// Every recorded script shift is read without changing the pack.
        #[test]
        fn every_recorded_script_shift_is_read_without_changing_the_pack() {
            use empyrean_content::corrections::{
                play_script_shift, play_script_shifts, PlayScriptShift,
            };
            use empyrean_entity::enums::PropertyDataId;

            let db = pack();
            let shifts = play_script_shifts(db.base());
            assert!(!shifts.is_empty(), "the pack exercises script corrections");
            for change in &shifts {
                assert_eq!(change.corrected, change.stored + 1, "{change:?}");
                let read = db
                    .get_cached_weenie(change.weenie_class_id)
                    .expect("in the pack");
                assert_eq!(
                    read.properties_did
                        .as_ref()
                        .and_then(|d| d.get(&PropertyDataId::PhysicsScript).copied()),
                    Some(change.corrected),
                    "{change:?}"
                );
                let stored = db
                    .base()
                    .get_stored_weenie(change.weenie_class_id)
                    .expect("in the pack");
                let script = stored
                    .weenie_properties_did
                    .iter()
                    .find(|d| d.r#type == PropertyDataId::PhysicsScript.0)
                    .map(|d| d.value);
                assert_eq!(
                    script,
                    Some(change.stored),
                    "the pack retains the original: {change:?}"
                );
            }
            let script = |wcid: u32| {
                let w = db
                    .get_cached_weenie(wcid)
                    .unwrap_or_else(|| panic!("{wcid} is in the pack"));
                w.properties_did
                    .as_ref()
                    .and_then(|d| d.get(&PropertyDataId::PhysicsScript).copied())
            };
            for (wcid, stored, retail) in [
                (301, 87, 88),
                (24557, 87, 88),
                (1499, 89, 90),
                (3768, 83, 84),
            ] {
                assert!(
                    shifts.contains(&PlayScriptShift {
                        weenie_class_id: wcid,
                        stored,
                        corrected: retail
                    }),
                    "{wcid}"
                );
                assert_eq!(script(wcid), Some(retail), "{wcid} reads retail's value");
                let kept = db.base().get_stored_weenie(wcid).expect("in the pack");
                assert_eq!(
                    play_script_shift(&kept),
                    Some(retail),
                    "{wcid}: the pack keeps ACE's value"
                );
            }
            for (wcid, value) in [
                (15871, 88),
                (49119, 87),
                (7982, 87),
                (1605, 84),
                (33527, 90),
            ] {
                assert!(
                    shifts.iter().all(|c| c.weenie_class_id != wcid),
                    "{wcid} is left alone"
                );
                assert_eq!(script(wcid), Some(value), "{wcid}");
            }
        }

        /// Every recorded emote correction is applied and other fields are preserved.
        #[test]
        fn every_recorded_emote_correction_is_applied_and_other_fields_are_preserved() {
            use empyrean_content::corrections::{
                emote_motion_shifts, stale_corrections, EmoteMotionField as F, EmoteMotionShift,
                Value, WEENIE_CORRECTIONS,
            };
            use empyrean_entity::enums::{MotionCommand as Mc, PlayScript, PropertyDataId};

            let db = pack();
            let shift = |weenie_class_id, emote_set, field, stored, corrected| EmoteMotionShift {
                weenie_class_id,
                emote_set,
                field,
                stored,
                corrected,
            };
            let changes = emote_motion_shifts(db.base());
            for expected in [
                shift(10980, 72547, F::Motion(0), 0x4300_0119, Mc::MeditateState.0),
                shift(10980, 72548, F::Substyle, 0x4300_0119, Mc::MeditateState.0),
                shift(24588, 89413, F::Motion(1), 0x4300_0117, Mc::CurtseyState.0),
                shift(25682, 49419, F::Motion(0), 0x1300_0116, Mc::WarmHands.0),
            ] {
                assert!(changes.contains(&expected), "{expected:?}");
            }

            let read = |wcid: u32| {
                db.get_weenie(wcid)
                    .unwrap_or_else(|| panic!("{wcid} is in the pack"))
            };
            let set = |w: &empyrean_content::models::world::Weenie, id: u32| {
                w.weenie_properties_emote
                    .iter()
                    .find(|e| e.id == id)
                    .cloned()
                    .unwrap_or_else(|| panic!("set {id}"))
            };
            let motion = |wcid: u32, id: u32, order: u32| {
                set(&read(wcid), id)
                    .weenie_properties_emote_action
                    .iter()
                    .find(|a| a.order == order)
                    .and_then(|a| a.motion)
            };
            let script = |wcid: u32, id: u32, order: u32| {
                set(&read(wcid), id)
                    .weenie_properties_emote_action
                    .iter()
                    .find(|a| a.order == order)
                    .and_then(|a| a.p_script)
            };
            for change in &changes {
                let row = set(&read(change.weenie_class_id), change.emote_set);
                let observed = match change.field {
                    F::Style => row.style,
                    F::Substyle => row.substyle,
                    F::Motion(order) => row
                        .weenie_properties_emote_action
                        .iter()
                        .find(|a| a.order == order)
                        .and_then(|a| a.motion),
                };
                assert_eq!(change.corrected, change.stored + 3, "{change:?}");
                assert_eq!(observed, Some(change.corrected), "{change:?}");
            }
            // changed
            assert_eq!(motion(25682, 49419, 0), Some(Mc::WarmHands.0));
            assert_eq!(motion(24588, 89413, 1), Some(Mc::CurtseyState.0));
            assert_eq!(motion(10980, 72547, 0), Some(Mc::MeditateState.0));
            assert_eq!(set(&read(10980), 72548).substyle, Some(Mc::MeditateState.0));
            // unchanged
            let noir = set(&read(25715), 49450);
            let noir: Vec<u32> = noir
                .weenie_properties_emote_action
                .iter()
                .filter_map(|a| a.motion)
                .collect();
            assert!(
                noir.contains(&Mc::NudgeLeft.0) && noir.contains(&Mc::SitState.0),
                "{noir:#X?}"
            );
            assert_eq!(motion(25715, 49450, 19), Some(Mc::NudgeLeft.0));
            assert_eq!(motion(24578, 729, 0), Some(Mc::ScanHorizon.0));
            assert_eq!(motion(8423, 69877, 0), Some(Mc::MeditateState.0));
            assert_eq!(motion(5772, 7956, 0), Some(Mc::DrudgeDance.0));
            assert_eq!(script(38947, 35844, 2), Some(138));
            assert_eq!(script(40091, 77622, 1), Some(138));
            // Restriction effects retain every value stored in the pack.
            for corrected in db.get_all_weenies() {
                let stored = db
                    .base()
                    .get_stored_weenie(corrected.class_id)
                    .expect("in the pack");
                let restriction = |w: &empyrean_content::models::world::Weenie| {
                    w.weenie_properties_did
                        .iter()
                        .find(|r| r.r#type == PropertyDataId::RestrictionEffect.0)
                        .map(|r| r.value)
                };
                assert_eq!(
                    restriction(&read(stored.class_id)),
                    restriction(&stored),
                    "{}",
                    stored.class_id
                );
            }

            let taunts: Vec<_> = WEENIE_CORRECTIONS
                .iter()
                .filter(|c| matches!(c.stored, Value::EmoteScript(..)))
                .collect();
            assert!(
                !taunts.is_empty(),
                "the correction table exercises taunt scripts"
            );
            for c in taunts {
                let Value::EmoteScript(a, Some(161)) = c.stored else {
                    panic!("{c:?}")
                };
                let kept = db
                    .base()
                    .get_stored_weenie(c.weenie_class_id)
                    .expect("in the pack");
                assert!(
                    stale_corrections(&kept).is_empty(),
                    "{c:?} no longer matches the pack"
                );
                assert_eq!(
                    script(c.weenie_class_id, a.emote_set, a.order),
                    Some(138),
                    "{c:?}"
                );
                let cached = db
                    .get_cached_weenie(c.weenie_class_id)
                    .expect("in the pack");
                let taunt = cached
                    .properties_emote
                    .as_ref()
                    .unwrap()
                    .iter()
                    .find(|e| e.category == empyrean_entity::enums::EmoteCategory::Taunt)
                    .expect("a Taunt set");
                assert_eq!(
                    taunt.properties_emote_action[0].p_script,
                    Some(PlayScript::LevelUp),
                    "{c:?}"
                );
            }
            // One of the family retail was not seen taunting with keeps ACE's value.
            assert_eq!(script(52519, 85959, 0), Some(161));
        }

        /// Every recorded correction applies and is reported.
        #[test]
        fn every_recorded_correction_applies_and_is_reported() {
            use empyrean_content::corrections::report::{CorrectionsReport, EntryState};
            use empyrean_content::corrections::{digest, SPELL_CORRECTIONS, WEENIE_CORRECTIONS};

            let db = pack();
            let r = CorrectionsReport::of(db.base());
            let s = r.summary();
            let entries = WEENIE_CORRECTIONS.len() + SPELL_CORRECTIONS.len();
            assert_eq!(
                (s.entries, s.applies, s.stale, s.absent),
                (entries, entries, 0, 0),
                "{}",
                r.render()
            );
            assert_eq!(
                (s.play_script_shifts, s.emote_motion_shifts),
                (r.play_script_shifts.len(), r.emote_motion_shifts.len())
            );
            for entry in &r.weenie_entries {
                assert_eq!(entry.state, EntryState::Applies, "{:?}", entry.correction);
            }
            assert_eq!(r.spell_entries.len(), SPELL_CORRECTIONS.len());
            for entry in &r.spell_entries {
                assert_eq!(entry.state, EntryState::Applies, "{:?}", entry.correction);
            }
            assert_eq!(r.digest, digest());
            let text = r.render();
            assert!(text.contains("\nevery entry applies\n"), "{text}");
            assert!(text.contains("  301 axebattle (Battle Axe): PhysicsScript 87 (BreatheLightning) -> 88 (Create)\n"), "{text}");
        }

        /// V327 on real content (V327): the Brewmaster's Bible pieces, Tusker Spit and the Healing
        /// Machine Base are created with the ItemType retail gave them
        /// in the retail captures, and the pieces
        /// retail made CraftCookingBase still count as crafting ingredients when dropped on death.
        #[test]
        fn corrected_items_are_created_with_retails_item_type() {
            use empyrean_entity::enums::ItemType;
            use empyrean_world::entity::death_item::{DeathItem, DeathItemCategory};

            let h = world();
            let cases = [
                (29065, ItemType::CraftCookingBase), // Healing Machine Base
                (29204, ItemType::CraftCookingBase), // Tusker Spit
                (29205, ItemType::Misc),             // Brewmaster's Front Cover
                (29206, ItemType::Misc),             // Brewmaster's Back Cover
                (29207, ItemType::Misc),             // Brewmaster's Pages
                (29208, ItemType::CraftCookingBase), // Brewmaster's Spine
                (29209, ItemType::CraftCookingBase), // Incomplete Brewmaster's Bible
                (29210, ItemType::CraftCookingBase), // Nearly Complete Brewmaster's Bible
            ];
            for (wcid, retail) in cases {
                let g = ObjectGuid::new(0x8000_0000 | wcid);
                let o = CtorEnv::with_world(&h.w, |env| {
                    factory::create_new_world_object_by_wcid(env, wcid, g)
                })
                .expect("the weenie");
                assert_eq!(o.item_type(), retail, "{wcid}");
                let category = if retail == ItemType::Misc {
                    DeathItemCategory::Misc
                } else {
                    DeathItemCategory::CraftingIngredient
                };
                assert_eq!(DeathItem::get_category(o.item_type()), category, "{wcid}");
            }
        }
    }
}
