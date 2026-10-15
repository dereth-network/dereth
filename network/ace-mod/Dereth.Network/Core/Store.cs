// What the network keeps across restarts: the stand-in ids (never reused), whether each character
// is on the network, its remote friends and remote squelches (and, for one linked, its tell
// squelches of this world's characters as it last left), the operator's blocks, the directory's listings
// (each listed character's latest profile and whether the hub has answered it, the listings owed
// that take characters out, and the keys allegiances are known by), and the world's rooms as its hub
// last told them. Nothing else: no message, no presence history.
//
// The file is JSON, written off the world thread through a temporary file and a rename, so a cut
// write leaves the last good file in place.

using System.Globalization;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace Dereth.Network;

/// <summary>One character's network record: whether it is on the network, its remote friends and squelches.</summary>
public sealed class CharacterRecord
{
    public CharacterSettings Settings = CharacterSettings.Off;

    /// <summary>Remote friends by identity key.</summary>
    public readonly SortedDictionary<string, Address> Friends = new(StringComparer.Ordinal);

    /// <summary>Remote characters (and channels) this character squelches, by identity key.</summary>
    public readonly SortedDictionary<string, Address> Squelched = new(StringComparer.Ordinal);

    /// <summary>Whole worlds this character squelches.</summary>
    public readonly SortedSet<string> SquelchedWorlds = new(StringComparer.Ordinal);

    /// <summary>
    /// The friends its person friends listed, by identity key, with the handle each came by. They are
    /// among <see cref="Friends"/> only while the character is online, and never written to the file.
    /// </summary>
    public readonly SortedDictionary<string, Address> FromPerson = new(StringComparer.Ordinal);

    /// <summary>
    /// The character asked the hub for a link code (`/dn link`). A link holds only for a character that
    /// did: one the hub says is linked otherwise was linked under its name before it, by a character
    /// since deleted.
    /// </summary>
    public bool AskedLink;

    /// <summary>The character's listing in the directory, while it is listed.</summary>
    public ListingRecord? Listing;

    /// <summary>Why the character was taken out of the directory while it was away, to tell it at its next login.</summary>
    public TakenOut? TakenOut;

    /// <summary>
    /// Which of this world's characters it squelched for tells as it last left the world, kept for
    /// a character on the network and linked; null for one that has not left since.
    /// </summary>
    public LocalSquelches? AwaySquelches;
}

/// <summary>
/// The tell squelches a character had of its own world's characters as it last left the world:
/// characters by id, accounts by name, or everyone. While it is away the world cannot ask it, so a
/// tell from one of them is answered by these. Never changed in place, only replaced.
/// </summary>
public sealed record LocalSquelches(IReadOnlySet<uint> Characters, IReadOnlySet<string> Accounts, bool Everyone)
{
    /// <summary>Whether a tell from the character `id`, of the account `account`, would be held back.</summary>
    public bool Covers(uint id, string? account) =>
        Everyone || Characters.Contains(id) || (account is not null && Accounts.Contains(account));
}

/// <summary>What the world keeps of a listed character.</summary>
public sealed record ListingRecord
{
    /// <summary>The address it is listed under.</summary>
    public required Address Who { get; init; }

    /// <summary>The latest profile sent, or the one due to go, as its JSON.</summary>
    public required string Profile { get; init; }

    /// <summary>The hub has not yet answered this profile.</summary>
    public bool Owed { get; init; }

    /// <summary>The character's page in the directory, as the hub last named it.</summary>
    public string? Page { get; init; }
}

/// <summary>
/// Why a character was taken out of the directory while it was away. A file that kept other reasons
/// before reads them as the operator's.
/// </summary>
public enum TakenOut
{
    /// <summary>The hub's operator took it out, for a while.</summary>
    Operator,
    /// <summary>The directory holds as many of this world's characters as it may.</summary>
    Full,
    /// <summary>Its player asked the Dereth Network to forget them: it is off the network.</summary>
    Forgotten,
}

