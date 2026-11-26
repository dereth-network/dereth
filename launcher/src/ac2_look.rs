//! Building the AC2 look ([`dereth_launch::ac2_look`]): the art tool beside the client run on the
//! player's own Asheron's Call 2 files, its lines read as it goes, the pack it writes joined to the
//! art packs on this computer.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Stdio;

use dereth_launch::ac2_look::{self, Line, Versions};
use serde::Serialize;

/// How far a build has got, for the page.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    /// What it is doing, in plain words.
    pub doing: String,
    /// The steps done.
    pub steps: usize,
}

/// One part's or creature's tick box.
#[derive(Debug, Clone, Serialize)]
pub struct PartView {
    pub id: &'static str,
    pub label: &'static str,
    pub on: bool,
}

/// The Library's AC2 look: whether it can be built, from where, with which parts, and how the last
/// build went.
#[derive(Debug, Clone, Serialize)]
pub struct View {
    /// The art tool is beside the client.
    pub tool: bool,
    /// The Asheron's Call 2 install it is built from, when there is one.
    pub install: Option<String>,
    /// Whether that install's files check.
    pub install_ok: bool,
    /// Asheron's Call's own files are here, which the rocks need.
    pub ac1: bool,
    pub parts: Vec<PartView>,
    pub creatures: Vec<PartView>,
    /// The look's version when it has been built.
    pub built: Option<String>,
    /// A newer table than the one it was built from has come with the tool.
    pub stale: bool,
    pub progress: Option<Progress>,
}

/// The parts' tick boxes, `on` as the player's choices say.
#[must_use]
pub fn parts_view(on: &[String]) -> Vec<PartView> {
    ac2_look::PARTS
        .iter()
        .map(|(id, label)| PartView {
            id,
            label,
            on: on.iter().any(|p| p == id),
        })
        .collect()
}

/// The creatures' tick boxes, `on` as the player's choices say.
#[must_use]
pub fn creatures_view(on: &[String]) -> Vec<PartView> {
    ac2_look::CREATURES
        .iter()
        .map(|(id, label)| PartView {
            id,
            label,
            on: on.iter().any(|c| c == id),
        })
        .collect()
}

/// Runs the art tool: its program and arguments, telling `line` each line of its standard output.
pub type RunTool<'a> =
    dyn Fn(&Path, &[OsString], &mut dyn FnMut(&str)) -> Result<(), String> + Send + Sync + 'a;

/// Run `program` with `args`, its lines of standard output to `line` as they come. An exit other
/// than success is an error, with the last lines it wrote to its standard error.
///
/// # Errors
/// It will not start, or it fails.
pub fn run_tool(
    program: &Path,
    args: &[OsString],
    line: &mut dyn FnMut(&str),
) -> Result<(), String> {
    run_tool_making(program, args, line, "The Asheron's Call 2 Art Pack")
}

/// [`run_tool`] for a build of `what` (its name, as a failure says it could not be built).
///
/// # Errors
/// It will not start, or it fails.
pub fn run_tool_making(
    program: &Path,
    args: &[OsString],
    line: &mut dyn FnMut(&str),
    what: &str,
) -> Result<(), String> {
    run_tool_logged(program, args, line, what, None)
}

