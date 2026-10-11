// Applies every hook of the built mod to the ACE method it patches, with the server's own Harmony,
// in a process that loads ACE's assemblies and runs none of ACE: it shows each patch can be made on
// this runtime, then takes them all off again. It does not run a patched method.
//
// Usage: Dereth.Network.PatchApply --ace <ACE server folder> --mod <the built Dereth.Network.dll>

using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Loader;

string? ace = null, mod = null;
for (var i = 0; i + 1 < args.Length; i += 2)
{
    if (args[i] == "--ace")
        ace = Path.GetFullPath(args[i + 1]);
    else if (args[i] == "--mod")
        mod = Path.GetFullPath(args[i + 1]);
}
if (ace is null || mod is null)
{
    Console.WriteLine("usage: Dereth.Network.PatchApply --ace <ACE server folder> --mod <Dereth.Network.dll>");
    return 2;
}
AssemblyLoadContext.Default.Resolving += (ctx, name) =>
{
    var f = Path.Combine(ace, name.Name + ".dll");
    return File.Exists(f) ? ctx.LoadFromAssemblyPath(f) : null;
};
return Dereth.Network.PatchApply.Apply.Run(ace, mod);

namespace Dereth.Network.PatchApply
{
    internal static class Apply
    {
        [MethodImpl(MethodImplOptions.NoInlining)]
        public static int Run(string ace, string mod)
        {
            var server = AssemblyLoadContext.Default.LoadFromAssemblyPath(Path.Combine(ace, "ACE.Server.dll"));
            var hooks = AssemblyLoadContext.Default.LoadFromAssemblyPath(mod).GetType("Dereth.Network.Patches")
                ?? throw new InvalidOperationException("no Dereth.Network.Patches in " + mod);
            var harmony = new HarmonyLib.Harmony("network.dereth.ace-mod.patch-check");
            int applied = 0, failed = 0;
            foreach (var t in PatchTargets.All)
            {
                var original = PatchTargets.Resolve(server, t, out var why);
                if (original is null)
                {
                    Console.WriteLine($"FAILED {t.Feature}: {t}: {why}");
                    failed++;
                    continue;
                }
                HarmonyLib.HarmonyMethod? Hook(string? name) =>
                    name is null ? null : new HarmonyLib.HarmonyMethod(hooks.GetMethod(name, BindingFlags.Public | BindingFlags.Static)
                        ?? throw new MissingMethodException("Dereth.Network.Patches", name));
                try
                {
                    harmony.Patch(original, prefix: Hook(t.Prefix), postfix: Hook(t.Postfix));
                    var info = HarmonyLib.Harmony.GetPatchInfo(original);
                    var ok = (t.Prefix is null || info?.Prefixes.Any(p => p.PatchMethod.Name == t.Prefix) == true)
                        && (t.Postfix is null || info?.Postfixes.Any(p => p.PatchMethod.Name == t.Postfix) == true);
                    Console.WriteLine($"{(ok ? "patched" : "FAILED")} {t.Feature}: {t}");
                    if (ok)
                        applied++;
                    else
                        failed++;
                }
                catch (Exception e)
                {
                    Console.WriteLine($"FAILED {t.Feature}: {t}: {e.GetType().Name}: {e.Message}");
                    failed++;
                }
            }
            harmony.UnpatchAll(harmony.Id);
            var left = PatchTargets.All.Select(t => PatchTargets.Resolve(server, t, out _)).Where(m => m is not null)
                .Count(m => HarmonyLib.Harmony.GetPatchInfo(m!) is { } i && (i.Prefixes.Count > 0 || i.Postfixes.Count > 0));
            Console.WriteLine($"{applied} patched, {failed} failed; {left} still patched after unpatching");
            return failed == 0 && left == 0 ? 0 : 1;
        }
    }
}
