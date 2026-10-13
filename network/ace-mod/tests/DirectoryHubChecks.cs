// The directory through a real hub, over loopback only: a character put on the network is published
// at once with its page, and taken out at once when it leaves; the hub operator's takedown from its
// command line, while the hub runs, holds the character out in game, is confirmed, and is told; and
// a listing during the takedown is refused with its line.

using System.Diagnostics;
using System.Net;

namespace Dereth.Network.Harness;

internal static class DirectoryHubChecks
{
    private const string S = "hub/directory";

    private static readonly LinkTiming Fast = new()
    {
        Backoff = [TimeSpan.FromMilliseconds(200), TimeSpan.FromMilliseconds(300), TimeSpan.FromMilliseconds(500)],
        RefusedWait = TimeSpan.FromSeconds(2),
        ConnectTimeout = TimeSpan.FromSeconds(5),
    };

    private static async Task<bool> Until(Func<bool> cond, double seconds, TestWorld w)
    {
        var until = DateTime.UtcNow.AddSeconds(seconds);
        while (DateTime.UtcNow < until)
        {
            w.Pump();
            if (cond())
                return true;
            await Task.Delay(10);
        }
        w.Pump();
        return cond();
    }

    public static async Task Run(Checks c, string exe)
    {
        if (!File.Exists(exe))
        {
            c.Fail(S, "no hub at " + exe);
            return;
        }
        using var hub = new HubProcess(exe);
        var key = hub.Register("alpha", "Alpha");
        hub.Federation("alpha", "on");
        hub.Serve();
        HubUrl.TryLink($"ws://127.0.0.1:{hub.Port}", "alpha", true, out var link, out _);
        var clock = Stopwatch.StartNew();
        var a = TestWorld.Make("alpha", new HubLink(link, key, (_, _) => { }, Fast), () => clock.Elapsed.TotalSeconds);
        ((HubLink)a.Link).Start();
        c.Check(S, await Until(() => a.World.Link == LinkState.Up, 10, a), "the world is welcomed");
        var ann = a.Host.Add(0x5000_0001, "Ann");
        ann.Level = 20;
        using var http = new HttpClient();
        async Task<(HttpStatusCode Status, string Body)> Page()
        {
            using var r = await http.GetAsync($"http://127.0.0.1:{hub.Port}/v1/directory/characters/Ann%40alpha");
            return (r.StatusCode, await r.Content.ReadAsStringAsync());
        }
        int Shown(string line) => a.Host.To(ann.Id).Count(x => x.Kind == "system" && x.Text == line);

        a.World.Dn(ann.Id, ["on"]);
        c.Check(S, await Until(() => a.Host.To(ann.Id).Any(x => x.Text.StartsWith("Your directory page: http", StringComparison.Ordinal)), 10, a),
            "/dn on is answered with the character's page");
        var page = await Page();
        c.Check(S, page.Status == HttpStatusCode.OK && page.Body.Contains("\"level\":20", StringComparison.Ordinal)
            && page.Body.Contains("\"title\":\"the Bloodletter\"", StringComparison.Ordinal),
            $"and the hub publishes the profile at once ({page.Status})");

        a.World.Dn(ann.Id, ["off"]);
        c.Check(S, await Until(() => a.World.RemovalsOwed == 0, 10, a) && (await Page()).Status == HttpStatusCode.NotFound,
            "/dn off takes it out at once, and the hub's answer clears what was owed");

        a.World.Dn(ann.Id, ["on"]);
        c.Check(S, await Until(() => a.World.ListingOf(ann.Id) is { Owed: false }, 10, a)
            && (await Page()).Status == HttpStatusCode.OK, "listed again");
        hub.DirectoryRemove("Ann@alpha");
        c.Check(S, await Until(() => Shown(NetworkWorld.TakenDownLine) == 1 && a.World.RemovalsOwed == 0
                && a.World.SettingsOf(ann.Id) == CharacterSettings.OnNetwork, 20, a),
            "the operator's takedown holds it out in game, is confirmed, and is told at once");
        c.Check(S, (await Page()).Status == HttpStatusCode.NotFound, "and the page is gone");

        a.World.Dn(ann.Id, ["off"]);
        a.World.Dn(ann.Id, ["on"]);
        c.Check(S, await Until(() => Shown(NetworkWorld.TakenDownLine) == 2, 10, a)
            && (await Page()).Status == HttpStatusCode.NotFound, "listing it again while taken down is refused, with its line");

        ((HubLink)a.Link).Stop();
        await Task.Delay(300);
        hub.Kill();
    }
}
