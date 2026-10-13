// The mod's harness. It compiles only the mod's ACE-free sources and checks them four ways:
//
//   vectors   the conformance vectors (addresses, text rules, link messages, the directory's, the
//             reach messages, the say messages)
//   units     stand-in ids, rate buckets, settings, the store, hub addresses, the game's string
//             layout, Windows' own best fits, the world half on a link in memory (the directory's
//             listings, the hub's reach questions and its says among it), and the link client
//             against a socket that floods it
//   hub       the link client and the world half against a real hub over loopback, the directory's
//             listings through it, `/dn verify` end to end: the hub's challenge heard on a game
//             port and answered, a world's own room mirrored into Discord end
//             to end: its lines said through the webhook its operator gave, a tell to a
//             character away carried into its person's direct messages, the reply back, and a
//             line its person says in a mirror from Discord, said by the world as the character
//   targets   every ACE method the mod patches, resolved in an ACE.Server.dll on disk
//
// Usage: Dereth.Network.Harness [--vectors <dir>] [--hub <hub executable>] [--ace <ACE server folder>]
//                                [--mod <the built Dereth.Network.dll>] [--skip <section>]...
//   --vectors defaults to the conformance vectors found from the harness's sources or above the
//   harness; --hub and --ace may also come from DERETH_NETWORK_HUB and DERETH_ACE_DIR; --mod
//   defaults to the mod built beside the harness. A section whose input is missing is reported as
//   not run and fails the run, unless it is skipped by name (vectors, hub, targets, hooks).

using Dereth.Network.Harness;

var options = Options.Parse(args);
var checks = new Checks();
try
{
    VectorChecks.Run(checks, options.Vectors);
    UnitChecks.Run(checks);
    if (options.Hub is not null)
    {
        await HubChecks.Run(checks, options.Hub);
        await VerifyHubChecks.Run(checks, options.Hub);
        await DirectoryHubChecks.Run(checks, options.Hub);
        await DiscordMirrorHubChecks.Run(checks, options.Hub);
        await ReachHubChecks.Run(checks, options.Hub);
        await SayHubChecks.Run(checks, options.Hub);
    }
    else
        checks.NotRun("hub", "no --hub (or DERETH_NETWORK_HUB) given");
    if (options.Ace is not null)
        TargetChecks.Run(checks, options.Ace, options.Mod);
    else
        checks.NotRun("targets", "no --ace (or DERETH_ACE_DIR) given");
    checks.Skipped.UnionWith(options.Skip);
}
catch (Exception e)
{
    checks.Fail("harness", "an unexpected exception: " + e);
}
return checks.Report();

internal sealed class Options
{
    public string? Vectors { get; private set; }
    public string? Hub { get; private set; }
    public string? Ace { get; private set; }
    public string? Mod { get; private set; }
    public List<string> Skip { get; } = [];

    public static Options Parse(string[] args)
    {
        var o = new Options
        {
            Hub = Environment.GetEnvironmentVariable("DERETH_NETWORK_HUB"),
            Ace = Environment.GetEnvironmentVariable("DERETH_ACE_DIR"),
        };
        for (var i = 0; i + 1 < args.Length; i += 2)
        {
            switch (args[i])
            {
                case "--vectors": o.Vectors = args[i + 1]; break;
                case "--hub": o.Hub = args[i + 1]; break;
                case "--ace": o.Ace = args[i + 1]; break;
                case "--mod": o.Mod = args[i + 1]; break;
                case "--skip": o.Skip.Add(args[i + 1]); break;
                default: throw new ArgumentException("unknown option " + args[i]);
            }
        }
        o.Vectors ??= FindVectors();
        return o;
    }

    private static string? FindVectors()
    {
        // From the sources the harness was built from (whichever tree holds them), then from where
        // it was built to.
        var starts = new[] { SourceDir(), AppContext.BaseDirectory };
        foreach (var start in starts)
        {
            var dir = start is null ? null : new DirectoryInfo(start);
            while (dir is not null)
            {
                foreach (var candidate in new[]
                         {
                             Path.Combine(dir.FullName, "conformance", "vectors"),
                             Path.Combine(dir.FullName, "network", "conformance", "vectors"),
                         })
                    if (File.Exists(Path.Combine(candidate, "addresses.json")))
                        return candidate;
                dir = dir.Parent;
            }
        }
        return null;
    }

    private static string? SourceDir([System.Runtime.CompilerServices.CallerFilePath] string path = "") =>
        string.IsNullOrEmpty(path) ? null : Path.GetDirectoryName(path);
}

internal sealed class Checks
{
    private readonly SortedDictionary<string, (int Passed, int Failed)> sections = new(StringComparer.Ordinal);
    private readonly List<string> failures = [];
    private readonly List<string> notes = [];
    private readonly List<(string Section, string Line)> notRun = [];

    /// <summary>Sections the caller chose not to run: not a failure.</summary>
    public HashSet<string> Skipped { get; } = new(StringComparer.Ordinal);

    public void Check(string section, bool ok, string what)
    {
        sections.TryGetValue(section, out var c);
        sections[section] = ok ? (c.Passed + 1, c.Failed) : (c.Passed, c.Failed + 1);
        if (!ok)
            failures.Add($"[{section}] {what}");
    }

    public void Pass(string section, string what) => Check(section, true, what);

    public void Fail(string section, string what) => Check(section, false, what);

    public void Note(string section, string what) => notes.Add($"[{section}] {what}");

    public void NotRun(string section, string why) => notRun.Add((section, $"[{section}] not run: {why}"));

    public int Report()
    {
        Console.WriteLine();
        foreach (var (s, (p, f)) in sections)
            Console.WriteLine($"{s,-26} {p,4} passed {f,4} failed");
        foreach (var (_, line) in notRun)
            Console.WriteLine(line);
        foreach (var n in notes)
            Console.WriteLine("note " + n);
        foreach (var f in failures)
            Console.WriteLine("FAILED " + f);
        var total = sections.Values.Sum(v => v.Failed);
        // A section left out without being skipped by name leaves the run incomplete.
        var missing = notRun.Where(n => !Skipped.Contains(n.Section.Split('/')[0])).Select(n => n.Section).ToList();
        if (total > 0)
            Console.WriteLine($"{total} check(s) failed");
        else if (missing.Count > 0)
            Console.WriteLine($"incomplete: {string.Join(", ", missing)} not run (pass --skip <section> to leave one out)");
        else
            Console.WriteLine("all checks passed");
        return total == 0 && missing.Count == 0 ? 0 : 1;
    }
}
