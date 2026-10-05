use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::{Message as _, Opcode};
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound, Player, ScreenPoint, Target};
use dereth_ui::ElemHandle;
use dereth_ui_screens::items::widget::ItemListWidget;
use dereth_ui_screens::panels::salvage::{self, ButtonState as SalvageButton};

use super::{centre, gameplay_screen, open_pack};

// =========================================================================================
// The row a drop leaves behind, and that it does not stay.
// =========================================================================================

/// The recording the drop is in.
const DROP_SESSION: &str = "long-solo-play";
/// That recording's own character.
const GHOST_PLAYER: ObjectId = ObjectId(0x5000_000A);
/// The one thing the promoted recording successfully drops: a Sack. It is a container, so its
/// row lives on the strip of side packs.
const SACK: ObjectId = ObjectId(0x8000_0997);
/// The same recorded description with its two capacities and its pack-slot bit cleared, so
/// that the identical eight messages are driven through the backpack grid as well. Its id is
/// not on the wire and is chosen here; every other byte is the recording's.
const LOOSE: ObjectId = ObjectId(0x8000_0996);

/// One message of one of the two chains: a blob of the recording, named by its index, or one
/// the scenario built for the derived subject.
#[derive(Clone)]
enum Msg {
    Recorded(usize),
    Built(SessionEvent),
}

/// The drop's two chains, found in the recording rather than written down.
struct Chains {
    /// The create that puts the thing back in the pack, for the reset between orders.
    login: ObjectCreatePayload,
    /// The four messages the shard puts on the interface queue, in its own order.
    ui: Vec<Msg>,
    /// The four it puts on the object queue, in its own order.
    sb: Vec<Msg>,
}

fn corpus() -> Corpus {
    Corpus::load(DROP_SESSION)
        .expect("the recording decodes")
        .expect("the recording is in the decoded corpus")
}

fn u32_at(p: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(p[at..at + 4].try_into().expect("four bytes"))
}

fn s2c<'a>(c: &'a Corpus, f: impl Fn(&CorpusBlob) -> bool + 'a) -> Vec<&'a CorpusBlob> {
    c.blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient && f(b))
        .collect()
}

/// The drop, read out of the recording: the two chains by blob index, and the create that
/// puts the Sack back where it started.
///
/// Every index is **found**, never written down: an index copied by hand goes stale the day
/// the recording is re-promoted and says nothing when it does.
fn drop_chains() -> Chains {
    let c = corpus();
    let creates = s2c(&c, |b| {
        b.opcode == 0xF745 && u32_at(&b.payload, 4) == SACK.0
    });
    assert_eq!(
        creates.len(),
        2,
        "the recording creates the Sack twice: once at login and once when it lands on the \
             ground"
    );
    let read = |b: &CorpusBlob| {
        ItemCreateObject::read(&mut dereth_protocol::Reader::new(&b.payload[4..]))
            .expect("the recorded create decodes")
            .0
    };
    let login = read(creates[0]);
    let dropped = read(creates[1]);
    assert_eq!(
        login.physicsdesc.timestamps.instance, dropped.physicsdesc.timestamps.instance,
        "both creates carry the same instance, so the drop's is a merge into the thing that \
             is already there -- which is the case this scenario is about"
    );
    assert_eq!(
        login.wdesc.container_id,
        Some(GHOST_PLAYER),
        "it starts in the player's pack"
    );
    assert_ne!(
        dropped.wdesc.container_id,
        Some(GHOST_PLAYER),
        "and the drop says it is not"
    );

    // The two "this thing is in no container any more" messages, and the weight update the
    // recording puts between them.
    let instance = s2c(&c, |b| {
        b.opcode == 0x02DA
            && b.payload.len() >= 17
            && u32_at(&b.payload, 5) == SACK.0
            && u32_at(&b.payload, 9) == 2
            && u32_at(&b.payload, 13) == 0
    });
    assert_eq!(instance.len(), 2, "the shard says it twice on a drop");
    let moved = s2c(&c, |b| {
        b.opcode == 0xF7B0 && b.payload.len() >= 20 && u32_at(&b.payload, 12) == 0x019A
    });
    assert_eq!(
        moved.len(),
        1,
        "one move answer in the whole recording, and it is this drop's"
    );
    assert_eq!(
        u32_at(&moved[0].payload, 16),
        SACK.0,
        "and it names the Sack"
    );
    let weight = s2c(&c, |b| {
        b.opcode == 0x02CD && b.idx > instance[0].idx && b.idx < instance[1].idx
    });
    let weight = *weight.first().expect("the burden update between the two");

    let ui = vec![
        Msg::Recorded(instance[0].idx),
        Msg::Recorded(weight.idx),
        Msg::Recorded(instance[1].idx),
        Msg::Recorded(moved[0].idx),
    ];

    // The create the drop produces, and the three movement messages the recording puts after
    // it, in their recorded order.
    let create = creates[1];
    let mut sb = vec![Msg::Recorded(create.idx)];
    for b in c.blobs.iter().filter(|b| {
        b.dir == Direction::ServerToClient
            && b.idx > create.idx
            && b.idx < create.idx + 5
            && matches!(b.opcode, 0xF748 | 0xF74B)
            && b.payload.len() >= 8
            && u32_at(&b.payload, 4) == SACK.0
    }) {
        sb.push(Msg::Recorded(b.idx));
    }
    assert_eq!(
        sb.len(),
        4,
        "the create and the three movement messages after it"
    );
    Chains { login, ui, sb }
}

/// The recorded description with the three facts that decide *which* list a thing lands in
/// cleared, so the same sequence can be driven through the backpack grid as well.
fn as_loose(mut p: ObjectCreatePayload) -> ObjectCreatePayload {
    use dereth_protocol::types::weeniedesc::header;
    p.id = LOOSE;
    p.wdesc.header &= !(header::ITEMS_CAPACITY | header::CONTAINERS_CAPACITY);
    p.wdesc.items_capacity = None;
    p.wdesc.containers_capacity = None;
    p.wdesc.bitfield &=
        !dereth_client_model::inventory::use_object::use_bitfield::REQUIRES_PACK_SLOT;
    p
}

/// The same eight messages, re-aimed at the derived subject. These cannot be blobs of the
/// recording -- the recording never carried them -- so they are built here, out of the
/// recording's own bytes with one field changed.
fn loose_chains(recorded: &Chains) -> Chains {
    let c = corpus();
    let blob = |m: &Msg| -> CorpusBlob {
        let Msg::Recorded(i) = m else {
            panic!("the recorded chain is all blobs")
        };
        c.blobs
            .iter()
            .find(|b| b.dir == Direction::ServerToClient && b.idx == *i)
            .expect("the index was found in this same recording")
            .clone()
    };
    let login = as_loose(recorded.login.clone());
    // The derived description has to survive its own encoder, or every field after the two
    // cleared capacities lands one byte out and the subject is a different thing entirely.
    {
        let bytes = dereth_protocol::write_body(&ItemCreateObject(login.clone())).expect("encode");
        let back = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&bytes))
            .expect("the derived create decodes")
            .0;
        assert_eq!(back.id, LOOSE);
        assert_eq!(
            back.wdesc, login.wdesc,
            "the derived description re-encodes to itself"
        );
    }
    let sb = recorded
        .sb
        .iter()
        .map(|m| {
            let b = blob(m);
            if b.opcode == 0xF745 {
                let p = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&b.payload[4..]))
                    .expect("re-decode")
                    .0;
                let body =
                    dereth_protocol::write_body(&ItemCreateObject(as_loose(p))).expect("re-encode");
                Msg::Built(SessionEvent::WorldObject {
                    opcode: Opcode(0xF745),
                    body,
                })
            } else {
                let mut body = b.payload[4..].to_vec();
                body[0..4].copy_from_slice(&LOOSE.0.to_le_bytes());
                Msg::Built(SessionEvent::WorldObject {
                    opcode: Opcode(b.opcode),
                    body,
                })
            }
        })
        .collect();
    let ui = recorded
        .ui
        .iter()
        .map(|m| {
            let b = blob(m);
            let (opcode, mut body) = if b.opcode == 0xF7B0 {
                (Opcode(u32_at(&b.payload, 12)), b.payload[12..].to_vec())
            } else {
                (Opcode(b.opcode), b.payload.clone())
            };
            match opcode.0 {
                0x02DA => body[5..9].copy_from_slice(&LOOSE.0.to_le_bytes()),
                0x019A => body[4..8].copy_from_slice(&LOOSE.0.to_le_bytes()),
                _ => {}
            }
            Msg::Built(SessionEvent::UiEvent { opcode, blob: body })
        })
        .collect();
    Chains { login, ui, sb }
}

/// Every arrival order two ordered chains of four permit -- seventy of them. Each entry's set
/// bits are the places the interface queue's messages take.
fn arrival_orders() -> Vec<[bool; 8]> {
    let mut out = Vec::new();
    for mask in 0_u32..256 {
        if mask.count_ones() != 4 {
            continue;
        }
        let mut slots = [false; 8];
        for (i, s) in slots.iter_mut().enumerate() {
            *s = mask & (1 << i) != 0;
        }
        out.push(slots);
    }
    assert_eq!(out.len(), 70, "the orders two queues of four permit");
    out
}

/// Every list the pack window owns, and which slot of it shows `id`.
fn rows_for(c: &mut HeadlessClient, id: ObjectId) -> Vec<(&'static str, usize)> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    let inv = &screen.inventory;
    let named: [(&'static str, Vec<&ItemListWidget>); 4] = [
        ("the pack's own row", inv.top_container.iter().collect()),
        (
            "the strip of side packs",
            inv.container_list.iter().collect(),
        ),
        ("the backpack grid", inv.item_list.iter().collect()),
        ("the figure", inv.doll.iter().map(|(_, w)| w).collect()),
    ];
    let mut out = Vec::new();
    for (name, lists) in named {
        for w in lists {
            for (i, s) in w.slots.iter().enumerate() {
                if s.item == Some(id) {
                    out.push((name, i));
                }
            }
        }
    }
    out
}

fn tile_of(c: &mut HeadlessClient, id: ObjectId) -> Option<ElemHandle> {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    let inv = &screen.inventory;
    inv.top_container
        .iter()
        .chain(inv.container_list.iter())
        .chain(inv.item_list.iter())
        .chain(inv.doll.iter().map(|(_, w)| w))
        .find_map(|w| {
            w.slots
                .iter()
                .find(|s| s.item == Some(id))
                .map(|s| s.handle)
        })
}

fn deliver(c: &mut HeadlessClient, seq: &[Msg]) {
    if seq.iter().all(|m| matches!(m, Msg::Recorded(_))) {
        // One step, so the recording is read once for the whole order rather than once a
        // message: `Inbound::blobs` is exactly "these blobs, in this order".
        let idx: Vec<usize> = seq
            .iter()
            .map(|m| match m {
                Msg::Recorded(i) => *i,
                Msg::Built(_) => unreachable!("checked above"),
            })
            .collect();
        c.when(Inbound::blobs(DROP_SESSION, &idx));
        return;
    }
    for m in seq {
        match m {
            Msg::Recorded(i) => {
                c.when(Inbound::blobs(DROP_SESSION, &[*i]));
            }
            Msg::Built(e) => {
                c.when(Inbound::event(e.clone()));
            }
        }
    }
}

/// What the pack showed after one arrival order, and what a click where the row used to be
/// reached.
struct Outcome {
    rows: Vec<(&'static str, usize)>,
    selected: Option<ObjectId>,
    opened: Option<ObjectId>,
}

fn run_one(
    c: &mut HeadlessClient,
    subject: ObjectId,
    chains: &Chains,
    order: [bool; 8],
    t: f64,
) -> Outcome {
    // Put the subject back in the pack, from the recording's own login create.
    {
        let w = &mut c.objects_mut().world;
        w.set_selected_object(None, false, &mut dereth_client_model::NullSink);
        w.delete_object(subject, ServerTime(t), &mut dereth_client_model::NullSink);
    }
    c.when(Inbound::world_view(&ItemCreateObject(chains.login.clone())));
    // Harness hygiene, and not a claim about the client: the previous order's click may have
    // left a side pack open, and the grid would then be showing that pack's contents.
    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.inventory.open_container = Some(GHOST_PLAYER);
    }
    c.tick(1);
    let before = rows_for(c, subject);
    assert_eq!(
        before.len(),
        1,
        "the control: {subject:?} has exactly one row in the pack before the drop, got \
             {before:?}"
    );
    let tile = tile_of(c, subject).expect("the row has a tile to take the coordinates of");
    let at = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        centre(ui, tile)
    };

    let (mut u, mut s) = (0_usize, 0_usize);
    let mut seq = Vec::with_capacity(8);
    for from_ui in order {
        if from_ui {
            seq.push(chains.ui[u].clone());
            u += 1;
        } else {
            seq.push(chains.sb[s].clone());
            s += 1;
        }
    }
    deliver(c, &seq);
    c.tick(1);
    let rows = rows_for(c, subject);

    c.when(Player::Click(Target::Point(ScreenPoint::new(at.0, at.1))));
    let selected = c.view().world().selected;
    let opened = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.inventory.open_container
    };
    Outcome {
        rows,
        selected,
        opened,
    }
}

