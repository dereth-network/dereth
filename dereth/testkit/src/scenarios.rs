//! Scenario declarations shared by the compiled census and the test runner.

/// Declare a scope's scenarios and their tests together.
///
/// Each entry gives an explicit test identifier, a body identifier and its claims:
/// `scenario_draws => draws ["chat.line.draws"],`. Bodies stay ordinary functions.
/// Entry attributes belong to the test; `cfg` also gates both census slices.
/// Conditional attributes support `cfg_attr(condition, ignore)` with an optional
/// reason. Use direct `cfg` for declaration gates; other `cfg_attr` forms are rejected.
/// The invocation emits `ALL` and `SCENARIOS` in its current module, so nested
/// modules retain their test paths. Every wrapper uses the assertion recorder.
///
/// Conditional gates must use direct `cfg` so the census and test cannot diverge:
///
/// ```compile_fail
/// fn gated() {}
/// dereth_testkit::scenarios! {
///     #[cfg_attr(all(), cfg(any()))]
///     scenario_gated => gated ["frame.log.is-the-counters-and-keeps-them-whole"],
/// }
/// ```
#[macro_export]
macro_rules! scenarios {
    (@entries [$($all:tt)*] [$($names:tt)*] [$($tests:tt)*]) => {
        pub static ALL: &[$crate::behaviours::Scenario] = &[$($all)*];
        pub const SCENARIOS: $crate::tier_census::ScenarioFile =
            $crate::tier_census::ScenarioFile {
                module: module_path!(),
                scenarios: ALL,
                tests: &[$($names)*],
            };
        $($tests)*
    };
    (@entries [$($all:tt)*] [$($names:tt)*] [$($tests:tt)*]
        $(#[$($attr:tt)*])* $test:ident => $body:ident [$($claim:literal),* $(,)?],
        $($rest:tt)*) => {
        $crate::scenarios!(@attrs
            [$($all)*] [$($names)*] [$($tests)*]
            [$(#[$($attr)*])*] [] [$(#[$($attr)*])*]
            $test $body [$($claim),*] [$($rest)*]);
    };
    (@attrs [$($all:tt)*] [$($names:tt)*] [$($tests:tt)*]
        [#[cfg($($cfg:tt)*)] $($attrs:tt)*] [$($gates:tt)*] [$($original:tt)*]
        $test:ident $body:ident [$($claim:literal),*] [$($rest:tt)*]) => {
        $crate::scenarios!(@attrs [$($all)*] [$($names)*] [$($tests)*]
            [$($attrs)*] [$($gates)* #[cfg($($cfg)*)]] [$($original)*]
            $test $body [$($claim),*] [$($rest)*]);
    };
    (@attrs [$($all:tt)*] [$($names:tt)*] [$($tests:tt)*]
        [#[cfg_attr($condition:meta, ignore $(= $reason:literal)?)] $($attrs:tt)*]
        [$($gates:tt)*] [$($original:tt)*]
        $test:ident $body:ident [$($claim:literal),*] [$($rest:tt)*]) => {
        $crate::scenarios!(@attrs [$($all)*] [$($names)*] [$($tests)*]
            [$($attrs)*] [$($gates)*] [$($original)*]
            $test $body [$($claim),*] [$($rest)*]);
    };
    (@attrs [$($all:tt)*] [$($names:tt)*] [$($tests:tt)*]
        [#[cfg_attr($($conditional:tt)*)] $($attrs:tt)*] $($rest:tt)*) => {
        compile_error!("scenarios! supports cfg_attr only for ignore; use cfg to gate an entry");
    };
    (@attrs [$($all:tt)*] [$($names:tt)*] [$($tests:tt)*]
        [#[$($attr:tt)*] $($attrs:tt)*] [$($gates:tt)*] [$($original:tt)*]
        $test:ident $body:ident [$($claim:literal),*] [$($rest:tt)*]) => {
        $crate::scenarios!(@attrs [$($all)*] [$($names)*] [$($tests)*]
            [$($attrs)*] [$($gates)*] [$($original)*]
            $test $body [$($claim),*] [$($rest)*]);
    };
    (@attrs [$($all:tt)*] [$($names:tt)*] [$($tests:tt)*]
        [] [$($gates:tt)*] [$($original:tt)*]
        $test:ident $body:ident [$($claim:literal),*] [$($rest:tt)*]) => {
        $crate::scenarios!(@entries
            [$($all)* $($gates)* (stringify!($body), &[$($claim),*], $body),]
            [$($names)* $($gates)* stringify!($test),]
            [$($tests)*
                $($original)*
                $(#[doc = concat!("Behaviour: ", $claim)])*
                #[test]
                fn $test() {
                    $crate::behaviours::run_scenario(ALL, stringify!($body));
                }
            ]
            $($rest)*);
    };
    ($($entries:tt)*) => {
        $crate::scenarios!(@entries [] [] [] $($entries)*);
    };
}

#[cfg(test)]
mod tests {
    mod nested {
        fn recorded() {
            crate::behaviours::note_asserted("frame.log.is-the-counters-and-keeps-them-whole");
        }

        fn mismatched() {}

        fn ignored() {
            recorded();
        }

        crate::scenarios! {
            scenario_recorded => recorded ["frame.log.is-the-counters-and-keeps-them-whole"],
            #[cfg(any())]
            scenario_absent => no_such_body ["frame.log.is-the-counters-and-keeps-them-whole"],
            #[should_panic(expected = "declares")]
            scenario_mismatched => mismatched ["frame.log.is-the-counters-and-keeps-them-whole"],
            #[cfg_attr(all(), ignore = "proves the wrapper retains the entry's ignore reason")]
            scenario_ignored => ignored ["frame.log.is-the-counters-and-keeps-them-whole"],
        }
    }

    #[test]
    fn compiled_declarations_share_the_wrappers_cfg_and_nested_identity() {
        assert_eq!(nested::ALL.len(), 3);
        assert_eq!(
            nested::SCENARIOS.tests,
            [
                "scenario_recorded",
                "scenario_mismatched",
                "scenario_ignored"
            ]
        );
        assert!(nested::SCENARIOS
            .module
            .ends_with("::scenarios::tests::nested"));
    }
}
