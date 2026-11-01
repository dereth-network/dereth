//! The script machine: the compiled library and its load pass, packed object graphs (saved states
//! and call arguments), the instruction set, the reference table, natives and script calls across
//! the connection.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "script.args.every-captured-call-round-trips",
        says: "Every script call recorded in 2013 and 2017, either way, carries a buffer of tagged \
               values that reads to its last byte, writes back to the same bytes, and holds exactly \
               the called function's declared parameters, last first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ARGS"),
        station: "auberean-script::corpus::args::every_relaunch_script_call_reads_exactly_writes_back_and_matches_its_parameters",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.args.values-in-buffer-order",
        says: "An argument buffer is a run of tagged values (integers, long integers, floats, long \
               floats, packed objects) in order, and a tag it does not know refuses the whole \
               buffer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ARG-TAGS"),
        station: "auberean-script::cpu::codec::an_argument_buffer_holds_tagged_values_in_order_and_refuses_an_unknown_tag",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.graph.a-saved-state-that-fails-to-read-once-is-read-again",
        says: "A saved script state whose read fails once is read again and kept; only a state that fails every time it is read counts as one that would not read.",
        since: THIS_CLIENT,
        divergence: "CD2-data-read-again",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-READ-AGAIN"),
        station: "auberean-script::dat::states::a_saved_state_whose_first_read_fails_is_read_again_not_remembered_absent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.graph.behavior-params-pack-what-differs",
        says: "A behavior's parameters pack a word of bits naming the fields that differ from a new one's (a time scale of 1, one hold cycle, an event sent when done, the rest zero; the impulse within 0.0002), then those fields in a fixed order; read over a new one they come back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-BEHAVIOR-PARAMS"),
        station: "auberean-script::cpu::codec::behavior_parameters_pack_only_what_differs_from_a_new_ones",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.graph.every-saved-state-round-trips",
        says: "Every saved script state's object graph reads to its last byte, every script object \
               the size its package declares, and writes back to the same bytes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-GRAPH"),
        station: "auberean-script::dat::states::every_saved_state_reads_exactly_and_writes_back_to_the_same_bytes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.graph.hash-new-entry-heads-its-chain",
        says: "A new hash table entry goes to the head of its bucket's chain, so a table packs \
               bucket by bucket with each chain newest first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-HASH-ADD"),
        station: "auberean-script::cpu::codec::a_hash_table_packs_bucket_by_bucket_each_chain_newest_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.graph.hash-tables-pack-in-bucket-order",
        says: "A packed hash table's header holds its entry count and the place of its bucket count \
               in a fixed list of primes, and its entries come bucket by bucket, each bucket's chain \
               from its head.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-HASH-ORDER"),
        station: "auberean-script::dat::states::every_packed_hash_table_lists_its_entries_in_bucket_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.graph.multi-hash-reads-as-the-engine-reads",
        says: "Every multi-hash in a recorded script call is packed in bucket order; read into the reference table as the engine reads one, entry by entry, each key's entries after its first come out reversed, and read and packed again the table is the bytes sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MULTI-ORDER"),
        station: "auberean-script::corpus::multi::every_recorded_multi_hash_reads_as_the_engine_reads_it_and_twice_comes_back_byte_for_byte",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.graph.object-size-must-match",
        says: "A packed script object whose slot count differs from its package's size refuses the \
               whole graph: packed data is tied to the library it was packed against.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SIZE-CHECK"),
        station: "auberean-script::cpu::codec::a_script_object_whose_slot_count_is_not_its_packages_size_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.graph.quest-list-reads-over-one-record",
        says: "A packed quest list is its count, then each record; read into the reference table, each record is read over the one before it, so a string reference keeps the variables earlier records' references left (one with the same id replaced) and, unused and unwritten, a literal it does not carry itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-QUEST-LIST-READ"),
        station: "auberean-script::cpu::codec::a_quest_list_read_into_the_table_keeps_what_earlier_records_left_in_their_strings",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.heap.import-rewrites-references",
        says: "Reading a packed graph into the reference table gives each entry a new id and \
               rewrites the references slots and containers hold; a singleton entry names the \
               singleton of its saved state.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-IMPORT"),
        station: "auberean-script::cpu::codec::a_graph_read_into_the_table_gets_new_ids_and_its_references_follow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.heap.saved-state-reads-into-the-table",
        says: "A graph read into the reference table and packed again from its root gives the same \
               bytes under new ids: entries depth first from the root, an object's references in \
               slot order, a table's keys before its values.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-PACK-ORDER"),
        station: "auberean-script::dat::states::a_saved_state_read_into_the_table_packs_again_to_the_same_bytes_under_new_ids",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.interp.bounds-checked",
        says: "A script that recurses past the call stack's 192 frames is stopped with a reason \
               instead of writing past the end of the stack.",
        since: THIS_CLIENT,
        divergence: "CD2-checked-script-machine",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-BOUNDS"),
        station: "auberean-script::cpu::machine::runaway_recursion_stops_at_the_call_stack_limit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.float-comparisons",
        says: "Float comparisons push 0 or 1: the ordered ones and equality are false when either \
               side is not a number, inequality is true, and the float or and and count a value \
               that is not a number as not zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-FLOAT-COMPARE"),
        station: "auberean-script::cpu::machine::float_comparisons_are_false_on_nan_but_inequality_or_and_and_count_it_as_not_zero",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.integer-arithmetic",
        says: "Integer arithmetic wraps on overflow, division and remainder truncate toward zero, \
               shift counts are taken modulo 32, and the comparisons push 0 or 1, signed or \
               unsigned as the instruction says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-INT-ARITH"),
        station: "auberean-script::cpu::machine::integer_arithmetic_wraps_truncates_and_masks_shift_counts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.long-values",
        says: "A 64-bit value takes two stack slots, low word first; a long constant is stored high \
               word first in the code; a 64-bit shift of 64 or more clears the value.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LONG"),
        station: "auberean-script::cpu::machine::long_values_take_two_slots_and_shifts_of_64_or_more_clear_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.native-suspends-context",
        says: "A native may leave its context suspended inside the call; resuming it closes the \
               native's frame and runs on from the instruction after the call.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SUSPEND"),
        station: "auberean-script::cpu::machine::a_native_can_leave_the_context_suspended_to_be_resumed_after_the_call",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.not-ported-native-stops",
        says: "Calling a native that is not written yet stops the script cleanly, naming the native \
               and the retail function it stands for.",
        since: THIS_CLIENT,
        divergence: "CD2-checked-script-machine",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-NOT-PORTED"),
        station: "auberean-script::cpu::machine::a_native_not_written_yet_stops_the_run_naming_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.references",
        says: "A field is read and written through a reference to its object, the value stored \
               staying on the stack; a reference to a slot of the current frame lets a callee write \
               the caller's local, as out parameters do.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-REFS"),
        station: "auberean-script::cpu::machine::fields_load_and_store_through_references_and_slot_references_work_as_out_parameters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.step-budget",
        says: "A script that runs past its step budget is stopped instead of freezing the game.",
        since: THIS_CLIENT,
        divergence: "CD2-script-step-budget",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-BUDGET"),
        station: "auberean-script::cpu::machine::a_run_that_uses_its_step_budget_stops_instead_of_hanging",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.string-literals",
        says: "Every use of a string literal names the same shared string object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LITERALS"),
        station: "auberean-script::cpu::machine::a_string_literal_is_one_shared_object_however_often_it_is_named",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.switch",
        says: "A switch compares its value with each case in turn and jumps by the matching case's \
               offset, or the default's, counted from the end of its case table.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SWITCH"),
        station: "auberean-script::cpu::machine::a_switch_jumps_from_the_end_of_its_case_table",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.interp.virtual-dispatch",
        says: "A call through a receiver runs the function's slot in the receiver's package's \
               table, an override in a derived package winning, and an empty slot runs nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-DISPATCH"),
        station: "auberean-script::cpu::machine::a_call_runs_the_receivers_slot_and_an_empty_slot_runs_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.library.loads-and-links",
        says: "The script library loads and links: every native call bound to a native by id, \
               every call that names its function outright turned into a direct call to that \
               function's code.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LINK"),
        station: "auberean-script::dat::library::the_library_links_every_native_call_and_turns_direct_calls_into_code_offsets",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.library.unbound-native-fails-the-load",
        says: "A native call the native table has no entry for fails the load instead of leaving a \
               call to nothing in the code.",
        since: THIS_CLIENT,
        divergence: "CD2-checked-script-machine",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-UNBOUND"),
        station: "auberean-script::dat::library::a_native_call_the_table_does_not_know_fails_the_load",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.natives.a-profiles-aliases-pack-in-their-tables-order",
        says: "An options profile's aliases are a table of 47 buckets keyed by the wide \
               strings' hash: a new alias goes to the head of its bucket's chain, a name held \
               takes its new text in place, names compare exactly, and the table packs bucket \
               by bucket; removing and flushing keep the header's count.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ALIASES"),
        station: "auberean-script::cpu::options::a_profiles_aliases_go_to_the_head_of_their_bucket_and_pack_in_bucket_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.the-chat-options-answer-a-new-profiles-defaults",
        says: "An options profile no packed form filled answers the chat options a new profile holds (the four chat windows' filters, every text kind small, the kinds' colors, the devoted windows' channels, the kinds that pop the chat up); one whose packed form carried a table answers from it, a key it lacks as 0, a devoted window's channel as the say channel.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-CHAT-OPTIONS"),
        station: "auberean-script::cpu::options::the_chat_options_answer_a_new_profiles_defaults",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.options.an-exported-file-imports-back",
        says: "The player's options export as a text tree of the aliases, the damage text's flags \
               and range and bare sections for the rest, and an import of that file gives a new \
               profile holding those aliases and damage text options and a new profile's \
               everything else.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-FILE"),
        station: "auberean-script::cpu::options_file::an_exported_options_file_imports_back_into_a_new_profile",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.options.an-import-refuses-what-it-does-not-know",
        says: "An options import fails on an empty file name, a file that does not parse or holds \
               nothing, a node that is not the options', an unknown section, an alias with \
               nothing under it or a range that is not a number.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-IMPORT"),
        station: "auberean-script::cpu::options_file::an_import_refuses_what_it_does_not_know",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.options.every-captured-profile-round-trips",
        says: "Every options profile a server sent at a session's start in 2013 and 2017, and every shortcut a client sent, reads as the engine holds it (a profile: the mask of the parts that differ from a new one, the parts in order, the version after the option word; a shortcut: its kind and the one datum the kind carries) and packs back to the same bytes; every hash table a client's scripts made and sent has the 47 buckets of a new table.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-OPTIONS-PROFILE"),
        station: "auberean-script::corpus::options::every_captured_options_profile_and_shortcut_reads_as_the_engine_holds_it_and_packs_back",
        tier: Tier::Corpus,
    },
];
