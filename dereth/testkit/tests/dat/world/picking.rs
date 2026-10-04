use super::*;

// -------------------------------------------------------------------------------------------
// world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked
// -------------------------------------------------------------------------------------------

/// A click armed by a world drop and answered on a frame with no scene reports nothing picked.
///
/// The pair is the point: arming writes "nothing, and no part either", and the answer has to *move*
/// the second half from that to "nothing".
pub fn a_scene_less_click_reports_nothing_picked() {
    use dereth_client::interaction::{self, Interaction};
    use dereth_primitives::{LocalTime, ObjectId, ServerTime};
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    const PLAYER_ID: ObjectId = ObjectId(0x5000_0449);

    struct Host {
        store: std::sync::Arc<dereth_dat::RetailDatStore>,
        objects: dereth_client::objects::ObjectStream,
        inter: Interaction,
        clock: f64,
    }

    impl Host {
        fn new(store: &std::sync::Arc<dereth_dat::RetailDatStore>) -> Self {
            let mut objects = dereth_client::objects::ObjectStream::new();
            objects.world.player = Some(PLAYER_ID);
            objects.world.tables.inventories.insert(
                PLAYER_ID,
                dereth_client_model::objects::ObjectInventory::new(PLAYER_ID),
            );
            let mut pw = dereth_client_model::Weenie::new(PLAYER_ID);
            pw.pwd.items_capacity = Some(0xFF);
            pw.pwd.containers_capacity = Some(0xFF);
            objects.world.tables.weenies.insert(PLAYER_ID, pw);
            let mut h = Self {
                store: std::sync::Arc::clone(store),
                objects,
                inter: Interaction::new(),
                clock: 1.0,
            };
            // Prime the viewport the pick reads.
            h.drive();
            h
        }

        /// One frame with **no scene at all**, which is the arm under test.
        fn drive(&mut self) {
            self.clock += 1.0;
            let _ = interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                Vec::new(),
                false,
                (1024, 768),
                LocalTime(self.clock),
            );
        }

        fn carry(&mut self, id: ObjectId) -> ObjectId {
            let mut w = dereth_client_model::Weenie::new(id);
            w.pwd.container_id = Some(PLAYER_ID);
            w.pwd.stack_size = Some(1);
            w.pwd.max_stack_size = Some(1);
            w.pwd.name = format!("thing {:X}", id.0);
            w.waiting = true;
            w.determine_position_state();
            self.objects.world.tables.weenies.insert(id, w);
            assert!(
                self.objects.world.is_owned_by_player(id),
                "the fixture must be carried"
            );
            id
        }

        /// Arm a pick the way the production path does: a world drop through the request pump.
        fn arm_a_drop(&mut self, item: ObjectId) {
            self.inter.queue(
                Vec::new(),
                vec![UiRequest::DragDrop {
                    item,
                    target: DropTarget::World,
                }],
            );
            self.clock += 1.0;
            let unowned =
                self.inter
                    .run_ui_requests(&mut self.objects.world, false, ServerTime(self.clock));
            assert!(
                unowned.is_empty(),
                "the drop is this scenario's own arm: {unowned:?}"
            );
            assert!(
                self.inter.pick.looking_for_object(),
                "the pick must be armed"
            );
        }
    }

    let store = store();
    let mut host = Host::new(&store);

    let mut starts_unanswered = host.inter.pick.click_object() == (ObjectId(0), -1);
    let mut moves_to_nothing = true;
    // Twice, so a single lucky frame cannot produce it.
    for round in 0..2 {
        let item = host.carry(ObjectId(0x4490_0001 + round));
        host.arm_a_drop(item);
        starts_unanswered &= host.inter.pick.click_object() == (ObjectId(0), -1);

        let notices = host.inter.stats.scene_less_notices;
        host.drive();
        moves_to_nothing &= host.inter.stats.scene_less_notices == notices + 1
            && host.inter.pick.click_object() == (ObjectId(0), 0)
            && !host.inter.pick.looking_for_object();
    }

    // The known-negative: with nothing armed, the frame answers nothing at all and the pair is
    // back at the value arming writes -- so the zero above is written by the *answer* and not by
    // the default.
    let mut idle = Host::new(&store);
    let notices = idle.inter.stats.scene_less_notices;
    idle.drive();
    let unarmed_is_never_answered = idle.inter.stats.scene_less_notices == notices
        && idle.inter.pick.click_object() == (ObjectId(0), -1)
        && !idle.inter.pick.looking_for_object();
    println!(
        "click armed answers move to nothing: {moves_to_nothing}; an unarmed frame answers \
         nothing at all: {unarmed_is_never_answered}"
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked",
        move |_| starts_unanswered && moves_to_nothing && unarmed_is_never_answered,
    );
}

