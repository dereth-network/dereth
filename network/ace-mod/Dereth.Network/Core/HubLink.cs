// The link to the hub: one WebSocket, opened by this world, on a task of its own.
//
// It connects to `<hub>/v1/servers/<slug>/link` with the world's key as a bearer token, carries
// text frames both ways, and reconnects with growing waits (1, 2, 5, 10, then every 30 seconds; a
// minute after the hub refuses the key or the world, or anything in front of the hub refuses the
// connection with a 401 or 403; the hub marks its own refusals, so the log says which it was). It
// knows nothing of the protocol beyond the frame: the world half decides what to send and reads
// what arrives, on the world thread, through <see cref="TryTake"/> and <see cref="Send"/>. A frame handed over for a connection that has since
// gone is dropped rather than sent first on the next one, where only `hello` may come first. A hub
// that sends faster than the world takes its frames, so that <see cref="HubLink.MaxWaitingFrames"/>
// wait, loses the link, as one that falls silent does.
//
// Nothing here logs the key or a frame's contents.

using System.Collections.Concurrent;
using System.Net.WebSockets;
using System.Text;
using System.Threading.Channels;

namespace Dereth.Network;

public enum LogLevel
{
    Debug,
    Info,
    Warn,
    Error,
}

public delegate void Logger(LogLevel level, string message);

public enum LinkEventKind
{
    /// <summary>A connection is up; frames for it carry its epoch.</summary>
    Up,
    /// <summary>A text frame from the hub.</summary>
    Frame,
    /// <summary>The connection is gone.</summary>
    Down,
    /// <summary>
    /// The connection was refused before the upgrade with a 401 or 403 (its <c>Status</c>). <c>Text</c> is the hub's
    /// mark (<see cref="HubRefusal.Key"/>, <see cref="HubRefusal.Federation"/>), or empty when the refusal came from something
    /// in front of the hub.
    /// </summary>
    Refused,
}

public readonly record struct LinkEvent(LinkEventKind Kind, long Epoch, string Text, int Status);

/// <summary>What the world half needs of a link.</summary>
public interface ILink
{
    bool TryTake(out LinkEvent e);

    /// <summary>Queues a frame for the connection `epoch`; dropped if that connection has gone.</summary>
    void Send(long epoch, string frame);
}

/// <summary>
/// Who refused a link before its upgrade. The hub marks each of its own refusals with <see cref="Header"/>; a 401 or 403
/// without it came from something in front of the hub (a reverse proxy, or Cloudflare challenging what it takes for a
/// bot), never from the hub.
/// </summary>
public static class HubRefusal
{
    public const string Header = "X-Dereth-Hub-Refusal";

    /// <summary>The mark of a refusal of the world's key (401).</summary>
    public const string Key = "key";

    /// <summary>The mark of a refusal of a world whose federation flag is not on (403).</summary>
    public const string Federation = "federation";

    /// <summary>The most of a header's value kept from a refusal, so a long one never fills the log.</summary>
    private const int MaxValue = 64;

    /// <summary>The hub's mark on a refusal, from its headers (<paramref name="header"/> gives one by name); null when it has none.</summary>
    public static string? Mark(Func<string, string?> header)
    {
        var mark = Kept(header(Header));
        return mark.Length > 0 ? mark : null;
    }

    /// <summary>What the log says of a refusal of <paramref name="status"/>, without what the link does next.</summary>
    public static string Describe(int status, Func<string, string?> header) => Mark(header) switch
    {
        Key => $"the hub refused this world's key ({status})",
        Federation => $"the hub refused this world ({status}: its federation flag is not on)",
        { } other => $"the hub refused this world ({status}: {other})",
        null => $"something in front of the hub refused the connection (HTTP {status}{Named(header)}); "
                + "check the proxy or Cloudflare (Bot Fight Mode challenges game servers)",
    };

    /// <summary>What answered in the hub's place, as it named itself: its Server and CF-Ray headers.</summary>
    private static string Named(Func<string, string?> header)
    {
        var named = new StringBuilder();
        var server = Kept(header("Server"));
        if (server.Length > 0)
            named.Append(", server: ").Append(server);
        var ray = Kept(header("CF-Ray"));
        if (ray.Length > 0)
            named.Append(", cf-ray: ").Append(ray);
        return named.ToString();
    }

    /// <summary>A header's value as the log keeps it: printable ASCII only, at most <see cref="MaxValue"/> characters.</summary>
    private static string Kept(string? v)
    {
        if (v is null)
            return "";
        var kept = new StringBuilder();
        foreach (var ch in v.Trim())
        {
            if (ch is >= ' ' and <= '~')
                kept.Append(ch);
            if (kept.Length == MaxValue)
                break;
        }
        return kept.ToString();
    }

    /// <summary>A lookup of a response's headers by name, ignoring case; the first value of each.</summary>
    public static Func<string, string?> Lookup(IReadOnlyDictionary<string, IEnumerable<string>>? headers) => name =>
    {
        if (headers is null)
            return null;
        foreach (var (k, values) in headers)
        {
            if (string.Equals(k, name, StringComparison.OrdinalIgnoreCase))
                return values.FirstOrDefault();
        }
        return null;
    };
}

