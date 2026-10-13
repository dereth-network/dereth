// The Global room, carried in one of the world's TurbineChat rooms, and the world's own rooms
// mirrored into Discord, both as the hub says.
//
// Where. The hub keeps which of the world's rooms carries the Global room, and which of its own
// rooms it mirrors into Discord, and tells the world at each welcome and whenever they change
// (`rooms`); the world applies them at once, joining and leaving the rooms on the link, and keeps the
// last it was told in its store, so that it starts right while the hub is away. Until a hub has told
// it, the world carries the Global room where its `GlobalRoom` setting says, and mirrors none of its
// own rooms. A hub that holds no room for the world is sent that one, once on each link (its seed);
// a hub that holds one has the last word, and the setting is logged once if it differs. The operator
// moves it in game with `/dn admin global` (which asks the hub) or on the hub's operator page.
//
// Out. A line an opted-in character says in that room reaches the local listeners as any room line
// does, and goes on to the hub. A line in one of the world's own rooms the hub mirrors goes to the
// hub the same way, under its own name (`general`), and from there to Discord alone; the hub hands no
// line back into it, and a line a person says there from Discord comes only as the hub's `say`
// (NetworkWorld.Say.cs). A room the hub says it does not mirror keeps its lines here for a minute
// without sending them, so that it costs its speakers nothing.
//
// In. A line from another world reaches every local player who listens to the room that carries the
// Global room, as a room event under the speaker's address and stand-in id (the retail client checks
// the id against its squelch list; the name link it draws always carries id 0).
//
// `/dng <text>` (and `/dn global <text>`) says a line in that room from wherever the character is
// talking: the world takes it exactly as the client's own room line, so every rule of the room
// applies to it.

namespace Dereth.Network;

public sealed partial class NetworkWorld
{
    /// <summary>
    /// How long the lines of one of the world's own rooms stay here, unsent, after the hub says it
    /// does not mirror the room into Discord, in seconds. The next line after it goes to the hub
    /// again, so a room its operator has mirrored since is carried within the minute.
    /// </summary>
    public const double UncarriedSecs = 60;

    /// <summary>The hub has told the world its rooms on this link.</summary>
    private bool roomsTold;

    /// <summary>The world has sent its setting's Global room on this link, to a hub that held none.</summary>
    private bool seeded;

    /// <summary>The log has said that the setting's Global room differs from the hub's.</summary>
    private bool settingDifferLogged;

    /// <summary>The world's own requests of the rooms messages, sent first while the link is up, and dropped when it goes.</summary>
    private readonly List<string> roomsRequests = [];

    /// <summary>The operator's `/dn admin global` waiting for the hub's answer, by the request's id: whom to tell (the log when none).</summary>
    private readonly Dictionary<string, uint?> globalAsks = new(StringComparer.Ordinal);

    /// <summary>The TurbineChat room that carries the Global room here now: where the hub last said, else where the setting says.</summary>
    public RoomSlot? GlobalSlot
    {
        get
        {
            if (Config.RoomsOff)
                return null;
            return store.Rooms?.Global switch
            {
                RoomsWire.Off => null,
                { } room when RoomSlots.TryParse(room, out var slot) => slot,
                _ => Config.GlobalRoom,
            };
        }
    }

    /// <summary>The world's own rooms whose lines go to the hub to be mirrored into Discord now, as the hub last said: never the one carrying the Global room.</summary>
    public IReadOnlyList<RoomSlot> MirroredSlots
    {
        get
        {
            var global = GlobalSlot;
            var slots = new List<RoomSlot>();
            if (Config.RoomsOff || store.Rooms is not { } kept)
                return slots;
            foreach (var room in kept.Mirrored)
                if (RoomSlots.TryParse(room, out var slot) && slot != global && !slots.Contains(slot))
                    slots.Add(slot);
            return slots;
        }
    }

    /// <summary>A room as the link names where the Global room is carried: one of the world's own rooms, or `off`.</summary>
    private static string RoomWord(RoomSlot? slot) => slot is RoomSlot s ? RoomSlots.Room(s) : RoomsWire.Off;

