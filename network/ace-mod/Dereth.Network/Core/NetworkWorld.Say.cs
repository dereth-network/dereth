// A line a person said elsewhere as one of this world's characters (docs/network/02-link-protocol.md
// section 9, 03-world-behaviour.md section 3): in a Discord channel that mirrors the Global room or
// one of the world's own rooms. The hub asks the world to say it as the character, and the world says
// it under the rules a line the character says itself meets, whether the character is online or not:
// its own character, on the network and linked by itself (it asked for its code, so that a person's
// link to an earlier character of the name speaks for no later one), its account not banned, the
// character not gagged and no younger than the operator's minimum age, the room carried here and not
// turned off, the world's own rules for the room letting the character speak there (an Olthoi
// player may not, say), and its pace of room lines not spent. Otherwise it answers `refused` with
// the reason, and shows and sends nothing.
//
// A line it says goes to the room's local listeners as the room's own line from the character, under
// the character's own id, its name marked where it was said (`Ann (Discord)`); then to the hub as a
// `room.say` carrying the same mark, under every rule of the room, so that it reaches the other
// worlds and every mirror of the room. The character is told nothing: its player said it.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>The answers to the hub's says, sent at once while the link is up; neither side queues one.</summary>
    private readonly List<string> sayResults = [];

    /// <summary>A refusal that names no reason: a line nothing is left of once through the text rules.</summary>
    private const string NoReason = "";

    /// <summary>One of the say messages from the hub: a say is answered; an answer, which only a world sends, is ignored.</summary>
    private void HandleSayMessage(SayMessage m)
    {
        if (m is not SayAsk a || link != LinkState.Up)
            return;
        var reason = SayAs(a);
        sayResults.Add(SayWire.Write(reason switch
        {
            null => new SayResult(a.Id, SayOutcome.Said),
            NoReason => new SayResult(a.Id, SayOutcome.Refused),
            _ => new SayResult(a.Id, SayOutcome.Refused, reason),
        }));
    }

    /// <summary>
    /// Says the hub's line as its character, when every rule allows it: null when said; otherwise the
    /// reason it is not, and nothing is shown or sent.
    /// </summary>
    private string? SayAs(SayAsk a)
    {
        if (a.Who.World != Config.Slug || host.FindCharacter(a.Who.Name) is not { } found || host.NameOf(found.Id) is null
            || Address.Create(found.Name, Config.Slug) is not { } from)
            return SayReasons.NoCharacter;
        var character = found.Id;
        if (Record(character) is not { Settings.On: true } record)
            return SayReasons.NotOnNetwork;
        if (!record.AskedLink)
            return SayReasons.NotLinked;
        if (host.IsBanned(character))
            return SayReasons.Banned;
        if (host.IsGagged(character))
            return SayReasons.Gagged;
        if (TooNew(character))
            return SayReasons.TooNew;
        if (SayRoom(a.Room) is not RoomSlot slot || !host.RoomOpen(slot))
            return SayReasons.RoomOff;
        if (host.RoomRefusal(character, slot) is { } refusal)
            return refusal;
        var line = TextRules.SanitizeLine(a.Text);
        if (line.Length == 0)
            return NoReason;
        // The pace is taken last, so that a line refused for any other reason costs the character none.
        if (!MaySend(character, Sends.RoomLines))
            return SayReasons.RateLimited;
        var name = $"{found.Name} ({SayWire.MarkName(a.Via)})";
        foreach (var listener in host.OnlineCharacters().Where(r => host.ListensToRoom(r, slot) && !host.SquelchesLocal(r, character)))
            host.ShowRoomLine(listener, slot, name, character, line);
        // The hub asked for the line from a mirror of the room, so it carries the room now.
        uncarried.Remove(a.Room);
        var id = FreshId('r');
        pendingRooms[id] = new PendingRoom(character, a.Room, null);
        Send(new RoomSay(id, a.Room, from, line, a.Via));
        return null;
    }

    /// <summary>The TurbineChat room that carries `room` here: the Global room's slot, or one of the world's own rooms the hub mirrors.</summary>
    private RoomSlot? SayRoom(string room)
    {
        if (room == Wire.GlobalRoom)
            return GlobalSlot;
        foreach (var r in MirroredSlots)
            if (RoomSlots.Room(r) == room)
                return r;
        return null;
    }

    /// <summary>The answers to the hub's says, before anything else but the redemptions, within the pace.</summary>
    private void TakeSayResults(double t, Bucket pace, List<string> frames)
    {
        while (sayResults.Count > 0 && pace.Has(t))
        {
            pace.Take(t);
            frames.Add(sayResults[0]);
            sayResults.RemoveAt(0);
        }
    }
}
