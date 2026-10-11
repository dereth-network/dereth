// The network's world half, hosted in ACE: who is online, what they are, and how to show them
// something, through ACE's own players, sessions and messages. Every call is on the world thread.

using ACE.Entity.Enum;
using ACE.Server.Managers;
using ACE.Server.Network;
using ACE.Server.Network.GameEvent;
using ACE.Server.Network.GameEvent.Events;
using ACE.Server.Network.GameMessages.Messages;
using ACE.Server.WorldObjects;

namespace Dereth.Network;

internal sealed class AceHost : IWorldHost
{
    private static Player? P(uint character) => PlayerManager.GetOnlinePlayer(character);

    private static Session? S(uint character) => P(character)?.Session;

    public string? OnlineName(uint character) => P(character)?.Name;

    public uint? OnlineByName(string name) => PlayerManager.GetOnlinePlayer(name)?.Guid.Full;

    public (uint Id, string Name)? FindCharacter(string name)
    {
        var p = PlayerManager.FindByName(name);
        return p is null ? null : (p.Guid.Full, p.Name);
    }

    public IReadOnlyList<uint> OnlineCharacters() => PlayerManager.GetAllOnline().Select(p => p.Guid.Full).ToList();

    public int OnlineCount() => PlayerManager.GetOnlineCount();

    public bool WorldOpen() => WorldManager.WorldStatus == WorldManager.WorldStatusState.Open;

    public int? AgeSeconds(uint character) => P(character)?.Age;

    public bool AppearsOffline(uint character) => P(character)?.GetAppearOffline() ?? false;

    public bool IsGagged(uint character) => P(character)?.IsGagged ?? false;

    public void ShowGagged(uint character) => P(character)?.SendGagError();

    public string? AwayMessage(uint character)
    {
        var p = P(character);
        if (p is null || !p.IsAfk)
            return null;
        return string.IsNullOrWhiteSpace(p.AfkMessage) ? Player.DefaultAFKMessage : p.AfkMessage;
    }

    public string DefaultAwayText => Player.DefaultAFKMessage;

    private static CharacterOption ListenOption(RoomSlot slot) => slot switch
    {
        RoomSlot.General => CharacterOption.ListenToGeneralChat,
        RoomSlot.Trade => CharacterOption.ListenToTradeChat,
        RoomSlot.Lfg => CharacterOption.ListenToLFGChat,
        _ => CharacterOption.ListenToRoleplayChat,
    };

    private static ChatType ChatTypeOf(RoomSlot slot) => slot switch
    {
        RoomSlot.General => ChatType.General,
        RoomSlot.Trade => ChatType.Trade,
        RoomSlot.Lfg => ChatType.LFG,
        _ => ChatType.Roleplay,
    };

    public bool ListensToRoom(uint character, RoomSlot slot)
    {
        var p = P(character);
        return p is not null && !p.IsOlthoiPlayer && p.GetCharacterOption(ListenOption(slot));
    }

    public bool RoomOpen(RoomSlot slot)
    {
        var disabled = slot switch
        {
            RoomSlot.General => "chat_disable_general",
            RoomSlot.Trade => "chat_disable_trade",
            RoomSlot.Lfg => "chat_disable_lfg",
            _ => "chat_disable_roleplay",
        };
        return PropertyManager.GetBool("use_turbine_chat").Item && !PropertyManager.GetBool(disabled).Item;
    }

    public void ShowTell(uint character, string senderName, uint senderId, string text)
    {
        var s = S(character);
        s?.Network.EnqueueSend(new GameEventTell(s, text, senderName, senderId, character, ChatMessageType.Tell));
    }

    public void ShowEcho(uint character, string to, string text) =>
        S(character)?.Network.EnqueueSend(new GameMessageSystemChat($"You tell {to}, \"{text}\"", ChatMessageType.OutgoingTell));

    public void ShowNotAvailable(uint character)
    {
        var s = S(character);
        s?.Network.EnqueueSend(new GameEventWeenieError(s, WeenieError.CharacterNotAvailable));
    }

    /// <summary>The error the retail client gives for a full friends list.</summary>
    public void ShowFriendsFull(uint character)
    {
        var s = S(character);
        s?.Network.EnqueueSend(new GameEventWeenieError(s, WeenieError.MaxFriendsExceeded));
    }

