// The Dereth Network's world half: the state this world keeps for the network and everything it
// does with it, with no ACE type and no I/O.
//
// It reaches the retail client only through messages the client already reads, by way of the
// host (<see cref="IWorldHost"/>):
//
// - Tells. A tell whose name holds `@` is a network address (`Bob@coldeve`) and goes to the hub, by
//   name or, for a reply, by the stand-in id the remote sender was shown under. A remote tell
//   arrives from the sender's stand-in id, which makes the name clickable and the sender the
//   client's reply target. The sender sees the echo and errors a local tell gives, once the hub
//   answers.
// - Friends. A remote friend is a friends-list row under its stand-in id, online as the friend's
//   home world reports, with the online and offline lines.
// - Rooms. The Global room is carried in one TurbineChat room the operator chooses; a channel is a
//   tell address (`oldguard@channel`).
// - Squelch. A remote character, a whole world, or (for the operator) any of these for everyone.
// - Persons. A character may be linked to its player's Discord account, through the hub; the
//   characters its person friends name are listed as its friends while it is online.
//
// Every remote line passes the network's text rules first. The link is the host's: <see cref="Pump"/>,
// run on the world thread, takes what the link received, lets the clock run, and hands the link
// what to send.

namespace Dereth.Network;

public sealed class WorldConfig
{
    /// <summary>The world's slug in the registry: the world half of its characters' addresses.</summary>
    public required string Slug { get; init; }

    /// <summary>
    /// The TurbineChat room the Global room is carried in; none carries no room. Settled before the
    /// link starts.
    /// </summary>
    public RoomSlot? GlobalRoom { get; set; }

    /// <summary>A character must have played this many seconds before it may send to the network.</summary>
    public uint MinCharacterAgeSeconds { get; init; }

    /// <summary>The software `hello` names.</summary>
    public string Software { get; init; } = "Dereth.Network";
}

public enum LinkState
{
    /// <summary>No connection. Tells, room lines and channel requests queue for the hold time.</summary>
    Down,
    /// <summary>Connected; `hello` sent, `welcome` awaited.</summary>
    Greeting,
    /// <summary>Welcomed.</summary>
    Up,
}

/// <summary>Where a tell by name goes.</summary>
public readonly record struct Routed(RouteKind Kind, string LocalName)
{
    public static readonly Routed NotMine = new(RouteKind.NotMine, "");
    public static readonly Routed Handled = new(RouteKind.Handled, "");
    public static Routed Local(string name) => new(RouteKind.Local, name);
}

public enum RouteKind
{
    /// <summary>Not a network address, or the network is off: the local tell as it is.</summary>
    NotMine,
    /// <summary>The network took it (or refused it, and told the sender).</summary>
    Handled,
    /// <summary>It names a character of this world by its address: the local tell goes to this name.</summary>
    Local,
}

public sealed partial class NetworkWorld
{
    /// <summary>
    /// How long after a character tells someone that someone may answer it, whatever the
    /// character's Reachable setting says, in seconds.
    /// </summary>
    public const double ReplyWindowSecs = 30 * 60;

    /// <summary>How often the heartbeat goes over the link, in seconds.</summary>
    public const double StatusEverySecs = 60;

    /// <summary>The least time between two saves of the store, in seconds.</summary>
    public const double SaveEverySecs = 2.0;

    /// <summary>
    /// The most remote watchers one local character keeps from any one world, and from all worlds
    /// together. A watcher the character lists as a friend is always kept.
    /// </summary>
    public const int MaxWatchersPerWorld = 100;

    /// <inheritdoc cref="MaxWatchersPerWorld"/>
    public const int MaxWatchers = 1000;

    /// <summary>
    /// How many new stand-in ids each remote world may cause this world to give: a burst, then a
    /// steady rate. A tell, a room line or a channel line from an identity that would need an id past
    /// that is not shown, nor is a character of that world a person view names, so one world
    /// inventing names cannot grow this world's table without end.
    /// </summary>
    public static readonly Rate NewStandInsPerWorld = new(200.0, 200.0 / 3600.0);

    /// <summary>
    /// The most watches and presence answers waiting for the link at once. They go after
    /// everything else; a newer one for the same pair of characters takes the waiting one's place,
    /// and past this the local character with the most waiting loses its oldest (a reconnect sends
    /// them again), so no one character can push out everyone else's.
    /// </summary>
    public const int MaxChatter = 2000;

    /// <summary>
    /// The most answers this world owes any one other world at once. A tell from a world already
    /// owed this many is not shown and not answered (the hub answers it `unavailable`), so a world
    /// flooding this one with tells delays only its own answers. Answers go after this world's own
    /// tells, a world at a time in turn.
    /// </summary>
    public const int MaxAnswersPerWorld = 100;

    /// <summary>How long a world has to answer a tell the hub hands it, in seconds.</summary>
    public const double AnswerSecs = 30;

    /// <summary>The most remote friends one character may list (the retail client's own limit for its list).</summary>
    public const int MaxRemoteFriends = 100;

    /// <summary>The most remote characters and channels one character may squelch.</summary>
    public const int MaxRemoteSquelches = 100;

