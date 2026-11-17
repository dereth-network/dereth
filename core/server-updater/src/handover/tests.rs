//! Behaviour: none (the updater's own plumbing: how a server hands over to the release installed in
//! its place on Windows; checked with the decisions alone, the console relay with inputs in memory,
//! and this test program run as a server that updates twice in a row)
//! The Windows check runs this test program again as each server, in a scratch folder of its own
//! that it removes after.

use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

/// What a server's standard input in memory was given, and whether it was closed.
#[derive(Default)]
struct Seen {
    bytes: Vec<u8>,
    closed: bool,
}

/// A server's standard input in memory: it takes everything until the server is ending.
struct Input {
    seen: Arc<Mutex<Seen>>,
    ending: Arc<AtomicBool>,
}

impl Write for Input {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.ending.load(Ordering::SeqCst) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.seen.lock().unwrap().bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.seen.lock().unwrap().closed = true;
    }
}

/// A server's input, what it was given, and the switch that makes it refuse what comes next.
fn server_input() -> (Box<dyn Write + Send>, Arc<Mutex<Seen>>, Arc<AtomicBool>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let ending = Arc::new(AtomicBool::new(false));
    let input = Input {
        seen: Arc::clone(&seen),
        ending: Arc::clone(&ending),
    };
    (Box::new(input), seen, ending)
}

fn text(seen: &Mutex<Seen>) -> String {
    String::from_utf8(seen.lock().unwrap().bytes.clone()).unwrap()
}

#[test]
fn a_server_whose_parent_does_not_wait_on_it_becomes_the_waiting_process() {
    assert_eq!(handover(None), Handover::Wait);
}

#[test]
fn a_server_whose_parent_waits_on_it_exits_to_be_started_again() {
    let console = OsString::from("C:\\Server\\server.exe.console-4242");
    assert_eq!(
        handover(Some(console.clone())),
        Handover::ToParent(PathBuf::from(console))
    );
}

#[test]
fn the_waiting_process_starts_the_server_again_when_it_exits_asking_for_it() {
    assert_eq!(
        after_server(Some(i32::from(START_AGAIN))),
        AfterServer::StartAgain
    );
}

#[test]
fn any_other_exit_ends_the_waiting_process_with_the_same_code() {
    for code in [0, 1, 2, 3, 7, 74, 76, 255] {
        assert_eq!(
            after_server(Some(code)),
            AfterServer::Exit(u8::try_from(code).unwrap()),
            "exit code {code}"
        );
    }
    // A code that does not fit in a byte (a crash's status on Windows), or none at all.
    for code in [Some(256), Some(-1_073_741_819), None] {
        assert_eq!(after_server(code), AfterServer::Exit(1), "{code:?}");
    }
}

#[test]
fn a_process_a_server_starts_is_not_told_its_parent_starts_it_again() {
    let mut started = Command::new("health-check");
    only_where_set(&mut started);
    assert!(
        started
            .get_envs()
            .any(|(key, value)| key == PARENT_RESTARTS && value.is_none()),
        "removed from what it inherits"
    );
    let mut server = Command::new("server");
    server.env(PARENT_RESTARTS, "server.exe.console-4242");
    only_where_set(&mut server);
    assert!(
        server.get_envs().any(|(key, value)| key == PARENT_RESTARTS
            && value == Some(OsStr::new("server.exe.console-4242"))),
        "kept where it is set"
    );
}

#[test]
fn the_next_server_gets_the_lines_the_last_one_left_then_those_typed_since() {
    let relay = Relay::default();
    let mut console = relay.input();
    let (first, first_seen, first_ending) = server_input();
    relay.connect(first);
    console.write_all(b"one\n").unwrap();
    // The first server is ending: what it no longer takes is kept.
    first_ending.store(true, Ordering::SeqCst);
    console.write_all(b"two\n").unwrap();
    // It has exited, leaving a line it took and did not run.
    relay.disconnect(b"held\n".to_vec());
    console.write_all(b"three\n").unwrap();
    let (second, second_seen, _) = server_input();
    relay.connect(second);
    console.write_all(b"four\n").unwrap();
    assert_eq!(text(&first_seen), "one\n");
    assert_eq!(text(&second_seen), "held\ntwo\nthree\nfour\n");
}