fn describe(order: [bool; 8]) -> String {
    order.iter().map(|u| if *u { 'U' } else { 'S' }).collect()
}

/// A client with the recording's own character, his pack open, and nothing else.
fn a_client_with_the_recorded_pack_open() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    {
        // The recording's login description is not replayed here, so the two facts the pack
        // window reads off the player -- what he is carrying and how much he can carry -- are
        // the premise.
        let w = &mut c.objects_mut().world;
        w.player = Some(GHOST_PLAYER);
        let mut p = dereth_client_model::Weenie::new(GHOST_PLAYER);
        p.valid = true;
        p.pwd.name = "lark".into();
        p.pwd.items_capacity = Some(102);
        p.pwd.containers_capacity = Some(7);
        p.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
        w.tables.weenies.insert(GHOST_PLAYER, p);
        w.tables.inventories.insert(
            GHOST_PLAYER,
            dereth_client_model::objects::ObjectInventory::new(GHOST_PLAYER),
        );
    }
    open_pack(&mut c);
    c.tick(1);
    c
}

/// The drop the recording carries, replayed in **every** order the shard's two queues permit,
/// against the recorded container and against the same description as a loose thing.
///
/// The premise is the recording's: its own bytes, its own character, and the pack window the
/// shipped layout draws. The claim is that the row goes, and that a click where it was
/// reaches nothing.
pub fn a_dropped_things_row_leaves_the_pack_whichever_answer_comes_first() {
    let recorded = drop_chains();
    let loose = loose_chains(&recorded);
    let mut c = a_client_with_the_recorded_pack_open();

    let mut ghosts: Vec<String> = Vec::new();
    let mut reachable: Vec<String> = Vec::new();
    let mut t = 10.0;
    for (subject, chains) in [(SACK, &recorded), (LOOSE, &loose)] {
        for order in arrival_orders() {
            t += 1.0;
            let out = run_one(&mut c, subject, chains, order, t);
            if !out.rows.is_empty() {
                ghosts.push(format!(
                    "{subject:?} order {} left {:?}",
                    describe(order),
                    out.rows
                ));
            }
            if out.selected == Some(subject) || out.opened == Some(subject) {
                reachable.push(format!(
                    "{subject:?} order {} -- the click selected {:?} and opened {:?}",
                    describe(order),
                    out.selected,
                    out.opened
                ));
            }
        }
    }

    // Both halves in one verdict, so the second is measured even when the first fails.
    let no_ghost = ghosts.is_empty();
    let out_of_reach = reachable.is_empty();
    assert!(
        no_ghost && out_of_reach,
        "{} of 140 arrival orders left a row behind; {} of 140 let a click reach the dropped \
             thing.\n{}\n{}",
        ghosts.len(),
        reachable.len(),
        ghosts.join("\n"),
        reachable.join("\n")
    );
    c.assert_behaviour(
        "inventory.world-drop.a-dropped-things-row-leaves-the-pack-whichever-answer-comes-first",
        move |_| no_ghost && out_of_reach,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_a_dropped_things_row_leaves_the_pack_whichever_answer_comes_first => a_dropped_things_row_leaves_the_pack_whichever_answer_comes_first ["inventory.world-drop.a-dropped-things-row-leaves-the-pack-whichever-answer-comes-first"],
    scenario_using_a_tinkering_tool_opens_it_empty_and_remembers_the_tool => using_a_tinkering_tool_opens_it_empty_and_remembers_the_tool ["salvage.window.using-a-tinkering-tool-opens-it-empty-and-remembers-the-tool"],
    scenario_only_a_suitable_thing_goes_in_and_the_first_one_fixes_the_material => only_a_suitable_thing_goes_in_and_the_first_one_fixes_the_material ["salvage.window.only-a-suitable-thing-goes-in-and-the-first-one-fixes-the-material"],
    scenario_the_second_material_only_goes_in_when_the_player_asked_for_it => the_second_material_only_goes_in_when_the_player_asked_for_it ["salvage.window.the-second-material-only-goes-in-when-the-player-asked-for-it"],
    scenario_the_button_sends_the_tool_and_every_row_and_empties_the_window => the_button_sends_the_tool_and_every_row_and_empties_the_window ["salvage.button.the-button-sends-the-tool-and-every-row-and-empties-the-window"],
    scenario_a_double_click_takes_a_row_out_and_a_single_click_does_not => a_double_click_takes_a_row_out_and_a_single_click_does_not ["salvage.row.a-double-click-takes-a-row-out-and-a-single-click-does-not"],
    scenario_closing_it_empties_it_and_forgets_the_tool => closing_it_empties_it_and_forgets_the_tool ["salvage.window.closing-it-empties-it-and-forgets-the-tool"],
    scenario_the_shards_report_is_read_out_on_the_scroll => the_shards_report_is_read_out_on_the_scroll ["salvage.report.the-shards-report-is-read-out-on-the-scroll"],
    scenario_a_thing_carried_over_it_is_marked_as_welcome_or_as_refused => a_thing_carried_over_it_is_marked_as_welcome_or_as_refused ["salvage.window.a-thing-carried-over-it-is-marked-as-welcome-or-as-refused"],
    scenario_the_mark_comes_off_the_tile_as_soon_as_the_drop_lands => the_mark_comes_off_the_tile_as_soon_as_the_drop_lands ["salvage.window.the-mark-comes-off-the-tile-as-soon-as-the-drop-lands"],
    scenario_a_shortcut_carried_over_it_is_marked_neither_way => a_shortcut_carried_over_it_is_marked_neither_way ["salvage.window.a-shortcut-carried-over-it-is-marked-neither-way"],
    scenario_a_bag_of_salvage_says_what_it_is_made_of => a_bag_of_salvage_says_what_it_is_made_of ["salvage.bag.a-bag-of-salvage-says-what-it-is-made-of"],
    scenario_the_split_controls_are_shown_for_a_stack_and_for_nothing_else => the_split_controls_are_shown_for_a_stack_and_for_nothing_else ["inventory.split.the-split-controls-are-shown-for-a-stack-and-for-nothing-else"],
    scenario_picking_a_stack_sets_the_quantity_to_that_stacks_own_count => picking_a_stack_sets_the_quantity_to_that_stacks_own_count ["inventory.split.picking-a-stack-sets-the-quantity-to-that-stacks-own-count"],
    scenario_the_quantity_the_player_chose_is_not_taken_back_by_the_frames_after_it => the_quantity_the_player_chose_is_not_taken_back_by_the_frames_after_it ["inventory.split.the-quantity-the-player-chose-is-not-taken-back-by-the-frames-after-it"],
    scenario_the_split_key_puts_the_caret_in_the_quantity_and_does_nothing_else => the_split_key_puts_the_caret_in_the_quantity_and_does_nothing_else ["inventory.split.the-split-key-puts-the-caret-in-the-quantity-and-does-nothing-else"],
    scenario_the_slider_chooses_the_quantity_on_the_drag_itself => the_slider_chooses_the_quantity_on_the_drag_itself ["inventory.split.the-slider-chooses-the-quantity-on-the-drag-itself"],
    scenario_a_typed_quantity_is_taken_when_the_caret_leaves_the_box => a_typed_quantity_is_taken_when_the_caret_leaves_the_box ["inventory.split.a-typed-quantity-is-taken-when-the-caret-leaves-the-box"],
    scenario_a_quantity_larger_than_the_stack_or_none_at_all_is_corrected_in_the_box => a_quantity_larger_than_the_stack_or_none_at_all_is_corrected_in_the_box ["inventory.split.a-quantity-larger-than-the-stack-or-none-at-all-is-corrected-in-the-box"],
    scenario_picking_a_different_thing_sets_the_quantity_back_to_all_of_it => picking_a_different_thing_sets_the_quantity_back_to_all_of_it ["inventory.split.picking-a-different-thing-sets-the-quantity-back-to-all-of-it"],
    scenario_two_splits_in_a_row_each_go_out_and_the_shards_answers_follow_them => two_splits_in_a_row_each_go_out_and_the_shards_answers_follow_them ["inventory.split.two-splits-in-a-row-each-go-out-and-the-shards-answers-follow-them"],
    scenario_a_leading_zero_and_a_number_too_big_are_read_the_way_the_client_reads_them => a_leading_zero_and_a_number_too_big_are_read_the_way_the_client_reads_them ["inventory.split.a-leading-zero-and-a-number-too-big-are-read-the-way-the-client-reads-them"],
    scenario_escape_in_the_box_changes_nothing_and_cancel_puts_the_taken_quantity_back => escape_in_the_box_changes_nothing_and_cancel_puts_the_taken_quantity_back ["inventory.split.escape-in-the-box-changes-nothing-and-cancel-puts-the-taken-quantity-back"],
    scenario_a_drag_that_ends_nowhere_and_a_refusal_both_free_the_stack => a_drag_that_ends_nowhere_and_a_refusal_both_free_the_stack ["inventory.split.a-drag-that-ends-nowhere-and-a-refusal-both-free-the-stack"],
    scenario_the_sliders_two_ends_are_one_of_them_and_all_of_them => the_sliders_two_ends_are_one_of_them_and_all_of_them ["inventory.split.the-sliders-two-ends-are-one-of-them-and-all-of-them"],
    scenario_starting_to_drag_the_stack_takes_the_quantity_in_the_box => starting_to_drag_the_stack_takes_the_quantity_in_the_box ["inventory.split.starting-to-drag-the-stack-takes-the-quantity-in-the-box"],
}

// =========================================================================================
// The window a tinkering tool opens.
// =========================================================================================

/// Two more suitable things of the same material as the fixture's ring, one that is fully
/// repaired, one of another material, one that is not the player's, and the bag the shard
/// makes out of what is salvaged.
const BAND: ObjectId = ObjectId(0x5000_0011);
const PRISTINE: ObjectId = ObjectId(0x5000_0012);
const FIGURINE: ObjectId = ObjectId(0x5000_0013);
const NOT_MINE: ObjectId = ObjectId(0x5000_0014);
const BAG: ObjectId = ObjectId(0x5000_0015);

/// The material the fixture's ring is made of, the one the figurine is made of, and the one
/// the bag is made of. They are the shipped table's own rows.
const BLACK_OPAL: u32 = 0x10;
const IVORY: u32 = 0x33;
const BRONZE: u32 = 0x3A;
/// The skill the shard's report names.
const SALVAGING_SKILL: u32 = 40;

/// Everything the salvage fixture in this file does not already seed. The player, the tool and
/// the ring are [`super::seed_salvage`]'s.
fn seed_the_rest_of_the_bench(w: &mut dereth_client_model::World) {
    use dereth_rules::weenie::item_type;

    let seeds: [(ObjectId, &str, u32, u16, u32, bool); 5] = [
        (
            BAND,
            "Black Opal Band",
            BLACK_OPAL,
            60,
            item_type::JEWELRY,
            true,
        ),
        (
            PRISTINE,
            "Pristine Black Opal Amulet",
            BLACK_OPAL,
            100,
            item_type::JEWELRY,
            true,
        ),
        (FIGURINE, "Ivory Figurine", IVORY, 30, item_type::MISC, true),
        // The shard names a bag of salvage after what is left in it and puts the material
        // beside the name rather than in it -- which is why the material word shows up on this
        // one and on none of the others, whose names already carry theirs.
        (BAG, "Salvage (100)", BRONZE, 100, item_type::MISC, true),
        // Not the player's: it is in nobody's pack, which is what the ownership test reads.
        (
            NOT_MINE,
            "Stranger's Black Opal Ring",
            BLACK_OPAL,
            20,
            item_type::JEWELRY,
            false,
        ),
    ];
    for (id, name, material, structure, obj_type, mine) in seeds {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = name.into();
        wn.pwd.icon_id = 0x0600_1234;
        wn.pwd.obj_type = obj_type;
        wn.pwd.material_type = Some(material);
        wn.pwd.structure = Some(structure);
        wn.pwd.container_id = mine.then_some(super::SALVAGE_PLAYER);
        w.tables.weenies.insert(id, wn);
        if mine {
            w.tables
                .inventories
                .get_mut(super::SALVAGE_PLAYER)
                .expect("the fixture made the player an inventory")
                .items
                .push(id);
        }
    }
    // The stranger's ring is on a quickbar tile, which is the one place a thing the player
    // does not own is still something the client is drawing. Nothing is dragged off that tile;
    // it is there so the window is asked about a thing that exists.
    w.player_system
        .add_shortcut(dereth_protocol::login::ShortCutData {
            index: 0,
            object_id: NOT_MINE,
            spell_id: 0,
        });
    // The tool on the next tile, for the one scenario whose drag has to be a shortcut.
    w.player_system
        .add_shortcut(dereth_protocol::login::ShortCutData {
            index: 1,
            object_id: super::SALVAGE_TOOL,
            spell_id: 0,
        });
}

/// A client with the whole bench in his pack and the pack open. The salvage window is **not**
/// open: opening it is one of the claims.
fn a_client_with_the_salvage_bench() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    {
        let w = &mut c.objects_mut().world;
        super::seed_salvage(w);
        seed_the_rest_of_the_bench(w);
    }
    open_pack(&mut c);
    c
}

