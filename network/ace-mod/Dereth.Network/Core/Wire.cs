// The link messages: one JSON object per WebSocket text frame, between a world and the hub.
//
// Every message is an object whose `type` names it. An object with a `type` this version does not
// know is skipped, not an error, so a later version can add messages; a known type with a missing
// or bad field is malformed. The JSON this writes is the protocol's own shape: snake_case fields,
// the `type` tag first, the fields in the order the specification lists them.

using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Dereth.Network;

/// <summary>How a tell, a room line or a hub request ended.</summary>
public enum Outcome
{
    /// <summary>Shown to the recipient (for a channel, to every member online).</summary>
    Delivered,
    /// <summary>Shown to the recipient, who is away; the result's message is the away text.</summary>
    Afk,
    /// <summary>Not shown: the recipient squelches the sender.</summary>
    Squelched,
    /// <summary>Not shown: not online, not on the network, does not allow the sender, or refused.</summary>
    Offline,
    /// <summary>Not shown: the recipient's world is on the network but unreachable now.</summary>
    Unavailable,
    /// <summary>Not shown: the recipient's world is not on the network.</summary>
    NotOnNetwork,
    /// <summary>Not sent: over a rate limit.</summary>
    RateLimited,
    /// <summary>Not sent: not a member of the channel, or the world does not carry the room.</summary>
    NotMember,
    /// <summary>Accepted.</summary>
    Ok,
}

/// <summary>What a channel request asks.</summary>
public enum ChannelOp
{
    Create,
    Invite,
    Join,
    Leave,
    List,
    Members,
}

/// <summary>How a channel request ended.</summary>
public enum ChannelOutcome
{
    Ok,
    Exists,
    NoChannel,
    NotMember,
    NotInvited,
    AlreadyMember,
    Refused,
}

/// <summary>What a person request asks.</summary>
public enum PersonOp
{
    /// <summary>A one-time code to link the character on the site.</summary>
    Code,
    /// <summary>Unlink the character from its person.</summary>
    Unlink,
    /// <summary>
    /// Set the alts setting of the character's person: sent by worlds of the network's first version
    /// alone, and refused by the hub. This world never sends it.
    /// </summary>
    Alts,
    /// <summary>List the target's person as a person friend.</summary>
    Befriend,
    /// <summary>Remove the person friend whose handle is the target.</summary>
    Unfriend,
    /// <summary>The characters of the target's person: sent by worlds of the network's first version alone. This world never sends it.</summary>
    Characters,
    /// <summary>List each of the targets' persons as a person friend: the friends the character had before it was linked.</summary>
    Carry,
}

/// <summary>How a person request ended.</summary>
public enum PersonOutcome
{
    Ok,
    NotLinked,
    RateLimited,
    Unavailable,
    Refused,
}

/// <summary>One person friend in a person view: its handle, then the characters it lists.</summary>
public sealed record PersonFriend(Address Handle, IReadOnlyList<Address> Characters);

/// <summary>A world as the hub describes it to the others.</summary>
public sealed record WorldInfo(string Slug, string Name, bool Online);

public abstract record WireMessage
{
    /// <summary>The message's `type`.</summary>
    public abstract string Type { get; }
}

/// <summary>World to hub, first: the protocol version it speaks and its software.</summary>
public sealed record Hello(uint Proto, string Software) : WireMessage
{
    public override string Type => "hello";
}

/// <summary>Hub to world, the answer to `hello`.</summary>
public sealed record Welcome(uint Proto, string World, string Hub, IReadOnlyList<WorldInfo> Worlds,
    IReadOnlyList<string> Rooms, Limits Limits) : WireMessage
{
    public override string Type => "welcome";
}

/// <summary>Hub to world: a federated world's link came up or went down.</summary>
public sealed record WorldChange(WorldInfo Info) : WireMessage
{
    public override string Type => "world";
}

/// <summary>World to hub, about once a minute: the registry heartbeat over the link.</summary>
public sealed record Status(string State, uint? PlayersOnline) : WireMessage
{
    public override string Type => "status";
}

/// <summary>`Owner` lists `Target` as a friend and is online (`On`), or stops.</summary>
public sealed record Watch(Address Owner, Address Target, bool On) : WireMessage
{
    public override string Type => "watch";
}

/// <summary>`Of`'s home world tells watcher `To` what it may see.</summary>
public sealed record Presence(Address To, Address Of, bool Online) : WireMessage
{
    public override string Type => "presence";
}

/// <summary>A tell. `Friend`: the sender lists the recipient as a friend.</summary>
public sealed record Tell(string Id, Address From, Address To, string Text, bool Friend) : WireMessage
{
    public override string Type => "tell";
}

/// <summary>How a tell ended; `Message` is the away text.</summary>
public sealed record TellResult(string Id, Outcome Outcome, string? Message) : WireMessage
{
    public override string Type => "tell.result";
}

