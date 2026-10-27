using System;
using System.Collections.Generic;
using System.Drawing;

namespace Dereth.Decal
{
    /// <summary>What an event on a view's control tells: the control's name, the event and its value.</summary>
    public delegate void ControlEvent(string control, string ev, object? value);

    /// <summary>Decal's views, as the plugin's view host makes them.</summary>
    public interface IViewBackend
    {
        /// <summary>
        /// A view from its XML, with <paramref name="controls"/>' states set and each control that
        /// stands for a node (its state has a <c>node</c>) wired to <paramref name="events"/>.
        /// </summary>
        IView Load(string xml, IReadOnlyList<Dictionary<string, object?>> controls, ControlEvent events);
    }

    /// <summary>One of Decal's views.</summary>
    public interface IView : IDisposable
    {
        /// <summary>Whether it shows (the player opens and closes it from Decal's bar too).</summary>
        bool Activated { get; set; }

        Rectangle Position { get; set; }

        /// <summary>One control's state set: its text, tick, rows, options or value, as its kind has them.</summary>
        void Apply(Dictionary<string, object?> state);
    }

    /// <summary>
    /// The plugins' windows as Decal views: built from the host's <c>view</c> ops, their controls
    /// kept up by <c>update</c>, shown and hidden by <c>show</c>, gone with <c>dispose</c>. Events on
    /// their controls, and the player opening or closing one, go back to the host. The views are
    /// made only while Decal's plugin is running (from the character's login to its logout); until
    /// then each window's latest state is kept and its view made when the plugin starts.
    /// </summary>
    public sealed class Views
    {
        private sealed class Window
        {
            public string Xml = "";
            public readonly Dictionary<string, Dictionary<string, object?>> Controls =
                new Dictionary<string, Dictionary<string, object?>>(StringComparer.Ordinal);
            public bool Open;
            public IView? View;
        }

        private readonly Dictionary<string, Window> windows = new Dictionary<string, Window>(StringComparer.Ordinal);
        private readonly Action<string, string, string, object?> send;
        private IViewBackend? backend;
        private bool applying;

        /// <param name="send">Where an event goes: the window's key, the control, the event, its value.</param>
        public Views(Action<string, string, string, object?> send) => this.send = send;

        /// <summary>The windows with views, by key.</summary>
        public IEnumerable<string> Keys => windows.Keys;

        /// <summary>Whether the window <paramref name="key"/> is open, as last told.</summary>
        public bool IsOpen(string key) => windows.TryGetValue(key, out var w) && w.Open;

        /// <summary>Decal's plugin is running: every window's view is made.</summary>
        public void Attach(IViewBackend views)
        {
            backend = views;
            foreach (var kv in windows)
                Build(kv.Key, kv.Value, null);
        }

        /// <summary>Decal's plugin stopped: the views go, the windows are kept.</summary>
        public void Detach()
        {
            foreach (var w in windows.Values)
            {
                w.View?.Dispose();
                w.View = null;
            }
            backend = null;
        }

        /// <summary>One of the host's view ops.</summary>
        public void Apply(Dictionary<string, object?> op)
        {
            var key = Json.Str(Json.Get(op, "key"));
            switch (Json.Str(Json.Get(op, "op")))
            {
                case "view":
                    {
                        windows.TryGetValue(key, out var old);
                        var w = new Window
                        {
                            Xml = Json.Str(Json.Get(op, "xml")),
                            Open = Json.Bool(Json.Get(op, "open")),
                        };
                        foreach (var c in Json.List(Json.Get(op, "controls")))
                        {
                            if (c is Dictionary<string, object?> state)
                                w.Controls[Json.Str(Json.Get(state, "name"))] = state;
                        }
                        windows[key] = w;
                        var place = old?.View?.Position;
                        old?.View?.Dispose();
                        Build(key, w, place);
                        break;
                    }
                case "update":
                    {
                        if (!windows.TryGetValue(key, out var w))
                            return;
                        foreach (var c in Json.List(Json.Get(op, "controls")))
                        {
                            if (!(c is Dictionary<string, object?> state))
                                continue;
                            w.Controls[Json.Str(Json.Get(state, "name"))] = state;
                            var view = w.View;
                            if (view != null)
                                Quietly(() => view.Apply(state));
                        }
                        break;
                    }
                case "show":
                    {
                        if (!windows.TryGetValue(key, out var w))
                            return;
                        w.Open = Json.Bool(Json.Get(op, "open"));
                        var view = w.View;
                        if (view != null)
                            Quietly(() => view.Activated = w.Open);
                        break;
                    }
                case "dispose":
                    {
                        if (windows.TryGetValue(key, out var w))
                        {
                            w.View?.Dispose();
                            windows.Remove(key);
                        }
                        break;
                    }
            }
        }

        /// <summary>
        /// Each frame: a view the player opened or closed from Decal's bar is told to the host, which
        /// opens or closes its window.
        /// </summary>
        public void Poll()
        {
            foreach (var kv in windows)
            {
                var w = kv.Value;
                if (w.View == null)
                    continue;
                var shown = w.View.Activated;
                if (shown == w.Open)
                    continue;
                w.Open = shown;
                send(kv.Key, "", shown ? "opened" : "closed", null);
            }
        }

        private void Build(string key, Window w, Rectangle? place)
        {
            var views = backend;
            if (views == null)
                return;
            var states = new List<Dictionary<string, object?>>(w.Controls.Values);
            Quietly(() =>
            {
                var view = views.Load(w.Xml, states, (control, ev, value) =>
                {
                    if (!applying)
                        send(key, control, ev, value);
                });
                w.View = view;
                if (place is Rectangle p)
                {
                    var size = view.Position;
                    view.Position = new Rectangle(p.X, p.Y, size.Width, size.Height);
                }
                view.Activated = w.Open;
            });
        }

        /// <summary>Runs <paramref name="set"/> with the events it raises (a box ticked, a page turned) kept from the host.</summary>
        private void Quietly(Action set)
        {
            applying = true;
            try
            {
                set();
            }
            finally
            {
                applying = false;
            }
        }
    }
}
