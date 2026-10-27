using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;

namespace Dereth.Decal.Tests
{
    /// <summary>Decal's calls, each written down as its name and arguments.</summary>
    internal sealed class FakeRetail : IRetail
    {
        public readonly List<string> Calls = new List<string>();

        /// <summary>A call that throws, as a Decal call can.</summary>
        public string? Failing;

        private void Call(string name, params object[] args)
        {
            if (name == Failing)
                throw new InvalidOperationException(name + " failed");
            var parts = new List<string> { name };
            foreach (var a in args)
                parts.Add(Convert.ToString(a, CultureInfo.InvariantCulture) ?? "");
            Calls.Add(string.Join(" ", parts));
        }

        public void AddChatText(string text, int color) => Call("AddChatText", text, color);
        public void InvokeChatParser(string line) => Call("InvokeChatParser", line);
        public void SelectItem(int id) => Call("SelectItem", id);
        public void UseItem(int id) => Call("UseItem", id);
        public void ApplyItem(int item, int target) => Call("ApplyItem", item, target);
        public void RequestId(int id) => Call("RequestId", id);
        public void MoveItem(int item, int container, int slot) => Call("MoveItem", item, container, slot);
        public void AutoWield(int item) => Call("AutoWield", item);
        public void DropItem(int id) => Call("DropItem", id);
        public void GiveItem(int item, int target) => Call("GiveItem", item, target);
        public void CastSpell(int spell, int target) => Call("CastSpell", spell, target);
        public void SetCombatMode(int mode) => Call("SetCombatMode", mode);
        public void VendorClearBuyList() => Call("VendorClearBuyList");
        public void VendorAddBuyList(int id, int count) => Call("VendorAddBuyList", id, count);
        public void VendorBuyAll() => Call("VendorBuyAll");
        public void VendorClearSellList() => Call("VendorClearSellList");
        public void VendorAddSellList(int id) => Call("VendorAddSellList", id);
        public void VendorSellAll() => Call("VendorSellAll");
        public void TradeAdd(int id) => Call("TradeAdd", id);
        public void TradeAccept() => Call("TradeAccept");
        public void TradeDecline() => Call("TradeDecline");
        public void TradeReset() => Call("TradeReset");
        public void TradeEnd() => Call("TradeEnd");
        public void FellowshipRecruit(int id) => Call("FellowshipRecruit", id);
        public void FellowshipDismiss(int id) => Call("FellowshipDismiss", id);
        public void FellowshipGrantLeader(int id) => Call("FellowshipGrantLeader", id);
        public void FellowshipSetOpen(bool open) => Call("FellowshipSetOpen", open);
        public void FellowshipQuit() => Call("FellowshipQuit");
        public void FellowshipDisband() => Call("FellowshipDisband");
        public void SalvagePanelAdd(int id) => Call("SalvagePanelAdd", id);
        public void SalvagePanelSalvage() => Call("SalvagePanelSalvage");
        public void AddSkillExperience(int skill, int xp) => Call("AddSkillExperience", skill, xp);
        public void AddAttributeExperience(int attribute, int xp) => Call("AddAttributeExperience", attribute, xp);
        public void AddVitalExperience(int vital, int xp) => Call("AddVitalExperience", vital, xp);
        public void FaceHeading(double heading) => Call("FaceHeading", heading);
        public void SetAutorun(bool on) => Call("SetAutorun", on);
        public void Logout() => Call("Logout");
    }

    /// <summary>The game's window: each key message posted to it, written down in <see cref="Log"/>.</summary>
    internal sealed class FakeKeyWindow : IKeyWindow
    {
        public readonly List<string> Log;

        public FakeKeyWindow(List<string> log) => Log = log;

        public void Post(int message, int wParam, int lParam) =>
            Log.Add(string.Format(CultureInfo.InvariantCulture, "Post 0x{0:X3} 0x{1:X2} 0x{2:X8}", message, wParam, lParam));
    }

    /// <summary>A Decal view: its XML, its controls' states as last set, and the events wired to it.</summary>
    internal sealed class FakeView : IView
    {
        public readonly string Xml;
        public readonly Dictionary<string, Dictionary<string, object?>> Controls =
            new Dictionary<string, Dictionary<string, object?>>(StringComparer.Ordinal);
        public readonly List<string> Wired = new List<string>();
        public ControlEvent Events;
        public bool Disposed;
        public int Applied;

        public FakeView(string xml, ControlEvent events)
        {
            Xml = xml;
            Events = events;
        }

        public bool Activated { get; set; }

        public Rectangle Position { get; set; } = new Rectangle(0, 0, 200, 100);

        /// <summary>
        /// A control's state set; a Decal control raises its own change event when its value is set,
        /// as a check box does when ticked.
        /// </summary>
        public void Apply(Dictionary<string, object?> state)
        {
            var name = Json.Str(Json.Get(state, "name"));
            Controls[name] = state;
            Applied++;
            if (Json.Str(Json.Get(state, "kind")) == "toggle")
                Events(name, "change", Json.Bool(Json.Get(state, "checked")));
        }

        public void Dispose() => Disposed = true;

        /// <summary>The player clicks, ticks, picks or types on control <paramref name="name"/>.</summary>
        public void Player(string name, string ev, object? value) => Events(name, ev, value);
    }

    internal sealed class FakeViews : IViewBackend
    {
        public readonly List<FakeView> Made = new List<FakeView>();

        public IView Load(string xml, IReadOnlyList<Dictionary<string, object?>> controls, ControlEvent events)
        {
            var v = new FakeView(xml, events);
            foreach (var c in controls)
            {
                v.Apply(c);
                if (Json.Get(c, "node") != null)
                    v.Wired.Add(Json.Str(Json.Get(c, "name")));
            }
            Made.Add(v);
            return v;
        }
    }

    /// <summary>Decal's 3-D service: what is drawn, and what was taken away.</summary>
    internal sealed class FakeMarks : IMarkBackend
    {
        public readonly List<Mark> Drawn = new List<Mark>();
        public int Gone;

        public IDisposable Draw(Mark mark)
        {
            Drawn.Add(mark);
            return new Undraw(this);
        }

        private sealed class Undraw : IDisposable
        {
            private readonly FakeMarks marks;
            public Undraw(FakeMarks marks) => this.marks = marks;
            public void Dispose() => marks.Gone++;
        }
    }

    /// <summary>JSON values to write ops with.</summary>
    internal static class J
    {
        public static Dictionary<string, object?> Op(string op, params (string, object?)[] fields)
        {
            var d = new Dictionary<string, object?>(StringComparer.Ordinal) { ["op"] = op };
            foreach (var (k, v) in fields)
                d[k] = v is int i ? (double)i : v is uint u ? (double)u : v;
            return d;
        }

        /// <summary>An op as it crosses the interface: written and read back.</summary>
        public static List<object?> Wire(params Dictionary<string, object?>[] ops) =>
            Json.List(Json.Parse(Json.Write(ops)));
    }
}