    /// <summary>The most whole worlds one character may squelch.</summary>
    public const int MaxSquelchedWorlds = 50;

    /// <summary>
    /// How long a stand-in id no character's record names (as a friend or a squelch) is kept after it
    /// was last used, in seconds: a day. Past that its identity is forgotten (the id itself is never
    /// given again), so the table holds what the world's characters keep and what was seen lately.
    /// </summary>
    public const double StandInIdleSecs = 24 * 3600;

    /// <summary>The most stand-in ids no record names that the table keeps; past this the least lately used go first.</summary>
    public const int MaxIdleStandIns = 10_000;

    /// <summary>How often the stand-in table is looked over for identities to forget, in seconds.</summary>
    private const double ReclaimEverySecs = 60;

    /// <summary>
    /// How many status changes a character is shown for one remote friend at once, then how often: a
    /// few at once, then one every five seconds. A friend's world that changes its answer faster is
    /// shown only its latest state, as the pace allows.
    /// </summary>
    public static readonly Rate PresencePace = new(4.0, 0.2);

    /// <summary>
    /// The most of what the link received that one pump hands the world; the rest waits for the
    /// next, so a hub that sends without pause holds no iteration of the world's loop for long.
    /// </summary>
    public const int FramesPerPump = 256;

    /// <summary>The key another world's watches are paced under, beside rooms and local characters (it can be neither).</summary>
    private const string WatchesKey = "@watch";

    private sealed record QueuedMessage(double At, WireMessage Message);

    /// <summary>
    /// Watches and presence answers waiting for the link, each about one local character (a watch's
    /// owner, a presence's subject) and for one other world. See <see cref="MaxChatter"/>. They go a
    /// world at a time in turn, each world's at most at its share of the pace
    /// (<see cref="Limits.WatchesPerWorldPair"/>, a tenth under), so a world that floods this one with
    /// watches delays only its own answers.
    /// </summary>
    private sealed class Chatter
    {
        private long next;
        private readonly SortedDictionary<long, (string Local, string Slot, string World, WireMessage M)> queue = new();
        private readonly Dictionary<string, long> slots = new(StringComparer.Ordinal);
        private readonly Dictionary<string, SortedSet<long>> byLocal = new(StringComparer.Ordinal);
        private readonly SortedDictionary<string, SortedSet<long>> byWorld = new(StringComparer.Ordinal);
        private readonly Dictionary<string, Bucket> paces = new(StringComparer.Ordinal);
        private string turn = "";

        public int Count => queue.Count;

        public void Clear()
        {
            queue.Clear();
            slots.Clear();
            byLocal.Clear();
            byWorld.Clear();
            paces.Clear();
        }

        private static (string Local, string Slot, string World)? Keys(WireMessage m) => m switch
        {
            Watch w => (w.Owner.Key, $"w {w.Owner.Key} {w.Target.Key}", w.Target.World),
            Presence p => (p.Of.Key, $"p {p.Of.Key} {p.To.Key}", p.To.World),
            _ => null,
        };

        public void Push(WireMessage m)
        {
            if (Keys(m) is not var (local, slot, world))
                return;
            if (slots.TryGetValue(slot, out var waiting))
            {
                queue[waiting] = (local, slot, world, m);
                return;
            }
            if (queue.Count >= MaxChatter)
            {
                var most = byLocal.Values.MaxBy(s => s.Count);
                if (most is { Count: > 0 })
                    Remove(most.Min);
            }
            var seq = next++;
            slots[slot] = seq;
            if (!byLocal.TryGetValue(local, out var mine))
            {
                mine = [];
                byLocal[local] = mine;
            }
            mine.Add(seq);
            if (!byWorld.TryGetValue(world, out var theirs))
            {
                theirs = [];
                byWorld[world] = theirs;
            }
            theirs.Add(seq);
            queue[seq] = (local, slot, world, m);
        }

        private WireMessage? Remove(long seq)
        {
            if (!queue.Remove(seq, out var e))
                return null;
            slots.Remove(e.Slot);
            if (byLocal.TryGetValue(e.Local, out var s))
            {
                s.Remove(seq);
                if (s.Count == 0)
                    byLocal.Remove(e.Local);
            }
            if (byWorld.TryGetValue(e.World, out var w))
            {
                w.Remove(seq);
                if (w.Count == 0)
                    byWorld.Remove(e.World);
            }
            return e.M;
        }

        /// <summary>The next message for the world after the one served last that is within its pace at `t`, if any is.</summary>
        public WireMessage? PopReady(double t, Rate rate)
        {
            var worlds = byWorld.Keys.Where(k => string.CompareOrdinal(k, turn) > 0)
                .Concat(byWorld.Keys.Where(k => string.CompareOrdinal(k, turn) <= 0)).ToList();
            foreach (var world in worlds)
            {
                if (!paces.TryGetValue(world, out var pace))
                {
                    pace = new Bucket(rate, t);
                    paces[world] = pace;
                }
                if (!pace.Take(t))
                    continue;
                turn = world;
                return Remove(byWorld[world].Min);
            }
            return null;
        }

