// Persons: the network's link between a player's characters and their Discord account.
//
// The hub keeps persons; the world asks on its own characters' behalf and shows what the hub
// answers:
//
// - Linking. `/dn link` asks the hub for a one-time code for the character; the player enters it on
//   the site, signed in with Discord. `/dn unlink` undoes it. A link holds only for the character
//   that asked for its code (`AskedLink`): a character made under the name of a deleted one is not
//   linked to the deleted one's person, and is unlinked.
// - Alts. `/dn alts hidden` hides the person's characters; the world never learns the person's other
//   characters while they are hidden. Only the person shows them, on the site: the hub refuses
//   `/dn alts shown` from a world, and names the page.
// - Friends. A linked character's friends are its person's, kept by the hub: adding one asks the hub
//   to list it (`befriend`), and the friends a character had before it was linked are carried over
//   to the hub when the view first says it is linked (`carry`). While a character is online and on
//   the network, the world watches its person view (`person.watch`), and lists every character the
//   view's person friends name as a remote friend of the character until the view stops naming it
//   or the character leaves. Such a friend is marked in the record (`FromPerson`, with the handle it
//   came by), so it is never confused with one kept in the record, and it is never written to the
//   store's file. A friend of its own the view lists as a handle becomes such a friend, so none is
//   listed twice. When a character is unlinked, the hub hands its friends back in its next view, and
//   they become its own again.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>What the hub's person view last said of one character online.</summary>
    public readonly record struct PersonState(bool Linked, Alts Alts);

    /// <summary>Why a person request was made, which decides what its answer shows.</summary>
    private enum Purpose
    {
        /// <summary>A `/dn` command, or adding a friend: its answer is shown.</summary>
        Command,
        /// <summary>A row the player removed, or friends carried over: nothing is shown.</summary>
        Quiet,
    }

    private sealed record PendingPerson(uint Player, PersonOp Op, Address? Target, bool? Shown, Purpose Purpose, double? Sent);

    private const string LinkFirstLine = "Link this character first: type /dn link.";

    /// <summary>What the hub's view last said of each character online, by character id.</summary>
    private readonly Dictionary<uint, PersonState> persons = new();

    private readonly Dictionary<string, PendingPerson> pendingPersons = new(StringComparer.Ordinal);

    /// <summary>What the hub's view last said of `character`, while it is online; null until it says.</summary>
    public PersonState? PersonStateOf(uint character)
    {
        lock (gate)
            return persons.TryGetValue(character, out var s) ? s : null;
    }

    /// <summary>The stand-in id of a remote identity, if it has one.</summary>
    public uint? StandInIdOf(Address who)
    {
        lock (gate)
            return store.StandIns.Get(who);
    }

    /// <summary>The keys of the friends `character`'s person friends listed.</summary>
    public IReadOnlyList<string> ListedByPersons(uint character)
    {
        lock (gate)
            return Record(character)?.FromPerson.Keys.ToList() ?? [];
    }

    /// <summary>Asks the hub for `player`'s person view, or stops: only while the link is up.</summary>
    private void WatchPerson(uint player, bool on, string? knownName = null)
    {
        if (link != LinkState.Up || OwnAddress(player, knownName) is not { } who)
            return;
        Send(new PersonWatch(who, on));
    }

    /// <summary>Every character online and on the network asks for its view again (after a welcome).</summary>
    private void WatchPersonsEveryone()
    {
        foreach (var player in host.OnlineCharacters())
            if (IsOn(player))
                WatchPerson(player, true);
    }

    /// <summary>
    /// A character left the world or the network: its view is no longer asked for, and the friends its
    /// person friends listed go; `clientStays` also takes their rows out of its friends panel.
    /// </summary>
    private void PersonLeft(uint player, string? name, bool clientStays)
    {
        WatchPerson(player, false, name);
        persons.Remove(player);
        foreach (var f in StripListed(player))
            if (clientStays && StandIn(f) is uint id)
                host.ShowFriendRow(player, FriendRowKind.Removed, id, f.ToString(), false);
    }

    /// <summary>Takes every friend a person friend listed out of `character`'s record: the friends taken.</summary>
    private List<Address> StripListed(uint character)
    {
        var r = Record(character);
        if (r is null || r.FromPerson.Count == 0)
            return [];
        var rec = RecordMut(character);
        var gone = new List<Address>();
        foreach (var k in rec.FromPerson.Keys.ToList())
        {
            if (rec.Friends.Remove(k, out var f))
                gone.Add(f);
            presence.Remove((character, k));
        }
        rec.FromPerson.Clear();
        dirty = true;
        return gone;
    }

    /// <summary>
    /// A person view from the hub, for one of this world's characters online. The characters its
    /// person friends name become the character's remote friends, except itself, this world's own
    /// characters, any it lists itself, and any past the friends a character may list; those it no
    /// longer names go.
    /// </summary>
    private void OnPersonView(Address to, bool linked, Alts alts, IReadOnlyList<PersonFriend> view)
    {
        if (to.World != Config.Slug || host.OnlineByName(to.Name) is not uint player || !IsOn(player))
            return;
        // A link holds only for the character that asked for its code: one that never asked carries a
        // name a deleted character was linked under. It is taken as unlinked, and the hub told so. A
        // view of a character nobody links names the friends handed back to it when it was unlinked:
        // they become its own, if the link was its own.
        var asked = Record(player)?.AskedLink == true;
        IReadOnlyList<PersonFriend> returned = [];
        if (linked && !asked)
        {
            PersonAsk(player, PersonOp.Unlink, null, null, Purpose.Quiet);
            linked = false;
            alts = Alts.Hidden;
            view = [];
        }
        else if (!linked)
        {
            alts = Alts.Hidden;
            returned = asked ? view : [];
            view = [];
        }
        var had = persons.TryGetValue(player, out var before);
        persons[player] = new PersonState(linked, alts);
        if (linked && had && !before.Linked)
            host.ShowSystem(player, "This character is now linked to a Discord account.");
        if (linked && (!had || !before.Linked))
            CarryFriends(player);
        if (OwnAddress(player) is not { } own)
            return;
        // A handed-back friend listed until now through a person friend stays, as the character's own.
        foreach (var p in returned)
            if (Record(player)?.FromPerson.ContainsKey(p.Handle.Key) == true)
            {
                RecordMut(player).FromPerson.Remove(p.Handle.Key);
                dirty = true;
            }
        // In the view's order, each character once, with the first handle that names it. A character
        // of this world (the character itself among them) is its own world's to list.
        var want = new Dictionary<string, (Address Character, Address Handle)>(StringComparer.Ordinal);
        var order = new List<string>();
        foreach (var p in view)
            foreach (var c in p.Characters)
            {
                if (c.IsChannel || c.World == Config.Slug || want.ContainsKey(c.Key))
                    continue;
                order.Add(c.Key);
                want[c.Key] = (c, p.Handle);
            }
        var derived = Record(player)?.FromPerson.ToDictionary(p => p.Key, p => p.Value, StringComparer.Ordinal)
            ?? new Dictionary<string, Address>(StringComparer.Ordinal);
        var gone = new List<Address>();
        foreach (var key in derived.Keys.Where(k => !want.ContainsKey(k)).ToList())
        {
            var rec = RecordMut(player);
            rec.FromPerson.Remove(key);
            if (rec.Friends.Remove(key, out var f))
            {
                presence.Remove((player, key));
                gone.Add(f);
            }
        }
        foreach (var f in gone)
            if (StandIn(f) is uint id)
                host.ShowFriendRow(player, FriendRowKind.Removed, id, f.ToString(), false);
        var added = new List<Address>();
        foreach (var key in order)
        {
            var (c, handle) = want[key];
            if (derived.ContainsKey(key))
            {
                RecordMut(player).FromPerson[key] = handle;
                continue;
            }
            var r = Record(player);
            // A friend of its own the view lists as a person friend's handle is the person's now: it
            // is listed once, and no longer kept in the record.
            if (handle.Key == key && r?.Friends.ContainsKey(key) == true)
            {
                RecordMut(player).FromPerson[key] = handle;
                dirty = true;
                continue;
            }
            if (r is not null && (r.Friends.ContainsKey(key) || r.Friends.Count >= MaxRemoteFriends))
                continue;
            // Another world's character: a new id for it is held to its world's allowance of new
            // names, as a tell's sender's is. One past it is listed when a later view names it.
            if (RemoteStandIn(c) is not uint id)
                continue;
            var rec = RecordMut(player);
            rec.Friends[key] = c;
            rec.FromPerson[key] = handle;
            host.ShowFriendRow(player, FriendRowKind.Added, id, c.ToString(), false);
            added.Add(c);
        }
        foreach (var p in returned)
        {
            var c = p.Handle;
            if (c.IsChannel || c.World == Config.Slug)
                continue;
            var r = Record(player);
            if (r is not null && (r.Friends.ContainsKey(c.Key) || r.Friends.Count >= MaxRemoteFriends))
                continue;
            if (RemoteStandIn(c) is not uint id)
                continue;
            RecordMut(player).Friends[c.Key] = c;
            host.ShowFriendRow(player, FriendRowKind.Added, id, c.ToString(), false);
            added.Add(c);
        }
        if (gone.Count > 0 || added.Count > 0)
            dirty = true;
        if (gone.Count > 0)
            FriendshipEnded(player, gone);
        if (added.Count > 0)
        {
            TellWatchers(player, null);
            foreach (var f in added)
                SendIfUp(new Watch(own, f, true));
        }
    }

    /// <summary>
    /// The view first says `player`'s character is linked: the friends it keeps in its record are carried
    /// over to its person, which then lists them (as many as a person may list; the rest stay its own).
    /// </summary>
    private void CarryFriends(uint player)
    {
        if (Record(player) is not { } r)
            return;
        var targets = r.Friends.Where(f => !r.FromPerson.ContainsKey(f.Key)).Select(f => f.Value)
            .Take(Wire.MaxCarried).ToList();
        if (targets.Count > 0)
            PersonAsk(player, PersonOp.Carry, null, null, Purpose.Quiet, targets);
    }

    /// <summary>The handle a friend of `character` came by, when a person friend listed it.</summary>
    private Address? HandleOf(uint character, string friendKey) =>
        Record(character)?.FromPerson.TryGetValue(friendKey, out var h) == true ? h : null;

    /// <summary>Sends a person request for `player`, within its pace; false when it was not sent.</summary>
    private bool PersonAsk(uint player, PersonOp op, Address? target, bool? shown, Purpose purpose,
        IReadOnlyList<Address>? targets = null)
    {
        if (purpose == Purpose.Command && !IsOn(player))
        {
            host.ShowSystem(player, "You are not on the Dereth Network. Type /dn on to join it.");
            return false;
        }
        // An `unfriend` only takes away, and one held back would leave a person friend the player has
        // already let go: it is not held to the character's pace.
        if (op != PersonOp.Unfriend && !MaySend(player, Sends.ChannelRequests))
        {
            if (purpose == Purpose.Command)
                host.ShowSystem(player, TooQuicklyLine);
            return false;
        }
        if (OwnAddress(player) is not { } who)
            return false;
        var id = FreshId('p');
        pendingPersons[id] = new PendingPerson(player, op, target, shown, purpose, null);
        Send(new PersonRequest(id, op, who, target, shown, targets));
        return true;
    }

    /// <summary>`/dn link`, `/dn unlink` and `/dn alts ...`.</summary>
    private void PersonCommand(uint player, IReadOnlyList<string> w)
    {
        Address? ParseCharacter(IEnumerable<string> words) =>
            Address.TryParse(string.Join(" ", words), out var a, out _) && !a.IsChannel ? a : null;
        switch (w[0])
        {
            case "link":
                // The character asked to be linked, so a link the hub then reports is its own.
                if (PersonAsk(player, PersonOp.Code, null, null, Purpose.Command) && Record(player)?.AskedLink != true)
                {
                    RecordMut(player).AskedLink = true;
                    dirty = true;
                }
                return;
            case "unlink":
                PersonAsk(player, PersonOp.Unlink, null, null, Purpose.Command);
                return;
            case "alts" when w.Count == 1:
                AltsStatus(player);
                return;
            case "alts" when w.Count == 2 && w[1].Equals("shown", StringComparison.OrdinalIgnoreCase):
                PersonAsk(player, PersonOp.Alts, null, true, Purpose.Command);
                return;
            case "alts" when w.Count == 2 && w[1].Equals("hidden", StringComparison.OrdinalIgnoreCase):
                PersonAsk(player, PersonOp.Alts, null, false, Purpose.Command);
                return;
            case "alts":
                if (ParseCharacter(w.Skip(1)) is { } of)
                    PersonAsk(player, PersonOp.Characters, of, null, Purpose.Command);
                else
                    host.ShowSystem(player, "/dn alts shown|hidden, or /dn alts Name@world");
                return;
        }
    }

    /// <summary>`/dn alts`: the setting the hub last gave, and, while shown, the person's characters.</summary>
    private void AltsStatus(uint player)
    {
        if (!IsOn(player))
        {
            host.ShowSystem(player, "You are not on the Dereth Network. Type /dn on to join it.");
            return;
        }
        if (!persons.TryGetValue(player, out var s))
            host.ShowSystem(player, "The Dereth Network has not said yet whether this character is linked.");
        else if (!s.Linked)
            host.ShowSystem(player, "This character is not linked to a Discord account. Type /dn link to link it.");
        else if (s.Alts == Alts.Hidden)
            host.ShowSystem(player, "Your characters are hidden: nobody learns that they are one person.");
        else
            PersonAsk(player, PersonOp.Characters, OwnAddress(player), null, Purpose.Command);
    }

    /// <summary>The player removed rows its person friends listed, or cleared its friends: those person friends go, quietly.</summary>
    private void UnfriendQuietly(uint player, IEnumerable<Address> handles)
    {
        foreach (var h in handles.DistinctBy(h => h.Key))
            PersonAsk(player, PersonOp.Unfriend, h, null, Purpose.Quiet);
    }

    /// <summary>The answer to a person request.</summary>
    private void OnPersonResult(PersonResult r)
    {
        if (!pendingPersons.Remove(r.Id, out var p) || p.Purpose == Purpose.Quiet || !Online(p.Player))
            return;
        var target = p.Target?.ToString() ?? "";
        string Joined(IEnumerable<Address> a) => string.Join(", ", a.Select(x => x.ToString()));
        string? line = (p.Op, r.Outcome) switch
        {
            (PersonOp.Code, PersonOutcome.Ok) when r.Code is { } code && r.Url is { } url =>
                $"Your link code is {code}. Within {((r.ExpiresSecs ?? 600) + 59) / 60} minutes, sign in with Discord at {url} and enter it there. It links this character to the account you sign in with; give it to nobody.",
            (PersonOp.Code, PersonOutcome.RateLimited) => "You have asked for too many link codes. Wait a while and try again.",
            (_, PersonOutcome.Unavailable) => "The Dereth Network's hub does not link characters.",
            (_, PersonOutcome.RateLimited) => "The Dereth Network is holding back your requests for now. Wait a while and try again.",
            (PersonOp.Code, _) => "The Dereth Network gave no link code.",
            (PersonOp.Unlink, PersonOutcome.Ok) => "This character is no longer linked to a Discord account.",
            (PersonOp.Unlink, _) => "This character is not linked to a Discord account.",
            (_, PersonOutcome.NotLinked) => LinkFirstLine,
            (PersonOp.Alts, PersonOutcome.Refused) => r.Url is { } page
                ? $"Only you can show your characters as one person: sign in with Discord at {page} and choose it there."
                : "Only you can show your characters as one person, signed in on the Dereth Network's site.",
            (PersonOp.Alts, _) when p.Shown == true =>
                "Your characters are shown as one person now: anyone may learn which characters are yours.",
            (PersonOp.Alts, _) => "Your characters are hidden now: nobody learns that they are one person.",
            (PersonOp.Befriend, PersonOutcome.Refused) => null,
            (PersonOp.Befriend, _) => $"{target} has been added to your friends list.",
            // Only ever asked quietly.
            (PersonOp.Unfriend or PersonOp.Carry, _) => null,
            _ => CharactersLine(p, r.Characters, target, Joined),
        };
        if (p.Op == PersonOp.Befriend && r.Outcome == PersonOutcome.Refused)
            host.ShowFriendsFull(p.Player);
        if (line is not null)
            host.ShowSystem(p.Player, TextRules.SanitizeLine(line));
    }

    private string? CharactersLine(PendingPerson p, IReadOnlyList<Address> characters, string target,
        Func<IEnumerable<Address>, string> joined)
    {
        var mine = p.Target is not null && OwnAddress(p.Player) is { } own && own.Key == p.Target.Key;
        var others = characters.Where(c => p.Target is null || c.Key != p.Target.Key).ToList();
        return (mine, others.Count == 0) switch
        {
            (true, _) => $"Your characters, shown as one person: {joined(characters)}.",
            (false, true) => $"{target} shows no other characters.",
            _ => $"{target} is one person with {joined(others)}.",
        };
    }

    /// <summary>A person request that will not be answered.</summary>
    private void GiveUpPerson(string id)
    {
        if (pendingPersons.Remove(id, out var p) && p.Purpose == Purpose.Command && Online(p.Player))
            host.ShowSystem(p.Player, "The Dereth Network is unavailable.");
    }

    /// <summary>The status line's word on linking.</summary>
    private string LinkedNote(uint player) =>
        persons.TryGetValue(player, out var s) && s.Linked ? " This character is linked to a Discord account." : "";
}
