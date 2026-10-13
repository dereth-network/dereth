// The network's directory, the world's half (docs/network/03-world-behaviour.md section 10).
//
// A character on the network has its profile in the network's directory. The world composes the
// profile, sends it to the hub when it changes, and takes it out when the player takes the
// character off the network, or the hub's operator takes it out:
//
// - The profile is the character's own, as the host gives it (CharacterFacts): level, heritage,
//   gender, title, experience, skill credits, luminance, attributes, vitals, skills and
//   augmentations, all without the effects of spells, and its allegiance. The six fields the
//   character's "Allow others to see your ..." options govern go only while those options allow,
//   the test appraising it makes.
// - The allegiance goes under a key the world keeps for each monarch, a random value derived from
//   nothing seen in game; with its name only when the monarch gave it one, never the monarch's name
//   in its place; and with its patron and monarch only while each is listed too.
// - When. At `/dn on` and at each login, at once; a change while the character plays at most once
//   every five minutes, the latest profile then (its age, which grows every second it plays, goes
//   with its next other change); at logout at once, if it changed. The listings of each character
//   and of the world together are paced under the hub's paces for listings and the link's own.
//   When a character comes onto the network or leaves it, the listings of its listed vassals (and,
//   for a monarch, of every listed member) go again, so that their links to it appear or go.
// - Held out. A character the hub's operator took out, or one the directory has no room for, is
//   sent no more until its next login, when the world asks again; the hub refuses it while it holds
//   it out. The player is told once: when the operator takes it out (at once, or at its next
//   login), or when `/dn on` is refused.
// - What is kept. Each listed character's latest profile, whether the hub has answered it, and
//   every listing that takes a character out until the hub answers it, are saved with the store,
//   so that a lost link, a restart or a crash never leaves a character in the directory its player
//   took off the network. They go again after the next welcome, the ones that take characters out
//   first.

