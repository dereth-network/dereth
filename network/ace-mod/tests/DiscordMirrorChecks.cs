// A world's own rooms mirrored into Discord, on a link in memory, as a hub last said: the old
// setting that named them is no longer read, and says so; the rooms are joined at each welcome; a
// line said in one goes to the hub under the room's own name, and a line in a room the hub does not
// mirror goes nowhere; nothing comes back into them: a room line there is never shown, whoever it is
// from; a room a hub answers it does not mirror keeps its lines here for a minute, telling nobody;
// and the room's rules (on the network, age, pace, the world's share) hold, the speaker told of
// Discord.

namespace Dereth.Network.Harness;

internal static class DiscordMirrorChecks
{
    private const string S = "units/discord-mirrors";

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    private static readonly Welcome Welcomed = new(Wire.Protocol, "alpha", "hub test",
        [new WorldInfo("alpha", "ALPHA", true), new WorldInfo("bravo", "BRAVO", true)], [Wire.GlobalRoom], Limits.Default);

    /// <summary>
    /// A world `alpha` carrying the Global room in Roleplay, whose General room a hub last said it
    /// mirrors into Discord, linked and welcomed, with Ann (listening to General and Roleplay) and Cy
    /// (Roleplay alone) on the network.
    /// </summary>
    private static (TestWorld W, MemoryLink Link, uint Ann, uint Cy, Clock Clock) Ready(uint minAge = 0)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now, minAge: minAge, mirrored: [RoomSlot.General]);
        var link = (MemoryLink)w.Link;
        var ann = w.Host.Add(0x5000_0001, "Ann");
        ann.Rooms = [RoomSlot.General, RoomSlot.Roleplay];
        var cy = w.Host.Add(0x5000_0002, "Cy");
        cy.Rooms = [RoomSlot.Roleplay];
        link.Up(1);
        w.Pump();
        link.Frame(1, Welcomed);
        w.Pump();
        w.World.Dn(ann.Id, ["on"]);
        w.World.Dn(cy.Id, ["on"]);
        w.Pump();
        return (w, link, ann.Id, cy.Id, clock);
    }

    private static List<RoomSay> Said(MemoryLink link, int from) =>
        link.Sent.Skip(from).Select(s => Wire.Read(s.Frame).Message).OfType<RoomSay>().ToList();

    private static List<Shown> Rooms(TestWorld w, uint who, int from) =>
        w.Host.Shown.Skip(from).Where(x => x.To == who && x.Kind == "room").ToList();

    public static void Run(Checks c)
    {
        Settings(c);
        Joins(c);
        Out(c);
        NothingComesBack(c);
        NotCarried(c);
        Rules(c);
    }

    private static void Settings(Checks c)
    {
        const string old = "{ \"Slug\": \"alpha\", \"DiscordMirrors\": [\"General\", \"Society\"] }";
        var notes = ModSettings.Notes(old);
        c.Check(S, ModSettings.Problem(old) is null && notes.Count == 1
            && notes[0] == "DiscordMirrors in Settings.json is not read: the world's own rooms are mirrored into Discord as its operator sets them on the hub's operator page",
            "a file that still names the world's own rooms for Discord is used, and the log says once that the setting is not read: " + string.Join(" | ", notes));
        c.Check(S, ModSettings.Notes("{ \"Slug\": \"alpha\" }").Count == 0, "a file without it says nothing");
        c.Check(S, ModSettings.FromJson("{ \"Slug\": \"alpha\" }").GlobalRoom is null
            && ModSettings.FromJson("{ \"GlobalRoom\": \"Trade\" }").GlobalRoom == "Trade",
            "the Global room's setting is read only when the file names it");
    }

    private static void Joins(Checks c)
    {
        var (w, link, _, _, _) = Ready();
        var joins = link.SentMessages().OfType<RoomJoin>().ToList();
        c.Check(S, joins.Count == 2 && joins[0] is { Room: "global", On: true } && joins[1] is { Room: "general", On: true },
            "the welcome joins the Global room, then the world's own room by its name");
        var status = w.World.Admin(["block"]);
        c.Check(S, status.EndsWith("The Global room is carried in Roleplay. Rooms mirrored into Discord: General.\nNothing is blocked.",
                StringComparison.Ordinal),
            "the operator's status names them: " + status);
    }

    private static void Out(Checks c)
    {
        var (w, link, ann, cy, _) = Ready();
        var (from, shown) = (link.Sent.Count, w.Host.Shown.Count);
        w.Host.SayInRoom(ann, RoomSlot.General, "selling scarabs by the fountain");
        w.Pump();
        var said = Said(link, from);
        c.Check(S, said.Count == 1 && said[0] is { Room: "general", Text: "selling scarabs by the fountain" } && said[0].From.ToString() == "Ann@alpha",
            "a line in the room named for Discord goes to the hub under the room's own name");
        c.Check(S, Rooms(w, ann, shown) is [{ Name: "Ann", Room: RoomSlot.General }] && Rooms(w, cy, shown).Count == 0
            && !w.Host.Shown.Skip(shown).Any(x => x.Kind == "system"),
            "and is said here as the room's own line, the speaker told nothing");
        from = link.Sent.Count;
        w.Host.SayInRoom(ann, RoomSlot.Trade, "trade stays here");
        w.Host.SayInRoom(ann, RoomSlot.Lfg, "so does this");
        w.Pump();
        c.Check(S, Said(link, from).Count == 0, "a room the hub does not mirror sends nothing");
        w.Host.SayInRoom(ann, RoomSlot.Roleplay, "dance at 8");
        w.Pump();
        c.Check(S, Said(link, from) is [{ Room: "global", Text: "dance at 8" }], "the Global room's carrying room goes as before");
    }

    private static void NothingComesBack(Checks c)
    {
        var (w, link, _, _, _) = Ready();
        var shown = w.Host.Shown.Count;
        link.Frame(1, new RoomLine("general", A("Nelly@discord"), "anyone near Arwic?"));
        link.Frame(1, new RoomLine("general", A("Ann@alpha"), "my own line back"));
        link.Frame(1, new RoomLine("general", A("Bob@bravo"), "another world's"));
        link.Frame(1, new RoomLine("trade", A("Bob@bravo"), "a room not named"));
        w.Pump();
        c.Check(S, w.Host.Shown.Skip(shown).All(x => x.Kind != "room"),
            "a room line in one of the world's own rooms is never shown, whoever it is from");
    }

    private static void NotCarried(Checks c)
    {
        var (w, link, ann, _, clock) = Ready();
        var from = link.Sent.Count;
        w.Host.SayInRoom(ann, RoomSlot.General, "first");
        w.Pump();
        var first = Said(link, from).Single();
        link.Frame(1, new RoomResult(first.Id, Outcome.NotMember));
        w.Pump();
        c.Check(S, w.Logs.Count(l => l.Contains("does not mirror this world's room general", StringComparison.Ordinal)) == 1,
            "the hub not mirroring the room is logged");
        var (sent, shown) = (link.Sent.Count, w.Host.Shown.Count);
        for (var i = 0; i < 6; i++)
            w.Host.SayInRoom(ann, RoomSlot.General, $"line {i}");
        w.Pump();
        c.Check(S, Said(link, sent).Count == 0 && w.Host.Shown.Skip(shown).All(x => x.Kind == "room"),
            "then the room's lines stay here, past any pace, and nobody is told anything");
        clock.Now += NetworkWorld.UncarriedSecs;
        w.Host.SayInRoom(ann, RoomSlot.General, "a minute on");
        w.Pump();
        var again = Said(link, sent);
        c.Check(S, again is [{ Text: "a minute on" }], "a minute on, a line goes to the hub again");
        link.Frame(1, new RoomResult(again[0].Id, Outcome.Ok));
        w.Pump();
        sent = link.Sent.Count;
        w.Host.SayInRoom(ann, RoomSlot.General, "and the next");
        w.Pump();
        c.Check(S, Said(link, sent).Count == 1, "and once the hub mirrors it, every line goes");

        // A new link forgets what the hub said.
        sent = link.Sent.Count;
        w.Host.SayInRoom(ann, RoomSlot.General, "not mirrored again");
        w.Pump();
        link.Frame(1, new RoomResult(Said(link, sent).Single().Id, Outcome.NotMember));
        w.Pump();
        link.Down(1);
        link.Up(2);
        link.Frame(2, Welcomed);
        w.Pump();
        clock.Now += 10;
        sent = link.Sent.Count;
        w.Host.SayInRoom(ann, RoomSlot.General, "after the welcome");
        w.Pump();
        c.Check(S, Said(link, sent) is [{ Text: "after the welcome" }], "a line after a new welcome goes at once");
    }

    private static void Rules(Checks c)
    {
        // Off the network: said here, not sent.
        var (w, link, ann, _, _) = Ready();
        w.World.Dn(ann, ["off"]);
        var (sent, shown) = (link.Sent.Count, w.Host.Shown.Count);
        w.Host.SayInRoom(ann, RoomSlot.General, "local only");
        w.Pump();
        c.Check(S, Said(link, sent).Count == 0 && w.Host.Shown.Skip(shown).All(x => x.Kind == "room"),
            "a character off the network is heard here only, and told nothing");

        // Faster than the speaker's pace: the rest stay here, and the speaker is told.
        (w, link, ann, _, _) = Ready();
        sent = link.Sent.Count;
        for (var i = 0; i < 4; i++)
            w.Host.SayInRoom(ann, RoomSlot.General, $"line {i}");
        w.Pump();
        c.Check(S, Said(link, sent).Count == 3 && w.Host.To(ann).Last().Text == "Your line did not reach Discord: you are sending too quickly.",
            "past the speaker's pace, the speaker is told the line did not reach Discord");

        // The world's share spent at the hub.
        var last = Said(link, sent).Last();
        link.Frame(1, new RoomResult(last.Id, Outcome.RateLimited));
        w.Pump();
        c.Check(S, w.Host.To(ann).Last().Text == "Your line did not reach Discord: the Dereth Network is holding back this world's lines for now.",
            "a line the hub holds back tells the speaker it is the world's share");

        // Too new for the operator.
        var (young, yl, yann, _, _) = Ready(minAge: 3600);
        young.Host.Characters[yann].Age = 10;
        sent = yl.Sent.Count;
        young.Host.SayInRoom(yann, RoomSlot.General, "hello Discord");
        young.Pump();
        c.Check(S, Said(yl, sent).Count == 0
            && young.Host.To(yann).Last().Text == "Your line did not reach Discord: this character is too new to send to the Dereth Network yet.",
            "a character too new sends nothing, and is told of Discord");
    }
}
