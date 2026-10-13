// The hub's says on the world half, on a link in memory: a line a person said in Discord is said as
// its character, online or not, to the room's local listeners under the character's own id and its
// name marked (`Ann (Discord)`), and sent on as a `room.say` with the same mark, in the Global room
// and in one of the world's own rooms for Discord; it is refused, with its reason, and nothing shown
// or sent, for a character the world does not have, one off the network, one that never asked for its
// link, a banned account, a gagged character, one too new, one the world's own rules keep out of the
// room (an Olthoi player), one over its pace and a room not carried;
// a line from another world marked as said in Discord is shown with the mark, under the address's own
// stand-in id; and the answers go at once, never queued.

namespace Dereth.Network.Harness;

internal static class SayChecks
{
    private const string S = "units/say";

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    private const uint Ann = 0x5000_0001;
    private const uint Bo = 0x5000_0002;
    private const uint Cy = 0x5000_0003;
    private const uint Dee = 0x5000_0004;
    private const uint Gus = 0x5000_0005;
    private const uint Eli = 0x5000_0006;

    /// <summary>
    /// A world `alpha` carrying the Global room in Roleplay, whose General a hub last said it
    /// mirrors, linked and welcomed: Ann on the network and linked by her own `/dn link`; Bo
    /// listening; Cy listening and squelching Ann; Dee on the network but never linked; Gus never on;
    /// Eli listening to nothing.
    /// </summary>
    private static (TestWorld W, MemoryLink Link, Clock Clock) Ready(uint minAge = 0, RoomSlot? global = RoomSlot.Roleplay)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now, room: global, minAge: minAge,
            mirrored: [RoomSlot.General]);
        var link = (MemoryLink)w.Link;
        w.Host.Add(Ann, "Ann");
        w.Host.Add(Bo, "Bo");
        w.Host.Add(Cy, "Cy").SquelchedLocals.Add(Ann);
        w.Host.Add(Dee, "Dee");
        w.Host.Add(Gus, "Gus");
        w.Host.Add(Eli, "Eli").ListensToRoom = false;
        link.Up(1);
        w.Pump();
        link.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test",
            [new WorldInfo("alpha", "ALPHA", true), new WorldInfo("bravo", "BRAVO", true)], [Wire.GlobalRoom], Limits.Default));
        w.Pump();
        w.World.Dn(Ann, ["on"]);
        w.World.Dn(Dee, ["on"]);
        w.World.Dn(Ann, ["link"]);
        clock.Now += 1;
        w.Pump();
        return (w, link, clock);
    }

    /// <summary>The hub asks `frame`: the say answers and the room lines the world sent after it, and what it showed.</summary>
    private static (List<SayMessage> Answers, List<RoomSay> Lines, List<Shown> Shown) Ask(TestWorld w, MemoryLink link, Clock clock,
        string frame)
    {
        var (sent, shown) = (link.Sent.Count, w.Host.Shown.Count);
        link.FrameText(1, frame);
        clock.Now += 0.01;
        w.Pump();
        var frames = link.Sent.Skip(sent).Select(s => s.Frame).ToList();
        return (frames.Select(SayWire.Read).Where(r => r.Status == ReadStatus.Ok).Select(r => r.Message!).ToList(),
            frames.Select(f => Wire.Read(f).Message).OfType<RoomSay>().ToList(),
            w.Host.Shown.Skip(shown).ToList());
    }

    private static string Say(string id, string who, string text = "evening all", string room = "global", string via = "discord") =>
        SayWire.Write(new SayAsk(id, room, A(who), text, via));

    private static bool Said(List<SayMessage> got, string id) => got is [SayResult { Outcome: SayOutcome.Said, Reason: null } r] && r.Id == id;

    private static bool Refused(List<SayMessage> got, string id, string reason) =>
        got is [SayResult { Outcome: SayOutcome.Refused } r] && r.Id == id && r.Reason == reason;

    public static void Run(Checks c)
    {
        SaidInGlobal(c);
        SaidAway(c);
        SaidInOwnRoom(c);
        Refusals(c);
        Pace(c);
        TheMark(c);
        Bounds(c);
        Signing(c);
    }

    /// <summary>
    /// The harness's own Ed25519 signer, which signs the interactions it hands a real hub, against
    /// RFC 8032's first test: its public key, and its signature of the empty message.
    /// </summary>
    private static void Signing(Checks c)
    {
        var secret = Convert.FromHexString("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        c.Check("units/ed25519", Convert.ToHexStringLower(Ed25519.PublicKey(secret))
                == "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "the harness's signer makes RFC 8032's public key of its first test");
        c.Check("units/ed25519", Convert.ToHexStringLower(Ed25519.Sign(secret, []))
                == "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            "and its signature of the empty message");
    }

    private static void SaidInGlobal(Checks c)
    {
        var (w, link, clock) = Ready();
        var (answers, lines, shown) = Ask(w, link, clock, Say("s1", "Ann@alpha"));
        c.Check(S, Said(answers, "s1"), "a linked character on the network says the line: said");
        var heard = shown.Where(x => x.Kind == "room").ToList();
        c.Check(S, heard.Select(x => x.To).Order().SequenceEqual([Ann, Bo, Dee, Gus])
                && heard.All(x => x.Name == "Ann (Discord)" && x.Id == Ann && x.Text == "evening all" && x.Room == RoomSlot.Roleplay),
            "every local listener of the carrying room hears it as the room's own line, under the character's id, its name marked: "
            + string.Join("; ", heard));
        c.Check(S, heard.All(x => x.To != Cy && x.To != Eli), "nobody who squelches the character, or does not listen to the room, hears it");
        c.Check(S, lines is [{ Room: "global", Text: "evening all", Via: "discord" } l] && l.From.ToString() == "Ann@alpha",
            "and it goes on to the hub as a room line of the Global room from the character, marked: " + string.Join("; ", lines));
        c.Check(S, shown.All(x => x.Kind == "room"), "the character is told nothing: its player said it");
        c.Check(S, link.Sent.Select(s => s.Frame).Any(f => f.Contains("\"via\":\"discord\"", StringComparison.Ordinal)),
            "the mark is on the frame the hub is sent");
    }

    private static void SaidAway(Checks c)
    {
        var (w, link, clock) = Ready();
        w.Host.Characters[Ann].Online = false;
        w.World.OnLogout(Ann, "Ann");
        clock.Now += 1;
        w.Pump();
        var (answers, lines, shown) = Ask(w, link, clock, Say("a1", "ann@alpha", "back later"));
        c.Check(S, Said(answers, "a1") && lines is [{ Via: "discord" } l] && l.From.ToString() == "Ann@alpha",
            "a character not online says it too, under the name the world gives it");
        c.Check(S, shown.Where(x => x.Kind == "room").Select(x => x.To).Order().SequenceEqual([Bo, Dee, Gus])
                && shown.All(x => x.Name == "Ann (Discord)" && x.Id == Ann),
            "heard by the listeners online, under the character's own id");
    }

    private static void SaidInOwnRoom(Checks c)
    {
        var (w, link, clock) = Ready();
        w.Host.Characters[Bo].Rooms = [RoomSlot.Roleplay];
        var (answers, lines, shown) = Ask(w, link, clock, Say("o1", "Ann@alpha", "auction at noon", room: "general"));
        c.Check(S, Said(answers, "o1") && lines is [{ Room: "general", Via: "discord" }],
            "a line in one of the world's own rooms for Discord is said there, and goes to the hub in that room, marked");
        c.Check(S, shown.Where(x => x.Kind == "room").All(x => x.Room == RoomSlot.General && x.Name == "Ann (Discord)")
                && shown.All(x => x.To != Bo),
            "heard by that room's listeners alone");
    }

    private static void Refusals(Checks c)
    {
        var (w, link, clock) = Ready();
        void No(string id, string frame, string reason, string what)
        {
            var (answers, lines, shown) = Ask(w, link, clock, frame);
            c.Check(S, Refused(answers, id, reason) && lines.Count == 0 && shown.Count == 0,
                $"{what}: refused, {reason}, and nothing shown or sent ({string.Join("; ", answers)})");
        }
        No("r1", Say("r1", "Zed@alpha"), SayReasons.NoCharacter, "a character the world does not have");
        No("r2", Say("r2", "Ann@bravo"), SayReasons.NoCharacter, "another world's");
        w.Host.Characters[Ann].Deleted = true;
        No("r3", Say("r3", "Ann@alpha"), SayReasons.NoCharacter, "one deleted");
        w.Host.Characters[Ann].Deleted = false;
        No("r4", Say("r4", "Gus@alpha"), SayReasons.NotOnNetwork, "one never on the network");
        No("r5", Say("r5", "Dee@alpha"), SayReasons.NotLinked,
            "one on the network that never asked for its link: the person linked an earlier character of its name");
        w.Host.Characters[Ann].Banned = true;
        No("r6", Say("r6", "Ann@alpha"), SayReasons.Banned, "one whose account is banned");
        w.Host.Characters[Ann].Banned = false;
        w.Host.Characters[Ann].Gagged = true;
        No("r7", Say("r7", "Ann@alpha"), SayReasons.Gagged, "one gagged");
        w.Host.Characters[Ann].Gagged = false;
        No("r8", Say("r8", "Ann@alpha", room: "trade"), SayReasons.RoomOff, "a room the world does not carry");
        w.Host.Characters[Ann].Olthoi = true;
        No("r8o", Say("r8o", "Ann@alpha"), SayReasons.NotAllowed, "one the world's own rules keep out of the room, an Olthoi player");
        w.Host.Characters[Ann].Online = false;
        No("r8p", Say("r8p", "Ann@alpha"), SayReasons.NotAllowed, "and so while it is not online");
        w.Host.Characters[Ann].Online = true;
        w.Host.Characters[Ann].Olthoi = false;
        w.World.Dn(Ann, ["off"]);
        No("r9", Say("r9", "Ann@alpha"), SayReasons.NotOnNetwork, "one its player took off the network");
        w.World.Dn(Ann, ["on"]);
        var (said, _, _) = Ask(w, link, clock, Say("r10", "Ann@alpha"));
        c.Check(S, Said(said, "r10"), "and the next line, every rule kept, is said");

        var (young, ylink, yclock) = Ready(minAge: 3600);
        young.Host.Characters[Ann].Age = 60;
        var (answers, lines, shown) = Ask(young, ylink, yclock, Say("y1", "Ann@alpha"));
        c.Check(S, Refused(answers, "y1", SayReasons.TooNew) && lines.Count == 0 && shown.Count == 0,
            "one younger than the operator's minimum age: refused, too_new");
        young.Host.Characters[Ann].Age = 7200;
        c.Check(S, Said(Ask(young, ylink, yclock, Say("y2", "Ann@alpha")).Answers, "y2"), "and old enough, said");

        var (none, nlink, nclock) = Ready(global: null);
        var (n, nl, ns) = Ask(none, nlink, nclock, Say("n1", "Ann@alpha"));
        c.Check(S, Refused(n, "n1", SayReasons.RoomOff) && nl.Count == 0 && ns.Count == 0,
            "the Global room on a world that carries none: refused, room_off");

        var (empty, elink, eclock) = Ready();
        var (e, el, es) = Ask(empty, elink, eclock, Say("e1", "Ann@alpha", " \n\t "));
        c.Check(S, e is [SayResult { Outcome: SayOutcome.Refused, Reason: null }] && el.Count == 0 && es.Count == 0,
            "a line nothing is left of through the text rules: refused, without a reason");
    }

    private static void Pace(Checks c)
    {
        var (w, link, clock) = Ready();
        var burst = (int)Limits.Default.RoomLinesPerCharacter.Burst;
        var said = 0;
        for (var i = 0; i < burst; i++)
            said += Said(Ask(w, link, clock, Say($"p{i}", "Ann@alpha")).Answers, $"p{i}") ? 1 : 0;
        var (answers, lines, shown) = Ask(w, link, clock, Say("p-over", "Ann@alpha"));
        c.Check(S, said == burst && Refused(answers, "p-over", SayReasons.RateLimited) && lines.Count == 0 && shown.Count == 0,
            $"a character's lines are held to its pace of room lines, a burst of {burst}: then refused, rate_limited");
        clock.Now += 1 / Limits.Default.RoomLinesPerCharacter.PerSec + 0.1;
        w.Host.Characters[Ann].Gagged = true;
        var gagged = Ask(w, link, clock, Say("p-gag", "Ann@alpha")).Answers;
        w.Host.Characters[Ann].Gagged = false;
        c.Check(S, Refused(gagged, "p-gag", SayReasons.Gagged) && Said(Ask(w, link, clock, Say("p-later", "Ann@alpha")).Answers, "p-later"),
            "the pace refills, and a line refused for another reason takes none of it");
    }

    private static void TheMark(Checks c)
    {
        var (w, link, clock) = Ready();
        var mark = w.Host.Shown.Count;
        link.Frame(1, new RoomLine(Wire.GlobalRoom, A("Bob@bravo"), "hello there"));
        link.Frame(1, new RoomLine(Wire.GlobalRoom, A("Bob@bravo"), "from discord", SayWire.ViaDiscord));
        link.Frame(1, new RoomLine(Wire.GlobalRoom, A("Bob@bravo"), "from somewhere new", "site"));
        clock.Now += 0.01;
        w.Pump();
        var bo = w.Host.Shown.Skip(mark).Where(x => x.To == Bo && x.Kind == "room").ToList();
        c.Check(S, bo.Select(x => x.Name).SequenceEqual(["Bob@bravo", "Bob@bravo (Discord)", "Bob@bravo (Site)"]),
            "a line from another world shows its mark beside the speaker's address, a mark this version does not name as its word: "
            + string.Join(", ", bo.Select(x => x.Name)));
        c.Check(S, bo.Count == 3 && bo.All(x => x.Id == bo[0].Id) && w.World.StandInFor(bo[0].Id)?.ToString() == "Bob@bravo",
            "under the address's own stand-in id, marked or not");
        w.World.Squelch(Bo, true, 0, "Bob@bravo");
        mark = w.Host.Shown.Count;
        link.Frame(1, new RoomLine(Wire.GlobalRoom, A("Bob@bravo"), "again", SayWire.ViaDiscord));
        clock.Now += 0.01;
        w.Pump();
        c.Check(S, !w.Host.Shown.Skip(mark).Any(x => x.To == Bo),
            "and a squelch of the address holds for its marked lines");
    }

    private static void Bounds(Checks c)
    {
        var (w, link, clock) = Ready();
        foreach (var frame in new[]
                 {
                     """{"type":"say","id":"b1","room":"global","who":"oldguard@channel","text":"x","via":"discord"}""",
                     """{"type":"say","id":"b2","room":"global","who":"Ann@alpha","text":"x"}""",
                     """{"type":"say","id":"b3","room":"global","who":"Ann@alpha","text":"x","via":"Discord"}""",
                     """{"type":"say.result","id":"b4","outcome":"said"}""",
                 })
        {
            var (answers, lines, shown) = Ask(w, link, clock, frame);
            c.Check(S, answers.Count == 0 && lines.Count == 0 && shown.Count == 0, $"{frame} goes unanswered, and nothing is said");
        }
        c.Check(S, w.World.AnswersOwed == 0 && w.World.Queued == 0, "an answer is sent at once, never queued");
        var sent = link.Sent.Count;
        link.Down(1);
        link.FrameText(1, Say("b5", "Ann@alpha"));
        clock.Now += 0.01;
        w.Pump();
        c.Check(S, link.Sent.Skip(sent).All(s => SayWire.Read(s.Frame).Status != ReadStatus.Ok)
                && link.Sent.Skip(sent).All(s => Wire.Read(s.Frame).Message is not RoomSay),
            "a say that comes while the link is down is not taken");
    }
}
