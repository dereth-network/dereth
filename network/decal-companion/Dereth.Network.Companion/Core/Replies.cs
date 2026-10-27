using System.Collections.Generic;
using System.Text.RegularExpressions;

namespace Dereth.Network.Companion
{
    /// <summary>What a line a world shows about the Dereth Network says.</summary>
    public enum ReplyKind
    {
        /// <summary>The character is on the network: its address, the hub's link, its friends count and whether it is linked.</summary>
        StatusOn,
        /// <summary>The character is off the network; <see cref="Reply.Address"/> is the address it would join as.</summary>
        StatusOff,
        /// <summary>A command was refused because the character is not on the network.</summary>
        NotOnNetwork,
        /// <summary>The world itself is not on the network.</summary>
        WorldOff,
        /// <summary>The character joined the network (<c>@dn on</c>).</summary>
        Joined,
        /// <summary>The character was on the network already.</summary>
        AlreadyOn,
        /// <summary>The character left the network (<c>@dn off</c>).</summary>
        Left,
        /// <summary>The character was off the network already.</summary>
        AlreadyOff,
        /// <summary>The character's name cannot be an address.</summary>
        CannotJoin,
        /// <summary>The character's page in the directory.</summary>
        DirectoryPage,
        /// <summary>The friends on other worlds, with the rows that came in the same text.</summary>
        Friends,
        /// <summary>One more row of the friends list.</summary>
        FriendRow,
        /// <summary>No friends on other worlds.</summary>
        NoFriends,
        FriendOnline,
        FriendOffline,
        FriendAdded,
        FriendRemoved,
        /// <summary>The character's channels.</summary>
        Channels,
        NoChannels,
        /// <summary>A channel's members.</summary>
        Members,
        ChannelMade,
        ChannelInvited,
        ChannelJoined,
        ChannelLeft,
        /// <summary>Someone invites the character to a channel.</summary>
        ChannelInvite,
        /// <summary>A channel request was refused, with the reason in <see cref="Reply.Line"/>.</summary>
        ChannelRefused,
        /// <summary>A line said in a channel.</summary>
        ChannelLine,
        /// <summary>A link to open to link the character to a Discord account.</summary>
        LinkOffered,
        Linked,
        Unlinked,
        NotLinked,
        /// <summary>The character's link is paused while it is off the network.</summary>
        LinkPaused,
        /// <summary>No link was given, with the reason in <see cref="Reply.Line"/>.</summary>
        LinkRefused,
        WorldSquelched,
        WorldUnsquelched,
        /// <summary>A line from another world in the room that carries the Global room.</summary>
        GlobalLine,
        /// <summary>The world carries no Global room.</summary>
        GlobalNotCarried,
        /// <summary>A line did not reach the other worlds or Discord, with the reason in <see cref="Reply.Line"/>.</summary>
        NotDelivered,
    }

    /// <summary>One friend in the friends list.</summary>
    public sealed class FriendRow
    {
        public FriendRow(string address, bool online)
        {
            Address = address;
            Online = online;
        }

        public string Address { get; }
        public bool Online { get; }
    }

    /// <summary>One line a world showed about the network, read.</summary>
    public sealed class Reply
    {
        public Reply(ReplyKind kind, string line)
        {
            Kind = kind;
            Line = line;
        }

        public ReplyKind Kind { get; }
        /// <summary>The line as the world showed it, links taken out.</summary>
        public string Line { get; }
        /// <summary>A character's or a channel's address, or a world's slug.</summary>
        public string Address { get; set; } = "";
        /// <summary>The hub's link: <c>connected</c>, <c>connecting</c> or <c>not connected</c>.</summary>
        public string Hub { get; set; } = "";
        /// <summary>How many friends on other worlds.</summary>
        public int Count { get; set; }
        public bool IsLinked { get; set; }
        public List<FriendRow> Friends { get; } = new List<FriendRow>();
        /// <summary>Channels, or a channel's members.</summary>
        public List<string> Items { get; } = new List<string>();
        public string Url { get; set; } = "";
        public string Code { get; set; } = "";
        /// <summary>How long a link lasts, as the world said it (<c>10 minutes</c>).</summary>
        public string Lasts { get; set; } = "";
        public string Room { get; set; } = "";
        public string Speaker { get; set; } = "";
        public string Text { get; set; } = "";
        public bool ViaDiscord { get; set; }
    }

