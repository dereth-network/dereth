// A world's own room mirrored into Discord end to end against a real hub, over loopback only: the
// hub runs with its sign-in and its Discord broker on, against a stand-in for Discord on 127.0.0.1
// (never Discord itself) that signs the world's operator in and holds the webhook the operator made
// in a channel of their own server. The world half, linked to the hub, tells a hub that holds none
// where it carries the Global room (Roleplay, its setting), and the hub keeps it; its General room's
// lines stay here while the hub does not mirror the room, and once its operator, signed in, mirrors
// the room through that webhook, the hub tells the world at once, a line said there is said through
// it under the speaker's address, the hub hands none of the world's own lines back, and nothing of
// another room reaches Discord. A mirror of Roleplay, which carries the Global room, is refused;
// the operator moves the Global room on the site, and the world's own `/dn admin global` moves it
// again, each carried by the world at once. The hub is stopped at the end.

using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace Dereth.Network.Harness;

/// <summary>
/// Discord's API as the hub uses it for sign-in and for a webhook an operator made, on 127.0.0.1:
/// `code-&lt;id&gt;` is exchanged for `tok-&lt;id&gt;` when the client secret is right, and that token
/// reads user `&lt;id&gt;`; one webhook, read and said through with its own token.
/// </summary>
internal sealed class MirrorStandIn : IDisposable
{
    public const string Secret = "the-secret";
    public const string BotToken = "stand-in-bot-token";
    public const string HookId = "1558376954619629701";
    public const string HookToken = "webhook-token_Zx9q";
    public const string Guild = "1558376951000000700";
    public const string Channel = "1558376951000000701";
    private readonly TcpListener listener = new(IPAddress.Loopback, 0);
    private readonly CancellationTokenSource stop = new();
    private readonly object gate = new();
    private long next = 1_558_376_954_619_629_800;
    private readonly List<JsonObject> said = [];

    public int Port { get; }

    public MirrorStandIn()
    {
        listener.Start();
        Port = ((IPEndPoint)listener.LocalEndpoint).Port;
        _ = Task.Run(Serve);
    }

    private string Id() => (++next).ToString(System.Globalization.CultureInfo.InvariantCulture);

    /// <summary>The lines said through the webhook, oldest first: what the hub sent.</summary>
    public List<JsonObject> Said()
    {
        lock (gate)
            return said.Select(x => (JsonObject)x.DeepClone()).ToList();
    }

    private async Task Serve()
    {
        while (!stop.IsCancellationRequested)
        {
            TcpClient client;
            try
            {
                client = await listener.AcceptTcpClientAsync(stop.Token);
            }
            catch (Exception)
            {
                return;
            }
            _ = Task.Run(() => Answer(client));
        }
    }

    private void Answer(TcpClient client)
    {
        using var _ = client;
        var stream = client.GetStream();
        stream.ReadTimeout = 5000;
        var head = new StringBuilder();
        var one = new byte[1];
        while (!head.ToString().EndsWith("\r\n\r\n", StringComparison.Ordinal) && stream.Read(one, 0, 1) == 1)
            head.Append((char)one[0]);
        var lines = head.ToString().Split("\r\n");
        var first = lines[0].Split(' ');
        string? Header(string name) => lines.Skip(1).Select(l => l.Split(':', 2)).Where(p => p.Length == 2)
            .FirstOrDefault(p => p[0].Trim().Equals(name, StringComparison.OrdinalIgnoreCase))?[1].Trim();
        var length = int.TryParse(Header("Content-Length"), out var n) ? n : 0;
        var body = new byte[length];
        var read = 0;
        while (read < length)
        {
            var got = stream.Read(body, read, length - read);
            if (got == 0)
                break;
            read += got;
        }
        var text = Encoding.UTF8.GetString(body);
        var (status, reply) = Route(first.ElementAtOrDefault(0) ?? "", first.ElementAtOrDefault(1) ?? "", Header("Authorization") ?? "", text);
        var bytes = Encoding.UTF8.GetBytes(reply);
        stream.Write(Encoding.ASCII.GetBytes(
            $"HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {bytes.Length}\r\nConnection: close\r\n\r\n"));
        stream.Write(bytes);
    }

