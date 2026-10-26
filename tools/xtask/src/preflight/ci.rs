//! The public repository's CI on the commit being released. `preflight` pushes the commit it tries
//! to the [`BRANCH`] branch of the remote `ship` pushes to, scratch that each preflight replaces;
//! the owner's mirror carries it to GitHub, where CI runs on it while the candidates build and the
//! worlds are tried. Nothing here pushes to GitHub. `ship` reads that commit's check runs from
//! GitHub's REST API (with no token, unless `GITHUB_TOKEN` is set) and waits while any of them
//! runs: it ships when every one passed, and refuses when one failed, naming each.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::machine::{GitHubAnswer, Machine};

/// The branch a preflight pushes its commit to, for the mirror to carry to GitHub.
pub const BRANCH: &str = "preflight";

/// Where GitHub's REST API is.
pub const API: &str = "https://api.github.com";

/// The shortest wait between two asks while CI runs, in seconds: GitHub gives calls made with no
/// token sixty an hour.
pub const POLL_S: u64 = 60;

/// How long `ship` waits for CI to finish before it gives up, in seconds.
pub const BOUND_S: u64 = 120 * 60;

/// How long it waits for the first check to appear (the commit reaches GitHub when the mirror
/// carries it there) before it gives up, in seconds.
pub const NONE_S: u64 = 15 * 60;

/// How many asks in a row may go unanswered before it stops.
pub const UNANSWERED: u32 = 3;

/// The most check runs one page of the answer holds.
const PER_PAGE: usize = 100;

/// Whether `repo` is a GitHub repository's name, `<owner>/<name>`.
pub fn repo_name(repo: &str) -> bool {
    let part = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    repo.split_once('/')
        .is_some_and(|(owner, name)| part(owner) && part(name))
}

/// The page of `commit`'s checks on GitHub, for a person to open.
pub fn checks_page(repo: &str, commit: &str) -> String {
    format!("https://github.com/{repo}/commit/{commit}/checks")
}

/// The API's address of one page of `commit`'s check runs.
pub fn check_runs_url(repo: &str, commit: &str, page: usize) -> String {
    format!("{API}/repos/{repo}/commits/{commit}/check-runs?per_page={PER_PAGE}&page={page}")
}

fn short(c: &str) -> &str {
    &c[..c.len().min(10)]
}

/// `commit` pushed to the [`BRANCH`] branch of `remote`, replacing what it held, for the mirror to
/// carry to GitHub, where CI runs on it; with `dry_run`, only said. What to say of it.
pub fn push(
    m: &mut dyn Machine,
    root: &Path,
    remote: &str,
    commit: &str,
    dry_run: bool,
) -> Result<String, String> {
    let args = vec![
        "push".to_owned(),
        "--force".to_owned(),
        remote.to_owned(),
        format!("{commit}:refs/heads/{BRANCH}"),
    ];
    let command = format!("git {}", args.join(" "));
    if dry_run {
        return Ok(format!(
            "would push {commit} to {remote}'s {BRANCH} branch, for the mirror to carry to GitHub \
             and CI to run on: {command}"
        ));
    }
    m.run(Some(root), "git", &args, &[]).map_err(|e| {
        format!(
            "{command}: {e}\nNothing was built or stopped. `cargo xtask ship` waits for CI on the \
             commit, which runs on it once it is pushed and mirrored."
        )
    })?;
    Ok(format!(
        "Pushed {commit} to {remote}'s {BRANCH} branch: once the mirror carries it to GitHub, CI \
         runs on it, and `cargo xtask ship` waits for that."
    ))
}

/// One check run of the commit, as GitHub describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub id: u64,
    pub name: String,
    /// `queued`, `in_progress`, `completed`, ...
    pub status: String,
    /// Once it is completed: `success`, `failure`, `cancelled`, ...
    pub conclusion: Option<String>,
    /// Its page on GitHub.
    pub url: String,
}