/// <summary>World to hub: the world carries a room, or stops.</summary>
public sealed record RoomJoin(string Room, bool On) : WireMessage
{
    public override string Type => "room.join";
}

/// <summary>
/// World to hub: a line one of its characters said in a room. `Via` marks a line its person said
/// elsewhere, which the hub asked the world to say (`discord`).
/// </summary>
public sealed record RoomSay(string Id, string Room, Address From, string Text, string? Via = null) : WireMessage
{
    public override string Type => "room.say";
}

/// <summary>Hub to world: another world's line in a room this world carries, with the mark its world gave it.</summary>
public sealed record RoomLine(string Room, Address From, string Text, string? Via = null) : WireMessage
{
    public override string Type => "room.line";
}

/// <summary>Hub to world: how a `room.say` ended.</summary>
public sealed record RoomResult(string Id, Outcome Outcome) : WireMessage
{
    public override string Type => "room.result";
}

/// <summary>World to hub: a channel request by one of its characters.</summary>
public sealed record ChannelRequest(string Id, ChannelOp Op, string Channel, Address By, Address? Who) : WireMessage
{
    public override string Type => "channel";
}

/// <summary>Hub to world: how a channel request ended.</summary>
public sealed record ChannelResult(string Id, ChannelOutcome Outcome, string Channel, IReadOnlyList<Address> Members,
    IReadOnlyList<string> Channels) : WireMessage
{
    public override string Type => "channel.result";
}

/// <summary>Hub to world: one of the world's characters is invited to a channel.</summary>
public sealed record ChannelInvite(string Channel, Address By, Address To) : WireMessage
{
    public override string Type => "channel.invite";
}

/// <summary>Hub to world: a channel line for one member on the world.</summary>
public sealed record ChannelLine(string Channel, Address From, Address To, string Text) : WireMessage
{
    public override string Type => "channel.line";
}

/// <summary>World to hub: a request about the person layer, for one of the world's characters.</summary>
public sealed record PersonRequest(string Id, PersonOp Op, Address Who, Address? Target, bool? Shown,
    IReadOnlyList<Address>? Targets = null) : WireMessage
{
    public override string Type => "person";

    /// <summary>The characters a `carry` names; none for any other request.</summary>
    public IReadOnlyList<Address> TargetList => Targets ?? [];
}

/// <summary>Hub to world: how a person request ended; a code and its page, or a person's characters.</summary>
public sealed record PersonResult(string Id, PersonOutcome Outcome, string? Code, uint? ExpiresSecs, string? Url,
    IReadOnlyList<Address> Characters) : WireMessage
{
    public override string Type => "person.result";
}

/// <summary>World to hub: `Who` is online and on the network (`On`): send its person view; or stop.</summary>
public sealed record PersonWatch(Address Who, bool On) : WireMessage
{
    public override string Type => "person.watch";
}

/// <summary>
/// Hub to world: `To`'s person view. Its `alts` is always `shown`, for worlds of the network's first
/// version, which read it; this world reads it as any word, or none, and keeps nothing of it.
/// </summary>
public sealed record PersonFriends(Address To, bool Linked, IReadOnlyList<PersonFriend> Persons) : WireMessage
{
    public override string Type => "person.friends";

    /// <summary>The `alts` a person view is written with.</summary>
    public const string EarlierAlts = "shown";
}

/// <summary>Either way: the other side broke the protocol.</summary>
public sealed record ProtocolError(string Code, string Message) : WireMessage
{
    public override string Type => "error";
}

public enum ReadStatus
{
    Ok,
    /// <summary>A `type` this version does not know: skipped, not an error.</summary>
    Unknown,
    /// <summary>Not JSON, not an object, no `type`, too long, or a known type with bad fields.</summary>
    Malformed,
}

public readonly record struct ReadResult(ReadStatus Status, WireMessage? Message, string Why);

public static class Wire
{
    /// <summary>The link protocol's version.</summary>
    public const uint Protocol = 1;

    /// <summary>The longest message, in bytes of JSON, either side accepts.</summary>
    public const int MaxMessageBytes = 16 * 1024;

    /// <summary>The longest id a sender may put on a message.</summary>
    public const int MaxIdChars = 64;

    /// <summary>The one room of the first version: the cross-world Global room.</summary>
    public const string GlobalRoom = "global";

    /// <summary>Every `type` this version knows.</summary>
    public static readonly IReadOnlyList<string> Types =
    [
        "hello", "welcome", "world", "status", "watch", "presence", "tell", "tell.result", "room.join",
        "room.say", "room.line", "room.result", "channel", "channel.result", "channel.invite", "channel.line",
        "person", "person.result", "person.watch", "person.friends", "error",
    ];

    /// <summary>The most person friends that may hold for one character, and a person view names.</summary>
    public const int MaxPersonFriends = 100;

