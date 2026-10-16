//! Everything the preflight does to a machine, behind one trait: commands here and over ssh,
//! copies both ways, processes it leaves running, files, services, and what a program here answers
//! over HTTP, with the waits between tries. [`Real`] does it; the tests hand the preflight a fake
//! that keeps a record instead, and never waits.

use std::path::Path;
use std::process::{Command, Stdio};

use base64::Engine as _;

/// What starts a PowerShell script given as [`powershell`] encodes it.
const ENCODED: &str = "powershell -NoProfile -NonInteractive -EncodedCommand ";

/// `script` as a command for a Windows machine's ssh: PowerShell given the script encoded (UTF-16,
/// in base64), so that it reaches PowerShell unchanged through whichever shell the ssh server
/// starts, its quotes, pipes and paths with spaces included.
pub fn powershell(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    format!(
        "{ENCODED}{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// The script a command made by [`powershell`] carries.
pub fn script_of(command: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(command.strip_prefix(ENCODED)?)
        .ok()?;
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    let units: Vec<u16> = pairs.iter().map(|p| u16::from_le_bytes(*p)).collect();
    String::from_utf16(&units).ok()
}

/// What a failed command said: its errors, and the error lines of its output too, since some
/// tools (dotnet and MSBuild among them) write their errors there; with no error line, the end of
/// its output.
pub fn failure_text(stderr: &str, stdout: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for l in stdout.lines().map(str::trim) {
        if (l.contains(": error ") || l.starts_with("error")) && !lines.contains(&l) {
            lines.push(l);
        }
    }
    if lines.is_empty() {
        let all: Vec<&str> = stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        lines = all[all.len().saturating_sub(15)..].to_vec();
    }
    let err = plain_errors(stderr);
    match (err.is_empty(), lines.is_empty()) {
        (_, true) => err,
        (true, false) => lines.join("\n"),
        (false, false) => format!("{err}\n{}", lines.join("\n")),
    }
}

/// Error output as a person reads it. PowerShell given an encoded command writes its errors as
/// CLIXML (`#< CLIXML`, then XML), so those are taken out as their lines; anything else is kept.
pub fn plain_errors(stderr: &str) -> String {
    let Some(xml) = stderr.trim_start().strip_prefix("#< CLIXML") else {
        return stderr.trim().to_owned();
    };
    let error = regex::Regex::new(r#"<S S="Error">(.*?)</S>"#).expect("a valid pattern");
    let text: String = error
        .captures_iter(xml)
        .map(|c| c[1].to_owned())
        .collect::<String>()
        .replace("_x000D_", "")
        .replace("_x000A_", "\n")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    if text.trim().is_empty() {
        stderr.trim().to_owned()
    } else {
        text.trim().to_owned()
    }
}

/// A command as a person reads it: a PowerShell script as its text.
pub fn readable(command: &str) -> String {
    script_of(command).map_or_else(|| command.to_owned(), |s| format!("PowerShell:\n{s}"))
}

/// What a service is asked to become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Stopped,
    Running,
}

pub trait Machine {
    /// `program args` here, in `dir` when given, with `env` added: its standard output, or why it
    /// failed (its error output).
    fn run(
        &mut self,
        dir: Option<&Path>,
        program: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<String, String>;
    /// `program args` here only to ask (a tool's version): it changes nothing, so a dry run
    /// asks too. Its output, or why it failed.
    fn probe(&mut self, program: &str, args: &[String]) -> Result<String, String>;
    /// `command` on `host` over ssh: its output, or why it failed.
    fn ssh(&mut self, host: &str, command: &str) -> Result<String, String>;
    /// The file or folder `remote` on `host` copied down to `local`, which must not be there.
    fn fetch(&mut self, host: &str, remote: &str, local: &Path) -> Result<(), String>;
    /// The file or folder `local` copied up to `remote` on `host`, which must not be a folder.
    fn send(&mut self, host: &str, local: &Path, remote: &str) -> Result<(), String>;
    /// `program args` started here and left running, its output in `log`: its process id.
    fn spawn(
        &mut self,
        program: &str,
        args: &[String],
        env: &[(String, String)],
        log: &Path,
    ) -> Result<u32, String>;
    /// The process `pid`, which [`Machine::spawn`] started, stopped; one already gone is no error.
    fn kill(&mut self, pid: u32) -> Result<(), String>;
    fn read(&mut self, path: &Path) -> Result<String, String>;
    fn write(&mut self, path: &Path, text: &str) -> Result<(), String>;
    /// A file or a whole folder copied to `to`, replacing what is there.
    fn copy(&mut self, from: &Path, to: &Path) -> Result<(), String>;
    fn exists(&mut self, path: &Path) -> bool;
    /// A file or a whole folder removed; one already gone is no error.
    fn remove(&mut self, path: &Path) -> Result<(), String>;
    /// The service `name` on this machine brought to `to`, waiting until it is there.
    fn service(&mut self, name: &str, to: Service) -> Result<(), String>;
    /// The body of `url` (plain HTTP, on this machine), or why there is none: no answer, or an
    /// error status.
    fn get(&mut self, url: &str) -> Result<String, String>;
    /// A wait of `ms` milliseconds between two tries of something that is still starting.
    fn pause(&mut self, ms: u64);
}

/// This machine.
#[derive(Debug, Default)]
pub struct Real;

/// What a finished command said, as an answer.
fn answer(what: &str, out: std::io::Result<std::process::Output>) -> Result<String, String> {
    let out = out.map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "{what} failed ({}): {}",
            out.status,
            failure_text(
                &String::from_utf8_lossy(&out.stderr),
                &String::from_utf8_lossy(&out.stdout)
            )
        ))
    }
}

