// The say messages (docs/network/02-link-protocol.md section 9): the hub asks a world to say a line
// as one of the world's own characters in a room, a line the character's person said elsewhere (in a
// Discord channel that mirrors the room), and the world's answer.
//
// They are a family of their own: SayWire reads them, and Wire, DirectoryWire and ReachWire skip
// them as types they do not know.

using System.Text;
using System.Text.Json;

namespace Dereth.Network;

public abstract record SayMessage(string Type);

/// <summary>How a world took a line the hub asked it to say.</summary>
public enum SayOutcome
{
    /// <summary>Said: shown to the world's own players in the room, and sent on as a `room.say`.</summary>
    Said,

    /// <summary>Not said, for the reason given.</summary>
    Refused,
}

/// <summary>
/// Hub to world: say `Text` as `Who`, one of this world's characters, in `Room` (`global`, or one of
/// the world's own rooms), marked as said `Via` (`discord`).
/// </summary>
public sealed record SayAsk(string Id, string Room, Address Who, string Text, string Via) : SayMessage("say");

/// <summary>World to hub: how it took the line; `Reason` with a refusal alone.</summary>
public sealed record SayResult(string Id, SayOutcome Outcome, string? Reason = null) : SayMessage("say.result");

public readonly record struct SayReadResult(ReadStatus Status, SayMessage? Message, string Why);

/// <summary>The reasons a world gives for saying no line, as this version names them.</summary>
public static class SayReasons
{
    /// <summary>The world has no such character (none of that name, or one deleted).</summary>
    public const string NoCharacter = "no_character";

    /// <summary>The character is not on the network.</summary>
    public const string NotOnNetwork = "not_on_network";

    /// <summary>The character never asked for its link: the person linked an earlier character of its name.</summary>
    public const string NotLinked = "not_linked";

    /// <summary>The character's account is banned.</summary>
    public const string Banned = "banned";

    /// <summary>The character is gagged.</summary>
    public const string Gagged = "gagged";

    /// <summary>The character is younger than the operator's minimum age for the network.</summary>
    public const string TooNew = "too_new";

    /// <summary>Over the character's pace of room lines.</summary>
    public const string RateLimited = "rate_limited";

    /// <summary>The world does not carry the room now, or its operator turned it off.</summary>
    public const string RoomOff = "room_off";

    /// <summary>The world's own rules keep the character out of the room: an Olthoi player in a room it may not use, say.</summary>
    public const string NotAllowed = "not_allowed";
}

public static class SayWire
{
    /// <summary>Every `type` of the say messages.</summary>
    public static readonly IReadOnlyList<string> Types = ["say", "say.result"];

    /// <summary>Where a line was said, as a line's mark names it: in Discord.</summary>
    public const string ViaDiscord = "discord";

    /// <summary>The longest reason a world may give for a refusal, in characters.</summary>
    public const int MaxReasonChars = 32;

    /// <summary>Whether `s` may be a line's mark: 2 to 20 of `a-z`, `0-9` and inner hyphens, as a room's name.</summary>
    public static bool IsVia(string s) => Wire.IsRoomName(s);

    /// <summary>Whether `s` may be a refusal's reason: 1 to <see cref="MaxReasonChars"/> of `a-z` and `_`.</summary>
    public static bool IsReason(string s) => s.Length is >= 1 and <= MaxReasonChars && s.All(c => c is >= 'a' and <= 'z' or '_');

    /// <summary>
    /// How a line's mark is shown beside its speaker's name: `Discord` for `discord`; a mark this
    /// version does not name, as its word with its first letter in capitals.
    /// </summary>
    public static string MarkName(string via) => via == ViaDiscord ? "Discord" : char.ToUpperInvariant(via[0]) + via[1..];

    /// <summary>
    /// A name as a player gives it, without the mark a world shows beside the speaker of a line said
    /// from Discord: `Ann (Discord)` is `Ann`, and `Ann@coldeve (discord)` is `Ann@coldeve`. No
    /// character's name holds a parenthesis, so a trailing `(Discord)`, in any case, is never part of
    /// one; a player who clicks a marked speaker's name, or types it as shown, names the character.
    /// A name that is nothing but the mark is left as it is.
    /// </summary>
    public static string WithoutMark(string name)
    {
        var t = name.TrimEnd();
        if (!t.EndsWith(')'))
            return name;
        var inner = t[..^1];
        var open = inner.LastIndexOf('(');
        if (open < 0 || !inner[(open + 1)..].Equals(ViaDiscord, StringComparison.OrdinalIgnoreCase))
            return name;
        var rest = inner[..open].TrimEnd();
        return rest.Length == 0 ? name : rest;
    }

    private sealed class Bad(string why) : Exception(why);

    /// <summary>Reads one frame: one of the say messages, `Unknown` for any other type, or `Malformed`.</summary>
    public static SayReadResult Read(string text)
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

    /// <summary>A character's address: a channel is never one.</summary>
    private static Address Character(Dictionary<string, JsonElement> f, string name) =>
        Address.TryParse(Str(f, name), out var a, out _) && !a.IsChannel ? a : throw new Bad(name + " is not a character");

    private static SayMessage Build(string kind, Dictionary<string, JsonElement> f)
    {
        var id = Str(f, "id");
        if (id.Length == 0 || Text.CharCount(id) > Wire.MaxIdChars)
            throw new Bad("id");
        if (kind == "say")
        {
            var room = Str(f, "room");
            if (!Wire.IsRoomName(room))
                throw new Bad("room");
            var who = Character(f, "who");
            var text = Str(f, "text");
            if (Text.CharCount(text) > TextRules.MaxTextChars)
                throw new Bad("text too long");
            var via = Str(f, "via");
            if (!IsVia(via))
                throw new Bad("via");
            return new SayAsk(id, room, who, text, via);
        }
        var outcome = Str(f, "outcome") switch
        {
            "said" => SayOutcome.Said,
            "refused" => SayOutcome.Refused,
            _ => throw new Bad("outcome"),
        };
        string? reason = null;
        if (f.TryGetValue("reason", out var r) && r.ValueKind != JsonValueKind.Null)
        {
            reason = r.ValueKind == JsonValueKind.String ? r.GetString()! : throw new Bad("reason");
            if (outcome != SayOutcome.Refused)
                throw new Bad("a reason without a refusal");
            if (!IsReason(reason))
                throw new Bad("reason");
        }
        return new SayResult(id, outcome, reason);
    }

    /// <summary>Writes one frame.</summary>
    public static string Write(SayMessage m)
    {
        var o = new JsonObj();
        o.Str("type", m.Type);
        switch (m)
        {
            case SayAsk a:
                o.Str("id", a.Id).Str("room", a.Room).Str("who", a.Who.ToString()).Str("text", a.Text).Str("via", a.Via);
                break;
            case SayResult r:
                o.Str("id", r.Id).Str("outcome", r.Outcome == SayOutcome.Said ? "said" : "refused");
                if (r.Reason is not null)
                    o.Str("reason", r.Reason);
                break;
        }
        return o.ToString();
    }
}