/// The same bench with the tool used, which is what puts the window on screen.
fn a_client_with_the_salvage_window_open() -> HeadlessClient {
    let mut c = a_client_with_the_salvage_bench();
    super::use_the_tool(&mut c);
    assert!(
        the_window_is_up(&mut c),
        "the premise: using the tool opened the window"
    );
    c
}

fn the_window_is_up(c: &mut HeadlessClient) -> bool {
    let root = c.view().expect_app().hud().panels.salvage.root;
    let (ui, _screen) = gameplay_screen(c.app_mut());
    root.is_some_and(|h| ui.is_visible(h))
}

/// The ids the **live** window holds, in slot order. The panel's own record cannot tell a
/// panel that worked the right rows out and drew none from one that drew them.
fn drawn(c: &HeadlessClient) -> Vec<ObjectId> {
    let Some(w) = c.view().expect_app().hud().panels.salvage.list.as_ref() else {
        return Vec::new();
    };
    (0..w.num_ui_items()).filter_map(|i| w.item_at(i)).collect()
}

fn salvage_tile_at(c: &HeadlessClient, i: usize) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .salvage
        .list
        .as_ref()
        .expect("the salvage list is bound")
        .slots
        .get(i)
        .unwrap_or_else(|| panic!("the salvage list has no row {i}"))
        .handle
}

/// The window's own button, as the panel last wrote it and as the element itself carries it.
fn button_state(c: &mut HeadlessClient) -> (SalvageButton, Option<dereth_ui::StateId>) {
    let (mirror, h) = {
        let p = &c.view().expect_app().hud().panels.salvage;
        (p.button, p.salvage_button)
    };
    let (ui, _screen) = gameplay_screen(c.app_mut());
    (mirror, h.and_then(|h| ui.node(h)).map(|n| n.state))
}

/// The mark on a row of the window, as the panel last wrote it and as the element carries it.
/// Two readings, because the panel's own record cannot detect one that decided rightly and
/// drew nothing.
fn hint_at(
    c: &mut HeadlessClient,
    i: usize,
) -> (Option<dereth_ui::StateId>, Option<dereth_ui::StateId>) {
    let (mirror, overlay) = {
        let s = &c
            .view()
            .expect_app()
            .hud()
            .panels
            .salvage
            .list
            .as_ref()
            .expect("bound")
            .slots[i];
        (s.drag_accept_state, s.drag_accept)
    };
    let (ui, _screen) = gameplay_screen(c.app_mut());
    (mirror, overlay.and_then(|h| ui.node(h)).map(|n| n.state))
}

fn point_of(c: &mut HeadlessClient, h: ElemHandle) -> Target {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let (x, y) = centre(ui, h);
    Target::Point(ScreenPoint::new(x, y))
}

/// The pointer really is over the row, so a gesture cannot pass while reaching nothing.
fn the_pointer_catches(c: &mut HeadlessClient, at: Target, want: ElemHandle) {
    let Target::Point(p) = at else {
        panic!("a point")
    };
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let hit = ui
        .hit_test_screen(p.x, p.y)
        .expect("the pointer is over something");
    assert_eq!(
        ui.drag_and_drop_catcher(hit),
        Some(want),
        "the thing under the pointer is the row the gesture is aimed at"
    );
}

/// Pick `item` out of the pack and carry it over the window's `tile`th row, leaving it in the
/// air. Answers where the pointer is.
fn carry_over_the_window(c: &mut HeadlessClient, item: ObjectId, tile: usize) -> Target {
    let from = super::pack_slot(c, item);
    let from = point_of(c, from);
    let to_h = salvage_tile_at(c, tile);
    let to = point_of(c, to_h);
    c.when(Player::Grab(from));
    c.when(Player::Over(to));
    the_pointer_catches(c, to, to_h);
    to
}

/// The whole gesture: out of the pack, over the window, let go.
fn drag_into_the_window(c: &mut HeadlessClient, item: ObjectId, tile: usize) {
    let at = carry_over_the_window(c, item, tile);
    c.when(Player::Drop(at));
}

/// A press on an element with a chosen input action -- the left click is `PRIMARY_CLICK` and
/// the left double click is its own action, and the difference is the subject of one of these.
fn press_with(c: &mut HeadlessClient, h: ElemHandle, action: u32, t: f64) {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let (x, y) = centre(ui, h);
    ui.mouse_move(dereth_primitives::LocalTime(t), x, y);
    ui.mouse_down(action, x, y);
    c.tick(3);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.mouse_up(action, x, y, false);
    c.tick(2);
}

/// One of the window's two buttons, clicked.
fn click_window_button(c: &mut HeadlessClient, id: dereth_ui::ElementId) {
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("a root");
        let h = ui
            .get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"));
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(3);
}

/// Every salvage request the client has built this scenario, as the tool and the list of ids.
fn salvage_requests(c: &HeadlessClient) -> Vec<(ObjectId, Vec<ObjectId>)> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            super::Request::CreateTinkeringTool(m) => Some((m.tool, m.items.clone())),
            _ => None,
        })
        .collect()
}

/// What the handler that has just run wrote on the local scroll.
///
/// It is read with no frame in between, because the client empties the scroll into its chat
/// windows at the head of the **next** batch: one message, then this, is exactly the lines
/// that message produced.
fn wrote_on_the_scroll(c: &HeadlessClient) -> Vec<String> {
    c.view()
        .world()
        .scroll
        .pending()
        .iter()
        .map(|l| l.body.clone())
        .collect()
}

// -----------------------------------------------------------------------------------------

/// Using a tinkering tool opens the window, empty, with the tool remembered and the button
/// dead -- and asks the shard for nothing.
pub fn using_a_tinkering_tool_opens_it_empty_and_remembers_the_tool() {
    let mut c = a_client_with_the_salvage_bench();

    // Before: the window is built and shut, and has never been opened.
    let before = {
        let p = &c.view().expect_app().hud().panels.salvage;
        (p.list.is_some(), p.root.is_some(), p.opens, p.tool)
    };
    let shut_to_start_with = !the_window_is_up(&mut c);
    assert_eq!(
        before,
        (true, true, 0, None),
        "the premise: a window that is built and shut"
    );
    assert_eq!(button_state(&mut c).0, SalvageButton::Disabled);

    super::use_the_tool(&mut c);

    let up = the_window_is_up(&mut c);
    let (button, button_element) = button_state(&mut c);
    let opened = {
        let p = &c.view().expect_app().hud().panels.salvage;
        p.opens == 1 && p.tool == Some(super::SALVAGE_TOOL) && p.material == 0
    };
    let empty = drawn(&c).is_empty();
    // Using a tinkering tool puts nothing on the wire: it opens a window and nothing else.
    let silent = salvage_requests(&c).is_empty();

    c.assert_behaviour(
        "salvage.window.using-a-tinkering-tool-opens-it-empty-and-remembers-the-tool",
        {
            move |_| {
                shut_to_start_with
                    && up
                    && opened
                    && empty
                    && silent
                    && button == SalvageButton::Disabled
                    && button_element == Some(dereth_ui::StateId(0x0D))
            }
        },
    );
    c.shutdown();
}

/// Only a thing the window will take goes in, and the first one decides what the rest must be
/// made of.
pub fn only_a_suitable_thing_goes_in_and_the_first_one_fixes_the_material() {
    let mut c = a_client_with_the_salvage_window_open();

    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    let first_in = drawn(&c) == vec![super::SALVAGE_RING]
        && c.view().expect_app().hud().panels.salvage.material == BLACK_OPAL
        && button_state(&mut c) == (SalvageButton::Enabled, Some(dereth_ui::StateId(0x01)));

    // A second of the same material goes in, and does not move what the window is locked to.
    drag_into_the_window(&mut c, BAND, 1);
    let second_in = drawn(&c) == vec![super::SALVAGE_RING, BAND]
        && c.view().expect_app().hud().panels.salvage.material == BLACK_OPAL;

    // The same thing again: it is already in the list.
    let refused_before = c.view().expect_app().hud().panels.salvage.drops_refused;
    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    let no_second_row = drawn(&c) == vec![super::SALVAGE_RING, BAND]
        && c.view().expect_app().hud().panels.salvage.drops_refused > refused_before;

    // Another material, with the player not having asked for more than one.
    let one_material_only = dereth_client_runtime::hud::character_option(
        c.view().world(),
        dereth_ui_screens::view::PlayerOption::SalvageMultiple,
    )
    .unwrap_or(false);
    assert!(
        !one_material_only,
        "the premise: the player has not asked for two materials"
    );
    drag_into_the_window(&mut c, FIGURINE, 2);
    let locked_to_one_material = drawn(&c) == vec![super::SALVAGE_RING, BAND];

    // Something with nothing wrong with it: there is nothing to salvage out of it.
    drag_into_the_window(&mut c, PRISTINE, 2);
    let whole_things_refused = drawn(&c) == vec![super::SALVAGE_RING, BAND];

    // And something that is not the player's, which is the one test that runs before every
    // other. **This is the one place the pointer half is stood in for**: a thing the player
    // does not own is in somebody else's corpse or chest, so its drag starts in a list this
    // client has not opened. What is entered is the window's own answer, with the client's own
    // reading of the world handed to it, so the ownership test, the refusal and the request
    // dispatch are all the client's.
    let lines_before = super::bubbles(&mut c).len();
    let world = c.view().snapshot();
    let mut panels = std::mem::take(&mut c.hud_mut().panels);
    let accepted = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        panels.salvage.accept_drag_object(ui, NOT_MINE, &world)
    };
    c.hud_mut().panels = panels;
    c.tick(2);
    let said = super::bubbles(&mut c);
    let not_yours = !accepted
        && drawn(&c) == vec![super::SALVAGE_RING, BAND]
        && said[lines_before..] == [salvage::NOT_YOURS.to_owned()];

    // Filling the window asks the shard for nothing at all.
    let silent = salvage_requests(&c).is_empty();

    c.assert_behaviour(
        "salvage.window.only-a-suitable-thing-goes-in-and-the-first-one-fixes-the-material",
        move |_| {
            first_in
                && second_in
                && no_second_row
                && locked_to_one_material
                && whole_things_refused
                && not_yours
                && silent
        },
    );
    c.shutdown();
}

/// A second material goes in only when the player has asked for it, and then the request
/// carries both.
///
/// One claim with two halves: that the second material is let in, and that the request then
/// carries it. Both halves are read here.
pub fn the_second_material_only_goes_in_when_the_player_asked_for_it() {
    // ---- the player has not asked ---------------------------------------------------------
    let mut c = a_client_with_the_salvage_window_open();
    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    drag_into_the_window(&mut c, FIGURINE, 1);
    let one_material = drawn(&c) == vec![super::SALVAGE_RING];
    click_window_button(&mut c, salvage::SALVAGE_BUTTON);
    let one_id = salvage_requests(&c) == vec![(super::SALVAGE_TOOL, vec![super::SALVAGE_RING])];
    c.shutdown();

    // ---- and the same bench with the option set -------------------------------------------
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    {
        let w = &mut c.objects_mut().world;
        super::seed_salvage(w);
        seed_the_rest_of_the_bench(w);
        // The option the player's own description would have carried.
        w.player_system.options.set(
            dereth_client_runtime::hud::option_ordinal(
                dereth_ui_screens::view::PlayerOption::SalvageMultiple,
            ),
            true,
        );
        w.player_system.module = Some(dereth_protocol::login::PlayerModule::default());
    }
    open_pack(&mut c);
    super::use_the_tool(&mut c);
    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    drag_into_the_window(&mut c, FIGURINE, 1);
    let both_materials = drawn(&c) == vec![super::SALVAGE_RING, FIGURINE];
    // The other tests still run: a thing with nothing wrong with it is not an option.
    drag_into_the_window(&mut c, PRISTINE, 2);
    let still_refused = drawn(&c) == vec![super::SALVAGE_RING, FIGURINE];
    click_window_button(&mut c, salvage::SALVAGE_BUTTON);
    // The window is read from the bottom up, so the list that goes out is its order reversed.
    let both_ids =
        salvage_requests(&c) == vec![(super::SALVAGE_TOOL, vec![FIGURINE, super::SALVAGE_RING])];

    c.assert_behaviour(
        "salvage.window.the-second-material-only-goes-in-when-the-player-asked-for-it",
        move |_| one_material && one_id && both_materials && still_refused && both_ids,
    );
    c.shutdown();
}

