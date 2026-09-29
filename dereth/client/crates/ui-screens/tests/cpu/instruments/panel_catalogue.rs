//! Behaviour: none (prints panel and element-id source references)
//! Fixture: the panel catalogue and production source files.

use crate::common::panel_catalogue::*;

/// Prints the whole matrix. It asserts only that the instrument could look; the gate is below.
#[test]
#[ignore = "instrument: prints panel and element-id source references"]
fn print_panel_catalogue_consumers() {
    let sources = production_sources();
    println!(
        "scanned {} production .rs files under core and dereth",
        sources.len()
    );

    let mut by_class: BTreeMap<&str, (Option<String>, Option<String>)> = BTreeMap::new();
    for spec in all_specs() {
        let ids = binding_ids(spec);
        let row = (
            class_witness(&sources, spec.class),
            id_witness(&sources, &ids),
        );
        by_class.insert(spec.class, row);
    }

    println!("\n{:<24} {:<28} best id witness", "panel", "class witness");
    for (class, (cw, iw)) in &by_class {
        println!(
            "{class:<24} {:<28} {}",
            cw.clone().unwrap_or_else(|| "-".into()),
            iw.clone().unwrap_or_else(|| "-".into())
        );
    }

    let none: Vec<&str> = by_class
        .iter()
        .filter(|(_, (c, i))| c.is_none() && i.is_none())
        .map(|(c, _)| *c)
        .collect();
    let weak: Vec<&str> = by_class
        .iter()
        .filter(|(_, (c, i))| c.is_none() && i.is_some())
        .map(|(c, _)| *c)
        .collect();
    println!(
        "\n{} specs, {} with no consumer at all: {none:?}",
        by_class.len(),
        none.len()
    );
    println!(
        "{} carried by the weak `id` signal only: {weak:?}",
        weak.len()
    );

    let named = ids_named_in_production(&sources);
    let accounted: BTreeMap<u32, Why> = ID_ACCOUNTED_FOR
        .iter()
        .map(|(id, why, _)| (*id, *why))
        .collect();

    println!(
        "\n{:<24} {:>5} {:>6} {:>10} {:>12}",
        "panel", "ids", "named", "accounted", "unexplained"
    );
    let mut totals = (0usize, 0usize, 0usize, 0usize);
    for spec in all_specs() {
        let ids = binding_ids(spec);
        if ids.is_empty() {
            continue;
        }
        let n = ids.iter().filter(|i| named.contains(i)).count();
        let a = ids
            .iter()
            .filter(|i| !named.contains(i) && accounted.contains_key(i))
            .count();
        let u = ids.len() - n - a;
        totals.0 += ids.len();
        totals.1 += n;
        totals.2 += a;
        totals.3 += u;
        let mark = if u == 0 {
            "  <- fully accounted for"
        } else {
            ""
        };
        println!(
            "{:<24} {:>5} {:>6} {:>10} {:>12}{mark}",
            spec.class,
            ids.len(),
            n,
            a,
            u
        );
    }
    println!(
        "{:<24} {:>5} {:>6} {:>10} {:>12}",
        "TOTAL", totals.0, totals.1, totals.2, totals.3
    );

    println!(
        "\nid-level allow-list ({} entries):",
        ID_ACCOUNTED_FOR.len()
    );
    for (id, why, note) in ID_ACCOUNTED_FOR {
        println!("  {:#010X}  {:<6}  {note}", id, why.keyword());
    }

    // **The instrument must be able to look.** Every "NONE" above is an absence of evidence and
    // not evidence of absence until both signals are shown to fire on a panel known to be wired.
    // Positive controls, one per signal, and a negative control:
    let (vitals_class, _) = &by_class["VitalsPanel"];
    assert!(
        vitals_class.is_some(),
        "positive control: `catalogue::spec(\"VitalsPanel\")` is called from \
         `screens/gameplay.rs`'s production `post_init`. If the label signal cannot see that, it \
         cannot see anything."
    );
    let (_, spellbook_id) = &by_class["SpellbookPanel"];
    assert!(
        spellbook_id
            .as_deref()
            .is_some_and(|w| w.contains("spellbook.rs")),
        "positive control: `panels/spellbook.rs` writes SpellbookPanel's own element ids; got {:?}",
        spellbook_id
    );
    assert!(
        class_witness(&sources, "MissingPanel").is_none(),
        "negative control: a behavior label nothing writes must not match"
    );
}
