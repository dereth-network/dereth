//! The degrade selector's negative-bias arm reads `min_dist` and `ideal_dist` (the non-negative arm
//! reads `ideal_dist` and `max_dist`), checked over every shipped `GfxObjDegradeInfo` record at
//! interior and end biases, with each distance probed between the right and the wrong threshold.
//! The oracle walks each record as five flat words (`gfxobj_id`, `degrade_mode`, `min_dist`,
//! `ideal_dist`, `max_dist`) rather than by field name, and `dereth-animation`'s and
//! `dereth-world-render`'s selectors are both asserted against it.
//! Fixture: `client_portal.dat` (a missing install fails).

use std::collections::BTreeSet;

use dereth_animation::parts::{get_degrade, DegradeSettings};
use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_world_data::anim_convert as convert;
use dereth_world_render::objects::degrade::{get_degrade as wr_get_degrade, DegradeGlobals};

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "no retail dats under {} -- set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    })
}

/// One shipped `GfxObjInfo` as the five 4-byte words the client's pointer walks.
///
/// `[0] gfxobj_id  [1] degrade_mode  [2] min_dist  [3] ideal_dist  [4] max_dist`
type Words = [f32; 5];

const MIN: usize = 2;
const IDEAL: usize = 3;
const MAX: usize = 4;

fn words(g: &dereth_assets::motion::GfxObjInfo) -> Words {
    [0.0, 0.0, g.min_dist, g.ideal_dist, g.max_dist]
}

/// ORACLE — the retail selector as a walk over flat five-word records.
///
/// For a non-negative bias the threshold per level is `ideal - (ideal - max) * bias`; for a
/// negative bias it is `(ideal - min) * bias + ideal`. `base` is the word each arm starts on
/// (`max_dist` or `ideal_dist`), and the other word is the one before it. Everything else is the
/// same loop.
///
/// The `d` clamp is `max(|distance| - degrade_distance, 0)`.
fn oracle(levels: &[Words], dist: f32, degrade_distance: f32, bias: f32) -> usize {
    let d = (dist.abs() - degrade_distance).max(0.0);
    let base = if bias >= 0.0 { MAX } else { IDEAL };
    for (i, l) in levels.iter().enumerate() {
        let back = l[base - 1];
        let here = l[base];
        let threshold = if bias >= 0.0 {
            back - (back - here) * bias
        } else {
            (here - back) * bias + here
        };
        if d < threshold {
            return i;
        }
    }
    levels.len().saturating_sub(1)
}

/// The wrong reading of the negative arm: the **positive** arm's pair of fields, with `max_dist`
/// where the client adds `ideal_dist`. Kept so the census below can say how much of the shipped
/// data can tell the two apart.
fn negative_arm_with_maximum_origin(
    levels: &[Words],
    dist: f32,
    degrade_distance: f32,
    bias: f32,
) -> usize {
    let d = (dist.abs() - degrade_distance).max(0.0);
    for (i, l) in levels.iter().enumerate() {
        let threshold = if bias >= 0.0 {
            l[IDEAL] - (l[IDEAL] - l[MAX]) * bias
        } else {
            (l[MAX] - l[IDEAL]) * bias + l[MAX]
        };
        if d < threshold {
            return i;
        }
    }
    levels.len().saturating_sub(1)
}

/// Interior biases, not only the ends. The client clamps `deg_mul` into
/// `[-1, +1]`, so this is the reachable range of the automatic bias.
const BIASES: [f32; 13] = [
    -1.0, -0.875, -0.75, -0.625, -0.5, -0.375, -0.25, -0.125, -0.001, 0.0, 0.25, 0.5, 1.0,
];

