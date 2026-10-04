use super::*;

// =============================================================================================
// examine.inscription.* -- the box, its focus edges, and what leaves on the commit
//
// The gestures below are the harness's own -- the pointer goes through the client's pump and
// input manager, and a character is the message the window loop produces after translation.
//
// **The focus edges are the whole subject**: a press that drops a caret into the box, the press
// elsewhere that commits what was typed, Escape, and a target change that takes the caret away.
// =============================================================================================

/// The box, the line under it that carries the signature, and the state the invitation is drawn
/// in.
const INSCRIPTION_TEXT: ElementId = ElementId(0x1000_013E);
const INSCRIPTION_SIGNATURE: ElementId = ElementId(0x1000_013F);
const INSCRIPTION_CENTRED: u32 = 0x1000_0050;
/// The assessment window's own close button.
const EXAM_CLOSE_BUTTON: ElementId = ElementId(0x1000_05F3);

/// The player these scenarios look out of, and the three things in their pack: two that can be
/// written on and one that cannot.
const SCRIBE: ObjectId = ObjectId(0x5000_0001);
const PARCHMENT: ObjectId = ObjectId(0x8000_0001);
const LETTER: ObjectId = ObjectId(0x8000_0002);
const PEBBLE: ObjectId = ObjectId(0x8000_0003);
/// The bit on a thing that says it can be written on.
const INSCRIBABLE: u32 = 0x0000_0002;

/// A client with a scribe and three things in their pack.
fn a_scribe_and_three_things() -> (HeadlessClient, dereth_testkit::Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = dereth_testkit::Peer::attach(&mut c, SCRIBE);
    for (id, parent) in [
        (SCRIBE, ObjectId(0)),
        (PARCHMENT, SCRIBE),
        (LETTER, SCRIBE),
        (PEBBLE, SCRIBE),
    ] {
        let mut p = dereth_protocol::objects::ObjectCreatePayload {
            id,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        p.wdesc.header |= dereth_protocol::types::weeniedesc::header::CONTAINER_ID;
        p.wdesc.container_id = Some(parent);
        peer.send(
            &mut c,
            dereth_testkit::replay::OBJECT_QUEUE,
            dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
                .expect("the create encodes"),
        );
    }
    c.tick(1);
    c.world_mut().player = Some(SCRIBE);
    c.world_mut()
        .weenie_mut(SCRIBE)
        .expect("the scribe was created")
        .pwd
        .name = "Larktest".to_owned();
    for (id, name, inscribable) in [
        (PARCHMENT, "Parchment", true),
        (LETTER, "Letter", true),
        (PEBBLE, "Pebble", false),
    ] {
        let w = c.world_mut().weenie_mut(id).expect("created");
        w.pwd.name = name.to_owned();
        if inscribable {
            w.pwd.bitfield |= INSCRIBABLE;
        }
    }
    (c, peer)
}

/// An assessment carrying whatever signature and inscription the scenario wants.
fn inscription_profile(scribe: Option<&str>, inscription: Option<&str>) -> AppraisalProfile {
    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    let mut entries = Vec::new();
    if let Some(t) = inscription {
        entries.push((7_u32, t.to_owned()));
    }
    if let Some(n) = scribe {
        entries.push((8_u32, n.to_owned()));
    }
    if !entries.is_empty() {
        p.flags |= dereth_protocol::types::appraisal::flags::STRING;
        p.tables.strings = Some(dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries,
        });
    }
    p
}

/// What the box is showing, and who may change it.
#[derive(Debug, Clone)]
struct BoxState {
    text: String,
    signature: String,
    state: u32,
    visible: bool,
    editable: bool,
    selectable: bool,
}

fn box_state(c: &mut HeadlessClient) -> BoxState {
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    let h = ui
        .get_element(INSCRIPTION_TEXT)
        .expect("the box is in the shipped layout");
    let hs = ui
        .get_element(INSCRIPTION_SIGNATURE)
        .expect("the signature line is in the layout");
    let state = ui.node(h).expect("a node").state.0;
    let visible = ui.node(h).expect("a node").region.flags.visible;
    let t = ui.text_element_mut(h).expect("a text element");
    let text = t.glyphs.inq_text(false);
    let editable = t.bits.editable();
    let selectable = t.bits.selectable();
    let signature = ui
        .text_element_mut(hs)
        .expect("a text element")
        .glyphs
        .inq_text(false);
    BoxState {
        text,
        signature,
        state,
        visible,
        editable,
        selectable,
    }
}

