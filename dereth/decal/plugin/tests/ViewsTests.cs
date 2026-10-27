using System.Collections.Generic;
using System.Drawing;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Decal.Tests
{
    [TestClass]
    public sealed class ViewsTests
    {
        private readonly List<string> sent = new List<string>();
        private FakeViews backend = new FakeViews();
        private Views views = null!;

        [TestInitialize]
        public void Make()
        {
            sent.Clear();
            backend = new FakeViews();
            views = new Views((key, control, ev, value) => sent.Add(key + " " + control + " " + ev + " " + Json.Write(value)));
        }

        private static Dictionary<string, object?> Control(string name, string kind, params (string, object?)[] more)
        {
            var c = new Dictionary<string, object?> { ["name"] = name, ["kind"] = kind };
            foreach (var (k, v) in more)
                c[k] = v;
            return c;
        }

        private void Op(Dictionary<string, object?> op) => views.Apply((Dictionary<string, object?>)J.Wire(op)[0]!);

        private void View(string key, bool open, params Dictionary<string, object?>[] controls) =>
            Op(J.Op("view", ("key", key), ("open", open), ("xml", "<view title=\"" + key + "\"/>"), ("controls", new List<object?>(controls))));

        [TestMethod]
        public void AWindowIsMadeAsAViewWithItsControlsSetAndTheBoundOnesWired()
        {
            views.Attach(backend);
            View("p/main", true, Control("c_0", "button", ("text", "Go"), ("node", "go")), Control("c_1", "text", ("text", "hi")));
            Assert.AreEqual(1, backend.Made.Count);
            var v = backend.Made[0];
            Assert.AreEqual("<view title=\"p/main\"/>", v.Xml);
            Assert.IsTrue(v.Activated);
            Assert.AreEqual("hi", Json.Str(Json.Get(v.Controls["c_1"], "text")));
            CollectionAssert.AreEqual(new[] { "c_0" }, v.Wired);
        }

        [TestMethod]
        public void AControlsEventGoesToTheHostWithItsWindow()
        {
            views.Attach(backend);
            View("p/main", true, Control("c_0", "button", ("node", "go")));
            backend.Made[0].Player("c_0", "click", null);
            CollectionAssert.AreEqual(new[] { "p/main c_0 click null" }, sent);
        }

        [TestMethod]
        public void SettingAControlFromTheHostTellsTheHostNothing()
        {
            views.Attach(backend);
            View("p/main", true, Control("c_0", "toggle", ("checked", true), ("node", "auto")));
            Op(J.Op("update", ("key", "p/main"), ("controls", new List<object?> { Control("c_0", "toggle", ("checked", false), ("node", "auto")) })));
            Assert.AreEqual(0, sent.Count, string.Join("; ", sent));
            Assert.IsFalse(Json.Bool(Json.Get(backend.Made[0].Controls["c_0"], "checked")));
            // The player's own tick is told.
            backend.Made[0].Player("c_0", "change", true);
            CollectionAssert.AreEqual(new[] { "p/main c_0 change true" }, sent);
        }

        [TestMethod]
        public void AViewOpenedOrClosedFromDecalsBarIsToldOnce()
        {
            views.Attach(backend);
            View("p/main", false);
            var v = backend.Made[0];
            views.Poll();
            Assert.AreEqual(0, sent.Count);
            v.Activated = true;
            views.Poll();
            views.Poll();
            v.Activated = false;
            views.Poll();
            CollectionAssert.AreEqual(new[] { "p/main  opened null", "p/main  closed null" }, sent);
        }

        [TestMethod]
        public void AShowOpReachesTheViewAndIsNotToldBack()
        {
            views.Attach(backend);
            View("p/main", false);
            Op(J.Op("show", ("key", "p/main"), ("open", true)));
            Assert.IsTrue(backend.Made[0].Activated);
            views.Poll();
            Assert.AreEqual(0, sent.Count);
        }

        [TestMethod]
        public void AViewBuiltAgainStaysWhereThePlayerPutIt()
        {
            views.Attach(backend);
            View("p/main", true);
            backend.Made[0].Position = new Rectangle(300, 40, 200, 100);
            View("p/main", true, Control("c_0", "text", ("text", "more")));
            Assert.IsTrue(backend.Made[0].Disposed);
            Assert.AreEqual(new Point(300, 40), backend.Made[1].Position.Location);
        }

        [TestMethod]
        public void TheWindowsWaitForDecalsPluginAndOutliveIt()
        {
            View("p/main", true, Control("c_0", "text", ("text", "a")));
            Op(J.Op("update", ("key", "p/main"), ("controls", new List<object?> { Control("c_0", "text", ("text", "b")) })));
            Assert.AreEqual(0, backend.Made.Count);
            views.Attach(backend);
            Assert.AreEqual("b", Json.Str(Json.Get(backend.Made[0].Controls["c_0"], "text")));
            views.Detach();
            Assert.IsTrue(backend.Made[0].Disposed);
            CollectionAssert.AreEqual(new[] { "p/main" }, new List<string>(views.Keys));
        }

        [TestMethod]
        public void ADisposedWindowsViewGoes()
        {
            views.Attach(backend);
            View("p/main", true);
            Op(J.Op("dispose", ("key", "p/main")));
            Assert.IsTrue(backend.Made[0].Disposed);
            Assert.AreEqual(0, new List<string>(views.Keys).Count);
        }
    }
}