#[test]
fn the_end_of_the_console_closes_each_server_input_once_it_has_what_was_kept() {
    let relay = Relay::default();
    let mut console = relay.input();
    let (first, first_seen, _) = server_input();
    relay.connect(first);
    console.write_all(b"one\n").unwrap();
    drop(console);
    assert!(
        first_seen.lock().unwrap().closed,
        "the running server's input is closed"
    );
    relay.disconnect(b"held\n".to_vec());
    let (second, second_seen, _) = server_input();
    relay.connect(second);
    assert_eq!(text(&second_seen), "held\n");
    assert!(
        second_seen.lock().unwrap().closed,
        "the next server's input is closed after what was kept"
    );
}

/// This test program run as the servers of a service that takes two updates in a row.
#[cfg(windows)]
mod windows {
    use std::io::{BufRead, BufReader, Read};
    use std::os::windows::fs::OpenOptionsExt;
    use std::process::{Child, ChildStdin, Stdio};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
    use std::time::Duration;

    use super::*;

    /// Set to a scratch folder: this test program is one of the servers.
    const AS_A_SERVER: &str = "DERETH_SERVER_UPDATER_TEST_AS_A_SERVER";
    /// The test below, which the servers run as.
    const THIS_TEST: &str =
        "handover::tests::windows::two_updates_in_a_row_leave_one_waiting_process_and_one_server";
    /// How many updates the servers take before one serves.
    const UPDATES: u32 = 2;
    /// What a server exits with when its console ends.
    const ENDED: i32 = 9;
    /// A guard against a hang, not the time a step is expected to take.
    const HANG: Duration = Duration::from_secs(120);
    /// Windows' "the file is open, shared with no one".
    const SHARING_VIOLATION: i32 = 32;

    /// The file the server of `generation` holds open, shared with no one, while it runs.
    fn running_file(dir: &Path, generation: u32) -> PathBuf {
        dir.join(format!("running-{generation}"))
    }

    /// Whether the server of `generation` still runs.
    fn running(dir: &Path, generation: u32) -> bool {
        let file = running_file(dir, generation);
        match std::fs::File::open(&file) {
            Ok(_) => false,
            Err(e) if e.raw_os_error() == Some(SHARING_VIOLATION) => true,
            Err(e) => panic!("{}: {e}", file.display()),
        }
    }

