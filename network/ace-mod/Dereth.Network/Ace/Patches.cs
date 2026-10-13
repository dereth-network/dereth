// The patches: each reads what ACE was handed, asks the network's world half whether it is the
// network's, and either answers itself (a prefix returning false) or lets ACE run as it always does.
// Arguments are taken by position, so a fork that renames a parameter still patches. Nothing here
// may throw into ACE: a fault is logged and ACE carries on as if the mod were not there.

using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Server.Network;
using ACE.Server.Network.Enum;
using ACE.Server.Network.GameEvent.Events;
using ACE.Server.Network.GameMessages.Messages;
using ACE.Server.WorldObjects;
using ACE.Server.WorldObjects.Managers;

namespace Dereth.Network;

internal static class Patches
{
    private static Runtime? R => Mod.Current;

    // ---- the pump ------------------------------------------------------------------------------

    /// <summary>After `PlayerManager.Tick`, on the world thread, every loop.</summary>
    public static void TickPostfix()
    {
        var r = R;
        if (r is null)
            return;
        try
        {
            r.StartLinkOnce();
            r.World.Pump(r.Link);
        }
        catch (Exception e)
        {
            r.Fault("the network's world tick", e);
        }
    }

    // ---- the character screen ------------------------------------------------------------------

    /// <summary>
    /// After `AuthenticationHandler.SendConnectResponse`, which sends a login its character list:
    /// the character screen's message, when one is set. It is the world's own, sent whether or not
    /// the network is on.
    /// </summary>
    public static void ConnectResponsePostfix(Session __0)
    {
        if (Mod.ScreenMessage is not { } text || __0 is null)
            return;
        try
        {
            __0.Network.EnqueueSend(new GameMessageCharacterScreenMessage(text));
        }
        catch (Exception e)
        {
            // Not the network's, so never the reason `/dn admin` gives for it being off.
            Mod.Fault("the character screen's message", e, LogLevel.Warn);
        }
    }

    // ---- verification in game -------------------------------------------------------------------

    /// <summary>
    /// Before `ClientPacket.Unpack`, on the listener's receive thread, for every datagram the game
    /// ports receive: the hub's challenge (the world's verification in game) goes to the network's
    /// world half and is not unpacked, as ACE's own size check would drop it anyway. Anything else
    /// is ACE's, untouched.
    /// </summary>
    public static bool ChallengePrefix(byte[] __0, int __1, ref bool __result)
    {
        if (__0 is null || __1 < Challenge.HeaderLength + 2 || __1 > __0.Length || __0[0] != (byte)'D' || __0[1] != (byte)'N')
            return true;
        var r = R;
        if (r is null)
            return true;
        try
        {
            if (!r.World.OnChallengeDatagram(__0.AsSpan(0, __1)))
                return true;
            __result = false;
            return false;
        }
        catch (Exception e)
        {
            r.Fault("the hub's challenge", e);
            return true;
        }
    }

    // ---- tells ---------------------------------------------------------------------------------

    /// <summary>
    /// Before `GameActionTell.Handle` (`Communication_TalkDirectByName`, `0x005D`): a target holding
    /// `@` is a network address, and one given with the mark a line said from Discord shows beside
    /// its speaker (`Ann (Discord)`) names the character without it. The payload is read and put
    /// back, so ACE reads it again when the tell is not the network's.
    /// </summary>
    public static bool TellPrefix(ClientMessage __0, Session __1)
    {
        var r = R;
        if (r is null || __1?.Player is not { } player)
            return true;
        try
        {
            var reader = __0.Payload;
            var start = reader.BaseStream.Position;
            string text, target;
            try
            {
                text = Payload.ReadString16L(reader);
                target = Payload.ReadString16L(reader).Trim();
            }
            finally
            {
                reader.BaseStream.Position = start;
            }
            if (player.IsGagged)
                return true;
            var routed = r.World.TellByName(player.Guid.Full, target, text);
            switch (routed.Kind)
            {
                case RouteKind.Handled:
                    return false;
                case RouteKind.Local:
                    r.Host.LocalTell(player.Guid.Full, routed.LocalName, text);
                    return false;
                default:
                    return true;
            }
        }
        catch (Exception e)
        {
            r.Fault("a tell by name", e);
            return true;
        }
    }

    /// <summary>
    /// Before `GameActionTalkDirect.Handle` (`Communication_TalkDirect`, `0x0032`): a reply by id to
    /// a stand-in id goes back to the remote correspondent it stands for.
    /// </summary>
    public static bool TalkDirectPrefix(ClientMessage __0, Session __1)
    {
        var r = R;
        if (r is null || __1?.Player is not { } player)
            return true;
        try
        {
            var reader = __0.Payload;
            var start = reader.BaseStream.Position;
            string text;
            uint target;
            try
            {
                text = Payload.ReadString16L(reader);
                target = reader.ReadUInt32();
            }
            finally
            {
                reader.BaseStream.Position = start;
            }
            if (!StandIns.IsStandIn(target))
                return true;
            if (player.IsGagged)
            {
                player.SendGagError();
                return false;
            }
            return !r.World.TellById(player.Guid.Full, target, text);
        }
        catch (Exception e)
        {
            r.Fault("a tell by id", e);
            return true;
        }
    }

