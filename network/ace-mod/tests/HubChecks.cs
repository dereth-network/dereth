// The link client and the world half against a real hub, over loopback only: the hub runs from a
// temporary registry with three worlds (two with federation on, one never set), on a free port of
// 127.0.0.1, and is stopped at the end.

using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text.RegularExpressions;

namespace Dereth.Network.Harness;

internal sealed class HubProcess : IDisposable
{
    private readonly string exe;
    private readonly string dir;
    private readonly string db;
    private Process? serving;
    public readonly List<string> Output = [];
    public int Port { get; private set; }

    /// <summary>Variables the serving hub's environment carries beside this process's own.</summary>
    public readonly Dictionary<string, string> Environment = new(StringComparer.Ordinal);

    public HubProcess(string exe)
    {
        this.exe = exe;
        dir = Path.Combine(Path.GetTempPath(), "dereth-network-hub-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(dir);
        db = Path.Combine(dir, "hub.sqlite");
    }

    private string Run(params string[] args)
    {
        var psi = new ProcessStartInfo(exe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        psi.ArgumentList.Add("--db");
        psi.ArgumentList.Add(db);
        foreach (var a in args)
            psi.ArgumentList.Add(a);
        using var p = Process.Start(psi)!;
        var stdout = p.StandardOutput.ReadToEndAsync();
        var stderr = p.StandardError.ReadToEndAsync();
        if (!p.WaitForExit(30_000))
        {
            p.Kill(true);
            throw new TimeoutException("the hub's command did not finish");
        }
        if (p.ExitCode != 0)
            throw new InvalidOperationException($"the hub's {args[0]} failed ({p.ExitCode}): {stderr.Result}");
        return stdout.Result;
    }

    /// <summary>Registers a world and returns its key (printed once by the hub).</summary>
    public string Register(string slug, string name)
    {
        var file = Path.Combine(dir, slug + ".json");
        File.WriteAllText(file,
            $"{{\"slug\":\"{slug}\",\"name\":\"{name}\",\"description\":\"harness\",\"ruleset\":\"PvE\"," +
            "\"endpoint\":{\"mode\":\"manual\",\"address\":\"192.0.2.1\",\"port\":9000},\"emulator\":{\"family\":\"ACE\"}}");
        var m = Regex.Match(Run("register", file), "\"key\"\\s*:\\s*\"(drth_[A-Za-z0-9_-]{43})\"");
        return m.Success ? m.Groups[1].Value : throw new InvalidOperationException("no key in the hub's answer");
    }

    public void Federation(string slug, string flag) => Run("federation", slug, flag);

    /// <summary>The hub's operator hands a managed world to the operator with Discord id `discordId`, signed in once already.</summary>
    public void Assign(string slug, string discordId) => Run("assign", slug, discordId);

    /// <summary>The hub's operator takes a character out of the directory, from the command line, while the hub runs.</summary>
    public void DirectoryRemove(string address) => Run("directory", "remove", address);

    /// <summary>Serves on a port of 127.0.0.1 (the same one again after a restart), with the options `extra` gives for it.</summary>
    public void Serve(Func<int, IEnumerable<string>>? extra = null)
    {
        if (Port == 0)
        {
            var l = new TcpListener(IPAddress.Loopback, 0);
            l.Start();
            Port = ((IPEndPoint)l.LocalEndpoint).Port;
            l.Stop();
        }
        var psi = new ProcessStartInfo(exe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        foreach (var a in new[] { "--db", db, "serve", "--listen", $"127.0.0.1:{Port}", "--run-for", "900" }
                     .Concat(extra?.Invoke(Port) ?? []))
            psi.ArgumentList.Add(a);
        foreach (var (k, v) in Environment)
            psi.Environment[k] = v;
        serving = Process.Start(psi)!;
        serving.OutputDataReceived += (_, e) => { if (e.Data is not null) lock (Output) Output.Add(e.Data); };
        serving.ErrorDataReceived += (_, e) => { if (e.Data is not null) lock (Output) Output.Add(e.Data); };
        serving.BeginOutputReadLine();
        serving.BeginErrorReadLine();
        var until = DateTime.UtcNow.AddSeconds(15);
        while (DateTime.UtcNow < until)
        {
            try
            {
                using var t = new TcpClient();
                t.Connect(IPAddress.Loopback, Port);
                return;
            }
            catch (SocketException)
            {
                Thread.Sleep(50);
            }
        }
        throw new TimeoutException("the hub did not start listening");
    }

    public void Kill()
    {
        if (serving is null)
            return;
        try
        {
            if (!serving.HasExited)
                serving.Kill(true);
            serving.WaitForExit(10_000);
        }
        catch (InvalidOperationException)
        {
        }
        serving.Dispose();
        serving = null;
    }

    public void Dispose()
    {
        Kill();
        try
        {
            Directory.Delete(dir, true);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }
}

/// <summary>A link used directly, frame by frame, with no world half.</summary>
internal sealed class RawLink(HubLink link)
{
    public readonly HubLink Link = link;
    private readonly List<WireMessage> inbox = [];
    public readonly List<int> Refused = [];
    public long Epoch;
    public bool IsUp;

    private void Drain()
    {
        while (Link.TryTake(out var e))
        {
            switch (e.Kind)
            {
                case LinkEventKind.Up:
                    Epoch = e.Epoch;
                    IsUp = true;
                    break;
                case LinkEventKind.Down:
                    IsUp = false;
                    break;
                case LinkEventKind.Refused:
                    Refused.Add(e.Status);
                    break;
                case LinkEventKind.Frame:
                    var r = Wire.Read(e.Text);
                    if (r.Status == ReadStatus.Ok)
                        inbox.Add(r.Message!);
                    break;
            }
        }
    }

    public async Task<bool> Until(Func<RawLink, bool> cond, double seconds = 10)
    {
        var until = DateTime.UtcNow.AddSeconds(seconds);
        while (DateTime.UtcNow < until)
        {
            Drain();
            if (cond(this))
                return true;
            await Task.Delay(10);
        }
        Drain();
        return cond(this);
    }

    public void Send(WireMessage m) => Link.Send(Epoch, Wire.Write(m));

    public async Task<T?> Next<T>(Func<T, bool> match, double seconds = 10) where T : WireMessage
    {
        T? found = null;
        await Until(_ =>
        {
            found = inbox.OfType<T>().FirstOrDefault(match);
            if (found is not null)
                inbox.Remove(found);
            return found is not null;
        }, seconds);
        return found;
    }
}

internal static class HubChecks
{
    private static readonly LinkTiming Fast = new()
    {
        Backoff = [TimeSpan.FromMilliseconds(200), TimeSpan.FromMilliseconds(300), TimeSpan.FromMilliseconds(500)],
        RefusedWait = TimeSpan.FromSeconds(2),
        ConnectTimeout = TimeSpan.FromSeconds(5),
    };

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    private static async Task<bool> Until(Func<bool> cond, double seconds, params TestWorld[] worlds)
    {
        var until = DateTime.UtcNow.AddSeconds(seconds);
        while (DateTime.UtcNow < until)
        {
            foreach (var w in worlds)
                w.Pump();
            if (cond())
                return true;
            await Task.Delay(10);
        }
        foreach (var w in worlds)
            w.Pump();
        return cond();
    }

    public static async Task Run(Checks c, string exe)
    {
        if (!File.Exists(exe))
        {
            c.Fail("hub", "no hub at " + exe);
            return;
        }
        var log = new List<string>();
        Logger linkLog(string who) => (l, m) =>
        {
            lock (log)
                log.Add($"[{who} link] {l}: {m}");
        };
        using var hub = new HubProcess(exe);
        var keyA = hub.Register("alpha", "Alpha");
        var keyB = hub.Register("bravo", "Bravo");
        var keyC = hub.Register("charlie", "Charlie");
        hub.Federation("alpha", "on");
        hub.Federation("bravo", "on");
        hub.Serve();
        Uri Link(string slug)
        {
            HubUrl.TryLink($"ws://127.0.0.1:{hub.Port}", slug, true, out var u, out _);
            return u;
        }
        var bodies = new List<string>();
        string Body(string what)
        {
            var b = $"{what}-{Guid.NewGuid():N}"[..24];
            bodies.Add(b);
            return b;
        }

        // ---- the link alone ------------------------------------------------------------------
        const string s = "hub/link";
        var ra = new RawLink(new HubLink(Link("alpha"), keyA, linkLog("alpha raw"), Fast));
        var rb = new RawLink(new HubLink(Link("bravo"), keyB, linkLog("bravo raw"), Fast));
        ra.Link.Start();
        rb.Link.Start();
        c.Check(s, await ra.Until(r => r.IsUp) && await rb.Until(r => r.IsUp), "both worlds connect with their keys");
        ra.Send(new Hello(Wire.Protocol, "harness raw"));
        rb.Send(new Hello(Wire.Protocol, "harness raw"));
        var wa0 = await ra.Next<Welcome>(_ => true);
        var wb0 = await rb.Next<Welcome>(_ => true);
        c.Check(s, wa0 is { World: "alpha", Proto: 1 } && wa0.Rooms.Contains("global") && wa0.Limits == Limits.Default
            && wb0 is { World: "bravo" }, "hello is answered with welcome: the world's own slug, the rooms and the limits");

        var tellBody = Body("tell");
        ra.Send(new Tell("t-alpha-1", A("Ann@alpha"), A("Bob@bravo"), tellBody, true));
        var atB = await rb.Next<Tell>(t => t.Text == tellBody);
        c.Check(s, atB is not null && atB.Id != "t-alpha-1" && atB.From.ToString() == "Ann@alpha" && atB.To.ToString() == "Bob@bravo"
            && atB.Friend, $"a tell from alpha reaches bravo under the hub's own id ({atB?.Id})");
        if (atB is not null)
            rb.Send(new TellResult(atB.Id, Outcome.Delivered, null));
        var resA = await ra.Next<TellResult>(r => r.Id == "t-alpha-1");
        c.Check(s, resA is { Outcome: Outcome.Delivered }, "bravo's answer reaches alpha under alpha's own id");

        ra.Send(new RoomJoin("global", true));
        rb.Send(new RoomJoin("global", true));
        await Task.Delay(500);
        var roomBody = Body("room");
        ra.Send(new RoomSay("r-alpha-1", "global", A("Ann@alpha"), roomBody));
        var lineB = await rb.Next<RoomLine>(l => l.Text == roomBody);
        var roomRes = await ra.Next<RoomResult>(r => r.Id == "r-alpha-1");
        c.Check(s, lineB is not null && lineB.From.ToString() == "Ann@alpha" && lineB.Room == "global", "a room line reaches the other world carrying the room");
        c.Check(s, roomRes is { Outcome: Outcome.Ok }, "and the speaker's world hears it was accepted");

        ra.Send(new ChannelRequest("c-alpha-1", ChannelOp.Create, "oldguard", A("Ann@alpha"), null));
        c.Check(s, await ra.Next<ChannelResult>(r => r.Id == "c-alpha-1") is { Outcome: ChannelOutcome.Ok, Channel: "oldguard" }, "a channel is created");
        ra.Send(new ChannelRequest("c-alpha-2", ChannelOp.Invite, "oldguard", A("Ann@alpha"), A("Bob@bravo")));
        var inv = await rb.Next<ChannelInvite>(i => i.Channel == "oldguard");
        c.Check(s, await ra.Next<ChannelResult>(r => r.Id == "c-alpha-2") is { Outcome: ChannelOutcome.Ok }
            && inv is not null && inv.By.ToString() == "Ann@alpha" && inv.To.ToString() == "Bob@bravo", "an invitation reaches the invitee's world");
        rb.Send(new ChannelRequest("c-bravo-1", ChannelOp.Join, "oldguard", A("Bob@bravo"), null));
        c.Check(s, await rb.Next<ChannelResult>(r => r.Id == "c-bravo-1") is { Outcome: ChannelOutcome.Ok }, "the invitee joins");
        var channelBody = Body("channel");
        ra.Send(new Tell("t-alpha-2", A("Ann@alpha"), A("oldguard@channel"), channelBody, false));
        var cl = await rb.Next<ChannelLine>(l => l.Text == channelBody);
        c.Check(s, cl is not null && cl.Channel == "oldguard" && cl.From.ToString() == "Ann@alpha" && cl.To.ToString() == "Bob@bravo",
            "a tell to the channel reaches the other member as a channel line");
        c.Check(s, await ra.Next<TellResult>(r => r.Id == "t-alpha-2") is { Outcome: Outcome.Delivered }, "and the speaker hears it was delivered");
        ra.Send(new ChannelRequest("c-alpha-3", ChannelOp.Members, "oldguard", A("Ann@alpha"), null));
        var members = await ra.Next<ChannelResult>(r => r.Id == "c-alpha-3");
        c.Check(s, members is not null && members.Members.Select(m => m.ToString()).Order().SequenceEqual(["Ann@alpha", "Bob@bravo"]),
            "the channel's members are both");
        ra.Send(new Tell("t-alpha-3", A("Eve@bravo"), A("Bob@bravo"), Body("forged"), false));
        c.Check(s, await ra.Next<ProtocolError>(e => e.Code == "identity") is not null, "a world speaking for another world's character is refused");

        // Persons, on a hub with no sign-in: a character nobody linked has no person, and no code is given.
        ra.Send(new PersonWatch(A("Ann@alpha"), true));
        c.Check(s, await ra.Next<PersonFriends>(v => v.To.ToString() == "Ann@alpha") is { Linked: false, Persons.Count: 0 },
            "a character nobody linked has an empty person view");
        ra.Send(new PersonRequest("p-alpha-1", PersonOp.Characters, A("Ann@alpha"), A("Bob@bravo"), null));
        c.Check(s, await ra.Next<PersonResult>(r => r.Id == "p-alpha-1") is { Outcome: PersonOutcome.Ok, Characters.Count: 0 },
            "and names no characters of another's");
        ra.Send(new PersonRequest("p-alpha-2", PersonOp.Befriend, A("Ann@alpha"), A("Bob@bravo"), null));
        c.Check(s, await ra.Next<PersonResult>(r => r.Id == "p-alpha-2") is { Outcome: PersonOutcome.NotLinked },
            "a person friend needs the asker linked");
        ra.Send(new PersonRequest("p-alpha-3", PersonOp.Code, A("Ann@alpha"), null, null));
        c.Check(s, await ra.Next<PersonResult>(r => r.Id == "p-alpha-3") is { Outcome: PersonOutcome.Unavailable, Code: null },
            "a hub without sign-in gives no link code");
        ra.Send(new PersonRequest("p-alpha-4", PersonOp.Code, A("Eve@bravo"), null, null));
        c.Check(s, await ra.Next<ProtocolError>(e => e.Code == "identity") is not null, "a code for another world's character is refused");

        var bad = new RawLink(new HubLink(Link("alpha"), "drth_" + new string('A', 43), linkLog("alpha bad key"), Fast));
        var unset = new RawLink(new HubLink(Link("charlie"), keyC, linkLog("charlie"), Fast));
        bad.Link.Start();
        unset.Link.Start();
        c.Check(s, await bad.Until(r => r.Refused.Count > 0) && bad.Refused[0] == 401 && !bad.IsUp, "a wrong key is refused 401 before the upgrade");
        c.Check(s, await unset.Until(r => r.Refused.Count > 0) && unset.Refused[0] == 403 && !unset.IsUp,
            "a world whose federation flag was never set is refused 403");
        foreach (var r in new[] { ra, rb, bad, unset })
            r.Link.Stop();
        await Task.Delay(300);

        // ---- the world half through the hub ----------------------------------------------------
        const string w = "hub/worlds";
        var clock = Stopwatch.StartNew();
        var a = TestWorld.Make("alpha", new HubLink(Link("alpha"), keyA, linkLog("alpha"), Fast), () => clock.Elapsed.TotalSeconds,
            RoomSlot.Roleplay, log);
        var b = TestWorld.Make("bravo", new HubLink(Link("bravo"), keyB, linkLog("bravo"), Fast), () => clock.Elapsed.TotalSeconds,
            RoomSlot.Roleplay, log);
        ((HubLink)a.Link).Start();
        ((HubLink)b.Link).Start();
        c.Check(w, await Until(() => a.World.Link == LinkState.Up && b.World.Link == LinkState.Up, 10, a, b), "both worlds are welcomed");
        var ann = a.Host.Add(0x5000_0001, "Ann").Id;
        var bob = b.Host.Add(0x5000_0001, "Bob").Id;
        a.World.Dn(ann, ["on"]);
        b.World.Dn(bob, ["on"]);

        var t1 = Body("hello");
        a.World.TellByName(ann, "Bob@bravo", t1);
        c.Check(w, await Until(() => b.Host.To(bob).Any(x => x.Kind == "tell" && x.Text == t1)
            && a.Host.To(ann).Any(x => x.Kind == "echo" && x.Text == t1), 10, a, b), "a tell by address reaches the other world, and the sender sees the echo");
        var shown = b.Host.To(bob).FirstOrDefault(x => x.Kind == "tell" && x.Text == t1);
        c.Check(w, shown is { Name: "Ann@alpha" } && StandIns.IsStandIn(shown.Id), "it is shown from Ann@alpha under a stand-in id");
        c.Check(w, a.Host.To(ann).First(x => x.Text == t1).Name == "Bob@bravo", "the echo names Bob@bravo");

        var t2 = Body("reply");
        b.World.TellById(bob, shown!.Id, t2);
        c.Check(w, await Until(() => a.Host.To(ann).Any(x => x.Kind == "tell" && x.Text == t2 && x.Name == "Bob@bravo" && StandIns.IsStandIn(x.Id))
            && b.Host.To(bob).Any(x => x.Kind == "echo" && x.Text == t2 && x.Name == "Ann@alpha"), 10, a, b),
            "a reply by the stand-in id goes back to the sender's world");

        var nOff = a.Host.To(ann).Count(x => x.Kind == "not-available");
        a.World.TellByName(ann, "Nobody@bravo", Body("nobody"));
        c.Check(w, await Until(() => a.Host.To(ann).Count(x => x.Kind == "not-available") == nOff + 1, 10, a, b),
            "a tell nobody can take is Character not available");
        a.World.TellByName(ann, "Zed@zulu", Body("zulu"));
        c.Check(w, await Until(() => a.Host.To(ann).Any(x => x.Text == "zulu isn't on the network."), 10, a, b),
            "a tell to a world not on the network says so");

        b.Host.Characters[bob].Away = "brb";
        var t3 = Body("afk");
        a.World.TellByName(ann, "Bob@bravo", t3);
        c.Check(w, await Until(() => a.Host.To(ann).Any(x => x.Kind == "away" && x.Text == "Bob@bravo is away: brb")
            && b.Host.To(bob).Any(x => x.Text == t3), 10, a, b), "an away recipient gets the tell and the sender the away message");
        b.Host.Characters[bob].Away = null;

        b.World.Squelch(bob, true, 0, "Ann@alpha");
        var t4 = Body("squelched");
        a.World.TellByName(ann, "Bob@bravo", t4);
        c.Check(w, await Until(() => a.Host.To(ann).Any(x => x.Kind == "squelched"), 10, a, b)
            && !b.Host.To(bob).Any(x => x.Text == t4), "a squelched sender sees what a local squelch shows, and the tell is not shown");
        b.World.Squelch(bob, false, 0, "Ann@alpha");

        a.World.AddFriend(ann, "Bob@bravo");
        // Bob does not list Ann: on the network, he shows online to her all the same.
        c.Check(w, await Until(() =>
                a.Host.To(ann).Any(x => x is { Kind: "friend", Row: FriendRowKind.Status, Name: "Bob@bravo", Online: true }), 10, a, b),
            "a friend on the network shows online to anyone who lists it, listed back or not");
        b.World.AddFriend(bob, "Ann@alpha");
        c.Check(w, await Until(() =>
                b.Host.To(bob).Any(x => x is { Kind: "friend", Row: FriendRowKind.Status, Name: "Ann@alpha", Online: true }), 10, a, b),
            "and the other way, on two worlds");
        b.Host.Characters[bob].Online = false;
        b.World.OnLogout(bob, "Bob");
        c.Check(w, await Until(() => a.Host.To(ann).Any(x => x.Text.EndsWith("Bob@bravo<\\Tell> has gone offline.", StringComparison.Ordinal)), 10, a, b),
            "a friend logging off is seen by the other world");
        b.Host.Characters[bob].Online = true;
        b.World.OnLogin(bob);
        var comeOnline = a.Host.To(ann).Count(x => x.Text.EndsWith("has come online.", StringComparison.Ordinal));
        c.Check(w, await Until(() => a.Host.To(ann).Count(x => x.Text.EndsWith("has come online.", StringComparison.Ordinal)) == comeOnline + 1, 10, a, b),
            "and logging on again");
        c.Check(w, a.Host.To(ann).Any(x => x.Text.EndsWith("Bob@bravo<\\Tell> has come online.", StringComparison.Ordinal)),
            "with the online line, the name a link");

        var t5 = Body("global");
        a.World.RoomSaid(ann, RoomSlots.Channel(RoomSlot.Roleplay), t5);
        c.Check(w, await Until(() => b.Host.To(bob).Any(x => x.Kind == "room" && x.Text == t5 && x.Name == "Ann@alpha" && StandIns.IsStandIn(x.Id)), 10, a, b),
            "a Global room line reaches the other world's listeners under the speaker's address");
        var tg = Body("dng");
        a.World.Global(ann, tg);
        c.Check(w, await Until(() => b.Host.To(bob).Any(x => x.Kind == "room" && x.Text == tg && x.Name == "Ann@alpha" && StandIns.IsStandIn(x.Id)), 10, a, b)
            && a.Host.To(ann).Any(x => x.Kind == "room" && x.Text == tg && x.Name == "Ann" && x.Id == ann),
            "/dng reaches the other world's listeners, and is shown here in the carrying room as the speaker's own line");

        a.World.Dn(ann, ["channel", "create", "guild"]);
        c.Check(w, await Until(() => a.Host.To(ann).Any(x => x.Text == "You have made the channel guild@channel. Invite someone with @dn channel invite guild Name@world."), 10, a, b), "/dn channel create");
        a.World.Dn(ann, ["channel", "invite", "guild", "Bob@bravo"]);
        c.Check(w, await Until(() => b.Host.To(bob).Any(x => x.Text == "Ann@alpha invites you to the channel guild@channel. Type @dn channel join guild to join it."), 10, a, b),
            "/dn channel invite tells the invitee on the other world");
        b.World.Dn(bob, ["channel", "join", "guild"]);
        c.Check(w, await Until(() => b.Host.To(bob).Any(x => x.Text == "You have joined guild@channel. Talk in it with @tell guild@channel, text."), 10, a, b), "/dn channel join");
        var t6 = Body("guild");
        a.World.TellByName(ann, "guild@channel", t6);
        c.Check(w, await Until(() => b.Host.To(bob).Any(x => x.Kind == "tell" && x.Name == "guild@channel" && x.Text == "Ann@alpha: " + t6)
            && a.Host.To(ann).Any(x => x.Kind == "echo" && x.Text == t6), 10, a, b), "a tell to the channel reaches the member as a tell from the channel");

        using (var http = new HttpClient())
        {
            var status = await http.GetStringAsync($"http://127.0.0.1:{hub.Port}/v1/servers/alpha/status");
            c.Check(w, status.Contains("\"online\"", StringComparison.Ordinal), "the heartbeat over the link reaches the registry");
        }

        // The hub goes away: a tell waits, and goes when the hub is back within the minute.
        hub.Kill();
        c.Check(w, await Until(() => a.World.Link == LinkState.Down && b.World.Link == LinkState.Down, 15, a, b), "both worlds see the hub go");
        c.Check(w, a.Host.To(ann).Last(x => x.Kind == "friend") is { Online: false, Name: "Bob@bravo" },
            "a lost hub shows remote friends offline");
        var t7 = Body("queued");
        a.World.TellByName(ann, "Bob@bravo", t7);
        a.Pump();
        c.Check(w, a.World.Queued == 1, "a tell waits while the hub is away");
        await Task.Delay(1500);
        hub.Serve();
        c.Check(w, await Until(() => b.Host.To(bob).Any(x => x.Text == t7) && a.Host.To(ann).Any(x => x.Kind == "echo" && x.Text == t7), 20, a, b),
            "and goes when the hub is back within the minute");
        c.Check(w, a.World.Queued == 0, "nothing is left waiting");
        c.Check(w, await Until(() => a.Host.To(ann).Last(x => x.Kind == "friend") is { Online: true, Name: "Bob@bravo" }, 10, a, b),
            "after the reconnection the watches go again and the friend shows online");

        ((HubLink)a.Link).Stop();
        ((HubLink)b.Link).Stop();
        await Task.Delay(300);
        hub.Kill();

        // ---- what was logged ---------------------------------------------------------------------
        const string l = "hub/logging";
        List<string> ours;
        lock (log)
            ours = [.. log];
        ours.AddRange(a.Logs);
        ours.AddRange(b.Logs);
        var keys = new[] { keyA, keyB, keyC };
        c.Check(l, ours.Count > 0 && !ours.Any(x => keys.Any(k => x.Contains(k, StringComparison.Ordinal))),
            $"no line the mod's code logged holds a key ({ours.Count} lines)");
        c.Check(l, !ours.Any(x => bodies.Any(bd => x.Contains(bd, StringComparison.Ordinal))), "no line the mod's code logged holds a message's text");
        List<string> hubOut;
        lock (hub.Output)
            hubOut = [.. hub.Output];
        c.Check(l, !hubOut.Any(x => keys.Any(k => x.Contains(k, StringComparison.Ordinal)) || bodies.Any(bd => x.Contains(bd, StringComparison.Ordinal))),
            $"the hub's own log holds no key and no message text either ({hubOut.Count} lines)");
        if (Environment.GetEnvironmentVariable("DERETH_HARNESS_LOG") is { Length: > 0 } dump)
            File.WriteAllLines(dump, ours.Append("---- the hub ----").Concat(hubOut));
    }
}
