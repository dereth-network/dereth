using System.Collections.Generic;
using System.Text;

namespace Dereth.Network.Companion
{
    /// <summary>Chat text as the game shows it, read back into plain lines.</summary>
    public static class ChatText
    {
        /// <summary>
        /// The text without the game's name links: <c>&lt;Tell:IIDString:12:Bob@coldeve&gt;Bob@coldeve&lt;\Tell&gt;</c>
        /// reads as <c>Bob@coldeve</c>.
        /// </summary>
        public static string StripLinks(string text)
        {
            if (text.IndexOf('<') < 0)
                return text;
            var sb = new StringBuilder(text.Length);
            var i = 0;
            while (i < text.Length)
            {
                if (text[i] == '<')
                {
                    if (string.CompareOrdinal(text, i, "<Tell:", 0, 6) == 0)
                    {
                        var end = text.IndexOf('>', i);
                        if (end > 0)
                        {
                            i = end + 1;
                            continue;
                        }
                    }
                    else if (string.CompareOrdinal(text, i, "<\\Tell>", 0, 7) == 0)
                    {
                        i += 7;
                        continue;
                    }
                }
                sb.Append(text[i]);
                i++;
            }
            return sb.ToString();
        }

        /// <summary><paramref name="text"/> broken into lines of at most <paramref name="width"/> characters, at spaces where it can be.</summary>
        public static List<string> Wrap(string text, int width)
        {
            var lines = new List<string>();
            foreach (var para in text.Split('\n'))
            {
                var rest = para.Trim();
                while (rest.Length > width)
                {
                    var cut = rest.LastIndexOf(' ', width);
                    if (cut <= 0)
                        cut = width;
                    lines.Add(rest.Substring(0, cut).TrimEnd());
                    rest = rest.Substring(cut).TrimStart();
                }
                if (rest.Length > 0)
                    lines.Add(rest);
            }
            return lines;
        }

        /// <summary>The text's lines, links taken out, each without its line end and trailing spaces; no empty lines.</summary>
        public static List<string> Lines(string text)
        {
            var lines = new List<string>();
            foreach (var raw in StripLinks(text).Split('\n'))
            {
                var line = raw.TrimEnd('\r', ' ', '\t');
                if (line.Length > 0)
                    lines.Add(line);
            }
            return lines;
        }
    }

    /// <summary>A network address, <c>Name@world</c>: a character's name, then its world's slug.</summary>
    public static class NetAddress
    {
        /// <summary>The name and the world of <paramref name="address"/>, split at its last <c>@</c>.</summary>
        public static bool TryParse(string address, out string name, out string world)
        {
            name = "";
            world = "";
            var text = address.Trim();
            var at = text.LastIndexOf('@');
            if (at <= 0 || at == text.Length - 1)
                return false;
            var w = text.Substring(at + 1);
            if (!IsSlug(w))
                return false;
            name = text.Substring(0, at).Trim();
            world = w;
            return name.Length > 0;
        }

        /// <summary>Whether <paramref name="text"/> is a world's slug: lowercase letters, digits and inner hyphens.</summary>
        public static bool IsSlug(string text)
        {
            if (text.Length < 2 || text.Length > 40 || text[0] == '-' || text[text.Length - 1] == '-')
                return false;
            foreach (var c in text)
                if (!(c >= 'a' && c <= 'z') && !(c >= '0' && c <= '9') && c != '-')
                    return false;
            return true;
        }

        /// <summary>Whether <paramref name="address"/> names a channel, <c>name@channel</c>.</summary>
        public static bool IsChannel(string address) =>
            address.EndsWith("@channel", System.StringComparison.Ordinal);
    }
}