/// The button sends the tool and every row, empties the window and leaves it open.
pub fn the_button_sends_the_tool_and_every_row_and_empties_the_window() {
    let mut c = a_client_with_the_salvage_window_open();
    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    drag_into_the_window(&mut c, BAND, 1);
    assert!(
        salvage_requests(&c).is_empty(),
        "the premise: nothing has been asked for yet"
    );
    assert_eq!(c.view().expect_app().hud().panels.salvage.salvages, 0);

    click_window_button(&mut c, salvage::SALVAGE_BUTTON);

    // The window is read from the bottom up, so what goes out is its order reversed. That is
    // not untidiness: it is what the shard receives.
    let sent = salvage_requests(&c) == vec![(super::SALVAGE_TOOL, vec![BAND, super::SALVAGE_RING])]
        && c.view().expect_app().hud().panels.salvage.salvages == 1;
    let emptied = drawn(&c).is_empty()
        && c.view().expect_app().hud().panels.salvage.material == 0
        && button_state(&mut c) == (SalvageButton::Disabled, Some(dereth_ui::StateId(0x0D)));
    // The tool is kept, so a second batch with the same tool needs no second use, and the
    // window stays on screen.
    let still_open = c.view().expect_app().hud().panels.salvage.tool == Some(super::SALVAGE_TOOL)
        && the_window_is_up(&mut c);

    // An empty window asks for nothing.
    click_window_button(&mut c, salvage::SALVAGE_BUTTON);
    let nothing_more =
        salvage_requests(&c).len() == 1 && c.view().expect_app().hud().panels.salvage.salvages == 1;

    c.assert_behaviour(
        "salvage.button.the-button-sends-the-tool-and-every-row-and-empties-the-window",
        move |_| sent && emptied && still_open && nothing_more,
    );
    c.shutdown();
}

/// A row comes out on a double click and stays on a single one.
pub fn a_double_click_takes_a_row_out_and_a_single_click_does_not() {
    let mut c = a_client_with_the_salvage_window_open();
    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    drag_into_the_window(&mut c, BAND, 1);
    assert_eq!(
        drawn(&c),
        vec![super::SALVAGE_RING, BAND],
        "the premise: two rows"
    );

    let row = salvage_tile_at(&c, 0);
    press_with(&mut c, row, dereth_ui::focus::action::PRIMARY_CLICK, 5.0);
    let a_click_does_nothing = drawn(&c) == vec![super::SALVAGE_RING, BAND]
        && c.view().expect_app().hud().panels.salvage.items_removed == 0;

    let row = salvage_tile_at(&c, 0);
    press_with(&mut c, row, 0x0A, 7.0);
    let taken_out = drawn(&c) == vec![BAND]
            && c.view().expect_app().hud().panels.salvage.items_removed == 1
            // The window stays locked to the material until the last row goes.
            && c.view().expect_app().hud().panels.salvage.material == BLACK_OPAL
            && button_state(&mut c).0 == SalvageButton::Enabled;

    // And the last row out empties the window, which is when the material lock lifts.
    let row = salvage_tile_at(&c, 0);
    press_with(&mut c, row, 0x0A, 9.0);
    let emptied = drawn(&c).is_empty()
        && c.view().expect_app().hud().panels.salvage.material == 0
        && button_state(&mut c) == (SalvageButton::Disabled, Some(dereth_ui::StateId(0x0D)));
    let silent = salvage_requests(&c).is_empty();

    c.assert_behaviour(
        "salvage.row.a-double-click-takes-a-row-out-and-a-single-click-does-not",
        move |_| a_click_does_nothing && taken_out && emptied && silent,
    );
    c.shutdown();
}

/// Closing the window empties it and forgets the tool, and does it once.
pub fn closing_it_empties_it_and_forgets_the_tool() {
    let mut c = a_client_with_the_salvage_window_open();
    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    assert_eq!(
        drawn(&c),
        vec![super::SALVAGE_RING],
        "the premise: a row to lose"
    );
    assert_eq!(c.view().expect_app().hud().panels.salvage.closes, 0);

    click_window_button(&mut c, salvage::CLOSE_BUTTON);

    let gone = !the_window_is_up(&mut c);
    let forgotten = {
        let p = &c.view().expect_app().hud().panels.salvage;
        p.closes == 1 && p.tool.is_none() && p.material == 0
    };
    let emptied = drawn(&c).is_empty()
        && button_state(&mut c) == (SalvageButton::Disabled, Some(dereth_ui::StateId(0x0D)));
    let silent = salvage_requests(&c).is_empty();

    // Closing is the client's own business, and the edge is taken once rather than once a
    // frame.
    c.tick(4);
    let once = c.view().expect_app().hud().panels.salvage.closes == 1;

    c.assert_behaviour(
        "salvage.window.closing-it-empties-it-and-forgets-the-tool",
        move |_| gone && forgotten && emptied && silent && once,
    );
    c.shutdown();
}

/// The shard's report is read out on the scroll, in the client's own words.
pub fn the_shards_report_is_read_out_on_the_scroll() {
    use dereth_protocol::items::{SalvageResult, SalvageResultMessage};

    let result = |skill: u32, rows: &[(u32, f64, i32)], spoiled: &[ObjectId], bonus: i32| {
        SalvageResultMessage {
            skill_used: skill,
            not_salvagable: spoiled.to_vec(),
            results: rows
                .iter()
                .map(|(material, workmanship, units)| SalvageResult {
                    material: *material,
                    workmanship: *workmanship,
                    units: *units,
                })
                .collect(),
            aug_bonus: bonus,
        }
    };

    let mut c = a_client_with_the_salvage_bench();
    assert_eq!(
        c.view().hud().stats.salvage_results,
        0,
        "the premise: nothing has arrived"
    );

    // Two materials and a bonus: one line, joined the way a person would join them, with the
    // skill and the bonus.
    c.when(Inbound::message(&result(
        SALVAGING_SKILL,
        &[(BLACK_OPAL, 5.0, 3), (IVORY, 4.5, 2)],
        &[],
        50,
    )));
    let two_and_a_bonus = wrote_on_the_scroll(&c)
        == vec![concat!(
            "You obtain 3 Black Opal (ws 5.00) and 2 Ivory (ws 4.50) using your ",
            "knowledge of Salvaging. Your augmentation has given you a return bonus ",
            "of 50%!",
        )
        .to_owned()];

    // Three take a comma for the middle one, and no bonus means no bonus clause at all.
    c.when(Inbound::message(&result(
        SALVAGING_SKILL,
        &[(BLACK_OPAL, 5.0, 3), (IVORY, 4.5, 2), (BLACK_OPAL, 1.0, 1)],
        &[],
        0,
    )));
    let lines = wrote_on_the_scroll(&c);
    let three_in_a_row = lines.len() == 1
        && lines[0].starts_with(
            "You obtain 3 Black Opal (ws 5.00), 2 Ivory (ws 4.50) and 1 Black Opal (ws 1.00) ",
        )
        && !lines[0].contains("augmentation");

    // What could not be salvaged is a line of its own, and the things are named as the player
    // knows them.
    c.when(Inbound::message(&result(
        SALVAGING_SKILL,
        &[(BLACK_OPAL, 2.0, 1)],
        &[super::SALVAGE_RING, BAND],
        0,
    )));
    let lines = wrote_on_the_scroll(&c);
    let spoiled_on_its_own = lines.len() == 2
        && lines[1]
            == concat!(
                "The following were not suitable for salvaging: Black Opal Ring, ",
                "Black Opal Band.",
            );

    // Nothing either way is its own answer and not a leftover of the two above.
    c.when(Inbound::message(&result(SALVAGING_SKILL, &[], &[], 0)));
    let nothing_at_all = wrote_on_the_scroll(&c) == vec!["Salvaging Failed!".to_owned()];

    // A thing the client has never heard of contributes nothing, and with nothing left to say
    // the client says nothing -- the failure line needs **both** lists to be empty.
    c.when(Inbound::message(&result(
        SALVAGING_SKILL,
        &[],
        &[ObjectId(0xDEAD_BEEF)],
        0,
    )));
    let unknown_says_nothing = wrote_on_the_scroll(&c).is_empty();

    // A material the shipped table does not name is named as unknown.
    c.when(Inbound::message(&result(
        SALVAGING_SKILL,
        &[(0x7F, 2.0, 7)],
        &[],
        0,
    )));
    let lines = wrote_on_the_scroll(&c);
    let unnamed_material =
        lines.len() == 1 && lines[0].starts_with("You obtain 7 Unknown (ws 2.00) ");

    // A player who has silenced everything is not told, and it is counted rather than lost.
    c.objects_mut()
        .world
        .chat
        .squelch
        .global
        .squelch_everything();
    let lines_before = c.view().hud().stats.salvage_lines;
    c.when(Inbound::message(&result(
        SALVAGING_SKILL,
        &[(BLACK_OPAL, 5.0, 3)],
        &[],
        0,
    )));
    let silenced = wrote_on_the_scroll(&c).is_empty()
        && c.view().hud().stats.salvage_lines == lines_before
        && c.view().hud().stats.salvage_results_squelched == 1
        && c.view().hud().stats.salvage_results == 7;

    c.assert_behaviour(
        "salvage.report.the-shards-report-is-read-out-on-the-scroll",
        move |_| {
            two_and_a_bonus
                && three_in_a_row
                && spoiled_on_its_own
                && nothing_at_all
                && unknown_says_nothing
                && unnamed_material
                && silenced
        },
    );
    c.shutdown();
}

/// A thing carried over the window is marked as welcome or as refused, before it is let go.
pub fn a_thing_carried_over_it_is_marked_as_welcome_or_as_refused() {
    use dereth_ui_screens::items::widget::drag_accept_state;

    let mut c = a_client_with_the_salvage_window_open();
    let nothing_yet = hint_at(&mut c, 0).0.is_none();

    let at = carry_over_the_window(&mut c, super::SALVAGE_RING, 0);
    let welcome = hint_at(&mut c, 0)
        == (
            Some(drag_accept_state::ACCEPT),
            Some(drag_accept_state::ACCEPT),
        );
    c.when(Player::Drop(at));
    let it_went_in = drawn(&c) == vec![super::SALVAGE_RING];
    assert_eq!(
        c.view().expect_app().hud().panels.salvage.material,
        BLACK_OPAL,
        "the premise: the window is now locked to the ring's material"
    );

    let at = carry_over_the_window(&mut c, FIGURINE, 1);
    let refused = hint_at(&mut c, 1)
        == (
            Some(drag_accept_state::REFUSE),
            Some(drag_accept_state::REFUSE),
        );
    c.when(Player::Drop(at));
    let and_the_drop_agreed = drawn(&c) == vec![super::SALVAGE_RING];

    c.assert_behaviour(
        "salvage.window.a-thing-carried-over-it-is-marked-as-welcome-or-as-refused",
        move |_| nothing_yet && welcome && it_went_in && refused && and_the_drop_agreed,
    );
    c.shutdown();
}

/// The mark comes off the row the moment the drop lands on it.
pub fn the_mark_comes_off_the_tile_as_soon_as_the_drop_lands() {
    use dereth_ui_screens::items::widget::drag_accept_state;

    let mut c = a_client_with_the_salvage_window_open();
    let at = carry_over_the_window(&mut c, super::SALVAGE_RING, 0);
    let up_first = hint_at(&mut c, 0).0 == Some(drag_accept_state::ACCEPT);
    c.when(Player::Drop(at));
    let down_again =
        hint_at(&mut c, 0) == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));
    let and_it_was_taken = drawn(&c) == vec![super::SALVAGE_RING];

    c.assert_behaviour(
        "salvage.window.the-mark-comes-off-the-tile-as-soon-as-the-drop-lands",
        move |_| up_first && down_again && and_it_was_taken,
    );
    c.shutdown();
}

/// A shortcut carried over the window is marked neither way, and leaves the row as it found it.
///
/// The scenario is arranged so that it can fail in both directions: a row that has never been
/// marked answers nothing whether or not the window is right, so a real drop is made first and
/// the claim is that the shortcut drag leaves the row **exactly** where the drop left it.
pub fn a_shortcut_carried_over_it_is_marked_neither_way() {
    use dereth_ui_screens::items::widget::drag_accept_state;

    let mut c = a_client_with_the_salvage_window_open();
    drag_into_the_window(&mut c, super::SALVAGE_RING, 0);
    let before = hint_at(&mut c, 0);
    assert_eq!(
        before,
        (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE)),
        "the premise: the drop left the row's mark cleared"
    );

    let from = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .shortcuts
            .slots
            .get(1)
            .expect("the shipped quickbar has a second tile")
            .handle
    };
    let from = point_of(&mut c, from);
    let to_h = salvage_tile_at(&c, 0);
    let to = point_of(&mut c, to_h);
    c.when(Player::Grab(from));
    c.when(Player::Over(to));
    the_pointer_catches(&mut c, to, to_h);
    let untouched = hint_at(&mut c, 0) == before;
    c.when(Player::Drop(to));

    c.assert_behaviour(
        "salvage.window.a-shortcut-carried-over-it-is-marked-neither-way",
        move |_| untouched,
    );
    c.shutdown();
}

