//! A script run on the SDK's driver: [`Runner`] carries out the commands in turn, one frame at a
//! time, and tells what happens as lines of text.
//!
//! A host calls [`Runner::step`] until it answers that the script is done, then [`Runner::finish`];
//! between steps it may hand the runner lines typed as they come ([`Runner::line`]) and prints
//! what the runner has to tell ([`Runner::take_lines`]). A command that takes time (entering the
//! world, `run`, `hold`) holds the next until it is done; one that takes none runs with the next
//! in the same step.
//!
//! Every frame the runner takes the frame's events and the scripts' notices, brings the view up to
//! date and reads the game from it alone: where the session stands, the character list, what is
//! said, offers, effects, a making's answer, the other players around the player and where they go.
//! It answers offers as its standing orders say (`agree`, `offer`). It reads no device: whatever
//! it does is an action.

use std::collections::{BTreeMap, VecDeque};

use auberean_client_sdk::contract::control::{Control, ControlFrame, ControlId};
use auberean_client_sdk::contract::event::{
    ChatEvent, CraftEvent, EffectEvent, SessionEvent, SocialEvent, TradeEvent,
};
use auberean_client_sdk::contract::notice::Notice;
use auberean_client_sdk::contract::session::CharacterList;
use auberean_client_sdk::contract::view::View;
use auberean_client_sdk::contract::{CraftAction, SocialAction, TradeAction};
use auberean_client_sdk::driver::CLIENT_CLOCK_START;
use auberean_client_sdk::plugins::{PluginHook, PluginHost};
use auberean_client_sdk::sim::Walk;
use auberean_client_sdk::{no_icons, Action, Clock, Driver, GameEvent, NewCharacter, Phase, Port};

use crate::script::{control_name, parse_line, Character, Command, Dump};

/// The template every player's object is made of.
const PLAYER_TEMPLATE: u32 = 0x4700_0530;
/// How long after an entry is refused (the character still in the world) it is asked again.
const RETRY_AFTER: f64 = 2.0;
/// How long the account's characters are waited for.
const LIST_IN: f64 = 600.0;
/// How long a making of a character, an entry once asked and a log-off are waited for.
const ANSWER_IN: f64 = 120.0;
const LOG_OFF_IN: f64 = 60.0;
/// The most events and notices kept for `dump`.
const KEPT: usize = 4096;

/// One step of the script: a command, or the second half of a making (its slots filled once the
/// recipe is read).
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Command(Command),
    Fill { recipe: u32, spinner: i32 },
}

/// What the command under way waits for.
#[derive(Clone, Debug, PartialEq)]
enum Wait {
    Nothing,
    Frames(u64),
    /// The host clock's time.
    Until(f64),
    List {
        since: f64,
    },
    Create {
        name: String,
        since: f64,
        asked: Option<u32>,
    },
    Entry {
        who: Option<Character>,
        since: f64,
        asked: bool,
        retry_at: f64,
    },
    LogOff {
        since: f64,
    },
}

/// A script being run (see the module's description).
pub struct Runner<'w> {
    driver: Driver,
    walk: Option<Walk<'w>>,
    queue: VecDeque<Step>,
    wait: Wait,
    done: bool,
    lines: Vec<String>,
    phase: Phase,
    characters: Option<CharacterList>,
    /// When the player entered the world, on the host clock.
    entered: Option<f64>,
    logging_off: bool,
    agree: bool,
    offer: bool,
    trade_accepted: bool,
    /// The items of the last making, by slot.
    slots: Vec<(u32, u64)>,
    /// The other players seen, and where each was last told.
    others: BTreeMap<u64, (u32, [f32; 3])>,
    events: VecDeque<GameEvent>,
    notices: VecDeque<Notice>,
    /// What the controls in force asked of the last frame, when no world moves the body.
    controls: ControlFrame,
    /// The host clock's time at the last frame.
    time: f64,
}

