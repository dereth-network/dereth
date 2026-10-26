//! Divergence: V458
//! The status endpoint's portal table: `/portals` is not served unless `server.publish_portals` is
//! on, and when it is, the document lists every placed portal with a destination, in from-cell then
//! class order, with land cells for bare landblocks, level limits and escaped names.
//! Fixture: a small in-memory world of placed portals, a portal with no destination and a
//! non-portal.

use std::sync::Arc;

use empyrean_content::models::world::{LandblockInstance, Weenie};
use empyrean_content::MemContent;
use empyrean_entity::enums::{PositionType, PropertyInt, PropertyString, WeenieType};
use empyrean_server::portal_table::{self, PortalTable};
use empyrean_server::status_endpoint::{respond, respond_with_portals, StatusSnapshot};

const NORTH: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

fn world() -> MemContent {
    MemContent::new()
        // A portal with a level range, a tab and a backslash in its name, into a dungeon cell.
        .weenie(
            Weenie::new(1000, "portalranged", WeenieType::Portal)
                .with_string(PropertyString::Name, "Ranged\tPortal \\ One")
                .with_int(PropertyInt::MinLevel, 10)
                .with_int(PropertyInt::MaxLevel, 50)
                .with_position(
                    PositionType::Destination,
                    0x01AC_0118,
                    [60.0, -50.25, 0.5],
                    NORTH,
                ),
        )
        // A portal with no limits whose destination is stored as a bare landblock.
        .weenie(
            Weenie::new(999, "portalplain", WeenieType::Portal)
                .with_string(PropertyString::Name, "Plain Portal")
                .with_position(
                    PositionType::Destination,
                    0xC6A9_0000,
                    [30.0, 100.0, 12.5],
                    NORTH,
                ),
        )
        // A portal that leads nowhere (its destination is set at run time) is left out.
        .weenie(
            Weenie::new(1001, "portalnowhere", WeenieType::Portal)
                .with_string(PropertyString::Name, "Nowhere"),
        )
        // Something else with a destination is not a portal.
        .weenie(
            Weenie::new(1002, "notaportal", WeenieType::Generic)
                .with_string(PropertyString::Name, "Sign")
                .with_position(PositionType::Destination, 0x0101_0101, [1.0; 3], NORTH),
        )
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4001,
            1000,
            0xA9B4_0021,
            [84.0, 7.5, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7A9B_4002,
            999,
            0xA9B4_0021,
            [86.0, 8.0, 94.0],
        ))
        // Stored as a bare landblock: x 191.9 is the eighth column, y 0.5 the first row.
        .landblock_instance(LandblockInstance::new(
            0x7123_4001,
            999,
            0x1234_0000,
            [191.9, 0.5, 10.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7123_4002,
            1001,
            0x1234_0002,
            [30.0, 30.0, 0.0],
        ))
        .landblock_instance(LandblockInstance::new(
            0x7123_4003,
            1002,
            0x1234_0002,
            [31.0, 31.0, 0.0],
        ))
}

const EXPECTED: &str = "dereth-navigation-store\t1\n\
world\tTest World \\\\ One\n\
portal\tpublished\t999\tPlain Portal\t0x12340039 191.900 0.500 10.000\t0xC6A9000D 30.000 100.000 12.500\t0\t0\t0\t-\t-\t-\n\
portal\tpublished\t999\tPlain Portal\t0xA9B40021 86.000 8.000 94.000\t0xC6A9000D 30.000 100.000 12.500\t0\t0\t0\t-\t-\t-\n\
portal\tpublished\t1000\tRanged Portal \\\\ One\t0xA9B40021 84.000 7.500 94.000\t0x01AC0118 60.000 -50.250 0.500\t0\t0\t0\t10\t50\t-\n";

fn snapshot() -> Option<StatusSnapshot> {
    None
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 response")
}

#[test]
fn the_portal_table_lists_placed_portals_in_order_with_land_cells_limits_and_escaped_names() {
    assert_eq!(
        portal_table::document("Test\tWorld \\ One", &world()),
        EXPECTED
    );
}

#[test]
fn portals_are_not_served_unless_publishing_is_on() {
    assert!(
        !empyrean_common::master_configuration::MasterConfiguration::default()
            .server
            .publish_portals,
        "off by default"
    );
    let off = text(&respond("GET /portals HTTP/1.1", snapshot));
    assert!(off.starts_with("HTTP/1.0 404 Not Found\r\n"), "{off}");
    let off = text(&respond_with_portals(
        "GET /portals HTTP/1.1",
        snapshot,
        None,
    ));
    assert!(off.starts_with("HTTP/1.0 404 Not Found\r\n"), "{off}");

    let config = empyrean_common::toml_config::from_toml_str("[server]\npublish_portals = true\n")
        .unwrap()
        .config;
    assert!(
        config.server.publish_portals,
        "the key is server.publish_portals"
    );
}

#[test]
fn published_portals_are_served_as_tab_separated_values() {
    let table = PortalTable::new("Test\tWorld \\ One", Arc::new(world()));
    assert_eq!(
        table.document(),
        EXPECTED,
        "built once, off the endpoint's thread"
    );
    let r = text(&respond_with_portals(
        "GET /portals HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(r.starts_with("HTTP/1.0 200 OK\r\n"), "{r}");
    assert!(
        r.contains("Content-Type: text/tab-separated-values; charset=utf-8\r\n"),
        "{r}"
    );
    assert!(
        r.contains(&format!("Content-Length: {}\r\n", EXPECTED.len())),
        "{r}"
    );
    assert!(r.ends_with(&format!("\r\n\r\n{EXPECTED}")), "{r}");
    // The other paths are unchanged.
    assert!(text(&respond_with_portals(
        "GET /nope HTTP/1.1",
        snapshot,
        Some(&table)
    ))
    .starts_with("HTTP/1.0 404 "));
    let head = text(&respond_with_portals(
        "HEAD /portals HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(head.ends_with("\r\n\r\n"), "HEAD has no body: {head}");
}

#[test]
fn the_portal_table_is_built_off_the_endpoint_and_answered_as_not_ready_until_it_is() {
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let table = PortalTable::building(move || {
        let _ = gate.recv_timeout(std::time::Duration::from_secs(5));
        portal_table::document("Test\tWorld \\ One", &world())
    });
    // While it is being built, the endpoint answers without waiting for it: the table is not
    // ready, and the other paths are as ever.
    let r = text(&respond_with_portals(
        "GET /portals HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(r.starts_with("HTTP/1.0 503 "), "{r}");
    assert!(text(&respond_with_portals(
        "GET /health HTTP/1.1",
        snapshot,
        Some(&table)
    ))
    .starts_with("HTTP/1.0 503 "));
    release.send(()).unwrap();
    assert_eq!(table.document(), EXPECTED);
    let r = text(&respond_with_portals(
        "GET /portals HTTP/1.1",
        snapshot,
        Some(&table),
    ));
    assert!(r.starts_with("HTTP/1.0 200 OK\r\n"), "{r}");
}
