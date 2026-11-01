//! Where tests find the retail data files (the `test-support` feature; test builds only).
//!
//! The tests read one set of data files, the most complete one known of the end of retail: the data
//! files as retail left them in January 2017, which the September 2013 client reads. It is wherever
//! **`AUBEREAN_TEST_DAT_DIR`** says and nowhere else: the workspace knows no location of its own.
//! Set it in the environment, or once for every build under a directory in a Cargo configuration
//! file there (`[env]`, with `relative = true` for a path relative to that directory).
//!
//! A test that needs the set fails, naming the variable to set, when it is not found
//! ([`require_install`]): a test that read nothing never passes.

use std::path::PathBuf;

use crate::locate::holds_install;

/// The variable naming the end-of-retail set's folder.
pub const DAT_DIR_VAR: &str = "AUBEREAN_TEST_DAT_DIR";

/// The end-of-retail set (January 2017).
pub const END_OF_RETAIL: &str = "end-of-retail";
/// The set the September 2013 client reads: the target.
pub const TARGET: &str = END_OF_RETAIL;
/// The sets every data claim is checked against. Each must be found.
pub const CHECKED: [&str; 1] = [END_OF_RETAIL];

/// The variable naming a set's folder, if `set` is one of [`CHECKED`].
#[must_use]
pub fn variable(set: &str) -> Option<&'static str> {
    match set {
        END_OF_RETAIL => Some(DAT_DIR_VAR),
        _ => None,
    }
}

/// Where a set is looked for: its variable's folder, when the variable is set.
#[must_use]
pub fn candidate(set: &str) -> Option<PathBuf> {
    std::env::var_os(variable(set)?)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The folder of one set, if it is found.
#[must_use]
pub fn install(set: &str) -> Option<PathBuf> {
    candidate(set).filter(|d| holds_install(d))
}

/// Why a set cannot be read, as the line a failed test prints, or `None` when it is found.
#[must_use]
pub fn shortfall(set: &str) -> Option<String> {
    let Some(var) = variable(set) else {
        return Some(format!(
            "no Asheron's Call 2 data set is called {set:?} (the sets are {CHECKED:?})"
        ));
    };
    match candidate(set) {
        Some(d) if holds_install(&d) => None,
        Some(d) => Some(format!(
            "the {set} data set of Asheron's Call 2 was not found: no portal.dat in {} ({var})",
            d.display()
        )),
        None => Some(format!(
            "the {set} data set of Asheron's Call 2 was not found: {var} is unset (set it to the \
             folder of that set's data files)"
        )),
    }
}

/// The folder of a set.
///
/// # Panics
///
/// With [`shortfall`]'s line when the set is not found.
#[must_use]
pub fn require_install(set: &str) -> PathBuf {
    if let Some(msg) = shortfall(set) {
        panic!("{msg}");
    }
    install(set).unwrap_or_default()
}

/// The target set's folder: the end-of-retail data files.
///
/// # Panics
///
/// With [`shortfall`]'s line when the set is not found.
#[must_use]
pub fn require_target_install() -> PathBuf {
    require_install(TARGET)
}
