// The Dereth Network for ACE: links this world to other worlds through a hub, so that its players
// can tell, befriend and talk with characters on other worlds with the unmodified retail client.
//
// ACE's mod loader creates `Dereth.Network.Mod` and calls Initialize once. Initialize never throws
// and never blocks: it reads Settings.json, starts the link on a task of its own, and patches ACE
// one feature at a time, each only if every method it needs is there (see PatchTargets). Without a
// Settings.json, or with one that cannot be served, nothing is patched and the world behaves as ACE
// always does.

using System.Diagnostics;
using System.Reflection;
using ACE.Server;
using ACE.Server.Managers;
using ACE.Server.Mods;
using HarmonyLib;

namespace Dereth.Network;

/// <summary>The running network: the world half, its link, its store and its host.</summary>
internal sealed class Runtime
{
    public required NetworkWorld World { get; init; }
    public required HubLink Link { get; init; }
    public required StoreWriter Writer { get; init; }
    public required AceHost Host { get; init; }

    private readonly Dictionary<string, long> faults = new();
    private int started;

    /// <summary>
    /// Starts the link on the world thread's first tick: ACE loads mods before its world exists, and
    /// the link has nothing to say until the world runs.
    /// </summary>
    public void StartLinkOnce()
    {
        if (Interlocked.Exchange(ref started, 1) == 0)
            Link.Start();
    }

    /// <summary>Logs a fault in one place at most once a minute.</summary>
    public void Fault(string where, Exception e)
    {
        var now = Stopwatch.GetTimestamp();
        lock (faults)
        {
            if (faults.TryGetValue(where, out var at) && Stopwatch.GetElapsedTime(at, now) < TimeSpan.FromMinutes(1))
                return;
            faults[where] = now;
        }
        Mod.Log(LogLevel.Error, $"{where} failed and was left to ACE: {e.GetType().Name}: {e.Message}");
    }
}

public sealed class Mod : IHarmonyMod
{
    public const string Version = "0.1.0";
    private const string HarmonyId = "network.dereth.ace-mod";

    private static Runtime? current;

    internal static Runtime? Current => Volatile.Read(ref current);

    private static string? offReason;

    /// <summary>The error that kept the network off, for `/dn admin status`; null when none was logged.</summary>
    internal static string? OffReason => Volatile.Read(ref offReason);

    private Harmony? harmony;

    public void Initialize()
    {
        try
        {
            Start();
        }
        catch (Exception e)
        {
            Log(LogLevel.Error, $"the network could not start and stays off: {e.GetType().Name}: {e.Message}");
            Stop();
        }
    }

    public void Dispose()
    {
        try
        {
            Stop();
        }
        catch (Exception e)
        {
            Log(LogLevel.Error, $"stopping: {e.GetType().Name}: {e.Message}");
        }
    }

    internal static void Log(LogLevel level, string message)
    {
        var line = "[Dereth.Network] " + message;
        if (level == LogLevel.Error && Current is null)
            Volatile.Write(ref offReason, message);
        switch (level)
        {
            case LogLevel.Debug:
                ModManager.Log(line, ModManager.LogLevel.Debug);
                break;
            case LogLevel.Warn:
                ModManager.Log(line, ModManager.LogLevel.Warn);
                break;
            case LogLevel.Error:
                ModManager.Log(line, ModManager.LogLevel.Error);
                break;
            default:
                ModManager.Log(line, ModManager.LogLevel.Info);
                break;
        }
    }

