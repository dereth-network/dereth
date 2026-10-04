//! Shell fixtures and scenarios for chargen appearance.

use super::*;
use dereth_ui_screens::screens::chargen;
// ---------------------------------------------------------------------------------------------
// The left arrow: **not a row here.** Walking back a page, and raising the exit warning from the
// first, is exactly what
// `chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other` says and
// what the exit scenario asserts.
// ---------------------------------------------------------------------------------------------

// =============================================================================================
// chargen.random.* and chargen.appearance.* -- the rolled character and the colour wheel
//
// The roll written out as its own call list, and the two picture operations' arithmetic pinned
// against literals, would be transcriptions and are not rows. Two runs rolling the same character,
// the stream position after a roll, and the draw that does not draw are legs of the scenarios below
// rather than rows of their own.
//
// **No generator is named twice.** This crate does not depend on `dereth_primitives::num` and must
// not gain an edge for a test, so the oracle is a second `CharGenRng` seeded the same way -- which
// is the client's own pair of streams and the very thing the claim is about.
// =============================================================================================

/// Everything the opening roll writes, as one comparable value.
#[derive(Debug, Clone, PartialEq)]
struct Roll {
    heritage: u32,
    gender: u32,
    start_area: u32,
    template: i32,
    face: [i32; 6],
    gear: [i32; 8],
    shades: [u64; 6],
}

fn roll_of(s: &CharGenState) -> Roll {
    Roll {
        heritage: s.heritage_group,
        gender: s.gender,
        start_area: s.start_area,
        template: s.template,
        face: [
            s.eyes_strip,
            s.nose_strip,
            s.mouth_strip,
            s.hair_color,
            s.eye_color,
            s.hair_style,
        ],
        gear: [
            s.headgear_style,
            s.headgear_color,
            s.shirt_style,
            s.shirt_color,
            s.trousers_style,
            s.trousers_color,
            s.footwear_style,
            s.footwear_color,
        ],
        shades: [
            s.skin_shade.to_bits(),
            s.hair_shade.to_bits(),
            s.headgear_shade.to_bits(),
            s.shirt_shade.to_bits(),
            s.trousers_shade.to_bits(),
            s.footwear_shade.to_bits(),
        ],
    }
}

/// The two shipped tables, out of the client's own store.
pub(super) fn chargen_tables(
    c: &HeadlessClient,
) -> (
    dereth_assets::tables::CharGen,
    dereth_assets::tables::SkillTable,
) {
    use dereth_assets::Decode as _;
    use dereth_primitives::{AssetSource as _, DataId};
    let store = c.dat_store().expect("a retail client has a store").clone();
    let cg_bytes = store
        .read(DataId(0x0E00_0002))
        .expect("the character-creation table");
    let cg = dereth_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &cg_bytes)
        .expect("it decodes");
    let sk_bytes = store.read(DataId(0x0E00_0004)).expect("the skill table");
    let sk = dereth_assets::tables::SkillTable::decode_payload(DataId(0x0E00_0004), &sk_bytes)
        .expect("it decodes");
    (cg, sk)
}

/// Every clothing table the character-creation tables name, which is the closed set the shell
/// hands the wizard. Without them a part reports no colours at all and four draws never happen,
/// so a roll built without them is a roll from a different place in the stream.
fn clothing_tables(
    c: &HeadlessClient,
    cg: &dereth_assets::tables::CharGen,
) -> std::rc::Rc<
    std::collections::BTreeMap<dereth_primitives::DataId, dereth_assets::motion::ClothingTable>,
