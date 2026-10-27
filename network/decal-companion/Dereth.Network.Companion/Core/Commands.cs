using System;

namespace Dereth.Network.Companion
{
    /// <summary>
    /// The <c>@dn</c> command lines the companion types for the player. Each is what the player could
    /// type themselves; the world answers it in chat.
    /// </summary>
    public static class Commands
    {
        public const string Status = "@dn";
        public const string On = "@dn on";
        public const string Off = "@dn off";
        public const string Friends = "@dn friends";
        public const string Channels = "@dn channel list";
        public const string Link = "@dn link";
        public const string Unlink = "@dn unlink";

        /// <summary>The longest line the game sends.</summary>
        public const int MaxLine = 400;

        /// <summary><c>@dn friends add Name@world</c>, or null when <paramref name="address"/> is not an address.</summary>
        public static string? AddFriend(string address) => FriendCommand("add", address);

        /// <summary><c>@dn friends remove Name@world</c>, or null when <paramref name="address"/> is not an address.</summary>
        public static string? RemoveFriend(string address) => FriendCommand("remove", address);

        /// <summary><c>@dn channel members name</c>.</summary>
        public static string? Members(string channel) => ChannelCommand("members", channel);

        public static string? CreateChannel(string channel) => ChannelCommand("create", channel);

        public static string? JoinChannel(string channel) => ChannelCommand("join", channel);

        public static string? LeaveChannel(string channel) => ChannelCommand("leave", channel);

        /// <summary><c>@dn channel invite name Name@world</c>.</summary>
        public static string? Invite(string channel, string address)
        {
            var name = ChannelName(channel);
            if (name == null || !NetAddress.TryParse(address, out var who, out var world) || NetAddress.IsChannel(address))
                return null;
            return Bounded("@dn channel invite " + name + " " + who + "@" + world);
        }

        /// <summary><c>@dng text</c>: a line in the Global room.</summary>
        public static string? SayGlobal(string text)
        {
            var t = OneLine(text);
            return t.Length == 0 ? null : Bounded("@dng " + t);
        }

        /// <summary>A channel's own name: <c>oldguard</c> from <c>oldguard</c> or <c>oldguard@channel</c>; null when it is none.</summary>
        public static string? ChannelName(string channel)
        {
            var name = channel.Trim();
            if (name.EndsWith("@channel", StringComparison.Ordinal))
                name = name.Substring(0, name.Length - "@channel".Length);
            name = name.ToLowerInvariant();
            if (name.Length < 2 || name.Length > 20)
                return null;
            foreach (var c in name)
                if (!(c >= 'a' && c <= 'z') && !(c >= '0' && c <= '9') && c != '-')
                    return null;
            return name;
        }

        private static string? FriendCommand(string verb, string address)
        {
            if (!NetAddress.TryParse(address, out var name, out var world) || NetAddress.IsChannel(address))
                return null;
            return Bounded("@dn friends " + verb + " " + name + "@" + world);
        }

        private static string? ChannelCommand(string verb, string channel)
        {
            var name = ChannelName(channel);
            return name == null ? null : "@dn channel " + verb + " " + name;
        }

        private static string OneLine(string text) => text.Replace('\r', ' ').Replace('\n', ' ').Trim();

        private static string? Bounded(string line) => line.Length <= MaxLine ? line : null;
    }
}
