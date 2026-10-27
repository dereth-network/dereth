using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Dereth.Network.Companion
{
    /// <summary>
    /// A world's plugin policy (<c>policy_version = 1</c>): which kinds of automation it allows, and
    /// the notes it shows players. The companion types a command only when the player clicks, so no
    /// rule of the policy stops it; it shows the world's rules beside its own window.
    /// </summary>
    public sealed class PluginPolicy
    {
        public string World { get; private set; } = "";
        /// <summary>Each automation domain the policy names, with its rule: <c>allow</c>, <c>interactive-only</c> or <c>deny</c>.</summary>
        public SortedDictionary<string, string> Automation { get; } = new SortedDictionary<string, string>(StringComparer.Ordinal);
        public string Notes { get; private set; } = "";
        public string NotesUrl { get; private set; } = "";

        /// <summary>The policy in <paramref name="text"/>, or null with the reason.</summary>
        public static PluginPolicy? Parse(string text, out string error)
        {
            error = "";
            var p = new PluginPolicy();
            var table = "";
            var version = 0;
            var n = 0;
            foreach (var raw in text.Split('\n'))
            {
                n++;
                var line = raw.Trim();
                if (line.Length == 0 || line[0] == '#')
                    continue;
                if (line[0] == '[')
                {
                    var end = line.IndexOf(']');
                    if (end < 0)
                    {
                        error = "line " + n + ": a table's name does not end";
                        return null;
                    }
                    table = line.Substring(1, end - 1).Trim();
                    continue;
                }
                var eq = line.IndexOf('=');
                if (eq <= 0)
                {
                    error = "line " + n + ": not a key and a value";
                    return null;
                }
                var key = line.Substring(0, eq).Trim();
                if (!ReadValue(line.Substring(eq + 1).Trim(), out var value))
                {
                    error = "line " + n + ": a value that does not read";
                    return null;
                }
                switch (table)
                {
                    case "":
                        if (key == "policy_version")
                            int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out version);
                        else if (key == "world")
                            p.World = value;
                        break;
                    case "automation":
                        if (value != "allow" && value != "interactive-only" && value != "deny")
                        {
                            error = "line " + n + ": " + key + " is " + value + ", not allow, interactive-only or deny";
                            return null;
                        }
                        p.Automation[key] = value;
                        break;
                    case "notes":
                        if (key == "text")
                            p.Notes = value;
                        else if (key == "url")
                            p.NotesUrl = value;
                        break;
                }
            }
            if (version != 1)
            {
                error = "it is not a version 1 policy";
                return null;
            }
            return p;
        }

        /// <summary>The rules in a line: the domains the world refuses, and those it allows only on a click.</summary>
        public string Summary()
        {
            var denied = new List<string>();
            var click = new List<string>();
            foreach (var kv in Automation)
            {
                if (kv.Key == "default")
                    continue;
                if (kv.Value == "deny")
                    denied.Add(kv.Key);
                else if (kv.Value == "interactive-only")
                    click.Add(kv.Key);
            }
            var sb = new StringBuilder();
            if (denied.Count > 0)
                sb.Append("No ").Append(string.Join(", ", denied.ToArray())).Append(" automation. ");
            if (click.Count > 0)
                sb.Append("On a click only: ").Append(string.Join(", ", click.ToArray())).Append(". ");
            if (Automation.TryGetValue("default", out var d) && d != "allow")
                sb.Append("Anything else: ").Append(d == "deny" ? "no" : "on a click only").Append(". ");
            if (sb.Length == 0)
                sb.Append("Every kind of automation is allowed. ");
            return sb.ToString().TrimEnd();
        }

        private static bool ReadValue(string v, out string value)
        {
            value = "";
            var hash = -1;
            if (v.StartsWith("\"", StringComparison.Ordinal))
            {
                var sb = new StringBuilder();
                for (var i = 1; i < v.Length; i++)
                {
                    var c = v[i];
                    if (c == '"')
                    {
                        value = sb.ToString();
                        return Rest(v.Substring(i + 1));
                    }
                    if (c == '\\' && i + 1 < v.Length)
                    {
                        var e = v[++i];
                        sb.Append(e == 'n' ? '\n' : e == 't' ? '\t' : e);
                        continue;
                    }
                    sb.Append(c);
                }
                return false;
            }
            if (v.StartsWith("'", StringComparison.Ordinal))
            {
                var end = v.IndexOf('\'', 1);
                if (end < 0)
                    return false;
                value = v.Substring(1, end - 1);
                return Rest(v.Substring(end + 1));
            }
            hash = v.IndexOf('#');
            value = (hash >= 0 ? v.Substring(0, hash) : v).Trim();
            return value.Length > 0;
        }

        private static bool Rest(string rest)
        {
            var r = rest.Trim();
            return r.Length == 0 || r[0] == '#';
        }
    }
}