impl core::fmt::Debug for Runner<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Runner")
            .field("queue", &self.queue)
            .field("wait", &self.wait)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl<'w> Runner<'w> {
    /// A run of `script` on `driver`, the player's body walked through `walk`'s world when one is
    /// given (without one, the controls move nothing).
    #[must_use]
    pub fn new(driver: Driver, walk: Option<Walk<'w>>, script: Vec<Command>) -> Self {
        Self {
            driver,
            walk,
            queue: script.into_iter().map(Step::Command).collect(),
            wait: Wait::Nothing,
            done: false,
            lines: Vec::new(),
            phase: Phase::Connecting,
            characters: None,
            entered: None,
            logging_off: false,
            agree: false,
            offer: false,
            trade_accepted: false,
            slots: Vec::new(),
            others: BTreeMap::new(),
            events: VecDeque::new(),
            notices: VecDeque::new(),
            controls: ControlFrame::default(),
            time: 0.0,
        }
    }

    /// The driver.
    #[must_use]
    pub const fn driver(&self) -> &Driver {
        &self.driver
    }

    /// The driver, mutably.
    pub fn driver_mut(&mut self) -> &mut Driver {
        &mut self.driver
    }

    /// The world the player's body is walked through, when the run walks it.
    #[must_use]
    pub const fn walk(&self) -> Option<&Walk<'w>> {
        self.walk.as_ref()
    }

    /// The game as the last frame left it.
    #[must_use]
    pub fn view(&self) -> &View {
        self.driver.view()
    }

    /// Where the session stands, as its events told.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// What the controls in force asked of the last frame, when no world moves the player's body
    /// (with one, the body takes them).
    #[must_use]
    pub const fn controls(&self) -> &ControlFrame {
        &self.controls
    }

    /// The player's body is moving.
    #[must_use]
    pub fn moving(&self) -> bool {
        self.view().player.as_ref().is_some_and(|p| p.body.moving)
    }

    /// The plugin host installed as the driver's hook, if one is.
    pub fn plugin_host(&mut self) -> Option<&mut PluginHost> {
        self.driver
            .hook_mut()
            .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
            .map(PluginHook::host_mut)
    }

    /// What there is to tell since the host last asked, a line each.
    pub fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// The host clock's time at the last frame (0 before the first).
    const fn now(&self) -> f64 {
        self.time
    }

    /// A line told: in the world, after the seconds since the entry.
    fn tell(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        match self.entered {
            Some(at) => {
                let line = format!("{:7.2} {text}", self.now() - at);
                self.lines.push(line);
            }
            None => self.lines.push(text.to_owned()),
        }
    }

    /// Run the script on until it waits for a frame, then run the frame. `false` once the script is
    /// done (nothing is run then).
    ///
    /// # Errors
    /// The session ended under the player, the server did not answer something waited for, or
    /// the character to enter is not on the account.
    pub fn step(&mut self, clock: &mut dyn Clock, port: &mut dyn Port) -> Result<bool, String> {
        self.start_steps()?;
        if self.done {
            return Ok(false);
        }
        self.frame(clock, port)?;
        self.check_wait()?;
        Ok(true)
    }

    /// Steps until the script is done or `frames` frames have run; then [`Self::finish`]. `Ok`
    /// with whether it was done.
    ///
    /// # Errors
    /// As [`Self::step`].
    pub fn run_to_end(
        &mut self,
        clock: &mut dyn Clock,
        port: &mut dyn Port,
        frames: usize,
    ) -> Result<bool, String> {
        for _ in 0..frames {
            if !self.step(clock, port)? {
                self.finish(port);
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The run's end: the plugins' stores written, a goodbye on every connection, as the client
    /// says when it closes.
    pub fn finish(&mut self, port: &mut dyn Port) {
        if let Some(host) = self.plugin_host() {
            host.flush();
        }
        self.driver.disconnect(port);
        self.lines.push("done".into());
    }

    /// A line typed as it comes: one starting `/` typed into the chat box (a plugin's command first,
    /// when one claims it), any other a command of the script carried out now. A command that
    /// takes time is not taken: it is told so.
    pub fn line(&mut self, text: &str) {
        let text = text.trim();
        let command = if text.starts_with('/') {
            Command::Say(text.to_owned())
        } else {
            match parse_line(text) {
                Ok(Some(c)) => c,
                Ok(None) => return,
                Err(e) => {
                    self.lines.push(format!("        {e}"));
                    return;
                }
            }
        };
        if !command.at_once() {
            self.lines.push(format!(
                "        {text}: it takes time, so only a script runs it"
            ));
            return;
        }
        if let Err(e) = self.start(Step::Command(command)) {
            self.lines.push(format!("        {e}"));
        }
    }

    /// Start the steps waiting, in turn, until one waits for frames or none are left.
    fn start_steps(&mut self) -> Result<(), String> {
        loop {
            self.check_wait()?;
            if self.wait != Wait::Nothing || self.done {
                return Ok(());
            }
            let Some(s) = self.queue.pop_front() else {
                self.done = true;
                return Ok(());
            };
            self.start(s)?;
        }
    }

    fn act(&mut self, a: Action) {
        self.driver.act(a);
    }

    fn press(&mut self, id: ControlId, on: bool) {
        let c = if on {
            Control::begin(id)
        } else {
            Control::end(id)
        };
        self.act(Action::Control(c));
        let what = if on { "begins" } else { "ends" };
        self.tell(format!("{what} {}", control_name(id)));
    }

    #[allow(clippy::too_many_lines)]
    fn start(&mut self, s: Step) -> Result<(), String> {
        let now = self.now();
        let c = match s {
            Step::Command(c) => c,
            Step::Fill { recipe, spinner } => {
                self.fill(recipe, spinner);
                return Ok(());
            }
        };
        match c {
            Command::Login => self.wait = Wait::List { since: now },
            Command::Create(name) => {
                self.wait = Wait::Create {
                    name,
                    since: now,
                    asked: None,
                }
            }
            Command::Enter(who) => {
                self.wait = Wait::Entry {
                    who,
                    since: now,
                    asked: false,
                    retry_at: now,
                }
            }
            Command::Tick(n) => {
                if n > 0 {
                    self.wait = Wait::Frames(n);
                }
            }
            Command::Run(s) => self.wait = Wait::Until(now + s),
            Command::At(s) => self.wait = Wait::Until(self.entered.unwrap_or(0.0) + s),
            Command::Act(a) => {
                self.tell(format!("acts: {a:?}"));
                self.act(a);
            }
            Command::Begin(id) => self.press(id, true),
            Command::End(id) => self.press(id, false),
            Command::Press(id) => {
                self.press(id, true);
                self.wait = Wait::Frames(1);
                self.queue.push_front(Step::Command(Command::Tick(1)));
                self.queue.push_front(Step::Command(Command::End(id)));
            }
            Command::Hold(id, s) => {
                self.press(id, true);
                self.wait = Wait::Until(now + s);
                self.queue.push_front(Step::Command(Command::End(id)));
            }
            Command::Say(text) => {
                let claimed =
                    text.starts_with('/') && self.plugin_host().is_some_and(|h| h.command(&text));
                if !claimed {
                    self.tell(format!("saying: {text}"));
                    self.act(Action::Say { text });
                }
            }
            Command::Select(s) => {
                let a = s.action();
                self.tell(format!("selects: {a:?}"));
                self.act(a);
            }
            Command::Use(target) => {
                if let Some(target) = target {
                    self.act(Action::Select { target });
                }
                self.tell(format!(
                    "uses {}",
                    target.map_or_else(|| "the selection".to_owned(), |t| format!("{t:#018x}"))
                ));
                self.act(Action::UseSelected);
            }
            Command::Craft { recipe, spinner } => {
                self.act(Action::Craft(CraftAction::Inquire { recipe, spinner }));
                self.wait = Wait::Frames(1);
                self.queue.push_front(Step::Fill { recipe, spinner });
            }
            Command::Slots => self.print_slots(),
            Command::Agree(on) => self.agree = on,
            Command::Offer(on) => self.offer = on,
            Command::Snapshot => {
                let lines = crate::snapshot::text(self.view(), self.phase);
                self.lines.extend(lines);
            }
            Command::Dump(Dump::Events) => {
                let events = std::mem::take(&mut self.events);
                self.lines
                    .extend(events.iter().map(|e| format!("event {e:?}")));
            }
            Command::Dump(Dump::Notices) => {
                let notices = std::mem::take(&mut self.notices);
                self.lines
                    .extend(notices.iter().map(|n| format!("notice {n:?}")));
            }
            Command::Position => {
                let line = crate::snapshot::position(self.view());
                self.tell(format!("position: {line}"));
            }
            Command::Plugin(line) => {
                let answer = match self.plugin_host() {
                    Some(host) => crate::plugins::line(host, &line),
                    None => Err("no plugins run in this client".to_owned()),
                };
                match answer {
                    Ok(lines) => self
                        .lines
                        .extend(lines.iter().map(|l| format!("        {l}"))),
                    Err(e) => self.lines.push(format!("        {e}")),
                }
            }
            Command::Logoff => {
                let v = self.view();
                let around = v.world.as_ref().map_or(0, |w| w.things.len());
                let held = v
                    .inventory
                    .as_ref()
                    .map_or(0, |i| crate::snapshot::held(i).len());
                let calls = v.calls;
                self.tell(format!(
                    "logging off: {around} objects around, {held} in the inventory, {} calls run \
                     ({} to the end)",
                    calls.run, calls.completed
                ));
                self.act(Action::LogOff);
                self.logging_off = true;
                self.wait = Wait::LogOff { since: now };
            }
            Command::Quit => {
                self.queue.clear();
                self.done = true;
            }
        }
        Ok(())
    }

    /// Whether the command under way is done; on to the next when it is.
    fn check_wait(&mut self) -> Result<(), String> {
        let now = self.now();
        match &mut self.wait {
            Wait::Nothing => {}
            Wait::Frames(n) => {
                if *n == 0 {
                    self.wait = Wait::Nothing;
                }
            }
            Wait::Until(t) => {
                if now >= *t {
                    self.wait = Wait::Nothing;
                }
            }
            Wait::List { since } => {
                if self.characters.is_some() {
                    self.wait = Wait::Nothing;
                } else if now - *since > LIST_IN {
                    return Err("no character list in ten minutes".into());
                }
            }
            Wait::Create { .. } => self.check_create(now)?,
            Wait::Entry { .. } => self.check_entry(now)?,
            Wait::LogOff { since } => {
                if self.phase == Phase::Characters {
                    self.logging_off = false;
                    self.entered = None;
                    // A second more, for the server to let the character go.
                    self.wait = Wait::Until(now + 1.0);
                } else if now - *since > LOG_OFF_IN {
                    return Err("the server did not answer the log-off".into());
                }
            }
        }
        Ok(())
    }

    fn check_create(&mut self, now: f64) -> Result<(), String> {
        let Wait::Create { name, since, asked } = &mut self.wait else {
            return Ok(());
        };
        let Some(list) = &self.characters else {
            if now - *since > LIST_IN {
                return Err("no character list in ten minutes".into());
            }
            return Ok(());
        };
        if list.named(name).is_some() {
            self.wait = Wait::Nothing;
            return Ok(());
        }
        let answers = self.driver.view().pregame.creation;
        match *asked {
            None => {
                let name = name.clone();
                *asked = Some(answers.map_or(0, |a| a.count));
                *since = now;
                self.lines.push(format!("making {name}"));
                self.driver
                    .act(Action::CreateCharacter(new_character(&name)));
            }
            Some(count) => {
                if let Some(a) = answers.filter(|a| a.count > count && a.response != 1) {
                    return Err(format!("{name} was not made (error {})", a.error));
                }
                if now - *since > ANSWER_IN {
                    return Err(format!("no answer to making {name}"));
                }
            }
        }
        Ok(())
    }

    fn check_entry(&mut self, now: f64) -> Result<(), String> {
        if self.phase == Phase::World && self.entered.is_some() {
            self.wait = Wait::Nothing;
            return Ok(());
        }
        let Wait::Entry {
            who,
            since,
            asked,
            retry_at,
        } = &mut self.wait
        else {
            return Ok(());
        };
        if *asked {
            if now - *since > ANSWER_IN {
                return Err("no entry into the world in two minutes".into());
            }
            return Ok(());
        }
        let Some(list) = &self.characters else {
            if now - *since > LIST_IN {
                return Err("no character list in ten minutes".into());
            }
            return Ok(());
        };
        if now < *retry_at {
            return Ok(());
        }
        let character = match who {
            None => list.characters.first().map(|c| c.id),
            Some(Character::Id(id)) => Some(*id),
            Some(Character::Name(n)) => list.named(n).map(|c| c.id),
        }
        .ok_or_else(|| match who {
            Some(Character::Name(n)) => format!("the account has no character {n}"),
            _ => "the account has no characters".to_owned(),
        })?;
        *asked = true;
        *since = now;
        self.lines
            .push(format!("entering the world as {character:#018x}"));
        self.driver.act(Action::PlayCharacter { character });
        if let Some(host) = self.plugin_host() {
            host.playing(character);
        }
        Ok(())
    }

    /// One frame, and what it brought heard.
    fn frame(&mut self, clock: &mut dyn Clock, port: &mut dyn Port) -> Result<(), String> {
        match self.walk.as_mut() {
            Some(walk) => self.driver.frame_walking(clock, port, walk),
            None => {
                let now = clock.frame_time();
                self.driver.receive(now, port);
                self.controls = self.driver.take_control_frame();
                self.driver.finish(port);
            }
        }
        self.time = self.driver.local_time() - CLIENT_CLOCK_START;
        if let Wait::Frames(n) = &mut self.wait {
            *n = n.saturating_sub(1);
        }
        self.driver.refresh_view(&no_icons);
        for n in self.driver.take_notices() {
            if self.notices.len() >= KEPT {
                self.notices.pop_front();
            }
            self.notices.push_back(n);
        }
        let now = self.now();
        for e in self.driver.take_events() {
            if self.events.len() >= KEPT {
                self.events.pop_front();
            }
            self.events.push_back(e.clone());
            self.event(e, now)?;
        }
        if self.entered.is_some() && !self.logging_off {
            self.watch();
        }
        // A camera a plugin chose: the walk's from the next frame (without a world to walk in,
        // nothing turns).
        let camera = self.plugin_host().and_then(PluginHost::take_camera);
        if let (Some(settings), Some(walk)) = (camera, self.walk.as_mut()) {
            walk.set_scheme(settings);
        }
        let printed = self
            .plugin_host()
            .map(PluginHost::take_printed)
            .unwrap_or_default();
        for l in printed {
            match l.from {
                Some(name) => self.tell(format!("plugin {name}: {}", l.text)),
                None => self.tell(format!("plugins: {}", l.text)),
            }
        }
        Ok(())
    }

    fn event(&mut self, e: GameEvent, now: f64) -> Result<(), String> {
        match e {
            GameEvent::Session(SessionEvent::Phase(p)) => {
                self.phase = p;
                let entering = matches!(self.wait, Wait::Entry { asked: true, .. });
                if p == Phase::World && entering && self.entered.is_none() {
                    self.entered = Some(now);
                    let cell = self.view().player.as_ref().map_or(0, |p| p.body.cell);
                    self.lines.push(format!("in the world at {cell:08X}"));
                }
            }
            GameEvent::Session(SessionEvent::Characters(list)) => self.characters = Some(list),
            GameEvent::Session(SessionEvent::EntryRefused { reason }) if self.entered.is_none() => {
                if let Wait::Entry {
                    asked, retry_at, ..
                } = &mut self.wait
                {
                    *asked = false;
                    *retry_at = now + RETRY_AFTER;
                    self.lines
                        .push(format!("the entry was refused ({reason}); asking again"));
                }
            }
            GameEvent::Session(SessionEvent::Ended(end)) if !self.logging_off => {
                return Err(format!("the session ended: {end:?}"));
            }
            e if self.entered.is_some() && !self.logging_off => self.react(&e),
            _ => {}
        }
        Ok(())
    }

    /// What happened in the world, told, and an offer answered as the standing orders say: a
    /// place in a fellowship, an oath and a trade taken, a trade's other side accepted, the first
    /// item of the pack offered.
    pub fn react(&mut self, e: &GameEvent) {
        let me = self.view().player_id;
        match e {
            GameEvent::Chat(ChatEvent::Said { speaker, text }) => {
                let who = match speaker {
                    Some(s) if Some(*s) == me => "itself".to_owned(),
                    Some(s) => format!("player {s:#018x}"),
                    None => "someone".to_owned(),
                };
                self.tell(format!("hears {who} say \"{text}\""));
            }
            GameEvent::Chat(ChatEvent::Told { name, text, .. }) => {
                self.tell(format!("is told by {name} \"{text}\""));
            }
            GameEvent::Chat(ChatEvent::UnknownCommand { text }) => {
                self.tell(format!("no command is known for \"{text}\""));
            }
            GameEvent::Social(SocialEvent::RecruitmentOffered { .. }) if self.agree => {
                self.tell("takes the place offered in a fellowship");
                self.act(Action::Social(SocialAction::AcceptRecruitment));
            }
            GameEvent::Social(SocialEvent::OathOffered { .. }) if self.agree => {
                self.tell("takes the oath offered");
                self.act(Action::Social(SocialAction::TakeOath));
            }
            GameEvent::Trade(TradeEvent::Registered { asker, .. })
                if self.agree && Some(*asker) != me =>
            {
                self.tell(format!("opens the trade with {asker:#018x}"));
                self.trade_accepted = false;
                self.act(Action::Trade(TradeAction::Open(*asker)));
            }
            GameEvent::Trade(TradeEvent::Opened) if self.offer => {
                let first = self
                    .view()
                    .inventory
                    .as_ref()
                    .and_then(|i| i.items.iter().find(|(_, it)| !it.worn).map(|(id, _)| *id));
                if let Some(item) = first {
                    self.tell(format!("offers {item:#018x}"));
                    self.act(Action::Trade(TradeAction::Offer { item, quantity: 1 }));
                }
            }
            GameEvent::Trade(TradeEvent::Accepted { by })
                if self.agree && !self.trade_accepted && Some(*by) != me =>
            {
                self.tell("accepts the trade");
                self.trade_accepted = true;
                self.act(Action::Trade(TradeAction::Accept));
            }
            GameEvent::Effect(EffectEvent::Added { effect }) => {
                self.tell(format!("an effect is put on: id {effect}"));
            }
            GameEvent::Effect(EffectEvent::Removed { effect }) => {
                self.tell(format!("an effect is taken off: id {effect}"));
            }
            GameEvent::Craft(CraftEvent::Done {
                recipe,
                error,
                shown,
            }) => {
                self.tell(format!(
                    "the craft of {recipe:08X} is answered: error {error}, shown {shown}"
                ));
            }
            _ => {}
        }
    }

    /// The other players around, told as they come and as they move.
    fn watch(&mut self) {
        let v = self.driver.view();
        let me = v.player_id;
        let seen: Vec<(u64, u32, [f32; 3])> = v
            .world
            .iter()
            .flat_map(|w| &w.things)
            .filter(|o| o.data_id == PLAYER_TEMPLATE && Some(o.id) != me)
            .filter_map(|o| o.place.map(|(cell, at)| (o.id, cell, at)))
            .collect();
        for (id, cell, at) in seen {
            let moved = self.others.get(&id).is_none_or(|(c, a)| {
                *c != cell || (a[0] - at[0]).powi(2) + (a[1] - at[1]).powi(2) > 0.25
            });
            if moved {
                let first = !self.others.contains_key(&id);
                let how = if first { "sees" } else { "sees moved" };
                self.tell(format!(
                    "{how} player {id:#018x} at {cell:08X} ({:.1}, {:.1})",
                    at[0], at[1]
                ));
                self.others.insert(id, (cell, at));
            }
        }
    }

    /// A making's second half: the recipe as the ingredient panel read it, each slot filled with
    /// the first item the scripts say fits it, and the making asked for; the panel closed again.
    fn fill(&mut self, recipe: u32, spinner: i32) {
        self.print_slots();
        let read = self
            .view()
            .craft
            .as_ref()
            .and_then(|c| c.opened.clone())
            .filter(|o| o.recipe == recipe);
        let Some(read) = read else {
            self.tell(format!("the scripts give no recipe {recipe:08X}"));
            self.act(Action::Craft(CraftAction::Inquire {
                recipe: 0,
                spinner: 0,
            }));
            return;
        };
        self.slots = read
            .ingredients
            .iter()
            .filter_map(|i| {
                read.items
                    .iter()
                    .find(|item| read.fits.get(&(i.id, **item)) == Some(&true))
                    .map(|item| (i.id, *item))
            })
            .collect();
        self.tell(format!(
            "makes {recipe:08X} at {spinner}: asked for slots {:?}, filled {:?}, cost {}, \
             difficulty {}",
            read.ingredients.iter().map(|i| i.id).collect::<Vec<_>>(),
            self.slots.iter().map(|(o, _)| *o).collect::<Vec<_>>(),
            read.cost,
            read.difficulty,
        ));
        for (ordinal, item) in self.slots.clone() {
            self.act(Action::Craft(CraftAction::AddIngredient {
                recipe,
                ordinal,
                item,
                spinner,
            }));
        }
        self.act(Action::Craft(CraftAction::ExecuteCraft {
            recipe,
            spinner,
            ingredients: self.slots.clone(),
        }));
        self.act(Action::Craft(CraftAction::Inquire {
            recipe: 0,
            spinner: 0,
        }));
    }

    /// What the slots of the last making hold now.
    fn print_slots(&mut self) {
        let held: Vec<(u32, Option<u32>)> = self
            .slots
            .iter()
            .map(|(ordinal, item)| {
                let q = self
                    .view()
                    .inventory
                    .as_ref()
                    .and_then(|i| i.items.get(item))
                    .map(|i| i.quantity);
                (*ordinal, q)
            })
            .collect();
        for (ordinal, q) in held {
            self.lines
                .push(format!("        slot {ordinal} after: {q:?}"));
        }
    }
}

/// The character the client makes when the account has none of the name: a human of the first
/// start area.
fn new_character(name: &str) -> NewCharacter {
    NewCharacter {
        name: name.encode_utf16().collect(),
        entity: 0x4700_110F,
        start_area: 0x4000_0001,
        race: 1,
        sex: 0x1000,
        class: 2,
        implement: 1,
        physique: vec![(1, 1.0), (2, 1.0)],
        physique_size_class: 2,
    }
}
