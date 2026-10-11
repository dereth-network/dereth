// What the network's world half needs of the world it runs in: who is here, what they are, and how
// to show them something. The ACE side implements it over ACE's players and sessions; a test
// implements it over a list. Every call is made on the world thread.

namespace Dereth.Network;

/// <summary>A friends-list row's kind, as the client reads it.</summary>
public enum FriendRowKind : uint
{
    Added = 0x1,
    Removed = 0x2,
    Status = 0x4,
}

public interface IWorldHost
{
    /// <summary>The name of a character online now, or null.</summary>
    string? OnlineName(uint character);

    /// <summary>The character online under `name` (matched without regard to case), or null.</summary>
    uint? OnlineByName(string name);

    /// <summary>A character of this world by name, online or not: its id and name, or null.</summary>
    (uint Id, string Name)? FindCharacter(string name);

    /// <summary>Every character online now.</summary>
    IReadOnlyList<uint> OnlineCharacters();

    int OnlineCount();

    /// <summary>Whether the world is open to ordinary logins.</summary>
    bool WorldOpen();

    /// <summary>Seconds the character has played, if the world knows.</summary>
    int? AgeSeconds(uint character);

    /// <summary>Whether the character chose to appear offline.</summary>
    bool AppearsOffline(uint character);

    /// <summary>Whether the character is gagged (may not chat).</summary>
    bool IsGagged(uint character) => false;

    /// <summary>What the world shows a gagged character that tries to talk.</summary>
    void ShowGagged(uint character) =>
        ShowSystem(character, "You are unable to talk locally, globally, or send tells because you have been gagged.");

    /// <summary>The character's away text when it is away (the default text if it set none), else null.</summary>
    string? AwayMessage(uint character);

    /// <summary>The away text a character that set none shows.</summary>
    string DefaultAwayText { get; }

    /// <summary>Whether the character listens to the room in `slot` (and may hear it at all).</summary>
    bool ListensToRoom(uint character, RoomSlot slot);

    /// <summary>Whether the world has the room in `slot` open.</summary>
    bool RoomOpen(RoomSlot slot);

    /// <summary>A tell to the character from `senderName` under `senderId`.</summary>
    void ShowTell(uint character, string senderName, uint senderId, string text);

    /// <summary>The outgoing-tell echo: `You tell {to}, "{text}"`.</summary>
    void ShowEcho(uint character, string to, string text);

    /// <summary>`Character not available`.</summary>
    void ShowNotAvailable(uint character);

    /// <summary>The away message a tell's recipient gave.</summary>
    void ShowAway(uint character, string text);

    /// <summary>What the world shows a sender whom a local character squelches.</summary>
    void ShowSquelched(uint character, string to, string text);

    /// <summary>A line of the server's own text.</summary>
    void ShowSystem(uint character, string text);

    /// <summary>What the world shows a character whose friends list is full.</summary>
    void ShowFriendsFull(uint character) => ShowSystem(character, "Your friends list is full.");

    /// <summary>One friends-list row.</summary>
    void ShowFriendRow(uint character, FriendRowKind kind, uint id, string name, bool online);

    /// <summary>A room line in the room in `slot`.</summary>
    void ShowRoomLine(uint character, RoomSlot slot, string senderName, uint senderId, string text);

    /// <summary>
    /// A line the character says in the room in `slot`, taken exactly as the client's own line there:
    /// shown to the room's local listeners under the room's rules, then handed to
    /// <see cref="NetworkWorld.RoomSaid"/> as any line the world let through.
    /// </summary>
    void SayInRoom(uint character, RoomSlot slot, string text);

    /// <summary>A tell to the local character `name`, answered as the world answers any local tell.</summary>
    void LocalTell(uint character, string name, string text);

    /// <summary>Adds the local character `name` to the character's own friends list, as the world does.</summary>
    void LocalAddFriend(uint character, string name);
}
