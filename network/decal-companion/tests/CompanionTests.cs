using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Network.Companion.Tests
{
    /// <summary>The companion's window, driven through a stand-in for Decal's controls and the game's chat.</summary>
    [TestClass]
    public sealed class CompanionTests
    {
        private const string StatusOn =
            "You are on the Dereth Network as Ann@frost. The hub is connected. You have 1 friend on other worlds.";

        private FakeView view = new FakeView();
        private FakeGame game = new FakeGame();
        private FakeHttp http = new FakeHttp();
        private double now;

        private Companion Make() => new Companion(view, game, new HubClient("https://hub.example", http), () => now);

        [TestInitialize]
        public void Reset()
        {
            view = new FakeView();
            game = new FakeGame();
            http = new FakeHttp();
            now = 1000;
        }

        [TestMethod]
        public void Showing_a_page_asks_the_world_once_and_keeps_the_answer_out_of_chat()
        {
            var c = Make();
            c.OnPage(Controls.StatusPage);
            CollectionAssert.AreEqual(new[] { "@dn" }, game.Typed);
            Assert.IsTrue(c.OnChat(StatusOn), "the answer to the companion's own question is kept out of chat");
            Assert.IsTrue(c.OnChat("Your directory page: https://dereth.network/directory/character/?a=Ann%40frost"),
                "the page line that follows the status is the same answer");
            Assert.AreEqual("On the network as Ann@frost. The hub is connected.", view.Text(Controls.StatusLine));
            Assert.AreEqual("Your directory page: https://dereth.network/directory/character/?a=Ann%40frost", view.Text(Controls.PageLine));

            c.OnPage(Controls.FriendsPage);
            c.OnPage(Controls.StatusPage);
            CollectionAssert.AreEqual(new[] { "@dn", "@dn friends" }, game.Typed, "shown again at once, the status page asks nothing");
            now += Companion.PageQuestionSecs;
            c.OnPage(Controls.StatusPage);
            CollectionAssert.AreEqual(new[] { "@dn", "@dn friends", "@dn" }, game.Typed);
        }

        [TestMethod]
        public void The_answer_to_a_command_the_player_typed_stays_in_chat()
        {
            var c = Make();
            Assert.IsFalse(c.OnChat(StatusOn));
            Assert.AreEqual("Ann@frost", c.State.Address, "the companion still reads it");
            c.OnPage(Controls.StatusPage);
            now += Companion.AnswerSecs + 1;
            Assert.IsFalse(c.OnChat(StatusOn), "an answer that comes too late is not taken for the companion's");
        }

        [TestMethod]
        public void Buttons_type_the_commands_the_player_could_type_and_refuse_what_is_not_one()
        {
            var c = Make();
            c.OnClick(Controls.NetOn);
            c.OnClick(Controls.NetOff);
            view.Texts[Controls.FriendName] = " Sir Bob@coldeve ";
            c.OnClick(Controls.FriendAdd);
            Assert.AreEqual("", view.Text(Controls.FriendName), "the field is cleared once the command is typed");
            view.Texts[Controls.FriendName] = "Bob";
            c.OnClick(Controls.FriendAdd);
            Assert.AreEqual("Type the friend's address: Name@world.", view.Text(Controls.FriendsNote));
            c.OnClick(Controls.Link);
            CollectionAssert.AreEqual(new[] { "@dn on", "@dn off", "@dn friends add Sir Bob@coldeve", "@dn link" }, game.Typed);
        }

        [TestMethod]
        public void The_friends_list_fills_from_the_answer_and_follows_presence_lines()
        {
            var c = Make();
            c.OnPage(Controls.FriendsPage);
            Assert.IsTrue(c.OnChat("Your friends on other worlds:"));
            Assert.IsTrue(c.OnChat("Cara@leafcull: offline"), "a row of the asked-for list");
            Assert.IsTrue(c.OnChat("Bob@coldeve: online"));
            CollectionAssert.AreEqual(new[] { "Bob|coldeve|online", "Cara|leafcull|offline" }, view.Rows(Controls.Friends));

            Assert.IsFalse(c.OnChat("<Tell:IIDString:7:Bob@coldeve>Bob@coldeve<\\Tell> has gone offline."), "news stays in chat");
            Assert.IsFalse(c.OnChat("Dan@frost has been added to your friends list."));
            CollectionAssert.AreEqual(new[] { "Bob|coldeve|offline", "Cara|leafcull|offline", "Dan|frost|offline" }, view.Rows(Controls.Friends));

            c.OnSelect(Controls.Friends, 1);
            c.OnClick(Controls.FriendRemove);
            Assert.AreEqual("@dn friends remove Cara@leafcull", game.Typed.Last());
            c.OnChat("Cara@leafcull has been removed from your friends list.");
            CollectionAssert.AreEqual(new[] { "Bob|coldeve|offline", "Dan|frost|offline" }, view.Rows(Controls.Friends));
        }

        [TestMethod]
        public void A_refusal_answers_the_question_that_was_asked()
        {
            var c = Make();
            c.OnPage(Controls.FriendsPage);
            Assert.IsTrue(c.OnChat("You are not on the Dereth Network. Type @dn on to join it."));
            Assert.AreEqual("Off the network.", view.Text(Controls.StatusLine));
            Assert.IsFalse(c.OnChat("You are not on the Dereth Network. Type @dn on to join it."), "only one question was asked");
        }

        [TestMethod]
        public void Picking_a_channel_asks_for_its_members()
        {
            var c = Make();
            c.OnPage(Controls.ChannelsPage);
            Assert.IsTrue(c.OnChat("Your channels: oldguard@channel, raid@channel."));
            CollectionAssert.AreEqual(new[] { "oldguard@channel", "raid@channel" }, view.Rows(Controls.Channels));
            c.OnSelect(Controls.Channels, 1);
            Assert.AreEqual("@dn channel members raid", game.Typed.Last());
            Assert.IsTrue(c.OnChat("Members of raid@channel: Ann@frost, Bob@coldeve."));
            CollectionAssert.AreEqual(new[] { "Ann@frost", "Bob@coldeve" }, view.Rows(Controls.Members));

            view.Texts[Controls.InviteName] = "Cara@leafcull";
            c.OnClick(Controls.ChannelInvite);
            Assert.AreEqual("@dn channel invite raid Cara@leafcull", game.Typed.Last());
            c.OnClick(Controls.ChannelLeave);
            Assert.AreEqual("@dn channel leave raid", game.Typed.Last());
            Assert.IsFalse(c.OnChat("You have left raid@channel."));
            CollectionAssert.AreEqual(new[] { "oldguard@channel" }, view.Rows(Controls.Channels));
        }

        [TestMethod]
        public void An_invitation_is_listed_and_accepted_with_a_click()
        {
            var c = Make();
            Assert.IsFalse(c.OnChat("Ann@frost invites you to the channel oldguard@channel. Type @dn channel join oldguard to join it."));
            CollectionAssert.AreEqual(new[] { "oldguard@channel|Ann@frost" }, view.Rows(Controls.Invites));
            c.OnSelect(Controls.Invites, 0);
            c.OnClick(Controls.InviteJoin);
            Assert.AreEqual("@dn channel join oldguard", game.Typed.Last());
            c.OnChat("You have joined oldguard@channel. Talk in it with @tell oldguard@channel, text.");
            Assert.AreEqual(0, view.Rows(Controls.Invites).Length);
            CollectionAssert.AreEqual(new[] { "oldguard@channel" }, view.Rows(Controls.Channels));
        }

        [TestMethod]
        public void Lines_from_other_worlds_in_the_global_room_are_kept_and_a_line_is_said_with_a_click()
        {
            var c = Make();
            c.OnChat("[Roleplay] Bob@coldeve says, \"evening all\"");
            c.OnChat("[Roleplay] Cara@leafcull (Discord) says, \"hi\"");
            CollectionAssert.AreEqual(new[] { "Bob@coldeve|evening all", "Cara@leafcull (Discord)|hi" }, view.Rows(Controls.GlobalLines));
            view.Texts[Controls.GlobalText] = "hello all";
            c.OnClick(Controls.GlobalSay);
            Assert.AreEqual("@dng hello all", game.Typed.Last());
            Assert.AreEqual("", view.Text(Controls.GlobalText));
            for (var i = 0; i < Companion.MaxLines + 5; i++)
                c.OnChat("[Roleplay] Bob@coldeve says, \"" + i + "\"");
            Assert.AreEqual(Companion.MaxLines, view.Rows(Controls.GlobalLines).Length);
        }

        [TestMethod]
        public void A_link_shows_its_address_its_code_and_how_long_it_lasts()
        {
            var c = Make();
            c.OnClick(Controls.Link);
            Assert.IsFalse(c.OnChat("Open https://dereth.network/link/#K7QD-2MXP within 10 minutes to link this character to your Discord account. Keep the link to yourself."),
                "the link stays in chat as well");
            Assert.AreEqual("https://dereth.network/link/#K7QD-2MXP", view.Text(Controls.LinkUrl));
            Assert.AreEqual("Code K7QD-2MXP. Open the link within 10 minutes.", view.Text(Controls.LinkNote));
            c.OnChat("This character is now linked to a Discord account.");
            Assert.AreEqual("", view.Text(Controls.LinkUrl));
            Assert.AreEqual("Linked to a Discord account.", view.Text(Controls.LinkNote));
        }

        [TestMethod]
        public void The_world_list_is_fetched_while_its_page_shows_and_again_a_minute_later()
        {
            http.Answers["https://hub.example/v1/servers?limit=100&offset=0"] =
                "{\"data\":[{\"slug\":\"frost\",\"name\":\"Frost\",\"federation\":\"on\",\"status\":{\"state\":\"online\",\"playersOnline\":12}}],\"total\":1}";
            var c = Make();
            c.OnChat(StatusOn);
            http.Asked.Clear();
            c.OnPage(Controls.WorldsPage);
            CollectionAssert.AreEqual(new[] { "Frost (here)|online|12|yes" }, view.Rows(Controls.Worlds));
            Assert.AreEqual(1, http.Asked.Count);
            now += 30;
            c.Tick();
            Assert.AreEqual(1, http.Asked.Count, "not again within the minute");
            now += 31;
            c.Tick();
            Assert.AreEqual(2, http.Asked.Count, "again a minute on");
            c.OnClosed();
            now += 120;
            c.Tick();
            Assert.AreEqual(2, http.Asked.Count, "not while the window is closed");
        }

        [TestMethod]
        public void A_hub_that_cannot_be_reached_is_said_on_the_page()
        {
            var c = Make();
            c.OnPage(Controls.WorldsPage);
            StringAssert.StartsWith(view.Text(Controls.WorldsNote), "The hub could not be reached");
        }

        [TestMethod]
        public void The_worlds_plugin_policy_is_read_once_its_slug_is_known()
        {
            http.Answers["https://hub.example/v1/servers/frost"] =
                "{\"slug\":\"frost\",\"plugins\":{\"policy\":\"policy_version = 1\\n[automation]\\ncombat = \\\"deny\\\"\\nnav = \\\"interactive-only\\\"\\n[notes]\\ntext = \\\"No unattended macros.\\\"\\n\"}}";
            var c = Make();
            c.OnChat(StatusOn);
            Assert.AreEqual("This world's rules for plugins: No combat automation. On a click only: nav. No unattended macros.",
                view.Text(Controls.PolicyNote));
            c.OnChat(StatusOn);
            Assert.AreEqual(1, http.Asked.Count(u => u.EndsWith("/v1/servers/frost", StringComparison.Ordinal)), "asked once");
        }

        [TestMethod]
        public void A_policy_published_at_an_address_is_fetched_from_there()
        {
            http.Answers["https://hub.example/v1/servers/frost"] = "{\"slug\":\"frost\",\"plugin_policy\":\"https://frost.example/policy.toml\"}";
            http.Answers["https://frost.example/policy.toml"] = "policy_version = 1\n[automation]\ndefault = \"deny\"\nloot = \"allow\"\n";
            var c = Make();
            c.OnChat("You are not on the Dereth Network. Type @dn on to join it as Ann@frost.");
            Assert.AreEqual("This world's rules for plugins: Anything else: no.", view.Text(Controls.PolicyNote));
        }

        [TestMethod]
        public void A_world_that_publishes_no_policy_is_said_to()
        {
            http.Answers["https://hub.example/v1/servers/frost"] = "{\"slug\":\"frost\"}";
            var c = Make();
            c.OnChat(StatusOn);
            Assert.AreEqual("This world publishes no rules for plugins.", view.Text(Controls.PolicyNote));
        }

        [TestMethod]
        public void A_search_fills_the_results()
        {
            http.Answers["https://hub.example/v1/directory/characters?q=Bob&limit=50"] =
                "{\"data\":[{\"address\":\"Bob@coldeve\",\"world\":{\"slug\":\"coldeve\",\"name\":\"Coldeve\"},\"level\":80}]}";
            var c = Make();
            view.Texts[Controls.SearchText] = "Bob";
            c.OnClick(Controls.Search);
            CollectionAssert.AreEqual(new[] { "Bob@coldeve|80|Coldeve" }, view.Rows(Controls.Results));
            Assert.AreEqual("", view.Text(Controls.SearchNote));
            view.Texts[Controls.SearchText] = "x";
            c.OnClick(Controls.Search);
            Assert.AreEqual("Search for 2 to 32 letters of a name.", view.Text(Controls.SearchNote));
        }
    }

    internal sealed class FakeView : ICompanionView
    {
        public readonly Dictionary<string, string> Texts = new Dictionary<string, string>();
        public readonly Dictionary<string, IList<string[]>> Lists = new Dictionary<string, IList<string[]>>();

        public void SetText(string control, string text) => Texts[control] = text;

        public string GetText(string control) => Texts.TryGetValue(control, out var t) ? t : "";

        public void SetRows(string list, IList<string[]> rows) => Lists[list] = rows.ToList();

        public string Text(string control) => GetText(control);

        public string[] Rows(string list) =>
            Lists.TryGetValue(list, out var rows) ? rows.Select(r => string.Join("|", r)).ToArray() : new string[0];
    }

    internal sealed class FakeGame : IGame
    {
        public readonly List<string> Typed = new List<string>();

        public void Type(string line) => Typed.Add(line);
    }

    /// <summary>Answers from a table at once; any other address is unreachable.</summary>
    internal sealed class FakeHttp : IHttp
    {
        public readonly Dictionary<string, string> Answers = new Dictionary<string, string>(StringComparer.Ordinal);
        public readonly List<string> Asked = new List<string>();

        public void Get(string url, Action<HttpReply> done)
        {
            Asked.Add(url);
            done(Answers.TryGetValue(url, out var body)
                ? new HttpReply { Status = 200, Body = body }
                : new HttpReply { Error = "no route to host" });
        }
    }
}
