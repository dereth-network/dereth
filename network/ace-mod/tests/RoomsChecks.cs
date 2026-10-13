// A world's rooms as its hub says, on a link in memory: the hub's `rooms` are applied at once (the
// Global room carried where it says, its own rooms mirrored joined and left), kept in the store
// across a restart without the hub; a hub that holds no room for the world is told the world's
// setting once on each link, and a setting that differs from the hub's room is logged once; the
// operator's `/dn admin global` asks the hub and says where the room is now; and a name given with
// the mark a line said from Discord shows names the character without it, for tells, friends,
// squelches and `/dn`'s commands.

namespace Dereth.Network.Harness;

internal static class RoomsChecks
{
    private const string S = "units/rooms";

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static readonly Welcome Welcomed = new(Wire.Protocol, "alpha", "hub test",
        [new WorldInfo("alpha", "ALPHA", true), new WorldInfo("bravo", "BRAVO", true)], [Wire.GlobalRoom], Limits.Default);

    private static RoomsSetup Setup(string? global, params string[] mirrored) => new(null, global, mirrored);

    private static void Hub(MemoryLink link, RoomsMessage m, long epoch = 1) => link.FrameText(epoch, RoomsWire.Write(m));

    /// <summary>
    /// A world `alpha`, linked and welcomed, with Ann (listening to every room) and Cy on the network,
    /// and Boss, its operator, online.
    /// </summary>
    private static (TestWorld W, MemoryLink Link, uint Ann, uint Cy, uint Boss) Ready(NetworkStore? store = null, RoomSlot? room = RoomSlot.Roleplay,
        bool named = false)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now, room: room, store: store, named: named);
        var link = (MemoryLink)w.Link;
        var ann = w.Host.Add(0x5000_0001, "Ann");
        var cy = w.Host.Add(0x5000_0002, "Cy");
        var boss = w.Host.Add(0x5000_0003, "Boss");
        link.Up(1);
        w.Pump();
        link.Frame(1, Welcomed);
        w.Pump();
        w.World.Dn(ann.Id, ["on"]);
        w.World.Dn(cy.Id, ["on"]);
        w.Pump();
        return (w, link, ann.Id, cy.Id, boss.Id);
    }

    private static List<RoomsGlobal> Asked(MemoryLink link, int from = 0) =>
        link.Sent.Skip(from).Select(s => RoomsWire.Read(s.Frame)).Where(r => r.Status == ReadStatus.Ok)
            .Select(r => r.Message).OfType<RoomsGlobal>().ToList();

    private static List<RoomJoin> Joins(MemoryLink link, int from) =>
        link.Sent.Skip(from).Select(s => Wire.Read(s.Frame).Message).OfType<RoomJoin>().ToList();

    private static List<RoomSay> Said(MemoryLink link, int from) =>
        link.Sent.Skip(from).Select(s => Wire.Read(s.Frame).Message).OfType<RoomSay>().ToList();

    public static void Run(Checks c)
    {
        Applied(c);
        Seeded(c);
        Admin(c);
        Kept(c);
        Marked(c);
    }

    private static void Applied(Checks c)
    {
        var (w, link, ann, _, _) = Ready();
        var from = link.Sent.Count;
        Hub(link, Setup("trade", "general"));
        w.Pump();
        c.Check(S, w.World.GlobalSlot == RoomSlot.Trade && w.World.MirroredSlots.SequenceEqual([RoomSlot.General]),
            "the hub's rooms hold at once");
        c.Check(S, Joins(link, from) is [{ Room: "general", On: true }],
            "the newly mirrored room is joined, and the Global room stays joined");
        from = link.Sent.Count;
        w.Host.SayInRoom(ann, RoomSlot.Trade, "trade carries it");
        w.Host.SayInRoom(ann, RoomSlot.Roleplay, "roleplay stays here");
        w.Host.SayInRoom(ann, RoomSlot.General, "general goes to Discord");
        w.Pump();
        c.Check(S, Said(link, from) is [{ Room: "global", Text: "trade carries it" }, { Room: "general", Text: "general goes to Discord" }],
            "Trade's lines go as the Global room's, General's under its own name, and Roleplay's stay here");
        var shown = w.Host.Shown.Count;
        link.Frame(1, new RoomLine("global", Address.Parse("Bob@bravo")!, "hello alpha"));
        w.Pump();
        c.Check(S, w.Host.Shown.Skip(shown).Where(x => x.Kind == "room").All(x => x.Room == RoomSlot.Trade)
            && w.Host.Shown.Skip(shown).Any(x => x.Kind == "room"),
            "another world's line is shown in Trade");
        c.Check(S, w.World.StoreJson().Contains("\"global\": \"trade\"", StringComparison.Ordinal), "and kept to be saved");

        from = link.Sent.Count;
        Hub(link, Setup("off"));
        w.Pump();
        c.Check(S, w.World.GlobalSlot is null && w.World.MirroredSlots.Count == 0
            && Joins(link, from) is [{ Room: "global", On: false }, { Room: "general", On: false }],
            "carried nowhere and mirrored nowhere, the world leaves both rooms");
        from = link.Sent.Count;
        w.World.Global(ann, "anyone?");
        w.Host.SayInRoom(ann, RoomSlot.Trade, "only here");
        w.Pump();
        c.Check(S, Said(link, from).Count == 0 && w.Host.To(ann).Any(x => x.Text == NetworkWorld.NotCarriedLine),
            "nothing goes, and /dng says the world carries no Global room");
    }

    private static void Seeded(Checks c)
    {
        var (w, link, _, _, _) = Ready(room: RoomSlot.Lfg, named: true);
        Hub(link, Setup(null));
        w.Pump();
        c.Check(S, Asked(link) is [{ Room: "lfg", Seed: true }], "a hub holding no room is told the setting's, once");
        Hub(link, Setup(null));
        w.Pump();
        c.Check(S, Asked(link).Count == 1, "and not again on the same link");
        Hub(link, Setup("general"));
        w.Pump();
        Hub(link, Setup("general"));
        w.Pump();
        c.Check(S, w.World.GlobalSlot == RoomSlot.General
            && w.Logs.Count(l => l.Contains("so GlobalRoom (lfg) is not read", StringComparison.Ordinal)) == 1,
            "the hub's room holds over the setting, and the log says so once");
        link.Down(1);
        link.Up(2);
        link.Frame(2, Welcomed);
        Hub(link, Setup("general"), 2);
        w.Pump();
        c.Check(S, Asked(link).Count == 1, "a hub holding a room is never sent the setting");

        // A setting of none is told as none; the hub's word, once given, is what the world sends.
        var (none, nl, _, _, _) = Ready(room: null, named: true);
        Hub(nl, Setup(null));
        none.Pump();
        c.Check(S, Asked(nl) is [{ Room: "off", Seed: true }], "a world carrying no room tells a hub that holds none so");
        var kept = new NetworkStore { Rooms = new KeptRooms("trade", []) };
        var (back, bl, _, _, _) = Ready(store: kept);
        Hub(bl, Setup(null));
        back.Pump();
        c.Check(S, Asked(bl) is [{ Room: "trade", Seed: true }],
            "a hub that lost the world's room is told the one it gave last");
    }

    private static void Admin(Checks c)
    {
        var (w, link, ann, _, boss) = Ready();
        c.Check(S, w.World.Admin(["global"]) ==
            "This world carries the Global room in Roleplay, as GlobalRoom says, until the Dereth Network's hub keeps a room. Type @dn admin global <general|trade|lfg|roleplay|off> to move it.",
            "the setting's room, until a hub keeps one");
        c.Check(S, w.World.Admin(["global", "trade"], boss) ==
            "This network's hub does not keep where a world carries the Global room: this world carries it as GlobalRoom says.",
            "a hub that never told the world its rooms keeps none");
        Hub(link, Setup("roleplay"));
        w.Pump();
        c.Check(S, w.World.Admin(["global", "lobby"]) == "dn admin global <general|trade|lfg|roleplay|off> - carry the Global room in another of this world's rooms",
            "a word that is no room is answered with the usage");
        var from = link.Sent.Count;
        var asked = w.World.Admin(["global", "Trade"], boss);
        w.Pump();
        var sent = Asked(link, from);
        c.Check(S, asked == "Asking the Dereth Network's hub to carry this world's Global room in Trade..." && sent is [{ Room: "trade", Seed: false }],
            "the operator's word goes to the hub: " + asked);
        c.Check(S, w.World.GlobalSlot == RoomSlot.Roleplay, "and holds only once the hub says so");
        Hub(link, new RoomsSetup(sent[0].Id, "trade", []));
        w.Pump();
        c.Check(S, w.World.GlobalSlot == RoomSlot.Trade
            && w.Host.To(boss).LastOrDefault()?.Text == "The Dereth Network's hub carries this world's Global room in Trade now.",
            "the hub's answer holds, and the operator is told");
        c.Check(S, w.World.Admin(["global"]).StartsWith("This world carries the Global room in Trade, as the Dereth Network's hub keeps it.", StringComparison.Ordinal),
            "and the status says where, and whence");
        w.World.Admin(["global", "off"], boss);
        link.Down(1);
        w.Pump();
        c.Check(S, w.Host.To(boss).LastOrDefault()?.Text == "The link to the Dereth Network's hub went before it answered. Type @dn admin global again once it is back.",
            "an operator whose answer will not come is told");
        c.Check(S, w.World.Admin(["global", "lfg"], boss) == "The Dereth Network's hub can't be reached now. Try again once the link is up.",
            "without the hub there is nobody to ask");
        c.Check(S, w.World.GlobalSlot == RoomSlot.Trade && ann != 0, "nothing changed without the hub");
    }

    private static void Kept(Checks c)
    {
        var (w, link, ann, _, _) = Ready();
        Hub(link, Setup("lfg", "general", "trade"));
        w.Pump();
        var saved = NetworkStore.FromJson(w.World.StoreJson());
        c.Check(S, saved.Rooms is { Global: "lfg" } r && r.Mirrored.SequenceEqual(["general", "trade"]), "the store keeps the hub's rooms");
        // The world starts again from its store, with no hub.
        var again = TestWorld.Make("alpha", new MemoryLink(), () => 1000, store: saved);
        var bee = again.Host.Add(0x5000_0001, "Ann");
        again.World.Dn(bee.Id, ["on"]);
        again.World.Global(bee.Id, "still here");
        c.Check(S, again.World.GlobalSlot == RoomSlot.Lfg && again.World.MirroredSlots.SequenceEqual([RoomSlot.General, RoomSlot.Trade])
            && again.Host.Shown.Any(x => x is { Kind: "room", Room: RoomSlot.Lfg, Text: "still here" }) && ann != 0,
            "and starts right without the hub: /dng is said in LFG");
    }

    private static void Marked(Checks c)
    {
        var (w, link, ann, _, _) = Ready();
        c.Check(S, NetworkWorld.GivenName("Ann (Discord)") == "Ann" && NetworkWorld.GivenName("Ann@bravo (DISCORD) ") == "Ann@bravo"
            && NetworkWorld.GivenName("Ann") == "Ann",
            "a name loses the mark a line said from Discord shows, and nothing else");
        var routed = w.World.TellByName(ann, "Cy (Discord)", "hi Cy");
        c.Check(S, routed == Routed.Local("Cy"), "a local name clicked in a marked line is a local tell to the character: " + routed);
        c.Check(S, w.World.TellByName(ann, "Cy", "hi") == Routed.NotMine, "a plain local name is the world's own");
        var from = link.Sent.Count;
        c.Check(S, w.World.TellByName(ann, "Bob@bravo (discord)", "hi Bob") == Routed.Handled, "a marked address goes to the network");
        w.Pump();
        c.Check(S, link.Sent.Skip(from).Select(s => Wire.Read(s.Frame).Message).OfType<Tell>().Single().To.ToString() == "Bob@bravo",
            "as a tell to the address");
        c.Check(S, w.World.AddFriend(ann, "Cy (Discord)") && w.Host.To(ann).Last() is { Kind: "local-friend", Name: "Cy" },
            "a marked local name is the world's own friend, without the mark");
        c.Check(S, !w.World.AddFriend(ann, "Cy"), "a plain local name is left to the world");
        c.Check(S, w.World.AddFriend(ann, "Bob@bravo (Discord)") && w.World.FriendRows(ann).Any(r => r.Name == "Bob@bravo"),
            "a marked address is a friend on another world");
        w.World.Dn(ann, ["friends", "remove", "Bob@bravo", "(Discord)"]);
        c.Check(S, !w.World.FriendRows(ann).Any(r => r.Name == "Bob@bravo"), "/dn friends remove takes it, marked");
        c.Check(S, w.World.Squelch(ann, true, 0, "Bob@bravo (Discord)") && !w.World.Squelch(ann, true, 0, "Cy (Discord)"),
            "a marked address is squelched on the network; a marked local name is the world's own");
        c.Check(S, w.World.Admin(["block", "Bob@bravo", "(Discord)"]) == "bob@bravo is blocked for everyone on this world.",
            "the operator's block takes a marked address");
    }
}
