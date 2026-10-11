// The conformance vectors, run against the mod's own code.

using System.Text.Json;

namespace Dereth.Network.Harness;

internal static class VectorChecks
{
    public static void Run(Checks c, string? dir)
    {
        if (dir is null)
        {
            c.NotRun("vectors", "network/conformance/vectors not found; pass --vectors");
            return;
        }
        Addresses(c, Path.Combine(dir, "addresses.json"));
        TextRulesVectors(c, Path.Combine(dir, "text.json"));
        Messages(c, Path.Combine(dir, "messages.json"));
        Challenges(c, Path.Combine(dir, "challenge.json"));
    }

    /// <summary>
    /// The challenge datagrams: each `ok` one reads as its slug and nonce and is written again byte
    /// for byte, each `later` one reads, and no `refused` one reads.
    /// </summary>
    private static void Challenges(Checks c, string path)
    {
        const string s = "vectors/challenge";
        using var doc = Load(c, s, path);
        var root = doc.RootElement;
        bool Reads(JsonElement v, out byte[] bytes)
        {
            bytes = Convert.FromHexString(v.GetProperty("datagram").GetString()!);
            var got = Challenge.TryParse(bytes);
            return got is not null && got.Slug == v.GetProperty("slug").GetString()
                && Challenge.NonceHex(got.Nonce) == v.GetProperty("nonce").GetString();
        }
        var count = 0;
        foreach (var v in root.GetProperty("ok").EnumerateArray())
        {
            var reads = Reads(v, out var bytes);
            var nonce = Challenge.ParseNonceHex(v.GetProperty("nonce").GetString()!);
            var written = nonce is null ? null : new Challenge(v.GetProperty("slug").GetString()!, nonce).Encode();
            c.Check(s, reads && written is not null && written.AsSpan().SequenceEqual(bytes),
                $"{v.GetProperty("slug").GetString()} reads, and is written again byte for byte");
            count++;
        }
        foreach (var v in root.GetProperty("later").EnumerateArray())
        {
            c.Check(s, Reads(v, out _), $"a later version's {v.GetProperty("slug").GetString()} reads");
            count++;
        }
        foreach (var v in root.GetProperty("refused").EnumerateArray())
        {
            var bytes = Convert.FromHexString(v.GetProperty("datagram").GetString()!);
            c.Check(s, Challenge.TryParse(bytes) is null, $"refused: {v.GetProperty("why").GetString()}");
            count++;
        }
        c.Check(s, count > 0, $"{count} challenge vectors ran");
    }

    private static JsonDocument Load(Checks c, string section, string path)
    {
        try
        {
            return JsonDocument.Parse(File.ReadAllText(path));
        }
        catch (Exception e)
        {
            c.Fail(section, $"{Path.GetFileName(path)} does not read as JSON: {e.Message}");
            throw;
        }
    }

    private static void Addresses(Checks c, string path)
    {
        const string s = "vectors/addresses";
        using var doc = Load(c, s, path);
        var root = doc.RootElement;
        foreach (var v in root.GetProperty("ok").EnumerateArray())
        {
            var input = v.GetProperty("input").GetString()!;
            var ok = Address.TryParse(input, out var a, out var err);
            c.Check(s, ok
                && a.Name == v.GetProperty("name").GetString()
                && a.World == v.GetProperty("world").GetString()
                && a.Key == v.GetProperty("key").GetString()
                && a.IsChannel == v.GetProperty("channel").GetBoolean(),
                $"{Show(input)} parses to its halves and key (got {(ok ? $"{a.Name}|{a.World}|{a.Key}|{a.IsChannel}" : err.ToString())})");
        }
        foreach (var v in root.GetProperty("refused").EnumerateArray())
        {
            var input = v.GetString()!;
            c.Check(s, !Address.TryParse(input, out _, out _), $"{Show(input)} is refused");
        }
        var reserved = root.GetProperty("reserved_slugs").EnumerateArray().Select(x => x.GetString()!).ToList();
        foreach (var r in reserved)
            c.Check(s, !Address.IsNetworkSlug(r) && Address.ReservedSlugs.Contains(r), $"{r} is reserved");
        c.Check(s, reserved.Order().SequenceEqual(Address.ReservedSlugs.Order()), "the reserved slugs are exactly the vectors'");
    }

