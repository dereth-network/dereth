//! Behaviour: none (checks source references to the panel catalogue)
//! Every panel and bound id has a production reference or an explicit explanation.
//! Fixture: the panel catalogue and a scan of production source files.

use crate::common::panel_catalogue::*;

/// Every panel spec has a production consumer or a named exception.
#[test]
fn every_panel_spec_has_a_consumer_or_an_allow_list_line() {
    let sources = production_sources();
    let allowed: BTreeMap<&str, &str> = NO_CONSUMER_YET.iter().copied().collect();
    assert_eq!(
        allowed.len(),
        NO_CONSUMER_YET.len(),
        "the allow-list names a panel twice - delete the duplicate line"
    );

    // Every allow-list line must name a real catalogue row, or it is silently protecting nothing.
    let classes: BTreeSet<&str> = all_specs().iter().map(|s| s.class).collect();
    for (class, _) in NO_CONSUMER_YET {
        assert!(
            classes.contains(class),
            "allow-list names {class}, which is not a catalogue row"
        );
    }

    let mut orphans: Vec<String> = Vec::new();
    let mut stale: Vec<String> = Vec::new();
    for spec in all_specs() {
        let ids = binding_ids(spec);
        let cw = class_witness(&sources, spec.class);
        let iw = id_witness(&sources, &ids);
        match (cw.is_some() || iw.is_some(), allowed.get(spec.class)) {
            (false, None) => orphans.push(format!(
                "  {} ({} bound ids) - no string literal \"{}\" and no id of its own in any \
                 production file",
                spec.class,
                ids.len(),
                spec.class
            )),
            (true, Some(_)) => stale.push(format!(
                "  {} - now consumed at {}",
                spec.class,
                cw.or(iw).unwrap_or_default()
            )),
            _ => {}
        }
    }

    assert!(
        stale.is_empty(),
        "these panels have a consumer and no longer need their allow-list line. **Delete the one \
         line for each** - do not reformat the block:\n{}",
        stale.join("\n")
    );
    assert!(
        orphans.is_empty(),
        "{} panel spec(s) in `panels/catalogue.rs` are a row and nothing else. A catalogue row is \
         not a panel: it proves the ids are real, not that anything builds them. Either land the \
         consumer, or add ONE line to `NO_CONSUMER_YET` in this file with the reason:\n{}",
        orphans.len(),
        orphans.join("\n")
    );
}

/// Every bound id is named or accounted for.
#[test]
fn every_bound_id_is_named_or_accounted_for() {
    let sources = production_sources();
    let named = ids_named_in_production(&sources);

    // The list must name each id once.
    let unique: BTreeSet<u32> = ID_ACCOUNTED_FOR.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        unique.len(),
        ID_ACCOUNTED_FOR.len(),
        "the id allow-list names an id twice - delete the duplicate line"
    );

    // Every entry must be a real catalogue binding, or the line protects nothing. The id-level
    // twin of the panel gate's "allow-list names X, which is not a catalogue row".
    let bound: BTreeSet<u32> = all_specs().iter().flat_map(|s| binding_ids(s)).collect();
    let phantom: Vec<String> = ID_ACCOUNTED_FOR
        .iter()
        .filter(|(id, _, _)| !bound.contains(id))
        .map(|(id, why, note)| format!("  {id:#010X} {} - {note}", why.keyword()))
        .collect();
    assert!(
        phantom.is_empty(),
        "these id allow-list entries name no binding in any catalogue spec, so nothing ever asks \
         about them and the line is documentation in the wrong file. Either add the binding to \
         `panels/catalogue.rs` or delete the line:\n{}",
        phantom.join("\n")
    );

    // **The stale rule**, the same one the panel gate applies: an allow-listed id that has since
    // gained a production consumer must lose its line, or the list quietly records a verdict about
    // an id the scan can now see for itself.
    let stale: Vec<String> = ID_ACCOUNTED_FOR
        .iter()
        .filter(|(id, _, _)| named.contains(id))
        .map(|(id, why, _)| {
            format!(
                "  {id:#010X} ({}) - now named at {}",
                why.keyword(),
                id_site(&sources, *id).unwrap_or_else(|| "?".into())
            )
        })
        .collect();
    assert!(
        stale.is_empty(),
        "these ids are now named by production code and no longer need an allow-list line. \
         **Delete the one line for each** - do not reformat the block:\n{}",
        stale.join("\n")
    );

    // And the panels that are closed must stay closed.
    let by_class: BTreeMap<&str, &PanelSpec> =
        all_specs().into_iter().map(|s| (s.class, s)).collect();
    let mut short: Vec<String> = Vec::new();
    for class in FULLY_ACCOUNTED {
        let spec = by_class.get(class).unwrap_or_else(|| {
            panic!("FULLY_ACCOUNTED names {class}, which is not a catalogue row")
        });
        let missing: Vec<String> = binding_ids(spec)
            .into_iter()
            .filter(|id| !named.contains(id) && !unique.contains(id))
            .map(|id| format!("{id:#010X}"))
            .collect();
        if !missing.is_empty() {
            short.push(format!(
                "  {class}: {} of {} ids neither named nor accounted for: {}",
                missing.len(),
                binding_ids(spec).len(),
                missing.join(", ")
            ));
        }
    }
    assert!(
        short.is_empty(),
        "{} panel(s) on `FULLY_ACCOUNTED` have a binding that nothing names and nothing explains. \
         Either land the consumer, or check the id against retail and add ONE line to \
         `ID_ACCOUNTED_FOR` with the reading and the station that pins it:\n{}",
        short.len(),
        short.join("\n")
    );
}