    private (int Status, string Body) Route(string method, string target, string auth, string body)
    {
        var path = target.StartsWith("/api/", StringComparison.Ordinal) ? target[4..] : "";
        var segs = (path.Contains('?') ? path[..path.IndexOf('?')] : path).Trim('/').Split('/');
        lock (gate)
        {
            switch (method, segs)
            {
                case ("POST", ["oauth2", "token"]):
                {
                    var form = body.Split('&').Select(kv => kv.Split('=', 2)).Where(kv => kv.Length == 2)
                        .ToDictionary(kv => kv[0], kv => Uri.UnescapeDataString(kv[1]));
                    return form.TryGetValue("code", out var code) && code.StartsWith("code-", StringComparison.Ordinal)
                                                                  && form.GetValueOrDefault("client_secret") == Secret
                        ? (200, new JsonObject { ["access_token"] = "tok-" + code[5..], ["token_type"] = "Bearer" }.ToJsonString())
                        : (400, "{\"error\":\"invalid_grant\"}");
                }
                case ("GET", ["users", "@me"]) when auth.StartsWith("Bearer tok-", StringComparison.Ordinal):
                    return (200, new JsonObject { ["id"] = auth[11..], ["username"] = "user" + auth[11..], ["global_name"] = "Delta's operator" }
                        .ToJsonString());
                case ("GET", ["webhooks", HookId, HookToken]):
                    return (200, new JsonObject
                    {
                        ["id"] = HookId, ["type"] = 1, ["guild_id"] = Guild, ["channel_id"] = Channel, ["name"] = "Spidey Bot",
                        ["token"] = HookToken,
                    }.ToJsonString());
                case ("POST", ["webhooks", HookId, HookToken]):
                {
                    var line = JsonNode.Parse(body) as JsonObject ?? [];
                    line["id"] = Id();
                    line["auth"] = auth;
                    said.Add(line);
                    return (200, new JsonObject { ["id"] = line["id"]?.DeepClone(), ["webhook_id"] = HookId }.ToJsonString());
                }
                default:
                    return (404, "{\"message\":\"404: Not Found\",\"code\":0}");
            }
        }
    }

    public void Dispose()
    {
        stop.Cancel();
        listener.Stop();
    }
}

internal static class DiscordMirrorHubChecks
{
    private const string S = "hub/discord-mirrors";
    private const string Site = "https://site.test";
    private const string Operator = "80351110224678940";

    private static readonly LinkTiming Fast = new()
    {
        Backoff = [TimeSpan.FromMilliseconds(200), TimeSpan.FromMilliseconds(300), TimeSpan.FromMilliseconds(500)],
        RefusedWait = TimeSpan.FromSeconds(2),
        ConnectTimeout = TimeSpan.FromSeconds(5),
    };

    private static string? CookieOf(HttpResponseMessage r, string name) =>
        r.Headers.TryGetValues("Set-Cookie", out var all)
            ? all.Select(c => c.Split(';')[0]).FirstOrDefault(c => c.StartsWith(name + "=", StringComparison.Ordinal))
            : null;

    private static string? Query(string? url, string name) =>
        url?.Split('?', 2).ElementAtOrDefault(1)?.Split('&').Select(kv => kv.Split('=', 2))
            .FirstOrDefault(kv => kv.Length == 2 && kv[0] == name)?[1];

