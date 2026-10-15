// The hub's reach questions on the world half, on a link in memory: a character not online is
// reachable as a shown tell would be, by what the world keeps for it while it is away (its
// squelches of the teller and of the teller's world, the operator's blocks, and the pace of tells
// from the teller's world to it), by anyone else, a stranger included; one
// the world lacks, one online, one off the network and another world's are not, alike; an answer
// shows nothing and gives no stand-in id; a malformed question and an answer from the hub go
// unanswered; and answers wait with those owed to the teller's world, at most its share, dropped
// when the link goes.

namespace Dereth.Network.Harness;

internal static class ReachChecks
{
    private const string S = "units/reach";

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    public static void Run(Checks c)
    {
        Answers(c);
        Squelches(c);
        AnyoneElse(c);
        Blocks(c);
        Pace(c);
        Bounds(c);
    }

    /// <summary>
    /// A world `alpha` welcomed with Ann, Fay and Gus: Ann and Fay on the network, Gus never; Ann
    /// does what `before` says, asks for her link code, then logs off.
    /// </summary>
    private static (TestWorld W, MemoryLink Link, Clock Clock) Away(Action<TestWorld, uint>? before = null)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now);
        var link = (MemoryLink)w.Link;
        var ann = w.Host.Add(0x5000_0001, "Ann").Id;
        var fay = w.Host.Add(0x5000_0002, "Fay").Id;
        w.Host.Add(0x5000_0003, "Gus").Online = false;
        link.Up(1);
        w.Pump();
        link.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test",
            new[] { "alpha", "bravo", "charlie" }.Select(s => new WorldInfo(s, s.ToUpperInvariant(), true)).ToList(),
            [Wire.GlobalRoom], Limits.Default));
        w.Pump();
        w.World.Dn(ann, ["on"]);
        w.World.Dn(fay, ["on"]);
        before?.Invoke(w, ann);
        w.World.Dn(ann, ["link"]);
        clock.Now += 1;
        w.Pump();
        w.Host.Characters[ann].Online = false;
        w.World.OnLogout(ann, "Ann");
        clock.Now += 1;
        w.Pump();
        return (w, link, clock);
    }

    /// <summary>The hub asks `frame`: the reach messages the world sent after it.</summary>
    private static List<ReachMessage> Ask(TestWorld w, MemoryLink link, Clock clock, string frame)
    {
        var mark = link.Sent.Count;
        link.FrameText(1, frame);
        clock.Now += 0.01;
        w.Pump();
        return link.Sent.Skip(mark).Select(s => ReachWire.Read(s.Frame)).Where(r => r.Status == ReadStatus.Ok)
            .Select(r => r.Message!).ToList();
    }

    private static string Question(string id, string from, string to) => ReachWire.Write(new ReachAsk(id, A(from), A(to)));

    /// <summary>Whether the world answered `id` alone, and `reachable`.</summary>
    private static bool Answered(List<ReachMessage> got, string id, bool reachable) =>
        got is [ReachResult r] && r.Id == id && r.Reachable == reachable;

    private static void Answers(Checks c)
    {
        var (w, link, clock) = Away();
        var (shown, ids) = (w.Host.Shown.Count, w.World.StandInCount);
        c.Check(S, Answered(Ask(w, link, clock, Question("q1", "Bob@bravo", "Ann@alpha")), "q1", true),
            "a character away and on the network is reachable");
        c.Check(S, Answered(Ask(w, link, clock, Question("q2", "Bob@bravo", "Zed@alpha")), "q2", false),
            "a character the world does not have is not");
        c.Check(S, Answered(Ask(w, link, clock, Question("q3", "Bob@bravo", "Fay@alpha")), "q3", false),
            "nor one online");
        c.Check(S, Answered(Ask(w, link, clock, Question("q4", "Bob@bravo", "Gus@alpha")), "q4", false),
            "nor one off the network");
        w.Host.Characters[0x5000_0002].Online = false;
        w.World.OnLogout(0x5000_0002, "Fay");
        c.Check(S, Answered(Ask(w, link, clock, Question("q4b", "Bob@bravo", "Fay@alpha")), "q4b", false),
            "nor one away and on the network that never asked for its link: the hub asks of an earlier character of its name");
        c.Check(S, Answered(Ask(w, link, clock, Question("q5", "Bob@bravo", "Ann@charlie")), "q5", false),
            "nor another world's");
        c.Check(S, Answered(Ask(w, link, clock, Question("q6", "Fay@alpha", "Ann@alpha")), "q6", false),
            "nor one asked about a tell from this world's own address, when the character kept no squelches as it left");
        w.Host.Characters[0x5000_0001].Deleted = true;
        c.Check(S, Answered(Ask(w, link, clock, Question("q6d", "Bob@bravo", "Ann@alpha")), "q6d", false),
            "nor one deleted");
        w.Host.Characters[0x5000_0001].Deleted = false;
        c.Check(S, w.Host.Shown.Count == shown && w.World.StandInCount == ids && w.World.AnswersOwed == 0,
            "an answer shows nothing, gives no stand-in id, and is sent at once");
        foreach (var frame in new[]
                 {
                     """{"type":"reach","id":"q7","from":"Bob@bravo","to":"oldguard@channel"}""",
                     """{"type":"reach","id":"","from":"Bob@bravo","to":"Ann@alpha"}""",
                     """{"type":"reach.result","id":"q9","reachable":true}""",
                 })
            c.Check(S, Ask(w, link, clock, frame).Count == 0, $"{frame} goes unanswered");
        c.Check(S, Answered(Ask(w, link, clock, Question("q10", "Bob@bravo", "Ann@alpha")), "q10", true),
            "and the next question is answered");
    }

    private static void Squelches(Checks c)
    {
        var (w, link, clock) = Away((t, ann) =>
        {
            t.World.Squelch(ann, true, 0, "Bob@bravo");
            t.World.Dn(ann, ["squelch", "charlie"]);
        });
        c.Check(S, Answered(Ask(w, link, clock, Question("s1", "Bob@bravo", "Ann@alpha")), "s1", false),
            "a teller the character squelches is not reachable");
        c.Check(S, Answered(Ask(w, link, clock, Question("s2", "Cy@charlie", "Ann@alpha")), "s2", false),
            "nor a teller of a world it squelches");
        c.Check(S, Answered(Ask(w, link, clock, Question("s3", "Dan@bravo", "Ann@alpha")), "s3", true),
            "another teller of the same world is");
    }

    private static void AnyoneElse(Checks c)
    {
        var (w, link, clock) = Away((t, ann) => t.World.AddFriend(ann, "Bob@bravo"));
        c.Check(S, Answered(Ask(w, link, clock, Question("f1", "Eve@bravo", "Ann@alpha")), "f1", true),
            "a stranger the character does not list is reachable");
        c.Check(S, Answered(Ask(w, link, clock, Question("f2", "Bob@bravo", "Ann@alpha")), "f2", true),
            "and so is a friend");
        c.Check(S, Answered(Ask(w, link, clock,
                """{"type":"reach","id":"f3","from":"Cy@charlie","to":"Ann@alpha","friend":false,"person_friend":false}"""), "f3", true),
            "a question of a hub that still names the teller's friendship is read without it");
    }

    private static void Blocks(Checks c)
    {
        var (w, link, clock) = Away();
        w.World.Admin(["block", "bravo"]);
        c.Check(S, Answered(Ask(w, link, clock, Question("b1", "Bob@bravo", "Ann@alpha")), "b1", false),
            "a teller of a world the operator blocks is not reachable");
        c.Check(S, Answered(Ask(w, link, clock, Question("b2", "Cy@charlie", "Ann@alpha")), "b2", true),
            "one of a world it does not block is");
        w.World.Admin(["unblock", "bravo"]);
        w.World.Admin(["block", "Bob@bravo"]);
        c.Check(S, Answered(Ask(w, link, clock, Question("b3", "Bob@bravo", "Ann@alpha")), "b3", false),
            "nor a teller the operator blocks");
        c.Check(S, Answered(Ask(w, link, clock, Question("b4", "Dan@bravo", "Ann@alpha")), "b4", true),
            "and another of its world is");
    }

    private static void Pace(Checks c)
    {
        var (w, link, clock) = Away();
        var yes = 0;
        var answered = 0;
        for (var i = 0; i < 25; i++)
        {
            var got = Ask(w, link, clock, Question($"p{i}", "Bob@bravo", "Ann@alpha"));
            answered += got.Count;
            yes += got.OfType<ReachResult>().Count(r => r.Reachable);
        }
        c.Check(S, answered == 25 && yes == 20,
            $"a yes is held to the pace of tells from one world to one character, a burst of 20 ({yes} of {answered})");
        c.Check(S, Answered(Ask(w, link, clock, Question("p-c", "Cy@charlie", "Ann@alpha")), "p-c", true),
            "another world's tells are not held to it");
        clock.Now += 1.1;
        c.Check(S, Answered(Ask(w, link, clock, Question("p-later", "Bob@bravo", "Ann@alpha")), "p-later", true),
            "and the pace refills");
    }

    private static void Bounds(Checks c)
    {
        var (w, link, clock) = Away();
        var mark = link.Sent.Count;
        for (var i = 0; i < 150; i++)
            link.FrameText(1, Question($"o{i}", "Dan@bravo", "Zed@alpha"));
        clock.Now += 0.01;
        w.Pump();
        clock.Now += 3;
        w.Pump();
        var answers = link.Sent.Skip(mark).Count(s => ReachWire.Read(s.Frame).Status == ReadStatus.Ok);
        c.Check(S, answers == NetworkWorld.MaxAnswersPerWorld,
            $"at most a world's share of answers is owed at once; the rest go unanswered ({answers})");

        mark = link.Sent.Count;
        for (var i = 0; i < 5; i++)
            link.FrameText(1, Question($"d{i}", "Dan@bravo", "Zed@alpha"));
        link.Down(1);
        w.Pump();
        link.Up(2);
        w.Pump();
        link.Frame(2, new Welcome(Wire.Protocol, "alpha", "hub test", [new WorldInfo("alpha", "ALPHA", true)], [Wire.GlobalRoom],
            Limits.Default));
        clock.Now += 1;
        w.Pump();
        c.Check(S, link.Sent.Skip(mark).All(s => ReachWire.Read(s.Frame).Status != ReadStatus.Ok) && w.World.AnswersOwed == 0,
            "answers owed when the link goes are dropped, not sent on the next one");
    }
}