/// A bag of salvage says what it is made of, wherever its name is drawn -- and the things
/// whose names already carry their material are left alone.
pub fn a_bag_of_salvage_says_what_it_is_made_of() {
    let mut c = a_client_with_the_salvage_bench();
    c.tick(2);

    // The instrument can look: the shipped table has the row this depends on.
    let named = dereth_client_runtime::hud::material_name_of(
        c.view().expect_app().hud().material_names.as_ref(),
        BRONZE,
    );
    assert_eq!(
        named.as_deref(),
        Some("Bronze"),
        "the premise: the shipped table names it"
    );
    assert!(
        c.view().expect_app().hud().stats.display_names_composed >= 1,
        "the premise: the pack composed at least one name"
    );

    let tooltip = |c: &mut HeadlessClient, item: ObjectId| -> String {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid")
            .slots
            .iter()
            .find(|s| s.item == Some(item))
            .unwrap_or_else(|| panic!("{item:?} has no pack tile"))
            .tooltip
            .clone()
            .unwrap_or_default()
    };
    let on_the_tile = tooltip(&mut c, BAG) == "Bronze Salvage (100)";
    // And it is a no-op where the shard already spelled the material into the name, which is
    // what makes it safe for every other thing in the game.
    let others_untouched = tooltip(&mut c, super::SALVAGE_RING) == "Black Opal Ring"
        && tooltip(&mut c, FIGURINE) == "Ivory Figurine"
        && tooltip(&mut c, super::SALVAGE_TOOL) == "Iron Salvaging Kit";

    // And in the window that tells the player what a thing is.
    {
        let mut sink = super::RecordingSink::default();
        let profile = dereth_protocol::types::AppraisalProfile::default();
        c.objects_mut()
            .world
            .set_appraise_info(BAG, profile, &mut sink);
        assert!(
            sink.0.iter().any(
                |n| matches!(n, dereth_client_model::Notice::AppraisalReady(id) if *id == BAG)
            ),
            "the premise: the client has something to say about the bag"
        );
    }
    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.examination.examine_object(BAG);
    }
    c.tick(2);
    let in_the_window = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.examination.title_text.as_deref() == Some("Bronze Salvage (100)")
            && screen.examination.replies_applied == 1
    };

    c.assert_behaviour(
        "salvage.bag.a-bag-of-salvage-says-what-it-is-made-of",
        move |_| on_the_tile && others_untouched && in_the_window,
    );
    c.shutdown();
}

// =========================================================================================
// Choosing how many to take.
// =========================================================================================

use dereth_ui::ElementId;
use dereth_ui_screens::toolbar::splitter::{ENTRY_BOX, SLIDER};

/// The strip the split controls live in, which is shown for anything selected at all.
const SEL_OBJECT_FIELD: ElementId = ElementId(0x1000_019E);
/// The chat entry, which is the nearest thing a player can put the caret in instead.
const CHAT_ENTRY: ElementId = dereth_ui_screens::chat::window::ENTRY;

fn element(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root().expect("a root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

/// Whether the element itself is drawn. The strip and its two children move independently,
/// which is what the three readings below are for.
fn is_shown(c: &mut HeadlessClient, id: ElementId) -> bool {
    let h = element(c, id);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.node(h)
        .expect("the element is alive")
        .region
        .flags
        .visible
}

/// The strip, the quantity box and the slider, so one reading names all three.
fn split_controls(c: &mut HeadlessClient) -> (bool, bool, bool) {
    (
        is_shown(c, SEL_OBJECT_FIELD),
        is_shown(c, ENTRY_BOX),
        is_shown(c, SLIDER),
    )
}

/// What the quantity box is showing.
fn box_text(c: &mut HeadlessClient) -> String {
    let h = element(c, ENTRY_BOX);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.text_element_mut(h).map_or_else(String::new, |t| {
        String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
    })
}

/// Where the slider's thumb is sitting, from nought to one.
fn slider_position(c: &mut HeadlessClient) -> Option<f32> {
    let h = element(c, SLIDER);
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.node(h)
        .expect("alive")
        .merged_properties()
        .get_float(dereth_ui::widgets::scrollbar::attr::POSITION)
}

/// The screen's own copy of the quantity, and the one the drop reads. In the client they are
/// one pair of numbers; here they are two, and the only thing that carries a value between
/// them is the notice the controls raise -- so both are read, every time.
fn both_copies(c: &mut HeadlessClient) -> (u32, (u32, u32)) {
    let far = c.view().world().split;
    let (_ui, screen) = gameplay_screen(c.app_mut());
    (
        screen.splitter.split_size,
        (far.split_size, far.max_split_size),
    )
}

/// A point on an element the pointer can really reach. Some of the toolbar's boxes overlap the
/// slider, so the middle of a box is not always a pixel of it.
fn exposed_point(c: &mut HeadlessClient, h: ElemHandle) -> Target {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let r = ui.screen_clip_box(h);
    assert!(
        r.is_valid(),
        "{h:?} is clipped away entirely, so nothing could land on it"
    );
    let middle = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let (x, y) = std::iter::once(middle)
        .chain(
            (r.y0..r.y1)
                .step_by(2)
                .flat_map(|y| (r.x0..r.x1).step_by(2).map(move |x| (x, y))),
        )
        .find(|&(x, y)| {
            ui.hit_test_screen(x, y)
                .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit))
        })
        .unwrap_or_else(|| panic!("no pixel of {h:?} is exposed to the pointer"));
    Target::Point(ScreenPoint::new(x, y))
}

fn point_of_element(c: &mut HeadlessClient, id: ElementId) -> Target {
    let h = element(c, id);
    exposed_point(c, h)
}

/// Pick something, the way the client's own selection does.
fn select(c: &mut HeadlessClient, id: ObjectId) {
    c.when(Player::ui(dereth_ui_screens::view::UiRequest::Select(id)));
    c.tick(3);
}

/// Put the caret in the quantity box and type, which alone commits nothing.
fn type_a_quantity(c: &mut HeadlessClient, text: &str) {
    let h = element(c, ENTRY_BOX);
    let at = exposed_point(c, h);
    c.when(Player::Focus(at));
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        assert_eq!(
            ui.focus_element(),
            Some(h),
            "the click put the caret in the quantity box"
        );
    }
    assert!(
        c.app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .text
            .text_mode,
        "the caret going in turned typing on, so the characters below are not dropped"
    );
    c.when(Player::Type(text.to_owned()));
}

/// Put the caret somewhere else, which is what makes the box hand its number over.
fn commit_by_leaving_the_box(c: &mut HeadlessClient) {
    let at = point_of_element(c, CHAT_ENTRY);
    c.when(Player::Click(at));
}

/// The tile the pack grid is drawing `item` on.
fn grid_tile(c: &mut HeadlessClient, item: ObjectId) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the pack grid")
        .slots
        .iter()
        .find(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("{item:?} has no pack tile"))
        .handle
}

/// The `n`th tile of the pack grid.
fn grid_tile_at(c: &mut HeadlessClient, n: usize) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the pack grid")
        .slots
        .get(n)
        .unwrap_or_else(|| panic!("the pack grid has no tile {n}"))
        .handle
}

/// The tile the strip of side packs is drawing `pack` on.
fn side_pack_tile(c: &mut HeadlessClient, pack: ObjectId) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    let inv = &screen.inventory;
    inv.top_container
        .iter()
        .chain(inv.container_list.iter())
        .find_map(|w| {
            w.slots
                .iter()
                .find(|s| s.item == Some(pack))
                .map(|s| s.handle)
        })
        .unwrap_or_else(|| panic!("{pack:?} has no row on the strip of side packs"))
}

/// The first tile of the pack grid with nothing on it.
fn an_empty_grid_tile(c: &mut HeadlessClient) -> ElemHandle {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the pack grid")
        .slots
        .iter()
        .find(|s| s.item.is_none())
        .expect("the pack has an empty tile")
        .handle
}

/// Drag `item`'s tile onto `to` and let go -- the gesture that sends the split.
fn drag_the_stack(c: &mut HeadlessClient, item: ObjectId, to: ElemHandle) {
    let from = grid_tile(c, item);
    let from = exposed_point(c, from);
    let to = exposed_point(c, to);
    c.when(Player::Drag {
        from,
        to,
        hold_frames: 1,
    });
}

/// Every split and every whole-stack move the client has asked for this scenario.
fn stack_requests(c: &HeadlessClient) -> Vec<super::Request> {
    c.view()
        .outbound()
        .iter()
        .filter(|r| {
            matches!(
                r,
                super::Request::StackableSplitToContainer(_)
                    | super::Request::PutItemInContainer(_)
            )
        })
        .cloned()
        .collect()
}

// -----------------------------------------------------------------------------------------
// o117: which selections get the split controls at all.
// -----------------------------------------------------------------------------------------

/// The recording every split scenario stands on.
const SPLIT_SESSION: &str = "long-solo-play";

/// Three things the recording carries, and the point it has to be replayed to for all three to
/// be there: a real stack, a creature, and the thing that tells the client's rule apart from
/// every plausible wrong one -- something that stacks by kind and is on its own.
struct Selectable {
    stack: ObjectId,
    how_many: u32,
    creature: ObjectId,
    single: ObjectId,
    cut: usize,
}

/// Found in the recording rather than written down: a number copied by hand goes stale the day
/// the recording is promoted again and says nothing when it does.
fn things_to_select() -> Selectable {
    let c = corpus();
    let mut stack: Option<(ObjectId, u32, usize)> = None;
    let mut creature: Option<(ObjectId, usize)> = None;
    let mut single: Option<(ObjectId, usize)> = None;
    for b in c
        .blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient && b.opcode == 0xF745 && b.payload.len() > 8)
    {
        let Ok(ItemCreateObject(p)) =
            ItemCreateObject::read(&mut dereth_protocol::Reader::new(&b.payload[4..]))
        else {
            continue;
        };
        let w = &p.wdesc;
        let n = u32::from(w.stack_size.unwrap_or(0));
        let most = u32::from(w.max_stack_size.unwrap_or(0));
        if stack.is_none() && n >= 4 {
            stack = Some((p.id, n, b.idx));
        }
        if creature.is_none()
            && w.obj_type & dereth_client_runtime::hud::ITEM_TYPE_CREATURE != 0
            && w.stack_size.is_none()
        {
            creature = Some((p.id, b.idx));
        }
        if single.is_none() && most > 1 && n <= 1 {
            single = Some((p.id, b.idx));
        }
    }
    let (stack, how_many, a) = stack.expect("the recording carries a stack to select");
    let (creature, b) = creature.expect("the recording carries a creature to select");
    let (single, d) =
        single.expect("the recording carries something that stacks by kind and is on its own");
    Selectable {
        stack,
        how_many,
        creature,
        single,
        cut: a.max(b).max(d) + 1,
    }
}

/// The recording replayed to the moment all three are in the world, with the pack open.
fn a_client_looking_at_the_recorded_world() -> (HeadlessClient, Selectable) {
    let s = things_to_select();
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::corpus_until(SPLIT_SESSION, s.cut));
    c.tick(2);
    open_pack(&mut c);
    c.tick(2);
    {
        let w = c.view().world();
        let carries = |id: ObjectId| w.weenie(id).is_some_and(|x| x.valid);
        assert!(
            carries(s.stack),
            "the premise: the recorded stack is in the world"
        );
        assert!(
            carries(s.creature),
            "the premise: the recorded creature is in the world"
        );
        assert!(
            carries(s.single),
            "the premise: the recorded single thing is in the world"
        );
        assert_eq!(
            w.weenie(s.creature).expect("just checked").pwd.stack_size,
            None,
            "the premise: the creature carries no count at all"
        );
        let single = w.weenie(s.single).expect("just checked");
        assert!(
            single.pwd.max_stack_size.unwrap_or(0) > 1
                && u32::from(single.pwd.stack_size.unwrap_or(0)) <= 1,
            "the premise: it stacks by kind and there is one of it"
        );
    }
    (c, s)
}

/// The split controls are shown for a stack and for nothing else -- not for a creature, not
/// for a thing that merely stacks by kind, and not before anything is picked at all.
///
/// The shipped layout's own authored visibility is a fact about the data rather than about the
/// client and is not asserted; what is behaviour is the last clause here.
pub fn the_split_controls_are_shown_for_a_stack_and_for_nothing_else() {
    let (mut c, s) = a_client_looking_at_the_recorded_world();

    // Nothing picked: the strip is up because the client never hides it, and the two split
    // controls are not.
    let before = split_controls(&mut c);

    select(&mut c, s.creature);
    let a_creature = split_controls(&mut c) == (true, false, false) && both_copies(&mut c).0 == 1;

    select(&mut c, s.stack);
    let a_stack = split_controls(&mut c) == (true, true, true)
        && both_copies(&mut c) == (s.how_many, (s.how_many, s.how_many))
        && box_text(&mut c) == s.how_many.to_string()
        && slider_position(&mut c) == Some(1.0);

    // The case that tells the client's rule apart from every plausible wrong one.
    select(&mut c, s.single);
    let stacks_by_kind_only = split_controls(&mut c) == (true, false, false);

    // And back to the stack, so none of the above is a one-way latch.
    select(&mut c, s.stack);
    let and_back = split_controls(&mut c) == (true, true, true);

    // Nothing picked at all takes the two controls down and leaves the strip where it was.
    {
        let w = &mut c.objects_mut().world;
        w.set_selected_object(None, false, &mut dereth_client_model::NullSink);
    }
    c.tick(3);
    let nothing_picked = split_controls(&mut c) == (true, false, false);

    c.assert_behaviour(
        "inventory.split.the-split-controls-are-shown-for-a-stack-and-for-nothing-else",
        move |_| {
            before == (true, false, false)
                && a_creature
                && a_stack
                && stacks_by_kind_only
                && and_back
                && nothing_picked
        },
    );
    c.shutdown();
}