/// <summary>
/// The world's rooms as its hub last told them: the room that carries the Global room (one of the
/// world's own rooms, or `off`; null while the hub held none), and the own rooms the hub mirrors
/// into Discord for it. Never changed in place, only replaced.
/// </summary>
public sealed record KeptRooms(string? Global, IReadOnlyList<string> Mirrored);

public sealed class NetworkStore
{
    /// <summary>The world's rooms as its hub last told them; null until a hub has.</summary>
    public KeptRooms? Rooms;

    public StandIns StandIns = new();

    /// <summary>By character id.</summary>
    public readonly SortedDictionary<uint, CharacterRecord> Characters = new();

    /// <summary>The operator's blocks: world slugs, and identity keys of single characters.</summary>
    public readonly SortedSet<string> Blocked = new(StringComparer.Ordinal);

    /// <summary>
    /// The listings that take a character out of the directory, owed to the hub until it answers
    /// them, by the identity key of the address each names.
    /// </summary>
    public readonly SortedDictionary<string, Address> Unlistings = new(StringComparer.Ordinal);

    /// <summary>The key the directory knows each allegiance by, by its monarch's character id: a random value.</summary>
    public readonly SortedDictionary<uint, string> AllegianceKeys = new();

    /// <summary>The words the file spells a <see cref="TakenOut"/> with.</summary>
    public static string Word(TakenOut t) => t switch
    {
        TakenOut.Full => "full",
        TakenOut.Forgotten => "forgotten",
        _ => "operator",
    };

    /// <summary>A copy of the store that shares nothing with it, for the writer to serialise.</summary>
    public NetworkStore Snapshot() => Snapshot(null, null, -1);

    /// <summary>
    /// A copy of the store for the writer that shares nothing the world changes. Given the last
    /// copy, it reuses that copy's records for every character not in `changed`, and its stand-in
    /// table while the table's <see cref="StandIns.Version"/> is still `standInsVersion`: those
    /// copies are only ever read, so a save copies what changed since the last, not the whole store.
    /// </summary>
    public NetworkStore Snapshot(NetworkStore? last, IReadOnlySet<uint>? changed, long standInsVersion)
    {
        var s = new NetworkStore
        {
            StandIns = last is not null && standInsVersion == StandIns.Version ? last.StandIns : StandIns.Copy(),
            Rooms = Rooms,
        };
        foreach (var (id, r) in Characters)
        {
            if (last is not null && changed is not null && !changed.Contains(id)
                && last.Characters.TryGetValue(id, out var kept))
            {
                s.Characters[id] = kept;
                continue;
            }
            // A listing record is never changed in place, only replaced: the copy may share it.
            var c = new CharacterRecord
            {
                Settings = r.Settings, AskedLink = r.AskedLink, Listing = r.Listing, TakenOut = r.TakenOut,
                AwaySquelches = r.AwaySquelches,
            };
            // What person friends listed is the character's only while it is online.
            foreach (var (k, a) in r.Friends)
                if (!r.FromPerson.ContainsKey(k))
                    c.Friends[k] = a;
            foreach (var (k, a) in r.Squelched)
                c.Squelched[k] = a;
            foreach (var w in r.SquelchedWorlds)
                c.SquelchedWorlds.Add(w);
            s.Characters[id] = c;
        }
        foreach (var b in Blocked)
            s.Blocked.Add(b);
        foreach (var (k, a) in Unlistings)
            s.Unlistings[k] = a;
        foreach (var (m, k) in AllegianceKeys)
            s.AllegianceKeys[m] = k;
        return s;
    }

    // ---- the file ------------------------------------------------------------------------------

