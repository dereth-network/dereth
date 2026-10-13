// The character directory's words: a listed character's profile and the bounds of each of its
// fields, the three messages a world and the hub exchange about listings, and the six fields a
// character's own "Allow others to see your ..." options keep out of its profile.
//
// The directory's messages are a family of their own: DirectoryWire reads them, and Wire skips
// them as types it does not know, as every reader of a link skips a type it does not know. The JSON
// this writes is the protocol's own shape: the fields in the order the specification lists them,
// an absent field left out.

using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Dereth.Network;

/// <summary>How far a skill is trained.</summary>
public enum Training
{
    Untrained,
    Trained,
    Specialized,
}

/// <summary>One attribute: the value the character was made with, and its value with the ranks raised since, unbuffed.</summary>
public readonly record struct AttributeValue(uint Innate, uint Base);

/// <summary>The six attributes.</summary>
public sealed record Attributes(AttributeValue Strength, AttributeValue Endurance, AttributeValue Coordination,
    AttributeValue Quickness, AttributeValue Focus, AttributeValue Self);

/// <summary>The three vitals' maximums, unbuffed.</summary>
public sealed record Vitals(uint Health, uint Stamina, uint Mana);

/// <summary>One skill: its number, as the game numbers its skills, how far it is trained, and its level unbuffed.</summary>
public readonly record struct ProfileSkill(uint Id, Training Training, uint Base);

/// <summary>One augmentation: the number of the integer property the game keeps it in, and how many times taken.</summary>
public readonly record struct ProfileAugmentation(uint Id, uint Times);

/// <summary>
/// The character's allegiance, as far as the directory shows it: its opaque key, its name only when
/// its monarch gave it one, how many characters it holds, the character's rank, and its patron and
/// monarch only while each is listed too.
/// </summary>
public sealed record ProfileAllegiance(string Key, string? Name, uint Members, uint Rank, Address? Patron, Address? Monarch);

/// <summary>
/// The final retail client's "Allow others to see your ..." character options, as a character has
/// them set. Each lets others see one of its fields when they appraise it, and the directory follows
/// them: a field its option keeps hidden is never in its profile.
/// </summary>
public readonly record struct OthersSee(bool DateOfBirth, bool Age, bool ChessRank, bool FishingSkill, bool NumberOfDeaths,
    bool NumberOfTitles);

/// <summary>The six fields <see cref="OthersSee"/> governs, as the world keeps them for a character.</summary>
public readonly record struct Seeable(ulong? Birth, ulong? Age, uint? Chess, uint? Fishing, uint? Deaths, uint? Titles);

/// <summary>
/// A listed character's profile, as its world sends it and the directory publishes it. Every field
/// but the first three may be left out.
/// </summary>
public sealed record Profile
{
    public required uint Level { get; init; }
    /// <summary>The heritage group, 1 to <see cref="DirectoryWire.MaxHeritage"/>.</summary>
    public required uint Heritage { get; init; }
    /// <summary>1, male, or 2, female.</summary>
    public required uint Gender { get; init; }
    /// <summary>The title the character shows.</summary>
    public string? Title { get; init; }
    /// <summary>Experience earned in all, and not yet spent.</summary>
    public (ulong Total, ulong Unassigned)? Experience { get; init; }
    /// <summary>Skill credits not yet spent.</summary>
    public uint? Credits { get; init; }
    /// <summary>Luminance to spend, and the most the character may hold.</summary>
    public (ulong Available, ulong Maximum)? Luminance { get; init; }
    public Attributes? Attributes { get; init; }
    public Vitals? Vitals { get; init; }
    public IReadOnlyList<ProfileSkill> Skills { get; init; } = [];
    public IReadOnlyList<ProfileAugmentation> Augmentations { get; init; } = [];
    public ProfileAllegiance? Allegiance { get; init; }
    /// <summary>When the character was made, in seconds since 1970: only while others may see its date of birth.</summary>
    public ulong? Birth { get; init; }
    /// <summary>How long it has been played, in seconds: only while others may see its age.</summary>
    public ulong? Age { get; init; }
    /// <summary>How many times it has died: only while others may see its number of deaths.</summary>
    public uint? Deaths { get; init; }
    /// <summary>Its chess rank: only while others may see it.</summary>
    public uint? Chess { get; init; }
    /// <summary>Its fishing skill: only while others may see it.</summary>
    public uint? Fishing { get; init; }
    /// <summary>How many titles it has earned: only while others may see its number of titles.</summary>
    public uint? Titles { get; init; }

