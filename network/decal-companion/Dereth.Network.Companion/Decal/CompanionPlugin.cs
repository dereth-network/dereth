using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using Decal.Adapter;
using Decal.Adapter.Wrappers;

namespace Dereth.Network.Companion.Decal
{
    /// <summary>
    /// The companion as a Decal plugin: its window in Decal's bar, the commands typed through the
    /// game's own chat parser, and the world's answers read from the chat window. It reads nothing
    /// of the game's memory and sends nothing the player could not type.
    /// </summary>
    [FriendlyName("Dereth Network")]
    public sealed class CompanionPlugin : PluginBase
    {
        private const string ViewResource = "Dereth.Network.Companion.companion.xml";
        private readonly Stopwatch clock = Stopwatch.StartNew();
        private ViewWrapper? view;
        private DecalView? surface;
        private WebHttp? http;
        private Companion? companion;
        private bool wasOpen;

        protected override void Startup()
        {
            try
            {
                http = new WebHttp("DerethNetworkCompanion/" + Assembly.GetExecutingAssembly().GetName().Version);
                var hub = new HubClient(HubAddress(), http);
                view = Host.LoadView(ReadView());
                surface = new DecalView(view);
                companion = new Companion(surface, new DecalGame(Host.Actions), hub, () => clock.Elapsed.TotalSeconds);
                surface.Wire(companion);
                Core.ChatBoxMessage += OnChatBoxMessage;
                Core.RenderFrame += OnRenderFrame;
            }
            catch (Exception e)
            {
                Report(e);
            }
        }

        protected override void Shutdown()
        {
            try
            {
                Core.ChatBoxMessage -= OnChatBoxMessage;
                Core.RenderFrame -= OnRenderFrame;
                view?.Dispose();
            }
            catch (Exception e)
            {
                Report(e);
            }
            view = null;
            companion = null;
        }

        private void OnChatBoxMessage(object sender, ChatTextInterceptEventArgs e)
        {
            try
            {
                if (companion != null && companion.OnChat(e.Text))
                    e.Eat = true;
            }
            catch (Exception ex)
            {
                Report(ex);
            }
        }

        private void OnRenderFrame(object sender, EventArgs e)
        {
            try
            {
                http?.Pump();
                if (companion == null || view == null || surface == null)
                    return;
                var open = view.Activated;
                if (open && !wasOpen)
                    companion.OnPage(surface.ActivePage);
                else if (!open && wasOpen)
                    companion.OnClosed();
                wasOpen = open;
                companion.Tick();
            }
            catch (Exception ex)
            {
                Report(ex);
            }
        }

        /// <summary>The hub to ask: <c>hub = https://…</c> in <c>settings.txt</c> beside the plugin, else the network's own.</summary>
        private string HubAddress()
        {
            try
            {
                var file = System.IO.Path.Combine(System.IO.Path.GetDirectoryName(typeof(CompanionPlugin).Assembly.Location) ?? "", "settings.txt");
                if (File.Exists(file))
                    foreach (var line in File.ReadAllLines(file))
                    {
                        var eq = line.IndexOf('=');
                        if (eq > 0 && line.Substring(0, eq).Trim() == "hub")
                        {
                            var url = line.Substring(eq + 1).Trim();
                            if (HubClient.IsAllowed(url))
                                return url;
                        }
                    }
            }
            catch (IOException)
            {
            }
            return HubClient.DefaultHub;
        }

        private static string ReadView()
        {
            using (var s = typeof(CompanionPlugin).Assembly.GetManifestResourceStream(ViewResource))
            using (var r = new StreamReader(s ?? throw new InvalidOperationException("the view is missing from the plugin")))
                return r.ReadToEnd();
        }

        private void Report(Exception e)
        {
            try
            {
                Host.Actions.AddChatText("[Dereth Network] " + e.Message, 6, 1);
            }
            catch (Exception)
            {
                // Nothing more can be done from inside the game.
            }
        }
    }