    /// <summary>The store as its file holds it.</summary>
    public string ToJson()
    {
        using var ms = new MemoryStream();
        using (var w = new Utf8JsonWriter(ms, new JsonWriterOptions
               {
                   Indented = true,
                   Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
               }))
        {
            w.WriteStartObject();
            w.WriteStartObject("stand_ins");
            w.WriteNumber("next", StandIns.NextForSave());
            w.WriteStartObject("by_key");
            foreach (var (id, who) in StandIns.All.OrderBy(p => p.Value.Key, StringComparer.Ordinal))
                w.WriteNumber(who.Key, id);
            w.WriteEndObject();
            w.WriteStartObject("by_id");
            foreach (var (id, who) in StandIns.All)
                w.WriteString(id.ToString(CultureInfo.InvariantCulture), who.ToString());
            w.WriteEndObject();
            w.WriteEndObject();
            w.WriteStartObject("characters");
            foreach (var (id, r) in Characters)
            {
                w.WriteStartObject(id.ToString(CultureInfo.InvariantCulture));
                w.WriteStartObject("settings");
                w.WriteBoolean("on", r.Settings.On);
                w.WriteEndObject();
                w.WriteStartObject("friends");
                foreach (var (k, a) in r.Friends)
                    if (!r.FromPerson.ContainsKey(k))
                        w.WriteString(k, a.ToString());
                w.WriteEndObject();
                w.WriteStartObject("squelched");
                foreach (var (k, a) in r.Squelched)
                    w.WriteString(k, a.ToString());
                w.WriteEndObject();
                w.WriteStartArray("squelched_worlds");
                foreach (var s in r.SquelchedWorlds)
                    w.WriteStringValue(s);
                w.WriteEndArray();
                if (r.AskedLink)
                    w.WriteBoolean("asked_link", true);
                if (r.Listing is { } l)
                {
                    w.WriteStartObject("listing");
                    w.WriteString("who", l.Who.ToString());
                    w.WritePropertyName("profile");
                    w.WriteRawValue(l.Profile);
                    if (l.Owed)
                        w.WriteBoolean("owed", true);
                    if (l.Page is not null)
                        w.WriteString("page", l.Page);
                    w.WriteEndObject();
                }
                if (r.TakenOut is { } t)
                    w.WriteString("taken_out", Word(t));
                if (r.AwaySquelches is { } q)
                {
                    w.WriteStartObject("away_squelches");
                    w.WriteStartArray("characters");
                    foreach (var x in q.Characters.Order())
                        w.WriteNumberValue(x);
                    w.WriteEndArray();
                    w.WriteStartArray("accounts");
                    foreach (var x in q.Accounts.Order(StringComparer.Ordinal))
                        w.WriteStringValue(x);
                    w.WriteEndArray();
                    if (q.Everyone)
                        w.WriteBoolean("everyone", true);
                    w.WriteEndObject();
                }
                w.WriteEndObject();
            }
            w.WriteEndObject();
            w.WriteStartArray("blocked");
            foreach (var b in Blocked)
                w.WriteStringValue(b);
            w.WriteEndArray();
            if (Unlistings.Count > 0)
            {
                w.WriteStartObject("unlistings");
                foreach (var (k, a) in Unlistings)
                    w.WriteString(k, a.ToString());
                w.WriteEndObject();
            }
            if (AllegianceKeys.Count > 0)
            {
                w.WriteStartObject("allegiance_keys");
                foreach (var (m, k) in AllegianceKeys)
                    w.WriteString(m.ToString(CultureInfo.InvariantCulture), k);
                w.WriteEndObject();
            }
            if (Rooms is { } rooms)
            {
                w.WriteStartObject("rooms");
                if (rooms.Global is not null)
                    w.WriteString("global", rooms.Global);
                w.WriteStartArray("mirrored");
                foreach (var r in rooms.Mirrored)
                    w.WriteStringValue(r);
                w.WriteEndArray();
                w.WriteEndObject();
            }
            w.WriteEndObject();
        }
        return Encoding.UTF8.GetString(ms.ToArray()) + "\n";
    }