    private static void TextRulesVectors(Checks c, string path)
    {
        const string s = "vectors/text";
        using var doc = Load(c, s, path);
        var root = doc.RootElement;
        var max = root.GetProperty("max_line_chars").GetInt32();
        c.Check(s, max == TextRules.MaxLineChars, "the longest line is " + max);
        foreach (var v in root.GetProperty("lines").EnumerateArray())
        {
            var input = TextOf(v, "input");
            var expected = TextOf(v, "output");
            if (input == "LONG")
            {
                input = new string('x', 300);
                expected = new string('x', max);
            }
            var got = TextRules.SanitizeLine(input);
            c.Check(s, got == expected, $"line {Show(input)} -> {Show(expected)} (got {Show(got)})");
            c.Check(s, TextRules.SanitizeLine(got) == got, $"line {Show(input)} sanitises to itself once sanitised");
        }
        foreach (var v in root.GetProperty("names").EnumerateArray())
        {
            var input = v.GetProperty("input").GetString()!;
            var got = TextRules.SanitizeName(input);
            if (v.TryGetProperty("refused", out var r) && r.GetBoolean())
                c.Check(s, got is null, $"name {Show(input)} is refused (got {Show(got)})");
            else
            {
                var expected = v.GetProperty("output").GetString()!;
                c.Check(s, got == expected, $"name {Show(input)} -> {Show(expected)} (got {Show(got)})");
                c.Check(s, got is not null && TextRules.SanitizeName(got) == got,
                    $"name {Show(input)} sanitises to itself once sanitised");
            }
        }
    }

    /// <summary>A text vector's string: `field` as it is, or `field_parts` joined, each piece repeated its number of times.</summary>
    private static string TextOf(JsonElement v, string field)
    {
        if (v.TryGetProperty(field + "_parts", out var parts))
            return string.Concat(parts.EnumerateArray().Select(p =>
                string.Concat(Enumerable.Repeat(p[0].GetString()!, p[1].GetInt32()))));
        return v.GetProperty(field).GetString()!;
    }

    /// <summary>A frame with every `${c*N}` written out as the character c, N times.</summary>
    private static string Expand(string frame)
    {
        var b = new System.Text.StringBuilder();
        var rest = frame;
        while (true)
        {
            var at = rest.IndexOf("${", StringComparison.Ordinal);
            var end = at < 0 ? -1 : rest.IndexOf('}', at);
            if (at < 0 || end < 0)
            {
                b.Append(rest);
                return b.ToString();
            }
            b.Append(rest[..at]);
            var spec = rest[(at + 2)..end];
            var star = spec.IndexOf('*');
            if (star == 1 && int.TryParse(spec[2..], out var n))
                b.Append(spec[0], n);
            else
                b.Append(rest[at..(end + 1)]);
            rest = rest[(end + 1)..];
        }
    }

    /// <summary>One field of a message, by what the field means: read from the message, never from a writing of it.</summary>
    private static object? Field(WireMessage m, string name) => (m, name) switch
    {
        (Welcome w, "world") => w.World,
        (Welcome w, "worlds") => (double)w.Worlds.Count,
        (Welcome w, "hold_secs") => w.Limits.HoldSecs,
        (Welcome w, "result_timeout_secs") => w.Limits.ResultTimeoutSecs,
        (Welcome w, "tells_per_character.burst") => w.Limits.TellsPerCharacter.Burst,
        (Watch w, "owner") => w.Owner.ToString(),
        (Watch w, "target") => w.Target.ToString(),
        (Watch w, "on") => w.On,
        (Presence p, "to") => p.To.ToString(),
        (Presence p, "of") => p.Of.ToString(),
        (Presence p, "online") => p.Online,
        (Tell t, "id") => t.Id,
        (Tell t, "from") => t.From.ToString(),
        (Tell t, "to") => t.To.ToString(),
        (Tell t, "text") => t.Text,
        (Tell t, "friend") => t.Friend,
        (TellResult r, "id") => r.Id,
        (TellResult r, "outcome") => Wire.Word(r.Outcome),
        (TellResult r, "message") => r.Message,
        (ChannelLine l, "channel") => l.Channel,
        (ChannelLine l, "from") => l.From.ToString(),
        (ChannelLine l, "to") => l.To.ToString(),
        (ChannelLine l, "text") => l.Text,
        (PersonRequest p, "id") => p.Id,
        (PersonRequest p, "op") => Wire.Word(p.Op),
        (PersonRequest p, "who") => p.Who.ToString(),
        (PersonRequest p, "target") => p.Target?.ToString(),
        (PersonRequest p, "targets") => Parse(Json.Array(p.TargetList.Select(a => Json.Str(a.ToString())))),
        (PersonRequest p, "shown") => p.Shown,
        (PersonResult r, "id") => r.Id,
        (PersonResult r, "outcome") => Wire.Word(r.Outcome),
        (PersonResult r, "code") => r.Code,
        (PersonResult r, "expires_secs") => (double?)r.ExpiresSecs,
        (PersonResult r, "url") => r.Url,
        (PersonResult r, "characters") => Parse(Json.Array(r.Characters.Select(a => Json.Str(a.ToString())))),
        (PersonWatch w, "who") => w.Who.ToString(),
        (PersonWatch w, "on") => w.On,
        (PersonFriends v, "to") => v.To.ToString(),
        (PersonFriends v, "linked") => v.Linked,
        (PersonFriends v, "alts") => Wire.Word(v.Alts),
        (PersonFriends v, "persons") => Parse(Json.Array(v.Persons.Select(p =>
            "{\"handle\":" + Json.Str(p.Handle.ToString()) + ",\"characters\":"
            + Json.Array(p.Characters.Select(a => Json.Str(a.ToString()))) + "}"))),
        _ => "(no such field)",
    };

