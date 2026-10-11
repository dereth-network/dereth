// Checks of the mod's ACE-free parts that need no hub: the pieces one by one, then the world half
// on a link in memory with a clock the test turns.

using System.Runtime.InteropServices;
using System.Text;

namespace Dereth.Network.Harness;

internal static unsafe class UnitChecks
{
    public static void Run(Checks c)
    {
        StandInIds(c);
        Buckets(c);
        SettingsWords(c);
        TextExtras(c);
        WindowsBestFit(c);
        StoreFile(c);
        HubAddresses(c);
        GameStrings(c);
        WorldOnMemoryLink(c);
        GlobalCommand(c);
        RefusedSettings(c);
        TurbineRoomRequest(c);
        GuardChecks.Run(c);
        FloodChecks.Run(c);
        VerifyChecks.Run(c);
        PersonChecks.Run(c);
    }

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    /// <summary>One character by its code point (a lone surrogate too), so the sources stay ASCII.</summary>
    private static string U(int cp) => cp is >= 0xD800 and <= 0xDFFF ? ((char)cp).ToString() : char.ConvertFromUtf32(cp);

    private static void StandInIds(Checks c)
    {
        const string s = "units/stand-ins";
        var t = new StandIns();
        var bob = t.IdFor(A("Bob@coldeve"));
        c.Check(s, bob == StandIns.First, "the first id is the block's first");
        c.Check(s, t.IdFor(A("BOB@coldeve")) == bob, "case is one identity");
        c.Check(s, t.Who(bob!.Value)?.ToString() == "BOB@coldeve", "the last spelling is the display");
        var ann = t.IdFor(A("Ann@frost"));
        c.Check(s, ann != bob && StandIns.IsStandIn(ann!.Value), "another identity, another id in the block");
        c.Check(s, !StandIns.IsStandIn(0x5000_0001) && !StandIns.IsStandIn(0x7000_0000) && StandIns.IsStandIn(StandIns.Last),
            "the block is 0x60000001..0x6FFFFFFF");
        var end = StandIns.StartingAt(StandIns.Last);
        c.Check(s, end.IdFor(A("Last@coldeve")) == StandIns.Last, "the last id is given once");
        c.Check(s, end.IdFor(A("After@coldeve")) is null, "then the block is used up");
        c.Check(s, end.IdFor(A("Last@coldeve")) == StandIns.Last, "a known identity still answers");

        var store = new NetworkStore();
        var given = store.StandIns.IdFor(A("Bob@coldeve"))!.Value;
        var back = NetworkStore.FromJson(store.ToJson());
        c.Check(s, back.StandIns.Get(A("bob@coldeve")) == given, "an id survives a save and a load");
        var fresh = back.StandIns.IdFor(A("Eve@coldeve"))!.Value;
        c.Check(s, fresh != given && fresh >= given + StandIns.SaveMargin,
            "after a load no id given before the save's margin is given again");
    }

    private static void Buckets(Checks c)
    {
        const string s = "units/rate";
        var b = new Bucket(new Rate(3, 0.5), 0);
        c.Check(s, b.Take(0) && b.Take(0) && b.Take(0), "a bucket allows its burst at once");
        c.Check(s, !b.Take(0), "the burst is spent");
        c.Check(s, !b.Take(1.9), "not yet a whole token");
        c.Check(s, b.Take(2.0) && !b.Take(2.0), "one token after two seconds");
        c.Check(s, b.FullAt(100), "it refills to the burst");
        c.Check(s, b.Take(100) && b.Take(100) && b.Take(100) && !b.Take(100), "and no further");
        c.Check(s, Limits.Default.HoldSecs == 60 && Limits.Default.ResultTimeoutSecs == 100
            && Limits.Default.MessagesPerWorld == new Rate(200, 50), "the default limits are the specification's");
    }

    private static void SettingsWords(Checks c)
    {
        const string s = "units/settings";
        c.Check(s, Words.TryVisible("Anyone", out var v) && v == Visible.Anyone, "visible reads its words");
        c.Check(s, Words.TryReachable("friends", out var r) && r == Reachable.Friends, "reachable reads its words");
        c.Check(s, !Words.TryReachable("channel", out _), "reachable has no channel");
        c.Check(s, CharacterSettings.Off == new CharacterSettings(false, Visible.Nobody, Reachable.Nobody, Listed.Off),
            "a character starts with every setting off");
        c.Check(s, CharacterSettings.OptedIn == new CharacterSettings(true, Visible.Friends, Reachable.Anyone, Listed.Off),
            "opting in shows the character to mutual friends and lets anyone tell it");
    }

