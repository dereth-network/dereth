using System.Collections.Generic;

namespace Dereth.Decal
{
    /// <summary>The game's window, as the game's keys are sent to it.</summary>
    public interface IKeyWindow
    {
        /// <summary>A key's message posted to the game's window.</summary>
        void Post(int message, int wParam, int lParam);
    }

    /// <summary>
    /// The game's keys, pressed and let go for the host's <c>key</c> ops: each as the key's own
    /// message to the game's window, <c>WM_KEYDOWN</c> or <c>WM_KEYUP</c> with the key's
    /// virtual-key code and, in its flags, the key's place on the keyboard, which is what the game
    /// reads. The game's window hears these as it hears the player's own keys; <see cref="Echo"/>
    /// tells the two apart.
    /// </summary>
    public sealed class Keys
    {
        public const int KeyDown = 0x0100;
        public const int KeyUp = 0x0101;

        /// <summary>The most messages kept to tell from the player's own.</summary>
        private const int Kept = 64;

        /// <summary>
        /// The game's own names for the actions the host presses keys for, which the plugin looks
        /// up in the game's key map.
        /// </summary>
        public static readonly string[] Actions =
        {
            "CombatHighAttack", "CombatMediumAttack", "CombatLowAttack",
            "CombatAimHigh", "CombatAimMedium", "CombatAimLow",
            "MovementForward", "MovementBackup", "MovementStrafeLeft", "MovementStrafeRight",
            "MovementTurnLeft", "MovementTurnRight", "MovementWalkMode", "MovementJump",
        };

        private readonly IKeyWindow window;
        private readonly List<(int message, int wParam, int lParam)> sent = new List<(int, int, int)>();

        public Keys(IKeyWindow window) => this.window = window;

        /// <summary>
        /// A key message's flags: a repeat count of one, the scan code, the extended-key bit, and
        /// for a key let go the bits that say it was down and is now up.
        /// </summary>
        public static int Flags(int scan, bool extended, bool down)
        {
            var f = 1u | ((uint)(scan & 0xFF) << 16);
            if (extended)
                f |= 1u << 24;
            if (!down)
                f |= 0xC000_0000u;
            return unchecked((int)f);
        }

        /// <summary>Press (<paramref name="down"/>) or let go of a key.</summary>
        public void Press(int vk, int scan, bool extended, bool down)
        {
            var message = down ? KeyDown : KeyUp;
            var flags = Flags(scan, extended, down);
            window.Post(message, vk, flags);
            sent.Add((message, vk, flags));
            if (sent.Count > Kept)
                sent.RemoveAt(0);
        }

        /// <summary>
        /// Whether a key message the game's window heard is one sent here: each sent message is
        /// heard once, and anything else is the player's own.
        /// </summary>
        public bool Echo(int message, int wParam, int lParam)
        {
            var i = sent.FindIndex(m => m.message == message && m.wParam == wParam && m.lParam == lParam);
            if (i < 0)
                return false;
            sent.RemoveAt(i);
            return true;
        }
    }
}