    /// <summary>
    /// The profile with the six fields a character's options govern: each its value where `allowed`
    /// lets others see it, and out of the profile where it does not.
    /// </summary>
    public Profile WithSeeable(Seeable values, OthersSee allowed) => this with
    {
        Birth = allowed.DateOfBirth ? values.Birth : null,
        Age = allowed.Age ? values.Age : null,
        Chess = allowed.ChessRank ? values.Chess : null,
        Fishing = allowed.FishingSkill ? values.Fishing : null,
        Deaths = allowed.NumberOfDeaths ? values.Deaths : null,
        Titles = allowed.NumberOfTitles ? values.Titles : null,
    };

    /// <summary>The six fields as the profile carries them.</summary>
    public Seeable Seeable => new(Birth, Age, Chess, Fishing, Deaths, Titles);

    /// <summary>The profile as its JSON, the form two profiles are compared in.</summary>
    public string Json() => DirectoryWire.ProfileJson(this);

    /// <summary>
    /// The bounds of every field, for a profile of `who`: why it is out of them, or null when it is
    /// within them.
    /// </summary>
    public string? Check(Address who)
    {
        if (Level is < 1 or > DirectoryWire.MaxLevel)
            return "level";
        if (Heritage is < 1 or > DirectoryWire.MaxHeritage)
            return "heritage";
        if (Gender is < 1 or > 2)
            return "gender";
        if (Title is not null && !DirectoryWire.IsShownText(Title, DirectoryWire.MaxTitleChars))
            return "title";
        if (Experience is { } e && (e.Total > DirectoryWire.MaxAmount || e.Unassigned > DirectoryWire.MaxAmount))
            return "experience";
        if (Luminance is { } l && (l.Available > DirectoryWire.MaxAmount || l.Maximum > DirectoryWire.MaxAmount))
            return "luminance";
        if (Attributes is { } a && new[] { a.Strength, a.Endurance, a.Coordination, a.Quickness, a.Focus, a.Self }
                .Any(x => x.Innate > DirectoryWire.MaxStat || x.Base > DirectoryWire.MaxStat))
            return "attributes";
        if (Vitals is { } v && new[] { v.Health, v.Stamina, v.Mana }.Any(x => x > DirectoryWire.MaxStat))
            return "vitals";
        if (Skills.Count > DirectoryWire.MaxSkills
            || Skills.Any(s => s.Id is < 1 or > DirectoryWire.MaxSkillId || s.Base > DirectoryWire.MaxStat)
            || Skills.Select(s => s.Id).Distinct().Count() != Skills.Count)
            return "skills";
        if (Augmentations.Count > DirectoryWire.MaxAugmentations
            || Augmentations.Any(x => x.Id is < 1 or > DirectoryWire.MaxAugmentationId
                || x.Times is < 1 or > DirectoryWire.MaxAugmentationTimes)
            || Augmentations.Select(x => x.Id).Distinct().Count() != Augmentations.Count)
            return "augmentations";
        if (Allegiance is { } al)
        {
            if (!DirectoryWire.IsAllegianceKey(al.Key))
                return "allegiance key";
            if (al.Name is not null && !DirectoryWire.IsShownText(al.Name, DirectoryWire.MaxAllegianceNameChars))
                return "allegiance name";
            if (al.Members is < 1 or > DirectoryWire.MaxMembers || al.Rank is < 1 or > DirectoryWire.MaxRank)
                return "allegiance size or rank";
            // Allegiances never span worlds, and a character is neither its own patron nor its own monarch.
            bool OtherMember(Address? x) => x is null || (!x.IsChannel && x.World == who.World && !x.Same(who));
            if (!OtherMember(al.Patron) || !OtherMember(al.Monarch))
                return "allegiance patron or monarch";
        }
        if (Birth > DirectoryWire.MaxAmount || Age > DirectoryWire.MaxAmount)
            return "birth or age";
        if (new[] { Credits, Deaths, Chess, Fishing, Titles }.Any(n => n > DirectoryWire.MaxCount))
            return "a count";
        return null;
    }
}

