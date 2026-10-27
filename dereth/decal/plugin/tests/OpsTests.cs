using System.Collections.Generic;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Decal.Tests
{
    [TestClass]
    public sealed class OpsTests
    {
        private FakeRetail retail = new FakeRetail();
        private ChatHides hides = new ChatHides();
        private Ops ops = null!;
        private readonly List<(string, string)> kept = new List<(string, string)>();

        [TestInitialize]
        public void Make()
        {
            retail = new FakeRetail();
            hides = new ChatHides();
            kept.Clear();
            ops = new Ops(
                retail, hides, new Views((k, c, e, v) => { }), new Marks(new FakeMarks()),
                new Keys(new FakeKeyWindow(retail.Calls)), (name, value) => kept.Add((name, value)));
        }

        private List<string> Run(params Dictionary<string, object?>[] o)
        {
            retail.Calls.Clear();
            ops.Apply(J.Wire(o), 0);
            return retail.Calls;
        }

        [TestMethod]
        public void EachItemOpIsTheDecalCallThatSendsItsRequest()
        {
            CollectionAssert.AreEqual(
                new[]
                {
                    "SelectItem 7", "UseItem 7", "ApplyItem 7 8", "RequestId 7", "MoveItem 7 9 2",
                    "AutoWield 7", "DropItem 7", "GiveItem 7 8", "CastSpell 1234 8", "SetCombatMode 8",
                },
                Run(
                    J.Op("select", ("id", 7)), J.Op("use", ("id", 7)), J.Op("apply", ("item", 7), ("target", 8)),
                    J.Op("appraise", ("id", 7)), J.Op("move", ("item", 7), ("container", 9), ("slot", 2)),
                    J.Op("wield", ("item", 7)), J.Op("drop", ("id", 7)), J.Op("give", ("item", 7), ("target", 8)),
                    J.Op("cast", ("spell", 1234), ("target", 8)), J.Op("stance", ("mode", 8))));
        }

        [TestMethod]
        public void AnObjectIdAboveTheSignedRangeKeepsItsBits()
        {
            // An item's id, as the server numbers them, is above 0x80000000.
            CollectionAssert.AreEqual(new[] { "UseItem " + unchecked((int)0x8000_0BB6u) }, Run(J.Op("use", ("id", 0x8000_0BB6u))));
        }

        [TestMethod]
        public void APurchaseFillsTheBuyListAfreshAndBuysItAll()
        {
            var items = new List<object?>
            {
                new Dictionary<string, object?> { ["id"] = 10.0, ["count"] = 5.0 },
                new Dictionary<string, object?> { ["id"] = 11.0, ["count"] = 1.0 },
            };
            CollectionAssert.AreEqual(
                new[] { "VendorClearBuyList", "VendorAddBuyList 10 5", "VendorAddBuyList 11 1", "VendorBuyAll" },
                Run(J.Op("vendor_buy", ("items", items))));
            CollectionAssert.AreEqual(
                new[] { "VendorClearSellList", "VendorAddSellList 12", "VendorAddSellList 13", "VendorSellAll" },
                Run(J.Op("vendor_sell", ("items", new List<object?> { 12.0, 13.0 }))));
        }

        [TestMethod]
        public void ASalvageUsesTheToolThenAddsTheItemsThenSalvagesAFrameApart()
        {
            CollectionAssert.AreEqual(
                new[] { "UseItem 3" },
                Run(J.Op("salvage", ("tool", 3), ("items", new List<object?> { 21.0, 22.0 }))));
            retail.Calls.Clear();
            ops.Frame();
            CollectionAssert.AreEqual(new[] { "SalvagePanelAdd 21", "SalvagePanelAdd 22" }, retail.Calls);
            retail.Calls.Clear();
            ops.Frame();
            CollectionAssert.AreEqual(new[] { "SalvagePanelSalvage" }, retail.Calls);
            Assert.AreEqual(0, ops.Waiting);
        }

        [TestMethod]
        public void TradeFellowshipTrainingAndMovementOps()
        {
            CollectionAssert.AreEqual(
                new[]
                {
                    "TradeAdd 4", "TradeAccept", "TradeDecline", "TradeReset", "TradeEnd",
                    "FellowshipRecruit 5", "FellowshipDismiss 5", "FellowshipGrantLeader 5", "FellowshipSetOpen True",
                    "FellowshipQuit", "FellowshipDisband",
                    "AddSkillExperience 34 1000", "AddAttributeExperience 1 50", "AddVitalExperience 5 70",
                    "FaceHeading 90.5", "SetAutorun True", "Logout",
                },
                Run(
                    J.Op("trade_add", ("item", 4)), J.Op("trade_accept"), J.Op("trade_decline"), J.Op("trade_reset"), J.Op("trade_end"),
                    J.Op("fellow_recruit", ("id", 5)), J.Op("fellow_dismiss", ("id", 5)), J.Op("fellow_leader", ("id", 5)),
                    J.Op("fellow_open", ("open", true)), J.Op("fellow_leave", ("disband", false)), J.Op("fellow_leave", ("disband", true)),
                    J.Op("train_skill", ("skill", 34), ("xp", 1000)), J.Op("train_attribute", ("attribute", 1), ("xp", 50)),
                    J.Op("train_vital", ("vital", 5), ("xp", 70)),
                    J.Op("face", ("heading", 90.5)), J.Op("autorun", ("on", true)), J.Op("logout")));
        }

        [TestMethod]
        public void ALinePrintedOrRunAndALineHidden()
        {
            CollectionAssert.AreEqual(
                new[] { "AddChatText [Plugins] hi 5", "InvokeChatParser /friends add Bex" },
                Run(J.Op("print", ("text", "[Plugins] hi"), ("color", 5)), J.Op("hide", ("text", "spam")), J.Op("chat", ("line", "/friends add Bex"))));
            Assert.IsTrue(hides.Take("spam\n", 1));
        }

        [TestMethod]
        public void AKeyOpIsTheKeysOwnMessageToTheGamesWindowAtItsPlace()
        {
            CollectionAssert.AreEqual(
                new[] { "Post 0x100 0x22 0x01510001", "Post 0x101 0x22 0xC1510001", "Post 0x100 0x57 0x00110001" },
                Run(
                    J.Op("key", ("vk", 0x22), ("scan", 0x51), ("extended", true), ("down", true)),
                    J.Op("key", ("vk", 0x22), ("scan", 0x51), ("extended", true), ("down", false)),
                    J.Op("key", ("vk", 0x57), ("scan", 0x11), ("extended", false), ("down", true))));
        }

        [TestMethod]
        public void ASettingOpIsKept()
        {
            Run(J.Op("setting", ("name", "keys"), ("value", "on")));
            CollectionAssert.AreEqual(new[] { ("keys", "on") }, kept);
        }

        [TestMethod]
        public void AFailingCallIsCountedAndTheOpsAfterItStillRun()
        {
            retail.Failing = "UseItem";
            CollectionAssert.AreEqual(new[] { "DropItem 2" }, Run(J.Op("use", ("id", 1)), J.Op("drop", ("id", 2)), J.Op("no such op")));
            Assert.AreEqual(1, ops.Failures);
            StringAssert.StartsWith(ops.LastFailure, "use: ");
        }
    }
}
