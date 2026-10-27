using System;
using System.Collections.Generic;

namespace Dereth.Decal
{
    /// <summary>The character's place as Decal reads it each frame.</summary>
    public struct Place
    {
        public uint Landcell;

        /// <summary>Its place in the landblock, in meters.</summary>
        public double X, Y, Z;

        /// <summary>Degrees clockwise from north.</summary>
        public double Heading;
    }

    /// <summary>
    /// What Decal hands the plugin host, and what the host asks back: every message the game
    /// receives and sends, each frame with the character's place and whether the game's window is
    /// in front with its chat line closed, the lines the player types, the lines the chat window is
    /// about to show, the keys the player presses, the player's key bindings and the selection; the
    /// ops that come back are carried out at once (<see cref="Ops"/>).
    /// </summary>
    public sealed class Driver : IDisposable
    {
        private readonly NativeHost host;
        private readonly ChatHides hides = new ChatHides();

        /// <param name="keys">The game's window, which the game's keys are sent to; none, and none are.</param>
        /// <param name="setting">Where a setting the player changed is kept: its name and value.</param>
        public Driver(NativeHost host, IRetail retail, IMarkBackend marks, IKeyWindow? keys = null, Action<string, string>? setting = null)
        {
            this.host = host;
            Views = new Views((key, control, ev, value) => host.ViewEvent(key, control, ev, value));
            Marks = new Marks(marks);
            Keys = keys == null ? null : new Keys(keys);
            Ops = new Ops(retail, hides, Views, Marks, Keys, setting);
        }

        public Views Views { get; }

        public Marks Marks { get; }

        /// <summary>The game's keys the host presses, when there is a window to send them to.</summary>
        public Keys? Keys { get; }

        public Ops Ops { get; }

        /// <summary>The host behind it.</summary>
        public NativeHost Host => host;

        /// <summary>
        /// A message the game received. Its ops are carried out at once, so a line a plugin hides is
        /// known before the chat window shows it.
        /// </summary>
        public void Server(byte[] message, double now)
        {
            host.ServerMessage(message, now);
            Drain(now);
        }

        /// <summary>A message the game sent.</summary>
        public void Client(byte[] message, double now)
        {
            host.ClientMessage(message, now);
            Drain(now);
        }

        /// <summary>
        /// One frame, the character's place given when the game has one, with whether the game's
        /// window is in front and whether its chat line is open: the game's keys go to it only
        /// while it is in front with the chat line closed.
        /// </summary>
        public void Frame(double now, Place? place, bool inFront = false, bool chatOpen = false)
        {
            if (place is Place p)
                host.Place(p.Landcell, p.X, p.Y, p.Z, p.Heading);
            host.GameState(inFront, chatOpen);
            host.Frame(now);
            Drain(now);
            Ops.Frame();
            Views.Poll();
        }

        /// <summary>A line the player typed: whether a plugin claims it, and the game is not to send it.</summary>
        public bool Typed(string line, double now)
        {
            var claimed = host.Typed(line);
            Drain(now);
            return claimed;
        }

        /// <summary>A line the chat window is about to show: whether a plugin hides it.</summary>
        public bool ChatLine(string text, double now) => hides.Take(text, now);

        /// <summary>
        /// A key message the game's window heard: one of the keys the host pressed is let be; a key
        /// the player pressed with the chat line closed moves the character, or might, and a
        /// plugin's movement gives way to it.
        /// </summary>
        public void KeyMessage(int message, int wParam, int lParam, bool chatOpen)
        {
            if (Keys != null && Keys.Echo(message, wParam, lParam))
                return;
            if (message == Keys.KeyDown && !chatOpen)
                host.PlayerMoved();
        }

        /// <summary>
        /// The player's key bindings, from <paramref name="lookup"/>: the game's key map's virtual-key
        /// code for each of the game's actions the host presses keys for (0 for none).
        /// </summary>
        public void Bindings(Func<string, int> lookup)
        {
            var bound = new Dictionary<string, object?>();
            foreach (var name in Keys.Actions)
            {
                var code = lookup(name);
                if (code > 0 && code < 0x100)
                    bound[name] = (double)code;
            }
            host.Bindings(Json.Write(bound));
        }

        /// <summary>The object the player selected.</summary>
        public void Selected(int id) => host.Selected(unchecked((uint)id));

        private void Drain(double now)
        {
            List<object?> ops = host.Ops();
            if (ops.Count > 0)
                Ops.Apply(ops, now);
        }

        /// <summary>The host gone, and the marks it drew; the views go with Decal's plugin.</summary>
        public void Dispose()
        {
            Marks.Clear();
            host.Dispose();
        }
    }
}
