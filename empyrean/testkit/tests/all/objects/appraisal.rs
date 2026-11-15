//! ACE: Source/ACE.Server/Network/GameAction/Actions/GameActionIdentifyObject.cs::Handle
//! Appraisal through game messages; every player's appraisal carries its Enlightenment (V542).
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

/// Divergence: V542
/// A player appraising another that never enlightened is told its Enlightenment all the same: 0.
#[test]
fn a_players_appraisal_carries_its_enlightenment_even_at_none() {
    use crate::chat::chat_between_players::{three, BRAVO};
    use dereth_primitives::ObjectId;
    use dereth_protocol::objects::{ItemAppraise, ItemSetAppraiseInfo};
    use dereth_protocol::Message as _;
    let mut t = three();
    let from = t.ts.received_raw(t.a).len();
    t.ts.send_game_action(
        t.a,
        &ItemAppraise {
            target: ObjectId(BRAVO),
        },
    );
    t.ts.advance(0.5);
    let appraisals: Vec<ItemSetAppraiseInfo> = t.ts.received_raw(t.a)[from..]
        .iter()
        .filter(|m| m.opcode == 0xF7B0)
        .filter(|m| u32::from_le_bytes(m.body[8..12].try_into().unwrap()) == 0x00C9)
        .map(|m| {
            ItemSetAppraiseInfo::read(&mut dereth_protocol::Reader::new(&m.body[12..]))
                .expect("the appraisal decodes")
        })
        .collect();
    assert_eq!(appraisals.len(), 1);
    let ints = appraisals[0]
        .profile
        .tables
        .ints
        .as_ref()
        .expect("the ints");
    assert!(
        ints.entries.contains(&(390, 0)),
        "Enlightenment 0 among {:?}",
        ints.entries
    );
    let _ = (t.b, t.c);
}