/// [`run_tool_making`], everything the tool says (its standard output, then its standard error)
/// also written to the file `log` when one is given, made anew; a failure says where it is.
///
/// # Errors
/// It will not start, or it fails.
pub fn run_tool_logged(
    program: &Path,
    args: &[OsString],
    line: &mut dyn FnMut(&str),
    what: &str,
    log: Option<&Path>,
) -> Result<(), String> {
    let mut file = log.and_then(|p| std::fs::File::create(p).ok());
    if let Some(f) = file.as_mut() {
        use std::io::Write as _;
        let shown: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let _ = writeln!(f, "{} {}", program.display(), shown.join(" "));
    }
    let mut line = |l: &str| {
        if let Some(f) = file.as_mut() {
            use std::io::Write as _;
            let _ = writeln!(f, "{l}");
        }
        line(l);
    };
    // No console window over the launcher's, and outside its own mount.
    let mut cmd = crate::components::command(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("The art tool did not start: {e}"))?;
    let mut err = child.stderr.take();
    let said = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut text);
        }
        text
    });
    if let Some(out) = child.stdout.take() {
        for l in BufReader::new(out).lines() {
            match l {
                Ok(l) => line(&l),
                Err(_) => break,
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let said = said.join().unwrap_or_default();
    if let Some(f) = file.as_mut() {
        use std::io::Write as _;
        let _ = write!(f, "{said}");
        let _ = writeln!(f, "{status}");
    }
    if status.success() {
        return Ok(());
    }
    let whole = log
        .filter(|_| file.is_some())
        .map(|p| format!(" Everything it said is in {}.", p.display()))
        .unwrap_or_default();
    Err(match failure(&said) {
        None => format!("{what} could not be built.{whole}"),
        Some(why) => format!("{what} could not be built: {why}{whole}"),
    })
}

/// Why the tool failed, from what it said on its error output: its own error line
/// (`dereth-art <command>: <why>`) alone, without what it was doing before; else its last lines.
fn failure(said: &str) -> Option<String> {
    let own = said.lines().rev().find_map(|l| {
        let rest = l.strip_prefix("dereth-art ")?;
        let (command, why) = rest.split_once(": ")?;
        (!command.contains(char::is_whitespace)).then(|| why.trim().to_owned())
    });
    if own.is_some() {
        return own;
    }
    let last: Vec<&str> = said.lines().rev().take(3).collect();
    let last: Vec<&str> = last.into_iter().rev().collect();
    (!last.is_empty()).then(|| last.join(" "))
}

/// The tool's tables' versions, asked of it.
///
/// # Errors
/// It will not run, or says no versions.
pub fn versions(run: &RunTool<'_>, tool: &Path) -> Result<Versions, String> {
    let mut text = String::new();
    run(tool, &ac2_look::versions_args(), &mut |l| {
        text.push_str(l);
        text.push('\n');
    })?;
    Versions::read(&text).ok_or_else(|| "The art tool said no versions.".to_owned())
}

/// Run a build, telling `progress` each step as the page says it; the file and version it wrote.
///
/// # Errors
/// It will not run, fails, or never says it wrote the pack.
pub fn build(
    run: &RunTool<'_>,
    tool: &Path,
    args: &[OsString],
    progress: &mut dyn FnMut(Progress),
) -> Result<(std::path::PathBuf, String), String> {
    let mut steps = 0;
    let mut built = None;
    run(tool, args, &mut |l| match Line::read(l) {
        Line::Step(what) => {
            steps += 1;
            progress(Progress {
                doing: ac2_look::step_words(&what).to_owned(),
                steps,
            });
        }
        Line::Built { file, version } => built = Some((file, version)),
        Line::Other => {}
    })?;
    built
        .ok_or_else(|| "The art tool never said it wrote the Asheron's Call 2 Art Pack.".to_owned())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher building the AC2 look, with a stand-in for the art tool)
    use super::*;
    use std::path::PathBuf;

    /// A build with a log keeps everything the tool said in it, its standard output and its
    /// error output, and its failure says where the log is.
    #[cfg(unix)]
    #[test]
    fn a_logged_build_keeps_everything_the_tool_said_and_says_where() {
        let dir = std::env::temp_dir().join(format!("dereth-launcher-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("build.log");
        let args: Vec<OsString> = ["-c", "echo step one; echo starting area >&2; echo 'dereth-art newdereth-world: no land' >&2; exit 1"]
            .map(OsString::from)
            .to_vec();
        let mut seen = Vec::new();
        let err = run_tool_logged(
            Path::new("/bin/sh"),
            &args,
            &mut |l| seen.push(l.to_owned()),
            "The set",
            Some(&log),
        )
        .unwrap_err();
        assert_eq!(seen, ["step one"]);
        assert_eq!(
            err,
            format!(
                "The set could not be built: no land Everything it said is in {}.",
                log.display()
            )
        );
        let kept = std::fs::read_to_string(&log).unwrap();
        for said in [
            "step one",
            "starting area",
            "dereth-art newdereth-world: no land",
        ] {
            assert!(kept.contains(said), "{kept}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A failure is the tool's own error line, without what it was doing before; a tool that
    /// says no such line is said by its last lines.
    #[test]
    fn a_failure_is_the_tools_own_error_line_alone() {
        let said = "placing the starting area\nstarting area: Cragstone (the nearest of AC2's towns, 128 m away) at 0x8E8C0028\ndereth-art newdereth-world: the world pins no land yet\n";
        assert_eq!(failure(said).as_deref(), Some("the world pins no land yet"));
        assert_eq!(failure("a\nb\nc\nd\n").as_deref(), Some("b c d"));
        assert_eq!(failure(""), None);
    }

    #[test]
    fn a_build_says_each_step_and_ends_with_the_file_it_wrote() {
        let run: &RunTool<'_> = &|_tool, args, line| {
            assert_eq!(args[0], "ac2-look");
            for l in [
                "step ground GrassLand",
                "step model 47000412-35.8",
                "step packing",
                "built C:/a b/look.dat 0.1.4",
            ] {
                line(l);
            }
            Ok(())
        };
        let mut seen = Vec::new();
        let done = build(
            run,
            Path::new("tool"),
            &ac2_look::build_args(Path::new("ac2"), None, Path::new("o"), &[]),
            &mut |p| seen.push((p.doing, p.steps)),
        );
        assert_eq!(
            done,
            Ok((PathBuf::from("C:/a b/look.dat"), "0.1.4".to_owned()))
        );
        assert_eq!(
            seen,
            [
                ("Ground".to_owned(), 1),
                ("Trees and rocks".to_owned(), 2),
                ("Packing".to_owned(), 3)
            ]
        );
        let silent: &RunTool<'_> = &|_, _, _| Ok(());
        assert!(build(silent, Path::new("tool"), &[], &mut |_| {}).is_err());
    }

    #[test]
    fn the_tools_versions_are_asked_with_its_own_switch() {
        let run: &RunTool<'_> = &|_, args, line| {
            assert_eq!(args, ac2_look::versions_args());
            line("look 0.1.4");
            line("sky 1");
            Ok(())
        };
        let v = versions(run, Path::new("tool")).expect("versions");
        assert_eq!((v.look.as_str(), v.parts.get("sky")), ("0.1.4", Some(&1)));
    }

    #[test]
    fn the_tick_boxes_follow_the_choices() {
        let v = parts_view(&["sky".to_owned()]);
        assert_eq!(v.len(), ac2_look::PARTS.len());
        assert!(v.iter().all(|p| p.on == (p.id == "sky")));
    }
}