        /// <summary>Forgets the paces of worlds that have nothing waiting and have refilled.</summary>
        public void Settle(double t)
        {
            foreach (var k in paces.Where(p => !byWorld.ContainsKey(p.Key) && p.Value.FullAt(t)).Select(p => p.Key).ToList())
                paces.Remove(k);
        }
    }

    /// <summary>
    /// What a local character's client has been shown of one remote friend, beside what the friend's
    /// home world last said. See <see cref="PresencePace"/>.
    /// </summary>
    private sealed class FriendStatus
    {
        /// <summary>What the friend's home world last said.</summary>
        public bool Online;

        /// <summary>What the client was last shown; null until it has been shown this state's first report.</summary>
        public bool? Shown;

        /// <summary>What the client may still be shown now.</summary>
        public required Bucket Pace;
    }

    /// <summary>A tell not yet answered; `Sent` is when it was handed to the link, null while it waits for one.</summary>
    private sealed record PendingTell(uint Player, Address To, string Text, double? Sent);

    private sealed record PendingChannel(uint Player, double? Sent);

    private sealed record PendingRoom(uint Player, double? Sent);

    private enum Sends
    {
        Tells,
        RoomLines,
        ChannelRequests,
    }

    private readonly object gate = new();
    private readonly IWorldHost host;
    private readonly Logger log;
    private readonly Func<double> clock;
    private readonly Action<NetworkStore>? save;

    private readonly NetworkStore store;
    private bool dirty;
    private double nextSave;
    private LinkState link = LinkState.Down;
    private long epoch;
    private readonly SortedDictionary<string, WorldInfo> worlds = new(StringComparer.Ordinal);
    private Limits limits = Limits.Default;
    private Bucket? pace;
    private readonly List<WireMessage> control = [];

    /// <summary>This world's own tells, room lines, channel requests and heartbeat, in order.</summary>
    private readonly List<QueuedMessage> outbox = [];

    /// <summary>
    /// The answers owed to each other world, by world: sent after the outbox, a world at a time in
    /// turn, at most <see cref="MaxAnswersPerWorld"/> for one world.
    /// </summary>
    private readonly SortedDictionary<string, Queue<QueuedMessage>> answers = new(StringComparer.Ordinal);

    /// <summary>The world whose answer went last.</summary>
    private string answered = "";

    /// <summary>Watches and presence answers: sent after the answers, at most <see cref="MaxChatter"/> waiting.</summary>
    private readonly Chatter chatter = new();

    /// <summary>How many stand-in ids the store had given when it was last handed to be saved.</summary>
    private int savedStandIns;

    /// <summary>
    /// Tells, room lines and channel requests handed to the link and not yet answered, so a link that
    /// goes before they reach the hub sends them again.
    /// </summary>
    private readonly List<WireMessage> inFlight = [];

    private readonly Dictionary<string, PendingTell> pending = new(StringComparer.Ordinal);
    private readonly Dictionary<string, PendingChannel> pendingChannels = new(StringComparer.Ordinal);
    private readonly Dictionary<string, PendingRoom> pendingRooms = new(StringComparer.Ordinal);

    /// <summary>
    /// The last status each remote friend's home world gave each local character that lists it, and
    /// what that character was shown, by character id and the friend's identity key. A home world
    /// answers each watcher by that watcher's own standing, so one character's answer is never
    /// another's.
    /// </summary>
    private readonly Dictionary<(uint, string), FriendStatus> presence = new();

    /// <summary>Statuses whose latest state waits to be shown (<see cref="PresencePace"/>).</summary>
    private readonly HashSet<(uint, string)> presenceDue = new();

    /// <summary>When each stand-in id was last used (given, shown or named), this run.</summary>
    private readonly Dictionary<uint, double> standInsUsed = new();

    private double nextReclaim;

    /// <summary>The characters whose records changed since the store was last handed to be saved.</summary>
    private readonly HashSet<uint> changedRecords = new();

    /// <summary>The copy last handed to be saved, and the stand-in table's version then.</summary>
    private NetworkStore? lastSaved;

    private long lastSavedStandIns = -1;

    /// <summary>What each remote world may still cause in new stand-in ids.</summary>
    private readonly Dictionary<string, Bucket> newStandIns = new(StringComparer.Ordinal);

    /// <summary>Each local character's own sends, paced as the hub paces them.</summary>
    private readonly Dictionary<(uint, Sends), Bucket> sends = new();

    /// <summary>
    /// What each remote world may still bring, paced by the hub's per-world limits, by world and
    /// what it brings (a room's lines, or tells to one local character).
    /// </summary>
    private readonly Dictionary<(string, string), Bucket> received = new();

    /// <summary>Remote watchers of each local character, by character id.</summary>
    private readonly Dictionary<uint, SortedDictionary<string, Address>> watchers = new();

    /// <summary>When each local character last told each remote identity (the reply window).</summary>
    private readonly Dictionary<(uint, string), double> told = new();

    /// <summary>The ids of this run start with this, so that a restart reuses none the hub remembers.</summary>
    private readonly string run;
    private ulong nextId = 1;
    private double nextStatus;
    private double nextScan;