public sealed class LinkTiming
{
    /// <summary>The waits between reconnection attempts; the last repeats.</summary>
    public TimeSpan[] Backoff { get; init; } =
    [
        TimeSpan.FromSeconds(1), TimeSpan.FromSeconds(2), TimeSpan.FromSeconds(5), TimeSpan.FromSeconds(10),
        TimeSpan.FromSeconds(30),
    ];

    /// <summary>The wait after the hub refuses the key or the world.</summary>
    public TimeSpan RefusedWait { get; init; } = TimeSpan.FromSeconds(60);

    public TimeSpan ConnectTimeout { get; init; } = TimeSpan.FromSeconds(10);

    /// <summary>How often this side pings, and how long it waits for the answer before it gives the link up.</summary>
    public TimeSpan KeepAlive { get; init; } = TimeSpan.FromSeconds(20);

    public TimeSpan KeepAliveTimeout { get; init; } = TimeSpan.FromSeconds(50);
}

public sealed class HubLink : ILink, IDisposable
{
    /// <summary>
    /// The most frames from the hub that may wait for the world. A hub sending faster than the world
    /// takes them loses the link (it is made again, as after any lost link), so it never holds this
    /// world's memory.
    /// </summary>
    public const int MaxWaitingFrames = 20_000;

    private static long nextEpoch;

    /// <summary>How many frames wait in the events queue.</summary>
    private int waiting;

    private readonly Uri uri;
    private readonly string key;
    private readonly Logger log;
    private readonly LinkTiming timing;
    private readonly ConcurrentQueue<LinkEvent> events = new();
    private readonly Channel<(long Epoch, string Frame)> outbound =
        Channel.CreateUnbounded<(long, string)>(new UnboundedChannelOptions { SingleReader = true });
    private CancellationTokenSource? cts;
    private Task? loop;
    private long epoch;
    private volatile bool stopping;

    public HubLink(Uri link, string key, Logger log, LinkTiming? timing = null)
    {
        uri = link;
        this.key = key;
        this.log = log;
        this.timing = timing ?? new LinkTiming();
    }

    /// <summary>The current connection's epoch, or 0 while there is none.</summary>
    public long Epoch => Volatile.Read(ref epoch);

    public void Start()
    {
        if (loop is not null)
            return;
        cts = new CancellationTokenSource();
        var token = cts.Token;
        loop = Task.Run(() => Run(token));
    }

    /// <summary>
    /// Stops the link without waiting for it: on the link's own task the socket is closed with a
    /// close frame, or dropped if the hub has not answered within a few seconds.
    /// </summary>
    public void Stop()
    {
        stopping = true;
        var c = cts;
        var l = loop;
        cts = null;
        loop = null;
        outbound.Writer.TryComplete();
        if (c is null)
            return;
        c.CancelAfter(TimeSpan.FromSeconds(3));
        _ = (l ?? Task.CompletedTask).ContinueWith(_ => c.Dispose(), TaskScheduler.Default);
    }

    /// <summary>For tests: waits for the stopped link's task to end.</summary>
    public static bool WaitStopped(Task? t, TimeSpan limit) => t is null || t.Wait(limit);

    public Task? Running => loop;

    public void Dispose() => Stop();

    public bool TryTake(out LinkEvent e)
    {
        if (!events.TryDequeue(out e))
            return false;
        if (e.Kind == LinkEventKind.Frame)
            Interlocked.Decrement(ref waiting);
        return true;
    }

    public void Send(long epoch, string frame) => outbound.Writer.TryWrite((epoch, frame));

    private string Describe(Exception e)
    {
        var inner = e;
        while (inner.InnerException is not null)
            inner = inner.InnerException;
        var why = inner.GetType().Name + ": " + inner.Message;
        return key.Length > 0 ? why.Replace(key, "(key)", StringComparison.Ordinal) : why;
    }

