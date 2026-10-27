using System;
using System.Collections.Generic;
using System.Globalization;

namespace Dereth.Network.Companion
{
    /// <summary>The window the companion draws in: controls by name.</summary>
    public interface ICompanionView
    {
        /// <summary>Set a label's, a button's or a text field's text.</summary>
        void SetText(string control, string text);

        /// <summary>A text field's text.</summary>
        string GetText(string control);

        /// <summary>Replace a list's rows; each row's cells in column order.</summary>
        void SetRows(string list, IList<string[]> rows);
    }

    /// <summary>The game, as the companion reaches it.</summary>
    public interface IGame
    {
        /// <summary>Type a line in the chat bar, as the player would.</summary>
        void Type(string line);
    }

    /// <summary>The controls of the companion's window, by name, as its view declares them.</summary>
    public static class Controls
    {
        public const int StatusPage = 0;
        public const int FriendsPage = 1;
        public const int ChannelsPage = 2;
        public const int GlobalPage = 3;
        public const int DirectoryPage = 4;
        public const int WorldsPage = 5;

        public const string StatusLine = "statusLine";
        public const string NetOn = "netOn";
        public const string NetOff = "netOff";
        public const string StatusRefresh = "statusRefresh";
        public const string PageLine = "pageLine";
        public const string Link = "link";
        public const string Unlink = "unlink";
        public const string LinkUrl = "linkUrl";
        public const string LinkNote = "linkNote";
        public const string PolicyNote = "policyNote";
        public const string StatusNote = "statusNote";

        public const string Friends = "friends";
        public const string FriendName = "friendName";
        public const string FriendAdd = "friendAdd";
        public const string FriendRemove = "friendRemove";
        public const string FriendsRefresh = "friendsRefresh";
        public const string FriendsNote = "friendsNote";

        public const string Channels = "channels";
        public const string Members = "members";
        public const string ChannelName = "channelName";
        public const string ChannelCreate = "channelCreate";
        public const string ChannelJoin = "channelJoin";
        public const string ChannelLeave = "channelLeave";
        public const string ChannelsRefresh = "channelsRefresh";
        public const string InviteName = "inviteName";
        public const string ChannelInvite = "channelInvite";
        public const string Invites = "invites";
        public const string InviteJoin = "inviteJoin";
        public const string ChannelLines = "channelLines";
        public const string ChannelsNote = "channelsNote";

        public const string GlobalLines = "globalLines";
        public const string GlobalText = "globalText";
        public const string GlobalSay = "globalSay";
        public const string GlobalNote = "globalNote";

        public const string SearchText = "searchText";
        public const string Search = "search";
        public const string Results = "results";
        public const string SearchNote = "searchNote";

        public const string Worlds = "worlds";
        public const string WorldsRefresh = "worldsRefresh";
        public const string WorldsNote = "worldsNote";
    }

    /// <summary>
    /// The Dereth Network companion: a window over the <c>@dn</c> commands and the hub's public API.
    /// It types a command only when the player clicks (a button, a list row, or a page of its window
    /// shown), reads the world's answers from chat, and keeps the answers to its own questions out of
    /// the chat window; the answers to commands the player types themselves stay in chat.
    /// </summary>
    public sealed class Companion
    {
        /// <summary>How long the companion waits for the answer to a question it asked, in seconds.</summary>
        public const double AnswerSecs = 10;
        /// <summary>The least time between two of the same question asked by showing a page, in seconds.</summary>
        public const double PageQuestionSecs = 10;
        /// <summary>How often the world list is fetched again while its page shows, in seconds.</summary>
        public const double WorldsSecs = 60;
        /// <summary>The most lines kept of the Global room and of the channels.</summary>
        public const int MaxLines = 50;

        private readonly ICompanionView view;
        private readonly IGame game;
        private readonly HubClient hub;
        private readonly Func<double> clock;
        private readonly ReplyReader reader = new ReplyReader();
        private readonly List<Pending> pending = new List<Pending>();
        private readonly Dictionary<string, double> asked = new Dictionary<string, double>(StringComparer.Ordinal);
        private bool eatingFriends;
        private double pageFollowsUntil = double.NegativeInfinity;
        private int page = -1;
        private double worldsAt = double.NegativeInfinity;
        private bool worldsLoading;
        private string policyFor = "";