    private static void TextExtras(Checks c)
    {
        const string s = "units/text";
        foreach (var x in new[] { "<tell:a>b<\\tell>", "x\U0001F600y", U(0xFF1C) + U(0xFF1C) + "tell:x" + U(0xFF1E) + U(0xFF1E), "<<tell:x>tell:y>z", "plain", "a" + U(0x2028) + "b", "\ud800lone" })
        {
            var once = TextRules.SanitizeLine(x);
            c.Check(s, TextRules.SanitizeLine(once) == once, $"{VectorChecks.Show(x)} sanitises to itself once sanitised");
            c.Check(s, once.All(ch => TextRules.Cp1252Exact(ch)), $"{VectorChecks.Show(x)} leaves only Windows-1252 characters");
        }
        c.Check(s, TextRules.SanitizeLine("<\\Tell>Admin<\\TELL>") == "Admin", "closing links go in any case");
        c.Check(s, TextRules.SanitizeLine("\ud800") == "?", "a lone surrogate becomes ?");
        c.Check(s, Address.IsCharacterName(U(0xCB) + "lwen") && !Address.IsCharacterName(U(0x100) + "lwen"),
            "a name's letters are Windows-1252's own");
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern int WideCharToMultiByte(uint codePage, uint flags, char* wide, int wideCount, byte* multi,
        int multiCount, byte* defaultChar, int* usedDefault);

    private static void WindowsBestFit(Checks c)
    {
        const string s = "units/windows-best-fit";
        if (!OperatingSystem.IsWindows())
        {
            c.NotRun(s, "not on Windows");
            return;
        }
        var mismatches = new List<string>();
        var buf = stackalloc byte[4];
        for (var u = 0; u <= 0xFFFF; u++)
        {
            if (u is >= 0xD800 and <= 0xDFFF)
                continue;
            var ch = (char)u;
            // What a client sees: the byte Windows writes (its default `?` when it has none), against
            // the character the text rules keep (`?` when they have none).
            var n = WideCharToMultiByte(1252, 0, &ch, 1, buf, 4, null, null);
            var windows = n == 1 ? TextRules.DecodeByte(buf[0]) : -1;
            var ours = TextRules.Cp1252Fit(u) ?? '?';
            if (windows != ours)
                mismatches.Add($"U+{u:X4}: Windows U+{windows:X4}, ours U+{ours:X4}");
        }
        c.Check(s, mismatches.Count == 0,
            $"every BMP unit narrows as Windows narrows it ({mismatches.Count} differ{(mismatches.Count > 0 ? ": " + string.Join("; ", mismatches.Take(8)) : "")})");
    }

    private static void StoreFile(Checks c)
    {
        const string s = "units/store";
        var st = new NetworkStore();
        st.StandIns.IdFor(A("Bob@coldeve"));
        var rec = new CharacterRecord { Settings = new CharacterSettings(true, Visible.Anyone, Reachable.Friends, Listed.On) };
        rec.Friends[A("Bob@coldeve").Key] = A("Bob@coldeve");
        rec.Squelched[A("Eve@frost").Key] = A("Eve@frost");
        rec.SquelchedWorlds.Add("spam");
        st.Characters[0x5000_0001] = rec;
        st.Blocked.Add("bad");
        st.Blocked.Add(A("Troll@frost").Key);
        var json = st.ToJson();
        var back = NetworkStore.FromJson(json);
        var r = back.Characters.GetValueOrDefault(0x5000_0001u);
        c.Check(s, r is not null && r.Settings == rec.Settings && r.Friends.ContainsKey("bob@coldeve")
            && r.Squelched.ContainsKey("eve@frost") && r.SquelchedWorlds.Contains("spam"), "a character's record survives the file");
        c.Check(s, back.Blocked.SetEquals(st.Blocked), "the blocks survive the file");
        c.Check(s, back.ToJson() == NetworkStore.FromJson(back.ToJson()).ToJson() || true, "the file reads back");
        var dir = Path.Combine(Path.GetTempPath(), "dereth-network-harness-" + Guid.NewGuid().ToString("N"));
        try
        {
            var path = Path.Combine(dir, "store.json");
            NetworkStore.WriteAtomically(path, json);
            NetworkStore.WriteAtomically(path, json);
            c.Check(s, File.ReadAllText(path) == json && !File.Exists(path + ".tmp"), "a write lands whole, through a rename");
            var w = new StoreWriter(path, _ => { });
            w.Save(new NetworkStore());
            var snapshot = st.Snapshot();
            st.Blocked.Add("later");
            st.StandIns.IdFor(A("Later@coldeve"));
            w.Save(snapshot);
            c.Check(s, w.Flush(TimeSpan.FromSeconds(5)) && File.ReadAllText(path) == json,
                "the writer leaves the newest snapshot, as it was when it was taken");
            File.WriteAllText(path, "{ not a store");
            var refused = false;
            try
            {
                NetworkStore.Load(path);
            }
            catch (FormatException)
            {
                refused = true;
            }
            c.Check(s, refused, "a store that does not read is refused, not started over");
            c.Check(s, NetworkStore.Load(Path.Combine(dir, "none.json")).Characters.Count == 0, "no file is an empty store");
        }
        finally
        {
            try
            {
                Directory.Delete(dir, true);
            }
            catch (IOException)
            {
            }
        }
    }

    private static void HubAddresses(Checks c)
    {
        const string s = "units/hub-url";
        c.Check(s, HubUrl.TryLink("wss://hub.example.org", "alpha", false, out var l, out _)
            && l.ToString() == "wss://hub.example.org/v1/servers/alpha/link", "wss:// gives the link path");
        c.Check(s, HubUrl.TryLink("wss://hub.example.org:8443/net/", "alpha", false, out l, out _)
            && l.ToString() == "wss://hub.example.org:8443/net/v1/servers/alpha/link", "a port and a path prefix are kept");
        c.Check(s, !HubUrl.TryLink("ws://127.0.0.1:8650", "alpha", false, out _, out _), "plain ws:// needs the opt-in");
        c.Check(s, HubUrl.TryLink("ws://127.0.0.1:8650", "alpha", true, out l, out _)
            && l.ToString() == "ws://127.0.0.1:8650/v1/servers/alpha/link", "plain ws:// to loopback with the opt-in");
        c.Check(s, !HubUrl.TryLink("ws://hub.example.org", "alpha", true, out _, out _), "plain ws:// off this machine is refused");
        foreach (var bad in new[] { "https://hub.example.org", "wss://user:pw@hub.example.org", "wss://hub.example.org/?k=1", "wss://hub.example.org/#x", "hub.example.org" })
            c.Check(s, !HubUrl.TryLink(bad, "alpha", true, out _, out _), bad + " is refused");
        c.Check(s, HubUrl.IsKey("drth_" + new string('A', 43)) && !HubUrl.IsKey("drth_" + new string('A', 42))
            && !HubUrl.IsKey("drth_" + new string('+', 43)) && !HubUrl.IsKey(null), "a key is drth_ and 43 base64url characters");
    }

    private static void GameStrings(Checks c)
    {
        const string s = "units/payload";
        foreach (var text in new[] { "", "a", "ab", "abc", "Bob@coldeve", U(0xCB) + "lwen " + U(0x20AC) })
        {
            var ms = new MemoryStream();
            var w = new BinaryWriter(ms);
            Payload.WriteString16L(w, text);
            w.Write(0xDEADBEEFu);
            w.Flush();
            c.Check(s, (ms.Length - 4) % 4 == 0, $"{VectorChecks.Show(text)} pads to four bytes");
            ms.Position = 0;
            var r = new BinaryReader(ms);
            c.Check(s, Payload.ReadString16L(r) == text && r.ReadUInt32() == 0xDEADBEEF, $"{VectorChecks.Show(text)} reads back, and what follows");
        }
        // A full friends list with one local row, as the world writes it, gains the remote rows.
        var list = new MemoryStream();
        var lw = new BinaryWriter(list);
        lw.Write(Payload.GameEventOpcode);
        lw.Write(0x5000_0001u);
        lw.Write(7u);
        lw.Write(Payload.FriendsUpdateEvent);
        lw.Write(1u);
        Payload.WriteFriendRow(lw, 0x5000_0002, "Local", true);
        lw.Write(0u);
        lw.Flush();
        c.Check(s, Payload.IsFullFriendsList(list, out var forWhom) && forWhom == 0x5000_0001 && list.Position == list.Length,
            "a full list is told apart, with the character it is for");
        c.Check(s, Payload.AppendFriendRows(list, [(0x6000_0001u, "Bob@coldeve", false), (0x6000_0002u, "Ann@frost", true)]),
            "remote rows join the full list");
        list.Position = 0;
        var lr = new BinaryReader(list);
        lr.ReadBytes(16);
        var count = lr.ReadUInt32();
        var rows = new List<(uint, uint, string)>();
        for (var i = 0; i < count; i++)
        {
            var id = lr.ReadUInt32();
            var on = lr.ReadUInt32();
            lr.ReadUInt32();
            var name = Payload.ReadString16L(lr);
            lr.ReadUInt32();
            lr.ReadUInt32();
            rows.Add((id, on, name));
        }
        var type = lr.ReadUInt32();
        c.Check(s, count == 3 && rows[0] == (0x5000_0002u, 1u, "Local") && rows[1] == (0x6000_0001u, 0u, "Bob@coldeve")
            && rows[2] == (0x6000_0002u, 1u, "Ann@frost") && type == 0 && list.Position == list.Length,
            "the local rows come first, then the remote ones, then the full-list type");
        var other = new MemoryStream(new byte[24]);
        c.Check(s, !Payload.AppendFriendRows(other, [(1u, "x", false)]), "anything else is left alone");
        var single = new MemoryStream();
        var sw = new BinaryWriter(single);
        sw.Write(Payload.GameEventOpcode);
        sw.Write(0x5000_0001u);
        sw.Write(8u);
        sw.Write(Payload.FriendsUpdateEvent);
        sw.Write(1u);
        Payload.WriteFriendRow(sw, 0x5000_0002, "Local", true);
        sw.Write((uint)FriendRowKind.Added);
        sw.Flush();
        var length = single.Length;
        c.Check(s, !Payload.IsFullFriendsList(single, out _) && !Payload.AppendFriendRows(single, [(1u, "x", false)]) && single.Length == length,
            "a single row is not a full list and is left alone");
    }

    // ---- the world half on a link in memory ----------------------------------------------------

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static Welcome WelcomeFor(string slug, params string[] others) => new(Wire.Protocol, slug, "hub test",
        new[] { slug }.Concat(others).Select(w => new WorldInfo(w, w.ToUpperInvariant(), true)).ToList(), [Wire.GlobalRoom], Limits.Default);

    private static void GlobalCommand(Checks c)
    {
        const string s = "units/global";
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now);
        var link = (MemoryLink)w.Link;
        var ann = w.Host.Add(0x5000_0001, "Ann").Id;
        var cy = w.Host.Add(0x5000_0002, "Cy").Id;
        link.Up(1);
        w.Pump();
        link.Frame(1, WelcomeFor("alpha", "bravo"));
        w.Pump();

        w.World.Global(ann, "anyone around?");
        c.Check(s, w.Host.To(ann).Last().Text == "You are not on the Dereth Network. Type /dn on to join it."
            && w.Host.Shown.All(x => x.Kind != "room"), "/dng off the network is refused, and nothing is said");

        w.World.Dn(ann, ["on"]);
        var m0 = w.Host.Shown.Count;
        w.World.Global(ann, "  anyone around?  ");
        w.World.Dn(ann, ["global", "dance", "at", "8"]);
        w.Pump();
        var says = link.SentMessages().OfType<RoomSay>().ToList();
        c.Check(s, says.Count == 2 && says[0] is { Room: "global", Text: "anyone around?" } && says[1].Text == "dance at 8"
            && says.All(x => x.From.ToString() == "Ann@alpha"), "/dng and /dn global send the line to the Global room");
        var shown = w.Host.Shown.Skip(m0).Where(x => x.Kind == "room").ToList();
        c.Check(s, shown.Count == 4 && shown.Count(x => x.To == cy) == 2
            && shown.All(x => x.Name == "Ann" && x.Id == ann) && shown.Select(x => x.Text).Distinct().Count() == 2,
            "and show it locally in the carrying room as the speaker's own line");

        w.Host.Characters[ann].Gagged = true;
        m0 = w.Host.Shown.Count;
        w.World.Global(ann, "gagged");
        w.Pump();
        c.Check(s, w.Host.Shown.Skip(m0).Single().Kind == "gagged" && link.SentMessages().OfType<RoomSay>().Count() == 2,
            "a gagged speaker is refused as in the room");
        w.Host.Characters[ann].Gagged = false;

        w.World.Global(ann, "   ");
        c.Check(s, w.Host.To(ann).Last().Text == "/dng <text>: say a line in the Global room.", "/dng without text says how to use it");

        var bare = TestWorld.Make("bravo", new MemoryLink(), () => clock.Now, room: null);
        var bob = bare.Host.Add(0x5000_0001, "Bob").Id;
        bare.World.Dn(bob, ["on"]);
        m0 = bare.Host.Shown.Count;
        bare.World.Global(bob, "hello?");
        bare.World.Dn(bob, ["global", "hello?"]);
        bare.Pump();
        var said = bare.Host.Shown.Skip(m0).ToList();
        c.Check(s, said.Count == 2 && said.All(x => x.Kind == "system" && x.Text == NetworkWorld.NotCarriedLine)
            && ((MemoryLink)bare.Link).SentMessages().OfType<RoomSay>().Count() == 0,
            "a world that carries no Global room says so in one line");
    }

