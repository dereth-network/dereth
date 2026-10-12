// The ACE methods the mod patches, named rather than referenced, and the one resolver that finds
// them. The mod resolves each feature's targets in the running server before it patches anything;
// a feature with a target the server does not have (a fork renamed it) stays off and the server
// behaves for it as it always did. The same table and resolver run against a server's assembly on
// disk, so a build can be checked before it is installed.

using System.Reflection;

namespace Dereth.Network;

/// <summary>
/// One method or constructor to patch: its type, name and parameter types, by full name, and the
/// names of the mod's prefix and postfix for it.
/// </summary>
public sealed record PatchTarget(string Feature, string Type, string Member, string[] Parameters, string? Prefix,
    string? Postfix)
{
    public bool IsConstructor => Member == ".ctor";

    public override string ToString() => $"{Type}.{Member}({string.Join(", ", Parameters)})";
}

public static class PatchTargets
{
    public const string Core = "core";
    public const string Tells = "tells";
    public const string Replies = "replies";
    public const string Friends = "friends";
    public const string AppearOffline = "appear-offline";
    public const string Squelch = "squelch";
    public const string GlobalRoom = "global-room";
    public const string Names = "names";
    public const string Verify = "verify";

    /// <summary>The character screen's message: the world's own, patched whether or not the network starts.</summary>
    public const string CharacterScreen = "character-screen";

    private const string Session = "ACE.Server.Network.Session";
    private const string ClientMessage = "ACE.Server.Network.ClientMessage";
    private const string Player = "ACE.Server.WorldObjects.Player";
    private const string PlayerManager = "ACE.Server.Managers.PlayerManager";

    /// <summary>Every target, by feature. A feature is on only when all of its targets resolve.</summary>
    public static readonly IReadOnlyList<PatchTarget> All =
    [
        // The world-thread pump: without it nothing else can run.
        new(Core, PlayerManager, "Tick", [], null, "TickPostfix"),
        // Tells by name, and replies by a stand-in id.
        new(Tells, "ACE.Server.Network.GameAction.Actions.GameActionTell", "Handle", [ClientMessage, Session], "TellPrefix", null),
        new(Replies, "ACE.Server.Network.GameAction.Actions.GameActionTalkDirect", "Handle", [ClientMessage, Session],
            "TalkDirectPrefix", null),
        // Remote friends, their rows, and the logins and logouts that drive their status. The
        // client's requests are taken where ACE dispatches them, which the runtime never inlines.
        new(Friends, "ACE.Server.Network.GameAction.Actions.GameActionAddFriend", "Handle", [ClientMessage, Session],
            "AddFriendPrefix", null),
        new(Friends, "ACE.Server.Network.GameAction.Actions.GameActionRemoveFriend", "Handle", [ClientMessage, Session],
            "RemoveFriendPrefix", null),
        new(Friends, "ACE.Server.Network.GameAction.Actions.GameActionRemoveAllFriends", "Handle", [ClientMessage, Session],
            null, "RemoveAllFriendsPostfix"),
        new(Friends, "ACE.Server.Network.GameEvent.Events.GameEventFriendsListUpdate", "WriteEventBody", [], null,
            "FriendsListPostfix"),
        new(Friends, PlayerManager, "SwitchPlayerFromOfflineToOnline", [Player], null, "LoginPostfix"),
        new(Friends, PlayerManager, "SwitchPlayerFromOnlineToOffline", [Player], null, "LogoutPostfix"),
        new(AppearOffline, "ACE.Server.Network.GameAction.Actions.GameActionSetSingleCharacterOption", "Handle",
            [ClientMessage, Session], "CharacterOptionPrefix", "CharacterOptionPostfix"),
        // Squelching a remote character.
        new(Squelch, "ACE.Server.WorldObjects.Managers.SquelchManager", "HandleActionModifyCharacterSquelch",
            ["System.Boolean", "System.UInt32", "System.String", "ACE.Entity.Enum.ChatMessageType"], "SquelchPrefix", null),
        // The Global room: who is speaking, and the line once the world has let it through.
        new(GlobalRoom, "ACE.Server.Network.Handlers.TurbineChatHandler", "TurbineChatReceived", [ClientMessage, Session],
            "TurbineChatPrefix", "TurbineChatPostfix"),
        new(GlobalRoom, "ACE.Server.Network.Handlers.TurbineChatHandler", "LogTurbineChat",
            ["System.UInt32", "System.String", "System.String", "System.UInt32", "ACE.Entity.Enum.ChatType"], "RoomLinePrefix", null),
        // No local character's name may hold the address separator.
        new(Names, "ACE.Server.Network.Handlers.CharacterHandler", "CharacterCreateEx", [ClientMessage, Session],
            "CharacterCreatePrefix", null),
        new(Names, "ACE.Server.Command.Handlers.AdminCommands", "HandleRename", [Session, "System.String[]"], "RenamePrefix", null),
        // The hub's challenge on the game port, when its operator verifies the world in game: read
        // where every datagram the listener receives is unpacked, before ACE's own parsing.
        new(Verify, "ACE.Server.Network.ClientPacket", "Unpack", ["System.Byte[]", "System.Int32"], "ChallengePrefix", null),
        // The character screen's message, after the character list sent at login.
        new(CharacterScreen, "ACE.Server.Network.Handlers.AuthenticationHandler", "SendConnectResponse",
            [Session, "System.Collections.Generic.List`1[ACE.Database.Models.Shard.Character]"], null, "ConnectResponsePostfix"),
    ];

    public static IEnumerable<string> Features => All.Select(t => t.Feature).Distinct();

    public static IEnumerable<PatchTarget> Of(string feature) => All.Where(t => t.Feature == feature);

    private const BindingFlags Any =
        BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static | BindingFlags.Instance | BindingFlags.DeclaredOnly;

    /// <summary>The method or constructor `t` names in `ace`, or null with the reason.</summary>
    public static MethodBase? Resolve(Assembly ace, PatchTarget t, out string why)
    {
        Type? type;
        try
        {
            type = ace.GetType(t.Type, throwOnError: false);
        }
        catch (Exception e)
        {
            why = $"{t.Type} could not be loaded ({e.GetType().Name})";
            return null;
        }
        if (type is null)
        {
            why = $"no type {t.Type}";
            return null;
        }
        try
        {
            var candidates = t.IsConstructor
                ? type.GetConstructors(Any).Cast<MethodBase>()
                : type.GetMethods(Any).Where(m => m.Name == t.Member);
            var found = candidates.Where(m => Matches(m, t.Parameters)).ToList();
            if (found.Count == 1)
            {
                why = "";
                return found[0];
            }
            why = found.Count == 0 ? $"no {t}" : $"more than one {t}";
            return null;
        }
        catch (Exception e)
        {
            why = $"{t} could not be read ({e.GetType().Name})";
            return null;
        }
    }

    private static bool Matches(MethodBase m, string[] parameters)
    {
        var ps = m.GetParameters();
        if (ps.Length != parameters.Length)
            return false;
        for (var i = 0; i < ps.Length; i++)
            if (NameOf(ps[i].ParameterType) != parameters[i])
                return false;
        return true;
    }

    /// <summary>A type's full name; a constructed generic type's arguments by theirs, without their assemblies.</summary>
    private static string? NameOf(Type t) => t.IsConstructedGenericType
        ? $"{t.GetGenericTypeDefinition().FullName}[{string.Join(",", t.GetGenericArguments().Select(NameOf))}]"
        : t.FullName;
}