/// Every `GfxObjDegradeInfo` in `client_portal.dat`, decoded once.
fn all_records(store: &RetailDatStore) -> Vec<(DataId, GfxObjDegradeInfo)> {
    store
        .ids_of(DbType::DegradeInfo)
        .into_iter()
        .map(|id| {
            let bytes = store
                .read_typed(DbType::DegradeInfo, id)
                .expect("degrade record reads");
            let rec = GfxObjDegradeInfo::decode_payload(id, &bytes).expect("it decodes");
            (id, rec)
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 1. The denominator
// ---------------------------------------------------------------------------------------------

/// The records the sweep walks: the graphics objects that carry a degrade record, and the distinct
/// `GfxObjDegradeInfo` objects they name.
#[test]
fn the_degrade_sweep_reads_every_shipped_degrade_record() {
    let store = store();
    let gfxobjs = store.ids_of(DbType::GfxObj);
    assert_eq!(gfxobjs.len(), 15_318, "GfxObjs in client_portal.dat");

    let mut with_record = 0usize;
    let mut named: BTreeSet<u32> = BTreeSet::new();
    for id in gfxobjs {
        let bytes = store.read_typed(DbType::GfxObj, id).expect("gfxobj reads");
        let obj = GfxObj::decode_payload(id, &bytes).expect("it decodes");
        if let Some(did) = obj.did_degrade {
            with_record += 1;
            named.insert(did.0);
        }
    }
    assert_eq!(with_record, 4_131, "GfxObjs carrying a GfxObjDegradeInfo");

    let records = all_records(&store);
    assert!(!records.is_empty(), "the DegradeInfo space is not empty");
    let levels: usize = records.iter().map(|(_, r)| r.degrades.len()).sum();
    eprintln!(
        "degrade corpus: {} GfxObjs, {with_record} with a degrade record naming {} distinct \
         GfxObjDegradeInfos; the DegradeInfo space holds {} records / {levels} levels",
        15_318,
        named.len(),
        records.len(),
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The acceptance: both arms, both signs, against the retail expression
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.degrade.a-negative-bias-reads-the-min-and-ideal-distances
/// Oracle: [`oracle`], written from the selector's pointer walk and `GfxObjInfo`'s offsets.
///
/// Sweeps every shipped record at every bias in [`BIASES`] over distances derived **from that
/// record's own thresholds**, probing just inside, just outside, and — the part that matters —
/// **between** the correct threshold and the wrong one, which is the only interval in which
/// the right and the wrong reading can name different levels.
#[test]
fn every_shipped_record_matches_the_retail_expression_on_both_signs_of_the_bias() {
    let store = store();
    let records = all_records(&store);
    assert!(
        records.len() > 1_000,
        "a corpus, not a handful: {}",
        records.len()
    );

    let dd = dereth_animation::parts::degrade::S_R_DEGRADE_DISTANCE;
    assert_eq!(dd, 50.0, "the default part-degrade distance is 50 metres");

    let mut checked = 0usize;
    let mut probes_past_the_radius = 0usize;
    for (id, rec) in &records {
        let flat: Vec<Words> = rec.degrades.iter().map(words).collect();
        let mine = convert::degrade_info(rec);
        assert_eq!(
            mine.degrades.len(),
            flat.len(),
            "{id:?}: convert dropped a level"
        );

        for &bias in &BIASES {
            // Distances built from this record's own numbers, at both readings' thresholds and
            // between them. `+ dd` because the loops see `dist - 50`.
            let mut probes: Vec<f32> = vec![0.0, dd, dd + 0.5];
            for l in &flat {
                let right = if bias >= 0.0 {
                    l[IDEAL] - (l[IDEAL] - l[MAX]) * bias
                } else {
                    (l[IDEAL] - l[MIN]) * bias + l[IDEAL]
                };
                let wrong = if bias >= 0.0 {
                    right
                } else {
                    (l[MAX] - l[IDEAL]) * bias + l[MAX]
                };
                for t in [right, wrong] {
                    if t.is_finite() && t.abs() < 1.0e7 {
                        probes.push(dd + t - 0.05);
                        probes.push(dd + t + 0.05);
                    }
                }
                if right.is_finite() && wrong.is_finite() && (right - wrong).abs() > 0.2 {
                    probes.push(dd + f32::midpoint(right, wrong));
                }
            }

            for dist in probes {
                if !dist.is_finite() {
                    continue;
                }
                if dist > dd {
                    probes_past_the_radius += 1;
                }
                let want = oracle(&flat, dist, dd, bias);
                let s = DegradeSettings {
                    degrade_distance: dd,
                    bias,
                    ..DegradeSettings::default()
                };
                let (got, mode) = get_degrade(&mine, dist, s);
                assert_eq!(
                    got as usize, want,
                    "{id:?} at bias {bias}, distance {dist}: dereth-animation said level {got}, \
                     get_degrade says {want}"
                );
                assert_eq!(
                    mode, rec.degrades[want].degrade_mode,
                    "{id:?} at bias {bias}, distance {dist}: the mode is the selected level's"
                );
                checked += 1;
            }
        }
    }

    // A sweep that never left the near clamp would assert nothing about either loop.
    assert!(
        probes_past_the_radius > 100_000,
        "only {probes_past_the_radius} probes were past the degrade distance"
    );
    eprintln!(
        "{checked} (record, bias, distance) comparisons against get_degrade \
         over {} records and {} biases, none disagreeing ({probes_past_the_radius} past the \
         50 m clamp)",
        records.len(),
        BIASES.len(),
    );
}

/// The two crates that carry this transcription agree over the same sweep.
///
/// `dereth_world_render`'s copy is the one `dereth-client` actually calls; this crate's is the one the
/// defect was in. They decode from different types and are asserted here against the *same*
/// oracle, not against each other, so "they agree" cannot mean "they are wrong together".
///
/// **One deliberate divergence, and it is `dereth_world_render`'s:** its `get_degrade` clamps the
/// bias into `[-1, +1]`. The client's degrade selector does not clamp — the level calculation
/// clamps `deg_mul` before storing it, but the user-supplied degrade bias is a raw
/// graphics-performance user preference and reaches the function unclamped. Inside
/// `[-1, +1]` the two are identical, which is the range this sweep uses; outside it they part, and
/// that belongs to whoever owns `objects::degrade`.
#[test]
fn the_two_crates_transcriptions_agree_over_the_whole_corpus() {
    let store = store();
    let records = all_records(&store);
    let dd = dereth_animation::parts::degrade::S_R_DEGRADE_DISTANCE;

    let mut checked = 0usize;
    let mut both_arms = [0usize; 2];
    for (id, rec) in &records {
        let flat: Vec<Words> = rec.degrades.iter().map(words).collect();
        let mine = convert::degrade_info(rec);
        for &bias in &BIASES {
            assert!(
                (-1.0..=1.0).contains(&bias),
                "the sweep stays inside the clamped range"
            );
            both_arms[usize::from(bias >= 0.0)] += 1;
            for step in 0..24 {
                // A ladder over the record's own range rather than a taste: 0 to twice the
                // furthest max_dist it names, plus the 50 m clamp.
                let far = flat
                    .iter()
                    .map(|l| l[MAX])
                    .filter(|v| v.is_finite() && *v < 1.0e6)
                    .fold(100.0_f32, f32::max);
                #[allow(clippy::cast_precision_loss)]
                let dist = dd + far * 2.0 * (step as f32) / 23.0;

                let want = oracle(&flat, dist, dd, bias);
                let s = DegradeSettings {
                    degrade_distance: dd,
                    bias,
                    ..DegradeSettings::default()
                };
                let (anim, _) = get_degrade(&mine, dist, s);
                let g = DegradeGlobals {
                    degrade_distance: dd,
                    auto_update_deg_mul: true,
                    deg_mul: bias,
                    ..DegradeGlobals::default()
                };
                let (wr, _) = wr_get_degrade(rec, dist, &g);
                assert_eq!(
                    anim as usize, want,
                    "{id:?} bias {bias} dist {dist}: dereth-animation"
                );
                assert_eq!(
                    wr, want,
                    "{id:?} bias {bias} dist {dist}: dereth-world-render"
                );
                checked += 1;
            }
        }
    }
    assert!(
        both_arms[0] > 0 && both_arms[1] > 0,
        "both arms were exercised: {both_arms:?}"
    );
    eprintln!(
        "{checked} three-way comparisons (oracle / dereth-animation / dereth-world-render) over {} \
         records; negative-bias sweeps {} and non-negative {}",
        records.len(),
        both_arms[0],
        both_arms[1],
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The third reading of a survivor: how much of the shipped data could ever have seen this
// ---------------------------------------------------------------------------------------------

/// **The bias is renderer-wide, not a field of a record**, so the question "how many of the
/// 4,131 carry a negative bias" has the answer **zero, by construction** — no record carries one,
/// and asserting that is the point rather than a dodge. What the corpus *can* answer is how many
/// records the wrong reading could ever move, i.e. for how many does that expression name
/// a different level than the client at some reachable negative bias and some distance.
///
/// A record can only be moved when its own numbers separate the two readings, so this is a
/// property of the shipped data and it is measured here rather than argued.
#[test]
fn the_census_of_records_the_defect_could_ever_have_moved() {
    let store = store();
    let records = all_records(&store);
    let dd = dereth_animation::parts::degrade::S_R_DEGRADE_DISTANCE;

    let mut movable = 0usize;
    let mut identical_triples = 0usize;
    let mut multi_level = 0usize;
    let mut worst: Option<(DataId, f32, f32, usize, usize)> = None;
    for (id, rec) in &records {
        let flat: Vec<Words> = rec.degrades.iter().map(words).collect();
        if flat.len() > 1 {
            multi_level += 1;
        }
        if flat
            .iter()
            .all(|l| l[MIN] == l[IDEAL] && l[IDEAL] == l[MAX])
        {
            // min == ideal == max on every level: the two expressions are the same function, so
            // this record is blind to the defect at every bias and every distance.
            identical_triples += 1;
        }
        let mut moved = false;
        for &bias in BIASES.iter().filter(|b| **b < 0.0) {
            let mut probes: Vec<f32> = Vec::new();
            for l in &flat {
                let right = (l[IDEAL] - l[MIN]) * bias + l[IDEAL];
                let wrong = (l[MAX] - l[IDEAL]) * bias + l[MAX];
                for t in [right, wrong] {
                    if t.is_finite() && t.abs() < 1.0e7 {
                        probes.push(dd + t - 0.05);
                        probes.push(dd + t + 0.05);
                    }
                }
                if right.is_finite() && wrong.is_finite() {
                    probes.push(dd + f32::midpoint(right, wrong));
                }
            }
            for dist in probes.into_iter().filter(|d| d.is_finite() && *d >= 0.0) {
                let right = oracle(&flat, dist, dd, bias);
                let wrong = negative_arm_with_maximum_origin(&flat, dist, dd, bias);
                if right != wrong {
                    moved = true;
                    if worst.is_none() {
                        worst = Some((*id, bias, dist, right, wrong));
                    }
                }
            }
        }
        if moved {
            movable += 1;
        }
    }

    // Calibration: the census instrument must be able to produce a non-zero. A zero here and a
    // zero from a broken loop are the same output otherwise.
    assert!(
        movable > 0,
        "the census found no record the wrong reading could move -- which would mean either that the \
         shipped data cannot tell the two readings apart, or that this loop does not look"
    );
    let (wid, wbias, wdist, wright, wwrong) = worst.expect("a witness, since movable > 0");
    eprintln!(
        "falsifiability census over {} shipped GfxObjDegradeInfo records: {movable} could \
         be moved by the wrong negative arm at some bias in [-1, 0) and some distance; \
         {identical_triples} have min == ideal == max on every level and are blind to it at every \
         bias; {multi_level} carry more than one level. Witness: {wid:?} at bias {wbias}, \
            distance {wdist} m -- the client says level {wright}, the wrong expression said \
         {wwrong}.",
        records.len(),
    );
    assert!(movable <= records.len());
}

/// The instrument that says the census above is not measuring nothing: a record whose numbers are
/// known to separate the two readings must come back *moved*, and a record with
/// `min == ideal == max` on every level must come back *not moved*. Both directions: an instrument
/// that could only ever read one way would pass while measuring nothing.
#[test]
fn the_falsifiability_census_reads_non_zero_on_a_known_positive_and_zero_on_a_known_negative() {
    let dd = dereth_animation::parts::degrade::S_R_DEGRADE_DISTANCE;
    let bias = -0.5_f32;

    // Known positive: min 10, ideal 40, max 100 then a far second level. Correct threshold
    // 40 + (40-10)*(-0.5) = 25; the wrong reading's threshold 100 + (100-40)*(-0.5) = 70. Anything in
    // (25, 70) is level 1 under the client and level 0 under the old reading.
    let sep: Vec<Words> = vec![
        [0.0, 0.0, 10.0, 40.0, 100.0],
        [0.0, 0.0, 200.0, 300.0, 400.0],
    ];
    assert_eq!(
        oracle(&sep, dd + 50.0, dd, bias),
        1,
        "the client degrades at 50 m past the clamp"
    );
    assert_eq!(
        negative_arm_with_maximum_origin(&sep, dd + 50.0, dd, bias),
        0,
        "the old reading did not"
    );

    // Known negative: min == ideal == max on every level, so the two expressions coincide.
    let flat: Vec<Words> = vec![[0.0, 0.0, 25.0, 25.0, 25.0], [0.0, 0.0, 90.0, 90.0, 90.0]];
    for step in 0..200 {
        #[allow(clippy::cast_precision_loss)]
        let dist = dd + (step as f32);
        assert_eq!(
            oracle(&flat, dist, dd, bias),
            negative_arm_with_maximum_origin(&flat, dist, dd, bias),
            "a record with min == ideal == max cannot tell the two readings apart, at {dist}"
        );
    }
}