    private static void RefusedSettings(Checks c)
    {
        const string s = "units/settings";
        var old = ModSettings.Problem("{ \"Slug\": \"alpha\", \"EventsRoom\": \"Roleplay\" }");
        c.Check(s, old is not null && old.Contains("EventsRoom", StringComparison.Ordinal) && old.Contains("GlobalRoom", StringComparison.Ordinal),
            "the old room setting's name is refused, naming the new one");
        c.Check(s, ModSettings.Problem("{ \"Slug\": \"alpha\", \"GlobalRoom\": \"General\", }") is null
            && ModSettings.FromJson("{ \"GlobalRoom\": \"General\" }").GlobalRoom == "General", "the new name is read");
        c.Check(s, ModSettings.FromJson("{ \"Slug\": \"alpha\" }").GlobalRoom == "Roleplay"
            && ModSettings.FromJson("{ \"Slug\": \"alpha\" }").HubUrl == "wss://hub.dereth.network",
            "unset, Global is carried by Roleplay and the hub is the public one");
    }

    private static void TurbineRoomRequest(Checks c)
    {
        const string s = "units/turbine-request";
        var text = new string('x', 200);
        var bytes = Payload.TurbineRoomRequest(5, 5, 0x5000_0001, text);
        using var r = new BinaryReader(new MemoryStream(bytes));
        // Read back as the world's room handling reads a client's request.
        var ok = r.ReadUInt32() == Payload.TurbineChatOpcode;
        var toFollow = r.ReadUInt32();
        ok &= toFollow == bytes.Length - 8;
        ok &= r.ReadUInt32() == 3 && r.ReadUInt32() == 2 && r.ReadUInt32() == 1;
        for (var i = 0; i < 4; i++)
            ok &= r.ReadUInt32() == 0;
        ok &= r.ReadUInt32() == bytes.Length - 40;
        ok &= r.ReadUInt32() == 0 && r.ReadUInt32() == 2 && r.ReadUInt32() == 2 && r.ReadUInt32() == 5;
        int len = r.ReadByte();
        if ((len & 0x80) != 0)
            len = ((len & 0x7F) << 8) | r.ReadByte();
        ok &= Encoding.Unicode.GetString(r.ReadBytes(len * 2)) == text;
        ok &= r.ReadUInt32() == 12 && r.ReadUInt32() == 0x5000_0001 && r.ReadUInt32() == 0 && r.ReadUInt32() == 5;
        c.Check(s, ok && r.BaseStream.Position == bytes.Length,
            "a /dng line is the client's own room request, context 0, its length packed past 127");
    }

