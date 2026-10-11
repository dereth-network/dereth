// The link client against a hub that sends without pause while the world takes nothing: a socket on
// 127.0.0.1 that answers the upgrade itself and writes text frames, no hub process needed.

using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;

namespace Dereth.Network.Harness;

internal static class FloodChecks
{
    private const string S = "units/link";

    public static void Run(Checks c)
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        var sent = new ManualResetEventSlim();
        var hub = Task.Run(() =>
        {
            using var client = listener.AcceptTcpClient();
            var stream = client.GetStream();
            // The upgrade: read the request's head, answer 101 with the key's accept value.
            var head = new StringBuilder();
            var buf = new byte[1];
            while (!head.ToString().EndsWith("\r\n\r\n", StringComparison.Ordinal) && stream.Read(buf, 0, 1) == 1)
                head.Append((char)buf[0]);
            var key = head.ToString().Split("\r\n")
                .First(l => l.StartsWith("Sec-WebSocket-Key:", StringComparison.OrdinalIgnoreCase))[18..].Trim();
            // RFC 6455's fixed GUID, which the accept value is hashed with (written in upper case).
            var guid = "258eafa5-e914-47da-95ca-c5ab0dc85b11".ToUpperInvariant();
            var accept = Convert.ToBase64String(SHA1.HashData(Encoding.ASCII.GetBytes(key + guid)));
            var answer = Encoding.ASCII.GetBytes("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n" +
                                                 $"Sec-WebSocket-Accept: {accept}\r\n\r\n");
            stream.Write(answer);
            var payload = Encoding.UTF8.GetBytes("{\"type\":\"a.later.type\"}");
            var frame = new byte[2 + payload.Length];
            frame[0] = 0x81;
            frame[1] = (byte)payload.Length;
            payload.CopyTo(frame, 2);
            try
            {
                for (var i = 0; i < HubLink.MaxWaitingFrames + 5_000; i++)
                    stream.Write(frame);
            }
            catch (IOException)
            {
                // The world dropped the link.
            }
            sent.Set();
            Thread.Sleep(1_000);
        });
        var link = new HubLink(new Uri($"ws://127.0.0.1:{port}/v1/servers/alpha/link"), "drth_" + new string('A', 43),
            (_, _) => { }, new LinkTiming { Backoff = [TimeSpan.FromSeconds(30)] });
        link.Start();
        sent.Wait(TimeSpan.FromSeconds(30));
        Thread.Sleep(500);
        var (frames, down) = (0, (string?)null);
        while (link.TryTake(out var e))
        {
            if (e.Kind == LinkEventKind.Frame)
                frames++;
            else if (e.Kind == LinkEventKind.Down)
            {
                down = e.Text;
                break;
            }
        }
        var running = link.Running;
        link.Stop();
        HubLink.WaitStopped(running, TimeSpan.FromSeconds(5));
        listener.Stop();
        hub.Wait(TimeSpan.FromSeconds(5));
        c.Check(S, frames == HubLink.MaxWaitingFrames && down == "the hub sends faster than this world takes its frames",
            $"a hub that sends faster than the world takes its frames loses the link ({frames} frames waited; {down ?? "no drop"})");
    }
}
