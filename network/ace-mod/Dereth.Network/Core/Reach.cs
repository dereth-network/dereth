// The reach messages (docs/network/02-link-protocol.md section 8): the hub's question whether a
// tell's recipient, not online, would have been shown it, asked before the hub carries the tell to
// the recipient's person elsewhere (its Discord), and the world's answer.
//
// They are a family of their own: ReachWire reads them, and Wire and DirectoryWire skip them as
// types they do not know.

using System.Text;
using System.Text.Json;

namespace Dereth.Network;

public abstract record ReachMessage(string Type);

/// <summary>
/// Hub to world: a tell from `From` to `To`, one of this world's characters, was answered `offline`.
/// Would `To`, not online, have been shown it?
/// </summary>
public sealed record ReachAsk(string Id, Address From, Address To) : ReachMessage("reach");

/// <summary>World to hub: `Reachable` when the character is not online and would have been shown the tell; false for anything else, alike.</summary>
public sealed record ReachResult(string Id, bool Reachable) : ReachMessage("reach.result");

public readonly record struct ReachReadResult(ReadStatus Status, ReachMessage? Message, string Why);

public static class ReachWire
{
    /// <summary>Every `type` of the reach messages.</summary>
    public static readonly IReadOnlyList<string> Types = ["reach", "reach.result"];

    private sealed class Bad(string why) : Exception(why);

    /// <summary>Reads one frame: one of the reach messages, `Unknown` for any other type, or `Malformed`.</summary>
    public static ReachReadResult Read(string text)
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

    private static bool Bool(JsonElement e, string name) => e.ValueKind switch
    {
        JsonValueKind.True => true,
        JsonValueKind.False => false,
        _ => throw new Bad(name + " is not a bool"),
    };

    /// <summary>A character's address: a channel is never one.</summary>
    private static Address Character(Dictionary<string, JsonElement> f, string name) =>
        Address.TryParse(Str(f, name), out var a, out _) && !a.IsChannel ? a : throw new Bad(name + " is not a character");

    private static ReachMessage Build(string kind, Dictionary<string, JsonElement> f)
    {
        var id = Str(f, "id");
        if (id.Length == 0 || Text.CharCount(id) > Wire.MaxIdChars)
            throw new Bad("id");
        if (kind == "reach")
            return new ReachAsk(id, Character(f, "from"), Character(f, "to"));
        return new ReachResult(id, Bool(Need(f, "reachable"), "reachable"));
    }

    /// <summary>Writes one frame.</summary>
    public static string Write(ReachMessage m)
    {
        var o = new JsonObj();
        o.Str("type", m.Type);
        switch (m)
        {
            case ReachAsk a:
                o.Str("id", a.Id).Str("from", a.From.ToString()).Str("to", a.To.ToString());
                break;
            case ReachResult r:
                o.Str("id", r.Id).Bool("reachable", r.Reachable);
                break;
        }
        return o.ToString();
    }
}
