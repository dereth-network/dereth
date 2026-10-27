using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Network.Companion.Tests
{
    /// <summary>A world's plugin policy document, and the chat text helpers.</summary>
    [TestClass]
    public sealed class PolicyTests
    {
        [TestMethod]
        public void A_policy_reads_its_rules_and_notes()
        {
            var p = PluginPolicy.Parse(
                "policy_version = 1\nworld = \"Frost\"\n\n[automation]   # rules\ncombat = \"deny\"\nnav = 'interactive-only'\nloot = \"allow\"\n" +
                "default = \"interactive-only\"\n[limits]\npeers_local = 3\n[notes]\ntext = \"No \\\"bots\\\".\"\nurl = \"https://frost.example/rules\"\n",
                out var why);
            Assert.IsNotNull(p, why);
            Assert.AreEqual("Frost", p!.World);
            Assert.AreEqual("deny", p.Automation["combat"]);
            Assert.AreEqual("No \"bots\".", p.Notes);
            Assert.AreEqual("https://frost.example/rules", p.NotesUrl);
            Assert.AreEqual("No combat automation. On a click only: nav. Anything else: on a click only.", p.Summary());
        }

        [TestMethod]
        public void A_policy_that_does_not_read_is_refused_with_the_reason()
        {
            Assert.IsNull(PluginPolicy.Parse("[automation]\ncombat = \"deny\"\n", out var why));
            Assert.AreEqual("it is not a version 1 policy", why);
            Assert.IsNull(PluginPolicy.Parse("policy_version = 1\n[automation]\ncombat = \"never\"\n", out why));
            StringAssert.Contains(why, "combat is never");
            Assert.AreEqual("Every kind of automation is allowed.", PluginPolicy.Parse("policy_version = 1\n", out _)!.Summary());
        }

        [TestMethod]
        public void Long_notes_wrap_at_spaces()
        {
            CollectionAssert.AreEqual(new[] { "one two", "three", "four" }, ChatText.Wrap("one two three four", 8));
            CollectionAssert.AreEqual(new[] { "abcdefgh", "ij" }, ChatText.Wrap("abcdefghij", 8));
        }

        [TestMethod]
        public void Commands_check_what_they_are_given()
        {
            Assert.AreEqual("@dn friends add Bob@coldeve", Commands.AddFriend("Bob@coldeve"));
            Assert.IsNull(Commands.AddFriend("Bob"));
            Assert.IsNull(Commands.AddFriend("oldguard@channel"), "a channel is no friend");
            Assert.IsNull(Commands.AddFriend("Bob@Cold Eve"));
            Assert.AreEqual("@dn channel join oldguard", Commands.JoinChannel("OldGuard@channel"));
            Assert.IsNull(Commands.JoinChannel("a"));
            Assert.IsNull(Commands.SayGlobal("  "));
            Assert.AreEqual("@dng one two", Commands.SayGlobal("one\ntwo"));
            Assert.IsNull(Commands.SayGlobal(new string('a', 400)), "past the longest line");
        }

        [TestMethod]
        public void Json_reads_the_hubs_answers()
        {
            var v = Json.Parse("{\"a\":[1,2.5,\"x\\u0041\\n\",true,null],\"b\":{\"c\":-3}}");
            Assert.AreEqual(1, Json.Int(Json.List(Json.Get(v, "a"))[0]));
            Assert.AreEqual("xA\n", Json.Str(Json.List(Json.Get(v, "a"))[2]));
            Assert.AreEqual(-3, Json.Int(Json.At(v, "b", "c")));
            Assert.IsNull(Json.Int(2.5));
            Assert.ThrowsException<System.FormatException>(() => Json.Parse("{\"a\":1} x"));
            Assert.ThrowsException<System.FormatException>(() => Json.Parse(new string('[', 100) + new string(']', 100)));
        }
    }
}