/// Picking a stack opens the quantity at the whole of that stack and sets the ceiling the box
/// clamps to, which is what makes the controls it just revealed work.
///
/// The clamp is the assertion that can only pass because the selection set the ceiling: on a
/// client that left it at one, a number above the stack comes back as one.
pub fn picking_a_stack_sets_the_quantity_to_that_stacks_own_count() {
    let (mut c, s) = a_client_looking_at_the_recorded_world();
    assert!(
        s.how_many >= 4,
        "the premise: the recorded stack has room for the halves below"
    );
    select(&mut c, s.stack);

    let opened_at_the_whole_stack = both_copies(&mut c) == (s.how_many, (s.how_many, s.how_many))
        && box_text(&mut c) == s.how_many.to_string();

    // A number inside the stack survives, and reaches the far end the drop reads.
    let inside = (s.how_many / 4).max(2);
    type_a_quantity(&mut c, &inside.to_string());
    commit_by_leaving_the_box(&mut c);
    let inside_survives = both_copies(&mut c) == (inside, (inside, s.how_many))
        && box_text(&mut c) == inside.to_string();

    // And one above the stack comes back as the stack -- the ceiling is the recording's own
    // count and not the client's default of one.
    type_a_quantity(&mut c, &(s.how_many + 5000).to_string());
    commit_by_leaving_the_box(&mut c);
    let clamped = both_copies(&mut c) == (s.how_many, (s.how_many, s.how_many))
        && box_text(&mut c) == s.how_many.to_string();

    c.assert_behaviour(
        "inventory.split.picking-a-stack-sets-the-quantity-to-that-stacks-own-count",
        move |_| opened_at_the_whole_stack && inside_survives && clamped,
    );
    c.shutdown();
}

/// The quantity the player chose is not taken back by the frames that follow it.
///
/// What a player can see is the quantity itself, and that is what is asserted here, rather
/// than a counter on the client saying the block had settled.
pub fn the_quantity_the_player_chose_is_not_taken_back_by_the_frames_after_it() {
    let (mut c, s) = a_client_looking_at_the_recorded_world();
    select(&mut c, s.stack);
    let opened = both_copies(&mut c) == (s.how_many, (s.how_many, s.how_many));

    c.tick(30);
    let settled = both_copies(&mut c) == (s.how_many, (s.how_many, s.how_many))
        && box_text(&mut c) == s.how_many.to_string();

    // A quantity of the player's own, and then more frames: it stays his.
    type_a_quantity(&mut c, "2");
    commit_by_leaving_the_box(&mut c);
    let chosen = both_copies(&mut c);
    assert_eq!(
        chosen,
        (2, (2, s.how_many)),
        "the premise: the player chose two"
    );
    c.tick(30);
    let kept = both_copies(&mut c) == chosen && box_text(&mut c) == "2";

    // Picking something else does set it again, in both directions.
    select(&mut c, s.creature);
    let armed = split_controls(&mut c) == (true, false, false);
    select(&mut c, s.stack);
    let armed_again = split_controls(&mut c) == (true, true, true)
        && both_copies(&mut c) == (s.how_many, (s.how_many, s.how_many));

    // Putting the selection down takes the controls away and leaves the quantity where the
    // last stack left it, which is odd and is what the client does.
    {
        let w = &mut c.objects_mut().world;
        w.set_selected_object(None, false, &mut dereth_client_model::NullSink);
    }
    c.tick(3);
    let put_down =
        split_controls(&mut c) == (true, false, false) && both_copies(&mut c).1 .1 == s.how_many;
    c.tick(10);
    let still_still = both_copies(&mut c).1 .1 == s.how_many;

    c.assert_behaviour(
        "inventory.split.the-quantity-the-player-chose-is-not-taken-back-by-the-frames-after-it",
        move |_| opened && settled && kept && armed && armed_again && put_down && still_still,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// o373: the two controls that choose the quantity.
// -----------------------------------------------------------------------------------------

const SPLIT_PLAYER: ObjectId = ObjectId(0x5000_0021);
const SPLIT_PACK: ObjectId = ObjectId(0x5000_0022);
const SPLIT_STACK: ObjectId = ObjectId(0x8000_0100);
const OTHER_STACK: ObjectId = ObjectId(0x8000_0200);
/// How many are in the seeded stack, and in the second one.
const WHOLE: u32 = 20;
const OTHER_WHOLE: u32 = 30;

/// A player with a side pack and one stack of twenty in his own pack.
fn seed_a_stack(w: &mut dereth_client_model::World) {
    w.player = Some(SPLIT_PLAYER);
    let mut me = dereth_client_model::Weenie::new(SPLIT_PLAYER);
    me.pwd.bitfield = dereth_rules::weenie::bitfield::PLAYER;
    me.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
    me.pwd.items_capacity = Some(102);
    me.pwd.containers_capacity = Some(7);
    me.valid = true;
    w.tables.weenies.insert(SPLIT_PLAYER, me);
    w.tables.inventories.insert(
        SPLIT_PLAYER,
        dereth_client_model::objects::ObjectInventory::new(SPLIT_PLAYER),
    );

    let mut pack = dereth_client_model::Weenie::new(SPLIT_PACK);
    pack.pwd.name = "Backpack".into();
    pack.pwd.obj_type = dereth_rules::weenie::item_type::CONTAINER;
    pack.pwd.bitfield = dereth_rules::weenie::bitfield::OPENABLE;
    pack.pwd.items_capacity = Some(24);
    pack.pwd.container_id = Some(SPLIT_PLAYER);
    pack.valid = true;
    w.tables.weenies.insert(SPLIT_PACK, pack);
    w.tables.inventories.insert(
        SPLIT_PACK,
        dereth_client_model::objects::ObjectInventory::new(SPLIT_PACK),
    );

    let mut stack = dereth_client_model::Weenie::new(SPLIT_STACK);
    stack.pwd = super::PublicWeenieDesc {
        name: "Pyreal".into(),
        wcid: 0x0111,
        stack_size: Some(u16::try_from(WHOLE).expect("small")),
        max_stack_size: Some(25_000),
        value: Some(WHOLE),
        container_id: Some(SPLIT_PLAYER),
        ..super::PublicWeenieDesc::default()
    };
    stack.valid = true;
    w.tables.weenies.insert(SPLIT_STACK, stack);

    let inventory = w
        .tables
        .inventories
        .get_mut(SPLIT_PLAYER)
        .expect("the player's own pack");
    inventory.add_content(SPLIT_STACK, false, 0);
    inventory.add_content(SPLIT_PACK, true, 0);
}

/// A second stack, of thirty, beside the first.
fn seed_a_second_stack(w: &mut dereth_client_model::World) {
    let mut s = dereth_client_model::Weenie::new(OTHER_STACK);
    s.pwd = super::PublicWeenieDesc {
        name: "Pyreal".into(),
        wcid: 0x0111,
        stack_size: Some(u16::try_from(OTHER_WHOLE).expect("small")),
        max_stack_size: Some(25_000),
        container_id: Some(SPLIT_PLAYER),
        ..super::PublicWeenieDesc::default()
    };
    s.valid = true;
    w.tables.weenies.insert(OTHER_STACK, s);
    w.tables
        .inventories
        .get_mut(SPLIT_PLAYER)
        .expect("the player's own pack")
        .add_content(OTHER_STACK, false, 1);
}

/// A client with that stack in an open pack, and nothing picked yet.
fn a_client_with_a_stack_of_twenty() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    seed_a_stack(&mut c.objects_mut().world);
    open_pack(&mut c);
    c.tick(3);
    c
}

/// The split key puts the caret in the quantity box with the whole quantity picked out, and
/// does nothing at all for a selection there is nothing to split about.
pub fn the_split_key_puts_the_caret_in_the_quantity_and_does_nothing_else() {
    let mut c = a_client_with_a_stack_of_twenty();
    select(&mut c, SPLIT_STACK);
    let box_h = element(&mut c, ENTRY_BOX);
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        assert_eq!(
            ui.focus_element(),
            None,
            "the premise: the key and not the setting-up must put the caret there"
        );
    }
    assert_eq!(
        box_text(&mut c),
        WHOLE.to_string(),
        "the premise: picking seeded the box"
    );

    let expired_before = c.app_mut().actions.stats().expired;
    press_the_split_key(&mut c);
    let caret_in_the_box = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.focus_element() == Some(box_h)
    };
    let consumed = c.app_mut().actions.stats().expired == expired_before;

    // The whole quantity was picked out, so one digit replaces it rather than joining it.
    c.when(Player::Type("7".to_owned()));
    let replaced = box_text(&mut c) == "7";

    // And the three selections there is nothing to split about: nothing is focused and the
    // key is still consumed rather than left to expire.
    let mut a_no_op = |c: &mut HeadlessClient| -> bool {
        commit_by_leaving_the_box(c);
        let before = c.app_mut().actions.stats().expired;
        press_the_split_key(c);
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let unfocused = ui.focus_element() != Some(box_h);
        let still_consumed = c.app_mut().actions.stats().expired == before;
        unfocused && still_consumed
    };

    {
        let w = &mut c.objects_mut().world;
        w.set_selected_object(None, false, &mut dereth_client_model::NullSink);
    }
    c.tick(3);
    let nothing_picked = a_no_op(&mut c);

    {
        let w = &mut c.objects_mut().world;
        w.weenie_mut(SPLIT_STACK).expect("the stack").pwd.stack_size = Some(1);
    }
    select(&mut c, SPLIT_STACK);
    let one_of_them = a_no_op(&mut c);

    {
        let w = &mut c.objects_mut().world;
        let o = w.weenie_mut(SPLIT_STACK).expect("the stack");
        o.pwd.stack_size = None;
        o.pwd.max_stack_size = None;
    }
    select(&mut c, SPLIT_STACK);
    let not_a_stack_at_all = a_no_op(&mut c);

    c.assert_behaviour(
        "inventory.split.the-split-key-puts-the-caret-in-the-quantity-and-does-nothing-else",
        move |_| {
            caret_in_the_box
                && consumed
                && replaced
                && nothing_picked
                && one_of_them
                && not_a_stack_at_all
        },
    );
    c.shutdown();
}

/// The key the shipped keymap binds to "split the stack I have picked".
fn press_the_split_key(c: &mut HeadlessClient) {
    let key = dereth_desktop::platform::window::key_from_key_code(winit::keyboard::KeyCode::KeyT)
        .expect("the host names this key");
    dereth_testkit::input_steps::press_bound(
        c,
        dereth_input::ActionId(SPLIT_STACK_ACTION),
        ITEM_SELECTION_COMMANDS,
        key,
    );
}

/// "Split the stack I have picked" -- the client's own action number, on the input map the
/// selection commands live on.
const SPLIT_STACK_ACTION: u32 = 0x1000_002D;
const ITEM_SELECTION_COMMANDS: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_0007);

/// Dragging the slider chooses the quantity on the drag itself, with no second gesture, and
/// the drop that follows carries it.
pub fn the_slider_chooses_the_quantity_on_the_drag_itself() {
    let mut c = a_client_with_a_stack_of_twenty();
    select(&mut c, SPLIT_STACK);
    assert_eq!(
        both_copies(&mut c),
        (WHOLE, (WHOLE, WHOLE)),
        "the premise: it opens at all"
    );

    let (x0, y) = {
        let h = element(&mut c, SLIDER);
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        (b.x0, (b.y0 + b.y1) / 2)
    };
    // Forty-four pixels along the shipped bar, which is where ten of twenty is.
    c.when(Player::Drag {
        from: Target::Point(ScreenPoint::new(x0, y)),
        to: Target::Point(ScreenPoint::new(x0 + 44, y)),
        hold_frames: 1,
    });
    let the_drag_alone = both_copies(&mut c) == (10, (10, WHOLE)) && box_text(&mut c) == "10";

    let to = an_empty_grid_tile(&mut c);
    drag_the_stack(&mut c, SPLIT_STACK, to);
    let and_the_drop_carried_it = matches!(
        stack_requests(&c).as_slice(),
        [super::Request::StackableSplitToContainer(m)] if m.stack == SPLIT_STACK && m.amount == 10
    );

    c.assert_behaviour(
        "inventory.split.the-slider-chooses-the-quantity-on-the-drag-itself",
        move |_| the_drag_alone && and_the_drop_carried_it,
    );
    c.shutdown();
}