impl Check {
    fn from_json(v: &Value) -> Result<Self, String> {
        let text = |k: &str| v[k].as_str().map(str::to_owned);
        Ok(Self {
            id: v["id"].as_u64().ok_or("a check run without an id")?,
            name: text("name").ok_or("a check run without a name")?,
            status: text("status").ok_or("a check run without a status")?,
            conclusion: text("conclusion"),
            url: text("html_url")
                .or_else(|| text("details_url"))
                .unwrap_or_default(),
        })
    }

    pub fn done(&self) -> bool {
        self.status == "completed"
    }

    /// Whether it finished and did not fail: a success, skipped, or neutral.
    pub fn passed(&self) -> bool {
        self.done()
            && matches!(
                self.conclusion.as_deref(),
                Some("success" | "skipped" | "neutral")
            )
    }

    /// Its state as a person reads it: where it is while it runs, how it ended once it has.
    pub fn state(&self) -> String {
        let s = if self.done() {
            self.conclusion.as_deref().unwrap_or("finished")
        } else {
            &self.status
        };
        s.replace('_', " ")
    }

    fn line(&self) -> String {
        format!("  {}: {}, {}", self.name, self.state(), self.url)
    }
}

/// One page of the answer: how many check runs the commit has, and this page's.
fn read_page(body: &str) -> Result<(u64, Vec<Check>), String> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| format!("GitHub's answer for the checks does not read: {e}"))?;
    let total = v["total_count"]
        .as_u64()
        .ok_or("GitHub's answer for the checks has no total_count")?;
    let runs = v["check_runs"]
        .as_array()
        .ok_or("GitHub's answer for the checks has no check_runs")?
        .iter()
        .map(Check::from_json)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((total, runs))
}

/// What one ask of GitHub for the commit's checks came to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    /// Every check run, with the calls left this hour and when they are given back, when GitHub
    /// said.
    Checks(Vec<Check>, Option<(u64, u64)>),
    /// The hour's calls are spent: GitHub may be asked again at this time.
    Limited(u64),
    /// No answer, or one that says GitHub could not answer now: asked again later.
    Unanswered(String),
}

/// When GitHub may be asked again, if `a` says its limit is spent (a 403 or a 429 that says so):
/// after `retry-after`'s wait, or once the hour's calls are given back.
fn limited(a: &GitHubAnswer, now: u64) -> Option<u64> {
    if a.status != 403 && a.status != 429 {
        return None;
    }
    if let Some(s) = a.retry_after {
        return Some(now + s);
    }
    if a.remaining == Some(0) {
        return Some(a.reset.unwrap_or(now + POLL_S).max(now));
    }
    // A limit on how fast calls come says so in its message alone: a minute, as GitHub asks.
    (a.status == 429 || a.body.contains("rate limit")).then_some(now + POLL_S)
}

