//! ACE: Source/ACE.Server/WorldObjects/Player_Tick.cs::UpdatePlayerPosition
//! An instrument: what one watcher receives a second, datagrams, bytes and the players' positions,
//! while 50 players run around it as the client reports, crowded within 20 m of it or spread over
//! its landblock and the eight beside it; and how often the runners' own clients were told to move.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats; flat land.

mod run {
    //! ACE: Source/ACE.Server/WorldObjects/Player_Tick.cs::UpdatePlayerPosition
    use empyrean_entity::Position;
    use empyrean_testkit::{ClientId, TestServer};

    use crate::movement::player_movement::{autonomous_position, outdoor};
    use crate::movement::position_cadence::{
        facing, move_to_state, positions_of, world, RUN_SPEED,
    };

    const WATCHER: u32 = 0x5000_0001;
    const RUNNERS: usize = 50;
    const UPDATE_POSITION: u32 = 0xF748;
    /// The IPv4 and UDP headers each datagram carries beyond its payload.
    const IP_UDP: u64 = 28;
    /// The south-west corner of the 3×3 landblocks round the watcher's (0xA9B4), in landblock
    /// coordinates.
    const WEST: u16 = 0xA8;
    const SOUTH: u16 = 0xB3;
    /// The watcher's spot, global meters from that corner: the middle of its landblock.
    const MIDDLE: (f32, f32) = (290.0, 290.0);
    const STEP: f64 = 1.0 / 60.0;