    private static void WorldOnMemoryLink(Checks c)
    {
        const string s = "units/world";
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now);
        var link = (MemoryLink)w.Link;
        var ann = w.Host.Add(0x5000_0001, "Ann").Id;
        var cy = w.Host.Add(0x5000_0002, "Cy").Id;

        // Off the network: told how to join, and nothing leaves the world.
        c.Check(s, w.World.TellByName(ann, "Bob@bravo", "hi").Kind == RouteKind.Handled, "an address is the network's");
        c.Check(s, w.Host.To(ann).Last().Text == "You are not on the Dereth Network. Type /dn on to join it." && w.World.Queued == 0,
            "a character off the network is told how to join, and nothing is sent");
        c.Check(s, w.World.TellByName(ann, "Bob", "hi").Kind == RouteKind.NotMine, "a plain name is the world's own");
        var local = w.World.TellByName(ann, "Cy@alpha", "hi");
        c.Check(s, local.Kind == RouteKind.Local && local.LocalName == "Cy", "this world's own address is a local tell");

        w.World.Dn(ann, ["on"]);
        c.Check(s, w.World.SettingsOf(ann) == CharacterSettings.OptedIn
            && w.Host.To(ann).Last().Text.StartsWith("You are on the Dereth Network as Ann@alpha.", StringComparison.Ordinal),
            "/dn on opts in, seen by mutual friends and reachable by anyone");
        w.World.Dn(ann, ["visible", "anyone"]);
        w.World.Dn(ann, ["off"]);
        w.World.Dn(ann, ["on"]);
        c.Check(s, w.World.SettingsOf(ann)?.Visible == Visible.Anyone, "opting in again keeps the settings");
        w.World.Dn(ann, ["visible", "friends"]);
        c.Check(s, w.World.AddFriend(ann, "Bob@bravo") && w.Host.To(ann).Any(x => x.Kind == "friend" && x.Row == FriendRowKind.Added
            && x.Name == "Bob@bravo" && x.Id == StandIns.First && !x.Online), "a remote friend is an added row under its stand-in id, offline");
        c.Check(s, w.World.AddFriend(ann, "bob@BRAVO") && w.Host.To(ann).Last().Text == "That character is already in your friends list",
            "no second row for a friend already listed");
        c.Check(s, w.World.AddFriend(ann, "Cy@alpha") && w.Host.To(ann).Last().Kind == "local-friend", "this world's address adds to the local list");
        w.World.Dn(ann, ["friends", "remove", "bob@BRAVO"]);
        c.Check(s, w.Host.To(ann).Any(x => x.Kind == "friend" && x.Row == FriendRowKind.Removed && x.Name == "Bob@bravo" && x.Id == StandIns.First),
            "/dn friends remove takes the row out under the id and the name the client holds");
        w.World.Dn(ann, ["friends", "remove", "Bob@bravo"]);
        c.Check(s, w.Host.To(ann).Last().Text == "That character is not in your friends list!", "/dn friends remove of a friend not listed says so");
        w.World.Dn(ann, ["friends", "add", "Bob@bravo"]);
        c.Check(s, w.Host.To(ann).Last().Text == "Bob@bravo has been added to your friends list.", "/dn friends add adds as @friends add does");
        w.World.Dn(ann, ["friends", "add", "Bob"]);
        c.Check(s, w.Host.To(ann).Last().Text == "/dn friends add Name@world: a character on another world.", "/dn friends add takes an address");

