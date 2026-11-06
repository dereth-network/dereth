//! The script machine: the compiled library and its load pass, packed object graphs (saved states
//! and call arguments), the instruction set, the reference table, natives and script calls across
//! the connection.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "script.admin.client-side-commands-run-the-admin-natives",
        says: "An admin character's client-side commands run the game's scripts to their end through every \
               native only they reach: physics properties, the script machine's tables, the object table, \
               the time, the quests and acts, the places, test scripts, a file of lines, a report and the \
               debugger; none stops for want of a port.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ADMIN-NATIVES"),
        station: "yalain-testkit::dat::admin::an_admin_characters_client_side_commands_run_to_their_end",
        tier: Tier::Dat,
    },
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
        id: "script.bridge.every-captured-call-dispatches",
        says: "Every server-to-client script call recorded in 2013 and 2017 runs in a new context of \
               the object it is for (the player for the broadcast carriers): its arguments read, its \
               handler called by name, ending or stopping at a named native, never failing the \
               machine.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-DISPATCH-IN"),
        station: "auberean-glue::corpus::calls::every_relaunch_server_call_runs_to_its_end_or_stops_at_a_named_native",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.bridge.every-captured-send-repacks",
        says: "Every client-to-server script call recorded in 2013 and 2017, made again by its \
               sending function from the recorded values, sends the same function and the same \
               argument bytes, objects aside from the ids they are packed under.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SEND"),
        station: "auberean-glue::corpus::sends::every_relaunch_client_call_is_packed_again_by_its_sending_function",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.a-killing-blow-is-shown-by-the-players-scripts",
        says: "A killing blow the server tells the player of is shown by the player's scripts as \
               one line of the kind 8: the killing blow's string with the defender's name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHAT-KILLING-BLOW"),
        station: "auberean-client-runtime::corpus::chat::a_killing_blow_is_shown_by_the_players_scripts",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.a-saying-is-written-by-the-scripts-without-the-gender-mark",
        says: "A saying near the player reaches the chat windows as the player's scripts write \
               it: the speaker's name from its description without the gender mark the server \
               sends with it, \"says,\" and the words in quotes, as a line of the spoken kind.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHAT-SCRIPT-LINES"),
        station: "auberean-client-runtime::corpus::chat::a_saying_is_written_by_the_scripts_without_the_gender_mark",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.a-typed-line-is-said-and-a-say-command-says-the-rest-of-its-line",
        says: "A line typed (from the chat panel or as the line-typed action) is carried out by \
               the player's command line: a line that is no command is said whole, one word or \
               more, and /say says the rest of its line with the command cut off; each goes to \
               the server as one saying carrying those words, none answered with the say \
               command's usage line.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHAT-TYPED-LINE"),
        station: "auberean-client-runtime::corpus::chat::a_typed_line_is_said_and_a_say_command_says_the_rest_of_its_line",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.an-alias-in-a-line-is-said-as-what-it-stands-for",
        says: "An alias made with /alias and typed after a ; is said as what it stands for, \
               expanded in turn (an alias already being expanded stays as typed), the search \
               going on after each expansion; the line kept among the lines typed is the line \
               as typed; a predefined alias cannot be made; a line a toolbar slot holds is \
               said when the slot is used, as if typed; an alias removed is said as typed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHAT-ALIASES"),
        station: "auberean-client-runtime::corpus::chat::an_alias_in_a_line_is_said_as_what_it_stands_for_and_a_slots_line_as_if_typed",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.every-recorded-saying-and-tell-is-written-with-the-words-and-a-bare-name",
        says: "Every saying and tell the 2005, 2013 and 2017 captures recorded, taken through the player's scripts, reaches the chat windows as a line of the spoken or the tell kind holding the words as said and the speaker's name without its marks; no line shows a name's mark.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHAT-REPLAY"),
        station: "auberean-client-runtime::corpus::chat::every_recorded_saying_and_tell_is_written_with_the_words_and_a_bare_name",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.help-shows-a-commands-help",
        says: "`/help` with a command's name shows that command's help as help text, the name taken \
               in any case; a name no command's help has shows the unrecognized-command line naming \
               it; with no name, the help list's label and every command's line, `  name -- text`, \
               in the names' order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHELL-HELP"),
        station: "auberean-client-runtime::corpus::chat::help_shows_a_commands_help_every_commands_line_or_that_it_knows_no_such_command",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.the-chat-log-is-written-with-the-local-time",
        says: "The chat log command, given no folder to write in, shows the player's scripts' line \
               that the file could not be opened; given one, it opens the player's log there, named \
               after the folder's full path, and every line the chat window is given is written to \
               it after the local time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-CHAT-LOG"),
        station: "auberean-client-runtime::corpus::chat::the_chat_log_says_when_it_cannot_open_and_writes_each_line_after_the_local_time",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.the-layout-commands-reach-the-panels-with-their-file",
        says: "The player's commands to save and load the panels' places reach the play screen as \
               notices naming the file, and the player's files are written and read back by a bare \
               name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LAYOUT-COMMANDS"),
        station: "auberean-client-runtime::corpus::chat::the_layout_commands_reach_the_panels_with_their_file",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.the-loc-alias-says-the-players-map-coordinates",
        says: "The predefined alias ;loc is said as the player's map coordinates, north or south \
               to a tenth then east or west, and /loc shows the place line with the same \
               coordinates; typed where a recorded client last reported its player, it says what \
               that client wrote into a channel.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LOC-ALIAS"),
        station: "auberean-client-runtime::corpus::chat::the_loc_alias_says_the_players_map_coordinates_as_the_recorded_clients_wrote_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.chat.the-servers-lines-of-text-are-shown-by-the-players-scripts",
        says: "Every line of text the servers sent a player in the world is given to the player's \
               scripts, which show it in the chat windows as the line the server wrote, of the kind \
               it named; with no player in the world it shows nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHAT-SERVER-TEXT"),
        station: "auberean-client-runtime::corpus::chat::the_servers_lines_of_text_are_shown_by_the_players_scripts",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.glue.a-copy-of-a-game-value-is-its-own",
        says: "A copy of a quest, a list of quests, an effect, a shortcut, a keyframe, a scene or an \
               act holds every part of the original and is owned apart from it; a new allegiance, \
               fellowship or shop adaptor holds 0 in every word.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-VALUE-COPIES"),
        station: "auberean-glue::cpu::copies::a_copy_of_a_game_value_holds_every_part_and_is_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-creations-choices-are-set-and-read",
        says: "A creation's race, sex, class and physique values read back what was set; a physique                type with no value reads as -1; setting a physique value, the class or the implement                answers 1, and setting the race, the sex or the starting area answers nothing; through                the null reference each crashes the script.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-CHARGEN-RESULT"),
        station: "auberean-glue::cpu::natives::a_creations_fields_read_back_what_was_set",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-hash-table-optimizes-only-a-size-that-does-not-suit",
        says: "A hash table asked to optimize its size keeps a bucket count within a factor of two of its entries and answers no; otherwise it moves to the smallest count that holds them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-HASH-OPTIMIZE-GUARD"),
        station: "auberean-glue::cpu::natives::a_hash_table_optimizes_only_a_size_that_does_not_suit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-line-of-sight-is-set-up-and-read",
        says: "A line of sight is set up through its natives as the engine's own: what it passes through copied, its test kind set, its start from a ray in a cell, a place and an offset, or one place to another across landblocks, objects added to pass through but the invalid id; after a test it answers whether it was unblocked or reached an unloaded landblock, its final ray and cell, and its end less the final cell's landblock offset.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LOS-VALUES"),
        station: "auberean-glue::cpu::motion::a_line_of_sight_is_set_up_and_read_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-mask-changed-by-property-is-cleared-first",
        says: "A mask of physics types in a body's collision settings, changed by a property, is first cleared and marked set; a property the install's list does not name as a flag sets nothing and answers 0. Reading a mask answers a new object holding a copy; a whole mask set is copied and marked set.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MISSILE-MASK"),
        station: "auberean-glue::cpu::missile::a_mask_changed_by_a_property_is_cleared_first_and_a_whole_mask_is_copied",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-name-is-left-alone-with-no-word-filter",
        says: "Writing a name for its audience with data files that hold no word filter leaves the \
               string it goes into alone and answers 0.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-NAME-NO-WORD-FILTER"),
        station: "auberean-glue::cpu::natives::a_name_is_left_alone_with_no_word_filter",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-package-by-name-makes-an-instance",
        says: "A package's name comes as a new string, empty for no package; a new instance comes of a \
               package by name, and a name no package has stops the call with the null reference; a \
               validated new type always answers -1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TYPE-INFO"),
        station: "auberean-glue::cpu::natives::a_package_by_name_makes_an_instance",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-positions-parts-distances-and-text",
        says: "A position's cell, origin and height are read and set whole and its frame copied \
               from another; two are in one landblock by their cells' top halves, equal when their \
               cells are and every part of their frames is within 0.0002; distances are worked at \
               double precision and kept as floats; offsets add in the landblock's frame or turned \
               by the position's rotation; a position written out shows its cell in hexadecimal \
               and each part with six decimals; before the region is known no cell is valid.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-POSITION"),
        station: "auberean-glue::cpu::natives::a_positions_parts_are_read_and_set_and_its_distances_kept_as_floats",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-quest-dumps-as-the-client-writes-it",
        says: "A quest dumps its id, status and phase, then, under way, the time left and its cancel line only when it runs out; otherwise when it was done, the time before it may be taken again and whether it may be taken no more. The client's formats for the challenge and the cancel line take no value. A list numbers its quests from 1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-QUEST-DUMP"),
        station: "auberean-glue::cpu::quests::dumps::a_quest_dumps_as_the_client_writes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-random-selection-table-picks-each-entry-once",
        says: "A random selection table keeps its entries as a weighted binary heap: an entry of a weight of 0 or less is not added; a choice rolls the engine's dice from 1 to the total weight and walks down from the root by the running sum, the chosen entry leaving the table and the last taking its place; an empty table answers no; starting over puts the chosen back in the order chosen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SELECTION-TABLE"),
        station: "auberean-glue::cpu::numbers::a_random_selection_table_picks_each_entry_once_by_weight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-saved-states-records-are-gathered",
        says: "The records a saved state needs are gathered into a set of ids, each once and in id \
               order, never id 0, each add answering whether it added; a string reference adds its \
               table and each nested reference's, a literal nothing; a scene adds each keyframe's \
               video, audio and voice records and its subtitle's tables, the tracks left out when \
               the type asked about is the one they are never fetched for; a race and sex's look \
               adds its body, its appearance table's records, its pile's look and its music.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-RECORD-IDS"),
        station: "auberean-glue::cpu::natives::the_records_a_saved_state_needs_are_gathered_once_each_in_id_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-storys-records-dump-as-the-client-writes-them",
        says: "A keyframe dumps its number, its duration in the C library's exponent form and only the tracks it defines; a scene its id, its act and its flags, then its keyframes; an act its flags and its scenes under headings numbered from 1, or a line saying it has none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STORY-DUMP"),
        station: "auberean-glue::cpu::quests::dumps::a_storys_records_dump_as_the_client_writes_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-string-reference-reads-from-a-text-tree",
        says: "A string reference reads itself from a text tree by its children's keywords: the table's name found by its record, the token's text hashed as a string's token, an unknown keyword warned of and passed over, and a keyword with no one text beneath it an error that ends the reading and keeps what was read.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRINGINFO-TREE"),
        station: "auberean-glue::cpu::strings::a_string_reference_reads_itself_from_a_text_tree_by_its_keywords",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-strings-case-changes-accented-letters-too",
        says: "A string changes case as the C library does in the English locale: accented letters change too, and a letter with no single partner stays as it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRING-CASE-LOCALE"),
        station: "auberean-glue::cpu::strings::a_strings_case_changes_accented_letters_too",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-strings-float-is-read-as-the-c-library-reads-one",
        says: "A string's leading float is read as the C library reads one: past white space, as far as a number goes, its exponent marked with e or d and read only with digits; the words for infinity and not-a-number read as zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRING-TO-FLOAT-WCSTOD"),
        station: "auberean-glue::cpu::strings::a_strings_float_is_read_as_the_c_library_reads_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.a-word-is-found-among-a-texts-words",
        says: "A word is one of a text's words when a piece of the text, split at spaces, commas and semicolons with the empty pieces dropped, is the word, ASCII letters compared without their case; part of a piece is not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-WORD-IN-TEXT"),
        station: "auberean-glue::cpu::natives::a_word_is_one_of_a_texts_words_split_at_spaces_commas_and_semicolons",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.adaptor-fields",
        says: "An interface adaptor's setter copies its argument into the adaptor and a getter reads it back; a fellowship adds a fellow under an id only once, answering whether it added it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ADAPTORS"),
        station: "auberean-glue::cpu::natives::an_adaptor_keeps_what_its_setters_give_and_a_fellowship_adds_a_fellow_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.an-arrays-new-places-hold-nothing",
        says: "An array of references whose element count is set higher holds no reference in \
               each new place; set lower, it keeps its first places.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ARRAY-COUNT"),
        station: "auberean-glue::cpu::natives::an_array_of_references_made_longer_holds_no_reference_in_its_new_places",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.damage-text-natives",
        says: "A new floating damage text is an empty string in a color of all zero, shown for -1 at a scale of 0; its duration and scale are kept as the floating-point unit leaves them (a signalling not-a-number made quiet).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-DAMAGE-TEXT"),
        station: "auberean-glue::cpu::natives::a_new_damage_text_lasts_minus_one_and_keeps_its_floats_as_the_fpu_leaves_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.debug-lines-are-printed-for-the-console",
        says: "A debug native prints one line after `interp:  `: an integer in hexadecimal and \
               signed, a 64-bit one likewise, a float to three places and a 64-bit float to six, a \
               string as it is; none answers anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-DEBUG-LINES"),
        station: "auberean-glue::cpu::debug::debug_lines_are_printed_for_the_console",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.dice-follow-the-engine-generator",
        says: "Dice draw from the engine's one random number generator, two congruential generators combined through a shuffle table, never seeded, so every process draws the same sequence from its start; a roll between two bounds takes either order, an integer roll including both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-DICE"),
        station: "auberean-glue::cpu::numbers::the_generator_draws_the_engines_sequence_and_rolls_span_their_bounds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.entity-names-and-link-requirements",
        says: "An entity's record is found by its name, and its name by its record, in the \
               client's table of file names as an entity record's file; a link's requirements are \
               five words, zero when new (whether an input, its type, how many links, whether \
               required, how many required), each set and read as a word and packed in that order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ENTITY-NAMES-LINKS"),
        station: "auberean-glue::cpu::entity::an_entity_is_found_by_name_and_a_links_requirements_are_five_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.enumeration-names-and-lists",
        says: "An enumeration's natives answer from its record: a value's name as a new string (the plain name and the name for logs alike, empty when it has none), how many values it has, every value as a new list of words and every name as a new list of new strings in the enumeration's order, and a name's value in a record named directly.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ENUM-LISTS"),
        station: "auberean-glue::dat::enums::an_enumerations_names_count_and_lists_come_from_its_record",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.glue.every-saved-state-names-its-records",
        says: "Every saved state of the install, made a process and asked which records it needs \
               as the client asks when it loads an object's state, runs the scripts' answer to its \
               end, or is not asked when its package is not a preprocessed one; the answers hold \
               every race and sex look's records and every packed scene's tracks and subtitles.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STATE-RECORDS"),
        station: "auberean-glue::dat::records::every_saved_state_names_the_records_it_needs_without_stopping",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "script.glue.file-names-and-tracks-by-name",
        says: "A file's name finds its record's id in the client's table of file names, typed by \
               its extension (read with white space trimmed; no known extension answers 0), or by a \
               type given; an id finds its file's name; a keyframe's video, audio and voice tracks \
               set by a file's name take that id and are marked defined even when the table answers \
               0, as it does for every name on an install of the relaunched game.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-FILE-NAMES"),
        station: "auberean-glue::cpu::natives::a_file_name_finds_its_id_by_its_extension_and_a_keyframe_takes_its_tracks_by_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.folder-names-for-file-names",
        says: "The folder a script builds a file's name on is the empty name, so the file stays \
               in the folder the host keeps files in; a folder's name is made one way round, each \
               backslash a slash or each slash a backslash as asked, narrowed, with that separator \
               added when the name does not end in it (an empty name stays empty).",
        since: THIS_CLIENT,
        divergence: "CD2-files-stay-in-settings-folder",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-FOLDER-NAMES"),
        station: "auberean-glue::cpu::strings::the_current_folder_is_the_empty_name_and_folder_names_are_made_one_way_round",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.hash-table-natives",
        says: "A hash table native adds an entry at the head of its bucket's chain, sets a value in \
               place, finds a value through an out reference, and walks its entries bucket by \
               bucket with an iterator.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-HASH-NATIVES"),
        station: "auberean-glue::cpu::natives::a_hash_table_adds_finds_and_walks_its_entries_in_bucket_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.math-as-the-floating-point-unit",
        says: "The math natives answer as the floating-point unit does: the trigonometric functions, logarithms, exponential, square root and remainder of doubles; a sine, cosine or tangent of a number from 2 to the 63rd is the number itself; an invalid operation answers the unit's own not-a-number; a quotient is mapped into -1 to 1 with the client's own two over pi.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MATH-UNIT"),
        station: "auberean-glue::cpu::numbers::the_math_natives_answer_as_the_floating_point_unit_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.missile-parts-copied-into-a-missile",
        says: "A missile filled from script keeps its record and holding place as given, whether it is thrown and whether it copies the weapon from each argument's low byte, and copies of its aim and its collision settings as they were when set: an object to strike at a fraction of its height with a scaled offset, a starting speed through the floating-point unit; a mask's low word set alone carries its sign into the high word and marks nothing set; passing through an object answers 1 and lists it placed and moving, the lists starting solid.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MISSILE"),
        station: "auberean-glue::cpu::missile::a_missile_takes_copies_of_its_aim_and_what_it_passes_through",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.moves-placements-headings-and-rays",
        says: "A move's parameters, what a move reports, an order to place a body, a heading and a ray are filled and read through their natives as the engine's own: each setter raises the flags it implies, the places to pass are taken one at a time, a report answers its parts, a heading is brought into 0 to 360 and tested against a tolerance, and a ray from a start and an offset has the offset's direction and length.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MOTION-VALUES"),
        station: "auberean-glue::cpu::motion::moves_placements_headings_and_rays_are_filled_and_read_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.multi-hash-natives",
        says: "A multi-hash native adds an entry right after the first entry with its key (at the head of its chain for a new key), finds a key's first entry and where its entries end, walks, counts and removes them, and erases the entry an iterator stands at.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MULTI-NATIVES"),
        station: "auberean-glue::cpu::natives::a_multi_hash_keeps_a_keys_entries_together_after_its_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.multi-map-natives",
        says: "A multi-map keeps its entries in key order, each key's in the order added; find is a \
               key's first entry, find-last the place after its last whether the key is there or \
               not; an iterator stays on its entry while others come and go, steps back from past \
               the last to the last, and equals another only at the same place of one map.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MULTI-MAP"),
        station: "auberean-glue::cpu::natives::a_multi_map_keeps_its_entries_in_key_order_and_its_iterators_on_their_entries",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.named-places-are-found-in-lower-case",
        says: "A named place is found by its name put in lower case and copied into a position, or written out into a string; not found leaves either as it was, and a table's name with a capital is never found.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACES-LOOKUP"),
        station: "auberean-glue::cpu::natives::places::a_named_place_is_found_by_its_name_in_lower_case",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.named-places-dump",
        says: "The named places' dump is a line for each place, its name, a colon and the position's own text with a stop, or a line saying it could not get the list when the install holds no table.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACES-DUMP"),
        station: "auberean-glue::cpu::natives::places::the_named_places_dump_lists_each_place_or_says_it_could_not_get_the_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.quest-list-natives",
        says: "The player's quest list keeps copies of quest records, newest given first: updating a quest copies the record over the first one with its id, in place, or else adds it before the first record given earlier than it (after those given at the same time), saying through its out argument whether it was new; erasing takes out the last record with the id; finding copies the first into the out argument's record, leaving it alone when there is none; popping copies out and removes the first; pushing adds at the end.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-QUEST-LIST"),
        station: "auberean-glue::cpu::quests::a_quest_list_keeps_quests_newest_given_first_and_updates_a_quest_in_place",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.quest-record-natives",
        says: "A quest record is new never given or done, its update playing the quest's effect; a script may fill it whole, the maxed-out flag the server never sends included; its id, status and effect read as words, its name, description and journal entry as new string references holding copies; its time left is the time before a retry once it is no longer under way, cut toward zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-QUEST-RECORD"),
        station: "auberean-glue::cpu::quests::a_quest_record_is_filled_whole_and_read_field_by_field",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.shop-sales-in-ordinal-order",
        says: "A shop adaptor holds copies of the sales added to it in order of their ordinals, lowest first: a sale goes before the first whose ordinal is greater, after those with the same one; a sale whose ordinal was never set has 0.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SHOP-SALES"),
        station: "auberean-glue::cpu::natives::a_shop_keeps_its_sales_in_ordinal_order_ties_in_the_order_added",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.string-building",
        says: "Joining a string and a number writes the number as the C library does (six decimals \
               for a float), before or after the string as the call asks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STRING-BUILD"),
        station: "auberean-glue::cpu::natives::numbers_join_strings_as_the_c_library_writes_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.string-references-write",
        says: "A literal a script gives a string reference (or a string variable) is kept with its meta language marks escaped, so the reference writes it back as it was given; a string's token folds its units seven bits apart.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STRING-REFERENCES"),
        station: "auberean-glue::cpu::natives::a_literal_string_reference_writes_back_as_given_and_a_string_has_its_token",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.strings-equal-with-case-kept-only-when-asked",
        says: "A string's equality and difference keep case when their flag is set; otherwise \
               each unit is lowered as the wide C library lowers it, accented capitals too, as \
               ordering with case ignored does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GLUE-TEXT-EQUAL"),
        station: "auberean-glue::cpu::strings::strings_are_equal_with_case_kept_only_when_asked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.strings-ordered-hashed-trimmed-replaced-scrambled",
        says: "A string orders against another as the C library compares (case kept: -1, 0 or 1; case ignored: the difference of the first units that differ), hashes as the client hashes, trims white space at both ends, replaces every occurrence answering how many, scrambles and unscrambles with the client's key, and appends 64-bit integers and doubles; the case-insensitive string's natives are the string's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STRINGS-ORDER"),
        station: "auberean-glue::cpu::strings::strings_order_hash_trim_replace_and_scramble_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.strings-read-64-bit-numbers-the-c-librarys-way",
        says: "A string reads as a 64-bit number in base 0 as the C library reads one: signed, \
               clamped to the least or greatest; unsigned, an overflowing magnitude is all ones, \
               which a minus sign counts back to 1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GLUE-TEXT-INT64"),
        station: "auberean-glue::cpu::strings::strings_read_64_bit_numbers_the_c_librarys_way",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.text-reads-as-a-floating-point-number",
        says: "A text counts as a floating-point number when the C library reads all of it: white \
               space first, then each unit narrowed in the Western code page with its best fit \
               (full-width digits read as digits), a sign, digits with one point, an exponent \
               after e, E, d or D; the empty text counts; a number past the largest double or \
               below the least normal one is a range error and does not count; only the first 21 \
               significant digits count.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-READS-DOUBLE"),
        station: "auberean-glue::cpu::strings::a_text_reads_as_a_floating_point_number_as_the_c_library_reads_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.text-utilities-read-numbers-and-cut-paths",
        says: "A text reads as a whole number as the C library reads one in base 0, all of it and \
               in range (an unsigned number takes a minus sign; the empty text counts, a sign or a \
               lone hexadecimal prefix does not); a path loses its extension at its last dot, or \
               its folders up to its last separator, its wide units narrowed on the way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-TEXT-UTILITIES"),
        station: "auberean-glue::cpu::strings::a_text_reads_as_a_whole_number_as_the_c_library_reads_it_and_paths_are_cut",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.the-machine-counts-its-processes",
        says: "The process table's text gives how many processes it holds and its room, 128 until full \
               and then doubled; the contexts' text counts the contexts each keeps and the one running; \
               a process asked after is there or not, with an empty text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VM-QUERY-PROCESSES"),
        station: "auberean-glue::cpu::natives::the_machine_counts_its_processes_and_contexts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.the-machines-other-listings-say-little",
        says: "The performance listing is a heading and a blank line; the listings of references are \
               empty and answer true; a null string stops the call.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VM-QUERY-LISTINGS"),
        station: "auberean-glue::cpu::natives::the_machines_other_listings_say_little",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.the-normal-and-skewed-dice-roll-as-the-client-does",
        says: "The normal dice draw two numbers a try by the ratio of uniforms until one is taken, \
               the percentile is 1 to 100 from one draw, and a skewed roll is a normal number about \
               a mean kept between two bounds in two tries, else the mean.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-DICE-NORMAL"),
        station: "auberean-glue::cpu::numbers::the_normal_and_skewed_dice_roll_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.the-wall-clock-and-the-computed-time",
        says: "The wall clock native answers the host's seconds since 1970, or -1 without a clock; \
               the computed time is the game's clock.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-WALL-CLOCK"),
        station: "auberean-glue::cpu::numbers::the_wall_clock_comes_from_the_host_and_answers_minus_one_without_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.vector-arithmetic",
        says: "A vector's arithmetic works at double precision, each result kept as a float: the cross product a new vector, the dot product, the length; adding and taking away another in place; a copy of another's parts; two vectors equal when every part is within 0.0002; the squared length answers nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-VECTOR-ARITH"),
        station: "auberean-glue::cpu::natives::vector_arithmetic_works_at_double_precision_and_keeps_floats",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.vector-set-and-parts",
        says: "A vector set from three floats keeps them as the floating-point unit leaves them (a signalling not-a-number made quiet), and each of its parts reads back as stored.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-VECTOR"),
        station: "auberean-glue::cpu::natives::a_vector_set_from_three_floats_reads_back_each_part",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.glue.vectors-and-rays-are-made-one-long",
        says: "A vector, or a ray's direction, is made one long by one Newton step from the \
               reciprocal square root of its squared length, each step in single precision, and \
               becomes zero when its squared length is below 2 to the -46th; a ray keeps its start \
               and length.",
        since: THIS_CLIENT,
        divergence: "CD2-normalize-as-on-sse2",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-NORMALIZE"),
        station: "auberean-glue::cpu::motion::a_vector_and_a_rays_direction_are_made_one_long_by_one_newton_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.graph.a-hashed-key-read-in-is-hashed-through-what-it-names",
        says: "A table key that is a hashed reference keeps its filed hash when a graph is read \
               into the reference table, follows its object to the object's new id, and from then \
               on is marked to be hashed through what it names rather than by the filed hash.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-HASHED-KEYS"),
        station: "auberean-script::cpu::codec::a_hashed_reference_key_read_into_the_table_follows_its_object_and_stops_using_its_filed_hash",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.graph.a-native-copy-reverses-each-hash-chain",
        says: "The native factory's copy of a hash table or set has as many buckets as the original \
               with each chain reversed; a multi-hash, a list and an iterator copy as they are. A new \
               hash or set has 47 buckets, a new multi-hash 193.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-NATIVE-COPY"),
        station: "auberean-script::cpu::codec::a_native_copy_of_a_hash_reverses_each_chain_and_keeps_everything_else_in_order",
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
        id: "script.graph.an-iterator-packs-its-container-and-its-steps",
        says: "A container iterator packs its container's reference and how many steps from the \
               container's first entry it stands (the entry count when past the last), and once \
               read into the table with its container stands there again: on the same place, past \
               the last, or on the same entry of a multi-hash.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ITERATOR-PACK"),
        station: "auberean-script::cpu::codec::an_iterator_packs_its_container_and_its_steps_and_stands_there_again_when_read_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.graph.an-object-copied-keeps-its-icons-layers",
        says: "An object holding an icon description, copied whole as a script's copy does it, \
               holds the same layers in the same order: the count, then each layer's number, \
               picture and color (the products of the sales recorded in January 2017, read from \
               the calls that list them).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ICON-COPY"),
        station: "auberean-script::corpus::icons::a_recorded_sale_copied_keeps_the_layers_of_its_products_icon",
        tier: Tier::Corpus,
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
        id: "script.interp.pure-functions-complete",
        says: "Every script function that calls no native, makes no call through a receiver and \
               makes no new object, with only numbers for parameters, runs to its end on an \
               instance of its package with every parameter zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-PURE"),
        station: "auberean-glue::dat::pure::every_pure_script_function_runs_to_its_end",
        tier: Tier::Dat,
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
        id: "script.interp.stops-counted",
        says: "The machine counts every run that stops short by what stopped it (a native not                written yet, the null reference, another stop) and the code it stopped in, for a                host to report what the scripts still lack.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STOPS-COUNTED"),
        station: "auberean-script::cpu::machine::the_machine_counts_every_stop_by_what_stopped_it_and_where",
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
        id: "script.library.taken-only-from-the-machines-version",
        says: "The script library is taken in only when its versions table names the script \
               machine's version as 13, or does not name it; any other entry of the table is not \
               checked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MACHINE-VERSION"),
        station: "auberean-script::cpu::codec::a_library_is_taken_only_when_it_names_the_machine_version_the_client_runs",
        tier: Tier::Cpu,
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
        id: "script.looks.a-description-packs-again-as-it-was-sent",
        says: "A visual description a server sent reads into the client's own form and packs again \
               to the same bytes: the fields marked present are those that differ from an empty \
               description's, the tables in the order sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOKS-PACK"),
        station: "auberean-client-runtime::corpus::effects::every_sent_description_packs_again_as_it_was_sent",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.looks.a-held-description-answers-as-its-own-form",
        says: "A visual description a script holds answers as the client's own form: its icon is                the one the inventory draws its object with (and whether it has a layer), the                icon's check says whether it can be drawn, its scale is its own, the appearance                values it gives a part group read back and add to another description as they                were; one value held on its own reads back, is given to an empty description and                taken off again; cleared, it adds nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOKS-HELD"),
        station: "auberean-client-runtime::corpus::effects::a_held_description_answers_its_icon_scale_and_appearance_as_its_own_form",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.looks.an-objects-own-description-is-read-changed-and-taken-as-its-scripts-ask",
        says: "An object's own description answers what the server sent; a change the scripts add \
               or take out is answered at once and taken by the body only when they ask, and a \
               description the server sends replaces a change not taken; started again it extends \
               the object's physics record; a body has a motion interpreter when its description's \
               total names a motion description.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOKS-OWN"),
        station: "auberean-client-runtime::corpus::effects::an_objects_own_description_is_read_changed_and_taken_as_its_scripts_ask",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.a-collision-ends-in-the-scripts-without-reading-the-profile",
        says: "A collision handed to the scripts of an object in a recorded world runs to its                end: the library's answer to the interface's collision function only says it was                handled, and no function that reads a collision profile has a caller.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-COLLISION-ANSWER"),
        station: "auberean-client-runtime::corpus::calls::a_collision_handed_to_recorded_objects_scripts_runs_to_its_end",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.a-data-id-loaded-by-a-files-name-finds-nothing-on-the-relaunched-game",
        says: "Setting an object's data id from a file's name asks the client's table of file \
               names for the record, typed by its extension; a name the table does not hold sets \
               nothing and answers 0, and on an install of the relaunched game the table holds no \
               name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LOAD-DATA-ID"),
        station: "auberean-client-runtime::cpu::scripts::repository::a_data_id_loaded_by_a_files_name_sets_nothing_when_the_table_has_no_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-dump-lists-a-descriptions-contents-in-order",
        says: "An object's description written out lists what it has set in the client's own order, \
               under a heading for each level; a record with no description says so.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-DESC"),
        station: "auberean-client-runtime::cpu::scripts::dumps::a_description_lists_what_it_has_set_in_the_clients_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-dump-lists-a-value-the-data-does-not-name-under-the-last-name",
        says: "A value whose number its kind's enumeration does not name is listed under the last \
               name found, nothing before the first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-NAMES"),
        station: "auberean-client-runtime::cpu::scripts::dumps::a_value_the_data_does_not_name_is_listed_under_the_last_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-dump-lists-each-level-of-a-kind-in-hash-order",
        says: "An object's qualities of one kind written out list the object's own level then its \
               record's, each saying how many it holds, a table in hash order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-LEVELS"),
        station: "auberean-client-runtime::cpu::scripts::dumps::a_dump_lists_the_object_then_its_record_in_hash_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-dump-native-answers-a-new-string",
        says: "A dump native takes whether to go on to the record and answers a new string; a null \
               receiver crashes the call and an object the client does not hold gets its own line.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-NATIVES"),
        station: "auberean-client-runtime::cpu::scripts::dumps::the_natives_answer_a_new_string_and_a_missing_object_says_so",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-dump-of-the-objects-own-level-answers-once",
        says: "Asked not to go on to the record, a dump lists the object's own level once and \
               answers.",
        since: THIS_CLIENT,
        divergence: "CD2-qualities-dump-answers",
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-ONE-LEVEL"),
        station: "auberean-client-runtime::cpu::scripts::dumps::a_dump_of_the_objects_own_level_answers_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-dump-writes-a-string-reference-for-a-log",
        says: "A string reference written out is its text for a log in parentheses, or a line saying \
               it cannot show.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-STRINGINFO"),
        station: "auberean-client-runtime::cpu::scripts::dumps::a_string_reference_is_written_for_a_log_or_says_it_cannot_show",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-dump-writes-each-kind-in-its-format",
        says: "Each kind of quality writes in its own format: yes or no as true or false, data ids in \
               eight hexadecimal digits, object ids in sixteen, floats and times to two decimals, \
               whole numbers unsigned, 64-bit numbers by their low half.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-FORMATS"),
        station: "auberean-client-runtime::cpu::scripts::dumps::each_kind_writes_its_values_in_its_format",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-gift-whose-object-came-first-says-the-item",
        says: "A gift whose object came in the frame of the answer to its move is called \"the item\" in the line about it; one the client had before is named.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-GIFT-LINE"),
        station: "auberean-client-runtime::corpus::inventory::a_gift_whose_object_came_first_says_the_item",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.a-moves-answer-does-not-find-its-item-being-made",
        says: "A move's answer asking for the item it moves finds nothing while the item is being made, and finds it after; it finds any other object, and another script finds the item at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-BEING-MADE"),
        station: "auberean-client-runtime::cpu::scripts::lookup::a_moves_answer_does_not_find_its_item_being_made",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-physics-object-passes-requests-to-its-body",
        says: "What a script asks of an object's physics object (its speed or acceleration scale, \
               cloaking, reporting or ignoring collisions, looking at an object, a push, stopping \
               a layer) goes to its body in order and answers 1 (stopping a layer 0); with no \
               physics object nothing goes and it answers 0 (stopping a layer 8); a scale answers \
               nothing and comes through the floating-point unit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-PHYSICS-ASK"),
        station: "auberean-client-runtime::cpu::scripts::physics_object::what_a_physics_object_is_asked_goes_to_its_body_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-physics-object-reads-its-body",
        says: "A script reads an object's physics object from its body: its speed, acceleration \
               and jump scales, its radius and height (its top level's from the object holding \
               it), its mode and the mode it is going to, the motion the server last asked of it; \
               with no physics object each reads 0 (undetectable 1). Whether it is in the world is \
               the object's own; undetectable and frozen are as the scripts last set them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-PHYSICS-READ"),
        station: "auberean-client-runtime::cpu::scripts::physics_object::a_physics_object_reads_its_body_and_none_reads_zero",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-physics-objects-properties-are-its-bodys-material",
        says: "A physics object's current properties are what the cells its body has been in made \
               of it: an object whose body has stood in none, or with no body, has none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-PHYSICS-PROPERTIES"),
        station: "auberean-client-runtime::cpu::scripts::physics_object::a_physics_objects_properties_are_its_bodys_material",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-quality-is-answered-by-the-description-then-the-object-then-its-record",
        says: "An object's quality is answered by its description, for a quality the description holds and has marked set; else by the object's own table of that kind; else by its qualities record, the record's description first and then its tables. Times, texts, positions, packages and 64-bit numbers are never the description's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-REPOSITORY-ASK"),
        station: "auberean-client-runtime::cpu::scripts::repository::a_quality_is_answered_by_the_description_then_the_object_then_its_record",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-quality-removed-leaves-the-record-answering",
        says: "Removing a quality takes it out of the object's description when the description has it set (its mark cleared), else out of the object's own table; the record keeps its value, which answers again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-REPOSITORY-REMOVE"),
        station: "auberean-client-runtime::cpu::scripts::repository::a_quality_removed_leaves_the_record_answering",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-quality-set-goes-to-the-description-when-it-holds-it",
        says: "Setting a quality the object's description holds sets it there and marks it set; any other is set in the object's own table of its kind. The record is never written.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-REPOSITORY-SET"),
        station: "auberean-client-runtime::cpu::scripts::repository::a_quality_set_goes_to_the_description_when_it_holds_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-rename-replaces-the-name-the-object-was-made-with",
        says: "A rename (the server's update of the name quality, of an object or the player) \
               takes the place of the name the object was made with: the name is the \
               description's, which answers for it first; the object's own qualities are left as \
               they were.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-RENAME"),
        station: "auberean-client-runtime::cpu::world::updates::a_rename_replaces_the_name_the_object_was_made_with",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-rider-is-refused-attacks-and-emotes-by-its-scripts",
        says: "While the player rides a mount (its private flag 2002 set), its scripts refuse an \
               attack and every emote (the emote menu's, a typed prefab emote, a typed custom emote) \
               with the client's own lines, sending nothing; no server sent those lines.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-RIDER-REFUSED"),
        station: "auberean-client-runtime::corpus::interface::a_rider_is_refused_attacks_and_emotes_by_its_scripts",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.a-server-gone-ends-the-session",
        says: "A server that ends the login connection (the client answers its goodbye) ends the \
               session: the server is gone, and nothing failed before it; while the server is \
               there, the session goes on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-SERVER-GONE"),
        station: "auberean-client-runtime::cpu::world::session::a_server_ending_the_connection_ends_the_session",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-silent-server-is-gone-after-140-seconds",
        says: "A login server that falls silent ends the session 140 seconds after it was last \
               heard, not before.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-SERVER-SILENT"),
        station: "auberean-client-runtime::cpu::world::session::a_silent_server_ends_the_session_after_140_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.a-sleeping-call-goes-on-when-its-time-comes",
        says: "A call that sleeps waits: nothing more of it happens before its delay has passed \
               on the client's clock, and at the first upkeep after that it goes on from after the sleep \
               to its end.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SLEEP"),
        station: "auberean-client-runtime::corpus::chat::a_sleeping_call_goes_on_at_the_first_upkeep_after_its_time",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.a-trade-partners-offer-is-shown-as-an-item",
        says: "An item a trade's partner offers, once the client has been sent it (still in the partner's pack), shows in the trade window with its name and icon, as the player's own items do; it is never shown as an item the client has not been sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-TRADE-OFFER-SHOWN"),
        station: "auberean-client-runtime::corpus::inventory::a_trade_partners_offer_is_shown_with_its_name_and_icon",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.an-adjusted-quality-adds-to-its-own-value-or-its-records",
        says: "Adjusting a whole number, a 64-bit number or a float adds to the object's own value; with none of its own, the record's value plus the change becomes the object's own (the change alone when the record has none); without a record nothing changes. The truncating forms hold the result between their ends, the change alone excepted.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-REPOSITORY-ADJUST"),
        station: "auberean-client-runtime::cpu::scripts::repository::an_adjusted_quality_adds_to_its_own_value_or_its_records",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-examined-effect-line-brings-the-effects-own-lines",
        says: "Every profile the recorded sessions' scripts handed the examination window with \
               effect lines comes with each effect line's own lines, made by the player's scripts \
               into a profile whose request about nothing asks for the line's tabs; nearly every \
               effect line has some, and each shows on the line's tabs or the first tab.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-EFFECT-LINES"),
        station: "auberean-client-runtime::corpus::profiles::an_examined_effect_line_brings_the_effects_own_lines",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.an-item-put-down-leaves-the-pack-for-the-world",
        says: "An item of the player's pack is held and out of the world; when the server clears \
               its container (a quality its description holds) it is no longer held, and once a \
               position places it, it stands in the world among the player's surroundings.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-CONTAINER-QUALITY"),
        station: "auberean-client-runtime::cpu::world::updates::an_item_put_down_leaves_the_pack_for_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-object-described-again-is-drawn-by-its-flags",
        says: "An object the server sends again in the incarnation the client has, with a body, \
               is not drawn when its no-draw flag is set, drawn again when neither that nor its \
               kept-hidden flag is, and left as it was when only kept hidden.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-DESCRIBED-AGAIN"),
        station: "auberean-client-runtime::cpu::scripts::names::an_object_described_again_is_drawn_or_not_by_its_flags",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-object-in-a-pack-keeps-what-its-scripts-set-of-its-body",
        says: "An object in a pack has a body as one in the world does: what its scripts set of it as it is made stays for when it comes out. One the client was not sent has none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CONTAINED-HAS-BODY"),
        station: "auberean-client-runtime::cpu::scripts::physics_object::an_object_in_a_pack_keeps_what_its_scripts_set_of_its_body",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-object-without-a-template-derives-from-its-packages-qualities",
        says: "The qualities record an object's own qualities derive from is its entity template's \
               when its description names a template; with no template, its package's (the \
               package's record is numbered by the low half of the package's number, the highest \
               such number with no package); a description naming something other than a \
               template as its entity names none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME3-PACKAGE-QUALITIES"),
        station: "auberean-client-runtime::cpu::scripts::repository::an_object_without_a_template_derives_from_its_packages_qualities",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-objects-kind-mobility-and-detection-words",
        says: "An object's physics record is its data id 1 (none: 0); it moves unless its \
               mobile flag says not; it is a player or a creature when its kind holds every bit of that kind; a \
               detection reports it when the report's mask meets its two detection words, low \
               word first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-KIND-WORDS"),
        station: "auberean-client-runtime::cpu::scripts::names::an_objects_kind_mobility_and_detection_words_answer_as_held",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-objects-name-is-its-name-quality",
        says: "An object's name and plural name are its name qualities as its repository answers \
               them; without one, a literal string saying it has no name (or no plural name).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-NAME-QUALITY"),
        station: "auberean-client-runtime::cpu::scripts::names::an_objects_name_is_its_name_quality_or_a_literal_saying_none",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-objects-package-is-its-scripts",
        says: "A script asking an object for its package is answered with the package of the \
               object's scripts' instance, and every bit set while the object has none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MY-PACKAGE"),
        station: "auberean-client-runtime::cpu::scripts::distributor::an_objects_package_is_its_scripts_or_none",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-objects-position-is-its-own",
        says: "A script asking an object's position gets its own: another object's cell and place while it is in the world, the player's body for the player, the zero position for one with no place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OBJECT-OWN-POSITION"),
        station: "auberean-client-runtime::cpu::scripts::physics_object::an_objects_position_is_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-objects-waiting-calls-go-with-it",
        says: "An object that goes takes the calls of its scripts that wait with it: every call of \
               its process waiting to sleep out, or to go on after a wait was closed, is dropped; \
               other processes' calls are kept in their order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME3-WAITING-PURGED"),
        station: "auberean-client-runtime::cpu::scripts::distributor::an_objects_waiting_calls_go_with_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.an-opened-container-shows-what-its-scripts-were-told-it-holds",
        says: "After every recorded opening of a container, its scripts list what the server's answer told them it holds: nearly every opened container shows items (one opened empty shows none), and every container the client was sent items in shows them but one in a hundred. An object's instance the scripts made by asking for it by id is the one every later call on that object reaches.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-OPENED-CONTAINER"),
        station: "auberean-client-runtime::corpus::inventory::an_opened_container_shows_what_its_scripts_were_told_it_holds",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.an-overflowing-adjustment-takes-the-end-it-ran-past",
        says: "An adjustment whose sum moved against its change (it overflowed) takes the end it ran past: the greatest for a positive change, the least for a negative one; then the result is held to both ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-REPOSITORY-TRUNCATE"),
        station: "auberean-client-runtime::cpu::scripts::repository::an_overflowing_adjustment_takes_the_end_it_ran_past",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.every-recorded-objects-life-is-told-to-its-scripts",
        says: "Every object the recorded servers made has its scripts told it was made (its instance made then), then placed in the world when it has a place and no parent; an object placed later by a position is told then; one in the world that is contained, leaves, is carried by another or goes is told it left the world and then that it is out of it; one the server destroys is told before it goes. Every one of these calls runs to its end in the captures that hold their player's creation.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-OBJECT-LIFE"),
        station: "auberean-client-runtime::corpus::calls::every_recorded_objects_life_is_told_to_its_scripts",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.every-recorded-players-story-acts-build",
        says: "Every recorded player's act list, asked of its scripts as the quest window's story \
               page asks for it when the player arrives and each time the scripts say the story \
               changed, builds to its end: the acts in order of their numbers, each act's scenes in \
               order of theirs and all of that act, and every act the scripts said changed among \
               them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STORY-ACTS"),
        station: "auberean-client-runtime::corpus::acts::every_recorded_players_story_acts_build",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.every-recorded-quality-change-is-told-to-the-objects-scripts",
        says: "Every quality the 2013 and 2017 servers set on an object here, whoever holds it, is then told to the object's scripts (its kind and its number), which take it to its end (251,546 changes); the objects' scripts keep their bodies' collision state in step with the quality words that hold its masks (4,671 bodies' states set).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STAT-CHANGED"),
        station: "auberean-client-runtime::corpus::calls::every_recorded_quality_change_is_told_to_the_objects_scripts",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.every-recorded-rename-reaches-the-objects-name",
        says: "Every rename the 2017 servers sent reaches the object's name as its scripts read \
               it, through the event queue: one stamped past the object's last visual stamp takes \
               effect at once; one stamped at or before it is dropped, the name the object's \
               creation brought already the same; one about an object not here yet waits, and the \
               name is the new one once the creation comes (16,313 of 16,315; the other two are \
               renamed again first).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-RENAMES-2017"),
        station: "auberean-client-runtime::corpus::calls::every_recorded_rename_reaches_the_objects_name",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.every-recorded-server-call-runs-against-the-model",
        says: "Every server-to-client script call recorded in 2013 and 2017, fed with the rest of \
               its capture's server messages through the client's event queue, runs against the \
               model those messages build once the queue lets it through: to its end, or stopped at \
               a named native not written yet, never failing the machine.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-MODEL-NATIVES"),
        station: "auberean-client-runtime::corpus::calls::every_recorded_server_call_runs_against_the_model_or_stops_at_a_named_native",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.every-recorded-story-hook-runs-against-the-scripts",
        says: "Every combat story the 2013 and 2017 servers told, its hooks run in order as the attacker's body plays them, asks the attacker's scripts what each hook shows (the effects on the attacker, the target and the weapon, the target's and the weapon's behaviors), the target's scripts to show each hit's damage (the combat lines and the floating text over the target), the attacker's whether the skill of a hit on the player harms, and the body's scripts for the missile each firing hook makes (its record, where it waits, its flight, its aim and what it passes through); every call runs to its end (of 852,166 stories: 1,558,873 hooks' effects, 795,493 hits' damage with 183,052 combat lines and 736,361 floating texts, 59,085 questions of harm, 327,650 missiles, each made from a record, aimed at an object and passing through physics types the property list names).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-STORY-HITS"),
        station: "auberean-client-runtime::corpus::calls::every_recorded_story_hook_runs_against_the_attackers_and_targets_scripts",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.selecting-sends-what-the-recorded-client-sent",
        says: "Every selection the recorded 2013 and 2017 players sent is sent again with the same bytes: by the player's scripts on their own when a server's call had them select or a corpse placed in the world took the selection from its creature (at least 3,400), else by the interface's selection made again at that moment, the recorded server's messages before it taken: at least 63,450 of 63,531 the same, at most 30 differing and 15 made with nothing sent (the replay places a corpse as its message arrives, the client once its parts have loaded, so a few selections and the uses, attacks and examinations of the selection after them fall the other way). Made again, these send the same request at least so many times: starting an attack 4,000 of 4,053; attacking with a toolbar slot's skill 40,700 of 40,783; using an item 21,850 of 21,948; the windows' places and opacities 17,400 of 17,415 and the game options 1,000 of 1,010, as the interface keeps them; a shortcut put in a slot 980 of 982; making a recipe 1,050 of 1,059 and adding its ingredients 560 of 566; coins put in the money bag 420 of 835; an item offered in trade 330 of 333; a pet's mode 275 of 279; the pets' attack 940 of 948 and return 440 of 447 and a pet renamed 12 of 12; a trade opened 100 of 101, accepted with what it holds 165 of 170, declined 2 of 2, an item taken back 3 of 3, and closed 26 of 26 with the scripts' own; the story's scenes done 115 of 119; a shop left 29 of 29, its next and previous pages 16 of 16 and 5 of 5, a store entered from a catalog 11 of 11 and a catalog left 7 of 7; a container melted into money 3,380 of 3,384, emptied into the pack 155 of 159 and closed 2,250 of 2,253; an attack stopped 1,130 of 1,139; a craft skill raised 85 of 89; the chat options sent, set as the chat options' window sets them, 18 of 19; a purchase from a shop, its question answered yes, 20 of 21; an item moved within a container 1,550 of 1,555 and into another 950 of 957; following what is selected, typed, 1,680 of 1,688; a line said, typed, 135 of 138; an emote, typed or (the away emote) from the friends window's status menu, 200 of 204; a tell, typed as a reply, a tell again or a new tell, 60 of 61; an item worn 38 of 39. Examining is the scripts' own on a selection and the examination window's when it opens, closes or changes its tabs (the classic window or the tabbed one as the player's options say): 106,450 of 106,582 the same, at most 100 differing; the next target the scripts pick in a fight 6,500 of 6,717. The calls the recorded client sent at one moment are made in the order it made them, a selection before the examination it asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SELECTION-SENDS"),
        station: "auberean-client-runtime::corpus::interface::selecting_what_the_recorded_player_selected_sends_what_the_recorded_client_sent",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-connections-measure-carries-the-rate-over-two-measures",
        says: "The connection's measure the connecting screen shows carries the rate received \
               over the last two measures (their bytes over their time), and the packets the last \
               one counted.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-RECEIVE-RATE"),
        station: "auberean-client-runtime::cpu::world::session::the_connections_measure_carries_the_rate_received_over_two_measures",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.the-crafting-page-reads-every-recorded-players-craft-skills",
        says: "For every recorded player in the world, the crafting page's questions run to their \
               end: the craft skills (each also asked on its own, the same skill), each one's \
               level and whether it may be raised, its recipes (each of that craft skill) and their \
               categories' names, and the experience the player may spend on craft skills.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-CRAFT-SKILLS"),
        station: "auberean-client-runtime::corpus::craft::the_crafting_page_reads_every_recorded_players_craft_skills",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-end-of-retail-data-names-no-quality",
        says: "The enumerations the qualities are named by are in the end-of-retail data and hold \
               no names, so a dump lists each value under an empty name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DUMP-NO-NAMES"),
        station: "auberean-client-runtime::corpus::calls::the_end_of_retail_data_names_no_quality",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-ingredient-panel-reads-every-recorded-recipe",
        says: "Before each ingredient a recorded client put in a slot, the recipe opened as the \
               crafting page opens one (the scripts then open the ingredient panel on it) and read \
               at the spinner value it was sent with, as the ingredient panel reads it, runs to its \
               end: the slot is one the recipe takes there, and nearly every ingredient is an item \
               the scripts say fits that slot.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-RECIPES"),
        station: "auberean-client-runtime::corpus::recipes::the_ingredient_panel_reads_every_recorded_recipe",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-journal-holds-the-quests-the-server-sent",
        says: "Through every recorded capture whose server changed a quest, the journal's list, a copy of the list the player's scripts keep asked for through the interface, holds at each log-off and at the end what the server sent: the session start's records, each update copied over its quest's record or added before the first record given earlier, removals taken out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-QUEST-JOURNAL"),
        station: "auberean-client-runtime::corpus::quests::the_journal_holds_every_quest_the_server_gave_newest_given_first",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-log-masks-start-and-change-as-the-scripts-say",
        says: "The client's log masks start with the file's at 420 and the console's at 0; a \
               kind of line is logged when its mask holds all of the kind (kind 4 always to the \
               file); the scripts set a mask, add bits to it and take bits off it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LOG-MASKS"),
        station: "auberean-client-runtime::cpu::scripts::log_system::the_log_masks_start_at_the_clients_and_change_as_the_scripts_say",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.the-pack-the-scripts-list-is-what-the-player-holds",
        says: "At every recorded log-off and at the end of each capture, the player's pack as its scripts list it through the interface is there in nearly every look and holds what the recorded messages have the player hold, but for items whose container the recordings never sent (under one in twenty); what it wears likewise; every listed item the client was sent has its name (the object's description, its own qualities, else its qualities record's description and table). The pack is listed whole and in order, its four segments (the main pack, then three bags) each a window of the list following the one before; most packs hold more than 42 items, up to 78.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-INVENTORY"),
        station: "auberean-client-runtime::corpus::inventory::the_pack_the_scripts_list_is_what_the_player_holds",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-players-commands-go-with-the-session",
        says: "When the character leaves the world the player's command line is made anew: the \
               commands its scripts made known are forgotten, and the next session's scripts make \
               theirs known again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME3-COMMANDS-RESET"),
        station: "auberean-client-runtime::cpu::scripts::distributor::the_players_commands_go_with_the_session",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.the-selection-is-listed-with-the-inventory",
        says: "Each object the recorded 2017 players selected, selected again, is the inventory's selection and is listed among the inventory's items with its facts, its icon among them, as the panels need it to drag the selection from the world into the pack: the first 2,000 selections of an object the client held, every one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SELECTION-LISTED"),
        station: "auberean-client-runtime::corpus::inventory::the_selection_is_listed_with_the_inventory",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-servers-updates-go-to-the-description-first",
        says: "The server's update of a quality the object's description holds (an item's container, its quantity, the scale, the icon, a yes-or-no flag) goes to the description, which answers for it first; the object's own qualities are left as they were. An update of a quality the description does not hold is the object's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-QUALITY-UPDATES"),
        station: "auberean-client-runtime::cpu::world::updates::the_servers_updates_go_to_the_description_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.model.the-skill-window-reads-every-recorded-players-panels",
        says: "For every recorded player in the world, the skill window's questions run to their end: the root panel of each kind of skill its race has (named through the enumerations' names), each root panel's name, icon and nodes from the skill panel manager, and each skill's state through the player's interface functions.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SKILL-PANELS"),
        station: "auberean-client-runtime::corpus::skills::the_skill_window_reads_every_recorded_players_panels",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.the-skill-windows-root-panels-are-known-when-the-player-arrives",
        says: "When the player arrives in the world the client asks the scripts what the skill window \
               shows, the race's root panels among it, once for that player and whatever the scripts \
               said before, as the skill window asks for the root panels when the play screen's \
               panels are made: its tabs open their panels from the first time it shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SKILL-ROOTS"),
        station: "auberean-client-runtime::corpus::skills::the_skill_windows_root_panels_are_known_when_the_player_arrives",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.model.what-the-recorded-player-selected-may-be-selected",
        says: "Each object the recorded 2013 and 2017 players selected is, at that moment, one the client's qualities say may be selected: the object's description answers when it holds the flag, else its own qualities, else its qualities record, whose own description is asked before its tables (50,735 of 51,013 selections; 56 the qualities refuse, each after the server said so in an update, 222 of objects the client did not know).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SELECTABLE"),
        station: "auberean-client-runtime::corpus::interface::what_the_recorded_player_selected_may_be_selected",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.natives.a-chat-windows-filter-gains-and-loses-kinds",
        says: "A chat window's filter gains kinds of text or'ed in (a window with none gets \
               them) and loses kinds cleared from it; taking out of a window with no filter leaves \
               it without one, and a profile holding no filters answers from a new one's; each \
               answers 1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-CHAT-FILTERS"),
        station: "auberean-glue::cpu::options::a_chat_windows_filter_gains_and_loses_kinds_of_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.a-file-reads-as-narrow-text",
        says: "A file a script reads comes as narrow text in the Windows Latin code page: a carriage \
               return before a line feed goes, a Ctrl+Z ends the text, a line stops at a zero, and a \
               line over 4095 characters goes on as a next one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-FILE-READ"),
        station: "auberean-client-runtime::cpu::scripts::files::a_file_reads_as_narrow_text_a_line_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.a-log-line-is-stamped-with-the-local-time",
        says: "A line written with a timestamp starts with the local time, month day year, hours \
               minutes seconds, in angle brackets and two spaces; a host with no files opens and \
               writes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LOG-STAMP"),
        station: "auberean-client-runtime::cpu::scripts::files::a_log_line_is_stamped_with_the_local_time_and_nothing_opens_without_a_disk",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.a-new-icon-packs-a-count-of-nothing",
        says: "A new icon description has no layers and packs as a count of zero, so an object \
               holding one can be copied and sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ICON-NEW"),
        station: "auberean-script::cpu::codec::a_new_icon_description_packs_a_count_of_nothing",
        tier: Tier::Cpu,
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
        id: "script.natives.a-skill-panel-node-keeps-what-it-is-given",
        says: "A skill panel node starts empty with its place at -1 on both axes; each part reads \
               back as set (a name or description as a new copy, the place into two out slots, \
               the parents and dependencies as new lists of words); a parent or dependency listed \
               already is not added again; a node is set from another whole, cleared, and packs \
               as the skill, the type, the name, the description, the icon, the place, the two \
               lists and the three flags.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SKILL-NODE"),
        station: "auberean-glue::cpu::skill_nodes::a_skill_panel_node_keeps_what_it_is_given",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.entity-description-properties",
        says: "An entity description finds a property by id in the first of its groups that has the group the property list files the id under, and nowhere else; asked for one type (a yes-or-no, an integer, a float, a vector, an enumeration value, a data id, a package id, a string reference) it answers 1 and the value for a property of that type, else 0, leaving the out argument as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-ENTITY-PROPERTIES"),
        station: "auberean-glue::cpu::entity::a_property_answers_from_its_listed_group_and_only_for_its_type",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.every-notice-is-read-with-the-words-its-call-sites-push",
        says: "Every notice the library's scripts send to the panels is one the client reads, with as \
               many words as each of its call sites pushes; the first names the object whose panels \
               hear it, except the interface's own three (portal space, its scene, opening the \
               examination panel), which every panel hears and whose words are all the notice's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-NOTICES"),
        station: "auberean-client-runtime::corpus::interface::every_notice_is_read_with_the_words_its_call_sites_push",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "script.natives.examination-line-answers-its-own-kind",
        says: "An examination line answers the value of its own kind and 0 for any other (a time \
               left -1); a line not yet given an element is drawn with the examination's text \
               template; a copy of a profile holds its own lines and tabs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-EXAM-LINE"),
        station: "auberean-glue::cpu::examination::a_line_answers_only_the_value_of_its_own_kind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.examination-profile-keeps-the-asked-tabs-lines",
        says: "An examination profile adds a line (a number, a float, a yes or no, a string reference, a picture, an icon, indents, a break, a time left, a 64-bit number, an effect) by noting its tabs in the profile's tab word and keeping it, at the end, only when it shows on a tab the profile's request asks for or on the first tab; a new profile answers a request about nothing with no lines; it packs as the request, the lines by kind and the tab word; two profiles are equal when their requests, tab words and every line are; a null profile crashes the context.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-EXAM-PROFILE"),
        station: "auberean-glue::cpu::examination::a_profile_keeps_the_lines_of_the_tabs_asked_and_notes_every_tab",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.examination-request-packs-by-kind",
        says: "An examination request packs its kind and its admin word, then what its kind \
               carries: the object and the tabs asked for (an object), the object, the combat \
               mode, the skill level and the tabs (an attackable target), the enumerated datum or \
               the data id (the kinds that name one), nothing for a request about nothing, which \
               is what a new request is, asking for the default tabs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-EXAM-REQUEST"),
        station: "auberean-client-runtime::cpu::world::requests::an_examination_request_packs_what_its_kind_carries",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.files-scripts-write",
        says: "A file a script writes starts with the byte-order mark and holds 16-bit units; each line feed is written after a carriage return and a line that does not end in one gets both; writing to a file that is not open opens it to add to it, which keeps what it holds unless that does not start with the mark; opening to read needs the file; closing answers whether it was open.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-FILES"),
        station: "auberean-client-runtime::cpu::scripts::files::a_file_is_marked_and_its_lines_end_in_a_carriage_return_and_a_line_feed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.files-stay-in-the-settings-folder",
        says: "A file the scripts name is a bare name in the client's own folder: a name with a \
               folder, a drive, a dot or two, or nothing is refused.",
        since: THIS_CLIENT,
        divergence: "CD2-files-stay-in-settings-folder",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-BARE-NAMES"),
        station: "auberean-client-runtime::cpu::scripts::files::a_file_is_named_by_a_bare_name_and_any_other_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.options-export-and-import-through-the-hosts-files",
        says: "Exporting the options writes them to a file and importing reads one back into the \
               profile, each answering 0; a file not written, not read or not named answers the \
               client's error, and a failed import leaves the profile as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-OPTIONS-FILES"),
        station: "auberean-glue::cpu::options::the_options_export_to_a_file_and_import_back_and_answer_the_clients_errors",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.reading-a-file-uses-it-up",
        says: "Reading a file that is not open opens it to read, and fails when there is none; reading \
               closes the stream while its name stays open, so it gives nothing more until it is \
               closed, and a file open to write gives nothing and is used up too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-FILE-READ-USED-UP"),
        station: "auberean-client-runtime::cpu::scripts::files::reading_a_file_opens_it_and_uses_it_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.selection-packs-its-numbers-by-type",
        says: "A selection packs its type, then, only for the type that carries them, its current health, the damage taken from other players, its maximum health, its current vigor, the vigor lost to other players and its maximum vigor; a new selection is all zeros.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SELECTION-INFO"),
        station: "auberean-client-runtime::cpu::world::requests::a_selection_packs_its_numbers_only_for_the_type_that_carries_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.shortcuts-set-from-a-list-and-saved-places-cleared",
        says: "A profile's shortcuts set from a list of shortcuts become a copy of each, in the \
               list's order; clearing a table of the windows' saved places takes every window out \
               and keeps the table's size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SHORTCUT-ARRAY"),
        station: "auberean-glue::cpu::options::a_profiles_shortcuts_are_set_from_a_list_and_saved_places_clear",
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
        id: "script.natives.the-chat-settings-are-set-over-a-new-players",
        says: "The options profile's chat setters set a window's filter or devoted channel, or a \
               kind of text's font size, color or popup flag (its low byte), answering 1; a table \
               the profile does not hold yet is first made as a new profile holds it, so the \
               other windows and kinds keep a new player's values.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-CHAT-SETTERS"),
        station: "auberean-glue::cpu::options::the_chat_settings_are_set_over_a_new_players",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.the-client-host-keeps-files-in-its-folder",
        says: "The client's host refuses a file name that leaves its folder even when the front end \
               would take it, opens nothing when the front end gives no files, and writes a \
               stamped line to the front end's folder.",
        since: THIS_CLIENT,
        divergence: "CD2-files-stay-in-settings-folder",
        evidence: Evidence::Private("AC2-EVID-SCRIPT-HOST-FILES"),
        station: "auberean-client-runtime::cpu::scripts::files::the_client_host_refuses_names_that_leave_its_folder_and_writes_stamped_lines",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.the-stand-ins-left-to-a-host-are-the-ones-the-client-binds",
        says: "The shared natives leave to a host's own model exactly the natives the client binds from its model, and the client binds no other over one of their stand-ins (the panel notices aside).",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-HOST-STANDINS"),
        station: "auberean-client-runtime::cpu::scripts::natives::the_stand_ins_left_to_a_host_are_the_natives_the_client_binds_from_its_model",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.the-world-clock-answers-the-calendars-values",
        says: "The world clock's natives answer whether it is night, the time of day, the day's and \
               year's lengths, the day and the year; a null receiver crashes each but the night's, \
               and with no clock each answers 0.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-NATIVES"),
        station: "auberean-client-runtime::cpu::scripts::game_time::the_clocks_natives_answer_its_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.the-world-clock-takes-an-admins-commands",
        says: "An admin's two commands move the time of day and dilate time, answering whether they \
               could: a rate at or below 0.0002 is refused and with no clock neither is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-COMMANDS"),
        station: "auberean-client-runtime::cpu::scripts::game_time::an_admins_commands_move_the_clock_and_answer_whether_they_could",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.the-world-clock-writes-the-date-and-time-as-string-references",
        says: "The date and time are written into two string references with the calendar's strings' \
               ids and their variables; with no clock nothing is written and the answer is 0.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-STRINGS"),
        station: "auberean-client-runtime::cpu::scripts::game_time::the_clock_writes_the_date_and_time_as_string_references",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.natives.two-options-are-on-with-their-bit-clear",
        says: "The pet panel's and fellows on the map's options read their bit the other way: \
               a cleared profile has them on, and turning one off sets its bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-OPTIONS-OFF-WHEN-SET"),
        station: "auberean-glue::cpu::options::two_options_are_on_with_their_bit_clear",
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
    behaviour! {
        id: "script.options.the-windows-saved-places-add-and-pack-as-the-engines",
        says: "A new table of the windows' saved places packs as no window in 47 buckets; a place \
               added goes to the head of its chain in a new table that is copied in as its \
               window's (so two elements in one bucket stand in the order added), one for an \
               element held is refused, and each packs as its four corners and a shown word \
               under its window and element.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-SAVED-PLACES"),
        station: "auberean-script::cpu::options::the_windows_saved_places_add_and_pack_as_the_engines",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "script.replay.server-calls-run-against-the-model",
        says: "Through every recorded 2017 game login replayed through the client, the server's \
               calls into the client's scripts run against the model to their end or stop at a \
               named native not written yet, never failing the machine.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-REPLAY-CALLS"),
        station: "auberean-headless::corpus::sessions::the_servers_calls_run_against_the_model",
        tier: Tier::Corpus,
    },
];