    /// <summary>A store from its file's text.</summary>
    /// <exception cref="FormatException">The text is not a store.</exception>
    public static NetworkStore FromJson(string text)
    {
        try
        {
            using var doc = JsonDocument.Parse(text, new JsonDocumentOptions
            {
                AllowTrailingCommas = true,
                CommentHandling = JsonCommentHandling.Skip,
            });
            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
                throw new FormatException("not an object");
            var s = new NetworkStore();
            if (root.TryGetProperty("stand_ins", out var si) && si.ValueKind == JsonValueKind.Object)
            {
                var next = si.TryGetProperty("next", out var n) ? n.GetUInt32() : StandIns.First;
                var given = new List<KeyValuePair<uint, Address>>();
                if (si.TryGetProperty("by_id", out var byId))
                    foreach (var p in byId.EnumerateObject())
                    {
                        var id = uint.Parse(p.Name, NumberStyles.None, CultureInfo.InvariantCulture);
                        var who = Address.Parse(p.Value.GetString()) ?? throw new FormatException("a stand-in's address");
                        given.Add(new(id, who));
                    }
                s.StandIns = StandIns.Restore(next, given);
            }
            if (root.TryGetProperty("characters", out var chars))
                foreach (var p in chars.EnumerateObject())
                {
                    var id = uint.Parse(p.Name, NumberStyles.None, CultureInfo.InvariantCulture);
                    var r = new CharacterRecord();
                    var c = p.Value;
                    // A file that kept more settings beside `on` reads as `on` alone.
                    if (c.TryGetProperty("settings", out var st))
                        r.Settings = new CharacterSettings(st.TryGetProperty("on", out var o) && o.GetBoolean());
                    ReadAddresses(c, "friends", r.Friends);
                    ReadAddresses(c, "squelched", r.Squelched);
                    if (c.TryGetProperty("squelched_worlds", out var sw))
                        foreach (var x in sw.EnumerateArray())
                            r.SquelchedWorlds.Add(x.GetString() ?? throw new FormatException("a world"));
                    r.AskedLink = c.TryGetProperty("asked_link", out var al) && al.GetBoolean();
                    if (c.TryGetProperty("listing", out var li) && li.ValueKind == JsonValueKind.Object)
                        r.Listing = new ListingRecord
                        {
                            Who = Address.Parse(li.GetProperty("who").GetString()) ?? throw new FormatException("a listing's address"),
                            Profile = li.GetProperty("profile").GetRawText(),
                            Owed = li.TryGetProperty("owed", out var ow) && ow.GetBoolean(),
                            Page = li.TryGetProperty("page", out var pg) ? pg.GetString() : null,
                        };
                    if (c.TryGetProperty("taken_out", out var to))
                        r.TakenOut = to.GetString() switch
                        {
                            "full" => TakenOut.Full,
                            "forgotten" => TakenOut.Forgotten,
                            _ => TakenOut.Operator,
                        };
                    if (c.TryGetProperty("away_squelches", out var aw) && aw.ValueKind == JsonValueKind.Object)
                    {
                        var characters = new HashSet<uint>();
                        if (aw.TryGetProperty("characters", out var ac))
                            foreach (var x in ac.EnumerateArray())
                                characters.Add(x.GetUInt32());
                        var accounts = new HashSet<string>(StringComparer.Ordinal);
                        if (aw.TryGetProperty("accounts", out var aa))
                            foreach (var x in aa.EnumerateArray())
                                accounts.Add(x.GetString() ?? throw new FormatException("an account"));
                        r.AwaySquelches = new LocalSquelches(characters, accounts,
                            aw.TryGetProperty("everyone", out var ev) && ev.GetBoolean());
                    }
                    s.Characters[id] = r;
                }
            if (root.TryGetProperty("blocked", out var bl))
                foreach (var x in bl.EnumerateArray())
                    s.Blocked.Add(x.GetString() ?? throw new FormatException("a block"));
            ReadAddresses(root, "unlistings", s.Unlistings);
            if (root.TryGetProperty("allegiance_keys", out var ak))
                foreach (var p in ak.EnumerateObject())
                    s.AllegianceKeys[uint.Parse(p.Name, NumberStyles.None, CultureInfo.InvariantCulture)] =
                        p.Value.GetString() ?? throw new FormatException("an allegiance key");
            if (root.TryGetProperty("rooms", out var rm) && rm.ValueKind == JsonValueKind.Object)
            {
                var global = rm.TryGetProperty("global", out var g) && g.ValueKind == JsonValueKind.String ? g.GetString() : null;
                var mirrored = new List<string>();
                if (rm.TryGetProperty("mirrored", out var mi))
                    foreach (var x in mi.EnumerateArray())
                        mirrored.Add(x.GetString() ?? throw new FormatException("a mirrored room"));
                s.Rooms = new KeptRooms(global, mirrored);
            }
            return s;
        }
        catch (Exception e) when (e is JsonException or InvalidOperationException or OverflowException or KeyNotFoundException)
        {
            throw new FormatException(e.GetType().Name, e);
        }
    }

