// The Global room, carried in one TurbineChat room of the operator's choosing.
//
// A line an opted-in character says in that room reaches the local listeners as any room line
// does, and goes on to the hub. A line from another world reaches every local player who listens to
// that room, as a room event under the speaker's address and stand-in id (the retail client checks
// the id against its squelch list; the name link it draws always carries id 0).
//
// `/dng <text>` (and `/dn global <text>`) says a line in that room from wherever the character is
// talking: the world takes it exactly as the client's own room line, so every rule of the room
// applies to it.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>
    /// A local line in TurbineChat room `channel`, after it has gone to the local listeners. It goes
    /// to the network when the room carries the Global room and the speaker is on the network.
    /// </summary>
    public void RoomSaid(uint player, uint channel, string text)
    {
        lock (gate)
        {
            if (Config.GlobalRoom is not RoomSlot slot || RoomSlots.Channel(slot) != channel || !IsOn(player))
                return;
            if (TooNew(player))
            {
                host.ShowSystem(player,
                    "Your line did not reach the other worlds: this character is too new to send to the Dereth Network yet.");
                return;
            }
            if (OwnAddress(player) is not { } from)
                return;
            if (!MaySend(player, Sends.RoomLines))
            {
                host.ShowSystem(player, RoomTooQuicklyLine);
                return;
            }
            var id = FreshId('r');
            pendingRooms[id] = new PendingRoom(player, null);
            Send(new RoomSay(id, Wire.GlobalRoom, from, text));
        }
    }

    /// <summary>What `/dng` says on a world that carries no Global room.</summary>
    public const string NotCarriedLine = "This world does not carry the Global room.";

    /// <summary>
    /// `/dng &lt;text&gt;`: `player` says `text` in the room that carries the Global room, as if the
    /// client had sent it there. The world's own room handling shows it to the local listeners and,
    /// through <see cref="RoomSaid"/>, passes it to the network under every rule a line said in the
    /// room meets.
    /// </summary>
    public void Global(uint player, string text)
    {
        lock (gate)
        {
            if (Config.GlobalRoom is not RoomSlot slot)
            {
                host.ShowSystem(player, NotCarriedLine);
                return;
            }
            text = text.Trim();
            if (text.Length == 0)
            {
                host.ShowSystem(player, "/dng <text>: say a line in the Global room.");
                return;
            }
            if (!IsOn(player))
            {
                host.ShowSystem(player, "You are not on the Dereth Network. Type /dn on to join it.");
                return;
            }
            host.SayInRoom(player, slot, text);
        }
    }

    /// <summary>How the hub took one of this world's room lines: only a refusal is news to the speaker.</summary>
    private void OnRoomResult(string id, Outcome outcome)
    {
        if (!pendingRooms.Remove(id, out var p))
            return;
        if (outcome == Outcome.RateLimited && Online(p.Player))
            host.ShowSystem(p.Player, RoomHeldBackLine);
    }

    /// <summary>What a speaker is told when its line stayed on this world because it sent too quickly.</summary>
    private const string RoomTooQuicklyLine = "Your line did not reach the other worlds: you are sending too quickly.";

    /// <summary>
    /// What a speaker is told when its line stayed on this world because the hub refused it: the
    /// speaker's own pace was kept here, so it is the world's share of the room that is spent.
    /// </summary>
    private const string RoomHeldBackLine =
        "Your line did not reach the other worlds: the Dereth Network is holding back this world's lines for now.";

    /// <summary>A line from another world's character in the Global room.</summary>
    private void OnRoomLine(string room, Address from, string text)
    {
        if (Config.GlobalRoom is not RoomSlot slot)
            return;
        if (room != Wire.GlobalRoom || Blocked(from) || from.World == Config.Slug)
            return;
        if (!host.RoomOpen(slot))
            return;
        var name = TextRules.SanitizeName(from.ToString());
        if (name is null)
            return;
        var line = TextRules.SanitizeLine(text);
        if (line.Length == 0)
            return;
        // One world's lines are held to the room's per-world pace here too.
        if (!MayReceive(from.World, Wire.GlobalRoom, limits.RoomLinesPerWorld))
            return;
        // Nothing is given (not even the speaker's stand-in id) unless someone here is shown it.
        var listeners = host.OnlineCharacters()
            .Where(r => host.ListensToRoom(r, slot) && !Squelches(r, from))
            .ToList();
        if (listeners.Count == 0 || RemoteStandIn(from) is not uint senderId)
            return;
        foreach (var recipient in listeners)
            host.ShowRoomLine(recipient, slot, name, senderId, line);
    }
}
