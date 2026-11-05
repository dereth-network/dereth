//! A run: the hub, the worlds and the players, stepped together, and the waits for what each step
//! should bring.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use auberean_client_sdk::Client;
use auberean_friends::{Hr, Request};
use yalain_testkit::session::Drive;

use crate::programs::{Hub, Process, Scratch, HANG};
use crate::{first_game, second_game};

/// Everything a run started. Dropped in order: the players, then the programs, then the folder.
pub struct Run {
    pub second: Vec<second_game::Player>,
    pub first: Vec<first_game::Player>,
    pub worlds: Vec<Process>,
    pub hub: Hub,
    pub scratch: Scratch,
}

impl Run {
    /// One pass: every player's client and stream, once every program the run started is found
    /// still running.
    pub fn step(&mut self) {
        self.check();
        for p in &mut self.second {
            p.step();
        }
        for p in &mut self.first {
            p.step();
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    /// Steps everyone until `done` holds, within the limit against a hang; fails with what every
    /// player was shown and every program logged.
    pub fn until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let until = Instant::now() + HANG;
        while !done(self) {
            if Instant::now() >= until {
                panic!("{what}: it never came\n{}", self.context());
            }
            self.step();
        }
    }

    /// Every program the run started: the hub and the worlds.
    pub fn programs(&self) -> Vec<&Process> {
        std::iter::once(&self.hub.process)
            .chain(&self.worlds)
            .collect()
    }

    /// Fails at once when a program the run started has ended without the run stopping it, with
    /// its name, how it ended and what it logged last, and what every player was shown.
    pub fn check(&self) {
        for p in self.programs() {
            if let Err(e) = p.alive() {
                panic!("{e}\n{}", self.context());
            }
        }
    }

    /// The world called `slug`.
    pub fn world(&mut self, slug: &str) -> &mut Process {
        self.worlds
            .iter_mut()
            .find(|w| w.name == slug)
            .expect("a world of the run")
    }

    /// The player of this game called `name`.
    pub fn p2(&mut self, name: &str) -> &mut second_game::Player {
        self.second
            .iter_mut()
            .find(|p| p.name == name)
            .expect("a player of the run")
    }

    /// The player of the first game called `name`.
    pub fn p1(&mut self, name: &str) -> &mut first_game::Player {
        self.first
            .iter_mut()
            .find(|p| p.name == name)
            .expect("a player of the run")
    }

    fn index2(&self, name: &str) -> usize {
        self.second
            .iter()
            .position(|p| p.name == name)
            .expect("a player of the run")
    }

    /// A request on a player's stream, and the world's answer.
    pub fn ask(&mut self, name: &str, request: Request) -> Hr {
        let i = self.index2(name);
        let what = format!("{name}'s {request:?} answered");
        let cx = self.second[i].stream.request(request);
        self.until(&what, |r| r.second[i].stream.answered(cx).is_some());
        self.second[i].stream.answered(cx).expect("answered")
    }

    /// A player of this game, its stream admitted, its character made and in the world; the
    /// character.
    pub fn enter2(&mut self, name: &str) -> u64 {
        let i = self.index2(name);
        self.until(&format!("{name}'s ticket admitted"), |r| {
            r.second[i].stream.admitted()
        });
        let mut d = Focus { run: self, who: i };
        let r = yalain_testkit::session::character_list(&mut d)
            .and_then(|()| yalain_testkit::session::create(&mut d, name))
            .and_then(|me| yalain_testkit::session::enter(&mut d, me).map(|()| me));
        let me = r.unwrap_or_else(|e| panic!("{name} into the world: {e}\n{}", self.context()));
        self.until(&format!("{name}'s friends list"), |r| {
            r.second[i].stream.started()
        });
        me
    }

    /// A player of this game logs its character out, its stream left open.
    pub fn log_off2(&mut self, name: &str) {
        let i = self.index2(name);
        let mut d = Focus { run: self, who: i };
        if let Err(e) = yalain_testkit::session::log_off(&mut d) {
            panic!("{name} out of the world: {e}\n{}", self.context());
        }
    }

    /// A player of this game takes its character into the world again.
    pub fn enter_again2(&mut self, name: &str, me: u64) {
        let i = self.index2(name);
        let mut d = Focus { run: self, who: i };
        if let Err(e) = yalain_testkit::session::enter(&mut d, me) {
            panic!("{name} into the world again: {e}\n{}", self.context());
        }
    }

    /// Something a player of this game says aloud.
    pub fn say2(&mut self, name: &str, text: &str) {
        self.p2(name)
            .game
            .client
            .act(auberean_client_sdk::Action::Say { text: text.into() });
    }

    /// Waits for a line in a player of this game's chat windows.
    pub fn shown2(&mut self, name: &str, from: usize, line: &str) {
        let i = self.index2(name);
        self.until(&format!("{name} shown {line:?}"), |r| {
            second_game::shown(&r.second[i].game.client)[from..]
                .iter()
                .any(|l| l.contains(line))
        });
    }

    /// How many lines of any kind a player of this game has been shown, its rooms' lines among
    /// them.
    pub fn lines_of_rooms(&mut self, name: &str) -> usize {
        self.p2(name).game.client.shown.len()
    }

    /// Whether a room's line since `from` names this speaker: the client writes it from its own
    /// strings, with the speaker as one of its variables.
    pub fn heard_in_room(&mut self, name: &str, from: usize, speaker: &str) -> bool {
        self.p2(name).game.client.shown[from..]
            .iter()
            .any(|s| second_game::names(&s.text, speaker))
    }

    /// How many lines a player of this game has been shown.
    pub fn lines2(&mut self, name: &str) -> usize {
        second_game::shown(&self.p2(name).game.client).len()
    }

    /// What every player was shown, the hub's link frames, and the tail of every program's log.
    pub fn context(&self) -> String {
        let mut s = String::new();
        for p in &self.second {
            let _ = writeln!(s, "--- {} (this game) was shown:", p.name);
            for l in second_game::shown(&p.game.client) {
                let _ = writeln!(s, "  {l}");
            }
            let _ = writeln!(s, "--- {}'s stream:", p.name);
            for o in &p.stream.got {
                let _ = writeln!(s, "  {o:?}");
            }
        }
        for p in &self.first {
            let _ = writeln!(s, "--- {} (the first game) was shown:", p.name);
            // A line repeated back to back once, with its count.
            let mut i = 0;
            while i < p.chat.len() {
                let n = p.chat[i..].iter().take_while(|l| **l == p.chat[i]).count();
                let times = if n > 1 {
                    format!(" (x{n})")
                } else {
                    String::new()
                };
                let _ = writeln!(s, "  {}{times}", p.chat[i]);
                i += n;
            }
        }
        let _ = writeln!(s, "--- the hub carried:");
        for l in self.hub.wire() {
            let _ = writeln!(s, "  {l}");
        }
        for w in std::iter::once(&self.hub.process).chain(&self.worlds) {
            let _ = writeln!(s, "--- {} logged, last:", w.name);
            for l in w.tail(40) {
                let _ = writeln!(s, "  {l}");
            }
        }
        let _ = writeln!(s, "the run's folder: {}", self.scratch.dir.display());
        s
    }
}

/// A run, driving one player of this game's client through the session's steps while everyone
/// else goes on.
struct Focus<'a> {
    run: &'a mut Run,
    who: usize,
}

impl Drive for Focus<'_> {
    fn client(&mut self) -> &mut Client {
        &mut self.run.second[self.who].game.client
    }

    fn client_ref(&self) -> &Client {
        &self.run.second[self.who].game.client
    }

    fn step(&mut self) {
        self.run.step();
    }

    fn now(&self) -> f64 {
        self.run.second[self.who].game.now()
    }
}
