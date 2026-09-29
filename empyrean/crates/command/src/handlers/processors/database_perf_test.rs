// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/Processors/DatabasePerfTest.cs
//! Port of `Source/ACE.Server/Command/Handlers/Processors/DatabasePerfTest.cs`.
//!
//! - DIVERGE (arch): ACE runs the test on a thread-pool task that sleeps until each database
//!   callback arrives. Here it is a chain of world-thread steps: the queue wait time's callback
//!   (from the database thread, as in ACE) continues the run.
//! - ACE's biota list is never filled (its `biotas.Add` lines are commented out, "TODO fix this"),
//!   so every stage reports 0/0; ACE's bulk add waited for a callback that never came (see
//!   [`DatabasePerfTest::run`]).

use empyrean_common::dotnet::{align, format, TimeSpan};
use empyrean_entity::enums::ChatMessageType;
use empyrean_net::{SessionId, SessionState};
use empyrean_store::models::shard::{
    Biota, BiotaPropertiesBool, BiotaPropertiesDID, BiotaPropertiesFloat, BiotaPropertiesIID,
    BiotaPropertiesInt, BiotaPropertiesInt64, BiotaPropertiesString,
};
use empyrean_world::factories::world_object_factory;
use empyrean_world::World;

use crate::handlers::command_handler_helper;

// ACE: DatabasePerfTest
/// The shard database performance test (`databaseperftest`).
#[derive(Debug, Clone)]
pub struct DatabasePerfTest {
    /// `testWeenies`.
    test_weenies: Vec<u32>,
}

impl Default for DatabasePerfTest {
    fn default() -> Self {
        Self {
            test_weenies: vec![
                1,     // Clay
                9035,  // Exarch Plate Girth
                9034,  // Exarch Plate Coat
                27361, // Palenqual's Ukira of the Vortex
                29947, // Bracelet of Creature Enchantments
            ],
        }
    }
}

/// `DefaultBiotasTestCount`.
pub const DEFAULT_BIOTAS_TEST_COUNT: i32 = 1000;

impl DatabasePerfTest {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: DatabasePerfTest.SessionIsStillInWorld
    fn session_is_still_in_world(w: &World, session: SessionId) -> bool {
        w.sessions
            .get(session)
            .is_some_and(|s| s.state == SessionState::WorldConnected && s.player.is_some())
    }

    // ACE: DatabasePerfTest.RunAsync
    /// `Task.Run(() => Run(session, biotasPerTest))`: starts the run (see the module docs).
    pub fn run_async(self, w: &mut World, session: Option<SessionId>, biotas_per_test: i32) {
        self.run(w, session, biotas_per_test);
    }

    /// `WorldObjectFactory.CreateNewWorldObject(testWeenies[i % testWeenies.Count])`, `count`
    /// times; ACE discards each object (its biota was to be collected, "TODO fix this").
    fn generate_world_objects(&self, w: &mut World, count: i32) {
        for i in 0..count {
            let wcid = self.test_weenies[usize::try_from(i).unwrap_or(0) % self.test_weenies.len()];
            let _world_object =
                world_object_factory::create_new_world_object_by_wcid_in_world(w, wcid);
            // TODO fix this
            //biotas.Add((worldObject.Biota, worldObject.BiotaDatabaseLock));
        }
    }

    // ACE: DatabasePerfTest.Run
    fn run(self, w: &mut World, session: Option<SessionId>, biotas_per_test: i32) {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!(
                "Starting Shard Database Performance Tests.\nBiotas per test: {biotas_per_test}\nThis may take several minutes to complete...\nCurrent database queue count: {}",
                w.shard.queue_count()
            ),
            ChatMessageType::Broadcast,
        );

