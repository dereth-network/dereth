// The hub's question whether one of this world's characters, not online, would have been shown a
// tell (docs/network/02-link-protocol.md section 8), asked before the hub carries the tell to the
// character's person elsewhere: its Discord.
//
// A tell to a character that is not online is answered `offline`. The hub then asks `reach`, and
// the world answers `reachable` true only for a character of its own, not online, on the network,
// whose link is its own (it asked for its code: the hub asks about one that never did for an
// earlier character of its name, since deleted), squelching neither the sender nor the sender's
// world, and blocking neither (the operator's blocks). A sender of this world's own reaches it too,
// unless the character squelched it, its account or everyone for tells as it last left the world:
// what it kept then decides, and a character that has not left since keeping it is not reached
// this way. A `true` takes one from the world's pace of tells from the sender's world to the
// character, as a shown tell does, and one past it is `false`. Anything else is `false`, alike: a
// character online now, one off the network, one deleted or waiting to be, one this world does not
// have. The answer shows the character nothing and gives no stand-in id.
//
// An answer waits with the answers owed to the sender's world: at most MaxAnswersPerWorld for one
// world (a question past that is not answered, and the hub carries the tell nowhere else), a world
// at a time in turn, dropped once the hub has stopped waiting for it, or when the link goes.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>One of the reach messages from the hub: a question is answered; an answer, which only a world sends, is ignored.</summary>
    private void HandleReachMessage(ReachMessage m)
    {
        if (m is not ReachAsk a || link != LinkState.Up)
            return;
        var origin = a.From.World;
        if (!MayOwe(origin))
        {
            log(LogLevel.Debug, "a world is owed its share of answers; a question is not taken");
            return;
        }
        Owe(origin, ReachWire.Write(new ReachResult(a.Id, ReachableAway(a.From, a.To))));
    }

    /// <summary>
    /// Whether `to`, one of this world's characters not online, would have been shown a tell from
    /// `from`; a true takes one from the world's pace of tells from `from`'s world to `to`.
    /// </summary>
    private bool ReachableAway(Address from, Address to)
    {
        if (to.World != Config.Slug)
            return false;
        if (host.FindCharacter(to.Name) is not { } found || Online(found.Id))
            return false;
        var character = found.Id;
        var shown = Record(character) is { Settings.On: true, AskedLink: true }
            && !Blocked(from) && !Squelches(character, from) && DisplayName(from) is not null
            && (from.World != Config.Slug || !SquelchedAway(character, from))
            // The host is asked last, only for a character that would be shown the tell.
            && host.NameOf(character) is not null;
        return shown && MayReceive(from.World, to.Key, limits.TellsPerRecipient);
    }

    /// <summary>
    /// Whether `character`, away, holds back a tell from `from`, a character of this world: by the
    /// tell squelches it kept as it last left, and always when it kept none, or `from` is no
    /// character of this world's or is itself.
    /// </summary>
    private bool SquelchedAway(uint character, Address from) =>
        host.FindCharacter(from.Name) is not { } teller || teller.Id == character
        || Record(character)?.AwaySquelches is not { } kept || kept.Covers(teller.Id, host.AccountOf(teller.Id));
}
