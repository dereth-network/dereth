use super::*;

/// A client in the world with the shell up, for a scenario whose subject is a typed line.
fn a_client_for_house_commands() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(4))
}

/// The text of every live bubble in the strip, in the order it holds them.
///
/// The strip is the observable and not a counter, because the refusals are drawn on the one
/// channel the chat windows' shipped filters drop and the strip's own accepts.
fn house_bubbles(c: &mut HeadlessClient) -> Vec<String> {
    let list = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen is current").roots()[0];
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::hud::speech_bubbles::LIST_BOX)
            .expect("the bubble strip is in the shipped layout")
    };
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell
        .ui
        .children(list)
        .into_iter()
        .filter_map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map(|t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// The chat log as its element really holds it -- what a player reads.
fn house_log_text(c: &mut HeadlessClient) -> String {
    let log = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen is current").roots()[0];
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::chat::window::LOG)
            .expect("the chat log is in the shipped layout")
    };
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(log)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// A name packed the way the client packs one: a count, the characters, and padding to four.
fn house_pstr(s: &str) -> Vec<u8> {
    let mut v = u16::try_from(s.len())
        .expect("a short name")
        .to_le_bytes()
        .to_vec();
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

fn house_dw(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

/// The opcode each request of this family carries, written out here rather than read off the
/// codec, so that the table and the codec cannot drift together.
fn house_opcode_of(r: &dereth_client_model::Request) -> u32 {
    use dereth_client_model::Request as R;
    match r {
        R::AbandonHouse(_) => 0x021F,
        R::AddPermanentGuest(_) => 0x0245,
        R::RemovePermanentGuest(_) => 0x0246,
        R::SetOpenHouseStatus(_) => 0x0247,
        R::ChangeStoragePermission(_) => 0x0249,
        R::BootSpecificHouseGuest(_) => 0x024A,
        R::RemoveAllStoragePermission(_) => 0x024C,
        R::RequestFullGuestList(_) => 0x024D,
        R::AddAllStoragePermission(_) => 0x025C,
        R::RemoveAllPermanentGuests(_) => 0x025E,
        R::BootEveryone(_) => 0x025F,
        R::TeleToHouse(_) => 0x0262,
        R::SetHooksVisibility(_) => 0x0266,
        R::ModifyAllegianceGuestPermission(_) => 0x0267,
        R::ModifyAllegianceStoragePermission(_) => 0x0268,
        R::ListAvailableHouses(_) => 0x0270,
        R::TeleToMansion(_) => 0x0278,
        other => panic!("this family never builds {other:?}"),
    }
}

/// The datagram a request becomes: the ordered envelope, the stamp, the opcode, the body.
fn house_action_bytes(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = 0xF7B1_u32.to_le_bytes().to_vec();
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

// ---------------------------------------------------------------------------------------------
// house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire
// ---------------------------------------------------------------------------------------------

/// Every typed line of the ladder that sends, with the bytes it sends. The counter the client
/// keeps of commands it does not know is asserted **not** to move, because that counter is where
/// a line the ladder misses lands.
pub(super) fn every_house_sub_command_that_sends_puts_its_own_message_on_the_wire() {
    use dereth_client_model::Request as R;
    use dereth_protocol::trade as t;

    let cases: Vec<(&str, R, Vec<u8>)> = vec![
        (
            "@house open",
            R::SetOpenHouseStatus(t::HouseSetOpenHouseStatus { open: 1 }),
            house_dw(1),
        ),
        (
            "@house close",
            R::SetOpenHouseStatus(t::HouseSetOpenHouseStatus { open: 0 }),
            house_dw(0),
        ),
        (
            "@house recall",
            R::TeleToHouse(t::HouseTeleToHouse),
            Vec::new(),
        ),
        ("@house re", R::TeleToHouse(t::HouseTeleToHouse), Vec::new()),
        (
            "@house mansion_recall",
            R::TeleToMansion(t::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@house alleg_recall",
            R::TeleToMansion(t::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@house ma",
            R::TeleToMansion(t::HouseTeleToMansion),
            Vec::new(),
        ),
        (
            "@house hooks on",
            R::SetHooksVisibility(t::HouseSetHooksVisibility { visible: 1 }),
            house_dw(1),
        ),
        (
            "@house hooks off",
            R::SetHooksVisibility(t::HouseSetHooksVisibility { visible: 0 }),
            house_dw(0),
        ),
        (
            "@house boot Brenwick",
            R::BootSpecificHouseGuest(t::HouseBootSpecificHouseGuest {
                name: "Brenwick".into(),
            }),
            house_pstr("Brenwick"),
        ),
        (
            "@house remove Brenwick",
            R::BootSpecificHouseGuest(t::HouseBootSpecificHouseGuest {
                name: "Brenwick".into(),
            }),
            house_pstr("Brenwick"),
        ),
        (
            "@house boot_all",
            R::BootEveryone(t::HouseBootEveryone),
            Vec::new(),
        ),
        (
            "@house remove_all",
            R::BootEveryone(t::HouseBootEveryone),
            Vec::new(),
        ),
        (
            "@house boot -all",
            R::BootEveryone(t::HouseBootEveryone),
            Vec::new(),
        ),
        (
            "@house guest add Ash",
            R::AddPermanentGuest(t::HouseAddPermanentGuest { name: "Ash".into() }),
            house_pstr("Ash"),
        ),
        (
            "@house guest remove Ash",
            R::RemovePermanentGuest(t::HouseRemovePermanentGuest { name: "Ash".into() }),
            house_pstr("Ash"),
        ),
        (
            "@house guest remove_all",
            R::RemoveAllPermanentGuests(t::HouseRemoveAllPermanentGuests),
            Vec::new(),
        ),
        (
            "@house guest list",
            R::RequestFullGuestList(t::HouseRequestFullGuestList),
            Vec::new(),
        ),
        (
            "@house guest show",
            R::RequestFullGuestList(t::HouseRequestFullGuestList),
            Vec::new(),
        ),
        (
            "@house guest add_allegiance",
            R::ModifyAllegianceGuestPermission(t::HouseModifyAllegianceGuestPermission {
                allow: 1,
            }),
            house_dw(1),
        ),
        (
            "@house guest remove_allegiance",
            R::ModifyAllegianceGuestPermission(t::HouseModifyAllegianceGuestPermission {
                allow: 0,
            }),
            house_dw(0),
        ),
        (
            "@house storage add Ash",
            R::ChangeStoragePermission(t::HouseChangeStoragePermission {
                name: "Ash".into(),
                has_permission: 1,
            }),
            [house_pstr("Ash"), house_dw(1)].concat(),
        ),
        (
            "@house storage remove Ash",
            R::ChangeStoragePermission(t::HouseChangeStoragePermission {
                name: "Ash".into(),
                has_permission: 0,
            }),
            [house_pstr("Ash"), house_dw(0)].concat(),
        ),
        (
            "@house storage add -all",
            R::AddAllStoragePermission(t::HouseAddAllStoragePermission),
            Vec::new(),
        ),
        (
            "@house storage remove -all",
            R::RemoveAllStoragePermission(t::HouseRemoveAllStoragePermission),
            Vec::new(),
        ),
        (
            "@house storage remove_all",
            R::RemoveAllStoragePermission(t::HouseRemoveAllStoragePermission),
            Vec::new(),
        ),
        (
            "@house storage list",
            R::RequestFullGuestList(t::HouseRequestFullGuestList),
            Vec::new(),
        ),
        (
            "@house storage add_allegiance",
            R::ModifyAllegianceStoragePermission(t::HouseModifyAllegianceStoragePermission {
                allow: 1,
            }),
            house_dw(1),
        ),
        (
            "@house storage remove_allegiance",
            R::ModifyAllegianceStoragePermission(t::HouseModifyAllegianceStoragePermission {
                allow: 0,
            }),
            house_dw(0),
        ),
        (
            "@house available cottage",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 1 }),
            house_dw(1),
        ),
        (
            "@hslist villa",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 2 }),
            house_dw(2),
        ),
        (
            "@hslist MANSION",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 3 }),
            house_dw(3),
        ),
        // The comparison is case-blind in every arm of the family, so case never matters.
        (
            "@hslist Apartment",
            R::ListAvailableHouses(t::HouseListAvailableHouses { house_type: 4 }),
            house_dw(4),
        ),
    ];

    let mut c = a_client_for_house_commands();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let mut stamp = 1_u32;
    let mut every_line_holds = true;

    for (line, want, body) in &cases {
        let unimplemented = c.view().interaction().stats.chat_commands_unimplemented;
        let refused = c.view().interaction().stats.chat_commands_refused;
        let before = c.view().outbound().len();
        hand.say(&mut c, line);
        c.tick(1);

        let asked: Vec<dereth_client_model::Request> = c.view().outbound()[before..].to_vec();
        every_line_holds &=
            c.view().interaction().stats.chat_commands_unimplemented == unimplemented;
        every_line_holds &= c.view().interaction().stats.chat_commands_refused == refused;
        every_line_holds &= asked.len() == 1 && asked[0] == *want;

        // And what that request becomes on a datagram, through the client's own sender.
        every_line_holds &= dereth_client_runtime::requests::send_request(&mut session, want);
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per request")
            .clone();
        every_line_holds &=
            (packet.queue, packet.ordered) == (dereth_primitives::NetQueue::Weenie, true);
        every_line_holds &=
            packet.payload == house_action_bytes(stamp, house_opcode_of(want), body);
        stamp += 1;
    }

    // The census: every opcode this ladder can reach without a question first. The one that is
    // behind the two-stage question has its own scenario.
    let mut opcodes: Vec<u32> = session
        .transport
        .sent
        .iter()
        .map(|p| u32::from_le_bytes(p.payload[8..12].try_into().expect("an opcode")))
        .collect();
    opcodes.sort_unstable();
    opcodes.dedup();
    let the_whole_family = opcodes
        == vec![
            0x0245, 0x0246, 0x0247, 0x0249, 0x024A, 0x024C, 0x024D, 0x025C, 0x025E, 0x025F, 0x0262,
            0x0266, 0x0267, 0x0268, 0x0270, 0x0278,
        ];

    c.assert_behaviour(
        "house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire",
        move |_| every_line_holds && the_whole_family,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing
// ---------------------------------------------------------------------------------------------

/// Every refusal the ladder has, read off the strip rather than off a counter -- and none of them
/// is the client's own not-a-command sentence, which a client without the ladder prints instead
/// of any of these.
pub(super) fn a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing() {
    let cases: Vec<(&str, &str)> = vec![
        ("@house", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@hou", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house wibble", house_cmd::PLEASE_SEE_HELP_HOUSE),
        // Neither word after `hooks`, so it falls through rather than defaulting either way.
        ("@house hooks", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house hooks maybe", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house boot", house_cmd::PLEASE_SEE_HELP_HOUSE),
        // The guests' own sentence...
        ("@house guest add", house_cmd::SPECIFY_THE_GUESTS_NAME),
        ("@house guest remove", house_cmd::SPECIFY_THE_GUESTS_NAME),
        // ...and the storage's, which is a *different* one, and the reason `@house storage add`
        // with no name is not an accidental grant to everybody.
        ("@house storage add", house_cmd::SPECIFY_AN_ACTUAL_NAME),
        ("@house storage remove", house_cmd::SPECIFY_AN_ACTUAL_NAME),
        ("@house guest wibble", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@house storage wibble", house_cmd::PLEASE_SEE_HELP_HOUSE),
        ("@hslist", house_cmd::PLEASE_SEE_HELP_HSLIST),
        ("@hslist castle", house_cmd::PLEASE_SEE_HELP_HSLIST),
        ("@house available", house_cmd::PLEASE_SEE_HELP_HSLIST),
    ];

    let mut c = a_client_for_house_commands();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    let mut every_refusal_holds = true;

    for (line, want) in &cases {
        let before = c.view().outbound().len();
        hand.say(&mut c, line);
        c.tick(3);
        every_refusal_holds &= c.view().outbound().len() == before;
        let drawn = house_bubbles(&mut c);
        every_refusal_holds &= drawn.iter().any(|t| t.trim() == want.trim());
        every_refusal_holds &= !drawn
            .iter()
            .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());
    }

    // The reader is proved able to see the sentence it has just reported absent fifteen times:
    // there is one shipped command that really is refused that way, and it says so here.
    hand.say(&mut c, "@afk wibble");
    c.tick(3);
    let the_reader_can_see_it = house_bubbles(&mut c)
        .iter()
        .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());

    c.assert_behaviour(
        "house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing",
        move |_| every_refusal_holds && the_reader_can_see_it,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships
// ---------------------------------------------------------------------------------------------

/// `/help` and `/help house`, typed with the slash a player types them with.
pub(super) fn the_help_listing_and_the_house_help_are_what_the_client_ships() {
    let mut c = a_client_for_house_commands();
    let mut hand = dereth_testkit::adapters_chat::Hand::new();

    let unimplemented = c.view().interaction().stats.chat_commands_unimplemented;
    let before = c.view().outbound().len();
    hand.say(&mut c, "/help");
    c.tick(3);
    let a_slash_works = c.view().interaction().stats.chat_commands_unimplemented == unimplemented
        && c.view().outbound().len() == before;

    let log = house_log_text(&mut c);
    let listed = log.contains("Available help:")
        && [
            "@help allegiances - Commands to help you deal with your Allegiance.",
            "@help house - Commands that help you manage your house, including guest and storage management.",
            "@help commands - Lists all commands.",
            "Note: You may substitute a forward slash (/) for the at symbol (@).",
        ]
        .iter()
        .all(|line| log.contains(line));
    let not_refused = !house_bubbles(&mut c)
        .iter()
        .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());

    hand.say(&mut c, "/help house");
    c.tick(3);
    let log = house_log_text(&mut c);
    let house_help = log.contains("For more information, type @help")
        && [
            "@house abandon - Abandons your house.",
            "- Adds players to your house guest list.",
            "@house storage remove_all - Removes all storage permissions from guests.",
            "@house hooks on|off - Makes the hooks in your house visible or invisible.",
            "@house available - See @hslist",
        ]
        .iter()
        .all(|line| log.contains(line));

    hand.say(&mut c, "@help hslist");
    c.tick(3);
    let the_list_help =
        house_log_text(&mut c).contains("Types include: Apartment, Cottage, Villa, Mansion");

    hand.say(&mut c, "@help wibble");
    c.tick(3);
    let drawn = house_bubbles(&mut c);
    let an_unknown_word = drawn
        .iter()
        .any(|t| t.trim() == dereth_client_model::cmd::help::UNKNOWN_COMMAND)
        && !drawn
            .iter()
            .any(|t| t.trim() == dereth_client_model::cmd::NOT_A_VALID_COMMAND.trim());

    c.assert_behaviour(
        "house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships",
        move |_| {
            a_slash_works && listed && not_refused && house_help && the_list_help && an_unknown_word
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log
// ---------------------------------------------------------------------------------------------

/// The guest table the shard sends back, laid out by hand so that this crate's own writer cannot
/// define the oracle, and read off the chat log's own element.
fn guest_table_body(guests: &[(u32, i32, &str)]) -> Vec<u8> {
    let mut b = 0x1000_0002_u32.to_le_bytes().to_vec(); // the version the recording carries
    b.extend_from_slice(&0_u32.to_le_bytes()); // bitmask
    b.extend_from_slice(&0_u32.to_le_bytes()); // the monarch
                                               // The table header: the count in the low half, the bucket count in the high half.
    b.extend_from_slice(
        &u32::try_from(guests.len())
            .expect("a short table")
            .to_le_bytes()[..2],
    );
    b.extend_from_slice(&64_u16.to_le_bytes());
    for (id, storage, name) in guests {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&storage.to_le_bytes());
        b.extend_from_slice(&house_pstr(name));
    }
    b.extend_from_slice(&0_u32.to_le_bytes()); // an empty roommate list
    while b.len() % 4 != 0 {
        b.push(0);
    }
    b
}

/// What the player asked for with `@house guest list`, arriving.
pub(super) fn the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log() {
    let (mut c, mut peer) = a_house_client();

    // The empty table first, which is the shape a recorded session carries.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_HAR,
        &guest_table_body(&[]),
    );
    let log = house_log_text(&mut c);
    let empty_table = c.view().expect_app().hud().stats.house_har_updates == 1
        && c.view().expect_app().hud().stats.house_har_guests == 0
        && log.contains("Guests:")
        && log.contains("None")
        // The client sends no roommate flag, so that heading is never drawn.
        && !log.contains("Roommates:");

    // And a populated one, with the mark on the guest that may use the storage and not the other.
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_UPDATE_HAR,
        &guest_table_body(&[(0x5000_0014, 0, "Plain"), (0x5000_0015, 1, "Trusted")]),
    );
    let log = house_log_text(&mut c);
    let two_guests = c.view().expect_app().hud().stats.house_har_updates == 2
        && c.view().expect_app().hud().stats.house_har_guests == 2
        && log.contains("Plain")
        && log.contains("Trusted *")
        && !log.contains("Plain *");

    // The lines themselves, pinned on the model as well, so the text is not read only through the
    // glyph reader.
    let one_guest = dereth_protocol::trade::Har {
        version: dereth_protocol::trade::Har::CURRENT_VERSION,
        bitmask: 0,
        monarch_iid: dereth_primitives::ObjectId(0),
        guest_table: dereth_protocol::archive::PackedHash {
            table_size: 64,
            entries: vec![(
                1,
                dereth_protocol::trade::GuestInfo {
                    item_storage_permission: 1,
                    char_name: "Trusted".into(),
                },
            )],
        },
        roommate_list: None,
    };
    let composed = dereth_client_model::housing::har_dump(&one_guest, false)
        == "Guests:\n  Trusted *\n"
        && dereth_client_model::housing::har_dump(&dereth_protocol::trade::Har::default(), false)
            == "Guests:\n  None\n";

    c.assert_behaviour(
        "house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log",
        move |_| empty_table && two_guests && composed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations
// ---------------------------------------------------------------------------------------------

/// What the player asked for with `@hslist`, arriving -- with the three branches the answer has.
pub(super) fn the_free_houses_the_shard_lists_are_written_out_with_their_locations() {
    /// Holtburg's landcell, which is west and south of the origin.
    const HOLTBURG: u32 = 0xA9B4_0025;

    let (mut c, mut peer) = a_house_client();

    let mut body = house_dw(2); // a villa
    body.extend_from_slice(&house_dw(1)); // one landcell
    body.extend_from_slice(&house_dw(HOLTBURG));
    body.extend_from_slice(&3_i32.to_le_bytes());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_AVAILABLE_HOUSES,
        &body,
    );
    let (ew, ns) = dereth_physics::landdefs::gid_to_lcoord(dereth_primitives::CellId(HOLTBURG))
        .expect("Holtburg is a real landcell");
    let want = dereth_client_model::housing::coord_line(ew, ns);
    let log = house_log_text(&mut c);
    let listed = c.view().expect_app().hud().stats.house_available_houses == 1
        && c.view()
            .expect_app()
            .hud()
            .stats
            .house_available_coord_lines
            == 1
        && log.contains("There are 3 villas available.")
        && log.contains(want.trim())
        && !log.contains("too many houses");

    // An apartment listing gives the count and stops, however large the count is.
    let coords = c
        .view()
        .expect_app()
        .hud()
        .stats
        .house_available_coord_lines;
    let mut body = house_dw(4);
    body.extend_from_slice(&house_dw(1));
    body.extend_from_slice(&house_dw(HOLTBURG));
    body.extend_from_slice(&500_i32.to_le_bytes());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_AVAILABLE_HOUSES,
        &body,
    );
    let log = house_log_text(&mut c);
    let apartments = c.view().expect_app().hud().stats.house_available_houses == 2
        && c.view()
            .expect_app()
            .hud()
            .stats
            .house_available_coord_lines
            == coords
        && log.contains("There are 500 apartments available.")
        && !log.contains("too many houses");

    // ...and the cut-off, on a kind that does list its places.
    let mut body = house_dw(1); // a cottage
    body.extend_from_slice(&house_dw(1));
    body.extend_from_slice(&house_dw(HOLTBURG));
    body.extend_from_slice(&401_i32.to_le_bytes());
    house_event(
        &mut c,
        &mut peer,
        dereth_protocol::Opcode::HOUSE_AVAILABLE_HOUSES,
        &body,
    );
    let cut_off =
        house_log_text(&mut c).contains("Only the first 400 locations are displayed here.");

    // The boundary and the kind the client has no word for, pinned on the composer so that the
    // exact numbers are readable.
    let boundary = !dereth_client_model::housing::available_houses_truncated(400)
        && dereth_client_model::housing::available_houses_truncated(401)
        && dereth_client_model::housing::available_houses_header(0, 7)
            == "There are 7  available.\n";

    c.assert_behaviour(
        "house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations",
        move |_| listed && apartments && cut_off && boundary,
    );
    c.shutdown();
}
