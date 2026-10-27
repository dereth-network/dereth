using System.Collections.Generic;

namespace Dereth.Decal
{
    /// <summary>
    /// The chat lines a plugin hides: each is kept from showing once, when the chat window is about
    /// to show a line with its text. A line the window never shows is let go after
    /// <see cref="Keep"/> seconds.
    /// </summary>
    public sealed class ChatHides
    {
        /// <summary>How long a line to hide waits for the window to show it, in seconds.</summary>
        public const double Keep = 5.0;

        private readonly List<KeyValuePair<string, double>> waiting = new List<KeyValuePair<string, double>>();

        /// <summary>The next line with <paramref name="text"/> is not to show; asked at <paramref name="now"/>.</summary>
        public void Add(string text, double now) => waiting.Add(new KeyValuePair<string, double>(Line(text), now));

        /// <summary>Whether the line the window is about to show at <paramref name="now"/> is one to hide; if so it is hidden only once.</summary>
        public bool Take(string text, double now)
        {
            waiting.RemoveAll(w => now - w.Value > Keep);
            var line = Line(text);
            var i = waiting.FindIndex(w => w.Key == line);
            if (i < 0)
                return false;
            waiting.RemoveAt(i);
            return true;
        }

        /// <summary>The lines waiting.</summary>
        public int Count => waiting.Count;

        /// <summary>A line as it compares: without the line ending the window adds.</summary>
        private static string Line(string text) => text.TrimEnd('\n', '\r');
    }
}
