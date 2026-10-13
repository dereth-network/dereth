// The network's world half, hosted in ACE: who is online, what they are, and how to show them
// something, through ACE's own players, sessions and messages. Every call is on the world thread.
// Beside it, the two game messages the mod writes itself.

using ACE.Entity.Enum;
using ACE.Entity.Enum.Properties;
using ACE.Server.Entity;
using ACE.Server.Managers;
using ACE.Server.Network;
using ACE.Server.Network.GameEvent;
using ACE.Server.Network.GameEvent.Events;
using ACE.Server.Network.GameMessages;
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

    /// <summary>A character of this world, in the world or not; null for one ACE does not have.</summary>
    private static IPlayer? Anyone(uint character) => PlayerManager.FindByGuid(character);

    public int? AgeSeconds(uint character) => P(character)?.Age ?? Anyone(character)?.GetProperty(PropertyInt.Age);

    public bool AppearsOffline(uint character) => P(character)?.GetAppearOffline() ?? false;

    public bool IsGagged(uint character) => P(character)?.IsGagged ?? Anyone(character)?.GetProperty(PropertyBool.IsGagged) ?? false;

    /// <summary>As ACE refuses a login: the account's ban not yet over.</summary>
    public bool IsBanned(uint character) =>
        Anyone(character)?.Account is { BanExpireTime: DateTime until } && DateTime.UtcNow < until;

    /// <summary>
    /// ACE's own rules for a line in a room every player may use, as its room handling applies them
    /// to a line the character says itself, in the world or not: an Olthoi player may not speak
    /// there; a room ACE only echoes back to its speaker carries no line; and the account's age, the
    /// character's time played and its level that ACE's settings ask for.
    /// </summary>
    public string? RoomRefusal(uint character, RoomSlot slot)
    {
        if (((IPlayer?)P(character) ?? Anyone(character)) is not { } p)
            return SayReasons.NoCharacter;
        if (p.Heritage is int h && (h == (int)HeritageGroup.Olthoi || h == (int)HeritageGroup.OlthoiAcid))
            return SayReasons.NotAllowed;
        if (PropertyManager.GetBool("chat_echo_only").Item)
            return SayReasons.RoomOff;
        var accountAge = p.Account is { } account ? DateTime.UtcNow - account.CreateTime : TimeSpan.Zero;
        if (PropertyManager.GetBool("chat_requires_account_15days").Item
            && !(p.GetProperty(PropertyBool.Account15Days) ?? false) && accountAge.TotalDays < 15)
            return SayReasons.TooNew;
        var accountSeconds = PropertyManager.GetLong("chat_requires_account_time_seconds").Item;
        if (accountSeconds > 0 && accountAge.TotalSeconds < accountSeconds)
            return SayReasons.TooNew;
        var played = PropertyManager.GetLong("chat_requires_player_age").Item;
        if (played > 0 && (AgeSeconds(character) ?? 0) < played)
            return SayReasons.TooNew;
        var level = PropertyManager.GetLong("chat_requires_player_level").Item;
        if (level > 0 && (p.Level ?? 0) < level)
            return SayReasons.NotAllowed;
        return null;
    }

    /// <summary>
    /// As ACE's own room handling asks of a local line's listener: a squelch of the speaker's account
    /// or character in every room, or of everyone; for a speaker in the world, ACE's own check.
    /// </summary>
    public bool SquelchesLocal(uint listener, uint speaker)
    {
        if (P(listener)?.SquelchManager.Squelches is not { } squelches)
            return false;
        if (P(speaker) is { } online)
            return squelches.Contains(online, ChatMessageType.AllChannels);
        var mask = ChatMessageType.AllChannels.ToMask();
        if (Anyone(speaker)?.Account?.AccountName is { } account && squelches.Accounts.ContainsKey(account))
            return true;
        if (squelches.Characters.TryGetValue(speaker, out var info) && info.Filters.Count > 0 && info.Filters[0].HasFlag(mask))
            return true;
        return squelches.Globals.Filters.Count > 0 && squelches.Globals.Filters[0].HasFlag(mask);
    }

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

    // ---- the directory -------------------------------------------------------------------------

    public CharacterFacts? Facts(uint character)
    {
        if (P(character) is not { } p)
            return null;
        try
        {
            return FactsOf(p);
        }
        catch (Exception e)
        {
            // A character the mod cannot read is not listed, and the rest of the network carries on.
            Mod.Fault("a profile for the directory", e, LogLevel.Warn);
            return null;
        }
    }

    /// <summary>
    /// A player's facts for its profile in the directory, all without the effects of spells, as ACE
    /// keeps them; null for one without a level, heritage or gender.
    /// </summary>
    public static CharacterFacts? FactsOf(Player p)
    {
        if (p.Level is not int level || level < 1 || p.Heritage is not int heritage || heritage < 1 || p.Gender is not int gender
            || gender < 1)
            return null;
        static ulong? Amount(long? v) => v is long n && n >= 0 ? (ulong)n : null;
        static uint? Count(int? v) => v is int n && n >= 0 ? (uint)n : null;
        string? title = p.CharacterTitleId is int t && t > 0 ? p.GetTitle((CharacterTitle)t) : null;
        if (title is not null && !DirectoryWire.IsShownText(title, DirectoryWire.MaxTitleChars))
            title = null;
        AttributeValue? A(PropertyAttribute a) => p.Attributes.TryGetValue(a, out var x) ? new AttributeValue(x.StartingValue, x.Base) : null;
        uint? V(PropertyAttribute2nd v) => p.Vitals.TryGetValue(v, out var x) ? x.Base : null;
        Attributes? attributes = A(PropertyAttribute.Strength) is { } str && A(PropertyAttribute.Endurance) is { } end
            && A(PropertyAttribute.Coordination) is { } coo && A(PropertyAttribute.Quickness) is { } qui
            && A(PropertyAttribute.Focus) is { } foc && A(PropertyAttribute.Self) is { } sel
            ? new Attributes(str, end, coo, qui, foc, sel)
            : null;
        Vitals? vitals = V(PropertyAttribute2nd.MaxHealth) is uint h && V(PropertyAttribute2nd.MaxStamina) is uint st
            && V(PropertyAttribute2nd.MaxMana) is uint m
            ? new Vitals(h, st, m)
            : null;
        var skills = p.Skills
            .Where(s => (int)s.Key is >= 1 and <= 255
                && s.Value.AdvancementClass is SkillAdvancementClass.Untrained or SkillAdvancementClass.Trained
                    or SkillAdvancementClass.Specialized)
            .Select(s => new ProfileSkill((uint)s.Key, s.Value.AdvancementClass switch
            {
                SkillAdvancementClass.Untrained => Training.Untrained,
                SkillAdvancementClass.Trained => Training.Trained,
                _ => Training.Specialized,
            }, s.Value.Base))
            .OrderBy(s => s.Id)
            .ToList();
        var augmentations = NetworkWorld.Augmentations
            .Select(n => (Id: (uint)n, Times: p.GetProperty((PropertyInt)n)))
            .Where(x => x.Times is >= 1 and <= (int)DirectoryWire.MaxAugmentationTimes)
            .Select(x => new ProfileAugmentation(x.Id, (uint)x.Times!.Value))
            .ToList();
        var own = new Profile
        {
            Level = (uint)level,
            Heritage = (uint)heritage,
            Gender = (uint)gender,
            Title = title,
            Experience = Amount(p.TotalExperience) is ulong total ? (total, Amount(p.AvailableExperience) ?? 0) : null,
            Credits = Count(p.AvailableSkillCredits),
            Luminance = Amount(p.MaximumLuminance) is ulong max ? (Amount(p.AvailableLuminance) ?? 0, max) : null,
            Attributes = attributes,
            Vitals = vitals,
            Skills = skills,
            Augmentations = augmentations,
        };
        var values = new Seeable(
            Amount(p.GetProperty(PropertyInt.CreationTimestamp)),
            Amount(p.GetProperty(PropertyInt.Age)),
            Count(p.GetProperty(PropertyInt.ChessRank)),
            Count(p.GetProperty(PropertyInt.FakeFishingSkill)),
            Count(p.GetProperty(PropertyInt.NumDeaths)),
            Count(p.GetProperty(PropertyInt.NumCharacterTitles)));
        // The test ACE's appraisal makes before it shows each field to another player.
        var allowed = new OthersSee(
            p.GetCharacterOption(CharacterOption.AllowOthersToSeeYourDateOfBirth),
            p.GetCharacterOption(CharacterOption.AllowOthersToSeeYourAge),
            p.GetCharacterOption(CharacterOption.AllowOthersToSeeYourChessRank),
            p.GetCharacterOption(CharacterOption.AllowOthersToSeeYourFishingSkill),
            p.GetCharacterOption(CharacterOption.AllowOthersToSeeYourNumberOfDeaths),
            p.GetCharacterOption(CharacterOption.AllowOthersToSeeYourNumberOfTitles));
        return new CharacterFacts(own, values, allowed, AllegianceOf(p));
    }

    public AllegianceFacts? AllegianceOf(uint character) =>
        PlayerManager.FindByGuid(character) is { } p ? AllegianceOf(p) : null;

    /// <summary>
    /// A player's allegiance as ACE has it loaded: its name only as the monarch set it (the
    /// allegiance's own property, never the monarch's name ACE shows in its place).
    /// </summary>
    private static AllegianceFacts? AllegianceOf(IPlayer p)
    {
        var node = AllegianceManager.GetAllegianceNode(p);
        if (node?.Allegiance is not { } a || node.Monarch is null)
            return null;
        return new AllegianceFacts(node.Monarch.PlayerGuid.Full, node.Patron?.PlayerGuid.Full, a.AllegianceName,
            (uint)Math.Max(0, a.TotalMembers), node.Rank);
    }

    public IReadOnlyList<uint> Followers(uint character)
    {
        if (PlayerManager.FindByGuid(character) is not { } p || AllegianceManager.GetAllegianceNode(p) is not { } node)
            return [];
        if (node.IsMonarch)
            return node.Allegiance?.Members?.Keys.Select(g => g.Full).Where(g => g != character).ToList() ?? [];
        return node.Vassals?.Values.Select(v => v.PlayerGuid.Full).ToList() ?? [];
    }

    public string? NameOf(uint character) =>
        PlayerManager.FindByGuid(character) is { } p && !p.IsDeleted && !p.IsPendingDeletion ? p.Name : null;
}

/// <summary>`Login_CharacterScreenMessage` (`0xF65A`): the world's message on the character screen.</summary>
internal sealed class GameMessageCharacterScreenMessage : GameMessage
{
    public GameMessageCharacterScreenMessage(string text)
        : base((GameMessageOpcode)Payload.CharacterScreenMessageOpcode, GameMessageGroup.UIQueue)
    {
        Payload.WriteCharacterScreenMessage(Writer, text);
    }
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
