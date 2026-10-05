use super::*;

/// The middle of the landblock the headless client starts on.
const SPAWN: (f32, f32) = (96.0, 96.0);

// ---------------------------------------------------------------------------------------------
// o417: the ground under a world drop.
// ---------------------------------------------------------------------------------------------

/// A body standing still on the retail terrain, settled for two seconds of frames.
pub(super) fn a_settled_body(store: &Arc<dereth_dat::RetailDatStore>) -> Character {
    let region = load_region(store).expect("the region decodes");
    let mut c = Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN).expect("a body");
    for i in 1..=60 {
        c.update(dereth_primitives::LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the premise: the body settles before anything is measured"
    );
    c
}

/// The same body, in the air -- through the body's own input, not by writing a bit.
pub(super) fn jump(c: &mut Character, from_frame: u32) {
    let mut f = from_frame;
    c.input = CharacterInput {
        jump: true,
        ..CharacterInput::default()
    };
    f += 1;
    c.update(dereth_primitives::LocalTime(f64::from(f) / 30.0));
    c.input = CharacterInput::default();
    for _ in 0..2 {
        f += 1;
        c.update(dereth_primitives::LocalTime(f64::from(f) / 30.0));
    }
    assert!(
        !c.on_ground(),
        "the premise: the jump took the body off the ground"
    );
}

/// A player and the things this pair of scenarios drops, in a world of their own.
///
/// It is not the scenario client's world because the gesture has to be driven with a body the
/// scenario made and put into a jump, and the client's own body is the one its frame simulates.
pub(super) struct DropHost {
    store: Arc<dereth_dat::RetailDatStore>,
    pub(super) objects: ObjectStream,
    pub(super) inter: Interaction,
    pub(super) player: ObjectId,
    clock: f64,
}

impl DropHost {
    pub(super) fn new(store: &Arc<dereth_dat::RetailDatStore>) -> Self {
        let mut objects = ObjectStream::new();
        let player = ObjectId(0x5000_0417);
        objects.world.player = Some(player);
        objects.world.tables.inventories.insert(
            player,
            dereth_client_model::objects::ObjectInventory::new(player),
        );
        let mut pw = dereth_client_model::Weenie::new(player);
        pw.pwd.items_capacity = Some(0xFF);
        pw.pwd.containers_capacity = Some(0xFF);
        objects.world.tables.weenies.insert(player, pw);
        let mut h = Self {
            store: Arc::clone(store),
            objects,
            inter: Interaction::new(),
            player,
            clock: 1.0,
        };
        h.frame();
        h
    }

    /// One frame of the client's own use-time pass, which is what reads the body every frame.
    pub(super) fn frame(&mut self) {
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
            dereth_primitives::LocalTime(self.clock),
        );
    }

    pub(super) fn carry(&mut self, id: ObjectId) -> ObjectId {
        let mut w = dereth_client_model::Weenie::new(id);
        w.pwd.container_id = Some(self.player);
        w.pwd.stack_size = Some(1);
        w.pwd.max_stack_size = Some(1);
        w.pwd.name = format!("thing {:X}", id.0);
        w.waiting = true;
        w.determine_position_state();
        self.objects.world.tables.weenies.insert(id, w);
        assert!(
            self.objects.world.is_owned_by_player(id),
            "the premise: it is being carried"
        );
        id
    }

    pub(super) fn a_creature(&mut self, id: ObjectId) -> ObjectId {
        let mut w = dereth_client_model::Weenie::new(id);
        w.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
        w.pwd.name = "Ulgrim".to_owned();
        self.objects.world.tables.weenies.insert(id, w);
        id
    }

    /// The whole gesture, with the body handed to the production producer at the point the
    /// frame's own pass hands it over.
    pub(super) fn drop_onto(
        &mut self,
        item: ObjectId,
        found: ObjectId,
        body: Option<&Character>,
    ) -> Vec<dereth_client_model::Request> {
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
        assert!(unowned.is_empty(), "the world-drop arm owns the gesture");
        assert!(
            self.inter.pick.looking_for_object(),
            "the premise: the pick is armed"
        );
        assert!(
            self.inter.pending_requests().is_empty(),
            "the premise: nothing is built before the answer"
        );

        self.clock += 1.0;
        self.inter.note_player_physics(body);
        self.inter
            .on_world_object_found(found, &mut self.objects.world, ServerTime(self.clock));
        self.inter.pending_requests().to_vec()
    }

    pub(super) fn lines(&self) -> Vec<(u32, String)> {
        self.objects
            .world
            .scroll
            .pending()
            .iter()
            .map(|f| (f.chat_type, f.body.clone()))
            .collect()
    }
}

pub(super) fn is_drop(r: &dereth_client_model::Request) -> bool {
    matches!(r, dereth_client_model::Request::DropItem(_))
}

pub(super) fn is_give(r: &dereth_client_model::Request) -> bool {
    matches!(r, dereth_client_model::Request::GiveObjectRequest(_))
}

// ---------------------------------------------------------------------------------------------
// The hold, driven on the host built in this file for the world drop.
// ---------------------------------------------------------------------------------------------

impl DropHost {
    /// What the client's one inventory hold is holding.
    pub(super) fn lock(&self) -> (InventoryRequest, Option<ObjectId>) {
        (
            self.objects.world.request_lock.pending,
            self.objects.world.request_lock.object,
        )
    }

    /// The shard's own answer, through the client's own apply path.
    pub(super) fn shard_says(&mut self, opcode: dereth_protocol::Opcode, blob: Vec<u8>) {
        interaction::apply_events(
            &mut self.inter,
            &[dereth_client_net::client_session::SessionEvent::UiEvent { opcode, blob }],
            &mut self.objects.world,
        );
    }

    /// A carried stack of `stack`.
    pub(super) fn carry_stack(&mut self, id: ObjectId, stack: u16) -> ObjectId {
        let id = self.carry(id);
        let w = self.objects.world.weenie_mut(id).expect("just seeded");
        w.pwd.stack_size = Some(stack);
        w.pwd.max_stack_size = Some(stack.max(1));
        id
    }

    /// The whole gesture, with the outbox cleared first so that what comes back is **this**
    /// gesture's and not the previous one's.
    pub(super) fn give(
        &mut self,
        item: ObjectId,
        to: ObjectId,
    ) -> Vec<dereth_client_model::Request> {
        let _ = self.inter.take_pending_requests();
        self.drop_onto(item, to, None)
    }

    /// Open a container lying in the world the way a use does.
    pub(super) fn open_ground_container(&mut self, id: ObjectId) {
        let mut req = dereth_client_model::RecordingRequests::default();
        let mut out = dereth_client_model::RecordingSink::default();
        self.clock += 1.0;
        self.objects.world.use_object(
            &mut req,
            &mut out,
            id,
            dereth_client_model::inventory::SplitState::default(),
            ServerTime(self.clock),
        );
    }
}

/// A host with a recipient in it.
pub(super) fn a_host_with_a_recipient(
    store: &Arc<dereth_dat::RetailDatStore>,
    to: ObjectId,
) -> DropHost {
    let mut host = DropHost::new(store);
    host.a_creature(to);
    host
}