    private void Start()
    {
        var folder = this.GetFolder();
        if (string.IsNullOrEmpty(folder))
            folder = Path.GetDirectoryName(typeof(Mod).Assembly.Location) ?? ".";
        var settingsPath = Path.Combine(folder, "Settings.json");
        if (!File.Exists(settingsPath))
        {
            Log(LogLevel.Info, "no Settings.json beside the mod; the network is off (copy Settings.json.example to start)");
            return;
        }
        var settingsText = File.ReadAllText(settingsPath);
        if (ModSettings.Problem(settingsText) is { } problem)
        {
            Log(LogLevel.Error, problem + "; the network is off");
            return;
        }
        var settings = ModSettings.FromJson(settingsText);
        var slug = Text.AsciiLower(settings.Slug.Trim());
        if (!Address.IsNetworkSlug(slug))
        {
            Log(LogLevel.Error, $"Slug \"{slug}\" is not a network slug (2-12 lowercase letters, digits and inner hyphens, not reserved); the network is off");
            return;
        }
        if (!HubUrl.TryLink(settings.HubUrl, slug, settings.AllowLocalWs, out var link, out var why))
        {
            Log(LogLevel.Error, why + "; the network is off");
            return;
        }
        var key = Environment.GetEnvironmentVariable(settings.KeyEnvironmentVariable)?.Trim();
        if (!HubUrl.IsKey(key))
        {
            Log(LogLevel.Error, $"the environment variable {settings.KeyEnvironmentVariable} holds no registry key (drth_...); the network is off");
            return;
        }
        RoomSlot? room = null;
        if (!string.IsNullOrWhiteSpace(settings.GlobalRoom))
        {
            if (!RoomSlots.TryParse(settings.GlobalRoom, out var slot))
            {
                Log(LogLevel.Error, $"GlobalRoom \"{settings.GlobalRoom}\": General, Trade, LFG, Roleplay or empty; the network is off");
                return;
            }
            room = slot;
        }
        var storePath = Path.IsPathRooted(settings.StorePath) ? settings.StorePath : Path.Combine(folder, settings.StorePath);
        NetworkStore store;
        try
        {
            store = NetworkStore.Load(storePath);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or FormatException)
        {
            // Starting over would give a stand-in id twice; the network stays off until the file is fixed.
            Log(LogLevel.Error, $"the network store {storePath} cannot be read ({e.GetType().Name}); the network is off");
            return;
        }

        var ace = typeof(PlayerManager).Assembly;
        var resolved = new Dictionary<string, List<(PatchTarget Target, MethodBase Method)>>();
        foreach (var feature in PatchTargets.Features)
        {
            var list = new List<(PatchTarget, MethodBase)>();
            var missing = new List<string>();
            foreach (var t in PatchTargets.Of(feature))
            {
                var m = PatchTargets.Resolve(ace, t, out var reason);
                if (m is null)
                    missing.Add(reason);
                else
                    list.Add((t, m));
            }
            if (missing.Count > 0)
            {
                Log(LogLevel.Warn, $"feature {feature} is off: this server has {string.Join("; ", missing)}");
                continue;
            }
            resolved[feature] = list;
        }
        if (!resolved.ContainsKey(PatchTargets.Core))
        {
            Log(LogLevel.Error, "the world tick cannot be patched on this server; the network is off");
            return;
        }
        if (!resolved.ContainsKey(PatchTargets.Names))
        {
            // A local name holding `@` could pass for a character of another world.
            Log(LogLevel.Error, "names holding @ cannot be refused on this server; the network is off");
            return;
        }

        var writer = new StoreWriter(storePath, m => Log(LogLevel.Error, m));
        var host = new AceHost();
        var software = $"Dereth.Network {Version} (ACE {SafeAceVersion()})";
        var world = new NetworkWorld(
            new WorldConfig
            {
                Slug = slug,
                GlobalRoom = room,
                MinCharacterAgeSeconds = settings.MinCharacterAgeSeconds,
                Software = software,
            },
            store, host, (l, m) => Log(l, m), Monotonic, DateTimeOffset.UtcNow.ToUnixTimeSeconds(), writer.Save);
        var hubLink = new HubLink(link, key!, (l, m) => Log(l, m));
        // `/dn verify` goes to the hub's verification route, over HTTPS, with the world's key.
        var verifyRoute = HubVerify.RouteFor(link);
        world.Verifier = request => HubVerify.SendAsync(VerifyHttp, verifyRoute, key!, request, CancellationToken.None);
        Volatile.Write(ref current, new Runtime { World = world, Link = hubLink, Writer = writer, Host = host });

        harmony = new Harmony(HarmonyId);
        var on = new List<string>();
        foreach (var (feature, targets) in resolved)
        {
            if (feature == PatchTargets.GlobalRoom && room is null)
                continue;
            if (Apply(feature, targets))
                on.Add(feature);
            else if (feature is PatchTargets.Core or PatchTargets.Names)
            {
                Log(LogLevel.Error, $"feature {feature} could not be patched; the network is off");
                Stop();
                return;
            }
        }
        if (on.Contains(PatchTargets.Verify))
        {
            // The hub's challenge, heard on the game port, is answered over HTTPS with the same key.
            var answerRoute = HubVerify.AnswerRouteFor(verifyRoute);
            world.Answerer = nonce => HubVerify.AnswerAsync(VerifyHttp, answerRoute, key!, nonce, CancellationToken.None);
        }
        else
            Log(LogLevel.Warn, "the hub's challenge cannot be heard on this server's game port: /dn verify cannot verify this world in game");
        if (room is not null && !on.Contains(PatchTargets.GlobalRoom))
        {
            // A room this world could hear but not speak in is not carried at all.
            Log(LogLevel.Warn, "the Global room is not carried: its lines cannot be taken from this server's chat");
            world.Config.GlobalRoom = null;
            room = null;
        }
        Log(LogLevel.Info, $"on as {slug}, linking to {link.GetLeftPart(UriPartial.Authority)} once the world runs; features: {string.Join(", ", on)}; " +
            $"global room: {(room is RoomSlot s ? RoomSlots.Name(s) : "none")}");
    }

