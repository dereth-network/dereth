// Tells to and from other worlds' characters, and channel lines.
//
// A local player's tell to an address goes to the hub, and the player sees what a local tell
// would give once the answer comes back: the echo, the away message, or `Character not available`.
// A remote tell is delivered from the sender's stand-in id, after the receiving character's
// settings, squelches and the operator's blocks have had their say.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>A tell by name. The sender's gag has been checked.</summary>
    public Routed TellByName(uint player, string target, string text)
    {
        lock (gate)
        {
            if (!Address.LooksRemote(target))
                return Routed.NotMine;
            if (!Address.TryParse(target, out var to, out _))
            {
                host.ShowNotAvailable(player);
                return Routed.Handled;
            }
            if (to.World == Config.Slug)
                return Routed.Local(to.Name);
            SendTell(player, to, text);
            return Routed.Handled;
        }
    }

    /// <summary>
    /// A tell by id: a reply to a remote sender goes by the stand-in id it was shown under. True
    /// when the id was one, and the network took the tell.
    /// </summary>
    public bool TellById(uint player, uint target, string text)
    {
        lock (gate)
        {
            if (!StandIns.IsStandIn(target))
                return false;
            var to = UseStandIn(target);
            if (to is null)
            {
                host.ShowNotAvailable(player);
                return true;
            }
            SendTell(player, to, text);
            return true;
        }
    }

    private void SendTell(uint player, Address to, string text)
    {
        if (!IsOn(player))
        {
            host.ShowSystem(player, "You are not on the Dereth Network. Type /dn on to join it.");
            return;
        }
        if (Blocked(to))
        {
            host.ShowNotAvailable(player);
            return;
        }
        if (TooNew(player))
        {
            host.ShowSystem(player, TooNewLine);
            return;
        }
        var from = OwnAddress(player);
        if (from is null)
        {
            host.ShowNotAvailable(player);
            return;
        }
        if (!MaySend(player, Sends.Tells))
        {
            host.ShowSystem(player, TooQuicklyLine);
            return;
        }
        if (!to.IsChannel)
            told[(player, to.Key)] = Now;
        var friend = Record(player)?.Friends.ContainsKey(to.Key) == true;
        var id = FreshId('t');
        pending[id] = new PendingTell(player, to, text, null);
        Send(new Tell(id, from, to, text, friend));
    }

    /// <summary>The answer to one of this world's tells.</summary>
    private void OnTellResult(string id, Outcome outcome, string? message)
    {
        if (!pending.Remove(id, out var p))
            return;
        // The sender may have logged off while it was in flight.
        if (!Online(p.Player))
            return;
        var to = p.To.ToString();
        switch (outcome)
        {
            case Outcome.Delivered:
            case Outcome.Ok:
                host.ShowEcho(p.Player, to, p.Text);
                break;
            case Outcome.Squelched:
                // What a sender squelched by a local character sees.
                host.ShowSquelched(p.Player, to, p.Text);
                break;
            case Outcome.Afk:
                host.ShowEcho(p.Player, to, p.Text);
                host.ShowAway(p.Player, AwayLine(p.To, message));
                break;
            case Outcome.Offline:
                host.ShowNotAvailable(p.Player);
                break;
            case Outcome.Unavailable:
                host.ShowNotAvailable(p.Player);
                host.ShowSystem(p.Player, $"The Dereth Network can't reach {p.To.World} right now.");
                break;
            case Outcome.NotOnNetwork:
                host.ShowNotAvailable(p.Player);
                host.ShowSystem(p.Player, $"{p.To.World} isn't on the network.");
                break;
            case Outcome.RateLimited:
                // The character's own pace was kept here before the tell went, so a refusal is the
                // world's share, or the loop guard's.
                host.ShowSystem(p.Player, HeldBackLine(to));
                break;
            case Outcome.NotMember:
                host.ShowSystem(p.Player, $"You are not a member of {to}.");
                break;
        }
    }

    /// <summary>
    /// The away line a sender sees: always `&lt;to&gt; is away: &lt;text&gt;`, built here from the away
    /// text alone, so another world cannot put a line of its own choosing in front of the sender. A
    /// text that repeats the start of the line itself is taken without it.
    /// </summary>
    private string AwayLine(Address to, string? message)
    {
        var text = message is null ? "" : TextRules.SanitizeLine(message);
        var prefix = $"{to} is away:";
        if (text.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
            text = text[prefix.Length..].Trim();
        if (text.Length == 0)
            text = host.DefaultAwayText;
        return TextRules.SanitizeLine($"{to} is away: {text}");
    }

    /// <summary>A tell that will not be answered: the link is down (`hubDown`), or the answer is too late.</summary>
    private void GiveUp(string id, bool hubDown)
    {
        if (!pending.Remove(id, out var p))
            return;
        if (!Online(p.Player))
            return;
        host.ShowNotAvailable(p.Player);
        host.ShowSystem(p.Player, hubDown
            ? "The Dereth Network is unavailable."
            : $"The Dereth Network can't reach {p.To.World} right now.");
    }

    /// <summary>
    /// What a sender is told when a limit the whole world shares held its tell back (the hub's, or
    /// the receiving world's).
    /// </summary>
    private static string HeldBackLine(string to) =>
        $"The Dereth Network is holding back tells from this world to {to} for now. Wait a moment and try again.";

    /// <summary>The answer owed to `world` for its tell `id`.</summary>
    private void Answer(string world, string id, Outcome outcome, string? message = null) =>
        SendAnswer(world, new TellResult(id, outcome, message));

    /// <summary>A remote sender's display name, or null when the network's rules refuse it.</summary>
    private static string? DisplayName(Address from) => TextRules.SanitizeName(from.ToString());

    /// <summary>Whether `character` takes a first tell from `from` (`friend`: the sender lists the recipient).</summary>
    private bool ReachableBy(uint character, Address from, bool friend)
    {
        var r = Record(character);
        if (r is null)
            return false;
        var inWindow = told.TryGetValue((character, from.Key), out var at) && Now - at < ReplyWindowSecs;
        return inWindow || r.Settings.Reachable switch
        {
            Reachable.Anyone => true,
            Reachable.Friends => friend && r.Friends.ContainsKey(from.Key),
            _ => false,
        };
    }

    /// <summary>Whether `character` squelches `who` (the character or its whole world).</summary>
    private bool Squelches(uint character, Address who)
    {
        var r = Record(character);
        return r is not null && (r.Squelched.ContainsKey(who.Key) || r.SquelchedWorlds.Contains(who.World));
    }

    /// <summary>A tell from another world for a character of this one.</summary>
    private void OnTell(string id, Address from, Address to, string text, bool friend)
    {
        if (link != LinkState.Up)
            return;
        var origin = from.World;
        // This world's own characters never reach it through the hub: a tell claiming one is refused.
        if (origin == Config.Slug)
        {
            Answer(origin, id, Outcome.Offline);
            return;
        }
        // A world already owed its share of answers is not answered again until it is owed fewer;
        // the hub answers this one for it.
        if (!MayOwe(origin))
        {
            log(LogLevel.Debug, "a world is owed its share of answers; a tell is not taken");
            return;
        }
        var target = to.World == Config.Slug ? host.OnlineByName(to.Name) : null;
        if (target is not uint character)
        {
            Answer(origin, id, Outcome.Offline);
            return;
        }
        var name = DisplayName(from);
        if (name is null)
        {
            Answer(origin, id, Outcome.Offline);
            return;
        }
        if (!IsOn(character) || Blocked(from) || !ReachableBy(character, from, friend))
        {
            Answer(origin, id, Outcome.Offline);
            return;
        }
        if (Squelches(character, from))
        {
            Answer(origin, id, Outcome.Squelched);
            return;
        }
        // Only a tell that would be shown counts against the paces below, so a refusal says
        // nothing a delivered tell would not.
        // One world's tells to one character are held to the per-recipient pace here too.
        if (!MayReceive(origin, to.Key, limits.TellsPerRecipient))
        {
            Answer(origin, id, Outcome.RateLimited);
            return;
        }
        if (RemoteStandIn(from, out var budgetSpent) is not uint senderId)
        {
            Answer(origin, id, budgetSpent ? Outcome.RateLimited : Outcome.Offline);
            return;
        }
        host.ShowTell(character, name, senderId, TextRules.SanitizeLine(text));
        // Only the away text goes back: the sender's world writes the line around it. The wire
        // carries at most the network's text limit, so a longer one is cut to it.
        var away = host.AwayMessage(character);
        if (away is null)
            Answer(origin, id, Outcome.Delivered);
        else
            Answer(origin, id, Outcome.Afk, Text.CharCount(away) <= TextRules.MaxTextChars ? away : Text.FirstChars(away, TextRules.MaxTextChars));
    }

    /// <summary>
    /// A channel line for a member on this world: a tell from the channel's own address, with the
    /// speaker's address at the start of the text. It is held to the pace a tell from the speaker's
    /// world to that member is.
    /// </summary>
    private void OnChannelLine(string channel, Address from, Address to, string text)
    {
        var channelAddress = Address.Create(channel, Address.ChannelWorld);
        if (channelAddress is null)
            return;
        var target = to.World == Config.Slug ? host.OnlineByName(to.Name) : null;
        if (target is not uint character)
            return;
        if (!IsOn(character) || Blocked(from) || Squelches(character, from) || Squelches(character, channelAddress))
            return;
        var speaker = DisplayName(from);
        if (speaker is null)
            return;
        if (!MayReceive(from.World, to.Key, limits.TellsPerRecipient))
            return;
        if (RemoteStandIn(channelAddress) is not uint senderId)
            return;
        var line = TextRules.SanitizeLine(speaker + ": " + TextRules.SanitizeLine(text));
        host.ShowTell(character, channelAddress.ToString(), senderId, line);
    }
}