    public WorldConfig Config { get; }

    /// <param name="clock">Seconds on a monotonic scale.</param>
    /// <param name="unixStart">The start time, in seconds since 1970, that this run's ids carry.</param>
    /// <param name="save">
    /// Takes a copy of the store whenever it changed, at most every <see cref="SaveEverySecs"/>, to
    /// serialise and write off the world thread.
    /// </param>
    public NetworkWorld(WorldConfig config, NetworkStore store, IWorldHost host, Logger log, Func<double> clock,
        long unixStart, Action<NetworkStore>? save)
    {
        Config = config;
        this.store = store;
        this.host = host;
        this.log = log;
        this.clock = clock;
        this.save = save;
        run = unixStart.ToString(System.Globalization.CultureInfo.InvariantCulture);
        savedStandIns = store.StandIns.Count;
    }

    // ---- what the host and tests may read ------------------------------------------------------

    public LinkState Link
    {
        get
        {
            lock (gate)
                return link;
        }
    }

    /// <summary>How many messages wait for the link.</summary>
    public int Queued
    {
        get
        {
            lock (gate)
                return outbox.Count + answers.Values.Sum(q => q.Count) + chatter.Count;
        }
    }

    /// <summary>How many answers this world owes other worlds, waiting for the link.</summary>
    public int AnswersOwed
    {
        get
        {
            lock (gate)
                return answers.Values.Sum(q => q.Count);
        }
    }

    /// <summary>How many remote watchers a local character has.</summary>
    public int WatcherCount(uint character)
    {
        lock (gate)
            return watchers.TryGetValue(character, out var m) ? m.Count : 0;
    }

    /// <summary>How many presence states this world holds, for all its characters together.</summary>
    public int PresenceCount
    {
        get
        {
            lock (gate)
                return presence.Count;
        }
    }

    /// <summary>
    /// The status a local character's client was last shown for a remote friend, by the friend's
    /// identity key; null while it has been shown none.
    /// </summary>
    public bool? ShownStatus(uint character, string friendKey)
    {
        lock (gate)
            return presence.TryGetValue((character, friendKey), out var s) ? s.Shown : null;
    }

    /// <summary>How many stand-in ids this world has given.</summary>
    public int StandInCount
    {
        get
        {
            lock (gate)
                return store.StandIns.Count;
        }
    }

    /// <summary>A copy of a character's settings, if it has a record.</summary>
    public CharacterSettings? SettingsOf(uint character)
    {
        lock (gate)
            return store.Characters.TryGetValue(character, out var r) ? r.Settings : null;
    }

    /// <summary>The identity a stand-in id stands for.</summary>
    public Address? StandInFor(uint id)
    {
        lock (gate)
            return store.StandIns.Who(id);
    }

    /// <summary>The store's text now.</summary>
    public string StoreJson()
    {
        lock (gate)
            return store.ToJson();
    }

    // ---- small helpers -------------------------------------------------------------------------

    private double Now => clock();

    private string FreshId(char kind)
    {
        var id = kind + run + "-" + nextId.ToString(System.Globalization.CultureInfo.InvariantCulture);
        nextId++;
        return id;
    }

    /// <summary>Queues a message for the hub: sent at once when the link is up, else held.</summary>
    private void Send(WireMessage m) => outbox.Add(new QueuedMessage(Now, m));

    /// <summary>
    /// Sends a watch or a presence answer, only while the link is up (a reconnect sends them again),
    /// after everything else waiting. Nothing goes to a world or character the operator blocks.
    /// </summary>
    private void SendIfUp(WireMessage m)
    {
        var toBlocked = m switch
        {
            Watch w => Blocked(w.Target),
            Presence p => Blocked(p.To),
            _ => false,
        };
        if (!toBlocked)
            AnswerIfUp(m);
    }

    /// <summary>
    /// <see cref="SendIfUp"/> for the answer to a watch, which goes to a blocked watcher too: it is
    /// the same `offline` any watch for a character the watcher may not see is answered with.
    /// </summary>
    private void AnswerIfUp(WireMessage m)
    {
        if (link == LinkState.Up)
            chatter.Push(m);
    }

    /// <summary>Whether this world may take on one more answer for `world` now.</summary>
    private bool MayOwe(string world) => !answers.TryGetValue(world, out var q) || q.Count < MaxAnswersPerWorld;

    /// <summary>Queues an answer owed to `world`.</summary>
    private void SendAnswer(string world, WireMessage m)
    {
        if (!answers.TryGetValue(world, out var q))
        {
            q = new Queue<QueuedMessage>();
            answers[world] = q;
        }
        q.Enqueue(new QueuedMessage(Now, m));
    }

    /// <summary>The next answer owed, from the world after the one answered last.</summary>
    private WireMessage? NextAnswer()
    {
        if (answers.Count == 0)
            return null;
        var world = answers.Keys.FirstOrDefault(k => string.CompareOrdinal(k, answered) > 0) ?? answers.Keys.First();
        var q = answers[world];
        var m = q.Dequeue().Message;
        if (q.Count == 0)
            answers.Remove(world);
        answered = world;
        return m;
    }

