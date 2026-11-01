//! The data files: the container, record types, and how each type's records are laid out.
//!
//! A layout row claims that every record of its type, in the end-of-retail set of data files, is
//! read with every byte consumed by the layout the decoder implements, as the September 2013 client
//! reads it.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
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
        id: "data.types.every-record-has-a-type",
        says: "Every record of the end-of-retail set has a type, derived from its id, and \
               lives in the file its type says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAT-TYPES"),
        station: "auberean-dat::dat::types::every_record_of_the_target_install_has_a_type_in_the_file_it_lives_in",
        tier: Tier::Dat,
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
];
