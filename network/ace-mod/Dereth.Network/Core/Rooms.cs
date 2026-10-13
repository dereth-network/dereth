// The rooms messages (docs/network/02-link-protocol.md section 10): which of a world's own rooms
// carries the Global room, and which of its own rooms the hub mirrors into Discord for it. The hub
// tells the world (`rooms`) at each welcome and whenever either changes; the world asks the hub to
// carry the Global room in another of its rooms (`rooms.global`), or tells a hub that holds none
// where it carried it before (a seed).
//
// They are a family of their own: RoomsWire reads them, and Wire, DirectoryWire, ReachWire and
// SayWire skip them as types they do not know.

using System.Text;
using System.Text.Json;

namespace Dereth.Network;

public abstract record RoomsMessage(string Type);

/// <summary>
/// Hub to world: the world's rooms. `Global` is the own room that carries the Global room, or
/// `off`; null while the hub holds none for the world. `Mirrored` are the world's own rooms the hub
/// mirrors into Discord for it now. `Id` names the world's `rooms.global` this answers.
/// </summary>
public sealed record RoomsSetup(string? Id, string? Global, IReadOnlyList<string> Mirrored) : RoomsMessage("rooms");

/// <summary>
/// World to hub: carry the Global room in `Room` (one of the world's own rooms, or `off`) from now
/// on; with `Seed`, only while the hub holds none for the world.
/// </summary>
public sealed record RoomsGlobal(string Id, string Room, bool Seed = false) : RoomsMessage("rooms.global");

public readonly record struct RoomsReadResult(ReadStatus Status, RoomsMessage? Message, string Why);

public static class RoomsWire
{
    /// <summary>Every `type` of the rooms messages.</summary>
    public static readonly IReadOnlyList<string> Types = ["rooms", "rooms.global"];

    /// <summary>A world's own rooms, by their names on the link.</summary>
    public static readonly IReadOnlyList<string> OwnRooms = ["general", "trade", "lfg", "roleplay"];

    /// <summary>The word that says a world carries the Global room in none of its rooms.</summary>
    public const string Off = "off";

    /// <summary>Whether `s` names where a world carries the Global room: one of its own rooms, or `off`.</summary>
    public static bool IsGlobalRoom(string s) => s == Off || OwnRooms.Contains(s);

    private sealed class Bad(string why) : Exception(why);

    /// <summary>Reads one frame: one of the rooms messages, `Unknown` for any other type, or `Malformed`.</summary>
    public static RoomsReadResult Read(string text)
    {
        if (Encoding.UTF8.GetByteCount(text) > Wire.MaxMessageBytes)
            return new(ReadStatus.Malformed, null, "longer than the limit");
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(text);
        }
        catch (JsonException e)
        {
            return new(ReadStatus.Malformed, null, "not JSON: " + e.GetType().Name);
        }
        using (doc)
        {
            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
                return new(ReadStatus.Malformed, null, "no type");
            try
            {
                var f = new Dictionary<string, JsonElement>(StringComparer.Ordinal);
                foreach (var p in root.EnumerateObject())
                    f[p.Name] = p.Value;
                if (!f.TryGetValue("type", out var t) || t.ValueKind != JsonValueKind.String)
                    return new(ReadStatus.Malformed, null, "no type");
                var kind = t.GetString()!;
                if (!Types.Contains(kind))
                    return new(ReadStatus.Unknown, null, kind);
                return new(ReadStatus.Ok, Build(kind, f), "");
            }
            catch (Bad e)
            {
                return new(ReadStatus.Malformed, null, e.Message);
            }
            catch (Exception e) when (e is InvalidOperationException or FormatException or JsonException)
            {
                return new(ReadStatus.Malformed, null, "bad text");
            }
        }
    }

    private static JsonElement Need(Dictionary<string, JsonElement> f, string name) =>
        f.TryGetValue(name, out var v) ? v : throw new Bad("missing field " + name);

    private static string Str(Dictionary<string, JsonElement> f, string name) =>
        Need(f, name) is { ValueKind: JsonValueKind.String } v ? v.GetString()! : throw new Bad(name);

    /// <summary>A field that may be left out (or null): its text, or null.</summary>
    private static string? OptionalStr(Dictionary<string, JsonElement> f, string name)
    {
        if (!f.TryGetValue(name, out var v) || v.ValueKind == JsonValueKind.Null)
            return null;
        return v.ValueKind == JsonValueKind.String ? v.GetString()! : throw new Bad(name);
    }

    private static bool IdOk(string id) => id.Length > 0 && Text.CharCount(id) <= Wire.MaxIdChars;

    private static RoomsMessage Build(string kind, Dictionary<string, JsonElement> f)
    {
        if (kind == "rooms")
        {
            var id = OptionalStr(f, "id");
            if (id is not null && !IdOk(id))
                throw new Bad("id");
            var global = OptionalStr(f, "global");
            if (global is not null && !IsGlobalRoom(global))
                throw new Bad("global");
            var m = Need(f, "mirrored");
            if (m.ValueKind != JsonValueKind.Array)
                throw new Bad("mirrored");
            var mirrored = new List<string>();
            foreach (var r in m.EnumerateArray())
            {
                var room = r.ValueKind == JsonValueKind.String ? r.GetString()! : throw new Bad("mirrored");
                if (!OwnRooms.Contains(room))
                    throw new Bad("mirrored: one of the world's own rooms");
                if (mirrored.Contains(room))
                    throw new Bad("mirrored: each room once");
                if (room == global)
                    throw new Bad("mirrored: never the room carrying the Global room");
                mirrored.Add(room);
            }
            return new RoomsSetup(id, global, mirrored);
        }
        var ask = Str(f, "id");
        if (!IdOk(ask))
            throw new Bad("id");
        var to = Str(f, "room");
        if (!IsGlobalRoom(to))
            throw new Bad("room");
        var seed = false;
        if (f.TryGetValue("seed", out var s) && s.ValueKind != JsonValueKind.Null)
            seed = s.ValueKind switch
            {
                JsonValueKind.True => true,
                JsonValueKind.False => false,
                _ => throw new Bad("seed"),
            };
        return new RoomsGlobal(ask, to, seed);
    }

    /// <summary>Writes one frame.</summary>
    public static string Write(RoomsMessage m)
    {
        var o = new JsonObj();
        o.Str("type", m.Type);
        switch (m)
        {
            case RoomsSetup r:
                if (r.Id is not null)
                    o.Str("id", r.Id);
                if (r.Global is not null)
                    o.Str("global", r.Global);
                o.Raw("mirrored", "[" + string.Join(",", r.Mirrored.Select(Json.Str)) + "]");
                break;
            case RoomsGlobal g:
                o.Str("id", g.Id).Str("room", g.Room);
                if (g.Seed)
                    o.Bool("seed", true);
                break;
        }
        return o.ToString();
    }
}