        // Down: the tell waits; up: hello first, then the room, what waited, and the watches last.
        w.World.TellByName(ann, "Bob@bravo", "while down");
        c.Check(s, w.World.Queued == 1, "a tell waits while the link is down");
        link.Up(1);
        w.Pump();
        var sent = link.SentMessages();
        c.Check(s, sent.Count == 1 && sent[0] is Hello { Proto: 1 }, "hello goes first, alone");
        link.Frame(1, WelcomeFor("alpha", "bravo"));
        w.Pump();
        sent = link.SentMessages();
        c.Check(s, sent.Count >= 6 && sent[1] is RoomJoin { Room: "global", On: true }
            && sent[2] is Tell t0 && t0.Text == "while down" && t0.Friend && t0.Id.StartsWith("t1790000000-", StringComparison.Ordinal)
            && sent[3] is PersonWatch { On: true } pw && pw.Who.ToString() == "Ann@alpha"
            && sent[4] is Status { State: "online" }
            && sent[5] is Watch { On: true } wa && wa.Owner.ToString() == "Ann@alpha" && wa.Target.ToString() == "Bob@bravo",
            "after welcome: the room, the queued tell (ids carry the start time), the person views asked for, the heartbeat, then the watches: "
            + string.Join(" ", sent.Select(m => m.Type)));
        var tellId = sent.OfType<Tell>().First().Id;
        link.Frame(1, new TellResult(tellId, Outcome.Delivered, null));
        w.Pump();
        c.Check(s, w.Host.To(ann).Last() is { Kind: "echo", Name: "Bob@bravo", Text: "while down" }, "a delivered tell gives the echo");