/// <summary>How a listing ended.</summary>
public enum ListingOutcome
{
    /// <summary>Done: the profile is published as sent, or the character is out of the directory.</summary>
    Ok,
    /// <summary>The hub will not list this character now; the reason says why.</summary>
    Refused,
    /// <summary>Over the hub's pace for listings: the world sends it again later.</summary>
    RateLimited,
    /// <summary>This hub keeps no directory.</summary>
    Unavailable,
}

/// <summary>Why the hub refused a listing.</summary>
public enum Refusal
{
    /// <summary>The hub's operator took the character out of the directory, for a while.</summary>
    TakenDown,
    /// <summary>The directory holds as many of the world's characters as it may.</summary>
    Full,
}

/// <summary>Who took a character out of the directory.</summary>
public enum OffBy
{
    /// <summary>
    /// The character's person, forgetting itself on the site: its world takes the character off the
    /// network, as `/dn off` does.
    /// </summary>
    Person,
    /// <summary>The hub's operator: its world holds the character out until its next login, on the network still.</summary>
    Operator,
}

public abstract record DirectoryMessage
{
    /// <summary>The message's `type`.</summary>
    public abstract string Type { get; }
}

/// <summary>World to hub: `Who` is listed, with its profile as it is now (`On`), or it is listed no longer.</summary>
public sealed record Listing(string Id, Address Who, bool On, Profile? Profile) : DirectoryMessage
{
    public override string Type => "listing";
}

/// <summary>Hub to world: how a listing ended, and the character's page once it is listed.</summary>
public sealed record ListingResult(string Id, ListingOutcome Outcome, Refusal? Reason, string? Url) : DirectoryMessage
{
    public override string Type => "listing.result";
}

/// <summary>
/// Hub to world: `Who` was taken out of the directory, so its world sends no listing of it until its
/// next login (taken off the network, by its person).
/// </summary>
public sealed record ListingOff(Address Who, OffBy By) : DirectoryMessage
{
    public override string Type => "listing.off";
}

public readonly record struct DirectoryReadResult(ReadStatus Status, DirectoryMessage? Message, string Why);

public static class DirectoryWire
{
    public const uint MaxLevel = 100_000;
    public const uint MaxHeritage = 13;
    public const int MaxTitleChars = 64;
    public const int MaxAllegianceNameChars = 40;
    public const uint MaxStat = 1_000_000;
    public const int MaxSkills = 64;
    public const uint MaxSkillId = 255;
    public const int MaxAugmentations = 64;
    public const uint MaxAugmentationId = 65_535;
    public const uint MaxAugmentationTimes = 1_000;

    /// <summary>The largest amount: what a page's script reads exactly.</summary>
    public const ulong MaxAmount = (1UL << 53) - 1;

    public const uint MaxCount = 1_000_000;
    public const uint MaxMembers = 100_000;
    public const uint MaxRank = 100;
    public const int MinAllegianceKeyChars = 8;
    public const int MaxAllegianceKeyChars = 64;

    /// <summary>
    /// The least time between two listings a world sends for one character, in seconds, but for one
    /// at its logout and one that takes it out, which go at once.
    /// </summary>
    public const double ListingEverySecs = 300;

    /// <summary>Listings of one character the hub takes; one that takes the character out is not held to it.</summary>
    public static readonly Rate ListingsPerCharacter = new(5.0, 1.0 / 60.0);