> {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let store = c.dat_store().expect("a retail client has a store").clone();
    let mut out = std::collections::BTreeMap::new();
    for hg in cg.heritage_groups.values() {
        for sx in hg.sexes.values() {
            for item in sx
                .headgear
                .iter()
                .chain(&sx.shirts)
                .chain(&sx.pants)
                .chain(&sx.footwear)
            {
                let id = item.clothing_table;
                if id.0 == 0 || out.contains_key(&id) {
                    continue;
                }
                let bytes = store
                    .read(id)
                    .expect("a named clothing table is in the data files");
                out.insert(
                    id,
                    dereth_assets::motion::ClothingTable::decode_payload(id, &bytes)
                        .expect("it decodes"),
                );
            }
        }
    }
    std::rc::Rc::new(out)
}

/// A character rolled off the two tables with the seeds named, with no shell at all.
fn rolled_with(c: &HeadlessClient, ran2: i32, crt: u32, expansion: bool) -> CharGenState {
    let (cg, sk) = chargen_tables(c);
    let mut s = CharGenState::default();
    s.rng = CharGenRng::new(ran2, crt);
    s.clothing = clothing_tables(c, &cg);
    s.randomize_character(&cg, &sk, expansion);
    s
}

/// Walk to a page by its tab, and check the wizard really got there.
pub(super) fn goto_page(c: &mut HeadlessClient, p: EcgProgress) {
    click_wizard(c, p.select_button().expect("every page has a tab"));
    assert_eq!(
        with_wizard(c, |_, w| w.progress),
        p,
        "the wizard reached {p:?}"
    );
}

/// The picture an element is showing, and the operation it is shown through.
fn picture_of(
    c: &HeadlessClient,
    id: ElementId,
) -> (
    Option<dereth_primitives::DataId>,
    Option<dereth_ui::region::SurfaceOp>,
) {
    let h = element(c, id);
    let n = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live");
    match &n.region.image {
        Some(g) => (Some(g.did), g.op),
        None => (None, None),
    }
}

fn tab_is_visible(c: &HeadlessClient, p: EcgProgress) -> bool {
    let h = element(c, p.select_button().expect("a tab"));
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .flags
        .visible
}

/// The rolled character's own half of the shipped table.
fn sex_of(c: &mut HeadlessClient) -> dereth_assets::tables::SexCg {
    let (t, heritage, gender) = with_wizard(c, |_, w| {
        (
            w.tables.clone().expect("the tables"),
            w.state.heritage_group,
            w.state.gender,
        )
    });
    t.chargen
        .heritage_groups
        .get(&heritage)
        .and_then(|h| h.sexes.get(&gender))
        .expect("the rolled people and sex are in the shipped table")
        .clone()
}

/// One entry of a palette, read out of the data file itself rather than off the host code.
fn palette_entry(c: &HeadlessClient, id: dereth_primitives::DataId, index: usize) -> u32 {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let store = c.dat_store().expect("a retail client has a store").clone();
    let bytes = store.read(id).expect("the palette is in the data files");
    let p = dereth_assets::material::Palette::decode_payload(id, &bytes).expect("it decodes");
    p.colors_argb[index]
}