/// Whether the box is what the keyboard is going to -- what the player notices first.
fn box_has_the_caret(c: &mut HeadlessClient) -> bool {
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    let h = ui
        .get_element(INSCRIPTION_TEXT)
        .expect("the box is in the shipped layout");
    ui.focus_element() == Some(h)
}

/// Where a run of text is drawn inside its own box, on both axes.
///
/// `(left gap, right gap)` and `(top gap, bottom gap)`: a centred run has the two of a pair equal
/// to within the pixel integer division drops, and a run drawn hard against the corner has a gap
/// of nothing on that side.
fn text_gaps(c: &mut HeadlessClient, id: ElementId) -> ((i32, i32), (i32, i32)) {
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    let h = ui
        .get_element(id)
        .expect("the element is in the shipped layout");
    let screen = ui.screen_box(h);
    let t = ui.text_element_mut(h).expect("a text element");
    let content = t.content_box(screen);
    let placed = t.compose(screen);
    if placed.is_empty() {
        return ((0, 0), (0, 0));
    }
    let run_w: i32 = t.glyphs.glyphs.iter().map(|g| g.width).sum();
    let run_h = t.text_height(screen);
    let left = placed[0].x - content.x0;
    let top = placed[0].y - content.y0;
    (
        (left, content.width() - run_w - left),
        (top, content.height() - run_h - top),
    )
}

/// Put the caret in the box, as a press inside it does.
fn press_the_box(c: &mut HeadlessClient) {
    dereth_testkit::input_steps::focus(c, INSCRIPTION_TEXT);
    c.tick(1);
}

/// One of the two actions the box answers -- Escape, which lets the caret go, and Return, which
/// this box deliberately does not treat as a commit.
fn send_box_action(c: &mut HeadlessClient, action: u32) {
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let Some(h) = ui.focus_element() else { return };
        ui.dispatch_action(
            h,
            &dereth_ui::focus::InputEvent {
                action,
                start: true,
                x: 0,
                y: 0,
            },
        );
    }
    c.tick(2);
}

fn press_escape_in_the_box(c: &mut HeadlessClient) {
    send_box_action(c, dereth_ui::focus::action::ESCAPE);
}

/// Rub out `n` characters, one key press each.
fn backspace(c: &mut HeadlessClient, n: usize) {
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let h = ui.focus_element().expect("the box has the caret");
        for _ in 0..n {
            ui.dispatch_action(
                h,
                &dereth_ui::focus::InputEvent {
                    action: dereth_ui::focus::action::BACKSPACE,
                    start: true,
                    x: 0,
                    y: 0,
                },
            );
        }
    }
    c.tick(1);
}

/// Let the caret go without pressing anything, which is what any other window taking it does.
fn drop_the_caret(c: &mut HeadlessClient) {
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let h = ui
            .get_element(INSCRIPTION_TEXT)
            .expect("the box is in the shipped layout");
        ui.relinquish_focus(h);
    }
    c.tick(2);
}

/// Look at another thing **without a pointer gesture**: the pane is told what it is waiting for
/// and the shard answers it.
///
/// It is not [`assess`] because that one presses the identify button, and a press anywhere else on
/// the screen is itself a focus edge -- which is the very thing these two claims are about.
fn look_at(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    profile: &AppraisalProfile,
) {
    await_the_answer_for(c, object);
    deliver_the_answer(c, peer, object, profile);
}

/// Every inscription this client has really put on the wire since the last look.
fn inscriptions_sent(c: &HeadlessClient) -> Vec<dereth_protocol::trade::WritingSetInscription> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SetInscription(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

/// **The invitation is centred and a real inscription is not.** The empty box says where to write
/// and says it in the middle of itself; a box that carries somebody's words draws them from the
/// corner, with the signature under it.
pub(super) fn the_invitation_is_centred_and_a_real_inscription_is_not() {
    let (mut c, mut peer) = a_scribe_and_three_things();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    let empty = box_state(&mut c);
    let (h_gaps, v_gaps) = text_gaps(&mut c, INSCRIPTION_TEXT);
    let centred = empty.text == "<Inscribe here>"
        && empty.state == INSCRIPTION_CENTRED
        && (h_gaps.0 - h_gaps.1).abs() <= 1
        && h_gaps.0 > 0
        && (v_gaps.0 - v_gaps.1).abs() <= 1
        && v_gaps.0 > 0;

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Aldwyne"), Some("For a friend.")),
    );
    let written = box_state(&mut c);
    let (h_gaps, v_gaps) = text_gaps(&mut c, INSCRIPTION_TEXT);
    let not_centred = written.text == "For a friend."
        && written.state != INSCRIPTION_CENTRED
        && h_gaps.0 == 0
        && v_gaps.0 == 0
        && written.signature == "--Aldwyne";

    c.assert_behaviour(
        "examine.inscription.the-invitation-is-centred-and-a-real-inscription-is-not",
        move |_| centred && not_centred,
    );
    c.shutdown();
}

