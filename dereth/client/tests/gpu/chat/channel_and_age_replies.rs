//! Chat reports: the two channel replies (`0x0148`, `0x0149`) print their lists into the chat log,
//! an empty channel reply still prints its header, and a query-age response (`0x01C3`) prints the
//! arm its target name selects.
//! Fixture: `net::late_receivers`' socket-free replay App and production-encoded game events.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::gpu_lock;
use crate::net::late_receivers::{chat, settle, setup};

use dereth_client::dropped;
use dereth_protocol::Opcode;

/// Behaviour: chat.channels.the-list-replies-print-into-the-log
///
/// **The two channel replies print lists into the chat log, and their names are swapped.**
///
/// The two handlers are identical except for one format literal,
/// and those literals say which is which:
/// `0x0148 ChannelList` lists **characters** ("The following characters are currently listening on
/// the channel:") and `0x0149 ChannelIndex` lists **channels** ("The following channels are
/// available to you:"). Each entry is its own line, `"   " + name + "\n"`, on chat type 0.
///
/// Neither drives a channel selector: both are chat output.
///
/// **Falsified by** making either arm unreachable — the header alone is enough to fail, which is
/// why the empty-list half below is a station and not a footnote.
#[test]
fn the_two_channel_replies_print_their_lists_into_the_chat_log() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("channels");
    let before = chat(&mut app).len();

    peer.event(
        &mut app,
        &dereth_protocol::comms::CommunicationChannelListRecv {
            names: vec!["Larktest".to_owned(), "Aldermoore".to_owned()],
        },
    );
    settle(&mut app);
    let lines: Vec<String> = chat(&mut app)[before..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    assert_eq!(
        lines,
        vec![
            "The following characters are currently listening on the channel:".to_owned(),
            "Larktest".to_owned(),
            "Aldermoore".to_owned(),
        ],
        "the header then one indented row per name (the chat display trims the indentation)"
    );
    let s = &app.interaction().stats;
    assert_eq!(s.channel_lists, 1);
    assert_eq!(s.channel_rows, 2);

    let before = chat(&mut app).len();
    peer.event(
        &mut app,
        &dereth_protocol::comms::CommunicationChannelIndexRecv {
            names: vec!["General".to_owned(), "Trade".to_owned(), "LFG".to_owned()],
        },
    );
    settle(&mut app);
    let lines: Vec<String> = chat(&mut app)[before..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    assert_eq!(lines.len(), 4, "header + three channels: {lines:?}");
    assert_eq!(
        lines[0], "The following channels are available to you:",
        "the OTHER literal -- 0x0149 lists channels, 0x0148 lists characters"
    );
    let s = &app.interaction().stats;
    assert_eq!(s.channel_indices, 1);
    assert_eq!(
        s.channel_rows, 5,
        "two from the first reply and three from this one"
    );

    assert!(!dropped::unreceived(Opcode::COMMUNICATION_CHANNEL_LIST));
    assert!(!dropped::unreceived(Opcode::COMMUNICATION_CHANNEL_INDEX));
}

/// **An empty list still prints its header, and that is not an accident of the transcription.**
///
/// The header is written before the handler loads the list head. A player who runs the command on
/// an empty channel must therefore still see that the command ran.
///
/// **Falsified by** guarding the header on a non-empty list: the log takes nothing.
#[test]
fn an_empty_channel_reply_still_prints_its_header() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("channels-empty");
    let before = chat(&mut app).len();

    peer.event(
        &mut app,
        &dereth_protocol::comms::CommunicationChannelListRecv { names: Vec::new() },
    );
    settle(&mut app);

    let lines: Vec<String> = chat(&mut app)[before..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    assert_eq!(
        lines,
        vec!["The following characters are currently listening on the channel:".to_owned()],
        "the header goes out before the list is looked at"
    );
    let s = &app.interaction().stats;
    assert_eq!(s.channel_lists, 1, "the message arrived");
    assert_eq!(
        s.channel_rows, 0,
        "and named nobody -- two counters, so this is not an absence"
    );
}

/// **`/age`'s two arms, and the split is an emptiness test.**
///
/// The wire's target-name length includes the terminator, so a length of `1` is the **empty**
/// string; the handler tests string emptiness to select the format: empty
/// gives *"You have played for %s.\n"* and a named target gives *"%s has played for %s.\n"*.
///
/// **Falsified by** using the same format for both arms, or by inverting the test.
#[test]
fn a_query_age_response_prints_the_arm_its_target_name_selects() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("age");
    let before = chat(&mut app).len();

    peer.event(
        &mut app,
        &dereth_protocol::admin::CharacterQueryAgeResponse {
            target_name: String::new(),
            age: "1mo 2d 3h 4m 5s".to_owned(),
        },
    );
    peer.event(
        &mut app,
        &dereth_protocol::admin::CharacterQueryAgeResponse {
            target_name: "Aldermoore".to_owned(),
            age: "6d 7h".to_owned(),
        },
    );
    settle(&mut app);

    assert_eq!(app.interaction().stats.age_responses, 2);
    let lines: Vec<String> = chat(&mut app)[before..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    assert_eq!(
        lines,
        vec![
            "You have played for 1mo 2d 3h 4m 5s.".to_owned(),
            "Aldermoore has played for 6d 7h.".to_owned(),
        ],
        "an empty target name is the server saying 'this is you'"
    );
    assert!(!dropped::unreceived(Opcode::CHARACTER_QUERY_AGE_RESPONSE));
}