    /// <summary>
    /// Reads what the world shows in chat about the Dereth Network. The friends list is a header line
    /// and a line for each friend, which may come in one text or in several: a row that follows the
    /// header, or another row, is read as the list's.
    /// </summary>
    public sealed class ReplyReader
    {
        private const RegexOptions Opts = RegexOptions.CultureInvariant;
        private const string Addr = @"(?<addr>[^@]+@[a-z0-9][a-z0-9-]*[a-z0-9])";
        private const string Chan = @"(?<ch>[a-z0-9-]+@channel)";

        private static readonly Regex StatusOn = new Regex(
            @"^You are on the Dereth Network as " + Addr + @"\. The hub is (?<hub>connected|connecting|not connected)\. " +
            @"(?:You have no friends on other worlds\.|You have (?<n>\d+) friends? on other worlds\.)" +
            @"(?<linked> This character is linked to a Discord account\.)?$", Opts);
        private static readonly Regex StatusOff = new Regex(
            @"^You are not on the Dereth Network\. Type @dn on to join it as " + Addr + @"\.$", Opts);
        private static readonly Regex Joined = new Regex(
            @"^You are on the Dereth Network as " + Addr + @"\. Anyone on the network can see", Opts);
        private static readonly Regex AlreadyOn = new Regex(
            @"^You are on the Dereth Network already, as " + Addr + @"\.$", Opts);
        private static readonly Regex Page = new Regex(@"^Your directory page: (?<url>\S+)$", Opts);
        private static readonly Regex Row = new Regex("^" + Addr + @": (?<state>online|offline)$", Opts);
        private static readonly Regex Online = new Regex("^" + Addr + @" has come online\.$", Opts);
        private static readonly Regex Offline = new Regex("^" + Addr + @" has gone offline\.$", Opts);
        private static readonly Regex Added = new Regex("^" + Addr + @" has been added to your friends list\.$", Opts);
        private static readonly Regex Removed = new Regex("^" + Addr + @" has been removed from your friends list\.$", Opts);
        private static readonly Regex ChannelList = new Regex(@"^Your channels: (?<list>.+)\.$", Opts);
        private static readonly Regex Members = new Regex("^Members of " + Chan + @": (?<list>.+)\.$", Opts);
        private static readonly Regex Made = new Regex("^You have made the channel " + Chan + @"\.", Opts);
        private static readonly Regex Invited = new Regex(@"^(?<who>.+) is invited to " + Chan + @"\.$", Opts);
        private static readonly Regex JoinedChannel = new Regex("^You have joined " + Chan + @"\.", Opts);
        private static readonly Regex LeftChannel = new Regex("^You have left " + Chan + @"\.$", Opts);
        private static readonly Regex Invite = new Regex(
            "^" + Addr + " invites you to the channel " + Chan + @"\. Type @dn channel join (?<name>[a-z0-9-]+) to join it\.$", Opts);
        private static readonly Regex ChannelRefused = new Regex(
            "^(?:" + Chan + @" already exists\.|There is no channel " + Chan + @"\.|You are not a member of " + Chan +
            @"\.|You have not been invited to " + Chan + @"\.|That character is already a member of " + Chan +
            @"\.|A channel's name is 2 to 20 letters, digits and hyphens\.)$", Opts);
        private static readonly Regex ChannelLine = new Regex("^" + Chan + @" tells you, ""(?<who>[^:""]+): (?<text>.*)""$", Opts);
        private static readonly Regex LinkUrl = new Regex(
            @"^Open (?<url>\S+) within (?<lasts>.+?) to link this character to your Discord account\.", Opts);
        private static readonly Regex LinkCode = new Regex(
            @"^Your link code is (?<code>\S+)\. Enter it at (?<url>\S+) within (?<lasts>.+?) to link this character", Opts);
        private static readonly Regex Squelched = new Regex(@"^Every character of (?<w>[a-z0-9-]+) has been squelched\.$", Opts);
        private static readonly Regex Unsquelched = new Regex(@"^(?<w>[a-z0-9-]+) has been unsquelched\.$", Opts);
        private static readonly Regex Global = new Regex(
            @"^\[(?<room>[^\]]+)\] " + Addr + @"(?<discord> \(Discord\))? says, ""(?<text>.*)""$", Opts);
        private static readonly Regex NotDelivered = new Regex(@"^Your line did not reach (?<where>.+?): ", Opts);