        public Companion(ICompanionView view, IGame game, HubClient hub, Func<double> clock)
        {
            this.view = view;
            this.game = game;
            this.hub = hub;
            this.clock = clock;
            Render();
        }

        /// <summary>What the companion knows.</summary>
        public CompanionState State { get; } = new CompanionState();

        /// <summary>A line the game shows in chat: read, and answered with whether it is to be kept out of the chat window.</summary>
        public bool OnChat(string text)
        {
            var replies = reader.Read(text);
            if (replies.Count == 0)
                return false;
            var now = clock();
            pending.RemoveAll(p => p.Until < now);
            var eat = true;
            foreach (var r in replies)
            {
                Apply(r, now);
                eat &= Consume(r, now);
            }
            Render();
            return eat;
        }

        /// <summary>A click on a button.</summary>
        public void OnClick(string control)
        {
            switch (control)
            {
                case Controls.NetOn:
                    game.Type(Commands.On);
                    break;
                case Controls.NetOff:
                    game.Type(Commands.Off);
                    break;
                case Controls.StatusRefresh:
                    Ask(Question.Status);
                    break;
                case Controls.Link:
                    game.Type(Commands.Link);
                    break;
                case Controls.Unlink:
                    game.Type(Commands.Unlink);
                    break;
                case Controls.FriendsRefresh:
                    Ask(Question.Friends);
                    break;
                case Controls.FriendAdd:
                    TypeOr(Commands.AddFriend(view.GetText(Controls.FriendName)), Controls.FriendsNote,
                        "Type the friend's address: Name@world.", Controls.FriendName);
                    break;
                case Controls.FriendRemove:
                    TypeOr(State.SelectedFriend.Length == 0 ? null : Commands.RemoveFriend(State.SelectedFriend),
                        Controls.FriendsNote, "Pick a friend in the list first.", null);
                    break;
                case Controls.ChannelsRefresh:
                    Ask(Question.Channels);
                    break;
                case Controls.ChannelCreate:
                    TypeOr(Commands.CreateChannel(view.GetText(Controls.ChannelName)), Controls.ChannelsNote,
                        "A channel's name is 2 to 20 letters, digits and hyphens.", Controls.ChannelName);
                    break;
                case Controls.ChannelJoin:
                    TypeOr(Commands.JoinChannel(ChannelInHand()), Controls.ChannelsNote,
                        "Type a channel's name, or pick one.", null);
                    break;
                case Controls.ChannelLeave:
                    TypeOr(Commands.LeaveChannel(ChannelInHand()), Controls.ChannelsNote,
                        "Type a channel's name, or pick one.", null);
                    break;
                case Controls.ChannelInvite:
                    TypeOr(Commands.Invite(ChannelInHand(), view.GetText(Controls.InviteName)), Controls.ChannelsNote,
                        "Pick a channel and type who to invite: Name@world.", Controls.InviteName);
                    break;
                case Controls.InviteJoin:
                    TypeOr(State.SelectedInvite.Length == 0 ? null : Commands.JoinChannel(State.SelectedInvite),
                        Controls.ChannelsNote, "Pick an invitation first.", null);
                    break;
                case Controls.GlobalSay:
                    var said = view.GetText(Controls.GlobalText);
                    TypeOr(Commands.SayGlobal(said), Controls.GlobalNote, "Type a line to say first.", Controls.GlobalText);
                    break;
                case Controls.Search:
                    Search();
                    break;
                case Controls.WorldsRefresh:
                    FetchWorlds();
                    break;
            }
            Render();
        }

        /// <summary>A click on a list's row, counted from 0.</summary>
        public void OnSelect(string list, int row)
        {
            switch (list)
            {
                case Controls.Friends:
                    var friends = State.FriendList();
                    if (row >= 0 && row < friends.Count)
                        State.SelectedFriend = friends[row].Address;
                    break;
                case Controls.Channels:
                    if (row >= 0 && row < State.Channels.Count)
                    {
                        State.SelectedChannel = State.Channels[row];
                        var line = Commands.Members(State.SelectedChannel);
                        if (line != null)
                            Ask(Question.Members, line);
                    }
                    break;
                case Controls.Invites:
                    if (row >= 0 && row < State.Invites.Count)
                        State.SelectedInvite = State.Invites[row].Channel;
                    break;
            }
            Render();
        }