// -------------------------------------------------------------------------------------------
// world.scene-less-frame.*
// -------------------------------------------------------------------------------------------

/// A drop armed on a frame that drew no world is answered on that frame, and the click after it
/// still works.
///
/// The second half is the one that matters and it is invisible to a single-drop measurement: a
/// reason that stays parked refuses the next viewport press outright.
pub fn a_drop_armed_with_nothing_drawn_is_answered() {
    use dereth_client::interaction::SearchReason;
    use dereth_primitives::{ObjectId, ServerTime};
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);
    let item = host.carry(ObjectId(0x2884_0001));

    // The drop release, which arms the pick and parks its reason.
    host.inter.queue(
        Vec::new(),
        vec![UiRequest::DragDrop {
            item,
            target: DropTarget::World,
        }],
    );
    host.clock += 1.0;
    let unowned =
        host.inter
            .run_ui_requests(&mut host.objects.world, false, ServerTime(host.clock));
    let armed = unowned.is_empty()
        && host.inter.pick.looking_for_object()
        && host.inter.search_reason() == SearchReason::Drop;
    let notices = host.inter.stats.scene_less_notices;

    // The rest of that same frame, with no world drawn.
    host.drive();
    let answered = host.inter.stats.scene_less_notices == notices + 1
        && !host.inter.pick.looking_for_object()
        // The clearing of the reason is the assertion the whole claim turns on.
        && host.inter.search_reason() == SearchReason::None;
    // The drop resolved rather than vanishing: nothing was named, so it took the ground leg,
    // which a client with no body of its own refuses -- and the item comes back out of the drag.
    let resolved = host.lines()
        == vec![(
            world_support::FEEDBACK_CHANNEL,
            world_support::MID_AIR.to_owned(),
        )]
        && !host
            .objects
            .world
            .weenie(item)
            .expect("the item is seeded")
            .waiting;

    // The next viewport press. This is the symptom the whole row exists for.
    let next_click_works =
        host.viewport_left_press() == 1 && host.inter.search_reason() == SearchReason::Select;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.a-drop-armed-with-nothing-drawn-is-answered-and-the-next-click-still-works",
        move |_| armed && answered && resolved && next_click_works,
    );
}

/// A click and a double-click armed on a frame that drew no world are answered too, so the
/// gestures whose own gates they would otherwise block still work.
pub fn a_click_with_nothing_drawn_is_answered_too() {
    use dereth_client::interaction::SearchReason;
    use dereth_client::ui::UiMouseEvent;
    use dereth_ui_screens::screens::gameplay::window;

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);

    let first =
        host.viewport_left_press() == 1 && host.inter.search_reason() == SearchReason::Select;
    host.drive();
    let cleared = host.inter.stats.scene_less_notices == 1
        && host.inter.search_reason() == SearchReason::None;

    // The harder edge: a double-click parks a reason that sits above the examine gate.
    host.inter.wrapper_mouse(
        UiMouseEvent {
            action: 0x0A,
            start: true,
            x: 512,
            y: 384,
            over: Some(window::SMART_BOX),
        },
        (1024, 768),
        true,
    );
    let double = host.inter.search_reason() == SearchReason::Use;
    host.drive();
    let double_cleared = host.inter.stats.scene_less_notices == 2
        && host.inter.search_reason() == SearchReason::None;

    let armed = host.inter.pick.stats.requests;
    // The press first: a release with no press is not a gesture the client can produce.
    for start in [true, false] {
        host.inter.wrapper_mouse(
            UiMouseEvent {
                action: dereth_ui::focus::action::SECONDARY_CLICK,
                start,
                x: 512,
                y: 384,
                over: Some(window::SMART_BOX),
            },
            (1024, 768),
            true,
        );
    }
    let examine = host.inter.pick.stats.requests - armed == 1
        && host.inter.search_reason() == SearchReason::Examine;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.a-click-and-a-double-click-are-answered-so-the-gestures-after-them-still-work",
        move |_| first && cleared && double && double_cleared && examine,
    );
}

