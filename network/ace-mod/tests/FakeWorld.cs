// A world without ACE: a list of characters, and a record of everything shown to them. It hosts the
// mod's own world half, so the harness drives exactly the code the mod runs inside ACE.

namespace Dereth.Network.Harness;

internal sealed record Shown(uint To, string Kind, string Name = "", uint Id = 0, string Text = "", bool Online = false,
    FriendRowKind Row = 0);

internal sealed class FakeCharacter
{
    public required uint Id { get; init; }
    public required string Name { get; init; }
    public bool Online { get; set; } = true;
    public int? Age { get; set; } = 100_000;
    public bool AppearOffline { get; set; }
    public string? Away { get; set; }
    public bool ListensToRoom { get; set; } = true;
    public bool Gagged { get; set; }
}

internal sealed class FakeHost : IWorldHost
{
    public readonly Dictionary<uint, FakeCharacter> Characters = new();
    public readonly List<Shown> Shown = [];

    /// <summary>The world half this host carries, for the lines its rooms let through.</summary>
    public NetworkWorld? World { get; set; }

    public FakeCharacter Add(uint id, string name)
    {
        var c = new FakeCharacter { Id = id, Name = name };
        Characters[id] = c;
        return c;
    }

    public List<Shown> To(uint character) => Shown.Where(s => s.To == character).ToList();

    public string? OnlineName(uint character) =>
        Characters.TryGetValue(character, out var c) && c.Online ? c.Name : null;

    public uint? OnlineByName(string name) =>
        Characters.Values.FirstOrDefault(c => c.Online && string.Equals(c.Name, name, StringComparison.OrdinalIgnoreCase))?.Id;

    public (uint Id, string Name)? FindCharacter(string name)
    {
        var c = Characters.Values.FirstOrDefault(c => string.Equals(c.Name, name, StringComparison.OrdinalIgnoreCase));
        return c is null ? null : (c.Id, c.Name);
    }

    public IReadOnlyList<uint> OnlineCharacters() => Characters.Values.Where(c => c.Online).Select(c => c.Id).ToList();

    public int OnlineCount() => Characters.Values.Count(c => c.Online);

    public bool WorldOpen() => true;

    public int? AgeSeconds(uint character) => Characters.TryGetValue(character, out var c) ? c.Age : null;

    public bool AppearsOffline(uint character) => Characters.TryGetValue(character, out var c) && c.AppearOffline;

    public bool IsGagged(uint character) => Characters.TryGetValue(character, out var c) && c.Gagged;

    public void ShowGagged(uint character) => Shown.Add(new(character, "gagged"));

    public string? AwayMessage(uint character) =>
        Characters.TryGetValue(character, out var c) && c.Away is not null
            ? string.IsNullOrWhiteSpace(c.Away) ? DefaultAwayText : c.Away
            : null;

    public string DefaultAwayText => "I am currently away from the keyboard.";

    public bool ListensToRoom(uint character, RoomSlot slot) => Characters.TryGetValue(character, out var c) && c.ListensToRoom;

    public bool RoomOpen(RoomSlot slot) => true;

    public void ShowTell(uint character, string senderName, uint senderId, string text) =>
        Shown.Add(new(character, "tell", senderName, senderId, text));

    public void ShowEcho(uint character, string to, string text) => Shown.Add(new(character, "echo", to, 0, text));

    public void ShowNotAvailable(uint character) => Shown.Add(new(character, "not-available"));

    public void ShowFriendsFull(uint character) => Shown.Add(new(character, "friends-full"));

    public void ShowAway(uint character, string text) => Shown.Add(new(character, "away", "", 0, text));

    // As ACE answers a tell to a local character that squelches the sender.
    public void ShowSquelched(uint character, string to, string text)
    {
        Shown.Add(new(character, "echo", to, 0, text));
        Shown.Add(new(character, "squelched", to, 0, $"{to} has you squelched."));
    }

    public void ShowSystem(uint character, string text) => Shown.Add(new(character, "system", "", 0, text));

    public void ShowFriendRow(uint character, FriendRowKind kind, uint id, string name, bool online) =>
        Shown.Add(new(character, "friend", name, id, "", online, kind));

    public void ShowRoomLine(uint character, RoomSlot slot, string senderName, uint senderId, string text) =>
        Shown.Add(new(character, "room", senderName, senderId, text));

    // As a world's room handling takes a line: a gagged speaker is refused, every listener is shown
    // it, and the line goes on to the network half.
    public void SayInRoom(uint character, RoomSlot slot, string text)
    {
        if (IsGagged(character))
        {
            ShowGagged(character);
            return;
        }
        var name = OnlineName(character) ?? "";
        foreach (var listener in OnlineCharacters().Where(c => ListensToRoom(c, slot)))
            ShowRoomLine(listener, slot, name, character, text);
        World?.RoomSaid(character, RoomSlots.Channel(slot), text);
    }

    public void LocalTell(uint character, string name, string text) => Shown.Add(new(character, "local-tell", name, 0, text));

    public void LocalAddFriend(uint character, string name) => Shown.Add(new(character, "local-friend", name));
}

/// <summary>A link in memory: what the world sends is kept, what the test hands it is received.</summary>
internal sealed class MemoryLink : ILink
{
    private readonly Queue<LinkEvent> events = new();
    public readonly List<(long Epoch, string Frame)> Sent = [];

    public void Up(long epoch) => events.Enqueue(new LinkEvent(LinkEventKind.Up, epoch, "", 0));

    public void Down(long epoch) => events.Enqueue(new LinkEvent(LinkEventKind.Down, epoch, "", 0));

    public void Frame(long epoch, WireMessage m) => events.Enqueue(new LinkEvent(LinkEventKind.Frame, epoch, Wire.Write(m), 0));

    public void FrameText(long epoch, string text) => events.Enqueue(new LinkEvent(LinkEventKind.Frame, epoch, text, 0));

    public bool TryTake(out LinkEvent e) => events.TryDequeue(out e);

    public void Send(long epoch, string frame) => Sent.Add((epoch, frame));

    public List<WireMessage> SentMessages() => Sent.Select(s => Wire.Read(s.Frame).Message!).ToList();
}

/// <summary>One world under test: its host, its world half, its link and everything it logged.</summary>
internal sealed class TestWorld
{
    public required string Slug { get; init; }
    public required FakeHost Host { get; init; }
    public required NetworkWorld World { get; init; }
    public required ILink Link { get; set; }
    public required List<string> Logs { get; init; }
    public string? Saved;

    public void Pump() => World.Pump(Link);

    public static TestWorld Make(string slug, ILink? link, Func<double> clock, RoomSlot? room = RoomSlot.Roleplay,
        List<string>? sharedLog = null, NetworkStore? store = null, uint minAge = 0)
    {
        var host = new FakeHost();
        var logs = new List<string>();
        TestWorld? tw = null;
        var world = new NetworkWorld(
            new WorldConfig { Slug = slug, GlobalRoom = room, Software = "Dereth.Network harness", MinCharacterAgeSeconds = minAge },
            store ?? new NetworkStore(), host,
            (l, m) =>
            {
                lock (logs)
                    logs.Add($"{l}: {m}");
                if (sharedLog is not null)
                    lock (sharedLog)
                        sharedLog.Add($"[{slug}] {l}: {m}");
            },
            clock, 1_790_000_000, s => { if (tw is not null) tw.Saved = s.ToJson(); });
        host.World = world;
        tw = new TestWorld { Slug = slug, Host = host, World = world, Link = link ?? new MemoryLink(), Logs = logs };
        return tw;
    }
}