        // Get the current queue wait time
        w.shard.get_current_queue_wait_time(Some(Box::new(
            move |w: &mut World, result: TimeSpan| {
                command_handler_helper::write_output_info(
                    w,
                    session,
                    &format!(
                        "Current database queue wait time: {} ms",
                        format(result.total_milliseconds(), "N0")
                    ),
                    ChatMessageType::Broadcast,
                );
                self.run_after_queue_wait_time(w, session, biotas_per_test);
            },
        )));
    }

    /// The rest of `Run`, once the queue wait time has arrived.
    fn run_after_queue_wait_time(
        &self,
        w: &mut World,
        session: Option<SessionId>,
        biotas_per_test: i32,
    ) {
        // Generate Individual WorldObjects
        let mut biotas: Vec<Biota> = Vec::new();

        self.generate_world_objects(w, biotas_per_test);

        // Add biotasPerTest biotas individually
        let true_results: i64 = 0;
        let false_results: i64 = 0;
        let start_time = w.now.utc;
        let initial_queue_wait_time = TimeSpan::ZERO;
        let total_query_execution_time = TimeSpan::ZERO;
        // foreach (var biota in biotas) { /* todo DatabaseManager.Shard.SaveBiota(...) */ }

        // (the wait for trueResults + falseResults to reach biotas.Count: the list is empty)
        let end_time = w.now.utc;
        Self::report_result(
            w,
            session,
            "individual add",
            biotas_per_test,
            end_time - start_time,
            initial_queue_wait_time,
            total_query_execution_time,
            true_results,
            false_results,
        );

        // Update biotasPerTest biotas individually
        if session.is_none_or(|s| Self::session_is_still_in_world(w, s)) {
            Self::modify_biotas(&mut biotas);

            let start_time = w.now.utc;
            // foreach (var biota in biotas) { /* todo DatabaseManager.Shard.SaveBiota(...) */ }

            let end_time = w.now.utc;
            Self::report_result(
                w,
                session,
                "individual save",
                biotas_per_test,
                end_time - start_time,
                TimeSpan::ZERO,
                TimeSpan::ZERO,
                0,
                0,
            );
        }

        // Delete biotasPerTest biotas individually
        let start_time = w.now.utc;

        // foreach (var biota in biotas) DatabaseManager.Shard.RemoveBiota(biota.biota.Id, ...): the list is empty

        let end_time = w.now.utc;
        Self::report_result(
            w,
            session,
            "individual remove",
            biotas_per_test,
            end_time - start_time,
            TimeSpan::ZERO,
            TimeSpan::ZERO,
            0,
            0,
        );

        if session.is_some_and(|s| !Self::session_is_still_in_world(w, s)) {
            return;
        }

        // Generate Bulk WorldObjects
        biotas.clear();

        self.generate_world_objects(w, biotas_per_test);

        // Add biotasPerTest biotas in bulk
        // Not ACE's (a fix): the bulk stages run over the same empty
        // list as the individual ones, so each reports 0/0 and the run completes. ACE's bulk save
        // is commented out ("todo"), so nothing ever counted a result and its wait for one spun
        // forever: its task never reported "bulk add", the bulk save and remove, or "Database
        // Performance Tests Completed", and held a thread-pool thread.
        let start_time = w.now.utc;
        let end_time = w.now.utc;
        Self::report_result(
            w,
            session,
            "bulk add",
            biotas_per_test,
            end_time - start_time,
            TimeSpan::ZERO,
            TimeSpan::ZERO,
            0,
            0,
        );

        // Update biotasPerTest biotas in bulk
        if session.is_none_or(|s| Self::session_is_still_in_world(w, s)) {
            Self::modify_biotas(&mut biotas);
            let start_time = w.now.utc;
            let end_time = w.now.utc;
            Self::report_result(
                w,
                session,
                "bulk save",
                biotas_per_test,
                end_time - start_time,
                TimeSpan::ZERO,
                TimeSpan::ZERO,
                0,
                0,
            );
        }

        // Delete biotasPerTest biotas in bulk (no ids)
        let start_time = w.now.utc;
        let end_time = w.now.utc;
        Self::report_result(
            w,
            session,
            "bulk remove",
            biotas_per_test,
            end_time - start_time,
            TimeSpan::ZERO,
            TimeSpan::ZERO,
            0,
            0,
        );

        if session.is_some_and(|s| !Self::session_is_still_in_world(w, s)) {
            return;
        }

        command_handler_helper::write_output_info(
            w,
            session,
            "Database Performance Tests Completed",
            ChatMessageType::Broadcast,
        );
    }

    // ACE: DatabasePerfTest.ModifyBiotas
    /// Changes the first row, removes the last row and adds a new row (type `ushort.MaxValue`) of
    /// each of the int, int64, IID, DID, float, bool and string tables of each biota.
    pub fn modify_biotas(biotas: &mut [Biota]) {
        for biota in biotas {
            let id = biota.id;

            // Change the first record
            if let Some(first) = biota.biota_properties_int.first_mut() {
                first.value = first.value.wrapping_add(1);
            }

            if let Some(first) = biota.biota_properties_int64.first_mut() {
                first.value = first.value.wrapping_add(1);
            }

            if let Some(first) = biota.biota_properties_iid.first_mut() {
                first.value = first.value.wrapping_add(1);
            }

            if let Some(first) = biota.biota_properties_did.first_mut() {
                first.value = first.value.wrapping_add(1);
            }

            if let Some(first) = biota.biota_properties_float.first_mut() {
                first.value += 1.0;
            }

            if let Some(first) = biota.biota_properties_bool.first_mut() {
                first.value = !first.value;
            }

            if let Some(first) = biota.biota_properties_string.first_mut() {
                first.value += " test";
            }

            // Remove the last record
            biota.biota_properties_int.pop();

            biota.biota_properties_int64.pop();

            biota.biota_properties_iid.pop();

            biota.biota_properties_did.pop();

            biota.biota_properties_float.pop();

            biota.biota_properties_bool.pop();

            biota.biota_properties_string.pop();

            // Add a new record
            biota.biota_properties_int.push(BiotaPropertiesInt {
                object_id: id,
                r#type: u16::MAX,
                value: 0,
            });

            biota.biota_properties_int64.push(BiotaPropertiesInt64 {
                object_id: id,
                r#type: u16::MAX,
                value: 0,
            });

            biota.biota_properties_iid.push(BiotaPropertiesIID {
                object_id: id,
                r#type: u16::MAX,
                value: 0,
            });

            biota.biota_properties_did.push(BiotaPropertiesDID {
                object_id: id,
                r#type: u16::MAX,
                value: 0,
            });

            biota.biota_properties_float.push(BiotaPropertiesFloat {
                object_id: id,
                r#type: u16::MAX,
                value: 0.0,
            });

            biota.biota_properties_bool.push(BiotaPropertiesBool {
                object_id: id,
                r#type: u16::MAX,
                value: false,
            });

            biota.biota_properties_string.push(BiotaPropertiesString {
                object_id: id,
                r#type: u16::MAX,
                value: String::new(),
            });
        }
    }

    // ACE: DatabasePerfTest.ReportResult
    #[allow(clippy::too_many_arguments)]
    fn report_result(
        w: &mut World,
        session: Option<SessionId>,
        test_description: &str,
        biotas_per_test: i32,
        duration: TimeSpan,
        queue_wait_time: TimeSpan,
        total_query_execution_time: TimeSpan,
        true_results: i64,
        false_results: i64,
    ) {
        if session.is_some_and(|s| !Self::session_is_still_in_world(w, s)) {
            return;
        }

        command_handler_helper::write_output_info(
            w,
            session,
            &report_text(
                test_description,
                biotas_per_test,
                duration,
                queue_wait_time,
                total_query_execution_time,
                true_results,
                false_results,
            ),
            ChatMessageType::System,
        );
    }
}

/// `ReportResult`'s line.
#[must_use]
pub fn report_text(
    test_description: &str,
    biotas_per_test: i32,
    duration: TimeSpan,
    queue_wait_time: TimeSpan,
    total_query_execution_time: TimeSpan,
    true_results: i64,
    false_results: i64,
) -> String {
    format!(
        "{biotas_per_test} {} Duration: {} s. Queue Wait Time: {} ms. Average Execution Time: {} ms. Success/Fail: {true_results}/{false_results}.",
        align(test_description, -17),
        align(&format(duration.total_seconds(), "N1"), 5),
        align(&format(queue_wait_time.total_milliseconds(), "N0"), 3),
        align(&format(total_query_execution_time.total_milliseconds() / f64::from(biotas_per_test), "N0"), 3),
    )
}
