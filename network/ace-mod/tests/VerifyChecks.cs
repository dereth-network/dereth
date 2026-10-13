// `/dn verify`: the token goes to the hub's verification route with the world's key, off the world
// thread, and the hub's answer is shown on a later pump; meanwhile the hub's challenge on the game
// port has its nonce posted back. The hub here is a socket on 127.0.0.1 that reads the request and
// answers as the hub does.

using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace Dereth.Network.Harness;

internal static class VerifyChecks
{
    private const string S = "units/verify";

    public static void Run(Checks c)
    {
        Routes(c);
        Lines(c);
        Codec(c);
        World(c);
        ChallengeRules(c);
        OverHttp(c);
    }

    private static void Routes(Checks c)
    {
        c.Check(S, HubVerify.RouteFor(new Uri("wss://hub.dereth.network/v1/servers/aa/link")).ToString()
            == "https://hub.dereth.network/v1/servers/aa/verify", "the route is the link's host over https");
        c.Check(S, HubVerify.RouteFor(new Uri("wss://example.org:9443/net/v1/servers/aa/link")).ToString()
            == "https://example.org:9443/net/v1/servers/aa/verify", "with its port and prefix");
        c.Check(S, HubVerify.RouteFor(new Uri("ws://127.0.0.1:8650/v1/servers/aa/link")).ToString()
            == "http://127.0.0.1:8650/v1/servers/aa/verify", "a loopback ws:// hub over http");
        c.Check(S, HubVerify.AnswerRouteFor(HubVerify.RouteFor(new Uri("wss://example.org:9443/net/v1/servers/aa/link"))).ToString()
            == "https://example.org:9443/net/v1/servers/aa/verify/answer", "the answer route is the verification route's /answer");
        c.Check(S, HubVerify.IsToken("dnv_" + new string('Q', 32)) && !HubVerify.IsToken("dnv_short")
            && !HubVerify.IsToken("drth_" + new string('Q', 31)), "a token is dnv_ and 32 characters of base64url");
    }

    private static void Lines(Checks c)
    {
        c.Check(S, HubVerify.Line(200, "{\"approval\":\"approved\"}").StartsWith("The Dereth Network verified this world. It is on the network now", StringComparison.Ordinal),
            "approved: on the network");
        c.Check(S, HubVerify.Line(200, "{\"approval\":\"rate_limited\"}").Contains("allowance", StringComparison.Ordinal), "the allowance spent");
        c.Check(S, HubVerify.Line(401, "{}").Contains("does not know this world's key", StringComparison.Ordinal), "a wrong key");
        c.Check(S, HubVerify.Line(422, "{\"error\":{\"message\":\"That is not this world's verification token\\nX\"}}")
            == "The Dereth Network did not verify this world: That is not this world's verification tokenX.",
            "the hub's reason, without its control characters");
        c.Check(S, HubVerify.Line(503, "not json") == "The Dereth Network did not verify this world: the hub answered 503.", "an answer that is not JSON");
        const string noAnswer =
            "The hub's challenge to retail.dereth.network:19000 (99.113.32.94) got no answer: is the game port reachable from the internet?";
        c.Check(S, HubVerify.Line(422, new JsonObject { ["error"] = new JsonObject { ["message"] = noAnswer, ["reason"] = "no_answer" } }.ToJsonString())
            == "The Dereth Network did not verify this world: " + noAnswer, "a message that ends as a sentence gets no period after it");
    }

    private static byte[] Nonce(byte fill) => Enumerable.Repeat(fill, Challenge.NonceLength).ToArray();