        /// <summary>A page of the window shown: the player looking at it asks for what it shows.</summary>
        public void OnPage(int shown)
        {
            page = shown;
            switch (shown)
            {
                case Controls.StatusPage:
                    AskFromPage(Question.Status);
                    break;
                case Controls.FriendsPage:
                    AskFromPage(Question.Friends);
                    break;
                case Controls.ChannelsPage:
                    AskFromPage(Question.Channels);
                    break;
                case Controls.WorldsPage:
                    if (clock() - worldsAt >= PageQuestionSecs)
                        FetchWorlds();
                    break;
            }
            Render();
        }

        /// <summary>The window was closed: nothing shows until a page is shown again.</summary>
        public void OnClosed() => page = -1;

        /// <summary>Once a frame: the world list fetched again while its page shows.</summary>
        public void Tick()
        {
            if (page == Controls.WorldsPage && !worldsLoading && clock() - worldsAt >= WorldsSecs)
                FetchWorlds();
        }

        private enum Question
        {
            Status,
            Friends,
            Channels,
            Members,
        }

        private sealed class Pending
        {
            public Question Q;
            public double Until;
        }

        private void AskFromPage(Question q)
        {
            var key = q.ToString();
            if (asked.TryGetValue(key, out var at) && clock() - at < PageQuestionSecs)
                return;
            Ask(q);
        }

        private void Ask(Question q, string? line = null)
        {
            var now = clock();
            asked[q.ToString()] = now;
            pending.Add(new Pending { Q = q, Until = now + AnswerSecs });
            game.Type(line ?? (q == Question.Status ? Commands.Status : q == Question.Friends ? Commands.Friends : Commands.Channels));
        }

        private void TypeOr(string? line, string note, string why, string? clear)
        {
            if (line == null)
            {
                view.SetText(note, why);
                return;
            }
            view.SetText(note, "");
            game.Type(line);
            if (clear != null)
                view.SetText(clear, "");
        }

        private string ChannelInHand()
        {
            var typed = view.GetText(Controls.ChannelName).Trim();
            return typed.Length > 0 ? typed : State.SelectedChannel;
        }

        /// <summary>Whether a reply answers one of the companion's own questions, which it then takes.</summary>
        private bool Consume(Reply r, double now)
        {
            switch (r.Kind)
            {
                case ReplyKind.FriendRow:
                    return eatingFriends;
                case ReplyKind.DirectoryPage:
                    return now <= pageFollowsUntil;
            }
            eatingFriends = false;
            int i;
            if (r.Kind == ReplyKind.NotOnNetwork || r.Kind == ReplyKind.WorldOff)
            {
                // A refusal answers whichever question was asked first.
                i = pending.Count > 0 ? 0 : -1;
            }
            else
            {
                var q = QuestionOf(r.Kind);
                if (q == null)
                    return false;
                i = pending.FindIndex(p => p.Q == q.Value);
            }
            if (i < 0)
                return false;
            pending.RemoveAt(i);
            if (r.Kind == ReplyKind.Friends)
                eatingFriends = true;
            if (r.Kind == ReplyKind.StatusOn)
                pageFollowsUntil = now + 2;
            return true;
        }

        private static Question? QuestionOf(ReplyKind k)
        {
            switch (k)
            {
                case ReplyKind.StatusOn:
                case ReplyKind.StatusOff:
                    return Question.Status;
                case ReplyKind.Friends:
                case ReplyKind.NoFriends:
                    return Question.Friends;
                case ReplyKind.Channels:
                case ReplyKind.NoChannels:
                    return Question.Channels;
                case ReplyKind.Members:
                case ReplyKind.ChannelRefused:
                    return Question.Members;
                default:
                    return null;
            }
        }