    /// <summary>
    /// The client `/dn verify` and the answer to the hub's challenge call the hub with: no redirects,
    /// and a bounded wait, longer than the hub holds a verification while it looks the world's host
    /// up and waits for the answer.
    /// </summary>
    private static readonly HttpClient VerifyHttp = new(new SocketsHttpHandler { AllowAutoRedirect = false })
    {
        Timeout = TimeSpan.FromSeconds(60),
    };

    private static double Monotonic() => Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;

    private static string SafeAceVersion()
    {
        try
        {
            return ServerBuildInfo.GetServerVersion().ToString();
        }
        catch (Exception)
        {
            return "unknown";
        }
    }

    private static HarmonyMethod? Hook(string? name) => name is null
        ? null
        : new HarmonyMethod(typeof(Patches).GetMethod(name, BindingFlags.Public | BindingFlags.Static)
            ?? throw new MissingMethodException(nameof(Patches), name));

    /// <summary>Patches every target of one feature, or none of them.</summary>
    private bool Apply(string feature, List<(PatchTarget Target, MethodBase Method)> targets)
    {
        var done = new List<MethodBase>();
        try
        {
            foreach (var (t, m) in targets)
            {
                harmony!.Patch(m, prefix: Hook(t.Prefix), postfix: Hook(t.Postfix));
                done.Add(m);
            }
            return true;
        }
        catch (Exception e)
        {
            Log(LogLevel.Warn, $"feature {feature} is off: patching failed ({e.GetType().Name}: {e.Message})");
            foreach (var m in done)
            {
                try
                {
                    harmony!.Unpatch(m, HarmonyPatchType.All, harmony.Id);
                }
                catch (Exception)
                {
                    // Left as it is; the hooks pass everything to ACE once the network stops.
                }
            }
            return false;
        }
    }

    /// <summary>Stops without waiting on the network: the link closes on its own task.</summary>
    private void Stop()
    {
        var r = Current;
        Volatile.Write(ref current, null);
        try
        {
            harmony?.UnpatchAll(HarmonyId);
        }
        catch (Exception e)
        {
            Log(LogLevel.Warn, $"unpatching: {e.GetType().Name}: {e.Message}");
        }
        harmony = null;
        if (r is null)
            return;
        r.Link.Stop();
        try
        {
            r.World.SaveNow();
            // The store is small; a short wait lets the last change reach the disk at shutdown.
            r.Writer.Flush(TimeSpan.FromSeconds(2));
        }
        catch (Exception e)
        {
            Log(LogLevel.Error, $"saving the network store at stop: {e.GetType().Name}: {e.Message}");
        }
    }
}