    /// <summary>Listings of one world's characters the hub takes, all of them together.</summary>
    public static readonly Rate ListingsPerWorld = new(50.0, 10.0);

    /// <summary>Every `type` of the directory's messages.</summary>
    public static readonly IReadOnlyList<string> Types = ["listing", "listing.result", "listing.off"];

    /// <summary>Whether `s` is an allegiance key: 8 to 64 of `A-Z`, `a-z`, `0-9`, `-` and `_`.</summary>
    public static bool IsAllegianceKey(string s) =>
        s.Length is >= MinAllegianceKeyChars and <= MaxAllegianceKeyChars && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_');

    /// <summary>Whether `c` is a character the hub refuses in any text it keeps or publishes.</summary>
    public static bool IsHidden(int c) => c is <= 0x1F or (>= 0x7F and <= 0x9F) or 0x061C or 0x180E or (>= 0x200B and <= 0x200F)
        or (>= 0x2028 and <= 0x202E) or (>= 0x2060 and <= 0x2064) or (>= 0x2066 and <= 0x2069) or 0xFEFF
        or (>= 0xFFF9 and <= 0xFFFB) or 0xFFFE or 0xFFFF;

    /// <summary>
    /// Whether `s` may be shown as a title or an allegiance name: 1 to `max` characters, no white
    /// space at either end, no hidden character and no angle bracket. Text that breaks the rule is
    /// refused, never cut or cleaned.
    /// </summary>
    public static bool IsShownText(string s, int max)
    {
        var n = Text.CharCount(s);
        if (n < 1 || n > max || s.Trim() != s)
            return false;
        for (var i = 0; i < s.Length; i++)
        {
            int c = s[i];
            if (char.IsHighSurrogate(s[i]) && i + 1 < s.Length && char.IsLowSurrogate(s[i + 1]))
            {
                c = char.ConvertToUtf32(s[i], s[i + 1]);
                i++;
            }
            if (IsHidden(c) || c is '<' or '>')
                return false;
        }
        return true;
    }

    public static string Word(Training t) => t switch
    {
        Training.Untrained => "untrained",
        Training.Trained => "trained",
        _ => "specialized",
    };

    public static string Word(ListingOutcome o) => o switch
    {
        ListingOutcome.Ok => "ok",
        ListingOutcome.Refused => "refused",
        ListingOutcome.RateLimited => "rate_limited",
        _ => "unavailable",
    };

    public static string Word(Refusal r) => r == Refusal.TakenDown ? "taken_down" : "full";

    public static string Word(OffBy b) => b == OffBy.Person ? "person" : "operator";

    private static readonly Dictionary<string, Training> Trainings =
        Enum.GetValues<Training>().ToDictionary(Word, x => x, StringComparer.Ordinal);

    private static readonly Dictionary<string, ListingOutcome> Outcomes =
        Enum.GetValues<ListingOutcome>().ToDictionary(Word, x => x, StringComparer.Ordinal);

    private static readonly Dictionary<string, Refusal> Refusals =
        Enum.GetValues<Refusal>().ToDictionary(Word, x => x, StringComparer.Ordinal);

    private static readonly Dictionary<string, OffBy> Offs =
        Enum.GetValues<OffBy>().ToDictionary(Word, x => x, StringComparer.Ordinal);

    // ---- reading -------------------------------------------------------------------------------

    private sealed class Bad(string why) : Exception(why);

    /// <summary>
    /// Reads one frame: one of the directory's messages, or `Unknown` for any other type (the link's
    /// own messages among them), or `Malformed`.
    /// </summary>
    public static DirectoryReadResult Read(string text)
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
                var f = Fields(root);
                if (!f.TryGetValue("type", out var t) || t.ValueKind != JsonValueKind.String)
                    return new(ReadStatus.Malformed, null, "no type");
                var kind = t.GetString()!;
                if (!Types.Contains(kind))
                    return new(ReadStatus.Unknown, null, kind);
                var m = Build(kind, f);
                return new(ReadStatus.Ok, m, "");
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

