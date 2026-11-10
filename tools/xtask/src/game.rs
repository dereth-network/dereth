//! `--game dereth|auberean|all`: which game's crates a tier covers or a build makes.
//!
//! The workspace holds two games' products on one engine. Asheron's Call's are Dereth, Empyrean and
//! Lyceum, with the shared crates and the tools; Asheron's Call 2's are Auberean, Yalain and
//! Athenaeum (the `auberean-*`, `yalain`, `yalain-*` and `athenaeum-*` packages). Every tier
//! (`check`, `ci tier0`, `ci tier1`, `ci tier2`, `gates`) and `build` cover both by default; `--game dereth`
//! (Asheron's Call) or `--game auberean` (Asheron's Call 2) limits one to a game. A crate belongs to exactly one of the two, so the two filters
//! together cover what the default covers.
//!
//! The rules that read the whole repository in seconds (formatting, the lints, the seams, the
//! source rules) run whatever the filter: they hold the tree, not a game.

use crate::test_lint::is_ac2_package;

/// The game a tier covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Game {
    /// Both games: the default.
    All,
    /// Asheron's Call: every crate that is not Asheron's Call 2's.
    Dereth,
    /// Asheron's Call 2: the `auberean-*`, `yalain`, `yalain-*` and `athenaeum-*` crates.
    Auberean,
}

/// The flag.
pub const FLAG: &str = "--game";

/// Asheron's Call 2's packages as cargo patterns, for `-p` and `--exclude`: the same set
/// [`is_ac2_package`] accepts (a test holds the two together over the workspace).
pub const AC2_PATTERNS: &[&str] = &["auberean-*", "yalain", "yalain-*", "athenaeum-*"];