    private static bool Same(object? got, JsonElement want) => want.ValueKind switch
    {
        JsonValueKind.String => got is string s && s == want.GetString(),
        JsonValueKind.Number => got is double d && d == want.GetDouble(),
        JsonValueKind.True => got is true,
        JsonValueKind.False => got is false,
        JsonValueKind.Null => got is null,
        JsonValueKind.Array or JsonValueKind.Object => got is JsonElement e && Covers(e, want) && Covers(want, e),
        _ => false,
    };

    private static void Messages(Checks c, string path)
    {
        const string s = "vectors/messages";
        using var doc = Load(c, s, path);
        var root = doc.RootElement;
        var max = root.GetProperty("max_frame_bytes").GetInt32();
        c.Check(s, max == Wire.MaxMessageBytes, "the longest frame is " + max);
        var identical = 0;
        var ok = 0;
        foreach (var v in root.GetProperty("ok").EnumerateArray())
        {
            ok++;
            var type = v.GetProperty("type").GetString()!;
            var frame = Expand(v.GetProperty("frame").GetString()!);
            var r = Wire.Read(frame);
            if (r.Status != ReadStatus.Ok)
            {
                c.Fail(s, $"{type} frame reads ({r.Status}: {r.Why})");
                continue;
            }
            c.Check(s, r.Message!.Type == type, $"{type} frame reads as {type} (got {r.Message!.Type})");
            var written = Wire.Write(r.Message!);
            var again = Wire.Read(written);
            c.Check(s, again.Status == ReadStatus.Ok && again.Message!.Type == type && Wire.Write(again.Message!) == written,
                $"{type} survives a write and a read: {written}");
            c.Check(s, Covers(Parse(written), Parse(frame)), $"{type}: every field of the vector is written with its value: {written}");
            if (v.TryGetProperty("expect", out var expect))
                foreach (var want in expect.EnumerateObject())
                {
                    var got = Field(r.Message!, want.Name);
                    c.Check(s, Same(got, want.Value), $"{type}: {want.Name} reads as {want.Value} (got {got})");
                }
            if (written == frame)
                identical++;
        }
        c.Note(s, $"{identical} of {ok} ok frames are written back byte for byte (the others differ only by a default the writer spells out)");
        foreach (var v in root.GetProperty("unknown").EnumerateArray())
        {
            var frame = Expand(v.GetString()!);
            c.Check(s, Wire.Read(frame).Status == ReadStatus.Unknown, $"{frame} is skipped as unknown");
        }
        foreach (var v in root.GetProperty("malformed").EnumerateArray())
        {
            var frame = Expand(v.GetString()!);
            var r = Wire.Read(frame);
            c.Check(s, r.Status == ReadStatus.Malformed, $"{frame} is malformed (got {r.Status})");
        }
        var longFrame = "{\"type\":\"status\",\"state\":\"" + new string('x', max) + "\"}";
        c.Check(s, Wire.Read(longFrame).Status == ReadStatus.Malformed, "a frame longer than the limit is malformed");
    }

    private static JsonElement Parse(string json)
    {
        using var d = JsonDocument.Parse(json);
        return d.RootElement.Clone();
    }

    /// <summary>Whether `big` has every field of `small` with an equal value (numbers by value).</summary>
    private static bool Covers(JsonElement big, JsonElement small)
    {
        if (small.ValueKind == JsonValueKind.Object)
        {
            if (big.ValueKind != JsonValueKind.Object)
                return false;
            foreach (var p in small.EnumerateObject())
                if (!big.TryGetProperty(p.Name, out var b) || !Covers(b, p.Value))
                    return false;
            return true;
        }
        if (small.ValueKind == JsonValueKind.Array)
        {
            if (big.ValueKind != JsonValueKind.Array || big.GetArrayLength() != small.GetArrayLength())
                return false;
            return big.EnumerateArray().Zip(small.EnumerateArray()).All(z => Covers(z.First, z.Second));
        }
        if (small.ValueKind == JsonValueKind.Number)
            return big.ValueKind == JsonValueKind.Number && big.GetDouble() == small.GetDouble();
        if (small.ValueKind == JsonValueKind.String)
            return big.ValueKind == JsonValueKind.String && big.GetString() == small.GetString();
        return big.ValueKind == small.ValueKind;
    }

    public static string Show(string? s) =>
        s is null ? "(refused)" : "\"" + string.Concat(s.Select(ch => ch < 0x20 || ch > 0x7E ? $"\\u{(int)ch:x4}" : ch.ToString())) + "\"";
}
