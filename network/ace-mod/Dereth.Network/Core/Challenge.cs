// The challenge datagram: what the hub sends the world's game port to prove the world answers at
// its registered address, when its operator verifies it in game.
//
// The hub sends a fresh random nonce to each of the endpoint's addresses, at the game port. A world
// that asked the hub to verify it in the last minute reads the datagram on its game port and posts
// the nonce back to the hub over HTTPS, from whatever address its outgoing calls leave by. It never
// answers over UDP, so the datagram draws nothing back to whoever its source address names.
//
// Every challenge starts with a 20-byte header shaped as a packet header's, whose size field is
// larger than any datagram a server reads: ACE's packet reader drops it at the size check, before it
// reads anything else, so a world without the mod (or with the network off) ignores it.

namespace Dereth.Network;

/// <summary>A challenge: the slug of the world it is for, and the nonce to post back.</summary>
public sealed record Challenge(string Slug, byte[] Nonce)
{
    /// <summary>The first four bytes, in place of a packet's sequence number: `DNVC`.</summary>
    public static ReadOnlySpan<byte> Magic => "DNVC"u8;

    /// <summary>The header's size field: more than any datagram's payload can be.</summary>
    public const ushort ImpossibleSize = 0xFFFF;

    /// <summary>The header in front of the challenge.</summary>
    public const int HeaderLength = 20;

    /// <summary>The byte after the header that says the datagram is a challenge.</summary>
    public const byte Kind = 0x01;

    /// <summary>The version of the layout this code writes. A later version only appends fields.</summary>
    public const byte Version = 1;

    /// <summary>The nonce's length.</summary>
    public const int NonceLength = 16;

    /// <summary>The longest slug a challenge carries: a registry slug's longest.</summary>
    public const int MaxSlugLength = 40;

    /// <summary>The longest challenge this version writes.</summary>
    public const int MaxLength = HeaderLength + 2 + NonceLength + 1 + MaxSlugLength;

    /// <summary>
    /// How long after the world asks the hub to verify it the world answers a challenge, in seconds;
    /// outside that window it ignores every challenge.
    /// </summary>
    public const double WindowSecs = 60;

    /// <summary>The most distinct nonces the world answers for one verification.</summary>
    public const int MaxAnswers = 8;

    /// <summary>Whether `slug` can be carried: 1 to 40 bytes of `a-z`, `0-9` and `-`.</summary>
    private static bool Carried(ReadOnlySpan<byte> slug)
    {
        if (slug.Length is < 1 or > MaxSlugLength)
            return false;
        foreach (var b in slug)
            if (!(b is >= (byte)'a' and <= (byte)'z' || b is >= (byte)'0' and <= (byte)'9' || b == (byte)'-'))
                return false;
        return true;
    }

    /// <summary>
    /// Whether `datagram` is shaped as a challenge: the magic and the impossible size. Checked before
    /// anything else, so a challenge never reaches the packet reader.
    /// </summary>
    public static bool IsChallenge(ReadOnlySpan<byte> datagram) =>
        datagram.Length >= HeaderLength + 2
        && datagram[..4].SequenceEqual(Magic)
        && datagram[16] == 0xFF && datagram[17] == 0xFF;

    /// <summary>The datagram, or null for a slug that cannot be carried or a nonce of the wrong length.</summary>
    public byte[]? Encode()
    {
        var slug = System.Text.Encoding.ASCII.GetBytes(Slug);
        if (Nonce.Length != NonceLength || !Carried(slug) || slug.Length != Slug.Length)
            return null;
        var datagram = new byte[HeaderLength + 2 + NonceLength + 1 + slug.Length];
        Magic.CopyTo(datagram);
        // Flags, checksum, recipient id and time: zero. Then the size, and a zero iteration.
        datagram[16] = 0xFF;
        datagram[17] = 0xFF;
        datagram[HeaderLength] = Kind;
        datagram[HeaderLength + 1] = Version;
        Nonce.CopyTo(datagram, HeaderLength + 2);
        datagram[HeaderLength + 2 + NonceLength] = (byte)slug.Length;
        slug.CopyTo(datagram, HeaderLength + 3 + NonceLength);
        return datagram;
    }

    /// <summary>
    /// Reads a challenge. Bytes after the fields this version knows are a later version's and are
    /// ignored. Null for anything else: another kind, version 0, a datagram cut short, or a slug that
    /// cannot be carried.
    /// </summary>
    public static Challenge? TryParse(ReadOnlySpan<byte> datagram)
    {
        if (!IsChallenge(datagram) || datagram[HeaderLength] != Kind || datagram[HeaderLength + 1] == 0)
            return null;
        const int at = HeaderLength + 2;
        if (datagram.Length < at + NonceLength + 1)
            return null;
        var nonce = datagram.Slice(at, NonceLength).ToArray();
        int length = datagram[at + NonceLength];
        const int start = at + NonceLength + 1;
        if (datagram.Length < start + length)
            return null;
        var slug = datagram.Slice(start, length);
        return Carried(slug) ? new Challenge(System.Text.Encoding.ASCII.GetString(slug), nonce) : null;
    }

    /// <summary>The nonce as the world posts it back: 32 lowercase hex digits.</summary>
    public static string NonceHex(ReadOnlySpan<byte> nonce) => Convert.ToHexString(nonce).ToLowerInvariant();

    /// <summary>A nonce read back from its hex form (either case); null for anything that is not 32 hex digits.</summary>
    public static byte[]? ParseNonceHex(string text)
    {
        if (text.Length != NonceLength * 2)
            return null;
        foreach (var ch in text)
            if (!Uri.IsHexDigit(ch))
                return null;
        return Convert.FromHexString(text);
    }
}