        private bool inFriends;

        /// <summary>Every line of <paramref name="text"/> that is about the network, in order; the others are skipped.</summary>
        public List<Reply> Read(string text)
        {
            var replies = new List<Reply>();
            Reply? list = null;
            foreach (var line in ChatText.Lines(text))
            {
                if (inFriends)
                {
                    var row = Row.Match(line);
                    if (row.Success)
                    {
                        var friend = new FriendRow(row.Groups["addr"].Value, row.Groups["state"].Value == "online");
                        if (list != null)
                        {
                            list.Friends.Add(friend);
                        }
                        else
                        {
                            var r = new Reply(ReplyKind.FriendRow, line);
                            r.Friends.Add(friend);
                            replies.Add(r);
                        }
                        continue;
                    }
                    inFriends = false;
                }
                var reply = ReadLine(line);
                if (reply == null)
                    continue;
                if (reply.Kind == ReplyKind.Friends)
                {
                    inFriends = true;
                    list = reply;
                }
                replies.Add(reply);
            }
            return replies;
        }

        /// <summary>Whether the next line may still be a row of the friends list.</summary>
        public bool InFriendsList => inFriends;

        private static Reply? ReadLine(string line)
        {
            Match m;
            switch (line)
            {
                case "You are not on the Dereth Network. Type @dn on to join it.":
                    return new Reply(ReplyKind.NotOnNetwork, line);
                case "This world is not on the Dereth Network.":
                    return new Reply(ReplyKind.WorldOff, line);
                case "You are not on the Dereth Network.":
                    return new Reply(ReplyKind.AlreadyOff, line);
                case "This character's name cannot be a network address, so it cannot join the Dereth Network.":
                    return new Reply(ReplyKind.CannotJoin, line);
                case "Your friends on other worlds:":
                    return new Reply(ReplyKind.Friends, line);
                case "You are in no channel.":
                    return new Reply(ReplyKind.NoChannels, line);
                case "This character is now linked to a Discord account.":
                    return new Reply(ReplyKind.Linked, line);
                case "This character is no longer linked to a Discord account.":
                    return new Reply(ReplyKind.Unlinked, line);
                case "This character is not linked to a Discord account.":
                    return new Reply(ReplyKind.NotLinked, line);
                case "This character's link to your Discord account is paused until you type @dn on.":
                    return new Reply(ReplyKind.LinkPaused, line);
                case "You have asked for too many links. Wait a while and try again.":
                case "The Dereth Network's hub does not link characters.":
                case "The Dereth Network gave no link.":
                    return new Reply(ReplyKind.LinkRefused, line);
                case "This world does not carry the Global room.":
                    return new Reply(ReplyKind.GlobalNotCarried, line);
            }
            if (line.StartsWith("You have no friends on other worlds.", System.StringComparison.Ordinal))
                return new Reply(ReplyKind.NoFriends, line);
            if (line.StartsWith("You are off the Dereth Network.", System.StringComparison.Ordinal))
                return new Reply(ReplyKind.Left, line);
            if ((m = StatusOn.Match(line)).Success)
            {
                var n = m.Groups["n"].Success ? int.Parse(m.Groups["n"].Value, System.Globalization.CultureInfo.InvariantCulture) : 0;
                return new Reply(ReplyKind.StatusOn, line)
                {
                    Address = m.Groups["addr"].Value,
                    Hub = m.Groups["hub"].Value,
                    Count = n,
                    IsLinked = m.Groups["linked"].Success,
                };
            }
            if ((m = StatusOff.Match(line)).Success)
                return new Reply(ReplyKind.StatusOff, line) { Address = m.Groups["addr"].Value };
            if ((m = Joined.Match(line)).Success)
                return new Reply(ReplyKind.Joined, line) { Address = m.Groups["addr"].Value };
            if ((m = AlreadyOn.Match(line)).Success)
                return new Reply(ReplyKind.AlreadyOn, line) { Address = m.Groups["addr"].Value };
            if ((m = Page.Match(line)).Success)
                return new Reply(ReplyKind.DirectoryPage, line) { Url = m.Groups["url"].Value };
            if ((m = Online.Match(line)).Success)
                return new Reply(ReplyKind.FriendOnline, line) { Address = m.Groups["addr"].Value };
            if ((m = Offline.Match(line)).Success)
                return new Reply(ReplyKind.FriendOffline, line) { Address = m.Groups["addr"].Value };
            if ((m = Added.Match(line)).Success)
                return new Reply(ReplyKind.FriendAdded, line) { Address = m.Groups["addr"].Value };
            if ((m = Removed.Match(line)).Success)
                return new Reply(ReplyKind.FriendRemoved, line) { Address = m.Groups["addr"].Value };
            if ((m = Members.Match(line)).Success)
            {
                var r = new Reply(ReplyKind.Members, line) { Address = m.Groups["ch"].Value };
                r.Items.AddRange(SplitList(m.Groups["list"].Value));
                return r;
            }
            if ((m = ChannelList.Match(line)).Success)
            {
                var r = new Reply(ReplyKind.Channels, line);
                r.Items.AddRange(SplitList(m.Groups["list"].Value));
                return r;
            }
            if ((m = Made.Match(line)).Success)
                return new Reply(ReplyKind.ChannelMade, line) { Address = m.Groups["ch"].Value };
            if ((m = Invite.Match(line)).Success)
                return new Reply(ReplyKind.ChannelInvite, line) { Address = m.Groups["ch"].Value, Speaker = m.Groups["addr"].Value };
            if ((m = Invited.Match(line)).Success)
                return new Reply(ReplyKind.ChannelInvited, line) { Address = m.Groups["ch"].Value, Speaker = m.Groups["who"].Value };
            if ((m = JoinedChannel.Match(line)).Success)
                return new Reply(ReplyKind.ChannelJoined, line) { Address = m.Groups["ch"].Value };
            if ((m = LeftChannel.Match(line)).Success)
                return new Reply(ReplyKind.ChannelLeft, line) { Address = m.Groups["ch"].Value };
            if ((m = ChannelRefused.Match(line)).Success)
                return new Reply(ReplyKind.ChannelRefused, line) { Address = m.Groups["ch"].Value };
            if ((m = ChannelLine.Match(line)).Success)
                return new Reply(ReplyKind.ChannelLine, line)
                {
                    Address = m.Groups["ch"].Value,
                    Speaker = m.Groups["who"].Value,
                    Text = m.Groups["text"].Value,
                };
            if ((m = LinkUrl.Match(line)).Success)
            {
                var url = m.Groups["url"].Value;
                var hash = url.IndexOf('#');
                return new Reply(ReplyKind.LinkOffered, line)
                {
                    Url = url,
                    Code = hash >= 0 ? url.Substring(hash + 1) : "",
                    Lasts = m.Groups["lasts"].Value,
                };
            }
            if ((m = LinkCode.Match(line)).Success)
                return new Reply(ReplyKind.LinkOffered, line)
                {
                    Url = m.Groups["url"].Value,
                    Code = m.Groups["code"].Value,
                    Lasts = m.Groups["lasts"].Value,
                };
            if ((m = Squelched.Match(line)).Success)
                return new Reply(ReplyKind.WorldSquelched, line) { Address = m.Groups["w"].Value };
            if ((m = Unsquelched.Match(line)).Success)
                return new Reply(ReplyKind.WorldUnsquelched, line) { Address = m.Groups["w"].Value };
            if ((m = Global.Match(line)).Success)
                return new Reply(ReplyKind.GlobalLine, line)
                {
                    Room = m.Groups["room"].Value,
                    Speaker = m.Groups["addr"].Value,
                    Text = m.Groups["text"].Value,
                    ViaDiscord = m.Groups["discord"].Success,
                };
            if (NotDelivered.IsMatch(line))
                return new Reply(ReplyKind.NotDelivered, line);
            return null;
        }

        private static IEnumerable<string> SplitList(string list)
        {
            foreach (var item in list.Split(new[] { ", " }, System.StringSplitOptions.RemoveEmptyEntries))
                yield return item.Trim();
        }
    }
}
