using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Network.Companion.Tests
{
    /// <summary>What the world says about the network, read back from chat.</summary>
    [TestClass]
    public sealed class ReplyTests
    {
        private static Reply One(string text)
        {
            var replies = new ReplyReader().Read(text);
            Assert.AreEqual(1, replies.Count, "one reply in: " + text);
            return replies[0];
        }

        [TestMethod]
        public void The_status_line_gives_the_address_the_hub_the_friends_and_the_link()
        {
            var r = One("You are on the Dereth Network as Sir Ann@frost. The hub is connected. You have 2 friends on other worlds. This character is linked to a Discord account.");
            Assert.AreEqual(ReplyKind.StatusOn, r.Kind);
            Assert.AreEqual("Sir Ann@frost", r.Address);
            Assert.AreEqual("connected", r.Hub);
            Assert.AreEqual(2, r.Count);
            Assert.IsTrue(r.IsLinked);

            r = One("You are on the Dereth Network as Ann@frost. The hub is not connected. You have no friends on other worlds.");
            Assert.AreEqual(("not connected", 0, false), (r.Hub, r.Count, r.IsLinked));
            r = One("You are on the Dereth Network as Ann@frost. The hub is connecting. You have 1 friend on other worlds.");
            Assert.AreEqual(("connecting", 1), (r.Hub, r.Count));
        }

        [TestMethod]
        public void Off_the_network_the_status_line_gives_the_address_the_character_would_join_as()
        {
            var r = One("You are not on the Dereth Network. Type @dn on to join it as Ann@frost.");
            Assert.AreEqual(ReplyKind.StatusOff, r.Kind);
            Assert.AreEqual("Ann@frost", r.Address);
            Assert.AreEqual(ReplyKind.NotOnNetwork, One("You are not on the Dereth Network. Type @dn on to join it.").Kind);
            Assert.AreEqual(ReplyKind.WorldOff, One("This world is not on the Dereth Network.").Kind);
            Assert.AreEqual(ReplyKind.AlreadyOff, One("You are not on the Dereth Network.").Kind);
        }

        [TestMethod]
        public void Joining_and_leaving_are_read()
        {
            var joined = One("You are on the Dereth Network as Ann@frost. Anyone on the network can see when you are online, send you tells and find your profile in its directory. Type @dn help for the commands.");
            Assert.AreEqual((ReplyKind.Joined, "Ann@frost"), (joined.Kind, joined.Address));
            Assert.AreEqual(ReplyKind.AlreadyOn, One("You are on the Dereth Network already, as Ann@frost.").Kind);
            Assert.AreEqual(ReplyKind.Left, One("You are off the Dereth Network. Nobody on another world can see you or tell you, and your profile is out of its directory.").Kind);
            Assert.AreEqual(ReplyKind.CannotJoin, One("This character's name cannot be a network address, so it cannot join the Dereth Network.").Kind);
        }

        [TestMethod]
        public void The_friends_list_in_one_text_carries_every_row()
        {
            var r = One("Your friends on other worlds:\nBob@coldeve: online\nSir Cara@leafcull: offline");
            Assert.AreEqual(ReplyKind.Friends, r.Kind);
            CollectionAssert.AreEqual(new[] { "Bob@coldeve", "Sir Cara@leafcull" }, r.Friends.Select(f => f.Address).ToArray());
            CollectionAssert.AreEqual(new[] { true, false }, r.Friends.Select(f => f.Online).ToArray());
        }

        [TestMethod]
        public void A_friends_list_split_over_texts_reads_its_rows_until_another_line()
        {
            var reader = new ReplyReader();
            var head = reader.Read("Your friends on other worlds:");
            Assert.AreEqual(ReplyKind.Friends, head.Single().Kind);
            var row = reader.Read("Bob@coldeve: online").Single();
            Assert.AreEqual(ReplyKind.FriendRow, row.Kind);
            Assert.AreEqual("Bob@coldeve", row.Friends.Single().Address);
            Assert.AreEqual(0, reader.Read("You say, \"Bob@coldeve: online\"").Count, "another line ends the list");
            Assert.AreEqual(0, reader.Read("Bob@coldeve: online").Count, "a row after the list ended is just a line");
        }

        [TestMethod]
        public void Presence_lines_are_read_through_the_name_link()
        {
            var on = One("<Tell:IIDString:1342177281:Bob@coldeve>Bob@coldeve<\\Tell> has come online.");
            Assert.AreEqual((ReplyKind.FriendOnline, "Bob@coldeve"), (on.Kind, on.Address));
            var off = One("<Tell:IIDString:1342177281:Bob@coldeve>Bob@coldeve<\\Tell> has gone offline.\n");
            Assert.AreEqual((ReplyKind.FriendOffline, "Bob@coldeve"), (off.Kind, off.Address));
            Assert.AreEqual(ReplyKind.FriendAdded, One("Bob@coldeve has been added to your friends list.").Kind);
            Assert.AreEqual(ReplyKind.FriendRemoved, One("Bob@coldeve has been removed from your friends list.").Kind);
            Assert.AreEqual(ReplyKind.NoFriends, One("You have no friends on other worlds. Add one with @dn friends add Name@world.").Kind);
        }

        [TestMethod]
        public void Channels_their_members_and_invitations_are_read()
        {
            var list = One("Your channels: oldguard@channel, raid-night@channel.");
            CollectionAssert.AreEqual(new[] { "oldguard@channel", "raid-night@channel" }, list.Items);
            Assert.AreEqual(ReplyKind.NoChannels, One("You are in no channel.").Kind);
            var members = One("Members of oldguard@channel: Ann@frost, Bob@coldeve.");
            Assert.AreEqual("oldguard@channel", members.Address);
            CollectionAssert.AreEqual(new[] { "Ann@frost", "Bob@coldeve" }, members.Items);
            var invite = One("Ann@frost invites you to the channel oldguard@channel. Type @dn channel join oldguard to join it.");
            Assert.AreEqual((ReplyKind.ChannelInvite, "oldguard@channel", "Ann@frost"), (invite.Kind, invite.Address, invite.Speaker));
            Assert.AreEqual(ReplyKind.ChannelMade, One("You have made the channel oldguard@channel. Invite someone with @dn channel invite oldguard Name@world.").Kind);
            Assert.AreEqual(ReplyKind.ChannelJoined, One("You have joined oldguard@channel. Talk in it with @tell oldguard@channel, text.").Kind);
            Assert.AreEqual(ReplyKind.ChannelLeft, One("You have left oldguard@channel.").Kind);
            Assert.AreEqual(ReplyKind.ChannelInvited, One("Bob@coldeve is invited to oldguard@channel.").Kind);
            Assert.AreEqual(ReplyKind.ChannelRefused, One("You are not a member of oldguard@channel.").Kind);
            Assert.AreEqual(ReplyKind.ChannelRefused, One("There is no channel oldguard@channel.").Kind);
            var said = One("<Tell:IIDString:1610612737:oldguard@channel>oldguard@channel<\\Tell> tells you, \"Bob@coldeve: I am!\"");
            Assert.AreEqual((ReplyKind.ChannelLine, "oldguard@channel", "Bob@coldeve", "I am!"), (said.Kind, said.Address, said.Speaker, said.Text));
        }

        [TestMethod]
        public void A_link_gives_its_address_its_code_and_how_long_it_lasts()
        {
            var url = One("Open https://dereth.network/link/#K7QD-2MXP within 10 minutes to link this character to your Discord account. Keep the link to yourself.");
            Assert.AreEqual((ReplyKind.LinkOffered, "https://dereth.network/link/#K7QD-2MXP", "K7QD-2MXP", "10 minutes"), (url.Kind, url.Url, url.Code, url.Lasts));
            var code = One("Your link code is K7QD-2MXP. Enter it at https://dereth.network/characters/ within 10 minutes to link this character to your Discord account. Keep the code to yourself.");
            Assert.AreEqual(("K7QD-2MXP", "https://dereth.network/characters/"), (code.Code, code.Url));
            Assert.AreEqual(ReplyKind.Linked, One("This character is now linked to a Discord account.").Kind);
            Assert.AreEqual(ReplyKind.Unlinked, One("This character is no longer linked to a Discord account.").Kind);
            Assert.AreEqual(ReplyKind.LinkRefused, One("You have asked for too many links. Wait a while and try again.").Kind);
        }

        [TestMethod]
        public void A_line_from_another_world_in_the_global_room_is_read_with_its_mark()
        {
            var r = One("[Roleplay] Bob@coldeve says, \"evening all\"");
            Assert.AreEqual((ReplyKind.GlobalLine, "Roleplay", "Bob@coldeve", "evening all", false), (r.Kind, r.Room, r.Speaker, r.Text, r.ViaDiscord));
            Assert.IsTrue(One("[Roleplay] Ann@coldeve (Discord) says, \"evening all\"").ViaDiscord);
            Assert.AreEqual(0, new ReplyReader().Read("[Roleplay] Ann says, \"a local line\"").Count, "a local speaker is the world's own room");
            Assert.AreEqual(ReplyKind.NotDelivered, One("Your line did not reach the other worlds: you are sending too quickly.").Kind);
        }

        [TestMethod]
        public void The_directory_page_and_world_squelches_are_read()
        {
            Assert.AreEqual("https://dereth.network/directory/character/?a=Sir%20Bob%40coldeve",
                One("Your directory page: https://dereth.network/directory/character/?a=Sir%20Bob%40coldeve").Url);
            Assert.AreEqual("coldeve", One("Every character of coldeve has been squelched.").Address);
            Assert.AreEqual(ReplyKind.WorldUnsquelched, One("coldeve has been unsquelched.").Kind);
        }

        [TestMethod]
        public void Lines_about_anything_else_are_skipped()
        {
            Assert.AreEqual(0, new ReplyReader().Read("You say, \"hello\"\nBob tells you, \"hi\"").Count);
        }

        /// <summary>
        /// The lines read here are the ones the worlds that speak the network show: the ACE mod beside
        /// the companion, and Empyrean where the tree holds it too.
        /// </summary>
        [TestMethod]
        public void The_lines_read_are_the_ones_the_ace_mod_and_empyrean_show()
        {
            var network = NetworkDir();
            var ace = File.ReadAllText(Path.Combine(network, "ace-mod", "Dereth.Network", "Core", "NetworkWorld.Commands.cs")) +
                      File.ReadAllText(Path.Combine(network, "ace-mod", "Dereth.Network", "Core", "NetworkWorld.Persons.cs"));
            var federation = Path.Combine(network, "..", "empyrean", "crates", "world", "src", "federation");
            var emp = Directory.Exists(federation)
                ? string.Concat(Directory.GetFiles(federation, "*.rs").Select(File.ReadAllText))
                : null;
            foreach (var phrase in new[]
                     {
                         "Your friends on other worlds:\\n",
                         "You have no friends on other worlds.",
                         "on other worlds.",
                         "Your channels: ",
                         "You are in no channel.",
                         "Members of ",
                         "to link this character to your Discord account. Keep the link to yourself.",
                         "This character is now linked to a Discord account.",
                         "invites you to the channel ",
                         "You are not on the Dereth Network. Type @dn on to join it.",
                         "Your directory page: ",
                     })
            {
                StringAssert.Contains(ace, phrase, "the ACE mod");
                if (emp != null)
                    StringAssert.Contains(emp, phrase, "Empyrean");
            }
        }

        /// <summary>The network's folder: the one holding the companion and the ACE mod.</summary>
        private static string NetworkDir([CallerFilePath] string here = "")
        {
            var dir = Path.GetFullPath(Path.Combine(Path.GetDirectoryName(here) ?? ".", "..", ".."));
            Assert.IsTrue(Directory.Exists(Path.Combine(dir, "ace-mod")), "the ACE mod beside the companion, in " + dir);
            return dir;
        }
    }
}
