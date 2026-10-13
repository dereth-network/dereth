// Checks of what the world half refuses another world, and what it keeps from being held up: on a
// link in memory, with a clock the test turns, each from a fresh world.

namespace Dereth.Network.Harness;

internal static class GuardChecks
{
    private const string S = "units/guards";

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    private sealed class Clock
    {
        public double Now = 1000;
    }

    /// <summary>Letters only, different for each `i`.</summary>
    internal static string Minted(int i)
    {
        var n = "Mint";
        do
        {
            n += (char)('a' + i % 26);
            i /= 26;
        } while (i > 0);
        return n;
    }

    /// <summary>A world `alpha`, linked and welcomed with `bravo` online, and `names` online on it, each on the network.</summary>
    private static (TestWorld W, MemoryLink Link, Clock Clock, uint[] Ids) World(params string[] names) =>
        World(new NetworkStore(), 0, names);

    private static (TestWorld W, MemoryLink Link, Clock Clock, uint[] Ids) World(NetworkStore store, uint minAge,
        params string[] names)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now, store: store, minAge: minAge);
        var link = (MemoryLink)w.Link;
        var ids = names.Select((n, i) => w.Host.Add(0x5000_0001u + (uint)i, n).Id).ToArray();
        link.Up(1);
        w.Pump();
        link.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test",
            [new WorldInfo("alpha", "ALPHA", true), new WorldInfo("bravo", "BRAVO", true)], [Wire.GlobalRoom], Limits.Default));
        w.Pump();
        foreach (var id in ids)
            w.World.Dn(id, ["on"]);
        w.Pump();
        return (w, link, clock, ids);
    }

    public static void Run(Checks c)
    {
        Review(c);
        WatchesAndPresence(c);
        StandInIds(c);
        ReceivedPace(c);
        Invitations(c);
        Sends(c);
        WorldNames(c);
        Answers(c);
        Blocks(c);
        Saves(c);
    }

    private static void Answers(Checks c)
    {
        // A hub that does not pace one world: 52 tells a second for two minutes, from invented
        // names to invented names here.
        var (w, link, clock, ids) = World("Bob");
        var bob = ids[0];
        var n = 0;
        for (var step = 0; step < 480; step++)
        {
            for (var i = 0; i < 13; i++, n++)
                link.Frame(1, new Tell($"f{n}", A($"{Minted(n)}@bravo"), A($"{Minted(n)}@alpha"), "hi", false));
            clock.Now += 0.25;
            w.Pump();
        }
        c.Check(S, w.World.AnswersOwed <= NetworkWorld.MaxAnswersPerWorld,
            $"a world flooding this one is owed at most its share of answers (owed {w.World.AnswersOwed})");
        var sent = link.Sent.Count;
        w.World.TellByName(bob, "Ann@bravo", "still there?");
        link.Frame(1, new Tell("c1", A("Cid@charlie"), A("Bob@alpha"), "hello from charlie", false));
        clock.Now += 0.25;
        w.Pump();
        var now = link.SentMessages().Skip(sent).ToList();
        c.Check(S, now.OfType<Tell>().Any(t => t.Text == "still there?"), "this world's own tell goes ahead of what it owes the flooder");
        c.Check(S, now.OfType<TellResult>().Any(r => r is { Id: "c1", Outcome: Outcome.Delivered }),
            "another world's answer does not wait behind the flooder's");

        // A pace refusal says nothing a delivered tell would not: characters online and off the
        // network answer as nobody does.
        var (o, ol, _, os) = World("Bob", "Cid");
        o.World.Dn(os[0], ["off"]);
        o.World.Dn(os[1], ["off"]);
        foreach (var target in new[] { "Cid@alpha", "Bob@alpha", "Nobody@alpha" })
        {
            var before = ol.SentMessages().OfType<TellResult>().Count();
            for (var i = 0; i < 25; i++)
                ol.Frame(1, new Tell($"{target}{i}", A($"{Minted(i)}@bravo"), A(target), "hi", false));
            o.Pump();
            var got = ol.SentMessages().OfType<TellResult>().Skip(before).ToList();
            c.Check(S, got.Count == 25 && got.All(r => r.Outcome == Outcome.Offline),
                $"{target}: every tell is answered offline ({string.Join(" ", got.Select(r => Wire.Word(r.Outcome)))})");
        }

        // Out of new names: held back, not offline.
        var (b, bl, bc, bs) = World("Bob");
        bl.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test", [], [Wire.GlobalRoom],
            Limits.Default with { TellsPerRecipient = new Rate(1e6, 1e6) }));
        b.Pump();
        var r0 = bl.SentMessages().OfType<TellResult>().Count();
        for (var i = 0; i < 210; i++)
        {
            bl.Frame(1, new Tell($"x{i}", A($"{Minted(i)}@bravo"), A("Bob@alpha"), "hi", false));
            b.Pump();
        }
        bc.Now += 2;
        b.Pump();
        var limited = bl.SentMessages().OfType<TellResult>().Skip(r0).Count(r => r.Outcome == Outcome.RateLimited);
        var budget = (int)NetworkWorld.NewStandInsPerWorld.Burst;
        c.Check(S, b.Host.To(bs[0]).Count(x => x.Kind == "tell") == budget && limited == 210 - budget,
            $"a world out of new names is answered rate_limited, not offline (limited {limited})");

        // One character's crowd of watchers cannot push out another's presence.
        var (f, fl, fc, fs) = World("Bob", "Bot");
        f.Pump();
        var p0 = fl.SentMessages().OfType<Presence>().Count();
        fl.Frame(1, new Watch(A("Ann@bravo"), A("Bob@alpha"), true));
        for (var i = 0; i < NetworkWorld.MaxChatter + 500; i++)
            fl.Frame(1, new Watch(A($"{Minted(i)}@bravo"), A("Bot@alpha"), true));
        f.Pump();
        fc.Now += 60;
        f.Pump();
        c.Check(S, fl.SentMessages().OfType<Presence>().Skip(p0)
                .Count(p => p.To.ToString() == "Ann@bravo" && p.Of.ToString() == "Bob@alpha") == 1,
            "a crowd watching one character does not push out another's answer");

        // A character lists at most the client's own number of remote friends.
        var (l, _, _, ls) = World("Ann");
        for (var i = 0; i < NetworkWorld.MaxRemoteFriends; i++)
            l.World.AddFriend(ls[0], $"{Minted(i)}@bravo");
        l.World.AddFriend(ls[0], "OneTooMany@bravo");
        c.Check(S, l.Host.To(ls[0]).Last().Kind == "friends-full" && l.World.FriendRows(ls[0]).Count == NetworkWorld.MaxRemoteFriends,
            "the friend past the limit is refused as a full list");

        // Held back by the world's share: the sender is told so.
        var (h, hl, _, hs) = World("Ann");
        h.World.TellByName(hs[0], "Bob@bravo", "hi");
        h.Pump();
        hl.Frame(1, new TellResult(hl.SentMessages().OfType<Tell>().Last().Id, Outcome.RateLimited, null));
        h.Pump();
        c.Check(S, h.Host.To(hs[0]).Last().Text
                == "The Dereth Network is holding back tells from this world to Bob@bravo for now. Wait a moment and try again.",
            "a rate_limited answer says the world's share is held back");
    }

    private static void Blocks(Checks c)
    {
        // Friends online on both sides; the operator blocks the other world.
        var (w, link, _, ids) = World("Ann");
        var ann = ids[0];
        w.World.AddFriend(ann, "Bob@bravo");
        link.Frame(1, new Watch(A("Bob@bravo"), A("Ann@alpha"), true));
        link.Frame(1, new Presence(A("Ann@alpha"), A("Bob@bravo"), true));
        w.Pump();
        var (m, s0) = (w.Host.To(ann).Count, link.Sent.Count);
        c.Check(S, w.World.Admin(["block", "bravo"]) == "bravo is blocked for everyone on this world.", "the operator blocks a world");
        w.Pump();
        var shown = w.Host.To(ann).Skip(m).ToList();
        var told = link.SentMessages().Skip(s0).ToList();
        c.Check(S, shown.Count == 1 && shown[0] is { Kind: "friend", Row: FriendRowKind.Status, Name: "Bob@bravo", Online: false },
            "the blocked world's characters show offline at once, without a line");
        c.Check(S, told.OfType<Presence>().Any(p => p.To.ToString() == "Bob@bravo" && !p.Online)
                && told.OfType<Watch>().Any(x => x.Target.ToString() == "Bob@bravo" && !x.On),
            "the blocked world is told once that this world's characters are offline, and is no longer watched");
        var s1 = link.Sent.Count;
        w.World.OnLogout(ann, "Ann");
        w.World.OnLogin(ann);
        w.World.AddFriend(ann, "Cat@bravo");
        w.Pump();
        c.Check(S, link.SentMessages().Skip(s1).All(x => x is not (Watch or Presence)),
            "nothing more goes to the blocked world: not a logout, not a login, not a watch");
        var s2 = link.Sent.Count;
        c.Check(S, w.World.Admin(["unblock", "bravo"]) == "bravo is no longer blocked.", "and lifts the block");
        w.Pump();
        var again = link.SentMessages().Skip(s2).ToList();
        c.Check(S, again.OfType<Watch>().Any(x => x.Target.ToString() == "Bob@bravo" && x.On)
                && again.OfType<Presence>().Any(p => p.To.ToString() == "Bob@bravo" && p.Online),
            "lifting it watches the friends again and shows them what they may see");

        // A watcher the character does not list: what the block took away, its lifting gives back.
        var (v, vl, _, vs) = World("Ann");
        vl.Frame(1, new Watch(A("Dan@bravo"), A("Ann@alpha"), true));
        v.Pump();
        v.World.Admin(["block", "bravo"]);
        v.Pump();
        var v0 = vl.Sent.Count;
        v.World.Admin(["unblock", "bravo"]);
        v.Pump();
        c.Check(S, vl.SentMessages().Skip(v0).OfType<Presence>().Any(p => p.To.ToString() == "Dan@bravo" && p.Online),
            "lifting a block shows a watcher the character does not list what it may see again");
        var v1 = vl.Sent.Count;
        v.World.OnLogout(vs[0], "Ann");
        v.Pump();
        c.Check(S, vl.SentMessages().Skip(v1).OfType<Presence>().Any(p => p.To.ToString() == "Dan@bravo" && !p.Online),
            "and goes on telling it");
    }

    /// <summary>What a world or hub that writes its own frames cannot make this one do, nor its own players.</summary>
    private static void Review(Checks c)
    {
        // An away text longer than the wire takes is cut to it.
        {
            var (w, link, _, ids) = World("Bob");
            w.Host.Characters[ids[0]].Away = new string('z', 3000);
            link.Frame(1, new Tell("h1", A("Ann@bravo"), A("Bob@alpha"), "around?", false));
            w.Pump();
            var r = link.SentMessages().OfType<TellResult>().LastOrDefault();
            c.Check(S, r is { Outcome: Outcome.Afk } && r.Message is { } m && Text.CharCount(m) == TextRules.MaxTextChars
                && Wire.Read(Wire.Write(r)).Status == ReadStatus.Ok, "an away text longer than the wire takes is cut to it");
        }
        // A tell from this world's own address is refused and shown to no one.
        {
            var (w, link, _, ids) = World("Ann", "Dee");
            link.Frame(1, new Tell("h2", A("Ann@alpha"), A("Dee@alpha"), "I am Ann", false));
            w.Pump();
            c.Check(S, !w.Host.To(ids[1]).Any(x => x.Kind == "tell")
                && link.SentMessages().OfType<TellResult>().Last() is { Id: "h2", Outcome: Outcome.Offline },
                "a tell from this world's own address is refused and shown to no one");
        }
        // Channel lines and invitations are held to a tell's pace.
        {
            var (w, link, clock, ids) = World("Ann");
            for (var i = 0; i < 60; i++)
                link.Frame(1, new ChannelLine("floodchannel", A($"{Minted(i)}@bravo"), A("Ann@alpha"), "buy gold"));
            w.Pump();
            var lines = w.Host.To(ids[0]).Count(x => x.Kind == "tell");
            clock.Now += 60;
            w.Pump();
            var m0 = w.Host.To(ids[0]).Count;
            for (var i = 0; i < 60; i++)
                link.Frame(1, new ChannelInvite($"spam{i}", A($"{Minted(i)}@bravo"), A("Ann@alpha")));
            w.Pump();
            var invites = w.Host.To(ids[0]).Skip(m0).Count(x => x.Text.Contains("invites you", StringComparison.Ordinal));
            var burst = (int)Limits.Default.TellsPerRecipient.Burst;
            c.Check(S, lines == burst && invites == burst,
                $"channel lines and invitations are held to a tell's pace ({lines} lines, {invites} invitations; {burst} each)");
        }
        // A friend's world flipping its answer is shown at a pace, then its latest state.
        {
            var (w, link, clock, ids) = World("Ann");
            w.World.AddFriend(ids[0], "Bob@bravo");
            var m0 = w.Host.To(ids[0]).Count;
            for (var i = 0; i < 400; i++)
            {
                link.Frame(1, new Presence(A("Ann@alpha"), A("Bob@bravo"), i % 2 == 1));
                if (i % 50 == 49)
                {
                    w.Pump();
                    clock.Now += 1;
                }
            }
            for (var i = 0; i < 40; i++)
            {
                clock.Now += 0.25;
                w.Pump();
            }
            var rows = w.Host.To(ids[0]).Skip(m0).Where(x => x.Kind == "friend").ToList();
            var most = (int)(NetworkWorld.PresencePace.Burst + 19 * NetworkWorld.PresencePace.PerSec);
            c.Check(S, rows.Count > 0 && rows.Count <= most && rows.Last().Online && w.World.ShownStatus(ids[0], "bob@bravo") == true,
                $"a friend's world flipping its answer is shown at a pace, then its latest state ({rows.Count} rows, at most {most})");
        }
        // Watches past a world's share are not answered.
        {
            var (w, link, clock, ids) = World("Bob");
            var p0 = link.SentMessages().OfType<Presence>().Count();
            for (var i = 0; i < 300; i++)
                link.Frame(1, new Watch(A($"{Minted(i)}@bravo"), A("Bob@alpha"), true));
            w.Pump();
            w.Pump();
            // What was taken goes out at bravo's share of the pace.
            clock.Now += 1;
            w.Pump();
            var answered = link.SentMessages().OfType<Presence>().Count() - p0;
            c.Check(S, answered == (int)Limits.Default.WatchesPerWorldPair.Burst,
                $"a world sending watches past its share is answered only its share ({answered})");
        }
        // Watch floods from many worlds do not hold up another world's answer.
        {
            var (w, link, clock, ids) = World("Ann");
            var flooders = new[] { "ba", "bb", "bc", "bd", "be" };
            var n = 0;
            void Flood()
            {
                foreach (var f in flooders)
                    for (var k = 0; k < 3; k++, n++)
                        link.Frame(1, new Watch(A($"{Minted(n)}@{f}"), A("Ann@alpha"), true));
                clock.Now += 0.25;
                w.Pump();
            }
            for (var i = 0; i < 240; i++)
                Flood();
            var queued = w.World.Queued;
            var s0 = link.Sent.Count;
            link.Frame(1, new Watch(A("Cid@cc"), A("Ann@alpha"), true));
            Flood();
            Flood();
            c.Check(S, queued > 100 && link.SentMessages().Skip(s0).OfType<Presence>().Any(p => p.To.ToString() == "Cid@cc"),
                $"watch floods from many worlds do not hold up another world's answer ({queued} waiting)");
        }
        // Squelch lists are capped.
        {
            var (w, _, _, ids) = World("Ann");
            for (var i = 0; i <= NetworkWorld.MaxRemoteSquelches; i++)
                w.World.Squelch(ids[0], true, 0, $"{Minted(i)}@bravo");
            var last = w.Host.To(ids[0]).Last().Text;
            for (var i = 0; i <= NetworkWorld.MaxSquelchedWorlds; i++)
                w.World.Dn(ids[0], ["squelch", "w" + Minted(i)[4..].ToLowerInvariant() + "x"]);
            c.Check(S, last == "You squelch 100 characters on other worlds already. Unsquelch one first."
                && w.Host.To(ids[0]).Last().Text == "You squelch 50 worlds already. Unsquelch one first.",
                "a character squelches at most so many remote characters and worlds");
        }
        // Stand-in ids nothing names are forgotten a day after their last use, and past the cap.
        {
            var (w, link, clock, ids) = World("Ann");
            w.World.AddFriend(ids[0], "Bob@bravo");
            var kept = w.World.StandInCount;
            for (var i = 0; i < 300; i++)
            {
                var name = $"{Minted(i)}@zz";
                w.World.AddFriend(ids[0], name);
                w.World.RemoveFriend(ids[0], w.Host.To(ids[0]).Last(x => x.Kind == "friend").Id);
            }
            clock.Now += 61;
            w.Pump();
            var lately = w.World.StandInCount;
            clock.Now += NetworkWorld.StandInIdleSecs;
            w.Pump();
            c.Check(S, lately == kept + 300 && w.World.StandInCount == kept,
                $"invented friends leave no stand-in id once unused for a day ({kept} kept, {lately} lately, {w.World.StandInCount} after)");
        }
        // One pump hands the world at most its budget of frames.
        {
            var (w, link, _, ids) = World("Ann");
            for (var i = 0; i < 1000; i++)
                link.FrameText(1, "{\"type\":\"a.later.type\"}");
            w.Pump();
            var left = 0;
            while (link.TryTake(out _))
                left++;
            c.Check(S, left == 1000 - NetworkWorld.FramesPerPump, $"one pump hands the world at most its budget of frames ({left} left)");
        }
        // A save copies what changed since the last, not the whole store.
        {
            var st = new NetworkStore();
            st.Characters[1] = new CharacterRecord { Settings = CharacterSettings.OnNetwork };
            st.Characters[2] = new CharacterRecord { Settings = CharacterSettings.OnNetwork };
            st.StandIns.IdFor(A("Bob@bravo"));
            var first = st.Snapshot(null, null, -1);
            st.Characters[1].Friends["bob@bravo"] = A("Bob@bravo");
            var second = st.Snapshot(first, new HashSet<uint> { 1 }, st.StandIns.Version);
            c.Check(S, ReferenceEquals(second.Characters[2], first.Characters[2])
                && !ReferenceEquals(second.Characters[1], first.Characters[1]) && second.Characters[1].Friends.Count == 1
                && first.Characters[1].Friends.Count == 0 && ReferenceEquals(second.StandIns, first.StandIns),
                "a save copies what changed since the last, and shares the rest with the last copy");
            st.StandIns.IdFor(A("Cat@bravo"));
        }
    }

    private static void Saves(Checks c)
    {
        // New ids faster than the save pace: a crash never gives one again.
        var (w, link, _, ids) = World("Bob");
        link.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test", [], [Wire.GlobalRoom],
            Limits.Default with { TellsPerRecipient = new Rate(1e6, 1e6) }));
        w.Pump();
        var ok = true;
        var n = 0;
        // Twelve worlds, each within its share of answers owed and of new names.
        foreach (var world in new[]
                 { "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima", "mike" })
        {
            for (var i = 0; i < NetworkWorld.MaxAnswersPerWorld; i++, n++)
                link.Frame(1, new Tell($"s{n}", A($"{Minted(n)}@{world}"), A("Bob@alpha"), "hi", false));
            w.Pump();
            var saved = w.Saved is null ? null : NetworkStore.FromJson(w.Saved);
            ok &= saved is not null && saved.StandIns.Next > StandIns.First + (uint)w.World.StandInCount - 1;
        }
        c.Check(S, ok && w.World.StandInCount == 12 * NetworkWorld.MaxAnswersPerWorld,
            $"new names from twelve worlds within one save interval are all past what a restart would give ({w.World.StandInCount} given)");
    }

    private static void WatchesAndPresence(Checks c)
    {
        // A watch for a character logged off, one never on the network, and one blocked.
        var (w, link, clock, ids) = World("Ann", "Cid");
        var (ann, cid) = (ids[0], ids[1]);
        w.World.Dn(cid, ["off"]);
        w.Host.Characters[ann].Online = false;
        var before = link.SentMessages().OfType<Presence>().Count();
        link.Frame(1, new Watch(A("Eve@bravo"), A("Ann@alpha"), true));
        link.Frame(1, new Watch(A("Eve@bravo"), A("Cid@alpha"), true));
        link.Frame(1, new Watch(A("Eve@bravo"), A("Nobody@alpha"), true));
        w.Pump();
        var answers = link.SentMessages().OfType<Presence>().Skip(before).ToList();
        c.Check(S, answers.Select(p => $"{p.Of}={p.Online}").SequenceEqual(["Ann@alpha=False", "Cid@alpha=False", "Nobody@alpha=False"]),
            "a logged-off character, one off the network and a name this world does not have are answered alike, offline, as named: "
            + string.Join(", ", answers.Select(p => $"{p.Of}={p.Online}")));
        w.Host.Characters[ann].Online = true;
        w.World.Admin(["block", "bravo"]);
        before = link.SentMessages().OfType<Presence>().Count();
        link.Frame(1, new Watch(A("Eve@bravo"), A("aNN@alpha"), true));
        w.World.SettingsChanged(ann);
        w.Pump();
        var toBravo = link.SentMessages().OfType<Presence>().Skip(before).Where(p => p.To.World == "bravo").ToList();
        c.Check(S, toBravo.Count == 1 && toBravo[0].Of.ToString() == "aNN@alpha" && !toBravo[0].Online,
            "a blocked world's watch is answered offline as any watch is, and it is told nothing more");
        w.World.Admin(["unblock", "bravo"]);

        // Strangers' watches are capped; presence for a character nobody here lists is not kept.
        // Ten watches a second: within bravo's share of this world's pace.
        for (var i = 0; i < 500; i++)
        {
            link.Frame(1, new Watch(A($"{Minted(i)}@bravo"), A("Ann@alpha"), true));
            link.Frame(1, new Presence(A("Ann@alpha"), A($"{Minted(i)}@bravo"), true));
            if (i % 10 == 9)
            {
                clock.Now += 1;
                w.Pump();
            }
        }
        w.Pump();
        c.Check(S, w.World.WatcherCount(ann) == NetworkWorld.MaxWatchersPerWorld,
            $"one world's strangers are kept as watchers up to its share (got {w.World.WatcherCount(ann)})");
        c.Check(S, w.World.PresenceCount == 0, $"presence nobody lists is not kept (got {w.World.PresenceCount})");

        // Presence is kept for each watcher: Bob's answer to Cat is not Ann's.
        var (x, xl, _, xs) = World("Ann", "Cat");
        x.World.AddFriend(xs[0], "Bob@bravo");
        x.World.AddFriend(xs[1], "Bob@bravo");
        xl.Frame(1, new Presence(A("Ann@alpha"), A("Bob@bravo"), true));
        xl.Frame(1, new Presence(A("Cat@alpha"), A("Bob@bravo"), false));
        x.Pump();
        var rows = (Ann: x.World.FriendRows(xs[0]), Cat: x.World.FriendRows(xs[1]));
        c.Check(S, rows.Ann.Single().Online && !rows.Cat.Single().Online, "each watcher keeps what the home world showed it");
        c.Check(S, !x.Host.To(xs[1]).Any(s => s.Text.Contains("has gone offline", StringComparison.Ordinal)),
            "no line for a state the watcher was never shown");

        // Clearing the friends list stops watching, and changes nothing a former friend sees.
        var (y, yl, _, ys) = World("Ann");
        y.World.AddFriend(ys[0], "Bob@bravo");
        yl.Frame(1, new Watch(A("Bob@bravo"), A("Ann@alpha"), true));
        y.Pump();
        var p0 = yl.Sent.Count;
        y.World.RemoveAllFriends(ys[0]);
        y.Pump();
        var cleared = yl.SentMessages().Skip(p0).ToList();
        c.Check(S, cleared.OfType<Watch>().Any(x => x.Target.ToString() == "Bob@bravo" && !x.On) && !cleared.OfType<Presence>().Any(),
            "a cleared friend is no longer watched, and still sees the character as it may: "
            + string.Join(" ", cleared.Select(m => m.Type)));

        // A flood of presence answers does not hold up a tell.
        var (z, zl, zc, zs) = World("Ann");
        for (var i = 0; i < 2000; i++)
            zl.Frame(1, new Watch(A($"{Minted(i)}@bravo"), A("Ann@alpha"), true));
        z.Pump();
        var sentBefore = zl.Sent.Count;
        z.World.TellByName(zs[0], "Bob@bravo", "first in line");
        zc.Now += 1;
        z.Pump();
        c.Check(S, zl.SentMessages().Skip(sentBefore).OfType<Tell>().Any(t => t.Text == "first in line"),
            "a tell goes ahead of the presence answers");
    }

    private static void StandInIds(Checks c)
    {
        // The block with one id left: the next new correspondent is not shown under the last id's.
        var (restored, rl, _, rs0) = World(new NetworkStore { StandIns = StandIns.StartingAt(StandIns.Last) }, 0, "Ann");
        var rann = rs0[0];
        rl.Frame(1, new Tell("e1", A("Eve@bravo"), A("Ann@alpha"), "I got the last id", false));
        rl.Frame(1, new Tell("a1", A("Ari@bravo"), A("Ann@alpha"), "and me?", false));
        restored.Pump();
        var tells = restored.Host.To(rann).Where(x => x.Kind == "tell").ToList();
        var results = rl.SentMessages().OfType<TellResult>().ToList();
        c.Check(S, tells.Count == 1 && tells[0].Name == "Eve@bravo" && tells[0].Id == StandIns.Last
            && results.Any(r => r is { Id: "a1", Outcome: Outcome.Offline }),
            "once the block is used up a new correspondent is answered offline, not shown under another's id: "
            + string.Join(", ", tells.Select(t => $"{t.Name}={t.Id:x}")));

        // A room line nobody hears gives no id; a world inventing names brings only so many.
        var (r, rlink, _, rs) = World("Bob");
        // A hub whose room pace lets every line through, so the stand-in ids alone hold them.
        rlink.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test", [], [Wire.GlobalRoom],
            Limits.Default with { RoomLinesPerWorld = new Rate(1e6, 1e6) }));
        r.Pump();
        r.Host.Characters[rs[0]].ListensToRoom = false;
        rlink.Frame(1, new RoomLine("global", A("Eve@bravo"), "free gold"));
        r.Pump();
        c.Check(S, r.World.StandInCount == 0, "a room line nobody here hears gives its speaker no stand-in id");
        r.Host.Characters[rs[0]].ListensToRoom = true;
        for (var i = 0; i < 300; i++)
            rlink.Frame(1, new RoomLine("global", A($"{Minted(i)}@bravo"), "free gold"));
        r.Pump();
        var shown = r.Host.To(rs[0]).Count(x => x.Kind == "room");
        c.Check(S, shown == (int)NetworkWorld.NewStandInsPerWorld.Burst && r.World.StandInCount == shown,
            $"one world's new names are shown up to its allowance (shown {shown})");
    }

    private static void ReceivedPace(Checks c)
    {
        // A hub that does not pace one world's room lines or tells to one character: this world does.
        var (w, link, _, ids) = World("Bob");
        var bob = ids[0];
        for (var i = 0; i < 15; i++)
            link.Frame(1, new RoomLine("global", A("Eve@bravo"), "free gold"));
        for (var i = 0; i < 25; i++)
            link.Frame(1, new Tell($"h{i}", A($"{Minted(i)}@bravo"), A("Bob@alpha"), "hi", false));
        w.Pump();
        var lines = w.Host.To(bob).Count(x => x.Kind == "room");
        var tells = w.Host.To(bob).Count(x => x.Kind == "tell");
        var limited = link.SentMessages().OfType<TellResult>().Count(r => r.Outcome == Outcome.RateLimited);
        c.Check(S, lines == (int)Limits.Default.RoomLinesPerWorld.Burst,
            $"one world's room lines are held to its pace (shown {lines})");
        c.Check(S, tells == (int)Limits.Default.TellsPerRecipient.Burst && limited == 25 - tells,
            $"one world's tells to one character are held to its pace, the rest answered rate_limited (shown {tells}, limited {limited})");
    }

    private static void Invitations(Checks c)
    {
        var (w, link, _, ids) = World("Bob");
        var bob = ids[0];
        int Invites() => w.Host.To(bob).Count(x => x.Text.Contains("invites you", StringComparison.Ordinal));
        w.World.Dn(bob, ["off"]);
        link.Frame(1, new ChannelInvite("freegold", A("Eve@bravo"), A("Bob@alpha")));
        w.Pump();
        c.Check(S, Invites() == 0, "an invitation does not reach a character off the network");
        w.World.Dn(bob, ["on"]);
        w.World.Squelch(bob, true, 0, "Eve@bravo");
        link.Frame(1, new ChannelInvite("freegold", A("Eve@bravo"), A("Bob@alpha")));
        w.Pump();
        c.Check(S, Invites() == 0, "nor past a squelch of the inviter");
        w.World.Squelch(bob, false, 0, "Eve@bravo");
        w.World.Dn(bob, ["squelch", "bravo"]);
        link.Frame(1, new ChannelInvite("freegold", A("Eve@bravo"), A("Bob@alpha")));
        w.Pump();
        c.Check(S, Invites() == 0, "nor past a squelch of the inviter's world");
        w.World.Dn(bob, ["unsquelch", "bravo"]);
        link.Frame(1, new ChannelInvite("freegold", A("Eve@bravo"), A("Bob@alpha")));
        w.Pump();
        c.Check(S, Invites() == 1 && w.Host.To(bob).Last().Text
                == "Eve@bravo invites you to the channel freegold@channel. Type @dn channel join freegold to join it.",
            "and reaches a character on the network from a stranger: " + w.Host.To(bob).Last().Text);
    }

    private static void Sends(Checks c)
    {
        // A tell the link lost on its way goes again on the next link.
        var (w, link, clock, ids) = World("Ann");
        var ann = ids[0];
        w.World.TellByName(ann, "Bob@bravo", "did this get through?");
        w.Pump();
        link.Down(1);
        w.Pump();
        link.Up(2);
        w.Pump();
        link.Frame(2, new Welcome(Wire.Protocol, "alpha", "hub test", [], [Wire.GlobalRoom], Limits.Default));
        w.Pump();
        var resent = link.Sent.Where(s => s.Epoch == 2).Select(s => Wire.Read(s.Frame).Message).OfType<Tell>().ToList();
        c.Check(S, resent.Count == 1 && resent[0].Text == "did this get through?", "a tell in flight when the link went goes again");

        // A tell handed to the hub is not given up on before the hub may still deliver it.
        var id = resent[0].Id;
        clock.Now += 95;
        w.Pump();
        c.Check(S, !w.Host.To(ann).Any(x => x.Kind == "not-available"), "not given up on within the hub's hold and answer window");
        link.Frame(2, new TellResult(id, Outcome.Delivered, null));
        w.Pump();
        c.Check(S, w.Host.To(ann).Last() is { Kind: "echo", Text: "did this get through?" }, "and its late answer is still shown");

        // A character too new, or gagged, sends nothing.
        var (young, nl, _, ns) = World(new NetworkStore(), 3600, "Neo");
        var nyou = ns[0];
        young.Host.Characters[nyou].Age = 10;
        var nsent = nl.Sent.Count;
        young.World.RoomSaid(nyou, RoomSlots.Channel(RoomSlot.Roleplay), "hello other worlds");
        young.World.Dn(nyou, ["channel", "create", "newbies"]);
        young.Pump();
        c.Check(S, nl.Sent.Count == nsent && young.Host.To(nyou).Count(x => x.Text.Contains("too new", StringComparison.Ordinal)) == 2,
            "a character too new sends neither a room line nor a channel request, and is told why");
        var (g, gl, _, gs) = World("Gag");
        g.Host.Characters[gs[0]].Gagged = true;
        var gsent = gl.Sent.Count;
        g.World.Dn(gs[0], ["channel", "invite", "oldguard", "Bob@bravo"]);
        g.Pump();
        c.Check(S, gl.Sent.Count == gsent && g.Host.To(gs[0]).Last().Kind == "gagged", "a gagged character makes no channel request");
    }

    private static void WorldNames(Checks c)
    {
        var (w, link, _, ids) = World("Ann");
        link.Frame(1, new WorldChange(new WorldInfo("cc", "<Tell:IIDString:1342177281:+Admin>Admin<\\Tell>\nYou have been banned", true)));
        link.Frame(1, new WorldChange(new WorldInfo("Not A Slug", "Anything", true)));
        w.Pump();
        w.World.Dn(ids[0], ["worlds"]);
        c.Check(S, w.Host.To(ids[0]).Last().Text == NetworkWorld.Help()
            && !w.Host.To(ids[0]).Any(x => x.Text.Contains("banned", StringComparison.Ordinal)),
            "/dn worlds is no command now, and a world's name from the hub is shown to nobody");
    }
}
