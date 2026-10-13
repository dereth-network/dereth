// Tells into a person's Discord end to end against a real hub, over loopback only: the hub runs
// with its sign-in and its Discord broker on, against a stand-in for Discord on 127.0.0.1 (never
// Discord itself) that signs a person in and holds the bot's direct messages. Two world halves are
// linked to the hub: Ann on one asks for her link, which carries its code; the person signs in on
// the site and links her with it, and Ann logs off. There is no setting to turn on. Eve, a stranger
// on the other world, tells Ann and reaches the person, and so does Bob: each is answered as for any
// character not online, Ann's world says she would have been shown the tell, and the bot says it in
// the person's direct messages; the person's reply there comes back to Bob as a tell from Ann. Cy,
// whom Ann squelches, does not reach them. The hub is stopped at the end.

using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace Dereth.Network.Harness;

/// <summary>
/// Discord's API as the hub uses it for sign-in and for the bot's direct messages, on 127.0.0.1:
/// `code-&lt;id&gt;` is exchanged for `tok-&lt;id&gt;` when the client secret is right, and that token
/// reads user `&lt;id&gt;`; the bot opens a user's direct messages, says lines there and reads them
/// after a line; a test writes a line there as the user.
/// </summary>
internal sealed class DirectMessageStandIn : IDisposable
{
    public const string Secret = "the-secret";
    public const string BotToken = "stand-in-bot-token";
    private readonly TcpListener listener = new(IPAddress.Loopback, 0);
    private readonly CancellationTokenSource stop = new();
    private readonly object gate = new();
    private long next = 1_558_376_954_619_629_600;
    /// <summary>The direct messages by channel: the user, and the lines, oldest first.</summary>
    private readonly Dictionary<string, (string User, List<JsonObject> Lines)> channels = new(StringComparer.Ordinal);

    public int Port { get; }

    public DirectMessageStandIn()
    {
        listener.Start();
        Port = ((IPEndPoint)listener.LocalEndpoint).Port;
        _ = Task.Run(Serve);
    }

    private string Id() => (++next).ToString(System.Globalization.CultureInfo.InvariantCulture);

    /// <summary>The bot's lines in `user`'s direct messages, oldest first.</summary>
    public List<string> SaidTo(string user)
    {
        lock (gate)
            return channels.Values.Where(c => c.User == user).SelectMany(c => c.Lines)
                .Where(l => l["author"]?["bot"]?.GetValue<bool>() == true)
                .Select(l => l["content"]?.GetValue<string>() ?? "").ToList();
    }

