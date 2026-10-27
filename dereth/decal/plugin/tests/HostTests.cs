using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Decal.Tests
{
    /// <summary>
    /// A recorded session through the host library, as Decal hands it over: every message the game
    /// received and sent, at its own time, and frames thirty times a second between them; Decal's
    /// calls, views and 3-D objects and the game's window are fakes, the window's key messages
    /// written down among Decal's calls.
    /// </summary>
    internal sealed class Session : IDisposable
    {
        public const double FrameRate = 30.0;

        public readonly FakeRetail Retail = new FakeRetail();
        public readonly FakeMarks Marks = new FakeMarks();
        public readonly FakeViews Views = new FakeViews();
        public readonly FakeKeyWindow Window;
        public readonly Driver Driver;

        /// <summary>The settings the player changed, as the plugin keeps them.</summary>
        public readonly List<(string, string)> Kept = new List<(string, string)>();

        /// <summary>Whether the game's window is in front, as each frame tells.</summary>
        public bool InFront;
        private readonly List<(bool received, double at, byte[] message)> messages;
        private int next;
        public double Now;

        public static string Workspace =>
            typeof(Session).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>().First(a => a.Key == "Workspace").Value ?? "";

        private static bool loaded;

        /// <summary>The host library, loaded from beside the tests.</summary>
        public static void Load()
        {
            if (loaded)
                return;
            var here = Path.GetDirectoryName(typeof(Session).Assembly.Location) ?? "";
            if (!File.Exists(Path.Combine(here, NativeHost.Library)))
                Assert.Fail("There is no " + NativeHost.Library + " beside the tests: build the dereth-decal-host crate for i686-pc-windows-msvc and name it with -p:HostLibrary.");
            NativeHost.LoadFrom(here);
            loaded = true;
        }

        /// <summary>
        /// The settings for a run: the test plugins named, nothing timed against the machine's speed,
        /// and the game's keys for plugins when <paramref name="keys"/>.
        /// </summary>
        public static string Settings(bool keys, string[] plugins) => Json.Write(new Dictionary<string, object?>
        {
            ["plugins"] = plugins.Select(p => (object?)Path.Combine(Workspace, "dereth", "decal", "host", "tests", "plugins", p)).ToList(),
            ["untimed"] = true,
            ["entry_budget_ms"] = 10000.0,
            ["keys"] = keys,
        });

        public Session(string session, params string[] plugins)
            : this(session, false, plugins)
        {
        }

        public Session(string session, bool keys, params string[] plugins)
        {
            Load();
            Window = new FakeKeyWindow(Retail.Calls);
            Driver = new Driver(NativeHost.Create(Settings(keys, plugins)), Retail, Marks, Window, (n, v) => Kept.Add((n, v)));
            messages = new List<(bool, double, byte[])>();
            var file = Path.Combine(Workspace, "fixtures", "message-corpus", session, "blobs.jsonl");
            foreach (var line in File.ReadLines(file))
            {
                var b = Json.Parse(line);
                var hex = Json.Str(Json.Get(b, "payload_hex"));
                var bytes = new byte[hex.Length / 2];
                for (var i = 0; i < bytes.Length; i++)
                    bytes[i] = byte.Parse(hex.Substring(2 * i, 2), NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                messages.Add((Json.Str(Json.Get(b, "dir")) == "s2c", Json.Num(Json.Get(b, "t_rel")) ?? 0, bytes));
            }
        }

        public void Frame() => Driver.Frame(Now, null, InFront, false);

        private void FramesUntil(double t)
        {
            while (Now + 1.0 / FrameRate <= t)
            {
                Now += 1.0 / FrameRate;
                Frame();
            }
        }

        private void Feed()
        {
            var (received, at, message) = messages[next++];
            FramesUntil(at);
            if (received)
            {
                Now = Math.Max(Now, at);
                Driver.Server(message, Now);
            }
            else
            {
                Driver.Client(message, Now);
            }
        }

        public string Phase => Json.Str(Json.Get(Driver.Host.Report(), "phase"));

        /// <summary>The recording up to the character's entry into the world, then a frame.</summary>
        public void Login()
        {
            while (next < messages.Count && Phase != "in_world")
                Feed();
            Assert.AreEqual("in_world", Phase, "the recording reaches the world");
            Frame();
        }

        /// <summary><paramref name="secs"/> more of the recording.</summary>
        public void Advance(double secs)
        {
            var until = Now + secs;
            while (next < messages.Count && messages[next].at <= until)
                Feed();
            FramesUntil(until);
        }

        /// <summary>The player types <paramref name="line"/>; two frames run. Whether a plugin claimed it.</summary>
        public bool Type(string line)
        {
            var claimed = Driver.Typed(line, Now);
            Frame();
            Frame();
            return claimed;
        }

        /// <summary>A plugin's console lines.</summary>
        public List<string> Console(string plugin) =>
            Json.List(Json.Get(Json.List(Json.Get(Driver.Host.Report(), "plugins"))
                    .FirstOrDefault(p => Json.Str(Json.Get(p, "id")) == plugin), "console"))
                .Select(Json.Str).ToList();

        public void Dispose() => Driver.Dispose();
    }

    [TestClass]
    public sealed class HostTests
    {
        [TestMethod]
        public void TheLibrarySpeaksThisPluginsInterfaceAndRefusesSettingsThatDoNotRead()
        {
            Session.Load();
            using (var h = NativeHost.Create("{}"))
                Assert.AreEqual(0, Json.List(Json.Get(h.Report(), "plugins")).Count, "a bare host runs no plugin");
            Assert.ThrowsException<InvalidOperationException>(() => NativeHost.Create("not json"));
        }

        [TestMethod]
        public void APluginsActionInARecordedSessionBecomesTheDecalCallThatSendsIt()
        {
            using (var s = new Session("long-solo-play", "actor"))
            {
                s.Login();
                s.Advance(5);
                s.Retail.Calls.Clear();
                Assert.IsTrue(s.Type("/act appraise"), "the actor claims its command");
                var item = s.Console("test.actor").Last(l => l.StartsWith("item\t", StringComparison.Ordinal)).Substring(5);
                var id = unchecked((int)uint.Parse(item, CultureInfo.InvariantCulture));
                CollectionAssert.Contains(s.Retail.Calls, "RequestId " + id.ToString(CultureInfo.InvariantCulture));
                s.Retail.Calls.Clear();
                s.Type("/act say");
                CollectionAssert.Contains(s.Retail.Calls, "InvokeChatParser hello from a plugin");
            }
        }

        [TestMethod]
        public void AnActionDecalHasNoCallForSendsNothingAndFailsAsUnavailable()
        {
            using (var s = new Session("long-solo-play", "actor"))
            {
                s.Login();
                s.Advance(5);
                s.Retail.Calls.Clear();
                s.Type("/act attack");
                CollectionAssert.AreEqual(new string[0], s.Retail.Calls);
                CollectionAssert.Contains(s.Console("test.actor"), "failed\tcombat.attack\tunavailable");
                Assert.IsFalse(s.Type("hello"), "a line no plugin claims goes to the game");
            }
        }

        /// <summary>The id the actor named its item by, as Decal takes it.</summary>
        private static int ItemOf(Session s) =>
            unchecked((int)uint.Parse(
                s.Console("test.actor").Last(l => l.StartsWith("item\t", StringComparison.Ordinal)).Substring(5),
                CultureInfo.InvariantCulture));

        [TestMethod]
        public void WithKeysOnAnAttackSelectsItsTargetThenPostsTheGamesAttackKeyAndLetsItGo()
        {
            using (var s = new Session("long-solo-play", true, "actor"))
            {
                s.InFront = true;
                s.Login();
                s.Advance(5);
                s.Retail.Calls.Clear();
                s.Type("/act strike");
                var select = s.Retail.Calls.IndexOf("SelectItem " + ItemOf(s).ToString(CultureInfo.InvariantCulture));
                var down = s.Retail.Calls.IndexOf("Post 0x100 0x22 0x01510001");
                Assert.IsTrue(select >= 0 && down > select, string.Join("; ", s.Retail.Calls));
                s.Advance(1);
                CollectionAssert.Contains(s.Retail.Calls, "Post 0x101 0x22 0xC1510001", "Page Down let go");
            }
        }

        [TestMethod]
        public void AKeyTheGamesWindowHearsFromTheHostIsNotThePlayersButTheirOwnStopsAMovement()
        {
            using (var s = new Session("long-solo-play", true, "actor"))
            {
                s.InFront = true;
                s.Login();
                s.Advance(5);
                s.Retail.Calls.Clear();
                s.Type("/act hold");
                CollectionAssert.Contains(s.Retail.Calls, "Post 0x100 0x57 0x00110001", "W held");
                s.Driver.KeyMessage(Keys.KeyDown, 0x57, 0x0011_0001, false);
                s.Frame();
                s.Frame();
                CollectionAssert.DoesNotContain(s.Retail.Calls, "Post 0x101 0x57 0xC0110001", "the key sent is not the player's");
                s.Driver.KeyMessage(Keys.KeyDown, 0x41, 0x001E_0001, false);
                s.Frame();
                s.Frame();
                CollectionAssert.Contains(s.Retail.Calls, "Post 0x101 0x57 0xC0110001", "the player's A lets go of W");
                CollectionAssert.Contains(s.Console("test.actor"), "failed\tnav.motion\tinterrupted");
            }
        }

        [TestMethod]
        public void WithTheGamesWindowBehindAnAttackPostsNothingAndFailsWithWhy()
        {
            using (var s = new Session("long-solo-play", true, "actor"))
            {
                s.Login();
                s.Advance(5);
                s.Retail.Calls.Clear();
                s.Type("/act strike");
                Assert.IsFalse(s.Retail.Calls.Any(c => c.StartsWith("Post", StringComparison.Ordinal)), string.Join("; ", s.Retail.Calls));
                CollectionAssert.Contains(s.Console("test.actor"), "failed\tcombat.attack\tunavailable (the game window is not in front)");
            }
        }

        [TestMethod]
        public void ThePluginPanelsSwitchForTheGamesKeysIsKeptInTheSettings()
        {
            using (var s = new Session("long-solo-play", "actor"))
            {
                s.InFront = true;
                s.Login();
                s.Advance(2);
                s.Driver.Views.Attach(s.Views);
                Assert.IsTrue(s.Type("/plugin"), "the host's own command");
                var panel = s.Views.Made.Last(v => v.Xml.Contains("title=\"Plugins\""));
                var name = panel.Controls.Single(c => Json.Str(Json.Get(c.Value, "node")) == "game_keys").Key;
                panel.Player(name, "change", true);
                s.Frame();
                s.Frame();
                CollectionAssert.AreEqual(new[] { ("keys", "on") }, s.Kept);
            }
        }

        [TestMethod]
        public void APluginsWindowIsADecalViewWhoseButtonReachesThePlugin()
        {
            using (var s = new Session("long-solo-play", "window"))
            {
                s.Login();
                s.Advance(2);
                // Decal's plugin starts once the character is in the world: the window's view is made then.
                s.Driver.Views.Attach(s.Views);
                var view = s.Views.Made.Single(v => v.Xml.Contains("title=\"Window\""));
                // Closed until the player opens it from Decal's bar, as every plugin window starts.
                Assert.IsFalse(view.Activated, "the window starts closed");
                view.Activated = true;
                s.Frame();
                s.Frame();
                CollectionAssert.Contains(s.Console("test.window"), "opened\tmain");
                Assert.IsTrue(view.Activated, "the host keeps it open");
                string Named(string node) => view.Controls.Single(c => Json.Str(Json.Get(c.Value, "node")) == node).Key;
                view.Player(Named("go"), "click", null);
                s.Frame();
                s.Frame();
                CollectionAssert.Contains(s.Console("test.window"), "clicked\tgo");
                var count = view.Controls.Values.Single(c => Json.Str(Json.Get(c, "text")).StartsWith("clicks", StringComparison.Ordinal));
                Assert.AreEqual("clicks 1", Json.Str(Json.Get(count, "text")));
                view.Player(Named("auto"), "change", true);
                s.Frame();
                CollectionAssert.Contains(s.Console("test.window"), "changed\tauto\ttrue");
                // Closed from Decal's bar: the plugin hears it.
                view.Activated = false;
                s.Frame();
                s.Frame();
                CollectionAssert.Contains(s.Console("test.window"), "closed\tmain");
            }
        }
    }
}
