// Every ACE method the mod patches, resolved in an ACE.Server.dll on disk by the same table and
// resolver the mod uses at load; and, given the built mod, every hook checked against its target
// the way Harmony binds it (by argument position, `__instance` and `__result`). Nothing here runs
// ACE: its assemblies are only loaded for reading, in a context of their own.

using System.Reflection;
using System.Runtime.Loader;

namespace Dereth.Network.Harness;

internal sealed class AceContext : AssemblyLoadContext
{
    private readonly string dir;
    private readonly AssemblyDependencyResolver resolver;

    public AceContext(string dir) : base("ace-for-reading", isCollectible: true)
    {
        this.dir = dir;
        resolver = new AssemblyDependencyResolver(Path.Combine(dir, "ACE.Server.dll"));
    }

    protected override Assembly? Load(AssemblyName name)
    {
        var p = resolver.ResolveAssemblyToPath(name);
        if (p is not null)
            return LoadFromAssemblyPath(p);
        var f = Path.Combine(dir, name.Name + ".dll");
        return File.Exists(f) ? LoadFromAssemblyPath(f) : null;
    }
}

internal static class TargetChecks
{
    public static void Run(Checks c, string aceDir, string? modDll)
    {
        const string s = "targets";
        var acePath = Path.GetFullPath(Path.Combine(aceDir, "ACE.Server.dll"));
        if (!File.Exists(acePath))
        {
            c.Fail(s, "no ACE.Server.dll in " + aceDir);
            return;
        }
        var ctx = new AceContext(Path.GetFullPath(aceDir));
        try
        {
            var ace = ctx.LoadFromAssemblyPath(acePath);
            var resolved = new Dictionary<PatchTarget, MethodBase>();
            foreach (var t in PatchTargets.All)
            {
                var m = PatchTargets.Resolve(ace, t, out var why);
                c.Check(s, m is not null, $"{t.Feature}: {t} {(m is null ? "does not resolve: " + why : "resolves")}");
                if (m is not null)
                    resolved[t] = m;
            }
            modDll ??= FindMod();
            if (modDll is null || !File.Exists(modDll))
            {
                c.NotRun("targets/hooks", "the built Dereth.Network.dll was not found; pass --mod");
                return;
            }
            Hooks(c, ctx, modDll, resolved);
        }
        finally
        {
            ctx.Unload();
        }
    }

    private static string? FindMod()
    {
        // The harness and the mod build side by side under the same artifacts folder.
        var bin = new DirectoryInfo(AppContext.BaseDirectory);
        while (bin is not null && bin.Name != "bin")
            bin = bin.Parent;
        if (bin is null)
            return null;
        foreach (var config in new[] { "release", "debug" })
        {
            var p = Path.Combine(bin.FullName, "Dereth.Network", config, "Dereth.Network.dll");
            if (File.Exists(p))
                return p;
        }
        return null;
    }

