// `/dn verify` end to end against a real hub, over loopback only: the hub runs with its operator
// API on, signing in through a stand-in for Discord on 127.0.0.1; an operator signs in, registers a
// world at a game port the harness listens on, and the world half verifies it in game. The hub's
// challenge arrives on that port, is handed to the world half as the mod's hook hands it, and its
// nonce goes back to the hub over HTTP from the harness. The hub is stopped at the end.

using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace Dereth.Network.Harness;

/// <summary>
/// Discord's API as the hub uses it, on 127.0.0.1: `code-&lt;id&gt;` is exchanged for
/// `tok-&lt;id&gt;` when the client secret is right, and that token reads user `&lt;id&gt;`.
/// </summary>
internal sealed class DiscordStandIn : IDisposable
{
    public const string Secret = "the-secret";
    private readonly TcpListener listener = new(IPAddress.Loopback, 0);
    private readonly CancellationTokenSource stop = new();

    public int Port { get; }

    public DiscordStandIn()
    {
        listener.Start();
        Port = ((IPEndPoint)listener.LocalEndpoint).Port;
        _ = Task.Run(Serve);
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

    private static void Answer(TcpClient client)
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
        var form = Encoding.UTF8.GetString(body).Split('&').Select(kv => kv.Split('=', 2))
            .Where(kv => kv.Length == 2).ToDictionary(kv => kv[0], kv => Uri.UnescapeDataString(kv[1]));
        (int Status, string Body) reply = (first.ElementAtOrDefault(0), first.ElementAtOrDefault(1)) switch
        {
            ("POST", "/api/oauth2/token") when form.TryGetValue("code", out var code) && code.StartsWith("code-", StringComparison.Ordinal)
                                               && form.GetValueOrDefault("client_secret") == Secret =>
                (200, new JsonObject { ["access_token"] = "tok-" + code[5..], ["token_type"] = "Bearer" }.ToJsonString()),
            ("GET", "/api/users/@me") when Header("Authorization") is { } auth && auth.StartsWith("Bearer tok-", StringComparison.Ordinal) =>
                (200, new JsonObject { ["id"] = auth[11..], ["username"] = "user" + auth[11..], ["global_name"] = "Operator " + auth[11..] }
                    .ToJsonString()),
            _ => (400, "{\"error\":\"invalid_grant\"}"),
        };
        var bytes = Encoding.UTF8.GetBytes(reply.Body);
        stream.Write(Encoding.ASCII.GetBytes(
            $"HTTP/1.1 {reply.Status} X\r\nContent-Type: application/json\r\nContent-Length: {bytes.Length}\r\nConnection: close\r\n\r\n"));
        stream.Write(bytes);
    }

    public void Dispose()
    {
        stop.Cancel();
        listener.Stop();
    }
}

internal static class VerifyHubChecks
{
    private const string S = "hub/verify";
    private const string Site = "https://site.test";
    // A Discord account made in 2015: old enough to register a world.
    private const string DiscordId = "80351110224678912";

    private static string? CookieOf(HttpResponseMessage r, string name) =>
        r.Headers.TryGetValues("Set-Cookie", out var all)
            ? all.Select(c => c.Split(';')[0]).FirstOrDefault(c => c.StartsWith(name + "=", StringComparison.Ordinal))
            : null;