/// A typed quantity is taken when the caret leaves the box and not before, and pressing a tile
/// of the pack is not leaving the box.
pub fn a_typed_quantity_is_taken_when_the_caret_leaves_the_box() {
    let mut c = a_client_with_a_stack_of_twenty();
    select(&mut c, SPLIT_STACK);

    type_a_quantity(&mut c, "5");
    let in_the_box = box_text(&mut c) == "5";
    // Typing alone commits nothing, here and in the client being rebuilt: the box hands its
    // number over on the way out and not on every keystroke.
    let not_yet = both_copies(&mut c) == (WHOLE, (WHOLE, WHOLE));

    // A tile of the pack is not something the caret can go into, so pressing one does not
    // take the caret out of the box.
    let box_h = element(&mut c, ENTRY_BOX);
    let tile = grid_tile(&mut c, SPLIT_STACK);
    press_with(&mut c, tile, dereth_ui::focus::action::PRIMARY_CLICK, 11.0);
    let caret_stayed = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.focus_element() == Some(box_h)
    };

    commit_by_leaving_the_box(&mut c);
    let taken = both_copies(&mut c) == (5, (5, WHOLE));

    let to = an_empty_grid_tile(&mut c);
    drag_the_stack(&mut c, SPLIT_STACK, to);
    let and_the_drop_carried_it = matches!(
        stack_requests(&c).as_slice(),
        [super::Request::StackableSplitToContainer(m)] if m.stack == SPLIT_STACK && m.amount == 5
    );

    c.assert_behaviour(
        "inventory.split.a-typed-quantity-is-taken-when-the-caret-leaves-the-box",
        move |_| in_the_box && not_yet && caret_stayed && taken && and_the_drop_carried_it,
    );
    c.shutdown();
}

/// A quantity larger than the stack, or none at all, is corrected in the box itself -- and the
/// two corrections land on two different requests.
pub fn a_quantity_larger_than_the_stack_or_none_at_all_is_corrected_in_the_box() {
    // ---- more than there are: the whole stack, and a whole-stack move ----------------------
    let mut c = a_client_with_a_stack_of_twenty();
    select(&mut c, SPLIT_STACK);
    type_a_quantity(&mut c, "999");
    commit_by_leaving_the_box(&mut c);
    let clamped_up =
        box_text(&mut c) == WHOLE.to_string() && both_copies(&mut c) == (WHOLE, (WHOLE, WHOLE));

    // …and the correction is not a latch: a number inside the stack still works afterwards.
    type_a_quantity(&mut c, "7");
    commit_by_leaving_the_box(&mut c);
    let not_a_latch = both_copies(&mut c) == (7, (7, WHOLE));
    type_a_quantity(&mut c, "999");
    commit_by_leaving_the_box(&mut c);

    // Into the side pack, so the move really is a move: all of a stack going to a new place
    // in the list it is already in is a different question.
    let to = side_pack_tile(&mut c, SPLIT_PACK);
    drag_the_stack(&mut c, SPLIT_STACK, to);
    // Taking all of them is a move of the stack itself and not a split of it, which is the
    // difference an assertion on the number alone cannot see.
    let a_whole_stack_move = matches!(
        stack_requests(&c).as_slice(),
        [super::Request::PutItemInContainer(_)]
    );
    c.shutdown();

    // ---- none at all: one of them, and a real split ----------------------------------------
    let mut c = a_client_with_a_stack_of_twenty();
    select(&mut c, SPLIT_STACK);
    type_a_quantity(&mut c, "0");
    commit_by_leaving_the_box(&mut c);
    let clamped_down = box_text(&mut c) == "1" && both_copies(&mut c) == (1, (1, WHOLE));
    let to = an_empty_grid_tile(&mut c);
    drag_the_stack(&mut c, SPLIT_STACK, to);
    let a_split_of_one = matches!(
        stack_requests(&c).as_slice(),
        [super::Request::StackableSplitToContainer(m)] if m.amount == 1
    );

    eprintln!(
        "DBG clamp {clamped_up} {not_a_latch} {a_whole_stack_move} {clamped_down} {a_split_of_one}"
    );
    c.assert_behaviour(
        "inventory.split.a-quantity-larger-than-the-stack-or-none-at-all-is-corrected-in-the-box",
        move |_| clamped_up && not_a_latch && a_whole_stack_move && clamped_down && a_split_of_one,
    );
    c.shutdown();
}

/// Picking a different thing sets the quantity back to all of it, on both copies -- so a drag
/// of the new thing carries the new thing's whole count and not the last one's choice.
pub fn picking_a_different_thing_sets_the_quantity_back_to_all_of_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    {
        let w = &mut c.objects_mut().world;
        seed_a_stack(w);
        seed_a_second_stack(w);
    }
    open_pack(&mut c);
    c.tick(3);

    select(&mut c, SPLIT_STACK);
    let seeded =
        both_copies(&mut c) == (WHOLE, (WHOLE, WHOLE)) && c.view().world().split.is_whole_stack();

    // Take five of the twenty, so the far end holds a part of a stack rather than all of one.
    type_a_quantity(&mut c, "5");
    commit_by_leaving_the_box(&mut c);
    let a_part_of_it = both_copies(&mut c) == (5, (5, WHOLE));

    // Now pick the other stack and leave it alone: it is all of the new stack, not five.
    select(&mut c, OTHER_STACK);
    let re_seeded = both_copies(&mut c) == (OTHER_WHOLE, (OTHER_WHOLE, OTHER_WHOLE))
        && c.view().world().split.is_whole_stack();

    // The not-a-stack case needs its own part, because a far end left at the last whole stack
    // would answer "all of it" too. So put a part of a stack there first.
    type_a_quantity(&mut c, "5");
    commit_by_leaving_the_box(&mut c);
    let a_part_again =
        both_copies(&mut c) == (5, (5, OTHER_WHOLE)) && !c.view().world().split.is_whole_stack();

    select(&mut c, SPLIT_PLAYER);
    let reset = both_copies(&mut c) == (1, (1, 1)) && c.view().world().split.is_whole_stack();

    c.assert_behaviour(
        "inventory.split.picking-a-different-thing-sets-the-quantity-back-to-all-of-it",
        move |_| seeded && a_part_of_it && re_seeded && a_part_again && reset,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// The recorded splits, end to end.
// -----------------------------------------------------------------------------------------

use dereth_testkit::outbound::Sent;
use dereth_testkit::Outbound;

/// The ordered action a split is.
const SPLIT_TO_CONTAINER: u32 = 0x0055;

/// Every split the recording's own client asked for, in recorded order.
fn recorded_splits() -> Vec<Sent> {
    let out: Vec<Sent> = Outbound::all(SPLIT_SESSION)
        .into_iter()
        .filter(|s| s.message == SPLIT_TO_CONTAINER)
        .collect();
    assert!(
        out.len() >= 2,
        "this scenario is about a second split following a first; the recording carries {}",
        out.len()
    );
    out
}

/// What one recorded split asked for.
fn asked_for(s: &Sent) -> (ObjectId, u32, u32) {
    (
        ObjectId(s.field(0).expect("a split names the stack")),
        s.field(2).expect("a split names the place"),
        s.field(3).expect("a split names how many"),
    )
}

/// The bytes one of this client's own splits would go out as, in the recording's own shape.
fn split_bytes(m: &dereth_protocol::items::InventoryStackableSplitToContainer) -> Vec<u8> {
    let mut b = SPLIT_TO_CONTAINER.to_le_bytes().to_vec();
    b.extend(dereth_protocol::write_body(m).expect("the body encodes"));
    b
}

/// Every split this client has asked for since `from`, as those bytes.
fn splits_since(c: &HeadlessClient, from: usize) -> Vec<Vec<u8>> {
    c.view().outbound()[from..]
        .iter()
        .filter_map(|r| match r {
            super::Request::StackableSplitToContainer(m) => Some(split_bytes(m)),
            _ => None,
        })
        .collect()
}

/// How many requests the client has made so far, so that the next reading is about the next
/// gesture and not about every gesture.
fn asked_so_far(c: &HeadlessClient) -> usize {
    c.view().outbound().len()
}

/// Exactly one split, and it is byte for byte the one the recording's own client sent.
fn matches_the_recorded_split(c: &HeadlessClient, from: usize, recorded: &Sent) -> bool {
    splits_since(c, from) == vec![recorded.payload[8..].to_vec()]
}

/// The recording replayed up to the moment before its first split, with the pack open.
///
/// Two lines of it are the world the shard would have built rather than anything this
/// scenario claims: the recording leaves a shop open over the lower rows of the pack -- a
/// player with a shop over his pack closes the shop, and so does this -- and one frame runs
/// before the pack is raised, because in a running client the book the recording opens and
/// the frame that opens it are the same frame.
fn a_client_at_the_recorded_split() -> (HeadlessClient, Vec<Sent>, ObjectId, u32) {
    let splits = recorded_splits();
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The recording carries no login, so the one thing a login would have told the client --
    // which character he is -- is the premise. It is read off the recorded split, which names
    // the pack it is going into, rather than written down.
    c.objects_mut().world.player = Some(ObjectId(
        splits[0]
            .field(1)
            .expect("a split names the pack it goes into"),
    ));
    c.when(Inbound::corpus_until(SPLIT_SESSION, splits[0].idx));
    {
        let w = &mut c.objects_mut().world;
        w.close_vendor(&mut dereth_client_model::NullSink);
    }
    c.tick(1);
    open_pack(&mut c);
    // The recording leaves a side pack open, and the grid draws whatever is open rather than
    // the player's own pack. A player looking for his stack clicks his own pack; this is that,
    // and it is the world the recording built rather than anything asserted here.
    {
        let me = c
            .view()
            .world()
            .player
            .expect("the recording carried its own character");
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.inventory.open_container = Some(me);
    }
    c.tick(3);
    let (source, _, _) = asked_for(&splits[0]);
    let how_many = {
        let w = c.view().world();
        assert!(
            w.request_lock.is_idle(),
            "the premise: the player is free to act"
        );
        u32::from(
            w.weenie(source)
                .expect("the premise: the stack the recording splits is in the pack")
                .pwd
                .stack_size
                .expect("and it is a stack"),
        )
    };
    (c, splits, source, how_many)
}

/// Pick a thing by clicking its tile in the pack.
fn click_the_tile_of(c: &mut HeadlessClient, item: ObjectId) {
    let h = grid_tile(c, item);
    let at = exposed_point(c, h);
    c.when(Player::Click(at));
    c.tick(2);
}

/// Whether the pack is drawing `item`'s tile as busy.
fn tile_is_busy(c: &mut HeadlessClient, item: ObjectId) -> bool {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the pack grid")
        .slots
        .iter()
        .find(|s| s.item == Some(item))
        .is_some_and(|s| s.waiting)
}

/// The shard refusing a request about `object`, which is what frees the player again.
fn the_shard_refuses(c: &mut HeadlessClient, object: ObjectId) {
    c.when(Inbound::message(
        &dereth_protocol::objects::CharacterServerSaysAttemptFailed { object, reason: 0 },
    ));
    c.tick(3);
}

/// Two splits in a row, each answered by the recording's own reply, with the quantity handed
/// over by pressing return the first time and by putting the caret somewhere else the second.
///
/// The journey is the same either way the typed quantity is handed over, so both ways are here
/// rather than in two scenarios.
pub fn two_splits_in_a_row_each_go_out_and_the_shards_answers_follow_them() {
    let (mut c, splits, source, how_many) = a_client_at_the_recorded_split();
    let (_, first_place, first_amount) = asked_for(&splits[0]);

    click_the_tile_of(&mut c, source);
    let picked = c.view().world().selected == Some(source)
        && c.view().world().split.max_split_size == how_many;

    // Typed, and handed over by pressing return.
    type_a_quantity(&mut c, &first_amount.to_string());
    let typing_alone = c.view().world().split.split_size == how_many;
    dereth_testkit::input_steps::press_return(&mut c);
    c.tick(2);
    let entry = element(&mut c, ENTRY_BOX);
    let return_committed = {
        let taken = c.view().world().split.split_size == first_amount;
        let (ui, _screen) = gameplay_screen(c.app_mut());
        taken && ui.focus_element() != Some(entry)
    };

    let before = asked_so_far(&c);
    let to = grid_tile_at(&mut c, first_place as usize);
    drag_the_stack(&mut c, source, to);
    let went_out = matches_the_recorded_split(&c, before, &splits[0]);
    let held = {
        let w = c.view().world();
        w.request_lock.object == Some(source)
                && w.pending_split.is_some_and(|p| u32::from(p.stack_size) == first_amount)
                && w.weenie(source).expect("the stack").waiting
                // Nothing is predicted: the stack still says what it said before.
                && u32::from(w.weenie(source).expect("the stack").pwd.stack_size.unwrap_or(0))
                    == how_many
    };

    // The recording's own answers to that split.
    c.when(Inbound::from_corpus(
        SPLIT_SESSION,
        splits[0].idx..splits[0].idx + 4,
    ));
    c.tick(3);
    let answered = {
        let w = c.view().world();
        w.request_lock.is_idle()
            && !w.weenie(source).expect("the stack").waiting
            && u32::from(
                w.weenie(source)
                    .expect("the stack")
                    .pwd
                    .stack_size
                    .unwrap_or(0),
            ) == how_many - first_amount
    };
    let new_stack = c
        .view()
        .world()
        .selected
        .expect("the answer picked the new stack");
    let the_new_one = new_stack != source
        && u32::from(
            c.view()
                .world()
                .weenie(new_stack)
                .expect("the new stack")
                .pwd
                .stack_size
                .unwrap_or(0),
        ) == first_amount;

    // And the second split, with the quantity handed over by putting the caret elsewhere.
    let (_, second_place, second_amount) = asked_for(&splits[1]);
    let reseeded = c.view().world().split.split_size == first_amount;
    type_a_quantity(&mut c, &second_amount.to_string());
    commit_by_leaving_the_box(&mut c);
    let before = asked_so_far(&c);
    let to = grid_tile_at(&mut c, second_place as usize);
    drag_the_stack(&mut c, new_stack, to);
    let second_went_out = matches_the_recorded_split(&c, before, &splits[1]);

    c.when(Inbound::from_corpus(
        SPLIT_SESSION,
        splits[1].idx..splits[1].idx + 4,
    ));
    c.tick(3);
    let second_answered = {
        let w = c.view().world();
        w.request_lock.is_idle()
            && !w.weenie(new_stack).expect("the stack").waiting
            && u32::from(
                w.weenie(new_stack)
                    .expect("the stack")
                    .pwd
                    .stack_size
                    .unwrap_or(0),
            ) == first_amount - second_amount
    };
    let nothing_left_busy = !tile_is_busy(&mut c, source) && !tile_is_busy(&mut c, new_stack);

    c.assert_behaviour(
        "inventory.split.two-splits-in-a-row-each-go-out-and-the-shards-answers-follow-them",
        move |_| {
            picked
                && typing_alone
                && return_committed
                && went_out
                && held
                && answered
                && the_new_one
                && reseeded
                && second_went_out
                && second_answered
                && nothing_left_busy
        },
    );
    c.shutdown();
}

/// A leading zero and a number too big to hold are read the way the client reads them, and
/// only a correction rewrites what the box is showing.
pub fn a_leading_zero_and_a_number_too_big_are_read_the_way_the_client_reads_them() {
    let (mut c, _splits, source, how_many) = a_client_at_the_recorded_split();
    click_the_tile_of(&mut c, source);

    type_a_quantity(&mut c, "0010");
    commit_by_leaving_the_box(&mut c);
    // A leading zero makes it an eight, not a ten.
    let octal = c.view().world().split.split_size == 8;
    let spelling_kept = box_text(&mut c) == "0010";

    type_a_quantity(&mut c, "4294967296");
    commit_by_leaving_the_box(&mut c);
    // One more than the biggest number the client can hold saturates and is then corrected to
    // the stack, rather than wrapping round to nought and coming back as one.
    let saturated =
        c.view().world().split.split_size == how_many && box_text(&mut c) == how_many.to_string();

    c.assert_behaviour(
            "inventory.split.a-leading-zero-and-a-number-too-big-are-read-the-way-the-client-reads-them",
            move |_| octal && spelling_kept && saturated,
        );
    c.shutdown();
}

/// Escape in the quantity box changes nothing, and the panel's own cancel puts back the
/// quantity that was last handed over rather than the whole stack.
pub fn escape_in_the_box_changes_nothing_and_cancel_puts_the_taken_quantity_back() {
    let (mut c, splits, source, how_many) = a_client_at_the_recorded_split();
    let (_, place, amount) = asked_for(&splits[0]);
    click_the_tile_of(&mut c, source);

    type_a_quantity(&mut c, &amount.to_string());
    commit_by_leaving_the_box(&mut c);
    type_a_quantity(&mut c, "99");

    let entry = element(&mut c, ENTRY_BOX);
    let before = asked_so_far(&c);
    press_escape(&mut c);
    let escape_did_nothing = {
        let quiet = c.view().outbound().len() == before && box_text(&mut c) == "99";
        let (ui, _screen) = gameplay_screen(c.app_mut());
        quiet && ui.focus_element() == Some(entry)
    };
    let the_box_keeps_the_caret = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        !ui.text_element_mut(entry)
            .expect("a text box")
            .bits
            .lose_focus_on_escape()
    };

    // The panel's own cancel, which is a different thing from the key the box just ate: it is
    // the message the toolbar listens for, and it puts back the quantity that was handed over
    // rather than the whole stack.
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.broadcast_global(dereth_ui::msg::global::KEY_DOWN_UNCONSUMED, 0x27);
    }
    c.tick(3);
    let put_back = {
        let back =
            box_text(&mut c) == amount.to_string() && c.view().world().split.split_size == amount;
        let (ui, _screen) = gameplay_screen(c.app_mut());
        back && ui.focus_element() != Some(entry)
    };
    assert!(
        amount != how_many,
        "the premise: the taken quantity is not the whole stack"
    );

    let before = asked_so_far(&c);
    let to = grid_tile_at(&mut c, place as usize);
    drag_the_stack(&mut c, source, to);
    let and_the_drag_carried_it = matches_the_recorded_split(&c, before, &splits[0]);

    c.assert_behaviour(
        "inventory.split.escape-in-the-box-changes-nothing-and-cancel-puts-the-taken-quantity-back",
        move |_| {
            escape_did_nothing && the_box_keeps_the_caret && put_back && and_the_drag_carried_it
        },
    );
    c.shutdown();
}