    /// The lines of this process's standard input, read on a thread of their own as a server's
    /// console reader reads them, which cannot be interrupted.
    fn console_lines() -> Receiver<String> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        rx
    }

    /// The servers' way of passing their console on: `held`, and what was read since, before it
    /// returns; then each line as it comes.
    fn forward(held: Option<String>, typed: Receiver<String>) -> ForwardConsole {
        Box::new(move |mut to| {
            for line in held.into_iter().chain(typed.try_iter()) {
                let _ = to.write_all(format!("{line}\n").as_bytes());
            }
            std::thread::spawn(move || {
                for line in typed {
                    if to.write_all(format!("{line}\n").as_bytes()).is_err() {
                        return;
                    }
                }
            });
        })
    }

    fn say(generation: u32, text: &str) {
        // On its errors: the test harness writes its own lines to the output.
        eprintln!("{generation} {text}");
    }

    /// One server: it says `<generation> ready`, then runs what is typed at its console
    /// (`say <text>`: it says `<generation> heard <text>`; `quit <code>`: it exits with that code)
    /// until it is told to `update`. The first [`UPDATES`] servers take it: each says
    /// `<generation> updating` and hands over as a server does once its release is installed, the
    /// second after one more line, which it holds for the server after it.
    fn as_a_server(dir: &Path) -> ! {
        let generation = match std::fs::read_to_string(dir.join("generation")) {
            Ok(text) => text.trim().parse::<u32>().expect("a generation") + 1,
            Err(_) => 0,
        };
        std::fs::write(dir.join("generation"), generation.to_string()).expect("written");
        let _running = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .share_mode(0)
            .open(running_file(dir, generation))
            .expect("the server's file");
        let typed = console_lines();
        say(generation, "ready");
        loop {
            let Ok(line) = typed.recv() else {
                std::process::exit(ENDED)
            };
            if line == "update" && generation < UPDATES {
                break;
            }
            if let Some(text) = line.strip_prefix("say ") {
                say(generation, &format!("heard {text}"));
            } else if let Some(code) = line.strip_prefix("quit ") {
                std::process::exit(code.parse().expect("an exit code"));
            }
        }
        say(generation, "updating");
        let held = if generation == 1 {
            let Ok(line) = typed.recv() else {
                std::process::exit(ENDED)
            };
            Some(line)
        } else {
            None
        };
        let exe = std::env::current_exe().expect("this test program");
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        let code = hand_over(&exe, &args, Some(forward(held, typed)));
        std::process::exit(i32::from(code))
    }

    /// What the servers said, on their standard output and error, all of them.
    struct Said {
        lines: Receiver<String>,
        seen: Vec<String>,
    }

    impl Said {
        fn read(child: &mut Child) -> Self {
            let (tx, lines) = mpsc::channel();
            for stream in [
                Box::new(child.stdout.take().expect("its output")) as Box<dyn Read + Send>,
                Box::new(child.stderr.take().expect("its errors")),
            ] {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for line in BufReader::new(stream).lines().map_while(Result::ok) {
                        let _ = tx.send(line);
                    }
                });
            }
            Self {
                lines,
                seen: Vec::new(),
            }
        }

        fn has(&self, line: &str) -> bool {
            self.seen.iter().any(|l| l == line)
        }

        fn wait_for(&mut self, line: &str) {
            while !self.has(line) {
                match self.lines.recv_timeout(HANG) {
                    Ok(l) => self.seen.push(l),
                    Err(e) => panic!(
                        "the servers never said `{line}` ({e}); they said:\n{}",
                        self.seen.join("\n")
                    ),
                }
            }
        }

        /// Until every server's output has closed: every server has exited.
        fn wait_for_the_end(&mut self) {
            loop {
                match self.lines.recv_timeout(HANG) {
                    Ok(l) => self.seen.push(l),
                    Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => panic!(
                        "the servers did not all exit; they said:\n{}",
                        self.seen.join("\n")
                    ),
                }
            }
        }
    }

    fn enter(console: &mut ChildStdin, line: &str) {
        console
            .write_all(format!("{line}\n").as_bytes())
            .expect("typed at the console");
    }

    /// The process the test starts, ended when a check fails first; the server it waits on then
    /// ends with it, its console closed.
    struct First(Child);

    impl Drop for First {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn two_updates_in_a_row_leave_one_waiting_process_and_one_server() {
        if let Some(dir) = std::env::var_os(AS_A_SERVER) {
            as_a_server(Path::new(&dir));
        }
        let dir = std::env::temp_dir().join(format!(
            "dereth-server-updater-handover-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        let exe = std::env::current_exe().expect("this test program");
        let mut first = First(
            Command::new(&exe)
                .args(["--exact", THIS_TEST, "--nocapture", "--test-threads=1"])
                .env(AS_A_SERVER, &dir)
                .env_remove(PARENT_RESTARTS)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("this test program starts as a server"),
        );
        let mut console = first.0.stdin.take().expect("its console");
        let mut said = Said::read(&mut first.0);

        said.wait_for("0 ready");
        enter(&mut console, "update");
        said.wait_for("1 ready");
        enter(&mut console, "update");
        said.wait_for("1 updating");
        // Typed while the second update runs: the server updating takes it and does not run it.
        enter(&mut console, "say during");
        said.wait_for("2 ready");
        enter(&mut console, "say after");
        said.wait_for("2 heard after");
        assert!(
            said.has("2 heard during"),
            "the line typed during the update reaches the next server first; they said:\n{}",
            said.seen.join("\n")
        );

        assert_eq!(
            std::fs::read_to_string(dir.join("generation")).expect("the last generation"),
            "2",
            "three servers started in all"
        );
        assert!(
            running(&dir, 0),
            "the process the service started is still waiting"
        );
        assert!(
            !running(&dir, 1),
            "the server that took the second update has exited"
        );
        assert!(running(&dir, 2), "the newest server serves");
        assert!(first.0.try_wait().expect("its state").is_none());

        enter(&mut console, "quit 7");
        said.wait_for_the_end();
        let status = first.0.wait().expect("it exits");
        assert_eq!(
            status.code(),
            Some(7),
            "the server's exit code ends the waiting process"
        );
        let left = exe.with_file_name(format!(
            "{}.console-{}",
            exe.file_name().unwrap().to_string_lossy(),
            first.0.id()
        ));
        assert!(!left.exists(), "{} is removed", left.display());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
