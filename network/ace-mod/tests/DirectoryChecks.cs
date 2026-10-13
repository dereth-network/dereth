// The directory on the world half, on a link in memory, with a clock the test turns: the mod's own
// checks of what the conformance suite's directory checks hold a world to (nothing sent while the
// character is off the network; `/dn on` sends the character's own profile, with the lines; a field
// its option hides is never sent; changes at most every five minutes and at once at logout; `/dn
// off` takes it out at once, kept until answered, across a restart too; `listing.off` holds the
// character out until its next login; only listed patrons and monarchs named, an allegiance under a
// key of the world's own with its name only when its monarch named it; refusals), and a character
// renamed or deleted taken out under the name it had.

namespace Dereth.Network.Harness;

internal static class DirectoryChecks
{
    private const string S = "units/directory";
    private const uint Ann = 0x5000_0001;
    private const uint Bob = 0x5000_0002;

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private sealed class W(TestWorld world, MemoryLink link, Clock clock)
    {
        public readonly TestWorld World = world;
        public readonly MemoryLink Link = link;
        public readonly Clock Clock = clock;
        private int mark;

        /// <summary>The link's current connection.</summary>
        public long Epoch = 1;

        public NetworkWorld N => World.World;
        public FakeHost Host => World.Host;

        /// <summary>Lets `secs` pass, a second at a time.</summary>
        public void Advance(double secs)
        {
            var step = Math.Min(1.0, secs);
            for (var t = 0.0; t < secs; t += step)
            {
                Clock.Now += step;
                World.Pump();
            }
        }

        /// <summary>The directory's messages sent since the last call.</summary>
        public List<DirectoryMessage> Taken()
        {
            World.Pump();
            var got = Link.SentListings(mark);
            mark = Link.Sent.Count;
            return got;
        }

        public List<Listing> Listings() => Taken().OfType<Listing>().ToList();

        public void Give(DirectoryMessage m)
        {
            Link.FrameText(Epoch, DirectoryWire.Write(m));
            Advance(0.5);
        }

        public void Dn(uint who, params string[] args)
        {
            N.Dn(who, args);
            Advance(0.5);
        }

        /// <summary>The system lines shown to `who`, from the shown entry `from` on.</summary>
        public List<string> Lines(uint who, int from = 0) =>
            Host.To(who).Skip(from).Where(x => x.Kind == "system").Select(x => x.Text).ToList();

        /// <summary>The link goes, comes back as connection `epoch`, and is welcomed.</summary>
        public void Relink(long epoch)
        {
            Link.Down(Epoch);
            Link.Up(epoch);
            Epoch = epoch;
            Advance(0.5);
        }

        public void Welcome(long epoch)
        {
            Link.Frame(epoch, new Welcome(Wire.Protocol, "alpha", "hub test",
                new[] { "alpha", "bravo" }.Select(s => new WorldInfo(s, s.ToUpperInvariant(), true)).ToList(),
                [Wire.GlobalRoom], Limits.Default));
            Advance(0.5);
        }

        public void LogOut(uint who)
        {
            var c = Host.Characters[who];
            var facts = Host.FactsOf(who);
            c.Online = false;
            N.OnLogout(who, c.Name, facts);
            Advance(0.5);
        }

        public void LogIn(uint who)
        {
            Host.Characters[who].Online = true;
            N.OnLogin(who);
            Advance(0.5);
        }
    }