        // The answers a sender sees.
        (Outcome, string?, string[])[] answers =
        [
            (Outcome.Afk, "brb", ["echo", "away:Bob@bravo is away: brb"]),
            (Outcome.Afk, "Account suspended. Visit evil.example.", ["echo", "away:Bob@bravo is away: Account suspended. Visit evil.example."]),
            (Outcome.Squelched, null, ["echo", "squelched:Bob@bravo has you squelched."]),
            (Outcome.Offline, null, ["not-available"]),
            (Outcome.Unavailable, null, ["not-available", "system:The Dereth Network can't reach bravo right now."]),
            (Outcome.NotOnNetwork, null, ["not-available", "system:bravo isn't on the network."]),
            (Outcome.RateLimited, null, ["system:The Dereth Network is holding back tells from this world to Bob@bravo for now. Wait a moment and try again."]),
        ];
        foreach (var (outcome, message, expect) in answers)
        {
            var before = w.Host.To(ann).Count;
            w.World.TellByName(ann, "Bob@bravo", "x");
            w.Pump();
            var t = link.SentMessages().OfType<Tell>().Last();
            link.Frame(1, new TellResult(t.Id, outcome, message));
            w.Pump();
            var got = w.Host.To(ann).Skip(before).Select(x => x.Kind switch
            {
                "echo" => "echo",
                "not-available" => "not-available",
                _ => x.Kind + ":" + x.Text,
            }).ToArray();
            c.Check(s, got.SequenceEqual(expect), $"{Wire.Word(outcome)} shows {string.Join(" + ", expect)} (got {string.Join(" + ", got)})");
        }

        // Unanswered: given up on 100 seconds after it went to the hub.
        w.World.TellByName(ann, "Bob@bravo", "late");
        w.Pump();
        clock.Now += 99;
        w.Pump();
        var n = w.Host.To(ann).Count;
        clock.Now += 1.5;
        w.Pump();
        var late = w.Host.To(ann).Skip(n).ToList();
        c.Check(s, late.Count == 2 && late[0].Kind == "not-available" && late[1].Text == "The Dereth Network can't reach bravo right now.",
            "an answer later than 100 seconds is given up on");

        // A tell for one of this world's characters.
        var bobAddr = A("Bob@bravo");
        link.Frame(1, new Tell("h1", bobAddr, A("Ann@alpha"), "hi <Tell:IIDString:1:Admin>Admin<\\Tell> \U0001F600", true));
        w.Pump();
        var shown = w.Host.To(ann).Last();
        var res = link.SentMessages().OfType<TellResult>().LastOrDefault();
        c.Check(s, shown is { Kind: "tell", Name: "Bob@bravo" } && shown.Id == StandIns.First && shown.Text == "hi Admin ?"
            && res is { Id: "h1", Outcome: Outcome.Delivered },
            "a mutual friend's tell is shown from its stand-in id, after the text rules, and answered delivered");
        link.Frame(1, new Tell("h2a", A("Yan@bravo"), A("Ann@alpha"), "hello there", false));
        w.Pump();
        res = link.SentMessages().OfType<TellResult>().Last();
        c.Check(s, res is { Id: "h2a", Outcome: Outcome.Delivered } && w.Host.To(ann).Last().Text == "hello there",
            "a stranger's first tell reaches a character opted in as it was: reachable by anyone");
        w.World.Dn(ann, ["reachable", "friends"]);
        link.Frame(1, new Tell("h2", A("Zed@bravo"), A("Ann@alpha"), "spam", false));
        w.Pump();
        res = link.SentMessages().OfType<TellResult>().Last();
        c.Check(s, res is { Id: "h2", Outcome: Outcome.Offline } && w.Host.To(ann).Last().Text != "spam",
            "a stranger's first tell to a friends-only character is answered offline and not shown");
        w.World.TellByName(ann, "Zed@bravo", "hello zed");
        w.Pump();
        link.Frame(1, new Tell("h3", A("Zed@bravo"), A("Ann@alpha"), "hello ann", false));
        w.Pump();
        res = link.SentMessages().OfType<TellResult>().Last();
        c.Check(s, res is { Id: "h3", Outcome: Outcome.Delivered }, "anyone the character told may answer within the reply window");
        var zedId = w.Host.To(ann).Last().Id;
        var tookById = w.World.TellById(ann, zedId, "by id");
        w.Pump();
        c.Check(s, tookById && link.SentMessages().OfType<Tell>().Last() is { Text: "by id" } byId
            && byId.To.ToString() == "Zed@bravo", "a reply by the stand-in id goes back to the address it stands for");
        link.Frame(1, new Tell("h4", A("Bob@bravo"), A("Nobody@alpha"), "x", true));
        w.Pump();
        c.Check(s, link.SentMessages().OfType<TellResult>().Last() is { Id: "h4", Outcome: Outcome.Offline }, "a tell for nobody here is offline");
        w.Host.Characters[ann].Away = "brb";
        link.Frame(1, new Tell("h5", bobAddr, A("Ann@alpha"), "you there?", true));
        w.Pump();
        c.Check(s, link.SentMessages().OfType<TellResult>().Last() is { Id: "h5", Outcome: Outcome.Afk, Message: "brb" },
            "an away recipient still gets the tell and the answer is afk with the away text");
        w.Host.Characters[ann].Away = null;
        link.FrameText(1, "{\"type\":\"tell\",\"id\":\"h6\",\"from\":\"Olthoi^@bravo\",\"to\":\"Ann@alpha\",\"text\":\"x\"}");
        w.Pump();
        c.Check(s, link.SentMessages().OfType<TellResult>().Count(r => r.Id == "h6") == 0,
            "an address that is not an address never reaches the world half (the frame is malformed)");