    private static void Codec(Checks c)
    {
        const string s = "units/challenge";
        var d = new Challenge("coldeve", Nonce(0xA5)).Encode()!;
        c.Check(s, d.Length == Challenge.HeaderLength + 2 + Challenge.NonceLength + 1 + 7 && Challenge.IsChallenge(d)
            && d[16] == 0xFF && d[17] == 0xFF && d.AsSpan(4, 12).IndexOfAnyExcept((byte)0) < 0,
            "a challenge is a packet header with the magic, no flags and a size larger than itself");
        var back = Challenge.TryParse(d);
        c.Check(s, back is { Slug: "coldeve" } && back.Nonce.SequenceEqual(Nonce(0xA5)), "it reads back");
        var later = d.Concat(new byte[] { 9, 9, 9 }).ToArray();
        later[Challenge.HeaderLength + 1] = 2;
        c.Check(s, Challenge.TryParse(later)?.Slug == "coldeve", "a later version's appended bytes are ignored");
        var ping = (byte[])d.Clone();
        "ACSP"u8.CopyTo(ping);
        c.Check(s, !Challenge.IsChallenge(ping) && Challenge.TryParse(ping) is null, "the status ping is not a challenge");
        c.Check(s, new Challenge("Upper", Nonce(1)).Encode() is null && new Challenge(new string('a', 41), Nonce(1)).Encode() is null
            && new Challenge("", Nonce(1)).Encode() is null && new Challenge("aa", new byte[3]).Encode() is null,
            "a slug or nonce it cannot carry is not written");
        c.Check(s, new Challenge(new string('a', 40), Nonce(1)).Encode()!.Length == Challenge.MaxLength, "the longest slug fits");
        var hex = Challenge.NonceHex(Enumerable.Range(0, 16).Select(i => (byte)(i * 17)).ToArray());
        c.Check(s, hex == "00112233445566778899aabbccddeeff" && Challenge.ParseNonceHex(hex.ToUpperInvariant()) is { } n
            && Challenge.NonceHex(n) == hex, "a nonce is posted back as 32 lowercase hex digits, read in either case");
        c.Check(s, Challenge.ParseNonceHex("not a nonce") is null && Challenge.ParseNonceHex(hex + "0") is null
            && Challenge.ParseNonceHex(hex.Replace('a', 'g')) is null, "and nothing else reads as one");
    }

    /// <summary>
    /// The world half's rules for the hub's challenge: it answers only while a `/dn verify` it sent
    /// waits, within the window, for its own slug, each nonce once and at most a few; and it says
    /// which datagrams are challenges, so the server's packet reader skips only those.
    /// </summary>
    private static void ChallengeRules(Checks c)
    {
        const string s = "units/challenge";
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now);
        var boss = w.Host.Add(0x5000_0002, "+Boss").Id;
        var token = "dnv_" + new string('Q', 32);
        var posted = new System.Collections.Concurrent.ConcurrentQueue<string>();
        var release = new TaskCompletionSource<string>();
        w.World.Verifier = _ => release.Task;
        w.World.Answerer = n =>
        {
            posted.Enqueue(n);
            return Task.FromResult("the hub took this world's answer to its challenge");
        };
        int Posted(int want)
        {
            var until = DateTime.UtcNow.AddSeconds(2);
            while (posted.Count < want && DateTime.UtcNow < until)
                Thread.Sleep(5);
            Thread.Sleep(50);
            return posted.Count;
        }
        byte[] To(string slug, byte fill) => new Challenge(slug, Nonce(fill)).Encode()!;

        c.Check(s, !w.World.OnChallengeDatagram(new byte[40]) && !w.World.OnChallengeDatagram("DN"u8),
            "a datagram that is not a challenge is left to the server");
        c.Check(s, w.World.OnChallengeDatagram(To("alpha", 1)) && Posted(1) == 0,
            "a challenge with no verification waiting is taken from the server and ignored");

        c.Check(s, w.World.Verify(boss, "+Boss", [token]) == "Asking the Dereth Network's hub to verify this world...", "a verification waits");
        c.Check(s, w.World.OnChallengeDatagram(To("bravo", 2)) && Posted(1) == 0, "another world's challenge is ignored");
        var cut = To("alpha", 2)[..30];
        c.Check(s, w.World.OnChallengeDatagram(cut) && Posted(1) == 0, "a challenge cut short is ignored");
        w.World.OnChallengeDatagram(To("alpha", 3));
        w.World.OnChallengeDatagram(To("alpha", 3));
        c.Check(s, Posted(1) == 1 && posted.TryPeek(out var first) && first == Challenge.NonceHex(Nonce(3)),
            "this world's challenge has its nonce posted back, once however many copies come");
        for (byte fill = 10; fill < 30; fill++)
            w.World.OnChallengeDatagram(To("alpha", fill));
        c.Check(s, Posted(Challenge.MaxAnswers) == Challenge.MaxAnswers,
            $"at most {Challenge.MaxAnswers} nonces are posted for one verification ({posted.Count})");
        c.Check(s, w.Logs.Any(l => l.Contains("the hub took this world's answer", StringComparison.Ordinal)),
            "how an answer went is logged");

