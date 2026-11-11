// The link client against a stand-in for the hub's link route on 127.0.0.1 that answers the upgrade 403: with the
// hub's mark, the refusal is the hub's; without it, something in front of the hub refused the connection, and the log
// says so rather than blame the world's federation flag.

using System.Net;
using System.Net.Sockets;
using System.Text;

namespace Dereth.Network.Harness;

internal static class RefusalChecks
{
    private const string S = "units/link";

    public static void Run(Checks c)
    {
        var (front, frontLines) = Refused("Server: cloudflare\r\nCF-RAY: 8f00aa-AMS\r\n");
        c.Check(S, front is { Kind: LinkEventKind.Refused, Status: 403, Text: "" }
                   && frontLines.Contains("something in front of the hub refused the connection (HTTP 403, server: cloudflare, "
                                          + "cf-ray: 8f00aa-AMS); check the proxy or Cloudflare (Bot Fight Mode challenges game "
                                          + "servers); trying again in a minute")
                   && !frontLines.Any(l => l.Contains("federation", StringComparison.Ordinal)),
            $"a 403 without the hub's mark is logged as a refusal in front of the hub ({string.Join(" | ", frontLines)})");
        var (hub, hubLines) = Refused("X-Dereth-Hub-Refusal: federation\r\n");
        c.Check(S, hub is { Kind: LinkEventKind.Refused, Status: 403, Text: HubRefusal.Federation }
                   && hubLines.Contains("the hub refused this world (403: its federation flag is not on); trying again in a minute"),
            $"a 403 with the hub's mark is logged as the hub's refusal of the world ({string.Join(" | ", hubLines)})");
    }

    /// <summary>
    /// What a link does with a stand-in that answers its upgrade 403 with <paramref name="headers"/> (each line ending
    /// CRLF): its first event, and what it logged.
    /// </summary>
    private static (LinkEvent? First, List<string> Lines) Refused(string headers)
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        var hub = Task.Run(() =>
        {
            using var client = listener.AcceptTcpClient();
            var stream = client.GetStream();
            var head = new StringBuilder();
            var buf = new byte[1];
            while (!head.ToString().EndsWith("\r\n\r\n", StringComparison.Ordinal) && stream.Read(buf, 0, 1) == 1)
                head.Append((char)buf[0]);
            stream.Write(Encoding.ASCII.GetBytes(
                $"HTTP/1.1 403 Forbidden\r\n{headers}Content-Type: text/html\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
        });
        var lines = new List<string>();
        var link = new HubLink(new Uri($"ws://127.0.0.1:{port}/v1/servers/alpha/link"), "drth_" + new string('A', 43),
            (_, m) =>
            {
                lock (lines)
                    lines.Add(m);
            }, new LinkTiming { RefusedWait = TimeSpan.FromMinutes(10) });
        link.Start();
        // A guard against a link that never answers, not the time it is expected to take.
        var until = DateTime.UtcNow.AddSeconds(60);
        LinkEvent? first = null;
        while (first is null && DateTime.UtcNow < until)
        {
            if (link.TryTake(out var e))
                first = e;
            else
                Thread.Sleep(2);
        }
        // The line is logged once the event is queued: wait for it as well.
        while (DateTime.UtcNow < until)
        {
            lock (lines)
            {
                if (lines.Count > 0)
                    break;
            }
            Thread.Sleep(2);
        }
        var running = link.Running;
        link.Stop();
        HubLink.WaitStopped(running, TimeSpan.FromSeconds(5));
        listener.Stop();
        hub.Wait(TimeSpan.FromSeconds(5));
        lock (lines)
            return (first, [.. lines]);
    }
}