using System.Security.Cryptography;

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>
    /// How often the profiles of the listed characters online are looked over for a change, and the
    /// listed characters for a rename or a deletion, in seconds.
    /// </summary>
    public const double LookEverySecs = 30;

    /// <summary>How long a listing the hub held back to its pace waits before it goes again, in seconds.</summary>
    public const double AgainSecs = 60;

    /// <summary>
    /// The integer properties the game keeps the augmentations in, whose numbers a profile's
    /// augmentations carry: the attribute, skill, resistance, combat and magic augmentations, and the
    /// ones bought with luminance.
    /// </summary>
    public static readonly IReadOnlyList<int> Augmentations =
    [
        .. Enumerable.Range(218, 21), .. Enumerable.Range(240, 7), .. Enumerable.Range(293, 10), 309, 310, 326, 327, 328,
        .. Enumerable.Range(333, 13), 365,
    ];

    /// <summary>The line for a character the hub's operator took out of the directory, or refused to list.</summary>
    public const string TakenDownLine = "The Dereth Network's operator has taken this character out of the directory for now.";

    /// <summary>The line for a character the directory has no room for.</summary>
    public const string FullLine = "The Dereth Network's directory holds as many of this world's characters as it may.";

    /// <summary>The line for a character its player took off the network by asking the Dereth Network to forget them.</summary>
    public const string ForgottenLine =
        "You asked the Dereth Network to forget you, so this character is off the network now. Type @dn on to join it again.";

    /// <summary>The line for a listing on a hub that keeps no directory.</summary>
    public const string NoDirectoryLine = "This network's hub keeps no directory, so your profile is not in one.";

    private static string LineOf(TakenOut why) => why switch
    {
        TakenOut.Full => FullLine,
        TakenOut.Forgotten => ForgottenLine,
        _ => TakenDownLine,
    };

    /// <summary>When a listed character's profile goes next, and whether the hub's answer is shown (it answers `/dn on`).</summary>
    private sealed record DueListing(double At, bool Announce);

    /// <summary>A listing on the link: the character it lists (none for one that takes a character out), its address's key.</summary>
    private sealed record SentListing(uint? Character, string Key, bool Announce);

    private readonly SortedDictionary<uint, DueListing> dueListings = new();
    private readonly Dictionary<string, SentListing> sentListings = new(StringComparer.Ordinal);

    /// <summary>The id of each character's latest listing.</summary>
    private readonly Dictionary<uint, string> latestListing = new();

    /// <summary>Listings that take a character out, on the link and not yet answered, by identity key.</summary>
    private readonly HashSet<string> removing = new(StringComparer.Ordinal);

    /// <summary>Those the hub held back to its pace, and when they may go again.</summary>
    private readonly Dictionary<string, double> removalsWait = new(StringComparer.Ordinal);

    /// <summary>The characters the hub holds out of the directory, sent no listing until their next login.</summary>
    private readonly HashSet<uint> heldOut = new();

    /// <summary>When each character's last listing went.</summary>
    private readonly Dictionary<uint, double> lastListed = new();

    /// <summary>Each character's listings, paced as the hub paces them.</summary>
    private readonly Dictionary<uint, Bucket> listingPaces = new();

    /// <summary>The world's listings, paced as the hub paces them.</summary>
    private Bucket? listingPace;

    private double nextLook;

    // ---- what the host and tests may read ------------------------------------------------------

    /// <summary>How many listings that take a character out the world owes the hub.</summary>
    public int RemovalsOwed
    {
        get
        {
            lock (gate)
                return store.Unlistings.Count;
        }
    }

    /// <summary>A character's listing, as the world keeps it; null while it is not listed.</summary>
    public ListingRecord? ListingOf(uint character)
    {
        lock (gate)
            return Record(character)?.Listing;
    }

    // ---- helpers ---------------------------------------------------------------------------------

    /// <summary>A tenth under the hub's own `rate`, so the two clocks never disagree at the edge.</summary>
    private static Rate Under(Rate rate) => new(Math.Max(1.0, rate.Burst * 0.9), rate.PerSec * 0.9);

    /// <summary>Whether `character` is listed: on the network, and not held out by the hub.</summary>
    private bool IsListed(uint character) => !heldOut.Contains(character) && IsOn(character);

    /// <summary>`character`'s address on this world, in the world or not.</summary>
    private Address? AddressOf(uint character) => host.NameOf(character) is { } n ? Address.Create(n, Config.Slug) : null;

    /// <summary>
    /// The key the directory knows `monarch`'s allegiance by: a random value the world keeps while the
    /// monarch is the same, made the first time it is needed.
    /// </summary>
    private string AllegianceKey(uint monarch)
    {
        if (store.AllegianceKeys.TryGetValue(monarch, out var k))
            return k;
        var key = Convert.ToHexString(RandomNumberGenerator.GetBytes(16)).ToLowerInvariant();
        store.AllegianceKeys[monarch] = key;
        dirty = true;
        return key;
    }

    /// <summary>
    /// `character`'s allegiance as the directory shows it: its key, its name when its monarch gave it
    /// one, its size, the character's rank, and its patron and monarch while each is listed.
    /// </summary>
    private ProfileAllegiance? AllegianceFrom(uint character, AllegianceFacts? facts)
    {
        if (facts is not { } a)
            return null;
        Address? Named(uint? other) => other is uint o && o != character && IsListed(o) ? AddressOf(o) : null;
        var name = a.Name is { } n && DirectoryWire.IsShownText(n, DirectoryWire.MaxAllegianceNameChars) ? n : null;
        var al = new ProfileAllegiance(AllegianceKey(a.Monarch), name, a.Members, a.Rank, Named(a.Patron), Named(a.Monarch));
        return al.Members is >= 1 and <= DirectoryWire.MaxMembers && al.Rank is >= 1 and <= DirectoryWire.MaxRank ? al : null;
    }

    /// <summary>The profile the host's facts of `character` make.</summary>
    private Profile? Compose(uint character, CharacterFacts? facts) =>
        facts is null ? null : facts.Own.WithSeeable(facts.Values, facts.Allowed) with { Allegiance = AllegianceFrom(character, facts.Allegiance) };

    /// <summary>
    /// A profile kept for a character not in the world, with its allegiance as it is now: from the
    /// allegiance the world has loaded, or failing that the kept one, naming a patron or monarch only
    /// while it is still listed.
    /// </summary>
    private Profile Refreshed(uint character, Profile kept)
    {
        if (host.AllegianceOf(character) is { } a)
            return kept with { Allegiance = AllegianceFrom(character, a) };
        if (kept.Allegiance is not { } al)
            return kept;
        Address? Still(Address? x) =>
            x is not null && x.World == Config.Slug && host.FindCharacter(x.Name) is { } f && IsListed(f.Id) ? x : null;
        return kept with { Allegiance = al with { Patron = Still(al.Patron), Monarch = Still(al.Monarch) } };
    }

    /// <summary>
    /// Whether two profiles of a character in the world differ in more than its age's value, which
    /// grows every second it plays: the age alone goes with its next other change, or at its logout
    /// (its option turned on or off is a change of its own).
    /// </summary>
    private static bool DiffersWhilePlaying(string kept, Profile now)
    {
        static string Ageless(Profile p) => (p with { Age = p.Age is null ? null : 0 }).Json();
        return DirectoryWire.ReadProfile(kept) is not { } k || Ageless(k) != Ageless(now);
    }

    /// <summary>Owes the hub a listing that takes `who` out of the directory, saved until the hub answers it.</summary>
    private void OweRemoval(Address who)
    {
        store.Unlistings[who.Key] = who;
        removalsWait.Remove(who.Key);
        dirty = true;
    }

    /// <summary>Forgets `character`'s listing: nothing of it is due or awaited.</summary>
    private void ForgetListing(uint character)
    {
        if (Record(character) is not null)
            RecordMut(character).Listing = null;
        dirty = true;
        dueListings.Remove(character);
        latestListing.Remove(character);
    }

    /// <summary>
    /// The hub holds `character` out of the directory: its listing is forgotten, and none is sent until
    /// its next login. Its followers' links to it go.
    /// </summary>
    private void HoldOut(uint character)
    {
        var was = IsListed(character);
        ForgetListing(character);
        heldOut.Add(character);
        if (was)
            LinksChanged(character);
    }

    /// <summary>Shows `character` why it was taken out of the directory: at once while it is in the world, else at its next login.</summary>
    private void TellTakenOut(uint character, TakenOut why)
    {
        if (Online(character))
        {
            host.ShowSystem(character, LineOf(why));
            return;
        }
        if (Record(character) is not null)
        {
            RecordMut(character).TakenOut = why;
            dirty = true;
        }
    }

    /// <summary>Marks `character`'s profile due at `at`, unless it is due sooner.</summary>
    private void DueAt(uint character, double at, bool announce)
    {
        if (dueListings.TryGetValue(character, out var d))
            dueListings[character] = new DueListing(Math.Min(d.At, at), d.Announce || announce);
        else
            dueListings[character] = new DueListing(at, announce);
    }

    /// <summary>
    /// `character` came onto the directory or left it: the listings of its listed vassals, and of
    /// every listed member when it is a monarch, go again, each within its own pace, so their links to
    /// it appear or go.
    /// </summary>
    private void LinksChanged(uint character)
    {
        var t = Now;
        foreach (var m in host.Followers(character))
            if (m != character && IsListed(m))
                DueAt(m, lastListed.TryGetValue(m, out var l) ? Math.Max(t, l + DirectoryWire.ListingEverySecs) : t, false);
    }

    /// <summary>
    /// The profile to list `character` with now, and the address it goes under: composed for a
    /// character in the world, the kept one for one away. Null for a character that is not listed or
    /// cannot be.
    /// </summary>
    private (Address Who, Profile Profile)? ListingFor(uint character)
    {
        if (!IsListed(character))
            return null;
        var inWorld = Online(character);
        if ((inWorld ? OwnAddress(character) : AddressOf(character)) is not { } who)
            return null;
        var kept = Record(character)?.Listing;
        // A character renamed since it was listed is taken out under the name it had, and listed under
        // its new one.
        if (kept is not null && kept.Who.Name != who.Name)
        {
            ForgetListing(character);
            OweRemoval(kept.Who);
            LinksChanged(character);
            kept = null;
        }
        var profile = inWorld
            ? Compose(character, host.Facts(character))
            : kept is not null && DirectoryWire.ReadProfile(kept.Profile) is { } k ? Refreshed(character, k) : null;
        if (profile is null)
            return null;
        if (profile.Check(who) is { } why)
        {
            log(LogLevel.Warn, $"a profile out of the directory's bounds ({why}) is not listed");
            return null;
        }
        return (who, profile);
    }

    /// <summary>
    /// The next listing for the link at `t`, if one may go: what takes a character out first, then the
    /// profiles due, each within its character's pace.
    /// </summary>
    private string? NextListing(double t)
    {
        foreach (var (key, who) in store.Unlistings)
        {
            if (removing.Contains(key) || (removalsWait.TryGetValue(key, out var at) && at > t))
                continue;
            var id = FreshId('l');
            removing.Add(key);
            removalsWait.Remove(key);
            sentListings[id] = new SentListing(null, key, false);
            return DirectoryWire.Write(new Listing(id, who, false, null));
        }
        var rate = Under(DirectoryWire.ListingsPerCharacter);
        foreach (var (c, d) in dueListings.ToList())
        {
            if (d.At > t)
                continue;
            if (!listingPaces.TryGetValue(c, out var pace))
            {
                pace = new Bucket(rate, t);
                listingPaces[c] = pace;
            }
            if (!pace.Has(t))
                continue;
            dueListings.Remove(c);
            if (ListingFor(c) is not var (who, profile))
                continue;
            var id = FreshId('l');
            var r = RecordMut(c);
            r.Listing = new ListingRecord
            {
                Who = who,
                Profile = profile.Json(),
                Owed = true,
                Page = r.Listing is { } l && l.Who.Same(who) ? l.Page : null,
            };
            dirty = true;
            pace.Take(t);
            lastListed[c] = t;
            latestListing[c] = id;
            sentListings[id] = new SentListing(c, who.Key, d.Announce);
            return DirectoryWire.Write(new Listing(id, who, true, profile));
        }
        return null;
    }

    /// <summary>
    /// The listings to send now, after everything else the link carries, as far as the link's pace
    /// (`pace`) and the hub's pace for this world's listings allow.
    /// </summary>
    private void TakeListings(double t, Bucket pace, List<string> frames)
    {
        listingPace ??= new Bucket(Under(DirectoryWire.ListingsPerWorld), t);
        while (pace.Has(t) && listingPace.Has(t) && NextListing(t) is { } frame)
        {
            pace.Take(t);
            listingPace.Take(t);
            frames.Add(frame);
        }
    }

    /// <summary>
    /// Every <see cref="LookEverySecs"/>: a listed character renamed or deleted without the world's
    /// hearing of it is taken out, and each listed character in the world whose last listing is five
    /// minutes old and whose profile changed is due.
    /// </summary>
    private void LookForChanges(double t)
    {
        TakeOutDeleted();
        if (t < nextLook)
            return;
        nextLook = t + LookEverySecs;
        foreach (var k in listingPaces.Where(p => p.Value.FullAt(t)).Select(p => p.Key).ToList())
            listingPaces.Remove(k);
        foreach (var (c, l) in store.Characters.Where(x => x.Value.Listing is not null).Select(x => (x.Key, x.Value.Listing!)).ToList())
        {
            // Its name as an address, as it was listed: without the + ACE gives an admin's name.
            if (AddressOf(c)?.Name == l.Who.Name)
                continue;
            ForgetListing(c);
            OweRemoval(l.Who);
            LinksChanged(c);
        }
        foreach (var c in host.OnlineCharacters())
        {
            if (!IsListed(c) || dueListings.ContainsKey(c)
                || (lastListed.TryGetValue(c, out var last) && t - last < DirectoryWire.ListingEverySecs))
                continue;
            if (OwnAddress(c) is not { } who || Compose(c, host.Facts(c)) is not { } p)
                continue;
            var kept = Record(c)?.Listing;
            if (kept is null || !kept.Who.Same(who) || DiffersWhilePlaying(kept.Profile, p))
                DueAt(c, t, false);
        }
    }

    // ---- what the world's characters do ----------------------------------------------------------

    /// <summary>
    /// `/dn on` for `player`: its listing goes at once, and the hub's answer is shown (its page, or why
    /// it is not listed).
    /// </summary>
    private void DirectoryAnnounce(uint player)
    {
        // A listing supersedes one still owed that takes the character out.
        if (OwnAddress(player) is { } me)
            store.Unlistings.Remove(me.Key);
        heldOut.Remove(player);
        DueAt(player, Now, true);
    }

    /// <summary>
    /// A character came into the world: it is told why it was taken out of the directory, or off the
    /// network, while it was away, on the network or off it now.
    /// </summary>
    private void ToldAtLogin(uint player)
    {
        if (Record(player)?.TakenOut is { } why)
        {
            RecordMut(player).TakenOut = null;
            dirty = true;
            host.ShowSystem(player, LineOf(why));
        }
    }

    /// <summary>
    /// A character on the network came into the world, or onto the network: it is listed at once (the
    /// hub refuses it while it holds it out).
    /// </summary>
    private void DirectoryArrived(uint player)
    {
        heldOut.Remove(player);
        if (IsListed(player))
            DueAt(player, Now, false);
    }

    /// <summary>
    /// A listed character left the world, as `name`, with the facts the host had of it: its profile
    /// goes at once if it changed since its last listing. One whose name changed since it was listed
    /// was renamed, and is taken out under the name it had and listed under its new one.
    /// </summary>
    private void DirectoryLeft(uint player, string name, CharacterFacts? facts)
    {
        if (!IsListed(player) || Address.Create(name, Config.Slug) is not { } who)
            return;
        var kept = Record(player)?.Listing;
        if (kept is not null && kept.Who.Name != who.Name)
        {
            ForgetListing(player);
            OweRemoval(kept.Who);
            LinksChanged(player);
            kept = null;
        }
        if (Compose(player, facts) is not { } profile)
            return;
        var json = profile.Json();
        if (kept is not null && kept.Profile == json)
            return;
        RecordMut(player).Listing = new ListingRecord { Who = who, Profile = json, Owed = true, Page = kept?.Page };
        dirty = true;
        DueAt(player, Now, false);
    }

    /// <summary>`player` went off the network (`/dn off`), its record already saying so: it is taken out of the directory at once.</summary>
    private void DirectoryWentOff(uint player)
    {
        heldOut.Remove(player);
        var who = Record(player)?.Listing?.Who ?? OwnAddress(player);
        RecordMut(player).Listing = null;
        dueListings.Remove(player);
        if (who is not null)
            OweRemoval(who);
        LinksChanged(player);
    }

    private readonly System.Collections.Concurrent.ConcurrentQueue<uint> deletedCharacters = new();

    /// <summary>
    /// A character of this world was deleted (or is waiting to be), from any thread: on the world
    /// thread's next turn, a listed one is taken out of the directory.
    /// </summary>
    public void OnDeleted(uint character) => deletedCharacters.Enqueue(character);

    /// <summary>The deletions heard since the last turn.</summary>
    private void TakeOutDeleted()
    {
        while (deletedCharacters.TryDequeue(out var character))
        {
            if (!IsListed(character))
                continue;
            var who = Record(character)?.Listing?.Who ?? AddressOf(character);
            ForgetListing(character);
            if (who is not null)
                OweRemoval(who);
            LinksChanged(character);
        }
    }

    // ---- the link -----------------------------------------------------------------------------------

    /// <summary>The link is welcomed: every listed character whose latest profile the hub has not answered is due again.</summary>
    private void DirectoryWelcomed()
    {
        var t = Now;
        foreach (var (c, r) in store.Characters)
            if (r.Listing is { Owed: true } && IsListed(c))
                DueAt(c, t, false);
    }

    /// <summary>The link is lost: what was on it and not yet answered is owed again, on the next link.</summary>
    private void DirectoryLinkDown()
    {
        sentListings.Clear();
        latestListing.Clear();
        removing.Clear();
        removalsWait.Clear();
        listingPace = null;
    }

    private void HandleDirectoryMessage(DirectoryMessage m)
    {
        switch (m)
        {
            case ListingResult r:
                OnListingResult(r);
                break;
            case ListingOff off:
                OnListingOff(off.Who, off.By);
                break;
        }
    }

    /// <summary>How a listing ended.</summary>
    private void OnListingResult(ListingResult r)
    {
        var t = Now;
        if (!sentListings.Remove(r.Id, out var sent))
            return;
        if (sent.Character is not uint character)
        {
            removing.Remove(sent.Key);
            if (r.Outcome == ListingOutcome.RateLimited)
                removalsWait[sent.Key] = t + AgainSecs;
            else
            {
                store.Unlistings.Remove(sent.Key);
                dirty = true;
            }
            return;
        }
        // An earlier listing of the character: the latest one's answer is the one that counts.
        if (!latestListing.TryGetValue(character, out var latest) || latest != r.Id)
            return;
        latestListing.Remove(character);
        var online = Online(character);
        void Answered(string? page)
        {
            if (Record(character)?.Listing is { } l)
                RecordMut(character).Listing = l with { Owed = false, Page = page ?? l.Page };
            dirty = true;
        }
        switch (r.Outcome)
        {
            case ListingOutcome.Ok:
                if (sent.Announce && online && r.Url is not null)
                    host.ShowSystem(character, $"Your directory page: {r.Url}");
                Answered(r.Url);
                break;
            case ListingOutcome.RateLimited:
                DueAt(character, t + AgainSecs, sent.Announce);
                break;
            case ListingOutcome.Unavailable:
                Answered(null);
                if (sent.Announce && online)
                    host.ShowSystem(character, NoDirectoryLine);
                break;
            default:
                // Held out by the hub: none is sent until the character's next login. Only the `/dn on`
                // that asked is told why; the hub's `listing.off` tells the rest.
                HoldOut(character);
                if (sent.Announce && online)
                    host.ShowSystem(character, r.Reason == Refusal.Full ? FullLine : TakenDownLine);
                break;
        }
    }

    /// <summary>
    /// `who` was taken out of the directory: a listing that is not on says the world has heard, and the
    /// player is told, at once or at its next login. Taken out by the hub's operator, none of its
    /// listings is sent until its next login, and it stays on the network. Taken out by its person,
    /// forgetting itself on the site, it goes off the network, as `/dn off` takes it.
    /// </summary>
    private void OnListingOff(Address who, OffBy by)
    {
        if (who.World != Config.Slug)
            return;
        OweRemoval(who);
        if (host.FindCharacter(who.Name) is not { } found || !IsOn(found.Id))
            return;
        if (by == OffBy.Operator)
        {
            HoldOut(found.Id);
            TellTakenOut(found.Id, TakenOut.Operator);
            return;
        }
        if (Online(found.Id))
            GoOff(found.Id);
        else
            TakenOff(found.Id);
        TellTakenOut(found.Id, TakenOut.Forgotten);
    }

    /// <summary>
    /// A character away goes off the network: its setting says so, and its listing is forgotten, the
    /// listing that takes it out owed already. Its followers' links to it go.
    /// </summary>
    private void TakenOff(uint character)
    {
        var was = IsListed(character);
        RecordMut(character).Settings = CharacterSettings.Off;
        ForgetListing(character);
        if (was)
            LinksChanged(character);
    }
}
