// The mod's settings (`Settings.json` in the mod's folder) and the checks that decide whether the
// network can start. The world's registry key is never in the file: an environment variable holds
// it, so it stays out of anything an operator might share.

using System.Text.Json;

namespace Dereth.Network;

public sealed class ModSettings
{
    /// <summary>The hub: `wss://host[:port][/prefix]`; plain `ws://` only on loopback, and only with <see cref="AllowLocalWs"/>.</summary>
    public string HubUrl { get; set; } = "wss://hub.dereth.network";

    /// <summary>Allows a plain `ws://` hub on this machine, for testing.</summary>
    public bool AllowLocalWs { get; set; }

    /// <summary>The world's slug in the hub's registry: the world half of its characters' addresses.</summary>
    public string Slug { get; set; } = "";

    /// <summary>The environment variable that holds the world's registry key (`drth_...`).</summary>
    public string KeyEnvironmentVariable { get; set; } = "DERETH_NETWORK_KEY";

    /// <summary>The TurbineChat room that carries the Global room: General, Trade, LFG, Roleplay, or empty for none.</summary>
    public string GlobalRoom { get; set; } = "Roleplay";

    /// <summary>Seconds a character must have played before it may send to the network.</summary>
    public uint MinCharacterAgeSeconds { get; set; }

    /// <summary>Where the network's records are kept, relative to the mod's folder.</summary>
    public string StorePath { get; set; } = "Dereth.Network.store.json";

    /// <summary>
    /// Settings renamed before any release read them under the old name: the old name is refused,
    /// naming the new one, so a setting is never silently dropped.
    /// </summary>
    private static readonly (string Old, string New)[] Refused = [("EventsRoom", "GlobalRoom")];

    /// <summary>Why the file cannot be used as it is: a refused old setting name, or null.</summary>
    public static string? Problem(string text)
    {
        using var doc = JsonDocument.Parse(text, new JsonDocumentOptions
        {
            AllowTrailingCommas = true,
            CommentHandling = JsonCommentHandling.Skip,
        });
        if (doc.RootElement.ValueKind != JsonValueKind.Object)
            return null;
        foreach (var p in doc.RootElement.EnumerateObject())
            foreach (var (old, now) in Refused)
                if (p.Name.Equals(old, StringComparison.OrdinalIgnoreCase))
                    return $"unknown setting {p.Name} in Settings.json: it is now {now}; rename it (the old name is not read)";
        return null;
    }

    public static ModSettings FromJson(string text) =>
        JsonSerializer.Deserialize<ModSettings>(text, new JsonSerializerOptions
        {
            PropertyNameCaseInsensitive = true,
            AllowTrailingCommas = true,
            ReadCommentHandling = JsonCommentHandling.Skip,
        }) ?? new ModSettings();
}

/// <summary>The room slot the Global room is carried in.</summary>
public enum RoomSlot
{
    General,
    Trade,
    Lfg,
    Roleplay,
}

public static class RoomSlots
{
    public static bool TryParse(string s, out RoomSlot slot)
    {
        switch (Text.AsciiLower(s.Trim()))
        {
            case "general": slot = RoomSlot.General; return true;
            case "trade": slot = RoomSlot.Trade; return true;
            case "lfg": slot = RoomSlot.Lfg; return true;
            case "roleplay": slot = RoomSlot.Roleplay; return true;
            default: slot = default; return false;
        }
    }

    /// <summary>The room's TurbineChat channel number.</summary>
    public static uint Channel(RoomSlot s) => s switch
    {
        RoomSlot.General => 2,
        RoomSlot.Trade => 3,
        RoomSlot.Lfg => 4,
        _ => 5,
    };

    public static string Name(RoomSlot s) => s switch
    {
        RoomSlot.General => "General",
        RoomSlot.Trade => "Trade",
        RoomSlot.Lfg => "LFG",
        _ => "Roleplay",
    };
}

/// <summary>A hub address, checked.</summary>
public static class HubUrl
{
    /// <summary>
    /// The link's address for `slug` at `hub`: `wss://` anywhere, `ws://` only to a loopback host
    /// and only when allowed. No user, query or fragment.
    /// </summary>
    public static bool TryLink(string hub, string slug, bool allowLocalWs, out Uri link, out string why)
    {
        link = null!;
        if (!Uri.TryCreate(hub.Trim(), UriKind.Absolute, out var root))
        {
            why = "HubUrl is not an address";
            return false;
        }
        if (root.Scheme != "wss" && !(root.Scheme == "ws" && allowLocalWs && root.IsLoopback))
        {
            why = root.Scheme == "ws"
                ? "plain ws:// is for a hub on this machine only, with AllowLocalWs; use wss://"
                : "HubUrl must be wss://host";
            return false;
        }
        if (root.UserInfo.Length != 0 || root.Query.Length != 0 || root.Fragment.Length != 0)
        {
            why = "HubUrl holds a user, a query or a fragment";
            return false;
        }
        var prefix = root.AbsolutePath.TrimEnd('/');
        link = new Uri(root.GetLeftPart(UriPartial.Authority) + prefix + "/v1/servers/" + slug + "/link");
        why = "";
        return true;
    }

    /// <summary>Whether `key` is a registry key: `drth_` and 43 characters of base64url.</summary>
    public static bool IsKey(string? key)
    {
        if (key is null || key.Length != 48 || !key.StartsWith("drth_", StringComparison.Ordinal))
            return false;
        for (var i = 5; i < key.Length; i++)
        {
            var c = key[i];
            if (!(c is >= 'a' and <= 'z' || c is >= 'A' and <= 'Z' || c is >= '0' and <= '9' || c is '-' or '_'))
                return false;
        }
        return true;
    }
}
