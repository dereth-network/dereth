// Addresses: `Name@world`, the one way a character, or a channel, is named across worlds.
//
// An address is a character name, the separator `@`, and the network slug of the character's home
// world: `Bob@coldeve`. A channel is addressed the same way at the reserved world `channel`:
// `oldguard@channel`. The world half is what makes an address unmistakable for a local name (no
// character name may hold the separator) and what keeps a vanity world from passing as staff.
//
// Matching ignores case on both halves; the name keeps the case it was written with, for display,
// and the world half is always written in lower case.

using System.Text;

namespace Dereth.Network;

public sealed class Address : IEquatable<Address>
{
    /// <summary>The character between the name and the world.</summary>
    public const char Separator = '@';

    /// <summary>The reserved world half that names a channel rather than a home world.</summary>
    public const string ChannelWorld = "channel";

    public const int MaxNameChars = 32;
    public const int MinSlugChars = 2;
    public const int MaxSlugChars = 12;
    public const int MinChannelChars = 2;
    public const int MaxChannelChars = 20;

    /// <summary>
    /// The slugs no world may take on the network: the nine retail worlds' names, the channel
    /// pseudo-world, the world kept for speakers from Discord and the hub's own words.
    /// </summary>
    public static readonly IReadOnlyList<string> ReservedSlugs =
    [
        "darktide", "frostfell", "harvestgain", "leafcull", "morningthaw", "solclaim", "thistledown",
        "verdantine", "wintersebb", ChannelWorld, "discord", "hub", "net", "network", "local", "localhost",
        "world", "all", "any",
    ];

    public string Name { get; }
    public string World { get; }

    private Address(string name, string world)
    {
        Name = name;
        World = world;
    }

    /// <summary>Whether this names a channel.</summary>
    public bool IsChannel => World == ChannelWorld;

    /// <summary>
    /// The identity key: the address in lower case. Two addresses that differ only in case name
    /// the same character.
    /// </summary>
    public string Key => Name.ToLowerInvariant() + Separator + World;

    public bool Same(Address other) => Key == other.Key;

    public override string ToString() => Name + Separator + World;

    public bool Equals(Address? other) => other is not null && Name == other.Name && World == other.World;

    public override bool Equals(object? obj) => obj is Address a && Equals(a);

    public override int GetHashCode() => HashCode.Combine(Name, World);

    /// <summary>
    /// Parses `Name@world`: one separator; white space trimmed from each half; one leading `+`
    /// (the administrator marker) removed from the name; the world half lower-cased.
    /// </summary>
    public static bool TryParse(string? s, out Address address, out AddressError error)
    {
        address = null!;
        if (s is null)
        {
            error = AddressError.Separator;
            return false;
        }
        var parts = s.Split(Separator);
        if (parts.Length != 2)
        {
            error = AddressError.Separator;
            return false;
        }
        return TryCreate(parts[0], parts[1], out address, out error);
    }

    public static Address? Parse(string? s) => TryParse(s, out var a, out _) ? a : null;

    /// <summary>An address from its halves, checked as <see cref="TryParse"/> checks them.</summary>
    public static bool TryCreate(string name, string world, out Address address, out AddressError error)
    {
        address = null!;
        var w = Text.AsciiLower(Text.Trim(world));
        if (!IsSlug(w))
        {
            error = AddressError.World;
            return false;
        }
        var n = Text.Trim(name);
        if (n.StartsWith('+'))
            n = Text.Trim(n[1..]);
        if (w == ChannelWorld)
        {
            if (!IsChannelName(n))
            {
                error = AddressError.Channel;
                return false;
            }
            address = new Address(Text.AsciiLower(n), w);
            error = AddressError.None;
            return true;
        }
        if (!IsCharacterName(n))
        {
            error = AddressError.Name;
            return false;
        }
        address = new Address(n, w);
        error = AddressError.None;
        return true;
    }

    public static Address? Create(string name, string world) => TryCreate(name, world, out var a, out _) ? a : null;

    /// <summary>
    /// Whether `s` holds the separator: how a world tells a network address from a local name
    /// before it parses it.
    /// </summary>
    public static bool LooksRemote(string? s) => s is not null && s.Contains(Separator);