    /// <summary>The chat bar, through the game's own chat parser.</summary>
    internal sealed class DecalGame : IGame
    {
        private readonly HooksWrapper actions;

        public DecalGame(HooksWrapper actions) => this.actions = actions;

        public void Type(string line) => actions.InvokeChatParser(line);
    }

    /// <summary>The companion's window as Decal draws it: its controls, found by name.</summary>
    internal sealed class DecalView : ICompanionView
    {
        /// <summary>The characters of a note shown in a list, a line.</summary>
        private const int NoteWidth = 58;

        private static readonly string[] Buttons =
        {
            Controls.NetOn, Controls.NetOff, Controls.StatusRefresh, Controls.Link, Controls.Unlink,
            Controls.FriendAdd, Controls.FriendRemove, Controls.FriendsRefresh,
            Controls.ChannelCreate, Controls.ChannelJoin, Controls.ChannelLeave, Controls.ChannelsRefresh,
            Controls.ChannelInvite, Controls.InviteJoin, Controls.GlobalSay, Controls.Search, Controls.WorldsRefresh,
        };

        private static readonly string[] SelectableLists = { Controls.Friends, Controls.Channels, Controls.Invites };

        private readonly ViewWrapper view;
        private readonly Dictionary<string, string> shown = new Dictionary<string, string>(StringComparer.Ordinal);

        public DecalView(ViewWrapper view) => this.view = view;

        /// <summary>The page the window shows.</summary>
        public int ActivePage => view.Controls["tabs"] is NotebookWrapper n ? n.ActiveTab : 0;

        /// <summary>Hand the window's clicks to the companion.</summary>
        public void Wire(Companion companion)
        {
            foreach (var name in Buttons)
                if (view.Controls[name] is PushButtonWrapper b)
                {
                    var n = name;
                    b.Hit += (s, e) => Safe(() => companion.OnClick(n));
                }
            foreach (var name in SelectableLists)
                if (view.Controls[name] is ListWrapper l)
                {
                    var n = name;
                    l.Selected += (s, e) => Safe(() => companion.OnSelect(n, e.Row));
                }
            if (view.Controls["tabs"] is NotebookWrapper tabs)
                tabs.Change += (s, e) => Safe(() => companion.OnPage(e.Index));
        }

        public void SetText(string control, string text)
        {
            if (shown.TryGetValue(control, out var was) && was == text)
                return;
            shown[control] = text;
            switch (view.Controls[control])
            {
                case StaticWrapper s:
                    s.Text = text;
                    break;
                case TextBoxWrapper t:
                    t.Text = text;
                    break;
                case PushButtonWrapper b:
                    b.Text = text;
                    break;
                case ListWrapper l:
                    l.Clear();
                    foreach (var line in ChatText.Wrap(text, NoteWidth))
                        l.Add()[0][0] = line;
                    break;
            }
        }

        public string GetText(string control) => view.Controls[control] is TextBoxWrapper t ? t.Text ?? "" : "";

        public void SetRows(string list, IList<string[]> rows)
        {
            var key = list + "\u0001" + Fingerprint(rows);
            if (shown.TryGetValue(list, out var was) && was == key)
                return;
            shown[list] = key;
            if (!(view.Controls[list] is ListWrapper l))
                return;
            l.Clear();
            foreach (var cells in rows)
            {
                var row = l.Add();
                for (var c = 0; c < cells.Length && c < l.ColCount; c++)
                    row[c][0] = cells[c];
            }
        }

        private static string Fingerprint(IList<string[]> rows)
        {
            var parts = new List<string>(rows.Count);
            foreach (var r in rows)
                parts.Add(string.Join("\u0002", r));
            return string.Join("\u0003", parts.ToArray());
        }

        private static void Safe(Action a)
        {
            try
            {
                a();
            }
            catch (Exception)
            {
                // A fault in one click must not reach the game.
            }
        }
    }
}