    /// <summary>A profile from its JSON, as a world keeps it; null when it does not read.</summary>
    public static Profile? ReadProfile(string json)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            return ProfileOf(doc.RootElement);
        }
        catch (Exception e) when (e is Bad or JsonException or InvalidOperationException or FormatException)
        {
            return null;
        }
    }

    private static Dictionary<string, JsonElement> Fields(JsonElement o)
    {
        var d = new Dictionary<string, JsonElement>(StringComparer.Ordinal);
        foreach (var p in o.EnumerateObject())
            d[p.Name] = p.Value;
        return d;
    }

    private static Dictionary<string, JsonElement> Obj(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Object ? Fields(e) : throw new Bad(name + " is not an object");

    private static JsonElement Need(Dictionary<string, JsonElement> f, string name) =>
        f.TryGetValue(name, out var v) ? v : throw new Bad("missing field " + name);

    private static bool Absent(Dictionary<string, JsonElement> f, string name) =>
        !f.TryGetValue(name, out var v) || v.ValueKind == JsonValueKind.Null;

    private static string Str(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.String ? e.GetString()! : throw new Bad(name + " is not a string");

    private static uint U32(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Number && e.TryGetUInt32(out var v) ? v : throw new Bad(name + " is not a u32");

    private static ulong U64(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Number && e.TryGetUInt64(out var v) ? v : throw new Bad(name + " is not a u64");

    private static byte U8(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Number && e.TryGetByte(out var v) ? v : throw new Bad(name + " is not a u8");

    private static bool Bool(JsonElement e, string name) => e.ValueKind switch
    {
        JsonValueKind.True => true,
        JsonValueKind.False => false,
        _ => throw new Bad(name + " is not a bool"),
    };

    private static Address Addr(JsonElement e, string name) =>
        Address.TryParse(Str(e, name), out var a, out _) ? a : throw new Bad(name + " is not an address");

    private static T Word<T>(Dictionary<string, T> words, JsonElement e, string name) =>
        words.TryGetValue(Str(e, name), out var v) ? v : throw new Bad("unknown " + name);

    private static List<JsonElement> Arr(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Array ? e.EnumerateArray().ToList() : throw new Bad(name + " is not a list");

    private static DirectoryMessage Build(string kind, Dictionary<string, JsonElement> f)
    {
        static bool IdOk(string id) => id.Length > 0 && Text.CharCount(id) <= Wire.MaxIdChars;
        switch (kind)
        {
            case "listing":
            {
                var id = Str(Need(f, "id"), "id");
                if (!IdOk(id))
                    throw new Bad("id");
                var who = Addr(Need(f, "who"), "who");
                if (who.IsChannel)
                    throw new Bad("a channel is not a character");
                var on = Bool(Need(f, "on"), "on");
                var profile = Absent(f, "profile") ? null : ProfileOf(f["profile"]);
                switch (on, profile)
                {
                    case (true, null):
                        throw new Bad("a listing that is on carries the profile");
                    case (false, not null):
                        throw new Bad("a listing that is off carries no profile");
                    case (true, { } p) when p.Check(who) is { } why:
                        throw new Bad(why);
                }
                return new Listing(id, who, on, profile);
            }
            case "listing.result":
            {
                var id = Str(Need(f, "id"), "id");
                if (!IdOk(id))
                    throw new Bad("id");
                var outcome = Word(Outcomes, Need(f, "outcome"), "outcome");
                Refusal? reason = Absent(f, "reason") ? null : Word(Refusals, f["reason"], "reason");
                var url = Absent(f, "url") ? null : Str(f["url"], "url");
                if (reason is not null && outcome != ListingOutcome.Refused)
                    throw new Bad("a reason is given for a refusal only");
                if (url is not null && !Wire.IsPageUrl(url))
                    throw new Bad("url");
                return new ListingResult(id, outcome, reason, url);
            }
            default:
            {
                var who = Addr(Need(f, "who"), "who");
                if (who.IsChannel)
                    throw new Bad("a channel is not a character");
                return new ListingOff(who, Word(Offs, Need(f, "by"), "by"));
            }
        }
    }

    private static AttributeValue AttributeOf(Dictionary<string, JsonElement> f, string name)
    {
        var a = Obj(Need(f, name), name);
        return new AttributeValue(U32(Need(a, "innate"), "innate"), U32(Need(a, "base"), "base"));
    }

    private static uint VitalOf(Dictionary<string, JsonElement> f, string name) =>
        U32(Need(Obj(Need(f, name), name), "base"), "base");

    /// <summary>A profile, read to its type: unknown fields are left out, and the bounds are the caller's to check.</summary>
    private static Profile ProfileOf(JsonElement e)
    {
        var f = Obj(e, "profile");
        var p = new Profile
        {
            Level = U32(Need(f, "level"), "level"),
            Heritage = U8(Need(f, "heritage"), "heritage"),
            Gender = U8(Need(f, "gender"), "gender"),
            Title = Absent(f, "title") ? null : Str(f["title"], "title"),
            Credits = Absent(f, "credits") ? null : U32(f["credits"], "credits"),
            Birth = Absent(f, "birth") ? null : U64(f["birth"], "birth"),
            Age = Absent(f, "age") ? null : U64(f["age"], "age"),
            Deaths = Absent(f, "deaths") ? null : U32(f["deaths"], "deaths"),
            Chess = Absent(f, "chess") ? null : U32(f["chess"], "chess"),
            Fishing = Absent(f, "fishing") ? null : U32(f["fishing"], "fishing"),
            Titles = Absent(f, "titles") ? null : U32(f["titles"], "titles"),
        };
        if (!Absent(f, "experience"))
        {
            var x = Obj(f["experience"], "experience");
            p = p with { Experience = (U64(Need(x, "total"), "total"), U64(Need(x, "unassigned"), "unassigned")) };
        }
        if (!Absent(f, "luminance"))
        {
            var x = Obj(f["luminance"], "luminance");
            p = p with { Luminance = (U64(Need(x, "available"), "available"), U64(Need(x, "maximum"), "maximum")) };
        }
        if (!Absent(f, "attributes"))
        {
            var a = Obj(f["attributes"], "attributes");
            p = p with
            {
                Attributes = new Attributes(AttributeOf(a, "strength"), AttributeOf(a, "endurance"), AttributeOf(a, "coordination"),
                    AttributeOf(a, "quickness"), AttributeOf(a, "focus"), AttributeOf(a, "self")),
            };
        }
        if (!Absent(f, "vitals"))
        {
            var v = Obj(f["vitals"], "vitals");
            p = p with { Vitals = new Vitals(VitalOf(v, "health"), VitalOf(v, "stamina"), VitalOf(v, "mana")) };
        }
        if (f.TryGetValue("skills", out var sk))
            p = p with
            {
                Skills = Arr(sk, "skills").Select(x =>
                {
                    var s = Obj(x, "skill");
                    return new ProfileSkill(U32(Need(s, "id"), "id"), Word(Trainings, Need(s, "training"), "training"),
                        U32(Need(s, "base"), "base"));
                }).ToList(),
            };
        if (f.TryGetValue("augmentations", out var au))
            p = p with
            {
                Augmentations = Arr(au, "augmentations").Select(x =>
                {
                    var s = Obj(x, "augmentation");
                    return new ProfileAugmentation(U32(Need(s, "id"), "id"), U32(Need(s, "times"), "times"));
                }).ToList(),
            };
        if (!Absent(f, "allegiance"))
        {
            var a = Obj(f["allegiance"], "allegiance");
            p = p with
            {
                Allegiance = new ProfileAllegiance(Str(Need(a, "key"), "key"), Absent(a, "name") ? null : Str(a["name"], "name"),
                    U32(Need(a, "members"), "members"), U32(Need(a, "rank"), "rank"),
                    Absent(a, "patron") ? null : Addr(a["patron"], "patron"),
                    Absent(a, "monarch") ? null : Addr(a["monarch"], "monarch")),
            };
        }
        return p;
    }

    // ---- writing -------------------------------------------------------------------------------

    /// <summary>Writes one frame.</summary>
    public static string Write(DirectoryMessage m)
    {
        var o = new JsonObj();
        o.Str("type", m.Type);
        switch (m)
        {
            case Listing l:
                o.Str("id", l.Id).Str("who", l.Who.ToString()).Bool("on", l.On);
                if (l.Profile is not null)
                    o.Raw("profile", ProfileJson(l.Profile));
                break;
            case ListingResult r:
                o.Str("id", r.Id).Str("outcome", Word(r.Outcome));
                if (r.Reason is { } reason)
                    o.Str("reason", Word(reason));
                if (r.Url is not null)
                    o.Str("url", r.Url);
                break;
            case ListingOff off:
                o.Str("who", off.Who.ToString()).Str("by", Word(off.By));
                break;
        }
        return o.ToString();
    }

    private static string AttributeJson(AttributeValue a) => new JsonObj().U64("innate", a.Innate).U64("base", a.Base).ToString();

    private static string VitalJson(uint v) => new JsonObj().U64("base", v).ToString();

    /// <summary>A profile's JSON.</summary>
    public static string ProfileJson(Profile p)
    {
        var o = new JsonObj().U64("level", p.Level).U64("heritage", p.Heritage).U64("gender", p.Gender);
        if (p.Title is not null)
            o.Str("title", p.Title);
        if (p.Experience is { } e)
            o.Raw("experience", new JsonObj().U64("total", e.Total).U64("unassigned", e.Unassigned).ToString());
        if (p.Credits is { } credits)
            o.U64("credits", credits);
        if (p.Luminance is { } l)
            o.Raw("luminance", new JsonObj().U64("available", l.Available).U64("maximum", l.Maximum).ToString());
        if (p.Attributes is { } a)
            o.Raw("attributes", new JsonObj().Raw("strength", AttributeJson(a.Strength)).Raw("endurance", AttributeJson(a.Endurance))
                .Raw("coordination", AttributeJson(a.Coordination)).Raw("quickness", AttributeJson(a.Quickness))
                .Raw("focus", AttributeJson(a.Focus)).Raw("self", AttributeJson(a.Self)).ToString());
        if (p.Vitals is { } v)
            o.Raw("vitals", new JsonObj().Raw("health", VitalJson(v.Health)).Raw("stamina", VitalJson(v.Stamina))
                .Raw("mana", VitalJson(v.Mana)).ToString());
        if (p.Skills.Count > 0)
            o.Raw("skills", Json.Array(p.Skills.Select(s =>
                new JsonObj().U64("id", s.Id).Str("training", Word(s.Training)).U64("base", s.Base).ToString())));
        if (p.Augmentations.Count > 0)
            o.Raw("augmentations", Json.Array(p.Augmentations.Select(x =>
                new JsonObj().U64("id", x.Id).U64("times", x.Times).ToString())));
        if (p.Allegiance is { } al)
        {
            var ao = new JsonObj().Str("key", al.Key);
            if (al.Name is not null)
                ao.Str("name", al.Name);
            ao.U64("members", al.Members).U64("rank", al.Rank);
            if (al.Patron is not null)
                ao.Str("patron", al.Patron.ToString());
            if (al.Monarch is not null)
                ao.Str("monarch", al.Monarch.ToString());
            o.Raw("allegiance", ao.ToString());
        }
        if (p.Birth is { } birth)
            o.U64("birth", birth);
        if (p.Age is { } age)
            o.U64("age", age);
        if (p.Deaths is { } deaths)
            o.U64("deaths", deaths);
        if (p.Chess is { } chess)
            o.U64("chess", chess);
        if (p.Fishing is { } fishing)
            o.U64("fishing", fishing);
        if (p.Titles is { } titles)
            o.U64("titles", titles);
        return o.ToString();
    }
}