    // ---- friends -------------------------------------------------------------------------------

    /// <summary>
    /// Before `GameActionAddFriend.Handle` (`Social_AddFriend`, `0x0018`, which carries the name
    /// only): an address is a remote friend, and a name given with the mark a line said from Discord
    /// shows names the character without it.
    /// </summary>
    public static bool AddFriendPrefix(ClientMessage __0, Session __1)
    {
        var r = R;
        if (r is null || __1?.Player is not { } player)
            return true;
        try
        {
            var reader = __0.Payload;
            var start = reader.BaseStream.Position;
            string name;
            try
            {
                name = Payload.ReadString16L(reader).Trim();
            }
            finally
            {
                reader.BaseStream.Position = start;
            }
            return !r.World.AddFriend(player.Guid.Full, name);
        }
        catch (Exception e)
        {
            r.Fault("adding a friend", e);
            return true;
        }
    }

    /// <summary>
    /// Before `GameActionRemoveFriend.Handle` (`Social_RemoveFriend`, `0x0017`, which carries the id):
    /// a stand-in id is a remote friend.
    /// </summary>
    public static bool RemoveFriendPrefix(ClientMessage __0, Session __1)
    {
        var r = R;
        if (r is null || __1?.Player is not { } player)
            return true;
        try
        {
            var reader = __0.Payload;
            var start = reader.BaseStream.Position;
            uint id;
            try
            {
                id = reader.ReadUInt32();
            }
            finally
            {
                reader.BaseStream.Position = start;
            }
            return !StandIns.IsStandIn(id) || !r.World.RemoveFriend(player.Guid.Full, id);
        }
        catch (Exception e)
        {
            r.Fault("removing a friend", e);
            return true;
        }
    }

    /// <summary>After `GameActionRemoveAllFriends.Handle`: the remote friends go too.</summary>
    public static void RemoveAllFriendsPostfix(Session __1)
    {
        var r = R;
        if (r is null || __1?.Player is not { } player)
            return;
        try
        {
            r.World.RemoveAllFriends(player.Guid.Full);
        }
        catch (Exception e)
        {
            r.Fault("clearing the friends list", e);
        }
    }

    /// <summary>
    /// After a friends list is written (`GameEventFriendsListUpdate.WriteEventBody`): a full list
    /// gains the remote friends after the local ones. A single row is left as it is.
    /// </summary>
    public static void FriendsListPostfix(GameEventFriendsListUpdate __instance)
    {
        var r = R;
        if (r is null || __instance?.Data is not { } data)
            return;
        try
        {
            if (!Payload.IsFullFriendsList(data, out var character))
                return;
            var rows = r.World.FriendRows(character);
            if (rows.Count > 0 && !Payload.AppendFriendRows(data, rows))
                r.Fault("the full friends list", new InvalidDataException("the list is not laid out as expected"));
        }
        catch (Exception e)
        {
            r.Fault("the full friends list", e);
        }
    }

    /// <summary>After `PlayerManager.SwitchPlayerFromOfflineToOnline`: a character entered the world.</summary>
    public static void LoginPostfix(Player __0, bool __result)
    {
        var r = R;
        if (r is null || !__result || __0 is null)
            return;
        try
        {
            r.World.OnLogin(__0.Guid.Full);
        }
        catch (Exception e)
        {
            r.Fault("a login", e);
        }
    }

    /// <summary>
    /// After `PlayerManager.SwitchPlayerFromOnlineToOffline`: a character left the world, with its
    /// profile for the directory as it left.
    /// </summary>
    public static void LogoutPostfix(Player __0, bool __result)
    {
        var r = R;
        if (r is null || !__result || __0 is null)
            return;
        CharacterFacts? facts = null;
        try
        {
            facts = AceHost.FactsOf(__0);
        }
        catch (Exception e)
        {
            Mod.Fault("a profile for the directory", e, LogLevel.Warn);
        }
        try
        {
            r.World.OnLogout(__0.Guid.Full, __0.Name, facts);
        }
        catch (Exception e)
        {
            r.Fault("a logout", e);
        }
    }

    /// <summary>
    /// After `PlayerManager.HandlePlayerDelete`, when a character is deleted on the character screen:
    /// a listed one leaves the directory.
    /// </summary>
    public static void PlayerDeletePostfix(uint __0)
    {
        var r = R;
        if (r is null)
            return;
        try
        {
            r.World.OnDeleted(__0);
        }
        catch (Exception e)
        {
            r.Fault("a deletion", e);
        }
    }