    /// <summary>Whether a local character's own send of `kind` is within the hub's pace for it; takes one.</summary>
    private bool MaySend(uint player, Sends kind)
    {
        var t = Now;
        if (!sends.TryGetValue((player, kind), out var b))
        {
            b = new Bucket(kind == Sends.RoomLines ? limits.RoomLinesPerCharacter : limits.TellsPerCharacter, t);
            sends[(player, kind)] = b;
        }
        return b.Take(t);
    }

    /// <summary>
    /// Whether another world is within the hub's per-world limit `rate` for what it brings (`what`:
    /// a room, or a local character it tells); takes one. The hub enforces the same limits, so this
    /// holds only for a hub that does not.
    /// </summary>
    private bool MayReceive(string world, string what, Rate rate)
    {
        var t = Now;
        if (!received.TryGetValue((world, what), out var b))
        {
            b = new Bucket(rate, t);
            received[(world, what)] = b;
        }
        return b.Take(t);
    }

    /// <summary>Whether a local character is younger than the operator's minimum age for sending to the network.</summary>
    private bool TooNew(uint player)
    {
        var minAge = Config.MinCharacterAgeSeconds;
        return minAge > 0 && (host.AgeSeconds(player) is not int age || age < minAge);
    }

    /// <summary>What a character too new for the operator's minimum age is told when it sends to the network.</summary>
    private const string TooNewLine = "This character is too new to send to the Dereth Network yet.";

    /// <summary>What a character sending faster than its own share of the hub's limits is told.</summary>
    private const string TooQuicklyLine = "You are sending to the Dereth Network too quickly. Wait a moment and try again.";

    /// <summary>This world's address for a local character (its name given when it has just left).</summary>
    private Address? OwnAddress(uint character, string? knownName = null)
    {
        var name = knownName ?? host.OnlineName(character);
        return name is null ? null : Address.Create(name, Config.Slug);
    }

    /// <summary>A character's record to change: it is marked changed, for the next save.</summary>
    private CharacterRecord RecordMut(uint character)
    {
        if (!store.Characters.TryGetValue(character, out var r))
        {
            r = new CharacterRecord();
            store.Characters[character] = r;
        }
        changedRecords.Add(character);
        return r;
    }

    private CharacterRecord? Record(uint character) => store.Characters.GetValueOrDefault(character);

    private bool IsOn(uint character) => Record(character)?.Settings.On == true;

    /// <summary>
    /// The stand-in id of a remote identity, given now if it has none; null once the block is used
    /// up, so an identity with no id of its own is never shown, rather than shown under another's.
    /// </summary>
    private uint? StandIn(Address who)
    {
        var before = store.StandIns.Version;
        var id = store.StandIns.IdFor(who);
        if (store.StandIns.Version != before)
            dirty = true;
        if (id is uint given)
            standInsUsed[given] = Now;
        else
            log(LogLevel.Warn, "every stand-in id has been given; a correspondent is not shown");
        return id;
    }

    /// <summary>The identity a stand-in id stands for, marked used now.</summary>
    private Address? UseStandIn(uint id)
    {
        var who = store.StandIns.Who(id);
        if (who is not null)
            standInsUsed[id] = Now;
        return who;
    }

    /// <summary>
    /// Forgets the identities of stand-in ids no record names and nobody has used for
    /// <see cref="StandInIdleSecs"/>, and the least lately used past <see cref="MaxIdleStandIns"/>.
    /// Their ids are never given again: an id a client still holds names no one, never someone else.
    /// </summary>
    private void ReclaimStandIns(double t)
    {
        var named = new HashSet<string>(StringComparer.Ordinal);
        foreach (var r in store.Characters.Values)
        {
            named.UnionWith(r.Friends.Keys);
            named.UnionWith(r.Squelched.Keys);
        }
        var idle = new List<(double Used, uint Id)>();
        foreach (var (id, who) in store.StandIns.All)
        {
            if (named.Contains(who.Key))
                continue;
            // An id from before this run is counted as used when the run began.
            if (!standInsUsed.TryGetValue(id, out var used))
            {
                used = t;
                standInsUsed[id] = t;
            }
            idle.Add((used, id));
        }
        var over = Math.Max(0, idle.Count - MaxIdleStandIns);
        var forget = idle.OrderBy(x => x.Used).ThenBy(x => x.Id)
            .Where((x, i) => i < over || t - x.Used >= StandInIdleSecs).Select(x => x.Id).ToList();
        if (forget.Count == 0)
            return;
        foreach (var id in forget)
        {
            store.StandIns.Forget(id);
            standInsUsed.Remove(id);
        }
        dirty = true;
        log(LogLevel.Debug, $"forgot {forget.Count} stand-in id(s) nothing names any more");
    }

    /// <summary>The store as the writer gets it: a copy that shares nothing the world changes, made from the last one.</summary>
    private NetworkStore SnapshotForSave()
    {
        var s = store.Snapshot(lastSaved, changedRecords, lastSavedStandIns);
        lastSaved = s;
        lastSavedStandIns = store.StandIns.Version;
        changedRecords.Clear();
        return s;
    }

