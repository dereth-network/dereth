// The text rules every receiving world applies to every remote line and every remote sender's name.
//
// A world never trusts another world's text. Before a remote line reaches a client it is made safe
// for the oldest client that may read it, the final retail client, which draws chat in
// Windows-1252 (a room line's fields end at the first character the code page cannot hold), reads
// `<Tell:...>` runs as clickable name links in any line, drops a room line holding a line feed, a
// name link or 256 characters or more, and takes a leading `+` on a name as the administrator
// marker and a name ending in `^` or `&` as Olthoi speech.
//
// Both rules are idempotent: a sanitised string sanitises to itself.

using System.Text;

namespace Dereth.Network;

public static class TextRules
{
    /// <summary>
    /// The longest remote line, in characters, after the other rules. The final retail client drops
    /// a room line of 256 characters or more, so a cut line is one shorter than that.
    /// </summary>
    public const int MaxLineChars = 255;

    /// <summary>
    /// The longest text a link message may carry, in characters. Longer text is malformed; shorter
    /// text is cut to <see cref="MaxLineChars"/> by the receiving world's rules.
    /// </summary>
    public const int MaxTextChars = 1024;

    /// <summary>`0x80..0x9F` of Windows-1252: the 32 code points where it differs from Latin-1.</summary>
    private static readonly ushort[] High =
    [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021,
        0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F,
        0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
        0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
    ];

    /// <summary>Whether code point `c` has a byte of its own in Windows-1252 (not a best fit).</summary>
    public static bool Cp1252Exact(int c)
    {
        if (c is < 0 or > 0xFFFF)
            return false;
        if (c is <= 0x7F or (>= 0xA0 and <= 0xFF))
            return true;
        return Array.IndexOf(High, (ushort)c) >= 0;
    }

    /// <summary>
    /// The code point Windows narrows `c` to in code page 1252: `c` itself when the code page holds
    /// it, its best fit when the published table has one (`ā` → `a`, `＜` → `&lt;`), else none.
    /// </summary>
    public static int? Cp1252Fit(int c)
    {
        if (Cp1252Exact(c))
            return c;
        if (c is < 0 or > 0xFFFF)
            return null;
        var unit = (ushort)c;
        foreach (var table in new[] { Cp1252BestFit.BestFit, Cp1252BestFit.BestFitSincePublished })
        {
            int lo = 0, hi = table.Length - 1;
            while (lo <= hi)
            {
                var mid = (lo + hi) >>> 1;
                var u = table[mid].Unit;
                if (u == unit)
                    return DecodeByte(table[mid].Byte);
                if (u < unit)
                    lo = mid + 1;
                else
                    hi = mid - 1;
            }
        }
        return null;
    }

    /// <summary>The code point a Windows-1252 byte stands for.</summary>
    public static int DecodeByte(byte b) => b is >= 0x80 and <= 0x9F ? High[b - 0x80] : b;

    /// <summary>Windows-1252 bytes as text, one character a byte.</summary>
    public static string DecodeCp1252(ReadOnlySpan<byte> bytes)
    {
        var b = new StringBuilder(bytes.Length);
        foreach (var x in bytes)
            b.Append((char)DecodeByte(x));
        return b.ToString();
    }

    /// <summary>
    /// Text as Windows-1252 bytes, a character a byte: each character its own byte, its best fit,
    /// or `?`.
    /// </summary>
    public static byte[] EncodeCp1252(string s)
    {
        var o = new List<byte>(s.Length);
        foreach (var r in Runes(s))
        {
            var f = Cp1252Fit(r);
            o.Add(f is int v ? ByteOf(v) : (byte)'?');
        }
        return o.ToArray();
    }

    private static byte ByteOf(int c)
    {
        if (c is <= 0x7F or (>= 0xA0 and <= 0xFF))
            return (byte)c;
        var i = Array.IndexOf(High, (ushort)c);
        return i >= 0 ? (byte)(0x80 + i) : (byte)'?';
    }

    /// <summary>Whether `c` is a control character: U+0000 to U+001F, or U+007F to U+009F.</summary>
    private static bool IsControl(int c) => c is (>= 0x00 and <= 0x1F) or (>= 0x7F and <= 0x9F);

    /// <summary>The code points of `s`; a lone surrogate reads as U+FFFD.</summary>
    private static List<int> Runes(string s)
    {
        var o = new List<int>(s.Length);
        var i = 0;
        while (i < s.Length)
        {
            Rune.DecodeFromUtf16(s.AsSpan(i), out var r, out var used);
            o.Add(r.Value);
            i += used;
        }
        return o;
    }