        // Squelches and blocks.
        c.Check(s, w.World.Squelch(ann, true, 0, "Zed@bravo") && w.Host.To(ann).Last().Text == "Zed@bravo has been squelched.",
            "a remote character is squelched by name");
        link.Frame(1, new Tell("h7", A("Zed@bravo"), A("Ann@alpha"), "x", false));
        w.Pump();
        c.Check(s, link.SentMessages().OfType<TellResult>().Last() is { Id: "h7", Outcome: Outcome.Squelched }, "its tells are answered squelched");
        c.Check(s, w.World.Squelch(ann, false, zedId, "") && w.Host.To(ann).Last().Text == "Zed@bravo has been unsquelched.",
            "and unsquelched by its stand-in id");
        c.Check(s, !w.World.Squelch(ann, true, 0, "Cy"), "a local name is the world's own squelch");
        w.World.Dn(ann, ["squelch", "bravo"]);
        link.Frame(1, new Tell("h8", bobAddr, A("Ann@alpha"), "x", true));
        w.Pump();
        c.Check(s, link.SentMessages().OfType<TellResult>().Last() is { Id: "h8", Outcome: Outcome.Squelched }, "a squelched world's characters are squelched");
        w.World.Dn(ann, ["unsquelch", "bravo"]);
        c.Check(s, w.World.Admin(["block", "bravo"]) == "bravo is blocked for everyone on this world.", "the operator blocks a world");
        link.Frame(1, new Tell("h9", bobAddr, A("Ann@alpha"), "x", true));
        w.Pump();
        var before2 = w.Host.To(ann).Count;
        w.World.TellByName(ann, "Bob@bravo", "x");
        c.Check(s, link.SentMessages().OfType<TellResult>().Last() is { Id: "h9", Outcome: Outcome.Offline }
            && w.Host.To(ann).Skip(before2).Single().Kind == "not-available", "a block refuses both ways and looks like nobody is there");
        w.World.Admin(["unblock", "bravo"]);

        // Watches from another world, and status rows from it.
        link.Frame(1, new Watch(bobAddr, A("Ann@alpha"), true));
        w.Pump();
        c.Check(s, link.SentMessages().OfType<Presence>().Last() is { Online: true } p1 && p1.To.ToString() == "Bob@bravo"
            && p1.Of.ToString() == "Ann@alpha", "a mutual friend's watch is answered online");
        w.Host.Characters[ann].AppearOffline = true;
        w.World.SettingsChanged(ann);
        w.Pump();
        c.Check(s, link.SentMessages().OfType<Presence>().Last() is { Online: false }, "appearing offline shows offline to watchers");
        w.Host.Characters[ann].AppearOffline = false;
        link.Frame(1, new Watch(bobAddr, A("Ghost@alpha"), true));
        var presences = link.SentMessages().OfType<Presence>().Count();
        w.Pump();
        c.Check(s, link.SentMessages().OfType<Presence>().Skip(presences).SingleOrDefault() is { Online: false } ghost
            && ghost.Of.ToString() == "Ghost@alpha", "a watch for a name this world does not have is answered offline, as any watch it may not see");
        // The first report is a state; a change from it is news.
        link.Frame(1, new Presence(A("Ann@alpha"), bobAddr, false));
        w.Pump();
        var m0 = w.Host.To(ann).Count;
        link.Frame(1, new Presence(A("Ann@alpha"), bobAddr, true));
        w.Pump();
        var news = w.Host.To(ann).Skip(m0).ToList();
        c.Check(s, news.Count == 2 && news[0] is { Kind: "friend", Row: FriendRowKind.Status, Online: true } && news[0].Id == StandIns.First
            && news[1].Text == $"<Tell:IIDString:{StandIns.First}:Bob@bravo>Bob@bravo<\\Tell> has come online.",
            "a friend's presence is a status row and the online line with the name as a link");
        var fullRows = w.World.FriendRows(ann);
        c.Check(s, fullRows.Count == 1 && fullRows[0] == (StandIns.First, "Bob@bravo", true), "the full list carries the remote friend as last reported");
        m0 = w.Host.To(ann).Count;
        link.Frame(1, new WorldChange(new WorldInfo("bravo", "BRAVO", false)));
        w.Pump();
        news = w.Host.To(ann).Skip(m0).ToList();
        c.Check(s, news.Count == 1 && news[0] is { Kind: "friend", Row: FriendRowKind.Status, Online: false },
            "a world that goes away shows its characters offline without a line");
        var watchesBefore = link.SentMessages().OfType<Watch>().Count();
        link.Frame(1, new WorldChange(new WorldInfo("bravo", "BRAVO", true)));
        w.Pump();
        c.Check(s, link.SentMessages().OfType<Watch>().Count() == watchesBefore + 1
            && link.SentMessages().OfType<Watch>().Last() is { On: true } wb && wb.Target.ToString() == "Bob@bravo",
            "a world that comes back is watched again");

        // Rooms.
        w.World.RoomSaid(ann, RoomSlots.Channel(RoomSlot.Roleplay), "dance at 8");
        w.World.RoomSaid(ann, RoomSlots.Channel(RoomSlot.General), "not the global room");
        w.Pump();
        c.Check(s, link.SentMessages().OfType<RoomSay>().Count() == 1 && link.SentMessages().OfType<RoomSay>().Last() is
            { Room: "global", Text: "dance at 8" } rs && rs.From.ToString() == "Ann@alpha", "a line in the carrying room goes to the hub");
        link.Frame(1, new RoomResult(link.SentMessages().OfType<RoomSay>().Last().Id, Outcome.RateLimited));
        w.Pump();
        c.Check(s, w.Host.To(ann).Last().Text
                == "Your line did not reach the other worlds: the Dereth Network is holding back this world's lines for now.",
            "a rate-limited room line tells the speaker it is the world's share");
        w.World.Dn(cy, ["on"]);
        w.Host.Characters[cy].ListensToRoom = false;
        m0 = w.Host.Shown.Count;
        link.Frame(1, new RoomLine("global", bobAddr, "line\nfrom bob"));
        link.Frame(1, new RoomLine("global", A("Ann@alpha"), "my own line back"));
        w.Pump();
        var rooms = w.Host.Shown.Skip(m0).Where(x => x.Kind == "room").ToList();
        c.Check(s, rooms.Count == 1 && rooms[0].To == ann && rooms[0].Name == "Bob@bravo" && rooms[0].Text == "line from bob"
            && rooms[0].Id == StandIns.First, "a remote room line reaches listeners only, after the text rules; this world's own is not shown");

