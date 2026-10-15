// Remote friends, their online status, and remote squelches.
//
// A remote friend is kept in the character's network record (the world's own friend list holds
// local characters only) and shown in the friends panel as a row under its stand-in id. Its status
// is its home world's to give: this world asks with a `watch` while the character is online, the
// home world answers with `presence` (online while the friend is on the network and online, to
// anyone its operator does not block), and each change is a status row and an online or offline
// line. A watch from another world for one of this world's characters is
// answered the same way, from this side.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>
    /// The rows a character's full friends list gains: each remote friend, under its stand-in id,
    /// with the status its home world last gave (offline until it gives one).
    /// </summary>
    public List<(uint Id, string Name, bool Online)> FriendRows(uint player)
    {
        lock (gate)
        {
            var rows = new List<(uint, string, bool)>();
            var r = Record(player);
            if (r is null)
                return rows;
            foreach (var f in r.Friends.Values.ToList())
                if (StandIn(f) is uint id)
                    rows.Add((id, f.ToString(), presence.TryGetValue((player, f.Key), out var st) && st.Online));
            return rows;
        }
    }

    /// <summary>
    /// `@friends add &lt;address&gt;`, which carries the name only. True when the name was an address
    /// the network took, or a local name given with the mark a line said from Discord shows (which
    /// goes to the world's own friend list without it); false leaves it to the world's own friend
    /// list. A linked character's friend is its person's, listed by the hub; any other character's is
    /// kept in its record.
    /// </summary>
    public bool AddFriend(uint player, string name)
    {
        lock (gate)
        {
            var given = name;
            name = SayWire.WithoutMark(name);
            if (!Address.LooksRemote(name))
            {
                if (name == given)
                    return false;
                host.LocalAddFriend(player, name);
                return true;
            }
            if (!Address.TryParse(name, out var friend, out _))
            {
                host.ShowSystem(player, "That character does not exist");
                return true;
            }
            if (friend.World == Config.Slug)
            {
                // A local character by its address: the local list, by its name.
                host.LocalAddFriend(player, friend.Name);
                return true;
            }
            if (friend.IsChannel)
            {
                host.ShowSystem(player, "A channel is not a character; it cannot be a friend.");
                return true;
            }
            if (!IsOn(player))
            {
                host.ShowSystem(player, NotOnLine);
                return true;
            }
            if (Record(player)?.Friends.ContainsKey(friend.Key) == true)
            {
                host.ShowSystem(player, "That character is already in your friends list");
                return true;
            }
            if (Record(player)?.Friends.Count >= MaxRemoteFriends)
            {
                host.ShowFriendsFull(player);
                return true;
            }
            // A linked character's friend is its person's: the hub lists it, and its row comes with the
            // person view.
            if (persons.TryGetValue(player, out var person) && person.Linked)
            {
                PersonAsk(player, PersonOp.Befriend, friend, Purpose.Command);
                return true;
            }
            if (StandIn(friend) is not uint id)
            {
                host.ShowSystem(player, "This world cannot show another character from the Dereth Network.");
                return true;
            }
            RecordMut(player).Friends[friend.Key] = friend;
            dirty = true;
            host.ShowFriendRow(player, FriendRowKind.Added, id, friend.ToString(), false);
            host.ShowSystem(player, $"{friend} has been added to your friends list.");
            if (OwnAddress(player) is { } owner)
                SendIfUp(new Watch(owner, friend, true));
            return true;
        }
    }

    /// <summary>A friend removed by a stand-in id. True when the id was a remote friend's.</summary>
    public bool RemoveFriend(uint player, uint id)
    {
        lock (gate)
        {
            if (!StandIns.IsStandIn(id))
                return false;
            var who = UseStandIn(id);
            if (who is null)
                return false;
            var r = Record(player);
            if (r is null || !RecordMut(player).Friends.Remove(who.Key, out var friend))
            {
                host.ShowSystem(player, "That character is not in your friends list!");
                return true;
            }
            dirty = true;
            presence.Remove((player, friend.Key));
            // A row a person friend listed: the person friend goes, and with it the rows it listed.
            if (RecordMut(player).FromPerson.Remove(friend.Key, out var handle))
                UnfriendQuietly(player, [handle]);
            host.ShowFriendRow(player, FriendRowKind.Removed, id, friend.ToString(), false);
            host.ShowSystem(player, $"{friend} has been removed from your friends list.");
            FriendshipEnded(player, [friend]);
            return true;
        }
    }

    /// <summary>`@friends clear`: the remote friends go with the local ones.</summary>
    public void RemoveAllFriends(uint player)
    {
        lock (gate)
        {
            var r = Record(player);
            if (r is null || r.Friends.Count == 0)
                return;
            var friends = r.Friends.Values.ToList();
            RecordMut(player).Friends.Clear();
            var handles = r.FromPerson.Values.ToList();
            RecordMut(player).FromPerson.Clear();
            UnfriendQuietly(player, handles);
            dirty = true;
            foreach (var k in presence.Keys.Where(k => k.Item1 == player).ToList())
                presence.Remove(k);
            FriendshipEnded(player, friends);
        }
    }

    /// <summary>
    /// `player` no longer lists `former`: it stops watching them. What they see of it is unchanged,
    /// since it is seen online by anyone.
    /// </summary>
    private void FriendshipEnded(uint player, IReadOnlyList<Address> former)
    {
        if (OwnAddress(player) is not { } owner)
            return;
        foreach (var f in former)
            SendIfUp(new Watch(owner, f, false));
    }

    /// <summary>
    /// Whether `watcher` may see `character` online now: the character is on the network, online and
    /// not appearing offline, and the operator does not block the watcher.
    /// </summary>
    private bool VisibleTo(uint character, Address watcher) =>
        IsOn(character) && Online(character) && !Blocked(watcher) && !host.AppearsOffline(character);

    /// <summary>Keeps `owner` as a watcher of `character`, within the caps (a watcher the character lists is always kept).</summary>
    private void KeepWatcher(uint character, Address owner)
    {
        var listed = Record(character)?.Friends.ContainsKey(owner.Key) == true;
        if (!watchers.TryGetValue(character, out var m))
        {
            m = new SortedDictionary<string, Address>(StringComparer.Ordinal);
            watchers[character] = m;
        }
        if (m.ContainsKey(owner.Key))
            return;
        if (!listed)
        {
            var suffix = "@" + owner.World;
            var fromWorld = m.Keys.Count(k => k.EndsWith(suffix, StringComparison.Ordinal));
            if (fromWorld >= MaxWatchersPerWorld || m.Count >= MaxWatchers)
                return;
        }
        m[owner.Key] = owner;
    }

    /// <summary>
    /// A watch from another world for an address of this one. Every watch within the other world's
    /// pace is answered, and alike: online only for a character on the network, online, not appearing
    /// offline, and the watcher not blocked; `offline` for anything else, a name this world does not
    /// have, a character off the network and a blocked watcher included, named as the watch named
    /// it. So a watch tells another world nothing a tell would not. A watcher of a character on the
    /// network is kept, a blocked one too (told nothing while the block lasts, so lifting it can show
    /// it what it may see).
    /// </summary>
    private void OnWatch(Address owner, Address target, bool on)
    {
        if (target.World != Config.Slug)
            return;
        var found = host.FindCharacter(target.Name);
        if (!on)
        {
            if (found is { } f && watchers.TryGetValue(f.Id, out var m))
                m.Remove(owner.Key);
            return;
        }
        // Each watch is owed an answer: another world is held to its share of this world's pace.
        if (!MayReceive(owner.World, WatchesKey, limits.WatchesPerWorldPair))
        {
            log(LogLevel.Debug, "a world sends watches faster than its share; one is not answered");
            return;
        }
        var online = false;
        if (found is { } c && IsOn(c.Id))
        {
            KeepWatcher(c.Id, owner);
            online = VisibleTo(c.Id, owner);
        }
        AnswerIfUp(new Presence(owner, target, online));
    }

    /// <summary>
    /// A remote character's status, from its home world, for the one local character it names. It
    /// is kept only while that character is online and lists the remote one, and shown at the pace
    /// <see cref="PresencePace"/> gives: a world that changes its answer faster is shown its latest.
    /// </summary>
    private void OnPresence(Address to, Address of, bool online)
    {
        if (to.World != Config.Slug || Blocked(of))
            return;
        if (host.OnlineByName(to.Name) is not uint watcher)
            return;
        if (Record(watcher)?.Friends.ContainsKey(of.Key) != true)
            return;
        var key = (watcher, of.Key);
        if (!presence.TryGetValue(key, out var status))
        {
            status = new FriendStatus { Online = online, Pace = new Bucket(PresencePace, Now) };
            presence[key] = status;
        }
        status.Online = online;
        if (status.Shown == online)
        {
            // Back to what the client shows: nothing to show, nothing waiting.
            presenceDue.Remove(key);
        }
        else if (status.Pace.Take(Now))
        {
            presenceDue.Remove(key);
            ShowStatus(watcher, of.Key);
        }
        else
        {
            presenceDue.Add(key);
        }
    }

    /// <summary>
    /// Shows `player` a remote friend's latest status, if it is not what the client shows: a status
    /// row, and the online or offline line for a change from a state the client was already shown
    /// (not for the first report after a friend is added or a world or the hub comes back, which is
    /// a state, not news).
    /// </summary>
    private void ShowStatus(uint player, string friendKey)
    {
        if (!presence.TryGetValue((player, friendKey), out var status) || status.Shown == status.Online)
            return;
        // The friend as this character listed it: the spelling the client holds.
        if (Record(player)?.Friends.TryGetValue(friendKey, out var listed) != true || listed is null)
            return;
        if (StandIn(listed) is not uint id)
            return;
        var before = status.Shown;
        status.Shown = status.Online;
        host.ShowFriendRow(player, FriendRowKind.Status, id, listed.ToString(), status.Online);
        if (before is not null)
        {
            // A character a person friend listed, other than its handle, comes online as its person.
            var name = listed.ToString();
            host.ShowSystem(player, HandleOf(player, friendKey) is { } handle && status.Online && handle.Key != listed.Key
                ? $"{handle} is online as <Tell:IIDString:{id}:{name}>{name}<\\Tell>."
                : PresenceLine(id, name, status.Online));
        }
    }

    /// <summary>Shows each status whose time has come its latest state.</summary>
    private void ShowDue(double t)
    {
        if (presenceDue.Count == 0)
            return;
        foreach (var key in presenceDue.ToList())
        {
            if (presence.TryGetValue(key, out var s) && !s.Pace.Has(t))
                continue;
            presenceDue.Remove(key);
            if (s is null)
                continue;
            s.Pace.Take(t);
            if (Online(key.Item1))
                ShowStatus(key.Item1, key.Item2);
        }
    }

    /// <summary>
    /// A world (or the whole hub) is gone: its characters show offline, without the lines, and their
    /// state is unknown until their world says again. Its watchers are forgotten too; it watches
    /// again when it is back.
    /// </summary>
    private void WorldGone(string slug)
    {
        var suffix = "@" + slug;
        foreach (var m in watchers.Values)
            foreach (var k in m.Keys.Where(k => k.EndsWith(suffix, StringComparison.Ordinal)).ToList())
                m.Remove(k);
        var known = presence.Where(p => p.Key.Item2.EndsWith(suffix, StringComparison.Ordinal)).ToList();
        foreach (var ((player, key), status) in known)
        {
            presence.Remove((player, key));
            if (status.Shown != true || !Online(player))
                continue;
            if (Record(player)?.Friends.TryGetValue(key, out var f) == true && f is not null && StandIn(f) is uint id)
                host.ShowFriendRow(player, FriendRowKind.Status, id, f.ToString(), false);
        }
    }

    /// <summary>Sends a watch for every remote friend of every character online, or of those on one world.</summary>
    private void WatchEveryone(string? only)
    {
        var watches = new List<WireMessage>();
        foreach (var player in host.OnlineCharacters())
        {
            var r = Record(player);
            if (r is null || !r.Settings.On)
                continue;
            if (OwnAddress(player) is not { } owner)
                continue;
            foreach (var f in r.Friends.Values)
                if (only is null || f.World == only)
                    watches.Add(new Watch(owner, f, true));
        }
        foreach (var m in watches)
            SendIfUp(m);
    }

    /// <summary>
    /// Tells every remote watcher of `player`, and every remote friend it lists (whose watch this
    /// world may not have kept, while the two were not yet mutual), what each may see of it now.
    /// </summary>
    private void TellWatchers(uint player, bool? online, string? knownName = null)
    {
        var to = new SortedDictionary<string, Address>(StringComparer.Ordinal);
        if (watchers.TryGetValue(player, out var m))
            foreach (var (k, a) in m)
                to[k] = a;
        if (Record(player) is { } r)
            foreach (var (k, a) in r.Friends)
                to.TryAdd(k, a);
        if (to.Count == 0 || OwnAddress(player, knownName) is not { } of)
            return;
        foreach (var w in to.Values)
            if (!Blocked(w))
                SendIfUp(new Presence(w, of, online ?? VisibleTo(player, w)));
    }

    /// <summary>A character entered the world: its watchers are told, and it watches its remote friends.</summary>
    public void OnLogin(uint player)
    {
        lock (gate)
            OnLoginLocked(player);
    }

    /// <summary>
    /// A character left the world (`name` is its name, since it is no longer online): its watchers
    /// see it offline, and it stops watching. A listed one's profile, from the facts the host had of
    /// it as it left, goes to the directory if it changed. A linked one keeps the tell squelches it
    /// had of this world's characters as it left (`squelches`; none when the host could not tell),
    /// by which a tell from one of them is answered while it is away.
    /// </summary>
    public void OnLogout(uint player, string name, CharacterFacts? facts = null, LocalSquelches? squelches = null)
    {
        lock (gate)
        {
            if (Record(player) is { Settings.On: true, AskedLink: true })
                RecordMut(player).AwaySquelches = squelches;
            DirectoryLeft(player, name, facts);
            OnLogoutLocked(player, name);
        }
    }

    /// <summary>A character's appear-offline choice changed: its watchers see what they may see now.</summary>
    public void SettingsChanged(uint player)
    {
        lock (gate)
            if (IsOn(player))
                TellWatchers(player, null);
    }

    /// <summary>Whether an operator's block of `key` (a world's slug, or a character's identity key) covers `who`.</summary>
    private static bool Covers(string key, string who)
    {
        if (who == key)
            return true;
        var at = who.LastIndexOf('@');
        return at >= 0 && who[(at + 1)..] == key;
    }

    /// <summary>
    /// The operator is about to block `key`: everything the block covers sees this world's
    /// characters offline, once, and is told nothing after; its watches are kept but answered nothing
    /// while the block lasts, its characters show offline here (without the lines), and this world's
    /// characters stop watching them.
    /// </summary>
    private void BlockBeginning(string key)
    {
        foreach (var player in host.OnlineCharacters())
        {
            if (!IsOn(player) || OwnAddress(player) is not { } of)
                continue;
            var told = new SortedDictionary<string, Address>(StringComparer.Ordinal);
            if (watchers.TryGetValue(player, out var m))
                foreach (var (k, a) in m)
                    if (Covers(key, k))
                        told[k] = a;
            var friends = Record(player)?.Friends.Where(f => Covers(key, f.Key)).Select(f => f.Value).ToList() ?? [];
            foreach (var f in friends)
                told.TryAdd(f.Key, f);
            foreach (var to in told.Values)
                SendIfUp(new Presence(to, of, false));
            foreach (var f in friends)
                SendIfUp(new Watch(of, f, false));
        }
        foreach (var ((player, k), status) in presence.Where(p => Covers(key, p.Key.Item2)).ToList())
        {
            presence.Remove((player, k));
            if (status.Shown != true || !Online(player))
                continue;
            if (Record(player)?.Friends.TryGetValue(k, out var f) == true && f is not null && StandIn(f) is uint id)
                host.ShowFriendRow(player, FriendRowKind.Status, id, f.ToString(), false);
        }
    }

    /// <summary>
    /// The operator has lifted the block of `key`: what the block took away is given back. Every
    /// watcher it covered (kept while it lasted) and every friend it covered sees what it may now, and
    /// this world's characters online watch the friends it covered again.
    /// </summary>
    private void BlockEnded(string key)
    {
        foreach (var player in host.OnlineCharacters())
        {
            if (!IsOn(player) || OwnAddress(player) is not { } of)
                continue;
            var friends = Record(player)?.Friends.Where(f => Covers(key, f.Key)).Select(f => f.Value).ToList() ?? [];
            var told = new SortedDictionary<string, Address>(StringComparer.Ordinal);
            if (watchers.TryGetValue(player, out var m))
                foreach (var (k, a) in m)
                    if (Covers(key, k))
                        told[k] = a;
            foreach (var f in friends)
                told.TryAdd(f.Key, f);
            foreach (var to in told.Values)
                SendIfUp(new Presence(to, of, VisibleTo(player, to)));
            foreach (var f in friends)
                SendIfUp(new Watch(of, f, true));
        }
    }

    /// <summary>
    /// A name a player gave (a tell's target, a squelch, a friend), without the mark a world shows
    /// beside the speaker of a line said from Discord: `Ann (Discord)` is `Ann`. No character's name
    /// holds a parenthesis, so a marked name a player clicks or types names the character.
    /// </summary>
    public static string GivenName(string name) => SayWire.WithoutMark(name);

    /// <summary>
    /// A character squelch by name (carried with id 0, its mark taken off: <see cref="GivenName"/>) or
    /// by a stand-in id. True when the target was remote. Squelching a channel leaves it.
    /// </summary>
    public bool Squelch(uint player, bool add, uint id, string name)
    {
        name = GivenName(name);
        lock (gate)
        {
            Address? who;
            if (StandIns.IsStandIn(id))
            {
                who = UseStandIn(id);
            }
            else if (Address.LooksRemote(name))
            {
                if (!Address.TryParse(name, out var a, out _))
                {
                    host.ShowSystem(player, $"{name.Trim()} not found.");
                    return true;
                }
                who = a;
            }
            else
            {
                return false;
            }
            if (who is null)
            {
                host.ShowSystem(player, "Couldn't find player to squelch.");
                return true;
            }
            if (who.World == Config.Slug)
                return false;
            if (add)
            {
                var held = Record(player);
                if (held?.Squelched.ContainsKey(who.Key) == true)
                {
                    host.ShowSystem(player, $"{who} is already squelched.");
                    return true;
                }
                if (held?.Squelched.Count >= MaxRemoteSquelches)
                {
                    host.ShowSystem(player, $"You squelch {MaxRemoteSquelches} characters on other worlds already. Unsquelch one first.");
                    return true;
                }
                // The squelch names the identity, so its stand-in id is kept as long as it lasts.
                StandIn(who);
                RecordMut(player).Squelched[who.Key] = who;
                dirty = true;
                host.ShowSystem(player, $"{who} has been squelched.");
                if (who.IsChannel)
                    LeaveChannel(player, who.Name);
            }
            else
            {
                var r = Record(player);
                if (r is not null && r.Squelched.ContainsKey(who.Key) && RecordMut(player).Squelched.Remove(who.Key))
                {
                    dirty = true;
                    host.ShowSystem(player, $"{who} has been unsquelched.");
                }
                else
                {
                    host.ShowSystem(player, $"{who} is not squelched.");
                }
            }
            return true;
        }
    }
}