    [ThreadStatic]
    private static bool settingAppearOffline;

    /// <summary>
    /// Before `GameActionSetSingleCharacterOption.Handle`: whether the option is the one that makes a
    /// character appear offline.
    /// </summary>
    public static void CharacterOptionPrefix(ClientMessage __0)
    {
        settingAppearOffline = false;
        if (R is null)
            return;
        try
        {
            var reader = __0.Payload;
            var start = reader.BaseStream.Position;
            try
            {
                settingAppearOffline = reader.ReadUInt32() == Payload.AppearOfflineOption;
            }
            finally
            {
                reader.BaseStream.Position = start;
            }
        }
        catch (Exception)
        {
            settingAppearOffline = false;
        }
    }

    /// <summary>After it: the character's remote watchers see what they may see now.</summary>
    public static void CharacterOptionPostfix(Session __1)
    {
        var r = R;
        if (!settingAppearOffline || r is null || __1?.Player is not { } player)
            return;
        settingAppearOffline = false;
        try
        {
            r.World.SettingsChanged(player.Guid.Full);
        }
        catch (Exception e)
        {
            r.Fault("appearing offline", e);
        }
    }

    // ---- squelch -------------------------------------------------------------------------------

    /// <summary>
    /// Before `SquelchManager.HandleActionModifyCharacterSquelch`
    /// (`Communication_ModifyCharacterSquelch`, `0x0058`): a name given with the mark a line said from
    /// Discord shows names the character without it, for ACE too; an address or a stand-in id is a
    /// remote squelch.
    /// </summary>
    public static bool SquelchPrefix(SquelchManager __instance, bool __0, uint __1, ref string __2, ChatMessageType __3)
    {
        var r = R;
        if (r is null || __instance?.Player is not { } player)
            return true;
        if (__2 is not null)
            __2 = NetworkWorld.GivenName(__2);
        if (!StandIns.IsStandIn(__1) && !Address.LooksRemote(__2))
            return true;
        try
        {
            return !r.World.Squelch(player.Guid.Full, __0, __1, __2 ?? "");
        }
        catch (Exception e)
        {
            r.Fault("a squelch", e);
            return true;
        }
    }

    // ---- the Global room -----------------------------------------------------------------------

    [ThreadStatic]
    private static Session? speaking;

    /// <summary>Before `TurbineChatHandler.TurbineChatReceived`: who is speaking.</summary>
    public static void TurbineChatPrefix(Session __1) => speaking = __1;

    /// <summary>After it: nobody is.</summary>
    public static void TurbineChatPostfix() => speaking = null;

    /// <summary>
    /// Before `TurbineChatHandler.LogTurbineChat`, which ACE reaches only for a room line it has let
    /// through to its local listeners: the line goes on to the network if it is the Global room's.
    /// </summary>
    public static void RoomLinePrefix(uint __0, string __1, string __2)
    {
        var r = R;
        var s = speaking;
        if (r is null || s?.Player is not { } player || player.Name != __1)
            return;
        try
        {
            r.World.RoomSaid(player.Guid.Full, __0, __2 ?? "");
        }
        catch (Exception e)
        {
            r.Fault("a room line", e);
        }
    }

    // ---- names ---------------------------------------------------------------------------------

    /// <summary>Before `CharacterHandler.CharacterCreateEx`: a new character's name may not hold `@`.</summary>
    public static bool CharacterCreatePrefix(ClientMessage __0, Session __1)
    {
        var r = R;
        if (r is null)
            return true;
        try
        {
            var reader = __0.Payload;
            var start = reader.BaseStream.Position;
            var info = new CharacterCreateInfo();
            try
            {
                info.Unpack(reader);
            }
            finally
            {
                reader.BaseStream.Position = start;
            }
            if (info.Name is null || !info.Name.Contains(Address.Separator))
                return true;
            __1.Network.EnqueueSend(new GameMessageCharacterCreateResponse(CharacterGenerationVerificationResponse.NameBanned,
                default(ObjectGuid), ""));
            return false;
        }
        catch (Exception e)
        {
            r.Fault("a character's creation", e);
            return true;
        }
    }

    /// <summary>Before `AdminCommands.HandleRename`: a new name may not hold `@`.</summary>
    public static bool RenamePrefix(Session __0, string[] __1)
    {
        var r = R;
        if (r is null || __1 is null)
            return true;
        try
        {
            var joined = string.Join(" ", __1);
            var comma = joined.IndexOf(',');
            if (comma < 0 || !joined[(comma + 1)..].Contains(Address.Separator))
                return true;
            NetCommands.Output(__0, "Error, cannot rename: a character's name may not hold @.");
            return false;
        }
        catch (Exception e)
        {
            r.Fault("a rename", e);
            return true;
        }
    }
}
