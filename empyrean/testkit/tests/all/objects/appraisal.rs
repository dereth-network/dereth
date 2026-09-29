//! ACE: Source/ACE.Server/Network/GameAction/Actions/GameActionIdentifyObject.cs::Handle
//! Appraisal through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

#[cfg(feature = "real-content")]
mod profile_real {
    //! ACE: Source/ACE.Server/Network/GameAction/Actions/GameActionIdentifyObject.cs::Handle
    use crate::support::real_content_bot::real::*;

    /// Appraising (the retail client's Item_Appraise on examine or select) answers with the
    /// object's appraisal (`Player.HandleActionIdentifyObject` -> `ItemSetAppraiseInfo`).
    #[test]
    fn appraise_answers_with_the_objects_profile() {
        use dereth_protocol::objects::{ItemAppraise, ItemSetAppraiseInfo};
        let mut l = create_and_enter();
        let dirk = container::inventory_values(&l.ts.world, l.g)
            .into_iter()
            .find(|i| {
                l.ts.world
                    .objects
                    .get(*i)
                    .is_some_and(|o| o.biota.weenie_class_id == TRAINING_DIRK)
            })
            .expect("the dirk");
        let mark = l.mark();
        l.action(&ItemAppraise {
            target: ObjectId(dirk.full()),
        });
        l.advance(0.5);
        assert_eq!(
            l.since::<ItemSetAppraiseInfo>(mark)
                .iter()
                .map(|a| a.object.0)
                .collect::<Vec<_>>(),
            [dirk.full()],
            "the appraisal reply for the dirk"
        );
    }
}
