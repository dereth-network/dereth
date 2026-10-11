// What `/dn` (and `/derethnetwork`, the same command spelled out) does, and the channel requests
// it makes.
//
// `/dn` is every player's: opting a character in and out, its three settings, its remote friends,
// linking it to its player's Discord account and listing persons as friends, squelching a whole
// world, and channels. `/dn admin` is the operator's: blocking a world or one
// remote character for everyone on this world.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>What `/dn` says when the world is not on the network.</summary>
    public const string NotOnNetworkLine = "This world is not on the Dereth Network.";

    private const string Help = """
        /dn                         your network status
        /dn on | off                put this character on the Dereth Network, or take it off
        /dn visible nobody|friends|channels|anyone   who sees you online
        /dn reachable nobody|friends|anyone          who may send you a first tell
        /dn listed on|off           kept for a later version: nothing is published yet
        /dn friends                 your friends on other worlds
        /dn friends add | remove <Name@world>        a friend on another world; while this character
                                    is linked, the friend's person, on whichever character it plays if it
                                    shows its alts
        /dn link | unlink           link this character to your Discord account on the site, or unlink it
        /dn alts shown|hidden       whether others may learn that your linked characters are one person
        /dn alts [Name@world]       the setting, or the characters of that character's person
        /dn worlds                  the worlds on the network now
        /dn global <text>           say a line in the Global room (also /dng <text>)
        /dn squelch | unsquelch <world>              every character of a world
        /dn channel create | join | leave | members <channel>
        /dn channel invite <channel> <Name@world>
        /dn channel list            your channels
        Tell someone on another world with @tell Name@world, text. A channel is name@channel.
        """;

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
                    Status(player);
                    return;
                case 1 when first == "status":
                    Status(player);
                    return;
                case 1 when first == "help":
                    host.ShowSystem(player, Help);
                    return;
                case 1 when first == "on":
                    NetOn(player);
                    return;
                case 1 when first == "off":
                    NetOff(player);
                    return;
                case 2 when first == "visible":
                    if (Words.TryVisible(w[1], out var v))
                    {
                        RecordMut(player).Settings = RecordMut(player).Settings with { Visible = v };
                        dirty = true;
                        host.ShowSystem(player, $"Visible: {Words.Of(v)}.");
                        TellWatchers(player, null);
                    }
                    else
                    {
                        host.ShowSystem(player, "/dn visible nobody|friends|channels|anyone");
                    }
                    return;
                case 2 when first == "reachable":
                    if (Words.TryReachable(w[1], out var r))
                    {
                        RecordMut(player).Settings = RecordMut(player).Settings with { Reachable = r };
                        dirty = true;
                        host.ShowSystem(player, $"Reachable: {Words.Of(r)}.");
                    }
                    else
                    {
                        host.ShowSystem(player, "/dn reachable nobody|friends|anyone");
                    }
                    return;
                case 2 when first == "listed":
                    if (Words.TryListed(w[1], out var l))
                    {
                        RecordMut(player).Settings = RecordMut(player).Settings with { Listed = l };
                        dirty = true;
                        host.ShowSystem(player, $"Listed: {Words.Of(l)}. Nothing is published yet: listing is kept for a later version.");
                    }
                    else
                    {
                        host.ShowSystem(player, "/dn listed on|off");
                    }
                    return;
                case 1 when first == "friends":
                    Friends(player);
                    return;
                case 1 when first == "worlds":
                    Worlds(player);
                    return;
                case 2 when first is "squelch" or "unsquelch":
                    SquelchWorld(player, first == "squelch", w[1]);
                    return;
            }
            if (first is "link" or "unlink" && w.Count == 1 || first == "alts")
            {
                PersonCommand(player, w);
                return;
            }
            if (w.Count >= 3 && first == "friends" && w[1] == "add")
            {
                // What `@friends add Name@world` does.
                if (!AddFriend(player, string.Join(" ", w.Skip(2))))
                    host.ShowSystem(player, "/dn friends add Name@world: a character on another world.");
                return;
            }
            if (w.Count >= 3 && first == "friends" && w[1] == "remove")
            {
                // What removing the friend's row in the friends panel does.
                Address? listed = null;
                if (Address.TryParse(string.Join(" ", w.Skip(2)), out var a, out _)
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
            host.ShowSystem(player, Help);
        }
    }

    private void NetOn(uint player)
    {
        var name = host.OnlineName(player) ?? "";
        if (name.Contains(Address.Separator) || OwnAddress(player) is null)
        {
            host.ShowSystem(player, "This character's name cannot be a network address, so it cannot join the network.");
            return;
        }
        var rec = RecordMut(player);
        var firstTime = !rec.Settings.On && rec.Settings == CharacterSettings.Off;
        rec.Settings = firstTime ? CharacterSettings.OptedIn : rec.Settings with { On = true };
        dirty = true;
        host.ShowSystem(player,
            $"You are on the Dereth Network as {OwnAddress(player)}. Mutual friends see you online, and anyone on the network may tell you; /dn help for more.");
        OnLoginLocked(player);
    }

    private void NetOff(uint player)
    {
        if (!IsOn(player))
        {
            host.ShowSystem(player, "You are not on the Dereth Network.");
            return;
        }
        OnLogoutLocked(player, host.OnlineName(player), clientStays: true);
        RecordMut(player).Settings = RecordMut(player).Settings with { On = false };
        dirty = true;
        host.ShowSystem(player, "You are off the Dereth Network. Nobody on another world sees or reaches you.");
    }

    private void OnLoginLocked(uint player)
    {
        if (!IsOn(player))
            return;
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

    private string LinkWord => link switch
    {
        LinkState.Up => "connected",
        LinkState.Greeting => "connecting",
        _ => "not connected",
    };

    private void Status(uint player)
    {
        var rec = Record(player);
        var s = rec?.Settings ?? CharacterSettings.Off;
        var who = OwnAddress(player)?.ToString() ?? "";
        host.ShowSystem(player, s.On
            ? $"You are on the Dereth Network as {who}. Visible: {Words.Of(s.Visible)}. Reachable: {Words.Of(s.Reachable)}. Listed: {Words.Of(s.Listed)}. {rec!.Friends.Count} friend(s) on other worlds. The hub is {LinkWord}.{LinkedNote(player)}"
            : $"You are not on the Dereth Network. Type /dn on to join it as {who}. The hub is {LinkWord}.");
    }

    private void Friends(uint player)
    {
        var rows = FriendRows(player);
        if (rows.Count == 0)
        {
            host.ShowSystem(player, "You have no friends on other worlds. Add one with /dn friends add Name@world.");
            return;
        }
        host.ShowSystem(player, "Friends on other worlds:\n" +
            string.Join("\n", rows.Select(x => $"  {x.Name}: {(x.Online ? "online" : "offline")}")));
    }

    private void Worlds(uint player)
    {
        // The hub names the worlds: each name passes the text rules, and a slug that is not a
        // network slug is not shown at all.
        var up = worlds.Values
            .Where(i => i.Online && i.Slug != Config.Slug && Address.IsNetworkSlug(i.Slug))
            .Select(i =>
            {
                var name = TextRules.SanitizeLine(i.Name);
                return $"  {i.Slug} ({(name.Length > 80 ? name[..80] : name)})";
            })
            .ToList();
        host.ShowSystem(player, up.Count == 0 ? "No other world is on the network now." : "On the network now:\n" + string.Join("\n", up));
    }

    private void SquelchWorld(uint player, bool add, string world)
    {
        world = Text.AsciiLower(world);
        if (!Address.IsSlug(world) || world == Config.Slug)
        {
            host.ShowSystem(player, "/dn squelch <world>: a world on the network, by its short name.");
            return;
        }
        var held = Record(player);
        var already = held?.SquelchedWorlds.Contains(world) == true;
        if (add && !already && held?.SquelchedWorlds.Count >= MaxSquelchedWorlds)
        {
            host.ShowSystem(player, $"You squelch {MaxSquelchedWorlds} worlds already; unsquelch one first.");
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
            host.ShowSystem(player, "You are not on the Dereth Network. Type /dn on to join it.");
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
                if (!Address.TryParse(string.Join(" ", words.Skip(2)), out var a, out _) || a.IsChannel)
                {
                    host.ShowSystem(player, "/dn channel invite <channel> <Name@world>");
                    return;
                }
                (op, name, who) = (ChannelOp.Invite, words[1], a);
                break;
            default:
                host.ShowSystem(player, Help);
                return;
        }
        var n = name;
        while (n.EndsWith("@channel", StringComparison.Ordinal))
            n = n[..^5];
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
        pendingChannels[id] = new PendingChannel(player, null);
        Send(new ChannelRequest(id, op, name, by, who));
    }

    /// <summary>Leaves a channel (a squelch of the channel's address).</summary>
    private void LeaveChannel(uint player, string name) => Request(player, ChannelOp.Leave, name, null);

    /// <summary>The answer to a channel request.</summary>
    private void OnChannelResult(string id, ChannelOutcome outcome, string channel, IReadOnlyList<Address> members,
        IReadOnlyList<string> channels)
    {
        if (!pendingChannels.Remove(id, out var p))
            return;
        if (!Online(p.Player))
            return;
        var address = channel + "@channel";
        var line = outcome switch
        {
            ChannelOutcome.Ok when channels.Count > 0 => $"Your channels: {string.Join(", ", channels.Select(c => c + "@channel"))}.",
            ChannelOutcome.Ok when members.Count > 0 => $"{address}: {string.Join(", ", members)}.",
            ChannelOutcome.Ok when channel == "list" => "You belong to no channel.",
            ChannelOutcome.Ok => $"Done. Tell the channel with @tell {address}, text.",
            ChannelOutcome.Exists => $"{address} already exists.",
            ChannelOutcome.NoChannel => $"There is no channel {address}.",
            ChannelOutcome.NotMember => $"You are not a member of {address}.",
            ChannelOutcome.NotInvited => $"You have not been invited to {address}.",
            ChannelOutcome.AlreadyMember => $"That character is already a member of {address}.",
            _ => $"The network refused that for {address}.",
        };
        host.ShowSystem(p.Player, line);
    }

    /// <summary>A channel request that will not be answered.</summary>
    private void GiveUpChannel(string id)
    {
        if (pendingChannels.Remove(id, out var p) && Online(p.Player))
            host.ShowSystem(p.Player, "The Dereth Network is unavailable.");
    }

    /// <summary>
    /// An invitation for one of this world's characters. It is shown as a first tell from the
    /// inviter would be: only to a character that inviter may reach, never past a squelch of the
    /// inviter, of its world or of the channel, or the operator's blocks, and held to the pace a tell
    /// from the inviter's world to that character is.
    /// </summary>
    private void OnChannelInvite(string channel, Address by, Address to)
    {
        if (to.World != Config.Slug)
            return;
        if (host.OnlineByName(to.Name) is not uint target)
            return;
        var channelAddress = Address.Create(channel, Address.ChannelWorld);
        if (!IsOn(target) || Blocked(by) || !ReachableBy(target, by, true) || Squelches(target, by)
            || channelAddress is null || Squelches(target, channelAddress))
            return;
        var byName = TextRules.SanitizeName(by.ToString());
        if (byName is null)
            return;
        if (!MayReceive(by.World, to.Key, limits.TellsPerRecipient))
            return;
        host.ShowSystem(target, $"{byName} invites you to the channel {channel}@channel. Type /dn channel join {channel} to join it.");
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
            return "/dn verify <token>: the token the site gave you (dnv_...).";
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

    /// <summary>`/dn admin &lt;args&gt;`: the operator's blocks, and the network's state.</summary>
    public string Admin(IReadOnlyList<string> args)
    {
        lock (gate)
        {
            var first = args.Count > 0 ? args[0] : "";
            if (args.Count >= 2 && first is "block" or "unblock")
            {
                var who = string.Join(" ", args.Skip(1));
                string key;
                if (Address.TryParse(who, out var a, out _))
                    key = a.Key;
                else if (Address.IsSlug(Text.AsciiLower(who)))
                    key = Text.AsciiLower(who);
                else
                    return "/dn admin block|unblock <world> or <Name@world>";
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
            if (args.Count == 1 && first == "blocks")
                return store.Blocked.Count == 0 ? "Nothing is blocked." : $"Blocked: {string.Join(", ", store.Blocked)}.";
            if (args.Count == 0 || (args.Count == 1 && first == "status"))
            {
                var room = Config.GlobalRoom is RoomSlot s ? RoomSlots.Name(s) : "none";
                return $"Dereth Network: on as {Config.Slug}; the hub is {LinkWord}; " +
                    $"{worlds.Values.Count(i => i.Online)} world(s) online; {outbox.Count + answers.Values.Sum(q => q.Count) + chatter.Count} queued; global room: {room}.";
            }
            return "/dn admin status | blocks | block <world or Name@world> | unblock <...>";
        }
    }
}