    private static void ReadAddresses(JsonElement c, string name, SortedDictionary<string, Address> into)
    {
        if (!c.TryGetProperty(name, out var m))
            return;
        foreach (var p in m.EnumerateObject())
        {
            var a = Address.Parse(p.Value.GetString()) ?? throw new FormatException("an address in " + name);
            into[a.Key] = a;
        }
    }

    /// <summary>
    /// The store saved at `path`; none yet is an empty store.
    /// </summary>
    /// <exception cref="IOException">The file exists and cannot be read.</exception>
    /// <exception cref="FormatException">The file exists and is not a store.</exception>
    public static NetworkStore Load(string path)
    {
        if (!File.Exists(path))
            return new NetworkStore();
        return FromJson(File.ReadAllText(path, Encoding.UTF8));
    }

    /// <summary>Writes `text` to `path` through a temporary file and a rename.</summary>
    public static void WriteAtomically(string path, string text)
    {
        var dir = Path.GetDirectoryName(Path.GetFullPath(path));
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);
        var tmp = path + ".tmp";
        using (var f = new FileStream(tmp, FileMode.Create, FileAccess.Write, FileShare.None))
        {
            var bytes = new UTF8Encoding(false).GetBytes(text);
            f.Write(bytes);
            f.Flush(true);
        }
        File.Move(tmp, path, overwrite: true);
    }
}

/// <summary>
/// Writes the store on a task of its own: the newest snapshot only, turned into JSON there, so the
/// world thread never serialises the store or waits on the disk.
/// </summary>
public sealed class StoreWriter
{
    private readonly string path;
    private readonly Action<string> log;
    private readonly object gate = new();
    private NetworkStore? pending;
    private bool draining;
    private Task running = Task.CompletedTask;

    public StoreWriter(string path, Action<string> log)
    {
        this.path = path;
        this.log = log;
    }

    /// <summary>Hands a snapshot to the writer; returns at once.</summary>
    public void Save(NetworkStore snapshot)
    {
        lock (gate)
        {
            pending = snapshot;
            if (!draining)
            {
                draining = true;
                running = Task.Run(Drain);
            }
        }
    }

    private void Drain()
    {
        while (true)
        {
            NetworkStore? snapshot;
            lock (gate)
            {
                snapshot = pending;
                pending = null;
                if (snapshot is null)
                {
                    draining = false;
                    return;
                }
            }
            try
            {
                NetworkStore.WriteAtomically(path, snapshot.ToJson());
            }
            catch (Exception e)
            {
                log("saving the network store failed: " + e.GetType().Name + ": " + e.Message);
            }
        }
    }

    /// <summary>Waits up to `limit` for what was handed over to be written.</summary>
    public bool Flush(TimeSpan limit)
    {
        Task t;
        lock (gate)
            t = running;
        try
        {
            return t.Wait(limit);
        }
        catch (AggregateException)
        {
            return true;
        }
    }
}