impl Machine for Real {
    fn run(
        &mut self,
        dir: Option<&Path>,
        program: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<String, String> {
        let mut c = Command::new(program);
        c.args(args).envs(env.iter().map(|(k, v)| (k, v)));
        if let Some(d) = dir {
            c.current_dir(d);
        }
        // The output goes to this console as it comes, for the long builds; only the answer of a
        // command whose output is read (a version, a count) is kept.
        answer(
            &format!("{program} {}", args.join(" ")),
            c.stderr(Stdio::inherit()).output(),
        )
    }

    fn probe(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        answer(
            &format!("{program} {}", args.join(" ")),
            Command::new(program).args(args).output(),
        )
    }

    fn ssh(&mut self, host: &str, command: &str) -> Result<String, String> {
        answer(
            &format!("on {host}, {}", readable(command)),
            Command::new("ssh")
                .args(["-o", "BatchMode=yes", host, command])
                .output(),
        )
    }

    fn fetch(&mut self, host: &str, remote: &str, local: &Path) -> Result<(), String> {
        if let Some(dir) = local.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let from = format!("{host}:{remote}");
        answer(
            &format!("scp {from}"),
            Command::new("scp")
                .args(["-q", "-r", "-o", "BatchMode=yes", &from])
                .arg(local)
                .output(),
        )
        .map(|_| ())
    }

    fn send(&mut self, host: &str, local: &Path, remote: &str) -> Result<(), String> {
        let to = format!("{host}:{remote}");
        answer(
            &format!("scp to {to}"),
            Command::new("scp")
                .args(["-q", "-r", "-o", "BatchMode=yes"])
                .arg(local)
                .arg(&to)
                .output(),
        )
        .map(|_| ())
    }

    fn spawn(
        &mut self,
        program: &str,
        args: &[String],
        env: &[(String, String)],
        log: &Path,
    ) -> Result<u32, String> {
        let file = std::fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
        let err = file.try_clone().map_err(|e| e.to_string())?;
        let mut c = Command::new(program);
        c.args(args)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(err);
        #[cfg(windows)]
        {
            // Left running after this command ends, with no console of its own.
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            c.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        let child = c.spawn().map_err(|e| format!("{program}: {e}"))?;
        Ok(child.id())
    }

    fn kill(&mut self, pid: u32) -> Result<(), String> {
        let pid = pid.to_string();
        let out = if cfg!(windows) {
            Command::new("taskkill")
                .args(["/PID", &pid, "/T", "/F"])
                .output()
        } else {
            Command::new("kill").arg(&pid).output()
        };
        // A process already gone is what was wanted.
        out.map(|_| ()).map_err(|e| format!("stopping {pid}: {e}"))
    }

    fn read(&mut self, path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn write(&mut self, path: &Path, text: &str) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn copy(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        copy_tree(from, to)
            .map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
    }

    fn exists(&mut self, path: &Path) -> bool {
        path.exists()
    }

    fn remove(&mut self, path: &Path) -> Result<(), String> {
        let r = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match r {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("{}: {e}", path.display()))
            }
            _ => Ok(()),
        }
    }

    fn service(&mut self, name: &str, to: Service) -> Result<(), String> {
        let (verb, state) = match to {
            Service::Stopped => ("stop", "STOPPED"),
            Service::Running => ("start", "RUNNING"),
        };
        let query = |c: &mut Command| c.args(["query", name]).output();
        let now = answer("sc query", query(&mut Command::new("sc.exe")))?;
        if !now.contains(state) {
            // The one-time setup gave this account the right to start and stop it.
            answer(
                &format!("sc {verb} {name}"),
                Command::new("sc.exe").args([verb, name]).output(),
            )?;
        }
        // Services take their time; a minute is long enough for a world to start or stop.
        let since = std::time::Instant::now();
        while since.elapsed() < std::time::Duration::from_secs(60) {
            if answer("sc query", query(&mut Command::new("sc.exe")))?.contains(state) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Err(format!(
            "the service {name} did not reach {state} within a minute"
        ))
    }

    fn get(&mut self, url: &str) -> Result<String, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            // A program that is starting may take the connection and say nothing yet.
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build()
            .into();
        agent
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))
    }

    fn pause(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// A dry run: what would be done is printed, and nothing is; files are read where they are.
#[derive(Debug, Default)]
pub struct DryRun;

impl Machine for DryRun {
    fn run(
        &mut self,
        dir: Option<&Path>,
        program: &str,
        args: &[String],
        _env: &[(String, String)],
    ) -> Result<String, String> {
        let at = dir
            .map(|d| format!(" (in {})", d.display()))
            .unwrap_or_default();
        println!("would run: {program} {}{at}", args.join(" "));
        Ok(String::new())
    }

    fn probe(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        Real.probe(program, args)
    }

    fn ssh(&mut self, host: &str, command: &str) -> Result<String, String> {
        println!("would run on {host}: {}", readable(command));
        Ok(String::new())
    }

    fn fetch(&mut self, host: &str, remote: &str, local: &Path) -> Result<(), String> {
        println!("would copy {host}:{remote} to {}", local.display());
        Ok(())
    }

    fn send(&mut self, host: &str, local: &Path, remote: &str) -> Result<(), String> {
        println!("would copy {} to {host}:{remote}", local.display());
        Ok(())
    }

    fn spawn(
        &mut self,
        program: &str,
        args: &[String],
        _env: &[(String, String)],
        log: &Path,
    ) -> Result<u32, String> {
        println!(
            "would start: {program} {} (its log {})",
            args.join(" "),
            log.display()
        );
        Ok(0)
    }

    fn kill(&mut self, pid: u32) -> Result<(), String> {
        println!("would stop process {pid}");
        Ok(())
    }

    fn read(&mut self, path: &Path) -> Result<String, String> {
        if path.exists() {
            return Real.read(path);
        }
        // A file a dry run would have copied down (a world's configuration) reads as empty.
        println!(
            "would read {}, which a dry run does not copy",
            path.display()
        );
        Ok(String::new())
    }

    fn write(&mut self, path: &Path, _text: &str) -> Result<(), String> {
        println!("would write {}", path.display());
        Ok(())
    }

    fn copy(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        println!("would copy {} to {}", from.display(), to.display());
        Ok(())
    }

    fn exists(&mut self, path: &Path) -> bool {
        path.exists()
    }

    fn remove(&mut self, path: &Path) -> Result<(), String> {
        if path.exists() {
            println!("would remove {}", path.display());
        }
        Ok(())
    }

    fn service(&mut self, name: &str, to: Service) -> Result<(), String> {
        println!("would bring the service {name} to {to:?}");
        Ok(())
    }

    fn get(&mut self, url: &str) -> Result<String, String> {
        println!("would ask {url}");
        Err("a dry run asks nothing".to_owned())
    }

    fn pause(&mut self, _ms: u64) {}
}

/// A file, or a folder and everything in it, copied to `to` (replacing what is there).
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        if to.is_file() {
            std::fs::remove_file(to)?;
        }
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(from, to).map(|_| ())
    }
}
