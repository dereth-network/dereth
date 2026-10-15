// Tells between two characters of one world while the one told is away and linked, on a link in
// memory: a tell by name to it goes to the hub, so that it may reach its player's Discord, from a
// sender on the network and while the link is up; one to a character online, off the network or
// never linked stays the world's own. The hub's question about such a tell is answered by the tell
// squelches the character kept as it left; its player's reply from Discord is shown as a tell from
// its address; and a tell from this world's own address that is no such reply is refused.

namespace Dereth.Network.Harness;

internal static class SameWorldChecks
{
    private const string S = "units/same-world";

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    private const uint Ann = 0x5000_0001;
    private const uint Fay = 0x5000_0002;
    private const uint Hal = 0x5000_0003;
    private const uint Ivy = 0x5000_0004;
    private const uint Jo = 0x5000_0005;

    public static void Run(Checks c)
    {
        Routing(c);
        Reach(c);
        Replies(c);
    }

    /// <summary>
    /// A world `alpha` welcomed with Ann on the network and linked, who does what `before` says and
    /// logs off with the squelches she has (none kept when `keep` is false); Fay and Jo online on the
    /// network; Hal online off it; Ivy on the network, never linked, away.
    /// </summary>
    private static (TestWorld W, MemoryLink Link, Clock Clock) Away(Action<FakeHost>? before = null, bool keep = true)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now);
        var link = (MemoryLink)w.Link;
        w.Host.Add(Ann, "Ann").Account = "annacct";
        w.Host.Add(Fay, "Fay").Account = "fayacct";
        w.Host.Add(Hal, "Hal").Account = "halacct";
        w.Host.Add(Ivy, "Ivy").Account = "ivyacct";
        w.Host.Add(Jo, "Jo").Account = "joacct";
        link.Up(1);
        w.Pump();
        link.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test",
            new[] { "alpha", "bravo" }.Select(s => new WorldInfo(s, s.ToUpperInvariant(), true)).ToList(),
            [Wire.GlobalRoom], Limits.Default));
        w.Pump();
        foreach (var who in new[] { Ann, Fay, Ivy, Jo })
            w.World.Dn(who, ["on"]);
        w.World.Dn(Ann, ["link"]);
        before?.Invoke(w.Host);
        clock.Now += 1;
        w.Pump();
        foreach (var (who, name) in new[] { (Ann, "Ann"), (Ivy, "Ivy") })
        {
            w.Host.Characters[who].Online = false;
            w.World.OnLogout(who, name, null, keep ? w.Host.KeptSquelches(who) : null);
        }
        clock.Now += 1;
        w.Pump();
        return (w, link, clock);
    }

    /// <summary>`from` tells `name` by name: how the world routed it, and the tells it sent the hub.</summary>
    private static (Routed Routed, List<Tell> Sent) TellByName(TestWorld w, MemoryLink link, Clock clock, uint from, string name)
    {
        var mark = link.Sent.Count;
        var routed = w.World.TellByName(from, name, "are you there?");
        clock.Now += 0.01;
        w.Pump();
        return (routed, link.SentMessages().Skip(mark).OfType<Tell>().ToList());
    }

    private static void Routing(Checks c)
    {
        var (w, link, clock) = Away();
        var shownBefore = w.Host.To(Fay).Count;
        var (routed, sent) = TellByName(w, link, clock, Fay, "Ann");
        c.Check(S, routed.Kind == RouteKind.Handled
                && sent is [{ } t] && t.From.ToString() == "Fay@alpha" && t.To.ToString() == "Ann@alpha" && t.Text == "are you there?",
            "a tell by name to a character of this world away, on the network and linked goes to the hub");
        c.Check(S, w.Host.To(Fay).Count == shownBefore, "and the sender is shown nothing until the hub answers");
        link.Frame(1, new TellResult(sent[0].Id, Outcome.Offline, null));
        clock.Now += 0.01;
        w.Pump();
        c.Check(S, w.Host.To(Fay).Skip(shownBefore).ToList() is [{ Kind: "not-available" }],
            "then it is answered as any tell to a character not online: Character not available");

        foreach (var name in new[] { "Ann@alpha", "ann", "Ann (Discord)" })
        {
            (routed, sent) = TellByName(w, link, clock, Fay, name);
            c.Check(S, routed.Kind == RouteKind.Handled && sent.Count == 1, $"so does one to `{name}`");
        }
        (routed, sent) = TellByName(w, link, clock, Hal, "Ann");
        c.Check(S, routed.Kind == RouteKind.NotMine && sent.Count == 0,
            "a sender off the network keeps the world's own tell, as for a tell to another world");
        (routed, sent) = TellByName(w, link, clock, Fay, "Jo");
        c.Check(S, routed.Kind == RouteKind.NotMine && sent.Count == 0, "a tell to a character online stays the world's own");
        (routed, sent) = TellByName(w, link, clock, Fay, "Jo@alpha");
        c.Check(S, routed == Routed.Local("Jo") && sent.Count == 0, "and so does one to its address");
        (routed, sent) = TellByName(w, link, clock, Fay, "Ivy");
        c.Check(S, routed.Kind == RouteKind.NotMine && sent.Count == 0, "and one to a character away that never asked for its link");
        (routed, sent) = TellByName(w, link, clock, Fay, "Gus");
        c.Check(S, routed.Kind == RouteKind.NotMine && sent.Count == 0, "and one to a name the world does not have");
        w.World.Dn(Ann, ["off"]);
        (routed, sent) = TellByName(w, link, clock, Fay, "Ann");
        c.Check(S, routed.Kind == RouteKind.NotMine && sent.Count == 0, "and one to a linked character off the network");

        (w, link, clock) = Away();
        link.Down(1);
        w.Pump();
        (routed, sent) = TellByName(w, link, clock, Fay, "Ann");
        c.Check(S, routed.Kind == RouteKind.NotMine && sent.Count == 0, "and every one while the link is down");
    }

    /// <summary>The hub asks whether `from` reaches `to`: true or false as the world answers, null for no answer.</summary>
    private static bool? Ask(TestWorld w, MemoryLink link, Clock clock, string id, string from, string to)
    {
        var mark = link.Sent.Count;
        link.FrameText(1, ReachWire.Write(new ReachAsk(id, A(from), A(to))));
        clock.Now += 0.01;
        w.Pump();
        var got = link.Sent.Skip(mark).Select(s => ReachWire.Read(s.Frame)).Where(r => r.Status == ReadStatus.Ok)
            .Select(r => r.Message).OfType<ReachResult>().Where(r => r.Id == id).ToList();
        return got is [{ } r] ? r.Reachable : null;
    }

    private static void Reach(Checks c)
    {
        var (w, link, clock) = Away();
        c.Check(S, Ask(w, link, clock, "r1", "Fay@alpha", "Ann@alpha") == true,
            "a character away is reachable by a teller of its own world it did not squelch as it left");
        c.Check(S, Ask(w, link, clock, "r2", "Hal@alpha", "Ann@alpha") == true, "by any, a teller off the network included");
        c.Check(S, Ask(w, link, clock, "r3", "Gus@alpha", "Ann@alpha") == false, "not by a name the world does not have");
        c.Check(S, Ask(w, link, clock, "r4", "Ann@alpha", "Ann@alpha") == false, "nor by itself");

        (w, link, clock) = Away(h => h.Characters[Ann].SquelchedLocals.Add(Fay));
        c.Check(S, Ask(w, link, clock, "s1", "Fay@alpha", "Ann@alpha") == false, "nor by a teller it squelched as it left");
        c.Check(S, Ask(w, link, clock, "s2", "Jo@alpha", "Ann@alpha") == true, "while another of its world is");
        (w, link, clock) = Away(h => h.Characters[Ann].SquelchedAccounts.Add("fayacct"));
        c.Check(S, Ask(w, link, clock, "s3", "Fay@alpha", "Ann@alpha") == false, "nor by a teller whose account it squelched");
        (w, link, clock) = Away(h => h.Characters[Ann].SquelchesEveryone = true);
        c.Check(S, Ask(w, link, clock, "s4", "Jo@alpha", "Ann@alpha") == false, "nor by anyone of its world, squelching everyone");
        c.Check(S, Ask(w, link, clock, "s5", "Bob@bravo", "Ann@alpha") == true,
            "which leaves another world's tellers to its squelches of them");
        (w, link, clock) = Away(keep: false);
        c.Check(S, Ask(w, link, clock, "k1", "Fay@alpha", "Ann@alpha") == false,
            "and a character that left keeping no squelches is reached by no teller of its world");
        c.Check(S, Ask(w, link, clock, "k2", "Bob@bravo", "Ann@alpha") == true, "while another world's still reach it");

        // The kept squelches outlive a restart: the store holds them.
        (w, link, clock) = Away(h => h.Characters[Ann].SquelchedLocals.Add(Fay));
        var again = NetworkStore.FromJson(w.World.StoreJson());
        c.Check(S, again.Characters[Ann].AwaySquelches is { } kept && kept.Covers(Fay, null) && !kept.Covers(Jo, "joacct"),
            "the squelches a character kept as it left are saved with its record");
    }

    private static void Replies(Checks c)
    {
        var (w, link, clock) = Away();
        var (mark, seen) = (link.Sent.Count, w.Host.To(Fay).Count);
        link.Frame(1, new Tell("h1", A("Ann@alpha"), A("Fay@alpha"), "back soon", false));
        clock.Now += 0.01;
        w.Pump();
        var answers = link.SentMessages().Skip(mark).OfType<TellResult>().ToList();
        c.Check(S, w.Host.To(Fay).Skip(seen).ToList() is [{ Kind: "tell", Name: "Ann@alpha", Text: "back soon" } t] && StandIns.IsStandIn(t.Id)
                && answers is [{ Id: "h1", Outcome: Outcome.Delivered }],
            "a reply from Discord, as a character of this world away, on the network and linked, is shown as a tell from its address");

        foreach (var (id, from, why) in new[]
                 {
                     ("h2", "Jo@alpha", "online"),
                     ("h3", "Ivy@alpha", "away, never linked"),
                     ("h4", "Hal@alpha", "off the network"),
                     ("h5", "Gus@alpha", "that the world does not have"),
                 })
        {
            var (shown, at) = (w.Host.To(Fay).Count, link.Sent.Count);
            link.Frame(1, new Tell(id, A(from), A("Fay@alpha"), "hi", false));
            clock.Now += 0.01;
            w.Pump();
            c.Check(S, w.Host.To(Fay).Count == shown
                    && link.SentMessages().Skip(at).OfType<TellResult>().ToList() is [{ Outcome: Outcome.Offline } r] && r.Id == id,
                $"a tell from this world's own address is refused, from a character {why}");
        }

        w.Host.Characters[Fay].SquelchedLocals.Add(Ann);
        var (before, sentAt) = (w.Host.To(Fay).Count, link.Sent.Count);
        link.Frame(1, new Tell("h6", A("Ann@alpha"), A("Fay@alpha"), "hello?", false));
        clock.Now += 0.01;
        w.Pump();
        c.Check(S, w.Host.To(Fay).Count == before
                && link.SentMessages().Skip(sentAt).OfType<TellResult>().ToList() is [{ Id: "h6", Outcome: Outcome.Squelched }],
            "and a reply to a teller who squelches the character is not shown, and answered squelched");
    }
}