    private static void Hooks(Checks c, AceContext ctx, string modDll, Dictionary<PatchTarget, MethodBase> resolved)
    {
        const string s = "targets/hooks";
        var mod = ctx.LoadFromAssemblyPath(Path.GetFullPath(modDll));
        Type[] types;
        try
        {
            types = mod.GetTypes();
            c.Pass(s, "every type of the mod loads beside ACE (as ACE's command registration reads them)");
        }
        catch (ReflectionTypeLoadException e)
        {
            c.Fail(s, "the mod's types do not all load: " + string.Join("; ", e.LoaderExceptions.Select(x => x?.Message)));
            return;
        }
        var entry = mod.GetType("Dereth.Network.Mod");
        var iface = entry?.GetInterfaces().FirstOrDefault(i => i.FullName == "ACE.Server.Mods.IHarmonyMod");
        c.Check(s, entry is not null && iface is not null && entry.GetConstructor(Type.EmptyTypes) is not null,
            "Dereth.Network.Mod is the IHarmonyMod ACE's loader looks for, with a constructor it can call");
        var patches = mod.GetType("Dereth.Network.Patches");
        if (patches is null)
        {
            c.Fail(s, "no Dereth.Network.Patches");
            return;
        }
        foreach (var (t, original) in resolved)
        {
            foreach (var (kind, name) in new[] { ("prefix", t.Prefix), ("postfix", t.Postfix) })
            {
                if (name is null)
                    continue;
                var hook = patches.GetMethod(name, BindingFlags.Public | BindingFlags.Static);
                if (hook is null)
                {
                    c.Fail(s, $"{t}: no {kind} {name}");
                    continue;
                }
                var why = Binds(hook, original, kind);
                c.Check(s, why is null, $"{t}: {kind} {name} {(why is null ? "binds" : "does not bind: " + why)}");
            }
        }
        var commandAttr = ctx.LoadFromAssemblyName(new AssemblyName("ACE.Server")).GetType("ACE.Server.Command.CommandHandlerAttribute");
        var rows = types.SelectMany(x => x.GetMethods())
            .SelectMany(m => m.GetCustomAttributesData().Where(a => a.AttributeType == commandAttr).Select(a => (m, a)))
            .ToList();
        foreach (var (m, a) in rows)
        {
            var ps = m.GetParameters();
            var ok = m.IsStatic && m.ReturnType == typeof(void) && ps.Length == 2
                && ps[0].ParameterType.FullName == "ACE.Server.Network.Session" && ps[1].ParameterType == typeof(string[]);
            c.Check(s, ok, $"the command {a.ConstructorArguments[0].Value} is a handler ACE can register");
        }
        var names = rows.Select(r => r.a.ConstructorArguments[0].Value as string ?? "").Order(StringComparer.Ordinal).ToList();
        c.Check(s, names.SequenceEqual(["derethnetwork", "dn", "dng"]),
            $"the command is registered as dn and as derethnetwork, and its Global room line as dng (found {string.Join(", ", names)})");
        // ACE's AccessLevel.Player and CommandHandlerFlag.None are both 0: every player, and the console.
        c.Check(s, rows.All(r => Convert.ToInt32(r.a.ConstructorArguments[1].Value) == 0
                && Convert.ToInt32(r.a.ConstructorArguments[2].Value) == 0),
            "every name is every player's and the console's, with the operator's words checked by the handler");
    }

    /// <summary>Null when Harmony can bind `hook` to `original`; else why not.</summary>
    private static string? Binds(MethodInfo hook, MethodBase original, string kind)
    {
        if (kind == "prefix" && hook.ReturnType != typeof(void) && hook.ReturnType != typeof(bool))
            return "a prefix returns void or bool";
        if (kind == "postfix" && hook.ReturnType != typeof(void))
            return "a postfix returns void";
        var ps = original.GetParameters();
        foreach (var p in hook.GetParameters())
        {
            var n = p.Name ?? "";
            if (n == "__instance")
            {
                if (original.IsStatic)
                    return "__instance on a static method";
                if (!p.ParameterType.IsAssignableFrom(original.DeclaringType))
                    return $"__instance is {p.ParameterType}, the target is on {original.DeclaringType}";
                continue;
            }
            if (n == "__result")
            {
                // A prefix that sets the result takes it by reference.
                var type = p.ParameterType.IsByRef ? p.ParameterType.GetElementType() : p.ParameterType;
                if (original is not MethodInfo mi || mi.ReturnType != type)
                    return "__result does not match the return type";
                continue;
            }
            if (n.StartsWith("__", StringComparison.Ordinal) && int.TryParse(n[2..], out var i))
            {
                if (i >= ps.Length)
                    return $"{n}: the target has {ps.Length} argument(s)";
                // A hook may take an argument by reference, to change it for the method.
                var type = p.ParameterType.IsByRef ? p.ParameterType.GetElementType() : p.ParameterType;
                if (ps[i].ParameterType != type)
                    return $"{n} is {p.ParameterType}, the target's is {ps[i].ParameterType}";
                continue;
            }
            return $"{n} is not an argument Harmony gives by position";
        }
        return null;
    }
}