    /// <summary>`user` writes `content` in its direct messages with the bot.</summary>
    public void Writes(string user, string content)
    {
        lock (gate)
        {
            var (channel, dm) = channels.First(c => c.Value.User == user);
            dm.Lines.Add(new JsonObject
            {
                ["id"] = Id(), ["type"] = 0, ["channel_id"] = channel, ["content"] = content,
                ["author"] = new JsonObject { ["id"] = user, ["username"] = "nelly", ["global_name"] = "Nelly" },
            });
        }
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
        var query = path.Contains('?') ? path[(path.IndexOf('?') + 1)..] : "";
        var segs = (path.Contains('?') ? path[..path.IndexOf('?')] : path).Trim('/').Split('/');
        var bot = auth == "Bot " + BotToken;
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
                case ("POST", ["users", "@me", "channels"]) when bot:
                {
                    var user = JsonNode.Parse(body)?["recipient_id"]?.GetValue<string>() ?? "";
                    var channel = channels.FirstOrDefault(c => c.Value.User == user).Key;
                    if (channel is null)
                    {
                        channel = Id();
                        channels[channel] = (user, []);
                    }
                    return (200, new JsonObject
                    {
                        ["id"] = channel, ["type"] = 1, ["recipients"] = new JsonArray(new JsonObject { ["id"] = user }),
                    }.ToJsonString());
                }
                case ("POST", ["channels", var channel, "messages"]) when bot && channels.ContainsKey(channel):
                {
                    var line = new JsonObject
                    {
                        ["id"] = Id(), ["type"] = 0, ["channel_id"] = channel,
                        ["content"] = JsonNode.Parse(body)?["content"]?.DeepClone(),
                        ["author"] = new JsonObject { ["id"] = "1", ["username"] = "Dereth Network", ["bot"] = true },
                    };
                    channels[channel].Lines.Add(line);
                    return (200, line.ToJsonString());
                }
                case ("GET", ["channels", var channel, "messages"]) when bot && channels.ContainsKey(channel):
                {
                    var after = query.Split('&').Select(kv => kv.Split('=', 2))
                        .Where(kv => kv.Length == 2 && kv[0] == "after").Select(kv => ulong.Parse(kv[1], System.Globalization.CultureInfo.InvariantCulture))
                        .FirstOrDefault();
                    return (200, new JsonArray(channels[channel].Lines
                        .Where(l => ulong.Parse(l["id"]!.GetValue<string>(), System.Globalization.CultureInfo.InvariantCulture) > after)
                        .Select(l => (JsonNode)l.DeepClone()).ToArray()).ToJsonString());
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

internal static class ReachHubChecks
{
    private const string S = "hub/reach";
    private const string Site = "https://site.test";
    private const string Person = "80351110224678930";

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
        using var discord = new DirectMessageStandIn();
        using var hub = new HubProcess(exe);
        var keyA = hub.Register("alpha", "Alpha");
        var keyB = hub.Register("bravo", "Bravo");
        hub.Federation("alpha", "on");
        hub.Federation("bravo", "on");
        hub.Environment["DERETH_HUB_DISCORD_CLIENT_SECRET"] = DirectMessageStandIn.Secret;
        hub.Environment["DERETH_HUB_DISCORD_BOT_TOKEN"] = DirectMessageStandIn.BotToken;
        hub.Serve(port =>
        [
            "--allow-loopback", "--public-url", $"http://localhost:{port}", "--site-origin", Site,
            "--discord-client-id", "app", "--discord-api", $"http://127.0.0.1:{discord.Port}/api",
            "--discord-authorize", "https://discord.test/oauth2/authorize", "--discord-poll-secs", "1",
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
        var bob = b.Host.Add(0x5000_0001, "Bob").Id;
        var cy = b.Host.Add(0x5000_0002, "Cy").Id;
        var eve = b.Host.Add(0x5000_0003, "Eve").Id;
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
        b.World.Dn(bob, ["on"]);
        b.World.Dn(cy, ["on"]);
        b.World.Dn(eve, ["on"]);
        a.World.Squelch(ann, true, 0, "Cy@bravo");
        a.World.Dn(ann, ["link"]);
        string? code = null;
        string? line = null;
        c.Check(S, await Until(() =>
        {
            line = a.Host.To(ann).Where(x => x.Kind == "system").Select(x => x.Text)
                .FirstOrDefault(t => t.StartsWith("Open ", StringComparison.Ordinal) && t.Contains("/link/#", StringComparison.Ordinal));
            code = line?[(line.IndexOf('#') + 1)..][..9];
            return code is not null;
        }), "Ann is given her link, which carries its code");
        c.Check(S, line is not null && line.EndsWith($"/link/#{code} within 10 minutes to link this character to your Discord account. Keep the link to yourself.",
            StringComparison.Ordinal), "in one line: " + line);

        // ---- the person signs in on the site and links Ann, and Ann logs off ---------------------
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
        c.Check(S, await Until(() => a.Host.To(ann).Any(x => x.Text == "This character is now linked to a Discord account.")),
            "Ann, online, is told she is linked");
        a.Host.Characters[ann].Online = false;
        a.World.OnLogout(ann, "Ann");
        a.Pump();
        c.Check(S, await OnSite(HttpMethod.Put, "/v1/person/discord-tells", new JsonObject { ["on"] = true }) == 404,
            "there is no setting for tells on Discord to turn on");

        // ---- Eve, a stranger, tells Ann, away ----------------------------------------------------
        b.World.TellByName(eve, "Ann@alpha", "hi stranger");
        c.Check(S, await Until(() => b.Host.To(eve).Any(x => x.Kind == "not-available") && discord.SaidTo(Person).Count > 0),
            "Eve is answered as for any character not online, and reaches the person");
        c.Check(S, discord.SaidTo(Person) is [var first] && first == "Eve@bravo tells Ann@alpha, \"hi stranger\"\n-# Reply to answer as Ann@alpha.",
            "the bot says it there, naming the teller and the character: " + string.Join(" | ", discord.SaidTo(Person)));

        // ---- Bob tells her -------------------------------------------------------------------------
        var said = "still playing? " + Guid.NewGuid().ToString("N")[..8];
        var shown = b.Host.Shown.Count;
        b.World.TellByName(bob, "Ann@alpha", said);
        c.Check(S, await Until(() => discord.SaidTo(Person).Count > 1
                && b.Host.Shown.Skip(shown).Any(x => x.To == bob && x.Kind == "not-available")),
            "the tell reaches the person's direct messages");
        c.Check(S, discord.SaidTo(Person) is [_, var dm] && dm == $"Bob@bravo tells Ann@alpha, \"{said}\"\n-# Reply to answer as Ann@alpha.",
            "the bot says it there after Eve's: " + string.Join(" | ", discord.SaidTo(Person)));
        c.Check(S, b.Host.Shown.Skip(shown).Where(x => x.To == bob && x.Kind is not ("friend" or "system")).ToList() is [{ Kind: "not-available" }],
            "Bob is answered as for any character not online");

        // ---- the person replies there, and Bob hears it from Ann -----------------------------
        var reply = "yes! come say hi Sat " + Guid.NewGuid().ToString("N")[..8];
        shown = b.Host.Shown.Count;
        discord.Writes(Person, reply);
        c.Check(S, await Until(() => b.Host.Shown.Skip(shown).Any(x => x.To == bob && x.Kind == "tell")),
            "the person's reply comes back to Bob");
        c.Check(S, b.Host.Shown.Skip(shown).Where(x => x.To == bob && x.Kind == "tell").ToList() is [{ Name: "Ann@alpha" } t] && t.Text == reply,
            "as a tell from Ann@alpha");

        // ---- a teller Ann squelches does not reach the person ------------------------------------
        b.World.TellByName(cy, "Ann@alpha", "it's Cy");
        c.Check(S, await Until(() => b.Host.To(cy).Any(x => x.Kind == "not-available")), "Cy is answered");
        b.World.TellByName(bob, "Ann@alpha", "and again");
        c.Check(S, await Until(() => discord.SaidTo(Person).Count > 2), "Bob's next tell, asked about after Cy's, is said there");
        c.Check(S, discord.SaidTo(Person) is [_, _, var again] && again.Contains("and again", StringComparison.Ordinal),
            "and Cy's tell, squelched, is not: " + string.Join(" | ", discord.SaidTo(Person)));

        ((HubLink)a.Link).Stop();
        ((HubLink)b.Link).Stop();
        hub.Kill();
        List<string> logged;
        lock (hub.Output)
            logged = [.. hub.Output, .. a.Logs, .. b.Logs];
        c.Check(S, !logged.Any(l => l.Contains(said, StringComparison.Ordinal) || l.Contains(reply, StringComparison.Ordinal)
                || l.Contains(keyA, StringComparison.Ordinal) || l.Contains(DirectMessageStandIn.BotToken, StringComparison.Ordinal)),
            "no tell, reply, key or token is in the hub's log or the worlds'");
    }
}
