// What the network keeps across restarts: the stand-in ids (never reused), each character's
// settings, remote friends and remote squelches, and the operator's blocks. Nothing else: no
// message, no presence history.
//
// The file is JSON, written off the world thread through a temporary file and a rename, so a cut
// write leaves the last good file in place.

using System.Globalization;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace Dereth.Network;

/// <summary>One character's network record: its settings, its remote friends and squelches.</summary>
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
}

public sealed class NetworkStore
{
    public StandIns StandIns = new();

    /// <summary>By character id.</summary>
    public readonly SortedDictionary<uint, CharacterRecord> Characters = new();

    /// <summary>The operator's blocks: world slugs, and identity keys of single characters.</summary>
    public readonly SortedSet<string> Blocked = new(StringComparer.Ordinal);

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
        };
        foreach (var (id, r) in Characters)
        {
            if (last is not null && changed is not null && !changed.Contains(id)
                && last.Characters.TryGetValue(id, out var kept))
            {
                s.Characters[id] = kept;
                continue;
            }
            var c = new CharacterRecord { Settings = r.Settings, AskedLink = r.AskedLink };
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
                w.WriteString("visible", Words.Of(r.Settings.Visible));
                w.WriteString("reachable", Words.Of(r.Settings.Reachable));
                w.WriteString("listed", Words.Of(r.Settings.Listed));
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
                w.WriteEndObject();
            }
            w.WriteEndObject();
            w.WriteStartArray("blocked");
            foreach (var b in Blocked)
                w.WriteStringValue(b);
            w.WriteEndArray();
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
                    if (c.TryGetProperty("settings", out var st))
                    {
                        var on = st.TryGetProperty("on", out var o) && o.GetBoolean();
                        var vis = st.TryGetProperty("visible", out var v) && Words.TryVisible(v.GetString() ?? "", out var vv)
                            ? vv
                            : Visible.Nobody;
                        var reach = st.TryGetProperty("reachable", out var re) && Words.TryReachable(re.GetString() ?? "", out var rr)
                            ? rr
                            : Reachable.Nobody;
                        var listed = st.TryGetProperty("listed", out var l) && Words.TryListed(l.GetString() ?? "", out var ll)
                            ? ll
                            : Listed.Off;
                        r.Settings = new CharacterSettings(on, vis, reach, listed);
                    }
                    ReadAddresses(c, "friends", r.Friends);
                    ReadAddresses(c, "squelched", r.Squelched);
                    if (c.TryGetProperty("squelched_worlds", out var sw))
                        foreach (var x in sw.EnumerateArray())
                            r.SquelchedWorlds.Add(x.GetString() ?? throw new FormatException("a world"));
                    r.AskedLink = c.TryGetProperty("asked_link", out var al) && al.GetBoolean();
                    s.Characters[id] = r;
                }
            if (root.TryGetProperty("blocked", out var bl))
                foreach (var x in bl.EnumerateArray())
                    s.Blocked.Add(x.GetString() ?? throw new FormatException("a block"));
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
