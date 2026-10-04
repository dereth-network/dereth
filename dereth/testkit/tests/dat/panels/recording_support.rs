use super::*;

/// The decoded corpus of `session`.
pub(super) fn corpus(session: &str) -> Corpus {
    Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| {
            panic!(
                "the decoded corpus has no scenario {session}; it is generated from the \
                 committed recordings and a missing one is a broken checkout"
            )
        })
}

/// The ordered-event envelope, and the sub-type it carries.
pub(super) const ORDERED_EVENT: u32 = 0xF7B0;
const PLAYER_DESCRIPTION: u32 = 0x0013;

/// Which blob of `session` is the shard's `0x0013` -- **found rather than pinned**, so a
/// re-promoted corpus moves the index instead of reddening an arithmetic nobody reads.
pub(super) fn description_blob(session: &str) -> CorpusBlob {
    corpus(session)
        .blobs
        .into_iter()
        .find(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == ORDERED_EVENT
                && b.payload
                    .get(12..16)
                    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
                    == Some(PLAYER_DESCRIPTION)
        })
        .unwrap_or_else(|| panic!("{session} never reached the shard's character description"))
}

/// A whole client in the world, with `session`'s own character behind the character page.
///
/// The recording is replayed **up to and including** its `0x0013` and stopped there: past it lies
/// the recorded log-off, which empties every panel below, and a count taken after that would be a
/// correct and meaningless zero.
pub(super) fn a_recorded_character(session: &'static str) -> HeadlessClient {
    let end = description_blob(session).idx;
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    // **The identity has to exist before the description arrives.** The shard's `0x0013` unpacks
    // into the player weenie's own record, and the slice replayed below carries no moment at
    // which this client adopts one -- a replay that ran on would reach the recorded log-off first.
    // Without it every quality lands nowhere: the panels draw the shard's numbers (they come
    // through the HUD's own copy) and nothing addressed to the *player* -- a spell landing on an
    // attribute, say -- reaches anything at all.
    {
        let player = ObjectId(0x5000_0001);
        let w = c.world_mut();
        w.player = None;
        w.tables
            .weenies
            .insert(player, dereth_client_model::weenie::Weenie::new(player));
        assert!(w.set_player(player), "the identity is adopted once");
    }
    c.when(Inbound::from_corpus(session, 0..end + 1));
    c.tick(4);
    assert!(
        !c.view().expect_app().hud().skills.is_empty(),
        "{session}'s character description is this scenario's oracle and it carried no skill"
    );
    c
}

/// The character's qualities as the shard sent them, rebuilt from the recorded blob itself rather
/// than read back out of the client -- so the scenario's arithmetic and the panel's cannot share a
/// mistake.
pub(super) fn recorded_qualities(session: &str) -> dereth_client_model::Qualities {
    let d = recorded_description(session);
    let mut q = dereth_client_model::Qualities::default();
    q.apply_ac_qualities(&d.qualities, LocalTime(0.0));
    q
}

/// The shard's own description of the character, as it recorded it.
pub(super) fn recorded_description(
    session: &str,
) -> dereth_protocol::login::LoginPlayerDescription {
    let b = description_blob(session);
    dereth_protocol::read_body(&b.payload[16..])
        .expect("the recorded character description decodes")
}

/// One shipped table, decoded straight out of the dats the client was built over.
pub(super) fn table<T: dereth_assets::Decode>(c: &HeadlessClient, id: u32) -> T {
    use dereth_primitives::AssetSource as _;
    let id = dereth_primitives::DataId(id);
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let bytes = store.read(id).expect("the shipped table is in the dats");
    T::decode_payload(id, &bytes).expect("the shipped table decodes")
}

/// The recording every scenario in this file drives from: the one whose character description
/// carries a full skill list.
pub(super) const SESSION: &str = "first-login-walk-jump";
