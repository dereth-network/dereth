// Speaking from Discord end to end against a real hub, over loopback only: the hub runs with its
// sign-in and its Discord broker on, against a stand-in for Discord on 127.0.0.1 (never Discord
// itself) that signs a person in, gives the application's public key, takes the hub's `/dn` command,
// holds two webhooks (one in a channel the world's operator mirrors the world's Global room into, one
// in the channel the hub's admins mirror it into) and the edits of interaction answers. The harness
// signs each interaction it hands the hub as Discord does, with a key of its own. Two world halves are
// linked to the hub: Ann on `alpha` asks for her link, and the person opens it on the site; the
// person, alpha's operator, mirrors its Global room; `/dn say` there, with Ann the person's one
// character on alpha, speaks as her with nothing to choose; `/dn identity` there offers Ann and the
// person chooses her; `/dn say` there has alpha say the line as Ann, marked, to its own listeners, to
// `bravo`'s and to both mirrors, and the answer edited to say so; gagged, Ann says nothing and the
// answer says why. A bad signature is refused, PING answered, and the admins' mirror answers that it
// is for reading. The hub is stopped at the end.

using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace Dereth.Network.Harness;

/// <summary>
/// Discord's API as the hub uses it for sign-in, for its application, its command, two webhooks and
/// the answers to interactions, on 127.0.0.1.
/// </summary>
internal sealed class SayStandIn : IDisposable
{
    public const string Secret = "the-secret";
    public const string BotToken = "stand-in-bot-token";
    public const string App = "1558376950000000001";
    public const string Bot = "1558376950000000002";
    public const string Guild = "1558376951000000900";
    public const string WorldChannel = "1558376951000000901";
    public const string WorldHook = "1558376954619629901";
    public const string WorldHookToken = "world-hook-token_Zx9q";
    public const string AdminChannel = "1558376951000000902";
    public const string AdminHook = "1558376954619629902";
    public const string AdminHookToken = "admin-hook-token_Zx9q";
    private readonly TcpListener listener = new(IPAddress.Loopback, 0);
    private readonly CancellationTokenSource stop = new();
    private readonly object gate = new();
    private long next = 1_558_376_954_619_630_000;
    private readonly List<(string Channel, JsonObject Line)> said = [];
    private readonly List<(string Token, string Content)> edits = [];
    private readonly List<JsonNode?> commands = [];
    private readonly string verifyKey;

    public int Port { get; }

    public SayStandIn(string verifyKey)
    {
        this.verifyKey = verifyKey;
        listener.Start();
        Port = ((IPEndPoint)listener.LocalEndpoint).Port;
        _ = Task.Run(Serve);
    }

    private string Id() => (++next).ToString(System.Globalization.CultureInfo.InvariantCulture);

    /// <summary>The lines said through a webhook, oldest first, each with its channel.</summary>
    public List<(string Channel, JsonObject Line)> Said()
    {
        lock (gate)
            return said.Select(x => (x.Channel, (JsonObject)x.Line.DeepClone())).ToList();
    }

    /// <summary>The edits of interaction answers, oldest first: the interaction's token and the content.</summary>
    public List<(string Token, string Content)> Edits()
    {
        lock (gate)
            return [.. edits];
    }