/// A frame that drew no world and has nothing armed answers nothing, and leaves a reason that
/// was deliberately parked exactly where it was.
///
/// This is the control for the two above: an arm that always fired would satisfy every "it
/// fired" assertion they make.
pub fn a_frame_with_nothing_armed_answers_nothing() {
    use dereth_client::interaction::SearchReason;
    use dereth_client::ui::UiMouseEvent;
    use dereth_primitives::ObjectId;
    use dereth_ui_screens::screens::gameplay::window;
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);
    let idle_to_start = !host.inter.pick.looking_for_object();

    for _ in 0..5 {
        host.drive();
    }
    let five_idle_frames_raise_nothing = host.inter.stats.scene_less_notices == 0;

    // Now park a reason **without** arming a pick: a drop whose point is outside the viewport.
    // The client latches here too, and this arm must not paper over that.
    let parked = host.carry(ObjectId(0x2884_0020));
    host.inter.queue(
        vec![UiMouseEvent {
            action: 9,
            start: true,
            x: 9000,
            y: 9000,
            over: Some(window::SMART_BOX),
        }],
        vec![UiRequest::DragDrop {
            item: parked,
            target: DropTarget::World,
        }],
    );
    host.drive();
    let rejected = host.inter.pick.stats.outside_viewport == 1;
    let still_nothing = host.inter.stats.scene_less_notices == 0;
    let still_parked = host.inter.search_reason() == SearchReason::Drop;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.a-frame-with-nothing-armed-answers-nothing-and-clears-nothing",
        move |_| {
            idle_to_start
                && five_idle_frames_raise_nothing
                && rejected
                && still_nothing
                && still_parked
        },
    );
}

/// The answer a scene-less frame gives names nothing, so a dropped item takes the ground leg
/// rather than being handed to whatever the last sweep found.
pub fn the_scene_less_answer_names_nothing() {
    use dereth_client::interaction::SearchReason;
    use dereth_primitives::{ObjectId, ServerTime};
    use dereth_ui_screens::view::{DropTarget, UiRequest};

    const CREATURE: ObjectId = ObjectId(0x2884_0031);

    let store = support::store();
    let mut host = world_support::SceneLess::new(&store);

    // A creature in the tables and a previous pick that found it, so that a stale answer has
    // something to be stale with: an answer naming it would have produced a give.
    let mut npc = dereth_client_model::Weenie::new(CREATURE);
    npc.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
    npc.pwd.name = "Ulgrim".to_owned();
    host.objects.world.tables.weenies.insert(CREATURE, npc);
    host.inter
        .on_world_object_found(CREATURE, &mut host.objects.world, ServerTime(host.clock));

    let item = host.carry(ObjectId(0x2884_0030));
    host.inter.queue(
        Vec::new(),
        vec![UiRequest::DragDrop {
            item,
            target: DropTarget::World,
        }],
    );
    host.clock += 1.0;
    let _ = host
        .inter
        .run_ui_requests(&mut host.objects.world, false, ServerTime(host.clock));
    host.drive();

    let answered = host.inter.stats.scene_less_notices == 1;
    let ground_leg = host.lines()
        == vec![(
            world_support::FEEDBACK_CHANNEL,
            world_support::MID_AIR.to_owned(),
        )];
    let cleared = host.inter.search_reason() == SearchReason::None;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.scene-less-frame.the-answer-names-nothing-so-a-dropped-item-takes-the-ground-leg",
        move |_| answered && ground_leg && cleared,
    );
}