    public static async Task Run(Checks c, string exe)
    {
        if (!File.Exists(exe))
        {
            c.Fail(S, "no hub at " + exe);
            return;
        }
        using var discord = new MirrorStandIn();
        using var hub = new HubProcess(exe);
        var key = hub.Register("delta", "Delta");
        hub.Federation("delta", "on");
        hub.Environment["DERETH_HUB_DISCORD_CLIENT_SECRET"] = MirrorStandIn.Secret;
        hub.Environment["DERETH_HUB_DISCORD_BOT_TOKEN"] = MirrorStandIn.BotToken;
        hub.Serve(port =>
        [
            "--allow-loopback", "--public-url", $"http://localhost:{port}", "--site-origin", Site,
            "--discord-client-id", "app", "--discord-api", $"http://127.0.0.1:{discord.Port}/api",
            "--discord-authorize", "https://discord.test/oauth2/authorize",
        ]);
        var baseUri = $"http://127.0.0.1:{hub.Port}";
        using var http = new HttpClient(new SocketsHttpHandler { AllowAutoRedirect = false, UseCookies = false })
        {
            Timeout = TimeSpan.FromSeconds(30),
        };

        // The world half, on its own clock.
        HubUrl.TryLink($"ws://127.0.0.1:{hub.Port}", "delta", true, out var link, out _);
        var clock = Stopwatch.StartNew();
        var w = TestWorld.Make("delta", new HubLink(link, key, (_, _) => { }, Fast), () => clock.Elapsed.TotalSeconds);
        var ann = w.Host.Add(0x5000_0001, "Ann");
        ann.Rooms = [RoomSlot.General, RoomSlot.Roleplay];
        ((HubLink)w.Link).Start();
        async Task<bool> Until(Func<bool> cond, double seconds = 15)
        {
            var until = DateTime.UtcNow.AddSeconds(seconds);
            while (DateTime.UtcNow < until)
            {
                w.Pump();
                if (cond())
                    return true;
                await Task.Delay(20);
            }
            w.Pump();
            return cond();
        }
        c.Check(S, await Until(() => w.World.Link == LinkState.Up), "the world is welcomed");
        c.Check(S, await Until(() => w.World.StoreJson().Contains("\"global\": \"roleplay\"", StringComparison.Ordinal)),
            "a hub holding no room for the world is told Roleplay, the world's setting, keeps it, and says so: " + w.World.StoreJson());
        w.World.Dn(ann.Id, ["on"]);
        List<Shown> RoomsOf(uint who, int from) => w.Host.Shown.Skip(from).Where(x => x.To == who && x.Kind == "room").ToList();

        // Before its operator mirrors it, the hub does not: the room's lines stay here.
        c.Check(S, w.World.MirroredSlots.Count == 0, "the hub says it mirrors none of the world's own rooms");
        w.Host.SayInRoom(ann.Id, RoomSlot.General, "before the hub mirrors it");

        // The world's operator signs in, is handed the world, and mirrors General through its webhook.
        var start = await http.GetAsync($"{baseUri}/v1/auth/discord?return=/");
        var state = Query(start.Headers.Location?.ToString(), "state");
        using var back = new HttpRequestMessage(HttpMethod.Get, $"{baseUri}/v1/auth/discord/callback?code=code-{Operator}&state={state}");
        back.Headers.Add("Cookie", CookieOf(start, "__Host-dn_oauth") ?? "");
        var session = CookieOf(await http.SendAsync(back), "__Host-dn_session");
        c.Check(S, session is not null, "the operator signs in through the stand-in for Discord");
        if (session is null)
            return;
        hub.Assign("delta", Operator);
        var address = $"https://discord.com/api/webhooks/{MirrorStandIn.HookId}/{MirrorStandIn.HookToken}";
        using var put = new HttpRequestMessage(HttpMethod.Put, $"{baseUri}/v1/operator/worlds/delta/discord/mirrors/general")
        {
            Content = new StringContent(new JsonObject { ["webhook"] = address }.ToJsonString(), Encoding.UTF8, "application/json"),
        };
        put.Headers.Add("Cookie", session);
        put.Headers.Add("Origin", Site);
        var mirrored = await http.SendAsync(put);
        var view = await mirrored.Content.ReadAsStringAsync();
        c.Check(S, (int)mirrored.StatusCode == 200 && JsonNode.Parse(view)?["mirrors"]?[0]?["channel"]?.GetValue<string>() == MirrorStandIn.Channel,
            $"the operator mirrors General through the webhook they made ({(int)mirrored.StatusCode}): {view}");
        c.Check(S, !view.Contains(MirrorStandIn.HookToken, StringComparison.Ordinal), "the answer never holds the webhook's token");
        c.Check(S, await Until(() => w.World.MirroredSlots.SequenceEqual([RoomSlot.General])),
            "the hub tells the world at once that it mirrors General");

        // Then the room's lines go.
        var said = "selling scarabs " + Guid.NewGuid().ToString("N")[..8];
        var shown = w.Host.Shown.Count;
        w.Host.SayInRoom(ann.Id, RoomSlot.General, said);
        w.Host.SayInRoom(ann.Id, RoomSlot.Trade, "trade stays here");
        c.Check(S, await Until(() => discord.Said().Any(l => l["content"]?.GetValue<string>() == said)),
            "a line said in the room is said through the webhook");
        var line = discord.Said().First(l => l["content"]?.GetValue<string>() == said);
        c.Check(S, line["username"]?.GetValue<string>() == "Ann@delta" && line["auth"]?.GetValue<string>() == "",
            "under the speaker's address, with no credential but the webhook's own: " + line.ToJsonString());
        await Task.Delay(500);
        w.Pump();
        c.Check(S, RoomsOf(ann.Id, shown) is [{ Name: "Ann", Room: RoomSlot.General }]
            && discord.Said().All(l => l["content"]?.GetValue<string>() is not ("trade stays here" or "before the hub mirrors it")),
            "the line is said here once, never handed back, and nothing of another room or from before reaches Discord");

        // Roleplay carries the Global room here: it is no room of the world's own to mirror.
        async Task<(int Status, string Body)> Put(string path, JsonObject body)
        {
            using var req = new HttpRequestMessage(HttpMethod.Put, $"{baseUri}/v1/operator/worlds/delta/{path}")
            {
                Content = new StringContent(body.ToJsonString(), Encoding.UTF8, "application/json"),
            };
            req.Headers.Add("Cookie", session);
            req.Headers.Add("Origin", Site);
            var r = await http.SendAsync(req);
            return ((int)r.StatusCode, await r.Content.ReadAsStringAsync());
        }
        var refused = await Put("discord/mirrors/roleplay", new JsonObject { ["webhook"] = address });
        c.Check(S, refused.Status == 409 && refused.Body.Contains("carries_global", StringComparison.Ordinal),
            $"a mirror of the room carrying the Global room is refused ({refused.Status}): {refused.Body}");

        // The operator moves the Global room on the site, and the world carries it there at once.
        var moved = await Put("global-room", new JsonObject { ["room"] = "trade" });
        c.Check(S, moved.Status == 200 && await Until(() => w.World.GlobalSlot == RoomSlot.Trade),
            $"the operator moves the Global room to Trade on the site, and the world carries it there ({moved.Status})");

        // The world's operator moves it in game, through the hub.
        var asked = w.World.Admin(["global", "lfg"]);
        c.Check(S, asked.StartsWith("Asking", StringComparison.Ordinal) && await Until(() => w.World.GlobalSlot == RoomSlot.Lfg)
            && await Until(() => w.Logs.Any(l => l.Contains("hub carries this world's Global room in LFG now", StringComparison.Ordinal))),
            "/dn admin global lfg moves it through the hub, and the operator is told: " + asked);

        ((HubLink)w.Link).Stop();
        hub.Kill();
        List<string> hubOut;
        lock (hub.Output)
            hubOut = [.. hub.Output];
        c.Check(S, !hubOut.Concat(w.Logs).Any(l => l.Contains(key, StringComparison.Ordinal) || l.Contains(MirrorStandIn.BotToken, StringComparison.Ordinal)
                || l.Contains(MirrorStandIn.HookToken, StringComparison.Ordinal) || l.Contains(said, StringComparison.Ordinal)),
            "no key, token, webhook or line is in the hub's log or the world's");
    }
}
