// Persons on the world half, on a link in memory: `/dn link` asks for the character's own code and
// shows the link that carries it; a person view's characters are listed as friends, watched, and
// online as their person, and go when the view stops naming them, at logout and at `/dn off`;
// removing one removes its person friend; none is ever written to the store's file; a view is taken
// only for this world's own characters on the network; a view's characters new to this world take
// ids from their world's share of new names; a linked character's friend is asked of the hub, an
// unlinked one's kept by the world; linking carries the world's friends to the person, and unlinking
// hands them back.

namespace Dereth.Network.Harness;

internal static class PersonChecks
{
    private const string S = "units/persons";

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static Address A(string s) => Address.Parse(s) ?? throw new ArgumentException(s);

    private static PersonFriends View(string to, params (string Handle, string[] Characters)[] persons) =>
        new(A(to), true, persons.Select(p => new PersonFriend(A(p.Handle), p.Characters.Select(A).ToList())).ToList());

    /// <summary>
    /// A world `alpha` linked and welcomed, with Ann on the network, and, unless `asked` is false,
    /// having asked for a link code, so a link the hub reports is hers.
    /// </summary>
    private static (TestWorld W, MemoryLink Link, uint Ann, Clock Clock) Ready(bool asked = true)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now);
        var link = (MemoryLink)w.Link;
        var ann = w.Host.Add(0x5000_0001, "Ann").Id;
        link.Up(1);
        w.Pump();
        link.Frame(1, new Welcome(Wire.Protocol, "alpha", "hub test",
            new[] { "alpha", "bravo" }.Select(s => new WorldInfo(s, s.ToUpperInvariant(), true)).ToList(),
            [Wire.GlobalRoom], Limits.Default));
        w.Pump();
        w.World.Dn(ann, ["on"]);
        clock.Now += 1;
        w.Pump();
        if (asked)
        {
            var mark = link.Sent.Count;
            w.World.Dn(ann, ["link"]);
            clock.Now += 0.5;
            w.Pump();
            var code = link.SentMessages().Skip(mark).OfType<PersonRequest>().First(p => p.Op == PersonOp.Code);
            link.Frame(1, new PersonResult(code.Id, PersonOutcome.Ok, "K7QD-2MXP", 600, "https://example.org/link/#K7QD-2MXP", []));
            clock.Now += 0.5;
            w.Pump();
        }
        return (w, link, ann, clock);
    }

    private static void Give(TestWorld w, MemoryLink link, Clock clock, WireMessage m)
    {
        link.Frame(1, m);
        clock.Now += 0.5;
        w.Pump();
    }

    private static List<string> Rows(TestWorld w, uint who, FriendRowKind kind, int from) =>
        w.Host.To(who).Skip(from).Where(x => x.Kind == "friend" && x.Row == kind).Select(x => x.Name).ToList();

    private static List<WireMessage> SentSince(MemoryLink link, int mark) => link.SentMessages().Skip(mark).ToList();

    /// <summary>The friends a store's text holds for one character.</summary>
    private static List<string> FriendsIn(string storeJson, uint character)
    {
        using var d = System.Text.Json.JsonDocument.Parse(storeJson);
        return d.RootElement.GetProperty("characters").TryGetProperty(character.ToString(System.Globalization.CultureInfo.InvariantCulture), out var r)
            ? r.GetProperty("friends").EnumerateObject().Select(p => p.Name).ToList()
            : [];
    }

    public static void Run(Checks c)
    {
        LinkCode(c);
        LinkOnlyForTheCharacterThatAsked(c);
        ListedWatchedAndGone(c);
        MutualAndOnlineAsItsPerson(c);
        RemovedWithItsPersonFriend(c);
        NeverSavedAndOnlyOwnCharacters(c);
        AltsAndBefriend(c);
        CarriedOnLinkingAndHandedBackOnUnlinking(c);
        ViewsHeldToTheWorldsShareOfNewNames(c);
    }

    /// <summary>A view's characters new to this world take ids from their world's share of new names, as a tell's sender does.</summary>
    private static void ViewsHeldToTheWorldsShareOfNewNames(Checks c)
    {
        var (w, link, ann, clock) = Ready();
        // Five person friends of 20 characters each: a full view of 100 characters, all new.
        PersonFriends Full(int from) => new(A("Ann@alpha"), true,
            Enumerable.Range(0, 5).Select(p =>
            {
                var cs = Enumerable.Range(0, 20).Select(i => A(GuardChecks.Minted(from + p * 20 + i) + "@bravo")).ToList();
                return new PersonFriend(cs[0], cs);
            }).ToList());
        int Listed(int from)
        {
            var shown = w.Host.To(ann).Count;
            Give(w, link, clock, Full(from));
            return Rows(w, ann, FriendRowKind.Added, shown).Count;
        }
        c.Check(S, (int)NetworkWorld.NewStandInsPerWorld.Burst == 200, "bravo's share is 200 new names at once");
        c.Check(S, Listed(0) == 100, "a full view's characters are listed");
        c.Check(S, Listed(100) == 100, "and a second's, to bravo's share");
        c.Check(S, Listed(200) == 0 && w.World.ListedByPersons(ann).Count == 0,
            "past it, a view's characters new to this world are not listed");
        c.Check(S, Listed(0) == 100, "those that already have an id are listed still");
    }

    private static void LinkCode(Checks c)
    {
        var (w, link, ann, clock) = Ready(asked: false);
        var mark = link.Sent.Count;
        w.World.Dn(ann, ["link"]);
        clock.Now += 0.5;
        w.Pump();
        var asked = SentSince(link, mark).OfType<PersonRequest>().FirstOrDefault();
        c.Check(S, asked is { Op: PersonOp.Code } && asked.Who.ToString() == "Ann@alpha", "/dn link asks for the character's own code");
        var before = w.Host.To(ann).Count;
        Give(w, link, clock, new PersonResult(asked?.Id ?? "", PersonOutcome.Ok, "K7QD-2MXP", 600, "https://example.org/link/#K7QD-2MXP", []));
        c.Check(S, w.Host.To(ann).Skip(before).Select(x => x.Text).SequenceEqual(
                ["Open https://example.org/link/#K7QD-2MXP within 10 minutes to link this character to your Discord account. Keep the link to yourself."]),
            "the link that carries the code is shown, one line: " + w.Host.To(ann).Last().Text);
        c.Check(S, NetworkWorld.LinkLine("K7QD-2MXP", "https://example.org/characters/", 60)
                == "Your link code is K7QD-2MXP. Enter it at https://example.org/characters/ within 1 minute to link this character to your Discord account. Keep the code to yourself.",
            "a hub whose page does not carry the code is answered with the code to enter there");
        Give(w, link, clock, new PersonFriends(A("Ann@alpha"), false, []));
        Give(w, link, clock, new PersonFriends(A("Ann@alpha"), true, []));
        c.Check(S, w.Host.To(ann).Any(x => x.Text == "This character is now linked to a Discord account."),
            "a link made while the character is online is told");
        w.World.Dn(ann, []);
        c.Check(S, w.Host.To(ann).Last().Text.EndsWith("This character is linked to a Discord account.", StringComparison.Ordinal),
            "/dn says the character is linked");
        var shownBefore = w.Host.To(ann).Count;
        w.World.Dn(ann, ["off"]);
        c.Check(S, w.Host.To(ann).Skip(shownBefore).Select(x => x.Text).SequenceEqual([
                "You are off the Dereth Network. Nobody on another world can see you or tell you, and your profile is out of its directory.",
                NetworkWorld.PausedLine,
            ]),
            "/dn off says a linked character's link is paused until /dn on");
        mark = link.Sent.Count;
        w.World.Dn(ann, ["link"]);
        clock.Now += 0.5;
        w.Pump();
        c.Check(S, !SentSince(link, mark).OfType<PersonRequest>().Any()
            && w.Host.To(ann).Last().Text == "You are not on the Dereth Network. Type @dn on to join it.",
            "off the network, nothing is asked");
    }

    private static void LinkOnlyForTheCharacterThatAsked(Checks c)
    {
        var (w, link, ann, clock) = Ready(asked: false);
        var shown = w.Host.To(ann).Count;
        var mark = link.Sent.Count;
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo", "Alt@bravo"])));
        c.Check(S, Rows(w, ann, FriendRowKind.Added, shown).Count == 0 && w.World.ListedByPersons(ann).Count == 0,
            "a link the character never asked for lists nothing");
        c.Check(S, w.World.PersonStateOf(ann) is { Linked: false },
            "and the character is taken as unlinked");
        c.Check(S, SentSince(link, mark).OfType<PersonRequest>().Any(p => p is { Op: PersonOp.Unlink } && p.Who.ToString() == "Ann@alpha"),
            "and the hub is told to unlink it");
        w.World.Dn(ann, ["link"]);
        clock.Now += 0.5;
        w.Pump();
        c.Check(S, w.World.StoreJson().Contains("\"asked_link\": true", StringComparison.Ordinal),
            "asking for a code is kept in the store");
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo"])));
        c.Check(S, w.World.ListedByPersons(ann).SequenceEqual(["bob@bravo"]), "once it asked, the link is its own");
        var again = NetworkStore.FromJson(w.World.StoreJson());
        c.Check(S, again.Characters.TryGetValue(ann, out var r) && r.AskedLink, "and read back from the store's file");
    }

    private static void ListedWatchedAndGone(Checks c)
    {
        var (w, link, ann, clock) = Ready();
        var shown = w.Host.To(ann).Count;
        var mark = link.Sent.Count;
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo", "Alt@bravo", "Ann@alpha", "Local@alpha"])));
        c.Check(S, Rows(w, ann, FriendRowKind.Added, shown).SequenceEqual(["Bob@bravo", "Alt@bravo"]),
            "a view's characters are rows, in its order, but for the character itself and this world's own");
        var watched = SentSince(link, mark).OfType<Watch>().Where(x => x.On).Select(x => x.Target.ToString()).ToList();
        c.Check(S, watched.Contains("Bob@bravo") && watched.Contains("Alt@bravo"), "each is watched");
        c.Check(S, w.World.ListedByPersons(ann).SequenceEqual(["alt@bravo", "bob@bravo"]), "each is marked as a person friend's");
        shown = w.Host.To(ann).Count;
        mark = link.Sent.Count;
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo"])));
        c.Check(S, Rows(w, ann, FriendRowKind.Removed, shown).SequenceEqual(["Alt@bravo"])
            && SentSince(link, mark).OfType<Watch>().Any(x => !x.On && x.Target.ToString() == "Alt@bravo"),
            "a character the view no longer names leaves the panel and is no longer watched");
        w.Host.Characters[ann].Online = false;
        w.World.OnLogout(ann, "Ann");
        c.Check(S, w.World.ListedByPersons(ann).Count == 0 && FriendsIn(w.World.StoreJson(), ann).Count == 0, "gone at logout");
        w.Host.Characters[ann].Online = true;
        w.World.OnLogin(ann);
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo", "Alt@bravo"])));
        shown = w.Host.To(ann).Count;
        w.World.Dn(ann, ["off"]);
        c.Check(S, Rows(w, ann, FriendRowKind.Removed, shown).SequenceEqual(["Alt@bravo", "Bob@bravo"])
            && w.World.ListedByPersons(ann).Count == 0, "/dn off takes their rows out of the panel");
    }

    private static void MutualAndOnlineAsItsPerson(Checks c)
    {
        var (w, link, ann, clock) = Ready();
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo", "Alt@bravo"])));
        var mark = link.Sent.Count;
        Give(w, link, clock, new Watch(A("Alt@bravo"), A("Ann@alpha"), true));
        c.Check(S, SentSince(link, mark).OfType<Presence>().Any(p => p.Online && p.To.ToString() == "Alt@bravo"),
            "a character a person friend lists watches it, and its watch is answered online");
        Give(w, link, clock, new Presence(A("Ann@alpha"), A("Alt@bravo"), false));
        Give(w, link, clock, new Presence(A("Ann@alpha"), A("Alt@bravo"), true));
        var line = w.Host.To(ann).LastOrDefault(x => x.Kind == "system")?.Text ?? "";
        c.Check(S, line.StartsWith("Bob@bravo is online as <Tell:IIDString:", StringComparison.Ordinal)
            && line.EndsWith(":Alt@bravo>Alt@bravo<\\Tell>.", StringComparison.Ordinal), "it comes online as its person: " + line);
    }

    private static void RemovedWithItsPersonFriend(Checks c)
    {
        var (w, link, ann, clock) = Ready();
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo", "Alt@bravo"])));
        var mark = link.Sent.Count;
        var altId = w.World.StandInIdOf(A("Alt@bravo"));
        c.Check(S, altId is uint id && w.World.RemoveFriend(ann, id), "a listed row is removed by its id");
        clock.Now += 0.5;
        w.Pump();
        c.Check(S, SentSince(link, mark).OfType<PersonRequest>().Any(p => p is { Op: PersonOp.Unfriend } && p.Target?.ToString() == "Bob@bravo"),
            "removing it removes its person friend, by its handle");
        // A character listed already is not asked for again.
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo", "Alt@bravo"])));
        mark = link.Sent.Count;
        w.World.AddFriend(ann, "Alt@bravo");
        c.Check(S, w.Host.To(ann).Last().Text == "That character is already in your friends list"
            && !SentSince(link, mark).OfType<PersonRequest>().Any(),
            "@friends add of a listed character says it is listed already");
        // Clearing the friends list removes every person friend, past the character's pace.
        var many = Enumerable.Range(0, 14)
            .Select(i => ($"P{(char)('a' + i)}@bravo", new[] { $"P{(char)('a' + i)}@bravo" })).ToArray();
        Give(w, link, clock, View("Ann@alpha", many));
        mark = link.Sent.Count;
        w.World.RemoveAllFriends(ann);
        clock.Now += 0.5;
        w.Pump();
        c.Check(S, SentSince(link, mark).OfType<PersonRequest>().Count(p => p.Op == PersonOp.Unfriend) == many.Length,
            "clearing the friends list removes every person friend, however many");
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo", "Alt@bravo"])));
        mark = link.Sent.Count;
        w.World.Dn(ann, ["friends", "remove", "Bob@bravo"]);
        clock.Now += 0.5;
        w.Pump();
        c.Check(S, SentSince(link, mark).OfType<PersonRequest>().Any(p => p is { Op: PersonOp.Unfriend } && p.Target?.ToString() == "Bob@bravo"),
            "/dn friends remove Name@world removes the person friend");
    }

    private static void NeverSavedAndOnlyOwnCharacters(Checks c)
    {
        var (w, link, ann, clock) = Ready();
        Give(w, link, clock, View("Bob@bravo", ("Eve@bravo", ["Eve@bravo"])));
        Give(w, link, clock, View("Ann@bravo", ("Eve@bravo", ["Eve@bravo"])));
        c.Check(S, w.World.ListedByPersons(ann).Count == 0, "a view for another world's character is not taken");
        Give(w, link, clock, View("Ann@alpha", ("Eve@bravo", ["Eve@bravo"])));
        c.Check(S, w.World.ListedByPersons(ann).Count == 1 && w.World.FriendRows(ann).Count == 1, "its own character's is");
        w.World.SaveNow();
        c.Check(S, w.Saved is not null && FriendsIn(w.Saved, ann).Count == 0 && FriendsIn(w.World.StoreJson(), ann).Count == 0,
            "what a person friend listed is never written to the store's file");
        w.World.Dn(ann, ["off"]);
        Give(w, link, clock, View("Ann@alpha", ("Eve@bravo", ["Eve@bravo"])));
        c.Check(S, w.World.ListedByPersons(ann).Count == 0, "nor one for a character off the network");
    }

    private static void AltsAndBefriend(Checks c)
    {
        var (w, link, ann, clock) = Ready();
        var mark = link.Sent.Count;
        foreach (var words in new[] { new[] { "alts" }, ["alts", "shown"], ["alts", "Bob@bravo"] })
        {
            w.World.Dn(ann, words);
            c.Check(S, w.Host.To(ann).Last().Text == NetworkWorld.Help(), $"/dn {string.Join(" ", words)} is no command now, and shows the help");
        }
        clock.Now += 0.5;
        w.Pump();
        c.Check(S, !SentSince(link, mark).OfType<PersonRequest>().Any(), "and asks the hub nothing");
        Give(w, link, clock, new PersonFriends(A("Ann@alpha"), true, []));
        // A linked character's friend is its person's: the hub is asked to list it.
        mark = link.Sent.Count;
        var rows = w.Host.To(ann).Count;
        w.World.AddFriend(ann, "Bob@bravo");
        clock.Now += 0.5;
        w.Pump();
        var ask = SentSince(link, mark).OfType<PersonRequest>().FirstOrDefault();
        c.Check(S, ask is { Op: PersonOp.Befriend } && ask.Target?.ToString() == "Bob@bravo"
            && Rows(w, ann, FriendRowKind.Added, rows).Count == 0 && w.World.FriendRows(ann).Count == 0,
            "a linked character's @friends add asks the hub to list the friend, and keeps nothing itself");
        Give(w, link, clock, new PersonResult(ask?.Id ?? "", PersonOutcome.Ok, null, null, null, []));
        c.Check(S, w.Host.To(ann).Last().Text == "Bob@bravo has been added to your friends list.",
            "and says it was added: " + w.Host.To(ann).Last().Text);
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo"])));
        c.Check(S, Rows(w, ann, FriendRowKind.Added, rows).SequenceEqual(["Bob@bravo"]), "its row comes with the view");
        mark = link.Sent.Count;
        w.World.AddFriend(ann, "Cat@bravo");
        clock.Now += 0.5;
        w.Pump();
        ask = SentSince(link, mark).OfType<PersonRequest>().FirstOrDefault();
        Give(w, link, clock, new PersonResult(ask?.Id ?? "", PersonOutcome.Refused, null, null, null, []));
        c.Check(S, w.Host.To(ann).Last().Text == NetworkWorld.FriendsFullLine
            && !w.Host.To(ann).Any(s => s.Kind == "friends-full"),
            "past the hub's limit on a person's friends, a plain line says so: " + w.Host.To(ann).Last().Text);
        mark = link.Sent.Count;
        w.World.AddFriend(ann, "Dan@bravo");
        clock.Now += 0.5;
        w.Pump();
        ask = SentSince(link, mark).OfType<PersonRequest>().FirstOrDefault();
        Give(w, link, clock, new PersonResult(ask?.Id ?? "", PersonOutcome.NotLinked, null, null, null, []));
        c.Check(S, w.Host.To(ann).Last().Text == "Link this character first: type @dn link.", "not linked at the hub: link first");
    }

    private static void CarriedOnLinkingAndHandedBackOnUnlinking(Checks c)
    {
        var (w, link, ann, clock) = Ready();
        var mark = link.Sent.Count;
        w.World.AddFriend(ann, "Bob@bravo");
        w.World.AddFriend(ann, "Zed@bravo");
        clock.Now += 0.5;
        w.Pump();
        c.Check(S, !SentSince(link, mark).OfType<PersonRequest>().Any()
            && FriendsIn(w.World.StoreJson(), ann).SequenceEqual(["bob@bravo", "zed@bravo"]),
            "an unlinked character's friends are kept by the world, and nothing is asked of the hub");
        mark = link.Sent.Count;
        var shown = w.Host.To(ann).Count;
        Give(w, link, clock, new PersonFriends(A("Ann@alpha"), true, []));
        var carry = SentSince(link, mark).OfType<PersonRequest>().FirstOrDefault(p => p.Op == PersonOp.Carry);
        c.Check(S, carry is not null && carry.TargetList.Select(t => t.ToString()).SequenceEqual(["Bob@bravo", "Zed@bravo"]),
            "linking carries them to the person");
        Give(w, link, clock, View("Ann@alpha", ("Bob@bravo", ["Bob@bravo"]), ("Zed@bravo", ["Zed@bravo"])));
        c.Check(S, Rows(w, ann, FriendRowKind.Added, shown).Count == 0 && Rows(w, ann, FriendRowKind.Removed, shown).Count == 0
            && w.World.FriendRows(ann).Count == 2,
            "listed by the view, each stays one row");
        c.Check(S, w.World.ListedByPersons(ann).SequenceEqual(["bob@bravo", "zed@bravo"])
            && FriendsIn(w.World.StoreJson(), ann).Count == 0,
            "they are the person's now, and the world keeps none");
        Give(w, link, clock, new PersonFriends(A("Ann@alpha"), false,
            [new PersonFriend(A("Bob@bravo"), [A("Bob@bravo")]), new PersonFriend(A("Zed@bravo"), [A("Zed@bravo")])]));
        Give(w, link, clock, new PersonFriends(A("Ann@alpha"), false, []));
        c.Check(S, Rows(w, ann, FriendRowKind.Removed, shown).Count == 0 && w.World.ListedByPersons(ann).Count == 0
            && FriendsIn(w.World.StoreJson(), ann).SequenceEqual(["bob@bravo", "zed@bravo"]),
            "unlinking hands them back: the world keeps them again, and no row goes");
    }
}