    private async Task Run(CancellationToken ct)
    {
        var attempt = 0;
        while (!stopping && !ct.IsCancellationRequested)
        {
            using var ws = new ClientWebSocket();
            ws.Options.SetRequestHeader("Authorization", "Bearer " + key);
            ws.Options.CollectHttpResponseDetails = true;
            ws.Options.KeepAliveInterval = timing.KeepAlive;
            ws.Options.KeepAliveTimeout = timing.KeepAliveTimeout;
            try
            {
                using var connect = CancellationTokenSource.CreateLinkedTokenSource(ct);
                connect.CancelAfter(timing.ConnectTimeout);
                await ws.ConnectAsync(uri, connect.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (ct.IsCancellationRequested)
            {
                return;
            }
            catch (Exception e)
            {
                if (stopping)
                    return;
                var code = (int)ws.HttpStatusCode;
                if (code is 401 or 403)
                {
                    var header = HubRefusal.Lookup(ws.HttpResponseHeaders);
                    events.Enqueue(new LinkEvent(LinkEventKind.Refused, 0, HubRefusal.Mark(header) ?? "", code));
                    log(LogLevel.Error, HubRefusal.Describe(code, header) + "; trying again in a minute");
                    await Pause(timing.RefusedWait, ct).ConfigureAwait(false);
                    continue;
                }
                var wait = timing.Backoff[Math.Min(attempt, timing.Backoff.Length - 1)];
                attempt++;
                log(LogLevel.Warn, code != 0
                    ? $"the hub answered {code}; trying again in {wait.TotalSeconds:0.#} s"
                    : $"cannot reach the hub ({Describe(e)}); trying again in {wait.TotalSeconds:0.#} s");
                await Pause(wait, ct).ConfigureAwait(false);
                continue;
            }
            attempt = 0;
            var mine = Interlocked.Increment(ref nextEpoch);
            Volatile.Write(ref epoch, mine);
            events.Enqueue(new LinkEvent(LinkEventKind.Up, mine, "", 0));
            log(LogLevel.Info, "connected to the hub");
            var why = await Carry(ws, mine, ct).ConfigureAwait(false);
            Volatile.Write(ref epoch, 0);
            events.Enqueue(new LinkEvent(LinkEventKind.Down, mine, why, 0));
            log(LogLevel.Info, "the link to the hub is down (" + why + ")");
            if (ws.State is WebSocketState.Open or WebSocketState.CloseReceived)
            {
                try
                {
                    using var close = new CancellationTokenSource(TimeSpan.FromSeconds(2));
                    await ws.CloseOutputAsync(WebSocketCloseStatus.NormalClosure, "", close.Token).ConfigureAwait(false);
                }
                catch (Exception)
                {
                    // The socket is going either way.
                }
            }
            if (!stopping && !ct.IsCancellationRequested)
                await Pause(timing.Backoff[0], ct).ConfigureAwait(false);
        }
    }

    private static async Task Pause(TimeSpan d, CancellationToken ct)
    {
        try
        {
            await Task.Delay(d, ct).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
        }
    }

    private async Task<string> Carry(ClientWebSocket ws, long mine, CancellationToken ct)
    {
        using var both = CancellationTokenSource.CreateLinkedTokenSource(ct);
        var receive = Receive(ws, mine, both.Token);
        var send = SendFrames(ws, mine, both.Token);
        var first = await Task.WhenAny(receive, send).ConfigureAwait(false);
        if (first == send && stopping && ws.State == WebSocketState.Open)
        {
            // Stopping: say so to the hub, and give it a moment to answer the close.
            try
            {
                using var close = new CancellationTokenSource(TimeSpan.FromSeconds(2));
                await ws.CloseOutputAsync(WebSocketCloseStatus.NormalClosure, "stopping", close.Token).ConfigureAwait(false);
                await Task.WhenAny(receive, Task.Delay(TimeSpan.FromSeconds(2), CancellationToken.None)).ConfigureAwait(false);
            }
            catch (Exception)
            {
                // The socket is going either way.
            }
        }
        both.Cancel();
        string why;
        try
        {
            why = await first.ConfigureAwait(false);
        }
        catch (Exception e)
        {
            why = stopping || ct.IsCancellationRequested ? "stopping" : Describe(e);
        }
        try
        {
            await Task.WhenAll(receive, send).ConfigureAwait(false);
        }
        catch (Exception)
        {
            // The other half ends because the first did.
        }
        return why;
    }

    private async Task<string> Receive(ClientWebSocket ws, long mine, CancellationToken ct)
    {
        var buffer = new byte[Wire.MaxMessageBytes + 1];
        var used = 0;
        while (true)
        {
            if (used >= buffer.Length)
                return "a frame longer than the limit";
            var r = await ws.ReceiveAsync(new ArraySegment<byte>(buffer, used, buffer.Length - used), ct).ConfigureAwait(false);
            if (r.MessageType == WebSocketMessageType.Close)
                return "closed by the hub";
            if (r.MessageType == WebSocketMessageType.Binary)
                return "a binary frame";
            used += r.Count;
            if (used > Wire.MaxMessageBytes)
                return "a frame longer than the limit";
            if (!r.EndOfMessage)
                continue;
            if (Volatile.Read(ref waiting) >= MaxWaitingFrames)
                return "the hub sends faster than this world takes its frames";
            Interlocked.Increment(ref waiting);
            events.Enqueue(new LinkEvent(LinkEventKind.Frame, mine, Encoding.UTF8.GetString(buffer, 0, used), 0));
            used = 0;
        }
    }

    private async Task<string> SendFrames(ClientWebSocket ws, long mine, CancellationToken ct)
    {
        var reader = outbound.Reader;
        while (await reader.WaitToReadAsync(ct).ConfigureAwait(false))
        {
            while (reader.TryRead(out var item))
            {
                if (item.Epoch != mine)
                    continue;
                var bytes = Encoding.UTF8.GetBytes(item.Frame);
                if (bytes.Length > Wire.MaxMessageBytes)
                {
                    log(LogLevel.Warn, "dropped a frame longer than the limit");
                    continue;
                }
                await ws.SendAsync(bytes, WebSocketMessageType.Text, true, ct).ConfigureAwait(false);
            }
        }
        return "stopping";
    }
}