impl Game {
    /// `--game <dereth|auberean|all>` (or `--game=<...>`) anywhere on the command line; [`Game::All`] when
    /// it is not given. A value that names no game, or a flag with none, is an error rather than
    /// a run of both.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        Ok(Self::given(args)?.unwrap_or(Game::All))
    }

    /// The game `--game` names, or `None` when the flag is not on the command line.
    pub fn given(args: &[String]) -> Result<Option<Self>, String> {
        let mut found = None;
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let value = if arg == FLAG {
                it.next()
                    .ok_or_else(|| format!("{FLAG} needs a value: dereth, auberean or all"))?
            } else if let Some(v) = arg.strip_prefix("--game=") {
                v
            } else {
                continue;
            };
            let game = Self::parse(value)?;
            if found.is_some_and(|g| g != game) {
                return Err(format!("{FLAG} is given twice with different games"));
            }
            found = Some(game);
        }
        Ok(found)
    }

    /// The command line without `--game` and its value: what a command passes on to cargo.
    pub fn without_flag(args: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if arg == FLAG {
                it.next();
            } else if !arg.starts_with("--game=") {
                out.push(arg.clone());
            }
        }
        out
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "dereth" => Ok(Game::Dereth),
            "auberean" => Ok(Game::Auberean),
            "all" => Ok(Game::All),
            other => Err(format!(
                "{FLAG} takes dereth, auberean or all, not {other:?}"
            )),
        }
    }

    /// The word `--game` takes for it.
    pub fn name(self) -> &'static str {
        match self {
            Game::All => "all",
            Game::Dereth => "dereth",
            Game::Auberean => "auberean",
        }
    }

    /// Whether Asheron's Call's crates are covered.
    pub fn dereth(self) -> bool {
        matches!(self, Game::All | Game::Dereth)
    }

    /// Whether Asheron's Call 2's crates are covered.
    pub fn auberean(self) -> bool {
        matches!(self, Game::All | Game::Auberean)
    }

    /// Whether the package `name` is covered.
    pub fn covers(self, name: &str) -> bool {
        if is_ac2_package(name) {
            self.auberean()
        } else {
            self.dereth()
        }
    }

    /// The cargo arguments that select the covered crates of the workspace: `--workspace`, the
    /// workspace without Asheron's Call 2's, or Asheron's Call 2's alone.
    pub fn selection(self) -> Vec<&'static str> {
        match self {
            Game::All => vec!["--workspace"],
            Game::Dereth => {
                let mut argv = vec!["--workspace"];
                argv.extend(AC2_PATTERNS.iter().flat_map(|p| ["--exclude", *p]));
                argv
            }
            Game::Auberean => AC2_PATTERNS.iter().flat_map(|p| ["-p", *p]).collect(),
        }
    }

    /// What a row's command covers, after the command: `--workspace` for both games, else the
    /// filter as typed.
    pub fn scope(self) -> &'static str {
        match self {
            Game::All => "--workspace",
            Game::Dereth => "(--game dereth)",
            Game::Auberean => "(--game auberean)",
        }
    }

    /// The end of a table's title: nothing for both games, else the one game.
    pub fn title_suffix(self) -> &'static str {
        match self {
            Game::All => "",
            Game::Dereth => " -- Asheron's Call only (--game dereth)",
            Game::Auberean => " -- Asheron's Call 2 only (--game auberean)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_flag_and_its_value_are_taken_off_what_goes_to_cargo() {
        assert_eq!(
            Game::without_flag(&argv(&["--game", "auberean", "--release"])),
            argv(&["--release"])
        );
        assert_eq!(
            Game::without_flag(&argv(&["--release", "--game=dereth", "--locked"])),
            argv(&["--release", "--locked"])
        );
        assert_eq!(
            Game::without_flag(&argv(&["--release"])),
            argv(&["--release"])
        );
    }

    /// Whether `name` matches a cargo package pattern of the shapes [`AC2_PATTERNS`] uses: a
    /// whole name, or a prefix and `*`.
    fn matches(pattern: &str, name: &str) -> bool {
        match pattern.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => pattern == name,
        }
    }

    /// The workspace's package names, read from the members' manifests.
    fn workspace_packages() -> Vec<String> {
        let ws = crate::util::workspace_root();
        let root = std::fs::read_to_string(ws.join("Cargo.toml")).expect("the root manifest");
        let members = root
            .split_once("members = [")
            .and_then(|(_, rest)| rest.split_once(']'))
            .expect("[workspace] members")
            .0;
        members
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with('"'))
            .map(|l| l.trim_end_matches(',').trim_matches('"'))
            .map(|dir| {
                let text = std::fs::read_to_string(ws.join(dir).join("Cargo.toml"))
                    .unwrap_or_else(|e| panic!("{dir}: {e}"));
                let manifest: toml::Value = toml::from_str(&text).expect("a manifest");
                manifest["package"]["name"]
                    .as_str()
                    .expect("[package] name")
                    .to_owned()
            })
            .collect()
    }

    /// **Both games by default; `--game` names one, from anywhere on the command line, in either
    /// spelling.** A value that names no game, a flag with no value and two different games are
    /// errors, never a silent run of both; a near-miss is not the flag.
    #[test]
    fn the_game_is_both_unless_the_flag_names_one() {
        assert_eq!(Game::from_args(&argv(&["tier1"])), Ok(Game::All));
        assert_eq!(Game::from_args(&argv(&[])), Ok(Game::All));
        for (words, game) in [
            (&["tier1", "--game", "dereth"][..], Game::Dereth),
            (&["--game", "auberean", "tier1"], Game::Auberean),
            (&["tier0", "--game", "all"], Game::All),
            (
                &["tier1", "--serial", "--game=auberean", "--debug"],
                Game::Auberean,
            ),
            (
                &["tier1", "--game", "dereth", "--game", "dereth"],
                Game::Dereth,
            ),
        ] {
            assert_eq!(Game::from_args(&argv(words)), Ok(game), "{words:?}");
        }
        assert_eq!(Game::given(&argv(&["tier1"])), Ok(None));
        assert_eq!(
            Game::given(&argv(&["tier1", "--game", "all"])),
            Ok(Some(Game::All))
        );
        for bad in [
            &["tier1", "--game"][..],
            &["tier1", "--game", "ac3"],
            &["tier1", "--game", "Auberean"],
            &["tier1", "--game="],
            &["tier1", "--game", "dereth", "--game", "auberean"],
        ] {
            assert!(Game::from_args(&argv(bad)).is_err(), "{bad:?}");
        }
        // Negative control: words that are not the flag leave the default alone.
        for near in [
            &["tier1", "game", "auberean"][..],
            &["tier1", "--games", "auberean"],
            &["tier1", "auberean"],
        ] {
            assert_eq!(Game::from_args(&argv(near)), Ok(Game::All), "{near:?}");
        }
    }

    /// **Every crate belongs to exactly one game, and both games together cover every crate.**
    #[test]
    fn every_crate_belongs_to_exactly_one_game() {
        let packages = workspace_packages();
        assert!(
            packages.iter().any(|p| p == "auberean-dat") && packages.iter().any(|p| p == "xtask"),
            "the manifests were read: {packages:?}"
        );
        for p in &packages {
            assert!(Game::All.covers(p), "{p}");
            assert_ne!(Game::Dereth.covers(p), Game::Auberean.covers(p), "{p}");
        }
        for p in ["auberean-dat", "yalain", "yalain-store", "athenaeum-cli"] {
            assert!(Game::Auberean.covers(p) && !Game::Dereth.covers(p), "{p}");
        }
        for p in ["dereth-dat", "empyrean-world", "lyceum-foundry", "xtask"] {
            assert!(Game::Dereth.covers(p) && !Game::Auberean.covers(p), "{p}");
        }
    }

    /// **The cargo patterns select exactly Asheron's Call 2's packages**: over every package of
    /// the workspace, a name matches one of [`AC2_PATTERNS`] exactly when [`is_ac2_package`]
    /// accepts it, so `-p` and `--exclude` split the workspace where the rest of the task runner
    /// does.
    #[test]
    fn the_cargo_patterns_select_exactly_the_second_games_packages() {
        let mut names = workspace_packages();
        // Planted near-misses: a crate that only starts like one of the second game's.
        names.extend(["yalainx", "auberean", "athenaeum"].map(str::to_owned));
        for name in &names {
            assert_eq!(
                AC2_PATTERNS.iter().any(|p| matches(p, name)),
                is_ac2_package(name),
                "{name}"
            );
        }
    }

    /// **Each filter selects its crates with the arguments cargo takes**: both games are the
    /// workspace, one game is the workspace without the other's or the other's alone.
    #[test]
    fn each_filter_selects_its_crates_with_cargos_arguments() {
        assert_eq!(Game::All.selection(), ["--workspace"]);
        assert_eq!(
            Game::Dereth.selection().join(" "),
            "--workspace --exclude auberean-* --exclude yalain --exclude yalain-* \
             --exclude athenaeum-*"
        );
        assert_eq!(
            Game::Auberean.selection().join(" "),
            "-p auberean-* -p yalain -p yalain-* -p athenaeum-*"
        );
        assert!(Game::All.dereth() && Game::All.auberean());
        assert!(Game::Dereth.dereth() && !Game::Dereth.auberean());
        assert!(!Game::Auberean.dereth() && Game::Auberean.auberean());
        assert_eq!(
            Game::All.title_suffix(),
            "",
            "the default's tables keep their titles"
        );
    }
}
