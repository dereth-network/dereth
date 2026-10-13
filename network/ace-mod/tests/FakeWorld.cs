// A world without ACE: a list of characters, and a record of everything shown to them. It hosts the
// mod's own world half, so the harness drives exactly the code the mod runs inside ACE.

namespace Dereth.Network.Harness;

internal sealed record Shown(uint To, string Kind, string Name = "", uint Id = 0, string Text = "", bool Online = false,
    FriendRowKind Row = 0, RoomSlot? Room = null);

internal sealed class FakeCharacter
{
    public required uint Id { get; init; }
    public required string Name { get; set; }
    public bool Online { get; set; } = true;
    public int? Age { get; set; } = 100_000;
    public bool AppearOffline { get; set; }
    public string? Away { get; set; }
    public bool ListensToRoom { get; set; } = true;
    /// <summary>The rooms it listens to, while it listens at all; every room when null.</summary>
    public HashSet<RoomSlot>? Rooms { get; set; }
    public bool Gagged { get; set; }
    public bool Banned { get; set; }
    /// <summary>An Olthoi player, whom the world's rooms every player may use do not take.</summary>
    public bool Olthoi { get; set; }
    /// <summary>The characters of this world it squelches in every room, by id.</summary>
    public HashSet<uint> SquelchedLocals { get; } = [];

    // What the directory's profile is made of.
    public uint Level { get; set; } = 10;
    public Seeable Values { get; set; }
    public OthersSee Allowed { get; set; }
    /// <summary>The character's patron, by id; none for a monarch or a character in no allegiance.</summary>
    public uint? Patron { get; set; }
    /// <summary>The name its monarch gave its allegiance (kept on the monarch); none for an unnamed one.</summary>
    public string? AllegianceName { get; set; }
    public bool Deleted { get; set; }
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

    public bool IsBanned(uint character) => Characters.TryGetValue(character, out var c) && c.Banned;

    public bool SquelchesLocal(uint listener, uint speaker) =>
        Characters.TryGetValue(listener, out var c) && c.SquelchedLocals.Contains(speaker);

    public string? RoomRefusal(uint character, RoomSlot slot) =>
        Characters.TryGetValue(character, out var c) && c.Olthoi ? SayReasons.NotAllowed : null;

    public void ShowGagged(uint character) => Shown.Add(new(character, "gagged"));

    public string? AwayMessage(uint character) =>
        Characters.TryGetValue(character, out var c) && c.Away is not null
            ? string.IsNullOrWhiteSpace(c.Away) ? DefaultAwayText : c.Away
            : null;

    public string DefaultAwayText => "I am currently away from the keyboard.";

    public bool ListensToRoom(uint character, RoomSlot slot) =>
        Characters.TryGetValue(character, out var c) && c.ListensToRoom && (c.Rooms is null || c.Rooms.Contains(slot));

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
        Shown.Add(new(character, "room", senderName, senderId, text, Room: slot));

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

    // ---- the directory -------------------------------------------------------------------------

    public CharacterFacts? Facts(uint character) =>
        Characters.TryGetValue(character, out var c) && c.Online && !c.Deleted ? FactsOf(character) : null;

    /// <summary>A character's facts whether it is in the world or not, as a host has them for one just logged out.</summary>
    public CharacterFacts FactsOf(uint character)
    {
        var c = Characters[character];
        return new CharacterFacts(new Profile { Level = c.Level, Heritage = 1, Gender = 2, Title = "the Bloodletter" }, c.Values, c.Allowed,
            AllegianceOf(character));
    }

    /// <summary>The top of `character`'s chain of patrons.</summary>
    private uint MonarchOf(uint character)
    {
        var at = character;
        for (var i = 0; i < 100 && Characters.TryGetValue(at, out var c) && c.Patron is uint p; i++)
            at = p;
        return at;
    }

    /// <summary>A character's rank: one more than its highest-ranked vassal's, 1 with none.</summary>
    private uint RankOf(uint character) =>
        1 + Characters.Values.Where(v => v.Patron == character).Select(v => RankOf(v.Id)).DefaultIfEmpty(0u).Max();

    public AllegianceFacts? AllegianceOf(uint character)
    {
        if (!Characters.TryGetValue(character, out var c) || c.Deleted)
            return null;
        var monarch = MonarchOf(character);
        var members = Characters.Values.Count(x => !x.Deleted && MonarchOf(x.Id) == monarch);
        if (members < 2)
            return null;
        return new AllegianceFacts(monarch, c.Patron, Characters.GetValueOrDefault(monarch)?.AllegianceName, (uint)members,
            RankOf(character));
    }

    public IReadOnlyList<uint> Followers(uint character)
    {
        if (!Characters.TryGetValue(character, out var c))
            return [];
        return c.Patron is null
            ? Characters.Values.Where(x => x.Id != character && MonarchOf(x.Id) == character).Select(x => x.Id).ToList()
            : Characters.Values.Where(x => x.Patron == character).Select(x => x.Id).ToList();
    }

    public string? NameOf(uint character) =>
        Characters.TryGetValue(character, out var c) && !c.Deleted ? c.Name : null;
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

    /// <summary>
    /// The link messages sent, one for each frame, so that a count of <see cref="Sent"/> marks a place
    /// in it: a frame of another family (the directory's, the reach answers) is an <see cref="OtherFrame"/>.
    /// </summary>
    public List<WireMessage> SentMessages() =>
        Sent.Select(s => Wire.Read(s.Frame) is { Status: ReadStatus.Ok } r ? r.Message! : new OtherFrame(s.Frame)).ToList();

    /// <summary>The directory's messages sent from the frame `from` on.</summary>
    public List<DirectoryMessage> SentListings(int from = 0) =>
        Sent.Skip(from).Select(s => DirectoryWire.Read(s.Frame)).Where(r => r.Status == ReadStatus.Ok).Select(r => r.Message!).ToList();
}

/// <summary>A frame the link carried that is not one of the link messages: the directory's, or a reach answer.</summary>
internal sealed record OtherFrame(string Frame) : WireMessage
{
    public override string Type => "(another family)";
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

    /// <summary>
    /// A world carrying the Global room in `room` until a hub says otherwise (`named`: its settings
    /// name it), whose own rooms `mirrored` a hub last said it mirrors into Discord.
    /// </summary>
    public static TestWorld Make(string slug, ILink? link, Func<double> clock, RoomSlot? room = RoomSlot.Roleplay,
        List<string>? sharedLog = null, NetworkStore? store = null, uint minAge = 0,
        IReadOnlyList<RoomSlot>? mirrored = null, bool named = false)
    {
        var host = new FakeHost();
        var logs = new List<string>();
        TestWorld? tw = null;
        store ??= new NetworkStore();
        if (mirrored is { Count: > 0 })
            store.Rooms = new KeptRooms(store.Rooms?.Global, mirrored.Select(RoomSlots.Room).ToList());
        var world = new NetworkWorld(
            new WorldConfig
            {
                Slug = slug, GlobalRoom = room, GlobalRoomNamed = named, Software = "Dereth.Network harness",
                MinCharacterAgeSeconds = minAge,
            },
            store, host,
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