        private void Apply(Reply r, double now)
        {
            var s = State;
            switch (r.Kind)
            {
                case ReplyKind.StatusOn:
                    s.On = true;
                    s.Address = r.Address;
                    s.Hub = r.Hub;
                    s.FriendsCount = r.Count;
                    s.Linked = r.IsLinked;
                    break;
                case ReplyKind.StatusOff:
                    s.On = false;
                    s.Address = r.Address;
                    s.Hub = "";
                    break;
                case ReplyKind.Joined:
                case ReplyKind.AlreadyOn:
                    s.On = true;
                    s.Address = r.Address;
                    break;
                case ReplyKind.Left:
                case ReplyKind.AlreadyOff:
                    s.On = false;
                    s.Hub = "";
                    break;
                case ReplyKind.WorldOff:
                    s.On = false;
                    s.WorldOff = true;
                    break;
                case ReplyKind.NotOnNetwork:
                    s.On = false;
                    s.Note = r.Line;
                    break;
                case ReplyKind.CannotJoin:
                case ReplyKind.NotDelivered:
                case ReplyKind.GlobalNotCarried:
                    s.Note = r.Line;
                    break;
                case ReplyKind.DirectoryPage:
                    s.DirectoryPage = r.Url;
                    break;
                case ReplyKind.Friends:
                    s.Friends.Clear();
                    foreach (var f in r.Friends)
                        s.Friends[f.Address] = f.Online;
                    break;
                case ReplyKind.FriendRow:
                    foreach (var f in r.Friends)
                        s.Friends[f.Address] = f.Online;
                    break;
                case ReplyKind.NoFriends:
                    s.Friends.Clear();
                    break;
                case ReplyKind.FriendOnline:
                    s.Friends[r.Address] = true;
                    break;
                case ReplyKind.FriendAdded:
                    // Shown offline until its world says otherwise.
                    if (!s.Friends.ContainsKey(r.Address))
                        s.Friends[r.Address] = false;
                    break;
                case ReplyKind.FriendOffline:
                    s.Friends[r.Address] = false;
                    break;
                case ReplyKind.FriendRemoved:
                    s.Friends.Remove(r.Address);
                    if (s.SelectedFriend == r.Address)
                        s.SelectedFriend = "";
                    break;
                case ReplyKind.Channels:
                    s.Channels.Clear();
                    s.Channels.AddRange(r.Items);
                    break;
                case ReplyKind.NoChannels:
                    s.Channels.Clear();
                    break;
                case ReplyKind.Members:
                    s.Members[r.Address] = new List<string>(r.Items);
                    break;
                case ReplyKind.ChannelMade:
                case ReplyKind.ChannelJoined:
                    if (!s.Channels.Contains(r.Address))
                        s.Channels.Add(r.Address);
                    s.Invites.RemoveAll(x => x.Channel == r.Address);
                    s.ChannelNote = r.Line;
                    break;
                case ReplyKind.ChannelLeft:
                    s.Channels.Remove(r.Address);
                    s.Members.Remove(r.Address);
                    if (s.SelectedChannel == r.Address)
                        s.SelectedChannel = "";
                    s.ChannelNote = r.Line;
                    break;
                case ReplyKind.ChannelInvited:
                case ReplyKind.ChannelRefused:
                    s.ChannelNote = r.Line;
                    break;
                case ReplyKind.ChannelInvite:
                    if (!s.Invites.Exists(x => x.Channel == r.Address))
                        s.Invites.Add(new Invitation(r.Address, r.Speaker));
                    break;
                case ReplyKind.ChannelLine:
                    Keep(s.ChannelLines, new[] { r.Address, r.Speaker, r.Text });
                    break;
                case ReplyKind.LinkOffered:
                    s.LinkUrl = r.Url;
                    s.LinkCode = r.Code;
                    s.LinkLasts = r.Lasts;
                    break;
                case ReplyKind.Linked:
                    s.Linked = true;
                    s.LinkUrl = "";
                    s.LinkCode = "";
                    break;
                case ReplyKind.Unlinked:
                case ReplyKind.NotLinked:
                    s.Linked = false;
                    break;
                case ReplyKind.LinkPaused:
                case ReplyKind.LinkRefused:
                    s.LinkNote = r.Line;
                    break;
                case ReplyKind.WorldSquelched:
                case ReplyKind.WorldUnsquelched:
                    s.Note = r.Line;
                    break;
                case ReplyKind.GlobalLine:
                    Keep(s.GlobalLines, new[] { r.Speaker + (r.ViaDiscord ? " (Discord)" : ""), r.Text });
                    break;
            }
            if (s.WorldSlug.Length > 0 && s.WorldSlug != policyFor)
                FetchPolicy(s.WorldSlug);
        }