/// The per-channel mean of one entry across a palette set, which is what a colour spot shows.
fn palette_set_average(c: &HeadlessClient, id: dereth_primitives::DataId, index: usize) -> u32 {
    use dereth_assets::Decode as _;
    use dereth_primitives::AssetSource as _;
    let store = c.dat_store().expect("a retail client has a store").clone();
    let bytes = store
        .read(id)
        .expect("the palette set is in the data files");
    let ps = dereth_assets::material::PaletteSet::decode_payload(id, &bytes).expect("it decodes");
    let n = u32::try_from(ps.palette_ids.len()).expect("a non-empty set");
    let (mut r, mut g, mut b) = (0_u32, 0_u32, 0_u32);
    for p in &ps.palette_ids {
        let col = palette_entry(c, *p, index);
        r += (col >> 16) & 0xFF;
        g += (col >> 8) & 0xFF;
        b += col & 0xFF;
    }
    0xFF00_0000 | ((r / n) << 16) | ((g / n) << 8) | (b / n)
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-wizard-opens-on-a-character-already-rolled
// ---------------------------------------------------------------------------------------------

/// Every field a player can see is a real choice the moment the wizard opens.
pub(super) fn the_wizard_opens_on_a_character_already_rolled() {
    let mut c = a_client_on_the_wizard();
    let chosen = with_wizard(&mut c, |_, w| {
        let s = &w.state;
        let people =
            s.heritage_group != 0 && s.gender != 0 && s.start_area != u32::MAX && s.template >= 1;
        let face = [
            s.eyes_strip,
            s.nose_strip,
            s.mouth_strip,
            s.hair_color,
            s.eye_color,
            s.hair_style,
        ]
        .iter()
        .all(|v| *v >= 0);
        // A hat is the one part whose "none" is a real answer, so only its colour is read.
        let clothes = s.shirt_style >= 0 && s.trousers_style >= 0 && s.footwear_style >= 0;
        let shades = [
            s.skin_shade,
            s.hair_shade,
            s.shirt_shade,
            s.trousers_shade,
            s.footwear_shade,
        ]
        .iter()
        .all(|v| (0.0..=1.0).contains(v));
        // And the roll's own tail ran: the three choices it makes are settled rather than open.
        people && face && clothes && shades && s.frozen == [true; 3]
    });

    c.assert_behaviour(
        "chargen.random.the-wizard-opens-on-a-character-already-rolled",
        move |_| chosen,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character
// ---------------------------------------------------------------------------------------------

/// The roll is a function of the seeds, and the shell adds one thing to it: the bared head.
pub(super) fn the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character() {
    let mut c = a_client_on_the_wizard();
    let shown = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));

    // The oracle is a second pair of the client's own streams, seeded the way a client with no
    // sound device and no window seeds its own: the people is the first draw of one of them and
    // the sex is the very next, with no draw of the other in between.
    let mut oracle = CharGenRng::new(1, 1);
    let heritage = u32::try_from(oracle.roll_dice(1, 3)).expect("one of three");
    let gender = u32::try_from(oracle.roll_dice(1, 2)).expect("one of two");
    let first_two_draws = shown.heritage == heritage && shown.gender == gender;

    // The same character rolled with no shell at all agrees in every field but one: the page the
    // player arrives on takes the hat off so the face can be seen, and parks the style it took.
    let bare = roll_of(&rolled_with(&c, 1, 1, false));
    let bared_head =
        shown.gear[0] == -1 && with_wizard(&mut c, |_, w| w.hold_headgear) == bare.gear[0];
    let mut want = bare;
    want.gear[0] = -1;
    let nothing_else_added = want == shown;
    c.shutdown();

    // And a second client rolls the identical character, down to the shade of every part.
    let mut b = a_client_on_the_wizard();
    let again = roll_of(&with_wizard(&mut b, |_, w| w.state.clone()));
    let reproducible = again == shown;

    b.assert_behaviour(
        "chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character",
        move |_| first_two_draws && bared_head && nothing_else_added && reproducible,
    );
    b.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-people-and-the-sex-come-out-of-a-different-draw-from-everything-else
// ---------------------------------------------------------------------------------------------

/// Two streams, proved from both sides, and the first left exactly two draws along.
pub(super) fn the_people_and_the_sex_come_out_of_their_own_stream() {
    let mut c = a_client_on_the_wizard();

    let a = rolled_with(&c, 1, 1, false);
    let b = rolled_with(&c, 1, 12_345, false);
    // Move only the second stream's seed: the people and the sex must not budge...
    let unmoved =
        (a.heritage_group, a.gender) == (b.heritage_group, b.gender) && roll_of(&a) != roll_of(&b);

    // ...and move only the first stream's seed to one that lands elsewhere, and they must.
    let base = CharGenRng::new(1, 1).roll_dice(1, 3);
    let other = (2..2000)
        .find(|s| CharGenRng::new(*s, 1).roll_dice(1, 3) != base)
        .expect("some seed of the first stream lands on a different people");
    let moved = rolled_with(&c, other, 1, false).heritage_group != a.heritage_group;

    // And the sharper reading the values alone cannot give: after a roll the first stream is
    // exactly two draws along. A people taken from the *other* stream would leave it one short,
    // and under these seeds that would still have produced the same people.
    let mut oracle = CharGenRng::new(1, 1);
    oracle.roll_dice(1, 3);
    oracle.roll_dice(1, 2);
    let mut mine = a.rng.ran2.clone();
    let mut theirs = oracle.ran2;
    let in_step = (0..8).all(|_| (mine.next_f64() - theirs.next_f64()).abs() < f64::EPSILON);

    c.assert_behaviour(
        "chargen.random.the-people-and-the-sex-come-out-of-a-different-draw-from-everything-else",
        move |_| unmoved && moved && in_step,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-opening-people-is-one-a-new-account-may-play-and-the-expansion-adds-one
// ---------------------------------------------------------------------------------------------

/// Three peoples without the expansion and four with it, out of the thirteen the data file holds.
pub(super) fn the_opening_people_is_one_a_plain_account_may_play() {
    let mut c = a_client_on_the_wizard();
    let (cg, sk) = chargen_tables(&c);

    let mut plain = std::collections::BTreeSet::new();
    let mut with_expansion = std::collections::BTreeSet::new();
    for seed in 1..512 {
        for (set, expansion) in [(&mut plain, false), (&mut with_expansion, true)] {
            let mut s = CharGenState::default();
            s.rng = CharGenRng::new(seed, 1);
            s.randomize_character(&cg, &sk, expansion);
            set.insert(s.heritage_group);
        }
    }
    let three = plain == [1, 2, 3].into_iter().collect();
    let four = with_expansion == [1, 2, 3, 4].into_iter().collect();
    // ...out of all thirteen the data file still carries, which is what makes three a choice.
    let thirteen = cg.heritage_groups.len() >= 13;

    c.assert_behaviour("chargen.random.the-opening-people-is-one-a-plain-account-may-play-and-the-expansion-adds-one", move |_| {
        three && four && thirteen
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.heritage.the-home-town-follows-the-people-and-is-one-of-that-peoples-own
// ---------------------------------------------------------------------------------------------

/// Choosing a people settles a home town, from that people's own list -- and, as the data files
/// ship, that list has one entry, so the town is a consequence of the people and not a roll.
pub(super) fn the_home_town_follows_the_people() {
    let mut c = a_client_on_the_wizard();
    let (cg, sk) = chargen_tables(&c);

    // Every people the data file has: the town settled on has to be one of that people's own.
    let mut from_their_own_list = true;
    for (&h, hg) in &cg.heritage_groups {
        if hg.primary_start_areas.is_empty() {
            continue;
        }
        let mut s = CharGenState::default();
        s.rng = CharGenRng::new(1, 1);
        s.reset(&cg, &sk);
        s.set_heritage_group(&cg, &sk, h);
        from_their_own_list &= hg.primary_start_areas.contains(&s.start_area)
            && (s.start_area as usize) < cg.starter_areas.len();
    }

    // And as the data files ship, every people lists exactly one, so the draw that chooses it has
    // one answer, takes no draw at all, and the town is a function of the people. A data file
    // that ever gave a people two would make that draw real, and this says so rather than
    // leaving a hole where the claim was.
    let one_each = cg.heritage_groups.len() == 13
        && cg
            .heritage_groups
            .values()
            .all(|hg| hg.primary_start_areas.len() == 1);
    let area = |h: u32| cg.heritage_groups[&h].primary_start_areas[0];
    let name = |i: u32| cg.starter_areas[i as usize].name.clone();
    let the_mapping = name(area(1)) == "Holtburg"
        && name(area(2)) == "Yaraq"
        && name(area(3)) == "Shoushi"
        && name(area(4)) == "Sanamar";

    // The draw itself, on a one-entry list: one answer, and the stream does not move -- which is
    // what makes the town a consequence rather than a coincidence.
    let mut r = CharGenRng::new(1, 1);
    let no_draw = r.rand_int_excluding(1, 7) == 0
        && r.rand_int_excluding(0, 7) == 0
        && r.rand_int_excluding(-3, 7) == 0
        && r.crt.next_u16() == CharGenRng::new(1, 1).crt.next_u16();
    // Two or more and it does draw, and never lands on the one it was told to avoid.
    let mut r = CharGenRng::new(1, 1);
    let a_real_draw = (0..200).all(|_| {
        let v = r.rand_int_excluding(4, 2);
        (0..4).contains(&v) && v != 2
    });

    c.assert_behaviour(
        "chargen.heritage.the-home-town-follows-the-people-and-is-one-of-that-peoples-own",
        move |_| from_their_own_list && one_each && the_mapping && no_draw && a_real_draw,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.random.the-button-re-rolls-the-page-the-player-is-on
// ---------------------------------------------------------------------------------------------

/// Each of the five ordinary pages re-rolls its own thing, asserted on that thing and not on
/// "something changed".
pub(super) fn the_random_button_re_rolls_the_page_the_player_is_on() {
    let mut c = a_client_on_the_wizard();
    // An account with the expansion, so the town roll can reach the fourth town and the people
    // roll the fourth people.
    c.app_mut().probe_mut().host_state_mut().account_has_tod = true;
    c.tick(1);

    // The people: the roll may legitimately land where it already is, so it is pressed until it
    // moves. Forty presses of a four-way roll all repeating is not a thing that happens.
    goto_page(&mut c, EcgProgress::Hertage);
    let before = with_wizard(&mut c, |_, w| w.state.heritage_group);
    let mut people_moved = false;
    for _ in 0..40 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        if with_wizard(&mut c, |_, w| w.state.heritage_group) != before {
            people_moved = true;
            break;
        }
    }

    // The profession: the roll excludes the one already chosen, so **one** press must move it,
    // and it never lands on the hand-built one.
    goto_page(&mut c, EcgProgress::Profession);
    let before = with_wizard(&mut c, |_, w| w.state.template);
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let after = with_wizard(&mut c, |_, w| w.state.template);
    let profession_moved = before != after && after >= 1;

    // The skills: re-rolled, and never overspent.
    goto_page(&mut c, EcgProgress::Skills);
    let before = with_wizard(&mut c, |_, w| w.state.skill_levels.clone());
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let skills_moved = with_wizard(&mut c, |_, w| {
        w.state.skill_levels != before && w.state.remaining_skill_credits >= 0
    });

    // The face, with the current choice excluded from every draw, so one press moves all of it.
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    let before = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let after = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    let face_moved = before.face != after.face && before.shades[0] != after.shades[0];

    // The clothes.
    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let before = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let clothes_moved = roll_of(&with_wizard(&mut c, |_, w| w.state.clone())).gear != before.gear;

    // The town, which is a flat roll and so can repeat.
    goto_page(&mut c, EcgProgress::Town);
    let before = with_wizard(&mut c, |_, w| w.state.start_area);
    let mut town_moved = false;
    for _ in 0..40 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        if with_wizard(&mut c, |_, w| w.state.start_area) != before {
            town_moved = true;
            break;
        }
    }

    c.assert_behaviour(
        "chargen.random.the-button-re-rolls-the-page-the-player-is-on",
        move |_| {
            people_moved
                && profession_moved
                && skills_moved
                && face_moved
                && clothes_moved
                && town_moved
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.random.on-the-last-page-it-warns-first-and-only-a-yes-re-rolls-the-whole-character
// ---------------------------------------------------------------------------------------------

/// The last page warns before throwing the whole character away, and a refusal rolls nothing.
pub(super) fn the_last_page_warns_before_re_rolling_the_whole_character() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Summary);
    let before = roll_of(&with_wizard(&mut c, |_, w| w.state.clone()));
    click_wizard(&mut c, chargen::RANDOM_BUTTON);
    let warned = with_wizard(&mut c, |_, w| w.open_dialog) == Some(CharGenDialog::RandomizeWarning);
    let nothing_yet = roll_of(&with_wizard(&mut c, |_, w| w.state.clone())) == before;

    with_wizard(&mut c, |_, w| w.close_dialog(true));
    c.tick(1);
    let re_rolled = roll_of(&with_wizard(&mut c, |_, w| w.state.clone())) != before;
    // ...and the re-roll leaves a playable character rather than a blank one: a profession is
    // chosen, its own skills are specialised, and credits were spent doing it.
    let playable = with_wizard(&mut c, |_, w| {
        let specialised = w
            .state
            .skill_levels
            .iter()
            .filter(|s| **s == dereth_chargen::SkillAdvancementClass::Specialized)
            .count();
        w.state.template >= 1
            && specialised >= 1
            && w.state.remaining_skill_credits < w.state.total_skill_credits
    });
    c.shutdown();

    // The other half: a refusal is not a re-roll.
    let mut d = a_client_on_the_wizard();
    goto_page(&mut d, EcgProgress::Summary);
    let before = roll_of(&with_wizard(&mut d, |_, w| w.state.clone()));
    click_wizard(&mut d, chargen::RANDOM_BUTTON);
    with_wizard(&mut d, |_, w| w.close_dialog(false));
    d.tick(2);
    let declined = roll_of(&with_wizard(&mut d, |_, w| w.state.clone())) == before;

    d.assert_behaviour("chargen.random.on-the-last-page-it-warns-first-and-only-a-yes-re-rolls-the-whole-character", move |_| {
        warned && nothing_yet && re_rolled && playable && declined
    });
    d.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.random.on-the-town-page-it-can-land-on-any-town-and-not-only-the-peoples-own
// ---------------------------------------------------------------------------------------------

/// The town page's own roll is flat, so it can give a player a town no people of theirs starts in.
pub(super) fn the_town_pages_roll_can_land_on_any_town() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Town);

    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        seen.insert(with_wizard(&mut c, |_, w| w.state.start_area));
    }
    let three: bool = seen == [0, 1, 2].into_iter().collect();

    c.app_mut().probe_mut().host_state_mut().account_has_tod = true;
    c.tick(1);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        click_wizard(&mut c, chargen::RANDOM_BUTTON);
        seen.insert(with_wizard(&mut c, |_, w| w.state.start_area));
    }
    let four: bool = seen == [0, 1, 2, 3].into_iter().collect();

    c.assert_behaviour(
        "chargen.random.on-the-town-page-it-can-land-on-any-town-and-not-only-the-peoples-own",
        move |_| three && four,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.tabs.the-two-insect-peoples-lose-three-pages-outright-rather-than-greyed
// ---------------------------------------------------------------------------------------------

/// Three tabs go away entirely for the two peoples that have no use for them.
pub(super) fn the_insect_peoples_lose_three_tabs_outright() {
    let mut c = a_client_on_the_wizard();
    let gone = [
        EcgProgress::Profession,
        EcgProgress::Skills,
        EcgProgress::Town,
    ];

    // Every tab is there for an ordinary people, which is what makes the reading below a change.
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    let all_there = EcgProgress::PAGES.iter().all(|p| tab_is_visible(&c, *p));

    let mut hidden_not_greyed = true;
    for heritage in [HERITAGE_OLTHOI, HERITAGE_OLTHOI_ACID] {
        let (id, _) = *chargen::HERITAGE_BUTTONS
            .iter()
            .find(|(_, h)| *h == heritage)
            .expect("both bullets are in the strip");
        click_wizard(&mut c, id);
        hidden_not_greyed &= with_wizard(&mut c, |_, w| w.state.heritage_group) == heritage;
        for p in EcgProgress::PAGES {
            let h = element(&c, p.select_button().expect("a tab"));
            let node = c
                .view()
                .expect_app()
                .ui()
                .expect("the UI shell is up")
                .ui
                .node(h)
                .expect("live");
            hidden_not_greyed &= node.region.flags.visible == !gone.contains(&p);
            // ...and a tab that is gone is not *also* greyed: one thing happens to it, not two.
            hidden_not_greyed &= !node
                .instance_properties
                .get(dereth_ui::props::attr::DISABLED)
                .is_some_and(|v| matches!(v, dereth_assets::ui::PropertyValue::Bool(true)));
        }
        click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    }

    c.assert_behaviour("chargen.tabs.the-two-insect-peoples-lose-three-pages-outright-rather-than-having-them-greyed", move |_| {
        all_there && hidden_not_greyed
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back
// ---------------------------------------------------------------------------------------------

/// The hat comes off to show the face and goes back on when the player leaves that tab -- and
/// "no hat" is an answer that survives the round trip, not an empty slot.
pub(super) fn the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);

    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let t = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables"));
    with_wizard(&mut c, |_, w| w.state.set_headgear_style(&t.chargen, 0));
    let wearing = with_wizard(&mut c, |_, w| w.state.headgear_style) == 0;

    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    let bared = with_wizard(&mut c, |_, w| (w.state.headgear_style, w.hold_headgear)) == (-1, 0);

    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let back_on = with_wizard(&mut c, |_, w| w.state.headgear_style) == 0;

    // "No hat" is a real choice and comes back as itself.
    let t = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables"));
    with_wizard(&mut c, |_, w| w.state.set_headgear_style(&t.chargen, -1));
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let no_hat_survives = with_wizard(&mut c, |_, w| w.state.headgear_style) == -1;

    // And the park is not eaten by the page being drawn again: leaving the page and coming back
    // is two whole repaints, and the hat is still there to be put back on.
    with_wizard(&mut c, |_, w| w.state.set_headgear_style(&t.chargen, 0));
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    goto_page(&mut c, EcgProgress::Town);
    goto_page(&mut c, EcgProgress::Appearance);
    let survived_the_repaints = with_wizard(&mut c, |_, w| w.hold_headgear) == 0;
    click_wizard(&mut c, chargen::appearance::TAB_CLOTHES);
    let and_came_back = with_wizard(&mut c, |_, w| w.state.headgear_style) == 0;

    c.assert_behaviour(
        "chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back",
        move |_| {
            wearing && bared && back_on && no_hat_survives && survived_the_repaints && and_came_back
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.the-colour-spots-are-the-chosen-parts-own-colours
// ---------------------------------------------------------------------------------------------

/// Nine coloured spots, each the mean of that colour's own palette, and the spare ones blank.
pub(super) fn the_colour_spots_are_the_chosen_parts_own_colours() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);

    // The pictures the wheel is built out of resolve through the shipped lookup -- which is the
    // premise, and was the whole defect: nine black holes where the colours should have been.
    let art = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables")).color_wheel_art;
    let resolved = art.bullet.0 != 0 && art.empty.0 != 0;

    let sx = sex_of(&mut c);
    let n = sx.hair_colors.len();
    assert!(n > 1, "this people offers {n} hair colours");
    let lit = n.min(9);

    let mut spots = true;
    for (i, id) in chargen::appearance::COLOR_SPOTS.iter().enumerate() {
        let (did, op) = picture_of(&c, *id);
        if i < lit {
            let want = palette_set_average(&c, dereth_primitives::DataId(sx.hair_colors[i]), 0xD0);
            spots &= did == Some(art.bullet)
                && op
                    == Some(dereth_ui::region::SurfaceOp::ReplaceColor {
                        from: dereth_ui::region::SurfaceOp::OPAQUE_BLACK,
                        to: want,
                    });
        } else {
            // Past the colours this part has: the blank picture itself, not a recolouring of one.
            spots &= did == Some(art.empty) && op.is_none();
        }
    }

    c.assert_behaviour("chargen.appearance.the-colour-spots-are-the-chosen-parts-own-colours-and-the-spare-ones-are-blank", move |_| {
        resolved && spots
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide
// ---------------------------------------------------------------------------------------------

/// The eye row is the one part that is a single colour rather than a range, and it has no shade.
pub(super) fn the_eyes_have_one_colour_each_and_no_shade_to_slide() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    // The eye row of the face tab, which the page binds to its own spinner.
    click_wizard(&mut c, ElementId(0x1000_03B0));

    let art = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables")).color_wheel_art;
    let sx = sex_of(&mut c);
    assert!(!sx.eye_colors.is_empty(), "this people offers eye colours");

    let (did, op) = picture_of(&c, chargen::appearance::COLOR_SPOTS[0]);
    let want = palette_entry(&c, dereth_primitives::DataId(sx.eye_colors[0]), 0x103);
    let one_colour = did == Some(art.bullet)
        && op
            == Some(dereth_ui::region::SurfaceOp::ReplaceColor {
                from: dereth_ui::region::SurfaceOp::OPAQUE_BLACK,
                to: want,
            });

    let (did, op) = picture_of(&c, chargen::appearance::GRAD_CIRCLE);
    let flat_disk = did == Some(art.plug) && op.is_none();
    let h = element(&c, chargen::appearance::SHADE_SCROLL);
    let no_slider = !c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .flags
        .visible;

    c.assert_behaviour(
        "chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide",
        move |_| one_colour && flat_disk && no_slider,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it
// ---------------------------------------------------------------------------------------------

/// Clicking a colour moves the marker to it and re-tints the shade wheel in that colour.
pub(super) fn clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel() {
    let mut c = a_client_on_the_wizard();
    goto_page(&mut c, EcgProgress::Appearance);
    click_wizard(&mut c, chargen::appearance::TAB_FACE);
    let art = with_wizard(&mut c, |_, w| w.tables.clone().expect("the tables")).color_wheel_art;
    let ring_resolved = art.ring.0 != 0;

    let marker_visible = |c: &HeadlessClient, i: usize| {
        let h = element(c, chargen::appearance::COLOR_POINTERS[i]);
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("live")
            .region
            .flags
            .visible
    };

    // Two colours this part really has: a click past the end is refused, and a people's list is
    // not always nine long.
    let n = with_wizard(&mut c, |_, w| w.choices[0].num_colors);
    let last = usize::try_from(n).expect("a count").min(9) - 1;
    assert!(last >= 1, "this people offers {n} hair colours");

    click_wizard(&mut c, chargen::appearance::COLOR_SPOTS[0]);
    let first_picked = with_wizard(&mut c, |_, w| w.current_color) == 0 && marker_visible(&c, 0);
    let (did, op) = picture_of(&c, chargen::appearance::GRAD_CIRCLE);
    let two = with_wizard(&mut c, |_, w| {
        w.color_wheel[0].expect("the first spot has a colour")
    });
    let tinted = did == Some(art.ring) && op == Some(dereth_ui::region::SurfaceOp::Multiply(two));

    click_wizard(&mut c, chargen::appearance::COLOR_SPOTS[last]);
    let moved = with_wizard(&mut c, |_, w| w.current_color)
        == i32::try_from(last).expect("in range")
        && marker_visible(&c, last)
        && !marker_visible(&c, 0);
    let four = with_wizard(&mut c, |_, w| {
        w.color_wheel[last].expect("the last spot has a colour")
    });
    let followed = two != four
        && picture_of(&c, chargen::appearance::GRAD_CIRCLE).1
            == Some(dereth_ui::region::SurfaceOp::Multiply(four));

    c.assert_behaviour(
        "chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it",
        move |_| ring_resolved && first_picked && tinted && moved && followed,
    );
    c.shutdown();
}
