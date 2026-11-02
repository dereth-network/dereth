//! The messages: how each is laid out, and the forms their parts take on the wire.
//!
//! A layout row claims that every recorded message of a type reads with every byte used and
//! writes back to the same bytes, which the September 2013 client and the servers it spoke to
//! both did.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "protocol.message.every-recorded-message-round-trips",
        says: "Every message recorded between the client and the retail servers, both ways, reads \
               with every byte used and writes back to the bytes recorded.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NET-CORPUS"),
        station: "auberean-protocol::corpus::messages::every_recorded_message_decodes_exactly_and_encodes_back",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "protocol.physics-desc.a-scale-left-out-is-one",
        says: "A physics description the server sends without its speed, jump or head scales has \
               each at 1, and with no position and no parent it is in no cell and hangs from \
               nothing: the values a new description has.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROTOCOL-PHYSICS-DESC-NEW"),
        station: "auberean-protocol::cpu::descriptions::a_physics_description_without_its_scales_has_each_at_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "protocol.qualities.the-header-names-the-tables-that-hold-entries",
        says: "Every player's qualities the 2005, 2013 and 2017 servers sent carry the header the \
               client works out from them (the description's bit, and the bit of each table holding \
               an entry: no table is sent empty) and pack to as many bytes as were sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROTOCOL-QUALITIES-HEADER"),
        station: "auberean-protocol::corpus::qualities::every_recorded_players_qualities_carry_the_header_the_client_works_out",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "protocol.script.every-relaunch-call-reads-to-values-and-back",
        says: "Every script call recorded with the relaunched game, both ways, carries arguments \
               that read as the script machine's packed values against the September 2013 \
               library, and the call rebuilt from those values is the call recorded.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NET-SCRIPT-ARGS"),
        station: "auberean-protocol::corpus::script_args::every_relaunch_script_call_reads_to_values_and_rebuilds_the_same_message",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "protocol.string.scrambled-on-the-wire",
        says: "A string on the wire is its length, then its characters scrambled, then padding to \
               four bytes from the start of the message: the client version 1802 travels as four \
               scrambled bytes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NET-SCRAMBLE"),
        station: "auberean-protocol::cpu::text::the_client_version_travels_scrambled",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "protocol.table.laid-out-bucket-by-bucket-newest-first",
        says: "A keyed table is sent bucket by bucket, a key's bucket being the key modulo the \
               table's bucket count, and within a bucket the entry added last comes first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NET-TABLE-ORDER"),
        station: "auberean-protocol::cpu::table::a_table_lays_out_its_entries_bucket_by_bucket_newest_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "protocol.weenie-desc.a-copy-holds-only-what-the-original-has",
        says: "A copy of a weenie description holds its package only when the original holds one \
               that is not all ones, its entity only when not 0, and its flag word whenever one of \
               the word's four low flags is on; every other field is copied as it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROTOCOL-WEENIE-DESC-COPY"),
        station: "auberean-protocol::cpu::descriptions::a_copied_weenie_description_holds_only_the_package_entity_and_flags_the_original_has",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "protocol.weenie-desc.a-new-one-has-a-quantity-of-one-and-no-package",
        says: "A weenie description the server sends without a package or a quantity has a \
               quantity of 1 and a package of all ones (none) beneath its mask: the values a new \
               description has.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PROTOCOL-WEENIE-DESC-NEW"),
        station: "auberean-protocol::cpu::descriptions::a_weenie_description_without_a_package_or_quantity_has_none_and_one",
        tier: Tier::Cpu,
    },
];
