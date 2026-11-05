//! The data files: the container, record types, and how each type's records are laid out.
//!
//! A layout row claims that every record of its type, in the end-of-retail set of data files, is
//! read with every byte consumed by the layout the decoder implements, as the September 2013 client
//! reads it.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "data.action-map.every-record-decodes",
        says: "The action map reads as its string table, the pairs of actions that \
               toggle each other, what each action does with each set of modifiers, and \
               each action's name and description, no key repeated in any list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ACTION-MAP"),
        station: "auberean-assets::dat::game::ui::every_action_map_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.anim-map.every-record-decodes",
        says: "Every animation map reads as its id and a table from an animation key to an \
               animation record.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ANIM-MAP"),
        station: "auberean-assets::dat::look::animation::every_animation_map_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.animation.every-record-decodes",
        says: "Every animation reads as its id, the kind of animator and what that kind stores: \
               keyframed bone tracks with their effect, hook and impulse events, a blend along \
               sliders, a body's animation, a head turn, a random pick, a switch, or bone angles.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ANIMATION"),
        station: "auberean-assets::dat::look::animation::every_animation_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.appearance.every-record-decodes",
        says: "Every appearance table reads as its base appearances and, per appearance key, what \
               each strength step changes: material modifiers, opacity, meshes, part groups, \
               effects and icon layers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-APPEARANCE"),
        station: "auberean-assets::dat::look::visual::every_appearance_table_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.behavior-table.every-record-decodes",
        says: "Every behavior table reads as its mode-change blend times, the animations each \
               element, mode and behavior play, the mode changes with their behaviors and \
               effects, the layers, the modes, the behaviors, and the initial behaviors and mode.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-BEHAVIOR-TABLE"),
        station: "auberean-assets::dat::look::animation::every_behavior_table_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.block-map.every-record-decodes",
        says: "Every block map reads as a height, a width and one byte per cell.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-BLOCK-MAP"),
        station: "auberean-assets::dat::world::region::every_block_map_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.block-property.a-color-blends-between-the-corners",
        says: "A color property laid over blocks blends channel by channel between the block's four \
               corners, alpha among them; a corner of another type gives no value.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-BLOCK-PROPERTY-COLOR"),
        station: "auberean-physics::cpu::geometry::material::a_color_laid_over_blocks_blends_between_the_corners",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.a-created-objects-state-names-its-records",
        says: "Every saved state a created object's records bring, or name in turn, is asked by the \
               player's scripts which records it needs, and each answer runs to its end. Given the \
               gaps the recorded 2013 and 2017 clients' files had, the client asks the server for no \
               picture, sound or saved state at a creation: the records a state names that the \
               recorded clients asked for came when a story played, not at a creation.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CACHE-STATE-RECORDS-REPLAY"),
        station: "auberean-client-runtime::corpus::calls::every_created_objects_saved_state_names_its_records_and_asks_for_none_the_recorded_clients_did_not",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "data.cache.a-qualities-record-that-comes-later-is-read",
        says: "Our choice: a qualities record no file holds answers nothing until the server sends \
               it, then is read through the one cache, also by an object made before it came. \
               Retail's lookup remembers no miss; what its older objects do is not known.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-CACHE-QUALITIES-LATER"),
        station: "auberean-client-runtime::cpu::world::cache::a_qualities_record_the_server_sends_later_is_read_by_an_object_made_before_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.an-objects-creation-asks-for-its-records",
        says: "An object's creation asks the data cache for the records it is made from (its visual \
               description's, its script state and qualities records by its entity or package, and \
               the entity); those the files lack are asked of the server, and a refused one is \
               asked for again only by the next object made from it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CACHE-CREATION"),
        station: "auberean-client-runtime::cpu::world::cache::an_objects_creation_asks_the_server_for_the_records_its_files_lack_and_a_refusal_ends_the_ask",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.each-asker-has-its-own-number",
        says: "Every asker of the data cache's requests (an object's creation, the text elements' \
               string downloads, the scene's cells) has a number of its own, and is told only its \
               own batches.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-CACHE-LISTENERS"),
        station: "auberean-client-runtime::cpu::world::cache::each_asker_of_the_data_cache_has_a_number_of_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.objects-hold-the-records-they-are-made-from",
        says: "Once an object's records have all come, the object takes each one that came into the \
               data cache, and the records each names that came with it, and holds them while it \
               lives: the next object made from them finds them in memory and reads none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CACHE-OBJECT-RECORDS"),
        station: "auberean-client-runtime::cpu::world::cache::an_object_holds_the_records_it_was_made_from_and_the_next_finds_them_in_memory",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.objects-made-before-their-records",
        says: "An object whose records are still being asked for is made at once; its records come \
               into the data cache behind it.",
        since: THIS_CLIENT,
        divergence: "CD2-objects-made-before-their-records",
        evidence: Evidence::Private("AC2-EVID-CACHE-CREATION-AT-ONCE"),
        station: "auberean-client-runtime::cpu::world::cache::an_object_is_made_at_once_while_its_records_are_asked_for",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.one-cache-for-the-whole-client",
        says: "The client has one data cache: a record the server sends for a request is read from \
               then on by the scripts through the same records' caches, and a string table it sends \
               is written from as the files' own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CACHE-ONE"),
        station: "auberean-client-runtime::cpu::world::cache::a_record_the_server_sends_is_read_by_the_scripts_and_their_text_through_the_one_cache",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.reads-see-records-stored-for-the-session",
        says: "Records are read through the data files as the cache stores into them: a record \
               stored after the records' caches were made is read as the files' own are.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CACHE-READS"),
        station: "auberean-assets::cpu::requests::a_record_stored_for_the_session_is_read_by_a_database_over_the_stores",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.cache.scripts-find-an-object-after-the-next-upkeep",
        says: "Scripts look for an object in vain from its creation until the next upkeep.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CACHE-BEING-MADE"),
        station: "auberean-client-runtime::cpu::world::cache::scripts_find_an_object_after_the_next_upkeep",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.camera-fx.every-record-decodes",
        says: "Every camera effect reads as a 64-bit word saying which settings follow, then those \
               settings: shake, the camera modes, intensity, period, ramps, blur, flash and distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-CAMERA-FX"),
        station: "auberean-assets::dat::look::fx::every_camera_effect_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.cell-mesh.every-record-decodes",
        says: "Every cell's geometry reads as a mesh, then the cell's own inside test as a tree of \
               splitting planes, its doorways (a side, a polygon and a bounding sphere each) and an \
               info record id.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-CELL-MESH"),
        station: "auberean-assets::dat::look::mesh::every_cell_mesh_decodes_exactly_with_its_tree_and_portals",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.cell.every-record-decodes",
        says: "Every interior cell reads as its frame, flags, mesh, doorways, the cells it sees \
               and shares, and its entities and properties when flagged, with nothing left over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-CELL"),
        station: "auberean-assets::dat::world::landblock::every_interior_cell_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.character-template.every-record-decodes",
        says: "The character template reads as every behavior with its flags, every slider with \
               its controller, every mode with its combat flag, the default element and mode, and \
               the sliders the player controls.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-CHARACTER-TEMPLATE"),
        station: "auberean-assets::dat::look::animation::the_character_template_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.client-files.opened-as-the-client-opens-them",
        says: "The client opens the portal and the high-resolution pictures where it finds them, a \
               region's cell file (made when it is missing, replaced when the region changes) and \
               a language's file (which must be there), each checked against the program's data \
               versions.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CLIENT-FILES"),
        station: "auberean-dat::cpu::client_files::a_regions_cell_file_is_made_when_missing_and_replaced_by_another_regions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.container.every-file-walks-soundly",
        says: "Every data file of the end-of-retail set is a sound container: its directory is \
               ordered with all leaves at one depth, and every block is either in a record or on \
               the free list the header counts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-CONTAINER"),
        station: "auberean-dat::dat::container::every_end_of_retail_data_file_walks_soundly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.container.iteration-list-holds-negative-versions",
        says: "An iteration list may hold data versions below zero, as the 2005 files' do: they read \
               back negative, all below the positive versions.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ITERATIONS"),
        station: "auberean-dat::cpu::iterations::versions_below_zero_read_back_negative_below_the_positive_run",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.container.iteration-list-is-well-formed",
        says: "Every data file holds a well-formed list of the data versions it contains.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ITERATIONS-FORM"),
        station: "auberean-dat::dat::container::every_data_file_holds_a_well_formed_iteration_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.container.tree-descent-finds-every-record",
        says: "Looking a record up by descending the directory tree finds exactly the entry a \
               full walk of the directory finds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LOOKUP"),
        station: "auberean-dat::dat::container::a_tree_descent_finds_every_record_the_walk_found",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.database.an-enum-map-names-its-base-record",
        says: "An enum map names its base map's record among the records it needs, and one with no \
               base names none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DB-ENUM-MAP-BASE"),
        station: "auberean-assets::cpu::requests::an_enum_map_names_its_base_record",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.database.data-graph",
        says: "A record that loads goes into an installed data graph as a node with an edge to each \
               record it needs; a cell file's record, or one of 24 types \
               (the region's parts, id and enum maps, categories and others), does not; a category \
               record links to the categories' enum map only when filled in directly.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DB-DATA-GRAPH"),
        station: "auberean-assets::cpu::graph::a_loaded_record_is_a_node_with_an_edge_to_each_record_it_needs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.database.enum-map-extends-its-base",
        says: "An enum map lists its own names by value and then its base map's, and a name or \
               value missing from it is looked for in its base map.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENUM-MAP-BASE"),
        station: "auberean-assets::dat::game::database::an_enum_map_holds_its_own_names_then_its_base_maps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.database.every-cached-type-has-a-decoder",
        says: "Every record type the client's cache start makes an object cache for has a decoder.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DATA-CACHE-TYPES"),
        station: "auberean-assets::cpu::cache::every_record_type_the_client_makes_a_cache_for_has_a_decoder",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.database.free-list-keeps-released-records",
        says: "A record nobody holds any more stays loaded on its type's free list, so asking for \
               it again finds it in memory; the list keeps at most the type's greatest number, the \
               oldest freed going first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DB-FREE-LIST"),
        station: "auberean-assets::cpu::cache::a_record_nobody_holds_stays_loaded_until_the_free_list_overflows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.database.free-records-go-after-thirty-seconds",
        says: "Past its type's ideal number of free records, the oldest one goes once it was freed \
               more than thirty seconds before, one each time the caches are given time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DB-FREE-TIME"),
        station: "auberean-assets::cpu::cache::past_the_ideal_number_the_oldest_free_record_goes_thirty_seconds_after_it_was_freed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.database.no-saving",
        says: "The client's database never saves a record and never says it reads only in the \
               foreground; its caches have no last words. A category record's names compare without \
               case.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DB-NO-SAVING"),
        station: "auberean-assets::cpu::graph::the_database_saves_nothing_and_a_category_name_ignores_case",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.database.no-word-filter-at-end-of-retail",
        says: "The end-of-retail files hold no word filter and the master id map names none, so \
               every name check the client makes against the filter answers that there is no \
               filter.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DB-NO-WORD-FILTER"),
        station: "auberean-assets::dat::game::database::the_end_of_retail_files_name_no_word_filter",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.database.not-keeping-free-records",
        says: "A cache told not to keep free records throws away the ones it has and every record \
               whose last holder lets go, as it does a record kept off the free list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DB-KEEP-FREE"),
        station: "auberean-assets::cpu::cache::a_cache_told_not_to_keep_free_records_throws_them_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.database.property-list-by-enumeration",
        says: "The property list is the record its enumeration value names through the master id \
               map the portal file's header points at, and the language's rules are found the same \
               way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-DB-ENUM"),
        station: "auberean-assets::dat::game::database::the_property_list_is_the_record_its_enumeration_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.database.property-names-type-and-group-names-tools-only",
        says: "Every property's name maps back to it, letters in any case; the value types' and \
               groups' names are in enum maps only the tools' half of the enum mapping names, \
               which no client file holds, so the game can name neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-DB-NAMES"),
        station: "auberean-assets::dat::game::database::property_names_resolve_but_value_type_and_group_names_are_not_in_the_files",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.day-desc.every-record-decodes",
        says: "Every day of the sky reads as a version stored before the id, its times of day with \
               their fog, light and rain, and its sky objects with their arcs and lens flares.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-DAY-DESC"),
        station: "auberean-assets::dat::world::region::every_day_of_the_sky_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.detail-map.every-record-decodes",
        says: "Every detail map reads as a block map and rows, each a value, a picture, a rectangle \
               and the map notes it shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-DETAIL-MAP"),
        station: "auberean-assets::dat::world::region::every_detail_map_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.disk.a-crashed-write-is-put-right-on-opening",
        says: "A data file whose writing stopped part way is put right when it is next opened: the change \
               recorded before it began is finished or undone. One whose rotation was cut short does not \
               open.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-RECOVER"),
        station: "auberean-dat::cpu::disk::a_write_cut_short_anywhere_is_finished_or_undone_on_opening_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.a-file-is-looked-for-in-four-places",
        says: "A data file is looked for in the folder given, then in the working folder, the folder the \
               PROJECT_DAT variable names and the folder the registry names, each with a slash added.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-LOOK"),
        station: "auberean-dat::cpu::disk::a_file_is_looked_for_in_its_folder_the_working_folder_the_variable_and_the_registry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.a-file-of-another-game-version-is-not-opened",
        says: "A data file whose game version isn't the one the client reads (3, and 0 for a cell \
               file) counts as not there: with no portal or cell file nothing opens.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DATA-GAME-VERSION"),
        station: "auberean-interface::dat::data::a_file_of_another_game_version_is_not_opened",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.disk.a-file-without-a-version-list-is-closed",
        says: "A data file opened reads the list of data versions it holds; one without a list that reads \
               is closed again and a notice shown.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-SHARED"),
        station: "auberean-dat::cpu::disk::a_shared_file_reads_its_data_versions_on_opening_and_one_without_them_is_closed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.a-record-is-looked-for-in-its-file",
        says: "A record is looked for in the file named for it, else by its type: a portal record in the \
               high-resolution pictures when that file holds it, else the portal; a cell record in the \
               cell file; a local one in the language's file.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-WHICH-FILE"),
        station: "auberean-dat::cpu::disk::a_record_is_looked_for_in_the_file_its_type_belongs_to",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.a-removal-that-empties-the-root-fails",
        says: "Removing a record fails, leaving it, when the merge on the way down empties the directory's \
               root; asked again, it goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-ROOT-MERGE"),
        station: "auberean-dat::cpu::disk::a_removal_that_merges_away_the_root_fails_and_the_next_one_succeeds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.a-retail-file-is-written-as-the-client-writes-it",
        says: "A retail data file given records to store, replace and remove ends byte for byte as the \
               client leaves it, and still reads soundly.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-RETAIL"),
        station: "auberean-dat::dat::disk::the_retail_cell_file_takes_records_and_removals_as_the_clients_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.disk.files-only-read-are-read-in-chunks",
        says: "A data file only read is read in chunks of 32 kilobytes, the last 32 kept: a record \
               and the directory's node that leads to it come from a read or two of the file, not \
               one read a block, and read again they read the file no more.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-DISK-CHUNKS"),
        station: "auberean-dat::cpu::disk::a_file_only_read_is_read_in_chunks_not_a_block_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.files-only-read-are-written-in-memory",
        says: "A data file only read takes stored, replaced and removed records in memory, growing \
               there past its end, and reads them back; the file underneath is unchanged.",
        since: THIS_CLIENT,
        divergence: "CD2-files-written-in-memory",
        evidence: Evidence::Private("AC2-EVID-DISK-OVERLAY"),
        station: "auberean-dat::cpu::disk::a_file_only_read_takes_records_in_memory_and_stays_as_it_was",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.opening-checks-versions-and-access",
        says: "A data file of other data versions than asked is refused, or made anew when asked to be; one \
               opened only to read takes no record and is not written.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-OPEN"),
        station: "auberean-dat::cpu::disk::a_file_of_other_data_versions_is_refused_or_made_anew_and_a_read_only_one_takes_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.disk.writes-as-the-client-writes",
        says: "Records stored, replaced and removed leave a data file byte for byte as the client leaves it: \
               blocks from the head of the free list, the directory split and rebalanced on the way \
               down, and the file grown a megabyte when it runs short.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DISK-WRITE"),
        station: "auberean-dat::cpu::disk::a_tree_two_levels_deep_splits_rotates_and_merges_as_the_clients_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.download.a-bad-record-is-asked-again",
        says: "A downloaded record that does not unpack is asked for again; one that does not store shows a \
               notice and stops the client.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DDD-RETRY"),
        station: "auberean-dat::cpu::download::a_record_that_does_not_unpack_is_asked_again_and_one_that_does_not_store_stops_the_client",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.download.asked-which-data-is-held",
        says: "Asked which data it holds, the client answers each open file's data versions, read from the \
               file, in the files' order; when one does not read, it answers nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DDD-ASK"),
        station: "auberean-dat::cpu::download::asked_which_data_it_holds_the_client_answers_each_files_versions_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.download.drops-a-landblock-with-its-block",
        says: "A download's dropped landblock takes every record of its block with it; any other dropped \
               record goes alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DDD-PURGE"),
        station: "auberean-dat::cpu::download::a_dropped_landblock_takes_every_record_of_its_block_and_other_drops_go_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.download.ids-typed-by-their-file",
        says: "A download's lists carry ids only: each is typed by its file, a cell file's by the id's low \
               word, else by its range, with the file's default when none claims it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DDD-IDS"),
        station: "auberean-dat::cpu::download::a_downloads_ids_are_typed_by_their_file",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.download.leaving-the-server-resets-the-cache",
        says: "Leaving the server resets the data cache: the download is idle whatever it was doing, \
               and every batch and kept get is let go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CACHE-RESET-ON-DISCONNECT"),
        station: "auberean-client-runtime::cpu::world::cache::leaving_the_server_resets_the_data_cache",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.download.lists-a-servers-library-as-held",
        says: "Asked which data it holds, the client lists the iterations of a server's library for its \
               portal file beside those the file has had, every time, and no other file's list changes.",
        since: THIS_CLIENT,
        divergence: "CD2-lists-iterations-it-need-not-hold",
        evidence: Evidence::Private("AC2-EVID-DDD-LISTS-SERVER-LIBRARY"),
        station: "auberean-client-runtime::cpu::world::download::the_portal_file_is_listed_as_holding_a_servers_library",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.download.records-stored-and-checked-off",
        says: "Each downloaded record is stored in its file and checked off; records that came before the \
               download began are checked off when it begins; a file whose records have all come, or \
               that had none to fetch, takes its new data version; with nothing left to wait for, the \
               client says the download is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DDD-STORE"),
        station: "auberean-dat::cpu::download::a_download_stores_each_record_checks_it_off_and_gives_each_file_its_version",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.encoded-wave.every-record-decodes",
        says: "Every encoded sound in a language file reads as a byte count and that many bytes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENCODED-WAVE"),
        station: "auberean-assets::dat::game::sound::every_encoded_wave_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.encounter-desc.every-record-decodes",
        says: "The region's encounters read as a version stored before the id, a block map, and \
               rows of encounters with their exceptions by terrain and by water.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENCOUNTER-DESC"),
        station: "auberean-assets::dat::world::region::every_region_encounter_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.entity.derived-holds-its-parents-properties",
        says: "An entity whose object is another entity holds that entity's properties (and so its \
               ancestors') beneath its own: a property it stores itself wins, every other comes \
               from the chain, and it shows as the visual description at the chain's end.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENTITY-CHAIN"),
        station: "auberean-assets::dat::game::property::a_derived_entity_holds_its_parents_properties_beneath_its_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.entity.every-record-decodes",
        says: "Every standalone entity reads as a flags word and the fields it flags, its own id \
               among them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENTITY"),
        station: "auberean-assets::dat::world::entity::every_entity_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.entity.written-with-the-flags-its-fields-call-for",
        says: "An entity is written with the flags its fields call for, not the ones it was read \
               with: its id, kind, object and version when not 0, its instance when it has one, \
               its offset when off the identity and its scale when off 1 (by 0.0002 or more), its \
               properties when it has a group, its name when not empty. Every retail entity's \
               stored flags are these.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENTITY-PACK-FLAGS"),
        station: "auberean-assets::dat::world::entity::every_entitys_flags_are_the_ones_its_fields_call_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.enum-map.every-record-decodes",
        says: "Every enum map reads as its id, its base map and a list of values with their names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENUM-MAP"),
        station: "auberean-assets::dat::game::strings::every_enum_map_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.file-id-table.every-record-decodes",
        says: "The file-id table reads as a list of record types, each with its name and its \
               records' ids and source names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-FILE-ID-TABLE"),
        station: "auberean-assets::dat::game::tables::the_file_id_table_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.file-id-table.names-find-their-ids",
        says: "Read as the client reads it, the file-id table names its music and script library files: \
               every name finds its id whatever its folders and letter case, and every id names its \
               file.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAMES-FILE-IDS"),
        station: "auberean-assets::dat::game::tables::every_name_in_the_file_id_table_finds_its_id",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.file-id-table.read-as-the-client-reads-it",
        says: "The file-id table is read into a table of names: each type's entry named, each name given \
               its id by the table's rules, registering no type.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAMES-FILE-IDS-READ"),
        station: "auberean-assets::cpu::names::the_file_id_table_record_reads_into_a_names_table_by_the_tables_rules",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.file-names.start-empty-and-answer-nothing",
        says: "The client's table of every record's file name, started with folders off, ids not to be \
               made and no file of names, holds nothing: every id asked of it is 0 and every name \
               and path empty; the types asked about are registered once, and the last stop forgets \
               them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FILE-NAMES-EMPTY"),
        station: "auberean-assets::cpu::names::the_clients_table_of_file_names_starts_empty_and_answers_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.file-names.the-scripts-ask-the-clients-table",
        says: "A script's question about a file's id or name goes to the client's table of file names, \
               started with its data cache, which answers from nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FILE-NAMES-TABLE"),
        station: "auberean-client-runtime::cpu::scripts::names::the_scripts_ask_the_clients_table_of_file_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.file-names.types-registered-with-their-names-and-folders",
        says: "A type is registered under the enum maps' name for it (the two mapping types under names \
               of their own, others UNKNOWN before the data cache starts), with its folders when its \
               records are files of their own; only a folder that names a drive is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FILE-NAMES-REGISTER"),
        station: "auberean-assets::cpu::names::types_register_with_their_own_names_and_no_folder_on_a_players_machine",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.fog-desc.every-record-decodes",
        says: "The region's fog reads as a version stored before the id, four fog maps, an optional \
               default and every fog kind, whose fog and light carry blend multipliers only for the \
               channels that add.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-FOG-DESC"),
        station: "auberean-assets::dat::world::region::every_region_fog_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.font.a-character-finds-its-glyph-or-the-question-marks",
        says: "A font read from the files finds a character's glyph through the map of its                characters, from its least code point to its greatest, a code point held twice                being its last glyph; a character the font lacks is drawn with its question                mark's glyph, or none when it lacks that too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FONT-CHARACTERS"),
        station: "auberean-assets::cpu::font::a_character_finds_its_glyph_or_the_question_marks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.font.a-strings-width-is-its-widest-line",
        says: "A string's width in a font is its widest line, its lines broken at new lines, each                character as wide as the glyph it is drawn with; a font holds a string when its                map has every character of it but new lines.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FONT-WIDTH"),
        station: "auberean-assets::cpu::font::a_strings_width_is_its_widest_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.font.every-record-decodes",
        says: "Every font, in the portal and language files, reads as its sizes, its \
               glyphs and one strip of pixels, a byte a pixel when antialiased and a \
               bit a pixel otherwise.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-FONT"),
        station: "auberean-assets::dat::game::ui::every_font_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.fx-script.every-record-decodes",
        says: "Every effect script reads as its timed effect triggers, its timed impulses and jumps, \
               and its period.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-FX-SCRIPT"),
        station: "auberean-assets::dat::look::fx::every_effect_script_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.fx-table.every-record-decodes",
        says: "Every effect table reads as, per effect, the effects it plays with their variants per \
               terrain, each effect a 64-bit word of the parts it stores, then the tables it extends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-FX-TABLE"),
        station: "auberean-assets::dat::look::fx::every_effect_table_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.game-time.every-record-decodes",
        says: "The calendar reads as the time and year it starts from, the lengths of the day and \
               year, the year's name, the parts of the day, the days of the week and the seasons.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-GAME-TIME"),
        station: "auberean-assets::dat::world::region::every_calendar_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.id-map.every-record-decodes",
        says: "Every id map reads as four tables: numbers to record ids and to names, for the game \
               and again for the tools.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ID-MAP"),
        station: "auberean-assets::dat::game::tables::every_id_map_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.iteration-list.every-record-decodes",
        says: "Every data file's version list reads as a count and its values and runs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ITERATION-LIST"),
        station: "auberean-assets::dat::game::tables::every_iteration_list_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.iteration-list.not-pairs",
        says: "The client's own reader for the iteration list's type wants a count, then that many \
               pairs of words, with no id; every file's iteration list counts versions instead (in \
               single values and runs), so none of them reads that way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-FILE-DATA"),
        station: "auberean-assets::dat::game::tables::no_iteration_list_reads_as_the_clients_pairs",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.key-map.an-older-plain-map-gains-reply-on-r",
        says: "The plain key map is read with one update: a file older than version 1 gains R as \
               the reply key when R is free in it, and the client then writes the file again; a \
               file of version 1, or one binding R, gains nothing. After a preprocessor header the \
               version is read from the header's node.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEY-MAP-VERSION"),
        station: "auberean-assets::cpu::keymap::an_older_plain_map_gains_reply_on_r_when_r_is_free",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.key-map.every-record-decodes",
        says: "Every key map reads as a name, an identity and a list of bindings, each \
               an action, an input and a name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-KEY-MAP"),
        station: "auberean-assets::dat::game::ui::every_key_map_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.key-map.every-record-reads-back-through-its-file",
        says: "Every key map of the data, written as the client writes a key map's file and read \
               back as it reads one, keeps its name, its identity and every binding in order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEY-MAP-ROUND-TRIP"),
        station: "auberean-assets::dat::game::keymap::every_key_map_reads_back_through_the_file_the_client_writes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.key-map.file-reads-as-the-client-reads-it",
        says: "A key map's file is an optional version, the name, an identity of eleven whole \
               numbers, then bindings of exactly two names, an action the input actions \
               enumeration knows and an input of the client's names; other nodes are skipped and \
               of two bindings of one input the first is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEY-MAP-FILE-READ"),
        station: "auberean-assets::cpu::keymap::a_key_map_file_keeps_known_bindings_the_first_of_an_input_winning",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.key-map.records-write-back-to-their-bytes",
        says: "Every key map and the action map, read and written back, give their records' \
               bytes; the action map's lists go in key order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-KEY-MAP-PACK"),
        station: "auberean-assets::dat::game::keymap::every_key_map_and_the_action_map_write_back_to_their_bytes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.key-map.written-as-the-client-saves-one",
        says: "A key map is saved as version 1, its name, its identity's eleven parts in decimal \
               and a binding a line, each line after a blank one, with Windows line ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEY-MAP-FILE-WRITE"),
        station: "auberean-assets::cpu::keymap::a_key_map_is_written_as_the_client_saves_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.landblock-info.every-record-decodes",
        says: "Every landblock's placed objects read as flags before the id, the cell count, the \
               light record, an entity group, properties and the cells seen from outside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LANDBLOCK-INFO"),
        station: "auberean-assets::dat::world::landblock::every_landblock_info_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.landblock-info.every-record-packs-back-to-its-bytes",
        says: "Every landblock and every landblock's record of its placed objects, read and written \
               again, are their own bytes.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-LANDBLOCK-PACKS-BACK"),
        station: "auberean-assets::dat::world::landblock::every_landblock_and_landblock_info_packs_back_to_its_bytes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.landblock.every-record-decodes",
        says: "Every landblock reads as 17 by 17 height bytes and 17 by 17 vertex words.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LANDBLOCK"),
        station: "auberean-assets::dat::world::landblock::every_landblock_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.landscape.every-record-decodes",
        says: "The landscape constants read as the land cell size, cells per landblock side, vertex \
               intervals per cell, object and sky heights, road width, the vertex word's six field \
               widths and 256 heights.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LANDSCAPE"),
        station: "auberean-assets::dat::world::region::every_landscape_constants_record_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.layout.every-record-decodes",
        says: "Every interface layout, in every language file, reads as a tree of \
               elements whose states carry attributes, media, transitions and child \
               elements.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LAYOUT"),
        station: "auberean-assets::dat::game::ui::every_layout_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.layout.every-record-packs-back-to-its-bytes",
        says: "Every interface layout of every language file, read and written back, gives its \
               record's bytes, and the size it says it takes is that many.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LAYOUT-PACK"),
        station: "auberean-assets::dat::game::ui::every_layout_packs_back_to_its_bytes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.light-info.every-record-decodes",
        says: "Every landblock's light record reads as its lights with the cells each reaches, a \
               table of the lights reaching each cell, and the sunlight when it is on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LIGHT-INFO"),
        station: "auberean-assets::dat::world::landblock::every_light_info_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.light-info.lit-vertices-unpack-to-their-count-and-pack-back",
        says: "A lit cell's packed vertices unpack to as many numbers as it says, and those numbers \
               pack back to the same words.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-LIT-LISTS"),
        station: "auberean-assets::dat::world::landblock::every_lit_list_unpacks_to_its_count_and_packs_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.map-notes.every-record-decodes",
        says: "Every set of map notes reads as a string table and its labels, each a text token, an \
               icon and a position on the map.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MAP-NOTES"),
        station: "auberean-assets::dat::world::region::every_set_of_map_notes_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.master-map.data-folders",
        says: "The type table is set up once and let go; a type's source folder is the game's or the \
               engine's folder followed by the type's folder name, or empty for a type with none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MASTER-MAP-FOLDERS"),
        station: "auberean-dat::cpu::types::a_types_folders_join_the_games_or_engines_folder_and_its_folder_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.master-map.has-file-and-pack-version",
        says: "Every type's pack version is 1, and every type's records are files of their own to the \
               authoring tools but the landblock's, the cell's, the file-id table's, the qualities' \
               and the iteration list's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MASTER-MAP-FIELDS"),
        station: "auberean-dat::cpu::types::every_type_but_five_keeps_its_records_as_files_and_every_pack_version_is_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.material-instance.every-record-decodes",
        says: "Every material instance reads as the material it draws with, its kind and the \
               modifiers applied to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MATERIAL-INSTANCE"),
        station: "auberean-assets::dat::look::material::every_material_instance_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.material-modifier.every-record-decodes",
        says: "Every material modifier reads as a list of named parameters, each with a data type, \
               its data and its fields.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MATERIAL-MODIFIER"),
        station: "auberean-assets::dat::look::material::every_material_modifier_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.material.every-record-decodes",
        says: "Every material reads as its parameters and its layers, each layer with its texture \
               stages, coordinate animations, blend, depth and lighting state, colors and shaders.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MATERIAL"),
        station: "auberean-assets::dat::look::material::every_material_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.mesh.every-record-decodes",
        says: "Every mesh reads as its kind, materials, vertices, bounding sphere and levels of \
               detail, then its polygons with a collision tree over them when there are any, then \
               its physics polygons and, when flagged, the physics tree over them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MESH"),
        station: "auberean-assets::dat::look::mesh::every_mesh_decodes_exactly_with_its_polygons_and_trees",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.motion-desc.every-record-decodes",
        says: "Every motion description reads as which motion set each element and mode uses, \
               the sets of motion values per terrain and modifier, and the sea-floor and water \
               multipliers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MOTION-DESC"),
        station: "auberean-assets::dat::look::animation::every_motion_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.music-info.every-record-decodes",
        says: "Every music setting reads as five words: flags, priority, fade distance, kind and \
               the music it plays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MUSIC-INFO"),
        station: "auberean-assets::dat::game::sound::every_music_info_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.music.every-record-decodes",
        says: "Every piece of music reads as the music file's bytes, its kind and the \
               records it uses.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-MUSIC"),
        station: "auberean-assets::dat::game::sound::every_piece_of_music_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.name-table.assign-only-by-extension",
        says: "A name is given an id only when its extension is the type's, kept without its folders \
               and in lower case; a name that has an id keeps it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAMES-ASSIGN"),
        station: "auberean-assets::cpu::names::a_files_name_is_given_its_id_only_when_its_extension_is_the_types",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.name-table.first-name-keeps-the-id",
        says: "An id keeps the first name given it: a later name given the same id has the id too, but \
               the type lists the first under it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAMES-FIRST"),
        station: "auberean-assets::cpu::names::an_id_keeps_the_first_name_given_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.name-table.lookup-without-folders-or-case",
        says: "A name is looked up without its folders and with letter case ignored, whatever type is \
               asked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAMES-LOOKUP"),
        station: "auberean-assets::cpu::names::a_name_is_looked_up_without_its_folders_or_letter_case",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.name-table.register-keeps-drive-folders",
        says: "A type registers once; of its folders only one that names a drive is kept, lowered with \
               its slashes turned, and the type counts as searched only when such folders were.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAMES-REGISTER"),
        station: "auberean-assets::cpu::names::a_type_registers_once_and_keeps_only_folders_that_name_a_drive",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.overlay.checks-its-files-against-what-it-is-told",
        says: "Told the records a world's overlay was made over, the client holding them goes on                and one holding others stops, saying why.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-OVERLAY-CHECKS-RECORDS"),
        station: "auberean-client-runtime::cpu::world::download::a_client_told_records_it_doesnt_hold_stops_saying_why",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.overlay.none-reads-the-files",
        says: "A file with no overlay over it is read as it always was, in a folder that holds one                over another file too.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-OVERLAY-NONE"),
        station: "auberean-dat::cpu::overlay::a_file_with_no_overlay_over_it_reads_as_it_did",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.overlay.over-the-retail-files",
        says: "Laid over the end-of-retail cell file, an overlay replaces, adds and deletes records                and leaves every other record and the file's data versions as they were.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-OVERLAY-RETAIL-FILES"),
        station: "auberean-dat::dat::overlay::a_world_replaces_and_deletes_records_of_the_retail_cell_file",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.overlay.reads-agree",
        says: "A record a world's overlay replaces, adds or deletes reads the same by id and through                the stores the data cache reads, and the file underneath is unchanged.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-OVERLAY-READS-AGREE"),
        station: "auberean-dat::cpu::overlay::a_record_the_overlay_changes_reads_the_same_by_both_ways_of_reading",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.overlay.refused-when-not-for-these-files",
        says: "An overlay made against other files is refused, the same records in another order                being the same files; so is a folder with no overlay in it or one holding the                install's own files.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-OVERLAY-REFUSED"),
        station: "auberean-dat::cpu::overlay::a_folder_not_for_these_files_is_refused_and_never_read",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.overlay.revisions-are-data-versions",
        says: "An overlay's revisions are among the data versions of the file it lies over, as read                by id and as the stores report them to a server.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-OVERLAY-REVISIONS"),
        station: "auberean-dat::cpu::overlay::a_revision_of_the_overlay_is_among_the_data_versions_the_client_reports",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.overlay.says-it-keeps-them",
        says: "Asked what it holds, the client first says it takes a world's records and names its                files by their layouts, in a message of its own that a server not knowing it skips. It takes no ordering \n               stamp, so what follows carries the stamps a retail client's would.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-OVERLAY-SAYS-IT-KEEPS"),
        station: "auberean-client-runtime::cpu::world::download::asked_what_it_holds_the_client_first_says_it_keeps_overlays_and_names_its_files",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.particles.every-record-decodes",
        says: "Every particle system reads as its limits and timing, its emitters with their \
               waveforms and keyframes, and the material it draws with.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-PARTICLES"),
        station: "auberean-assets::dat::look::fx::every_particle_system_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.physics-material.every-record-decodes",
        says: "Every physics material reads as a property collection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-PHYSICS-MATERIAL"),
        station: "auberean-assets::dat::game::property::every_physics_material_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.picture.every-record-decodes",
        says: "Every picture, in the portal, high-resolution and language files, reads as a \
               category word, a size, a pixel format and its bytes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-PICTURE"),
        station: "auberean-assets::dat::look::picture::every_picture_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.property-collection.find-add-update",
        says: "A collection finds a property through the group the property list puts it in, \
               refuses to add one it already finds, makes the group when adding to a missing one, \
               and updates a stored property by taking the other's value when it is of the stored \
               type.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROPERTY-COLLECTION"),
        station: "auberean-assets::cpu::property::a_collection_finds_adds_and_updates_through_the_listed_group",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.property-collection.optional-properties",
        says: "Optional properties are every property the list puts in a group, each with a new \
               value, kept apart from the stored ones, and they become the stored ones only in a \
               group that stores none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROPERTY-OPTIONAL"),
        station: "auberean-assets::cpu::property::optional_properties_come_from_the_list_and_become_required_only_into_an_empty_group",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.property-desc.every-record-decodes",
        says: "The region's mapped properties read as a version and, per property, a block map and \
               a property value for each value of the map.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-PROPERTY-DESC"),
        station: "auberean-assets::dat::world::region::every_region_property_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.property-list.every-record-decodes",
        says: "The property list reads as the property names and a descriptor per property, whose \
               first word is the value type every property of that id is read by.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-PROPERTY-LIST"),
        station: "auberean-assets::dat::game::property::the_property_list_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.property-value.answers-only-for-its-type",
        says: "A property value answers a question, or takes another value's contents, only for \
               its own type; asked for another it answers nothing and stays as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROPERTY-VALUE-SLOTS"),
        station: "auberean-assets::cpu::property::a_value_answers_only_a_question_about_its_own_type",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.property-value.as-text",
        says: "A property writes as True or False, %d (a 64-bit value's low half), %.3g, three or \
               four %.3f numbers (a color red, green, blue, alpha), the string, a name, or <file \
               unset>, <PackageID unset>, <string table unset>:<token unset>.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROPERTY-VALUE-TEXT"),
        station: "auberean-assets::cpu::property::a_value_writes_as_the_game_writes_it_for_output",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.property-value.compare-within-a-tolerance",
        says: "Vectors, colors, waveforms and positions compare equal within two ten-thousandths a \
               component; strings ignore case; floats, integers and ids compare exactly; values of \
               different types never compare equal.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROPERTY-VALUE-COMPARE"),
        station: "auberean-assets::cpu::property::vectors_and_waveforms_compare_within_a_tolerance_floats_exactly_and_strings_ignoring_case",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.property-value.new-values",
        says: "A new property value is false, zero, the zero vector, white, the empty string, no \
               file, a waveform of no shape with amplitude and frequency 1, an empty reference, or \
               no package; a package, string, file, enum or reference with nothing in it holds no \
               valid data.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROPERTY-VALUE-NEW"),
        station: "auberean-assets::cpu::property::a_new_value_is_false_zero_white_empty_or_unset_by_its_type",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.property.found-through-its-listed-group",
        says: "A record's property is found through the group the property list puts it in: one \
               stored under another group is never seen by a lookup (fewer than one stored property \
               in a hundred is).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-PROPERTY-LOOKUP"),
        station: "auberean-assets::dat::game::property::a_stored_property_is_found_only_in_the_group_its_list_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.property.string-reference-token-then-table",
        says: "A string reference stored as a property value names its token first and its string \
               table second: the second word of every one in the data files is a string table.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-PROPERTY-STRING-REF"),
        station: "auberean-assets::dat::game::property::a_stored_string_reference_names_its_token_and_then_its_table",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.qualities.a-record-names-its-data-ids-references-and-description",
        says: "A qualities record names, for any gathering, each data id it holds (not 0), the \
               tables of its string references and of the references inside them, then its \
               description's icon and its name's and plural name's tables.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-QUALITIES-SUB-IDS"),
        station: "auberean-assets::cpu::requests::a_qualities_record_names_its_data_ids_references_and_descriptions_records",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.qualities.every-record-decodes",
        says: "Every object's default qualities read as flags before the id, then the object's \
               description and the tables of typed values the flags name, in a fixed order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-QUALITIES"),
        station: "auberean-assets::dat::game::script::every_qualities_record_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.quality-filter.every-record-decodes",
        says: "Every quality filter reads as a table of quality ids to a 32-bit value.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-QUALITY-FILTER"),
        station: "auberean-assets::dat::game::script::every_quality_filter_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.quality-filter.packs-back-to-its-own-bytes",
        says: "A quality filter written out is its id and its table as read, the bytes it was \
               read from.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-FILTER-PACK"),
        station: "auberean-assets::dat::game::script::every_quality_filter_packs_back_to_its_own_bytes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.read.failed-records-are-checked-and-read-again",
        says: "A record that fails to read or decode is checked against its file: a file that changed \
               since it was opened is reported as changed; over one that did not, the record is read \
               once more, and a second failure reports both and whether the bytes were the same.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-READ-RETRY"),
        station: "auberean-dat::cpu::checked::a_record_read_wrong_once_over_an_unchanged_file_is_read_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.read.script-library-reads-are-checked",
        says: "The client and the world builder read the script library as the server does: a library \
               read wrong once, over a file that did not change, is read again.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-READ-LIBRARY"),
        station: "auberean-client-runtime::corpus::calls::a_script_library_read_wrong_once_is_read_again_and_links",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "data.read.short-reads-are-reported",
        says: "A read the file ends before its buffer is full is reported as a short read, with where it \
               started and how much came back, and nothing it filled is decoded.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-READ-SHORT"),
        station: "auberean-dat::cpu::checked::a_read_the_file_ends_early_is_a_short_read_and_is_never_decoded",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.records.encode-to-their-own-bytes",
        says: "Every property list, physics material, entity, enum map, id map, string table, \
               string state, landblock's placed objects, light record, interior cell, scene description \
               and saved script state, read and written again, gives back exactly the bytes it was read from.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ENCODE"),
        station: "auberean-assets::dat::game::encode::every_record_with_a_writer_writes_back_to_its_own_bytes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.region.every-record-decodes",
        says: "The region reads as its number, its name, its maps, the landscape constants it uses, \
               its size in landblocks, and eleven lists of the descriptions it draws on with the \
               index each is known by.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-REGION"),
        station: "auberean-assets::dat::world::region::every_region_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.requests.a-get-waits-for-the-records-it-names",
        says: "A record asked for without waiting is read, then the records it names are asked for \
               the same way; it finishes when they all have, a partial success when any failed, and \
               its asker is told of it, then of its batch.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-GET"),
        station: "auberean-assets::cpu::requests::a_get_reads_its_record_then_the_records_it_names_and_its_batch_is_told_when_all_are_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.a-host-grows-a-get",
        says: "A host grows a get itself, found by its record while it is kept or by the batch that \
               asked for it: the get waits on the records added and finishes again when they are \
               in; a record no kept get is for, or a batch let go, grows nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-HOST-GROWS"),
        station: "auberean-assets::cpu::requests::a_host_grows_a_get_by_its_record_or_by_its_batch",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.a-landblock-gathers-its-info-and-light-record",
        says: "A landblock got with the records it needs asks for its info and path map, and the \
               info for what its entities and properties name and its light record; a cell for its \
               geometry and what its entities and properties name. The light records the 2017 \
               clients asked for, refused each time, are the ones the end-of-retail files lack \
               under those landblocks' infos.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CELLS-SUB-IDS"),
        station: "auberean-assets::dat::world::landblock::a_landblocks_get_asks_for_the_light_record_its_info_names_when_the_files_lack_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.requests.a-saved-states-records-are-named-by-its-scripts",
        says: "A saved script state got for a gathering is asked, as it was loaded, which records it \
               needs, through the scripts; those are got in turn, a state among them asked the same \
               way, and the get waits on them all. Without scripts, or for the gathering that asks \
               for no named records, a state names none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-STATE-RECORDS"),
        station: "auberean-assets::cpu::requests::a_saved_state_names_its_records_through_its_scripts_and_each_state_they_name_is_asked_in_turn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.a-waiting-get-takes-a-kept-gets-record",
        says: "A record asked for now, that a request has already read, is taken from that request \
               into the records kept; one in neither memory nor the files is not found.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-WAITING"),
        station: "auberean-assets::cpu::requests::a_waiting_get_takes_the_record_a_kept_get_decoded_into_its_cache",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.an-asker-grows-its-request-as-it-finishes",
        says: "An asker that grows its requests hears each record finish before the request counts \
               as done and may add records to it: the request waits on those too and finishes \
               again for the asker when they are in, and only then is the batch done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-GROW"),
        station: "auberean-assets::cpu::requests::an_asker_that_grows_its_request_adds_records_to_it_before_it_counts_as_done",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.missing-records-are-asked-of-the-server",
        says: "A record in neither memory nor the data files is asked of the server; the record it \
               sends, once stored, finishes the request, and a refusal fails it. Not connected, the \
               request fails at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-ASK"),
        station: "auberean-assets::cpu::requests::a_record_in_neither_memory_nor_the_files_is_asked_of_the_server_and_its_answer_finishes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.one-request-per-record",
        says: "However many ask for a record, it is read once: later askers share the request, one \
               already finished answers at once, and the request is forgotten when the last asker \
               lets go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-SHARED"),
        station: "auberean-assets::cpu::requests::a_second_asker_shares_the_kept_request_and_one_already_finished_answers_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.purges-and-saves-go-to-the-worker",
        says: "Records removed from or stored in the data files without waiting are removed or stored \
               by the worker; a request waiting for a stored record takes it, unless it is with the \
               worker itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-WORKER"),
        station: "auberean-assets::cpu::requests::saves_and_purges_run_on_the_worker_and_a_get_waiting_for_a_saved_record_takes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.records-in-memory-are-taken-from-their-cache",
        says: "A record already in memory finishes its request at once; one the game has read \
               already, with what it names, asks for none of those again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-MEMORY"),
        station: "auberean-assets::cpu::requests::a_record_in_memory_finishes_at_once_and_one_got_and_kept_asks_for_none_it_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.the-worker-runs-at-the-end-of-a-frame",
        says: "The worker reads and stores what is waiting at the end of each frame, all of it, and \
               what finishing asks for waits for the next frame.",
        since: THIS_CLIENT,
        divergence: "CD2-data-cache-worker-in-frame",
        evidence: Evidence::Private("AC2-EVID-REQUESTS-IN-FRAME"),
        station: "auberean-assets::cpu::requests::the_worker_runs_its_whole_queue_each_time_the_cache_is_given_time_and_what_finishing_queues_waits_for_the_next",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.the-worker-stops",
        says: "Shutting down stops the worker, leaving what it had waiting, and keeps no records \
               nobody holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-STOP"),
        station: "auberean-assets::cpu::requests::a_stopped_worker_leaves_its_queue_and_shutting_down_keeps_no_free_records",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.requests.without-a-worker-each-runs-at-once",
        says: "Without a worker, each request is carried out as it is made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REQUESTS-IN-PLACE"),
        station: "auberean-assets::cpu::requests::without_a_worker_each_request_is_carried_out_as_it_is_made",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.run-length.a-run-unpacks-to-its-rising-numbers",
        says: "A run-length list keeps a run of two or more rising numbers as its first number with \
               the top bit set and its length; the largest number and a form past the limit are \
               refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-RUN-LENGTH"),
        station: "auberean-assets::cpu::rle::a_run_unpacks_to_its_rising_numbers_and_packs_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.scene-desc.every-record-decodes",
        says: "The region's scenery reads as a version stored before the id and, per kind of \
               scenery, an index, a sound and the scenes it chooses among with their frequencies.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SCENE-DESC"),
        station: "auberean-assets::dat::world::region::every_region_scenery_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.scene.every-record-decodes",
        says: "Every scene reads as a version before its id, a rotate flag, an optional decoration \
               group, and the objects it scatters at each level of detail.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SCENE"),
        station: "auberean-assets::dat::look::visual::every_scene_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.script-library.a-hash-tables-count-is-24-bits",
        says: "A hash table in the script library counts its entries in the low 24 bits of its \
               header word; the top byte only sizes its buckets.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LIBRARY-HASH"),
        station: "auberean-assets::cpu::script::a_hash_tables_count_is_24_bits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.script-library.a-saved-fields-tag-is-one-byte",
        says: "A package's fields named for saving are counted in a full word, each a name, a \
               slot and a one-byte tag, brought to a multiple of four bytes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LIBRARY-CHECKPOINT"),
        station: "auberean-assets::cpu::script::a_saved_fields_tag_is_one_byte",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.script-library.a-version-entry-is-two-strings",
        says: "The script library's version section holds counted pairs of narrow strings.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCRIPT-LIBRARY-VERSION"),
        station: "auberean-assets::cpu::script::a_version_entry_is_two_strings",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.script-library.every-record-decodes",
        says: "The script library reads as a magic word, a version and typed sections, every \
               section consumed to its declared size: code, literals, links, packages and their \
               functions, dispatch tables, the server's callable functions and the debugging names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SCRIPT-LIBRARY"),
        station: "auberean-assets::dat::game::script::the_script_library_decodes_exactly_section_by_section",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.script-state.every-record-decodes",
        says: "Every saved script state reads as a sized object graph and its package id.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SCRIPT-STATE"),
        station: "auberean-assets::dat::game::script::every_saved_script_state_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.setup.every-record-decodes",
        says: "Every setup reads as its flags, meshes, skeleton, poses, holding and connection \
               points, collision cylinders and spheres, sizes, bounding spheres, detail distances, \
               shadow kind and physics bounds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SETUP"),
        station: "auberean-assets::dat::look::setup::every_setup_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.sky-desc.every-record-decodes",
        says: "The region's sky reads as a version stored before the id, four tick intervals, the \
               calendar it follows and the days it chooses among.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SKY-DESC"),
        station: "auberean-assets::dat::world::region::every_region_sky_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.sound-desc.every-record-decodes",
        says: "The region's ambient sounds read as a version and, per kind of place, a sound \
               environment with the sounds that play now and then and the ones that play \
               continuously.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SOUND-DESC"),
        station: "auberean-assets::dat::world::region::every_region_sound_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.sound-environment.every-record-decodes",
        says: "Every sound environment reads as a mask and one word for each reverb \
               setting the mask names, in a fixed order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SOUND-ENVIRONMENT"),
        station: "auberean-assets::dat::game::sound::every_sound_environment_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.sound-info.every-record-decodes",
        says: "Every sound setting reads as six words: flags, a pitch range, priority, fade \
               distance and the sound it plays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SOUND-INFO"),
        station: "auberean-assets::dat::game::sound::every_sound_info_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.sound-info.new-settings-and-what-they-need",
        says: "A new sound setting has no flags, no random pitch (each end -1) and fades from a \
               distance of 1, a new music setting fades from 1 too; each tests any of a mask's flags \
               and needs its wave or music.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SOUND-INFO-NEW"),
        station: "auberean-assets::cpu::records::new_sound_and_music_settings_and_what_they_need",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-download.a-dropped-download-lets-its-batch-go",
        says: "A text element's string download dropped while it waits lets its batch go: it is \
               forgotten and told nothing, and the table, when it comes, is stored as any other.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-DROPPED"),
        station: "auberean-client-runtime::cpu::world::strings::a_download_dropped_while_it_waits_lets_its_batch_go",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-download.a-private-table-the-files-lack-is-asked-for",
        says: "A string reference given to a text element, whose table the files hold but whose \
               entry sends its text on to a private table they lack, asks the server for the \
               private table alone and waits until it comes; one whose tables are all at hand asks \
               for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-PRIVATE"),
        station: "auberean-client-runtime::cpu::world::strings::a_reference_whose_private_table_the_files_lack_asks_for_it_and_is_written_when_it_comes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-download.a-refused-table-shows-the-error-message",
        says: "A refused string table ends the text element's wait and its reference shows the \
               client's error message naming the token and the table; the refusal is kept for the \
               session, so the table is not asked for again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-REFUSED"),
        station: "auberean-client-runtime::cpu::world::strings::a_refused_table_ends_the_wait_and_the_reference_shows_the_clients_error_message",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-download.a-table-the-files-lack-is-asked-for-then-its-private-table",
        says: "A string reference whose own table the files lack asks for it; once it comes, the \
               private table the reference's entry in it names is asked for too, and the text \
               element waits for both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-TABLE"),
        station: "auberean-client-runtime::cpu::world::strings::a_table_the_files_lack_is_asked_for_and_then_the_private_table_its_entry_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-reference.same",
        says: "Two string references are the same when their token, table and literal flag are, \
               and then their literal, or as many variables by key, each the same as the other's \
               of its key (a later variable of a key replacing an earlier one).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRING-REF-SAME"),
        station: "auberean-assets::cpu::property::two_references_are_the_same_by_token_table_and_variables_by_key",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-state.every-record-decodes",
        says: "Every language's string state reads as how it writes numbers, which \
               counts take the singular, the letters of generated names and its \
               input-method settings, without its own id.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-STRING-STATE"),
        station: "auberean-assets::dat::game::strings::every_string_state_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.string-state.settings",
        says: "A string state's twenty settings are the top gore level and blood color, whether the \
               input method is on, its look (bytes where the input method takes a byte), another \
               input method's setting and whether text wraps at spaces.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRING-STATE-SETTINGS"),
        station: "auberean-assets::cpu::strings::a_string_states_settings_are_gore_blood_and_the_input_methods_look",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-table.copy-and-sub-ids",
        says: "A new or emptied string table has its number settings unset; a copy drops its entries' \
               empty string slots and keeps the first entry of a token; a table needs the private \
               tables its entries name loaded with it, unless string data is asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRING-TABLE-COPY"),
        station: "auberean-assets::cpu::strings::a_copied_table_drops_empty_slots_and_names_its_private_tables",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.string-table.every-record-decodes",
        says: "Every string table, in every language file, reads as its number format and its \
               tokens' strings, with the strings unscrambled.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-STRING-TABLE"),
        station: "auberean-assets::dat::game::strings::every_string_table_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.string-table.every-record-packs-to-its-size",
        says: "Every string table of every language packs to as many bytes as it holds, and every \
               entry storing its variables' names is an item name's, whose names are worth storing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRING-TABLE-PACK"),
        station: "auberean-assets::dat::game::strings::every_string_table_packs_to_its_own_size_and_stores_names_only_for_item_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.string-table.names-worth-packing",
        says: "A string table entry's variable names are stored only when two or more of them are \
               PRE, NAME, MID or POST, the names a generated item's name is put together from; \
               otherwise they are dropped when it is written.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRING-TABLE-NAMES"),
        station: "auberean-assets::cpu::strings::variable_names_are_stored_only_for_an_item_name_and_the_size_says_so",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.surface-desc.every-record-decodes",
        says: "The region's ground surfaces read as a version stored before the id, the far terrain \
               and cloud textures, and every surface and road with its material groups by slope.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-SURFACE-DESC"),
        station: "auberean-assets::dat::world::region::every_region_surface_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.terrain-desc.every-record-decodes",
        says: "The region's terrain reads as a version, an optional default terrain and every \
               terrain kind, each with a name, a surface, an index, a hard-edge angle and the \
               scenery it grows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-TERRAIN-DESC"),
        station: "auberean-assets::dat::world::region::every_region_terrain_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.text-tree.directives",
        says: "In a text tree an include puts the named file's nodes in its place, its first \
               node carrying the directive's comment, the file's leading comment and its own; a \
               file not found or not parsing fails the parse, as does an error directive; a \
               warning directive is dropped; any other name starting with an exclamation mark is \
               an ordinary node.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-TREE-DIRECTIVES"),
        station: "auberean-dat::cpu::tree::directives_include_fail_or_drop_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.text-tree.file-reads-in-text-mode",
        says: "A text tree's file is read as the C library reads a file in text mode (a carriage \
               return before a line feed dropped, nothing after a Ctrl-Z, a byte of all ones the end) and \
               written with Windows line ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-TREE-FILE"),
        station: "auberean-dat::cpu::tree::a_file_reads_in_text_mode_and_writes_its_line_breaks_as_crlf",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.text-tree.reads-as-the-client-parses",
        says: "A text tree reads as the client's parser reads it: names quoted or not with their \
               escapes, children in any of three kinds of bracket, comments given to the node \
               after them (one after a name to the next node), each node's line and column, and \
               a failed parse on a stray, mismatched or unclosed bracket or an empty text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-TREE-PARSE"),
        station: "auberean-dat::cpu::tree::braces_and_comments_read_as_the_client_reads_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.text-tree.typed-names",
        says: "A name set as a number or flag is written in the C library's form for its kind; a \
               name read as a number is read whole as the C library reads one, and becomes that \
               number.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-TREE-NAMES"),
        station: "auberean-dat::cpu::tree::a_name_reads_and_writes_as_a_number_in_the_c_librarys_forms",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.text-tree.writes-as-the-client-writes",
        says: "A text tree is written as the client writes one: a blank line before each top \
               node, a node's children on its line unless it is too wide (past 80 characters, \
               three levels deep) or a child has a comment, then a child a line indented two \
               spaces a level, each comment line after a hash.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-TREE-WRITE"),
        station: "auberean-dat::cpu::tree::the_writer_spreads_a_node_too_wide_too_deep_or_with_commented_children",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.texture.every-record-decodes",
        says: "Every texture reads as a kind word and the list of its pictures, one per level of \
               detail.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-TEXTURE"),
        station: "auberean-assets::dat::look::picture::every_texture_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.tree.a-paths-folder-runs-to-its-last-separator",
        says: "A path's folder runs up to and including its last backslash, slash or colon, and is \
               empty without one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-DIRECTORY"),
        station: "auberean-dat::cpu::tree::a_paths_folder_runs_to_its_last_separator",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.types.every-record-has-a-type",
        says: "Every record of the end-of-retail set has a type, derived from its id, and \
               lives in the file its type says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-TYPES"),
        station: "auberean-dat::dat::types::every_record_of_the_target_install_has_a_type_in_the_file_it_lives_in",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.ui-scene.every-record-decodes",
        says: "Every scene shown inside the interface reads as a list of placed \
               entities, a fog color, whether there is fog, and the fog's near and far \
               distances.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-UI-SCENE"),
        station: "auberean-assets::dat::game::ui::every_ui_scene_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.valid-modes.every-record-decodes",
        says: "Every valid-modes record reads as a counted list of mode ids, with no id of its own \
               before it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-VALID-MODES"),
        station: "auberean-assets::dat::look::animation::every_valid_modes_list_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.version-set.packs-runs-of-three-or-more",
        says: "A set of data versions packs in order without repeats, runs of three or more as a length \
               and a first value; unpacked, a lone value with its second-highest bit set is negative.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VERSION-SET"),
        station: "auberean-dat::cpu::disk::a_version_set_packs_runs_of_three_or_more_and_unpacks_negatives",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.version-stamp.text-and-pack",
        says: "A data version stamp is a unique id (version 4 when made new) and a minor version, \
               written as the id in its 8-4-4-4-12 hexadecimal form, ' : ' and the minor; read back \
               as the C library scans it (the minor kept modulo 2^32 and read before the id is \
               checked); packed as that text in a narrow string.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-ID-STAMP"),
        station: "auberean-dat::cpu::stamp::a_stamp_is_its_id_and_minor_as_text_and_packs_as_that_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "data.visual-desc.every-record-a-look-names-is-there",
        says: "Every record a visual description, appearance table or effect table names, gathered \
               as the client gathers what a loaded record needs, is in the data files but one: an \
               appearance table names a material modifier that no file holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-LOOK-NAMED"),
        station: "auberean-assets::dat::look::visual::every_record_a_look_names_is_there",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.visual-desc.every-record-decodes",
        says: "Every visual description reads as a flags word and the fields it flags, its own id \
               among them: parent, motion and behavior tables, scale, icon and part groups.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-VISUAL-DESC"),
        station: "auberean-assets::dat::look::visual::every_visual_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.water-desc.every-record-decodes",
        says: "The region's water reads as a version stored before the id, height and kind maps, an \
               optional default and every water kind with its materials, color and fog and light by \
               depth.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-WATER-DESC"),
        station: "auberean-assets::dat::world::region::every_region_water_description_decodes_exactly",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data.wave.every-record-decodes",
        says: "Every sound reads as a format block and its sample data, the block's size taken \
               from the record.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-WAVE"),
        station: "auberean-assets::dat::game::sound::every_wave_decodes_exactly",
        tier: Tier::Dat,
    },
];
