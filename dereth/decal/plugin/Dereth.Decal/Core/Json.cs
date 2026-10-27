using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Dereth.Decal
{
    /// <summary>
    /// The JSON that crosses the host library's interface. An object reads as a
    /// <see cref="Dictionary{TKey,TValue}"/> of string to value, an array as a <see cref="List{T}"/>,
    /// a number as a <see cref="double"/>, and <c>null</c> as null; <see cref="Write"/> writes the same
    /// shapes back.
    /// </summary>
    public static class Json
    {
        /// <summary>The value <paramref name="text"/> holds.</summary>
        /// <exception cref="FormatException">The text is not one JSON value.</exception>
        public static object? Parse(string text)
        {
            var i = 0;
            var v = Value(text, ref i, 0);
            Space(text, ref i);
            if (i != text.Length)
                throw new FormatException("text after the JSON value at " + i);
            return v;
        }

        /// <summary>A member of an object, or null.</summary>
        public static object? Get(object? obj, string key) =>
            obj is Dictionary<string, object?> d && d.TryGetValue(key, out var v) ? v : null;

        public static string Str(object? v) => v as string ?? "";

        public static bool Bool(object? v) => v is bool b && b;

        public static double? Num(object? v) => v is double d ? d : (double?)null;

        /// <summary>A whole number that fits an <see cref="int"/>, or null.</summary>
        public static int? Int(object? v) =>
            v is double d && d >= int.MinValue && d <= int.MaxValue && Math.Floor(d) == d ? (int)d : (int?)null;

        /// <summary>
        /// An object id, or another unsigned 32-bit number, as the signed number Decal's calls take
        /// (the same 32 bits); null when it is not one.
        /// </summary>
        public static int? Id(object? v) =>
            v is double d && d >= 0 && d <= uint.MaxValue && Math.Floor(d) == d ? unchecked((int)(uint)d) : (int?)null;

        public static List<object?> List(object? v) => v as List<object?> ?? new List<object?>();

        /// <summary>The JSON text of <paramref name="v"/>: a dictionary, a list, a string, a number, a bool or null.</summary>
        public static string Write(object? v)
        {
            var sb = new StringBuilder();
            Write(sb, v);
            return sb.ToString();
        }

        private static void Write(StringBuilder sb, object? v)
        {
            switch (v)
            {
                case null:
                    sb.Append("null");
                    break;
                case bool b:
                    sb.Append(b ? "true" : "false");
                    break;
                case string s:
                    Quote(sb, s);
                    break;
                case int n:
                    sb.Append(n.ToString(CultureInfo.InvariantCulture));
                    break;
                case uint n:
                    sb.Append(n.ToString(CultureInfo.InvariantCulture));
                    break;
                case long n:
                    sb.Append(n.ToString(CultureInfo.InvariantCulture));
                    break;
                case double d:
                    sb.Append(double.IsNaN(d) || double.IsInfinity(d) ? "null" : d.ToString("R", CultureInfo.InvariantCulture));
                    break;
                case IDictionary<string, object?> map:
                    {
                        sb.Append('{');
                        var first = true;
                        foreach (var kv in map)
                        {
                            if (!first)
                                sb.Append(',');
                            first = false;
                            Quote(sb, kv.Key);
                            sb.Append(':');
                            Write(sb, kv.Value);
                        }
                        sb.Append('}');
                        break;
                    }
                case System.Collections.IEnumerable list:
                    {
                        sb.Append('[');
                        var first = true;
                        foreach (var item in list)
                        {
                            if (!first)
                                sb.Append(',');
                            first = false;
                            Write(sb, item);
                        }
                        sb.Append(']');
                        break;
                    }
                default:
                    Quote(sb, v.ToString() ?? "");
                    break;
            }
        }

        private static void Quote(StringBuilder sb, string s)
        {
            sb.Append('"');
            foreach (var c in s)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < ' ')
                            sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        else
                            sb.Append(c);
                        break;
                }
            }
            sb.Append('"');
        }

        private const int MaxDepth = 64;

        private static object? Value(string s, ref int i, int depth)
        {
            if (depth > MaxDepth)
                throw new FormatException("JSON nested too deep");
            Space(s, ref i);
            if (i >= s.Length)
                throw new FormatException("JSON ends early");
            switch (s[i])
            {
                case '{':
                    {
                        i++;
                        var obj = new Dictionary<string, object?>(StringComparer.Ordinal);
                        Space(s, ref i);
                        if (i < s.Length && s[i] == '}')
                        {
                            i++;
                            return obj;
                        }
                        while (true)
                        {
                            Space(s, ref i);
                            var key = String(s, ref i);
                            Space(s, ref i);
                            Expect(s, ref i, ':');
                            obj[key] = Value(s, ref i, depth + 1);
                            Space(s, ref i);
                            if (i < s.Length && s[i] == ',')
                            {
                                i++;
                                continue;
                            }
                            Expect(s, ref i, '}');
                            return obj;
                        }
                    }
                case '[':
                    {
                        i++;
                        var list = new List<object?>();
                        Space(s, ref i);
                        if (i < s.Length && s[i] == ']')
                        {
                            i++;
                            return list;
                        }
                        while (true)
                        {
                            list.Add(Value(s, ref i, depth + 1));
                            Space(s, ref i);
                            if (i < s.Length && s[i] == ',')
                            {
                                i++;
                                continue;
                            }
                            Expect(s, ref i, ']');
                            return list;
                        }
                    }
                case '"':
                    return String(s, ref i);
                case 't':
                    Word(s, ref i, "true");
                    return true;
                case 'f':
                    Word(s, ref i, "false");
                    return false;
                case 'n':
                    Word(s, ref i, "null");
                    return null;
                default:
                    return Number(s, ref i);
            }
        }

        private static void Space(string s, ref int i)
        {
            while (i < s.Length && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r'))
                i++;
        }

        private static void Expect(string s, ref int i, char c)
        {
            if (i >= s.Length || s[i] != c)
                throw new FormatException("expected '" + c + "' at " + i);
            i++;
        }

        private static void Word(string s, ref int i, string word)
        {
            if (string.CompareOrdinal(s, i, word, 0, word.Length) != 0)
                throw new FormatException("expected " + word + " at " + i);
            i += word.Length;
        }

        private static double Number(string s, ref int i)
        {
            var start = i;
            if (i < s.Length && s[i] == '-')
                i++;
            while (i < s.Length && (char.IsDigit(s[i]) || s[i] == '.' || s[i] == 'e' || s[i] == 'E' || s[i] == '+' || s[i] == '-'))
                i++;
            if (i == start)
                throw new FormatException("expected a value at " + i);
            if (!double.TryParse(s.Substring(start, i - start), NumberStyles.Float, CultureInfo.InvariantCulture, out var d))
                throw new FormatException("not a number at " + start);
            return d;
        }

        private static string String(string s, ref int i)
        {
            Expect(s, ref i, '"');
            var sb = new StringBuilder();
            while (true)
            {
                if (i >= s.Length)
                    throw new FormatException("a string does not end");
                var c = s[i++];
                if (c == '"')
                    return sb.ToString();
                if (c != '\\')
                {
                    sb.Append(c);
                    continue;
                }
                if (i >= s.Length)
                    throw new FormatException("a string does not end");
                var e = s[i++];
                switch (e)
                {
                    case '"': sb.Append('"'); break;
                    case '\\': sb.Append('\\'); break;
                    case '/': sb.Append('/'); break;
                    case 'b': sb.Append('\b'); break;
                    case 'f': sb.Append('\f'); break;
                    case 'n': sb.Append('\n'); break;
                    case 'r': sb.Append('\r'); break;
                    case 't': sb.Append('\t'); break;
                    case 'u':
                        if (i + 4 > s.Length ||
                            !int.TryParse(s.Substring(i, 4), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var code))
                            throw new FormatException("a bad \\u escape at " + i);
                        sb.Append((char)code);
                        i += 4;
                        break;
                    default:
                        throw new FormatException("a bad escape at " + i);
                }
            }
        }
    }
}