        // The hub answers: nothing more is posted until the next verification.
        release.SetResult("The Dereth Network verified this world.");
        var until = DateTime.UtcNow.AddSeconds(5);
        while (!w.Host.To(boss).Any(x => x.Text == "The Dereth Network verified this world.") && DateTime.UtcNow < until)
        {
            Thread.Sleep(5);
            w.Pump();
        }
        var before = posted.Count;
        w.World.OnChallengeDatagram(To("alpha", 40));
        c.Check(s, Posted(before + 1) == before, "once the hub's answer is shown, a challenge is ignored");

        // A verification waits past the window: its challenges are ignored.
        w.World.Verifier = _ => new TaskCompletionSource<string>().Task;
        c.Check(s, w.World.Verify(boss, "+Boss", [token]) == "Asking the Dereth Network's hub to verify this world...", "the next verification");
        clock.Now += Challenge.WindowSecs + 1;
        w.World.OnChallengeDatagram(To("alpha", 41));
        c.Check(s, Posted(before + 1) == before, "a challenge past the window after /dn verify is ignored");
        clock.Now -= Challenge.WindowSecs;
        w.World.OnChallengeDatagram(To("alpha", 42));
        c.Check(s, Posted(before + 1) == before + 1, "and one within it is answered");
    }

    private sealed class Clock
    {
        public double Now = 1000;
    }

    private static void World(Checks c)
    {
        var clock = new Clock();
        var w = TestWorld.Make("alpha", new MemoryLink(), () => clock.Now);
        var boss = w.Host.Add(0x5000_0002, "+Boss").Id;
        var token = "dnv_" + new string('Q', 32);
        c.Check(S, w.World.Verify(boss, "+Boss", [token]) == "This world cannot reach the hub to verify.", "no route, no verification");
        var release = new TaskCompletionSource<string>();
        VerifyRequest? seen = null;
        w.World.Verifier = r =>
        {
            seen = r;
            return release.Task;
        };
        c.Check(S, w.World.Verify(boss, "+Boss", ["not-a-token"]) == "@dn verify <token> - verify this world with the token the site gave you", "a bad token is refused here");
        c.Check(S, w.World.Verify(boss, "+Boss", [token]) == NetworkWorld.CannotHearLine && seen is null,
            "a server whose game port cannot be heard says so, and sends nothing");
        w.World.Answerer = _ => Task.FromResult("");
        c.Check(S, w.World.Verify(boss, "+Boss", [token]) == "Asking the Dereth Network's hub to verify this world...", "the token goes");
        c.Check(S, w.World.Verify(boss, "+Boss", [token]) == "A verification is on its way to the hub already.", "one at a time");
        var until = DateTime.UtcNow.AddSeconds(5);
        while (seen is null && DateTime.UtcNow < until)
            Thread.Sleep(5);
        c.Check(S, seen is { } r && r.Token == token && r.By == "Boss" && r.Player == boss, "the token, who typed it (without the admin mark), and whom to answer");
        w.Pump();
        c.Check(S, !w.Host.To(boss).Any(x => x.Text.StartsWith("The Dereth", StringComparison.Ordinal)), "nothing shown before the hub answers");
        release.SetResult("The Dereth Network verified this world.");
        until = DateTime.UtcNow.AddSeconds(5);
        while (!w.Host.To(boss).Any(x => x.Text == "The Dereth Network verified this world.") && DateTime.UtcNow < until)
        {
            Thread.Sleep(5);
            w.Pump();
        }
        c.Check(S, w.Host.To(boss).Any(x => x.Text == "The Dereth Network verified this world."), "the answer is shown on a later pump");
        w.World.Verifier = _ => Task.FromResult("The Dereth Network verified this world, again.");
        c.Check(S, w.World.Verify(null, "the console", [token]) == "Asking the Dereth Network's hub to verify this world...", "the next may go");
        until = DateTime.UtcNow.AddSeconds(5);
        while (!w.Logs.Any(l => l.Contains("verified this world, again", StringComparison.Ordinal)) && DateTime.UtcNow < until)
        {
            Thread.Sleep(5);
            w.Pump();
        }
        c.Check(S, w.Logs.Any(l => l.Contains("verified this world, again", StringComparison.Ordinal)), "the console's answer goes to the log");
    }

    private static void OverHttp(Checks c)
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        string? head = null, body = null;
        var hub = Task.Run(() =>
        {
            for (var i = 0; i < 2; i++)
            {
                using var client = listener.AcceptTcpClient();
                var stream = client.GetStream();
                var text = new StringBuilder();
                var buf = new byte[1];
                while (!text.ToString().EndsWith("\r\n\r\n", StringComparison.Ordinal) && stream.Read(buf, 0, 1) == 1)
                    text.Append((char)buf[0]);
                var h = text.ToString();
                var length = int.Parse(h.Split("\r\n").First(l => l.StartsWith("Content-Length:", StringComparison.OrdinalIgnoreCase))[15..].Trim());
                var content = new byte[length];
                var read = 0;
                while (read < length)
                    read += stream.Read(content, read, length - read);
                head = h;
                body = Encoding.UTF8.GetString(content);
                var good = h.Contains("Authorization: Bearer drth_" + new string('k', 43), StringComparison.Ordinal);
                var answer = good ? "{\"verified\":true,\"federation\":\"on\",\"approval\":\"approved\"}"
                                  : "{\"error\":{\"code\":\"UNAUTHORIZED\",\"message\":\"A valid key for this server is required\"}}";
                var status = good ? "200 OK" : "401 Unauthorized";
                stream.Write(Encoding.ASCII.GetBytes($"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {Encoding.UTF8.GetByteCount(answer)}\r\nConnection: close\r\n\r\n{answer}"));
            }
        });
        using var http = new HttpClient(new SocketsHttpHandler { AllowAutoRedirect = false }) { Timeout = TimeSpan.FromSeconds(10) };
        var route = HubVerify.RouteFor(new Uri($"ws://127.0.0.1:{port}/v1/servers/alpha/link"));
        var request = new VerifyRequest("dnv_" + new string('Q', 32), "Boss", null);
        var line = HubVerify.SendAsync(http, route, "drth_" + new string('k', 43), request, CancellationToken.None).GetAwaiter().GetResult();
        c.Check(S, head?.StartsWith("POST /v1/servers/alpha/verify HTTP/1.1", StringComparison.Ordinal) == true, "a POST to the world's verification route");
        var sent = body is null ? null : JsonNode.Parse(body);
        c.Check(S, sent?["token"]?.GetValue<string>() == request.Token && sent?["by"]?.GetValue<string>() == "Boss", "the token and who typed it, as JSON");
        c.Check(S, line.StartsWith("The Dereth Network verified this world. It is on the network now", StringComparison.Ordinal), "the hub's answer as a line");
        var wrong = HubVerify.SendAsync(http, route, "drth_" + new string('x', 43), request, CancellationToken.None).GetAwaiter().GetResult();
        c.Check(S, wrong.Contains("does not know this world's key", StringComparison.Ordinal), "a key the hub refuses");
        hub.Wait(TimeSpan.FromSeconds(5));
        listener.Stop();
        var gone = HubVerify.SendAsync(http, route, "drth_" + new string('k', 43), request, CancellationToken.None).GetAwaiter().GetResult();
        c.Check(S, gone.StartsWith("The Dereth Network's hub could not be reached", StringComparison.Ordinal), "a hub that is not there");
    }
}