    /// <summary>A room as players know it, or "no room".</summary>
    private static string Shown(RoomSlot? slot) => slot is RoomSlot s ? RoomSlots.Name(s) : "no room";

    /// <summary>
    /// The room TurbineChat room `channel` passes on to the hub, by its name on the link: the Global
    /// room, or one of the world's own rooms the hub mirrors into Discord.
    /// </summary>
    private string? CarriedRoom(uint channel)
    {
        if (GlobalSlot is RoomSlot g && RoomSlots.Channel(g) == channel)
            return Wire.GlobalRoom;
        foreach (var r in MirroredSlots)
            if (RoomSlots.Channel(r) == channel)
                return RoomSlots.Room(r);
        return null;
    }

    /// <summary>Where a line said in `room` goes past this world, as its speaker is told.</summary>
    private static string Reach(string room) => room == Wire.GlobalRoom ? "the other worlds" : "Discord";

    /// <summary>
    /// At each welcome: the world's own rooms the hub last said it mirrors are joined by their names,
    /// what the hub said of them before is forgotten, since this hub may mirror them now, and the hub
    /// is to tell the world its rooms again.
    /// </summary>
    private void JoinOwnRooms()
    {
        uncarried.Clear();
        roomsTold = false;
        seeded = false;
        foreach (var r in MirroredSlots)
            Send(new RoomJoin(RoomSlots.Room(r), true));
    }

    /// <summary>The link went: the requests not yet sent are dropped, and an operator waiting for the hub's answer is told it will not come.</summary>
    private void RoomsLinkDown()
    {
        roomsRequests.Clear();
        foreach (var who in globalAsks.Values.ToList())
            TellOperator(who, "The link to the Dereth Network's hub went before it answered. Type @dn admin global again once it is back.");
        globalAsks.Clear();
    }

    /// <summary>A line for whoever typed an operator's command: a character while it is online, else the log.</summary>
    private void TellOperator(uint? who, string line)
    {
        if (who is uint p && Online(p))
            host.ShowSystem(p, line);
        else
            log(LogLevel.Info, line);
    }

    /// <summary>One of the rooms messages from the hub: the world's rooms are applied; a request, which only a world sends, is ignored.</summary>
    private void HandleRoomsMessage(RoomsMessage m)
    {
        if (m is RoomsSetup s)
            OnRooms(s.Id, s.Global, s.Mirrored);
    }

    /// <summary>
    /// The hub's word on the world's rooms: the room that carries the Global room (null while the
    /// hub holds none) and the own rooms it mirrors into Discord. They hold at once and are kept: the
    /// rooms that stop being carried are left on the link, and those that start are joined. A hub
    /// that holds no room is sent the one the world carries the Global room in now, once on each
    /// link; a hub that holds one has the last word, and the setting is logged once if it differs.
    /// An operator's `/dn admin global` the message answers (`id`) is told where the room is now.
    /// </summary>
    private void OnRooms(string? id, string? global, IReadOnlyList<string> mirrored)
    {
        var (globalBefore, mirroredBefore) = (GlobalSlot, MirroredSlots);
        roomsTold = true;
        var kept = new KeptRooms(global ?? store.Rooms?.Global, mirrored.Where(RoomsWire.OwnRooms.Contains).ToList());
        if (store.Rooms is not { } was || was.Global != kept.Global || !was.Mirrored.SequenceEqual(kept.Mirrored))
        {
            store.Rooms = kept;
            dirty = true;
        }
        var (globalNow, mirroredNow) = (GlobalSlot, MirroredSlots);
        if (globalNow != globalBefore)
        {
            log(LogLevel.Info, $"the Global room is carried in {Shown(globalNow)} now");
            if (globalNow.HasValue != globalBefore.HasValue)
                Send(new RoomJoin(Wire.GlobalRoom, globalNow.HasValue));
        }
        foreach (var r in mirroredBefore.Where(r => !mirroredNow.Contains(r)))
            Send(new RoomJoin(RoomSlots.Room(r), false));
        foreach (var r in mirroredNow.Where(r => !mirroredBefore.Contains(r)))
        {
            uncarried.Remove(RoomSlots.Room(r));
            Send(new RoomJoin(RoomSlots.Room(r), true));
        }
        if (global is null && !seeded && !Config.RoomsOff)
        {
            seeded = true;
            var room = RoomWord(globalNow);
            log(LogLevel.Info, $"the hub holds no room for this world's Global room; it is told {room}, as this world carries it now");
            roomsRequests.Add(RoomsWire.Write(new RoomsGlobal(FreshId('g'), room, Seed: true)));
        }
        else if (global is not null && Config.GlobalRoomNamed && global != RoomWord(Config.GlobalRoom) && !settingDifferLogged)
        {
            settingDifferLogged = true;
            log(LogLevel.Info,
                $"the hub carries this world's Global room in {global}, so GlobalRoom ({RoomWord(Config.GlobalRoom)}) is not read: it was read until the hub kept a room, and goes in a later release. Move the room with @dn admin global, or on the hub's operator page");
        }
        if (id is not null && globalAsks.Remove(id, out var who))
            TellOperator(who, $"The Dereth Network's hub carries this world's Global room in {Shown(globalNow)} now.");
    }