    /// <summary>
    /// The stand-in id of an identity another world brought here (a tell, a room line or a channel
    /// line from it, or a person view naming it), as <see cref="StandIn"/>; a new one only while
    /// that world is within <see cref="NewStandInsPerWorld"/>. `budgetSpent` says whether a null
    /// is that world's share spent (rather than the block used up).
    /// </summary>
    private uint? RemoteStandIn(Address who) => RemoteStandIn(who, out _);

    /// <inheritdoc cref="RemoteStandIn(Address)"/>
    private uint? RemoteStandIn(Address who, out bool budgetSpent)
    {
        budgetSpent = false;
        if (store.StandIns.Get(who) is not null)
            return StandIn(who);
        var t = Now;
        if (!newStandIns.TryGetValue(who.World, out var b))
        {
            b = new Bucket(NewStandInsPerWorld, t);
            newStandIns[who.World] = b;
        }
        if (!b.Take(t))
        {
            log(LogLevel.Debug, "a world brought too many new names; one is not shown");
            budgetSpent = true;
            return null;
        }
        return StandIn(who);
    }

    /// <summary>Whether the operator blocks `who` (the character, or its whole world).</summary>
    private bool Blocked(Address who) => store.Blocked.Contains(who.World) || store.Blocked.Contains(who.Key);

    private bool Online(uint character) => host.OnlineName(character) is not null;

    // ---- the link ------------------------------------------------------------------------------

    /// <summary>
    /// On the world thread: what the link received goes to the world, the world's clock runs, what
    /// it has to send goes to the link, and a changed store is handed to be saved.
    /// </summary>
    public void Pump(ILink l)
    {
        lock (gate)
        {
            for (var taken = 0; taken < FramesPerPump && l.TryTake(out var e); taken++)
            {
                switch (e.Kind)
                {
                    case LinkEventKind.Up:
                        epoch = e.Epoch;
                        OnLinkUp();
                        break;
                    case LinkEventKind.Frame:
                        if (e.Epoch == epoch)
                            OnFrame(e.Text);
                        break;
                    case LinkEventKind.Down:
                        if (e.Epoch == epoch)
                            OnLinkDown();
                        break;
                    case LinkEventKind.Refused:
                        break;
                }
            }
            Tick();
            foreach (var frame in TakeFrames())
                l.Send(epoch, frame);
            ShowVerifyAnswers();
            var t = Now;
            // Half the save margin in new stand-in ids goes at once, so the ids given since the last
            // save never outrun the margin the saved table starts past.
            var urgent = store.StandIns.Count - savedStandIns >= StandIns.SaveMargin / 2;
            if (dirty && (t >= nextSave || urgent))
            {
                dirty = false;
                nextSave = t + SaveEverySecs;
                savedStandIns = store.StandIns.Count;
                save?.Invoke(SnapshotForSave());
            }
        }
    }

    /// <summary>Hands the store to be saved now if it changed (at shutdown).</summary>
    public void SaveNow()
    {
        lock (gate)
        {
            dirty = false;
            savedStandIns = store.StandIns.Count;
            save?.Invoke(SnapshotForSave());
        }
    }

    private void OnLinkUp()
    {
        link = LinkState.Greeting;
        control.Clear();
        control.Add(new Hello(Wire.Protocol, Config.Software));
        log(LogLevel.Info, "connected to the hub; greeting");
    }

    private void OnLinkDown()
    {
        if (link == LinkState.Down)
            return;
        link = LinkState.Down;
        control.Clear();
        pace = null;
        chatter.Clear();
        // The hub answers for this world every tell it had not answered when the link went.
        answers.Clear();
        // What a reconnect sends again anyway is not held.
        outbox.RemoveAll(q => q.Message is not (Tell or RoomSay or ChannelRequest or PersonRequest));
        // What was handed to the link and not yet answered waits again at the front, since it may
        // never have reached the hub (which drops a repeat of one that did).
        var t = Now;
        var again = new List<QueuedMessage>();
        foreach (var m in inFlight)
        {
            switch (m)
            {
                case Tell tell when pending.TryGetValue(tell.Id, out var p):
                    pending[tell.Id] = p with { Sent = null };
                    again.Add(new QueuedMessage(t, m));
                    break;
                case ChannelRequest c when pendingChannels.TryGetValue(c.Id, out var p):
                    pendingChannels[c.Id] = p with { Sent = null };
                    again.Add(new QueuedMessage(t, m));
                    break;
                case RoomSay r when pendingRooms.TryGetValue(r.Id, out var p):
                    pendingRooms[r.Id] = p with { Sent = null };
                    again.Add(new QueuedMessage(t, m));
                    break;
                case PersonRequest r when pendingPersons.TryGetValue(r.Id, out var p):
                    pendingPersons[r.Id] = p with { Sent = null };
                    again.Add(new QueuedMessage(t, m));
                    break;
            }
        }
        inFlight.Clear();
        outbox.InsertRange(0, again);
        log(LogLevel.Info, "the link to the hub is down; " + outbox.Count + " message(s) held");
        foreach (var slug in worlds.Keys.ToList())
            WorldGone(slug);
        // Every other world watches again when this one is back.
        watchers.Clear();
        presence.Clear();
        presenceDue.Clear();
        foreach (var slug in worlds.Keys.ToList())
            worlds[slug] = worlds[slug] with { Online = false };
    }

