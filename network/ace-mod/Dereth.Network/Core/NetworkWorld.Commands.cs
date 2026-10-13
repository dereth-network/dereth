// What `/dn` (and `/derethnetwork`, the same command spelled out) does, and the channel requests
// it makes.
//
// `/dn` is every player's: putting a character on the network and taking it off, its remote
// friends, linking it to its player's Discord account, squelching a whole world, and channels.
// `/dn admin` is the operator's: blocking a world or one remote character for everyone on this
// world, and where the world carries the Global room.
//
// Help is one line per command, `@dn on - put this character on the Dereth Network`; a command
// given wrongly is answered with its own line.
//
// A name or address a command takes may be given with the mark a line said from Discord shows beside
// its speaker (`Ann@coldeve (Discord)`), as a player copies it: the mark is taken off.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>What `/dn` says when the world is not on the network.</summary>
    public const string NotOnNetworkLine = "This world is not on the Dereth Network.";

    /// <summary>What a character off the network is told when it asks the network for something.</summary>
    public const string NotOnLine = "You are not on the Dereth Network. Type @dn on to join it.";

    /// <summary>
    /// The player's commands, each with what it does: `/dn help` shows one line for each,
    /// `@dn on - put this character on the Dereth Network`.
    /// </summary>
    public static readonly IReadOnlyList<(string Command, string Does)> Commands =
    [
        ("@dn", "show your Dereth Network status"),
        ("@dn on", "put this character on the Dereth Network"),
        ("@dn off", "take this character off the Dereth Network"),
        ("@dn friends", "list your friends on other worlds"),
        ("@dn friends add <Name@world>", "add a friend on another world"),
        ("@dn friends remove <Name@world>", "remove a friend on another world"),
        ("@dng <text>", "say a line in the Global room"),
        ("@dn global <text>", "say a line in the Global room"),
        ("@dn squelch <world>", "squelch every character of a world"),
        ("@dn unsquelch <world>", "unsquelch a world"),
        ("@dn channel create <channel>", "make a channel"),
        ("@dn channel invite <channel> <Name@world>", "invite a character to a channel"),
        ("@dn channel join <channel>", "join a channel you were invited to"),
        ("@dn channel leave <channel>", "leave a channel"),
        ("@dn channel members <channel>", "list a channel's members"),
        ("@dn channel list", "list your channels"),
        ("@dn link", "get a link that links this character to your Discord account"),
        ("@dn unlink", "unlink this character from your Discord account"),
        ("@dn help", "show these commands"),
        ("@tell <Name@world>, <text>", "tell a character on another world"),
        ("@tell <channel>@channel, <text>", "say a line in a channel"),
    ];

    /// <summary>The operator's commands, as <see cref="Commands"/> (`@dn` replaced by `dn` at the console).</summary>
    public static readonly IReadOnlyList<(string Command, string Does)> AdminCommands =
    [
        ("@dn admin block <world or Name@world>", "block a world or a character for everyone on this world"),
        ("@dn admin unblock <world or Name@world>", "lift a block"),
        ("@dn admin block", "show the blocks and the network's state"),
        ("@dn admin global <general|trade|lfg|roleplay|off>", "carry the Global room in another of this world's rooms"),
        ("@dn admin global", "show where this world carries the Global room"),
        ("@dn verify <token>", "verify this world with the token the site gave you"),
    ];

    /// <summary>One command's help line: `@dn on - put this character on the Dereth Network`.</summary>
    private static string HelpLine(string command, string does) => $"{command} - {does}";

    /// <summary>`/dn help`: one line for each of the player's commands.</summary>
    public static string Help() => string.Join("\n", Commands.Select(c => HelpLine(c.Command, c.Does)));

    /// <summary>The operator's help, one line for each of its commands, as typed in game (`@dn`) or at the console (`dn`).</summary>
    public static string AdminHelp(string prefix) =>
        string.Join("\n", AdminCommands.Select(c => HelpLine(prefix + c.Command[3..], c.Does)));

    /// <summary>The help line of the command that starts `command`, for a command given wrongly.</summary>
    public static string Usage(string command) =>
        Commands.Concat(AdminCommands).FirstOrDefault(c => c.Command.StartsWith(command, StringComparison.Ordinal)) is
        { Command: not null } found
            ? HelpLine(found.Command, found.Does)
            : Help();

    /// <summary>`n` of `thing`, in words: "1 friend", "3 friends".</summary>
    private static string Count(int n, string thing) => n == 1 ? $"1 {thing}" : $"{n} {thing}s";

    /// <summary>`/dn &lt;args&gt;` for `player`.</summary>
    public void Dn(uint player, IReadOnlyList<string> args)
    {
        lock (gate)
        {
            var w = args;
            var first = w.Count > 0 ? w[0] : "";
            switch (w.Count)
            {
                case 0:
                case 1 when first == "status":
                    Status(player);
                    return;
                case 1 when first == "help":
                    host.ShowSystem(player, Help());
                    return;
                case 1 when first == "on":
                    NetOn(player);
                    return;
                case 1 when first == "off":
                    NetOff(player);
                    return;
                case 1 when first == "friends":
                    Friends(player);
                    return;
                case 1 when first == "link":
                    LinkCharacter(player);
                    return;
                case 1 when first == "unlink":
                    UnlinkCharacter(player);
                    return;
                case 2 when first == "friends" && w[1] is "add" or "remove":
                    host.ShowSystem(player, Usage($"@dn friends {w[1]}"));
                    return;
                case 2 when first is "squelch" or "unsquelch":
                    SquelchWorld(player, first, w[1]);
                    return;
            }
            if (w.Count >= 3 && first == "friends" && w[1] == "add")
            {
                // What `@friends add Name@world` does.
                if (!AddFriend(player, string.Join(" ", w.Skip(2))))
                    host.ShowSystem(player, Usage("@dn friends add"));
                return;
            }
            if (w.Count >= 3 && first == "friends" && w[1] == "remove")
            {
                // What removing the friend's row in the friends panel does.
                Address? listed = null;
                if (Address.TryParse(GivenName(string.Join(" ", w.Skip(2))), out var a, out _)
                    && Record(player)?.Friends.TryGetValue(a.Key, out var f) == true)
                    listed = f;
                if (listed is not null && StandIn(listed) is uint id)
                    RemoveFriend(player, id);
                else
                    host.ShowSystem(player, "That character is not in your friends list!");
                return;
            }
            if (w.Count >= 1 && first == "channel")
            {
                Channel(player, w.Skip(1).ToList());
                return;
            }
            if (w.Count >= 1 && first == "global")
            {
                Global(player, string.Join(" ", w.Skip(1)));
                return;
            }
            host.ShowSystem(player, Help());
        }
    }

    /// <summary>
    /// `/dn on`: the character goes on the network as `Name@thisworld`, seen online by anyone, told by
    /// anyone and listed in the directory.
    /// </summary>
    private void NetOn(uint player)
    {
        var name = host.OnlineName(player) ?? "";
        if (name.Contains(Address.Separator) || OwnAddress(player) is not { } address)
        {
            host.ShowSystem(player, "This character's name cannot be a network address, so it cannot join the Dereth Network.");
            return;
        }
        if (IsOn(player))
        {
            host.ShowSystem(player, $"You are on the Dereth Network already, as {address}.");
            return;
        }
        RecordMut(player).Settings = CharacterSettings.OnNetwork;
        dirty = true;
        host.ShowSystem(player,
            $"You are on the Dereth Network as {address}. Anyone on the network can see when you are online, send you tells and find your profile in its directory. Type @dn help for the commands.");
        host.ShowSystem(player,
            "Your profile shows your level, skills, allegiance and more, and changes as you play. Type @dn off to leave the network.");
        DirectoryAnnounce(player);
        OnLoginLocked(player);
        // Back on the network: its followers' links to it appear.
        if (IsListed(player))
            LinksChanged(player);
    }

    /// <summary>What a linked character going off the network is told: its link pauses, and `/dn on` resumes it with nothing to redo.</summary>
    public const string PausedLine = "This character's link to your Discord account is paused until you type @dn on.";

    /// <summary>`/dn off`: the character leaves the network, and its profile the directory, at once. A linked one's link pauses.</summary>
    private void NetOff(uint player)
    {
        if (!IsOn(player))
        {
            host.ShowSystem(player, "You are not on the Dereth Network.");
            return;
        }
        var linked = persons.TryGetValue(player, out var state) && state.Linked;
        GoOff(player);
        host.ShowSystem(player,
            "You are off the Dereth Network. Nobody on another world can see you or tell you, and your profile is out of its directory.");
        if (linked)
            host.ShowSystem(player, PausedLine);
    }

    /// <summary>
    /// `player`'s character goes off the network, as `/dn off` takes it: its watchers see it offline,
    /// the rows its person friends listed leave its friends panel, and it is out of the directory at
    /// once.
    /// </summary>
    private void GoOff(uint player)
    {
        OnLogoutLocked(player, host.OnlineName(player), clientStays: true);
        RecordMut(player).Settings = CharacterSettings.Off;
        dirty = true;
        DirectoryWentOff(player);
    }

    private void OnLoginLocked(uint player)
    {
        ToldAtLogin(player);
        if (!IsOn(player))
            return;
        DirectoryArrived(player);
        TellWatchers(player, null);
        if (OwnAddress(player) is not { } owner)
            return;
        foreach (var f in Record(player)!.Friends.Values.ToList())
            SendIfUp(new Watch(owner, f, true));
        WatchPerson(player, true);
    }

    private void OnLogoutLocked(uint player, string? name, bool clientStays = false)
    {
        foreach (var k in presence.Keys.Where(k => k.Item1 == player).ToList())
            presence.Remove(k);
        if (!IsOn(player))
            return;
        TellWatchers(player, false, name);
        if (OwnAddress(player, name) is not { } owner)
            return;
        foreach (var f in Record(player)!.Friends.Values.ToList())
            SendIfUp(new Watch(owner, f, false));
        PersonLeft(player, name, clientStays);
    }

    /// <summary>The hub's state, as the status lines say it.</summary>
    private string LinkWord => link switch
    {
        LinkState.Up => "connected",
        LinkState.Greeting => "connecting",
        _ => "not connected",
    };

    /// <summary>
    /// `/dn`: whether the character is on the network, as what, the hub, its friends on other worlds,
    /// whether it is linked, and its page in the directory once the hub has named one.
    /// </summary>
    private void Status(uint player)
    {
        var rec = Record(player);
        var who = OwnAddress(player)?.ToString() ?? "";
        if (rec is not { Settings.On: true })
        {
            host.ShowSystem(player, $"You are not on the Dereth Network. Type @dn on to join it as {who}.");
            return;
        }
        var friends = rec.Friends.Count == 0
            ? "You have no friends on other worlds."
            : $"You have {Count(rec.Friends.Count, "friend")} on other worlds.";
        host.ShowSystem(player, $"You are on the Dereth Network as {who}. The hub is {LinkWord}. {friends}{LinkedNote(player)}");
        if (rec.Listing?.Page is { } page)
            host.ShowSystem(player, $"Your directory page: {page}");
    }

    private void Friends(uint player)
    {
        var rows = FriendRows(player);
        if (rows.Count == 0)
        {
            host.ShowSystem(player, "You have no friends on other worlds. Add one with @dn friends add Name@world.");
            return;
        }
        host.ShowSystem(player, "Your friends on other worlds:\n" +
            string.Join("\n", rows.Select(x => $"{x.Name}: {(x.Online ? "online" : "offline")}")));
    }

    /// <summary>`/dn squelch &lt;world&gt;` and `/dn unsquelch &lt;world&gt;`.</summary>
    private void SquelchWorld(uint player, string word, string world)
    {
        var add = word == "squelch";
        world = Text.AsciiLower(world);
        if (!Address.IsSlug(world) || world == Config.Slug)
        {
            host.ShowSystem(player, Usage($"@dn {word}"));
            return;
        }
        var held = Record(player);
        var already = held?.SquelchedWorlds.Contains(world) == true;
        if (add && !already && held?.SquelchedWorlds.Count >= MaxSquelchedWorlds)
        {
            host.ShowSystem(player, $"You squelch {MaxSquelchedWorlds} worlds already. Unsquelch one first.");
            return;
        }
        var changed = add != already && (add ? RecordMut(player).SquelchedWorlds.Add(world) : RecordMut(player).SquelchedWorlds.Remove(world));
        dirty |= changed;
        host.ShowSystem(player, (add, changed) switch
        {
            (true, true) => $"Every character of {world} has been squelched.",
            (true, false) => $"{world} is already squelched.",
            (false, true) => $"{world} has been unsquelched.",
            _ => $"{world} is not squelched.",
        });
    }

    private void Channel(uint player, List<string> words)
    {
        if (!IsOn(player))
        {
            host.ShowSystem(player, NotOnLine);
            return;
        }
        ChannelOp op;
        string name;
        Address? who = null;
        switch (words.Count)
        {
            case 2 when words[0] == "create":
                (op, name) = (ChannelOp.Create, words[1]);
                break;
            case 2 when words[0] == "join":
                (op, name) = (ChannelOp.Join, words[1]);
                break;
            case 2 when words[0] == "leave":
                (op, name) = (ChannelOp.Leave, words[1]);
                break;
            case 2 when words[0] == "members":
                (op, name) = (ChannelOp.Members, words[1]);
                break;
            case 1 when words[0] == "list":
                (op, name) = (ChannelOp.List, "list");
                break;
            case >= 3 when words[0] == "invite":
                if (!Address.TryParse(GivenName(string.Join(" ", words.Skip(2))), out var a, out _) || a.IsChannel)
                {
                    host.ShowSystem(player, Usage("@dn channel invite"));
                    return;
                }
                (op, name, who) = (ChannelOp.Invite, words[1], a);
                break;
            case >= 1 when words[0] is "create" or "join" or "leave" or "members" or "invite":
                host.ShowSystem(player, Usage($"@dn channel {words[0]}"));
                return;
            default:
                host.ShowSystem(player, Help());
                return;
        }
        var n = name;
        while (n.EndsWith("@channel", StringComparison.Ordinal))
            n = n[..^"@channel".Length];
        n = Text.AsciiLower(n);
        if (!Address.IsChannelName(n))
        {
            host.ShowSystem(player, "A channel's name is 2 to 20 letters, digits and hyphens.");
            return;
        }
        // A channel request carries text to other worlds' players (an invitation, a channel's name): a
        // gagged or too new character may not make one, and each counts against the hub's pace.
        if (host.IsGagged(player))
        {
            host.ShowGagged(player);
            return;
        }
        if (TooNew(player))
        {
            host.ShowSystem(player, TooNewLine);
            return;
        }
        if (!MaySend(player, Sends.ChannelRequests))
        {
            host.ShowSystem(player, TooQuicklyLine);
            return;
        }
        Request(player, op, n, who);
    }

    private void Request(uint player, ChannelOp op, string name, Address? who)
    {
        if (OwnAddress(player) is not { } by)
            return;
        var id = FreshId('c');
        pendingChannels[id] = new PendingChannel(player, op, who, null);
        Send(new ChannelRequest(id, op, name, by, who));
    }

    /// <summary>Leaves a channel (a squelch of the channel's address).</summary>
    private void LeaveChannel(uint player, string name) => Request(player, ChannelOp.Leave, name, null);

    /// <summary>The line a channel request's answer shows.</summary>
    public static string ChannelLine(ChannelOp op, ChannelOutcome outcome, string channel, Address? who,
        IReadOnlyList<Address> members, IReadOnlyList<string> channels)
    {
        var address = channel + "@channel";
        return (outcome, op) switch
        {
            (ChannelOutcome.Ok, ChannelOp.List) when channels.Count == 0 => "You are in no channel.",
            (ChannelOutcome.Ok, ChannelOp.List) => $"Your channels: {string.Join(", ", channels.Select(c => c + "@channel"))}.",
            (ChannelOutcome.Ok, ChannelOp.Members) => $"Members of {address}: {string.Join(", ", members)}.",
            (ChannelOutcome.Ok, ChannelOp.Create) =>
                $"You have made the channel {address}. Invite someone with @dn channel invite {channel} Name@world.",
            (ChannelOutcome.Ok, ChannelOp.Invite) => $"{who?.ToString() ?? "That character"} is invited to {address}.",
            (ChannelOutcome.Ok, ChannelOp.Join) => $"You have joined {address}. Talk in it with @tell {address}, text.",
            (ChannelOutcome.Ok, _) => $"You have left {address}.",
            (ChannelOutcome.Exists, _) => $"{address} already exists.",
            (ChannelOutcome.NoChannel, _) => $"There is no channel {address}.",
            (ChannelOutcome.NotMember, _) => $"You are not a member of {address}.",
            (ChannelOutcome.NotInvited, _) => $"You have not been invited to {address}.",
            (ChannelOutcome.AlreadyMember, _) => $"That character is already a member of {address}.",
            _ => $"The Dereth Network refused that for {address}.",
        };
    }

    /// <summary>The answer to a channel request.</summary>
    private void OnChannelResult(string id, ChannelOutcome outcome, string channel, IReadOnlyList<Address> members,
        IReadOnlyList<string> channels)
    {
        if (!pendingChannels.Remove(id, out var p))
            return;
        if (!Online(p.Player))
            return;
        host.ShowSystem(p.Player, ChannelLine(p.Op, outcome, channel, p.Who, members, channels));
    }

    /// <summary>A channel request that will not be answered.</summary>
    private void GiveUpChannel(string id)
    {
        if (pendingChannels.Remove(id, out var p) && Online(p.Player))
            host.ShowSystem(p.Player, "The Dereth Network is unavailable.");
    }

    /// <summary>
    /// An invitation for one of this world's characters. It is shown as a tell from the inviter would
    /// be: never past a squelch of the inviter, of its world or of the channel, or the operator's
    /// blocks, and held to the pace a tell from the inviter's world to that character is.
    /// </summary>
    private void OnChannelInvite(string channel, Address by, Address to)
    {
        if (to.World != Config.Slug)
            return;
        if (host.OnlineByName(to.Name) is not uint target)
            return;
        var channelAddress = Address.Create(channel, Address.ChannelWorld);
        if (!IsOn(target) || Blocked(by) || Squelches(target, by)
            || channelAddress is null || Squelches(target, channelAddress))
            return;
        var byName = TextRules.SanitizeName(by.ToString());
        if (byName is null)
            return;
        if (!MayReceive(by.World, to.Key, limits.TellsPerRecipient))
            return;
        host.ShowSystem(target, $"{byName} invites you to the channel {channel}@channel. Type @dn channel join {channel} to join it.");
    }

    /// <summary>
    /// Sends a `/dn verify` to the hub and returns how it went, off the world thread (the host sets
    /// it: <see cref="HubVerify.SendAsync"/> with the world's key). None leaves `/dn verify` saying
    /// the network cannot verify.
    /// </summary>
    public Func<VerifyRequest, Task<string>>? Verifier { get; set; }

    /// <summary>
    /// Posts the nonce of a challenge the hub sent the game port (as hex) back to the hub, off the
    /// world thread, and says how it went, for the log (the host sets it:
    /// <see cref="HubVerify.AnswerAsync"/> with the world's key, once it can hear the game port).
    /// None leaves `/dn verify` saying this server cannot be verified in game.
    /// </summary>
    public Func<string, Task<string>>? Answerer { get; set; }

    /// <summary>What `/dn verify` says when the server cannot hear the hub's challenge on its game port.</summary>
    public const string CannotHearLine =
        "This server cannot hear the hub's challenge on its game port, so this world cannot be verified in game. Verify it with a page on its host instead.";

    private readonly System.Collections.Concurrent.ConcurrentQueue<(uint? Player, string Line)> verifyAnswers = new();
    private int verifying;

    /// <summary>Guards the challenge state, which the game port's receive thread reads.</summary>
    private readonly object challengeGate = new();

    /// <summary>When the `/dn verify` waiting for the hub's answer was typed; null when none waits.</summary>
    private double? verifyAskedAt;

    /// <summary>The nonces answered for the verification that waits (as hex).</summary>
    private readonly List<string> answeredNonces = [];

    /// <summary>
    /// `/dn verify &lt;token&gt;` by `player` (the console when none), named `by`: the token goes to
    /// the hub off the world thread, and its answer is shown on a later pump. The line returned is
    /// the immediate one. While the hub's answer is awaited, for <see cref="Challenge.WindowSecs"/>,
    /// the world answers the hub's challenge on its game port (<see cref="OnChallengeDatagram"/>).
    /// </summary>
    public string Verify(uint? player, string by, IReadOnlyList<string> args)
    {
        if (args.Count != 1 || !HubVerify.IsToken(args[0].Trim()))
        {
            var line = Usage("@dn verify");
            return player is null ? "dn" + line[3..] : line;
        }
        var verifier = Verifier;
        if (verifier is null)
            return "This world cannot reach the hub to verify.";
        if (Answerer is null)
            return CannotHearLine;
        if (Interlocked.CompareExchange(ref verifying, 1, 0) != 0)
            return "A verification is on its way to the hub already.";
        lock (challengeGate)
        {
            verifyAskedAt = Now;
            answeredNonces.Clear();
        }
        var request = new VerifyRequest(args[0].Trim(), new string(by.TrimStart('+').Take(32).ToArray()), player);
        _ = Task.Run(async () =>
        {
            string line;
            try
            {
                line = await verifier(request).ConfigureAwait(false);
            }
            catch (Exception e)
            {
                line = $"The verification failed ({e.GetType().Name}).";
            }
            verifyAnswers.Enqueue((request.Player, line));
        });
        return "Asking the Dereth Network's hub to verify this world...";
    }

    /// <summary>
    /// A datagram the game port received, from any thread: whether it is a challenge (the server's
    /// packet reader then skips it). A challenge for this world, while a `/dn verify` it sent waits
    /// for the hub's answer and within <see cref="Challenge.WindowSecs"/> of being typed, has its
    /// nonce posted back to the hub over HTTPS, once for each nonce and for at most
    /// <see cref="Challenge.MaxAnswers"/> of them; any other is ignored. Nothing is ever sent back
    /// over UDP.
    /// </summary>
    public bool OnChallengeDatagram(ReadOnlySpan<byte> datagram)
    {
        if (!Challenge.IsChallenge(datagram))
            return false;
        var c = Challenge.TryParse(datagram);
        var answerer = Answerer;
        if (c is null || answerer is null || !string.Equals(c.Slug, Config.Slug, StringComparison.Ordinal))
            return true;
        var hex = Challenge.NonceHex(c.Nonce);
        lock (challengeGate)
        {
            var t = Now;
            if (verifyAskedAt is not double asked || t < asked || t - asked > Challenge.WindowSecs)
                return true;
            if (answeredNonces.Contains(hex) || answeredNonces.Count >= Challenge.MaxAnswers)
                return true;
            answeredNonces.Add(hex);
        }
        _ = Task.Run(async () =>
        {
            string line;
            try
            {
                line = await answerer(hex).ConfigureAwait(false);
            }
            catch (Exception e)
            {
                line = $"the answer to the hub's challenge failed ({e.GetType().Name})";
            }
            log(LogLevel.Info, line);
        });
        return true;
    }

    /// <summary>On the world thread: each answer that has come is shown to whoever asked (the log, for the console).</summary>
    private void ShowVerifyAnswers()
    {
        while (verifyAnswers.TryDequeue(out var a))
        {
            lock (challengeGate)
            {
                verifyAskedAt = null;
                answeredNonces.Clear();
            }
            Volatile.Write(ref verifying, 0);
            if (a.Player is uint p && host.OnlineName(p) is not null)
                host.ShowSystem(p, a.Line);
            else
                log(LogLevel.Info, a.Line);
        }
    }

    /// <summary>
    /// What the operator's bare `/dn admin block` and `/dn admin unblock` show: the network's state on
    /// this world, then what is blocked.
    /// </summary>
    private string AdminStatus()
    {
        var mirrored = MirroredSlots;
        var online = worlds.Values.Count(i => i.Online);
        var queued = outbox.Count + answers.Values.Sum(q => q.Count) + chatter.Count;
        var blocked = store.Blocked.Count == 0 ? "Nothing is blocked." : $"Blocked: {string.Join(", ", store.Blocked)}.";
        return $"This world is on the Dereth Network as {Config.Slug}. The hub is {LinkWord}. " +
            $"{(online == 1 ? "1 world is" : $"{online} worlds are")} online. " +
            $"{(queued == 1 ? "1 message is" : $"{queued} messages are")} waiting for the hub. " +
            $"The Global room is carried in {Shown(GlobalSlot)}. " +
            $"Rooms mirrored into Discord: {(mirrored.Count == 0 ? "none" : string.Join(", ", mirrored.Select(RoomSlots.Name)))}.\n{blocked}";
    }

    /// <summary>
    /// `/dn admin &lt;args&gt;`, typed by `player` (the console when null): the operator's blocks, and
    /// where the world carries the Global room, whose answer from the hub goes to `player`. The line
    /// returned is the immediate one. `/dn admin block` or `/dn admin unblock` without a target shows
    /// the network's state and the blocks; any other word shows the operator's help.
    /// </summary>
    public string Admin(IReadOnlyList<string> args, uint? player = null)
    {
        lock (gate)
        {
            var prefix = player is null ? "dn" : "@dn";
            var first = args.Count > 0 ? args[0] : "";
            if (first == "global")
                return AdminGlobal(player, args.Skip(1).ToList());
            if (first is not ("block" or "unblock"))
                return AdminHelp(prefix);
            if (args.Count == 1)
                return AdminStatus();
            var who = GivenName(string.Join(" ", args.Skip(1)));
            string key;
            if (Address.TryParse(who, out var a, out _))
                key = a.Key;
            else if (Address.IsSlug(Text.AsciiLower(who)))
                key = Text.AsciiLower(who);
            else
                return prefix + Usage($"@dn admin {first} <")[3..];
            var add = first == "block";
            bool changed;
            if (add)
            {
                // What the block covers is told this world's characters are offline first.
                if (!store.Blocked.Contains(key))
                    BlockBeginning(key);
                changed = store.Blocked.Add(key);
            }
            else
            {
                changed = store.Blocked.Remove(key);
                if (changed)
                    BlockEnded(key);
            }
            dirty |= changed;
            return (add, changed) switch
            {
                (true, true) => $"{key} is blocked for everyone on this world.",
                (true, false) => $"{key} was already blocked.",
                (false, true) => $"{key} is no longer blocked.",
                _ => $"{key} was not blocked.",
            };
        }
    }
}
