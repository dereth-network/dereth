using System;
using System.Collections.Generic;
using System.IO;

namespace Dereth.Decal
{
    /// <summary>
    /// Where the plugin host finds things, as the host library's settings say it. Every plugin
    /// found is listed in the plugin panel: the launcher's are on the first time the host finds
    /// them, and the others are off until the player ticks each.
    /// <list type="bullet">
    /// <item>the plugins in the Dereth client's own plugins folder,
    /// <c>%APPDATA%\Dereth\client\plugins</c>, and the plugins the launcher installed, so a
    /// plugin installed once is found by both clients; a <c>plugins = …</c> line names another
    /// folder instead of both, and each more line one more;</item>
    /// <item>the plugins that come with it in the <c>plugins</c> folder beside it;</item>
    /// <item>the plugins' stores and data folders in <c>%APPDATA%\Dereth\decal</c>, apart from the
    /// Dereth client's;</item>
    /// <item>the game's data files in the game's own folder;</item>
    /// <item>the bus to the Dereth clients on this machine in their settings folder, unless a
    /// <c>peers = off</c> line turns it off;</item>
    /// <item>the world's plugin policy in the file a <c>policy = …</c> line names;</item>
    /// <item>whether plugins may use the game's keys: a <c>keys = on</c> line lets them, and the
    /// plugin panel's switch writes the line (<see cref="Write"/>).</item>
    /// </list>
    /// The lines are read from <c>settings.txt</c> in <c>%APPDATA%\Dereth\decal</c>.
    /// </summary>
    public static class Settings
    {
        /// <summary>The settings file's name.</summary>
        public const string File = "settings.txt";

        /// <summary>The host's own folder in <c>%APPDATA%</c>.</summary>
        public static string DataFolder(string appData) => Path.Combine(appData, "Dereth", "decal");

        /// <summary>The Dereth client's plugins folder in <c>%APPDATA%</c>.</summary>
        public static string ClientPlugins(string appData) => Path.Combine(appData, "Dereth", "client", "plugins");

        /// <summary>
        /// The host library's settings, for the plugin installed in <paramref name="installed"/>, the
        /// player's <paramref name="appData"/> and the game in <paramref name="game"/> (null when not
        /// known), with <paramref name="lines"/> from the settings file. <paramref name="notes"/> gets
        /// a line for the player for each setting that did not read.
        /// </summary>
        public static string HostJson(string installed, string appData, string? game, IEnumerable<string> lines, List<string> notes)
        {
            var data = DataFolder(appData);
            var named = new List<object?>();
            var peers = true;
            var keys = false;
            string? policy = null;
            foreach (var raw in lines)
            {
                var line = raw.Trim();
                if (line.Length == 0 || line.StartsWith("#", StringComparison.Ordinal))
                    continue;
                var eq = line.IndexOf('=');
                if (eq <= 0)
                {
                    notes.Add("A line of " + File + " is not \"name = value\": " + line);
                    continue;
                }
                var name = line.Substring(0, eq).Trim();
                var value = line.Substring(eq + 1).Trim();
                switch (name)
                {
                    case "plugins":
                        named.Add(value);
                        break;
                    case "peers":
                        peers = value != "off";
                        break;
                    case "keys":
                        if (value == "on" || value == "off")
                            keys = value == "on";
                        else
                            notes.Add(File + "'s keys is on or off, not \"" + value + "\".");
                        break;
                    case "policy":
                        try
                        {
                            policy = System.IO.File.ReadAllText(value);
                        }
                        catch (Exception e) when (e is IOException || e is UnauthorizedAccessException || e is ArgumentException || e is NotSupportedException)
                        {
                            notes.Add("The world's plugin policy did not read from " + value + ": " + e.Message);
                        }
                        break;
                    default:
                        notes.Add(File + " has no setting \"" + name + "\".");
                        break;
                }
            }
            var settings = new Dictionary<string, object?>
            {
                ["found"] = named.Count > 0 ? named : new List<object?> { ClientPlugins(appData) },
                ["data"] = data,
                ["launcher"] = named.Count == 0,
                ["keys"] = keys,
            };
            var bundled = Path.Combine(installed, "plugins");
            if (Directory.Exists(bundled))
                settings["bundled"] = bundled;
            if (game != null)
                settings["dats"] = game;
            if (peers)
                settings["peers"] = Path.Combine(appData, "Dereth", "client", "plugin-peers");
            if (policy != null)
                settings["policy"] = policy;
            return Json.Write(settings);
        }

        /// <summary>
        /// The settings file's lines with <paramref name="name"/> set to <paramref name="value"/>:
        /// its first line of that name changed, any more taken out, or a line added when there was
        /// none; every other line kept as it was.
        /// </summary>
        public static List<string> With(IEnumerable<string> lines, string name, string value)
        {
            var kept = new List<string>();
            var set = false;
            foreach (var raw in lines)
            {
                var eq = raw.IndexOf('=');
                var named = eq > 0
                    && !raw.TrimStart().StartsWith("#", StringComparison.Ordinal)
                    && raw.Substring(0, eq).Trim() == name;
                if (!named)
                {
                    kept.Add(raw);
                    continue;
                }
                if (!set)
                    kept.Add(name + " = " + value);
                set = true;
            }
            if (!set)
                kept.Add(name + " = " + value);
            return kept;
        }

        /// <summary>
        /// Keep <paramref name="name"/> as <paramref name="value"/> in the settings file
        /// <paramref name="file"/> (<see cref="With"/>), making it and its folder when there are none.
        /// </summary>
        public static void Write(string file, string name, string value)
        {
            var lines = System.IO.File.Exists(file) ? System.IO.File.ReadAllLines(file) : new string[0];
            var dir = Path.GetDirectoryName(file);
            if (!string.IsNullOrEmpty(dir))
                Directory.CreateDirectory(dir);
            System.IO.File.WriteAllText(file, string.Join("\n", With(lines, name, value)) + "\n");
        }
    }
}