    public static async Task Run(Checks c, string exe)
    {
        if (!File.Exists(exe))
        {
            c.Fail(S, "no hub at " + exe);
            return;
        }
        using var discord = new DiscordStandIn();
        using var hub = new HubProcess(exe);
        hub.Environment["DERETH_HUB_DISCORD_CLIENT_SECRET"] = DiscordStandIn.Secret;
        hub.Serve(port =>
        [
            // The hub names itself by another spelling of loopback than the world's endpoint uses: an
            // endpoint may never be a name the hub answers on.
            "--allow-loopback", "--public-url", $"http://localhost:{port}", "--site-origin", Site,
            "--discord-client-id", "app", "--discord-api", $"http://127.0.0.1:{discord.Port}/api",
            "--discord-authorize", "https://discord.test/oauth2/authorize",
        ]);
        var baseUri = $"http://127.0.0.1:{hub.Port}";
        using var http = new HttpClient(new SocketsHttpHandler { AllowAutoRedirect = false, UseCookies = false })
        {
            Timeout = TimeSpan.FromSeconds(60),
        };

        // ---- an operator signs in and registers a world at the harness's game port --------------
        var start = await http.GetAsync($"{baseUri}/v1/auth/discord?return=/operator/");
        var state = start.Headers.Location?.ToString().Split('&').Select(kv => kv.Split('=', 2))
            .FirstOrDefault(kv => kv[0].EndsWith("state", StringComparison.Ordinal) && kv.Length == 2)?[1];
        var oauth = CookieOf(start, "__Host-dn_oauth");
        using var back = new HttpRequestMessage(HttpMethod.Get, $"{baseUri}/v1/auth/discord/callback?code=code-{DiscordId}&state={state}");
        back.Headers.Add("Cookie", oauth ?? "");
        var signedIn = await http.SendAsync(back);
        var session = CookieOf(signedIn, "__Host-dn_session");
        c.Check(S, (int)start.StatusCode == 302 && state is not null && (int)signedIn.StatusCode == 303 && session is not null,
            $"an operator signs in through the stand-in for Discord ({(int)start.StatusCode}, {(int)signedIn.StatusCode})");
        if (session is null)
            return;

        using var gamePort = new UdpClient(new IPEndPoint(IPAddress.Loopback, 0));
        var port = ((IPEndPoint)gamePort.Client.LocalEndPoint!).Port;
        const string slug = "vworld";
        using var register = new HttpRequestMessage(HttpMethod.Post, $"{baseUri}/v1/operator/worlds")
        {
            Content = new StringContent(new JsonObject
            {
                ["slug"] = slug, ["name"] = "Verified World", ["ruleset"] = "PvE",
                ["endpoint"] = new JsonObject { ["mode"] = "manual", ["address"] = "127.0.0.1", ["port"] = port },
                ["emulator"] = new JsonObject { ["family"] = "ACE" },
            }.ToJsonString(), Encoding.UTF8, "application/json"),
        };
        register.Headers.Add("Cookie", session);
        register.Headers.Add("Origin", Site);
        var registered = await http.SendAsync(register);
        var registeredBody = await registered.Content.ReadAsStringAsync();
        var answer = JsonNode.Parse(registeredBody);
        var key = answer?["key"]?.GetValue<string>();
        var token = answer?["verification"]?["token"]?.GetValue<string>();
        c.Check(S, (int)registered.StatusCode == 201 && key is not null && token is not null,
            $"the world is registered, pending, with its key and token ({(int)registered.StatusCode}: {(key is null ? registeredBody : "")})");
        if (key is null || token is null)
            return;
        var pending = await http.GetAsync($"{baseUri}/v1/servers/{slug}");
        c.Check(S, (int)pending.StatusCode == 404, "a pending world is in no public answer");

        // ---- the world half, with the mod's own verification and answer calls -------------------
        var clock = System.Diagnostics.Stopwatch.StartNew();
        var w = TestWorld.Make(slug, new MemoryLink(), () => clock.Elapsed.TotalSeconds);
        var boss = w.Host.Add(0x5000_0002, "+Boss").Id;
        var route = HubVerify.RouteFor(new Uri($"ws://127.0.0.1:{hub.Port}/v1/servers/{slug}/link"));
        var answerRoute = HubVerify.AnswerRouteFor(route);
        w.World.Verifier = r => HubVerify.SendAsync(http, route, key, r, CancellationToken.None);
        w.World.Answerer = nonce => HubVerify.AnswerAsync(http, answerRoute, key, nonce, CancellationToken.None);
        c.Check(S, answerRoute.ToString() == $"http://127.0.0.1:{hub.Port}/v1/servers/{slug}/verify/answer",
            "the answer goes beside the verification route");

        // What the game port receives goes to the world half, as the mod's hook hands it; polled
        // without blocking, since a read that times out can lose a datagram on Windows.
        var heard = 0;
        using var listening = new CancellationTokenSource();
        var listener = Task.Run(async () =>
        {
            while (!listening.IsCancellationRequested)
            {
                if (gamePort.Available > 0)
                {
                    var from = new IPEndPoint(IPAddress.Any, 0);
                    var datagram = gamePort.Receive(ref from);
                    if (w.World.OnChallengeDatagram(datagram))
                        Interlocked.Increment(ref heard);
                    continue;
                }
                await Task.Delay(5);
            }
        });

        async Task<bool> Shown(Func<string, bool> line, double seconds)
        {
            var until = DateTime.UtcNow.AddSeconds(seconds);
            while (DateTime.UtcNow < until)
            {
                w.Pump();
                if (w.Host.To(boss).Any(x => x.Kind == "system" && line(x.Text)))
                    return true;
                await Task.Delay(10);
            }
            w.Pump();
            return w.Host.To(boss).Any(x => x.Kind == "system" && line(x.Text));
        }

        var started = DateTime.UtcNow;
        c.Check(S, w.World.Verify(boss, "+Boss", ["dnv_" + new string('Q', 32)]) == "Asking the Dereth Network's hub to verify this world...",
            "a wrong token goes");
        c.Check(S, await Shown(t => t == "The Dereth Network did not verify this world: That is not this world's verification token.", 10)
            && DateTime.UtcNow - started < TimeSpan.FromSeconds(5) && Volatile.Read(ref heard) == 0,
            "and is refused at once, with no challenge sent");

        c.Check(S, w.World.Verify(boss, "+Boss", [token]) == "Asking the Dereth Network's hub to verify this world...", "the token goes");
        c.Check(S, await Shown(t => t == "The Dereth Network verified this world. It is on the network now; the link comes up within a minute.", 30),
            "the hub's challenge is heard on the game port and answered, and the world is verified: " +
            string.Join(" | ", w.Host.To(boss).Select(x => x.Text)));
        c.Check(S, Volatile.Read(ref heard) >= 1, $"the challenge came to the game port ({heard} heard)");
        var listed = await http.GetAsync($"{baseUri}/v1/servers/{slug}");
        c.Check(S, (int)listed.StatusCode == 200, $"the verified world is in the public answers ({(int)listed.StatusCode})");
        c.Check(S, w.Logs.Any(l => l.Contains("the hub took this world's answer to its challenge", StringComparison.Ordinal)),
            "the answer's outcome is logged");
        listening.Cancel();
        await listener;

        // A token is spent by its use; the world half answers nothing once the hub has answered.
        c.Check(S, w.World.OnChallengeDatagram(new Challenge(slug, new byte[Challenge.NonceLength]).Encode()!), "a late challenge is still the network's");
        await Task.Delay(200);
        c.Check(S, w.Logs.Count(l => l.Contains("challenge", StringComparison.Ordinal)) == 1,
            "and is not answered once the hub's answer is shown");
        hub.Kill();
    }
}