    /// <summary>
    /// What to send now: `hello` while greeting; once welcomed, the queue, paced to the hub's world
    /// limit. This world's own tells, room lines and channel requests go first, then the answers it
    /// owes other worlds (a world at a time in turn), then watches and presence answers, so no
    /// number of the later can hold up the earlier.
    /// </summary>
    private List<string> TakeFrames()
    {
        var frames = new List<string>();
        switch (link)
        {
            case LinkState.Greeting:
                frames.AddRange(control.Select(Wire.Write));
                control.Clear();
                break;
            case LinkState.Up:
                var t = Now;
                // A tenth under the hub's own numbers, so the two clocks never disagree at the edge.
                pace ??= new Bucket(new Rate(Math.Max(1.0, limits.MessagesPerWorld.Burst * 0.9),
                    limits.MessagesPerWorld.PerSec * 0.9), t);
                var share = limits.WatchesPerWorldPair;
                share = new Rate(Math.Max(1.0, share.Burst * 0.9), share.PerSec * 0.9);
                while ((outbox.Count > 0 || answers.Count > 0 || chatter.Count > 0) && pace.Has(t))
                {
                    WireMessage? next;
                    if (outbox.Count > 0)
                    {
                        next = outbox[0].Message;
                        outbox.RemoveAt(0);
                    }
                    else
                    {
                        next = NextAnswer() ?? chatter.PopReady(t, share);
                    }
                    if (next is not { } m)
                        break;
                    pace.Take(t);
                    switch (m)
                    {
                        case Tell tell when pending.TryGetValue(tell.Id, out var p):
                            pending[tell.Id] = p with { Sent = t };
                            inFlight.Add(m);
                            break;
                        case ChannelRequest c when pendingChannels.TryGetValue(c.Id, out var p):
                            pendingChannels[c.Id] = p with { Sent = t };
                            inFlight.Add(m);
                            break;
                        case RoomSay r when pendingRooms.TryGetValue(r.Id, out var p):
                            pendingRooms[r.Id] = p with { Sent = t };
                            inFlight.Add(m);
                            break;
                        case PersonRequest r when pendingPersons.TryGetValue(r.Id, out var p):
                            pendingPersons[r.Id] = p with { Sent = t };
                            inFlight.Add(m);
                            break;
                    }
                    frames.Add(Wire.Write(m));
                }
                break;
        }
        return frames;
    }

    private void OnFrame(string text)
    {
        var r = Wire.Read(text);
        switch (r.Status)
        {
            case ReadStatus.Ok:
                HandleHubMessage(r.Message!);
                break;
            case ReadStatus.Unknown:
                log(LogLevel.Debug, "skipped a message of a type this version does not know");
                break;
            default:
                log(LogLevel.Debug, "skipped a malformed frame from the hub");
                break;
        }
    }

    private void HandleHubMessage(WireMessage m)
    {
        switch (m)
        {
            case Welcome w:
            {
                if (w.World != Config.Slug)
                    log(LogLevel.Warn, $"the hub knows this world as \"{w.World}\", not \"{Config.Slug}\"");
                link = LinkState.Up;
                limits = w.Limits;
                pace = null;
                worlds.Clear();
                foreach (var i in w.Worlds)
                    if (i.Slug.Length > 0)
                        worlds[i.Slug] = i;
                log(LogLevel.Info, $"welcomed; {worlds.Values.Count(i => i.Online)} world(s) online");
                // The room goes first, then what was queued; the watches of everyone online, for
                // the worlds online now, go after (a world the hub names later is watched when it does).
                var queued = outbox.ToList();
                outbox.Clear();
                if (Config.GlobalRoom is not null && w.Rooms.Contains(Wire.GlobalRoom))
                    Send(new RoomJoin(Wire.GlobalRoom, true));
                outbox.AddRange(queued);
                foreach (var slug in worlds.Values.Where(i => i.Online).Select(i => i.Slug).ToList())
                    WatchEveryone(slug);
                WatchPersonsEveryone();
                break;
            }
            case WorldChange c:
            {
                var was = worlds.TryGetValue(c.Info.Slug, out var before) && before.Online;
                worlds[c.Info.Slug] = c.Info;
                if (c.Info.Online && !was)
                    WatchEveryone(c.Info.Slug);
                else if (!c.Info.Online)
                    WorldGone(c.Info.Slug);
                break;
            }
            case Watch w:
                OnWatch(w.Owner, w.Target, w.On);
                break;
            case Presence p:
                OnPresence(p.To, p.Of, p.Online);
                break;
            case Tell t:
                OnTell(t.Id, t.From, t.To, t.Text, t.Friend);
                break;
            case TellResult r:
                OnTellResult(r.Id, r.Outcome, r.Message);
                break;
            case RoomLine r:
                OnRoomLine(r.Room, r.From, r.Text);
                break;
            case RoomResult r:
                OnRoomResult(r.Id, r.Outcome);
                break;
            case ChannelResult c:
                OnChannelResult(c.Id, c.Outcome, c.Channel, c.Members, c.Channels);
                break;
            case ChannelInvite c:
                OnChannelInvite(c.Channel, c.By, c.To);
                break;
            case ChannelLine c:
                OnChannelLine(c.Channel, c.From, c.To, c.Text);
                break;
            case PersonResult r:
                OnPersonResult(r);
                break;
            case PersonFriends v:
                OnPersonView(v.To, v.Linked, v.Alts, v.Persons);
                break;
            case ProtocolError e:
                log(LogLevel.Warn, "the hub reports a protocol error: " + Sanitized(e.Code));
                break;
        }
    }