/// Every check run of `commit` in `repo`, page by page (none while the commit is not on GitHub,
/// which it reaches when the mirror carries it there); or why GitHub will not say, which asking
/// again does not change (the repository is not there).
fn ask(m: &mut dyn Machine, repo: &str, commit: &str) -> Result<Asked, String> {
    let mut checks = Vec::new();
    let mut page = 1;
    loop {
        let url = check_runs_url(repo, commit, page);
        let a = match m.github(&url) {
            Ok(a) => a,
            Err(e) => return Ok(Asked::Unanswered(e)),
        };
        if let Some(until) = limited(&a, m.now()) {
            return Ok(Asked::Limited(until));
        }
        if a.status >= 500 {
            return Ok(Asked::Unanswered(format!("GitHub answered {}", a.status)));
        }
        // "No commit found": the mirror has not carried it there yet.
        if a.status == 422 {
            return Ok(Asked::Checks(Vec::new(), None));
        }
        if a.status != 200 {
            let message = serde_json::from_str::<Value>(&a.body)
                .ok()
                .and_then(|v| v["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| a.body.trim().to_owned());
            return Err(format!(
                "GitHub answered {} for the checks on {} in {repo}: {message}\n`github_repo` in \
                 the setup file names the repository.",
                a.status,
                short(commit)
            ));
        }
        let (total, runs) = read_page(&a.body)?;
        let left = a.remaining.zip(a.reset);
        let got = runs.len();
        checks.extend(runs);
        if got < PER_PAGE || checks.len() as u64 >= total {
            return Ok(Asked::Checks(checks, left));
        }
        page += 1;
    }
}

/// The lines naming `checks`.
fn lines(checks: &[&Check]) -> String {
    checks
        .iter()
        .map(|c| c.line())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `checks` still running, named after `heading`, on lines of their own; nothing when none is.
fn still_running(checks: &[Check], heading: &str) -> String {
    let running: Vec<&Check> = checks.iter().filter(|c| !c.done()).collect();
    if running.is_empty() {
        String::new()
    } else {
        format!("\n{heading}\n{}", lines(&running))
    }
}

fn minutes(s: u64) -> u64 {
    s.div_ceil(60)
}

/// The wait until GitHub's spent limit is given back at `until`, a minute at least; or, when that
/// is after `deadline` (or with `once`, at all), why `ship` stops, naming what was still running
/// at the last ask (`last`).
fn wait_for_limit(
    until: u64,
    now: u64,
    deadline: u64,
    once: bool,
    last: &[Check],
) -> Result<u64, String> {
    let wait = (until.saturating_sub(now) + 1).max(POLL_S);
    if !once && now + wait <= deadline {
        return Ok(wait);
    }
    Err(format!(
        "GitHub's hourly limit on calls is spent for {} more minutes{}; nothing is shipped.{}\n\
         `cargo xtask ship` asks again; `--skip-checks` ships without asking.",
        minutes(until.saturating_sub(now)),
        if once {
            String::new()
        } else {
            format!(", past the {} minutes `ship` waits", minutes(BOUND_S))
        },
        still_running(last, "Still running at the last ask:")
    ))
}

/// What `say` says when the wait is GitHub's limit's.
fn limit_said(wait: u64) -> String {
    format!(
        "GitHub's hourly limit on calls is spent: asking again in {} minutes, once it is given \
         back",
        minutes(wait)
    )
}

/// What is said while nothing has run on the commit on GitHub: the mirror may not have carried
/// it there yet.
const NOTHING_YET: &str = "nothing has run on it on GitHub yet: has the mirror pushed `preflight`?";

/// What one ask's `checks` decide, once every one has finished or one has failed: the line to say
/// when `ship` carries on (all passed, or with `allow_failing`, the failed ones named), or why it
/// refuses. `None` while it waits: some still run, and none has failed (or `allow_failing`).
fn verdict(
    checks: &[Check],
    c: &str,
    page: &str,
    allow_failing: bool,
) -> Option<Result<String, String>> {
    let failed: Vec<&Check> = checks.iter().filter(|c| c.done() && !c.passed()).collect();
    let finished = !checks.is_empty() && checks.iter().all(Check::done);
    if finished && failed.is_empty() {
        return Some(Ok(format!(
            "CI passed on {c}: all {} checks green ({page}).",
            checks.len()
        )));
    }
    if failed.is_empty() || (allow_failing && !finished) {
        return None;
    }
    Some(if allow_failing {
        Ok(format!(
            "CI failed on {c}; shipping despite it (--allow-failing):\n{}",
            lines(&failed)
        ))
    } else {
        Err(format!(
            "CI failed on {c}; nothing is shipped. The checks that failed:\n{}{}\n`cargo xtask \
             ship --allow-failing` ships despite them.",
            lines(&failed),
            still_running(checks, "Still running:")
        ))
    })
}

/// What a dry run says of `checks` that still run, or of none started yet.
fn would_wait(checks: &[Check], c: &str, page: &str) -> String {
    let running: Vec<&str> = checks
        .iter()
        .filter(|c| !c.done())
        .map(|c| c.name.as_str())
        .collect();
    if running.is_empty() {
        format!(
            "Nothing has run on {c} on GitHub yet: has the mirror pushed `{BRANCH}`? Would wait \
             for it ({page})."
        )
    } else {
        format!(
            "CI is still running on {c}: would wait for {} ({page}).",
            running.join(", ")
        )
    }
}

/// `commit`'s checks in the GitHub repository `repo` (`<owner>/<name>`), waited for while any of
/// them runs: asked [`POLL_S`] apart at least, further apart when GitHub's hourly limit needs it,
/// and not before a spent limit is given back; each job's state said through `say` as it changes;
/// for [`BOUND_S`] at most, and [`NONE_S`] while nothing has run on the commit there (which the
/// mirror may not have carried there yet). Answers the line to say when it ships on them
/// ([`verdict`]), or why it refuses: a failed check, the wait spent, or GitHub not answering. With
/// `once` (a dry run), asked once and never waited for.
pub fn await_checks(
    m: &mut dyn Machine,
    repo: &str,
    commit: &str,
    allow_failing: bool,
    once: bool,
    say: &mut dyn FnMut(&str),
) -> Result<String, String> {
    if !repo_name(repo) {
        return Err(format!("`github_repo` is {repo}, not `<owner>/<name>`"));
    }
    let page = checks_page(repo, commit);
    let c = short(commit);
    let start = m.now();
    let deadline = start + BOUND_S;
    // Each check's state as it was last said, by its id.
    let mut shown: BTreeMap<u64, String> = BTreeMap::new();
    let mut unanswered = 0;
    let mut last: Vec<Check> = Vec::new();
    let mut waiting = false;
    let mut said_nothing = false;
    loop {
        let asked = ask(m, repo, commit)?;
        let now = m.now();
        let wait = match asked {
            Asked::Unanswered(e) => {
                unanswered += 1;
                if once || unanswered >= UNANSWERED {
                    return Err(format!(
                        "GitHub did not answer for the checks on {c} ({e}); nothing is shipped. \
                         `cargo xtask ship` asks again; `--skip-checks` ships without asking."
                    ));
                }
                say(&format!(
                    "GitHub did not answer ({e}): asking again in a minute"
                ));
                POLL_S
            }
            Asked::Limited(until) => {
                unanswered = 0;
                let wait = wait_for_limit(until, now, deadline, once, &last)?;
                say(&limit_said(wait));
                wait
            }
            Asked::Checks(checks, left) => {
                unanswered = 0;
                if let Some(v) = verdict(&checks, c, &page, allow_failing) {
                    return v;
                }
                if once {
                    return Ok(would_wait(&checks, c, &page));
                }
                if !waiting {
                    waiting = true;
                    say(&format!(
                        "Waiting for CI on {c} ({page}), for {} minutes at most:",
                        minutes(BOUND_S)
                    ));
                }
                if checks.is_empty() && !said_nothing {
                    said_nothing = true;
                    say(&format!("  {NOTHING_YET}"));
                }
                for check in &checks {
                    let state = check.state();
                    if shown.get(&check.id) != Some(&state) {
                        say(&format!("  {}: {state}", check.name));
                        shown.insert(check.id, state);
                    }
                }
                if checks.is_empty() && now >= start + NONE_S {
                    return Err(format!(
                        "Nothing has run on {c} on GitHub in {} minutes ({page}); nothing is \
                         shipped. Has the mirror pushed `{BRANCH}`? `cargo xtask preflight` \
                         pushes the commit to that branch for the mirror to carry to GitHub; \
                         `--skip-checks` ships without CI.",
                        minutes(NONE_S)
                    ));
                }
                if now >= deadline {
                    return Err(format!(
                        "CI has not finished on {c} in {} minutes ({page}); nothing is shipped.{}\n\
                         `cargo xtask ship` waits again; `--skip-checks` ships without waiting.",
                        minutes(BOUND_S),
                        still_running(&checks, "Still running:")
                    ));
                }
                last = checks;
                // The calls left this hour spread over what is left of it, the waits never longer
                // than what is left of `ship`'s.
                match left {
                    Some((0, reset)) => {
                        let wait = wait_for_limit(reset, now, deadline, false, &last)?;
                        say(&limit_said(wait));
                        wait
                    }
                    Some((left, reset)) => (reset.saturating_sub(now) / left)
                        .clamp(POLL_S, deadline.saturating_sub(now).max(POLL_S)),
                    None => POLL_S,
                }
            }
        };
        m.pause(wait * 1000);
    }
}
