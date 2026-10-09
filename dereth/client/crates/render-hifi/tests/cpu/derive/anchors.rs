//! Every anchored replacement finds its text exactly once in every shader it is applied to, and
//! nowhere it is not; a source that has drifted fails the derivation instead of deriving wrongly.

use dereth_render_hifi::derive::{
    self, apply, legacy_source, matches, Anchor, DeriveError, ModuleKey, Scope, Variant,
};

/// Every re-shaded module and the ordinary source it derives from.
fn reshade_modules() -> Vec<(ModuleKey, String)> {
    ModuleKey::all(Variant::Reshade)
        .map(|m| (m, legacy_source(m.format, m.splat)))
        .collect()
}

/// An anchor's identity: its name and where it applies.
fn identity(a: &Anchor) -> (&'static str, Scope) {
    (a.name, a.scope)
}

/// Behaviour: hifi.derive.every-anchor-matches-once-in-every-permutation
#[test]
fn every_anchor_matches_once_where_it_applies_and_nowhere_else() {
    let modules = reshade_modules();
    // Every anchor any permutation uses, so each permutation can be checked for the ones it
    // does not use as well.
    let mut every: Vec<Anchor> = Vec::new();
    for (m, _) in &modules {
        for a in derive::anchors(*m).expect("re-shade derives") {
            if !every
                .iter()
                .any(|b| identity(b) == identity(&a) && b.find == a.find)
            {
                every.push(a);
            }
        }
    }
    for (m, source) in &modules {
        let used = derive::anchors(*m).expect("re-shade derives");
        let mut seen = std::collections::HashSet::new();
        for a in &used {
            assert!(
                seen.insert(identity(a)),
                "{m:?}: {} in {} is applied twice",
                a.name,
                a.scope
            );
            assert_eq!(
                matches(source, a).expect("each scope is one function"),
                1,
                "{m:?}: {} in {}",
                a.name,
                a.scope
            );
        }
        for a in every
            .iter()
            .filter(|a| !used.iter().any(|u| identity(u) == identity(a)))
        {
            assert_eq!(
                matches(source, a).expect("each scope is one function"),
                0,
                "{m:?}: {} in {} matches where it is not applied",
                a.name,
                a.scope
            );
        }
        derive::derive(*m).unwrap_or_else(|e| panic!("{m:?}: {e}"));
    }
}

/// Behaviour: hifi.derive.every-anchor-matches-once-in-every-permutation
#[test]
fn a_source_with_an_anchor_doubled_or_missing_fails_the_derivation() {
    let module = ModuleKey::all(Variant::Reshade).next().expect("one module");
    let source = legacy_source(module.format, module.splat);
    // Doubling a function's body anchor inside it.
    let doubled = source.replacen(
        "    return finish(c, i.fog);\n}\n",
        "    return finish(c, i.fog);\n    return finish(c, i.fog);\n}\n",
        1,
    );
    assert_ne!(doubled, source);
    assert!(matches!(
        derive::derive_from(&doubled, module),
        Err(DeriveError::Anchor { matches: 2, .. })
    ));
    // Losing a whole-source anchor.
    let missing = source.replacen("struct VsOut {\n", "struct VsOutput {\n", 1);
    assert!(matches!(
        derive::derive_from(&missing, module),
        Err(DeriveError::Anchor { matches: 0, .. })
    ));
    // Repeating a function the anchors are scoped to.
    let twice = format!("{source}\nfn ps_modulate(i: VsOut) -> @location(0) vec4<f32> {{\n}}\n");
    assert!(matches!(
        derive::derive_from(&twice, module),
        Err(DeriveError::Scope { matches: 2, .. })
    ));
    // The anchor engine on its own: once is applied, twice is refused.
    let a = Anchor {
        name: "x",
        scope: Scope::Function("f"),
        find: "a;",
        with: "b;".to_owned(),
    };
    assert_eq!(
        apply(
            "fn f() {\n a;\n}\nfn g() {\n a;\n}\n",
            std::slice::from_ref(&a)
        ),
        Ok("fn f() {\n b;\n}\nfn g() {\n a;\n}\n".to_owned())
    );
    assert!(apply("fn f() {\n a; a;\n}\n", &[a]).is_err());
}

/// Behaviour: hifi.derive.every-anchor-matches-once-in-every-permutation
#[test]
fn every_line_the_anchors_do_not_touch_is_kept_verbatim_and_in_order() {
    for (m, source) in reshade_modules() {
        let used = derive::anchors(m).expect("re-shade derives");
        let derived = derive::derive(m).expect("derives");
        let mut rest = derived.lines();
        for line in source.lines() {
            if used
                .iter()
                .any(|a| a.find.lines().any(|f| !f.is_empty() && line.contains(f)))
            {
                continue;
            }
            assert!(
                rest.any(|d| d == line),
                "{m:?}: the ordinary line {line:?} is missing or out of order"
            );
        }
    }
}

/// Behaviour: hifi.derive.every-anchor-matches-once-in-every-permutation
#[test]
fn a_variant_without_a_derivation_says_so() {
    for v in Variant::ALL {
        let module = ModuleKey::all(v).next().expect("one module");
        match v {
            Variant::Reshade | Variant::Surface => assert!(derive::derive(module).is_ok()),
            other => assert_eq!(derive::derive(module), Err(DeriveError::NotDerived(other))),
        }
    }
}