    /// A global spot from the corner as a position in its landblock, facing north or south.
    fn at(x: f32, y: f32, north: bool) -> Position {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (bx, by) = ((x / 192.0) as u16, (y / 192.0) as u16);
        let lb = (WEST + bx) << 8 | (SOUTH + by);
        facing(
            outdoor(
                lb,
                x - f32::from(bx) * 192.0,
                y - f32::from(by) * 192.0,
                20.0,
            ),
            north,
        )
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Doing {
        Running,
        /// Stopped until the time.
        Stopped(f64),
        /// Turning round at a lane's end until the time.
        Turning(f64),
    }

    struct Runner {
        client: ClientId,
        guid: u32,
        x: f32,
        y: f32,
        lo: f32,
        hi: f32,
        north: bool,
        doing: Doing,
        next_stop: f64,
        last_report: f64,
        cell: u32,
    }

    impl Runner {
        fn here(&self) -> Position {
            at(self.x, self.y, self.north)
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Layout {
        /// North-south lanes within about 20 m of the watcher.
        Crowd,
        /// North-south lanes over the nine landblocks.
        Spread,
    }

    /// Runner `i`'s lane: `(x, y low, y high)`.
    fn lane(layout: Layout, i: usize) -> (f32, f32, f32) {
        #[allow(clippy::cast_precision_loss)]
        let f = (i as f32 + 0.5) / RUNNERS as f32;
        match layout {
            // (off the cells' edges by a little)
            Layout::Crowd => (
                MIDDLE.0 - 20.13 + 40.0 * f,
                MIDDLE.1 - 18.0,
                MIDDLE.1 + 18.0,
            ),
            Layout::Spread => (8.13 + 560.0 * f, 8.0, 568.0),
        }
    }

    fn measure(layout: Layout) -> String {
        let mut players = vec![("watcher".to_owned(), WATCHER, at(MIDDLE.0, MIDDLE.1, true))];
        for i in 0..RUNNERS {
            let (x, lo, _) = lane(layout, i);
            let guid = WATCHER + 1 + u32::try_from(i).expect("few");
            players.push((format!("runner{i}"), guid, at(x, lo, true)));
        }
        let seeds: Vec<(&str, u32, Position)> = players
            .iter()
            .map(|(a, g, p)| (a.as_str(), *g, *p))
            .collect();
        let (mut ts, clients) = world(&seeds);
        let watcher = clients[0];
        let mut runners: Vec<Runner> = (0..RUNNERS)
            .map(|i| {
                let (x, lo, hi) = lane(layout, i);
                #[allow(clippy::cast_precision_loss)]
                let phase = 2.0 * i as f64 / RUNNERS as f64;
                Runner {
                    client: clients[i + 1],
                    guid: WATCHER + 1 + u32::try_from(i).expect("few"),
                    x,
                    y: lo,
                    lo,
                    hi,
                    north: true,
                    doing: Doing::Running,
                    next_stop: 0.0,
                    last_report: 0.0,
                    cell: 0,
                }
                .with_start(&mut ts, phase)
            })
            .collect();

        // warm up, then measure
        run_for(&mut ts, &mut runners, 2.0);
        let wire = ts.received_wire(watcher);
        let mark = ts.received_raw(watcher).len();
        let own_marks: Vec<usize> = runners
            .iter()
            .map(|r| ts.received_raw(r.client).len())
            .collect();
        let secs = 30.0;
        run_for(&mut ts, &mut runners, secs);

        let after = ts.received_wire(watcher);
        let datagrams = after.datagrams - wire.datagrams;
        let bytes = after.bytes - wire.bytes + datagrams * IP_UDP;
        let guids: Vec<u32> = runners.iter().map(|r| r.guid).collect();
        let updates: Vec<usize> = ts.received_raw(watcher)[mark..]
            .iter()
            .filter(|m| {
                m.opcode == UPDATE_POSITION && guids.iter().any(|g| m.body[0..4] == g.to_le_bytes())
            })
            // the opcode and the fragment header
            .map(|m| m.body.len() + 4 + 16)
            .collect();
        let forced: usize = runners
            .iter()
            .zip(&own_marks)
            .map(|(r, &m)| {
                let own = positions_of(&ts, r.client, m, r.guid);
                own.windows(2).filter(|w| w[1].force != w[0].force).count()
            })
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let line = format!(
            "{layout:<6?} total {:>6.0} B/s {:>6.1} datagrams/s; positions {:>6.1}/s {:>6.0} B/s; forced {forced} in {:.0} runner-minutes",
            bytes as f64 / secs,
            datagrams as f64 / secs,
            updates.len() as f64 / secs,
            updates.iter().sum::<usize>() as f64 / secs,
            secs * RUNNERS as f64 / 60.0
        );
        line
    }

    impl Runner {
        /// Starts running north; its first stop comes `phase` seconds on.
        fn with_start(mut self, ts: &mut TestServer, phase: f64) -> Self {
            let now = ts.seconds();
            ts.send_game_action(self.client, &move_to_state(&self.here(), true, false));
            self.last_report = now;
            self.next_stop = now + 1.0 + phase;
            self.cell = self.here().cell();
            self
        }
    }

    /// Runs the runners for `secs`: each runs its lane, stops for 0.4 s every 2 s, turns round at
    /// its lane's ends for 0.25 s, and reports its position as the client does (once a second
    /// while moving, and at once in a new cell).
    fn run_for(ts: &mut TestServer, runners: &mut [Runner], secs: f64) {
        let end = ts.seconds() + secs;
        while ts.seconds() < end {
            ts.advance(STEP);
            let t = ts.seconds();
            #[allow(clippy::cast_possible_truncation)]
            let stride = RUN_SPEED * STEP as f32;
            for r in runners.iter_mut() {
                match r.doing {
                    Doing::Running => {
                        r.y += if r.north { stride } else { -stride };
                        if r.y >= r.hi || r.y <= r.lo {
                            r.y = r.y.clamp(r.lo, r.hi);
                            ts.send_game_action(r.client, &move_to_state(&r.here(), false, true));
                            r.doing = Doing::Turning(t + 0.25);
                            r.last_report = t;
                            continue;
                        }
                        if t >= r.next_stop {
                            ts.send_game_action(r.client, &move_to_state(&r.here(), false, false));
                            r.doing = Doing::Stopped(t + 0.4);
                            r.next_stop = t + 2.0;
                            r.last_report = t;
                            continue;
                        }
                        let cell = r.here().cell();
                        if cell != r.cell || t - r.last_report >= 1.0 {
                            ts.send_game_action(r.client, &autonomous_position(&r.here(), true));
                            r.last_report = t;
                            r.cell = cell;
                        }
                    }
                    Doing::Stopped(until) if t >= until => {
                        ts.send_game_action(r.client, &move_to_state(&r.here(), true, false));
                        r.doing = Doing::Running;
                        r.last_report = t;
                    }
                    Doing::Turning(until) if t >= until => {
                        r.north = !r.north;
                        ts.send_game_action(r.client, &move_to_state(&r.here(), true, false));
                        r.doing = Doing::Running;
                        r.last_report = t;
                    }
                    Doing::Stopped(_) | Doing::Turning(_) => {}
                }
            }
        }
    }

    /// Prints one line a layout.
    #[test]
    #[ignore = "instrument: prints what one watcher receives among 50 runners; run explicitly by name"]
    fn position_load_on_a_watcher_among_50_runners() {
        for layout in [Layout::Crowd, Layout::Spread] {
            println!("{}", measure(layout));
        }
    }
}