    /// <summary>The commands the hub registered, each time it did.</summary>
    public List<JsonNode?> Commands()
    {
        lock (gate)
            return commands.Select(x => x?.DeepClone()).ToList();
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
        var bot = auth == "Bot " + BotToken;
        JsonObject Hook(string id, string token, string channel) => new()
        {
            ["id"] = id, ["type"] = 1, ["guild_id"] = Guild, ["channel_id"] = channel, ["name"] = "Spidey Bot", ["token"] = token,
        };
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
                    return (200, new JsonObject { ["id"] = auth[11..], ["username"] = "user" + auth[11..], ["global_name"] = "Nelly" }
                        .ToJsonString());
                case ("GET", ["applications", "@me"]) when bot:
                    return (200, new JsonObject
                    {
                        ["id"] = App, ["bot"] = new JsonObject { ["id"] = Bot, ["username"] = "Dereth Network" }, ["verify_key"] = verifyKey,
                    }.ToJsonString());
                case ("PUT", ["applications", App, "commands"]) when bot:
                    commands.Add(JsonNode.Parse(body));
                    return (200, body);
                case ("GET", ["webhooks", WorldHook, WorldHookToken]):
                    return (200, Hook(WorldHook, WorldHookToken, WorldChannel).ToJsonString());
                case ("GET", ["webhooks", AdminHook, AdminHookToken]):
                    return (200, Hook(AdminHook, AdminHookToken, AdminChannel).ToJsonString());
                case ("POST", ["webhooks", var id, var token])
                    when (id, token) is (WorldHook, WorldHookToken) or (AdminHook, AdminHookToken):
                {
                    var line = JsonNode.Parse(body) as JsonObject ?? [];
                    line["id"] = Id();
                    line["auth"] = auth;
                    said.Add((id == WorldHook ? WorldChannel : AdminChannel, line));
                    return (200, new JsonObject { ["id"] = line["id"]?.DeepClone(), ["webhook_id"] = id }.ToJsonString());
                }
                case ("PATCH", ["webhooks", App, var token, "messages", "@original"]):
                    edits.Add((token, JsonNode.Parse(body)?["content"]?.GetValue<string>() ?? ""));
                    return (200, new JsonObject { ["id"] = Id() }.ToJsonString());
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

internal static class SayHubChecks
{
    private const string S = "hub/say";
    private const string Site = "https://site.test";
    private const string Person = "80351110224678950";

    /// <summary>The harness's own signing key, standing in for Discord's.</summary>
    private static readonly byte[] SigningKey = Convert.FromHexString("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb");

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

    private static long nextInteraction = 1_558_376_960_000_000_000;

    /// <summary>An interaction as Discord sends it: `type`, its token, the channel it came from, the user, and `data`.</summary>
    private static (string Token, string Body) Interaction(int type, string channel, JsonObject? data = null)
    {
        var id = Interlocked.Increment(ref nextInteraction).ToString(System.Globalization.CultureInfo.InvariantCulture);
        var token = "aW50ZXJhY3Rpb24-" + id;
        var o = new JsonObject
        {
            ["id"] = id, ["application_id"] = SayStandIn.App, ["type"] = type, ["token"] = token, ["version"] = 1,
            ["guild_id"] = SayStandIn.Guild, ["channel_id"] = channel,
            ["member"] = new JsonObject { ["user"] = new JsonObject { ["id"] = Person, ["username"] = "nelly" } },
        };
        if (data is not null)
            o["data"] = data;
        return (token, o.ToJsonString());
    }

    private static JsonObject Command(string sub, string? text = null)
    {
        var option = new JsonObject { ["type"] = 1, ["name"] = sub };
        if (text is not null)
            option["options"] = new JsonArray(new JsonObject { ["type"] = 3, ["name"] = "text", ["value"] = text });
        return new JsonObject { ["id"] = "1558376950000000010", ["name"] = "dn", ["type"] = 1, ["options"] = new JsonArray(option) };
    }

    public static async Task Run(Checks c, string exe)
    {
        if (!File.Exists(exe))
        {
            c.Fail(S, "no hub at " + exe);
            return;
        }
        using var discord = new SayStandIn(Convert.ToHexStringLower(Ed25519.PublicKey(SigningKey)));
        using var hub = new HubProcess(exe);
        var keyA = hub.Register("alpha", "Alpha");
        var keyB = hub.Register("bravo", "Bravo");
        hub.Federation("alpha", "on");
        hub.Federation("bravo", "on");
        hub.Environment["DERETH_HUB_DISCORD_CLIENT_SECRET"] = SayStandIn.Secret;
        hub.Environment["DERETH_HUB_DISCORD_BOT_TOKEN"] = SayStandIn.BotToken;
        hub.Serve(port =>
        [
            "--allow-loopback", "--public-url", $"http://localhost:{port}", "--site-origin", Site, "--admin", Person,
            "--discord-client-id", "app", "--discord-api", $"http://127.0.0.1:{discord.Port}/api",
            "--discord-authorize", "https://discord.test/oauth2/authorize",
        ]);
        var baseUri = $"http://127.0.0.1:{hub.Port}";
        using var http = new HttpClient(new SocketsHttpHandler { AllowAutoRedirect = false, UseCookies = false })
        {
            Timeout = TimeSpan.FromSeconds(30),
        };

        // ---- the world halves, linked --------------------------------------------------------
        var clock = Stopwatch.StartNew();
        TestWorld World(string slug, string key)
        {
            HubUrl.TryLink($"ws://127.0.0.1:{hub.Port}", slug, true, out var link, out _);
            var w = TestWorld.Make(slug, new HubLink(link, key, (_, _) => { }, Fast), () => clock.Elapsed.TotalSeconds);
            ((HubLink)w.Link).Start();
            return w;
        }
        var a = World("alpha", keyA);
        var b = World("bravo", keyB);
        var ann = a.Host.Add(0x5000_0001, "Ann").Id;
        var cid = a.Host.Add(0x5000_0002, "Cid").Id;
        var bob = b.Host.Add(0x5000_0001, "Bob").Id;
        async Task<bool> Until(Func<bool> cond, double seconds = 15)
        {
            var until = DateTime.UtcNow.AddSeconds(seconds);
            while (DateTime.UtcNow < until)
            {
                a.Pump();
                b.Pump();
                if (cond())
                    return true;
                await Task.Delay(20);
            }
            a.Pump();
            b.Pump();
            return cond();
        }
        c.Check(S, await Until(() => a.World.Link == LinkState.Up && b.World.Link == LinkState.Up), "both worlds are welcomed");
        a.World.Dn(ann, ["on"]);
        a.World.Dn(ann, ["link"]);
        string? code = null;
        c.Check(S, await Until(() =>
        {
            var line = a.Host.To(ann).Where(x => x.Kind == "system").Select(x => x.Text)
                .FirstOrDefault(t => t.StartsWith("Open ", StringComparison.Ordinal) && t.Contains("/link/#", StringComparison.Ordinal));
            code = line?[(line.IndexOf('#') + 1)..][..9];
            return code is not null;
        }), "Ann is given her link, which carries its code");

        // ---- the person signs in, links Ann, and, as alpha's operator and an admin, mirrors Global
        var start = await http.GetAsync($"{baseUri}/v1/auth/discord?return=/");
        var state = Query(start.Headers.Location?.ToString(), "state");
        using var back = new HttpRequestMessage(HttpMethod.Get, $"{baseUri}/v1/auth/discord/callback?code=code-{Person}&state={state}");
        back.Headers.Add("Cookie", CookieOf(start, "__Host-dn_oauth") ?? "");
        var session = CookieOf(await http.SendAsync(back), "__Host-dn_session");
        c.Check(S, session is not null, "the person signs in through the stand-in for Discord");
        if (session is null || code is null)
            return;
        async Task<int> OnSite(HttpMethod method, string route, JsonObject body)
        {
            using var ask = new HttpRequestMessage(method, baseUri + route)
            {
                Content = new StringContent(body.ToJsonString(), Encoding.UTF8, "application/json"),
            };
            ask.Headers.Add("Cookie", session);
            ask.Headers.Add("Origin", Site);
            return (int)(await http.SendAsync(ask)).StatusCode;
        }
        c.Check(S, await OnSite(HttpMethod.Post, "/v1/person/link", new JsonObject { ["code"] = code }) == 200, "the person links Ann with her code");
        hub.Assign("alpha", Person);
        c.Check(S, await OnSite(HttpMethod.Put, "/v1/operator/worlds/alpha/discord/mirrors/global", new JsonObject
            {
                ["webhook"] = $"https://discord.com/api/webhooks/{SayStandIn.WorldHook}/{SayStandIn.WorldHookToken}",
            }) == 200, "alpha's operator mirrors its Global room into a channel of its server");
        c.Check(S, await OnSite(HttpMethod.Put, "/v1/admin/discord/mirrors/global", new JsonObject
            {
                ["webhook"] = $"https://discord.com/api/webhooks/{SayStandIn.AdminHook}/{SayStandIn.AdminHookToken}",
            }) == 200, "the admins mirror the Global room into the network's own channel");

        // ---- Discord's interactions, signed as Discord signs them --------------------------------
        async Task<(int Status, JsonNode? Body)> Post(string body, bool goodSignature = true)
        {
            var timestamp = DateTimeOffset.UtcNow.ToUnixTimeSeconds().ToString(System.Globalization.CultureInfo.InvariantCulture);
            var signature = Ed25519.Sign(SigningKey, Encoding.UTF8.GetBytes(timestamp + body));
            if (!goodSignature)
                signature[0] ^= 1;
            using var ask = new HttpRequestMessage(HttpMethod.Post, baseUri + "/v1/discord/interactions")
            {
                Content = new StringContent(body, Encoding.UTF8, "application/json"),
            };
            ask.Headers.Add("X-Signature-Ed25519", Convert.ToHexStringLower(signature));
            ask.Headers.Add("X-Signature-Timestamp", timestamp);
            var r = await http.SendAsync(ask);
            var text = await r.Content.ReadAsStringAsync();
            JsonNode? json = null;
            try
            {
                json = text.Length > 0 ? JsonNode.Parse(text) : null;
            }
            catch (System.Text.Json.JsonException)
            {
            }
            return ((int)r.StatusCode, json);
        }
        // The hub reads the application's key and registers its command at its start: an interaction
        // is answered once it has. Each interaction has an id of its own, which the hub takes once.
        c.Check(S, await Until(() => discord.Commands().Count > 0), "the hub registers its command at its start");
        var pong = (0, (JsonNode?)null);
        c.Check(S, await Until(() =>
        {
            pong = Post(Interaction(1, SayStandIn.WorldChannel).Body).GetAwaiter().GetResult();
            return pong.Item1 == 200;
        }) && pong.Item2?["type"]?.GetValue<int>() == 1, "a signed PING is answered PONG, once the hub has the application's key");
        c.Check(S, (await Post(Interaction(1, SayStandIn.WorldChannel).Body, goodSignature: false)).Status == 401,
            "an interaction whose signature is not Discord's is refused 401");
        c.Check(S, discord.Commands().Any(x => x is JsonArray cmds && cmds.Any(cmd => cmd?["name"]?.GetValue<string>() == "dn"
                && cmd["options"] is JsonArray subs
                && subs.Select(o => o?["name"]?.GetValue<string>()).Order().SequenceEqual(["identity", "say"]))),
            "the hub registered /dn with its two subcommands: " + string.Join(" | ", discord.Commands().Select(x => x?.ToJsonString())));

        var (earlyToken, earlyBody) = Interaction(2, SayStandIn.WorldChannel, Command("say", "first line"));
        var early = await Post(earlyBody);
        c.Check(S, early.Status == 200 && early.Body?["type"]?.GetValue<int>() == 5 && early.Body?["data"]?["flags"]?.GetValue<int>() == 64,
            "with one character linked on alpha and none chosen, /dn say speaks as it, deferring at once: " + early.Body?.ToJsonString());
        c.Check(S, await Until(() => discord.Edits().Any(e => e.Token == earlyToken)
                && b.Host.Shown.Any(x => x.To == bob && x.Kind == "room" && x.Text == "first line")),
            "the line is said on both worlds");
        c.Check(S, discord.Edits().Where(e => e.Token == earlyToken).Select(e => e.Content).ToList() is ["Said as Ann@alpha."],
            "as Ann, with nothing to choose: " + string.Join(" | ", discord.Edits().Select(e => e.Content)));

        var (_, menuBody) = Interaction(2, SayStandIn.WorldChannel, Command("identity"));
        var menu = await Post(menuBody);
        var again = await Post(menuBody);
        c.Check(S, again.Status == 401, $"an interaction the hub took, sent again, is refused 401 ({again.Status})");
        var options = menu.Body?["data"]?["components"]?[0]?["components"]?[0]?["options"] as JsonArray;
        c.Check(S, menu.Status == 200 && menu.Body?["type"]?.GetValue<int>() == 4 && menu.Body?["data"]?["flags"]?.GetValue<int>() == 64
                && menu.Body?["data"]?["components"]?[0]?["components"]?[0]?["custom_id"]?.GetValue<string>() == "dn-identity"
                && options is not null && options.Select(o => o?["value"]?.GetValue<string>()).SequenceEqual(["Ann@alpha"]),
            "/dn identity offers the person, alone, its character on alpha: " + menu.Body?.ToJsonString());
        var chose = await Post(Interaction(3, SayStandIn.WorldChannel,
            new JsonObject { ["custom_id"] = "dn-identity", ["component_type"] = 3, ["values"] = new JsonArray("Ann@alpha") }).Body);
        c.Check(S, chose.Status == 200 && chose.Body?["type"]?.GetValue<int>() == 7
                && chose.Body?["data"]?["content"]?.GetValue<string>()?.Contains("Ann@alpha", StringComparison.Ordinal) == true
                && chose.Body?["data"]?["components"] is JsonArray { Count: 0 },
            "choosing Ann answers who the person speaks as now: " + chose.Body?.ToJsonString());

        // ---- /dn say, through alpha ------------------------------------------------------------
        var line = "evening all " + Guid.NewGuid().ToString("N")[..8];
        var (sayToken, sayBody) = Interaction(2, SayStandIn.WorldChannel, Command("say", line));
        var shownA = a.Host.Shown.Count;
        var shownB = b.Host.Shown.Count;
        var deferred = await Post(sayBody);
        c.Check(S, deferred.Status == 200 && deferred.Body?["type"]?.GetValue<int>() == 5 && deferred.Body?["data"]?["flags"]?.GetValue<int>() == 64,
            "/dn say is answered at once by deferring, to the person alone: " + deferred.Body?.ToJsonString());
        c.Check(S, await Until(() => discord.Edits().Any(e => e.Token == sayToken)
                && discord.Said().Count(s => s.Line["content"]?.GetValue<string>() == line) == 2
                && b.Host.Shown.Skip(shownB).Any(x => x.To == bob && x.Kind == "room")),
            "the line is said, both mirrors carry it, and the answer is edited");
        c.Check(S, discord.Edits().Where(e => e.Token == sayToken).Select(e => e.Content).ToList() is ["Said as Ann@alpha."],
            "the answer says it was said as Ann: " + string.Join(" | ", discord.Edits().Select(e => e.Content)));
        c.Check(S, a.Host.Shown.Skip(shownA).Where(x => x.Kind == "room").ToList() is { Count: 2 } local
                && local.All(x => x.Name == "Ann (Discord)" && x.Id == ann && x.Text == line) && local.Any(x => x.To == cid),
            "alpha says it to its own listeners as Ann's line, under her id, marked");
        c.Check(S, b.Host.Shown.Skip(shownB).Where(x => x.Kind == "room").ToList() is [{ Name: "Ann@alpha (Discord)" } remote]
                && remote.Text == line,
            "bravo shows it as a line from Ann@alpha, marked");
        c.Check(S, discord.Said().Where(s => s.Line["content"]?.GetValue<string>() == line)
                .Select(s => (s.Channel, s.Line["username"]?.GetValue<string>())).Order().ToList()
                is [(SayStandIn.WorldChannel, "Ann@alpha (via /dn)"), (SayStandIn.AdminChannel, "Ann@alpha (via /dn)")],
            "every mirror of the room, the channel it was said in among them, names Ann marked: "
            + string.Join(" | ", discord.Said().Select(s => s.Line.ToJsonString())));

        // ---- gagged, Ann says nothing, and the answer says why ---------------------------------
        a.Host.Characters[ann].Gagged = true;
        var gagged = "not said " + Guid.NewGuid().ToString("N")[..8];
        var (gagToken, gagBody) = Interaction(2, SayStandIn.WorldChannel, Command("say", gagged));
        shownA = a.Host.Shown.Count;
        shownB = b.Host.Shown.Count;
        var deferredAgain = await Post(gagBody);
        c.Check(S, deferredAgain.Body?["type"]?.GetValue<int>() == 5, "a gagged character's line is deferred as any");
        c.Check(S, await Until(() => discord.Edits().Any(e => e.Token == gagToken)), "and its answer edited");
        c.Check(S, discord.Edits().Where(e => e.Token == gagToken).Select(e => e.Content).ToList() is [var why]
                && why.StartsWith("Ann@alpha did not say it:", StringComparison.Ordinal) && why.Contains("gagged", StringComparison.Ordinal),
            "the answer says Ann did not say it, gagged: " + string.Join(" | ", discord.Edits().Select(e => e.Content)));
        await Task.Delay(500);
        a.Pump();
        b.Pump();
        c.Check(S, discord.Said().All(s => s.Line["content"]?.GetValue<string>() != gagged)
                && a.Host.Shown.Skip(shownA).All(x => x.Kind != "room") && b.Host.Shown.Skip(shownB).All(x => x.Kind != "room"),
            "and nothing is said anywhere");
        a.Host.Characters[ann].Gagged = false;

        // ---- the admins' mirror is for reading -------------------------------------------------
        var admins = await Post(Interaction(2, SayStandIn.AdminChannel, Command("say", "from the network's channel")).Body);
        c.Check(S, admins.Status == 200 && admins.Body?["type"]?.GetValue<int>() == 4 && admins.Body?["data"]?["flags"]?.GetValue<int>() == 64
                && admins.Body?["data"]?["content"]?.GetValue<string>()?.Contains("for everyone to read", StringComparison.Ordinal) == true,
            "/dn say in the admins' mirror answers, to the person alone, that it is for reading: " + admins.Body?.ToJsonString());

        ((HubLink)a.Link).Stop();
        ((HubLink)b.Link).Stop();
        hub.Kill();
        List<string> hubOut;
        lock (hub.Output)
            hubOut = [.. hub.Output];
        c.Check(S, !hubOut.Concat(a.Logs).Concat(b.Logs).Any(l => l.Contains(keyA, StringComparison.Ordinal)
                || l.Contains(SayStandIn.BotToken, StringComparison.Ordinal) || l.Contains(sayToken, StringComparison.Ordinal)
                || l.Contains(line, StringComparison.Ordinal) || l.Contains(Person, StringComparison.Ordinal)),
            "no key, token, interaction token, line or Discord id is in the hub's log or the worlds'");
    }
}