/// **A press inside the box.** A press inside the box empties the invitation out of it rather
/// than dropping a caret into the literal words, and puts the player's own signature under it so
/// they can see whose name they are about to leave -- and nothing but a press inside it gives the
/// box the keyboard, so assessing a thing does not quietly take it.
pub(super) fn a_press_empties_the_box_and_shows_whose_signature_it_will_carry() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    let before = box_state(&mut c);
    let untouched = before.text == "<Inscribe here>" && before.signature.is_empty();
    let no_caret_from_assessing = !box_has_the_caret(&mut c);

    press_the_box(&mut c);
    let after = box_state(&mut c);
    let emptied = after.text.is_empty()
        && after.state != INSCRIPTION_CENTRED
        && after.signature == "--Larktest";
    let has_caret = box_has_the_caret(&mut c);

    c.assert_behaviour(
        "examine.inscription.a-press-empties-the-box-and-shows-whose-signature-it-will-carry",
        move |_| untouched && no_caret_from_assessing && emptied && has_caret,
    );
    c.shutdown();
}

/// **The inscription persists.** Letting the caret go is what sends
/// what was written; the box keeps showing it and the signature keeps showing who wrote it, and
/// the same thing assessed again comes back with both, still the writer's to change -- with a
/// second look saying nothing more.
pub(super) fn letting_the_caret_go_sends_what_was_written() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Property of Larktest");
    drop_the_caret(&mut c);

    let sent = inscriptions_sent(&c);
    let on_the_wire = sent
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: "Property of Larktest".to_owned(),
        }];
    let kept = {
        let b = box_state(&mut c);
        b.text == "Property of Larktest" && b.signature == "--Larktest"
    };

    // The shard took it, and answers the next look with both strings.
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Larktest"), Some("Property of Larktest")),
    );
    let b = box_state(&mut c);
    let came_back = b.text == "Property of Larktest"
        && b.signature == "--Larktest"
        && b.state != INSCRIPTION_CENTRED
        && b.editable;

    // ...and looking in and out again without changing a letter is not a second inscription.
    press_the_box(&mut c);
    drop_the_caret(&mut c);
    let no_second = inscriptions_sent(&c).len() == 1;

    c.assert_behaviour(
        "examine.inscription.letting-the-caret-go-sends-what-was-written-and-it-comes-back",
        move |_| on_the_wire && kept && came_back && no_second,
    );
    c.shutdown();
}

/// An empty box on a thing nobody has signed is nothing to say: the caret going in and out again
/// sends nothing and the invitation comes back, centred.
pub(super) fn leaving_an_unchanged_box_sends_nothing() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    press_the_box(&mut c);
    drop_the_caret(&mut c);
    let nothing_sent = inscriptions_sent(&c).is_empty();
    let b = box_state(&mut c);
    let invitation_back = b.text == "<Inscribe here>" && b.state == INSCRIPTION_CENTRED;

    // Write something, leave, come back, leave again without touching it: one inscription.
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Mine");
    drop_the_caret(&mut c);
    let one = inscriptions_sent(&c).len() == 1;
    press_the_box(&mut c);
    drop_the_caret(&mut c);
    let still_one = inscriptions_sent(&c).len() == 1;

    c.assert_behaviour(
        "examine.inscription.leaving-an-unchanged-box-sends-nothing-and-the-invitation-comes-back",
        move |_| nothing_sent && invitation_back && one && still_one,
    );
    c.shutdown();
}