    /// <summary>The world's own requests of the rooms messages, before anything else, within the pace.</summary>
    private void TakeRoomsRequests(double t, Bucket pace, List<string> frames)
    {
        while (roomsRequests.Count > 0 && pace.Has(t))
        {
            pace.Take(t);
            frames.Add(roomsRequests[0]);
            roomsRequests.RemoveAt(0);
        }
    }

    /// <summary>What `/dn admin global` says of a room it does not know: its help line, as typed in game or at the console.</summary>
    private static string GlobalUsage(uint? who)
    {
        var line = Usage("@dn admin global <");
        return who is null ? "dn" + line[3..] : line;
    }

    /// <summary>Where this world carries the Global room, and whence it knows, as `/dn admin global` says it.</summary>
    private string GlobalStatus()
    {
        var from = store.Rooms?.Global is not null
            ? "as the Dereth Network's hub keeps it"
            : "as GlobalRoom says, until the Dereth Network's hub keeps a room";
        return $"This world carries the Global room in {Shown(GlobalSlot)}, {from}. Type @dn admin global <general|trade|lfg|roleplay|off> to move it.";
    }

    /// <summary>
    /// `/dn admin global [&lt;room&gt;]`: where this world carries the Global room, or asks the hub to
    /// carry it in `room` (or nowhere, `off`) from now on. The hub's answer goes to `who` (the log when
    /// none); the line returned is the immediate one.
    /// </summary>
    private string AdminGlobal(uint? who, IReadOnlyList<string> args)
    {
        if (args.Count == 0)
            return GlobalStatus();
        if (args.Count > 1)
            return GlobalUsage(who);
        RoomSlot? room;
        var word = Text.AsciiLower(args[0]);
        if (word is RoomsWire.Off or "none")
            room = null;
        else if (RoomSlots.TryParse(word, out var slot))
            room = slot;
        else
            return GlobalUsage(who);
        if (Config.RoomsOff)
            return "This server's chat cannot be taken, so this world carries no room for the Dereth Network.";
        if (link != LinkState.Up)
            return "The Dereth Network's hub can't be reached now. Try again once the link is up.";
        if (!roomsTold)
            return "This network's hub does not keep where a world carries the Global room: this world carries it as GlobalRoom says.";
        var id = FreshId('g');
        roomsRequests.Add(RoomsWire.Write(new RoomsGlobal(id, RoomWord(room))));
        globalAsks[id] = who;
        return $"Asking the Dereth Network's hub to carry this world's Global room in {Shown(room)}...";
    }