    public void ShowAway(uint character, string text)
    {
        var s = S(character);
        s?.Network.EnqueueSend(new GameEventWeenieErrorWithString(s, WeenieErrorWithString.AFK, text));
    }

    /// <summary>As ACE answers a tell to a local character that squelches the sender: the echo, then the refusal.</summary>
    public void ShowSquelched(uint character, string to, string text)
    {
        var s = S(character);
        if (s is null)
            return;
        ShowEcho(character, to, text);
        s.Network.EnqueueSend(new GameEventWeenieErrorWithString(s, WeenieErrorWithString.MessageBlocked_, $"{to} has you squelched."));
    }

    public void ShowSystem(uint character, string text) =>
        S(character)?.Network.EnqueueSend(new GameMessageSystemChat(text, ChatMessageType.Broadcast));

    public void ShowFriendRow(uint character, FriendRowKind kind, uint id, string name, bool online)
    {
        var s = S(character);
        s?.Network.EnqueueSend(new FriendRowEvent(s, kind, id, name, online));
    }

    public void ShowRoomLine(uint character, RoomSlot slot, string senderName, uint senderId, string text) =>
        S(character)?.Network.EnqueueSend(new GameMessageTurbineChat(ChatNetworkBlobType.NETBLOB_EVENT_BINARY,
            ChatNetworkBlobDispatchType.ASYNCMETHOD_SENDTOROOMBYNAME, RoomSlots.Channel(slot), senderName, text, senderId,
            ChatTypeOf(slot)));

    /// <summary>
    /// A line in the room in `slot`, through ACE's own room handling as if the client had sent it:
    /// ACE's room rules decide who hears it, and the Global room's patch hands it to the network.
    /// </summary>
    public void SayInRoom(uint character, RoomSlot slot, string text)
    {
        var p = P(character);
        if (p?.Session is not { } session)
            return;
        var bytes = Payload.TurbineRoomRequest(RoomSlots.Channel(slot), (uint)ChatTypeOf(slot), character, text);
        ACE.Server.Network.Handlers.TurbineChatHandler.TurbineChatReceived(new ClientMessage(bytes), session);
    }

    /// <summary>A local tell to `name`, answered exactly as ACE answers `@tell name, text`.</summary>
    public void LocalTell(uint character, string name, string text)
    {
        var p = P(character);
        var session = p?.Session;
        if (p is null || session is null)
            return;
        var target = PlayerManager.GetOnlinePlayer(name);
        if (target is null)
        {
            session.Network.EnqueueSend(new GameEventWeenieError(session, WeenieError.CharacterNotAvailable));
            return;
        }
        if (p != target)
            session.Network.EnqueueSend(new GameMessageSystemChat($"You tell {target.Name}, \"{text}\"", ChatMessageType.OutgoingTell));
        if (target.SquelchManager.Squelches.Contains(p, ChatMessageType.Tell))
        {
            session.Network.EnqueueSend(new GameEventWeenieErrorWithString(session, WeenieErrorWithString.MessageBlocked_, $"{name} has you squelched."));
            return;
        }
        if (target.IsAfk)
            session.Network.EnqueueSend(new GameEventWeenieErrorWithString(session, WeenieErrorWithString.AFK,
                $"{target.Name} is away: " + (string.IsNullOrWhiteSpace(target.AfkMessage) ? Player.DefaultAFKMessage : target.AfkMessage)));
        target.Session.Network.EnqueueSend(new GameEventTell(target.Session, text, p.GetNameWithSuffix(), p.Guid.Full, target.Guid.Full, ChatMessageType.Tell));
    }

    public void LocalAddFriend(uint character, string name) => P(character)?.HandleActionAddFriend(name);
}

/// <summary>`Social_FriendsUpdate` (`0x0021`) with one row, for a remote friend under its stand-in id.</summary>
internal sealed class FriendRowEvent : GameEventMessage
{
    public FriendRowEvent(Session session, FriendRowKind kind, uint id, string name, bool online)
        : base(GameEventType.FriendsListUpdate, GameMessageGroup.UIQueue, session)
    {
        Writer.Write(1u);
        Payload.WriteFriendRow(Writer, id, name, online);
        Writer.Write((uint)kind);
    }
}
