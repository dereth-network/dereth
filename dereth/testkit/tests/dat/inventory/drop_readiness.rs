use super::*;

/// The refusal the client prints when a drop would leave something in the air, as a literal --
/// a scenario that read it through the symbol it is written through could not catch a wrong one.
const MID_AIR: &str = "You cannot do that in mid air";
/// The one-thing-at-a-time refusal, likewise verbatim: it is the sentence a player reads.
pub(super) const BUSY: &str = "You can only move or use one item at a time";
/// The channel the client's own inventory feedback goes out on.
pub(super) const FEEDBACK_CHANNEL: u32 = 0x1A;

/// A drop into the world is refused in the air and allowed on the ground, from one body.
pub(super) fn a_world_drop_is_refused_in_mid_air() {
    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut body = a_settled_body(&store);

    // On the ground.
    let mut host = DropHost::new(&store);
    let item = host.carry(ObjectId(0x2883_0001));
    let out = host.drop_onto(item, ObjectId(0), Some(&body));
    let standing = host.inter.player_on_ground() == Some(true)
        && out.len() == 1
        && is_drop(&out[0])
        && host.lines().is_empty();

    // The same body, airborne. **The two arms differ in nothing but the jump**, which is what
    // makes this discriminating rather than a pair of one-sided measurements.
    jump(&mut body, 60);
    let mut host = DropHost::new(&store);
    let item = host.carry(ObjectId(0x2883_0002));
    let out = host.drop_onto(item, ObjectId(0), Some(&body));
    let airborne = host.inter.player_on_ground() == Some(false)
        && out.is_empty()
        && host.lines() == vec![(FEEDBACK_CHANNEL, MID_AIR.to_owned())]
        && !host.objects.world.weenie(item).expect("seeded").waiting
        // The producer moved, rather than having been constant and lucky.
        && host.inter.stats.physics_answers_changed > 0;

    // …and a client with no body at all is treated exactly as one in the air.
    let mut host = DropHost::new(&store);
    let item = host.carry(ObjectId(0x2883_0020));
    let out = host.drop_onto(item, ObjectId(0), None);
    let bodiless = host.inter.player_on_ground().is_none()
        && out.is_empty()
        && host.lines() == vec![(FEEDBACK_CHANNEL, MID_AIR.to_owned())];

    c.assert_behaviour(
        "inventory.world-drop.a-player-in-mid-air-is-refused-and-one-on-the-ground-is-not",
        move |_| standing && airborne && bodiless,
    );
    c.shutdown();
}

/// Handing something over while falling is not refused -- the gate is downstream of that arm.
pub(super) fn a_give_in_mid_air_is_not_refused() {
    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut body = a_settled_body(&store);
    jump(&mut body, 60);

    let mut host = DropHost::new(&store);
    let item = host.carry(ObjectId(0x2883_0010));
    let creature = host.a_creature(ObjectId(0x2883_0011));
    let out = host.drop_onto(item, creature, Some(&body));
    let given = host.inter.player_on_ground() == Some(false)
        && out.len() == 1
        && is_give(&out[0])
        && host.lines().is_empty();

    c.assert_behaviour(
        "inventory.give.handing-something-over-in-mid-air-is-not-refused",
        { move |_| given },
    );
    c.shutdown();
}

/// Whether the player is on the ground is read again every frame, and not knowing is a value.
///
/// **A declared limit**: the call site that reads the body out of the rendered world needs a
/// device, so what is driven here is that the frame's own pass re-reads it at all and that a frame
/// with no world answers that it does not know. A change to *which* field of the body it reads
/// would not redden this, and nothing here claims otherwise.
pub(super) fn the_ground_answer_is_read_again_every_frame() {
    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let body = a_settled_body(&store);
    let mut host = DropHost::new(&store);

    host.inter.note_player_physics(Some(&body));
    let told = host.inter.player_on_ground() == Some(true);
    let changed = host.inter.stats.physics_answers_changed;

    host.frame();
    let re_read = host.inter.player_on_ground().is_none()
        && host.inter.stats.physics_answers_changed == changed + 1;

    c.assert_behaviour(
        "inventory.world-drop.whether-the-player-is-on-the-ground-is-read-again-every-frame",
        move |_| told && re_read,
    );
    c.shutdown();
}