    /// <summary>
    /// A world `alpha` linked and welcomed, with Ann and Bob off the network, and, given a store, from
    /// it; what it sent on the way is taken unless `keep` is set.
    /// </summary>
    private static W Ready(NetworkStore? store = null, bool keep = false)
    {
        var clock = new Clock();
        var tw = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now, store: store);
        var w = new W(tw, (MemoryLink)tw.Link, clock);
        w.Host.Add(Ann, "Ann");
        w.Host.Add(Bob, "Bob");
        w.Link.Up(1);
        w.World.Pump();
        w.Welcome(1);
        if (!keep)
            w.Taken();
        return w;
    }

    private static ListingResult Answer(string id, ListingOutcome outcome = ListingOutcome.Ok, Refusal? reason = null) =>
        new(id, outcome, reason, outcome == ListingOutcome.Ok ? "https://example.org/directory/character/" : null);

    /// <summary>`who` goes on the network: the one listing it sends.</summary>
    private static Listing? List(W w, uint who)
    {
        w.Dn(who, "on");
        var sent = w.Listings();
        return sent.Count == 1 && sent[0].On ? sent[0] : null;
    }

    /// <summary>`who` goes on the network, and the hub lists it.</summary>
    private static Listing? Listed(W w, uint who)
    {
        var l = List(w, who);
        if (l is not null)
            w.Give(Answer(l.Id));
        w.Taken();
        return l;
    }

    private static readonly Seeable Values = new(1_083_412_800, 3_600_000, 1_400, 10, 12, 37);

    public static void Run(Checks c)
    {
        NothingWhileOffTheNetwork(c);
        OnSendsTheOwnProfile(c);
        AFieldItsOptionHidesIsNeverSent(c);
        ChangesAtMostEveryFiveMinutesAndAtOnceAtLogout(c);
        TakingOutAtOnceKeptUntilAnswered(c);
        AnOffFromTheHubHoldsTheCharacterOutUntilItsNextLogin(c);
        AnOffByItsPersonTakesTheCharacterOffTheNetwork(c);
        OnlyListedPatronsAndMonarchsNamed(c);
        RefusalsHoldTheCharacterOutUntilItsNextLogin(c);
        RenamedOrDeletedTakenOut(c);
        AnAdminStaysListed(c);
    }

    private static void NothingWhileOffTheNetwork(Checks c)
    {
        var w = Ready();
        w.Host.Characters[Ann].Level = 20;
        w.Advance(400);
        w.LogOut(Ann);
        w.LogIn(Ann);
        c.Check(S, w.Taken().Count == 0, "nothing is sent for a character off the network, at a level gained, logout or login");
    }

    private static void OnSendsTheOwnProfile(Checks c)
    {
        var w = Ready();
        w.Host.Characters[Ann].Level = 42;
        var from = w.Host.To(Ann).Count;
        var l = List(w, Ann);
        c.Check(S, l is { Who: var who, Profile: { Level: 42, Heritage: 1, Gender: 2, Title: "the Bloodletter" } } && who.ToString() == "Ann@alpha"
            && l.Profile!.Check(who) is null, $"/dn on sends Ann's own profile ({(l is null ? "none" : DirectoryWire.Write(l))})");
        c.Check(S, w.Lines(Ann, from).Take(2).SequenceEqual([
                "You are on the Dereth Network as Ann@alpha. Anyone on the network can see when you are online, send you tells and find your profile in its directory. Type @dn help for the commands.",
                "Your profile shows your level, skills, allegiance and more, and changes as you play. Type @dn off to leave the network.",
            ]), "and says what it publishes: " + string.Join(" | ", w.Lines(Ann, from)));
        w.Give(Answer(l?.Id ?? ""));
        c.Check(S, w.Lines(Ann, from).Contains("Your directory page: https://example.org/directory/character/"), "the hub's answer shows the page");
        from = w.Host.To(Ann).Count;
        w.Dn(Ann);
        c.Check(S, w.Lines(Ann, from).SequenceEqual([
                "You are on the Dereth Network as Ann@alpha. The hub is connected. You have no friends on other worlds.",
                "Your directory page: https://example.org/directory/character/",
            ]), "/dn shows the character on the network and its page: " + string.Join(" | ", w.Lines(Ann, from)));
    }

    private static void AFieldItsOptionHidesIsNeverSent(Checks c)
    {
        var w = Ready();
        var first = new OthersSee(DateOfBirth: true, Age: false, ChessRank: true, FishingSkill: false, NumberOfDeaths: true, NumberOfTitles: false);
        var second = new OthersSee(DateOfBirth: false, Age: true, ChessRank: false, FishingSkill: true, NumberOfDeaths: false, NumberOfTitles: true);
        w.Host.Characters[Ann].Values = Values;
        w.Host.Characters[Ann].Allowed = first;
        var l = Listed(w, Ann);
        c.Check(S, l?.Profile?.Seeable == new Seeable(Values.Birth, null, Values.Chess, null, Values.Deaths, null),
            $"with half its options on, only their fields go ({l?.Profile?.Seeable})");
        w.Host.Characters[Ann].Allowed = second;
        w.Advance(400);
        var sent = w.Listings();
        c.Check(S, sent is [{ On: true, Profile: { } p }] && p.Seeable == new Seeable(null, Values.Age, null, Values.Fishing, null, Values.Titles),
            "turning the options over is a change, and sends the other half alone");
    }

    private static void ChangesAtMostEveryFiveMinutesAndAtOnceAtLogout(Checks c)
    {
        var w = Ready();
        Listed(w, Ann);
        w.Host.Characters[Ann].Level = 11;
        w.Advance(60);
        w.Host.Characters[Ann].Level = 12;
        w.Advance(120);
        c.Check(S, w.Taken().Count == 0, "a change within five minutes of the last listing waits");
        w.Advance(200);
        var sent = w.Listings();
        c.Check(S, sent is [{ On: true, Profile.Level: 12 }], "after five minutes only the latest profile goes");
        if (sent.Count > 0)
            w.Give(Answer(sent[0].Id));
        w.Host.Characters[Ann].Level = 13;
        w.Advance(1);
        c.Check(S, w.Taken().Count == 0, "a change just after a listing waits");
        w.LogOut(Ann);
        c.Check(S, w.Listings() is [{ On: true, Profile.Level: 13 }], "logout sends the latest profile at once");
        // The age alone, which grows as the character plays, is no change of its own.
        w.LogIn(Ann);
        var login = w.Listings();
        if (login.Count > 0)
            w.Give(Answer(login[0].Id));
        w.Host.Characters[Ann].Allowed = new OthersSee(false, true, false, false, false, false);
        w.Host.Characters[Ann].Values = Values;
        w.Advance(400);
        var shown = w.Listings();
        c.Check(S, shown is [{ On: true, Profile.Age: 3_600_000 }], "turning on the age's option is a change of its own");
        foreach (var x in shown)
            w.Give(Answer(x.Id));
        for (var i = 0; i < 3; i++)
        {
            w.Host.Characters[Ann].Values = w.Host.Characters[Ann].Values with { Age = w.Host.Characters[Ann].Values.Age + 400 };
            w.Advance(400);
        }
        c.Check(S, w.Taken().Count == 0, "the age growing alone sends nothing while playing");
        w.LogOut(Ann);
        c.Check(S, w.Listings() is [{ On: true, Profile.Age: > 3_600_000 }], "and goes at logout");
    }

    private static void TakingOutAtOnceKeptUntilAnswered(Checks c)
    {
        var w = Ready();
        Listed(w, Ann);
        Listed(w, Bob);
        w.Dn(Ann, "off");
        w.Dn(Bob, "off");
        bool Out(List<Listing> sent, string who) => sent.Any(l => !l.On && l.Who.ToString() == who && l.Profile is null);
        var sent = w.Listings();
        c.Check(S, Out(sent, "Ann@alpha") && Out(sent, "Bob@alpha"), "/dn off takes the character out at once");
        c.Check(S, w.N.RemovalsOwed == 2 && w.N.StoreJson().Contains("\"unlistings\"", StringComparison.Ordinal),
            "and are owed, in the store, until answered");
        // The link goes before the hub answers: both go again on the next link, after its welcome.
        w.Relink(2);
        c.Check(S, w.Taken().Count == 0, "nothing of the directory goes before the welcome");
        w.Welcome(2);
        sent = w.Listings();
        c.Check(S, Out(sent, "Ann@alpha") && Out(sent, "Bob@alpha"), "and both go again after it");
        // A restart: the store as saved brings them again.
        var saved = NetworkStore.FromJson(w.N.StoreJson());
        var again = Ready(saved, keep: true);
        c.Check(S, again.N.RemovalsOwed == 2 && Out(again.Listings(), "Ann@alpha"), "a restart keeps what is owed, and sends it after the welcome");
        foreach (var l in sent)
            w.Give(Answer(l.Id));
        c.Check(S, w.N.RemovalsOwed == 0, "an answer clears what was owed");
        w.Relink(3);
        w.Welcome(3);
        c.Check(S, w.Taken().Count == 0, "and nothing goes again on the next link");
    }

    private static void AnOffFromTheHubHoldsTheCharacterOutUntilItsNextLogin(Checks c)
    {
        var w = Ready();
        Listed(w, Ann);
        Listed(w, Bob);
        var from = w.Host.To(Ann).Count;
        w.Give(new ListingOff(Address.Parse("Ann@alpha")!, OffBy.Operator));
        c.Check(S, w.Listings().Any(l => !l.On && l.Who.ToString() == "Ann@alpha"), "listing.off is confirmed with a listing that is not on");
        c.Check(S, w.Lines(Ann, from).SequenceEqual([NetworkWorld.TakenDownLine]) && w.N.SettingsOf(Ann) == CharacterSettings.OnNetwork,
            "the player is told at once, and stays on the network");
        w.Host.Characters[Ann].Level = 30;
        w.Advance(400);
        c.Check(S, w.Taken().Count == 0 && w.Lines(Ann, from).Count == 1, "nothing is listed after it, and nothing more said");
        w.LogOut(Ann);
        w.LogIn(Ann);
        c.Check(S, w.Listings().Any(l => l.On && l.Who.ToString() == "Ann@alpha"), "its next login asks to list it again");
        w.LogOut(Bob);
        w.Taken();
        w.Give(new ListingOff(Address.Parse("Bob@alpha")!, OffBy.Operator));
        c.Check(S, w.Listings().Any(l => !l.On && l.Who.ToString() == "Bob@alpha"), "one away is confirmed off as well");
        from = w.Host.To(Bob).Count;
        w.LogIn(Bob);
        c.Check(S, w.Lines(Bob, from).Contains(NetworkWorld.TakenDownLine), "and told at its next login");
    }

    private static void AnOffByItsPersonTakesTheCharacterOffTheNetwork(Checks c)
    {
        var w = Ready();
        Listed(w, Ann);
        Listed(w, Bob);
        var from = w.Host.To(Ann).Count;
        w.Give(new ListingOff(Address.Parse("Ann@alpha")!, OffBy.Person));
        c.Check(S, w.Listings().Any(l => !l.On && l.Who.ToString() == "Ann@alpha"),
            "an off by its person is confirmed with a listing that is not on");
        c.Check(S, w.Lines(Ann, from).SequenceEqual([NetworkWorld.ForgottenLine]) && w.N.SettingsOf(Ann) == CharacterSettings.Off,
            "the player is told at once, and is off the network");
        w.Host.Characters[Ann].Level = 30;
        w.Advance(400);
        w.LogOut(Ann);
        w.LogIn(Ann);
        c.Check(S, w.Taken().Count == 0, "nothing is listed after it, its next login included");
        w.LogOut(Bob);
        w.Taken();
        w.Give(new ListingOff(Address.Parse("Bob@alpha")!, OffBy.Person));
        c.Check(S, w.Listings().Any(l => !l.On && l.Who.ToString() == "Bob@alpha") && w.N.SettingsOf(Bob) == CharacterSettings.Off,
            "one away is confirmed off, and is off the network");
        from = w.Host.To(Bob).Count;
        w.LogIn(Bob);
        c.Check(S, w.Lines(Bob, from).Contains(NetworkWorld.ForgottenLine) && w.Taken().Count == 0,
            "and told at its next login, off the network still");
    }

    private static void OnlyListedPatronsAndMonarchsNamed(Checks c)
    {
        var w = Ready();
        w.Host.Characters[Ann].Patron = Bob;
        var l = Listed(w, Ann);
        var al = l?.Profile?.Allegiance;
        c.Check(S, al is { Patron: null, Monarch: null, Name: null, Members: 2 }, $"an unlisted patron and monarch are not named, and no name is made up ({al})");
        c.Check(S, al?.Key is { Length: 32 } key && key.All(char.IsAsciiHexDigitLower) && !key.Contains(Bob.ToString("x8"), StringComparison.Ordinal)
            && !key.Contains(Bob.ToString(System.Globalization.CultureInfo.InvariantCulture), StringComparison.Ordinal),
            "the allegiance goes under a random key of the world's own");
        Listed(w, Bob);
        w.Advance(400);
        var sent = w.Listings();
        var named = sent.FirstOrDefault(x => x.Who.ToString() == "Ann@alpha" && x.On)?.Profile?.Allegiance;
        c.Check(S, named?.Patron?.ToString() == "Bob@alpha" && named.Monarch?.ToString() == "Bob@alpha" && named.Key == al?.Key,
            "once the patron goes on the network, its vassal's listing goes again naming it, under the same key");
        foreach (var x in sent)
            w.Give(Answer(x.Id));
        w.Host.Characters[Bob].AllegianceName = "Order of the Dawn";
        w.Advance(400);
        sent = w.Listings();
        c.Check(S, sent.Any(x => x.Who.ToString() == "Ann@alpha" && x.Profile?.Allegiance?.Name == "Order of the Dawn"),
            "the name its monarch gives it goes");
        foreach (var x in sent)
            w.Give(Answer(x.Id));
        w.Dn(Bob, "off");
        w.Advance(400);
        sent = w.Listings();
        c.Check(S, sent.Any(x => x.Who.ToString() == "Ann@alpha" && x.On && x.Profile?.Allegiance is { Patron: null, Monarch: null }),
            "once it leaves the network, its vassal's listing names it no more");
    }

    private static void RefusalsHoldTheCharacterOutUntilItsNextLogin(Checks c)
    {
        var w = Ready();
        var l = List(w, Ann);
        var from = w.Host.To(Ann).Count;
        w.Give(Answer(l?.Id ?? "", ListingOutcome.Refused, Refusal.TakenDown));
        c.Check(S, w.Lines(Ann, from).SequenceEqual([NetworkWorld.TakenDownLine]) && w.N.SettingsOf(Ann) == CharacterSettings.OnNetwork,
            "taken_down answers /dn on with its line, and the character stays on the network");
        l = List(w, Bob);
        from = w.Host.To(Bob).Count;
        w.Give(Answer(l?.Id ?? "", ListingOutcome.Refused, Refusal.Full));
        c.Check(S, w.Lines(Bob, from).SequenceEqual([NetworkWorld.FullLine]), "full answers /dn on with its line");
        w.Host.Characters[Ann].Level = 30;
        w.Advance(400);
        c.Check(S, w.Taken().Count == 0, "nothing is listed after a refusal");
        w.LogOut(Ann);
        var atLogout = w.Taken();
        w.LogIn(Ann);
        c.Check(S, atLogout.Count == 0 && w.Listings() is [{ On: true, Profile.Level: 30 }], "until its next login, which asks again");
        var r = Ready();
        l = List(r, Ann);
        r.Give(Answer(l?.Id ?? "", ListingOutcome.RateLimited));
        r.Host.Characters[Ann].Level = 31;
        r.Advance(30);
        c.Check(S, r.Taken().Count == 0, "rate_limited waits a minute");
        r.Advance(40);
        c.Check(S, r.Listings() is [{ On: true, Profile.Level: 31 }], "then sends the latest profile again");
        var u = Ready();
        l = List(u, Ann);
        from = u.Host.To(Ann).Count;
        u.Give(Answer(l?.Id ?? "", ListingOutcome.Unavailable));
        c.Check(S, u.Lines(Ann, from).SequenceEqual([NetworkWorld.NoDirectoryLine]) && u.N.SettingsOf(Ann) == CharacterSettings.OnNetwork,
            "a hub that keeps no directory says so, and the character stays on the network");
    }

    private static void RenamedOrDeletedTakenOut(Checks c)
    {
        var w = Ready();
        Listed(w, Ann);
        Listed(w, Bob);
        w.LogOut(Ann);
        w.Taken();
        w.Host.Characters[Ann].Name = "Annie";
        w.Advance(31);
        var sent = w.Listings();
        c.Check(S, sent.Any(x => !x.On && x.Who.ToString() == "Ann@alpha") && !sent.Any(x => x.On),
            "a character renamed is taken out under the name it had");
        w.LogIn(Ann);
        c.Check(S, w.Listings() is [{ On: true } again] && again.Who.ToString() == "Annie@alpha",
            "and listed under its new name when it is next in the world");
        w.N.OnDeleted(Bob);
        w.Advance(0.5);
        c.Check(S, w.Listings().Any(x => !x.On && x.Who.ToString() == "Bob@alpha"), "a character deleted is taken out at once");
    }

    private static void AnAdminStaysListed(Checks c)
    {
        var w = Ready();
        // ACE names an admin character with a leading +, which its address leaves off.
        w.Host.Characters[Ann].Name = "+Ann";
        var l = List(w, Ann);
        c.Check(S, l is { Who: var who } && who.ToString() == "Ann@alpha", "an admin character is listed under its name without the +");
        w.Give(Answer(l?.Id ?? ""));
        w.Taken();
        w.Advance(NetworkWorld.LookEverySecs + 1);
        w.Advance(NetworkWorld.LookEverySecs + 1);
        c.Check(S, !w.Listings().Any(x => !x.On), "and is not taken out as renamed when the world looks for changes");
    }
}