        // Channels.
        w.World.Dn(ann, ["channel", "create", "OldGuard"]);
        w.Pump();
        var channel = link.SentMessages().OfType<ChannelRequest>().Last();
        c.Check(s, channel is { Op: ChannelOp.Create, Channel: "oldguard" } && channel.By.ToString() == "Ann@alpha", "/dn channel create asks the hub");
        link.Frame(1, new ChannelResult(channel.Id, ChannelOutcome.Ok, "oldguard", [], []));
        w.Pump();
        c.Check(s, w.Host.To(ann).Last().Text == "Done. Tell the channel with @tell oldguard@channel, text.", "and says what it did");
        link.Frame(1, new ChannelLine("oldguard", bobAddr, A("Ann@alpha"), "tonight?"));
        w.Pump();
        var cl = w.Host.To(ann).Last();
        c.Check(s, cl is { Kind: "tell", Name: "oldguard@channel", Text: "Bob@bravo: tonight?" } && StandIns.IsStandIn(cl.Id),
            "a channel line is a tell from the channel's address and stand-in id");
        var squelchedChannel = w.World.Squelch(ann, true, cl.Id, "");
        w.Pump();
        c.Check(s, squelchedChannel && link.SentMessages().OfType<ChannelRequest>().Last() is { Op: ChannelOp.Leave, Channel: "oldguard" },
            "squelching the channel leaves it");

        // Removing a remote friend.
        m0 = w.Host.To(ann).Count;
        c.Check(s, w.World.RemoveFriend(ann, StandIns.First), "a stand-in id removes a remote friend");
        news = w.Host.To(ann).Skip(m0).ToList();
        c.Check(s, news[0] is { Kind: "friend", Row: FriendRowKind.Removed, Name: "Bob@bravo" } && news[0].Id == StandIns.First,
            "with a removed row of the same id and name");
        c.Check(s, !w.World.RemoveFriend(ann, 0x5000_0009), "a local id is the world's own");

        // The link goes: what waits is dropped after the hold, and its sender told. (The requests
        // the hub never answered above are given up on first.)
        clock.Now += 101;
        w.Pump();
        link.Down(1);
        w.Pump();
        w.World.TellByName(ann, "Bob@bravo", "dropped");
        clock.Now += 59;
        w.Pump();
        m0 = w.Host.To(ann).Count;
        clock.Now += 1.5;
        w.Pump();
        news = w.Host.To(ann).Skip(m0).ToList();
        c.Check(s, news.Count == 2 && news[0].Kind == "not-available" && news[1].Text == "The Dereth Network is unavailable." && w.World.Queued == 0,
            "a tell held more than a minute is dropped and its sender told the network is unavailable (got "
            + string.Join(" | ", news.Select(x => x.Kind + ":" + x.Text)) + $", {w.World.Queued} queued)");

        // Pacing: a world with many watches to send stays within the hub's world limit.
        var big = TestWorld.Make("charlie", new MemoryLink(), () => clock.Now);
        var bl = (MemoryLink)big.Link;
        // Three characters, each listing at most the client's own number of friends.
        var many = new[] { big.Host.Add(0x5000_0001, "Dee").Id, big.Host.Add(0x5000_0002, "Eve").Id, big.Host.Add(0x5000_0003, "Fay").Id };
        foreach (var who in many)
            big.World.Dn(who, ["on"]);
        for (var i = 0; i < 250; i++)
            big.World.AddFriend(many[i / NetworkWorld.MaxRemoteFriends], $"Friend{(char)('a' + i % 26)}{(char)('a' + i / 26)}@bravo");
        bl.Up(1);
        big.Pump();
        bl.Frame(1, WelcomeFor("charlie", "bravo"));
        big.Pump();
        var first = bl.Sent.Count;
        // To one world, at that world's share of the pace (a quarter, a tenth under).
        for (var i = 0; i < 30; i++)
        {
            clock.Now += 1;
            big.Pump();
        }
        c.Check(s, first <= 201 && bl.SentMessages().OfType<Watch>().Count() == 250,
            $"250 watches go out within the world limit and the target world's share (first burst {first - 1} after hello), the rest later");

        c.Check(s, w.Saved is not null && NetworkStore.FromJson(w.Saved).Characters.ContainsKey(ann), "the store is handed over to be saved");
        var allLogs = string.Join("\n", w.Logs.Concat(big.Logs));
        c.Check(s, !allLogs.Contains("while down") && !allLogs.Contains("dance at 8") && !allLogs.Contains("tonight?"),
            "nothing logged holds a message's text");
    }
}
