using System;
using System.Collections.Generic;
using System.IO;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Decal.Tests
{
    [TestClass]
    public sealed class MarksTests
    {
        [TestMethod]
        public void AMarkIsDrawnOnItsObjectOrAtItsPlaceInOpaqueColor()
        {
            var d3d = new FakeMarks();
            var marks = new Marks(d3d);
            marks.Apply((Dictionary<string, object?>)J.Wire(
                J.Op("mark", ("key", "p/1"), ("look", "text"), ("text", "Loot"), ("color", 0x00FF00), ("scale", 1.0), ("object", 0x8000_0001u)))[0]!);
            marks.Apply((Dictionary<string, object?>)J.Wire(
                J.Op("mark", ("key", "p/2"), ("look", "ring"), ("text", ""), ("color", 0xFF0000), ("scale", 3.0), ("ns", 12.5), ("ew", -40.25), ("z", 6.0)))[0]!);
            var a = d3d.Drawn[0];
            Assert.AreEqual((MarkLook.Text, (int?)unchecked((int)0x8000_0001u), "Loot", unchecked((int)0xFF00FF00u)), (a.Look, a.Object, a.Text, a.Color));
            var b = d3d.Drawn[1];
            Assert.AreEqual((MarkLook.Ring, (int?)null, 12.5, -40.25, 6.0, 3.0), (b.Look, b.Object, b.Ns, b.Ew, b.Z, b.Scale));
        }

        [TestMethod]
        public void AMarkSentAgainReplacesItsOldOneAndAnUnmarkTakesItAway()
        {
            var d3d = new FakeMarks();
            var marks = new Marks(d3d);
            Dictionary<string, object?> Mark(string look) => (Dictionary<string, object?>)J.Wire(
                J.Op("mark", ("key", "arrow"), ("look", look), ("object", 5)))[0]!;
            marks.Apply(Mark("pointer"));
            marks.Apply(Mark("pointer"));
            Assert.AreEqual((2, 1), (d3d.Drawn.Count, d3d.Gone));
            marks.Apply((Dictionary<string, object?>)J.Wire(J.Op("unmark", ("key", "arrow")))[0]!);
            Assert.AreEqual(2, d3d.Gone);
            marks.Apply(Mark("no such look"));
            Assert.AreEqual(2, d3d.Drawn.Count);
            marks.Apply(Mark("arrow"));
            marks.Clear();
            Assert.AreEqual((3, 3), (d3d.Drawn.Count, d3d.Gone));
        }
    }

    [TestClass]
    public sealed class ChatHidesTests
    {
        [TestMethod]
        public void ALineIsHiddenOnceWhateverItsLineEnding()
        {
            var h = new ChatHides();
            h.Add("You say, \"hi\"", 0);
            Assert.IsFalse(h.Take("You say, \"ho\"\n", 0.1));
            Assert.IsTrue(h.Take("You say, \"hi\"\n", 0.1));
            Assert.IsFalse(h.Take("You say, \"hi\"\n", 0.2));
        }

        [TestMethod]
        public void ALineTheWindowNeverShowsIsLetGo()
        {
            var h = new ChatHides();
            h.Add("gone", 0);
            Assert.IsFalse(h.Take("gone", ChatHides.Keep + 0.5));
            Assert.AreEqual(0, h.Count);
        }
    }

    [TestClass]
    public sealed class KeysTests
    {
        [TestMethod]
        public void AKeyMessagesFlagsCarryItsPlaceAndWhetherItGoesDownOrUp()
        {
            Assert.AreEqual(0x0151_0001, Keys.Flags(0x51, true, true), "Page Down, down");
            Assert.AreEqual(unchecked((int)0xC151_0001u), Keys.Flags(0x51, true, false), "Page Down, up");
            Assert.AreEqual(0x0039_0001, Keys.Flags(0x39, false, true), "Space, down");
        }

        [TestMethod]
        public void AKeySentHereIsHeardBackOnceAndTheRestAreThePlayers()
        {
            var log = new List<string>();
            var keys = new Keys(new FakeKeyWindow(log));
            keys.Press(0x57, 0x11, false, true);
            CollectionAssert.AreEqual(new[] { "Post 0x100 0x57 0x00110001" }, log);
            Assert.IsTrue(keys.Echo(Keys.KeyDown, 0x57, 0x0011_0001), "the key sent");
            Assert.IsFalse(keys.Echo(Keys.KeyDown, 0x57, 0x0011_0001), "the same again is the player's");
            Assert.IsFalse(keys.Echo(Keys.KeyDown, 0x41, 0x001E_0001), "another key is the player's");
        }
    }

    [TestClass]
    public sealed class SettingsTests
    {
        private static string Folder()
        {
            var d = Path.Combine(Path.GetTempPath(), "dereth-decal-tests-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(d);
            return d;
        }

        [TestMethod]
        public void TheHostFindsItsPluginsDataGameAndPeersInTheirFolders()
        {
            var installed = Folder();
            Directory.CreateDirectory(Path.Combine(installed, "plugins"));
            var notes = new List<string>();
            var s = Json.Parse(Settings.HostJson(installed, @"C:\Users\p\AppData\Roaming", @"C:\Games\AC", new string[0], notes));
            CollectionAssert.AreEqual(
                new object?[] { @"C:\Users\p\AppData\Roaming\Dereth\client\plugins" }, Json.List(Json.Get(s, "found")),
                "the Dereth client's own, found and off until ticked");
            Assert.IsNull(Json.Get(s, "plugins"), "none named to load");
            Assert.AreEqual(@"C:\Users\p\AppData\Roaming\Dereth\decal", Json.Str(Json.Get(s, "data")));
            Assert.AreEqual(Path.Combine(installed, "plugins"), Json.Str(Json.Get(s, "bundled")));
            Assert.AreEqual(@"C:\Games\AC", Json.Str(Json.Get(s, "dats")));
            Assert.AreEqual(@"C:\Users\p\AppData\Roaming\Dereth\client\plugin-peers", Json.Str(Json.Get(s, "peers")));
            Assert.AreEqual(true, Json.Get(s, "launcher"), "and the launcher's");
            Assert.AreEqual(false, Json.Get(s, "keys"), "the game's keys only when the player says");
            Assert.AreEqual(0, notes.Count);
            Directory.Delete(installed, true);
        }

        [TestMethod]
        public void TheSettingsFileNamesThePluginsFoldersTurnsThePeersOffGivesThePolicyAndLetsPluginsUseKeys()
        {
            var dir = Folder();
            var policy = Path.Combine(dir, "policy.json");
            File.WriteAllText(policy, "{\"automation\":{}}");
            var notes = new List<string>();
            var s = Json.Parse(Settings.HostJson(dir, @"C:\a", null, new[]
            {
                "# mine", "plugins = D:\\more", "plugins = E:\\most", "peers = off", "policy = " + policy, "keys = on",
                "color = red", "nonsense", "keys = sometimes",
            }, notes));
            CollectionAssert.AreEqual(new object?[] { @"D:\more", @"E:\most" }, Json.List(Json.Get(s, "found")), "in place of the client's");
            Assert.AreEqual(false, Json.Get(s, "launcher"), "and of the launcher's");
            Assert.IsNull(Json.Get(s, "peers"));
            Assert.IsNull(Json.Get(s, "bundled"));
            Assert.IsNull(Json.Get(s, "dats"));
            Assert.AreEqual("{\"automation\":{}}", Json.Str(Json.Get(s, "policy")));
            Assert.AreEqual(true, Json.Get(s, "keys"));
            Assert.AreEqual(3, notes.Count, string.Join("; ", notes));
            Directory.Delete(dir, true);
        }

        [TestMethod]
        public void ASettingIsWrittenOverItsLineOrAddedAndTheRestKept()
        {
            CollectionAssert.AreEqual(
                new[] { "# mine", "keys = on", "peers = off" },
                Settings.With(new[] { "# mine", "keys = off", "peers = off", " keys=off" }, "keys", "on"));
            CollectionAssert.AreEqual(
                new[] { "# keys = off", "keys = on" },
                Settings.With(new[] { "# keys = off" }, "keys", "on"));
            var dir = Folder();
            var file = Path.Combine(dir, "decal", Settings.File);
            Settings.Write(file, "keys", "on");
            Settings.Write(file, "keys", "off");
            Assert.AreEqual("keys = off\n", File.ReadAllText(file));
            Directory.Delete(dir, true);
        }

        [TestMethod]
        public void APolicyFileThatDoesNotReadIsSaid()
        {
            var dir = Folder();
            var notes = new List<string>();
            var s = Json.Parse(Settings.HostJson(dir, @"C:\a", null, new[] { "policy = " + Path.Combine(dir, "no-such-file.json") }, notes));
            Assert.IsNull(Json.Get(s, "policy"));
            StringAssert.StartsWith(notes[0], "The world's plugin policy did not read");
            Directory.Delete(dir, true);
        }
    }

    [TestClass]
    public sealed class JsonTests
    {
        [TestMethod]
        public void WhatIsWrittenReadsBackTheSame()
        {
            var v = new Dictionary<string, object?>
            {
                ["text"] = "a \"quote\", a \\, a line\nand a tab\t\u0001",
                ["n"] = 1.5,
                ["list"] = new List<object?> { true, false, null, 3.0 },
            };
            var back = Json.Parse(Json.Write(v));
            Assert.AreEqual(v["text"], Json.Get(back, "text"));
            Assert.AreEqual(1.5, Json.Get(back, "n"));
            CollectionAssert.AreEqual(new object?[] { true, false, null, 3.0 }, Json.List(Json.Get(back, "list")));
        }

        [TestMethod]
        public void AnIdIsTheSigned32BitsDecalTakes()
        {
            Assert.AreEqual(unchecked((int)0xFFFF_FFFFu), Json.Id(4294967295.0));
            Assert.AreEqual(5, Json.Id(5.0));
            Assert.IsNull(Json.Id(-1.0));
            Assert.IsNull(Json.Id(4294967296.0));
            Assert.IsNull(Json.Id(1.5));
        }

        [TestMethod]
        public void TextThatIsNotJsonDoesNotRead()
        {
            Assert.ThrowsException<FormatException>(() => Json.Parse("{\"a\":"));
            Assert.ThrowsException<FormatException>(() => Json.Parse("[1] 2"));
        }
    }
}