    /// <summary>A word from the hub fit for the log: short, and only letters, digits and a few marks.</summary>
    private static string Sanitized(string s)
    {
        var t = new string(s.Where(c => char.IsAsciiLetterOrDigit(c) || c is '_' or '-' or '.').Take(32).ToArray());
        return t.Length == 0 ? "?" : t;
    }

    /// <summary>
    /// The passage of time: queued messages and unanswered requests whose time is up are given up
    /// on, and the heartbeat goes once a minute while the link is up.
    /// </summary>
    private void Tick()
    {
        var t = Now;
        if (link == LinkState.Up && t >= nextStatus)
        {
            nextStatus = t + StatusEverySecs;
            Send(new Status(host.WorldOpen() ? "online" : "maintenance", (uint)Math.Max(0, host.OnlineCount())));
        }
        if (t < nextScan)
            return;
        nextScan = t + 0.25;
        var hold = limits.HoldSecs;
        var timeout = limits.ResultTimeoutSecs;
        // An answer the hub has stopped waiting for is of no use to anyone.
        foreach (var (world, q) in answers.ToList())
        {
            while (q.Count > 0 && t - q.Peek().At >= AnswerSecs)
                q.Dequeue();
            if (q.Count == 0)
                answers.Remove(world);
        }
        // Queued too long: dropped, and a tell's sender told.
        var expired = outbox.Where(q => t - q.At >= hold).ToList();
        if (expired.Count > 0)
        {
            outbox.RemoveAll(q => t - q.At >= hold);
            foreach (var q in expired)
            {
                switch (q.Message)
                {
                    case Tell tell:
                        GiveUp(tell.Id, true);
                        break;
                    case ChannelRequest c:
                        GiveUpChannel(c.Id);
                        break;
                    case RoomSay r:
                        pendingRooms.Remove(r.Id);
                        break;
                    case PersonRequest r:
                        GiveUpPerson(r.Id);
                        break;
                }
            }
        }
        // A request is given up on only once it has been with the hub longer than the hub may take
        // to answer it (its hold for a reconnecting world and the target's answer window together).
        bool Late(double? sent) => sent is double s && t - s >= timeout;
        foreach (var id in pending.Where(p => Late(p.Value.Sent)).Select(p => p.Key).ToList())
            GiveUp(id, link != LinkState.Up);
        foreach (var id in pendingChannels.Where(p => Late(p.Value.Sent)).Select(p => p.Key).ToList())
            GiveUpChannel(id);
        foreach (var id in pendingRooms.Where(p => Late(p.Value.Sent)).Select(p => p.Key).ToList())
            pendingRooms.Remove(id);
        foreach (var id in pendingPersons.Where(p => Late(p.Value.Sent)).Select(p => p.Key).ToList())
            GiveUpPerson(id);
        inFlight.RemoveAll(m => m switch
        {
            Tell tell => !pending.ContainsKey(tell.Id),
            ChannelRequest c => !pendingChannels.ContainsKey(c.Id),
            RoomSay r => !pendingRooms.ContainsKey(r.Id),
            PersonRequest r => !pendingPersons.ContainsKey(r.Id),
            _ => true,
        });
        foreach (var k in sends.Where(p => p.Value.FullAt(t)).Select(p => p.Key).ToList())
            sends.Remove(k);
        foreach (var k in newStandIns.Where(p => p.Value.FullAt(t)).Select(p => p.Key).ToList())
            newStandIns.Remove(k);
        foreach (var k in received.Where(p => p.Value.FullAt(t)).Select(p => p.Key).ToList())
            received.Remove(k);
        foreach (var k in told.Where(p => t - p.Value >= ReplyWindowSecs).Select(p => p.Key).ToList())
            told.Remove(k);
        chatter.Settle(t);
        ShowDue(t);
        if (t >= nextReclaim)
        {
            nextReclaim = t + ReclaimEverySecs;
            ReclaimStandIns(t);
        }
    }

    /// <summary>
    /// The friend-list line a status change gives, with the name a link the client can click to
    /// start a tell (the retail client draws name links in any line, server text included).
    /// </summary>
    private static string PresenceLine(uint id, string name, bool online) =>
        $"<Tell:IIDString:{id}:{name}>{name}<\\Tell> has {(online ? "come on" : "gone off")}line.";
}