        private static void Keep(List<string[]> lines, string[] line)
        {
            lines.Add(line);
            if (lines.Count > MaxLines)
                lines.RemoveRange(0, lines.Count - MaxLines);
        }

        private void FetchWorlds()
        {
            worldsLoading = true;
            worldsAt = clock();
            hub.Worlds(r =>
            {
                worldsLoading = false;
                worldsAt = clock();
                if (r.Value != null)
                {
                    State.Worlds = r.Value;
                    State.WorldsNote = "";
                }
                else
                {
                    State.WorldsNote = r.Error;
                }
                Render();
            });
        }

        private void Search()
        {
            State.SearchNote = "Searching...";
            hub.Search(view.GetText(Controls.SearchText), r =>
            {
                if (r.Value != null)
                {
                    State.Results = r.Value;
                    State.SearchNote = r.Value.Count == 0 ? "Nobody listed by that name." : "";
                }
                else
                {
                    State.SearchNote = r.Error;
                }
                Render();
            });
        }

        private void FetchPolicy(string slug)
        {
            policyFor = slug;
            hub.World(slug, r =>
            {
                if (r.Value == null || r.Value.Policy.Length == 0)
                {
                    State.Policy = null;
                    State.PolicyNote = r.Value == null ? "" : "This world publishes no rules for plugins.";
                    Render();
                    return;
                }
                var published = r.Value.Policy.Trim();
                if (published.StartsWith("https://", StringComparison.Ordinal) && published.IndexOf('\n') < 0)
                    hub.Text(published, t => TakePolicy(t.Value, t.Error));
                else
                    TakePolicy(published, "");
            });
        }

        private void TakePolicy(string? text, string error)
        {
            if (text == null)
            {
                State.PolicyNote = "This world's rules for plugins could not be read: " + error;
            }
            else
            {
                var p = PluginPolicy.Parse(text, out var why);
                State.Policy = p;
                State.PolicyNote = p == null
                    ? "This world's rules for plugins did not read: " + why + "."
                    : "This world's rules for plugins: " + p.Summary() + (p.Notes.Length > 0 ? " " + p.Notes : "") +
                      (p.NotesUrl.Length > 0 ? " " + p.NotesUrl : "");
            }
            Render();
        }

        private void Render()
        {
            var s = State;
            view.SetText(Controls.StatusLine, s.StatusText());
            view.SetText(Controls.StatusNote, s.Note);
            view.SetText(Controls.PageLine, s.DirectoryPage.Length > 0 ? "Your directory page: " + s.DirectoryPage : "");
            view.SetText(Controls.LinkUrl, s.LinkUrl);
            view.SetText(Controls.LinkNote, s.LinkText());
            view.SetText(Controls.PolicyNote, s.PolicyNote);

            var friends = new List<string[]>();
            foreach (var f in s.FriendList())
            {
                NetAddress.TryParse(f.Address, out var name, out var world);
                friends.Add(new[] { name, world, f.Online ? "online" : "offline" });
            }
            view.SetRows(Controls.Friends, friends);

            var channels = new List<string[]>();
            foreach (var c in s.Channels)
                channels.Add(new[] { c });
            view.SetRows(Controls.Channels, channels);
            var members = new List<string[]>();
            if (s.SelectedChannel.Length > 0 && s.Members.TryGetValue(s.SelectedChannel, out var m))
                foreach (var who in m)
                    members.Add(new[] { who });
            view.SetRows(Controls.Members, members);
            var invites = new List<string[]>();
            foreach (var inv in s.Invites)
                invites.Add(new[] { inv.Channel, inv.From });
            view.SetRows(Controls.Invites, invites);
            view.SetRows(Controls.ChannelLines, s.ChannelLines);
            view.SetText(Controls.ChannelsNote, s.ChannelNote);

            view.SetRows(Controls.GlobalLines, s.GlobalLines);

            var results = new List<string[]>();
            foreach (var r in s.Results)
                results.Add(new[]
                {
                    r.Address,
                    r.Level.HasValue ? r.Level.Value.ToString(CultureInfo.InvariantCulture) : "",
                    r.WorldName,
                });
            view.SetRows(Controls.Results, results);
            view.SetText(Controls.SearchNote, s.SearchNote);

            var worlds = new List<string[]>();
            foreach (var w in s.Worlds)
                worlds.Add(new[]
                {
                    w.Slug == s.WorldSlug ? w.Name + " (here)" : w.Name,
                    w.State,
                    w.Players.HasValue ? w.Players.Value.ToString(CultureInfo.InvariantCulture) : "",
                    w.Federation == "on" ? "yes" : "",
                });
            view.SetRows(Controls.Worlds, worlds);
            view.SetText(Controls.WorldsNote, s.WorldsNote);
        }
    }