    /// <summary>
    /// Whether `s` is a network world slug: 2 to 12 of `a-z` and `0-9`, with single hyphens
    /// between them.
    /// </summary>
    public static bool IsSlug(string s)
    {
        var len = Text.CharCount(s);
        if (len < MinSlugChars || len > MaxSlugChars)
            return false;
        foreach (var part in s.Split('-'))
        {
            if (part.Length == 0)
                return false;
            foreach (var c in part)
                if (!(c is >= 'a' and <= 'z' || c is >= '0' and <= '9'))
                    return false;
        }
        return true;
    }

    /// <summary>Whether a world may take `slug` on the network: a slug, and not reserved.</summary>
    public static bool IsNetworkSlug(string slug) => IsSlug(slug) && !ReservedSlugs.Contains(slug);

    /// <summary>
    /// Whether `s` is a channel name: 2 to 20 of `a-z`, `A-Z` and `0-9`, with single hyphens between
    /// them. Channel names are matched and written in lower case.
    /// </summary>
    public static bool IsChannelName(string s)
    {
        var len = Text.CharCount(s);
        if (len < MinChannelChars || len > MaxChannelChars)
            return false;
        foreach (var part in s.Split('-'))
        {
            if (part.Length == 0)
                return false;
            foreach (var c in part)
                if (!(c is >= 'a' and <= 'z' || c is >= 'A' and <= 'Z' || c is >= '0' and <= '9'))
                    return false;
        }
        return true;
    }

    /// <summary>
    /// Whether `s` may be the name half of a character's address: 1 to 32 characters, starting and
    /// ending with a letter, holding only letters, single spaces, apostrophes and hyphens. A letter
    /// is any alphabetic character Windows-1252 holds exactly (`Ëlwen` is a name). So a name never
    /// holds the separator, a comma, a markup bracket, a control character, or `^` or `&` (the
    /// Olthoi speech marks).
    /// </summary>
    public static bool IsCharacterName(string s)
    {
        var runes = new List<Rune>();
        foreach (var r in s.EnumerateRunes())
            runes.Add(r);
        if (runes.Count == 0 || runes.Count > MaxNameChars)
            return false;
        static bool IsLetter(Rune r) => Rune.IsLetter(r) && TextRules.Cp1252Exact(r.Value);
        if (!IsLetter(runes[0]) || !IsLetter(runes[^1]))
            return false;
        foreach (var r in runes)
            if (!IsLetter(r) && r.Value is not (' ' or '\'' or '-'))
                return false;
        return !s.Contains("  ", StringComparison.Ordinal);
    }
}

public enum AddressError
{
    None,
    /// <summary>No separator, or more than one.</summary>
    Separator,
    /// <summary>The name half is empty, too long, or holds a character no name may hold.</summary>
    Name,
    /// <summary>The world half is not a network slug.</summary>
    World,
    /// <summary>The world half is the channel world and the name half is not a channel name.</summary>
    Channel,
}

/// <summary>Small string helpers that keep the rules' meaning of "character" and "white space".</summary>
public static class Text
{
    /// <summary>The number of characters (code points) in `s`.</summary>
    public static int CharCount(string s)
    {
        var n = 0;
        foreach (var _ in s.EnumerateRunes())
            n++;
        return n;
    }

    /// <summary>The first `n` characters (code points) of `s`.</summary>
    public static string FirstChars(string s, int n)
    {
        var b = new StringBuilder();
        var i = 0;
        foreach (var r in s.EnumerateRunes())
        {
            if (i++ >= n)
                break;
            b.Append(r.ToString());
        }
        return b.ToString();
    }

    /// <summary>`s` with the white space at both ends removed.</summary>
    public static string Trim(string s) => s.Trim();

    /// <summary>`s` with its ASCII capitals, and only those, made small.</summary>
    public static string AsciiLower(string s)
    {
        var any = false;
        foreach (var c in s)
            if (c is >= 'A' and <= 'Z')
            {
                any = true;
                break;
            }
        if (!any)
            return s;
        var b = new StringBuilder(s.Length);
        foreach (var c in s)
            b.Append(c is >= 'A' and <= 'Z' ? (char)(c + 32) : c);
        return b.ToString();
    }
}