    private static string FromRunes(List<int> runes)
    {
        var b = new StringBuilder(runes.Count);
        foreach (var r in runes)
            b.Append(new Rune(r).ToString());
        return b.ToString();
    }

    /// <summary>
    /// Every character narrowed to one Windows-1252 can hold: kept, best-fitted, or `?`. Control
    /// characters are dropped, except that a line break or tab becomes one space.
    /// </summary>
    private static List<int> Narrow(List<int> s)
    {
        var o = new List<int>(s.Count);
        foreach (var c in s)
        {
            if (c is '\r' or '\n' or '\t' or 0x2028 or 0x2029 or 0x85)
            {
                o.Add(' ');
                continue;
            }
            if (IsControl(c))
                continue;
            var f = Cp1252Fit(c);
            if (f is int v)
            {
                // A best fit may itself be a control character (none is, in the published table);
                // dropping it keeps the rule.
                if (!IsControl(v))
                    o.Add(v);
            }
            else
            {
                o.Add('?');
            }
        }
        return o;
    }

    private static int AsciiFold(int c) => c is >= 'A' and <= 'Z' ? c + 32 : c;

    /// <summary>
    /// `s` with every name-link run taken out: any `&lt;tell:` or `&lt;tell ` opening up to its `&gt;`,
    /// and any `&lt;\tell&gt;` or `&lt;/tell&gt;` closing, matched without regard to case. An opening
    /// with no `&gt;` loses its `&lt;`.
    /// </summary>
    private static List<int> StripMarkup(List<int> s)
    {
        bool Starts(int i, string pat)
        {
            if (i + pat.Length > s.Count)
                return false;
            for (var k = 0; k < pat.Length; k++)
                if (AsciiFold(s[i + k]) != pat[k])
                    return false;
            return true;
        }
        var o = new List<int>(s.Count);
        var i = 0;
        while (i < s.Count)
        {
            if (s[i] == '<' && (Starts(i, "<tell:") || Starts(i, "<tell ")))
            {
                var end = s.IndexOf('>', i);
                i = end >= 0 ? end + 1 : i + 1;
                continue;
            }
            if (s[i] == '<' && (Starts(i, "<\\tell>") || Starts(i, "</tell>")))
            {
                i += 7;
                continue;
            }
            o.Add(s[i]);
            i++;
        }
        return o;
    }

    private static bool SameRunes(List<int> a, List<int> b)
    {
        if (a.Count != b.Count)
            return false;
        for (var i = 0; i < a.Count; i++)
            if (a[i] != b[i])
                return false;
        return true;
    }

    /// <summary>
    /// A remote line made safe: name-link markup goes, line breaks become spaces, control
    /// characters go, every character becomes one Windows-1252 holds (its best fit, else `?`),
    /// markup goes again until none is left, the ends are trimmed, and the result is cut at
    /// <see cref="MaxLineChars"/>.
    /// </summary>
    public static string SanitizeLine(string text)
    {
        // Markup first, so a link split by a control character cannot reassemble after it goes;
        // and again after narrowing, so a fullwidth bracket that best-fits to `<` cannot either.
        var s = StripMarkup(Narrow(StripMarkup(Runes(text))));
        while (true)
        {
            var again = StripMarkup(s);
            if (SameRunes(again, s))
                break;
            s = again;
        }
        var trimmed = FromRunes(s).Trim();
        // Every character left is one UTF-16 unit, so the cut counts characters. Cut, then trim
        // again: a cut just after a space would leave one at the end, which a second pass would
        // take off.
        return trimmed.Length > MaxLineChars ? trimmed[..MaxLineChars].TrimEnd() : trimmed;
    }

    /// <summary>
    /// A remote sender's display name made safe, or refused (null). The line rules run, and every
    /// leading `+` (the administrator marker) and white space before or between them goes; a name
    /// holding `^` or `&amp;` anywhere is refused rather than changed, as is one that is empty.
    /// </summary>
    public static string? SanitizeName(string name) => SanitizeName(name, out _);

    public static string? SanitizeName(string name, out NameRefused why)
    {
        var s = SanitizeLine(name);
        while (true)
        {
            var t = s.TrimStart().TrimStart('+');
            if (t.Length == s.Length)
                break;
            s = t;
        }
        if (s.Contains('^') || s.Contains('&'))
        {
            why = NameRefused.OlthoiMark;
            return null;
        }
        if (s.Length == 0)
        {
            why = NameRefused.Empty;
            return null;
        }
        why = NameRefused.None;
        return s;
    }
}

public enum NameRefused
{
    None,
    /// <summary>It holds `^` or `&amp;`, the Olthoi speech marks.</summary>
    OlthoiMark,
    /// <summary>Nothing is left of it once the other rules have run.</summary>
    Empty,
}