    /// <summary>An invitation to a channel.</summary>
    public sealed class Invitation
    {
        public Invitation(string channel, string from)
        {
            Channel = channel;
            From = from;
        }

        public string Channel { get; }
        public string From { get; }
    }

    /// <summary>What the companion knows of the character on the network.</summary>
    public sealed class CompanionState
    {
        /// <summary>Whether the character is on the network; null until the world has said.</summary>
        public bool? On { get; set; }
        public bool WorldOff { get; set; }
        /// <summary>The character's address, <c>Name@world</c>, once the world has said it.</summary>
        public string Address { get; set; } = "";
        public string Hub { get; set; } = "";
        public int FriendsCount { get; set; }
        public bool Linked { get; set; }
        public string DirectoryPage { get; set; } = "";
        public string Note { get; set; } = "";
        public SortedDictionary<string, bool> Friends { get; } = new SortedDictionary<string, bool>(StringComparer.OrdinalIgnoreCase);
        public string SelectedFriend { get; set; } = "";
        public List<string> Channels { get; } = new List<string>();
        public Dictionary<string, List<string>> Members { get; } = new Dictionary<string, List<string>>(StringComparer.Ordinal);
        public string SelectedChannel { get; set; } = "";
        public List<Invitation> Invites { get; } = new List<Invitation>();
        public string SelectedInvite { get; set; } = "";
        public List<string[]> ChannelLines { get; } = new List<string[]>();
        public string ChannelNote { get; set; } = "";
        public List<string[]> GlobalLines { get; } = new List<string[]>();
        public string LinkUrl { get; set; } = "";
        public string LinkCode { get; set; } = "";
        public string LinkLasts { get; set; } = "";
        public string LinkNote { get; set; } = "";
        public List<WorldRow> Worlds { get; set; } = new List<WorldRow>();
        public string WorldsNote { get; set; } = "";
        public List<DirectoryRow> Results { get; set; } = new List<DirectoryRow>();
        public string SearchNote { get; set; } = "";
        public PluginPolicy? Policy { get; set; }
        public string PolicyNote { get; set; } = "";

        /// <summary>The world's slug, from the character's address.</summary>
        public string WorldSlug => NetAddress.TryParse(Address, out _, out var w) ? w : "";

        /// <summary>The friends in the order the list shows them: online first, then by address.</summary>
        public List<FriendRow> FriendList()
        {
            var list = new List<FriendRow>();
            foreach (var kv in Friends)
                if (kv.Value)
                    list.Add(new FriendRow(kv.Key, true));
            foreach (var kv in Friends)
                if (!kv.Value)
                    list.Add(new FriendRow(kv.Key, false));
            return list;
        }

        /// <summary>The status line the window shows.</summary>
        public string StatusText()
        {
            if (WorldOff)
                return "This world is not on the Dereth Network.";
            if (On == null)
                return "Not asked yet.";
            if (On == false)
                return Address.Length > 0 ? "Off the network. On, you would be " + Address + "." : "Off the network.";
            var hub = Hub.Length > 0 ? " The hub is " + Hub + "." : "";
            return "On the network as " + Address + "." + hub + (Linked ? " Linked to Discord." : "");
        }

        /// <summary>The link line the window shows.</summary>
        public string LinkText()
        {
            if (LinkUrl.Length > 0)
                return (LinkCode.Length > 0 ? "Code " + LinkCode + ". " : "") + "Open the link within " + LinkLasts + ".";
            if (LinkNote.Length > 0)
                return LinkNote;
            return Linked ? "Linked to a Discord account." : "";
        }
    }
}
