//! Receive and single-message semantic delivery. UI messages drain
//! before timer/network processing; queue 4 drains before send and queue 5 afterward.
//! Queue 10 is admitted only after maintenance and physics. The host consumes every
//! returned event and its synchronous callbacks before asking for the next message.
use crate::client_session::{dispatch, DropReason, Opcode, Session, SessionEvent};
use dereth_primitives::{IncomingMessage, LocalTime, NetQueue, ObjectId, Transport};
use std::collections::VecDeque;

#[derive(Debug, Default)]
pub(crate) struct IncomingQueues {
    logon: VecDeque<IncomingMessage>,
    cache: VecDeque<IncomingMessage>,
    ui: VecDeque<IncomingMessage>,
    world_objects: VecDeque<IncomingMessage>,
    /// A live ordering-router continuation, not a second stamp admission.
    ui_resume: Option<ObjectId>,
}
impl IncomingQueues {
    pub(crate) fn forget_object(&mut self, id: ObjectId) {
        if self.ui_resume == Some(id) {
            self.ui_resume = None;
        }
        // Incoming queue entries belong to the queue, not the deleted object's parked list.
    }
    pub(crate) fn forget_all_object_resumes(&mut self) {
        self.ui_resume = None;
    }
}

impl<T: Transport> Session<T> {
    /// Receive-only handoff from the transport. No UI stamps, instance gates or model handlers
    /// run here. Raw bytes first received now stay queued until their next owning phase.
    pub fn receive(&mut self, network_now: LocalTime) {
        self.network_now = network_now;
        while let Some(message) = self.transport.poll() {
            match message.queue {
                NetQueue::Logon => self.queues.logon.push_back(message),
                NetQueue::ClientCache => self.queues.cache.push_back(message),
                NetQueue::UiQueue => self.queues.ui.push_back(message),
                NetQueue::WorldObjects => self.queues.world_objects.push_back(message),
                queue => self.events.push(SessionEvent::Dropped {
                    queue,
                    opcode: Opcode(message.opcode),
                    reason: DropReason::NoHandler,
                }),
            }
        }
    }

    /// Existing transport/session timeout domain, deliberately not the simulated UI clock.
    pub fn network_use_time(&mut self, network_now: LocalTime) {
        self.network_now = network_now;
        let mut actions = Vec::new();
        self.flow.tick(network_now, &mut actions);
        self.run_actions(actions);
    }

    /// Process one queue4 message. Its events must be consumed before the next call.
    pub fn process_next_logon(&mut self) -> bool {
        let Some(message) = self.queues.logon.pop_front() else {
            return false;
        };
        self.events.push(dispatch::logon::dispatch(&message));
        true
    }

    /// Database phase after the transport send; an EndDDD response waits for the next send.
    pub fn process_next_cache(&mut self) -> bool {
        let Some(message) = self.queues.cache.pop_front() else {
            return false;
        };
        let result = dispatch::database::dispatch(&mut self.ddd, &message);
        if result.send_end {
            self.send_bare(&dereth_protocol::admin::DddEndDdd);
        }
        self.on_ddd(&result.event);
        self.events.push(result.event);
        true
    }

    /// One UI callback at the supplied source Timer value. At frame entry this is the prior
    /// simulated time; an object-arrival callback later in WorldObjects uses current simulated time.
    pub fn process_next_ui(&mut self, ui_now: LocalTime) -> bool {
        self.now = ui_now;
        if let Some(id) = self.queues.ui_resume {
            if let Some(blob) = self.ui.next_ready(id, ui_now) {
                self.deliver_ui_blob(&blob);
                return true;
            }
            self.queues.ui_resume = None;
        }
        let Some(message) = self.queues.ui.pop_front() else {
            return false;
        };
        let result = self
            .ui
            .route_one(&message, ui_now, &|id| self.weenies.contains(&id));
        self.queues.ui_resume = result.resume;
        self.events.extend(result.events);
        for blob in result.ready {
            self.deliver_ui_blob(&blob);
        }
        true
    }

    /// One authoritative WorldObjects admission, against the owner table AFTER the preceding
    /// message's host work. No coarse-tick recheck or second PropertySequenceGate pass belongs here.
    pub fn process_next_world_object(&mut self, object_now: LocalTime) -> bool {
        self.now = object_now;
        let Some(message) = self.queues.world_objects.pop_front() else {
            return false;
        };
        // Publish current time for the parking step's destruction deadline,
        // published rather than read because this crate has no globals.
        self.parked_world_objects.set_time(object_now.0);
        let result = dispatch::world_objects::dispatch(
            &mut self.instances,
            &mut self.parked_world_objects,
            self.player_id,
            &message,
        );
        if let Some(event) = result.event {
            if let SessionEvent::PlayerCreated(id) = event {
                self.player_id = Some(id);
            }
            self.events.push(event);
        }
        true
    }

    /// First half of the client's object-blob processing. Register the new physics identity, then
    /// drain its Weenie/UI window synchronously before snapshotting its physical parked list.
    pub fn begin_object_arrival(&mut self, id: ObjectId, instance: u16) {
        self.begin_weenie_arrival(id);
        self.instances.set(id, instance);
    }

    /// Weenie creation can succeed while the physical null-object setup fails. This publishes only
    /// the UI-ordering owner; it neither invents a physical instance nor releases its blobs.
    pub fn begin_weenie_arrival(&mut self, id: ObjectId) {
        self.weenies.insert(id);
    }

    pub fn process_next_object_ui(&mut self, id: ObjectId, object_now: LocalTime) -> bool {
        self.now = object_now;
        let Some(blob) = self.ui.next_ready(id, object_now) else {
            return false;
        };
        self.deliver_ui_blob(&blob);
        true
    }

    /// Called only after the Weenie/UI callbacks complete. The client snapshots the physical list
    /// at this point so a refused future entry is reparked, not retried forever in this drain.
    pub fn take_object_message(&mut self, id: ObjectId) -> Vec<Vec<u8>> {
        self.parked_world_objects.release(id)
    }

    pub fn process_object_message(&mut self, blob: &[u8], object_now: LocalTime) {
        self.now = object_now;
        self.deliver_world_object_blob(blob);
    }
}
