// Rate limits: a token bucket on a caller's clock, and the hub's limits as `welcome` gives them.
//
// Nothing here reads a clock. Each call passes the time, in seconds on any monotonic scale the
// caller keeps.

namespace Dereth.Network;

/// <summary>A rate: a burst and a sustained rate.</summary>
public readonly record struct Rate(double Burst, double PerSec);

/// <summary>A token bucket: up to `burst` messages at once, refilled at `per_sec` a second.</summary>
public sealed class Bucket
{
    public double Burst { get; }
    public double PerSec { get; }
    private double tokens;
    private double at;

    public Bucket(Rate rate, double now)
    {
        Burst = rate.Burst;
        PerSec = rate.PerSec;
        tokens = rate.Burst;
        at = now;
    }

    /// <summary>Takes one token at `now`; false (and nothing taken) when the bucket is empty.</summary>
    public bool Take(double now)
    {
        Refill(now);
        if (tokens >= 1.0)
        {
            tokens -= 1.0;
            return true;
        }
        return false;
    }

    private void Refill(double now)
    {
        if (now > at)
        {
            tokens = Math.Min(tokens + (now - at) * PerSec, Burst);
            at = now;
        }
    }

    /// <summary>Whether a token is there to take at `now` (nothing is taken).</summary>
    public bool Has(double now)
    {
        var t = tokens;
        if (now > at)
            t = Math.Min(t + (now - at) * PerSec, Burst);
        return t >= 1.0;
    }

    /// <summary>Whether the bucket has refilled completely by `now`.</summary>
    public bool FullAt(double now)
    {
        var t = tokens;
        if (now > at)
            t = Math.Min(t + (now - at) * PerSec, Burst);
        return t >= Burst;
    }
}

/// <summary>
/// The hub's limits; <see cref="Default"/> are the values the specification gives. A limit a hub
/// leaves out of `welcome` takes its default (<see cref="DefaultRoomLinesPerWorld"/>,
/// <see cref="DefaultTellsPerRecipient"/>).
/// </summary>
public sealed record Limits(
    Rate TellsPerCharacter,
    Rate RoomLinesPerCharacter,
    Rate MessagesPerWorld,
    Rate RoomLinesPerWorld,
    Rate TellsPerRecipient,
    uint PairBurst,
    double PairWindowSecs,
    double PairBlockSecs,
    double HoldSecs,
    double ResultTimeoutSecs)
{
    public static readonly Rate DefaultRoomLinesPerWorld = new(10.0, 1.0);
    public static readonly Rate DefaultTellsPerRecipient = new(20.0, 1.0);

    /// <summary>
    /// Watches one world sends any one other world, all its characters together: a quarter of
    /// <see cref="MessagesPerWorld"/>, since each is owed an answer that counts against the target's
    /// own pace. Not a field of its own: it follows `messages_per_world`.
    /// </summary>
    public Rate WatchesPerWorldPair => new(Math.Max(1.0, MessagesPerWorld.Burst / 4.0), MessagesPerWorld.PerSec / 4.0);

    public static readonly Limits Default = new(
        new Rate(10.0, 0.5),
        new Rate(3.0, 0.2),
        new Rate(200.0, 50.0),
        DefaultRoomLinesPerWorld,
        DefaultTellsPerRecipient,
        20,
        60.0,
        300.0,
        60.0,
        // Longer than the hub's hold and the target's answer window together (60 + 30), so a tell
        // the hub may still deliver is never given up on first.
        100.0);
}