/// **The premise the report got the wrong way round.** The shipped data says Escape is the edge
/// that confirms an inscription and that Return is not: the box carries the attribute for one and
/// not the other, Return is swallowed and the caret stays, and Escape lets the caret go -- which
/// is what sends.
pub(super) fn escape_is_the_confirm_edge_and_return_is_not() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    // The shipped attributes, so this stands on the data and not on a belief about it.
    let (escape_attr, accept_attr) = {
        let ui = &c.app_mut().ui().expect("the shell is up").ui;
        let h = ui
            .get_element(INSCRIPTION_TEXT)
            .expect("the box is in the shipped layout");
        let p = ui.node(h).expect("a node").merged_properties();
        (
            p.get_bool(dereth_ui::props::attr::TEXT_LOSE_FOCUS_ON_ESCAPE),
            p.get_bool(dereth_ui::props::attr::TEXT_LOSE_FOCUS_ON_ACCEPT),
        )
    };
    let shipped = escape_attr == Some(true) && accept_attr.is_none();

    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");

    send_box_action(&mut c, dereth_ui::focus::action::ACCEPT);
    let return_does_nothing = inscriptions_sent(&c).is_empty() && box_has_the_caret(&mut c);

    press_escape_in_the_box(&mut c);
    let escape_commits = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: "Ink".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.escape-is-the-edge-that-confirms-it-and-return-is-not",
        move |_| shipped && return_does_nothing && escape_commits,
    );
    c.shutdown();
}

/// A box somebody else signed is not the player's to change, and whatever is typed at it goes
/// nowhere -- while their own signature does not lock them out, and a thing that is not in their
/// possession is not theirs to write on either.
pub(super) fn a_box_somebody_else_signed_is_not_yours_to_change() {
    let (mut c, mut peer) = a_scribe_and_three_things();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Aldwyne"), Some("For a friend.")),
    );
    let b = box_state(&mut c);
    let locked = !b.editable && !b.selectable;

    // ...and nothing typed at it reaches the shard: the backing plate has the pointer, so a press
    // never reaches the text at all.
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "mine now");
    press_escape_in_the_box(&mut c);
    let silent = inscriptions_sent(&c).is_empty();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Larktest"), Some("Mine.")),
    );
    let own = box_state(&mut c).editable;

    // A thing that is not in the player's possession is not theirs either.
    c.world_mut()
        .weenie_mut(PARCHMENT)
        .expect("the parchment")
        .pwd
        .container_id = Some(ObjectId(0x7000_0009));
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    let not_yours = !box_state(&mut c).editable;

    c.assert_behaviour(
        "examine.inscription.a-box-somebody-else-signed-is-not-yours-to-change",
        move |_| locked && silent && own && not_yours,
    );
    c.shutdown();
}

/// **Signed and blank.** A thing somebody has signed but written nothing on shows a box
/// that is there and blank -- still centred, with no signature under it -- and it is not the
/// player's to change unless the signature is their own, in which case the box is theirs to write
/// on and the press does not wipe what is not there.
pub(super) fn a_signed_thing_with_no_words_shows_a_blank_box() {
    let (mut c, mut peer) = a_scribe_and_three_things();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Aldwyne"), None),
    );
    let b = box_state(&mut c);
    let blank_and_locked = b.text.is_empty()
        && b.visible
        && b.state == INSCRIPTION_CENTRED
        && b.signature.is_empty()
        && !b.editable
        && !b.selectable;

    assess(
        &mut c,
        &mut peer,
        LETTER,
        &inscription_profile(Some("Larktest"), None),
    );
    let b = box_state(&mut c);
    let blank_and_yours = b.text.is_empty() && b.editable;

    press_the_box(&mut c);
    let after = box_state(&mut c);
    // Nothing to wipe, so nothing is wiped -- and the signature preview runs on every press and
    // not only on the empty-scribe one.
    let preview = after.text.is_empty() && after.signature == "--Larktest";

    dereth_testkit::input_steps::type_text(&mut c, "Ink");
    press_escape_in_the_box(&mut c);
    let sent = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: LETTER,
            text: "Ink".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.a-signed-thing-with-no-words-shows-a-blank-box-that-is-the-scribes",
        move |_| blank_and_locked && blank_and_yours && preview && sent,
    );
    c.shutdown();
}

/// **The erase half.** The writer rubbing their own words out sends an
/// empty inscription, the centred invitation comes back with the signature line cleared -- and the
/// thing is anybody's to sign again.
pub(super) fn the_scribe_can_rub_out_his_own_words() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(Some("Larktest"), Some("Ink")),
    );
    let starts_with_words = box_state(&mut c).text == "Ink";

    press_the_box(&mut c);
    backspace(&mut c, 8);
    press_escape_in_the_box(&mut c);

    let erased = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: String::new(),
        }];
    let b = box_state(&mut c);
    let invitation_back =
        b.text == "<Inscribe here>" && b.state == INSCRIPTION_CENTRED && b.signature.is_empty();

    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    let b = box_state(&mut c);
    let anybodys = b.text == "<Inscribe here>" && b.editable;

    c.assert_behaviour(
        "examine.inscription.the-writer-can-rub-out-his-own-words-and-the-invitation-comes-back",
        move |_| starts_with_words && erased && invitation_back && anybodys,
    );
    c.shutdown();
}