    /// <summary>The most characters one `carry` request names.</summary>
    public const int MaxCarried = 100;

    /// <summary>The most characters one person friend lists, and a `characters` answer names.</summary>
    public const int MaxPersonCharacters = 20;

    /// <summary>The most characters a person view names in all.</summary>
    public const int MaxViewCharacters = 100;

    /// <summary>The longest a link code may last, in seconds, as a result may say.</summary>
    public const uint MaxCodeSecs = 3600;

    /// <summary>The symbols of a link code: Crockford's base 32.</summary>
    public const string CodeAlphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    /// <summary>Whether `s` is a link code as the hub writes one: two groups of four symbols, hyphenated.</summary>
    public static bool IsLinkCode(string s)
    {
        if (s.Length != 9 || s[4] != '-')
            return false;
        for (var i = 0; i < 9; i++)
            if (i != 4 && !CodeAlphabet.Contains(s[i], StringComparison.Ordinal))
                return false;
        return true;
    }

    /// <summary>
    /// Whether `s` may be a result's page address: 1 to 200 printable ASCII characters, none a space, an
    /// angle bracket or a quote, so a world can put it in a line as it is.
    /// </summary>
    public static bool IsPageUrl(string s) =>
        s.Length is >= 1 and <= 200 && s.All(c => c is > ' ' and < (char)0x7F and not ('<' or '>' or '"' or '\''));

    /// <summary>Whether a person view's person friends are within its limits and name no channel.</summary>
    public static bool IsWithinViewLimits(IReadOnlyList<PersonFriend> persons) =>
        persons.Count <= MaxPersonFriends
        && persons.All(p => p.Characters.Count <= MaxPersonCharacters)
        && persons.Sum(p => p.Characters.Count) <= MaxViewCharacters
        && persons.All(p => !p.Handle.IsChannel && p.Characters.All(c => !c.IsChannel));

    /// <summary>Whether `s` is a room name: 2 to 20 of `a-z`, `0-9` and inner hyphens.</summary>
    public static bool IsRoomName(string s)
    {
        if (s.Length is < 2 or > 20)
            return false;
        foreach (var p in s.Split('-'))
        {
            if (p.Length == 0)
                return false;
            foreach (var c in p)
                if (!(c is >= 'a' and <= 'z' || c is >= '0' and <= '9'))
                    return false;
        }
        return true;
    }

    // ---- words ---------------------------------------------------------------------------------

    public static string Word(Outcome o) => o switch
    {
        Outcome.Delivered => "delivered",
        Outcome.Afk => "afk",
        Outcome.Squelched => "squelched",
        Outcome.Offline => "offline",
        Outcome.Unavailable => "unavailable",
        Outcome.NotOnNetwork => "not_on_network",
        Outcome.RateLimited => "rate_limited",
        Outcome.NotMember => "not_member",
        _ => "ok",
    };

    public static string Word(ChannelOp o) => o switch
    {
        ChannelOp.Create => "create",
        ChannelOp.Invite => "invite",
        ChannelOp.Join => "join",
        ChannelOp.Leave => "leave",
        ChannelOp.List => "list",
        _ => "members",
    };

    public static string Word(ChannelOutcome o) => o switch
    {
        ChannelOutcome.Ok => "ok",
        ChannelOutcome.Exists => "exists",
        ChannelOutcome.NoChannel => "no_channel",
        ChannelOutcome.NotMember => "not_member",
        ChannelOutcome.NotInvited => "not_invited",
        ChannelOutcome.AlreadyMember => "already_member",
        _ => "refused",
    };

    public static string Word(PersonOp o) => o switch
    {
        PersonOp.Code => "code",
        PersonOp.Unlink => "unlink",
        PersonOp.Alts => "alts",
        PersonOp.Befriend => "befriend",
        PersonOp.Unfriend => "unfriend",
        PersonOp.Carry => "carry",
        _ => "characters",
    };

    public static string Word(PersonOutcome o) => o switch
    {
        PersonOutcome.Ok => "ok",
        PersonOutcome.NotLinked => "not_linked",
        PersonOutcome.RateLimited => "rate_limited",
        PersonOutcome.Unavailable => "unavailable",
        _ => "refused",
    };

    private static readonly Dictionary<string, PersonOp> PersonOps =
        Enum.GetValues<PersonOp>().ToDictionary(Word, o => o, StringComparer.Ordinal);

    private static readonly Dictionary<string, PersonOutcome> PersonOutcomes =
        Enum.GetValues<PersonOutcome>().ToDictionary(Word, o => o, StringComparer.Ordinal);

    private static readonly Dictionary<string, Outcome> Outcomes =
        Enum.GetValues<Outcome>().ToDictionary(Word, o => o, StringComparer.Ordinal);