/// The escape key, as a key rather than as an action.
fn press_escape(c: &mut HeadlessClient) {
    let key = dereth_desktop::platform::window::key_from_key_code(winit::keyboard::KeyCode::Escape)
        .expect("the host names this key");
    dereth_testkit::input_steps::key(c, key, true);
    c.tick(1);
    dereth_testkit::input_steps::key(c, key, false);
    c.tick(2);
}

/// A drag that ends nowhere and a refusal from the shard both leave the stack free for the
/// next try.
pub fn a_drag_that_ends_nowhere_and_a_refusal_both_free_the_stack() {
    let (mut c, splits, source, how_many) = a_client_at_the_recorded_split();
    let (_, place, amount) = asked_for(&splits[0]);
    click_the_tile_of(&mut c, source);
    type_a_quantity(&mut c, &amount.to_string());
    commit_by_leaving_the_box(&mut c);

    // A drag let go where there is nothing at all.
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        assert!(
            ui.hit_test_screen(-40, -40).is_none(),
            "the premise: there is nothing off the corner of the screen to let go on"
        );
    }
    let before = asked_so_far(&c);
    let from = grid_tile(&mut c, source);
    let from = exposed_point(&mut c, from);
    c.when(Player::Drag {
        from,
        to: Target::Point(ScreenPoint::new(-40, -40)),
        hold_frames: 1,
    });
    c.tick(2);
    let nowhere = splits_since(&c, before).is_empty()
        && c.view().world().request_lock.is_idle()
        && !c.view().world().weenie(source).expect("the stack").waiting
        && !tile_is_busy(&mut c, source);

    // The same gesture, aimed at the place the recording aimed at.
    let before = asked_so_far(&c);
    let to = grid_tile_at(&mut c, place as usize);
    drag_the_stack(&mut c, source, to);
    let went_out = matches_the_recorded_split(&c, before, &splits[0])
        && c.view().world().weenie(source).expect("the stack").waiting;

    // A failed drop somewhere else entirely must not let the player go while he is waiting.
    {
        let unrelated = element(&mut c, ENTRY_BOX);
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(unrelated, dereth_ui::msg::element::id::DROP_FAILED, 0, 0);
    }
    c.tick(2);
    let still_waiting = !c.view().world().request_lock.is_idle()
        && c.view().world().weenie(source).expect("the stack").waiting
        && tile_is_busy(&mut c, source);

    // The shard's refusal does let him go, and changes nothing about the stack.
    the_shard_refuses(&mut c, source);
    let freed = c.view().world().request_lock.is_idle()
        && !c.view().world().weenie(source).expect("the stack").waiting
        && u32::from(
            c.view()
                .world()
                .weenie(source)
                .expect("the stack")
                .pwd
                .stack_size
                .unwrap_or(0),
        ) == how_many
        && c.view().world().split.split_size == amount;

    let before = asked_so_far(&c);
    let to = grid_tile_at(&mut c, place as usize);
    drag_the_stack(&mut c, source, to);
    let and_again = matches_the_recorded_split(&c, before, &splits[0]);

    c.assert_behaviour(
        "inventory.split.a-drag-that-ends-nowhere-and-a-refusal-both-free-the-stack",
        move |_| nowhere && went_out && still_waiting && freed && and_again,
    );
    c.shutdown();
}

/// The slider's two ends are one of them and all of them, and the two go out as two different
/// requests.
pub fn the_sliders_two_ends_are_one_of_them_and_all_of_them() {
    let (mut c, splits, source, how_many) = a_client_at_the_recorded_split();
    let (_, place, _) = asked_for(&splits[0]);
    click_the_tile_of(&mut c, source);

    let (bar, thumb_y, x0, x1) = {
        let h = element(&mut c, SLIDER);
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        (h, (b.y0 + b.y1) / 2, b.x0, b.x1)
    };
    // The thumb is see-through to the bar it belongs to, so the gesture below really is the
    // player pushing the bar's own thumb about.
    {
        let thumb = {
            let (ui, _screen) = gameplay_screen(c.app_mut());
            ui.get_child(bar, dereth_ui::widgets::scrollbar::WIDGET_ID)
                .expect("a thumb")
        };
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let t = ui.screen_box(thumb);
        let mid = ((t.x0 + t.x1) / 2, (t.y0 + t.y1) / 2);
        assert_eq!(
            ui.hit_test_screen(mid.0, mid.1),
            Some(bar),
            "the premise: the thumb answers the pointer as the bar it sits on"
        );
    }

    // All the way to the left: one of them.
    c.when(Player::Drag {
        from: Target::Point(ScreenPoint::new((x0 + x1) / 2, thumb_y)),
        to: Target::Point(ScreenPoint::new(x0, thumb_y)),
        hold_frames: 1,
    });
    let just_one = c.view().world().split.split_size == 1 && box_text(&mut c) == "1";

    let before = asked_so_far(&c);
    let to = grid_tile_at(&mut c, place as usize);
    drag_the_stack(&mut c, source, to);
    let a_split_of_one = matches!(
        c.view().outbound()[before..],
        [super::Request::StackableSplitToContainer(ref m)]
            if m.stack == source && m.slot == place && m.amount == 1
    );

    // Freed again, so the next gesture is measured and not the hold.
    the_shard_refuses(&mut c, source);

    // And all the way to the right: all of them.
    c.when(Player::Drag {
        from: Target::Point(ScreenPoint::new((x0 + x1) / 2, thumb_y)),
        to: Target::Point(ScreenPoint::new(x1, thumb_y)),
        hold_frames: 1,
    });
    let all_of_them =
        c.view().world().split.split_size == how_many && box_text(&mut c) == how_many.to_string();

    let before = asked_so_far(&c);
    let to = grid_tile_at(&mut c, place as usize);
    drag_the_stack(&mut c, source, to);
    // All of them is a move of the stack itself, and the place it lands is one earlier than
    // the one aimed at, because the thing being moved is taken out of the list on its way.
    let a_whole_stack_move = matches!(
        c.view().outbound()[before..],
        [super::Request::PutItemInContainer(ref m)] if m.item == source && m.slot == place - 1
    );

    c.assert_behaviour(
        "inventory.split.the-sliders-two-ends-are-one-of-them-and-all-of-them",
        move |_| just_one && a_split_of_one && all_of_them && a_whole_stack_move,
    );
    c.shutdown();
}

/// Starting to drag the stack takes the quantity that is showing in the box, with no other
/// gesture in between.
pub fn starting_to_drag_the_stack_takes_the_quantity_in_the_box() {
    let (mut c, splits, source, _how_many) = a_client_at_the_recorded_split();
    let (_, place, amount) = asked_for(&splits[0]);
    click_the_tile_of(&mut c, source);

    let entry = element(&mut c, ENTRY_BOX);
    press_the_split_key(&mut c);
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "the premise: the key put the caret there"
        );
    }
    c.when(Player::Type(amount.to_string()));
    let typed = box_text(&mut c) == amount.to_string();

    // No return, no click anywhere else: the drag itself is the second gesture.
    let before = asked_so_far(&c);
    let from = grid_tile(&mut c, source);
    let from = exposed_point(&mut c, from);
    c.when(Player::Grab(from));
    let taken_on_the_lift = {
        let taken = c.view().world().split.split_size == amount;
        let (ui, _screen) = gameplay_screen(c.app_mut());
        taken && ui.drag_state().element.is_some() && ui.focus_element() != Some(entry)
    };
    let to = grid_tile_at(&mut c, place as usize);
    let to = exposed_point(&mut c, to);
    c.when(Player::Drop(to));
    let went_out = matches_the_recorded_split(&c, before, &splits[0])
        && box_text(&mut c) == amount.to_string();

    c.assert_behaviour(
        "inventory.split.starting-to-drag-the-stack-takes-the-quantity-in-the-box",
        move |_| typed && taken_on_the_lift && went_out,
    );
    c.shutdown();
}
