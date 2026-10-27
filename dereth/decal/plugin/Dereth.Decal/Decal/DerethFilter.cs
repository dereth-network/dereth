using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using Decal.Adapter;

namespace Dereth.Decal.Plugin
{
    /// <summary>
    /// The plugin host's half that runs from the game's start: Decal's network filter, so the host
    /// hears every message from the character list on, and everything else Decal hands an
    /// extension (the frames, the lines typed and shown, the keys, the selection), and the game's
    /// window the game's keys go to. The views and the player's key bindings wait for
    /// <see cref="DerethPlugin"/>, which Decal starts once a character is in the world.
    /// </summary>
    [FriendlyName("Dereth Plugins")]
    public sealed class DerethFilter : FilterBase
    {
        private readonly Stopwatch clock = Stopwatch.StartNew();
        private Driver? driver;
        private DecalKeys? window;
        private readonly List<string> notes = new List<string>();

        /// <summary>The host's driver, while it runs.</summary>
        internal Driver? Driver => driver;

        /// <summary>What the player is to be told once a character is in the world: settings that did not read, a host that did not start.</summary>
        internal List<string> Notes => notes;

        private double Now => clock.Elapsed.TotalSeconds;

        protected override void Startup()
        {
            try
            {
                var installed = System.IO.Path.GetDirectoryName(typeof(DerethFilter).Assembly.Location) ?? "";
                NativeHost.LoadFrom(installed);
                var appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
                var file = System.IO.Path.Combine(Settings.DataFolder(appData), Settings.File);
                var lines = File.Exists(file) ? File.ReadAllLines(file) : new string[0];
                var json = Settings.HostJson(installed, appData, GameFolder(), lines, notes);
                MakeClientPlugins(appData);
                window = new DecalKeys(Core);
                driver = new Driver(
                    NativeHost.Create(json), new DecalRetail(Core.Actions), new DecalMarks(Core.D3DService), window,
                    (name, value) => KeepSetting(file, name, value));
                ServerDispatch += OnServer;
                ClientDispatch += OnClient;
                Core.RenderFrame += OnFrame;
                Core.ChatBoxMessage += OnChatLine;
                Core.CommandLineText += OnTyped;
                Core.WindowMessage += OnWindowMessage;
                Core.ItemSelected += OnSelected;
            }
            catch (Exception e)
            {
                notes.Add("The plugin host did not start: " + e.Message);
                driver?.Dispose();
                driver = null;
            }
        }

        protected override void Shutdown()
        {
            ServerDispatch -= OnServer;
            ClientDispatch -= OnClient;
            Core.RenderFrame -= OnFrame;
            Core.ChatBoxMessage -= OnChatLine;
            Core.CommandLineText -= OnTyped;
            Core.WindowMessage -= OnWindowMessage;
            Core.ItemSelected -= OnSelected;
            driver?.Dispose();
            driver = null;
        }

        /// <summary>The Dereth client's plugins folder, made when there is none, for the player to put plugins in.</summary>
        private static void MakeClientPlugins(string appData)
        {
            try
            {
                Directory.CreateDirectory(Settings.ClientPlugins(appData));
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException)
            {
            }
        }

        /// <summary>A setting the player changed, kept in the settings file; one that cannot be kept is said.</summary>
        private void KeepSetting(string file, string name, string value)
        {
            try
            {
                Settings.Write(file, name, value);
            }
            catch (Exception e) when (e is IOException || e is UnauthorizedAccessException)
            {
                Core.Actions.AddChatText("[Plugins] " + Settings.File + " could not be written: " + e.Message, 6);
            }
        }

        /// <summary>The game's own folder, which holds its data files.</summary>
        private static string? GameFolder()
        {
            try
            {
                return System.IO.Path.GetDirectoryName(Process.GetCurrentProcess().MainModule?.FileName ?? "");
            }
            catch (Exception)
            {
                return null;
            }
        }

        /// <summary>Runs <paramref name="step"/>; a fault in it stops the host, and the player is told.</summary>
        private void Guard(Action<Driver> step)
        {
            var d = driver;
            if (d == null)
                return;
            try
            {
                step(d);
            }
            catch (Exception e)
            {
                driver = null;
                try
                {
                    d.Dispose();
                    Core.Actions.AddChatText("[Plugins] The plugins stopped: " + e.Message, 6);
                }
                catch (Exception)
                {
                }
            }
        }

        private void OnServer(object sender, NetworkMessageEventArgs e) =>
            Guard(d => d.Server(e.Message.RawData, Now));

        private void OnClient(object sender, NetworkMessageEventArgs e) =>
            Guard(d => d.Client(e.Message.RawData, Now));

        private void OnFrame(object sender, EventArgs e) =>
            Guard(d => d.Frame(Now, CurrentPlace(), window?.InFront ?? false, Core.Actions.ChatState));

        private void OnChatLine(object sender, ChatTextInterceptEventArgs e) =>
            Guard(d =>
            {
                if (d.ChatLine(e.Text, Now))
                    e.Eat = true;
            });

        private void OnTyped(object sender, ChatParserInterceptEventArgs e) =>
            Guard(d =>
            {
                if (d.Typed(e.Text, Now))
                    e.Eat = true;
            });

        private void OnWindowMessage(object sender, WindowMessageEventArgs e)
        {
            if (e.Msg != Keys.KeyDown && e.Msg != Keys.KeyUp)
                return;
            Guard(d => d.KeyMessage(e.Msg, e.WParam, e.LParam, Core.Actions.ChatState));
        }

        private void OnSelected(object sender, ItemSelectedEventArgs e) => Guard(d => d.Selected(e.ItemGuid));

        /// <summary>The character's place as Decal reads it, when the game has one.</summary>
        private Place? CurrentPlace()
        {
            try
            {
                var a = Core.Actions;
                var cell = a.Landcell;
                if (cell == 0)
                    return null;
                return new Place
                {
                    Landcell = unchecked((uint)cell),
                    X = a.LocationX,
                    Y = a.LocationY,
                    Z = a.LocationZ,
                    Heading = a.Heading,
                };
            }
            catch (Exception)
            {
                return null;
            }
        }
    }
}