    /// <summary>
    /// A local line in TurbineChat room `channel`, after it has gone to the local listeners. It goes
    /// to the network when the speaker is on the network and the room carries the Global room, or is
    /// one of the world's own rooms the hub mirrors that it has not lately said it does not carry.
    /// </summary>
    public void RoomSaid(uint player, uint channel, string text)
    {
        lock (gate)
        {
            if (CarriedRoom(channel) is not { } room || !IsOn(player))
                return;
            if (uncarried.TryGetValue(room, out var until) && Now < until)
                return;
            if (TooNew(player))
            {
                host.ShowSystem(player,
                    $"Your line did not reach {Reach(room)}: this character is too new to send to the Dereth Network yet.");
                return;
            }
            if (OwnAddress(player) is not { } from)
                return;
            if (!MaySend(player, Sends.RoomLines))
            {
                host.ShowSystem(player, RoomTooQuicklyLine(room));
                return;
            }
            var id = FreshId('r');
            pendingRooms[id] = new PendingRoom(player, room, null);
            Send(new RoomSay(id, room, from, text));
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
            if (GlobalSlot is not RoomSlot slot)
            {
                host.ShowSystem(player, NotCarriedLine);
                return;
            }
            text = text.Trim();
            if (text.Length == 0)
            {
                host.ShowSystem(player, Usage("@dng"));
                return;
            }
            if (!IsOn(player))
            {
                host.ShowSystem(player, NotOnLine);
                return;
            }
            host.SayInRoom(player, slot, text);
        }
    }

    /// <summary>
    /// How the hub took one of this world's room lines: only a refusal is news to the speaker. A
    /// `not_member` for one of the world's own rooms says the hub does not mirror it into Discord
    /// now (its operator has not mirrored it, or the hub mirrors nothing): the speaker is told
    /// nothing, and the room's lines stay here for <see cref="UncarriedSecs"/>.
    /// </summary>
    private void OnRoomResult(string id, Outcome outcome)
    {
        if (!pendingRooms.Remove(id, out var p))
            return;
        var own = p.Room != Wire.GlobalRoom;
        if (outcome == Outcome.RateLimited)
        {
            if (Online(p.Player))
                host.ShowSystem(p.Player, RoomHeldBackLine(p.Room));
        }
        else if (outcome == Outcome.NotMember && own)
        {
            if (!uncarried.ContainsKey(p.Room))
                log(LogLevel.Info,
                    $"the hub does not mirror this world's room {p.Room} into Discord; its lines stay here until the world's operator mirrors it on the hub's operator page");
            uncarried[p.Room] = Now + UncarriedSecs;
        }
        else if (outcome == Outcome.Ok && own)
            uncarried.Remove(p.Room);
    }

    /// <summary>What a speaker is told when its line stayed on this world because it sent too quickly.</summary>
    private static string RoomTooQuicklyLine(string room) => $"Your line did not reach {Reach(room)}: you are sending too quickly.";

    /// <summary>
    /// What a speaker is told when its line stayed on this world because the hub refused it: the
    /// speaker's own pace was kept here, so it is the world's share of the room that is spent.
    /// </summary>
    private static string RoomHeldBackLine(string room) =>
        $"Your line did not reach {Reach(room)}: the Dereth Network is holding back this world's lines for now.";

    /// <summary>
    /// A line from another world's character in the Global room; one its speaker's person said
    /// elsewhere (`via`) shows where beside the speaker's name (`Ann@coldeve (Discord)`), its stand-in
    /// id and squelches still the address's. A line in one of the world's own rooms, which nothing
    /// comes back into, is never shown.
    /// </summary>
    private void OnRoomLine(string room, Address from, string text, string? via)
    {
        if (GlobalSlot is not RoomSlot slot)
            return;
        if (room != Wire.GlobalRoom || Blocked(from) || from.World == Config.Slug)
            return;
        if (!host.RoomOpen(slot))
            return;
        var name = TextRules.SanitizeName(from.ToString());
        if (name is null)
            return;
        if (via is not null)
            name = $"{name} ({SayWire.MarkName(via)})";
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