/// **The commit a player really makes.** Pressing the window's own close button takes the caret
/// away before the window goes, so what was typed is sent on the way out rather than lost.
pub(super) fn the_close_button_commits_on_its_way_out() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );

    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");
    c.when(Player::click(EXAM_CLOSE_BUTTON));
    c.tick(2);

    let sent = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: PARCHMENT,
            text: "Ink".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.the-windows-close-button-commits-what-was-typed-on-its-way-out",
        move |_| sent,
    );
    c.shutdown();
}

/// **Changing target.** Looking at something the player may not write on
/// -- or at something that cannot be written on at all -- takes the caret away with it, and what
/// was typed is discarded rather than sent to either thing; the keyboard stops going to the box,
/// which is the symptom.
pub(super) fn changing_the_target_to_one_you_may_not_write_on_drops_the_caret() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");
    let writing = box_has_the_caret(&mut c) && box_state(&mut c).text == "Ink";

    // Somebody else's, and signed.
    look_at(
        &mut c,
        &mut peer,
        LETTER,
        &inscription_profile(Some("Aldwyne"), Some("Bob's")),
    );
    let dropped = !box_has_the_caret(&mut c);
    let b = box_state(&mut c);
    let shows_the_new_one =
        !b.editable && !b.selectable && b.text == "Bob's" && b.signature == "--Aldwyne";
    let discarded = inscriptions_sent(&c).is_empty();

    // ...and the keyboard really has stopped going there.
    dereth_testkit::input_steps::type_text(&mut c, "more");
    let no_insertion = box_state(&mut c).text == "Bob's";

    // The other way the caret goes: a thing that cannot be written on at all.
    let (mut c2, mut peer2) = a_scribe_and_three_things();
    assess(
        &mut c2,
        &mut peer2,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    press_the_box(&mut c2);
    dereth_testkit::input_steps::type_text(&mut c2, "Ink");
    look_at(
        &mut c2,
        &mut peer2,
        PEBBLE,
        &inscription_profile(None, None),
    );
    let pebble_dropped = !box_has_the_caret(&mut c2);
    let b = box_state(&mut c2);
    let pebble_silent = !b.editable
        && b.text == "<Inscribe here>"
        && b.signature.is_empty()
        && inscriptions_sent(&c2).is_empty();
    c2.shutdown();

    c.assert_behaviour(
        "examine.inscription.looking-at-one-you-may-not-write-on-takes-the-caret-away",
        move |_| {
            writing
                && dropped
                && shows_the_new_one
                && discarded
                && no_insertion
                && pebble_dropped
                && pebble_silent
        },
    );
    c.shutdown();
}

/// **The half the fix must not overreach.** Looking at another thing the player *may* write on
/// keeps the caret exactly where it was and sends nothing -- and after a drop, the next
/// inscription still commits, once, carrying the second set of words and naming the thing the box
/// was showing.
pub(super) fn changing_to_another_one_you_may_write_on_keeps_the_caret() {
    let (mut c, mut peer) = a_scribe_and_three_things();
    assess(
        &mut c,
        &mut peer,
        PARCHMENT,
        &inscription_profile(None, None),
    );
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Ink");

    look_at(&mut c, &mut peer, LETTER, &inscription_profile(None, None));
    let kept = box_has_the_caret(&mut c) && box_state(&mut c).editable;
    let silent = inscriptions_sent(&c).is_empty();

    // A drop, and then a second inscription that still works.
    look_at(&mut c, &mut peer, PEBBLE, &inscription_profile(None, None));
    let dropped = !box_has_the_caret(&mut c);
    look_at(&mut c, &mut peer, LETTER, &inscription_profile(None, None));
    press_the_box(&mut c);
    dereth_testkit::input_steps::type_text(&mut c, "Second");
    press_escape_in_the_box(&mut c);
    let committed = inscriptions_sent(&c)
        == vec![dereth_protocol::trade::WritingSetInscription {
            object_id: LETTER,
            text: "Second".to_owned(),
        }];

    c.assert_behaviour(
        "examine.inscription.looking-at-another-you-may-write-on-keeps-the-caret-where-it-was",
        move |_| kept && silent && dropped && committed,
    );
    c.shutdown();
}