    private static readonly Dictionary<string, ChannelOp> ChannelOps =
        Enum.GetValues<ChannelOp>().ToDictionary(Word, o => o, StringComparer.Ordinal);

    private static readonly Dictionary<string, ChannelOutcome> ChannelOutcomes =
        Enum.GetValues<ChannelOutcome>().ToDictionary(Word, o => o, StringComparer.Ordinal);

    // ---- reading -------------------------------------------------------------------------------

    private sealed class Bad(string why) : Exception(why);

    /// <summary>Reads one frame.</summary>
    public static ReadResult Read(string text)
    {
        if (Encoding.UTF8.GetByteCount(text) > MaxMessageBytes)
            return new ReadResult(ReadStatus.Malformed, null, "longer than the limit");
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(text);
        }
        catch (JsonException e)
        {
            return new ReadResult(ReadStatus.Malformed, null, "not JSON: " + e.GetType().Name);
        }
        using (doc)
        {
            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
                return new ReadResult(ReadStatus.Malformed, null, "no type");
            Dictionary<string, JsonElement> f;
            try
            {
                f = Fields(root);
            }
            catch (Exception e) when (e is InvalidOperationException or JsonException)
            {
                return new ReadResult(ReadStatus.Malformed, null, "bad text");
            }
            if (!f.TryGetValue("type", out var t) || t.ValueKind != JsonValueKind.String)
                return new ReadResult(ReadStatus.Malformed, null, "no type");
            string kind;
            try
            {
                kind = t.GetString()!;
            }
            catch (InvalidOperationException)
            {
                return new ReadResult(ReadStatus.Malformed, null, "no type");
            }
            if (!Types.Contains(kind))
                return new ReadResult(ReadStatus.Unknown, null, kind);
            try
            {
                var m = Build(kind, f);
                Check(m);
                return new ReadResult(ReadStatus.Ok, m, "");
            }
            catch (Bad e)
            {
                return new ReadResult(ReadStatus.Malformed, null, kind + ": " + e.Message);
            }
            catch (Exception e) when (e is InvalidOperationException or FormatException or JsonException)
            {
                return new ReadResult(ReadStatus.Malformed, null, kind + ": bad text");
            }
        }
    }

    /// <summary>An object's fields; a name given twice keeps its last value.</summary>
    private static Dictionary<string, JsonElement> Fields(JsonElement o)
    {
        var d = new Dictionary<string, JsonElement>(StringComparer.Ordinal);
        foreach (var p in o.EnumerateObject())
            d[p.Name] = p.Value;
        return d;
    }

    private static JsonElement Need(Dictionary<string, JsonElement> f, string name) =>
        f.TryGetValue(name, out var v) ? v : throw new Bad("missing field " + name);

    private static string Str(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.String ? e.GetString()! : throw new Bad(name + " is not a string");

    private static string Str(Dictionary<string, JsonElement> f, string name) => Str(Need(f, name), name);

    private static uint U32(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Number && e.TryGetUInt32(out var v) ? v : throw new Bad(name + " is not a u32");

    private static double F64(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Number && e.TryGetDouble(out var v) && double.IsFinite(v)
            ? v
            : throw new Bad(name + " is not a number");

    private static bool Bool(JsonElement e, string name) => e.ValueKind switch
    {
        JsonValueKind.True => true,
        JsonValueKind.False => false,
        _ => throw new Bad(name + " is not a bool"),
    };

    private static bool Bool(Dictionary<string, JsonElement> f, string name) => Bool(Need(f, name), name);

    private static Address Addr(JsonElement e, string name) =>
        Address.TryParse(Str(e, name), out var a, out _) ? a : throw new Bad(name + " is not an address");

    private static Address Addr(Dictionary<string, JsonElement> f, string name) => Addr(Need(f, name), name);

    private static bool Absent(Dictionary<string, JsonElement> f, string name) =>
        !f.TryGetValue(name, out var v) || v.ValueKind == JsonValueKind.Null;

    private static List<JsonElement> Arr(JsonElement e, string name)
    {
        if (e.ValueKind != JsonValueKind.Array)
            throw new Bad(name + " is not a list");
        return e.EnumerateArray().ToList();
    }

    private static T Word<T>(Dictionary<string, T> words, JsonElement e, string name) =>
        words.TryGetValue(Str(e, name), out var v) ? v : throw new Bad("unknown " + name);

    private static Rate RateOf(JsonElement e, string name)
    {
        if (e.ValueKind != JsonValueKind.Object)
            throw new Bad(name + " is not a rate");
        var f = Fields(e);
        return new Rate(F64(Need(f, "burst"), "burst"), F64(Need(f, "per_sec"), "per_sec"));
    }

    /// <summary>The states a world's `status` may report.</summary>
    public static readonly string[] States = ["online", "maintenance", "degraded", "offline"];

    private static Limits LimitsOf(JsonElement e)
    {
        if (e.ValueKind != JsonValueKind.Object)
            throw new Bad("limits is not an object");
        var f = Fields(e);
        return new Limits(
            RateOf(Need(f, "tells_per_character"), "tells_per_character"),
            RateOf(Need(f, "room_lines_per_character"), "room_lines_per_character"),
            RateOf(Need(f, "messages_per_world"), "messages_per_world"),
            Absent(f, "room_lines_per_world")
                ? Limits.DefaultRoomLinesPerWorld
                : RateOf(f["room_lines_per_world"], "room_lines_per_world"),
            Absent(f, "tells_per_recipient")
                ? Limits.DefaultTellsPerRecipient
                : RateOf(f["tells_per_recipient"], "tells_per_recipient"),
            U32(Need(f, "pair_burst"), "pair_burst"),
            F64(Need(f, "pair_window_secs"), "pair_window_secs"),
            F64(Need(f, "pair_block_secs"), "pair_block_secs"),
            F64(Need(f, "hold_secs"), "hold_secs"),
            F64(Need(f, "result_timeout_secs"), "result_timeout_secs"));
    }

    private static WorldInfo WorldOf(Dictionary<string, JsonElement> f) =>
        new(Str(f, "slug"), Str(f, "name"), Bool(f, "online"));

    private static WireMessage Build(string kind, Dictionary<string, JsonElement> f)
    {
        switch (kind)
        {
            case "hello":
                return new Hello(U32(Need(f, "proto"), "proto"), Str(f, "software"));
            case "welcome":
            {
                var worlds = Arr(Need(f, "worlds"), "worlds").Select(w =>
                    w.ValueKind == JsonValueKind.Object ? WorldOf(Fields(w)) : throw new Bad("a world is not an object")).ToList();
                var rooms = Arr(Need(f, "rooms"), "rooms").Select(r => Str(r, "room")).ToList();
                return new Welcome(U32(Need(f, "proto"), "proto"), Str(f, "world"), Str(f, "hub"), worlds, rooms,
                    LimitsOf(Need(f, "limits")));
            }
            case "world":
                return new WorldChange(WorldOf(f));
            case "status":
                return new Status(Str(f, "state"),
                    Absent(f, "players_online") ? null : U32(f["players_online"], "players_online"));
            case "watch":
                return new Watch(Addr(f, "owner"), Addr(f, "target"), Bool(f, "on"));
            case "presence":
                return new Presence(Addr(f, "to"), Addr(f, "of"), Bool(f, "online"));
            case "tell":
                return new Tell(Str(f, "id"), Addr(f, "from"), Addr(f, "to"), Str(f, "text"),
                    f.TryGetValue("friend", out var fr) && Bool(fr, "friend"));
            case "tell.result":
                return new TellResult(Str(f, "id"), Word(Outcomes, Need(f, "outcome"), "outcome"),
                    Absent(f, "message") ? null : Str(f["message"], "message"));
            case "room.join":
                return new RoomJoin(Str(f, "room"), Bool(f, "on"));
            case "room.say":
                return new RoomSay(Str(f, "id"), Str(f, "room"), Addr(f, "from"), Str(f, "text"),
                    Absent(f, "via") ? null : Str(f["via"], "via"));
            case "room.line":
                return new RoomLine(Str(f, "room"), Addr(f, "from"), Str(f, "text"),
                    Absent(f, "via") ? null : Str(f["via"], "via"));
            case "room.result":
                return new RoomResult(Str(f, "id"), Word(Outcomes, Need(f, "outcome"), "outcome"));
            case "channel":
                return new ChannelRequest(Str(f, "id"), Word(ChannelOps, Need(f, "op"), "op"), Str(f, "channel"),
                    Addr(f, "by"), Absent(f, "who") ? null : Addr(f["who"], "who"));
            case "channel.result":
            {
                var members = f.TryGetValue("members", out var m)
                    ? Arr(m, "members").Select(x => Addr(x, "member")).ToList()
                    : [];
                var channels = f.TryGetValue("channels", out var c)
                    ? Arr(c, "channels").Select(x => Str(x, "channel")).ToList()
                    : [];
                return new ChannelResult(Str(f, "id"), Word(ChannelOutcomes, Need(f, "outcome"), "outcome"), Str(f, "channel"),
                    members, channels);
            }
            case "channel.invite":
                return new ChannelInvite(Str(f, "channel"), Addr(f, "by"), Addr(f, "to"));
            case "channel.line":
                return new ChannelLine(Str(f, "channel"), Addr(f, "from"), Addr(f, "to"), Str(f, "text"));
            case "person":
                return new PersonRequest(Str(f, "id"), Word(PersonOps, Need(f, "op"), "op"), Addr(f, "who"),
                    Absent(f, "target") ? null : Addr(f["target"], "target"),
                    Absent(f, "shown") ? null : Bool(f["shown"], "shown"),
                    f.TryGetValue("targets", out var pt) ? Arr(pt, "targets").Select(x => Addr(x, "target")).ToList() : null);
            case "person.result":
                return new PersonResult(Str(f, "id"), Word(PersonOutcomes, Need(f, "outcome"), "outcome"),
                    Absent(f, "code") ? null : Str(f["code"], "code"),
                    Absent(f, "expires_secs") ? null : U32(f["expires_secs"], "expires_secs"),
                    Absent(f, "url") ? null : Str(f["url"], "url"),
                    f.TryGetValue("characters", out var pc) ? Arr(pc, "characters").Select(x => Addr(x, "character")).ToList() : []);
            case "person.watch":
                return new PersonWatch(Addr(f, "who"), Bool(f, "on"));
            case "person.friends":
            {
                var persons = f.TryGetValue("persons", out var ps)
                    ? Arr(ps, "persons").Select(p =>
                    {
                        if (p.ValueKind != JsonValueKind.Object)
                            throw new Bad("a person friend is not an object");
                        var pf = Fields(p);
                        return new PersonFriend(Addr(pf, "handle"),
                            Arr(Need(pf, "characters"), "characters").Select(x => Addr(x, "character")).ToList());
                    }).ToList()
                    : [];
                if (f.TryGetValue("alts", out var alts) && alts.ValueKind != JsonValueKind.String)
                    throw new Bad("alts is not a word");
                return new PersonFriends(Addr(f, "to"), Bool(f, "linked"), persons);
            }
            case "error":
                return new ProtocolError(Str(f, "code"), Str(f, "message"));
            default:
                throw new Bad("unknown type");
        }
    }

    /// <summary>
    /// The range checks the field types do not make: ids, text lengths (an away text's as a tell's),
    /// room and channel names (a channel result's too, since a world shows them to a player).
    /// </summary>
    private static void Check(WireMessage m)
    {
        static bool IdOk(string id) => id.Length > 0 && Text.CharCount(id) <= MaxIdChars;
        // Text is cut by the receiving world's text rules; the wire refuses only what no rule would
        // ever keep.
        static bool TextOk(string t) => Text.CharCount(t) <= TextRules.MaxTextChars;
        switch (m)
        {
            case Status st:
                if (!States.Contains(st.State)) throw new Bad("state");
                break;
            case Tell t:
                if (!IdOk(t.Id)) throw new Bad("id");
                if (!TextOk(t.Text)) throw new Bad("text too long");
                break;
            case RoomSay r:
                if (!IdOk(r.Id)) throw new Bad("id");
                if (!TextOk(r.Text)) throw new Bad("text too long");
                if (!IsRoomName(r.Room)) throw new Bad("room");
                if (r.Via is not null && !SayWire.IsVia(r.Via)) throw new Bad("via");
                break;
            case TellResult r:
                if (!IdOk(r.Id)) throw new Bad("id");
                if (r.Message is not null && !TextOk(r.Message)) throw new Bad("text too long");
                break;
            case RoomResult r:
                if (!IdOk(r.Id)) throw new Bad("id");
                break;
            case ChannelRequest c:
                if (!IdOk(c.Id)) throw new Bad("id");
                if (!Address.IsChannelName(c.Channel)) throw new Bad("channel");
                break;
            case ChannelResult c:
                if (!IdOk(c.Id)) throw new Bad("id");
                if (!Address.IsChannelName(c.Channel) || !c.Channels.All(Address.IsChannelName)) throw new Bad("channel");
                break;
            case RoomLine r:
                if (!TextOk(r.Text)) throw new Bad("text too long");
                if (!IsRoomName(r.Room)) throw new Bad("room");
                if (r.Via is not null && !SayWire.IsVia(r.Via)) throw new Bad("via");
                break;
            case ChannelLine c:
                if (!TextOk(c.Text)) throw new Bad("text too long");
                if (!Address.IsChannelName(c.Channel)) throw new Bad("channel");
                break;
            case RoomJoin r:
                if (!IsRoomName(r.Room)) throw new Bad("room");
                break;
            case ChannelInvite c:
                if (!Address.IsChannelName(c.Channel)) throw new Bad("channel");
                break;
            case PersonRequest p:
                if (!IdOk(p.Id)) throw new Bad("id");
                if (p.Who.IsChannel || p.Target?.IsChannel == true || p.TargetList.Any(t => t.IsChannel))
                    throw new Bad("person: a channel is not a character");
                if (p.Op is PersonOp.Befriend or PersonOp.Unfriend or PersonOp.Characters && p.Target is null)
                    throw new Bad("person: no target");
                if (p.Op == PersonOp.Carry && p.TargetList.Count is < 1 or > MaxCarried)
                    throw new Bad("person: carry without targets, or too many");
                if (p.Op == PersonOp.Alts && p.Shown is null) throw new Bad("person: alts without shown");
                break;
            case PersonResult r:
                if (!IdOk(r.Id)) throw new Bad("id");
                if (r.Code is not null && !IsLinkCode(r.Code)) throw new Bad("person.result: code");
                if (r.ExpiresSecs > MaxCodeSecs) throw new Bad("person.result: expires_secs");
                if (r.Url is not null && !IsPageUrl(r.Url)) throw new Bad("person.result: url");
                if (r.Characters.Count > MaxPersonCharacters || r.Characters.Any(c => c.IsChannel))
                    throw new Bad("person.result: characters");
                break;
            case PersonWatch w:
                if (w.Who.IsChannel) throw new Bad("person.watch: a channel is not a character");
                break;
            case PersonFriends v:
                if (v.To.IsChannel || !IsWithinViewLimits(v.Persons)) throw new Bad("person.friends: persons");
                break;
        }
    }

    // ---- writing -------------------------------------------------------------------------------

    /// <summary>Writes one frame.</summary>
    public static string Write(WireMessage m)
    {
        var o = new JsonObj();
        o.Str("type", m.Type);
        switch (m)
        {
            case Hello h:
                o.U32("proto", h.Proto).Str("software", h.Software);
                break;
            case Welcome w:
                o.U32("proto", w.Proto).Str("world", w.World).Str("hub", w.Hub)
                    .Raw("worlds", Json.Array(w.Worlds.Select(WorldJson)))
                    .Raw("rooms", Json.Array(w.Rooms.Select(Json.Str)))
                    .Raw("limits", LimitsJson(w.Limits));
                break;
            case WorldChange c:
                o.Str("slug", c.Info.Slug).Str("name", c.Info.Name).Bool("online", c.Info.Online);
                break;
            case Status s:
                o.Str("state", s.State);
                if (s.PlayersOnline is uint n)
                    o.U32("players_online", n);
                break;
            case Watch w:
                o.Str("owner", w.Owner.ToString()).Str("target", w.Target.ToString()).Bool("on", w.On);
                break;
            case Presence p:
                o.Str("to", p.To.ToString()).Str("of", p.Of.ToString()).Bool("online", p.Online);
                break;
            case Tell t:
                o.Str("id", t.Id).Str("from", t.From.ToString()).Str("to", t.To.ToString()).Str("text", t.Text)
                    .Bool("friend", t.Friend);
                break;
            case TellResult r:
                o.Str("id", r.Id).Str("outcome", Word(r.Outcome));
                if (r.Message is not null)
                    o.Str("message", r.Message);
                break;
            case RoomJoin r:
                o.Str("room", r.Room).Bool("on", r.On);
                break;
            case RoomSay r:
                o.Str("id", r.Id).Str("room", r.Room).Str("from", r.From.ToString()).Str("text", r.Text);
                if (r.Via is not null)
                    o.Str("via", r.Via);
                break;
            case RoomLine r:
                o.Str("room", r.Room).Str("from", r.From.ToString()).Str("text", r.Text);
                if (r.Via is not null)
                    o.Str("via", r.Via);
                break;
            case RoomResult r:
                o.Str("id", r.Id).Str("outcome", Word(r.Outcome));
                break;
            case ChannelRequest c:
                o.Str("id", c.Id).Str("op", Word(c.Op)).Str("channel", c.Channel).Str("by", c.By.ToString());
                if (c.Who is not null)
                    o.Str("who", c.Who.ToString());
                break;
            case ChannelResult c:
                o.Str("id", c.Id).Str("outcome", Word(c.Outcome)).Str("channel", c.Channel);
                if (c.Members.Count > 0)
                    o.Raw("members", Json.Array(c.Members.Select(a => Json.Str(a.ToString()))));
                if (c.Channels.Count > 0)
                    o.Raw("channels", Json.Array(c.Channels.Select(Json.Str)));
                break;
            case ChannelInvite c:
                o.Str("channel", c.Channel).Str("by", c.By.ToString()).Str("to", c.To.ToString());
                break;
            case ChannelLine c:
                o.Str("channel", c.Channel).Str("from", c.From.ToString()).Str("to", c.To.ToString()).Str("text", c.Text);
                break;
            case PersonRequest p:
                o.Str("id", p.Id).Str("op", Word(p.Op)).Str("who", p.Who.ToString());
                if (p.Target is not null)
                    o.Str("target", p.Target.ToString());
                if (p.TargetList.Count > 0)
                    o.Raw("targets", Json.Array(p.TargetList.Select(a => Json.Str(a.ToString()))));
                if (p.Shown is bool shown)
                    o.Bool("shown", shown);
                break;
            case PersonResult r:
                o.Str("id", r.Id).Str("outcome", Word(r.Outcome));
                if (r.Code is not null)
                    o.Str("code", r.Code);
                if (r.ExpiresSecs is uint secs)
                    o.U32("expires_secs", secs);
                if (r.Url is not null)
                    o.Str("url", r.Url);
                if (r.Characters.Count > 0)
                    o.Raw("characters", Json.Array(r.Characters.Select(a => Json.Str(a.ToString()))));
                break;
            case PersonWatch w:
                o.Str("who", w.Who.ToString()).Bool("on", w.On);
                break;
            case PersonFriends v:
                o.Str("to", v.To.ToString()).Bool("linked", v.Linked).Str("alts", PersonFriends.EarlierAlts)
                    .Raw("persons", Json.Array(v.Persons.Select(p => new JsonObj().Str("handle", p.Handle.ToString())
                        .Raw("characters", Json.Array(p.Characters.Select(a => Json.Str(a.ToString())))).ToString())));
                break;
            case ProtocolError e:
                o.Str("code", e.Code).Str("message", e.Message);
                break;
        }
        return o.ToString();
    }

    private static string WorldJson(WorldInfo w) =>
        new JsonObj().Str("slug", w.Slug).Str("name", w.Name).Bool("online", w.Online).ToString();

    private static string RateJson(Rate r) => new JsonObj().F64("burst", r.Burst).F64("per_sec", r.PerSec).ToString();

    private static string LimitsJson(Limits l) => new JsonObj()
        .Raw("tells_per_character", RateJson(l.TellsPerCharacter))
        .Raw("room_lines_per_character", RateJson(l.RoomLinesPerCharacter))
        .Raw("messages_per_world", RateJson(l.MessagesPerWorld))
        .Raw("room_lines_per_world", RateJson(l.RoomLinesPerWorld))
        .Raw("tells_per_recipient", RateJson(l.TellsPerRecipient))
        .U32("pair_burst", l.PairBurst)
        .F64("pair_window_secs", l.PairWindowSecs)
        .F64("pair_block_secs", l.PairBlockSecs)
        .F64("hold_secs", l.HoldSecs)
        .F64("result_timeout_secs", l.ResultTimeoutSecs)
        .ToString();
}

/// <summary>JSON text in the shape the protocol's reference writes it: compact, UTF-8 left as is.</summary>
public static class Json
{
    public static string Str(string s)
    {
        var b = new StringBuilder(s.Length + 2);
        b.Append('"');
        foreach (var c in s)
        {
            switch (c)
            {
                case '"': b.Append("\\\""); break;
                case '\\': b.Append("\\\\"); break;
                case '\b': b.Append("\\b"); break;
                case '\f': b.Append("\\f"); break;
                case '\n': b.Append("\\n"); break;
                case '\r': b.Append("\\r"); break;
                case '\t': b.Append("\\t"); break;
                default:
                    if (c < 0x20)
                        b.Append("\\u00").Append(((int)c).ToString("x2", CultureInfo.InvariantCulture));
                    else
                        b.Append(c);
                    break;
            }
        }
        b.Append('"');
        return b.ToString();
    }

    public static string F64(double v)
    {
        if (double.IsInteger(v) && Math.Abs(v) < 1e16)
            return (v == 0 && double.IsNegative(v) ? "-0" : v.ToString("F0", CultureInfo.InvariantCulture)) + ".0";
        var r = v.ToString("R", CultureInfo.InvariantCulture);
        var e = r.IndexOf('E');
        if (e < 0)
            return r;
        var exp = int.Parse(r[(e + 1)..], CultureInfo.InvariantCulture);
        return r[..e] + "e" + exp.ToString(CultureInfo.InvariantCulture);
    }

    public static string Array(IEnumerable<string> raws) => "[" + string.Join(",", raws) + "]";
}

internal sealed class JsonObj
{
    private readonly StringBuilder b = new("{");
    private bool any;

    private JsonObj Key(string k)
    {
        if (any)
            b.Append(',');
        any = true;
        b.Append(Json.Str(k)).Append(':');
        return this;
    }

    public JsonObj Str(string k, string v)
    {
        Key(k).b.Append(Json.Str(v));
        return this;
    }

    public JsonObj U32(string k, uint v)
    {
        Key(k).b.Append(v.ToString(CultureInfo.InvariantCulture));
        return this;
    }

    public JsonObj U64(string k, ulong v)
    {
        Key(k).b.Append(v.ToString(CultureInfo.InvariantCulture));
        return this;
    }

    public JsonObj F64(string k, double v)
    {
        Key(k).b.Append(Json.F64(v));
        return this;
    }

    public JsonObj Bool(string k, bool v)
    {
        Key(k).b.Append(v ? "true" : "false");
        return this;
    }

    public JsonObj Raw(string k, string raw)
    {
        Key(k).b.Append(raw);
        return this;
    }

    public override string ToString() => b + "}";
}
